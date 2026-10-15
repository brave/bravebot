//! Editing the line being typed: typing, moving, deleting, killing and recalling.

use super::*;

/// A readline kill, as it left the line.
#[derive(Debug, Clone)]
pub(super) struct Kill {
    /// Whether it took text after the caret, which joins at the end of the kill buffer, rather than
    /// before it, which joins at the start.
    forward: bool,
    /// All that the kills joined so far took.
    text: String,
    line: String,
    caret: usize,
}

impl Session {
    /// Accept a typed character.
    ///
    /// Allowed while a turn runs as well as between turns. What it cannot do then is send: a
    /// second turn must not begin while the first is in flight, and [`Session::submit`] still
    /// refuses. Dropping the keys instead, which is what this used to do, meant a user typing
    /// during a slow turn watched their words go nowhere with nothing to say why.
    pub fn type_char(&mut self, c: char) {
        // Before everything, including the two markers below. In NORMAL mode a letter is an
        // instruction, and that is the whole of what the mode means: `!` and `?` there are vi's own
        // keys rather than the ways shell mode and the key list are opened, and reading either as its
        // marker would answer a press that was asking for something else. Both are a press of `i`
        // away for somebody who wanted them.
        if self.vi_normal() {
            self.obey(c);
            return;
        }
        // `?` on an empty line puts the list of keys up rather than typing a character, and a second
        // press takes it down again. Only on an empty line, since a `?` in a sentence is the
        // punctuation somebody is asking a question with, and not in shell mode, where it is a glob
        // for the shell to expand.
        //
        // First, so the press that closes the list is not also the press that clears it below.
        if c == '?' && self.input.is_empty() && !self.shell {
            self.shortcuts = !self.shortcuts;
            return;
        }
        // Any other key takes it down. The question has been asked and moved on from, and somebody
        // typing again has finished reading.
        self.shortcuts = false;

        // `!` on an empty line is the mode rather than a character, which is what makes the rest of
        // the line the command verbatim. Only on an empty line: a `!` inside a sentence is
        // punctuation, and inside a command it is history expansion for the shell to deal with.
        //
        // Idle only. The mode is an armed state that changes what Enter does, and mid-turn the user
        // cannot act on it: it would still be armed when the turn ended, over whatever the box held
        // by then. A cancelled turn puts the prompt back, so `!` during one used to leave a sentence
        // sitting behind a marker, and "rm the old builds" is a reasonable thing to have typed.
        if c == '!' && self.input.is_empty() && !self.shell && self.status == Status::Idle {
            self.shell = true;
            self.shell_private = false;
            return;
        }
        // A second `!` before anything else is typed makes the command private: its output is drawn
        // and never recorded. Like the first it is a mode and not a character, so the line that runs
        // is what the person sees after the marker.
        if c == '!'
            && self.input.is_empty()
            && self.shell
            && !self.shell_private
            && self.status == Status::Idle
        {
            self.shell_private = true;
            return;
        }
        if self.typing_over() {
            self.type_over(c);
            return;
        }
        // Editing a recalled prompt makes it the working line rather than a view of history,
        // so the position indicator goes away as soon as a key is pressed.
        self.history.leave();
        self.begin_an_ordinary_run();
        self.input.insert(self.caret, c);
        self.caret += c.len_utf8();
        // Back to the top of whatever is now offered. A cursor left where it was would sit on a
        // different command after one more letter, so the highlighted row would drift as the list
        // narrowed under it.
        self.completion = 0;
    }

    /// Start a new line in the prompt without sending it.
    ///
    /// Not [`Session::type_char`] with a newline, because that would read a leading `!` as shell
    /// mode and would let the character be typed by any path that thinks it is typing text. A
    /// newline in the prompt is one deliberate keystroke.
    pub fn type_newline(&mut self) {
        if self.typing_over() {
            self.type_over('\n');
            return;
        }
        self.abandon_the_selection();
        self.history.leave();
        self.begin_an_ordinary_run();
        // A line that grew under the list is one the list was not put up over.
        self.shortcuts = false;
        self.input.insert(self.caret, '\n');
        self.caret += 1;
        // A command is one line by definition, and a reference ends at whitespace, so a newline
        // closes whatever was being offered rather than narrowing it.
        self.completion = 0;
    }

