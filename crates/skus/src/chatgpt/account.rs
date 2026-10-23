//! The sign-in kept in one file the account owns
//! ([CHATGPT-6](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-6)).
//!
//! `chatgpt-account.json` sits beside the Leo credentials in the state directory and is made the
//! way that file is: private to the account as it is created, written beside and renamed over, and
//! written and removed under a claim of its own. It holds the two tokens, the instant the access
//! token expires, the client OpenAI issued and the host identifier. It does not hold the ID token.

use crate::Secret;
use crate::secret::Document;
use crate::store::{Claim, StoreError, beside, write_through};
use std::path::{Path, PathBuf};

/// The file, in the state directory.
const FILE: &str = "chatgpt-account.json";

/// The claim taken while the file is written or removed.
const CLAIM: &str = "chatgpt-account.lock";

/// How old a claim has to be before it is taken to belong to a process that died holding it.
///
/// Longer than the thirty seconds a request for a new token may wait for its reply
/// ([CHATGPT-7](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-7)), so a holder still waiting
/// is never broken. Writing the file at a sign-in holds it for a moment, and a claim this old is a
/// process that died.
const ABANDONED_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

/// A signed-in ChatGPT account.
///
/// No equality, which the tokens it holds do not have: see [`Secret`].
#[derive(Debug, Clone)]
pub struct Account {
    /// What a request to the serving host carries.
    pub access_token: Secret,
    /// What mints another access token. Long-lived: it ends only when the person signs out at
    /// OpenAI or removes this program's access there.
    pub refresh_token: Secret,
    /// The instant the access token expires, in seconds since 1970-01-01.
    pub expires_at: u64,
    /// The client OpenAI issued for this program.
    pub client_id: String,
    /// The host identifier this program generated at its first sign-in.
    pub host_id: String,
}

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

/// A store failure while writing or removing the file, which the store words for the Leo file.
fn unwritable(failure: StoreError) -> AccountError {
    match failure {
        StoreError::Unusable { detail } | StoreError::Malformed { detail } => {
            AccountError::Unwritable { detail }
        }
        other => AccountError::Unwritable {
            detail: other.to_string(),
        },
    }
}

/// Where the sign-in is kept.
pub fn path() -> Result<PathBuf, AccountError> {
    beside(FILE).map_err(|failure| match failure {
        StoreError::Unusable { detail } => AccountError::Nowhere { detail },
        other => AccountError::Nowhere {
            detail: other.to_string(),
        },
    })
}

impl Account {
    /// The stored sign-in, or `None` where nothing is stored.
    ///
    /// A file that cannot be read, is empty, or holds the fields of another version is an error
    /// that names the remedy, and is not read as an absent sign-in.
    pub fn load() -> Result<Option<Self>, AccountError> {
        load_at(&path()?)
    }

    /// Store this as the sign-in, replacing what was there.
    pub fn save(&self) -> Result<(), AccountError> {
        save_at(&path()?, self)
    }

    /// Forget the stored sign-in. Whether there was one is the answer; none is not a failure.
    ///
    /// This removes the copy here and ends nothing at OpenAI.
    pub fn forget() -> Result<bool, AccountError> {
        forget_at(&path()?)
    }

    /// The instant the access token expires, as `2026-10-22T21:05:27Z`.
    pub fn expires(&self) -> String {
        super::encoding::utc(self.expires_at)
    }
}

pub(super) fn load_at(path: &Path) -> Result<Option<Account>, AccountError> {
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
    decode(raw.expose()).map(Some)
}

pub(super) fn save_at(path: &Path, account: &Account) -> Result<(), AccountError> {
    let _claim = claim(path)?;
    write_held(path, account)
}

pub(super) fn forget_at(path: &Path) -> Result<bool, AccountError> {
    let _claim = claim(path)?;
    remove_held(path)
}

/// The claim every write and removal of the file is made under. A caller that holds one across a
/// request for new tokens writes with [`write_held`] and removes with [`remove_held`], which take
/// none of their own: a second claim by the process that holds the first would wait on itself.
pub(super) fn claim(path: &Path) -> Result<Claim, AccountError> {
    Claim::take_as(path, CLAIM, ABANDONED_AFTER).map_err(unwritable)
}

