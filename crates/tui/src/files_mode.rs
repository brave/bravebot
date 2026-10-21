//! The info panel's files state: the files the session wrote and every edit to the highlighted one
//! ([FILES-1](../../../docs/specs/files-mode.md#FILES-1) onward).
//!
//! Everything here is read from the activity entries the transcript already holds. Nothing is read
//! from disk, run or sent to a model, and nothing is kept in a second place, so a bound on those
//! entries is a bound here.

use crate::state::{Entry, Speaker};
use crate::theme;
use bravebot_agent::diff::{Change, Numbered};
use bravebot_agent::report::{Activity, Written};
use bravebot_i18n::t;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// The fewest columns the state is drawn in, border included.
const NARROWEST_STATE: u16 = 60;

/// The most columns the state is drawn in, border included.
const WIDEST_STATE: u16 = 100;

/// The columns the state takes on a terminal `columns` wide: half of them, held between 60 and 100
/// (FILES-4). Not the width the panel was dragged to, which the ordinary state keeps.
pub fn columns(columns: u16) -> u16 {
    (columns / 2).clamp(NARROWEST_STATE, WIDEST_STATE)
}

/// One finished write call: what it did to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// The prompt this edit was made under, counted from one.
    pub turn: usize,
    /// Lines the edit added, counted by the kernel with the diff.
    pub added: usize,
    /// Lines the edit removed, counted by the kernel with the diff.
    pub removed: usize,
    /// The changes the transcript draws for the write.
    pub changes: Vec<Change>,
    /// The line numbers of each change, as the kernel made them.
    pub numbers: Vec<Numbered>,
    /// Whether the file the edit replaced was not vouched for (FILES-7).
    pub untrusted: bool,
}

/// A file the session wrote, with each edit to it in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// The path as the call row of the first write to the file draws it.
    pub path: String,
    /// Every edit, oldest first.
    pub edits: Vec<Edit>,
}

impl File {
    /// Lines added by every edit to the file, summed from the counts the edits carry.
    pub fn added(&self) -> usize {
        self.edits.iter().map(|edit| edit.added).sum()
    }

    /// Lines removed by every edit to the file, summed from the counts the edits carry.
    pub fn removed(&self) -> usize {
        self.edits.iter().map(|edit| edit.removed).sum()
    }
}

/// The finished write calls of the transcript, oldest first, each with the turn it was made in: the
/// number of prompts the transcript holds before it.
///
/// A call that was refused, failed or has not finished carries no [`bravebot_agent::report::Written`]
/// and so is not an edit. A write a program made, and one the person made, never had a call to draw
/// from.
fn calls<'a>(
    transcript: impl IntoIterator<Item = &'a Entry>,
) -> impl Iterator<Item = (usize, &'a Activity, &'a Written)> {
    let mut turn = 0;
    transcript.into_iter().filter_map(move |entry| {
        if entry.speaker == Speaker::User {
            turn += 1;
        }
        let activity = entry.activity.as_ref()?;
        let written = activity.written.as_ref()?;
        (!activity.failed && !activity.is_running()).then_some((turn, activity, written))
    })
}

/// The files the transcript's finished write calls changed, in the order each was first written
/// (FILES-1).
pub fn collect<'a>(transcript: impl IntoIterator<Item = &'a Entry>) -> Vec<File> {
    let mut files: Vec<File> = Vec::new();
    let mut at: HashMap<&Path, usize> = HashMap::new();
    for (turn, activity, written) in calls(transcript) {
        let edit = Edit {
            turn,
            added: written.added,
            removed: written.removed,
            changes: activity.changes.clone(),
            numbers: written.numbers.clone(),
            untrusted: activity.untrusted,
        };
        match at.get(written.landed.as_path()) {
            Some(&index) => files[index].edits.push(edit),
            None => {
                at.insert(&written.landed, files.len());
                files.push(File {
                    path: activity.target.clone(),
                    edits: vec![edit],
                });
            }
        }
    }
    files
}

/// How many files the transcript's write calls changed and the lines they added and removed, summed
/// from the counts the calls carry. The Files section of the ordinary panel draws this on every
/// frame, so it copies none of the changes [`collect`] does.
pub fn totals<'a>(transcript: impl IntoIterator<Item = &'a Entry>) -> (usize, usize, usize) {
    let mut paths: HashSet<&Path> = HashSet::new();
    let (mut added, mut removed) = (0, 0);
    for (_, _, written) in calls(transcript) {
        paths.insert(&written.landed);
        added += written.added;
        removed += written.removed;
    }
    (paths.len(), added, removed)
}

/// Whether the transcript holds a call from before this session began, which holds no changes to
/// list (FILES-10).
pub fn has_earlier_calls<'a>(transcript: impl IntoIterator<Item = &'a Entry>) -> bool {
    transcript
        .into_iter()
        .any(|entry| entry.speaker == Speaker::Tool && entry.activity.is_none())
}

/// What the reader has done in the state: the file highlighted and how far its edits are scrolled.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    /// The highlighted file, as an index into [`collect`]'s list.
    pub selected: usize,
    /// The first row of the highlighted file's edits that is drawn.
    pub scroll: usize,
    /// How many files the list held when the reader last pressed a key, so that a file arriving
    /// after it is said to have arrived (FILES-8).
    pub known: usize,
}

/// The rows each region of a panel `height` rows tall has: the list, and the edits under it.
///
/// A heading row, a rule and the highlighted file's name take three rows, and the list takes at most
/// a third of what is left, so a long list leaves the edits most of the room.
pub fn regions(height: usize, files: usize) -> (usize, usize) {
    let left = height.saturating_sub(3);
    let list = files.min((left / 3).max(1));
    (list, left.saturating_sub(list))
}

/// The rows of a list `files` long drawn in `room` rows around the highlight `at`: for each row,
/// the file it holds, or how many are left out beyond it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    File(usize),
    /// Files left out above, in the first row.
    Above(usize),
    /// Files left out below, in the last row.
    Below(usize),
}

