//! The scroller, its search, and the history search.

use super::*;

/// The most steps of an edge scroll a pass of a loop makes up for, however long it was away.
const MOST_STEPS_OWED: u128 = 3;

/// The most rows one step of an edge scroll moves, however far past the edge the pointer is.
const MOST_ROWS_A_STEP: u16 = 4;

/// What the last frame laid the transcript out to.
///
/// Written back after every draw, because none of it is knowable before one: the answers exist
/// only once the paragraph has been wrapped at the width it is being shown at. A key pressed next
/// is then answered against the frame the person is looking at, which is the one this describes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Laid {
    /// Columns the transcript was drawn in.
    pub width: u16,
    /// Columns the whole frame had, which is what decides whether the info panel fits.
    pub columns: u16,
    /// Rows the whole frame had.
    pub lines: u16,
    /// Rows it had room for.
    pub height: u16,
    /// Rows the whole of it came to.
    pub rows: u16,
    /// How many of the last of those rows hold no text: the blank row that follows the last entry or
    /// the reply being written. A selection whose end is below the text ends above them, on the last
    /// row that holds some (MOUSE-1).
    pub closing_blank_rows: u16,
    /// The row each prompt the person typed begins at, in the order they were typed.
    ///
    /// Empty unless the scroller is open, since working it out costs a wrap of every line and
    /// nothing at rest asks the question.
    pub prompts: Vec<u16>,
    /// The row each search match is reached at, top to bottom.
    ///
    /// One entry per match rather than per row, so two hits on one row are two entries holding
    /// that row. The view has one place to go for both of them, but they are two matches: the
    /// footer counts them and `n` steps through each one.
    ///
    /// A match is reached at the row the line holding it begins at, as a prompt is: a line the
    /// width wraps is several rows of the screen and one entry here.
    pub matches: Vec<u16>,
    /// The screen rows of the transcript that continue the line drawn above them, because the
    /// line was wider than the transcript and wrapped, each with whether the wrap broke the line
    /// at a blank. A copy joins them without a line break, and with a space where it did.
    pub continued: Vec<(u16, bool)>,
    /// The regions of the screen a selection can begin in, empty for a view that has none.
    pub panes: Vec<(crate::select::Pane, ratatui::layout::Rect)>,
    /// Where the transcript was drawn and the row of it at the top, where a transcript was drawn
    /// at all (MOUSE-34). What a click on the transcript is placed by, and what a selection of its
    /// rows follows.
    pub window: Option<crate::select::Window>,
    /// What tells each end of the selection of the transcript's row by in this frame, anchor then
    /// head, where there is one and the frame drew the transcript (MOUSE-34).
    pub marks: Option<[crate::select::Mark; 2]>,
    /// What a click on a cell acts on, as each draw recorded it, the last drawn being the topmost
    /// (MOUSE-45). Looked up by [`Session::target_at`] and nowhere else.
    pub targets: Vec<Target>,
    /// [`Laid::targets`] by the row each covers, made the first time a cell is asked about
    /// ([`Laid::find`]).
    pub by_row: super::targets::ByRow,
    /// Cells drawn for a click and no part of the text around them. A copy leaves them out and a
    /// highlight passes over them.
    pub chrome: Vec<ratatui::layout::Rect>,
    /// What each link in the assistant's words opens, which a [`Hit::Link`] target names by its place
    /// here (MOUSE-40).
    pub links: Vec<crate::text_links::Target>,
    /// The row the frame drew its hints in, where the line that says what a click on the target under
    /// the pointer does is drawn too (MOUSE-51).
    pub hint_row: Option<HintRow>,
    /// Where the thumb of the scrollbar the frame drew beside the transcript is, where it drew one
    /// (VIEW-39, MOUSE-49). The track is a target of its own.
    pub bar: Option<Bar>,
}

impl Laid {
    /// How many of the transcript's first rows hold text, which is all of them but the blank ones that
    /// close it.
    pub fn text_rows(&self) -> u16 {
        self.rows.saturating_sub(self.closing_blank_rows)
    }
}

/// The row a frame drew its hints in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HintRow {
    pub area: ratatui::layout::Rect,
    /// Whether the row says something that is not a hint, a message that fills it, so nothing is
    /// drawn beside it.
    pub taken: bool,
}

/// The scrollbar in the last column of the transcript pane.
///
/// The thumb stands for the rows the pane shows among the rows the transcript came to, and a press
/// on the track puts the view where the press is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    /// The cells of the column, one to each row of the pane.
    pub track: ratatui::layout::Rect,
    /// How many rows the transcript came to.
    pub rows: u16,
    /// The row of the transcript at the top of the pane.
    pub top: u16,
}

impl Bar {
    /// The rows of the track the thumb covers, as the first of them counted from the top of the
    /// track and how many there are. At least one, and never the whole track, since a bar is drawn
    /// only where the transcript is longer than the pane and the thumb has to be able to move.
    pub fn thumb(&self) -> (u16, u16) {
        let track = u64::from(self.track.height);
        let rows = u64::from(self.rows).max(track + 1);
        // The share of the track the pane is of the transcript, rounded. With a row more than the
        // pane at least, it comes to a row less than the track at most, so the thumb can move.
        let length = ((track * track * 2 + rows) / (2 * rows)).max(1);
        let range = rows - track;
        let top = u64::from(self.top).min(range);
        let first = ((track - length) * top * 2 + range) / (2 * range);
        (narrowed(first), narrowed(length))
    }

    /// The row of the transcript the pane starts at when the thumb's first row is `first` rows
    /// down the track, which is what [`Bar::thumb`] answers the other way round.
    pub fn top_for(&self, first: u16) -> u16 {
        let (_, length) = self.thumb();
        let room = u64::from(self.track.height)
            .saturating_sub(u64::from(length))
            .max(1);
        let range = u64::from(self.rows).saturating_sub(u64::from(self.track.height));
        let first = u64::from(first).min(room);
        narrowed((first * range * 2 + room) / (2 * room))
    }
}

/// The words at the top of the view: the entry of the transcript the first row on the screen belongs
/// to, and how many rows into the rows that entry takes it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopOfView {
    pub entry: usize,
    pub rows_in: usize,
}

/// A count of rows as the `u16` the layout holds them in. Every count here is at most the rows of the
/// transcript, which is one.
fn narrowed(rows: u64) -> u16 {
    u16::try_from(rows).unwrap_or(u16::MAX)
}

