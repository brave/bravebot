//! Repeating a prompt the person typed.
//!
//! `/loop` holds one line and sends it again on a schedule. The line is settled the moment it is
//! typed and never changes afterwards: a tick is the same prompt as the first one, so nothing a
//! turn reads, writes or says can decide what the next turn asks for. A planner may say *when* the
//! next tick happens, within an hour, and that is the whole of what it may say.
//!
//! Two ways the next tick's moment is decided, and the argument picks between them. An interval
//! the person wrote is kept by the driver, and the planner is not consulted. No interval, and each
//! tick's turn says how long to wait; a turn that says nothing gets one fallback wake and then the
//! loop ends, because a loop nobody is re-arming is a loop nobody is running.
//!
//! Nothing here is written to disk. A loop lives as long as the session that started it.

pub use bravebot_agent::turn::{Tick, Wakeup};
use bravebot_i18n::t;
use std::time::{Duration, Instant};

/// The shortest interval a person may set.
///
/// Low, because this is their own number: somebody who types `10s` wants ten seconds, and the
/// gap is measured from the end of a tick, so a slow turn spaces itself out whatever this says.
/// It is not zero because a loop with no gap at all is a way to spend a rate limit rather than a
/// way to watch something, and because the interface has to stay usable between ticks.
///
/// A wait a *turn* asks for is bounded by the tool that takes it, lower than this at the bottom and
/// far more tightly at the top: that number is the planner's rather than the person's, and a turn's
/// own length already keeps the interface usable between ticks.
const FLOOR: Duration = Duration::from_secs(5);

/// How long a loop may run before it ends itself.
const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// What the driver waits when a self-paced turn ended without saying when to wake.
const KEEPALIVE: Duration = Duration::from_secs(1_200);

/// How many times that fallback is used before the loop ends.
///
/// One. The first time is a turn that forgot; the second is a turn that is not going to remember,
/// and waking it every twenty minutes forever is not what anybody asked for.
const KEEPALIVE_BUDGET: u8 = 1;

/// The unit letters an interval may be written with, and what each is worth in seconds.
///
/// Longest first, so `days` is matched before `d` and a suffix search cannot stop early.
const UNITS: [(&str, u64); 18] = [
    ("seconds", 1),
    ("second", 1),
    ("secs", 1),
    ("sec", 1),
    ("minutes", 60),
    ("minute", 60),
    ("mins", 60),
    ("min", 60),
    ("hours", 3_600),
    ("hour", 3_600),
    ("hrs", 3_600),
    ("hr", 3_600),
    ("days", 86_400),
    ("day", 86_400),
    ("s", 1),
    ("m", 60),
    ("h", 3_600),
    ("d", 86_400),
];

/// The unit letters the token at the front of the argument may use.
///
/// Narrower than [`UNITS`] on purpose: this token is recognised before anything else is read, so
/// `minutes of silence` must stay a prompt rather than becoming an interval and a sentence.
const SHORT_UNITS: [(&str, u64); 4] = [("s", 1), ("m", 60), ("h", 3_600), ("d", 86_400)];

/// The word that introduces an interval written at the end of a sentence.
const EVERY: &str = "every";

/// The word that ends the loop that is running, rather than starting one over it.
const STOP: &str = "stop";

/// How the moment of the next tick is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pacing {
    /// The person gave an interval, and the driver keeps to it without asking anybody.
    Every(Duration),
    /// Nobody gave one, so each tick's turn says when the next is due.
    SelfPaced,
}

/// What `/loop` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub pacing: Pacing,
    /// The line to send on every tick, exactly as it was typed.
    pub prompt: String,
    /// What the interval became, where the one asked for was outside the bounds.
    ///
    /// Said out loud rather than applied quietly: somebody who typed `10s` and is told nothing
    /// believes they are watching something ten times more closely than they are.
    pub adjusted: Option<Held>,
}

/// An interval that was not the one asked for, and which way it moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// Faster than a loop goes.
    Raised(Duration),
    /// Longer than a loop lives.
    Capped(Duration),
}

/// What the argument to `/loop` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The bare word: say what is repeating, or that nothing is.
    Report,
    /// End the loop that is running.
    Stop,
    /// Start a loop over this line.
    Start(Request),
    /// An interval with nothing to send, answered by saying what the command needs.
    Unreadable,
}

