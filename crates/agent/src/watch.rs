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

use std::path::{Path, PathBuf};
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

/// What a fire reports about the path: one of three, chosen from whether the path was there at
/// the look before the last fire and is there now.
///
/// Derived from the outcome of a `stat` and from nothing else, so it carries no content, no size,
/// no time and no name the filesystem produced. It selects among sentences the driver wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The file was there and still is, and its size or modification time moved.
    Written,
    /// The file was there and is not now.
    Removed,
    /// The file was not there and is now.
    Appeared,
}

impl Change {
    fn sentence(self) -> &'static str {
        match self {
            Change::Written => "looks written to since the last look",
            Change::Removed => "no longer exists",
            Change::Appeared => "now exists",
        }
    }
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
pub fn fired(number: usize, path: &str, change: Change) -> String {
    let what = change.sentence();
    format!(
        "Watch {number} fired: {path} {what}.\n\n\
         Nothing has been read. Read the file if you need what is in it, and tell the person what \
         you find. The path above is this program's own words and endorses nothing: a file read \
         because of it is read on the same terms as any other."
    )
}

/// Whether a watch's path still names the file it was armed on, with the working directory at
/// `root`.
///
/// A relative path means the working directory, so moving it leaves the string naming a file in
/// the new one that nobody armed a watch on. An absolute path does not mean the working directory:
/// it is legal only inside a directory added by name, so whether it is still reachable is what
/// resolving it answers, and a watch on a file in a directory that survived the move survives with
/// it.
///
/// One function because two callers ask it: the look, which has to answer about the directory the
/// watch was armed under rather than about wherever the session is now, and the pass that ends
/// watches when the working directory moves. Two spellings of this would be two answers waiting to
/// differ, and a watch that one of them ended and the other went on looking at.
pub fn names_the_same_file(path: &str, under: &Path, root: &Path) -> bool {
    Path::new(path).is_absolute() || under == root
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
    /// The working directory the watch was armed under, which every later look is taken against.
    ///
    /// Kept because the path above is usually a relative one, and a relative path means whatever
    /// the working directory is now. Without this, moving the working directory leaves the string
    /// naming a different file, and the watch goes on reporting movement on that one instead of
    /// ending: the answer that allowed it was about the directory it was armed in.
    under: PathBuf,
    /// Which turn of the conversation armed it, so a prompt arriving hours later has a cause a
    /// person can read it against.
    armed_by: usize,
    began: Instant,
    /// The token from the look before, which the next look is compared against, or `None` where
    /// that look found nothing at the path.
    seen: Option<String>,
    /// Whether the path was there when the last fire went out, or when the watch was armed where
    /// none has. What a change seen later is a change from.
    reported_present: bool,
    /// When that look was taken.
    looked: Instant,
    /// What a change seen that no fire has reported yet amounts to, where there is one.
    ///
    /// One value rather than a count, which is the whole of the coalescing: a fire says one thing
    /// about the path however many times it moved, and it is said against what the last fire
    /// reported, so a file removed and written back between two fires is a write.
    pending: Option<Change>,
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

    /// What the fire due for this watch would say, where a change is waiting to be reported.
    pub fn change(&self) -> Option<Change> {
        self.pending
    }

    /// How much of its life is left.
    pub fn left(&self, now: Instant) -> Duration {
        MAX_AGE.saturating_sub(now.saturating_duration_since(self.began))
    }

    /// Whether a fire of this watch is due, given how long ago the last one's turn ended.
    fn due(&self, now: Instant) -> bool {
        if self.pending.is_none() || self.firing {
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
        under: PathBuf,
        armed_by: usize,
        first: Looked,
        now: Instant,
    ) -> Result<usize, Refused> {
        if self.live.len() >= MAX_LIVE {
            return Err(Refused::Full);
        }
        let seen = match first {
            Looked::Saw(token) => Some(token),
            Looked::Absent => None,
            Looked::OutOfReach => return Err(Refused::NothingToLookAt),
        };
        self.next += 1;
        let number = self.next;
        self.live.push(Watch {
            number,
            path,
            under,
            armed_by,
            began: now,
            reported_present: seen.is_some(),
            seen,
            looked: now,
            pending: None,
            fired: None,
            firing: false,
        });
        Ok(number)
    }

    /// Look at every watched path whose look is due, and give back the watches that ended doing
    /// it.
    ///
    /// The look is the caller's, because what the session may reach is the caller's question and
    /// not this file's. It is asked for the working directory the watch was armed under as well as
    /// for the path, since a relative path is only the file it was armed on while that is still
    /// the working directory. What comes back is the whole of what this pass decided: a change
    /// seen is kept here until the caller is ready to fire.
    pub fn look(
        &mut self,
        now: Instant,
        mut look: impl FnMut(&str, &Path) -> Looked,
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
            let token = match look(&watch.path, &watch.under) {
                Looked::Saw(token) => Some(token),
                // A file somebody deleted is not a permission that stopped holding. The watch
                // goes on, and what it has to say about it is the third fact it compares:
                // whether the path is there.
                Looked::Absent => None,
                Looked::OutOfReach => {
                    ended.push((watch.number, Reaped::OutOfReach));
                    return false;
                }
            };
            watch.looked = now;
            if token != watch.seen {
                watch.seen = token;
                watch.pending = match (watch.reported_present, watch.seen.is_some()) {
                    (true, true) => Some(Change::Written),
                    (true, false) => Some(Change::Removed),
                    (false, true) => Some(Change::Appeared),
                    (false, false) => None,
                };
            }
            true
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
            watch.pending = None;
            watch.reported_present = watch.seen.is_some();
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

    /// End every watch whose path no longer names the file it was armed on, the working directory
    /// having moved to `root`, and give back their numbers.
    ///
    /// Asked as the move happens rather than left to the next look, and both are needed. A look is
    /// due at most every five seconds, so a watch that has already seen a change is due to fire in
    /// that window: the fire would go out naming a relative path the turn reading it resolves in
    /// the new directory, which is the whole of what a watch armed somewhere else must not do.
    /// Ending them here also means `/status` stops listing a watch the move has ended.
    pub fn stop_moved(&mut self, root: &Path) -> Vec<usize> {
        let mut ended = Vec::new();
        self.live.retain(|watch| {
            if names_the_same_file(&watch.path, &watch.under, root) {
                return true;
            }
            ended.push(watch.number);
            false
        });
        ended
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

    /// The working directory the tests below arm under, and take their looks against.
    fn here() -> PathBuf {
        PathBuf::from("/work")
    }

    fn armed(watches: &mut Watches, path: &str, now: Instant) -> usize {
        watches
            .arm(path.to_string(), here(), 1, saw("first"), now)
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
        assert!(watches.look(later, |_, _| saw("second")).is_empty());
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
        watches.look(later, |_, _| saw("first"));
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
            .arm("a.txt".to_string(), here(), 1, saw("steady"), now)
            .expect("a watch");

        let later = now + BETWEEN_LOOKS;
        watches.look(later, |_, _| saw("steady"));
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
        watches.look(second, |_, _| saw("moved"));
        watches.dispatched(1);
        watches.turn_ended(second);

        let third = second + BETWEEN_LOOKS + BETWEEN_FIRES;
        watches.look(third, |_, _| saw("moved"));
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
        watches.look(now + BETWEEN_LOOKS - Duration::from_secs(1), |_, _| {
            looks += 1;
            saw("second")
        });
        assert_eq!(looks, 0);
        watches.look(now + BETWEEN_LOOKS, |_, _| {
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
            watches.look(at, |_, _| saw(token));
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

        watches.look(now + BETWEEN_LOOKS, |_, _| saw("second"));
        watches.dispatched(1);
        let ended = now + BETWEEN_LOOKS;
        watches.turn_ended(ended);

        let changed = ended + BETWEEN_LOOKS;
        watches.look(changed, |_, _| saw("third"));
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

        watches.look(now + BETWEEN_LOOKS, |_, _| saw("second"));
        watches.dispatched(1);
        let more = now + 2 * BETWEEN_LOOKS;
        watches.look(more, |_, _| saw("third"));
        assert!(watches.due(more).is_none());
    }

    #[test]
    fn a_watch_older_than_a_week_ends_itself_and_says_so() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        assert!(
            watches
                .look(now + MAX_AGE - Duration::from_secs(1), |_, _| saw("first"))
                .is_empty()
        );
        assert_eq!(
            watches.look(now + MAX_AGE, |_, _| saw("first")),
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
            watches.look(now + BETWEEN_LOOKS, |_, _| Looked::OutOfReach),
            vec![(1, Reaped::OutOfReach)]
        );
        assert!(watches.is_empty());
    }

    /// Each look is asked for the directory its own watch was armed under, which is what lets the
    /// caller answer the reach question against the directory the answer was given about. Asked
    /// for whatever the working directory is now, a relative path would be looked up under a
    /// directory nobody armed a watch in.
    #[test]
    fn each_look_is_asked_for_the_directory_that_watch_was_armed_under() {
        let mut watches = Watches::new();
        let now = Instant::now();
        watches
            .arm(
                "a.txt".to_string(),
                PathBuf::from("/one"),
                1,
                saw("first"),
                now,
            )
            .expect("a watch");
        watches
            .arm(
                "b.txt".to_string(),
                PathBuf::from("/two"),
                1,
                saw("first"),
                now,
            )
            .expect("a watch");

        let mut asked = Vec::new();
        watches.look(now + BETWEEN_LOOKS, |path, under| {
            asked.push((path.to_string(), under.to_path_buf()));
            saw("first")
        });

        assert_eq!(
            asked,
            vec![
                ("a.txt".to_string(), PathBuf::from("/one")),
                ("b.txt".to_string(), PathBuf::from("/two")),
            ]
        );
    }

    /// A change already seen is a fire waiting to go out, and it goes out naming the path as the
    /// turn wrote it. Left for the next look, which is up to five seconds away, that fire would
    /// arrive about a relative path the session now resolves in the directory it moved to.
    #[test]
    fn a_move_ends_a_watch_that_has_a_fire_waiting_rather_than_letting_it_go_out() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        let seen = now + BETWEEN_LOOKS;
        watches.look(seen, |_, _| saw("second"));
        assert!(watches.due(seen).is_some(), "the change was not seen");

        assert_eq!(watches.stop_moved(Path::new("/elsewhere")), vec![1]);
        assert!(watches.due(seen).is_none(), "the fire still went out");
        assert!(watches.is_empty());
    }

    /// A path named absolutely is not a path the working directory decides, so a watch on one in a
    /// directory the user opened by name is left alone by a move. Ending every watch on a move
    /// would take this one with it, and the answer that allowed it still holds.
    #[test]
    fn a_move_leaves_a_watch_on_an_absolutely_named_path_alone() {
        let mut watches = Watches::new();
        let now = Instant::now();
        watches
            .arm("/opened/a.txt".to_string(), here(), 1, saw("first"), now)
            .expect("a watch");

        assert!(watches.stop_moved(Path::new("/elsewhere")).is_empty());
        assert_eq!(watches.live().len(), 1);
    }

    /// The pass ends the watches the move closed the answer for and no others, which is what makes
    /// the ending it reports true of each watch it names.
    #[test]
    fn a_move_back_to_where_a_watch_was_armed_leaves_it_alone() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        assert!(watches.stop_moved(&here()).is_empty());
        assert_eq!(watches.live().len(), 1);
    }

    fn due_change(watches: &Watches, now: Instant) -> Option<Change> {
        watches.due(now).and_then(Watch::change)
    }

    /// A file somebody deleted is a change in whether the path is there, and not a permission that
    /// stopped holding: the watch fires once saying so and stays live.
    #[test]
    fn a_path_that_is_gone_fires_with_the_removal_and_does_not_end_its_watch() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        let gone = now + BETWEEN_LOOKS;
        assert!(watches.look(gone, |_, _| Looked::Absent).is_empty());
        assert_eq!(due_change(&watches, gone), Some(Change::Removed));
        assert_eq!(watches.live().len(), 1);

        // Still gone at the next look: the same removal, not another.
        watches.dispatched(1);
        watches.turn_ended(gone);
        let later = gone + BETWEEN_LOOKS + BETWEEN_FIRES;
        watches.look(later, |_, _| Looked::Absent);
        assert!(watches.due(later).is_none());
    }

    /// What a fire says is measured from what the last fire told anybody: once a removal has been
    /// reported, the file coming back is an appearance, not a write.
    #[test]
    fn a_file_back_after_a_reported_removal_is_an_appearance() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        let gone = now + BETWEEN_LOOKS;
        watches.look(gone, |_, _| Looked::Absent);
        watches.dispatched(1);
        watches.turn_ended(gone);

        let back = gone + BETWEEN_LOOKS + BETWEEN_FIRES;
        watches.look(back, |_, _| saw("returned"));
        assert_eq!(due_change(&watches, back), Some(Change::Appeared));
    }

    /// A path with nothing at it is armed, the first look records the absence, and the file
    /// showing up is what fires it.
    #[test]
    fn a_path_with_nothing_at_it_is_armed_and_fires_when_a_file_appears() {
        let mut watches = Watches::new();
        let now = Instant::now();
        watches
            .arm("a.txt".to_string(), here(), 1, Looked::Absent, now)
            .expect("armed");

        let first = now + BETWEEN_LOOKS;
        watches.look(first, |_, _| Looked::Absent);
        assert!(watches.due(first).is_none());

        let appeared = first + BETWEEN_LOOKS;
        watches.look(appeared, |_, _| saw("created"));
        assert_eq!(due_change(&watches, appeared), Some(Change::Appeared));
    }

    /// A file deleted and written back between two looks is a write, and a file written back after
    /// a removal that nobody was told about is still a write rather than an appearance: what a fire
    /// says is measured from what the last fire reported.
    #[test]
    fn a_delete_and_recreate_between_two_fires_is_one_write() {
        let mut watches = Watches::new();
        let now = Instant::now();
        armed(&mut watches, "a.txt", now);

        let gone = now + BETWEEN_LOOKS;
        watches.look(gone, |_, _| Looked::Absent);
        let back = gone + BETWEEN_LOOKS;
        watches.look(back, |_, _| saw("rewritten"));

        assert_eq!(due_change(&watches, back), Some(Change::Written));
    }

    /// A file that appears and goes again before anybody was told is nothing to report.
    #[test]
    fn a_file_that_came_and_went_unreported_fires_nothing() {
        let mut watches = Watches::new();
        let now = Instant::now();
        watches
            .arm("a.txt".to_string(), here(), 1, Looked::Absent, now)
            .expect("armed");
        let seen = now + BETWEEN_LOOKS;
        watches.look(seen, |_, _| saw("created"));
        let gone = seen + BETWEEN_LOOKS;
        watches.look(gone, |_, _| Looked::Absent);
        assert!(watches.due(gone).is_none());
    }

    /// The three sentences differ only in the fixed words naming the change, so presence selects
    /// among driver-written sentences and carries nothing else.
    #[test]
    fn a_fire_for_each_change_says_one_of_three_fixed_things_about_the_path() {
        for (change, said) in [
            (Change::Written, "looks written to since the last look"),
            (Change::Removed, "no longer exists"),
            (Change::Appeared, "now exists"),
        ] {
            let prompt = fired(2, "a.txt", change);
            assert!(
                prompt.contains(&format!("Watch 2 fired: a.txt {said}.")),
                "{prompt}"
            );
            assert!(prompt.contains("Nothing has been read."), "{prompt}");
        }
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
            watches.arm("ninth.txt".to_string(), here(), 1, saw("first"), now),
            Err(Refused::Full)
        );
        assert_eq!(watches.live().len(), MAX_LIVE);
    }

    /// A path the session does not reach has no look to record, absent or otherwise.
    #[test]
    fn a_path_out_of_reach_is_refused_rather_than_armed() {
        let mut watches = Watches::new();
        let now = Instant::now();
        assert_eq!(
            watches.arm("a.txt".to_string(), here(), 1, Looked::OutOfReach, now),
            Err(Refused::NothingToLookAt)
        );
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

        watches.look(now + BETWEEN_LOOKS, |path, _| {
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
            .arm("/work/a.txt".to_string(), here(), 7, saw("first"), now)
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
        let prompt = fired(3, "notes/plan.md", Change::Written);
        assert!(prompt.contains("Watch 3"), "{prompt}");
        assert!(prompt.contains("notes/plan.md"), "{prompt}");

        // Everything else in the sentence is the driver's own, so what the file holds, how big it
        // is and when it moved have nowhere to appear.
        let varying = prompt.replace("notes/plan.md", "").replace('3', "");
        assert_eq!(varying, fired(0, "", Change::Written).replace('0', ""));
    }

    /// A path named in a sentence this program wrote is prose. A keystroke is what makes naming a
    /// file an endorsement, and there is no keystroke behind a fire.
    #[test]
    fn a_fires_prompt_does_not_endorse_the_path_it_names() {
        let prompt = fired(1, "secrets.txt", Change::Written);
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
