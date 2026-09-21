//! Reading the terminal, and telling typing apart from what a program typed.
//!
//! A terminal delivers one byte stream and says nothing about who wrote it. A person at a keyboard
//! and a program holding the other end of the pty arrive the same way, byte for byte, so every
//! reader above this one was answering a question about provenance it had no way to ask. This is
//! the one place that asks it, and it asks by timing, which is the only evidence there is.
//!
//! What a program writing a command line produces is a burst: every character of it available in
//! the same instant, because it was written in one call. A person cannot do that. So a run of keys
//! that were all waiting together is not typing, and this module reports such a run as
//! [`Input::TypedIn`] rather than as keys. [`crate::app::handle_typed_in`] then puts it in the box
//! where a person can read it, and INPUT-35 is what stops it being sent from there.
//!
//! Two things follow from that, and the second is the one that matters:
//!
//! - A burst never sends. Its own trailing newline becomes a newline in the box, and the mark it
//!   leaves on the line means a later Enter does not send it either, however that Enter arrived.
//! - A burst never answers. Every prompt in this crate answers a key and discards everything else,
//!   so a run cannot press `y` at a trust question or `a` at a run prompt. Those two questions do
//!   not take a single key at all (PROMPT-11), which is the stronger half of the same point.
//!
//! A write does not always arrive as one read, so a run is not always the whole of what was
//! written. The line VS Code sends crosses the extension host and the pty host on its way to the
//! terminal and can be split anywhere, which leaves a piece of it arriving alone; one character
//! alone is a keystroke by [`characters`], and the piece that arrives alone most often is the
//! carriage return that ended the line. So the run is not the unit. [`CONTINUATION`] is: what
//! follows a run closely enough is the rest of the same write, and is carried or dropped with it
//! rather than read as a key.
//!
//! **What this does not buy, measured rather than assumed.** The test is a gap, so a writer that
//! leaves a bigger one defeats it: past [`CONTINUATION`] a character delivered on its own is a
//! keystroke, because a person typing is exactly that and there is nothing to tell them apart. What
//! the timing buys is the write that arrives at once or in pieces, which is the shape every writer
//! doing this today has and the one that was reported. The rest is bought by the two clauses above,
//! which do not rest on timing at all. The guarantee that keeps untrusted content out of the
//! driver's decisions is none of this, and does not rest on it.
//!
//! Every reader in this crate goes through [`read`] and [`poll`] rather than calling crossterm
//! directly, because a run can only be recognised where the whole run is visible. A prompt that
//! read the terminal itself would see the first key of a burst with nothing behind it and answer
//! on it, which is the bug this exists for.

use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use std::collections::VecDeque;
use std::io;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// One thing read from the terminal, and whether a person produced it.
///
/// The distinction is not a guess. A person pasting from the clipboard arrives inside the markers
/// bracketed paste puts round it, which crossterm parses and reports as [`TermEvent::Paste`] before
/// this module sees a key at all. A program writing into the pty sends bare bytes, so its words
/// arrive as keys and are recognised here by their timing. The two are produced at two different
/// points in [`read`], and nothing downstream has to tell them apart by looking at the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// What the terminal reported as itself: a key press, a bracketed paste, a mouse report, a
    /// resize. A paste arriving this way came off the clipboard, which a person put there.
    Terminal(TermEvent),
    /// Words another program typed into the terminal, recognised as a run rather than as typing.
    ///
    /// Carried and shown like any other text. What it may not do is send: see INPUT-35.
    TypedIn(String),
}

impl Input {
    /// The key press this is, for a reader that answers keys and discards everything else.
    ///
    /// Most callers are prompts, which take one key and ignore the rest, and this spares each of
    /// them a nested match on an event they would drop anyway.
    pub fn key(&self) -> Option<KeyEvent> {
        match self {
            Input::Terminal(TermEvent::Key(key)) => Some(*key),
            _ => None,
        }
    }

