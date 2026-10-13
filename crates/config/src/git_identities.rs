//! `git.identities`: the ssh key a confined `git` stage offers a host, chosen by the owner its
//! address names (SANDBOX-16).
//!
//! An ssh agent holding keys of two accounts offers them in order, and GitHub accepts the first one
//! registered to any account, so with no choice made every push goes out as one account. Which key a
//! program authenticates with is the person's to choose and not a checkout's, so the block is read
//! from the person's own file and from a `--settings` file outside the workspace, and a checkout's
//! files are reported as not obeyed, as `sandbox.mode` is (SANDBOX-22).

use crate::Settings;
use std::path::{Path, PathBuf};

pub const BLOCK: &str = "git";
pub const IDENTITIES_KEY: &str = "identities";

/// One entry: the key the person chose for the repositories of one owner on one host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The host as an ssh address names it, `github.com`.
    pub host: String,
    /// The account or organisation whose name is the first part of a repository's path.
    pub owner: String,
    /// The key as the person wrote it, a path judged when a stage is started.
    pub key: String,
    /// The file that wrote it.
    pub by: Option<PathBuf>,
}

/// What the layers came to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identities {
    pub entries: Vec<Identity>,
}

impl Identities {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry for `host` and `owner`, both compared without regard to ASCII case, as a host name
    /// and a GitHub account are.
    pub fn for_address(&self, host: &str, owner: &str) -> Option<&Identity> {
        self.entries.iter().find(|entry| {
            entry.host.eq_ignore_ascii_case(host) && entry.owner.eq_ignore_ascii_case(owner)
        })
    }

    /// Fold one root in. `granting` says whether this layer may choose a key.
    ///
    /// A layer that may not is named in `ignored`. An entry whose name is not `<host>/<owner>` or
    /// whose value is not a string is named in `misshapen` and the others are read; a block that is
    /// not an object is named whole. A later layer's entry for a host and owner replaces an
    /// earlier one's.
    pub(crate) fn absorb(
        &mut self,
        root: &serde_json::Map<String, serde_json::Value>,
        path: Option<&Path>,
        granting: bool,
        ignored: &mut Vec<PathBuf>,
        misshapen: &mut Vec<(PathBuf, String)>,
    ) {
        let Some(value) = root
            .get(BLOCK)
            .and_then(serde_json::Value::as_object)
            .and_then(|block| block.get(IDENTITIES_KEY))
        else {
            return;
        };
        let by = path.map(Path::to_path_buf);
        let mut note = |name: String| {
            if let Some(path) = &by {
                misshapen.push((path.clone(), name));
            }
        };
        let serde_json::Value::Object(entries) = value else {
            note(format!("{BLOCK}.{IDENTITIES_KEY}"));
            return;
        };
        if !granting {
            if let Some(path) = &by {
                ignored.push(path.clone());
            }
            return;
        }
        for (name, key) in entries {
            let (Some((host, owner)), Some(key)) = (address(name), key.as_str()) else {
                note(format!("{BLOCK}.{IDENTITIES_KEY}.{name}"));
                continue;
            };
            let entry = Identity {
                host: host.to_string(),
                owner: owner.to_string(),
                key: key.to_string(),
                by: by.clone(),
            };
            self.entries.retain(|held| {
                !(held.host.eq_ignore_ascii_case(host) && held.owner.eq_ignore_ascii_case(owner))
            });
            self.entries.push(entry);
        }
    }
}

/// The host and owner an entry's name spells, `<host>/<owner>`, or `None`.
fn address(name: &str) -> Option<(&str, &str)> {
    let (host, owner) = name.split_once('/')?;
    let host_spelled = !host.is_empty()
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'));
    let owner_spelled = !owner.is_empty()
        && owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    (host_spelled && owner_spelled).then_some((host, owner))
}

/// The keys of `git` other than `identities`, for the unread-key report.
pub(crate) fn unread_inside(root: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    match root.get(BLOCK) {
        Some(serde_json::Value::Object(block)) => block
            .keys()
            .filter(|key| key.as_str() != IDENTITIES_KEY)
            .map(|key| format!("{BLOCK}.{key}"))
            .collect(),
        _ => Vec::new(),
    }
}

