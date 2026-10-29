//! The Sessions screen: every session on the machine in one list, grouped, filtered and
//! searched, with the keys that open, make, rename, stop, archive, delete, pin and hide one
//! (SCREEN-1).
//!
//! This module is the state and the keys, and draws nothing: [`crate::render`] draws it, and
//! `app` carries out what a key asks for, because opening or making a session takes the terminal.
//! Every field a row holds is a name the person typed, a fact about a process or a file, or one of
//! a fixed set of words (SCREEN-2). The strings among them are pictured on the way in, so a control
//! character in a name or a model cannot reach the screen as one.

mod worker;

use crate::render::printable;
use crate::sessions_keys::SessionsKeys;
use bravebot_session::jobs::Roster;
use bravebot_session::listing::{Ask, Group, Grouping, Page, Reader, Task};
use bravebot_session::search::{Corpus, Query};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use worker::{PageAsk, Reply, SaidAsk, Worker};

/// How many sessions the Sessions screen asks for, and how many more each press of "Show more" adds.
pub const PAGE: usize = 40;

/// How long between two reads of the machine's sessions while the Sessions screen is open.
pub const REFRESH: Duration = Duration::from_secs(2);

/// How often the loop looks for what the worker has found while a read is under way.
const ANSWER_POLL: Duration = Duration::from_millis(50);

/// The tabs: the sets a task can be counted in and filtered to, in the order they are drawn. The
/// first is every task.
pub const TABS: [Option<Group>; 5] = [
    None,
    Some(Group::NeedsYou),
    Some(Group::Working),
    Some(Group::Ready),
    Some(Group::Inactive),
];

/// The word for a status, as the Sessions screen and the list `bravebot sessions` open both say it
/// (SCREEN-16).
pub fn status_word(status: bravebot_session::listing::Status) -> &'static str {
    crate::render::session_status_word(status)
}

/// A status with, for a session holding a prompt, the word for the kind held (BG-5).
pub fn status_in_words(
    status: bravebot_session::listing::Status,
    held: Option<bravebot_session::jobs::Held>,
) -> String {
    crate::render::session_status_in_words(status, held)
}

/// What a click pointed at.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pressed {
    Row(String),
    ShowMore,
}

/// How much of an id is drawn, which is enough to tell one session of a list from another.
const SHORT_ID: usize = 8;

/// The issue and pull request links of a row, lowercased.
fn links_of(row: &Row) -> Vec<String> {
    [row.issue.as_deref(), row.pull_request.as_deref()]
        .into_iter()
        .flatten()
        .map(str::to_lowercase)
        .collect()
}

/// Whether the session's pull request is the one `wanted` names: its address, or its number, which
/// is the last part of the address on GitHub, GitLab and Bitbucket alike. A number is not matched
/// as a substring, so `127` does not find the session whose pull request is `1270`.
fn is_pull_request(task: &Task, wanted: &str) -> bool {
    let Some(link) = task.pull_request.as_deref() else {
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

/// One task as the Sessions screen draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The task as the session crate read it. Its strings are not drawn: the fields below are.
    pub task: Task,
    pub group: Group,
    /// The name, to its first line, pictured.
    pub title: String,
    /// The directory the session ran in, pictured.
    pub directory: String,
    /// The model that answered, pictured, or none.
    pub model: Option<String>,
    pub branch: Option<String>,
    /// The agent definition the session was started with, pictured, or none.
    pub agent: Option<String>,
    /// The last prompt the person typed, pictured, for a session with a roster entry.
    pub prompt: Option<String>,
    /// The issue and the pull request the person linked, pictured.
    pub issue: Option<String>,
    pub pull_request: Option<String>,
    /// The start of the id of the session this one was handed off from.
    pub handed_off_from: Option<String>,
    /// What the session takes up on disk, in the units a person reads.
    pub size: String,
    /// The start of the id, which tells one session of a list from another.
    pub short_id: String,
    /// What a search matches against, lowercased.
    haystack: String,
}

impl Row {
    pub fn of(task: Task) -> Self {
        let title = printable(task.title.lines().next().unwrap_or("").trim());
        let directory = printable(&task.directory);
        let model = task.model.as_deref().map(printable);
        let branch = task.branch.as_deref().map(printable);
        let prompt = task.prompt.as_deref().map(printable);
        let agent = task.agent.as_deref().map(printable);
        let issue = task.issue.as_deref().map(printable);
        let pull_request = task.pull_request.as_deref().map(printable);
        let handed_off_from = task
            .handed_off_from
            .as_deref()
            .map(|id| id.chars().take(SHORT_ID).collect::<String>());
        // A session with no record yet takes up nothing worth saying.
        let size = match task.bytes {
            0 => String::new(),
            bytes => bravebot_session::sessions::size(bytes),
        };
        let short_id = task.id.chars().take(SHORT_ID).collect::<String>();
        let haystack = [
            Some(title.as_str()),
            prompt.as_deref(),
            Some(directory.as_str()),
            branch.as_deref(),
            agent.as_deref(),
            Some(task.id.as_str()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
        Self {
            group: task.status.group(),
            task,
            title,
            directory,
            model,
            branch,
            agent,
            prompt,
            issue,
            pull_request,
            handed_off_from,
            size,
            short_id,
            haystack,
        }
    }
}

/// What a row of the drawn list is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// The heading above the group the row at this index starts.
    Group(usize),
    Task(usize),
    /// The line of what the session at this index said that holds the search, under its row.
    Said(usize),
    /// The blank line between two groups.
    Gap,
    ShowMore,
}

/// What a key asked for beyond changing the Sessions screen's own state, for `app` to carry out.
///
/// The tasks are boxed, so that an effect is not the size of a task in every action that may carry
/// one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Carry on in this task: the conversation, a session open here, a record to resume, or a
    /// background process to attach to.
    Open(Box<Task>),
    /// Begin a session in the directory of the task under the cursor, or in this one's.
    New(Option<String>),
    /// Copy the task's record and carry on in the copy.
    Fork(Box<Task>),
    /// End the process running the task.
    Stop(Box<Task>),
    Rename {
        task: Box<Task>,
        name: String,
    },
    Archive(Box<Task>),
    Delete(Box<Task>),
    /// Read more of the machine's sessions.
    ShowMore,
    /// Save how the list is grouped.
    Grouping(Grouping),
    /// Save which tasks are pinned.
    Pins(Vec<String>),
    /// Stop the turn that is running, then carry out what is boxed, which the question over the
    /// list asked for and `y` answered (SCREEN-15).
    StopTurnThen(Box<Effect>),
}

/// What a key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing changed, so nothing is drawn again.
    Nothing,
    Redraw,
    /// Leave the Sessions screen for the conversation.
    Close,
    Effect(Effect),
}

/// What a confirmation is asked about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destructive {
    Archive,
    Delete,
    /// Stop the turn that is running so that what `then` names can be done (SCREEN-15).
    Interrupt,
}

/// The question put before an archive, a delete, or the stop of a turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub task: Task,
    pub what: Destructive,
    /// What is carried out once the turn has been stopped, where `what` is
    /// [`Destructive::Interrupt`].
    pub then: Option<Box<Effect>>,
}

/// The line being typed to rename a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renaming {
    pub task: Task,
    pub text: String,
}

/// What a click can land on (SCREEN-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// The row at this index of [`SessionsScreen::rows`].
    Row(usize),
    ShowMore,
    /// The tab at this index of [`TABS`].
    Tab(usize),
}

/// The Sessions screen, while it is open.
#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the independent states of one screen: a search, a help, a read"
)]
pub struct SessionsScreen {
    rows: Vec<Row>,
    /// Where each row was last put within its group, so a refresh keeps the rows that were there
    /// where they were and adds the new ones after them (SCREEN-6).
    rank: HashMap<String, (Group, usize)>,
    /// The task the cursor is on, by id, which a refresh cannot move.
    selected: Option<String>,
    /// Whether the cursor is on "Show more".
    on_show_more: bool,
    more: bool,
    /// How many records the list reads past the ones that are running, which "Show more" raises.
    paged: usize,
    tab: usize,
    grouping: Grouping,
    search: String,
    searching: bool,
    /// The keys the list answers, which a settings file can move (SCREEN-14).
    keys: SessionsKeys,
    /// The thread that reads the records, once the screen is opened over a session, and not in a
    /// test of the state alone (SCREEN-17).
    worker: Option<Worker>,
    /// What the sessions say, as a test gives it instead of reading it (SESSION-33).
    corpora: Option<(HashSet<String>, Vec<Corpus>)>,
    /// For each session whose words hold the phrase, the line that does, and the phrase it was
    /// found for, so that a line is never drawn for a phrase it was not found for.
    said: (String, HashMap<String, String>),
    /// A pull request number or address the list is narrowed to (`--from-pr`).
    from_pr: Option<String>,
    renaming: Option<Renaming>,
    confirm: Option<Confirm>,
    help: bool,
    pinned: Vec<String>,
    hidden: HashSet<String>,
    archived: HashMap<String, u64>,
    /// The session the terminal is in, which the cursor starts on.
    current: Option<String>,
    /// The directory that session works in, which the project layout puts first.
    here: Option<PathBuf>,
    /// Where each project stands in the project layout, fixed when it is first seen so that a group
    /// does not move because one of its sessions was written (SCREEN-6).
    group_rank: HashMap<String, usize>,
    /// What the last click pointed at, which a key or a turn of the wheel lets go of.
    pressed: Option<Pressed>,
    /// Said under the list until the next key.
    notice: Option<String>,
    reader: Reader,
    next_read: Instant,
    /// Whether the first read has been made, so an empty list can tell "nothing yet" from "not read".
    read: bool,
    scroll: Cell<usize>,
    page_height: Cell<usize>,
}

impl SessionsScreen {
    /// A Sessions screen over no tasks yet, with the preferences the person last left.
    pub fn new(
        grouping: Grouping,
        pinned: Vec<String>,
        archived: HashMap<String, u64>,
        current: Option<String>,
    ) -> Self {
        Self {
            rows: Vec::new(),
            rank: HashMap::new(),
            selected: current.clone(),
            on_show_more: false,
            more: false,
            paged: PAGE,
            tab: 0,
            grouping,
            search: String::new(),
            searching: false,
            keys: SessionsKeys::default(),
            worker: None,
            corpora: None,
            said: (String::new(), HashMap::new()),
            from_pr: None,
            renaming: None,
            confirm: None,
            help: false,
            pinned,
            hidden: HashSet::new(),
            archived,
            current,
            here: None,
            group_rank: HashMap::new(),
            pressed: None,
            notice: None,
            reader: Reader::new(),
            next_read: Instant::now(),
            read: false,
            scroll: Cell::new(0),
            page_height: Cell::new(0),
        }
    }

    /// Open with the search box focused, so that what is typed narrows the list at once (`--resume`
    /// and `/resume`), and not with the cursor on the first row.
    pub fn typing_first(mut self) -> Self {
        self.searching = true;
        self
    }

    /// Answer these keys in place of the defaults (SCREEN-14).
    pub fn with_keys(mut self, keys: SessionsKeys) -> Self {
        self.keys = keys;
        self
    }

    /// The keys the list answers.
    pub fn keys(&self) -> &SessionsKeys {
        &self.keys
    }

    /// Search what the sessions say as well as their names, reading it from disk the first time a
    /// phrase is typed.
    pub fn reading(mut self) -> Self {
        self.worker = Some(Worker::start(bravebot_session::sessions::is_held_in));
        self
    }

    /// [`SessionsScreen::reading`], with the thread that reads supplied by a test.
    #[cfg(test)]
    pub(crate) fn with_worker(mut self, worker: Worker) -> Self {
        self.worker = Some(worker);
        self
    }

    /// Search these lines for what the sessions say, instead of reading them from disk.
    #[cfg(test)]
    pub(crate) fn with_corpus(mut self, corpus: Corpus) -> Self {
        self.corpora = Some((HashSet::new(), vec![corpus]));
        self
    }

    /// [`SessionsScreen::with_corpus`], for a test in another module.
    #[cfg(test)]
    pub(crate) fn set_corpus_for_test(&mut self, corpus: Corpus) {
        self.corpora = Some((HashSet::new(), vec![corpus]));
    }

    /// Keep only the sessions linked to a pull request, given as its number or its address
    /// (`--from-pr`).
    pub fn from_pull_request(mut self, wanted: impl Into<String>) -> Self {
        self.from_pr = Some(wanted.into());
        self
    }

    /// The pull request the list is narrowed to.
    pub fn pull_request(&self) -> Option<&str> {
        self.from_pr.as_deref()
    }

    /// What is typed as a query. A `since:` word that is not one is typed text like any other.
    fn query(&self) -> Query {
        Query::parse(&self.search).unwrap_or_else(|_| Query::plain(&self.search))
    }

    /// The line of this session's words that holds what is typed, if one does.
    pub fn said_in(&self, row: &Row) -> Option<&str> {
        if self.said.0 != self.query().phrase() {
            return None;
        }
        self.said.1.get(&row.task.id).map(String::as_str)
    }

