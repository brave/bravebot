//! Keeping a credential's pages out of swap.
//!
//! [CRED-23](../../../docs/specs/credential-protection.md#CRED-23) promises that the pages
//! `bravebot-config`'s `Secret` holds a credential in are kept off swap by a Unix kernel that grants
//! the lock. A lock is on a page rather than on a value, and a page of the ordinary heap is shared
//! with whatever else the allocator put there: locking a value in place locks its neighbours with
//! it, and unlocking it when it goes unlocks a second credential that happens to share its page. So
//! each value here is held in a mapping of its own, which nothing else is ever placed in, and the
//! lock and the mapping go together.
//!
//! A page or more per value is the price, paid against one memory lock limit for the whole process.
//! What is held here is the handful of credentials a configuration resolves and, while an import
//! rewrites a settings file, that file twice over: as read, which is at most sixteen pages of four
//! kilobytes, and as rewritten, whose size is checked only once it is held. A value held once the
//! limit is spent is held unlocked.
//!
//! It lives in this crate for the reason [`crate::crash`] does: it is a platform call, and the
//! crate that owns the type a credential is held in forbids `unsafe`.

/// Text held in pages of its own, locked into memory where the kernel agrees to it, and
/// overwritten where it lies before those pages are handed back.
///
/// A kernel that refuses the lock, because the process is over its memory lock limit or the
/// platform offers none, still gets a value that reads back and is cleared when it goes. Refusing
/// to hold a credential over it would protect the credential by making the product unusable, which
/// is the trade [`crate::crash::disable_core_dumps`] declines too.
#[cfg(unix)]
pub struct LockedText {
    start: std::ptr::NonNull<u8>,
    len: usize,
    /// A whole number of pages, and zero for empty text, which is given no mapping at all.
    mapped: usize,
    locked: bool,
}

// The mapping is this value's alone and nothing writes to it between construction and drop, so it
// may be read from any thread and moved to any other.
#[cfg(unix)]
#[allow(unsafe_code)]
// nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
unsafe impl Send for LockedText {}
#[cfg(unix)]
#[allow(unsafe_code)]
// nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
unsafe impl Sync for LockedText {}

// Each `unsafe` below sits in the method whose fields it relies on, rather than in a helper that
// would accept any pointer: what makes it sound is that `start` is the mapping `new` made, `len`
// bytes of text inside `mapped` bytes that are this value's alone, and that only this value unmaps
// it.
#[cfg(unix)]
impl LockedText {
    #[allow(unsafe_code)]
    pub fn new(text: &str) -> Self {
        if text.is_empty() {
            return Self {
                start: std::ptr::NonNull::dangling(),
                len: 0,
                mapped: 0,
                locked: false,
            };
        }
        let mapped = text.len().div_ceil(page_size()) * page_size();
        let start = map(mapped);
        // Locked before the text is written, so there is no moment at which the pages hold it and
        // may still be written out.
        let locked = lock(start, mapped);
        // `mapped` is at least `text.len()`, and the pages are fresh, so nothing else refers to
        // them.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), start.as_ptr(), text.len()) };
        Self {
            start,
            len: text.len(),
            mapped,
            locked,
        }
    }

