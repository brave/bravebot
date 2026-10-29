//! The xAI sign-in kept in one file the account owns
//! ([XAI-5](../../../../docs/specs/xai-sign-in.md#XAI-5)).
//!
//! `xai-account.json` sits in the state directory and is made private to the account, which
//! [`crate::oauth`] does for every sign-in that keeps its tokens in a file. It holds a
//! version, the two tokens and the instant the access token expires. The client the sign-in presents
//! is a constant of the program, and is not stored.

use super::Hosts;
use crate::Secret;
use crate::oauth::{self, AccountError, Defect, Kind, Refused, Renewed};
use crate::secret::Document;
use std::path::{Path, PathBuf};

/// The file, in the state directory.
const FILE: &str = "xai-account.json";

/// The claim taken while the file is written or removed.
const CLAIM: &str = "xai-account.lock";

/// A signed-in xAI account.
///
/// No equality, which the tokens it holds do not have: see [`Secret`].
#[derive(Debug, Clone)]
pub struct Account {
    /// What a request to the service carries.
    pub access_token: Secret,
    /// What mints another access token.
    pub refresh_token: Secret,
    /// The instant the access token expires, in seconds since 1970-01-01.
    pub expires_at: u64,
}

/// The xAI sign-in as the file and the renewer of [`oauth`] state it: which file, what it holds, and
/// what presenting its refresh token looks like.
pub struct Xai;

impl Kind for Xai {
    type Account = Account;
    type Hosts = Hosts;
    const FILE: &'static str = FILE;
    const CLAIM: &'static str = CLAIM;

    fn encode(account: &Account) -> Secret {
        encode(account)
    }

    fn decode(raw: &str) -> Result<Account, AccountError> {
        decode(raw)
    }

    fn access(account: &Account) -> &Secret {
        &account.access_token
    }

    fn refresh(account: &Account) -> &Secret {
        &account.refresh_token
    }

    fn expires_at(account: &Account) -> u64 {
        account.expires_at
    }

    fn replace(
        current: &Account,
        access: Secret,
        refresh: Option<Secret>,
        expires_at: u64,
    ) -> Account {
        Account {
            access_token: access,
            refresh_token: refresh.unwrap_or_else(|| current.refresh_token.clone()),
            expires_at,
        }
    }

    fn renew(agent: &ureq::Agent, hosts: &Hosts, account: &Account) -> Result<Renewed, Refused> {
        super::exchange::refresh(agent, hosts, &account.refresh_token)
    }
}

/// Where the sign-in is kept.
pub fn path() -> Result<PathBuf, AccountError> {
    oauth::path::<Xai>()
}

impl Account {
    /// The stored sign-in, or `None` where nothing is stored.
    ///
    /// A file that cannot be read, is empty, or holds the fields of another version is an error
    /// that names the remedy, and is not read as an absent sign-in.
    pub fn load() -> Result<Option<Self>, AccountError> {
        load_at(&path()?)
    }

    /// Forget the stored sign-in. Whether there was one is the answer; none is not a failure.
    ///
    /// This removes the copy here and ends nothing at xAI.
    pub fn forget() -> Result<bool, AccountError> {
        forget_at(&path()?)
    }

    /// The instant the access token expires, as `2026-10-22T21:05:27Z`.
    pub fn expires(&self) -> String {
        crate::oauth::encoding::utc(self.expires_at)
    }
}

pub(super) fn load_at(path: &Path) -> Result<Option<Account>, AccountError> {
    oauth::load_at::<Xai>(path)
}

pub(super) fn save_at(path: &Path, account: &Account) -> Result<(), AccountError> {
    oauth::save_at::<Xai>(path, account)
}

pub(super) fn forget_at(path: &Path) -> Result<bool, AccountError> {
    oauth::forget_at::<Xai>(path)
}

fn encode(account: &Account) -> Secret {
    // Built inside a guard, and the text it serialises to is a secret of its own: both hold both
    // tokens, and the file the caller writes is the only copy meant to outlive this call.
    let document = Document::of(serde_json::json!({
        "version": 1,
        "access_token": account.access_token.expose(),
        "refresh_token": account.refresh_token.expose(),
        "expires_at": account.expires_at,
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
    let (Some(access), Some(refresh)) = (text("access_token"), text("refresh_token")) else {
        return Err(AccountError::Malformed(Defect::MissingField));
    };
    let Some(expires_at) = value.get("expires_at").and_then(serde_json::Value::as_u64) else {
        return Err(AccountError::Malformed(Defect::NoExpiry));
    };
    Ok(Account {
        access_token: Secret::new(access),
        refresh_token: Secret::new(refresh),
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::ABANDONED_AFTER;
    use crate::store::{Claim, write_through};

    fn account() -> Account {
        Account {
            access_token: Secret::new("an-access-token"),
            refresh_token: Secret::new("a-refresh-token"),
            expires_at: 1_792_703_127,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let directory = crate::testutil::scratch_dir(&format!("bravebot-xai-account-{name}"));
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
    }

    /// The file holds the fields XAI-5 names and no others: not the client, which is a constant of
    /// the program, and not a field of the reply. A file that kept the whole reply would hold values
    /// the clause says are never stored.
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
            ["access_token", "expires_at", "refresh_token", "version"]
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
                "{\"version\":1,\"access_token\":\"a\",\"refresh_token\":\"\",\"expires_at\":1}",
                Defect::MissingField,
            ),
            (
                "{\"version\":1,\"access_token\":\"a\",\"expires_at\":1}",
                Defect::MissingField,
            ),
            (
                "{\"version\":1,\"access_token\":\"a\",\"refresh_token\":\"r\",\"expires_at\":\"1\"}",
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
    /// was one. Forgetting where nothing is stored is not an error.
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

    /// The sign-in has a file and a claim of its own, named for it and not for the ChatGPT account.
    /// A write that waited on another sign-in's claim would be held up by it, and one that took
    /// another's file name would write the tokens over a sign-in that is not this one.
    #[test]
    fn the_file_and_the_claim_are_this_sign_ins_own() {
        assert_eq!(<Xai as Kind>::FILE, "xai-account.json");
        assert_eq!(<Xai as Kind>::CLAIM, "xai-account.lock");
        let path = scratch("claim");
        std::fs::File::create(path.with_file_name("chatgpt-account.lock"))
            .expect("a ChatGPT claim");

        // The write ends well inside the minute after which a claim is judged abandoned, so a write
        // that waited on that claim and then broke it is not one that passes.
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn({
            let path = path.clone();
            move || {
                let _ = done.send(save_at(&path, &account()));
            }
        });
        finished
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("another sign-in's claim held up a write of this one")
            .expect("a write");

        let claim = Claim::take_as(&path, CLAIM, ABANDONED_AFTER).expect("a claim");
        let released = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(claim);
        });
        let started = std::time::Instant::now();
        save_at(&path, &account()).expect("a write once the claim is released");
        released.join().expect("the releaser");
        assert!(
            started.elapsed() >= std::time::Duration::from_millis(300),
            "the write did not wait for the claim on its own file"
        );
    }
}
