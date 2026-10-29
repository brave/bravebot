//! Files kept under `~/.bravebot` that no other account can read.
//!
//! The helpers the sign-ins in this crate keep their tokens with: the directory they live in, a
//! file created private to the account before anything is written to it, a write that replaces the
//! file whole, and the claim a writer holds while it does.
//!
//! # Why not the OS keychain
//!
//! These are bearer secrets, so the keychain looks like the obvious home for them, and it was that
//! for a while. It was the wrong call on both halves of the trade.
//!
//! Nor does it hold against the threat it was written for, which was a program `run` launches
//! reading the file. Those are deliberately unconfined (RUN-10), and the AWS credentials that sign
//! every model request are cached by the `aws` CLI in plain 0600 JSON, so anything able to read a
//! file here can already take the larger secret. Guarding the smaller one behind the keychain
//! bought a password dialog, not a boundary.
//!
//! What it did cost was availability. The keychain crate builds one Linux backend, the D-Bus
//! Secret Service, so a machine reached over SSH with no desktop session had no store to open at
//! all.

use std::path::Path;
use std::path::PathBuf;

/// The directory inside the user's home the files are kept in.
const DIRECTORY: &str = ".bravebot";

/// Why a file could not be reached or written.
#[derive(Debug)]
pub enum StoreError {
    /// The directory could not be named, or a file in it could not be read or written.
    Unusable { detail: String },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unusable { detail } => f.write_str(detail),
        }
    }
}

impl std::error::Error for StoreError {}

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

/// The variable that names the state directory itself, ahead of the profile directory's.
///
/// Spelled here for the same reason as [`PROFILE_VARIABLES`]. A value that is empty or not an
/// absolute path names nothing and is passed over, so the credentials are never put under wherever
/// the process happened to start.
const STATE_VARIABLE: &str = "BRAVEBOT_HOME";

/// The directory the files are kept in.
///
/// The variables are read directly rather than through a dependency, for the same reason and with
/// the same absence of a fallback as everywhere else the directory is resolved: inventing one would
/// put a bearer secret somewhere the user never chose.
fn directory() -> Result<PathBuf, StoreError> {
    directory_stated(
        std::env::var_os(STATE_VARIABLE),
        PROFILE_VARIABLES.iter().map(std::env::var_os),
    )
}

/// A file named `file` in the directory the sign-ins are kept in.
///
/// The directory is decided once, by the rules above, so two files of one person's credentials
/// cannot end up in two places.
pub(crate) fn beside(file: &str) -> Result<PathBuf, StoreError> {
    Ok(directory()?.join(file))
}

/// The same answer from the values, `stated` being what [`STATE_VARIABLE`] holds.
///
/// The directory it names is the one the files are kept in, and a value that names nothing falls
/// through to the profile directory's.
fn directory_stated(
    stated: Option<std::ffi::OsString>,
    named: impl IntoIterator<Item = Option<std::ffi::OsString>>,
) -> Result<PathBuf, StoreError> {
    match stated
        .map(PathBuf::from)
        .filter(|stated| stated.is_absolute())
    {
        Some(stated) => Ok(stated),
        None => directory_named(named),
    }
}

/// The same answer, from the values rather than from the variables.
///
/// Split from the read so the order they answer in is testable without a process-wide variable,
/// which here also means without the `unsafe` block setting one takes.
///
/// The first value that names something wins. An empty one names nothing, so it is passed over
/// rather than joined onto: joining would put a bearer secret in `/.bravebot`, and stopping there
/// would lose a profile directory the platform does name to a variable some shell exported empty.
fn directory_named(
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
    Ok(PathBuf::from(home).join(DIRECTORY))
}

/// The claim taken while a sign-in's file is read and written, so two of them cannot interleave.
///
/// Nothing bounds how many `bravebot` processes a person runs, so the file is the only place they
/// can agree about who is writing it. The claim is a file created with `create_new`, which is one
/// atomic operation on every filesystem this runs on and needs nothing added to a crate that
/// depends on nothing.
///
/// It holds no bytes. What it says, it says by existing, so a sign-in's file is still the one file
/// its credentials are kept in.
pub(crate) struct Claim {
    path: PathBuf,
}