    /// Which style of editing the box does.
    pub fn editing(&self) -> crate::vim::Editing {
        self.editing
    }

    /// Open the ordinary box's undo step for an edit that is not a run of typing: a press that
    /// deletes or pastes closes whatever run was open and opens its own step, the way `d`, `c` or
    /// `p` would in vi.
    ///
    /// Vi opens a step where a change begins, at `c`, an opening into INSERT or an operator other
    /// than `y` ([`Session::begin_a_change`]); the ordinary box has no such moment to open one at,
    /// so each of these opens and closes its own instead. A no-op is read before this is reached, so
    /// a press that changes nothing opens no step either, the way `y` fills no step for vi.
    pub(super) fn begin_an_ordinary_edit(&mut self) {
        if self.editing() == crate::vim::Editing::Ordinary {
            self.begin_a_change();
            self.ordinary_run_open = false;
        }
    }

    /// Open the ordinary box's undo step for a run of typing, once for the run rather than once a
    /// character, the way everything typed into one vi INSERT session shares the step that opened
    /// it ([`Session::begin_a_change`]).
    fn begin_an_ordinary_run(&mut self) {
        if self.editing() == crate::vim::Editing::Ordinary && !self.ordinary_run_open {
            self.begin_a_change();
            self.ordinary_run_open = true;
        }
    }

    /// The line being typed.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Where the next keystroke will land, as a byte offset into the line.
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// Replace the line, leaving the caret where the user would carry on typing.
    ///
    /// Everything that puts a whole line in the box goes through here, so no path can leave the
    /// caret pointing into a line that is no longer there.
    ///
    /// The list of keys goes with it, for the same reason and in the same one place. It is not part
    /// of the line and cannot be rewritten with it, but it stands over the box, and a list left up
    /// over a line that arrived under it belongs to a press two prompts ago. Several callers took it
    /// down themselves and the ones that did not were the ones a person reached mid-turn: recalling
    /// an earlier prompt, and a stopped turn handing its prompt back.
    ///
    /// The cursor into what is offered goes with it too, for a reason the list of keys does not
    /// have: a row is read as a choice somebody made. What is offered is a function of the line, so
    /// a line that arrived whole is a new list and an index into the one before it stands for a row
    /// nobody walked to. Recalling `look at @test` under a cursor an arrow had moved sent nothing
    /// and left `look at @tests/` in the box, because a finished name loses to a chosen row.
    ///
    /// The steps to undo go too, being copies of a line that is no longer the one in the box. Kept,
    /// `u` after a send would put back the prompt that had just gone.
    pub(super) fn set_input(&mut self, line: impl Into<String>) {
        self.handoff = false;
        self.put_in_the_box(line);
        self.undo.clear();
        self.inserting = None;
        self.ordinary_run_open = false;
    }

    /// Replace the line as [`Session::set_input`] does, keeping what `u` can take back.
    ///
    /// For a completion, which finishes the line being typed rather than bringing another.
    pub(super) fn put_in_the_box(&mut self, line: impl Into<String>) {
        self.abandon_the_selection();
        // A line that arrives whole has none of the rows the last selection was kept by.
        self.last_selection = None;
        self.input = line.into();
        self.caret = self.input.len();
        self.shortcuts = false;
        self.completion = 0;
    }

    /// Whether the line has more than one line in it, which is what gives Up and Down something
    /// to move between.
    pub fn is_multiline(&self) -> bool {
        self.input.contains('\n')
    }

    /// The line the caret is on, as byte offsets into the input.
    pub(super) fn caret_line(&self) -> (usize, usize) {
        let start = self.input[..self.caret]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        let end = self.input[self.caret..]
            .find('\n')
            .map_or(self.input.len(), |newline| self.caret + newline);
        (start, end)
    }

