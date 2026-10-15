//! vi's modal editing of the input box: its motions, operators, text objects, registers, the
//! visual selection, `.` and undo, and REPLACE mode.

use super::{Session, Spelled, UNDO_DEPTH, Yanked, along};

/// What kind of character one is, for working out where a word begins and ends.
///
/// Blanks, words and punctuation rather than blanks and the rest, because vi's `w` treats punctuation
/// as a word of its own: in `src/main.rs` the slashes are not part of either name, which is what makes
/// `diw` on one of them take the slash alone. `W` uses the same machinery with punctuation folded into
/// `Word`, and that is the whole of the difference between the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Blank,
    Word,
    Punctuation,
    /// A marker, to the word motions of `w`, `e`, `b` and `ge`. It stands for one picture or paste,
    /// so it is a word by itself and joins no run, not even that of a marker beside it.
    Marker,
}

/// The kind of character `c` is to `w` and `iw`.
fn word_class(c: char) -> Class {
    if c.is_whitespace() {
        Class::Blank
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Punctuation
    }
}

/// The kind of character `c` is to `W` and `iW`, where anything but a blank is one thing: that is
/// what makes a path or a flag a single word.
fn bigword_class(c: char) -> Class {
    if c.is_whitespace() {
        Class::Blank
    } else {
        Class::Word
    }
}

/// A selection that has ended, as the row and the column each of its ends was at, counting from
/// zero and a column in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Marked {
    anchor: (usize, usize),
    caret: (usize, usize),
    lines: bool,
}

/// One character typed in REPLACE mode, and the one it took the place of.
///
/// `None` where it took the place of nothing: at the end of a row, over a marker, and for a newline,
/// each of which it went in beside rather than over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TypedOver {
    at: usize,
    typed: char,
    was: Option<char>,
}

/// A change `.` can make again, with what was typed after it where it opened INSERT mode.
///
/// What was typed is part of the instruction, as it is in vi: `cw` then `X` is "change the word to
/// X", and repeating only the `cw` would leave the next word gone and nothing in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Change {
    /// An operator over an extent. `typed` is empty for every operator but `c`.
    Operated {
        operator: crate::vim::Operator,
        extent: crate::vim::Extent,
        count: Option<u32>,
        typed: String,
    },
    /// One of the keys that open INSERT mode, which take no count.
    Opened {
        opening: crate::vim::Opening,
        typed: String,
    },
    /// `p` or `P`.
    Put { before: bool, count: Option<u32> },
    /// `J` or `gJ`.
    Joined { spaced: bool, count: Option<u32> },
}

/// An INSERT session a change or an opening began, until Escape ends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Inserting {
    /// The line and the caret before the change or the opening, which become the step to undo if
    /// the line comes out of the session changed.
    before: (String, usize),
    /// The line and the caret the session began from, which what is on the line at Escape is read
    /// against to find what was typed.
    opened_on: (String, usize),
    /// What `.` will make again, or `None` where the session did something it cannot.
    pub(super) change: Option<Change>,
}

impl Session {
    /// Whether a character typed now goes over the line rather than into it, which is REPLACE mode.
    pub(super) fn typing_over(&self) -> bool {
        self.vi_mode() == Some(crate::vim::Mode::Replace)
    }

    /// Put a character typed in REPLACE mode where the one under the caret was.
    ///
    /// In beside it instead where there is no one character to take the place of: at the end of a
    /// row, and on a marker, which stands for an attachment and would name nothing with its first
    /// bracket typed over. A newline goes in beside as well, as it does in vi, so a new line breaks
    /// the row rather than eating into it.
    ///
    /// The first character after `R` is where the change begins, so everything typed until Escape
    /// is one change and `u` puts the line back as it stood before it. Not the `R` itself: one left
    /// with nothing typed has changed nothing, and a step taken there would be a press of `u` that
    /// puts back nothing. `.` repeats none of it, since what was typed over is not kept.
    pub(super) fn type_over(&mut self, c: char) {
        if std::mem::take(&mut self.typing_over_begins_a_change) {
            self.begin_a_change();
        }
        let was = self.input[self.caret..]
            .chars()
            .next()
            .filter(|&was| was != '\n' && c != '\n' && self.marker_at_caret().is_none());
        self.history.leave();
        self.input.replace_range(
            self.caret..self.caret + was.map_or(0, char::len_utf8),
            c.encode_utf8(&mut [0; 4]),
        );
        self.typed_over.push(TypedOver {
            at: self.caret,
            typed: c,
            was,
        });
        self.caret += c.len_utf8();
        self.completion = 0;
    }

    /// Take back the last character typed in REPLACE mode and put back the one it took the place
    /// of, which is what Backspace does there.
    ///
    /// Only what was typed since `R`, and only while the caret is at the end of it. What is behind
    /// the caret otherwise is the line the person came to, which Backspace steps over without
    /// taking, as it does in vi: the mode is for typing over a line, and a key that ate into it
    /// would be INSERT's Backspace under another name.
    pub(super) fn take_back_what_was_typed_over(&mut self) {
        let last = self.typed_over.last().copied();
        match last {
            Some(TypedOver { at, typed, was })
                if self.input.get(at..self.caret) == Some(typed.encode_utf8(&mut [0; 4])) =>
            {
                self.typed_over.pop();
                self.history.leave();
                let mut spelled = [0; 4];
                let put_back = match was {
                    Some(was) => &*was.encode_utf8(&mut spelled),
                    None => "",
                };
                self.input.replace_range(at..self.caret, put_back);
                self.caret = at;
                self.completion = 0;
            }
            _ => {
                self.typed_over.clear();
                self.move_left();
            }
        }
    }

    /// Backspace in REPLACE mode until the caret is back at `start`, which is what Ctrl-W and Ctrl-U
    /// do there, as they do in vim: a word or a row of typing taken back at once, with the line from
    /// before the `R` stepped over rather than deleted.
    pub(super) fn take_back_as_far_as(&mut self, start: usize) {
        while self.caret > start {
            self.take_back_what_was_typed_over();
        }
    }

    /// Which vi mode the box is in, or `None` where vi is not the style.
    ///
    /// `None` rather than INSERT for the ordinary box, so nothing can draw a mode at somebody who
    /// never asked for one.
    pub fn vi_mode(&self) -> Option<crate::vim::Mode> {
        match self.editing {
            crate::vim::Editing::Vi => Some(self.mode),
            crate::vim::Editing::Ordinary => None,
        }
    }

    /// Whether a letter typed now is an instruction rather than a letter.
    ///
    /// True in VISUAL mode as well as NORMAL: the difference between the two is what an instruction acts
    /// on, not whether a letter is one.
    pub fn vi_normal(&self) -> bool {
        self.vi_mode()
            .is_some_and(crate::vim::Mode::takes_instructions)
    }

    /// The keys typed so far of a vi instruction still waiting for more, or `None` where none is.
    ///
    /// A count on its own counts as waiting: `2` has decided what the next letter does as much as a
    /// `d` has, so it is drawn for the same reason and `2d3` is what the hint line says after three
    /// presses.
    pub fn half_typed(&self) -> Option<&str> {
        (self.half_typed.is_some() || self.count.is_some() || self.held_count.is_some())
            .then_some(self.typed_so_far.as_str())
    }

    /// Drops a vi instruction still waiting for its next key, for a press that cannot be that key.
    pub fn abandon_half_typed(&mut self) {
        self.half_typed = None;
        self.count = None;
        self.held_count = None;
    }

    /// The stretch VISUAL mode has marked out, as byte offsets, or `None` where it is not open.
    ///
    /// Both ends inclusive of the characters they sit on, which is what vi shows and what makes `v` then
    /// `d` take two characters rather than one. Whole lines in the line-wise mode however far along a
    /// line either end happens to sit.
    pub fn vi_selection(&self) -> Option<(usize, usize)> {
        self.marked_out(matches!(
            self.vi_mode(),
            Some(crate::vim::Mode::Visual { lines: true })
        ))
    }

    /// The stretch between the anchor and the caret, as the whole rows it crosses where `rows` says
    /// so, or `None` where VISUAL mode is not open.
    fn marked_out(&self, rows: bool) -> Option<(usize, usize)> {
        // The mode as well as the anchor: the anchor is what the stretch is, and the mode is whether
        // there is one at all. Read from the anchor alone, a box that had left VISUAL mode without
        // dropping it would draw a stretch its keys can no longer act on.
        let Some(crate::vim::Mode::Visual { .. }) = self.vi_mode() else {
            return None;
        };
        // Clamped to the line as it stands rather than trusted to be within it, the way `wrap`
        // clamps the caret it is handed. Nothing reachable leaves a stale anchor behind — every edit
        // of the line abandons the selection — but this is read on every frame, and an offset past
        // the end here is not a stretch drawn wrong: it is the line sliced outside its bounds, which
        // panics out of the draw with the terminal still in raw mode.
        let anchor = crate::wrap::boundary_at_or_before(&self.input, self.anchor?);
        let (from, to) = (anchor.min(self.caret), anchor.max(self.caret));
        if rows {
            let starts = self.input[..from].rfind('\n').map_or(0, |at| at + 1);
            let ends = self.input[to..]
                .find('\n')
                .map_or(self.input.len(), |at| to + at);
            return Some((starts, ends));
        }
        // The character the far end sits on is part of the selection, so the stretch runs past it.
        Some((from, self.past(to)))
    }

    /// The position just past the character at `at`, or `at` itself at the end of the input.
    ///
    /// Past the whole of a marker where `at` is on one, since that is the one character it stands
    /// for: an end half way through one would draw a stretch over a picture's opening bracket and
    /// leave the rest of it outside, which is not a stretch anybody marked out.
    fn past(&self, at: usize) -> usize {
        match self.marker_at(at) {
            Some((_, end)) => end,
            None => match self.input[at..].chars().next() {
                Some(c) => at + c.len_utf8(),
                None => at,
            },
        }
    }

