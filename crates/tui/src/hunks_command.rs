//! `/hunks`: the hunks the last turn wrote, and putting one of them back.
//!
//! Which hunks a file holds, and whether one is still the hunk that was listed, are decisions taken
//! from file content, so they are taken by [`bravebot_agent::hunks`] under the gate for content the
//! map vouches for. What is here is the half that is a terminal thing: the words a person types,
//! the numbers a list is read by, and the rows it is drawn in.

use crate::state::{Entry, Session};
use bravebot_agent::hunks::{self, Left, Listing, Refused};
use bravebot_agent::lsp::LanguageServers;
use bravebot_agent::report::Activity;
use bravebot_agent::workspace::Workspace;
use bravebot_core::event::Sink;
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use std::path::Path;

/// What the argument to `/hunks` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The bare word: list the hunks the last turn wrote.
    List,
    /// Put back the hunk listed under this number.
    PutBack(usize),
    /// Anything else, answered by saying what the command takes.
    Unreadable,
}

/// Read the argument to `/hunks`.
///
/// A number is ASCII digits counting from one, as the list prints it. Anything else is unreadable
/// rather than a guess, since putting back the wrong hunk writes over the person's file.
pub fn parse(argument: &str) -> Asked {
    let argument = argument.trim();
    if argument.is_empty() {
        return Asked::List;
    }
    if !argument.bytes().all(|byte| byte.is_ascii_digit()) {
        return Asked::Unreadable;
    }
    match argument.parse::<usize>() {
        Ok(number) if number > 0 => Asked::PutBack(number),
        _ => Asked::Unreadable,
    }
}

/// What the last list held, and the turn it was a list of.
///
/// Kept so a number typed afterwards means the hunk the person read beside it. The turn is checked
/// when a number is used, so a list read before another turn is not applied to that one.
#[derive(Debug, Clone)]
pub struct Listed {
    turn: usize,
    listing: Listing,
}

/// List the hunks the last turn wrote, in the transcript, and remember the list.
///
/// The list before is forgotten first, so a list that could not be drawn again leaves no numbers
/// to be read against what it described.
pub fn list<S: Sink>(
    session: &mut Session,
    workspace: &Workspace,
    trust: &TrustStore,
    sink: &mut S,
) {
    session.hunks_listed = None;
    session.rewind.bind_coverage(workspace);
    let Some(point) = session.rewind.points().last() else {
        session.note(t!(session_nothing_to_undo));
        return;
    };
    let turn = point.snapshot.turns;
    let gaps = point.coverage.gaps();
    let listing = hunks::list(
        workspace,
        &point.backups,
        &point.snapshot.trust,
        trust,
        sink,
    );
    match listing {
        Ok(listing) => {
            show(session, workspace, &listing, turn);
            // What the file tools did not account for is not in the list, and the list reads as
            // everything the turn changed unless it says so.
            if !gaps.is_empty() {
                session.note(t!(
                    session_hunks_uncovered,
                    causes = crate::app::coverage_causes(&gaps)
                ));
            }
            session.hunks_listed = Some(Listed { turn, listing });
        }
        Err(refused) => session.note(refusal(&refused, 0, "")),
    }
}

/// The path as the list names it: as the workspace names it, and with a control character in a
/// file name made visible, since the transcript draws a note and a call's heading as they are and
/// a name the turn chose could otherwise move the cursor.
fn named(workspace: &Workspace, path: &Path) -> String {
    crate::render::printable(&workspace.relative_display(path))
}

/// Draw a list: what was found, one row for each hunk, and a line for each path with none.
fn show(session: &mut Session, workspace: &Workspace, listing: &Listing, turn: usize) {
    if listing.files.is_empty() && listing.left.is_empty() {
        session.note(t!(session_hunks_wrote_nothing, turn = turn + 1));
        return;
    }
    if !listing.files.is_empty() {
        session.note(t!(session_hunks_heading, turn = turn + 1));
    }
    let mut number = 0;
    for file in &listing.files {
        for hunk in &file.hunks {
            number += 1;
            let removed = t!(session_hunk_removed, count = hunk.before.len());
            // A hunk that only removes lines has none in the file now to start at, so the line
            // before it places it.
            let row = match (hunk.after.is_empty(), hunk.after.start) {
                (true, 0) => t!(session_hunk_row_at_start, removed = removed),
                (true, before) => t!(session_hunk_row_after, line = before, removed = removed),
                (false, start) => t!(
                    session_hunk_row,
                    line = start + 1,
                    added = t!(session_hunk_added, count = hunk.after.len()),
                    removed = removed
                ),
            };
            session.transcript.push(Entry::tool(
                Activity::running("Hunk", format!("{number} {}", named(workspace, &file.path)))
                    .done(row)
                    .with_changes(hunk.drawn()),
            ));
        }
    }
    for (path, left) in &listing.left {
        session.note(t!(
            session_hunks_left_row,
            path = named(workspace, path),
            why = why_left(*left)
        ));
    }
    if listing.unlisted > 0 {
        session.note(t!(session_hunks_more, count = listing.unlisted));
    }
}

/// What came of trying to put a hunk back: the line to say, and whether a hunk went back.
struct Attempt {
    said: String,
    done: bool,
}

