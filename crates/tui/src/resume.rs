//! Choosing a session to pick up again.
//!
//! Shown by `bravebot --resume`, before anything else happens. The list is this working directory's
//! sessions, newest first, because the one being looked for is nearly always the last one.
//!
//! Typing filters rather than jumping, since a title is remembered as a few words out of the
//! middle of it rather than as the way it starts. The branch and the issue and pull request the
//! person linked are searched as well, and so is what was said in the session (SESSION-33), read
//! the first time something is typed. A `since:7d` word limits the list to recent sessions. `--from-pr` opens the list already narrowed to the
//! sessions linked to one pull request. Escape leaves without resuming anything, which
//! starts an ordinary session: nothing here can strand a user who opened it by mistake.

use crate::input;
use crate::select::{self, Click};
use crate::theme;
use bravebot_i18n::t;
use bravebot_session::search::{Corpus, Query};
use bravebot_session::sessions::{self, Summary};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What the picker is showing and where the cursor is.
#[derive(Debug)]
pub struct Picker {
    /// Every session, newest first.
    sessions: Vec<Summary>,
    /// What has been typed to narrow it.
    search: String,
    /// A refusal to show under the list, cleared by the next key.
    note: Option<String>,
    /// Which of the matching sessions is under the cursor.
    selected: usize,
    /// The first session drawn, held where a press left it: a press moves the cursor to a session
    /// that is already drawn and the list stays where it is, so a second press on the same cell is
    /// on the same session. A key, a turn of the wheel, a letter typed or a record deleted lets go of
    /// it, and the list moves as it always did.
    held_from: Option<usize>,
    /// The project these sessions belong to, for the heading.
    project: String,
    /// A pull request number or address the list was opened for, which keeps only the sessions
    /// linked to it before anything typed narrows further.
    from_pr: Option<String>,
    /// Whether it is drawn over a running session, where leaving it stays in that session.
    within_a_session: bool,
    /// Where to read what the sessions say from, once something is typed.
    reads_from: Option<PathBuf>,
    /// What the sessions say. Read on the first key that types a phrase, so opening the picker
    /// costs what it did before.
    corpus: Option<Corpus>,
    /// For each session whose words hold the phrase, the line that does.
    said: HashMap<String, String>,
    /// Whether a running background session holds a record, which Enter refuses (BG-9).
    held: fn(&str) -> bool,
    /// Whether another process holds a record of a project, which Enter refuses (SESSION-38).
    open: fn(&Path, &str) -> bool,
    /// The record whose deletion is waiting on a `y`, asked about before anything is removed.
    deleting: Option<String>,
}

impl Picker {
    pub fn new(sessions: Vec<Summary>, project: impl Into<String>) -> Self {
        Self {
            sessions,
            search: String::new(),
            note: None,
            selected: 0,
            held_from: None,
            project: project.into(),
            from_pr: None,
            within_a_session: false,
            reads_from: None,
            corpus: None,
            said: HashMap::new(),
            held: bravebot_session::jobs::is_running,
            open: sessions::is_open_elsewhere,
            deleting: None,
        }
    }

    /// Search what the sessions of `project` say, as well as their titles.
    pub fn reading(mut self, project: &Path) -> Self {
        self.reads_from = Some(project.to_path_buf());
        self
    }

    /// Search these lines for what the sessions say, instead of reading them from disk.
    #[cfg(test)]
    fn with_corpus(mut self, corpus: Corpus) -> Self {
        self.corpus = Some(corpus);
        self
    }

    /// Ask `held` rather than the roster whether a running background session holds a record.
    #[cfg(test)]
    fn with_held(mut self, held: fn(&str) -> bool) -> Self {
        self.held = held;
        self
    }

    /// Ask `open` rather than the record's lock whether another process holds a record.
    #[cfg(test)]
    fn with_open(mut self, open: fn(&Path, &str) -> bool) -> Self {
        self.open = open;
        self
    }

    /// What is typed as a query. A `since:` word that is not one is typed text like any other.
    fn query(&self) -> Query {
        Query::parse(&self.search).unwrap_or_else(|_| Query::plain(&self.search))
    }

    /// The line of this session's words that holds what is typed, if one does.
    fn said_in(&self, session: &Summary) -> Option<&str> {
        self.said.get(&session.id).map(String::as_str)
    }

    /// Find the sessions whose words hold what has been typed.
    fn search_words(&mut self) {
        let query = self.query();
        self.said.clear();
        if !query.has_phrase() {
            return;
        }
        if self.corpus.is_none() {
            let Some(project) = &self.reads_from else {
                return;
            };
            self.corpus = Some(Corpus::read(project, |_| true));
        }
        let Some(corpus) = &self.corpus else {
            return;
        };
        for session in &self.sessions {
            if let Some(line) = corpus.found(&session.id, &query) {
                self.said.insert(session.id.clone(), line);
            }
        }
    }

    /// Open the list for one pull request, given as its number or its address (`--from-pr`).
    pub fn from_pull_request(mut self, wanted: impl Into<String>) -> Self {
        self.from_pr = Some(wanted.into());
        self
    }

    /// The sessions matching what has been typed, in order.
    ///
    /// Matched without regard to case and anywhere in the title, because a session is remembered
    /// by a word out of the middle of what was asked, and anywhere in the branch or in either
    /// link, because it is just as often remembered by where its work went.
    ///
    /// Where the typed text is the whole of some session's link, a link matches only by being that
    /// text. A pasted pull request address then finds the session that was given it, rather than
    /// that one and every session whose pull request number merely starts with the same digits.
    /// The title and the branch still match by what they contain.
    ///
    /// A session whose words hold the typed text matches too, and a `since:` word keeps only the
    /// sessions written within it.
    pub fn matching(&self) -> Vec<&Summary> {
        let query = self.query();
        let needle = if query.has_window() {
            query.phrase().to_string()
        } else {
            self.search.to_lowercase()
        };
        let links = |session: &'_ Summary| {
            [session.issue.as_deref(), session.pull_request.as_deref()]
                .into_iter()
                .flatten()
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        };
        let pasted = !needle.is_empty()
            && self
                .sessions
                .iter()
                .any(|session| links(session).contains(&needle));
        self.sessions
            .iter()
            .filter(|session| {
                self.from_pr
                    .as_deref()
                    .is_none_or(|wanted| is_pull_request(session, wanted))
            })
            .filter(|session| query.admits_now(session.updated))
            .filter(|session| {
                let words = [Some(session.title.as_str()), session.branch.as_deref()]
                    .into_iter()
                    .flatten()
                    .any(|field| field.to_lowercase().contains(&needle));
                let linked = links(session).iter().any(|link| {
                    if pasted {
                        *link == needle
                    } else {
                        link.contains(&needle)
                    }
                });
                words || linked || self.said_in(session).is_some()
            })
            .collect()
    }

    /// The session under the cursor, if the list is not empty.
    pub fn chosen(&self) -> Option<&Summary> {
        self.matching().get(self.selected).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Drop a session from the list once its record is gone, keeping the cursor inside it.
    fn forget(&mut self, id: &str) {
        self.sessions.retain(|session| session.id != id);
        self.said.remove(id);
        self.held_from = None;
        self.clamp();
    }

    fn down(&mut self) {
        let last = self.matching().len().saturating_sub(1);
        self.selected = (self.selected + 1).min(last);
        self.held_from = None;
    }

    fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.held_from = None;
    }

    /// Narrow the list, keeping the cursor inside it.
    fn typed(&mut self, c: char) {
        self.search.push(c);
        self.search_words();
        self.held_from = None;
        self.clamp();
    }

    fn backspace(&mut self) {
        self.search.pop();
        self.search_words();
        self.held_from = None;
        self.clamp();
    }

    /// A search that no longer matches what was selected must not leave the cursor past the end.
    fn clamp(&mut self) {
        let last = self.matching().len().saturating_sub(1);
        self.selected = self.selected.min(last);
    }
}

/// Whether the session's pull request is the one `wanted` names: its address, or its number, which
/// is the last part of the address on GitHub, GitLab and Bitbucket alike. A number is not matched
/// as a substring, so `127` does not find the session whose pull request is `1270`.
fn is_pull_request(session: &Summary, wanted: &str) -> bool {
    let Some(link) = session.pull_request.as_deref() else {
        return false;
    };
    let link = link.trim().trim_end_matches('/').to_lowercase();
    let wanted = wanted.trim().trim_end_matches('/').to_lowercase();
    let number = wanted.trim_start_matches('#');
    if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
        return link.rsplit('/').next() == Some(number);
    }
    link == wanted
}

