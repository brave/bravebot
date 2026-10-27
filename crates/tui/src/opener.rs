//! Handing an address to the person's own browser.
//!
//! Two things start the browser: `bravebot auth login chatgpt`, which shows the address a sign-in
//! waits on, and a click on a link in the interface. Neither fetches anything: the platform's own
//! opener is started with the address and the browser it chooses does the rest, as it would for an
//! address typed at a shell.
//!
//! The address is one argument and no shell stands between it and the program, because a query
//! holds `&`. It is checked again here, whatever the caller checked, since it is the last place
//! before a program that reads an argument starting with `-` as an option or a file name as
//! something to open.
//!
//! The opener is looked up and started as every program bravebot looks up by name is
//! ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)): resolved first, from where a
//! confined stage could not have written, with the variables that spec removes left out, and with
//! no standard stream.

use bravebot_sandbox::programs::{Inherits, Resolved, SearchPath};
use std::ffi::OsStr;
use std::process::Stdio;

/// Whether `address` may be handed to the opener, which is the one test every caller shares.
///
/// An `http` or `https` address with a host, in printable ASCII and without a backslash. Nothing else
/// is something a click or a sign-in has a reason to open, and everything else is how an opener is
/// made to do something other than show a page: a `file:` address opens a program, an address that
/// starts with `-` is an option, and a character outside printable ASCII is a lookalike of the host or
/// hides one (a right-to-left override, a zero-width character). An address naming a user, as in
/// `https://github.com@example.test/`, is refused because the part a person reads first is not the
/// host. So is one with a backslash, which a browser reads as a slash in an `http` or `https`
/// address: the host of `https://evil.test\.github.com/` is `evil.test`, and it reads like
/// `github.com`.
pub fn is_openable(address: &str) -> bool {
    if !address
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && byte != b'\\')
    {
        return false;
    }
    let Some((scheme, rest)) = address.split_once("://") else {
        return false;
    };
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")) {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !authority.is_empty()
        && !authority.contains('@')
        && authority.starts_with(|first: char| first.is_ascii_alphanumeric())
}

/// The program that opens an address in the platform's browser, and the arguments it takes, for
/// the operating system named.
///
/// The address is one argument and no shell is between it and the program: the query holds `&`, which a
/// shell would read. Windows has no program for this that is not a shell builtin, so it is
/// `rundll32` with the library's entry point rather than `start`.
fn opener(os: &str, address: &str) -> (&'static str, Vec<String>) {
    match os {
        "macos" => ("open", vec![address.to_string()]),
        "windows" => (
            "rundll32",
            vec![
                "url.dll,FileProtocolHandler".to_string(),
                address.to_string(),
            ],
        ),
        _ => ("xdg-open", vec![address.to_string()]),
    }
}

/// Ask the platform to open `address`, and say whether its opener was started.
///
/// Not whether a page appeared: an opener that exits successfully and opens nothing is what a
/// machine reached over SSH has, and nothing here can tell. `false` is an address that fails
/// [`is_openable`], which starts nothing, and an opener that is not installed or would not start.
/// The sign-in carries on waiting whatever this says, since the address was shown to the person; a
/// click says that it could not.
pub fn open_address(address: &str) -> bool {
    if !is_openable(address) {
        return false;
    }
    let (program, arguments) = opener(std::env::consts::OS, address);
    let Some(program) = Resolved::find(OsStr::new(program)) else {
        return false;
    };
    let Ok(mut child) = opening(&program, &arguments).spawn() else {
        return false;
    };
    // Waited for on a thread of its own, so the opener is not left a zombie and a browser that
    // keeps it running does not hold anything up.
    std::thread::spawn(move || {
        let _ended = child.wait();
    });
    true
}

