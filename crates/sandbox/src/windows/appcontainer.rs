//! The Win32 calls behind the Windows backend, and the only unsafe in this crate outside
//! the Linux one.
//!
//! Every entry point here is a thin wrapper reporting the platform's error as an
//! [`std::io::Error`], so the backend above reads the same as the other two. What is
//! *decided* is in the parent module instead, where a test on any platform can call it:
//! which capability the token carries, what each grant permits, which policies are
//! refused, and how an argument is written onto a command line.

use super::{
    Grant, capability_names, command_line, grants_for, paths_that_are_not_there, profile_name,
    refusal_for,
};
use crate::policy::{Capabilities, SandboxPolicy};
use crate::process::{ConfinedChild, Environment, Stream, Streams};
use crate::{Sandbox, SandboxError};
use std::ffi::{OsStr, c_void};
use std::fs::{File, OpenOptions};
use std::io::{Error, Result};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS,
    HANDLE, HLOCAL, LocalFree, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    ACCESS_MODE, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE,
    REVOKE_ACCESS, SE_FILE_OBJECT, SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_GROUP,
    TRUSTEE_IS_SID, TRUSTEE_W,
};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, DeriveCapabilitySidsFromName, FreeSid, GetLengthSid, PSID,
    SECURITY_CAPABILITIES, SID_AND_ATTRIBUTES,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
};
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess, GetExitCodeProcess, INFINITE,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
    PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};

/// The numbers the parent module writes its grants in are the platform's own rights.
///
/// Spelled there so every platform's test run decides what a grant permits, and held here
/// so a number there cannot come to mean something other than the right it names. A
/// mismatch is a build failure on the target that ships rather than a grant that is wider
/// than it reads.
const _: () = {
    assert!(super::grant_for_reading().mask == FILE_GENERIC_READ | FILE_GENERIC_EXECUTE);
    assert!(
        super::grant_for_writing().mask
            == FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE
    );
};

/// `SE_GROUP_ENABLED`: the capability is in force in the token rather than merely listed.
///
/// Spelled rather than imported, because the module holding it is not one this build
/// compiles for anything else.
const CAPABILITY_ENABLED: u32 = 4;

/// What the platform records about the profile, which is the only place this name is read
/// by a person: Windows lists it under the account's app containers.
const PROFILE_DESCRIPTION: &str = "Confinement for a program bravebot runs on your behalf";

/// How many profiles this process has created, so each backend gets one of its own.
static PROFILES: AtomicU64 = AtomicU64::new(0);

/// `HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)`, which is what creating a profile that is
/// already there reports.
const PROFILE_ALREADY_EXISTS: i32 = (0x8007_0000u32 | ERROR_ALREADY_EXISTS) as i32;

/// AppContainer-based confinement.
///
/// Holds the profile it created and the grants it wrote, because both outlive the confined
/// process and both are this backend's to remove.
#[derive(Debug)]
pub struct AppContainerSandbox {
    /// The profile name, as the platform takes it.
    name: Vec<u16>,
    /// The container's security identifier, held as its own bytes so this backend is plain
    /// data rather than a pointer into somebody else's allocation.
    sid: Sid,
    /// Every grant written, so dropping this can take them off again.
    granted: Mutex<Vec<(PathBuf, Grant)>>,
}

impl AppContainerSandbox {
    /// Create this run's container profile, or refuse.
    pub fn new() -> std::result::Result<Self, SandboxError> {
        let name = profile_name(std::process::id(), PROFILES.fetch_add(1, Ordering::Relaxed));
        let name = wide(OsStr::new(&name)).map_err(|e| SandboxError::Unavailable {
            platform: "windows",
            detail: format!("the container profile could not be named: {e}"),
        })?;

        let sid = create_profile(&name).map_err(|e| SandboxError::Unavailable {
            platform: "windows",
            detail: format!(
                "an appcontainer profile could not be created ({e}); refusing to run \
                 untrusted code unconfined"
            ),
        })?;

        Ok(Self {
            name,
            sid,
            granted: Mutex::new(Vec::new()),
        })
    }

