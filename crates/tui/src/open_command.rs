//! `/open`: the keyboard's way to what a click on a link in the assistant's words does (CMD-30).
//!
//! A click on a web address or a file path acts on cells a person can see. A line typed in the box
//! cannot point at cells, so `/open` lists the links of the latest reply that has any and says what
//! number each is, and `/open <number>` does what a click on that link does. What it lists is found
//! as a click finds a link ([`crate::text_links::scan`]), and what it does with one is carried out
//! by the code a click's action is carried out by. Nothing here reads a reply: the list is made of
//! the text the frame was drawn from and the person reads it as drawn.

use crate::app::Action;
use crate::state::{Session, Status};
use crate::text_links::{PathRef, Target};
use bravebot_i18n::t;

/// The word before a number that opens the file of a path in the person's editor, as a click with
/// ctrl held on the path does, in place of showing it.
pub const EDIT: &str = "edit";

/// The most links `/open` lists, and so the most a number can name. A reply holds as many as the
/// assistant wrote, and a line in the transcript for each of thousands is no list a person reads. A
/// link past it is opened by typing it after the command.
pub const MOST_LISTED: usize = 50;

/// What the argument to `/open` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The bare word: list the links of the latest reply that has any.
    List,
    /// Open the link the last list numbered so.
    Number(usize),
    /// Open the file of the path the last list numbered so in the person's editor.
    Edit(usize),
    /// Open this address or show this file, as the person typed it.
    Typed(String),
    /// Open the file this path names in the person's editor, as the person typed it.
    EditTyped(String),
    /// `edit` with nothing to open, answered by saying what the command takes.
    Unreadable,
}

/// Read the argument to `/open`.
///
/// A number is digits and nothing else, so no word is taken for one, and a run of digits too large for
/// a `usize` is not one: it is a name, as any text is that is not a number and not `edit`. What the
/// person typed is taken as it was typed, with the blanks around it trimmed (CMD-5), and is the
/// address or the path that is opened: nothing here reads it, and what reads it is the check an
/// address passes and the rules a name is resolved by, which are a click's.
pub fn parse(argument: &str) -> Asked {
    let argument = argument.trim();
    if argument.is_empty() {
        return Asked::List;
    }
    if let Some(number) = number(argument) {
        return Asked::Number(number);
    }
    match argument.split_once(char::is_whitespace) {
        Some((EDIT, rest)) => {
            let rest = rest.trim();
            match number(rest) {
                Some(number) => Asked::Edit(number),
                None => Asked::EditTyped(rest.to_string()),
            }
        }
        None if argument == EDIT => Asked::Unreadable,
        _ => Asked::Typed(argument.to_string()),
    }
}

/// `text` as a number: digits and nothing else, and small enough to be one.
fn number(text: &str) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

impl Session {
    /// List the links of the latest reply that has any, one line each, and keep what was listed.
    ///
    /// What a number typed afterwards names is the link it was printed beside, so the list is kept
    /// as it was said and not looked for again, as `/hunks` keeps its list.
    pub fn list_links(&mut self) {
        let found = crate::render::links_of_the_latest_reply(self);
        if found.is_empty() {
            self.links_listed = Some(Vec::new());
            self.note(t!(open_none));
            return;
        }
        self.note(t!(open_listed, count = found.len()));
        for (at, link) in found.iter().take(MOST_LISTED).enumerate() {
            self.note(t!(
                open_item,
                number = at + 1,
                link = crate::render::printable(&link.drawn)
            ));
        }
        if found.len() > MOST_LISTED {
            self.note(t!(open_more, count = found.len() - MOST_LISTED));
        }
        // Only what was printed, so a number names a link the person was shown.
        self.links_listed = Some(
            found
                .into_iter()
                .take(MOST_LISTED)
                .map(|link| link.target)
                .collect(),
        );
    }

    /// The links the last `/open` listed, in the order it numbered them.
    pub fn listed_links(&self) -> &[Target] {
        self.links_listed.as_deref().unwrap_or_default()
    }