/// The rows of the list. Where there are more files than rows, the ones around the highlight, with
/// the first and last rows saying how many are left out (FILES-5).
pub fn window(files: usize, room: usize, at: usize) -> Vec<Row> {
    if room == 0 || files == 0 {
        return Vec::new();
    }
    let at = at.min(files - 1);
    if files <= room {
        return (0..files).map(Row::File).collect();
    }
    if room < 3 {
        return vec![Row::File(at)];
    }
    // The highlight stays off the rows that give way to a count.
    let mut start = at.saturating_sub(room / 2).min(files - room);
    loop {
        let end = start + room;
        if start > 0 && at == start {
            start -= 1;
        } else if end < files && at + 1 == end {
            start += 1;
        } else {
            break;
        }
    }
    let end = start + room;
    (start..end)
        .map(|index| {
            if start > 0 && index == start {
                Row::Above(start + 1)
            } else if end < files && index + 1 == end {
                Row::Below(files - index)
            } else {
                Row::File(index)
            }
        })
        .collect()
}

fn dim() -> Style {
    Style::default().fg(theme::muted())
}

fn added_style() -> Style {
    Style::default().fg(theme::ok())
}

fn removed_style() -> Style {
    Style::default().fg(theme::fail())
}

/// `+{n}` and `-{n}` for the counts of a file or of the session, each in the colour of the rows it
/// counts and left out where it is nothing.
fn counts(added: usize, removed: usize) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if added > 0 {
        spans.push(Span::styled(format!("+{added}"), added_style()));
    }
    if added > 0 && removed > 0 {
        spans.push(Span::raw(" "));
    }
    if removed > 0 {
        spans.push(Span::styled(format!("-{removed}"), removed_style()));
    }
    spans
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|span| crate::wrap::display_width(&span.content))
        .sum()
}

/// The heading row: how many files changed and the totals, with a note where the session was
/// resumed and earlier edits are not kept.
fn heading(files: &[File], earlier: bool, width: usize) -> Line<'static> {
    let added: usize = files.iter().map(File::added).sum();
    let removed: usize = files.iter().map(File::removed).sum();
    let mut spans = vec![Span::styled(
        format!(" {}", t!(files_heading, count = files.len())),
        Style::default()
            .fg(theme::brand_primary())
            .add_modifier(Modifier::BOLD),
    )];
    let totals = counts(added, removed);
    if !totals.is_empty() {
        spans.push(Span::raw("  "));
        spans.extend(totals);
    }
    if earlier {
        let used = spans_width(&spans);
        let note = format!("  {}", t!(files_earlier_not_kept));
        let room = width.saturating_sub(used);
        spans.push(Span::styled(crate::status::cut(&note, room), dim()));
    }
    Line::from(spans)
}

/// One row of the list: the path cut from the left to the room the counts leave, then the counts at
/// the right edge.
fn list_row(file: &File, highlighted: bool, width: usize) -> Line<'static> {
    let counted = counts(file.added(), file.removed());
    let taken = spans_width(&counted);
    let room = width.saturating_sub(taken + 3);
    let path = crate::panel::ending(&crate::render::printable(&file.path), room);
    let gap = width.saturating_sub(crate::wrap::display_width(&path) + taken + 2);
    let mark = if highlighted { "▸" } else { " " };
    let style = if highlighted {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(mark.to_string(), style),
        Span::styled(path, style),
        Span::raw(" ".repeat(gap)),
    ];
    spans.extend(counted);
    Line::from(spans)
}

/// The rows the state shows of the list, `room` of them.
fn list_rows(files: &[File], state: &State, room: usize, width: usize) -> Vec<Line<'static>> {
    window(files.len(), room, state.selected)
        .into_iter()
        .map(|row| match row {
            Row::File(index) => list_row(&files[index], index == state.selected, width),
            Row::Above(count) => Line::from(Span::styled(
                format!(" {}", t!(files_more_above, count = count)),
                dim(),
            )),
            Row::Below(count) => {
                let new = files.len().saturating_sub(state.known);
                let said = if new > 0 {
                    t!(files_more_below_new, count = count, new = new)
                } else {
                    t!(files_more_below, count = count)
                };
                Line::from(Span::styled(format!(" {said}"), dim()))
            }
        })
        .collect()
}

/// The edits to one file as rows, and the row each edit's header is on.
///
/// Every change is drawn, and none is cut to a count (FILES-6). A line wider than the panel wraps,
/// and its continuation rows have a blank gutter and sign. An edit made over a file nobody vouched
/// for is drawn behind the bar on every row, and its lines are neutralised and broken to the width
/// by the step that draws the margin (FILES-7), so a line cannot paint a bar of its own.
pub fn edit_rows(file: &File, width: usize) -> (Vec<Line<'static>>, Vec<usize>) {
    let numbered = file
        .edits
        .iter()
        .flat_map(|edit| edit.numbers.iter())
        .flat_map(|numbers| [numbers.before, numbers.after])
        .flatten()
        .max()
        .unwrap_or(0);
    let gutter = numbered.to_string().len().max(2);
    // The margin, the gutter and its space, then the sign and its space.
    let fixed = 2 + gutter + 1 + 2;
    let mut rows = Vec::new();
    let mut headers = Vec::new();
    let total = file.edits.len();
    for (index, edit) in file.edits.iter().enumerate() {
        headers.push(rows.len());
        let mut header = vec![Span::styled(
            format!(
                " {}",
                t!(
                    files_edit_header,
                    index = index + 1,
                    total = total,
                    turn = edit.turn
                )
            ),
            Style::default().add_modifier(Modifier::BOLD),
        )];
        let counted = counts(edit.added, edit.removed);
        if !counted.is_empty() {
            header.push(Span::raw("  "));
            header.extend(counted);
        }
        rows.push(Line::from(header));

        let margin = if edit.untrusted {
            Span::styled(
                format!(" {}", crate::render::QUARANTINE_BAR),
                Style::default().fg(theme::running()),
            )
        } else {
            Span::raw("  ")
        };
        for (position, change) in edit.changes.iter().enumerate() {
            let at = edit.numbers.get(position).copied().unwrap_or_default();
            let (number, sign, text, style) = match change {
                Change::Added(text) => (at.after, "+", text.clone(), added_style()),
                Change::Removed(text) => (at.before, "-", text.clone(), removed_style()),
                Change::Kept(text) => (at.after, " ", text.clone(), dim()),
                Change::Elided(count) => (
                    None,
                    " ",
                    t!(transcript_unchanged, count = *count).to_string(),
                    dim(),
                ),
            };
            let drawn = crate::render::marked_rows(
                &margin,
                &[Span::styled(text, style)],
                width.saturating_sub(fixed - 2),
            );
            for (line, mut row) in drawn.into_iter().enumerate() {
                let (shown, mark) = if line == 0 {
                    (number.map(|n| n.to_string()).unwrap_or_default(), sign)
                } else {
                    (String::new(), " ")
                };
                // After the margin, which `marked_rows` put at the head of the row.
                row.spans.insert(
                    1,
                    Span::styled(
                        format!("{shown:>gutter$} {mark} "),
                        dim().patch(style_of(sign)),
                    ),
                );
                rows.push(row);
            }
        }
    }
    (rows, headers)
}

