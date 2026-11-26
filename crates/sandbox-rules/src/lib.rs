//! What a sandbox backend decides before it is written into a platform's syntax.
//!
//! `bravebot-sandbox` confines a process by handing a platform a profile or a command line, and
//! what it hands is built from paths and arguments that may hold anything. The part of that which
//! is a decision, such as how a value is quoted so it cannot be read as syntax, is here, in a crate
//! that depends on nothing, so Verus can prove it (docs/development/verification.md). What the
//! platform does with the result is the platform's, and is not proved.
//!
//! This crate does no I/O and takes no input but the strings it is given.

#![forbid(unsafe_code)]
// A Verus contract on a loop is an attribute on an expression, which rustc takes only behind this
// feature. Verus's toolchain has it, and only Verus sets `verus_keep_ghost`.
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

// Verus reads this crate as it is. A contract is a `verus_spec` attribute behind
// `cfg_attr(verus_keep_ghost, ...)`, which only Verus sets, so a build without Verus compiles none
// of it.
#[cfg(verus_keep_ghost)]
#[allow(
    unused_imports,
    reason = "Verus requires its prelude in the crate root"
)]
use vstd::prelude::*;

pub mod quote;
