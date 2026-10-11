// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Whether another process of the same user may reach into this one.
//!
//! A control channel is an anonymous socket held only by the two processes at its ends. On Linux a
//! process of the same user can still copy a descriptor out of another with `pidfd_getfd`, attach
//! with `ptrace` or read `/proc/<pid>/mem`, when the kernel's ptrace policy (`ptrace_scope` 0, the
//! default on some distributions) lets it. Marking the process non-dumpable makes the kernel refuse
//! all three to anyone without `CAP_SYS_PTRACE`. The mark is cleared by `execve`, so a program this
//! process starts is not affected. See
//! [RPCVIEW-8](../../../docs/specs/session-view.md#RPCVIEW-8).

/// What this platform did about same-user inspection of this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inspection {
    /// The kernel refuses same-user `ptrace`, `pidfd_getfd` and `/proc/<pid>/mem`.
    Refused,
    /// The platform offers no such switch, so a same-user process may still try.
    NotOffered,
    /// The call the platform offers failed, with this error.
    Failed(i32),
}

impl Inspection {
    /// A line for whoever runs this process when the remedy failed, and nothing when it worked or
    /// the platform has none, which is the ordinary state on macOS.
    pub fn notice(&self) -> Option<String> {
        match self {
            Self::Failed(error) => Some(format!(
                "this process could not be made non-dumpable (error {error}), so another process \
                 of the same user may be able to copy the descriptors of its control channel"
            )),
            Self::Refused | Self::NotOffered => None,
        }
    }
}

/// Stop other processes of this user reading this one's memory or descriptors.
///
/// Call it once at startup, as `disable_core_dumps` is called. A failure is returned rather than
/// ending the process, because a refusal to start would make the product unusable on a system where
/// the remedy is outside the program.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub fn refuse_same_user_inspection() -> Inspection {
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let set = unsafe {
        libc::prctl(
            libc::PR_SET_DUMPABLE,
            0 as libc::c_ulong,
            0 as libc::c_ulong,
            0 as libc::c_ulong,
            0 as libc::c_ulong,
        )
    };
    if set == 0 {
        Inspection::Refused
    } else {
        Inspection::Failed(std::io::Error::last_os_error().raw_os_error().unwrap_or(0))
    }
}

/// See the Linux implementation. macOS gives a process no switch that stops a same-user debugger
/// without also blocking the developer's own, and Windows is not covered.
#[cfg(not(target_os = "linux"))]
pub fn refuse_same_user_inspection() -> Inspection {
    Inspection::NotOffered
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    /// The mark belongs to the whole process and the other tests of this binary run in it, so the
    /// check runs in a copy of the binary started for it alone.
    #[test]
    fn the_kernel_marks_the_process_non_dumpable() {
        let ran = std::process::Command::new(std::env::current_exe().expect("this binary"))
            .args([
                "--exact",
                "inspection::tests::the_marked_process_reads_back_non_dumpable",
            ])
            .env(CHILD, "1")
            .output()
            .expect("the binary starts again");
        let said = String::from_utf8_lossy(&ran.stdout);
        assert!(ran.status.success(), "{said}");
        // A name that matches nothing runs no test and succeeds all the same.
        assert!(
            said.contains("running 1 test"),
            "the check never ran: {said}"
        );
    }

    const CHILD: &str = "BRAVEBOT_INSPECTION_TEST_CHILD";

    /// Runs the check for the test above, and does nothing when run on its own with the others.
    /// The kernel's own record is read back, not the answer the call gave.
    #[test]
    #[allow(unsafe_code)]
    fn the_marked_process_reads_back_non_dumpable() {
        if std::env::var_os(CHILD).is_none() {
            return;
        }
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        assert_eq!(
            unsafe {
                libc::prctl(
                    libc::PR_SET_DUMPABLE,
                    1 as libc::c_ulong,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong,
                )
            },
            0
        );
        assert_eq!(refuse_same_user_inspection(), Inspection::Refused);
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        assert_eq!(
            unsafe {
                libc::prctl(
                    libc::PR_GET_DUMPABLE,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong,
                )
            },
            0
        );
    }
}

#[cfg(all(test, not(target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn a_platform_without_the_switch_says_so() {
        assert_eq!(refuse_same_user_inspection(), Inspection::NotOffered);
    }
}

#[cfg(test)]
mod notice_tests {
    use super::*;

    #[test]
    fn a_failed_remedy_is_said_with_its_error() {
        let notice = Inspection::Failed(1).notice().expect("a failure is said");
        assert!(notice.contains("error 1"), "{notice}");
        assert!(notice.contains("same user"), "{notice}");
    }

    #[test]
    fn a_remedy_that_worked_or_that_the_platform_lacks_says_nothing() {
        assert_eq!(Inspection::Refused.notice(), None);
        assert_eq!(Inspection::NotOffered.notice(), None);
    }
}
