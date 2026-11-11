//! Pins that what a confined program's child does between the fork and the exec allocates
//! nothing.
//!
//! A child made by `fork` in a process with other threads holds a copy of whichever allocator lock
//! another thread held at that moment, and nothing releases it there. The child therefore must not
//! call the allocator, which is part of what makes its work async-signal-safe. The allocator here
//! ends the process at once, with a code of its own, when a thread that is armed calls it. A test
//! forks, arms the thread in the child, runs the work, and reads the child's exit code. Ending the
//! child at the call means a child that would have blocked in the allocator is reported instead
//! of hanging the run, and a wait with a deadline covers a child that blocks elsewhere.
//!
//! The harness is compiled on every Unix so that the allocator and the wait are tested where a
//! developer is likely to run them. The tests of the work itself need Linux.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::{Duration, Instant};

thread_local! {
    /// Whether a call to the allocator by this thread ends the process. A `const` initialiser
    /// with no destructor, so reading it does not allocate.
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

/// The exit code of a child that called the allocator while armed.
const CALLED_THE_ALLOCATOR: i32 = 3;
/// The exit code of a child whose work succeeded without calling the allocator.
const NO_ALLOCATION: i32 = 0;
/// The exit code of a child whose work returned a failure without calling the allocator.
const WORK_FAILED: i32 = 4;
/// What [`in_a_forked_child`] returns for a child that did not end within the deadline, and for
/// one that ended on a signal.
const DID_NOT_EXIT: i32 = -1;
/// How long a child may take. The work is a few system calls.
const DEADLINE: Duration = Duration::from_secs(30);

/// The system allocator, ending the process when an armed thread calls it.
struct Guarding;

#[allow(
    unsafe_code,
    reason = "`_exit` takes an integer; it is called from the allocator, which cannot return an error to the caller that must not allocate"
)]
fn leave_if_armed() {
    if ARMED.get() {
        // SAFETY: `_exit` takes an integer and never returns.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { libc::_exit(CALLED_THE_ALLOCATOR) }
    }
}

#[allow(
    unsafe_code,
    reason = "a `GlobalAlloc` implementation is an unsafe impl; this one passes every call to the system allocator after the armed check"
)]
// SAFETY: every method forwards its arguments unchanged to `System`, which upholds the contract
// of `GlobalAlloc`; the armed check touches only a const thread-local, which does not allocate.
// nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
unsafe impl GlobalAlloc for Guarding {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        leave_if_armed();
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract for `layout`.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        leave_if_armed();
        // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s contract for `layout`.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        leave_if_armed();
        // SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract for these arguments.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // A free takes the allocator's lock as an allocation does.
        leave_if_armed();
        // SAFETY: the caller upholds `GlobalAlloc::dealloc`'s contract for these arguments.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Guarding = Guarding;

/// Forks, runs `work` in the child with the thread armed, and returns the child's exit code:
/// [`NO_ALLOCATION`] when it returned true without calling the allocator, [`WORK_FAILED`] when it
/// returned false without calling it, and [`CALLED_THE_ALLOCATOR`] when it called it. A child that
/// has not ended within `deadline` is killed, and the result is [`DID_NOT_EXIT`], as it is for one
/// that ended on a signal.
#[allow(
    unsafe_code,
    reason = "forks a child that runs only `work`, which the caller keeps to system calls, and leaves with `_exit`; the parent waits for it and kills it at the deadline"
)]
fn in_a_forked_child_within(deadline: Duration, work: impl FnOnce() -> bool) -> i32 {
    // SAFETY: the child runs `work` and leaves with `_exit`, never returning into the test
    // harness. The parent waits for exactly that child, and kills and collects it at the deadline.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        let child = libc::fork();
        assert!(child >= 0, "fork");
        if child == 0 {
            ARMED.set(true);
            let succeeded = work();
            ARMED.set(false);
            libc::_exit(if succeeded {
                NO_ALLOCATION
            } else {
                WORK_FAILED
            });
        }
        let started = Instant::now();
        let mut status = 0;
        loop {
            match libc::waitpid(child, &mut status, libc::WNOHANG) {
                waited if waited == child => break,
                0 if started.elapsed() > deadline => {
                    libc::kill(child, libc::SIGKILL);
                    libc::waitpid(child, &mut status, 0);
                    return DID_NOT_EXIT;
                }
                0 => std::thread::sleep(Duration::from_millis(5)),
                _ => assert!(
                    std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted,
                    "waitpid"
                ),
            }
        }
        if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else {
            DID_NOT_EXIT
        }
    }
}

