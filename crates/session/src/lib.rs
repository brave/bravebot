//! What a session leaves on disk: the record, the state directory, and the audit trail.
//!
//! None of this is presentation. A record is read back by whatever is resuming it, and a front end
//! is not needed to read one — so this crate draws nothing and links no terminal library. The
//! record holds what the planner could have held, which is why the types here are its own rather
//! than an interface's: a struct on disk outlives the shape of a struct in memory.

#![forbid(unsafe_code)]

#[cfg(test)]
mod testutil;

pub mod audit;
pub mod sessions;
pub mod store;
