//! The walk's one open on Windows: `NtCreateFile` relative to the handle of the directory above.
//!
//! Win32 has no relative open, and every Win32 call taking a path takes a whole one, so the walk
//! is the native call `openat` corresponds to. `OBJ_DONT_REPARSE` refuses a reparse point met
//! while the name is parsed, and `FILE_OPEN_REPARSE_POINT` opens one at the name as itself rather
//! than following it, so no link is resolved. What was opened is then asked whether it is a link,
//! which `std` answers for a symlink and a junction alike, as `O_NOFOLLOW` does on POSIX. A reparse
//! point that is not a link, such as a cloud placeholder, is opened as itself; reading one that
//! has not been fetched fails, which is a refusal. Renaming and removing are done on the open
//! handle rather than by name, so neither reads a path either.
//!
//! These are the crate's only `unsafe` blocks. Each is a call with the arguments it documents and
//! nothing decided inside it.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Component, Path, Prefix};
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_DISPOSITION_INFORMATION, FILE_NON_DIRECTORY_FILE,
    FILE_OPEN, FILE_OPEN_IF, FILE_OPEN_REPARSE_POINT, FILE_RENAME_INFORMATION,
    FILE_SYNCHRONOUS_IO_NONALERT, FileDispositionInformation, FileRenameInformation,
    NTCREATEFILE_CREATE_DISPOSITION, NTCREATEFILE_CREATE_OPTIONS, NtCreateFile,
    NtSetInformationFile,
};
use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, LocalFree, NTSTATUS, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE,
    RtlNtStatusToDosError, UNICODE_STRING,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ACCESS_RIGHTS, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub(crate) use crate::names::misleads;

/// The drive's root directory, and the rest of `root` to descend by name.
///
/// Only `C:\` and its like. A share, a device path and the `\\?\` form are refused rather than
/// walked: none of them has been shown to honour the flags the walk rests on, so a project on a
/// share is refused until one has. `C:x` is relative to a directory nobody named, as on POSIX.
pub fn volume(root: &Path) -> io::Result<(File, &Path)> {
    let mut parts = root.components();
    let Some(Component::Prefix(prefix)) = parts.next() else {
        return Err(crate::invalid("Project root must be absolute"));
    };
    let Prefix::Disk(letter) = prefix.kind() else {
        return Err(crate::invalid("Project root must be on a drive letter"));
    };
    if parts.next() != Some(Component::RootDir) {
        return Err(crate::invalid("Project root must be absolute"));
    }
    // By name, since a drive's root has nothing above it to be a link.
    let volume = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(format!("{}:\\", char::from(letter)))?;
    Ok((volume, parts.as_path()))
}

pub fn directory_at(parent: &File, part: &str, create: bool) -> io::Result<File> {
    let access = FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES;
    // Open-or-create in one call, so there is no gap between making the directory and opening it.
    let directory = if create {
        let private = private(true)?;
        open_relative(
            parent,
            part,
            access,
            FILE_OPEN_IF,
            FILE_DIRECTORY_FILE,
            Some(&private),
        )?
    } else {
        open_relative(parent, part, access, FILE_OPEN, FILE_DIRECTORY_FILE, None)?
    };
    // A junction opened as itself is a directory too, so the open alone does not refuse one.
    if !directory.metadata()?.is_dir() {
        return Err(crate::invalid("Not a directory"));
    }
    Ok(directory)
}

pub fn open_at(parent: &File, part: &str) -> io::Result<File> {
    open_relative(parent, part, FILE_GENERIC_READ, FILE_OPEN, 0, None)
}

/// A new private file, never one already there: `FILE_CREATE` refuses an existing file or link.
pub fn create_new_at(parent: &File, part: &str) -> io::Result<File> {
    let private = private(false)?;
    open_relative(
        parent,
        part,
        FILE_GENERIC_WRITE | DELETE,
        FILE_CREATE,
        FILE_NON_DIRECTORY_FILE,
        Some(&private),
    )
}