    /// What `/open <number>` asks for: what a click on the link the last list numbered so asks for
    /// without a modifier, or a note saying why there is none.
    ///
    /// An address is handed on as the action a click on it gives, and a path as the one that shows
    /// the file, so both are carried out by the code a click's are. A file is shown only where the
    /// click on a path acts ([`Session::shows_a_file_here`]).
    pub(crate) fn open_listed_link(&mut self, number: usize) -> Action {
        match self.link_numbered(number) {
            Some(Target::Address(address)) => Action::OpenUrl(address),
            Some(Target::Path(path)) => self.open_path(path, false),
            None => Action::Redraw,
        }
    }

    /// What `/open edit <number>` asks for: what a click with ctrl held on the path the last list
    /// numbered so asks for, which is the file opened in the person's editor at the line the path
    /// gave, or a note saying why there is none. An address is no file and is refused with a note,
    /// since the browser is what opens it.
    ///
    /// The editor takes the terminal, so it is asked for where a click on a path acts and not while a
    /// turn is drawing on it ([`Session::shows_a_file_here`]).
    pub(crate) fn edit_listed_link(&mut self, number: usize) -> Action {
        match self.link_numbered(number) {
            Some(Target::Path(path)) => self.open_path(path, true),
            Some(Target::Address(_)) => {
                self.note(t!(open_edit_an_address));
                Action::Redraw
            }
            None => Action::Redraw,
        }
    }

    /// The link the last list numbered `number`, or a note saying why there is none: no list was
    /// made since the session began, since `/clear` or since another turn began, or the list has no
    /// link of that number.
    fn link_numbered(&mut self, number: usize) -> Option<Target> {
        let Some(listed) = &self.links_listed else {
            self.note(t!(open_list_first));
            return None;
        };
        let link = number.checked_sub(1).and_then(|at| listed.get(at)).cloned();
        if link.is_none() {
            let said = t!(open_no_such, number = number, links = listed.len());
            self.note(said);
        }
        link
    }

    /// What `/open <address>`, `/open <path>` and the same after `edit` ask for, with the text as it
    /// was typed: the person typing the exact field is the endorsement, as typing `@path` is.
    ///
    /// Text with `://` in it is an address and passes the check a click's address passes, and is
    /// refused where it does not. Any other text is a path: read as a click reads the place after a
    /// name (`:12`, `:12:5`, `:9-12`) where it has one, and whole otherwise, so a name with no
    /// extension, or with a blank in it, can be typed where it could not be clicked. The name is
    /// resolved by the action a click's path is carried out by, which refuses it in the same words.
    pub(crate) fn open_typed(&mut self, typed: &str, edit: bool) -> Action {
        if typed.contains("://") {
            return match (crate::opener::is_openable(typed), edit) {
                (true, false) => Action::OpenUrl(typed.to_string()),
                (true, true) => {
                    self.note(t!(open_edit_an_address));
                    Action::Redraw
                }
                (false, _) => {
                    self.note(t!(open_not_an_address));
                    Action::Redraw
                }
            };
        }
        let path = crate::text_links::path_of(typed).unwrap_or_else(|| PathRef {
            path: typed.to_string(),
            line: None,
            column: None,
        });
        self.open_path(path, edit)
    }

    /// The action that shows the file of `path`, or opens it in the editor, where a click on a path
    /// acts, and a note saying why not elsewhere.
    fn open_path(&mut self, path: PathRef, edit: bool) -> Action {
        match (self.shows_a_file_here(), edit) {
            (true, false) => Action::Preview(path),
            (true, true) => Action::EditFile(path),
            (false, false) => {
                self.note(t!(open_not_while_working));
                Action::Redraw
            }
            (false, true) => {
                self.note(t!(open_edit_not_while_working));
                Action::Redraw
            }
        }
    }

    /// Whether a file named in the transcript is shown or opened here: where nothing is running that
    /// is drawing at the end of the transcript or has the terminal, and not in a delegate's view,
    /// where a name may be another file than it is here (CHECKOUT-7). The click on a path and the
    /// command that stands for it ask this and no second question.
    pub(crate) fn shows_a_file_here(&self) -> bool {
        self.status == Status::Idle && self.watching().is_none()
    }
}
