//! The reports waiting to be sent: a directory of files this crate made, kept few and kept short
//! (CRASH-6).
//!
//! The panic hook writes one file and stops. A later launch finds the files by their names, which
//! are the time they were written and the process that wrote them, and sends them. Nothing here
//! reads a file's contents: that is [`crate::send`]'s, which hands them on without parsing them.

use bravebot_agent::home;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The subdirectory of the state directory the reports wait in.
pub(crate) const DIRECTORY: &str = "crash-reports";

/// How many reports wait at most. A crash that recurs on every launch leaves this many and no more,
/// the oldest going first.
pub(crate) const KEEP: usize = 5;

/// How old a report may be and still be sent: two weeks. One older describes a build somebody has
/// most likely replaced, and sending it now would be telling the service about a failure long
/// after anyone could act on it.
pub(crate) const STALE_AFTER_MILLIS: u128 = 14 * 24 * 60 * 60 * 1000;

/// Delete `path`, dropping the failure. A report that cannot be deleted is found again by the next
/// launch, which sends it again, and the service drops the second copy by its `uuid`, so there is
/// nothing a caller could do with the error.
pub(crate) fn discard(path: &Path) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a report that stays is sent again next launch, and the service drops the copy"
    )]
    let _ = fs::remove_file(path);
}

/// The file that records the person has been told reports are sent. Not a report name, so it is
/// never sent and never counted against [`KEEP`].
const NOTICE: &str = "notice";

pub(crate) fn directory(state: &Path) -> PathBuf {
    state.join(DIRECTORY)
}

pub(crate) fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis())
}

/// `<milliseconds, sixteen digits>-<process>.json`. Fixed width, so the names sort in the order the
/// reports were written.
pub(crate) fn name(at_millis: u128, process: u32) -> String {
    format!("{at_millis:016}-{process}.json")
}

/// When the report named `name` was written, or `None` where this crate did not name it.
///
/// Strict: sixteen digits, a hyphen, digits and `.json`. Anything else in the directory is somebody
/// else's and is neither sent nor deleted.
pub(crate) fn written_at(name: &str) -> Option<u128> {
    let stem = name.strip_suffix(".json")?;
    let (at, process) = stem.split_once('-')?;
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if at.len() != 16 || !digits(at) || !digits(process) {
        return None;
    }
    at.parse().ok()
}

/// Leave one report, readable only by this user, and keep no more than [`KEEP`] of them.
///
/// Called from a panic hook, so it makes no decision that could itself fail loudly: every error is
/// returned and the hook drops it.
pub(crate) fn write(state: &Path, body: &[u8], at_millis: u128, process: u32) -> io::Result<()> {
    let dir = directory(state);
    home::create_directory(&dir)?;
    let path = dir.join(name(at_millis, process));
    home::create_new_file(&path, body)?;
    prune(&dir, KEEP, Some(&path));
    Ok(())
}

/// The reports waiting, oldest first, with when each was written.
///
/// Regular files only, chosen by name without opening any. A link with a report's name is not
/// followed, so a file that is somewhere else is never what is sent.
pub(crate) fn pending(state: &Path) -> Vec<(PathBuf, u128)> {
    let Ok(entries) = fs::read_dir(directory(state)) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, u128)> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let at = written_at(entry.file_name().to_str()?)?;
            Some((entry.path(), at))
        })
        .collect();
    found.sort_by(|one, other| one.0.cmp(&other.0));
    found
}

/// Delete the oldest reports until `keep` remain, never `current`.
///
/// `current` is the one just written. A clock set behind the older files would otherwise sort it
/// first and delete the report this call exists to keep.
fn prune(dir: &Path, keep: usize, current: Option<&Path>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| entry.file_name().to_str().and_then(written_at).is_some())
        .map(|entry| entry.path())
        .collect();
    names.sort();
    let excess = names.len().saturating_sub(keep);
    for path in names
        .into_iter()
        .filter(|path| Some(path.as_path()) != current)
        .take(excess)
    {
        discard(&path);
    }
}

/// Delete the reports older than [`STALE_AFTER_MILLIS`] at `now_millis`, unsent.
///
/// A report named for the future is left: a clock that moved is not a reason to throw one away.
pub(crate) fn forget_stale(state: &Path, now_millis: u128) {
    for (path, at) in pending(state) {
        if now_millis.saturating_sub(at) > STALE_AFTER_MILLIS {
            discard(&path);
        }
    }
}

/// Whether the person has been told that reports are sent.
pub(crate) fn notice_was_given(state: &Path) -> bool {
    directory(state).join(NOTICE).exists()
}