    /// Take the letters as instructions, which is what Escape asks for.
    ///
    /// `false` where vi is not the style and nothing happened. Callers read the style themselves,
    /// since a key press is chosen between several meanings and a call that mutates while that choice
    /// is being made is one whose order of evaluation is load-bearing. The refusal is here as well
    /// because it is the property worth holding whatever a caller does: the ordinary box has no
    /// NORMAL mode to be put into, so nothing reachable can leave it in one.
    pub fn enter_vi_normal(&mut self) -> bool {
        if self.editing != crate::vim::Editing::Vi {
            return false;
        }
        // Before the caret steps back, since where it stands is where the typing ended.
        self.finish_inserting();
        // The selection goes with the mode that showed it. Escape out of VISUAL mode abandons the
        // stretch, so what the next operator acts on is what the caret is on and nothing invisible.
        self.let_go_of_the_selection();
        self.mode = crate::vim::Mode::Normal;
        // Escape takes the list of keys down in every editing style. The ordinary box clears the
        // line on it, which takes the list down through the line; here the key is the mode's, and
        // `?` in NORMAL mode is vi's own key, so nothing else could close it from there.
        self.shortcuts = false;
        // Where vi leaves it. The caret in NORMAL mode sits on a character rather than between two,
        // so the position one past the end of the line is not one it can hold, and Escape at the end
        // of a line somebody has just typed lands on the last character they typed.
        self.step_back_off_the_end();
        true
    }

    /// Carry out the instruction a letter is in NORMAL mode.
    ///
    /// A letter vi does not use does nothing at all, which is the mode's whole bargain: the box is
    /// not typing, so an instruction it does not recognise is not text to fall back on.
    pub(super) fn obey(&mut self, c: char) {
        if self.half_typed.is_none() && self.count.is_none() && self.held_count.is_none() {
            self.typed_so_far.clear();
        }
        self.typed_so_far.push(c);
        // The digits in front of an instruction, which say how many times over it is meant. Read
        // before the wait below, since an operator waiting for its stretch is one of the two places
        // a digit is a count rather than the key being waited for.
        if crate::vim::takes_a_count(self.half_typed)
            && let Some(count) = crate::vim::counted(self.count, c)
        {
            self.count = Some(count);
            return;
        }
        // A key that was waiting for one more takes this press and nothing else looks at it. Cleared
        // first, so a pair that means nothing ends the wait rather than holding it open: one stray
        // press would otherwise swallow every letter after it until something happened to match.
        if let Some(pending) = self.half_typed.take() {
            self.carry_out(pending.then(c));
            return;
        }
        // A counted row key, which is the input's own rows rather than the ladder the bare key walks
        // (see [`Session::vi_spells`]). Answered here because the count is what tells the two apart,
        // and the bare key never reaches this far.
        if let ('j' | 'k', Some(_)) = (c, self.count) {
            let count = self.take_the_count();
            self.move_rows(c == 'j', count);
            return;
        }
        // The keys that repeat a jump, which mean nothing on their own: they say "that again", and
        // what "that" was is the only thing the session remembers about a motion.
        if let (';' | ',', Some(find)) = (c, self.last_find) {
            let repeated = if c == ';' { find } else { find.reversed() };
            let count = self.take_the_count();
            self.repeatedly(count, |session| session.jump_to_char(repeated));
            return;
        }
        // The two modes disagree about what most of the letters mean, so each reads its own table: `u`
        // lowers the case of a selection where in NORMAL mode it undoes.
        let command = if self.anchor.is_some() {
            crate::vim::visual_command(c)
        } else {
            crate::vim::command(c)
        };
        self.carry_out(command);
    }

    /// Act on an instruction that has everything it needs.
    ///
    /// A selection the instruction ends is kept for `gv` as it stood before the instruction rather
    /// than after, since most of the ones that end it have moved the caret off the end it was at by
    /// then: `vjU` leaves the caret at the start of what it raised, and a selection kept from there
    /// would come back as one character.
    fn carry_out(&mut self, command: crate::vim::Command) {
        let open = self.marked_now();
        self.act_on(command);
        if self.anchor.is_none() && open.is_some() {
            self.last_selection = open;
        }
    }

    /// Act on an instruction, leaving what it does to a selection to [`Session::carry_out`].
    fn act_on(&mut self, command: crate::vim::Command) {
        use crate::vim::Command;

        // An operator that is still waiting keeps the count typed in front of it, since the one in
        // front of its motion has yet to arrive and the two multiply.
        if let Command::Wait(pending) = command {
            self.held_count = crate::vim::multiplied(self.held_count, self.count.take());
            self.half_typed = Some(pending);
            return;
        }
        // Taken before anything acts, and taken whatever the instruction turns out to be: a key that
        // means nothing spends the count in front of it rather than leaving it for the next press.
        let count = self.take_the_count();

        match command {
            Command::Insert(opening) => self.open_insert(opening),
            Command::Move(motion) => self.move_by_counted(motion, count, false),
            Command::Change(operator, extent) => self.change(operator, extent, count),
            Command::Wait(_) => unreachable!("a wait is answered above"),
            // A count is how many changes back, as `3u` is in vim.
            Command::Undo => {
                for _ in 0..count.unwrap_or(1) {
                    if !self.undo_last_change() {
                        break;
                    }
                }
            }
            Command::Again => self.make_the_last_change_again(count),
            Command::Paste { before } => self.put_the_register_back(before, count),
            Command::PutOver { keep } => self.put_over_the_selection(keep, count),
            Command::Join { spaced } => self.join_the_line_below(count, spaced),
            Command::Select { lines } => self.select(lines),
            Command::Reselect => self.mark_out_the_last_selection_again(),
            Command::TypeOver => {
                self.typing_over_begins_a_change = true;
                self.typed_over.clear();
                self.mode = crate::vim::Mode::Replace;
            }
            Command::SwapEnds => self.swap_the_ends_of_the_selection(),
            Command::Replace(c) => self.replace_the_selection_with(c),
            Command::Case(case) => self.change_the_case_of_the_selection(case),
            Command::Nothing => {}
        }
    }

    /// The count the instruction now being carried out was typed with, clearing both halves of it.
    ///
    /// `None` where no digit was typed, which is not the same as one: a count of one would make
    /// `G` the first row rather than the last, and `.` forget the count its change was made with.
    fn take_the_count(&mut self) -> Option<u32> {
        crate::vim::multiplied(self.held_count.take(), self.count.take())
    }

    /// Do something as many times as the count says, stopping at the first go that moves nothing.
    ///
    /// What a count reaches is bounded by the line rather than by the number typed, which is what
    /// makes `999l` cost the length of a line: the caret runs out of line, the step after that moves
    /// nothing, and the rest of the count is not walked out one position at a time.
    fn repeatedly(&mut self, count: Option<u32>, mut once: impl FnMut(&mut Self)) {
        for _ in 0..count.unwrap_or(1) {
            let was = self.caret;
            once(self);
            if self.caret == was {
                return;
            }
        }
    }

    /// Move the caret that many rows within the input, stopping at the first row or the last.
    ///
    /// Never the prompt history, which is where the bare key goes once the input runs out: see
    /// [`Session::vi_spells`] for why a counted one stops instead.
    fn move_rows(&mut self, down: bool, count: Option<u32>) {
        for _ in 0..count.unwrap_or(1) {
            let moved = if down {
                self.move_down_a_line()
            } else {
                self.move_up_a_line()
            };
            if !moved {
                return;
            }
        }
    }

