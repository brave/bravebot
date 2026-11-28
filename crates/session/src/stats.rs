//! What the recorded sessions spent, totalled across the store (CLI-37).
//!
//! Reads the records under the sessions directory and adds up what each says: how many turns it
//! had, how many tokens it spent, how many of those the model read and how many it wrote, what the
//! service's cache read and wrote, and which model and which service answered last. Nothing is
//! written, and
//! nothing in a record is read for what it says: the figures are the ones the program itself wrote
//! down, and the model name is the one the server reported.
//!
//! A record holds one total for the session, and the model that answered its last turn. A total
//! cannot be split by day inside a session or by model across turns, so a session is counted
//! whole, under the day it was last written and the model it last used.
//!
//! The split into read and written covers what the record kept it for. A record written before the
//! split was kept has none, and a session that went on after one has it for the later turns only,
//! so the two figures can total less than the tokens, and [`Spent::unsplit`] is the difference.
//!
//! The cache figures are added over the records that carry them, and the hit rate is taken over the
//! tokens those records read and no others. A record keeps the figures, zeros included, for every
//! session whose service stated them, so a record without them is counted apart: one that names the
//! service that answered ran on a service that states no cache figure, and one that does not may be
//! older than the figures.

use crate::search;
use crate::sessions;
use bravebot_agent::BackendKind;
use bravebot_i18n::t;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The longest model name a row shows, in characters.
pub const LONGEST_MODEL: usize = 60;

/// A row's model, as the record named it, and the service that answered it, `None` where the
/// record names none.
pub type ModelAndService = (Option<String>, Option<BackendKind>);

/// What a total covers.
#[derive(Debug, Clone, Copy, Default)]
pub struct Scope<'a> {
    /// Only the sessions whose record says they ran in this directory. Every project's sessions
    /// where `None`.
    ///
    /// Decided by the path the record kept and never by the name of the directory holding it,
    /// which two working directories can share ([`sessions::key_for`]).
    pub project: Option<&'a Path>,
    /// Only the sessions last written at or after this time, in seconds since the epoch.
    pub since: Option<u64>,
}

/// The figures that add up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Spent {
    pub sessions: u64,
    pub turns: u64,
    pub tokens: u64,
    /// Of `tokens`, those the model read and those it wrote, for the sessions that recorded the
    /// split, and the cache figures of the sessions that recorded those.
    pub split: sessions::Split,
    /// How many of `sessions` carry cache figures in their record.
    pub cache_sessions: u64,
    /// The tokens read by the sessions counted in `cache_sessions`, which the hit rate is taken over.
    pub cache_sessions_read: u64,
    /// How many of the sessions that carry no cache figures name no service either, so their record
    /// may have been written before the cache figures were kept.
    pub older_without_cache: u64,
}

impl Spent {
    /// What one record contributes.
    fn of(counted: &Counted) -> Self {
        // A record somebody edited by hand can name a split past its own total, which would make
        // `unsplit` wrap; the split is cut to the total so a session never reads as more than it
        // spent.
        let mut split = counted.split.of().within(counted.tokens);
        // The same record can state more served from the cache than it read, which would raise the
        // hit rate of its row past what the row read; the cache read is cut to the read.
        split.cache.read_tokens = split.cache.read_tokens.min(split.read);
        let carries_cache = counted.split.cache.is_some();
        Self {
            sessions: 1,
            turns: counted.turns,
            tokens: counted.tokens,
            split,
            cache_sessions: u64::from(carries_cache),
            cache_sessions_read: if carries_cache { split.read } else { 0 },
            older_without_cache: u64::from(!carries_cache && counted.service.is_none()),
        }
    }

    /// How many sessions carry no cache figures in their record.
    pub fn without_cache(&self) -> u64 {
        self.sessions.saturating_sub(self.cache_sessions)
    }

    /// How many of the sessions that carry no cache figures name the service that answered, which
    /// therefore stated no cache figure: a record keeps the figures, zeros included, for every
    /// session whose service stated them.
    pub fn silent_without_cache(&self) -> u64 {
        self.without_cache()
            .saturating_sub(self.older_without_cache)
    }

    /// The share of the tokens read by the sessions that carry cache figures that the service
    /// answered out of its cache, in tenths of a percent, or `None` where those sessions read
    /// nothing.
    ///
    /// Cut to the whole where a record edited by hand states more served from the cache than read.
    pub fn hit_rate_permille(&self) -> Option<u64> {
        (self.cache_sessions_read > 0).then(|| {
            let served = u128::from(self.split.cache.read_tokens.min(self.cache_sessions_read));
            let rate = served * 1000 / u128::from(self.cache_sessions_read);
            u64::try_from(rate).unwrap_or(1000)
        })
    }

    /// The hit rate as a row shows it: to a tenth of a percent with the catalog's decimal separator
    /// and percent form, or `-` where the sessions that carry cache figures read nothing. The
    /// terminal prints it and the desktop bridge sends it (CLI-37).
    pub fn shown_hit_rate(&self) -> String {
        match self.hit_rate_permille() {
            Some(permille) => t!(
                stats_hit_rate,
                rate = format!(
                    "{}{}{}",
                    permille / 10,
                    t!(number_decimal_separator),
                    permille % 10
                )
            ),
            None => "-".to_string(),
        }
    }

