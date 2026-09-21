//! Driving bravebot from a graphical front-end.
//!
//! Everything in this crate is transport-agnostic. It knows how to list and open the
//! sessions the agent already keeps, how to run a turn and report what it is doing, and
//! how to carry an approval question to whoever is watching and an answer back. It does
//! not know what is watching.
//!
//! That is deliberate and it is enforced by two rules, because the whole point of the
//! layout is that the transport stays replaceable:
//!
//! 1. **Nothing below `src/bin/` writes to stdout or exits the process.** The kernel
//!    never prints, for the same reason: a stray `println!` interleaves with the
//!    protocol and there is no way for a caller to stop it.
//! 2. **Events leave through a callback, never a global.** A second front-end has to be
//!    able to install its own.
//!
//! See `docs/phase-0-rpc-protocol.md` for the protocol these calls project onto.

pub mod bridge;
pub mod emit;
pub mod fork;
pub mod models;
pub mod protocol;
pub mod running;
pub mod store;
pub mod settings;
pub mod turn;
pub mod wire;

/// Which build of the agent this bridge is linked against.
///
/// Read from the agent rather than computed here, so a record written by the app and one
/// written by `bravebot` carry the same string with no coordination. A transcript is read
/// after the fact, usually because something went wrong, and the first question is which
/// code produced it.
pub fn agent_build() -> &'static str {
    bravebot_tui::BUILD
}
