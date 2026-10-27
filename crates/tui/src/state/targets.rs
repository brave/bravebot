//! What a click on a cell of the last frame does.
//!
//! Every draw writes the targets it puts on the screen into [`Laid::targets`], each as the cells it
//! covers and what a click on them is, and the mouse handler only looks them up
//! ([`Session::target_at`]). Nothing in the handler works out where a widget is, and no target is
//! recorded anywhere else, so what the screen shows and what a click finds are the same record.

use super::{Footer, Laid, Panel, Popup, Session, Status};
use ratatui::layout::{Position, Rect};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Which coordinates the area of a [`Target`] is counted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    /// Cells of the screen.
    Screen,
    /// Columns of the screen and rows of the transcript counted from its first row, so that the
    /// target follows the view as it scrolls. Recorded for everything the transcript draws, which
    /// includes rows that are not on the screen.
    Transcript,
}

/// What a click on a cell the frame drew for the purpose does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Out of a delegate's view, to what it was opened over (MOUSE-24).
    Back,
    /// Copy the reply held by the transcript entry at this index (MOUSE-26).
    Reply(usize),
    /// Copy one fenced block of the reply held by the transcript entry at this index, the one that
    /// is the `n`th of its blocks (MOUSE-27).
    Block(usize, usize),
    /// Take the waiting prompt at this place in the queue back into the box (MOUSE-28).
    Queued(usize),
    /// The cells a panel that holds the input covers: a picker opened while a turn runs, the
    /// scroller's list of keys, or the search over the prompts already sent. A press on them is the
    /// panel's, and reaches nothing drawn beneath (MOUSE-45).
    Popup(Panel),
    /// The cells of the rows of a panel's list, drawn over the panel's, for a panel whose rows a
    /// press can take (MOUSE-15).
    PopupRows(Panel),
    /// The row of the search over the prompts already sent that holds this match, counted from the
    /// oldest: a press puts the cursor on it, and a second takes it into the box (MOUSE-15).
    Match(usize),
    /// The rows of the words of the reply of the transcript entry at this index, across the width
    /// of the transcript: nothing a left click does, and where a right click finds the reply
    /// (MOUSE-52). The marker, the word that copies a block and a link are targets over it.
    Words(usize),
    /// The track of the scrollbar beside the transcript: a press on it puts the view there or takes
    /// hold of the thumb, which [`Laid::bar`] says where it is (MOUSE-49).
    Scrollbar,
    /// The row of the list of what could be typed under the box that holds this item of what is
    /// offered: a click puts the cursor on it, and a second takes it as Tab does (MOUSE-15).
    Item(usize),
    /// A row of the prompt box's text: where the text of the row begins on the screen and which
    /// bytes of the line it holds. A click puts the caret before the character under it (MOUSE-16).
    Text {
        text_at: u16,
        start: usize,
        end: usize,
    },
    /// A row of the info panel or a control of its files state, and what a click on it is
    /// (MOUSE-11 to MOUSE-13, MOUSE-18 to MOUSE-21, MOUSE-30 to MOUSE-32, MOUSE-41).
    Panel(crate::panel::Target),
    /// The link in the assistant's words that [`Laid::links`] holds at this index, drawn in the cells
    /// the target covers: with ctrl held a click opens an address in the browser and a path in the
    /// editor, and without it a click on a path shows the file (MOUSE-40, MOUSE-42, MOUSE-43).
    Link(usize),
    /// The call of the transcript entry at this index, which has more to show than it draws: a click
    /// expands it in the scroller and opens the scroller on it at the plain prompt (MOUSE-9,
    /// MOUSE-25).
    Call(usize),
    /// The block of the delegate of the transcript entry at this index: a click opens that
    /// delegate's view (MOUSE-10, MOUSE-23).
    Delegate(usize),
    /// Do what the key a hint on the row under the box names does, or what the permission mode
    /// there does (MOUSE-14, MOUSE-48).
    Footer(Footer),
    /// A pane of the files state that a turn of the wheel scrolls, whatever is drawn over it. A
    /// click on it does nothing that a click on the text under it would not (MOUSE-19).
    Wheel(Scrolled),
}

