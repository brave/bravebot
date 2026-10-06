//! A session that loads none of a person's own customizations.
//!
//! When a session misbehaves, the first question is whether the cause is the configuration a person
//! built up around it: a hook that fails, a skill that steers the planner, a definition that changes
//! what a delegate is, a server that offers a tool, a standing instruction that says something
//! nobody remembers writing. Engaging this answers it in one switch. The session starts without
//! reading any of them, and everything else stays as it was: the credentials, the model, the
//! permission rules, the trust map and credential protection all apply as they ordinarily do.
//!
//! # Only removes
//!
//! Nothing here adds to what the planner may do, and nothing relaxes a rule. Each loader that
//! consults [`engaged`] returns what it would have returned had the person written nothing, so the
//! mode can only narrow a session. That is also why it needs no check against
//! `--dangerously-skip-permissions`: the two say different things, and neither widens the other.
//!
//! # One way
//!
//! There is no way to turn it off, for the reason [`crate::incognito`] has none: a loader that
//! consulted a flag which could be cleared would have to reason about when it was cleared, and a
//! single missed ordering would load the thing the mode exists to leave out.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether this process was asked to load none of the person's own customizations.
static ENGAGED: AtomicBool = AtomicBool::new(false);

/// Ask that no hook, skill, definition, MCP server declaration or `AGENTS.md` be read for the rest
/// of this process.
///
/// Called once, from the entry point, before any session is assembled.
pub fn engage() {
    ENGAGED.store(true, Ordering::Release);
}

/// Whether this session skips the person's own customizations.
pub fn engaged() -> bool {
    ENGAGED.load(Ordering::Acquire)
}
