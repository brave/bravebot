//! A notice written to the terminal when a turn ends or a question is waiting for the person, so
//! one who switched windows learns it without looking again ([VIEW-31](../../../docs/specs/terminal-transcript.md#VIEW-31)).
//!
//! The notice is chosen from what the interface itself saw: the change of what the session is
//! doing, which the title already tracks, and whether the terminal reported being left, which
//! [`crate::input`] records. The text of each kind is fixed here, so nothing a turn produced, a
//! prompt asked about or a file named reaches the escape sequence, and nothing untrusted is
//! branched on.

use std::io::{self, Write};
use std::sync::{Mutex, PoisonError};

use bravebot_i18n::t;

use crate::title::State;

/// What the settings' `notificationMethod` word asks for. A word that names none of them is the
/// default, [`Method::Auto`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    /// OSC 9 where the terminal is known to read it, and the bell where it is not.
    Auto,
    Osc9,
    Bel,
    Off,
}

impl Method {
    fn named(word: Option<&str>) -> Self {
        match word {
            Some("osc9") => Self::Osc9,
            Some("bel") => Self::Bel,
            Some("off") => Self::Off,
            _ => Self::Auto,
        }
    }
}

/// What the settings' `notificationCondition` word asks for. A word that names neither is the
/// default, [`Condition::Unfocused`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Condition {
    Unfocused,
    Always,
}

impl Condition {
    fn named(word: Option<&str>) -> Self {
        match word {
            Some("always") => Self::Always,
            _ => Self::Unfocused,
        }
    }
}

/// What the notice is about. A closed set with no payload: the text of each is below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    TurnFinished,
    Waiting,
}

/// The terminals whose `TERM_PROGRAM` is known to show an OSC 9 sequence as a desktop notification.
const READS_OSC9: &[&str] = &["iTerm.app", "WezTerm", "ghostty"];

/// What is written once a [`Method`] has been settled: `auto` and `off` have no bytes of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wire {
    Osc9,
    Bel,
}

/// The notice for `kind` as `wire`.
///
/// The sentence is the catalog's own, one per kind with no argument, so what an OSC 9 sequence
/// carries is a message the program wrote.
fn bytes(wire: Wire, kind: Kind) -> String {
    match wire {
        Wire::Bel => "\x07".to_string(),
        Wire::Osc9 => {
            let words = match kind {
                Kind::TurnFinished => t!(notice_turn_finished),
                Kind::Waiting => t!(notice_waiting),
            };
            format!("\x1b]9;{}\x07", crate::render::printable(words))
        }
    }
}

/// One run's worth of notification state, kept between frames so a state the loop reports on every
/// frame is noticed once.
#[derive(Debug)]
struct Notifier {
    method: Method,
    condition: Condition,
    /// Whether the terminal named by `TERM_PROGRAM` is known to read OSC 9.
    reads_osc9: bool,
    last: State,
}

impl Notifier {
    const fn new() -> Self {
        Self {
            method: Method::Auto,
            condition: Condition::Unfocused,
            reads_osc9: false,
            last: State::Ready,
        }
    }

    /// Write the notice a change to `state` calls for, if the settings and the focus allow one.
    ///
    /// A question that opens is a notice, and so is a turn or a command that goes from working to
    /// ready. Everything else, the same state again included, writes nothing. `in_front` is whether
    /// the terminal is believed to be in front of the person, so a terminal that reports no change
    /// of focus is never written to under [`Condition::Unfocused`].
    fn state<W: Write>(&mut self, out: &mut W, state: State, in_front: bool) -> io::Result<()> {
        let before = std::mem::replace(&mut self.last, state);
        let kind = match (before, state) {
            (before, State::Waiting) if before != State::Waiting => Kind::Waiting,
            (State::Working | State::Waiting, State::Ready) => Kind::TurnFinished,
            _ => return Ok(()),
        };
        if self.condition == Condition::Unfocused && in_front {
            return Ok(());
        }
        let wire = match self.method {
            Method::Off => return Ok(()),
            Method::Osc9 => Wire::Osc9,
            Method::Bel => Wire::Bel,
            Method::Auto if self.reads_osc9 => Wire::Osc9,
            Method::Auto => Wire::Bel,
        };
        out.write_all(bytes(wire, kind).as_bytes())?;
        out.flush()
    }
}

