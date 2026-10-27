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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confirm::tests::*;
    use crate::confirm::*;
    use ratatui::backend::TestBackend;

    fn typed(text: &str) -> Vec<(KeyEvent, bool)> {
        text.chars().map(|c| lone(KeyCode::Char(c))).collect()
    }

    fn noted_write(script: &[(KeyEvent, bool)], request: &WriteRequest) -> Noted<WriteAnswer> {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let (next, alone) = scripted(script);
        write::ask_reading(&mut terminal, request, next, alone)
    }

    /// PROMPT-12: Tab opens a line, what is typed in it is the note, and Enter sends it with a no
    /// until the person switches it. A refusal with words is the case the issue is about.
    #[test]
    fn a_note_typed_at_a_write_is_sent_with_the_no() {
        let mut script = vec![lone(KeyCode::Tab)];
        script.extend(typed("use config.toml"));
        script.push(lone(KeyCode::Enter));
        let answer = noted_write(&script, &request("new", Some("old")));
        assert_eq!(answer.answer, WriteAnswer::Reject);
        assert_eq!(answer.note.as_deref(), Some("use config.toml"));
        assert!(!answer.stops_the_turn());
        let decision = answer.decision();
        assert!(!decision.approved());
        assert_eq!(decision.note(), Some("use config.toml"));
    }

    /// PROMPT-12: Tab inside the line switches the answer it is sent with to a yes, where the
    /// question's own `y` is taken from the draw.
    #[test]
    fn a_note_can_be_sent_with_a_yes_where_the_question_takes_one() {
        let mut script = vec![lone(KeyCode::Tab)];
        script.extend(typed("mind the tests"));
        script.extend([lone(KeyCode::Tab), lone(KeyCode::Enter)]);
        let answer = noted_write(&script, &request("new", Some("old")));
        assert_eq!(answer.answer, WriteAnswer::Approve);
        assert_eq!(answer.note.as_deref(), Some("mind the tests"));
        assert!(answer.decision().approved());
    }

    /// PROMPT-12: an empty line sends the answer and no note.
    #[test]
    fn an_empty_note_is_no_note() {
        let answer = noted_write(
            &[lone(KeyCode::Tab), lone(KeyCode::Enter)],
            &request("new", Some("old")),
        );
        assert_eq!(answer, Noted::plain(WriteAnswer::Reject));
        let blank = noted_write(
            &[
                lone(KeyCode::Tab),
                lone(KeyCode::Char(' ')),
                lone(KeyCode::Enter),
            ],
            &request("new", Some("old")),
        );
        assert_eq!(blank.note, None, "spaces were sent as a note");
    }

    /// PROMPT-12: Esc closes the line and nothing is answered, so what was typed goes nowhere and
    /// the question is still there to be answered, either way.
    #[test]
    fn escape_closes_the_note_and_leaves_the_question_open() {
        for (then, expected) in [
            (KeyCode::Char('n'), WriteAnswer::Reject),
            (KeyCode::Char('y'), WriteAnswer::Approve),
        ] {
            let mut script = vec![lone(KeyCode::Tab)];
            script.extend(typed("never mind"));
            script.extend([lone(KeyCode::Esc), lone(then)]);
            assert_eq!(
                noted_write(&script, &request("new", Some("old"))),
                Noted::plain(expected),
                "the closed line's words went with the answer to {then:?}"
            );
        }
    }

    /// INPUT-34: a Tab that arrived with others opens nothing, so a program writing a Tab, words and
    /// a newline into the terminal fills no note. The `y` after it is the person's own and answers.
    #[test]
    fn a_tab_that_arrived_with_others_opens_no_note() {
        let answer = noted_write(
            &[
                crowded(KeyCode::Tab),
                crowded(KeyCode::Char('x')),
                crowded(KeyCode::Enter),
                lone(KeyCode::Char('y')),
            ],
            &request("new", Some("old")),
        );
        assert_eq!(answer, Noted::plain(WriteAnswer::Approve));
    }

    /// INPUT-34: inside the line, an Enter that arrived with others sends nothing and types
    /// nothing, and a Tab that did changes nothing. What was typed waits for a key of the person's.
    #[test]
    fn a_note_is_sent_and_switched_only_by_keys_that_arrived_alone() {
        let mut script = vec![lone(KeyCode::Tab)];
        script.extend(typed("hi"));
        script.extend([
            crowded(KeyCode::Enter),
            crowded(KeyCode::Tab),
            lone(KeyCode::Enter),
        ]);
        let answer = noted_write(&script, &request("new", Some("old")));
        assert_eq!(answer.answer, WriteAnswer::Reject, "a crowded Tab said yes");
        assert_eq!(answer.note.as_deref(), Some("hi"));
    }

    /// PROMPT-6: no standing key takes a note. `a` and `r` answer as they did, with none, and typed
    /// inside the line they are letters.
    #[test]
    fn no_standing_key_takes_a_note() {
        let request = a_credential_write(true, true);
        for (key, expected) in [
            ('a', WriteAnswer::ApproveAlways),
            ('r', WriteAnswer::ApproveAndRecord),
        ] {
            assert_eq!(
                noted_write(&[lone(KeyCode::Char(key))], &request),
                Noted::plain(expected)
            );
        }
        let answer = noted_write(
            &[
                lone(KeyCode::Tab),
                lone(KeyCode::Char('a')),
                lone(KeyCode::Char('r')),
                lone(KeyCode::Enter),
            ],
            &request,
        );
        assert_eq!(answer.answer, WriteAnswer::Reject);
        assert_eq!(answer.note.as_deref(), Some("ar"));
    }

    /// PROMPT-1, PROMPT-4: the line sends a yes only where the question's own `y` is taken. This
    /// draw is too short to show what `a` and `r` settle, so no key that approves is taken, and a
    /// Tab inside the line does not make one.
    #[test]
    fn a_note_cannot_say_yes_where_the_question_takes_no_yes() {
        let request = a_credential_write(true, true);
        let mut terminal = Terminal::new(TestBackend::new(60, 12)).expect("terminal");
        let mut drawn = Drawn::default();
        terminal
            .draw(|frame| drawn = write::draw(frame, &request, 0, &mut Seen::default()))
            .expect("draw");
        assert!(!drawn.answerable(), "the fixture's yes was taken");

        let (next, alone) = scripted(&[
            lone(KeyCode::Tab),
            lone(KeyCode::Char('x')),
            lone(KeyCode::Tab),
            lone(KeyCode::Enter),
        ]);
        let answer = write::ask_reading(&mut terminal, &request, next, alone);
        assert_eq!(answer.answer, WriteAnswer::Reject);
        assert_eq!(answer.note.as_deref(), Some("x"));
    }

    /// Ctrl-C in the line stops the turn and sends nothing, as it does at the question.
    #[test]
    fn ctrl_c_in_a_note_interrupts_and_sends_no_words() {
        let answer = noted_write(
            &[
                lone(KeyCode::Tab),
                lone(KeyCode::Char('x')),
                (
                    KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                    true,
                ),
            ],
            &request("new", Some("old")),
        );
        assert_eq!(answer, Noted::plain(WriteAnswer::Interrupt));
        assert!(answer.stops_the_turn());
    }

    /// The line opens at a run prompt and at a call prompt as at a write, and the standing answers
    /// there take none.
    #[test]
    fn a_run_and_a_call_take_a_note_with_the_plain_answers_only() {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let mut script = vec![lone(KeyCode::Tab)];
        script.extend(typed("run it in ci"));
        script.push(lone(KeyCode::Enter));
        let (next, alone) = scripted(&script);
        let refused = run::ask_run_reading(&mut terminal, &a_run(false), next, alone);
        assert_eq!(refused.answer, RunAnswer::Reject);
        assert_eq!(refused.decision().note(), Some("run it in ci"));

        let mut script = vec![lone(KeyCode::Tab)];
        script.extend(typed("fine"));
        script.extend([lone(KeyCode::Tab), lone(KeyCode::Enter)]);
        let (next, alone) = scripted(&script);
        let approved = run::ask_run_reading(&mut terminal, &a_run(false), next, alone);
        assert_eq!(approved.answer, RunAnswer::Approve);
        assert!(approved.decision().approved());

        let recordable = a_recordable_run();
        let (next, alone) = scripted(&[lone(KeyCode::Char('a'))]);
        let standing = run::ask_run_reading(&mut terminal, &recordable, next, alone);
        assert_eq!(standing, Noted::plain(RunAnswer::ApproveAlways));

        let mut script = vec![lone(KeyCode::Tab)];
        script.extend(typed("ask for Paris"));
        script.push(lone(KeyCode::Enter));
        let (next, alone) = scripted(&script);
        let call_refused = mcp::ask_mcp_call_reading(&mut terminal, &call(true, None), next, alone);
        assert_eq!(call_refused.answer, CallAnswer::Reject);
        assert_eq!(call_refused.decision().note(), Some("ask for Paris"));

        let (next, alone) = scripted(&[lone(KeyCode::Char('2'))]);
        let stands = mcp::ask_mcp_call_reading(&mut terminal, &call(true, None), next, alone);
        assert_eq!(stands, Noted::plain(CallAnswer::ApproveAndStand));
    }

    /// PROMPT-1, PROMPT-4: at a run whose plan has not all been on the screen the line can send a
    /// no and cannot be switched to a yes.
    #[test]
    fn a_note_cannot_run_a_plan_that_was_not_all_on_the_screen() {
        let request = a_run_with_a_long_argument_list();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let mut script = vec![lone(KeyCode::Tab)];
        script.extend(typed("go"));
        script.extend([lone(KeyCode::Tab), lone(KeyCode::Enter)]);
        let (next, alone) = scripted(&script);
        let answer = run::ask_run_reading(&mut terminal, &request, next, alone);
        assert_eq!(answer.answer, RunAnswer::Reject);
        assert_eq!(answer.note.as_deref(), Some("go"));
    }
}