impl Asked {
    /// The request where the argument started a loop, and nothing where it asked for anything else.
    pub fn started(self) -> Option<Request> {
        match self {
            Asked::Start(request) => Some(request),
            _ => None,
        }
    }
}

/// Read the argument to `/loop`.
///
/// Three forms and a refusal. The bare word says what is repeating, because what is going to
/// happen without anybody typing is the one thing about a session that its transcript does not
/// hold. `stop` ends the loop, which is the same ending Ctrl-C reaches and does not also stop the
/// turn in flight. Anything else is a line to repeat, read for an interval as in
/// [`leading_interval`] and [`trailing_interval`].
///
/// [`Asked::Unreadable`] is an interval with nothing after it, which is a loop that cannot be
/// started. It stays apart from the bare word so that `/loop 5m` says what the command needs even
/// while some other loop is running and could have been reported instead.
pub fn parse(argument: &str) -> Asked {
    let argument = argument.trim();
    if argument.is_empty() {
        return Asked::Report;
    }

    // Only as the whole of what would be sent. `stop the deploy` is a line somebody wants sent
    // again, and a reserved word that ate its first token would send them the rest of their
    // sentence forever. An interval beside it leaves the word the whole line even so, which is why
    // the reading happens again in [`interval`]: `stop every 5m` is somebody ending the loop that
    // runs every five minutes, and a loop sending the bare word `stop` to a model is nothing
    // anybody means by anything.
    if is_stop(argument) {
        return Asked::Stop;
    }

    // The front of the line first. A token that is nothing but a number and a unit letter was
    // written to be an interval, and reading it as one is what makes `/loop 5m /babysit` work.
    if let Some((asked, prompt)) = leading_interval(argument) {
        return interval(asked, prompt);
    }

    // Then the end of it, which is where an interval goes when the line is a sentence.
    if let Some((asked, prompt)) = trailing_interval(argument) {
        return interval(asked, prompt);
    }

    Asked::Start(Request {
        pacing: Pacing::SelfPaced,
        prompt: argument.to_string(),
        adjusted: None,
    })
}

/// The reserved word, however somebody's terminal capitalised it.
///
/// Case-insensitive because the miss is not a message: a line that is the word and is not read as
/// one becomes a loop sending `Stop` to a model until somebody presses Ctrl-C twice.
fn is_stop(line: &str) -> bool {
    line.eq_ignore_ascii_case(STOP)
}

/// A request from an interval and what is left of the line, where anything is left.
fn interval(asked: Duration, prompt: &str) -> Asked {
    if prompt.is_empty() {
        return Asked::Unreadable;
    }
    if is_stop(prompt) {
        return Asked::Stop;
    }
    let held = asked.clamp(FLOOR, MAX_AGE);
    let adjusted = match held {
        _ if held == asked => None,
        _ if held == FLOOR => Some(Held::Raised(held)),
        _ => Some(Held::Capped(held)),
    };
    Asked::Start(Request {
        pacing: Pacing::Every(held),
        prompt: prompt.to_string(),
        adjusted,
    })
}

/// The request in an argument that starts a loop, for the tests of what a loop then does.
///
/// Those tests want a running loop rather than the reading of an argument, and going through
/// [`parse`] for it keeps them from disagreeing with it.
#[cfg(test)]
pub(crate) fn request(argument: &str) -> Request {
    parse(argument)
        .started()
        .unwrap_or_else(|| panic!("{argument:?} started no loop"))
}

/// An interval written as the first word, and the rest of the line.
fn leading_interval(argument: &str) -> Option<(Duration, &str)> {
    let (token, rest) = match argument.split_once(char::is_whitespace) {
        Some((token, rest)) => (token, rest.trim()),
        None => (argument, ""),
    };
    let seconds = measure(token, &SHORT_UNITS)?;
    Some((seconds, rest))
}

