//! `git.identities`: the ssh key a confined `git` stage offers a host first, chosen by the owner its
//! address names (SANDBOX-16).
//!
//! An ssh agent holding keys of two accounts offers them in order, and GitHub accepts the first one
//! registered to any account, so with no choice made every push goes out as one account. Which key a
//! program authenticates with is the person's to choose and not a checkout's, so the block is read
//! from the person's own file and from a `--settings` file outside the workspace, and a checkout's
//! files are reported as not obeyed, as `sandbox.mode` is (SANDBOX-22).

use crate::Settings;
use bravebot_sandbox::scope::Address;
use std::path::{Path, PathBuf};

pub const BLOCK: &str = "git";
pub const IDENTITIES_KEY: &str = "identities";

/// One entry: the key the person chose for the repositories of one owner on one host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The host and owner the entry's name spells, `github.com/netzenbot`.
    pub address: Address,
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

    /// The entry for the host and owner of `address`.
    pub fn for_address(&self, address: &Address) -> Option<&Identity> {
        self.entries
            .iter()
            .find(|entry| entry.address.same_as(address))
    }

    /// Fold one root in. `granting` says whether this layer may choose a key.
    ///
    /// A layer that may not is named in `ignored`. An entry whose name is not `<host>/<owner>` or
    /// whose value is not a string is named in `misshapen` and the others are read; a `git` block
    /// or an `identities` value that is not an object is named whole. A later layer's entry for a
    /// host and owner replaces an earlier one's.
    pub(crate) fn absorb(
        &mut self,
        root: &serde_json::Map<String, serde_json::Value>,
        path: Option<&Path>,
        granting: bool,
        ignored: &mut Vec<PathBuf>,
        misshapen: &mut Vec<(PathBuf, String)>,
    ) {
        let by = path.map(Path::to_path_buf);
        let mut note = |name: String| {
            if let Some(path) = &by {
                misshapen.push((path.clone(), name));
            }
        };
        let block = match root.get(BLOCK) {
            None => return,
            Some(serde_json::Value::Object(block)) => block,
            Some(_) => return note(BLOCK.to_string()),
        };
        let entries = match block.get(IDENTITIES_KEY) {
            None => return,
            Some(serde_json::Value::Object(entries)) => entries,
            Some(_) => return note(format!("{BLOCK}.{IDENTITIES_KEY}")),
        };
        if !granting {
            if let Some(path) = &by {
                ignored.push(path.clone());
            }
            return;
        }
        for (name, key) in entries {
            let address = Address::from_entry_name(name);
            let (Some(address), Some(key)) = (address, key.as_str()) else {
                note(format!("{BLOCK}.{IDENTITIES_KEY}.{name}"));
                continue;
            };
            self.entries.retain(|held| !held.address.same_as(&address));
            self.entries.push(Identity {
                address,
                key: key.to_string(),
                by: by.clone(),
            });
        }
    }
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
                    entry.address.host.as_str(),
                    entry.address.owner.as_str(),
                    entry.key.as_str(),
                )
            })
            .collect()
    }

    fn address(host: &str, owner: &str) -> Address {
        Address::spelled(host, owner).unwrap()
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
                .for_address(&address("GitHub.com", "Brave"))
                .map(|entry| entry.key.as_str()),
            Some("~/.ssh/work")
        );
        assert_eq!(
            identities.for_address(&address("github.com", "other")),
            None
        );
        assert_eq!(
            identities.for_address(&address("gitlab.com", "brave")),
            None
        );
    }

    /// An entry whose name is not `<host>/<owner>`, or whose value is not a string, is reported and
    /// the rest are read, and a block or a value of the wrong kind is reported whole. The regression
    /// it rejects is one bad entry dropping the block, which would leave every owner on the default
    /// key with nothing said.
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

        for (text, whole) in [
            (r#"{"git": {"identities": ["x"]}}"#, "git.identities"),
            (r#"{"git": ["github.com/brave", "~/.ssh/a.pub"]}"#, "git"),
        ] {
            let mut misshapen = Vec::new();
            Identities::default().absorb(
                &root_of(text),
                Some(Path::new("/f")),
                true,
                &mut Vec::new(),
                &mut misshapen,
            );
            assert_eq!(misshapen, [(PathBuf::from("/f"), whole.to_string())]);
        }
    }

    /// A key beside `identities` is reported as unread, so a block written for another tool does not
    /// look configured.
    #[test]
    fn a_key_beside_identities_is_reported_as_unread() {
        let root = root_of(r#"{"git": {"identities": {}, "sshCommand": "x"}}"#);
        assert_eq!(unread_inside(&root), ["git.sshCommand"]);
    }
}
