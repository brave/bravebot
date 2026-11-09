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

use bravebot_sandbox::programs::{CodeSelectors, Inherits, Names, Resolved, SearchPath, selectors};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Stdio;

/// The variables `xdg-open` and the desktop tools it starts read that name code they then run,
/// beyond the generic names ([`bravebot_sandbox::programs::CODE_SELECTORS`]).
///
/// `BROWSER` is a list of commands that `xdg-open` runs before it looks at a desktop environment,
/// so a value is kept only when every entry is a program found on the reduced `PATH` or an existing
/// absolute path outside every directory a stage may write, alone or followed by ` %s`
/// ([`Names::Commands`]). The four `XDG_` names choose the directories `.desktop` entries, MIME
/// handler associations and configuration are read from, which name the program that opens the
/// address. `XDG_DATA_DIRS` is also in the generic list and is repeated here so this list reads as
/// complete. A value is kept only when every entry is an existing absolute path outside every
/// directory a stage may write
/// ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)).
const CODE_SELECTORS: [(&str, Names); 5] = [
    selectors::BROWSER,
    ("XDG_DATA_HOME", Names::Directory),
    ("XDG_DATA_DIRS", Names::Directories),
    ("XDG_CONFIG_HOME", Names::Directory),
    ("XDG_CONFIG_DIRS", Names::Directories),
];

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
    opening_against(
        program,
        arguments,
        std::env::vars_os(),
        &bravebot_sandbox::programs::outside_stage_writable,
    )
}

