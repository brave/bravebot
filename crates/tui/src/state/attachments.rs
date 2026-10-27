//! What a line carries beside its words: text pasted, files dropped and things attached, and the markers
//! that stand for them in the box.

#[cfg_attr(
    not(test),
    expect(
        clippy::wildcard_imports,
        reason = "a child of state.rs, which holds Session and the types every sibling shares"
    )
)]
use super::*;

/// How many newlines a paste carries before it is folded behind a marker.
///
/// Counted in newlines rather than in lines so that three lines with nothing after the last of
/// them is the first paste to fold: the third newline is the point where the box was going to grow
/// past what a prompt is meant to look like.
const FOLD_AT_NEWLINES: usize = 3;

/// How many lines a paste is, as a person reading it would count them.
///
/// A trailing newline ends the last line rather than starting another, so text copied with the end
/// of its last line does not claim an empty line that nobody can see.
fn lines_in(text: &str) -> usize {
    let newlines = text.matches('\n').count();
    if text.ends_with('\n') {
        newlines
    } else {
        newlines + 1
    }
}

/// A command line on its way to being carried out, and the pictures it named.
///
/// The two travel together because what becomes of a picture depends on the command: three of them
/// hand their argument to a planner and a picture goes with it, and for the rest there is nowhere
/// for one to go. Which is which is the caller's to know, so the box hands over both and says
/// nothing about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commanded {
    /// The line, exactly as it was typed, markers and all.
    pub line: String,
    /// The pictures pasted into it, in the order the markers number them.
    pub pasted: Vec<AttachedImage>,
    /// The files dropped onto it, in the order the markers number them.
    ///
    /// Beside the pictures rather than folded into them, because the two reach a request by
    /// different routes: a picture is bytes that never touched the filesystem, and a dropped file is
    /// a path something has to read through a gate.
    pub attached: Vec<Attached>,
}

/// A thumbnail of a dropped picture, read for the screen and for nothing else.
///
/// The file is one the person just dragged onto the window, so the path is their gesture's and
/// not a model's. A PDF, a text file, a file too big to be an attachment, or one that will not
/// decode has no thumbnail. The turn reads the file again under its own policy, exactly as
/// before; this read decides nothing and sends nothing, and happens off the interface's thread.
pub(super) fn preview_of_dropped(
    found: &crate::dropped::Dropped,
) -> Option<crate::preview::Preview> {
    let bravebot_filetype::by_name::Kind::Attachment(media) = found.kind else {
        return None;
    };
    if !media.starts_with("image/") {
        return None;
    }
    Some(crate::preview::Preview::start(
        crate::preview::Source::File(std::path::PathBuf::from(&found.path)),
        crate::preview::Fit::Thumbnail,
    ))
}

/// Thumbnails for pictures and files going back into the box, each started again from what was
/// staged.
///
/// A line that is sent clears its previews, and a drawable picture cannot be cloned into the sent
/// set, so a line that returns to the box (a stopped turn, an unqueue) has its thumbnails made
/// again. Display only, as the first ones were.
pub(super) fn previews_restaged(
    pasted: &[AttachedImage],
    attached: &[Attached],
) -> Vec<(String, crate::preview::Preview)> {
    let pictures = pasted.iter().map(|picture| {
        (
            picture.marker.clone(),
            crate::preview::Preview::start(
                crate::preview::Source::Bytes(picture.bytes.clone()),
                crate::preview::Fit::Thumbnail,
            ),
        )
    });
    let files = attached.iter().filter_map(|file| {
        let dropped = crate::dropped::Dropped {
            path: file.shown.clone(),
            kind: file.kind,
        };
        preview_of_dropped(&dropped).map(|preview| (file.marker.clone(), preview))
    });
    pictures.chain(files).collect()
}

/// A file dropped on the box, and the marker standing for it in the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attached {
    /// What the user sees in the line, such as `[Image #1]`.
    pub marker: String,
    /// The name to give the task, already checked against what the workspace can open.
    pub name: String,
    /// The path as the user's filesystem names it, for showing them what they attached.
    pub shown: String,
    pub kind: bravebot_filetype::by_name::Kind,
}