    /// Write one grant onto `path`, recording it so it can be taken off again.
    ///
    /// Recorded before the call rather than after, so a grant the platform wrote and then
    /// reported a failure for is still one this backend removes.
    fn write(&self, path: &Path, grant: Grant) -> Result<()> {
        self.granted
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push((path.to_path_buf(), grant));
        set_entry(path, &self.sid, GRANT_ACCESS, grant)
    }
}

impl Drop for AppContainerSandbox {
    /// Take off every grant and delete the profile.
    ///
    /// Best effort, and the reason a leftover entry is survivable: the profile is this
    /// run's alone, so an entry that outlives a run names a container nothing else is,
    /// and no later run inherits the access it describes.
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        let granted = std::mem::take(
            self.granted
                .get_mut()
                .unwrap_or_else(|held| held.into_inner()),
        );
        for (path, grant) in granted {
            let _ = set_entry(&path, &self.sid, REVOKE_ACCESS, grant);
        }
        let name = self.name.as_ptr();
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { DeleteAppContainerProfile(name) };
    }
}

impl Sandbox for AppContainerSandbox {
    fn capabilities(&self) -> Capabilities {
        super::capabilities()
    }

    fn spawn(
        &self,
        program: &str,
        args: &[String],
        policy: &SandboxPolicy,
        streams: Streams,
        environment: Environment,
    ) -> std::result::Result<ConfinedChild, SandboxError> {
        if let Some(refusal) = refusal_for(policy) {
            return Err(refusal);
        }

        // Before a profile is touched, so a policy this backend cannot apply costs no
        // entry on anybody's directory.
        let missing = paths_that_are_not_there(policy, |path| path.exists());
        if !missing.is_empty() {
            let named: Vec<_> = missing
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            return Err(SandboxError::SetupFailed {
                mechanism: "appcontainer",
                detail: format!(
                    "a grant is an entry written onto an object, and the policy names {} \
                     path(s) that are not on disk ({}); refusing rather than confining to \
                     less than it asked for",
                    missing.len(),
                    named.join(", ")
                ),
            });
        }

        for (path, grant) in grants_for(policy) {
            self.write(&path, grant)
                .map_err(|e| SandboxError::SetupFailed {
                    mechanism: "appcontainer",
                    detail: format!(
                        "the policy grants {} and the entry for it could not be written \
                         ({e}); refusing rather than confining to less than it asked for",
                        path.display()
                    ),
                })?;
        }

        let capabilities =
            capability_sids(&capability_names(policy)).map_err(|e| SandboxError::SetupFailed {
                mechanism: "appcontainer",
                detail: format!("a capability the policy grants could not be resolved: {e}"),
            })?;

        start(
            &self.sid,
            &capabilities,
            program,
            args,
            streams,
            environment,
        )
    }
}

/// A process started under a container.
///
/// The standard library's [`Child`](std::process::Child) cannot be built from a handle,
/// and a process started through `CreateProcessW` with an attribute list is one this crate
/// created itself, so this is what a caller waits on and kills instead.
#[derive(Debug)]
pub(crate) struct CreatedProcess {
    handle: OwnedHandle,
    id: u32,
}

impl CreatedProcess {
    pub(crate) fn id(&self) -> u32 {
        self.id
    }

    /// The handle as the platform takes it.
    fn raw(&self) -> HANDLE {
        self.handle.as_raw_handle() as HANDLE
    }

    #[allow(unsafe_code)]
    pub(crate) fn wait(&mut self) -> Result<std::process::ExitStatus> {
        use std::os::windows::process::ExitStatusExt;

        let process = self.raw();
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let waited = unsafe { WaitForSingleObject(process, INFINITE) };
        if waited != WAIT_OBJECT_0 {
            return Err(Error::last_os_error());
        }

        let mut code = 0u32;
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let read = unsafe { GetExitCodeProcess(process, &mut code) };
        if read == 0 {
            return Err(Error::last_os_error());
        }
        Ok(std::process::ExitStatus::from_raw(code))
    }