/// How long to leave a claim that is not yet abandoned before looking again.
const LOOK_AGAIN_IN: std::time::Duration = std::time::Duration::from_millis(10);

/// When to stop trying, which only means anything for longer than the age a claim is judged
/// abandoned at: a claim held past that is broken below, so reaching here means the break itself is
/// not working and waiting on is no longer the answer.
const GIVE_UP_AFTER: std::time::Duration = std::time::Duration::from_secs(15);

impl Claim {
    /// Take the claim named `name` beside `store`, waiting for whoever holds it. It is judged
    /// abandoned once it is `abandoned_after` old.
    ///
    /// A file whose hold is longer than one local read and write, such as one that is renewed over
    /// a network round trip, states a longer age: a claim broken while its holder still waits for a
    /// reply lets a second holder in beside it.
    pub(crate) fn take_as(
        store: &Path,
        name: &str,
        abandoned_after: std::time::Duration,
    ) -> Result<Self, StoreError> {
        prepare_directory(store)?;
        let path = store.with_file_name(name);
        let waiting_since = std::time::Instant::now();
        loop {
            match create_claim(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    return Err(StoreError::Unusable {
                        detail: format!("{}: {e}", path.display()),
                    });
                }
            }
            if waiting_since.elapsed() >= GIVE_UP_AFTER.max(abandoned_after * 3) {
                return Err(StoreError::Unusable {
                    detail: format!(
                        "{} is held by another bravebot and did not come free",
                        path.display()
                    ),
                });
            }
            if abandoned(&path, abandoned_after) {
                break_abandoned(&path, abandoned_after);
            }
            std::thread::sleep(LOOK_AGAIN_IN);
        }
    }

    /// Make this claim as young as a new one.
    ///
    /// A holder whose work under the claim is more than one step that waits on the network calls this
    /// between the steps, so that each step starts under a claim of the age a new one has and none of
    /// them is the one the age of abandonment lands in. A claim another process broke and removed is
    /// not made again by this, and one whose time cannot be set keeps the age it has.
    pub(crate) fn touch(&self) {
        std::fs::OpenOptions::new()
            .write(true)
            .open(&self.path)
            .and_then(|file| file.set_modified(std::time::SystemTime::now()))
            .ok();
    }
}

