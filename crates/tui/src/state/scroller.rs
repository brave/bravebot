//! The scroller, its search, and the history search.

use super::*;

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
    /// Rows it had room for.
    pub height: u16,
    /// Rows the whole of it came to.
    pub rows: u16,
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
    /// The first row, the last row and the transcript index of each call that has more to show
    /// than it draws, top to bottom. Empty unless the scroller is open.
    pub expandable: Vec<(u16, u16, usize)>,
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

    /// What the picker closed on, taken so it is carried out once.
    pub fn take_pick(&mut self) -> Option<crate::picking::Picked> {
        self.picked.take()
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
        if self.scrolling() || self.scroll > 0 {
            let top = self.top_row();
            let furthest = laid.rows.saturating_sub(laid.height);
            self.scroll = furthest.saturating_sub(top);
        }
        self.laid = laid;
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
        let Some(&(start, _, index)) = self
            .laid
            .expandable
            .iter()
            .find(|(start, end, _)| *end >= top && *start < bottom)
        else {
            return;
        };
        if let Some(entry) = self.transcript.get_mut(index) {
            entry.expanded = !entry.expanded;
        }
        self.laid = relaid(self);
        self.scroller_to_row(start.min(top));
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
