//! The record of directories a confined stage may write, kept in the state directory so that a
//! later process knows them ([SANDBOX-30]).
//!
//! `bravebot_sandbox::programs` leaves every directory a stage may write out of the search for the
//! programs bravebot starts itself. It holds them for one process, so a file a stage wrote in an
//! earlier process into a directory this one has no row for (an `--add-dir` directory, a remembered
//! grant, a virtualenv) would be found as the system's. This module reads the file that carries
//! them from one process to the next and appends to it.
//!
//! The file is in the state directory, which a stage cannot write, and is read from nowhere else:
//! never from a checkout, and never from a path a stage may write. A line names a directory and no
//! program, so what a record can do is leave a directory out of a search. It cannot make a file
//! found. A line is never removed, because closing a directory does not remove a file already
//! written there. A person clears the record by deleting the file.
//!
//! [SANDBOX-30]: ../../../docs/specs/sandboxing.md

use std::io::{Read, Write};
use std::path::Path;

use bravebot_sandbox::programs::{RECORD_MOST_BYTES, RECORD_MOST_LINES};

/// The name of the record inside the state directory.
pub const FILE: &str = "stage-writes";

/// Take the directories earlier processes recorded from the state directory, so the lookups of
/// this process leave them out. Reads the file in an incognito session as in any other. Nothing is
/// taken where the state directory is not named or holds no record.
pub fn load() {
    if let Some(directory) = crate::home::directory() {
        bravebot_sandbox::programs::load_record(&read_in(&directory));
    }
}

/// From now on append each directory this process records to the record. A session that is
/// incognito appends nothing, since the state directory is not one it may write
/// ([`crate::home::writable`]).
pub fn follow() {
    bravebot_sandbox::programs::follow_record(append);
}

/// Append `line` to the record in the state directory, where this session may write one. A line
/// that cannot be written is reported to the caller, which then hands it on again the next time
/// the directory is recorded. The directory is known to this process alone until one is written.
///
/// A directory that is the account's home directory or holds it is not appended. The search for
/// programs would leave out `~/.local/bin`, `~/.cargo/bin` and every other `bin` under it in each
/// later command, and the confinement already declines to count such a directory as a checkout for
/// the same reason. Nothing is appended where the home cannot be resolved.
///
/// # Errors
///
/// The error from creating the state directory or writing the record.
fn append(line: &[u8]) -> std::io::Result<()> {
    let Some(directory) = crate::home::writable() else {
        return Ok(());
    };
    let named = line
        .strip_suffix(b"\n")
        .and_then(|line| line.splitn(2, |byte| *byte == b'\t').nth(1))
        .and_then(bravebot_sandbox::programs::record_path);
    if named.is_none_or(|named| holds_the_home(named, crate::home::profile().as_deref())) {
        return Ok(());
    }
    append_in(&directory, line)
}

/// Whether `directory` is `profile`, the account's home directory, or a directory above it, by any
/// spelling of either: as given and as the file system resolves it. With `HOME` a link to the real
/// home, a session opened on the resolved spelling of the home is the home as much as one opened on
/// the link.
///
/// Nothing is held where there is no home. A home that cannot be resolved holds every directory, so
/// nothing is appended that might be it. A directory that does not exist yet is compared as given:
/// it is not the home, which exists, and a stage that makes it later is why it is recorded.
fn holds_the_home(directory: &Path, profile: Option<&Path>) -> bool {
    let Some(profile) = profile else {
        return false;
    };
    let Ok(profile_resolved) = profile.canonicalize() else {
        return true;
    };
    let directory_resolved = directory.canonicalize().ok();
    [profile, profile_resolved.as_path()].iter().any(|home| {
        home.starts_with(directory)
            || directory_resolved
                .as_deref()
                .is_some_and(|resolved| home.starts_with(resolved))
    })
}

/// Read the record at `path` into `bytes`, at most [`RECORD_MOST_BYTES`] of it. What was read before
/// an error is left in `bytes`.
fn read_up_to_the_bound(path: &Path, bytes: &mut Vec<u8>) -> std::io::Result<()> {
    std::fs::File::open(path)?
        .take(RECORD_MOST_BYTES as u64)
        .read_to_end(bytes)?;
    Ok(())
}