/// What taking the session under the cursor does: resume it, or say why it cannot be.
fn take(picker: &Picker) -> Outcome {
    match picker.chosen() {
        Some(session) if session.manifest => Outcome::Refused(manifest_note().to_string()),
        Some(session) => {
            match held_note(&session.id, picker.held).or_else(|| open_note(picker, &session.id)) {
                Some(note) => Outcome::Refused(note),
                None => Outcome::Resume,
            }
        }
        _ => Outcome::Resume,
    }
}

/// What a key press did to the picker.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Still choosing.
    Continue,
    /// Resume the session under the cursor.
    Resume,
    /// Leave without resuming anything, and start an ordinary session.
    Cancel,
    /// Leave without starting anything at all.
    Quit,
    /// The session under the cursor cannot be continued, with the reason to show.
    ///
    /// Distinct from `Continue` so the refusal is said out loud. A picker that quietly ignored
    /// Enter would look broken, and the reason is worth knowing: the record is still there to
    /// read, it just has nothing to carry on from.
    Refused(String),
    /// The person confirmed deleting this session's record.
    Delete(String),
}

/// Why a manifest run cannot be picked up.
///
/// A session is turns over one conversation and a manifest run has none: the planner is never
/// shown a result, so there is nothing for a later turn to continue. The record is written all
/// the same, because what a run produced is worth reading whether or not it can be resumed.
pub fn manifest_note() -> &'static str {
    t!(resume_manifest_run)
}

/// How much of a background session's name the refusal quotes, so that the id after it is not
/// cut off at the edge of a 100 column terminal.
const NAME_SHOWN: usize = 12;

/// The refusal for choosing a record a running background session holds, which names the session
/// and says how to join it, or `None` where `held` says nothing holds it (BG-9).
fn held_note(id: &str, held: impl Fn(&str) -> bool) -> Option<String> {
    if !held(id) {
        return None;
    }
    Some(bravebot_session::jobs::held_refusal(id, NAME_SHOWN))
}

/// The refusal for choosing a record another process holds, which names the session and says how to
/// copy it instead, or `None` where nothing holds it (SESSION-38).
fn open_note(picker: &Picker, id: &str) -> Option<String> {
    let project = picker.reads_from.as_deref()?;
    (picker.open)(project, id).then(|| sessions::Held::Open { id: id.to_string() }.refusal())
}

/// How much of a title the deletion question quotes, so the keys after it stay on the row.
const TITLE_ASKED: usize = 40;

/// Interpret one key press.
///
/// Separated from the loop so it can be tested without a terminal.
pub fn handle_key(picker: &mut Picker, code: KeyCode, modifiers: KeyModifiers) -> Outcome {
    if modifiers.contains(KeyModifiers::CONTROL) {
        return match code {
            // Raw mode delivers the interrupt as a key rather than as a signal, and someone
            // pressing it wants out of the program, not a different session.
            KeyCode::Char('c') => Outcome::Quit,
            _ => Outcome::Continue,
        };
    }

    // A question asked of the highlighted row is answered by the next key and by nothing else:
    // `y` agrees, and any other key, Escape included, keeps the record and stays in the list.
    if let Some(id) = picker.deleting.take() {
        return match code {
            KeyCode::Char('y' | 'Y') => Outcome::Delete(id),
            _ => Outcome::Continue,
        };
    }

    match code {
        KeyCode::Esc => Outcome::Cancel,
        KeyCode::Delete => match picker.chosen() {
            Some(session) if (picker.held)(&session.id) => {
                Outcome::Refused(t!(resume_delete_held).to_string())
            }
            Some(session) => {
                picker.deleting = Some(session.id.clone());
                Outcome::Continue
            }
            None => Outcome::Continue,
        },
        KeyCode::Enter => take(picker),
        KeyCode::Up => {
            picker.up();
            Outcome::Continue
        }
        KeyCode::Down => {
            picker.down();
            Outcome::Continue
        }
        KeyCode::Backspace => {
            picker.backspace();
            Outcome::Continue
        }
        KeyCode::Char(c) => {
            picker.typed(c);
            Outcome::Continue
        }
        _ => Outcome::Continue,
    }
}

/// What a mouse event did to the picker.
///
/// A left press on a session puts the cursor on it, and a second on the same session within
/// [`select::MULTI_CLICK`] takes it as Enter does, which resumes it or says why it cannot be
/// (MOUSE-15). A turn of the wheel moves the cursor `wheel` sessions (MOUSE-46). Everything else,
/// a press on the heading, the search box, the project or the keys, a press below the last session,
/// another button, a drag and a release, does nothing. The question of deleting a record is
/// answered by the next key and by nothing else, so while it stands no mouse event does anything
/// (MOUSE-44). `list` is where [`draw`] drew the list, and `last` the press answered before this
/// one.
pub fn handle_mouse(
    picker: &mut Picker,
    list: Rect,
    mouse: MouseEvent,
    last: &mut Option<Click>,
    now: std::time::Instant,
    wheel: u16,
) -> Outcome {
    if picker.deleting.is_some() {
        *last = None;
        return Outcome::Continue;
    }
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            for _ in 0..wheel {
                picker.down();
            }
            return Outcome::Continue;
        }
        MouseEventKind::ScrollUp => {
            for _ in 0..wheel {
                picker.up();
            }
            return Outcome::Continue;
        }
        MouseEventKind::Down(MouseButton::Left) => {}
        _ => return Outcome::Continue,
    }
    let Some(index) = entry_at(picker, list, mouse.row, mouse.column) else {
        *last = None;
        return Outcome::Continue;
    };
    let session = picker
        .matching()
        .get(index)
        .map(|session| session.id.clone());
    // The list stays where it was: the cursor moves to a session already on the screen.
    picker.held_from = Some(window(picker, list.height).0);
    picker.selected = index;
    let (click, count) = select::count_press_on(*last, now, (mouse.row, mouse.column), &session);
    *last = Some(click);
    if count == 2 {
        *last = None;
        take(picker)
    } else {
        Outcome::Continue
    }
}

/// Which of the matching sessions is drawn at a cell, as [`list_lines`] draws them in `list`.
fn entry_at(picker: &Picker, list: Rect, row: u16, column: u16) -> Option<usize> {
    if !list.contains(ratatui::layout::Position::new(column, row)) {
        return None;
    }
    let (first, visible) = window(picker, list.height);
    let drawn = usize::from(row - list.y) / ENTRY_ROWS;
    (drawn < visible)
        .then_some(first + drawn)
        .filter(|index| *index < picker.matching().len())
}

/// What the picker settled on.
#[derive(Debug)]
pub enum Choice {
    /// Pick this session up.
    Resume(Box<sessions::Record>),
    /// Start an ordinary session: there was nothing to resume, or the user decided not to.
    /// Neither is a failure.
    Fresh,
    /// Start nothing. The user asked to leave.
    Quit,
}

/// Show the list and return what to do.
///
/// `from_pr` opens the list for one pull request, by number or address. Where no session is linked
/// to it the picker says so rather than starting a session, so the answer is seen.
pub fn choose<B: Backend>(
    terminal: &mut Terminal<B>,
    project: &Path,
    from_pr: Option<&str>,
    wheel: u16,
) -> Choice {
    pick(terminal, project, None, from_pr, wheel)
}

/// Why `/resume` carried on from no record.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The id is not shaped like a session's name, or names no record of this directory.
    NoSuchSession,
    /// The id is the session being run.
    AlreadyHere,
    /// The record is a manifest run, which has no conversation to continue.
    Manifest,
    /// A running background session holds the record, so a second writer would fork it (BG-9).
    Held,
    /// Another process holds the record, which one process writes at a time (SESSION-38).
    HeldByAnother,
}

impl Refusal {
    /// What to say in the transcript. The id typed is never part of it, since it may hold an escape.
    pub fn note(&self) -> String {
        match self {
            Self::NoSuchSession => t!(session_resume_no_such).to_string(),
            Self::AlreadyHere => t!(session_resume_already_here).to_string(),
            Self::Manifest => manifest_note().to_string(),
            Self::Held => t!(session_resume_held_by_background).to_string(),
            Self::HeldByAnother => t!(session_resume_held_by_another_process).to_string(),
        }
    }
}