    #[allow(unsafe_code)]
    pub(crate) fn kill(&mut self) -> Result<()> {
        // The code a killed process reports. One rather than zero, so a caller reading the
        // status cannot mistake a process this ended for one that succeeded.
        const KILLED: u32 = 1;

        let process = self.raw();
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let ended = unsafe { TerminateProcess(process, KILLED) };
        if ended == 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }
}

/// A security identifier held as its own bytes.
///
/// Copied out of whatever allocated it, so nothing in this crate holds a pointer whose
/// lifetime belongs to the platform, and the backend stays plain data.
#[derive(Debug, Clone)]
struct Sid {
    /// Words rather than bytes, so the value is aligned as the platform expects to read it.
    words: Vec<u32>,
}

impl Sid {
    /// Copy the identifier at `psid`.
    ///
    /// # Safety
    ///
    /// `psid` points at a valid security identifier.
    #[allow(unsafe_code)]
    unsafe fn copied_from(psid: PSID) -> Self {
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let length = unsafe { GetLengthSid(psid) } as usize;
        let mut words = vec![0u32; length.div_ceil(size_of::<u32>()).max(1)];
        // Sound because the destination holds at least `length` bytes and the two do not
        // overlap: one is this vector and the other is the platform's own allocation.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe {
            std::ptr::copy_nonoverlapping(
                psid.cast::<u8>(),
                words.as_mut_ptr().cast::<u8>(),
                length,
            )
        };
        Self { words }
    }

    fn as_psid(&self) -> PSID {
        self.words.as_ptr() as PSID
    }
}

/// A pointer the platform allocated with `LocalAlloc`, freed when this is dropped.
struct LocalBuffer(*mut c_void);

impl Drop for LocalBuffer {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if !self.0.is_null() {
            let allocation = self.0 as HLOCAL;
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            unsafe { LocalFree(allocation) };
        }
    }
}

/// Create the profile named `name`, or find the one already there, and return its
/// identifier.
///
/// A profile that already exists is not an error: a process identifier is reused once the
/// process holding it is gone, so a run can be handed a name a previous one left behind
/// after failing to delete it. Deriving the identifier from the name gives the same answer
/// creating it would have.
#[allow(unsafe_code)]
fn create_profile(name: &[u16]) -> Result<Sid> {
    let description = wide(OsStr::new(PROFILE_DESCRIPTION))?;
    let mut psid: PSID = null_mut();

    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let created = unsafe {
        CreateAppContainerProfile(
            name.as_ptr(),
            name.as_ptr(),
            description.as_ptr(),
            null(),
            0,
            &mut psid,
        )
    };

    if created < 0 {
        if created != PROFILE_ALREADY_EXISTS {
            return Err(reported("CreateAppContainerProfile", created));
        }
        let name = name.as_ptr();
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let derived = unsafe { DeriveAppContainerSidFromAppContainerName(name, &mut psid) };
        if derived < 0 {
            return Err(reported(
                "DeriveAppContainerSidFromAppContainerName",
                derived,
            ));
        }
    }

    // Sound because the call above reported success, which means it wrote an identifier
    // this now owns.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let sid = unsafe { Sid::copied_from(psid) };
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe { FreeSid(psid) };
    Ok(sid)
}