/// A picture pasted into the line being typed.
///
/// Separate from [`Attached`] because the two arrive by different routes and only one of them has
/// a path: a dropped file is read out of the workspace, where the trust map has something to say
/// about it, and a paste is bytes that never touched the filesystem. They share the marker
/// numbering, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedImage {
    /// The text standing for it in the prompt, as `[Image #1]`.
    ///
    /// Held rather than derived from a position, because the line is edited around it: the picture
    /// belongs to the words the marker sits in, and finding it again by counting would go wrong the
    /// first time somebody rewrote the sentence.
    pub marker: String,
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

/// A line with every marker in it standing for one of `carried` replaced by what it says instead.
///
/// One place, because a picture misses a request for more than one reason and the sentence that
/// takes its place reads the same however it was missed. `why` is the reason, which is the caller's:
/// see [`bravebot_filetype::markers::picture_in_words`].
pub(super) fn put_back_to_words(line: &str, carried: &[AttachedImage], why: &str) -> String {
    bravebot_filetype::markers::put_back_to_words(
        line,
        carried.iter().map(|picture| picture.marker.as_str()),
        why,
    )
}

/// A line with every marker in it standing for one of `dropped` replaced by that file's name.
///
/// What a file becomes wherever the thing being built cannot be handed the file itself. A name is
/// enough for that: the planner reads it and goes to the file through the same gate it reads any
/// other file through, which is what a file dropped onto a line queued mid-turn and a prompt
/// recalled from the history both already rely on (`dropping.md` DROP-8, DROP-9).
///
/// Unlike a picture there is no sentence to write, because the name is the honest answer rather than
/// a report that the honest answer is missing.
pub(super) fn put_back_to_names(line: &str, dropped: &[Attached]) -> String {
    bravebot_filetype::markers::put_back_to_names(
        line,
        dropped
            .iter()
            .map(|file| (file.marker.as_str(), file.name.as_str())),
    )
}

/// The same command with the text files it named put back to their names.
///
/// What a command whose argument reaches a planner outside any turn is sent as, which is `/btw` and
/// `/manifest`. A picture and a PDF are read and carried as bytes, because bytes are what the
/// request can hold. A text file's contents would be a context message, and both of those planners
/// are precommitted to a context holding the task and the driver's own words and nothing read out
/// of the tree, so what a text file becomes here is its name.
///
/// Nothing is said about it, unlike a picture the command cannot carry: the name is what the file
/// is, so the planner is given something it can act on and the person reads the question they asked
/// with the file named in it. A picture is the case where no such name exists, which is why that one
/// is spoken of and this one is not.
///
/// A free function, and beside [`put_back_to_names`] rather than on the session, because it reads
/// the line and writes nothing: there is nothing here for the box to be told.
pub fn without_text_files(commanded: Commanded) -> Commanded {
    let (text, carried) = commanded
        .attached
        .into_iter()
        .partition::<Vec<_>, _>(|file| file.kind == bravebot_filetype::by_name::Kind::Text);
    Commanded {
        line: put_back_to_names(&commanded.line, &text),
        pasted: commanded.pasted,
        attached: carried,
    }
}

/// A paragraph pasted into the line, standing behind the marker written in its place.
///
/// Several lines of text in the box push everything else off the screen, and what they push off is
/// the reply the paste was about. So a paste of any length reads as one row, and the row says how
/// many lines are behind it.
///
/// The marker is the handle, as it is for a picture, and it is the only part of this a user can
/// see: deleting it is how the paste is taken back. Unlike a picture it is put back before the
/// prompt is sent, because here the marker stands for the words themselves rather than for
/// something travelling beside them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PastedText {
    /// The text standing for it in the line, as `[Pasted text #1 +12 lines]`.
    pub marker: String,
    /// What was pasted, with its line endings already normalised.
    pub text: String,
}

