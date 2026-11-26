//! Repeating a prompt the person typed.
//!
//! `/loop` holds a line and sends it again on a schedule, and a session holds a few such loops at
//! once, each numbered the way a watch is. The line is settled the moment it is
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
//!
//! Every front end uses this scheduler. Two things differ between them, and each is a type
//! parameter of [`Running`] and [`Loops`]: `C`, what the first tick carries besides its line, such
//! as a picture pasted into it, and `N`, where every tick reads the names in its line from. A
//! front end with neither uses `()` for both.

pub use crate::turn::{Tick, Wakeup};
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
pub const FLOOR: Duration = Duration::from_secs(5);

/// How long a loop may run before it ends itself.
pub const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// What the driver waits when a self-paced turn ended without saying when to wake.
pub const KEEPALIVE: Duration = Duration::from_secs(1_200);

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

/// The word that ends a loop, rather than starting one over it.
const STOP: &str = "stop";

/// How many loops a session holds at once.
///
/// Low, because every tick of every loop is a turn with the whole conversation re-sent, and the
/// cost of that is the one number here nobody sees growing. A sixth is refused rather than
/// replacing one, since the person typed the other five.
pub const MAX_LIVE: usize = 5;

/// How the moment of the next tick is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pacing {
    /// The person gave an interval, and the driver keeps to it without asking anybody.
    Every(Duration),
    /// Nobody gave one, so each tick's turn says when the next is due.
    SelfPaced,
    /// `/after`: one tick, once the delay is up, and then nothing.
    Once(Duration),
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
    /// End a loop: the one with this number, or the only one where none was named.
    Stop(Option<usize>),
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
    if let Some(which) = stop_of(argument) {
        return Asked::Stop(which);
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

/// The reserved word, however somebody's terminal capitalised it, and the loop it names where it
/// names one.
///
/// Case-insensitive because the miss is not a message: a line that is the word and is not read as
/// one becomes a loop sending `Stop` to a model until somebody presses Ctrl-C twice. The number is
/// the whole of what follows the word and nothing else is read as one, so `stop 2 the deploy` is
/// still a line to repeat. A number too large to be a loop's names none, and is said to.
fn stop_of(line: &str) -> Option<Option<usize>> {
    if line.eq_ignore_ascii_case(STOP) {
        return Some(None);
    }
    let (word, number) = line.split_once(char::is_whitespace)?;
    let number = number.trim();
    if !word.eq_ignore_ascii_case(STOP)
        || number.is_empty()
        || !number.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(Some(number.parse().unwrap_or(usize::MAX)))
}

/// A request from an interval and what is left of the line, where anything is left.
fn interval(asked: Duration, prompt: &str) -> Asked {
    if prompt.is_empty() {
        return Asked::Unreadable;
    }
    if let Some(which) = stop_of(prompt) {
        return Asked::Stop(which);
    }
    let (held, adjusted) = bound(asked);
    Asked::Start(Request {
        pacing: Pacing::Every(held),
        prompt: prompt.to_string(),
        adjusted,
    })
}

/// A wait the person gave, held to the bounds, and which way it moved where it did.
pub fn bound(asked: Duration) -> (Duration, Option<Held>) {
    let held = asked.clamp(FLOOR, MAX_AGE);
    let adjusted = match held {
        _ if held == asked => None,
        _ if held == FLOOR => Some(Held::Raised(held)),
        _ => Some(Held::Capped(held)),
    };
    (held, adjusted)
}

/// What `/after` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delayed {
    /// Send this line once, when the delay is up.
    Start(Request),
    /// No delay, or a delay with nothing after it, answered by saying what the command needs.
    Unreadable,
}

/// Read the argument to `/after`: a delay at the front of the line, and the line to send once.
///
/// The delay is read the way [`leading_interval`] reads an interval, and only there: a line that
/// does not begin with one is not a request, since a trailing `every` clause means repetition and
/// this does not repeat. The rest of the line is sent unchanged, so `/after 5m /status` sends the
/// seven characters `/status` and runs nothing.
pub fn parse_after(argument: &str) -> Delayed {
    let argument = argument.trim();
    let Some((asked, prompt)) = leading_interval(argument) else {
        return Delayed::Unreadable;
    };
    if prompt.is_empty() {
        return Delayed::Unreadable;
    }
    let (held, adjusted) = bound(asked);
    Delayed::Start(Request {
        pacing: Pacing::Once(held),
        prompt: prompt.to_string(),
        adjusted,
    })
}

