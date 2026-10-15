//! What a key press or a mouse event does: to the input box, to the modes that stand over it, and
//! to a turn or a command in flight.

use crate::command_table::{command_typed, runs_while_working, waits_behind_the_queue};
use crate::state::{Session, Status};
use crate::{input, render};
use bravebot_core::cancel::Cancel;
use bravebot_i18n::t;
use ratatui::crossterm::event::{
    Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};

use super::dispatch_command;

/// What a key press asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Redraw,
    Submit(String),
    /// Bring what is on the clipboard into the prompt. Runs the platform's clipboard tools, so
    /// the loop does it rather than the key handler.
    Paste,
    /// Stop the turn in flight.
    Cancel,
    /// Stop the turn in flight so what is waiting goes now, the prompts among it as one turn.
    SendNow,
    /// Take what the selection covers, which needs the screen as it was last drawn.
    Copy,
    /// Put this reply's text on the clipboard. Runs the platform's clipboard tools, so the loop
    /// does it rather than the command.
    CopyReply(String),
    /// Write the prompt somewhere with room to think. Needs the terminal, which the loop hands
    /// over to the editor and takes back afterwards.
    Edit,
    /// Ask which model to use. Needs the network and the terminal, so the loop runs it.
    ChooseModel,
    /// Ask which theme to paint in. Needs the terminal, so the loop runs it.
    ChooseTheme,
    /// Apply a theme by name without opening the picker.
    SetTheme(String),
    /// Ask how hard to think. Needs the terminal, so the loop runs it.
    ChooseEffort,
    /// Take a level by name without opening the picker.
    SetEffort(String),
    /// Say which model the planner may consult, take one by name, or drop the choice. Empty says.
    /// Needs the configuration to resolve the name against, which the loop owns.
    Advisor(String),
    /// Say which output style is in force, take one by name, or clear the pick. Empty says.
    Style(String),
    /// Ask how the box should edit. Needs the terminal, so the loop runs it.
    ChooseEditing,
    /// Open another directory. Needs the workspace and the trust map, which the loop owns.
    AddDirectory(String),
    /// Close one directory `/add-dir` opened. Needs the workspace, the trust map and the session
    /// record, which the loop owns.
    CloseDirectory(String),
    /// Hand this session to a background process and leave. Needs the session record, which the
    /// loop owns.
    Background,
    /// Work somewhere else from now on. Needs the workspace, the trust map and the session
    /// record, all of which the loop owns.
    ChangeDirectory(String),
    /// Summarise the conversation so far. Needs the conversation and the network, which the loop
    /// owns.
    Compact(String),
    /// Ask something beside the work, over a copy of the conversation. Needs the conversation and
    /// the network, which the loop owns, and gives the conversation nothing back.
    ///
    /// The pictures pasted into the question travel with it, because it is a request and a paste is
    /// the person's own input to the thing they are asking about. So do the files dropped onto it,
    /// which are read before the request is assembled: the same gesture put both there.
    Aside(
        String,
        Vec<crate::state::AttachedImage>,
        Vec<crate::state::Attached>,
    ),
    /// Recap the session so far, over a copy of the conversation, in the way an aside is answered.
    Recap,
    /// Plan this task in full and then walk it. Needs the workspace, the trust map and the
    /// network, which the loop owns, and gives the conversation nothing back.
    ///
    /// The pictures pasted into the task travel with it, for the reason a question's do: a
    /// screenshot of the thing to be built is part of the task and nothing observed. So do the files
    /// dropped onto it, whose paths a person's gesture fixed before the plan was asked for.
    Manifest(
        String,
        Vec<crate::state::AttachedImage>,
        Vec<crate::state::Attached>,
    ),
    /// Run a definition the person named on the task after its name, or list the names where
    /// there is none. Needs the workspace and the trust map, which decide what resolves, and
    /// becomes a [`Action::Submit`] once the name has matched.
    Address(
        String,
        Vec<crate::state::AttachedImage>,
        Vec<crate::state::Attached>,
    ),
    /// Start a new session here. Needs the conversation and the session record, which the loop owns.
    Clear,
    /// Have a brief written for the goal typed, from this session's conversation, and put it in the
    /// box. Needs the conversation and the network, which the loop owns.
    Handoff(String),
    /// Start a new session whose first prompt is this, the brief as the person left it, and record
    /// which session it came from. Needs the conversation and the session record, which the loop
    /// owns.
    HandOver(String),
    /// Call this session something else. Needs the session record, which the loop owns.
    Rename(String),
    /// Add a note to the session record, kept beside the conversation (SESSION-36). Needs the
    /// session record, which the loop owns.
    Note(String),
    /// Copy this session and carry on in the copy, named as given or marked as a fork. Needs the
    /// conversation and the session record, which the loop owns.
    Branch(String),
    /// Leave this session for another of this directory's: the id typed, or empty for the picker.
    /// Needs the terminal, the record store and the loop's state, which the loop owns.
    Resume(String),
    /// Show, set or clear one of the session's links. Needs the session record, which the loop
    /// owns, and carries the argument unparsed, since what it says back goes in the transcript.
    Link(bravebot_session::sessions::Link, String),
    /// Report what this session is. Needs the workspace and the trust map, which the loop owns.
    Status,
    /// List, add or remove the reach remembered for commands. Carries the argument unparsed, since
    /// what it says back goes in the transcript, and needs the session's id and working directory.
    Reach(String),
    /// Report the sandbox mode, or move it from the next turn. Carries the word unparsed, since the
    /// answer depends on the managed file and may need the screen to ask.
    Sandbox(String),
    /// Withdraw the remembered answer about the working directory. Needs the workspace, which the
    /// loop owns, and leaves this session's map as it is.
    ForgetTrust,
    /// List the checkouts the session keeps. Needs the workspace, which the loop owns.
    ListCheckouts,
    /// List each definition's memory (MEMORY-12).
    ListMemories,
    /// Remove the checkout with this number. Needs the workspace, the trust map and the terminal
    /// to ask on, which the loop owns.
    RemoveCheckout(String),
    /// Bring back the files written in the checkout with this number, or only the paths given.
    /// Needs the workspace, the trust map and the terminal to ask on, which the loop owns.
    ApplyCheckout(String, Vec<String>),
    /// Run a command the user typed in shell mode. Needs the workspace and the conversation.
    Run(String),
    /// Run a command typed `!!`: the same, with its output drawn and never put in the conversation.
    RunPrivately(String),
    /// Put the transcript in front of the user in their editor. Needs the terminal, which the
    /// loop owns, and gives the session nothing back.
    Show,
    /// Export the session transcript to a markdown file.
    Export(Option<String>),
    /// Undo the last turn. Needs the workspace and conversation.
    Undo,
    /// List the points a rewind could reach, or go back to one. Needs the same as `Undo`, which
    /// is why the argument is carried unparsed: a number that is not one is answered with a line
    /// in the transcript, and the transcript is the loop's.
    Rewind(String),
    Quit,
}

/// The scope chord as the prompt history's switch, and the answer to the key if it was that.
///
/// One key cycles: every stored prompt, this project's, this session's, and back, skipping a scope
/// that holds nothing. Only where the history can use it (INPUT-40): on a sent prompt, and where
/// another scope holds a prompt. Anywhere else the chord is not
/// a switch, so the key is what `edit_line` makes of it, and when that is nothing it is swallowed
/// rather than typed.
fn change_history_scope(session: &mut Session, key: KeyEvent) -> Option<Action> {
    if !session.bindings().is_scope(&key) {
        return None;
    }
    if session.history.next_scope().is_some() {
        session.cycle_history_scope();
    } else if !edit_line(session, key) {
        return Some(Action::None);
    }
    Some(Action::Redraw)
}

