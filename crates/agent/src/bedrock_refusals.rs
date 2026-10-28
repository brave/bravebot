//! Which Bedrock models refused the ask to show thinking, kept for the sessions after the one that
//! found out.
//!
//! A model that wants to be told whether to think refuses a request whose `thinking` field says
//! only how thinking is shown (BACKEND-46). The client learns that from one refused request and
//! stops asking in the process, and every later process would spend a refused request learning it
//! again. This keeps the finding in `bedrock-refusals.json` in the state directory, so the first
//! streamed request of a later session leaves the ask out.
//!
//! `docs/specs/backends.md` (BACKEND-63) decides what is kept and for how long. This module is the
//! record: how it is spelled on disk, which entries are believed, and the two calls the backend
//! makes around a request.
//!
//! # What is kept
//!
//! A model name and the time the refusal was found. Nothing a service sent: the finding is that a
//! request carrying the ask was refused and the same request without it was answered, which the
//! client settles from statuses alone.
//!
//! # Believed for a period
//!
//! An entry is believed while it is less than [`KEPT_FOR`] from now in either direction, so a
//! model that has since started reading the ask is asked again within the period, and a stamp that
//! is far in the future, from a clock set wrong or a file edited by hand, is not believed for
//! longer than a stamp in the past would be.
//!
//! # Everything degrades to asking
//!
//! No state directory, a file that is missing, too large, not a regular file or not this shape, and
//! a write that fails each mean no entry, and a model with no entry is asked as it was before this
//! existed. Nothing here fails a request.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FILE: &str = "bedrock-refusals.json";

/// How long a refusal is believed, in either direction from the time it was found.
pub const KEPT_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The most models the file names. Past it the oldest are dropped, so the file stays small however
/// many profiles a person has pointed this at.
const MOST_MODELS: usize = 64;

/// The largest file read. A larger one is read as holding nothing, so a file that grew without
/// bound costs a lookup and no memory.
const LARGEST_FILE: u64 = 64 * 1024;

/// The file as it is spelled on disk.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Record {
    /// The models that refused the ask to show thinking, each with the seconds since the epoch at
    /// which the refusal was found.
    #[serde(default)]
    thinking_shown: BTreeMap<String, u64>,
}

/// The record in one state directory.
pub struct Store {
    file: PathBuf,
}

impl Store {
    pub fn new(home: &Path) -> Self {
        Self {
            file: home.join(FILE),
        }
    }

    /// Whether `model` was found to refuse the ask to show thinking within [`KEPT_FOR`] of `now`.
    pub fn refuses_showing_thinking(&self, model: &str, now: SystemTime) -> bool {
        self.read()
            .thinking_shown
            .get(model)
            .is_some_and(|found| believed(*found, now))
    }

    /// Keep that `model` refused the ask to show thinking, found at `now`.
    ///
    /// Writes nothing in an incognito session, which adds nothing to the state directory
    /// (INCOG-5). Entries no longer believed are dropped with this write, and so are the oldest
    /// past [`MOST_MODELS`].
    pub fn keep_refusing_showing_thinking(
        &self,
        model: &str,
        now: SystemTime,
    ) -> std::io::Result<()> {
        if bravebot_core::incognito::engaged() {
            return Ok(());
        }
        let mut record = self.read();
        record
            .thinking_shown
            .retain(|_, found| believed(*found, now));
        record
            .thinking_shown
            .insert(model.to_string(), seconds_since_the_epoch(now));
        while record.thinking_shown.len() > MOST_MODELS {
            let Some(oldest) = record
                .thinking_shown
                .iter()
                .min_by_key(|(name, found)| (**found, (*name).clone()))
                .map(|(name, _)| name.clone())
            else {
                break;
            };
            record.thinking_shown.remove(&oldest);
        }
        if let Some(directory) = self.file.parent() {
            crate::home::create_directory(directory)?;
        }
        crate::home::replace(&self.file, serde_json::to_vec(&record)?)
    }

    /// What the file holds, or nothing where it cannot be read as this record.
    ///
    /// Opened without waiting for a pipe at the path, and read no further than [`LARGEST_FILE`].
    fn read(&self) -> Record {
        let Ok(file) = crate::recall::store::open_regular(&self.file) else {
            return Record::default();
        };
        let mut text = String::new();
        match file.take(LARGEST_FILE + 1).read_to_string(&mut text) {
            Ok(read) if read as u64 <= LARGEST_FILE => {
                serde_json::from_str(&text).unwrap_or_default()
            }
            _ => Record::default(),
        }
    }
}

