//! Windows confinement via AppContainer.
//!
//! A lowbox token is default-deny against every securable object on the machine, so a
//! process holding one reaches nothing it has not been granted by name. A grant is an
//! access-control entry for the container's own security identifier, written onto the
//! directory the policy names; egress is a capability, so it is denied by leaving the
//! capability out rather than by a rule that has to be got right.
//!
//! What that costs, and what neither the Seatbelt nor the Landlock backend costs, is
//! that a grant is a change to the filesystem rather than to this process: it is on the
//! directory when the confined process starts and it is still there afterwards unless
//! something removes it. [`AppContainerSandbox`] removes the grants it wrote and the
//! profile it created as it is dropped, and a run that ends without reaching that leaves
//! entries behind. They name a security identifier no other run uses, because the
//! profile is this run's alone, so what is left is an entry for a container that no
//! longer exists rather than a standing grant to something still running.
//!
//! Subprocess denial is not enforceable here: a container bounds what a process reaches,
//! not whether it creates children, and a child of a confined process is in the same
//! container rather than outside it. A policy asking for that denial is refused
//! ([`refusal_for`]) rather than applied without it.
//!
//! Compiled under test on every platform as well as on the one it confines, so what this
//! backend decides before a process starts is pinned by every job that runs the suite:
//! which capability the policy asks for, what each grant permits, and which policies are
//! refused. The Win32 calls that apply those decisions are in [`appcontainer`], which
//! only the Windows build compiles and only `make check-windows` and CI's Windows job
//! check.

use crate::SandboxError;
use crate::policy::{Capabilities, ConfinementLevel, SandboxPolicy};
use std::path::{Path, PathBuf};

/// The capability that lets a confined process open an outbound socket.
///
/// Named rather than numbered: capabilities are resolved from this name by the platform,
/// and a well-known one spelled wrong derives a security identifier that grants nothing,
/// which would read as egress denied and be egress denied by accident.
const INTERNET_CLIENT: &str = "internetClient";

/// What a container is granted over a path the policy names.
///
/// The mask is a Win32 access mask and the inheritance is Win32 ACE flags, spelled here
/// as numbers so that what is granted is decided by something every platform's test run
/// can call. The Windows build holds each of them to the platform constant it stands for,
/// so a number here cannot drift from the right it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Grant {
    /// The rights the entry allows.
    mask: u32,
    /// How the entry reaches what is under the directory it is written on.
    inheritance: u32,
}

/// `FILE_GENERIC_READ`: open, read, and read the entry's own attributes.
const FILE_GENERIC_READ: u32 = 0x0012_0089;
/// `FILE_GENERIC_WRITE`: create, write, append, and write the entry's own attributes.
const FILE_GENERIC_WRITE: u32 = 0x0012_0116;
/// `FILE_GENERIC_EXECUTE`: run a program from the path.
const FILE_GENERIC_EXECUTE: u32 = 0x0012_00A0;
/// `DELETE`: remove an entry, and the right the source of a rename needs.
const DELETE: u32 = 0x0001_0000;
/// `SUB_CONTAINERS_AND_OBJECTS_INHERIT`: the entry reaches files and directories under
/// the one it is written on.
const SUB_CONTAINERS_AND_OBJECTS_INHERIT: u32 = 3;

/// What a path the policy lists as readable is granted.
///
/// Execution comes with reading rather than separately, because a policy naming a
/// directory of programs means the confined process to run them and there is no list of
/// paths it may read but not execute. Landlock and Seatbelt both carry execution in their
/// read grant, so splitting it here would make the same policy mean a different thing on
/// this platform.
const fn grant_for_reading() -> Grant {
    Grant {
        mask: FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
        inheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
    }
}

/// What a path the policy lists as writable is granted.
///
/// `DELETE` as well as the write rights, because [SANDBOX-7] requires a write grant to
/// cover moving a file within it and the source of a rename is deleted from where it was.
///
/// `FILE_ALL_ACCESS` is not what this is: it carries `WRITE_DAC` and `WRITE_OWNER`, and a
/// confined process holding either can write itself an entry for any right it likes on
/// anything under the path, which is a container that grants whatever it is asked for.
///
/// [SANDBOX-7]: ../../../docs/specs/sandboxing.md
const fn grant_for_writing() -> Grant {
    Grant {
        mask: FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE,
        inheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
    }
}