/// Move the caret or delete around it, and say whether the key was one that does.
///
/// Shared by the idle and mid-turn handlers, because what has been typed can be edited in both:
/// the box holds the same line either way, and only sending it is refused while a turn runs.
///
/// Both Ctrl and Alt are read as the word modifier, since terminals disagree about which they send
/// for Ctrl-Left.
fn edit_line(session: &mut Session, key: KeyEvent) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let word = ctrl || alt;

    match key.code {
        KeyCode::Left if word => session.move_word_left(),
        KeyCode::Right if word => session.move_word_right(),
        KeyCode::Left => session.move_left(),
        KeyCode::Right => session.move_right(),
        // Bare, because the ends of the line being typed are what these mean in every other text
        // field. The transcript keeps them under Ctrl.
        KeyCode::Home if !ctrl => session.move_to_line_start(),
        KeyCode::End if !ctrl => session.move_to_line_end(),
        KeyCode::Delete => session.delete_forward(),
        // With a modifier only: a bare Backspace is also how shell mode is left, so it stays
        // where that is decided.
        KeyCode::Backspace if word => session.delete_word_before(),
        // The readline bindings as well as the named keys, because a terminal or an ssh session
        // with a keymap of its own may deliver none of the above, and then the middle of a line
        // could not be reached at all.
        KeyCode::Char('a') if ctrl => session.move_to_line_start(),
        KeyCode::Char('e') if ctrl => session.move_to_line_end(),
        KeyCode::Char('b') if word => session.move_word_left(),
        KeyCode::Char('f') if word => session.move_word_right(),
        KeyCode::Char('w') if ctrl => session.delete_word_before(),
        KeyCode::Char('u') if ctrl => session.delete_to_line_start(),
        KeyCode::Char('k') if ctrl => session.delete_to_line_end(),
        // Alt only: Ctrl-D leaves (INPUT-4).
        KeyCode::Char('d') if alt && !ctrl => session.delete_word_after(),
        KeyCode::Char('y') if ctrl => session.yank(),
        _ => return false,
    }
    true
}

/// Whether `key` asks for a new line in the prompt rather than for the prompt to be sent.
///
/// Two spellings, because a terminal has two ways of saying it and which one arrives is not
/// something the user chose:
///
/// - Shift-Enter, which needs the terminal to report the modifier. Most do not: the byte for Enter
///   is the same however it was pressed, which is why [`super::take_over_terminal`] asks for disambiguated
///   keys where that is understood.
/// - Ctrl-J, which is the byte `\n`. That is what a terminal configured to send a newline for
///   Shift-Enter delivers, and it is the arrangement iTerm and Terminal.app need, since neither
///   reports the modifier. It is also typeable directly on a terminal that does neither.
///
/// Ctrl-J rather than Ctrl-M: `\r` is Enter itself and binding it would break sending. Nothing else
/// may claim Ctrl-J, whatever readline does with it, since on those terminals it is the only way to
/// write a paragraph.
pub(super) fn starts_a_line(key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Enter => key.modifiers.contains(KeyModifiers::SHIFT),
        KeyCode::Char('j') => key.modifiers.contains(KeyModifiers::CONTROL),
        _ => false,
    }
}