    /// Find the sessions whose words hold what has been typed.
    fn search_words(&mut self) {
        let query = self.query();
        // The lines found for this phrase stay while the worker is asked again, so that a refresh does
        // not take away the rows that matched only by what was said until it answers.
        if self.worker.is_none() || self.said.0 != query.phrase() {
            self.said = (query.phrase().to_string(), HashMap::new());
        }
        if !query.has_phrase() {
            return;
        }
        // The records are read on the worker, and the lines found are applied when they arrive.
        if let Some(worker) = &mut self.worker {
            worker.ask_said(SaidAsk {
                query,
                rows: self
                    .rows
                    .iter()
                    .map(|row| (row.task.id.clone(), row.task.directory.clone()))
                    .collect(),
            });
            return;
        }
        let Some((_, corpora)) = &self.corpora else {
            return;
        };
        let mut found = HashMap::new();
        for row in &self.rows {
            if let Some(line) = corpora
                .iter()
                .find_map(|corpus| corpus.found(&row.task.id, &query))
            {
                found.insert(row.task.id.clone(), line);
            }
        }
        self.said.1 = found;
    }

    /// The directory the session this Sessions screen was opened over works in.
    pub fn in_directory(mut self, directory: PathBuf) -> Self {
        self.here = Some(directory);
        self
    }

    /// Whether `directory` is the one the session this Sessions screen was opened over works in.
    pub fn is_here(&self, directory: &str) -> bool {
        self.here.as_deref() == Some(Path::new(directory))
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn grouping(&self) -> Grouping {
        self.grouping
    }

    pub fn tab(&self) -> usize {
        self.tab
    }

    pub fn has_more(&self) -> bool {
        self.more
    }

    pub fn has_read(&self) -> bool {
        self.read
    }

    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn searching(&self) -> bool {
        self.searching
    }

    pub fn renaming(&self) -> Option<&Renaming> {
        self.renaming.as_ref()
    }

    pub fn confirming(&self) -> Option<&Confirm> {
        self.confirm.as_ref()
    }

    pub fn helping(&self) -> bool {
        self.help
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn say(&mut self, notice: impl Into<String>) {
        self.notice = Some(notice.into());
    }

    pub fn pinned(&self) -> &[String] {
        &self.pinned
    }

    pub fn archived(&self) -> &HashMap<String, u64> {
        &self.archived
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// Take back what the person hid in this Sessions screen, for the next one opened in the same session.
    pub fn take_hidden(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.hidden)
    }

    /// Start from what was hidden in an earlier Sessions screen of this session (SCREEN-10).
    pub fn hide_these(&mut self, hidden: HashSet<String>) {
        self.hidden = hidden;
        let hidden = &self.hidden;
        self.rows.retain(|row| !hidden.contains(&row.task.id));
        self.reconcile();
    }

    /// Whether a line is being typed, a question is up or the help is shown, so that a click or a
    /// turn of the wheel is not for the list under it.
    pub fn modal(&self) -> bool {
        self.renaming.is_some() || self.confirm.is_some() || self.help
    }

    pub fn on_show_more(&self) -> bool {
        self.on_show_more
    }

    pub fn scroll(&self) -> &Cell<usize> {
        &self.scroll
    }

    pub fn page_height(&self) -> &Cell<usize> {
        &self.page_height
    }

    /// Whether a line is being typed, so the letters are the line's and not the Sessions screen's.
    pub fn editing(&self) -> bool {
        self.searching || self.renaming.is_some()
    }

    /// How many tasks the tab at `tab` counts. Every task is counted, whatever a search has
    /// narrowed the list to.
    pub fn count(&self, tab: usize) -> usize {
        self.rows
            .iter()
            .filter(|row| TABS[tab].is_none_or(|group| group == row.group))
            .count()
    }

    // ---- reading the machine

    /// Whether it is time to read the machine's sessions again.
    pub fn due(&self, now: Instant) -> bool {
        now >= self.next_read || self.worker.as_ref().is_some_and(Worker::busy)
    }

    /// Apply what the worker has answered since the last pass. Whether anything changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Some(reply) = self.worker.as_mut().and_then(Worker::answer) {
            match reply {
                Reply::Page(page) => self.replace(page),
                // Only for the text still in the box: an answer to text that has changed is dropped.
                Reply::Said { phrase, lines } => {
                    if phrase == self.query().phrase() {
                        self.said = (phrase, lines);
                    }
                }
            }
            changed = true;
        }
        changed
    }

    /// Ask the worker for the list, and for the next one a refresh later. Without a worker the list is
    /// read here and now. Whether it was, so that the list has changed.
    pub fn read_later(
        &mut self,
        root: &Path,
        roster: Option<&Roster>,
        working: Option<bool>,
        open: impl Fn(&Path, &str) -> bool,
        now: Instant,
    ) -> bool {
        let Some(worker) = &mut self.worker else {
            self.refresh(root, roster, working, open, now);
            return true;
        };
        let sent = worker.ask_page(PageAsk {
            root: root.to_path_buf(),
            current: self.current.clone().zip(working),
            wanted: match self.search.is_empty() {
                true => self.paged,
                false => usize::MAX,
            },
            archived: self.archived.clone(),
        });
        if sent {
            self.next_read = now + REFRESH;
        }
        false
    }

    /// Whether it is time for the next read of the list.
    pub fn read_due(&self, now: Instant) -> bool {
        now >= self.next_read
    }

    /// Whether the list is read on a thread of its own, which reads the roster too.
    pub fn reads_apart(&self) -> bool {
        self.worker.is_some()
    }

    /// How long until the next read is due.
    pub fn due_in(&self, now: Instant) -> Duration {
        match self.worker.as_ref().is_some_and(Worker::busy) {
            // Asked again soon, to take what the worker has found.
            true => ANSWER_POLL,
            false => self.next_read.saturating_duration_since(now),
        }
    }

    /// Read the machine's sessions under `root` and take them in.
    pub fn refresh(
        &mut self,
        root: &Path,
        roster: Option<&Roster>,
        working: Option<bool>,
        open: impl Fn(&Path, &str) -> bool,
        now: Instant,
    ) {
        let current = self
            .current
            .as_deref()
            .zip(working)
            .map(|(id, working)| bravebot_session::listing::Current { id, working });
        let ask = Ask {
            root,
            roster,
            current,
            wanted: self.wanted(),
        };
        let archived = std::mem::take(&mut self.archived);
        let page = self.reader.read(&ask, open, |id, updated| {
            bravebot_session::listing::is_archived(&archived, id, updated)
        });
        self.archived = archived;
        self.next_read = now + REFRESH;
        self.replace(page);
    }

    /// How many records the next read takes: a page, or all of them while something is typed in a
    /// search, since a search that covered only the rows read would miss the session being looked
    /// for (SCREEN-2).
    pub(crate) fn wanted(&self) -> usize {
        match self.search.is_empty() {
            true => self.paged,
            false => usize::MAX,
        }
    }

    /// Ask for more of the machine's sessions at the next read, and read at once.
    pub fn want_more(&mut self, now: Instant) {
        self.paged += PAGE;
        self.next_read = now;
    }

    /// Take a page of tasks in, keeping the rows that were here where they were while they stay in
    /// the group they were in (SCREEN-6).
    pub fn replace(&mut self, page: Page) {
        let mut rows: Vec<Row> = page
            .tasks
            .into_iter()
            .filter(|task| !self.hidden.contains(&task.id))
            .filter(|task| {
                self.from_pr
                    .as_deref()
                    .is_none_or(|wanted| is_pull_request(task, wanted))
            })
            .map(Row::of)
            .collect();
        rows.sort_by_key(|row| {
            let rank = self
                .rank
                .get(&row.task.id)
                .filter(|(group, _)| *group == row.group)
                .map_or(usize::MAX, |(_, rank)| *rank);
            (row.group, rank)
        });
        self.rank = rows
            .iter()
            .enumerate()
            .map(|(rank, row)| (row.task.id.clone(), (row.group, rank)))
            .collect();
        self.rows = rows;
        self.more = page.more;
        if self.query().has_phrase() {
            self.search_words();
        }
        self.rank_new_groups();
        let first_read = !self.read;
        self.read = true;
        self.reconcile();
        // What is waiting on the person is where the cursor starts when there is anything, and the
        // session they came from when there is not (SCREEN-6).
        if first_read
            && let Some(waiting) = self
                .visible()
                .into_iter()
                .find(|index| self.rows[*index].group == Group::NeedsYou)
        {
            self.selected = Some(self.rows[waiting].task.id.clone());
            self.on_show_more = false;
        }
    }

    /// Lay the list out as `grouping`. The headings of a layout are placed again, the newest first,
    /// when it is chosen.
    pub(crate) fn group_by(&mut self, grouping: Grouping) {
        self.grouping = grouping;
        self.group_rank.clear();
        self.rank_new_groups();
    }

    /// The heading a row is under in the layout it is in: its directory, agent or model. Empty for
    /// a session with no agent or no model, and for the list with no headings.
    pub fn group_key(&self, row: &Row) -> String {
        match self.grouping {
            Grouping::Together => String::new(),
            Grouping::Project => row.task.directory.clone(),
            Grouping::Agent => row.task.agent.clone().unwrap_or_default(),
            Grouping::Model => row.task.model.clone().unwrap_or_default(),
        }
    }

    /// Give each heading not seen before its place in the layout: this directory first where the
    /// headings are projects, then the one with the most recently written session, after the
    /// headings already placed.
    fn rank_new_groups(&mut self) {
        let mut fresh: HashMap<String, u64> = HashMap::new();
        for row in &self.rows {
            let key = self.group_key(row);
            if !self.group_rank.contains_key(&key) {
                let newest = fresh.entry(key).or_insert(0);
                *newest = (*newest).max(row.task.updated);
            }
        }
        let here = |key: &str| self.grouping == Grouping::Project && self.is_here(key);
        let mut fresh: Vec<(String, u64)> = fresh.into_iter().collect();
        fresh.sort_by(|a, b| (here(&b.0), b.1, &b.0).cmp(&(here(&a.0), a.1, &a.0)));
        let placed = self.group_rank.len();
        let ranked: Vec<(String, usize)> = fresh
            .into_iter()
            .enumerate()
            .map(|(at, (key, _))| (key, placed + at))
            .collect();
        self.group_rank.extend(ranked);
    }

    // ---- what is shown