static SETTLED: std::sync::OnceLock<Identities> = std::sync::OnceLock::new();

/// Settle the entries for the rest of this process, from the layers on this machine.
///
/// Called once from the entry point with the other sandbox settings, so every stage of every line
/// holds one answer. First call wins.
pub fn settle(settings: &Settings) -> &'static Identities {
    SETTLED.get_or_init(|| settings.git_identities().clone())
}

/// The entries [`settle`] settled, or none where nothing did, which is every test that does not
/// start from an entry point.
pub fn settled() -> Option<&'static Identities> {
    SETTLED.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_of(text: &str) -> serde_json::Map<String, serde_json::Value> {
        serde_json::from_str(text).unwrap()
    }

    fn entries(identities: &Identities) -> Vec<(&str, &str, &str)> {
        identities
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.host.as_str(),
                    entry.owner.as_str(),
                    entry.key.as_str(),
                )
            })
            .collect()
    }

    /// One root's entries are read by host and owner, and found without regard to case. The
    /// regression it rejects is an exact comparison, which leaves `github.com/Brave` on whichever key
    /// ssh offers first for a person who wrote `brave`.
    #[test]
    fn an_entry_is_read_by_host_and_owner_and_found_without_regard_to_case() {
        let settings = Settings::parse(
            r#"{"git": {"identities": {"github.com/netzenbot": "~/.ssh/netzenbot.pub", "github.com/brave": "~/.ssh/work"}}}"#,
        );
        let identities = settings.git_identities();
        assert_eq!(
            entries(identities),
            [
                ("github.com", "brave", "~/.ssh/work"),
                ("github.com", "netzenbot", "~/.ssh/netzenbot.pub"),
            ]
        );
        assert_eq!(
            identities
                .for_address("GitHub.com", "Brave")
                .map(|entry| entry.key.as_str()),
            Some("~/.ssh/work")
        );
        assert_eq!(identities.for_address("github.com", "other"), None);
        assert_eq!(identities.for_address("gitlab.com", "brave"), None);
    }

    /// An entry whose name is not `<host>/<owner>`, or whose value is not a string, is reported and
    /// the rest are read. The regression it rejects is one bad entry dropping the block, which would
    /// leave every owner on the default key with nothing said.
    #[test]
    fn a_misshapen_entry_is_reported_and_the_rest_are_read() {
        let root = root_of(
            r#"{"git": {"identities": {"github.com": "~/.ssh/a.pub", "github.com/a/b": "~/.ssh/b.pub", "/brave": "~/.ssh/c.pub", "github.com/brave": 7, "github.com/kept": "~/.ssh/kept.pub"}}}"#,
        );
        let mut identities = Identities::default();
        let (mut ignored, mut misshapen) = (Vec::new(), Vec::new());
        identities.absorb(
            &root,
            Some(Path::new("/home/person/.bravebot/settings.json")),
            true,
            &mut ignored,
            &mut misshapen,
        );
        assert_eq!(
            entries(&identities),
            [("github.com", "kept", "~/.ssh/kept.pub")]
        );
        let mut named: Vec<&str> = misshapen.iter().map(|(_, name)| name.as_str()).collect();
        named.sort();
        assert_eq!(
            named,
            [
                "git.identities./brave",
                "git.identities.github.com",
                "git.identities.github.com/a/b",
                "git.identities.github.com/brave",
            ]
        );
        assert!(ignored.is_empty());

        let whole = root_of(r#"{"git": {"identities": ["x"]}}"#);
        let mut misshapen = Vec::new();
        Identities::default().absorb(
            &whole,
            Some(Path::new("/f")),
            true,
            &mut Vec::new(),
            &mut misshapen,
        );
        assert_eq!(
            misshapen,
            [(PathBuf::from("/f"), "git.identities".to_string())]
        );
    }

    /// A key beside `identities` is reported as unread, so a block written for another tool does not
    /// look configured.
    #[test]
    fn a_key_beside_identities_is_reported_as_unread() {
        let root = root_of(r#"{"git": {"identities": {}, "sshCommand": "x"}}"#);
        assert_eq!(unread_inside(&root), ["git.sshCommand"]);
    }
}