impl Session {
    /// Insert pasted text into the input.
    ///
    /// Kept apart from typing because a paste is one act, not a stream of keys. Pasted text
    /// routinely ends in a newline, and a terminal that delivers a paste as keystrokes turns
    /// that into Enter: a prompt copied from somewhere else used to send itself before its
    /// author had read it back. Nothing here submits.
    ///
    /// Line endings are normalised so text copied from anywhere lands as the same thing. The
    /// newlines are kept rather than flattened, since a pasted paragraph was written with them
    /// and the box draws them.
    pub fn paste(&mut self, text: &str) {
        // Every path that writes text in whole comes through here (a paste, a dropped file, an
        // image marker), so this is the one place that opens the ordinary box's step for all of
        // them: one step for whichever of the three it was, not one for each insertion folding or
        // a marker makes underneath it.
        self.begin_an_ordinary_edit();
        self.abandon_the_selection();
        self.history.leave();
        // The list stands over a line that arrived under it as it does over a recalled one.
        self.shortcuts = false;
        let text = normalised(text);
        self.input.insert_str(self.caret, &text);
        self.caret += text.len();
        // Back to the top of whatever is now offered, for the reason [`Session::type_char`] goes
        // back: the list is a function of the line, and this changed the line. A cursor left where
        // an arrow put it would stand for a row of the list that was on the screen before the
        // paste, and a row nobody walked to is read as a choice all the same, so Enter on a name
        // the user had finished typing completes it away into whichever entry the stale index now
        // lands on. Clamping when the cursor is read cannot stand in for this: the stale index is
        // usually inside the narrowed list rather than past its end.
        self.completion = 0;
    }

    /// Take text the user pasted, folding a long one behind a marker.
    ///
    /// [`Session::paste`] writes whatever it is given, which is what the markers themselves are
    /// written with, so the folding lives here: a paste is the one thing that arrives long enough
    /// to be worth hiding, and everything else that reaches the line is already a row or less.
    ///
    /// Long is counted in newlines rather than in rows the box would draw, because a wrapped line
    /// is one line the user pasted and folding on the width would fold differently in a narrow
    /// window. Two of them read fine in the box; the third is where a paste starts taking the
    /// screen, so that is where it is put away.
    ///
    /// Shell mode is left alone, and has to be: the line there is the command, and a command that
    /// is not what the user is looking at is the one thing that mode may never do.
    ///
    /// Characters a terminal draws as nothing are taken out first, in shell mode too, and the
    /// number removed is said. That comes before the fold so the words a marker stands for are the
    /// words that were checked, and before the shell test because a command is the line where
    /// text nobody can see matters most (`pasting.md` PASTE-10).
    pub fn paste_text(&mut self, text: &str) {
        let (text, removed) = crate::invisible::without_invisible(&normalised(text));
        if removed > 0 {
            self.note(t!(paste_invisible_removed, count = removed));
        }
        if self.shell || text.matches('\n').count() < FOLD_AT_NEWLINES {
            self.paste(&text);
            return;
        }

        // Numbered off the counter a dropped file and a pasted picture use, so no two markers in
        // one line can carry the same number and a number is never reused.
        self.attachments_made += 1;
        let marker = t!(
            paste_folded,
            number = self.attachments_made,
            lines = lines_in(&text)
        );
        self.paste(&marker);
        self.pasted_text.push(PastedText { marker, text });
    }

    /// A line with every paste marker in it put back to the text it stands for.
    ///
    /// Every marker, not the ones a count says should be there: a user who deleted one meant to
    /// drop that paste, and one who copied a marker to somewhere else in the line meant the words
    /// twice.
    ///
    /// Called where the line leaves the box, so the words are what is sent, what the transcript
    /// keeps and what the history remembers. The box is the only place a marker belongs: it is
    /// there to keep a stack trace from taking the screen while somebody is typing around it, and
    /// a marker that outlived the line would be a handle on words this session is the only one
    /// holding.
    pub fn unfolded(&self, line: &str) -> String {
        let mut line = line.to_string();
        for pasted in &self.pasted_text {
            line = line.replace(&pasted.marker, &pasted.text);
        }
        line
    }