    /// The rows the list shows, as indexes into [`SessionsScreen::rows`]: the pinned ones first, in the
    /// order they were pinned, then the rest in the order the grouping gives (SCREEN-5).
    pub fn visible(&self) -> Vec<usize> {
        let query = self.query();
        // Without a `since:` word the whole of what is typed is the text; with one, the rest of it.
        let needle = if query.has_window() {
            query.phrase().to_string()
        } else {
            self.search.to_lowercase()
        };
        // Where the text typed is the whole of some session's link, a link matches only by being
        // that text, so a pasted address finds the session given it and not every one whose number
        // starts with the same digits (SESSION-33).
        let pasted =
            !needle.is_empty() && self.rows.iter().any(|row| links_of(row).contains(&needle));
        let tab = TABS[self.tab];
        let target = self
            .renaming
            .as_ref()
            .map(|renaming| renaming.task.id.as_str());
        let (mut pinned, mut rest): (Vec<usize>, Vec<usize>) = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                self.matches(row, &query, &needle, pasted)
                    && (Some(row.task.id.as_str()) == target
                        || tab.is_none_or(|group| group == row.group))
            })
            .map(|(index, _)| index)
            .partition(|index| self.is_pinned(*index));
        pinned.sort_by_key(|index| {
            self.pinned
                .iter()
                .position(|id| *id == self.rows[*index].task.id)
        });
        // The rows are held in the order of the attention they ask for, newest last written first
        // within each ([`SessionsScreen::replace`]). Together is that order, and a project is a stable
        // partition of it.
        if self.grouping != Grouping::Together {
            rest.sort_by_key(|index| {
                self.group_rank
                    .get(&self.group_key(&self.rows[*index]))
                    .copied()
                    .unwrap_or(usize::MAX)
            });
        }
        pinned.extend(rest);
        pinned
    }

    /// Whether a row is what was typed: written within a `since:` word, and with the text in its
    /// name, directory, branch, agent or id, in a link it holds, or in what the session said.
    fn matches(&self, row: &Row, query: &Query, needle: &str, pasted: bool) -> bool {
        query.admits_now(row.task.updated)
            && (needle.is_empty()
                || row.haystack.contains(needle)
                || links_of(row).iter().any(|link| match pasted {
                    true => link == needle,
                    false => link.contains(needle),
                })
                || self.said_in(row).is_some())
    }

    pub fn is_pinned(&self, index: usize) -> bool {
        self.pinned.iter().any(|id| *id == self.rows[index].task.id)
    }

    /// Whether two rows are under one heading.
    pub fn same_group(&self, left: usize, right: usize) -> bool {
        if self.is_pinned(left) || self.is_pinned(right) {
            return self.is_pinned(left) && self.is_pinned(right);
        }
        let (left, right) = (&self.rows[left], &self.rows[right]);
        self.group_key(left) == self.group_key(right)
    }

    /// The list as it is drawn: headings, rows, the blank lines between groups, and "Show more".
    pub fn entries(&self) -> Vec<Entry> {
        let mut entries = Vec::new();
        let mut previous: Option<usize> = None;
        let headed = self.grouping != Grouping::Together;
        for index in self.visible() {
            if headed && previous.is_none_or(|previous| !self.same_group(previous, index)) {
                if previous.is_some() {
                    entries.push(Entry::Gap);
                }
                entries.push(Entry::Group(index));
            }
            entries.push(Entry::Task(index));
            if self.said_in(&self.rows[index]).is_some() {
                entries.push(Entry::Said(index));
            }
            previous = Some(index);
        }
        if self.more {
            if headed && previous.is_some() {
                entries.push(Entry::Gap);
            }
            entries.push(Entry::ShowMore);
        }
        entries
    }

    /// How many rows are under the heading the row at `index` starts, and how many there are of that
    /// group with nothing narrowing the list.
    pub fn group_counts(&self, index: usize) -> (usize, usize) {
        let shown = self
            .visible()
            .into_iter()
            .filter(|other| self.same_group(index, *other))
            .count();
        let all = (0..self.rows.len())
            .filter(|other| self.same_group(index, *other))
            .count();
        (shown, all)
    }

    /// The index of the row the cursor is on, if it is on one.
    pub fn selected_index(&self) -> Option<usize> {
        if self.on_show_more {
            return None;
        }
        let id = self.selected.as_deref()?;
        let index = self.rows.iter().position(|row| row.task.id == id)?;
        self.visible().contains(&index).then_some(index)
    }

    pub fn selected_row(&self) -> Option<&Row> {
        self.selected_index().map(|index| &self.rows[index])
    }

    /// Put the cursor on a row that is shown, after what is shown changed under it.
    fn reconcile(&mut self) {
        if self.selected_index().is_some() || (self.on_show_more && self.more) {
            return;
        }
        self.on_show_more = false;
        let first = self.visible().first().copied();
        self.selected = first.map(|index| self.rows[index].task.id.clone());
        if first.is_none() && self.more {
            self.on_show_more = true;
        }
    }

    /// The task the cursor would be on were `removed` gone: the one after it, or else the one
    /// before it (SCREEN-8).
    pub fn successor(&self, removed: &HashSet<String>) -> Option<String> {
        let visible = self.visible();
        let at = visible
            .iter()
            .position(|index| Some(*index) == self.selected_index())?;
        visible[at..]
            .iter()
            .chain(visible[..at].iter().rev())
            .map(|index| self.rows[*index].task.id.clone())
            .find(|id| !removed.contains(id))
    }

    /// Take a task off the list, because it was archived, deleted or hidden, and move the cursor to
    /// the neighbour it had (SCREEN-8).
    pub fn remove(&mut self, id: &str) {
        let removed = HashSet::from([id.to_string()]);
        let successor = self.successor(&removed);
        self.rows.retain(|row| row.task.id != id);
        self.rank.remove(id);
        self.pinned.retain(|pinned| pinned != id);
        if self.selected.as_deref() == Some(id) {
            self.selected = successor;
            self.on_show_more = false;
        }
        self.reconcile();
    }

    /// Record that a task was archived, so the list leaves it out until it is written again.
    pub fn archived_at(&mut self, id: &str, at: u64) {
        self.archived.insert(id.to_string(), at);
        self.remove(id);
    }

    // ---- moving the cursor

    fn step(&mut self, forward: bool) {
        if self.renaming.is_some() {
            return;
        }
        let mut cells: Vec<Option<usize>> = self.visible().into_iter().map(Some).collect();
        if self.more {
            cells.push(None);
        }
        // The search box is the cell before the first row: Down from it is the first row, Up from
        // it is the last, and Up from the first row and Down from the last come to it (SCREEN-4).
        if self.searching || cells.is_empty() {
            match (self.searching, cells.is_empty()) {
                (_, true) => self.searching = true,
                (_, false) => {
                    self.searching = false;
                    self.land_on(cells[if forward { 0 } else { cells.len() - 1 }]);
                }
            }
            return;
        }
        let now = if self.on_show_more {
            cells.len() - 1
        } else {
            cells
                .iter()
                .position(|cell| *cell == self.selected_index())
                .unwrap_or(0)
        };
        if (forward && now + 1 == cells.len()) || (!forward && now == 0) {
            self.searching = true;
            return;
        }
        let to = if forward { now + 1 } else { now - 1 };
        self.land_on(cells[to]);
    }

    fn land_on(&mut self, cell: Option<usize>) {
        match cell {
            Some(index) => {
                self.selected = Some(self.rows[index].task.id.clone());
                self.on_show_more = false;
            }
            None => self.on_show_more = true,
        }
    }

    fn jump(&mut self, top: bool) {
        if self.renaming.is_some() {
            return;
        }
        let visible = self.visible();
        let to = if top { visible.first() } else { visible.last() };
        if let Some(index) = to.copied() {
            self.land_on(Some(index));
        }
    }

    /// Move the cursor a page, and the view with it, stopping at either end (SCREEN-7).
    fn page(&mut self, forward: bool) {
        if self.renaming.is_some() {
            return;
        }
        let entries = self.entries();
        let cells: Vec<(usize, Option<usize>)> = entries
            .iter()
            .enumerate()
            .filter_map(|(at, entry)| match entry {
                Entry::Task(index) => Some((at, Some(*index))),
                Entry::ShowMore => Some((at, None)),
                _ => None,
            })
            .collect();
        if cells.is_empty() {
            return;
        }
        let now = cells
            .iter()
            .find(|(_, cell)| match cell {
                Some(index) => Some(*index) == self.selected_index(),
                None => self.on_show_more,
            })
            .map_or(cells[0].0, |(at, _)| *at);
        let height = self.page_height.get().max(1);
        let target = if forward {
            now.saturating_add(height)
        } else {
            now.saturating_sub(height)
        };
        let chosen = if forward {
            cells
                .partition_point(|(at, _)| *at < target)
                .min(cells.len() - 1)
        } else {
            cells
                .partition_point(|(at, _)| *at <= target)
                .saturating_sub(1)
        };
        let scroll = self.scroll.get();
        self.scroll.set(
            (if forward {
                scroll.saturating_add(height)
            } else {
                scroll.saturating_sub(height)
            })
            .min(entries.len().saturating_sub(height)),
        );
        self.land_on(cells[chosen].1);
    }

    /// Put the cursor on the row at `index`, as a click does.
    pub fn point_at(&mut self, index: usize) {
        if index < self.rows.len() && !self.modal() {
            self.land_on(Some(index));
        }
    }

    /// Take a click: choose the tab, or put the cursor on the row or on "Show more". Whether it is on
    /// what the click before it pointed at, so that a second press counts as one only where it
    /// is on the same session and not on whatever a wheel turn or a refresh has since put at the cell
    /// (MOUSE-15).
    pub fn press(&mut self, click: Click) -> bool {
        let now = match click {
            Click::Tab(tab) => {
                self.choose_tab(tab);
                None
            }
            Click::Row(index) => {
                let id = self.rows.get(index).map(|row| row.task.id.clone());
                self.point_at(index);
                id.map(Pressed::Row)
            }
            Click::ShowMore => {
                self.point_at_show_more();
                Some(Pressed::ShowMore)
            }
        };
        let same = now.is_some() && now == self.pressed;
        self.pressed = now;
        same
    }

    /// Put the cursor on "Show more", as a click does.
    pub fn point_at_show_more(&mut self) {
        if self.more && !self.modal() {
            self.on_show_more = true;
        }
    }

    /// Move the cursor `rows` rows with the wheel, over the headings, stopping at the first row and
    /// the last without wrapping (SCREEN-12).
    pub fn wheel(&mut self, rows: usize, down: bool) {
        self.pressed = None;
        if self.modal() {
            return;
        }
        let mut cells: Vec<Option<usize>> = self.visible().into_iter().map(Some).collect();
        if self.more {
            cells.push(None);
        }
        if cells.is_empty() {
            return;
        }
        let now = if self.on_show_more {
            cells.len() - 1
        } else {
            cells
                .iter()
                .position(|cell| *cell == self.selected_index())
                .unwrap_or(0)
        };
        let to = if down {
            (now + rows).min(cells.len() - 1)
        } else {
            now.saturating_sub(rows)
        };
        self.land_on(cells[to]);
    }

    /// Move to the tab at `tab`, as a click or Tab does.
    pub fn choose_tab(&mut self, tab: usize) {
        if tab < TABS.len() && !self.editing() && !self.modal() {
            self.tab = tab;
            self.scroll.set(0);
            self.reconcile();
        }
    }

    // ---- keys

    /// What a key does. The Sessions screen takes every key: nothing falls through to the box.
    pub fn key(&mut self, key: KeyEvent) -> Outcome {
        if key.kind == KeyEventKind::Release {
            return Outcome::Nothing;
        }
        let had_notice = self.notice.take().is_some();
        self.pressed = None;
        let outcome = self.answer(key);
        match (outcome, had_notice) {
            (Outcome::Nothing, true) => Outcome::Redraw,
            (outcome, _) => outcome,
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "the keys of the list, each answered by the arm that names it"
    )]
    fn answer(&mut self, key: KeyEvent) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT);
        let plain = key.modifiers.difference(KeyModifiers::SHIFT).is_empty();
        let leaving = key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('c'));

        if let Some(confirm) = &self.confirm {
            return match key.code {
                KeyCode::Char('y') if plain => {
                    let effect = match (confirm.what, confirm.then.clone()) {
                        (Destructive::Archive, _) => {
                            Effect::Archive(Box::new(confirm.task.clone()))
                        }
                        (Destructive::Delete, _) => Effect::Delete(Box::new(confirm.task.clone())),
                        (Destructive::Interrupt, Some(then)) => Effect::StopTurnThen(then),
                        // A question about a turn with nothing to carry out afterwards asks for
                        // nothing.
                        (Destructive::Interrupt, None) => {
                            self.confirm = None;
                            return Outcome::Redraw;
                        }
                    };
                    self.confirm = None;
                    Outcome::Effect(effect)
                }
                KeyCode::Char('n') if plain => {
                    self.confirm = None;
                    Outcome::Redraw
                }
                _ if leaving => {
                    self.confirm = None;
                    Outcome::Redraw
                }
                _ => Outcome::Nothing,
            };
        }
        if self.help {
            return match key.code {
                _ if self.keys.is_help(&key) => {
                    self.help = false;
                    Outcome::Redraw
                }
                _ if leaving => {
                    self.help = false;
                    Outcome::Redraw
                }
                _ => Outcome::Nothing,
            };
        }
        if self.renaming.is_some() {
            return self.rename_key(key, plain, ctrl, leaving);
        }
        if self.searching
            && let Some(done) = self.search_key(key, plain, ctrl, leaving)
        {
            return done;
        }

        if leaving {
            return match self.search.is_empty() {
                false => {
                    self.search.clear();
                    self.searching = false;
                    self.reconcile();
                    Outcome::Redraw
                }
                true => Outcome::Close,
            };
        }
        // A key a settings file moved an action onto is read as the key that action answers by
        // default, and the key it was moved off does nothing (SCREEN-14).
        let Some(key) = self.keys.translate(&key) else {
            return Outcome::Nothing;
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT);
        let plain = key.modifiers.difference(KeyModifiers::SHIFT).is_empty();
        // Terminals spell Shift-Tab as BackTab, or as Tab with Shift held.
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            let back = key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
            self.choose_tab(match back {
                true => (self.tab + TABS.len() - 1) % TABS.len(),
                false => (self.tab + 1) % TABS.len(),
            });
            return Outcome::Redraw;
        }
        if ctrl {
            return match key.code {
                KeyCode::Char('p') => self.moved(|sessions_screen| sessions_screen.step(false)),
                KeyCode::Char('n') => self.moved(|sessions_screen| sessions_screen.step(true)),
                KeyCode::Char('b') => self.moved(|sessions_screen| sessions_screen.page(false)),
                KeyCode::Char('f') => self.moved(|sessions_screen| sessions_screen.page(true)),
                _ => Outcome::Nothing,
            };
        }
        if !plain {
            return Outcome::Nothing;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.moved(|sessions_screen| sessions_screen.step(false))
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.moved(|sessions_screen| sessions_screen.step(true))
            }
            KeyCode::PageUp => self.moved(|sessions_screen| sessions_screen.page(false)),
            KeyCode::PageDown => self.moved(|sessions_screen| sessions_screen.page(true)),
            KeyCode::Home => self.moved(|sessions_screen| sessions_screen.jump(true)),
            KeyCode::End => self.moved(|sessions_screen| sessions_screen.jump(false)),
            KeyCode::Enter | KeyCode::Right => self.open(),
            KeyCode::Char('?') => {
                self.help = true;
                Outcome::Redraw
            }
            KeyCode::Char('/') | KeyCode::F(3) => {
                self.searching = true;
                Outcome::Redraw
            }
            // The tabs by their place, so one press reaches any of them.
            KeyCode::Char(digit @ '1'..='5') => {
                self.choose_tab(usize::from(digit as u8 - b'1'));
                Outcome::Redraw
            }
            KeyCode::Char('n') => Outcome::Effect(Effect::New(
                self.selected_row().map(|row| row.task.directory.clone()),
            )),
            KeyCode::Char('g') => {
                self.group_by(self.grouping.next());
                self.scroll.set(0);
                self.reconcile();
                Outcome::Effect(Effect::Grouping(self.grouping))
            }
            KeyCode::Char('f') => match self.selected_row() {
                Some(row) => Outcome::Effect(Effect::Fork(Box::new(row.task.clone()))),
                None => Outcome::Nothing,
            },
            KeyCode::Char('r') => self.begin_rename(),
            KeyCode::Char('x') => match self.selected_row() {
                Some(row) if row.task.live => {
                    Outcome::Effect(Effect::Stop(Box::new(row.task.clone())))
                }
                _ => Outcome::Nothing,
            },
            KeyCode::Char('a') => self.ask(Destructive::Archive),
            KeyCode::Backspace | KeyCode::Delete => self.ask(Destructive::Delete),
            KeyCode::Char('h') => match self.selected_row().map(|row| row.task.id.clone()) {
                Some(id) => {
                    self.hidden.insert(id.clone());
                    self.remove(&id);
                    Outcome::Redraw
                }
                None => Outcome::Nothing,
            },
            KeyCode::Char('p') => self.toggle_pin(),
            _ => Outcome::Nothing,
        }
    }

    /// Run a move of the cursor and say the screen changed.
    fn moved(&mut self, mut go: impl FnMut(&mut Self)) -> Outcome {
        go(self);
        Outcome::Redraw
    }

    /// Enter and Right: the task under the cursor, or more of the list.
    pub fn open(&mut self) -> Outcome {
        if self.modal() {
            return Outcome::Nothing;
        }
        if self.on_show_more {
            return Outcome::Effect(Effect::ShowMore);
        }
        match self.selected_row() {
            Some(row) => Outcome::Effect(Effect::Open(Box::new(row.task.clone()))),
            None => Outcome::Nothing,
        }
    }

    fn begin_rename(&mut self) -> Outcome {
        let Some(row) = self.selected_row() else {
            return Outcome::Nothing;
        };
        self.renaming = Some(Renaming {
            text: row.title.clone(),
            task: row.task.clone(),
        });
        self.searching = false;
        self.search.clear();
        Outcome::Redraw
    }

    fn rename_key(&mut self, key: KeyEvent, plain: bool, ctrl: bool, leaving: bool) -> Outcome {
        let Some(renaming) = self.renaming.as_mut() else {
            return Outcome::Nothing;
        };
        match key.code {
            _ if leaving => {
                self.renaming = None;
                self.reconcile();
            }
            KeyCode::Enter => {
                let name = renaming.text.trim().to_string();
                if name.is_empty() {
                    return Outcome::Nothing;
                }
                let task = renaming.task.clone();
                self.renaming = None;
                self.reconcile();
                return Outcome::Effect(Effect::Rename {
                    task: Box::new(task),
                    name,
                });
            }
            KeyCode::Backspace => {
                renaming.text.pop();
            }
            KeyCode::Char('u') if ctrl => renaming.text.clear(),
            KeyCode::Char(typed) if plain && !typed.is_control() => renaming.text.push(typed),
            _ => return Outcome::Nothing,
        }
        Outcome::Redraw
    }

    /// A key while a search is being typed, or `None` where the key is the list's to answer.
    fn search_key(
        &mut self,
        key: KeyEvent,
        plain: bool,
        ctrl: bool,
        leaving: bool,
    ) -> Option<Outcome> {
        if leaving {
            return None;
        }
        // Left and Right would move a caret that only ever stands at the end, and Right is the key
        // that opens a row, which a person typing has not asked for.
        // Delete would otherwise reach the key that asks to delete the first match.
        if matches!(key.code, KeyCode::Left | KeyCode::Right | KeyCode::Delete) {
            return Some(Outcome::Nothing);
        }
        let edited = match key.code {
            KeyCode::Backspace => {
                self.search.pop();
                true
            }
            KeyCode::Char('u') if ctrl => {
                self.search.clear();
                true
            }
            KeyCode::Char(typed) if plain && !typed.is_control() => {
                self.search.push(typed);
                true
            }
            _ => false,
        };
        if !edited {
            return None;
        }
        // A search covers every record, so the next read takes them all.
        if !self.search.is_empty() {
            self.next_read = Instant::now();
        }
        self.search_words();
        self.scroll.set(0);
        self.on_show_more = false;
        self.selected = self
            .visible()
            .first()
            .map(|index| self.rows[*index].task.id.clone());
        self.reconcile();
        Some(Outcome::Redraw)
    }

    /// Put the question before stopping the turn that is running so that `then`, an opening, a copy
    /// or the beginning of a session, can be carried out (SCREEN-15). `false` where there is no
    /// session to name in it, and nothing is asked.
    pub fn ask_to_stop_the_turn(&mut self, then: Effect) -> bool {
        let named = match &then {
            Effect::Open(task) | Effect::Fork(task) => Some((**task).clone()),
            Effect::New(_) => self
                .selected_row()
                .or_else(|| self.rows.iter().find(|row| row.task.current))
                .map(|row| row.task.clone()),
            _ => None,
        };
        let Some(task) = named else {
            return false;
        };
        self.confirm = Some(Confirm {
            task,
            what: Destructive::Interrupt,
            then: Some(Box::new(then)),
        });
        true
    }

    fn ask(&mut self, what: Destructive) -> Outcome {
        match self.selected_row() {
            Some(row) => {
                self.confirm = Some(Confirm {
                    task: row.task.clone(),
                    what,
                    then: None,
                });
                Outcome::Redraw
            }
            None => Outcome::Nothing,
        }
    }

    fn toggle_pin(&mut self) -> Outcome {
        let Some(id) = self.selected_row().map(|row| row.task.id.clone()) else {
            return Outcome::Nothing;
        };
        match self.pinned.iter().position(|pinned| *pinned == id) {
            Some(at) => {
                self.pinned.remove(at);
            }
            None => self.pinned.push(id),
        }
        self.reconcile();
        Outcome::Effect(Effect::Pins(self.pinned.clone()))
    }
}

