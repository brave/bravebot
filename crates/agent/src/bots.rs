use serde_json::{Map, Number, Value, json};
use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const CEILING: usize = 100;

pub const APPLICATION_FOLDER: &str = "bravebot-ui";

const BOTS_FILE: &str = "bots.json";
const LOCK_FILE: &str = "bots.json.lock";
const OLD_FILE: &str = "bravebot-ui.json";
const LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_POLL: Duration = Duration::from_millis(20);
const MODEL_MAX: usize = 512;
const SLUG_MAX: usize = 64;
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    pub id: String,
    pub directory: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub slug: String,
    pub name: String,
    pub purpose: String,
    pub avatar: String,
    pub model: Option<String>,
    pub home: String,
    pub definition: Option<String>,
    pub session: Option<String>,
    pub conversations: Vec<Conversation>,
    pub archived: u64,
    pub retired: f64,
    pub created: f64,
    pub updated: f64,
}

impl Row {
    pub fn to_value(&self) -> Value {
        let mut fields = Map::new();
        fields.insert("slug".into(), json!(self.slug));
        fields.insert("name".into(), json!(self.name));
        fields.insert("purpose".into(), json!(self.purpose));
        fields.insert("avatar".into(), json!(self.avatar));
        fields.insert("model".into(), json!(self.model));
        fields.insert("home".into(), json!(self.home));
        fields.insert("definition".into(), json!(self.definition));
        fields.insert("session".into(), json!(self.session));
        fields.insert(
            "conversations".into(),
            Value::Array(
                self.conversations
                    .iter()
                    .map(|each| json!({"id": each.id, "directory": each.directory}))
                    .collect(),
            ),
        );
        fields.insert("archived".into(), Value::from(self.archived));
        fields.insert("retired".into(), number(self.retired));
        fields.insert("created".into(), number(self.created));
        fields.insert("updated".into(), number(self.updated));
        Value::Object(fields)
    }
}

fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER {
        Value::from(value as i64)
    } else {
        Number::from_f64(value).map_or(Value::Null, Value::Number)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    NotAnObject,
    BadSlug,
    RepeatedSlug,
    BlankNameOrPurpose,
    NoHome,
    BadSession,
    BadCreated,
    BadUpdated,
    PastCeiling,
}

impl fmt::Display for Reason {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            Self::NotAnObject => "it is not an object",
            Self::BadSlug => {
                "its slug is not lowercase letters and digits joined by single hyphens"
            }
            Self::RepeatedSlug => "its slug repeats the slug of an earlier bot",
            Self::BlankNameOrPurpose => "its name or purpose is blank or holds a null character",
            Self::NoHome => "it has no home folder and none can be made from the data folder",
            Self::BadSession => "its session is not an identifier",
            Self::BadCreated => "its created time is not a number",
            Self::BadUpdated => "its updated time is not a number",
            Self::PastCeiling => "it is past the 100th bot kept",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropped {
    pub index: usize,
    pub slug: Option<String>,
    pub reason: Reason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub path: PathBuf,
    pub reason: String,
}

impl Problem {
    fn new(path: &Path, reason: impl fmt::Display) -> Self {
        Self {
            path: path.to_path_buf(),
            reason: reason.to_string(),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}: {}", self.path.display(), self.reason)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Listing {
    pub rows: Vec<Row>,
    pub dropped: Vec<Dropped>,
    pub unreadable: Option<Problem>,
}

impl Listing {
    fn unreadable(problem: Problem) -> Self {
        Self {
            unreadable: Some(problem),
            ..Self::default()
        }
    }
}

#[derive(Debug)]
pub enum Refused {
    Incognito,
    NoFolder,
    Ceiling,
    Entry {
        index: usize,
        slug: Option<String>,
        reason: Reason,
    },
    Unreadable(Problem),
    OldFile(Problem),
    Locked(PathBuf),
    Io(Problem),
}

impl Refused {
    pub fn problem(&self) -> Option<Problem> {
        match self {
            Self::Unreadable(problem) | Self::OldFile(problem) | Self::Io(problem) => {
                Some(problem.clone())
            }
            Self::Locked(path) => Some(Problem::new(path, self)),
            _ => None,
        }
    }
}

impl fmt::Display for Refused {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incognito => {
                out.write_str("an incognito run keeps no bots, so nothing was saved")
            }
            Self::NoFolder => {
                out.write_str("this machine names no state directory to keep bots in")
            }
            Self::Ceiling => write!(
                out,
                "at most {CEILING} bots are kept, archived ones included"
            ),
            Self::Entry {
                index,
                slug,
                reason,
            } => match slug {
                Some(slug) => write!(out, "bot {slug} (entry {index}) was refused: {reason}"),
                None => write!(out, "entry {index} was refused: {reason}"),
            },
            Self::Unreadable(problem) => write!(
                out,
                "the bots file was left as it is, since it cannot be read as bots ({problem})"
            ),
            Self::OldFile(problem) => write!(
                out,
                "the bots in the old settings file cannot be moved until it is repaired or removed ({problem})"
            ),
            Self::Locked(path) => write!(
                out,
                "another program holds {}, so nothing was saved",
                path.display()
            ),
            Self::Io(problem) => write!(out, "the bots could not be saved ({problem})"),
        }
    }
}

impl std::error::Error for Refused {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Linux,
    MacOs,
    Windows,
}

