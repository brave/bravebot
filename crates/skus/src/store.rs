//! Keeping the imported credentials in a file under `~/.bravebot` that no other account can read.
//!
//! # One batch, whichever channel it came from
//!
//! There is a single file, so importing from Nightly replaces what was imported from Stable. The
//! channel says where to *read the order id from*, which is a fact about the machine's browsers, not
//! about the agent: one person's subscription is one subscription however many Brave builds they
//! have installed, and every import registers this same install as a device against it.
//!
//! Keeping one per channel implied a choice the user never made. It also meant a stale batch from a
//! channel long since uninstalled sat there being reported, and a re-import after switching channels
//! left the old file to be picked up by whichever load happened to reach it first.
//!
//! # Why not the OS keychain
//!
//! These are bearer secrets, so the keychain looks like the obvious home for them, and it was that
//! for a while. It was the wrong call on both halves of the trade.
//!
//! The browser these are imported from keeps the same secret in a plain preference. `skus.state`
//! and `brave.ai_chat.premium_credential_cache` are unencrypted JSON in the profile, and nothing
//! under brave-core's `components/skus` or `components/ai_chat` references OSCrypt for them. So a
//! keychain here guarded a copy of something already readable in the file the copy came from.
//!
//! Nor does it hold against the threat it was written for, which was a program `run` launches
//! reading the file. Those are deliberately unconfined (RUN-10), and the AWS credentials that sign
//! every model request are cached by the `aws` CLI in plain 0600 JSON, so anything able to read a
//! file here can already take the larger secret. Guarding the smaller one behind the keychain
//! bought a password dialog, not a boundary: [`Wallet`] holds the whole decrypted batch in memory
//! for the session either way.
//!
//! What it did cost was availability. The keychain crate builds one Linux backend, the D-Bus
//! Secret Service, so a machine reached over SSH with no desktop session had no store to open at
//! all and every such user was silently spending no subscription.
//!
//! # Why the whole batch is stored, not one cookie
//!
//! A time-limited-v2 credential is single-use. Presenting one to the backend spends it, so what is
//! stored is the batch the server signed, and a request takes the next unspent one. Caching a
//! ready-made cookie value would mean replaying a spent credential on the second request.

use crate::device::Registration;
use std::path::PathBuf;

/// The directory inside the user's home the file is kept in.
const DIRECTORY: &str = ".bravebot";

/// The file itself. One, not one per channel: see the module documentation.
const FILE: &str = "leo-premium.json";

#[derive(Debug)]
pub enum StoreError {
    /// Nothing has been imported yet.
    NotFound,
    /// The file exists and could not be read or written.
    Unusable { detail: String },
    /// The file exists but is not what this version writes.
    Malformed { detail: String },
    /// Every credential valid now has been spent.
    Exhausted,
    /// The batch's last validity window has closed.
    ///
    /// Separate from [`StoreError::Exhausted`] because it is the usual way a batch stops working,
    /// and it typically happens with most of the batch never used.
    Expired { until: String, unspent: usize },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("no imported Leo subscription found"),
            Self::Unusable { detail } => {
                write!(f, "the stored credentials could not be read: {detail}")
            }
            Self::Malformed { detail } => {
                write!(f, "the stored credentials are unusable: {detail}")
            }
            Self::Exhausted => f.write_str(
                "every credential valid today has been spent; run `bravebot import-leo-creds` again",
            ),
            Self::Expired { until, unspent } => write!(
                f,
                "the imported credentials expired at {until} with {unspent} never used; \
                 run `bravebot import-leo-creds` again"
            ),
        }
    }
}

impl std::error::Error for StoreError {}

/// A signed credential batch, as stored.
///
/// Serialised as JSON rather than a bespoke encoding because a readable shape is one less thing to
/// get wrong when the format changes.
///
/// No equality, which the credential a batch is made of does not have: see [`crate::Secret`].
#[derive(Debug, Clone)]
pub struct StoredCredentials {
    /// The order the batch belongs to, so a re-import can refresh in place.
    pub order_id: String,
    /// Which service issued this batch, so a refill goes back to the same one.
    ///
    /// Stored rather than recomputed because the browser it was imported from may be gone by then,
    /// and a credential minted against the wrong environment cannot be used.
    pub environment: crate::Environment,
    /// The item the credentials are for.
    pub item_id: String,
    /// `merchant?sku=` string the presentation signs over.
    pub issuer: String,
    /// The unblinded credentials, each usable once.
    pub credentials: Vec<Credential>,
}

/// One single-use credential.
#[derive(Debug, Clone)]
pub struct Credential {
    /// Base64 unblinded token, from which the verification key is derived.
    ///
    /// A bearer value for as long as the batch is held, so it lives in a buffer that clears itself
    /// when it goes ([CRED-23](../../../docs/specs/credential-protection.md#CRED-23)).
    pub unblinded: crate::Secret,
    /// Start of the window this credential is valid in, as the server stated it.
    pub valid_from: String,
    /// End of that window.
    pub valid_to: String,
    /// Whether this one has already been presented.
    pub spent: bool,
    /// Which key derivation this token was blinded with.
    ///
    /// Stored per credential because the two derivations yield different verification keys, and
    /// picking the wrong one produces a signature the server rejects with nothing to explain it.
    pub rfc: bool,
}

impl StoredCredentials {
    /// How many credentials remain unspent.
    pub fn remaining(&self) -> usize {
        self.credentials.iter().filter(|c| !c.spent).count()
    }

    /// The index of the next credential usable at `now`.
    ///
    /// `now` is passed in rather than read from the clock so the choice is testable, and compared
    /// as a string because the server's timestamps are fixed-width ISO 8601 in UTC, which orders
    /// lexicographically. Parsing them would add a date library to compare two strings.
    pub fn next_usable(&self, now: &str) -> Option<usize> {
        self.credentials
            .iter()
            .position(|c| !c.spent && c.valid_from.as_str() <= now && now < c.valid_to.as_str())
    }

    /// Whether every credential's window has closed by `now`.
    ///
    /// Distinct from being spent, and the distinction matters because it is the common case: a
    /// batch covers a few daily windows, so it usually stops working with most of it never used.
    /// Reporting that as "used up" would send someone looking for heavy usage that did not happen.
    pub fn expired(&self, now: &str) -> bool {
        !self.credentials.is_empty() && self.credentials.iter().all(|c| c.valid_to.as_str() <= now)
    }