    /// A line with every paste in it put back behind the marker that stood for it.
    ///
    /// The inverse, for a line coming back to the box after a turn was stopped. What returns is
    /// the line as it was typed, because the box is where a long paste takes the screen and
    /// somebody who has just stopped a turn is about to edit the prompt, not read it.
    pub fn folded(&self, line: &str) -> String {
        let mut line = line.to_string();
        for pasted in &self.pasted_text {
            line = line.replace(&pasted.text, &pasted.marker);
        }
        line
    }

    /// A line in the form worth remembering, with every marker settled.
    ///
    /// A marker stands for something staged beside the line, and nothing staged outlives the
    /// session that staged it. Remembered as it stands, a marker comes back naming nothing: a
    /// picture nobody can produce, a file nothing read.
    ///
    /// A dropped file becomes its name, which is a path a person recognises and one the planner
    /// can go and read through the gate it reads anything through. That is what a file dropped
    /// onto a line queued mid-turn already becomes.
    ///
    /// A picture becomes nothing at all, along with the space beside it, because there is no
    /// durable text that stands for a screenshot and the words around it are what the person
    /// meant. Somebody recalling that prompt pastes the picture they mean now.
    pub(super) fn recallable(&self, line: &str) -> String {
        let mut line = line.to_string();
        for attached in &self.attached {
            line = line.replace(&attached.marker, &attached.name);
        }
        for pasted in &self.pasted {
            line = line.replace(&format!("{} ", pasted.marker), "");
            line = line.replace(&format!(" {}", pasted.marker), "");
            line = line.replace(&pasted.marker, "");
        }
        line
    }

    /// Take a paste that turned out to be a drop, or say it was not one.
    ///
    /// A recognised file becomes a marker in the line and an attachment behind it. Anything else,
    /// an unsupported type or a path naming no file at all, has its path written out, which is
    /// what dropping a file did before any of this existed.
    ///
    /// In shell mode every file has its path written out, quoted as one word, and nothing is
    /// staged, so the command the shell is given names the file and no record is left behind the
    /// line. A drop whose names hold a control character, or one a terminal draws as nothing,
    /// writes nothing and says why (`dropping.md` DROP-6).
    ///
    /// Returns whether the text was a drop at all. A paste that was not one is left to
    /// [`Session::paste`], untouched.
    pub fn drop_files(&mut self, text: &str) -> bool {
        let exists = |path: &str| std::path::Path::new(path).is_file();
        if !crate::dropped::is_drop(text, exists) {
            return false;
        }

        // The line in shell mode is a command and nothing else, and the shell is handed it as it
        // stands. A marker there would reach it as text, so nothing is staged and the path is
        // written as one word, which is what a terminal does when a file is dropped into a shell.
        if self.shell {
            let paths = crate::dropped::paths(text);
            // Taking the characters out would name another file, and leaving them in would let
            // the line run something the box does not show. A control character is in the same
            // case: the box rewrites a carriage return as a newline.
            if paths.iter().any(|path| {
                path.chars().any(char::is_control)
                    || crate::invisible::without_invisible(path).1 > 0
            }) {
                self.note(t!(drop_name_not_written));
                return true;
            }
            let words: Vec<String> = paths
                .iter()
                .map(|path| crate::dropped::shell_word(path))
                .collect();
            self.paste(&format!("{} ", words.join(" ")));
            return true;
        }

        let taken = crate::dropped::dropped_with(text, exists);
        let mut written = Vec::new();

        for path in crate::dropped::paths(text) {
            match taken
                .iter()
                .find(|found| found.path == path)
                .and_then(|found| {
                    crate::dropped::name_for(&self.workspace, &found.path).map(|name| (found, name))
                }) {
                Some((found, name)) => {
                    self.attachments_made += 1;
                    let marker = format!("[{} #{}]", found.kind.noun(), self.attachments_made);
                    if let Some(preview) = preview_of_dropped(found) {
                        self.previews.push((marker.clone(), preview));
                    }
                    self.attached.push(Attached {
                        marker: marker.clone(),
                        name,
                        shown: found.path.clone(),
                        kind: found.kind,
                    });
                    written.push(marker);
                }
                // Out of reach, or a type nothing here takes. The path is what a drop always
                // produced, and it is still useful: the user can read it and say what they meant.
                None => written.push(path),
            }
        }

        // A trailing space, which is what a terminal does when a file is dropped into a shell:
        // whatever is typed next, or dropped next, does not run into the marker.
        self.paste(&format!("{} ", written.join(" ")));
        true
    }