    /// Whether this is words another program typed.
    fn is_typed_in(&self) -> bool {
        matches!(self, Input::TypedIn(_))
    }
}

/// The most keys taken into one run.
///
/// The gathering loop ends when the terminal has nothing more waiting, which is the ordinary way
/// out, so this bounds only the pathological case: a writer streaming without pause, which would
/// otherwise hold the loop for as long as it kept writing and leave the screen unredrawn. Past the
/// cap the rest of the stream is left where it is and becomes the next run, so nothing is dropped
/// and a very long burst arrives as a few pastes rather than one.
const MOST_IN_A_RUN: usize = 8192;

/// How long after words another program typed a key still belongs to them.
///
/// A write does not always arrive as one read. `terminal.sendText` in VS Code crosses the extension
/// host and the pty host before it reaches the terminal, and the line can land in pieces, which
/// leaves the carriage return at the end of it arriving alone a few milliseconds behind the rest.
/// One key carrying one character is a keystroke by [`characters`], so that return was read as a
/// person pressing Enter and answered the question on screen: the defect in #403 that survived
/// recognising the run.
///
/// So a key this close behind a run belongs to the run. Well above the gap a fragmented write
/// leaves, and well below the time a person needs to see a question that has just appeared and
/// decide about it.
const CONTINUATION: Duration = Duration::from_millis(100);

/// Events taken from the terminal and not yet handed out.
///
/// A run has to be read in full before it can be classified, and a run that turns out to be
/// typing has to be handed out key by key, so the reader buffers. Shared rather than per thread
/// because the queue must be the same queue whoever reads: the terminal is one stream, and a
/// second queue would hand out events in an order nothing wrote them in.
fn pending() -> &'static Mutex<VecDeque<Input>> {
    static PENDING: OnceLock<Mutex<VecDeque<Input>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// When words another program typed were last handed out, if the stream has not moved on since.
///
/// What [`CONTINUATION`] is measured from, so a write that arrived in pieces is recognised as one
/// run rather than as a run and then a keystroke. Cleared as soon as a real keypress is delivered,
/// because the next thing after that is a person's again.
fn typed_in_at() -> &'static Mutex<Option<Instant>> {
    static AT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    AT.get_or_init(|| Mutex::new(None))
}

/// Whether an event is waiting, here or at the terminal.
///
/// Answers for the buffer first, because an event already taken is an event waiting: a caller
/// asking this to decide whether to redraw or to keep reading would otherwise be told the terminal
/// is quiet while a gathered run sits unread.
pub fn poll(timeout: Duration) -> io::Result<bool> {
    if !pending().lock().expect("input queue").is_empty() {
        return Ok(true);
    }
    event::poll(timeout)
}

/// The next event, blocking until there is one.
///
/// Hands out what is buffered before reading the terminal again, so the order the terminal wrote
/// events in is the order callers see them.
pub fn read() -> io::Result<Input> {
    // A loop because a run can resolve to nothing: a key a program wrote that carries no text is
    // dropped rather than delivered, and the caller is still waiting for something.
    loop {
        if let Some(event) = pending().lock().expect("input queue").pop_front() {
            return Ok(event);
        }
        gather()?;
    }
}