/// An interval written as an `every ...` clause at the end, and the line without it.
fn trailing_interval(argument: &str) -> Option<(Duration, &str)> {
    // Byte offsets are shared between the two because lowercasing only ASCII leaves every other
    // byte where it was, so an index found in one is an index into the other.
    let lowered = argument.to_ascii_lowercase();
    let mut at = None;
    let mut from = 0;
    while let Some(found) = lowered[from..].find(EVERY) {
        let found = from + found;
        let opens = found == 0 || lowered[..found].ends_with(char::is_whitespace);
        let after = found + EVERY.len();
        let closes = after == lowered.len() || lowered[after..].starts_with(char::is_whitespace);
        if opens && closes {
            at = Some(found);
        }
        from = after;
    }
    let at = at?;

    let mut words = lowered[at..].split_whitespace();
    // The word itself, which the search already established is there.
    words.next()?;
    let seconds = match (words.next(), words.next(), words.next()) {
        // `every 20m`
        (Some(one), None, None) => measure(one, &UNITS)?,
        // `every 20 minutes`
        (Some(count), Some(unit), None) => {
            let worth = UNITS.iter().find(|(name, _)| *name == unit)?.1;
            multiply(count, worth)?
        }
        // Anything else is words. `every PR` and `every so often` are not intervals, and a
        // sentence that ends in one of them is a prompt with nothing taken out of it.
        _ => return None,
    };

    Some((seconds, argument[..at].trim()))
}

/// A count and a unit fused into one token, as a duration.
fn measure(token: &str, units: &[(&str, u64)]) -> Option<Duration> {
    for (name, worth) in units {
        if let Some(count) = token.strip_suffix(name) {
            return multiply(count, *worth);
        }
    }
    None
}

/// A run of digits multiplied by what its unit is worth.
///
/// `None` for anything that is not a number, and for a number too large to be a duration: a
/// count that overflows is a word that happened to be made of digits, not an interval.
fn multiply(count: &str, worth: u64) -> Option<Duration> {
    if count.is_empty() || !count.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    count
        .parse::<u64>()
        .ok()?
        .checked_mul(worth)
        .map(Duration::from_secs)
}

/// A loop that is running.
#[derive(Debug, Clone)]
pub struct Running {
    /// The line every tick sends. Settled when it was typed and never written to again.
    prompt: String,
    /// The pictures pasted into the line, which the first tick takes and no later one has.
    ///
    /// Emptied by that tick, so the rule holds because there is nothing left to carry rather than
    /// because something counted the ticks.
    pasted: Vec<crate::state::AttachedImage>,
    /// The files dropped onto it, which that same tick takes and no later one has, for that reason.
    attached: Vec<crate::state::Attached>,
    /// What a later tick sends instead, with those pictures put back to words and those files put
    /// back to their names.
    ///
    /// `None` where the line named neither, which is every loop but one somebody dropped on or
    /// pasted into.
    settled: Option<String>,
    pacing: Pacing,
    began: Instant,
    /// When the next tick is due, or `None` while a tick is in flight or a self-paced turn has
    /// yet to say.
    due: Option<Instant>,
    /// Whether the turn for a tick is running now, so its end is the moment to arm the next.
    running: bool,
    /// Fallback wakes left before an unarmed self-paced loop ends.
    keepalive: u8,
    /// Ticks dispatched so far, the first one included.
    ticks: usize,
    /// Ticks that reported nothing to do, since the last one that reported something.
    quiet: usize,
}

impl Running {
    /// Start a loop, with nothing due: the caller dispatches the first tick straight away.
    pub fn begin(request: Request) -> Self {
        Self {
            prompt: request.prompt,
            pasted: Vec::new(),
            attached: Vec::new(),
            settled: None,
            pacing: request.pacing,
            began: Instant::now(),
            due: None,
            running: false,
            keepalive: KEEPALIVE_BUDGET,
            ticks: 0,
            quiet: 0,
        }
    }

    /// Carry what the line named, for the first tick and no other.
    ///
    /// `settled` is the same line with each marker put back to what stands for it durably, and is
    /// what every tick after the first sends. What was named belongs to the line the person pressed
    /// Enter on, so the first tick is a turn like any other prompt with a paste or a drop in it; a
    /// later tick has nothing to carry, and a marker left in one would name a screenshot nothing came
    /// with or a file nothing read.
    ///
    /// Only called where something was named, which is why nothing here asks: see
    /// [`crate::state::Session::start_loop`], where the same question decides what is said about it.
    pub fn carrying(
        mut self,
        pasted: Vec<crate::state::AttachedImage>,
        attached: Vec<crate::state::Attached>,
        settled: String,
    ) -> Self {
        self.pasted = pasted;
        self.attached = attached;
        self.settled = Some(settled);
        self
    }