/// The command that opens the browser, which is handed no standard stream and the person's
/// environment without this agent's own credentials.
fn opening(program: &Resolved, arguments: &[String]) -> std::process::Command {
    let mut command = bravebot_sandbox::programs::start(
        program,
        Inherits::Without(&bravebot_config::scrub::own_credentials()),
        SearchPath::Reduced,
    );
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CHATGPT-13: the address is one argument and no shell is between it and the program. The query
    /// holds `&`, which a shell reads as the end of a command, and Windows has no opener that is not
    /// a builtin of `cmd` except `rundll32` with the library's entry point.
    #[test]
    fn the_address_is_one_argument_of_the_platforms_own_opener() {
        let url = "https://auth.example/authorize?a=1&b=2";

        assert_eq!(opener("macos", url), ("open", vec![url.to_string()]));
        assert_eq!(opener("linux", url), ("xdg-open", vec![url.to_string()]));
        assert_eq!(
            opener("windows", url),
            (
                "rundll32",
                vec!["url.dll,FileProtocolHandler".to_string(), url.to_string()]
            )
        );
        for os in ["macos", "linux", "windows", "freebsd"] {
            let (program, arguments) = opener(os, url);
            assert!(
                !["sh", "cmd", "start", "bash"].contains(&program),
                "{os} goes through a shell: {program}"
            );
            assert_eq!(
                arguments
                    .iter()
                    .filter(|argument| argument.as_str() == url)
                    .count(),
                1,
                "{os}: {arguments:?}"
            );
        }
    }

    /// What a command was told to unset, which is how `env_remove` reads back.
    fn withheld_from(command: &std::process::Command) -> Vec<String> {
        let mut names: Vec<String> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// CHATGPT-13: the opener is started with the variables [CRED-14] removes left out and the
    /// arguments it was given. A start that inherited everything would hand this agent's own
    /// credentials to whatever `open` or `xdg-open` resolves to.
    #[test]
    fn the_opener_is_not_handed_this_agents_own_credentials() {
        let program = Resolved::found_earlier("/usr/bin/open");
        let arguments = vec!["https://example.invalid/".to_string()];

        let command = opening(&program, &arguments);

        let mut expected = bravebot_config::scrub::own_credentials();
        expected.sort();
        assert_eq!(withheld_from(&command), expected);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["https://example.invalid/"]
        );
    }

    /// An address that is not a web address with a host in printable ASCII is not handed to the
    /// opener, because an opener reads an option, a file, a program and a lookalike host as readily
    /// as a page.
    #[test]
    fn only_a_web_address_with_a_host_in_printable_ascii_is_openable() {
        for address in [
            "https://example.test/",
            "http://example.test/a?b=1&c=2#d",
            "HTTPS://EXAMPLE.TEST/",
            "https://127.0.0.1:8080/x",
            "https://example.test",
            "https://example.test?q=1",
        ] {
            assert!(is_openable(address), "{address}");
        }
        for address in [
            "",
            "-a Calculator",
            "--help",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://example.test/",
            "mailto:a@example.test",
            "example.test",
            "https://",
            "https:///path",
            "https://:80/",
            "https://example.test/a b",
            "https://example.test/\u{1b}]8;;",
            "https://example.test/\u{7}",
            "https://example.test/\n",
            "https://exampl\u{e9}.test/",
            "https://example.test/\u{202e}txt.exe",
            "https://example.test/\u{200b}",
            "https://github.com@example.test/",
            "https://user:pass@example.test/",
            "https://example.test@/",
            "https://evil.test\\.github.com/",
            "https://example.test\\",
            "https://example.test/a\\b",
        ] {
            assert!(!is_openable(address), "{address:?}");
        }
    }

    /// PATH-005: what the opener is started as goes through [`open_address`], in a child run of this
    /// test binary whose `PATH` is a directory SANDBOX-30 leaves out, an installed directory and the
    /// system directories, in that order. Each name the opener could be has a planted file in the
    /// first directory and an installed one in the second, and each records that it ran.
    ///
    /// The child also asks for addresses that fail [`is_openable`] while an installed opener is
    /// there to start. An address refused for that reason is the only way `false` comes back with an
    /// opener present, so the refusal is what stops the start.
    #[cfg(unix)]
    fn through_open_address(
        test: &str,
        name: &str,
        installed_opener: impl Fn(&str) -> String,
        expect: &str,
    ) {
        use std::os::unix::fs::PermissionsExt;
        const SESSION: &str = "BRAVEBOT_TEST_OPENER_SESSION";

        if let Some(session) = std::env::var_os(SESSION) {
            let session = std::path::PathBuf::from(session);
            std::fs::write(session.join("ran"), "").unwrap();
            bravebot_sandbox::programs::keep_out(&session);

            for refused in [
                "-a Calculator",
                "file:///etc/passwd",
                "https://user@example.invalid/",
            ] {
                assert!(!open_address(refused), "{refused} started the opener");
            }
            assert!(open_address("https://example.invalid/"));

            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !session.join(expect).exists() && std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(
                !session.join("planted-ran").exists(),
                "a file in a directory a stage may write was run"
            );
            assert!(
                session.join(expect).exists(),
                "the installed opener did not run"
            );
            return;
        }

        let session = crate::testutil::scratch_dir(&format!("opener-{name}-session"));
        let installed = crate::testutil::scratch_dir(&format!("opener-{name}-installed"));
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        let planted = session.join(".venv/bin");
        std::fs::create_dir_all(&planted).unwrap();
        std::fs::create_dir_all(&installed).unwrap();
        let write = |directory: &std::path::Path, name: &str, body: String| {
            let file = directory.join(name);
            std::fs::write(&file, body).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let records =
            |marker: &str| format!("#!/bin/sh\n: > '{}'\n", session.join(marker).display());
        for name in ["open", "xdg-open", "interp"] {
            write(&planted, name, records("planted-ran"));
        }
        for name in ["open", "xdg-open"] {
            write(
                &installed,
                name,
                installed_opener(&session.display().to_string()),
            );
        }
        write(&installed, "interp", records("installed-interp-ran"));
        let path = std::env::join_paths([
            planted,
            installed.clone(),
            std::path::PathBuf::from("/usr/bin"),
            std::path::PathBuf::from("/bin"),
        ])
        .unwrap();

        // nosemgrep: rust.lang.security.current-exe.current-exe
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--test-threads=1", "--nocapture"])
            .env(SESSION, &session)
            .env("PATH", &path)
            .output()
            .unwrap();

        let ran = session.join("ran").exists();
        let report = String::from_utf8_lossy(&child.stderr).into_owned();
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        assert!(ran, "the child did not run the test: {report}");
        assert!(child.status.success(), "{report}");
    }

    /// PATH-005, the program: a file named `open` or `xdg-open` in a directory SANDBOX-30 leaves out,
    /// first on `PATH`, is not the opener. The installed one in the next directory is.
    #[cfg(unix)]
    #[test]
    fn a_browser_opener_in_a_directory_a_stage_may_write_is_not_the_one_started() {
        through_open_address(
            "opener::tests::a_browser_opener_in_a_directory_a_stage_may_write_is_not_the_one_started",
            "kept-out",
            |session| format!("#!/bin/sh\n: > '{session}/installed-opener-ran'\n"),
            "installed-opener-ran",
        );
    }

    /// PATH-005, a name the program starts: the installed opener is `#!/usr/bin/env interp`, and an
    /// `interp` in the directory a stage may write comes first on the `PATH` bravebot inherits. The
    /// opener is started with the reduced `PATH`, so `env` finds the installed `interp`.
    #[cfg(unix)]
    #[test]
    fn a_program_the_opener_starts_by_name_is_never_one_a_stage_may_have_written() {
        through_open_address(
            "opener::tests::a_program_the_opener_starts_by_name_is_never_one_a_stage_may_have_written",
            "by-name",
            |_| "#!/usr/bin/env interp\n".to_string(),
            "installed-interp-ran",
        );
    }
}