/// The identifiers of the capabilities `names` describes.
///
/// Each is copied out before the platform's arrays are freed, so what the token is built
/// from outlives the call that resolved it.
#[allow(unsafe_code)]
fn capability_sids(names: &[&str]) -> Result<Vec<Sid>> {
    let mut sids = Vec::with_capacity(names.len());
    for name in names {
        let name = wide(OsStr::new(name))?;
        let mut groups: *mut PSID = null_mut();
        let mut group_count = 0u32;
        let mut resolved: *mut PSID = null_mut();
        let mut resolved_count = 0u32;

        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let derived = unsafe {
            DeriveCapabilitySidsFromName(
                name.as_ptr(),
                &mut groups,
                &mut group_count,
                &mut resolved,
                &mut resolved_count,
            )
        };
        if derived == 0 {
            return Err(Error::last_os_error());
        }

        // Sound because the call reported success, so both arrays hold as many identifiers
        // as it counted, and each of them and the arrays themselves are this caller's to
        // free.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe {
            for index in 0..resolved_count as usize {
                let psid = *resolved.add(index);
                sids.push(Sid::copied_from(psid));
                LocalFree(psid as HLOCAL);
            }
            for index in 0..group_count as usize {
                LocalFree(*groups.add(index) as HLOCAL);
            }
            LocalFree(resolved as HLOCAL);
            LocalFree(groups as HLOCAL);
        }
    }
    Ok(sids)
}

/// Add `grant` for `sid` to what `path` already allows, or take it off again.
///
/// The list is read, the entry merged into it and the result written back, rather than a
/// list built from scratch: a directory carries entries that are not this backend's, and
/// replacing them would take somebody's own access to their own files away.
#[allow(unsafe_code)]
fn set_entry(path: &Path, sid: &Sid, mode: ACCESS_MODE, grant: Grant) -> Result<()> {
    let path = wide(path.as_os_str())?;

    let mut existing: *mut ACL = null_mut();
    let mut descriptor = null_mut();
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let read = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut existing,
            null_mut(),
            &mut descriptor,
        )
    };
    if read != ERROR_SUCCESS {
        return Err(Error::from_raw_os_error(read as i32));
    }
    let _descriptor = LocalBuffer(descriptor);

    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: grant.mask,
        grfAccessMode: mode,
        grfInheritance: grant.inheritance,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            // A container identifier names no account, which is what this form is for.
            TrusteeType: TRUSTEE_IS_GROUP,
            ptstrName: sid.as_psid().cast::<u16>(),
        },
    };

    let mut merged: *mut ACL = null_mut();
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let built = unsafe { SetEntriesInAclW(1, &entry, existing, &mut merged) };
    if built != ERROR_SUCCESS {
        return Err(Error::from_raw_os_error(built as i32));
    }
    let _merged = LocalBuffer(merged.cast::<c_void>());

    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let written = unsafe {
        SetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            merged,
            null(),
        )
    };
    if written != ERROR_SUCCESS {
        return Err(Error::from_raw_os_error(written as i32));
    }
    Ok(())
}

/// A list of process creation attributes, freed when it is dropped.
struct AttributeList {
    /// Words rather than bytes, so the platform's own structures are aligned inside it.
    buffer: Vec<u64>,
}

impl AttributeList {
    #[allow(unsafe_code)]
    fn holding(count: u32) -> Result<Self> {
        let mut size = 0usize;
        // The call that asks how large the list has to be fails saying so, which is the
        // only way to find out.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { InitializeProcThreadAttributeList(null_mut(), count, 0, &mut size) };
        if size == 0 {
            return Err(Error::last_os_error());
        }

        let mut list = Self {
            buffer: vec![0u64; size.div_ceil(size_of::<u64>())],
        };
        let buffer = list.as_ptr();
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let ready = unsafe { InitializeProcThreadAttributeList(buffer, count, 0, &mut size) };
        if ready == 0 {
            // Nothing was initialised, so nothing is there to delete. Forgetting the list
            // rather than dropping it is what keeps the deletion off an uninitialised one.
            std::mem::forget(list);
            return Err(Error::last_os_error());
        }
        Ok(list)
    }