/// Whether a press asks for what is waiting to go now rather than when the turn in flight ends.
///
/// Ctrl-Enter, which reaches this process only where the terminal reports the modifier, for the
/// reason Shift-Enter does. Elsewhere it arrives as Enter and queues the line, which is the half of
/// what it asks for that Enter does too.
fn sends_now(key: KeyEvent) -> bool {
    key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// The key a press stands for in vi's NORMAL mode, where vi spells one of these with a letter.
///
/// `None` for every other press, including every press at all in the ordinary box. Only the bindings
/// that reach past the line are here: the caret motions are the line's own and are carried out where
/// the line is.
fn spelled_by_vi(session: &Session, key: KeyEvent) -> Option<KeyEvent> {
    let KeyCode::Char(c) = key.code else {
        return None;
    };
    // A letter with a modifier is a chord rather than one of vi's keys, and Ctrl-D is not `d`.
    if !key.modifiers.is_empty() {
        return None;
    }
    let code = match session.vi_spells(c)? {
        crate::state::Spelled::Up => KeyCode::Up,
        crate::state::Spelled::Down => KeyCode::Down,
        // The chord rather than the action, so the arm that opens the search is the only place it is
        // opened from and the two cannot come to disagree about when it may be.
        crate::state::Spelled::SearchPrompts => {
            let chord = session.bindings().history;
            return Some(KeyEvent::new(chord.code, chord.modifiers));
        }
    };
    Some(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Whether a key press asks for the next permission mode.
///
/// Shift-Tab, which reaches this process as either of two things: a terminal that has been asked to
/// disambiguate reports Tab with a Shift modifier, and one that has not sends the older `BackTab`.
/// Both are accepted, since which arrives is the terminal's choice and a binding that worked on one
/// machine and not the next would read as broken.
fn cycles_the_mode(key: KeyEvent) -> bool {
    match key.code {
        KeyCode::BackTab => true,
        KeyCode::Tab => key.modifiers.contains(KeyModifiers::SHIFT),
        _ => false,
    }
}

/// Whether a key press reaches the turn in flight.
///
/// A mode standing over the session answers these keys before the turn does, because it is the
/// nearer thing to stop. Somebody who opened the scroller to read what the turn had already done,
/// the view to see what a delegate was doing, or the search to find a prompt that has scrolled
/// away is not asking for the turn to end when they close it again, and the press that reaches
/// the turn is the next one. Watching is also the mode most likely to be open while something is
/// going wrong. A picker opened mid-turn is left the same way.
///
/// Each of the four names these keys itself, and this is what lets them have them: the loops read
/// this before any ladder, so a mode left out here is one whose way out ends the turn instead.
///
/// Named rather than written out at each loop, because there are three of them and a condition
/// copied three times is a condition that ends up meaning three things.
///
/// Escape is the box's as well for somebody editing the way vi does, and there it reaches the turn
/// only once the box has no use for it (INPUT-24). Ctrl-C reaches the turn from any mode.
pub(super) fn stops_the_turn(session: &Session, key: KeyEvent) -> bool {
    stops_a_command(session, key) && !(is_escape(session, key) && escape_is_the_boxs(session))
}

/// Whether a key press reaches a command running from shell mode.
///
/// The keys that stop a turn, with Escape reaching the command from every vi mode: no press reaches
/// the box while a command runs, so there is no mode for Escape to have been meant for.
pub(super) fn stops_a_command(session: &Session, key: KeyEvent) -> bool {
    !session.scrolling()
        && session.watching().is_none()
        && !session.searching_history()
        && session.picking().is_none()
        && (is_ctrl_c(key) || is_escape(session, key))
}

/// Whether a press is Escape, in either spelling the box's style answers.
///
/// For somebody editing the way vi does, Ctrl-`[` is Escape too: a terminal asked to disambiguate
/// reports that chord where another sends the byte Escape already is, and which arrives is the
/// terminal's choice rather than the person's. The ordinary box gives the chord nothing.
fn is_escape(session: &Session, key: KeyEvent) -> bool {
    wants_cancel(key)
        || (key.code == KeyCode::Char('[')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && session.editing() == crate::vim::Editing::Vi)
}

/// Whether Escape is for the box rather than for what is running.
///
/// A vi user presses Escape to leave INSERT, VISUAL or REPLACE and to abandon a half-typed
/// instruction, out of habit, and the box is still in INSERT once the prompt has gone. Only NORMAL
/// mode with nothing waiting leaves the box no use for the press. A count is waiting, since it is
/// the start of an instruction. The ordinary box has no mode to leave.
fn escape_is_the_boxs(session: &Session) -> bool {
    match session.vi_mode() {
        None => false,
        Some(crate::vim::Mode::Normal) => session.half_typed().is_some(),
        Some(_) => true,
    }
}

/// Stop what is running, at the press that asked for it.
///
/// None of the places that answer such a press is a ladder: the loops running a turn, a plan or a
/// command read the stopping keys against [`stops_the_turn`] or [`stops_a_command`] themselves,
/// and a Ctrl-C at an approval box is answered by the box. So what [`handle_key_while_working`]
/// does for every other key has to be done here as well: the offer to leave lives for one press,
/// and each of these is a press.
///
/// It can be standing when the press arrives, because a turn can begin with nobody pressing
/// anything. A loop tick or a watch submits from the main loop, carrying an offer put up while the
/// session was idle into the turn, and the press that stops the turn is the press that offer was
/// waiting for. Left up, the hint row goes on saying the next Ctrl-C leaves over a box holding the
/// line the stop put back, which the next Ctrl-C would only take.
///
/// A half-typed vi instruction goes for the same reason: it waits for a character, and neither
/// stopping key is one.
pub(super) fn stop_what_is_running(session: &mut Session, cancel: &Cancel) {
    session.cleared_by_interrupt = false;
    session.abandon_half_typed();
    session.stop_asked();
    cancel.cancel();
}

/// Whether a press is a character typed, which is the only kind of press a half-typed vi
/// instruction waits for.
fn types_a_character(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char(_)) && key.modifiers.difference(KeyModifiers::SHIFT).is_empty()
}

/// Whether a character pressed with these modifiers is a key the scroller names (SCROLL-3).
///
/// A character with no modifier beyond Shift is the letter. With Ctrl alone it is one of the
/// chords the table lists. Anything else, such as Alt-J or Ctrl-K, is a chord the scroller does
/// not name.
fn names_the_chord(modifiers: KeyModifiers, c: char) -> bool {
    let modifiers = modifiers.difference(KeyModifiers::SHIFT);
    modifiers.is_empty()
        || (modifiers == KeyModifiers::CONTROL
            && matches!(c, 'y' | 'e' | 'p' | 'n' | 'u' | 'd' | 'f' | 'b'))
}

/// Interpret a key press while the scroller is open.
///
/// Every key the scroller answers is answered here, and a key it does not name does nothing at
/// all: nothing falls through to the box. A mode that leaks its keystrokes into a box the person
/// cannot see is the worse half of both, since `j` would scroll and also type a `j`, and the
/// prompt they had half written would quietly be a different prompt by the time they came back
/// to it. The line is untouched throughout and comes back exactly as it was.
///
/// Nothing here sends anything, so there is nothing in it for a running turn to refuse, and the
/// same list answers whether or not one is in flight.
pub(super) fn scroller_key(session: &mut Session, key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // The key list is read instead of the transcript rather than alongside it, so anything at all
    // puts it away, and that press is spent doing so. The list says as much, because a key that
    // silently did two things would be worse than one that does the obvious one.
    if session.scroller().is_some_and(|scroller| scroller.help) {
        session.toggle_scroller_help();
        return Action::Redraw;
    }

    // A search being typed takes the letters back, because typing is what they mean. What else
    // means anything here either finishes the search or leaves the mode.
    if session.typing_a_search() {
        return match key.code {
            KeyCode::Enter => {
                session.run_search();
                land_on_a_match(session);
                Action::Redraw
            }
            KeyCode::Esc => {
                session.abandon_search();
                Action::Redraw
            }
            // Backspacing past the start abandons the search, which is what the key means once
            // there is nothing left of the thing it deletes.
            KeyCode::Backspace => {
                if !session.backspace_search() {
                    session.abandon_search();
                }
                Action::Redraw
            }
            // The two chords that close the mode close it from in here too, and a needle half
            // typed into a mode that is going away goes with it. Neither is a character, so
            // neither is read as typing, and leaving both to do nothing left a mode whose only
            // way out was Escape.
            KeyCode::Char('c') if is_ctrl_c(key) => {
                session.close_scroller();
                Action::Redraw
            }
            _ if session.bindings().is_scroller(&key) => {
                session.close_scroller();
                Action::Redraw
            }
            KeyCode::Char(c) if !ctrl => {
                session.type_into_search(c);
                Action::Redraw
            }
            _ => Action::None,
        };
    }

    // A count is read as the box reads one (INPUT-35), and the key after it takes it whatever that
    // key is: the ones that move the view go that many times as far, and the rest drop it.
    if types_a_character(key)
        && let KeyCode::Char(c) = key.code
        && session.count_in_the_scroller(c)
    {
        return Action::Redraw;
    }
    let counted = session.take_the_scroller_count();
    let times = counted.unwrap_or(1);
    let by = |step: u16| step.saturating_mul(u16::try_from(times).unwrap_or(u16::MAX));

    match key.code {
        // The same ladder every other stop key here walks: the nearest thing there is to stop.
        // Four keys close the scroller: `q`, Escape, Ctrl-O and Ctrl-C. Ctrl-C does nothing else
        // here: the scroller is the nearest thing there is to stop, so a turn in flight goes on
        // running and the press that reaches it is the next one.
        //
        // A count waiting for its key is nearer than a standing search, and that is nearer than
        // the mode holding it, so each press takes off one and the last closes the scroller.
        KeyCode::Esc => {
            if counted.is_none() && !session.clear_search() {
                session.close_scroller();
            }
            Action::Redraw
        }
        KeyCode::Char('c') if is_ctrl_c(key) => {
            session.close_scroller();
            Action::Redraw
        }
        _ if session.bindings().is_scroller(&key) => {
            session.close_scroller();
            Action::Redraw
        }

        // A character pressed with Ctrl, Alt or Super held is the chord and not the letter. The
        // file names only the Ctrl chords listed in `names_the_chord`, so any other such press
        // does nothing instead of acting as the plain letter would (SCROLL-2).
        KeyCode::Char(c) if !names_the_chord(key.modifiers, c) => Action::None,

        KeyCode::Char('q') => {
            session.close_scroller();
            Action::Redraw
        }

        // A line at a time. The letters here are the same key with or without Ctrl, as `b` is
        // below: `less` reads `e` and Ctrl-E alike, and vi has only the chord. Ctrl-N and Ctrl-P
        // are chords only, since `n` alone is the next match.
        KeyCode::Up | KeyCode::Char('k' | 'y') => {
            session.scroller_back(by(1));
            Action::Redraw
        }
        KeyCode::Char('p') if ctrl => {
            session.scroller_back(by(1));
            Action::Redraw
        }
        KeyCode::Down | KeyCode::Char('j' | 'e') => {
            session.scroller_on(by(1));
            Action::Redraw
        }
        KeyCode::Char('n') if ctrl => {
            session.scroller_on(by(1));
            Action::Redraw
        }

        // Half a screen, which is the one movement that keeps context on both sides of itself.
        KeyCode::Char('u') => {
            session.scroller_back(by(session.half_screen()));
            Action::Redraw
        }
        KeyCode::Char('d') => {
            session.scroller_on(by(session.half_screen()));
            Action::Redraw
        }

        // A whole screen, in both dialects. `b` and `f` are the same key with or without Ctrl,
        // because somebody who knows one spelling should not find the other typing a letter.
        KeyCode::Char(' ' | 'f') | KeyCode::PageDown => {
            session.scroller_on(by(session.whole_screen()));
            Action::Redraw
        }
        KeyCode::Char('b') | KeyCode::PageUp => {
            session.scroller_back(by(session.whole_screen()));
            Action::Redraw
        }

        // The ends.
        KeyCode::Char('g') if !ctrl => {
            session.scroller_to_first_row();
            Action::Redraw
        }
        KeyCode::Char('G') => {
            session.scroller_to_last_row();
            Action::Redraw
        }
        KeyCode::Char('<') | KeyCode::Home => {
            session.scroller_to_first_row();
            Action::Redraw
        }
        KeyCode::Char('>') | KeyCode::End => {
            session.scroller_to_last_row();
            Action::Redraw
        }

        // Turn by turn. Where these land is settled by what the person typed, since a prompt is
        // the one thing in a transcript they wrote themselves.
        KeyCode::Char('{') => {
            session.to_previous_prompt(times);
            Action::Redraw
        }
        KeyCode::Char('}') => {
            session.to_next_prompt(times);
            Action::Redraw
        }

        KeyCode::Char('/') => {
            session.begin_search();
            Action::Redraw
        }
        KeyCode::Char('n') => {
            walk_the_matches(session, true, times);
            Action::Redraw
        }
        KeyCode::Char('N') => {
            walk_the_matches(session, false, times);
            Action::Redraw
        }

        KeyCode::Char('?') => {
            session.toggle_scroller_help();
            Action::Redraw
        }
        KeyCode::Char('x') => {
            session.toggle_expanded(render::as_last_drawn);
            Action::Redraw
        }
        KeyCode::Char('v')
            if !ctrl && session.status != Status::Working && !session.viewing_request() =>
        {
            Action::Show
        }

        _ => Action::None,
    }
}

/// Move to the first match at or after the top of the view.
///
/// The transcript is laid out again here, because the needle was set by the key press being
/// answered: the last frame was drawn looking for something else, and what it found is no answer
/// to the question just asked.
fn land_on_a_match(session: &mut Session) {
    let laid = render::as_last_drawn(session);
    session.land_on_a_match(&laid.matches);
}

/// Walk `times` matches on, or back, wrapping at either end as a single step does.
///
/// The rows are the ones the last frame found, which is the frame the person is looking at while
/// they press the key. Nothing has changed about what is being looked for since it was drawn, so
/// there is nothing to lay out again. A count stops at a thousand, so the walk is a thousand steps
/// at most whatever was typed.
fn walk_the_matches(session: &mut Session, forwards: bool, times: u32) {
    let found = session.laid.matches.clone();
    for _ in 0..times {
        session.to_a_match(&found, forwards);
    }
}

/// Interpret one key press while the prompt history is being searched.
///
/// Every letter narrows the list, so the keys that do anything else are the ones a letter cannot
/// be: the arrows, Enter, Escape, Backspace, and two chords. Nothing here sends anything, which is
/// why the same list answers whether or not a turn is running.
fn history_search_key(session: &mut Session, key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // The two chords first, and read off the bindings rather than off the keyboard. Every character
    // narrows the list, so one moved onto a letter would otherwise be typed into the needle here
    // while it opened the search from the box.
    //
    // Every prompt, or the ones sent from this workspace. It is the chord that put the line away
    // out there, which is the same key INPUT-31 opens this search with.
    if session.bindings().is_stash(&key) {
        session.scope_history_search();
        return Action::Redraw;
    }
    // The chord that opened it closes it, and so does Ctrl-C: the nearest thing there is to stop is
    // the search, and the turn behind it goes on running, so the press that reaches it is the next
    // one.
    if session.bindings().is_history(&key) || is_ctrl_c(key) {
        session.close_history_search();
        return Action::Redraw;
    }
    if ctrl {
        return Action::None;
    }

    match key.code {
        KeyCode::Esc => {
            session.close_history_search();
            Action::Redraw
        }
        // Into the box, never into a request. What was chosen off a list is read by the person
        // whose next keystroke sends it, which is the whole of what makes a stored line usable.
        KeyCode::Enter => {
            session.take_history_match();
            Action::Redraw
        }
        KeyCode::Up => {
            session.history_search_older();
            Action::Redraw
        }
        KeyCode::Down => {
            session.history_search_newer();
            Action::Redraw
        }
        // Backspacing past the start leaves, which is what the key means once there is nothing
        // left of the thing it deletes. The same ladder the scroller's search walks.
        KeyCode::Backspace => {
            if !session.backspace_history_search() {
                session.close_history_search();
            }
            Action::Redraw
        }
        KeyCode::Char(c) => {
            session.type_into_history_search(c);
            Action::Redraw
        }
        _ => Action::None,
    }
}

/// Interpret a key press while a delegate is being watched.
///
/// Every key is answered here and nothing falls through. What a person types while watching would
/// otherwise go into a box they cannot see, to be sent to a turn they are not looking at.
///
/// Two levels, and the key that leaves is read against the nearer one: from a delegate it goes
/// back to the list, and from the list it closes. The chord that opened it and Ctrl-C leave the
/// mode outright from either, because a person who wants out of a mode wants out of the mode.
pub(super) fn watching_key(session: &mut Session, key: KeyEvent) -> Action {
    // Ctrl alone: a settings file can give Ctrl-Alt-U to an action, and that is not the view's key.
    let ctrl = key.modifiers == KeyModifiers::CONTROL;
    let listing = session.listing_delegates();

    match key.code {
        // Out of the mode entirely, from wherever they are. Ctrl-C does nothing else here: the
        // view is the nearest thing there is to stop, and somebody who went to look at what a
        // delegate was doing is not asking for the turn to end when they come back out.
        //
        // The chord that opened the view is read off the bindings, and before every other arm: the
        // arm below refuses every chord but the view's own, so one moved onto `alt-j` would do
        // nothing instead of leaving.
        _ if session.bindings().is_watch(&key) => {
            session.stop_watching();
            Action::Redraw
        }
        KeyCode::Char('c') if ctrl => {
            session.stop_watching();
            Action::Redraw
        }

        // Half a screen, and a whole one back, in the scroller's chords (SCROLL-3). They are the
        // view's own, so they are read before the arm that refuses every other chord.
        KeyCode::Char('u') if ctrl => {
            session.scroll_up(session.half_screen());
            Action::Redraw
        }
        KeyCode::Char('d') if ctrl => {
            session.scroll_down(session.half_screen());
            Action::Redraw
        }
        KeyCode::Char('b') if ctrl => {
            session.scroll_up(session.whole_screen());
            Action::Redraw
        }

        // Every other chord does nothing. The arms below match the key and not what is held with
        // it, and a chord read as the key it carries is a key nobody gave the view: the Ctrl-L an
        // action was moved off of would still open the delegate the list is on. Shift is let
        // through, since it is how a terminal spells Shift-Tab.
        _ if !key.modifiers.difference(KeyModifiers::SHIFT).is_empty() => Action::None,

        // Back one level, or out where there is no level to go back to.
        KeyCode::Char('q') | KeyCode::Esc => {
            if listing || !session.list_delegates() {
                session.stop_watching();
            }
            Action::Redraw
        }

        // Into the delegate the list is on, or out to the conversation where the highlight is on
        // the session. The session is a row like the others, so the key that opens a row opens it.
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') if listing => {
            if session.listing_on_the_session() {
                session.stop_watching();
            } else {
                session.open_watched();
            }
            Action::Redraw
        }

        // Through the list, in the order they were spawned. The same keys move the highlight in
        // the list and the delegate in the view, since both are asking for the next one.
        KeyCode::Down | KeyCode::Char('j') if listing => {
            session.watch_next();
            Action::Redraw
        }
        KeyCode::Up | KeyCode::Char('k') if listing => {
            session.watch_previous();
            Action::Redraw
        }

        // Stops the delegate the view is on, and nothing else: the turn and the other delegates go
        // on (DELEGATE-25). Read only in a delegate's own view, so a person on the list or on a
        // command's view who presses it does nothing.
        KeyCode::Char('x') if !listing => {
            if session.stop_watched_delegate() {
                Action::Redraw
            } else {
                Action::None
            }
        }

        // Between delegates without going back to the list, for somebody comparing two runs.
        KeyCode::Char('n') | KeyCode::Right | KeyCode::Tab => {
            session.watch_next();
            Action::Redraw
        }
        KeyCode::Char('p') | KeyCode::Left | KeyCode::BackTab => {
            session.watch_previous();
            Action::Redraw
        }

        // Back through what a delegate has done, in the keys the scroller answers, since a person
        // arriving here has already learned those.
        KeyCode::Up | KeyCode::Char('k') => {
            session.scroll_up(1);
            Action::Redraw
        }
        KeyCode::Down | KeyCode::Char('j') => {
            session.scroll_down(1);
            Action::Redraw
        }
        KeyCode::PageUp | KeyCode::Char('b') => {
            session.scroll_up(session.whole_screen());
            Action::Redraw
        }
        KeyCode::PageDown | KeyCode::Char(' ') => {
            session.scroll_down(session.whole_screen());
            Action::Redraw
        }

        // Nothing else does anything, and nothing else reaches the box either.
        _ => Action::None,
    }
}

/// What a key does to the suggestion in the empty box, if one is on offer (INPUT-43).
///
/// Tab or Right, pressed bare, moves it into the box as text; the key is spent doing so and the
/// line is not otherwise edited. Any other key takes the suggestion away and is then read as it
/// always was, so typing over an empty box is no different with one on offer. A modifier pressed
/// on its own is not a key anybody typed, and leaves it standing. `None` where the key goes on.
fn suggestion_key(session: &mut Session, key: KeyEvent) -> Option<Action> {
    session.suggestion()?;
    if matches!(key.code, KeyCode::Tab | KeyCode::Right) && key.modifiers.is_empty() {
        session.accept_suggestion();
        return Some(Action::None);
    }
    if !matches!(key.code, KeyCode::Modifier(_)) {
        session.dismiss_suggestion();
    }
    None
}

/// The answer to a key that a mode standing over the session takes, and `None` where no mode is
/// open.
///
/// A mode owns every key before anything edits the line: while a picker is open, a delegate is
/// being watched, the scroller is open or a history search is, there is no line being edited, and a
/// letter belongs to the mode rather than to a box nobody can see. Checked in that order.
fn claimed_by_a_mode(session: &mut Session, key: KeyEvent) -> Option<Action> {
    if session.picking().is_some() {
        session.press_in_picking(key.code, key.modifiers);
        return Some(Action::Redraw);
    }
    if session.watching().is_some() {
        return Some(watching_key(session, key));
    }
    if session.scrolling() {
        return Some(scroller_key(session, key));
    }
    if session.searching_history() {
        return Some(history_search_key(session, key));
    }
    None
}

/// Interpret a key press against the session.
///
/// Separated from the loop so it can be tested without a terminal.
pub fn handle_key(session: &mut Session, key: KeyEvent) -> Action {
    // Before the key edits anything: a brief whose box was emptied is no longer being edited.
    session.drop_a_wiped_handoff();
    // At rest, so the modes are the whole of what stands between the key and the box.
    if let Some(action) = claimed_by_a_mode(session, key) {
        return action;
    }

    // After the modes above, which claim every key of their own, and before anything that edits the
    // line: a suggestion is taken by Tab or Right, and any other key removes it (INPUT-43).
    if let Some(action) = suggestion_key(session, key) {
        return action;
    }

    // After the modes above, which claim every key of their own, and before everything that reads
    // one: in vi's NORMAL mode three letters spell keys answered further down, and what they reach is
    // not the line. `k` and `j` walk the rows of a paragraph and then the prompt history and then the
    // transcript; `/` opens the search Ctrl-R opens. Translated to the key rather than answered a
    // second time, so a letter cannot come to disagree with the chord it stands for.
    let key = spelled_by_vi(session, key).unwrap_or(key);

    // Read before the instruction below is abandoned, since whether Escape is the box's or the
    // turn's depends on whether one is waiting.
    let escape_stops_the_turn = session.status == Status::Working
        && is_escape(session, key)
        && stops_the_turn(session, key);

    // A press that is not a character cannot be the key a half-typed vi instruction waits for. Left
    // standing, the wait would take the next letter instead, so `d`, Left, `w` would delete a word
    // from wherever the arrow had put the caret.
    if !types_a_character(key) {
        session.abandon_half_typed();
    }

    // Ctrl without Alt: this is only read by the arms for Ctrl-C and Ctrl-D, and Ctrl-Alt-C and
    // Ctrl-Alt-D are chords a settings file can give to an action (INPUT-32).
    let ctrl = holds_ctrl_without_alt(key.modifiers);

    // The hint offering the way out lives for one press, and this is it. Cleared before the arms
    // rather than after, so the Ctrl-C that puts it up survives its own press.
    session.cleared_by_interrupt = false;

    // The offer to leave outlives its own press, because the press that takes it is the next one.
    // Any other key withdraws it: somebody who went and did something else has moved on, and an
    // offer still standing then would let one later byte end the session. Both keys that leave keep
    // it, so the second press of either is the one that goes.
    if !(ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'))) {
        session.offered_to_leave = false;
    }

    // Before the match, since a key that moves the caret cannot also be one of the keys below:
    // the ones this answers are exactly the ones nothing else claims.
    if let Some(action) = change_history_scope(session, key) {
        return action;
    }
    if !session.bindings().claims(&key) && edit_line(session, key) {
        return Action::Redraw;
    }

    match key.code {
        // Ctrl-C is read against what there is to stop, nearest first: the turn in flight, then
        // the line in the box, then the loop that keeps sending prompts, then the session. A
        // person who wants the answer to stop gets that from the first press, and nothing they
        // were part way through writing is taken with it.
        KeyCode::Char('c') if ctrl && session.status == Status::Working => Action::Cancel,
        KeyCode::Char('c') if ctrl && !session.input().is_empty() => {
            session.clear_input();
            // Said only here. A press that leaves is not one to explain, and the hint is the
            // answer to what a person has just done rather than standing advice.
            session.cleared_by_interrupt = true;
            // The hint says the next press leaves, so it has to: the table's line-taking rung also
            // offers the way out. Held to the guard the rung that leaves is held to, since a run of
            // bytes is one press however many keys it holds and must not arm what the next takes.
            session.offered_to_leave = session.key_arrived_alone;
            Action::Redraw
        }
        // Before leaving, because a loop is a thing still happening and leaving is what a person
        // reaches for when nothing else will stop it. Reversed, the key that ends a loop would be
        // the key that ends the session, and there would be no way to keep one without the other.
        KeyCode::Char('c') if ctrl && session.looping().is_some() => {
            session.stop_loop();
            Action::Redraw
        }
        // For the reason the loop is here: a goal is a thing still happening, and a person
        // reaching for the key that stops things must be able to stop it without leaving.
        KeyCode::Char('c') if ctrl && session.goal().is_some() => {
            session.clear_goal();
            Action::Redraw
        }
        // The last rung before leaving, for the reason the loop and the goal are rungs at all:
        // every live watch is a thing still happening, and somebody pressing the key that stops
        // things wants them stopped. Taking all of them rather than one, because picking which of
        // eight survived is not a decision to make from a keystroke.
        KeyCode::Char('c') if ctrl && !session.watches().is_empty() => {
            session.stop_watches();
            Action::Redraw
        }
        // The bottom rung, and the only one that ends the session, so it is the one rung a single
        // byte must not reach. An interrupt is one byte another program can write into the terminal,
        // and the editor that activates a virtualenv writes one (#403), so the first press offers
        // the way out and the second takes it. Every rung above this one stops something a person
        // asked for and is unchanged: those are recoverable, and this is not.
        // Neither half of the gesture is taken from a key that did not arrive on its own. A run of
        // two or more events was available in the same instant, so `\x03\x03` in one write is two
        // key events and not the two presses this asks for; without this the offer is armed and
        // taken by one write, which is the channel the rest of this fix is about. The rungs above
        // are untouched and still answer such a key, because each of them stops something and
        // stays.
        // Said rather than passed over in silence, for the reason the offer itself is said: a press
        // that does nothing and explains nothing reads as an interface that has stopped answering,
        // and a person whose two presses shared one read is exactly who is owed the account.
        KeyCode::Char('c') if ctrl && !session.key_arrived_alone => {
            session.note(t!(leave_not_pressed));
            Action::Redraw
        }
        KeyCode::Char('c') if ctrl && !session.offered_to_leave => {
            session.offered_to_leave = true;
            Action::Redraw
        }
        KeyCode::Char('c') if ctrl => {
            session.quit();
            Action::Quit
        }
        // Held to the same rule as the interrupt: end-of-transmission is also one byte, and a
        // session ended by one byte is ended by whatever could write it.
        KeyCode::Char('d') if ctrl && session.input().is_empty() && !session.key_arrived_alone => {
            session.note(t!(leave_not_pressed));
            Action::Redraw
        }
        KeyCode::Char('d') if ctrl && session.input().is_empty() && !session.offered_to_leave => {
            session.offered_to_leave = true;
            Action::Redraw
        }
        KeyCode::Char('d') if ctrl && session.input().is_empty() => {
            session.quit();
            Action::Quit
        }
        // Nothing is waiting at rest, so there is nothing to move. Answered all the same, so the
        // chord, which the box does not read, is not typed into it either.
        _ if session.bindings().is_background(&key) => Action::None,
        // Reading back through what happened, rather than typing at it. The transcript already
        // scrolls; what needs a mode is everything a person does once they are reading, since the
        // keys for it are letters and the box takes letters.
        _ if session.bindings().is_scroller(&key) => {
            session.open_scroller();
            Action::Redraw
        }
        // The other way into the history, and the one that scales: Up walks a prompt at a time,
        // which is no way to reach the hundredth. The chord every shell answers with this same
        // question, so the muscle memory is already there.
        _ if session.bindings().is_history(&key) => {
            session.open_history_search();
            Action::Redraw
        }
        // The paste that can carry a picture. Command-V is the terminal's own, goes through the
        // pty as text, and therefore drops everything that is not text; this one reads the
        // clipboard directly. Readline's quoted-insert is what the chord costs, and nobody has
        // ever wanted it here.
        _ if session.bindings().is_paste(&key) => Action::Paste,
        // The box can be moved around in now, but it is still capped at ten rows and has none of
        // what someone reaches for on a long prompt. A paragraph worth thinking about goes
        // somewhere with room instead.
        _ if session.bindings().is_editor(&key) => Action::Edit,
        // Escape stops a turn in flight where it is the turn's rather than the box's, which the loop
        // running the turn decides before any ladder. The prompt comes back for editing rather than
        // being lost.
        _ if escape_stops_the_turn => Action::Cancel,
        // In the ordinary box it then discards a half-typed prompt, and an armed shell mode is
        // something to abandon even with no line behind it: the marker is on screen, and Backspace
        // at that same caret already backs out of it. For somebody editing the way vi does it is how
        // the letters become instructions, which the shared ladder answers.
        //
        // On an empty line it does nothing at all. It used to leave, which made every press a
        // question of what was in the box: the key for abandoning a thought was the key for
        // ending the session as soon as the thought was short enough. Ctrl-C is the way out.
        KeyCode::Esc if session.editing() == crate::vim::Editing::Ordinary => {
            session.clear_input();
            Action::Redraw
        }
        // Before every arm that sends, because this is the one Enter that does not: a paragraph is
        // written in the box rather than only pasted into it, and the box grows to hold it. Shell
        // mode too, where a multi-line command is a `for` loop somebody typed.
        _ if starts_a_line(key) => {
            session.type_newline();
            Action::Redraw
        }
        // Before every arm that takes the line anywhere, shell mode and the command arms included.
        // A return that arrived with other keys was part of a write rather than a press: an editor
        // typing a virtualenv activation into the terminal ends the line with one, and taking it sent
        // a line nobody wrote to the planner and spent a turn on it (#403). The same rule the question
        // at startup keeps and the rung that leaves keeps, for the same reason (INPUT-34).
        //
        // The line stays exactly where it is, so somebody can read it and send it themselves or clear
        // it. Said rather than ignored, since a key that does nothing and explains nothing reads as an
        // interface that has stopped answering.
        KeyCode::Enter if !session.key_arrived_alone => {
            session.note(t!(return_not_pressed));
            Action::Redraw
        }
        // The brief `/handoff` wrote, as the person left it. Before the command arm, since a brief
        // that happens to begin with a slash is still the brief, and after the arm that refuses a
        // return that arrived with other keys. Shell mode is off whenever the brief is in the box.
        KeyCode::Enter if !session.shell && session.handoff_pending() => {
            match session.take_handoff() {
                Some(brief) => Action::HandOver(brief),
                None => Action::None,
            }
        }
        // Before every command arm, because in shell mode the line is a command and nothing else.
        // `/status` is a path to a program somebody might have, and `!` is how they said so.
        KeyCode::Enter if session.shell => {
            // Read before the line is taken, which leaves the mode.
            let private = session.shell_private();
            match session.submit_command() {
                Some(line) if private => Action::RunPrivately(line),
                Some(line) => Action::Run(line),
                None => Action::None,
            }
        }
        // Every command on one arm, reading the one table the set is written in, so a word the table
        // names is dispatched and never sent. Before the arm that completes a half-typed one, which
        // is what a whole word is not, and before the arm that submits, which is what the word must
        // not reach.
        //
        // The line is taken off the box before it is dispatched, the way each of these arms took it
        // before doing anything, so a command that opens a picker or ends the session does not leave
        // its own word sitting in the box behind it. Taken rather than cleared, so what the line
        // named comes with it: clearing the box left a pasted picture staged behind nothing and its
        // marker in the line being dispatched. The words a folded paste stood for are put back as
        // the line is taken, the way a line queued behind a turn is settled as it is queued, since
        // that marker is a handle only the session holding it can undo.
        KeyCode::Enter if command_typed(session.input()).is_some() => {
            let commanded = session.take_command();
            dispatch_command(session, commanded)
        }
        // A line whose first word names a file in the person's prompt directory: the file's text
        // replaces the line and waits in the box (CMD-17). After every command, so a file cannot
        // shadow one, and before completion, so `/review the diff` is not read as a half-typed word.
        KeyCode::Enter if session.expand_prompt_file() => Action::Redraw,
        // A half-typed command, after every arm that recognises a whole one. Enter takes the
        // highlighted row rather than sending "/mod" to the planner, which is never what was meant.
        //
        // A reference only completes while it is still unfinished. A prompt ending in one that
        // already names a file is a finished sentence, and Enter has to send it: completing there
        // would leave a user pressing Enter twice to say something perfectly well formed.
        KeyCode::Enter if session.completion_would_change_the_line() => {
            session.accept_completion();
            Action::Redraw
        }
        KeyCode::Enter => match session.submit() {
            Some(prompt) => Action::Submit(prompt),
            None => Action::None,
        },
        _ => navigate(session, key),
    }
}

/// The keys that mean the same thing whether or not a turn is running.
///
/// Everything a person does to the line they are composing and to their view of what has already
/// happened: editing it, completing it, walking back through what they have said before, and
/// scrolling. None of it sends anything, and sending is the whole of what a running turn refuses,
/// so none of it has any reason to ask whether one is running.
///
/// One ladder rather than one per caller, because there were two and they drifted. Mid-turn, Up
/// and Down reached no arm at all and fell through to nothing, so a person who could see their
/// last prompt in the transcript could not recall it into the box, and the keys that scroll did
/// not scroll either. `handle_paste_while_working` was already named for this reason; this is the
/// same lesson in the same file.
fn navigate(session: &mut Session, key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Backspace => {
            session.backspace();
            Action::Redraw
        }
        // A thought to come back to, set aside without being sent and without being lost. One key
        // both ways, because what it does is read off the line: there is nothing for a person to
        // remember about which press was which.
        //
        // In the shared ladder, so it works while a turn runs like everything else that writes the
        // box: it sends nothing, and sending is the whole of what a running turn refuses. Putting a
        // half-written thought away is most wanted exactly when a turn is in flight and a better
        // one has just occurred to somebody.
        //
        // It reaches this process at all because raw mode turns off the terminal's flow control,
        // where this chord is the byte that freezes the screen.
        //
        // Except on a prompt walked back to, where the line in the box is one the history put
        // there: putting it away stores a second copy of a stored prompt, and the search is what
        // somebody who has walked back at all is looking for. Before the arm below, since that one
        // answers every line.
        _ if session.bindings().is_stash(&key) && session.history.on_a_sent_prompt() => {
            session.open_history_search_here();
            Action::Redraw
        }
        _ if session.bindings().is_stash(&key) => {
            session.stash();
            Action::Redraw
        }
        // In the shared ladder so it works mid-turn, like every other key that edits the line
        // rather than sends it. Reaches vi's own undo stack as well as the ordinary box's, so a
        // change already on it comes back in INSERT mode without first pressing Escape to reach
        // `u` from NORMAL mode; `undo_last_change` says whether there was a step to put back.
        _ if session.bindings().is_undo(&key) => {
            if session.undo_last_change() {
                Action::Redraw
            } else {
                Action::None
            }
        }
        // What a turn did, drawn per entry as the turn fills in. In the shared ladder because it
        // sets a render flag and sends nothing, and because the trail is *for* watching a turn:
        // refused mid-turn, a person who wanted to see which tools a turn was calling had to wait
        // for it to finish before they were allowed to ask.
        _ if session.bindings().is_trail(&key) => {
            session.toggle_trail();
            Action::Redraw
        }
        // In the shared ladder for the trail's reason: it changes what is drawn and sends nothing,
        // and the plan it shows is most worth reading while a turn is working through it.
        _ if session.bindings().is_panel(&key) => {
            session.toggle_panel();
            Action::Redraw
        }
        // What a delegate is doing, which is drawn nowhere else in full. In the shared ladder
        // because it sends nothing, and wanted mid-turn above all: a delegate exists only while a
        // turn runs.
        //
        // Nothing at all where this session has spawned none. A key that does nothing is better
        // than a screen with nothing on it, and the shortcut list is where its meaning lives.
        _ if session.bindings().is_watch(&key) => {
            if session.watch() {
                Action::Redraw
            } else {
                Action::None
            }
        }
        // Before the Tab arm below, since a terminal that has been asked to disambiguate reports
        // this as Tab with a modifier rather than as its own code, and it would otherwise be taken
        // for a completion. Both spellings, because which one arrives is the terminal's choice.
        //
        // In the shared ladder so it works mid-turn, which is when it is most wanted: a person
        // watching a turn edit files it should not be stops it with this key. The turn in flight
        // reads the session's handle on the mode, so it follows the last press (MODE-8).
        _ if cycles_the_mode(key) => {
            session.cycle_permission_mode();
            Action::Redraw
        }
        // Tab completes, which is what it does everywhere else. Only while a command is being
        // typed: with nothing offered it inserts nothing, rather than a stray character.
        KeyCode::Tab if session.is_completing() => {
            session.accept_completion();
            Action::Redraw
        }
        // The level after `/effort `, which the list above does not offer since the command is
        // already settled once a space follows it.
        KeyCode::Tab if session.argument_completion().is_some() => {
            session.accept_argument_completion();
            Action::Redraw
        }
        // While the list is open the arrows walk it. History and scrolling get them back the moment
        // it closes, which is as soon as the line stops being a lone half-typed command.
        KeyCode::Up if session.is_completing() => {
            session.previous_completion();
            Action::Redraw
        }
        KeyCode::Down if session.is_completing() => {
            session.next_completion();
            Action::Redraw
        }
        // A pasted paragraph has rows of its own, and moving between them is what these keys mean
        // inside it. Only while there is a row to move to: at the top of the line they go back to
        // the history below, the way they do in a shell.
        KeyCode::Up if session.is_multiline() && session.move_up_a_line() => Action::Redraw,
        KeyCode::Down if session.is_multiline() && session.move_down_a_line() => Action::Redraw,
        // What is waiting comes back before anything older does. The queue holds the most recent
        // thing the person said, and the history holds a copy of every line in it, so Up handed
        // back a copy of a prompt that was still going to be sent: the person edited the copy,
        // and the original went anyway. Taking the queue back is about sending, which is the one
        // thing a running turn is allowed to differ about.
        KeyCode::Up if !session.queued.is_empty() => {
            session.unqueue();
            Action::Redraw
        }
        // Up and Down walk the prompt history, which is what they do in a shell and so what a
        // user expects at a prompt. Scrolling the transcript keeps the wheel and the page keys,
        // and Up still scrolls once there is no history left to walk.
        KeyCode::Up if session.history.can_recall() => {
            session.recall_older();
            Action::Redraw
        }
        KeyCode::Down if session.history.is_browsing() => {
            session.recall_newer();
            Action::Redraw
        }
        KeyCode::Up => {
            session.scroll_up(1);
            Action::Redraw
        }
        KeyCode::Down => {
            session.scroll_down(1);
            Action::Redraw
        }
        // The page keys walk the prompt first: the start of this line, then the line before, and
        // the same downwards. Only while there is somewhere to go, so at the ends of the prompt they
        // fall through to the transcript below, which is what they did before there was a caret.
        KeyCode::PageUp if session.page_up() => Action::Redraw,
        KeyCode::PageDown if session.page_down() => Action::Redraw,
        KeyCode::PageUp => {
            session.scroll_up(10);
            Action::Redraw
        }
        KeyCode::PageDown => {
            session.scroll_down(10);
            Action::Redraw
        }
        // Jump to either end of the transcript. Under Ctrl, because bare Home and End belong to
        // the line being typed: they were the transcript's before there was a caret to move.
        KeyCode::Home if ctrl => {
            session.scroll_up(u16::MAX);
            Action::Redraw
        }
        KeyCode::End if ctrl => {
            session.scroll_down(u16::MAX);
            Action::Redraw
        }
        // How the letters become instructions, for somebody editing the way vi does, in both of the
        // spellings [`is_escape`] answers. The line is untouched: throwing a paragraph away is
        // Ctrl-C's job, and a key that did both would be one nobody could press safely. In NORMAL
        // mode already it is claimed and does nothing, which is what it does in every vi.
        //
        // In the shared ladder, so it works while a turn runs like everything else that only moves
        // the caret. Mid-turn it arrives here only while it is the box's: from NORMAL mode with
        // nothing waiting it stops the turn, which the loops decide before any ladder is reached.
        //
        // Before the catch-all below, which would otherwise swallow Ctrl-`[` as an unclaimed control
        // chord.
        _ if session.editing() == crate::vim::Editing::Vi && is_escape(session, key) => {
            session.enter_vi_normal();
            Action::Redraw
        }
        // Any other control combination is ignored rather than typed. Without this,
        // Ctrl-D on a non-empty line falls through and inserts a literal 'd'.
        KeyCode::Char(_) if ctrl => Action::None,
        KeyCode::Char(c) => {
            session.type_char(c);
            Action::Redraw
        }
        _ => Action::None,
    }
}

