//! The target the pointer is over (MOUSE-51).
//!
//! Where the terminal reports the pointer with no button held, the session holds the target under it
//! that says what a click there does, so the frame can mark it and say what a click on it does. It is
//! read from the same record a click is ([`Session::target_at`]), and it changes nothing but what is
//! marked and said.

use super::{Hit, Laid, Session, Space, Target};
use ratatui::layout::Rect;

/// A target the pointer is over, and the cells of the screen it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hovered {
    pub hit: Hit,
    /// The cells it covers on the screen, which for a target in the transcript are those of the
    /// rows in view.
    pub area: Rect,
}

impl Hit {
    /// Whether the pointer over this kind of target says what a click on it does. The text of the box
    /// is the place the caret goes wherever the pointer is over it, and a panel that holds the input
    /// answers the pointer by itself.
    pub fn is_hoverable(self) -> bool {
        !matches!(
            self,
            Hit::Text { .. } | Hit::Words(_) | Hit::Popup(_) | Hit::PopupRows(_) | Hit::Match(_)
        )
    }
}

impl Laid {
    /// The cells of the screen a target covers, which for a target in the transcript are those of its
    /// rows that are in view, where `top` is the row of the transcript at the top of the area it was
    /// drawn in.
    pub fn on_the_screen(&self, target: &Target, top: u16) -> Rect {
        match (target.space, self.window) {
            (Space::Screen, _) | (Space::Transcript, None) => target.area,
            (Space::Transcript, Some(window)) => {
                let first = i32::from(target.area.y) - i32::from(top) + i32::from(window.area.y);
                let (y, height) = if first < i32::from(window.area.y) {
                    let cut = u16::try_from(i32::from(window.area.y) - first).unwrap_or(u16::MAX);
                    (window.area.y, target.area.height.saturating_sub(cut))
                } else {
                    (u16::try_from(first).unwrap_or(u16::MAX), target.area.height)
                };
                Rect::new(target.area.x, y, target.area.width, height).intersection(window.area)
            }
        }
    }

    /// The target under a cell, among those this record holds, that says what a click on it does.
    pub fn hovered(&self, session: &Session, top: u16, row: u16, column: u16) -> Option<Hovered> {
        let target = self
            .find(session, top, row, column)
            .filter(|target| target.hit.is_hoverable())?;
        Some(Hovered {
            hit: target.hit,
            area: self.on_the_screen(target, top),
        })
    }
}

impl Session {
    /// The target the pointer is over, where it was last moved with no button held and nothing since
    /// has ended that.
    pub fn hovered(&self) -> Option<Hovered> {
        self.hover
    }

    /// Whether the pointer is hovering, which the frame reads the target under it for.
    pub fn is_hovering(&self) -> bool {
        self.hovering
    }

    /// Where the pointer is, as a row and a column, where a report has said.
    pub fn pointer_at(&self) -> Option<(u16, u16)> {
        self.pointer
    }

    /// Take the pointer's position with no button held, and say whether the target under it is not
    /// the one that was, which is whether a frame is wanted (MOUSE-51).
    pub(crate) fn hover_at(&mut self, row: u16, column: u16) -> bool {
        self.hovering = true;
        let now = self.laid.hovered(self, self.top_row(), row, column);
        let changed = now != self.hover;
        self.hover = now;
        changed
    }

    /// End hovering: a key was pressed, the terminal was left or resized, or a button went down. Says
    /// whether a target was marked, so that a frame is drawn without it.
    pub(crate) fn end_hover(&mut self) -> bool {
        self.hovering = false;
        let was_marked = self.hover.take().is_some();
        self.hover_ended |= was_marked;
        was_marked
    }

    /// Whether a target that was marked stopped being since this was last asked, which a loop that
    /// draws only what has changed is asked for a frame by.
    pub(crate) fn take_the_hover_ended(&mut self) -> bool {
        std::mem::take(&mut self.hover_ended)
    }

    /// Read the target under the pointer again, in the layout a frame has just drawn, where the pointer
    /// is hovering. The frame marked the same target, so no frame is asked for.
    pub(super) fn follow_the_hover(&mut self) {
        self.hover = match (self.hovering, self.pointer) {
            (true, Some((row, column))) => self.laid.hovered(self, self.top_row(), row, column),
            _ => None,
        };
    }
}