/// Record that the person has been told. Said once: the file stays.
pub(crate) fn note_notice_given(state: &Path) {
    let dir = directory(state);
    if home::create_directory(&dir).is_ok() {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a notice that cannot be recorded is said again next launch, which is the safe way to fail"
        )]
        let _ = home::create_new_file(&dir.join(NOTICE), b"");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A day in milliseconds, spelled out so no test takes a bound from the code it tests.
    const DAY: u128 = 24 * 60 * 60 * 1000;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("a scratch directory")
    }

    fn names_in(state: &Path) -> Vec<String> {
        pending(state)
            .into_iter()
            .map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    /// Only a name this crate made is a report, so a file the person or another program put in the
    /// directory is never sent and never deleted. Each name here breaks one rule of the shape.
    #[test]
    fn only_a_name_this_crate_made_is_a_report() {
        assert_eq!(name(1_700_000_000_123, 4242), "0001700000000123-4242.json");
        assert_eq!(
            written_at(&name(1_700_000_000_123, 4242)),
            Some(1_700_000_000_123)
        );
        for other in [
            "notice",
            "notes.json",
            "",
            "001700000000123-4242.json",
            "00001700000000123-4242.json",
            "000170000000012x-4242.json",
            "0001700000000123-42x2.json",
            "0001700000000123-.json",
            "0001700000000123.json",
            "0001700000000123-4242.txt",
            "0001700000000123-4242.json.bak",
            "../0001700000000123-4242.json",
        ] {
            assert_eq!(written_at(other), None, "{other:?} was read as a report");
        }
    }

    /// CRASH-6: a crash on every launch leaves five reports and no more, and the five are the
    /// newest.
    #[test]
    fn at_most_five_reports_are_kept_and_the_oldest_go_first() {
        let state = scratch();
        for second in 1..=8u128 {
            write(state.path(), b"{}", second * 1000, 7).unwrap();
        }
        let kept: Vec<u128> = pending(state.path()).iter().map(|(_, at)| *at).collect();
        assert_eq!(kept, [4000, 5000, 6000, 7000, 8000]);
    }

    /// A clock set behind the files already there must not make the report just written the one
    /// that is pruned.
    #[test]
    fn the_report_just_written_is_never_the_one_pruned() {
        let state = scratch();
        for second in 10..15u128 {
            write(state.path(), b"{}", second * 1000, 7).unwrap();
        }
        write(state.path(), b"{}", 1000, 7).unwrap();
        let kept: Vec<u128> = pending(state.path()).iter().map(|(_, at)| *at).collect();
        // The new report stays, and the oldest of the others goes in its place.
        assert_eq!(kept, [1000, 11_000, 12_000, 13_000, 14_000]);
    }

    /// What is waiting comes back oldest first, which is the order it should be sent in.
    #[test]
    fn waiting_reports_come_back_oldest_first() {
        let state = scratch();
        for second in [3u128, 1, 2] {
            write(state.path(), b"{}", second * 1000, 7).unwrap();
        }
        let order: Vec<u128> = pending(state.path()).iter().map(|(_, at)| *at).collect();
        assert_eq!(order, [1000, 2000, 3000]);
    }

    /// CRASH-6: a report older than two weeks is dropped and one younger is kept.
    #[test]
    fn a_report_older_than_two_weeks_is_dropped_unsent() {
        let state = scratch();
        let now = 40 * DAY;
        // Written out as days rather than read from the constant, so a changed bound moves the
        // reports the test is about and not the test with them.
        let two_weeks = 14 * DAY;
        write(state.path(), b"{}", now - two_weeks - 1, 1).unwrap();
        write(state.path(), b"{}", now - two_weeks, 2).unwrap();
        write(state.path(), b"{}", now - 1000, 3).unwrap();
        // A report from the future is a clock that moved, not a stale one.
        write(state.path(), b"{}", now + 5000, 4).unwrap();
        forget_stale(state.path(), now);
        assert_eq!(
            pending(state.path())
                .iter()
                .map(|(_, at)| *at)
                .collect::<Vec<_>>(),
            [now - two_weeks, now - 1000, now + 5000]
        );
    }

    /// Whatever else is in the directory stays exactly where it is, whichever of the three
    /// operations ran.
    #[test]
    fn a_file_this_crate_did_not_make_is_left_alone() {
        let state = scratch();
        let dir = directory(state.path());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("notes.txt"), "mine").unwrap();
        fs::write(dir.join("0000000000000001-9.json.bak"), "mine").unwrap();
        for second in 1..=9u128 {
            write(state.path(), b"{}", second * 1000, 7).unwrap();
        }
        forget_stale(state.path(), 1_000_000_000_000_000);
        assert!(dir.join("notes.txt").exists());
        assert!(dir.join("0000000000000001-9.json.bak").exists());
        assert!(names_in(state.path()).is_empty());
    }

    /// A link with a report's name is not a report: following it would send whatever it leads to.
    #[cfg(unix)]
    #[test]
    fn a_link_with_a_reports_name_is_not_one() {
        let state = scratch();
        let elsewhere = scratch();
        let target = elsewhere.path().join("private.txt");
        fs::write(&target, "not a report").unwrap();
        let dir = directory(state.path());
        fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink(&target, dir.join(name(1000, 1))).unwrap();
        write(state.path(), b"{}", 2000, 2).unwrap();
        let found = names_in(state.path());
        assert_eq!(found, [name(2000, 2)]);
    }

    /// The report is the person's: nobody else on the machine may read it.
    #[cfg(unix)]
    #[test]
    fn a_report_and_its_directory_are_readable_only_by_this_user() {
        use std::os::unix::fs::PermissionsExt;
        let state = scratch();
        write(state.path(), b"{}", 1000, 1).unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&directory(state.path())), 0o700);
        let (path, _) = pending(state.path()).remove(0);
        assert_eq!(mode(&path), 0o600);
    }

    /// Two reports from one process in one millisecond are not one, and the second does not
    /// replace the first.
    #[test]
    fn a_report_is_never_written_over_another() {
        let state = scratch();
        write(state.path(), b"first", 1000, 1).unwrap();
        assert!(write(state.path(), b"second", 1000, 1).is_err());
        let (path, _) = pending(state.path()).remove(0);
        assert_eq!(fs::read(path).unwrap(), b"first");
    }

    /// The notice is said once, and the file that says so is not a report.
    #[test]
    fn the_notice_is_recorded_once_and_is_not_a_report() {
        let state = scratch();
        assert!(!notice_was_given(state.path()));
        note_notice_given(state.path());
        assert!(notice_was_given(state.path()));
        note_notice_given(state.path());
        assert!(notice_was_given(state.path()));
        assert!(pending(state.path()).is_empty());
    }
}