/// What a click on the row under the box does, which is what the key the cell names does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Footer {
    /// The permission mode: cycles it (MOUSE-14).
    Mode,
    /// `show trail` and `hide trail`.
    Trail,
    /// `? for shortcuts`.
    Shortcuts,
    /// The way to the info panel.
    Info,
    /// The way into the view of the delegates and asides.
    Watch,
    /// The way into the view of what commands printed.
    Outputs,
    /// The way to move a command the turn waits on to the background.
    Background,
    /// The way back to the end of a view that is held where it was scrolled to.
    Held,
    /// `esc to cancel`, on the line of shell mode.
    CancelShell,
    /// The key that pastes the picture the clipboard holds.
    Paste,
}

/// Which panel a [`Popup`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// A picker opened while a turn runs.
    Picker,
    /// The scroller's list of keys.
    KeyList,
    /// The search over the prompts already sent, drawn where the box is.
    HistorySearch,
    /// The small menu a right click opens (MOUSE-52).
    Menu,
}

/// A panel drawn over the frame, or in the place of part of it.
///
/// What whatever draws it records, in the frame it is drawn in, so that a press is looked up against
/// what is over the cell rather than against the targets drawn beneath it (MOUSE-45).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Popup {
    pub panel: Panel,
    /// The cells it covers, border included.
    pub area: ratatui::layout::Rect,
    /// The cells of the rows it lists, for a panel whose rows a press can take.
    pub list: Option<ratatui::layout::Rect>,
}

impl Popup {
    /// The targets that record the panel in a frame: the cells it covers, and over them the cells of
    /// its rows, which are found before the rest of the panel.
    pub fn recorded(self) -> Vec<Target> {
        let mut recorded = vec![Target::on_screen(self.area, Hit::Popup(self.panel))];
        if let Some(list) = self.list {
            recorded.push(Target::on_screen(list, Hit::PopupRows(self.panel)));
        }
        recorded
    }
}

/// The scroller, while it is open.
///
/// Holds what the mode itself is doing and nothing else. Where the view is looking stays in the
/// field the wheel and the arrows move at rest, so opening the scroller and closing it again move
/// nothing.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scroller {
    /// What a finished search is looking for. Empty until one has been run.
    pub needle: String,
    /// A search being typed, before Enter runs it or Escape abandons it.
    pub typing: Option<String>,
    /// Whether the key list is up.
    pub help: bool,
    /// Which match the view is on, counted from zero.
    ///
    /// Clamped where it is read. A list laid out afresh can be shorter than the one this indexed,
    /// because a turn goes on writing underneath.
    pub at: usize,
    /// A count typed and waiting for the key it is for.
    pub count: Option<u32>,
    /// The request this scroller is showing in place of the transcript, when `/request` opened it.
    ///
    /// A copy of the value the turn sent, so what is read is that request and a turn going on
    /// underneath cannot change it.
    pub request: Option<Box<bravebot_agent::request_view::RequestView>>,
    /// The view the transcript had when `/request` opened this, put back when it closes.
    pub before: Option<(u16, Laid)>,
}

impl Session {
    /// Open the search over the prompt history.
    ///
    /// Seeded with whatever is in the box, when that is one line the person typed: somebody who
    /// typed half a prompt and then reached for the history has already said what they are looking
    /// for, and a pasted paragraph is not that. The line stays in the box, so leaving puts nothing
    /// back.
    ///
    /// A prompt walked back to is not a line they typed, and seeding with it leaves a search whose
    /// only match is the prompt already in the box: the walk again, one keystroke wider.
    ///
    /// Nothing to search is nothing to open. A panel over the transcript saying a person has never
    /// sent a prompt is a mode they then have to get out of.
    pub fn open_history_search(&mut self) -> bool {
        if self.history.is_empty() {
            return false;
        }
        let typed = !self.input.contains('\n') && !self.history.on_a_sent_prompt();
        let seed = match typed {
            true => self.input.trim().to_string(),
            false => String::new(),
        };
        self.history_search = Some(crate::history_search::Search::looking_for(seed));
        self.pick_click = None;
        true
    }

    /// Open it with the scope already narrowed to the prompts sent from this workspace.
    ///
    /// The way in from a prompt walked back to. Somebody who has walked back at all has said the
    /// prompt they want is an old one, and the workspace they are in is the narrower question; the
    /// wide list is one more press of the same key from there.
    pub fn open_history_search_here(&mut self) {
        self.open_history_search();
        if let Some(search) = &mut self.history_search {
            search.narrow();
        }
    }

    /// Close it, leaving the box as it was.
    pub fn close_history_search(&mut self) {
        self.history_search = None;
    }

    pub fn searching_history(&self) -> bool {
        self.history_search.is_some()
    }

    /// Open a picker, which takes every key until it is left.
    pub fn open_picking(&mut self, picking: crate::picking::Picking) {
        self.picking = Some(picking);
        self.pick_click = None;
    }

    /// The picker that is open, for the renderer and for a test.
    pub fn picking(&self) -> Option<&crate::picking::Picking> {
        self.picking.as_ref()
    }

    /// Give a key to the open picker, and close it when the key left it.
    ///
    /// What was picked is held for [`Session::take_pick`] rather than returned, because the loops
    /// that can carry a pick out are not the ones the key arrived in.
    pub fn press_in_picking(
        &mut self,
        code: ratatui::crossterm::event::KeyCode,
        modifiers: ratatui::crossterm::event::KeyModifiers,
    ) {
        let Some(picking) = &mut self.picking else {
            return;
        };
        match picking.press(code, modifiers) {
            crate::picking::Step::Continue => {}
            crate::picking::Step::Cancelled => self.picking = None,
            crate::picking::Step::Picked(picked) => {
                self.picking = None;
                self.picked = Some(picked);
            }
        }
    }

    /// Whether a model picker is open and its rows have yet to arrive.
    pub fn picking_is_loading(&self) -> bool {
        matches!(&self.picking, Some(crate::picking::Picking::Model(picker)) if picker.is_loading())
    }