/// A pane of the files state of the info panel that the wheel scrolls (MOUSE-19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scrolled {
    /// The heading, the files and the rule under them: the wheel moves the first row of the list.
    List,
    /// The file's name, the bar, the edits and the row of keys: the wheel scrolls the edits.
    Edits,
}

impl Hit {
    /// Whether this is a panel that holds the input or a row of its list, which a press belongs to
    /// whatever else is drawn under it.
    pub fn is_a_panel(self) -> bool {
        matches!(self, Hit::Popup(_) | Hit::PopupRows(_) | Hit::Match(_))
    }

    /// Whether the cells of this target are drawn for the click and are no part of the text around
    /// them, which a copy, a highlight and what tells a row of the text from another leave out.
    pub fn is_chrome(self) -> bool {
        matches!(self, Hit::Block(..))
    }

    /// The columns of a line that only the interface put there, as the first and how many, for a
    /// target of this kind whose cells begin at `column` and are `width` wide with `padding` blanks
    /// drawn before them to put them there. `None` where the cells are text (see [`Hit::is_chrome`]).
    pub fn chrome_columns(self, column: u16, width: u16, padding: u16) -> Option<(u16, u16)> {
        self.is_chrome()
            .then(|| (column.saturating_sub(padding), padding + width))
    }

    /// Whether what a frame drew, recorded in `laid`, still stands in the state the session is in now.
    ///
    /// The one place that says in which states each kind of target works. A frame is drawn once and
    /// the state moves on before the next, as when a key opens the scroller or a turn ends, so a
    /// target found in the record is only one a click can act on if this agrees.
    pub fn stands(self, session: &Session, laid: &Laid) -> bool {
        // While a panel that holds the input is open nothing of the frame beneath it stands
        // (MOUSE-45), which every kind below the panels' own is asked first.
        let beneath = !session.modal();
        match self {
            // The panels that hold the input stand for as long as they are drawn over the frame, and
            // only while the panel is open: one a key closed before the frame that draws it gone is
            // not there to take a press, which then reaches what is beneath.
            Hit::Popup(panel) | Hit::PopupRows(panel) => session.has_open(panel),
            Hit::Match(_) => session.history_search.is_some(),
            Hit::Back => beneath && session.watching_a_delegate() && !session.files_open(),
            // The entries a view of a delegate draws are the delegate's own, so an index is a place
            // in them and not in the session's transcript.
            Hit::Reply(_) | Hit::Block(..) | Hit::Words(_) => {
                beneath && session.watching().is_none() && !session.files_open()
            }
            Hit::Queued(_) => beneath && !session.files_open() && session.in_the_plain_frame(),
            // A view of a delegate, of what a command printed or of an aside is drawn from other
            // entries than the session's, and the list of them has no call or block of the
            // session's to click.
            Hit::Call(_) | Hit::Delegate(_) => {
                beneath && session.watching().is_none() && !session.files_open()
            }
            // The files state draws a line of its own in place of the hints.
            Hit::Footer(footer) => {
                beneath
                    && session.in_the_plain_frame()
                    && !session.files_open()
                    && session.footer_stands(footer)
            }
            // The panel is drawn beside the transcript at the plain prompt and while work runs,
            // and by no view that replaces the frame. The files state draws other rows in place of
            // the ordinary ones, so each kind stands in the state that draws it.
            Hit::Panel(target) => {
                beneath
                    && session.in_the_plain_frame()
                    && session.files_open() == target.is_in_the_files_state()
            }
            Hit::Wheel(_) => beneath && session.in_the_plain_frame() && session.files_open(),
            Hit::Scrollbar => beneath && session.in_the_plain_frame(),
            Hit::Item(_) => beneath && session.in_the_plain_frame() && session.is_completing(),
            // The vi style has modes a click would have to know the meaning of, and the files state
            // owns every key, so neither has a caret to put (MOUSE-16).
            Hit::Text { .. } => {
                beneath
                    && session.in_the_plain_frame()
                    && !session.files_open()
                    && session.editing() == crate::vim::Editing::Ordinary
            }
            // An address opens wherever it is drawn, since opening it changes nothing the work holds.
            // A path is shown at the end of the transcript and opened in an editor that takes the
            // terminal, so neither while a turn or a command is drawing the screen, and not in a
            // delegate's view, where a name may be another file than it is here (CHECKOUT-7).
            Hit::Link(index) => {
                beneath
                    && match laid.links.get(index) {
                        Some(crate::text_links::Target::Address(_)) => true,
                        Some(crate::text_links::Target::Path(_)) => session.shows_a_file_here(),
                        None => false,
                    }
            }
        }
    }
}