/// The contents of the record in `directory`, at most [`RECORD_MOST_BYTES`] bytes of it, as bytes
/// and whole lines only. Empty where there is no record or it cannot be read; what was read before
/// an error is kept, to its last whole line.
///
/// The bytes are handed on as they are, and are not read through `from_utf8` or a lossy rendering:
/// on Unix a path is bytes, and a line that is not text names a directory that is not UTF-8, which
/// the record holds so that a later process leaves it out ([`bravebot_sandbox::programs::record_path`]).
#[must_use]
pub fn read_in(directory: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    let read = read_up_to_the_bound(&directory.join(FILE), &mut bytes);
    if read.is_err() || bytes.len() >= RECORD_MOST_BYTES {
        // The last line may be cut short, and a cut path can name a directory above the one that
        // was written.
        let whole = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |at| at + 1);
        bytes.truncate(whole);
    }
    bytes
}

/// Whether the record at `path` holds as many bytes or lines as are read, so nothing more is added.
/// A record that is not there is not full.
///
/// # Errors
///
/// The error from reading a record that is there, since its size is then not known.
fn is_full(path: &Path) -> std::io::Result<bool> {
    let mut bytes = Vec::new();
    match read_up_to_the_bound(path, &mut bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    }
    Ok(bytes.len() >= RECORD_MOST_BYTES
        || bytes.iter().filter(|byte| **byte == b'\n').count() >= RECORD_MOST_LINES)
}