    /// Take the rows of an open model picker if they have arrived, without waiting for them.
    ///
    /// Asked on every pass of the loops that own the session, since the rows arrive from a thread
    /// and no key is pressed for them. The sentences about services that could not be read go into
    /// the transcript as the rows arrive. A listing that failed closes the picker and says why, so
    /// the model stays as it was. Returns whether the screen changed.
    pub fn settle_picking(&mut self) -> bool {
        let Some(crate::picking::Picking::Model(picker)) = &mut self.picking else {
            return false;
        };
        match picker.settle() {
            crate::model_prompt::Arrival::Nothing => false,
            crate::model_prompt::Arrival::Rows(problems) => {
                for problem in problems {
                    self.note(problem);
                }
                true
            }
            crate::model_prompt::Arrival::Failed(problem) => {
                self.picking = None;
                self.note(t!(session_models_unavailable, problem = problem));
                true
            }
        }
    }

    /// What the picker closed on, taken so it is carried out once.
    pub fn take_pick(&mut self) -> Option<crate::picking::Picked> {
        self.picked.take()
    }

    /// Whether this mouse event belongs to a panel drawn over the frame and not to the frame
    /// (MOUSE-45).
    ///
    /// A press that lands on one is the panel's, and so are the drag and the release that follow
    /// it, wherever they end. A picker and the scroller's key list hold the input, so while either
    /// is open every press, drag, release and turn of the wheel is theirs, wherever the pointer is.
    /// A picker answers what it can ([`Session::mouse_in_picking`]), and a left press outside the
    /// key list puts it away, as any key does (MOUSE-47). What a panel takes never reaches the
    /// targets drawn beneath it: a row of the info panel, the permission mode, a call in the
    /// scroller or a place in the box.
    ///
    /// A press also drops the click before it, so the release that ends it is not read as the end of
    /// a click on what is beneath, and drops the selection held, which a press on a panel is no
    /// part of.
    pub(crate) fn popup_claims(&mut self, mouse: ratatui::crossterm::event::MouseEvent) -> bool {
        use ratatui::crossterm::event::MouseEventKind::{Down, Drag, ScrollDown, ScrollUp, Up};
        let over = self
            .target_at(mouse.row, mouse.column)
            .is_some_and(|target| target.hit.is_a_panel());
        let key_list = self.scroller.as_ref().is_some_and(|scroller| scroller.help);
        let searching = self.history_search.is_some();
        let modal = self.modal();
        let claimed = match mouse.kind {
            Down(_) => modal || over,
            Up(_) | Drag(_) => modal || self.pressed_on_a_popup,
            ScrollUp | ScrollDown => modal || over,
            _ => false,
        };
        if claimed && matches!(mouse.kind, Down(_)) {
            self.last_click = None;
            // Not while the menu is up: it was opened for the selection held, the item that copies
            // it needs it, and closing the menu changes nothing else (MOUSE-52).
            if self.menu.is_none() {
                self.clear_selection();
            }
        }
        if claimed && self.menu.is_some() {
            self.mouse_in_the_menu(mouse);
        } else if claimed && self.picking.is_some() {
            self.mouse_in_picking(mouse);
        } else if claimed && searching {
            self.mouse_in_history_search(mouse);
        }
        if claimed
            && key_list
            && !over
            && mouse.kind
                == ratatui::crossterm::event::MouseEventKind::Down(
                    ratatui::crossterm::event::MouseButton::Left,
                )
        {
            self.toggle_scroller_help();
        }
        match mouse.kind {
            Down(_) => self.pressed_on_a_popup = claimed,
            Up(_) => self.pressed_on_a_popup = false,
            _ => {}
        }
        claimed
    }

    /// Say that a picker which has just closed answered the press the pointer may still be holding.
    ///
    /// A picker that runs in a loop of its own reads the presses itself, so the release of the one
    /// that closed it arrives afterwards, in the loop of the session. The click before it is dropped
    /// and the release is taken, so a press on a row of the info panel that closed a picker is not
    /// completed as a click on that row (MOUSE-45).
    pub fn end_a_pickers_press(&mut self) {
        self.last_click = None;
        self.pressed_on_a_popup = true;
    }

