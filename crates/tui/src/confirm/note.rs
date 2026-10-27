//! The line a person types a note into beside their answer to a write, a run or a call (PROMPT-12).
//!
//! Opened from the question with Tab, and closed by Esc, which goes back to the question with
//! nothing answered. The question stays the one that decides: this line only chooses between the
//! plain yes and the plain no and attaches words to whichever it was. It carries no standing answer
//! (PROMPT-6), it takes a yes only where the question's own `y` would have been taken (PROMPT-1,
//! PROMPT-4), and it opens and sends only from keys that arrived on their own (INPUT-34).

use bravebot_agent::confirm::NOTE_LIMIT;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::theme;

use super::Next;
use super::chrome::{approving, centred, indented, panel, refusing, stopping};

/// A question's answer and the words typed beside it, where any were.
///
/// The answer is only ever the plain yes or the plain no when there is a note: no key that stands
/// for more than one effect opens this line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Noted<A> {
    pub answer: A,
    pub note: Option<String>,
}

impl<A> Noted<A> {
    /// An answer given without opening the line.
    pub(super) fn plain(answer: A) -> Self {
        Self { answer, note: None }
    }
}

/// How a line ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Taken {
    /// Esc: the line is closed and the question is still being asked.
    Back,
    /// Sent: yes or no, and what was typed, which may be nothing.
    Sent { yes: bool, note: String },
    /// Ctrl-C.
    Interrupt,
    /// The terminal cannot be drawn to or read.
    Lost,
}

impl Taken {
    /// The answer the question gives for how the line ended, or `None` where it is still being
    /// asked. Only the plain yes and the plain no ever carry words.
    pub(super) fn settle<A>(self, yes: A, no: A, interrupt: A) -> Option<Noted<A>> {
        match self {
            Taken::Back => None,
            Taken::Sent {
                yes: said_yes,
                note,
            } => Some(Noted {
                answer: if said_yes { yes } else { no },
                note: bravebot_agent::confirm::tidy_note(&note),
            }),
            Taken::Interrupt => Some(Noted::plain(interrupt)),
            Taken::Lost => Some(Noted::plain(no)),
        }
    }
}

/// What a key means at the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit {
    Insert(char),
    Erase,
    Back,
    Switch,
    Send,
    Interrupt,
    Nothing,
}

/// Whether `key` opens the line from a question. Only a Tab that arrived on its own does.
pub(super) fn opens(key: KeyEvent, arrived_alone: bool) -> bool {
    arrived_alone && key.code == KeyCode::Tab && key.modifiers == KeyModifiers::NONE
}

/// What one key does at the line.
///
/// Typing is taken however the characters arrived, since filling the line decides nothing and the
/// person reads it before sending. Switching and sending are not: those come from a key that
/// arrived on its own, so text another program wrote into the terminal cannot send itself.
fn edit_for(key: KeyEvent, arrived_alone: bool) -> Edit {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Edit::Interrupt,
            _ => Edit::Nothing,
        };
    }
    match key.code {
        KeyCode::Esc => Edit::Back,
        KeyCode::Tab if arrived_alone => Edit::Switch,
        KeyCode::Enter if arrived_alone => Edit::Send,
        KeyCode::Backspace => Edit::Erase,
        KeyCode::Char(c) if !c.is_control() && !key.modifiers.contains(KeyModifiers::ALT) => {
            Edit::Insert(c)
        }
        _ => Edit::Nothing,
    }
}

/// Open the line over the question and wait for it to end, reading each event from `next` and
/// asking `alone` straight after each one.
///
/// `can_say_yes` is whether the question's own `y` is taken from the draw this was opened at.
/// Where it is not, the line can only send a no.
pub(super) fn take_reading<B: Backend>(
    terminal: &mut Terminal<B>,
    can_say_yes: bool,
    mut next: impl FnMut() -> Next,
    mut alone: impl FnMut() -> bool,
) -> Taken {
    let mut typed = String::new();
    // A no until the person switches it: nothing is approved by default.
    let mut yes = false;
    let mut ignored = false;
    loop {
        // Not again for a report that changed nothing (MOUSE-44).
        if !ignored
            && terminal
                .draw(|frame| draw(frame, &typed, yes, can_say_yes))
                .is_err()
        {
            return Taken::Lost;
        }
        ignored = false;
        match next() {
            Next::Ignored => ignored = true,
            // The line has nothing to scroll.
            Next::Skip | Next::Wheel(_) => continue,
            Next::Lost => return Taken::Lost,
            Next::Press(key) => match edit_for(key, alone()) {
                Edit::Insert(c) => {
                    if typed.chars().count() < NOTE_LIMIT {
                        typed.push(c);
                    }
                }
                Edit::Erase => {
                    typed.pop();
                }
                Edit::Back => return Taken::Back,
                Edit::Switch => yes = can_say_yes && !yes,
                Edit::Send => {
                    return Taken::Sent {
                        yes: yes && can_say_yes,
                        note: typed,
                    };
                }
                Edit::Interrupt => return Taken::Interrupt,
                Edit::Nothing => {}
            },
        }
    }
}

/// Draw the line: what it will be sent with, what has been typed, and the keys.
fn draw(frame: &mut ratatui::Frame, typed: &str, yes: bool, can_say_yes: bool) {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::brand_primary(), t!(note_title));
    let width = inside.width as usize;
    let muted = Style::default().fg(theme::muted());
    let bold = Style::default().add_modifier(Modifier::BOLD);

    let (sent_with, colour) = if yes {
        (t!(note_sent_with_yes), theme::ok())
    } else {
        (t!(note_sent_with_no), theme::fail())
    };
    let mut lines = vec![
        Line::from(Span::styled(
            format!("  {sent_with}"),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  > ", muted),
            Span::styled(typed.to_string(), bold),
            Span::styled("▏", Style::default().fg(theme::brand_primary())),
        ]),
        Line::raw(""),
    ];
    lines.extend(indented(t!(note_explained), muted, width));
    lines.push(Line::raw(""));

    let mut keys = vec![
        Span::styled("  enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!(" {}    ", t!(note_send))),
    ];
    if can_say_yes {
        keys.push(Span::styled("tab", approving(theme::ok(), true)));
        keys.push(Span::raw(format!(
            " {}    ",
            if yes {
                t!(note_switch_to_no)
            } else {
                t!(note_switch_to_yes)
            }
        )));
    }
    keys.push(Span::styled("esc", refusing()));
    keys.push(Span::raw(format!(" {}    ", t!(note_back))));
    keys.extend(stopping());
    lines.push(Line::from(keys));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inside);
}

/// The hint drawn beside a question's keys that Tab opens this line.
pub(super) fn hint() -> [Span<'static>; 2] {
    [
        Span::styled(
            "tab",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(note_key)),
            Style::default().fg(theme::muted()),
        ),
    ]
}