    /// Do something to the stretch of the line an extent names.
    ///
    /// The one place an operator changes the line, so the register, the undo step and the record of
    /// what `.` repeats are kept here for all of them. A copy of that bookkeeping for each operator is
    /// how one of them would come to be missing.
    fn change(
        &mut self,
        operator: crate::vim::Operator,
        extent: crate::vim::Extent,
        count: Option<u32>,
    ) {
        use crate::vim::Operator;

        let (read, read_count) = self.as_vi_reads_it(operator, extent, count);
        let Some((from, to)) = self.stretch(read, read_count) else {
            return;
        };
        // All of the characters asked for or none, which is vim's rule: `5rx` with three left on the
        // line is a count nobody meant, and replacing the three would be a guess at what they did
        // mean. A marker is not characters to overwrite, for the reason it is not in a selection.
        if let Operator::Replace(_) = operator
            && (self.input[from..to].chars().count() < count.unwrap_or(1) as usize
                || self
                    .marker_spans()
                    .any(|(start, end)| start < to && from < end))
        {
            return;
        }

        let whole_lines = self.takes_whole_lines(extent);
        let mut repeated = None;
        let before = (self.input.clone(), self.caret);
        if !operator.reads_only() {
            // `c` takes its step at Escape instead, since one that typed back what it took has
            // changed nothing and a step there would be a press of `u` that puts back nothing.
            if operator == Operator::Change {
                self.begin_an_edit();
            } else {
                self.begin_a_change();
            }
            let (extent, count) = match extent {
                // The selection is gone by the time `.` is pressed, so the rows it crossed are kept
                // as that many rows from the caret, which is what vim repeats.
                crate::vim::Extent::Selection | crate::vim::Extent::SelectedRows if whole_lines => {
                    (
                        crate::vim::Extent::Line,
                        Some(self.input[from..to].matches('\n').count() as u32 + 1),
                    )
                }
                _ => (extent, count),
            };
            let change = Change::Operated {
                operator,
                extent,
                count,
                typed: String::new(),
            };
            // `c` is not the change to make again until what is typed after it is known, which is
            // at Escape.
            if operator == Operator::Change {
                repeated = Some(change);
            } else {
                self.last_change = Some(change);
            }
        }

        if operator.fills_the_register() {
            self.register = Some(Yanked {
                text: self.input[from..to].to_string(),
                lines: whole_lines,
            });
        }

        match operator {
            // Nothing moves, so the caret has no reason to be anywhere but where the yank began, which
            // is where vi leaves it.
            Operator::Yank => self.caret = from,
            Operator::Change => {
                self.input.replace_range(from..to, "");
                self.caret = from;
                self.mode = crate::vim::Mode::Insert;
                self.inserting = Some(Inserting {
                    before,
                    opened_on: (self.input.clone(), self.caret),
                    change: repeated,
                });
            }
            Operator::Delete => {
                // A whole line takes the newline that ends it, so the gap closes rather than leaving a
                // blank line where the line was. The one before it on the last line, or the line above
                // gains a trailing newline it never had. `cc` is the other way and keeps it, which is
                // what leaves the person typing on the line they asked to replace.
                let (from, to) = if whole_lines {
                    match (to < self.input.len(), from > 0) {
                        (true, _) => (from, to + 1),
                        (false, true) => (from - 1, to),
                        (false, false) => (from, to),
                    }
                } else {
                    (from, to)
                };
                self.input.replace_range(from..to, "");
                self.caret = from;
                // The caret has to land on a character, and taking the end of a line leaves it past the
                // last one.
                self.step_back_off_the_end();
            }
            Operator::Indent | Operator::Dedent => {
                self.shift_the_lines(from, to, operator == Operator::Indent)
            }
            Operator::Case(case) => {
                let end = self.change_the_case(from, to, case);
                // `~` moves on past what it changed, so pressing it again walks along the line, and
                // stops on the last character rather than past it. The operator spelled after `g`
                // leaves the caret at the start of its stretch, as a yank does.
                self.caret = if extent == crate::vim::Extent::Character {
                    end
                } else {
                    from
                };
                self.step_back_off_the_end();
            }
            Operator::Replace(with) => {
                let replaced = self.input[from..to].chars().count();
                self.input
                    .replace_range(from..to, &with.to_string().repeat(replaced));
                // On the last character replaced, which is where vi leaves it.
                self.caret = from + (replaced - 1) * with.len_utf8();
            }
        }
        // Every operator ends the selection, the stretch it named having been acted on. A change has
        // already put the box into INSERT mode, so only the others go back to NORMAL.
        if self.anchor.take().is_some() && operator != Operator::Change {
            self.mode = crate::vim::Mode::Normal;
            self.step_back_off_the_end();
        }
    }

    /// Whether an operator over this extent takes whole lines: the doubled letter, the row keys under
    /// an operator, a line-wise selection and the rows of any selection.
    ///
    /// Decided here once, because it settles two things that have to agree: what happens to the
    /// newline and how the register puts the stretch back, and whether a stretch of no characters is
    /// still one to act on. An empty row is a row, so `dd` there takes it, where `dl` at the end of a
    /// line has nothing to take.
    fn takes_whole_lines(&self, extent: crate::vim::Extent) -> bool {
        extent == crate::vim::Extent::Line
            || extent == crate::vim::Extent::SelectedRows
            || matches!(extent, crate::vim::Extent::To(motion) if motion.line_wise())
            || (extent == crate::vim::Extent::Selection
                && matches!(
                    self.vi_mode(),
                    Some(crate::vim::Mode::Visual { lines: true })
                ))
    }

    /// The extent an operator actually acts on, which is not always the one the keys spelled.
    ///
    /// `cw` on a character that is not a blank means `ce`: it changes the word and leaves the space
    /// after it, where `dw` takes that space. This is vi's own special case rather than something
    /// derivable, and it exists because the alternative is useless: somebody replacing a word almost
    /// never wants it run into the next one, and typing the space back every time is what `cw` would
    /// otherwise cost. On a blank there is no word to change, and it means `dw` again.
    ///
    /// On the last character of a word, `ce` would reach the end of the next word, and vim's `cw`
    /// changes that character alone, so the first of the count is the word the caret is already at
    /// the end of. That is every one-character word, which a slash or a dot in a path is.
    ///
    /// Measured against vim itself rather than reasoned about, since a special case is a fact about
    /// what people's hands expect and not something the rest of this can predict.
    fn as_vi_reads_it(
        &self,
        operator: crate::vim::Operator,
        extent: crate::vim::Extent,
        count: Option<u32>,
    ) -> (crate::vim::Extent, Option<u32>) {
        use crate::vim::{Extent, Motion, Operator};

        let (big, end) = match (operator, extent) {
            (Operator::Change, Extent::To(Motion::WordRight)) => (false, Motion::WordEnd),
            (Operator::Change, Extent::To(Motion::BigwordRight)) => (true, Motion::BigwordEnd),
            _ => return (extent, count),
        };
        let Some(here) = self
            .class_at(self.caret, big)
            .filter(|class| *class != Class::Blank)
        else {
            return (extent, count);
        };
        let at_its_end =
            here == Class::Marker || self.class_at(self.past(self.caret), big) != Some(here);
        match (at_its_end, count.unwrap_or(1)) {
            (false, _) => (Extent::To(end), count),
            (true, 1) => (Extent::Character, None),
            (true, words) => (Extent::To(end), Some(words - 1)),
        }
    }

    /// The stretch of the line an extent names, as byte offsets, or `None` where it names nothing.
    ///
    /// Mostly worked out by moving the caret and reading where it lands, then putting it back, which is
    /// what makes those extents obey the marker rules for free: the ends are positions the caret could
    /// rest at, so no stretch can begin or end inside a marker.
    ///
    /// A text object is the exception, since it is found by reading the line rather than by walking, and
    /// a marker is spelled with brackets and a digit: `di[` on one named the brackets it is written with
    /// and left `[]` standing for nothing. So every stretch is checked against the markers before it is
    /// returned, once here rather than in each kind of object.
    fn stretch(
        &mut self,
        extent: crate::vim::Extent,
        count: Option<u32>,
    ) -> Option<(usize, usize)> {
        let whole_lines = self.takes_whole_lines(extent);
        let (from, to) = self.stretch_unchecked(extent, count)?;
        // Widened to whole markers rather than refused, so an object that reached into one takes it with
        // what it was already taking. Refusing would leave `daw` over a marker doing nothing at all,
        // where taking the marker is plainly what was asked for.
        let from = self
            .marker_spans()
            .find(|(start, end)| *start < from && from < *end)
            .map_or(from, |(start, _)| start);
        let to = self
            .marker_spans()
            .find(|(start, end)| *start < to && to < *end)
            .map_or(to, |(_, end)| end);
        Some((from, to)).filter(|(from, to)| from < to || whole_lines)
    }

    /// The stretch an extent names, before the markers are taken into account.
    ///
    /// The count is how many of the extent to take: `3dd` is three lines, `3x` three characters and
    /// `d3w` three words. Those that name no quantity of anything take no count, since there is no
    /// second end of the line to reach, no second object the keys named and no second selection:
    /// `3D`, `d3iw` and a counted operator in VISUAL mode act on what the uncounted one would.
    fn stretch_unchecked(
        &mut self,
        extent: crate::vim::Extent,
        count: Option<u32>,
    ) -> Option<(usize, usize)> {
        use crate::vim::Extent;

        let whole_lines = self.takes_whole_lines(extent);
        let was = self.caret;
        let span = match extent {
            // The line's own characters, and not the newline that ends it. What happens to that
            // newline is the operator's business rather than the extent's: `dd` takes it so the gap
            // closes, `cc` leaves it so the person is typing on the line they asked to replace, and
            // `yy` records the content and puts a newline back when it lands.
            Extent::Line => {
                let (start, mut end) = self.caret_line();
                // The rows below this one, as many as the count asks for and no further than the
                // input goes: `9dd` on a two-row paragraph takes the two rows there are.
                for _ in 1..count.unwrap_or(1) {
                    if end >= self.input.len() {
                        break;
                    }
                    let below = end + 1;
                    end = self.input[below..]
                        .find('\n')
                        .map_or(self.input.len(), |newline| below + newline);
                }
                Some((start, end))
            }
            Extent::ToLineEnd => Some((self.caret, self.caret_line().1)),
            Extent::Character => {
                // Whole where one is a marker, which is what the caret is on rather than the bracket
                // it begins with. Stopping at the end of the line, since the newline is not a
                // character `3x` was asked to take.
                let (_, line_end) = self.caret_line();
                let mut to = self.caret;
                for _ in 0..count.unwrap_or(1) {
                    if to >= line_end {
                        break;
                    }
                    to = self.past(to);
                }
                Some((self.caret, to))
            }
            // Every row from the caret's to the one landed on, and not the newline after the last,
            // which is the operator's business for the reason it is for `dd`. `dG` on the last row
            // still takes that row, where `dj` there is a row that is not there to take.
            Extent::To(motion) if motion.line_wise() => {
                self.move_by_counted(motion, count, true);
                let landed = self.caret;
                if landed == was
                    && matches!(motion, crate::vim::Motion::Down | crate::vim::Motion::Up)
                {
                    None
                } else {
                    self.caret = landed.min(was);
                    let (start, _) = self.caret_line();
                    self.caret = landed.max(was);
                    let (_, end) = self.caret_line();
                    Some((start, end))
                }
            }
            Extent::To(motion) => {
                self.move_by_counted(motion, count, true);
                let landed = self.caret;
                match landed.cmp(&was) {
                    std::cmp::Ordering::Equal => None,
                    // Forwards, and the character landed on is taken or not depending on the motion:
                    // `de` takes the word's last letter and `dw` stops before the next word's first.
                    std::cmp::Ordering::Greater => {
                        let end = if motion.takes_what_it_lands_on() {
                            self.after_the_caret()
                        } else {
                            landed
                        };
                        Some((was, end))
                    }
                    // Backwards, where the character the caret started on is the one not asked for:
                    // `db` takes back to the start of the word and leaves what the caret was on.
                    // Except for the motions that take it, where `dge` takes that character too.
                    // Not the newline of an empty row, which is no character the caret was on.
                    std::cmp::Ordering::Less => {
                        let end = if motion.takes_what_it_lands_on()
                            && !self.input[was..].starts_with('\n')
                        {
                            self.past(was)
                        } else {
                            was
                        };
                        Some((landed, end))
                    }
                }
            }
            Extent::Object(object) => self.object_span(object),
            Extent::Selection => self.vi_selection(),
            Extent::SelectedRows => self.marked_out(true),
        };
        self.caret = was;
        // A stretch of no characters names nothing, unless it is rows: an empty row is still one for
        // `dd` to take and `cc` to type on.
        span.filter(|(from, to)| from < to || whole_lines)
    }