/// The cells of one thing the frame drew that a click acts on, and what the click is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// The cells it covers, in the coordinates `space` names. In the transcript's the `x` is a
    /// column of the screen and the `y` a row of the transcript.
    pub area: Rect,
    pub space: Space,
    pub hit: Hit,
    /// The blanks drawn before the cells only to put them where they are, which are chrome with
    /// them.
    pub padding: u16,
}

impl Target {
    /// A target on cells of the screen.
    pub fn on_screen(area: Rect, hit: Hit) -> Self {
        Self {
            area,
            space: Space::Screen,
            hit,
            padding: 0,
        }
    }

    /// A target on rows of the transcript.
    pub fn in_transcript(area: Rect, hit: Hit, padding: u16) -> Self {
        Self {
            area,
            space: Space::Transcript,
            hit,
            padding,
        }
    }

    /// The columns of the line this is on that only the interface put there, as the first and how
    /// many, where it is chrome (see [`Hit::is_chrome`]).
    pub fn drawn_for_the_click(&self) -> Option<(u16, u16)> {
        self.hit
            .chrome_columns(self.area.x, self.area.width, self.padding)
    }

    /// Whether `cell` is one of the cells, given the row of the transcript that is under it where
    /// the cell is in the transcript.
    fn is_under(&self, cell: Position, transcript_row: Option<u16>) -> bool {
        match self.space {
            Space::Screen => self.area.contains(cell),
            Space::Transcript => {
                transcript_row.is_some_and(|row| self.area.contains(Position::new(cell.x, row)))
            }
        }
    }
}

/// The targets of a [`Laid`] by the row each covers, so that a cell is asked of the few targets on its
/// row and not of every target the frame recorded, which for a long transcript is tens of thousands
/// and is asked at every report of the pointer.
///
/// Made the first time a cell is asked about, since a frame nobody points at never needs it, and
/// kept for the targets recorded when it was made. It describes what [`Laid::targets`] holds and is
/// no part of what a frame says, so two frames are equal whatever either has been asked.
#[derive(Debug, Default, Clone)]
pub struct ByRow(OnceLock<Rows>);