impl Attempt {
    fn said(said: impl Into<String>) -> Self {
        Self {
            said: said.into(),
            done: false,
        }
    }
}

/// Put back the hunk of the last list that is listed under `number`.
fn attempt<S: Sink>(
    session: &Session,
    number: usize,
    workspace: &Workspace,
    trust: &mut TrustStore,
    servers: &mut Option<LanguageServers>,
    home: Option<&Path>,
    sink: &mut S,
) -> Attempt {
    let Some(listed) = &session.hunks_listed else {
        return Attempt::said(t!(session_hunks_list_first));
    };
    let Some(point) = session.rewind.points().last() else {
        return Attempt::said(t!(session_nothing_to_undo));
    };
    if point.snapshot.turns != listed.turn {
        return Attempt::said(t!(session_hunks_turn_moved));
    }
    let Some((file, hunk)) = listed.listing.numbered(number) else {
        return Attempt::said(t!(
            session_hunk_no_such,
            number = number,
            hunks = listed.listing.hunks()
        ));
    };
    let Some(backup) = point.backups.iter().find(|backup| backup.path == file.path) else {
        return Attempt::said(t!(session_hunks_turn_moved));
    };
    let path = named(workspace, &file.path);
    let result = hunks::reject(
        workspace,
        backup,
        hunk,
        &point.snapshot.trust,
        trust,
        servers,
        home,
        sink,
    );
    match result {
        Ok(()) => Attempt {
            said: t!(
                session_hunk_put_back,
                number = number,
                path = path,
                turn = listed.turn + 1
            ),
            done: true,
        },
        Err(refused) => Attempt::said(refusal(&refused, number, &path)),
    }
}

/// Put back the hunk listed under `number`, and list what is left.
///
/// Says what happened either way. A hunk that was not put back says why in a line, since a person
/// who asked for one and saw nothing would take it for done. The list is drawn again after a hunk
/// goes back, because the numbers below it have moved.
pub fn put_back<S: Sink>(
    session: &mut Session,
    number: usize,
    workspace: &Workspace,
    trust: &mut TrustStore,
    servers: &mut Option<LanguageServers>,
    home: Option<&Path>,
    sink: &mut S,
) {
    let Attempt { said, done } = attempt(session, number, workspace, trust, servers, home, sink);
    session.note(said);
    if done {
        list(session, workspace, trust, sink);
    }
}

/// The line saying why a hunk was not put back.
fn refusal(refused: &Refused, number: usize, path: &str) -> String {
    match refused {
        Refused::Left(left) => t!(
            session_hunk_not_put_back,
            number = number,
            path = path,
            why = why_left(*left)
        ),
        Refused::Changed => t!(session_hunk_changed, number = number, path = path),
        Refused::Io(problem) => t!(session_hunk_failed, path = path, problem = problem),
        Refused::Policy(problem) => t!(session_hunks_unavailable, problem = problem),
    }
}

/// Why a path has no hunks to put back.
fn why_left(left: Left) -> String {
    match left {
        Left::Created => t!(session_hunks_left_created).to_string(),
        Left::NotKept => t!(session_hunks_left_not_kept).to_string(),
        Left::Untrusted => t!(session_hunks_left_untrusted).to_string(),
        Left::NotText => t!(session_hunks_left_not_text).to_string(),
        Left::TooDifferent => t!(session_hunks_left_too_different).to_string(),
        Left::Unreadable => t!(session_hunks_left_unreadable).to_string(),
        Left::Outside => t!(session_hunks_left_outside).to_string(),
        Left::AsItWas => t!(session_hunks_left_as_it_was).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file name can hold anything but a slash, and the turn chose it. Drawn as it is, an escape
    /// in it would act on the terminal the list is read in.
    #[cfg(unix)]
    #[test]
    fn a_file_name_with_an_escape_in_it_is_drawn_with_the_escape_made_visible() {
        if !crate::test_profile::in_isolated_profile() {
            return;
        }
        let root = crate::test_profile::project("hunks-escaped-name");
        std::fs::create_dir_all(&root).unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let path = workspace.root().join("a\u{1b}[2Jb.txt");

        let drawn = named(&workspace, &path);

        assert_eq!(drawn, "a\u{241b}[2Jb.txt");
        assert!(!drawn.chars().any(char::is_control));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_bare_word_lists_and_a_number_names_a_hunk() {
        assert_eq!(parse(""), Asked::List);
        assert_eq!(parse("  "), Asked::List);
        assert_eq!(parse("2"), Asked::PutBack(2));
        assert_eq!(parse(" 12 "), Asked::PutBack(12));
    }

    /// A line that is not one number is not a guess at one: `0` and `-1` name no hunk, `1 2` would
    /// be two, and `+1` and a number in another script are not the digits a list prints.
    #[test]
    fn anything_but_one_number_is_unreadable() {
        for argument in [
            "0",
            "-1",
            "+1",
            "1 2",
            "two",
            "1x",
            "\u{661}",
            "99999999999999999999999999",
        ] {
            assert_eq!(parse(argument), Asked::Unreadable, "{argument:?}");
        }
    }
}
