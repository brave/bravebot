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
/// `BROWSER` is a list of commands that `xdg-open` runs in its generic mode, which it takes when it
/// detects no desktop environment and falls back to when the tool a desktop environment's branch
/// uses (`gio`, for GNOME) is missing. With `gio` installed it never reads `BROWSER` under GNOME.
/// A value is kept only when every entry is a program found on the reduced `PATH` or an existing
/// absolute path outside every directory a stage may write, alone or followed by ` %s`
/// ([`Names::Commands`]). The four `XDG_` names choose the directories `.desktop` entries, MIME
/// handler associations and configuration are read from, which name the program that opens the
/// address. `XDG_DATA_DIRS` is also in the generic list and is repeated here so this list reads as
/// complete. A value is kept only when every entry is an existing absolute path outside every
/// directory a stage may write
/// ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)).
///
/// The other names are those of `gio open` and `kde-open`. GLib loads a module from each directory
/// of `GIO_EXTRA_MODULES` and from `GIO_MODULE_DIR`. Qt loads plug-ins from `QT_PLUGIN_PATH` and
/// `QT_QPA_PLATFORM_PLUGIN_PATH`, and finds them by the names in `QT_QPA_PLATFORMTHEME`,
/// `QT_QPA_GENERIC_PLUGINS`, `QT_QPA_PLATFORM` and `QT_STYLE_OVERRIDE`, which are options and so
/// removed whatever they hold. `KDESYCOCA` is the service cache `kde-open` reads `Exec` lines from.
///
/// `HOME` is listed as one directory because `gio open`, which `xdg-open` runs under a GNOME
/// session, reads `$HOME/.config/mimeapps.list` and `$HOME/.local/share/applications` when
/// `XDG_CONFIG_HOME` and `XDG_DATA_HOME` are not set, and Qt and GLib keep configuration under it.
/// [`home_reads_outside_stage_writable`] also judges those two paths, since a stage can write them
/// without writing `HOME` itself. A value a stage may write is removed and the program falls back
/// to the account's own home.
///
/// `GIO_LAUNCH_DESKTOP` is the path of the wrapper program that `gio open` executes with the whole
/// command line of the handler it starts (GLib 2.88.4, `gdesktopappinfo.c` line 3100), so it is one
/// file ([`Names::File`]) and is kept only when it is an existing file outside every directory a
/// stage may write.
const CODE_SELECTORS: [(&str, Names); 16] = [
    selectors::BROWSER,
    ("HOME", Names::Directory),
    ("XDG_DATA_HOME", Names::Directory),
    ("XDG_DATA_DIRS", Names::Directories),
    ("XDG_CONFIG_HOME", Names::Directory),
    ("XDG_CONFIG_DIRS", Names::Directories),
    ("GIO_EXTRA_MODULES", Names::Directories),
    ("GIO_MODULE_DIR", Names::Directory),
    ("GIO_LAUNCH_DESKTOP", Names::File),
    ("QT_PLUGIN_PATH", Names::Directories),
    ("QT_QPA_PLATFORM_PLUGIN_PATH", Names::Directory),
    ("QT_QPA_PLATFORMTHEME", Names::Options),
    ("QT_QPA_GENERIC_PLUGINS", Names::Options),
    ("QT_QPA_PLATFORM", Names::Options),
    ("QT_STYLE_OVERRIDE", Names::Options),
    ("KDESYCOCA", Names::File),
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
    let held: Vec<(OsString, OsString)> = held.into_iter().collect();
    let home = held
        .iter()
        .find(|(name, _)| name == OsStr::new("HOME"))
        .map(|(_, value)| std::path::PathBuf::from(value));
    let mut command = bravebot_sandbox::programs::start_against(
        program,
        Inherits::Without(&bravebot_config::scrub::own_credentials()),
        SearchPath::Reduced,
        CodeSelectors::Checked(&CODE_SELECTORS),
        held,
        outside_stage_writable,
    );
    if let Some(home) = home
        && !home_reads_outside_stage_writable(&home, outside_stage_writable)
    {
        command.env_remove("HOME");
    }
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// The paths under `HOME` that `gio open` reads the handler from when the matching `XDG_`
/// directory is not set.
const READ_UNDER_HOME: [&str; 2] = [".config", ".local/share/applications"];

/// Whether what `xdg-open` reads under `home` is outside the directories a stage may write.
///
/// A session opened on `~/.config` or `~/.local/share` has no row for `HOME` itself, so `HOME` alone
/// passes while the files read under it can be written. Each path in [`READ_UNDER_HOME`] is judged
/// as it is when it exists (a link counts and is followed by the judgment). One that does not exist
/// is judged by the nearest existing directory above it below `home`, since whoever may write that
/// directory can make the path. `home` itself is judged by [`CODE_SELECTORS`].
fn home_reads_outside_stage_writable(
    home: &Path,
    outside_stage_writable: &impl Fn(&Path) -> bool,
) -> bool {
    READ_UNDER_HOME.iter().all(|under| {
        let mut path = home.join(under);
        while path != home {
            if path.symlink_metadata().is_ok() {
                return outside_stage_writable(&path);
            }
            if !path.pop() {
                break;
            }
        }
        true
    })
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
        let address = std::env::var("BRAVEBOT_TEST_BROWSER_ADDRESS")
            .unwrap_or_else(|_| "https://example.invalid/".to_string());
        let status = opening(
            &Resolved::found_earlier(std::path::PathBuf::from(opener)),
            &[address],
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
    /// as a stage's directory. The variables that choose a desktop environment and the display
    /// variables are removed, so `xdg-open` takes its generic mode, which is the only place it reads
    /// `BROWSER` (a desktop environment's own tool, `gio` for GNOME, is tried first and `BROWSER`
    /// is not read when it runs).
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

    /// SANDBOX-30, through the installed `xdg-open` where this host has one, in its generic mode
    /// (no desktop environment and no display named), the only mode in which `xdg-open` reads
    /// `BROWSER`: under GNOME, with `gio` installed, it runs `gio open` and never reads it. The same
    /// two runs as
    /// [`a_browser_helper_a_stage_may_write_is_not_run_by_an_opener_that_runs_browser`], with the
    /// program that reads `BROWSER` being the real one. It prints `skipped` and passes where there
    /// is no `xdg-open` (macOS opens with `/usr/bin/open`, which does not read `BROWSER`), and where
    /// the installed one does not run an installed helper named by `BROWSER` here (a host whose
    /// session bus names a desktop environment `xdg-open` detects, for one), since the planted
    /// helper then not running says nothing.
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

    /// The installed `xdg-open` run under the desktop environment `desktop` sets, with `variable`
    /// naming a directory that selects the handler, or for `BROWSER` a command. Says whether the
    /// handler in a directory a stage may write ran and whether the one in an installed directory
    /// ran. `desktop` is the environment that chooses the desktop: an empty list is generic mode,
    /// and every variable that chooses a desktop is removed first, so only those given are set.
    /// Under GNOME `xdg-open` hands the address to `gio open`, and under KDE to `kde-open`.
    ///
    /// Each directory holds a `mimeapps.list` naming its own handler as the default for `https`.
    /// `XDG_DATA_DIRS` names an installed directory that holds both handlers' entries, so a
    /// `mimeapps.list` read from `variable` is the only thing that chooses. `HOME` is a scratch
    /// directory, so the person's own defaults cannot start a browser, except when `variable` is
    /// `HOME`, which is then the directory under test.
    ///
    /// `XDG_DATA_DIRS` is the variable that cannot be run that way: with the planted value
    /// removed, `gio open` reads the system's directories and starts the browser they name for
    /// `https`. Its directories hold the entries and the `mimeapps.list` themselves, and the
    /// address has the scheme `bravebot-test`, which no system handler opens, so a removed value
    /// leaves `gio open` with no handler and starts nothing.
    ///
    /// In generic mode `xdg-open` asks `xdg-mime` for the handler of `https` only when it sees a
    /// display (`has_display` in `xdg-open.in`: `DISPLAY` or `WAYLAND_DISPLAY` not empty), so a
    /// generic run that gives no display sets `WAYLAND_DISPLAY` to a name no socket has, and the
    /// generic-mode test gives `DISPLAY` set to `:9999`, which no X server answers. Nothing
    /// connects to a real display on that path: the only programs that could, `xprop` in `detectDE`
    /// and an X client, read `DISPLAY`. `DBUS_SESSION_BUS_ADDRESS` is removed in every run that
    /// sets no `XDG_CURRENT_DESKTOP` so the
    /// `dbus-send` probe in `detectDE` cannot name GNOME. `BROWSER` names an installed stand-in
    /// that records `fallback-ran`, which the generic path runs after the handler lookup finds
    /// nothing; with `BROWSER` unset and a display it would try `x-www-browser`, `firefox` and
    /// `chromium` and start a real browser. Under GNOME `BROWSER` is not read.
    ///
    /// `BROWSER` names a command, so each side holds a `helper.sh` that records it ran and
    /// `BROWSER` is that path followed by ` %s`, which replaces the stand-in above.
    #[cfg(unix)]
    fn handler_run_by(
        test: &str,
        variable: &str,
        desktop: &[(&str, &str)],
        opener: &Path,
    ) -> (bool, bool) {
        use std::os::unix::fs::PermissionsExt;

        let name = variable.to_ascii_lowercase();
        // The test's own name is part of the directory, since tests run in parallel and every test
        // uses the same four variable names.
        let label = test.rsplit("::").next().unwrap_or(test);
        let session = crate::testutil::scratch_dir(&format!("opener-{label}-{name}-session"));
        let installed = crate::testutil::scratch_dir(&format!("opener-{label}-{name}-installed"));
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
        let scheme = if variable == "XDG_DATA_DIRS" {
            "bravebot-test"
        } else {
            "https"
        };
        let default = |file: &Path, marker: &str| {
            std::fs::write(
                file,
                format!("[Default Applications]\nx-scheme-handler/{scheme}={marker}.desktop\n"),
            )
            .unwrap();
        };
        let fallback = installed.join("fallback.sh");
        std::fs::write(
            &fallback,
            format!(
                "#!/bin/sh\n: > '{}'\n",
                session.join("fallback-ran").display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fallback, std::fs::Permissions::from_mode(0o755)).unwrap();
        let planted = session.join("selected");
        let genuine = installed.join("selected");
        for (directory, marker) in [(&planted, "planted-ran"), (&genuine, "genuine-ran")] {
            if variable == "BROWSER" {
                std::fs::create_dir_all(directory).unwrap();
                let helper = directory.join("helper.sh");
                std::fs::write(
                    &helper,
                    format!("#!/bin/sh\n: > '{}'\n", session.join(marker).display()),
                )
                .unwrap();
                std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
            } else if variable == "XDG_DATA_HOME" || variable == "XDG_DATA_DIRS" {
                entry(&directory.join("applications"), marker);
                default(&directory.join("applications/mimeapps.list"), marker);
            } else if variable == "HOME" {
                entry(&directory.join(".local/share/applications"), marker);
                std::fs::create_dir_all(directory.join(".config")).unwrap();
                default(&directory.join(".config/mimeapps.list"), marker);
            } else {
                entry(&data, marker);
                std::fs::create_dir_all(directory).unwrap();
                default(&directory.join("mimeapps.list"), marker);
            }
        }
        let mut logs: Vec<String> = Vec::new();
        for directory in [&planted, &genuine] {
            let value = if variable == "BROWSER" {
                OsString::from(format!("{} %s", directory.join("helper.sh").display()))
            } else {
                directory.clone().into_os_string()
            };
            let child = {
                // nosemgrep: rust.lang.security.current-exe.current-exe
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                command
                    .args(["--exact", test, "--test-threads=1", "--nocapture"])
                    .env("BRAVEBOT_TEST_BROWSER_SESSION", &session)
                    .env("BRAVEBOT_TEST_BROWSER_OPENER", opener)
                    .env(
                        "BRAVEBOT_TEST_BROWSER_ADDRESS",
                        format!("{scheme}://example.invalid/"),
                    )
                    .env("BROWSER", format!("{} %s", fallback.display()))
                    .env("HOME", &home)
                    .env_remove("BRAVEBOT_HOME")
                    .env("XDG_DATA_DIRS", installed.join("data"))
                    .env_remove("XDG_DATA_HOME")
                    .env_remove("XDG_CONFIG_HOME")
                    .env_remove("XDG_CONFIG_DIRS");
                for chooser in [
                    "XDG_CURRENT_DESKTOP",
                    "XDG_SESSION_DESKTOP",
                    "DESKTOP_SESSION",
                    "GNOME_DESKTOP_SESSION_ID",
                    "KDE_FULL_SESSION",
                    "KDE_SESSION_VERSION",
                    "DISPLAY",
                    "WAYLAND_DISPLAY",
                ] {
                    command.env_remove(chooser);
                }
                for (name, set) in desktop {
                    command.env(name, set);
                }
                // Generic mode is a run that sets no desktop environment of its own. It must not
                // see the runner's session bus, whose `dbus-send` probe in `detectDE` could name
                // GNOME, and it needs a display variable for `has_display`.
                if !desktop
                    .iter()
                    .any(|(name, _)| *name == "XDG_CURRENT_DESKTOP")
                {
                    command.env_remove("DBUS_SESSION_BUS_ADDRESS");
                    if !desktop.iter().any(|(name, _)| *name == "DISPLAY") {
                        command.env("WAYLAND_DISPLAY", "bravebot-test-no-such-display");
                    }
                }
                command
                    .env("XDG_UTILS_DEBUG_LEVEL", "3")
                    .env(variable, value)
                    .output()
                    .unwrap()
            };
            logs.push(format!(
                "{variable} in {}:\nstdout:\n{}\nstderr:\n{}",
                directory.display(),
                String::from_utf8_lossy(&child.stdout),
                String::from_utf8_lossy(&child.stderr)
            ));
            assert!(
                child.status.success(),
                "{}",
                String::from_utf8_lossy(&child.stderr)
            );
        }
        // The handler is started by `xdg-open` and may be slow to run on a loaded host, so wait
        // for its marker rather than for the opener to return.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !session.join("genuine-ran").exists() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if !session.join("genuine-ran").exists() {
            eprintln!(
                "the handler in the installed directory did not run for {variable}:\n{}",
                logs.join("\n")
            );
        }
        let ran = (
            session.join("planted-ran").exists(),
            session.join("genuine-ran").exists(),
        );
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        ran
    }

    /// A host where the programs are expected to be there and to start a handler: Linux with `CI`
    /// set, where `ci.yml` installs `xdg-utils`, `libglib2.0-bin` and `kde-cli-tools`. A test that
    /// finds a program missing, or a handler that does not start, fails there, since a skip would
    /// report green for an opener that was never run. Everywhere else, macOS included, it prints
    /// `skipped` and passes.
    #[cfg(unix)]
    fn the_runner_must_run_the_handlers() -> bool {
        cfg!(target_os = "linux") && std::env::var_os("CI").is_some()
    }

    /// SANDBOX-30, through the installed `xdg-open` and `gio open` where this host has both. The
    /// regression it rejects: the opener left `XDG_CONFIG_DIRS`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`
    /// or `XDG_DATA_DIRS` as the person set it, so a `mimeapps.list` a stage wrote in the directory it
    /// names chooses the program `gio open` runs. The same directory in an installed place is read
    /// and its handler runs, which shows the variable does choose the handler here. `XDG_DATA_DIRS`
    /// is run with an address whose scheme only the planted and the installed entries handle,
    /// since with the planted value removed `gio open` falls back to the system's handlers and
    /// would start a browser for `https`. It prints `skipped` and passes where either program is
    /// missing or the handler in the installed directory does not run, except on Linux with `CI`
    /// set, where either fails it.
    ///
    /// `BROWSER` is not run here and has no case. `xdg-open` reads it in `open_generic`, which
    /// `open_gnome` reaches only when `gio` is not installed (per the 1.1.x script; not checked
    /// against 1.2.1), and this test needs `gio`. A `BROWSER` helper under GNOME would not
    /// run when it is installed either, so a case for it could not tell a removed value from a kept
    /// one. `BROWSER` is run in generic mode by
    /// [`a_browser_helper_a_stage_may_write_is_not_run_by_the_installed_xdg_open`].
    ///
    /// A logged-in GNOME session adds `GNOME_DESKTOP_SESSION_ID`, a session bus and a display to
    /// what this sets. `xdg-open` reads the first only to choose `open_gnome`, which
    /// `XDG_CURRENT_DESKTOP=GNOME` also chooses, and `gio open` reads its handler from the
    /// directories the four names select. The names the survey in SANDBOX-30 found are the same
    /// with or without a session, so none is expected to be added by one. No session was run.
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
            assert!(
                !the_runner_must_run_the_handlers(),
                "CI is set and xdg-open or gio is not installed"
            );
            println!("skipped: no xdg-open and gio on this host");
            return;
        };
        for variable in [
            "XDG_CONFIG_DIRS",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_DATA_DIRS",
        ] {
            let ran = handler_run_by(
                "opener::tests::an_xdg_directory_a_stage_may_write_is_not_read_by_the_installed_xdg_open_under_gnome",
                variable,
                &[("XDG_CURRENT_DESKTOP", "GNOME")],
                &opener,
            );
            match ran {
                (_, false) => {
                    assert!(
                        !the_runner_must_run_the_handlers(),
                        "{variable}: the xdg-open here did not run the handler in the installed directory"
                    );
                    println!(
                        "skipped {variable}: the xdg-open here does not run a handler chosen by it"
                    );
                }
                ran => assert_eq!(ran, (false, true), "{variable}"),
            }
        }
    }

    /// SANDBOX-30, through the installed `xdg-open` in generic-desktop mode, where it reads the
    /// handler for the address from the directories the four `XDG_` names select and does not hand
    /// the address to `gio open`. The regression it rejects: the opener left `XDG_CONFIG_DIRS`,
    /// `XDG_CONFIG_HOME`, `XDG_DATA_HOME` or `XDG_DATA_DIRS` as the person set it, so a
    /// `mimeapps.list` and a `.desktop` entry a stage wrote in the directory it names choose the
    /// program that opens the address. The same value naming an installed directory still runs its
    /// handler, which shows the variable chooses the handler here and that a safe value opens the
    /// link. It prints `skipped` and passes where there is no `xdg-open`, and for a variable whose
    /// installed handler does not run here, since the planted handler then not running says nothing.
    /// On Linux with `CI` set either fails it.
    ///
    /// The run removes every variable that names a desktop environment and `DBUS_SESSION_BUS_ADDRESS`,
    /// so `xdg-open` takes its generic path. That path asks `xdg-mime` for the handler of `https` only
    /// when a display is set (`has_display` in `xdg-open.in`), so the run sets `DISPLAY` to a number no
    /// X server answers ([`handler_run_by`] says why that starts nothing). It also skips where
    /// `/etc/xdg/mimeapps.list` exists, since a removed `XDG_CONFIG_DIRS` falls back to it and it could
    /// name a real browser.
    #[cfg(unix)]
    #[test]
    fn an_xdg_directory_a_stage_may_write_is_not_read_by_the_installed_xdg_open_in_generic_mode() {
        if is_the_child_of_helper_run_by() {
            return;
        }
        let (Some(opener), Some(_xdg_mime)) = (
            bravebot_sandbox::programs::find(OsStr::new("xdg-open")),
            bravebot_sandbox::programs::find(OsStr::new("xdg-mime")),
        ) else {
            assert!(
                !the_runner_must_run_the_handlers(),
                "CI is set and xdg-open or xdg-mime is not installed"
            );
            println!("skipped: no xdg-open and xdg-mime on this host");
            return;
        };
        if Path::new("/etc/xdg/mimeapps.list").exists() {
            println!("skipped: this host has a system mimeapps.list");
            return;
        }
        for variable in [
            "XDG_CONFIG_DIRS",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_DATA_DIRS",
        ] {
            // `xdg-open` 1.2.1 looks up a scheme handler in generic mode only when `DISPLAY` or
            // `WAYLAND_DISPLAY` is set. `DISPLAY` names a number no X server answers, so `xprop`
            // in `detectDE` cannot reach a real one. The handlers are scripts that connect to nothing.
            let ran = handler_run_by(
                "opener::tests::an_xdg_directory_a_stage_may_write_is_not_read_by_the_installed_xdg_open_in_generic_mode",
                variable,
                &[("DISPLAY", ":9999")],
                &opener,
            );
            match ran {
                (_, false) => {
                    assert!(
                        !the_runner_must_run_the_handlers(),
                        "{variable}: the xdg-open here did not run the handler in the installed directory"
                    );
                    println!(
                        "skipped {variable}: the xdg-open here does not run a handler chosen by it"
                    );
                }
                ran => assert_eq!(ran, (false, true), "{variable}"),
            }
        }
    }

    /// SANDBOX-30, through the installed `xdg-open` under KDE (`XDG_CURRENT_DESKTOP=KDE` and
    /// `KDE_FULL_SESSION=true`), which hands the address to `kde-open` (package `kde-cli-tools`),
    /// or to `kde-open5` when `KDE_SESSION_VERSION` is `5`. The regression it rejects: the opener
    /// left one of the five names as the person set it, so a handler, a `mimeapps.list` or a
    /// `BROWSER` command a stage wrote is run by the program `xdg-open` hands the address to. For
    /// each of `XDG_CONFIG_DIRS`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_DATA_DIRS` a
    /// directory a stage may write is not read, and the same value naming an installed directory
    /// still runs its handler, so a safe value opens the link.
    ///
    /// `BROWSER` has a control of its own, because it is not known whether `kde-open` reads it. The
    /// same two runs are made with a command in a stage-writable directory and one in an installed
    /// directory. If the installed command runs, `kde-open` reads `BROWSER` here and the command in
    /// the stage-writable directory must not run. If the installed command does not run, the test
    /// prints that `kde-open` does not read `BROWSER` and asserts nothing about it, since the
    /// planted command then not running says nothing.
    ///
    /// Where there is no `xdg-open` or no `kde-open` it prints `skipped` and passes, except on Linux
    /// with `CI` set, where it fails, because a runner that lacks them runs nothing. A variable whose
    /// installed handler does not start is skipped, since the planted handler then not running says
    /// nothing. The stand-in test below is the one that fails on CI when the opener's names reach
    /// a started `kde-open`, since this one needs a KDE session bus a headless runner lacks.
    #[cfg(unix)]
    #[test]
    fn a_browser_or_xdg_name_a_stage_may_write_is_not_read_by_the_program_xdg_open_hands_the_address_to_under_kde()
     {
        if is_the_child_of_helper_run_by() {
            return;
        }
        let on_ci = the_runner_must_run_the_handlers();
        let kde_open5 = bravebot_sandbox::programs::find(OsStr::new("kde-open5"));
        let kde_open = kde_open5
            .clone()
            .or_else(|| bravebot_sandbox::programs::find(OsStr::new("kde-open")));
        let (Some(opener), Some(_kde_open)) = (
            bravebot_sandbox::programs::find(OsStr::new("xdg-open")),
            kde_open,
        ) else {
            assert!(
                !on_ci,
                "CI is set and xdg-open or kde-open is not installed"
            );
            println!("skipped: no xdg-open and kde-open on this host");
            return;
        };
        let mut desktop = vec![("XDG_CURRENT_DESKTOP", "KDE"), ("KDE_FULL_SESSION", "true")];
        // `xdg-open` 1.2.1 runs `kfmclient exec` when `KDE_SESSION_VERSION` is unset, `kde-open5`
        // for 5 and `kde-open` for 4 and 6.
        desktop.push((
            "KDE_SESSION_VERSION",
            if kde_open5.is_some() { "5" } else { "6" },
        ));
        for variable in [
            "BROWSER",
            "XDG_CONFIG_DIRS",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_DATA_DIRS",
        ] {
            let ran = handler_run_by(
                "opener::tests::a_browser_or_xdg_name_a_stage_may_write_is_not_read_by_the_program_xdg_open_hands_the_address_to_under_kde",
                variable,
                &desktop,
                &opener,
            );
            if variable == "BROWSER" {
                if ran.1 {
                    assert!(!ran.0, "BROWSER: a command a stage may write was run");
                } else {
                    println!(
                        "BROWSER: kde-open here does not run an installed BROWSER, so it does not read it; nothing asserted"
                    );
                }
                continue;
            }
            match ran {
                (_, false) => {
                    // `kde-open` may need a session bus that a headless runner lacks, and the
                    // planted handler then not running says nothing.
                    println!(
                        "skipped {variable}: kde-open here does not run a handler chosen by it"
                    );
                }
                ran => {
                    assert_eq!(ran, (false, true), "{variable}");
                }
            }
        }
    }

    /// SANDBOX-30, `XDG_DATA_DIRS` with no desktop environment named (`XDG_CURRENT_DESKTOP` and the
    /// other variables that choose one are removed, so `xdg-open` takes its generic path). The
    /// regression it rejects: the opener left `XDG_DATA_DIRS` as the person set it, so the `.desktop`
    /// entries in a directory a stage wrote choose the program `xdg-open` and `xdg-mime` run.
    ///
    /// The installed `xdg-open` cannot show this on every host (`XDG_DATA_DIRS` removed makes `gio`
    /// fall back to the system's handlers, which would start a real browser, and macOS has no
    /// `xdg-open`). The opener's launch path runs a stand-in program instead, in a child run of
    /// this test binary that holds the planted directory in `XDG_DATA_DIRS`, as the other tests
    /// here do. The stand-in appends the `XDG_DATA_DIRS` it receives to a log and starts nothing.
    /// A control run of the same stand-in that does not go through the opener must log the planted
    /// value, which shows the log can hold it.
    #[cfg(unix)]
    #[test]
    fn xdg_data_dirs_a_stage_may_write_is_not_passed_by_the_opener_in_generic_mode() {
        use std::os::unix::fs::PermissionsExt;

        if is_the_child_of_helper_run_by() {
            return;
        }
        let session = crate::testutil::scratch_dir("opener-data-dirs-session");
        let installed = crate::testutil::scratch_dir("opener-data-dirs-installed");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        let planted = session.join("planted-data-dirs-marker");
        std::fs::create_dir_all(&planted).unwrap();
        std::fs::create_dir_all(&installed).unwrap();
        let log = installed.join("seen.log");
        let stand_in = installed.join("xdg-open");
        std::fs::write(
            &stand_in,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"${{XDG_DATA_DIRS-unset}}\" >> '{}'\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755)).unwrap();
        let without_a_desktop = |command: &mut std::process::Command| {
            command
                .env("XDG_DATA_DIRS", &planted)
                .env_remove("XDG_CURRENT_DESKTOP")
                .env_remove("XDG_SESSION_DESKTOP")
                .env_remove("DESKTOP_SESSION")
                .env_remove("GNOME_DESKTOP_SESSION_ID")
                .env_remove("KDE_FULL_SESSION")
                .env_remove("DISPLAY")
                .env_remove("WAYLAND_DISPLAY")
                .env_remove("BROWSER");
        };

        let mut control = std::process::Command::new(&stand_in);
        without_a_desktop(&mut control);
        assert!(control.status().unwrap().success());

        // nosemgrep: rust.lang.security.current-exe.current-exe
        let mut through_the_opener = std::process::Command::new(std::env::current_exe().unwrap());
        through_the_opener
            .args([
                "--exact",
                "opener::tests::xdg_data_dirs_a_stage_may_write_is_not_passed_by_the_opener_in_generic_mode",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("BRAVEBOT_TEST_BROWSER_SESSION", &session)
            .env("BRAVEBOT_TEST_BROWSER_OPENER", &stand_in);
        without_a_desktop(&mut through_the_opener);
        let child = through_the_opener.output().unwrap();

        let seen = std::fs::read_to_string(&log).unwrap_or_default();
        let report = String::from_utf8_lossy(&child.stderr).into_owned();
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        assert!(child.status.success(), "{report}");
        let lines: Vec<&str> = seen.lines().collect();
        assert_eq!(lines.len(), 2, "the stand-in ran once per start: {seen:?}");
        assert_eq!(
            lines[0],
            planted.display().to_string(),
            "the control did not receive the planted directory"
        );
        assert!(
            !lines[1].contains("planted-data-dirs-marker"),
            "the opener passed on a directory a stage may write: {}",
            lines[1]
        );
    }

    /// SANDBOX-30, through the installed `xdg-open` and `gio open` where this host has both, with
    /// every `XDG_` directory unset so that `HOME` alone chooses the handler. The regression it
    /// rejects: the opener left `HOME` as the person set it, so a `mimeapps.list` and a `.desktop`
    /// entry a stage wrote under a `HOME` it may write choose the program `gio open` runs. The same
    /// layout under an installed `HOME` is read and its handler runs, which shows `HOME` does choose
    /// the handler here. It prints `skipped` and passes where either program is missing, where the
    /// handler in the installed `HOME` does not run, and where the person running it has handlers of
    /// their own under their `HOME`, since `gio open` falls back to that directory when `HOME` is
    /// removed and would start their browser.
    #[cfg(unix)]
    #[test]
    fn a_home_a_stage_may_write_is_not_read_by_the_installed_xdg_open_under_gnome() {
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
        if std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .is_some_and(|own| {
                own.join(".config/mimeapps.list").exists()
                    || own.join(".local/share/applications").exists()
            })
        {
            println!("skipped: this account has handlers of its own under its home");
            return;
        }
        let ran = handler_run_by(
            "opener::tests::a_home_a_stage_may_write_is_not_read_by_the_installed_xdg_open_under_gnome",
            "HOME",
            &[("XDG_CURRENT_DESKTOP", "GNOME")],
            &opener,
        );
        match ran {
            (_, false) => {
                println!("skipped: the xdg-open here does not run a handler chosen by HOME")
            }
            ran => assert_eq!(ran, (false, true)),
        }
    }

    /// The stand-in `kde-open` that [`kde_open_saw`] places first on `PATH`: it writes the arguments
    /// it was started with and each variable of the opener's list it can see to `@LOG@`, and starts
    /// nothing. The names are those of [`CODE_SELECTORS`], so a name added to it is looked for.
    #[cfg(unix)]
    fn kde_open_stand_in(log: &Path) -> String {
        let names: Vec<&str> = CODE_SELECTORS.iter().map(|(name, _)| *name).collect();
        format!(
            "#!/bin/sh\n{{\necho \"ARGS $*\"\nfor name in {}; do\n  if printenv \"$name\" >/dev/null 2>&1; then echo \"SET $name=$(printenv \"$name\")\"; fi\ndone\n}} > '{}'\n",
            names.join(" "),
            log.display()
        )
    }

    /// What a stand-in `kde-open`, started by the installed `xdg-open` that the opener starts under
    /// `XDG_CURRENT_DESKTOP=KDE`, was given: the lines it logged, or `None` when it did not run.
    ///
    /// `variable` is set in the environment of a child run of this test binary (the command takes its
    /// environment from the process, so a test cannot set one) to a path in a directory a stage may
    /// write when `planted`, and to one in an installed directory otherwise. The child keeps the
    /// session directory out as a stage's, runs the opener against `opener`, and every other name
    /// of the opener's list is removed from its environment. `KDE_SESSION_VERSION=6` makes
    /// `xdg-open`'s `open_kde` start `kde-open` (`xdg-open.in`), which is found by name on `PATH`,
    /// and the stand-in is first on it.
    #[cfg(unix)]
    fn kde_open_saw(test: &str, variable: &str, opener: &Path, planted: bool) -> Option<String> {
        use std::os::unix::fs::PermissionsExt;

        let which = if planted { "planted" } else { "installed" };
        let name = variable.to_ascii_lowercase();
        let session = crate::testutil::scratch_dir(&format!("opener-kde-{name}-{which}-session"));
        let installed =
            crate::testutil::scratch_dir(&format!("opener-kde-{name}-{which}-installed"));
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        std::fs::create_dir_all(&session).unwrap();
        std::fs::create_dir_all(&installed).unwrap();
        let log = installed.join("seen.log");
        let executable = |path: &Path, body: &str| {
            std::fs::write(path, body).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        executable(&installed.join("kde-open"), &kde_open_stand_in(&log));
        let place = if planted { &session } else { &installed };
        let value = match variable {
            "BROWSER" => {
                executable(&place.join("browser.sh"), "#!/bin/sh\n");
                format!("{} %s", place.join("browser.sh").display())
            }
            "GIO_LAUNCH_DESKTOP" => {
                executable(&place.join("gio-launch-desktop"), "#!/bin/sh\n");
                place.join("gio-launch-desktop").display().to_string()
            }
            "KDESYCOCA" => {
                std::fs::write(place.join("sycoca"), "").unwrap();
                place.join("sycoca").display().to_string()
            }
            _ => {
                std::fs::create_dir_all(place.join("selected")).unwrap();
                place.join("selected").display().to_string()
            }
        };
        let mut path = vec![installed.clone()];
        path.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        // nosemgrep: rust.lang.security.current-exe.current-exe
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", test, "--test-threads=1", "--nocapture"])
            .env("BRAVEBOT_TEST_BROWSER_SESSION", &session)
            .env("BRAVEBOT_TEST_BROWSER_OPENER", opener)
            .env("PATH", std::env::join_paths(path).unwrap())
            .env("XDG_CURRENT_DESKTOP", "KDE")
            .env("KDE_SESSION_VERSION", "6")
            .env_remove("XDG_SESSION_DESKTOP")
            .env_remove("DESKTOP_SESSION")
            .env_remove("GNOME_DESKTOP_SESSION_ID")
            .env_remove("KDE_FULL_SESSION")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY");
        for (listed, _) in CODE_SELECTORS {
            command.env_remove(listed);
        }
        command.env(variable, &value);
        let child = command.output().unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        let seen = std::fs::read_to_string(&log).ok();
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        seen
    }

    /// SANDBOX-30, KDE mode, through the installed `xdg-open` where this host has one and a stand-in
    /// `kde-open` first on `PATH`, for every name in the opener's list. The opener's real launch
    /// path starts `xdg-open` with `XDG_CURRENT_DESKTOP=KDE`, which starts the stand-in with the
    /// address. With the name set to a path a stage may write, the stand-in is still started with
    /// the address and does not see the name. With it set to a path in an installed directory the
    /// stand-in sees it, which shows the log can hold the name and that a safe value still opens the
    /// address. A name that [`Names::Options`] judges is removed with either value and the address
    /// is still opened. The regression it rejects: a name left out of the opener's list or kept
    /// whatever it holds, so the real `kde-open` would read it from the environment the opener
    /// passed.
    ///
    /// The stand-in shows what the opener hands on, not what `kde-open` reads: that is from the source
    /// ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)). It prints `skipped` and passes
    /// where there is no `xdg-open`, as on macOS, and where the installed one does not start the
    /// stand-in. On Linux with `CI` set it fails instead, since the runner has `xdg-open`.
    #[cfg(unix)]
    #[test]
    fn a_name_a_stage_may_write_is_not_passed_to_the_kde_open_the_installed_xdg_open_starts() {
        if is_the_child_of_helper_run_by() {
            return;
        }
        let Some(opener) = bravebot_sandbox::programs::find(OsStr::new("xdg-open")) else {
            assert!(
                !the_runner_must_run_the_handlers(),
                "CI is set and xdg-open is not installed"
            );
            println!("skipped: no xdg-open on this host");
            return;
        };
        let test = "opener::tests::a_name_a_stage_may_write_is_not_passed_to_the_kde_open_the_installed_xdg_open_starts";
        for (variable, names) in CODE_SELECTORS {
            let Some(installed) = kde_open_saw(test, variable, &opener, false) else {
                assert!(
                    !the_runner_must_run_the_handlers(),
                    "{variable}: CI is set and the xdg-open here did not start the kde-open"
                );
                println!("skipped {variable}: the xdg-open here did not start the kde-open");
                continue;
            };
            let sets = |log: &str| {
                log.lines()
                    .filter_map(|line| line.strip_prefix("SET "))
                    .filter_map(|line| line.split_once('=').map(|(name, _)| name.to_string()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                installed.lines().next(),
                Some("ARGS https://example.invalid/"),
                "{variable}: {installed}"
            );
            // An options name is removed whatever it holds, so the safe value is not kept either.
            if names == Names::Options {
                assert!(
                    sets(&installed).is_empty(),
                    "{variable} reached kde-open: {installed}"
                );
            } else {
                assert_eq!(sets(&installed), [variable], "{variable} was not kept");
            }
            let planted = kde_open_saw(test, variable, &opener, true)
                .unwrap_or_else(|| panic!("{variable}: the address was not opened"));
            assert_eq!(
                planted.lines().next(),
                Some("ARGS https://example.invalid/"),
                "{variable}: {planted}"
            );
            assert!(
                sets(&planted).is_empty(),
                "{variable} reached kde-open: {planted}"
            );
        }
    }

    /// CHATGPT-13, the Windows start, checked on every host. The command built for it is
    /// `rundll32`, the library's entry point and the address, as exactly those arguments: an address
    /// holding `&`, `|`, `^`, `%` and `"` is neither split nor joined with another, and no shell is
    /// between it and the program. The regression it rejects: the Windows address joined into one
    /// string with the entry point, or passed to `cmd /c start`, which reads `&`. That `rundll32`
    /// then opens a page was read and not run, which needs Windows.
    #[test]
    fn the_windows_launch_is_rundll32_with_the_entry_point_and_the_address_as_two_arguments() {
        let url = "https://auth.example/a?x=1&y=2|3^4%20\"5";
        assert!(is_openable(url));

        let (program, arguments) = opener("windows", url);
        let command = opening_against(
            &Resolved::found_earlier(r"C:\Windows\System32\rundll32.exe"),
            &arguments,
            [],
            &|_| true,
        );

        assert_eq!(program, "rundll32");
        assert_eq!(
            command.get_program(),
            OsStr::new(r"C:\Windows\System32\rundll32.exe")
        );
        let given: Vec<&OsStr> = command.get_args().collect();
        assert_eq!(
            given,
            [OsStr::new("url.dll,FileProtocolHandler"), OsStr::new(url)]
        );
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

        assert!(withheld("HOME", "/stage/home"));
        assert!(withheld("HOME", "home"));
        assert!(withheld("HOME", ""));
        assert!(!withheld("HOME", "/safe/home"));
        assert!(!withheld("HOME", "/safe/home/"));
        assert!(withheld("HOME", "/safe-not/home"));
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
            "XDG_RUNTIME_DIR",
            "DISPLAY",
            "LANG",
            "PATH",
            "GSETTINGS_SCHEMA_DIR",
            "GIO_USE_VFS",
            "QT_QPA_PLATFORM_PLUGIN_PATHS",
        ] {
            assert!(!withheld(name, "/stage/anything"), "{name} was removed");
        }
        assert!(!withheld("BROWSER", "/safe/bin/firefox"));
        assert!(!withheld("XDG_DATA_DIRS", "/safe/share"));
        assert!(!withheld("XDG_CONFIG_DIRS", "/safe/etc/xdg"));
        assert!(!withheld("HOME", "/safe/home"));
    }

    /// SANDBOX-30, on the disk: `HOME` is removed when it, or the configuration or handler
    /// directory `xdg-open` reads under it, leads to a directory a stage may write, and kept
    /// otherwise. The regression it rejects: `HOME` judged alone, so a session opened on
    /// `~/.config` or on `~/.local/share/applications` (which has no row for `HOME`) leaves `HOME`
    /// as set while the files read under it are the stage's; and a comparison of the names as
    /// written, which a link into the session slips past.
    #[cfg(unix)]
    #[test]
    fn a_home_is_judged_with_the_paths_the_opener_reads_under_it_on_the_disk() {
        let session = crate::testutil::scratch_dir("opener-home-session");
        let safe = crate::testutil::scratch_dir("opener-home-safe");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
        let planted = session.join("planted");
        std::fs::create_dir_all(&planted).unwrap();
        let make = |name: &str| {
            let home = safe.join(name);
            std::fs::create_dir_all(&home).unwrap();
            home
        };
        let genuine = make("genuine");
        std::fs::create_dir_all(genuine.join(".config")).unwrap();
        std::fs::create_dir_all(genuine.join(".local/share/applications")).unwrap();
        let bare = make("bare");
        let linked_config = make("linked-config");
        std::os::unix::fs::symlink(&planted, linked_config.join(".config")).unwrap();
        let linked_applications = make("linked-applications");
        std::fs::create_dir_all(linked_applications.join(".local/share")).unwrap();
        std::os::unix::fs::symlink(
            &planted,
            linked_applications.join(".local/share/applications"),
        )
        .unwrap();
        let linked_share = make("linked-share");
        std::fs::create_dir_all(linked_share.join(".local")).unwrap();
        std::os::unix::fs::symlink(&planted, linked_share.join(".local/share")).unwrap();
        let linked_home = safe.join("to-planted");
        std::os::unix::fs::symlink(&planted, &linked_home).unwrap();
        let writable = vec![session.clone()];
        let outside = |path: &Path| {
            bravebot_sandbox::programs::outside_stage_writable_with(path, &writable, &[])
        };
        let removed = |value: &Path| {
            opening_against(
                &Resolved::found_earlier("/usr/bin/open"),
                &[],
                [(OsString::from("HOME"), value.as_os_str().to_os_string())],
                &outside,
            )
            .get_envs()
            .any(|(set, now)| set == OsStr::new("HOME") && now.is_none())
        };

        assert!(!removed(&genuine));
        assert!(!removed(&bare));
        assert!(removed(&planted));
        assert!(removed(&linked_home));
        assert!(removed(&linked_config));
        assert!(removed(&linked_applications));
        assert!(removed(&linked_share));
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
    }

    /// SANDBOX-30, `gio open` and `kde-open`. The regression it rejects: the opener's list left at
    /// the XDG names, so a module directory, a launcher, a plug-in directory, a plug-in name or a
    /// service cache a stage wrote is loaded or read by the program the opener starts. A name whose
    /// value is a path has a value a stage may write, which is removed, and a safe one, which is
    /// kept; an option is removed whatever it holds. `GIO_LAUNCH_DESKTOP` is one file, so what
    /// it keeps is shown on the disk by a test of its own.
    #[test]
    fn the_opener_removes_a_glib_or_qt_name_that_selects_code_a_stage_may_write() {
        assert!(withheld("HOME", "/stage/home"));
        assert!(withheld("HOME", "home"));
        assert!(!withheld("HOME", "/safe/home"));

        assert!(withheld("GIO_EXTRA_MODULES", "/stage/modules"));
        assert!(withheld(
            "GIO_EXTRA_MODULES",
            "/safe/modules:/stage/modules"
        ));
        assert!(!withheld("GIO_EXTRA_MODULES", "/safe/modules:/safe/more"));

        assert!(withheld("GIO_MODULE_DIR", "/stage/modules"));
        assert!(!withheld("GIO_MODULE_DIR", "/safe/modules"));

        assert!(withheld("GIO_LAUNCH_DESKTOP", "/stage/launcher"));

        assert!(withheld("QT_PLUGIN_PATH", "/stage/plugins"));
        assert!(withheld("QT_PLUGIN_PATH", "/safe/plugins:/stage/plugins"));
        assert!(!withheld("QT_PLUGIN_PATH", "/safe/plugins:/safe/more"));

        assert!(withheld("QT_QPA_PLATFORM_PLUGIN_PATH", "/stage/platforms"));
        assert!(!withheld("QT_QPA_PLATFORM_PLUGIN_PATH", "/safe/platforms"));

        assert!(withheld("QT_QPA_PLATFORMTHEME", "evil"));
        assert!(withheld("QT_QPA_PLATFORMTHEME", "/safe/anything"));
        assert!(withheld("QT_QPA_PLATFORMTHEME", ""));
        assert!(withheld("QT_QPA_GENERIC_PLUGINS", "evil"));
        assert!(withheld("QT_QPA_GENERIC_PLUGINS", "/safe/anything"));
        assert!(withheld("QT_QPA_GENERIC_PLUGINS", ""));
        assert!(withheld("QT_QPA_PLATFORM", "evil"));
        assert!(withheld("QT_QPA_PLATFORM", "/safe/anything"));
        assert!(withheld("QT_QPA_PLATFORM", ""));
        assert!(withheld("QT_STYLE_OVERRIDE", "evil"));
        assert!(withheld("QT_STYLE_OVERRIDE", "/safe/anything"));
        assert!(withheld("QT_STYLE_OVERRIDE", ""));
    }

    /// SANDBOX-30, `kde-open`: `KDESYCOCA` names a file, so it is removed when that file is one a
    /// stage may write and kept when it is an existing file outside them. The regression it rejects:
    /// `KDESYCOCA` judged as a directory or as an option, or not listed. Judged on the disk, since a
    /// file has to exist to be kept.
    #[cfg(unix)]
    #[test]
    fn the_opener_judges_a_kde_service_cache_by_the_file_it_names() {
        let session = crate::testutil::scratch_dir("opener-sycoca-session");
        let safe = crate::testutil::scratch_dir("opener-sycoca-safe");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
        std::fs::create_dir_all(&session).unwrap();
        std::fs::create_dir_all(&safe).unwrap();
        let planted = session.join("ksycoca6");
        let genuine = safe.join("ksycoca6");
        std::fs::write(&planted, "").unwrap();
        std::fs::write(&genuine, "").unwrap();
        let session = session.canonicalize().unwrap();
        let safe = safe.canonicalize().unwrap();

        // Whether the start takes `name` off when it holds `value`, with every file under the
        // session one a stage may write.
        let checked = |name: &str, value: &Path| {
            let command = opening_against(
                &Resolved::found_earlier("/usr/bin/open"),
                &[],
                [(OsString::from(name), value.as_os_str().to_os_string())],
                &|path: &Path| !path.starts_with(&session),
            );
            command
                .get_envs()
                .any(|(set, now)| set == OsStr::new(name) && now.is_none())
        };

        assert!(checked("KDESYCOCA", &session.join("ksycoca6")));
        assert!(checked("KDESYCOCA", &session.join("not-yet")));
        assert!(checked("KDESYCOCA", Path::new("ksycoca6")));
        assert!(!checked("KDESYCOCA", &safe.join("ksycoca6")));
        assert!(checked("KDESYCOCA", &safe));

        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
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
        let removed_for = |name: &str, value: &OsStr| {
            opening_against(
                &Resolved::found_earlier("/usr/bin/open"),
                &[],
                [(OsString::from(name), value.to_os_string())],
                &outside,
            )
            .get_envs()
            .any(|(set, now)| set == OsStr::new(name) && now.is_none())
        };
        let removed = |value: &Path| removed_for("XDG_DATA_HOME", value.as_os_str());

        assert!(removed(&planted));
        assert!(removed(&link));
        assert!(removed(&safe.join("missing")));
        assert!(!removed(&genuine));

        // A list is judged entry by entry. The regression it rejects: a list judged as one path,
        // which names no directory on the disk, so a person's list of installed directories is
        // removed.
        let genuine_list = std::env::join_paths([&genuine, &safe]).unwrap();
        let mixed_list = std::env::join_paths([&genuine, &link]).unwrap();
        for name in ["XDG_DATA_DIRS", "XDG_CONFIG_DIRS"] {
            assert!(!removed_for(name, &genuine_list), "{name}: a genuine list");
            assert!(removed_for(name, &mixed_list), "{name}: a list with a link");
        }
    }

    /// SANDBOX-30, on the disk: `GIO_LAUNCH_DESKTOP` names the wrapper `gio open` executes with the
    /// handler's command line, so it is removed when it is a file in a directory a stage may write,
    /// a link into one, a directory, a path that does not exist or a bare name, and kept when it is
    /// an existing file outside every such directory. The regression it rejects: the name left out
    /// of the opener's list, so a wrapper a stage wrote runs the handler outside the sandbox; and a
    /// judgment of the name as written, which a link into the session slips past.
    #[cfg(unix)]
    #[test]
    fn gio_launch_desktop_is_removed_for_a_wrapper_a_stage_may_write_and_kept_for_an_installed_one()
    {
        let session = crate::testutil::scratch_dir("opener-gio-session");
        let safe = crate::testutil::scratch_dir("opener-gio-safe");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
        std::fs::create_dir_all(&session).unwrap();
        std::fs::create_dir_all(&safe).unwrap();
        let planted = session.join("gio-launch-desktop");
        let genuine = safe.join("gio-launch-desktop");
        std::fs::write(&planted, "#!/bin/sh\n").unwrap();
        std::fs::write(&genuine, "#!/bin/sh\n").unwrap();
        let link = safe.join("to-planted");
        std::os::unix::fs::symlink(&planted, &link).unwrap();
        let writable = vec![session.clone()];
        let outside = |path: &Path| {
            bravebot_sandbox::programs::outside_stage_writable_with(path, &writable, &[])
        };
        // Shadows the helper above, which judges by the name `/safe` and cannot see a file on disk.
        let withheld = |name: &str, value: &Path| {
            opening_against(
                &Resolved::found_earlier("/usr/bin/open"),
                &[],
                [(OsString::from(name), value.as_os_str().to_os_string())],
                &outside,
            )
            .get_envs()
            .any(|(set, now)| set == OsStr::new(name) && now.is_none())
        };

        assert!(withheld("GIO_LAUNCH_DESKTOP", &planted));
        assert!(withheld("GIO_LAUNCH_DESKTOP", &link));
        assert!(withheld("GIO_LAUNCH_DESKTOP", &safe));
        assert!(withheld("GIO_LAUNCH_DESKTOP", &safe.join("missing")));
        assert!(withheld(
            "GIO_LAUNCH_DESKTOP",
            Path::new("gio-launch-desktop")
        ));
        assert!(!withheld("GIO_LAUNCH_DESKTOP", &genuine));
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
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
