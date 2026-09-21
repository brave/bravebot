//! Taking this agent's own credentials off a program it runs.
//!
//! Which names those are is [`bravebot_config::scrub`]'s, along with the reason the set is narrow
//! and the switch that turns it off. What is here is the removal itself, for a command this crate
//! is about to start: the `run` tool's stages and a hook. The language server host asks for the
//! names and removes them where it builds its own command.

use std::process::Command;

/// Remove them from `command`.
///
/// Removal rather than an empty value: a program that checks whether a variable is set would read
/// an empty string as configured-but-blank, and `aws` treats an empty profile differently from an
/// absent one. Unset is the state a machine that never held the credential is in, which is the
/// state being reproduced.
pub fn apply(command: &mut Command) {
    for name in bravebot_config::scrub::withheld() {
        command.env_remove(name);
    }
}
