//! Standing watches: a path a turn armed, looked at from then on with no turn running to notice.
//!
//! A watch observes and never acts. It starts no process, writes nothing, sends nothing, and reads
//! no byte of the file it is on: the whole of what it holds is the change token a read hands back,
//! and the whole of what it does is compare this look's token with the one before it. A difference
//! arms a fire, and a fire is a turn beginning with a sentence naming which watch fired and on
//! what path.
//!
//! Nothing here is written to disk. A watch lives as long as the session that armed it, and the
//! process ending is what reaps it.
//!
//! The bounds are all in this file, and each of them is the answer to "what stops this from being
//! an effect nobody is watching": an age, a count, a floor between two fires, and an interval
//! between two looks.

pub use bravebot_agent::watch::Looked;
use std::time::{Duration, Instant};

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

/// The word that ends a watch, rather than naming one.
const STOP: &str = "stop";

/// What the argument to `/watch` asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// The bare word: list the live watches, or say there are none.
    List,
    /// End the watch with this number.
    Stop(usize),
    /// Anything else, answered by saying what the command takes.
    Unreadable,
}

/// Read the argument to `/watch`.
///
/// Two forms and nothing else. A watch is armed by asking for one in a prompt, so there is no
/// form here that arms one: what a person needs from a command is to see what is live and to end
/// one of them, which are the two things a transcript cannot tell them.
pub fn parse(argument: &str) -> Asked {
    let argument = argument.trim();
    if argument.is_empty() {
        return Asked::List;
    }
    match argument.split_once(char::is_whitespace) {
        Some((STOP, number)) => match number.trim().parse::<usize>() {
            Ok(number) => Asked::Stop(number),
            Err(_) => Asked::Unreadable,
        },
        _ => Asked::Unreadable,
    }
}

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

    #[test]
    fn the_bare_word_lists_what_is_live() {
        assert_eq!(parse(""), Asked::List);
        assert_eq!(parse("   "), Asked::List);
    }

    #[test]
    fn stop_and_a_number_ends_that_watch() {
        assert_eq!(parse("stop 3"), Asked::Stop(3));
        assert_eq!(parse("stop   12  "), Asked::Stop(12));
    }

    /// Answered by saying what the command takes rather than by guessing which watch was meant:
    /// ending the wrong one is the mistake a guess makes here.
    #[test]
    fn anything_else_is_answered_by_saying_what_the_command_takes() {
        for argument in ["stop", "stop all", "3", "stop three", "list", "src/main.rs"] {
            assert_eq!(parse(argument), Asked::Unreadable, "{argument}");
        }
    }
}