    #[allow(unsafe_code)]
    pub fn as_str(&self) -> &str {
        // The `len` bytes at `start` were copied from a `str`, and clearing overwrites them with
        // zero bytes, each of which is still UTF-8. The borrow ends before the drop that unmaps
        // them.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.start.as_ptr(), self.len))
        }
    }

    /// Whether the kernel agreed to keep these pages resident.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Overwrite the text where it lies, leaving it as many zero bytes long as it was. Volatile,
    /// because nothing reads these zeros back and a store nobody observes is one a compiler is
    /// entitled to delete.
    #[allow(unsafe_code)]
    fn clear(&mut self) {
        for offset in 0..self.len {
            // Inside the `len` bytes `new` wrote.
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            unsafe { self.start.as_ptr().add(offset).write_volatile(0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(unix)]
impl Clone for LockedText {
    fn clone(&self) -> Self {
        Self::new(self.as_str())
    }
}

#[cfg(unix)]
impl Drop for LockedText {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        self.clear();
        if self.mapped != 0 {
            // Unmapping releases the lock with the pages, so there is no separate unlock to forget.
            // The mapping is the one `new` made, and a value is dropped once.
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            unsafe { libc::munmap(self.start.as_ptr().cast(), self.mapped) };
        }
    }
}

#[cfg(unix)]
// The exemption sits on the function because the function is the syscall.
#[allow(unsafe_code)]
fn page_size() -> usize {
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    usize::try_from(size).expect("every Unix this builds for states its page size")
}

/// Fresh anonymous pages, readable and writable by this process alone.
#[cfg(unix)]
// The exemption sits on the function because the function is the syscall.
#[allow(unsafe_code)]
fn map(len: usize) -> std::ptr::NonNull<u8> {
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let start = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    match std::ptr::NonNull::new(start.cast::<u8>()) {
        Some(start) if start.as_ptr().cast() != libc::MAP_FAILED => start,
        // What any allocation does when the memory is not there, so a credential that cannot be
        // held stops the program the way every other value would.
        _ => std::alloc::handle_alloc_error(
            std::alloc::Layout::array::<u8>(len).unwrap_or(std::alloc::Layout::new::<u8>()),
        ),
    }
}

/// Whether the kernel locked the pages. A lock on pages this process maps is a request the kernel
/// grants or refuses, and asks nothing of the memory, so any range may be passed.
#[cfg(unix)]
// The exemption sits on the function because the function is the syscall.
#[allow(unsafe_code)]
fn lock(start: std::ptr::NonNull<u8>, len: usize) -> bool {
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let locked = unsafe { libc::mlock(start.as_ptr().cast(), len) };
    locked == 0
}

/// See the Unix implementation above. No lock is taken here: the platform call that would take one
/// is not compiled into this build, so the text is held on the heap and cleared when it goes.
#[cfg(not(unix))]
#[derive(Clone)]
pub struct LockedText(String);

#[cfg(not(unix))]
impl LockedText {
    pub fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_locked(&self) -> bool {
        false
    }

    fn clear(&mut self) {
        let len = self.0.len();
        self.0.clear();
        self.0.extend(std::iter::repeat_n('\0', len));
        std::hint::black_box(self.0.as_bytes());
    }
}

#[cfg(not(unix))]
impl Drop for LockedText {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;

    /// The memory lock limit belongs to the process, and one test below lowers it, so every test
    /// that reads a lock back takes it in turn. Run alongside the lowering, a lock that should
    /// have been granted is refused and the test reports a fault the code does not have.
    static THE_LIMIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Long enough to need more than one page, and with a character of more than one byte on the
    /// boundary side of it, so that a length counted in characters holds less than the text.
    fn a_credential_longer_than_a_page() -> String {
        let mut text = "é".repeat(page_size());
        text.push_str("-end");
        text
    }

    /// Whether the kernel holds the page `address` is on resident, read from the kernel rather than
    /// from the value: a value reporting its own success says nothing about whether the kernel
    /// agreed.
    fn the_kernel_keeps_resident(address: *const u8) -> bool {
        residency(address).unwrap_or_else(|| panic!("no mapping holds {address:p}"))
    }

    /// Whether the page `address` is on is locked, or `None` where nothing is mapped there.
    #[cfg(target_os = "linux")]
    fn residency(address: *const u8) -> Option<bool> {
        let address = address.addr();
        let maps = std::fs::read_to_string("/proc/self/smaps").expect("smaps is readable");
        let mut inside = false;
        for line in maps.lines() {
            let first = line.split_whitespace().next().unwrap_or_default();
            if let Some((start, end)) = first.split_once('-')
                && let (Ok(start), Ok(end)) = (
                    usize::from_str_radix(start, 16),
                    usize::from_str_radix(end, 16),
                )
            {
                inside = (start..end).contains(&address);
                continue;
            }
            if inside && let Some(flags) = line.strip_prefix("VmFlags:") {
                return Some(flags.split_whitespace().any(|flag| flag == "lo"));
            }
        }
        None
    }

    /// See the Linux version above. macOS states a region's wired count through `proc_pidinfo`,
    /// whose record `libc` does not declare, so the layout is spelled here from
    /// `<sys/proc_info.h>`. Asked about an address nothing maps, it describes the next region up,
    /// or none.
    #[cfg(target_os = "macos")]
    #[allow(unsafe_code)]
    fn residency(address: *const u8) -> Option<bool> {
        #[repr(C)]
        #[derive(Default)]
        struct RegionInfo {
            protection: u32,
            max_protection: u32,
            inheritance: u32,
            flags: u32,
            offset: u64,
            behavior: u32,
            user_wired_count: u32,
            user_tag: u32,
            pages_resident: u32,
            pages_shared_now_private: u32,
            pages_swapped_out: u32,
            pages_dirtied: u32,
            ref_count: u32,
            shadow_depth: u32,
            share_mode: u32,
            private_pages_resident: u32,
            shared_pages_resident: u32,
            obj_id: u32,
            depth: u32,
            address: u64,
            size: u64,
        }
        const PROC_PIDREGIONINFO: libc::c_int = 7;
        let size = std::mem::size_of::<RegionInfo>();
        let mut info = RegionInfo::default();
        let address = u64::try_from(address.addr()).expect("an address fits in 64 bits");
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let written = unsafe {
            libc::proc_pidinfo(
                libc::getpid(),
                PROC_PIDREGIONINFO,
                address,
                (&raw mut info).cast(),
                libc::c_int::try_from(size).expect("the record is small"),
            )
        };
        if usize::try_from(written).ok() != Some(size)
            || !(info.address..info.address + info.size).contains(&address)
        {
            return None;
        }
        Some(info.user_wired_count > 0)
    }

    #[allow(unsafe_code)]
    fn memory_lock_limits() -> libc::rlimit {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let read = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &raw mut limit) };
        assert_eq!(read, 0, "the memory lock limit is readable");
        limit
    }

    #[allow(unsafe_code)]
    fn set_memory_lock_limits(limit: &libc::rlimit) {
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let written = unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, limit) };
        assert_eq!(written, 0, "the memory lock limit could not be set");
    }

    /// Puts the memory lock limit back when it goes, so a test that fails while the limit is
    /// lowered does not leave every test after it refused a lock it should have been granted.
    struct LimitRestored(libc::rlimit);

    impl Drop for LimitRestored {
        fn drop(&mut self) {
            set_memory_lock_limits(&self.0);
        }
    }

    /// The first and last byte of a value spanning pages are each on a page the kernel will not
    /// write out. A lock on the value's first page alone, or on the pages the text was copied from
    /// rather than the ones it was copied to, leaves the credential swappable while the value
    /// reports it held.
    #[test]
    fn held_text_is_in_pages_the_kernel_keeps_resident() {
        let _in_turn = THE_LIMIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let text = a_credential_longer_than_a_page();

        let held = LockedText::new(&text);

        assert_eq!(held.as_str(), text);
        assert!(
            held.is_locked(),
            "the kernel refused the lock; this machine's memory lock limit is {:?} bytes",
            memory_lock_limits().rlim_cur
        );
        let bytes = held.as_str().as_bytes();
        assert!(
            the_kernel_keeps_resident(bytes.as_ptr()),
            "the first page is swappable"
        );
        assert!(
            the_kernel_keeps_resident(&raw const bytes[bytes.len() - 1]),
            "the last page is swappable"
        );
    }

    /// A copy is a second credential and is held the same way, in pages of its own: two values
    /// sharing one mapping would unmap it twice, and a copy made on the heap would be swappable.
    #[test]
    fn a_copy_is_held_in_locked_pages_of_its_own() {
        let _in_turn = THE_LIMIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let held = LockedText::new("live-credential");

        let copy = held.clone();
        let copy_start = copy.as_str().as_ptr();
        // Before the original goes: a copy sharing its mapping would read unmapped pages after.
        if std::ptr::eq(copy_start, held.as_str().as_ptr()) {
            // Forgotten, so that unwinding does not clear and unmap the same pages twice.
            std::mem::forget(copy);
            panic!("the copy is the original's pages");
        }
        drop(held);

        assert_eq!(copy.as_str(), "live-credential");
        assert!(copy.is_locked());
        assert!(the_kernel_keeps_resident(copy_start));
    }

    /// Unmapping is what gives the lock back. A value that went without it would keep its pages
    /// locked for the rest of the run, against the one limit every later credential is locked
    /// under, until each new one was held unlocked.
    #[test]
    fn a_value_that_goes_gives_its_locked_pages_back() {
        let _in_turn = THE_LIMIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let held = LockedText::new("live-credential");
        let start = held.as_str().as_ptr();
        assert!(the_kernel_keeps_resident(start));

        drop(held);

        // Not that nothing is mapped there: another thread of this binary may map the address
        // again at once, and nothing but a test here locks what it maps.
        assert_ne!(
            residency(start),
            Some(true),
            "the pages are still locked after the value went"
        );
    }

    /// A process over its memory lock limit still has to be able to hold the credentials it
    /// resolves, and has to say that it is holding them unlocked rather than report a lock the
    /// kernel refused.
    ///
    /// A privileged process may lock past the limit, and there the kernel grants the lock anyway;
    /// what is asserted is that the value says what the kernel did, which holds either way.
    #[test]
    fn a_refused_lock_still_holds_the_value_and_says_so() {
        let _in_turn = THE_LIMIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = memory_lock_limits();
        let _restored = LimitRestored(before);
        set_memory_lock_limits(&libc::rlimit {
            rlim_cur: 0,
            rlim_max: before.rlim_max,
        });

        let held = LockedText::new("live-credential");

        assert_eq!(held.as_str(), "live-credential");
        assert_eq!(
            held.is_locked(),
            the_kernel_keeps_resident(held.as_str().as_ptr()),
            "the value misreports what the kernel did"
        );
    }

    /// The pages are unreachable once the value is gone, so what a test can run is the clearing
    /// rather than the drop, whose body is this call and then the unmapping, after which
    /// `a_value_that_goes_gives_its_locked_pages_back` finds the lock gone.
    /// Setting the length to nothing is the mistake available: it leaves every byte of the
    /// credential where it was.
    #[test]
    fn clearing_overwrites_the_bytes_where_they_lie() {
        // Not asserting on the lock, but taking one out of the same limit.
        let _in_turn = THE_LIMIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let text = a_credential_longer_than_a_page();
        let mut held = LockedText::new(&text);

        held.clear();

        assert_eq!(held.as_str().len(), text.len());
        assert!(held.as_str().bytes().all(|byte| byte == 0));
    }
}