/// Tasks and sessions_screens for the tests of this module, of the loop that carries out what a key asks
/// for, and of the screen.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use bravebot_session::listing::Status;

    /// A task of `directory` last written at `updated`, with every field a row draws filled.
    pub(crate) fn task(id: &str, directory: &str, status: Status, updated: u64) -> Task {
        Task {
            id: id.to_string(),
            directory: directory.to_string(),
            title: format!("title {id}"),
            status,
            held: None,
            updated,
            started: updated.saturating_sub(10),
            model: Some("model-a".to_string()),
            branch: None,
            prompt: None,
            turns: 1,
            tokens: 0,
            read: 0,
            written: 0,
            mode: None,
            background: false,
            live: matches!(status, Status::Working | Status::NeedsInput | Status::Ready),
            current: false,
            manifest: false,
            agent: None,
            bytes: 0,
            issue: None,
            pull_request: None,
            handed_off_from: None,
        }
    }

    pub(crate) fn page(tasks: Vec<Task>) -> Page {
        Page { tasks, more: false }
    }

    /// A Sessions screen grouped by status over `tasks`, with no preference saved and no session of its own.
    pub(crate) fn sessions_screen_of(tasks: Vec<Task>) -> SessionsScreen {
        let mut sessions_screen =
            SessionsScreen::new(Grouping::Together, Vec::new(), HashMap::new(), None);
        sessions_screen.replace(page(tasks));
        sessions_screen
    }

    /// The five tasks of two directories that most tests start from, one of each status group but
    /// working, which has two.
    pub(crate) fn mixed() -> Vec<Task> {
        vec![
            task("a", "/work/one", Status::Saved, 50),
            task("b", "/work/one", Status::Working, 40),
            task("c", "/work/two", Status::NeedsInput, 30),
            task("d", "/work/two", Status::Ready, 20),
            task("e", "/work/two", Status::Working, 10),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_session::listing::Status;
    use ratatui::crossterm::event::{KeyEventKind, KeyEventState};

    use super::fixtures::{mixed, page, sessions_screen_of as sessions_screen, task};

    fn press(sessions_screen: &mut SessionsScreen, code: KeyCode) -> Outcome {
        sessions_screen.key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn typed(sessions_screen: &mut SessionsScreen, text: &str) {
        for letter in text.chars() {
            press(sessions_screen, KeyCode::Char(letter));
        }
    }

    fn ids(sessions_screen: &SessionsScreen) -> Vec<String> {
        sessions_screen
            .visible()
            .into_iter()
            .map(|index| sessions_screen.rows()[index].task.id.clone())
            .collect()
    }

    fn selected(sessions_screen: &SessionsScreen) -> Option<&str> {
        sessions_screen
            .selected_row()
            .map(|row| row.task.id.as_str())
    }

    /// SCREEN-4. The tabs count the four sets and each filters to its own.
    #[test]
    fn each_tab_counts_its_set_and_filters_to_it() {
        let mut sessions_screen = sessions_screen(mixed());

        let counts: Vec<usize> = (0..TABS.len())
            .map(|tab| sessions_screen.count(tab))
            .collect();
        assert_eq!(counts, [5, 1, 2, 1, 1]);

        press(&mut sessions_screen, KeyCode::Tab);
        assert_eq!(
            ids(&sessions_screen),
            ["c"],
            "the needs-you tab showed another set"
        );
        press(&mut sessions_screen, KeyCode::Tab);
        assert_eq!(ids(&sessions_screen), ["b", "e"]);
        press(&mut sessions_screen, KeyCode::BackTab);
        assert_eq!(
            ids(&sessions_screen),
            ["c"],
            "Shift-Tab did not go back one tab"
        );
        sessions_screen.choose_tab(0);
        press(&mut sessions_screen, KeyCode::BackTab);
        assert_eq!(
            sessions_screen.tab(),
            TABS.len() - 1,
            "Shift-Tab did not wrap round"
        );
    }

    /// SCREEN-4. Narrowing the list does not change what the tabs count, so the counts are the
    /// machine's and not the search's.
    #[test]
    fn a_search_narrows_the_list_and_not_the_counts() {
        let mut sessions_screen = sessions_screen(mixed());

        press(&mut sessions_screen, KeyCode::Char('/'));
        typed(&mut sessions_screen, "TITLE d");

        assert_eq!(
            ids(&sessions_screen),
            ["d"],
            "the search is not case-blind or did not narrow"
        );
        assert_eq!(sessions_screen.count(0), 5);
        assert_eq!(
            selected(&sessions_screen),
            Some("d"),
            "the cursor is not on the first match"
        );
    }

    /// SCREEN-7. Letters typed in a search are the search's and not the Sessions screen's keys.
    #[test]
    fn a_letter_typed_in_a_search_is_not_a_command() {
        let mut sessions_screen = sessions_screen(mixed());

        press(&mut sessions_screen, KeyCode::Char('/'));
        let outcome = [
            press(&mut sessions_screen, KeyCode::Char('x')),
            press(&mut sessions_screen, KeyCode::Char('a')),
            press(&mut sessions_screen, KeyCode::Char('g')),
        ];

        assert!(
            outcome.iter().all(|outcome| *outcome == Outcome::Redraw),
            "{outcome:?}"
        );
        assert_eq!(sessions_screen.search(), "xag");
        assert_eq!(
            sessions_screen.grouping(),
            Grouping::Together,
            "g regrouped the list"
        );
        assert!(sessions_screen.confirming().is_none(), "a asked to archive");
    }

    /// SCREEN-7. Backspace takes a letter off a search or a rename, and is the key that deletes a
    /// task only when no line is being typed.
    #[test]
    fn backspace_edits_a_line_and_deletes_a_task_only_outside_one() {
        let mut sessions_screen = sessions_screen(mixed());
        press(&mut sessions_screen, KeyCode::Char('/'));
        typed(&mut sessions_screen, "ab");
        assert_eq!(
            press(&mut sessions_screen, KeyCode::Backspace),
            Outcome::Redraw
        );
        assert_eq!(sessions_screen.search(), "a");
        assert!(
            sessions_screen.confirming().is_none(),
            "Backspace in a search asked to delete"
        );
        press(&mut sessions_screen, KeyCode::Esc);

        press(&mut sessions_screen, KeyCode::Char('r'));
        press(&mut sessions_screen, KeyCode::Backspace);
        assert!(
            sessions_screen.confirming().is_none(),
            "Backspace in a rename asked to delete"
        );
        assert_eq!(
            sessions_screen
                .renaming()
                .map(|renaming| renaming.text.chars().count()),
            Some("title c".chars().count() - 1)
        );
        press(&mut sessions_screen, KeyCode::Esc);

        press(&mut sessions_screen, KeyCode::Backspace);
        assert_eq!(
            sessions_screen.confirming().map(|confirm| confirm.what),
            Some(Destructive::Delete)
        );
    }

    /// SCREEN-7. Escape leaves a search before it leaves the Sessions screen, and clears what was typed.
    #[test]
    fn escape_ends_the_search_and_then_the_sessions_screen() {
        let mut sessions_screen = sessions_screen(mixed());
        press(&mut sessions_screen, KeyCode::Char('/'));
        typed(&mut sessions_screen, "zzz");
        assert!(ids(&sessions_screen).is_empty());

        assert_eq!(press(&mut sessions_screen, KeyCode::Esc), Outcome::Redraw);
        assert!(!sessions_screen.searching() && sessions_screen.search().is_empty());
        assert_eq!(
            ids(&sessions_screen).len(),
            5,
            "the cleared search left rows out"
        );
        assert_eq!(press(&mut sessions_screen, KeyCode::Esc), Outcome::Close);
    }

    /// SCREEN-6. A refresh keeps the rows that were there where they were, and a row whose status
    /// changed is put at the end of its new group.
    #[test]
    fn a_refresh_keeps_rows_in_place_while_they_stay_in_their_group() {
        let mut sessions_screen = sessions_screen(vec![
            task("a", "/w", Status::Working, 10),
            task("b", "/w", Status::Working, 20),
            task("c", "/w", Status::Working, 30),
        ]);
        let before = ids(&sessions_screen);

        // The machine gives the newest first, which is not the order the Sessions screen holds them in.
        sessions_screen.replace(page(vec![
            task("c", "/w", Status::Working, 99),
            task("b", "/w", Status::Working, 98),
            task("a", "/w", Status::Working, 97),
            task("new", "/w", Status::Working, 100),
        ]));

        assert_eq!(before, ["a", "b", "c"]);
        assert_eq!(
            ids(&sessions_screen),
            ["a", "b", "c", "new"],
            "a refresh reordered rows that stayed in their group, or put an arrival ahead of them"
        );

        sessions_screen.replace(page(vec![
            task("a", "/w", Status::Ready, 101),
            task("b", "/w", Status::Working, 98),
            task("c", "/w", Status::Working, 99),
            task("new", "/w", Status::Working, 100),
        ]));
        assert_eq!(
            ids(&sessions_screen),
            ["b", "c", "new", "a"],
            "a row that became ready is not after the working ones"
        );
    }

    /// SCREEN-6. A refresh does not move the cursor off its task, even when the task moves.
    #[test]
    fn the_cursor_stays_on_its_task_when_a_refresh_moves_it() {
        let mut sessions_screen = sessions_screen(vec![
            task("a", "/w", Status::Working, 10),
            task("b", "/w", Status::Working, 20),
        ]);
        press(&mut sessions_screen, KeyCode::Down);
        assert_eq!(selected(&sessions_screen), Some("b"));

        sessions_screen.replace(page(vec![
            task("a", "/w", Status::NeedsInput, 30),
            task("b", "/w", Status::Working, 20),
        ]));

        assert_eq!(
            selected(&sessions_screen),
            Some("b"),
            "the cursor followed the row that moved"
        );
    }

    /// SCREEN-5. Together is one list with no headings, in the order of the attention each session
    /// asks for and the newest first within it. A project is a partition of that same order, this
    /// directory's first and then the project with the newest session.
    #[test]
    fn together_is_the_attention_order_and_a_project_is_a_partition_of_it() {
        let mut sessions_screen =
            SessionsScreen::new(Grouping::Together, Vec::new(), HashMap::new(), None)
                .in_directory(PathBuf::from("/work/home"));
        sessions_screen.replace(page(vec![
            task("saved-far", "/work/far", Status::Saved, 90),
            task("saved-home", "/work/home", Status::Saved, 10),
            task("asking-far", "/work/far", Status::NeedsInput, 20),
            task("working-home", "/work/home", Status::Working, 30),
            task("saved-mid", "/work/mid", Status::Saved, 50),
        ]));

        assert_eq!(
            ids(&sessions_screen),
            [
                "asking-far",
                "working-home",
                "saved-far",
                "saved-home",
                "saved-mid"
            ],
            "together is not the attention order"
        );
        assert!(
            sessions_screen
                .entries()
                .iter()
                .all(|entry| matches!(entry, Entry::Task(_))),
            "the list that is not grouped has a heading or a gap in it"
        );

        sessions_screen.group_by(Grouping::Project);
        assert_eq!(
            ids(&sessions_screen),
            [
                "working-home",
                "saved-home",
                "asking-far",
                "saved-far",
                "saved-mid"
            ],
            "this directory first, then the project with the newest session, each in attention order"
        );
        let headings = sessions_screen
            .entries()
            .iter()
            .filter(|entry| matches!(entry, Entry::Group(_)))
            .count();
        assert_eq!(headings, 3);
    }

    /// SCREEN-6. A project keeps its place when one of its sessions is written, so a group does not
    /// move under the cursor.
    #[test]
    fn a_project_keeps_its_place_when_one_of_its_sessions_is_written() {
        let mut sessions_screen =
            SessionsScreen::new(Grouping::Project, Vec::new(), HashMap::new(), None);
        sessions_screen.replace(page(vec![
            task("a", "/work/new", Status::Saved, 50),
            task("b", "/work/old", Status::Saved, 10),
        ]));
        assert_eq!(ids(&sessions_screen), ["a", "b"]);

        sessions_screen.replace(page(vec![
            task("a", "/work/new", Status::Saved, 50),
            task("b", "/work/old", Status::Saved, 99),
        ]));

        assert_eq!(
            ids(&sessions_screen),
            ["a", "b"],
            "a write moved a project above another"
        );
    }

    /// SCREEN-5. The grouping key goes between the two layouts and asks for each choice to be saved.
    #[test]
    fn the_grouping_key_goes_between_the_layouts_and_asks_to_save_each() {
        let mut sessions_screen = sessions_screen(mixed());
        let mut asked = Vec::new();
        for _ in 0..5 {
            if let Outcome::Effect(Effect::Grouping(grouping)) =
                press(&mut sessions_screen, KeyCode::Char('g'))
            {
                asked.push(grouping);
            }
        }
        assert_eq!(
            asked,
            [
                Grouping::Project,
                Grouping::Agent,
                Grouping::Model,
                Grouping::Together,
                Grouping::Project
            ]
        );
    }

    /// SCREEN-6. The cursor starts on the first session waiting on the person where there is one, and
    /// on the session the terminal is in where there is not.
    #[test]
    fn the_cursor_starts_on_what_is_waiting_on_the_person() {
        let waiting = vec![
            task("quiet", "/w", Status::Saved, 90),
            task("here", "/w", Status::Ready, 80),
            task("asking", "/w", Status::NeedsInput, 5),
        ];
        let mut with_one = SessionsScreen::new(
            Grouping::Together,
            Vec::new(),
            HashMap::new(),
            Some("here".to_string()),
        );
        with_one.replace(page(waiting.clone()));
        assert_eq!(selected(&with_one), Some("asking"));

        // Only the first read chooses: after it the cursor stays on the session it is on.
        press(&mut with_one, KeyCode::Down);
        let moved = selected(&with_one).map(str::to_string);
        with_one.replace(page(waiting.clone()));
        assert_eq!(selected(&with_one).map(str::to_string), moved);

        let calm: Vec<Task> = waiting
            .into_iter()
            .filter(|task| task.id != "asking")
            .collect();
        let mut with_none = SessionsScreen::new(
            Grouping::Together,
            Vec::new(),
            HashMap::new(),
            Some("here".to_string()),
        );
        with_none.replace(page(calm));
        assert_eq!(selected(&with_none), Some("here"));
    }

    /// SCREEN-4. A digit chooses the tab in that place, and is a letter of the line while one is typed.
    #[test]
    fn a_digit_chooses_the_tab_in_that_place_and_is_text_in_a_line() {
        let mut sessions_screen = sessions_screen(mixed());
        for (digit, tab) in [('2', 1), ('3', 2), ('4', 3), ('5', 4), ('1', 0)] {
            assert_eq!(
                press(&mut sessions_screen, KeyCode::Char(digit)),
                Outcome::Redraw
            );
            assert_eq!(sessions_screen.tab(), tab, "{digit}");
        }
        press(&mut sessions_screen, KeyCode::Char('/'));
        typed(&mut sessions_screen, "3");
        assert_eq!(
            sessions_screen.tab(),
            0,
            "a digit typed in a search chose a tab"
        );
        assert_eq!(sessions_screen.search(), "3");
        press(&mut sessions_screen, KeyCode::Esc);
        press(&mut sessions_screen, KeyCode::Char('6'));
        assert_eq!(
            sessions_screen.tab(),
            0,
            "a digit that names no tab chose one"
        );
    }

    /// SCREEN-5. Pinned rows come first in the order they were pinned, and are a group of their own
    /// whatever the grouping is.
    #[test]
    fn pinned_rows_come_first_in_the_order_they_were_pinned() {
        let mut sessions_screen = sessions_screen(mixed());
        for target in ["e", "b"] {
            while selected(&sessions_screen) != Some(target) {
                press(&mut sessions_screen, KeyCode::Down);
            }
            press(&mut sessions_screen, KeyCode::Char('p'));
        }

        assert_eq!(sessions_screen.pinned(), ["e", "b"]);
        assert_eq!(&ids(&sessions_screen)[..2], ["e", "b"]);
        let order = sessions_screen.visible();
        assert!(sessions_screen.same_group(order[0], order[1]));
        assert!(
            !sessions_screen.same_group(order[1], order[2]),
            "an unpinned row is under the pinned heading"
        );
    }

    /// SCREEN-5. Pinning asks for the pins to be saved, and pinning again takes the pin off.
    #[test]
    fn pinning_twice_unpins_and_each_press_asks_to_save() {
        let mut sessions_screen = sessions_screen(mixed());
        let under_the_cursor = selected(&sessions_screen)
            .map(str::to_string)
            .expect("a row");
        let first = press(&mut sessions_screen, KeyCode::Char('p'));
        let second = press(&mut sessions_screen, KeyCode::Char('p'));

        assert_eq!(first, Outcome::Effect(Effect::Pins(vec![under_the_cursor])));
        assert_eq!(second, Outcome::Effect(Effect::Pins(Vec::new())));
    }

    /// SCREEN-7. The search box is the cell before the first row: Up from the first row comes to it,
    /// Up from it is the last row, Down from the last row comes to it and Down from it is the first
    /// row. Home and End go to the ends of the rows, and neither moves while a name is being typed.
    #[test]
    fn up_and_down_pass_through_the_search_box_and_nothing_moves_while_a_name_is_typed() {
        let mut sessions_screen = sessions_screen(mixed());
        let order = ids(&sessions_screen);
        let first = order.first().map(String::as_str);
        let last = order.last().map(String::as_str);
        assert_eq!(selected(&sessions_screen), first);

        press(&mut sessions_screen, KeyCode::Up);
        assert!(
            sessions_screen.searching(),
            "Up from the first row missed the box"
        );
        assert_eq!(selected(&sessions_screen), first);
        press(&mut sessions_screen, KeyCode::Up);
        assert!(!sessions_screen.searching());
        assert_eq!(
            selected(&sessions_screen),
            last,
            "Up from the box is not the last row"
        );
        press(&mut sessions_screen, KeyCode::Down);
        assert!(
            sessions_screen.searching(),
            "Down from the last row missed the box"
        );
        press(&mut sessions_screen, KeyCode::Down);
        assert!(!sessions_screen.searching());
        assert_eq!(
            selected(&sessions_screen),
            first,
            "Down from the box is not the first row"
        );
        press(&mut sessions_screen, KeyCode::Down);
        assert_eq!(selected(&sessions_screen), order.get(1).map(String::as_str));
        press(&mut sessions_screen, KeyCode::Home);
        press(&mut sessions_screen, KeyCode::End);
        assert_eq!(selected(&sessions_screen), order.last().map(String::as_str));
        press(&mut sessions_screen, KeyCode::Home);

        press(&mut sessions_screen, KeyCode::Char('r'));
        press(&mut sessions_screen, KeyCode::Down);
        press(&mut sessions_screen, KeyCode::End);
        assert_eq!(
            selected(&sessions_screen),
            order.first().map(String::as_str),
            "the cursor moved under a rename"
        );
    }

    /// SCREEN-7. A page moves the cursor by the height of the list and stops at either end without
    /// wrapping.
    #[test]
    fn a_page_moves_by_the_height_and_stops_at_the_ends() {
        let tasks = (0..30)
            .map(|at| task(&format!("t{at:02}"), "/w", Status::Saved, 100 - at))
            .collect();
        let mut sessions_screen = sessions_screen(tasks);
        sessions_screen.group_by(Grouping::Together);
        sessions_screen.page_height.set(10);

        press(&mut sessions_screen, KeyCode::PageDown);
        let one = ids(&sessions_screen)
            .iter()
            .position(|id| Some(id.as_str()) == selected(&sessions_screen));
        assert!(one.is_some_and(|at| (8..=11).contains(&at)), "{one:?}");
        for _ in 0..10 {
            press(&mut sessions_screen, KeyCode::PageDown);
        }
        assert_eq!(
            selected(&sessions_screen),
            Some("t29"),
            "a page past the end wrapped or stopped short"
        );
        for _ in 0..10 {
            press(&mut sessions_screen, KeyCode::PageUp);
        }
        assert_eq!(
            selected(&sessions_screen),
            Some("t00"),
            "a page past the start wrapped or stopped short"
        );
    }

    /// SCREEN-8. Taking a task off moves the cursor to the one after it, or the one before it when
    /// it was the last.
    #[test]
    fn removing_the_task_under_the_cursor_moves_to_its_neighbour() {
        let mut sessions_screen = sessions_screen(mixed());
        let order = ids(&sessions_screen);
        sessions_screen.remove(&order[1]);
        assert_eq!(
            selected(&sessions_screen),
            Some(order[0].as_str()),
            "the cursor moved when another task went"
        );

        let mut sessions_screen = self::sessions_screen(mixed());
        let order = ids(&sessions_screen);
        press(&mut sessions_screen, KeyCode::Down);
        sessions_screen.remove(&order[1]);
        assert_eq!(
            selected(&sessions_screen),
            Some(order[2].as_str()),
            "the cursor did not take the next task"
        );

        press(&mut sessions_screen, KeyCode::End);
        let last = order[order.len() - 1].clone();
        sessions_screen.remove(&last);
        assert_eq!(
            selected(&sessions_screen),
            order.get(order.len() - 2).map(String::as_str),
            "the last task has no next"
        );
    }

    /// SCREEN-8. Hide takes the task off this list and a refresh does not bring it back.
    #[test]
    fn a_hidden_task_stays_off_the_list_through_a_refresh() {
        let mut sessions_screen = sessions_screen(mixed());
        let hidden = selected(&sessions_screen)
            .map(str::to_string)
            .expect("a row");

        press(&mut sessions_screen, KeyCode::Char('h'));
        sessions_screen.replace(page(mixed()));

        assert!(
            !ids(&sessions_screen).contains(&hidden),
            "a refresh brought back a hidden task"
        );
        assert_eq!(sessions_screen.rows().len(), 4);
    }

    /// SCREEN-9. Archive and delete ask first, only `y` carries them out, and Escape, n and Ctrl-C
    /// withdraw the question.
    #[test]
    fn archive_and_delete_ask_and_only_y_answers() {
        for (key, what) in [
            (KeyCode::Char('a'), Destructive::Archive),
            (KeyCode::Backspace, Destructive::Delete),
        ] {
            let mut sessions_screen = sessions_screen(mixed());
            let task = sessions_screen.selected_row().expect("a row").task.clone();

            assert_eq!(press(&mut sessions_screen, key), Outcome::Redraw);
            assert_eq!(
                sessions_screen.confirming().map(|confirm| confirm.what),
                Some(what)
            );

            for withdraw in [KeyCode::Esc, KeyCode::Char('n')] {
                assert_eq!(press(&mut sessions_screen, withdraw), Outcome::Redraw);
                assert!(sessions_screen.confirming().is_none());
                press(&mut sessions_screen, key);
            }
            assert_eq!(
                press(&mut sessions_screen, KeyCode::Enter),
                Outcome::Nothing,
                "Enter answered the question"
            );
            assert_eq!(
                press(&mut sessions_screen, KeyCode::Char('x')),
                Outcome::Nothing,
                "another letter answered it"
            );
            let answered = press(&mut sessions_screen, KeyCode::Char('y'));
            let expected = match what {
                Destructive::Archive => Effect::Archive(Box::new(task)),
                Destructive::Delete => Effect::Delete(Box::new(task)),
                Destructive::Interrupt => unreachable!("asked for by `d` and `a` only"),
            };
            assert_eq!(answered, Outcome::Effect(expected));
            assert!(sessions_screen.confirming().is_none());
        }
    }

    /// SCREEN-9. A rename starts from the task's name, edits it, and carries out only a name that is
    /// not empty.
    #[test]
    fn a_rename_starts_from_the_name_and_an_empty_name_is_not_carried_out() {
        let mut sessions_screen = sessions_screen(mixed());
        let task = sessions_screen.selected_row().expect("a row").task.clone();

        let name = sessions_screen.selected_row().expect("a row").title.clone();
        press(&mut sessions_screen, KeyCode::Char('r'));
        assert_eq!(
            sessions_screen
                .renaming()
                .map(|renaming| renaming.text.as_str()),
            Some(name.as_str())
        );
        for _ in 0..name.len() {
            press(&mut sessions_screen, KeyCode::Backspace);
        }
        assert_eq!(
            press(&mut sessions_screen, KeyCode::Enter),
            Outcome::Nothing,
            "an empty name was carried out"
        );
        assert!(
            sessions_screen.renaming().is_some(),
            "an empty name ended the rename"
        );

        typed(&mut sessions_screen, " new  name ");
        let done = press(&mut sessions_screen, KeyCode::Enter);

        assert_eq!(
            done,
            Outcome::Effect(Effect::Rename {
                task: Box::new(task),
                name: "new  name".to_string()
            })
        );
        assert!(sessions_screen.renaming().is_none());
    }

    /// SCREEN-9. A rename keeps its task on the list when the tab it is on would leave it out.
    #[test]
    fn the_task_being_renamed_stays_listed_when_the_tab_would_leave_it_out() {
        let mut sessions_screen = sessions_screen(mixed());
        sessions_screen.choose_tab(2);
        let id = selected(&sessions_screen)
            .map(str::to_string)
            .expect("a working task");
        press(&mut sessions_screen, KeyCode::Char('r'));

        sessions_screen.replace(page(
            mixed()
                .into_iter()
                .map(|task| match task.id == id {
                    true => Task {
                        status: Status::Ready,
                        ..task
                    },
                    false => task,
                })
                .collect(),
        ));

        assert!(
            ids(&sessions_screen).contains(&id),
            "the task left the list under the line being typed"
        );
    }

    /// SCREEN-9. The keys that act on a task do nothing on no task, and stop does nothing on a task
    /// no process is running.
    #[test]
    fn stop_needs_a_running_process_and_the_rest_need_a_task() {
        let mut sessions_screen = sessions_screen(vec![task("s", "/w", Status::Saved, 1)]);
        assert_eq!(
            press(&mut sessions_screen, KeyCode::Char('x')),
            Outcome::Nothing
        );

        let mut running = self::sessions_screen(vec![task("w", "/w", Status::Working, 1)]);
        assert!(matches!(
            press(&mut running, KeyCode::Char('x')),
            Outcome::Effect(Effect::Stop(_))
        ));

        let mut empty = self::sessions_screen(Vec::new());
        for key in ['f', 'r', 'x', 'a', 'h', 'p'] {
            assert_eq!(
                press(&mut empty, KeyCode::Char(key)),
                Outcome::Nothing,
                "{key}"
            );
        }
        assert_eq!(press(&mut empty, KeyCode::Backspace), Outcome::Nothing);
        assert_eq!(press(&mut empty, KeyCode::Enter), Outcome::Nothing);
    }

    /// SCREEN-7. Enter and Right open the task under the cursor, and "Show more" asks for more.
    #[test]
    fn enter_and_right_open_the_task_and_show_more_asks_for_more() {
        let mut sessions_screen = sessions_screen(mixed());
        let task = sessions_screen.selected_row().expect("a row").task.clone();
        assert_eq!(
            press(&mut sessions_screen, KeyCode::Enter),
            Outcome::Effect(Effect::Open(Box::new(task.clone())))
        );
        assert_eq!(
            press(&mut sessions_screen, KeyCode::Right),
            Outcome::Effect(Effect::Open(Box::new(task)))
        );

        sessions_screen.replace(Page {
            tasks: mixed(),
            more: true,
        });
        press(&mut sessions_screen, KeyCode::End);
        press(&mut sessions_screen, KeyCode::Down);
        assert!(
            sessions_screen.on_show_more(),
            "the entry after the last task is Show more"
        );
        assert_eq!(
            press(&mut sessions_screen, KeyCode::Enter),
            Outcome::Effect(Effect::ShowMore)
        );
        assert_eq!(
            sessions_screen.entries().last(),
            Some(&Entry::ShowMore),
            "Show more is not the last entry"
        );
    }

    /// SCREEN-7. New is begun in the directory of the task under the cursor.
    #[test]
    fn new_is_begun_where_the_task_under_the_cursor_ran() {
        let mut sessions_screen = sessions_screen(mixed());
        let directory = sessions_screen
            .selected_row()
            .expect("a row")
            .task
            .directory
            .clone();
        assert_eq!(
            press(&mut sessions_screen, KeyCode::Char('n')),
            Outcome::Effect(Effect::New(Some(directory)))
        );
        press(&mut sessions_screen, KeyCode::Down);
        press(&mut sessions_screen, KeyCode::Down);
        press(&mut sessions_screen, KeyCode::Down);
        press(&mut sessions_screen, KeyCode::Down);
        let (id, directory) = sessions_screen
            .selected_row()
            .map(|row| (row.task.id.clone(), row.task.directory.clone()))
            .expect("a row");
        assert_eq!(
            press(&mut sessions_screen, KeyCode::Char('n')),
            Outcome::Effect(Effect::New(Some(directory.clone()))),
            "{id} is not new in its own directory"
        );
        assert!(
            ["/work/one", "/work/two"].contains(&directory.as_str()),
            "the directory is not one a task ran in"
        );
        let mut empty = self::sessions_screen(Vec::new());
        assert_eq!(
            press(&mut empty, KeyCode::Char('n')),
            Outcome::Effect(Effect::New(None))
        );
    }

    /// SCREEN-7. The help list is opened by `?`, takes every key but the ones that close it, and
    /// Escape closes it before it closes the Sessions screen.
    #[test]
    fn help_takes_every_key_until_it_is_closed() {
        let mut sessions_screen = sessions_screen(mixed());
        press(&mut sessions_screen, KeyCode::Char('?'));
        assert!(sessions_screen.helping());

        assert_eq!(
            press(&mut sessions_screen, KeyCode::Char('a')),
            Outcome::Nothing
        );
        assert_eq!(
            press(&mut sessions_screen, KeyCode::Enter),
            Outcome::Nothing
        );
        assert!(sessions_screen.confirming().is_none());
        assert_eq!(press(&mut sessions_screen, KeyCode::Esc), Outcome::Redraw);
        assert!(!sessions_screen.helping());
        press(&mut sessions_screen, KeyCode::Char('?'));
        press(&mut sessions_screen, KeyCode::Char('?'));
        assert!(!sessions_screen.helping(), "? did not close the help");
        assert_eq!(press(&mut sessions_screen, KeyCode::Esc), Outcome::Close);
    }

    /// SCREEN-7. A key held with Ctrl or Alt is not the letter it carries.
    #[test]
    fn a_chord_is_not_the_letter_it_carries() {
        let mut sessions_screen = sessions_screen(mixed());
        for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
            for letter in ['a', 'n', 'x', 'g', 'r', 'h', 'p', 'f', 'o', 'q'] {
                let outcome = sessions_screen.key(KeyEvent {
                    code: KeyCode::Char(letter),
                    modifiers,
                    kind: KeyEventKind::Press,
                    state: KeyEventState::NONE,
                });
                assert!(
                    matches!(outcome, Outcome::Nothing | Outcome::Redraw),
                    "{modifiers:?}-{letter} did {outcome:?}"
                );
            }
        }
        assert!(sessions_screen.confirming().is_none() && sessions_screen.renaming().is_none());
        assert_eq!(sessions_screen.grouping(), Grouping::Together);
    }

    /// SCREEN-2. A control character in a name, a directory or a model is pictured, so it reaches
    /// the screen as a picture and not as the character.
    #[test]
    fn a_control_character_in_what_a_row_holds_is_pictured() {
        let mut hostile = task("h", "/w/\u{1b}[2J", Status::Saved, 1);
        hostile.title = "name \u{1b}[31m red".to_string();
        hostile.model = Some("m\u{7}".to_string());
        hostile.branch = Some("b\u{8}".to_string());
        hostile.prompt = Some("p\u{1b}]0;x\u{7}".to_string());

        let row = Row::of(hostile);

        for shown in [
            row.title.as_str(),
            row.directory.as_str(),
            row.model.as_deref().unwrap_or(""),
            row.branch.as_deref().unwrap_or(""),
            row.prompt.as_deref().unwrap_or(""),
        ] {
            assert!(!shown.chars().any(char::is_control), "{shown:?}");
        }
        assert!(
            row.title.contains('\u{241b}'),
            "the escape was dropped and not pictured"
        );
    }

    /// SCREEN-7. A task's name is its first line, so a name with a newline in it is one row.
    #[test]
    fn a_name_of_several_lines_is_one_row_of_its_first() {
        let mut many = task("m", "/w", Status::Saved, 1);
        many.title = "first\nsecond".to_string();
        assert_eq!(Row::of(many).title, "first");
    }

    /// SCREEN-6. The cursor starts on the session the terminal is in when nothing waits on the person.
    #[test]
    fn the_cursor_starts_on_the_session_the_terminal_is_in() {
        let mut sessions_screen = SessionsScreen::new(
            Grouping::Together,
            Vec::new(),
            HashMap::new(),
            Some("d".to_string()),
        );
        sessions_screen.replace(page(vec![
            task("a", "/work/one", Status::Saved, 50),
            task("d", "/work/two", Status::Ready, 20),
            task("e", "/work/two", Status::Working, 10),
        ]));
        assert_eq!(selected(&sessions_screen), Some("d"));
    }

    /// SCREEN-10. A session archived is gone from the list and the cursor goes to its neighbour.
    #[test]
    fn an_archived_task_leaves_the_list() {
        let mut sessions_screen = sessions_screen(mixed());
        let id = selected(&sessions_screen)
            .map(str::to_string)
            .expect("a row");
        sessions_screen.archived_at(&id, 99);
        assert!(!ids(&sessions_screen).contains(&id));
        assert_eq!(sessions_screen.archived().get(&id), Some(&99));
        assert!(sessions_screen.selected_row().is_some());
    }

    /// A task last written at `updated` that ran on `branch` and was given the links.
    fn linked(id: &str, branch: &str, issue: Option<&str>, pull_request: Option<&str>) -> Task {
        let mut linked = task(id, "/work/one", Status::Saved, 100);
        linked.branch = Some(branch.to_string());
        linked.issue = issue.map(str::to_string);
        linked.pull_request = pull_request.map(str::to_string);
        linked
    }

    fn linked_screen() -> SessionsScreen {
        sessions_screen(vec![
            linked(
                "p",
                "fix-login",
                Some("https://example.com/org/repo/issues/12"),
                Some("https://example.com/org/repo/pull/1270"),
            ),
            linked(
                "q",
                "add-search",
                None,
                Some("https://example.com/org/repo/pull/127"),
            ),
            linked("r", "plain", None, None),
        ])
    }

    fn found_by(screen: &mut SessionsScreen, text: &str) -> Vec<String> {
        press(screen, KeyCode::Char('/'));
        typed(screen, text);
        ids(screen)
    }

    /// SCREEN-4. The branch a session ran on is searched, anywhere in it and without regard to case.
    #[test]
    fn typing_part_of_a_branch_finds_the_session_that_ran_on_it() {
        let mut screen = linked_screen();
        assert_eq!(found_by(&mut screen, "LOGIN"), ["p"]);
    }

    /// SCREEN-4. The issue a person linked is searched, anywhere in the link.
    #[test]
    fn typing_part_of_an_issue_link_finds_the_session_given_it() {
        let mut screen = linked_screen();
        assert_eq!(found_by(&mut screen, "issues/12"), ["p"]);
    }

    /// SCREEN-4. A pasted pull request address finds the session that holds it, and not every one
    /// whose number starts with the same digits.
    #[test]
    fn a_pasted_pull_request_link_leaves_only_the_session_that_holds_it() {
        let mut screen = linked_screen();
        assert_eq!(
            found_by(&mut screen, "https://example.com/org/repo/pull/127"),
            ["q"]
        );
    }

    /// SCREEN-4. A pasted address still keeps a session whose name holds it.
    #[test]
    fn a_pasted_link_keeps_a_session_whose_title_holds_it() {
        let mut named = linked("s", "main", None, None);
        named.title = "see https://example.com/org/repo/pull/127".to_string();
        let mut screen = sessions_screen(vec![
            linked(
                "q",
                "add-search",
                None,
                Some("https://example.com/org/repo/pull/127"),
            ),
            named,
        ]);
        let mut found = found_by(&mut screen, "https://example.com/org/repo/pull/127");
        found.sort();
        assert_eq!(found, ["q", "s"]);
    }

    /// SCREEN-4. Part of an address keeps every session it is part of.
    #[test]
    fn part_of_a_pull_request_link_keeps_every_session_it_is_part_of() {
        let mut screen = linked_screen();
        let mut found = found_by(&mut screen, "pull/127");
        found.sort();
        assert_eq!(found, ["p", "q"]);
    }

    /// SCREEN-4. `--from-pr` keeps the sessions linked to the pull request, by number or address, and
    /// not those whose number merely starts with the same digits; an issue link does not count.
    #[test]
    fn a_pull_request_narrows_the_list_by_number_or_address_and_not_by_a_prefix() {
        for wanted in ["127", "#127", "https://example.com/org/repo/pull/127/"] {
            let mut screen =
                SessionsScreen::new(Grouping::Together, Vec::new(), HashMap::new(), None)
                    .from_pull_request(wanted);
            screen.replace(page(vec![
                linked(
                    "p",
                    "a",
                    None,
                    Some("https://example.com/org/repo/pull/1270"),
                ),
                linked(
                    "q",
                    "b",
                    None,
                    Some("https://example.com/org/repo/pull/127"),
                ),
                linked(
                    "r",
                    "c",
                    Some("https://example.com/org/repo/issues/127"),
                    None,
                ),
            ]));
            assert_eq!(ids(&screen), ["q"], "for {wanted}");
            assert_eq!(
                screen.count(0),
                1,
                "the tabs count more than the pull request"
            );
        }
    }

    /// SCREEN-4. Typing narrows the list a pull request opened further, and a pull request nothing
    /// is linked to leaves the list empty rather than showing every session.
    #[test]
    fn typing_narrows_a_pull_requests_list_and_an_unlinked_one_lists_nothing() {
        let mut screen = SessionsScreen::new(Grouping::Together, Vec::new(), HashMap::new(), None)
            .from_pull_request("127");
        screen.replace(page(vec![
            linked(
                "q",
                "add-search",
                None,
                Some("https://example.com/org/repo/pull/127"),
            ),
            linked(
                "p",
                "other",
                None,
                Some("https://example.com/org/repo/pull/127"),
            ),
        ]));
        assert_eq!(found_by(&mut screen, "search"), ["q"]);
        let mut none = SessionsScreen::new(Grouping::Together, Vec::new(), HashMap::new(), None)
            .from_pull_request("999");
        none.replace(page(vec![linked("q", "a", None, None)]));
        assert!(ids(&none).is_empty());
        assert_eq!(none.pull_request(), Some("999"));
    }

    fn remembering() -> SessionsScreen {
        let now = bravebot_session::sessions::now();
        let mut recent = task("a", "/work/one", Status::Saved, now - 60);
        recent.title = "the first thing asked".to_string();
        let mut old = task("b", "/work/one", Status::Saved, now - 40 * 86_400);
        old.title = "something else".to_string();
        let mut screen = sessions_screen(vec![recent, old]).with_corpus(Corpus::of([
            (
                "a".to_string(),
                vec!["the migration failed on the staging table".to_string()],
            ),
            (
                "b".to_string(),
                vec!["the migration failed on the old table".to_string()],
            ),
        ]));
        screen.read = true;
        screen
    }

    /// SCREEN-4. A session is found by something said in it, and the row shows the line that holds it.
    #[test]
    fn typing_words_said_in_a_session_finds_it_and_shows_the_line_that_did() {
        let mut screen = remembering();
        assert_eq!(found_by(&mut screen, "staging"), ["a"]);
        let shown: Vec<String> = screen
            .entries()
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Said(index) => screen.said_in(&screen.rows()[index]).map(str::to_string),
                _ => None,
            })
            .collect();
        assert_eq!(shown.len(), 1);
        assert!(shown[0].contains("staging"), "{shown:?}");
    }

    /// SCREEN-4. A session whose words and name do not hold the text is left out, and clearing the
    /// text brings every session back.
    #[test]
    fn a_session_whose_words_do_not_hold_the_phrase_is_left_out_until_it_is_cleared() {
        let mut screen = remembering();
        assert!(found_by(&mut screen, "nothing of the sort").is_empty());
        for _ in 0.."nothing of the sort".len() {
            press(&mut screen, KeyCode::Backspace);
        }
        assert_eq!(ids(&screen).len(), 2);
    }

    /// SCREEN-4. A session whose name holds the text shows no line of what it said.
    #[test]
    fn a_name_that_holds_the_text_shows_no_line_of_what_was_said() {
        let mut screen = remembering();
        assert_eq!(found_by(&mut screen, "first thing"), ["a"]);
        assert!(
            !screen
                .entries()
                .iter()
                .any(|entry| matches!(entry, Entry::Said(_)))
        );
    }

    /// SCREEN-4. A `since:` word keeps the sessions written within it, narrows the words found
    /// without being part of them, and is typed text until it is finished.
    #[test]
    fn a_since_word_keeps_the_sessions_written_within_it_and_is_text_until_finished() {
        let mut screen = remembering();
        assert_eq!(found_by(&mut screen, "since:7d"), ["a"]);
        let mut screen = remembering();
        assert_eq!(found_by(&mut screen, "since:7d migration"), ["a"]);
        let mut screen = remembering();
        assert_eq!(found_by(&mut screen, "migration since:90d").len(), 2);
        let mut screen = remembering();
        assert!(found_by(&mut screen, "since:7").is_empty());
    }

    /// A worker whose reads of the words wait for a signal, and whose answers hold `a` for every id.
    fn gated_worker() -> (Worker, std::sync::mpsc::Sender<()>) {
        let (open, gate) = std::sync::mpsc::channel::<()>();
        let worker = Worker::start_with(
            Box::new(|_| Page {
                tasks: Vec::new(),
                more: false,
            }),
            Box::new(move |ask| {
                gate.recv().ok();
                ask.rows
                    .iter()
                    .map(|(id, _)| (id.clone(), format!("said {}", ask.query.phrase())))
                    .collect()
            }),
        );
        (worker, open)
    }

    fn wait_for(screen: &mut SessionsScreen) {
        for _ in 0..200 {
            if screen.poll() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the worker answered nothing");
    }

    /// SCREEN-17. What the sessions say is asked for the first time a phrase is typed and not when
    /// the screen opens or the search is, and the keys answer while it is read.
    #[test]
    fn what_the_sessions_say_is_read_on_the_worker_after_the_first_character_and_the_keys_do_not_wait()
     {
        let (worker, open) = gated_worker();
        let mut screen = sessions_screen(mixed()).with_worker(worker);
        assert_eq!(
            screen.worker.as_ref().map(Worker::outstanding),
            Some((0, 0))
        );
        press(&mut screen, KeyCode::Char('/'));
        assert_eq!(
            screen.worker.as_ref().map(Worker::outstanding),
            Some((0, 0)),
            "opening the search read the records"
        );

        // The read is held shut, so a key that waited for it would never return.
        assert_eq!(press(&mut screen, KeyCode::Char('t')), Outcome::Redraw);
        assert_eq!(press(&mut screen, KeyCode::Char('i')), Outcome::Redraw);
        assert_eq!(screen.search(), "ti");
        assert!(
            screen.due(Instant::now()),
            "nothing says a read is under way"
        );
        assert!(screen.due_in(Instant::now()) <= ANSWER_POLL);
        assert!(screen.said_in(&screen.rows()[0]).is_none());

        open.send(()).unwrap();
        open.send(()).unwrap();
        wait_for(&mut screen);
        assert!(!screen.rows().is_empty());
        let phrase_now = screen.search().to_string();
        assert_eq!(screen.said.0, phrase_now);
        assert!(
            screen
                .said_in(&screen.rows()[0])
                .is_some_and(|line| line == "said ti")
        );
    }

    /// SCREEN-17. An answer to text that has since changed is dropped: with "a" and then "ab" asked
    /// for and only the first answered, nothing is applied, and the answer for "ab" is.
    #[test]
    fn an_answer_for_text_that_has_changed_is_not_applied() {
        let (worker, open) = gated_worker();
        let mut screen = sessions_screen(mixed()).with_worker(worker);
        press(&mut screen, KeyCode::Char('/'));
        typed(&mut screen, "ab");

        // The worker answers in the order it was asked, so the first answer is the one for "a".
        open.send(()).unwrap();
        wait_for(&mut screen);
        assert_eq!(screen.said.0, "ab", "the answer for \"a\" was applied");
        assert!(screen.said.1.is_empty(), "lines for \"a\" were applied");

        open.send(()).unwrap();
        wait_for(&mut screen);
        assert_eq!(screen.said.0, "ab");
        assert!(!screen.said.1.is_empty());
        assert!(screen.said.1.values().all(|line| line == "said ab"));
    }

    /// SCREEN-17. A refresh keeps the lines found for the phrase in the box until the worker answers
    /// the new ask, so a row that matched only by what was said stays where it is.
    #[test]
    fn a_refresh_keeps_the_lines_of_what_was_said_until_the_new_answer_arrives() {
        let (worker, open) = gated_worker();
        let mut screen = sessions_screen(mixed()).with_worker(worker);
        press(&mut screen, KeyCode::Char('/'));
        typed(&mut screen, "ab");
        open.send(()).unwrap();
        open.send(()).unwrap();
        wait_for(&mut screen);
        std::thread::sleep(Duration::from_millis(30));
        screen.poll();
        let kept = screen.said.1.clone();
        assert!(!kept.is_empty());
        assert_eq!(
            ids(&screen).len(),
            5,
            "rows matched by what was said are missing"
        );

        // The list is read again, and the worker is asked for the words again and holds the answer.
        screen.replace(page(mixed()));

        assert_eq!(screen.said.1, kept, "the lines were dropped by the refresh");
        assert_eq!(ids(&screen).len(), 5, "the refresh took rows off the list");
        drop(open);
    }

    /// SCREEN-4. Delete in the search box is not the key that asks to delete the first match.
    #[test]
    fn delete_in_the_search_box_asks_nothing() {
        let mut screen = SessionsScreen::new(Grouping::Together, Vec::new(), HashMap::new(), None)
            .typing_first();
        screen.replace(page(mixed()));
        typed(&mut screen, "title");

        assert_eq!(press(&mut screen, KeyCode::Delete), Outcome::Nothing);
        assert!(screen.confirming().is_none(), "Delete asked to delete");
        assert_eq!(screen.search(), "title");
        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Delete);
        assert!(
            screen.confirming().is_some(),
            "Delete in the list no longer asks"
        );
    }

    /// SCREEN-5. This directory is told from the others as a path is, so a trailing separator does
    /// not make it another one.
    #[test]
    fn this_directory_is_compared_as_a_path() {
        let screen = sessions_screen(Vec::new()).in_directory(PathBuf::from("/work/home/"));
        assert!(screen.is_here("/work/home"));
        assert!(!screen.is_here("/work/homes"));
    }

    /// SCREEN-17. The list is asked for on the worker and drawn when it is answered, not waited for.
    #[test]
    fn the_list_is_read_on_the_worker_and_the_keys_do_not_wait_for_it() {
        let (release, held) = std::sync::mpsc::channel::<()>();
        let tasks = mixed();
        let worker = Worker::start_with(
            Box::new(move |_| {
                held.recv().ok();
                Page {
                    tasks: tasks.clone(),
                    more: false,
                }
            }),
            Box::new(|_| HashMap::new()),
        );
        let mut screen = SessionsScreen::new(Grouping::Together, Vec::new(), HashMap::new(), None)
            .with_worker(worker);
        let now = Instant::now();
        assert!(!screen.read_later(Path::new("/nowhere"), None, None, |_, _| false, now));
        assert!(
            screen.rows().is_empty(),
            "the list was read before the worker answered"
        );
        assert_eq!(
            press(&mut screen, KeyCode::Char('g')),
            Outcome::Effect(Effect::Grouping(Grouping::Project))
        );

        release.send(()).unwrap();
        wait_for(&mut screen);
        assert_eq!(screen.rows().len(), 5);
        assert!(
            !screen.due(Instant::now()),
            "still waiting for an answer that came"
        );
    }

    /// SCREEN-5. By agent there is a heading for each agent and one for the sessions started with
    /// none; by model, one for each model. The three partitions differ, so a heading that was the
    /// directory, or the model, in the place of the agent is seen.
    #[test]
    fn an_agent_or_a_model_is_a_heading_and_a_session_with_none_has_its_own() {
        let mut one = task("a", "/work/one", Status::Saved, 50);
        one.agent = Some("reviewer".to_string());
        one.model = Some("model-a".to_string());
        let mut two = task("b", "/work/two", Status::Saved, 40);
        two.agent = Some("reviewer".to_string());
        two.model = Some("model-b".to_string());
        let mut three = task("c", "/work/one", Status::Saved, 30);
        three.agent = None;
        three.model = Some("model-b".to_string());
        let mut screen = sessions_screen(vec![one, two, three]);
        let at = |screen: &SessionsScreen, id: &str| {
            screen
                .rows()
                .iter()
                .position(|row| row.task.id == id)
                .expect("a row")
        };
        let headings = |screen: &SessionsScreen| {
            screen
                .entries()
                .into_iter()
                .filter(|entry| matches!(entry, Entry::Group(_)))
                .count()
        };

        screen.group_by(Grouping::Agent);
        assert_eq!(headings(&screen), 2, "reviewer, and none");
        let (a, b, c) = (at(&screen, "a"), at(&screen, "b"), at(&screen, "c"));
        assert!(screen.same_group(a, b), "the agent is not what groups");
        assert!(
            !screen.same_group(b, c),
            "a session with no agent is under an agent"
        );

        screen.group_by(Grouping::Model);
        assert_eq!(headings(&screen), 2, "model-a and model-b");
        assert!(screen.same_group(b, c), "the model is not what groups");
        assert!(!screen.same_group(a, b));
    }

    /// SCREEN-4. Leaving the search takes the lines of what was said away with it.
    #[test]
    fn leaving_the_search_takes_the_lines_of_what_was_said_away() {
        let mut screen = remembering();
        assert_eq!(found_by(&mut screen, "staging"), ["a"]);
        assert!(
            screen
                .entries()
                .iter()
                .any(|entry| matches!(entry, Entry::Said(_)))
        );
        press(&mut screen, KeyCode::Esc);
        assert!(!screen.searching());
        assert!(
            !screen
                .entries()
                .iter()
                .any(|entry| matches!(entry, Entry::Said(_))),
            "a line found for a phrase that is gone is still drawn"
        );
    }

    /// SCREEN-4. A search reads every record and not only the page, and clearing it goes back to the page.
    #[test]
    fn a_search_asks_for_every_record_and_clearing_it_goes_back_to_a_page() {
        let mut screen = sessions_screen(mixed());
        assert_eq!(screen.wanted(), PAGE);
        press(&mut screen, KeyCode::Char('/'));
        typed(&mut screen, "x");
        assert_eq!(screen.wanted(), usize::MAX);
        press(&mut screen, KeyCode::Backspace);
        assert_eq!(screen.wanted(), PAGE);
    }

    /// SCREEN-4. A screen opened with the box focused takes letters as the search, Right and Left
    /// move nothing and open nothing, Enter opens the first match, and Escape on an empty box leaves.
    #[test]
    fn a_screen_opened_for_typing_takes_letters_as_text_and_enter_opens_the_first_match() {
        let mut screen = SessionsScreen::new(Grouping::Together, Vec::new(), HashMap::new(), None)
            .typing_first();
        screen.replace(page(mixed()));
        assert!(screen.searching());

        assert_eq!(press(&mut screen, KeyCode::Char('x')), Outcome::Redraw);
        assert_eq!(
            screen.search(),
            "x",
            "a letter was taken for a key of the list"
        );
        press(&mut screen, KeyCode::Backspace);
        assert_eq!(press(&mut screen, KeyCode::Right), Outcome::Nothing);
        assert_eq!(press(&mut screen, KeyCode::Left), Outcome::Nothing);

        typed(&mut screen, "title d");
        let first = selected(&screen).map(str::to_string);
        assert_eq!(first.as_deref(), Some("d"));
        match press(&mut screen, KeyCode::Enter) {
            Outcome::Effect(Effect::Open(task)) => assert_eq!(task.id, "d"),
            other => panic!("Enter in the box did not open the first match: {other:?}"),
        }
        // An open that the loop then refuses leaves the screen as it was, so the search is kept.
        assert_eq!(screen.search(), "title d", "opening cleared the search");

        let mut empty = SessionsScreen::new(Grouping::Together, Vec::new(), HashMap::new(), None)
            .typing_first();
        empty.replace(page(mixed()));
        assert_eq!(press(&mut empty, KeyCode::Esc), Outcome::Close);
    }

    /// SCREEN-4. Text left in the box while the cursor is in the list is cleared by Escape, which
    /// leaves the screen only when there is none.
    #[test]
    fn escape_clears_text_left_in_the_box_before_it_leaves() {
        let mut screen = sessions_screen(mixed());
        found_by(&mut screen, "title");
        press(&mut screen, KeyCode::Down);
        assert!(!screen.searching() && screen.search() == "title");
        assert_eq!(press(&mut screen, KeyCode::Esc), Outcome::Redraw);
        assert!(screen.search().is_empty());
        assert_eq!(press(&mut screen, KeyCode::Esc), Outcome::Close);
    }

    /// SCREEN-15. The question about stopping a turn names a session, is answered by `y` with the
    /// stop and what follows it, is withdrawn by `n`, Escape and Ctrl-C, and is not asked where there
    /// is no session to name.
    #[test]
    fn the_question_about_a_running_turn_is_answered_by_y_alone() {
        let mut screen = sessions_screen(mixed());
        let task = screen.selected_row().expect("a row").task.clone();
        let then = Effect::Fork(Box::new(task.clone()));

        assert!(screen.ask_to_stop_the_turn(then.clone()));
        let confirm = screen.confirming().expect("a question");
        assert_eq!(confirm.what, Destructive::Interrupt);
        assert_eq!(confirm.task.id, task.id);

        assert_eq!(press(&mut screen, KeyCode::Enter), Outcome::Nothing);
        assert_eq!(press(&mut screen, KeyCode::Char('x')), Outcome::Nothing);
        assert_eq!(press(&mut screen, KeyCode::Esc), Outcome::Redraw);
        assert!(screen.confirming().is_none());

        assert!(screen.ask_to_stop_the_turn(then.clone()));
        assert_eq!(
            press(&mut screen, KeyCode::Char('y')),
            Outcome::Effect(Effect::StopTurnThen(Box::new(then)))
        );
        assert!(screen.confirming().is_none());

        let mut empty = sessions_screen(Vec::new());
        assert!(
            !empty.ask_to_stop_the_turn(Effect::New(None)),
            "a question was put with no session to name"
        );
        assert!(empty.confirming().is_none());
    }

    fn keyed(pairs: &[(&str, &str)]) -> SessionsScreen {
        let configured = pairs
            .iter()
            .map(|(name, key)| ((*name).to_string(), (*key).to_string()))
            .collect();
        let (keys, refused) = SessionsKeys::from_map(&configured);
        assert!(refused.is_empty(), "{refused:?}");
        sessions_screen(mixed()).with_keys(keys)
    }

    fn press_with(screen: &mut SessionsScreen, code: KeyCode, modifiers: KeyModifiers) -> Outcome {
        screen.key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    /// SCREEN-14. A key an action was moved onto does what the action does, and the key it was moved
    /// off does nothing.
    #[test]
    fn a_moved_key_acts_and_the_key_it_left_does_nothing() {
        let mut screen = keyed(&[("archive", "z"), ("pin", "ctrl-y"), ("group", "w")]);

        press(&mut screen, KeyCode::Char('a'));
        assert!(
            screen.confirming().is_none(),
            "the key it was moved off still asked"
        );
        press(&mut screen, KeyCode::Char('z'));
        assert!(
            matches!(
                screen.confirming(),
                Some(Confirm {
                    what: Destructive::Archive,
                    ..
                })
            ),
            "the key it was moved onto did not ask"
        );
        press(&mut screen, KeyCode::Esc);

        press(&mut screen, KeyCode::Char('p'));
        assert!(
            screen.pinned().is_empty(),
            "the key pin was moved off still pinned"
        );
        press_with(&mut screen, KeyCode::Char('y'), KeyModifiers::CONTROL);
        assert_eq!(screen.pinned().len(), 1, "a Ctrl key did not pin");

        assert_eq!(press(&mut screen, KeyCode::Char('g')), Outcome::Nothing);
        assert!(matches!(
            press(&mut screen, KeyCode::Char('w')),
            Outcome::Effect(Effect::Grouping(Grouping::Project))
        ));
    }

    /// SCREEN-14. Delete moved to a letter takes Backspace and Delete with it, and a key not moved,
    /// and the keys the screen keeps, answer as they did.
    #[test]
    fn a_moved_delete_takes_backspace_and_delete_with_it_and_the_rest_stay() {
        let mut screen = keyed(&[("delete", "d")]);
        press(&mut screen, KeyCode::Backspace);
        press(&mut screen, KeyCode::Delete);
        assert!(screen.confirming().is_none());
        press(&mut screen, KeyCode::Char('d'));
        assert!(matches!(
            screen.confirming(),
            Some(Confirm {
                what: Destructive::Delete,
                ..
            })
        ));
        press(&mut screen, KeyCode::Esc);

        let before = selected(&screen).map(str::to_string);
        press(&mut screen, KeyCode::Down);
        assert_ne!(
            selected(&screen).map(str::to_string),
            before,
            "Down was taken"
        );
        assert!(matches!(
            press(&mut screen, KeyCode::Char('2')),
            Outcome::Redraw
        ));
    }

    /// SCREEN-14. A moved key is a letter in a search and in a rename, as every letter is.
    #[test]
    fn a_moved_key_is_text_in_a_search_and_in_a_rename() {
        let mut screen = keyed(&[("archive", "z"), ("hide", "q")]);
        press(&mut screen, KeyCode::Char('/'));
        typed(&mut screen, "zq");
        assert_eq!(screen.search(), "zq");
        assert!(screen.confirming().is_none());
        press(&mut screen, KeyCode::Esc);

        press(&mut screen, KeyCode::Char('r'));
        typed(&mut screen, "zq");
        let text = screen.renaming().map(|renaming| renaming.text.clone());
        assert!(text.is_some_and(|text| text.ends_with("zq")));
    }

    /// SCREEN-14. The help key in force opens and closes the help, and the one it was moved off does
    /// neither.
    #[test]
    fn the_moved_help_key_opens_and_closes_the_help() {
        let mut screen = keyed(&[("help", "f2")]);
        press(&mut screen, KeyCode::Char('?'));
        assert!(!screen.helping(), "the key help was moved off opened it");
        press(&mut screen, KeyCode::F(2));
        assert!(
            screen.helping(),
            "the key help was moved onto did not open it"
        );
        press(&mut screen, KeyCode::Char('?'));
        assert!(screen.helping(), "the key help was moved off closed it");
        press(&mut screen, KeyCode::F(2));
        assert!(
            !screen.helping(),
            "the key help was moved onto did not close it"
        );
    }
}