    /// Move the caret one character towards the start, counting a marker as one.
    ///
    /// A marker stands for one thing, and a caret resting in the middle of one would be a caret
    /// between two halves of a picture. So it is stepped over whole, in either direction, and the
    /// places the caret can rest are the same places the user can see.
    pub fn move_left(&mut self) {
        if let Some((start, _)) = self.marker_before_caret() {
            self.caret = start;
            return;
        }
        if let Some(c) = self.input[..self.caret].chars().next_back() {
            self.caret -= c.len_utf8();
        }
    }

    /// Move the caret one character towards the end, counting a marker as one.
    pub fn move_right(&mut self) {
        if let Some((_, end)) = self.marker_at_caret() {
            self.caret = end;
            return;
        }
        if let Some(c) = self.input[self.caret..].chars().next() {
            self.caret += c.len_utf8();
        }
    }

    /// Move the caret to the start of the word before it.
    ///
    /// Words are runs of anything but whitespace, which is what makes a path or a flag one word:
    /// stopping inside `--file` or `src/main.rs` would be several presses to cross something the
    /// user thinks of as one thing.
    pub fn move_word_left(&mut self) {
        while self.input[..self.caret]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
        {
            self.move_left();
        }
        while self.input[..self.caret]
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_whitespace())
        {
            self.move_left();
        }
    }

    /// Move the caret to the end of the word after it.
    pub fn move_word_right(&mut self) {
        while self.input[self.caret..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            self.move_right();
        }
        while self.input[self.caret..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace())
        {
            self.move_right();
        }
    }

    /// Move the caret to the start of the line it is on.
    pub fn move_to_line_start(&mut self) {
        self.caret = self.caret_line().0;
    }

    /// Move the caret to the end of the line it is on.
    pub fn move_to_line_end(&mut self) {
        self.caret = self.caret_line().1;
    }

    /// Move the caret to the start of its line, or to the start of the line above when it is
    /// already there.
    ///
    /// `false` when there was nowhere left to go, so the caller can leave the key to the transcript
    /// at the top of the prompt rather than having it do nothing.
    ///
    /// Two presses to cross a line boundary, which is what makes the first press cheap: someone who
    /// wanted the start of this line gets it without also losing their place in the paragraph.
    pub fn page_up(&mut self) -> bool {
        let (start, _) = self.caret_line();
        if self.caret > start {
            self.caret = start;
            return true;
        }
        if start == 0 {
            return false;
        }
        self.caret = self.input[..start - 1]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        true
    }

    /// Move the caret to the end of its line, or to the end of the line below when it is already
    /// there.
    pub fn page_down(&mut self) -> bool {
        let (_, end) = self.caret_line();
        if self.caret < end {
            self.caret = end;
            return true;
        }
        if end == self.input.len() {
            return false;
        }
        let below = end + 1;
        self.caret = self.input[below..]
            .find('\n')
            .map_or(self.input.len(), |newline| below + newline);
        true
    }

    /// Move the caret to the line above, keeping its position along the line where it can.
    ///
    /// `false` when there is no line above, which is what leaves Up to the history it belongs to
    /// on a line with nothing to move within.
    pub fn move_up_a_line(&mut self) -> bool {
        let (start, _) = self.caret_line();
        if start == 0 {
            return false;
        }
        let above = self.input[..start - 1]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        self.caret = along(&self.input[above..start - 1], self.column()) + above;
        self.settle_onto_a_marker();
        true
    }

    /// Move the caret to the line below, keeping its position along the line where it can.
    pub fn move_down_a_line(&mut self) -> bool {
        let (_, end) = self.caret_line();
        if end == self.input.len() {
            return false;
        }
        let below = end + 1;
        let ends = self.input[below..]
            .find('\n')
            .map_or(self.input.len(), |newline| below + newline);
        self.caret = along(&self.input[below..ends], self.column()) + below;
        self.settle_onto_a_marker();
        true
    }

    /// Take the caret onto a marker it has landed inside of.
    ///
    /// A place along a line is counted in characters, and the characters a marker happens to be
    /// spelled with count like any others, so a move between lines lands where Left and Right
    /// refuse to stop and the next thing typed splits the marker in half.
    ///
    /// Onto the start of it rather than past the end, because the caret is drawn over the whole
    /// marker it is on: the column the move was keeping is still under the caret, and the marker
    /// the person is now looking at is the one they were aiming at.
    pub(super) fn settle_onto_a_marker(&mut self) {
        if let Some((start, _)) = self.marker_at_caret() {
            self.caret = start;
        }
    }

    /// How many characters along its line the caret is.
    fn column(&self) -> usize {
        let (start, _) = self.caret_line();
        self.input[start..self.caret].chars().count()
    }

    /// Delete the character after the caret, or the whole marker the caret is on.
    ///
    /// Whole for the reason [`Session::backspace`] takes one whole: the caret rests on a marker
    /// as it rests on a character, and half a marker stands for nothing.
    pub fn delete_forward(&mut self) {
        if self.caret == self.input.len() {
            return;
        }
        self.begin_an_ordinary_edit();
        self.abandon_the_selection();
        self.history.leave();
        match self.marker_at_caret() {
            Some((start, end)) => self.input.replace_range(start..end, ""),
            None => {
                self.input.remove(self.caret);
            }
        }
        self.completion = 0;
    }

    /// Delete the word before the caret.
    ///
    /// Nothing where there is no character before it, as [`Session::delete_forward`] does with none
    /// in front: a press that deletes nothing leaves the line, the history it is being browsed from,
    /// and any selection standing over it exactly as they were.
    pub fn delete_word_before(&mut self) {
        if self.caret == 0 {
            return;
        }
        if self.typing_over() {
            let was = self.caret;
            self.move_word_left();
            let start = std::mem::replace(&mut self.caret, was);
            self.take_back_as_far_as(start);
            return;
        }
        self.abandon_the_selection();
        self.history.leave();
        let was = self.caret;
        self.move_word_left();
        self.kill(self.caret, was, false);
    }

    /// Delete the word after the caret, with the blanks in front of it.
    ///
    /// Nothing at the end of the line, for the reason [`Session::delete_word_before`] does nothing
    /// at the start of it.
    pub fn delete_word_after(&mut self) {
        let was = self.caret;
        self.move_word_right();
        let end = std::mem::replace(&mut self.caret, was);
        if end == was || self.typing_over() {
            return;
        }
        self.abandon_the_selection();
        self.history.leave();
        self.kill(was, end, true);
    }

    /// Take `from..to` out of the line and keep it for [`Session::yank`], joined to what the last
    /// kill took where this one goes the same way and the line has not changed since.
    ///
    /// The register vi's `p` reads, so there is one place for removed text, and it is put back as
    /// characters whatever the register held before.
    fn kill(&mut self, from: usize, to: usize, forward: bool) {
        self.begin_an_ordinary_edit();
        let taken = self.input[from..to].to_string();
        // Where the caret was: the callers may have moved it to the far end of the stretch already.
        let was = if forward { from } else { to };
        let joined = self.last_kill.take().filter(|last| {
            last.forward == forward
                && last.line == self.input
                && last.caret == was
                && self
                    .register
                    .as_ref()
                    .is_some_and(|r| !r.lines && r.text == last.text)
        });
        let text = match joined {
            Some(last) if forward => last.text + &taken,
            Some(last) => taken + &last.text,
            None => taken,
        };
        self.input.replace_range(from..to, "");
        self.caret = from;
        self.completion = 0;
        self.register = Some(Yanked {
            text: text.clone(),
            lines: false,
        });
        self.last_kill = Some(Kill {
            forward,
            text,
            line: self.input.clone(),
            caret: self.caret,
        });
    }

    /// Put the text the last kill or vi's `d` or `y` took back at the caret.
    ///
    /// Nothing where nothing has been taken. A marker in it names its attachment again while that is
    /// still staged, and is text like any other once it is not.
    pub fn yank(&mut self) {
        let Some(yanked) = self.register.clone() else {
            return;
        };
        if yanked.text.is_empty() {
            return;
        }
        self.begin_a_change();
        // Closes whatever run was open in the ordinary box, the way `begin_an_ordinary_edit` does:
        // a yank is not a run of typing, and leaving the flag set would have the next character
        // typed find a run already open and share a step this press already closed.
        self.ordinary_run_open = false;
        self.abandon_the_selection();
        // The list stands over a line that arrived under it as it does over a paste or a recalled
        // one (INPUT-13).
        self.shortcuts = false;
        self.input.insert_str(self.caret, &yanked.text);
        self.caret += yanked.text.len();
    }

    /// Delete from the caret back to the start of its line.
    ///
    /// Nothing where the caret is already there, for the reason [`Session::delete_word_before`] does
    /// nothing at the start of the line.
    pub fn delete_to_line_start(&mut self) {
        let (start, _) = self.caret_line();
        if start == self.caret {
            return;
        }
        if self.typing_over() {
            self.take_back_as_far_as(start);
            return;
        }
        self.abandon_the_selection();
        self.history.leave();
        self.kill(start, self.caret, false);
    }

    /// Delete from the caret to the end of its line.
    ///
    /// Nothing where the caret is already there, for the reason [`Session::delete_word_before`] does
    /// nothing at the start of the line.
    pub fn delete_to_line_end(&mut self) {
        let (_, end) = self.caret_line();
        if end == self.caret {
            return;
        }
        self.abandon_the_selection();
        self.history.leave();
        self.kill(self.caret, end, true);
    }

    /// Delete the character before the caret, or leave shell mode where there is nothing left to
    /// delete.
    ///
    /// Deleting back past the `!` leaves the mode, which is where the marker appears to be: a user
    /// who typed it by mistake gets rid of it the way they got rid of any other character. Without
    /// this the mode could only be left by clearing the whole line.
    ///
    /// A marker goes whole. It is one thing on the screen and one thing to the user, so taking a
    /// character off the end of it would leave text standing for a picture that is no longer
    /// attached, and the user would have to keep pressing to find out.
    ///
    /// A marker the caret is covering goes first, before anything in front of it. The covering is
    /// visible: the whole of that marker is drawn under the caret, and a press that took the
    /// character beside it instead would take something the user could see was not selected.
    ///
    /// Nothing where there is nothing to delete, as [`Session::delete_word_before`] does at the
    /// same caret: the press leaves the line, the history it is being browsed from, and any
    /// selection standing over it exactly as they were.
    pub fn backspace(&mut self) {
        // Read before anything is touched, because whether this press deletes at all is the
        // question the guard below asks: a marker at the first column is under a caret there, and
        // that press takes it.
        let marker = self
            .marker_at_caret()
            .or_else(|| self.marker_before_caret());
        // Nothing before the caret is where the marker appears to be, so this is the press that
        // deletes it. Whatever follows stays and becomes an ordinary prompt: the mode is what was
        // deleted, not the words. Leaving shell mode is not an edit of the line, so the offsets a
        // selection is held as stay valid across it and the stretch stays on the screen.
        if marker.is_none() && self.caret == 0 {
            // The second marker goes first, so `!!` backs out to `!` as it was typed.
            if self.shell_private() {
                self.shell_private = false;
            } else {
                self.shell = false;
            }
            return;
        }
        if self.typing_over() {
            self.take_back_what_was_typed_over();
            return;
        }
        self.begin_an_ordinary_edit();
        self.abandon_the_selection();
        self.history.leave();
        if let Some((start, end)) = marker {
            self.input.replace_range(start..end, "");
            self.caret = start;
            self.completion = 0;
            return;
        }
        self.move_left();
        self.input.remove(self.caret);
        self.completion = 0;
    }

    /// Discard whatever has been typed, keeping it as the draft Up brings back first.
    ///
    /// A prompt walked back to is not kept: it is in the history already, and keeping it would
    /// displace a draft for a line that was never lost.
    ///
    /// Guarded like the other editing methods: input belongs to the idle state, and clearing it
    /// mid-turn would mean the field the user returns to is not the one they left.
    pub fn clear_input(&mut self) {
        if self.status == Status::Idle {
            if !self.history.on_a_sent_prompt() && !self.input.trim().is_empty() {
                self.history.keep_draft(self.input.clone());
            }
            self.history.leave();
            self.set_input(String::new());
            // The mode goes with the line. Escape means "never mind this", and leaving the marker
            // behind would arm the next thing typed as a command. The list of keys goes with the
            // line too, which is settled where the line is written.
            self.shell = false;
        }
    }

    /// Put the line away, or bring back the one that was put away.
    ///
    /// Which of the two it does is read off the line rather than chosen: a line to put away is put
    /// away, and an empty box is where a line put away earlier is wanted. Returns whether anything
    /// happened, so a press that had nothing to do either way can be told from one that acted.
    ///
    /// The words alone travel. The mode stays where the user left it, so a prompt put away as a
    /// prompt comes back into an armed shell as the command they meant to write, and a caret is not
    /// carried because it belongs to an edit that has finished. Bringing one back empties the slot:
    /// the line is in the box now, and a second press would put a copy of it beside the first.
    ///
    /// Markers are not touched on the way out. What is staged stays staged, and the row beneath the
    /// box goes on saying so because it is drawn from what the line names: a marker put away names
    /// nothing until the words holding it come back, and then it names what it always did.
    ///
    /// Allowed while a turn runs, exactly as typing and recall are: this writes a line and sends
    /// nothing, and sending is the whole of what a running turn refuses. A turn in flight is when a
    /// person most wants a half-written thought out of the way, since it is when a better one has
    /// just occurred to them.
    pub fn stash(&mut self) -> bool {
        self.history.leave();
        // The list of keys is not part of the line and cannot be put away, but it is standing over
        // a box that is about to change, and everything else that rewrites the box takes it down.
        self.shortcuts = false;
        self.completion = 0;

        if self.input.is_empty() {
            // Taken rather than read, so the slot empties as the line fills: what was put away is
            // back in front of the user, and the only copy of it is the one they can see.
            match self.stashed.take() {
                Some(line) => {
                    self.set_input(line);
                    true
                }
                None => false,
            }
        } else {
            // Overwriting rather than stacking. One slot is what the key promises, and a press that
            // silently pushed a second line would leave the first reachable only by pressing again.
            self.abandon_the_selection();
            self.last_selection = None;
            self.stashed = Some(std::mem::take(&mut self.input));
            self.caret = 0;
            self.undo.clear();
            self.inserting = None;
            self.ordinary_run_open = false;
            true
        }
    }

    /// The line put away, for saying that there is one.
    pub fn stashed(&self) -> Option<&str> {
        self.stashed.as_deref()
    }

    /// Take the line back from an external editor.
    ///
    /// Replaces rather than appends: the editor was opened on this line, so what comes back is
    /// the same line after however much thought, not something to add to it.
    ///
    /// Guarded like the other editing methods. Nothing can reach it mid-turn, since the key is
    /// ignored while one runs, but the field belongs to the idle state either way.
    pub fn take_edited(&mut self, line: impl Into<String>) {
        if self.status != Status::Idle {
            return;
        }
        // A recalled prompt that has been through an editor is the working line now, exactly as
        // it would be after a keystroke.
        self.history.leave();
        self.set_input(line);
        self.completion = 0;
    }

    /// Show the previous prompt, stepping further back on each call.
    ///
    /// Allowed while a turn runs, exactly as typing into the box is. Recall writes a line and
    /// sends nothing, and sending is the whole of what a running turn refuses. It used to refuse
    /// here too, from the days when the box took nothing mid-turn at all; typing was opened up
    /// and this was left behind, so a person could compose their next prompt during a turn but
    /// not reach the one they sent last.
    pub fn recall_older(&mut self) {
        if let Some(prompt) = self.history.older(&self.input) {
            self.set_input(prompt);
        }
    }

    /// The scope chord on a sent prompt: walk the next scope that holds a prompt, staying on the
    /// one on screen where it is in that scope.
    pub fn cycle_history_scope(&mut self) {
        if let Some(prompt) = self.history.cycle() {
            self.set_input(prompt);
        }
    }

    /// Step forward through recalled prompts, back to the line being typed.
    pub fn recall_newer(&mut self) {
        if let Some(prompt) = self.history.newer() {
            self.set_input(prompt);
        }
    }
}
