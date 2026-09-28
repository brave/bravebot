//! What a crash of this process is allowed to leave behind.
//!
//! The rest of this crate decides what a process running code we did not write may reach. This
//! module is the same kind of thing said about the process we are: an operating system call made
//! once, before anything worth protecting is in memory, about what the kernel may write out when
//! the process dies.
//!
//! It lives here rather than beside the credential it protects because it is a platform call and
//! nothing else. `bravebot-config`, which owns the type a credential is held in, forbids `unsafe`,
//! so what it has the kernel do about the pages one sits in goes through [`crate::swap`], and what
//! a crash may write out is decided here, once, for the whole process. See
//! [CRED-23](../../../docs/specs/credential-protection.md#CRED-23).

/// What this platform did about the memory image a crash would write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreDumps {
    /// Off for this process: the kernel writes no image of its memory when it dies.
    Disabled,
    /// The platform gives a process no say in it.
    ///
    /// Windows writes a user-mode dump only where an administrator turned Error Reporting's
    /// local dumps on, and the process it is written about cannot decline one. Reported rather
    /// than counted as success, because the two are different states to be in.
    NotOffered,
    /// The call the platform offers refused, and this is the error it reported.
    Refused(i32),
}

/// Stop this process leaving a copy of its memory behind when it dies.
///
/// Call it once, as early in `main` as the other things that have to be true before the first
/// thing that could go wrong: the signing key is unmasked into memory while the command line is
/// still being read, and an AWS session credential arrives with the first request.
///
/// Answering rather than refusing to start. A platform that will not do this is one where the
/// remedy is outside the program, and a process that quit over it would protect the credential by
/// making the product unusable. The answer is returned so that a caller which has somewhere to
/// report it can.
#[cfg(unix)]
pub fn disable_core_dumps() -> CoreDumps {
    let Some((_, hard)) = core_dump_limits() else {
        return CoreDumps::Refused(last_error());
    };
    // The soft limit only. Zero is what the kernel reads when it decides whether to write a dump
    // of this process, and a child inherits it, so this reaches every process a run starts as
    // well as this one. Lowering the hard limit too would take from a command somebody runs
    // through `run` the ability to raise its own again, which is a decision about their program
    // rather than about the credential in this one.
    let limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: hard,
    };
    if set_core_dump_limit(&limit) {
        CoreDumps::Disabled
    } else {
        CoreDumps::Refused(last_error())
    }
}

/// See the Unix implementation above. Nothing here is a no-op standing in for a call that exists.
#[cfg(not(unix))]
pub fn disable_core_dumps() -> CoreDumps {
    CoreDumps::NotOffered
}

/// The soft and hard limits on the size of a core dump of this process.
///
/// Reading the state back is how a test says what happened: a function reporting its own success
/// says nothing about whether the kernel agreed.
#[cfg(unix)]
// The exemption sits on the function because the function is the syscall.
#[allow(unsafe_code)]
fn core_dump_limits() -> Option<(libc::rlim_t, libc::rlim_t)> {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let read = unsafe { libc::getrlimit(libc::RLIMIT_CORE, &raw mut limit) };
    (read == 0).then_some((limit.rlim_cur, limit.rlim_max))
}

/// Whether the kernel accepted `limit` as this process's core dump limit.
#[cfg(unix)]
// The exemption sits on the function because the function is the syscall.
#[allow(unsafe_code)]
fn set_core_dump_limit(limit: &libc::rlimit) -> bool {
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let written = unsafe { libc::setrlimit(libc::RLIMIT_CORE, limit) };
    written == 0
}

/// What the last failing call reported, as a number a report can carry.
#[cfg(unix)]
fn last_error() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// The core dump limit belongs to the process, and both tests below move it, so they take it
    /// in turn.
    ///
    /// Run together, one test's fixture raising the limit lands between the other's call and its
    /// read, and the read sees the raise rather than the call: a failure at about one run in ten
    /// that says the kernel was left able to write a dump when the code under test is correct.
    /// Nothing else in this binary reads the limit, so the lock is between these two alone.
    static THE_LIMIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Put the core dump limit up, so that what the call under test does is visible.
    ///
    /// Without this the test measures the machine. A shell that already set the limit to zero,
    /// which is the default on more than one distribution and is the case on the host this was
    /// written on, agrees with a call that never reached the kernel at all.
    ///
    /// Returns the hard limit, which is what the soft one is raised to: any value the kernel will
    /// accept does, and the hard limit is the largest it will.
    fn raise_the_core_dump_limit() -> libc::rlim_t {
        let (_, hard) = core_dump_limits().expect("the limit is readable");
        assert_ne!(
            hard, 0,
            "this machine's hard limit is zero, so nothing can raise the soft one and this test \
             cannot tell a call that reached the kernel from one that did not"
        );
        let raised = libc::rlimit {
            rlim_cur: hard,
            rlim_max: hard,
        };
        assert!(
            set_core_dump_limit(&raised),
            "the fixture could not raise the limit it is about to watch come down"
        );
        hard
    }

    /// The state of the process is what the clause promises, so the state is what is read back.
    /// A call that reported success without reaching the kernel would satisfy an assertion made
    /// on its own answer.
    ///
    /// The limit is process wide, so this holds [`THE_LIMIT`] while it moves it. A suite that
    /// ends up writing no core dump is a suite behaving as the shipped binary does.
    #[test]
    fn disabling_core_dumps_leaves_the_kernel_unable_to_write_one() {
        let _in_turn = THE_LIMIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        raise_the_core_dump_limit();

        assert_eq!(disable_core_dumps(), CoreDumps::Disabled);

        let (soft, _) = core_dump_limits().expect("the limit is readable");
        assert_eq!(
            soft, 0,
            "the kernel still has room to write this process's memory out"
        );
    }

    /// A command somebody runs through this program is their program, and taking away its ability
    /// to raise its own limit again protects nothing here: the credential is in this process's
    /// memory rather than in its child's.
    #[test]
    fn disabling_core_dumps_does_not_lower_the_hard_limit() {
        let _in_turn = THE_LIMIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = raise_the_core_dump_limit();

        assert_eq!(disable_core_dumps(), CoreDumps::Disabled);

        let (_, after) = core_dump_limits().expect("the limit is readable");
        assert_eq!(after, before, "the hard limit moved");
    }
}
