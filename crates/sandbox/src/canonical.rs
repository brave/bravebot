//! Unicode canonical decomposition, for the names a `denyRead` glob is compared with
//! (SANDBOX-25).
//!
//! This crate has no dependency on `bravebot-core`, so it compiles the two files that crate's
//! trust map decomposes names with: the tables Unicode publishes and the code that reads them. The
//! tests of that code are in `crates/core/src/canonical.rs`.

#[path = "../../core/src/canonical/decompose.rs"]
mod decompose;
#[rustfmt::skip]
#[path = "../../core/src/canonical/tables.rs"]
mod tables;

pub(crate) use decompose::{combining_class, decompose};