    /// The end of the last window, which is when this batch stops being usable.
    pub fn usable_until(&self) -> Option<&str> {
        self.credentials.iter().map(|c| c.valid_to.as_str()).max()
    }
}

impl From<Registration> for StoredCredentials {
    fn from(value: Registration) -> Self {
        Self {
            order_id: value.order_id,
            environment: value.environment,
            item_id: value.item_id,
            issuer: value.issuer,
            credentials: value
                .credentials
                .into_iter()
                .map(|c| Credential {
                    unblinded: c.unblinded,
                    valid_from: c.valid_from,
                    valid_to: c.valid_to,
                    spent: false,
                    rfc: c.rfc,
                })
                .collect(),
        }
    }
}

/// The variables the platform states the user's profile directory in, in the order they answer.
///
/// Spelled here as well as in the crates above this one, which is what a crate depending on nothing
/// costs. What has to hold across the copies is the name of the directory, the variables, and the
/// refusal to invent one.
///
/// `HOME` on either platform: it is the one Unix sets, and a Windows shell environment that sets one
/// has been told where the profile is. `USERPROFILE` is the one stock Windows sets, and is read there
/// only, since on Unix it is not a name the platform states anything in.
#[cfg(windows)]
const PROFILE_VARIABLES: &[&str] = &["HOME", "USERPROFILE"];
#[cfg(not(windows))]
const PROFILE_VARIABLES: &[&str] = &["HOME"];

/// The file holding the imported credentials.
///
/// The variables are read directly rather than through a dependency, for the same reason and with
/// the same absence of a fallback as everywhere else the directory is resolved: inventing one would
/// put a bearer secret somewhere the user never chose.
pub fn path() -> Result<PathBuf, StoreError> {
    path_named(PROFILE_VARIABLES.iter().map(std::env::var_os))
}

/// The same answer, from the values rather than from the variables.
///
/// Split from the read so the order they answer in is testable without a process-wide variable,
/// which here also means without the `unsafe` block setting one takes.
///
/// The first value that names something wins. An empty one names nothing, so it is passed over
/// rather than joined onto: joining would put a bearer secret in `/.bravebot`, and stopping there
/// would lose a profile directory the platform does name to a variable some shell exported empty.
fn path_named(
    named: impl IntoIterator<Item = Option<std::ffi::OsString>>,
) -> Result<PathBuf, StoreError> {
    let home = named.into_iter().flatten().find(|home| !home.is_empty());
    let Some(home) = home else {
        return Err(StoreError::Unusable {
            detail: format!(
                "{} names nothing, so there is nowhere to keep credentials",
                PROFILE_VARIABLES.join(" or ")
            ),
        });
    };
    Ok(PathBuf::from(home).join(DIRECTORY).join(FILE))
}

/// Write the batch, replacing whatever was there.
///
/// Created 0600 before anything is written to it, rather than written and then chmod'ed: the other
/// order leaves the secret world-readable for the moment in between.
#[cfg(unix)]
pub fn save(credentials: &StoredCredentials) -> Result<(), StoreError> {
    use std::io::Write;
    use std::os::unix::fs::DirBuilderExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::fs::PermissionsExt;

    let path = path()?;
    let unusable = |detail: String| StoreError::Unusable { detail };

    // Created reachable only by this user, and narrowed where it is already there. The state
    // directory is made by whichever subsystem writes to it first, so one that made it at the
    // umask would decide the mode for the prompt history and the session records too. Spelled out
    // here rather than shared with the crates that have their own helper for it: this crate
    // depends on nothing, and an auth-only crate is not worth a dependency for four lines.
    if let Some(parent) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|e| unusable(format!("{}: {e}", parent.display())))?;
        // A link is stepped over rather than followed. `set_permissions` resolves one, and the
        // name this was given says where the directory sits and nothing about where it leads:
        // somebody who keeps their sessions on a synced volume and links `~/.bravebot` into place
        // would otherwise have importing a subscription setting the mode of the volume's
        // directory, which is outside anything this program was given.
        let is_link =
            std::fs::symlink_metadata(parent).is_ok_and(|found| found.file_type().is_symlink());
        if !is_link {
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| unusable(format!("{}: {e}", path.display())))?;
    // The mode above is asked for as the file is created and says nothing about one already there,
    // which a person may have restored from a backup or copied between machines. Narrowed after the
    // open and before the write, so the truncated file is private before it holds a token again.
    let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));

    file.write_all(encode(credentials).expose().as_bytes())
        .map_err(|e| unusable(format!("{}: {e}", path.display())))
}

/// Write the batch, replacing whatever was there.
///
/// Windows has no mode, so what says the same thing is an access-control list, and the list is asked
/// for as the file is created rather than set afterwards: the other order leaves the secret readable
/// by whatever the directory grants for the moment in between. [`dacl_granting_only`] is the list.
#[cfg(windows)]
pub fn save(credentials: &StoredCredentials) -> Result<(), StoreError> {
    use std::io::Write;

    let path = path()?;
    let unusable = |detail: String| StoreError::Unusable { detail };

    // The directory is left with whatever the profile directory grants it, which is the cost
    // state-directory.md records for a platform where the mode is not there to be set: what else is
    // kept in there is decided by the crates that write it, not by this one. The file's own list
    // does not depend on the directory's, which is why it is protected below.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| unusable(format!("{}: {e}", parent.display())))?;
    }

    let mut file = acl::create_granted_to_this_account_only(&path)
        .map_err(|e| unusable(format!("{}: {e}", path.display())))?;

    file.write_all(encode(credentials).expose().as_bytes())
        .map_err(|e| unusable(format!("{}: {e}", path.display())))
}

/// The access-control list the credential file is kept under, written as SDDL.
///
/// `D:` opens the list and `P` protects it, so nothing the directory above offers is added to what
/// follows: a profile directory on a shared or a roaming volume can carry an inheritable entry for
/// `Users`, and a list that took it would hand a bearer token to every account on the machine. `A`
/// allows and `FA` is full access to a file, so the one entry is the one account, and an account
/// with no entry in a protected list is granted nothing.
///
/// A string handed to the platform's parser rather than a list built entry by entry, so that what is
/// granted is decided by something a test can call anywhere. Nothing else in this function can be
/// exercised off Windows, and `make check-windows` compiles it without running it.
#[cfg(any(windows, test))]
fn dacl_granting_only(account: &str) -> String {
    format!("D:P(A;;FA;;;{account})")
}