fn seconds_since_the_epoch(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

/// Whether a refusal found at `found` seconds since the epoch is believed at `now`.
fn believed(found: u64, now: SystemTime) -> bool {
    seconds_since_the_epoch(now).abs_diff(found) < KEPT_FOR.as_secs()
}

/// What this process has done with the record, per model.
#[derive(Default)]
struct Tracked {
    /// Models whose entry has been looked up, so the file is read once for each.
    looked_up: HashSet<String>,
    /// Models the file is known to name, because it was read or written here.
    on_disk: HashSet<String>,
}

fn tracked() -> &'static Mutex<Tracked> {
    static TRACKED: OnceLock<Mutex<Tracked>> = OnceLock::new();
    TRACKED.get_or_init(Mutex::default)
}

/// Before a Bedrock request for `model`: tell the client what an earlier session found, once for
/// each model in this process.
pub fn put_back(model: &str) {
    let Ok(mut tracked) = tracked().lock() else {
        return;
    };
    if !tracked.looked_up.insert(model.to_string()) {
        return;
    }
    let Some(home) = crate::home::directory() else {
        return;
    };
    if Store::new(&home).refuses_showing_thinking(model, SystemTime::now()) {
        bravebot_bedrock::known_to_refuse_showing_thinking(model);
        tracked.on_disk.insert(model.to_string());
    }
}

