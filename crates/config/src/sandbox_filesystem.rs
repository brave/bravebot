//! The four lists that move what a confined program reads and writes, and who decided each.
//!
//! Three things write them: the command line (`--sandbox-allow-read` and the three like it), the
//! settings layers ([`Settings::sandbox_filesystem`], which already keeps a checkout's addition of
//! reach out), and the managed file. A list the managed file wrote is pinned: for `allowRead` and
//! `allowWrite` it is the whole list and nothing a person wrote is added to it, and for a refusal
//! it is added to theirs and nothing they wrote lifts it. This module settles that into one
//! [`Lists`] and leaves the paths to [`bravebot_sandbox::rules::resolve`], which needs the session.

use crate::{Managed, Settings};
use bravebot_sandbox::rules::{Entry, List, Lists};
use std::path::PathBuf;
use std::sync::OnceLock;

/// What the four lists came to, and which of them a file pinned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filesystem {
    pub lists: Lists,
    /// The lists the managed file wrote, so a report can say a person's own entries were not read.
    pub pinned: Vec<List>,
    /// The person's own entries a pinned list left out, for a report.
    pub unread: Vec<(List, Entry)>,
}

/// The answer for flags, the settings layers and the managed layer, none read from the machine here.
pub fn resolve(flags: &Lists, settings: &Settings, managed: &Managed) -> Filesystem {
    let mut out = Filesystem::default();
    let own = settings.sandbox_filesystem();
    for list in crate::settings::FILESYSTEM_LISTS {
        let mut entries: Vec<Entry> = own.of(list).iter().chain(flags.of(list)).cloned().collect();
        let pin = managed.filesystem(list).map(|pinned| {
            pinned
                .iter()
                .map(|path| Entry {
                    path: path.clone(),
                    by: managed.path().map(PathBuf::from),
                    // A refusal the managed file wrote cannot be lifted by an entry beneath it. A
                    // pinned allow is the administrator's own row.
                    pinned: list.is_a_denial(),
                })
                .collect::<Vec<_>>()
        });
        if let Some(pinned) = pin {
            out.pinned.push(list);
            if list.is_a_denial() {
                entries.extend(pinned);
            } else {
                out.unread
                    .extend(entries.drain(..).map(|entry| (list, entry)));
                entries = pinned;
            }
        }
        let target = match list {
            List::AllowRead => &mut out.lists.allow_read,
            List::DenyRead => &mut out.lists.deny_read,
            List::AllowWrite => &mut out.lists.allow_write,
            List::DenyWrite => &mut out.lists.deny_write,
        };
        *target = entries;
    }
    out
}

static SETTLED: OnceLock<Filesystem> = OnceLock::new();

/// Read the layers on this machine and settle the lists for the rest of this process.
///
/// Called once, from the entry point, after the settings file the command line named has been
/// registered and before a session is assembled. First call wins, for the reason
/// [`crate::settle_run_network`] does.
pub fn settle_sandbox_filesystem(flags: &Lists) -> &'static Filesystem {
    SETTLED.get_or_init(|| resolve(flags, &Settings::load(), &Managed::load()))
}

/// As [`settle_sandbox_filesystem`], for an entry point that reads its settings layers itself.
///
/// The desktop bridge answers a settings file it was started on without registering it for the
/// whole process, because a window can choose another file later and the process-wide one would
/// then disagree with the bridge's own reads. It has no flags, so what it settles is the layers and
/// the managed file.
pub fn settle_sandbox_filesystem_in(flags: &Lists, settings: &Settings) -> &'static Filesystem {
    SETTLED.get_or_init(|| resolve(flags, settings, &Managed::load()))
}

/// The lists [`settle_sandbox_filesystem`] settled, or none where nothing did, which is every test
/// that does not start from the entry point.
pub fn sandbox_filesystem() -> Lists {
    SETTLED
        .get()
        .map(|settled| settled.lists.clone())
        .unwrap_or_default()
}

/// The settled answer with what was pinned, for a report.
pub fn settled_sandbox_filesystem() -> Option<&'static Filesystem> {
    SETTLED.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> Entry {
        Entry {
            path: path.to_string(),
            by: None,
            pinned: false,
        }
    }

    fn managed(name: &str, text: &str) -> Managed {
        crate::managed::scratch(name, text)
    }

    fn paths(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.path.as_str()).collect()
    }

    /// Nobody writing a list leaves every list empty, so a person who never sets one has the
    /// profile they had before the lists existed.
    #[test]
    fn nobody_deciding_leaves_every_list_empty() {
        let answer = resolve(&Lists::default(), &Settings::default(), &Managed::default());
        assert!(answer.lists.is_empty());
        assert!(answer.pinned.is_empty());
    }

    /// A flag is the person's own act for this run, so it adds to what their settings say, in
    /// every list, rather than replacing it.
    #[test]
    fn a_flag_adds_to_the_settings() {
        let settings = Settings::parse(
            r#"{"sandbox": {"filesystem": {"denyRead": ["~/a"], "allowWrite": ["~/w"]}}}"#,
        );
        let flags = Lists {
            deny_read: vec![entry("~/b")],
            allow_read: vec![entry("~/c")],
            ..Lists::default()
        };
        let answer = resolve(&flags, &settings, &Managed::default());
        assert_eq!(paths(&answer.lists.deny_read), ["~/a", "~/b"]);
        assert_eq!(paths(&answer.lists.allow_read), ["~/c"]);
        assert_eq!(paths(&answer.lists.allow_write), ["~/w"]);
    }

    /// A pinned `allowRead` is the whole list, so a looser entry of the person's is not read, and it
    /// is reported as one that was not. The control is the unpinned list beside it, which keeps the
    /// person's entry: without it the first half could be a list that was dropped for another reason.
    #[test]
    fn a_pinned_allow_list_replaces_the_persons() {
        let settings = Settings::parse(
            r#"{"sandbox": {"filesystem": {"allowRead": ["~/loose"], "allowWrite": ["~/w"]}}}"#,
        );
        let managed = managed(
            "sandbox-pin-allow",
            r#"{"sandbox": {"filesystem": {"allowRead": ["/opt/approved"]}}}"#,
        );
        let answer = resolve(&Lists::default(), &settings, &managed);
        assert_eq!(paths(&answer.lists.allow_read), ["/opt/approved"]);
        assert_eq!(paths(&answer.lists.allow_write), ["~/w"]);
        assert_eq!(answer.pinned, vec![List::AllowRead]);
        assert_eq!(answer.unread.len(), 1);
        assert_eq!(answer.unread[0].1.path, "~/loose");
    }

    /// A pinned refusal is added to the person's and marked, so the resolution can refuse an entry
    /// beneath it; a pinned empty list is still a pin and reads nothing of the person's.
    #[test]
    fn a_pinned_refusal_is_added_to_the_persons_and_marked() {
        let settings = Settings::parse(r#"{"sandbox": {"filesystem": {"denyRead": ["~/mine"]}}}"#);
        let managed = managed(
            "sandbox-pin-deny",
            r#"{"sandbox": {"filesystem": {"denyRead": ["/etc/secret"], "allowWrite": []}}}"#,
        );
        let answer = resolve(
            &Lists {
                allow_write: vec![entry("~/w")],
                ..Lists::default()
            },
            &settings,
            &managed,
        );
        assert_eq!(paths(&answer.lists.deny_read), ["~/mine", "/etc/secret"]);
        assert!(!answer.lists.deny_read[0].pinned);
        assert!(answer.lists.deny_read[1].pinned);
        assert!(answer.lists.allow_write.is_empty());
    }
}
