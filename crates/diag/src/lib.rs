//! The diagnostic log: what went wrong, as hosts, counts and words, never content.
//!
//! A log is for a person reading it after a failure, so it records the failure's shape (which
//! host, which status, which attempt) and nothing a server, a file or a model said. That is held
//! by the type of a field rather than by each caller's care: a [`Field`] is a sanitised host, a
//! number or a fixed word, and there is no constructor from a string of unknown origin. The log is
//! write-only. Nothing here reads a file back, so a line a hostile reply managed to influence
//! cannot reach the driver or the planner. The one thing that looks into the directory is
//! [`newest`], which reads file names and never a file.

#![forbid(unsafe_code)]

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// The subdirectory of the state directory the logs live in.
pub const DIRECTORY: &str = "logs";

/// How many log files are kept. A new one is made per process, so this is how many recent runs a
/// bug report can draw on.
pub const KEEP: usize = 10;

const HOST_LIMIT: usize = 253;

/// How much is written: `Error` only failures, `Info` also the steps taken, `Debug` also the
/// detail between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Info,
    Debug,
}

impl Level {
    /// The level a flag's word names.
    pub fn parse(word: &str) -> Option<Level> {
        match word {
            "error" => Some(Level::Error),
            "info" => Some(Level::Info),
            "debug" => Some(Level::Debug),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
        }
    }
}

/// One value on a line: a sanitised host, a number or a fixed word. The representation is private
/// and there is no constructor from a string of unknown origin, so a body, a prompt, a header or a
/// credential has no way in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field(Repr);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Repr {
    Host(String),
    Num(u64),
    Word(&'static str),
}

impl Field {
    /// The host of `url`, with any userinfo, path, query and fragment dropped. A host that is not
    /// made of host characters is written as `?`, so no byte a server chose lands in the file.
    pub fn host(url: &str) -> Field {
        Field(Repr::Host(host_of(url)))
    }

    pub fn num(value: impl TryInto<u64>) -> Field {
        Field(Repr::Num(value.try_into().unwrap_or(u64::MAX)))
    }

    pub fn word(word: &'static str) -> Field {
        Field(Repr::Word(word))
    }

    fn write_to(&self, out: &mut String) {
        match &self.0 {
            Repr::Host(host) => out.push_str(host),
            Repr::Num(n) => out.push_str(&n.to_string()),
            Repr::Word(word) => out.push_str(word),
        }
    }
}

fn host_of(url: &str) -> String {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    let allowed =
        |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']');
    if host.is_empty() || host.len() > HOST_LIMIT || !host.chars().all(allowed) {
        return "?".to_string();
    }
    host.to_string()
}

/// One log: a level, a directory, and the file made on the first line that is written.
pub struct Log {
    level: Level,
    dir: Option<PathBuf>,
    file: Option<File>,
    gave_up: bool,
}

impl Log {
    /// A log that writes into `dir` at `level`. With no directory nothing is ever written.
    pub fn new(level: Level, dir: Option<PathBuf>) -> Log {
        Log {
            level,
            dir,
            file: None,
            gave_up: false,
        }
    }

