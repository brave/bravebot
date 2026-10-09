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

/// The socket every host name lookup on macOS goes through.
const RESOLVER: &str = "/private/var/run/mDNSResponder";

/// Refuses a write to any path with a `.git` component, in either case, since the default
/// volume opens `.GIT` when git asks for `.git`.
const GIT_DIRECTORY_WRITE: &str = "(deny file-write* (regex #\"/\\.[Gg][Ii][Tt](/|$)\"))\n";

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
        // denied read. Seatbelt has no operation that separates opening `/` from listing
        // it, so this row also lets a process list the entries of `/`. That is the one
        // directory listing SANDBOX-13 allows outside the grants; no file contents are
        // readable through it.
        out.push_str("(allow file-read* (literal \"/\"))\n");

        // `ENV` is exec'd under `process-exec` above and is not opened for reading, so no
        // read row names it: its contents are readable only where a policy grants /usr/bin.

        // The last matching rule wins, so the rows are written from the widest path to the
        // narrowest: a refusal follows the grants it narrows, a row beneath a refusal lifts it, and
        // a refusal beneath that row holds again. At one path a read row follows the refusal, which
        // is how a credential scope lifts a location the base refuses.
        let mut reads: Vec<(&Path, bool)> = policy
            .readable
            .iter()
            .map(|path| (path.as_path(), true))
            .chain(policy.unreadable.iter().map(|path| (path.as_path(), false)))
            .collect();
        reads.sort_by_key(|(path, allows)| (depth(path), *allows));
        for (path, allows) in reads {
            out.push_str(&format!(
                "({} file-read* (subpath {}))\n",
                if allows { "allow" } else { "deny" },
                quote(&path.to_string_lossy())
            ));
        }

        // Written the same way, except that a row at a refused path is withdrawn and not lifted:
        // the refusal of a write is a person's, and it wins at the path it names.
        let mut writes: Vec<(&Path, bool)> = policy
            .writable
            .iter()
            .filter(|row| !policy.unwritable.contains(&row.path))
            .map(|row| (row.path.as_path(), true))
            .chain(policy.unwritable.iter().map(|path| (path.as_path(), false)))
            .collect();
        writes.sort_by_key(|(path, allows)| (depth(path), *allows));
        for (path, allows) in writes {
            out.push_str(&format!(
                "({} file-write* (subpath {}))\n",
                if allows { "allow" } else { "deny" },
                quote(&path.to_string_lossy())
            ));
        }

        // A repository's configuration and hooks are commands git runs, unconfined, the next time
        // anybody runs git there. Seatbelt lets the last matching rule decide, so this follows the
        // rows it narrows.
        if !policy.writable.is_empty() && !policy.git_directories_writable {
            out.push_str(GIT_DIRECTORY_WRITE);
        }

        // Looking at a path answers what it is and not what it holds: this allows stat and
        // readlink, and neither a file's contents nor a directory's entries. Without it every path
        // outside the grants answers "refused" where it would answer "not there", and a search of
        // PATH stops at the first such entry rather than going on to the next, as node's does when
        // it starts anything; and node resolves its own script through each directory above it.
        // Landlock restricts no look at all, so this is the reach Linux already gives.
        out.push_str("(allow file-read-metadata)\n");

        // Seatbelt counts a connect to a unix socket as network-outbound and no file rule covers
        // it, so an unfiltered rule reaches every socket the account can, a Docker daemon's
        // included. Egress is IP, and a socket is reached where a write row reaches it, as
        // Landlock's ResolveUnix is granted. Host names resolve through the platform resolver's
        // socket, where Linux resolves over IP.
        if policy.allow_network {
            out.push_str("(allow network-outbound (remote ip))\n");
            out.push_str(&format!(
                "(allow network-outbound (literal {}))\n",
                quote(RESOLVER)
            ));
            for row in &policy.writable {
                out.push_str(&format!(
                    "(allow network-outbound (subpath {}))\n",
                    quote(&row.path.to_string_lossy())
                ));
            }
        }

        if policy.allow_subprocesses {
            out.push_str("(allow process-fork)\n");
        }

        out
    }
}

/// The per-user cache directory the platform names for this account, with its links followed,
/// which is how Seatbelt matches it. `None` where the platform does not name one.
// The exemption sits on the function because the function is the syscall.
#[allow(unsafe_code)]
#[cfg(target_os = "macos")]
pub fn user_cache_directory() -> Option<std::path::PathBuf> {
    let mut buffer = [0u8; libc::PATH_MAX as usize];
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let length = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_CACHE_DIR,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
        )
    };
    if length == 0 || length > buffer.len() {
        return None;
    }
    let text = std::ffi::CStr::from_bytes_until_nul(&buffer).ok()?;
    Path::new(OsStr::from_bytes(text.to_bytes()))
        .canonicalize()
        .ok()
}