    /// The note on the sessions the hit rate leaves out, how many of them ran on a service that
    /// states no cache figure and how many name no service, or `None` where every session carries
    /// cache figures. The terminal prints it on stderr and the desktop bridge sends it (CLI-37).
    pub fn without_cache_note(&self) -> Option<String> {
        (self.without_cache() > 0).then(|| {
            t!(
                stats_without_cache,
                count = self.without_cache(),
                silent = self.silent_without_cache(),
                older = self.older_without_cache
            )
        })
    }

    /// The tokens no split was recorded for: what the total holds beyond the two figures.
    pub fn unsplit(&self) -> u64 {
        self.tokens
            .saturating_sub(self.split.read)
            .saturating_sub(self.split.written)
    }

    /// Add another total to this one.
    ///
    /// Saturating, because a record somebody edited by hand can hold any number, and a total that
    /// wrapped or panicked on one would take the figures of every other record with it.
    pub fn merge(&mut self, other: Spent) {
        self.sessions = self.sessions.saturating_add(other.sessions);
        self.turns = self.turns.saturating_add(other.turns);
        self.tokens = self.tokens.saturating_add(other.tokens);
        self.split.add(other.split);
        self.cache_sessions = self.cache_sessions.saturating_add(other.cache_sessions);
        self.cache_sessions_read = self
            .cache_sessions_read
            .saturating_add(other.cache_sessions_read);
        self.older_without_cache = self
            .older_without_cache
            .saturating_add(other.older_without_cache);
    }
}

/// The totals over a scope.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Every session in the scope.
    pub all: Spent,
    /// The same, split by the model each session last used and by the service that answered it, as
    /// the record named them. `None` for the model is a session whose record names none: one
    /// written before the model was kept, one whose turns never reached a server, and one copied in
    /// from another program. `None` for the service is a record that names none, or names a word
    /// that is no service.
    pub by_model_and_service: BTreeMap<ModelAndService, Spent>,
    /// How many entries under the sessions directory could not be read as a record: a `.json` file
    /// that does not parse as one, and a directory or an entry that could not be listed. They are
    /// counted and not named, and add to no figure above.
    pub unreadable: u64,
}

/// The fields of a record a total needs, and nothing else.
///
/// Its own shape rather than [`sessions::Record`], for the reason the picker's rows have one: a
/// record holds the conversation and the bytes its turns wrote over, and building all of that to add
/// up three numbers would allocate a copy of it for every session in the store. The file is still
/// read whole.
///
/// A file is a record here when a listing would show it as one, so the id and the title are
/// required though nothing reads them, and `stats` and `sessions list` agree on how many sessions
/// there are.
#[derive(Deserialize)]
struct Counted {
    #[serde(rename = "id")]
    _id: String,
    #[serde(rename = "title")]
    _title: String,
    directory: String,
    updated: u64,
    /// Absent from a record written before turns were counted, which reads as none.
    #[serde(default)]
    turns: u64,
    /// Absent from a record written before spend was kept, which reads as none.
    #[serde(default)]
    tokens: u64,
    /// Absent from a record written before the split was kept, which reads as none recorded.
    #[serde(default)]
    split: CountedSplit,
    /// Absent from a record written before the model was kept.
    #[serde(default)]
    model: Option<String>,
    /// The word for the service that answered, absent from a record written before it was kept.
    #[serde(default)]
    service: Option<String>,
}

/// The split as a record holds it, with whether it carries the cache figures at all, which
/// [`sessions::Split`] reads as zero when they are absent.
#[derive(Deserialize, Default)]
struct CountedSplit {
    #[serde(default)]
    read: u64,
    #[serde(default)]
    written: u64,
    #[serde(default)]
    cache: Option<bravebot_aichat::protocol::Cached>,
}

impl CountedSplit {
    fn of(&self) -> sessions::Split {
        sessions::Split {
            read: self.read,
            written: self.written,
            cache: self.cache.unwrap_or_default(),
        }
    }
}

impl Counted {
    fn in_scope(&self, scope: &Scope<'_>) -> bool {
        scope
            .project
            .is_none_or(|project| Path::new(&self.directory) == project)
            && scope.since.is_none_or(|since| self.updated >= since)
    }
}

impl Totals {
    /// A row for each model as a person is shown it and each service that answered it, the one that
    /// spent most first.
    ///
    /// The name the server reported is cleaned of anything a terminal would act on and cut to
    /// [`LONGEST_MODEL`] characters (VIEW-4). Rows are grouped on the cleaned name, so two names
    /// that differ only in what cleaning removes are one row rather than two that read alike. A
    /// record that names no model, and a name with nothing left after cleaning, are the row for no
    /// name.
    ///
    /// Ordered by tokens, then sessions, most first, then by name, with the row for no name after
    /// every named row of the same figures, then by service, with the row for no service after the
    /// named ones.
    pub fn by_shown_model_and_service(&self) -> Vec<(ModelAndService, Spent)> {
        let mut shown: BTreeMap<ModelAndService, Spent> = BTreeMap::new();
        for ((model, service), spent) in &self.by_model_and_service {
            shown
                .entry((shown_model(model), *service))
                .or_default()
                .merge(*spent);
        }
        let mut rows: Vec<_> = shown.into_iter().collect();
        rows.sort_by(
            |((left_name, left_service), left), ((right_name, right_service), right)| {
                most_spent_first(left, right)
                    .then_with(|| named_first(left_name, right_name))
                    .then_with(|| named_first(left_service, right_service))
            },
        );
        rows
    }

    fn take(&mut self, counted: Counted) {
        let spent = Spent::of(&counted);
        self.all.merge(spent);
        let service = counted.service.as_deref().and_then(BackendKind::named);
        self.by_model_and_service
            .entry((counted.model, service))
            .or_default()
            .merge(spent);
    }

