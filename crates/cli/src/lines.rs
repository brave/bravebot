//! The lines a run given `--input-format lines` reads from stdin, and what becomes of each (CLI-35).
//!
//! One process serves several turns: a `prompt` line starts one, a `steer` line reaches the one
//! running, an `answer` line replies to a question the planner asked, and a `stop` line ends the
//! turn in flight. The lines are read on a thread of their own, because a turn waits on a model
//! for as long as it takes and a line sent meanwhile has to be seen then.
//!
//! The envelope of a line is parsed here: its `type`, a question's `id` and the positions of the
//! choices picked are fixed words and numbers, which decide among alternatives this module lists.
//! The `text` of a `prompt` or `steer` is carried as the line spelt it: it is unescaped, and seen
//! to be there, and nothing in it is read. It reaches the turn either as piped input, which the
//! planner is given a reference to, or, where a person vouched for the channel, as the words of a
//! typed prompt.
//!
//! Parsed by hand, as [`crate::json`] writes by hand: this crate carries no serialisation
//! dependency, and the envelope is flat. A field the envelope does not name is checked to be JSON
//! and dropped as it is read, whatever it holds, so a sender may add one and a line within the cap
//! cannot cost more memory than its own length.

use crate::json;
use bravebot_core::ask::{Answer, Asking};
use bravebot_core::cancel::Cancel;
use std::collections::VecDeque;
use std::io::{BufRead, ErrorKind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// How deep a value the envelope does not name may nest before the line is refused, so a line of
/// open brackets cannot exhaust the stack of the thread reading it.
const DEEPEST: usize = 32;

/// How many questions an `answer` may answer at once. The planner puts four at most
/// ([`bravebot_core::ask::MOST_AT_ONCE`]), and the room above that is for a change to it.
const MOST_QUESTIONS: usize = 16;

/// How many positions one question's pick may hold. A question has as many choices as the planner
/// wrote, and no pick needs more than this.
const MOST_POSITIONS: usize = 256;

/// How often a question waiting for its answer looks to see that the turn was stopped.
const LOOK_AGAIN: Duration = Duration::from_millis(100);

/// One line, once it has been read.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// Words that start a turn of their own once the turns before it are done.
    Prompt(String),
    /// Words for the turn that is running, delivered at its next round boundary.
    Steer(String),
    /// What to pick for each of the questions asked under `id`, as the positions of the choices.
    Answer { id: u64, choices: Vec<Vec<usize>> },
    /// End the turn that is running.
    Stop,
}

/// Why a line was not taken, as the stream names it.
///
/// A fixed word for each cause, never a sentence made of the line: it is written to a stream a
/// program reads, about a line that program wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    /// Longer than the cap, or not UTF-8.
    Unreadable,
    /// Not one JSON object.
    NotJson,
    /// A `type` this build does not know.
    UnknownType,
    /// A field the line's type needs is missing, repeated or of another kind.
    BadField,
    /// An answer to a question that is not waiting for one, or that does not fit the question.
    NoSuchQuestion,
    /// Words that would take the bytes waiting past the cap.
    TooMuchWaiting,
}

impl Refused {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::NotJson => "not_json",
            Self::UnknownType => "unknown_type",
            Self::BadField => "bad_field",
            Self::NoSuchQuestion => "no_such_question",
            Self::TooMuchWaiting => "too_much_waiting",
        }
    }
}

/// A field of the envelope, as far as the envelope reads one.
enum Value {
    Text(String),
    /// A number written as digits alone that fits a `u64`.
    Whole(u64),
    /// An array of arrays of such numbers, within [`MOST_QUESTIONS`] and [`MOST_POSITIONS`].
    Positions(Vec<Vec<usize>>),
    /// Anything else that is valid JSON.
    Other,
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> Result<(), Refused> {
        match self.peek() {
            Some(found) if found == byte => {
                self.at += 1;
                Ok(())
            }
            _ => Err(Refused::NotJson),
        }
    }

    fn literal(&mut self, word: &[u8]) -> Result<(), Refused> {
        match self.bytes[self.at..].starts_with(word) {
            true => {
                self.at += word.len();
                Ok(())
            }
            false => Err(Refused::NotJson),
        }
    }

    /// Read past one value, checking it is JSON and keeping none of it.
    fn skip(&mut self, depth: usize) -> Result<(), Refused> {
        if depth > DEEPEST {
            return Err(Refused::NotJson);
        }
        match self.peek() {
            Some(b'"') => self.string().map(drop),
            Some(b'{') => self.skip_members(depth + 1),
            Some(b'[') => self.skip_list(depth + 1),
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            Some(b'-' | b'0'..=b'9') => self.number().map(drop),
            _ => Err(Refused::NotJson),
        }
    }