    /// The attachments the line still names, in the order they appear in it.
    ///
    /// Read back out of the line rather than taken wholesale, so deleting a marker takes its
    /// attachment off. That is the only way a user has to change their mind, since the marker is
    /// the only part of it they can see.
    pub fn attachments_named(&self, line: &str) -> Vec<Attached> {
        self.named_in(line).cloned().collect()
    }

    /// What the line being typed carries, for drawing under the box.
    ///
    /// The same question the turn is built from asks, so the row and the turn can never disagree
    /// about which files a line is carrying. Drawn from what a drop staged instead, a file whose
    /// marker the person had rubbed out kept its row, which is the one place they can see whether
    /// rubbing it out worked.
    pub fn attached_to_the_line(&self) -> impl Iterator<Item = &Attached> {
        self.named_in(&self.input)
    }

    /// Everything a drop staged, whether or not the line still names it.
    pub fn attached(&self) -> &[Attached] {
        &self.attached
    }

    /// The attachments a line names, without cloning them.
    fn named_in<'a>(&'a self, line: &'a str) -> impl Iterator<Item = &'a Attached> {
        self.attached
            .iter()
            .filter(move |attached| line.contains(&attached.marker))
    }

    /// What the line carried when it was sent, for the task being built from it.
    pub fn sent_attachments(&self) -> &[Attached] {
        &self.sent
    }

    /// Attach a pasted picture, writing the text that stands for it where the caret is.
    ///
    /// The marker is what makes a picture something a person can see and edit. Without one the
    /// prompt would say nothing about what came with it, and a user would be left counting pastes
    /// to work out what the planner was about to be shown.
    ///
    /// Numbered off the same counter a dropped file uses, so a paste and a drop can never both
    /// call themselves `[Image #1]`, and so a number is never reused: renumbering on a deletion
    /// would change the marker sitting in the line the user is looking at.
    pub fn attach(&mut self, image: crate::clipboard::Image) {
        self.attachments_made += 1;
        // Not from a catalog, and deliberately. Unlike a folded paste, which is put back to its
        // words before the turn is built, this marker is sent as it stands: the planner reads
        // "[Image #2]" and counts to the picture that answers it. Translating it would change
        // what the model is given, which is the one thing a change of language must not do.
        let noun = bravebot_filetype::by_name::Kind::Attachment(image.media_type).noun();
        let marker = format!("[{noun} #{}]", self.attachments_made);
        self.paste(&marker);
        // The bytes are the user's own paste, taken from their clipboard a moment ago. A picture
        // that will not decode, or a terminal that draws none, just has no thumbnail.
        self.previews.push((
            marker.clone(),
            crate::preview::Preview::start(
                crate::preview::Source::Bytes(image.bytes.clone()),
                crate::preview::Fit::Thumbnail,
            ),
        ));
        self.pasted.push(AttachedImage {
            marker,
            media_type: image.media_type,
            bytes: image.bytes,
        });
    }

    /// The pictures the line still names, in the order they appear in it.
    ///
    /// Read back out of the line for the reason [`Session::attachments_named`] is: deleting the
    /// marker is the only way a user has to take a picture back off.
    pub fn pasted_named(&self, line: &str) -> Vec<AttachedImage> {
        self.pasted
            .iter()
            .filter(|pasted| line.contains(&pasted.marker))
            .cloned()
            .collect()
    }

