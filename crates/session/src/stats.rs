//! What the recorded sessions spent, totalled across the store (CLI-37).
//!
//! Reads the records under the sessions directory and adds up three numbers from each: how many
//! turns it had, how many tokens it spent, and which model answered last. Nothing is written, and
//! nothing in a record is read for what it says: the figures are the ones the program itself wrote
//! down, and the model name is the one the server reported.
//!
//! A record holds one total for the session, and the model that answered its last turn. A total
//! cannot be split by day inside a session or by model across turns, so a session is counted
//! whole, under the day it was last written and the model it last used.

use crate::sessions;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
}

impl Spent {
    /// What one record contributes.
    fn of(counted: &Counted) -> Self {
        Self {
            sessions: 1,
            turns: counted.turns,
            tokens: counted.tokens,
        }
    }

    /// Add another total to this one.
    ///
    /// Saturating, because a record somebody edited by hand can hold any number, and a total that
    /// wrapped or panicked on one would take the figures of every other record with it.
    pub fn merge(&mut self, other: Spent) {
        self.sessions = self.sessions.saturating_add(other.sessions);
        self.turns = self.turns.saturating_add(other.turns);
        self.tokens = self.tokens.saturating_add(other.tokens);
    }
}

/// The totals over a scope.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Every session in the scope.
    pub all: Spent,
    /// The same, split by the model each session last used, as the record named it. `None` is a
    /// session whose record names none: one written before the model was kept, one whose turns
    /// never reached a server, and one copied in from another program.
    pub by_model: BTreeMap<Option<String>, Spent>,
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
    /// Absent from a record written before the model was kept.
    #[serde(default)]
    model: Option<String>,
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
    fn take(&mut self, counted: Counted) {
        let spent = Spent::of(&counted);
        self.all.merge(spent);
        self.by_model.entry(counted.model).or_default().merge(spent);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::scratch_dir;
    use std::path::PathBuf;

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
            let record = serde_json::json!({
                "id": id,
                "title": "t",
                "directory": project.display().to_string(),
                "updated": updated,
                "turns": turns,
                "tokens": tokens,
                "model": model,
            });
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

    fn spent(sessions: u64, turns: u64, tokens: u64) -> Spent {
        Spent {
            sessions,
            turns,
            tokens,
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

        assert_eq!(totals.by_model[&Some("big".to_string())], spent(2, 3, 30));
        assert_eq!(
            totals.by_model[&Some("small".to_string())],
            spent(1, 4, 400)
        );
        assert_eq!(totals.by_model[&None], spent(1, 8, 8_000));
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
        assert_eq!(totals.by_model[&None], spent(1, 0, 0));
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
            },
        );

        let totals = totals(Scope {
            project: Some(&project),
            since: None,
        });

        assert_eq!(totals.all, spent(1, 3, 1_234));
        assert_eq!(
            totals.by_model[&Some("a-model".to_string())],
            spent(1, 3, 1_234)
        );
        assert_eq!(totals.unreadable, 0);
    }
}
