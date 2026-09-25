//! macOS confinement via Seatbelt.
//!
//! Wraps the target command in `sandbox-exec` with a generated deny-by-default
//! profile. The mechanism is what Chrome and Firefox use for their renderer
//! processes; the `sandbox_init` C API is marked deprecated but has no supported
//! replacement for binaries distributed outside the App Store, and `sandbox-exec`
//! remains present on supported macOS versions.
//!
//! Verified empirically: with `(deny default)` a process cannot reach the network
//! (curl fails to resolve or connect) and cannot create files, while still being able
//! to exec and read permitted paths.

use crate::policy::{Capabilities, ConfinementLevel, SandboxPolicy};
use crate::process::{ConfinedChild, Environment, Streams};
use crate::{Sandbox, SandboxError};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// The program a confined process is reached through where the environment would not
/// survive the journey otherwise.
///
/// `sandbox-exec` is protected by System Integrity Protection, and dyld empties the loader
/// variables out of a protected process before its first instruction: what it removes is
/// gone from the environ handed on to whatever that process execs, so it never reaches the
/// program being confined. Which variables a program is trusted with is the caller's
/// decision and not a platform's, so a variable lost between the two execs is that
/// decision taken away with nothing saying so.
///
/// An argument vector is not an environment and nothing prunes one, so what would be lost
/// travels to the far side of `sandbox-exec` as arguments and `env` assigns it back there.
///
/// **Known cost.** An argument vector is readable by any local user through `ps`, and the
/// environment of another user's process is not, so a variable carried this way is
/// disclosed more widely than one that is inherited. Only the variables named by
/// [`stripped_from_a_protected_process`] are carried, which are loader search paths rather
/// than anything a program authenticates with. Carrying the whole environment would put
/// every credential in it on a command line every user of the machine can read.
const ENV: &str = "/usr/bin/env";

/// The variables dyld empties out of a process protected by System Integrity Protection:
/// every `DYLD_` variable, and `LD_LIBRARY_PATH`.
///
/// Over-approximating is free and under-approximating is the bug: a variable named here
/// that the platform would have passed on is assigned the value it already had, and one
/// left out that the platform removes is gone.
fn stripped_from_a_protected_process(name: &OsStr) -> bool {
    let name = name.as_bytes();
    name.starts_with(b"DYLD_") || name == b"LD_LIBRARY_PATH"
}

/// Seatbelt-based confinement.
#[derive(Debug, Default)]
pub struct SeatbeltSandbox;

impl SeatbeltSandbox {
    /// Fails if `sandbox-exec` is missing rather than degrading to no confinement.
    pub fn new() -> Result<Self, SandboxError> {
        if !Path::new(SANDBOX_EXEC).exists() {
            return Err(SandboxError::Unavailable {
                platform: "macos",
                detail: format!("{SANDBOX_EXEC} is not present"),
            });
        }
        Ok(Self)
    }

    /// Build the Seatbelt profile for a policy.
    ///
    /// Starts from `(deny default)` and adds only what the policy grants. Paths are
    /// written as subpath rules so a granted directory covers its contents.
    pub fn profile(policy: &SandboxPolicy) -> String {
        let mut out = String::from("(version 1)\n(deny default)\n");

        // Without these the process cannot start at all: the loader must exec the
        // binary and mach lookups are needed for basic runtime services. They grant no
        // filesystem or network reach of their own.
        out.push_str("(allow process-exec)\n");
        out.push_str("(allow sysctl-read)\n");
        out.push_str("(allow mach-lookup)\n");

        // The dynamic loader reads the root directory entry itself, which a subpath
        // grant for e.g. /usr does not cover. Without this the process dies with
        // SIGABRT before main runs, which looks like a mysterious crash rather than a
        // denied read. Reading `/` alone exposes no file contents.
        out.push_str("(allow file-read* (literal \"/\"))\n");

        // The program is sometimes reached through the `ENV` constant above, which the
        // process reads to exec it, and the profile is built from the policy alone and so
        // cannot tell which times those are. One file, world-readable, and one a policy
        // naming any of /usr grants already.
        out.push_str(&format!("(allow file-read* (literal {}))\n", quote(ENV)));

        for path in &policy.readable {
            out.push_str(&format!(
                "(allow file-read* (subpath {}))\n",
                quote(&path.to_string_lossy())
            ));
        }

        for row in &policy.writable {
            out.push_str(&format!(
                "(allow file-write* (subpath {}))\n",
                quote(&row.path.to_string_lossy())
            ));
        }

        // Looking at a path answers what it is and not what it holds: this allows stat and
        // readlink, and neither a file's contents nor a directory's entries. Without it every path
        // outside the grants answers "refused" where it would answer "not there", and a search of
        // PATH stops at the first such entry rather than going on to the next, as node's does when
        // it starts anything; and node resolves its own script through each directory above it.
        // Landlock restricts no look at all, so this is the reach Linux already gives.
        out.push_str("(allow file-read-metadata)\n");

        if policy.allow_network {
            out.push_str("(allow network-outbound)\n");
        }

        if policy.allow_subprocesses {
            out.push_str("(allow process-fork)\n");
        }

        out
    }
}