/// Create the claim file at `path`, failing if it is already there.
///
/// Created 0600 as it is made: the file holds no bytes, but it is written into the state directory,
/// and STATE-1 asks for the mode as the file is created rather than leaving it to the directory
/// above. It is new whenever this succeeds, so there is no earlier mode to narrow.
#[cfg(unix)]
fn create_claim(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// Create the claim file at `path`, failing if it is already there.
///
/// Windows has no mode to ask for, and the claim holds no bytes.
#[cfg(not(unix))]
fn create_claim(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Whether the claim at `path` is old enough to have been left behind by a process that died.
///
/// A modification time that cannot be read, or one in the future, is not old: a clock disagreeing
/// with the one that wrote the file is a reason to wait rather than a reason to take it.
fn abandoned(path: &Path, after: std::time::Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|found| found.modified())
        .ok()
        .and_then(|written| written.elapsed().ok())
        .is_some_and(|age| age >= after)
}

/// Remove the claim at `path` if what is there is abandoned, and leave a live one in place.
///
/// Judging a claim abandoned and removing it are two steps, and the claim can change between them:
/// another process that judged the same one abandoned breaks it and takes a claim of its own. A
/// removal by name would then delete that live claim and let a third process in beside its holder.
/// So the claim is moved aside first, which only one process can do to a given file, and judged
/// there, where nothing can replace it. A live claim found that way is put back as a new file, which
/// can only make it look younger.
///
/// The name is free between the move and the put-back. A third process that takes it in that gap
/// holds beside the claim's owner, which needs that process to arrive within those two calls of a
/// claim being broken. An owner that finishes in the gap removes nothing, and the claim put back
/// is then nobody's until it ages out after `after`.
fn break_abandoned(path: &Path, after: std::time::Duration) {
    static MOVED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(
        ".{}.{}",
        std::process::id(),
        MOVED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let aside = path.with_file_name(name);
    if std::fs::rename(path, &aside).is_err() {
        return;
    }
    let was_abandoned = abandoned(&aside, after);
    let _ = std::fs::remove_file(&aside);
    if !was_abandoned {
        let _ = create_claim(path);
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Make the directory the credentials live in, reachable by this account and no other.
///
/// The state directory is made by whichever subsystem writes to it first, so one that made it at
/// the umask would decide the mode for the prompt history and the session records too. Spelled out
/// here rather than shared with the crates that have their own helper for it: this crate depends on
/// nothing, and an auth-only crate is not worth a dependency for four lines.
#[cfg(unix)]
fn prepare_directory(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::DirBuilderExt;
    use std::os::unix::fs::PermissionsExt;

    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|e| StoreError::Unusable {
            detail: format!("{}: {e}", parent.display()),
        })?;
    // A link is stepped over rather than followed. `set_permissions` resolves one, and the name
    // this was given says where the directory sits and nothing about where it leads: somebody who
    // keeps their sessions on a synced volume and links `~/.bravebot` into place would otherwise
    // have a sign-in setting the mode of the volume's directory, which is outside
    // anything this program was given.
    let is_link =
        std::fs::symlink_metadata(parent).is_ok_and(|found| found.file_type().is_symlink());
    if !is_link {
        let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// Make the directory the credentials live in.
///
/// It is left with whatever the profile directory grants it, which is the cost
/// [state-directory.md](../../../docs/specs/state-directory.md) records for a platform where the
/// mode is not there to be set: what else is kept in there is decided by the crates that write it,
/// not by this one. The file's own list does not depend on the directory's, which is why
/// [`create_private`] protects it.
#[cfg(not(unix))]
fn prepare_directory(path: &Path) -> Result<(), StoreError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|e| StoreError::Unusable {
        detail: format!("{}: {e}", parent.display()),
    })
}

/// Create `path`, empty, reachable by the account this process runs as and by no other.
///
/// Created 0600 before anything is written to it, rather than written and then chmod'ed: the other
/// order leaves the secret world-readable for the moment in between.
///
/// Public because this is the one crate whose root may name the Win32 calls the Windows version
/// makes, and the gateway keys `bravebot auth login gateway` stores are a bearer token too.
#[cfg(unix)]
pub fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::fs::PermissionsExt;

    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    // The mode above is asked for as the file is created and says nothing about one already there,
    // which is what a temporary a killed write left behind is. Narrowed after the open and before
    // the write, so the file is private before it holds a token.
    let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    Ok(file)
}

/// Create `path`, empty, reachable by the account this process runs as and by no other.
///
/// Windows has no mode, so what says the same thing is an access-control list, and the list is asked
/// for as the file is created rather than set afterwards: the other order leaves the secret readable
/// by whatever the directory grants for the moment in between. [`dacl_granting_only`] is the list.
#[cfg(windows)]
pub fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
    acl::create_granted_to_this_account_only(path)
}

/// The access-control list a credential file is kept under, written as SDDL.
///
/// `D:` opens the list and `P` protects it, so nothing the directory above offers is added to what
/// follows: a profile directory on a shared or a roaming volume can carry an inheritable entry for
/// `Users`, and a list that took it would hand a bearer token to every account on the machine. `A`
/// allows and `FA` is full access to a file, so the one entry is the one account, and an account
/// with no entry in a protected list is granted nothing.
///
/// A string handed to the platform's parser rather than a list built entry by entry, so that what is
/// granted is decided by something a test can call anywhere. Nothing else in [`create_private`] can
/// be exercised off Windows, and `make check-windows` compiles it without running it.
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
/// Reading stays available: an existing file is no less safe for being read, and one copied from
/// elsewhere should still work here.
#[cfg(not(any(unix, windows)))]
pub fn create_private(_path: &Path) -> std::io::Result<std::fs::File> {
    Err(std::io::Error::other(
        "this platform has no way to restrict the file to your account, and the credentials are a \
         bearer token, so they were not written",
    ))
}

/// Fill a temporary beside `destination` with `fill` and rename it over, removing the temporary
/// when any step fails.
///
/// The temporary holds tokens, so one left by a failed write (a full disk, for one) would stay in
/// the directory, and each failing process would leave another named for its id.
pub(crate) fn write_through(
    destination: &Path,
    fill: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
) -> Result<(), StoreError> {
    // Built as an OS string: a name that is not UTF-8 stays the name it is, so two such names do not
    // share a temporary.
    let mut name = destination.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let temporary = destination.with_file_name(name);
    let placed = stage_and_rename(&temporary, destination, fill);
    if placed.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    placed
}

/// The steps of [`write_through`] that can fail, in order: create the temporary, fill it, rename it.
fn stage_and_rename(
    temporary: &Path,
    destination: &Path,
    fill: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
) -> Result<(), StoreError> {
    let unusable = |detail: String| StoreError::Unusable { detail };

    let mut file = create_private(temporary)
        .map_err(|e| unusable(format!("{}: {e}", destination.display())))?;
    fill(&mut file).map_err(|e| unusable(format!("{}: {e}", temporary.display())))?;
    // Closed before the rename, which Windows refuses while a handle is still open on either name.
    drop(file);

    std::fs::rename(temporary, destination)
        .map_err(|e| unusable(format!("{}: {e}", destination.display())))
}

/// The Win32 calls behind the Windows [`create_private`], the only unsafe in this crate outside the
/// helper the tests point `HOME` with.
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
        // nothing about one already there, which here is a temporary a killed write left behind
        // carrying whatever the directory it was made in offers, an inheritable entry for `Users`
        // included. So it is set again on the handle, before anything is written.
        descriptor.apply_to(&file)?;
        // Emptied once the list is settled rather than by the open, so that nothing is written into
        // a file whose list this could not settle.
        file.set_len(0)?;
        Ok(file)
    }

    /// Open `path` for writing, creating it if it is not there, asking for `descriptor` as it is
    /// created.
    #[allow(unsafe_code, reason = "calls `CreateFileW`")]
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
    #[allow(
        unsafe_code,
        reason = "calls `OpenProcessToken`, `GetTokenInformation`, `ConvertSidToStringSidW` and `LocalFree`"
    )]
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
        #[allow(
            unsafe_code,
            reason = "calls `ConvertStringSecurityDescriptorToSecurityDescriptorW`"
        )]
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
        #[allow(
            unsafe_code,
            reason = "calls `GetSecurityDescriptorDacl` and `SetSecurityInfo`"
        )]
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
        #[allow(
            unsafe_code,
            reason = "calls `LocalFree` on the descriptor this value owns"
        )]
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
    #[allow(
        unsafe_code,
        reason = "`set_var` and `remove_var` are unsafe, and `HOME_LOCK` is held while they run and until restored"
    )]
    fn with_home<T>(dir: Option<PathBuf>, body: impl FnOnce() -> T) -> T {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(dir) = &dir {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir_all(dir).expect("scratch home");
        }

        // Every variable the state directory is stated in, not `HOME` alone: one left set would
        // answer where `HOME` does not, which would have `with_no_home` asking what happens when one
        // variable is missing rather than when there is no home at all, and a `BRAVEBOT_HOME` in the
        // developer's environment would send every test to the credentials it names.
        let previous: Vec<_> = std::iter::once(&STATE_VARIABLE)
            .chain(PROFILE_VARIABLES)
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

    /// The file the tests write, standing for any sign-in's.
    const FILE: &str = "sign-in.json";

    /// The claim they take beside it.
    const CLAIM: &str = "sign-in.lock";

    /// How old a claim has to be before these tests take it for a dead process's.
    const ABANDONED_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

    fn path() -> Result<PathBuf, StoreError> {
        beside(FILE)
    }

    /// Write a small file at [`path`] under a claim, as a sign-in's writer does.
    fn save() -> Result<(), StoreError> {
        use std::io::Write;

        let path = path()?;
        let _claim = Claim::take_as(&path, CLAIM, ABANDONED_AFTER)?;
        write_through(&path, |file| file.write_all(b"{}"))
    }

    /// The file is replaced rather than opened and truncated, so a write killed partway leaves the
    /// last good file rather than an empty one.
    ///
    /// An empty file is reported as unusable, and the tokens in it are recoverable from nowhere
    /// else, so the cost of landing in that window is signing in again. What a test can see of
    /// "replaced" is the inode, which changes when a rename puts a different file at the name and
    /// does not when the file at it is opened and emptied.
    #[cfg(unix)]
    #[test]
    fn a_write_replaces_the_file_rather_than_truncating_it() {
        use std::os::unix::fs::MetadataExt;

        with_temp_home("replaced", || {
            save().expect("a write");
            let path = path().expect("a path");
            let before = std::fs::metadata(&path).expect("a stat").ino();

            save().expect("a second write");

            assert_ne!(
                before,
                std::fs::metadata(&path).expect("a stat").ino(),
                "the file was written into the file that held it, so a write killed partway \
                 leaves nothing to read"
            );
        });
    }

    /// A write leaves nothing beside the credentials: no temporary and no claim.
    ///
    /// The temporary holds every token in the file, and a claim left behind is what a later run
    /// waits on.
    #[test]
    fn a_write_leaves_neither_a_temporary_nor_a_claim_behind() {
        with_temp_home("leftovers", || {
            save().expect("a write");

            let path = path().expect("a path");
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
        });
    }

    /// A write that fails partway leaves the last good file in place and no temporary beside it.
    ///
    /// The temporary is either renamed over the file or removed. The partial temporary holds some of
    /// the tokens, and a failing process leaves one named for its id each time it runs, so a full
    /// disk would fill the directory with them.
    #[test]
    fn a_write_that_fails_partway_removes_its_temporary_and_keeps_the_last_file() {
        use std::io::Write;

        with_temp_home("failed-write", || {
            save().expect("a write");
            let path = path().expect("a path");
            let before = std::fs::read(&path).expect("the file just written");

            let failed = write_through(&path, |file| {
                file.write_all(b"{\"credentials\": [")?;
                Err(std::io::Error::other("no space left on device"))
            });

            assert!(failed.is_err(), "a failed write was reported as written");
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
            assert_eq!(left, [FILE], "a temporary was left beside the credentials");
            assert_eq!(
                std::fs::read(&path).expect("the file"),
                before,
                "the last good file was changed by a write that failed"
            );
        });
    }

    /// The claim is a file in the state directory, so STATE-1 asks for it at 0600 as it is created.
    /// The directory above is 0700, which is a second answer and not this one: a claim created at
    /// the process umask is readable by group and other the moment the directory is ever loosened.
    #[cfg(unix)]
    #[test]
    fn a_claim_is_created_readable_only_by_its_owner() {
        with_temp_home("claim-mode", || {
            use std::os::unix::fs::PermissionsExt;

            let store = path().expect("a path");
            let claim = Claim::take_as(&store, CLAIM, ABANDONED_AFTER).expect("a claim");

            let mode = std::fs::metadata(&claim.path)
                .expect("the claim")
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o077,
                0,
                "group or other can reach {}",
                claim.path.display()
            );
        });
    }

    /// A claim a process died holding does not cost the person their sign-in.
    ///
    /// Nothing removes a claim but the process that took it, so one killed while holding it leaves
    /// the file there for ever. Waiting on that would leave every later run unable to renew the
    /// sign-in, with nothing on screen naming the file to delete.
    #[test]
    fn a_claim_left_by_a_dead_process_is_broken_rather_than_waited_on() {
        with_temp_home("dead-claim", || {
            save().expect("a write");
            let claim = path().expect("a path").with_file_name(CLAIM);
            std::fs::File::create(&claim)
                .expect("a claim")
                .set_modified(std::time::SystemTime::now() - ABANDONED_AFTER * 2)
                .expect("a claim as old as a dead process leaves");

            save().expect("a write that is not left waiting on the dead process's claim");

            assert!(!claim.exists(), "the claim outlived the write that took it");
        });
    }

    /// A claim taken a moment ago is waited for rather than taken.
    ///
    /// Breaking an abandoned claim is what keeps a crash from being permanent, and an age it
    /// applied to a live holder would put two writers back inside the file together, which is what
    /// the claim exists to prevent.
    #[test]
    fn a_claim_taken_a_moment_ago_is_not_treated_as_abandoned() {
        with_temp_home("live-claim", || {
            let path = path().expect("a path");
            prepare_directory(&path).expect("the directory");
            let claim = path.with_file_name(CLAIM);
            std::fs::File::create(&claim).expect("a claim");

            assert!(
                !abandoned(&claim, ABANDONED_AFTER),
                "a claim taken now is somebody's"
            );

            std::fs::OpenOptions::new()
                .write(true)
                .open(&claim)
                .expect("the claim")
                .set_modified(std::time::SystemTime::now() - ABANDONED_AFTER)
                .expect("an aged claim");

            assert!(
                abandoned(&claim, ABANDONED_AFTER),
                "a claim this old belongs to a process that died holding it"
            );
        });
    }

    /// A claim that is not abandoned stays where it is when a process that judged it abandoned
    /// earlier gets to removing it.
    ///
    /// Two processes can both see the same dead claim, and the slower one reaches the removal after
    /// the faster has broken it and taken a claim of its own. Removing what is at the path by then
    /// deletes that live claim, so the next process to arrive is let in beside its holder.
    #[test]
    fn breaking_a_claim_that_is_no_longer_abandoned_leaves_it_in_place() {
        with_temp_home("live-claim-break", || {
            let path = path().expect("a path");
            prepare_directory(&path).expect("the directory");
            let claim = path.with_file_name(CLAIM);
            create_claim(&claim).expect("a claim just taken by another process");

            break_abandoned(&claim, ABANDONED_AFTER);

            let third = create_claim(&claim);
            assert_eq!(
                third.err().map(|e| e.kind()),
                Some(std::io::ErrorKind::AlreadyExists),
                "a third process could take the claim while its holder was still working"
            );
        });
    }

    /// An abandoned claim is gone once it is broken, and nothing is left beside it.
    ///
    /// The claim is moved aside to be judged, and that file is the thing a crash in the middle of
    /// breaking would leave for ever, so the ordinary path has to remove it.
    #[test]
    fn breaking_an_abandoned_claim_removes_it_and_leaves_nothing_beside_it() {
        with_temp_home("dead-claim-break", || {
            let path = path().expect("a path");
            prepare_directory(&path).expect("the directory");
            let claim = path.with_file_name(CLAIM);
            std::fs::File::create(&claim)
                .expect("a claim")
                .set_modified(std::time::SystemTime::now() - ABANDONED_AFTER * 2)
                .expect("a claim as old as a dead process leaves");

            break_abandoned(&claim, ABANDONED_AFTER);

            let left: Vec<_> = std::fs::read_dir(path.parent().expect("a directory"))
                .expect("a listing")
                .map(|entry| entry.expect("an entry").file_name())
                .collect();
            assert!(left.is_empty(), "left behind: {left:?}");
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

            save().expect("a write");
            // Written twice: the second open finds the file already there, which is the case that
            // would silently keep a wider mode set by something else.
            save().expect("a second write");

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
    /// there from a backup or copied between machines. It holds a bearer token, so what importing
    /// leaves at the name must not be reachable by anything the file it replaced was.
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

            save().expect("a write");

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
    /// subsystem writes first is the one that creates it. A sign-in that made it at the
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

            save().expect("a write");

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
    /// narrowing through one has a sign-in setting the mode of a directory this
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

            save().expect("a write");

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

    /// A Windows file has no mode, so what keeps one to the account that signed in is
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

    /// A profile directory as the environment hands one over.
    fn named(value: &str) -> Option<std::ffi::OsString> {
        Some(std::ffi::OsString::from(value))
    }

    /// With no home there is nowhere a secret belongs, and inventing one would write it somewhere
    /// the user never chose. Reported rather than treated as nothing signed in.
    #[test]
    fn no_home_directory_is_reported_rather_than_guessed() {
        with_no_home(|| {
            assert!(matches!(path(), Err(StoreError::Unusable { .. })));
        });
    }

    /// STATE-2: a directory a person names in `BRAVEBOT_HOME` holds the credentials file itself,
    /// answering before `HOME`. Joined onto, a sign-in would write the bearer token into a hidden
    /// directory inside the one the person named for their own account's state, and a second
    /// account's session would not find the first one's.
    #[test]
    fn a_directory_named_in_the_environment_holds_the_credentials_itself() {
        #[cfg(windows)]
        const NAMED: &str = "C:\\srv\\ci\\state";
        #[cfg(not(windows))]
        const NAMED: &str = "/srv/ci/state";
        assert_eq!(
            directory_stated(named(NAMED), [named("/somebody")]).expect("a state directory"),
            PathBuf::from(NAMED)
        );
    }

    /// STATE-2: a directory a person names holds the credentials file byte for byte, and one whose
    /// name is not UTF-8 is not the directory its U+FFFD rendering spells. A resolver that went
    /// through text would put the bearer token in the second, or pass the first over and put it under
    /// `HOME`, and a second account's session would not find the first one's.
    #[cfg(unix)]
    #[test]
    fn a_directory_that_is_not_text_holds_the_credentials_and_not_its_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let invalid = OsStr::from_bytes(b"/srv/state-\xff");
        let lookalike = OsStr::new("/srv/state-\u{FFFD}");
        let answer = |stated: &OsStr| {
            directory_stated(Some(stated.to_os_string()), [named("/somebody")])
                .expect("a state directory")
        };

        assert_eq!(answer(invalid), PathBuf::from(invalid));
        assert_eq!(answer(lookalike), PathBuf::from(lookalike));
        assert_ne!(answer(invalid), answer(lookalike));
    }

    /// STATE-2: a value that is empty or not an absolute path names nothing and is passed over, so
    /// the profile directory answers as it would have, and where nothing else is named the sign-in is
    /// reported rather than put under wherever the process started.
    #[test]
    fn a_value_that_names_no_directory_is_passed_over() {
        for value in ["", "state", "./state", "../state", "~/state", "$HOME/state"] {
            assert_eq!(
                directory_stated(named(value), [named("/somebody")]).expect("a profile directory"),
                PathBuf::from("/somebody").join(DIRECTORY),
                "{value:?} was taken as a directory"
            );
            assert!(
                matches!(
                    directory_stated(named(value), [None]),
                    Err(StoreError::Unusable { .. })
                ),
                "{value:?} produced a location nothing stated"
            );
        }
    }

    /// Stock Windows sets no `HOME`, so a sign-in made there belongs under the profile directory the
    /// platform does name. Nowhere to keep it means a sign-in that cannot be kept.
    #[test]
    fn the_profile_directory_answers_where_no_home_is_named() {
        let expected = PathBuf::from("C:\\Users\\someone").join(DIRECTORY);
        // In the order `PROFILE_VARIABLES` names them: no `HOME`, then the profile directory stock
        // Windows names in `USERPROFILE`.
        assert_eq!(
            directory_named([None, named("C:\\Users\\someone")]).expect("a profile directory"),
            expected
        );
        assert_eq!(
            directory_named([named(""), named("C:\\Users\\someone")]).expect("a profile directory"),
            expected,
            "a variable exported empty took away a profile directory the platform names"
        );
    }

    /// A shell environment that sets `HOME` has been told where the profile is, and the file is read
    /// back by whatever is started from that shell next. A secret written somewhere else would leave
    /// a sign-in made in one session unusable in the next.
    #[test]
    fn a_named_home_answers_before_the_profile_directory() {
        assert_eq!(
            directory_named([named("/somebody"), named("C:\\Users\\someone")])
                .expect("a profile directory"),
            PathBuf::from("/somebody").join(DIRECTORY)
        );
    }
}