/// Every grant this policy asks to be written, in the order they are written.
///
/// A path named in both lists is granted once, for writing, since a second entry for the
/// same container on the same path adds nothing the first did not already allow and
/// leaves a second entry to remove.
fn grants_for(policy: &SandboxPolicy) -> Vec<(PathBuf, Grant)> {
    let mut grants: Vec<(PathBuf, Grant)> = Vec::new();
    for row in &policy.writable {
        if !grants.iter().any(|(granted, _)| *granted == row.path) {
            grants.push((row.path.clone(), grant_for_writing()));
        }
    }
    for path in &policy.readable {
        if !grants.iter().any(|(granted, _)| granted == path) {
            grants.push((path.clone(), grant_for_reading()));
        }
    }
    grants
}

/// The capabilities the token is built with, which is what the policy asks for and never
/// more.
///
/// A capability is the only thing an AppContainer holds beyond its grants, so an empty
/// list is a process that reaches neither the network nor anything else the platform
/// gates this way. That is what makes [`Capabilities::network_denial_enforced`] true here
/// rather than aspirational: denial is the absence of a grant, not a rule to apply.
fn capability_names(policy: &SandboxPolicy) -> Vec<&'static str> {
    if policy.allow_network {
        vec![INTERNET_CLIENT]
    } else {
        Vec::new()
    }
}

/// Why this backend will not apply `policy`, where it will not.
///
/// Decided before anything is created, so a refusal costs no profile and writes no entry
/// onto a directory.
fn refusal_for(policy: &SandboxPolicy) -> Option<SandboxError> {
    if !policy.is_meaningful() {
        return Some(SandboxError::PolicyTooPermissive);
    }

    if !policy.allow_subprocesses {
        return Some(SandboxError::SetupFailed {
            mechanism: "appcontainer",
            detail: "a container bounds what a process reaches and not whether it creates \
                     children, so subprocess denial is not enforced here; refusing rather \
                     than reporting confinement that is not applied"
                .into(),
        });
    }

    None
}

/// The paths `policy` names that `on_disk` says are not there, each named once.
///
/// An entry is written onto an object, so a path that is not on disk is a grant this
/// backend cannot install, and installing the rest of the policy instead would confine
/// the process to fewer paths than the policy names with nothing saying so
/// ([SANDBOX-6]).
///
/// The existence test is a parameter so the answer is decided by something a test can
/// call with either answer, rather than by what the machine running the suite happens to
/// carry.
///
/// [SANDBOX-6]: ../../../docs/specs/sandboxing.md
fn paths_that_are_not_there(
    policy: &SandboxPolicy,
    on_disk: impl Fn(&Path) -> bool,
) -> Vec<PathBuf> {
    let mut missing: Vec<PathBuf> = Vec::new();
    for (path, _) in grants_for(policy) {
        if !on_disk(&path) && !missing.contains(&path) {
            missing.push(path);
        }
    }
    missing
}

/// What an AppContainer enforces here.
fn capabilities() -> Capabilities {
    Capabilities {
        // The filesystem and the network are both the kernel's decision: a lowbox token
        // is denied every object whose list does not name the container, and egress needs
        // a capability the token either carries or does not. Subprocess denial is not
        // enforced, which is why this is not reported as full kernel confinement, and a
        // policy asking for it is refused rather than run under this level.
        level: ConfinementLevel::Partial,
        mechanisms: vec!["appcontainer"],
        network_denial_enforced: true,
        // An entry is written onto an object, and there is nothing at a path that is not
        // on disk to write one onto.
        grants_paths_that_do_not_exist: false,
    }
}

/// The name of the container profile a run confines through.
///
/// One per backend rather than one per installation, because a grant outlives the process
/// it was written for when a run is interrupted. A name no other run uses means the entry
/// left behind names a container that no longer exists, so the residue is unreachable
/// rather than a standing grant to whatever holds the shared name next.
///
/// The platform accepts up to 64 characters, and rejects the whole profile rather than
/// truncating, so the two numbers are the only variable part and both are bounded by their
/// own width.
fn profile_name(process: u32, sequence: u64) -> String {
    format!("bravebot-{process}-{sequence}")
}