impl System {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

pub fn data_folder(
    system: System,
    environment: impl Fn(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    let named = |variable: &str| environment(variable).filter(|value| !value.is_empty());
    let (base, separator, below) = match system {
        System::Linux => match named("XDG_CONFIG_HOME").filter(|value| {
            value
                .to_str()
                .is_some_and(|text| text.starts_with('/') && !text.contains('\0'))
        }) {
            Some(config) => (config, '/', None),
            None => (named("HOME")?, '/', Some(".config")),
        },
        System::MacOs => (named("HOME")?, '/', Some("Library/Application Support")),
        System::Windows => (named("APPDATA")?, '\\', None),
    };
    let mut folder = base;
    if let Some(below) = below {
        folder.push(format!("{separator}{below}"));
    }
    folder.push(format!("{separator}{APPLICATION_FOLDER}"));
    Some(PathBuf::from(folder))
}

pub fn bots_file(home: &Path) -> PathBuf {
    home.join(BOTS_FILE)
}

pub fn is_absolute_path(value: &str) -> bool {
    if value.contains('\0') {
        return false;
    }
    if value.starts_with('/') {
        return true;
    }
    let bytes = value.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let share = is_share(value);
    (drive || share) && !value[2..].contains(':')
}

fn is_share(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("\\\\") else {
        return false;
    };
    let is_separator = |character: char| matches!(character, '\\' | '/' | ':');
    let Some(first) = rest.chars().next() else {
        return false;
    };
    if is_separator(first) || matches!(first, '?' | '.') {
        return false;
    }
    let Some((server, share)) = rest.split_once('\\') else {
        return false;
    };
    !server.contains(is_separator) && share.chars().next().is_some_and(|c| !is_separator(c))
}

pub fn is_slug(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= SLUG_MAX
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
        && !value.contains("--")
}

pub fn is_identifier(value: &str) -> bool {
    !value.is_empty() && !value.contains(['/', '\0'])
}

fn is_text(value: &str) -> bool {
    !value.trim_matches(is_blank).is_empty() && !value.contains('\0')
}

fn is_blank(character: char) -> bool {
    character.is_whitespace() || character == '\u{feff}'
}

pub fn is_model(value: &str) -> bool {
    is_text(value)
        && value.chars().count() <= MODEL_MAX
        && !value.contains(['\n', '\r'])
        && value == value.trim_matches(is_blank)
        && !value.eq_ignore_ascii_case("inherit")
}

struct Reading<'a> {
    data_folder: &'a Path,
    now: f64,
}

struct Parsed {
    rows: Vec<Row>,
    dropped: Vec<Dropped>,
}

fn slug_of(entry: &Value) -> Option<String> {
    entry.get("slug")?.as_str().map(str::to_string)
}

fn parse_entries(entries: &[Value], reading: &Reading) -> Parsed {
    let mut rows: Vec<Row> = Vec::new();
    let mut dropped = Vec::new();
    let mut seen = HashSet::new();
    for (index, entry) in entries.iter().enumerate() {
        if rows.len() == CEILING {
            dropped.push(Dropped {
                index,
                slug: slug_of(entry),
                reason: Reason::PastCeiling,
            });
            continue;
        }
        match read_entry(entry, reading, &seen) {
            Ok(row) => {
                seen.insert(row.slug.clone());
                rows.push(row);
            }
            Err(reason) => dropped.push(Dropped {
                index,
                slug: slug_of(entry),
                reason,
            }),
        }
    }
    Parsed { rows, dropped }
}

fn read_entry(entry: &Value, reading: &Reading, seen: &HashSet<String>) -> Result<Row, Reason> {
    let fields = entry.as_object().ok_or(Reason::NotAnObject)?;
    let text_of = |name: &str| fields.get(name).and_then(Value::as_str);

    let slug = text_of("slug")
        .filter(|slug| is_slug(slug))
        .ok_or(Reason::BadSlug)?;
    if seen.contains(slug) {
        return Err(Reason::RepeatedSlug);
    }
    let (Some(name), Some(purpose)) = (
        text_of("name").filter(|name| is_text(name)),
        text_of("purpose").filter(|purpose| is_text(purpose)),
    ) else {
        return Err(Reason::BlankNameOrPurpose);
    };
    let home = match text_of("home").filter(|home| is_absolute_path(home)) {
        Some(home) => home.to_string(),
        None => reading
            .data_folder
            .join("bot-homes")
            .join(slug)
            .to_str()
            .filter(|composed| is_absolute_path(composed))
            .ok_or(Reason::NoHome)?
            .to_string(),
    };
    let legacy_directory = text_of("directory").filter(|directory| is_absolute_path(directory));
    let session = match fields.get("session") {
        None | Some(Value::Null) => None,
        Some(Value::String(session)) if is_identifier(session) => Some(session.clone()),
        Some(_) => return Err(Reason::BadSession),
    };
    let created =
        number_when_present(fields.get("created")).map_err(|NotANumber| Reason::BadCreated)?;
    let updated =
        number_when_present(fields.get("updated")).map_err(|NotANumber| Reason::BadUpdated)?;

    let created = created.unwrap_or(reading.now);
    let fallback = legacy_directory.unwrap_or(&home);
    Ok(Row {
        slug: slug.to_string(),
        name: name.to_string(),
        purpose: purpose.to_string(),
        avatar: text_of("avatar")
            .filter(|avatar| is_text(avatar))
            .unwrap_or(slug)
            .to_string(),
        model: text_of("model")
            .filter(|model| is_model(model))
            .map(str::to_string),
        conversations: conversations(fields.get("conversations"), session.as_deref(), fallback),
        home,
        definition: text_of("definition")
            .filter(|definition| is_slug(definition))
            .map(str::to_string),
        session,
        archived: fields
            .get("archived")
            .and_then(Value::as_f64)
            .filter(|archived| {
                archived.fract() == 0.0 && (0.0..=MAX_SAFE_INTEGER).contains(archived)
            })
            .map_or(0, |archived| archived as u64),
        retired: fields
            .get("retired")
            .and_then(Value::as_f64)
            .filter(|retired| retired.is_finite() && *retired >= 0.0)
            .unwrap_or(0.0),
        created,
        updated: updated.unwrap_or(created),
    })
}

fn number_when_present(value: Option<&Value>) -> Result<Option<f64>, NotANumber> {
    match value {
        None => Ok(None),
        Some(Value::Number(number)) => number
            .as_f64()
            .filter(|n| n.is_finite())
            .map(Some)
            .ok_or(NotANumber),
        Some(_) => Err(NotANumber),
    }
}

struct NotANumber;

fn conversations(
    value: Option<&Value>,
    session: Option<&str>,
    fallback: &str,
) -> Vec<Conversation> {
    let mut kept = Vec::new();
    for entry in value.and_then(Value::as_array).into_iter().flatten() {
        match entry {
            Value::String(id) => add_conversation(&mut kept, Some(id), Some(fallback)),
            Value::Object(pair) => add_conversation(
                &mut kept,
                pair.get("id").and_then(Value::as_str),
                pair.get("directory").and_then(Value::as_str),
            ),
            _ => {}
        }
    }
    if let Some(session) = session
        && !kept.iter().any(|each| each.id == session)
    {
        add_conversation(&mut kept, Some(session), Some(fallback));
    }
    kept
}

fn add_conversation(kept: &mut Vec<Conversation>, id: Option<&str>, directory: Option<&str>) {
    let (Some(id), Some(directory)) = (
        id.filter(|id| is_identifier(id)),
        directory.filter(|directory| is_absolute_path(directory)),
    ) else {
        return;
    };
    if !kept
        .iter()
        .any(|each| each.id == id && each.directory == directory)
    {
        kept.push(Conversation {
            id: id.to_string(),
            directory: directory.to_string(),
        });
    }
}

fn entries_of(text: &str) -> Result<Vec<Value>, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    match value.get("bots") {
        Some(Value::Array(entries)) => Ok(entries.clone()),
        _ => Err("it holds no `bots` array".to_string()),
    }
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_millis() as f64)
}