/// The record `/resume <id>` names, read as `--resume <id>` reads it.
///
/// The id is checked for the shape of a session's name before it reaches a path.
pub fn named(project: &Path, current: &str, typed: &str) -> Result<Box<sessions::Record>, Refusal> {
    if typed == current {
        return Err(Refusal::AlreadyHere);
    }
    sessions::is_a_session_name(typed)
        .then(|| sessions::load(project, typed))
        .flatten()
        .map(Box::new)
        .ok_or(Refusal::NoSuchSession)
        .and_then(|record| continuable(project, record))
}

/// The record, if it is one that can be carried on from: neither a manifest run nor one a running
/// background session or another process holds. The picker refuses the first on Enter; the others
/// are checked here because a record is picked up in this process and not another.
pub fn continuable(
    project: &Path,
    record: Box<sessions::Record>,
) -> Result<Box<sessions::Record>, Refusal> {
    continuable_unless(record, bravebot_session::jobs::is_running, |id| {
        sessions::is_open_elsewhere(project, id)
    })
}

/// [`continuable`], with the questions of whether a running background session holds an id and
/// whether another process does put by the caller, so that the refusals can be reached without
/// either.
pub fn continuable_unless(
    record: Box<sessions::Record>,
    held: impl Fn(&str) -> bool,
    open: impl Fn(&str) -> bool,
) -> Result<Box<sessions::Record>, Refusal> {
    if record.manifest.is_some() {
        return Err(Refusal::Manifest);
    }
    if held(&record.id) {
        return Err(Refusal::Held);
    }
    if open(&record.id) {
        return Err(Refusal::HeldByAnother);
    }
    Ok(record)
}

/// Show the list from inside a running session, leaving out the session being run.
///
/// Picking the session already open would reload it from its record and gain nothing, so it is not
/// offered. Where nothing else is left the answer is `Fresh`, which the caller reads as staying put.
pub fn choose_other<B: Backend>(
    terminal: &mut Terminal<B>,
    project: &Path,
    current: &str,
    wheel: u16,
) -> Choice {
    pick(terminal, project, Some(current), None, wheel)
}

fn pick<B: Backend>(
    terminal: &mut Terminal<B>,
    project: &Path,
    current: Option<&str>,
    from_pr: Option<&str>,
    wheel: u16,
) -> Choice {
    match opened(project, current, from_pr) {
        Some(picker) => converse(terminal, project, picker, wheel, sessions::delete),
        None => Choice::Fresh,
    }
}

/// Draw `picker` and take keys until one settles it. `delete` removes a record the person
/// confirmed deleting.
pub(crate) fn converse<B: Backend>(
    terminal: &mut Terminal<B>,
    project: &Path,
    mut picker: Picker,
    wheel: u16,
    delete: impl Fn(&Path, &str) -> Result<(), sessions::Deletion>,
) -> Choice {
    let mut list = Rect::default();
    let mut last = None;
    let mut ignored = false;
    loop {
        // Not again for a position of the pointer, which says nothing to a list, and a pointer
        // crossing the window sends one for each cell.
        if !ignored
            && terminal
                .draw(|frame| {
                    draw(frame, &picker);
                    list = list_area(frame.area());
                })
                .is_err()
        {
            return Choice::Fresh;
        }
        ignored = false;

        let Ok(event) = input::read() else {
            return Choice::Fresh;
        };
        if input::is_a_move(&event) {
            ignored = true;
            continue;
        }
        let outcome = match event {
            TermEvent::Mouse(mouse) => {
                // Cleared by what moves the cursor or takes a session, as a key clears it, and not
                // while the question of deleting is up, which a refusal is not part of.
                if picker.deleting.is_none()
                    && matches!(
                        mouse.kind,
                        MouseEventKind::Down(MouseButton::Left)
                            | MouseEventKind::ScrollUp
                            | MouseEventKind::ScrollDown
                    )
                {
                    picker.note = None;
                }
                let now = std::time::Instant::now();
                handle_mouse(&mut picker, list, mouse, &mut last, now, wheel)
            }
            other => {
                let Some(key) = input::key_of(&other) else {
                    continue;
                };
                // A key event arrives twice on Windows, once pressed and once released, and the
                // release would otherwise choose whatever the press had just moved to.
                if key.kind != event::KeyEventKind::Press {
                    continue;
                }

                // Cleared on every key, so a refusal stays up until the user does something and no
                // longer than that.
                picker.note = None;

                handle_key(&mut picker, key.code, key.modifiers)
            }
        };

        match outcome {
            Outcome::Continue => continue,
            Outcome::Cancel => return Choice::Fresh,
            Outcome::Quit => return Choice::Quit,
            Outcome::Refused(note) => {
                picker.note = Some(note);
                continue;
            }
            Outcome::Delete(id) => {
                picker.note = Some(delete_record(&mut picker, project, &id, &delete));
                continue;
            }
            Outcome::Resume => {
                let Some(chosen) = picker.chosen() else {
                    continue;
                };
                let id = chosen.id.clone();
                return match sessions::load(project, &id) {
                    Some(record) => Choice::Resume(Box::new(record)),
                    None => Choice::Fresh,
                };
            }
        }
    }
}

/// Remove a session's record with `delete` and drop it from the list, returning what to say.
///
/// A record that cannot be removed stays listed, because the list is what is on disk.
fn delete_record(
    picker: &mut Picker,
    project: &Path,
    id: &str,
    delete: impl Fn(&Path, &str) -> Result<(), sessions::Deletion>,
) -> String {
    match delete(project, id) {
        Ok(()) => {
            picker.forget(id);
            t!(resume_delete_done).to_string()
        }
        Err(_) => t!(resume_delete_failed).to_string(),
    }
}

/// The list `pick` shows, or `None` where there is nothing to show.
///
/// A named pull request is shown even with no session listed at all: that none is linked to it is
/// the answer, and it has to be seen.
fn opened(project: &Path, current: Option<&str>, from_pr: Option<&str>) -> Option<Picker> {
    let mut listed = sessions::list(project);
    listed.retain(|session| Some(session.id.as_str()) != current);
    let mut picker = Picker::new(listed, project.display().to_string()).reading(project);
    picker.within_a_session = current.is_some();
    if let Some(wanted) = from_pr {
        picker = picker.from_pull_request(wanted);
    }
    (!picker.is_empty() || picker.from_pr.is_some()).then_some(picker)
}

/// The heading, the search box, the project, the list and the keys, in a frame of `area`.
fn layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // heading
            Constraint::Length(3), // search box
            Constraint::Length(1), // project
            Constraint::Min(1),    // list
            Constraint::Length(1), // keys
        ])
        .split(area)
}

/// Where [`draw`] draws the list in a frame of `frame`.
pub fn list_area(frame: Rect) -> Rect {
    layout(frame)[3]
}

/// Draw the list.
fn draw(frame: &mut Frame, picker: &Picker) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let layout = layout(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  {}", t!(resume_heading)),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        ))),
        layout[0],
    );

    let search = if picker.search.is_empty() {
        Span::styled(
            t!(resume_search_placeholder),
            Style::default().fg(theme::muted()),
        )
    } else {
        Span::raw(picker.search.clone())
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::raw(" "), search])).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::muted())),
        ),
        layout[1],
    );

    let project = match &picker.from_pr {
        Some(wanted) => format!(
            "  {}  ·  {}",
            picker.project,
            t!(resume_from_pr, pull_request = wanted.as_str())
        ),
        None => format!("  {}", picker.project),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            project,
            Style::default().fg(theme::muted()),
        ))),
        layout[2],
    );

    frame.render_widget(Paragraph::new(list_lines(picker, layout[3])), layout[3]);

    let asked = picker.deleting.as_deref().and_then(|id| {
        picker
            .sessions
            .iter()
            .find(|session| session.id == id)
            .map(|session| bravebot_session::search::clean(&session.title, TITLE_ASKED))
    });
    let (footer, colour) = match &picker.note {
        _ if asked.is_some() => (
            format!(
                "  {}",
                t!(resume_delete_question, title = asked.unwrap_or_default())
            ),
            theme::note(),
        ),
        Some(note) => (format!("  {note}"), theme::note()),
        None if picker.within_a_session => {
            (format!("  {}", t!(resume_keys_within)), theme::muted())
        }
        None => (format!("  {}", t!(resume_keys)), theme::muted()),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            footer,
            Style::default().fg(colour),
        ))),
        layout[4],
    );
}

/// How many rows an entry takes in the list: its title, when and where, and the line of what was
/// said that holds the search.
const ENTRY_ROWS: usize = 3;

