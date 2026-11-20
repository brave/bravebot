//! Where a plugin was installed from (PLUGIN-3, PLUGIN-5, PLUGIN-14).
//!
//! A [`Source`] is what the install record and the install prompt name as the origin of a plugin.
//! It is built from the person's typed text or from a path the driver canonicalised, never from a
//! byte of a staged tree.

use std::fmt;
use std::path::PathBuf;

/// The origin of an installed plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A directory on this machine. Once staged it is the canonical absolute path, and before
    /// that it is the path as typed.
    Local(PathBuf),
}

impl fmt::Display for Source {
    /// The source as the record and the listings write it.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Local(path) => write!(formatter, "{}", path.display()),
        }
    }
}