    /// The stretch a text object names, on the line the caret is on.
    ///
    /// The line rather than the whole input, for the reason a jump to a character stays on its own
    /// line: these keys are for the thing in front of you, and one that reached across a newline would
    /// take part of a paragraph nobody was looking at.
    fn object_span(&self, object: crate::vim::Object) -> Option<(usize, usize)> {
        use crate::vim::Kind;

        match object.kind {
            Kind::Word => self.run_span(object.around, false),
            Kind::Bigword => self.run_span(object.around, true),
            Kind::Pair(opens, closes) => self.pair_span(opens, closes, object.around),
        }
    }

    /// The run of same-kind characters the caret is in, and the blanks after it where asked for.
    ///
    /// A run of blanks is itself a run, which is what makes `diw` on a space take the spaces: the caret
    /// is in something, and the object is whatever it is in.
    ///
    /// Read with the word motions' classes, markers included, so an object and a motion never
    /// disagree about where a word is: a marker is a word by itself to `iw` as it is to `w`.
    fn run_span(&self, around: bool, big: bool) -> Option<(usize, usize)> {
        let (line_start, line_end) = self.caret_line();
        let at = self.caret.min(line_end);
        if at == line_end {
            return None;
        }
        let class = |at: usize| self.class_at(at, big);
        let here = class(at)?;
        let forward = |from: usize| {
            self.input[from..line_end]
                .char_indices()
                .map(move |(index, c)| (from + index, c))
        };
        let backward = |to: usize| {
            self.input[line_start..to]
                .char_indices()
                .rev()
                .map(|(index, c)| (line_start + index, c))
        };

        let (from, mut to) = match self.marker_at(at) {
            Some(marker) if here == Class::Marker => marker,
            _ => (
                backward(at)
                    .take_while(|(index, _)| class(*index) == Some(here))
                    .last()
                    .map_or(at, |(index, _)| index),
                forward(at)
                    .take_while(|(index, _)| class(*index) == Some(here))
                    .last()
                    .map_or(at, |(index, c)| index + c.len_utf8()),
            ),
        };

        // `aw` takes the blanks after the word as well, which is what makes it the whole word rather
        // than the word alone: deleting one and leaving two spaces behind is not what was asked for.
        if around {
            let after = forward(to)
                .take_while(|(index, _)| {
                    class(*index) == Some(Class::Blank) && here != Class::Blank
                })
                .last()
                .map_or(to, |(index, c)| index + c.len_utf8());
            // Nothing after it, so the blanks before it are what `aw` takes instead: on the last word
            // of a line, taking nothing extra would make `daw` the same as `diw`.
            if after == to && here != Class::Blank {
                let before = backward(from)
                    .take_while(|(index, _)| class(*index) == Some(Class::Blank))
                    .last()
                    .map_or(from, |(index, _)| index);
                return Some((before, to));
            }
            to = after;
        }
        Some((from, to))
    }

    /// The stretch a pair of delimiters names, with or without the delimiters themselves.
    ///
    /// The pair the caret is inside, or else the next one along the line. That second half is what makes
    /// `ci(` work with the caret on the name in front of the bracket, which is where it usually is.
    fn pair_span(&self, opens: char, closes: char, around: bool) -> Option<(usize, usize)> {
        let (line_start, line_end) = self.caret_line();
        let line = &self.input[line_start..line_end];
        let at = self.caret.min(line_end) - line_start;

        let (from, to) = if opens == closes {
            // A quote is its own closing mark, so there is no nesting to count and the pairs are the
            // marks taken two at a time from the start of the line.
            let marks: Vec<usize> = line
                .char_indices()
                .filter(|(_, c)| *c == opens)
                .map(|(index, _)| index)
                .collect();
            marks
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| (pair[0], pair[1]))
                .find(|(open, close)| at <= *close || at <= *open)?
        } else {
            // Counted rather than matched by position, so a bracket inside a bracket is the pair that
            // encloses the caret rather than whichever one came first.
            let enclosing = self.enclosing_pair(line, at, opens, closes);
            match enclosing {
                Some(pair) => pair,
                None => self.next_pair(line, at, opens, closes)?,
            }
        };

        if !around {
            return Some((line_start + from + opens.len_utf8(), line_start + to));
        }