/// Quote a path as a Seatbelt string literal.
///
/// Escapes backslashes and quotes so a path containing either cannot terminate the
/// literal early and inject profile syntax.
fn quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// What `sandbox-exec` is given after the profile: the program to confine, its arguments,
/// and the assignments restoring whatever the caller holds that would not otherwise arrive.
///
/// A caller holding none of those variables reaches its program directly, and so does a
/// caller asking to hand over no environment at all: there is nothing to restore, and an
/// assignment written here would be a variable arriving by a route that emptying the
/// environment does not reach.
///
/// A program whose path holds an `=` is refused where it is passed to `env`, because it
/// would not be run. `env` reads its arguments as assignments up to the first that is not
/// one, so such a path is read as a variable, the program it names is never exec'd, and
/// `env` prints its environment and exits reporting success.
fn confined_argv(
    program: &str,
    args: &[String],
    environment: &Environment,
    held: &[(OsString, OsString)],
) -> Result<Vec<OsString>, SandboxError> {
    let mut argv = Vec::new();

    // Matched rather than compared, so a fourth answer added to `Environment` is a compile
    // error here instead of a program handed a variable by an argument that nothing in the
    // new answer knows to withhold.
    let handed: Vec<&(OsString, OsString)> = match environment {
        Environment::Inherited => held.iter().collect(),
        Environment::Empty => Vec::new(),
        Environment::Only(variables) => variables.iter().collect(),
    };
    let restore: Vec<&(OsString, OsString)> = handed
        .into_iter()
        .filter(|(name, _)| stripped_from_a_protected_process(name))
        .collect();

    if !restore.is_empty() {
        if program.contains('=') {
            return Err(SandboxError::SetupFailed {
                mechanism: "seatbelt",
                detail: format!(
                    "the program path {program} contains '=', which {ENV} reads as a \
                     variable assignment rather than as the program to run"
                ),
            });
        }
        argv.push(OsString::from(ENV));
        argv.extend(restore.iter().map(|(name, value)| {
            let mut assignment = name.clone();
            assignment.push("=");
            assignment.push(value);
            assignment
        }));
    }

    argv.push(OsString::from(program));
    argv.extend(args.iter().map(OsString::from));
    Ok(argv)
}

impl Sandbox for SeatbeltSandbox {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // Seatbelt is enforced by the kernel, though its policy language is
            // coarser than Landlock plus seccomp on Linux.
            level: ConfinementLevel::Kernel,
            mechanisms: vec!["seatbelt"],
            network_denial_enforced: true,
            // A profile is text the kernel reads as the process starts, so a path that is
            // not there yet is named in one and the file can be created afterwards.
            grants_paths_that_do_not_exist: true,
        }
    }

    fn spawn(
        &self,
        program: &str,
        args: &[String],
        policy: &SandboxPolicy,
        streams: Streams,
        environment: Environment,
    ) -> Result<ConfinedChild, SandboxError> {
        if !policy.is_meaningful() {
            return Err(SandboxError::PolicyTooPermissive);
        }

        let mut wrapped = Command::new(SANDBOX_EXEC);
        wrapped.arg("-p").arg(Self::profile(policy));
        wrapped.args(confined_argv(
            program,
            args,
            &environment,
            &std::env::vars_os().collect::<Vec<_>>(),
        )?);
        if let Some(directory) = &policy.starting_in {
            wrapped.current_dir(directory);
        }

        crate::process::start(wrapped, streams, &environment)
    }
}

/// What this backend decides before any process starts, checked wherever the suite runs.
///
/// Seatbelt is compiled on macOS alone, so everything below the next module is checked by
/// one CI job on one platform. What reaches the program, and what is put on a command line
/// to get it there, is worth pinning on every job that runs, which is why this module has
/// no platform of its own (see the declaration of `macos` in `lib.rs`).
#[cfg(test)]
mod argument_tests {
    use super::*;

    fn held(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs
            .iter()
            .map(|(name, value)| (OsString::from(name), OsString::from(value)))
            .collect()
    }