    /// Give a mouse event to the search over the prompts already sent, as it was drawn.
    ///
    /// A press on a match puts the cursor on it and a second on the same prompt takes it into the
    /// box, as Enter does. The wheel walks the matches by `tui.wheelRows`, up to the older ones, and
    /// a left press outside the panel closes the search, as Escape does (MOUSE-15, MOUSE-46,
    /// MOUSE-47).
    fn mouse_in_history_search(&mut self, mouse: ratatui::crossterm::event::MouseEvent) {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                for _ in 0..self.wheel_rows() {
                    self.history_search_older();
                }
            }
            MouseEventKind::ScrollDown => {
                for _ in 0..self.wheel_rows() {
                    self.history_search_newer();
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let cell = ratatui::layout::Position::new(mouse.column, mouse.row);
                let Some(panel) = self.laid.popup(Panel::HistorySearch) else {
                    return;
                };
                if !panel.area.contains(cell) {
                    self.pick_click = None;
                    self.close_history_search();
                    return;
                }
                let Some(Hit::Match(index)) = self
                    .target_at(mouse.row, mouse.column)
                    .map(|target| target.hit)
                else {
                    self.pick_click = None;
                    return;
                };
                let matches = self.history_matches().len();
                // The prompt the press is on, counted from the oldest as the rows are: what a
                // second press is held to.
                let prompt = self
                    .history_matches()
                    .get(index)
                    .map(|entry| entry.prompt.clone());
                // The rows drawn stay where they were: the cursor moves to a prompt already on the
                // screen. The first of them is the oldest of the rows the last frame recorded.
                let first_drawn = self
                    .laid
                    .targets
                    .iter()
                    .filter_map(|target| match target.hit {
                        Hit::Match(shown) => Some(shown),
                        _ => None,
                    })
                    .min();
                if let Some(search) = &mut self.history_search {
                    if let Some(first) = first_drawn {
                        search.hold_from(first);
                    }
                    search.select(index, matches);
                }
                let (click, count) = crate::select::count_press_on(
                    self.pick_click,
                    std::time::Instant::now(),
                    (mouse.row, mouse.column),
                    &prompt,
                );
                self.pick_click = Some(click);
                if count == 2 {
                    self.pick_click = None;
                    self.take_history_match();
                }
            }
            _ => {}
        }
    }

    /// Give a mouse event to the picker that is open, as it was drawn, and close it when the event
    /// left it. What was picked is held for [`Session::take_pick`], as a key's is.
    fn mouse_in_picking(&mut self, mouse: ratatui::crossterm::event::MouseEvent) {
        let Some(popup) = self.laid.popup(Panel::Picker) else {
            return;
        };
        let wheel = self.wheel_rows();
        let Some(picking) = &mut self.picking else {
            return;
        };
        match picking.pointer(
            &popup,
            mouse,
            &mut self.pick_click,
            std::time::Instant::now(),
            wheel,
        ) {
            crate::picking::Step::Continue => {}
            crate::picking::Step::Cancelled => {
                self.picking = None;
                self.pick_click = None;
            }
            crate::picking::Step::Picked(picked) => {
                self.picking = None;
                self.pick_click = None;
                self.picked = Some(picked);
            }
        }
    }

    /// What is being searched for, for the renderer and for a test.
    pub fn history_search(&self) -> Option<&crate::history_search::Search> {
        self.history_search.as_ref()
    }

    /// The prompts the open search answers with, oldest first.
    ///
    /// Empty when nothing is open, so a caller need not ask twice.
    pub fn history_matches(&self) -> Vec<&bravebot_session::store::Entry> {
        match &self.history_search {
            Some(search) => search.matching(&self.history),
            None => Vec::new(),
        }
    }

    /// The prompt under the cursor in the open search.
    pub fn history_match(&self) -> Option<&bravebot_session::store::Entry> {
        let matching = self.history_matches();
        let at = self.history_search.as_ref()?.at(matching.len())?;
        matching.get(at).copied()
    }

    /// Narrow the search by one character.
    pub fn type_into_history_search(&mut self, c: char) {
        if let Some(search) = &mut self.history_search {
            search.typed(c);
        }
    }

    /// Widen it by one, reporting whether there was anything to remove.
    pub fn backspace_history_search(&mut self) -> bool {
        match &mut self.history_search {
            Some(search) => search.backspace(),
            None => false,
        }
    }

    /// Move the cursor to an older match.
    pub fn history_search_older(&mut self) {
        let matches = self.history_matches().len();
        if let Some(search) = &mut self.history_search {
            search.older(matches);
        }
    }

    /// Move the cursor to a newer match.
    pub fn history_search_newer(&mut self) {
        if let Some(search) = &mut self.history_search {
            search.newer();
        }
    }

    /// Move the search to the next of everywhere, this project and this session that holds a
    /// prompt.
    pub fn scope_history_search(&mut self) {
        if let Some(search) = &mut self.history_search {
            search.cycle(&self.history);
        }
    }

    /// Take the prompt under the cursor into the box, and close the search.
    ///
    /// Into the box rather than sent, because a stored line is content: the file it came from can
    /// be edited, and on a shared machine by somebody else. The keystroke that sends it is the
    /// person's, exactly as it would have been had they typed the line.
    ///
    /// It replaces what was in the box, since what was in the box is what the search was seeded
    /// with: keeping both would leave the person editing their own half-typed line with the
    /// recalled one appended to it.
    pub fn take_history_match(&mut self) -> bool {
        let Some(prompt) = self.history_match().map(|entry| entry.prompt.clone()) else {
            return false;
        };
        self.close_history_search();
        self.set_input(prompt);
        self.history.leave();
        true
    }

    /// Open the scroller as Ctrl-O does, with the words at the top of the view still at the top
    /// (SCROLL-1).
    ///
    /// The scroller draws the transcript at the full width of the screen and the plain frame at the
    /// width the info panel and the scrollbar leave, so the row at the top is not the same words in the
    /// two, and holding the row number moves the view. `top_of_view` finds the words in the layout the
    /// last frame drew, before the scroller is open, and `place` finds them in the layout the scroller
    /// will be drawn in, which is also laid out afresh here. Where no frame has been drawn there is
    /// nothing to hold the words by, and the row is held as [`Session::open_scroller`] holds it.
    pub fn open_the_scroller(
        &mut self,
        top_of_view: impl Fn(&Session) -> Option<TopOfView>,
        place: impl Fn(&Session, TopOfView, u16, u16) -> Option<(Laid, u16)>,
    ) {
        let top = top_of_view(self);
        self.open_scroller();
        let (width, height) = (self.laid.columns, self.laid.lines.saturating_sub(1));
        let Some(top) = top.filter(|_| width != 0 && height != 0) else {
            return;
        };
        if let Some((laid, row)) = place(self, top, width, height) {
            self.laid = laid;
            self.scroller_to_row(row);
        }
    }

    /// Open the scroller on the view already on the screen.
    ///
    /// Nothing about the view is touched. The scroller reads the offset the wheel writes, so the
    /// row under somebody's eye when they press the key is the row under it afterwards. The
    /// screen around it does change shape, since the box and the indicator come off it, and the
    /// rows they were using are given to the transcript. What that means for the view is settled
    /// where every other change of shape is settled, in [`Session::note_layout`]: the row at the
    /// top stays where it is, and the rows gained appear beneath it, which is where the box that
    /// gave them up was.
    pub fn open_scroller(&mut self) {
        self.scroller = Some(Scroller::default());
    }

    /// Close the scroller as the keys do, with the words at the top of the view still at the top
    /// (SCROLL-1).
    ///
    /// The plain frame draws the transcript at the width the info panel and the scrollbar leave, which
    /// is known only to the draw, so the words are held for the first frame that draws it
    /// ([`Session::words_held`]) and not worked out here. `top_of_view` finds them in the scroller's
    /// layout, while it is open.
    pub fn close_the_scroller(&mut self, top_of_view: impl Fn(&Session) -> Option<TopOfView>) {
        let words = (!self.viewing_request())
            .then(|| top_of_view(self))
            .flatten();
        self.close_scroller();
        self.words_held = words;
    }

    /// The words the view was left on by the scroller that has just closed, for the first frame
    /// after it to put at the top.
    pub fn words_held(&self) -> Option<TopOfView> {
        self.words_held
    }

    /// Close it, leaving the view where it was left.
    pub fn close_scroller(&mut self) {
        if let Some(Scroller {
            before: Some((scroll, laid)),
            ..
        }) = self.scroller.take()
        {
            self.scroll = scroll;
            self.laid = laid;
        }
    }

    /// Keep the request a turn is about to send, replacing the one before it.
    pub fn set_last_request(&mut self, view: bravebot_agent::request_view::RequestView) {
        self.last_request = Some(view);
    }

    /// Open the scroller on the last request a turn built (`/request`).
    ///
    /// Read from the value the turn handed over, so it shows what was sent and nothing is rebuilt
    /// from the transcript. Before any turn has sent one there is nothing to open, which is said.
    pub fn show_request(&mut self) {
        let Some(view) = self.last_request.clone() else {
            self.note(t!(request_none_yet));
            return;
        };
        let before = (self.scroll, self.laid.clone());
        // The first row, counted from the end of the layout the last frame drew; the next frame
        // measures the request and holds this row.
        self.scroll = self.furthest();
        self.scroller = Some(Scroller {
            request: Some(Box::new(view)),
            before: Some(before),
            ..Scroller::default()
        });
    }

    /// Whether the scroller is showing a request rather than the transcript.
    pub fn viewing_request(&self) -> bool {
        self.scroller
            .as_ref()
            .is_some_and(|scroller| scroller.request.is_some())
    }

    pub fn scrolling(&self) -> bool {
        self.scroller.is_some()
    }

    /// What the scroller is doing, for the renderer and for a test.
    pub fn scroller(&self) -> Option<&Scroller> {
        self.scroller.as_ref()
    }

    /// Take what the last frame laid out, and hold the view on the row it was showing.
    ///
    /// The offset is counted from the end, and while the scroller is open both ends move: rows
    /// arrive underneath as a turn writes them, and the screen changes shape when the box comes
    /// off it. Either would otherwise slide the view down the transcript. Holding the view is the
    /// whole of what the scroller is for, so the offset is worked out afresh from the row that was
    /// at the top, rather than carried across a layout it was measured against.
    ///
    /// A view sitting at the tail with nothing open stays at the tail, since somebody watching a
    /// reply arrive is watching the end of it. An open scroller holds even that, because opening it
    /// is somebody saying they have stopped watching. A view scrolled away from the tail is held
    /// wherever it was left, the wheel at rest as much as the scroller, so the lines a reply adds
    /// under it do not drag it back down.
    pub fn note_layout(&mut self, laid: Laid) {
        // The frame that has just placed the words a scroller was closed on drew them at the top, and
        // the row it drew at the top is the one to hold.
        let placed = self
            .words_held
            .take()
            .and(laid.window)
            .map(|window| window.top);
        if self.scrolling() || self.scroll > 0 {
            let top = placed.unwrap_or_else(|| self.top_row());
            let furthest = laid.rows.saturating_sub(laid.height);
            self.scroll = furthest.saturating_sub(top);
        }
        self.keep_selection_within(&laid);
        self.laid = laid;
        self.follow_the_hover();
    }

    /// Hold a selection to what it began on as that is now drawn, and drop it when that is gone or
    /// the terminal changed size, since the cells then mean something else.
    ///
    /// A selection in a pane that stays where it is follows the pane's rectangle and is dropped when
    /// the pane is no longer drawn. A selection of the transcript counts its rows and is dropped when
    /// the transcript is no longer drawn, is drawn in another view or at another width, or has fewer
    /// rows than the selection reaches, since the rows then hold other text (MOUSE-3, MOUSE-34).
    fn keep_selection_within(&mut self, laid: &Laid) {
        let Some(selection) = &mut self.selection else {
            return;
        };
        let resized = self.laid.columns != 0
            && (self.laid.columns != laid.columns || self.laid.lines != laid.lines);
        let kept = !resized
            && if selection.scrolls() {
                laid.window
                    .is_some_and(|window| selection.follow(window, laid.rows))
                    && laid.marks.is_none_or(|now| selection.keep_marks(now))
            } else {
                let pane = selection.pane();
                let region = laid.panes.iter().find(|(kind, _)| *kind == pane);
                let alive = if pane == crate::select::Pane::Screen {
                    laid.panes.is_empty()
                } else {
                    region.is_some()
                };
                if let Some((_, rect)) = region.filter(|_| alive) {
                    selection.rebound(*rect);
                }
                alive
            };
        if !kept {
            self.selection = None;
        }
    }

    /// How far back the view can go before it is looking at the first row.
    fn furthest(&self) -> u16 {
        self.laid.rows.saturating_sub(self.laid.height)
    }

    /// The row at the top of the view.
    pub fn top_row(&self) -> u16 {
        self.furthest()
            .saturating_sub(self.scroll.min(self.furthest()))
    }

    /// Whether the plain frame draws a scrollbar beside the transcript where the transcript is longer
    /// than the pane (VIEW-39). Only while the mouse is captured, since the bar is there to be
    /// pressed, and not over the search of earlier prompts, which holds the input.
    pub fn shows_a_scrollbar(&self) -> bool {
        self.mouse_captured() && !self.searching_history()
    }

    /// Take hold of the scrollbar where a press landed, and say whether it landed on it.
    ///
    /// A press on the thumb holds it where it is, so the view does not move until the pointer does.
    /// A press elsewhere in the track puts the middle of the thumb at the pointer, and a drag after
    /// it carries on from there (MOUSE-49). Neither begins a selection.
    pub fn begin_bar_drag(&mut self, row: u16, column: u16) -> bool {
        let (
            Some(bar),
            Some(Target {
                hit: Hit::Scrollbar,
                ..
            }),
        ) = (self.laid.bar, self.target_at(row, column))
        else {
            return false;
        };
        let at = row - bar.track.y;
        let (first, length) = bar.thumb();
        let grab = if (first..first + length).contains(&at) {
            at - first
        } else {
            let grab = length / 2;
            self.scroller_to_row(bar.top_for(at.saturating_sub(grab)));
            grab
        };
        self.bar_drag = Some(grab);
        self.clear_selection();
        self.last_click = None;
        true
    }

    /// Whether a drag that began on the scrollbar is still held.
    pub fn dragging_bar(&self) -> bool {
        self.bar_drag.is_some()
    }

    /// Carry the thumb to the row the pointer is on, keeping the place on it that was taken hold of.
    /// A pointer above or below the track puts it at the first or the last row.
    pub fn drag_bar_to(&mut self, row: u16) {
        let (Some(grab), Some(bar)) = (self.bar_drag, self.laid.bar) else {
            return;
        };
        let at = row.saturating_sub(bar.track.y);
        self.scroller_to_row(bar.top_for(at.saturating_sub(grab)));
    }

    /// Let go of the scrollbar.
    pub fn end_bar_drag(&mut self) {
        self.bar_drag = None;
    }

    /// End every drag the left button holds: the sweep of a selection, the thumb of the scrollbar and
    /// the border of the panel. They end when the button comes up, and where it does not say so they
    /// end here: the window loses focus, which is the last the terminal says of a button let go
    /// outside it, the terminal is resized, or a press lands, which is not the end of the drag it
    /// finds held and begins one of its own where it is on a thumb or a border (MOUSE-37, MOUSE-49,
    /// PANEL-6).
    pub fn end_the_drags(&mut self) {
        self.stop_sweeping();
        self.end_bar_drag();
        if self.dragging_panel() {
            self.end_panel_drag();
        }
    }

    /// Move the view back by `rows`, stopping at the first row rather than counting past it.
    pub fn scroller_back(&mut self, rows: u16) {
        self.scroll = self.scroll.saturating_add(rows).min(self.furthest());
    }

    /// Move the view on by `rows`, stopping at the last.
    pub fn scroller_on(&mut self, rows: u16) {
        self.scroll = self.scroll.saturating_sub(rows);
    }

    pub fn scroller_to_first_row(&mut self) {
        self.scroll = self.furthest();
    }

    pub fn scroller_to_last_row(&mut self) {
        self.scroll = 0;
    }

    /// Half a screen, and never nothing: a view one row tall still has to move when asked.
    pub fn half_screen(&self) -> u16 {
        (self.laid.height / 2).max(1)
    }

    pub fn whole_screen(&self) -> u16 {
        self.laid.height.max(1)
    }

    /// Put `row` at the top of the view.
    fn scroller_to_row(&mut self, row: u16) {
        self.scroll = self.furthest().saturating_sub(row);
    }

    /// Move the view to the plan item `item`, which is what a click on its row in the panel asks for
    /// (MOUSE-31).
    ///
    /// Where the plan has been written into the transcript by a turn that ended, the view goes to the
    /// row that turn drew the task on, which `row_of` says, and puts it at the top as every jump here
    /// does. Where the plan is the list the turn in flight reported, it is drawn above the box and
    /// the turn's own words are at the end of the transcript, so the view goes to the end. The place
    /// is read from where the plan is held, and nothing in a task's words decides it.
    pub fn go_to_task(&mut self, item: usize, row_of: impl Fn(&Session, usize) -> Option<u16>) {
        match self.plan_entry() {
            None => self.scroller_to_last_row(),
            Some(_) => {
                if let Some(row) = row_of(self, item) {
                    self.scroller_to_row(row);
                }
            }
        }
    }

    /// Move `times` prompts back, or to the first row past the earliest.
    ///
    /// Where these land is settled by what the person typed and by nothing read out of the
    /// workspace: a prompt is the one thing in a transcript they wrote themselves. Every step that
    /// moves reaches another prompt or the end, so stopping at the first that moves nothing bounds a
    /// count by how many prompts there are.
    pub fn to_previous_prompt(&mut self, times: u32) {
        for _ in 0..times {
            let top = self.top_row();
            match self.laid.prompts.iter().rev().find(|row| **row < top) {
                Some(row) => {
                    let row = *row;
                    self.scroller_to_row(row)
                }
                None => self.scroller_to_first_row(),
            }
            if self.top_row() == top {
                break;
            }
        }
    }

    /// Move `times` prompts on, or to the last row past the latest.
    pub fn to_next_prompt(&mut self, times: u32) {
        for _ in 0..times {
            let top = self.top_row();
            match self.laid.prompts.iter().find(|row| **row > top) {
                Some(row) => {
                    let row = *row;
                    self.scroller_to_row(row)
                }
                None => self.scroller_to_last_row(),
            }
            if self.top_row() == top {
                break;
            }
        }
    }

    /// Expand the call nearest the top of the view, among those on the screen, past its glimpse or its twelve lines of change,
    /// or collapse it again, and keep the view on it.
    ///
    /// `relaid` lays the transcript out afresh, since the rows move once a call changes size.
    /// Only the session's own transcript expands: a delegate's block draws its calls as one row.
    pub fn toggle_expanded(&mut self, relaid: impl Fn(&Session) -> Laid) {
        if self.watched_delegate().is_some() {
            return;
        }
        let top = self.top_row();
        let bottom = top.saturating_add(self.laid.height);
        let Some((start, _, index)) = self
            .laid
            .calls()
            .find(|(start, end, _)| *end >= top && *start < bottom)
        else {
            return;
        };
        self.flip_call(index, start, relaid);
    }

    /// The words of the reply the transcript entry at `index` holds, as the transcript holds them,
    /// where it is a reply that is not blank (MOUSE-52).
    pub fn reply_to_copy(&self, index: usize) -> Option<String> {
        self.transcript
            .get(index)
            .filter(|entry| entry.speaker == Speaker::Assistant && !entry.text.trim().is_empty())
            .map(|entry| entry.text.clone())
    }

    /// The words a click on the marker that leads a reply, or on the word at the right of a fenced
    /// block's header, copies, as the transcript holds them (MOUSE-26, MOUSE-27).
    ///
    /// The marker that leads a finished reply and the word on the header of one of its blocks are
    /// the only cells that name anything to copy. A reply still arriving is not an entry yet and
    /// has neither. A block is its lines as the reply wrote them, with no fence and none of what
    /// the screen drew around them. `None` for any other target.
    pub fn words_to_copy(&self, hit: Hit) -> Option<String> {
        let (index, block) = match hit {
            Hit::Reply(index) => (index, None),
            Hit::Block(index, number) => (index, Some(number)),
            _ => return None,
        };
        let entry = self
            .transcript
            .get(index)
            .filter(|entry| entry.speaker == Speaker::Assistant)?;
        let Some(number) = block else {
            return Some(entry.text.clone());
        };
        crate::markdown::block_text(&entry.text, number)
    }

    /// Open the scroller on the call of the transcript entry at `index`, whose heading is drawn at row
    /// `first` of the transcript, with the call expanded, as Ctrl-O and then `x` do (MOUSE-25).
    ///
    /// Says whether it opened. Expansion is drawn only in the scroller, so a click at rest on a call
    /// that has more to show is the way into it. The call's heading stays on the screen row it was
    /// drawn on, with the rest of what it returned under it, unless that would leave the end of the
    /// call below the screen. The view then moves on far enough to show the whole call, or to put its
    /// heading at the top where the call is taller than the screen, since a call opened at the bottom
    /// of the screen is the usual one. `relaid` lays the transcript out at the size the scroller will
    /// be drawn at, which is the whole screen but the footer and not the pane the transcript has
    /// beside the info panel. Nothing happens where the scroller is open.
    pub fn open_the_scroller_on_the_call(
        &mut self,
        index: usize,
        first: u16,
        relaid: impl Fn(&Session, u16, u16) -> Laid,
    ) -> bool {
        if self.scrolling() {
            return false;
        }
        let (width, height) = (self.laid.columns, self.laid.lines.saturating_sub(1));
        if width == 0 || height == 0 {
            return false;
        }
        // Where the heading was on the screen, or the first row where it is above it.
        let anchor = first.saturating_sub(self.top_row());
        self.open_scroller();
        if let Some(entry) = self.transcript.get_mut(index) {
            entry.expanded = true;
        }
        self.laid = relaid(self, width, height);
        let (start, end) = self
            .laid
            .calls()
            .find(|(_, _, held)| *held == index)
            .map_or((first, first), |(start, end, _)| (start, end));
        let shown = start
            .saturating_sub(anchor)
            .max(end.saturating_add(1).saturating_sub(height))
            .min(start);
        self.scroller_to_row(shown);
        true
    }

    /// Open the view of the delegate whose block is the transcript entry at `index`, as a click on
    /// the block does (MOUSE-10, MOUSE-23).
    ///
    /// Says whether it opened. The view is the one the watch key opens on that delegate, and it
    /// closes the same way.
    ///
    /// The delegate is the one the entry holds, found by its place among the delegates the session
    /// has started and not by its number: the driver numbers them from one in each turn, so a number
    /// names a delegate of every turn that started enough.
    pub fn watch_delegate(&mut self, index: usize) -> bool {
        if !self
            .transcript
            .get(index)
            .is_some_and(|entry| entry.delegate.is_some())
        {
            return false;
        }
        let place = self.transcript[..index]
            .iter()
            .filter(|entry| entry.delegate.is_some())
            .count();
        self.held_view = Some(self.scroll);
        self.scroll = 0;
        self.watching = Some(Watching {
            scope: WatchScope::Agents,
            at: self.asides.len() + place,
            listing: false,
            on_session: false,
        });
        true
    }

    /// Flip one call and lay the transcript out again, keeping the view on the call.
    ///
    /// A selection is dropped: the rows below the call move with its size, so the rows it holds
    /// would hold other text (MOUSE-34).
    pub(crate) fn flip_call(
        &mut self,
        index: usize,
        start: u16,
        relaid: impl Fn(&Session) -> Laid,
    ) {
        self.selection = None;
        let top = self.top_row();
        if let Some(entry) = self.transcript.get_mut(index) {
            entry.expanded = !entry.expanded;
        }
        self.laid = relaid(self);
        self.scroller_to_row(start.min(top));
    }

    /// Which way the view is being asked to scroll by a drag held outside the transcript, and how
    /// many rows past its edge the pointer is.
    ///
    /// Only while the button is held and has moved a selection of the transcript, and only where the
    /// view can go that way: a view already at the first row has nothing further up to show.
    fn edge_pull(&self) -> Option<(crate::select::Edge, u16)> {
        use crate::select::Edge;
        if !self.sweeping {
            return None;
        }
        let selection = self.selection.as_ref()?;
        let (row, _) = self.pointer?;
        let (edge, distance) = selection.past_an_edge(row, self.laid.lines)?;
        let room = match edge {
            Edge::Above => self.top_row(),
            Edge::Below => self.rows_below(),
        };
        (room > 0).then_some((edge, distance))
    }

    /// Whether a drag held outside the transcript is waiting on the view to scroll, so the loop that
    /// reads the mouse wakes in time for the next step and not when the person next moves.
    pub fn pulling_at_an_edge(&self) -> bool {
        self.edge_pull().is_some()
    }

    /// Scroll the transcript a step toward a drag held outside it, extending the selection to the
    /// row that comes into view, and say whether the view moved (MOUSE-37).
    ///
    /// The pointer of a drag held still sends nothing, so this is called by the loops that read the
    /// mouse, on their own clock, and a step is taken for each [`crate::select::EDGE_TICK`] since
    /// the last, so a loop that wakes less often scrolls as far. The further past the edge the
    /// pointer is, the more rows a step moves.
    pub fn scroll_toward_the_pointer(&mut self, now: Instant) -> bool {
        let Some((edge, distance)) = self.edge_pull() else {
            self.edge_clock = None;
            return self.keep_at_the_end_of_the_view();
        };
        let owed = match self.edge_clock {
            Some(last) => (now.saturating_duration_since(last).as_millis()
                / crate::select::EDGE_TICK.as_millis())
            .min(MOST_STEPS_OWED),
            None => 1,
        };
        if owed == 0 {
            return false;
        }
        self.edge_clock = Some(now);
        let rows = u16::try_from(owed).unwrap_or(u16::MAX) * distance.clamp(1, MOST_ROWS_A_STEP);
        match edge {
            crate::select::Edge::Above if self.scrolling() => self.scroller_back(rows),
            crate::select::Edge::Above => self.scroll_up(rows.min(self.top_row())),
            crate::select::Edge::Below if self.scrolling() => self.scroller_on(rows),
            crate::select::Edge::Below => self.scroll_down(rows),
        }
        self.follow_the_view();
        true
    }

    /// Move the loose end of the selection of a drag held past an edge the view cannot scroll
    /// further to the row at that edge, and say whether it moved.
    ///
    /// At the tail the view follows the rows a turn writes, so the last row in view is a different
    /// row from one frame to the next, and a pointer held below the transcript means the last row.
    fn keep_at_the_end_of_the_view(&mut self) -> bool {
        let Some((row, _)) = self.pointer else {
            return false;
        };
        let held_past_an_edge = self.sweeping
            && self
                .selection
                .as_ref()
                .is_some_and(|selection| selection.past_an_edge(row, self.laid.lines).is_some());
        if !held_past_an_edge {
            return false;
        }
        let before = self.selection;
        self.follow_the_view();
        self.selection != before
    }

    /// Put the window the last frame recorded, and the rows a selection of the transcript counts, on
    /// the rows the view shows now.
    ///
    /// A wheel turn or a key moves the view and the frame that draws it comes later, since a frame is
    /// waited out while reports stream in. A press or a drag handled in between is placed by this, and
    /// by the rows the view was on before it would be placed on the wrong text (MOUSE-34).
    pub(crate) fn settle_the_view(&mut self) {
        let top = self.top_row();
        let Some(window) = self.laid.window.as_mut() else {
            return;
        };
        window.top = top;
        let (window, rows) = (*window, self.laid.rows);
        if let Some(selection) = &mut self.selection {
            selection.follow(window, rows);
        }
    }

    /// Put the selection on the rows the view now shows and its loose end where the pointer is.
    fn follow_the_view(&mut self) {
        let (Some(window), Some((row, column))) = (self.laid.window, self.pointer) else {
            return;
        };
        let window = crate::select::Window {
            top: self.top_row(),
            ..window
        };
        self.laid.window = Some(window);
        let (rows, text_rows) = (self.laid.rows, self.laid.text_rows());
        if let Some(selection) = &mut self.selection
            && selection.follow(window, rows)
        {
            selection.extend_within(row, column, text_rows);
        }
    }

    /// Start typing a search, with nothing in it yet.
    pub fn begin_search(&mut self) {
        if let Some(scroller) = &mut self.scroller {
            scroller.typing = Some(String::new());
        }
    }

    /// Whether a search is being typed, which is what makes a letter a letter again.
    pub fn typing_a_search(&self) -> bool {
        self.scroller
            .as_ref()
            .is_some_and(|scroller| scroller.typing.is_some())
    }

    pub fn type_into_search(&mut self, c: char) {
        if let Some(typing) = self.scroller.as_mut().and_then(|s| s.typing.as_mut()) {
            typing.push(c);
        }
    }

    /// Take the last character back, and say whether there was one.
    ///
    /// An empty needle backspaced into is the search being abandoned, which is what the key means
    /// when there is nothing left of what it deletes.
    pub fn backspace_search(&mut self) -> bool {
        match self.scroller.as_mut().and_then(|s| s.typing.as_mut()) {
            Some(typing) => typing.pop().is_some(),
            None => false,
        }
    }

    /// Clear a finished search, and say whether there was one to clear.
    ///
    /// The highlights go and the view stays. Somebody who has found what they were looking for
    /// wants the marks off the screen, not to be put back at the box.
    pub fn clear_search(&mut self) -> bool {
        match &mut self.scroller {
            Some(scroller) if !scroller.needle.is_empty() => {
                scroller.needle.clear();
                scroller.at = 0;
                true
            }
            _ => false,
        }
    }

    /// Abandon a search being typed, leaving the view where it was.
    pub fn abandon_search(&mut self) {
        if let Some(scroller) = &mut self.scroller {
            scroller.typing = None;
        }
    }

    /// Run what has been typed, and say what is now being looked for.
    ///
    /// The rows it matches are not known here. They come from a layout at the width the transcript
    /// is drawn in, which is the renderer's to do, so the caller runs this and then lands the view
    /// on what the layout found.
    pub fn run_search(&mut self) {
        if let Some(scroller) = &mut self.scroller
            && let Some(typed) = scroller.typing.take()
        {
            scroller.needle = typed;
            scroller.at = 0;
        }
    }

    /// What a finished search is looking for, which is what the renderer highlights.
    pub fn needle(&self) -> &str {
        self.scroller
            .as_ref()
            .map_or("", |scroller| scroller.needle.as_str())
    }

    /// Land on the first match at or after the top of the view, wrapping to the first of all.
    ///
    /// At or after, rather than after, because a search run while a match is already on the top
    /// row has found that one and should not step over it.
    pub fn land_on_a_match(&mut self, rows: &[u16]) {
        let top = self.top_row();
        let landing = rows
            .iter()
            .position(|row| *row >= top)
            .or(if rows.is_empty() { None } else { Some(0) });
        self.land_at(rows, landing);
    }

    /// Walk to the next match or the previous one, wrapping at either end.
    ///
    /// `rows` holds the row each match was drawn on, one entry per match, so two matches on one
    /// row are two presses: the view stays where it is for the second and the footer says which
    /// of the two it is on. That is what makes the count reachable, since a step that moved the
    /// view would have nowhere to go.
    ///
    /// Counting from the match the view last landed on where it is still there, and from the view
    /// otherwise: somebody who has scrolled away means the next match from what they are looking
    /// at rather than from where the key last took them.
    pub fn to_a_match(&mut self, rows: &[u16], forwards: bool) {
        let top = self.top_row();
        let landing = match self.on_a_match(rows) {
            Some(at) if forwards => Some((at + 1) % rows.len()),
            Some(at) => Some(at.checked_sub(1).unwrap_or(rows.len() - 1)),
            None if forwards => rows
                .iter()
                .position(|row| *row > top)
                .or(if rows.is_empty() { None } else { Some(0) }),
            None => rows
                .iter()
                .rposition(|row| *row < top)
                .or(rows.len().checked_sub(1)),
        };
        self.land_at(rows, landing);
    }

    /// Which match the view is on, where it is still on the one it last landed on.
    ///
    /// Landing on a row nearer the end than a screen leaves the view a screen short of it, so
    /// what the view is on is the row landing there would have reached rather than the row
    /// itself.
    fn on_a_match(&self, rows: &[u16]) -> Option<usize> {
        let at = self.scroller.as_ref()?.at;
        let row = *rows.get(at)?;
        (row.min(self.furthest()) == self.top_row()).then_some(at)
    }

    fn land_at(&mut self, rows: &[u16], landing: Option<usize>) {
        let Some(index) = landing else {
            return;
        };
        let row = rows[index];
        self.scroller_to_row(row);
        if let Some(scroller) = &mut self.scroller {
            scroller.at = index;
        }
    }

    pub fn toggle_scroller_help(&mut self) {
        if let Some(scroller) = &mut self.scroller {
            scroller.help = !scroller.help;
        }
    }

    /// Take a key as the next digit of a count, or `false` where it is not one.
    pub fn count_in_the_scroller(&mut self, c: char) -> bool {
        let Some(scroller) = self.scroller.as_mut() else {
            return false;
        };
        match crate::vim::counted(scroller.count, c) {
            Some(count) => {
                scroller.count = Some(count);
                true
            }
            None => false,
        }
    }

    /// Take the count waiting for this key, so that the key after it starts with none.
    pub fn take_the_scroller_count(&mut self) -> Option<u32> {
        self.scroller
            .as_mut()
            .and_then(|scroller| scroller.count.take())
    }

    /// How many rows of transcript sit below the view, which is what has yet to be read.
    pub fn rows_below(&self) -> u16 {
        self.scroll.min(self.furthest())
    }

    pub fn scroll_up(&mut self, lines: u16) {
        self.scroll = self.scroll.saturating_add(lines);
    }

    pub fn scroll_down(&mut self, lines: u16) {
        self.scroll = self.scroll.saturating_sub(lines);
    }
}