    /// What is in `directory`, or nothing where there is no such directory, which is the ordinary
    /// case for a project that has no sessions. A directory that is there and cannot be listed, and
    /// an entry in it that cannot be read, are each counted as one that could not be read: a
    /// listing that stopped quietly would report a store it never looked in as an empty one.
    fn entries_of(&mut self, directory: &Path) -> Vec<PathBuf> {
        match std::fs::read_dir(directory) {
            Ok(entries) => entries
                .filter_map(|entry| match entry {
                    Ok(entry) => Some(entry.path()),
                    Err(_) => {
                        self.unreadable += 1;
                        None
                    }
                })
                .collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(_) => {
                self.unreadable += 1;
                Vec::new()
            }
        }
    }
}

/// A model's name as a row shows it: cleaned of anything a terminal would act on and cut to
/// [`LONGEST_MODEL`] characters (VIEW-4), with a name that has nothing left being no name.
fn shown_model(model: &Option<String>) -> Option<String> {
    model
        .as_deref()
        .map(|name| search::clean(name, LONGEST_MODEL))
        .filter(|name| !name.is_empty())
}

/// The word a row names its service by, one fixed word for each kind of backend and `unknown` for
/// none. Chosen from the closed set of backends, never from the record. The terminal prints it and
/// the desktop bridge sends it (CLI-37).
pub fn service_word(service: Option<BackendKind>) -> &'static str {
    match service {
        Some(BackendKind::Aichat) => t!(stats_service_aichat),
        Some(BackendKind::Bedrock) => t!(stats_service_bedrock),
        Some(BackendKind::Gateway) => t!(stats_service_gateway),
        Some(BackendKind::Chatgpt) => t!(stats_service_chatgpt),
        Some(BackendKind::Xai) => t!(stats_service_xai),
        None => t!(stats_service_unknown),
    }
}

/// More tokens first, then more sessions.
fn most_spent_first(left: &Spent, right: &Spent) -> std::cmp::Ordering {
    right
        .tokens
        .cmp(&left.tokens)
        .then(right.sessions.cmp(&left.sessions))
}

/// Named values in their own order, then the `None` row.
fn named_first<T: Ord>(left: &Option<T>, right: &Option<T>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// The totals over the sessions directory this process reads, which is nothing where there is no
/// state directory.
pub fn totals(scope: Scope<'_>) -> Totals {
    match sessions::sessions_root() {
        Some(root) => totals_under(&root, scope),
        None => Totals::default(),
    }
}

/// The totals over the stores directly under `root`, or over the one store the scope's project
/// names.
///
/// A project's store is read without being listed first, since its name is derived from the path;
/// the records inside are still filtered on the path they kept. With no project, every directory
/// under `root` is a store, and the real path of each project is read from its records rather
/// than reconstructed from the directory's name.
fn totals_under(root: &Path, scope: Scope<'_>) -> Totals {
    let mut totals = Totals::default();
    let stores: Vec<PathBuf> = match scope.project {
        Some(project) => vec![root.join(sessions::key_for(project))],
        None => totals
            .entries_of(root)
            .into_iter()
            .filter(|path| path.is_dir())
            .collect(),
    };
    for store in stores {
        for path in totals.entries_of(&store).into_iter().filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        }) {
            match read(&path) {
                Some(counted) if counted.in_scope(&scope) => totals.take(counted),
                Some(_) => {}
                None => totals.unreadable += 1,
            }
        }
    }
    totals
}

fn read(path: &Path) -> Option<Counted> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Seconds in a day of 24 hours, which a count of days goes back from now in.
const SECONDS_IN_A_DAY: u64 = 86_400;

/// The most days a count may name. The terminal's sentence says the number back through a message
/// that counts in `i64`, and a larger one would be printed as another number than the one the cut
/// used.
pub const MOST_DAYS: u64 = i64::MAX as u64;

/// The [`Scope::since`] for the last `days` days, counted back from `now`, or `None` for a count
/// of zero or one past [`MOST_DAYS`].
pub fn since_days_ago_from(now: u64, days: u64) -> Option<u64> {
    (1..=MOST_DAYS)
        .contains(&days)
        .then(|| now.saturating_sub(days.saturating_mul(SECONDS_IN_A_DAY)))
}