    /// A variable the platform empties out of the wrapper reaches the program anyway, so
    /// the environment a confined process receives is the caller's whole environment and
    /// not the part of it that survives a program the backend introduced. A variable holds
    /// what no grant over paths withholds or hands over, so one the caller holds and the
    /// program does not is the caller's decision made by the platform instead.
    ///
    /// Both loader variables are in the fixture because an implementation restoring the
    /// `DYLD_` prefix alone passes on one of them and loses the other.
    ///
    /// The variables that are *not* restored matter as much: an argument vector is
    /// readable by every user of the machine and another user's environment is not, so a
    /// credential inherited in the ordinary way must not be written onto this command
    /// line to reach a program that would have received it regardless.
    #[test]
    fn a_variable_the_platform_strips_from_the_wrapper_is_carried_to_the_program_as_an_argument() {
        let argv = confined_argv(
            "/opt/tool/server",
            &["--stdio".to_owned()],
            &Environment::Inherited,
            &held(&[
                ("DYLD_LIBRARY_PATH", "/tmp/lib"),
                ("AWS_SECRET_ACCESS_KEY", "a credential"),
                ("LD_LIBRARY_PATH", "/tmp/other"),
            ]),
        )
        .expect("a program path `env` can name");

        assert_eq!(
            argv,
            vec![
                OsString::from(ENV),
                OsString::from("DYLD_LIBRARY_PATH=/tmp/lib"),
                OsString::from("LD_LIBRARY_PATH=/tmp/other"),
                OsString::from("/opt/tool/server"),
                OsString::from("--stdio"),
            ]
        );
    }

    /// A caller holding nothing the platform would strip reaches its program the way it
    /// did before there was anything to restore: the program `sandbox-exec` execs is the
    /// caller's own. Everything the confined process receives arrives by inheritance, so
    /// a second program in the chain would buy nothing and cost a dependency on that
    /// program being readable under the policy.
    #[test]
    fn a_caller_holding_nothing_the_platform_strips_reaches_its_program_directly() {
        let argv = confined_argv(
            "/opt/tool/server",
            &["--stdio".to_owned()],
            &Environment::Inherited,
            &held(&[("PATH", "/usr/bin"), ("HOME", "/Users/someone")]),
        )
        .expect("nothing is carried, so there is nothing to refuse over");

        assert_eq!(
            argv,
            vec![
                OsString::from("/opt/tool/server"),
                OsString::from("--stdio"),
            ]
        );
    }

    /// The other half of the caller's decision, and the one a caller launching third-party
    /// code makes. Emptying the environment of the process the backend starts reaches no
    /// assignment written on its command line, so a caller asking to hand over nothing
    /// hands over nothing only while the argument vector carries no assignment at all.
    #[test]
    fn a_confined_process_asked_to_receive_no_variables_is_handed_none_as_an_argument() {
        let argv = confined_argv(
            "/opt/tool/server",
            &["--stdio".to_owned()],
            &Environment::Empty,
            &held(&[
                ("DYLD_LIBRARY_PATH", "/tmp/lib"),
                ("DYLD_INSERT_LIBRARIES", "/tmp/hook.dylib"),
            ]),
        )
        .expect("nothing is carried, so there is nothing to refuse over");

        assert_eq!(
            argv,
            vec![
                OsString::from("/opt/tool/server"),
                OsString::from("--stdio"),
            ]
        );
    }

    /// A caller naming the variables a program receives names the whole of it, so a loader
    /// variable the platform strips is carried where the caller named it and nowhere else:
    /// one this process holds and the caller did not name stays here, and a named value
    /// that is not a loader variable arrives by the emptied environment rather than on a
    /// command line every user of the machine can read.
    #[test]
    fn a_caller_naming_its_variables_has_only_the_named_loader_variables_carried_as_arguments() {
        let argv = confined_argv(
            "/opt/tool/server",
            &[],
            &Environment::Only(
                crate::process::Variables::new()
                    .with("DYLD_LIBRARY_PATH", "/opt/tool/lib")
                    .with("WEATHER_TOKEN", "a credential"),
            ),
            &held(&[("DYLD_INSERT_LIBRARIES", "/tmp/hook.dylib")]),
        )
        .expect("a program path `env` can name");

        assert_eq!(
            argv,
            vec![
                OsString::from(ENV),
                OsString::from("DYLD_LIBRARY_PATH=/opt/tool/lib"),
                OsString::from("/opt/tool/server"),
            ]
        );
    }