/// The colour of a sign, so the gutter reads as part of its row.
fn style_of(sign: &str) -> Style {
    match sign {
        "+" => added_style(),
        "-" => removed_style(),
        _ => Style::default(),
    }
}

/// The state's rows: the heading, the list, a rule, the highlighted file's name and edit count, and
/// its edits, `height` rows in all (FILES-4).
pub fn rows(
    files: &[File],
    state: &State,
    earlier: bool,
    width: u16,
    height: u16,
) -> Vec<Line<'static>> {
    let height = usize::from(height);
    // The border takes a column.
    let width = usize::from(width).saturating_sub(1).max(1);
    let (list, room) = regions(height, files.len());
    let mut body = vec![heading(files, earlier, width)];
    body.extend(list_rows(files, state, list, width));
    body.push(Line::from(Span::styled("─".repeat(width), dim())));
    let selected = state.selected.min(files.len().saturating_sub(1));
    if let Some(file) = files.get(selected) {
        let name = crate::panel::ending(
            &crate::render::printable(&file.path),
            width.saturating_sub(2 + 12),
        );
        body.push(Line::from(vec![
            Span::styled(
                format!(" {name}"),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", t!(files_edit_count, count = file.edits.len())),
                dim(),
            ),
        ]));
        let (edits, _) = edit_rows(file, width);
        let first = state.scroll.min(edits.len().saturating_sub(room));
        body.extend(edits.into_iter().skip(first).take(room));
    }
    body.truncate(height);
    body.resize(height, Line::raw(""));
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Action, handle_key};
    use crate::state::Session;
    use bravebot_agent::diff::Diff;
    use bravebot_agent::report::{Activity, Written};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// A finished write call over `before` and `after`, with what the kernel makes of the diff.
    fn write(path: &str, before: &str, after: &str) -> Activity {
        let diff = Diff::compute(before, after);
        Activity::running("Update", path)
            .of_tool("edit_file")
            .with_changes(diff.condensed(3))
            .with_written(Some(Written {
                landed: path.into(),
                added: diff.added(),
                removed: diff.removed(),
                numbers: diff.condensed_numbers(3),
            }))
            .done("done")
    }

    fn wrote(session: &mut Session, activity: Activity) {
        session.transcript.push(Entry::tool(activity));
    }

    fn prompt(session: &mut Session, text: &str) {
        session.transcript.push(Entry::user(text));
    }

    /// A session at its first prompt with each of `paths` written once.
    fn session_with(paths: &[&str]) -> Session {
        let mut session = Session::new("none");
        prompt(&mut session, "change things");
        for path in paths {
            wrote(
                &mut session,
                write(path, "one\ntwo\n", "one\nthree\nfour\n"),
            );
        }
        session
    }

    /// The frame drawn `width` by `height`, one string a row, with its layout recorded on the
    /// session the way the event loop records it after every draw.
    fn screen(session: &mut Session, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let mut laid = None;
        terminal
            .draw(|frame| laid = Some(crate::render::draw(frame, session)))
            .expect("draw succeeds");
        session.note_layout(laid.expect("the frame was drawn"));
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    /// The panel's rows of the frame, border left off and trailing space trimmed, with the hint
    /// line below them.
    fn panel_of(session: &mut Session, width: u16, height: u16) -> Vec<String> {
        let rows = screen(session, width, height);
        let start = usize::from(width - session.panel_columns(width)) + 1;
        rows[..rows.len() - 1]
            .iter()
            .map(|row| {
                row.chars()
                    .skip(start)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// Open the state on a terminal wide enough for it.
    fn opened(session: &mut Session) {
        screen(session, 120, 30);
        session.toggle_files();
        assert!(session.files_open(), "the state did not open");
        screen(session, 120, 30);
    }

    fn press(session: &mut Session, code: KeyCode) -> Action {
        handle_key(session, KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn text_of(lines: &[Line<'static>]) -> Vec<String> {
        lines.iter().map(|line| line.to_string()).collect()
    }

    /// FILES-1: a finished write call lists its file, and `a.rs` is one row with one edit.
    #[test]
    fn a_file_is_listed_once_a_write_call_has_changed_it() {
        let session = session_with(&["a.rs"]);
        let files = session.written_files();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a.rs");
        assert_eq!(files[0].edits.len(), 1);
        assert_eq!((files[0].added(), files[0].removed()), (2, 1));
    }

    /// FILES-1: a call that was refused or failed, one still running, and one that read a file
    /// list nothing, so a file only they named has no row.
    #[test]
    fn a_refused_failed_or_running_write_lists_nothing() {
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        let done = write("a.rs", "x\n", "y\n");
        let written = done.written.clone();
        let mut failed = Activity::running("Update", "failed.rs")
            .of_tool("edit_file")
            .failed("refused");
        failed.written = written.clone();
        let mut running = Activity::running("Update", "running.rs").of_tool("edit_file");
        running.written = written;
        wrote(&mut session, failed);
        wrote(&mut session, running);
        wrote(
            &mut session,
            Activity::running("Read", "read.rs")
                .of_tool("read_file")
                .done("1 line"),
        );
        assert!(session.written_files().is_empty());
    }

    /// FILES-1: writing a file again keeps its row, in the place it first had, and adds an edit.
    #[test]
    fn a_second_write_keeps_the_files_row_and_adds_an_edit() {
        let mut session = session_with(&["a.rs", "b.rs"]);
        prompt(&mut session, "again");
        wrote(&mut session, write("a.rs", "p\n", "q\n"));
        let files = session.written_files();
        let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, ["a.rs", "b.rs"]);
        assert_eq!(files[0].edits.len(), 2);
        assert_eq!(
            files[0]
                .edits
                .iter()
                .map(|edit| edit.turn)
                .collect::<Vec<_>>(),
            [1, 2]
        );
    }

    /// FILES-1: rows are keyed on the file a call landed on and not on the text of its row, so two
    /// files drawn under one name stay two, and two names for one file are one row.
    #[test]
    fn rows_are_keyed_on_the_file_a_call_landed_on() {
        let landing = |mut call: Activity, landed: &str| {
            let written = call.written.as_mut().expect("a write carries counts");
            written.landed = landed.into();
            call
        };
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(
            &mut session,
            landing(write("a.rs", "x\n", "y\n"), "/work/one/a.rs"),
        );
        wrote(
            &mut session,
            landing(write("a.rs", "x\n", "y\n"), "/work/two/a.rs"),
        );
        wrote(
            &mut session,
            landing(write("./a.rs", "p\n", "q\n"), "/work/one/a.rs"),
        );
        let files = session.written_files();
        assert_eq!(files.len(), 2, "{files:#?}");
        assert_eq!(files[0].edits.len(), 2);
        assert_eq!(files[1].edits.len(), 1);
        assert_eq!(session.written_totals().0, 2);
    }

    /// FILES-1: the key is the file's own bytes, so two names that differ only in bytes a lossy
    /// conversion folds together stay two rows.
    #[cfg(unix)]
    #[test]
    fn rows_keyed_on_a_name_that_is_not_utf8_stay_apart_from_its_lossy_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let landing = |mut call: Activity, landed: &OsStr| {
            call.written
                .as_mut()
                .expect("a write carries counts")
                .landed = landed.into();
            call
        };
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(
            &mut session,
            landing(write("a", "x\n", "y\n"), OsStr::from_bytes(b"/w/a-\xff")),
        );
        wrote(
            &mut session,
            landing(write("a", "x\n", "y\n"), OsStr::new("/w/a-\u{FFFD}")),
        );
        assert_eq!(session.written_files().len(), 2);
        assert_eq!(session.written_totals().0, 2);
    }

    /// FILES-1: one call that changed several places of a file is one edit, drawn under one header.
    #[test]
    fn several_passages_of_one_edit_call_are_one_edit() {
        let before: String = (1..=30).map(|n| format!("line {n}\n")).collect();
        let after = before
            .replace("line 3\n", "three\n")
            .replace("line 27\n", "twenty-seven\n");
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, write("big.rs", &before, &after));
        let files = session.written_files();
        assert_eq!(files[0].edits.len(), 1);
        let (rows, headers) = edit_rows(&files[0], 58);
        assert_eq!(headers, [0]);
        assert!(
            text_of(&rows)
                .iter()
                .any(|row| row.contains("twenty-seven"))
        );
    }

    /// FILES-2: the heading and the section hold the sum of the counts the edits carry, which here
    /// differ from the rows the changes have, so a count of rows would give another figure.
    #[test]
    fn the_totals_are_sums_of_the_counts_the_edits_carry() {
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        for (path, added, removed) in [("a.rs", 7, 2), ("b.rs", 5, 0)] {
            let mut call = write(path, "x\n", "y\n");
            call.written = Some(Written {
                added,
                removed,
                ..call.written.clone().unwrap()
            });
            wrote(&mut session, call);
        }
        opened(&mut session);
        let panel = panel_of(&mut session, 120, 30);
        assert!(panel[0].contains("2 files changed"), "{panel:#?}");
        assert!(
            panel[0].contains("+12") && panel[0].contains("-2"),
            "{panel:#?}"
        );
        assert!(
            panel[1..3].iter().any(|row| row.contains("+7 -2"))
                && panel[1..3].iter().any(|row| row.trim_end().ends_with("+5")),
            "{panel:#?}"
        );
    }

    /// FILES-4: half the terminal's columns, never fewer than sixty or more than a hundred.
    #[test]
    fn the_state_is_half_the_columns_between_sixty_and_a_hundred() {
        assert_eq!(columns(100), 60);
        assert_eq!(columns(140), 70);
        assert_eq!(columns(180), 90);
        assert_eq!(columns(200), 100);
        assert_eq!(columns(300), 100);
    }

    /// FILES-4: a press on the state's border starts no drag, and the width the panel was dragged
    /// to is the one it has again once the state is left.
    #[test]
    fn dragging_the_border_does_nothing_in_the_state_and_keeps_the_dragged_width() {
        let mut session = session_with(&["a.rs"]);
        screen(&mut session, 120, 30);
        session.toggle_panel();
        screen(&mut session, 120, 30);
        assert!(session.begin_panel_drag(5, 84), "the ordinary border");
        session.drag_panel_to(70);
        session.end_panel_drag();
        assert_eq!(session.panel_columns(120), 50);

        session.toggle_files();
        assert!(session.files_open(), "the state did not open");
        screen(&mut session, 120, 30);
        assert_eq!(session.panel_columns(120), 60, "half of 120");
        assert!(
            !session.begin_panel_drag(5, 60),
            "a press on the state's border began a drag"
        );
        assert!(!session.dragging_panel());

        session.toggle_files();
        assert_eq!(session.panel_columns(120), 50, "the dragged width was lost");
    }

    /// FILES-4: below a hundred columns the state is not drawn and the keys still answer, and it is
    /// drawn again when the terminal is widened.
    #[test]
    fn the_state_is_not_drawn_below_a_hundred_columns_and_comes_back_when_widened() {
        let mut session = session_with(&["a.rs"]);
        opened(&mut session);
        let narrow = screen(&mut session, 99, 30).join("\n");
        assert!(!narrow.contains("files changed"), "drawn at 99 columns");
        assert!(session.files_open(), "narrowing ended the state");
        let wide = screen(&mut session, 100, 30).join("\n");
        assert!(
            wide.contains("1 file changed"),
            "not drawn again at 100: {wide}"
        );
    }

    /// FILES-4: the sections of the ordinary panel are not drawn beside the state.
    #[test]
    fn no_ordinary_section_is_drawn_in_the_state() {
        let mut session = session_with(&["a.rs"]);
        session.set_todos(bravebot_core::todo::rows(&bravebot_core::todo::List::new(
            vec![bravebot_core::todo::Item::new(
                "a plan item",
                bravebot_core::todo::Status::Pending,
            )],
        )));
        opened(&mut session);
        let panel = panel_of(&mut session, 120, 30).join("\n");
        for heading in [
            t!(panel_session),
            t!(panel_context),
            t!(panel_plan),
            t!(panel_files),
        ] {
            assert!(
                !panel.contains(heading),
                "{heading} drawn in the state: {panel}"
            );
        }
        assert!(!panel.contains("a plan item"));
    }

    /// FILES-4: the panel has no Context section in the state, so the hint line keeps the reading and
    /// names the state's keys.
    #[test]
    fn the_hint_line_has_the_context_reading_while_the_state_is_open() {
        let mut session = session_with(&["a.rs"]);
        opened(&mut session);
        let rows = screen(&mut session, 120, 30);
        let hint = rows.last().expect("a hint line");
        assert!(
            hint.contains(&*t!(files_hint)),
            "no keys on the hint line: {hint:?}"
        );
        assert!(
            hint.contains(&crate::render::context_reading(&session)),
            "no context reading on the hint line: {hint:?}"
        );
    }

    /// FILES-5: a row is the path and the counts at the right edge, and a count of nothing is left
    /// out.
    #[test]
    fn a_row_has_the_path_and_the_counts_of_its_edits() {
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, write("src/a.rs", "x\ny\n", "x\ny\nz\n"));
        wrote(&mut session, write("src/gone.rs", "x\ny\n", "x\n"));
        opened(&mut session);
        let panel = panel_of(&mut session, 120, 30);
        let first = panel
            .iter()
            .find(|row| row.contains("src/a.rs"))
            .expect("a row");
        assert!(first.trim_end().ends_with("+1"), "{first:?}");
        assert!(
            !first.contains('-'),
            "a count of nothing was drawn: {first:?}"
        );
        let second = panel
            .iter()
            .find(|row| row.contains("src/gone.rs"))
            .expect("a row");
        assert!(
            second.trim_end().ends_with("-1") && !second.contains('+'),
            "{second:?}"
        );
    }

    /// FILES-5: a path past its room is cut from the left, so the end that tells two apart stays.
    #[test]
    fn a_long_path_keeps_its_end() {
        let path = format!("{}/deep/file_name.rs", "directory".repeat(12));
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, write(&path, "x\n", "y\n"));
        opened(&mut session);
        let panel = panel_of(&mut session, 120, 30);
        let row = panel
            .iter()
            .find(|row| row.contains("file_name.rs"))
            .expect("a row");
        assert!(
            row.contains("…") && row.contains("deep/file_name.rs"),
            "{row:?}"
        );
        assert!(!row.contains(" ▸directory"), "the start was kept: {row:?}");
    }

    /// FILES-5: an escape in a path is drawn as its picture, so a path cannot act on the terminal.
    #[test]
    fn a_control_character_in_a_path_is_drawn_as_a_picture() {
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, write("a\u{1b}[2Jb.rs", "x\n", "y\n"));
        opened(&mut session);
        let panel = panel_of(&mut session, 120, 30).join("\n");
        assert!(panel.contains("a\u{241b}[2Jb.rs"), "{panel}");
        assert!(!panel.contains('\u{1b}'));
    }

    /// FILES-5: the state opens with the last file written highlighted.
    #[test]
    fn the_state_opens_on_the_last_file_written() {
        let mut session = session_with(&["a.rs", "b.rs", "c.rs"]);
        opened(&mut session);
        assert_eq!(session.files_state().map(|state| state.selected), Some(2));
        let panel = panel_of(&mut session, 120, 30);
        let marked = panel
            .iter()
            .find(|row| row.contains('▸'))
            .expect("a highlight");
        assert!(marked.contains("c.rs"), "{marked:?}");
    }

    /// FILES-5: Up, Down, `k` and `j` stop at each end, and Home, End, `g` and `G` go to the ends.
    #[test]
    fn the_highlight_stops_at_each_end() {
        let mut session = session_with(&["a.rs", "b.rs", "c.rs"]);
        opened(&mut session);
        let at = |session: &Session| session.files_state().map(|state| state.selected);
        press(&mut session, KeyCode::Down);
        press(&mut session, KeyCode::Char('j'));
        assert_eq!(
            at(&session),
            Some(2),
            "the highlight went past the last file"
        );
        press(&mut session, KeyCode::Up);
        press(&mut session, KeyCode::Char('k'));
        press(&mut session, KeyCode::Up);
        assert_eq!(
            at(&session),
            Some(0),
            "the highlight went before the first file"
        );
        press(&mut session, KeyCode::End);
        assert_eq!(at(&session), Some(2));
        press(&mut session, KeyCode::Char('g'));
        assert_eq!(at(&session), Some(0));
        press(&mut session, KeyCode::Char('G'));
        assert_eq!(at(&session), Some(2));
        press(&mut session, KeyCode::Home);
        assert_eq!(at(&session), Some(0));
    }

    /// FILES-5: with more files than rows the rows drawn are the ones around the highlight, and the
    /// first and last say how many are left out above and below.
    #[test]
    fn a_long_list_keeps_the_highlight_on_screen_and_counts_each_side() {
        let paths: Vec<String> = (1..=12).map(|n| format!("f{n:02}.rs")).collect();
        let names: Vec<&str> = paths.iter().map(String::as_str).collect();
        let mut session = session_with(&names);
        // Room for a list of four rows: a heading, a rule and a name take three more.
        screen(&mut session, 120, 16);
        session.toggle_files();
        screen(&mut session, 120, 16);
        let at_end = panel_of(&mut session, 120, 16).join("\n");
        assert!(
            at_end.contains("f12.rs") && at_end.contains("↑ 9 more above"),
            "{at_end}"
        );
        assert!(!at_end.contains("below"), "{at_end}");
        press(&mut session, KeyCode::Home);
        let at_start = panel_of(&mut session, 120, 16).join("\n");
        assert!(
            at_start.contains("f01.rs") && at_start.contains("↓ 9 more below"),
            "{at_start}"
        );
        assert!(!at_start.contains("above"), "{at_start}");
        for _ in 0..5 {
            press(&mut session, KeyCode::Down);
        }
        let middle = panel_of(&mut session, 120, 16).join("\n");
        let marked = middle
            .lines()
            .find(|row| row.contains('▸'))
            .expect("a highlight");
        assert!(marked.contains("f06.rs"), "{middle}");
        assert!(
            middle.contains("more above") && middle.contains("more below"),
            "{middle}"
        );
    }

    /// FILES-8: a file written while the list is held below the window is said to have arrived.
    #[test]
    fn the_list_says_a_file_has_arrived_below() {
        let paths: Vec<String> = (1..=12).map(|n| format!("f{n:02}.rs")).collect();
        let names: Vec<&str> = paths.iter().map(String::as_str).collect();
        let mut session = session_with(&names);
        screen(&mut session, 120, 16);
        session.toggle_files();
        press(&mut session, KeyCode::Home);
        assert!(!panel_of(&mut session, 120, 16).join("\n").contains("new"));
        wrote(&mut session, write("late.rs", "x\n", "y\n"));
        let panel = panel_of(&mut session, 120, 16).join("\n");
        assert!(panel.contains("↓ 10 more below, 1 new"), "{panel}");
        let held = session.files_state().cloned().expect("open");
        assert_eq!(held.selected, 0, "the highlight moved for the arrival");
    }

    /// FILES-6: the edits of the highlighted file are drawn oldest first, each under a header that
    /// says its place and its turn.
    #[test]
    fn the_edits_of_a_file_are_drawn_oldest_first_each_under_its_header() {
        let mut session = Session::new("none");
        prompt(&mut session, "first");
        wrote(&mut session, write("a.rs", "one\n", "alpha\n"));
        prompt(&mut session, "second");
        wrote(&mut session, write("a.rs", "alpha\n", "beta\n"));
        opened(&mut session);
        let panel = panel_of(&mut session, 120, 30).join("\n");
        let first = panel.find("edit 1 of 2, turn 1").expect("a first header");
        let alpha = panel.find("+ alpha").expect("the first edit's change");
        let second = panel.find("edit 2 of 2, turn 2").expect("a second header");
        let beta = panel.find("+ beta").expect("the second edit's change");
        assert!(first < alpha && alpha < second && second < beta, "{panel}");
        assert!(panel.contains("2 edits"), "{panel}");
    }

    /// FILES-6: every change of an edit is drawn, where the transcript draws the first twelve.
    #[test]
    fn every_change_of_an_edit_is_drawn_and_not_twelve() {
        let after: String = (1..=40).map(|n| format!("added {n}\n")).collect();
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, write("new.rs", "", &after));
        let files = session.written_files();
        let (rows, _) = edit_rows(&files[0], 58);
        let drawn = text_of(&rows);
        assert_eq!(
            drawn.iter().filter(|row| row.contains("+ added")).count(),
            40
        );
        assert!(drawn.iter().any(|row| row.contains("+ added 40")));
    }

    /// FILES-6: the gutter holds the number after the edit on a kept or added row and the number
    /// before it on a removed row, as the kernel made them, and an elided run counts the lines it
    /// stands for.
    #[test]
    fn the_gutter_holds_the_numbers_the_kernel_made() {
        let before: String = (1..=12).map(|n| format!("l{n}\n")).collect();
        let after = before.replace("l2\n", "").replace("l11\n", "l11\nnew\n");
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, write("n.rs", &before, &after));
        let files = session.written_files();
        let (rows, _) = edit_rows(&files[0], 58);
        let drawn = text_of(&rows);
        let find = |needle: &str| {
            drawn
                .iter()
                .find(|row| row.contains(needle))
                .unwrap_or_else(|| panic!("no row with {needle}: {drawn:#?}"))
                .clone()
        };
        assert!(find("- l2").contains(" 2 - l2"), "{:?}", find("- l2"));
        assert!(find("  l3").contains(" 2   l3"), "{:?}", find("  l3"));
        assert!(find("+ new").contains("11 + new"), "{:?}", find("+ new"));
        assert!(find("  l12").contains("12   l12"), "{:?}", find("  l12"));
    }

    /// FILES-6: a line longer than the panel wraps to its width, and the rows after the first have a
    /// blank gutter and sign.
    #[test]
    fn a_long_line_wraps_to_the_panel_and_its_continuation_has_a_blank_gutter() {
        let long = "word ".repeat(30);
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, write("w.rs", "x\n", &format!("{long}\n")));
        let files = session.written_files();
        let (rows, _) = edit_rows(&files[0], 40);
        let drawn = text_of(&rows);
        let start = drawn
            .iter()
            .position(|row| row.contains("+ word"))
            .expect("the line");
        assert!(drawn[start].contains(" 1 + word"), "{:?}", drawn[start]);
        let after = &drawn[start + 1];
        assert!(
            after.starts_with("     ") && !after.contains('+'),
            "{after:?}"
        );
        assert!(
            drawn
                .iter()
                .all(|row| crate::wrap::display_width(row) <= 40),
            "a row was wider than the panel: {drawn:#?}"
        );
    }

    /// FILES-6: `[` and `]` move to the previous and next edit header, and Page Down scrolls.
    #[test]
    fn the_bracket_keys_move_between_edit_headers() {
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        for n in 0..3 {
            let after: String = (0..20)
                .map(|line| format!("edit {n} line {line}\n"))
                .collect();
            wrote(&mut session, write("a.rs", "", &after));
        }
        opened(&mut session);
        let scroll = |session: &Session| session.files_state().map_or(0, |state| state.scroll);
        assert_eq!(scroll(&session), 0, "the edits did not open at their top");
        press(&mut session, KeyCode::Char(']'));
        assert_eq!(scroll(&session), 21, "] did not reach the second header");
        let panel = panel_of(&mut session, 120, 30);
        assert!(panel[4].contains("edit 2 of 3"), "{panel:#?}");
        press(&mut session, KeyCode::Char('['));
        assert_eq!(scroll(&session), 0);
        press(&mut session, KeyCode::Char('['));
        assert_eq!(scroll(&session), 0, "[ went before the first header");
        press(&mut session, KeyCode::PageDown);
        assert!(scroll(&session) > 0, "Page Down did not scroll");
        press(&mut session, KeyCode::PageUp);
        assert_eq!(scroll(&session), 0);
        for _ in 0..10 {
            press(&mut session, KeyCode::PageDown);
        }
        let panel = panel_of(&mut session, 120, 30);
        assert!(
            panel
                .last()
                .is_some_and(|row| row.contains("edit 2 of 3") || row.contains("line 19")),
            "scrolling past the end left the edits: {panel:#?}"
        );
    }

    /// FILES-7: an edit over a file nobody vouched for has the bar on every row it reaches, and its
    /// wrapped rows too.
    #[test]
    fn an_untrusted_edit_is_marked_on_every_row_it_reaches() {
        let mut call = write("u.rs", "x\n", &format!("{}\n", "word ".repeat(30)));
        call.untrusted = true;
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, call);
        let files = session.written_files();
        let (rows, _) = edit_rows(&files[0], 40);
        let drawn = text_of(&rows);
        assert!(drawn.len() > 3);
        for row in &drawn[1..] {
            assert!(row.starts_with(" ┃"), "a row had no bar: {row:?}");
        }
        assert!(
            !drawn[0].contains('┃'),
            "the header carries the bar: {:?}",
            drawn[0]
        );
    }

    /// FILES-7: the marking belongs to the edit, so a file with a trusted edit and then an untrusted
    /// one has the bar on the second alone.
    #[test]
    fn a_file_with_a_trusted_and_an_untrusted_edit_marks_only_the_second() {
        let mut second = write("m.rs", "a\n", "b\n");
        second.untrusted = true;
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, write("m.rs", "", "a\n"));
        wrote(&mut session, second);
        let files = session.written_files();
        let (rows, headers) = edit_rows(&files[0], 58);
        let drawn = text_of(&rows);
        assert!(
            drawn[..headers[1]].iter().all(|row| !row.contains('┃')),
            "the trusted edit was marked: {drawn:#?}"
        );
        assert!(
            drawn[headers[1] + 1..]
                .iter()
                .all(|row| row.starts_with(" ┃")),
            "{drawn:#?}"
        );
    }

    /// FILES-7: a line that opens with the bar is drawn after the margin and not as it, and its
    /// control characters are made visible.
    #[test]
    fn an_edit_cannot_paint_a_bar_in_the_margin_column() {
        let mut call = write("p.rs", "x\n", "┃ spoof\u{1b}[31m\n");
        call.untrusted = true;
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, call);
        let files = session.written_files();
        let (rows, _) = edit_rows(&files[0], 40);
        let drawn = text_of(&rows);
        let row = drawn
            .iter()
            .find(|row| row.contains("spoof"))
            .expect("the line");
        assert!(
            row.starts_with(" ┃") && row.contains("+ ┃ spoof\u{241b}[31m"),
            "{row:?}"
        );
        assert!(drawn.iter().all(|row| !row.contains('\u{1b}')));
        // The margin's own column holds the bar and no content.
        assert_eq!(row.chars().nth(1), Some('┃'));
        assert_ne!(row.chars().nth(2), Some('┃'));
    }

    /// FILES-7: no change from a file's first "before" to its last "after" is drawn, so two edits
    /// that cancel each other show each of them and no net change.
    #[test]
    fn the_state_draws_no_net_change_of_a_file() {
        let mut session = Session::new("none");
        prompt(&mut session, "go");
        wrote(&mut session, write("c.rs", "keep\nold\n", "keep\nnew\n"));
        wrote(&mut session, write("c.rs", "keep\nnew\n", "keep\nold\n"));
        opened(&mut session);
        let panel = panel_of(&mut session, 120, 30).join("\n");
        assert!(
            panel.contains("edit 1 of 2") && panel.contains("edit 2 of 2"),
            "{panel}"
        );
        assert_eq!(panel.matches("+ new").count(), 1, "{panel}");
        assert_eq!(panel.matches("- old").count(), 1, "{panel}");
        assert!(panel.contains("+1 -1"), "no net 0/0 row was drawn: {panel}");
    }

    /// FILES-10: the state draws from the transcript and holds only indexes, so nothing it shows is
    /// in a request or the record.
    #[test]
    fn nothing_the_state_holds_reaches_the_request_or_the_record() {
        let mut session = session_with(&["a.rs"]);
        opened(&mut session);
        let held = format!("{:?}", session.files_state());
        assert!(!held.contains("a.rs") && !held.contains("three"), "{held}");
        // The command is the interface's own and is not sent to the planner.
        assert!(
            crate::command_table::commands()
                .iter()
                .any(|command| command.name == crate::command_table::FILES_COMMAND)
        );
    }

    /// FILES-10: every word the state says is in the catalogue, so a French session is not shown
    /// English by it.
    #[test]
    fn the_state_draws_only_catalogue_words() {
        for message in [
            t!(files_heading, count = 2).to_string(),
            t!(files_more_above, count = 3).to_string(),
            t!(files_more_below, count = 3).to_string(),
            t!(files_edit_header, index = 1, total = 2, turn = 3).to_string(),
            t!(files_edit_count, count = 2).to_string(),
            t!(files_nothing_written).to_string(),
            t!(files_hint).to_string(),
            t!(panel_files).to_string(),
        ] {
            assert!(!message.is_empty());
        }
        let source = include_str!("files_mode.rs");
        let code = source.split("#[cfg(test)]").next().expect("code");
        for word in [
            "files changed",
            "more above",
            "more below",
            "unchanged lines",
            "edit {",
        ] {
            assert!(
                !code.contains(&format!("\"{word}")),
                "{word:?} is written in the source"
            );
        }
    }
    /// FILES-8: an edit that finishes while the state is open is added at the end, and the highlight
    /// and the scroll stay where the reader put them, in the frame it arrives in. The totals follow.
    #[test]
    fn an_edit_arriving_leaves_the_highlight_and_the_scroll_where_they_were() {
        let mut session = session_with(&["a.rs", "b.rs"]);
        prompt(&mut session, "again");
        let after: String = (0..40).map(|n| format!("line {n}\n")).collect();
        wrote(&mut session, write("a.rs", "", &after));
        opened(&mut session);
        press(&mut session, KeyCode::Home);
        press(&mut session, KeyCode::PageDown);
        let held = session.files_state().cloned().expect("open");
        assert_eq!(held.selected, 0);
        assert!(held.scroll > 0);

        wrote(&mut session, write("a.rs", "x\n", "y\n"));
        wrote(&mut session, write("c.rs", "x\n", "y\n"));
        let panel = panel_of(&mut session, 120, 30);
        let now = session.files_state().cloned().expect("open");
        assert_eq!((now.selected, now.scroll), (held.selected, held.scroll));
        assert!(
            panel[0].contains("3 files changed"),
            "the totals did not follow: {panel:#?}"
        );
        assert!(panel.iter().any(|row| row.contains("c.rs")), "{panel:#?}");
        let files = session.written_files();
        assert_eq!(files[0].edits.len(), 3, "the edit was not added at the end");
        assert_eq!(files[2].path, "c.rs", "a new file was not a new last row");
    }

    /// FILES-8: a turn starting, a call starting and finishing, and a turn ending leave the state
    /// where its reader put it.
    #[test]
    fn what_starts_without_a_press_leaves_the_files_state_where_its_reader_put_it() {
        let mut session = session_with(&["a.rs", "b.rs", "c.rs"]);
        opened(&mut session);
        press(&mut session, KeyCode::Up);
        let held = session.files_state().cloned().expect("open");
        session.status = crate::state::Status::Working;
        session.start_activity(Activity::running("Read", "z.rs").of_tool("read_file"));
        session.finish_activity(
            Activity::running("Read", "z.rs")
                .of_tool("read_file")
                .done("1 line"),
        );
        session.status = crate::state::Status::Idle;
        session.complete("done", vec![], 10);
        assert!(session.files_open(), "the state was ended without a press");
        assert_eq!(session.files_state().cloned(), Some(held));
    }

    /// FILES-10: `/clear` ends the state and the edits it listed are gone with the transcript.
    #[test]
    fn clearing_ends_the_files_state_and_forgets_its_edits() {
        let mut session = session_with(&["a.rs"]);
        opened(&mut session);
        session.clear();
        assert!(!session.files_open());
        assert!(session.written_files().is_empty());
        session.toggle_files();
        assert!(
            !session.files_open(),
            "the state opened with nothing written"
        );
    }

    /// FILES-10: a resumed session holds its earlier calls as lines with no changes, so it lists only
    /// the edits made since, and the heading says earlier edits are not kept.
    #[test]
    fn a_resumed_session_lists_only_the_edits_made_since() {
        let mut session = Session::new("none");
        session.transcript.push(Entry::user("before the resume"));
        session
            .transcript
            .push(Entry::recalled_tool("Update(old.rs)", ""));
        screen(&mut session, 120, 30);
        session.toggle_files();
        assert!(!session.files_open(), "a recalled call was listed");

        prompt(&mut session, "after the resume");
        wrote(&mut session, write("new.rs", "x\n", "y\n"));
        opened(&mut session);
        let panel = panel_of(&mut session, 120, 30);
        assert!(panel[0].contains("1 file changed"), "{panel:#?}");
        assert!(
            panel[0].contains(&*t!(files_earlier_not_kept)),
            "{panel:#?}"
        );
        let paths: Vec<String> = session
            .written_files()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(paths, ["new.rs"]);

        // And without a resume the heading says nothing of it.
        let mut fresh = session_with(&["a.rs"]);
        opened(&mut fresh);
        assert!(!panel_of(&mut fresh, 120, 30)[0].contains(&*t!(files_earlier_not_kept)));
    }
}
