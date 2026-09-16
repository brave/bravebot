//! Live rows in the core memory store (`core.jsonl`), read each turn and written by `remember`
//! or `/memory`.
//!
//! One file for every session on the machine, so every load and mutation takes the store lock
//! under the store directory (`.lock`). On Unix that is `flock`; elsewhere callers proceed without it,
//! which is a known cost.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Default segment under the Bravebot state directory (`~/.bravebot/agent_memory`).
pub const DEFAULT_STORE_NAME: &str = "agent_memory";
const CORE_FILE: &str = "core.jsonl";
const LOCK_FILE: &str = ".lock";
pub const MAX_LIVE_CORE_ROWS: usize = 20;

/// Labels the `/memory` panel offers and the remember tool describes.
pub const CORE_MTYPE_FACT: &str = "fact";
pub const CORE_MTYPE_PREFERENCE: &str = "preference";

/// Every core memory type a person may pick in the UI.
pub const CORE_MTYPES: &[&str] = &[CORE_MTYPE_FACT, CORE_MTYPE_PREFERENCE];

/// One live row from `core.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreRow {
    pub id: String,
    pub mtype: String,
    pub text: String,
    pub confidence: f64,
    pub confirmed: bool,
    #[serde(default)]
    pub deleted: bool,
}

impl CoreRow {
    pub fn new(mtype: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            mtype: mtype.into(),
            text: text.into(),
            confidence: 1.0,
            confirmed: true,
            deleted: false,
        }
    }

    pub fn is_live(&self) -> bool {
        !self.deleted
    }
}

/// Why a store operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryError {
    Locked,
    AtCap,
    NoHome,
    Io(String),
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Locked => f.write_str("another session is writing memory"),
            Self::AtCap => f.write_str("core memory is full"),
            Self::NoHome => f.write_str("no home directory to keep memory in"),
            Self::Io(message) => f.write_str(message),
        }
    }
}

/// The directory holding `core.jsonl`, from settings and the state directory.
///
/// `state_home` is usually [`crate::home::directory`]. With no configured override, this is
/// `state_home/agent_memory`.
pub fn resolve_store_directory(
    state_home: &Path,
    configured: Option<&str>,
) -> PathBuf {
    match configured {
        Some(path) => expand_tilde(path),
        None => state_home.join(DEFAULT_STORE_NAME),
    }
}

/// The store directory when settings and a state home are both available.
pub fn store_from_settings(settings: &bravebot_config::Settings) -> Option<PathBuf> {
    crate::home::directory().map(|state| {
        resolve_store_directory(&state, settings.auto_memory_directory())
    })
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return profile_home().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = profile_home() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn profile_home() -> Option<PathBuf> {
    crate::home::PROFILE_VARIABLES
        .iter()
        .find_map(|variable| {
            let value = std::env::var_os(variable)?;
            if value.is_empty() {
                None
            } else {
                Some(PathBuf::from(value))
            }
        })
}

/// Path to `core.jsonl` under the store directory.
pub fn core_path(store: &Path) -> PathBuf {
    store.join(CORE_FILE)
}

/// Read every live row, or an empty list when the file is missing or unreadable.
pub fn load_live_core(store: &Path) -> Vec<CoreRow> {
    with_store_lock(store, || Ok(parse_live_core(store))).unwrap_or_default()
}

/// Append one live row when memory may be written and the cap allows it.
pub fn append_core(store: &Path, row: CoreRow) -> Result<(), MemoryError> {
    with_store_lock(store, || {
        let live = parse_live_core(store);
        if live.len() >= MAX_LIVE_CORE_ROWS {
            return Err(MemoryError::AtCap);
        }
        if crate::home::create_directory(store).is_err() {
            return Err(MemoryError::Io("could not create the memory directory".into()));
        }
        let path = core_path(store);
        let mut line = serde_json::to_string(&row).map_err(|e| MemoryError::Io(e.to_string()))?;
        line.push('\n');
        let mut file = crate::home::append_to_file(&path).map_err(|e| MemoryError::Io(e.to_string()))?;
        file.write_all(line.as_bytes())
            .map_err(|e| MemoryError::Io(e.to_string()))?;
        Ok(())
    })
}

/// Replace the file with exactly these live rows.
pub fn replace_live_core(store: &Path, rows: &[CoreRow]) -> Result<(), MemoryError> {
    if rows.len() > MAX_LIVE_CORE_ROWS {
        return Err(MemoryError::AtCap);
    }
    with_store_lock(store, || {
        if crate::home::create_directory(store).is_err() {
            return Err(MemoryError::Io("could not create the memory directory".into()));
        }
        let mut body = String::new();
        for row in rows {
            let line = serde_json::to_string(row).map_err(|e| MemoryError::Io(e.to_string()))?;
            body.push_str(&line);
            body.push('\n');
        }
        crate::home::write_file(&core_path(store), body.as_bytes())
            .map_err(|e| MemoryError::Io(e.to_string()))?;
        Ok(())
    })
}

fn parse_live_core(store: &Path) -> Vec<CoreRow> {
    let path = core_path(store);
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut by_id: HashMap<String, CoreRow> = HashMap::new();
    for line in contents.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(row) = serde_json::from_str::<CoreRow>(line) {
            if row.deleted {
                by_id.remove(&row.id);
            } else {
                by_id.insert(row.id.clone(), row);
            }
        }
    }
    let mut live: Vec<CoreRow> = by_id.into_values().collect();
    live.sort_by(|left, right| left.id.cmp(&right.id));
    live
}