    /// Start a loop a turn asked for, with the next look already armed.
    ///
    /// The difference from [`Running::begin`] is what happens first. Somebody who has just typed
    /// `/loop` has not seen the work done once, so the first tick goes immediately. A turn that
    /// asks for a later look has just taken one and is reporting it, so an immediate tick would
    /// repeat the look already in the answer. Here the first tick is the wait away.
    ///
    /// Always self-paced. An interval is a number a person gives, and there is nowhere for a turn
    /// to say one: what it gives is the wait until the next look, which it is asked for again then.
    pub fn armed(prompt: String, wakeup: Wakeup, now: Instant) -> Self {
        Self {
            prompt,
            // Nothing pasted, nothing dropped, and nothing to settle: what a turn asks to look at
            // again is a line out of the conversation, and no box was open for anybody to paste into
            // or drop on.
            pasted: Vec::new(),
            attached: Vec::new(),
            settled: None,
            pacing: Pacing::SelfPaced,
            began: now,
            due: now.checked_add(wakeup.after),
            running: false,
            keepalive: KEEPALIVE_BUDGET,
            ticks: 0,
            quiet: usize::from(wakeup.quiet),
        }
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn pacing(&self) -> Pacing {
        self.pacing
    }

    /// Whether the planner is asked when the next tick is due.
    pub fn self_paced(&self) -> bool {
        self.pacing == Pacing::SelfPaced
    }

    /// Whether the turn running now is a tick of this loop.
    ///
    /// False for a prompt the person typed in the middle of one. That turn is theirs, it is not
    /// asked when the next tick is due, and its end does not reset the clock.
    pub fn ticking(&self) -> bool {
        self.running
    }

    /// What the turn running now needs to know about being a tick, where it is one.
    pub fn tick(&self) -> Option<Tick> {
        self.running.then_some(Tick {
            number: self.ticks,
            self_paced: self.self_paced(),
        })
    }

    pub fn ticks(&self) -> usize {
        self.ticks
    }

    pub fn quiet(&self) -> usize {
        self.quiet
    }

    /// How long until the next tick, or `None` where none is armed.
    pub fn until(&self, now: Instant) -> Option<Duration> {
        self.due.map(|due| due.saturating_duration_since(now))
    }

    /// How often this loop runs, in the words the status panel and the report both say it in.
    pub fn pace(&self) -> String {
        match self.pacing {
            Pacing::Every(every) => t!(status_loop_every, every = spell(every)),
            Pacing::SelfPaced => t!(status_loop_self_paced).to_string(),
        }
    }

    /// When the next tick is, in those same words.
    ///
    /// Beside [`Self::pace`] because the two are read together and a session may hold either half
    /// without the other making sense: a loop with a tick in flight has a pace and no moment, and
    /// one waiting on a planner has neither.
    pub fn when(&self, now: Instant) -> String {
        match self.until(now) {
            Some(until) => t!(status_loop_next, next = spell(until)),
            None if self.ticking() => t!(status_loop_running).to_string(),
            None => t!(status_loop_unpaced).to_string(),
        }
    }

    /// Whether the loop has run longer than a loop may run.
    pub fn aged_out(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.began) >= MAX_AGE
    }

    /// Whether a tick should be dispatched now.
    ///
    /// A tick in flight is not due again, and a loop with nothing armed is waiting rather than
    /// ready. Whether the *session* is free to take one is the caller's question, not this one's.
    pub fn due(&self, now: Instant) -> bool {
        !self.running && self.due.is_some_and(|due| due <= now)
    }

    /// Record that a tick is going out, and give up what it sends.
    ///
    /// The first tick after a paste or a drop sends the line as it was typed, markers and contents
    /// together, which is what any prompt with a paste or a drop in it sends. Every tick after it
    /// sends the settled line and carries nothing, because what was staged left with the tick that
    /// took it.
    ///
    /// Read off `settled` rather than off a tick count, so the two cannot disagree: what decides is
    /// whether anything is left to carry.
    pub fn dispatching(
        &mut self,
    ) -> (
        String,
        Vec<crate::state::AttachedImage>,
        Vec<crate::state::Attached>,
    ) {
        self.ticks += 1;
        self.running = true;
        self.due = None;
        let pasted = std::mem::take(&mut self.pasted);
        let attached = std::mem::take(&mut self.attached);
        match pasted.is_empty() && attached.is_empty() {
            true => (
                self.settled.clone().unwrap_or_else(|| self.prompt.clone()),
                Vec::new(),
                Vec::new(),
            ),
            false => (self.prompt.clone(), pasted, attached),
        }
    }