impl PartialEq for ByRow {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for ByRow {}

#[derive(Debug, Default, Clone)]
struct Rows {
    /// How many targets were recorded when this was made.
    covered: usize,
    /// For each row of the screen and each row of the transcript, the places in
    /// [`Laid::targets`] of the targets that cover it, in the order they were recorded.
    on_screen: HashMap<u16, Vec<usize>>,
    in_transcript: HashMap<u16, Vec<usize>>,
}

impl Rows {
    fn of(targets: &[Target]) -> Self {
        let mut rows = Rows {
            covered: targets.len(),
            ..Rows::default()
        };
        for (place, target) in targets.iter().enumerate() {
            let covered = match target.space {
                Space::Screen => &mut rows.on_screen,
                Space::Transcript => &mut rows.in_transcript,
            };
            for row in target.area.y..target.area.bottom() {
                covered.entry(row).or_default().push(place);
            }
        }
        rows
    }
}

// How many targets `Laid::find` asked whether they stand under a cell, on this thread, for the test
// that holds the work of a lookup to the targets on one row.
#[cfg(test)]
thread_local! {
    static ASKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Laid {
    /// The target under a cell of the screen, among those this record holds, that is drawn over the
    /// others and stands in the state `session` is in.
    ///
    /// What is drawn last is drawn over what is drawn first, so the targets are read from the last
    /// recorded to the first, and a panel recorded after the frame it covers is found before what is
    /// under it. `top` is the row of the transcript at the top of the area it was drawn in. Only the
    /// targets that cover the row of the cell are read, which are found from [`ByRow`].
    pub fn find(&self, session: &Session, top: u16, row: u16, column: u16) -> Option<&Target> {
        let cell = Position::new(column, row);
        let transcript_row = self.window.and_then(|window| {
            window
                .area
                .contains(cell)
                .then(|| top.saturating_add(row - window.area.y))
        });
        let under = |target: &Target| {
            #[cfg(test)]
            ASKED.with(|asked| asked.set(asked.get() + 1));
            target.is_under(cell, transcript_row) && target.hit.stands(session, self)
        };
        let by_row = self.by_row.0.get_or_init(|| Rows::of(&self.targets));
        // Targets recorded since the rows were made, which are the last drawn, and every target where
        // some were taken away.
        let Some(recorded_since) = self.targets.get(by_row.covered..) else {
            return self.targets.iter().rev().find(|target| under(target));
        };
        if let Some(found) = recorded_since.iter().rev().find(|target| under(target)) {
            return Some(found);
        }
        let mut on_screen = by_row.on_screen.get(&row).map_or(&[][..], Vec::as_slice);
        let mut in_transcript = transcript_row
            .and_then(|at| by_row.in_transcript.get(&at))
            .map_or(&[][..], Vec::as_slice);
        // From the last recorded of the two lists to the first.
        loop {
            let place = match (on_screen.last(), in_transcript.last()) {
                (Some(&a), Some(&b)) if a > b => {
                    on_screen = &on_screen[..on_screen.len() - 1];
                    a
                }
                (_, Some(&b)) => {
                    in_transcript = &in_transcript[..in_transcript.len() - 1];
                    b
                }
                (Some(&a), None) => {
                    on_screen = &on_screen[..on_screen.len() - 1];
                    a
                }
                (None, None) => return None,
            };
            let target = self.targets.get(place)?;
            if under(target) {
                return Some(target);
            }
        }
    }

    /// The target of the screen under a cell, among those that stand, whose kind `wanted` names,
    /// whatever is drawn over it: where the wheel is over a pane, and not over the row or the
    /// control that is drawn on it.
    pub fn find_of_kind(
        &self,
        session: &Session,
        row: u16,
        column: u16,
        wanted: impl Fn(Hit) -> bool,
    ) -> Option<&Target> {
        let cell = Position::new(column, row);
        let under = |target: &Target| {
            wanted(target.hit) && target.area.contains(cell) && target.hit.stands(session, self)
        };
        let by_row = self.by_row.0.get_or_init(|| Rows::of(&self.targets));
        let Some(recorded_since) = self.targets.get(by_row.covered..) else {
            return self.targets.iter().rev().find(|target| under(target));
        };
        if let Some(found) = recorded_since.iter().rev().find(|target| under(target)) {
            return Some(found);
        }
        by_row
            .on_screen
            .get(&row)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .rev()
            .filter_map(|&place| self.targets.get(place))
            .find(|target| under(target))
    }

    /// The panel of this kind the frame drew over the others, with the cells it covers and those of
    /// its rows, where it drew one.
    pub fn popup(&self, panel: Panel) -> Option<Popup> {
        let cells = |hit| {
            self.targets
                .iter()
                .rev()
                .find(|target| target.hit == hit)
                .map(|target| target.area)
        };
        Some(Popup {
            panel,
            area: cells(Hit::Popup(panel))?,
            list: cells(Hit::PopupRows(panel)),
        })
    }

    /// The first row, the last row and the transcript index of each call that has more to show than
    /// it draws, top to bottom.
    pub fn calls(&self) -> impl Iterator<Item = (u16, u16, usize)> + '_ {
        self.rows_of(|hit| match hit {
            Hit::Call(index) => Some(index),
            _ => None,
        })
    }

    /// The first row, the last row and the transcript index of each delegate's block, top to bottom,
    /// for the test that reads the rows a block was recorded over.
    #[cfg(test)]
    pub fn delegate_blocks(&self) -> impl Iterator<Item = (u16, u16, usize)> + '_ {
        self.rows_of(|hit| match hit {
            Hit::Delegate(index) => Some(index),
            _ => None,
        })
    }

    fn rows_of(
        &self,
        index_of: impl Fn(Hit) -> Option<usize> + 'static,
    ) -> impl Iterator<Item = (u16, u16, usize)> + '_ {
        self.targets
            .iter()
            .filter(|target| target.space == Space::Transcript)
            .filter_map(move |target| {
                let index = index_of(target.hit)?;
                let last = target
                    .area
                    .y
                    .saturating_add(target.area.height.saturating_sub(1));
                Some((target.area.y, last, index))
            })
    }
}