fn new_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{nanos:x}-{seq}")
}

fn with_store_lock<T, F>(store: &Path, operation: F) -> Result<T, MemoryError>
where
    F: FnOnce() -> Result<T, MemoryError>,
{
    #[cfg(unix)]
    {
        let _ = crate::home::create_directory(store);
        let lock_path = store.join(LOCK_FILE);
        let file = crate::home::append_to_file(&lock_path).map_err(|e| MemoryError::Io(e.to_string()))?;
        exclusive_lock(&file)?;
        let result = operation();
        let _ = unlock(&file);
        return result;
    }
    #[cfg(not(unix))]
    {
        operation()
    }
}

#[cfg(unix)]
fn exclusive_lock(file: &std::fs::File) -> Result<(), MemoryError> {
    use rustix::fs::{fcntl_lock, FlockOperation};
    fcntl_lock(file, FlockOperation::LockExclusive).map_err(|err| MemoryError::Io(err.to_string()))
}

#[cfg(unix)]
fn unlock(file: &std::fs::File) -> Result<(), MemoryError> {
    use rustix::fs::{fcntl_lock, FlockOperation};
    fcntl_lock(file, FlockOperation::Unlock)
        .map_err(|err| MemoryError::Io(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn home_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|held| held.into_inner())
    }

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("bravebot-memory-{name}"))
    }

    #[test]
    fn append_and_load_round_trip() {
        let _guard = home_lock();
        let store = scratch("round-trip");
        let _ = std::fs::remove_dir_all(&store);
        let row = CoreRow::new("fact", "likes tea");
        append_core(&store, row.clone()).expect("append");
        let loaded = load_live_core(&store);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].text, "likes tea");
        let _ = std::fs::remove_dir_all(&store);
    }

    #[test]
    fn replace_rewrites_the_file() {
        let _guard = home_lock();
        let store = scratch("replace");
        let _ = std::fs::remove_dir_all(&store);
        let first = CoreRow::new("fact", "one");
        let second = CoreRow::new("fact", "two");
        replace_live_core(&store, &[first.clone(), second.clone()]).expect("replace");
        let loaded = load_live_core(&store);
        assert_eq!(loaded.len(), 2);
        replace_live_core(&store, &[first]).expect("replace smaller");
        assert_eq!(load_live_core(&store).len(), 1);
        let _ = std::fs::remove_dir_all(&store);
    }

    #[test]
    fn append_refuses_at_cap() {
        let _guard = home_lock();
        let store = scratch("cap");
        let _ = std::fs::remove_dir_all(&store);
        let rows: Vec<CoreRow> = (0..MAX_LIVE_CORE_ROWS)
            .map(|n| CoreRow::new("fact", format!("row {n}")))
            .collect();
        replace_live_core(&store, &rows).expect("fill");
        let err = append_core(&store, CoreRow::new("fact", "one more")).unwrap_err();
        assert_eq!(err, MemoryError::AtCap);
        let _ = std::fs::remove_dir_all(&store);
    }

    #[test]
    fn default_store_is_under_state_home() {
        let state = Path::new("/tmp/bravebot-state");
        assert_eq!(
            resolve_store_directory(state, None),
            PathBuf::from("/tmp/bravebot-state/agent_memory")
        );
    }
}