    /// Arm the next tick from the turn that has just ended, and say whether the loop goes on.
    ///
    /// `wakeup` is what a self-paced turn asked for, and is ignored where the person gave an
    /// interval: a loop the driver paces is not the planner's to re-time.
    ///
    /// The gap is measured from here rather than from when the tick was sent, so an interval is
    /// the time between runs. Measured from the send, a turn that outlasts its own interval would
    /// be due again the instant it finished, and a slow loop would become a continuous one.
    pub fn ended(&mut self, wakeup: Option<Wakeup>, now: Instant) -> bool {
        if !self.running {
            return true;
        }
        self.running = false;

        if let Pacing::Every(every) = self.pacing {
            self.due = now.checked_add(every);
            return true;
        }

        match wakeup {
            Some(wakeup) => {
                self.quiet = if wakeup.quiet { self.quiet + 1 } else { 0 };
                self.due = now.checked_add(wakeup.after);
                self.keepalive = KEEPALIVE_BUDGET;
                true
            }
            // A turn that said nothing is given one more chance, on the driver's own clock.
            None if self.keepalive > 0 => {
                self.keepalive -= 1;
                self.due = now.checked_add(KEEPALIVE);
                true
            }
            None => false,
        }
    }
}

/// A duration in the units somebody would say it in, at most two of them.
///
/// Not a message in a catalog: these are the unit letters the person typed into `/loop` coming
/// back, and translating them would answer `5m` with something they could not type again.
pub fn spell(duration: Duration) -> String {
    let mut left = duration.as_secs();
    let mut parts = Vec::new();
    for (unit, worth) in [("d", 86_400), ("h", 3_600), ("m", 60), ("s", 1)] {
        let count = left / worth;
        if count > 0 {
            parts.push(format!("{count}{unit}"));
            left -= count * worth;
        }
        if parts.len() == 2 {
            break;
        }
    }
    if parts.is_empty() {
        return "0s".to_string();
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every(request: &Request) -> Duration {
        match request.pacing {
            Pacing::Every(duration) => duration,
            Pacing::SelfPaced => panic!("expected an interval"),
        }
    }

    /// The form the command was documented with, and the one a person types when they already
    /// know what they want repeated.
    #[test]
    fn an_interval_written_first_is_taken_off_the_front() {
        let request = request("5m check the deploy");
        assert_eq!(every(&request), Duration::from_secs(300));
        assert_eq!(request.prompt, "check the deploy");
    }

    #[test]
    fn every_unit_letter_is_understood_at_the_front() {
        for (line, seconds) in [
            ("90s watch", 90),
            ("5m watch", 300),
            ("2h watch", 7_200),
            ("1d watch", 86_400),
        ] {
            let request = request(line);
            assert_eq!(every(&request), Duration::from_secs(seconds), "{line}");
        }
    }

    /// The other way people write it, which reads as a sentence rather than as an argument.
    #[test]
    fn an_interval_written_last_is_taken_off_the_end() {
        let request = request("check the deploy every 20m");
        assert_eq!(every(&request), Duration::from_secs(1_200));
        assert_eq!(request.prompt, "check the deploy");
    }

    #[test]
    fn a_trailing_interval_may_spell_its_unit_out() {
        for line in [
            "check it every 20 minutes",
            "check it every 20 min",
            "check it every 20m",
            "check it EVERY 20 MINUTES",
        ] {
            let request = request(line);
            assert_eq!(every(&request), Duration::from_secs(1_200), "{line}");
            assert_eq!(request.prompt, "check it", "{line}");
        }
    }

    /// The whole reason the trailing form insists on a time expression. Without it, half the
    /// sentences somebody would loop over would lose their last two words.
    #[test]
    fn every_without_a_time_after_it_is_words_rather_than_an_interval() {
        for line in ["check every PR", "look at every so often", "review every"] {
            let request = request(line);
            assert_eq!(request.pacing, Pacing::SelfPaced, "{line}");
            assert_eq!(request.prompt, line, "{line}");
        }
    }

    /// A word that merely begins with every is not an every clause, and the sentence keeps it.
    #[test]
    fn a_word_that_merely_starts_with_every_is_not_an_interval() {
        for line in [
            "check everything 20m",
            "notify everyone 30 minutes",
            "search everywhere 1h",
            "practice everyday 10s",
        ] {
            let request = request(line);
            assert_eq!(request.pacing, Pacing::SelfPaced, "{line}");
            assert_eq!(request.prompt, line, "{line}");
        }

        let with_trailing = request("check everything 20m every 5m");
        assert_eq!(every(&with_trailing), Duration::from_secs(300));
        assert_eq!(with_trailing.prompt, "check everything 20m");
    }

    /// The front of the line is read first, so a line with both is paced by the one written as
    /// an argument rather than by the one written in the sentence.
    #[test]
    fn a_leading_interval_wins_over_a_trailing_one() {
        let request = request("5m check the deploy every 20m");
        assert_eq!(every(&request), Duration::from_secs(300));
        assert_eq!(request.prompt, "check the deploy every 20m");
    }

    #[test]
    fn a_line_with_no_interval_is_paced_by_the_planner() {
        let request = request("watch the build");
        assert_eq!(request.pacing, Pacing::SelfPaced);
        assert_eq!(request.prompt, "watch the build");
    }

    /// Nothing to send is not a loop, however it was written. Kept apart from the bare word so
    /// that a person who typed an interval and stopped is told what the command needs, rather
    /// than handed a report about the loop they were in the middle of replacing.
    #[test]
    fn an_interval_with_nothing_to_send_is_not_a_request() {
        for line in ["5m", "  5m  ", "every 5m"] {
            assert_eq!(parse(line), Asked::Unreadable, "{line}");
        }
    }

    /// What is going to happen next without anybody typing is the one thing about a session that
    /// its transcript does not hold, so the bare word asks for it rather than being a mistake.
    #[test]
    fn the_bare_command_asks_what_is_repeating() {
        for line in ["", "   "] {
            assert_eq!(parse(line), Asked::Report, "{line}");
        }
    }

    #[test]
    fn stop_on_its_own_ends_the_loop() {
        for line in ["stop", "  stop  "] {
            assert_eq!(parse(line), Asked::Stop, "{line}");
        }
    }

    /// A terminal that capitalises the first letter of a line, or a shift held a beat too long.
    /// Read as a line instead, the word becomes a loop sending `Stop` to a model on its own pace
    /// until somebody presses ctrl-c twice, which is the one wrong reading here that spends money.
    #[test]
    fn the_word_ends_the_loop_whatever_its_case() {
        for line in ["Stop", "STOP", "  StOp  "] {
            assert_eq!(parse(line), Asked::Stop, "{line}");
        }
    }

    /// `stop every 5m` is somebody ending the loop that runs every five minutes, and `5m stop` is
    /// the same sentence the other way round. Read as lines they replace that loop with one sending
    /// the bare word `stop` to a model, which is the one remainder that cannot be a prompt.
    #[test]
    fn an_interval_beside_the_word_still_ends_the_loop() {
        for line in ["stop every 5m", "5m stop", "  STOP every 1h  "] {
            assert_eq!(parse(line), Asked::Stop, "{line}");
        }
    }

    /// The one place in the command where a reserved word could eat a line somebody meant to have
    /// repeated. `stop` ends the loop as the whole of the line to be sent, and is an ordinary first
    /// word of a prompt anywhere else.
    #[test]
    fn a_line_that_begins_with_stop_is_still_a_line_to_repeat() {
        for line in ["stop the deploy", "stop 5m", "stopping at noon"] {
            let asked = request(line);
            assert_eq!(asked.pacing, Pacing::SelfPaced, "{line}");
            assert_eq!(asked.prompt, line, "{line}");
        }

        let with_interval = request("stop the deploy every 5m");
        assert_eq!(every(&with_interval), Duration::from_secs(300));
        assert_eq!(with_interval.prompt, "stop the deploy");
    }

    /// A word that merely starts with a digit is not an interval, and the line keeps it.
    #[test]
    fn a_word_that_is_not_a_time_stays_part_of_the_prompt() {
        let request = request("5minutes to midnight");
        assert_eq!(request.pacing, Pacing::SelfPaced);
        assert_eq!(request.prompt, "5minutes to midnight");
    }

    /// The person's own number is honoured as far down as a loop goes, which is far enough that
    /// the interval somebody actually types is usually the interval they get.
    #[test]
    fn an_interval_the_person_gave_is_kept_down_to_the_floor() {
        let request = request("10s watch");
        assert_eq!(every(&request), Duration::from_secs(10));
        assert_eq!(request.adjusted, None);
    }

    #[test]
    fn an_interval_faster_than_the_floor_is_raised_to_it_and_said_so() {
        let request = request("1s watch");
        assert_eq!(every(&request), FLOOR);
        assert_eq!(request.adjusted, Some(Held::Raised(FLOOR)));
    }

    #[test]
    fn an_interval_longer_than_a_loop_may_live_is_capped() {
        let request = request("30d watch");
        assert_eq!(every(&request), MAX_AGE);
        assert_eq!(request.adjusted, Some(Held::Capped(MAX_AGE)));
    }

    /// A number nothing could hold is a word made of digits, not a schedule, and reading it as
    /// one would have to invent a duration nobody asked for.
    #[test]
    fn a_count_too_large_to_be_a_duration_is_not_an_interval() {
        let request = request("check it every 99999999999999999999 days");
        assert_eq!(request.pacing, Pacing::SelfPaced);
    }

    #[test]
    fn an_interval_within_the_bounds_is_reported_as_unadjusted() {
        let request = request("5m watch");
        assert_eq!(request.adjusted, None);
    }

    /// The driver keeps its own time, so a planner cannot lengthen or shorten a loop the person
    /// set the pace of.
    #[test]
    fn a_paced_loop_ignores_what_a_turn_asked_for() {
        let mut running = Running::begin(request("5m watch"));
        let now = Instant::now();
        running.dispatching();
        assert!(running.ended(Some(Wakeup::asked(3_600, false)), now,));
        assert_eq!(running.until(now), Some(Duration::from_secs(300)));
    }

    /// The pictures are handed over once and then there are none left to hand over, so "the first
    /// tick and no other" holds because the loop no longer has them rather than because something
    /// counted the ticks. What the ticks after it send is the line with the markers put back.
    #[test]
    fn a_pasted_picture_goes_to_one_tick_and_the_settled_line_to_every_other() {
        let picture = crate::state::AttachedImage {
            marker: "[Image #1]".to_string(),
            media_type: "image/png",
            bytes: b"pixels".to_vec(),
        };
        let mut running = Running::begin(request("5m look at [Image #1]")).carrying(
            vec![picture.clone()],
            Vec::new(),
            "look at the picture".to_string(),
        );

        assert_eq!(
            running.dispatching(),
            ("look at [Image #1]".to_string(), vec![picture], Vec::new())
        );
        assert_eq!(
            running.dispatching(),
            ("look at the picture".to_string(), Vec::new(), Vec::new())
        );
        assert_eq!(
            running.dispatching(),
            ("look at the picture".to_string(), Vec::new(), Vec::new())
        );
    }

    /// A dropped file leaves with the same tick and for the same reason, and what the ticks after it
    /// send is its name. A name is what a turn can act on: it reads the file through the gate it
    /// reads any other file through, so a later tick looks at the same file rather than at a marker
    /// standing for nothing.
    #[test]
    fn a_dropped_file_goes_to_one_tick_and_its_name_to_every_other() {
        let file = crate::state::Attached {
            marker: "[Image #1]".to_string(),
            name: "shot.png".to_string(),
            shown: "/home/me/shot.png".to_string(),
            kind: crate::dropped::Kind::Attachment("image/png"),
        };
        let mut running = Running::begin(request("5m look at [Image #1]")).carrying(
            Vec::new(),
            vec![file.clone()],
            "look at shot.png".to_string(),
        );

        assert_eq!(
            running.dispatching(),
            ("look at [Image #1]".to_string(), Vec::new(), vec![file])
        );
        assert_eq!(
            running.dispatching(),
            ("look at shot.png".to_string(), Vec::new(), Vec::new())
        );
    }

    #[test]
    fn a_self_paced_loop_waits_as_long_as_the_turn_asked() {
        let mut running = Running::begin(request("watch"));
        let now = Instant::now();
        running.dispatching();
        assert!(running.ended(Some(Wakeup::asked(900, false)), now,));
        assert_eq!(running.until(now), Some(Duration::from_secs(900)));
    }

    /// The failure a loop a turn started would otherwise have. A person who types `/loop` has not
    /// seen the work done once, so the first tick goes immediately; a turn that asked for a later
    /// look has just taken one and is reporting it, so an immediate tick would send the line again
    /// before the person has read the first answer and reply with the same look twice.
    #[test]
    fn a_loop_a_turn_asked_for_starts_a_wait_away_rather_than_now() {
        let now = Instant::now();
        let running = Running::armed(
            "tell me when a.txt changes".to_string(),
            Wakeup::asked(900, false),
            now,
        );
        assert_eq!(running.until(now), Some(Duration::from_secs(900)));
        assert!(
            !running.due(now),
            "a loop a turn asked for sent its first tick at once"
        );
        assert_eq!(running.prompt(), "tell me when a.txt changes");
    }

    /// A turn cannot give an interval, only the wait until the next look, and it is asked again
    /// then. So the loop it starts is self-paced whatever it said, and a loop reported as the
    /// person's would show their pace in the status bar and lose the tool that keeps it alive.
    #[test]
    fn a_loop_a_turn_asked_for_is_paced_by_the_turns() {
        let running = Running::armed(
            "tell me when a.txt changes".to_string(),
            Wakeup::asked(900, false),
            Instant::now(),
        );
        assert_eq!(running.pacing(), Pacing::SelfPaced);
    }

    /// The bounds are the tool's, so this is the loop honouring what the tool already held the
    /// number to rather than a second clamp of its own.
    #[test]
    fn a_wait_a_turn_asked_for_is_held_to_the_bounds() {
        for (asked, held) in [(0, Wakeup::FLOOR), (86_400, Wakeup::CEILING)] {
            let mut running = Running::begin(request("watch"));
            let now = Instant::now();
            running.dispatching();
            running.ended(Some(Wakeup::asked(asked, false)), now);
            assert_eq!(running.until(now), Some(held), "{asked}");
        }
    }

    /// One forgotten turn is a turn that forgot. Two is a loop nobody is running, and waking it
    /// every twenty minutes for the rest of the session helps nobody.
    #[test]
    fn a_self_paced_turn_that_says_nothing_is_woken_once_more_and_then_the_loop_ends() {
        let mut running = Running::begin(request("watch"));
        let now = Instant::now();

        running.dispatching();
        assert!(running.ended(None, now));
        assert_eq!(running.until(now), Some(KEEPALIVE));

        running.dispatching();
        assert!(!running.ended(None, now));
    }

    /// The budget is for turns that stopped saying when to wake, not for the one that did.
    #[test]
    fn a_turn_that_says_when_to_wake_restores_the_fallback() {
        let mut running = Running::begin(request("watch"));
        let now = Instant::now();

        running.dispatching();
        running.ended(None, now);
        running.dispatching();
        running.ended(Some(Wakeup::asked(120, false)), now);

        running.dispatching();
        assert!(running.ended(None, now));
    }

    #[test]
    fn quiet_ticks_are_counted_until_one_reports_something() {
        let mut running = Running::begin(request("watch"));
        let now = Instant::now();
        for expected in [1, 2, 3] {
            running.dispatching();
            running.ended(Some(Wakeup::asked(60, true)), now);
            assert_eq!(running.quiet(), expected);
        }
        running.dispatching();
        running.ended(Some(Wakeup::asked(60, false)), now);
        assert_eq!(running.quiet(), 0);
    }

    /// A tick already running is not due again. Without this a turn that outlasts its interval
    /// would have a second tick waiting the moment it drew breath.
    #[test]
    fn a_tick_in_flight_is_not_due_again() {
        let mut running = Running::begin(request("5m watch"));
        let now = Instant::now();
        running.dispatching();
        running.ended(None, now);
        let due = now + Duration::from_secs(600);
        assert!(running.due(due));
        running.dispatching();
        assert!(!running.due(due));
    }

    #[test]
    fn a_loop_with_nothing_armed_is_not_due() {
        let running = Running::begin(request("watch"));
        assert!(!running.due(Instant::now() + Duration::from_secs(86_400)));
    }

    #[test]
    fn a_loop_older_than_a_week_has_aged_out() {
        let running = Running::begin(request("5m watch"));
        let now = Instant::now();
        assert!(!running.aged_out(now + MAX_AGE - Duration::from_secs(1)));
        assert!(running.aged_out(now + MAX_AGE));
    }

    #[test]
    fn a_duration_is_spelled_in_the_units_it_was_asked_for() {
        assert_eq!(spell(Duration::from_secs(45)), "45s");
        assert_eq!(spell(Duration::from_secs(300)), "5m");
        assert_eq!(spell(Duration::from_secs(90)), "1m 30s");
        assert_eq!(spell(Duration::from_secs(5_400)), "1h 30m");
        assert_eq!(spell(Duration::from_secs(86_400)), "1d");
        assert_eq!(spell(Duration::ZERO), "0s");
    }
}