/// The longest profile name the platform accepts.
///
/// Held by a test rather than checked at runtime: both numbers in a name are bounded by
/// their own width, so the longest one this can produce is known here.
#[cfg(test)]
const LONGEST_PROFILE_NAME: usize = 64;

/// The command line `CreateProcessW` is given, which is the one thing on this platform
/// that is a string rather than a vector.
///
/// Windows starts a process from a single command line and leaves the program to split
/// it, so an argument containing a space or a quotation mark is two arguments unless it
/// is quoted here. A policy's paths reach this as arguments, so an unquoted one is a path
/// deciding what the program was asked to do ([SANDBOX-4]).
///
/// The rules are the ones `CommandLineToArgvW` parses back, which is what the C runtime
/// and every language runtime on this platform use: every argument is quoted, a backslash
/// run immediately before the closing quotation mark is doubled, and a backslash run
/// before an embedded quotation mark is doubled and the quotation mark escaped. A
/// backslash anywhere else is literal, so a path is not rewritten by being quoted.
///
/// [SANDBOX-4]: ../../../docs/specs/sandboxing.md
fn command_line(program: &str, args: &[String]) -> String {
    let mut line = quoted(program);
    for arg in args {
        line.push(' ');
        line.push_str(&quoted(arg));
    }
    line
}

/// One argument, written so `CommandLineToArgvW` gives it back unchanged.
fn quoted(arg: &str) -> String {
    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    let mut backslashes = 0usize;
    for character in arg.chars() {
        match character {
            '\\' => {
                backslashes += 1;
                quoted.push('\\');
            }
            '"' => {
                // The run before a quotation mark is doubled so it is read as backslashes
                // rather than as escapes, and the quotation mark is escaped so it is read
                // as content rather than as the end of the argument.
                quoted.extend(std::iter::repeat_n('\\', backslashes + 1));
                backslashes = 0;
                quoted.push('"');
            }
            other => {
                backslashes = 0;
                quoted.push(other);
            }
        }
    }
    // The run before the closing quotation mark is doubled for the same reason.
    quoted.extend(std::iter::repeat_n('\\', backslashes));
    quoted.push('"');
    quoted
}

#[cfg(windows)]
pub use appcontainer::AppContainerSandbox;
#[cfg(windows)]
pub(crate) use appcontainer::CreatedProcess;

#[cfg(windows)]
mod appcontainer;

#[cfg(test)]
mod tests {
    use super::*;

    /// A policy that grants everything this backend can withhold, so a test about one
    /// refusal is not quietly a test about another.
    fn a_policy_this_backend_applies() -> SandboxPolicy {
        SandboxPolicy::strict()
            .allow_subprocesses()
            .allow_read("/workspace")
    }

    /// Egress is denied by the absence of a capability rather than by a rule, so a
    /// backend that asked for the capability regardless would report network denial
    /// enforced and enforce nothing.
    #[test]
    fn a_policy_that_did_not_ask_for_the_network_asks_for_no_capability() {
        assert!(capability_names(&a_policy_this_backend_applies()).is_empty());
    }

    /// The other half: a policy that did ask for egress has to get it, or a stage that
    /// needs the network fails while the record says the policy was applied.
    #[test]
    fn a_policy_that_asked_for_the_network_asks_for_the_internet_client_capability() {
        let policy = a_policy_this_backend_applies().allow_network_egress();
        assert_eq!(capability_names(&policy), vec!["internetClient"]);
    }

    /// `FILE_WRITE_DATA`, `FILE_APPEND_DATA` and `FILE_WRITE_EA`: the rights that change
    /// what is at a path, as against the ones every grant carries.
    const CHANGES_WHAT_IS_THERE: u32 = 0x0000_0002 | 0x0000_0004 | 0x0000_0010;

    /// A read grant is what a policy said it was. An entry carrying the write rights
    /// would let a program the policy meant to read a directory rewrite it, and nothing
    /// about the policy would say so.
    #[test]
    fn a_path_granted_for_reading_is_not_granted_writing_or_deleting() {
        let reading = grant_for_reading();
        assert_eq!(reading.mask & CHANGES_WHAT_IS_THERE, 0);
        assert_eq!(reading.mask & DELETE, 0);
        assert_eq!(reading.mask & FILE_GENERIC_READ, FILE_GENERIC_READ);
    }

