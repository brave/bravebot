//! How much a program `run` starts is held to, as a person chose it (SANDBOX-22). The modes and
//! what each decides are in `bravebot_sandbox_rules::mode`, where Verus reads them.

pub use bravebot_sandbox_rules::mode::{Looser, SandboxMode};

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
            ReadOnly.looser_than_on(Strict, false).is_some(),
            "read-only reads the machine"
        );
        assert!(
            Strict.looser_than_on(ReadOnly, false).is_some(),
            "strict writes the session's directories"
        );
    }

    /// On Windows no mode but `off` reads the machine, so `read-only` writes less than `strict`
    /// and reads no more: a `strict` pin keeps it, and a `read-only` pin still refuses `strict`.
    /// The fault it rejects is the Linux ranking applied there, which refuses `read-only` under
    /// `strict`.
    #[test]
    fn on_windows_read_only_is_no_looser_than_strict() {
        use SandboxMode::{Off, ReadOnly, Strict, WorkspaceWrite};
        assert_eq!(ReadOnly.looser_than_on(Strict, true), None);
        assert_eq!(Strict.looser_than_on(ReadOnly, true), Some(Looser::Writes));
        assert_eq!(
            WorkspaceWrite.looser_than_on(Strict, true),
            Some(Looser::Writes)
        );
        assert_eq!(Strict.looser_than_on(WorkspaceWrite, true), None);
        assert_eq!(Off.looser_than_on(WorkspaceWrite, true), Some(Looser::Both));
        assert_eq!(ReadOnly.looser_than_on(Strict, false), Some(Looser::Reads));
    }

    /// A refusal under a pin says which of reading and writing the asked mode does more of, so the
    /// answer for each pair has to be the one the ranks give and not merely "looser". The fault it
    /// rejects is an answer that names one side for every pair, or swaps the two. Ranked as Linux
    /// and macOS do; the Windows ranking is the one above.
    #[test]
    fn a_looser_mode_reads_more_writes_more_or_both() {
        use SandboxMode::{Off, ReadOnly, Strict, WorkspaceWrite};
        for (asked, pinned, looser) in [
            (ReadOnly, Strict, Looser::Reads),
            (WorkspaceWrite, Strict, Looser::Both),
            (Off, Strict, Looser::Both),
            (Strict, ReadOnly, Looser::Writes),
            (WorkspaceWrite, ReadOnly, Looser::Writes),
            (Off, ReadOnly, Looser::Both),
            (Off, WorkspaceWrite, Looser::Both),
        ] {
            assert_eq!(
                asked.looser_than_on(pinned, false),
                Some(looser),
                "{asked} under {pinned}"
            );
        }
        for (asked, pinned) in [(Strict, WorkspaceWrite), (ReadOnly, WorkspaceWrite)] {
            assert_eq!(
                asked.looser_than_on(pinned, false),
                None,
                "{asked} under {pinned}"
            );
        }
        for mode in SandboxMode::ALL {
            assert_eq!(mode.looser_than_on(mode, false), None, "{mode}");
        }
    }

    #[test]
    fn the_default_mode_is_workspace_write() {
        assert_eq!(SandboxMode::default(), SandboxMode::WorkspaceWrite);
    }
}