/// [`opening`] against a given environment and a given judgment of which files are outside the
/// directories a stage may write.
fn opening_against(
    program: &Resolved,
    arguments: &[String],
    held: impl IntoIterator<Item = (OsString, OsString)>,
    outside_stage_writable: &impl Fn(&Path) -> bool,
) -> std::process::Command {
    let mut command = bravebot_sandbox::programs::start_against(
        program,
        Inherits::Without(&bravebot_config::scrub::own_credentials()),
        SearchPath::Reduced,
        CodeSelectors::Checked(&CODE_SELECTORS),
        held,
        outside_stage_writable,
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

    /// Whether the opener's command takes `name` off, when the person's environment holds `name`
    /// set to `value` and only a path under `/safe` is outside the directories a stage may write.
    fn withheld(name: &str, value: &str) -> bool {
        withheld_bytes(name, OsStr::new(value))
    }

    /// [`withheld`] for a value that need not be text.
    fn withheld_bytes(name: &str, value: &OsStr) -> bool {
        let command = opening_against(
            &Resolved::found_earlier("/usr/bin/open"),
            &[],
            [(OsString::from(name), value.to_os_string())],
            &|path: &Path| path.starts_with("/safe"),
        );
        command
            .get_envs()
            .any(|(set, now)| set == OsStr::new(name) && now.is_none())
    }

    /// SANDBOX-30. The regression it rejects: an entry of `BROWSER` that is not UTF-8 judged by its
    /// lossy rendering, so `/safe/bin/fire\xffox` is read as the name `/safe/bin/fire\u{fffd}ox`
    /// and kept, and `xdg-open` then runs the entry as the bytes it is. The same text with the
    /// replacement character written out is a name, and is kept.
    #[cfg(unix)]
    #[test]
    fn a_browser_entry_that_is_not_text_is_removed_and_not_judged_by_its_lossy_rendering() {
        use std::os::unix::ffi::OsStrExt;

        for value in [
            &b"fire\xffox"[..],
            &b"/safe/bin/fire\xffox %s"[..],
            &b"firefox:/safe/bin/fire\xffox"[..],
        ] {
            assert!(
                withheld_bytes("BROWSER", OsStr::from_bytes(value)),
                "{value:?} was passed on"
            );
        }
        assert!(!withheld("BROWSER", "fire\u{fffd}ox"));
        assert!(!withheld("BROWSER", "/safe/bin/fire\u{fffd}ox %s"));
    }

    /// Whether this process is the child of [`helper_run_by`], which has done what it is for.
    #[cfg(unix)]
    fn is_the_child_of_helper_run_by() -> bool {
        const SESSION: &str = "BRAVEBOT_TEST_BROWSER_SESSION";
        const OPENER: &str = "BRAVEBOT_TEST_BROWSER_OPENER";
        let (Some(session), Some(opener)) = (std::env::var_os(SESSION), std::env::var_os(OPENER))
        else {
            return false;
        };
        bravebot_sandbox::programs::keep_out(Path::new(&session));
        let status = opening(
            &Resolved::found_earlier(std::path::PathBuf::from(opener)),
            &["https://example.invalid/".to_string()],
        )
        .status()
        .unwrap();
        println!("opener ended: {status}");
        true
    }

    /// The opener's command run with `BROWSER` naming a helper that records it ran, once with the
    /// helper in a directory a stage may write and once with it in an installed directory. Says
    /// whether the helper ran in each case, or `None` when `opener` gives no program.
    ///
    /// The opener runs in a child run of this test binary named by `test`, because the command takes
    /// its environment from the process and a test cannot set one. The child keeps the session out
    /// as a stage's directory. The variables that choose a desktop environment are removed, since a
    /// desktop environment's own opener is read before `BROWSER`.
    #[cfg(unix)]
    fn helper_run_by(
        test: &str,
        name: &str,
        opener: impl Fn(&Path) -> Option<std::path::PathBuf>,
    ) -> Option<(bool, bool)> {
        use std::os::unix::fs::PermissionsExt;

        let session = crate::testutil::scratch_dir(&format!("opener-browser-{name}-session"));
        let installed = crate::testutil::scratch_dir(&format!("opener-browser-{name}-installed"));
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        std::fs::create_dir_all(&session).unwrap();
        std::fs::create_dir_all(&installed).unwrap();
        let write = |directory: &Path, file: &str, body: String| {
            let path = directory.join(file);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let records =
            |marker: &str| format!("#!/bin/sh\n: > '{}'\n", session.join(marker).display());
        let planted = write(&session, "helper.sh", records("planted-ran"));
        let genuine = write(&installed, "helper.sh", records("genuine-ran"));
        let program = opener(&installed);
        let ran = program.map(|program| {
            for helper in [&planted, &genuine] {
                // nosemgrep: rust.lang.security.current-exe.current-exe
                let child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", test, "--test-threads=1", "--nocapture"])
                    .env("BRAVEBOT_TEST_BROWSER_SESSION", &session)
                    .env("BRAVEBOT_TEST_BROWSER_OPENER", &program)
                    .env("BROWSER", format!("{} %s", helper.display()))
                    .env_remove("XDG_CURRENT_DESKTOP")
                    .env_remove("XDG_SESSION_DESKTOP")
                    .env_remove("DESKTOP_SESSION")
                    .env_remove("GNOME_DESKTOP_SESSION_ID")
                    .env_remove("KDE_FULL_SESSION")
                    .env_remove("DISPLAY")
                    .env_remove("WAYLAND_DISPLAY")
                    .output()
                    .unwrap();
                assert!(
                    child.status.success(),
                    "{}",
                    String::from_utf8_lossy(&child.stderr)
                );
            }
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !session.join("genuine-ran").exists() && std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            (
                session.join("planted-ran").exists(),
                session.join("genuine-ran").exists(),
            )
        });
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        ran
    }

    /// SANDBOX-30, through a program that runs `BROWSER` as `xdg-open` does: a stand-in that runs
    /// `$BROWSER`. The regression it rejects: the opener started with `BROWSER` as the person set it,
    /// so a helper a stage wrote runs when the sign-in opens a page. The helper in an installed
    /// directory runs, which shows the stand-in does run what `BROWSER` names.
    #[cfg(unix)]
    #[test]
    fn a_browser_helper_a_stage_may_write_is_not_run_by_an_opener_that_runs_browser() {
        if is_the_child_of_helper_run_by() {
            return;
        }
        let ran = helper_run_by(
            "opener::tests::a_browser_helper_a_stage_may_write_is_not_run_by_an_opener_that_runs_browser",
            "stand-in",
            |installed| {
                use std::os::unix::fs::PermissionsExt;
                let path = installed.join("xdg-open");
                std::fs::write(&path, "#!/bin/sh\nsh -c \"$BROWSER\"\n").unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
                Some(path)
            },
        );
        assert_eq!(ran, Some((false, true)));
    }

    /// SANDBOX-30, through the installed `xdg-open` where this host has one. The same two runs as
    /// [`a_browser_helper_a_stage_may_write_is_not_run_by_an_opener_that_runs_browser`], with the
    /// program that reads `BROWSER` being the real one. It prints `skipped` and passes where there
    /// is no `xdg-open` (macOS opens with `/usr/bin/open`, which does not read `BROWSER`), and where
    /// the installed one does not run an installed helper named by `BROWSER` here, since the
    /// planted helper then not running says nothing.
    #[cfg(unix)]
    #[test]
    fn a_browser_helper_a_stage_may_write_is_not_run_by_the_installed_xdg_open() {
        if is_the_child_of_helper_run_by() {
            return;
        }
        let ran = helper_run_by(
            "opener::tests::a_browser_helper_a_stage_may_write_is_not_run_by_the_installed_xdg_open",
            "installed-xdg-open",
            |_| bravebot_sandbox::programs::find(OsStr::new("xdg-open")),
        );
        match ran {
            None => println!("skipped: no xdg-open on this host"),
            Some((_, false)) => {
                println!("skipped: the xdg-open here does not run a BROWSER that is installed");
            }
            Some(ran) => assert_eq!(ran, (false, true)),
        }
    }

    /// The installed `xdg-open` run under a GNOME session, which hands the address to `gio open`,
    /// with `variable` naming a directory that selects the handler. Says whether the handler in a
    /// directory a stage may write ran and whether the one in an installed directory ran.
    ///
    /// Each directory holds a `mimeapps.list` naming its own handler as the default for `https`.
    /// `XDG_DATA_DIRS` names an installed directory that holds both handlers' entries, so a
    /// `mimeapps.list` read from `variable` is the only thing that chooses. `HOME` is a scratch
    /// directory, so the person's own defaults cannot start a browser.
    #[cfg(unix)]
    fn gnome_handler_run_by(test: &str, variable: &str, opener: &Path) -> (bool, bool) {
        use std::os::unix::fs::PermissionsExt;

        let name = variable.to_ascii_lowercase();
        let session = crate::testutil::scratch_dir(&format!("opener-gnome-{name}-session"));
        let installed = crate::testutil::scratch_dir(&format!("opener-gnome-{name}-installed"));
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        let data = installed.join("data/applications");
        let home = installed.join("home");
        for directory in [&session, &data, &home] {
            std::fs::create_dir_all(directory).unwrap();
        }
        let entry = |directory: &Path, marker: &str| {
            let script = installed.join(format!("{marker}.sh"));
            std::fs::write(
                &script,
                format!("#!/bin/sh\n: > '{}'\n", session.join(marker).display()),
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(
                directory.join(format!("{marker}.desktop")),
                format!(
                    "[Desktop Entry]\nType=Application\nName={marker}\nExec={} %u\n",
                    script.display()
                ),
            )
            .unwrap();
        };
        let default = |file: &Path, marker: &str| {
            std::fs::write(
                file,
                format!("[Default Applications]\nx-scheme-handler/https={marker}.desktop\n"),
            )
            .unwrap();
        };
        let planted = session.join("selected");
        let genuine = installed.join("selected");
        for (directory, marker) in [(&planted, "planted-ran"), (&genuine, "genuine-ran")] {
            if variable == "XDG_DATA_HOME" {
                entry(&directory.join("applications"), marker);
                default(&directory.join("applications/mimeapps.list"), marker);
            } else {
                entry(&data, marker);
                std::fs::create_dir_all(directory).unwrap();
                default(&directory.join("mimeapps.list"), marker);
            }
        }
        for directory in [&planted, &genuine] {
            let child = {
                // nosemgrep: rust.lang.security.current-exe.current-exe
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", test, "--test-threads=1", "--nocapture"])
                    .env("BRAVEBOT_TEST_BROWSER_SESSION", &session)
                    .env("BRAVEBOT_TEST_BROWSER_OPENER", opener)
                    .env("HOME", &home)
                    .env_remove("BRAVEBOT_HOME")
                    .env("XDG_CURRENT_DESKTOP", "GNOME")
                    .env("XDG_DATA_DIRS", installed.join("data"))
                    .env_remove("XDG_DATA_HOME")
                    .env_remove("XDG_CONFIG_HOME")
                    .env_remove("XDG_CONFIG_DIRS")
                    .env_remove("BROWSER")
                    .env_remove("XDG_SESSION_DESKTOP")
                    .env_remove("DESKTOP_SESSION")
                    .env_remove("GNOME_DESKTOP_SESSION_ID")
                    .env_remove("KDE_FULL_SESSION")
                    .env_remove("DISPLAY")
                    .env_remove("WAYLAND_DISPLAY")
                    .env(variable, directory)
                    .output()
                    .unwrap()
            };
            assert!(
                child.status.success(),
                "{}",
                String::from_utf8_lossy(&child.stderr)
            );
        }
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !session.join("genuine-ran").exists() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let ran = (
            session.join("planted-ran").exists(),
            session.join("genuine-ran").exists(),
        );
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        ran
    }

    /// SANDBOX-30, through the installed `xdg-open` and `gio open` where this host has both. The
    /// regression it rejects: the opener left `XDG_CONFIG_DIRS`, `XDG_CONFIG_HOME` or
    /// `XDG_DATA_HOME` as the person set it, so a `mimeapps.list` a stage wrote in the directory it
    /// names chooses the program `gio open` runs. The same directory in an installed place is read
    /// and its handler runs, which shows the variable does choose the handler here. `XDG_DATA_DIRS`
    /// is not run: with the planted value removed, `gio open` falls back to the system's handlers
    /// and would start a browser. It prints `skipped` and passes where either program is missing
    /// or the handler in the installed directory does not run.
    #[cfg(unix)]
    #[test]
    fn an_xdg_directory_a_stage_may_write_is_not_read_by_the_installed_xdg_open_under_gnome() {
        if is_the_child_of_helper_run_by() {
            return;
        }
        let (Some(opener), Some(_gio)) = (
            bravebot_sandbox::programs::find(OsStr::new("xdg-open")),
            bravebot_sandbox::programs::find(OsStr::new("gio")),
        ) else {
            println!("skipped: no xdg-open and gio on this host");
            return;
        };
        for variable in ["XDG_CONFIG_DIRS", "XDG_CONFIG_HOME", "XDG_DATA_HOME"] {
            let ran = gnome_handler_run_by(
                "opener::tests::an_xdg_directory_a_stage_may_write_is_not_read_by_the_installed_xdg_open_under_gnome",
                variable,
                &opener,
            );
            match ran {
                (_, false) => println!(
                    "skipped {variable}: the xdg-open here does not run a handler chosen by it"
                ),
                ran => assert_eq!(ran, (false, true), "{variable}"),
            }
        }
    }

    /// SANDBOX-30. The regression it rejects: the opener started with `Checked(&[])`, or with the
    /// list left out of the start, so `BROWSER` names a program `xdg-open` runs before it reads a
    /// desktop environment, and the `XDG_` directories name the handler it runs, from a file a stage
    /// wrote. Each name has a value a stage may write and a safe one.
    #[test]
    fn the_opener_removes_a_browser_or_xdg_directory_that_names_a_file_a_stage_may_write() {
        assert!(withheld("BROWSER", "/stage/browser.sh %s"));
        assert!(withheld("BROWSER", "/safe/bin/firefox:/stage/browser.sh"));
        assert!(!withheld("BROWSER", "firefox"));
        assert!(!withheld("BROWSER", "/safe/bin/firefox %s"));

        assert!(withheld("XDG_DATA_HOME", "/stage/share"));
        assert!(withheld("XDG_DATA_HOME", "share"));
        assert!(!withheld("XDG_DATA_HOME", "/safe/share"));

        assert!(withheld("XDG_DATA_DIRS", "/safe/share:/stage/share"));
        assert!(!withheld("XDG_DATA_DIRS", "/safe/share:/safe/local/share"));

        assert!(withheld("XDG_CONFIG_HOME", "/stage/config"));
        assert!(!withheld("XDG_CONFIG_HOME", "/safe/config"));

        assert!(withheld("XDG_CONFIG_DIRS", "/safe/etc:/stage/etc"));
        assert!(!withheld("XDG_CONFIG_DIRS", "/safe/etc/xdg"));
    }

    /// SANDBOX-30. The regression it rejects: `BROWSER` judged as a single program name, so a list
    /// of commands, a command with a `%s`, or a command with an argument or shell syntax is kept
    /// because its first word is a bare name.
    #[test]
    fn a_browser_value_is_kept_only_when_every_command_in_it_is_one_program_alone_or_with_a_placeholder()
     {
        assert!(!withheld("BROWSER", "firefox:chromium %s"));
        assert!(!withheld("BROWSER", "w3m %s:lynx"));
        assert!(!withheld("BROWSER", "firefox %s:/safe/bin/w3m"));

        assert!(withheld("BROWSER", "firefox:/stage/firefox"));
        assert!(withheld("BROWSER", "firefox --new-window %s"));
        assert!(withheld("BROWSER", "sh -c 'x' %s"));
        assert!(withheld("BROWSER", "firefox %s; x"));
        assert!(withheld("BROWSER", "$(x) %s"));
        assert!(withheld("BROWSER", ""));
    }

    /// The regression it rejects: the opener's list of names applied by removing everything, or by
    /// removing a variable whose value is safe, so a person who set `BROWSER` or `XDG_DATA_DIRS` to
    /// an installed program or directory, or any other variable, loses it.
    #[test]
    fn the_opener_keeps_an_unrelated_variable_and_a_safe_value_of_its_own_names() {
        for name in [
            "EDITOR",
            "HOME",
            "XDG_RUNTIME_DIR",
            "DISPLAY",
            "LANG",
            "PATH",
        ] {
            assert!(!withheld(name, "/stage/anything"), "{name} was removed");
        }
        assert!(!withheld("BROWSER", "/safe/bin/firefox"));
        assert!(!withheld("XDG_DATA_DIRS", "/safe/share"));
        assert!(!withheld("XDG_CONFIG_DIRS", "/safe/etc/xdg"));
    }

    /// SANDBOX-30, on the disk: a directory a stage planted under the session is removed from
    /// `XDG_DATA_HOME` and a directory outside it is kept, with a link into the session followed.
    /// The regression it rejects: a comparison of the names as written, which a link slips past.
    #[cfg(unix)]
    #[test]
    fn an_xdg_directory_is_judged_by_where_it_leads_on_the_disk() {
        let session = crate::testutil::scratch_dir("opener-xdg-session");
        let safe = crate::testutil::scratch_dir("opener-xdg-safe");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
        let planted = session.join("share");
        let genuine = safe.join("share");
        std::fs::create_dir_all(&planted).unwrap();
        std::fs::create_dir_all(&genuine).unwrap();
        let link = safe.join("to-planted");
        std::os::unix::fs::symlink(&planted, &link).unwrap();
        let writable = vec![session.clone()];
        let outside = |path: &Path| {
            bravebot_sandbox::programs::outside_stage_writable_with(path, &writable, &[])
        };
        let removed = |value: &Path| {
            opening_against(
                &Resolved::found_earlier("/usr/bin/open"),
                &[],
                [(
                    OsString::from("XDG_DATA_HOME"),
                    value.as_os_str().to_os_string(),
                )],
                &outside,
            )
            .get_envs()
            .any(|(set, now)| set == OsStr::new("XDG_DATA_HOME") && now.is_none())
        };

        assert!(removed(&planted));
        assert!(removed(&link));
        assert!(removed(&safe.join("missing")));
        assert!(!removed(&genuine));
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
