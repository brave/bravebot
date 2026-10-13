//! How much a program `run` starts is held to, as a person chose it (SANDBOX-22).

use std::fmt;

/// What `workspace-write` was called before it was renamed.
const RENAMED_WORKSPACE_WRITE: &str = "standard";

/// The bundle of rows a confined program runs under.
///
/// The modes are not a line. `strict` reads less than `read-only` and writes the session's
/// directories, which `read-only` does not, so neither is the stricter of the two. A caller asks
/// [`SandboxMode::is_looser_than`] and never compares two modes by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SandboxMode {
    /// The deny-by-default profile: the base, the list the program's binary brings, and the scope
    /// its argv names. The machine is not read.
    Strict,
    /// The machine except the credential table (SANDBOX-12), read, with nothing in the session's
    /// directories or the toolchain caches written.
    ReadOnly,
    /// The machine except the credential table (SANDBOX-12), with the session's directories
    /// writable. What a session runs under unless a person chose otherwise.
    #[default]
    WorkspaceWrite,
    /// No confinement.
    Off,
}

impl SandboxMode {
    /// Every mode, in the order they are listed to a person.
    pub const ALL: [Self; 4] = [
        Self::Strict,
        Self::ReadOnly,
        Self::WorkspaceWrite,
        Self::Off,
    ];

    /// The word a person types for this mode, which is also how every report spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
            Self::Off => "off",
        }
    }

    /// The mode a word names, or `None`.
    ///
    /// Exact and lower case, as the permission modes are read: a value that is almost a mode is
    /// not obeyed as one, because the modes differ in what a program may reach and a guess would
    /// choose for the person. `standard`, the word `workspace-write` replaced, still names it, so a
    /// settings file or a managed pin written before the rename keeps its meaning.
    pub fn parse(word: &str) -> Option<Self> {
        if word == RENAMED_WORKSPACE_WRITE {
            return Some(Self::WorkspaceWrite);
        }
        Self::ALL.into_iter().find(|mode| mode.name() == word)
    }

    /// Whether a program under this mode can read or write more than under `other`.
    ///
    /// `strict` and `read-only` are each looser than the other: `read-only` reads the machine,
    /// and `strict` writes the session's directories. A managed pin of one therefore refuses the
    /// other.
    pub fn is_looser_than(self, other: Self) -> bool {
        self.reads() > other.reads() || self.writes() > other.writes()
    }

    /// How much of the machine a program can read, as a rank.
    fn reads(self) -> u8 {
        match self {
            Self::Strict => 0,
            Self::ReadOnly | Self::WorkspaceWrite => 1,
            Self::Off => 2,
        }
    }

    /// How much a program can write, as a rank.
    fn writes(self) -> u8 {
        match self {
            Self::ReadOnly => 0,
            Self::Strict => 1,
            Self::WorkspaceWrite => 2,
            Self::Off => 3,
        }
    }
}

impl fmt::Display for SandboxMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_is_read_only_from_its_exact_word() {
        for mode in SandboxMode::ALL {
            assert_eq!(SandboxMode::parse(mode.name()), Some(mode));
        }
        for word in [
            "",
            "Strict",
            "OFF",
            " off",
            "off ",
            "Standard",
            "Read-Only",
            "read_only",
            "readonly",
            "workspace_write",
            "default",
            "none",
            "root",
        ] {
            assert_eq!(SandboxMode::parse(word), None, "{word:?}");
        }
    }

    #[test]
    fn the_word_workspace_write_replaced_still_names_it() {
        assert_eq!(
            SandboxMode::parse("standard"),
            Some(SandboxMode::WorkspaceWrite)
        );
        assert_eq!(SandboxMode::WorkspaceWrite.name(), "workspace-write");
    }

    #[test]
    fn off_is_the_loosest_and_workspace_write_writes_more_than_read_only() {
        use SandboxMode::{Off, ReadOnly, Strict, WorkspaceWrite};
        for mode in [Strict, ReadOnly, WorkspaceWrite] {
            assert!(Off.is_looser_than(mode), "{mode}");
            assert!(!mode.is_looser_than(Off), "{mode}");
        }
        assert!(WorkspaceWrite.is_looser_than(ReadOnly));
        assert!(!ReadOnly.is_looser_than(WorkspaceWrite));
        assert!(WorkspaceWrite.is_looser_than(Strict));
        assert!(!Strict.is_looser_than(WorkspaceWrite));
        for mode in SandboxMode::ALL {
            assert!(!mode.is_looser_than(mode), "{mode}");
        }
    }

    #[test]
    fn strict_and_read_only_are_each_looser_than_the_other() {
        use SandboxMode::{ReadOnly, Strict};
        assert!(
            ReadOnly.is_looser_than(Strict),
            "read-only reads the machine"
        );
        assert!(
            Strict.is_looser_than(ReadOnly),
            "strict writes the session's directories"
        );
    }

    #[test]
    fn the_default_mode_is_workspace_write() {
        assert_eq!(SandboxMode::default(), SandboxMode::WorkspaceWrite);
    }
}