/// Interpret a key press while a turn is running.
///
/// Only the ones that cannot start anything. What the user types goes into the box and stays
/// there, and the keys that look back through the transcript still work. Enter is not among them:
/// a second turn must not begin while the first is in flight, so the line waits, and sending it
/// is the first thing available when the turn ends.
///
/// Everything used to be dropped here, cancel and mouse aside. A user typing during a slow turn
/// therefore watched their words go nowhere, with nothing on the screen to say why, which is
/// indistinguishable from an interface that has stopped responding.
pub fn handle_key_while_working(session: &mut Session, key: KeyEvent) -> Action {
    // None of the modes sends anything, and sending is the whole of what a running turn refuses.
    // Mid-turn is also when a delegate is there to watch, since one does not outlive the turn that
    // spawned it, and when the prompt somebody wants back is most likely one they can no longer
    // see: the turn they are watching has filled the screen.
    if let Some(action) = claimed_by_a_mode(session, key) {
        return action;
    }

    // The same translation the idle path makes, in the same place: after the modes that claim every
    // key of their own and before anything reads one. The letters vi spells these with reach the
    // prompt history and the search, neither of which sends anything, so a running turn refuses
    // nothing here.
    let key = spelled_by_vi(session, key).unwrap_or(key);

    // Where the idle path abandons it, and for the reason that path does.
    if !types_a_character(key) {
        session.abandon_half_typed();
    }

    // Where the idle path clears it, and for the reason that path clears it: the hint offering the
    // way out lives for one press, and this is it. A turn a loop tick or a watch began started
    // from the main loop with no key press, so an offer put up while the session was idle is still
    // up when this path takes over, and a press that did not take it down would leave it standing
    // frame after frame saying the next Ctrl-C leaves, which mid-turn stops the turn instead.
    session.cleared_by_interrupt = false;

    // Before the modifier guard, since the readline bindings are how the caret moves on a terminal
    // that sends nothing for the named keys, and a line that can be typed mid-turn has to be
    // editable mid-turn: the alternative is a box that takes words and will not let them be fixed.
    if let Some(action) = change_history_scope(session, key) {
        return action;
    }
    if !session.bindings().claims(&key) && edit_line(session, key) {
        return Action::Redraw;
    }

    // Before the modifier guard, since one of its two spellings is a Ctrl chord. A paragraph can be
    // written while a turn runs, like everything else typed here; plain Enter is still refused.
    if starts_a_line(key) {
        session.type_newline();
        return Action::Redraw;
    }

    // Before the modifier guard, since a line that can be typed mid-turn can be pasted into
    // mid-turn: what is refused while a turn runs is sending, never writing.
    if session.bindings().is_paste(&key) {
        return Action::Paste;
    }

    // Before the modifier guard for the same reason, and mid-turn is when it is wanted most: a
    // person reading back through a turn that is going wrong is reading because it is going wrong.
    if session.bindings().is_scroller(&key) {
        session.open_scroller();
        return Action::Redraw;
    }

    // Before the modifier guard for the same reason. Searching the history sends nothing, and the
    // prompt somebody wants back mid-turn is the one the turn they are watching came from.
    if session.bindings().is_history(&key) {
        session.open_history_search();
        return Action::Redraw;
    }

    // Answered whether or not a command can be moved, so the chord never falls through to the
    // ladder and moves the caret instead: a person pressing it a moment after the command ended
    // asked for nothing the box should do. With nothing moved it is answered as the idle path
    // answers it.
    if session.bindings().is_background(&key) {
        return if session.move_to_background() {
            Action::Redraw
        } else {
            Action::None
        };
    }

    // For the reason the idle ladder refuses first: queueing is sending with a wait in front of it,
    // so a return another program wrote would reach the planner when the turn in flight ended.
    if key.code == KeyCode::Enter && !session.key_arrived_alone {
        session.note(t!(return_not_pressed));
        return Action::Redraw;
    }

    // Before every arm that queues anything else, because in shell mode the line is a command line
    // and nothing else, which is the order the idle ladder answers the two in: `/status` there is a
    // path to a program somebody may have. The line waits, as every line Enter is pressed on
    // mid-turn waits, and what it waits to be is what the person armed the mode to make it. What it
    // is spared is the running turn: handing a command line to the planner would put `echo pwned`
    // in the conversation as a sentence somebody said.
    //
    // Reachable because a turn can begin with nobody pressing anything, from a loop's tick or a
    // watch firing, which leaves the mode armed over a line that was typed at rest to be run.
    if key.code == KeyCode::Enter && session.shell && session.queue_shell() {
        return queued(session, key);
    }

    // Before the arm that queues a command, because these commands need not wait: each reads or
    // changes only what the session keeps for itself, a loop, a goal, a watch, the spend so far, its
    // name, a level or a theme, or what is kept for the next session here, and the turn holds none
    // of it (CMD-8). A command that only reads goes ahead of everything queued, so `/cost` typed
    // behind a waiting prompt answers for the moment it was typed. What it says is held off the
    // transcript until the turn ends, since the transcript is how a stopped turn tells whether it
    // did anything. A command that needs more than the session comes back as its action, for the
    // turn's loop to finish.
    //
    // A turn only. The other loops that run while the session works, a compaction among them, keep
    // every command waiting. Ctrl-Enter here stops nothing: the command has already been carried
    // out, and stopping the turn would send prompts waiting behind it that nobody asked to hurry.
    if key.code == KeyCode::Enter
        && !session.shell
        && session.a_turn_is_running()
        && runs_while_working(session.input())
        && !waits_behind_the_queue(session, session.input())
    {
        let commanded = session.take_command();
        return session.answer_while_working(|session| dispatch_command(session, commanded));
    }

    // Before the arm that queues a prompt, because the two do the same thing to the box and differ
    // only in what is waiting afterwards. This line waits to be carried out; the queue is what a
    // person typing mid-turn already understands, so a command joins it rather than sitting in the
    // box asking to be pressed again. What it is spared is the running turn: a queued command is
    // never offered to it, so the planner is never asked what to clear.
    //
    // Not in shell mode, where the line is a command line and `/status` is a path to a program, for
    // the reason the arm above answers that mode first.
    if key.code == KeyCode::Enter
        && !session.shell
        && command_typed(session.input()).is_some()
        && session.queue_command()
    {
        return queued(session, key);
    }

    // A prompt file expands mid-turn as it does at rest, into the box, so the text is read before it
    // is queued with the Enter that follows (CMD-17). Nothing is sent by this press.
    if key.code == KeyCode::Enter
        && session.answer_while_working(|session| session.expand_prompt_file())
    {
        return Action::Redraw;
    }

    // After the arms that recognise a whole command, as at rest: Enter on a half-typed word takes
    // the highlighted row rather than queueing "/mod" as a prompt.
    if key.code == KeyCode::Enter && session.completion_would_change_the_line() {
        session.accept_completion();
        return Action::Redraw;
    }

    // After the arm that starts a line, so Shift-Enter still writes a paragraph, and before the
    // ladder, where Enter means nothing. A second turn still must not begin while the first is in
    // flight; what changes is that the line no longer waits in the box for the person to notice
    // the turn has ended and press Enter again.
    if key.code == KeyCode::Enter && session.queue() {
        return queued(session, key);
    }

    // Over an empty box, where there was nothing to queue and what is already waiting can go all
    // the same.
    if sends_now(key) && session.hurry() {
        return Action::SendNow;
    }

    // Refused here rather than left to fall through the ladder's catch-all for control chords. The
    // editor takes the terminal for a child process, which is the screen the turn is drawing on, and
    // the line it hands back would be waiting for a box that has moved on. That is a decision about
    // this key, and a key that does nothing by accident reads the same as one that does nothing on
    // purpose.
    if session.bindings().is_editor(&key) {
        return Action::None;
    }

    // The same ladder the idle path uses, rather than a shorter copy of it. Nothing in it sends,
    // so there is nothing here for a running turn to refuse.
    navigate(session, key)
}