/// How many names a path has, which is how far down the filesystem it is.
fn depth(path: &Path) -> usize {
    path.components()
        .filter(|part| matches!(part, std::path::Component::Normal(_)))
        .count()
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
    program: &OsStr,
    args: &[OsString],
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
        if program.as_bytes().contains(&b'=') {
            return Err(SandboxError::SetupFailed {
                mechanism: "seatbelt",
                detail: format!(
                    "the program path {} contains '=', which {ENV} reads as a \
                     variable assignment rather than as the program to run",
                    program.to_string_lossy()
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

    argv.push(program.to_os_string());
    argv.extend(args.iter().cloned());
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
        program: &OsStr,
        args: &[OsString],
        policy: &SandboxPolicy,
        streams: Streams,
        environment: Environment,
    ) -> Result<ConfinedChild, SandboxError> {
        let wrapped = Self::wrapping(program, args, policy, &environment)?;
        crate::process::start(wrapped, streams, &environment)
    }

    fn command(
        &self,
        program: &OsStr,
        args: &[OsString],
        policy: &SandboxPolicy,
        environment: &Environment,
    ) -> Result<Command, SandboxError> {
        let mut wrapped = Self::wrapping(program, args, policy, environment)?;
        crate::process::apply_environment(&mut wrapped, environment);
        Ok(wrapped)
    }
}

impl SeatbeltSandbox {
    /// `program` behind `sandbox-exec` with the profile for `policy`, or a refusal of the policy.
    fn wrapping(
        program: &OsStr,
        args: &[OsString],
        policy: &SandboxPolicy,
        environment: &Environment,
    ) -> Result<Command, SandboxError> {
        if !policy.is_meaningful() {
            return Err(SandboxError::PolicyTooPermissive);
        }

        if let Some(path) = policy
            .readable
            .iter()
            .chain(&policy.unreadable)
            .chain(policy.writable.iter().map(|row| &row.path))
            .chain(&policy.unwritable)
            .find(|path| path.to_str().is_none())
        {
            return Err(SandboxError::SetupFailed {
                mechanism: "seatbelt",
                detail: format!(
                    "the policy names the path {}, which is not text and cannot be written \
                     into a profile without naming a different file; refusing rather than \
                     granting that one",
                    path.display()
                ),
            });
        }

        let mut wrapped = Command::new(SANDBOX_EXEC);
        wrapped.arg("-p").arg(Self::profile(policy));
        wrapped.args(confined_argv(
            program,
            args,
            environment,
            &std::env::vars_os().collect::<Vec<_>>(),
        )?);
        if let Some(directory) = &policy.starting_in {
            wrapped.current_dir(directory);
        }

        Ok(wrapped)
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
    use crate::policy::SandboxPolicy;

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
            OsStr::new("/opt/tool/server"),
            &["--stdio".into()],
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

    /// RUN-8: a program or argument that is not text is handed to `sandbox-exec` as the bytes it
    /// holds, with and without the assignments restoring the loader variables. The regression it
    /// rejects is a conversion to text on the way, which hands over the file whose name spells the
    /// replacement of the bytes.
    #[cfg(unix)]
    #[test]
    fn a_program_and_an_argument_that_are_not_text_reach_the_program_as_their_bytes() {
        let program = OsStr::from_bytes(b"/opt/dir-\xff/server");
        let argument = OsString::from(OsStr::from_bytes(b"dir-\xff"));

        for (environment, held_variables) in [
            (Environment::Inherited, held(&[("PATH", "/usr/bin")])),
            (
                Environment::Inherited,
                held(&[("DYLD_LIBRARY_PATH", "/tmp/lib")]),
            ),
        ] {
            let argv = confined_argv(
                program,
                std::slice::from_ref(&argument),
                &environment,
                &held_variables,
            )
            .expect("a program path `env` can name");

            assert_eq!(&argv[argv.len() - 2..], [program, argument.as_os_str()]);
        }
    }

    /// RUN-8: a profile is text, so a path in the policy that is not text cannot be written into
    /// one without naming the file whose name spells the replacement. It is refused instead of
    /// granted: the regression it rejects is the lossy rendering written as the rule.
    #[cfg(unix)]
    #[test]
    fn a_policy_path_that_is_not_text_is_refused_rather_than_granted_as_another() {
        let not_text = std::path::PathBuf::from(OsStr::from_bytes(b"/usr/dir-\xff"));
        let granted_to: [fn(SandboxPolicy, &Path) -> SandboxPolicy; 4] = [
            |policy, path| policy.allow_read(path),
            |policy, path| policy.deny_read(path),
            |policy, path| policy.allow_write(path),
            |policy, path| policy.deny_write(path),
        ];

        for grant in granted_to {
            let policy = grant(
                SandboxPolicy::strict()
                    .allow_read("/usr")
                    .allow_write("/usr"),
                &not_text,
            );

            let refused = SeatbeltSandbox::wrapping(
                OsStr::new("/usr/bin/true"),
                &[],
                &policy,
                &Environment::Inherited,
            );

            assert!(
                matches!(refused, Err(SandboxError::SetupFailed { .. })),
                "{refused:?}"
            );
        }
    }

    /// A caller holding nothing the platform would strip reaches its program the way it
    /// did before there was anything to restore: the program `sandbox-exec` execs is the
    /// caller's own. Everything the confined process receives arrives by inheritance, so
    /// a second program in the chain would buy nothing and cost a dependency on that
    /// program being readable under the policy.
    #[test]
    fn a_caller_holding_nothing_the_platform_strips_reaches_its_program_directly() {
        let argv = confined_argv(
            OsStr::new("/opt/tool/server"),
            &["--stdio".into()],
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
            OsStr::new("/opt/tool/server"),
            &["--stdio".into()],
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
            OsStr::new("/opt/tool/server"),
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
            OsStr::new(path),
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
                OsStr::new(path),
                &[],
                &Environment::Inherited,
                &held(&[("PATH", "/usr/bin")])
            )
            .expect("no wrapper reads this path, so nothing misreads it"),
            vec![OsString::from(path)]
        );
    }

    /// A backslash in a path is escaped as well as a quote. Left alone, the backslash would
    /// combine with the escape written for a quote after it, the pair would read as an escaped
    /// backslash, and the quote would end the literal and let the rest of the path be read as
    /// profile directives.
    #[test]
    fn a_backslash_in_a_path_cannot_cancel_the_escape_of_the_quote_after_it() {
        assert_eq!(quote(r#"/tmp/x\"#), r#""/tmp/x\\""#);
        assert_eq!(
            quote(r#"/tmp/x\") (allow network-outbound) (""#),
            r#""/tmp/x\\\") (allow network-outbound) (\"""#
        );

        let policy = SandboxPolicy::strict().allow_read(r#"/tmp/x\") (allow network-outbound) ("#);
        let profile = SeatbeltSandbox::profile(&policy);
        assert!(
            profile.contains(r#"(subpath "/tmp/x\\\") (allow network-outbound) (")"#),
            "the path did not stay one literal: {profile}"
        );
    }

    /// The regression it rejects: refusals written after every grant, which cannot say "the
    /// persons own row beneath this refusal stands, and a refusal beneath that row holds again".
    /// The last matching rule decides, so each row has to come after every wider one.
    #[test]
    fn the_profile_orders_every_row_from_the_widest_path_to_the_narrowest() {
        let policy = SandboxPolicy::strict()
            .allow_read("/")
            .deny_read("/h/p")
            .allow_read("/h/p/public")
            .deny_read("/h/p/public/inner")
            .allow_write("/h")
            .allow_write("/h/p/out")
            .allow_write("/h/gone")
            .deny_write("/h/p")
            .deny_write("/h/gone");
        let profile = SeatbeltSandbox::profile(&policy);
        let at = |rule: &str| {
            profile
                .find(rule)
                .unwrap_or_else(|| panic!("{rule} is not in the profile:\n{profile}"))
        };
        let (grant, refusal, lift, again) = (
            at(r#"(allow file-read* (subpath "/"))"#),
            at(r#"(deny file-read* (subpath "/h/p"))"#),
            at(r#"(allow file-read* (subpath "/h/p/public"))"#),
            at(r#"(deny file-read* (subpath "/h/p/public/inner"))"#),
        );
        assert!(
            grant < refusal && refusal < lift && lift < again,
            "{profile}"
        );
        let (allowed, denied, narrower) = (
            at(r#"(allow file-write* (subpath "/h"))"#),
            at(r#"(deny file-write* (subpath "/h/p"))"#),
            at(r#"(allow file-write* (subpath "/h/p/out"))"#),
        );
        assert!(allowed < denied && denied < narrower, "{profile}");
        assert!(
            !profile.contains(r#"(allow file-write* (subpath "/h/gone"))"#),
            "a write row at a refused path was kept: {profile}"
        );
        assert!(profile.contains(r#"(deny file-write* (subpath "/h/gone"))"#));
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
    use std::os::unix::net::UnixListener;
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

    /// Seatbelt lets the last matching rule decide, so a refusal written before the grant it
    /// narrows is no refusal, and a lift written before the refusal is lifted by nothing. The
    /// regression it rejects is either order, and a refusal written with a hostile path in it
    /// that ends the string it is quoted in.
    #[test]
    fn the_profile_states_a_refusal_after_the_grant_it_narrows_and_a_lift_after_the_refusal() {
        let policy = SandboxPolicy::strict()
            .allow_read("/")
            .deny_read("/home/a-person/.ssh")
            .deny_read("/home/a \"person\"/.aws")
            .allow_read("/home/a-person/.ssh/config")
            .allow_read("/usr");

        let profile = SeatbeltSandbox::profile(&policy);
        let at = |rule: &str| {
            profile
                .find(rule)
                .unwrap_or_else(|| panic!("{rule} is not in the profile:\n{profile}"))
        };
        let grant = at(r#"(allow file-read* (subpath "/"))"#);
        let usr = at(r#"(allow file-read* (subpath "/usr"))"#);
        let refusal = at(r#"(deny file-read* (subpath "/home/a-person/.ssh"))"#);
        let hostile = at(r#"(deny file-read* (subpath "/home/a \"person\"/.aws"))"#);
        let lift = at(r#"(allow file-read* (subpath "/home/a-person/.ssh/config"))"#);

        assert!(grant < refusal && usr < refusal, "{profile}");
        assert!(grant < hostile && usr < hostile, "{profile}");
        assert!(refusal < lift && hostile < lift, "{profile}");
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
                OsStr::new("/usr/bin/true"),
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
            OsStr::new("/usr/bin/true"),
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

    /// A write row reaches a socket only while egress is granted, since Seatbelt counts the
    /// connect as egress. Unfiltered egress would reach every socket on the machine.
    #[test]
    fn network_is_only_allowed_when_requested() {
        let denied = SeatbeltSandbox::profile(&SandboxPolicy::strict().allow_write("/w"));
        assert!(!denied.contains("network-outbound"), "{denied}");

        let allowed = SeatbeltSandbox::profile(&SandboxPolicy::strict().allow_network_egress());
        assert!(
            allowed.contains("(allow network-outbound (remote ip))\n"),
            "{allowed}"
        );
        assert!(
            !allowed.contains("(allow network-outbound)\n"),
            "egress reaches every unix socket: {allowed}"
        );
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
                OsStr::new("/usr/bin/true"),
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
                OsStr::new("/usr/bin/true"),
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
            let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
            sandbox
                .spawn(
                    OsStr::new(program),
                    &arguments,
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
        // The program a loader variable is restored through is a file like any other. A policy
        // that does not grant /usr/bin cannot read its contents.
        let without_usr_bin = SandboxPolicy::strict()
            .allow_read("/bin")
            .allow_read("/usr/lib");
        assert!(
            !SeatbeltSandbox::profile(&without_usr_bin).contains(ENV),
            "the profile grants {ENV} to a policy that does not name it"
        );
        let read_env = sandbox
            .spawn(
                OsStr::new("/bin/cat"),
                &[ENV.into()],
                &without_usr_bin,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn")
            .wait()
            .expect("should wait");
        assert_eq!(
            read_env.code(),
            Some(1),
            "{ENV} was read without a grant naming it, or cat did not start: {read_env:?}"
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
                OsStr::new("/usr/bin/true"),
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
                OsStr::new("/usr/bin/touch"),
                &[target.display().to_string().into()],
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
            let args: Vec<OsString> = [
                "-s",
                "--noproxy",
                "*",
                "-m",
                "5",
                &format!("http://127.0.0.1:{port}/"),
            ]
            .iter()
            .map(OsString::from)
            .collect();
            sandbox
                .spawn(
                    OsStr::new("/usr/bin/curl"),
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

    /// `nc`, confined by `policy`, connecting to the unix socket at `path` and giving up after a
    /// second, and what it exited with.
    fn nc_to_socket(sandbox: &SeatbeltSandbox, policy: &SandboxPolicy, path: &Path) -> Option<i32> {
        let args = ["-U", "-w", "1"]
            .iter()
            .map(OsString::from)
            .chain([path.as_os_str().to_os_string()])
            .collect::<Vec<_>>();
        sandbox
            .spawn(
                OsStr::new("/usr/bin/nc"),
                &args,
                policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn")
            .wait()
            .expect("should wait")
            .code()
    }

    /// A connect to a unix socket is egress to Seatbelt and no file rule covers it, so egress
    /// left unfiltered reaches every socket the account can, and a Docker daemon's mounts the
    /// home into a container on request. A socket is reached where the policy names a path for
    /// writing, as on Linux, and a grant for reading carries no connect.
    ///
    /// The assertions are on whether a connection arrived at the listener. nc exits 1 against a
    /// socket nobody listens on as readily as against one it was refused, so its exit code says
    /// nothing. The socket under the write grant being reached is also what makes the refusals
    /// mean anything.
    #[test]
    fn a_confined_process_granted_egress_cannot_reach_a_socket_outside_its_grants() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        // A socket path must fit in sun_path's 104 bytes, which a checkout's scratch directory
        // need not leave room for. The policy names no part of the temporary directory.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.subsec_nanos())
            .unwrap_or_default();
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let scratch = std::env::temp_dir().join(format!("bb{}-{stamp}", std::process::id()));
        std::fs::create_dir(&scratch).expect("the scratch directory is creatable");
        for dir in ["none", "read", "write"] {
            std::fs::create_dir(scratch.join(dir)).expect("the scratch directory is creatable");
        }
        let scratch = scratch
            .canonicalize()
            .expect("the scratch directory is there");
        let listening = |dir: &str| {
            let path = scratch.join(dir).join("s");
            let listener = UnixListener::bind(&path).expect("a socket to connect to");
            listener
                .set_nonblocking(true)
                .expect("the listener can be polled");
            (path, listener)
        };
        let (outside, outside_listener) = listening("none");
        let (readable, readable_listener) = listening("read");
        let (writable, writable_listener) = listening("write");
        // Seatbelt matches the path a connect resolves to, as Landlock does, so a link under the
        // write grant reaches no further than its target.
        let linked = scratch.join("write").join("l");
        std::os::unix::fs::symlink(&outside, &linked).expect("the scratch link is creatable");

        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(scratch.join("read"))
            .allow_write(scratch.join("write"))
            .allow_network_egress();
        let arrived = |listener: &UnixListener| match listener.accept() {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => false,
            Err(error) => panic!("the listener failed: {error}"),
        };

        nc_to_socket(&sandbox, &policy, &writable);
        assert!(
            arrived(&writable_listener),
            "a socket under a write grant was refused"
        );
        nc_to_socket(&sandbox, &policy, &outside);
        assert!(
            !arrived(&outside_listener),
            "a socket outside every grant was reached"
        );
        nc_to_socket(&sandbox, &policy, &linked);
        assert!(
            !arrived(&outside_listener),
            "a socket outside every grant was reached through a link under a write grant"
        );
        nc_to_socket(&sandbox, &policy, &readable);
        assert!(
            !arrived(&readable_listener),
            "a socket under a read grant was reached"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A host name lookup on macOS is a message to the resolver's unix socket, so egress to IP
    /// addresses alone reaches no host by name: curl stops at "could not resolve" before it
    /// opens a connection. A lookup needs the network, so reaching the socket is what is
    /// asserted.
    #[test]
    fn a_confined_process_granted_egress_can_reach_the_resolver() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_network_egress();
        assert_eq!(
            nc_to_socket(&sandbox, &policy, Path::new(RESOLVER)),
            Some(0),
            "the resolver's socket was refused, so no host name resolves"
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
                OsStr::new("/bin/mv"),
                &[
                    source.display().to_string().into(),
                    destination.display().to_string().into(),
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

    /// A program that can write a repository's `.git/config` or a hook in it has a command run
    /// outside confinement the next time a person, or a shell prompt, runs git there. So a write
    /// row reaches everything beneath it except a `.git`, existing or new, in either case.
    ///
    /// Writes beside the `.git` succeed in the same run, so the refusals are of `.git` and not of
    /// the row.
    #[test]
    fn a_write_row_does_not_reach_a_git_directory_beneath_it() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");

        let dir = crate::testutil::scratch_dir("bravebot-sandbox-git-directory");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git").join("hooks"))
            .expect("the scratch directory is creatable");
        std::fs::create_dir_all(dir.join("below")).expect("the scratch directory is creatable");
        let config = dir.join(".git").join("config");
        std::fs::write(&config, b"[core]\n").expect("the file is writable");

        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(&dir)
            .allow_write(&dir);
        let touch = |target: &Path| {
            sandbox
                .spawn(
                    OsStr::new("/usr/bin/touch"),
                    &[target.display().to_string().into()],
                    &policy,
                    nothing_attached(),
                    Environment::Inherited,
                )
                .expect("should spawn")
                .wait()
                .expect("should wait")
                .code()
        };

        for allowed in [
            dir.join("beside"),
            dir.join(".gitignore"),
            dir.join("below").join("file"),
        ] {
            assert_eq!(
                touch(&allowed),
                Some(0),
                "{} was refused",
                allowed.display()
            );
            assert!(allowed.exists(), "{} was not written", allowed.display());
        }

        let modified = std::fs::metadata(&config).and_then(|m| m.modified()).ok();
        assert_eq!(
            touch(&config),
            Some(TOUCH_FAILED),
            "the repository's config was written"
        );
        assert_eq!(
            std::fs::metadata(&config).and_then(|m| m.modified()).ok(),
            modified,
            "the repository's config was touched"
        );
        for refused in [
            dir.join(".git").join("hooks").join("pre-commit"),
            dir.join("below").join(".git"),
            dir.join("below").join(".GIT"),
        ] {
            assert_eq!(
                touch(&refused),
                Some(TOUCH_FAILED),
                "{} was not refused",
                refused.display()
            );
            assert!(!refused.exists(), "{} was written", refused.display());
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A caller running a program a person asked to run against a repository may lift the
    /// withholding, and the lifting reaches the `.git` and nothing else: the control that gives
    /// the refusal above its meaning, since a profile dropping every write refusal passes the
    /// writes below just as readily.
    #[test]
    fn a_policy_allowing_git_directory_writes_reaches_a_git_directory() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");

        let dir = crate::testutil::scratch_dir("bravebot-sandbox-git-directory-allowed");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git")).expect("the scratch directory is creatable");

        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_read(&dir)
            .allow_write(&dir)
            .allow_git_directory_writes();
        let target = dir.join(".git").join("config");
        let status = sandbox
            .spawn(
                OsStr::new("/usr/bin/touch"),
                &[target.display().to_string().into()],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn")
            .wait()
            .expect("should wait");

        assert!(status.success(), "a write under .git was refused");
        assert!(target.exists(), "the write created nothing");
        let outside = dir.parent().expect("a parent").join("bravebot-not-granted");
        let refused = sandbox
            .spawn(
                OsStr::new("/usr/bin/touch"),
                &[outside.display().to_string().into()],
                &policy,
                nothing_attached(),
                Environment::Inherited,
            )
            .expect("should spawn")
            .wait()
            .expect("should wait");
        assert_eq!(refused.code(), Some(TOUCH_FAILED));
        assert!(!outside.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The profile carries the `.git` refusal exactly when a write is granted and the policy has
    /// not lifted it, so neither a read-only policy gains a stray rule nor a lifted one keeps it.
    #[test]
    fn the_git_refusal_is_in_the_profile_only_for_a_policy_that_writes_and_has_not_lifted_it() {
        let writes = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_write("/work");
        assert!(SeatbeltSandbox::profile(&writes).contains(GIT_DIRECTORY_WRITE));
        assert!(
            !SeatbeltSandbox::profile(&writes.clone().allow_git_directory_writes())
                .contains(GIT_DIRECTORY_WRITE)
        );
        assert!(
            !SeatbeltSandbox::profile(&SandboxPolicy::strict().allow_read("/usr"))
                .contains(GIT_DIRECTORY_WRITE)
        );
    }

    /// A command handed back for the caller to spawn is confined by the time it runs: the caller
    /// wires its own streams, as a pipeline must, and the profile and the environment it was given
    /// are still what the process gets. A `command` that handed back the bare program passes the
    /// granted half and fails the refused one.
    #[test]
    fn a_command_handed_back_is_confined_when_the_caller_spawns_it() {
        let sandbox = SeatbeltSandbox::new().expect("sandbox-exec is present on macOS");
        let granted = crate::testutil::scratch_dir("bravebot-sandbox-command-granted");
        let withheld = crate::testutil::scratch_dir("bravebot-sandbox-command-withheld");
        for dir in [&granted, &withheld] {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir_all(dir).expect("the scratch directory is creatable");
        }
        let policy = SandboxPolicy::strict()
            .allow_read("/usr")
            .allow_read("/bin")
            .allow_write(&granted);
        let environment = Environment::Only(crate::Variables::new().with("PATH", "/usr/bin:/bin"));
        let touch = |target: &Path| {
            sandbox
                .command(
                    OsStr::new("/usr/bin/touch"),
                    &[target.display().to_string().into()],
                    &policy,
                    &environment,
                )
                .expect("a command")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .expect("spawned")
                .code()
        };

        assert_eq!(touch(&granted.join("written")), Some(0));
        assert!(granted.join("written").exists());
        assert_eq!(touch(&withheld.join("written")), Some(TOUCH_FAILED));
        assert!(!withheld.join("written").exists());

        let printed = sandbox
            .command(OsStr::new("/usr/bin/env"), &[], &policy, &environment)
            .expect("a command")
            .output()
            .expect("spawned");
        assert_eq!(
            String::from_utf8_lossy(&printed.stdout).trim(),
            "PATH=/usr/bin:/bin"
        );

        for dir in [&granted, &withheld] {
            let _ = std::fs::remove_dir_all(dir);
        }
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
                program.as_os_str(),
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
                OsStr::new("/bin/pwd"),
                &["-P".into()],
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
            confined_argv(program.as_os_str(), &[], &Environment::Inherited, &stripped)
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
                program.as_os_str(),
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

    /// cat or ls reporting that what it was asked for could not be read.
    const READ_FAILED: i32 = 1;

    /// A scratch directory holding a home and a temporary directory, with the two returned as
    /// their links are followed, since that is the path Seatbelt matches a grant against. They are
    /// apart because the base grants its temporary directory whole, and a home inside it would
    /// have every file in it reached through that row.
    fn a_home_and_a_temporary_directory(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let scratch = crate::testutil::scratch_dir(name);
        let _ = std::fs::remove_dir_all(&scratch);
        let home = scratch.join("home");
        let temporary = scratch.join("tmp");
        std::fs::create_dir_all(&home).expect("the scratch home is creatable");
        std::fs::create_dir_all(&temporary).expect("the scratch directory is creatable");
        let home = home.canonicalize().expect("the scratch home is there");
        let temporary = temporary
            .canonicalize()
            .expect("the scratch directory is there");
        (scratch, home, temporary)
    }

    /// What a stage run for the account whose home is `home` is handed: that home, as a session's
    /// stage has the home its policy was built for, and a search path the base reaches.
    fn a_stage_for(home: &Path) -> crate::process::Variables {
        crate::process::Variables::new()
            .with("HOME", home)
            .with("PATH", "/usr/bin:/bin")
    }

    /// The code `program` exits with under `policy` holding `environment` and nothing else, which
    /// is `None` for a process a signal ended.
    fn exit_code_under(
        policy: &SandboxPolicy,
        environment: crate::process::Variables,
        program: &str,
        arguments: &[&str],
    ) -> Option<i32> {
        exit_code_under_with(policy, environment, program, arguments, nothing_attached())
    }

    /// [`exit_code_under`] with the streams the caller chose, for a test whose failure needs what
    /// the program said.
    fn exit_code_under_with(
        policy: &SandboxPolicy,
        environment: crate::process::Variables,
        program: &str,
        arguments: &[&str],
        streams: crate::process::Streams,
    ) -> Option<i32> {
        let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        SeatbeltSandbox::new()
            .expect("sandbox-exec is present on macOS")
            .spawn(
                OsStr::new(program),
                &arguments,
                policy,
                streams,
                Environment::Only(environment),
            )
            .expect("should spawn")
            .wait()
            .expect("should wait")
            .code()
    }

    /// The TLS library this platform ships aborts every program linked against it that cannot
    /// read its configuration file, before the program's own code runs. A base without the row is
    /// `curl`, `openssl` and rustup's `cargo` refused on every machine, as a program that failed.
    #[test]
    fn a_program_linked_against_the_platforms_tls_library_starts_under_the_base() {
        let (scratch, home, temporary) =
            a_home_and_a_temporary_directory("bravebot-sandbox-base-starts-tls");
        let policy = crate::base::base(crate::base::Prelude::MacOs, &temporary, None, Some(&home));

        for (program, argument) in [
            ("/usr/bin/openssl", "version"),
            ("/usr/bin/curl", "--version"),
        ] {
            assert_eq!(
                exit_code_under(&policy, a_stage_for(&home), program, &[argument]),
                Some(0),
                "{program} did not start under the base"
            );
        }

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// `git` and `make` in `/usr/bin` are shims that run the real program out of the active
    /// developer directory. A base without it is every git stage refused before git runs. The
    /// suite was linked by `cc`, which is the same kind of shim, so a machine running this test
    /// has a developer directory, and it is the one `xcode-select` names, as a session's is.
    ///
    /// The temporary directory and `HOME` are this process's, as a session's are: the shims keep
    /// a lookup cache, keyed on the home, and a lookup that misses it runs `xcodebuild`, which
    /// refuses on a machine whose Xcode licence has not been accepted since its last update,
    /// confined or not. The cache is a file this test names inside the temporary directory. git is pointed at an empty configuration
    /// of the account's, so that what starts or does not is the shim and the machine's own
    /// configuration rather than this account's settings.
    #[test]
    fn a_developer_tool_the_platform_ships_as_a_shim_starts_under_the_base() {
        // Nothing is created here but the repository below, which is removed: the path becomes
        // the base's temporary row, and the shims keep their lookup cache in it.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let temporary = std::env::temp_dir()
            .canonicalize()
            .expect("the temporary directory is there");
        let selected = Command::new("/usr/bin/xcode-select")
            .arg("-p")
            .output()
            .expect("xcode-select is on every macOS");
        let developer_directory = PathBuf::from(String::from_utf8_lossy(&selected.stdout).trim())
            .canonicalize()
            .expect("the selected developer directory is there");
        // The repository is initialised under the temporary directory, which a write row would
        // keep git from doing while a `.git` is withheld from it.
        let policy = crate::base::base(
            crate::base::Prelude::MacOs,
            &temporary,
            Some(&developer_directory),
            None,
        )
        .allow_git_directory_writes();
        let home = std::env::var_os("HOME").expect("cargo runs a test with a HOME");
        let repository = temporary.join(format!(
            "bravebot-sandbox-a-shim-repository-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&repository);
        let repository = repository
            .to_str()
            .expect("the temporary directory is UTF-8");

        // The shims keep what a lookup found in a database, and a lookup that misses it runs
        // `xcodebuild`, which a confined process cannot start. The database is named here, inside
        // the temporary directory the base writes, and looked up unconfined first, so that the
        // confined runs below read what is already there whatever this machine's own database
        // holds or where it keeps it.
        let database = temporary.join(format!(
            "bravebot-sandbox-a-shim-lookups-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&database);
        for tool in ["git", "make"] {
            let _ = Command::new("/usr/bin/xcrun")
                .args(["--find", tool])
                .env_clear()
                .env("HOME", &home)
                .env("PATH", "/usr/bin:/bin")
                .env("xcrun_db", &database)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        for program in ["/usr/bin/git", "/usr/bin/make"] {
            assert!(
                Command::new(program)
                    .arg("--version")
                    .env_clear()
                    .env("HOME", &home)
                    .env("PATH", "/usr/bin:/bin")
                    .env("xcrun_db", &database)
                    .stdout(std::process::Stdio::null())
                    .status()
                    .expect("the shim is on every macOS")
                    .success(),
                "{program} does not start here unconfined either"
            );
        }
        for (program, arguments) in [
            ("/usr/bin/git", vec!["--version"]),
            ("/usr/bin/make", vec!["--version"]),
            ("/usr/bin/git", vec!["init", "-q", repository]),
            ("/usr/bin/git", vec!["-C", repository, "status", "--short"]),
        ] {
            let environment = crate::process::Variables::new()
                .with("HOME", &home)
                .with("PATH", "/usr/bin:/bin")
                .with("xcrun_db", &database)
                .with("GIT_CONFIG_GLOBAL", "/dev/null");
            let streams = crate::process::Streams {
                stdin: crate::process::Stream::Null,
                stdout: crate::process::Stream::Null,
                stderr: crate::process::Stream::Inherited,
            };
            assert_eq!(
                exit_code_under_with(&policy, environment, program, &arguments, streams),
                Some(0),
                "{program} {arguments:?} did not start under the base"
            );
        }

        let _ = std::fs::remove_dir_all(repository);
        let _ = std::fs::remove_file(&database);
    }

    /// The run base as the kernel holds it: one process per credential location is refused it,
    /// each token file a program reads by name is read, the three kinds of file in `~/.ssh` that
    /// hold no secret are read beside the private key that is not, the login keychain is read
    /// beside the other keychain files and the directory listing that are not, and nothing outside
    /// the temporary directory is written. The table is spelled out here and not read from the
    /// base, so a row dropped from the base is a row this fails on.
    #[test]
    fn a_stage_under_the_run_base_is_refused_each_credential_location_and_reads_the_rest() {
        let (scratch, home, temporary) =
            a_home_and_a_temporary_directory("bravebot-sandbox-run-base-table");
        let held_back = [
            ".bravebot/gateway-keys.json",
            ".bravebot/leo-premium.json",
            ".bravebot/mcp.json",
            ".ssh/id_ed25519",
            ".ssh/id_rsa",
            ".aws/credentials",
            ".kube/config",
            ".docker/config.json",
            ".azure/accessTokens.json",
            ".config/gcloud/credentials.db",
            ".gnupg/private-keys-v1.d/key",
            "Library/Keychains/aws-vault.keychain-db",
            "Library/Keychains/a-keychain-of-its-own.keychain-db",
            "Library/Application Support/BraveSoftware/Brave-Browser/Default/Cookies",
            "Library/Application Support/Google/Chrome/Default/Cookies",
            "Library/Application Support/Firefox/Profiles/x/cookies.sqlite",
            "Library/Cookies/Cookies.binarycookies",
            "Library/Safari/History.db",
        ];
        let read = [
            ".gitconfig",
            ".config/gh/hosts.yml",
            ".npmrc",
            ".cargo/credentials.toml",
            ".pypirc",
            ".netrc",
            ".git-credentials",
            ".ssh/config",
            ".ssh/known_hosts",
            ".ssh/id_ed25519.pub",
            ".bravebotx/state",
            "Library/Keychains/login.keychain-db",
            "Documents/notes.txt",
            "Library/Application Support/Other/state",
        ];
        for row in held_back.iter().chain(&read) {
            let file = home.join(row);
            std::fs::create_dir_all(file.parent().expect("a row has a parent"))
                .expect("the scratch home is creatable");
            std::fs::write(&file, "contents").expect("the scratch home is writable");
        }
        let policy = crate::base::run_base(crate::base::Prelude::MacOs, &temporary, Some(&home));
        let code = |program: &str, path: PathBuf| {
            exit_code_under(
                &policy,
                a_stage_for(&home),
                program,
                &[&path.display().to_string()],
            )
        };

        for row in held_back {
            assert_eq!(
                code("/bin/cat", home.join(row)),
                Some(READ_FAILED),
                "{row} was read"
            );
        }
        for row in read {
            assert_eq!(
                code("/bin/cat", home.join(row)),
                Some(0),
                "{row} was refused"
            );
        }
        assert_eq!(
            code("/bin/ls", home.join("Library").join("Keychains")),
            Some(READ_FAILED),
            "the keychain directory was listed"
        );
        assert_eq!(
            code("/usr/bin/touch", home.join("Documents").join("made")),
            Some(TOUCH_FAILED),
            "a file was made outside the temporary directory"
        );
        assert_eq!(
            code("/usr/bin/touch", temporary.join("made")),
            Some(0),
            "the temporary directory was not written"
        );
        assert_eq!(
            code(
                "/usr/bin/touch",
                home.join("Library/Keychains/login.keychain-db")
            ),
            Some(TOUCH_FAILED),
            "the login keychain was written"
        );
        assert!(
            SeatbeltSandbox::profile(&policy)
                .contains("(deny file-read* (subpath \"/Library/Keychains\"))"),
            "the machine's keychains are not refused"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The Security framework's cache row as the kernel holds it: a file in the `mds` directory
    /// under the user cache directory is made, and a file beside that directory is not. The cache
    /// directory here is a scratch one, since the row is the same under any.
    #[test]
    fn a_stage_under_the_run_base_writes_the_security_cache_and_no_other_cache() {
        let (scratch, home, temporary) =
            a_home_and_a_temporary_directory("bravebot-sandbox-run-base-security-cache");
        let cache = scratch.join("C");
        std::fs::create_dir_all(cache.join("mds")).expect("the scratch cache is creatable");
        let cache = cache.canonicalize().expect("the scratch cache is there");
        let policy = crate::base::with_security_cache(
            crate::base::run_base(crate::base::Prelude::MacOs, &temporary, Some(&home)),
            &cache,
        );
        let code = |path: PathBuf| {
            exit_code_under(
                &policy,
                a_stage_for(&home),
                "/usr/bin/touch",
                &[&path.display().to_string()],
            )
        };

        assert_eq!(code(cache.join("mds").join("mds.lock")), Some(0));
        assert_eq!(
            code(cache.join("com.example.cache")),
            Some(TOUCH_FAILED),
            "a cache beside the Security framework's was written"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The host's answer: a directory that exists, with no link left in it, which is the form a
    /// Seatbelt row has to be written in.
    #[test]
    fn the_user_cache_directory_is_an_existing_path_with_its_links_followed() {
        let cache = user_cache_directory().expect("macOS names a cache directory per user");

        assert!(cache.is_dir(), "{} is not a directory", cache.display());
        assert_eq!(cache.canonicalize().expect("it is there"), cache);
    }

    /// The cargo list as the kernel holds it: the registry is written, the configuration cargo
    /// fails without is read, and the token beside both, the install, and the configuration
    /// itself are out of reach for writing or reading as the list says.
    #[test]
    fn a_cargo_stage_writes_its_registry_and_reaches_neither_its_token_nor_its_install() {
        let (scratch, home, temporary) =
            a_home_and_a_temporary_directory("bravebot-sandbox-a-cargo-stage");
        let cargo = home.join(".cargo");
        std::fs::create_dir_all(cargo.join("registry").join("index"))
            .expect("the scratch registry is creatable");
        std::fs::create_dir_all(cargo.join("bin")).expect("the scratch install is creatable");
        std::fs::write(cargo.join("config.toml"), "[net]\n").expect("the scratch home is writable");
        std::fs::write(cargo.join("credentials.toml"), "token = \"a token\"\n")
            .expect("the scratch home is writable");
        std::fs::write(cargo.join("bin").join("cargo"), "").expect("the scratch home is writable");
        let policy = crate::toolchain::Toolchain::Cargo.grant(
            crate::base::base(crate::base::Prelude::MacOs, &temporary, None, Some(&home)),
            crate::base::Prelude::MacOs,
            &home,
        );
        let at = |path: PathBuf| path.display().to_string();
        let code = |program: &str, path: PathBuf| {
            exit_code_under(&policy, a_stage_for(&home), program, &[&at(path)])
        };

        assert_eq!(
            code("/bin/cat", cargo.join("config.toml")),
            Some(0),
            "the configuration cargo fails without was not read"
        );
        assert_eq!(
            code(
                "/usr/bin/touch",
                cargo.join("registry").join("index").join("an-entry")
            ),
            Some(0),
            "the registry was not written"
        );
        assert_eq!(
            code("/bin/cat", cargo.join("credentials.toml")),
            Some(READ_FAILED),
            "the token beside the registry was read"
        );
        for not_written in [
            cargo.join("config.toml"),
            cargo.join("credentials.toml"),
            cargo.join("bin").join("cargo"),
        ] {
            assert_eq!(
                code("/usr/bin/touch", not_written.clone()),
                Some(TOUCH_FAILED),
                "{} was written",
                not_written.display()
            );
        }

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The remote scope as the kernel holds it: what ssh reads to verify a host and offer a key
    /// is read, a host it has verified is added, to a `known_hosts` there or to one made in a
    /// `~/.ssh` that has none, and the private key beside them is not read,
    /// nor the directory holding it listed, nor a file written there that ssh or sshd later reads.
    #[test]
    fn a_remote_stage_reads_what_ssh_reads_and_never_a_private_key() {
        let (scratch, home, temporary) =
            a_home_and_a_temporary_directory("bravebot-sandbox-a-remote-stage");
        let ssh = home.join(".ssh");
        std::fs::create_dir_all(&ssh).expect("the scratch home is creatable");
        std::fs::write(ssh.join("config"), "Host *\n").expect("the scratch home is writable");
        std::fs::write(ssh.join("known_hosts"), "github.com ssh-ed25519 AAAA\n")
            .expect("the scratch home is writable");
        std::fs::write(ssh.join("id_ed25519"), "a private key")
            .expect("the scratch home is writable");
        std::fs::write(ssh.join("id_ed25519.pub"), "ssh-ed25519 AAAA")
            .expect("the scratch home is writable");
        let policy = crate::scope::Scope::Remote.grant(
            crate::base::base(crate::base::Prelude::MacOs, &temporary, None, Some(&home)),
            &home,
        );
        let at = |path: PathBuf| path.display().to_string();
        let code = |program: &str, path: PathBuf| {
            exit_code_under(&policy, a_stage_for(&home), program, &[&at(path)])
        };

        for read in ["config", "known_hosts", "id_ed25519.pub"] {
            assert_eq!(
                code("/bin/cat", ssh.join(read)),
                Some(0),
                "{read} was not read"
            );
        }
        assert_eq!(
            exit_code_under(
                &policy,
                a_stage_for(&home),
                "/bin/sh",
                &[
                    "-c",
                    r#"printf 'a-host ssh-ed25519 AAAA\n' >> "$1""#,
                    "sh",
                    &at(ssh.join("known_hosts")),
                ],
            ),
            Some(0),
            "a verified host could not be added"
        );
        assert!(
            std::fs::read_to_string(ssh.join("known_hosts"))
                .expect("the scratch file is readable")
                .contains("a-host"),
            "the added host is not in the file"
        );
        assert_eq!(
            code("/bin/cat", ssh.join("id_ed25519")),
            Some(READ_FAILED),
            "the private key was read"
        );
        assert_eq!(
            code("/bin/ls", ssh.clone()),
            Some(READ_FAILED),
            "the directory holding the private key was listed"
        );
        for not_written in ["config", "authorized_keys"] {
            assert_eq!(
                code("/usr/bin/touch", ssh.join(not_written)),
                Some(TOUCH_FAILED),
                "{not_written} was written"
            );
        }
        assert!(!ssh.join("authorized_keys").exists());

        std::fs::remove_file(ssh.join("known_hosts")).expect("the scratch file is removable");
        assert_eq!(
            code("/usr/bin/touch", ssh.join("known_hosts")),
            Some(0),
            "an account with no known_hosts could not have one made"
        );
        assert!(ssh.join("known_hosts").is_file());

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A public key the person's `~/.ssh/config` names, in a stage that reads the machine except
    /// its credential table: the kernel lets ssh read that file and still refuses the private key
    /// beside it, a public key the configuration does not name, and a file a link in `~/.ssh`
    /// leads to in another credential directory.
    #[test]
    fn a_remote_stage_reads_a_public_key_its_configuration_names_and_never_the_private_one() {
        let (scratch, home, temporary) =
            a_home_and_a_temporary_directory("bravebot-sandbox-a-named-public-key");
        let ssh = home.join(".ssh");
        std::fs::create_dir_all(&ssh).expect("the scratch home is creatable");
        std::fs::create_dir_all(home.join(".aws")).expect("the scratch home is creatable");
        std::fs::write(
            ssh.join("config"),
            "Host *\n  IdentityFile ~/.ssh/work\n  IdentityFile ~/.ssh/toward-aws.pub\n",
        )
        .expect("the scratch home is writable");
        for file in ["work", "work.pub", "unnamed.pub"] {
            std::fs::write(ssh.join(file), "a key").expect("the scratch home is writable");
        }
        std::fs::write(home.join(".aws/credentials.pub"), "a credential")
            .expect("the scratch home is writable");
        std::os::unix::fs::symlink(
            home.join(".aws/credentials.pub"),
            ssh.join("toward-aws.pub"),
        )
        .expect("the scratch home is writable");
        let policy = crate::scope::Scope::Remote.grant(
            crate::base::run_base(crate::base::Prelude::MacOs, &temporary, Some(&home)),
            &home,
        );
        let code = |path: PathBuf| {
            exit_code_under(
                &policy,
                a_stage_for(&home),
                "/bin/cat",
                &[&path.display().to_string()],
            )
        };

        assert_eq!(code(ssh.join("work.pub")), Some(0), "work.pub was not read");
        for refused in ["work", "unnamed.pub", "toward-aws.pub"] {
            assert_eq!(
                code(ssh.join(refused)),
                Some(READ_FAILED),
                "{refused} was read"
            );
        }

        let _ = std::fs::remove_dir_all(&scratch);
    }
}