    /// A program `env` would read as a variable assignment is refused rather than started,
    /// because it would not be started: `env` would set a variable named after part of the
    /// path, find no program to run, print its environment and exit reporting success, and
    /// a caller that believes it launched a server would be reading that.
    ///
    /// The refusal belongs to the wrapper rather than to the path, so the same program
    /// with nothing to restore is exec'd directly and runs.
    #[test]
    fn a_program_path_the_wrapper_would_read_as_a_variable_is_refused() {
        let path = "/opt/name=value/server";

        let refused = confined_argv(
            path,
            &[],
            &Environment::Inherited,
            &held(&[("DYLD_LIBRARY_PATH", "/tmp/lib")]),
        )
        .expect_err("a path `env` cannot name is not a process to start");
        assert!(
            matches!(
                refused,
                SandboxError::SetupFailed {
                    mechanism: "seatbelt",
                    ..
                }
            ),
            "the refusal does not say confinement could not be applied: {refused}"
        );

        assert_eq!(
            confined_argv(
                path,
                &[],
                &Environment::Inherited,
                &held(&[("PATH", "/usr/bin")])
            )
            .expect("no wrapper reads this path, so nothing misreads it"),
            vec![OsString::from(path)]
        );
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::testutil::{
        capturing_stdout, nothing_attached, printed_by, variable_names_received_by,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::os::unix::fs::MetadataExt;
    use std::path::PathBuf;

    /// `CURLE_COULDNT_CONNECT`: curl reached the connection and was refused it. Any other code
    /// means it stopped before that, which is some other denial reported as this one.
    const CURL_COULDNT_CONNECT: i32 = 7;

    /// touch reporting that the operation it was asked for failed.
    const TOUCH_FAILED: i32 = 1;

    /// `env` as a program the platform does not protect: it prints the environment it
    /// received, one variable to a line, and is built in `dir` rather than found on the
    /// machine.
    ///
    /// A test about which variables reach a confined process cannot read them back through
    /// `/usr/bin/env`, or any other program macOS ships. dyld empties the loader variables
    /// out of every program the platform protects, whatever route they took to arrive, so a
    /// protected program asked what it received answers about the platform's treatment of
    /// itself: `DYLD_LIBRARY_PATH=/tmp /usr/bin/env` prints no such variable with no sandbox
    /// anywhere near it. A protected program reports a restored variable as missing and a
    /// leaked one as withheld, so both the carrying and the emptying are read back through a
    /// program that reports what it was handed.
    ///
    /// `cc` is what linked the binary running this test, so a machine that built the suite
    /// has it.
    fn unprotected_env(dir: &Path) -> PathBuf {
        const SOURCE: &str = r#"#include <stdio.h>
extern char **environ;
int main(void) {
    for (char **held = environ; *held; held++) {
        puts(*held);
    }
    return 0;
}
"#;
        let source = dir.join("env.c");
        std::fs::write(&source, SOURCE).expect("the scratch directory is writable");
        let program = dir.join("env");
        let built = Command::new("cc")
            .arg("-o")
            .arg(&program)
            .arg(&source)
            .status()
            .expect("cc, which linked this test binary, is on the machine");
        assert!(
            built.success(),
            "the program reporting its environment did not compile"
        );
        program
    }

    /// Answer one request, so a curl that was permitted a socket gets a reply and exits rather
    /// than waiting out its own timeout. Called only where a connection is expected to arrive.
    fn answer_one(listener: &TcpListener) {
        let (mut stream, _) = listener.accept().expect("the connection arrives");
        let _ = stream.read(&mut [0u8; 1024]);
        let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
    }

    #[test]
    fn a_strict_profile_denies_by_default() {
        let profile = SeatbeltSandbox::profile(&SandboxPolicy::strict());
        assert!(profile.contains("(deny default)"));
        assert!(!profile.contains("network-outbound"));
        assert!(!profile.contains("file-write*"));
        assert!(!profile.contains("process-fork"));
    }

    #[test]
    fn granted_paths_appear_as_subpath_rules() {
        let policy = SandboxPolicy::strict()
            .allow_read("/workspace")
            .allow_write("/workspace/target");
        let profile = SeatbeltSandbox::profile(&policy);
        assert!(profile.contains(r#"(allow file-read* (subpath "/workspace"))"#));
        assert!(profile.contains(r#"(allow file-write* (subpath "/workspace/target"))"#));
    }

    /// A grant here is a name in a profile rather than a right on an open descriptor, so a
    /// path that does not exist yet is one this backend can grant, and what a policy names
    /// is what the profile carries. That is the half of the shared rule this backend
    /// answers: the other cannot name such a path at all and refuses the policy rather
    /// than granting less than it asked for, so a caller that meets a refusal there knows
    /// it is the platform and not the policy.
    ///
    /// The process is started as well as the profile built, so validation added here later
    /// has to be decided rather than inherited from the other backend.
    #[test]
    fn a_path_that_is_not_there_yet_is_granted_as_named() {
        let absent = "/bravebot-no-such-path/known_hosts";
        assert!(
            !Path::new(absent).exists(),
            "the path has to be absent for this to say anything"
        );

        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_write(absent);
        let profile = SeatbeltSandbox::profile(&policy);
        assert!(
            profile.contains(&format!(r#"(allow file-write* (subpath "{absent}"))"#)),
            "the grant the policy named is not in the profile: {profile}"
        );
        let mut confined = SeatbeltSandbox
            .spawn(
                "/usr/bin/true",
                &[],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("a path that is not there yet is a grant, not a refusal");
        assert!(confined.wait().expect("should wait").success());
    }

    /// A caller reads this to decide whether an absent path in a policy needs creating
    /// first or the directory holding it named instead, and a report disagreeing with the
    /// backend costs it one of those on a platform where neither was necessary. Both
    /// directions of disagreement cost that, which is why the assertion ties the report to
    /// what the backend does rather than pinning a value.
    ///
    /// What the kernel installs rather than what the profile text holds: a rule the profile
    /// compiler refuses is a grant this backend cannot make however the policy named it,
    /// and a capability claiming otherwise is the overstatement SANDBOX-5 exists to stop.
    #[test]
    fn a_path_that_does_not_exist_is_granted_exactly_where_the_capability_says_so() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");

        let dir = crate::testutil::scratch_dir("bravebot-sandbox-absent-path-capability");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let absent = dir.join("not-created-yet");
        assert!(
            !absent.exists(),
            "the path has to be absent for this to say anything"
        );

        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_write(&absent);
        let started = sandbox.spawn(
            "/usr/bin/true",
            &[],
            &policy,
            nothing_attached(),
            Environment::Inherited,
        );
        // Whether the backend installed the grant, which is what the capability reports.
        // How the process then exited is a separate question, asked separately below, so
        // that a program failing for its own reasons cannot read as a refused policy.
        let granted = started.is_ok();
        if let Ok(mut confined) = started {
            assert!(
                confined.wait().expect("should wait").success(),
                "the confined process failed, so it says nothing about the grant"
            );
        }

        assert_eq!(
            granted,
            sandbox.capabilities().grants_paths_that_do_not_exist,
            "what this backend reports about a path that does not exist is not what it does"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn network_is_only_allowed_when_requested() {
        let denied = SeatbeltSandbox::profile(&SandboxPolicy::strict());
        assert!(!denied.contains("network-outbound"));

        let allowed = SeatbeltSandbox::profile(&SandboxPolicy::strict().allow_network_egress());
        assert!(allowed.contains("(allow network-outbound)"));
    }

    /// A path containing a quote must not close the string literal and let its
    /// remainder be parsed as profile directives. The payload text still appears, but it
    /// is part of the path, but every quote in it is escaped, so Seatbelt reads the
    /// whole thing as one string.
    #[test]
    fn paths_cannot_inject_profile_syntax() {
        let policy = SandboxPolicy::strict().allow_read(r#"/tmp/x") (allow network-outbound) ("#);
        let profile = SeatbeltSandbox::profile(&policy);

        assert!(
            profile.contains(r#"\""#),
            "the embedded quote was not escaped: {profile}"
        );
        // Unescaped, this would have been a directive of its own.
        assert!(
            !profile.contains("\n(allow network-outbound)"),
            "injected directive escaped the literal: {profile}"
        );
    }

    /// Seatbelt must accept a profile built from a hostile path rather than failing to
    /// parse, since a parse failure would be reported as confinement being unavailable.
    #[test]
    fn a_profile_containing_a_hostile_path_still_applies() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(r#"/tmp/x") (allow network-outbound) ("#);

        let mut child = sandbox
            .spawn(
                "/usr/bin/true",
                &[],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn");
        assert!(child.wait().expect("should wait").success());
    }

    #[test]
    fn a_fully_permissive_policy_is_refused() {
        let sandbox = SeatbeltSandbox;
        let policy = SandboxPolicy::strict()
            .allow_network_egress()
            .allow_subprocesses()
            .allow_write("/");
        let err = sandbox
            .spawn(
                "/usr/bin/true",
                &[],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect_err("must refuse a policy that confines nothing");
        assert!(matches!(err, SandboxError::PolicyTooPermissive));
    }

    #[test]
    fn capabilities_report_kernel_enforcement() {
        let caps = SeatbeltSandbox.capabilities();
        assert_eq!(caps.level, ConfinementLevel::Kernel);
        assert!(caps.network_denial_enforced);
        assert!(caps.mechanisms.contains(&"seatbelt"));
    }

    /// Node walks each directory above its script before it loads it, and a search of `PATH` stops
    /// at an entry it is refused rather than told is missing, which a link on the way to it is: how
    /// npx died here twice before it started a server. A look reaches past the grants, and reading
    /// a file or listing a directory does not.
    #[test]
    fn a_confined_process_can_look_at_any_path_and_read_or_list_only_its_grants() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let scratch = crate::testutil::scratch_dir("bravebot-sandbox-looking-past-a-grant");
        let _ = std::fs::remove_dir_all(&scratch);
        let granted = scratch.join("installation").join("bin");
        let beside = scratch.join("beside");
        std::fs::create_dir_all(&granted).expect("the scratch directory is creatable");
        std::fs::create_dir_all(&beside).expect("the scratch directory is creatable");
        std::fs::write(beside.join("secret"), "a token").expect("the scratch file is writable");
        let above = scratch
            .canonicalize()
            .expect("the scratch directory is there");
        let granted = granted.canonicalize().expect("the grant is there");
        let beside = beside
            .canonicalize()
            .expect("the scratch directory is there");
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(&granted);
        let succeeds = |program: &str, arguments: &[String]| {
            sandbox
                .spawn(
                    program,
                    arguments,
                    &policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect("should spawn")
                .wait()
                .expect("should wait")
                .success()
        };
        let shown = |path: &Path| path.display().to_string();

        for directory in granted.ancestors().skip(1) {
            assert!(
                succeeds("/usr/bin/stat", &[shown(directory)]),
                "{} is above the grant and could not be looked at",
                directory.display()
            );
        }
        // As `/System/Cryptexes/App/usr/bin` is on a PATH here: a link outside the grants whose
        // target is inside them.
        let tool = granted.join("tool");
        std::fs::write(&tool, "").expect("the scratch file is writable");
        let linked = above.join("linked");
        std::os::unix::fs::symlink(granted.parent().expect("a parent"), &linked)
            .expect("the scratch link is creatable");
        assert!(
            succeeds("/usr/bin/stat", &[shown(&linked.join("bin").join("tool"))]),
            "a granted file reached through a link outside the grants was refused"
        );
        assert!(
            succeeds("/bin/ls", &[shown(&granted)]),
            "the grant is listed"
        );
        assert!(
            !succeeds("/bin/ls", &[shown(&above)]),
            "a directory above the grant was listed"
        );
        assert!(
            !succeeds("/bin/cat", &[shown(&beside.join("secret"))]),
            "a file beside the grant was read"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// Confirms the sandbox actually runs a process, not just that a profile string
    /// was built.
    #[test]
    fn a_confined_process_runs() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin");
        let mut child = sandbox
            .spawn(
                "/usr/bin/true",
                &[],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn");
        assert!(child.wait().expect("should wait").success());
    }

    /// The point of the sandbox: a confined process cannot write outside its grants.
    #[test]
    fn a_confined_process_cannot_write_outside_its_grants() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin");

        // The directory has to be there before touch runs, and empty. Into a parent that does
        // not exist, touch fails with ENOENT and creates nothing whatever the profile permits,
        // which holds just as well against a sandbox granting every write; and a file left by a
        // run that failed would fail every run after it.
        let dir = crate::testutil::scratch_dir("bravebot-sandbox-denied-write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let target = dir.join("must-not-exist");

        let mut child = sandbox
            .spawn(
                "/usr/bin/touch",
                &[target.display().to_string()],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn");
        let status = child.wait().expect("should wait");

        // touch's own refusal, rather than any failure at all: sandbox-exec declining to exec it
        // exits 71 and a process dying before main exits by signal, and neither of those says
        // anything about a write.
        assert_eq!(
            status.code(),
            Some(TOUCH_FAILED),
            "the write was not what failed"
        );
        assert!(!target.exists(), "file was created despite confinement");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Network denial is the property that makes exfiltration structurally impossible,
    /// so it is asserted against a real process rather than only in the profile text.
    ///
    /// Both halves run against a socket this test is listening on. The permitted half is what
    /// makes the denied half mean anything: curl exits 7 against a port nothing is listening on
    /// just as readily as against a socket it was refused, so without establishing that a
    /// connection succeeds here, the denied half would pass against a sandbox that enforces
    /// nothing.
    #[test]
    fn a_confined_process_cannot_reach_the_network() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port to connect to");
        let port = listener.local_addr().expect("the bound address").port();

        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let denied = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            // Resolved rather than /etc, which is a symlink to it. Seatbelt matches the resolved
            // path, so a grant for /etc reaches nothing, and curl exits over its unreadable
            // LibreSSL configuration before it opens a socket at all.
            .allow_read("/private/etc")
            .allow_read("/System")
            .allow_read("/Library");
        let permitted = denied.clone().allow_network_egress();

        // An address, so no resolver is involved, and stdout is discarded, so curl needs no file
        // to write the body to. The proxy is refused in the argument vector because the
        // environment here is this process's own: a machine whose shell exports `http_proxy`
        // would otherwise have both halves of this test measure a connection to somewhere else.
        let curl = |policy: &SandboxPolicy| {
            let args: Vec<String> = [
                "-s",
                "--noproxy",
                "*",
                "-m",
                "5",
                &format!("http://127.0.0.1:{port}/"),
            ]
            .iter()
            .map(|s| s.to_string())
            .collect();
            sandbox
                .spawn(
                    "/usr/bin/curl",
                    &args,
                    policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect("should spawn")
        };

        let mut reaching = curl(&permitted);
        answer_one(&listener);
        assert_eq!(
            reaching.wait().expect("should wait").code(),
            Some(0),
            "a permitted process could not reach the listener, so nothing below means anything"
        );

        let mut refused = curl(&denied);
        assert_eq!(
            refused.wait().expect("should wait").code(),
            Some(CURL_COULDNT_CONNECT),
            "the connection was not what failed, so this says nothing about network denial"
        );
    }

    /// Writing a temporary file and renaming it into place is how a compiler, a package
    /// manager and an editor write anything at all, so a confinement that denies the move
    /// holds an ordinary build to less than the paths its policy granted. A write grant
    /// here is a subpath rule covering every operation on what is under it, so this
    /// backend needs nothing beyond the grant to permit the move; the other one needs the
    /// kernel right that governs it, and the two are held to one rule.
    ///
    /// The inode is what the assertion is on, because `mv` answers a refused rename by
    /// copying the file and unlinking the original: the destination exists either way, and
    /// only a preserved inode says the move happened rather than a copy that is neither
    /// atomic nor cheap.
    #[test]
    fn a_confined_process_can_rename_a_file_between_two_granted_directories() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");

        let dir = crate::testutil::scratch_dir("bravebot-sandbox-rename");
        let _ = std::fs::remove_dir_all(&dir);
        let source = dir.join("from").join("moved");
        let destination = dir.join("to").join("moved");
        std::fs::create_dir_all(dir.join("from")).expect("the scratch directory is creatable");
        std::fs::create_dir_all(dir.join("to")).expect("the scratch directory is creatable");
        std::fs::write(&source, b"contents").expect("the file is writable");
        let inode = std::fs::metadata(&source).expect("the file is there").ino();

        // Read as well as write: mv stats both ends before it moves anything, so a profile
        // granting only the write fails over the stat and says nothing about the move.
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(&dir)
            .allow_write(&dir);
        let mut child = sandbox
            .spawn(
                "/bin/mv",
                &[
                    source.display().to_string(),
                    destination.display().to_string(),
                ],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn");
        assert!(
            child.wait().expect("should wait").success(),
            "a move inside one granted directory failed"
        );
        assert_eq!(
            std::fs::metadata(&destination)
                .expect("the destination is there")
                .ino(),
            inode,
            "the file was copied and unlinked rather than moved, so the move was denied"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a program may be trusted with in the environment is the caller's decision and
    /// not a backend's: a credential lives in a variable rather than in a file, so no
    /// grant over paths either withholds one or hands one over, and the agent socket a
    /// push signs through is named by a variable as well. A caller that asks for its own
    /// environment receives exactly that, on either platform, so the decision reads the
    /// same wherever it is made.
    ///
    /// Every variable rather than one of them: what the backend puts between this process
    /// and the program decides which variables survive the journey, so a test reading back
    /// a variable chosen for being ordinary passes against a backend that drops the ones
    /// that are not. Cargo hands a test `DYLD_FALLBACK_LIBRARY_PATH`, so the variables this
    /// process holds include one of the class the platform empties out of the wrapper, and
    /// this reads it back rather than standing in for it. A failure names what was withheld
    /// and not what it held.
    #[test]
    fn the_environment_a_confined_process_receives_is_the_callers() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        assert!(
            std::env::var_os("CARGO_MANIFEST_DIR").is_some(),
            "cargo sets this for a test, and without it there is nothing here to carry"
        );
        let dir = crate::testutil::scratch_dir("bravebot-sandbox-environment-carried");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let program = unprotected_env(&dir);
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(&dir);

        let mut child = sandbox
            .spawn(
                &program.to_string_lossy(),
                &[],
                &policy,
                capturing_stdout(),
                Environment::Inherited,
            )
            .expect("the confined process runs");

        let received = variable_names_received_by(&mut child);
        let withheld: Vec<String> = std::env::vars_os()
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .filter(|name| !received.contains(name))
            .collect();
        assert!(
            withheld.is_empty(),
            "variables this process holds did not reach the confined process: {withheld:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A server declared with a directory runs there rather than wherever this process was
    /// started, since a relative path it opens is meant to be one inside it.
    #[test]
    fn a_confined_process_starts_in_the_directory_its_policy_names() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let dir = crate::testutil::scratch_dir("bravebot-sandbox-starting-in");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let dir = dir.canonicalize().expect("the scratch directory resolves");
        // Readable too, so a process started where this one was prints that rather than failing.
        let here = std::env::current_dir()
            .and_then(|here| here.canonicalize())
            .expect("this process has a directory");
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(&dir)
            .allow_read(&here)
            .starting_in(&dir);

        let mut child = sandbox
            .spawn(
                "/bin/pwd",
                &["-P".to_owned()],
                &policy,
                capturing_stdout(),
                Environment::Empty,
            )
            .expect("the confined process runs");

        assert_eq!(printed_by(&mut child).trim_end(), dir.to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The reason the program is reached through `env` at all, on the platform that makes
    /// it one. `sandbox-exec` is protected by System Integrity Protection, so a loader
    /// variable named in its own environment is emptied out of it before the program it
    /// execs is reached, and the caller holding that variable is the only one who knows it
    /// is gone. Here it arrives.
    ///
    /// The value as well as the name, which is the whole of what a loader search path is:
    /// a variable arriving emptied is a program told to look nowhere.
    ///
    /// Whether the plain route loses the variable is the machine's answer rather than this
    /// code's, so it is not asserted here: the loss happens where the platform protects
    /// `sandbox-exec`, and a machine with System Integrity Protection disabled, as the
    /// hosted macOS runners are, hands the variable over untouched. Asserting the loss
    /// reports a bug in this backend on a machine that is protecting nothing. What the
    /// backend itself decided, the assignment written onto the command line and the
    /// variables kept off it, is pinned on every platform by
    /// `a_variable_the_platform_strips_from_the_wrapper_is_carried_to_the_program_as_an_argument`
    /// above.
    #[test]
    fn a_variable_stripped_from_the_wrapper_still_reaches_the_confined_process() {
        let dir = crate::testutil::scratch_dir("bravebot-sandbox-loader-variable");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let program = unprotected_env(&dir);

        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(&dir);
        let stripped = [(OsString::from("DYLD_LIBRARY_PATH"), OsString::from("/tmp"))];

        // Carried the way the backend carries it: as arguments, which nothing prunes.
        let mut as_arguments = Command::new(SANDBOX_EXEC);
        as_arguments
            .arg("-p")
            .arg(SeatbeltSandbox::profile(&policy));
        as_arguments.args(
            confined_argv(
                &program.to_string_lossy(),
                &[],
                &Environment::Inherited,
                &stripped,
            )
            .expect("a program path `env` can name"),
        );
        let mut restored =
            crate::process::start(as_arguments, capturing_stdout(), &Environment::Inherited)
                .expect("the confined process runs");

        // Named without its value, so a failure says what arrived and not what a machine
        // running this holds.
        let carried = printed_by(&mut restored);
        assert!(
            carried.lines().any(|line| line == "DYLD_LIBRARY_PATH=/tmp"),
            "the variable the caller holds did not reach the confined process, which \
             received: {:?}",
            carried
                .lines()
                .map(|line| line.split_once('=').map_or(line, |(name, _)| name))
                .collect::<Vec<_>>()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other half of that decision, and the one a caller launching third-party code
    /// makes. Emptying it is this crate's to do rather than each caller's, so a program
    /// meant to hold none of this process's credentials holds none of them under either
    /// backend.
    ///
    /// Empty means empty rather than short of one named variable, which is the clause as
    /// written and the only form of it worth having: a process handed `PATH`, `HOME` or the
    /// socket a signature is made through has been handed a credential whatever else was
    /// withheld. A failure prints the names that arrived and not their values, so it says
    /// what leaked without publishing what a machine running this holds.
    ///
    /// Read back through the same unprotected program as the test above, because the leak
    /// this guards against is a loader variable written onto the command line for a caller
    /// who asked for no environment, and a program the platform protects would report one
    /// that did arrive as absent.
    #[test]
    fn a_confined_process_given_an_empty_environment_receives_none_of_this_processes_variables() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        assert!(
            std::env::var_os("CARGO_MANIFEST_DIR").is_some(),
            "cargo sets this for a test, and without it there is nothing here to withhold"
        );
        let dir = crate::testutil::scratch_dir("bravebot-sandbox-environment-emptied");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        let program = unprotected_env(&dir);
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(&dir);

        let mut child = sandbox
            .spawn(
                &program.to_string_lossy(),
                &[],
                &policy,
                capturing_stdout(),
                Environment::Empty,
            )
            .expect("the confined process runs");

        let received = variable_names_received_by(&mut child);
        assert!(
            received.is_empty(),
            "a process asked to receive no variables received some: {received:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