    /// SANDBOX-7: writing a temporary file and renaming it into place is how a compiler
    /// and a package manager write anything, and the source of a rename is deleted from
    /// where it was, so a write grant without `DELETE` holds a program to less than the
    /// policy granted it.
    #[test]
    fn a_path_granted_for_writing_can_be_written_and_moved_within() {
        let writing = grant_for_writing();
        assert_eq!(
            writing.mask & CHANGES_WHAT_IS_THERE,
            CHANGES_WHAT_IS_THERE,
            "a write grant that cannot change what is at the path"
        );
        assert_eq!(writing.mask & DELETE, DELETE);
    }

    /// A container that can rewrite an access list can grant itself anything, so the
    /// grant is bounded rights and not `FILE_ALL_ACCESS`.
    #[test]
    fn no_grant_lets_a_confined_process_rewrite_an_access_list() {
        const WRITE_DAC: u32 = 0x0004_0000;
        const WRITE_OWNER: u32 = 0x0008_0000;
        for grant in [grant_for_reading(), grant_for_writing()] {
            assert_eq!(grant.mask & (WRITE_DAC | WRITE_OWNER), 0);
        }
    }

    /// A grant is written on the directory the policy names, and what the policy means is
    /// the tree under it. An entry that is not inherited reaches the directory entry
    /// alone, so every file in it stays unreachable and the policy reads as applied.
    #[test]
    fn a_grant_reaches_what_is_under_the_directory_it_is_written_on() {
        for grant in [grant_for_reading(), grant_for_writing()] {
            assert_eq!(grant.inheritance, SUB_CONTAINERS_AND_OBJECTS_INHERIT);
        }
    }

    /// Resolution keeps a path in the list it was named in, and so does this: a path the
    /// policy wanted read must not arrive at the platform carrying the write rights.
    #[test]
    fn each_path_is_granted_what_the_list_it_was_named_in_asks_for() {
        let policy = a_policy_this_backend_applies().allow_write("/workspace/out");

        assert_eq!(
            grants_for(&policy),
            vec![
                (PathBuf::from("/workspace/out"), grant_for_writing()),
                (PathBuf::from("/workspace"), grant_for_reading()),
            ]
        );
    }

    /// A path in both lists is one entry to write and one to remove. Two entries for the
    /// same container on the same path allow nothing the first did not, and the second is
    /// residue nothing later looks for.
    #[test]
    fn a_path_named_for_reading_and_for_writing_is_granted_once_for_writing() {
        let policy = SandboxPolicy::strict()
            .allow_subprocesses()
            .allow_read("/workspace")
            .allow_write("/workspace");

        assert_eq!(
            grants_for(&policy),
            vec![(PathBuf::from("/workspace"), grant_for_writing())]
        );
    }

    /// SANDBOX-2, on this backend: a policy granting everything is not a confinement
    /// decision, and applying one would present a container that bounds nothing as a
    /// sandbox.
    #[test]
    fn a_fully_permissive_policy_is_refused() {
        let policy = SandboxPolicy::strict()
            .allow_network_egress()
            .allow_subprocesses()
            .allow_write("/");

        assert!(matches!(
            refusal_for(&policy),
            Some(SandboxError::PolicyTooPermissive)
        ));
    }

    /// A container does not stop a process creating children, so a policy asking for that
    /// denial is refused. Applying the rest would run the program with the record saying
    /// children were denied.
    #[test]
    fn a_policy_requiring_subprocess_denial_is_refused() {
        let policy = SandboxPolicy::strict().allow_read("/workspace");

        let refusal = refusal_for(&policy).expect("subprocess denial is not enforceable here");
        assert!(matches!(refusal, SandboxError::SetupFailed { .. }));
        assert!(refusal.to_string().contains("refusing"));
    }

    /// The denial this backend does enforce must not be refused with the one it does not:
    /// a policy withholding egress is the ordinary case and has to be applied.
    #[test]
    fn a_policy_withholding_the_network_is_applied_rather_than_refused() {
        assert!(refusal_for(&a_policy_this_backend_applies()).is_none());
    }