        // `a"` takes the blanks in front of the quotes and `a(` does not, which is vim's own
        // inconsistency and measurably what it does. A quote has no shape of its own on the line, so
        // what it delimits reads as a word and the blank beside it belongs to it; a bracket usually
        // follows the name it belongs to, and taking the space would take part of that name's spacing.
        let ends = to + closes.len_utf8();
        let from = if opens == closes {
            line[..from]
                .char_indices()
                .rev()
                .take_while(|(_, c)| *c == ' ')
                .last()
                .map_or(from, |(index, _)| index)
        } else {
            from
        };
        Some((line_start + from, line_start + ends))
    }

    /// The innermost pair of delimiters the caret lies within, counting nesting.
    fn enclosing_pair(
        &self,
        line: &str,
        at: usize,
        opens: char,
        closes: char,
    ) -> Option<(usize, usize)> {
        let mut open_at = Vec::new();
        for (index, c) in line.char_indices() {
            if c == opens {
                open_at.push(index);
            } else if c == closes
                && let Some(from) = open_at.pop()
                && from <= at
                && at <= index
            {
                return Some((from, index));
            }
        }
        None
    }

    /// The first complete pair of delimiters beginning at or after the caret.
    fn next_pair(
        &self,
        line: &str,
        at: usize,
        opens: char,
        closes: char,
    ) -> Option<(usize, usize)> {
        let from = line[at..]
            .char_indices()
            .find(|(_, c)| *c == opens)
            .map(|(index, _)| at + index)?;
        let mut depth = 0usize;
        for (index, c) in line[from..].char_indices() {
            if c == opens {
                depth += 1;
            } else if c == closes {
                depth -= 1;
                if depth == 0 {
                    return Some((from, from + index));
                }
            }
        }
        None
    }

    /// Move every line a stretch reaches towards or away from the margin.
    ///
    /// Every line rather than the caret's, because this is the one operator whose unit is the line
    /// however the stretch was named: `3>>` is three rows each moved one step, not one row moved
    /// three. A stretch inside a single line is that line, which is what leaves `>>` where it was.
    ///
    /// A fixed step of spaces rather than a tab, because the box draws what it holds and a tab's width
    /// is the terminal's opinion: a line indented with one would sit somewhere different here than in
    /// the file it was copied from.
    fn shift_the_lines(&mut self, from: usize, to: usize, further: bool) {
        /// How far one press moves a line.
        const STEP: usize = 2;

        let first = self.input[..from].rfind('\n').map_or(0, |at| at + 1);
        let mut starts = vec![first];
        starts.extend(
            self.input[first..to]
                .match_indices('\n')
                .map(|(at, _)| first + at + 1),
        );
        // Furthest along first, since shifting a line moves every offset after it and the lines
        // still to do are the ones in front.
        //
        // The caret follows every line shifted at or before it, and not only its own: the caret is
        // a byte offset into the whole input, so spaces put into the row above move it as surely as
        // spaces put into its own. Left where it was, it would end up past the end of an input that
        // had just been dedented, and the next frame reads the line outside its bounds and panics
        // with the terminal still in raw mode.
        for start in starts.into_iter().rev() {
            if further {
                // An empty row stays empty, as it does in vi: spaces there are nothing anybody can
                // see, and they would go out with the prompt.
                if self.input[start..].starts_with('\n') || start == self.input.len() {
                    continue;
                }
                self.input.insert_str(start, &" ".repeat(STEP));
                if start <= self.caret {
                    self.caret += STEP;
                }
                continue;
            }
            let blanks = self.input[start..]
                .chars()
                .take(STEP)
                .take_while(|c| *c == ' ')
                .count();
            self.input.replace_range(start..start + blanks, "");
            if start <= self.caret {
                // Never back past the start of the line shifted, which is where a caret sitting in
                // the blanks that went ends up. A caret on a later line is further along than that
                // already, so the clamp only ever answers for its own.
                self.caret = self.caret.saturating_sub(blanks).max(start);
            }
        }
    }

    /// Put the register back into the line, beside the caret or as a line of its own.
    ///
    /// Nothing where nothing has been yanked, rather than a guess at what to insert.
    fn put_the_register_back(&mut self, before: bool, count: Option<u32>) {
        let Some(yanked) = self.register.clone() else {
            return;
        };
        // A count is how many copies, and they go in together: one insertion is one change and one
        // step to undo, where putting it back N times over would take N presses of `u` to take back.
        let copies = count.unwrap_or(1) as usize;
        self.begin_a_change();
        self.last_change = Some(Change::Put { before, count });
        self.abandon_the_selection();

        if yanked.lines {
            // A yanked line goes back as a line rather than into the middle of the one the caret is
            // on, which is the whole reason the register remembers which it was.
            // The register holds the line's characters without the newline that ended it, so the
            // newline goes on whichever side puts the text on a line of its own.
            let (start, end) = self.caret_line();
            let (at, text) = if before {
                (start, format!("{}\n", yanked.text).repeat(copies))
            } else {
                (end, format!("\n{}", yanked.text).repeat(copies))
            };
            self.input.insert_str(at, &text);
            self.caret = if before { start } else { at + 1 };
            return;
        }

        // `p` puts it after the character the caret is on, which is where vi puts it: the caret is on a
        // character rather than between two, so there is no position that means "here" for both keys.
        let at = if before {
            self.caret
        } else {
            self.after_the_caret()
        };
        let text = yanked.text.repeat(copies);
        self.input.insert_str(at, &text);
        self.caret = at + text.len();
        self.step_back_off_the_end();
    }

    /// Put the register where the selection is, which is what `p` and `P` ask for there.
    ///
    /// `p` leaves what the selection held in the register, so two stretches trade places in two
    /// presses, and `P` leaves the register as it was, so one yank can go over several. Rows stay
    /// rows: over rows they replace them, and over a stretch within a row they split it, so neither
    /// half of the line runs into them. Measured against vim, caret included.
    ///
    /// Nothing where nothing has been yanked, and the selection stays. vim takes the stretch out and
    /// then finds nothing to put, which is a delete nobody asked for.
    fn put_over_the_selection(&mut self, keep: bool, count: Option<u32>) {
        let Some(yanked) = self.register.clone() else {
            return;
        };
        // A selection on the empty last row holds no character, and the register still goes there,
        // as it does in vim. The register keeps what it held, nothing having been taken out.
        let (from, to) = self
            .stretch(crate::vim::Extent::Selection, None)
            .unwrap_or((self.caret, self.caret));
        let rows = self.takes_whole_lines(crate::vim::Extent::Selection);
        self.begin_a_change();
        if !keep && (from < to || rows) {
            self.register = Some(Yanked {
                text: self.input[from..to].to_string(),
                lines: rows,
            });
        }

        // How many copies, as beside the caret, and in one insertion for the same reason. Where
        // either side is rows, every copy is a row of its own.
        let copies = vec![yanked.text.as_str(); count.unwrap_or(1) as usize];
        let text = match (rows, yanked.lines) {
            (true, _) => copies.join("\n"),
            (false, true) => format!("\n{}\n", copies.join("\n")),
            (false, false) => copies.concat(),
        };
        self.input.replace_range(from..to, &text);
        if rows || yanked.lines {
            self.caret = if rows { from } else { from + 1 };
            self.move_to_first_non_blank();
        } else if yanked.text.contains('\n') {
            // Characters that run over rows leave the caret where they begin, as vim does, since
            // their last one is on another row from the place they were put.
            self.caret = from;
        } else {
            self.caret = from + text.len();
            self.move_left();
        }
        self.leave_visual_mode();
    }

    /// The position just past the character the caret is on, which is where `p` inserts.
    fn after_the_caret(&self) -> usize {
        if let Some((_, end)) = self.marker_at_caret() {
            return end;
        }
        match self.input[self.caret..].chars().next() {
            Some(c) => self.caret + c.len_utf8(),
            None => self.caret,
        }
    }

    /// Make this line and the one below into one, which is what `J` and `gJ` ask for.
    ///
    /// Spaced, the newline becomes a single space, which is what vi does: two sentences run together
    /// with no gap is not what somebody joining lines wants, and the blanks the next line was indented
    /// with are part of the shape it no longer has. Unspaced is `gJ`, for the line that was broken in
    /// the middle of a word or a path, where any space would be one the text never had.
    fn join_the_line_below(&mut self, count: Option<u32>, spaced: bool) {
        let (_, end) = self.caret_line();
        if end >= self.input.len() {
            return;
        }
        // Pressed over a selection it is not made again, as nothing else pressed over one is but an
        // operator.
        let selected = self.anchor.is_some();
        self.begin_a_change();
        if !selected {
            self.last_change = Some(Change::Joined { spaced, count });
        }
        self.abandon_the_selection();

        // A count is how many rows end up as one, so it is one join fewer than the number typed and
        // `2J` is the bare key: joining two rows is what one press does. One snapshot for the lot,
        // since the rows went together in one press and come back in one.
        for _ in 1..count.unwrap_or(2).max(2) {
            let (_, end) = self.caret_line();
            if end >= self.input.len() {
                break;
            }
            let below = end + 1;
            if spaced {
                let text = self.input[below..].to_string();
                let blanks = text.len() - text.trim_start_matches([' ', '\t']).len();
                self.input.replace_range(end..below + blanks, " ");
            } else {
                self.input.replace_range(end..below, "");
            }
            self.caret = end;
        }
        // Where the row joined on was empty, `gJ` leaves the caret where the newline was, which is
        // the column after the line.
        self.step_back_off_the_end();
    }

    /// Open VISUAL mode, or change which kind it is, or leave it.
    ///
    /// One key both ways, read against the mode already in force: the press that opens the mode is the
    /// press that closes it, so there is nothing to remember about which was which. `v` while `V` is in
    /// force changes the kind rather than leaving, which is what vi does and what somebody who pressed
    /// the wrong one of the two wants.
    fn select(&mut self, lines: bool) {
        match self.vi_mode() {
            Some(crate::vim::Mode::Visual { lines: already }) if already == lines => {
                self.mode = crate::vim::Mode::Normal;
                self.anchor = None;
            }
            Some(crate::vim::Mode::Visual { .. }) => self.mode = crate::vim::Mode::Visual { lines },
            _ => {
                self.mode = crate::vim::Mode::Visual { lines };
                // Both ends at the caret, so a selection just opened covers the character it is on and
                // is never empty: an operator pressed straight away acts on something.
                self.anchor = Some(self.caret);
            }
        }
    }

    /// Put the caret at the other end of the selection, which is what `o` asks for.
    ///
    /// The end being moved is the one the caret is at, so this is how the other end is adjusted without
    /// starting the selection again.
    fn swap_the_ends_of_the_selection(&mut self) {
        if let Some(anchor) = self.anchor {
            self.anchor = Some(self.caret);
            self.caret = anchor;
        }
    }

    /// Replace every character of the selection with one, which is what `r` asks for.
    ///
    /// A marker is not a run of characters to overwrite, so a selection holding one is left alone rather
    /// than turned into a row of `x` where a picture was. Refused whole rather than in part: replacing
    /// the text either side and leaving the marker would be a line nobody could read.
    fn replace_the_selection_with(&mut self, c: char) {
        let Some((from, to)) = self.vi_selection() else {
            return;
        };
        if self
            .marker_spans()
            .any(|(start, end)| start < to && from < end)
        {
            self.leave_visual_mode();
            return;
        }
        self.begin_a_change();

        let replaced: String = self.input[from..to]
            .chars()
            .map(|was| if was == '\n' { was } else { c })
            .collect();
        self.input.replace_range(from..to, &replaced);
        self.caret = from;
        self.leave_visual_mode();
    }

    /// Change the case of the selection, which is what `~`, `u` and `U` ask for there, and `g~`, `gu`
    /// and `gU`.
    fn change_the_case_of_the_selection(&mut self, case: crate::vim::Case) {
        let Some((from, to)) = self.vi_selection() else {
            return;
        };
        self.begin_a_change();

        self.change_the_case(from, to, case);
        self.caret = from;
        self.leave_visual_mode();
    }

    /// Change the case of a stretch of the line, and return where the stretch now ends.
    ///
    /// Around the markers in it rather than through them: a marker is found by its text, so `[IMAGE
    /// #1]` names no picture, and the picture would be left off the prompt while the line still
    /// read as though it carried one. The end moves because a letter can become two, as `ß` does.
    fn change_the_case(&mut self, from: usize, to: usize, case: crate::vim::Case) -> usize {
        let mut markers: Vec<(usize, usize)> = self
            .marker_spans()
            .filter(|(start, end)| from < *end && *start < to)
            .collect();
        markers.sort_unstable();
        let mut changed = String::new();
        let mut at = from;
        for (start, end) in markers {
            let (start, end) = (start.max(from), end.min(to));
            changed.push_str(&case.applied_to(&self.input[at..start]));
            changed.push_str(&self.input[start..end]);
            at = end;
        }
        changed.push_str(&case.applied_to(&self.input[at..to]));
        self.input.replace_range(from..to, &changed);
        from + changed.len()
    }

    /// Back to NORMAL mode with no selection, which is where every operator leaves VISUAL mode.
    ///
    /// The stretch has been acted on, so a selection left standing would be one the next press acted on
    /// again for reasons nothing on the screen explains.
    fn leave_visual_mode(&mut self) {
        if self.anchor.take().is_some() {
            self.mode = crate::vim::Mode::Normal;
            self.step_back_off_the_end();
        }
    }

    /// Abandon the selection, the line it was marked on having been edited out from under it.
    ///
    /// Everything that shortens or replaces the line calls this, because a stretch is a pair of
    /// offsets into the line and nothing records which line they were taken from. Most of the keys
    /// that edit are not vi's own — Backspace, Delete, the readline bindings, a paste, a prompt
    /// recalled — and VISUAL mode claims none of them, so they reach the box with a selection open
    /// and leave it naming characters that have moved or gone. What is left is not a stretch drawn
    /// wrong: [`Session::vi_selection`] is read on every frame, so the next draw reads the line
    /// outside its bounds and the session panics with the terminal still in raw mode.
    ///
    /// The mode goes back to NORMAL with the stretch, since VISUAL mode with nothing marked out is a
    /// mode whose whole subject is missing and the letters would be read from the wrong table.
    /// Nothing else about the press changes: the caret is left where the edit put it, which is where
    /// the same key leaves it in NORMAL mode, rather than stepped off the end of the line the way
    /// `leave_visual_mode` steps it after an operator has acted on the stretch.
    ///
    /// What `R` has typed over goes as well, being offsets into the line for the same reason, and
    /// with it the change `R` was making: what is typed over the line from here is a change of its
    /// own, so `u` after it cannot put back a prompt that has since been sent.
    pub(super) fn abandon_the_selection(&mut self) {
        self.typed_over.clear();
        self.typing_over_begins_a_change = true;
        if self.let_go_of_the_selection() {
            self.mode = crate::vim::Mode::Normal;
        }
    }

    /// Drop the selection, keeping where it was for `gv`, and say whether there was one.
    ///
    /// Before the mode changes, which is what says whether the selection was rows.
    pub(super) fn let_go_of_the_selection(&mut self) -> bool {
        let Some(marked) = self.marked_now() else {
            return false;
        };
        self.anchor = None;
        self.last_selection = Some(marked);
        true
    }

    /// The selection open now, as the rows and columns its ends are at, or `None` where none is.
    fn marked_now(&self) -> Option<Marked> {
        Some(Marked {
            anchor: self.row_and_column(self.anchor?),
            caret: self.row_and_column(self.caret),
            lines: self.mode == crate::vim::Mode::Visual { lines: true },
        })
    }

    /// Mark out the selection that ended last, which is what `gv` asks for.
    ///
    /// The selection open now, if there is one, becomes the one to come back to, so a second `gv`
    /// goes back to it as it does in vi. Nothing where no selection has ended.
    ///
    /// Each end is found again by its row and its column in the line as it now stands, which may
    /// be shorter than the line it was marked on: the last row past the end of the input, and the
    /// last character past the end of a row, which are where a count past either end stops. An end
    /// that now falls inside a marker is the marker.
    fn mark_out_the_last_selection_again(&mut self) {
        let Some(last) = self.last_selection else {
            return;
        };
        if let Some(open) = self.marked_now() {
            self.last_selection = Some(open);
        }
        self.put_the_caret_at(last.anchor);
        let anchor = self.caret;
        self.put_the_caret_at(last.caret);
        self.anchor = Some(anchor);
        self.mode = crate::vim::Mode::Visual { lines: last.lines };
    }

    /// The row and the column a position in the line is at, counting from zero and a column in
    /// characters.
    ///
    /// Clamped to the line for the reason [`Session::marked_out`] clamps the anchor: this is read on
    /// every Escape, and an anchor left past the end would panic with the terminal in raw mode.
    fn row_and_column(&self, at: usize) -> (usize, usize) {
        let before = &self.input[..crate::wrap::boundary_at_or_before(&self.input, at)];
        let start = before.rfind('\n').map_or(0, |newline| newline + 1);
        (
            before.matches('\n').count(),
            before[start..].chars().count(),
        )
    }

    /// Put the caret at a row and a column, or as near as the line now reaches, on a character the
    /// way NORMAL mode's caret is.
    fn put_the_caret_at(&mut self, (row, column): (usize, usize)) {
        let start = self
            .input
            .match_indices('\n')
            .take(row)
            .last()
            .map_or(0, |(newline, _)| newline + 1);
        let end = self.input[start..]
            .find('\n')
            .map_or(self.input.len(), |newline| start + newline);
        self.caret = start + along(&self.input[start..end], column);
        self.settle_onto_a_marker();
        self.step_back_off_the_end();
    }

    /// Keep the line for `u` and forget what `.` repeats, since this edit is now the last change.
    pub(super) fn begin_a_change(&mut self) {
        self.keep_for_undo((self.input.clone(), self.caret));
        self.begin_an_edit();
    }

    /// Forget what `.` repeats, the prompt recalled and the completion offered, since the line is
    /// about to be edited and none of them describe it any more.
    fn begin_an_edit(&mut self) {
        self.last_change = None;
        self.history.leave();
        self.completion = 0;
    }

    /// Keep a line and a caret as the latest step to undo, letting the oldest go past the depth.
    fn keep_for_undo(&mut self, step: (String, usize)) {
        if self.undo.len() == UNDO_DEPTH {
            self.undo.pop_front();
        }
        self.undo.push_back(step);
    }

    /// Put the line back as it stood before the last change, and say whether there was one.
    ///
    /// The ordinary box reaches this too, through its own undo chord: vi's `u` loops it by the
    /// count typed before the key, which the ordinary box has no count in front of a chord for,
    /// so one press is one step back for either style.
    pub fn undo_last_change(&mut self) -> bool {
        let Some((line, caret)) = self.undo.pop_back() else {
            return false;
        };
        self.input = line;
        self.caret = caret;
        self.history.leave();
        self.completion = 0;
        // The step just popped is not an open run any more, whichever kind of step it was, so the
        // next character typed in the ordinary box must open one of its own rather than finding
        // this one still marked open and sharing it, which would leave that typing with no step to
        // undo at all once the stack is otherwise empty.
        self.ordinary_run_open = false;
        // Only where vi's own rule holds that the caret rests on a character rather than past the
        // last one. The ordinary box has no such rule, and the caret a step was taken with is
        // wherever the person had put it, past the last character being nowhere unusual there.
        if self.editing == crate::vim::Editing::Vi {
            self.step_back_off_the_end();
        }
        true
    }

    /// Make the last change again at the caret, typing again what was typed into it.
    ///
    /// A count in front of `.` replaces the one the change was made with, which is vi's rule and the
    /// useful one: `3.` is how a repeat is made bigger than what it repeats. The keys that open
    /// INSERT mode take no count, so there it is spent.
    fn make_the_last_change_again(&mut self, count: Option<u32>) {
        let Some(change) = self.last_change.clone() else {
            return;
        };
        match change {
            Change::Operated {
                operator,
                extent,
                count: recorded,
                typed,
            } => {
                self.change(operator, extent, count.or(recorded));
                self.type_again(&typed);
            }
            Change::Opened { opening, typed } => {
                self.open_insert(opening);
                self.type_again(&typed);
            }
            Change::Put {
                before,
                count: recorded,
            } => self.put_the_register_back(before, count.or(recorded)),
            Change::Joined {
                spaced,
                count: recorded,
            } => self.join_the_line_below(count.or(recorded), spaced),
        }
    }

    /// Type again what a change typed, where making it again has opened INSERT mode, and leave.
    ///
    /// Put in whole rather than a character at a time, since typed one at a time a `?` or a `!` on
    /// an empty line would be the key list or shell mode rather than the character it was.
    fn type_again(&mut self, typed: &str) {
        if self.mode != crate::vim::Mode::Insert {
            return;
        }
        // As a character typed does, so a recalled prompt made again is the person's own line and
        // `j` does not walk away from it.
        self.history.leave();
        self.completion = 0;
        self.input.insert_str(self.caret, typed);
        self.caret += typed.len();
        self.enter_vi_normal();
    }

    /// End the INSERT session a change or an opening began, keeping it for `u` and for `.`.
    ///
    /// What was typed is read off the line rather than recorded key by key: it is whatever stands
    /// between the two halves of the line the session began from, with the caret at its end. A line
    /// that does not read that way was edited somewhere other than where the typing went, by an arrow
    /// and a Delete, and typing the same characters again would not make the same change, so `.` is
    /// left with nothing to make. So is a session whose typing holds a marker or an `@`: made again,
    /// either would attach the same thing a second time, which nobody asked for.
    fn finish_inserting(&mut self) {
        let Some(Inserting {
            before,
            opened_on: (line, at),
            change,
        }) = self.inserting.take()
        else {
            return;
        };
        // A session that changed nothing is no step, as an `R` that typed nothing is not: the step
        // would be a press of `u` that put back nothing.
        let changed = self.input != before.0;
        if changed {
            self.keep_for_undo(before);
        }
        let ends = line.len() - at;
        let typed = self
            .input
            .len()
            .checked_sub(line.len())
            .filter(|_| self.caret + ends == self.input.len())
            .filter(|_| self.input.get(..at) == line.get(..at))
            .filter(|_| self.input.get(self.caret..) == line.get(at..))
            .and_then(|_| self.input.get(at..self.caret))
            .filter(|typed| !typed.contains('@') && !self.markers().any(|m| typed.contains(m)))
            .map(str::to_string);
        self.last_change = match (change, typed) {
            (
                Some(Change::Operated {
                    operator,
                    extent,
                    count,
                    ..
                }),
                Some(typed),
            ) => Some(Change::Operated {
                operator,
                extent,
                count,
                typed,
            }),
            (Some(Change::Opened { opening, .. }), Some(typed)) => {
                Some(Change::Opened { opening, typed })
            }
            _ => None,
        };
    }

    /// The key a press in NORMAL mode stands for, where vi spells an existing binding with a letter.
    ///
    /// `j` and `k` are Down and Up, and `/` is the chord that searches the prompts already sent. Named
    /// here and answered by the key handler rather than acted on from this side, because what those
    /// keys reach is not the line: at the ends of the input they walk the prompt history and then
    /// scroll the transcript, and a letter that moved the caret from here would never reach either.
    ///
    /// Nothing while a key is waiting for one more, where every character is that one: `f/` jumps to
    /// a slash rather than opening a search.
    ///
    /// Nothing for a counted `j` or `k`, which moves rows inside the input and stops at the first or
    /// last, where the key it spells walks the history once the input runs out: `5k` on a two-row
    /// paragraph would replace the whole line with a prompt from three back, with nothing on the
    /// screen to say why. Those two alone, since a count says nothing about opening a search and
    /// `2/` should still open one.
    pub fn vi_spells(&self, c: char) -> Option<Spelled> {
        if !self.vi_normal() || self.half_typed.is_some() {
            return None;
        }
        if self.count.is_some() && matches!(c, 'j' | 'k') {
            return None;
        }
        match c {
            'j' => Some(Spelled::Down),
            'k' => Some(Spelled::Up),
            // `/` searches in vi, and the prompts already sent are the only thing here to search.
            '/' => Some(Spelled::SearchPrompts),

            _ => None,
        }
    }

    /// The same motion, as many times over as the count in front of it asked for.
    ///
    /// The two that reach the ends of the input are the exception, because a count in front of them
    /// means somewhere else entirely rather than more of the same: `3G` and `3gg` are both the third
    /// row, which is what the count is for on a key that already goes as far as it can go.
    /// Told whether it is moving the caret or measuring a stretch, the way [`Session::move_by_for`]
    /// is: the stretch `d2G` names reaches the row `2G` would, so the count has to be read the same
    /// way on both sides or the operator takes the whole input where the motion took two rows.
    ///
    /// `w` measured for an operator is the other exception: the last word of the count ends where
    /// its line does, as vim's does, so `dw` on the last word of a row takes that word and leaves
    /// the newline, where the caret moving on would have crossed it to the next row.
    fn move_by_counted(&mut self, motion: crate::vim::Motion, count: Option<u32>, measuring: bool) {
        use crate::vim::Motion;

        match (motion, count) {
            (Motion::InputStart | Motion::InputEnd, Some(row)) => self.move_to_row(row),
            // The row the count names, counting this one as the first, so `_` alone is this row.
            (Motion::FirstNonBlankBelow, _) => {
                self.move_rows(true, Some(count.unwrap_or(1).saturating_sub(1)));
                self.move_to_first_non_blank();
            }
            (Motion::Column, _) => self.move_to_column(count.unwrap_or(1)),
            // In vi a count makes `%` a different key, the row that many hundredths of the way
            // through the file, and a prompt's handful of rows is what `G` already names. Spent and
            // moving nothing, where a count taken as more of the same would bounce between the two
            // brackets and land on the one it happened to be odd or even for.
            (Motion::MatchingBracket, Some(_)) => {}
            (Motion::WordRight | Motion::BigwordRight, _) if measuring => {
                let big = motion == Motion::BigwordRight;
                let before_the_last = count.unwrap_or(1).saturating_sub(1);
                self.repeatedly(Some(before_the_last), |session| {
                    session.move_word_start_right(big, false);
                });
                self.move_word_start_right(big, true);
            }
            _ => self.repeatedly(count, |session| session.move_by_for(motion, measuring)),
        }
    }

    /// Put the caret on the first character of a row that is not a blank, counting the first row as
    /// one.
    ///
    /// Past the last row is the last row, the way every counted thing here stops where the input
    /// does rather than doing nothing at all.
    fn move_to_row(&mut self, row: u32) {
        self.caret = 0;
        for _ in 1..row {
            if !self.move_down_a_line() {
                break;
            }
        }
        self.move_to_first_non_blank();
    }

    /// Put the caret on a column of its row, counting the first as one, which is what `|` asks for.
    ///
    /// A column is a character, as it is to `gr`, and a column inside a marker is the marker, since
    /// there is no position inside one for the caret to rest at. Past the end of the row is its last
    /// character, as it is in vi.
    fn move_to_column(&mut self, column: u32) {
        self.move_to_line_start();
        let (_, end) = self.caret_line();
        let mut reached = 0;
        while self.caret < end {
            reached += self.input[self.caret..self.past(self.caret)]
                .chars()
                .count();
            if reached >= column as usize {
                break;
            }
            self.move_right();
        }
        self.step_back_off_the_end();
    }

    /// Move to the bracket that pairs with the first one at or after the caret on its row, which is
    /// what `%` asks for, and nowhere where the row holds no such pair.
    ///
    /// Only brackets of the kind found are counted, so a `]` between two parentheses is text. The
    /// row rather than the input, for the reason a jump to a character stays on its own row.
    ///
    /// Read over the caret's own positions, where a marker is one position and no bracket at all. A
    /// marker is spelled with brackets, and searched as characters one would pair with itself and
    /// land the caret inside a picture.
    fn move_to_the_matching_bracket(&mut self) {
        let (start, end) = self.caret_line();
        let mut steps = Vec::new();
        let mut at = start;
        while at < end {
            let bracket = match self.marker_at(at) {
                Some(_) => None,
                None => self.input[at..]
                    .chars()
                    .next()
                    .filter(|c| "()[]{}".contains(*c)),
            };
            steps.push((at, bracket));
            at = self.past(at);
        }
        let Some((first, bracket)) =
            steps
                .iter()
                .enumerate()
                .find_map(|(index, &(at, bracket))| {
                    Some((index, bracket.filter(|_| at >= self.caret)?))
                })
        else {
            return;
        };
        let (partner, forwards) = match bracket {
            '(' => (')', true),
            ')' => ('(', false),
            '[' => (']', true),
            ']' => ('[', false),
            '{' => ('}', true),
            _ => ('{', false),
        };
        let mut ahead = steps[first..].iter();
        let mut behind = steps[..=first].iter().rev();
        let walk: &mut dyn Iterator<Item = &(usize, Option<char>)> =
            if forwards { &mut ahead } else { &mut behind };
        let mut depth = 0usize;
        for &(at, c) in walk {
            if c == Some(bracket) {
                depth += 1;
            } else if c == Some(partner) {
                depth -= 1;
                if depth == 0 {
                    self.caret = at;
                    return;
                }
            }
        }
    }

    /// Move the caret where a motion says, told whether it is moving the caret or measuring a
    /// stretch for an operator.
    ///
    /// Every one of these goes through the caret methods the arrows already use, so a marker is
    /// crossed whole and there is no position inside one for a motion to leave the caret at. A motion
    /// that did byte arithmetic on the line would have to know about markers itself, and the one that
    /// forgot would be the one that put the caret in the middle of a picture.
    ///
    /// The two callers differ in one place: the position after the last character of the line. The
    /// caret cannot rest there, but it is where a stretch ending at that character ends, so a motion
    /// clamped for both would leave `dl` on a final character measuring nothing at all.
    fn move_by_for(&mut self, motion: crate::vim::Motion, measuring: bool) {
        use crate::vim::Motion;

        match motion {
            Motion::Left => {
                let (start, _) = self.caret_line();
                if self.caret > start {
                    self.move_left();
                }
            }
            // Stopping on the last character rather than the column after it, which is where the
            // arrows leave the caret in INSERT mode and is not a position NORMAL mode has.
            //
            // Except when a stretch is being measured, where the position after the last character is
            // the end of that stretch rather than somewhere the caret will rest: clamped, `dl` on the
            // final character measures nothing and the key that means `x` would do nothing there.
            Motion::Right => {
                self.move_right();
                if !measuring {
                    self.step_back_off_the_end();
                }
            }
            Motion::WordRight => self.move_word_start_right(false, false),
            Motion::WordEnd => self.move_word_end_right(false),
            Motion::WordLeft => self.move_word_start_left(false),
            Motion::WordEndLeft => self.move_word_end_left(false),
            Motion::BigwordRight => self.move_word_start_right(true, false),
            Motion::BigwordEnd => self.move_word_end_right(true),
            Motion::BigwordLeft => self.move_word_start_left(true),
            Motion::BigwordEndLeft => self.move_word_end_left(true),
            Motion::LineStart => self.move_to_line_start(),
            // The last character rather than the position after it, since that is not one the caret
            // can hold in NORMAL mode.
            Motion::LineEnd => {
                self.move_to_line_end();
                self.step_back_off_the_end();
            }
            Motion::FirstNonBlank | Motion::FirstNonBlankBelow => self.move_to_first_non_blank(),
            Motion::Column => self.move_to_column(1),
            Motion::MatchingBracket => self.move_to_the_matching_bracket(),
            // On the first character that is not a blank, as vi does: the row's indent is not
            // where anything a person would reach for begins.
            Motion::InputStart => {
                self.caret = 0;
                self.move_to_first_non_blank();
            }
            Motion::InputEnd => {
                self.caret = self.input.len();
                self.move_to_first_non_blank();
            }
            Motion::Down => {
                self.move_down_a_line();
            }
            Motion::Up => {
                self.move_up_a_line();
            }
            Motion::ToChar(find) => {
                self.last_find = Some(find);
                self.jump_to_char(find);
            }
            // Both ends of the object, which in VISUAL mode is what selects it: the anchor takes the near
            // end and the caret the far one. On the object's last character rather than past it, since
            // the selection covers the character each end sits on and one past would take a character
            // the object does not include.
            Motion::Object(object) => {
                if let Some((from, to)) = self.object_span(object) {
                    if self.anchor.is_some() {
                        // Onto the marker where the object's near end is inside one, for the reason
                        // the stretch an operator measures is widened to whole markers: `o` puts the
                        // caret on this end, and there is no position inside a marker for it.
                        self.anchor = Some(self.marker_at(from).map_or(from, |(start, _)| start));
                    }
                    // Stepped back from the far end rather than measured off it, for the reason
                    // the word motions step: the last character of an object that reaches a marker
                    // is the whole marker, and the byte before that end is a position inside it.
                    self.caret = to;
                    self.move_left();
                }
            }
        }
    }

    /// Put the caret on a character rather than past the end of the line.
    ///
    /// NORMAL mode's caret sits on the character the next instruction acts on, and the column after
    /// the line holds none. Nothing to do on an empty line, which has no character to sit on either.
    fn step_back_off_the_end(&mut self) {
        let (start, _) = self.caret_line();
        if self.caret > start && self.at_line_end() {
            self.move_left();
        }
    }

    /// Whether the caret is at the end of the line it is on.
    fn at_line_end(&self) -> bool {
        self.caret == self.caret_line().1
    }

    /// Move to the first character of the line that is not a blank, which is what `^` asks for.
    fn move_to_first_non_blank(&mut self) {
        self.move_to_line_start();
        while !self.at_line_end()
            && self.input[self.caret..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace())
        {
            self.move_right();
        }
        self.step_back_off_the_end();
    }

    /// The kind of thing at `at` to the word motions, or nothing at the end of the input.
    ///
    /// `big` asks for the capital motions' answer, where a marker is part of the run of non-blanks
    /// beside it as any other character would be. The small motions see a marker as a word by itself.
    fn class_at(&self, at: usize, big: bool) -> Option<Class> {
        if self.marker_at(at).is_some() {
            return Some(if big { Class::Word } else { Class::Marker });
        }
        let c = self.input.get(at..)?.chars().next()?;
        Some(if big { bigword_class(c) } else { word_class(c) })
    }

    /// Whether the character at the caret carries on a run of `run`.
    ///
    /// A marker carries on nothing, not even a marker beside it: each is one picture or paste.
    fn run_continues(&self, run: Class, big: bool) -> bool {
        run != Class::Marker && self.class_at(self.caret, big) == Some(run)
    }

    /// Move to the start of the next word, which is what `w` and `W` ask for.
    ///
    /// Different from the word motion the arrows use under Ctrl: that one lands after the word it
    /// crossed, and this one lands on the first character of the next. Both are wanted, and vi's is
    /// the one an instruction typed as `w` has to mean.
    ///
    /// Every step is a caret step, so a marker is crossed whole and never landed inside. An empty
    /// row is a word to `w`, `b` and `ge`, as it is in vim, so none of them crosses a paragraph
    /// break unnoticed and `db` at the start of a paragraph leaves the one above alone.
    ///
    /// `ends_the_operand` is the last `w` of an operator's count, which stops at the end of the
    /// line it started on rather than crossing it: the stretch ends there, and the newline stays.
    /// From an empty row it crosses the one newline, which is the row `dw` there takes.
    fn move_word_start_right(&mut self, big: bool, ends_the_operand: bool) {
        let (_, line_end) = self.caret_line();
        let was = self.caret;
        // Out of the run the caret is in, then over the blanks after it. A caret already on a blank
        // skips the first part and lands on the next word, which is the same answer.
        let Some(here) = self.class_at(self.caret, big) else {
            return;
        };
        self.move_right();
        if here != Class::Blank {
            while self.run_continues(here, big) {
                self.move_right();
            }
        }
        while self.class_at(self.caret, big) == Some(Class::Blank) && !self.on_an_empty_row() {
            self.move_right();
        }
        if ends_the_operand {
            self.caret = self.caret.min(if was < line_end {
                line_end
            } else {
                self.past(line_end)
            });
        } else if self.at_input_end() {
            // At the end of the input there is no next word, so the caret stays on the last
            // character rather than coming to rest past it.
            self.step_back_off_the_end();
        }
    }

    /// Whether the caret is on a row with nothing on it.
    fn on_an_empty_row(&self) -> bool {
        let (start, end) = self.caret_line();
        start == end
    }

    /// Move to the end of this word, or of the next one where the caret is already at an end.
    ///
    /// Which is what `e` and `E` ask for, and why it is not `w` stepped back: on the last character
    /// of a word it has to reach the end of the following one.
    fn move_word_end_right(&mut self, big: bool) {
        let was = self.caret;
        let Some(started) = self.class_at(self.caret, big) else {
            return;
        };
        self.move_right();
        if started == Class::Blank || !self.run_continues(started, big) {
            // At an end already, so the end wanted is that of the next word.
            while self.class_at(self.caret, big) == Some(Class::Blank) {
                self.move_right();
            }
            if let Some(word) = self.class_at(self.caret, big) {
                self.move_right();
                while self.run_continues(word, big) {
                    self.move_right();
                }
            }
        } else {
            while self.run_continues(started, big) {
                self.move_right();
            }
        }
        // One step past the end found, so back one. Stepped rather than subtracted: where the word
        // ends in a marker that byte offset is a position inside it, and `move_left` is the step that
        // knows a marker is one character.
        let reached_the_end = self.at_input_end();
        self.move_left();
        if reached_the_end {
            // The final character of the input is a newline where the input ends with one, and that
            // is the column after the line above it rather than a character to land on.
            self.step_back_off_the_end();
            // Never behind where the motion started: a key that reaches forwards and finds nothing
            // leaves the caret alone rather than walking it back a word.
            self.caret = self.caret.max(was);
        }
    }

    /// Move to the start of this word, or of the one before where the caret is already at a start,
    /// which is what `b` and `B` ask for.
    fn move_word_start_left(&mut self, big: bool) {
        if self.caret == 0 {
            return;
        }
        self.move_left();
        while self.class_at(self.caret, big) == Some(Class::Blank) {
            if self.caret == 0 || self.on_an_empty_row() {
                return;
            }
            self.move_left();
        }
        let Some(word) = self.class_at(self.caret, big) else {
            return;
        };
        // Back to the first character of the run, which is where the step behind it leaves it.
        while self.caret > 0 {
            let here = self.caret;
            self.move_left();
            if !self.run_continues(word, big) {
                self.caret = here;
                return;
            }
        }
    }

    /// Move to the end of the word before, which is what `ge` and `gE` ask for.
    ///
    /// Out of the word the caret is in, then back over the blanks before it, stopping on an empty
    /// row. In the first word there is no word before, and the caret goes to the start of the input
    /// as vim's does.
    fn move_word_end_left(&mut self, big: bool) {
        let started = self.class_at(self.caret, big).unwrap_or(Class::Blank);
        if self.caret == 0 {
            return;
        }
        self.move_left();
        if started != Class::Blank {
            while self.run_continues(started, big) {
                if self.caret == 0 {
                    return;
                }
                self.move_left();
            }
        }
        while self.class_at(self.caret, big) == Some(Class::Blank) {
            if self.caret == 0 || self.on_an_empty_row() {
                return;
            }
            self.move_left();
        }
    }

    /// Whether the caret is at the end of the whole input.
    fn at_input_end(&self) -> bool {
        self.caret >= self.input.len()
    }

    /// Jump to a character on the line the caret is on, if it holds one.
    ///
    /// The line rather than the whole input, which is what vi does: these keys are for reaching a
    /// bracket or a comma in front of you, and one that crossed a newline would land somewhere off
    /// the row being read. A character that is not there leaves the caret alone.
    /// A marker is one thing to the caret, so a target inside one is not somewhere to land: the caret
    /// would sit between two halves of a picture, in a place the person cannot see, and the next
    /// instruction would act there. A marker is spelled with brackets and a digit, so `f]` and `f1`
    /// both name characters that are inside one.
    ///
    /// Walked a position at a time through the caret methods rather than found by searching the bytes,
    /// which is what keeps this true without a second copy of the marker rules living here.
    fn jump_to_char(&mut self, find: crate::vim::Find) {
        let was = self.caret;
        let (start, end) = self.caret_line();
        let step = |session: &mut Self| {
            if find.forwards {
                session.move_right();
            } else {
                session.move_left();
            }
        };
        // The first character of the line is on it, and the last place a backward walk looks.
        let line = start..end;

        // Off the character the caret is on before looking, so `f` twice reaches the second occurrence
        // rather than staying on the first. A step that leaves the caret where it was has reached an
        // end of the input, and the character under it is not an occurrence.
        let mut last = self.caret;
        step(self);
        while self.caret != last && line.contains(&self.caret) {
            if self.input[self.caret..].starts_with(find.target) {
                // `t` and `T` stop one character short of the target, on the side the jump came from.
                if find.short {
                    if find.forwards {
                        self.move_left();
                    } else {
                        self.move_right();
                    }
                }
                return;
            }
            last = self.caret;
            step(self);
        }

        // The character is not on this line, and a jump to nothing leaves the caret where it was
        // rather than at whichever end the walk gave up at.
        self.caret = was;
    }

    /// Take the letters as letters again, with the caret where the key asked for it.
    ///
    /// The session this begins is one change, `o`'s new row with what is typed on it, so one `u`
    /// takes back the lot and `.` makes it again ([`Session::finish_inserting`]).
    fn open_insert(&mut self, opening: crate::vim::Opening) {
        use crate::vim::Opening;

        let before = (self.input.clone(), self.caret);
        self.mode = crate::vim::Mode::Insert;
        match opening {
            Opening::Here => {}
            Opening::LineStart => self.move_to_line_start(),
            // Onto the position after the character the caret is on, which in INSERT mode is where
            // the next thing typed lands. On an empty line there is nowhere to move and `a` is `i`.
            Opening::After => self.move_right(),
            Opening::LineEnd => self.move_to_line_end(),
            Opening::LineBelow => {
                self.move_to_line_end();
                self.type_newline();
            }
            Opening::LineAbove => {
                self.move_to_line_start();
                self.type_newline();
                // The newline went in before the caret, so the caret is now on the line that was
                // there and the empty one is above it. Stepping back puts it on the empty one.
                self.move_left();
            }
        }
        self.inserting = Some(Inserting {
            before,
            opened_on: (self.input.clone(), self.caret),
            change: Some(Change::Opened {
                opening,
                typed: String::new(),
            }),
        });
    }
}