    /// The thumbnails of the pictures the line still names, in the order the line names them.
    pub fn previews_on_the_line(&self) -> Vec<&crate::preview::Thumb> {
        let mut named: Vec<(usize, &crate::preview::Thumb)> = self
            .previews
            .iter()
            .filter_map(|(marker, preview)| {
                let at = self.input.find(marker.as_str())?;
                Some((at, preview.thumb()?))
            })
            .collect();
        named.sort_by_key(|(at, _)| *at);
        named.into_iter().map(|(_, thumb)| thumb).collect()
    }

    /// Take the thumbnails that finished since the last look. True when one did, so the screen is
    /// drawn again: nobody presses a key for a decode to end.
    pub fn settle_previews(&mut self) -> bool {
        let mut changed = false;
        for (_, preview) in &mut self.previews {
            changed |= preview.settle();
        }
        changed
    }

    /// Stage a thumbnail directly, for a test that has no terminal to ask.
    #[cfg(test)]
    pub(crate) fn stage_preview(&mut self, marker: &str, thumb: crate::preview::Thumb) {
        self.previews
            .push((marker.to_string(), crate::preview::Preview::ready(thumb)));
    }

    /// How many pictures the line being typed still refers to, for the line beneath the box.
    pub fn pasted_count(&self) -> usize {
        self.pasted_named(&self.input).len()
    }

    /// What the line carried when it was sent, for the task being built from it.
    pub fn sent_pasted(&self) -> &[AttachedImage] {
        &self.sent_pasted
    }

    /// Every marker standing in the line for something carried beside it.
    pub(super) fn markers(&self) -> impl Iterator<Item = &str> {
        self.attached
            .iter()
            .map(|attached| attached.marker.as_str())
            .chain(self.pasted.iter().map(|pasted| pasted.marker.as_str()))
            .chain(self.pasted_text.iter().map(|pasted| pasted.marker.as_str()))
    }

    /// Where every marker in the line starts and ends.
    ///
    /// Found by looking the line up rather than by remembering a position, because the line is
    /// edited around a marker and a remembered offset would be wrong the first time somebody
    /// rewrote the sentence in front of it.
    pub(super) fn marker_spans(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.markers().flat_map(|marker| {
            self.input
                .match_indices(marker)
                .map(|(at, found)| (at, at + found.len()))
        })
    }

    /// The marker the caret would move back into, if it is against one.
    ///
    /// A caret at the very start of a marker is not against it from this side: what sits before
    /// that caret is ordinary text, and moving over it or deleting it is an ordinary press.
    pub(super) fn marker_before_caret(&self) -> Option<(usize, usize)> {
        self.marker_spans()
            .find(|&(start, end)| start < self.caret && self.caret <= end)
    }

    /// The marker the caret is on, meaning the one the next forward press would move over.
    ///
    /// A caret at the end of a marker is past it, and what lies ahead is the text after it.
    pub fn marker_at_caret(&self) -> Option<(usize, usize)> {
        self.marker_at(self.caret)
    }

    /// The marker `at` is on, on the same terms: the start of one is on it and its end is past it.
    pub(super) fn marker_at(&self, at: usize) -> Option<(usize, usize)> {
        self.marker_spans()
            .find(|&(start, end)| start <= at && at < end)
    }

    /// Take the current line as a command to carry out now.
    ///
    /// The line comes off the box with everything it named, the way [`Session::submit`] takes a
    /// prompt: the box is about to be empty, and a picture or a dropped file left staged behind an
    /// empty box is one no line can name again. What becomes of them is the caller's, since the
    /// caller is what knows which command this is.
    ///
    /// Both are read back out of the line and then cleared, which is what makes deleting a marker
    /// the way to change your mind on a command line as much as on a prompt (`dropping.md` DROP-6).
    ///
    /// A folded paste is put back to its words here, unlike a picture or a dropped file. That marker
    /// is a handle on text only the session holding it can undo, so it goes no further than the box
    /// whatever the command does with its argument.
    pub fn take_command(&mut self) -> Commanded {
        let typed = self.input.clone();
        let pasted = self.pasted_named(&typed);
        let attached = self.attachments_named(&typed);
        let line = self.unfolded(&typed);
        self.settle_staged();
        // Not through [`Session::clear_input`], which keeps a line typed mid-turn: a command carried
        // out while a turn runs (CMD-8) leaves the box as one carried out at rest does.
        self.replace_the_box(String::new(), false);
        Commanded {
            line,
            pasted,
            attached,
        }
    }