/// The run's notifier. One per process, because one process owns the terminal it writes to.
static NOTIFIER: Mutex<Notifier> = Mutex::new(Notifier::new());

fn notifier() -> std::sync::MutexGuard<'static, Notifier> {
    NOTIFIER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Take what the settings say for this run, and what the terminal is.
pub(crate) fn adopt(method: Option<&str>, condition: Option<&str>, term_program: Option<&str>) {
    let mut notifier = notifier();
    notifier.method = Method::named(method);
    notifier.condition = Condition::named(condition);
    notifier.reads_osc9 = term_program.is_some_and(|program| READS_OSC9.contains(&program));
}

/// [`Notifier::state`] for this run, with the focus [`crate::input`] last saw reported.
pub(crate) fn state<W: Write>(out: &mut W, state: State) -> io::Result<()> {
    notifier().state(out, state, crate::input::terminal_is_in_front())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn left(method: Method) -> Notifier {
        Notifier {
            method,
            ..Notifier::new()
        }
    }

    /// What `notifier` writes for `state` while the terminal is not in front of the person.
    fn noticed(notifier: &mut Notifier, state: State) -> String {
        written(notifier, state, false)
    }

    fn written(notifier: &mut Notifier, state: State, in_front: bool) -> String {
        let mut out = Vec::new();
        notifier.state(&mut out, state, in_front).expect("write");
        String::from_utf8(out).expect("utf-8")
    }

    /// The person who walked off during a turn is told it ended, once: the loop reports the state on
    /// every frame, and a notice per frame would ring until they came back.
    #[test]
    fn a_turn_that_ends_with_the_terminal_left_is_noticed_once() {
        let mut notifier = left(Method::Osc9);
        assert_eq!(noticed(&mut notifier, State::Working), "");
        assert_eq!(
            noticed(&mut notifier, State::Ready),
            "\x1b]9;bravebot: turn finished\x07"
        );
        assert_eq!(noticed(&mut notifier, State::Ready), "");
    }

    /// A turn blocked on a question is the one a person who looked away cannot tell from a long one,
    /// and a turn that then ends is a second notice.
    #[test]
    fn a_question_that_opens_with_the_terminal_left_is_noticed_once() {
        let mut notifier = left(Method::Osc9);
        noticed(&mut notifier, State::Working);
        let waiting = "\x1b]9;bravebot: waiting for an answer\x07";
        assert_eq!(noticed(&mut notifier, State::Waiting), waiting);
        assert_eq!(noticed(&mut notifier, State::Waiting), "");
        assert_eq!(noticed(&mut notifier, State::Working), "");
        assert_eq!(noticed(&mut notifier, State::Waiting), waiting);
    }

    /// Looking at the terminal is the notice, so a terminal in front of the person, and one that
    /// never reports focus and so is taken to be in front, is not written to; `always` writes
    /// regardless.
    #[test]
    fn a_focused_terminal_is_noticed_only_where_the_condition_is_always() {
        let mut notifier = left(Method::Osc9);
        written(&mut notifier, State::Working, true);
        assert_eq!(written(&mut notifier, State::Ready, true), "");

        notifier.condition = Condition::Always;
        written(&mut notifier, State::Working, true);
        assert_eq!(
            written(&mut notifier, State::Ready, true),
            "\x1b]9;bravebot: turn finished\x07"
        );
    }

    /// A question left open when the turn stops, as a refusal that ends it does, still ends the turn
    /// for a person who is not looking.
    #[test]
    fn a_turn_that_ends_while_a_question_is_open_is_noticed() {
        let mut notifier = left(Method::Osc9);
        noticed(&mut notifier, State::Working);
        noticed(&mut notifier, State::Waiting);
        assert_eq!(
            noticed(&mut notifier, State::Ready),
            "\x1b]9;bravebot: turn finished\x07"
        );
    }

    /// `off` is for somebody whose terminal turns the sequence into something they do not want.
    #[test]
    fn off_writes_nothing() {
        let mut notifier = Notifier {
            condition: Condition::Always,
            ..left(Method::Off)
        };
        noticed(&mut notifier, State::Working);
        assert_eq!(noticed(&mut notifier, State::Waiting), "");
        assert_eq!(noticed(&mut notifier, State::Ready), "");
    }

    /// A session that starts, a question that was answered and a command that finished before
    /// anything ran each change the state without ending a turn.
    #[test]
    fn only_a_turn_ending_or_a_question_opening_is_noticed() {
        let mut notifier = left(Method::Bel);
        assert_eq!(noticed(&mut notifier, State::Ready), "");
        assert_eq!(noticed(&mut notifier, State::Working), "");
        assert_eq!(noticed(&mut notifier, State::Waiting), "\x07");
        assert_eq!(noticed(&mut notifier, State::Working), "");
        assert_eq!(noticed(&mut notifier, State::Ready), "\x07");
    }

    /// `auto` is OSC 9 for a terminal known to read it and the bell for any other, which would show
    /// an unread sequence as stray text or drop it without a sound.
    #[test]
    fn auto_chooses_osc9_only_for_a_terminal_known_to_read_it() {
        let mut known = Notifier {
            reads_osc9: true,
            ..left(Method::Auto)
        };
        noticed(&mut known, State::Working);
        assert!(noticed(&mut known, State::Ready).starts_with("\x1b]9;"));

        let mut unknown = left(Method::Auto);
        noticed(&mut unknown, State::Working);
        assert_eq!(noticed(&mut unknown, State::Ready), "\x07");
    }

    /// A word that names no method or condition is the default for it, not a refusal to start and
    /// not a silent switch-off.
    #[test]
    fn a_word_that_names_nothing_is_the_default() {
        assert_eq!(Method::named(Some("loud")), Method::Auto);
        assert_eq!(Method::named(None), Method::Auto);
        assert_eq!(Method::named(Some("off")), Method::Off);
        assert_eq!(Condition::named(Some("sometimes")), Condition::Unfocused);
        assert_eq!(Condition::named(Some("always")), Condition::Always);
    }

    /// `osc9` and `bel` each ask for their own method, which `auto` and `off` do not decide, so a
    /// person who names one gets it on a terminal `auto` would have chosen the other for.
    #[test]
    fn each_method_word_asks_for_its_own_method() {
        assert_eq!(Method::named(Some("osc9")), Method::Osc9);
        assert_eq!(Method::named(Some("bel")), Method::Bel);
        assert_eq!(Method::named(Some("auto")), Method::Auto);
        assert_eq!(Condition::named(Some("unfocused")), Condition::Unfocused);
    }

    /// The sequence is the one thing here a terminal acts on, so what it carries is fixed text. No
    /// kind takes an argument, and each text holds no control character beyond the two that frame it.
    #[test]
    fn the_text_of_each_notice_is_fixed_and_carries_no_control_character_of_its_own() {
        for wire in [Wire::Osc9, Wire::Bel] {
            for kind in [Kind::TurnFinished, Kind::Waiting] {
                let text = bytes(wire, kind);
                let inner = text
                    .strip_prefix("\x1b]9;")
                    .and_then(|rest| rest.strip_suffix('\x07'))
                    .unwrap_or("");
                assert!(!inner.chars().any(char::is_control), "{text:?}");
                assert!(
                    text == "\x07" || text.starts_with("\x1b]9;bravebot: "),
                    "{text:?}"
                );
            }
        }
    }
}
