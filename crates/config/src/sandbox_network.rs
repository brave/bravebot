//! The three `sandbox.network` keys that name the hosts a confined program may reach, and which
//! layer each may be read from (SANDBOX-24).
//!
//! `deniedHosts` only takes reach away, so any layer may write it. `allowedHosts` and `onUnlisted`
//! give reach back (a listed host, a question answered "yes"), so they are read from the person's
//! own file, the file `--settings` names outside the workspace, and nothing a clone brings. Not
//! setting `allowedHosts` is no list, and no list is no proxy; a list that is set, even an empty
//! one, filters.

use crate::{Managed, Settings};
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

/// What the three keys came to once the managed file is applied, and what it left unread.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    pub hosts: Hosts,
    /// The keys the managed file wrote, so a report can say a person's own were not read.
    pub pinned: Vec<&'static str>,
    /// The person's own allowed entries a pinned `allowedHosts` left out, for a report.
    pub unread: Vec<HostEntry>,
}

static SETTLED: std::sync::OnceLock<Resolved> = std::sync::OnceLock::new();

/// Read the layers on this machine and settle the host list for the rest of this process.
///
/// Called once from the entry point with the other sandbox settings, so every stage of every
/// line holds one answer. First call wins, for the reason [`crate::settle_run_network`] does.
pub fn settle(settings: &Settings) -> &'static Resolved {
    SETTLED.get_or_init(|| resolve(settings, &Managed::load()))
}

/// The list [`settle`] settled, or none where nothing did, which is every test that does not
/// start from an entry point.
pub fn settled() -> Option<&'static Resolved> {
    SETTLED.get()
}

/// The answer for the settings layers and the managed layer, none read from the machine here.
///
/// A managed `allowedHosts` is the whole allowed set, so a person's entries are not added to it,
/// and a managed `onUnlisted` is the answer whichever the person said. A managed `deniedHosts`
/// is added to theirs and nothing they wrote lifts it.
pub fn resolve(settings: &Settings, managed: &Managed) -> Resolved {
    let mut hosts = settings.sandbox_hosts().clone();
    let pin = managed.sandbox_hosts();
    let mut out = Resolved::default();
    if let Some(allowed) = &pin.allowed {
        out.pinned.push("sandbox.network.allowedHosts");
        out.unread = hosts.allowed.take().unwrap_or_default();
        hosts.allowed = Some(allowed.clone());
    }
    if !pin.denied.is_empty() {
        out.pinned.push("sandbox.network.deniedHosts");
        hosts.denied.extend(pin.denied.iter().cloned());
    }
    if let Some(answer) = pin.on_unlisted {
        out.pinned.push("sandbox.network.onUnlisted");
        hosts.on_unlisted = Some(answer);
    }
    out.hosts = hosts;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spelled(entries: &[HostEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.entry.as_str()).collect()
    }

    fn own() -> Settings {
        Settings::parse(
            r#"{"sandbox": {"network": {"allowedHosts": ["mine.example"], "deniedHosts": ["bad.example"], "onUnlisted": "ask"}}}"#,
        )
    }

    /// Nothing pinned leaves the person's own lists as they are.
    #[test]
    fn no_managed_file_leaves_the_settings_as_they_are() {
        let answer = resolve(&own(), &Managed::default());
        assert_eq!(answer.hosts, own().sandbox_hosts().clone());
        assert!(answer.pinned.is_empty() && answer.unread.is_empty());
    }

    /// A pinned `allowedHosts` is the whole allowed set: the person's entry is not added to it and
    /// is reported unread. The regression it rejects is a union, which lets a person widen what an
    /// administrator narrowed.
    #[test]
    fn a_pinned_allowed_list_replaces_the_persons() {
        let managed = crate::managed::scratch(
            "hosts-pin-allowed",
            r#"{"sandbox": {"network": {"allowedHosts": ["corp.example"]}}}"#,
        );
        let answer = resolve(&own(), &managed);
        assert_eq!(
            spelled(answer.hosts.allowed.as_ref().unwrap()),
            ["corp.example"]
        );
        assert_eq!(spelled(&answer.unread), ["mine.example"]);
        assert_eq!(answer.pinned, ["sandbox.network.allowedHosts"]);
        assert!(
            managed
                .pinned()
                .any(|name| name == "sandbox.network.allowedHosts")
        );
    }

    /// A pinned empty list is a list: it refuses every host and still replaces the person's, and
    /// a person with no list of their own is filtered by it. The regression it rejects is an empty
    /// pin read as no pin.
    #[test]
    fn a_pinned_empty_list_filters_a_session_that_set_none() {
        let managed = crate::managed::scratch(
            "hosts-pin-empty",
            r#"{"sandbox": {"network": {"allowedHosts": []}}}"#,
        );
        let answer = resolve(&Settings::default(), &managed);
        assert_eq!(answer.hosts.allowed, Some(Vec::new()));
    }

    /// A pinned denial is added to the person's and a pinned `onUnlisted` wins over `ask`.
    /// The regression it rejects is the person's `ask` surviving a pinned `refuse`.
    #[test]
    fn a_pinned_denial_is_added_and_a_pinned_answer_wins() {
        let managed = crate::managed::scratch(
            "hosts-pin-deny",
            r#"{"sandbox": {"network": {"deniedHosts": ["pinned.example"], "onUnlisted": "refuse"}}}"#,
        );
        let answer = resolve(&own(), &managed);
        assert_eq!(
            spelled(&answer.hosts.denied),
            ["bad.example", "pinned.example"]
        );
        assert_eq!(answer.hosts.on_unlisted, Some(OnUnlisted::Refuse));
        assert_eq!(
            spelled(answer.hosts.allowed.as_ref().unwrap()),
            ["mine.example"]
        );
    }

    /// A value that is not a list pins nothing and is reported.
    #[test]
    fn a_misshapen_pin_pins_nothing_and_is_reported() {
        let managed = crate::managed::scratch(
            "hosts-pin-misshapen",
            r#"{"sandbox": {"network": {"allowedHosts": "x"}}}"#,
        );
        assert!(resolve(&own(), &managed).pinned.is_empty());
        assert_eq!(
            managed.sandbox_hosts_unreadable().collect::<Vec<_>>(),
            ["allowedHosts"]
        );
    }
}
