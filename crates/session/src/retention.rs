//! Removing what the state directory keeps past the period a person set (STATE-4).
//!
//! Three things accumulate for as long as a machine is used and have no other bound: a session
//! record with the trail beside it, a language server's index of a workspace, and a file of what a
//! scan found. Each is removed once nothing has written it for the retention period
//! ([`bravebot_config::Retention`]), by the crate that owns it: the records here, the indexes in
//! `bravebot-lsp` and the findings in `bravebot-agent`.
//!
//! What a person asked to keep past a session is not touched, whatever its age: the rules they
//! granted, the answers to the startup question, the memories they left untrusted, the prompt
//! history and their settings. Those are decisions rather than records of work.
//!
//! The age of a file is its modification time. No byte a record, an index or a finding holds is
//! read to decide whether it goes, so nothing untrusted decides anything.

use bravebot_config::Retention;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

/// What a sweep removed, by kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Removed {
    /// Session records, each with the trail and the search index beside it.
    pub sessions: usize,
    /// Language server indexes.
    pub indexes: usize,
    /// Files of findings.
    pub findings: usize,
}

const SECONDS_PER_DAY: u64 = 86_400;

/// What the process starting a session has open, which the sweep leaves whatever its age.
///
/// Both default to nothing, which is what a front end that holds no session when it starts passes.
#[derive(Debug, Clone, Copy, Default)]
pub struct InUse<'a> {
    /// The session being opened or resumed. Its record is kept: a rewind reads the record of a live
    /// session, and a person who resumes one older than the period is resuming it.
    pub session: Option<&'a str>,
    /// The workspace the session works in. Its language server index is kept: a server is about to
    /// start on it.
    pub workspace: Option<&'a Path>,
}

/// Remove from the state directory `state` what has not been written for `period` days before `now`.
///
/// The least `period` can be is one day, which is what protects whatever was written recently.
/// Nothing here fails: an entry that cannot be read or removed is left where it is.
pub fn sweep(state: &Path, period: NonZeroU32, now: SystemTime, in_use: InUse<'_>) -> Removed {
    let Some(cutoff) = now.checked_sub(Duration::from_secs(
        u64::from(period.get()) * SECONDS_PER_DAY,
    )) else {
        return Removed::default();
    };
    let sessions = crate::sessions::state_sessions(state);
    let in_background = crate::jobs::Roster::at(state.join("jobs")).live_ids();
    let kept = |id: &str| in_use.session == Some(id) || in_background.iter().any(|live| live == id);
    let workspace_directory = in_use
        .workspace
        .map(|workspace| sessions.join(crate::sessions::key_for(workspace)));
    Removed {
        sessions: crate::sessions::remove_unwritten_since(
            &sessions,
            cutoff,
            &kept,
            workspace_directory.as_deref(),
        ),
        indexes: bravebot_agent::lsp::remove_unused_indexes(state, cutoff, in_use.workspace),
        findings: bravebot_agent::findings::remove_unwritten_since(state, cutoff),
    }
}

/// Start a sweep of `state` on a thread of its own, or `None` where there is nothing to do or no
/// thread could be made.
///
/// Nothing for [`Retention::Paused`], which is the setting saying the period cannot be determined,
/// and nothing without a state directory. A thread, because a long list of long sessions is slow to
/// walk and a session is not made to wait for it to begin.
pub fn start(
    retention: Retention,
    state: Option<std::path::PathBuf>,
    in_use: InUse<'_>,
) -> Option<std::thread::JoinHandle<Removed>> {
    let Retention::Days(period) = retention else {
        return None;
    };
    let state = state?;
    let session = in_use.session.map(str::to_owned);
    let workspace = in_use.workspace.map(Path::to_path_buf);
    std::thread::Builder::new()
        .name("retention sweep".into())
        .spawn(move || {
            sweep(
                &state,
                period,
                SystemTime::now(),
                InUse {
                    session: session.as_deref(),
                    workspace: workspace.as_deref(),
                },
            )
        })
        .ok()
}