    /// SANDBOX-6: a path the backend cannot grant is named rather than dropped, because a
    /// process confined to fewer paths than the policy lists is the degradation
    /// SANDBOX-1 forbids reached one grant at a time.
    #[test]
    fn a_path_that_is_not_on_disk_is_named_rather_than_left_out() {
        let policy = a_policy_this_backend_applies().allow_write("/workspace/out");

        let missing = paths_that_are_not_there(&policy, |path| path != Path::new("/workspace/out"));

        assert_eq!(missing, vec![PathBuf::from("/workspace/out")]);
    }

    /// The other answer: a policy every path of which is there is applied, so a backend
    /// refusing on a path it could have granted is caught here rather than by a program
    /// that never starts.
    #[test]
    fn a_policy_whose_paths_are_all_there_names_none() {
        let policy = a_policy_this_backend_applies().allow_write("/workspace/out");

        assert!(paths_that_are_not_there(&policy, |_| true).is_empty());
    }

    /// Two runs sharing a profile share a security identifier, and then an entry one of
    /// them left behind is a live grant to whatever the other is running.
    #[test]
    fn each_run_confines_through_a_profile_of_its_own() {
        assert_ne!(profile_name(4, 1), profile_name(4, 2));
        assert_ne!(profile_name(4, 1), profile_name(5, 1));
    }

    /// The platform rejects a name longer than this rather than truncating it, so a run
    /// on a machine handing out long process identifiers would have no confinement at
    /// all.
    #[test]
    fn a_profile_name_fits_what_the_platform_accepts() {
        let longest = profile_name(u32::MAX, u64::MAX);
        assert!(
            longest.len() <= LONGEST_PROFILE_NAME,
            "{longest} is {} characters",
            longest.len()
        );
    }

    /// SANDBOX-4 on a platform whose process creation takes one string: a path with a
    /// space in it is one argument, and a backend that did not quote it would hand the
    /// program two.
    #[test]
    fn a_path_containing_a_space_reaches_the_program_as_one_argument() {
        assert_eq!(
            command_line("C:\\Program Files\\server.exe", &["a b".to_string()]),
            r#""C:\Program Files\server.exe" "a b""#
        );
    }

    /// The hostile case: a quotation mark in an argument would otherwise end it, and
    /// everything after it would be read as further arguments the caller never passed.
    #[test]
    fn a_quotation_mark_in_an_argument_does_not_end_it() {
        assert_eq!(
            command_line("s.exe", &[r#"a" --elsewhere "b"#.to_string()]),
            r#""s.exe" "a\" --elsewhere \"b""#
        );
    }

    /// A path ends in a separator often enough to matter, and a trailing backslash left
    /// undoubled escapes the closing quotation mark, which joins the argument to the next
    /// one.
    #[test]
    fn a_path_ending_in_a_separator_does_not_swallow_the_argument_after_it() {
        assert_eq!(
            command_line("s.exe", &["C:\\dir\\".to_string(), "next".to_string()]),
            r#""s.exe" "C:\dir\\" "next""#
        );
    }

    /// A backslash that is not before a quotation mark is content, so quoting must not
    /// rewrite an ordinary path into one naming a different directory.
    #[test]
    fn quoting_a_path_leaves_the_path_it_names_alone() {
        assert_eq!(
            command_line("s.exe", &["C:\\dir\\file".to_string()]),
            r#""s.exe" "C:\dir\file""#
        );
    }

    /// An argument with nothing in it is still an argument, and one written unquoted
    /// disappears, shifting every argument after it.
    #[test]
    fn an_empty_argument_is_still_an_argument() {
        assert_eq!(
            command_line("s.exe", &[String::new(), "after".to_string()]),
            r#""s.exe" "" "after""#
        );
    }

    /// SANDBOX-5: what is reported is what a container actually enforces. An overstated
    /// level is the same failure as a silent fallback, and an understated
    /// `grants_paths_that_do_not_exist` costs the caller a grant it did not mean to lose.
    #[test]
    fn capabilities_report_what_a_container_enforces() {
        let reported = capabilities();
        assert_eq!(reported.level, ConfinementLevel::Partial);
        assert!(reported.network_denial_enforced);
        assert!(!reported.grants_paths_that_do_not_exist);
        assert!(reported.mechanisms.contains(&"appcontainer"));
    }
}