/// The first session drawn in a list `height` rows tall, and how many are: the window is what fits
/// rather than what exists, and keeps the cursor on the last row of it once it moves past the end.
/// Where a press has held the list it stays, while the cursor is among the sessions drawn.
fn window(picker: &Picker, height: u16) -> (usize, usize) {
    let visible = (usize::from(height) / ENTRY_ROWS).max(1);
    match picker.held_from {
        Some(first) if (first..first + visible).contains(&picker.selected) => (first, visible),
        _ => (picker.selected.saturating_sub(visible - 1), visible),
    }
}

/// The list itself: two lines per session, and a word when nothing matches.
fn list_lines(picker: &Picker, area: Rect) -> Vec<Line<'static>> {
    let matching = picker.matching();
    if matching.is_empty() {
        return vec![Line::from(Span::styled(
            format!("  {}", t!(resume_nothing_matches)),
            Style::default().fg(theme::muted()),
        ))];
    }

    let (first, visible) = window(picker, area.height);

    let mut lines = Vec::new();
    for (index, session) in matching.iter().enumerate().skip(first).take(visible) {
        let chosen = index == picker.selected;
        let marker = if chosen { "❯ " } else { "  " };
        let title = if chosen {
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme::brand_primary())),
            Span::styled(session.title.clone(), title),
        ]));
        lines.push(Line::from(Span::styled(
            format!("  {}", describe(session)),
            Style::default().fg(theme::muted()),
        )));
        lines.push(match picker.said_in(session) {
            Some(line) => Line::from(Span::raw(format!("  {line}"))),
            None => Line::raw(""),
        });
    }
    lines
}

