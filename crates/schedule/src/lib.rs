//! Scheduled runs: the entries a person registers, the system scheduler's files for them, and the
//! record of each firing.
//!
//! Nothing here draws or prints. A subcommand returns the lines it would say and how it ended, so
//! the terminal, the line-by-line interface and the desktop bridge show the same words.

#![forbid(unsafe_code)]