/// Refused, because there is nothing here that can keep the secret to the user.
///
/// Unix creates the file 0600 and Windows creates it granted to one account, both before a byte is
/// written, and there is no equivalent on this target: `std::os::unix` does not exist and neither
/// does the Win32 call. The file holds a bearer token, so writing it under whatever permissions it
/// happened to inherit is worse than not writing it, and doing that silently is worse still, since
/// nothing would ever say the secret is unprotected. This refuses instead, and the caller reports it.
///
/// Reading stays available: an existing file is no less safe for being read, and a batch imported
/// elsewhere should still work here.
#[cfg(not(any(unix, windows)))]
pub fn save(_credentials: &StoredCredentials) -> Result<(), StoreError> {
    Err(StoreError::Unusable {
        detail: "this platform has no way to restrict the file to your account, and the \
                 credentials are a bearer token, so they were not written"
            .to_string(),
    })
}

/// The Win32 calls behind the Windows [`save`], the only unsafe in this crate outside the helper
/// the tests point `HOME` with.
///
/// Every entry point here is a thin wrapper reporting `GetLastError` as an [`std::io::Error`], so the
/// caller above reads the same as the Unix one. What the list *says* is [`super::dacl_granting_only`]
/// instead, so the decision a reviewer cares about is not inside an unsafe block.
#[cfg(windows)]
mod acl {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io::{Error, Result};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::path::Path;
    use windows_sys::Win32::Foundation::{
        ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1, SE_FILE_OBJECT, SetSecurityInfo,
    };
    use windows_sys::Win32::Security::{
        ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetTokenInformation,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_ALWAYS, WRITE_DAC,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// Open `path`, creating it if it is not there, reachable by the account this process runs as and
    /// by no other, and empty ready to be written.
    pub fn create_granted_to_this_account_only(path: &Path) -> Result<File> {
        let descriptor = SecurityDescriptor::of(&super::dacl_granting_only(&current_account()?))?;
        let file = open(path, &descriptor)?;
        // The list asked for in the open is the list a file gets when the open creates it, and says
        // nothing about one already there: a file copied in from another machine carries whatever the
        // directory it landed in offers, an inheritable entry for `Users` included. So it is set again
        // on the handle, before anything is written.
        descriptor.apply_to(&file)?;
        // Emptied once the list is settled rather than by the open, so that a batch somebody copied
        // here is merely unwritten and not destroyed if setting the list fails. Importing again is
        // how a lost batch is replaced, and this is the platform that cannot import.
        file.set_len(0)?;
        Ok(file)
    }

    /// Open `path` for writing, creating it if it is not there, asking for `descriptor` as it is
    /// created.
    #[allow(unsafe_code)]
    fn open(path: &Path, descriptor: &SecurityDescriptor) -> Result<File> {
        let path = wide(path.as_os_str());
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                // The right to set the list as well as the right to write the bytes, since the list
                // is set again once the file is open.
                GENERIC_WRITE | WRITE_DAC,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                &attributes,
                OPEN_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(Error::last_os_error());
        }
        // Sound because the handle is the one this call just opened, checked against the value it
        // reports a failure with, and is owned by nothing else.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    /// The account this process runs as, written the way an entry in a list names one.
    ///
    /// Taken from the process token rather than from a user name, because a name is not what the list
    /// holds and the same name can belong to two accounts, one on the machine and one in a domain.
    #[allow(unsafe_code)]
    fn current_account() -> Result<String> {
        // Sound because a call that returns nonzero has written an open handle nothing else owns,
        // which is what [`OwnedHandle`] takes responsibility for closing.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let token = unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(Error::last_os_error());
            }
            OwnedHandle::from_raw_handle(token)
        };

        // An account is a variable-length value, so its length is asked for before there is anywhere
        // to put it, and the call that asks fails saying so.
        let mut length = 0;
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let measured = unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                std::ptr::null_mut(),
                0,
                &mut length,
            )
        };
        if measured == 0 {
            let no_room = Error::last_os_error();
            if no_room.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
                return Err(no_room);
            }
        }

        // Words rather than bytes, so the struct written into it is aligned as it expects.
        let mut buffer = vec![0u64; (length as usize).div_ceil(size_of::<u64>()).max(1)];
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let read = unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        };
        if read == 0 {
            return Err(Error::last_os_error());
        }

        // Sound because the call above returned success, which means it wrote a whole `TOKEN_USER`
        // and the account it points at into this buffer, and a `Vec<u64>` is aligned for both. The
        // string the SID is written to is allocated by the call that writes it, null-terminated, and
        // freed here.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe {
            let user: *const TOKEN_USER = buffer.as_ptr().cast();
            let mut written = std::ptr::null_mut();
            if ConvertSidToStringSidW((*user).User.Sid, &mut written) == 0 {
                return Err(Error::last_os_error());
            }
            let mut end = written;
            while *end != 0 {
                end = end.add(1);
            }
            let account = String::from_utf16_lossy(std::slice::from_raw_parts(
                written,
                end.offset_from(written) as usize,
            ));
            LocalFree(written.cast());
            Ok(account)
        }
    }

    /// A list the platform parsed out of its written form, freed when it goes out of scope.
    struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

    impl SecurityDescriptor {
        /// Parse SDDL into the form a file takes.
        #[allow(unsafe_code)]
        fn of(sddl: &str) -> Result<Self> {
            let sddl = wide(OsStr::new(sddl));
            let mut descriptor = std::ptr::null_mut();
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            let parsed = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            };
            if parsed == 0 {
                return Err(Error::last_os_error());
            }
            Ok(Self(descriptor))
        }

        /// Replace the list on an open file with this one, and stop it inheriting any other.
        ///
        /// A descriptor that named no list at all would answer nothing here, and a file with no list
        /// grants every account everything, so that answer is an error rather than a token written
        /// out in the open.
        #[allow(unsafe_code)]
        fn apply_to(&self, file: &File) -> Result<()> {
            let mut dacl: *mut ACL = std::ptr::null_mut();
            let mut present = 0;
            let mut defaulted = 0;
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            let read = unsafe {
                GetSecurityDescriptorDacl(self.0, &mut present, &mut dacl, &mut defaulted)
            };
            if read == 0 {
                return Err(Error::last_os_error());
            }
            if present == 0 || dacl.is_null() {
                return Err(Error::other(
                    "names no account, so it would grant every account",
                ));
            }

            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            let set = unsafe {
                SetSecurityInfo(
                    file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    dacl,
                    std::ptr::null(),
                )
            };
            if set != ERROR_SUCCESS {
                return Err(Error::from_raw_os_error(set as i32));
            }
            Ok(())
        }
    }

    impl Drop for SecurityDescriptor {
        #[allow(unsafe_code)]
        fn drop(&mut self) {
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            unsafe { LocalFree(self.0) };
        }
    }

    /// A null-terminated UTF-16 copy, which is what every call here reads a string as.
    fn wide(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain(std::iter::once(0)).collect()
    }
}

