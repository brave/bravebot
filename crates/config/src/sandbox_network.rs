//! The three `sandbox.network` keys that name the hosts a confined program may reach, and which
//! layer each may be read from (SANDBOX-24).
//!
//! `deniedHosts` only takes reach away, so any layer may write it. `allowedHosts` and `onUnlisted`
//! give reach back (a listed host, a question answered "yes"), so they are read from the person's
//! own file, the file `--settings` names outside the workspace, and nothing a clone brings. Not
//! setting `allowedHosts` is no list, and no list is no proxy; a list that is set, even an empty
//! one, filters.

use std::path::{Path, PathBuf};

pub const BLOCK: &str = "network";
pub const ALLOWED_KEY: &str = "allowedHosts";
pub const DENIED_KEY: &str = "deniedHosts";
pub const ON_UNLISTED_KEY: &str = "onUnlisted";

/// What a session does with a host no list covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnUnlisted {
    Ask,
    Refuse,
}

/// One entry of a list with the file that wrote it, spelled as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    pub entry: String,
    pub by: Option<PathBuf>,
}

/// What the layers came to for the three keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hosts {
    /// `None` is no list, so no proxy. `Some` of nothing is a list that refuses every host.
    pub allowed: Option<Vec<HostEntry>>,
    pub denied: Vec<HostEntry>,
    /// The last layer entitled to say, if any said.
    pub on_unlisted: Option<OnUnlisted>,
}

impl Hosts {
    pub fn is_empty(&self) -> bool {
        self.allowed.is_none() && self.denied.is_empty() && self.on_unlisted.is_none()
    }

    /// The keys that carry something, as the settings are named in a report.
    pub fn keys(&self) -> Vec<&'static str> {
        [
            (self.allowed.is_some(), "sandbox.network.allowedHosts"),
            (!self.denied.is_empty(), "sandbox.network.deniedHosts"),
            (self.on_unlisted.is_some(), "sandbox.network.onUnlisted"),
        ]
        .into_iter()
        .filter_map(|(set, key)| set.then_some(key))
        .collect()
    }

    /// Fold one root in. `granting` says whether this layer may give reach.
    ///
    /// What was refused or unreadable goes to `ignored` and `misshapen` with the key, so `doctor`
    /// can name the file.
    pub(crate) fn absorb(
        &mut self,
        root: &serde_json::Map<String, serde_json::Value>,
        path: Option<&Path>,
        granting: bool,
        ignored: &mut Vec<(PathBuf, &'static str)>,
        misshapen: &mut Vec<(PathBuf, &'static str)>,
    ) {
        let Some(block) = network_block(root) else {
            return;
        };
        let by = path.map(Path::to_path_buf);
        let note = |into: &mut Vec<(PathBuf, &'static str)>, key| {
            if let Some(path) = &by {
                into.push((path.clone(), key));
            }
        };
        for (key, denial) in [(ALLOWED_KEY, false), (DENIED_KEY, true)] {
            match block.get(key) {
                None => {}
                Some(value) => match entries(value) {
                    None => note(misshapen, key),
                    Some(_) if !denial && !granting => note(ignored, key),
                    Some(found) => {
                        let target = match denial {
                            true => &mut self.denied,
                            false => self.allowed.get_or_insert_with(Vec::new),
                        };
                        target.extend(found.into_iter().map(|entry| HostEntry {
                            entry,
                            by: by.clone(),
                        }));
                    }
                },
            }
        }
        match block.get(ON_UNLISTED_KEY).map(|value| value.as_str()) {
            None => {}
            Some(Some("ask")) if granting => self.on_unlisted = Some(OnUnlisted::Ask),
            Some(Some("ask")) => note(ignored, ON_UNLISTED_KEY),
            // `refuse` takes reach away, so any layer may say it.
            Some(Some("refuse")) => self.on_unlisted = Some(OnUnlisted::Refuse),
            Some(_) => note(misshapen, ON_UNLISTED_KEY),
        }
    }
}

fn network_block(
    root: &serde_json::Map<String, serde_json::Value>,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    match root.get("sandbox")?.as_object()?.get(BLOCK)? {
        serde_json::Value::Object(block) => Some(block),
        _ => None,
    }
}

/// The strings of a list, blank ones left out, or `None` for anything but a list of strings.
fn entries(value: &serde_json::Value) -> Option<Vec<String>> {
    let serde_json::Value::Array(items) = value else {
        return None;
    };
    let mut out = Vec::new();
    for item in items {
        let text = item.as_str()?;
        if !text.trim().is_empty() {
            out.push(text.trim().to_string());
        }
    }
    Some(out)
}

/// The keys of `sandbox.network` this build reads, for the unread-key report.
pub(crate) fn is_read(key: &str) -> bool {
    matches!(key, ALLOWED_KEY | DENIED_KEY | ON_UNLISTED_KEY)
}

pub(crate) fn unread_inside(root: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    match network_block(root) {
        Some(block) => block
            .keys()
            .filter(|key| !is_read(key))
            .map(|key| format!("sandbox.{BLOCK}.{key}"))
            .collect(),
        None => Vec::new(),
    }
}