/// Add `line`, as bytes ending in a line break, to the record in `directory` in one write, so two
/// processes appending at once leave whole lines and a repeated line does no harm. Nothing is added
/// once the record holds [`RECORD_MOST_BYTES`] bytes or [`RECORD_MOST_LINES`] lines, the most that
/// is read. Creates `directory` private to the account.
///
/// # Errors
///
/// The error from reading the record, creating the directory or writing the file.
pub fn append_in(directory: &Path, line: &[u8]) -> std::io::Result<()> {
    let path = directory.join(FILE);
    if is_full(&path)? {
        return Ok(());
    }
    crate::home::create_directory(directory)?;
    crate::home::append_to_file(&path)?.write_all(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory = crate::testutil::scratch_dir(&format!("stage-record-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("scratch directory");
        directory
    }

    /// A record that was never written reads as nothing. Rejects a read that fails or panics on a
    /// missing file, which would stop every command on a machine that has recorded nothing.
    #[test]
    fn a_missing_record_reads_as_empty() {
        let directory = scratch("missing");
        assert_eq!(read_in(&directory), b"");
        assert_eq!(read_in(&directory.join("no-such-directory")), b"");
    }

    /// The account's home directory and every directory above it are not appended, and a directory
    /// beside or below it is. Rejects an append that records a session opened on the home
    /// directory, which leaves every `bin` under the home out of the next command's search, and a
    /// guard that compares text and holds `/home/personal` to be above `/home/person`. A home that
    /// cannot be resolved holds every directory, so an unreadable home is never appended by accident,
    /// and a directory that does not exist yet is still recorded.
    #[test]
    fn a_directory_that_is_or_holds_the_home_is_not_recorded() {
        let top = scratch("home-guard").canonicalize().expect("canonical top");
        let home = top.join("person");
        let below = home.join("project");
        let beside = top.join("personal");
        for made in [&below, &beside] {
            std::fs::create_dir_all(made).expect("directory");
        }

        for held in home.ancestors() {
            assert!(holds_the_home(held, Some(&home)), "{}", held.display());
        }
        for kept in [&below, &beside] {
            assert!(!holds_the_home(kept, Some(&home)), "{}", kept.display());
        }
        assert!(!holds_the_home(&home, None), "no home holds nothing");
        assert!(
            !holds_the_home(&top.join("gone"), Some(&home)),
            "a directory that does not exist yet is not the home"
        );
        assert!(
            holds_the_home(&home, Some(&top.join("gone"))),
            "a home that cannot be resolved holds every directory"
        );
    }

    /// A link to the home is the home, whichever spelling the directory or the home is given in.
    /// Rejects a guard that compares the spellings as given, which records the resolved spelling of
    /// the home when `HOME` is a link.
    #[cfg(unix)]
    #[test]
    fn a_directory_is_the_home_by_the_spelling_it_resolves_to() {
        let top = scratch("home-link").canonicalize().expect("canonical top");
        let real = top.join("real");
        let project = real.join("project");
        std::fs::create_dir_all(&project).expect("directory");
        let link = top.join("link");
        std::os::unix::fs::symlink(&real, &link).expect("link");

        assert!(
            holds_the_home(&real, Some(&link)),
            "the resolved spelling of the home"
        );
        assert!(
            holds_the_home(&top, Some(&link)),
            "a directory above the resolved home"
        );
        assert!(
            holds_the_home(&link, Some(&real)),
            "the link to the resolved home"
        );
        assert!(holds_the_home(&link, Some(&link)));
        assert!(
            !holds_the_home(&project, Some(&link)),
            "a directory below the home is not it"
        );
        assert!(!holds_the_home(&link.join("project"), Some(&link)));
    }

    /// What is appended is read back, whole lines in the order written, and a second line does not
    /// replace the first. Rejects an append that truncates the file.
    #[test]
    fn lines_appended_are_read_back_in_order() {
        let directory = scratch("append");
        append_in(&directory, b"directory\t/one\n").expect("first line");
        append_in(&directory, b"directory\t/two\n").expect("second line");
        assert_eq!(read_in(&directory), b"directory\t/one\ndirectory\t/two\n");
    }

    /// The state directory is made private to the account when the record is the first thing
    /// written into it. Rejects a record written into a directory made with the umask.
    #[cfg(unix)]
    #[test]
    fn the_record_and_its_directory_are_private_to_the_account() {
        use std::os::unix::fs::PermissionsExt;
        let directory = scratch("private").join("state");
        append_in(&directory, b"directory\t/one\n").expect("line");
        let mode = |path: &Path| {
            std::fs::metadata(path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode(&directory), 0o700);
        assert_eq!(mode(&directory.join(FILE)), 0o600);
    }

    /// Once the record holds the most bytes it is read to, nothing is added, and a read stops at
    /// the bound on a whole line. Rejects an append with no bound, which grows the file at every
    /// session for as long as the account lives, and a read that takes a cut line, which names a
    /// directory above the one written.
    #[test]
    fn a_record_at_the_size_bound_takes_no_more_and_is_read_to_a_whole_line() {
        let directory = scratch("bound");
        let line = "directory\t/a-directory-of-some-length\n";
        let mut full = String::new();
        while full.len() < RECORD_MOST_BYTES {
            full.push_str(line);
        }
        std::fs::write(directory.join(FILE), &full).expect("full record");
        append_in(&directory, b"directory\t/one-more\n").expect("append past the bound");
        let held = std::fs::read_to_string(directory.join(FILE)).expect("record");
        assert!(
            !held.contains("/one-more"),
            "the record grew past its bound"
        );
        let read = read_in(&directory);
        assert!(read.len() <= RECORD_MOST_BYTES);
        assert!(read.ends_with(b"\n"), "the read ends on a cut line");
        assert!(
            read.strip_suffix(b"\n")
                .unwrap_or(&read)
                .split(|byte| *byte == b'\n')
                .all(|read| read == line.trim_end().as_bytes())
        );
    }

    /// A record that holds as many bytes as are read takes no more, though it holds few lines.
    /// Rejects an append bound that counts lines alone, which lets long lines grow the file past
    /// what is read. A record just under both bounds takes the line, so the fixture is not one that
    /// refuses everything.
    #[test]
    fn a_record_at_the_byte_bound_alone_takes_no_more() {
        let directory = scratch("byte-bound");
        let long = format!("directory\t/{}\n", "a".repeat(200));
        let lines = RECORD_MOST_BYTES / long.len() + 1;
        assert!(
            lines < RECORD_MOST_LINES,
            "the fixture must stay under the line bound"
        );
        std::fs::write(directory.join(FILE), long.repeat(lines)).expect("record");
        append_in(&directory, b"directory\t/one-more\n").expect("append past the byte bound");
        let held = std::fs::read_to_string(directory.join(FILE)).expect("record");
        assert!(
            !held.contains("/one-more"),
            "the record grew past its byte bound"
        );

        let under = scratch("byte-bound-under");
        std::fs::write(under.join(FILE), long.repeat(lines - 2)).expect("record");
        append_in(&under, b"directory\t/one-more\n").expect("append under the bounds");
        let held = std::fs::read_to_string(under.join(FILE)).expect("record");
        assert!(
            held.ends_with("directory\t/one-more\n"),
            "the control took no line"
        );
    }

    /// A record that holds as many lines as are read takes no more, though it holds few bytes.
    /// Rejects an append bound that counts bytes alone, which lets a file of short lines grow past
    /// what is read. A record one line short of the bound takes the line.
    #[test]
    fn a_record_at_the_line_bound_alone_takes_no_more() {
        let directory = scratch("line-bound");
        let short = "directory\t/d\n";
        assert!(
            short.len() * RECORD_MOST_LINES < RECORD_MOST_BYTES,
            "the fixture must stay under the byte bound"
        );
        std::fs::write(directory.join(FILE), short.repeat(RECORD_MOST_LINES)).expect("record");
        append_in(&directory, b"directory\t/one-more\n").expect("append past the line bound");
        let held = std::fs::read_to_string(directory.join(FILE)).expect("record");
        assert!(
            !held.contains("/one-more"),
            "the record grew past its line bound"
        );

        let under = scratch("line-bound-under");
        std::fs::write(under.join(FILE), short.repeat(RECORD_MOST_LINES - 1)).expect("record");
        append_in(&under, b"directory\t/one-more\n").expect("append under the bounds");
        let held = std::fs::read_to_string(under.join(FILE)).expect("record");
        assert!(
            held.ends_with("directory\t/one-more\n"),
            "the control took no line"
        );
    }

    /// A record that is there and cannot be read is an error, which the sink reports, and nothing is
    /// added to it. Rejects an append that reads a failure as room, which writes past a bound it
    /// could not check. The record is made write-only, so the read fails and a write would not.
    #[cfg(unix)]
    #[test]
    fn a_record_that_cannot_be_read_is_not_appended_to() {
        use std::os::unix::fs::PermissionsExt;
        let directory = scratch("unreadable");
        let record = directory.join(FILE);
        std::fs::write(&record, "").expect("record");
        std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o200)).expect("mode");
        if std::fs::File::open(&record).is_ok() {
            // A process that reads whatever it likes, such as root, has no unreadable record.
            return;
        }

        let appended = append_in(&directory, b"directory\t/one\n");

        std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o600)).expect("mode");
        assert!(
            appended.is_err(),
            "a record that cannot be read was appended to"
        );
        assert_eq!(std::fs::read_to_string(&record).expect("record"), "");
    }

    /// A record whose bytes are not text is handed on as the bytes it holds, with the lines on
    /// either side of it, so a directory whose name is not UTF-8 is left out by a later process.
    /// Rejects a read that gives up on the whole file for one bad byte, which drops every directory
    /// in it, one that drops the line that is not text, which leaves that directory searched, and
    /// one that reads it through a lossy rendering, which names a directory nobody recorded.
    #[test]
    fn a_record_with_bytes_that_are_not_text_is_handed_on_as_it_is() {
        let directory = scratch("bytes");
        let mut bytes = b"directory\t/one\n".to_vec();
        bytes.extend_from_slice(b"directory\t/tool-\xff\n");
        bytes.extend_from_slice(b"directory\t/two\n");
        std::fs::write(directory.join(FILE), &bytes).expect("record");
        assert_eq!(read_in(&directory), bytes);
    }
}
