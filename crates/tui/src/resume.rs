//! Choosing a session to pick up again.
//!
//! Shown by `bravebot --resume`, before anything else happens. The list is this working directory's
//! sessions, newest first, because the one being looked for is nearly always the last one.
//!
//! Typing filters rather than jumping, since a title is remembered as a few words out of the
//! middle of it rather than as the way it starts. The branch and the issue and pull request the
//! person linked are searched as well, and `--from-pr` opens the list already narrowed to the
//! sessions linked to one pull request. Escape leaves without resuming anything, which
//! starts an ordinary session: nothing here can strand a user who opened it by mistake.

use crate::input;
use crate::theme;
use bravebot_i18n::t;
use bravebot_session::sessions::{self, Summary};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use std::path::Path;

/// What the picker is showing and where the cursor is.
#[derive(Debug)]
pub struct Picker {
    /// Every session, newest first.
    sessions: Vec<Summary>,
    /// What has been typed to narrow it.
    search: String,
    /// A refusal to show under the list, cleared by the next key.
    note: Option<&'static str>,
    /// Which of the matching sessions is under the cursor.
    selected: usize,
    /// The project these sessions belong to, for the heading.
    project: String,
    /// A pull request number or address the list was opened for, which keeps only the sessions
    /// linked to it before anything typed narrows further.
    from_pr: Option<String>,
    /// Whether it is drawn over a running session, where leaving it stays in that session.
    within_a_session: bool,
}

impl Picker {
    pub fn new(sessions: Vec<Summary>, project: impl Into<String>) -> Self {
        Self {
            sessions,
            search: String::new(),
            note: None,
            selected: 0,
            project: project.into(),
            from_pr: None,
            within_a_session: false,
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
    pub fn matching(&self) -> Vec<&Summary> {
        let needle = self.search.to_lowercase();
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
                words || linked
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

    fn down(&mut self) {
        let last = self.matching().len().saturating_sub(1);
        self.selected = (self.selected + 1).min(last);
    }

    fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Narrow the list, keeping the cursor inside it.
    fn typed(&mut self, c: char) {
        self.search.push(c);
        self.clamp();
    }

    fn backspace(&mut self) {
        self.search.pop();
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
    Refused(&'static str),
}

/// Why a manifest run cannot be picked up.
///
/// A session is turns over one conversation and a manifest run has none: the planner is never
/// shown a result, so there is nothing for a later turn to continue. The record is written all
/// the same, because what a run produced is worth reading whether or not it can be resumed.
pub fn manifest_note() -> &'static str {
    t!(resume_manifest_run)
}

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

    match code {
        KeyCode::Esc => Outcome::Cancel,
        KeyCode::Enter => match picker.chosen() {
            Some(session) if session.manifest => Outcome::Refused(manifest_note()),
            _ => Outcome::Resume,
        },
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
) -> Choice {
    pick(terminal, project, None, from_pr)
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
}

impl Refusal {
    /// What to say in the transcript. The id typed is never part of it, since it may hold an escape.
    pub fn note(&self) -> String {
        match self {
            Self::NoSuchSession => t!(session_resume_no_such).to_string(),
            Self::AlreadyHere => t!(session_resume_already_here).to_string(),
            Self::Manifest => manifest_note().to_string(),
            Self::Held => t!(session_resume_held_by_background).to_string(),
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
        .and_then(continuable)
}

/// The record, if it is one that can be carried on from: neither a manifest run nor one a running
/// background session holds. The picker refuses the first on Enter; the second is checked here
/// because a record is picked up in this process and not another.
pub fn continuable(record: Box<sessions::Record>) -> Result<Box<sessions::Record>, Refusal> {
    continuable_unless(record, bravebot_session::jobs::is_running)
}

/// [`continuable`], with the question of whether a running background session holds an id put by
/// the caller, so that the refusal can be reached without one.
pub fn continuable_unless(
    record: Box<sessions::Record>,
    held: impl Fn(&str) -> bool,
) -> Result<Box<sessions::Record>, Refusal> {
    if record.manifest.is_some() {
        return Err(Refusal::Manifest);
    }
    if held(&record.id) {
        return Err(Refusal::Held);
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
) -> Choice {
    pick(terminal, project, Some(current), None)
}

fn pick<B: Backend>(
    terminal: &mut Terminal<B>,
    project: &Path,
    current: Option<&str>,
    from_pr: Option<&str>,
) -> Choice {
    let mut listed = sessions::list(project);
    listed.retain(|session| Some(session.id.as_str()) != current);
    let mut picker = Picker::new(listed, project.display().to_string());
    picker.within_a_session = current.is_some();
    if let Some(wanted) = from_pr {
        picker = picker.from_pull_request(wanted);
    }
    if picker.is_empty() {
        return Choice::Fresh;
    }

    loop {
        if terminal.draw(|frame| draw(frame, &picker)).is_err() {
            return Choice::Fresh;
        }

        let Ok(event) = input::read() else {
            return Choice::Fresh;
        };
        let Some(key) = input::key_of(&event) else {
            continue;
        };
        // A key event arrives twice on Windows, once pressed and once released, and the release
        // would otherwise choose whatever the press had just moved to.
        if key.kind != event::KeyEventKind::Press {
            continue;
        }

        // Cleared on every key, so a refusal stays up until the user does something and no
        // longer than that.
        picker.note = None;

        match handle_key(&mut picker, key.code, key.modifiers) {
            Outcome::Continue => continue,
            Outcome::Cancel => return Choice::Fresh,
            Outcome::Quit => return Choice::Quit,
            Outcome::Refused(note) => {
                picker.note = Some(note);
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

/// Draw the list.
fn draw(frame: &mut Frame, picker: &Picker) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // heading
            Constraint::Length(3), // search box
            Constraint::Length(1), // project
            Constraint::Min(1),    // list
            Constraint::Length(1), // keys
        ])
        .split(area);

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

    let (footer, colour) = match picker.note {
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

/// The list itself: two lines per session, and a word when nothing matches.
fn list_lines(picker: &Picker, area: Rect) -> Vec<Line<'static>> {
    let matching = picker.matching();
    if matching.is_empty() {
        return vec![Line::from(Span::styled(
            format!("  {}", t!(resume_nothing_matches)),
            Style::default().fg(theme::muted()),
        ))];
    }

    // Three rows per entry, so the window is what fits rather than what exists.
    let visible = (area.height as usize / 3).max(1);
    let first = picker.selected.saturating_sub(visible.saturating_sub(1));

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
        lines.push(Line::raw(""));
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
            branch: Some("main".to_string()),
            issue: None,
            pull_request: None,
            updated,
            bytes: 1024,
            manifest: false,
        }
    }

    fn manifest_summary(id: &str, title: &str, updated: u64) -> Summary {
        Summary {
            id: id.to_string(),
            title: title.to_string(),
            branch: Some("main".to_string()),
            issue: None,
            pull_request: None,
            updated,
            bytes: 1024,
            manifest: true,
        }
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

    fn linked(id: &str, branch: &str, issue: Option<&str>, pull_request: Option<&str>) -> Summary {
        Summary {
            branch: Some(branch.to_string()),
            issue: issue.map(str::to_string),
            pull_request: pull_request.map(str::to_string),
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
            Outcome::Refused(manifest_note())
        );
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
}