/// [`since_days_ago_from`] the present moment.
pub fn since_days_ago(days: u64) -> Option<u64> {
    since_days_ago_from(sessions::now(), days)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::scratch_dir;
    use std::path::PathBuf;

    #[test]
    fn a_count_of_days_goes_back_that_many_days_of_seconds() {
        assert_eq!(since_days_ago_from(10 * 86_400, 3), Some(7 * 86_400));
        // Further back than the epoch stops at it.
        assert_eq!(since_days_ago_from(86_400, 2), Some(0));
        assert_eq!(since_days_ago_from(86_400, MOST_DAYS), Some(0));
    }

    #[test]
    fn a_count_of_zero_or_past_the_most_is_refused() {
        assert_eq!(since_days_ago_from(86_400, 0), None);
        assert_eq!(since_days_ago_from(86_400, MOST_DAYS + 1), None);
        assert_eq!(since_days_ago_from(86_400, u64::MAX), None);
    }

    /// A sessions directory with one store per project, which a test fills with records.
    struct Store {
        root: PathBuf,
    }

    impl Store {
        fn new(name: &str) -> Self {
            let root = scratch_dir(name);
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("create the sessions directory");
            Self { root }
        }

        /// A record, as its own file in the store `project` is kept under.
        fn write(&self, project: &Path, id: &str, body: &str) {
            let store = self.root.join(sessions::key_for(project));
            std::fs::create_dir_all(&store).expect("create the store");
            std::fs::write(store.join(format!("{id}.json")), body).expect("write the record");
        }

        /// A record that says it ran in `project`, written the way a session writes the path.
        fn session(
            &self,
            project: &Path,
            id: &str,
            updated: u64,
            turns: u64,
            tokens: u64,
            model: Option<&str>,
        ) {
            self.session_split(project, id, updated, (turns, tokens), None, model);
        }

        /// The same, from a build that kept how the tokens divided.
        fn session_split(
            &self,
            project: &Path,
            id: &str,
            updated: u64,
            (turns, tokens): (u64, u64),
            split: Option<(u64, u64)>,
            model: Option<&str>,
        ) {
            let mut record = serde_json::json!({
                "id": id,
                "title": "t",
                "directory": project.display().to_string(),
                "updated": updated,
                "turns": turns,
                "tokens": tokens,
                "model": model,
            });
            if let Some((read, written)) = split {
                record["split"] = serde_json::json!({ "read": read, "written": written });
            }
            self.write(project, id, &record.to_string());
        }
    }

    impl Drop for Store {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    const NOW: u64 = 1_700_000_000;
    const DAY: u64 = 86_400;

    /// What `sessions` records spent, where none of them carries cache figures or names a service,
    /// as every record [`Store::session`] writes.
    fn spent(sessions: u64, turns: u64, tokens: u64) -> Spent {
        Spent {
            sessions,
            turns,
            tokens,
            split: sessions::Split::default(),
            cache_sessions: 0,
            cache_sessions_read: 0,
            older_without_cache: sessions,
        }
    }

    /// The same, for sessions that recorded how the tokens divided.
    fn spent_split(sessions: u64, turns: u64, read: u64, written: u64) -> Spent {
        Spent {
            split: sessions::Split {
                read,
                written,
                ..Default::default()
            },
            ..spent(sessions, turns, read + written)
        }
    }

    /// The total is a sum. A total that kept the last record's figures would read as the spend of
    /// one session.
    #[test]
    fn two_records_are_added_into_one_total() {
        let store = Store::new("stats-two-records");
        let here = Path::new("/w/one");
        store.session(here, "a", NOW, 3, 1_000, Some("m"));
        store.session(here, "b", NOW, 5, 250, Some("m"));

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(totals.all, spent(2, 8, 1_250));
        assert_eq!(totals.unreadable, 0);
    }

    /// A cut that kept a session written exactly at the edge, and dropped the one a second
    /// before it, is the boundary the day count is measured to.
    #[test]
    fn a_session_last_written_before_the_cut_is_left_out() {
        let store = Store::new("stats-the-day-cut");
        let here = Path::new("/w/one");
        store.session(here, "inside", NOW - 2 * DAY, 2, 100, Some("m"));
        store.session(here, "edge", NOW - 3 * DAY, 4, 10, Some("m"));
        store.session(here, "outside", NOW - 3 * DAY - 1, 8, 1, Some("m"));

        let since = Some(NOW - 3 * DAY);
        let totals = totals_under(
            &store.root,
            Scope {
                project: None,
                since,
            },
        );

        assert_eq!(totals.all, spent(2, 6, 110));
    }

    /// A person asking about one project is not asking about the others, and the total over every
    /// project is the sum of the rows, so neither may drop or repeat a record.
    #[test]
    fn a_project_total_holds_the_sessions_that_ran_there_and_no_others() {
        let store = Store::new("stats-one-project");
        let one = Path::new("/w/one");
        let two = Path::new("/w/two");
        store.session(one, "a", NOW, 1, 10, Some("m"));
        store.session(two, "b", NOW, 2, 200, Some("m"));
        store.session(two, "c", NOW, 4, 3_000, Some("m"));

        let for_two = totals_under(
            &store.root,
            Scope {
                project: Some(two),
                since: None,
            },
        );
        let everywhere = totals_under(&store.root, Scope::default());

        assert_eq!(for_two.all, spent(2, 6, 3_200));
        assert_eq!(everywhere.all, spent(3, 7, 3_210));
    }

    /// Two directories can share one store, since the name it is kept under is derived lossily.
    /// A record in the store of `/w/one` that says it ran in `/w/other` is not `/w/one`'s.
    #[test]
    fn a_record_in_a_shared_store_that_ran_elsewhere_is_not_the_projects() {
        let store = Store::new("stats-shared-store");
        let one = Path::new("/w/one");
        store.session(one, "mine", NOW, 1, 10, Some("m"));
        store.write(
            one,
            "theirs",
            &format!(
                r#"{{"id":"theirs","title":"t","directory":"/w/other","updated":{NOW},"turns":9,"tokens":900}}"#
            ),
        );

        let totals = totals_under(
            &store.root,
            Scope {
                project: Some(one),
                since: None,
            },
        );

        assert_eq!(totals.all, spent(1, 1, 10));
    }

    /// A model's row is what the sessions that last used it spent. A record that names none is its
    /// own row, so its spend is neither lost nor charged to a model that did not answer.
    #[test]
    fn each_model_is_totalled_on_its_own_and_a_record_naming_none_is_kept_apart() {
        let store = Store::new("stats-by-model");
        let here = Path::new("/w/one");
        store.session(here, "a", NOW, 1, 10, Some("big"));
        store.session(here, "b", NOW, 2, 20, Some("big"));
        store.session(here, "c", NOW, 4, 400, Some("small"));
        store.session(here, "d", NOW, 8, 8_000, None);

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(
            totals.by_model_and_service[&(Some("big".to_string()), None)],
            spent(2, 3, 30)
        );
        assert_eq!(
            totals.by_model_and_service[&(Some("small".to_string()), None)],
            spent(1, 4, 400)
        );
        assert_eq!(
            totals.by_model_and_service[&(None, None)],
            spent(1, 8, 8_000)
        );
        assert_eq!(totals.all, spent(4, 15, 8_430));
    }

    /// The file a person's editor half-wrote, a file that is not a record, and a record missing a
    /// field a listing needs are counted, are not named, and add nothing. Each missing field has a
    /// file of its own, so a field that stopped being required is found by the one that has it.
    #[test]
    fn a_file_that_is_not_a_record_is_counted_and_adds_to_nothing() {
        let store = Store::new("stats-unreadable");
        let here = Path::new("/w/one");
        store.session(here, "good", NOW, 1, 10, Some("m"));
        store.write(here, "torn", r#"{"id":"torn","directory":"/w/one","upd"#);
        store.write(here, "other", r#"{"hello":"world"}"#);
        store.write(
            here,
            "no-id",
            r#"{"title":"t","directory":"/w/one","updated":1}"#,
        );
        store.write(
            here,
            "no-title",
            r#"{"id":"x","directory":"/w/one","updated":1}"#,
        );
        store.write(
            here,
            "no-directory",
            r#"{"id":"x","title":"t","updated":1}"#,
        );
        store.write(
            here,
            "no-time",
            r#"{"id":"x","title":"t","directory":"/w/one"}"#,
        );
        store.write(
            here,
            "wrong-type",
            r#"{"id":"x","title":"t","directory":"/w/one","updated":1,"tokens":"many"}"#,
        );

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(totals.all, spent(1, 1, 10));
        assert_eq!(totals.unreadable, 7);
    }

    /// Every record has a trail and a search index beside it, under other extensions. They are
    /// not records, so they are neither read as one nor counted as one that could not be.
    #[test]
    fn the_trail_and_the_search_index_beside_a_record_are_not_records() {
        let store = Store::new("stats-beside-a-record");
        let here = Path::new("/w/one");
        store.session(here, "a", NOW, 3, 30, Some("m"));
        let directory = store.root.join(sessions::key_for(here));
        std::fs::write(directory.join("a.audit.jsonl"), "{}\n").expect("write the trail");
        std::fs::write(directory.join("a.search"), "said something\n").expect("write the index");

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(totals.all, spent(1, 3, 30));
        assert_eq!(totals.unreadable, 0);
    }

    /// An older record names no turns, no spend and no model. It is still a session that
    /// happened, so it counts as one, and the figures it never had are none.
    #[test]
    fn a_record_from_before_spend_was_kept_counts_as_a_session_with_none() {
        let store = Store::new("stats-old-record");
        let here = Path::new("/w/one");
        store.write(
            here,
            "old",
            &format!(r#"{{"id":"old","title":"t","directory":"/w/one","updated":{NOW}}}"#),
        );

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(totals.all, spent(1, 0, 0));
        assert_eq!(totals.by_model_and_service[&(None, None)], spent(1, 0, 0));
        assert_eq!(totals.unreadable, 0);
    }

    /// A record is a file a person can edit, and one holding the largest number must not take the
    /// figures of the others with it.
    #[test]
    fn a_hand_edited_figure_cannot_wrap_or_panic_the_total() {
        let store = Store::new("stats-huge-figures");
        let here = Path::new("/w/one");
        store.session(here, "a", NOW, u64::MAX, u64::MAX, Some("m"));
        store.session(here, "b", NOW, 5, 5, Some("m"));

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(totals.all, spent(2, u64::MAX, u64::MAX));
    }

    /// The split is added over the records that kept it, and a record that did not is left out of
    /// both figures. Counting its tokens as read would make the figures sum to the total while
    /// saying something no record does.
    #[test]
    fn the_split_totals_the_records_that_kept_it_and_leaves_the_rest_unsplit() {
        let store = Store::new("stats-split");
        let here = Path::new("/w/one");
        store.session_split(here, "a", NOW, (3, 1_000), Some((700, 300)), Some("m"));
        store.session_split(here, "b", NOW, (1, 100), Some((60, 40)), Some("m"));
        store.session(here, "old", NOW, 5, 400, Some("m"));

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(
            totals.all,
            Spent {
                sessions: 3,
                turns: 9,
                tokens: 1_500,
                split: sessions::Split {
                    read: 760,
                    written: 340,
                    ..Default::default()
                },
                ..spent(3, 9, 1_500)
            }
        );
        assert_eq!(totals.all.unsplit(), 400);
    }

    /// A session picked up from a record written before the split was kept holds the split for the
    /// turns after it, so what it leaves out is the older turns' tokens and not the whole session's.
    #[test]
    fn a_session_that_went_on_after_an_unsplit_record_is_split_for_the_later_turns_only() {
        let store = Store::new("stats-split-resumed");
        let here = Path::new("/w/one");
        store.session_split(here, "a", NOW, (4, 1_000), Some((150, 50)), Some("m"));

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(totals.all.split.read + totals.all.split.written, 200);
        assert_eq!(totals.all.unsplit(), 800);
    }

    /// A record somebody edited can name a split larger than its own total. The figures are cut to
    /// the total, so the tokens a session is said not to have a split for never wrap past zero.
    #[test]
    fn a_split_past_the_total_is_cut_to_it() {
        let store = Store::new("stats-split-past-total");
        let here = Path::new("/w/one");
        store.session_split(
            here,
            "a",
            NOW,
            (1, 100),
            Some((u64::MAX, u64::MAX)),
            Some("m"),
        );
        store.session_split(here, "b", NOW, (1, 100), Some((80, 20)), Some("m"));

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(totals.all.tokens, 200);
        assert_eq!(totals.all.split.written, 120);
        assert_eq!(totals.all.split.read, 80);
        assert_eq!(totals.all.unsplit(), 0);
    }

    /// A person with no sessions, and one asking about a project that has none, are told so with
    /// zeros, not with a failure.
    #[test]
    fn nothing_recorded_totals_to_nothing() {
        let store = Store::new("stats-empty");
        assert_eq!(
            totals_under(&store.root, Scope::default()),
            Totals::default()
        );
        assert_eq!(
            totals_under(&store.root.join("missing"), Scope::default()),
            Totals::default()
        );
        assert_eq!(
            totals_under(
                &store.root,
                Scope {
                    project: Some(Path::new("/w/never")),
                    since: None
                }
            ),
            Totals::default()
        );
    }

    /// The record keeps the directory as the text `Path::display` gave it, so a directory whose
    /// name is not UTF-8 is held as the replacement-character rendering, which is also what an
    /// ordinary directory could be called. A total for the directory with the odd name finds none
    /// of its own records by the exact bytes, since no record can hold them, and the ordinary
    /// directory the rendering spells finds the records written under either. The names are built
    /// in memory so this runs where the file system cannot hold the first.
    #[test]
    #[cfg(unix)]
    fn a_directory_that_is_not_utf8_is_told_from_an_ordinary_one_only_up_to_its_rendering() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let not_utf8 = PathBuf::from(OsStr::from_bytes(b"/w/\xff"));
        let rendering = not_utf8.to_string_lossy().into_owned();
        assert_ne!(
            rendering.as_bytes(),
            not_utf8.as_os_str().as_bytes(),
            "the fixture is not actually lossy"
        );
        let lookalike = PathBuf::from(&rendering);
        let store = Store::new("stats-not-utf8");
        store.session(&not_utf8, "recorded", NOW, 2, 20, Some("m"));

        let by_exact_bytes = totals_under(
            &store.root,
            Scope {
                project: Some(&not_utf8),
                since: None,
            },
        );
        let by_rendering = totals_under(
            &store.root,
            Scope {
                project: Some(&lookalike),
                since: None,
            },
        );

        assert_eq!(
            by_exact_bytes.all,
            Spent::default(),
            "a directory that is not UTF-8 was matched by its own exact bytes, which nothing \
             recorded under it can hold"
        );
        assert_eq!(
            by_rendering.all,
            spent(1, 2, 20),
            "the ordinary directory the rendering spells was not matched by its own exact name"
        );
    }

    /// A store this process cannot list is a store it never looked in. Reported as an empty one it
    /// would read as nothing recorded there, so it is counted with the files that could not be read.
    /// A file where a store's directory should be cannot be listed by anybody, which is not so of a
    /// directory with its permissions taken away when the process is root.
    #[test]
    fn a_store_that_cannot_be_listed_is_counted_and_not_read_as_empty() {
        let store = Store::new("stats-unlistable");
        let here = Path::new("/w/one");
        std::fs::write(store.root.join(sessions::key_for(here)), "not a directory")
            .expect("write the file");

        let totals = totals_under(
            &store.root,
            Scope {
                project: Some(here),
                since: None,
            },
        );

        assert_eq!(totals.all, Spent::default());
        assert_eq!(totals.unreadable, 1);
    }

    /// The reader names the fields of a record that [`sessions::Record`] writes under those names.
    /// A field renamed there would read here as none and fail nothing built from literal JSON, so
    /// this writes a session the way a front end writes one and totals what was written.
    #[test]
    fn a_session_written_the_way_a_front_end_writes_one_is_totalled_as_it_was_written() {
        if !crate::test_profile::in_isolated_profile() {
            return;
        }
        let project = crate::test_profile::project("stats-real-writer");
        std::fs::create_dir_all(&project).expect("create the project");
        let snapshot = bravebot_agent::Conversation::new().snapshot();
        let mut handle =
            sessions::Handle::begin(&project, sessions::Front::Terminal, "0.0.0-test+0000000");
        handle.save(
            "fix the build",
            sessions::Standing {
                history: None,
                conversation: &snapshot,
                turns: 3,
                tokens: 1_234,
                split: sessions::Split {
                    read: 1_000,
                    written: 234,
                    ..Default::default()
                },
                spend: &BTreeMap::from([(1, 1_234)]),
                timing: &BTreeMap::new(),
                model: Some("a-model"),
                todos: &BTreeMap::new(),
                asides: &[],
                trust: &bravebot_core::trust::TrustStore::new("/work"),
                programs: &bravebot_core::programs::TrustedPrograms::default(),
                directories: &[],
                manifest: None,
                rewind: &[],
                checkouts: &[],
                touched: None,
                answered_by_mode: false,
            },
        );

        let totals = totals(Scope {
            project: Some(&project),
            since: None,
        });

        assert_eq!(totals.all, spent_split(1, 3, 1_000, 234));
        assert_eq!(
            totals.by_model_and_service[&(Some("a-model".to_string()), None)],
            spent_split(1, 3, 1_000, 234)
        );
        assert_eq!(totals.unreadable, 0);
    }

    /// A record as a build that keeps the cache figures and the service writes it: a split with or
    /// without a cache block, and a service word or none. A block of zeros is a service that stated a
    /// miss.
    fn a_record_with(
        id: &str,
        model: &str,
        (read, written): (u64, u64),
        cache: Option<(u64, u64)>,
        service: Option<&str>,
    ) -> String {
        let mut record = serde_json::json!({
            "id": id,
            "title": "t",
            "directory": "/w/one",
            "updated": NOW,
            "turns": 1,
            "tokens": read + written,
            "model": model,
            "split": { "read": read, "written": written },
        });
        if let Some((read_tokens, written_tokens)) = cache {
            record["split"]["cache"] =
                serde_json::json!({ "read_tokens": read_tokens, "written_tokens": written_tokens });
        }
        if let Some(service) = service {
            record["service"] = serde_json::json!(service);
        }
        record.to_string()
    }

    /// The cache figures and the tokens they are a share of are added over the records that carry
    /// a cache block, and the rest are counted apart. A hit rate over every token read would be
    /// 600 of 1,500, 40.0%, counting the two sessions that carry none, where the sessions that
    /// carry figures read 600 and the cache served 540 of them, 90.0%.
    #[test]
    fn the_hit_rate_is_taken_over_the_sessions_that_carry_cache_figures_only() {
        let store = Store::new("stats-cache-hit-rate");
        let here = Path::new("/w/one");
        store.write(
            here,
            "cached",
            &a_record_with("cached", "m", (600, 100), Some((540, 30)), Some("gateway")),
        );
        store.write(
            here,
            "silent",
            &a_record_with("silent", "m", (400, 50), None, Some("gateway")),
        );
        store.write(
            here,
            "older",
            &a_record_with("older", "m", (500, 50), None, None),
        );

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(totals.all.split.cache.read_tokens, 540);
        assert_eq!(totals.all.split.cache.written_tokens, 30);
        assert_eq!(totals.all.cache_sessions, 1);
        assert_eq!(totals.all.cache_sessions_read, 600);
        assert_eq!(totals.all.hit_rate_permille(), Some(900));
        assert_eq!(totals.all.without_cache(), 2);
        assert_eq!(totals.unreadable, 0);
    }

    /// A record that names no service is told apart from one that names a service and carries no
    /// cache figures: the first may be older than the cache figures, and the second ran on a service
    /// that states none. Reading the two alike is how an older store makes a hit rate low.
    #[test]
    fn a_record_without_cache_figures_is_older_only_where_it_names_no_service() {
        let store = Store::new("stats-cache-older");
        let here = Path::new("/w/one");
        store.write(
            here,
            "missed",
            &a_record_with("missed", "m", (100, 10), None, Some("bedrock")),
        );
        store.write(
            here,
            "older",
            &a_record_with("older", "m", (100, 10), None, None),
        );
        store.write(
            here,
            "older-too",
            &a_record_with("older-too", "m", (100, 10), None, None),
        );

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(totals.all.without_cache(), 3);
        assert_eq!(totals.all.older_without_cache, 2);
        assert_eq!(totals.all.silent_without_cache(), 1);
        assert_eq!(totals.all.hit_rate_permille(), None);
    }

    /// A record keeps a cache block of zeros for a session whose service stated a miss, so that
    /// session is in the hit rate: 540 served of the 1,000 the two stating sessions read, 54.0%. A
    /// reader that took a block of zeros for no block would leave the miss out and report 90.0%,
    /// and would count the miss as a service that states nothing. The silent service and the record
    /// that names no service are each counted apart.
    #[test]
    fn a_stated_miss_is_in_the_hit_rate_and_a_silent_service_is_counted_apart() {
        let store = Store::new("stats-cache-miss");
        let here = Path::new("/w/one");
        store.write(
            here,
            "hit",
            &a_record_with("hit", "m", (600, 100), Some((540, 30)), Some("bedrock")),
        );
        store.write(
            here,
            "missed",
            &a_record_with("missed", "m", (400, 50), Some((0, 0)), Some("gateway")),
        );
        store.write(
            here,
            "silent",
            &a_record_with("silent", "m", (300, 30), None, Some("gateway")),
        );
        store.write(
            here,
            "older",
            &a_record_with("older", "m", (200, 20), None, None),
        );

        let totals = totals_under(&store.root, Scope::default());

        assert_eq!(totals.all.cache_sessions, 2);
        assert_eq!(totals.all.cache_sessions_read, 1_000);
        assert_eq!(totals.all.hit_rate_permille(), Some(540));
        assert_eq!(totals.all.without_cache(), 2);
        assert_eq!(totals.all.silent_without_cache(), 1);
        assert_eq!(totals.all.older_without_cache, 1);
    }

    /// A record edited by hand can state more served from the cache than it read. Its cache read
    /// is cut to its read before it is added, so the row it shares with a record that missed reads
    /// 100 served of 1,000, 10.0%. Added uncut, the row would read 500 of 1,000, 50.0%, which the
    /// cap on the group's total does not catch because the row read more than one record's figure.
    #[test]
    fn a_cache_read_past_the_read_is_cut_to_it_in_its_own_row() {
        let store = Store::new("stats-cache-past-read");
        let here = Path::new("/w/one");
        store.write(
            here,
            "edited",
            &a_record_with("edited", "m", (100, 10), Some((500, 0)), Some("bedrock")),
        );
        store.write(
            here,
            "missed",
            &a_record_with("missed", "m", (900, 10), Some((0, 0)), Some("bedrock")),
        );

        let totals = totals_under(&store.root, Scope::default());

        let rows = totals.by_shown_model_and_service();
        assert_eq!(rows.len(), 1, "{rows:?}");
        let row = rows[0].1;
        assert_eq!(row.split.cache.read_tokens, 100);
        assert_eq!(row.cache_sessions_read, 1_000);
        assert_eq!(row.hit_rate_permille(), Some(100));
        assert_eq!(totals.all.hit_rate_permille(), Some(100));
    }

    /// One model served by Bedrock and by a gateway is two rows, keyed on the word the program
    /// wrote. A word that is no service, as in a record edited by hand, is the row for no service
    /// with the records that name none, so no word from the file becomes a row's name.
    #[test]
    fn a_model_is_a_row_for_each_service_and_a_word_that_is_no_service_is_none() {
        let store = Store::new("stats-by-service");
        let here = Path::new("/w/one");
        store.write(
            here,
            "a",
            &a_record_with("a", "claude", (100, 10), None, Some("bedrock")),
        );
        store.write(
            here,
            "b",
            &a_record_with("b", "claude", (200, 20), None, Some("gateway")),
        );
        store.write(
            here,
            "c",
            &a_record_with("c", "claude", (300, 30), None, Some("evil\u{1b}[2J")),
        );
        store.write(
            here,
            "d",
            &a_record_with("d", "claude", (400, 40), None, None),
        );

        let rows: Vec<(Option<BackendKind>, u64)> = totals_under(&store.root, Scope::default())
            .by_shown_model_and_service()
            .into_iter()
            .map(|((model, service), spent)| {
                assert_eq!(model.as_deref(), Some("claude"));
                (service, spent.tokens)
            })
            .collect();

        assert_eq!(
            rows,
            vec![
                (None, 770),
                (Some(BackendKind::Gateway), 220),
                (Some(BackendKind::Bedrock), 110),
            ]
        );
    }

    /// Totals holding one entry for each model, none of which names a service.
    fn by(models: &[(Option<&str>, Spent)]) -> Totals {
        let mut totals = Totals::default();
        for (model, spent) in models {
            totals.all.merge(*spent);
            totals
                .by_model_and_service
                .insert((model.map(str::to_string), None), *spent);
        }
        totals
    }

    /// The model of each row, with what it spent, where no row names a service.
    fn shown(totals: &Totals) -> Vec<(Option<String>, Spent)> {
        totals
            .by_shown_model_and_service()
            .into_iter()
            .map(|((model, service), spent)| {
                assert_eq!(service, None);
                (model, spent)
            })
            .collect()
    }

    /// A model name is the server's, so a row that kept an escape in it would hand a terminal a
    /// command, and one that kept a name of any length would push the figures off the screen.
    #[test]
    fn a_shown_model_name_is_cleaned_and_cut() {
        let long = "m".repeat(LONGEST_MODEL + 20);
        let totals = by(&[
            (Some("evil\u{1b}[2Jmodel\u{7}"), spent(1, 1, 10)),
            (Some(long.as_str()), spent(1, 1, 5)),
        ]);

        let rows = shown(&totals);

        assert_eq!(rows.len(), 2);
        let first = rows[0].0.as_deref().expect("a named row");
        assert!(
            !first.contains('\u{1b}') && !first.contains('\u{7}'),
            "{first:?}"
        );
        assert!(first.contains("model"), "{first:?}");
        let second = rows[1].0.as_deref().expect("a named row");
        assert!(second.chars().count() <= LONGEST_MODEL, "{second:?}");
    }

    /// Two names that differ only in what cleaning removes read alike, and two rows that read
    /// alike would split one model's spend in two.
    #[test]
    fn names_that_read_alike_once_cleaned_are_one_row() {
        let totals = by(&[
            (Some("model"), spent(1, 2, 10)),
            (Some("mo\u{200B}del"), spent(2, 3, 20)),
        ]);

        let rows = shown(&totals);

        assert_eq!(rows, vec![(Some("model".to_string()), spent(3, 5, 30))]);
    }

    /// A name with nothing left after cleaning would head a row with a blank label beside the row
    /// for no name, so it is that row.
    #[test]
    fn an_empty_name_and_one_with_nothing_left_after_cleaning_are_the_row_for_no_name() {
        let totals = by(&[
            (None, spent(1, 1, 1)),
            (Some(""), spent(1, 2, 2)),
            (Some("\u{1b}\u{7}"), spent(1, 4, 4)),
            (Some("m"), spent(1, 1, 100)),
        ]);

        let rows = shown(&totals);

        assert_eq!(
            rows,
            vec![
                (Some("m".to_string()), spent(1, 1, 100)),
                (None, spent(3, 7, 7)),
            ]
        );
    }

    /// The model that spent most comes first; sessions settle a tie in tokens, and the name a tie in
    /// both, with the row for no name after the named ones.
    #[test]
    fn rows_are_ordered_by_tokens_then_sessions_then_name() {
        let totals = by(&[
            (Some("b"), spent(1, 1, 50)),
            (Some("a"), spent(1, 1, 50)),
            (None, spent(1, 1, 50)),
            (Some("c"), spent(2, 1, 50)),
            (Some("d"), spent(1, 1, 500)),
            (Some("e"), spent(9, 9, 5)),
        ]);

        let names: Vec<Option<String>> = shown(&totals).into_iter().map(|(name, _)| name).collect();

        let some = |name: &str| Some(name.to_string());
        assert_eq!(
            names,
            vec![some("d"), some("c"), some("a"), some("b"), None, some("e")]
        );
    }
}
