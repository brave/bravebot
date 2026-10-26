//! The small menu a right click opens over a reply, a delegate's block or a held selection (MOUSE-52).
//!
//! The menu is a list of what the person could do with the keyboard, each item doing what its
//! command or its click does and nothing else. It holds the input while it is open, so the keys that
//! move its cursor, take an item and close it are the menu's, and a press outside it only closes it.

use super::{Hit, Panel, Session, Status};

/// What an item of the menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Copy the reply of the transcript entry at this index, as `/copy` does.
    Copy(usize),
    /// Copy the same reply as the screen shows it, without the marks of the markdown it was written
    /// in.
    CopyAsText(usize),
    /// Branch the session, as `/branch` does.
    Branch,
    /// Open the view of the delegate whose block is the transcript entry at this index, as a click
    /// on the block does.
    OpenDelegate(usize),
    /// Copy the selection held, as letting go of the button after the sweep did.
    CopySelection,
}

/// The menu that is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    choices: Vec<Choice>,
    cursor: usize,
    at: (u16, u16),
}

impl Menu {
    /// The items, top to bottom.
    pub fn choices(&self) -> &[Choice] {
        &self.choices
    }

    /// Which item the cursor is on, counted from the top.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The row and the column of the pointer when the menu was opened, which is where it is drawn
    /// from where there is room.
    pub fn at(&self) -> (u16, u16) {
        self.at
    }
}

impl Session {
    /// The menu, while one is open.
    pub fn menu(&self) -> Option<&Menu> {
        self.menu.as_ref()
    }

    /// Whether a menu is open, which holds the input while it is.
    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// What a right click at a cell offers, from what is under it: a held selection the pointer is
    /// inside offers to copy it, a reply offers to copy it as written and as shown and to branch the
    /// session, and a delegate's block offers to open the delegate's view. Nothing else offers
    /// anything.
    ///
    /// What is offered depends on which kind of thing is there and never on the words it holds.
    fn offered_at(&self, row: u16, column: u16) -> Vec<Choice> {
        if self
            .selection
            .is_some_and(|selection| !selection.is_empty() && selection.covers(row, column))
        {
            return vec![Choice::CopySelection];
        }
        match self.target_at(row, column).map(|target| target.hit) {
            Some(Hit::Words(index) | Hit::Reply(index) | Hit::Block(index, _)) => {
                vec![
                    Choice::Copy(index),
                    Choice::CopyAsText(index),
                    Choice::Branch,
                ]
            }
            Some(Hit::Delegate(index)) => vec![Choice::OpenDelegate(index)],
            _ => Vec::new(),
        }
    }

    /// Open the menu a right click at a cell asks for, and say whether one opened.
    ///
    /// None where the setting turned it off, where a panel that holds the input is up, while a
    /// command the person typed is running, which reads no key but the one that stops it and so has
    /// nothing to hand the menu's keys to, and in a delegate's view and the files state, whose rows
    /// are not the session's replies, a held selection of them included. A menu is never a
    /// question: nothing in it answers an approval, a trust or a credential prompt, which read no
    /// pointer event at all (MOUSE-44).
    pub(crate) fn open_the_menu(&mut self, row: u16, column: u16) -> bool {
        if !self.context_menu
            || self.modal()
            || self.status == Status::Running
            || self.watching().is_some()
            || self.files_open()
        {
            return false;
        }
        let choices = self.offered_at(row, column);
        if choices.is_empty() {
            return false;
        }
        self.menu = Some(Menu {
            choices,
            cursor: 0,
            at: (row, column),
        });
        true
    }

    /// Close the menu with no effect.
    pub(crate) fn close_the_menu(&mut self) {
        self.menu = None;
        self.menu_taken = None;
    }

    /// Move the cursor of the menu `rows` items, up or down, stopping at the first and the last.
    pub(crate) fn walk_the_menu(&mut self, rows: u16, down: bool) {
        let Some(menu) = &mut self.menu else {
            return;
        };
        let last = menu.choices.len().saturating_sub(1);
        let rows = usize::from(rows);
        menu.cursor = match down {
            true => menu.cursor.saturating_add(rows).min(last),
            false => menu.cursor.saturating_sub(rows),
        };
    }

    /// Take the item the cursor is on, closing the menu.
    pub(crate) fn take_from_the_menu(&mut self) -> Option<Choice> {
        let menu = self.menu.take()?;
        menu.choices.get(menu.cursor).copied()
    }

    /// What an item a press took asks for, taken so it is carried out once.
    pub(crate) fn take_the_menu_choice(&mut self) -> Option<Choice> {
        self.menu_taken.take()
    }

    /// Give a mouse event to the open menu, as it was drawn.
    ///
    /// A left press on an item takes it. A left press anywhere else outside the menu closes it with
    /// no effect, as Escape does (MOUSE-47), and one on its border or between its rows does nothing.
    /// A turn of the wheel moves the cursor by `tui.wheelRows`. A press of another button, a drag and
    /// a release do nothing, so the release of the press that opened the menu leaves it open.
    pub(super) fn mouse_in_the_menu(&mut self, mouse: ratatui::crossterm::event::MouseEvent) {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        let rows = self.wheel_rows();
        match mouse.kind {
            MouseEventKind::ScrollUp => self.walk_the_menu(rows, false),
            MouseEventKind::ScrollDown => self.walk_the_menu(rows, true),
            MouseEventKind::Down(MouseButton::Left) => {
                let cell = ratatui::layout::Position::new(mouse.column, mouse.row);
                let Some(popup) = self.laid.popup(Panel::Menu) else {
                    return;
                };
                if !popup.area.contains(cell) {
                    self.close_the_menu();
                    return;
                }
                let Some(list) = popup.list.filter(|list| list.contains(cell)) else {
                    return;
                };
                let item = usize::from(mouse.row - list.y);
                let Some(menu) = &mut self.menu else {
                    return;
                };
                if item < menu.choices.len() {
                    menu.cursor = item;
                    self.menu_taken = self.take_from_the_menu();
                }
            }
            _ => {}
        }
    }
}