fn in_a_forked_child(work: impl FnOnce() -> bool) -> i32 {
    in_a_forked_child_within(DEADLINE, work)
}

/// A call to the allocator by an armed thread ends the child with its own code, whichever of the
/// four calls it is, so a zero from the tests of the work means the work made none of them.
///
/// The regression it rejects: an allocator that never checks the thread, or checks it in only
/// some of the four calls, which would let every test of the work pass whatever the work did.
#[test]
fn a_call_to_the_allocator_by_an_armed_thread_ends_the_child() {
    type Case = (&'static str, fn() -> bool);
    let cases: [Case; 4] = [
        ("an allocation", || {
            std::hint::black_box(Box::new(7_u64));
            true
        }),
        ("a zeroed allocation", || {
            std::hint::black_box(vec![0_u8; 64]);
            true
        }),
        ("a reallocation", || {
            ARMED.set(false);
            let mut grown = Vec::<u8>::with_capacity(1);
            ARMED.set(true);
            grown.extend_from_slice(&[0; 4096]);
            std::hint::black_box(&grown);
            ARMED.set(false);
            true
        }),
        ("a free", || {
            ARMED.set(false);
            let held = Box::new(7_u64);
            ARMED.set(true);
            drop(std::hint::black_box(held));
            true
        }),
    ];
    for (case, work) in cases {
        assert_eq!(in_a_forked_child(work), CALLED_THE_ALLOCATOR, "{case}");
    }
}

/// A thread that is not armed allocates as usual, and work that makes no call ends with the code
/// for its result.
///
/// The regression it rejects: an allocator that ends the child whatever the thread, which would
/// fail every test of the work for a reason that is not the work's.
#[test]
fn a_call_to_the_allocator_while_disarmed_is_allowed_and_work_without_one_reports_its_result() {
    let allocated_while_disarmed = in_a_forked_child(|| {
        ARMED.set(false);
        drop(std::hint::black_box(Box::new(7_u64)));
        ARMED.set(true);
        true
    });
    assert_eq!(allocated_while_disarmed, NO_ALLOCATION);
    assert_eq!(in_a_forked_child(|| true), NO_ALLOCATION);
    assert_eq!(in_a_forked_child(|| false), WORK_FAILED);
}

/// A child that blocks is killed at the deadline and reported, so a run does not wait on it
/// forever.
///
/// The regression it rejects: a wait with no deadline, which hangs the whole test run on a child
/// that never ends.
#[test]
fn a_child_that_blocks_is_killed_at_the_deadline() {
    #[allow(
        unsafe_code,
        reason = "`pause` takes no argument and returns when a signal arrives"
    )]
    fn block() -> bool {
        loop {
            // SAFETY: `pause` takes no argument.
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            unsafe { libc::pause() };
        }
    }

    let started = Instant::now();
    let code = in_a_forked_child_within(Duration::from_millis(200), block);
    assert_eq!(code, DID_NOT_EXIT);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the wait ended at its deadline"
    );
}

// The tests of the work a confined program's child does, which is the Linux backend's.
#[cfg(target_os = "linux")]
use crate::linux::{ChildWork, Held, Recheck, built_ruleset};
#[cfg(target_os = "linux")]
use crate::netns::{self, Namespace};
#[cfg(target_os = "linux")]
use crate::policy::Enumeration;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