    /// Writes one line if `level` is within what was asked for. The file is made here, on the
    /// first line, so a run with nothing to report leaves nothing behind. A failure to write is
    /// dropped: the log is never the reason a run stops.
    pub fn record(&mut self, level: Level, event: &'static str, fields: &[(&'static str, Field)]) {
        if level > self.level || self.gave_up {
            return;
        }
        let Some(dir) = self.dir.clone() else {
            return;
        };
        if self.file.is_none() {
            match open_new(&dir) {
                Some(file) => self.file = Some(file),
                None => {
                    self.gave_up = true;
                    return;
                }
            }
        }
        let line = format_line(now_millis(), level, event, fields);
        if let Some(file) = self.file.as_mut() {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

fn format_line(
    millis: u128,
    level: Level,
    event: &'static str,
    fields: &[(&'static str, Field)],
) -> String {
    let mut line = format!("{} {} {}", timestamp(millis), level.tag(), event);
    for (key, value) in fields {
        line.push(' ');
        line.push_str(key);
        line.push('=');
        value.write_to(&mut line);
    }
    line.push('\n');
    line
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// `2026-10-08T12:00:00.123Z`, from milliseconds since the epoch, without a date library.
fn timestamp(millis: u128) -> String {
    let (date, time) = civil(millis);
    format!("{date}T{time}.{:03}Z", millis % 1000)
}

/// `(YYYY-MM-DD, HH:MM:SS)` in UTC.
fn civil(millis: u128) -> (String, String) {
    let secs = (millis / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Days since 1970-01-01 to a calendar date (Hinnant's civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        format!("{year:04}-{month:02}-{day:02}"),
        format!("{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60),
    )
}

/// A file name this crate made: `YYYYMMDDTHHMMSSZ-<pid>.log`. Retention deletes only these.
fn is_log_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".log") else {
        return false;
    };
    let Some((stamp, pid)) = stem.split_once('-') else {
        return false;
    };
    let bytes = stamp.as_bytes();
    bytes.len() == 16
        && bytes[8] == b'T'
        && bytes[15] == b'Z'
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && !pid.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
}

/// The path of the newest log this crate made in `dir`, chosen from the file names alone.
///
/// For a person to be told which file to attach. No file is opened, so a line in one cannot reach
/// whatever asks (DIAG-6). The names start with a timestamp, so the greatest sorts newest.
pub fn newest(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_log_name(n))
        .max()
        .map(|name| dir.join(name))
}

fn file_name(millis: u128) -> String {
    let (date, time) = civil(millis);
    format!(
        "{}T{}Z-{}.log",
        date.replace('-', ""),
        time.replace(':', ""),
        std::process::id()
    )
}

fn open_new(dir: &Path) -> Option<File> {
    make_directory(dir).ok()?;
    let path = dir.join(file_name(now_millis()));
    let file = create_private(&path).ok()?;
    prune(dir, &path);
    Some(file)
}

/// Deletes the oldest files this crate made until [`KEEP`] remain, never `current`, which a clock
/// set behind the older files would otherwise sort first. Anything else in the directory is left
/// alone.
fn prune(dir: &Path, current: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_log_name(n) && current.file_name().is_none_or(|c| c != n.as_str()))
        .collect();
    names.sort();
    let excess = (names.len() + 1).saturating_sub(KEEP);
    for name in &names[..excess] {
        let _ = fs::remove_file(dir.join(name));
    }
}

#[cfg(unix)]
fn make_directory(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn make_directory(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)
}

#[cfg(unix)]
fn create_private(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .create_new(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().create_new(true).append(true).open(path)
}

static LOG: Mutex<Option<Log>> = Mutex::new(None);

/// Sets where this process logs and how much. Before this is called nothing is written. The CLI
/// calls it with no directory under incognito or with no home.
pub fn configure(level: Level, dir: Option<PathBuf>) {
    let mut guard = LOG.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(Log::new(level, dir));
}

fn record(level: Level, event: &'static str, fields: &[(&'static str, Field)]) {
    let mut guard = LOG.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(log) = guard.as_mut() {
        log.record(level, event, fields);
    }
}

pub fn error(event: &'static str, fields: &[(&'static str, Field)]) {
    record(Level::Error, event, fields);
}

pub fn info(event: &'static str, fields: &[(&'static str, Field)]) {
    record(Level::Info, event, fields);
}

