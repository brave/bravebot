//! The shape of the mouse pointer over the border between the transcript and the info panel, so a
//! person can see the border can be dragged ([PANEL-6](../../../docs/specs/info-panel.md#PANEL-6)).
//!
//! A program asks for a pointer shape with OSC 22 and the terminal draws it over its window. A
//! terminal that does not read the sequence ignores it, and terminals that do read it disagree about
//! the names. So only terminals documented to take a name this module sends are asked, and anywhere
//! else nothing is written and the pointer stays the terminal's own.

use std::io::{self, Write};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

/// OSC 22 with no name, which gives the pointer back to the terminal.
const RESET: &str = "\x1b]22;\x1b\\";

/// What the pointer is asked to look like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// The terminal's own pointer.
    Default,
    /// The one that says a border can be dragged sideways.
    ResizeColumns,
}

/// The name for the pointer that resizes columns in the terminal `term_program` and `term` say this
/// is, where that terminal is documented to take it.
///
/// Kitty, Ghostty and foot take the names CSS gives to cursors. xterm takes X11 names, iTerm2 takes
/// a subset of its own that it does not list, and VTE and Alacritty do not read the sequence, so
/// none of those is asked. Inside tmux `TERM_PROGRAM` names tmux and `TERM` is tmux's, so a session
/// there is not recognised.
fn resize_name(term_program: Option<&str>, term: Option<&str>) -> Option<&'static str> {
    let css = term_program == Some("ghostty")
        || term.is_some_and(|term| {
            matches!(term, "xterm-kitty" | "xterm-ghostty") || term.starts_with("foot")
        });
    css.then_some("ew-resize")
}

/// What the terminal was last asked to draw as the pointer, kept between passes so that an
/// unchanged shape is not written again.
#[derive(Debug)]
pub(crate) struct Pointer {
    /// The name this terminal gives the resize shape, or none where the terminal is not asked.
    name: Option<&'static str>,
    shown: Shape,
}

impl Pointer {
    const fn new(name: Option<&'static str>) -> Self {
        Self {
            name,
            shown: Shape::Default,
        }
    }

    /// Ask for `wanted` where it differs from what the terminal was last asked for.
    pub(crate) fn show<W: Write>(&mut self, out: &mut W, wanted: Shape) -> io::Result<()> {
        let Some(name) = self.name else {
            return Ok(());
        };
        if wanted == self.shown {
            return Ok(());
        }
        match wanted {
            Shape::ResizeColumns => write!(out, "\x1b]22;{name}\x1b\\")?,
            Shape::Default => out.write_all(RESET.as_bytes())?,
        }
        out.flush()?;
        self.shown = wanted;
        Ok(())
    }
}

/// The run's pointer. One per process, because one process owns the window it is the pointer over.
fn pointer() -> MutexGuard<'static, Pointer> {
    static POINTER: OnceLock<Mutex<Pointer>> = OnceLock::new();
    POINTER
        .get_or_init(|| {
            let term_program = std::env::var("TERM_PROGRAM").ok();
            let term = std::env::var("TERM").ok();
            Mutex::new(Pointer::new(resize_name(
                term_program.as_deref(),
                term.as_deref(),
            )))
        })
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// [`Pointer::show`] for this run.
pub(crate) fn show<W: Write>(out: &mut W, wanted: Shape) -> io::Result<()> {
    pointer().show(out, wanted)
}

/// Give the pointer back to the terminal, where this run changed it.
pub(crate) fn give_back<W: Write>(out: &mut W) -> io::Result<()> {
    pointer().show(out, Shape::Default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(pointer: &mut Pointer, wanted: Shape) -> String {
        let mut out = Vec::new();
        pointer.show(&mut out, wanted).expect("show");
        String::from_utf8(out).expect("utf-8")
    }

    /// The terminal is told when the pointer moves onto the border and when it moves off, and not
    /// on every report in between: a pointer crossing the window reports a position for each cell.
    #[test]
    fn a_shape_is_written_when_it_changes_and_not_while_it_stays() {
        let mut pointer = Pointer::new(Some("ew-resize"));
        assert_eq!(written(&mut pointer, Shape::Default), "", "nothing changed");
        assert_eq!(
            written(&mut pointer, Shape::ResizeColumns),
            "\x1b]22;ew-resize\x1b\\"
        );
        assert_eq!(written(&mut pointer, Shape::ResizeColumns), "");
        assert_eq!(written(&mut pointer, Shape::Default), RESET);
        assert_eq!(written(&mut pointer, Shape::Default), "");
    }

    /// A terminal with no known name is never written to, whatever shape is wanted, so one that
    /// would print the sequence or react to it badly sees none.
    #[test]
    fn a_terminal_with_no_known_name_is_written_nothing() {
        let mut pointer = Pointer::new(None);
        assert_eq!(written(&mut pointer, Shape::ResizeColumns), "");
        assert_eq!(written(&mut pointer, Shape::Default), "");
    }

    /// Only the terminals documented to take CSS names are asked. `xterm-256color` is what iTerm2,
    /// Terminal.app and many others set, so it names none of them.
    #[test]
    fn only_terminals_documented_to_take_css_names_are_asked() {
        for (term_program, term) in [
            (None, Some("xterm-kitty")),
            (Some("ghostty"), Some("xterm-256color")),
            (None, Some("xterm-ghostty")),
            (None, Some("foot")),
            (None, Some("foot-extra")),
        ] {
            assert_eq!(
                resize_name(term_program, term),
                Some("ew-resize"),
                "{term_program:?} {term:?} is not asked"
            );
        }
        for (term_program, term) in [
            (Some("iTerm.app"), Some("xterm-256color")),
            (Some("Apple_Terminal"), Some("xterm-256color")),
            (Some("vscode"), Some("xterm-256color")),
            (Some("WezTerm"), Some("xterm-256color")),
            (Some("tmux"), Some("tmux-256color")),
            (None, Some("alacritty")),
            (None, Some("xterm-256color")),
            (None, Some("screen")),
            (None, None),
        ] {
            assert_eq!(
                resize_name(term_program, term),
                None,
                "{term_program:?} {term:?} is asked"
            );
        }
    }
}