/// Read the batch.
pub fn load() -> Result<StoredCredentials, StoreError> {
    let path = path()?;
    // Held in a buffer that clears itself, since the text of the file is every token in the batch
    // and a read that is answered with an error hands it back as readily as one that succeeds.
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => crate::Secret::new(raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(StoreError::NotFound),
        Err(e) => {
            return Err(StoreError::Unusable {
                detail: format!("{}: {e}", path.display()),
            });
        }
    };
    decode(raw.expose())
}

/// A batch held open for a session, spending from memory.
///
/// # Why this is not read and written per request
///
/// A credential is spent on *every* model request, and a batch is hundreds of them, so writing the
/// whole file per spend would rewrite the same few hundred kilobytes several times a turn to change
/// one boolean. The batch is read once, spent from memory, and the markers are written back when
/// the session ends or when [`Wallet::flush`] is called.
///
/// The failure mode is losing spend markers if the process dies, which means a credential that was
/// presented is still recorded as unspent. That is deliberately the direction to fail in: a batch
/// is hundreds of credentials valid for days, so wasting a few is free, and the alternative
/// (recording a spend that never happened) is what runs the batch down for no benefit.
pub struct Wallet {
    batch: StoredCredentials,
    /// Where a flush writes, or `None` for a detached batch that must never be written.
    ///
    /// Resolved when the wallet is opened rather than at flush time, which is what makes a detached
    /// wallet safe: there is no path to write to, so no code path, including [`Drop`], can reach the
    /// filesystem. A boolean would leave a real destination sitting there for a later edit to use.
    destination: Option<PathBuf>,
    /// Whether anything has been spent since the last write.
    dirty: bool,
}

impl Wallet {
    /// Read the batch, reading the file at most once.
    pub fn open() -> Result<Self, StoreError> {
        Ok(Self {
            batch: load()?,
            destination: Some(path()?),
            dirty: false,
        })
    }

    /// Hold a batch that is already in hand, with no file behind it.
    ///
    /// For tests, including those in crates above this one, which is why it is public. A test must
    /// never write to the real store: it would overwrite the credentials of whoever ran it.
    ///
    /// The result is detached, so spending and flushing behave normally but nothing is ever
    /// written, not even by [`Drop`].
    pub fn detached(batch: StoredCredentials) -> Self {
        Self {
            batch,
            destination: None,
            dirty: false,
        }
    }

    /// Take the next credential usable at `now`, marking it spent in memory.
    pub fn spend(&mut self, now: &str) -> Result<Spent, StoreError> {
        let index = match self.batch.next_usable(now) {
            Some(index) => index,
            // Nothing usable is normal rather than exceptional: a batch covers a few daily windows
            // and stops working when the last one closes, usually with most of it unspent. So the
            // two cases are reported apart, and the caller refills rather than giving up.
            None if self.batch.expired(now) => {
                return Err(StoreError::Expired {
                    until: self.batch.usable_until().unwrap_or("unknown").to_string(),
                    unspent: self.batch.remaining(),
                });
            }
            None => return Err(StoreError::Exhausted),
        };

        self.batch.credentials[index].spent = true;
        self.dirty = true;

        Ok(Spent {
            credential: self.batch.credentials[index].clone(),
            issuer: self.batch.issuer.clone(),
            remaining: self.batch.remaining(),
        })
    }

    /// The order this batch belongs to, so a refill knows what to register against.
    pub fn order_id(&self) -> &str {
        &self.batch.order_id
    }

    /// Which service issued this batch, so a refill returns to the same one.
    pub fn environment(&self) -> crate::Environment {
        self.batch.environment
    }

    /// Replace the batch with a freshly issued one, keeping the same destination.
    ///
    /// Marked dirty so the new batch is written even though nothing has been spent from it yet:
    /// losing it would mean minting another on the next run for no reason.
    pub fn refill(&mut self, batch: StoredCredentials) {
        self.batch = batch;
        self.dirty = true;
    }

    /// Write the spent markers back, if any.
    ///
    /// A no-op when nothing was spent, so an idle session never writes at all, and a no-op for a
    /// detached batch, which has nowhere to write.
    pub fn flush(&mut self) -> Result<(), StoreError> {
        if self.destination.is_none() || !self.dirty {
            return Ok(());
        }
        save(&self.batch)?;
        self.dirty = false;
        Ok(())
    }

    pub fn remaining(&self) -> usize {
        self.batch.remaining()
    }
}

/// Writes the spent markers back, so a session that ends normally does not replay credentials.
///
/// Errors are dropped: this runs during teardown where there is nothing useful to do with one, and
/// the consequence is only that some spent credentials look unspent next time.
impl Drop for Wallet {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// A credential taken out of the store, already recorded as used.
#[derive(Debug, Clone)]
pub struct Spent {
    pub credential: Credential,
    /// The issuer string this credential's presentation signs over.
    pub issuer: String,
    /// How many are left, so a caller can warn before the batch runs out.
    pub remaining: usize,
}

/// Forget the imported batch.
pub fn clear() -> Result<(), StoreError> {
    let path = path()?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        // Already absent is the outcome asked for, not a failure.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(StoreError::Unusable {
            detail: format!("{}: {e}", path.display()),
        }),
    }
}

/// Only [`save`] writes, and the target that has neither a mode nor an access-control list has a
/// [`save`] that refuses, so there this is reachable from the tests alone.
#[cfg(any(unix, windows, test))]
fn encode(credentials: &StoredCredentials) -> crate::Secret {
    // The document is built inside a guard and the text it serialises to is a secret of its own:
    // both hold every token in the batch, and the file the caller writes is the only copy meant to
    // outlive this call.
    let document = crate::secret::Document::of(serde_json::json!({
        "version": 1,
        "order_id": credentials.order_id,
        "environment": credentials.environment.as_str(),
        "item_id": credentials.item_id,
        "issuer": credentials.issuer,
        "credentials": credentials
            .credentials
            .iter()
            .map(|c| serde_json::json!({
                "unblinded": c.unblinded.expose(),
                "valid_from": c.valid_from,
                "valid_to": c.valid_to,
                "spent": c.spent,
                "rfc": c.rfc,
            }))
            .collect::<Vec<_>>(),
    }));
    crate::Secret::new(document.read().to_string())
}