/// The work done between the fork and the exec of a confined program allocates nothing, over
/// every kind of row it looks at again: a named row, a listed entry that is there, one that is
/// gone, and a row above a refusal. The ruleset is restricted in the child, which the test
/// process is not.
///
/// The regression it rejects: the closure building its ruleset, formatting an error or
/// opening rows after the fork, which hangs a child whose parent had another thread inside the
/// allocator at the fork. Such a child calls the allocator here, and the test sees code 3.
#[cfg(target_os = "linux")]
#[test]
fn the_work_between_the_fork_and_the_exec_allocates_nothing() {
    let readable = Enumeration {
        named: vec![PathBuf::from("/")],
        above_a_refusal: Vec::new(),
        listed: Vec::new(),
    };
    let writable = Enumeration::default();
    let mut rechecks = Vec::new();
    let ruleset = match built_ruleset(&readable, &writable, &[], &mut rechecks) {
        Ok(ruleset) => ruleset,
        Err(error) => {
            eprintln!("skipped: this kernel builds no Landlock ruleset ({error})");
            return;
        }
    };
    rechecks.push(Recheck::new(Path::new("/"), false, Held::Directory).expect("a path"));
    rechecks.push(Recheck::new(Path::new("/"), true, Held::Listed).expect("a path"));
    rechecks.push(
        Recheck::new(Path::new("/bravebot-no-such-entry"), false, Held::Listed).expect("a path"),
    );
    let work = ChildWork {
        namespace: None,
        ruleset,
        rechecks,
    };

    assert_eq!(in_a_forked_child(|| work.run().is_ok()), NO_ALLOCATION);
}

/// A refusal in the work, a named row that is gone, is returned without calling the allocator
/// either, so the error path of the closure is as safe as the success path.
///
/// The regression it rejects: an error built with `format!` or `io::Error::other(String)` in
/// the closure, which allocates.
#[cfg(target_os = "linux")]
#[test]
fn a_refusal_between_the_fork_and_the_exec_allocates_nothing() {
    let readable = Enumeration {
        named: vec![PathBuf::from("/")],
        above_a_refusal: Vec::new(),
        listed: Vec::new(),
    };
    let mut rechecks = Vec::new();
    let Ok(ruleset) = built_ruleset(&readable, &Enumeration::default(), &[], &mut rechecks) else {
        eprintln!("skipped: this kernel builds no Landlock ruleset");
        return;
    };
    rechecks.push(
        Recheck::new(Path::new("/bravebot-no-such-row"), false, Held::Directory).expect("a path"),
    );
    let work = ChildWork {
        namespace: None,
        ruleset,
        rechecks,
    };

    // The work failed, and nothing was allocated in failing.
    assert_eq!(in_a_forked_child(|| work.run().is_ok()), WORK_FAILED);
}

/// Entering a namespace allocates nothing, with and without the forwarder, which is what lets
/// the closure that calls it be run in a forked child.
///
/// The regression it rejects: `enter` formatting the id maps or a path after the fork.
#[cfg(target_os = "linux")]
#[test]
fn entering_a_namespace_allocates_nothing() {
    if let Err(reason) = netns::available() {
        eprintln!("skipped: no namespace can be made here ({reason})");
        return;
    }
    // The name carries the process id, and the forwarder only binds a socket at it; nothing
    // else is created in the shared directory.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let directory = std::env::temp_dir();
    let socket = directory.join(format!("bravebot-alloc-probe-{}.sock", std::process::id()));
    for forward in [None, Some((8123_u16, socket.as_path()))] {
        let namespace = Namespace::prepare(forward).expect("a namespace");
        assert_eq!(
            in_a_forked_child(|| namespace.enter().is_ok()),
            NO_ALLOCATION,
            "forwarding: {}",
            forward.is_some()
        );
    }
}
