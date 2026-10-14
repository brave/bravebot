//! Why a sandbox mode is refused: the sentence the terminal, the command line and the window give
//! for a mode looser than the managed file's pin (SANDBOX-22).
//!
//! The modes are not a line, so the sentence says which of reading and writing the asked mode does
//! more of. It is chosen by the two modes alone, from the `sandbox-looser-` catalog messages, so
//! the three front ends say the same thing in the same language. Nothing here reads a file, a tool
//! result or a network reply.

use bravebot_config::sandbox::Refused;
use bravebot_i18n::t;
use bravebot_sandbox::mode::Looser;

/// The sentence saying which of reading and writing the refused mode does more of than the pin.
pub fn looser(refused: &Refused) -> String {
    let asked = refused.asked.name();
    let pinned = refused.pinned.name();
    match refused.asked.looser_than(refused.pinned) {
        Some(Looser::Reads) => t!(sandbox_looser_reads, asked = asked, pinned = pinned),
        Some(Looser::Writes) => t!(sandbox_looser_writes, asked = asked, pinned = pinned),
        // A mode is refused only where it is looser than the pin, so `None` is not reached.
        Some(Looser::Both) | None => t!(sandbox_looser_both, asked = asked, pinned = pinned),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::sandbox::Floor;
    use bravebot_sandbox::SandboxMode;

    /// SANDBOX-22: each way a mode can be looser than the pin has its own sentence. The regression
    /// it rejects is two outcomes sharing a sentence, which under a `strict` pin leaves `read-only`
    /// refused with no word of how.
    #[test]
    fn each_way_a_mode_is_looser_than_the_pin_has_its_own_sentence() {
        for (asked, pinned, said) in [
            (
                SandboxMode::ReadOnly,
                SandboxMode::Strict,
                "read-only reads more of the machine than strict.",
            ),
            (
                SandboxMode::Strict,
                SandboxMode::ReadOnly,
                "strict writes more than read-only.",
            ),
            (
                SandboxMode::WorkspaceWrite,
                SandboxMode::Strict,
                "workspace-write reads more of the machine and writes more than strict.",
            ),
        ] {
            let refused = Refused {
                asked,
                asked_in: None,
                pinned,
                pinned_in: "/etc/bravebot/managed.json".into(),
                because: Floor::Mode,
            };
            assert_eq!(looser(&refused), said);
        }
    }
}