    fn as_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.buffer.as_mut_ptr().cast::<c_void>()
    }

    /// Point `attribute` at `value`, which must outlive the process creation this list is
    /// used for: the list holds the pointer rather than a copy of what it points at.
    #[allow(unsafe_code)]
    fn set(&mut self, attribute: usize, value: *const c_void, size: usize) -> Result<()> {
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let updated = unsafe {
            UpdateProcThreadAttribute(self.as_ptr(), 0, attribute, value, size, null_mut(), null())
        };
        if updated == 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for AttributeList {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        let buffer = self.as_ptr();
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { DeleteProcThreadAttributeList(buffer) };
    }
}

/// The two ends of what one of the confined process's streams is attached to: the handle
/// it is given, and the pipe this process keeps where there is one.
struct Attachment {
    child: OwnedHandle,
    ours: Option<File>,
}

/// What the confined process reads its input from.
fn input(stream: Stream) -> Result<Attachment> {
    match stream {
        Stream::Piped => {
            let (reader, writer) = std::io::pipe()?;
            let reader = OwnedHandle::from(reader);
            Ok(Attachment {
                child: inheritable(reader.as_raw_handle())?,
                ours: Some(File::from(OwnedHandle::from(writer))),
            })
        }
        Stream::Inherited => Ok(Attachment {
            child: inheritable(std::io::stdin().as_raw_handle())?,
            ours: None,
        }),
        Stream::Null => {
            let nowhere = discarded()?;
            Ok(Attachment {
                child: inheritable(nowhere.as_raw_handle())?,
                ours: None,
            })
        }
    }
}

/// What the confined process writes one of its output streams to.
///
/// `inherited` is this process's own end of that stream, so each of the two is attached to
/// what this process is attached to rather than both to one of them.
fn output(stream: Stream, inherited: RawHandle) -> Result<Attachment> {
    match stream {
        Stream::Piped => {
            let (reader, writer) = std::io::pipe()?;
            let writer = OwnedHandle::from(writer);
            Ok(Attachment {
                child: inheritable(writer.as_raw_handle())?,
                ours: Some(File::from(OwnedHandle::from(reader))),
            })
        }
        Stream::Inherited => Ok(Attachment {
            child: inheritable(inherited)?,
            ours: None,
        }),
        Stream::Null => {
            let nowhere = discarded()?;
            Ok(Attachment {
                child: inheritable(nowhere.as_raw_handle())?,
                ours: None,
            })
        }
    }
}

/// Somewhere a write goes nowhere and a read is at its end.
fn discarded() -> Result<File> {
    OpenOptions::new().read(true).write(true).open("NUL")
}

/// A copy of `handle` the confined process inherits.
///
/// A copy rather than the handle itself, because inheritance is a property of the handle
/// and marking this process's own would hand it to every process started from here rather
/// than to this one.
#[allow(unsafe_code)]
fn inheritable(handle: RawHandle) -> Result<OwnedHandle> {
    let mut copy: HANDLE = null_mut();
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let duplicated = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle as HANDLE,
            GetCurrentProcess(),
            &mut copy,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if duplicated == 0 {
        return Err(Error::last_os_error());
    }
    // Sound because the call reported success, which means it wrote a handle nothing else
    // owns.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    Ok(unsafe { OwnedHandle::from_raw_handle(copy as RawHandle) })
}

/// Start the process, or refuse.
fn start(
    sid: &Sid,
    capabilities: &[Sid],
    program: &str,
    args: &[String],
    streams: Streams,
    environment: Environment,
) -> std::result::Result<ConfinedChild, SandboxError> {
    started(sid, capabilities, program, args, streams, environment)
        .map_err(SandboxError::SpawnFailed)
}