/// What a press that has just queued the line asks for next.
///
/// Enter asks for nothing more. Ctrl-Enter queues the line the same way, so a line typed with it
/// joins what goes, and then asks for the turn to stop.
fn queued(session: &mut Session, key: KeyEvent) -> Action {
    if sends_now(key) && session.hurry() {
        return Action::SendNow;
    }
    Action::Redraw
}

/// Interpret a mouse event.
///
/// The wheel is what most people reach for first, so it scrolls without any modifier.
///
/// Dragging selects. Capturing the mouse for the wheel is what took the terminal's own selection
/// away, so the drag that would have highlighted a line arrives here instead, and answering it
/// is the only way a user gets to copy anything.
pub fn handle_mouse(session: &mut Session, mouse: MouseEvent) -> Action {
    match mouse.kind {
        // While the scroller is open the wheel is one of its keys, so it stops at the first row
        // and the last the way every other movement in that mode does. It takes no count, so it
        // drops one, as any other key would. How far it goes is what `tui.wheelRows` named, which
        // is the same distance it moves at rest.
        MouseEventKind::ScrollUp if session.scrolling() => {
            session.take_the_scroller_count();
            session.scroller_back(session.wheel_rows());
            Action::Redraw
        }
        MouseEventKind::ScrollDown if session.scrolling() => {
            session.take_the_scroller_count();
            session.scroller_on(session.wheel_rows());
            Action::Redraw
        }
        MouseEventKind::ScrollUp => {
            session.scroll_up(session.wheel_rows());
            Action::Redraw
        }
        MouseEventKind::ScrollDown => {
            session.scroll_down(session.wheel_rows());
            Action::Redraw
        }
        MouseEventKind::Down(MouseButton::Left) => {
            session.begin_selection(mouse.row, mouse.column);
            Action::Redraw
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            session.extend_selection(mouse.row, mouse.column);
            Action::Redraw
        }
        MouseEventKind::Up(MouseButton::Left) => Action::Copy,
        _ => Action::None,
    }
}