    fn skip_list(&mut self, depth: usize) -> Result<(), Refused> {
        self.eat(b'[')?;
        self.space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(());
        }
        loop {
            self.space();
            self.skip(depth)?;
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(());
                }
                _ => return Err(Refused::NotJson),
            }
        }
    }

    fn skip_members(&mut self, depth: usize) -> Result<(), Refused> {
        self.eat(b'{')?;
        self.space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(());
        }
        loop {
            self.space();
            self.string()?;
            self.space();
            self.eat(b':')?;
            self.space();
            self.skip(depth)?;
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(());
                }
                _ => return Err(Refused::NotJson),
            }
        }
    }

    fn number(&mut self) -> Result<Value, Refused> {
        let start = self.at;
        let negative = self.peek() == Some(b'-');
        if negative {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(Refused::NotJson),
        }
        let mut whole = !negative;
        if self.peek() == Some(b'.') {
            self.at += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(Refused::NotJson);
            }
            self.digits();
            whole = false;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(Refused::NotJson);
            }
            self.digits();
            whole = false;
        }
        let digits = &self.bytes[start..self.at];
        let parsed = whole
            .then(|| std::str::from_utf8(digits).ok()?.parse::<u64>().ok())
            .flatten();
        Ok(parsed.map_or(Value::Other, Value::Whole))
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
    }

    fn hex4(&mut self) -> Result<u32, Refused> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or(Refused::NotJson)?;
        // Checked before the parse, which would accept a leading sign that an escape does not.
        if !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err(Refused::NotJson);
        }
        let text = std::str::from_utf8(digits).map_err(|_| Refused::NotJson)?;
        let unit = u32::from_str_radix(text, 16).map_err(|_| Refused::NotJson)?;
        self.at += 4;
        Ok(unit)
    }

    fn string(&mut self) -> Result<String, Refused> {
        self.eat(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let byte = self.peek().ok_or(Refused::NotJson)?;
            self.at += 1;
            match byte {
                b'"' => return String::from_utf8(out).map_err(|_| Refused::NotJson),
                // JSON does not allow a raw control character inside a string.
                0..=0x1f => return Err(Refused::NotJson),
                b'\\' => {
                    let escaped = self.peek().ok_or(Refused::NotJson)?;
                    self.at += 1;
                    let character = match escaped {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let unit = self.hex4()?;
                            match unit {
                                0xD800..=0xDBFF => {
                                    // A high surrogate is the first half of a pair and nothing else.
                                    self.eat(b'\\')?;
                                    self.eat(b'u')?;
                                    let low = self.hex4()?;
                                    if !(0xDC00..=0xDFFF).contains(&low) {
                                        return Err(Refused::NotJson);
                                    }
                                    char::from_u32(
                                        0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00),
                                    )
                                    .ok_or(Refused::NotJson)?
                                }
                                0xDC00..=0xDFFF => return Err(Refused::NotJson),
                                _ => char::from_u32(unit).ok_or(Refused::NotJson)?,
                            }
                        }
                        _ => return Err(Refused::NotJson),
                    };
                    let mut buffer = [0u8; 4];
                    out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                }
                other => out.push(other),
            }
        }
    }

    /// A string, or any other value read past and kept as [`Value::Other`].
    fn text(&mut self) -> Result<Value, Refused> {
        match self.peek() {
            Some(b'"') => self.string().map(Value::Text),
            _ => self.skip(1).map(|()| Value::Other),
        }
    }

    /// A number written as digits alone, or any other value read past.
    fn whole(&mut self) -> Result<Value, Refused> {
        match self.peek() {
            Some(b'0'..=b'9') => self.number(),
            _ => self.skip(1).map(|()| Value::Other),
        }
    }

    /// An array of arrays of positions, or, where the value is anything else or goes past the
    /// bounds, the value read past and kept as [`Value::Other`].
    fn positions(&mut self) -> Result<Value, Refused> {
        let start = self.at;
        match self.strictly_positions()? {
            Some(found) => Ok(Value::Positions(found)),
            None => {
                self.at = start;
                self.skip(1).map(|()| Value::Other)
            }
        }
    }

    /// `None` where the value is not the shape of a pick, which leaves the parser part way through
    /// it for the caller to start again from the beginning.
    fn strictly_positions(&mut self) -> Result<Option<Vec<Vec<usize>>>, Refused> {
        if self.peek() != Some(b'[') {
            return Ok(None);
        }
        self.at += 1;
        self.space();
        let mut picks = Vec::new();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Some(picks));
        }
        loop {
            self.space();
            if self.peek() != Some(b'[') || picks.len() == MOST_QUESTIONS {
                return Ok(None);
            }
            self.at += 1;
            self.space();
            let mut pick = Vec::new();
            if self.peek() == Some(b']') {
                self.at += 1;
            } else {
                loop {
                    self.space();
                    if !matches!(self.peek(), Some(b'0'..=b'9')) || pick.len() == MOST_POSITIONS {
                        return Ok(None);
                    }
                    let Value::Whole(position) = self.number()? else {
                        return Ok(None);
                    };
                    let Ok(position) = usize::try_from(position) else {
                        return Ok(None);
                    };
                    pick.push(position);
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            break;
                        }
                        _ => return Err(Refused::NotJson),
                    }
                }
            }
            picks.push(pick);
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Some(picks));
                }
                _ => return Err(Refused::NotJson),
            }
        }
    }

    /// The top-level object, keeping the four fields the envelope reads and nothing else.
    fn envelope(&mut self) -> Result<Fields, Refused> {
        self.eat(b'{')?;
        self.space();
        let mut fields = Fields::default();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(fields);
        }
        loop {
            self.space();
            let name = self.string()?;
            self.space();
            self.eat(b':')?;
            self.space();
            match name.as_str() {
                "type" => once(&mut fields.kind, self.text()?)?,
                "text" => once(&mut fields.text, self.text()?)?,
                "id" => once(&mut fields.id, self.whole()?)?,
                "choices" => once(&mut fields.choices, self.positions()?)?,
                _ => self.skip(1)?,
            }
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(fields);
                }
                _ => return Err(Refused::NotJson),
            }
        }
    }
}

/// The fields of the envelope that mean something, each at most once.
///
/// Kept as read and given a meaning only once the line's `type` says which of them it uses, so a
/// sender may put a field of another kind under one of these names on a line that does not use it.
#[derive(Default)]
struct Fields {
    kind: Option<Value>,
    text: Option<Value>,
    id: Option<Value>,
    choices: Option<Value>,
}

/// Set a field that may be set once: a second `type` or `id` is a line that says two things.
fn once(slot: &mut Option<Value>, value: Value) -> Result<(), Refused> {
    match slot.replace(value) {
        None => Ok(()),
        Some(_) => Err(Refused::BadField),
    }
}

/// No words at all is not a turn, as a blank line is not a prompt (CLI-14).
fn words(field: Option<Value>) -> Result<String, Refused> {
    match field {
        Some(Value::Text(words)) if !words.is_empty() => Ok(words),
        _ => Err(Refused::BadField),
    }
}

/// One line of input as the line it is, or the reason it is none.
pub(crate) fn parse(text: &str) -> Result<Line, Refused> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        at: 0,
    };
    parser.space();
    let fields = parser.envelope()?;
    parser.space();
    if parser.peek().is_some() {
        return Err(Refused::NotJson);
    }

    let Some(Value::Text(kind)) = fields.kind else {
        return Err(Refused::BadField);
    };
    match kind.as_str() {
        "prompt" => words(fields.text).map(Line::Prompt),
        "steer" => words(fields.text).map(Line::Steer),
        "stop" => Ok(Line::Stop),
        "answer" => match (fields.id, fields.choices) {
            (Some(Value::Whole(id)), Some(Value::Positions(choices))) => {
                Ok(Line::Answer { id, choices })
            }
            _ => Err(Refused::BadField),
        },
        _ => Err(Refused::UnknownType),
    }
}