fn decode(raw: &str) -> Result<StoredCredentials, StoreError> {
    // A file can exist holding nothing, if a write was interrupted partway: the open truncates
    // before anything is written. Reported rather than read as absent, since a subscription that
    // was paid for is not being spent and nothing else would say so.
    if raw.trim().is_empty() {
        return Err(StoreError::Malformed {
            detail: "the file holds nothing, which an interrupted write leaves behind; \
                     run `bravebot import-leo-creds` again"
                .to_string(),
        });
    }

    // Parsed into a guard, so the copy of every token the parse makes is cleared however this
    // returns: a malformed file leaves by one of the refusals below and its tokens are as real as
    // a good file's.
    let document = crate::secret::Document::of(serde_json::from_str(raw).map_err(|e| {
        StoreError::Malformed {
            detail: format!("not valid JSON: {e}"),
        }
    })?);
    let value = document.read();

    let field = |name: &str| -> Result<String, StoreError> {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| StoreError::Malformed {
                detail: format!("missing '{name}'"),
            })
    };

    let credentials = value
        .get("credentials")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| StoreError::Malformed {
            detail: "missing 'credentials'".to_string(),
        })?
        .iter()
        .map(|c| {
            let text = |name: &str| {
                c.get(name)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            Credential {
                unblinded: crate::Secret::new(text("unblinded")),
                valid_from: text("valid_from"),
                valid_to: text("valid_to"),
                spent: c
                    .get("spent")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                // Every batch this writes is blinded with the rfc derivation, so that is the
                // reading for an entry that predates the field being recorded.
                rfc: c
                    .get("rfc")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
            }
        })
        .collect::<Vec<_>>();

    if credentials.iter().any(|c| c.unblinded.is_empty()) {
        return Err(StoreError::Malformed {
            detail: "a credential has no token".to_string(),
        });
    }

    Ok(StoredCredentials {
        order_id: field("order_id")?,
        // Entries written before this was recorded were all production, which was the only
        // environment reachable then.
        environment: value
            .get("environment")
            .and_then(serde_json::Value::as_str)
            .and_then(crate::Environment::of_name)
            .unwrap_or(crate::Environment::Production),
        item_id: field("item_id")?,
        issuer: field("issuer")?,
        credentials,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One lock for the whole file, not one per test.
    ///
    /// `HOME` is process-wide and these tests run in threads of one process, so every test that
    /// touches it contends for the same thing. A mutex declared inside each function would be a
    /// different mutex, and two tests would then see each other's home.
    static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Point `HOME` at a scratch directory, so no test can read or write the developer's own
    /// credentials.
    fn with_temp_home<T>(name: &str, body: impl FnOnce() -> T) -> T {
        let dir = crate::testutil::scratch_dir(&format!("bravebot-skus-{name}"));
        with_home(Some(dir), body)
    }

    fn with_no_home<T>(body: impl FnOnce() -> T) -> T {
        with_home(None, body)
    }

    // These tests need `HOME` pointed somewhere else, and there is no safe way to do that.
    #[allow(unsafe_code)]
    fn with_home<T>(dir: Option<PathBuf>, body: impl FnOnce() -> T) -> T {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(dir) = &dir {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir_all(dir).expect("scratch home");
        }

        // Every variable the platform states a profile directory in, not `HOME` alone: one left set
        // would answer where `HOME` does not, which would have `with_no_home` asking what happens
        // when one variable is missing rather than when there is no home at all.
        let previous: Vec<_> = PROFILE_VARIABLES
            .iter()
            .map(|variable| (*variable, std::env::var_os(variable)))
            .collect();
        // SAFETY: single-threaded within the lock, and restored before returning.
        for (variable, _) in &previous {
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            unsafe { std::env::remove_var(variable) };
        }
        if let Some(dir) = &dir {
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            unsafe { std::env::set_var("HOME", dir) };
        }

        let result = body();

        for (variable, value) in previous {
            match value {
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                Some(value) => unsafe { std::env::set_var(variable, value) },
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                None => unsafe { std::env::remove_var(variable) },
            }
        }
        if let Some(dir) = &dir {
            let _ = std::fs::remove_dir_all(dir);
        }
        result
    }

    /// The order, the environment, the item, the issuer, and every credential in the batch.
    type Fields<'a> = (
        &'a str,
        crate::Environment,
        &'a str,
        &'a str,
        Vec<(&'a str, &'a str, &'a str, bool, bool)>,
    );

    /// Every field of a batch, as one comparable value.
    ///
    /// A credential has no equality and neither, therefore, has a batch made of them, so a test
    /// comparing two of them spells out what it is comparing. Written as a whole rather than as an
    /// assertion per field so that a failure prints both batches.
    fn fields(batch: &StoredCredentials) -> Fields<'_> {
        (
            &batch.order_id,
            batch.environment,
            &batch.item_id,
            &batch.issuer,
            batch
                .credentials
                .iter()
                .map(|c| {
                    (
                        c.unblinded.expose(),
                        c.valid_from.as_str(),
                        c.valid_to.as_str(),
                        c.spent,
                        c.rfc,
                    )
                })
                .collect(),
        )
    }

    fn batch() -> StoredCredentials {
        StoredCredentials {
            order_id: "aaaaaaaa-1111-4222-8333-444444444444".to_string(),
            environment: crate::Environment::Production,
            item_id: "b7114ccc-b3a5-4951-9a5d-8b7a28731111".to_string(),
            issuer: "brave.com?sku=brave-leo-premium".to_string(),
            credentials: vec![
                Credential {
                    unblinded: crate::Secret::new("token-one"),
                    valid_from: "2026-08-22T00:00:00".to_string(),
                    valid_to: "2026-08-23T00:00:00".to_string(),
                    spent: false,
                    rfc: true,
                },
                Credential {
                    unblinded: crate::Secret::new("token-two"),
                    valid_from: "2026-08-23T00:00:00".to_string(),
                    valid_to: "2026-08-24T00:00:00".to_string(),
                    spent: false,
                    rfc: true,
                },
            ],
        }
    }

    /// The reason Wallet exists: spending must not write, because a credential is spent per model
    /// request and each write rewrites the whole batch.
    ///
    /// Asserted through the dirty flag, which is what decides whether a write happens at all.
    #[test]
    fn spending_does_not_write_until_asked_to() {
        let mut wallet = Wallet::detached(batch());
        assert!(!wallet.dirty, "a freshly opened batch has nothing to write");

        wallet
            .spend("2026-08-22T12:00:00")
            .expect("a usable credential");
        assert!(wallet.dirty, "a spend must be recorded for the next flush");
        assert_eq!(wallet.remaining(), 1);
    }

    /// A session that spends nothing must never write, so opening the agent and not using premium
    /// leaves the stored batch untouched.
    ///
    /// Asserted on a wallet with a real file behind it, and on the file's own bytes: a detached
    /// wallet has no destination to write to, so nothing about one can tell whether a write was
    /// declined or merely impossible.
    #[test]
    fn a_session_that_spends_nothing_never_writes() {
        with_temp_home("idle", || {
            save(&batch()).expect("a write");
            let path = path().expect("a path");

            // Held in a shape no write here produces, so a needless rewrite shows up as a change
            // rather than landing on the same bytes.
            let value: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("a read"))
                    .expect("what save wrote");
            let spaced = serde_json::to_string_pretty(&value).expect("a spaced form");
            std::fs::write(&path, &spaced).expect("a rewrite");

            {
                let mut wallet = Wallet::open().expect("the batch just written");
                wallet.flush().expect("flushing nothing is a no-op");
                assert!(!wallet.dirty);
            }
            // Dropped by here, and Drop flushes too.

            assert_eq!(
                std::fs::read_to_string(&path).expect("a read"),
                spaced,
                "an idle session must leave the file exactly as it found it"
            );
        });
    }

    /// A spend reaches the file when the wallet is flushed and not before, which is the whole reason
    /// the batch is held open: one credential is spent per model request, and writing per spend
    /// would rewrite hundreds of credentials to change one boolean.
    #[test]
    fn a_spend_is_written_back_only_on_a_flush() {
        with_temp_home("flush", || {
            save(&batch()).expect("a write");
            let path = path().expect("a path");
            let written = std::fs::read_to_string(&path).expect("a read");

            let mut wallet = Wallet::open().expect("the batch just written");
            wallet
                .spend("2026-08-22T12:00:00")
                .expect("a usable credential");
            assert_eq!(
                std::fs::read_to_string(&path).expect("a read"),
                written,
                "spending alone must not touch the file"
            );

            wallet.flush().expect("the write back");
            let stored = load().expect("a read");
            assert!(
                stored.credentials[0].spent,
                "the flush must record which credential was spent"
            );
            assert_eq!(stored.remaining(), 1);
        });
    }

    /// A session that ends without a flush of its own must still have its spends recorded. The
    /// credential went out with a request the moment it was spent, so a batch read back next session
    /// with that credential still unspent would offer it to a second request.
    #[test]
    fn a_spend_is_written_back_when_the_session_ends() {
        with_temp_home("ends", || {
            save(&batch()).expect("a write");

            {
                let mut wallet = Wallet::open().expect("the batch just written");
                wallet
                    .spend("2026-08-22T12:00:00")
                    .expect("a usable credential");
            }
            // Dropped here, having been asked to flush nothing.

            assert!(
                load().expect("a read").credentials[0].spent,
                "a credential spent in a session that ended must not be offered again"
            );
        });
    }

    /// A detached batch must have no destination at all, which is what lets these tests run
    /// anywhere: a test that could reach the real store would overwrite the credentials of whoever
    /// ran it.
    #[test]
    fn a_detached_batch_has_nowhere_to_write() {
        let mut wallet = Wallet::detached(batch());
        assert!(wallet.destination.is_none());

        wallet
            .spend("2026-08-22T12:00:00")
            .expect("a usable credential");
        assert!(wallet.dirty, "the spend is recorded in memory");

        // Flushing a dirty detached batch is still a no-op, so neither this nor Drop can write.
        wallet.flush().expect("a detached flush cannot fail");
        assert!(wallet.dirty, "and it stays unwritten");
    }

    /// Two spends in one session must hand out different credentials: the whole batch is held in
    /// memory, so an index that did not advance would replay the same one every request.
    #[test]
    fn consecutive_spends_hand_out_different_credentials() {
        let mut batch = batch();
        // Both windows cover the same moment, so the only thing separating them is the spent mark.
        batch.credentials[1].valid_from = batch.credentials[0].valid_from.clone();
        batch.credentials[1].valid_to = batch.credentials[0].valid_to.clone();

        let mut wallet = Wallet::detached(batch);
        let first = wallet.spend("2026-08-22T12:00:00").expect("first");
        let second = wallet.spend("2026-08-22T12:00:00").expect("second");

        assert_ne!(
            first.credential.unblinded.expose(),
            second.credential.unblinded.expose()
        );
        assert_eq!(second.remaining, 0);
    }

    /// Once every credential in the window is spent, further requests must be refused rather than
    /// replaying one the server has already seen.
    #[test]
    fn spending_past_the_end_of_the_batch_is_refused() {
        let mut wallet = Wallet::detached(batch());
        wallet
            .spend("2026-08-22T12:00:00")
            .expect("the one usable credential");
        assert!(matches!(
            wallet.spend("2026-08-22T12:00:00"),
            Err(StoreError::Exhausted)
        ));
    }

    /// A batch is printed by whatever holds it: an error path formatting a wallet, a panic
    /// unwinding through one, a trace line somebody adds later. The token is a bearer value, so
    /// what a derived `Debug` puts in that output is the credential itself.
    #[test]
    fn a_stored_batch_does_not_print_its_tokens() {
        let printed = format!("{:?}", batch());

        assert!(
            !printed.contains("token-one") && !printed.contains("token-two"),
            "a token is in the printed form of the batch: {printed}"
        );
        assert!(
            printed.contains("brave-leo-premium"),
            "the rest of the batch is still printed: {printed}"
        );
    }

    #[test]
    fn a_batch_survives_a_round_trip_through_the_stored_form() {
        let decoded = decode(encode(&batch()).expose()).unwrap();
        assert_eq!(fields(&decoded), fields(&batch()));
    }

    /// One subscription means one stored batch: importing from another channel must replace what
    /// was there, not sit beside it. Two files meant a stale batch from an uninstalled channel kept
    /// being reported, and a load picking whichever it reached first.
    #[test]
    fn importing_again_replaces_the_previous_batch() {
        with_temp_home("replace", || {
            save(&batch()).expect("a write");

            // A second import, as switching channels produces: same order, different tokens.
            let mut second = batch();
            second.credentials[0].unblinded = crate::Secret::new("from-the-other-channel");
            save(&second).expect("a second write");

            let loaded = load().expect("a read");
            assert_eq!(
                fields(&loaded),
                fields(&second),
                "the newer import must win"
            );
            assert_eq!(
                loaded.credentials.len(),
                second.credentials.len(),
                "the batches must not have accumulated"
            );
        });
    }

    /// The point of moving off the keychain: a batch must survive a write and read back with no
    /// secret store involved, on a machine with no desktop session.
    #[test]
    fn a_batch_written_to_the_file_is_read_back() {
        with_temp_home("roundtrip", || {
            assert!(matches!(load(), Err(StoreError::NotFound)));

            save(&batch()).expect("a write");
            assert_eq!(fields(&load().expect("a read")), fields(&batch()));
        });
    }

    /// The file holds a bearer secret, so it must not be readable by other users on the machine.
    /// Checked on the real file rather than trusted from the open flags, since an existing file
    /// keeps its own mode and the truncating reopen is the easy way to lose this.
    ///
    /// Unix only: there is no mode to read on Windows, and the extension that reads one is not on
    /// that target.
    #[cfg(unix)]
    #[test]
    fn the_file_is_not_readable_by_anyone_else() {
        with_temp_home("mode", || {
            use std::os::unix::fs::PermissionsExt;

            save(&batch()).expect("a write");
            // Written twice: the second open finds the file already there, which is the case that
            // would silently keep a wider mode set by something else.
            save(&batch()).expect("a second write");

            let path = path().expect("a home");
            let mode = std::fs::metadata(&path)
                .expect("the file")
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o077,
                0,
                "group or other can reach {}",
                path.display()
            );
        });
    }

    /// A mode asked for at creation says nothing about a file already on disk, and this one may be
    /// there from a backup or copied between machines. It holds a bearer token, so importing over
    /// it has to narrow it rather than keep the mode it was found with.
    #[cfg(unix)]
    #[test]
    fn a_file_left_readable_by_something_else_is_narrowed() {
        with_temp_home("file-mode-narrowed", || {
            use std::os::unix::fs::PermissionsExt;

            let path = path().expect("a home");
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
            std::fs::write(&path, "{}").expect("a write");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
                .expect("loosen");

            save(&batch()).expect("a write");

            let mode = std::fs::metadata(&path)
                .expect("the file")
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o077,
                0,
                "group or other can read {}",
                path.display()
            );
        });
    }

    /// The state directory is shared with the prompt history and the session records, and whichever
    /// subsystem writes first is the one that creates it. A credential store that made it at the
    /// umask would decide the mode for everything else kept there.
    #[cfg(unix)]
    #[test]
    fn the_directory_it_is_kept_in_is_not_reachable_by_anyone_else() {
        with_temp_home("parent-mode", || {
            use std::os::unix::fs::PermissionsExt;

            let parent = path()
                .expect("a home")
                .parent()
                .expect("a parent")
                .to_path_buf();
            // As a build with no opinion about modes would have left it, which is the case that
            // has to be narrowed rather than kept.
            std::fs::create_dir_all(&parent).expect("the directory");
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755))
                .expect("loosen");

            save(&batch()).expect("a write");

            let mode = std::fs::metadata(&parent)
                .expect("the directory")
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o077,
                0,
                "group or other can reach {}",
                parent.display()
            );
        });
    }

    /// STATE-1 bounds the narrowing at the state directory by comparing names, which says where a
    /// directory sits and nothing about where it leads. Somebody who keeps their sessions on a
    /// synced volume links `~/.bravebot` into place, and `set_permissions` resolves that link, so
    /// narrowing through one has importing a subscription setting the mode of a directory this
    /// program was never given.
    #[cfg(unix)]
    #[test]
    fn narrowing_does_not_follow_a_linked_state_directory() {
        with_temp_home("parent-link", || {
            use std::os::unix::fs::PermissionsExt;

            let parent = path()
                .expect("a home")
                .parent()
                .expect("a parent")
                .to_path_buf();
            // The synced volume: outside the state directory, and at a mode of its owner's
            // choosing rather than one this program decided.
            let elsewhere = parent.with_file_name("elsewhere");
            std::fs::create_dir_all(&elsewhere).expect("the directory");
            std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755))
                .expect("loosen");
            std::os::unix::fs::symlink(&elsewhere, &parent).expect("link");

            save(&batch()).expect("a write");

            let mode = std::fs::metadata(&elsewhere)
                .expect("the directory")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(
                mode,
                0o755,
                "a directory outside the state directory was narrowed through a link: {}",
                elsewhere.display()
            );
        });
    }

    /// PREM-7: a Windows file has no mode, so what keeps this one to the account that imported it is
    /// the access-control list it is created with. One entry is one account granted, and `P` protects
    /// the list, so an inheritable entry carried by the directory above (`Users`, on a shared or a
    /// roaming profile volume) is not added to it. Losing either would write a bearer token every
    /// account on the machine can read.
    ///
    /// Asserted on the list rather than on a file written on Windows because the suite is not run on
    /// that target at all: `make check-windows` compiles and lints it and stops there.
    #[test]
    fn the_windows_list_grants_one_account_and_inherits_nothing() {
        // An account as `ConvertSidToStringSidW` writes the one this process runs as.
        let account = "S-1-5-21-2127521184-1604012920-1887927527-72713";
        assert_eq!(
            dacl_granting_only(account),
            "D:P(A;;FA;;;S-1-5-21-2127521184-1604012920-1887927527-72713)"
        );
    }

    /// Discarding an import must remove the secret from disk, and asking twice is not an error:
    /// `--forget` on a machine that never imported is a request that is already satisfied.
    #[test]
    fn forgetting_removes_the_file_and_is_repeatable() {
        with_temp_home("forget", || {
            save(&batch()).expect("a write");
            let path = path().expect("a home");
            assert!(path.exists());

            clear().expect("a clear");
            assert!(!path.exists(), "the secret is still on disk");
            clear().expect("clearing nothing is not an error");
        });
    }

    /// The store is under the user's own directory, not the working directory, so credentials do
    /// not depend on where the agent was started from.
    #[test]
    fn the_file_lives_in_the_users_own_directory() {
        with_temp_home("location", || {
            let path = path().expect("a home");
            let home = std::env::var("HOME").expect("a home");
            assert!(path.starts_with(&home), "{}", path.display());
            assert!(path.starts_with(PathBuf::from(&home).join(DIRECTORY)));
        });
    }

    /// With no home there is nowhere a secret belongs, and inventing one would write it somewhere
    /// the user never chose. Reported rather than treated as nothing imported, since a paid
    /// subscription silently unused is what PREM-8 exists to prevent.
    #[test]
    fn no_home_directory_is_reported_rather_than_guessed() {
        with_no_home(|| {
            assert!(matches!(path(), Err(StoreError::Unusable { .. })));
            assert!(matches!(load(), Err(StoreError::Unusable { .. })));
        });
    }

    /// A profile directory as the environment hands one over.
    fn named(value: &str) -> Option<std::ffi::OsString> {
        Some(std::ffi::OsString::from(value))
    }

    /// PREM-7: stock Windows sets no `HOME`, so the batch imported there belongs under the profile
    /// directory the platform does name. Nowhere to keep it means an import that cannot be kept, and
    /// a machine paying for a subscription spending it on nothing.
    #[test]
    fn the_profile_directory_answers_where_no_home_is_named() {
        let expected = PathBuf::from("C:\\Users\\someone")
            .join(DIRECTORY)
            .join(FILE);
        // In the order `PROFILE_VARIABLES` names them: no `HOME`, then the profile directory stock
        // Windows names in `USERPROFILE`.
        assert_eq!(
            path_named([None, named("C:\\Users\\someone")]).expect("a profile directory"),
            expected
        );
        assert_eq!(
            path_named([named(""), named("C:\\Users\\someone")]).expect("a profile directory"),
            expected,
            "a variable exported empty took away a profile directory the platform names"
        );
    }

    /// PREM-7: a shell environment that sets `HOME` has been told where the profile is, and the file
    /// is read back by whatever is started from that shell next. A secret written somewhere else
    /// would leave a subscription imported in one session unspendable in the next.
    #[test]
    fn a_named_home_answers_before_the_profile_directory() {
        assert_eq!(
            path_named([named("/somebody"), named("C:\\Users\\someone")])
                .expect("a profile directory"),
            PathBuf::from("/somebody").join(DIRECTORY).join(FILE)
        );
    }

    /// A file holding something another version wrote must be reported, not read as absent: the
    /// remedy is re-importing, and treating it as absent would stop the subscription being spent
    /// without a word.
    #[test]
    fn a_file_that_is_not_json_is_reported_when_loaded() {
        with_temp_home("garbage", || {
            let path = path().expect("a home");
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
            std::fs::write(&path, "not json at all").expect("a write");

            assert!(matches!(load(), Err(StoreError::Malformed { .. })));
        });
    }

    /// A credential is single-use, so the one presented must be valid *now*: a batch covers
    /// months of daily windows and most of it is not usable on any given day.
    #[test]
    fn the_next_usable_credential_is_the_one_valid_at_that_moment() {
        let batch = batch();
        assert_eq!(batch.next_usable("2026-08-22T12:00:00"), Some(0));
        assert_eq!(batch.next_usable("2026-08-23T12:00:00"), Some(1));
    }

    #[test]
    fn a_spent_credential_is_never_offered_again() {
        let mut batch = batch();
        batch.credentials[0].spent = true;
        assert_eq!(batch.next_usable("2026-08-22T12:00:00"), None);
        assert_eq!(batch.remaining(), 1);
    }

    /// Before the first window and after the last, there is nothing to present.
    #[test]
    fn a_moment_outside_every_window_yields_no_credential() {
        let batch = batch();
        assert_eq!(batch.next_usable("2026-08-21T23:59:59"), None);
        assert_eq!(batch.next_usable("2026-09-01T00:00:00"), None);
    }

    /// The end of a window is exclusive, so the credential that expires exactly now is not used.
    #[test]
    fn a_window_does_not_include_its_own_end() {
        let batch = batch();
        assert_eq!(batch.next_usable("2026-08-23T00:00:00"), Some(1));
    }

    /// The environment must survive a round trip, since a refill uses it to pick the service and
    /// the browser it came from may be gone by then.
    #[test]
    fn the_environment_survives_a_round_trip() {
        let mut staging = batch();
        staging.environment = crate::Environment::Staging;
        assert_eq!(
            decode(encode(&staging).expose()).unwrap().environment,
            crate::Environment::Staging
        );
    }

    /// An interrupted write can leave the file present and holding nothing, which is a different
    /// fact from never having imported: a subscription was paid for and is not being spent. Read as
    /// absent it costs the user the model they chose with nothing said about it, so the emptiness is
    /// reported and the message names the one thing that fixes it.
    #[test]
    fn an_empty_file_is_reported_rather_than_read_as_absent() {
        for raw in ["", "   "] {
            let err = decode(raw).unwrap_err();
            assert!(matches!(err, StoreError::Malformed { .. }), "{err}");
            assert!(err.to_string().contains("import-leo-creds"), "{err}");
        }
    }

    #[test]
    fn a_batch_that_is_not_json_is_reported_as_malformed() {
        assert!(matches!(
            decode("not json").unwrap_err(),
            StoreError::Malformed { .. }
        ));
    }

    /// A credential with no token would fail at presentation time with something obscure, so it
    /// is rejected while there is still context to report.
    #[test]
    fn a_credential_without_a_token_is_rejected_on_load() {
        let raw = serde_json::json!({
            "version": 1,
            "order_id": "o", "item_id": "i", "issuer": "x",
            "credentials": [{ "valid_from": "a", "valid_to": "b", "spent": false }],
        })
        .to_string();
        assert!(matches!(
            decode(&raw).unwrap_err(),
            StoreError::Malformed { .. }
        ));
    }

    #[test]
    fn an_entry_missing_its_order_is_reported_as_malformed() {
        let raw = serde_json::json!({ "version": 1, "credentials": [] }).to_string();
        assert!(matches!(
            decode(&raw).unwrap_err(),
            StoreError::Malformed { .. }
        ));
    }
}