/// Whether this process has started its sweep, so one process sweeps once however many sessions it
/// opens.
static STARTED: AtomicBool = AtomicBool::new(false);

/// The sweep a front end runs as a session opens (STATE-4), at most once per process.
///
/// `in_use` is the session being opened or resumed and the workspace it is in. A front end that
/// holds no session when it starts, as the desktop bridge does, passes none.
///
/// Nothing in an incognito session, which adds nothing to the state directory and so removes
/// nothing from it either, and nothing on a machine with no state directory. A thread that could
/// not be made leaves the next session to try again.
pub fn at_startup(retention: Retention, in_use: InUse<'_>) {
    if matches!(retention, Retention::Paused) {
        return;
    }
    let Some(state) = bravebot_agent::home::writable() else {
        return;
    };
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    if start(retention, Some(state), in_use).is_none() {
        STARTED.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A state directory of this test's own, removed with it.
    struct State {
        path: PathBuf,
    }

    impl State {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch state directory");
            Self { path }
        }

        /// A file under the state directory, last written `seconds` ago.
        fn put(&self, relative: &str, body: &str, seconds: u64) -> PathBuf {
            let path = self.path.join(relative);
            std::fs::create_dir_all(path.parent().expect("a directory")).expect("directories");
            std::fs::write(&path, body).expect("write");
            written_ago(&path, seconds);
            path
        }

        /// A session record with a trail, a search index and a temporary file beside it, all last
        /// written `seconds` ago. Returns the record's path.
        fn record(&self, project: &str, id: &str, seconds: u64) -> PathBuf {
            let directory = format!("sessions/{project}");
            for extension in ["audit.jsonl", "search", "tmp"] {
                self.put(&format!("{directory}/{id}.{extension}"), "words", seconds);
            }
            self.put(&format!("{directory}/{id}.json"), "{}", seconds)
        }
    }

    impl Drop for State {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    const DAY: u64 = SECONDS_PER_DAY;

    fn written_ago(path: &Path, seconds: u64) {
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open the file")
            .set_modified(SystemTime::now() - Duration::from_secs(seconds))
            .expect("age the file");
    }

    fn days(count: u32) -> NonZeroU32 {
        NonZeroU32::new(count).expect("a period above zero")
    }

    fn ids(id: &str) -> [String; 4] {
        ["json", "audit.jsonl", "search", "tmp"].map(|extension| format!("{id}.{extension}"))
    }

    fn files_of(state: &State, project: &str, id: &str) -> Vec<bool> {
        ids(id)
            .iter()
            .map(|name| {
                state
                    .path
                    .join("sessions")
                    .join(project)
                    .join(name)
                    .exists()
            })
            .collect()
    }

    /// STATE-4: a record not written for longer than the period goes with the trail, the search
    /// index and the temporary file beside it, in every project, and one written inside the period
    /// stays whole. The newer record is what tells this from a sweep that empties the directory.
    #[test]
    fn a_record_older_than_the_period_is_removed_with_what_is_beside_it_and_a_newer_one_is_kept() {
        let state = State::new("retention-age");
        state.record("one", "aaaa-old", 40 * DAY);
        state.record("one", "bbbb-new", 3 * DAY);
        state.record("two", "cccc-old", 400 * DAY);

        let removed = sweep(&state.path, days(30), SystemTime::now(), InUse::default());

        assert_eq!(removed.sessions, 2);
        assert_eq!(files_of(&state, "one", "aaaa-old"), [false; 4]);
        assert_eq!(files_of(&state, "two", "cccc-old"), [false; 4]);
        assert_eq!(files_of(&state, "one", "bbbb-new"), [true; 4]);
    }

    /// STATE-4, SESSION-19: the record of the session this process is in is kept at any age, because
    /// a rewind reads it while the session is live. The same record goes once it is not the session
    /// running, so the exemption is what kept it.
    #[test]
    fn the_running_sessions_record_is_kept_at_any_age() {
        let state = State::new("retention-running");
        state.record("one", "running-session", 400 * DAY);
        state.record("one", "another-session", 400 * DAY);

        let removed = sweep(
            &state.path,
            days(30),
            SystemTime::now(),
            InUse {
                session: Some("running-session"),
                workspace: None,
            },
        );

        assert_eq!(removed.sessions, 1);
        assert_eq!(files_of(&state, "one", "running-session"), [true; 4]);
        assert_eq!(files_of(&state, "one", "another-session"), [false; 4]);

        sweep(&state.path, days(30), SystemTime::now(), InUse::default());
        assert_eq!(files_of(&state, "one", "running-session"), [false; 4]);
    }

    /// STATE-4: a record's age is the time its file was written and nothing it says. A record whose
    /// own fields claim to be from the future is removed when its file is old, and one claiming to be
    /// from long ago is kept when its file is new, so no byte of a conversation decides.
    #[test]
    fn a_records_age_is_when_it_was_written_and_not_what_it_says() {
        let state = State::new("retention-claims");
        let says_new = state.put(
            "sessions/one/says-new.json",
            r#"{"id": "says-new", "updated": 99999999999, "started": 99999999999}"#,
            100 * DAY,
        );
        let says_old = state.put(
            "sessions/one/says-old.json",
            r#"{"id": "says-old", "updated": 1, "started": 1}"#,
            60,
        );

        sweep(&state.path, days(30), SystemTime::now(), InUse::default());

        assert!(
            !says_new.exists(),
            "a record's own claim of being recent kept it"
        );
        assert!(
            says_old.exists(),
            "a record's own claim of being old removed it"
        );
    }

    /// STATE-4: the least period is one day, so what was written within the day is kept at the
    /// shortest period there is, and what was not is removed.
    #[test]
    fn the_shortest_period_still_keeps_what_was_written_today() {
        let state = State::new("retention-floor");
        state.record("one", "today", DAY / 2);
        state.record("one", "earlier", 2 * DAY);

        let removed = sweep(&state.path, days(1), SystemTime::now(), InUse::default());

        assert_eq!(removed.sessions, 1);
        assert_eq!(files_of(&state, "one", "today"), [true; 4]);
        assert_eq!(files_of(&state, "one", "earlier"), [false; 4]);
    }

    /// STATE-4: a file stamped later than now, by a clock that was wrong when it was written, is not
    /// older than anything.
    #[test]
    fn a_file_stamped_in_the_future_is_kept() {
        let state = State::new("retention-future");
        let record = state.record("one", "ahead", 0);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&record)
            .expect("open")
            .set_modified(SystemTime::now() + Duration::from_secs(400 * DAY))
            .expect("stamp in the future");

        let removed = sweep(
            &state.path,
            days(1),
            SystemTime::now() + Duration::from_secs(10 * DAY),
            InUse::default(),
        );

        assert_eq!(removed.sessions, 0);
        assert!(record.exists());
    }

    /// STATE-4: a trail or a temporary file with no record is what an interrupted first save leaves,
    /// and it holds the conversation as the record would, so it goes by its own age. One beside a
    /// record that is itself recent stays with it.
    #[test]
    fn a_trail_or_temporary_file_with_no_record_goes_by_its_own_age() {
        let state = State::new("retention-orphans");
        let old_trail = state.put("sessions/one/lost.audit.jsonl", "words", 40 * DAY);
        let old_temporary = state.put("sessions/one/lost.tmp", "words", 40 * DAY);
        let new_trail = state.put("sessions/one/begun.audit.jsonl", "words", 60);
        let paired = state.put("sessions/one/paired.audit.jsonl", "words", 40 * DAY);
        state.put("sessions/one/paired.json", "{}", 60);

        let removed = sweep(&state.path, days(30), SystemTime::now(), InUse::default());

        assert_eq!(removed.sessions, 0, "an orphan is not a record");
        assert!(!old_trail.exists());
        assert!(!old_temporary.exists());
        assert!(
            new_trail.exists(),
            "a trail written a minute ago was removed"
        );
        assert!(
            paired.exists(),
            "a trail was removed from beside a recent record"
        );
    }

    /// STATE-4, PATH-003: a file whose name is not text is not a session's file, so it is left where
    /// it is at any age, and the recent file named by the lossy rendering of that name is not removed
    /// in its place. A sweep that chose by the rendering, and took a replacement character as part of
    /// a session's name, would join the rendering back onto the directory and remove the lookalike.
    /// Only a Linux file system holds the name, so this runs there.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_name_that_is_not_text_is_left_and_the_file_its_lossy_rendering_names_is_not_removed() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let state = State::new("retention-not-text");
        let control = state.put("sessions/one/plain-old.json", "{}", 40 * DAY);
        let directory = state.path.join("sessions").join("one");
        let record = directory.join(OsStr::from_bytes(b"old-\xff.json"));
        let trail = directory.join(OsStr::from_bytes(b"old-\xff.audit.jsonl"));
        let lookalike = directory.join("old-\u{FFFD}.json");
        for (path, seconds) in [(&record, 40 * DAY), (&trail, 40 * DAY), (&lookalike, 60)] {
            std::fs::write(path, "{}").expect("write");
            written_ago(path, seconds);
        }

        let removed = sweep(&state.path, days(30), SystemTime::now(), InUse::default());

        assert_eq!(removed.sessions, 1, "only the record with a text name goes");
        assert!(!control.exists());
        assert!(
            record.exists(),
            "a record whose name is not text was removed"
        );
        assert!(trail.exists(), "a trail whose name is not text was removed");
        assert!(
            lookalike.exists(),
            "a recent file was removed through the lossy rendering of another's name"
        );
    }

    /// STATE-1: removal stops at the state directory and steps over links. A link standing as the
    /// directory of projects, as a project's directory or as a record leads to something this was not
    /// given, and what is there is not removed.
    #[cfg(unix)]
    #[test]
    fn the_sweep_does_not_follow_a_link_out_of_the_state_directory() {
        let state = State::new("retention-link");
        let outside = state.path.join("outside");
        let theirs = state.put("outside/project/old.json", "somebody's own", 400 * DAY);
        let a_record = state.put("outside/a-record.json", "somebody's own", 400 * DAY);

        let linked_state = state.path.join("linked-state");
        std::fs::create_dir_all(linked_state.join("sessions")).expect("sessions");
        std::os::unix::fs::symlink(
            outside.join("project"),
            linked_state.join("sessions").join("linked-project"),
        )
        .expect("link a project");
        std::os::unix::fs::symlink(
            &a_record,
            linked_state
                .join("sessions")
                .join("linked-project-two.json"),
        )
        .expect("link a record");
        std::fs::create_dir_all(linked_state.join("sessions").join("real")).expect("a project");
        std::os::unix::fs::symlink(
            &a_record,
            linked_state
                .join("sessions")
                .join("real")
                .join("linked.json"),
        )
        .expect("link a record inside a project");

        sweep(&linked_state, days(30), SystemTime::now(), InUse::default());
        assert!(
            theirs.exists(),
            "a project outside was swept through a link"
        );
        assert!(
            a_record.exists(),
            "a file outside was removed through a link"
        );

        let whole = state.path.join("whole-state");
        std::fs::create_dir_all(&whole).expect("state");
        std::os::unix::fs::symlink(&outside, whole.join("sessions")).expect("link the sessions");
        sweep(&whole, days(30), SystemTime::now(), InUse::default());
        assert!(theirs.exists(), "the directory of projects was followed");
    }

    /// STATE-4: three kinds of file are swept and nothing else. What a person asked to keep past a
    /// session, and what they wrote there themselves, is left however old it is.
    #[test]
    fn what_a_person_asked_to_keep_is_left_whatever_its_age() {
        let state = State::new("retention-kept");
        state.record("one", "old-session", 400 * DAY);
        let record_of_findings = state.put("findings/work.jsonl", "{}", 400 * DAY);
        let index = state.path.join("lsp").join("00000000000000aa");
        std::fs::create_dir_all(&index).expect("an index");
        std::fs::write(index.join("derived"), "derived").expect("write");
        let kept: Vec<PathBuf> = [
            "granted/work.jsonl",
            "trusted/work.jsonl",
            "untrusted/work.jsonl",
            "findings/accepted/work.jsonl",
            "history",
            "model",
            "settings.json",
            "AGENTS.md",
            "skills/mine/SKILL.md",
            "jobs/one/state.json",
            "logs/old.log",
        ]
        .iter()
        .map(|relative| state.put(relative, "kept", 400 * DAY))
        .collect();

        let removed = sweep(&state.path, days(30), SystemTime::now(), InUse::default());

        assert_eq!(removed.sessions, 1);
        assert_eq!(removed.findings, 1);
        assert!(!record_of_findings.exists());
        assert_eq!(
            removed.indexes, 0,
            "an index with no recorded use went at first sight"
        );
        let a_period_later = SystemTime::now() + Duration::from_secs(31 * DAY);
        assert_eq!(
            sweep(&state.path, days(30), a_period_later, InUse::default()).indexes,
            1
        );
        for path in kept {
            assert!(path.exists(), "{} was removed", path.display());
        }
    }

    /// STATE-4: a setting that is paused starts nothing, and neither does a machine with no state
    /// directory. A sweep that read a paused setting as the built-in period would remove records on
    /// a figure the person never chose.
    #[test]
    fn a_paused_sweep_removes_nothing() {
        let state = State::new("retention-paused");
        let record = state.record("one", "old-session", 400 * DAY);

        let nothing = InUse::default();
        assert!(start(Retention::Paused, Some(state.path.clone()), nothing).is_none());
        assert!(start(Retention::Days(days(30)), None, nothing).is_none());
        assert!(record.exists());
    }

    /// STATE-4: a started sweep removes what is old and keeps the session it was told is running.
    #[test]
    fn a_started_sweep_removes_what_is_old_and_keeps_the_running_session() {
        let state = State::new("retention-started");
        state.record("one", "old-session", 400 * DAY);
        state.record("one", "running-session", 400 * DAY);

        let removed = start(
            Retention::Days(days(30)),
            Some(state.path.clone()),
            InUse {
                session: Some("running-session"),
                workspace: None,
            },
        )
        .expect("a sweep started")
        .join()
        .expect("the sweep finished");

        assert_eq!(removed.sessions, 1);
        assert_eq!(files_of(&state, "one", "old-session"), [false; 4]);
        assert_eq!(files_of(&state, "one", "running-session"), [true; 4]);
    }
    /// STATE-4: the index of the workspace the session starting is in is kept at any age, and the
    /// workspace reaches the sweep from the caller: another workspace's index of the same age goes.
    #[test]
    fn the_index_of_the_workspace_being_opened_is_kept_at_any_age() {
        let state = State::new("retention-workspace");
        let workspace = state.path.join("workspace");
        std::fs::create_dir_all(&workspace).expect("a workspace");
        let index = bravebot_agent::lsp::cache_for(Some(&state.path), &workspace, false)
            .expect("an index for the workspace");
        let name = index
            .file_name()
            .expect("named")
            .to_string_lossy()
            .into_owned();
        let a_period_later = SystemTime::now() + Duration::from_secs(31 * DAY);
        for directory in [name.as_str(), "00000000000000ee"] {
            std::fs::create_dir_all(state.path.join("lsp").join(directory)).expect("an index");
            std::fs::write(
                state.path.join("lsp").join(directory).join("derived"),
                "derived",
            )
            .expect("write");
        }
        // Stamped, so the second sweep ages them rather than meeting them for the first time.
        sweep(&state.path, days(30), SystemTime::now(), InUse::default());

        let in_use = InUse {
            session: None,
            workspace: Some(&workspace),
        };
        let removed = sweep(&state.path, days(30), a_period_later, in_use);

        assert_eq!(removed.indexes, 1);
        assert!(state.path.join("lsp").join(&name).join("derived").is_file());
        assert!(!state.path.join("lsp").join("00000000000000ee").exists());
    }

    /// STATE-4: a project's directory goes with its last record, so a project nobody has worked in for
    /// the period leaves no name behind, and a directory that still holds anything is left.
    #[test]
    fn a_project_directory_the_sweep_emptied_is_removed_and_one_holding_anything_is_not() {
        let state = State::new("retention-directories");
        state.record("emptied", "old-session", 400 * DAY);
        state.record("recent", "new-session", 3 * DAY);
        state.record("mixed", "old-session", 400 * DAY);
        let stray = state.put("sessions/mixed/notes.txt", "somebody's own", 400 * DAY);
        let never_held_one = state.path.join("sessions").join("never-held-one");
        std::fs::create_dir_all(&never_held_one).expect("an empty directory");

        sweep(&state.path, days(30), SystemTime::now(), InUse::default());

        assert!(!state.path.join("sessions/emptied").exists());
        assert!(state.path.join("sessions/recent").is_dir());
        assert!(stray.is_file(), "a file that is not a record was removed");
        assert!(
            never_held_one.is_dir(),
            "a directory the sweep did not empty"
        );
    }
    /// STATE-4: a session a background process is running is kept at any age, as `sessions purge` leaves
    /// one, and the same record goes once nothing is running it. A held lock on the roster entry is how
    /// a process says it is alive.
    #[test]
    fn a_session_a_background_process_is_running_is_kept_at_any_age() {
        let state = State::new("retention-background");
        state.record("one", "0b0b0b0b-1111", 400 * DAY);
        state.record("one", "another-session", 400 * DAY);
        let roster = crate::jobs::Roster::at(state.path.join("jobs"));
        let lease = roster.claim("0b0b0b0b-1111").expect("claim").expect("free");

        let removed = sweep(&state.path, days(30), SystemTime::now(), InUse::default());

        assert_eq!(removed.sessions, 1);
        assert_eq!(files_of(&state, "one", "0b0b0b0b-1111"), [true; 4]);
        assert_eq!(files_of(&state, "one", "another-session"), [false; 4]);

        // A lock is released when the last copy of its file closes, and a process another test is
        // starting holds a copy until it execs, so the release is waited for rather than assumed.
        drop(lease);
        let until = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            sweep(&state.path, days(30), SystemTime::now(), InUse::default());
            let gone = files_of(&state, "one", "0b0b0b0b-1111") == [false; 4];
            if gone || std::time::Instant::now() >= until {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(files_of(&state, "one", "0b0b0b0b-1111"), [false; 4]);
    }

    /// STATE-4: the directory of the project the session starting is in is not removed, because that
    /// session may be about to write into it, and the records in it still go by age.
    #[test]
    fn the_directory_of_the_workspace_being_opened_is_not_removed() {
        let state = State::new("retention-workspace-directory");
        let workspace = state.path.join("workspace");
        std::fs::create_dir_all(&workspace).expect("a workspace");
        let key = crate::sessions::key_for(&workspace);
        state.record(&key, "old-session", 400 * DAY);
        state.record("elsewhere", "old-session", 400 * DAY);

        let in_use = InUse {
            session: None,
            workspace: Some(&workspace),
        };
        let removed = sweep(&state.path, days(30), SystemTime::now(), in_use);

        assert_eq!(removed.sessions, 2);
        assert!(state.path.join("sessions").join(&key).is_dir());
        assert!(!state.path.join("sessions/elsewhere").exists());
        assert_eq!(files_of(&state, &key, "old-session"), [false; 4]);
    }
}