/// Read one run from the terminal and put what it resolves to on the queue.
///
/// May add nothing, when the run was part of what another program wrote and carried no text.
fn gather() -> io::Result<()> {
    let first = event::read()?;
    // Only keys are gathered. A mouse event, a resize or a focus change says nothing about who
    // typed and is delivered as it arrived. A paste is here too, and this is the one place a
    // person's paste is told from a program's: the terminal marked this one, so it came off the
    // clipboard and goes through as itself.
    let TermEvent::Key(first) = first else {
        *typed_in_at().lock().expect("typed in at") = None;
        pending()
            .lock()
            .expect("input queue")
            .push_back(Input::Terminal(first));
        return Ok(());
    };

    let mut run = vec![first];
    while run.len() < MOST_IN_A_RUN && event::poll(Duration::ZERO)? {
        match event::read()? {
            TermEvent::Key(key) => run.push(key),
            // Ends the run and keeps its place. A bracketed paste in the middle of one is already
            // a paste and is passed through as itself rather than folded into the run, and the
            // queue is drained before the terminal is read again, so it stays where it was.
            other => {
                pending()
                    .lock()
                    .expect("input queue")
                    .push_back(Input::Terminal(other));
                break;
            }
        }
    }

    // Held across the resolving so the window cannot move between the asking and the recording.
    let mut mark = typed_in_at().lock().expect("typed in at");
    let resolved = resolve(run, mark.is_some_and(|at| at.elapsed() < CONTINUATION));
    if resolved.iter().any(|taken| taken.is_typed_in()) {
        // Refreshed rather than left where it was, so a line arriving in many pieces stays one run
        // for as long as the pieces keep coming.
        *mark = Some(Instant::now());
    } else if !resolved.is_empty() {
        *mark = None;
    }
    drop(mark);

    let mut queue = pending().lock().expect("input queue");
    for event in resolved {
        queue.push_back(event);
    }
    Ok(())
}

/// What a run of keys that arrived together is: one paste, or the keys themselves.
///
/// Separated from the reading so it can be tested without a terminal, which is the whole of the
/// decision this module exists to make.
///
/// Two or more characters waiting together is the test. One character is a keystroke, however it
/// got there, because that is what a person pressing a key looks like and there is nothing to tell
/// the two apart. Two is not: it asks for a keyboard held down long enough to fill the buffer
/// between one read and the next, and a program writing a command line clears it in one write.
///
/// A run that carries no characters at all is not a paste whatever its length. Key autorepeat
/// behind a slow redraw looks exactly like a burst of `Down`, and folding that into a paste of
/// nothing would eat the scrolling.
///
/// Everything in a run that is not text is dropped when the run is a paste: a chord, an Escape, an
/// interrupt. They are dropped rather than delivered because the run is text, and an instruction
/// inside text is one nobody gave. Delivering them is what lets a burst say `ctrl-c` to a running
/// turn or Escape to a prompt, which is the same defect as the one this fixes.
pub(crate) fn resolve(run: Vec<KeyEvent>, continuing: bool) -> Vec<Input> {
    // Behind words another program typed, and close enough behind them to be the rest of the same
    // write. Then the count does not apply: the piece a fragmented write leaves on its own is one
    // character, and in the reported case it is the carriage return that ended the line.
    if continuing {
        let carried: String = run.iter().filter_map(text_of).collect();
        // Nothing in it spells anything, so there is nothing to carry. Dropped for the reason a
        // chord inside a burst is: a key a program wrote decides nothing.
        if carried.is_empty() {
            return Vec::new();
        }
        return vec![Input::TypedIn(carried)];
    }

    if characters(&run) < 2 {
        return run
            .into_iter()
            .map(|key| Input::Terminal(TermEvent::Key(key)))
            .collect();
    }

    // Non-empty by the count above: two keys carrying text is what put the run on this branch.
    // [`Input::TypedIn`] rather than a paste, because a paste is what a person did with a clipboard
    // and this is what a program did with a write, and the box may show one and send the other.
    vec![Input::TypedIn(run.iter().filter_map(text_of).collect())]
}

/// How many keys in a run a person would have had to type a character with.
///
/// Presses only. Asking for disambiguated keys asks for releases too, so one character typed can
/// be two events, and counting a release would make every keystroke a burst of two. A repeat is
/// not counted either: a held key is one press the terminal is repeating, and counting its repeats
/// would turn holding a letter down into a paste.
fn characters(run: &[KeyEvent]) -> usize {
    run.iter()
        .filter(|key| key.kind == KeyEventKind::Press && text_of(key).is_some())
        .count()
}