/// After a Bedrock request for `model`: keep what this process has found the model to refuse, once.
///
/// Asked after every request, not only a refused one, since the client learns a refusal when the
/// request without the ask has answered.
pub fn keep_what_was_found(model: &str) {
    if !bravebot_bedrock::refusals(model).thinking_shown {
        return;
    }
    let Ok(mut tracked) = tracked().lock() else {
        return;
    };
    if tracked.on_disk.contains(model) {
        return;
    }
    let Some(home) = crate::home::writable() else {
        return;
    };
    if Store::new(&home)
        .keep_refusing_showing_thinking(model, SystemTime::now())
        .is_ok()
    {
        tracked.on_disk.insert(model.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "arn:aws:bedrock:us-west-2:111122223333:application-inference-profile/abc";

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    /// A scratch state directory that is emptied when the test starts.
    fn scratch(name: &str) -> PathBuf {
        let directory = crate::testutil::scratch_dir(&format!(
            "bedrock-refusals-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    /// BACKEND-63: a finding kept by one session is read by a later one, for the model it was found
    /// about and for no other. What one model refuses says nothing about another.
    ///
    /// The fault this rejects: a record read for every model once any is kept, which stops asking
    /// a model that reads the ask, or a record not read back at all, which spends a refused
    /// request in every session.
    #[test]
    fn a_refusal_kept_by_one_session_is_read_by_the_next_for_that_model_only() {
        let home = scratch("read-back");
        assert!(
            !Store::new(&home).refuses_showing_thinking(MODEL, at(1_000)),
            "nothing was kept yet"
        );

        Store::new(&home)
            .keep_refusing_showing_thinking(MODEL, at(1_000))
            .expect("kept");

        let later = Store::new(&home);
        assert!(later.refuses_showing_thinking(MODEL, at(2_000)));
        assert!(!later.refuses_showing_thinking("anthropic.claude-opus-5-5", at(2_000)));
        assert!(!later.refuses_showing_thinking("", at(2_000)));
    }

    /// BACKEND-63: a refusal is believed for thirty days after it was found, so a model that starts
    /// reading the ask is asked again within a month, and not a second longer.
    ///
    /// The faults this rejects: no expiry, which asks no more for as long as the file exists, and
    /// a bound off by one, which keeps a refusal a period old.
    #[test]
    fn a_refusal_is_believed_for_thirty_days_and_not_after() {
        let home = scratch("period");
        let found = 5_000_000;
        Store::new(&home)
            .keep_refusing_showing_thinking(MODEL, at(found))
            .expect("kept");
        let period = KEPT_FOR.as_secs();
        assert_eq!(period, 30 * 24 * 60 * 60);

        let store = Store::new(&home);
        assert!(store.refuses_showing_thinking(MODEL, at(found + period - 1)));
        assert!(!store.refuses_showing_thinking(MODEL, at(found + period)));
        assert!(!store.refuses_showing_thinking(MODEL, at(found + 10 * period)));
    }

    /// BACKEND-63: a stamp in the future is believed for the period and no longer, so a clock set
    /// back for a day does not lose the finding and a stamp far ahead, from a wrong clock or a file
    /// edited by hand, does not pin a model's refusal for good.
    ///
    /// The fault this rejects: treating a future stamp as never old, which makes a stamp of the
    /// largest number a refusal that is never asked about again.
    #[test]
    fn a_stamp_in_the_future_is_believed_for_the_period_and_no_longer() {
        let home = scratch("future");
        std::fs::create_dir_all(&home).expect("directory");
        let now = 5_000_000;
        let period = KEPT_FOR.as_secs();
        let file = home.join(FILE);

        for (stamp, honoured) in [
            (now + 86_400, true),
            (now + period - 1, true),
            (now + period, false),
            (u64::MAX, false),
        ] {
            std::fs::write(
                &file,
                format!(r#"{{"thinking_shown":{{"{MODEL}":{stamp}}}}}"#),
            )
            .expect("written");
            assert_eq!(
                Store::new(&home).refuses_showing_thinking(MODEL, at(now)),
                honoured,
                "stamp {stamp}"
            );
        }
    }

    /// BACKEND-63: a file that cannot be read as this record holds nothing, and the next finding
    /// replaces it. A request is never failed by the file.
    ///
    /// The fault this rejects: a parse error that ends the request, or a read that waits on a pipe
    /// or reads a file of any size.
    #[test]
    fn a_file_that_is_not_this_record_holds_nothing_and_is_replaced_by_the_next_finding() {
        let home = scratch("unreadable");
        std::fs::create_dir_all(&home).expect("directory");
        let file = home.join(FILE);
        let too_large = format!(
            r#"{{"thinking_shown":{{"{MODEL}":1000}},"padding":"{}"}}"#,
            "x".repeat(LARGEST_FILE as usize)
        );

        for (what, text) in [
            ("not json", "not json at all".to_string()),
            ("a list", "[]".to_string()),
            (
                "the stamp as text",
                format!(r#"{{"thinking_shown":{{"{MODEL}":"yesterday"}}}}"#),
            ),
            (
                "a negative stamp",
                format!(r#"{{"thinking_shown":{{"{MODEL}":-1}}}}"#),
            ),
            ("past the largest file read", too_large),
            ("empty", String::new()),
        ] {
            std::fs::write(&file, text).expect("written");
            let store = Store::new(&home);
            assert!(
                !store.refuses_showing_thinking(MODEL, at(1_500)),
                "{what} was believed"
            );
            store
                .keep_refusing_showing_thinking(MODEL, at(1_500))
                .unwrap_or_else(|error| panic!("{what} could not be replaced: {error}"));
            assert!(
                Store::new(&home).refuses_showing_thinking(MODEL, at(1_500)),
                "{what} was not replaced by the finding"
            );
        }

        std::fs::remove_file(&file).expect("removed");
        std::fs::create_dir(&file).expect("a directory where the file goes");
        assert!(
            !Store::new(&home).refuses_showing_thinking(MODEL, at(1_500)),
            "a directory was read as the record"
        );
    }

    /// BACKEND-63: the file names at most sixty-four models, dropping the oldest, and a write drops
    /// the entries no longer believed. A profile per account per region could otherwise grow it
    /// without bound.
    ///
    /// The fault this rejects: dropping the newest instead of the oldest, which forgets what was
    /// just found, and keeping expired entries, which never leaves the file.
    #[test]
    fn the_file_keeps_the_newest_sixty_four_models_and_none_that_expired() {
        let home = scratch("bounded");
        let store = Store::new(&home);
        let start = 10_000_000;
        store
            .keep_refusing_showing_thinking("long-expired", at(start - 2 * KEPT_FOR.as_secs()))
            .expect("kept");
        // Stamped two periods before the writes below, so the first of them finds it expired.
        for n in 0..70u64 {
            store
                .keep_refusing_showing_thinking(&format!("model-{n:02}"), at(start + n))
                .expect("kept");
        }

        let read = |model: &str| store.refuses_showing_thinking(model, at(start + 70));
        assert!(!read("long-expired"), "an entry that expired stayed");
        for dropped in 0..6 {
            assert!(!read(&format!("model-{dropped:02}")), "{dropped} stayed");
        }
        for kept in 6..70 {
            assert!(read(&format!("model-{kept:02}")), "{kept} was dropped");
        }
    }
}