/// Rename `file` to `to` in `parent`, replacing what is there, by the handle rather than by name.
#[allow(unsafe_code)]
pub fn rename_at(parent: &File, file: &File, _from: &str, to: &str) -> io::Result<()> {
    let name: Vec<u16> = to.encode_utf16().collect();
    let bytes = size_of_val(name.as_slice());
    let offset = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName);
    let length = (offset + bytes).max(size_of::<FILE_RENAME_INFORMATION>());
    let (Ok(name_length), Ok(length_u32)) = (u32::try_from(bytes), u32::try_from(length)) else {
        return Err(crate::invalid("File name is too long"));
    };
    // Words rather than bytes, so the header written into it is aligned as it expects.
    let mut buffer = vec![0u64; length.div_ceil(size_of::<u64>())];
    let information: *mut FILE_RENAME_INFORMATION = buffer.as_mut_ptr().cast();
    // Sound because the buffer is zeroed, aligned for the header, and at least `offset + bytes`
    // long, so the header and the name after it are both inside it; `offset` is even, which is the
    // alignment the name needs.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        (*information).Anonymous.ReplaceIfExists = true;
        (*information).RootDirectory = parent.as_raw_handle();
        (*information).FileNameLength = name_length;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            buffer.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
            name.len(),
        );
    }
    let mut status = IO_STATUS_BLOCK::default();
    // Sound because the handle is open for `DELETE`, which a rename needs, and the buffer is the
    // length passed with it.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    check(unsafe {
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status,
            buffer.as_ptr().cast(),
            length_u32,
            FileRenameInformation,
        )
    })
}

/// Delete `file` once its last handle closes, which is the one the caller holds.
#[allow(unsafe_code)]
pub fn remove_at(_parent: &File, file: &File, _name: &str) {
    let information = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
    let mut status = IO_STATUS_BLOCK::default();
    // Sound because the structure passed is the one the class names, at its own size.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status,
            (&raw const information).cast(),
            size_of::<FILE_DISPOSITION_INFORMATION>() as u32,
            FileDispositionInformation,
        );
    }
}

/// Open `name` in `parent` without resolving a link, as `openat` with `O_NOFOLLOW` does.
///
/// The name is one component: the callers above hand it nothing with a separator in it. A name is
/// matched without regard to case, as every other Windows program opens it.
#[allow(unsafe_code)]
fn open_relative(
    parent: &File,
    name: &str,
    access: FILE_ACCESS_RIGHTS,
    disposition: NTCREATEFILE_CREATE_DISPOSITION,
    options: NTCREATEFILE_CREATE_OPTIONS,
    descriptor: Option<&SecurityDescriptor>,
) -> io::Result<File> {
    let name: Vec<u16> = name.encode_utf16().collect();
    let Ok(length) = u16::try_from(size_of_val(name.as_slice())) else {
        return Err(crate::invalid("File name is too long"));
    };
    let object_name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: name.as_ptr().cast_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &object_name,
        Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        SecurityDescriptor: descriptor.map_or(std::ptr::null(), |descriptor| {
            descriptor.0.cast_const().cast()
        }),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle = std::ptr::null_mut();
    let mut status = IO_STATUS_BLOCK::default();
    // Sound because every pointer is to a live local of the type the call expects, the name's
    // length is its own in bytes, and the handle is written only by a call that succeeds.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    check(unsafe {
        NtCreateFile(
            &mut handle,
            access | SYNCHRONIZE,
            &attributes,
            &mut status,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            disposition,
            options | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    })?;
    // Sound because the call above succeeded, so it wrote an open handle nothing else owns.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    Ok(File::from(unsafe { OwnedHandle::from_raw_handle(handle) }))
}

/// An `NTSTATUS` as the `io::Error` a Win32 call would have reported, so a name that is not there
/// is `NotFound` here as it is on POSIX.
#[allow(unsafe_code)]
fn check(status: NTSTATUS) -> io::Result<()> {
    if status >= 0 {
        return Ok(());
    }
    // Sound because the call takes a value and touches no memory of ours.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let code = unsafe { RtlNtStatusToDosError(status) };
    Err(io::Error::from_raw_os_error(code as i32))
}

