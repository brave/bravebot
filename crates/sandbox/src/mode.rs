//! How much a program `run` starts is held to, as a person chose it (SANDBOX-22).

use std::fmt;

/// The bundle of rows a confined program runs under.
///
/// Ordered by what it lets a program reach, strictest first, so that "no looser than" is the
/// comparison `<=` and a caller never spells the order out a second time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum SandboxMode {
    /// The deny-by-default profile: the base, the list the program's binary brings, and the scope
    /// its argv names. The machine is not read.
    Strict,
    /// The machine except the credential table (SANDBOX-12). What a session runs under unless a
    /// person chose otherwise.
    #[default]
    Standard,
    /// No confinement.
    Off,
}

impl SandboxMode {
    /// The word a person types for this mode, which is also how every report spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Standard => "standard",
            Self::Off => "off",
        }
    }

    /// The mode a word names, or `None`.
    ///
    /// Exact and lower case, as the permission modes are read: a value that is almost a mode is
    /// not obeyed as one, because the three differ in what a program may reach and a guess would
    /// choose for the person.
    pub fn parse(word: &str) -> Option<Self> {
        [Self::Strict, Self::Standard, Self::Off]
            .into_iter()
            .find(|mode| mode.name() == word)
    }

    /// Whether this lets a program reach more than `other` does.
    pub fn is_looser_than(self, other: Self) -> bool {
        self > other
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
        for mode in [SandboxMode::Strict, SandboxMode::Standard, SandboxMode::Off] {
            assert_eq!(SandboxMode::parse(mode.name()), Some(mode));
        }
        for word in [
            "", "Strict", "OFF", " off", "off ", "default", "none", "root",
        ] {
            assert_eq!(SandboxMode::parse(word), None, "{word:?}");
        }
    }

    #[test]
    fn strict_is_the_tightest_and_off_the_loosest() {
        assert!(SandboxMode::Standard.is_looser_than(SandboxMode::Strict));
        assert!(SandboxMode::Off.is_looser_than(SandboxMode::Standard));
        assert!(SandboxMode::Off.is_looser_than(SandboxMode::Strict));
        assert!(!SandboxMode::Strict.is_looser_than(SandboxMode::Standard));
        assert!(!SandboxMode::Standard.is_looser_than(SandboxMode::Standard));
        assert_eq!(SandboxMode::default(), SandboxMode::Standard);
    }
}