/// Whether a key press asks for whatever is in flight to stop, and nothing more.
///
/// Escape, and only Escape. Ctrl-C asks for it too, but Ctrl-C also leaves, so the loops take it
/// separately: which of the two it means depends on whether there is anything to stop. In vi's
/// style Escape can be the box's instead, which [`stops_the_turn`] decides.
pub(super) fn wants_cancel(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc)
}

/// Record what the reader just handed out, before any arm acts on it.
///
/// Two things the arms cannot work out for themselves. Whether the key arrived on its own, which only
/// the reader knows and only the rung that ends the session asks about. And whether anything at all
/// has happened since the way out was offered: the offer answers the press just made, so any other
/// input withdraws it. A mouse report or a resize means somebody is still here and has moved on, and
/// an offer left standing through one would let a byte written much later take it.
pub(super) fn took_input(session: &mut Session, taken: &TermEvent) {
    // A release is the tail of a press that has already been answered, not something somebody did
    // next, so it says nothing about either of the two things below. Windows reports a key-up for
    // every keystroke and carries the modifiers still held at that moment, so letting go of Ctrl
    // before C arrives here as a bare `c`: read as other input, it withdrew the offer that the same
    // person's press had just armed, and no press of Ctrl-C ever reached the rung that leaves.
    if input::key_of(taken).is_some_and(|key| key.kind == KeyEventKind::Release) {
        return;
    }

    session.key_arrived_alone = input::the_last_event_arrived_alone();

    let asks_to_leave = input::key_of(taken).is_some_and(|key| {
        holds_ctrl_without_alt(key.modifiers)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'))
    });
    if !asks_to_leave {
        session.offered_to_leave = false;
    }
}

/// Whether a key press is Ctrl-C.
///
/// What it asks for depends on what is happening. With a turn in flight, or a command running, it
/// stops that and stays; with nothing to stop it leaves. So this says which key was pressed and
/// the loops say what it meant, since only they know which of the two they are.
///
/// It once always left, because stopping the turn and staying had left no way out at all: Ctrl-C
/// did nothing whatever at the prompt. It leaves from the prompt now, so the press that stops a
/// turn is followed by a press that leaves, and both requests have a key again.
pub(super) fn is_ctrl_c(key: KeyEvent) -> bool {
    holds_ctrl_without_alt(key.modifiers) && matches!(key.code, KeyCode::Char('c'))
}

/// Whether Ctrl is held and Alt is not.
///
/// Ctrl-C and Ctrl-D are the chords that stop and leave (INPUT-4). Ctrl-Alt-C and Ctrl-Alt-D are
/// different chords that a settings file may give to an action (INPUT-32), so the handlers for the
/// first two must not answer the second two.
fn holds_ctrl_without_alt(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL) && !modifiers.contains(KeyModifiers::ALT)
}