/// A list granting this account everything and nobody else anything, protected so the directory
/// it is made in adds nothing: what `0600` and `0700` are on POSIX. A directory's entry is also
/// inherited by what is made inside it.
fn private(directory: bool) -> io::Result<SecurityDescriptor> {
    let inherit = if directory { "OICI" } else { "" };
    SecurityDescriptor::of(&format!("D:P(A;{inherit};FA;;;{})", current_account()?))
}

/// The account this process runs as, written the way an entry in a list names one.
///
/// Taken from the process token rather than from a user name, because a name is not what the list
/// holds and the same name can belong to two accounts, one on the machine and one in a domain.
#[allow(unsafe_code)]
pub fn current_account() -> io::Result<String> {
    // Sound because a call that returns nonzero has written an open handle nothing else owns,
    // which is what [`OwnedHandle`] takes responsibility for closing.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let token = unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
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
        let no_room = io::Error::last_os_error();
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
        return Err(io::Error::last_os_error());
    }

    // Sound because the call above returned success, which means it wrote a whole `TOKEN_USER` and
    // the account it points at into this buffer, and a `Vec<u64>` is aligned for both. The string
    // the SID is written to is allocated by the call that writes it, null-terminated, and freed
    // here.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        let user: *const TOKEN_USER = buffer.as_ptr().cast();
        let mut written = std::ptr::null_mut();
        if ConvertSidToStringSidW((*user).User.Sid, &mut written) == 0 {
            return Err(io::Error::last_os_error());
        }
        let account = wide_to_string(written);
        LocalFree(written.cast());
        Ok(account)
    }
}

/// A list the platform parsed out of its written form, freed when it goes out of scope.
struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

impl SecurityDescriptor {
    /// Parse SDDL into the form a file takes.
    #[allow(unsafe_code)]
    fn of(sddl: &str) -> io::Result<Self> {
        let sddl: Vec<u16> = OsStr::new(sddl)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
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
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
}

impl Drop for SecurityDescriptor {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // Sound because the descriptor was allocated by the call that parsed it, which documents
        // `LocalFree` as its release, and is freed nowhere else.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { LocalFree(self.0) };
    }
}

/// A null-terminated UTF-16 string the platform wrote, as a Rust one.
///
/// # Safety
///
/// `text` points at a readable, null-terminated UTF-16 string.
#[allow(unsafe_code)]
unsafe fn wide_to_string(text: *const u16) -> String {
    let mut end = text;
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        while *end != 0 {
            end = end.add(1);
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(
            text,
            end.offset_from(text) as usize,
        ))
    }
}

/// `sddl` as the platform writes it back, which names some accounts by alias (`LA` for the
/// built-in Administrator) rather than by SID.
#[cfg(test)]
pub fn written(sddl: &str) -> io::Result<String> {
    list_text(&SecurityDescriptor::of(sddl)?)
}

/// The access-control list on the file or directory at `path`, in its written form.
#[cfg(test)]
#[allow(unsafe_code)]
pub fn list_at(path: &Path) -> io::Result<String> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut descriptor = std::ptr::null_mut();
    // Sound because every out-pointer is either null or a live local, and the descriptor written
    // is freed by the guard it goes into.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let read = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if read != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(read as i32));
    }
    list_text(&SecurityDescriptor(descriptor))
}

#[cfg(test)]
#[allow(unsafe_code)]
fn list_text(descriptor: &SecurityDescriptor) -> io::Result<String> {
    use windows_sys::Win32::Security::Authorization::ConvertSecurityDescriptorToStringSecurityDescriptorW;
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

    let mut text = std::ptr::null_mut();
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let converted = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor.0,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(io::Error::last_os_error());
    }
    // Sound because a successful conversion wrote a null-terminated string it allocated, which is
    // freed here and nowhere else.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        let list = wide_to_string(text);
        LocalFree(text.cast());
        Ok(list)
    }
}
