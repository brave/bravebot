//! The event loop.
//!
//! Runs turns one at a time. Each turn builds its own policy inside `turn::run`, so
//! nothing about the interface can extend a policy's life beyond the turn that created
//! it. What does outlive a turn is the conversation, which is how a follow-up like "try that
//! again" has anything to refer to.
//!
//! Turns run synchronously: the interface shows "working" and stops accepting input until
//! the reply arrives. That is honest about what is happening, and it keeps two turns from
//! ever being in flight together.

use bravebot_agent::conversation::Conversation;
use bravebot_agent::lsp::LanguageServers;
use bravebot_agent::turn::{self, PastedImage, Task};
use bravebot_agent::{SessionScratch, Workspace};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::permissions::Permissions;
use bravebot_core::programs::{AskedAbout, TrustedPrograms};
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use bravebot_net::Egress;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    supports_keyboard_enhancement,
};
use std::io::{self, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::audit::{Stamped, Trail};
use crate::render;
use crate::select;
use crate::state::{Session, Status};

/// How long to wait for a key before redrawing. Short enough that a status change appears
/// promptly, long enough not to spin.
const POLL: Duration = Duration::from_millis(100);

/// Asks for motion reported only while a button is held.
///
/// Sent after [`EnableMouseCapture`], which asks for all three tracking modes at once, including
/// the one that reports a pointer merely crossing the window. Turning that one off is the whole
/// intent, but terminals disagree about what the three modes are: some keep a flag per mode and
/// use the highest one set, others keep a single state that the last request wins. Sending the
/// two that are wanted again, after the one that is not, lands both kinds in the same place.
/// Without that, a terminal of the second kind reads it as "no tracking at all" and the wheel
/// goes back to scrolling the window behind the session.
const TRACK_MOTION_ONLY_WHILE_DRAGGING: &str = "\x1b[?1003l\x1b[?1000h\x1b[?1002h";

/// How often to redraw while a turn runs. Matches the spinner's own frame time so the animation
/// advances by one glyph per redraw rather than skipping.
const FRAME: Duration = Duration::from_millis(120);

/// The line that opens the model picker instead of starting a turn.
const MODEL_COMMAND: &str = "/model";

/// The line that opens the theme picker, or applies a theme named after the word.
const THEME_COMMAND: &str = "/theme";

/// The line that opens the effort picker, or takes a level named after the word.
const EFFORT_COMMAND: &str = "/effort";

/// The line that opens the panel of preferences about the interface itself.
const CONFIG_COMMAND: &str = "/config";

/// The line that opens another directory, taking the path to open as its argument.
const ADD_DIR_COMMAND: &str = "/add-dir";

/// The line that moves the session to another working directory, taking the path as its argument.
const CD_COMMAND: &str = "/cd";

/// The line that reports what this session is and what it may touch.
const STATUS_COMMAND: &str = "/status";

/// The line that summarises the conversation so far, in place of sending all of it.
const COMPACT_COMMAND: &str = "/compact";

/// The line that starts a new session in place of this one.
const CLEAR_COMMAND: &str = "/clear";

/// The line that renames this session, taking the new name as its argument.
const RENAME_COMMAND: &str = "/rename";

/// The line that asks a question beside the work, taking the question as its argument.
const BTW_COMMAND: &str = "/btw";

/// The line that repeats a prompt, taking the prompt and any interval as its argument.
const LOOP_COMMAND: &str = "/loop";

/// The line that sets the condition a session works towards, taking that condition as its
/// argument.
const GOAL_COMMAND: &str = "/goal";

/// The line that lists the standing watches, and ends one by its number.
///
/// It cannot arm one. A watch is asked for in a prompt, and what a person needs a command for is
/// the half they cannot read off the transcript: which watches are live, and how to end one.
const WATCH_COMMAND: &str = "/watch";

/// The one line that ends the session instead of starting a turn.
const EXIT_COMMAND: &str = "/exit";

/// The line that writes the transcript as a markdown file.
const EXPORT_COMMAND: &str = "/export";

/// The line that rewinds the conversation and restores files changed in the last turn.
const UNDO_COMMAND: &str = "/undo";

/// The line that lists what a rewind would put back, and goes back to one of those points.
///
/// A word of its own rather than an argument to `/undo`, because `/undo` deliberately takes none:
/// the table's empty argument column is what keeps `/undo the last thing I asked for` a prompt
/// (CMD-2), and a command that takes a number cannot also do that.
const REWIND_COMMAND: &str = "/rewind";

/// The line that plans one task in full before anything is read, taking the task as its argument.
///
/// Not a mode the session holds. A session is several turns over one conversation and this is one
/// run with a frozen plan, so the word starts a run and the session comes back to the turn loop
/// when it ends. See [`manifest_animated`] and `docs/specs/manifest.md`.
const MANIFEST_COMMAND: &str = "/manifest";

/// One command, and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// The word typed, including the leading slash.
    pub name: &'static str,
    /// What it takes after the word, or empty where it takes nothing.
    pub argument: &'static str,
    /// One line, for the list shown while a command is being typed.
    pub description: &'static str,
}

/// Every command, in the order they are offered.
///
/// The one place they are written down. The hint line, the completion list and the key handler all
/// read from here, so a command that is renamed or added cannot leave any of them advertising
/// something that no longer works.
pub fn commands() -> [Command; 19] {
    [
        Command {
            name: STATUS_COMMAND,
            argument: "",
            description: t!(command_status),
        },
        Command {
            name: MODEL_COMMAND,
            argument: "",
            description: t!(command_model),
        },
        Command {
            name: THEME_COMMAND,
            argument: "[name]",
            description: t!(command_theme),
        },
        Command {
            name: EFFORT_COMMAND,
            argument: "[level]",
            description: t!(command_effort),
        },
        Command {
            name: CONFIG_COMMAND,
            argument: "",
            description: t!(command_config),
        },
        Command {
            name: ADD_DIR_COMMAND,
            argument: "<path>",
            description: t!(command_add_dir),
        },
        Command {
            name: CD_COMMAND,
            argument: "<path>",
            description: t!(command_cd),
        },
        Command {
            name: RENAME_COMMAND,
            argument: "<name>",
            description: t!(command_rename),
        },
        Command {
            name: COMPACT_COMMAND,
            argument: "",
            description: t!(command_compact),
        },
        Command {
            name: BTW_COMMAND,
            argument: "<question>",
            description: t!(command_btw),
        },
        Command {
            name: CLEAR_COMMAND,
            argument: "",
            description: t!(command_clear),
        },
        Command {
            name: LOOP_COMMAND,
            argument: "[interval] <prompt>",
            description: t!(command_loop),
        },
        Command {
            name: GOAL_COMMAND,
            argument: "[<condition> | clear]",
            description: t!(command_goal),
        },
        Command {
            name: WATCH_COMMAND,
            argument: "[stop <n>]",
            description: t!(command_watch),
        },
        Command {
            name: MANIFEST_COMMAND,
            argument: "<task>",
            description: t!(command_manifest),
        },
        Command {
            name: EXPORT_COMMAND,
            argument: "[path]",
            description: t!(command_export),
        },
        Command {
            name: UNDO_COMMAND,
            argument: "",
            description: t!(command_undo),
        },
        Command {
            name: REWIND_COMMAND,
            argument: "[turns]",
            description: t!(command_rewind),
        },
        Command {
            name: EXIT_COMMAND,
            argument: "",
            description: t!(command_exit),
        },
    ]
}

/// The commands a half-typed line could still become, in the order they are offered.
///
/// Empty unless the line is a lone word starting with a slash: a command takes its argument after a
/// space, so once there is one the command is settled and there is nothing left to complete. A
/// line that is not a command at all completes to nothing, which is what closes the list.
pub fn completions(line: &str) -> Vec<Command> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('/') || trimmed.contains(char::is_whitespace) {
        return Vec::new();
    }
    commands()
        .iter()
        .filter(|command| command.name.starts_with(trimmed))
        .copied()
        .collect()
}

/// The argument given to `command`, if that is what the line is.
///
/// `None` for anything else, so a prompt that merely mentions the word is still a prompt, and so is
/// a longer word that happens to start with it. The bare command with no argument is `Some("")`,
/// which the caller answers by saying what it needs rather than silently doing nothing.
fn argument_to<'a>(line: &'a str, command: &str) -> Option<&'a str> {
    let rest = line.trim().strip_prefix(command)?;
    if rest.is_empty() {
        return Some("");
    }
    // A following character that is not a space means a longer word, not this command.
    rest.strip_prefix(' ').map(str::trim)
}

/// The command word the line is, or `None` where the line is a prompt.
///
/// Read off the table rather than word by word, because the question is only which command was
/// typed and not what it does. A caller deciding what to do about one needs an arm each; a caller
/// that has only to recognise one gets every word the table names, so a command added there is
/// recognised here without anybody having to remember to.
///
/// A command the table gives no argument is only ever itself: the ladder dispatches those on the
/// bare word, so `/undo the last change` is a prompt there and has to be one here too. The
/// argument the table names is what says which of the two a word is, so the two agree by reading
/// the same column rather than by anybody keeping two lists in step.
fn command_typed(line: &str) -> Option<&'static str> {
    commands()
        .into_iter()
        .find(|command| match argument_to(line, command.name) {
            Some(argument) => !command.argument.is_empty() || argument.is_empty(),
            None => false,
        })
        .map(|command| command.name)
}

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
    /// Take what the selection covers, which needs the screen as it was last drawn.
    Copy,
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
    /// Ask how the box should edit. Needs the terminal, so the loop runs it.
    ChooseEditing,
    /// Open another directory. Needs the workspace and the trust map, which the loop owns.
    AddDirectory(String),
    /// Work somewhere else from now on. Needs the workspace, the trust map and the session
    /// record, all of which the loop owns.
    ChangeDirectory(String),
    /// Summarise the conversation so far. Needs the conversation and the network, which the loop
    /// owns.
    Compact,
    /// Ask something beside the work, over a copy of the conversation. Needs the conversation and
    /// the network, which the loop owns, and gives the conversation nothing back.
    Aside(String),
    /// Plan this task in full and then walk it. Needs the workspace, the trust map and the
    /// network, which the loop owns, and gives the conversation nothing back.
    Manifest(String),
    /// Start a new session here. Needs the conversation and the session record, which the loop owns.
    Clear,
    /// Call this session something else. Needs the session record, which the loop owns.
    Rename(String),
    /// Report what this session is. Needs the workspace and the trust map, which the loop owns.
    Status,
    /// Run a command the user typed in shell mode. Needs the workspace and the conversation.
    Run(String),
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
///   is the same however it was pressed, which is why [`take_over_terminal`] asks for disambiguated
///   keys where that is understood.
/// - Ctrl-J, which is the byte `\n`. That is what a terminal configured to send a newline for
///   Shift-Enter delivers, and it is the arrangement iTerm and Terminal.app need, since neither
///   reports the modifier. It is also typeable directly on a terminal that does neither.
///
/// Ctrl-J rather than Ctrl-M: `\r` is Enter itself and binding it would break sending. Nothing else
/// may claim Ctrl-J, whatever readline does with it, since on those terminals it is the only way to
/// write a paragraph.
fn starts_a_line(key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Enter => key.modifiers.contains(KeyModifiers::SHIFT),
        KeyCode::Char('j') => key.modifiers.contains(KeyModifiers::CONTROL),
        _ => false,
    }
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
/// going wrong.
///
/// Each of the three names these keys itself, and this is what lets them have them: the loops read
/// this before any ladder, so a mode left out here is one whose way out ends the turn instead.
///
/// Named rather than written out at each loop, because there are three of them and a condition
/// copied three times is a condition that ends up meaning three things.
fn stops_the_turn(session: &Session, key: KeyEvent) -> bool {
    !session.scrolling()
        && session.watching().is_none()
        && !session.searching_history()
        && (is_ctrl_c(key) || wants_cancel(key))
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
fn scroller_key(session: &mut Session, key: KeyEvent) -> Action {
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
            KeyCode::Char('c') if ctrl => {
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

    match key.code {
        // Four keys close it. Ctrl-C is one of them and does nothing else here: the scroller is
        // the nearest thing there is to stop, so a turn in flight goes on running and the press
        // that reaches it is the next one.
        KeyCode::Char('q') if !ctrl => {
            session.close_scroller();
            Action::Redraw
        }
        // The same ladder every other stop key here walks: the nearest thing there is to stop.
        // A standing search is nearer than the mode holding it, so the highlights come off first
        // and the press after that is the one that closes the scroller.
        KeyCode::Esc => {
            if !session.clear_search() {
                session.close_scroller();
            }
            Action::Redraw
        }
        KeyCode::Char('c') if ctrl => {
            session.close_scroller();
            Action::Redraw
        }
        _ if session.bindings().is_scroller(&key) => {
            session.close_scroller();
            Action::Redraw
        }

        // A line at a time.
        KeyCode::Up | KeyCode::Char('k') => {
            session.scroller_back(1);
            Action::Redraw
        }
        KeyCode::Down | KeyCode::Char('j') => {
            session.scroller_on(1);
            Action::Redraw
        }

        // Half a screen, which is the one movement that keeps context on both sides of itself.
        KeyCode::Char('u') if ctrl => {
            session.scroller_back(session.half_screen());
            Action::Redraw
        }
        KeyCode::Char('d') if ctrl => {
            session.scroller_on(session.half_screen());
            Action::Redraw
        }

        // A whole screen, in both dialects. `b` is the same key with or without Ctrl, because
        // somebody who knows one spelling should not find the other typing a letter.
        KeyCode::Char(' ') | KeyCode::PageDown => {
            session.scroller_on(session.whole_screen());
            Action::Redraw
        }
        KeyCode::Char('f') if ctrl => {
            session.scroller_on(session.whole_screen());
            Action::Redraw
        }
        KeyCode::Char('b') | KeyCode::PageUp => {
            session.scroller_back(session.whole_screen());
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
        KeyCode::Home => {
            session.scroller_to_first_row();
            Action::Redraw
        }
        KeyCode::End => {
            session.scroller_to_last_row();
            Action::Redraw
        }

        // Turn by turn. Where these land is settled by what the person typed, since a prompt is
        // the one thing in a transcript they wrote themselves.
        KeyCode::Char('{') => {
            session.to_previous_prompt();
            Action::Redraw
        }
        KeyCode::Char('}') => {
            session.to_next_prompt();
            Action::Redraw
        }

        KeyCode::Char('/') => {
            session.begin_search();
            Action::Redraw
        }
        KeyCode::Char('n') => {
            walk_the_matches(session, true);
            Action::Redraw
        }
        KeyCode::Char('N') => {
            walk_the_matches(session, false);
            Action::Redraw
        }

        KeyCode::Char('?') => {
            session.toggle_scroller_help();
            Action::Redraw
        }
        KeyCode::Char('v') if !ctrl && session.status != Status::Working => Action::Show,

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

/// Walk to the next match, or the previous one.
///
/// The rows are the ones the last frame found, which is the frame the person is looking at while
/// they press the key. Nothing has changed about what is being looked for since it was drawn, so
/// there is nothing to lay out again.
fn walk_the_matches(session: &mut Session, forwards: bool) {
    let found = session.laid.matches.clone();
    session.to_a_match(&found, forwards);
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
    if session.bindings().is_history(&key) || (ctrl && key.code == KeyCode::Char('c')) {
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
/// back to the list, and from the list it closes. Ctrl-L and Ctrl-C leave the mode outright from
/// either, because a person who wants out of a mode wants out of the mode.
fn watching_key(session: &mut Session, key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let listing = session.listing_delegates();

    match key.code {
        // Out of the mode entirely, from wherever they are. Ctrl-C does nothing else here: the
        // view is the nearest thing there is to stop, and somebody who went to look at what a
        // delegate was doing is not asking for the turn to end when they come back out.
        //
        // The chord that opened the view is read off the bindings, and before every other arm: the
        // keys that walk the list are bare letters matched whatever is held with them, so one moved
        // onto `j` would walk the list instead of leaving.
        _ if session.bindings().is_watch(&key) => {
            session.stop_watching();
            Action::Redraw
        }
        KeyCode::Char('c') if ctrl => {
            session.stop_watching();
            Action::Redraw
        }

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
        KeyCode::Char('u') if ctrl => {
            session.scroll_up(session.half_screen());
            Action::Redraw
        }
        KeyCode::Char('d') if ctrl => {
            session.scroll_down(session.half_screen());
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

/// Interpret a key press against the session.
///
/// Separated from the loop so it can be tested without a terminal.
pub fn handle_key(session: &mut Session, key: KeyEvent) -> Action {
    // Before everything, including the keys that edit the line: while a delegate is being watched
    // there is no line being edited, and every key belongs to the mode.
    if session.watching().is_some() {
        return watching_key(session, key);
    }

    // Before everything, including the keys that edit the line: while the scroller is open there
    // is no line being edited, and the keys belong to it.
    if session.scrolling() {
        return scroller_key(session, key);
    }

    // On the same footing, and for the same reason: while a search is open every letter is
    // narrowing a list rather than being typed into a box nobody can see.
    if session.searching_history() {
        return history_search_key(session, key);
    }

    // After the modes above, which claim every key of their own, and before everything that reads
    // one: in vi's NORMAL mode three letters spell keys answered further down, and what they reach is
    // not the line. `k` and `j` walk the rows of a paragraph and then the prompt history and then the
    // transcript; `/` opens the search Ctrl-R opens. Translated to the key rather than answered a
    // second time, so a letter cannot come to disagree with the chord it stands for.
    let key = spelled_by_vi(session, key).unwrap_or(key);

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // The hint offering the way out lives for one press, and this is it. Cleared before the arms
    // rather than after, so the Ctrl-C that puts it up survives its own press.
    session.cleared_by_interrupt = false;

    // Before the match, since a key that moves the caret cannot also be one of the keys below:
    // the ones this answers are exactly the ones nothing else claims.
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
        KeyCode::Char('c') if ctrl => {
            session.quit();
            Action::Quit
        }
        KeyCode::Char('d') if ctrl && session.input().is_empty() => {
            session.quit();
            Action::Quit
        }
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
        // Escape means "stop what is happening" before it means anything else, so a turn in
        // flight is cancelled first. The prompt comes back for editing rather than being lost.
        KeyCode::Esc if session.status == Status::Working => Action::Cancel,
        // Then, for somebody editing the way vi does, it is how the letters become instructions. The
        // line is untouched: throwing a paragraph away is Ctrl-C's job, and a key that did both would
        // be one nobody could press safely. In NORMAL mode already it is claimed and does nothing,
        // which is what it does in every vi.
        //
        // The guard asks the style rather than calling the method that changes the mode, so nothing
        // here mutates the session while the arms are still being chosen between. Ctrl-`[` is the same
        // request from a terminal that reports the modifier, and the shared ladder answers that one.
        KeyCode::Esc if session.editing() == crate::vim::Editing::Vi => {
            session.enter_vi_normal();
            Action::Redraw
        }
        // Then it discards a half-typed prompt, and an armed shell mode is something to abandon
        // even with no line behind it: the marker is on screen, and Backspace at that same caret
        // already backs out of it.
        //
        // On an empty line it does nothing at all. It used to leave, which made every press a
        // question of what was in the box: the key for abandoning a thought was the key for
        // ending the session as soon as the thought was short enough. Ctrl-C is the way out.
        KeyCode::Esc => {
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
        // Before every command arm, because in shell mode the line is a command and nothing else.
        // `/status` is a path to a program somebody might have, and `!` is how they said so.
        KeyCode::Enter if session.shell => match session.submit_command() {
            Some(line) => Action::Run(line),
            None => Action::None,
        },
        // Every command on one arm, reading the one table the set is written in, so a word the table
        // names is dispatched and never sent. Before the arm that completes a half-typed one, which
        // is what a whole word is not, and before the arm that submits, which is what the word must
        // not reach.
        //
        // The line is taken off the box before it is dispatched, the way each of these arms took it
        // before doing anything, so a command that opens a picker or ends the session does not leave
        // its own word sitting in the box behind it.
        KeyCode::Enter if command_typed(session.input()).is_some() => {
            let line = session.input().to_string();
            session.clear_input();
            dispatch_command(session, &line)
        }
        // A half-typed command, after every arm that recognises a whole one. Enter takes the
        // highlighted row rather than sending "/mod" to the planner, which is never what was meant.
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

/// Carry out the command a line is, whether it was typed just now or queued while a turn ran.
///
/// One place both callers reach, because the two differ only in when the line arrives. Enter at rest
/// dispatches the line the box holds; the loop dispatches the line somebody queued once the turn it
/// was waiting for has ended. Neither reads anything but the line, so a queued command cannot be
/// confused by whatever is in the box by the time it runs.
///
/// The line, not the box, for that reason: a person who queued `/clear` and then began typing
/// something else has a box that is theirs, and this must not read it or write it.
///
/// Every word [`commands`] names is answered here. The last line is unreachable for any of them and
/// is a table entry that nobody wired up: `every_command_in_the_table_dispatches` is what says so,
/// since a word recognised and then quietly dropped would be worse than one never recognised.
fn dispatch_command(session: &mut Session, line: &str) -> Action {
    if line.trim() == EXIT_COMMAND {
        session.quit();
        return Action::Quit;
    }
    if line.trim() == MODEL_COMMAND {
        return Action::ChooseModel;
    }
    if let Some(name) = argument_to(line, THEME_COMMAND) {
        return if name.is_empty() {
            Action::ChooseTheme
        } else {
            Action::SetTheme(name.to_string())
        };
    }
    if let Some(level) = argument_to(line, EFFORT_COMMAND) {
        return if level.is_empty() {
            Action::ChooseEffort
        } else {
            Action::SetEffort(level.to_string())
        };
    }
    if line.trim() == CONFIG_COMMAND {
        return Action::ChooseEditing;
    }
    if line.trim() == STATUS_COMMAND {
        return Action::Status;
    }
    if line.trim() == COMPACT_COMMAND {
        return Action::Compact;
    }
    // The question is taken verbatim and never sent as a prompt: it goes out over a copy of the
    // conversation and the copy is thrown away, so nothing about it joins the exchange.
    if let Some(question) = argument_to(line, BTW_COMMAND) {
        return Action::Aside(question.to_string());
    }
    if line.trim() == CLEAR_COMMAND {
        return Action::Clear;
    }
    if let Some(path) = argument_to(line, EXPORT_COMMAND) {
        return if path.is_empty() {
            Action::Export(None)
        } else {
            Action::Export(Some(path.to_string()))
        };
    }
    if line.trim() == UNDO_COMMAND {
        return Action::Undo;
    }
    if let Some(turns) = argument_to(line, REWIND_COMMAND) {
        return Action::Rewind(turns.to_string());
    }
    if let Some(directory) = argument_to(line, ADD_DIR_COMMAND) {
        return Action::AddDirectory(directory.to_string());
    }
    if let Some(directory) = argument_to(line, CD_COMMAND) {
        return Action::ChangeDirectory(directory.to_string());
    }
    if let Some(name) = argument_to(line, RENAME_COMMAND) {
        return Action::Rename(name.to_string());
    }
    // The command that starts the other kind of run. The task is taken verbatim and is never sent
    // as a prompt: the planner that reads it is a fresh one with nothing but the task and the
    // driver's own words in its context, so the conversation neither goes into the run nor hears
    // anything back from it.
    if let Some(task) = argument_to(line, MANIFEST_COMMAND) {
        return Action::Manifest(task.to_string());
    }
    // The command that sends a prompt rather than the line it was typed on. `/loop 5m check the
    // deploy` arms the loop and hands back "check the deploy", which is what every tick sends from
    // here on.
    if let Some(argument) = argument_to(line, LOOP_COMMAND) {
        return match crate::loops::parse(argument) {
            Some(request) => match session.start_loop(request) {
                Some(prompt) => Action::Submit(prompt),
                None => Action::Redraw,
            },
            None => {
                session.note(t!(loop_needs_a_prompt));
                Action::Redraw
            }
        };
    }
    // The command that sends nothing. A goal is a condition and not a prompt, so setting one arms it
    // and waits: what it keeps going is whatever the person asks for next.
    if let Some(argument) = argument_to(line, GOAL_COMMAND) {
        match crate::goals::parse(argument) {
            crate::goals::Asked::Report => session.report_goal(),
            crate::goals::Asked::Clear => {
                if !session.clear_goal() {
                    session.note(t!(goal_none));
                }
            }
            crate::goals::Asked::Set(condition) => session.start_goal(condition),
        }
        return Action::Redraw;
    }
    // The command that sends nothing either. A watch is armed by asking for one in a prompt, so
    // what is here is the reading and the ending: the two halves of a standing watch that a
    // transcript cannot show.
    if let Some(argument) = argument_to(line, WATCH_COMMAND) {
        match crate::watches::parse(argument) {
            crate::watches::Asked::List => session.report_watches(),
            crate::watches::Asked::Stop(number) => {
                session.stop_watch(number);
            }
            crate::watches::Asked::Unreadable => session.note(t!(watch_command_takes)),
        }
        return Action::Redraw;
    }
    Action::None
}

/// The next thing the queue is holding for the loop, if the session is free to take it.
///
/// A command somebody queued while a turn ran is carried out here rather than anywhere in the key
/// handling, because nobody is going to press anything to make it happen: the press already happened,
/// and this is the loop keeping the promise the queue made. It becomes the action Enter on that line
/// would have become at rest, which is the whole of what was deferred.
///
/// A prompt waiting behind a command goes from here too. The turn loop sends what is queued as each
/// turn ends and stops at a command, so without this the line behind one would wait for a key press
/// that nobody is going to make.
fn queued_next(session: &mut Session) -> Option<Action> {
    if let Some(line) = session.take_queued_command() {
        return Some(dispatch_command(session, &line));
    }
    session.send_queued().map(Action::Submit)
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
        _ if session.bindings().is_stash(&key) && session.history.is_browsing() => {
            session.open_history_search_here();
            Action::Redraw
        }
        _ if session.bindings().is_stash(&key) => {
            session.stash();
            Action::Redraw
        }
        // What a turn did, drawn per entry as the turn fills in. In the shared ladder because it
        // sets a render flag and sends nothing, and because the trail is *for* watching a turn:
        // refused mid-turn, a person who wanted to see which tools a turn was calling had to wait
        // for it to finish before they were allowed to ask.
        _ if session.bindings().is_trail(&key) => {
            session.toggle_trail();
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
        // watching a turn edit files it should not be is deciding about the next turn, and this is
        // how they say so. The turn in flight keeps the mode it began with, its confirmer having
        // been built with it.
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
        KeyCode::Up if !session.history.is_empty() => {
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
        // How the letters become instructions, for somebody editing the way vi does. The other
        // spelling is Escape, which the idle ladder answers: a terminal asked to disambiguate reports
        // this chord where another sends the byte Escape already is, and which arrives is the
        // terminal's choice rather than the person's.
        //
        // In the shared ladder, so it works while a turn runs like everything else that only moves
        // the caret. It is also the one spelling that can: Escape mid-turn stops the turn, which is a
        // difference the box is allowed and this chord is not part of.
        //
        // Before the catch-all below, which would otherwise swallow it as an unclaimed control chord.
        KeyCode::Char('[') if ctrl && session.editing() == crate::vim::Editing::Vi => {
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

/// Take a paste while a turn is running.
///
/// The same folding as the idle path, for the same reason: a stack trace pasted mid-turn would
/// otherwise push the reply being read off the screen, and mid-turn is exactly when there is a
/// reply worth reading. Named rather than bound inline at each working loop, because there are two
/// of them and writing the same arm twice is how they came to disagree.
///
/// A drop is recognised here too, since a drop reaches the terminal as a paste of the path and a
/// running turn is when people drop a file: they are watching a reply and want to hand over the
/// screenshot it is about. Without this the path was written out as prose, so the line said
/// nothing about a file and the attachment was never staged.
///
/// What comes back is what [`act_while_working`] carries out, which is the clipboard read an empty
/// paste asks for and nothing else.
pub fn handle_paste_while_working(session: &mut Session, text: &str) -> Action {
    // The line is the scroller's to leave alone, and a paste is not a key: it arrives from the
    // terminal whatever mode is open, so the guard that holds the box still for a keystroke never
    // sees it. Nothing is said about it, because the scroller has the whole screen but its footer
    // and a sentence drawn nowhere is no answer.
    if session.scrolling() {
        return Action::None;
    }
    // A picture the terminal could not carry arrives as a paste of nothing, mid-turn exactly as at
    // rest, and the answer is the same one: go and read the clipboard. Before the drop check, for
    // the reason the idle path says it before anything looks at the text, since an empty paste is
    // no more a drop than it is a prompt.
    //
    // Without this arm the empty paste reached `paste_text`, which writes nothing and says
    // nothing, so a screenshot pasted while the turn it was about was running went nowhere and
    // the key that would have carried it was never named.
    if text.is_empty() {
        let chord = session.bindings().paste_name();
        session.note_once(t!(paste_arrived_empty, chord = chord));
        return Action::Paste;
    }
    if !session.drop_files(text) {
        session.paste_text(text);
    }
    Action::Redraw
}

/// Carry out what a key or a paste answered with while a turn is running.
///
/// [`Action::Paste`] is the only answer either of the mid-turn handlers gives that the session
/// cannot carry out itself, since reading the clipboard runs the platform's own tools. Everything
/// else they return is a redraw the loops do every frame regardless.
///
/// Every working loop goes through here rather than answering the event and dropping what came
/// back. Four of them did exactly that, so Ctrl-V mid-turn reached its arm, produced the right
/// answer, and had it thrown away: the clipboard was never read and the picture never staged, on a
/// key the idle path has always answered (INPUT-9, PASTE-7).
///
/// The read arrives as an argument for the reason [`take_from_clipboard`] takes one: a test can
/// then say what the clipboard held and watch the picture land in the box, rather than assert only
/// that the answer came back, which is what left the discard invisible.
fn act_while_working(
    session: &mut Session,
    action: Action,
    read: impl FnOnce() -> crate::clipboard::Pasted,
) {
    if action == Action::Paste {
        take_from_clipboard(session, read());
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
    // Watching sends nothing either, and mid-turn is the whole of when there is a delegate to
    // watch: one does not outlive the turn that spawned it.
    if session.watching().is_some() {
        return watching_key(session, key);
    }

    // The same list as at rest, for the reason the ladder below is the same list: nothing the
    // scroller does sends anything, and sending is the whole of what a running turn refuses.
    if session.scrolling() {
        return scroller_key(session, key);
    }

    // Searching sends nothing either, and mid-turn is when the prompt somebody wants back is most
    // likely to be one they can no longer see: the turn they are watching has filled the screen.
    if session.searching_history() {
        return history_search_key(session, key);
    }

    // The same translation the idle path makes, in the same place: after the modes that claim every
    // key of their own and before anything reads one. The letters vi spells these with reach the
    // prompt history and the search, neither of which sends anything, so a running turn refuses
    // nothing here.
    let key = spelled_by_vi(session, key).unwrap_or(key);

    // Before the modifier guard, since the readline bindings are how the caret moves on a terminal
    // that sends nothing for the named keys, and a line that can be typed mid-turn has to be
    // editable mid-turn: the alternative is a box that takes words and will not let them be fixed.
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

    // Before the arm that queues a prompt, because the two do the same thing to the box and differ
    // only in what is waiting afterwards. This line waits to be carried out; the queue is what a
    // person typing mid-turn already understands, so a command joins it rather than sitting in the
    // box asking to be pressed again. What it is spared is the running turn: a queued command is
    // never offered to it, so the planner is never asked what to clear.
    //
    // Not in shell mode, where the line is a command line and `/status` is a path to a program, for
    // the reason the idle ladder answers a shell line before its command arms.
    if key.code == KeyCode::Enter
        && !session.shell
        && command_typed(session.input()).is_some()
        && session.queue_command()
    {
        return Action::Redraw;
    }

    // After the arm that starts a line, so Shift-Enter still writes a paragraph, and before the
    // ladder, where Enter means nothing. A second turn still must not begin while the first is in
    // flight; what changes is that the line no longer waits in the box for the person to notice
    // the turn has ended and press Enter again.
    if key.code == KeyCode::Enter && session.queue() {
        return Action::Redraw;
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

/// Interpret a paste.
///
/// A paste is one act rather than a run of keys, which is the whole point of asking the
/// terminal for it separately: the text lands in the box and the user decides when to send it.
/// It never submits, whatever it ends with.
/// Answer a paste the terminal delivered, which is what Command-V and the middle mouse button
/// become by the time they reach this process.
///
/// A paste that arrives carrying nothing is the interesting one. It means the terminal wrote the
/// bracketed-paste markers with nothing between them, which is what happens when the clipboard
/// holds something with no text in it at all: a picture. The terminal has no way to send that down
/// a pty and no way to say so either, so the empty paste is the whole of the signal, and the answer
/// is to go and read the clipboard directly.
///
/// Said once per session and then not again, because a user who has been told which key carries a
/// picture does not need telling every time they use the other one.
pub fn handle_paste(session: &mut Session, text: &str) -> Action {
    // The line is the scroller's to leave alone, and a paste is not a key: it arrives from the
    // terminal whatever mode is open, so the guard that holds the box still for a keystroke never
    // sees it. Before the empty case as much as the rest, because that one spends a hint said
    // once a session on a sentence the scroller leaves no room to draw.
    if session.scrolling() {
        return Action::None;
    }
    // A picture copied to the clipboard reaches a terminal as a paste of nothing at all, since
    // the terminal hands over text and there is none. Said before anything else looks at the
    // text, because an empty paste is no more a drop than it is a prompt.
    if text.is_empty() {
        let chord = session.bindings().paste_name();
        session.note_once(t!(paste_arrived_empty, chord = chord));
        return Action::Paste;
    }
    // A drop reaches the terminal as a paste of the path, so this is where one is recognised.
    // Anything that is not a drop is text the user pasted, which lands in the box whole or behind
    // a marker depending on how much of the screen it was about to take.
    if !session.drop_files(text) {
        session.paste_text(text);
    }
    Action::Redraw
}

/// Bring what is on the clipboard into the prompt.
///
/// Separated from the loop so it can be tested without a terminal, and taking the read as an
/// argument so a test can say what the clipboard held.
fn take_from_clipboard(session: &mut Session, pasted: crate::clipboard::Pasted) {
    use crate::clipboard::{MAX_IMAGE_BYTES, Pasted};

    // Whatever the answer was, it is the current one, so the hint has served its purpose. It comes
    // back at the next focus change if the picture is still there and still wanted.
    session.image_on_clipboard = false;

    match pasted {
        // A command line is not a sentence, so a marker in one names nothing and would be passed to
        // the shell as literal text. Saying so beats writing it and letting the shell complain.
        Pasted::Image(_) if session.shell => session.note(t!(paste_not_a_command)),
        Pasted::Image(image) => session.attach(image),
        Pasted::Text(text) => session.paste_text(&text),
        Pasted::TooLarge(bytes) => session.note(t!(
            paste_too_large,
            size = in_megabytes(bytes),
            limit = in_megabytes(MAX_IMAGE_BYTES)
        )),
        Pasted::Nothing => session.note(t!(paste_nothing_on_clipboard)),
    }
}

/// A byte count as a person would say it, since nobody reads seven digits off a screen.
fn in_megabytes(bytes: usize) -> String {
    let size = format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
        .replace('.', t!(number_decimal_separator));
    t!(megabytes, size = size)
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
        // and the last the way every other movement in that mode does.
        MouseEventKind::ScrollUp if session.scrolling() => {
            session.scroller_back(3);
            Action::Redraw
        }
        MouseEventKind::ScrollDown if session.scrolling() => {
            session.scroller_on(3);
            Action::Redraw
        }
        MouseEventKind::ScrollUp => {
            session.scroll_up(3);
            Action::Redraw
        }
        MouseEventKind::ScrollDown => {
            session.scroll_down(3);
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

/// Take what the selection covers and put it on the clipboard.
///
/// What the user swept over is what they saw: wrapped, scrolled and trimmed exactly as it was
/// drawn. So it is read back off a frame rather than out of the transcript, which would have to
/// be laid out a second time to say what any of it looked like.
///
/// Drawn again to get one. A finished draw resets the buffer the next one will be built in, so
/// the frame that drew the screen is the only place the screen can be read from.
///
/// A click that swept over nothing just puts the selection away.
/// Draw a frame, and take back what it laid the transcript out to.
///
/// Every draw a person looks at goes through here, so nothing can redraw without the session
/// learning the shape of what was drawn. The scroller's keys are answered against those numbers,
/// and a stale set is a jump to the wrong row.
fn redraw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) -> io::Result<()> {
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, session))
        .map_err(io::Error::other)?;
    session.note_layout(laid);
    Ok(())
}

fn copy_selection(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) -> io::Result<()> {
    let Some(selection) = session.selection else {
        return Ok(());
    };
    if selection.is_empty() {
        session.clear_selection();
        return Ok(());
    }

    let text = {
        // Drawn to be read back rather than to be looked at, and it draws what the last frame
        // drew, so it has nothing new to tell the session about the layout.
        let completed = terminal
            .draw(|frame| {
                render::draw(frame, session);
            })
            .map_err(io::Error::other)?;
        select::text(completed.buffer, &selection)
    };

    // Nothing but the padding between widgets, which is not something to put on a clipboard and
    // not something to claim to have copied either.
    if text.is_empty() {
        return Ok(());
    }

    if crate::clipboard::copy(&text) {
        session.note_copied(text.chars().count());
    }
    Ok(())
}

/// What a session begins with.
#[derive(Debug, Default)]
pub enum Start {
    /// A new session, with nothing behind it.
    #[default]
    Fresh,
    /// Ask which of this directory's sessions to pick up, if there are any.
    Choose,
    /// A session read back off disk, continuing where it left off.
    Resuming(Box<crate::sessions::Record>),
}

/// Run the interface until the user leaves.
/// Returns the session left behind, where there is one to pick up again.
pub fn run(
    config: &mut Config,
    workspace: &Workspace,
    confinement: String,
    start: Start,
    skip_permissions: bool,
) -> io::Result<Option<crate::sessions::Resumable>> {
    // Before the terminal is taken, because the request for no colour decides whether it is asked
    // about its background on the way in, and that question happens inside the takeover.
    crate::theme::sense_no_color();
    crate::indicator::sense_no_motion();

    let mut stdout = io::stdout();
    take_over_terminal(&mut stdout)?;

    // Handed back on the way out, because the terminal is already taken by the line above and a
    // failure here would otherwise return from a session that never started, leaving the person in
    // raw mode on a screen they cannot get off.
    let mut terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => terminal,
        Err(failure) => {
            hand_back_terminal(&mut io::stdout())?;
            return Err(failure);
        }
    };

    // Asked before the session begins, so what it starts with is settled before anything is
    // drawn for it. Choosing nothing is an ordinary session rather than an error.
    let start = match start {
        Start::Choose => match crate::resume::choose(&mut terminal, workspace.root()) {
            crate::resume::Choice::Resume(record) => Some(Start::Resuming(record)),
            crate::resume::Choice::Fresh => Some(Start::Fresh),
            // Leaving at the picker starts nothing. The terminal is still put back below.
            crate::resume::Choice::Quit => None,
        },
        chosen => Some(chosen),
    };

    let result = match start {
        // The picker refuses Enter on one of these, and `--resume` refuses it too. If a record
        // still arrives here, loading its empty conversation as a turn would continue a run
        // that cannot be continued.
        Some(Start::Resuming(record)) if record.manifest.is_some() => Ok(None),
        Some(start) => event_loop(
            &mut terminal,
            config,
            workspace,
            confinement,
            start,
            skip_permissions,
        ),
        // Leaving at the picker resumed nothing and started nothing, so there is nothing to say
        // about picking anything up.
        None => Ok(None),
    };

    // Restore the terminal even if the loop failed: leaving a user in raw mode on an
    // alternate screen is worse than the original error.
    hand_back_terminal(terminal.backend_mut())?;
    terminal.show_cursor()?;

    result
}

/// Put the terminal into the state the interface draws in.
///
/// Mouse capture is what makes the wheel scroll the transcript. It costs the terminal's own text
/// selection, so it is given back on the way out.
///
/// Bracketed paste is what keeps a pasted prompt from sending itself. Without it the terminal
/// delivers a paste as ordinary keystrokes, and the newline most clipboards carry at the end
/// arrives as Enter.
///
/// Disambiguated keys are what makes Shift-Enter reach the interface at all. A terminal sends the
/// same byte for Enter however it was pressed, so without this the modifier is not merely ignored,
/// it never arrives, and a newline in the prompt would be unreachable.
///
/// Asked for only where the terminal says it understands the request. Sending it blind to one that
/// does not leaves the escape sequence on the screen.
///
/// One definition rather than one per caller, because the interface gives the terminal away and
/// takes it back again whenever the prompt is edited elsewhere, and a difference between the two
/// setups would show as a mode that only survives until the first edit.
fn take_over_terminal<W: Write>(out: &mut W) -> io::Result<()> {
    enable_raw_mode()?;
    // After raw mode, so the reply arrives as bytes rather than a line, and before the
    // alternate screen, so the query is not painted into the session.
    crate::theme::sense(out);
    crate::theme::restore_saved();
    ask_for_modes(out, enhanced_keys())
}

/// Ask the terminal for every mode the interface draws and reads in.
///
/// Separate from raw mode, and told rather than asked whether the terminal understands
/// disambiguated keys, because both of those are properties of a real tty: what is left here is
/// bytes, so a test can read back what was asked for and what was given up.
fn ask_for_modes<W: Write>(out: &mut W, enhanced: bool) -> io::Result<()> {
    execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        // Asked for so the clipboard can be looked at when the user comes back from copying
        // something, which is the moment a picture appears on it and the only cheap moment to
        // notice. Polling instead would spawn a clipboard tool every few frames forever.
        EnableFocusChange
    )?;

    if enhanced {
        execute!(
            out,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }

    // Mouse capture asks for motion reported whether or not a button is down, which is a stream
    // of events for a pointer merely crossing the window and a redraw for each one. Only the
    // drag matters here, so all-motion reporting goes back off: what stays on reports the
    // buttons, the wheel, and motion while a button is held, which is the gesture being read.
    write!(out, "{TRACK_MOTION_ONLY_WHILE_DRAGGING}")?;
    out.flush()
}

/// Put the terminal back the way it was found.
fn hand_back_terminal<W: Write>(out: &mut W) -> io::Result<()> {
    disable_raw_mode()?;
    give_back_modes(out, enhanced_keys())
}

/// Give back every mode [`ask_for_modes`] asked for.
fn give_back_modes<W: Write>(out: &mut W, enhanced: bool) -> io::Result<()> {
    // Popped before the modes below, so the stack is unwound in the order it was built.
    if enhanced {
        execute!(out, PopKeyboardEnhancementFlags)?;
    }
    execute!(
        out,
        DisableBracketedPaste,
        DisableFocusChange,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    out.flush()
}

/// Whether the terminal understands the request for disambiguated keys.
///
/// Asked once and remembered, because answering means writing a query and waiting for a reply, and
/// doing that on every handover to an editor would cost a round trip each time.
fn enhanced_keys() -> bool {
    use std::sync::OnceLock;
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| supports_keyboard_enhancement().unwrap_or(false))
}

/// Hand the terminal to the user's editor, and take back whatever they saved.
///
/// The editor gets the terminal properly rather than sharing it: raw mode off, the alternate
/// screen left, the mouse and paste modes given back. A full-screen editor drawing over an
/// interface that still believes it owns the screen is the alternative, and neither of them
/// would be legible.
///
/// The screen is cleared on the way back because the alternate screen came back empty while the
/// interface still holds the frame it drew before leaving. Without it the first redraw sends only
/// what changed, over a screen that has nothing under it.
fn edit_prompt(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) -> io::Result<()> {
    hand_back_terminal(terminal.backend_mut())?;
    terminal.show_cursor()?;

    let edited = crate::editor::edit(session.input());

    take_over_terminal(terminal.backend_mut())?;
    terminal.clear()?;

    match edited {
        Ok(line) => session.take_edited(line),
        // Said rather than swallowed: an editor that would not start looks exactly like a key
        // that does nothing, and the user has no other way to tell the two apart.
        Err(failure) => session.note(failure.to_string()),
    }
    Ok(())
}

/// Sign in to the backend the next request will use, where it needs one and has none.
///
/// The URL and the code go into the transcript, where the person is already looking. The interface
/// keeps the screen throughout: handing it over instead put the one thing somebody has to read and
/// type underneath a display that was about to be redrawn, and left them in a terminal that no longer
/// looked like the program they were using.
///
/// Off-thread for the reason a turn is: the sign-in waits for a browser to be visited, which is as
/// long as the person takes, and run here it would freeze the interface for the whole of it.
///
/// Nothing happens in the common case. A good session is not a sign-in, a build with no AWS
/// configuration cannot want one, and a model served by Brave never needs one whatever else is
/// configured, so the question is asked of the model about to answer rather than of what exists.
fn sign_in_if_needed(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
) -> io::Result<()> {
    let model = session.model().unwrap_or(&config.default_model).to_string();
    if !bravebot_agent::backend::Backend::needs_sign_in(config, &model) {
        return Ok(());
    }

    session.note(t!(session_signing_in));

    let (lines, arriving) = mpsc::channel::<String>();
    let worker_config = config.clone();
    let worker = thread::spawn(move || {
        bravebot_agent::backend::Backend::sign_in_if_needed(&worker_config, &model, |line| {
            // A closed channel is an interface that has stopped listening, and there is nothing to
            // be done about it from here: the sign-in is already running.
            let _ = lines.send(line);
        })
        .map_err(|failure| failure.to_string())
    });

    // Drawn as they arrive rather than collected, because a code is only useful while the command
    // that printed it is still waiting.
    loop {
        redraw(terminal, session)?;

        match arriving.recv_timeout(FRAME) {
            Ok(line) => session.note(line),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    // Said rather than swallowed, and not fatal: the turn goes ahead and fails with the backend's
    // own account of what is wrong, which is more use than this function's guess at it.
    if let Ok(Err(failure)) = worker.join() {
        session.note(failure);
    }
    redraw(terminal, session)?;
    Ok(())
}

/// Hand the transcript to the user's editor, and take nothing back from it.
///
/// The terminal is given up and taken back the way it is for a prompt, since an editor needs the
/// screen. Nothing is read afterwards: the file was a look at the record, not a draft of it.
fn show_transcript(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) -> io::Result<()> {
    let text = render::as_text(session);

    hand_back_terminal(terminal.backend_mut())?;
    terminal.show_cursor()?;

    let shown = crate::editor::show(&text);

    take_over_terminal(terminal.backend_mut())?;
    terminal.clear()?;

    // Said rather than swallowed, for the reason the prompt's editor says it: an editor that
    // would not start looks exactly like a key that does nothing.
    if let Err(failure) = shown {
        session.note(failure.to_string());
    }
    Ok(())
}

/// Concrete in the backend rather than generic: the loop is only ever driven by a real
/// terminal, and a generic backend's error type carries no bounds to convert from.
/// The session left behind, for telling somebody how to pick it up again.
///
/// Nothing for a session that was opened and left without sending anything: there is no record to
/// resume, and naming one would be offering a command that answers "no session by that name".
///
/// Where it is, and not only what it is called: `/cd` moves the record, and the shell this is
/// eventually printed into did not move with it.
fn left_behind(stored: &crate::sessions::Handle) -> Option<crate::sessions::Resumable> {
    stored.to_resume()
}

/// The directory this session writes what is not part of the project into, made and reachable.
///
/// Both at once, because a directory nothing may write in is not worth creating, and reach left
/// pointing at the directory a previous session was given names one that has been removed.
///
/// A session that cannot be given one carries on without it. A temporary directory that is full,
/// read-only or missing is a reason to have nowhere to put an intermediate file, and not a reason
/// to refuse somebody the session they asked for.
///
/// Said out loud when that happens, because the alternative is a turn told it has nowhere to write
/// with nothing on the screen to say why.
fn opened_scratch(session: &mut Session, workspace: &mut Workspace) -> Option<SessionScratch> {
    let scratch = match SessionScratch::create() {
        Ok(scratch) => Some(scratch),
        Err(problem) => {
            session.note(t!(
                session_scratch_unavailable,
                problem = problem.to_string()
            ));
            None
        }
    };
    workspace.open_scratch(scratch.as_ref().map(|held| held.path().to_path_buf()));
    scratch
}

/// The session as it stood before the turn now in flight, for `/undo` to rewind to.
///
/// The turn's own counts come from [`Session::turn_start`] and not from the session in hand: a
/// prompt arrives already pushed onto the transcript and already counted as a turn, so the live
/// figures describe the turn this exists to undo rather than what it replaced.
fn rewind_point(
    session: &Session,
    conversation: &Conversation,
    trust: &TrustStore,
    programs: &TrustedPrograms,
    stored: &crate::sessions::Handle,
) -> crate::state::TurnSnapshot {
    let began = session.turn_start();
    crate::state::TurnSnapshot {
        conversation: conversation.snapshot(),
        turns: began.turns,
        tokens: session.tokens,
        spend: session.spend_by_turn().clone(),
        timing: session.timing_by_turn().clone(),
        cached: session.cached(),
        trust: trust.clone(),
        programs: programs.clone(),
        transcript_len: began.transcript_len,
        title: stored.title().to_string(),
        was_wrote: stored.resumable().is_some(),
    }
}

/// Say what each point a rewind could reach wrote over, most recent first.
///
/// Most recent first because that is the order the numbers run in: `/rewind 1` is the turn that
/// just ended, so a list starting at the far end would have somebody counting rows backwards to
/// find the one they mean. It is also what lets each row carry one turn's paths rather than that
/// turn's and every later turn's: going back to a row puts back every row above it as well,
/// which the heading says, and repeating the same paths down the list would bury the new ones.
///
/// Every path is named rather than counted, for the reason a refused path is: deciding whether
/// to go back is deciding about those files, and a count of them decides nothing.
fn list_rewind_points(session: &mut Session) {
    let lines: Vec<String> = session
        .rewind_points()
        .iter()
        .rev()
        .enumerate()
        .map(|(back, point)| {
            let turns = back + 1;
            let turn = point.snapshot.turns + 1;
            let asked = crate::sessions::title_from(&point.prompt);
            if point.backups.is_empty() {
                t!(
                    session_rewind_point_wrote_nothing,
                    turns = turns,
                    turn = turn,
                    asked = asked
                )
            } else {
                let paths = point
                    .backups
                    .iter()
                    .map(|backup| backup.path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                t!(
                    session_rewind_point,
                    turns = turns,
                    turn = turn,
                    asked = asked,
                    paths = paths
                )
            }
        })
        .collect();
    if lines.is_empty() {
        session.note(t!(session_nothing_to_undo));
        return;
    }
    session.note(t!(session_rewind_points));
    for line in lines {
        session.note(line);
    }
}

/// Put the session back to where it stood `steps` turns ago, on disk and in the conversation.
///
/// Says what happened either way. A rewind that reported nothing would leave somebody who asked
/// for three turns back and had two believing the tree in front of them is three turns older
/// than it is.
fn rewind(
    session: &mut Session,
    conversation: &mut Conversation,
    trust: &mut TrustStore,
    programs: &mut TrustedPrograms,
    stored: &mut crate::sessions::Handle,
    workspace: &Workspace,
    steps: usize,
) {
    let Some((snapshot, backups)) = session.take_rewind(steps) else {
        // Saying how far back it does go rather than refusing in the abstract, since the next
        // thing the person types is that number.
        match session.rewind_points().len() {
            0 => session.note(t!(session_nothing_to_undo)),
            kept => session.note(t!(session_rewind_goes_no_further, kept = kept)),
        }
        return;
    };
    let refused = workspace.restore_backups(backups);

    *conversation = bravebot_agent::Conversation::restored(snapshot.conversation);
    session.turns = snapshot.turns;
    session.tokens = snapshot.tokens;
    session.restore_spend(snapshot.tokens, snapshot.spend);
    session.restore_timing(snapshot.timing);
    // With the spend, for the same reason clearing takes it: the figure describes a
    // prompt that is no longer part of what this session sent.
    session.restore_cache(snapshot.cached);
    session.written = 0;
    session.finished = None;
    *trust = snapshot.trust;
    *programs = snapshot.programs;

    session.transcript.truncate(snapshot.transcript_len);
    stored.truncate_audit(session.turns + 1);

    if snapshot.turns == 0 && !snapshot.was_wrote {
        stored.discard_unwritten(&snapshot.title);
    } else {
        stored.save(
            &snapshot.title,
            crate::sessions::Standing {
                conversation: &conversation.snapshot(),
                turns: session.turns,
                tokens: session.tokens,
                spend: session.spend_by_turn(),
                timing: session.timing_by_turn(),
                model: session.served_model(),
                todos: &session.todos_by_turn(),
                asides: session.asides(),
                trust,
                programs,
                directories: workspace.added_directories(),
                manifest: None,
                rewind: session.rewind_points(),
            },
        );
    }
    // Where it landed rather than how far it came, because that is the fact a person checks the
    // tree against, and a count of turns is one they would have to do the arithmetic on.
    let turn = snapshot.turns + 1;
    if refused.is_empty() {
        session.note(t!(session_rewound, turn = turn));
    } else {
        // Named rather than counted. A person who has to go and put a file back by
        // hand needs to know which one, and a count sends them looking.
        let paths = refused
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        session.note(t!(session_rewound_partly, turn = turn, paths = paths));
    }
}

/// Returns the session left behind, where there is one to pick up again.
fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: &mut Config,
    workspace: &Workspace,
    confinement: String,
    start: Start,
    skip_permissions: bool,
) -> io::Result<Option<crate::sessions::Resumable>> {
    // Owned rather than borrowed, because `/add-dir` opens another directory partway through and
    // the turns after it must see one. The primary root never changes, so nothing keyed on it
    // (the session record, where AGENTS.md is looked for) moves underneath.
    let mut workspace = workspace.clone();

    // The one place persistence is turned on: history in ~/.bravebot outlives the session.
    let mut session = Session::new(confinement)
        .with_stored_history()
        .in_workspace(workspace.root())
        .on_tier(crate::status::configured_tier(config));
    // The flag both opens the session in bypass and puts that rung on the ladder the key walks.
    if skip_permissions {
        session = session.allowing_bypass();
    }

    // The model outlived the session that chose it, so the window that came with it has to be asked
    // for again: it is reported by the listing and nowhere else, and nothing on disk remembers it.
    adopt_budget_for_current_model(&mut session, config);

    // What the session begins holding, which is what decides whether the startup question is put
    // to its user at all. Read off `start` before the match below consumes it.
    let beginning = beginning_of(&start, workspace.root());

    // Outlives every turn, which is the point: a turn begins with the exchange so far rather
    // than with nothing, so the user can say "try that again" and be understood. A resumed
    // session begins with an exchange that outlived the process it happened in.
    // Every run prompt this session has drawn, so a second prompt for one binary under other
    // arguments can say that a settings file is what ends the asking. It grants nothing and is not
    // written down anywhere: a session that ends forgets what it asked, which is the same lifetime
    // the list of programs it vouched for has.
    let mut asked_about = AskedAbout::new();
    let (mut conversation, mut stored, mut programs) = match start {
        // Already answered before the loop was entered: the picker runs once, in `run`.
        Start::Fresh | Start::Choose => (
            Conversation::new(),
            crate::sessions::Handle::begin(workspace.root()),
            // A session that was never asked vouches for nothing, exactly as with the map.
            TrustedPrograms::new(),
        ),
        Start::Resuming(record) => {
            let handle = crate::sessions::Handle::resuming(workspace.root(), &record);
            let conversation = Conversation::restored(record.conversation.clone());
            // Shown before anything else, because a session that silently continues something
            // the user cannot see is one they will contradict without meaning to. The trail comes
            // out of the audit beside the record, so Ctrl-T answers for the whole session rather
            // than only for the turns this process ran.
            let recalled = crate::sessions::recall(workspace.root(), &record);
            session.replay(&conversation, &record.title, &recalled);
            session.restore_spend(record.tokens, record.spend.clone());
            session.restore_timing(record.timing.clone());
            if conversation.last_request_tokens() > 0 {
                session.measured(
                    conversation.last_request_tokens(),
                    config.context_budget,
                    config.budget_is_guessed(),
                );
            }
            // Said after the transcript, so it reads as a caveat on what was just shown: the work
            // it describes may not be in the tree the user is now looking at.
            if let Some(note) = crate::sessions::branch_note(
                record.branch.as_deref(),
                crate::sessions::branch_of(workspace.root()).as_deref(),
            ) {
                session.note(note);
            }
            // The same caveat about the other half of what produced that transcript: not the
            // tree it ran against, but the code that ran.
            if let Some(note) = crate::sessions::build_note(record.build.as_deref(), crate::BUILD) {
                session.note(note);
            }
            // The programs go the way the map does and for the same reason: the person resuming
            // is the person who vouched for them. Unlike the map there is nothing to ask about an
            // absent list, since an empty one simply means every run asks.
            let vouched = record.trusted_programs();
            // The other half of `/add-dir`, which the map cannot carry: a directory has to be
            // open for an absolute path in it to resolve at all. Restored here so the rule and
            // the reach come back together, rather than the rule alone.
            for note in record.reopen_added_directories(&mut workspace) {
                session.note(note);
            }
            // After the transcript rather than before it, because each point's place in that
            // transcript is worked out from it: a point is a turn number in the record and an
            // index in the session, and the list it indexes has to exist first.
            session.restore_rewind_points(record.rewind_points(workspace.root()), &conversation);
            (conversation, handle, vouched)
        }
    };

    // Settled once, before any turn. Nothing means the user left at the question, and a session
    // they never agreed to have must not begin behind it.
    let Some((mut trust, whence)) =
        opening_trust(terminal, &mut session, workspace.root(), beginning)
    else {
        return Ok(left_behind(&stored));
    };

    // Somewhere of its own to write what is not part of the project, held for as long as the
    // session: dropping it takes the directory and everything in it. After the question, so a
    // person who left at it has nothing created for a session they declined to have.
    let mut scratch = opened_scratch(&mut session, &mut workspace);

    // The language servers this session has started, LSP-8. Held here rather than inside a turn
    // because the process is the session's: one built per turn would be shut down at the end of
    // the turn that started it, so the next message would ask the same person about the same
    // language and wait for a second index of the same tree. `None` until the first turn builds
    // one, and again after `/clear`, which ends the session the approval belonged to, and after
    // `/cd`, which leaves the tree it was approved for.
    let mut servers: Option<LanguageServers> = None;

    // After the trust answer, because that question is the first thing on the screen and an aside
    // about a newer release does not come before it. Nothing is fetched here: the line is read off
    // what an earlier launch wrote down, and the ask that answers the next launch runs behind the
    // session rather than in front of it.
    if let Some(newer) = crate::update::at_startup() {
        session.note(newer);
    }

    // The rules the user wrote in advance, read once for the session: a person editing the file
    // mid-session is describing the next one, and rules that changed halfway through a turn would
    // be the harder thing to explain. Every turn below is given these.
    let settings = bravebot_config::Settings::load();
    // Settled before a key can be pressed, since this is what decides whether a letter is a letter.
    session.adopt_editing(settings.editor_mode());
    session.adopt_keybindings(settings.keybindings());
    let (permissions, rejected) = bravebot_agent::permissions::from_settings(
        &settings,
        bravebot_agent::home::directory().as_deref(),
    );
    // Said out loud, because a rule that parses as nothing is a rule somebody believes is in
    // force. A misspelled deny rule reads as protection that is not there.
    for problem in &rejected {
        session.note(t!(
            session_permission_rule_ignored,
            problem = problem.to_string()
        ));
    }
    // For the same reason, and it matters more: this one is not a rule that quietly does nothing but
    // every rule at once. Said before the first prompt can be typed, so a person who did not mean to
    // pass the flag finds out before a write happens rather than after one. `/status` says it too,
    // for the rest of the session, since a note scrolls away.
    if skip_permissions {
        session.note(t!(session_permissions_skipped));
    }
    // After the startup question, and put rather than applied: naming a directory in a settings
    // file asks for it instead of granting it, so one the person accepts is opened on the same
    // terms as one typed at `/add-dir`, and one they decline is not opened at all.
    let requested = named_directories(
        whence,
        bravebot_agent::permissions::additional_directories(&settings),
    );
    let opening = match requested {
        Named::Opening(names) => named_to_open(&mut session, &workspace, &names),
        Named::Asking(names) => {
            let paths = named_to_open(&mut session, &workspace, &names);
            match crate::trust_prompt::ask_named(terminal, &paths) {
                Some(accepted) => accepted,
                None => return Ok(left_behind(&stored)),
            }
        }
    };
    open_named(&mut session, &mut workspace, &mut trust, &opening);

    // Drawn when something has changed rather than on every pass. A drag arrives as a stream of
    // positions, and a frame for each costs more than the whole gesture is worth: with a long
    // transcript the queue outruns the drawing and the highlight trails seconds behind the
    // pointer. Coalescing a burst into one frame is what makes it keep up.
    let mut needs_draw = true;
    let mut drawn_at = Instant::now();

    loop {
        // Waiting for the burst to end, but not indefinitely: a drag that never pauses would
        // otherwise show nothing until it stopped.
        let waited_long_enough = drawn_at.elapsed() >= FRAME;
        if needs_draw && (waited_long_enough || !event::poll(Duration::ZERO)?) {
            redraw(terminal, &mut session)?;
            needs_draw = false;
            drawn_at = Instant::now();
        }

        if session.is_quitting() {
            return Ok(left_behind(&stored));
        }

        // A loop's next tick, looked at before the interface settles down to wait for a key.
        // Nobody is going to press anything to make it happen, so a tick taken only after input
        // arrives would sit there until somebody typed something unrelated. It becomes an
        // ordinary submission, because that is what it is: the line the person typed at `/loop`,
        // sent again.
        let action = match session.loop_tick() {
            Some(prompt) => Action::Submit(prompt),
            // Then the watches, looked at here for the same reason and in the same pass: a
            // filesystem change is the one event in this program that nobody presses a key for.
            // Only one of the two can produce a turn, and a session holds only one kind at a
            // time, so the order between them decides nothing.
            None => match session.watch_fired(Instant::now(), |path| workspace.look(path)) {
                Some(prompt) => Action::Submit(prompt),
                // Then what the queue is holding, before the interface settles down to wait, for
                // the reason a tick is looked at here.
                None => match queued_next(&mut session) {
                    Some(action) => action,
                    None => {
                        if !event::poll(POLL)? {
                            continue;
                        }
                        match event::read()? {
                            // Presses only. Asking for disambiguated keys asks for releases as well, and a
                            // release handled as a press types every character twice.
                            TermEvent::Key(key) if key.kind == KeyEventKind::Release => {
                                Action::None
                            }
                            TermEvent::Key(key) => handle_key(&mut session, key),
                            TermEvent::Mouse(mouse) => handle_mouse(&mut session, mouse),
                            TermEvent::Paste(text) => handle_paste(&mut session, &text),
                            // Coming back from copying something is the moment a picture appears on the
                            // clipboard, and the cheapest moment to notice: once per switch away and back,
                            // rather than a clipboard tool spawned on a timer for the whole life of the
                            // session.
                            TermEvent::FocusGained => {
                                session.image_on_clipboard = crate::clipboard::holds_an_image();
                                Action::Redraw
                            }
                            _ => Action::None,
                        }
                    }
                },
            },
        };

        needs_draw |= !matches!(action, Action::None);

        match action {
            Action::Quit => return Ok(left_behind(&stored)),
            Action::Copy => copy_selection(terminal, &mut session)?,
            Action::Paste => take_from_clipboard(&mut session, crate::clipboard::paste()),
            Action::Edit => {
                edit_prompt(terminal, &mut session)?;
                needs_draw = true;
            }
            Action::Export(path) => {
                let markdown = crate::render::as_markdown(&session, stored.title());
                let exported_path = crate::sessions::export(
                    workspace.root(),
                    stored.id(),
                    path.as_deref(),
                    &markdown,
                );
                match exported_path {
                    Ok(p) => session.note(t!(session_exported, path = p.display().to_string())),
                    Err(e) => session.note(t!(session_export_failed, problem = e.to_string())),
                }
                needs_draw = true;
            }
            Action::Undo => {
                rewind(
                    &mut session,
                    &mut conversation,
                    &mut trust,
                    &mut programs,
                    &mut stored,
                    &workspace,
                    1,
                );
                needs_draw = true;
            }
            Action::Rewind(turns) => {
                if turns.is_empty() {
                    list_rewind_points(&mut session);
                } else {
                    match turns.parse::<usize>() {
                        Ok(steps) if steps > 0 => rewind(
                            &mut session,
                            &mut conversation,
                            &mut trust,
                            &mut programs,
                            &mut stored,
                            &workspace,
                            steps,
                        ),
                        _ => session.note(t!(session_rewind_needs_a_number)),
                    }
                }
                needs_draw = true;
            }
            Action::Show => {
                show_transcript(terminal, &mut session)?;
                needs_draw = true;
            }
            Action::ChooseModel => {
                choose_model(terminal, &mut session, config);
                needs_draw = true;
            }
            Action::ChooseTheme => {
                choose_theme(terminal, &mut session);
                needs_draw = true;
            }
            Action::SetTheme(name) => {
                set_theme(&mut session, &name);
                needs_draw = true;
            }
            Action::ChooseEffort => {
                choose_effort(terminal, &mut session);
                needs_draw = true;
            }
            Action::SetEffort(level) => {
                set_effort(&mut session, &level);
                needs_draw = true;
            }
            Action::ChooseEditing => {
                choose_editing(terminal, &mut session);
                needs_draw = true;
            }
            Action::AddDirectory(directory) => {
                // The snapshot holds a trust map without this directory's rule in it, while the
                // directory itself would stay open.
                session.close_rewind_window();
                add_directory(&mut session, &mut workspace, &mut trust, &directory);
            }
            Action::ChangeDirectory(directory) => {
                // The record moves with the working directory, so the snapshot describes a
                // session that is no longer written where it was.
                session.close_rewind_window();
                if change_directory(
                    &mut session,
                    &mut workspace,
                    &mut trust,
                    &mut servers,
                    &directory,
                ) {
                    stored.move_to(
                        workspace.root(),
                        crate::sessions::Standing {
                            conversation: &conversation.snapshot(),
                            turns: session.turns,
                            tokens: session.tokens,
                            spend: session.spend_by_turn(),
                            timing: session.timing_by_turn(),
                            model: session.served_model(),
                            todos: &session.todos_by_turn(),
                            asides: session.asides(),
                            trust: &trust,
                            programs: &programs,
                            directories: workspace.added_directories(),
                            manifest: None,
                            rewind: session.rewind_points(),
                        },
                    );
                }
            }
            Action::Rename(name) => {
                // The snapshot holds the name the session had before it was renamed.
                session.close_rewind_window();
                if name.is_empty() {
                    session.note(t!(session_rename_needs_a_name));
                } else if stored.rename(&name) {
                    session.note(t!(session_renamed, title = stored.title()));
                } else {
                    session.note(t!(session_rename_needs_something));
                }
            }
            Action::Status => {
                let theme = crate::theme::name();
                // Read here rather than held, for the reason the run prompt reads it where it would
                // draw: the file belongs to every session begun in this directory, so a person
                // asking what they are carrying should be told what the file says now.
                let record = remembered_record(&workspace);
                let report = crate::status::report(&crate::status::Facts {
                    session_name: stored.title(),
                    session_id: stored.id(),
                    directory: workspace.root(),
                    added_directories: workspace.added_directories(),
                    scratch: scratch.as_ref().map(SessionScratch::path),
                    model: session.model(),
                    effort: session.effort(),
                    model_reads_effort: session.model_reads_effort(),
                    served_model: session.served_model(),
                    premium: session.premium(),
                    theme: &theme,
                    config,
                    confinement: &session.confinement,
                    permission_mode: session.permission_mode(),
                    turns: session.turns,
                    tokens: session.tokens,
                    timing: session.timing_total(),
                    cached: session.cached(),
                    trust: &trust,
                    programs: &programs,
                    looping: session.looping(),
                    watches: session.watches(),
                    goal: session.goal(),
                    remembered: record
                        .as_ref()
                        .map(|(store, lines)| crate::status::Remembered {
                            lines,
                            path: store.path(),
                        }),
                });
                session.report(report);
                needs_draw = true;
            }
            Action::Compact => {
                // The snapshot holds the conversation as it was before it was shortened.
                session.close_rewind_window();
                let events;
                (conversation, events) =
                    compact_animated(terminal, &mut session, config, conversation, &trust)?;

                // Written now rather than at the end of the next turn: the shortening is the
                // change, and a session that compacted and then slept should resume compacted.
                let title = stored.title().to_string();
                stored.save(
                    &title,
                    crate::sessions::Standing {
                        conversation: &conversation.snapshot(),
                        turns: session.turns,
                        tokens: session.tokens,
                        spend: session.spend_by_turn(),
                        timing: session.timing_by_turn(),
                        model: session.served_model(),
                        todos: &session.todos_by_turn(),
                        asides: session.asides(),
                        trust: &trust,
                        programs: &programs,
                        directories: workspace.added_directories(),
                        manifest: None,
                        rewind: session.rewind_points(),
                    },
                );
                stored.append_audit(session.turns, &events);
                needs_draw = true;
            }
            Action::Aside(question) => {
                if question.is_empty() {
                    session.note(t!(btw_needs_a_question));
                } else {
                    // The snapshot holds the spend and the timing as they were before the turn,
                    // and an aside is charged to that turn: rewinding to it would un-charge a
                    // request that really went out. The same reason `/compact` closes it.
                    session.close_rewind_window();
                    let events = aside_animated(
                        terminal,
                        &mut session,
                        config,
                        &conversation,
                        &trust,
                        &question,
                    )?;

                    // Written now rather than at the end of the next turn, for the reason a
                    // compaction is: the aside is the change, and a session that asked something
                    // and then slept should resume with the answer still there.
                    //
                    // Nothing yet where no turn has been had, as everywhere else that writes
                    // outside a turn: a session opened and abandoned should leave no record, and
                    // a question asked before the first prompt was asked over an empty exchange.
                    if session.turns > 0 {
                        let title = stored.title().to_string();
                        stored.save(
                            &title,
                            crate::sessions::Standing {
                                conversation: &conversation.snapshot(),
                                turns: session.turns,
                                tokens: session.tokens,
                                spend: session.spend_by_turn(),
                                timing: session.timing_by_turn(),
                                model: session.served_model(),
                                todos: &session.todos_by_turn(),
                                asides: session.asides(),
                                trust: &trust,
                                programs: &programs,
                                directories: workspace.added_directories(),
                                manifest: None,
                                rewind: session.rewind_points(),
                            },
                        );
                        stored.append_audit(session.turns, &events);
                    }
                }
                needs_draw = true;
            }
            Action::Manifest(task) => {
                if task.is_empty() {
                    session.note(t!(manifest_needs_a_task));
                } else {
                    // The snapshot holds the spend and the timing as they were before the turn,
                    // and this run is charged to that turn, exactly as an aside is: rewinding to
                    // it would un-charge requests that really went out, and steps that really
                    // wrote to the tree are not something the rewind window covers.
                    session.close_rewind_window();
                    let events = manifest_animated(
                        terminal,
                        &mut session,
                        config,
                        &workspace,
                        &task,
                        &trust,
                        &permissions,
                    )?;

                    // Taken off the workspace rather than kept for anything: a run is not a turn of
                    // this conversation, and `close_rewind_window` above already shut the window
                    // that could have rewound to one, so nothing will put these back. What the
                    // drain is for is the *next* turn, which takes whatever the workspace is
                    // holding as its own. Left here, the run's writes would be attributed to that
                    // turn and `/undo` on it would revert them.
                    let _ = workspace.take_backups();

                    // The session's own record, written for the reason an aside's is: the run is
                    // the change, and a session that started one and then slept should resume
                    // with the note saying where it went still in the transcript. The run's own
                    // record is a separate file that `manifest_animated` already wrote.
                    if session.turns > 0 {
                        let title = stored.title().to_string();
                        stored.save(
                            &title,
                            crate::sessions::Standing {
                                conversation: &conversation.snapshot(),
                                turns: session.turns,
                                tokens: session.tokens,
                                spend: session.spend_by_turn(),
                                timing: session.timing_by_turn(),
                                model: session.served_model(),
                                todos: &session.todos_by_turn(),
                                asides: session.asides(),
                                trust: &trust,
                                programs: &programs,
                                directories: workspace.added_directories(),
                                // None, and it stays none however many runs this session starts.
                                // Its presence is what makes a record a manifest run, and this
                                // record is a conversation that can be resumed; the run has a
                                // record of its own where that field is filled.
                                manifest: None,
                                rewind: session.rewind_points(),
                            },
                        );
                        stored.append_audit(session.turns, &events);
                    }
                }
                needs_draw = true;
            }
            Action::Clear => {
                // A new handle means a new id, so the session so far keeps its own files and stays
                // resumable. Nothing is deleted: what the user asked for is a clean context, and
                // throwing away the record would be answering a question they did not ask.
                session.clear();
                conversation = Conversation::new();
                stored = crate::sessions::Handle::begin(workspace.root());
                session.note(t!(session_cleared));

                // A new session, so it is asked what a new session is asked. The map goes with the
                // context and the directories opened under it go too, since opening one is a grant
                // and leaving it reachable with nothing vouching for it would outlive its answer.
                workspace.close_added_directories();
                // Where this map came from decides nothing further: a directory a settings file
                // named was opened by an answer the cleared session's user gave, and it closed
                // with that session rather than carrying into this one.
                let Some((fresh, _)) =
                    opening_trust(terminal, &mut session, workspace.root(), Beginning::New)
                else {
                    return Ok(left_behind(&stored));
                };
                trust = fresh;
                // A new session vouches for no program, on the same reasoning as the map: the
                // list is a standing permission, and this begins a session that was never asked.
                programs = TrustedPrograms::new();
                // And nothing has been asked about, since the questions this list holds were put
                // in a session that is over.
                asked_about = AskedAbout::new();
                // And a new directory, since nothing in the old one outlives the session that
                // wrote it. The old one is removed either way: what the cleared context wrote is
                // not something the session after it should find lying there.
                scratch = opened_scratch(&mut session, &mut workspace);
                // The servers go with it, LSP-8: what was approved was a process for the session,
                // so dropping the set shuts them down and the session beginning here is asked
                // again before one starts.
                servers = None;
                needs_draw = true;
            }
            Action::Submit(prompt) => {
                // A prompt sent while this one was running goes when it ends, in the order it was
                // typed, and so does anything typed during that one. Looping here rather than
                // going back round the outer loop keeps a queued prompt from waiting on a key
                // press that nobody is there to make.
                // Whose line each one is, which decides whether a `@path` in it vouches for a
                // file. Everything the person typed or queued is theirs; the sentence a goal
                // carries the work on with is this program's, and so is the one a watch fires
                // with. A fire also must not leave a loop behind repeating itself, which is the
                // other thing this answers.
                let whose = if session.watch_is_firing() {
                    Wrote::TheDriver
                } else {
                    Wrote::ThePerson
                };
                let mut sending = Some((prompt, whose));
                while let Some((prompt, wrote)) = sending {
                    let point = rewind_point(&session, &conversation, &trust, &programs, &stored);
                    session.open_rewind_point(point, prompt.clone());
                    let _ = workspace.take_backups();

                    // Everything the session holds is lent for the turn and taken back: a turn that
                    // writes untrusted data into a trusted path records that, and the next turn must
                    // honour it; a turn that has been had is a turn the next one can be asked about;
                    // a server somebody approved answers the next question without asking again;
                    // and a run prompt already read is what tells the next one to name a settings
                    // file instead of a key.
                    let continued = run_turn_animated(
                        terminal,
                        &mut session,
                        config,
                        &workspace,
                        &prompt,
                        wrote,
                        conversation,
                        trust,
                        programs,
                        servers,
                        asked_about,
                        &permissions,
                        stored.id(),
                    )?;
                    let events = continued.events;
                    conversation = continued.conversation;
                    trust = continued.trust;
                    programs = continued.programs;
                    servers = continued.servers;
                    asked_about = continued.asked_about;

                    session.keep_backups(workspace.take_backups());

                    // Written after each turn rather than at the end, because the end may never
                    // come: the session worth resuming is the one whose machine slept and never
                    // woke. Best-effort, like everything else under ~/.bravebot.
                    stored.save(
                        &prompt,
                        crate::sessions::Standing {
                            conversation: &conversation.snapshot(),
                            turns: session.turns,
                            tokens: session.tokens,
                            spend: session.spend_by_turn(),
                            timing: session.timing_by_turn(),
                            model: session.served_model(),
                            todos: &session.todos_by_turn(),
                            asides: session.asides(),
                            trust: &trust,
                            programs: &programs,
                            directories: workspace.added_directories(),
                            manifest: None,
                            rewind: session.rewind_points(),
                        },
                    );
                    stored.append_audit(session.turns, &events);

                    // Nothing waiting goes out after somebody has asked to leave.
                    //
                    // What the person queued goes before the goal is put to a judge. Their own
                    // prompts are the session, and a condition judged before they have been sent
                    // would be judged against an exchange that is missing them.
                    sending = if session.is_quitting() {
                        None
                    } else if let Some(queued) = session.send_queued() {
                        Some((queued, Wrote::ThePerson))
                    } else {
                        let (carrying_on, checked) = goal_check_animated(
                            terminal,
                            &mut session,
                            config,
                            &conversation,
                            &trust,
                        )?;
                        // The check is a request that really went out, and a refusal in one is
                        // exactly what somebody reading the trail afterwards wants to find.
                        stored.append_audit(session.turns, &checked);
                        carrying_on.map(|prompt| (prompt, Wrote::TheDriver))
                    };
                }
            }
            Action::Run(line) => {
                // The command is in the conversation, and the workspace never saw what it wrote.
                session.close_rewind_window();
                let events =
                    run_command(terminal, &mut session, &workspace, &line, &mut conversation)?;
                // Saved like a turn, and for the same reason: the command is in the conversation
                // now, so a session resumed without it would have the planner referring to output
                // it can no longer see.
                stored.save(
                    &line,
                    crate::sessions::Standing {
                        conversation: &conversation.snapshot(),
                        turns: session.turns,
                        tokens: session.tokens,
                        spend: session.spend_by_turn(),
                        timing: session.timing_by_turn(),
                        model: session.served_model(),
                        todos: &session.todos_by_turn(),
                        asides: session.asides(),
                        trust: &trust,
                        programs: &programs,
                        directories: workspace.added_directories(),
                        manifest: None,
                        rewind: session.rewind_points(),
                    },
                );
                stored.append_audit(session.turns, &events);
                needs_draw = true;
            }
            // Cancel is only reachable while a turn runs, which `run_turn_animated` handles.
            Action::Cancel | Action::None | Action::Redraw => {}
        }
    }
}

/// Open another directory and vouch for it, for the rest of this session.
///
/// Two things happen together, and both are needed. The workspace makes the directory reachable at
/// all, since an absolute path is refused otherwise. The trust map records that the user vouched
/// for it, which is what the write gates consult. Doing only the first would leave every write
/// there asking; doing only the second would leave a rule about files nothing can open.
///
/// The path recorded is the canonical one, not the name typed: `~/notes/../notes` and a symlink
/// both name a directory whose rules should be about where it actually is.
///
/// Session-scoped on purpose. `docs/specs/trust-map.md` is explicit that trust is not sticky per directory,
/// so a later session starts without this and is asked again. It does survive `--resume`, since
/// that restores the map its own user gave.
fn add_directory(
    session: &mut Session,
    workspace: &mut Workspace,
    trust: &mut TrustStore,
    directory: &str,
) {
    if directory.is_empty() {
        session.note(t!(session_add_dir_needs_a_path));
        return;
    }

    // Expanded here rather than in the workspace, because `~` is a shell convention and a library
    // resolving it would be guessing at a home the caller never named.
    let expanded = expand_home(directory);

    match workspace.add_directory(&expanded) {
        Ok(added) => {
            let shown = added.display().to_string();
            trust.trust(&shown);
            session.note(t!(session_directory_added, directory = shown));
        }
        Err(error) => session.note(t!(
            session_directory_not_added,
            directory = directory,
            problem = error
        )),
    }
}

/// Work somewhere else from now on, and vouch for it, for the rest of this session.
///
/// The primary root is what a relative path means, where commands run, and where `AGENTS.md` and
/// the project's skills are looked for, so moving it moves the whole of what the session is about.
/// Says whether it moved, since the session record has to move with it.
///
/// Three things happen together, and the order matters. The workspace moves, closing whatever
/// overlapped the new root. The trust map is re-spelled against the new root, so every rule the
/// session already held goes on saying what it said about the same files: a no given inside the
/// new directory is still a no, and a yes given for the old one does not become a yes for this
/// one. Only then is the new directory vouched for, which is the one thing this grants, and it is
/// granted for the same reason `/add-dir` grants it: a person typed the path themselves, and a
/// later decision replaces an earlier one.
///
/// Session-scoped like everything else the map holds. A later session in the directory the user
/// started in is asked the opening question there, exactly as it would have been.
fn change_directory(
    session: &mut Session,
    workspace: &mut Workspace,
    trust: &mut TrustStore,
    servers: &mut Option<LanguageServers>,
    directory: &str,
) -> bool {
    if directory.is_empty() {
        session.note(t!(session_cd_needs_a_path));
        return false;
    }

    // Against where the session is now, which is what changing directory means everywhere else: a
    // command that only took an absolute path would refuse `/cd crates/tui` and `/cd ..`, which are
    // the two ways anybody would think to say it.
    let named = against_workspace(workspace.root(), &expand_home(directory));

    let moved = match workspace.change_root(&named) {
        Ok(moved) => moved,
        Err(problem) => {
            session.note(t!(
                session_directory_not_changed,
                directory = directory,
                problem = problem
            ));
            return false;
        }
    };

    *trust = trust.rebased(&moved.root);
    trust.trust(".");
    // The servers go, and LSP-5 is why: what a person approved was a server reading *that* tree,
    // and a set indexes the root it was built with. Kept across the move it would answer questions
    // about the new directory out of the old one's index, and resolve the paths it opens against a
    // root the session has left. Dropping it shuts the processes down, and the first question asked
    // here starts one for this directory with the person asked again.
    *servers = None;
    session.now_in_workspace(&moved.root);
    session.note(t!(
        session_directory_changed,
        directory = moved.root.display().to_string()
    ));
    // Said one directory at a time, because each one is something the session could read a moment
    // ago and now cannot. A person is entitled to know which, and to open it again by name.
    for closed in &moved.closed {
        session.note(t!(
            session_directory_closed,
            directory = closed.display().to_string()
        ));
    }
    true
}

/// The name a settings file gave a directory, as an absolute path.
///
/// A relative name means a path under the workspace, which is what `../shared` in a settings file
/// says: the file is about a project, and the directory it wants is next to that project rather
/// than next to wherever the agent happened to be started from.
fn against_workspace(root: &std::path::Path, directory: &str) -> String {
    match std::path::Path::new(directory).is_absolute() {
        true => directory.to_string(),
        false => root.join(directory).display().to_string(),
    }
}

/// Replace a leading `~` with the user's home directory.
///
/// Only a leading one, and only when it is the whole first segment, so a directory genuinely called
/// `~notes` is left alone. Without a home to expand to, the path is passed through and the
/// workspace refuses it for not being absolute, which says the same thing.
fn expand_home(directory: &str) -> String {
    let Some(rest) = directory.strip_prefix('~') else {
        return directory.to_string();
    };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return directory.to_string();
    }
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => {
            format!("{}{rest}", std::path::Path::new(&home).display())
        }
        _ => directory.to_string(),
    }
}

/// Ask the endpoint what it offers, let the user pick, and remember what they picked.
///
/// A refusal or an unreachable endpoint leaves the model as it was and says so. That is the right
/// outcome for a list nobody could fetch: guessing a set of names would offer choices the backend
/// may not have, and a picker showing only "automatic" would look like a server with one model.
///
/// The list is content and the choice is routing. Nothing here is quarantined, because there is no
/// planner context to keep it out of: the names are drawn for a person, and their pick is the
/// endorsement for the request field it lands in.
/// Ask the endpoint what it offers.
///
/// Shared by the picker and by the budget lookup a session does when it starts, because both want
/// the same listing and a second copy of the policy setup would be a second place for the gate to
/// be got wrong.
fn list_models(
    config: &Config,
    chosen: Option<&str>,
) -> Result<Vec<bravebot_aichat::models::Model>, String> {
    // Bedrock is additive. Its tiers come from configuration rather than from a listing, since it has
    // no listing endpoint and an ARN does not say which model it resolves to, but the Brave roster is
    // what everyone has and a settings block adds to it rather than replacing it.
    //
    // The configured tiers come first: someone who named them went out of their way to, and the
    // alternative buries them under a roster they did not ask about.
    let mut configured = config
        .bedrock
        .as_ref()
        .map(bedrock_models)
        .unwrap_or_default();

    // Additive on the same terms. A block that named its models is taken at its word and costs no
    // round trip, which is what keeps a configured gateway working offline. One that named none is
    // asked, because the alternative is a gateway configured exactly as the tool this block's shape
    // came from configures it, offering nothing.
    for provider in &config.providers {
        // An entry naming AWS reaches the Bedrock backend, so its rows are built the way that
        // backend's are: the models the block named, and nothing fetched. There is no listing
        // endpoint to ask, which is why a block that named none offers none rather than being
        // asked what it serves.
        if let Some(bedrock) = provider.bedrock.as_ref() {
            configured.extend(bedrock_models(bedrock));
            continue;
        }
        match provider.models.is_empty() {
            false => configured.extend(provider_models(provider)),
            true => configured.extend(in_reading_order(
                fetch_gateway_models(provider).unwrap_or_default(),
                chosen.unwrap_or(&config.default_model),
            )),
        }
    }

    // A build from source pointed only at Bedrock has blank Brave credentials, and asking with them
    // would list a roster whose every request then fails unsigned.
    if !config.serves_aichat() {
        return Ok(configured);
    }

    combined(configured, fetch_models(config))
}

/// The tiers and the listing as one roster.
///
/// Split from the request so the joining is testable without a server. A listing nobody could fetch
/// is not worth losing the tiers over: they need no network to know, and a picker that refused
/// everything because one half was unreachable would leave the working half unpickable.
fn combined(
    configured: Vec<bravebot_aichat::models::Model>,
    listed: Result<Vec<bravebot_aichat::models::Model>, String>,
) -> Result<Vec<bravebot_aichat::models::Model>, String> {
    match listed {
        Ok(listed) => Ok(configured.into_iter().chain(listed).collect()),
        Err(_) if !configured.is_empty() => Ok(configured),
        Err(problem) => Err(problem),
    }
}

/// Ask the Brave endpoint what it offers.
fn fetch_models(config: &Config) -> Result<Vec<bravebot_aichat::models::Model>, String> {
    let mut sink = Trail::new();
    let egress = Egress::new();

    // A policy exists because `bravebot-net` is the only way out to the network and its gate takes one.
    // Routing is the listing itself: this is not a turn, nothing is read from the workspace, and no
    // model is involved, so there is no prompt to anchor it to.
    let mut routing = bravebot_core::policy::Routing::new();
    routing.insert_trusted("models", config.models_url());

    bravebot_core::policy::Policy::begin(
        routing,
        bravebot_core::policy::ReleasePlan::new(),
        bravebot_core::capability::CapabilitySet::from_iter([
            bravebot_core::capability::Capability::WebFetch,
        ]),
        &mut sink,
    )
    .map_err(|denial| denial.to_string())
    .and_then(|mut policy| {
        bravebot_aichat::models::list(&mut policy, config, &egress)
            .map_err(|error| error.to_string())
    })
}

/// Ask one gateway what it offers, for a block that named no models.
///
/// What comes back names the gateway the same way a configured model from it does, so one service
/// does not appear twice under two names in the same list.
///
/// A gateway whose block named a credential nothing holds is not asked. The listing would come back
/// refused, and the useful thing to say about that gateway is what `doctor` already says: no
/// credential found.
///
/// A block naming no credential at all is asked, unauthenticated. That is somebody saying the gateway
/// wants none, and it is the block a local Ollama is configured with. That block lists no models
/// either, so not asking leaves it offering nothing.
fn fetch_gateway_models(
    provider: &bravebot_config::provider::Provider,
) -> Result<Vec<bravebot_aichat::models::Model>, String> {
    let token = match provider.credential(|name| std::env::var(name).ok()) {
        bravebot_config::provider::Credential::Token(token) => Some(token),
        bravebot_config::provider::Credential::NotNeeded => None,
        bravebot_config::provider::Credential::Absent => {
            return Err(format!("no credential for {}", provider.display_name()));
        }
    };

    let mut sink = Trail::new();
    let egress = Egress::new();

    // Both destinations are the gateway's own endpoint, which came from configuration. Nothing
    // fetched decides either, which is what makes asking a service for a list of names safe at all.
    let mut routing = bravebot_core::policy::Routing::new();
    routing.insert_trusted("models", provider.models_url());
    routing.insert_trusted("account-models", provider.account_models_url());

    bravebot_core::policy::Policy::begin(
        routing,
        bravebot_core::policy::ReleasePlan::new(),
        bravebot_core::capability::CapabilitySet::from_iter([
            bravebot_core::capability::Capability::WebFetch,
        ]),
        &mut sink,
    )
    .map_err(|denial| denial.to_string())
    .and_then(|mut policy| {
        bravebot_aichat::models::list_from_gateway(&mut policy, provider, token.as_deref(), &egress)
            .map_err(|error| error.to_string())
    })
}

/// A fetched gateway roster in the order a person should meet it: what a session would use, then
/// everything else by name.
///
/// The gateway's own order is roughly newest-first, which puts a model somebody has never heard of at
/// the top and buries the one they work with. A configured roster needs none of this: the file is
/// already the order they chose.
///
/// Alphabetical by the key, so the provider id groups the rows and a name somebody half-remembers is
/// where they would look for it. Sorted rather than capped, because a picker filters as it is typed
/// and dropping rows would decide somebody may not choose a model their gateway serves.
fn in_reading_order(
    mut models: Vec<bravebot_aichat::models::Model>,
    chosen: &str,
) -> Vec<bravebot_aichat::models::Model> {
    models.sort_by(|left, right| {
        let ranked = |model: &bravebot_aichat::models::Model| model.key != chosen;
        ranked(left)
            .cmp(&ranked(right))
            .then_with(|| left.key.cmp(&right.key))
    });
    models
}

/// The models a Bedrock configuration offers, strongest tier first.
///
/// One entry per tier that names a model, and nothing else. A tier whose variable is unset is one
/// this configuration cannot reach: an ARN cannot be derived from a model name, so an entry invented
/// for it would be a choice that fails at the far end for a reason nothing here could explain.
///
/// No `automatic` among them. There it means "let the server choose", which Bedrock does not offer: a
/// request names one model and gets it or an error. The entry still reaches the picker, from the Brave
/// half of the roster, where it is a choice that backend can honour.
///
/// Every entry is marked free. Premium here means a Leo subscription, and reaching a model through
/// somebody's own AWS account does not involve one.
///
/// Every entry says whose account answers. Both rosters are offered together and a bare tier name
/// would sit beside a Brave entry for the same model, where the two are reached and billed
/// differently and nothing else would say which was about to answer.
///
/// Not "Bedrock" alone: the Brave roster already says that of models it serves through its own AWS
/// account, so the word distinguishes nothing. The profile is the useful thing, being what decides
/// which credentials sign the request, and the account is all that can be said without one.
fn bedrock_models(
    bedrock: &bravebot_config::bedrock::Bedrock,
) -> Vec<bravebot_aichat::models::Model> {
    bedrock
        .models()
        .iter()
        .map(|entry| bravebot_aichat::models::Model {
            key: entry.id.clone(),
            // The tier word where a tier named it, because the account it is reached through is the
            // same for all of them and is said once, over the section these rows sit in. A model a
            // `provider` block named has no tier, and an inference-profile ARN is not a name
            // anybody reads, so what that block called it is what a row says.
            display_name: entry.display_name().to_string(),
            premium: false,
            provider: Some(match bedrock.profile.as_deref() {
                Some(profile) => t!(picker_service_bedrock_profile, profile = profile),
                None => t!(picker_service_bedrock).to_string(),
            }),
            // The figure a tier gets is a property of what an opaque profile ARN gets rather than of
            // a particular model. A block that stated one knew better.
            conversation_tokens: Some(entry.window()),
            // True until this model has refused the field. Bedrock has no listing to say which
            // parameters a model takes, so the only way to find out is to have sent one, and a row
            // that went on claiming otherwise would report a level as in force after the requests
            // carrying it had stopped.
            reads_effort: bravebot_agent::backend::bedrock_reads_effort(&entry.id),
        })
        .collect()
}

/// The models a gateway was configured to offer, in the order the file listed them.
///
/// One entry per named model, and nothing else. A gateway's own roster runs to hundreds of models
/// across upstreams most people will never use, so asking it would produce a listing nobody could
/// pick from, and asking it at all would cost a round trip on a path that must still work offline.
///
/// Every entry is marked free. Premium means a Leo subscription, and a gateway reached with somebody's
/// own bearer token does not involve one. What it costs them is between them and the gateway.
///
/// Every entry says which service answers it, because the same slug may be reachable more than one
/// way: `anthropic/claude-sonnet-4.5` through a gateway and Brave's own Sonnet are different bills
/// and different credentials, and nothing else about the model would say which was about to answer.
fn provider_models(
    provider: &bravebot_config::provider::Provider,
) -> Vec<bravebot_aichat::models::Model> {
    provider
        .models
        .iter()
        .map(|model| bravebot_aichat::models::Model {
            // Qualified by the provider's own id, because the key is what a choice is remembered as
            // and what later selects a backend. The same slug may be reachable through more than one
            // service, and the bare name says nothing about which was picked.
            key: format!("{}/{}", provider.id, model.id),
            // The slug whole, since it is what a request names, and unqualified, since the gateway
            // is said over the section rather than on every row under it.
            display_name: model.id.clone(),
            premium: false,
            provider: Some(provider.display_name().to_string()),
            // Stated or assumed, never absent: a window is what the budget is taken from, and
            // reporting nothing would leave the session on a default chosen for a different service.
            conversation_tokens: Some(model.window()),
            // A block names models and never their parameters, so the level goes out to be judged
            // at the far end and the judgment is the only description there is: read one where
            // this service has already refused the field for this model.
            reads_effort: bravebot_aichat::reads_effort(
                &provider.chat_completions_url(),
                &model.id,
            ),
        })
        .collect()
}

/// Take the budget for the model already in force, without asking anyone to choose it again.
///
/// A model chosen in an earlier session is read back off disk, and until this ran the window that
/// came with it was not: the budget stayed at the default and a session with room for a hundred
/// thousand tokens compacted at twenty-four, having said nothing about why.
///
/// A listing that cannot be fetched is not worth a word. The budget falls back to the default, which
/// is what it was before this existed, and a session that is merely offline should not open with a
/// complaint about a request nobody asked for.
fn adopt_budget_for_current_model(session: &mut Session, config: &mut Config) {
    let Ok(models) = list_models(config, session.model()) else {
        return;
    };
    if config.adopt_window(advertised_window(&models, session.model())) {
        session.note(t!(session_context_budget, budget = config.context_budget));
    }
    // Outside the note, because a budget that did not move can still have stopped being one the
    // endpoint advertised: nothing changed for compaction, and what the hint line may claim did.
    session.update_budget(config.context_budget, config.budget_is_guessed());
    session.note_model_reads_effort(reads_effort(&models, session.model()));
}

/// Take the budget for the model in force where there is no session to tell about it.
///
/// A one-shot run puts a model in force without anybody picking one: the command line named it, or
/// it was read back off disk from a session that has ended. The listing is the only place a window
/// is ever reported, so this is the only place such a run can learn one, and without it the run
/// compacts against the default: a figure a narrow window never reaches, so compaction cannot fire
/// at all, and one a wide window passes three quarters of the way through the conversation it could
/// have held.
///
/// Silent, where [`adopt_budget_for_current_model`] notes the new budget, because a run has nobody
/// watching and no transcript to put a line in. A listing that cannot be fetched leaves the default
/// in place for the same reason it does in a session.
pub fn adopt_budget_for_model(config: &mut Config, model: &str) {
    let Ok(models) = list_models(config, Some(model)) else {
        return;
    };
    config.adopt_window(advertised_window(&models, Some(model)));
}

/// Whether the roster says `chosen` reads an effort level.
///
/// Split from the fetch so the matching is testable without a server. True for a model no listing
/// described, which covers a name from a settings file and a listing that could not be fetched:
/// neither is the roster saying a level would be ignored, and only the roster saying so is a reason
/// to withhold what somebody asked for.
fn reads_effort(models: &[bravebot_aichat::models::Model], chosen: Option<&str>) -> bool {
    let Some(name) = chosen else {
        return true;
    };
    models
        .iter()
        .find(|model| model.key == name)
        .is_none_or(|model| model.reads_effort)
}

/// The window advertised for `chosen`, or `None` where the listing does not describe it.
///
/// Split from the fetch so the matching is testable without a server. Nothing chosen means
/// `automatic`, whose model is resolved per request, so no entry's window is the one in force.
fn advertised_window(
    models: &[bravebot_aichat::models::Model],
    chosen: Option<&str>,
) -> Option<u64> {
    let name = chosen?;
    models
        .iter()
        .find(|model| model.key == name)?
        .conversation_tokens
}

fn choose_model(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &mut Config,
) {
    match list_models(config, session.model()) {
        Ok(models) => {
            let chosen = crate::model_prompt::choose(terminal, models, session.model(), |frame| {
                render::draw(frame, session);
            });
            if let Some(chosen) = chosen {
                // The listing is the only place a window is ever reported, so the budget is taken
                // here, while the entry that named it is in hand. Said once when it changes, since
                // a budget belongs to the model rather than to a turn.
                if config.adopt_window(chosen.conversation_tokens) {
                    session.note(t!(session_context_budget, budget = config.context_budget));
                }
                session.update_budget(config.context_budget, config.budget_is_guessed());
                // Which service answers is said here as well as in the picker: the row that
                // carried it is gone by the time the note is read, and the same slug reached
                // through two services is two bills.
                session.note(match chosen.provider.as_deref() {
                    Some(service) => t!(
                        session_using_model_from,
                        model = &chosen.display_name,
                        service = service
                    ),
                    None => t!(session_using_model, model = &chosen.display_name),
                });
                session.note_model_reads_effort(chosen.reads_effort);
                if !chosen.reads_effort && session.effort().is_some() {
                    session.note(t!(session_effort_not_read));
                }
                session.choose_model(chosen.key);
            }
        }
        Err(detail) => session.note(t!(session_models_unavailable, problem = detail)),
    }
}

/// Open the theme picker and persist what the person chose.
fn choose_theme(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, session: &mut Session) {
    let current = crate::theme::name();
    let themes = crate::theme::listed(&current);
    if let Some(chosen) = crate::theme_prompt::choose(terminal, themes, &current, |frame| {
        render::draw(frame, session);
    }) {
        crate::store::save_theme(&chosen.name);
        session.note(t!(session_theme_set, theme = &chosen.name));
    }
}

/// Open the effort picker and keep what the person chose.
fn choose_effort(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, session: &mut Session) {
    if let Some(row) = crate::effort_prompt::choose(terminal, session.effort(), |frame| {
        render::draw(frame, session);
    }) {
        session.choose_effort(row.0);
        session.note(said_of(row.0));
        say_if_unread(session);
    }
}

/// Open the panel of preferences and keep what the person chose.
fn choose_editing(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, session: &mut Session) {
    if let Some(row) = crate::config_prompt::choose(terminal, session.editing(), |frame| {
        render::draw(frame, session);
    }) {
        session.choose_editing(row.0);
        // Said out loud, because the choice changes what the next keystroke does and the box gives no
        // other sign of it until a letter has already gone somewhere unexpected. The mode beneath the
        // box says which vi mode is in force, and this says the style was chosen at all.
        session.note(match row.0 {
            crate::vim::Editing::Vi => t!(session_editing_vi),
            crate::vim::Editing::Ordinary => t!(session_editing_ordinary),
        });
    }
}

/// Say that the model in force reads no level, where one has just been asked for.
///
/// The choice is kept either way: a person may be about to change model, and throwing away what
/// they just picked would make the two commands depend on the order they were typed in.
fn say_if_unread(session: &mut Session) {
    if session.effort().is_some() && !session.model_reads_effort() {
        session.note(t!(session_effort_not_read));
    }
}

/// Take a level by name without opening the picker.
///
/// A word this program does not know changes nothing and says so. It must not reach a request
/// field, and silently ignoring it would leave somebody believing they had asked for something.
fn set_effort(session: &mut Session, word: &str) {
    match bravebot_aichat::protocol::Effort::named(word) {
        Some(level) => {
            session.choose_effort(Some(level));
            session.note(said_of(Some(level)));
            say_if_unread(session);
        }
        None => session.note(t!(session_no_such_effort, effort = word)),
    }
}

/// What the session says about the level now in force.
fn said_of(effort: Option<bravebot_aichat::protocol::Effort>) -> String {
    match effort {
        Some(level) => t!(session_effort_set, effort = level.as_str()),
        None => t!(session_effort_unset).to_string(),
    }
}

/// Apply a theme by name without opening the picker.
fn set_theme(session: &mut Session, name: &str) {
    match crate::theme::find(name) {
        Some(theme) => {
            crate::theme::apply(&theme);
            crate::store::save_theme(&theme.name);
            session.note(t!(session_theme_set, theme = &theme.name));
        }
        None => session.note(t!(session_no_such_theme, theme = name)),
    }
}

/// Where a session's opening trust map came from, which is what it says about it.
#[derive(Debug, Clone, Copy)]
enum Whence {
    /// The person answered the startup question just now.
    Asked,
    /// The record of the session being picked up, so the answer is that session's user's own.
    Resumed,
    /// Nobody was asked, because the mode in force answers this question too.
    Unasked,
}

/// What becomes of the directories a settings file named.
#[derive(Debug, PartialEq, Eq)]
enum Named {
    /// Opened with nothing put to anybody.
    Opening(Vec<String>),
    /// Put to the person, one question per directory.
    Asking(Vec<String>),
}

/// What becomes of the directories a settings file named, for a session that opened this way.
///
/// A file names them and a person grants them, so whoever answered for the working directory
/// answers for these: a person who was asked about it is asked about each of these, and the mode
/// that answers every question answers these too, on the terms it answers that one.
///
/// A resumed session opens none of them. Nothing is put to it, and the directories it has open are
/// the ones its own record reopened, so a name in a file that has been edited since is not a grant
/// that session's user made.
fn named_directories(whence: Whence, named: &[String]) -> Named {
    match whence {
        Whence::Asked => Named::Asking(named.to_vec()),
        Whence::Unasked => Named::Opening(named.to_vec()),
        // Nothing is resolved for this one either: a name is a path on the filesystem, and
        // reading the disk to decide about directories this session will not open is work done
        // for a question nobody is being asked.
        Whence::Resumed => Named::Opening(Vec::new()),
    }
}

/// The directories the names in a settings file would open, in the order they were named.
///
/// Resolved before anything is asked, because what a person is shown has to be what they would be
/// granting: a name is canonicalized when it is opened, so `shared` pointing somewhere else opens
/// the tree at the other end of it, and a box showing the spelling would collect an answer to a
/// different question. A name that cannot be opened at all is said so rather than asked about, and
/// a path two layers both named is one request rather than the same question twice.
fn named_to_open(session: &mut Session, workspace: &Workspace, named: &[String]) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for directory in named {
        // Home first, then the workspace, so `~/notes` means the home directory rather than a
        // directory called `~` under the project. `/cd` resolves a name in that order too.
        let candidate = against_workspace(workspace.root(), &expand_home(directory));
        match workspace.resolve_directory(&candidate) {
            Ok(path) => {
                let shown = path.display().to_string();
                if !paths.contains(&shown) {
                    paths.push(shown);
                }
            }
            Err(problem) => session.note(t!(
                session_directory_not_added,
                directory = directory,
                problem = problem
            )),
        }
    }
    paths
}

/// Open and vouch for each directory that was accepted, and nothing else.
///
/// Separated so the other half can be tested where it bites: a name nobody accepted leaves its
/// path as unreachable and as unvouched for as any other outside the workspace.
fn open_named(
    session: &mut Session,
    workspace: &mut Workspace,
    trust: &mut TrustStore,
    accepted: &[String],
) {
    for directory in accepted {
        add_directory(session, workspace, trust, directory);
    }
}

/// What a session brings to the startup question, which is all that decides whether it is put.
#[derive(Debug)]
enum Beginning {
    /// A launch with nothing behind it, and `/clear`, which begins a session too. Nothing is
    /// carried in, whatever any earlier session in this directory answered.
    New,
    /// A resume, holding the map the record being picked up kept. `None` where that record
    /// predates maps being kept, which is asked about like anything else unanswered.
    Resumed(Option<TrustStore>),
}

/// What the way a session was started leaves it holding.
///
/// Only a resume brings a map, and the map it brings is the record of the session it is picking
/// up, so the answer honoured is the one that session's own user gave. The directory's other
/// records are not read: a map taken from one of those would be standing permission granted on
/// behalf of somebody who was never asked.
fn beginning_of(start: &Start, root: &std::path::Path) -> Beginning {
    match start {
        // Choosing has already resolved into one of the other two by the time this runs.
        Start::Fresh | Start::Choose => Beginning::New,
        // Read under the directory being resumed into rather than the one recorded, so a project
        // that was moved or renamed since resumes with its rules about the same files.
        Start::Resuming(record) => Beginning::Resumed(record.trust_map(root)),
    }
}

/// Where a session's opening map comes from, or that there is nobody to take it from.
#[derive(Debug)]
enum Opening {
    /// Settled without asking, and what that says about where it came from.
    Settled(TrustStore, Whence),
    /// Nothing has answered, so the person is.
    Ask,
}

/// Where the map comes from for a session that began this way, under this mode.
///
/// Everything about the answer bar the terminal it is put on, separated from [`opening_trust`] so
/// it can be decided without one, the way [`crate::trust_prompt::answered_by`] is.
fn opening_for(
    beginning: Beginning,
    mode: bravebot_agent::PermissionMode,
    root: &std::path::Path,
) -> Opening {
    match beginning {
        // Before the mode is consulted, because the question is not being put in either case and
        // the map this session's own user gave is the more specific record.
        Beginning::Resumed(Some(trust)) => Opening::Settled(trust, Whence::Resumed),
        Beginning::New | Beginning::Resumed(None) => {
            match crate::trust_prompt::answered_by(mode, root) {
                Some(trust) => Opening::Settled(trust, Whence::Unasked),
                None => Opening::Ask,
            }
        }
    }
}

/// The trust map the session starts with, or nothing if the user asked to leave.
///
/// A fresh session asks, whatever any session in this directory answered before. The
/// question grants standing permission, and a launch that skipped it because someone said yes
/// last week would be granting that permission on behalf of a user who was never asked, which is
/// trust assumed from silence rather than granted.
///
/// Resuming is the one case that does not ask, and it is not an exception to that: the map comes
/// out of the record of the very session being picked up, so the answer being honoured is the one
/// its own user gave. It carries the rules that session's writes recorded too, which is what stops
/// a resumed turn reading back a file an earlier turn poisoned. A record from before the map was
/// kept has none, and is asked about.
///
/// A session bypassing every permission is not asked either, and takes the map a yes would have
/// written. Resuming still wins over that: the question is not being put in either case, so there
/// is nothing for the mode to answer, and the map its own user gave is the more specific record.
fn opening_trust(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    root: &std::path::Path,
    beginning: Beginning,
) -> Option<(TrustStore, Whence)> {
    let (trust, whence) = match opening_for(beginning, session.permission_mode(), root) {
        Opening::Settled(trust, whence) => (trust, whence),
        Opening::Ask => (crate::trust_prompt::ask(terminal, root)?, Whence::Asked),
    };

    if !trust.is_trusted(".") {
        session.note(t!(session_not_trusting));
        return Some((trust, whence));
    }
    let where_it_is = root.display();
    // Named, because two of the three are a grant nobody made just now, and this line is the only
    // place that says where it came from.
    session.note(match whence {
        Whence::Asked => t!(session_trusting, directory = where_it_is),
        Whence::Resumed => t!(session_trusting_as_left, directory = where_it_is),
        Whence::Unasked => t!(session_trusting_unasked, directory = where_it_is),
    });
    Some((trust, whence))
}

/// Run a command the user typed in shell mode, redrawing while it runs.
///
/// On a worker thread for the reason a turn is: a command can take as long as it likes, and running
/// it on the thread that owns the terminal would freeze the interface for the duration and make a
/// slow build indistinguishable from a hang. Escape cancels, which kills it.
///
/// No approval is asked for. The prompt a run normally goes through exists so a person endorses argv
/// the *planner* chose, and here the person typed it themselves: asking would be asking them to
/// confirm their own keystroke. What it printed goes into the conversation, labelled from that same
/// provenance by the kernel. See [`bravebot_agent::shell::record`].
fn run_command(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    workspace: &Workspace,
    line: &str,
    conversation: &mut Conversation,
) -> io::Result<Vec<Stamped>> {
    let cancel = Cancel::new();
    let worker_cancel = cancel.clone();
    let worker_line = line.to_string();
    // Commands run in the primary root, which is the directory the prompt says it is in. An added
    // directory is reachable by path from here, so nothing is out of reach.
    let directory = workspace.root().to_path_buf();

    // Only the process goes to the worker. The conversation stays here, because handing it over
    // means taking it back, and a panicked thread hands back nothing: substituting a fresh one would
    // reset context integrity to trusted, which is an upgrade and never allowed.
    let worker =
        thread::spawn(move || bravebot_agent::shell::run(&worker_line, &directory, &worker_cancel));

    // Drawn while it runs so the command appears in the transcript immediately and Escape has
    // somewhere to be pressed. The spinner is the turn indicator's, without the token counters,
    // which measure something no command spends.
    session.begin_command();
    while !worker.is_finished() {
        redraw(terminal, session)?;

        while event::poll(Duration::ZERO)? {
            match event::read()? {
                TermEvent::Key(key) if key.kind == KeyEventKind::Release => {}
                // A running command is something to stop, so Ctrl-C stops it and stays, for the
                // reason it stops a turn: the way out is the press after that, at the box.
                TermEvent::Key(key) if stops_the_turn(session, key) => {
                    cancel.cancel();
                }
                TermEvent::Mouse(mouse) => {
                    let action = handle_mouse(session, mouse);
                    if action == Action::Copy {
                        copy_selection(terminal, session)?;
                    }
                }
                // Everything else waits. A command is brief and the keys that matter during one are
                // the two above; taking a prompt here would leave it half-typed when the output
                // lands on top of it.
                _ => {}
            }
        }

        std::thread::sleep(FRAME.min(Duration::from_millis(50)));
    }

    let ran = worker.join().unwrap_or_else(|_| {
        Err(bravebot_agent::shell::ShellError::Io(
            t!(command_thread_stopped).to_string(),
        ))
    });
    session.finish_command();

    // The labelling happens here, on the thread that owns the conversation, and its trail is
    // returned so the decision reaches the audit file and Ctrl-T. Trusting a command's output is the
    // most consequential thing this feature does, so it must not be the one thing left unrecorded.
    let mut sink = Trail::new();
    match ran {
        Ok(ran) => match bravebot_agent::shell::record(line, &ran, conversation, &mut sink) {
            Ok(recorded) => {
                session.printed(&recorded.text);
                if !recorded.succeeded {
                    session.note(t!(command_reported_a_failure));
                }
            }
            Err(error) => session.note(format!("{error}")),
        },
        Err(error) => session.note(format!("{error}")),
    }

    Ok(sink.events().to_vec())
}

/// Interpret a key press while a single request is out.
///
/// A summary and an aside are one request each, with no round for a stop to land between, so
/// nothing here can stop the work: Ctrl-C leaves, since leaving is all the press can mean, and
/// Escape says so in the words the caller passes. Said once, because a key that does nothing and
/// says nothing reads as the interface having hung at the one moment it is working hardest.
///
/// Both keys are read against [`stops_the_turn`], as the turn's own loop reads them, so a scroller,
/// a view or a search opened while the request was out answers the press and the press that reaches
/// the request behind it is the next one. Without that, the key that closes those modes everywhere
/// else ended the session from inside them.
fn one_request_key(session: &mut Session, key: KeyEvent, uninterruptible: &str) {
    // Presses only, for the reason the outer loop ignores releases, and for one this function
    // brings with it: the press that closes a mode leaves nothing open, so its own release would
    // read as a second press with the way out ahead of it and end the session on the way up.
    if key.kind == KeyEventKind::Release {
        return;
    }

    if !stops_the_turn(session, key) {
        let action = handle_key_while_working(session, key);
        act_while_working(session, action, crate::clipboard::paste);
        return;
    }

    if is_ctrl_c(key) {
        session.quit();
    } else {
        session.note_once(uninterruptible);
    }
}

/// Summarise the conversation, showing the spinner while it happens.
///
/// A smaller relative of [`run_turn_animated`], and smaller because there is less to do: a
/// summariser has no tools, so nothing asks about a write, nothing asks the user a question, and
/// nothing lands in the workspace. What is left is a model call that takes as long as a round, and
/// a loop that keeps the screen alive while it does.
///
/// Not cancellable, deliberately. The call is one round with nothing to interrupt part way, and a
/// cancel would leave the same conversation it started with, which is what happens anyway if it
/// fails.
fn compact_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    conversation: Conversation,
    trust: &TrustStore,
) -> io::Result<(Conversation, Vec<Stamped>)> {
    // For the reason a turn does it: the summary is one request to the same backend, and a sign-in
    // is not something a worker thread can ask for.
    sign_in_if_needed(terminal, session, config)?;

    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();

    let worker_config = config.clone();
    let worker_trust = trust.clone();
    let model = session.model().map(str::to_string);

    session.begin_aside();

    let worker = thread::spawn(move || {
        let mut sink = Trail::new();
        let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main);
        let egress = Egress::new();
        let mut conversation = conversation;
        // Reduced to what a person can be told before it crosses back, since the error types are
        // the kernel's and this thread is the only place they mean anything.
        let done = turn::compact(
            &worker_config,
            &egress,
            &mut conversation,
            model.as_deref(),
            &mut reporter,
            &mut sink,
            worker_trust,
        )
        .map_err(|error| error.category().name().to_string());
        (done, conversation, sink)
    });

    loop {
        redraw(terminal, session)?;

        // Input is still read, so a long summary does not leave the interface deaf, and the
        // frame's waiting is done here for the reason the turn loop does it here: a key press
        // has to wake the loop rather than queue behind it.
        if event::poll(FRAME)? {
            while event::poll(Duration::ZERO)? {
                match event::read()? {
                    // The one place Ctrl-C still leaves with something in flight, and what a mode
                    // standing over the session takes ahead of it, are both that function's.
                    TermEvent::Key(key) => {
                        one_request_key(session, key, t!(compact_uninterruptible));
                    }
                    TermEvent::Paste(text) => {
                        let action = handle_paste_while_working(session, &text);
                        act_while_working(session, action, crate::clipboard::paste);
                    }
                    TermEvent::Mouse(mouse) => {
                        let action = handle_mouse(session, mouse);
                        if action == Action::Copy {
                            copy_selection(terminal, session)?;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Drained for the same reason the turn's own loop drains, and waiting on the terminal
        // for the same reason it does: a person may be typing their next prompt while this runs.
        let carrying_on = drain_worker(&from_worker, Duration::ZERO, |message| match message {
            crate::remote_confirm::ToMain::Phase(phase) => session.set_phase(phase),
            crate::remote_confirm::ToMain::Narration(text) => session.narrate(text),
            crate::remote_confirm::ToMain::Streaming(text) => session.streaming(&text),
            _ => {}
        });

        if !carrying_on {
            break;
        }
    }

    let (done, conversation, sink) = worker.join().unwrap_or_else(|_| {
        (
            Err(t!(compact_ended_unexpectedly).to_string()),
            Conversation::new(),
            Trail::new(),
        )
    });

    match &done {
        Ok(Some(summary)) => {
            session.end_aside(summary.usage.total());
            session.compacted();
            session.note(t!(
                compact_done,
                summarised = summary.summarised,
                kept = summary.kept
            ));
        }
        Ok(None) => {
            session.end_aside(0);
            session.note(t!(compact_nothing_to_do));
        }
        Err(message) => {
            session.end_aside(0);
            session.note(t!(compact_failed, problem = message));
        }
    }

    Ok((conversation, sink.events().to_vec()))
}

/// Ask one question beside the work, redrawing while it is answered.
///
/// The same shape as `compact_animated`, and for the same reasons: one request, off the thread
/// that owns the terminal, with the loop below drawing so a slow answer does not read as a hang.
///
/// The conversation is lent by reference and comes back unchanged, which is the whole of what
/// makes this an aside. Nothing here can push a message, so there is no path by which the question
/// or its answer reaches the exchange a later turn resumes.
///
/// A question that could not be answered leaves a line in the transcript rather than a row with
/// nothing in it. The gates it did pass are still returned, since they decided about a request
/// that really went out.
fn aside_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    conversation: &Conversation,
    trust: &TrustStore,
    question: &str,
) -> io::Result<Vec<Stamped>> {
    // For the reason a turn and a summary both do it: this is one request to the same backend,
    // and a sign-in is not something a worker thread can ask for.
    sign_in_if_needed(terminal, session, config)?;

    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();

    let worker_config = config.clone();
    let worker_trust = trust.clone();
    let model = session.model().map(str::to_string);
    // Taken here, before the worker starts, because that is what crosses to it: the request, not
    // the conversation. The conversation stays on this thread and is not touched again, so there
    // is no path by which an aside could add anything to it.
    let asking = bravebot_agent::aside::Question::about(conversation, question);
    let asked = question.to_string();

    session.begin_aside();

    let worker = thread::spawn(move || {
        let mut sink = Trail::new();
        let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main);
        let egress = Egress::new();
        // Reduced to what a person can be told before it crosses back, since the error types are
        // the kernel's and this thread is the only place they mean anything.
        let done = turn::aside(
            &worker_config,
            &egress,
            asking,
            model.as_deref(),
            &mut reporter,
            &mut sink,
            worker_trust,
            // Taken and dropped. An answer has to be released to be looked at, and the one place
            // this side could draw it as it arrives is the tail the turn's own half-written reply
            // fills, which is among the turn's own lines.
            |_written| {},
        )
        .map_err(|e| e.to_string());
        (done, sink)
    });

    loop {
        redraw(terminal, session)?;

        // Input is still read for the reason a summary reads it: a long answer must not leave the
        // interface deaf, and the frame's waiting is done here so a key press wakes the loop
        // rather than queueing behind the worker.
        if event::poll(FRAME)? {
            while event::poll(Duration::ZERO)? {
                match event::read()? {
                    // The same shape as a summary's keys, and the same function reads them.
                    TermEvent::Key(key) => {
                        one_request_key(session, key, t!(btw_uninterruptible));
                    }
                    TermEvent::Paste(text) => {
                        let action = handle_paste_while_working(session, &text);
                        act_while_working(session, action, crate::clipboard::paste);
                    }
                    TermEvent::Mouse(mouse) => {
                        let action = handle_mouse(session, mouse);
                        if action == Action::Copy {
                            copy_selection(terminal, session)?;
                        }
                    }
                    _ => {}
                }
            }
        }

        let carrying_on = drain_worker(&from_worker, Duration::ZERO, |message| {
            aside_reported(session, message);
        });

        if !carrying_on {
            break;
        }
    }

    let (done, sink) = worker
        .join()
        .unwrap_or_else(|_| (Err(t!(btw_ended_unexpectedly).to_string()), Trail::new()));

    match done {
        Ok(answered) => aside_answered(session, asked, answered),
        Err(message) => {
            session.end_aside(0);
            session.note(t!(btw_failed, problem = message));
        }
    }

    Ok(sink.events().to_vec())
}

/// Take what a question asked beside the work came back with.
///
/// WATCH-18: the row and the view are where an aside is read, and neither half of it is anywhere
/// else. Apart from [`aside_animated`], which needs a terminal and a backend to reach, so that
/// what an answer does to the session is somewhere a test can call.
fn aside_answered(session: &mut Session, asked: String, answered: bravebot_agent::aside::Answered) {
    session.end_aside(answered.usage.total());
    // The row and the view are the whole of it, and nothing joins the transcript. A line there
    // about an exchange the planner has read no part of is one a reader takes it to have had, and
    // what says afterwards that an aside happened is the hint line, which counts the rows and
    // names the key that opens them.
    session.asked_aside(crate::state::Aside {
        question: asked,
        answer: Some(answered.shown),
        kept: answered.kept.is_some(),
    });
}

/// Take what the thread answering a question beside the work says while it works.
///
/// Apart from [`aside_animated`] for the reason [`aside_answered`] is, and under the same clause:
/// an answer arriving as it is written is half of an aside, and the one place this side could draw
/// it is the tail where the planner's own half-written reply goes.
fn aside_reported(session: &mut Session, message: crate::remote_confirm::ToMain) {
    match message {
        crate::remote_confirm::ToMain::Phase(phase) => session.set_phase(phase),
        crate::remote_confirm::ToMain::Notice(text) => session.note_once(text),
        _ => {}
    }
}

/// Plan one task in full, put the frozen plan to the person, and walk it, redrawing throughout.
///
/// The other kind of run, started from a session rather than from the command line. Between
/// [`run_turn_animated`] and [`aside_animated`] in what it needs: it asks about a plan and about
/// every write a step reaches, so it has the turn's answer channel, and it gives the conversation
/// nothing, so it takes none.
///
/// **The session is blocked for the duration.** A manifest run is one run with a frozen plan, and
/// there is no planner in it to hand a line to: the person cannot interleave a turn with it, and
/// the loop below refuses to send. What they can still do is everything the turn loop lets them do
/// while a turn runs, which is read the transcript, edit the box, and stop what is in flight.
///
/// **The conversation is neither read nor written.** It is not lent here at all, which is stronger
/// than an aside's promise to hand it back unchanged: the planner that reads the task is a fresh
/// one whose context holds the task string and the driver's own words (MANIFEST-1), so sending the
/// conversation would break the gate rather than merely widen it, and a step's result is
/// quarantined (MANIFEST-8) so there is nothing it could give back. What the transcript shows is
/// the goal as the planner understood it, the frozen plan, each step as it runs, and the reply, all
/// of it released for a screen and none of it in the exchange a later turn resumes.
///
/// **The run is written down as its own record**, by the same function the command line uses, and
/// the session notes the id. That keeps the presence of `manifest` in a record the thing that makes
/// it a manifest run: this session's own record stays a conversation, so it still resumes, and the
/// run's record still has no conversation, so the picker still refuses it for the right reason.
///
/// The trust map is lent and comes back unused. A grant a step was given belongs to the run, and
/// travels into the run's own record with everything else it produced; folding it into the session
/// would leave the conversation holding a yes that was given about a plan it never saw.
#[allow(clippy::too_many_arguments)]
fn manifest_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    workspace: &Workspace,
    task: &str,
    trust: &TrustStore,
    permissions: &Permissions,
) -> io::Result<Vec<Stamped>> {
    // For the reason a turn does it: a sign-in needs the terminal, and this is the thread that has
    // it. Left to the worker, the URL and code the AWS CLI prints would land nowhere anyone reads.
    sign_in_if_needed(terminal, session, config)?;

    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();
    let (answer_tx, answer_rx) = mpsc::channel::<crate::remote_confirm::Reply>();

    // A fresh token, as a turn takes: reusing one could stop a run before it started.
    let cancel = Cancel::new();
    let worker_cancel = cancel.clone();

    let worker_config = config.clone();
    let worker_workspace = workspace.clone();
    let worker_trust = trust.clone();
    // The mode as it stands now. A run keeps the one it began with, for the reason a turn does: a
    // key pressed while it walks describes what comes after it, and a plan already on the screen
    // must not have the question withdrawn from under the person answering it.
    let permission_mode = session.permission_mode();
    // No files, no attachments and no pasted pictures. Every one of those is context, and this mode
    // fixes its plan before it observes anything; the task string is the whole of the input, which
    // is the same reason a pipe is refused (MANIFEST-9).
    let worker_task = Task::new(task)
        .with_home(bravebot_agent::home::directory())
        .with_model(session.model().map(str::to_string))
        .with_effort(session.effort_in_force())
        .with_permissions(permissions.clone())
        .with_permission_mode(permission_mode);
    // Nothing is said about the standing form of a write answer, so nothing offers it. A plan has
    // no standing answer at all (MANIFEST-10), and a run whose steps were fixed before anything was
    // read is the worst place to record one: the key would be pressed about a step in a plan that
    // is written afresh for every run.
    let asked = task.to_string();

    session.begin_aside();
    session.note(t!(manifest_began));

    let worker = thread::spawn(move || {
        let mut sink = Trail::new();
        let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main.clone());
        // A queue of its own, and empty. A line typed while a run walks is not something the run
        // can take: there is no planner in it to hand one to, so the queue the session holds stays
        // the session's and the next turn sends it.
        let mut asking = crate::remote_confirm::RemoteConfirmer::new(
            to_main,
            answer_rx,
            crate::remote_confirm::Interjections::new(),
        );
        let mut confirmer = bravebot_agent::Confining::new(&mut asking, permission_mode);
        let egress = Egress::new();
        let outcome = bravebot_agent::manifest::run(
            &worker_config,
            &egress,
            &worker_workspace,
            &worker_task,
            &mut confirmer,
            &mut reporter,
            &mut sink,
            worker_trust,
            &worker_cancel,
        );
        (outcome, sink)
    });

    loop {
        redraw(terminal, session)?;

        // Read here rather than in the outer loop, which is blocked for the duration, and for the
        // reason a turn reads it here: a run that walks for ten minutes must not leave the
        // interface deaf, and the frame's waiting is done here so a key press wakes the loop.
        if event::poll(FRAME)? {
            while event::poll(Duration::ZERO)? {
                match event::read()? {
                    TermEvent::Key(key) if key.kind == KeyEventKind::Release => {}
                    // Both keys stop the run and neither leaves, exactly as in a turn. A person
                    // watching a plan go wrong is asking for the plan to stop; the next press, at
                    // the box, is the one that leaves.
                    TermEvent::Key(key) if stops_the_turn(session, key) => {
                        cancel.cancel();
                    }
                    TermEvent::Key(key) => {
                        let action = handle_key_while_working(session, key);
                        act_while_working(session, action, crate::clipboard::paste);
                    }
                    TermEvent::Paste(text) => {
                        let action = handle_paste_while_working(session, &text);
                        act_while_working(session, action, crate::clipboard::paste);
                    }
                    TermEvent::Mouse(mouse) => {
                        let action = handle_mouse(session, mouse);
                        if action == Action::Copy {
                            copy_selection(terminal, session)?;
                        }
                    }
                    _ => {}
                }
            }
        }

        let carrying_on = drain_worker(&from_worker, Duration::ZERO, |message| match message {
            // The one question this mode asks that a turn does not, and the reason the session
            // prompt is worth reaching: it is drawn and scrolled rather than printed and read off
            // a line, so a plan longer than the window can be walked back through before it is
            // answered.
            crate::remote_confirm::ToMain::Manifest(request) => {
                let answer = crate::confirm::ask_manifest(terminal, &request);
                if answer == crate::confirm::Answer::Interrupt {
                    cancel.cancel();
                }
                // Nothing is noted on the transcript, for the reason a turn notes nothing: the
                // answer covers this plan and no other, so there is no standing decision to
                // record, and the plan is about to be walked in the open where the transcript
                // shows every step of it.
                let _ = answer_tx.send(crate::remote_confirm::Reply::Manifest(answer.decision()));
            }
            // Approving the plan was not approving its writes, so each one is still put to the
            // person as its step reaches it.
            crate::remote_confirm::ToMain::Write(request) => {
                let answer = crate::confirm::ask(terminal, &request);
                if answer == crate::confirm::Answer::Interrupt {
                    cancel.cancel();
                }
                let _ = answer_tx.send(crate::remote_confirm::Reply::Write(answer.decision()));
            }
            crate::remote_confirm::ToMain::Fetch(request) => {
                let answer = crate::confirm::ask_fetch(terminal, &request);
                if answer == crate::confirm::Answer::Interrupt {
                    cancel.cancel();
                }
                let _ = answer_tx.send(crate::remote_confirm::Reply::Fetch(answer.decision()));
            }
            crate::remote_confirm::ToMain::Vouch(request) => {
                let answer = crate::confirm::ask_vouch(terminal, &request);
                if answer == crate::confirm::Answer::Interrupt {
                    cancel.cancel();
                }
                let _ = answer_tx.send(crate::remote_confirm::Reply::Vouch(answer.decision()));
            }
            // Progress, with no reply to give. The goal as the planner understood it and the frozen
            // plan both arrive as narration, and each step as an activity, so the transcript of a
            // run reads the way the transcript of a turn does.
            crate::remote_confirm::ToMain::Written(written) => session.set_written(written),
            crate::remote_confirm::ToMain::Phase(phase) => session.set_phase(phase),
            crate::remote_confirm::ToMain::Narration(text) => session.narrate(text),
            crate::remote_confirm::ToMain::Notice(text) => session.note_once(text),
            crate::remote_confirm::ToMain::Streaming(text) => session.streaming(&text),
            crate::remote_confirm::ToMain::Started(activity) => session.start_activity(activity),
            crate::remote_confirm::ToMain::Finished(activity) => session.finish_activity(activity),
            crate::remote_confirm::ToMain::Quarantined(shown) => session.show(shown),
            crate::remote_confirm::ToMain::Landed(landing) => session.landed(landing),
            // The questions a turn asks that this mode cannot. There is no shell and no `run` in
            // the schema (MANIFEST-5), so no pipeline is proposed and no output is read back;
            // there is no step that asks to be shown a slot; and there is no planner left to pose
            // a question. None of the four can arrive, and each
            // of them is a question the worker is *blocked* on, so silence here would be a hang
            // nothing can break: the loop would go round forever with the worker waiting on a
            // reply and the cancel token never looked at. Answered the way every other failure to
            // carry a question is answered, with the negative one.
            crate::remote_confirm::ToMain::Run(_) => {
                let _ = answer_tx.send(crate::remote_confirm::Reply::Run(
                    bravebot_agent::confirm::RunDecision::reject(),
                ));
            }
            crate::remote_confirm::ToMain::ReadOutput(_) => {
                let _ = answer_tx.send(crate::remote_confirm::Reply::ReadOutput(
                    bravebot_agent::confirm::Decision::Reject,
                ));
            }
            crate::remote_confirm::ToMain::Vet(_) => {
                let _ = answer_tx.send(crate::remote_confirm::Reply::Vet(
                    bravebot_agent::confirm::Decision::Reject,
                ));
            }
            crate::remote_confirm::ToMain::Server(_) => {
                let _ = answer_tx.send(crate::remote_confirm::Reply::Server(
                    bravebot_agent::confirm::Decision::Reject,
                ));
            }
            crate::remote_confirm::ToMain::Ask(_) => {
                let _ = answer_tx.send(crate::remote_confirm::Reply::Ask(Vec::new()));
            }
            // What is left announces rather than asks, so nothing waits on it. The manifest is the
            // task list, so no list changes; there is no planner to delegate or to be interjected
            // at; and a run's steps report through `Started` and `Finished` above.
            _ => {}
        });

        if !carrying_on {
            break;
        }
    }

    let (outcome, sink) = worker.join().unwrap_or_else(|_| {
        (
            Err(bravebot_agent::TurnError::Precommit(
                t!(manifest_ended_unexpectedly).to_string(),
            )),
            Trail::new(),
        )
    });

    let stopped_by_the_person = was_stopped(&outcome, &cancel);

    // Only from a run that finished. A run that stopped comes back as an error carrying what it
    // produced (MANIFEST-3) and no figures, so the tokens it did spend are not recoverable here,
    // and the breakdown is absent rather than guessed.
    let (tokens, spent) = match &outcome {
        Ok(finished) => (finished.tokens, Some(finished.timing)),
        Err(_) => (0, None),
    };
    session.end_run(tokens, spent);

    // Nothing for a run the person stopped, which is what the command line does with one too: it
    // has nothing in it anybody needs to read, and a record per interrupted run would fill the
    // picker with rows whose whole content is that somebody changed their mind. Every other
    // outcome is written before anything is said, so the note can name it.
    let recorded = match stopped_by_the_person {
        true => None,
        false => crate::sessions::record_manifest_run(workspace.root(), &asked, &outcome),
    };

    match &outcome {
        Ok(finished) => {
            // Released for a screen, exactly as a turn's reply is, and into the transcript rather
            // than into the conversation: what this run said is not something a later turn holds.
            // Nothing is said about that here. A reply that arrived is the run having worked, and a
            // line under it announcing what did not happen reads as a complaint about the run.
            session.narrate(finished.reply_for_display());
        }
        // Stopped by the person, so there is nothing to report. The transcript already holds the
        // steps that ran, and a complaint about a run they turned off themselves is noise.
        Err(_) if stopped_by_the_person => {}
        Err(failure) => {
            session.note(t!(manifest_failed, problem = failure.to_string()));
        }
    }
    // Last, so it is the line under the reply or under what went wrong. The run's own record is
    // where the plan, the proposal and the steps are kept in full, and the id is the only way in:
    // the picker marks a run's row but refuses Enter on it, so naming the id on the command line is
    // what reads one back, and a person who never saw this line has no way to ask for it.
    if let Some(id) = recorded {
        session.note(t!(manifest_recorded, id = &id));
    }

    Ok(sink.events().to_vec())
}

/// Whether a manifest run ended because the person stopped it.
///
/// Read off the token they set rather than off the error that came back, and that is the whole of
/// why this is a function. The two keys that stop a run reach it as two different things: pressed at
/// the plan prompt they are answered as a decline, so the run reports a plan nobody approved, and
/// pressed a step later they are a cancellation. Matching on `Cancelled` would therefore make the
/// same key mean two things depending on the moment it was pressed, and the person who pressed it
/// asked for the same thing at both.
///
/// A run that finished is never this, however late the key arrived: the work is done, and throwing
/// away the record of a run that completed would lose what it did.
fn was_stopped(
    outcome: &Result<bravebot_agent::Outcome, bravebot_agent::TurnError>,
    cancel: &Cancel,
) -> bool {
    outcome.is_err() && cancel.is_cancelled()
}

/// Interpret a key press while the goal check is out.
///
/// Read against [`stops_the_turn`] first, for the reason a summary reads it, and here the mode is
/// the likelier of the two to be open: nothing closes a view when a turn ends, so one opened while
/// the turn ran is still standing over the session when the goal goes to the judge.
///
/// Past that, both keys mean stop and there is something here to stop. The request in flight is one
/// round with nothing to cancel between, but the goal behind it is the thing a person watching this
/// actually wants off: without that, the key pressed at the ninth round would leave the session
/// rather than end the goal, and the tenth would go out anyway. With the goal already off Ctrl-C
/// means what it means everywhere else, and Escape has nothing left to ask for.
fn goal_check_key(session: &mut Session, key: KeyEvent) {
    // Presses only, as in [`one_request_key`]: the release of the press that closed a mode arrives
    // with nothing open and would clear the goal the press was never about.
    if key.kind == KeyEventKind::Release {
        return;
    }

    if !stops_the_turn(session, key) {
        let action = handle_key_while_working(session, key);
        act_while_working(session, action, crate::clipboard::paste);
        return;
    }

    if session.goal().is_some() {
        session.clear_goal();
        session.note_once(t!(goal_uninterruptible));
    } else if is_ctrl_c(key) {
        session.quit();
    }
}

/// Put the session's stopping condition to a judge, and give back the prompt that carries the work
/// on where it is not met yet.
///
/// `None` whenever nothing should happen: no goal is set, or the turn that just ended produced no
/// answer to judge. A turn that failed leaves the goal armed and is not judged, because a request
/// that never came back says nothing about whether the work is finished; the next turn is judged
/// instead.
///
/// The same shape as `aside_animated`, and for the same reasons: one request, off the thread that
/// owns the terminal, with the loop below drawing so a slow answer does not read as a hang. The
/// conversation is lent by reference and comes back unchanged. What goes back into it is the
/// driver's own sentence, sent as an ordinary prompt by the caller, so the exchange only ever
/// grows the way it grows for anything else.
fn goal_check_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    conversation: &Conversation,
    trust: &TrustStore,
) -> io::Result<(Option<String>, Vec<Stamped>)> {
    if session.goal().is_none() {
        return Ok((None, Vec::new()));
    }
    // Read rather than assumed: `begin_aside` below sets the session working again, and after that
    // there is no telling a turn that answered from one that did not.
    if session.finished.is_none_or(|turn| turn.ending.unanswered()) {
        return Ok((None, Vec::new()));
    }
    // An invariant of the branch above, not a runtime condition.
    // nosemgrep: trailofbits.rs.panic-in-function-returning-result.panic-in-function-returning-result
    let condition = session
        .goal()
        .expect("the goal was there a moment ago")
        .condition()
        .to_string();

    // For the reason a turn and an aside both do it: this is one request to the same backend, and
    // a sign-in is not something a worker thread can ask for.
    sign_in_if_needed(terminal, session, config)?;

    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();

    let worker_config = config.clone();
    let worker_trust = trust.clone();
    let model = session.model().map(str::to_string);
    // Taken here, before the worker starts, because that is what crosses to it: the request, not
    // the conversation. The conversation stays on this thread and is not touched again.
    let check = bravebot_agent::goal::Check::of(conversation, &condition);

    session.begin_aside();

    let worker = thread::spawn(move || {
        let mut sink = Trail::new();
        let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main);
        let egress = Egress::new();
        // Reduced to what a person can be told before it crosses back, since the error types are
        // the kernel's and this thread is the only place they mean anything.
        let done = turn::goal(
            &worker_config,
            &egress,
            check,
            model.as_deref(),
            &mut reporter,
            &mut sink,
            worker_trust,
        )
        .map_err(|e| e.to_string());
        (done, sink)
    });

    loop {
        redraw(terminal, session)?;

        // Input is still read for the reason an aside reads it: a slow answer must not leave the
        // interface deaf, and the frame's waiting is done here so a key press wakes the loop
        // rather than queueing behind the worker.
        if event::poll(FRAME)? {
            while event::poll(Duration::ZERO)? {
                match event::read()? {
                    // Which of the goal, a mode over the session, and the session itself a stop
                    // key is asking about is that function's to say.
                    TermEvent::Key(key) => goal_check_key(session, key),
                    TermEvent::Paste(text) => {
                        let action = handle_paste_while_working(session, &text);
                        act_while_working(session, action, crate::clipboard::paste);
                    }
                    TermEvent::Mouse(mouse) => {
                        let action = handle_mouse(session, mouse);
                        if action == Action::Copy {
                            copy_selection(terminal, session)?;
                        }
                    }
                    _ => {}
                }
            }
        }

        let carrying_on = drain_worker(&from_worker, Duration::ZERO, |message| match message {
            crate::remote_confirm::ToMain::Phase(phase) => session.set_phase(phase),
            crate::remote_confirm::ToMain::Notice(text) => session.note_once(text),
            _ => {}
        });

        if !carrying_on {
            break;
        }
    }

    let (done, sink) = worker
        .join()
        .unwrap_or_else(|_| (Err(t!(goal_ended_unexpectedly).to_string()), Trail::new()));

    let assessed = match done {
        Ok(assessed) => assessed,
        Err(message) => {
            session.end_aside(0);
            session.drop_goal();
            session.note(t!(goal_failed, problem = message));
            return Ok((None, sink.events().to_vec()));
        }
    };
    session.end_aside(assessed.usage.total());

    // A goal taken off while the check was in flight is a person having said stop. The verdict is
    // about a goal that no longer exists, so it is not acted on and not reported: telling them the
    // condition cannot be met, a moment after they cleared it, describes a session they are no
    // longer in.
    if session.goal().is_none() {
        return Ok((None, sink.events().to_vec()));
    }

    // Four of the five verdicts end the goal, and the person is told which one it was in each
    // case. Only one sends the work back, and even that one stops where the rounds are spent.
    let carrying_on = match assessed.verdict {
        bravebot_agent::goal::Verdict::NotMet { reason } => session.goal_not_met(reason),
        bravebot_agent::goal::Verdict::Met { reason } => {
            session.goal_met(reason);
            None
        }
        bravebot_agent::goal::Verdict::Impossible { reason } => {
            session.drop_goal();
            session.note(t!(goal_impossible, reason = &reason));
            None
        }
        bravebot_agent::goal::Verdict::Unreadable => {
            session.drop_goal();
            session.note(t!(goal_unreadable));
            None
        }
        bravebot_agent::goal::Verdict::Quarantined => {
            session.drop_goal();
            session.note(t!(goal_quarantined));
            None
        }
    };

    Ok((carrying_on, sink.events().to_vec()))
}

/// The record of lines remembered past a session for this workspace, and what it holds now.
///
/// `None` on a machine that names no state directory, which is a session with nothing to report.
///
/// The reading directory rather than the writable one: a session that adds nothing to
/// `~/.bravebot` still honours what an earlier one recorded, so it is still carrying those answers
/// and still has to be able to read them back.
///
/// Read rather than held, and keyed by the workspace root, which is the tree a person answered
/// about: what `make check` does depends on the tree it runs in.
fn remembered_record(
    workspace: &Workspace,
) -> Option<(
    bravebot_agent::remembered::Store,
    bravebot_core::remembered::Remembered,
)> {
    let store = bravebot_agent::remembered::Store::new(
        &bravebot_agent::home::directory()?,
        workspace.root(),
    );
    let lines = store.read();
    Some((store, lines))
}

/// What the session gets back from a turn and carries into the next one.
///
/// A struct rather than a tuple because every field is something the session owns and lends for
/// the length of one turn, and a caller taking six positional values back has no way of saying
/// which is which.
struct Continued {
    conversation: Conversation,
    trust: TrustStore,
    programs: TrustedPrograms,
    servers: Option<LanguageServers>,
    asked_about: AskedAbout,
    events: Vec<Stamped>,
}

/// Run a turn on a worker thread, redrawing while it works.
///
/// The turn itself blocks on network requests, so running it here would freeze the indicator on
/// its first frame and make a slow model look like a hang. Off-thread, the loop below keeps
/// drawing, and the elapsed time and spinner advance on their own.
///
/// Write approvals come back over a channel because only this thread owns the terminal. The
/// worker blocks until an answer arrives, which is what a write must wait for anyway.
#[allow(clippy::too_many_arguments)]
fn run_turn_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    workspace: &Workspace,
    prompt: &str,
    wrote: Wrote,
    conversation: Conversation,
    trust: TrustStore,
    programs: TrustedPrograms,
    // The servers this session has started, LSP-8, or `None` before the first turn has had the
    // chance to start one. Owned here and handed back like the conversation, because it is the
    // session that keeps it: built inside a turn it would be shut down at the end of that turn,
    // and the next message would ask the same person about the same language.
    servers: Option<LanguageServers>,
    asked_about: AskedAbout,
    permissions: &Permissions,
    // This session's own identifier. It travels with the task because a run prompt may be answered
    // with the key whose grant outlives the session, and the record of that says which session
    // pressed it so that `/status` can tell a person which answers they are still carrying.
    session_id: &str,
) -> io::Result<Continued> {
    // The prompt is in the transcript by now, and drawn before anything that might take a moment:
    // a check that has to run the AWS CLI holds the frame for as long as the process takes, and
    // until this the line somebody typed is nowhere on their screen.
    redraw(terminal, session)?;

    // Before the worker starts, because a sign-in needs the terminal and this is the thread that
    // has it. Left to the worker, the URL and code the AWS CLI prints would land in a frame this
    // loop redraws over.
    sign_in_if_needed(terminal, session, config)?;

    // One channel for everything the worker sends, because the main thread waits on exactly one
    // thing and `mpsc` cannot select across two. Only a write expects a reply.
    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();
    let (answer_tx, answer_rx) = mpsc::channel::<crate::remote_confirm::Reply>();
    // Prompts typed while this turn runs, going the other way. Shared rather than sent, so that
    // taking one back off the queue takes it out of the turn's reach too.
    let typed = session.interjections();

    // A fresh token per turn: reusing one could cancel a turn before it started.
    let cancel = Cancel::new();
    let worker_cancel = cancel.clone();

    // Cloned rather than borrowed so the worker owns everything it needs. Config and Workspace
    // are cheap handles; Egress builds its own connection pool.
    let worker_config = config.clone();
    let worker_workspace = workspace.clone();
    // No round limit: the person reading this screen is the bound, and a stop reaches a turn
    // mid-round. A number here would only interrupt work that was going fine.
    // Which tick of a loop this is, where it is one at all. A prompt the person typed in the
    // middle of a loop is not a tick of it and carries nothing.
    let tick = session.looping().and_then(|running| running.tick());
    // And what the session is working towards, where a person set a condition. Every turn under a
    // goal carries it, the first one included: the round that sets the direction is the one that
    // most needs to know what it is aiming at.
    let working_towards = session.goal().map(|goal| goal.condition().to_string());
    // And whether this turn may arm a standing watch. Read here for the reason the mode is read
    // here: only the session can count what is live, and a tool answering out of a stale count
    // would tell the planner about a watch the session then refused.
    let arming = session.arming();
    // Read once, here, so the mode the planner is told about and the mode the confirmer enforces are
    // the same one: the person may press the key while this turn runs, and the two halves reading it
    // at different moments is how they would come to disagree.
    let permission_mode = session.permission_mode();
    let mut task = Task::new(prompt)
        .with_rounds(None)
        .with_home(bravebot_agent::home::directory())
        // There is somebody in front of this, so a run prompt here may offer the key whose answer
        // outlives the session. A one-shot run says nothing here and reads no record.
        .remembering(Some(session_id.to_string()))
        .with_model(session.model().map(str::to_string))
        .with_effort(session.effort_in_force())
        .with_permissions(permissions.clone())
        .with_permission_mode(permission_mode)
        .ticking(tick)
        .arming(arming)
        .working_towards(working_towards);
    // Every file named with `@` becomes context, which a turn treats as trusted: the user typed the
    // path and their keystroke is what vouches for it, exactly as `--file` does on the command
    // line. Read back out of the prompt rather than tracked while it is typed, so the line that was
    // sent and the files that came with it cannot disagree.
    for file in files_named_in(prompt, wrote) {
        task = task.with_file(file);
    }
    // Dropped files, read back out of the line the same way and for the same reason: a marker the
    // user deleted is an attachment they took off.
    for attached in session.sent_attachments().to_vec() {
        task = match attached.kind {
            crate::dropped::Kind::Attachment(media) => task.with_attachment(attached.name, media),
            // A text file is context, which is what `@` and `--file` already do with one. It
            // goes in as a drop rather than as a named file because a drop comes from wherever
            // the user dragged it from, and that is rarely inside the workspace.
            crate::dropped::Kind::Text => task.with_dropped_text(attached.name),
        };
    }
    // Pasted pictures, in the order the markers in the prompt number them. A model reading
    // "[Image #2]" has to be able to count to the picture that answers it.
    for image in session.sent_pasted() {
        task = task.with_image(PastedImage {
            media_type: image.media_type,
            bytes: image.bytes.clone(),
        });
    }
    let task = task;
    // Kept so a failed turn does not lose the user's decisions. Both of them: a run approved
    // "always" in a turn that then failed is still an answer the user gave.
    let fallback = trust.clone();
    let fallback_programs = programs.clone();
    let fallback_asked = asked_about.clone();

    let worker = thread::spawn(move || {
        let mut sink = Trail::new();
        // Two handles over one channel back to the thread that owns the terminal: one asks about
        // writes and waits, the other reports progress and moves on.
        let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main.clone());
        let mut asking = crate::remote_confirm::RemoteConfirmer::new(to_main, answer_rx, typed);
        // Wrapped rather than replaced, because two of the confirmer's questions still cross back to
        // the terminal: a question the planner posed asks for information rather than consent, and an
        // interjection is the person typing unprompted.
        //
        // The mode as it was when the prompt was sent. A turn keeps the one it began with: a mode
        // changed while it runs describes the next turn, and a write already being reviewed must not
        // have the question withdrawn from under the person answering it.
        let mut confirmer = bravebot_agent::Confining::new(&mut asking, permission_mode);
        let egress = Egress::new();
        // Owned by the worker for the duration and handed back afterwards, whether the turn
        // succeeded or not. A failed turn is still part of the conversation, and the next one
        // is usually about it.
        let mut conversation = conversation;
        // Built on the first turn of the session and handed back with the conversation. Nothing is
        // started by building it: LSP-8 starts a server on the first question that needs one, so a
        // session that asks about no symbol still starts no process and prompts about none.
        let mut servers = servers.unwrap_or_else(|| {
            // The task's home rather than a second look at the state directory, so an index is
            // cached where the rest of this session's state goes.
            LanguageServers::new(worker_workspace.root().to_path_buf(), task.home.clone())
        });
        let outcome = turn::resume(
            &worker_config,
            &egress,
            &worker_workspace,
            &task,
            &mut conversation,
            &mut confirmer,
            &mut reporter,
            &mut sink,
            trust,
            programs,
            Some(&mut servers),
            &worker_cancel,
        );
        (outcome, conversation, sink, Some(servers))
    });

    // Redraw until the turn finishes, answering approvals and watching for a cancel on the way.
    loop {
        redraw(terminal, session)?;

        // Input is polled here rather than in the outer loop, which is blocked for the duration
        // of the turn. Without this the interface would take none at all while working, and a
        // long turn is exactly when someone wants to copy what has appeared so far.
        //
        // This is also where the frame's waiting is done, and that is the point of it. Blocking on
        // the worker instead left a key press sitting for up to a frame before anything looked at
        // it, so typing during a turn lagged while typing between turns did not, for no reason a
        // person could see. A keystroke wakes this the instant it arrives; the worker's messages
        // are picked up on the way round and drawn at the frame rate, which is all they need.
        //
        // Everything waiting, not one event per pass: a drag read one event at a time would take
        // seconds to catch up with the pointer.
        if event::poll(FRAME)? {
            while event::poll(Duration::ZERO)? {
                match event::read()? {
                    // Presses only, for the reason the outer loop ignores releases: a release taken
                    // for a press would type every character twice, and cancel the turn on the way up
                    // from the Escape that already cancelled it.
                    TermEvent::Key(key) if key.kind == KeyEventKind::Release => {}
                    // Both keys stop the turn and neither leaves. Ctrl-C is the way out of the
                    // program, but there is a turn to stop first, and a person watching an answer
                    // go wrong is asking for the answer to stop rather than for the session to
                    // end. The next press, at the box, is the one that leaves.
                    //
                    // Nothing is said about stopping. The stop is the prompt coming back to the
                    // box a moment later, which is both the answer and what the person wanted;
                    // a line saying "cancelling…" is a progress report on a key press.
                    TermEvent::Key(key) if stops_the_turn(session, key) => {
                        cancel.cancel();
                    }
                    TermEvent::Key(key) => {
                        let action = handle_key_while_working(session, key);
                        act_while_working(session, action, crate::clipboard::paste);
                    }
                    TermEvent::Paste(text) => {
                        let action = handle_paste_while_working(session, &text);
                        act_while_working(session, action, crate::clipboard::paste);
                    }
                    TermEvent::Mouse(mouse) => {
                        // Bound rather than tested inline, because handling the event scrolls and
                        // moves the selection whatever it returns. A match guard would hide that.
                        let action = handle_mouse(session, mouse);
                        if action == Action::Copy {
                            copy_selection(terminal, session)?;
                        }
                    }
                    _ => {}
                }
            }
        }

        let carrying_on = drain_worker(&from_worker, Duration::ZERO, |message| match message {
            crate::remote_confirm::ToMain::Write(request) => {
                let answer = crate::confirm::ask(terminal, &request);
                // Ctrl-C at the prompt is the same request it is anywhere else in a turn: stop.
                // Set before the answer goes back, so the worker sees it as soon as it wakes.
                if answer.stops_the_turn() {
                    cancel.cancel();
                }
                // A closed channel means the worker is already gone, so there is nothing to
                // answer and the loop below will collect its result.
                let _ = answer_tx.send(crate::remote_confirm::Reply::Write(answer.decision()));
            }
            crate::remote_confirm::ToMain::Run(request) => {
                let answer = crate::confirm::ask_run(terminal, &request);
                // Ctrl-C at the prompt is the same request it is anywhere else in a turn: stop.
                // Set before the answer goes back, so the worker sees it as soon as it wakes.
                if answer.stops_the_turn() {
                    cancel.cancel();
                }
                // What was vouched for travels back with the turn's outcome, exactly as the
                // trust map does: the tool records it on the policy, and the policy carries it
                // out. Recording it here as well would give the session a second copy to
                // disagree with.
                let _ = answer_tx.send(crate::remote_confirm::Reply::Run(answer.decision()));
            }
            crate::remote_confirm::ToMain::ReadOutput(request) => {
                let answer = crate::confirm::ask_output(terminal, &request);
                if answer.stops_the_turn() {
                    cancel.cancel();
                }
                let _ = answer_tx.send(crate::remote_confirm::Reply::ReadOutput(answer.decision()));
            }
            crate::remote_confirm::ToMain::Vet(request) => {
                let answer = crate::confirm::ask_vet(terminal, &request);
                if answer.stops_the_turn() {
                    cancel.cancel();
                }
                // Nothing is noted on the transcript: an approval covers the bytes that were on
                // the screen and leaves no rule behind, so there is no standing decision to
                // record.
                let _ = answer_tx.send(crate::remote_confirm::Reply::Vet(answer.decision()));
            }
            crate::remote_confirm::ToMain::Fetch(request) => {
                let answer = crate::confirm::ask_fetch(terminal, &request);
                if answer.stops_the_turn() {
                    cancel.cancel();
                }
                // Nothing is noted on the transcript: an approval covers this one URL and leaves
                // no standing permission behind, so there is no decision to record.
                let _ = answer_tx.send(crate::remote_confirm::Reply::Fetch(answer.decision()));
            }
            crate::remote_confirm::ToMain::Vouch(request) => {
                let answer = crate::confirm::ask_vouch(terminal, &request);
                if answer.stops_the_turn() {
                    cancel.cancel();
                }
                if answer == crate::confirm::Answer::Approve {
                    // Said on the transcript because it is a standing decision the user will not
                    // otherwise see recorded anywhere until they ask for /status.
                    session.note(t!(session_vouched_for, path = &request.path));
                }
                let _ = answer_tx.send(crate::remote_confirm::Reply::Vouch(answer.decision()));
            }
            crate::remote_confirm::ToMain::Server(request) => {
                let answer = crate::confirm::ask_server(terminal, &request);
                if answer.stops_the_turn() {
                    cancel.cancel();
                }
                if answer == crate::confirm::Answer::Approve {
                    // Said on the transcript for the reason vouching for a file is: it lasts the
                    // session, and a process running with the user's access is worth being able to
                    // see they agreed to.
                    session.note(t!(
                        session_started_server,
                        language = request.language,
                        program = request.program.as_str()
                    ));
                }
                let _ = answer_tx.send(crate::remote_confirm::Reply::Server(answer.decision()));
            }
            crate::remote_confirm::ToMain::Manifest(request) => {
                let answer = crate::confirm::ask_manifest(terminal, &request);
                if answer.stops_the_turn() {
                    cancel.cancel();
                }
                // Nothing is noted on the transcript. The answer covers this plan and no other, so
                // there is no standing decision to record, and the plan itself is about to be
                // walked in the open where the transcript will show every step of it.
                let _ = answer_tx.send(crate::remote_confirm::Reply::Manifest(answer.decision()));
            }
            crate::remote_confirm::ToMain::Ask(asking) => {
                // A planner that loops back over the same decision should not make the user
                // restate it. The note is what keeps that from being invisible: an answer given
                // once and reused silently would look like a question that was never asked.
                let known: Vec<Option<bravebot_core::ask::Answer>> = asking
                    .prompts
                    .iter()
                    .map(|prompt| session.recall_answer(&prompt.key))
                    .collect();
                for (prompt, earlier) in asking.prompts.iter().zip(&known) {
                    if earlier.is_some() {
                        session.note(t!(session_answered_already, question = &prompt.question));
                    }
                }

                // Only what is still outstanding is drawn, so the count in the title is the
                // number of questions the person actually has to answer.
                let outstanding = bravebot_core::ask::Asking {
                    prompts: asking
                        .prompts
                        .iter()
                        .zip(&known)
                        .filter(|(_, earlier)| earlier.is_none())
                        .map(|(prompt, _)| prompt.clone())
                        .collect(),
                };
                let fresh = crate::ask::ask(terminal, &outstanding);

                let answers = crate::ask::in_order(known, fresh);
                for (prompt, answer) in asking.prompts.iter().zip(&answers) {
                    session.remember_answer(prompt.key.clone(), answer.clone());
                }
                let _ = answer_tx.send(crate::remote_confirm::Reply::Ask(answers));
            }
            // No reply: each of these is recorded and the next redraw, one iteration away,
            // shows it. That is what makes a long turn legible while it runs.
            crate::remote_confirm::ToMain::Todos(rows) => session.set_todos(rows),
            crate::remote_confirm::ToMain::Written(written) => session.set_written(written),
            crate::remote_confirm::ToMain::Phase(phase) => session.set_phase(phase),
            crate::remote_confirm::ToMain::Narration(text) => session.narrate(text),
            crate::remote_confirm::ToMain::Notice(text) => session.note_once(text),
            crate::remote_confirm::ToMain::Streaming(text) => session.streaming(&text),
            crate::remote_confirm::ToMain::Started(activity) => session.start_activity(activity),
            crate::remote_confirm::ToMain::Finished(activity) => session.finish_activity(activity),
            crate::remote_confirm::ToMain::Quarantined(shown) => session.show(shown),
            crate::remote_confirm::ToMain::Printed(output) => session.command_printed(output),
            crate::remote_confirm::ToMain::Landed(landing) => session.landed(landing),
            // The turn has taken the oldest waiting prompt, so it stops being something waiting
            // above the box and becomes something said. Which prompt is not named: the turn takes
            // them in the order they were sent and this end hands them over in that order, so the
            // oldest is the one that has gone.
            crate::remote_confirm::ToMain::Interjected(_) => session.interjected(),
            // Whose work the lines that follow are, as the driver said. Nothing here reads a line
            // to find out: several delegates and the turn report at once.
            crate::remote_confirm::ToMain::ReportingFor(delegate) => {
                session.reporting_for(delegate)
            }
            crate::remote_confirm::ToMain::DelegateStarted(delegation) => {
                session.delegate_started(delegation)
            }
            crate::remote_confirm::ToMain::DelegateFinished {
                id,
                note,
                failed,
                reported,
            } => session.delegate_finished(id, note, failed, reported),
        });

        // The worker dropped its senders, so the turn is over.
        if !carrying_on {
            break;
        }
    }

    let (outcome, conversation, sink, servers) = worker.join().unwrap_or_else(|_| {
        // A panicked turn is reported rather than propagated: the session survives. The
        // conversation does not, since the thread that held it is gone, and neither do the
        // servers: they went down with the thread that owned them, so the next turn starts and
        // is asked about a fresh one.
        (
            Err(turn::TurnError::Precommit(
                t!(turn_ended_unexpectedly).to_string(),
            )),
            Conversation::new(),
            Trail::new(),
            None,
        )
    });

    // Record cancellation separately from failure, then restore the prompt when possible.
    let events = sink.events().to_vec();

    if let Err(turn::TurnError::Cancelled { attempts }) = &outcome {
        session.stopped(*attempts);
        // Not when the cancel was somebody leaving. Restoring returns the session to idle, which
        // would put it back in the loop it was on its way out of, and hand back a prompt to a box
        // nobody is going to see.
        if session.is_quitting() {
            return Ok(Continued {
                conversation,
                trust: fallback,
                programs: fallback_programs,
                servers,
                asked_about: fallback_asked,
                events,
            });
        }
        session.restore(prompt);
        // What was lined up behind it stays lined up, and the loop sends the next one as it does
        // after any turn. A stop is aimed at the turn in flight: the prompts behind it are ones
        // the person typed and has not taken back, and throwing them away made stopping a turn
        // that had gone wrong cost every prompt they had queued while it did.
        return Ok(Continued {
            conversation,
            trust: fallback,
            programs: fallback_programs,
            servers,
            asked_about: fallback_asked,
            events,
        });
    }

    // Whether a difference between the model asked for and the one reported means anything is the
    // backend's question, and so is which name the service was actually asked for: a gateway is
    // asked for the part of a qualified name that it knows the model by. Answered here, where the
    // configuration is in hand.
    let chosen = session.model().unwrap_or(&config.default_model);
    let asked = Asked {
        name: bravebot_agent::backend::Backend::name_as_asked(config, chosen),
        comparable: bravebot_agent::backend::Backend::reports_the_model_it_was_asked_for(
            config, chosen,
        ),
    };
    let carried = fold_outcome(
        session,
        outcome,
        sink,
        Carried {
            trust: fallback,
            programs: fallback_programs,
            asked: fallback_asked,
        },
        Occupied {
            budget: config.context_budget,
            guessed: config.budget_is_guessed(),
            last_request_tokens: conversation.last_request_tokens(),
        },
        asked,
        Line {
            text: prompt,
            wrote,
        },
        workspace,
    );
    Ok(Continued {
        conversation,
        trust: carried.trust,
        programs: carried.programs,
        servers,
        asked_about: carried.asked,
        events,
    })
}

/// Hand the worker's messages to `handle`: the one this frame waited for, and then everything
/// already queued behind it.
///
/// `wait` is how long to block for the first, and a caller that is already blocking on something
/// else passes nothing. Which one a loop waits on decides how quickly it answers a key press, so
/// it belongs to the caller rather than here.
///
/// `false` when the worker has dropped its senders and the turn is over.
///
/// Drained rather than taken one per pass, for the same reason terminal events are. A reply
/// arrives as hundreds of small messages, a draw rebuilds the whole transcript from its markdown,
/// and a frame for each spent longer laying out finished turns than the reply took to arrive: the
/// queue outran the drawing and what was on the screen fell behind what had been said.
fn drain_worker(
    from_worker: &mpsc::Receiver<crate::remote_confirm::ToMain>,
    wait: Duration,
    mut handle: impl FnMut(crate::remote_confirm::ToMain),
) -> bool {
    let mut received = from_worker.recv_timeout(wait);
    loop {
        match received {
            Ok(message) => handle(message),
            Err(mpsc::RecvTimeoutError::Timeout) => return true,
            Err(mpsc::RecvTimeoutError::Disconnected) => return false,
        }

        // Whatever else is waiting, without waiting for it. Empty means the burst is over and the
        // next frame shows all of it at once.
        received = from_worker.try_recv().map_err(|gone| match gone {
            mpsc::TryRecvError::Empty => mpsc::RecvTimeoutError::Timeout,
            mpsc::TryRecvError::Disconnected => mpsc::RecvTimeoutError::Disconnected,
        });
    }
}

/// Whether a key press asks for whatever is in flight to stop, and nothing more.
///
/// Escape, and only Escape. Ctrl-C asks for it too, but Ctrl-C also leaves, so the loops take it
/// separately: which of the two it means depends on whether there is anything to stop.
fn wants_cancel(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc)
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
fn is_ctrl_c(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c'))
}

/// Whose line a turn is running.
///
/// A `@path` in a prompt is a person vouching for a file with their keystroke, so it is read out
/// of a line they typed and out of no other. Every prompt was one until a goal could write the
/// sentence that carries the work on: that sentence is this program's, quoting a judge, and an
/// `@` inside it names nothing and vouches for nobody.
///
/// A loop tick is the person's, because the line a tick sends is the one they typed into `/loop`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wrote {
    /// The person, at the box, or a loop repeating the line they typed there.
    ThePerson,
    /// This program, carrying the work on towards a condition the person set.
    TheDriver,
}

/// The wait that starts a loop nobody typed `/loop` for, where there is one.
///
/// `None` where the turn asked for nothing, and where the line the loop would repeat is not the
/// person's. A loop repeats a line somebody endorsed, and the sentence this program writes to
/// carry a goal on is not one: a turn under a goal asking for a later look would otherwise leave
/// behind a loop sending the driver's own words back every quarter of an hour.
///
/// Split out from `fold_outcome` so it can be tested. That function's answer to a finished turn
/// needs a whole [`turn::Outcome`], and the field holding the released reply is its own crate's.
fn watch_to_start(wakeup: Option<turn::Wakeup>, wrote: Wrote) -> Option<turn::Wakeup> {
    wakeup.filter(|_| wrote == Wrote::ThePerson)
}

/// The files a prompt vouches for by naming them with `@`.
///
/// Out of a line the person wrote, and out of no other. The keystroke is the whole of what vouches
/// for the path, so a sentence this program wrote has nothing to vouch with: an `@` a judge
/// happened to put in a reason would otherwise open a file on its own say-so, wearing an
/// endorsement nobody gave.
fn files_named_in(prompt: &str, wrote: Wrote) -> Vec<String> {
    match wrote {
        Wrote::ThePerson => crate::entries::referenced(prompt),
        Wrote::TheDriver => Vec::new(),
    }
}

/// What the last turn asked its backend for, for comparing against what answered.
///
/// The two travel together because either alone is misleading: a name means nothing without knowing
/// whether it is comparable to a reply at all.
struct Asked {
    /// The name the service was actually sent, which for a gateway is not the name a session holds.
    name: String,
    /// Whether that name and the one a reply reports are drawn from one roster.
    comparable: bool,
}

/// What a finished turn is measured against, and what to fall back on where it reported nothing.
///
/// One value rather than three arguments because none of them says anything alone: a figure without
/// the budget it is against is not a percentage, and a budget without knowing whether it was
/// advertised or assumed is not one the hint line may state flatly.
struct Occupied {
    /// The budget the conversation is compacted at.
    budget: u64,
    /// Whether that budget is a guess rather than one somebody typed or the endpoint advertised.
    guessed: bool,
    /// What the last request the turn managed to send came to, or zero where it sent none.
    last_request_tokens: u64,
}

/// The line a turn ran, and whose it was.
///
/// One value rather than two because neither says anything alone here: a loop repeats a line, and
/// whether this one may be repeated is a question about who wrote it rather than about the words.
#[derive(Clone, Copy)]
struct Line<'a> {
    text: &'a str,
    wrote: Wrote,
}

/// What a turn hands to the next one: paths the person vouched for, programs they allowed, and the
/// run prompts they have read.
///
/// One value rather than three because they travel together in both directions, and a turn that
/// reported none of them hands on exactly what it was given. The third is not a grant and sits here
/// only because it has the same lifetime as the other two.
struct Carried {
    trust: TrustStore,
    programs: TrustedPrograms,
    asked: AskedAbout,
}

/// Fold a finished turn into the session.
#[allow(clippy::too_many_arguments)]
fn fold_outcome(
    session: &mut Session,
    outcome: Result<turn::Outcome, turn::TurnError>,
    sink: Trail,
    fallback: Carried,
    occupied: Occupied,
    asked: Asked,
    line: Line<'_>,
    // Lent for the one thing a finished turn needs it for: the first look at a path the turn
    // asked to have watched, which has to come from the same place every later look will.
    workspace: &Workspace,
) -> Carried {
    let carried = match outcome {
        Ok(outcome) => {
            let trail = sink.lines();
            session.complete(
                outcome.reply_for_display().to_string(),
                trail,
                outcome.tokens,
            );
            if !outcome.clean {
                session.note(t!(session_something_was_refused));
            }
            // Where the wall clock went, beside what the turn cost. The wall figure is the
            // session's own, taken from the moment Enter was pressed; this fills in the parts, which
            // only the worker saw.
            session.spent_time(outcome.timing);

            // How much of what the turn sent the backend recognised, which the two figures above
            // cannot say: a turn costs the same tokens whether they were read or recognised, and
            // about ten times the money.
            session.served_from_cache(outcome.cached);

            // What the turn's last request came to, against what it would be compacted at. Not
            // the same figure as the cost above: that adds every round together, this says how
            // full the context is now.
            session.measured(outcome.context_tokens, occupied.budget, occupied.guessed);

            // What was asked for against what answered. The endpoint substitutes rather than
            // refusing: a premium model requested without a credential comes back as whatever the
            // free tier serves, with a 200 and a perfectly ordinary reply. So the only trace is
            // this field, and a session that never compares them cannot tell a model it chose from
            // one chosen for it.
            //
            // Said only when they differ, and only when the difference is new, since it would
            // otherwise be a line on every turn for the rest of the session.
            let already = session.substituted_model().is_some();
            session.served(
                session.model().map(|_| asked.name),
                outcome.model.clone(),
                outcome.premium,
                asked.comparable,
            );
            if !already && let Some(asked) = session.substituted_model() {
                session.note(t!(
                    session_model_substituted,
                    asked = asked,
                    served = &outcome.model
                ));
            }
            // Where the turn was a tick, this is what arms the next one: an interval from the
            // driver's own clock, or the wait the turn asked for. Measured from here rather than
            // from when the tick went out, so the gap is between runs and a turn that outlasts
            // its own interval is not immediately due again.
            //
            // Where no loop is running, a wait the turn asked for starts one, which is how a turn
            // asked to watch something gets the later look it needs. The line it repeats is this
            // turn's own, and only where the person wrote it: a loop repeats a line somebody
            // endorsed, and a sentence this program wrote carrying a goal on is not one.
            // The watches this turn asked for, in the order it asked, and before the wait below:
            // a session does one of a watch, a loop and a goal at a time, and a turn that asked
            // for both meant the watch. Each has been through the gate a read of that path goes
            // through; what is left is the session's own question, which is whether it has room
            // and whether the first look sees anything.
            for path in &outcome.watches {
                session.arm_watch(path, workspace.look(path));
            }

            if session.looping().is_some() {
                session.loop_turn_ended(outcome.wakeup);
            } else if let Some(wakeup) = watch_to_start(outcome.wakeup, line.wrote) {
                session.watch_again(line.text, wakeup);
            }

            // Carries forward any rule the turn recorded, so a path that received untrusted
            // data cannot be read back as trusted by the next turn, and any program the user
            // vouched for during it, so they are not asked about it again.
            Carried {
                trust: outcome.trust,
                programs: outcome.programs,
                asked: outcome.asked_about,
            }
        }
        Err(error) => {
            // Stopping a turn stops the loop it was part of, and stops one the person was not
            // part way through: the key means "stop what is happening", and a schedule that
            // survived it would send the next prompt as though nothing had been said.
            if matches!(error, turn::TurnError::Cancelled { .. }) {
                session.stop_loop();
                // And the watch whose fire this turn was, since stopping a fire's turn is the
                // most exact way anybody has to say which watch they are finished with: they are
                // reading its prompt when they press the key. A turn that was not a fire ends no
                // watch, because that press is a person steering their own work.
                session.stop_firing_watch();
                // Not the goal, which outlives one turn by design. A stopped turn is recorded as
                // failed, so it is not judged and the work is not sent straight back, and the
                // person who stops a turn going the wrong way keeps the condition they set: the
                // key means "stop this", and a second press with nothing running takes it off.
            } else {
                // Any other failure is a tick that ended, and a self-paced loop that was told
                // nothing falls back to the driver's own wait. A loop must not end because one
                // request failed, and must not run forever on failures either.
                session.loop_turn_ended(None);
            }

            // The trail is kept on failure too: a refusal is exactly when a user wants
            // to see what happened.
            let trail = sink.lines();
            // Format safe fields instead of an error that may contain endpoint credentials.
            let ending = error.ending();
            match ending {
                bravebot_agent::Ending::Failed(diagnosis) => {
                    session.fail(crate::state::failure_reason(diagnosis), ending);
                }
                bravebot_agent::Ending::Stopped { attempts } => session.stopped(attempts),
                bravebot_agent::Ending::Done => {}
            }
            // The panel reports the last turn's cache split, and this turn is now the last one. It
            // measured nothing, so leaving the turn before it on the panel would report a figure
            // against an exchange that never finished.
            session.restore_cache(None);
            if let Some(last) = session.transcript.last_mut() {
                last.trail = trail;
            }
            if occupied.last_request_tokens > 0 {
                session.measured(
                    occupied.last_request_tokens,
                    occupied.budget,
                    occupied.guessed,
                );
            }
            fallback
        }
    };

    // A fire is a whole turn, so the gap to the next fire of that watch is measured from here:
    // the turn's own length is what spaces fires out, exactly as it is for a tick. Outside the
    // two arms because a turn that failed still ended, and a watch left marked as firing would
    // never fire again and would make every prompt after it read as the driver's.
    session.watch_turn_ended();
    carried
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Status;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// The private modes a run of bytes leaves turned on, in the order the terminal would read
    /// them: a mode asked for and then given up again is not left on.
    fn modes_left_on(written: &str) -> std::collections::BTreeSet<String> {
        let mut on = std::collections::BTreeSet::new();
        for request in written.split("\x1b[?").skip(1) {
            let mode: String = request.chars().take_while(char::is_ascii_digit).collect();
            match request[mode.len()..].chars().next() {
                Some('h') => {
                    on.insert(mode);
                }
                Some('l') => {
                    on.remove(&mode);
                }
                _ => {}
            }
        }
        on
    }

    fn asked_for(enhanced: bool) -> String {
        let mut written = Vec::new();
        ask_for_modes(&mut written, enhanced).expect("ask");
        String::from_utf8(written).expect("utf8")
    }

    fn given_back(enhanced: bool) -> String {
        let mut written = Vec::new();
        give_back_modes(&mut written, enhanced).expect("give back");
        String::from_utf8(written).expect("utf8")
    }

    /// A session draws on a screen of its own, so what was in the terminal before it started is
    /// still there afterwards. Nothing else in this file can be checked against a terminal, and a
    /// takeover that stopped happening would look like a working program.
    #[test]
    fn a_session_draws_on_a_screen_of_its_own() {
        // 1049 is the alternate screen. Named as a number because that is what is sent.
        assert!(modes_left_on(&asked_for(false)).contains("1049"));
        assert!(!modes_left_on(&given_back(false)).contains("1049"));
    }

    /// Every mode is given back, whether the session ended by being left or by failing. A mode
    /// kept past the last frame is a terminal the person has to repair by hand: mouse reporting
    /// left on turns every click into unreadable bytes, and bracketed paste left on prints its
    /// markers into whatever they type next.
    #[test]
    fn every_mode_a_session_asks_for_is_given_back() {
        for enhanced in [false, true] {
            let asked = modes_left_on(&asked_for(enhanced));
            let handed_back = modes_left_on(&given_back(enhanced));
            assert!(!asked.is_empty(), "nothing was asked for");
            for mode in &asked {
                assert!(
                    !handed_back.contains(mode),
                    "mode {mode} is left on: asked {asked:?}, handed back {handed_back:?}"
                );
            }
        }
    }

    /// Disambiguated keys are pushed onto a stack the terminal keeps, so the one thing owed is a
    /// pop: a push left on the stack outlives the process and changes what every later program
    /// reads from the keyboard.
    #[test]
    fn a_pushed_keyboard_mode_is_popped_and_an_unpushed_one_is_not() {
        assert!(asked_for(true).contains("\x1b[>"));
        assert!(given_back(true).contains("\x1b[<"));

        assert!(!asked_for(false).contains("\x1b[>"));
        assert!(!given_back(false).contains("\x1b[<"));
    }

    /// The wheel scrolls the transcript, which is what mouse reporting is asked for, and a pointer
    /// merely crossing the window is not reported: that arrives as an event and a redraw per pixel
    /// of travel, for a gesture nothing here reads.
    #[test]
    fn the_session_reads_a_drag_and_not_every_pointer_movement() {
        let on = modes_left_on(&asked_for(false));
        assert!(on.contains("1000"), "buttons are not reported: {on:?}");
        assert!(on.contains("1002"), "a drag is not reported: {on:?}");
        assert!(!on.contains("1003"), "all motion is reported: {on:?}");
    }

    fn listed(key: &str, window: Option<u64>) -> bravebot_aichat::models::Model {
        bravebot_aichat::models::Model {
            key: key.to_string(),
            display_name: key.to_string(),
            premium: false,
            provider: None,
            conversation_tokens: window,
            reads_effort: true,
        }
    }

    fn bedrock_configured(pairs: &[(&str, &str)]) -> bravebot_config::bedrock::Bedrock {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        bravebot_config::bedrock::Bedrock::from_lookup(|name| {
            owned
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        })
        .expect("configured")
    }

    /// The picker offers what was configured, under the tier names a person recognises rather than
    /// the ARNs they were given as. An ARN is unreadable and identical-looking between tiers.
    #[test]
    fn the_bedrock_picker_offers_the_configured_tiers_by_name() {
        use bravebot_config::env_var;

        let models = bedrock_models(&bedrock_configured(&[
            (env_var::USE_BEDROCK, "1"),
            (env_var::AWS_REGION, "us-west-2"),
            (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
            (env_var::BEDROCK_SONNET_MODEL, "sonnet-arn"),
            (env_var::BEDROCK_HAIKU_MODEL, "haiku-arn"),
        ]));

        let shown: Vec<&str> = models.iter().map(|m| m.display_name.as_str()).collect();
        assert_eq!(shown, ["Opus", "Sonnet", "Haiku"]);
        // The key is what lands in the request's model field, and it has to be the ARN.
        assert_eq!(models[0].key, "opus-arn");
    }

    /// Naming the service does not distinguish the row: Brave serves some of its own roster through
    /// Bedrock and says so in the display name it sends, so "(Bedrock)" appeared on both halves of the
    /// picker and told a person nothing about which account was about to be billed.
    #[test]
    fn a_configured_tier_is_not_confusable_with_a_brave_model_served_through_bedrock() {
        use bravebot_config::env_var;

        let mut brave = listed("gpt-5.5", Some(102_400));
        brave.display_name = "GPT-5.5 (Bedrock)".to_string();

        let roster = combined(
            bedrock_models(&bedrock_configured(&[
                (env_var::USE_BEDROCK, "1"),
                (env_var::AWS_REGION, "us-west-2"),
                (env_var::AWS_PROFILE, "some-profile"),
                (env_var::BEDROCK_SONNET_MODEL, "sonnet-arn"),
            ])),
            Ok(vec![brave]),
        )
        .expect("a roster");

        assert_eq!(roster.len(), 2);
        // The profile is what decides which credentials sign, and no name off the wire carries it.
        let configured = roster[0].provider.as_deref().expect("a service");
        assert!(configured.contains("some-profile"), "{configured}");
        assert_eq!(
            roster[1].provider, None,
            "the Brave roster names no service"
        );
    }

    /// A model a `provider` block named has no tier, so there is no tier word to draw a row from.
    /// Drawn from one anyway, every such model would appear as "Opus", and an account reaching four
    /// of them would show four rows with three names between them.
    #[test]
    fn a_bedrock_model_a_block_named_is_shown_under_that_name() {
        use bravebot_config::bedrock::{Bedrock, Entry};

        let models = bedrock_models(&Bedrock::from_provider(
            "us-west-2".to_string(),
            Some("sso".to_string()),
            vec![
                Entry {
                    tier: None,
                    id: "arn:aws:bedrock:us-west-2:1:application-inference-profile/abc".to_string(),
                    name: Some("GPT-5.6 Sol (Bedrock)".to_string()),
                    context_window: Some(1_050_000),
                },
                Entry {
                    tier: None,
                    id: "openai.gpt-5.6-sol".to_string(),
                    name: None,
                    context_window: None,
                },
            ],
        ));

        assert_eq!(models[0].display_name, "GPT-5.6 Sol (Bedrock)");
        assert_eq!(
            models[0].key, "arn:aws:bedrock:us-west-2:1:application-inference-profile/abc",
            "the row carries the ARN a request names, not the word a person reads"
        );
        assert_eq!(
            models[0].conversation_tokens,
            Some(1_050_000),
            "a window the block stated was thrown away"
        );

        // Nothing friendlier was said about the second, so its id stands.
        assert_eq!(models[1].display_name, "openai.gpt-5.6-sol");
        assert_eq!(
            models[1].conversation_tokens,
            Some(bravebot_config::bedrock::CONTEXT_WINDOW)
        );

        // Both are reached through the person's own account, which a row has to say.
        assert!(models.iter().all(|model| model.provider.is_some()));
    }

    /// A profile is optional, and a row still has to say the tier is reached through the person's own
    /// account rather than through Brave's.
    #[test]
    fn a_tier_with_no_profile_configured_still_names_the_account() {
        use bravebot_config::env_var;

        let models = bedrock_models(&bedrock_configured(&[
            (env_var::USE_BEDROCK, "1"),
            (env_var::AWS_REGION, "us-west-2"),
            (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
        ]));
        assert_eq!(models[0].display_name, "Opus");
        assert_eq!(
            models[0].provider.as_deref(),
            Some(t!(picker_service_bedrock))
        );
    }

    /// The same condition Claude Code applies: a tier appears only when its variable names a model.
    /// An entry invented for an unset tier is a choice that fails at the far end.
    #[test]
    fn a_tier_with_no_model_configured_is_not_offered() {
        use bravebot_config::env_var;

        let models = bedrock_models(&bedrock_configured(&[
            (env_var::USE_BEDROCK, "1"),
            (env_var::AWS_REGION, "us-west-2"),
            (env_var::BEDROCK_SONNET_MODEL, "sonnet-arn"),
        ]));

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].display_name, "Sonnet");
    }

    /// A settings block adds to the roster rather than replacing it. Replacing it left a person who
    /// configured one tier with a picker offering exactly one model, and no way back to the Brave
    /// models every build has.
    #[test]
    fn configured_tiers_are_offered_alongside_the_brave_roster() {
        use bravebot_config::env_var;

        let configured = bedrock_models(&bedrock_configured(&[
            (env_var::USE_BEDROCK, "1"),
            (env_var::AWS_REGION, "us-west-2"),
            (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
        ]));
        let roster = combined(
            configured,
            Ok(vec![
                bravebot_aichat::models::Model::automatic(),
                listed("claude-sonnet", Some(102_400)),
            ]),
        )
        .expect("a roster");

        let keys: Vec<&str> = roster.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(
            keys,
            ["opus-arn", bravebot_config::DEFAULT_MODEL, "claude-sonnet"]
        );
    }

    /// The tiers need no network to know. Losing them because the other half was unreachable would
    /// leave the only models this configuration can definitely reach unpickable.
    #[test]
    fn an_unreachable_listing_still_offers_the_configured_tiers() {
        use bravebot_config::env_var;

        let configured = bedrock_models(&bedrock_configured(&[
            (env_var::USE_BEDROCK, "1"),
            (env_var::AWS_REGION, "us-west-2"),
            (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
        ]));
        let roster = combined(configured, Err("the endpoint is unreachable".into()))
            .expect("the tiers survive");
        assert_eq!(roster.len(), 1);
        assert_eq!(roster[0].key, "opus-arn");
    }

    /// With nothing configured there is nothing to fall back to, and a picker showing an empty list
    /// would read as a backend with no models rather than as a listing that failed.
    #[test]
    fn an_unreachable_listing_with_no_tiers_configured_is_still_a_failure() {
        assert!(combined(vec![], Err("the endpoint is unreachable".into())).is_err());
    }

    /// `automatic` means "let the server choose", which Bedrock does not offer: a request names one
    /// model and gets it or an error. It reaches the picker from the Brave half instead.
    #[test]
    fn the_bedrock_picker_does_not_offer_automatic() {
        use bravebot_config::env_var;

        let models = bedrock_models(&bedrock_configured(&[
            (env_var::USE_BEDROCK, "1"),
            (env_var::AWS_REGION, "us-west-2"),
            (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
        ]));
        assert!(!models.iter().any(|model| model.is_automatic()));
    }

    /// Premium means a Leo subscription. Reaching a model through somebody's own AWS account does
    /// not involve one, and marking it premium would ask them to import a subscription to use what
    /// they already pay for.
    #[test]
    fn bedrock_models_are_not_marked_premium() {
        use bravebot_config::env_var;

        let models = bedrock_models(&bedrock_configured(&[
            (env_var::USE_BEDROCK, "1"),
            (env_var::AWS_REGION, "us-west-2"),
            (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
        ]));
        assert!(!models[0].premium);
    }

    /// The budget lookup shares this listing, so a Bedrock entry has to carry a window or a session
    /// would fall back to the default and compact five times sooner than it had to.
    #[test]
    fn a_bedrock_entry_carries_the_window_the_budget_is_taken_from() {
        use bravebot_config::env_var;

        let models = bedrock_models(&bedrock_configured(&[
            (env_var::USE_BEDROCK, "1"),
            (env_var::AWS_REGION, "us-west-2"),
            (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
        ]));
        assert_eq!(
            advertised_window(&models, Some("opus-arn")),
            Some(bravebot_config::bedrock::CONTEXT_WINDOW)
        );
    }

    /// A gateway configured by a `provider` block, for the picker tests below.
    fn gateway(models: &str) -> bravebot_config::provider::Provider {
        let text = format!(
            r#"{{"provider": {{"openrouter": {{
                "options": {{"baseURL": "https://openrouter.example.invalid/api/v1"}},
                "models": {models}
            }}}}}}"#
        );
        let serde_json::Value::Object(root) = serde_json::from_str(&text).expect("json") else {
            panic!("not an object");
        };
        bravebot_config::provider::Provider::all(&root)
            .pop()
            .expect("one provider")
    }

    /// A gateway's roster is what the file named and nothing else. Asking the gateway would list
    /// hundreds of models across upstreams nobody configured, and would cost a round trip on a path
    /// that has to work offline.
    ///
    /// Each key is qualified by the provider's own id, which is what makes a remembered choice say
    /// which service it was for.
    #[test]
    fn only_the_gateway_models_the_file_named_are_offered() {
        let models = provider_models(&gateway(r#"{"z-ai/glm-4.6": {}, "moonshot/kimi-k2": {}}"#));
        let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(
            keys,
            ["openrouter/moonshot/kimi-k2", "openrouter/z-ai/glm-4.6"]
        );
    }

    /// The same model may be reachable more than one way, billed and credentialled differently, and
    /// nothing else on the row would say which was about to answer.
    #[test]
    fn a_gateway_row_says_which_service_answers_it() {
        let models = provider_models(&gateway(r#"{"anthropic/claude-sonnet-4.5": {}}"#));
        assert_eq!(models[0].display_name, "anthropic/claude-sonnet-4.5");
        assert_eq!(models[0].provider.as_deref(), Some("openrouter"));
    }

    /// Premium means a Leo subscription. A gateway reached with somebody's own bearer token does not
    /// involve one, and marking it premium would ask them to import a subscription to use what they
    /// already pay for.
    #[test]
    fn gateway_models_are_not_marked_premium() {
        let models = provider_models(&gateway(r#"{"z-ai/glm-4.6": {}}"#));
        assert!(!models[0].premium);
    }

    /// The budget lookup shares this listing, so an entry has to carry a window. A model that stated
    /// one gets it, and the figure has to survive reaching the picker or the session compacts against
    /// a window belonging to a different service.
    #[test]
    fn a_gateway_entry_carries_the_window_the_budget_is_taken_from() {
        let models = provider_models(&gateway(
            r#"{"anthropic/claude-sonnet-4.5": {"limit": {"context": 1000000, "output": 64000}}}"#,
        ));
        assert_eq!(
            advertised_window(&models, Some("openrouter/anthropic/claude-sonnet-4.5")),
            Some(1_000_000)
        );
    }

    /// A window nobody stated is the conservative default rather than nothing at all. Reporting
    /// nothing would leave the budget on a figure chosen for a different service, and a budget above
    /// the real window does not delay compaction but removes it.
    #[test]
    fn a_gateway_model_with_no_stated_window_still_carries_one() {
        let models = provider_models(&gateway(r#"{"z-ai/glm-4.6": {}}"#));
        assert_eq!(
            advertised_window(&models, Some("openrouter/z-ai/glm-4.6")),
            Some(bravebot_config::provider::CONTEXT_WINDOW)
        );
    }

    /// A gateway is additive on the same terms Bedrock is: its models are offered beside the others
    /// rather than in place of them, so nothing a person could reach before stops being reachable.
    #[test]
    fn gateway_models_are_offered_alongside_the_other_rosters() {
        use bravebot_config::env_var;

        let mut configured = bedrock_models(&bedrock_configured(&[
            (env_var::USE_BEDROCK, "1"),
            (env_var::AWS_REGION, "us-west-2"),
            (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
        ]));
        configured.extend(provider_models(&gateway(r#"{"z-ai/glm-4.6": {}}"#)));

        let roster = combined(
            configured,
            Ok(vec![bravebot_aichat::models::Model::automatic()]),
        )
        .expect("a roster");
        let keys: Vec<&str> = roster.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "opus-arn",
                "openrouter/z-ai/glm-4.6",
                bravebot_config::DEFAULT_MODEL
            ]
        );
    }

    /// A fetched roster arrives roughly newest-first, which puts a model nobody has heard of at the
    /// top and buries the one they work with. The model in force leads, and the rest sort by name so
    /// a half-remembered one is where somebody would look for it.
    #[test]
    fn a_fetched_roster_leads_with_the_model_in_force() {
        // Last in the gateway's order and last alphabetically, so leading is the only way it gets
        // to the top: a test whose input already led with it would pass against no sorting at all.
        let roster = vec![
            listed("openrouter/anthropic/claude-sonnet-4.5", None),
            listed("openrouter/moonshot/kimi-k2", None),
            listed("openrouter/z-ai/glm-4.6", None),
        ];
        let ordered = in_reading_order(roster, "openrouter/z-ai/glm-4.6");
        let keys: Vec<&str> = ordered.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "openrouter/z-ai/glm-4.6",
                "openrouter/anthropic/claude-sonnet-4.5",
                "openrouter/moonshot/kimi-k2"
            ]
        );
    }

    /// Nothing in force is the ordinary first run. Every row still sorts by name rather than keeping
    /// an order that means nothing to the person reading it.
    #[test]
    fn a_fetched_roster_nobody_has_chosen_from_is_still_sorted() {
        let roster = vec![
            listed("openrouter/z-ai/glm-4.6", None),
            listed("openrouter/anthropic/claude-sonnet-4.5", None),
        ];
        let ordered = in_reading_order(roster, bravebot_config::DEFAULT_MODEL);
        let keys: Vec<&str> = ordered.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "openrouter/anthropic/claude-sonnet-4.5",
                "openrouter/z-ai/glm-4.6"
            ]
        );
    }

    /// A gateway's models need no network to know, so a Brave listing that could not be fetched must
    /// not withdraw them: that is the position somebody offline is most likely to be in.
    #[test]
    fn an_unreachable_listing_still_offers_the_gateway_models() {
        let configured = provider_models(&gateway(r#"{"z-ai/glm-4.6": {}}"#));
        let roster = combined(configured, Err("the endpoint is unreachable".into()))
            .expect("the gateway models survive");
        assert_eq!(roster.len(), 1);
        assert_eq!(roster[0].key, "openrouter/z-ai/glm-4.6");
    }

    /// A model chosen in an earlier session is read back off disk, and the window that came with it
    /// is not: it is reported by the listing and nowhere else. Until this was looked up, a session
    /// with room for a hundred thousand tokens compacted at twenty-four thousand.
    #[test]
    fn the_window_of_a_model_chosen_earlier_is_found_in_the_listing() {
        let models = [
            listed("claude-opus", Some(102_400)),
            listed("some-other-model", Some(8_000)),
        ];
        assert_eq!(
            advertised_window(&models, Some("claude-opus")),
            Some(102_400)
        );
    }

    /// Nothing chosen is `automatic`, whose model is resolved per request, so no entry's window is
    /// the one in force.
    #[test]
    fn nothing_chosen_has_no_advertised_window() {
        let models = [listed("claude-opus", Some(102_400))];
        assert_eq!(advertised_window(&models, None), None);
    }

    /// A model that has been withdrawn since it was chosen. The default stands rather than the
    /// window of whichever entry happened to be first.
    #[test]
    fn a_model_the_listing_no_longer_offers_has_no_window() {
        let models = [listed("claude-opus", Some(102_400))];
        assert_eq!(advertised_window(&models, Some("withdrawn-model")), None);
    }

    /// An entry that reports no window of its own leaves the budget alone.
    #[test]
    fn a_model_that_advertises_nothing_has_no_window() {
        let models = [listed("quiet-model", None)];
        assert_eq!(advertised_window(&models, Some("quiet-model")), None);
    }

    /// A configuration whose roster is what a settings file named and whose Brave credentials are
    /// blank, so the listing costs no request and the window under test is the one in the file.
    fn a_config_with_a_named_roster() -> Config {
        use bravebot_config::env_var;

        Config::from_lookup(|key| match key {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
            _ => None,
        })
        .expect("an account named on its own is a working configuration")
    }

    /// A run holds no session and opens no picker, so the window of the model it puts in force is
    /// looked up here or nowhere. Left unlooked up, a run against a model advertising 131,072
    /// tokens compacts at 24,000, and one advertising less than 24,000 cannot compact at all.
    #[test]
    fn a_run_with_no_session_adopts_the_window_of_the_model_in_force() {
        let mut config = a_config_with_a_named_roster();
        assert_eq!(
            config.context_budget,
            bravebot_config::DEFAULT_CONTEXT_BUDGET
        );

        adopt_budget_for_model(&mut config, "opus-arn");

        assert_eq!(
            config.context_budget,
            bravebot_config::bedrock::CONTEXT_WINDOW
        );
        assert!(!config.budget_is_guessed());
    }

    /// A name no roster describes: a model withdrawn since it was chosen, or one a settings file
    /// names and no listing offers. The default stands rather than the window of whichever entry
    /// happened to be first, and the run says nothing about having looked.
    #[test]
    fn a_run_whose_model_no_roster_describes_keeps_the_default() {
        let mut config = a_config_with_a_named_roster();

        adopt_budget_for_model(&mut config, "a-model-nothing-lists");

        assert_eq!(
            config.context_budget,
            bravebot_config::DEFAULT_CONTEXT_BUDGET
        );
        assert!(config.budget_is_guessed());
    }

    /// A reply arrives as hundreds of messages and a draw rebuilds the whole transcript, so a
    /// frame per message put the drawing behind the talking. Everything queued is taken before
    /// the caller draws again, which is what keeps one frame's worth of reply to one frame.
    #[test]
    fn everything_the_worker_has_already_said_is_taken_in_one_pass() {
        let (outbound, inbound) = std::sync::mpsc::channel();
        for piece in ["a", "b", "c", "d"] {
            outbound
                .send(crate::remote_confirm::ToMain::Streaming(piece.to_string()))
                .expect("queued");
        }

        let mut taken = Vec::new();
        let carrying_on = drain_worker(&inbound, FRAME, |message| {
            if let crate::remote_confirm::ToMain::Streaming(text) = message {
                taken.push(text);
            }
        });

        assert!(carrying_on, "the worker is still there");
        assert_eq!(
            taken,
            vec!["a", "b", "c", "d"],
            "the burst took several passes"
        );
    }

    /// The turn is over when the worker lets go of its senders, and the loop has to notice that
    /// rather than waiting a frame at a time forever.
    #[test]
    fn a_worker_that_has_gone_ends_the_wait() {
        let (outbound, inbound) = std::sync::mpsc::channel();
        outbound
            .send(crate::remote_confirm::ToMain::Streaming(
                "last words".into(),
            ))
            .expect("queued");
        drop(outbound);

        let mut taken = Vec::new();
        let carrying_on = drain_worker(&inbound, FRAME, |message| taken.push(message));

        assert_eq!(
            taken.len(),
            1,
            "what was already said was dropped on the floor"
        );
        assert!(!carrying_on, "the loop would have gone on waiting");
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    /// The same chord on the way up. A terminal asked for disambiguated keys sends these as well as
    /// presses, so every loop that reads a key has to tell them apart.
    fn released(key: KeyEvent) -> KeyEvent {
        KeyEvent {
            kind: KeyEventKind::Release,
            ..key
        }
    }

    mod watching {
        use super::*;
        use bravebot_agent::report::{DelegateId, Delegation};

        /// A delegate beginning, numbered the way the driver numbers them.
        fn spawn(session: &mut Session, kind: &'static str, task: &str) -> DelegateId {
            let id = DelegateId::nth(session.delegates().len() as u32 + 1);
            session.delegate_started(Delegation {
                id,
                kind,
                task: task.to_string(),
            });
            session.reporting_for(Some(id));
            id
        }

        /// A turn in flight is the whole of when there is a delegate to watch, so the key has to
        /// answer then above all.
        #[test]
        fn ctrl_l_watches_the_delegate_that_is_working() {
            let mut session = Session::new("kernel-enforced");
            session.status = Status::Working;
            spawn(&mut session, "checker", "run the build");

            handle_key_while_working(&mut session, ctrl('l'));

            assert!(
                session.watching_a_delegate(),
                "the key did not open the view"
            );
        }

        /// A key that does nothing is better than a mode that opens on an empty screen, which
        /// puts a person somewhere with nothing to read and something to get out of.
        #[test]
        fn ctrl_l_does_nothing_where_no_delegate_has_run() {
            let mut session = Session::new("kernel-enforced");

            let action = handle_key(&mut session, ctrl('l'));

            assert!(
                matches!(action, Action::None),
                "the key answered with a mode"
            );
            assert!(session.watching().is_none());
        }

        /// The chord that opened the view leaves it, wherever it has been moved to. Bound onto `j`
        /// on purpose: the keys that walk the list are bare letters matched whatever is held with
        /// them, so a chord read after them would walk the list instead of leaving.
        #[test]
        fn a_moved_chord_leaves_the_view_it_opened() {
            let mut session = Session::new("kernel-enforced");
            let mut moved = std::collections::BTreeMap::new();
            moved.insert("watch".to_string(), "alt-j".to_string());
            session.adopt_keybindings(&moved);
            spawn(&mut session, "reader", "find the parser");
            spawn(&mut session, "checker", "run the build");

            let alt_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT);
            handle_key(&mut session, alt_j);
            assert!(session.watching().is_some(), "the chord did not open it");

            session.open_watched();
            handle_key(&mut session, alt_j);
            assert!(
                session.watching().is_none(),
                "the chord walked the list instead of leaving"
            );

            // And the chord it was moved off of does neither.
            handle_key(&mut session, ctrl('l'));
            assert!(
                session.watching().is_none(),
                "the old chord still opened it"
            );
        }

        /// The way out is read against the nearest level. Somebody who opened a delegate from the
        /// list is going back to the list, and only then out.
        #[test]
        fn q_goes_back_to_the_list_before_it_closes() {
            let mut session = Session::new("kernel-enforced");
            spawn(&mut session, "reader", "find the parser");
            spawn(&mut session, "checker", "run the build");
            handle_key(&mut session, ctrl('l'));
            session.open_watched();

            handle_key(&mut session, key(KeyCode::Char('q')));
            assert!(
                session.listing_delegates(),
                "q left the mode from a delegate"
            );

            handle_key(&mut session, key(KeyCode::Char('q')));
            assert!(session.watching().is_none(), "q did not close the list");
        }

        /// With one delegate there is no list behind it, and a key that went back to an empty
        /// index would be a worse answer than closing.
        #[test]
        fn q_closes_outright_where_there_is_no_list_to_go_back_to() {
            let mut session = Session::new("kernel-enforced");
            spawn(&mut session, "reader", "find the parser");
            handle_key(&mut session, ctrl('l'));

            handle_key(&mut session, key(KeyCode::Char('q')));

            assert!(
                session.watching().is_none(),
                "the view had nowhere to go and stayed"
            );
        }

        /// A person stops the nearest thing. Somebody who went to look at what a delegate was
        /// doing is not asking for the turn to end when they come back out.
        #[test]
        fn the_view_answers_the_stop_keys_before_the_turn_does() {
            let mut session = Session::new("kernel-enforced");
            session.status = Status::Working;
            spawn(&mut session, "checker", "run the build");

            // The loops answer the stop keys before any ladder does, so the guard that lets the
            // view have them first is the whole of what keeps the key that closes it from ending
            // the turn the person opened it to watch.
            for stopping in [ctrl('c'), key(KeyCode::Esc)] {
                assert!(
                    stops_the_turn(&session, stopping),
                    "{stopping:?} did not reach the turn with nothing in the way"
                );
            }

            handle_key_while_working(&mut session, ctrl('l'));
            for stopping in [ctrl('c'), key(KeyCode::Esc)] {
                assert!(
                    !stops_the_turn(&session, stopping),
                    "{stopping:?} stopped the turn from inside the view"
                );
            }

            let action = handle_key_while_working(&mut session, ctrl('c'));

            assert!(
                !matches!(action, Action::Cancel),
                "closing the view cancelled the turn behind it"
            );
            assert!(session.watching().is_none(), "the view did not close");
            assert_eq!(session.status, Status::Working, "the turn was stopped");
        }

        /// A summary and an aside read the stop keys themselves rather than through the turn's
        /// loop, and the view is open across them: it is opened mid-turn and nothing closes it when
        /// the turn ends. Answered by the request instead, Ctrl-C ended the session from inside the
        /// view and Escape wrote a notice into the transcript behind it.
        #[test]
        fn the_view_answers_the_stop_keys_before_a_single_request_does() {
            for stopping in [ctrl('c'), key(KeyCode::Esc)] {
                let mut session = Session::new("kernel-enforced");
                spawn(&mut session, "checker", "run the build");
                session.begin_aside();

                one_request_key(&mut session, ctrl('l'), "nothing to interrupt");
                assert!(session.watching().is_some(), "the view did not open");
                let said = session.transcript.len();

                one_request_key(&mut session, stopping, "nothing to interrupt");

                assert!(
                    session.watching().is_none(),
                    "{stopping:?} left the view open"
                );
                assert!(!session.is_quitting(), "{stopping:?} ended the session");
                assert_eq!(
                    session.transcript.len(),
                    said,
                    "{stopping:?} reported the request from inside the view"
                );
            }
        }

        /// The goal check goes out as a turn ends, which is exactly when a view opened during that
        /// turn is still standing over the session, so it is the loop most likely to be asked one
        /// of these keys with the view open. The goal is not what the press is about there, and
        /// Ctrl-C is not a request to leave.
        #[test]
        fn the_view_answers_the_stop_keys_before_the_goal_check_does() {
            for stopping in [ctrl('c'), key(KeyCode::Esc)] {
                let mut session = Session::new("kernel-enforced");
                spawn(&mut session, "checker", "run the build");
                session.begin_aside();
                session.start_goal("cargo test exits 0".to_string());

                goal_check_key(&mut session, ctrl('l'));
                assert!(session.watching().is_some(), "the view did not open");
                let said = session.transcript.len();

                goal_check_key(&mut session, stopping);

                assert!(
                    session.watching().is_none(),
                    "{stopping:?} left the view open"
                );
                assert!(
                    session.goal().is_some(),
                    "{stopping:?} cleared the goal from inside the view"
                );
                assert!(!session.is_quitting(), "{stopping:?} ended the session");
                assert_eq!(
                    session.transcript.len(),
                    said,
                    "{stopping:?} reported the goal from inside the view"
                );
            }
        }

        /// Reading the mode first is what makes a release worth telling from a press here: the press
        /// closes the view, so the release of that same chord arrives with nothing open and reaches
        /// the request behind it. Handled as a press it ends the session, or takes the goal off, on
        /// the way up from the press that did neither.
        #[test]
        fn the_release_of_the_press_that_closed_the_view_is_not_a_second_press() {
            let mut session = Session::new("kernel-enforced");
            spawn(&mut session, "checker", "run the build");
            session.begin_aside();

            one_request_key(&mut session, ctrl('l'), "nothing to interrupt");
            one_request_key(&mut session, ctrl('c'), "nothing to interrupt");
            assert!(session.watching().is_none(), "the view did not close");

            one_request_key(&mut session, released(ctrl('c')), "nothing to interrupt");
            assert!(!session.is_quitting(), "the release ended the session");

            let mut session = Session::new("kernel-enforced");
            spawn(&mut session, "checker", "run the build");
            session.begin_aside();
            session.start_goal("cargo test exits 0".to_string());

            goal_check_key(&mut session, ctrl('l'));
            goal_check_key(&mut session, ctrl('c'));
            assert!(session.watching().is_none(), "the view did not close");

            goal_check_key(&mut session, released(ctrl('c')));
            assert!(session.goal().is_some(), "the release took the goal off");
            assert!(!session.is_quitting(), "the release ended the session");
        }

        /// Both keys close, and a person who reached for the one they close every other panel
        /// with is not asking for the turn behind it to end.
        #[test]
        fn escape_leaves_the_view_the_way_q_does() {
            for leaving in [key(KeyCode::Esc), key(KeyCode::Char('q'))] {
                let mut session = Session::new("kernel-enforced");
                session.status = Status::Working;
                spawn(&mut session, "reader", "find the parser");
                spawn(&mut session, "checker", "run the build");
                handle_key_while_working(&mut session, ctrl('l'));
                session.open_watched();

                let action = handle_key_while_working(&mut session, leaving);
                assert!(
                    !matches!(action, Action::Cancel),
                    "{leaving:?} cancelled the turn behind the view"
                );
                assert!(
                    session.listing_delegates(),
                    "{leaving:?} did not go back to the list"
                );

                handle_key_while_working(&mut session, leaving);
                assert!(session.watching().is_none(), "{leaving:?} did not close");
                assert_eq!(session.status, Status::Working, "the turn was stopped");
            }
        }

        /// Nothing falls through to a box the person cannot see. Typed there, the words would be
        /// waiting in a line nobody knows they are writing.
        #[test]
        fn a_typed_character_does_not_reach_the_box_while_a_delegate_is_watched() {
            let mut session = Session::new("kernel-enforced");
            spawn(&mut session, "reader", "find the parser");
            handle_key(&mut session, ctrl('l'));

            handle_key(&mut session, key(KeyCode::Char('x')));
            handle_key(&mut session, key(KeyCode::Char('z')));

            assert!(session.input().is_empty(), "what was typed reached the box");
        }

        /// Comparing two runs is what having several is for, and going back through the list to
        /// do it is three keys where one will do.
        #[test]
        fn n_and_p_move_between_delegates() {
            let mut session = Session::new("kernel-enforced");
            spawn(&mut session, "reader", "find the parser");
            spawn(&mut session, "checker", "run the build");
            handle_key(&mut session, ctrl('l'));
            session.open_watched();

            handle_key(&mut session, key(KeyCode::Char('p')));
            assert_eq!(
                session.watched_delegate().map(|delegate| delegate.kind),
                Some("reader")
            );

            handle_key(&mut session, key(KeyCode::Char('n')));
            assert_eq!(
                session.watched_delegate().map(|delegate| delegate.kind),
                Some("checker")
            );
        }

        /// The list is a list of one thing to do: open the row it is on.
        #[test]
        fn enter_opens_the_delegate_the_list_is_on() {
            let mut session = Session::new("kernel-enforced");
            spawn(&mut session, "reader", "find the parser");
            spawn(&mut session, "checker", "run the build");
            handle_key(&mut session, ctrl('l'));
            handle_key(&mut session, key(KeyCode::Up));

            handle_key(&mut session, key(KeyCode::Enter));

            assert!(
                session.watching_a_delegate(),
                "enter did not open a delegate"
            );
            assert_eq!(
                session.watched_delegate().map(|delegate| delegate.kind),
                Some("reader"),
                "enter opened a delegate other than the one the list was on"
            );
        }

        /// The session is a row in the list like the others, so the key that opens a row is what
        /// takes somebody back to the conversation. Leaving by a key that is on no row is the
        /// part of this that was hard to find.
        #[test]
        fn opening_the_session_row_goes_back_to_the_conversation() {
            let mut session = Session::new("kernel-enforced");
            spawn(&mut session, "reader", "find the parser");
            spawn(&mut session, "checker", "run the build");
            handle_key(&mut session, ctrl('l'));
            handle_key(&mut session, key(KeyCode::Up));
            handle_key(&mut session, key(KeyCode::Up));
            assert!(
                session.listing_on_the_session(),
                "the highlight never reached the session row"
            );

            handle_key(&mut session, key(KeyCode::Enter));

            assert!(
                session.watching().is_none(),
                "enter on the session row did not go back to the conversation"
            );
        }
    }

    mod scroller {
        use super::*;
        use crate::state::Laid;

        /// A session with a transcript already laid out, so the keys have rows to move over.
        /// The numbers are the ones a frame would have written back after drawing.
        fn reading() -> Session {
            let mut session = Session::new("kernel-enforced");
            session.note_layout(Laid {
                width: 80,
                height: 10,
                rows: 100,
                prompts: vec![0, 30, 60],
                matches: Vec::new(),
            });
            session
        }

        fn opened() -> Session {
            let mut session = reading();
            session.open_scroller();
            session
        }

        #[test]
        fn ctrl_o_opens_the_scroller() {
            let mut session = reading();
            assert!(!session.scrolling());

            handle_key(&mut session, ctrl('o'));

            assert!(session.scrolling(), "the key did not open it");
        }

        /// A viewer that jumps somewhere on the way in has lost the thing the person opened it to
        /// look at. The scroller reads the offset the wheel writes, so there is nothing to jump.
        #[test]
        fn the_scroller_opens_on_the_view_that_was_already_there() {
            let mut session = reading();
            session.scroll_up(7);

            handle_key(&mut session, ctrl('o'));

            assert_eq!(session.scroll, 7, "opening it moved the view");
        }

        #[test]
        fn q_escape_and_ctrl_o_each_close_the_scroller() {
            for closing in [key(KeyCode::Char('q')), key(KeyCode::Esc), ctrl('o')] {
                let mut session = opened();
                handle_key(&mut session, closing);
                assert!(!session.scrolling(), "{closing:?} did not close it");
            }
        }

        #[test]
        fn closing_the_scroller_leaves_the_view_where_it_was() {
            let mut session = opened();
            handle_key(&mut session, key(KeyCode::Char('k')));
            handle_key(&mut session, key(KeyCode::Char('k')));
            let looking_at = session.scroll;

            handle_key(&mut session, key(KeyCode::Char('q')));

            assert_eq!(session.scroll, looking_at, "closing it moved the view");
        }

        /// The scroller is the nearest thing there is to stop, so the press that stops it is not
        /// also the press that stops the turn. Each rung of that ladder is visible, and this one
        /// is the innermost.
        #[test]
        fn ctrl_c_closes_the_scroller_before_it_reaches_anything_else() {
            let mut session = opened();
            session.status = Status::Working;

            assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
            assert!(!session.scrolling(), "the scroller stayed open");
            assert_eq!(
                session.status,
                Status::Working,
                "the turn was stopped by the press that closed the scroller"
            );

            assert_eq!(
                handle_key(&mut session, ctrl('c')),
                Action::Cancel,
                "the next press did not reach the turn"
            );
        }

        /// Reading back through a turn that is going wrong is reading precisely because it is
        /// going wrong. Nothing in the mode sends anything, so there is nothing here for a
        /// running turn to refuse.
        #[test]
        fn a_turn_goes_on_running_while_the_scroller_is_open() {
            let mut session = reading();
            session.status = Status::Working;

            handle_key_while_working(&mut session, ctrl('o'));
            assert!(session.scrolling(), "the key did not open it mid-turn");

            for pressed in [key(KeyCode::Char('k')), key(KeyCode::Char('g'))] {
                assert_eq!(
                    handle_key_while_working(&mut session, pressed),
                    Action::Redraw
                );
            }

            assert_eq!(session.status, Status::Working, "the turn was stopped");
            assert!(session.scrolling(), "the scroller closed on its own");
        }

        /// A mode that leaks its keystrokes into a box nobody can see is the worse half of both:
        /// `j` would scroll and also type a `j`, and the only way to find out would be to close
        /// the scroller and look.
        /// The loops answer the stop keys before anything else does, so the guard that lets the
        /// scroller have them first is the whole of what keeps the key that closes it from
        /// ending the turn the person opened it to read.
        #[test]
        fn the_scroller_answers_the_stop_keys_before_the_turn_does() {
            let mut session = reading();
            session.status = Status::Working;

            for stopping in [ctrl('c'), key(KeyCode::Esc)] {
                assert!(
                    stops_the_turn(&session, stopping),
                    "{stopping:?} did not reach the turn with nothing in the way"
                );
            }

            session.open_scroller();
            for stopping in [ctrl('c'), key(KeyCode::Esc)] {
                assert!(
                    !stops_the_turn(&session, stopping),
                    "{stopping:?} stopped the turn from inside the scroller"
                );
            }
        }

        #[test]
        fn a_typed_character_does_not_reach_the_box_while_the_scroller_is_open() {
            let mut session = opened();
            for c in "jkgbnv".chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }

            assert!(
                session.input().is_empty(),
                "the keys reached the box: {:?}",
                session.input()
            );
        }

        #[test]
        fn the_line_comes_back_untouched_when_the_scroller_closes() {
            let mut session = reading();
            for c in "half a thought".chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }
            handle_key(&mut session, key(KeyCode::Left));

            handle_key(&mut session, ctrl('o'));
            for c in "jkG{}".chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }
            handle_key(&mut session, key(KeyCode::Char('q')));

            assert_eq!(session.input(), "half a thought");

            // The caret is where it was left, which is the half of the line's state that is not
            // in the text: typing here has to land where it would have landed.
            handle_key(&mut session, key(KeyCode::Char('!')));
            assert_eq!(session.input(), "half a though!t");
        }

        /// A paste and a drop reach this process as events of their own rather than as keys, so
        /// the guard that holds the box still for a keystroke does not see them. What the person
        /// comes back to has to be the line they left: a fragment spliced in at a caret they
        /// cannot see is a different prompt by the time the mode closes, and an attachment staged
        /// from in here is one nobody asked for.
        #[test]
        fn a_paste_and_a_drop_do_not_reach_the_line_while_the_scroller_is_open() {
            let directory = crate::testutil::scratch_dir("bravebot-app-drop-while-scrolling");
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).expect("scratch");
            let file = directory.join("shot.png");
            std::fs::write(&file, [0x89u8, 0x50]).expect("write");

            let mut session = reading().in_workspace(&directory);
            for c in "half a thought".chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }
            handle_key(&mut session, key(KeyCode::Left));
            handle_key(&mut session, ctrl('o'));

            assert_eq!(handle_paste(&mut session, "pasted"), Action::None);
            // The clipboard holding a picture, which arrives as a paste of nothing at all.
            assert_eq!(handle_paste(&mut session, ""), Action::None);
            handle_paste_while_working(&mut session, &file.to_string_lossy());

            handle_key(&mut session, key(KeyCode::Char('q')));
            assert_eq!(session.input(), "half a thought");
            assert!(
                session.attached().is_empty(),
                "a drop staged an attachment from inside the scroller"
            );

            // The caret is where it was left, which is the half of the line's state that is not
            // in the text.
            handle_key(&mut session, key(KeyCode::Char('!')));
            assert_eq!(session.input(), "half a though!t");

            let _ = std::fs::remove_dir_all(&directory);
        }

        #[test]
        fn enter_sends_nothing_from_inside_the_scroller() {
            let mut session = opened();
            for c in "a prompt".chars() {
                session.type_char(c);
            }

            assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::None);
            assert_eq!(session.input(), "a prompt", "the line was taken");
        }

        #[test]
        fn a_key_the_scroller_does_not_take_does_nothing() {
            let mut session = opened();
            let before = session.scroll;

            for pressed in [key(KeyCode::Char('z')), key(KeyCode::Tab), ctrl('w')] {
                assert_eq!(
                    handle_key(&mut session, pressed),
                    Action::None,
                    "{pressed:?} did something"
                );
            }

            assert_eq!(session.scroll, before);
            assert!(session.input().is_empty());
        }

        #[test]
        fn the_line_keys_move_the_view_by_a_line() {
            let mut session = opened();

            handle_key(&mut session, key(KeyCode::Up));
            assert_eq!(session.scroll, 1);
            handle_key(&mut session, key(KeyCode::Char('k')));
            assert_eq!(session.scroll, 2);
            handle_key(&mut session, key(KeyCode::Char('j')));
            assert_eq!(session.scroll, 1);
            handle_key(&mut session, key(KeyCode::Down));
            assert_eq!(session.scroll, 0);
        }

        #[test]
        fn the_half_page_keys_move_the_view_by_half_a_screen() {
            let mut session = opened();

            handle_key(&mut session, ctrl('u'));
            assert_eq!(session.scroll, 5);
            handle_key(&mut session, ctrl('d'));
            assert_eq!(session.scroll, 0);
        }

        /// Both dialects, because somebody who knows one spelling should not find the other
        /// typing a letter at them.
        #[test]
        fn the_page_keys_move_the_view_by_a_whole_screen() {
            for (back, on) in [
                (key(KeyCode::Char('b')), key(KeyCode::Char(' '))),
                (ctrl('b'), ctrl('f')),
                (key(KeyCode::PageUp), key(KeyCode::PageDown)),
            ] {
                let mut session = opened();
                handle_key(&mut session, back);
                assert_eq!(session.scroll, 10, "{back:?} did not move a screen");
                handle_key(&mut session, on);
                assert_eq!(session.scroll, 0, "{on:?} did not move a screen");
            }
        }

        #[test]
        fn g_and_shift_g_reach_the_first_row_and_the_last() {
            for (first, last) in [
                (key(KeyCode::Char('g')), key(KeyCode::Char('G'))),
                (ctrl_key(KeyCode::Home), ctrl_key(KeyCode::End)),
            ] {
                let mut session = opened();
                handle_key(&mut session, first);
                assert_eq!(
                    session.top_row(),
                    0,
                    "{first:?} did not reach the first row"
                );
                handle_key(&mut session, last);
                assert_eq!(session.top_row(), 90, "{last:?} did not reach the last");
            }
        }

        /// A prompt is the one thing in a transcript the person wrote themselves, so where these
        /// land is settled by what they typed and by nothing read out of the workspace.
        #[test]
        fn the_prompt_keys_land_on_the_turn_before_and_the_turn_after() {
            let mut session = opened();

            handle_key(&mut session, key(KeyCode::Char('{')));
            assert_eq!(session.top_row(), 60);
            handle_key(&mut session, key(KeyCode::Char('{')));
            assert_eq!(session.top_row(), 30);
            handle_key(&mut session, key(KeyCode::Char('}')));
            assert_eq!(session.top_row(), 60);

            // Past the last prompt there is nowhere further to go but the end of the transcript,
            // which is where somebody pressing the key again is asking to be.
            handle_key(&mut session, key(KeyCode::Char('}')));
            assert_eq!(session.top_row(), 90);
        }

        /// A held key has to come to rest somewhere the next press can move away from. Counting
        /// past the end and back again is a key that does nothing for as long as it was held.
        #[test]
        fn the_view_stops_at_the_first_row_rather_than_scrolling_past_it() {
            let mut session = opened();
            for _ in 0..40 {
                handle_key(&mut session, key(KeyCode::Char('b')));
            }

            assert_eq!(session.top_row(), 0);
            handle_key(&mut session, key(KeyCode::Char('j')));
            assert_eq!(
                session.top_row(),
                1,
                "the view had counted past the first row"
            );
        }

        #[test]
        fn the_view_stops_at_the_last_row_rather_than_scrolling_past_it() {
            let mut session = opened();
            for _ in 0..40 {
                handle_key(&mut session, key(KeyCode::Char(' ')));
            }

            assert_eq!(session.scroll, 0);
            handle_key(&mut session, key(KeyCode::Char('k')));
            assert_eq!(session.scroll, 1, "the view had counted past the last row");
        }

        #[test]
        fn the_wheel_scrolls_the_scroller_as_it_scrolls_the_transcript() {
            let mut session = opened();

            handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 0, 0));
            assert_eq!(session.scroll, 3);
            handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 0, 0));
            assert_eq!(session.scroll, 0);

            for _ in 0..40 {
                handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 0, 0));
            }
            assert_eq!(session.top_row(), 0, "the wheel counted past the first row");
        }

        /// The keys go into the needle while one is being typed, because that is what typing
        /// means. Abandoning it leaves the view where it was rather than where a half-typed
        /// search had reached.
        #[test]
        fn escape_abandons_a_half_typed_search() {
            let mut session = opened();
            session.scroller_back(20);
            let looking_at = session.scroll;

            handle_key(&mut session, key(KeyCode::Char('/')));
            for c in "gjkq".chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }
            assert!(session.typing_a_search(), "a letter was read as a movement");
            assert_eq!(session.scroll, looking_at, "typing moved the view");

            handle_key(&mut session, key(KeyCode::Esc));

            assert!(!session.typing_a_search());
            assert!(
                session.scrolling(),
                "abandoning the search closed the scroller"
            );
            assert_eq!(session.scroll, looking_at);
        }

        /// A mode with no way out is what this interface is most careful about, and the loops
        /// hand these two presses to the scroller before anything else sees them: one that
        /// swallowed them would leave a person pressing Ctrl-C at a turn it never reaches, with
        /// nothing on the screen changing to say why.
        #[test]
        fn the_chords_that_close_the_scroller_close_it_while_a_search_is_typed() {
            for closing in [ctrl('o'), ctrl('c')] {
                let mut session = opened();
                handle_key(&mut session, key(KeyCode::Char('/')));
                for c in "ne".chars() {
                    handle_key(&mut session, key(KeyCode::Char(c)));
                }
                assert!(session.typing_a_search(), "the search was not being typed");

                assert_eq!(
                    handle_key(&mut session, closing),
                    Action::Redraw,
                    "{closing:?} did nothing at all"
                );
                assert!(!session.scrolling(), "{closing:?} did not close it");
            }
        }

        /// The nearest thing there is to stop, which is the ladder every other stop key here
        /// walks. Closing the mode to get the highlights off the screen would mean losing the
        /// place somebody had scrolled to in order to undo a search they had finished with.
        #[test]
        fn escape_clears_a_finished_search_before_it_closes_the_scroller() {
            let mut session = opened();
            session.scroller_back(20);
            let looking_at = session.scroll;

            handle_key(&mut session, key(KeyCode::Char('/')));
            for c in "notes".chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }
            handle_key(&mut session, key(KeyCode::Enter));
            assert_eq!(session.needle(), "notes");

            handle_key(&mut session, key(KeyCode::Esc));
            assert_eq!(session.needle(), "", "the search was not cleared");
            assert!(
                session.scrolling(),
                "clearing the search closed the scroller"
            );
            assert_eq!(
                session.scroll, looking_at,
                "clearing the search moved the view"
            );

            handle_key(&mut session, key(KeyCode::Esc));
            assert!(!session.scrolling(), "the next press did not close it");
        }

        /// Backspacing past the start is what the key means when there is nothing left of the
        /// thing it deletes: the search goes, rather than the press doing nothing at all.
        #[test]
        fn backspacing_past_the_start_abandons_the_search() {
            let mut session = opened();
            handle_key(&mut session, key(KeyCode::Char('/')));
            handle_key(&mut session, key(KeyCode::Char('a')));

            handle_key(&mut session, key(KeyCode::Backspace));
            assert!(
                session.typing_a_search(),
                "one character took the whole search"
            );
            handle_key(&mut session, key(KeyCode::Backspace));
            assert!(!session.typing_a_search());
        }

        /// A mode where the letters do nothing and nothing says why is indistinguishable from an
        /// interface that has stopped responding.
        #[test]
        fn the_help_key_lists_the_keys() {
            let mut session = opened();

            handle_key(&mut session, key(KeyCode::Char('?')));
            assert!(session.scroller().expect("open").help);

            // Read instead of the transcript rather than alongside it, so anything at all puts it
            // away and that press is spent doing so.
            handle_key(&mut session, key(KeyCode::Char('j')));
            assert!(!session.scroller().expect("open").help);
            assert_eq!(
                session.scroll, 0,
                "the press that put the list away also moved"
            );
        }

        #[test]
        fn v_asks_for_the_editor() {
            let mut session = opened();
            assert_eq!(
                handle_key(&mut session, key(KeyCode::Char('v'))),
                Action::Show
            );
        }

        /// The same answer the key that edits a prompt gives, and for the same reason: an editor
        /// needs the screen, and a running turn is drawing it.
        #[test]
        fn the_transcript_editor_key_does_nothing_while_a_turn_runs() {
            let mut session = opened();
            session.status = Status::Working;

            assert_eq!(
                handle_key_while_working(&mut session, key(KeyCode::Char('v'))),
                Action::None
            );
        }
    }

    mod pasting {
        use super::*;
        use crate::clipboard::{Image, MAX_IMAGE_BYTES, Pasted};

        fn picture(bytes: Vec<u8>) -> Pasted {
            Pasted::Image(Image {
                media_type: "image/png",
                bytes,
            })
        }

        /// The chord exists because Command-V cannot reach this process, so it has to be answered
        /// where the keys are read rather than left to the terminal.
        #[test]
        fn ctrl_v_asks_for_the_clipboard_to_be_read() {
            let mut session = Session::new("kernel-enforced");
            assert_eq!(handle_key(&mut session, ctrl('v')), Action::Paste);
        }

        /// A line can be typed while a turn runs, so it can be pasted into while a turn runs. What
        /// is refused mid-turn is sending, never writing.
        ///
        /// The picture rather than the answer, because the answer was right all along and every
        /// working loop threw it away: the key reached its arm, produced `Action::Paste`, and the
        /// clipboard was never read.
        #[test]
        fn ctrl_v_reads_the_clipboard_during_a_turn_too() {
            let mut session = Session::new("kernel-enforced");
            session.status = Status::Working;

            let action = handle_key_while_working(&mut session, ctrl('v'));
            assert_eq!(action, Action::Paste);
            act_while_working(&mut session, action, || picture(b"pixels".to_vec()));

            assert_eq!(
                session.input(),
                "[Image #1]",
                "the picture the key asked for was not staged"
            );
        }

        /// A picture pasted with the terminal's own chord arrives as a paste of nothing, and mid-turn
        /// is when one is pasted: the person is watching the reply the screenshot is about. The empty
        /// paste went into the box instead, which writes nothing and says nothing, so the picture was
        /// dropped and the key that would have carried it was never named.
        #[test]
        fn an_empty_paste_mid_turn_goes_and_reads_the_clipboard_too() {
            let mut session = Session::new("kernel-enforced");
            session.status = Status::Working;

            let action = handle_paste_while_working(&mut session, "");
            assert!(
                session.transcript[0].text.contains("ctrl-v"),
                "the note did not name the key that works"
            );
            act_while_working(&mut session, action, || picture(b"pixels".to_vec()));

            assert_eq!(
                session.input(),
                "[Image #1]",
                "the picture the empty paste stood for was not staged"
            );
        }

        /// Nothing is read for a paste that carried text, mid-turn as at rest: every Command-V of a
        /// paragraph would otherwise spawn the platform's clipboard tools for an answer the paste
        /// already had.
        #[test]
        fn a_paste_that_carried_text_mid_turn_is_left_alone() {
            let mut session = Session::new("kernel-enforced");
            session.status = Status::Working;

            let action = handle_paste_while_working(&mut session, "some words");
            assert_eq!(action, Action::Redraw);
            act_while_working(&mut session, action, || {
                panic!("the clipboard was read for a paste that carried text")
            });

            assert_eq!(session.input(), "some words");
        }

        /// A paste that arrives carrying nothing is a Command-V the terminal could not answer: it
        /// wrote the markers and found no text between them, which is what a clipboard holding
        /// only a picture looks like from in here.
        #[test]
        fn an_empty_paste_goes_and_reads_the_clipboard_instead() {
            let mut session = Session::new("kernel-enforced");
            assert_eq!(handle_paste(&mut session, ""), Action::Paste);
            assert!(
                session.transcript[0].text.contains("ctrl-v"),
                "the note did not name the key that works"
            );
        }

        /// Said once and then not again: a user who has been told which key carries a picture does
        /// not need telling every time they use the other one.
        #[test]
        fn which_key_carries_a_picture_is_said_once_per_session() {
            let mut session = Session::new("kernel-enforced");
            handle_paste(&mut session, "");
            handle_paste(&mut session, "");
            handle_paste(&mut session, "");

            assert_eq!(session.transcript.len(), 1, "the note was repeated");
        }

        /// A paste that did carry text is an ordinary paste and must stay one, or every Command-V
        /// of a paragraph would go and read the clipboard a second time.
        #[test]
        fn a_paste_that_carried_text_is_left_alone() {
            let mut session = Session::new("kernel-enforced");
            assert_eq!(handle_paste(&mut session, "some words"), Action::Redraw);
            assert_eq!(session.input(), "some words");
            assert!(
                session.transcript.is_empty(),
                "an ordinary paste said something"
            );
        }

        /// Shell mode's line is a command, and a marker in one is text the shell would be handed
        /// verbatim. Nothing about a picture belongs there.
        #[test]
        fn a_picture_is_refused_in_shell_mode_rather_than_written_into_the_command() {
            let mut session = Session::new("kernel-enforced");
            session.shell = true;
            take_from_clipboard(&mut session, picture(b"pixels".to_vec()));

            assert!(
                session.input().is_empty(),
                "a marker reached the command line"
            );
            assert!(session.transcript[0].text.contains("not a command"));
        }

        #[test]
        fn a_picture_off_the_clipboard_becomes_a_marker_in_the_line() {
            let mut session = Session::new("kernel-enforced");
            take_from_clipboard(&mut session, picture(b"pixels".to_vec()));
            assert_eq!(session.input(), "[Image #1]");
        }

        /// Refused rather than truncated, and with the size, because half a picture would be sent,
        /// rejected by the endpoint, and reported as a fault of the request.
        #[test]
        fn a_picture_too_large_to_send_says_so_with_its_size() {
            let mut session = Session::new("kernel-enforced");
            take_from_clipboard(&mut session, Pasted::TooLarge(MAX_IMAGE_BYTES * 2));

            assert!(
                session.input().is_empty(),
                "an oversized picture reached the line"
            );
            assert!(
                session.transcript[0].text.contains("20.0 MB"),
                "the size was not reported: {}",
                session.transcript[0].text
            );
        }

        /// Carrying on saying it once the picture is in the prompt is nagging, and the answer is
        /// stale in any case: the hint is about what a key would do, and the key has been pressed.
        #[test]
        fn a_paste_clears_the_hint_that_prompted_it() {
            let mut session = Session::new("kernel-enforced");
            session.image_on_clipboard = true;
            take_from_clipboard(&mut session, picture(b"pixels".to_vec()));

            assert!(!session.image_on_clipboard, "the hint outlived the paste");
        }
    }

    fn shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }

    /// A session with `text` typed into the box, one key at a time.
    fn typed_into(text: &str) -> Session {
        let mut session = Session::new("none");
        for c in text.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        session
    }

    fn drag(kind: MouseEventKind, row: u16, column: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Capturing the mouse for the wheel took the terminal's own selection away, so a drag has
    /// to select here or a user cannot copy a line of their own transcript at all.
    #[test]
    fn dragging_selects_and_releasing_asks_for_the_copy() {
        let mut session = Session::new("none");

        assert_eq!(
            handle_mouse(
                &mut session,
                drag(MouseEventKind::Down(MouseButton::Left), 3, 4)
            ),
            Action::Redraw
        );
        assert_eq!(
            handle_mouse(
                &mut session,
                drag(MouseEventKind::Drag(MouseButton::Left), 3, 20)
            ),
            Action::Redraw
        );
        assert_eq!(
            handle_mouse(
                &mut session,
                drag(MouseEventKind::Up(MouseButton::Left), 3, 20)
            ),
            Action::Copy
        );

        let selection = session.selection.expect("nothing was selected");
        assert!(!selection.is_empty());
        assert!(
            selection.covers(3, 10),
            "the sweep did not cover its middle"
        );
    }

    /// The sequence has to leave a terminal that keeps one tracking state in the mode that
    /// reports drags, or the wheel stops reaching the session and scrolls the window behind it
    /// instead. What is asked for last is what such a terminal ends up in.
    #[test]
    fn the_mouse_request_ends_in_the_mode_that_reports_a_drag() {
        assert!(
            TRACK_MOTION_ONLY_WHILE_DRAGGING.ends_with("\x1b[?1002h"),
            "a terminal with one tracking state would be left without drags"
        );
        assert!(
            TRACK_MOTION_ONLY_WHILE_DRAGGING.starts_with("\x1b[?1003l"),
            "all-motion reporting was never turned off"
        );
        assert!(
            TRACK_MOTION_ONLY_WHILE_DRAGGING.contains("\x1b[?1000h"),
            "button reporting, which carries the wheel, was not asked for again"
        );
    }

    /// A pointer crossing the window is not input. Terminals report that motion by default, and
    /// answering each report with a redraw is what made a drag crawl: the queue outran the
    /// drawing and the highlight trailed behind the pointer.
    #[test]
    fn moving_the_pointer_without_a_button_asks_for_nothing() {
        let mut session = Session::new("none");
        session.begin_selection(2, 2);
        session.extend_selection(2, 8);

        let action = handle_mouse(&mut session, drag(MouseEventKind::Moved, 9, 40));

        assert_eq!(action, Action::None, "a bare move asked for work");
        let selection = session.selection.expect("the selection was disturbed");
        assert!(
            selection.covers(2, 4),
            "the selection moved with the pointer"
        );
    }

    /// Selecting has nothing to do with the turn: what is on the screen is already there, and a
    /// long turn is exactly when someone wants to copy part of it.
    #[test]
    fn selecting_works_while_a_turn_is_running() {
        let mut session = Session::new("none");
        session.type_char('x');
        session.submit();
        assert_eq!(session.status, Status::Working);

        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(MouseButton::Left), 1, 0),
        );
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Drag(MouseButton::Left), 1, 5),
        );
        assert!(session.selection.is_some());
    }

    /// The bug this exists for: a prompt copied from somewhere else usually ends in a newline,
    /// and a paste delivered as keystrokes turns that into Enter. It used to send itself before
    /// its author had read it back.
    #[test]
    fn a_paste_that_ends_in_a_newline_does_not_send_it() {
        let mut session = Session::new("none");
        let action = handle_paste(&mut session, "write me a game\n");

        assert_eq!(action, Action::Redraw);
        assert_eq!(session.input(), "write me a game\n");
        assert_eq!(session.status, Status::Idle, "the paste started a turn");
        assert!(session.transcript.is_empty(), "the paste sent something");
    }

    /// A working loop is where a paste most needs folding, since a turn in flight is what the
    /// long paste would push off the screen. One of the two loops used to write the paste whole,
    /// so this pins the folding at the seam they now share rather than at either call site.
    #[test]
    fn a_long_paste_folds_while_a_turn_is_running() {
        let mut session = Session::new("none");
        handle_paste_while_working(&mut session, "first\nsecond\nthird\nfourth");

        assert_eq!(session.input(), "[Pasted text #1 +4 lines]");
    }

    /// A drop reaches the terminal as a paste, so a working loop that only pasted wrote the path
    /// out as prose: the file was never staged, and the line said nothing about an attachment.
    /// A turn in flight is when people drop a file, because they are reading a reply and want to
    /// hand over the picture it is about.
    #[test]
    fn a_file_dropped_while_a_turn_is_running_is_attached() {
        let directory = crate::testutil::scratch_dir("bravebot-app-drop-while-working");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("scratch");
        let file = directory.join("shot.png");
        std::fs::write(&file, [0x89u8, 0x50]).expect("write");

        let mut session = Session::new("none").in_workspace(&directory);
        session.type_char('a');
        session.submit();
        handle_paste_while_working(&mut session, &file.to_string_lossy());

        assert_eq!(session.input(), "[Image #1] ");
        assert_eq!(session.attached().len(), 1, "nothing was staged");
        assert_eq!(session.attached()[0].name, "shot.png");

        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A paragraph can be written in the box rather than only pasted into it. Shift-Enter is the
    /// one Enter that does not send.
    #[test]
    fn shift_enter_starts_a_line_instead_of_sending() {
        let mut session = typed_into("first");
        assert_eq!(
            handle_key(&mut session, shift(KeyCode::Enter)),
            Action::Redraw
        );
        for c in "second".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(session.input(), "first\nsecond");
        assert_eq!(session.status, Status::Idle, "the newline started a turn");
        assert!(session.transcript.is_empty(), "the newline sent something");
    }

    /// The bug this exists for: a terminal that cannot report the modifier on Enter is configured
    /// to send `\n` for Shift-Enter instead, which arrives in raw mode as Ctrl-J. Unbound, the
    /// keystroke did nothing at all, which is what iTerm and Terminal.app both showed.
    #[test]
    fn ctrl_j_starts_a_line_too() {
        let mut session = typed_into("first");
        assert_eq!(handle_key(&mut session, ctrl('j')), Action::Redraw);
        for c in "second".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(session.input(), "first\nsecond");
        assert_eq!(session.status, Status::Idle, "the newline started a turn");
    }

    /// Whichever spelling arrives, sending is still Enter's alone.
    #[test]
    fn ctrl_j_is_not_swallowed_while_a_turn_runs() {
        let mut session = typed_into("go");
        handle_key(&mut session, key(KeyCode::Enter));
        handle_key_while_working(&mut session, key(KeyCode::Char('a')));

        assert_eq!(
            handle_key_while_working(&mut session, ctrl('j')),
            Action::Redraw,
            "the control guard swallowed the newline"
        );
        assert_eq!(session.input(), "a\n");
    }

    /// Ctrl-M is Enter itself, so binding it would take sending away.
    #[test]
    fn only_ctrl_j_starts_a_line() {
        assert!(starts_a_line(ctrl('j')));
        assert!(starts_a_line(shift(KeyCode::Enter)));
        assert!(!starts_a_line(ctrl('m')));
        assert!(!starts_a_line(key(KeyCode::Enter)));
        assert!(!starts_a_line(key(KeyCode::Char('j'))));
    }

    /// The newline lands at the caret like any other keystroke, not at the end of the line.
    #[test]
    fn a_newline_lands_at_the_caret() {
        let mut session = typed_into("ab");
        handle_key(&mut session, key(KeyCode::Left));
        handle_key(&mut session, shift(KeyCode::Enter));

        assert_eq!(session.input(), "a\nb");
        assert_eq!(session.caret(), 2);
    }

    /// And plain Enter still sends the paragraph, once: the two are not the same key.
    #[test]
    fn enter_still_sends_a_paragraph_written_with_shift_enter() {
        let mut session = typed_into("first");
        handle_key(&mut session, shift(KeyCode::Enter));
        for c in "second".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("first\nsecond".to_string())
        );
    }

    /// A `!` after a newline is punctuation in a sentence, not a request for a shell: the mode is
    /// only ever armed by the first character of an empty line.
    #[test]
    fn a_newline_does_not_arm_shell_mode() {
        let mut session = typed_into("wait");
        handle_key(&mut session, shift(KeyCode::Enter));
        handle_key(&mut session, key(KeyCode::Char('!')));

        assert!(!session.shell, "a newline left the box looking empty");
        assert_eq!(session.input(), "wait\n!");
    }

    /// What arms the mode is a keystroke, and a paste is not one. The text arrived from a file, a
    /// web page or somebody suggesting a command, and any of those can begin with `!`, so arming
    /// from one would leave a line nobody chose to run one Enter from a shell. It goes to the
    /// planner as the words it is, `!` and all.
    #[test]
    fn a_pasted_line_does_not_arm_shell_mode() {
        let mut session = Session::new("none");
        handle_paste(&mut session, "!rm -rf build");

        assert!(!session.shell, "a paste armed shell mode");
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("!rm -rf build".to_string()),
            "the pasted line was taken as a command"
        );
    }

    /// A multi-line command is a `for` loop somebody typed, so the mode gets the key too.
    #[test]
    fn shift_enter_works_in_shell_mode() {
        let mut session = Session::new("none");
        type_line(&mut session, "!for f in *; do");
        handle_key(&mut session, shift(KeyCode::Enter));
        type_line(&mut session, "  echo $f");

        assert!(session.shell, "the mode was left by a newline");
        assert_eq!(session.input(), "for f in *; do\n  echo $f");
    }

    /// A turn in flight refuses Enter but not this one: what can be typed mid-turn can be written
    /// as a paragraph mid-turn.
    #[test]
    fn shift_enter_works_while_a_turn_runs() {
        let mut session = typed_into("go");
        handle_key(&mut session, key(KeyCode::Enter));
        handle_key_while_working(&mut session, key(KeyCode::Char('a')));

        assert_eq!(
            handle_key_while_working(&mut session, shift(KeyCode::Enter)),
            Action::Redraw
        );
        assert_eq!(session.input(), "a\n");
    }

    /// And it is still one prompt afterwards: Enter sends what was pasted, once.
    #[test]
    fn a_pasted_prompt_is_sent_when_the_user_says_so() {
        let mut session = Session::new("none");
        handle_paste(&mut session, "write me a game\n");

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("write me a game".to_string())
        );
    }

    #[test]
    fn typing_a_character_asks_for_a_redraw() {
        let mut session = Session::new("none");
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Char('a'))),
            Action::Redraw
        );
        assert_eq!(session.input(), "a");
    }

    #[test]
    fn enter_submits_the_prompt() {
        let mut session = Session::new("none");
        for c in "hello".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("hello".to_string())
        );
    }

    #[test]
    fn enter_on_empty_input_does_nothing() {
        let mut session = Session::new("none");
        assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::None);
        assert_eq!(session.status, Status::Idle);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut session = Session::new("none");
        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
        assert!(session.is_quitting());
    }

    /// Escape on an empty line used to leave, which made every press a question of what was in
    /// the box: the key for abandoning a thought ended the session as soon as the thought was
    /// short enough. It abandons, and never more than that.
    #[test]
    fn escape_on_an_empty_line_does_not_quit() {
        let mut session = Session::new("none");
        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert!(!session.is_quitting(), "escape ended the session");
    }

    /// Escape is how a half-typed prompt is abandoned, so it must not also end the session
    /// while there is something to discard.
    #[test]
    fn escape_clears_a_typed_line_without_quitting() {
        let mut session = Session::new("none");
        for c in "half a thought".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert!(session.input().is_empty(), "the input was not cleared");
        assert!(
            !session.is_quitting(),
            "clearing the input ended the session"
        );
    }

    /// A session editing vi's way. The choice is recorded nowhere, since a session that does not
    /// persist reads no file and writes none.
    fn editing_vis_way() -> Session {
        let mut session = Session::new("none");
        session.choose_editing(crate::vim::Editing::Vi);
        session
    }

    /// The key that takes the letters as instructions must not also throw the paragraph away. Both
    /// are one press of Escape in the box everybody has, so somebody switching to vi editing would
    /// otherwise lose a prompt every time they reached for NORMAL mode, and the only way to find out
    /// would be to have already lost one.
    #[test]
    fn escape_enters_normal_mode_without_discarding_the_line() {
        let mut session = editing_vis_way();
        for c in "half a thought".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);

        assert_eq!(
            session.input(),
            "half a thought",
            "entering NORMAL mode discarded the line"
        );
        assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal));
    }

    /// The other spelling of the same request. A terminal asked to disambiguate reports this chord
    /// where another sends the byte Escape already is, and which arrives is the terminal's choice
    /// rather than the person's: a binding answering one spelling reads as broken on the other machine.
    #[test]
    fn either_spelling_of_escape_enters_normal_mode() {
        let mut session = editing_vis_way();
        for c in "half a thought".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(handle_key(&mut session, ctrl('[')), Action::Redraw);

        assert_eq!(session.input(), "half a thought");
        assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal));
    }

    /// The chord means nothing for the box everybody has, and a key that quietly did something there
    /// would be one nobody could account for. Ctrl chords are otherwise ignored rather than typed, so
    /// what this pins is that nothing was typed either.
    #[test]
    fn the_chord_that_enters_normal_mode_does_nothing_to_the_ordinary_box() {
        let mut session = Session::new("none");
        for c in "half a thought".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(handle_key(&mut session, ctrl('[')), Action::None);

        assert_eq!(session.input(), "half a thought");
        assert_eq!(session.vi_mode(), None);
    }

    /// Escape stops the turn in flight before it means anything else, which is the one thing every
    /// press of it has always done first. Entering a mode instead would leave the key that stops a
    /// runaway turn doing nothing a person could see.
    #[test]
    fn escape_still_stops_a_turn_before_it_enters_normal_mode() {
        let mut session = editing_vis_way();
        type_line(&mut session, "a question");
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Cancel);
        assert_eq!(
            session.vi_mode(),
            Some(crate::vim::Mode::Insert),
            "the press that stopped the turn also changed the mode"
        );
    }

    /// A session in NORMAL mode over a paragraph, which is what gives the row keys somewhere to go.
    fn in_normal_mode(line: &str) -> Session {
        let mut session = editing_vis_way();
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Esc));
        session
    }

    /// `k` and `j` walk the rows of a paragraph, which is what the arrows do there. Answered by
    /// translating the letter rather than by a second copy of that ladder, so the two cannot drift.
    #[test]
    fn the_row_keys_walk_a_paragraph() {
        // Escape leaves the caret on the last character of the second row, which is where the walk
        // upwards starts from.
        let mut session = in_normal_mode("one\ntwo");
        assert_eq!(session.caret(), 6);

        handle_key(&mut session, key(KeyCode::Char('k')));
        assert_eq!(session.caret(), 2, "k did not reach the row above");

        handle_key(&mut session, key(KeyCode::Char('j')));
        assert_eq!(session.caret(), 6, "j did not reach the row below");
    }

    /// Past the ends of the input the row keys reach the prompt history, which is what the arrows reach
    /// there. A letter that only ever moved the caret would leave the prompt somebody wants most
    /// unreachable from the mode they are in.
    #[test]
    fn the_row_keys_reach_the_prompt_history_at_the_ends_of_the_input() {
        let mut session = editing_vis_way();
        type_line(&mut session, "an earlier prompt");
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("an answer", Vec::new(), 0);
        handle_key(&mut session, key(KeyCode::Esc));

        handle_key(&mut session, key(KeyCode::Char('k')));

        assert_eq!(
            session.input(),
            "an earlier prompt",
            "k did not reach the history from an empty line"
        );
    }

    /// `/` searches in vi, and the prompts already sent are the only thing here to search, so it asks
    /// the question Ctrl-R asks. Nothing is typed into the line: the mode is not typing.
    #[test]
    fn a_slash_opens_the_search_over_earlier_prompts() {
        let mut session = editing_vis_way();
        type_line(&mut session, "an earlier prompt");
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("an answer", Vec::new(), 0);
        handle_key(&mut session, key(KeyCode::Esc));

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Char('/'))),
            Action::Redraw
        );

        assert!(session.searching_history(), "the search did not open");
        assert!(session.input().is_empty(), "the slash was typed");
    }

    /// In INSERT mode every one of those letters is a letter, which is the whole of what the mode
    /// means. A `/` typed there is the start of a command, and a `j` is a `j`.
    #[test]
    fn the_letters_that_spell_keys_are_typed_in_insert_mode() {
        let mut session = editing_vis_way();
        type_line(&mut session, "reject/j");

        assert_eq!(session.input(), "reject/j");
        assert!(
            !session.searching_history(),
            "a typed slash opened a search"
        );
    }

    /// The prompt has to be able to leave the box. Ctrl-G is the one key that says so, and a
    /// control combination that falls through to the catch-all would type a stray 'g' instead.
    #[test]
    fn ctrl_g_asks_for_the_editor() {
        let mut session = Session::new("none");
        for c in "half a thought".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(handle_key(&mut session, ctrl('g')), Action::Edit);
        assert_eq!(
            session.input(),
            "half a thought",
            "the line was disturbed before the editor saw it"
        );
    }

    /// Handing the terminal to an editor mid-turn would take the screen away from the turn that
    /// is drawing on it, and the line the editor returned would be waiting for a box that has
    /// moved on. The keys the user can still use while a turn runs do not include this one.
    #[test]
    fn the_editor_key_does_nothing_while_a_turn_runs() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('x')));
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        assert_eq!(
            handle_key_while_working(&mut session, ctrl('g')),
            Action::None
        );
    }

    /// One key both ways, read against the line rather than remembered: a line to put away is put
    /// away, and an empty box is where the one put away earlier is wanted. A control combination
    /// falling through to the catch-all would type a stray 's' into the prompt instead.
    #[test]
    fn ctrl_s_puts_the_line_away_and_brings_it_back() {
        let mut session = Session::new("none");
        for c in "half a thought".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(handle_key(&mut session, ctrl('s')), Action::Redraw);
        assert_eq!(session.input(), "", "the line stayed in the box");

        assert_eq!(handle_key(&mut session, ctrl('s')), Action::Redraw);
        assert_eq!(session.input(), "half a thought");
    }

    /// The key writes a line and sends nothing, and sending is the whole of what a running turn
    /// refuses. Mid-turn is also when it is most wanted: a person watching a turn go wrong has
    /// somewhere to put the half-written thought a better one just replaced.
    #[test]
    fn the_stash_key_works_while_a_turn_runs() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('x')));
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        for c in "the next thing".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key_while_working(&mut session, ctrl('s')),
            Action::Redraw
        );
        assert_eq!(session.input(), "");

        handle_key_while_working(&mut session, ctrl('s'));
        assert_eq!(session.input(), "the next thing");
    }

    /// A settings file naming a key the box already answers does not get it. The arms that read a
    /// configured chord sit above the one that types, so a letter handed to an action would be a
    /// letter that can no longer be written.
    #[test]
    fn a_settings_file_cannot_take_a_letter_away_from_typing() {
        let mut session = Session::new("none");
        let mut custom = std::collections::BTreeMap::new();
        custom.insert("stash".to_string(), "x".to_string());
        custom.insert("trail".to_string(), "?".to_string());
        session.adopt_keybindings(&custom);

        for c in "exit".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            session.input(),
            "exit",
            "a letter an action was given stopped typing"
        );

        session.clear_input();
        handle_key(&mut session, key(KeyCode::Char('?')));
        assert!(
            matches!(session.offered(), crate::state::Offered::Shortcuts),
            "the key list stopped answering the key that opens it"
        );
    }

    /// Customizable keybindings route actions to the configured chord, and the default chord
    /// is ignored once remapped.
    #[test]
    fn custom_keybindings_route_actions_and_old_chords_are_ignored() {
        let mut session = Session::new("none");
        let mut custom = std::collections::BTreeMap::new();
        custom.insert("stash".to_string(), "alt-s".to_string());
        custom.insert("scroller".to_string(), "alt-o".to_string());
        session.adopt_keybindings(&custom);

        for c in "custom key test".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        // Default ctrl-s should be ignored for stashing now
        handle_key(&mut session, ctrl('s'));
        assert_eq!(
            session.input(),
            "custom key test",
            "default chord still stashed"
        );

        // Custom alt-s stashes
        let alt_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT);
        assert_eq!(handle_key(&mut session, alt_s), Action::Redraw);
        assert_eq!(session.input(), "", "custom chord did not stash");

        // Custom alt-s restores the line
        handle_key(&mut session, alt_s);
        assert_eq!(
            session.input(),
            "custom key test",
            "custom chord did not restore stash"
        );

        // Default ctrl-o should not open scroller
        handle_key(&mut session, ctrl('o'));
        assert!(
            !session.scrolling(),
            "default ctrl-o opened scroller when remapped"
        );

        // Custom alt-o opens scroller
        let alt_o = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::ALT);
        handle_key(&mut session, alt_o);
        assert!(session.scrolling(), "custom alt-o did not open scroller");

        // Custom alt-o closes scroller
        handle_key(&mut session, alt_o);
        assert!(!session.scrolling(), "custom alt-o did not close scroller");
    }

    /// Custom keybindings function properly mid-turn in handle_key_while_working.
    #[test]
    fn custom_keybindings_work_while_a_turn_runs() {
        let mut session = Session::new("none");
        let mut custom = std::collections::BTreeMap::new();
        custom.insert("stash".to_string(), "alt-s".to_string());
        custom.insert("scroller".to_string(), "alt-o".to_string());
        session.adopt_keybindings(&custom);

        handle_key(&mut session, key(KeyCode::Char('x')));
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        for c in "working input".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }

        // Default ctrl-s does not stash
        handle_key_while_working(&mut session, ctrl('s'));
        assert_eq!(session.input(), "working input");

        // Custom alt-s stashes mid-turn
        let alt_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT);
        assert_eq!(
            handle_key_while_working(&mut session, alt_s),
            Action::Redraw
        );
        assert_eq!(session.input(), "");

        // Custom alt-o opens scroller mid-turn
        let alt_o = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::ALT);
        assert_eq!(
            handle_key_while_working(&mut session, alt_o),
            Action::Redraw
        );
        assert!(session.scrolling());
    }

    /// Inside the prompt search every character narrows the list, so a moved chord has to be read
    /// before the arm that types: bound onto a letter it would otherwise open the search from the
    /// box and then be typed into it.
    #[test]
    fn a_moved_chord_is_read_inside_the_search_it_opened() {
        let alt = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT);
        let mut session = Session::new("none");
        let mut moved = std::collections::BTreeMap::new();
        moved.insert("history".to_string(), "alt-r".to_string());
        moved.insert("stash".to_string(), "alt-s".to_string());
        session.adopt_keybindings(&moved);

        type_line(&mut session, "run the tests");
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("ok", Vec::new(), 0);

        handle_key(&mut session, alt('r'));
        assert!(session.searching_history(), "the chord did not open it");

        handle_key(&mut session, alt('s'));
        let search = session.history_search().expect("the search closed");
        assert!(search.here(), "the chord did not narrow the scope");
        assert_eq!(search.needle(), "", "the chord was typed into the needle");

        handle_key(&mut session, alt('s'));
        let search = session.history_search().expect("the search closed");
        assert!(!search.here(), "one more press did not widen it again");

        // The chords they were moved off of do nothing in here, rather than narrowing the list to
        // prompts holding an `s` or an `r`.
        for old in [ctrl('s'), ctrl('r')] {
            handle_key(&mut session, old);
        }
        let search = session
            .history_search()
            .expect("an old chord closed the search");
        assert_eq!(
            search.needle(),
            "",
            "an old chord was typed into the needle"
        );

        handle_key(&mut session, alt('r'));
        assert!(!session.searching_history(), "the chord did not close it");
    }

    /// In Vi mode, `/` in NORMAL mode translates to the configured history chord rather than
    /// hardcoded ctrl-r, opening the prompt history search.
    #[test]
    fn vi_mode_search_prompts_uses_configured_history_chord() {
        let mut session = Session::new("none");
        let mut custom = std::collections::BTreeMap::new();
        custom.insert("history".to_string(), "alt-r".to_string());
        session.adopt_keybindings(&custom);
        session.adopt_editing(Some("vim"));

        type_line(&mut session, "first prompt");
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("ok", Vec::new(), 0);

        // Enter vi normal mode via Escape
        handle_key(&mut session, key(KeyCode::Esc));
        assert!(session.vi_normal());

        // In Vi NORMAL mode, `/` opens prompt history search
        handle_key(&mut session, key(KeyCode::Char('/')));
        assert!(
            session.searching_history(),
            "vi / did not open history search with custom chord"
        );
    }

    /// A configured chord that collides with a readline line-editing key (such as ctrl-u or alt-b)
    /// executes the configured action rather than being swallowed by line editing.
    #[test]
    fn configured_keybinding_overrides_readline_editing() {
        let mut session = Session::new("none");
        let mut custom = std::collections::BTreeMap::new();
        // ctrl-u is readline delete_to_line_start; bound to stash here
        custom.insert("stash".to_string(), "ctrl-u".to_string());
        session.adopt_keybindings(&custom);

        type_line(&mut session, "line to stash");
        let ctrl_u = ctrl('u');
        assert_eq!(handle_key(&mut session, ctrl_u), Action::Redraw);
        assert_eq!(session.input(), "", "ctrl-u did not stash the line");
        assert_eq!(session.stashed(), Some("line to stash"));

        // Unbound readline keys still perform editing normally
        type_line(&mut session, "hello world");
        // ctrl-w deletes previous word
        handle_key(&mut session, ctrl('w'));
        assert_eq!(session.input(), "hello ");
    }

    /// Escape means "stop this" before it means anything else, so a turn in flight is cancelled
    /// rather than the input being cleared.
    #[test]
    fn escape_cancels_a_turn_in_flight() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('x')));
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Cancel);
        assert!(!session.is_quitting(), "cancelling ended the session");
    }

    /// Escape only ever stops. Ctrl-C is the key the loops read against what is happening, so it
    /// is told apart from a plain `c` and from every other press before either meaning is
    /// reached.
    #[test]
    fn escape_only_stops_and_ctrl_c_is_read_against_what_is_happening() {
        assert!(wants_cancel(key(KeyCode::Esc)));
        assert!(
            !wants_cancel(ctrl('c')),
            "ctrl-c is more than a request to stop"
        );

        assert!(is_ctrl_c(ctrl('c')));
        assert!(!is_ctrl_c(key(KeyCode::Esc)));
        assert!(!is_ctrl_c(key(KeyCode::Char('c'))));
        assert!(!is_ctrl_c(key(KeyCode::Enter)));
        assert!(!is_ctrl_c(key(KeyCode::Up)));
    }

    /// And a second press does the same thing again rather than leaving. A key pressed twice in
    /// a row should not mean two different things, least of all when the second is the exit.
    #[test]
    fn escape_twice_clears_and_stays() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('x')));

        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert!(!session.is_quitting());
    }

    /// Ctrl-D only quits on an empty line, matching shell behaviour, so it cannot discard
    /// a half-typed prompt.
    #[test]
    fn ctrl_d_quits_only_when_the_line_is_empty() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('x')));
        assert_ne!(handle_key(&mut session, ctrl('d')), Action::Quit);
        assert!(!session.is_quitting());

        handle_key(&mut session, key(KeyCode::Backspace));
        assert!(
            session.input().is_empty(),
            "backspace did not clear the line"
        );
        assert_eq!(handle_key(&mut session, ctrl('d')), Action::Quit);
    }

    /// Type a line, one key at a time, the way a user does.
    fn type_line(session: &mut Session, line: &str) {
        for c in line.chars() {
            handle_key(session, key(KeyCode::Char(c)));
        }
    }

    /// `!` on an empty line is the mode rather than a character, which is what makes the rest of
    /// the line the command exactly as typed.
    #[test]
    fn a_bang_on_an_empty_line_enters_shell_mode_without_being_typed() {
        let mut session = Session::new("none");
        type_line(&mut session, "!");

        assert!(session.shell, "the mode did not turn on");
        assert!(
            session.input().is_empty(),
            "the marker was typed into the line"
        );
    }

    /// The same for `?`: it puts the list of keys up rather than typing a character, so nothing is
    /// left in the box to delete and pressing it again takes the list down.
    #[test]
    fn a_question_mark_on_an_empty_line_toggles_the_list_without_being_typed() {
        let mut session = Session::new("none");

        type_line(&mut session, "?");
        assert!(session.shortcuts, "the list did not come up");
        assert!(session.input().is_empty(), "the marker was typed");

        type_line(&mut session, "?");
        assert!(!session.shortcuts, "the list did not go down again");
        assert!(session.input().is_empty(), "the marker was typed");
    }

    /// Punctuation everywhere but the head of the line. Somebody writing "what is this?" is asking
    /// the model a question, not asking for the keys.
    #[test]
    fn a_question_mark_inside_a_sentence_is_punctuation() {
        let mut session = Session::new("none");
        type_line(&mut session, "what is this?");

        assert!(!session.shortcuts, "the list came up mid-sentence");
        assert_eq!(session.input(), "what is this?");
    }

    /// Typing again means the reading is over, so the list goes rather than sitting under a line it
    /// says nothing about.
    #[test]
    fn typing_takes_the_list_down() {
        let mut session = Session::new("none");
        type_line(&mut session, "?");
        type_line(&mut session, "a");

        assert!(!session.shortcuts, "the list stayed up");
        assert_eq!(session.input(), "a", "the character was not typed");
    }

    /// Escape takes down whatever is up, and with an empty box the list is the only thing there is.
    #[test]
    fn escape_takes_the_list_down() {
        let mut session = Session::new("none");
        type_line(&mut session, "?");

        assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
        assert!(!session.shortcuts, "the list stayed up");
    }

    /// The list is documentation, and a turn in flight refuses sending and nothing else. The key
    /// used to set the flag with the list refused a place to be drawn, so the press did nothing on
    /// screen and the list came up unasked when the turn ended, attached to no press at all.
    #[test]
    fn a_question_mark_lists_the_keys_while_a_turn_runs() {
        let mut session = Session::new("none");
        type_line(&mut session, "anything");
        session.submit().expect("the prompt is sent");

        handle_key_while_working(&mut session, key(KeyCode::Char('?')));
        assert!(session.shortcuts, "the list did not come up");
        assert!(session.input().is_empty(), "the marker was typed");
        assert_eq!(
            session.offered(),
            crate::state::Offered::Shortcuts,
            "the list had nowhere to be drawn"
        );

        handle_key_while_working(&mut session, key(KeyCode::Char('?')));
        assert!(!session.shortcuts, "the list did not go down again");
    }

    /// A line being composed mid-turn is one Enter queues, and finishing it is machinery for
    /// something about to be sent. The list is not, which is why it is the one thing offered there.
    #[test]
    fn nothing_is_offered_for_completion_while_a_turn_runs() {
        let mut session = Session::new("none");
        type_line(&mut session, "anything");
        session.submit().expect("the prompt is sent");

        handle_key_while_working(&mut session, key(KeyCode::Char('/')));
        assert_eq!(session.offered(), crate::state::Offered::Nothing);
    }

    /// In shell mode a `?` is a glob for the shell to expand, so it is typed like any other
    /// character rather than putting the list up.
    #[test]
    fn a_question_mark_in_shell_mode_is_a_glob() {
        let mut session = Session::new("none");
        type_line(&mut session, "!");
        type_line(&mut session, "?");

        assert!(!session.shortcuts, "the list came up over a command");
        assert_eq!(session.input(), "?");
    }

    #[test]
    fn enter_in_shell_mode_runs_the_line_rather_than_prompting() {
        let mut session = Session::new("none");
        type_line(&mut session, "!ls -la");

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Run("ls -la".to_string())
        );
    }

    /// The mode lasts one command. Leaving it on would send the next thing typed to a shell, which
    /// is the sort of surprise that ends up running a sentence.
    #[test]
    fn running_a_command_leaves_shell_mode() {
        let mut session = Session::new("none");
        type_line(&mut session, "!pwd");
        handle_key(&mut session, key(KeyCode::Enter));

        assert!(!session.shell, "the mode stayed on after the command ran");
        type_line(&mut session, "what did that print");
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what did that print".to_string())
        );
    }

    /// A slash command is a word this program knows, but in shell mode the line is a command line:
    /// `/status` could be a program somebody has, and `!` is how they said which they meant.
    #[test]
    fn a_slash_command_in_shell_mode_is_a_command_line() {
        let mut session = Session::new("none");
        type_line(&mut session, "!/status");

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Run("/status".to_string())
        );
    }

    /// Nothing is offered in shell mode: `/usr/bin/env` is a path and an argument with an `@` in it
    /// is an argument, so completing either would rewrite the line under someone typing it.
    #[test]
    fn shell_mode_offers_no_completions() {
        let mut session = Session::new("none");
        type_line(&mut session, "!/st");

        assert!(!session.is_completing(), "a completion was offered");
        // Enter must therefore run it rather than accepting a highlighted row.
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Run("/st".to_string())
        );
    }

    /// The marker looks like a character, so deleting back past it has to leave the mode. Otherwise
    /// the only way out is clearing the whole line, which nobody would guess.
    #[test]
    fn backspacing_past_the_marker_leaves_shell_mode() {
        let mut session = Session::new("none");
        type_line(&mut session, "!ls");

        handle_key(&mut session, key(KeyCode::Backspace));
        handle_key(&mut session, key(KeyCode::Backspace));
        assert!(session.shell, "the mode was left while text remained");

        handle_key(&mut session, key(KeyCode::Backspace));
        assert!(!session.shell, "the mode outlived the marker");
    }

    /// The marker sits before the caret, not before the line, so Backspace with the caret moved to
    /// the start is the press that deletes it. What follows was typed on purpose and stays: the
    /// mode is what was deleted, and the words become an ordinary prompt.
    #[test]
    fn backspacing_at_the_start_leaves_the_mode_and_keeps_the_line() {
        let mut session = Session::new("none");
        type_line(&mut session, "!ls -la");

        handle_key(&mut session, key(KeyCode::Home));
        handle_key(&mut session, key(KeyCode::Backspace));

        assert!(!session.shell, "the mode outlived the marker");
        assert_eq!(session.input(), "ls -la", "the line went with the marker");
    }

    /// Escape abandons the line, and the mode is part of the line: leaving it armed would send the
    /// next thing typed to a shell.
    #[test]
    fn escape_leaves_shell_mode() {
        let mut session = Session::new("none");
        type_line(&mut session, "!rm -rf /");

        handle_key(&mut session, key(KeyCode::Esc));

        assert!(!session.shell, "the mode survived being cancelled");
        assert!(session.input().is_empty());
    }

    /// The mode is part of the line even when nothing was typed behind the marker, so Escape
    /// leaves it rather than ending the session. Backspace at the same caret already does, and
    /// having the two keys disagree is how a press meant to back out of a mode loses the session.
    #[test]
    fn escape_leaves_shell_mode_armed_on_an_empty_line() {
        let mut session = Session::new("none");
        type_line(&mut session, "!");
        assert!(session.shell, "the mode was never armed");

        let action = handle_key(&mut session, key(KeyCode::Esc));

        assert_eq!(action, Action::Redraw, "escape ended the session");
        assert!(!session.shell, "the mode survived being cancelled");
    }

    /// A `!` mid-sentence is punctuation. Treating it as the mode would make "no way!" a command.
    #[test]
    fn a_bang_inside_a_line_is_an_ordinary_character() {
        let mut session = Session::new("none");
        type_line(&mut session, "no way!");

        assert!(!session.shell);
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("no way!".to_string())
        );
    }

    /// Shell history expansion is the shell's business, and a `!` inside a command belongs to it.
    #[test]
    fn a_bang_inside_a_command_stays_in_the_command() {
        let mut session = Session::new("none");
        type_line(&mut session, "!echo hi!");

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Run("echo hi!".to_string())
        );
    }

    /// A prompt that comes back after a cancelled turn is English, not a command line. The mode was
    /// enterable mid-turn, so `!` then escape then enter ran the user's own sentence: "rm the old
    /// builds" is a reasonable prompt, and a shell reads it as an instruction.
    #[test]
    fn a_prompt_restored_after_a_cancelled_turn_is_never_run_as_a_command() {
        let mut session = Session::new("none");
        type_line(&mut session, "rm the old builds");
        session.submit().expect("the prompt is sent");

        // Mid-turn, which used to be the moment the mode could be armed unseen.
        handle_key_while_working(&mut session, key(KeyCode::Char('!')));
        session.restore("rm the old builds".to_string());
        session.clear_input();

        assert!(
            !session.shell,
            "the mode survived a cancelled turn, so the prompt was armed as a command"
        );
        type_line(&mut session, "rm the old builds");
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("rm the old builds".to_string()),
            "the user's sentence was about to be run by a shell"
        );
    }

    /// Whatever else a cancelled turn does with the line, it must not come back armed: the text is a
    /// prompt, and the mode changes what Enter does to it.
    #[test]
    fn restoring_a_prompt_leaves_shell_mode() {
        let mut session = Session::new("none");
        type_line(&mut session, "some prompt");
        session.submit().expect("the prompt is sent");
        session.shell = true;

        session.restore("some prompt".to_string());

        assert!(!session.shell);
    }

    /// The mode belongs to the idle prompt. Entering it mid-turn arms a line the user cannot act on
    /// until the turn ends, which is the wrong moment to find out what Enter now does.
    ///
    /// The character is still typed, because words typed during a turn are kept: what is refused is
    /// the mode, not the keystroke.
    #[test]
    fn shell_mode_cannot_be_entered_while_a_turn_runs() {
        let mut session = Session::new("none");
        type_line(&mut session, "anything");
        session.submit().expect("the prompt is sent");

        handle_key_while_working(&mut session, key(KeyCode::Char('!')));

        assert!(!session.shell, "the mode turned on mid-turn");
        assert_eq!(session.input(), "!", "the keystroke was dropped");
    }

    /// Enter on a bare marker would run an empty line, which a shell accepts and which would put a
    /// pointless entry in the transcript.
    #[test]
    fn enter_on_an_empty_shell_line_does_nothing() {
        let mut session = Session::new("none");
        type_line(&mut session, "!");

        assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::None);
        assert!(session.transcript.is_empty());
    }

    /// The word is a command, not a prompt: it must end the session rather than reach the planner.
    #[test]
    fn typing_the_exit_command_quits() {
        let mut session = Session::new("none");
        for c in EXIT_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::Quit);
        assert!(session.is_quitting());
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
    }

    /// Only the bare word, so a prompt that merely mentions it is still a prompt.
    #[test]
    fn a_prompt_containing_the_exit_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "what does /exit do".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what does /exit do".to_string())
        );
        assert!(!session.is_quitting());
    }

    /// A command, not a prompt: asking for the picker must not also ask the planner about models.
    #[test]
    fn typing_the_model_command_opens_the_picker() {
        let mut session = Session::new("none");
        for c in MODEL_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::ChooseModel
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
        assert_eq!(session.status, Status::Idle, "a turn began");
    }

    /// Only the bare word. "/model is slow today" is a thing to say to the planner.
    #[test]
    fn a_prompt_containing_the_model_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "why is /model slow".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("why is /model slow".to_string())
        );
    }

    /// A command, not a prompt: asking for the picker must not also ask the planner about themes.
    #[test]
    fn typing_the_theme_command_opens_the_picker() {
        let mut session = Session::new("none");
        for c in THEME_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::ChooseTheme
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
    }

    /// A command, not a prompt: asking how hard to think must not also ask the planner about it.
    #[test]
    fn typing_the_effort_command_opens_the_picker() {
        let mut session = Session::new("none");
        for c in EFFORT_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::ChooseEffort
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
        assert_eq!(session.status, Status::Idle, "a turn began");
    }

    /// A command, not a prompt: asking how the box edits must not also ask the planner about it.
    #[test]
    fn typing_the_config_command_opens_the_panel() {
        let mut session = Session::new("none");
        for c in CONFIG_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::ChooseEditing
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
        assert_eq!(session.status, Status::Idle, "a turn began");
    }

    /// Only the bare word. "what does /config change" is a thing to say to the planner.
    #[test]
    fn a_prompt_containing_the_config_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        type_line(&mut session, "what does /config change");

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what does /config change".to_string())
        );
    }

    /// The command is on the list `/` offers, since a panel nobody can find is a panel nobody has.
    #[test]
    fn the_config_command_is_offered_like_every_other() {
        assert!(
            commands()
                .iter()
                .any(|command| command.name == CONFIG_COMMAND),
            "the command is not on the list"
        );
        assert!(
            completions("/con")
                .iter()
                .any(|command| command.name == CONFIG_COMMAND),
            "the command does not complete"
        );
    }

    /// Only the bare word. "why is /effort high" is a thing to say to the planner.
    #[test]
    fn a_prompt_containing_the_effort_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "why is /effort high".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("why is /effort high".to_string())
        );
    }

    /// `/efforts` is not `/effort`: the whole word must match.
    #[test]
    fn a_longer_word_starting_with_effort_is_a_prompt() {
        let mut session = Session::new("none");
        for c in "/efforts".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("/efforts".to_string())
        );
    }

    /// Naming a level on the line takes it without opening the picker.
    #[test]
    fn the_effort_command_carries_its_level() {
        let mut session = Session::new("none");
        for c in "/effort xhigh".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::SetEffort("xhigh".to_string())
        );
    }

    /// A level the user chose must be the level the turn asks for, or the choice is decoration.
    #[test]
    fn a_chosen_level_reaches_the_session() {
        let mut session = Session::new("none");
        assert_eq!(session.effort(), None);

        set_effort(&mut session, "max");
        assert_eq!(
            session.effort(),
            Some(bravebot_aichat::protocol::Effort::Max)
        );
    }

    /// Asking for no level puts the session back to sending none, so a first pick is not
    /// permanent.
    #[test]
    fn asking_for_no_level_puts_the_session_back_to_sending_none() {
        let mut session = Session::new("none");
        set_effort(&mut session, "max");

        session.choose_effort(None);
        assert_eq!(session.effort(), None);
    }

    /// A word this program does not know must change nothing and say so: silently ignoring it
    /// leaves somebody believing they asked for something.
    #[test]
    fn a_level_this_program_does_not_know_changes_nothing_and_says_so() {
        let mut session = Session::new("none");
        set_effort(&mut session, "high");
        set_effort(&mut session, "highest");

        assert_eq!(
            session.effort(),
            Some(bravebot_aichat::protocol::Effort::High),
            "an unknown word replaced the level in force"
        );
        assert!(
            session
                .transcript
                .iter()
                .any(|entry| entry.text.contains("highest")),
            "nothing was said about the word"
        );
    }

    /// A level is sent only where the roster says the model reads one. Sending it anywhere else is
    /// a field dropped at the far end while the session reports it as in force.
    #[test]
    fn a_level_is_withheld_from_a_model_that_reads_none() {
        let mut session = Session::new("none");
        set_effort(&mut session, "max");
        assert_eq!(
            session.effort_in_force(),
            Some(bravebot_aichat::protocol::Effort::Max)
        );

        session.note_model_reads_effort(false);
        assert_eq!(
            session.effort_in_force(),
            None,
            "a dropped field was still sent"
        );
    }

    /// The choice survives the model that cannot use it: somebody may be about to change model,
    /// and discarding it would make the two commands depend on the order they were typed in.
    #[test]
    fn a_level_a_model_cannot_use_is_kept_rather_than_forgotten() {
        let mut session = Session::new("none");
        session.note_model_reads_effort(false);
        set_effort(&mut session, "max");

        assert_eq!(
            session.effort(),
            Some(bravebot_aichat::protocol::Effort::Max),
            "the choice was thrown away"
        );
        session.note_model_reads_effort(true);
        assert_eq!(
            session.effort_in_force(),
            Some(bravebot_aichat::protocol::Effort::Max),
            "the choice did not come back with a model that reads it"
        );
    }

    /// Picking a level a model cannot use says so. Taking it silently would leave somebody
    /// believing every later turn was thinking harder.
    #[test]
    fn asking_for_a_level_a_model_cannot_use_says_so() {
        let mut session = Session::new("none");
        session.note_model_reads_effort(false);
        set_effort(&mut session, "max");

        assert!(
            session
                .transcript
                .iter()
                .any(|entry| entry.text.contains("reads no effort level")),
            "nothing was said about the model reading none"
        );
    }

    /// A model no listing described is not a model stated to read nothing, so the level still goes
    /// out: a name from a settings file, and a listing nobody could fetch, both land here.
    #[test]
    fn a_model_the_listing_did_not_describe_still_takes_a_level() {
        let described = vec![bravebot_aichat::models::Model {
            key: "openrouter/reasons-only".to_string(),
            display_name: "reasons-only".to_string(),
            premium: false,
            provider: Some("OpenRouter".to_string()),
            conversation_tokens: None,
            reads_effort: false,
        }];

        assert!(reads_effort(&described, Some("openrouter/not-listed")));
        assert!(
            reads_effort(&described, None),
            "automatic was withheld a level"
        );
        assert!(!reads_effort(&described, Some("openrouter/reasons-only")));
    }

    /// A longer word that only starts with the command is a prompt, not the command.
    #[test]
    fn a_prompt_containing_the_theme_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "what does /theme do".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what does /theme do".to_string())
        );
    }

    /// `/themes` is not `/theme`: the whole word must match.
    #[test]
    fn a_longer_word_starting_with_theme_is_a_prompt() {
        let mut session = Session::new("none");
        for c in "/themes".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("/themes".to_string())
        );
    }

    /// Naming a theme on the line applies it without opening the picker.
    #[test]
    fn the_theme_command_carries_its_name() {
        let mut session = Session::new("none");
        for c in "/theme nord".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::SetTheme("nord".to_string())
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
    }

    /// The argument is the point of this one, so it must arrive with the action.
    #[test]
    fn the_add_dir_command_carries_its_directory() {
        let mut session = Session::new("none");
        for c in "/add-dir ~/notes".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::AddDirectory("~/notes".to_string())
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
    }

    /// The argument is the point of this one too, so it must arrive with the action.
    #[test]
    fn the_cd_command_carries_its_directory() {
        let mut session = Session::new("none");
        for c in "/cd ~/projects/other".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::ChangeDirectory("~/projects/other".to_string())
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
    }

    /// With no argument there is nowhere to move to, and the loop says so rather than moving to
    /// the home directory the way a shell would.
    #[test]
    fn the_bare_cd_command_is_still_the_command() {
        let mut session = Session::new("none");
        for c in CD_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::ChangeDirectory(String::new())
        );
    }

    /// A longer word beginning with it is a prompt, or "/cdn caching is slow" would try to work in
    /// a directory called "n caching is slow".
    #[test]
    fn a_longer_word_starting_with_cd_is_a_prompt() {
        let mut session = Session::new("none");
        for c in "/cdn caching is slow".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("/cdn caching is slow".to_string())
        );
    }

    /// Asking about the command has to stay a question.
    #[test]
    fn a_prompt_containing_the_cd_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "what does /cd do".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what does /cd do".to_string())
        );
    }

    /// With no argument there is nothing to open, and the loop says so rather than doing nothing.
    #[test]
    fn the_bare_add_dir_command_is_still_the_command() {
        let mut session = Session::new("none");
        for c in ADD_DIR_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::AddDirectory(String::new())
        );
    }

    /// A longer word beginning with the command is not the command, or "/add-dirs are useful"
    /// would open a directory called "s are useful".
    #[test]
    fn a_longer_word_starting_with_the_command_is_a_prompt() {
        let mut session = Session::new("none");
        for c in "/add-dirs are confusing".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("/add-dirs are confusing".to_string())
        );
    }

    /// And a sentence that merely mentions it is a thing to say to the planner.
    #[test]
    fn a_prompt_containing_the_add_dir_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "what does /add-dir do".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what does /add-dir do".to_string())
        );
    }

    /// A command, not a prompt: it must not reach the planner as a request to clear something.
    #[test]
    fn typing_the_clear_command_starts_a_new_session() {
        let mut session = Session::new("none");
        for c in CLEAR_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::Clear);
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
    }

    /// Typed before submitting, so the word never reaches the planner as a prompt: a session
    /// asking to be shortened must not answer by talking about shortening itself.
    #[test]
    fn the_compact_command_asks_for_a_summary_rather_than_being_sent() {
        let mut session = Session::new("none");
        for c in COMPACT_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Compact
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
    }

    /// Only the bare word, so asking the planner about compacting something still asks it.
    #[test]
    fn a_prompt_containing_the_compact_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "how does /compact work".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("how does /compact work".to_string())
        );
    }

    /// The question is what the command acts on, so the whole of what was typed after the word
    /// has to arrive, spaces and punctuation included.
    #[test]
    fn the_btw_command_carries_its_question() {
        let mut session = Session::new("none");
        for c in "/btw why is the parser recursive?".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Aside("why is the parser recursive?".to_string())
        );
    }

    /// With no question there is nothing to ask, and the loop says so rather than spending a
    /// request to find out.
    #[test]
    fn the_bare_btw_command_is_still_the_command() {
        let mut session = Session::new("none");
        for c in BTW_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Aside(String::new())
        );
    }

    /// A sentence mentioning it is a thing to say to the planner, and the word this one claims is
    /// short enough that it will be said.
    #[test]
    fn a_prompt_containing_the_btw_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "what does /btw do".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what does /btw do".to_string())
        );
    }

    #[test]
    fn a_longer_word_starting_with_btw_is_a_prompt() {
        let mut session = Session::new("none");
        for c in "/btwice is not a word".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("/btwice is not a word".to_string())
        );
    }

    /// Neither half of an aside is in the conversation, so an answer that arrives adds nothing to
    /// the turn's own lines: a line about it there is one a reader takes the planner to have had,
    /// and the planner has read neither the question nor the answer. The row the answer becomes,
    /// and the hint line that counts it, are where an aside is said to have happened.
    #[test]
    fn answering_a_question_beside_the_work_leaves_the_transcript_alone() {
        let mut session = Session::new("none");
        session.begin_aside();
        let before = session.transcript.len();

        aside_answered(
            &mut session,
            "why is the parser recursive?".to_string(),
            bravebot_agent::aside::Answered {
                shown: "because the grammar nests".to_string(),
                kept: Some("because the grammar nests".to_string()),
                usage: Default::default(),
            },
        );

        assert_eq!(
            session.watched_aside().and_then(|a| a.answer.as_deref()),
            Some("because the grammar nests"),
            "the answer did not reach the row that is the only place it is read"
        );
        let added: Vec<&str> = session.transcript[before..]
            .iter()
            .map(|entry| entry.text.as_str())
            .collect();
        assert!(
            added.is_empty(),
            "an answered aside put a line among the turn's own: {added:?}"
        );
    }

    /// The tail is where the planner's own half-written reply is drawn, so an answer arriving
    /// there reads as the planner writing something it has not read. There is one model writing
    /// at a time in that place, and an aside is not the one.
    #[test]
    fn an_answer_being_written_beside_the_work_is_not_drawn_over_the_turn() {
        let mut session = Session::new("none");
        session.streaming("the turn was saying this");
        session.begin_aside();
        let before = session.transcript.len();

        aside_reported(
            &mut session,
            crate::remote_confirm::ToMain::Streaming("because the grammar".to_string()),
        );

        assert_eq!(
            session.reply_so_far(),
            "the turn was saying this",
            "the answer reached the tail the turn's own reply is drawn in"
        );
        let added: Vec<&str> = session.transcript[before..]
            .iter()
            .map(|entry| entry.text.as_str())
            .collect();
        assert!(
            added.is_empty(),
            "the answer was drawn among the turn's own lines: {added:?}"
        );
    }

    /// A person who has just typed /compact will reach for one of these, and nothing here can
    /// stop the one request a summary takes. Both are answered rather than swallowed: a key that
    /// does nothing and says nothing reads as an interface that has hung.
    #[test]
    fn a_key_that_would_stop_a_turn_is_answered_during_a_summary() {
        assert!(wants_cancel(key(KeyCode::Esc)));
        assert!(is_ctrl_c(ctrl('c')));
    }

    /// `commands()` is the one place the set is written down, so a command missing from it is a
    /// command the hint line, the completion list and Tab all fail to know about.
    #[test]
    fn compacting_is_offered_while_a_command_is_being_typed() {
        assert_eq!(
            completions("/comp")
                .iter()
                .map(|command| command.name)
                .collect::<Vec<_>>(),
            vec![COMPACT_COMMAND]
        );
    }

    /// Only the bare word, so asking the planner about clearing something still asks it.
    #[test]
    fn a_prompt_containing_the_clear_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "does /clear delete anything".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("does /clear delete anything".to_string())
        );
    }

    /// The name is the point of this one, so it must arrive with the action, spaces and all.
    #[test]
    fn the_rename_command_carries_the_whole_name() {
        let mut session = Session::new("none");
        for c in "/rename the parser bug".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Rename("the parser bug".to_string())
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
    }

    /// With no name there is nothing to rename to, and the loop says so.
    #[test]
    fn the_bare_rename_command_is_still_the_command() {
        let mut session = Session::new("none");
        for c in RENAME_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Rename(String::new())
        );
    }

    /// A sentence mentioning it is a thing to say to the planner.
    #[test]
    fn a_prompt_containing_the_rename_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "what does /rename do".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what does /rename do".to_string())
        );
    }

    /// The command that does not send the line it was typed on. What goes to the planner is what
    /// was left after the interval was read off, which is what every later tick sends too.
    #[test]
    fn the_loop_command_sends_what_is_left_after_the_interval() {
        let mut session = Session::new("none");
        for c in "/loop 5m check the deploy".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("check the deploy".to_string())
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert_eq!(
            session.looping().map(|running| running.prompt()),
            Some("check the deploy")
        );
    }

    /// With nothing to repeat there is no loop to start, and the interface says what it needs
    /// rather than quietly doing nothing.
    #[test]
    fn the_bare_loop_command_is_still_the_command() {
        for line in [LOOP_COMMAND, "/loop 5m"] {
            let mut session = Session::new("none");
            for c in line.chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }

            assert_eq!(
                handle_key(&mut session, key(KeyCode::Enter)),
                Action::Redraw
            );
            assert!(session.looping().is_none(), "{line} started a loop");
            assert_eq!(session.transcript.len(), 1, "{line} said nothing");
        }
    }

    /// A goal is a condition, not a prompt. Setting one that started a turn would send a line
    /// nobody typed, and there is no line: the argument is what stops the work, not what starts
    /// it.
    #[test]
    fn the_goal_command_sets_a_condition_without_sending_anything() {
        let mut session = Session::new("none");
        for c in "/goal cargo test exits 0".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert_eq!(
            session.goal().map(crate::goals::Running::condition),
            Some("cargo test exits 0")
        );
    }

    /// CMD-5's rule, on the one command whose argument is a whole sentence: the condition a judge
    /// is given has to be the characters the person endorsed.
    #[test]
    fn the_goal_command_carries_the_whole_condition() {
        let mut session = Session::new("none");
        for c in "/goal every test in bravebot-tui passes and clippy is clean".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));

        assert_eq!(
            session.goal().map(crate::goals::Running::condition),
            Some("every test in bravebot-tui passes and clippy is clean")
        );
    }

    #[test]
    fn the_goal_command_takes_the_goal_off_again() {
        let mut session = Session::new("none");
        session.start_goal("cargo test exits 0".to_string());

        for c in "/goal clear".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert!(session.goal().is_none());
    }

    /// With no condition there is no goal to set, and the interface says what it needs rather
    /// than quietly doing nothing.
    #[test]
    fn the_bare_goal_command_is_still_the_command() {
        let mut session = Session::new("none");
        for c in GOAL_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert!(session.goal().is_none(), "the bare word set a goal");
        assert_eq!(session.transcript.len(), 1, "the bare word said nothing");
    }

    /// A `@path` is a person vouching for a file with their keystroke. The sentence a goal sends
    /// the work back with is this program's, quoting a judge, so a word beginning with `@` in one
    /// is prose: reading it as a path would open a file on the say-so of a model, wearing an
    /// endorsement nobody gave.
    #[test]
    fn a_path_named_in_a_sentence_the_driver_wrote_vouches_for_nothing() {
        let carrying_on = bravebot_agent::goal::carry_on(
            "cargo test exits 0",
            "the failure is in @crates/core/src/policy.rs",
        );

        assert!(
            files_named_in(&carrying_on, Wrote::TheDriver).is_empty(),
            "a sentence this program wrote opened a file"
        );
        assert_eq!(
            files_named_in("look at @crates/core/src/policy.rs", Wrote::ThePerson),
            vec!["crates/core/src/policy.rs".to_string()],
            "a line the person typed stopped naming its files"
        );
    }

    /// A sentence mentioning it is a thing to say to the planner.
    #[test]
    fn a_prompt_containing_the_goal_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "what does /goal do".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what does /goal do".to_string())
        );
        assert!(session.goal().is_none());
    }

    #[test]
    fn a_longer_word_starting_with_goal_is_a_prompt() {
        let mut session = Session::new("none");
        for c in "/goals are useful".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("/goals are useful".to_string())
        );
        assert!(session.goal().is_none());
    }

    /// MANIFEST-9's other half: a session can reach the planning mode, so the word has to start a
    /// run and carry the whole task. The task is a sentence, like a goal's condition, so nothing
    /// may be cut off it: what the planner is shown is what it plans, and the planner is shown
    /// this and nothing else.
    #[test]
    fn a_session_can_ask_for_a_manifest_run() {
        let mut session = Session::new("none");
        for c in "/manifest summarise every doc under docs/specs".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Manifest("summarise every doc under docs/specs".to_string())
        );
        assert!(
            session.input().is_empty(),
            "the command is still in the box"
        );
    }

    /// The bare word plans nothing. There is no default task and there could not be one: the task
    /// string is the whole of a manifest run's input, so a run started without one would plan
    /// against nothing at all.
    #[test]
    fn a_manifest_run_needs_a_task_to_plan() {
        let mut session = Session::new("none");
        for c in MANIFEST_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Manifest(String::new()),
            "the bare word has to reach the loop, which says what it needs"
        );
        assert!(
            t!(manifest_needs_a_task).contains(MANIFEST_COMMAND),
            "the answer does not name the command it is about"
        );
    }

    /// MANIFEST-11. Escape at the plan prompt is answered as a decline, so a run stopped there
    /// comes back saying the plan was not approved rather than saying it was cancelled. Read off
    /// the error, the key would mean "leave no record" one moment and "write one" the moment
    /// before, for a person who pressed the same key asking for the same thing.
    #[test]
    fn a_run_the_person_stopped_is_read_off_the_key_and_not_off_the_error() {
        let declined = || {
            Err(bravebot_agent::TurnError::Precommit(
                "the plan was not approved, so nothing ran".to_string(),
            ))
        };
        let cancelled = || Err(bravebot_agent::TurnError::Cancelled { attempts: None });

        let pressed = Cancel::new();
        pressed.cancel();
        assert!(
            was_stopped(&declined(), &pressed),
            "a decline the person's own key produced was not read as a stop"
        );
        assert!(was_stopped(&cancelled(), &pressed));

        let untouched = Cancel::new();
        assert!(
            !was_stopped(&declined(), &untouched),
            "a plan declined at the prompt is a run worth recording"
        );
    }

    /// A sentence that merely mentions the word is a prompt. Starting a run from one would plan a
    /// whole run's worth of effects off a line somebody meant to say.
    #[test]
    fn a_manifest_run_is_not_a_prompt() {
        for line in [
            "/manifests are the other mode",
            "what does /manifest do",
            "explain the /manifest command",
        ] {
            let mut session = Session::new("none");
            for c in line.chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }

            assert_eq!(
                handle_key(&mut session, key(KeyCode::Enter)),
                Action::Submit(line.to_string()),
                "{line} started a run"
            );
        }
    }

    /// The key that stops things has to stop this one too, or a person watching a goal go wrong
    /// has to leave the session to get out of it.
    #[test]
    fn interrupting_takes_the_goal_off_before_it_leaves() {
        let mut session = Session::new("none");
        session.start_goal("cargo test exits 0".to_string());

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
        assert!(session.goal().is_none());
        assert!(
            !session.is_quitting(),
            "the press that stopped it also left"
        );

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
    }

    /// A turn that failed still ended. A watch left marked as firing would never fire again, and
    /// every prompt after it would be read as this program's rather than the person's, which
    /// quietly stops `@path` from vouching for anything.
    #[test]
    fn a_fire_whose_turn_failed_stops_being_the_turn_in_flight() {
        let mut session = Session::new("none");
        session.arm_watch(
            "notes.md",
            bravebot_agent::watch::Looked::Saw("first".to_string()),
        );
        session
            .watch_fired(Instant::now() + Duration::from_secs(6), |_| {
                bravebot_agent::watch::Looked::Saw("second".to_string())
            })
            .expect("a fire");
        assert!(session.watch_is_firing());

        fold_outcome(
            &mut session,
            Err(turn::TurnError::Precommit(
                "the backend said no".to_string(),
            )),
            Trail::new(),
            Carried {
                trust: TrustStore::new("/work"),
                programs: TrustedPrograms::new(),
                asked: AskedAbout::new(),
            },
            Occupied {
                budget: 100_000,
                guessed: false,
                last_request_tokens: 0,
            },
            Asked {
                name: "test-model".to_string(),
                comparable: true,
            },
            Line {
                text: "",
                wrote: Wrote::TheDriver,
            },
            &workspace_for_test(),
        );

        assert!(!session.watch_is_firing(), "the watch is still firing");
        assert_eq!(
            session.watches().len(),
            1,
            "a failed request ended the watch"
        );
    }

    /// Watches are the last rung before leaving, for the reason the loop and the goal are rungs:
    /// somebody pressing the key that stops things wants the things stopped, and picking which of
    /// eight survived is not a decision to make from a keystroke.
    #[test]
    fn interrupting_ends_every_watch_before_it_leaves() {
        let mut session = Session::new("none");
        session.arm_watch(
            "a.md",
            bravebot_agent::watch::Looked::Saw("first".to_string()),
        );
        session.arm_watch(
            "b.md",
            bravebot_agent::watch::Looked::Saw("first".to_string()),
        );

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
        assert!(session.watches().is_empty());
        assert!(
            !session.is_quitting(),
            "the press that stopped them also left"
        );

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
    }

    /// The command cannot arm one: a watch is asked for in a prompt, and what a person needs a
    /// command for is the half they cannot read off the transcript.
    #[test]
    fn the_watch_command_lists_and_stops_and_arms_nothing() {
        let mut session = Session::new("none");
        session.arm_watch(
            "a.md",
            bravebot_agent::watch::Looked::Saw("first".to_string()),
        );

        for c in "/watch".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert_eq!(session.watches().len(), 1, "a report ended a watch");

        for c in "/watch stop 1".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert!(session.watches().is_empty());

        for c in "/watch src/main.rs".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert!(
            session.watches().is_empty(),
            "the command armed a watch of its own"
        );
    }

    /// A sentence mentioning it is a thing to say to the planner.
    #[test]
    fn a_prompt_containing_the_loop_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "what does /loop do".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what does /loop do".to_string())
        );
        assert!(session.looping().is_none());
    }

    #[test]
    fn a_longer_word_starting_with_loop_is_a_prompt() {
        let mut session = Session::new("none");
        for c in "/looper is a good name".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("/looper is a good name".to_string())
        );
    }

    /// The key that means "stop what is happening" has to reach the thing that keeps happening
    /// before it reaches the thing that would take the session with it.
    #[test]
    fn interrupting_stops_the_loop_before_it_leaves() {
        let mut session = Session::new("none");
        session.start_loop(crate::loops::parse("5m watch").expect("a request"));
        session.complete("done", Vec::new(), 0);

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
        assert!(
            session.looping().is_none(),
            "the loop survived the interrupt"
        );
        assert!(!session.is_quitting(), "the session left as well");

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
    }

    /// A half-written line is nearer than the loop, so stopping the loop does not cost somebody
    /// the sentence they were in the middle of.
    #[test]
    fn interrupting_clears_the_line_before_it_stops_the_loop() {
        let mut session = Session::new("none");
        session.start_loop(crate::loops::parse("5m watch").expect("a request"));
        session.complete("done", Vec::new(), 0);
        for c in "half a thought".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        handle_key(&mut session, ctrl('c'));
        assert!(session.input().is_empty());
        assert!(session.looping().is_some(), "the loop went with the line");
    }

    /// One parser serves every command that takes an argument, so its rules are worth pinning
    /// once: the bare word, the word with an argument, and a longer word that merely starts alike.
    #[test]
    fn an_argument_is_taken_only_after_the_whole_command_word() {
        assert_eq!(
            argument_to("/rename a name", RENAME_COMMAND),
            Some("a name")
        );
        assert_eq!(argument_to("/rename", RENAME_COMMAND), Some(""));
        assert_eq!(
            argument_to("  /rename  spaced  ", RENAME_COMMAND),
            Some("spaced")
        );
        assert_eq!(argument_to("/renamed thing", RENAME_COMMAND), None);
        assert_eq!(argument_to("please /rename it", RENAME_COMMAND), None);
    }

    /// A slash offers everything; a letter narrows it; a space settles it, since the argument
    /// comes next and there is nothing left to complete.
    #[test]
    fn what_a_half_typed_line_could_become() {
        assert_eq!(completions("/").len(), commands().len());
        assert_eq!(
            completions("/cl")
                .iter()
                .map(|c| c.name)
                .collect::<Vec<_>>(),
            vec![CLEAR_COMMAND]
        );
        assert!(
            completions("/add-dir ~/notes").is_empty(),
            "a settled command"
        );
        assert!(completions("what does /model do").is_empty(), "a prompt");
        assert!(completions("").is_empty());
        assert!(completions("/zzz").is_empty(), "a word matching nothing");
    }

    /// Tab is what completes everywhere else, and a command taking an argument gets the space its
    /// argument goes after.
    #[test]
    fn tab_takes_the_highlighted_command() {
        let mut session = Session::new("none");
        for c in "/mod".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
        assert_eq!(session.input(), MODEL_COMMAND, "no argument, so no space");

        let mut session = Session::new("none");
        for c in "/add".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Tab));
        assert_eq!(
            session.input(),
            format!("{ADD_DIR_COMMAND} "),
            "argument follows"
        );
    }

    /// Enter on a half-typed command takes the highlighted row rather than sending "/mod" to the
    /// planner, which is never what was meant.
    #[test]
    fn enter_on_a_half_typed_command_completes_it() {
        let mut session = Session::new("none");
        for c in "/mod".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert_eq!(session.input(), MODEL_COMMAND);
        assert!(session.transcript.is_empty(), "a fragment was sent");
    }

    /// A command typed out in full still runs on Enter: completing must not get in the way of the
    /// thing it exists to help with.
    #[test]
    fn enter_on_a_whole_command_still_runs_it() {
        let mut session = Session::new("none");
        for c in CLEAR_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::Clear);
    }

    /// The arrows belong to the list while it is open, and to history and scrolling once it is not.
    #[test]
    fn the_arrows_walk_the_offered_commands_while_one_is_being_typed() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('/')));

        assert_eq!(
            session.highlighted_completion().map(|c| c.name),
            Some(commands()[0].name),
            "the list opens at the top"
        );
        handle_key(&mut session, key(KeyCode::Down));
        assert_eq!(
            session.highlighted_completion().map(|c| c.name),
            Some(commands()[1].name)
        );
        handle_key(&mut session, key(KeyCode::Up));
        assert_eq!(
            session.highlighted_completion().map(|c| c.name),
            Some(commands()[0].name)
        );

        // Up at the top stays, rather than wrapping to the end.
        handle_key(&mut session, key(KeyCode::Up));
        assert_eq!(
            session.highlighted_completion().map(|c| c.name),
            Some(commands()[0].name)
        );
    }

    /// Down past the end stays on the last, so Tab always takes something.
    #[test]
    fn walking_past_the_end_stays_on_the_last_command() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('/')));
        for _ in 0..commands().len() + 3 {
            handle_key(&mut session, key(KeyCode::Down));
        }
        assert_eq!(
            session.highlighted_completion().map(|c| c.name),
            Some(commands()[commands().len() - 1].name)
        );
    }

    /// Typing returns the cursor to the top, so the highlighted row does not drift to a different
    /// command as the list narrows under it.
    #[test]
    fn typing_another_letter_returns_to_the_top_of_the_list() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('/')));
        for _ in 0..4 {
            handle_key(&mut session, key(KeyCode::Down));
        }
        handle_key(&mut session, key(KeyCode::Char('c')));

        // The first command that still matches, whichever it is: the property is where the cursor
        // lands, and naming one command would make the test fail the next time one is added.
        let first = completions("/c").first().map(|command| command.name);
        assert!(first.is_some(), "nothing matched, so this proves nothing");
        assert_eq!(
            session.highlighted_completion().map(|c| c.name),
            first,
            "the cursor did not return to the top of what now matches"
        );
    }

    /// A paste narrows the list without touching the cursor, so the cursor may be past the end of
    /// what is left. Reading it must still name something, or Tab would complete nothing.
    #[test]
    fn a_cursor_past_the_end_of_a_narrowed_list_still_names_a_command() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('/')));
        // To the last of them, counted rather than named, so adding a command does not make this
        // test assert about the wrong row.
        for _ in 0..commands().len() {
            handle_key(&mut session, key(KeyCode::Down));
        }
        assert_eq!(
            session.highlighted_completion().map(|c| c.name),
            Some(commands()[commands().len() - 1].name)
        );

        // Now one command matches, while the cursor still points at the last.
        handle_paste(&mut session, "cl");
        assert_eq!(
            session.highlighted_completion().map(|c| c.name),
            Some(CLEAR_COMMAND),
            "the cursor pointed past the narrowed list"
        );
        handle_key(&mut session, key(KeyCode::Tab));
        assert_eq!(session.input(), CLEAR_COMMAND);
    }

    /// With nothing being offered, Tab must not insert anything and the arrows go back to what they
    /// do the rest of the time.
    #[test]
    fn tab_does_nothing_when_no_command_is_being_typed() {
        let mut session = Session::new("none");
        for c in "an ordinary prompt".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::None);
        assert_eq!(session.input(), "an ordinary prompt");
    }

    #[test]
    fn typing_the_status_command_reports_rather_than_prompting() {
        let mut session = Session::new("none");
        for c in STATUS_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Status
        );
        assert!(session.input().is_empty(), "the command stayed on the line");
        assert!(
            session.transcript.is_empty(),
            "the command was sent as a prompt"
        );
    }

    /// Asking the planner about status is a question, not a command.
    #[test]
    fn a_prompt_containing_the_status_command_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "what does /status show".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("what does /status show".to_string())
        );
    }

    /// A directory whose own name begins with a tilde is not a home-relative path.
    #[test]
    fn a_tilde_is_expanded_only_as_a_whole_first_segment() {
        let home = std::env::var("HOME").expect("a home directory");
        assert_eq!(expand_home("~/notes"), format!("{home}/notes"));
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("~notes"), "~notes");
        assert_eq!(expand_home("/tmp/notes"), "/tmp/notes");
        assert_eq!(expand_home("relative/notes"), "relative/notes");
    }

    #[test]
    fn ctrl_t_toggles_the_trail() {
        let mut session = Session::new("none");
        assert_eq!(handle_key(&mut session, ctrl('t')), Action::Redraw);
        assert!(session.show_trail);
        handle_key(&mut session, ctrl('t'));
        assert!(!session.show_trail);
    }

    /// The trail is drawn per entry as a turn fills in, so it is what somebody watching a turn call
    /// tools wants. Refused mid-turn, the key was swallowed by the catch-all for control chords
    /// while the hint line went on advertising it on every frame of that turn.
    #[test]
    fn the_trail_can_be_asked_for_while_a_turn_runs() {
        let mut session = Session::new("none");
        type_line(&mut session, "anything");
        session.submit().expect("the prompt is sent");

        assert_eq!(
            handle_key_while_working(&mut session, ctrl('t')),
            Action::Redraw
        );
        assert!(session.show_trail, "the key was swallowed");
        assert_eq!(session.input(), "", "a stray character was typed");
    }

    /// With nothing sent yet, the arrows scroll: there is no history to walk.
    #[test]
    fn arrow_keys_scroll_when_there_is_no_history() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Up));
        assert_eq!(session.scroll, 1);
        handle_key(&mut session, key(KeyCode::Up));
        assert_eq!(session.scroll, 2);
        handle_key(&mut session, key(KeyCode::Down));
        assert_eq!(session.scroll, 1);
    }

    /// Once something has been sent, Up recalls it, which is what a shell does and so what a
    /// user expects at a prompt.
    #[test]
    fn up_recalls_a_previous_prompt() {
        let mut session = Session::new("none");
        for c in "first question".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("an answer", Vec::new(), 0);

        assert_eq!(handle_key(&mut session, key(KeyCode::Up)), Action::Redraw);
        assert_eq!(session.input(), "first question");
        assert_eq!(session.scroll, 0, "recall scrolled the transcript as well");
    }

    /// The keys that walk the history do it whether or not a turn is running. They reached no
    /// arm at all mid-turn and fell through to nothing, so a person watching a turn go wrong
    /// could see their last prompt in the transcript and had no way to get it back into the box.
    #[test]
    fn up_recalls_a_previous_prompt_while_a_turn_is_running() {
        let mut session = Session::new("none");
        for c in "first question".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));
        // No `complete`: the turn is still in flight.
        assert_eq!(session.status, Status::Working);

        assert_eq!(
            handle_key_while_working(&mut session, key(KeyCode::Up)),
            Action::Redraw
        );
        assert_eq!(session.input(), "first question");

        assert_eq!(
            handle_key_while_working(&mut session, key(KeyCode::Down)),
            Action::Redraw
        );
        assert!(session.input().is_empty(), "Down did not walk back out");
    }

    /// The press a person makes when an answer is going wrong in front of them is asking for the
    /// answer to stop, not for the session to end. It left instead, taking the transcript and
    /// everything else with it.
    #[test]
    fn ctrl_c_stops_a_turn_rather_than_leaving() {
        let mut session = Session::new("none");
        session.type_char('a');
        session.submit();
        assert_eq!(session.status, Status::Working);

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Cancel);
        assert!(
            !session.is_quitting(),
            "stopping the turn ended the session"
        );
    }

    /// The line in the box is the next thing there is to stop, and a person half way through a
    /// sentence is not asking to leave. Leaving over it would take the words with the session.
    #[test]
    fn ctrl_c_clears_the_line_before_it_leaves() {
        let mut session = Session::new("none");
        for c in "half a thought".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
        assert_eq!(session.input(), "");
        assert!(
            !session.is_quitting(),
            "clearing the line ended the session"
        );
        assert!(
            session.cleared_by_interrupt,
            "nothing said what the next press would do"
        );
    }

    /// A press that ends the session is not one to explain, and the hint is the answer to a line
    /// having just gone. On an empty line nothing went, so there is nothing to answer.
    #[test]
    fn the_way_out_is_offered_only_where_a_line_was_taken() {
        let mut session = Session::new("none");
        handle_key(&mut session, ctrl('c'));
        assert!(
            !session.cleared_by_interrupt,
            "offered where nothing was cleared"
        );
    }

    /// It lives for one press. Standing there afterwards, it would go on offering an exit to
    /// somebody who has started writing the next line and is no longer being asked anything.
    #[test]
    fn the_way_out_stops_being_offered_at_the_next_press() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('x')));
        handle_key(&mut session, ctrl('c'));
        assert!(session.cleared_by_interrupt);

        handle_key(&mut session, key(KeyCode::Char('y')));
        assert!(
            !session.cleared_by_interrupt,
            "the hint outstayed its press"
        );
    }

    /// The whole ladder, nearest first: the turn, then the line the stop put back, then the
    /// session. Each press has something of its own to answer, so none of them is a press that
    /// silently did another one's job.
    #[test]
    fn ctrl_c_leaves_once_there_is_nothing_left_to_stop() {
        let mut session = Session::new("none");
        session.type_char('a');
        session.submit();

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Cancel);
        session.restore("a");
        assert_eq!(session.input(), "a", "the stopped prompt came back");

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
        assert!(!session.is_quitting());

        assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
        assert!(session.is_quitting());
    }

    /// Escape is still the key that stops a turn, and stopping a turn is not leaving. Losing that
    /// distinction is what made Ctrl-C useless as a way out.
    #[test]
    fn escape_stops_the_turn_without_ending_the_session() {
        let mut session = Session::new("none");
        session.type_char('a');
        session.submit();

        assert!(wants_cancel(key(KeyCode::Esc)));
        assert!(!session.is_quitting(), "stopping a turn ended the session");
    }

    /// Enter mid-turn used to reach nothing, so the line sat in the box while the person waited
    /// to notice the turn had ended. It goes now, and says that it is waiting.
    #[test]
    fn enter_queues_a_prompt_while_a_turn_is_running() {
        let mut session = Session::new("none");
        for c in "first".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        for c in "second".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key_while_working(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert!(session.input().is_empty(), "the line stayed in the box");
        assert_eq!(session.queued.len(), 1);
        assert_eq!(session.queued[0].prompt, "second");
    }

    /// A command is not a prompt, so what waits for the turn is the word itself, to be carried out
    /// when the turn ends. It used to wait as a prompt, which put `/clear` into the running turn's
    /// reach: the planner was asked what to clear and the session was never cleared.
    #[test]
    fn a_command_typed_while_a_turn_runs_is_not_sent_as_a_prompt() {
        let mut session = Session::new("none");
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        for c in CLEAR_COMMAND.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key_while_working(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );

        assert!(session.input().is_empty(), "the line stayed in the box");
        assert_eq!(session.queued.len(), 1, "the command is not waiting");
        assert_eq!(session.queued[0].prompt, CLEAR_COMMAND);
        assert_eq!(
            session.interjections().take(),
            None,
            "the running turn was handed the command"
        );
        assert_eq!(
            session
                .transcript
                .last()
                .expect("the first prompt is in the transcript")
                .text,
            "first",
            "the command joined the conversation while it waited"
        );
    }

    /// What queueing promises, and the whole of what was deferred: the word runs when the queue is
    /// reached, it runs as the command rather than as anything the planner is asked, and it runs off
    /// the loop, since the press that asked for it has already been made.
    #[test]
    fn the_command_queued_while_a_turn_ran_is_carried_out_when_the_turn_ends() {
        let mut session = Session::new("none");
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));

        for c in CLEAR_COMMAND.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
        assert_eq!(
            queued_next(&mut session),
            None,
            "it was carried out while the turn was still running"
        );

        session.complete("answered", Vec::new(), 0);
        assert_eq!(queued_next(&mut session), Some(Action::Clear));
        assert!(session.queued.is_empty(), "it is still waiting");
        assert_eq!(queued_next(&mut session), None, "it was carried out twice");
    }

    /// The line behind a command has nobody else to send it: what sends a queued prompt is a turn
    /// ending, and the turn that ended stopped at the command in front of it. Left to the key
    /// handling it would have sat there marked as waiting until the person typed something unrelated.
    #[test]
    fn a_prompt_queued_behind_a_command_is_sent_once_the_command_has_run() {
        let mut session = Session::new("none");
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));

        for c in CLEAR_COMMAND.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
        for c in "and tidy up".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
        session.complete("answered", Vec::new(), 0);

        assert_eq!(
            queued_next(&mut session),
            Some(Action::Clear),
            "the prompt went before the command in front of it"
        );
        assert_eq!(
            queued_next(&mut session),
            Some(Action::Submit("and tidy up".to_string())),
            "the prompt behind the command was left waiting"
        );
        assert_eq!(session.status, Status::Working);
    }

    /// The turn takes the oldest prompt, and a command queued ahead of one is not it. Popping the head
    /// of the queue regardless would have recorded the command as the line the planner was given and
    /// left the prompt waiting for a turn that had already read it.
    #[test]
    fn a_queued_command_is_not_what_the_turn_took() {
        let mut session = Session::new("none");
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));

        for c in CLEAR_COMMAND.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
        for c in "and tidy up".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));

        assert_eq!(
            session.interjections().take().as_deref(),
            Some("and tidy up"),
            "the turn was offered something other than the prompt"
        );
        session.interjected();

        assert_eq!(
            session
                .transcript
                .last()
                .expect("the interjection is in the transcript")
                .text,
            "and tidy up",
            "the command was recorded as the line the turn took"
        );
        assert_eq!(session.queued.len(), 1, "the command stopped waiting");
        assert_eq!(session.queued[0].prompt, CLEAR_COMMAND);
    }

    /// A queued command comes back like a queued prompt, because a person who changed their mind about
    /// `/clear` has the same claim on it. It comes back whatever the turn has reached: what stops a
    /// prompt coming back is the planner having been given it, and a command was given to nobody.
    #[test]
    fn a_queued_command_comes_back_to_the_box() {
        let mut session = Session::new("none");
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));

        for c in CLEAR_COMMAND.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));

        assert!(session.unqueue(), "the command could not be taken back");
        assert_eq!(session.input(), CLEAR_COMMAND);
        assert!(session.queued.is_empty(), "it is waiting still");
    }

    /// The two paths have to agree about which lines are commands, and the table is what says so: a
    /// command it gives no argument is only ever the bare word. Queueing `/undo the last change` as a
    /// command would carry out an undo the idle path sends to the planner instead.
    #[test]
    fn a_command_that_takes_no_argument_is_only_the_bare_word_mid_turn() {
        let line = format!("{UNDO_COMMAND} the last change");
        let mut session = Session::new("none");
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        for c in line.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));

        assert_eq!(session.queued.len(), 1, "the prompt was not queued");
        assert_eq!(session.queued[0].prompt, line);
        assert_eq!(
            session.interjections().take().as_deref(),
            Some(line.as_str()),
            "a prompt was queued as a command"
        );
    }

    /// In shell mode the line is a command line and `/status` is a path to a program, which is the
    /// guard the idle ladder answers ahead of its command arm. Taking it for a command would run this
    /// program's `/status` at the end of the turn instead of the one the person named.
    #[test]
    fn a_shell_line_is_not_taken_for_a_command_while_a_turn_runs() {
        let mut session = Session::new("none");
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));
        session.shell = true;

        for c in STATUS_COMMAND.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
        session.complete("answered", Vec::new(), 0);

        assert_eq!(
            session.take_queued_command(),
            None,
            "a command line was queued as a command"
        );
    }

    /// Every word in the table rather than the one that was reported. The arm reads the same table the
    /// idle path dispatches from, so a command added there waits as a command here without anybody
    /// having to remember a second list.
    #[test]
    fn no_command_is_sent_as_a_prompt_while_a_turn_runs() {
        for command in commands() {
            let mut session = Session::new("none");
            type_line(&mut session, "first");
            handle_key(&mut session, key(KeyCode::Enter));
            assert_eq!(session.status, Status::Working);

            for c in command.name.chars() {
                handle_key_while_working(&mut session, key(KeyCode::Char(c)));
            }
            handle_key_while_working(&mut session, key(KeyCode::Enter));

            assert_eq!(
                session.interjections().take(),
                None,
                "the running turn was handed {}",
                command.name
            );
            session.complete("answered", Vec::new(), 0);
            assert_eq!(
                session.take_queued_command().as_deref(),
                Some(command.name),
                "{} did not wait to be carried out",
                command.name
            );
        }
    }

    /// The narrowness CMD-2 asks for, on the path that had no command arm at all: a sentence about
    /// a command is a sentence, and a command with its argument is still the command. An arm that
    /// took either for the other would make the box mid-turn a different language from the box at
    /// rest.
    #[test]
    fn a_prompt_mentioning_a_command_is_queued_while_a_turn_runs() {
        let mut session = Session::new("none");
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));

        assert_eq!(session.status, Status::Working);
        for c in "what does /clear do".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));

        assert_eq!(
            session.queued.len(),
            1,
            "the question was taken as a command"
        );
        assert_eq!(session.queued[0].prompt, "what does /clear do");
        assert_eq!(
            session.interjections().take().as_deref(),
            Some("what does /clear do"),
            "a question waits as a command"
        );
    }

    /// A command that takes an argument is a command on this path too, argument and all. `/rename`
    /// mid-turn used to name nothing and ask the planner about renaming instead.
    #[test]
    fn a_command_with_an_argument_is_not_sent_as_a_prompt_while_a_turn_runs() {
        let mut session = Session::new("none");
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));

        assert_eq!(session.status, Status::Working);
        for c in "/rename the parser work".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
        assert_eq!(
            session.interjections().take(),
            None,
            "the running turn was handed the command"
        );
        session.complete("answered", Vec::new(), 0);

        let queued = session
            .take_queued_command()
            .expect("the command was not waiting to be carried out");
        assert_eq!(
            dispatch_command(&mut session, &queued),
            Action::Rename("the parser work".to_string())
        );
    }

    /// The fallback in [`dispatch_command`] is unreachable, and this is what says so: a word the table
    /// names that no arm answers would be recognised, taken off the box and then quietly dropped, which
    /// is worse than a word nobody recognises. The bare word is enough for every one of them, since a
    /// command that takes an argument takes an empty one.
    #[test]
    fn every_command_in_the_table_dispatches() {
        for command in commands() {
            let mut session = Session::new("none");
            assert_ne!(
                dispatch_command(&mut session, command.name),
                Action::None,
                "{} is in the table and does nothing",
                command.name
            );
        }
    }

    /// The point of queueing, and what it used to fail at: a prompt typed mid-turn is put where the
    /// running turn can reach it, not left for a turn that may be minutes away. A person who says
    /// "no, not that file" while an agent works is trying to redirect the work in front of them,
    /// and an instruction that waits for the answer arrives after the thing it was meant to change.
    #[test]
    fn a_prompt_queued_mid_turn_is_within_the_running_turns_reach() {
        let mut session = Session::new("none");
        for c in "first".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));

        // What the worker holds. Taken before the prompt is queued, exactly as a turn takes it.
        let reaching = session.interjections();
        assert!(reaching.take().is_none(), "something was waiting already");

        for c in "actually stop".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));

        assert_eq!(
            reaching.take().as_deref(),
            Some("actually stop"),
            "the turn could not reach a prompt typed while it ran"
        );
    }

    /// Until the turn takes it, a queued prompt has not been said: it is drawn above the box as
    /// waiting, and the transcript is for what has happened. It joins the transcript at the moment
    /// the planner is given it, which is what keeps the two reading in the same order.
    #[test]
    fn a_queued_prompt_joins_the_transcript_when_the_planner_is_given_it() {
        let mut session = Session::new("none");
        for c in "first".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));
        for c in "second".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));

        let said = session
            .transcript
            .iter()
            .filter(|entry| entry.speaker == crate::state::Speaker::User)
            .count();
        assert_eq!(said, 1, "a prompt that had not gone anywhere was recorded");

        session.interjected();
        let said: Vec<&str> = session
            .transcript
            .iter()
            .filter(|entry| entry.speaker == crate::state::Speaker::User)
            .map(|entry| entry.text.as_str())
            .collect();
        assert_eq!(said, vec!["first", "second"]);
        assert!(session.queued.is_empty(), "it is still drawn as waiting");
    }

    /// A prompt still waiting when the turn ends is a turn of its own, as every queued prompt used
    /// to be. What must not happen is both: the copy left for the turn that has finished has to go,
    /// or the same words reach the planner twice, the second time as an interjection into the very
    /// turn they started.
    #[test]
    fn a_prompt_that_outlived_the_turn_is_sent_once() {
        let mut session = Session::new("none");
        for c in "first".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));
        for c in "second".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));

        let reaching = session.interjections();
        session.complete("done", Vec::new(), 0);
        assert_eq!(session.send_queued().as_deref(), Some("second"));
        assert!(
            reaching.take().is_none(),
            "the prompt was sent as a turn and left waiting to be interjected as well"
        );
    }

    /// Three prompts queued during one turn, and the rule that governs them afterwards: the running
    /// turn takes what it can, one becomes the next turn, and the rest go on waiting under the same
    /// rule. What must hold throughout is that the queue on the screen and the buffer the turn takes
    /// from never disagree about how many prompts are waiting.
    #[test]
    fn what_is_still_waiting_stays_in_step_with_what_is_drawn() {
        let mut session = Session::new("none");
        session.type_char('a');
        session.submit();
        let reaching = session.interjections();

        for line in ["one", "two", "three"] {
            for c in line.chars() {
                session.type_char(c);
            }
            assert!(session.queue(), "nothing was queued");
        }

        // The turn takes the first, as it would at a round boundary.
        assert_eq!(reaching.take().as_deref(), Some("one"));
        session.interjected();
        assert_eq!(session.queued.len(), 2);

        // Then it ends with two still waiting. The oldest becomes a turn of its own.
        session.complete("an answer", Vec::new(), 0);
        assert_eq!(session.send_queued().as_deref(), Some("two"));
        assert_eq!(session.queued.len(), 1);

        // And the last is waiting for the turn that just began, not for a second copy of itself.
        assert_eq!(reaching.take().as_deref(), Some("three"));
        assert!(reaching.take().is_none(), "a prompt was waiting twice over");
    }

    /// Up reaches for the last thing the person said, and while prompts are waiting that is the
    /// queue. It walked the history instead, which holds a copy of every queued line: the copy
    /// came back, the person rewrote it, and the prompt they meant to take back went anyway.
    ///
    /// One press for the whole queue, not one press each. A key that gave them back a line at a
    /// time would leave the person pressing it until they guessed there were none left, with the
    /// ones they had already taken back sitting in the box in front of them.
    #[test]
    fn up_takes_back_everything_waiting_rather_than_a_copy_of_it() {
        let mut session = Session::new("none");
        for c in "first".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));
        for line in ["second", "third"] {
            for c in line.chars() {
                handle_key_while_working(&mut session, key(KeyCode::Char(c)));
            }
            handle_key_while_working(&mut session, key(KeyCode::Enter));
        }
        assert_eq!(session.queued.len(), 2);

        assert_eq!(
            handle_key_while_working(&mut session, key(KeyCode::Up)),
            Action::Redraw
        );
        assert_eq!(session.input(), "second\nthird");
        assert!(session.queued.is_empty(), "a prompt was left waiting");
    }

    /// Taking a prompt back has to take it out of the turn's reach too, or the key reads as having
    /// done nothing: the line comes back to the box, the person rewrites it, and the copy the turn
    /// was still holding arrives at the planner anyway. Which is the bug the old queue could not
    /// have, because nothing could reach a queued prompt until the turn was over.
    #[test]
    fn taking_the_queue_back_takes_it_out_of_the_turns_reach() {
        let mut session = Session::new("none");
        for c in "first".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));
        let reaching = session.interjections();
        for c in "second".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));

        handle_key_while_working(&mut session, key(KeyCode::Up));
        assert_eq!(session.input(), "second");
        assert!(
            reaching.take().is_none(),
            "a prompt taken back was still on its way to the planner"
        );
    }

    /// The other side of it: a prompt the turn has already been given cannot be taken back, because
    /// the planner has it. Offering it to the box would leave the person editing a line that had
    /// gone, and pressing Enter would say it twice.
    #[test]
    fn a_prompt_the_turn_has_taken_cannot_be_taken_back() {
        let mut session = Session::new("none");
        for c in "first".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));
        let reaching = session.interjections();
        for c in "second".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));

        // The turn takes it, as it would at its next round boundary.
        assert_eq!(reaching.take().as_deref(), Some("second"));

        handle_key_while_working(&mut session, key(KeyCode::Up));
        assert!(
            !session.input().contains("second"),
            "a prompt the planner already had came back to the box: {}",
            session.input()
        );
    }

    /// With nothing waiting the key means what it has always meant. Taking the queue back is the
    /// exception, and it lasts exactly as long as there is a queue.
    #[test]
    fn up_walks_the_history_again_once_nothing_is_waiting() {
        let mut session = Session::new("none");
        for c in "first".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));
        for c in "second".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
        handle_key_while_working(&mut session, key(KeyCode::Up));
        for _ in 0.."second".len() {
            handle_key_while_working(&mut session, key(KeyCode::Backspace));
        }
        assert_eq!(session.input(), "");

        handle_key_while_working(&mut session, key(KeyCode::Up));
        assert_eq!(session.input(), "second", "the history was out of reach");
    }

    /// Shift-Enter writes a paragraph mid-turn as it does at rest, so it must not be caught by
    /// the arm that queues. A queued half-sentence is worse than one that waits to be finished.
    #[test]
    fn starting_a_line_mid_turn_does_not_queue_it() {
        let mut session = Session::new("none");
        session.type_char('a');
        session.submit();

        for c in "half".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(
            &mut session,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
        );

        assert!(
            session.queued.is_empty(),
            "a paragraph was sent half-written"
        );
        assert!(session.input().contains('\n'), "no line was started");
    }

    /// Why a key is allowed to mean something different while a turn runs, or `None` where it is
    /// not.
    ///
    /// The whole of the difference between the two paths, in one place. Every one of them is about
    /// sending or about leaving, which are the two things the box does not decide.
    fn allowed_to_differ(key: KeyEvent) -> Option<&'static str> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            // Shift-Enter and Ctrl-J start a line and are not this key.
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => Some("sends"),
            KeyCode::Esc => Some("stops the turn in flight"),
            KeyCode::Char('c') if ctrl => Some("stops the turn in flight, and then leaves"),
            KeyCode::Char('d') if ctrl => Some("leaves, which is not something the box does"),
            KeyCode::Char('g') if ctrl => {
                Some("hands the screen the turn is drawing on to an editor")
            }
            KeyCode::Char('!') => Some("arms a mode that changes what Enter does"),
            _ => None,
        }
    }

    /// Every key the two paths could see, so a binding added to one and not the other is caught
    /// here rather than by somebody pressing it.
    fn every_key() -> Vec<KeyEvent> {
        let codes = (0x20u8..=0x7e)
            .map(|byte| KeyCode::Char(char::from(byte)))
            .chain([
                KeyCode::Enter,
                KeyCode::Tab,
                KeyCode::BackTab,
                KeyCode::Backspace,
                KeyCode::Delete,
                KeyCode::Insert,
                KeyCode::Home,
                KeyCode::End,
                KeyCode::PageUp,
                KeyCode::PageDown,
                KeyCode::Up,
                KeyCode::Down,
                KeyCode::Left,
                KeyCode::Right,
                KeyCode::Esc,
                KeyCode::F(1),
            ]);
        let modifiers = [
            KeyModifiers::NONE,
            KeyModifiers::CONTROL,
            KeyModifiers::SHIFT,
            KeyModifiers::ALT,
        ];
        codes
            .flat_map(|code| modifiers.iter().map(move |held| KeyEvent::new(code, *held)))
            .collect()
    }

    /// The two paths answer the same set of keys, and the ones they are allowed to disagree about
    /// are named rather than discovered.
    ///
    /// Asserted as a set because a list was the bug. This walked six key codes, and three keys that
    /// send nothing were answered by the idle path alone: none of the three was in the list, so the
    /// test that was supposed to pin the agreement said nothing whatever about any of them.
    #[test]
    fn the_two_paths_answer_the_same_set_of_keys() {
        /// Everything a key press can reach that is not about sending, so a difference in any of it
        /// is a difference the clause forbids.
        fn answered(session: &Session, action: Action) -> String {
            format!(
                "{action:?} input={:?} caret={} scroll={} shell={} shortcuts={} trail={} \
                 stashed={:?} scrolling={} queued={} browsing={} searching={} vi={:?}",
                session.input(),
                session.caret(),
                session.scroll,
                session.shell,
                session.shortcuts,
                session.show_trail,
                session.stashed(),
                session.scrolling(),
                session.queued.len(),
                session.history.is_browsing(),
                session.searching_history(),
                session.vi_mode(),
            )
        }

        let sent = |finished: bool, editing: crate::vim::Editing, typed: &str| {
            let mut session = Session::new("none");
            session.choose_editing(editing);
            type_line(&mut session, "first question");
            handle_key(&mut session, key(KeyCode::Enter));
            // The same transcript either way, so only the running turn differs.
            if finished {
                session.complete("an answer", Vec::new(), 0);
            } else {
                session.narrate("an answer");
            }
            // Several arms are read against whether there is a line, so both cases are swept.
            for c in typed.chars() {
                if finished {
                    handle_key(&mut session, key(KeyCode::Char(c)));
                } else {
                    handle_key_while_working(&mut session, key(KeyCode::Char(c)));
                }
            }
            session
        };

        // Both styles of editing, and in the vi one every mode it has. A key wired into one path and
        // not the other is the bug this catches, and vi editing multiplies the places to wire one:
        // each mode gives every printable character a meaning of its own, so a letter answered by the
        // idle path alone would be a letter that silently did nothing during a turn.
        let modes = [
            None,
            Some(crate::vim::Mode::Normal),
            Some(crate::vim::Mode::Visual { lines: false }),
            Some(crate::vim::Mode::Visual { lines: true }),
        ];
        for editing in crate::vim::Editing::ALL {
            for mode in modes {
                if mode.is_some() != (editing == crate::vim::Editing::Vi) {
                    continue;
                }
                for line in ["", "half a thought"] {
                    for pressed in every_key() {
                        if allowed_to_differ(pressed).is_some() {
                            continue;
                        }
                        let mut idle = sent(true, editing, line);
                        let mut working = sent(false, editing, line);
                        for session in [&mut idle, &mut working] {
                            match mode {
                                None => {}
                                Some(crate::vim::Mode::Visual { lines }) => {
                                    session.enter_vi_normal();
                                    session.type_char(if lines { 'V' } else { 'v' });
                                }
                                Some(_) => {
                                    session.enter_vi_normal();
                                }
                            }
                        }
                        assert_eq!(working.status, Status::Working);
                        assert_eq!(
                            answered(&idle, Action::None),
                            answered(&working, Action::None),
                            "the two sessions differed before {pressed:?} was pressed"
                        );

                        let at_rest = handle_key(&mut idle, pressed);
                        let mid_turn = handle_key_while_working(&mut working, pressed);

                        assert_eq!(
                            answered(&idle, at_rest),
                            answered(&working, mid_turn),
                            "{pressed:?} was answered differently while a turn was running, \
                             over a line of {line:?}, editing {editing:?} in {mode:?}"
                        );
                    }
                }
            }
        }
    }

    /// Down walks back out of history, restoring the line that was being typed.
    #[test]
    fn down_returns_from_history_to_the_typed_line() {
        let mut session = Session::new("none");
        for c in "sent".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("ok", Vec::new(), 0);

        for c in "being typed".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Up));
        assert_eq!(session.input(), "sent");

        handle_key(&mut session, key(KeyCode::Down));
        assert_eq!(
            session.input(),
            "being typed",
            "the half-typed line was not restored"
        );
    }

    /// Typing over a recalled prompt makes it the working line, so the position label goes away.
    #[test]
    fn typing_leaves_history_browsing() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('a')));
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("ok", Vec::new(), 0);

        handle_key(&mut session, key(KeyCode::Up));
        assert!(session.history.is_browsing());
        handle_key(&mut session, key(KeyCode::Char('b')));
        assert!(!session.history.is_browsing());
    }

    /// Escape leaves history along with clearing, so it cannot be left labelled with an empty box.
    #[test]
    fn escape_leaves_history_browsing() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('a')));
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("ok", Vec::new(), 0);

        handle_key(&mut session, key(KeyCode::Up));
        assert!(session.history.is_browsing());
        handle_key(&mut session, key(KeyCode::Esc));
        assert!(!session.history.is_browsing());
        assert!(session.input().is_empty());
    }

    /// A session that has sent `prompts`, with nothing running.
    fn having_sent(prompts: &[&str]) -> Session {
        let mut session = Session::new("none");
        for prompt in prompts {
            type_line(&mut session, prompt);
            handle_key(&mut session, key(KeyCode::Enter));
            session.complete("ok", Vec::new(), 0);
        }
        session
    }

    /// Up walks a prompt at a time, which is no way to reach the hundredth. Ctrl-R is the chord
    /// every shell answers with this question, so it is the one to answer it with here.
    #[test]
    fn ctrl_r_searches_the_prompts_already_sent() {
        let mut session = having_sent(&["first question", "second question"]);
        handle_key(&mut session, ctrl('r'));
        assert!(session.searching_history());
        assert_eq!(
            session.history_match().expect("a prompt").prompt,
            "second question"
        );
    }

    /// Mid-turn is when the prompt somebody wants back is most likely to be one they can no longer
    /// see, and nothing about searching sends anything, which is the whole of what a turn refuses.
    #[test]
    fn the_prompts_can_be_searched_while_a_turn_is_running() {
        let mut session = having_sent(&["first question"]);
        type_line(&mut session, "second question");
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        handle_key_while_working(&mut session, ctrl('r'));
        assert!(session.searching_history());
    }

    /// The loops answer the stop keys before any ladder does, so the guard that lets the search
    /// have them first is the whole of what keeps the keys that close it from ending the turn a
    /// person opened it during. Searching sends nothing, so there was never anything in it for the
    /// turn to refuse.
    #[test]
    fn the_search_answers_the_stop_keys_before_the_turn_does() {
        let mut session = having_sent(&["first question"]);
        type_line(&mut session, "second question");
        handle_key(&mut session, key(KeyCode::Enter));

        for stopping in [ctrl('c'), key(KeyCode::Esc)] {
            assert!(
                stops_the_turn(&session, stopping),
                "{stopping:?} did not reach the turn with nothing in the way"
            );
        }

        handle_key_while_working(&mut session, ctrl('r'));
        for stopping in [ctrl('c'), key(KeyCode::Esc)] {
            assert!(
                !stops_the_turn(&session, stopping),
                "{stopping:?} stopped the turn from inside the search"
            );
        }

        handle_key_while_working(&mut session, key(KeyCode::Esc));
        assert!(!session.searching_history(), "the search did not close");
        assert_eq!(session.status, Status::Working, "the turn was stopped");
    }

    /// A summary and an aside answer the stop keys themselves, so the search opened while one of
    /// them was out has to be asked before the request is: answered by the request instead, Escape
    /// reported work that cannot be interrupted and Ctrl-C ended the session, and the search a
    /// person was trying to leave was still open behind both.
    #[test]
    fn the_search_answers_the_stop_keys_before_a_single_request_does() {
        for stopping in [ctrl('c'), key(KeyCode::Esc)] {
            let mut session = having_sent(&["first question"]);
            session.begin_aside();

            one_request_key(&mut session, ctrl('r'), "nothing to interrupt");
            assert!(session.searching_history(), "the search did not open");
            let said = session.transcript.len();

            one_request_key(&mut session, stopping, "nothing to interrupt");

            assert!(
                !session.searching_history(),
                "{stopping:?} left the search open"
            );
            assert!(!session.is_quitting(), "{stopping:?} ended the session");
            assert_eq!(
                session.transcript.len(),
                said,
                "{stopping:?} reported the request from inside the search"
            );
        }
    }

    /// The goal check is one request too, and the goal behind it is what its stop keys are for.
    /// Neither key is about the goal while the search is open, and Ctrl-C there took the session
    /// with it.
    #[test]
    fn the_search_answers_the_stop_keys_before_the_goal_check_does() {
        for stopping in [ctrl('c'), key(KeyCode::Esc)] {
            let mut session = having_sent(&["first question"]);
            session.begin_aside();
            session.start_goal("cargo test exits 0".to_string());

            goal_check_key(&mut session, ctrl('r'));
            assert!(session.searching_history(), "the search did not open");
            let said = session.transcript.len();

            goal_check_key(&mut session, stopping);

            assert!(
                !session.searching_history(),
                "{stopping:?} left the search open"
            );
            assert!(
                session.goal().is_some(),
                "{stopping:?} cleared the goal from inside the search"
            );
            assert!(!session.is_quitting(), "{stopping:?} ended the session");
            assert_eq!(
                session.transcript.len(),
                said,
                "{stopping:?} reported the goal from inside the search"
            );
        }
    }

    /// What the same keys mean with nothing standing over the request. There is no round for a stop
    /// to land between, so Escape can only report that and Ctrl-C can only be about leaving. The
    /// report is made once: pressing again is asking the same question, and answering it every time
    /// fills the transcript with the news that nothing happened.
    #[test]
    fn a_single_request_says_it_cannot_be_stopped_and_leaves_on_ctrl_c() {
        let mut session = having_sent(&["first question"]);
        session.begin_aside();
        let said = session.transcript.len();

        one_request_key(&mut session, key(KeyCode::Esc), "nothing to interrupt");

        assert_eq!(
            session.transcript.len(),
            said + 1,
            "escape said nothing about a request that cannot be stopped"
        );
        assert!(!session.is_quitting(), "escape ended the session");

        one_request_key(&mut session, key(KeyCode::Esc), "nothing to interrupt");
        assert_eq!(
            session.transcript.len(),
            said + 1,
            "pressing again said it a second time"
        );

        one_request_key(&mut session, ctrl('c'), "nothing to interrupt");
        assert!(session.is_quitting(), "ctrl-c did not leave");
    }

    /// The goal check is one request as well, but there is something behind it worth stopping: the
    /// goal that keeps putting turns out. Both keys take it off and say so, since a person pressing
    /// either at the ninth round is asking for the tenth not to go. Only with the goal already off
    /// does Ctrl-C mean leaving, and Escape has nothing left to ask for.
    #[test]
    fn the_goal_check_takes_the_goal_off_before_ctrl_c_means_leaving() {
        for stopping in [ctrl('c'), key(KeyCode::Esc)] {
            let mut session = having_sent(&["first question"]);
            session.begin_aside();
            session.start_goal("cargo test exits 0".to_string());
            let said = session.transcript.len();

            goal_check_key(&mut session, stopping);

            assert!(session.goal().is_none(), "{stopping:?} left the goal armed");
            let after = session.transcript[said..]
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                after.contains(t!(goal_cleared)),
                "{stopping:?} took the goal off without saying so: {after}"
            );
            assert!(
                after.contains(t!(goal_uninterruptible)),
                "{stopping:?} said nothing of the check still in flight: {after}"
            );
            assert!(!session.is_quitting(), "{stopping:?} ended the session");
        }

        let mut session = having_sent(&["first question"]);
        session.begin_aside();
        let said = session.transcript.len();

        goal_check_key(&mut session, key(KeyCode::Esc));

        assert_eq!(
            session.transcript.len(),
            said,
            "escape reported something with no goal to take off"
        );
        assert!(!session.is_quitting(), "escape ended the session");

        goal_check_key(&mut session, ctrl('c'));
        assert!(
            session.is_quitting(),
            "ctrl-c did not leave with the goal already off"
        );
    }

    /// With nothing sent there is nothing to search, and a panel saying so is a mode a person then
    /// has to get out of.
    #[test]
    fn ctrl_r_with_nothing_sent_yet_opens_nothing() {
        let mut session = Session::new("none");
        handle_key(&mut session, ctrl('r'));
        assert!(!session.searching_history());
    }

    /// Walking back at all says the wanted prompt is an old one, and the workspace somebody is in
    /// is the narrower question about it. The list it opens on is the whole point: seeded with the
    /// prompt in the box it would answer with that prompt alone, and narrowed to somewhere else it
    /// would answer with nothing. A line put away earlier is not touched, since the slot holds one
    /// line and this press is not putting anything into it.
    #[test]
    fn ctrl_s_searches_this_workspace_while_an_older_prompt_is_shown() {
        let mut session = Session::new("none");
        for (workspace, prompt) in [
            ("/work/elsewhere", "why is the picker slow"),
            ("/work/here", "commit that"),
            ("/work/here", "run the tests"),
        ] {
            session.now_in_workspace(workspace);
            type_line(&mut session, prompt);
            handle_key(&mut session, key(KeyCode::Enter));
            session.complete("ok", Vec::new(), 0);
        }

        type_line(&mut session, "half a thought");
        handle_key(&mut session, ctrl('s'));
        assert_eq!(
            session.stashed(),
            Some("half a thought"),
            "the line was not put away"
        );

        handle_key(&mut session, key(KeyCode::Up));
        assert_eq!(
            session.input(),
            "run the tests",
            "the history was not walked"
        );

        assert_eq!(handle_key(&mut session, ctrl('s')), Action::Redraw);
        let search = session.history_search().expect("the search did not open");
        assert!(search.here(), "the search opened over every workspace");
        assert_eq!(
            search.needle(),
            "",
            "the search opened looking for the prompt in the box"
        );
        assert_eq!(
            session
                .history_matches()
                .iter()
                .map(|entry| entry.prompt.as_str())
                .collect::<Vec<_>>(),
            ["commit that", "run the tests"],
            "not this workspace's prompts"
        );
        assert_eq!(
            session.stashed(),
            Some("half a thought"),
            "the line put away earlier is gone"
        );
    }

    /// Somebody who typed half a prompt and then reached for the history has already said what
    /// they are looking for.
    #[test]
    fn the_search_starts_from_what_was_already_typed() {
        let mut session = having_sent(&["run the tests", "commit that"]);
        type_line(&mut session, "tests");
        handle_key(&mut session, ctrl('r'));

        assert_eq!(
            session.history_match().expect("a prompt").prompt,
            "run the tests"
        );
    }

    /// A prompt walked back to is not something anybody typed to look for it. Seeded with it, the
    /// search answers with the one prompt already in the box, which is the walk it exists to take
    /// somebody off: the hint offering the search is drawn in that very state.
    #[test]
    fn the_search_does_not_start_from_a_prompt_walked_back_to() {
        let mut session = having_sent(&["run the tests", "commit that"]);
        handle_key(&mut session, key(KeyCode::Up));
        assert_eq!(session.input(), "commit that", "the history was not walked");

        handle_key(&mut session, ctrl('r'));
        assert_eq!(
            session.history_search().expect("a search").needle(),
            "",
            "the search opened looking for the prompt in the box"
        );
        assert_eq!(
            session.history_matches().len(),
            2,
            "prompts are missing from the list"
        );
    }

    /// While the search is open a letter narrows the list. Typed into the box instead it would go
    /// somewhere nobody can see, and the list would never narrow.
    #[test]
    fn a_letter_narrows_the_search_rather_than_reaching_the_box() {
        let mut session = having_sent(&["run the tests", "commit that"]);
        handle_key(&mut session, ctrl('r'));
        type_line(&mut session, "run");

        assert!(session.input().is_empty(), "the box took the letters");
        assert_eq!(
            session.history_match().expect("a prompt").prompt,
            "run the tests"
        );
    }

    /// Into the box rather than into a request: a history file can be edited, on a shared machine
    /// by somebody else, so the keystroke that sends a stored line has to be the person's own.
    #[test]
    fn enter_puts_the_chosen_prompt_in_the_box_without_sending_it() {
        let mut session = having_sent(&["run the tests", "commit that"]);
        handle_key(&mut session, ctrl('r'));
        type_line(&mut session, "run");
        let action = handle_key(&mut session, key(KeyCode::Enter));

        assert_eq!(action, Action::Redraw, "the prompt was sent");
        assert!(!session.searching_history());
        assert_eq!(session.input(), "run the tests");
        assert_eq!(
            session.status,
            Status::Idle,
            "a turn was started from the list"
        );
    }

    /// Escape means "stop what is happening", and what is happening is the search. The line in the
    /// box was never taken away, so there is nothing to put back.
    #[test]
    fn escape_closes_the_search_and_leaves_the_box_as_it_was() {
        let mut session = having_sent(&["run the tests"]);
        type_line(&mut session, "half a thought");
        handle_key(&mut session, ctrl('r'));
        handle_key(&mut session, key(KeyCode::Esc));

        assert!(!session.searching_history());
        assert_eq!(session.input(), "half a thought");
    }

    /// Backspacing past the start leaves, which is what the key means once there is nothing left of
    /// the thing it deletes. The same ladder the scroller's search walks.
    #[test]
    fn backspacing_past_the_start_of_the_search_closes_it() {
        let mut session = having_sent(&["run the tests"]);
        handle_key(&mut session, ctrl('r'));
        type_line(&mut session, "ru");

        handle_key(&mut session, key(KeyCode::Backspace));
        assert!(session.searching_history());
        handle_key(&mut session, key(KeyCode::Backspace));
        assert!(session.searching_history());
        handle_key(&mut session, key(KeyCode::Backspace));
        assert!(!session.searching_history(), "the search would not close");
    }

    /// The arrows walk the matches while the search is open, rather than the history behind it or
    /// the transcript behind that.
    #[test]
    fn the_arrows_walk_the_matches_while_the_search_is_open() {
        let mut session = having_sent(&["first question", "second question"]);
        handle_key(&mut session, ctrl('r'));
        handle_key(&mut session, key(KeyCode::Up));

        assert_eq!(
            session.history_match().expect("a prompt").prompt,
            "first question"
        );
        assert_eq!(session.scroll, 0, "the transcript scrolled instead");
        assert!(!session.history.is_browsing(), "the box walked the history");
    }

    /// With nothing typed the prompt has nowhere to go, so the keys are the transcript's, which is
    /// what they were before there was a caret to move.
    #[test]
    fn page_keys_scroll_further() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::PageUp));
        assert_eq!(session.scroll, 10);
        handle_key(&mut session, key(KeyCode::PageDown));
        assert_eq!(session.scroll, 0);
    }

    /// The first press takes the start of the line and the second the line before it, so someone
    /// who wanted this line's start gets it without losing their place in the paragraph.
    #[test]
    fn page_up_takes_the_start_of_the_line_then_the_line_before() {
        let mut session = Session::new("none");
        handle_paste(&mut session, "first line\nsecond line\nthird line");

        handle_key(&mut session, key(KeyCode::PageUp));
        assert_eq!(session.caret(), "first line\nsecond line\n".len());
        handle_key(&mut session, key(KeyCode::PageUp));
        assert_eq!(session.caret(), "first line\n".len());
        handle_key(&mut session, key(KeyCode::PageUp));
        assert_eq!(session.caret(), 0);
    }

    /// And the same downwards, by line ends.
    #[test]
    fn page_down_takes_the_end_of_the_line_then_the_line_after() {
        let mut session = Session::new("none");
        handle_paste(&mut session, "first line\nsecond line\nthird line");
        // Up to the very start, which is where paging down has somewhere to go from.
        for _ in 0..3 {
            handle_key(&mut session, key(KeyCode::PageUp));
        }
        assert_eq!(session.caret(), 0);

        handle_key(&mut session, key(KeyCode::PageDown));
        assert_eq!(session.caret(), "first line".len());
        handle_key(&mut session, key(KeyCode::PageDown));
        assert_eq!(session.caret(), "first line\nsecond line".len());
    }

    /// Off the ends of the prompt the keys go back to the transcript, so a paragraph does not trap
    /// them and scrolling stays reachable.
    #[test]
    fn the_page_keys_reach_the_transcript_from_the_ends_of_the_prompt() {
        let mut session = Session::new("none");
        handle_paste(&mut session, "one\ntwo");

        // The caret starts at the end, so the start of "two" and then the start of "one".
        handle_key(&mut session, key(KeyCode::PageUp));
        handle_key(&mut session, key(KeyCode::PageUp));
        assert_eq!(session.caret(), 0);

        handle_key(&mut session, key(KeyCode::PageUp));
        assert_eq!(session.scroll, 10, "the transcript was unreachable");

        // And back down: the end of "one", then the end of "two".
        handle_key(&mut session, key(KeyCode::PageDown));
        handle_key(&mut session, key(KeyCode::PageDown));
        assert_eq!(session.caret(), session.input().len());

        handle_key(&mut session, key(KeyCode::PageDown));
        assert_eq!(session.scroll, 0);
    }

    /// A single-line prompt has ends too, so the keys reach them before the transcript.
    #[test]
    fn the_page_keys_work_on_one_line() {
        let mut session = typed_into("a sentence");
        handle_key(&mut session, key(KeyCode::PageUp));
        assert_eq!(session.caret(), 0);
        assert_eq!(session.scroll, 0, "the transcript scrolled instead");
        handle_key(&mut session, key(KeyCode::PageDown));
        assert_eq!(session.caret(), "a sentence".len());
    }

    /// Under Ctrl, because the bare keys belong to the line being typed.
    #[test]
    fn home_and_end_jump_to_the_extremes() {
        let mut session = Session::new("none");
        handle_key(&mut session, ctrl_key(KeyCode::Home));
        assert_eq!(session.scroll, u16::MAX);
        handle_key(&mut session, ctrl_key(KeyCode::End));
        assert_eq!(session.scroll, 0);
    }

    /// The keys that move around a line of text, which is what makes the box editable rather than
    /// only appendable.
    #[test]
    fn the_arrows_move_the_caret_through_the_line() {
        let mut session = typed_into("abc");
        assert_eq!(session.caret(), 3);

        assert_eq!(handle_key(&mut session, key(KeyCode::Left)), Action::Redraw);
        assert_eq!(session.caret(), 2);
        handle_key(&mut session, key(KeyCode::Right));
        assert_eq!(session.caret(), 3);

        // And they stop at the ends rather than wrapping or panicking.
        handle_key(&mut session, key(KeyCode::Right));
        assert_eq!(session.caret(), 3);
        for _ in 0..5 {
            handle_key(&mut session, key(KeyCode::Left));
        }
        assert_eq!(session.caret(), 0);
    }

    /// The bug this exists for: typing used to only ever append, so a correction meant deleting
    /// back to it and retyping the rest.
    #[test]
    fn typing_lands_where_the_caret_is() {
        let mut session = typed_into("ac");
        handle_key(&mut session, key(KeyCode::Left));
        handle_key(&mut session, key(KeyCode::Char('b')));
        assert_eq!(session.input(), "abc");
        assert_eq!(
            session.caret(),
            2,
            "the caret did not follow what was typed"
        );
    }

    /// Home and End reach the ends of the line, which is what they do in every other text field.
    #[test]
    fn home_and_end_reach_the_ends_of_the_line() {
        let mut session = typed_into("a sentence");
        handle_key(&mut session, key(KeyCode::Home));
        assert_eq!(session.caret(), 0);
        assert_eq!(session.scroll, 0, "the transcript scrolled instead");
        handle_key(&mut session, key(KeyCode::End));
        assert_eq!(session.caret(), "a sentence".len());
    }

    /// Backspace deletes before the caret rather than at the end of the line.
    #[test]
    fn backspace_deletes_at_the_caret() {
        let mut session = typed_into("abc");
        handle_key(&mut session, key(KeyCode::Left));
        handle_key(&mut session, key(KeyCode::Backspace));
        assert_eq!(session.input(), "ac");
        assert_eq!(session.caret(), 1);
    }

    /// Delete takes the character after the caret, which is the half Backspace cannot reach.
    #[test]
    fn delete_takes_the_character_after_the_caret() {
        let mut session = typed_into("abc");
        handle_key(&mut session, key(KeyCode::Home));
        handle_key(&mut session, key(KeyCode::Delete));
        assert_eq!(session.input(), "bc");
        assert_eq!(session.caret(), 0);
    }

    /// A word at a time, since a path or a flag is one thing to cross rather than a dozen.
    #[test]
    fn the_word_keys_cross_a_word_at_a_time() {
        let mut session = typed_into("read src/main.rs now");
        handle_key(&mut session, ctrl_key(KeyCode::Left));
        assert_eq!(session.caret(), "read src/main.rs ".len());
        handle_key(&mut session, ctrl_key(KeyCode::Left));
        assert_eq!(session.caret(), "read ".len());
        handle_key(&mut session, ctrl_key(KeyCode::Right));
        assert_eq!(session.caret(), "read src/main.rs".len());
    }

    /// The readline bindings, because a terminal may send nothing at all for the named keys and
    /// then the middle of a line would be unreachable.
    #[test]
    fn the_readline_bindings_move_and_delete_too() {
        let mut session = typed_into("some words here");
        handle_key(&mut session, ctrl('a'));
        assert_eq!(session.caret(), 0);
        handle_key(&mut session, ctrl('e'));
        assert_eq!(session.caret(), "some words here".len());

        handle_key(&mut session, ctrl('w'));
        assert_eq!(session.input(), "some words ");
        handle_key(&mut session, ctrl('u'));
        assert_eq!(session.input(), "");
    }

    /// Ctrl-K takes the rest of the line, which is the other half of Ctrl-U.
    #[test]
    fn ctrl_k_takes_the_rest_of_the_line() {
        let mut session = typed_into("keep this drop that");
        for _ in 0..2 {
            handle_key(&mut session, ctrl_key(KeyCode::Left));
        }
        handle_key(&mut session, ctrl('k'));
        assert_eq!(session.input(), "keep this ");
    }

    /// The keys that used to be typed as characters, and now must not be: Ctrl-A on an empty line
    /// once inserted a literal 'a'.
    #[test]
    fn the_editing_keys_are_not_typed_as_characters() {
        let mut session = Session::new("none");
        for binding in ['a', 'e', 'b', 'f', 'w', 'u', 'k'] {
            handle_key(&mut session, ctrl(binding));
        }
        assert!(
            session.input().is_empty(),
            "a binding was typed: {}",
            session.input()
        );
    }

    /// A pasted paragraph has rows, and Up and Down move between them before they reach for the
    /// history: a line the user can see is the one they meant to edit.
    #[test]
    fn up_and_down_move_within_a_pasted_paragraph() {
        let mut session = Session::new("none");
        handle_paste(&mut session, "first line\nsecond row");
        assert_eq!(handle_key(&mut session, key(KeyCode::Up)), Action::Redraw);
        assert_eq!(session.caret(), "first line".len());
        handle_key(&mut session, key(KeyCode::Down));
        assert_eq!(session.caret(), "first line\nsecond row".len());
    }

    /// The caret keeps its place along the line, and clamps to the end of a shorter one rather
    /// than off it.
    #[test]
    fn moving_between_lines_keeps_the_place_along_them() {
        let mut session = Session::new("none");
        handle_paste(&mut session, "a longer first line\nshort\nlast");
        handle_key(&mut session, key(KeyCode::Home));
        for _ in 0..3 {
            handle_key(&mut session, key(KeyCode::Right));
        }
        handle_key(&mut session, key(KeyCode::Up));
        assert_eq!(session.caret(), "a longer first line\nsho".len());

        // "last" is shorter than where the caret is, so it clamps to the end of it.
        handle_key(&mut session, key(KeyCode::End));
        handle_key(&mut session, key(KeyCode::Down));
        handle_key(&mut session, key(KeyCode::Down));
        assert_eq!(session.caret(), session.input().len());
    }

    /// Off the top of the line, Up is the history's again, so a one-line prompt is unaffected and
    /// a paragraph does not trap the keys.
    #[test]
    fn up_reaches_history_from_the_top_of_the_line() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('a')));
        handle_key(&mut session, key(KeyCode::Enter));
        session.complete("ok", Vec::new(), 0);

        handle_paste(&mut session, "one\ntwo");
        handle_key(&mut session, key(KeyCode::Up));
        handle_key(&mut session, key(KeyCode::Up));
        assert!(session.history.is_browsing(), "history was unreachable");
    }

    /// The line can be typed mid-turn, so it has to be editable mid-turn: a box that takes words
    /// and will not let them be fixed is worse than one that takes none.
    #[test]
    fn the_caret_moves_while_a_turn_runs() {
        let mut session = typed_into("ac");
        handle_key(&mut session, key(KeyCode::Enter));
        for c in "ac".chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }

        handle_key_while_working(&mut session, key(KeyCode::Left));
        handle_key_while_working(&mut session, key(KeyCode::Char('b')));
        assert_eq!(session.input(), "abc");
    }

    /// The wheel is what most people reach for, so it scrolls with no modifier.
    #[test]
    fn the_mouse_wheel_scrolls() {
        let mut session = Session::new("none");
        let wheel = |kind| MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };

        assert_eq!(
            handle_mouse(&mut session, wheel(MouseEventKind::ScrollUp)),
            Action::Redraw
        );
        assert_eq!(session.scroll, 3);
        handle_mouse(&mut session, wheel(MouseEventKind::ScrollDown));
        assert_eq!(session.scroll, 0);
    }

    /// Clicks and drags are not bound to anything, so they must be ignored rather than
    /// misinterpreted as scrolling.
    #[test]
    fn other_mouse_events_are_ignored() {
        let mut session = Session::new("none");
        let moved = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(handle_mouse(&mut session, moved), Action::None);
        assert_eq!(session.scroll, 0);
    }

    /// Typing must not be captured by the scroll bindings.
    #[test]
    fn scroll_keys_do_not_disturb_the_input() {
        let mut session = Session::new("none");
        for c in "hello".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        handle_key(&mut session, key(KeyCode::Up));
        handle_key(&mut session, key(KeyCode::Down));
        assert_eq!(session.input(), "hello", "scrolling altered the input");
    }

    #[test]
    fn backspace_deletes() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('a')));
        handle_key(&mut session, key(KeyCode::Backspace));
        assert!(session.input().is_empty());
    }

    /// The ladder the key walks, from the box. Three rungs where the flag was not given, and back to
    /// asking: a key that stopped cycling would leave somebody in a mode they could not press their
    /// way out of.
    #[test]
    fn shift_tab_cycles_the_permission_mode() {
        use bravebot_agent::PermissionMode;
        let mut session = Session::new("none");
        assert_eq!(session.permission_mode(), PermissionMode::Ask);

        for expected in [
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
            PermissionMode::Ask,
        ] {
            handle_key(&mut session, shift(KeyCode::Tab));
            assert_eq!(session.permission_mode(), expected);
        }
    }

    /// Which spelling arrives is the terminal's choice: one that has been asked to disambiguate sends
    /// Tab with a modifier, one that has not sends `BackTab`. A binding that worked on one machine
    /// and not the next would read as broken.
    #[test]
    fn either_spelling_of_shift_tab_cycles_the_mode() {
        use bravebot_agent::PermissionMode;
        for press in [shift(KeyCode::Tab), key(KeyCode::BackTab)] {
            let mut session = Session::new("none");
            handle_key(&mut session, press);
            assert_eq!(
                session.permission_mode(),
                PermissionMode::AcceptEdits,
                "{press:?} did not cycle the mode"
            );
        }
    }

    /// The mode key must not type anything, and must not be taken for the completion Tab: a press
    /// that changed the mode *and* accepted a half-typed command would do two things at once.
    #[test]
    fn the_mode_key_leaves_the_line_alone() {
        let mut session = typed_into("/mod");
        handle_key(&mut session, shift(KeyCode::Tab));
        assert_eq!(
            session.input(),
            "/mod",
            "the mode key completed the line or typed into it"
        );
    }

    /// Bypass is reachable only where `--dangerously-skip-permissions` was given. Without it the
    /// rung does not exist, however many times the key is pressed, or the flag would be decorative.
    #[test]
    fn the_key_cannot_reach_bypass_without_the_flag() {
        use bravebot_agent::PermissionMode;
        let mut session = Session::new("none");
        for _ in 0..12 {
            handle_key(&mut session, shift(KeyCode::Tab));
            assert_ne!(session.permission_mode(), PermissionMode::Bypass);
        }
    }

    /// The flag opens the session in bypass and puts that rung on the ladder. Honouring only the
    /// second would make the flag do nothing a person could see.
    #[test]
    fn the_flag_opens_the_session_in_bypass_and_can_be_cycled_out_of() {
        use bravebot_agent::PermissionMode;
        let mut session = Session::new("none").allowing_bypass();
        assert_eq!(session.permission_mode(), PermissionMode::Bypass);

        // Out of it, round the ladder, and back: the key means the same thing wherever it started.
        for expected in [
            PermissionMode::Ask,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
            PermissionMode::Bypass,
        ] {
            handle_key(&mut session, shift(KeyCode::Tab));
            assert_eq!(session.permission_mode(), expected);
        }
    }

    /// Most wanted mid-turn, which is when somebody watching a turn edit the wrong files decides the
    /// next one should stop and ask. The turn in flight keeps the mode its confirmer was built with.
    #[test]
    fn the_mode_can_be_changed_while_a_turn_runs() {
        use bravebot_agent::PermissionMode;
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('a')));
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        handle_key(&mut session, shift(KeyCode::Tab));
        assert_eq!(session.permission_mode(), PermissionMode::AcceptEdits);
    }

    /// Keys that mean nothing here must be ignored rather than mishandled.
    #[test]
    fn unknown_keys_are_ignored() {
        let mut session = Session::new("none");
        assert_eq!(handle_key(&mut session, key(KeyCode::F(5))), Action::None);
        assert_eq!(handle_key(&mut session, key(KeyCode::Insert)), Action::None);
    }

    /// A second submission cannot start while a turn is in flight.
    #[test]
    fn enter_is_inert_while_working() {
        let mut session = Session::new("none");
        handle_key(&mut session, key(KeyCode::Char('a')));
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        handle_key(&mut session, key(KeyCode::Char('b')));
        assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::None);
    }

    /// A directory the person accepted is opened and vouched for by the same route `/add-dir`
    /// takes, so neither has a way in that the other lacks. A relative name means a path under the
    /// workspace, which is what `../shared` in a file about a project says.
    #[test]
    fn an_accepted_directory_is_opened_and_trusted_like_one_typed() {
        let root = crate::testutil::scratch_dir("bravebot-settings-dir-test");
        let outside = root.join("shared");
        let project = root.join("project");
        std::fs::create_dir_all(&outside).expect("scratch");
        std::fs::create_dir_all(&project).expect("scratch");

        let mut workspace = Workspace::new(&project).expect("workspace");
        let mut session = Session::new("none");
        let mut trust = TrustStore::new("/work");

        // Named the way a settings file names it, relative to the project.
        let asked = named_to_open(&mut session, &workspace, &["../shared".to_string()]);
        open_named(&mut session, &mut workspace, &mut trust, &asked);

        // Both halves, since either alone is useless: reach without trust asks about every write
        // there, and trust without reach is a rule about files nothing can open.
        let canonical = outside.canonicalize().expect("canonical");
        assert!(
            workspace.added_directories().contains(&canonical),
            "a directory the person accepted was not opened"
        );
        assert!(
            trust.is_trusted(&canonical.display().to_string()),
            "a directory the person accepted was not vouched for"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// The other half of the answer, asserted where a refusal bites rather than in the list that
    /// was asked about: a name nobody accepted leaves its path unreachable and out of the map, so
    /// reading or writing there is refused as it is anywhere else outside the workspace.
    #[test]
    fn a_declined_directory_is_left_unreachable_and_unvouched_for() {
        let root = crate::testutil::scratch_dir("bravebot-declined-dir-test");
        let accepted = root.join("accepted");
        let declined = root.join("declined");
        let project = root.join("project");
        for directory in [&accepted, &declined, &project] {
            std::fs::create_dir_all(directory).expect("scratch");
        }

        let mut workspace = Workspace::new(&project).expect("workspace");
        let mut session = Session::new("none");
        let mut trust = TrustStore::new("/work");

        let asked = named_to_open(
            &mut session,
            &workspace,
            &["../accepted".to_string(), "../declined".to_string()],
        );
        assert_eq!(asked.len(), 2, "both names were asked about");
        // One question answered yes, the other no.
        open_named(&mut session, &mut workspace, &mut trust, &asked[..1]);

        let yes = accepted.canonicalize().expect("canonical");
        let no = declined.canonicalize().expect("canonical");
        assert!(workspace.confines(&yes.join("notes.md")).is_ok());
        assert!(
            workspace.confines(&no.join("notes.md")).is_err(),
            "a declined directory is still reachable"
        );
        assert!(
            !trust.is_trusted(&no.display().to_string()),
            "a declined directory is still vouched for"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// What the question shows has to be what accepting it would reach. A name is canonicalized
    /// when it is opened, so a settings file naming a link inside the project would otherwise put
    /// a path beginning with the person's own project root in front of them and grant the tree at
    /// the other end of it.
    #[test]
    #[cfg(unix)]
    fn a_named_directory_is_resolved_before_it_is_asked_about() {
        let root = crate::testutil::scratch_dir("bravebot-named-link-test");
        let outside = root.join("outside");
        let project = root.join("project");
        std::fs::create_dir_all(&outside).expect("scratch");
        std::fs::create_dir_all(&project).expect("scratch");
        std::os::unix::fs::symlink(&outside, project.join("link")).expect("symlink");

        let workspace = Workspace::new(&project).expect("workspace");
        let mut session = Session::new("none");

        let asked = named_to_open(&mut session, &workspace, &["link".to_string()]);

        assert_eq!(
            asked,
            vec![
                outside
                    .canonicalize()
                    .expect("canonical")
                    .display()
                    .to_string()
            ],
            "the question would have shown the name rather than the directory"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// Two layers of settings both naming a directory is one request. A second box for a path
    /// already answered is a question whose answer is already known, and answering the same
    /// question twice differently is a contradiction nothing could resolve.
    #[test]
    fn a_directory_two_layers_both_named_is_asked_about_once() {
        let root = crate::testutil::scratch_dir("bravebot-named-twice-test");
        let outside = root.join("shared");
        let project = root.join("project");
        std::fs::create_dir_all(&outside).expect("scratch");
        std::fs::create_dir_all(&project).expect("scratch");

        let workspace = Workspace::new(&project).expect("workspace");
        let mut session = Session::new("none");

        let both = vec!["../shared".to_string(), outside.display().to_string()];
        let asked = named_to_open(&mut session, &workspace, &both);

        assert_eq!(asked.len(), 1, "the same directory was asked about twice");

        std::fs::remove_dir_all(&root).ok();
    }

    /// A name that could not be opened whatever the answer is reported rather than put as a
    /// question: a directory already inside the project is reachable by its relative path, so a
    /// box asking about it collects an answer that changes nothing either way.
    #[test]
    fn a_name_that_cannot_be_opened_is_said_so_rather_than_asked_about() {
        let root = crate::testutil::scratch_dir("bravebot-named-inside-test");
        let project = root.join("project");
        std::fs::create_dir_all(project.join("docs")).expect("scratch");

        let workspace = Workspace::new(&project).expect("workspace");
        let mut session = Session::new("none");

        let asked = named_to_open(&mut session, &workspace, &["docs".to_string()]);

        assert!(
            asked.is_empty(),
            "a name that cannot be opened was asked about"
        );
        assert!(
            session.transcript[0].text.contains("could not add"),
            "nothing said why the directory was not opened: {}",
            session.transcript[0].text
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// The person who was asked about the working directory is asked about each directory a file
    /// named too, because a name in a file is a request for reach and trust rather than a grant of
    /// either, and the file is the easiest thing in a checkout to write to.
    #[test]
    fn a_person_asked_about_the_workspace_is_asked_about_each_named_directory() {
        let named = vec!["/home/me/notes".to_string(), "/home/me/.ssh".to_string()];

        assert_eq!(
            named_directories(Whence::Asked, &named),
            Named::Asking(named)
        );
    }

    /// The mode that answers every permission question answers these as well. It approves every
    /// write, every run and vouching for every file the planner reads, so stopping at a modal box
    /// about a directory would be the one thing it did not answer.
    #[test]
    fn bypassing_opens_the_directories_a_file_named_without_asking() {
        let named = vec!["/home/me/notes".to_string()];

        assert_eq!(
            named_directories(Whence::Unasked, &named),
            Named::Opening(named)
        );
    }

    /// A session resumed with the map its own user left puts no question, and the directories it
    /// has open are the ones its own record reopened. Taking a name from the file as well would
    /// open a directory on behalf of somebody who was never asked, out of a file that may have
    /// been edited since they answered.
    #[test]
    fn a_resume_that_brought_its_own_map_opens_no_directory_a_file_named() {
        let named = vec!["/home/me/notes".to_string()];

        assert_eq!(
            named_directories(Whence::Resumed, &named),
            Named::Opening(Vec::new())
        );
    }

    /// The directory those sessions ran in, which is the one their rules are read under.
    fn here() -> &'static std::path::Path {
        std::path::Path::new("/tmp/x")
    }

    /// A session that ran in this directory and vouched for it, which is what an earlier answer
    /// of yes leaves behind in the directory's list of sessions.
    fn a_record_that_answered_yes_here() -> Box<crate::sessions::Record> {
        Box::new(
            serde_json::from_value(serde_json::json!({
                "id": "1-2",
                "directory": "/tmp/x",
                "title": "an earlier session here",
                "started": 1,
                "updated": 1,
                "trust": [{"path": ".", "integrity": "trusted"}],
                "conversation": {"messages": [], "context": "trusted"},
            }))
            .expect("a record"),
        )
    }

    /// The opening map comes from how this session was started and from nowhere else. A fresh
    /// start brings no record, so the question is put: reading back the yes somebody gave in this
    /// directory last week would grant standing permission over the tree on behalf of a user
    /// nobody asked. `/clear` begins a session too, and reaches this with the same
    /// `Beginning::New`.
    #[test]
    fn a_fresh_session_is_asked_rather_than_inheriting_a_map() {
        use bravebot_agent::PermissionMode;

        assert!(
            matches!(
                opening_for(
                    beginning_of(&Start::Fresh, here()),
                    PermissionMode::Ask,
                    here(),
                ),
                Opening::Ask
            ),
            "a session started fresh took an answer its own user never gave",
        );
    }

    /// A resume is not an exception to that: the answer it honours is the one its own user gave,
    /// and it comes with the rules that session's writes recorded, which is what stops a resumed
    /// turn reading back a file an earlier turn of the same session poisoned.
    #[test]
    fn a_resume_starts_with_the_map_its_own_record_kept() {
        use bravebot_agent::PermissionMode;

        let record = a_record_that_answered_yes_here();
        match opening_for(
            beginning_of(&Start::Resuming(record), here()),
            PermissionMode::Ask,
            here(),
        ) {
            Opening::Settled(trust, Whence::Resumed) => {
                assert!(trust.is_trusted("."));
                assert!(trust.is_trusted("src/main.rs"), "the rule covers the tree");
            }
            settled => panic!("a resume did not take its own record's map: {settled:?}"),
        }
    }

    /// Nothing recorded is not the same as nothing trusted, so a record from before maps were
    /// kept is asked about rather than resumed into a map that vouches for nothing.
    #[test]
    fn a_record_from_before_maps_were_kept_is_asked_about() {
        use bravebot_agent::PermissionMode;

        let mut record = a_record_that_answered_yes_here();
        record.trust = None;

        assert!(
            matches!(
                opening_for(
                    beginning_of(&Start::Resuming(record), here()),
                    PermissionMode::Ask,
                    here(),
                ),
                Opening::Ask
            ),
            "a record that answered nothing was resumed as an answer",
        );
    }

    /// Bypassing answers the question rather than putting it, but a resume is still the more
    /// specific record: the question is not being put in either case, so the mode has nothing to
    /// answer, and a no given inside a trusted tree must survive being picked up again.
    #[test]
    fn a_resume_keeps_its_own_map_even_where_the_mode_would_answer() {
        use bravebot_agent::PermissionMode;

        let mut record = a_record_that_answered_yes_here();
        record.trust = Some(vec![
            crate::sessions::StoredRule {
                path: ".".to_string(),
                integrity: "trusted".to_string(),
            },
            crate::sessions::StoredRule {
                path: "vendor".to_string(),
                integrity: "untrusted".to_string(),
            },
        ]);

        match opening_for(
            beginning_of(&Start::Resuming(record), here()),
            PermissionMode::Bypass,
            here(),
        ) {
            Opening::Settled(trust, Whence::Resumed) => assert!(
                !trust.is_trusted("vendor/lib.js"),
                "the mode's own answer replaced the one the record kept",
            ),
            settled => panic!("a resume was answered by the mode instead: {settled:?}"),
        }
    }

    /// A rewind point read off the live session lands inside the turn it undoes: by the time the
    /// loop has a prompt in hand it is on the transcript and counted as a turn, so rewinding to
    /// such a point would leave the prompt in the scrollback and number the next turn one too high.
    #[test]
    fn a_rewind_point_excludes_the_turn_it_undoes() {
        let root = crate::testutil::scratch_dir("bravebot-app-rewind-point");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create");

        let mut session = Session::new("none");
        let conversation = Conversation::new();
        let trust = TrustStore::new("/work");
        let programs = TrustedPrograms::new();
        let stored = crate::sessions::Handle::begin(&root);

        type_line(&mut session, "delete the tests");
        session.submit().expect("the prompt is sent");

        let first = rewind_point(&session, &conversation, &trust, &programs, &stored);
        assert_eq!(
            first.turns, 0,
            "the first turn rewinds to a session with no turns in it"
        );
        assert_eq!(
            first.transcript_len, 0,
            "the prompt is the turn's, and goes back with it"
        );
        assert!(
            !first.was_wrote,
            "nothing was written before the first turn"
        );

        session.complete("nothing was deleted", Vec::new(), 10);
        type_line(&mut session, "delete the docs");
        session.submit().expect("the prompt is sent");

        let second = rewind_point(&session, &conversation, &trust, &programs, &stored);
        assert_eq!(second.turns, 1, "the turn that finished stays");
        assert_eq!(
            second.transcript_len, 2,
            "the finished turn's prompt and reply stay"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The bare word is the list, which is the surface the command exists for: seeing what a
    /// rewind would put back before running it.
    #[test]
    fn the_bare_rewind_command_asks_for_the_list() {
        let mut session = Session::new("none");
        for c in REWIND_COMMAND.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Rewind(String::new())
        );
    }

    /// A number after the word is how many turns to go back.
    #[test]
    fn the_rewind_command_carries_how_far_back_to_go() {
        let mut session = Session::new("none");
        for c in "/rewind 3".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Rewind("3".to_string())
        );
    }

    /// `/rewinding the tape` is a longer word, so it is a prompt, as CMD-2 has it.
    #[test]
    fn a_longer_word_starting_with_rewind_is_a_prompt() {
        let mut session = Session::new("none");
        for c in "/rewinding the tape".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("/rewinding the tape".to_string())
        );
    }

    /// `/undo` takes no argument, so a sentence that begins with it is still a prompt. The list
    /// and the number went to a word of their own to keep that true.
    #[test]
    fn undo_with_something_after_it_is_still_a_prompt() {
        let mut session = Session::new("none");
        for c in "/undo the last thing I asked for".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit("/undo the last thing I asked for".to_string())
        );
    }

    /// Deciding whether to go back is deciding about the files a turn wrote, so the list names
    /// them, and names what each turn was asked so the rows can be told apart.
    #[test]
    fn the_list_names_what_each_point_would_put_back() {
        let mut session = Session::new("none");
        session.open_rewind_point(a_point_before(0), "add a line to notes.md".into());
        session.keep_backups(vec![bravebot_agent::workspace::Backup {
            path: std::path::PathBuf::from("/work/notes.md"),
            was: bravebot_agent::workspace::Before::Nothing,
        }]);
        session.open_rewind_point(a_point_before(1), "read the notes back".into());

        list_rewind_points(&mut session);

        let said = session
            .transcript
            .iter()
            .map(|entry| entry.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            said.contains("/work/notes.md"),
            "the list did not say which file would go back: {said}"
        );
        assert!(
            said.contains("add a line to notes.md"),
            "the list did not say what the turn was asked: {said}"
        );
        let first = said.find("read the notes back").expect("the newest point");
        let second = said
            .find("add a line to notes.md")
            .expect("the older point");
        assert!(
            first < second,
            "the list did not begin at the turn that just ended: {said}"
        );
    }

    /// A session with nothing to rewind says so, rather than printing an empty heading.
    #[test]
    fn the_list_of_a_session_with_no_points_says_there_is_nothing() {
        let mut session = Session::new("none");

        list_rewind_points(&mut session);

        assert_eq!(session.transcript.len(), 1, "an empty list still had rows");
    }

    /// The state before some turn, for a test that only needs a point to exist.
    fn a_point_before(turns: usize) -> crate::state::TurnSnapshot {
        crate::state::TurnSnapshot {
            conversation: Conversation::new().snapshot(),
            turns,
            tokens: 0,
            spend: std::collections::BTreeMap::new(),
            timing: std::collections::BTreeMap::new(),
            cached: None,
            trust: TrustStore::new("/work"),
            programs: TrustedPrograms::new(),
            transcript_len: turns,
            title: "a session".to_string(),
            was_wrote: true,
        }
    }

    /// Both halves of what `/cd` does, together: the working directory moves, and the directory
    /// moved to is vouched for, which is what a relative path means and what decides a write there.
    #[test]
    fn changing_directory_moves_the_workspace_and_vouches_for_where_it_moved() {
        let root = crate::testutil::scratch_dir("bravebot-cd-test");
        let project = root.join("project");
        let other = root.join("other");
        std::fs::create_dir_all(&project).expect("scratch");
        std::fs::create_dir_all(&other).expect("scratch");

        let mut workspace = Workspace::new(&project).expect("workspace");
        let mut session = Session::new("none");
        let mut trust = TrustStore::new("/work");

        assert!(change_directory(
            &mut session,
            &mut workspace,
            &mut trust,
            &mut None,
            other.to_str().expect("utf-8 path")
        ));

        let canonical = other.canonicalize().expect("canonical");
        assert_eq!(workspace.root(), canonical);
        assert!(
            trust.is_trusted("."),
            "the directory moved to was not vouched for"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// LSP-5 and LSP-8: a server is approved for one tree and indexes that tree, so the working
    /// directory moving leaves it behind.
    ///
    /// Kept across the move it would answer questions about the new directory out of the old one's
    /// index, and the paths it opens would be resolved against a root the session has left. Nobody
    /// approved a server reading that directory from here.
    #[test]
    fn changing_directory_stops_the_servers_started_in_the_old_one() {
        let root = crate::testutil::scratch_dir("bravebot-cd-servers-test");
        let project = root.join("project");
        let other = root.join("other");
        std::fs::create_dir_all(&project).expect("scratch");
        std::fs::create_dir_all(&other).expect("scratch");

        let mut workspace = Workspace::new(&project).expect("workspace");
        let mut session = Session::new("none");
        let mut trust = TrustStore::new(workspace.root());
        // Nothing is started by building the set, which is LSP-8: a server comes up on the first
        // question that needs one. What is being asserted is who holds the set afterwards.
        let mut servers = Some(LanguageServers::new(workspace.root().to_path_buf(), None));

        assert!(change_directory(
            &mut session,
            &mut workspace,
            &mut trust,
            &mut servers,
            other.to_str().expect("utf-8 path")
        ));

        assert!(
            servers.is_none(),
            "the servers of the directory left behind are still the session's"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// The map goes with the session, so it has to keep saying what it said about the same files
    /// once the working directory has moved. A yes given for the directory left behind must not
    /// become a yes for the one arrived at, which is what carrying it across unchanged would do.
    #[test]
    fn changing_directory_leaves_the_previous_answer_where_it_was_given() {
        let root = crate::testutil::scratch_dir("bravebot-cd-rules-test");
        let project = root.join("project");
        let other = root.join("other");
        std::fs::create_dir_all(project.join("vendor")).expect("scratch");
        std::fs::create_dir_all(&other).expect("scratch");

        let mut workspace = Workspace::new(&project).expect("workspace");
        let mut session = Session::new("none");
        let mut trust = TrustStore::new(workspace.root());
        trust.trust(".");
        trust.distrust("vendor");

        let left = workspace.root().to_path_buf();
        assert!(change_directory(
            &mut session,
            &mut workspace,
            &mut trust,
            &mut None,
            other.to_str().expect("utf-8 path")
        ));

        assert!(
            trust.is_trusted(&left.display().to_string()),
            "the answer given for the directory left behind was forgotten"
        );
        assert!(
            !trust.is_trusted(&left.join("vendor").display().to_string()),
            "a no given inside the directory left behind was forgotten"
        );
        assert_eq!(
            trust.integrity_of("vendor"),
            Some(bravebot_core::label::Integrity::Trusted),
            "a rule about the old vendor directory decided a path in the new one"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// Moving into a directory keeps the nos given inside it. They are about the same files after
    /// the move as before, and vouching for the directory moved to is not vouching for a subtree
    /// somebody has already refused.
    #[test]
    fn moving_into_a_directory_keeps_the_answers_given_inside_it() {
        let root = crate::testutil::scratch_dir("bravebot-cd-inner-test");
        let project = root.join("project");
        std::fs::create_dir_all(project.join("src/vendor")).expect("scratch");

        let mut workspace = Workspace::new(&project).expect("workspace");
        let mut session = Session::new("none");
        let mut trust = TrustStore::new(workspace.root());
        trust.trust(".");
        trust.distrust("src/vendor");

        assert!(change_directory(
            &mut session,
            &mut workspace,
            &mut trust,
            &mut None,
            "src"
        ));

        assert!(
            trust.is_trusted("main.rs"),
            "the directory moved into was not vouched for"
        );
        assert!(
            !trust.is_trusted("vendor/lib.js"),
            "an untrusted subtree became trusted by moving into its parent"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// A path that names nothing moves nothing, and says so. Half a move would leave the session
    /// vouching for a directory it is not in.
    #[test]
    fn a_directory_that_is_not_there_moves_nothing() {
        let root = crate::testutil::scratch_dir("bravebot-cd-missing-test");
        std::fs::create_dir_all(&root).expect("scratch");

        let mut workspace = Workspace::new(&root).expect("workspace");
        let mut session = Session::new("none");
        let mut trust = TrustStore::new("/work");
        let before = workspace.root().to_path_buf();

        assert!(!change_directory(
            &mut session,
            &mut workspace,
            &mut trust,
            &mut None,
            "nowhere"
        ));
        assert_eq!(workspace.root(), before);
        assert!(trust.is_empty(), "a refused move vouched for something");

        std::fs::remove_dir_all(&root).ok();
    }

    /// An absolute name is left alone. Joining it onto the workspace would name a path inside the
    /// project that nobody asked for.
    #[test]
    fn an_absolute_directory_in_a_settings_file_is_not_joined_onto_the_workspace() {
        assert_eq!(
            against_workspace(std::path::Path::new("/tmp/project"), "/opt/other"),
            "/opt/other"
        );
    }

    /// A workspace for the tests below, which need one only because a finished turn takes the
    /// first look at anything it asked to have watched.
    fn workspace_for_test() -> Workspace {
        let root = crate::testutil::scratch_dir("bravebot-fold-outcome-workspace");
        std::fs::create_dir_all(&root).expect("scratch");
        Workspace::new(&root).expect("workspace")
    }

    #[test]
    fn a_failed_turn_measures_context_if_requests_were_sent() {
        let mut session = Session::new("none");
        let sink = Trail::new();
        let fallback = TrustStore::new("/work");
        let fallback_programs = TrustedPrograms::new();
        let asked = Asked {
            name: "test-model".to_string(),
            comparable: true,
        };

        fold_outcome(
            &mut session,
            Err(turn::TurnError::Precommit("failed".to_string())),
            sink,
            Carried {
                trust: fallback,
                programs: fallback_programs,
                asked: AskedAbout::new(),
            },
            Occupied {
                budget: 100_000,
                guessed: false,
                last_request_tokens: 45_000,
            },
            asked,
            Line {
                text: "",
                wrote: Wrote::ThePerson,
            },
            &workspace_for_test(),
        );

        assert_eq!(
            session.occupancy(),
            crate::state::Occupancy::Measured {
                used: 45_000,
                budget: 100_000,
                guessed: false,
            }
        );
        assert_eq!(session.fullness(), Some(45));
    }

    /// A loop repeats a line somebody endorsed. The sentence this program writes to carry a goal
    /// on is not one, so a turn running that sentence and asking for a later look gets no loop:
    /// otherwise the driver's own words would come back every quarter of an hour, and the person
    /// would be reading a watch nobody set up over a line nobody typed.
    #[test]
    fn only_a_line_the_person_wrote_becomes_a_watch_the_turn_asked_for() {
        let wakeup = turn::Wakeup::asked(900, false);
        assert_eq!(
            watch_to_start(Some(wakeup), Wrote::ThePerson),
            Some(wakeup),
            "a turn on the person's own line could not arrange a later look"
        );
        assert_eq!(watch_to_start(Some(wakeup), Wrote::TheDriver), None);
        assert_eq!(watch_to_start(None, Wrote::ThePerson), None);
    }

    /// A goal is a condition for a session and not for one turn, so stopping a turn going the
    /// wrong way has to leave it: a person who has to retype the condition every time they
    /// interrupt cannot steer the work at all. The stopped turn is recorded as stopped, which is
    /// what keeps it from being judged and sent straight back: there is no answer to judge.
    #[test]
    fn stopping_a_turn_leaves_the_goal_set() {
        let mut session = Session::new("none");
        session.start_goal("cargo test exits 0".to_string());
        let asked = Asked {
            name: "test-model".to_string(),
            comparable: true,
        };

        fold_outcome(
            &mut session,
            Err(turn::TurnError::Cancelled { attempts: None }),
            Trail::new(),
            Carried {
                trust: TrustStore::new("/work"),
                programs: TrustedPrograms::new(),
                asked: AskedAbout::new(),
            },
            Occupied {
                budget: 100_000,
                guessed: false,
                last_request_tokens: 0,
            },
            asked,
            Line {
                text: "",
                wrote: Wrote::ThePerson,
            },
            &workspace_for_test(),
        );

        assert_eq!(
            session.goal().map(crate::goals::Running::condition),
            Some("cargo test exits 0"),
            "the stop took the goal off with the turn"
        );
        assert_eq!(
            session.finished.map(|turn| turn.ending),
            Some(bravebot_agent::Ending::Stopped { attempts: None }),
            "a stop was not recorded as one"
        );
        assert!(
            session
                .finished
                .is_some_and(|turn| turn.ending.unanswered()),
            "a stopped turn looked answered, so it would be judged"
        );
    }

    /// A request that never came back says nothing about whether the work is finished, so the
    /// goal waits for a turn there is something to judge rather than being thrown away.
    #[test]
    fn a_turn_that_failed_leaves_the_goal_where_it_was() {
        let mut session = Session::new("none");
        session.start_goal("cargo test exits 0".to_string());
        let asked = Asked {
            name: "test-model".to_string(),
            comparable: true,
        };

        fold_outcome(
            &mut session,
            Err(turn::TurnError::Precommit("failed".to_string())),
            Trail::new(),
            Carried {
                trust: TrustStore::new("/work"),
                programs: TrustedPrograms::new(),
                asked: AskedAbout::new(),
            },
            Occupied {
                budget: 100_000,
                guessed: false,
                last_request_tokens: 0,
            },
            asked,
            Line {
                text: "",
                wrote: Wrote::ThePerson,
            },
            &workspace_for_test(),
        );

        assert!(
            session.goal().is_some(),
            "one failed request ended the goal"
        );
    }

    #[test]
    fn a_failed_turn_with_no_requests_sent_remains_unmeasured() {
        let mut session = Session::new("none");
        let sink = Trail::new();
        let fallback = TrustStore::new("/work");
        let fallback_programs = TrustedPrograms::new();
        let asked = Asked {
            name: "test-model".to_string(),
            comparable: true,
        };

        fold_outcome(
            &mut session,
            Err(turn::TurnError::Precommit("failed".to_string())),
            sink,
            Carried {
                trust: fallback,
                programs: fallback_programs,
                asked: AskedAbout::new(),
            },
            Occupied {
                budget: 100_000,
                guessed: false,
                last_request_tokens: 0,
            },
            asked,
            Line {
                text: "",
                wrote: Wrote::ThePerson,
            },
            &workspace_for_test(),
        );

        assert_eq!(session.occupancy(), crate::state::Occupancy::Unmeasured);
        assert_eq!(session.fullness(), None);
    }

    #[test]
    fn failure_reporting_uses_safe_fields_in_the_transcript_and_status() {
        let mut session = Session::new("none");
        type_line(&mut session, "read a file");
        session.submit().unwrap();
        let error = bravebot_agent::backend::BackendError::Attempted {
            attempts: 3,
            cause: Box::new(bravebot_agent::backend::BackendError::from(
                bravebot_aichat::ChatError::Egress(bravebot_net::EgressError::Status {
                    url: "https://example.test/PRIVATE_PATH?key=PRIVATE_QUERY".into(),
                    status: 503,
                }),
            )),
        };
        fold_outcome(
            &mut session,
            Err(turn::TurnError::from(error)),
            Trail::new(),
            Carried {
                asked: Default::default(),
                trust: TrustStore::new("/work"),
                programs: TrustedPrograms::new(),
            },
            Occupied {
                budget: 1000,
                guessed: false,
                last_request_tokens: 0,
            },
            Asked {
                name: "test-model".into(),
                comparable: true,
            },
            Line {
                text: "read a file",
                wrote: Wrote::ThePerson,
            },
            &workspace_for_test(),
        );
        let reason = session.failure_said().expect("visible reason");
        assert!(reason.contains("503"), "{reason}");
        assert!(reason.contains("3 attempts"), "{reason}");
        let exported = render::as_markdown(&session, "test");
        for secret in ["PRIVATE_PATH", "PRIVATE_QUERY", "example.test"] {
            assert!(!reason.contains(secret), "{reason}");
            assert!(!exported.contains(secret), "{exported}");
        }
        assert_eq!(exported.matches("503").count(), 1);
        assert!(session.finished.unwrap().failed());
    }
}
