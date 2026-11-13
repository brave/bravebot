//! A sign-in kept in one file the account owns.
//!
//! The file sits in the state directory and is private to the account as it is created, written
//! beside and renamed over, and written and removed under a claim of its own. Which file, which claim and what it holds are the [`Kind`]'s.

use super::Kind;
use crate::Secret;
use crate::secret::Document;
use crate::store::{Claim, StoreError, beside, write_through};
use std::path::{Path, PathBuf};

/// How old a claim has to be before it is taken to belong to a process that died holding it.
///
/// Twice the thirty seconds a request for a new token may wait for its reply. A refresh makes one
/// request, or two where another process changed the file after the first was refused, and makes the
/// claim young again before the second, so no request waits under a claim older than its own thirty
/// seconds. Writing the file at a sign-in holds it for a moment. A claim this old is a process that
/// died, or one that was stopped for longer than a request may take, and a holder that was stopped
/// that long can have its claim broken.
pub(crate) const ABANDONED_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

/// Where a file that exists is not what this version writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Defect {
    /// The file holds nothing, which an interrupted write leaves behind.
    Empty,
    /// The file is not JSON.
    NotJson,
    /// The file was written by another version.
    OtherVersion,
    /// A field is missing or empty.
    MissingField,
    /// The expiry is missing or is not a number of seconds.
    NoExpiry,
}

/// Why the sign-in could not be read, written or removed.
///
/// Each is a condition. The words that say what to do about it are the caller's, since this crate
/// holds no message catalog, and a `detail` is a path and what the operating system said, which is
/// text about a file and not about the sign-in.
#[derive(Debug)]
pub enum AccountError {
    /// The environment names no directory to keep a sign-in in, so none is stored and none can be.
    Nowhere { detail: String },
    /// The file exists and could not be read.
    Unusable { detail: String },
    /// The file exists and is not what this version writes.
    Malformed(Defect),
    /// The file could not be written or removed.
    Unwritable { detail: String },
}

/// A short name for the condition, for a log or a test. What a person is told is the caller's.
impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nowhere { .. } => f.write_str("nowhere"),
            Self::Unusable { .. } => f.write_str("unusable"),
            Self::Malformed(defect) => write!(f, "malformed ({defect:?})"),
            Self::Unwritable { .. } => f.write_str("unwritable"),
        }
    }
}

impl std::error::Error for AccountError {}

/// A store failure while writing or removing the file.
fn unwritable(failure: StoreError) -> AccountError {
    let StoreError::Unusable { detail } = failure;
    AccountError::Unwritable { detail }
}

/// Where the sign-in of `K` is kept.
pub(crate) fn path<K: Kind>() -> Result<PathBuf, AccountError> {
    beside(K::FILE).map_err(|failure| {
        let StoreError::Unusable { detail } = failure;
        AccountError::Nowhere { detail }
    })
}

pub(crate) fn load_at<K: Kind>(path: &Path) -> Result<Option<K::Account>, AccountError> {
    // A buffer that clears itself: the text of the file is both tokens, and a read that is
    // answered with an error hands it back as readily as one that succeeds.
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => Secret::new(raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(AccountError::Unusable {
                detail: format!("{}: {e}", path.display()),
            });
        }
    };
    K::decode(raw.expose()).map(Some)
}

/// The parsed text of a file that is not empty, is JSON and was written by `version`.
///
/// The guard clears the copy of each token the parse makes however the caller returns. The parse
/// error is not repeated: it can quote the text it stopped at.
pub(crate) fn versioned(raw: &str, version: u64) -> Result<Document, AccountError> {
    // A file can exist holding nothing, if a write was interrupted partway.
    if raw.trim().is_empty() {
        return Err(AccountError::Malformed(Defect::Empty));
    }
    let document = Document::of(
        serde_json::from_str(raw).map_err(|_| AccountError::Malformed(Defect::NotJson))?,
    );
    if document
        .read()
        .get("version")
        .and_then(serde_json::Value::as_u64)
        != Some(version)
    {
        return Err(AccountError::Malformed(Defect::OtherVersion));
    }
    Ok(document)
}

/// The string field `name` of a file, where it is present and not empty.
pub(crate) fn text(value: &serde_json::Value, name: &str) -> Option<String> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// The instant the access token of a file expires, in seconds since 1970-01-01.
pub(crate) fn expiry(value: &serde_json::Value) -> Result<u64, AccountError> {
    value
        .get("expires_at")
        .and_then(serde_json::Value::as_u64)
        .ok_or(AccountError::Malformed(Defect::NoExpiry))
}

pub(crate) fn save_at<K: Kind>(path: &Path, account: &K::Account) -> Result<(), AccountError> {
    let _claim = claim::<K>(path)?;
    write_held::<K>(path, account)
}

pub(crate) fn forget_at<K: Kind>(path: &Path) -> Result<bool, AccountError> {
    let _claim = claim::<K>(path)?;
    remove_held(path)
}

/// The claim every write and removal of the file is made under. A caller that holds one across a
/// request for new tokens writes with [`write_held`] and removes with [`remove_held`], which take
/// none of their own: a second claim by the process that holds the first would wait on itself.
pub(crate) fn claim<K: Kind>(path: &Path) -> Result<Claim, AccountError> {
    Claim::take_as(path, K::CLAIM, ABANDONED_AFTER).map_err(unwritable)
}

/// Write the file, the caller holding the claim.
pub(crate) fn write_held<K: Kind>(path: &Path, account: &K::Account) -> Result<(), AccountError> {
    use std::io::Write;

    let text = K::encode(account);
    write_through(path, |file| file.write_all(text.expose().as_bytes())).map_err(unwritable)
}

/// Remove the file, the caller holding the claim. Whether there was one is the answer.
pub(crate) fn remove_held(path: &Path) -> Result<bool, AccountError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(AccountError::Unwritable {
            detail: format!("{}: {e}", path.display()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file is read as the version the caller names. A check that held every caller to the
    /// first version would refuse the file a later format writes and read an old one as current.
    #[test]
    fn a_file_is_read_as_the_version_the_caller_names_and_no_other() {
        assert!(versioned("{\"version\": 2}", 2).is_ok());
        assert!(matches!(
            versioned("{\"version\": 1}", 2),
            Err(AccountError::Malformed(Defect::OtherVersion))
        ));
    }
}