/// The second line of an entry: when, where and how much.
fn describe(session: &Summary) -> String {
    let mut parts = vec![sessions::how_long_ago(session.updated)];
    // Said on the row rather than on selection, so nobody picks one and then finds out.
    if session.manifest {
        parts.push("manifest".to_string());
    }
    if let Some(branch) = &session.branch {
        parts.push(branch.clone());
    }
    // Shortened to the front of the id, which is what tells one session of a list from another.
    if let Some(source) = &session.handed_off_from {
        parts.push(t!(
            resume_handed_off,
            id = source.chars().take(8).collect::<String>()
        ));
    }
    parts.push(sessions::size(session.bytes));
    parts.join("  ·  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: &str, title: &str, updated: u64) -> Summary {
        Summary {
            id: id.to_string(),
            title: title.to_string(),
            directory: "/w".to_string(),
            front: Some("terminal".to_string()),
            branch: Some("main".to_string()),
            issue: None,
            pull_request: None,
            handed_off_from: None,
            updated,
            bytes: 1024,
            manifest: false,
        }
    }

    fn manifest_summary(id: &str, title: &str, updated: u64) -> Summary {
        Summary {
            id: id.to_string(),
            title: title.to_string(),
            directory: "/w".to_string(),
            front: Some("terminal".to_string()),
            branch: Some("main".to_string()),
            issue: None,
            pull_request: None,
            handed_off_from: None,
            updated,
            bytes: 1024,
            manifest: true,
        }
    }

    /// A session begun by a handoff says on its row which one it came from, and one that was not
    /// says nothing of the kind.
    #[test]
    fn a_handed_off_session_names_its_source_on_its_row() {
        let mut handed = summary("a", "Port the parser", 100);
        handed.handed_off_from = Some("0a1b2c3d-0000-4000-8000-000000000000".to_string());
        assert!(
            describe(&handed).contains("from 0a1b2c3d"),
            "{}",
            describe(&handed)
        );
        assert!(!describe(&summary("b", "Other", 100)).contains("from "));
    }

    fn drawn(picker: &Picker) -> String {
        let mut terminal =
            Terminal::new(ratatui::backend::TestBackend::new(100, 12)).expect("a terminal");
        terminal.draw(|frame| draw(frame, picker)).expect("a frame");
        terminal.backend().to_string()
    }

    /// CMD-14. Drawn over a running session the list says Escape stays in it, and drawn at startup
    /// it says Escape starts a new one: the same key, and the footer is where a person reads which.
    #[test]
    fn the_footer_says_what_escape_does_where_the_list_is_drawn() {
        let mut within = picker();
        within.within_a_session = true;
        assert!(drawn(&within).contains(t!(resume_keys_within)));
        assert!(!drawn(&within).contains(t!(resume_keys)));

        let at_startup = picker();
        assert!(drawn(&at_startup).contains(t!(resume_keys)));
        assert!(!drawn(&at_startup).contains(t!(resume_keys_within)));
    }

    fn picker() -> Picker {
        Picker::new(
            vec![
                summary("1", "Session recovery after laptop sleep", 300),
                summary("2", "User experience progress updates", 200),
                summary("3", "Launch the TUI", 100),
            ],
            "/work/bravebot",
        )
    }

    fn press(picker: &mut Picker, code: KeyCode) -> Outcome {
        handle_key(picker, code, KeyModifiers::NONE)
    }

    /// CLI-33. Delete asks first and nothing is removed until `y`: a picker that deleted on the
    /// key alone would lose a record to a stray press.
    #[test]
    fn delete_asks_and_only_y_confirms_it() {
        let mut picker = picker();
        press(&mut picker, KeyCode::Down);
        assert_eq!(press(&mut picker, KeyCode::Delete), Outcome::Continue);
        assert!(drawn(&picker).contains("User experience progress updates"));
        assert!(drawn(&picker).contains("y deletes it"));
        assert_eq!(
            press(&mut picker, KeyCode::Char('y')),
            Outcome::Delete("2".to_string())
        );
    }

    /// CLI-33. Any other key keeps the record and the list, Escape included, which does not leave
    /// the picker while a question is open; the key is not typed into the search either.
    #[test]
    fn any_other_key_than_y_keeps_the_record() {
        for code in [
            KeyCode::Esc,
            KeyCode::Enter,
            KeyCode::Char('n'),
            KeyCode::Char('x'),
        ] {
            let mut picker = picker();
            press(&mut picker, KeyCode::Delete);
            assert_eq!(press(&mut picker, code), Outcome::Continue, "{code:?}");
            assert!(picker.deleting.is_none(), "{code:?}");
            assert!(picker.search.is_empty(), "{code:?}");
            assert_eq!(ids(&picker), ["1", "2", "3"]);
            // The question is gone, so a `y` now is typed text and deletes nothing.
            assert_eq!(press(&mut picker, KeyCode::Char('y')), Outcome::Continue);
        }
    }

    /// CLI-33. A record a running background session holds is refused, as `sessions delete` does,
    /// and no question is asked.
    #[test]
    fn delete_on_a_held_session_is_refused_without_asking() {
        let mut picker = picker().with_held(|id| id == "1");
        let Outcome::Refused(note) = press(&mut picker, KeyCode::Delete) else {
            panic!("a held session was offered for deletion");
        };
        assert_eq!(note, t!(resume_delete_held));
        assert!(picker.deleting.is_none());
        assert_eq!(press(&mut picker, KeyCode::Char('y')), Outcome::Continue);
    }

    /// CLI-33. A removed record leaves the list and a record that would not go stays in it, since
    /// the list shows what is on disk.
    #[test]
    fn a_deleted_row_leaves_the_list_and_a_failed_one_stays() {
        let mut picker = picker();
        press(&mut picker, KeyCode::Down);
        press(&mut picker, KeyCode::Down);
        let note = delete_record(&mut picker, Path::new("/w"), "3", |_, _| Ok(()));
        assert_eq!(note, t!(resume_delete_done));
        assert_eq!(ids(&picker), ["1", "2"]);
        assert_eq!(picker.chosen().expect("a session").id, "2");

        let note = delete_record(&mut picker, Path::new("/w"), "2", |_, _| {
            Err(sessions::Deletion::NotFound)
        });
        assert_eq!(note, t!(resume_delete_failed));
        assert_eq!(ids(&picker), ["1", "2"]);
    }

    fn linked(id: &str, branch: &str, issue: Option<&str>, pull_request: Option<&str>) -> Summary {
        Summary {
            branch: Some(branch.to_string()),
            issue: issue.map(str::to_string),
            pull_request: pull_request.map(str::to_string),
            handed_off_from: None,
            ..summary(id, "a title that names none of them", 100)
        }
    }

    fn typed(picker: &mut Picker, text: &str) {
        for c in text.chars() {
            handle_key(picker, KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    fn ids(picker: &Picker) -> Vec<&str> {
        picker.matching().iter().map(|s| s.id.as_str()).collect()
    }

    fn linked_picker() -> Picker {
        Picker::new(
            vec![
                linked(
                    "a",
                    "fix-resume-search",
                    Some("https://github.com/brave/bravebot/issues/1429"),
                    Some("https://github.com/brave/bravebot/pull/1270"),
                ),
                linked(
                    "b",
                    "Add-Compaction-Notes",
                    None,
                    Some("https://github.com/brave/bravebot/pull/12700"),
                ),
                linked("c", "main", None, None),
            ],
            "/work/bravebot",
        )
    }

    /// Someone who remembers where the work went rather than what was asked first.
    #[test]
    fn typing_part_of_a_branch_finds_the_session_that_ran_on_it() {
        let mut picker = linked_picker();
        typed(&mut picker, "compaction-NOTES");
        assert_eq!(ids(&picker), ["b"]);
    }

    #[test]
    fn typing_part_of_an_issue_link_finds_the_session_given_it() {
        let mut picker = linked_picker();
        typed(&mut picker, "issues/1429");
        assert_eq!(ids(&picker), ["a"]);
    }

    /// A pasted address is the whole of one link and the start of another, and only the session
    /// that holds it is wanted.
    #[test]
    fn a_pasted_pull_request_link_leaves_only_the_session_that_holds_it() {
        let mut picker = linked_picker();
        typed(&mut picker, "https://github.com/brave/bravebot/pull/1270");
        assert_eq!(ids(&picker), ["a"]);
    }

    /// A session may have been started by pasting the address into its first prompt, and it is
    /// still one the person is looking for.
    #[test]
    fn a_pasted_link_keeps_a_session_whose_title_holds_it() {
        let mut picker = linked_picker();
        picker.sessions.push(Summary {
            title: "review https://github.com/brave/bravebot/pull/1270 for races".to_string(),
            ..linked("d", "main", None, None)
        });
        typed(&mut picker, "https://github.com/brave/bravebot/pull/1270");
        assert_eq!(ids(&picker), ["a", "d"]);
    }

    /// Part of an address is not the whole of one, so it still narrows by what it contains.
    #[test]
    fn part_of_a_pull_request_link_keeps_every_session_it_is_part_of() {
        let mut picker = linked_picker();
        typed(&mut picker, "pull/1270");
        assert_eq!(ids(&picker), ["a", "b"]);
    }

    /// `--from-pr 1270` is the number, and the session whose number merely starts with it is not
    /// the one meant.
    #[test]
    fn a_pull_request_number_leaves_only_the_session_linked_to_that_number() {
        let mut picker = linked_picker();
        // Ends in the same digits, without being that number.
        picker.sessions.push(linked(
            "d",
            "main",
            None,
            Some("https://github.com/brave/bravebot/pull/21270"),
        ));
        let picker = picker.from_pull_request("1270");
        assert_eq!(ids(&picker), ["a"]);
        let picker = linked_picker().from_pull_request("#12700");
        assert_eq!(ids(&picker), ["b"]);
        let picker = linked_picker().from_pull_request("127");
        assert!(ids(&picker).is_empty());
    }

    /// A full address names exactly one pull request, so a trailing slash does not change which
    /// session is meant and a different number in the same repository does not match it.
    #[test]
    fn a_pull_request_address_leaves_only_the_session_linked_to_it() {
        let picker =
            linked_picker().from_pull_request("https://github.com/brave/bravebot/pull/1270/");
        assert_eq!(ids(&picker), ["a"]);
        let picker =
            linked_picker().from_pull_request("https://github.com/brave/bravebot/pull/127");
        assert!(ids(&picker).is_empty());
    }

    /// An issue link ending in the same number is not a pull request, and typing still narrows
    /// the opened list.
    #[test]
    fn the_pull_request_opening_ignores_issue_links_and_typing_narrows_it_further() {
        let mut picker = linked_picker().from_pull_request("1429");
        assert!(ids(&picker).is_empty());
        picker = linked_picker().from_pull_request("12700");
        typed(&mut picker, "xyz");
        assert!(ids(&picker).is_empty());
    }

    /// A record from before the links were kept has none, and the empty search lists everything.
    #[test]
    fn a_session_without_links_is_listed_and_found_by_its_title() {
        let mut picker = linked_picker();
        assert_eq!(ids(&picker), ["a", "b", "c"]);
        typed(&mut picker, "names none");
        assert_eq!(ids(&picker), ["a", "b", "c"]);
        typed(&mut picker, "xyz");
        assert!(ids(&picker).is_empty());
    }

    #[test]
    fn the_newest_session_is_the_one_under_the_cursor() {
        assert_eq!(picker().chosen().expect("a session").id, "1");
    }

    #[test]
    fn the_arrows_walk_the_list_and_stop_at_its_ends() {
        let mut picker = picker();
        handle_key(&mut picker, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(
            picker.chosen().expect("a session").id,
            "1",
            "walked past the top"
        );

        for _ in 0..5 {
            handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        }
        assert_eq!(
            picker.chosen().expect("a session").id,
            "3",
            "walked past the bottom"
        );
    }

    /// A session is remembered by a word out of the middle of what was asked, not by how the
    /// title starts.
    #[test]
    fn typing_narrows_the_list_by_any_part_of_a_title() {
        let mut picker = picker();
        for c in "PROGRESS".chars() {
            handle_key(&mut picker, KeyCode::Char(c), KeyModifiers::NONE);
        }
        assert_eq!(picker.matching().len(), 1);
        assert_eq!(picker.chosen().expect("a session").id, "2");
    }

    /// Narrowing the list under a cursor that was further down must not leave it pointing past
    /// the end, which would be a picker that resumes nothing when Enter is pressed.
    #[test]
    fn narrowing_the_list_keeps_the_cursor_inside_it() {
        let mut picker = picker();
        handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        for c in "recovery".chars() {
            handle_key(&mut picker, KeyCode::Char(c), KeyModifiers::NONE);
        }
        assert_eq!(picker.chosen().expect("a session").id, "1");
    }

    #[test]
    fn backspace_widens_it_again() {
        let mut picker = picker();
        for c in "zzz".chars() {
            handle_key(&mut picker, KeyCode::Char(c), KeyModifiers::NONE);
        }
        assert!(picker.matching().is_empty());
        assert!(
            picker.chosen().is_none(),
            "a session was chosen from nothing"
        );

        for _ in 0..3 {
            handle_key(&mut picker, KeyCode::Backspace, KeyModifiers::NONE);
        }
        assert_eq!(picker.matching().len(), 3);
    }

    #[test]
    fn escape_leaves_without_resuming_anything() {
        let mut picker = picker();
        assert_eq!(
            handle_key(&mut picker, KeyCode::Esc, KeyModifiers::NONE),
            Outcome::Cancel
        );
        assert_eq!(
            handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Resume
        );
    }

    /// Ctrl-C is the key a user reaches for to get out of anything, and it must not be typed
    /// into the search box as a letter. Escape declines to resume, which leaves a session
    /// running; Ctrl-C asks for the program to end, so the two are not the same answer.
    #[test]
    fn ctrl_c_quits_rather_than_typing_a_letter() {
        let mut picker = picker();
        assert_eq!(
            handle_key(&mut picker, KeyCode::Char('c'), KeyModifiers::CONTROL),
            Outcome::Quit
        );
        assert!(picker.search.is_empty());
    }

    #[test]
    fn an_entry_says_when_where_and_how_much() {
        let line = describe(&summary("1", "anything", sessions_now() - 120));
        assert!(line.contains("2 minutes ago"), "{line}");
        assert!(line.contains("main"), "{line}");
        assert!(line.contains("1.0KB"), "{line}");
    }

    /// A manifest run has no conversation to continue, so Enter must say so rather than
    /// loading an empty session and asking the model to carry on from nothing.
    #[test]
    fn a_manifest_session_cannot_be_resumed() {
        let mut picker = Picker::new(
            vec![manifest_summary("1", "summarise the docs", 100)],
            "/work",
        );
        assert_eq!(
            handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Refused(manifest_note().to_string())
        );
    }

    /// BG-9: a record a running background session holds is refused on Enter in the picker, with
    /// the line that tells the person to attach; a record nothing holds is picked as before.
    #[test]
    fn a_session_a_background_session_holds_cannot_be_picked() {
        fn holds_the_second(id: &str) -> bool {
            id == "2222222222"
        }
        let mut picker = Picker::new(
            vec![
                summary("1111111111", "free", 200),
                summary("2222222222", "held", 100),
            ],
            "/work",
        )
        .with_held(holds_the_second);
        assert_eq!(
            handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Resume
        );
        picker.down();
        let Outcome::Refused(note) = handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE)
        else {
            panic!("a held session was picked");
        };
        assert!(note.contains("bravebot attach 22222222"), "{note}");
    }

    /// SESSION-38: a record another process holds is refused on Enter in the picker, with the line
    /// that names the session and says how to copy it; a record nothing holds is picked as before.
    #[test]
    fn a_session_another_process_holds_cannot_be_picked() {
        fn holds_the_second(_project: &Path, id: &str) -> bool {
            id == "2222222222"
        }
        let mut picker = Picker::new(
            vec![
                summary("1111111111", "free", 200),
                summary("2222222222", "held", 100),
            ],
            "/work",
        )
        .reading(Path::new("/work"))
        .with_open(holds_the_second);
        assert_eq!(
            handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Resume
        );
        picker.down();
        let Outcome::Refused(note) = handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE)
        else {
            panic!("a record another process holds was picked");
        };
        assert!(note.contains("bravebot --fork 2222222222"), "{note}");
    }

    /// SESSION-38: the list `--from-pr` opens refuses a record another process holds, as the plain
    /// list does.
    #[test]
    fn a_session_another_process_holds_cannot_be_picked_from_a_pull_requests_list() {
        fn holds_it(_project: &Path, id: &str) -> bool {
            id == "2222222222"
        }
        let mut held = summary("2222222222", "held", 100);
        held.pull_request = Some("https://github.com/brave/bravebot/pull/12".to_string());
        let mut picker = Picker::new(vec![held], "/work")
            .reading(Path::new("/work"))
            .with_open(holds_it)
            .from_pull_request("12");
        assert_eq!(
            picker.matching().len(),
            1,
            "the list did not offer the session"
        );
        let Outcome::Refused(note) = handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE)
        else {
            panic!("a record another process holds was picked from a pull request's list");
        };
        assert!(note.contains("bravebot --fork 2222222222"), "{note}");
    }

    #[test]
    fn a_manifest_run_is_marked_in_the_list() {
        assert!(describe(&manifest_summary("1", "t", 100)).contains("manifest"));
        assert!(!describe(&summary("1", "t", 100)).contains("manifest"));
    }

    fn sessions_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock")
            .as_secs()
    }

    const DAY: u64 = 24 * 3600;

    fn remembering() -> Picker {
        let now = sessions_now();
        Picker::new(
            vec![
                summary("new", "Tidy the docs", now - DAY),
                summary("mid", "Fix the build", now - 5 * DAY),
                summary("old", "Add retries", now - 30 * DAY),
            ],
            "/work/bravebot",
        )
        .with_corpus(Corpus::of([
            (
                "new".to_string(),
                vec!["move the ledger rounding into one place".to_string()],
            ),
            (
                "mid".to_string(),
                vec!["the Ledger Rounding test flakes on CI".to_string()],
            ),
            ("old".to_string(), vec!["retry the upload".to_string()]),
        ]))
    }

    /// The title is the first thing asked; the session is remembered by something said later.
    #[test]
    fn typing_words_said_in_a_session_finds_it_and_enter_resumes_it() {
        let mut picker = remembering();
        typed(&mut picker, "LEDGER rounding");
        assert_eq!(ids(&picker), ["new", "mid"]);
        assert_eq!(
            handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Resume
        );
        assert_eq!(picker.chosen().expect("a session").id, "new");
    }

    #[test]
    fn a_session_whose_words_do_not_hold_the_phrase_is_left_out() {
        let mut picker = remembering();
        typed(&mut picker, "ledger");
        assert!(!ids(&picker).contains(&"old"));
        typed(&mut picker, " nowhere");
        assert!(picker.matching().is_empty());
    }

    #[test]
    fn backspacing_the_phrase_away_brings_every_session_back() {
        let mut picker = remembering();
        typed(&mut picker, "ledger");
        for _ in 0.."ledger".len() {
            handle_key(&mut picker, KeyCode::Backspace, KeyModifiers::NONE);
        }
        assert_eq!(ids(&picker), ["new", "mid", "old"]);
    }

    #[test]
    fn a_since_word_keeps_only_the_sessions_written_within_it() {
        let mut picker = remembering();
        typed(&mut picker, "since:7d");
        assert_eq!(ids(&picker), ["new", "mid"]);
        typed(&mut picker, " since:2d");
        assert_eq!(ids(&picker), ["new"]);
    }

    #[test]
    fn a_since_word_narrows_the_words_found_and_is_not_part_of_them() {
        let mut picker = remembering();
        typed(&mut picker, "ledger rounding since:2d");
        assert_eq!(ids(&picker), ["new"]);
    }

    /// Halfway through typing the word it is only text, which no session holds.
    #[test]
    fn a_since_word_not_yet_finished_is_typed_text() {
        let mut picker = remembering();
        typed(&mut picker, "since:2");
        assert!(picker.matching().is_empty());
        typed(&mut picker, "d");
        assert_eq!(ids(&picker), ["new"]);
    }

    /// Opening the picker costs what it did before; the records are read for the first phrase.
    #[test]
    fn what_the_sessions_say_is_not_read_until_a_phrase_is_typed() {
        let mut picker = Picker::new(vec![summary("a", "anything", 100)], "/work")
            .reading(Path::new("/work/does-not-exist"));
        assert!(picker.corpus.is_none());
        typed(&mut picker, "w");
        assert!(picker.corpus.is_some());
    }

    #[test]
    fn a_found_session_shows_the_line_that_matched_and_a_title_match_shows_none() {
        let mut picker = remembering();
        typed(&mut picker, "ledger");
        let screen = drawn(&picker);
        assert!(screen.contains("Tidy the docs"), "{screen}");
        assert!(
            screen.contains("move the ledger rounding into one place"),
            "{screen}"
        );

        let mut by_title = remembering();
        typed(&mut by_title, "tidy");
        let screen = drawn(&by_title);
        assert!(screen.contains("Tidy the docs"), "{screen}");
        assert!(!screen.contains("ledger"), "{screen}");
    }

    /// A directory with no sessions is where a plain `--resume` has nothing to offer and starts an
    /// ordinary session, but `--from-pr` there asked whether any session is linked to one pull
    /// request, and a new session that never mentions it does not answer that.
    #[test]
    fn with_no_sessions_only_from_pr_opens_the_list_and_it_says_nothing_matches() {
        if !crate::test_profile::in_isolated_profile() {
            return;
        }
        let project = crate::test_profile::project("never-used");
        std::fs::create_dir_all(&project).expect("a project directory");

        assert!(
            opened(&project, None, None).is_none(),
            "--resume drew an empty list"
        );
        let picker = opened(&project, None, Some("42")).expect("--from-pr drew no list");
        // Wide enough that the scratch profile's long path leaves the pull request on its row.
        let mut terminal =
            Terminal::new(ratatui::backend::TestBackend::new(400, 12)).expect("a terminal");
        terminal
            .draw(|frame| draw(frame, &picker))
            .expect("a frame");
        let screen = terminal.backend().to_string();
        assert!(screen.contains(t!(resume_nothing_matches)), "{screen}");
        assert!(
            screen.contains(&t!(resume_from_pr, pull_request = "42")),
            "{screen}"
        );
    }

    fn mouse(kind: MouseEventKind, row: u16, column: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn pressed(row: u16, column: u16) -> MouseEvent {
        mouse(MouseEventKind::Down(MouseButton::Left), row, column)
    }

    /// Five sessions, newest first, as a list 24 rows tall draws them.
    fn five() -> Picker {
        Picker::new(
            (1..=5)
                .map(|n| summary(&format!("s{n}"), &format!("session {n}"), 600 - n))
                .collect(),
            "/work",
        )
    }

    fn chosen_title(picker: &Picker) -> Option<&str> {
        picker.chosen().map(|session| session.title.as_str())
    }

    /// The row a title is drawn on in a frame 24 rows tall.
    fn row_of_title(picker: &Picker, title: &str) -> u16 {
        let mut terminal =
            Terminal::new(ratatui::backend::TestBackend::new(100, 24)).expect("a terminal");
        terminal.draw(|frame| draw(frame, picker)).expect("a frame");
        let screen = terminal.backend().to_string();
        screen
            .lines()
            .position(|line| line.contains(title))
            .unwrap_or_else(|| panic!("{title} is not on the screen: {screen}")) as u16
    }

    /// MOUSE-15. Rejects: a press that moves nothing or takes at once, a second press that does not
    /// resume, and a session found from a row of another: any of the three rows of an entry picks
    /// that entry, and the one after it starts on the next row.
    #[test]
    fn a_press_on_a_session_moves_the_cursor_and_a_second_takes_it() {
        let mut picker = five();
        let list = list_area(Rect::new(0, 0, 100, 24));
        let (mut last, now) = (None, std::time::Instant::now());
        let third = row_of_title(&picker, "session 3");

        // The title, the line of when and where, and the line of what was said are one entry.
        for offset in 0..3 {
            let mut fresh = five();
            let mut first_press = None;
            let outcome = handle_mouse(
                &mut fresh,
                list,
                pressed(third + offset, 6),
                &mut first_press,
                now,
                3,
            );
            assert_eq!(outcome, Outcome::Continue, "row {offset}");
            assert_eq!(chosen_title(&fresh), Some("session 3"), "row {offset}");
        }
        let outcome = handle_mouse(&mut picker, list, pressed(third + 3, 6), &mut last, now, 3);
        assert_eq!(outcome, Outcome::Continue);
        assert_eq!(chosen_title(&picker), Some("session 4"));

        let outcome = handle_mouse(&mut picker, list, pressed(third + 3, 6), &mut last, now, 3);
        assert_eq!(outcome, Outcome::Resume);
        assert_eq!(chosen_title(&picker), Some("session 4"));
    }

    /// The title of the session whose entry is drawn over `row` of a frame `height` rows tall: an
    /// entry takes three rows and its title is the first.
    fn session_drawn_at(picker: &Picker, row: u16, height: u16) -> Option<String> {
        let mut terminal =
            Terminal::new(ratatui::backend::TestBackend::new(100, height)).expect("a terminal");
        terminal.draw(|frame| draw(frame, picker)).expect("a frame");
        let screen = terminal.backend().to_string();
        let lines: Vec<&str> = screen.lines().collect();
        (0..ENTRY_ROWS as u16).find_map(|up| {
            let line = lines.get(usize::from(row.checked_sub(up)?))?;
            let rest = line.split("session ").nth(1)?;
            Some(format!("session {}", rest.split_whitespace().next()?))
        })
    }

    /// The rows of a frame `height` tall that draw the title of an entry, with the session each
    /// names, top to bottom.
    fn titles_drawn(picker: &Picker, height: u16) -> Vec<(u16, String)> {
        let list = list_area(Rect::new(0, 0, 100, height));
        (list.y..list.bottom())
            .filter_map(|row| {
                let title = session_drawn_at(picker, row, height)?;
                let own =
                    (row.checked_sub(1)).and_then(|above| session_drawn_at(picker, above, height));
                (own.as_deref() != Some(title.as_str()) || row == list.y).then_some((row, title))
            })
            .collect()
    }

    /// A press and a release on a cell, as a terminal sends them. The release takes nothing.
    fn click(
        picker: &mut Picker,
        list: Rect,
        last: &mut Option<Click>,
        now: std::time::Instant,
        (row, column): (u16, u16),
    ) -> Outcome {
        let outcome = handle_mouse(picker, list, pressed(row, column), last, now, 3);
        let released = mouse(MouseEventKind::Up(MouseButton::Left), row, column);
        assert_eq!(
            handle_mouse(picker, list, released, last, now, 3),
            Outcome::Continue
        );
        outcome
    }

    /// Nine sessions with the cursor on the seventh, or five with it on the first.
    fn nine_or_five(scrolled: bool) -> Picker {
        let count = if scrolled { 9 } else { 5 };
        let mut picker = Picker::new(
            (1..=count)
                .map(|n| summary(&format!("s{n}"), &format!("session {n}"), 600 - n))
                .collect(),
            "/work",
        );
        if scrolled {
            for _ in 0..6 {
                picker.down();
            }
        }
        picker
    }

    /// MOUSE-15. Rejects: a double click in place that does not resume the session pointed at, because
    /// a press that moved the cursor to an entry already drawn slid the list, so the second press
    /// landed on another session. On every entry of a list that has not scrolled, and of one that has,
    /// a press and a release, a frame, and a press and a release on the same cell put the cursor on the
    /// session drawn there, leave the entries where they were, and take it.
    #[test]
    fn a_double_click_in_place_takes_the_session_pointed_at_on_every_entry_of_a_list_scrolled_or_not()
     {
        for (scrolled, height) in [(false, 24), (true, 16)] {
            let list = list_area(Rect::new(0, 0, 100, height));
            let before = titles_drawn(&nine_or_five(scrolled), height);
            assert!(before.len() >= 3, "{before:?}");
            assert_eq!(
                nine_or_five(scrolled).matching().len() > before.len(),
                scrolled,
                "{before:?}"
            );
            for (row, title) in &before {
                let mut picker = nine_or_five(scrolled);
                let (mut last, now) = (None, std::time::Instant::now());
                let cell = (*row, 6);

                let first = click(&mut picker, list, &mut last, now, cell);
                assert_eq!(first, Outcome::Continue, "scrolled {scrolled}, {title}");
                assert_eq!(chosen_title(&picker), Some(title.as_str()));
                let after_the_press = titles_drawn(&picker, height);
                let second = click(&mut picker, list, &mut last, now, cell);
                assert_eq!(second, Outcome::Resume, "scrolled {scrolled}, {title}");
                assert_eq!(chosen_title(&picker), Some(title.as_str()));
                assert_eq!(
                    after_the_press, before,
                    "scrolled {scrolled}, {title}: the press slid the list"
                );
            }
        }
    }

    /// MOUSE-15. Rejects: a second press counted with the first when the list moved between them,
    /// which takes a session the first press did not name: the wheel moves the cursor and the list
    /// with it, so the cell holds another session, and a press there is a first press on that one. A
    /// press where the session first named is drawn now is the second press on it, and takes it.
    #[test]
    fn a_press_where_the_wheel_has_since_drawn_another_session_is_a_first_press_on_it() {
        let height = 12;
        let list = list_area(Rect::new(0, 0, 100, height));
        let (mut last, now) = (None, std::time::Instant::now());
        let mut picker = nine_or_five(true);
        let (row, named) = titles_drawn(&picker, height)[0].clone();

        click(&mut picker, list, &mut last, now, (row, 6));
        assert_eq!(chosen_title(&picker), Some(named.as_str()));
        let wheel = |kind| mouse(kind, row, 6);
        handle_mouse(
            &mut picker,
            list,
            wheel(MouseEventKind::ScrollUp),
            &mut last,
            now,
            3,
        );
        let drawn_there_now = session_drawn_at(&picker, row, height).expect("a session");
        assert_ne!(drawn_there_now, named, "the wheel did not move the list");
        let outcome = click(&mut picker, list, &mut last, now, (row, 6));
        assert_eq!(
            outcome,
            Outcome::Continue,
            "a session nobody pressed first was resumed"
        );
        assert_eq!(chosen_title(&picker), Some(drawn_there_now.as_str()));

        let mut picker = nine_or_five(true);
        let mut last = None;
        click(&mut picker, list, &mut last, now, (row, 6));
        handle_mouse(
            &mut picker,
            list,
            wheel(MouseEventKind::ScrollDown),
            &mut last,
            now,
            1,
        );
        let moved_to = titles_drawn(&picker, height)
            .into_iter()
            .find(|(_, title)| *title == named)
            .map(|(at, _)| at)
            .expect("the session first named is still drawn");
        let outcome = click(&mut picker, list, &mut last, now, (moved_to, 6));
        assert_eq!(
            outcome,
            Outcome::Resume,
            "the session first named was not resumed"
        );
        assert_eq!(chosen_title(&picker), Some(named.as_str()));
    }

    /// MOUSE-15. Rejects: a window that stays where a press left it once a key moves the cursor: the
    /// keys and the wheel move it as they always did, so the list a press and a key left draws what
    /// the list the keys alone reach the same session in draws.
    #[test]
    fn a_key_after_a_press_moves_the_list_as_the_keys_alone_do() {
        // Three entries are drawn, so the key moves the cursor to an entry the held list draws
        // and the list the keys alone leave draws another run of them.
        let height = 16;
        let list = list_area(Rect::new(0, 0, 100, height));
        let mut pressed_on = nine_or_five(true);
        let (mut last, now) = (None, std::time::Instant::now());
        let (row, named) = titles_drawn(&pressed_on, height)[0].clone();
        click(&mut pressed_on, list, &mut last, now, (row, 6));
        pressed_on.down();

        let mut by_keys = nine_or_five(true);
        while chosen_title(&by_keys) != Some(named.as_str()) {
            by_keys.up();
        }
        by_keys.down();

        assert_eq!(chosen_title(&pressed_on), chosen_title(&by_keys));
        assert_eq!(
            titles_drawn(&pressed_on, height),
            titles_drawn(&by_keys, height)
        );
    }

    /// MOUSE-15. Rejects: a second press that skips what Enter checks. A manifest run and a record
    /// a running background session or another process holds are refused with the sentence Enter
    /// gives, whichever way the row is taken.
    #[test]
    fn a_second_press_on_a_session_that_cannot_be_resumed_says_why() {
        fn holds_the_second(id: &str) -> bool {
            id == "2222222222"
        }
        let mut picker = Picker::new(
            vec![
                summary("1111111111", "free", 300),
                summary("2222222222", "held", 200),
                manifest_summary("3333333333", "a plan", 100),
            ],
            "/work",
        )
        .with_held(holds_the_second);
        let list = list_area(Rect::new(0, 0, 100, 24));
        let (now, mut last) = (std::time::Instant::now(), None);
        let held = row_of_title(&picker, "held");
        let plan = row_of_title(&picker, "a plan");

        for (row, expected) in [
            (
                held,
                bravebot_session::jobs::held_refusal("2222222222", NAME_SHOWN),
            ),
            (plan, manifest_note().to_string()),
        ] {
            handle_mouse(&mut picker, list, pressed(row, 5), &mut last, now, 3);
            let outcome = handle_mouse(&mut picker, list, pressed(row, 5), &mut last, now, 3);
            assert_eq!(outcome, Outcome::Refused(expected), "row {row}");
        }
    }

    /// MOUSE-15. Rejects: a press on the heading, the search box, the project, the line of keys or
    /// below the last session choosing or moving the cursor, even twice, and another button, a drag
    /// or a release acting.
    #[test]
    fn a_press_off_the_sessions_and_any_other_mouse_event_leave_the_list_alone() {
        let mut picker = five();
        for _ in 0..2 {
            picker.down();
        }
        let list = list_area(Rect::new(0, 0, 100, 24));
        let (now, mut last) = (std::time::Instant::now(), None);
        let last_row = row_of_title(&picker, "session 5") + 3;
        for row in [0, 1, 3, 4, last_row, 23] {
            for _ in 0..2 {
                let outcome = handle_mouse(&mut picker, list, pressed(row, 4), &mut last, now, 3);
                assert_eq!(outcome, Outcome::Continue, "row {row}");
            }
        }
        for kind in [
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Down(MouseButton::Middle),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Moved,
        ] {
            let outcome = handle_mouse(
                &mut picker,
                list,
                mouse(kind, list.y + 1, 4),
                &mut last,
                now,
                3,
            );
            assert_eq!(outcome, Outcome::Continue, "{kind:?}");
        }
        assert_eq!(chosen_title(&picker), Some("session 3"));
    }

    /// MOUSE-15. Rejects: counting entries from the top of the list when it has scrolled to keep the
    /// cursor in view: with the cursor on the fifth of nine sessions and room for two, each title
    /// drawn is the session a press on it picks.
    #[test]
    fn each_session_drawn_is_the_one_a_press_on_it_picks_in_a_list_that_has_scrolled() {
        let nine = || {
            Picker::new(
                (1..=9)
                    .map(|n| summary(&format!("s{n}"), &format!("session {n}"), 600 - n))
                    .collect(),
                "/work",
            )
        };
        let mut scrolled = nine();
        for _ in 0..4 {
            scrolled.down();
        }
        let mut terminal =
            Terminal::new(ratatui::backend::TestBackend::new(100, 12)).expect("a terminal");
        terminal
            .draw(|frame| draw(frame, &scrolled))
            .expect("a frame");
        let screen = terminal.backend().to_string();
        let list = list_area(Rect::new(0, 0, 100, 12));
        let mut drawn = 0;
        for (row, line) in screen.lines().enumerate() {
            let entry = line.trim_start_matches(['"', ' ', '❯']);
            if !entry.starts_with("session ")
                || !entry["session ".len()..].starts_with(|c: char| c.is_ascii_digit())
            {
                continue;
            }
            let title: String = entry
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ");
            let mut picker = scrolled_like(&nine, 4);
            let (now, mut last) = (std::time::Instant::now(), None);
            handle_mouse(&mut picker, list, pressed(row as u16, 4), &mut last, now, 3);
            assert_eq!(
                chosen_title(&picker),
                Some(title.as_str()),
                "row {row}: {line}"
            );
            drawn += 1;
        }
        assert!(drawn >= 2, "the list drew {drawn} sessions: {screen}");
    }

    fn scrolled_like(make: &dyn Fn() -> Picker, down: usize) -> Picker {
        let mut picker = make();
        for _ in 0..down {
            picker.down();
        }
        picker
    }

    /// MOUSE-46. Rejects: a wheel turn that moves the cursor the wrong way, by another step than the
    /// one asked, past either end, or that resumes or leaves.
    #[test]
    fn the_wheel_moves_the_cursor_by_the_sessions_asked_and_stops_at_both_ends() {
        let mut picker = five();
        let list = list_area(Rect::new(0, 0, 100, 24));
        let (now, mut last) = (std::time::Instant::now(), None);
        let mut turn = |picker: &mut Picker, kind, wheel| {
            let outcome = handle_mouse(picker, list, mouse(kind, 0, 0), &mut last, now, wheel);
            assert_eq!(outcome, Outcome::Continue, "{kind:?}");
            chosen_title(picker).map(str::to_string)
        };
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollDown, 2).as_deref(),
            Some("session 3")
        );
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollUp, 1).as_deref(),
            Some("session 2")
        );
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollDown, 50).as_deref(),
            Some("session 5")
        );
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollUp, 50).as_deref(),
            Some("session 1")
        );
    }

    /// MOUSE-44. Rejects: the question of deleting a record answered or put away by a press, a
    /// double press that resumes the session behind it, or the wheel moving the cursor off the
    /// record the question is about. The question is answered by the next key and by nothing else.
    #[test]
    fn while_the_question_of_deleting_stands_the_pointer_does_nothing() {
        let mut picker = five();
        picker.down();
        assert_eq!(
            handle_key(&mut picker, KeyCode::Delete, KeyModifiers::NONE),
            Outcome::Continue
        );
        assert_eq!(picker.deleting.as_deref(), Some("s2"));
        let list = list_area(Rect::new(0, 0, 100, 24));
        let (now, mut last) = (std::time::Instant::now(), None);
        let first = row_of_title(&picker, "session 1");

        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollUp,
        ] {
            let outcome = handle_mouse(&mut picker, list, mouse(kind, first, 5), &mut last, now, 3);
            assert_eq!(outcome, Outcome::Continue, "{kind:?}");
            assert_eq!(picker.deleting.as_deref(), Some("s2"), "{kind:?}");
        }
        assert_eq!(chosen_title(&picker), Some("session 2"));
        assert_eq!(
            handle_key(&mut picker, KeyCode::Char('y'), KeyModifiers::NONE),
            Outcome::Delete("s2".to_string())
        );
    }

    /// MOUSE-15, MOUSE-46, through the loop the list runs. Rejects: a loop that reads only keys. A
    /// press and the wheel move the cursor, and the record the question of deleting names is the one
    /// the cursor was on when the question was asked.
    #[test]
    fn the_list_reads_the_pointer_before_the_question_of_deleting_is_asked() {
        let (width, height) = (100, 24);
        let list = list_area(Rect::new(0, 0, width, height));
        let down = MouseEventKind::Down(MouseButton::Left);
        let key = |code| {
            TermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                code,
                KeyModifiers::NONE,
            ))
        };
        let third = list.y + 6;
        let run = |events: Vec<TermEvent>| {
            let _held = crate::input::script::hold(events);
            let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(width, height))
                .expect("a terminal");
            let deleted = std::cell::RefCell::new(Vec::new());
            converse(&mut terminal, Path::new("/work"), five(), 2, |_, id| {
                deleted.borrow_mut().push(id.to_string());
                Ok(())
            });
            deleted.into_inner()
        };
        let by_press = vec![
            TermEvent::Mouse(mouse(down, third, 5)),
            key(KeyCode::Delete),
            key(KeyCode::Char('y')),
            key(KeyCode::Esc),
        ];
        assert_eq!(run(by_press), ["s3"]);
        let by_wheel = vec![
            TermEvent::Mouse(mouse(MouseEventKind::ScrollDown, 0, 0)),
            key(KeyCode::Delete),
            key(KeyCode::Char('y')),
            key(KeyCode::Esc),
        ];
        assert_eq!(run(by_wheel), ["s3"]);
    }
}