#[allow(unsafe_code)]
fn started(
    sid: &Sid,
    capabilities: &[Sid],
    program: &str,
    args: &[String],
    streams: Streams,
    environment: Environment,
) -> Result<ConfinedChild> {
    let mut line = wide(OsStr::new(&command_line(program, args)))?;

    let stdin = input(streams.stdin)?;
    let stdout = output(streams.stdout, std::io::stdout().as_raw_handle())?;
    let stderr = output(streams.stderr, std::io::stderr().as_raw_handle())?;

    let mut granted: Vec<SID_AND_ATTRIBUTES> = capabilities
        .iter()
        .map(|capability| SID_AND_ATTRIBUTES {
            Sid: capability.as_psid(),
            Attributes: CAPABILITY_ENABLED,
        })
        .collect();
    let security = SECURITY_CAPABILITIES {
        AppContainerSid: sid.as_psid(),
        Capabilities: granted.as_mut_ptr(),
        CapabilityCount: granted.len() as u32,
        Reserved: 0,
    };

    // The three the confined process is given, and the whole of what it inherits: every
    // other inheritable handle this process holds stays here, which is what the list is
    // for. Without it the flag that lets the three through lets all of them through.
    let inherited: [HANDLE; 3] = [
        stdin.child.as_raw_handle() as HANDLE,
        stdout.child.as_raw_handle() as HANDLE,
        stderr.child.as_raw_handle() as HANDLE,
    ];

    let mut attributes = AttributeList::holding(2)?;
    attributes.set(
        PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
        std::ptr::from_ref(&security).cast::<c_void>(),
        size_of::<SECURITY_CAPABILITIES>(),
    )?;
    attributes.set(
        PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
        inherited.as_ptr().cast::<c_void>(),
        size_of_val(&inherited),
    )?;

    let mut startup = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW {
            cb: size_of::<STARTUPINFOEXW>() as u32,
            dwFlags: STARTF_USESTDHANDLES,
            hStdInput: inherited[0],
            hStdOutput: inherited[1],
            hStdError: inherited[2],
            ..Default::default()
        },
        lpAttributeList: attributes.as_ptr(),
    };

    // Two terminators: one ends the last variable and one ends the block, so this is a
    // block holding nothing rather than a block that was never terminated.
    let empty_environment: [u16; 2] = [0, 0];
    let (block, unicode) = match environment {
        Environment::Empty => (
            empty_environment.as_ptr().cast::<c_void>(),
            CREATE_UNICODE_ENVIRONMENT,
        ),
        Environment::Inherited => (null(), 0),
    };

    let mut created = PROCESS_INFORMATION::default();
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let started = unsafe {
        CreateProcessW(
            null(),
            line.as_mut_ptr(),
            null(),
            null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | unicode,
            block,
            null(),
            std::ptr::from_mut(&mut startup).cast::<STARTUPINFOW>(),
            &mut created,
        )
    };
    if started == 0 {
        return Err(Error::last_os_error());
    }

    // Nothing here resumes or waits on the initial thread, and a handle to it left open
    // keeps the thread object alive for as long as this process runs.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe { CloseHandle(created.hThread) };

    // Sound because the call reported success, which means it wrote a process handle
    // nothing else owns.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let handle = unsafe { OwnedHandle::from_raw_handle(created.hProcess as RawHandle) };

    Ok(crate::process::confined(
        CreatedProcess {
            handle,
            id: created.dwProcessId,
        },
        stdin.ours,
        stdout.ours,
        stderr.ours,
    ))
}

/// `text` as the platform takes a string, terminated.
///
/// A string with a terminator inside it is refused rather than truncated at it: the
/// platform reads to the first one, so a path carrying one would name a different path
/// from the one the policy granted.
fn wide(text: &OsStr) -> Result<Vec<u16>> {
    let mut encoded: Vec<u16> = text.encode_wide().collect();
    if encoded.contains(&0) {
        return Err(Error::new(
            std::io::ErrorKind::InvalidInput,
            "a string handed to the platform has a terminator inside it",
        ));
    }
    encoded.push(0);
    Ok(encoded)
}

/// What a call reporting an `HRESULT` failed with.
///
/// Written as the number rather than translated, because these come from `userenv` rather
/// than from `GetLastError` and the platform's message table is keyed on the other kind.
fn reported(call: &str, result: i32) -> Error {
    Error::other(format!("{call} failed (0x{:08x})", result as u32))
}