pub fn debug(event: &'static str, fields: &[(&'static str, Field)]) {
    record(Level::Debug, event, fields);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("a scratch directory")
    }

    fn only_file(dir: &Path) -> PathBuf {
        let mut files: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(files.len(), 1, "expected one log file in {dir:?}");
        files.remove(0)
    }

    /// A person is told which log to attach by its name alone, and the newest is the greatest
    /// timestamp, not the last one written or the one whose name sorts last as a plain file.
    #[test]
    fn the_newest_log_is_the_greatest_timestamp_among_files_this_crate_made() {
        let dir = scratch();
        assert_eq!(newest(&dir.path().join("absent")), None);
        assert_eq!(newest(dir.path()), None);
        for name in [
            "20260101T000000Z-9.log",
            "20260301T000000Z-2.log",
            "20260201T000000Z-5.log",
            "zzz.log",
            "99999999T999999Z.log",
        ] {
            fs::write(dir.path().join(name), "x").unwrap();
        }
        assert_eq!(
            newest(dir.path()),
            Some(dir.path().join("20260301T000000Z-2.log"))
        );
    }

    /// A URL's userinfo is a credential and its path and query are the page asked for, so only the
    /// host may be written. Keeping the whole URL would put a token in a file a user attaches to a
    /// public issue.
    #[test]
    fn a_host_field_drops_userinfo_path_and_query() {
        assert_eq!(
            Field::host("https://user:secret@api.example.com:8443/v1/x?key=abc#frag"),
            Field(Repr::Host("api.example.com:8443".to_string()))
        );
    }

    /// A host with a newline or a space in it is a forged second line, so it is not written.
    #[test]
    fn a_host_with_odd_bytes_is_written_as_a_question_mark() {
        for url in [
            "https://exa\nmple.com/",
            "https://exa mple.com/",
            "https://ex\u{1b}[31m.com/",
            "https:///path",
        ] {
            assert_eq!(
                Field::host(url),
                Field(Repr::Host("?".to_string())),
                "{url:?}"
            );
        }
    }

    /// Only the lines at or under the level asked for are written.
    #[test]
    fn a_line_above_the_level_is_not_written() {
        let tmp = scratch();
        let dir = tmp.path().to_path_buf();
        let mut log = Log::new(Level::Info, Some(dir.clone()));
        log.record(Level::Debug, "too.detailed", &[]);
        log.record(Level::Info, "step", &[]);
        log.record(Level::Error, "failure", &[]);
        let text = fs::read_to_string(only_file(&dir)).unwrap();
        assert!(!text.contains("too.detailed"));
        assert!(text.contains("INFO step"));
        assert!(text.contains("ERROR failure"));
    }

    /// A run that never fails leaves no file, since at the default level the file is made by the
    /// first error and not by starting.
    #[test]
    fn no_file_is_made_until_a_line_is_written() {
        let tmp = scratch();
        let dir = tmp.path().to_path_buf();
        let logs = dir.join("logs");
        let mut log = Log::new(Level::Error, Some(logs.clone()));
        log.record(Level::Info, "step", &[]);
        log.record(Level::Debug, "detail", &[]);
        assert!(!logs.exists(), "a log directory was made for nothing");
    }

    /// With no directory (incognito, or no home) nothing is written anywhere.
    #[test]
    fn a_log_with_no_directory_writes_nothing() {
        let mut log = Log::new(Level::Debug, None);
        log.record(Level::Error, "failure", &[]);
        assert!(log.file.is_none());
    }

    /// The file holds what the session was doing, so it is the owner's alone, in a directory that
    /// is too.
    #[cfg(unix)]
    #[test]
    fn the_log_and_its_directory_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = scratch();
        let dir = tmp.path().to_path_buf();
        let mut log = Log::new(Level::Error, Some(dir.join("logs")));
        log.record(Level::Error, "failure", &[]);
        let logs = dir.join("logs");
        let file = only_file(&logs);
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(&logs), 0o700);
    }

    /// Old logs go, so the directory does not grow without bound, but a file this crate did not
    /// make is not its to delete.
    #[test]
    fn retention_keeps_the_newest_and_leaves_foreign_files() {
        let tmp = scratch();
        let dir = tmp.path().to_path_buf();
        fs::create_dir_all(&dir).unwrap();
        for day in 1..=14 {
            fs::write(dir.join(format!("202601{day:02}T000000Z-1.log")), "old").unwrap();
        }
        fs::write(dir.join("notes.txt"), "mine").unwrap();
        fs::write(dir.join("20260101T000000Z.log"), "not ours").unwrap();
        let mut log = Log::new(Level::Error, Some(dir.clone()));
        log.record(Level::Error, "failure", &[]);
        let mut ours: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| is_log_name(n))
            .collect();
        ours.sort();
        assert_eq!(ours.len(), KEEP);
        assert!(!ours.contains(&"20260101T000000Z-1.log".to_string()));
        assert!(ours.contains(&"20260114T000000Z-1.log".to_string()));
        assert!(dir.join("notes.txt").exists());
        assert!(dir.join("20260101T000000Z.log").exists());
    }

    /// A clock set behind the older files would sort the new one first, and pruning it would lose
    /// the log of the run that is writing it.
    #[test]
    fn the_file_being_written_survives_retention_when_the_clock_is_behind() {
        let tmp = scratch();
        let dir = tmp.path().to_path_buf();
        fs::create_dir_all(&dir).unwrap();
        for day in 1..=KEEP {
            fs::write(dir.join(format!("20990101T0000{day:02}Z-1.log")), "future").unwrap();
        }
        let mut log = Log::new(Level::Error, Some(dir.clone()));
        log.record(Level::Error, "failure", &[]);
        let text: String = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| fs::read_to_string(e.path()).unwrap_or_default())
            .collect();
        assert!(text.contains("ERROR failure"), "the new log was pruned");
        let count = fs::read_dir(&dir).unwrap().count();
        assert_eq!(count, KEEP);
    }

    /// An underscore is a legal host character, and a service named with one is the host a person
    /// most needs to see in the log, not `?`.
    #[test]
    fn a_host_with_an_underscore_is_kept() {
        assert_eq!(
            Field::host("http://my_service:8080/x"),
            Field(Repr::Host("my_service:8080".to_string()))
        );
    }

    /// Dates are computed by hand, and a leap day is where that goes wrong.
    #[test]
    fn the_timestamp_is_the_utc_date() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(timestamp(1_709_164_800_123), "2024-02-29T00:00:00.123Z");
        assert_eq!(timestamp(1_791_460_800_000), "2026-10-08T12:00:00.000Z");
    }

    /// A line is one event, its keys and its values, and a value is a host, a number or a word.
    #[test]
    fn a_line_names_the_event_and_its_fields() {
        let line = format_line(
            0,
            Level::Error,
            "net.fetch",
            &[
                ("host", Field::host("https://example.com/a?b=c")),
                ("status", Field::num(503u16)),
                ("kind", Field::word("status")),
            ],
        );
        assert_eq!(
            line,
            "1970-01-01T00:00:00.000Z ERROR net.fetch host=example.com status=503 kind=status\n"
        );
    }

    #[test]
    fn a_level_is_parsed_from_its_word() {
        assert_eq!(Level::parse("debug"), Some(Level::Debug));
        assert_eq!(Level::parse("trace"), None);
        assert_eq!(Level::parse("INFO"), None);
    }
}