/// The next line of `from`, `None` at the end of the input.
///
/// A line over `cap` bytes is read to its end and dropped, so the one after it is read as itself
/// and not from the middle of the one before.
pub(crate) fn next_line(
    from: &mut impl BufRead,
    cap: usize,
) -> std::io::Result<Option<Result<String, Refused>>> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut over = false;
    let mut any = false;
    loop {
        let buffer = match from.fill_buf() {
            Ok(buffer) => buffer,
            // A signal that arrived mid-read is not the end of the input, and `read_line` goes on
            // through one for the same reason.
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if buffer.is_empty() {
            break;
        }
        any = true;
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let end = newline.unwrap_or(buffer.len());
        if !over {
            if bytes.len() + end > cap {
                over = true;
                bytes = Vec::new();
            } else {
                bytes.extend_from_slice(&buffer[..end]);
            }
        }
        from.consume(newline.map_or(end, |at| at + 1));
        if newline.is_some() {
            break;
        }
    }
    if !any {
        return Ok(None);
    }
    if over {
        return Ok(Some(Err(Refused::Unreadable)));
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    Ok(Some(
        String::from_utf8(bytes).map_err(|_| Refused::Unreadable),
    ))
}

/// What a question put to the program allows it to answer with.
struct Shape {
    choices: usize,
    multiple: bool,
}

struct Asked {
    id: u64,
    shape: Vec<Shape>,
}

#[derive(Default)]
struct State {
    /// Words each waiting to start a turn, oldest first.
    prompts: VecDeque<String>,
    /// Words for the turn that is running, which it takes at a round boundary.
    steers: VecDeque<String>,
    /// The bytes held in the two queues, which the cap bounds.
    waiting: usize,
    /// The token of the turn that is running, `None` between turns.
    running: Option<Cancel>,
    /// The question the planner is waiting on.
    asked: Option<Asked>,
    answered: Option<Vec<Answer>>,
    questions: u64,
    /// `next_prompt` and `ask` both wake on it, so a run never waits for a line that can no longer
    /// arrive.
    closed: bool,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    /// Whether a person typed the flag that makes these lines their own words.
    vouched: bool,
    /// The most bytes one line may hold, and the most the words waiting for a turn may hold
    /// together, so a sender that writes faster than turns run cannot fill memory (CLI-4).
    cap: usize,
    events: Mutex<json::Stream>,
}

impl State {
    /// Hold words in the queue of a turn that has yet to start, or in the one a turn already running
    /// takes from, unless that would take what is waiting past `cap`.
    fn hold(
        &mut self,
        words: String,
        for_the_running_turn: bool,
        cap: usize,
    ) -> Result<(), Refused> {
        if self.waiting + words.len() > cap {
            return Err(Refused::TooMuchWaiting);
        }
        self.waiting += words.len();
        match for_the_running_turn {
            true => self.steers.push_back(words),
            false => self.prompts.push_back(words),
        }
        Ok(())
    }
}

/// What the thread reading the lines hands to the run, and the run to the confirmer.
///
/// What INPUT-10 and INPUT-36 settle inside a turn is the agent's, and is reused rather than held
/// here: a steer reaches the planner through [`bravebot_agent::Confirmer::interjection`], which the
/// turn drains at a round boundary in the order sent, and a stop sets the [`Cancel`] the turn
/// carries, as the terminal and the desktop bridge do. What is kept here is the lines protocol's
/// own: a queue of prompts that each wait for a turn of their own, where the interface sends a line
/// typed mid-turn into the turn, the bytes waiting, and the question waiting for its answer. The
/// terminal holds a line sent mid-turn as screen state, with its files and marks, and the desktop
/// bridge leaves a line the turn did not take to the window that sent it, so neither has a module
/// for this to call.
#[derive(Clone)]
pub(crate) struct Channel {
    shared: Arc<Shared>,
}

/// Ends the input when the thread reading it does, however that happens, so a run waiting on a line
/// that can no longer come is told so.
struct Closes(Channel);

impl Drop for Closes {
    fn drop(&mut self) {
        self.0.close();
    }
}

impl Channel {
    /// A channel whose lines are the person's own words where `vouched`, and a program's
    /// otherwise, holding no line over `cap` bytes and no more than `cap` bytes of words waiting.
    /// `events` is where a line that was not taken is said.
    pub(crate) fn new(vouched: bool, events: json::Stream, cap: usize) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
                changed: Condvar::new(),
                vouched,
                cap,
                events: Mutex::new(events),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn say(&self, line: &str) {
        self.shared
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .line(line);
    }

    /// How the run hands a `prompt` line to its turn: as the turn's own prompt where a person
    /// vouched for the channel, and as piped input, which the planner is given a reference to,
    /// where nobody did (CLI-35).
    pub(crate) fn vouched(&self) -> bool {
        self.shared.vouched
    }

    /// Read `input` to its end on a thread of its own, giving each line to the run as it arrives.
    /// A read that fails ends the input as the end of it does.
    pub(crate) fn read(&self, mut input: impl BufRead + Send + 'static) {
        let closes = Closes(self.clone());
        std::thread::spawn(move || {
            let channel = &closes.0;
            let mut number = 0;
            while let Ok(Some(read)) = next_line(&mut input, channel.shared.cap) {
                number += 1;
                channel.deliver(number, read);
            }
        });
    }

    /// Take one line off the input, or say why it was not taken.
    pub(crate) fn deliver(&self, number: usize, read: Result<String, Refused>) {
        let text = match read {
            Ok(text) => text,
            Err(reason) => return self.refuse(number, reason),
        };
        if text.trim().is_empty() {
            return;
        }
        match parse(&text) {
            Ok(line) => {
                if let Err(reason) = self.take(line) {
                    self.refuse(number, reason);
                }
            }
            Err(reason) => self.refuse(number, reason),
        }
    }

    fn refuse(&self, number: usize, reason: Refused) {
        self.say(&json::input_refused_event(number, reason.name()));
    }

    fn take(&self, line: Line) -> Result<(), Refused> {
        let cap = self.shared.cap;
        let mut state = self.lock();
        match line {
            Line::Prompt(words) => state.hold(words, false, cap)?,
            // A running turn reads a steer as words from the person. Where nobody vouched for the
            // channel the planner would be shown a reference to them, which steers nothing, so the
            // words wait for a turn of their own, where a reference is what a prompt gives too. So
            // do words sent while no turn runs, which have no turn to reach.
            Line::Steer(words) => {
                let to_the_running_turn = self.shared.vouched && state.running.is_some();
                state.hold(words, to_the_running_turn, cap)?;
            }
            Line::Stop => {
                if let Some(running) = &state.running {
                    running.cancel();
                }
            }
            Line::Answer { id, choices } => {
                let Some(asked) = state.asked.as_ref().filter(|asked| asked.id == id) else {
                    return Err(Refused::NoSuchQuestion);
                };
                let answers = fit(asked, choices).ok_or(Refused::NoSuchQuestion)?;
                // The question is answered once: a second line naming it finds nothing waiting.
                state.asked = None;
                state.answered = Some(answers);
            }
        }
        self.shared.changed.notify_all();
        Ok(())
    }

    fn close(&self) {
        self.lock().closed = true;
        self.shared.changed.notify_all();
    }

    /// A turn is starting that the run began itself, as it does for the words on the command line.
    /// The token a `stop` line sets is registered as the turn is, so there is no moment a turn is
    /// running that a stop cannot reach.
    pub(crate) fn start(&self) -> Cancel {
        let cancel = Cancel::new();
        self.lock().running = Some(cancel.clone());
        cancel
    }

    /// The words that start the next turn, with the token a stop will set, waiting for them while
    /// the input is open. `None` once the input has ended and nothing is left to run.
    ///
    /// Taking the words and registering the turn are one step, so a steer or a stop sent right
    /// after a prompt reaches the turn that prompt starts and not the gap before it.
    pub(crate) fn next_prompt(&self) -> Option<(String, Cancel)> {
        let mut state = self.lock();
        loop {
            if let Some(words) = state.prompts.pop_front() {
                state.waiting -= words.len();
                let cancel = Cancel::new();
                state.running = Some(cancel.clone());
                return Some((words, cancel));
            }
            if state.closed {
                return None;
            }
            state = self
                .shared
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// The turn that was running has ended, so a `stop` line has nothing to stop.
    ///
    /// A steer it ended without taking starts a turn of its own, after the prompts already waiting,
    /// as a line typed during a turn does in the interface that draws (INPUT-10). Left where it was
    /// it would be handed to the next turn, which it was not aimed at.
    pub(crate) fn idle(&self) {
        let mut state = self.lock();
        state.running = None;
        let left = std::mem::take(&mut state.steers);
        state.prompts.extend(left);
        self.shared.changed.notify_all();
    }

    /// The turn asks for these through [`bravebot_agent::Confirmer::interjection`] at a round
    /// boundary, one call for each steer until none is left, so a steer is read between steps and
    /// not while one runs.
    pub(crate) fn steer(&self) -> Option<String> {
        let mut state = self.lock();
        let words = state.steers.pop_front()?;
        state.waiting -= words.len();
        Some(words)
    }

    /// Put the planner's questions to the program and wait for its answer.
    ///
    /// Declined, with no answers at all as [`bravebot_agent::Confirmer::ask_user`] asks, when the
    /// input has ended without one or the turn was stopped while the question waited.
    pub(crate) fn ask(&self, asking: &Asking) -> Vec<Answer> {
        let id = {
            let mut state = self.lock();
            state.questions += 1;
            let id = state.questions;
            state.answered = None;
            state.asked = Some(Asked {
                id,
                shape: asking
                    .prompts
                    .iter()
                    .map(|prompt| Shape {
                        choices: prompt.rows.len(),
                        multiple: prompt.multiple,
                    })
                    .collect(),
            });
            id
        };
        self.say(&json::question_event(id, asking));

        let mut state = self.lock();
        loop {
            if let Some(answers) = state.answered.take() {
                return answers;
            }
            let stopped = state.running.as_ref().is_some_and(Cancel::is_cancelled);
            if state.closed || stopped {
                state.asked = None;
                return Vec::new();
            }
            state = self
                .shared
                .changed
                .wait_timeout(state, LOOK_AGAIN)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

/// The answers a line gives, where it gives one for each question and each pick is a choice the
/// question has, at most one unless the question takes several, and none twice. An empty pick
/// declines that question.
fn fit(asked: &Asked, choices: Vec<Vec<usize>>) -> Option<Vec<Answer>> {
    if choices.len() != asked.shape.len() {
        return None;
    }
    choices
        .into_iter()
        .zip(&asked.shape)
        .map(|(picked, shape)| {
            let mut sorted = picked.clone();
            sorted.sort_unstable();
            sorted.dedup();
            let fits = sorted.len() == picked.len()
                && picked.iter().all(|at| *at < shape.choices)
                && (shape.multiple || picked.len() <= 1);
            fits.then_some(match picked.is_empty() {
                true => Answer::Declined,
                false => Answer::Chosen(picked),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::ask::{Prompt, Row};
    use std::io::Cursor;

    /// A stream writing into memory, so a test reads back what the run said on stdout.
    #[derive(Clone, Default)]
    struct Said(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Said {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Said {
        fn lines(&self) -> Vec<String> {
            String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner))
                .lines()
                .map(str::to_string)
                .collect()
        }
    }

    fn channel_holding(vouched: bool, cap: usize) -> (Channel, Said) {
        let said = Said::default();
        (
            Channel::new(vouched, json::Stream::new(said.clone()), cap),
            said,
        )
    }

    fn channel(vouched: bool) -> (Channel, Said) {
        channel_holding(vouched, 1024 * 1024)
    }

    fn deliver(channel: &Channel, text: &str) {
        channel.deliver(1, Ok(text.to_string()));
    }

    /// The words of the next turn, which a test that does not stop it has no use for the token of.
    fn next(channel: &Channel) -> Option<String> {
        channel.next_prompt().map(|(words, _)| words)
    }

    fn asking(shapes: &[(usize, bool)]) -> Asking {
        Asking {
            prompts: shapes
                .iter()
                .enumerate()
                .map(|(at, (choices, multiple))| Prompt {
                    header: format!("H{at}"),
                    question: format!("Q{at}?"),
                    rows: (0..*choices)
                        .map(|index| Row {
                            index,
                            label: format!("L{index}"),
                            detail: None,
                        })
                        .collect(),
                    multiple: *multiple,
                    key: format!("k{at}"),
                })
                .collect(),
            closes_when_idle: false,
        }
    }

    /// The four kinds of line, each read as what it names. A parse that kept the words of a
    /// `steer` as a `prompt`, or lost the positions of an answer, would start the wrong turn or
    /// answer the wrong question.
    #[test]
    fn each_kind_of_line_is_read_as_the_kind_it_names() {
        assert_eq!(
            parse(r#"{"type":"prompt","text":"fix it"}"#),
            Ok(Line::Prompt("fix it".into()))
        );
        assert_eq!(
            parse(r#"{"type":"steer","text":"not that file"}"#),
            Ok(Line::Steer("not that file".into()))
        );
        assert_eq!(parse(r#"{"type":"stop"}"#), Ok(Line::Stop));
        assert_eq!(
            parse(r#"{"type":"answer","id":3,"choices":[[0],[1,2],[]]}"#),
            Ok(Line::Answer {
                id: 3,
                choices: vec![vec![0], vec![1, 2], vec![]],
            })
        );
    }

    /// The words of a line reach the turn as the line spelt them. Every escape JSON has is read,
    /// including a pair of surrogates, since a sender written in any language may use any of them.
    #[test]
    fn the_words_of_a_line_are_carried_as_the_line_spelt_them() {
        let cases = [
            (r#""a\"b""#, "a\"b"),
            (r#""a\\b""#, "a\\b"),
            (r#""a\/b""#, "a/b"),
            (r#""a\nb\tc\rd""#, "a\nb\tc\rd"),
            (r#""\b\f""#, "\u{8}\u{c}"),
            (r#""café""#, "café"),
            (r#""😀""#, "😀"),
            (r#""日本語""#, "日本語"),
        ];
        for (spelt, words) in cases {
            let line = format!(r#"{{"type":"prompt","text":{spelt}}}"#);
            assert_eq!(parse(&line), Ok(Line::Prompt(words.into())), "{line}");
        }
    }

    /// A sender may add a field, whatever it holds, and an envelope with the fields in another
    /// order or with space around them is the same line.
    #[test]
    fn a_field_the_envelope_does_not_name_is_skipped_whatever_it_holds() {
        let line = r#"  { "extra": {"a": [1, 2.5e3, -4, true, false, null, "x", {"b": []}]},
            "text": "hi", "n": 1e2, "type" : "prompt" }  "#;
        assert_eq!(parse(line), Ok(Line::Prompt("hi".into())));
        // A name the envelope uses, carrying something else on a line that does not use it, is
        // the sender's own field: an id of its own on a prompt must not refuse the prompt.
        assert_eq!(
            parse(r#"{"type":"prompt","text":"hi","id":"job-7","choices":null}"#),
            Ok(Line::Prompt("hi".into()))
        );
        assert_eq!(
            parse(r#"{"type":"stop","text":7,"id":"x","choices":false}"#),
            Ok(Line::Stop)
        );
    }

    /// Anything that is not one JSON object is refused as such, so a line cut off by a crashed
    /// sender, or two objects run together, starts nothing.
    #[test]
    fn a_line_that_is_not_one_json_object_is_refused() {
        let deep = format!(
            r#"{{"type":"stop","x":{}{}}}"#,
            "[".repeat(DEEPEST + 2),
            "]".repeat(DEEPEST + 2)
        );
        let lines = [
            "",
            "prompt",
            "[]",
            r#""prompt""#,
            "{",
            r#"{"type":"stop""#,
            r#"{"type":"stop"} {"type":"stop"}"#,
            r#"{"type":"stop"} x"#,
            r#"{"type":"stop",}"#,
            r#"{type:"stop"}"#,
            r#"{"type":'stop'}"#,
            "{\"type\":\"pro\nmpt\"}",
            "{\"type\":\"pro\tmpt\"}",
            r#"{"type":"prompt","text":"\x"}"#,
            r#"{"type":"prompt","text":"\ud83d"}"#,
            r#"{"type":"prompt","text":"\ud83dx"}"#,
            r#"{"type":"prompt","text":"\ude00"}"#,
            r#"{"type":"prompt","text":"\u12"}"#,
            r#"{"type":"prompt","text":"\u+123"}"#,
            r#"{"type":"stop","n":01}"#,
            r#"{"type":"stop","n":1.}"#,
            r#"{"type":"stop","n":-}"#,
            r#"{"type":"stop","n":1e}"#,
            r#"{"type":"stop","n":tru}"#,
            deep.as_str(),
        ];
        for line in lines {
            assert_eq!(parse(line), Err(Refused::NotJson), "{line:?}");
        }
    }

    /// A line that is JSON and an object but not a line of ours says which way it is wrong, and a
    /// type this build does not know is not one it guesses at.
    #[test]
    fn a_line_missing_what_its_type_needs_is_refused_by_name() {
        let bad_field = [
            r#"{}"#,
            r#"{"text":"hi"}"#,
            r#"{"type":1}"#,
            r#"{"type":"prompt"}"#,
            r#"{"type":"prompt","text":""}"#,
            r#"{"type":"prompt","text":7}"#,
            r#"{"type":"steer"}"#,
            r#"{"type":"steer","text":null}"#,
            r#"{"type":"prompt","type":"prompt","text":"hi"}"#,
            r#"{"type":"prompt","text":"a","text":"b"}"#,
            r#"{"type":"answer"}"#,
            r#"{"type":"answer","id":1}"#,
            r#"{"type":"answer","choices":[[0]]}"#,
            r#"{"type":"answer","id":"1","choices":[[0]]}"#,
            r#"{"type":"answer","id":-1,"choices":[[0]]}"#,
            r#"{"type":"answer","id":1.5,"choices":[[0]]}"#,
            r#"{"type":"answer","id":99999999999999999999,"choices":[[0]]}"#,
            r#"{"type":"answer","id":1,"choices":[0]}"#,
            r#"{"type":"answer","id":1,"choices":[["a"]]}"#,
            r#"{"type":"answer","id":1,"choices":[[-1]]}"#,
            r#"{"type":"answer","id":1,"choices":"0"}"#,
            r#"{"type":"answer","id":1,"choices":[[0]],"choices":[[0]]}"#,
        ];
        for line in bad_field {
            assert_eq!(parse(line), Err(Refused::BadField), "{line}");
        }
        for line in [
            r#"{"type":"Prompt","text":"hi"}"#,
            r#"{"type":"cancel"}"#,
            r#"{"type":""}"#,
        ] {
            assert_eq!(parse(line), Err(Refused::UnknownType), "{line}");
        }
    }

    /// A line is read to its end whatever is wrong with it, so the one after it is read as itself.
    /// Without that, one over-long line leaves the tail of itself to be read as the next.
    #[test]
    fn a_line_over_the_cap_is_dropped_and_the_next_is_read_whole() {
        let mut input = Cursor::new(
            b"{\"type\":\"stop\"}\r\n0123456789ABCDEF\n{\"type\":\"stop\"}\n\n\xff\xfe\n{\"type\":\"stop\"}"
                .to_vec(),
        );
        let mut read = Vec::new();
        while let Some(line) = next_line(&mut input, 16).expect("reads") {
            read.push(line);
        }
        assert_eq!(
            read,
            vec![
                // The carriage return a Windows sender leaves is not part of the line.
                Ok(r#"{"type":"stop"}"#.to_string()),
                // Exactly the cap fits; one past it does not.
                Ok("0123456789ABCDEF".to_string()),
                Ok(r#"{"type":"stop"}"#.to_string()),
                Ok(String::new()),
                Err(Refused::Unreadable),
                // The last line has no newline after it and is still a line.
                Ok(r#"{"type":"stop"}"#.to_string()),
            ]
        );

        let mut over = Cursor::new(b"0123456789ABCDEFG\n{\"type\":\"stop\"}\n".to_vec());
        assert_eq!(
            next_line(&mut over, 16).expect("reads"),
            Some(Err(Refused::Unreadable))
        );
        assert_eq!(
            next_line(&mut over, 16).expect("reads"),
            Some(Ok(r#"{"type":"stop"}"#.to_string()))
        );
        assert_eq!(next_line(&mut over, 16).expect("reads"), None);
    }

    /// Prompts start turns in the order they were sent, and a blank line starts none.
    #[test]
    fn prompts_start_turns_in_the_order_they_arrived() {
        let (channel, _) = channel(true);
        for line in [
            r#"{"type":"prompt","text":"first"}"#,
            "   ",
            r#"{"type":"prompt","text":"second"}"#,
        ] {
            deliver(&channel, line);
        }
        channel.close();
        assert_eq!(next(&channel).as_deref(), Some("first"));
        assert_eq!(next(&channel).as_deref(), Some("second"));
        assert_eq!(next(&channel), None);
    }

    /// A steer reaches the running turn only where a person vouched for the channel. Elsewhere it
    /// would be a reference the planner cannot read, which steers nothing, so it waits for a turn.
    #[test]
    fn a_steer_reaches_the_running_turn_only_on_a_vouched_channel() {
        let steer = r#"{"type":"steer","text":"look at b.txt"}"#;

        let (vouched, _) = channel(true);
        vouched.start();
        deliver(&vouched, steer);
        assert_eq!(vouched.steer().as_deref(), Some("look at b.txt"));
        assert_eq!(vouched.steer(), None, "taken once");

        let (unvouched, _) = channel(false);
        unvouched.start();
        deliver(&unvouched, steer);
        assert_eq!(
            unvouched.steer(),
            None,
            "words from a program were handed to the running turn as the person's"
        );
        unvouched.idle();
        unvouched.close();
        assert_eq!(next(&unvouched).as_deref(), Some("look at b.txt"));
    }

    /// A steer sent while no turn runs has no turn to reach, so it starts one, as a line typed at
    /// rest does in the interface that draws. Held for the next turn to take mid-run it would steer
    /// a turn it was not aimed at.
    #[test]
    fn a_steer_sent_while_no_turn_runs_starts_a_turn_of_its_own() {
        let (channel, _) = channel(true);
        deliver(&channel, r#"{"type":"steer","text":"no turn yet"}"#);
        assert_eq!(channel.steer(), None);
        channel.close();
        assert_eq!(next(&channel).as_deref(), Some("no turn yet"));
    }

    /// A steer the turn ended without taking is not lost: it starts a turn after the prompts
    /// already waiting, and the turn that starts first is not handed it. Handed to that turn it
    /// would be injected into a conversation it was not aimed at.
    #[test]
    fn a_steer_the_turn_ended_without_taking_starts_a_turn_of_its_own() {
        let (channel, _) = channel(true);
        channel.start();
        deliver(&channel, r#"{"type":"steer","text":"late"}"#);
        deliver(&channel, r#"{"type":"prompt","text":"queued"}"#);
        channel.idle();
        channel.close();

        assert_eq!(next(&channel).as_deref(), Some("queued"));
        assert_eq!(
            channel.steer(),
            None,
            "the steer was left to be taken by the turn after the one it was aimed at"
        );
        assert_eq!(next(&channel).as_deref(), Some("late"));
        assert_eq!(next(&channel), None);
    }

    /// A stop reaches the turn a prompt starts from the moment the prompt is taken, and so does a
    /// steer. With the turn registered a step later, a line sent right behind its prompt would find
    /// no turn and be dropped or held for another.
    #[test]
    fn a_stop_or_a_steer_right_behind_a_prompt_reaches_the_turn_that_prompt_starts() {
        let (channel, said) = channel(true);
        deliver(&channel, r#"{"type":"prompt","text":"go"}"#);
        let (words, cancel) = channel.next_prompt().expect("a turn starts");
        assert_eq!(words, "go");

        deliver(&channel, r#"{"type":"steer","text":"left"}"#);
        deliver(&channel, r#"{"type":"stop"}"#);
        assert!(cancel.is_cancelled(), "the stop found no turn to stop");
        assert_eq!(channel.steer().as_deref(), Some("left"));
        assert!(said.lines().is_empty(), "{:?}", said.lines());
    }

    /// A stop ends the turn that is running and nothing else: the prompts behind it still run, and
    /// a stop sent when nothing runs is not held for the next turn.
    #[test]
    fn a_stop_ends_the_running_turn_and_is_not_kept_for_the_next() {
        let (channel, said) = channel(false);
        let first = channel.start();
        deliver(&channel, r#"{"type":"prompt","text":"after"}"#);
        deliver(&channel, r#"{"type":"stop"}"#);
        assert!(first.is_cancelled());
        channel.idle();

        deliver(&channel, r#"{"type":"stop"}"#);
        let (words, second) = channel
            .next_prompt()
            .expect("the prompt behind the stop runs");
        assert_eq!(words, "after");
        assert!(
            !second.is_cancelled(),
            "a stop sent between turns stopped the next"
        );
        assert!(
            said.lines().is_empty(),
            "a stop with nothing to stop is not an error"
        );
    }

    /// A line that is not taken is said by number and by a fixed word, and nothing of the line is
    /// repeated: the sender is a program, and its words are not echoed into a stream another
    /// program reads.
    #[test]
    fn a_refused_line_is_said_by_number_and_cause_and_never_repeated() {
        let (channel, said) = channel(false);
        channel.deliver(
            4,
            Ok(r#"{"type":"prompt","text":""} SECRET-WORDS"#.to_string()),
        );
        channel.deliver(5, Ok(r#"{"type":"run","text":"SECRET-WORDS"}"#.to_string()));
        channel.deliver(6, Err(Refused::Unreadable));
        assert_eq!(
            said.lines(),
            vec![
                r#"{"schema":1,"event":"input_refused","line":4,"reason":"not_json"}"#,
                r#"{"schema":1,"event":"input_refused","line":5,"reason":"unknown_type"}"#,
                r#"{"schema":1,"event":"input_refused","line":6,"reason":"unreadable"}"#,
            ]
        );
    }

    /// An answer that fits the question is the answer, and one that does not is refused and leaves
    /// the question waiting. A planner told a person chose something nobody offered would have a
    /// choice that never existed in its context.
    #[test]
    fn an_answer_that_does_not_fit_the_question_is_refused_and_the_question_waits() {
        let (channel, said) = channel(false);
        let asked = std::thread::spawn({
            let channel = channel.clone();
            move || channel.ask(&asking(&[(3, false), (2, true)]))
        });
        // The question is on the stream before an answer can name it.
        while said.lines().is_empty() {
            std::thread::yield_now();
        }

        let wrong = [
            // Another question's id.
            r#"{"type":"answer","id":9,"choices":[[0],[0]]}"#,
            // One answer for two questions.
            r#"{"type":"answer","id":1,"choices":[[0]]}"#,
            // A choice the question does not have.
            r#"{"type":"answer","id":1,"choices":[[3],[0]]}"#,
            // Two picks where the question takes one.
            r#"{"type":"answer","id":1,"choices":[[0,1],[0]]}"#,
            // The same pick twice.
            r#"{"type":"answer","id":1,"choices":[[0],[1,1]]}"#,
        ];
        for line in wrong {
            deliver(&channel, line);
        }
        let refused = said
            .lines()
            .iter()
            .filter(|line| line.contains(r#""reason":"no_such_question""#))
            .count();
        assert_eq!(refused, wrong.len(), "{:?}", said.lines());

        deliver(
            &channel,
            r#"{"type":"answer","id":1,"choices":[[2],[1,0]]}"#,
        );
        assert_eq!(
            asked.join().expect("the question returned"),
            vec![Answer::Chosen(vec![2]), Answer::Chosen(vec![1, 0])]
        );
    }

    /// An empty pick is a declined question, and an answer with no question waiting is refused.
    #[test]
    fn an_empty_pick_declines_one_question_and_no_question_waiting_takes_no_answer() {
        let (channel, said) = channel(false);
        deliver(&channel, r#"{"type":"answer","id":1,"choices":[[0]]}"#);
        assert!(
            said.lines()[0].contains("no_such_question"),
            "{:?}",
            said.lines()
        );

        let asked = std::thread::spawn({
            let channel = channel.clone();
            move || channel.ask(&asking(&[(2, false), (2, false)]))
        });
        while said.lines().len() < 2 {
            std::thread::yield_now();
        }
        deliver(&channel, r#"{"type":"answer","id":1,"choices":[[],[1]]}"#);
        assert_eq!(
            asked.join().expect("the question returned"),
            vec![Answer::Declined, Answer::Chosen(vec![1])]
        );
        // The question is over, so the same id answers nothing now.
        deliver(&channel, r#"{"type":"answer","id":1,"choices":[[],[1]]}"#);
        assert!(said.lines().last().unwrap().contains("no_such_question"));
    }

    /// Each question has an id of its own, so an answer meant for the first cannot settle the
    /// second.
    #[test]
    fn a_question_has_an_id_of_its_own() {
        let (channel, said) = channel(false);
        for expected in 1..=2u64 {
            let asked = std::thread::spawn({
                let channel = channel.clone();
                move || channel.ask(&asking(&[(2, false)]))
            });
            while said.lines().len() < expected as usize {
                std::thread::yield_now();
            }
            assert!(
                said.lines()
                    .last()
                    .unwrap()
                    .contains(&format!(r#""id":{expected}"#)),
                "{:?}",
                said.lines()
            );
            deliver(
                &channel,
                &format!(r#"{{"type":"answer","id":{expected},"choices":[[1]]}}"#),
            );
            assert_eq!(
                asked.join().expect("returned"),
                vec![Answer::Chosen(vec![1])]
            );
        }
    }

    /// A question that nothing can answer is declined with no answers at all, which is the reply
    /// the confirmer's contract asks for: when the input has ended, and when the turn is stopped
    /// while the question waits.
    #[test]
    fn a_question_nothing_can_answer_is_declined_when_the_input_ends_or_the_turn_stops() {
        let (ended, _) = channel(false);
        let asked = std::thread::spawn({
            let channel = ended.clone();
            move || channel.ask(&asking(&[(2, false)]))
        });
        ended.close();
        assert_eq!(asked.join().expect("returned"), Vec::new());

        let (stopped, said) = channel(false);
        stopped.start();
        let asked = std::thread::spawn({
            let channel = stopped.clone();
            move || channel.ask(&asking(&[(2, false)]))
        });
        while said.lines().is_empty() {
            std::thread::yield_now();
        }
        deliver(&stopped, r#"{"type":"stop"}"#);
        assert_eq!(asked.join().expect("returned"), Vec::new());
    }

    /// An answer sent and the input closed straight after, as a script piping a file does, is
    /// still the answer: the line was read before the end of the input was.
    #[test]
    fn an_answer_followed_by_the_end_of_the_input_is_still_the_answer() {
        let (channel, said) = channel(false);
        let asked = std::thread::spawn({
            let channel = channel.clone();
            move || channel.ask(&asking(&[(2, false)]))
        });
        while said.lines().is_empty() {
            std::thread::yield_now();
        }
        channel.read(Cursor::new(
            b"{\"type\":\"answer\",\"id\":1,\"choices\":[[1]]}\n".to_vec(),
        ));
        assert_eq!(
            asked.join().expect("returned"),
            vec![Answer::Chosen(vec![1])]
        );
    }

    /// The words waiting for a turn are bounded, so a sender that writes faster than turns run, or
    /// a file piped in whole, cannot fill memory as a pipe of CLI-4's cap cannot. What is waiting
    /// counts prompts and steers together, and room comes back as a turn takes words.
    #[test]
    fn the_words_waiting_for_a_turn_are_bounded_and_room_comes_back_as_they_are_taken() {
        let (channel, said) = channel_holding(true, 10);
        deliver(&channel, r#"{"type":"prompt","text":"aaaaaa"}"#);
        // Four more bytes fit and five do not, whichever queue they would wait in.
        deliver(&channel, r#"{"type":"steer","text":"bbbbb"}"#);
        deliver(&channel, r#"{"type":"prompt","text":"bbbbb"}"#);
        assert_eq!(
            said.lines(),
            vec![
                r#"{"schema":1,"event":"input_refused","line":1,"reason":"too_much_waiting"}"#,
                r#"{"schema":1,"event":"input_refused","line":1,"reason":"too_much_waiting"}"#,
            ]
        );

        assert_eq!(next(&channel).as_deref(), Some("aaaaaa"));
        deliver(&channel, r#"{"type":"steer","text":"cccccccc"}"#);
        assert_eq!(channel.steer().as_deref(), Some("cccccccc"));
        deliver(&channel, r#"{"type":"prompt","text":"dddddddddd"}"#);
        channel.close();
        assert_eq!(next(&channel).as_deref(), Some("dddddddddd"));
        assert_eq!(
            said.lines().len(),
            2,
            "room did not come back: {:?}",
            said.lines()
        );
    }

    /// The thread that reads takes lines as they come and numbers them as the sender wrote them,
    /// blank lines included, which is what a sender counting its own writes will see.
    #[test]
    fn the_reader_numbers_lines_as_they_were_written() {
        let (channel, said) = channel(true);
        channel.read(
            Cursor::new(
                b"\n{\"type\":\"prompt\",\"text\":\"one\"}\nnot json\n{\"type\":\"prompt\",\"text\":\"two\"}\n"
                    .to_vec(),
            ),
        );
        assert_eq!(next(&channel).as_deref(), Some("one"));
        assert_eq!(next(&channel).as_deref(), Some("two"));
        assert_eq!(next(&channel), None);
        assert_eq!(
            said.lines(),
            vec![r#"{"schema":1,"event":"input_refused","line":3,"reason":"not_json"}"#]
        );
    }

    /// A question is answered once. A second line naming it, sent before the turn has woken to read
    /// the first, finds nothing waiting and is refused, so the planner is told the pick that was
    /// sent first and not whichever arrived last.
    #[test]
    fn a_second_answer_to_a_question_does_not_replace_the_first() {
        let (channel, said) = channel(false);
        let asked = std::thread::spawn({
            let channel = channel.clone();
            move || channel.ask(&asking(&[(3, false)]))
        });
        while said.lines().is_empty() {
            std::thread::yield_now();
        }
        deliver(&channel, r#"{"type":"answer","id":1,"choices":[[0]]}"#);
        deliver(&channel, r#"{"type":"answer","id":1,"choices":[[2]]}"#);
        assert_eq!(
            asked.join().expect("returned"),
            vec![Answer::Chosen(vec![0])]
        );
        assert!(
            said.lines().last().unwrap().contains("no_such_question"),
            "{:?}",
            said.lines()
        );
    }

    /// A reader that dies, however it dies, ends the input. A run waiting on a line from a thread
    /// that panicked would otherwise wait for ever.
    #[test]
    fn a_reader_that_panics_ends_the_input_and_leaves_nothing_waiting() {
        struct Panics;
        impl std::io::Read for Panics {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                panic!("the reader failed");
            }
        }
        let (channel, _) = channel(true);
        channel.read(std::io::BufReader::new(Panics));
        assert_eq!(next(&channel), None);
    }

    /// A read that a signal interrupted is read again. Taken for the end of the input it would end
    /// the run while the sender is still there.
    #[test]
    fn an_interrupted_read_is_read_again_and_not_taken_for_the_end() {
        struct Interrupted {
            first: bool,
            rest: Cursor<Vec<u8>>,
        }
        impl std::io::Read for Interrupted {
            fn read(&mut self, into: &mut [u8]) -> std::io::Result<usize> {
                match std::mem::replace(&mut self.first, false) {
                    true => Err(std::io::ErrorKind::Interrupted.into()),
                    false => self.rest.read(into),
                }
            }
        }
        let mut input = std::io::BufReader::new(Interrupted {
            first: true,
            rest: Cursor::new(b"{\"type\":\"stop\"}\n".to_vec()),
        });
        assert_eq!(
            next_line(&mut input, 64).expect("reads"),
            Some(Ok(r#"{"type":"stop"}"#.to_string()))
        );
    }

    /// A pick is as wide as the questions the planner may put and the choices it may write, and
    /// not wider: what is past that is a field of another kind, so a line cannot cost memory in
    /// proportion to a list it names. A pick at the bound is taken whole.
    #[test]
    fn a_pick_past_the_bounds_is_a_field_of_another_kind() {
        let at_most = format!(
            r#"{{"type":"answer","id":1,"choices":[{}]}}"#,
            vec!["[0]"; MOST_QUESTIONS].join(",")
        );
        match parse(&at_most) {
            Ok(Line::Answer { choices, .. }) => assert_eq!(choices.len(), MOST_QUESTIONS),
            other => panic!("{other:?}"),
        }
        let wide = vec!["0"; MOST_POSITIONS].join(",");
        match parse(&format!(
            r#"{{"type":"answer","id":1,"choices":[[{wide}]]}}"#
        )) {
            Ok(Line::Answer { choices, .. }) => assert_eq!(choices[0].len(), MOST_POSITIONS),
            other => panic!("{other:?}"),
        }

        let too_many = format!(
            r#"{{"type":"answer","id":1,"choices":[{}]}}"#,
            vec!["[0]"; MOST_QUESTIONS + 1].join(",")
        );
        assert_eq!(parse(&too_many), Err(Refused::BadField));
        let too_wide = vec!["0"; MOST_POSITIONS + 1].join(",");
        assert_eq!(
            parse(&format!(
                r#"{{"type":"answer","id":1,"choices":[[{too_wide}]]}}"#
            )),
            Err(Refused::BadField)
        );
        // Past the bound is still read as JSON, so what follows it is read as itself.
        assert_eq!(
            parse(&format!(r#"{{"choices":[[{too_wide}]],"type":"stop"}}"#)),
            Ok(Line::Stop)
        );
        assert_eq!(
            parse(&format!(r#"{{"choices":[[{too_wide}],"type":"stop"}}"#)),
            Err(Refused::NotJson)
        );
    }

    /// A field the envelope does not name is read past whatever its size, and a line of nothing
    /// else is taken for what it says.
    #[test]
    fn a_large_field_the_envelope_does_not_name_is_read_past() {
        let list = vec!["0"; 200_000].join(",");
        let line = format!(r#"{{"x":[{list}],"type":"prompt","text":"hi","y":{{"z":[{list}]}}}}"#);
        assert_eq!(parse(&line), Ok(Line::Prompt("hi".into())));
    }
}