    /// The same command with everything it named put back to words and names.
    ///
    /// What a command that has nowhere to put either is carried out as. It is carried out rather than
    /// sent, so unless its argument goes to a planner there is no request for a picture to travel in:
    /// a marker left in the line would name a screenshot nothing came with, which is the same thing
    /// [`Session::resolved`] keeps a queued prompt from doing. What a command does with its argument
    /// is not something the box knows, so which commands those are is the caller's to say.
    ///
    /// A dropped file becomes its name, which is what it becomes everywhere the file itself cannot
    /// travel. A picture becomes words saying it was pasted and cannot be shown, and the person is
    /// told, because a picture is the one thing no name stands in for.
    pub fn without_what_it_cannot_carry(&mut self, commanded: Commanded) -> Commanded {
        if !commanded.pasted.is_empty() {
            self.note(t!(paste_not_with_a_command));
        }
        Commanded {
            line: put_back_to_words(
                &put_back_to_names(&commanded.line, &commanded.attached),
                &commanded.pasted,
                "a picture cannot join that command",
            ),
            pasted: Vec::new(),
            attached: Vec::new(),
        }
    }

    /// A queued line with every marker in it put back to words.
    ///
    /// What a mid-turn prompt is given to the planner as. A turn already running cannot be handed
    /// a file or a picture: routing was precommitted before it read anything, and a file admitted
    /// now would be context this turn never fixed the shape of. So what travels is text, and each
    /// marker becomes the most honest sentence available about what stood there.
    ///
    /// A paste becomes the words themselves, exactly as [`Session::unfolded`] makes it: the marker
    /// was only ever a way of drawing a long paste short.
    ///
    /// A file and a picture cannot become their contents, so they become their names. A named file
    /// is one the planner can go and read, through the same gate it reads anything else through,
    /// and a person who dropped a file mid-turn is telling it which file to look at. A picture has
    /// no such recourse and says so: better a planner that knows a screenshot was meant for it and
    /// cannot see it than one that is handed `[Image #2]` and left counting.
    pub(super) fn resolved(&self, line: &str) -> String {
        let mut resolved = self.unfolded(line);
        for attached in self.named_in(line) {
            resolved = resolved.replace(&attached.marker, &attached.name);
        }
        put_back_to_words(
            &resolved,
            &self.pasted_named(line),
            "a picture cannot join a turn already running",
        )
    }

    /// Clear the line and settle what it named.
    ///
    /// Everything still named goes; a marker the user deleted is an attachment they took off, and
    /// it goes nowhere. Pictures settle the same way and at the same moment, since a marker rubbed
    /// out means the same thing whether the thing behind it was dropped or pasted.
    pub(super) fn take_line(&mut self, prompt: &str) -> (Vec<Attached>, Vec<AttachedImage>) {
        let attached = self.attachments_named(prompt);
        let pasted = self.pasted_named(prompt);
        self.settle_staged();
        self.set_input(String::new());
        (attached, pasted)
    }

    /// Drop what was staged, except what the line put away still names.
    ///
    /// A stashed line holds only its words, and what its markers stand for stays staged beside it
    /// (INPUT-17). Sending some other line must not take that with it, or the line comes back
    /// carrying markers that name nothing.
    pub(super) fn settle_staged(&mut self) {
        let stashed = self.stashed.as_deref().unwrap_or("");
        self.attached.retain(|a| stashed.contains(&a.marker));
        self.pasted.retain(|p| stashed.contains(&p.marker));
        self.previews.retain(|(marker, _)| stashed.contains(marker));
    }
}