pub fn list(data_folder: &Path) -> Listing {
    let Some(home) = crate::home::directory() else {
        return Listing::default();
    };
    list_in(&home, crate::home::writable().is_some(), data_folder, now())
}

pub fn list_in(home: &Path, may_move: bool, data_folder: &Path, now: f64) -> Listing {
    if may_move && let Err(refused) = move_old_rows(home, data_folder) {
        return match refused.problem() {
            Some(problem) => Listing::unreadable(problem),
            None => Listing::default(),
        };
    }
    let path = bots_file(home);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Listing::default(),
        Err(error) => return Listing::unreadable(Problem::new(&path, error)),
    };
    match entries_of(&text) {
        Ok(entries) => {
            let Parsed { rows, dropped } = parse_entries(&entries, &Reading { data_folder, now });
            Listing {
                rows,
                dropped,
                unreadable: None,
            }
        }
        Err(reason) => Listing::unreadable(Problem::new(&path, reason)),
    }
}

pub fn replace(data_folder: &Path, entries: &[Value]) -> Result<Vec<Row>, Refused> {
    if bravebot_core::incognito::engaged() {
        return Err(Refused::Incognito);
    }
    let home = crate::home::writable().ok_or(Refused::NoFolder)?;
    replace_in(&home, data_folder, entries, now(), LOCK_WAIT)
}

pub fn replace_in(
    home: &Path,
    data_folder: &Path,
    entries: &[Value],
    now: f64,
    wait: Duration,
) -> Result<Vec<Row>, Refused> {
    if entries.len() > CEILING {
        return Err(Refused::Ceiling);
    }
    let Parsed { mut rows, dropped } = parse_entries(entries, &Reading { data_folder, now });
    if let Some(first) = dropped.into_iter().next() {
        return Err(Refused::Entry {
            index: first.index,
            slug: first.slug,
            reason: first.reason,
        });
    }
    move_old_rows(home, data_folder)?;
    make_state_directory(home)?;
    let _held = lock(home, wait)?;
    let path = bots_file(home);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            entries_of(&text).map_err(|reason| Refused::Unreadable(Problem::new(&path, reason)))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(Refused::Unreadable(Problem::new(&path, error))),
    }
    rows.sort_by(|a, b| a.slug.cmp(&b.slug));
    write_entries(&path, rows.iter().map(Row::to_value).collect())?;
    Ok(rows)
}

fn make_state_directory(home: &Path) -> Result<(), Refused> {
    crate::home::create_directory(home).map_err(|error| Refused::Io(Problem::new(home, error)))
}

fn write_entries(path: &Path, entries: Vec<Value>) -> Result<(), Refused> {
    let mut text = serde_json::to_string_pretty(&json!({ "bots": entries }))
        .map_err(|error| Refused::Io(Problem::new(path, error)))?;
    text.push('\n');
    crate::mcp::replace(path, text).map_err(|error| Refused::Io(Problem::new(path, error)))
}