/// What a press stood on, so that its release finds the same thing under it and not what a frame
/// drawn between the two put there: rows added above move the cells of a row, and a report replaces
/// the plan a press on one of its tasks was made against.
#[derive(Debug, Clone, PartialEq)]
pub struct Pressed {
    hit: Hit,
    /// What the link a press stood on opens, since the number of a link names another one when links
    /// are added above it.
    link: Option<crate::text_links::Target>,
    /// The plan the press was made against, for the rows of it.
    plan: u64,
}

impl Session {
    /// Remember what the press of the left button at a cell stands on (MOUSE-50).
    pub(crate) fn note_the_press(&mut self, row: u16, column: u16) {
        self.pressed_on = self.target_at(row, column).map(|target| Pressed {
            hit: target.hit,
            link: self.link_of(target.hit),
            plan: self.plan_generation,
        });
    }

    /// Whether the release found under it what the press stood on: the same kind of target, the same
    /// place of it, the same link where it is one and the same plan where it is a row of the plan.
    pub(crate) fn press_stood_on(&self, hit: Hit) -> bool {
        self.pressed_on.as_ref().is_some_and(|pressed| {
            pressed.hit == hit
                && pressed.link == self.link_of(hit)
                && (!matches!(hit, Hit::Panel(_)) || pressed.plan == self.plan_generation)
        })
    }

    /// What the link a target is opens, for the targets that are one.
    fn link_of(&self, hit: Hit) -> Option<crate::text_links::Target> {
        match hit {
            Hit::Link(index) => self.laid.links.get(index).cloned(),
            _ => None,
        }
    }

    /// Whether the frame on the screen is the plain one, with the transcript, the box, the row of
    /// hints and the info panel, and not one of the views that take the screen in place of it: the
    /// scroller and the views of a delegate, of what a command printed and of an aside.
    pub(crate) fn in_the_plain_frame(&self) -> bool {
        !self.scrolling() && self.watching().is_none()
    }

    /// Whether the panel of this kind is open, whatever frame is on the screen.
    pub(crate) fn has_open(&self, panel: Panel) -> bool {
        match panel {
            Panel::Picker => self.picking.is_some(),
            Panel::KeyList => self.scroller.as_ref().is_some_and(|scroller| scroller.help),
            Panel::HistorySearch => self.history_search.is_some(),
            Panel::Menu => self.menu.is_some(),
        }
    }

    /// Whether a panel that holds the input is up: a picker, the scroller's list of keys, the
    /// search over the prompts already sent or the menu a right click opened. Every press, drag, release and turn of the wheel is the
    /// panel's then, and nothing of the frame beneath it is a target (MOUSE-45).
    pub(crate) fn modal(&self) -> bool {
        self.picking.is_some()
            || self.scroller.as_ref().is_some_and(|scroller| scroller.help)
            || self.history_search.is_some()
            || self.menu.is_some()
    }

    /// Whether the key a hint names does anything in the state the session is in. A hint is drawn
    /// while the key it names is refused in some states, so a click is answered against the state
    /// as well as the cells.
    fn footer_stands(&self, footer: Footer) -> bool {
        // A command the person typed is running, and the keys of the row are the turn's: it reads
        // none of them, the one that cycles the permission mode included, so a click is not given
        // what a key is refused (MOUSE-14, MOUSE-48).
        if self.status == Status::Running {
            return false;
        }
        match footer {
            // Keys that act whatever the box holds. The three that open a view and move a command
            // to the background do nothing where there is nothing to open or move, which is also
            // what a click on a hint left over from an earlier frame does.
            Footer::Mode
            | Footer::Trail
            | Footer::Info
            | Footer::Held
            | Footer::Watch
            | Footer::Outputs
            | Footer::Background
            | Footer::Paste => true,
            Footer::Shortcuts => self.can_ask_for_shortcuts(),
            Footer::CancelShell => {
                self.shell
                    && self.status == Status::Idle
                    && self.editing() == crate::vim::Editing::Ordinary
                    && !self.enhanced_pending()
            }
        }
    }