/// The text one key stands for, or `None` for a key that stands for none.
///
/// A character with no modifier but Shift is itself. Enter is a newline and Tab a tab, because a
/// command line written into a terminal carries both and dropping them would join two lines into
/// one. A release carries nothing: the character was already taken from the press.
///
/// Every other key, Ctrl-anything included, stands for no text. `Ctrl-j` is the newline some
/// terminals send for Shift-Enter (INPUT-1) and would qualify on that reading, but a run is text
/// here and a chord in it was not typed by anybody, so it is left out with the rest.
fn text_of(key: &KeyEvent) -> Option<char> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let bare = (key.modifiers - KeyModifiers::SHIFT).is_empty();
    match key.code {
        KeyCode::Char(c) if bare => Some(c),
        KeyCode::Enter if bare => Some('\n'),
        KeyCode::Tab if bare => Some('\t'),
        _ => None,
    }
}

/// The keys a line of text arrives as, for a test that needs a run to hand to [`resolve`].
///
/// Here rather than in each test module because two of them are at call sites, and a run spelled
/// differently in each place would be two tests about two things.
#[cfg(test)]
pub(crate) fn run_spelling(line: &str) -> Vec<KeyEvent> {
    line.chars()
        .map(|c| match c {
            '\r' | '\n' => KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            c => KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(text: &str) -> Vec<KeyEvent> {
        text.chars().map(|c| key(KeyCode::Char(c))).collect()
    }

    fn pasted(run: Vec<KeyEvent>) -> Option<String> {
        match resolve(run, false).as_slice() {
            [Input::TypedIn(text)] => Some(text.clone()),
            _ => None,
        }
    }

    /// The reported bug. A program that writes a command line into the terminal writes every
    /// character of it at once, so the whole line is waiting together, and a reader that took the
    /// keys one at a time would answer the question on screen with whichever letter of the path
    /// came first and send the rest.
    #[test]
    fn a_line_that_arrived_all_at_once_is_a_paste() {
        let mut run = typed("source /tmp/x/.venv/bin/activate");
        run.push(key(KeyCode::Enter));
        assert_eq!(
            pasted(run).as_deref(),
            Some("source /tmp/x/.venv/bin/activate\n")
        );
    }

    /// The half that decides whether a question can be answered by a program. Every prompt in
    /// this crate discards an event that is not a key, so a run resolving to one paste cannot
    /// reach `answer_for` at all, and a run resolving to keys can.
    #[test]
    fn a_run_carrying_two_characters_reaches_nothing_that_reads_keys() {
        assert!(
            resolve(typed("no"), false)
                .iter()
                .all(|taken| !matches!(taken, Input::Terminal(TermEvent::Key(_))))
        );
    }

    /// One key is a keystroke whatever else is true of it, because a person pressing a key and a
    /// program writing one byte are indistinguishable and the person is the one who has to be
    /// able to work. Every prompt is answered by exactly this.
    #[test]
    fn one_character_on_its_own_stays_a_key() {
        assert_eq!(
            resolve(typed("y"), false),
            vec![Input::Terminal(TermEvent::Key(key(KeyCode::Char('y'))))]
        );
    }

    /// A held key repeats faster than a slow frame is drawn, so its presses queue up and arrive
    /// together. Reading that as pasted text would eat the scrolling of every long transcript,
    /// which is a working feature broken to fix an injection.
    #[test]
    fn a_run_of_keys_carrying_no_text_is_still_keys() {
        let run = vec![key(KeyCode::Down); 5];
        assert_eq!(resolve(run, false).len(), 5);
    }

    /// The other half of a run that is text: what is in it that is not. An interrupt inside a
    /// burst would stop the turn in flight, and an Escape would discard a half-typed line, neither
    /// of which anybody asked for.
    #[test]
    fn a_chord_inside_a_burst_is_dropped_rather_than_obeyed() {
        let mut run = typed("de");
        run.push(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        run.push(key(KeyCode::Esc));
        run.extend(typed("f"));
        assert_eq!(pasted(run).as_deref(), Some("def"));
    }

    /// Asking for disambiguated keys asks for releases as well, so the events for one typed
    /// character are a press and a release. Counting the release would make every single
    /// keystroke a two-event burst, and every prompt in the program would stop answering.
    #[test]
    fn a_press_and_its_release_are_one_character_and_not_a_burst() {
        let mut press = key(KeyCode::Char('y'));
        press.kind = KeyEventKind::Press;
        let mut release = key(KeyCode::Char('y'));
        release.kind = KeyEventKind::Release;
        assert_eq!(
            resolve(vec![press, release], false),
            vec![
                Input::Terminal(TermEvent::Key(press)),
                Input::Terminal(TermEvent::Key(release)),
            ]
        );
    }

    /// A key held down reports a press and then repeats. They are one character the terminal is
    /// repeating, not several typed together, and reading them as a paste would turn holding a
    /// letter down into text in the box instead of a repeated keystroke.
    #[test]
    fn a_repeat_does_not_make_a_press_into_a_burst() {
        let mut press = key(KeyCode::Char('j'));
        press.kind = KeyEventKind::Press;
        let mut repeat = key(KeyCode::Char('j'));
        repeat.kind = KeyEventKind::Repeat;
        assert_eq!(resolve(vec![press, repeat, repeat], false).len(), 3);
    }

    /// The half of the reported case that survived recognising the run. VS Code's write crosses two
    /// process boundaries before it reaches the terminal, so the line can arrive in pieces and the
    /// carriage return that ends it lands on its own. Alone it is one character, which is a
    /// keystroke by every test this module can make, and it answered the question on screen.
    #[test]
    fn the_return_a_fragmented_write_leaves_on_its_own_is_not_a_keypress() {
        assert_eq!(
            resolve(vec![key(KeyCode::Enter)], true),
            vec![Input::TypedIn("\n".to_string())],
            "the return that ended a written line was read as a person pressing Enter"
        );
    }

    /// The same for a letter, which is what a write split mid-word leaves. It is the `n` of `.venv`
    /// that answered the trust question in the report, and one letter alone is what a split can
    /// always produce however the rest of the line was buffered.
    #[test]
    fn a_letter_a_fragmented_write_leaves_on_its_own_is_not_a_keypress() {
        assert_eq!(
            resolve(typed("n"), true),
            vec![Input::TypedIn("n".to_string())]
        );
    }

    /// A piece carrying nothing that spells anything has nothing to add to the line, and obeying it
    /// is what let a written Escape or interrupt reach a prompt. Dropped for the reason a chord
    /// inside a burst is.
    #[test]
    fn a_continuation_that_spells_nothing_is_dropped() {
        assert!(resolve(vec![key(KeyCode::Esc)], true).is_empty());
        assert!(
            resolve(
                vec![KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)],
                true
            )
            .is_empty()
        );
    }

    /// The limit of it, said out loud. Past the window a lone character is a keystroke, because that
    /// is what a person typing is, and a prompt that stopped answering one would stop working.
    #[test]
    fn past_the_window_a_lone_character_is_a_keypress_again() {
        assert_eq!(
            resolve(typed("y"), false),
            vec![Input::Terminal(TermEvent::Key(key(KeyCode::Char('y'))))]
        );
    }

    /// A command line written into a terminal carries the newlines its author wrote, and a shell
    /// heredoc or a `for` loop is several lines. Dropping them would join the lines into one, so
    /// what lands in the box would not be what was written.
    #[test]
    fn a_burst_keeps_the_newlines_and_tabs_it_carried() {
        let mut run = typed("a");
        run.push(key(KeyCode::Enter));
        run.push(key(KeyCode::Tab));
        run.extend(typed("b"));
        assert_eq!(pasted(run).as_deref(), Some("a\n\tb"));
    }
}