// The lock is on a file of its own because the rename replaces bots.json and a lock held on it would be lost.
fn lock(home: &Path, wait: Duration) -> Result<File, Refused> {
    let path = home.join(LOCK_FILE);
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&path)
        .map_err(|error| Refused::Io(Problem::new(&path, error)))?;
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(LOCK_POLL);
            }
            Err(TryLockError::WouldBlock) => return Err(Refused::Locked(path)),
            Err(TryLockError::Error(error)) => {
                return Err(Refused::Io(Problem::new(&path, error)));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moved {
    Nothing,
    Entries(usize),
}

enum OldRows {
    Nothing,
    Entries(Vec<Value>),
    Unconvertible(Problem),
}

fn old_rows(data_folder: &Path) -> OldRows {
    let path = data_folder.join(OLD_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return OldRows::Nothing,
        Err(error) => return OldRows::Unconvertible(Problem::new(&path, error)),
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => return OldRows::Unconvertible(Problem::new(&path, error)),
    };
    match value.get("bots") {
        None => OldRows::Nothing,
        Some(Value::Array(entries)) if entries.is_empty() => OldRows::Nothing,
        Some(Value::Array(entries)) => OldRows::Entries(entries.clone()),
        Some(_) => OldRows::Unconvertible(Problem::new(&path, "its `bots` key is not an array")),
    }
}

pub fn move_old_rows(home: &Path, data_folder: &Path) -> Result<Moved, Refused> {
    move_with(home, data_folder, LOCK_WAIT, || {})
}

fn move_with(
    home: &Path,
    data_folder: &Path,
    wait: Duration,
    before_lock: impl FnOnce(),
) -> Result<Moved, Refused> {
    let target = bots_file(home);
    if target.exists() {
        return Ok(Moved::Nothing);
    }
    match old_rows(data_folder) {
        OldRows::Nothing => return Ok(Moved::Nothing),
        OldRows::Unconvertible(problem) => return Err(Refused::OldFile(problem)),
        OldRows::Entries(_) => {}
    }
    make_state_directory(home)?;
    before_lock();
    let _held = lock(home, wait)?;
    if target.exists() {
        return Ok(Moved::Nothing);
    }
    match old_rows(data_folder) {
        OldRows::Nothing => Ok(Moved::Nothing),
        OldRows::Unconvertible(problem) => Err(Refused::OldFile(problem)),
        OldRows::Entries(entries) => {
            let moved = entries.len();
            write_entries(&target, entries)?;
            Ok(Moved::Entries(moved))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(&format!("bots-{name}"));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn home(&self) -> PathBuf {
            self.0.join("state")
        }

        fn data(&self) -> PathBuf {
            self.0.join("gui")
        }

        fn made(name: &str) -> Self {
            let scratch = Self::new(name);
            std::fs::create_dir_all(scratch.home()).expect("a state directory");
            std::fs::create_dir_all(scratch.data()).expect("a data folder");
            scratch
        }

        fn write_bots(&self, entries: &Value) {
            std::fs::create_dir_all(self.home()).expect("a state directory");
            std::fs::write(
                self.home().join("bots.json"),
                json!({ "bots": entries }).to_string(),
            )
            .expect("a bots file");
        }

        fn write_old(&self, text: &str) -> Vec<u8> {
            std::fs::create_dir_all(self.data()).expect("a data folder");
            std::fs::write(self.data().join("bravebot-ui.json"), text).expect("an old file");
            text.as_bytes().to_vec()
        }

        fn bots_text(&self) -> Option<String> {
            std::fs::read_to_string(self.home().join("bots.json")).ok()
        }

        fn stored(&self) -> Vec<Value> {
            let text = self.bots_text().expect("a bots file");
            entries_of(&text).expect("a bots array")
        }

        fn list(&self) -> Listing {
            list_in(&self.home(), true, &self.data(), 1_000.0)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn bot(slug: &str) -> Value {
        json!({
            "slug": slug, "name": slug.to_uppercase(), "purpose": "Does a thing.",
            "avatar": slug, "model": null, "home": format!("/gui/bot-homes/{slug}"),
            "definition": slug, "session": null, "conversations": [],
            "archived": 0, "retired": 0, "created": 5, "updated": 6,
        })
    }

    fn slugs(listing: &Listing) -> Vec<&str> {
        listing.rows.iter().map(|row| row.slug.as_str()).collect()
    }

    fn environment<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn linux_uses_the_xdg_config_home_when_it_is_absolute() {
        let found = data_folder(
            System::Linux,
            environment(&[("XDG_CONFIG_HOME", "/cfg"), ("HOME", "/home/p")]),
        );
        assert_eq!(found, Some(PathBuf::from("/cfg/bravebot-ui")));
    }

    #[test]
    fn linux_falls_back_to_dot_config_under_home() {
        for xdg in [None, Some(""), Some("relative/cfg")] {
            let mut pairs = vec![("HOME", "/home/p")];
            if let Some(xdg) = xdg {
                pairs.push(("XDG_CONFIG_HOME", xdg));
            }
            assert_eq!(
                data_folder(System::Linux, environment(&pairs)),
                Some(PathBuf::from("/home/p/.config/bravebot-ui")),
                "{xdg:?}"
            );
        }
    }

    #[test]
    fn macos_uses_application_support_under_home() {
        assert_eq!(
            data_folder(
                System::MacOs,
                environment(&[("HOME", "/Users/p"), ("XDG_CONFIG_HOME", "/cfg")])
            ),
            Some(PathBuf::from(
                "/Users/p/Library/Application Support/bravebot-ui"
            ))
        );
    }

    #[test]
    fn windows_uses_appdata() {
        assert_eq!(
            data_folder(
                System::Windows,
                environment(&[
                    ("APPDATA", "C:\\Users\\p\\AppData\\Roaming"),
                    ("HOME", "/h")
                ])
            ),
            Some(PathBuf::from("C:\\Users\\p\\AppData\\Roaming\\bravebot-ui"))
        );
    }

    #[test]
    fn a_missing_variable_names_no_data_folder() {
        assert_eq!(data_folder(System::Linux, environment(&[])), None);
        assert_eq!(
            data_folder(System::MacOs, environment(&[("HOME", "")])),
            None
        );
        assert_eq!(
            data_folder(System::Windows, environment(&[("HOME", "/h")])),
            None
        );
    }

    #[test]
    fn an_absolute_path_is_either_spelling_and_never_a_device_or_stream() {
        for accepted in [
            "/home/p",
            "C:\\work",
            "C:/work",
            "z:\\",
            "\\\\server\\share",
            "\\\\server\\share\\dir",
        ] {
            assert!(is_absolute_path(accepted), "{accepted}");
        }
        for refused in [
            "",
            "relative/dir",
            "C:work",
            "C:",
            "\\\\?\\C:\\work",
            "\\\\.\\pipe\\x",
            "\\\\server",
            "\\\\server\\",
            "\\\\:server\\share",
            "C:\\dir:stream",
            "\\\\server\\share:stream",
            "/with\0null",
        ] {
            assert!(!is_absolute_path(refused), "{refused:?}");
        }
    }

    #[test]
    fn a_slug_is_runs_of_lowercase_letters_and_digits_joined_by_single_hyphens() {
        for accepted in ["a", "web-dev", "a1-b2-c3", "9"] {
            assert!(is_slug(accepted), "{accepted}");
        }
        let longest = "a".repeat(64);
        assert!(is_slug(&longest));
        for refused in [
            "",
            "-a",
            "a-",
            "a--b",
            "A",
            "a_b",
            "a.b",
            "..",
            "a/b",
            "é",
            "a b",
            &"a".repeat(65),
        ] {
            assert!(!is_slug(refused), "{refused:?}");
        }
    }

    #[test]
    fn a_model_is_one_bounded_line_that_is_not_inherit() {
        assert!(is_model("provider/model-a"));
        assert!(is_model(&"m".repeat(512)));
        for refused in [
            "",
            "  ",
            " m",
            "m ",
            "m\nn",
            "m\rn",
            "m\0",
            "inherit",
            "Inherit",
            &"m".repeat(513),
        ] {
            assert!(!is_model(refused), "{refused:?}");
        }
    }

    #[test]
    fn a_bots_file_lists_each_bot_the_way_the_gui_read_it() {
        let scratch = Scratch::made("lists");
        let mut archived = bot("zeta");
        archived["retired"] = json!(900);
        let mut homeless = bot("mid");
        homeless.as_object_mut().unwrap().remove("home");
        let mut legacy = bot("old");
        legacy["conversations"] = json!(["conv-1"]);
        legacy["session"] = json!("conv-2");
        legacy["directory"] = json!("/projects/p");
        legacy["remembered"] = json!(7);
        legacy["quiet"] = json!(3);
        scratch.write_bots(&json!([
            archived,
            bot("Bad_Slug"),
            homeless,
            legacy,
            bot("alpha")
        ]));

        let listing = scratch.list();

        assert_eq!(slugs(&listing), ["zeta", "mid", "old", "alpha"]);
        assert_eq!(listing.rows[0].retired, 900.0);
        assert_eq!(
            listing.rows[1].home,
            scratch.data().join("bot-homes/mid").to_str().unwrap()
        );
        assert_eq!(
            listing.rows[2].conversations,
            vec![
                Conversation {
                    id: "conv-1".into(),
                    directory: "/projects/p".into()
                },
                Conversation {
                    id: "conv-2".into(),
                    directory: "/projects/p".into()
                },
            ]
        );
        assert_eq!(
            listing.dropped,
            vec![Dropped {
                index: 1,
                slug: Some("Bad_Slug".into()),
                reason: Reason::BadSlug
            }]
        );
        assert_eq!(listing.unreadable, None);
    }

    #[test]
    fn a_row_with_a_repairable_field_is_listed_repaired() {
        let scratch = Scratch::made("repairs");
        let mut row = bot("fix");
        row["avatar"] = json!("  ");
        row["model"] = json!("inherit");
        row["definition"] = json!("Not A Slug");
        row["archived"] = json!(-4);
        row["retired"] = json!("soon");
        let object = row.as_object_mut().unwrap();
        object.remove("created");
        object.remove("updated");
        scratch.write_bots(&json!([row]));

        let listing = scratch.list();

        let row = &listing.rows[0];
        assert_eq!(row.avatar, "fix");
        assert_eq!(row.model, None);
        assert_eq!(row.definition, None);
        assert_eq!(row.archived, 0);
        assert_eq!(row.retired, 0.0);
        assert_eq!(row.created, 1_000.0);
        assert_eq!(row.updated, 1_000.0);
        assert!(listing.dropped.is_empty());
    }

    #[test]
    fn a_home_in_the_windows_spelling_is_kept_and_a_relative_one_is_replaced() {
        let scratch = Scratch::made("spelling");
        let mut windows = bot("win");
        windows["home"] = json!("C:\\Users\\p\\bots\\win");
        let mut relative = bot("rel");
        relative["home"] = json!("bots/rel");
        scratch.write_bots(&json!([windows, relative]));

        let listing = scratch.list();

        assert_eq!(listing.rows[0].home, "C:\\Users\\p\\bots\\win");
        assert_eq!(
            listing.rows[1].home,
            scratch.data().join("bot-homes/rel").to_str().unwrap()
        );
    }

    #[test]
    fn every_entry_the_rules_drop_is_reported_with_its_reason() {
        let scratch = Scratch::made("drops");
        let with = |field: &str, value: Value| {
            let mut row = bot("x");
            row[field] = value;
            row
        };
        let mut named = bot("blank");
        named["name"] = json!(" \t");
        let mut one = bot("one");
        one["purpose"] = json!("has \u{0} inside");
        scratch.write_bots(&json!([
            "not an object",
            bot("dup"),
            bot("dup"),
            named,
            one,
            with("session", json!("a/b")),
            with("session", json!(7)),
            with("created", json!("yesterday")),
            with("created", Value::Null),
            with("updated", json!([1])),
            bot("kept"),
        ]));

        let listing = scratch.list();

        assert_eq!(slugs(&listing), ["dup", "kept"]);
        let reported: Vec<_> = listing
            .dropped
            .iter()
            .map(|each| (each.index, each.slug.as_deref(), each.reason))
            .collect();
        assert_eq!(
            reported,
            vec![
                (0, None, Reason::NotAnObject),
                (2, Some("dup"), Reason::RepeatedSlug),
                (3, Some("blank"), Reason::BlankNameOrPurpose),
                (4, Some("one"), Reason::BlankNameOrPurpose),
                (5, Some("x"), Reason::BadSession),
                (6, Some("x"), Reason::BadSession),
                (7, Some("x"), Reason::BadCreated),
                (8, Some("x"), Reason::BadCreated),
                (9, Some("x"), Reason::BadUpdated),
            ]
        );
    }

    #[test]
    fn rows_past_the_hundredth_are_reported_with_their_slugs() {
        let scratch = Scratch::made("ceiling-read");
        let mut entries: Vec<Value> = (0..100).map(|n| bot(&format!("b{n:03}"))).collect();
        entries.insert(10, bot("Bad_Slug"));
        entries.push(bot("extra-one"));
        entries.push(json!(5));
        entries.push(bot("Bad_Late"));
        scratch.write_bots(&Value::Array(entries));

        let listing = scratch.list();

        assert_eq!(listing.rows.len(), 100);
        assert_eq!(listing.rows[99].slug, "b099");
        let reported: Vec<_> = listing
            .dropped
            .iter()
            .map(|each| (each.index, each.slug.as_deref(), each.reason))
            .collect();
        assert_eq!(
            reported,
            vec![
                (10, Some("Bad_Slug"), Reason::BadSlug),
                (101, Some("extra-one"), Reason::PastCeiling),
                (102, None, Reason::PastCeiling),
                (103, Some("Bad_Late"), Reason::PastCeiling),
            ]
        );
    }

    #[test]
    fn a_missing_bots_file_gives_no_rows_and_makes_no_file() {
        let scratch = Scratch::new("missing");
        std::fs::create_dir_all(scratch.data()).unwrap();

        let listing = scratch.list();

        assert_eq!(listing, Listing::default());
        assert!(!scratch.home().exists());
    }

    #[test]
    fn a_file_that_does_not_parse_is_reported_and_never_overwritten() {
        for (name, text) in [
            ("truncated", "{\"bots\": [{\"slug\": \"a\""),
            ("empty", ""),
            ("not-bots", "{\"bots\": 4}"),
            ("a-list", "[]"),
        ] {
            let scratch = Scratch::made(&format!("unparsed-{name}"));
            std::fs::write(scratch.home().join("bots.json"), text).unwrap();

            let listing = scratch.list();
            let refused = replace_in(
                &scratch.home(),
                &scratch.data(),
                &[bot("a")],
                1.0,
                Duration::from_millis(50),
            )
            .unwrap_err();

            assert!(listing.rows.is_empty(), "{name}");
            assert_eq!(
                listing.unreadable.as_ref().map(|each| each.path.clone()),
                Some(scratch.home().join("bots.json")),
                "{name}"
            );
            assert!(
                matches!(refused, Refused::Unreadable(_)),
                "{name}: {refused}"
            );
            assert_eq!(scratch.bots_text().as_deref(), Some(text), "{name}");
        }
    }

    #[test]
    fn rows_are_written_in_the_byte_order_of_the_slug() {
        let scratch = Scratch::made("order");
        let given: Vec<Value> = ["b", "a2", "a-1", "10", "a"].map(bot).to_vec();

        let written = replace_in(&scratch.home(), &scratch.data(), &given, 1.0, LOCK_WAIT).unwrap();

        let order: Vec<_> = written.iter().map(|row| row.slug.as_str()).collect();
        assert_eq!(order, ["10", "a", "a-1", "a2", "b"]);
        let stored: Vec<_> = scratch
            .stored()
            .iter()
            .map(|entry| entry["slug"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(stored, order);
    }

    #[test]
    fn a_written_row_reads_back_without_the_fields_the_row_no_longer_has() {
        let scratch = Scratch::made("roundtrip");
        let mut row = bot("keep");
        row["model"] = json!("provider/m");
        row["session"] = json!("s-1");
        row["conversations"] = json!([{"id": "s-1", "directory": "/p"}]);
        row["archived"] = json!(3);
        row["retired"] = json!(1_700_000_000_000_i64);
        row["remembered"] = json!(9);
        row["quiet"] = json!(2);
        row["extra"] = json!("unknown");

        replace_in(&scratch.home(), &scratch.data(), &[row], 1.0, LOCK_WAIT).unwrap();

        let stored = scratch.stored();
        let fields = stored[0].as_object().unwrap();
        assert!(!fields.contains_key("remembered"));
        assert!(!fields.contains_key("quiet"));
        assert!(!fields.contains_key("extra"));
        assert_eq!(fields["retired"], json!(1_700_000_000_000_i64));
        let listing = scratch.list();
        assert_eq!(listing.rows.len(), 1);
        assert_eq!(listing.rows[0].to_value(), stored[0]);
        assert_eq!(listing.rows[0].model.as_deref(), Some("provider/m"));
        assert_eq!(listing.rows[0].archived, 3);
    }

    #[test]
    fn a_hundred_and_first_row_is_refused_and_every_row_is_kept() {
        let scratch = Scratch::made("ceiling-write");
        let mut hundred: Vec<Value> = (0..100).map(|n| bot(&format!("b{n:03}"))).collect();
        hundred[0]["retired"] = json!(5);

        replace_in(&scratch.home(), &scratch.data(), &hundred, 1.0, LOCK_WAIT).unwrap();
        let before = scratch.bots_text();
        hundred.push(bot("one-too-many"));
        let refused =
            replace_in(&scratch.home(), &scratch.data(), &hundred, 1.0, LOCK_WAIT).unwrap_err();

        assert!(matches!(refused, Refused::Ceiling), "{refused}");
        assert_eq!(scratch.bots_text(), before);
        assert_eq!(scratch.list().rows.len(), 100);
    }

    #[test]
    fn a_list_with_an_entry_the_rules_drop_writes_nothing() {
        let scratch = Scratch::made("refused-entry");
        for entries in [
            vec![bot("fine"), bot("Bad_Slug")],
            vec![bot("same"), bot("same")],
            vec![json!(null)],
        ] {
            let refused =
                replace_in(&scratch.home(), &scratch.data(), &entries, 1.0, LOCK_WAIT).unwrap_err();
            assert!(matches!(refused, Refused::Entry { .. }), "{refused}");
        }
        assert_eq!(scratch.bots_text(), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_first_write_makes_a_private_directory_and_file() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("private");
        std::fs::create_dir_all(scratch.data()).unwrap();

        replace_in(
            &scratch.home(),
            &scratch.data(),
            &[bot("a")],
            1.0,
            LOCK_WAIT,
        )
        .unwrap();

        let mode = |path: PathBuf| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(scratch.home()), 0o700);
        assert_eq!(mode(scratch.home().join("bots.json")), 0o600);
        assert_eq!(mode(scratch.home().join("bots.json.lock")), 0o600);
    }

    #[test]
    fn a_write_that_waits_past_the_limit_for_the_lock_is_refused_naming_it() {
        let scratch = Scratch::made("held");
        scratch.write_bots(&json!([bot("old")]));
        let before = scratch.bots_text();
        let held = lock(&scratch.home(), Duration::from_secs(1)).unwrap();

        let refused = replace_in(
            &scratch.home(),
            &scratch.data(),
            &[bot("new")],
            1.0,
            Duration::from_millis(100),
        )
        .unwrap_err();
        drop(held);

        match refused {
            Refused::Locked(path) => assert_eq!(path, scratch.home().join("bots.json.lock")),
            other => panic!("not refused for the lock: {other}"),
        }
        assert_eq!(scratch.bots_text(), before);
        replace_in(
            &scratch.home(),
            &scratch.data(),
            &[bot("new")],
            1.0,
            Duration::from_millis(100),
        )
        .expect("the lock was released with the handle");
    }

    #[cfg(unix)]
    #[test]
    fn a_write_that_fails_before_the_rename_leaves_the_file_as_it_was() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::made("interrupted");
        replace_in(
            &scratch.home(),
            &scratch.data(),
            &[bot("old")],
            1.0,
            LOCK_WAIT,
        )
        .unwrap();
        let before = scratch.bots_text();
        let readonly = std::fs::Permissions::from_mode(0o500);
        std::fs::set_permissions(scratch.home(), readonly).unwrap();
        let probe = scratch.home().join("probe");
        if std::fs::write(&probe, "").is_ok() {
            std::fs::set_permissions(scratch.home(), std::fs::Permissions::from_mode(0o700))
                .unwrap();
            eprintln!("SKIPPED (permissions do not bind this user)");
            return;
        }

        let refused = replace_in(
            &scratch.home(),
            &scratch.data(),
            &[bot("new")],
            1.0,
            LOCK_WAIT,
        );
        std::fs::set_permissions(scratch.home(), std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(matches!(refused, Err(Refused::Io(_))), "{refused:?}");
        assert_eq!(scratch.bots_text(), before);
        let leftovers: Vec<_> = std::fs::read_dir(scratch.home())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    fn old_forms() -> Vec<Value> {
        let mut bare = bot("bare");
        bare.as_object_mut().unwrap().remove("home");
        bare.as_object_mut().unwrap().remove("avatar");
        bare.as_object_mut().unwrap().remove("retired");
        bare["conversations"] = json!(["c-1"]);
        bare["session"] = json!("c-1");
        bare["directory"] = json!("/projects/old");
        bare["remembered"] = json!(12);
        bare["quiet"] = json!(1);
        bare["unknown"] = json!({"kept": [1, 2]});
        let mut entries = vec![bare, bot("Bad_Slug"), json!("junk"), bot("fine")];
        entries.extend((0..100).map(|n| bot(&format!("filler{n:03}"))));
        entries
    }

    #[test]
    fn the_rows_in_the_old_file_move_unchanged_and_the_old_file_is_untouched() {
        let scratch = Scratch::made("move");
        let entries = old_forms();
        let old = scratch.write_old(&json!({"layout": null, "bots": entries}).to_string());

        let listing = scratch.list();

        assert_eq!(scratch.stored(), entries);
        assert_eq!(
            std::fs::read(scratch.data().join("bravebot-ui.json")).unwrap(),
            old
        );
        assert_eq!(listing.rows.len(), 100);
        assert_eq!(listing.rows[0].slug, "bare");
        assert_eq!(listing.rows[0].avatar, "bare");
        assert_eq!(
            listing.rows[0].conversations,
            vec![Conversation {
                id: "c-1".into(),
                directory: "/projects/old".into()
            }]
        );
        let reasons: Vec<_> = listing.dropped.iter().map(|each| each.reason).collect();
        assert_eq!(
            reasons,
            vec![
                Reason::BadSlug,
                Reason::NotAnObject,
                Reason::PastCeiling,
                Reason::PastCeiling
            ]
        );
        assert_eq!(listing.dropped[2].slug.as_deref(), Some("filler098"));
    }

    #[test]
    fn a_second_start_does_not_read_the_old_key_again() {
        let first = Scratch::made("move-first");
        let second = Scratch::made("move-second");
        let entries = old_forms();
        let old = json!({"bots": entries}).to_string();
        first.write_old(&old);
        second.write_old(&old);

        let by_a_read = first.list();
        assert_eq!(
            move_old_rows(&second.home(), &second.data()).unwrap(),
            Moved::Entries(entries.len())
        );
        let after_a_move = second.list();
        assert_eq!(first.stored(), second.stored());
        assert_eq!(slugs(&by_a_read), slugs(&after_a_move));
        assert_eq!(by_a_read.dropped, after_a_move.dropped);

        first.write_old(&json!({"bots": [bot("later")]}).to_string());
        assert_eq!(first.list(), by_a_read);
        assert_eq!(first.stored(), second.stored());
        assert_eq!(
            move_old_rows(&first.home(), &first.data()).unwrap(),
            Moved::Nothing
        );
    }

    #[test]
    fn an_existing_bots_file_is_not_moved_over() {
        let scratch = Scratch::made("move-exists");
        scratch.write_old(&json!({"bots": [bot("old")]}).to_string());
        scratch.write_bots(&json!([bot("new")]));

        assert_eq!(slugs(&scratch.list()), ["new"]);
    }

    #[test]
    fn a_move_that_waited_for_the_lock_finds_the_file_another_program_wrote() {
        let scratch = Scratch::made("move-race");
        scratch.write_old(&json!({"bots": [bot("old")]}).to_string());

        let moved = move_with(&scratch.home(), &scratch.data(), LOCK_WAIT, || {
            scratch.write_bots(&json!([bot("written-meanwhile")]));
        })
        .unwrap();

        assert_eq!(moved, Moved::Nothing);
        assert_eq!(slugs(&scratch.list()), ["written-meanwhile"]);
    }

    #[test]
    fn nothing_to_move_leaves_no_bots_file() {
        for (name, old) in [
            ("absent", None),
            ("no-key", Some("{\"layout\": null}")),
            ("empty", Some("{\"bots\": []}")),
        ] {
            let scratch = Scratch::new(&format!("move-none-{name}"));
            std::fs::create_dir_all(scratch.data()).unwrap();
            if let Some(old) = old {
                scratch.write_old(old);
            }

            let listing = scratch.list();

            assert_eq!(listing, Listing::default(), "{name}");
            assert!(!scratch.home().exists(), "{name}");
        }
    }

    #[test]
    fn an_old_file_that_cannot_be_converted_is_named_and_nothing_is_written() {
        for (name, text) in [
            ("truncated", "{\"bots\": [{"),
            ("an-object", "{\"bots\": {\"a\": 1}}"),
            ("null", "{\"bots\": null}"),
        ] {
            let scratch = Scratch::made(&format!("move-bad-{name}"));
            let old = scratch.write_old(text);

            let listing = scratch.list();
            let refused = replace_in(
                &scratch.home(),
                &scratch.data(),
                &[bot("a")],
                1.0,
                LOCK_WAIT,
            )
            .unwrap_err();

            assert!(listing.rows.is_empty(), "{name}");
            assert_eq!(
                listing.unreadable.map(|each| each.path),
                Some(scratch.data().join("bravebot-ui.json")),
                "{name}"
            );
            assert!(matches!(refused, Refused::OldFile(_)), "{name}: {refused}");
            assert_eq!(scratch.bots_text(), None, "{name}");
            assert_eq!(
                std::fs::read(scratch.data().join("bravebot-ui.json")).unwrap(),
                old
            );
        }
    }

    #[test]
    fn nothing_is_moved_where_nothing_may_be_written() {
        let scratch = Scratch::made("move-incognito");
        scratch.write_old(&json!({"bots": [bot("old")]}).to_string());

        let listing = list_in(&scratch.home(), false, &scratch.data(), 1.0);

        assert_eq!(listing, Listing::default());
        assert_eq!(scratch.bots_text(), None);
    }
}
