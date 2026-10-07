//! The terminal's title, named after the session and marked with what it is doing, so a row of tabs
//! running this can be told apart and the one waiting for an answer found.
//!
//! The title the terminal had is pushed before the first one is written and popped whenever the
//! terminal is handed back, so the shell gets its own title again. The title is emptied just before
//! the pop, so a terminal that keeps no stack of titles is not left naming a session that ended.

use std::io::{self, Write};
use std::sync::{Mutex, PoisonError};

/// What every title starts with, so a tab running this is recognisable before its name is read.
const PREFIX: &str = "bravebot";

/// The most columns a title takes, since a tab bar shows a fraction of a long one anyway.
const MOST_COLUMNS: usize = 60;

/// xterm's request to push the window and icon titles onto its stack.
const PUSH: &str = "\x1b[22;0t";

/// And to pop them back off.
const POP: &str = "\x1b[23;0t";

/// An empty title, written before the pop for a terminal that ignores the pop.
const EMPTY: &str = "\x1b]0;\x07";

/// What the session is doing, as the title says it. Chosen by the driver from its own state: whether
/// a turn is running or a prompt to the person is open, and never from anything a turn produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    /// Nothing is running and nothing is asked: the box is waiting for a prompt.
    Ready,
    /// A turn or a command is running.
    Working,
    /// An approval, a question or a confirmation is open and the run is waiting for the person.
    Waiting,
}

impl State {
    /// The fixed text in front of the title, so a tab bar that cuts the title still shows it.
    const fn marker(self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::Working => Some("✦"),
            Self::Waiting => Some("[!]"),
        }
    }
}

/// One run's worth of title, kept between frames so an unchanged title is not written again.
#[derive(Debug)]
pub(crate) struct Title {
    on: bool,
    /// The session's name as last given, trimmed.
    name: String,
    state: State,
    /// Whether a push is outstanding, which is the same as the terminal showing a title written here.
    pushed: bool,
    /// The text last written, kept across a handover so the title can be put back afterwards.
    shown: Option<String>,
}

impl Title {
    pub(crate) const fn new() -> Self {
        Self {
            on: true,
            name: String::new(),
            state: State::Ready,
            pushed: false,
            shown: None,
        }
    }

    /// Write the title for `name` where it differs from what the terminal shows.
    ///
    /// Nothing is written until a session has a name or something to say about its state. A session
    /// that loses its name, as `/clear` does, is given the bare prefix rather than left showing the
    /// name of the one before it.
    pub(crate) fn show<W: Write>(&mut self, out: &mut W, name: &str) -> io::Result<()> {
        self.name = name.trim().to_string();
        self.write(out)
    }

    /// Write the title for `state` where it differs from what the terminal shows.
    ///
    /// Called on every frame, so an unchanged state writes nothing: a title rewritten while a turn
    /// runs makes some terminals tick, and none of them needs it.
    pub(crate) fn mark<W: Write>(&mut self, out: &mut W, state: State) -> io::Result<()> {
        self.state = state;
        self.write(out)
    }

    fn write<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        if !self.on {
            return Ok(());
        }
        let text = match self.name.as_str() {
            "" if self.shown.is_none() && self.state == State::Ready => return Ok(()),
            "" => marked(PREFIX.to_string(), self.state),
            name => text(name, self.state),
        };
        if self.pushed && self.shown.as_deref() == Some(text.as_str()) {
            return Ok(());
        }
        if !self.pushed {
            out.write_all(PUSH.as_bytes())?;
            self.pushed = true;
        }
        write!(out, "\x1b]0;{text}\x07")?;
        out.flush()?;
        self.shown = Some(text);
        Ok(())
    }

    /// Put back the title the terminal had before the first one written here.
    pub(crate) fn give_back<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        if !self.pushed {
            return Ok(());
        }
        out.write_all(EMPTY.as_bytes())?;
        out.write_all(POP.as_bytes())?;
        out.flush()?;
        self.pushed = false;
        Ok(())
    }
}

/// The title for a session named `name` in `state`, safe to write inside an escape sequence.
///
/// Control characters are replaced by visible ones, as they are anywhere on the screen: an escape or
/// a bell in a name would otherwise end the sequence early and let the rest act on the terminal.
pub(crate) fn text(name: &str, state: State) -> String {
    marked(
        format!("{PREFIX} · {}", crate::render::printable(name)),
        state,
    )
}

/// `title` behind the marker for `state`, cut to the columns a title takes with the marker counted.
fn marked(title: String, state: State) -> String {
    let title = match state.marker() {
        Some(marker) => format!("{marker} {title}"),
        None => title,
    };
    crate::status::cut(&title, MOST_COLUMNS)
}

