//! Whether the programs a session starts may reach the network, and which of them need to.
//!
//! The base grants egress to every confined program, because a profile gates it as a whole and
//! cannot tell an approved push from an exfiltration. A session that closes the network takes it
//! from every stage that does not carry a reason to have it: a toolchain that fetches, a
//! credential scope, or a program whose only use is to talk to
//! one. `docs/specs/sandboxing.md` decides which, and this is that table in code.

use std::path::Path;

/// What a session decided about the network for the programs it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Network {
    /// Every stage keeps the egress the base grants.
    #[default]
    Open,
    /// A stage has no egress unless it carries a reason to.
    Closed,
}

impl Network {
    /// The word a settings file, a flag and a report spell it with.
    pub fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    /// The setting a word names, or `None` for a word that names neither.
    pub fn parse(word: &str) -> Option<Self> {
        match word.trim() {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    /// Whether this is the setting that removes reach.
    pub fn is_closed(self) -> bool {
        self == Self::Closed
    }
}

/// Whether the program a stage resolved to exists to talk to a remote host.
///
/// Keyed on the file, for the reason [`crate::toolchain::Toolchain::of`] is: the file is what the
/// plan resolved and a person read. `git` and `gh` are not here, since the operation their argv
/// names decides it and is read as the remote scope.
pub fn program_talks_to_a_remote(resolved: &Path) -> bool {
    resolved
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "curl" | "ssh"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_is_read_from_exactly_its_two_words() {
        assert_eq!(Network::parse("open"), Some(Network::Open));
        assert_eq!(Network::parse(" closed "), Some(Network::Closed));
        for word in ["", "Closed", "off", "true", "none"] {
            assert_eq!(Network::parse(word), None, "{word:?}");
        }
        for network in [Network::Open, Network::Closed] {
            assert_eq!(Network::parse(network.name()), Some(network));
        }
        assert_eq!(Network::default(), Network::Open);
    }

    #[test]
    fn curl_and_ssh_are_known_by_the_file_and_nothing_else_is() {
        assert!(program_talks_to_a_remote(Path::new("/usr/bin/curl")));
        assert!(program_talks_to_a_remote(Path::new("/usr/bin/ssh")));
        for path in [
            "/usr/bin/cat",
            "/usr/bin/curl-config",
            "/usr/bin/ssh-keygen",
            "/usr/bin/git",
            "/home/a-person/curl/run",
        ] {
            assert!(!program_talks_to_a_remote(Path::new(path)), "{path}");
        }
    }
}