/// Write the file, the caller holding the claim.
pub(super) fn write_held(path: &Path, account: &Account) -> Result<(), AccountError> {
    use std::io::Write;

    let text = encode(account);
    write_through(path, |file| file.write_all(text.expose().as_bytes())).map_err(unwritable)
}

/// Remove the file, the caller holding the claim. Whether there was one is the answer.
pub(super) fn remove_held(path: &Path) -> Result<bool, AccountError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(AccountError::Unwritable {
            detail: format!("{}: {e}", path.display()),
        }),
    }
}

fn encode(account: &Account) -> Secret {
    // Built inside a guard, and the text it serialises to is a secret of its own: both hold both
    // tokens, and the file the caller writes is the only copy meant to outlive this call.
    let document = Document::of(serde_json::json!({
        "version": 1,
        "access_token": account.access_token.expose(),
        "refresh_token": account.refresh_token.expose(),
        "expires_at": account.expires_at,
        "client_id": account.client_id,
        "host_id": account.host_id,
    }));
    Secret::new(document.read().to_string())
}

fn decode(raw: &str) -> Result<Account, AccountError> {
    // A file can exist holding nothing, if a write was interrupted partway.
    if raw.trim().is_empty() {
        return Err(AccountError::Malformed(Defect::Empty));
    }
    // Parsed into a guard, so the copy of each token the parse makes is cleared however this
    // returns. The parse error is not repeated: it can quote the text it stopped at.
    let document = Document::of(
        serde_json::from_str(raw).map_err(|_| AccountError::Malformed(Defect::NotJson))?,
    );
    let value = document.read();

    if value.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err(AccountError::Malformed(Defect::OtherVersion));
    }
    let text = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let (Some(access), Some(refresh), Some(client_id), Some(host_id)) = (
        text("access_token"),
        text("refresh_token"),
        text("client_id"),
        text("host_id"),
    ) else {
        return Err(AccountError::Malformed(Defect::MissingField));
    };
    let Some(expires_at) = value.get("expires_at").and_then(serde_json::Value::as_u64) else {
        return Err(AccountError::Malformed(Defect::NoExpiry));
    };
    Ok(Account {
        access_token: Secret::new(access),
        refresh_token: Secret::new(refresh),
        expires_at,
        client_id,
        host_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> Account {
        Account {
            access_token: Secret::new("an-access-token"),
            refresh_token: Secret::new("a-refresh-token"),
            expires_at: 1_792_703_127,
            client_id: "issued-client".to_string(),
            host_id: "urn:uuid:a-host".to_string(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let directory = crate::testutil::scratch_dir(&format!("bravebot-chatgpt-account-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        directory.join(FILE)
    }

    /// What was stored is what is read back, field for field. A field written under another name,
    /// or an expiry read as a string, would load as another sign-in or none.
    #[test]
    fn a_stored_sign_in_is_read_back_field_for_field() {
        let path = scratch("round-trip");

        save_at(&path, &account()).expect("a write");
        let loaded = load_at(&path).expect("a read").expect("a sign-in");

        assert_eq!(loaded.access_token.expose(), "an-access-token");
        assert_eq!(loaded.refresh_token.expose(), "a-refresh-token");
        assert_eq!(loaded.expires_at, 1_792_703_127);
        assert_eq!(loaded.client_id, "issued-client");
        assert_eq!(loaded.host_id, "urn:uuid:a-host");
    }

    /// The file holds the fields CHATGPT-6 names and no others: not the ID token, and no claim of
    /// one. A file that kept the whole reply would hold a value the clause says is never stored.
    #[test]
    fn the_file_holds_the_named_fields_and_nothing_else() {
        let path = scratch("fields");
        save_at(&path, &account()).expect("a write");

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("the file")).expect("JSON");
        let mut names: Vec<&str> = written
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        names.sort_unstable();

        assert_eq!(
            names,
            [
                "access_token",
                "client_id",
                "expires_at",
                "host_id",
                "refresh_token",
                "version"
            ]
        );
    }

    /// Nothing stored is nothing, and is not an error: a person who never signed in is not told
    /// theirs is unusable.
    #[test]
    fn no_file_is_no_sign_in() {
        let path = scratch("absent");
        assert!(load_at(&path).expect("a read").is_none());
    }

    /// A file that cannot be used is an error that says what is wrong with it, and is never read as
    /// an absent sign-in. Reading it as absent would let the next sign-in overwrite what a person
    /// may want to look at, and `auth status` say "not signed in" over a file that is there.
    #[test]
    fn a_file_that_cannot_be_used_is_an_error_naming_its_defect_and_not_an_absent_sign_in() {
        let path = scratch("unusable");
        for (contents, defect) in [
            ("", Defect::Empty),
            ("   \n", Defect::Empty),
            ("not json {", Defect::NotJson),
            ("{\"version\": 2}", Defect::OtherVersion),
            ("{}", Defect::OtherVersion),
            (
                "{\"version\":1,\"access_token\":\"a\",\"refresh_token\":\"\",\"expires_at\":1,\
                 \"client_id\":\"c\",\"host_id\":\"h\"}",
                Defect::MissingField,
            ),
            (
                "{\"version\":1,\"access_token\":\"a\",\"refresh_token\":\"r\",\"expires_at\":\"1\",\
                 \"client_id\":\"c\",\"host_id\":\"h\"}",
                Defect::NoExpiry,
            ),
        ] {
            std::fs::write(&path, contents).expect("a file");

            let failure = load_at(&path).expect_err(contents);

            assert!(
                matches!(failure, AccountError::Malformed(found) if found == defect),
                "{contents:?}: {failure:?}"
            );
        }
    }

    /// A refusal does not quote the file. A malformed file's text is a token, and a parse error
    /// quotes the bytes it stopped at.
    #[test]
    fn a_refusal_of_a_malformed_file_quotes_none_of_it() {
        let path = scratch("quote");
        std::fs::write(&path, "{\"access_token\": \"a-secret-access-token\" oops").expect("a file");

        let failure = load_at(&path).expect_err("a refusal");

        assert!(!failure.to_string().contains("a-secret-access-token"));
        assert!(!format!("{failure:?}").contains("a-secret-access-token"));
    }

    /// The tokens are not in the debug form, which is where a panic or a log line would print them.
    #[test]
    fn the_tokens_are_redacted_wherever_the_account_is_printed() {
        let printed = format!("{:?}", account());

        assert!(!printed.contains("an-access-token"), "{printed}");
        assert!(!printed.contains("a-refresh-token"), "{printed}");
    }

    /// The file is created readable by its owner alone, as it is created, and so is what replaces
    /// it. A file made at the process's umask holds both tokens for any account on the machine.
    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_by_its_owner_alone_and_so_is_its_replacement() {
        use std::os::unix::fs::PermissionsExt;

        let path = scratch("mode");
        for _ in 0..2 {
            save_at(&path, &account()).expect("a write");
            let mode = std::fs::metadata(&path)
                .expect("the file")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        }
    }

    /// A write that fails partway leaves the last good file in place and no temporary beside it. A
    /// temporary left behind holds both tokens, and one lost file is a person signed out.
    #[test]
    fn a_write_that_stops_part_way_leaves_the_last_good_file_and_no_temporary() {
        use std::io::Write;

        let path = scratch("partway");
        save_at(&path, &account()).expect("a write");
        let before = std::fs::read(&path).expect("the file");

        let failed = write_through(&path, |file| {
            file.write_all(b"{\"version\": 1, \"access_to")?;
            Err(std::io::Error::other("no space left on device"))
        });

        assert!(failed.is_err());
        assert_eq!(std::fs::read(&path).expect("the file"), before);
        let mut left: Vec<String> = std::fs::read_dir(path.parent().expect("a directory"))
            .expect("a listing")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        left.sort();
        assert_eq!(left, [FILE]);
    }

    /// A second sign-in replaces the first, and forgetting removes the file and says whether there
    /// was one. Forgetting where nothing is stored is not an error, as forgetting an import is not.
    #[test]
    fn a_second_sign_in_replaces_the_first_and_forgetting_removes_it() {
        let path = scratch("forget");
        save_at(&path, &account()).expect("a write");
        let mut again = account();
        again.access_token = Secret::new("a-newer-access-token");
        save_at(&path, &again).expect("a replacement");

        let loaded = load_at(&path).expect("a read").expect("a sign-in");
        assert_eq!(loaded.access_token.expose(), "a-newer-access-token");

        assert!(forget_at(&path).expect("forgotten"));
        assert!(load_at(&path).expect("a read").is_none());
        assert!(!forget_at(&path).expect("nothing to forget"));
        let left: Vec<_> = std::fs::read_dir(path.parent().expect("a directory"))
            .expect("a listing")
            .collect();
        assert!(left.is_empty(), "left behind: {left:?}");
    }

    /// Writing and forgetting are made under a claim of their own, named for this file and not
    /// for the Leo credentials: a claim held on the Leo file must not stop a sign-in, and one held
    /// here must stop a write. A write that waited on the wrong name would be a lock on nothing.
    #[test]
    fn a_write_waits_for_the_claim_on_this_file_and_not_for_the_leo_one() {
        let path = scratch("claim");
        std::fs::File::create(path.with_file_name("leo-premium.lock")).expect("a Leo claim");

        // The write ends well inside the minute after which a claim is judged abandoned, so a write
        // that waited on the Leo claim and then broke it is not one that passes.
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn({
            let path = path.clone();
            move || {
                let _ = done.send(save_at(&path, &account()));
            }
        });
        finished
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("the Leo claim held up a write to the ChatGPT file")
            .expect("a write");

        let claim = Claim::take_as(&path, CLAIM, ABANDONED_AFTER).expect("a claim");
        let started = std::time::Instant::now();
        let released = std::thread::spawn({
            let path = path.clone();
            move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                let _ = &path;
                drop(claim);
            }
        });
        save_at(&path, &account()).expect("a write once the claim is released");
        released.join().expect("the releaser");

        assert!(
            started.elapsed() >= std::time::Duration::from_millis(300),
            "the write did not wait for the claim on its own file"
        );
    }

    /// PATH-001: the temporary a write is staged in is named from the destination's own bytes. Two
    /// names that differ only in a byte that is not UTF-8 would share a temporary if the name were
    /// made through a lossy string, and one write would stage over the other. Linux only: macOS refuses a name that is not UTF-8.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_temporary_of_a_name_that_is_not_utf8_is_named_from_its_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let directory = scratch("non-utf8")
            .parent()
            .expect("a directory")
            .to_path_buf();
        let destination = directory.join(OsString::from_vec(b"account-\xff.json".to_vec()));
        let mut staged = Vec::new();

        write_through(&destination, |_| {
            for entry in std::fs::read_dir(&directory).expect("a listing") {
                staged.push(entry.expect("an entry").file_name());
            }
            Ok(())
        })
        .expect("a write");

        let mut wanted = b"account-\xff.json.".to_vec();
        wanted.extend(std::process::id().to_string().as_bytes());
        wanted.extend(b".tmp");
        assert!(
            staged
                .iter()
                .any(|name| name.as_bytes() == wanted.as_slice()),
            "no temporary named from the destination's bytes: {staged:?}"
        );
    }

    /// TS-004: a claim aged less than the 60 seconds this file's claim is given is waited on, not
    /// broken, and one aged past it is broken. The Leo claim is abandoned after 5 seconds, so a
    /// ChatGPT claim aged 10 seconds that was judged by that figure would be taken from a holder
    /// still waiting on its reply.
    #[test]
    fn a_claim_on_this_file_is_waited_on_until_it_is_a_minute_old() {
        let path = scratch("claim-age");
        let claim = path.with_file_name(CLAIM);
        let aged = |seconds: u64| {
            std::fs::File::create(&claim)
                .expect("a claim")
                .set_modified(
                    std::time::SystemTime::now() - std::time::Duration::from_secs(seconds),
                )
                .expect("an aged claim");
        };

        // Ten seconds old: held by a process that may still be waiting for its reply.
        aged(10);
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn({
            let path = path.clone();
            move || {
                let _ = done.send(save_at(&path, &account()).is_ok());
            }
        });
        assert!(
            finished
                .recv_timeout(std::time::Duration::from_millis(700))
                .is_err(),
            "a claim ten seconds old was broken"
        );
        assert!(claim.exists(), "a claim ten seconds old was removed");
        // Released by its holder, the waiting write goes ahead.
        std::fs::remove_file(&claim).expect("the holder releases it");
        assert_eq!(
            finished.recv_timeout(std::time::Duration::from_secs(20)),
            Ok(true)
        );

        // Two minutes old: left by a process that died, and broken.
        aged(120);
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn({
            let path = path.clone();
            move || {
                let _ = done.send(save_at(&path, &account()).is_ok());
            }
        });
        assert_eq!(
            finished.recv_timeout(std::time::Duration::from_secs(20)),
            Ok(true),
            "a claim two minutes old was waited on"
        );
    }
}