/// The run's title. One per process, because one process owns the terminal it is the title of.
static TITLE: Mutex<Title> = Mutex::new(Title::new());

fn title() -> std::sync::MutexGuard<'static, Title> {
    TITLE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Take what the settings say for this run.
pub(crate) fn adopt(setting: Option<bool>, incognito: bool) {
    title().on = wanted(setting, incognito);
}

/// Whether to write a title: unless `terminalTitle` is `false`, and never in an incognito session,
/// whose name is the first line of a prompt and whose title could outlive it on the terminal.
fn wanted(setting: Option<bool>, incognito: bool) -> bool {
    setting != Some(false) && !incognito
}

/// [`Title::show`] for this run.
pub(crate) fn show<W: Write>(out: &mut W, name: &str) -> io::Result<()> {
    title().show(out, name)
}

/// [`Title::mark`] for this run.
pub(crate) fn mark<W: Write>(out: &mut W, state: State) -> io::Result<()> {
    title().mark(out, state)
}

/// [`Title::give_back`] for this run.
pub(crate) fn give_back<W: Write>(out: &mut W) -> io::Result<()> {
    title().give_back(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(title: &mut Title, name: &str) -> String {
        let mut out = Vec::new();
        title.show(&mut out, name).expect("show");
        String::from_utf8(out).expect("utf-8")
    }

    fn given_back(title: &mut Title) -> String {
        let mut out = Vec::new();
        title.give_back(&mut out).expect("give back");
        String::from_utf8(out).expect("utf-8")
    }

    /// A session name is text somebody typed or a record on disk held, and it is written inside an
    /// escape sequence. An escape, a bell or a C1 control left in it would end that sequence and
    /// hand the rest of the name to the terminal to act on.
    #[test]
    fn control_characters_in_a_name_cannot_reach_the_terminal() {
        let mut title = Title::new();
        let out = written(&mut title, "fix\x07\x1b]0;owned\x1b\\\u{9c}\x1b[2J\nnext");
        let inner = out
            .strip_prefix(PUSH)
            .and_then(|rest| rest.strip_prefix("\x1b]0;"))
            .and_then(|rest| rest.strip_suffix('\x07'))
            .expect("one title sequence after the push");
        assert!(
            !inner.chars().any(char::is_control),
            "a control character survived: {inner:?}"
        );
        assert!(inner.starts_with("bravebot · fix␇␛]0;owned"), "{inner:?}");
    }

    /// A tab bar shows a fraction of a long title, and the whole of one is in `/status`. The cut is
    /// marked so a title that was cut does not read as the whole name.
    #[test]
    fn a_long_name_is_cut_to_sixty_columns() {
        let long = "a".repeat(200);
        let cut = text(&long, State::Ready);
        assert_eq!(crate::wrap::display_width(&cut), MOST_COLUMNS);
        assert!(cut.starts_with("bravebot · aaa"));
        assert!(cut.ends_with('…'));

        let wide = "名".repeat(100);
        assert!(crate::wrap::display_width(&text(&wide, State::Ready)) <= MOST_COLUMNS);
        assert_eq!(text("short", State::Ready), "bravebot · short");
    }

    /// The push is what lets the shell have its own title back, so it has to come before anything
    /// overwrites that title, and only once: a second push would leave one pop short at the end.
    #[test]
    fn the_old_title_is_pushed_once_before_the_first_title() {
        let mut title = Title::new();
        assert_eq!(
            written(&mut title, "first"),
            format!("{PUSH}\x1b]0;bravebot · first\x07")
        );
        assert_eq!(
            written(&mut title, "second"),
            "\x1b]0;bravebot · second\x07"
        );
    }

    /// A rename is the change somebody makes to tell this tab from the others, so it has to reach
    /// the title. The same name again is not written, since the loop asks on every frame.
    #[test]
    fn a_rename_rewrites_the_title_and_an_unchanged_name_does_not() {
        let mut title = Title::new();
        written(&mut title, "first");
        assert_eq!(written(&mut title, "first"), "");
        assert_eq!(
            written(&mut title, "renamed"),
            "\x1b]0;bravebot · renamed\x07"
        );
    }

    /// Until a session has a name there is nothing to tell it apart by, and writing the bare prefix
    /// would replace the shell's title for nothing. Once one was shown, losing it, as `/clear` does,
    /// must not leave the old session's name on a new one.
    #[test]
    fn no_title_is_written_before_a_name_and_a_lost_name_leaves_the_prefix() {
        let mut title = Title::new();
        assert_eq!(written(&mut title, ""), "");
        assert_eq!(given_back(&mut title), "");
        written(&mut title, "named");
        assert_eq!(written(&mut title, "  "), "\x1b]0;bravebot\x07");
    }

    /// The switch exists for somebody whose terminal or multiplexer manages titles itself. Off means
    /// nothing at all is written, the push included, so their title is never touched.
    #[test]
    fn nothing_is_written_when_the_title_is_turned_off() {
        let mut title = Title::new();
        title.on = false;
        assert_eq!(written(&mut title, "named"), "");
        assert_eq!(given_back(&mut title), "");
    }

    /// Handing the terminal back pops the title, once. A pop without a push would take a title off
    /// the terminal's stack that some other program put there. The title is emptied first, for a
    /// terminal with no stack to pop. Taking the terminal over again, after an editor, pushes and
    /// writes the same title anew, since the pop put the shell's back.
    #[test]
    fn handing_back_pops_once_and_taking_over_writes_the_title_again() {
        let mut title = Title::new();
        written(&mut title, "named");
        assert_eq!(given_back(&mut title), format!("{EMPTY}{POP}"));
        assert_eq!(given_back(&mut title), "");
        assert_eq!(
            written(&mut title, "named"),
            format!("{PUSH}\x1b]0;bravebot · named\x07")
        );
    }

    fn marked_as(title: &mut Title, state: State) -> String {
        let mut out = Vec::new();
        title.mark(&mut out, state).expect("mark");
        String::from_utf8(out).expect("utf-8")
    }

    /// A person with many tabs open finds the one that stopped by its marker, so each change of
    /// state has to reach the title, and the same state again, which the loop reports on every
    /// frame, must not: a title rewritten while a turn runs makes some terminals tick.
    #[test]
    fn a_change_of_state_rewrites_the_title_and_a_repeated_state_does_not() {
        let mut title = Title::new();
        written(&mut title, "named");
        assert_eq!(
            marked_as(&mut title, State::Working),
            "\x1b]0;✦ bravebot · named\x07"
        );
        assert_eq!(marked_as(&mut title, State::Working), "");
        assert_eq!(
            marked_as(&mut title, State::Waiting),
            "\x1b]0;[!] bravebot · named\x07"
        );
        assert_eq!(marked_as(&mut title, State::Waiting), "");
        assert_eq!(
            marked_as(&mut title, State::Ready),
            "\x1b]0;bravebot · named\x07"
        );
    }

    /// The first turn of a session runs before it has a name, and that is when a tab most needs to
    /// say it is waiting. A state is written without one, and a ready session with no name still
    /// writes nothing, so the shell's title is not replaced for nothing.
    #[test]
    fn a_state_is_written_before_the_session_has_a_name() {
        let mut title = Title::new();
        assert_eq!(marked_as(&mut title, State::Ready), "");
        assert_eq!(
            marked_as(&mut title, State::Waiting),
            format!("{PUSH}\x1b]0;[!] bravebot\x07")
        );
        assert_eq!(
            written(&mut title, "named"),
            "\x1b]0;[!] bravebot · named\x07"
        );
    }

    /// The marker is the first thing in the title, so a tab bar that cuts it still shows it, and it
    /// counts toward the sixty columns rather than being added to them.
    #[test]
    fn the_marker_leads_the_title_and_counts_toward_sixty_columns() {
        let long = "a".repeat(200);
        for state in [State::Working, State::Waiting] {
            let marked = text(&long, state);
            assert!(marked.starts_with(state.marker().expect("a marker")));
            assert!(crate::wrap::display_width(&marked) <= MOST_COLUMNS);
            assert!(marked.ends_with('…'));
        }
    }

    /// Switched off means nothing is written for a state either.
    #[test]
    fn no_state_is_written_when_the_title_is_turned_off() {
        let mut title = Title::new();
        title.on = false;
        assert_eq!(marked_as(&mut title, State::Waiting), "");
    }

    /// The setting turns the title off with a real `false` and leaves it on otherwise, since the
    /// title is on unless somebody asked for it not to be.
    #[test]
    fn only_false_turns_the_run_title_off() {
        assert!(!wanted(Some(false), false));
        assert!(wanted(None, false));
        assert!(wanted(Some(true), false));
    }

    /// An incognito session leaves nothing behind, and its name is the first line of a prompt. A
    /// terminal with no title stack, or one that saves its windows between launches, would keep
    /// that line after the session ended, so no setting turns the title on there.
    #[test]
    fn an_incognito_session_writes_no_title() {
        assert!(!wanted(Some(true), true));
        assert!(!wanted(None, true));
        assert!(!wanted(Some(false), true));
    }
}