    /// The target a click on a cell of the screen acts on: the one drawn over the others that is
    /// still standing, among the targets the last frame recorded ([`Laid::find`]).
    pub fn target_at(&self, row: u16, column: u16) -> Option<&Target> {
        self.laid.find(self, self.top_row(), row, column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select::{View, Window};

    /// A session whose last frame drew an 80 by 10 transcript of 100 rows, with the view `scroll`
    /// rows above the tail.
    fn with_a_transcript(scroll: u16) -> Session {
        let mut session = Session::new("none");
        session.laid.rows = 100;
        session.laid.height = 10;
        session.scroll = scroll;
        session.laid.window = Some(Window {
            area: Rect::new(0, 0, 80, 10),
            top: session.top_row(),
            view: View::of(&session),
        });
        session
    }

    /// Rejects: a lookup that returns the first target recorded where two cover a cell. A panel is
    /// recorded after the frame it covers, and a press on it is the panel's.
    #[test]
    fn the_target_recorded_last_is_the_one_drawn_on_top() {
        let mut session = Session::new("none");
        let under = Rect::new(0, 20, 40, 1);
        session
            .laid
            .targets
            .push(Target::on_screen(under, Hit::Queued(0)));
        session
            .laid
            .targets
            .push(Target::on_screen(Rect::new(10, 20, 5, 1), Hit::Queued(1)));

        let hit = |column| session.target_at(20, column).map(|target| target.hit);
        assert_eq!(hit(12), Some(Hit::Queued(1)));
        assert_eq!(hit(5), Some(Hit::Queued(0)));
        assert_eq!(hit(40), None, "the cell after the last one covered");
        assert_eq!(session.target_at(21, 5), None, "the row below");
    }

    /// Rejects: a target in the transcript that is looked up by the row of the screen, which stops
    /// being the row it was drawn at once the view scrolls, and one found outside the area the
    /// transcript was drawn in.
    #[test]
    fn a_target_in_the_transcript_follows_the_view_as_it_scrolls() {
        let mut session = with_a_transcript(0);
        session.laid.targets.push(Target::in_transcript(
            Rect::new(0, 95, 2, 1),
            Hit::Reply(3),
            0,
        ));
        let found = |session: &Session, row, column| {
            session.target_at(row, column).map(|target| target.hit)
        };

        // At the tail the view shows rows 90 to 99, so row 95 is screen row 5.
        assert_eq!(found(&session, 5, 1), Some(Hit::Reply(3)));
        assert_eq!(found(&session, 5, 2), None, "past the cells it covers");
        assert_eq!(found(&session, 4, 1), None, "the row above");

        // Four rows back it is screen row 9, the last of the area, and five rows back it is below it.
        session.scroll = 4;
        assert_eq!(found(&session, 5, 1), None);
        assert_eq!(found(&session, 9, 1), Some(Hit::Reply(3)));
        session.scroll = 5;
        assert_eq!(found(&session, 9, 1), None);
        assert_eq!(
            found(&session, 10, 1),
            None,
            "under the area the transcript is in"
        );
    }

    /// Rejects: a lookup that asks every target the frame recorded whether it is under the cell, which
    /// for a long transcript is tens of thousands at each report of the pointer. Twenty thousand rows
    /// each have a target, and the cell asks of the few on its row, finds the one drawn last of them,
    /// and finds one recorded after the first lookup before any of those.
    #[test]
    fn a_lookup_asks_only_the_targets_that_cover_the_row_of_the_cell() {
        let mut session = with_a_transcript(0);
        session.laid.rows = 20_000;
        session.laid.window = Some(Window {
            area: Rect::new(0, 0, 80, 10),
            top: session.top_row(),
            view: View::of(&session),
        });
        let top = session.top_row();
        for row in 0..session.laid.rows {
            session.laid.targets.push(Target::in_transcript(
                Rect::new(0, row, 2, 1),
                Hit::Reply(usize::from(row)),
                0,
            ));
        }
        // A target of the screen over the cell, recorded before the transcript's, and another over it.
        session
            .laid
            .targets
            .push(Target::on_screen(Rect::new(0, 5, 2, 1), Hit::Queued(0)));
        let asked = |session: &Session| {
            ASKED.with(|asked| asked.set(0));
            let found = session.target_at(5, 1).map(|target| target.hit);
            (found, ASKED.with(std::cell::Cell::get))
        };

        let (found, asked_of) = asked(&session);
        assert_eq!(found, Some(Hit::Queued(0)), "the one drawn last");
        assert!(asked_of <= 4, "{asked_of} targets were asked");

        // What is drawn over it is found first, and so is what a frame recorded after the lookup.
        session
            .laid
            .targets
            .push(Target::on_screen(Rect::new(0, 5, 2, 1), Hit::Queued(1)));
        let (found, asked_of) = asked(&session);
        assert_eq!(
            found,
            Some(Hit::Queued(1)),
            "a target recorded since the lookup"
        );
        assert!(asked_of <= 4, "{asked_of} targets were asked");

        // And a cell with nothing screen-side is the transcript's own row, the right one for the view.
        let row_of_the_cell = top + 7;
        ASKED.with(|asked| asked.set(0));
        let found = session.target_at(7, 1).map(|target| target.hit);
        assert_eq!(found, Some(Hit::Reply(usize::from(row_of_the_cell))));
        assert!(ASKED.with(std::cell::Cell::get) <= 4);
    }

    /// Rejects: a target drawn in a frame that is found after the state has moved on to one where it
    /// does nothing, and one that does nothing hiding a target beneath it that does.
    #[test]
    fn a_target_the_state_no_longer_stands_behind_is_skipped() {
        let mut session = Session::new("none");
        let cell = Rect::new(3, 3, 4, 1);
        session
            .laid
            .targets
            .push(Target::on_screen(cell, Hit::Queued(0)));
        // The way back out of a delegate's view stands only in one.
        session
            .laid
            .targets
            .push(Target::on_screen(cell, Hit::Back));
        let hit = |session: &Session| session.target_at(3, 4).map(|target| target.hit);

        assert_eq!(hit(&session), Some(Hit::Queued(0)), "no view is open");
        assert!(!Hit::Back.stands(&session, &session.laid));
    }

    /// Rejects: the rows of a call counted to the row after its last, or the blocks of a call and of
    /// a delegate read back as one kind. The keys that act on the call at the top of the view and the
    /// click that lays the transcript out again read these rows.
    #[test]
    fn the_rows_of_a_call_and_of_a_delegate_are_read_back_apart_and_inclusive() {
        let mut laid = Laid::default();
        laid.targets.push(Target::in_transcript(
            Rect::new(0, 10, 80, 3),
            Hit::Call(2),
            0,
        ));
        laid.targets.push(Target::in_transcript(
            Rect::new(0, 13, 80, 1),
            Hit::Delegate(3),
            0,
        ));
        laid.targets
            .push(Target::on_screen(Rect::new(0, 10, 80, 3), Hit::Call(9)));

        assert_eq!(laid.calls().collect::<Vec<_>>(), vec![(10, 12, 2)]);
        assert_eq!(
            laid.delegate_blocks().collect::<Vec<_>>(),
            vec![(13, 13, 3)]
        );
    }

    /// Rejects: a target of the frame being found under a panel that holds the input, which a pointer
    /// moved over a picker would otherwise say it could act on, and the panel not being found where
    /// it is drawn.
    #[test]
    fn while_a_panel_holds_the_input_only_the_panel_is_a_target() {
        let mut session = Session::new("none");
        session
            .laid
            .targets
            .push(Target::on_screen(Rect::new(0, 0, 80, 24), Hit::Queued(0)));
        let found = |session: &Session, row, column| {
            session.target_at(row, column).map(|target| target.hit)
        };
        assert_eq!(found(&session, 11, 11), Some(Hit::Queued(0)));

        session.open_picking(crate::picking::Picking::Effort(
            crate::effort_prompt::Picker::new(None),
        ));
        session.laid.targets.extend(
            Popup {
                panel: Panel::Picker,
                area: Rect::new(10, 10, 5, 5),
                list: None,
            }
            .recorded(),
        );
        assert_eq!(found(&session, 11, 11), Some(Hit::Popup(Panel::Picker)));
        assert_eq!(found(&session, 0, 0), None, "beside the panel");
    }
}