/// An interval written as the first word, and the rest of the line.
pub fn leading_interval(argument: &str) -> Option<(Duration, &str)> {
    let (token, rest) = match argument.split_once(char::is_whitespace) {
        Some((token, rest)) => (token, rest.trim()),
        None => (argument, ""),
    };
    let seconds = measure(token, &SHORT_UNITS)?;
    Some((seconds, rest))
}

/// An interval written as an `every ...` clause at the end, and the line without it.
pub fn trailing_interval(argument: &str) -> Option<(Duration, &str)> {
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
///
/// `C` is what the first tick carries besides its line, and `N` is where every tick reads the
/// names in its line from. Both are the front end's, and `()` where it has neither.
#[derive(Debug, Clone)]
pub struct Running<C = (), N = ()> {
    /// Which loop this is, as `/loop` lists it and `/loop stop` names it. Given when it joins a
    /// session's loops, and never reused within that session.
    number: usize,
    /// The line every tick sends. Settled when it was typed and never written to again.
    prompt: String,
    /// What the line named, such as pictures pasted into it or files dropped onto it, which the
    /// first tick takes and no later one has.
    ///
    /// Emptied by that tick, so the rule holds because there is nothing left to carry rather than
    /// because something counted the ticks. `None` from then on, and for a loop that carries
    /// nothing.
    carry: Option<C>,
    /// What a later tick sends instead, with what was carried put back to words or names.
    ///
    /// `None` where the line named nothing, which is every loop but one somebody dropped on or
    /// pasted into.
    settled: Option<String>,
    /// Where the names of every tick are read from: in the terminal, the line the box showed when
    /// this was typed, markers included, so a tick reads the names the person typed and not the
    /// words a paste stood for (NAME-12). The default for a loop no box line started.
    names: N,
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

impl<C: Default, N: Default> Running<C, N> {
    /// Start a loop, with nothing due: the caller dispatches the first tick straight away.
    ///
    /// Not for [`Pacing::Once`], whose one tick is the delay away: see [`Running::after`].
    pub fn begin(request: Request) -> Self {
        Self {
            number: 0,
            prompt: request.prompt,
            carry: None,
            settled: None,
            names: N::default(),
            pacing: request.pacing,
            began: Instant::now(),
            due: None,
            running: false,
            keepalive: KEEPALIVE_BUDGET,
            ticks: 0,
            quiet: 0,
        }
    }

    /// Start a loop that sends its line once, with that one tick due when the delay is up.
    ///
    /// Nothing is sent now, so unlike [`Running::begin`] the caller dispatches nothing.
    pub fn after(request: Request, now: Instant) -> Self {
        let mut running = Self::begin(request);
        if let Pacing::Once(delay) = running.pacing {
            running.began = now;
            running.due = now.checked_add(delay);
        }
        running
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
            number: 0,
            prompt,
            // Nothing carried and nothing to settle: what a turn asks to look at again is a line
            // out of the conversation, and no box was open for anybody to paste into or drop on.
            carry: None,
            settled: None,
            names: N::default(),
            pacing: Pacing::SelfPaced,
            began: now,
            due: now.checked_add(wakeup.after),
            running: false,
            keepalive: KEEPALIVE_BUDGET,
            ticks: 0,
            quiet: usize::from(wakeup.quiet),
        }
    }

    /// Record that a tick is going out, and give up what it sends.
    ///
    /// The first tick after a paste or a drop sends the line as it was typed, with what was
    /// carried, which is what any prompt with a paste or a drop in it sends. Every tick after it
    /// sends the settled line and carries nothing, because what was staged left with the tick that
    /// took it.
    ///
    /// Read off whether the carry is still held rather than off a tick count, so the two cannot
    /// disagree: what decides is whether anything is left to carry.
    pub fn dispatching(&mut self) -> (String, C) {
        self.ticks += 1;
        self.running = true;
        self.due = None;
        match self.carry.take() {
            Some(carry) => (self.prompt.clone(), carry),
            None => (
                self.settled.clone().unwrap_or_else(|| self.prompt.clone()),
                C::default(),
            ),
        }
    }
}

impl<C, N> Running<C, N> {
    /// Whether this sends its line once and goes, rather than repeating it.
    pub fn once(&self) -> bool {
        matches!(self.pacing, Pacing::Once(_))
    }

    /// Keep where the names of every tick are read from (NAME-12).
    pub fn names_as(mut self, names: N) -> Self {
        self.names = names;
        self
    }

    /// Where the names of every tick are read from.
    pub fn names(&self) -> &N {
        &self.names
    }

    /// Carry what the line named, for the first tick and no other.
    ///
    /// `settled` is the same line with each marker put back to what stands for it durably, and is
    /// what every tick after the first sends. What was named belongs to the line the person pressed
    /// Enter on, so the first tick is a turn like any other prompt with a paste or a drop in it; a
    /// later tick has nothing to carry, and a marker left in one would name a screenshot nothing came
    /// with or a file nothing read.
    ///
    /// Only called where something was named, which is why nothing here asks: the front end that
    /// starts the loop asks, where the same question decides what is said about it.
    pub fn carrying(mut self, carry: C, settled: String) -> Self {
        self.carry = Some(carry);
        self.settled = Some(settled);
        self
    }

    pub fn number(&self) -> usize {
        self.number
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
    ///
    /// Nothing for a line sent once: that turn is the person's prompt arriving later, and telling it
    /// it is inside a loop would send it looking for a way to say when to run again.
    pub fn tick(&self) -> Option<Tick> {
        (self.running && !self.once()).then_some(Tick {
            number: self.ticks,
            self_paced: self.self_paced(),
            unpaceable: false,
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
            Pacing::Once(_) => t!(status_loop_once).to_string(),
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
    ///
    /// Never a line sent once: its delay is already held to [`MAX_AGE`], and it would otherwise end
    /// at the very moment it was due.
    pub fn aged_out(&self, now: Instant) -> bool {
        !self.once() && now.saturating_duration_since(self.began) >= MAX_AGE
    }

    /// Whether a tick should be dispatched now.
    ///
    /// A tick in flight is not due again, and a loop with nothing armed is waiting rather than
    /// ready. Whether the *session* is free to take one is the caller's question, not this one's.
    pub fn due(&self, now: Instant) -> bool {
        !self.running && self.due.is_some_and(|due| due <= now)
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

        match self.pacing {
            Pacing::Every(every) => {
                self.due = now.checked_add(every);
                return true;
            }
            // Sent, so removed.
            Pacing::Once(_) => return false,
            Pacing::SelfPaced => {}
        }

        match wakeup {
            // A turn that said it is finished is a deliberate ending, not a silence, so the
            // keepalive is not spent on it.
            Some(wakeup) if wakeup.stop => false,
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

/// The loops a session holds, oldest first.
///
/// A loop is numbered when it joins and keeps the number until it ends, and a number is not handed
/// out twice: `/loop stop 2` typed after loop 2 ended and another began would otherwise end a loop
/// the person never meant.
#[derive(Debug, Clone)]
pub struct Loops<C = (), N = ()> {
    live: Vec<Running<C, N>>,
    /// The number the next loop to join is given, less one.
    issued: usize,
}

impl<C, N> Default for Loops<C, N> {
    fn default() -> Self {
        Self {
            live: Vec::new(),
            issued: 0,
        }
    }
}

impl<C, N> Loops<C, N> {
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    pub fn len(&self) -> usize {
        self.live.len()
    }

    /// Whether another loop would be refused.
    pub fn is_full(&self) -> bool {
        self.live.len() >= MAX_LIVE
    }

    pub fn as_slice(&self) -> &[Running<C, N>] {
        &self.live
    }

    /// Every loop, oldest first.
    pub fn iter(&self) -> std::slice::Iter<'_, Running<C, N>> {
        self.live.iter()
    }

    /// The oldest loop, which is the only one in a session that holds one.
    pub fn oldest(&self) -> Option<&Running<C, N>> {
        self.live.first()
    }

    /// The number the next loop will be given.
    pub fn next_number(&self) -> usize {
        self.issued + 1
    }

    /// Add a loop and say which number it is. The caller has checked [`Loops::is_full`].
    pub fn start(&mut self, mut running: Running<C, N>) -> usize {
        self.issued += 1;
        running.number = self.issued;
        self.live.push(running);
        self.issued
    }

    pub fn get_mut(&mut self, number: usize) -> Option<&mut Running<C, N>> {
        self.live
            .iter_mut()
            .find(|running| running.number == number)
    }

    /// The answer is what lets `/loop stop <n>` tell a number that never ran from one already ended.
    pub fn stop(&mut self, number: usize) -> bool {
        let before = self.live.len();
        self.live.retain(|running| running.number != number);
        self.live.len() != before
    }

    /// The count is what the stop notice reports to the person.
    pub fn stop_all(&mut self) -> usize {
        let count = self.live.len();
        self.live.clear();
        count
    }

    /// The loop whose tick is in flight. At most one, because ticks go one at a time.
    pub fn in_flight(&self) -> Option<&Running<C, N>> {
        self.live.iter().find(|running| running.ticking())
    }

    pub fn in_flight_mut(&mut self) -> Option<&mut Running<C, N>> {
        self.live.iter_mut().find(|running| running.ticking())
    }

    /// The loop whose tick is due soonest, and so the one that goes next.
    ///
    /// Ties go to the older loop, which is the one `/loop` lists first. A loop with a tick in
    /// flight is never due again, so it is not in the running.
    pub fn next_due(&self, now: Instant) -> Option<usize> {
        self.live
            .iter()
            .filter(|running| running.due(now))
            .min_by_key(|running| (running.due, running.number))
            .map(|running| running.number)
    }

    /// The loop that goes next whether or not its moment has come: the earliest one armed, else a
    /// loop that has not ticked at all, and one with a tick in flight only when nothing else is.
    pub fn next_to_go(&self) -> Option<usize> {
        self.live
            .iter()
            .min_by_key(|running| {
                (
                    running.ticking(),
                    running.due.is_none(),
                    running.due,
                    running.number,
                )
            })
            .map(|running| running.number)
    }

    /// When the soonest armed tick is due, which is what the row under the box counts down to.
    pub fn soonest(&self, now: Instant) -> Option<Duration> {
        self.live
            .iter()
            .filter_map(|running| running.until(now))
            .min()
    }

    /// End every loop that has outlived a loop, and give back the numbers that went.
    pub fn age_out(&mut self, now: Instant) -> Vec<usize> {
        let gone: Vec<usize> = self
            .live
            .iter()
            .filter(|running| running.aged_out(now))
            .map(|running| running.number)
            .collect();
        self.live.retain(|running| !running.aged_out(now));
        gone
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

    fn request(argument: &str) -> Request {
        parse(argument)
            .started()
            .unwrap_or_else(|| panic!("{argument:?} started no loop"))
    }

    /// A loop that carries nothing sends its line on every tick, and hands over the empty carry
    /// each time. A wrong implementation sends nothing, or a settled line nobody set.
    #[test]
    fn a_loop_that_carries_nothing_sends_its_line_on_every_tick() {
        let mut running: Running = Running::begin(request("5m check the deploy"));
        for _ in 0..3 {
            assert_eq!(running.dispatching(), ("check the deploy".to_string(), ()));
        }
        assert_eq!(running.ticks(), 3);
    }

    /// The carry goes to the first tick only, with the line as it was typed, and every tick after
    /// it sends the settled line with the empty carry. A wrong implementation hands the carry over
    /// twice, or sends the settled line first.
    #[test]
    fn the_carry_goes_to_the_first_tick_and_the_settled_line_to_every_other() {
        let mut running: Running<Vec<&str>> = Running::begin(request("5m look at [Image #1]"))
            .carrying(vec!["picture"], "look at the picture".to_string());

        assert_eq!(
            running.dispatching(),
            ("look at [Image #1]".to_string(), vec!["picture"])
        );
        for _ in 0..2 {
            assert_eq!(
                running.dispatching(),
                ("look at the picture".to_string(), Vec::new())
            );
        }
    }

    /// The names are kept for every tick rather than carried by the first, so a later tick reads
    /// the same names. A wrong implementation loses them after the first tick.
    #[test]
    fn the_names_are_kept_for_every_tick() {
        let mut running: Running<Vec<&str>, &str> = Running::begin(request("5m watch"))
            .names_as("the line")
            .carrying(vec!["file"], "watch".to_string());
        running.dispatching();
        running.dispatching();
        assert_eq!(running.names(), &"the line");
    }

    /// A loop a turn asked for carries nothing and is due a wait away.
    #[test]
    fn a_loop_a_turn_asked_for_carries_nothing() {
        let now = Instant::now();
        let mut running: Running<Vec<&str>> =
            Running::armed("look again".to_string(), Wakeup::asked(900, false), now);
        assert!(!running.due(now));
        assert_eq!(
            running.dispatching(),
            ("look again".to_string(), Vec::new())
        );
    }

    /// Loops numbered and ordered over the generic carry, as over the terminal's.
    #[test]
    fn loops_of_any_carry_are_numbered_and_stopped() {
        let mut loops: Loops = Loops::default();
        assert_eq!(loops.start(Running::begin(request("5m a"))), 1);
        assert_eq!(loops.start(Running::begin(request("5m b"))), 2);
        assert!(loops.stop(1));
        assert_eq!(loops.start(Running::begin(request("5m c"))), 3);
        assert_eq!(loops.stop_all(), 2);
        assert!(loops.is_empty());
    }
}
