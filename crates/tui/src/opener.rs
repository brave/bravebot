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
/// removed whatever they hold. `KDESYCOCA` is the service cache `kde-open` reads `Exec` lines from,
/// and `XDG_CACHE_HOME` is the directory that cache is looked up in when `KDESYCOCA` is not set
/// (KService `ksycoca.cpp` lines 693-709 and Qt `qstandardpaths_unix.cpp` lines 185-200, KDE
/// Frameworks 6.29.0 and Qt 6.10.0). Qt ignores a relative `XDG_CACHE_HOME`, so a relative value
/// is removed here as it is for the other directories.
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
///
/// `gio open` connects to the session bus, which it finds from `DBUS_SESSION_BUS_ADDRESS` and
/// otherwise from `XDG_RUNTIME_DIR/bus`. A service a stage runs on a socket it can create answers
/// as the bus, and what it answers to the calls that choose or start a handler was not shown
/// to be harmless, so both names are judged by where the socket is. `XDG_RUNTIME_DIR` is one
/// directory. `DBUS_SESSION_BUS_ADDRESS` is [`Names::BusAddress`]: kept only when every entry is
/// `unix:path=` with an absolute path outside every directory a stage may write, or
/// `unix:runtime=yes`. A path is judged where it leads on the disk, a path that does not exist is
/// removed, and the file at it is not required to be a socket.
///
/// The last ten names are those of the Windows start, `rundll32 url.dll,FileProtocolHandler`
/// (brave/bravebot#3860). The sources are Wine's and ReactOS's source and Microsoft's
/// `ShellExecute` page. No run on a Windows host shows that a name chooses the program. Wine's
/// `url.dll` passes the address to `ShellExecuteA` with no verb, and its `shlexec.c` reads
/// `%name%` in the command template of the handler it runs from the process's environment
/// (`GetEnvironmentVariableW`), so a handler whose command is written with `%ProgramFiles%` or
/// `%LOCALAPPDATA%` runs whatever those variables name. The directory names listed are those
/// handlers and profiles are written under: `SystemRoot`, `windir`, `ProgramFiles`,
/// `ProgramW6432`, `ProgramFiles(x86)`, `LOCALAPPDATA`, `APPDATA` and `USERPROFILE`, each one
/// directory ([`Names::Directory`]). Wine's `kernelbase/process.c` starts a batch file through the program
/// `COMSPEC` names, so it is one file ([`Names::File`]). `PATHEXT` is the list of extensions a
/// command processor tries for a bare name; none of those sources reads it for this start, and it
/// is removed whatever it holds ([`Names::Options`]). Whether the start needs it was not run.
/// `PSModulePath` is not listed: nothing read shows `rundll32` or `ShellExecute` reading it, and the
/// opener does not start PowerShell. The Windows run test sets it on a control start and records
/// whether it reaches the helper.
const CODE_SELECTORS: [(&str, Names); 29] = [
    selectors::BROWSER,
    ("HOME", Names::Directory),
    ("XDG_DATA_HOME", Names::Directory),
    ("XDG_DATA_DIRS", Names::Directories),
    ("XDG_CONFIG_HOME", Names::Directory),
    ("XDG_CONFIG_DIRS", Names::Directories),
    ("XDG_RUNTIME_DIR", Names::Directory),
    ("DBUS_SESSION_BUS_ADDRESS", Names::BusAddress),
    ("XDG_CACHE_HOME", Names::Directory),
    selectors::GIO_EXTRA_MODULES,
    selectors::GIO_MODULE_DIR,
    ("GIO_LAUNCH_DESKTOP", Names::File),
    selectors::QT_PLUGIN_PATH,
    ("QT_QPA_PLATFORM_PLUGIN_PATH", Names::Directory),
    ("QT_QPA_PLATFORMTHEME", Names::Options),
    ("QT_QPA_GENERIC_PLUGINS", Names::Options),
    ("QT_QPA_PLATFORM", Names::Options),
    ("QT_STYLE_OVERRIDE", Names::Options),
    ("KDESYCOCA", Names::File),
    ("PATHEXT", Names::Options),
    ("COMSPEC", Names::File),
    ("SystemRoot", Names::Directory),
    ("windir", Names::Directory),
    ("ProgramFiles", Names::Directory),
    ("ProgramW6432", Names::Directory),
    ("ProgramFiles(x86)", Names::Directory),
    ("LOCALAPPDATA", Names::Directory),
    ("APPDATA", Names::Directory),
    ("USERPROFILE", Names::Directory),
];

/// The names `xdg-mime` 1.2.1 joins to a directory it has already judged, as the start of a file
/// name: `XDG_CURRENT_DESKTOP` (each `:` entry, lowercased, then `-mimeapps.list`, in
/// `check_mimeapps_list`) and `XDG_MENU_PREFIX` (then `defaults.list` and `mimeinfo.cache`, in
/// `defapp_fallback`). Neither strips a `/`, so a value with one leads out of that directory to a
/// `mimeapps.list`, `defaults.list` or `mimeinfo.cache` a stage wrote, which names a `.desktop` file
/// whose `Exec` line `xdg-open` runs. Run against the real script, `../../stage/evil` and
/// `../../../stage/` each ran the planted handler. A value without a `/` stays inside the directory,
/// so it is kept: `XDG_CURRENT_DESKTOP` also tells `xdg-open` which desktop it is on.
const FILE_NAME_PREFIXES: [&str; 2] = ["XDG_CURRENT_DESKTOP", "XDG_MENU_PREFIX"];

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
    let prefixes_with_a_slash: Vec<&str> = FILE_NAME_PREFIXES
        .into_iter()
        .filter(|prefix| {
            held.iter().any(|(name, value)| {
                name == OsStr::new(prefix) && value.as_encoded_bytes().contains(&b'/')
            })
        })
        .collect();
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
    for prefix in prefixes_with_a_slash {
        command.env_remove(prefix);
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
    ///
    /// `desktop` is set after those are removed, so a test can name a desktop environment. The
    /// installed directory is first on the child's `PATH`, so a program `opener` writes there is
    /// found by name, and `DBUS_SESSION_BUS_ADDRESS` is removed so the `dbus-send` probe in
    /// `detectDE` cannot name GNOME for a run that names no desktop.
    #[cfg(unix)]
    fn helper_run_by(
        test: &str,
        name: &str,
        desktop: &[(&str, &str)],
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
        let mut path = vec![installed.clone()];
        path.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(path).unwrap();
        let ran = program.map(|program| {
            for helper in [&planted, &genuine] {
                // nosemgrep: rust.lang.security.current-exe.current-exe
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                command
                    .args(["--exact", test, "--test-threads=1", "--nocapture"])
                    .env("BRAVEBOT_TEST_BROWSER_SESSION", &session)
                    .env("BRAVEBOT_TEST_BROWSER_OPENER", &program)
                    .env("BROWSER", format!("{} %s", helper.display()))
                    .env("PATH", &path)
                    .env_remove("XDG_CURRENT_DESKTOP")
                    .env_remove("XDG_SESSION_DESKTOP")
                    .env_remove("DESKTOP_SESSION")
                    .env_remove("GNOME_DESKTOP_SESSION_ID")
                    .env_remove("KDE_FULL_SESSION")
                    .env_remove("DISPLAY")
                    .env_remove("WAYLAND_DISPLAY")
                    .env_remove("DBUS_SESSION_BUS_ADDRESS");
                for (name, set) in desktop {
                    command.env(name, set);
                }
                let child = command.output().unwrap();
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
            &[],
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
    /// helper then not running says nothing. On Linux with `CI` set, where `ci.yml` installs
    /// `xdg-utils`, either case fails instead, so a pass there means the real `xdg-open` ran.
    #[cfg(unix)]
    #[test]
    fn a_browser_helper_a_stage_may_write_is_not_run_by_the_installed_xdg_open() {
        if is_the_child_of_helper_run_by() {
            return;
        }
        let ran = helper_run_by(
            "opener::tests::a_browser_helper_a_stage_may_write_is_not_run_by_the_installed_xdg_open",
            "installed-xdg-open",
            &[],
            |_| bravebot_sandbox::programs::find(OsStr::new("xdg-open")),
        );
        match ran {
            None => {
                assert!(
                    !the_runner_must_run_the_handlers(),
                    "CI is set and xdg-open is not installed"
                );
                println!("skipped: no xdg-open on this host");
            }
            Some((_, false)) => {
                assert!(
                    !the_runner_must_run_the_handlers(),
                    "CI is set and the xdg-open here did not run a BROWSER that is installed"
                );
                println!("skipped: the xdg-open here does not run a BROWSER that is installed");
            }
            Some(ran) => assert_eq!(ran, (false, true)),
        }
    }

    /// SANDBOX-30, `BROWSER` under GNOME, through the installed `xdg-open` with `gio`, `gvfs-open`
    /// and `gnome-open` shadowed by programs that fail. In `xdg-open` 1.2.1 `open_gnome` and
    /// `open_gnome3` run `gio open`, then `gvfs-open` (and for `open_gnome` `gnome-open`), and call
    /// `open_generic` only when none of them answers `--help` (lines 645 to 676 of the script), and
    /// `open_generic` is where `BROWSER` is read (`open_envvar`). With `gio` installed, as on every
    /// GNOME session, `BROWSER` is never read, so the case that can show the opener's removal of
    /// `BROWSER` mattering under `XDG_CURRENT_DESKTOP=GNOME` is the one where it is reached. The
    /// same two runs as the generic test: a helper in a stage-writable directory is not run and one
    /// in an installed directory is.
    ///
    /// It prints `skipped` and passes where there is no `xdg-open`, or the one here does not run an
    /// installed `BROWSER` helper under `XDG_CURRENT_DESKTOP=GNOME`, except on Linux with `CI` set,
    /// where either fails. It does not show what a real GNOME session adds.
    #[cfg(unix)]
    #[test]
    fn a_browser_helper_a_stage_may_write_is_not_run_by_the_installed_xdg_open_under_gnome_without_gio()
     {
        if is_the_child_of_helper_run_by() {
            return;
        }
        let ran = helper_run_by(
            "opener::tests::a_browser_helper_a_stage_may_write_is_not_run_by_the_installed_xdg_open_under_gnome_without_gio",
            "installed-xdg-open-gnome",
            &[("XDG_CURRENT_DESKTOP", "GNOME")],
            |installed| {
                use std::os::unix::fs::PermissionsExt;
                let opener = bravebot_sandbox::programs::find(OsStr::new("xdg-open"))?;
                for shadow in ["gio", "gvfs-open", "gnome-open"] {
                    let path = installed.join(shadow);
                    std::fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                        .unwrap();
                }
                Some(opener)
            },
        );
        match ran {
            None => {
                assert!(
                    !the_runner_must_run_the_handlers(),
                    "CI is set and xdg-open is not installed"
                );
                println!("skipped: no xdg-open on this host");
            }
            Some((_, false)) => {
                assert!(
                    !the_runner_must_run_the_handlers(),
                    "CI is set and the xdg-open here did not run a BROWSER that is installed under GNOME without gio"
                );
                println!(
                    "skipped: the xdg-open here does not run a BROWSER that is installed under GNOME without gio"
                );
            }
            Some(ran) => assert_eq!(ran, (false, true)),
        }
    }

    /// SANDBOX-30, `BROWSER` under KDE, through the installed `xdg-open` with `kde-open`, `kde-open5`
    /// and `kfmclient` shadowed by programs that fail. In `xdg-open` 1.2.1 `open_kde` (script lines
    /// 609 to 633) runs one of those and never calls `open_generic`, so the script itself never
    /// reads `BROWSER` under KDE: neither a helper in a stage-writable directory nor one in an
    /// installed directory runs. This pins that claim for the script on the host. It does not show
    /// what the real `kde-open` reads: that is from the source (`kde-open` creates a
    /// `KIO::OpenUrlJob` and reaches `QDesktopServices`, which reads `BROWSER`, only without an
    /// open-with handler and only where `xdg-open` is missing), and the stand-in test
    /// [`a_name_a_stage_may_write_is_not_passed_to_the_kde_open_the_installed_xdg_open_starts`]
    /// shows `BROWSER` does not reach a started `kde-open`. It prints `skipped` and passes where
    /// there is no `xdg-open`, except on Linux with `CI` set, where that fails.
    #[cfg(unix)]
    #[test]
    fn the_installed_xdg_open_does_not_read_browser_under_kde_whatever_kde_open_does() {
        if is_the_child_of_helper_run_by() {
            return;
        }
        let ran = helper_run_by(
            "opener::tests::the_installed_xdg_open_does_not_read_browser_under_kde_whatever_kde_open_does",
            "installed-xdg-open-kde",
            &[
                ("XDG_CURRENT_DESKTOP", "KDE"),
                ("KDE_FULL_SESSION", "true"),
                ("KDE_SESSION_VERSION", "6"),
            ],
            |installed| {
                use std::os::unix::fs::PermissionsExt;
                let opener = bravebot_sandbox::programs::find(OsStr::new("xdg-open"))?;
                for shadow in ["kde-open", "kde-open5", "kfmclient"] {
                    let path = installed.join(shadow);
                    std::fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                        .unwrap();
                }
                Some(opener)
            },
        );
        match ran {
            None => {
                assert!(
                    !the_runner_must_run_the_handlers(),
                    "CI is set and xdg-open is not installed"
                );
                println!("skipped: no xdg-open on this host");
            }
            Some(ran) => assert_eq!(ran, (false, false)),
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
    /// name a real browser. The `XDG_CONFIG_DIRS` case is the variable under test, so the run cannot
    /// point it at an empty directory to avoid the file, and the skip stays on CI too.
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
    /// a started `kde-open`. Whether `kde-open` needs a session bus is not established, and a
    /// headless runner has none.
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
                    // Whether `kde-open` needs a session bus is not established, and a headless
                    // runner has none; either way the planted handler then not running says
                    // nothing.
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
    /// removed and would start their browser. On Linux with `CI` set each of the three fails it.
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
            assert!(
                !the_runner_must_run_the_handlers(),
                "CI is set and xdg-open or gio is not installed"
            );
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
            assert!(
                !the_runner_must_run_the_handlers(),
                "CI is set and this account has handlers of its own under its home"
            );
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
                assert!(
                    !the_runner_must_run_the_handlers(),
                    "CI is set and the xdg-open here did not run the handler in the installed HOME"
                );
                println!("skipped: the xdg-open here does not run a handler chosen by HOME")
            }
            ran => assert_eq!(ran, (false, true)),
        }
    }

    /// The stand-in that [`stand_in_saw`] places first on `PATH` under the name of the program
    /// `xdg-open` hands the address to (`kde-open`, or `gio`): it writes the arguments it was started
    /// with and each variable of the opener's list it can see to `log`, and starts nothing. The names
    /// are those of [`CODE_SELECTORS`], so a name added to it is looked for. The `gio` stand-in
    /// answers `gio help open`, which `xdg-open` runs first, with success and writes nothing, and
    /// logs only a run of `gio open`, with the arguments after `open`.
    #[cfg(unix)]
    fn stand_in_program(log: &Path, program: &str) -> String {
        let names: Vec<String> = CODE_SELECTORS
            .iter()
            .map(|(name, _)| single_quoted(name))
            .collect();
        let only_open = if program == "gio" {
            "if [ \"$1\" != open ]; then exit 0; fi\nshift\n"
        } else {
            ""
        };
        format!(
            "#!/bin/sh\n{only_open}{{\necho \"ARGS $*\"\nfor name in {}; do\n  if printenv \"$name\" >/dev/null 2>&1; then echo \"SET $name=$(printenv \"$name\")\"; fi\ndone\n}} > '{}'\n",
            names.join(" "),
            log.display()
        )
    }

    /// `text` as one word of `sh`: in single quotes, with each `'` written as `'\''`. A name such as
    /// `ProgramFiles(x86)` holds a character `sh` reads as syntax when it is not quoted.
    #[cfg(unix)]
    fn single_quoted(text: &str) -> String {
        format!("'{}'", text.replace('\'', "'\\''"))
    }

    /// Whether `name` is a valid name of a shell variable: a letter or `_`, then letters, digits
    /// and `_`. A shell other than `bash` (`dash`, which is `/bin/sh` on Debian and Ubuntu) drops an
    /// environment entry whose name is not one when it starts, so a script run by `sh` cannot see
    /// such a name, such as `ProgramFiles(x86)`, whatever the process that started it set.
    #[cfg(unix)]
    fn is_shell_identifier(name: &str) -> bool {
        let mut characters = name.chars();
        characters
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && characters.all(|next| next.is_ascii_alphanumeric() || next == '_')
    }

    /// The regression it rejects: `is_shell_identifier` accepting a name `dash` drops (one with a
    /// parenthesis or a leading digit, or none), so a test would expect `sh` to show it, or
    /// refusing a plain name, so a test would skip the assertion that it is kept.
    #[cfg(unix)]
    #[test]
    fn a_name_with_a_parenthesis_a_leading_digit_or_no_characters_is_not_a_shell_identifier() {
        assert!(!is_shell_identifier("ProgramFiles(x86)"));
        assert!(!is_shell_identifier("1ABC"));
        assert!(!is_shell_identifier(""));
        assert!(!is_shell_identifier("A-B"));
        assert!(is_shell_identifier("ProgramFiles"));
        assert!(is_shell_identifier("QT_PLUGIN_PATH"));
        assert!(is_shell_identifier("_x1"));
    }

    /// The regression it rejects: a name of [`CODE_SELECTORS`] written into the stand-in `kde-open`
    /// unquoted, so a name holding `(` makes the script a syntax error for `sh`, the stand-in writes
    /// no log, and the test that places it first on `PATH` fails on a host with `xdg-open`. It
    /// checks the generated script with `sh -n`, then runs it with `ProgramFiles(x86)` and a plain
    /// name set and reads both back from the log, which needs no `xdg-open`. The plain name is read
    /// back whichever shell runs the script. `ProgramFiles(x86)` is read back only where `bash` is
    /// installed, which is run with the script, because `dash` drops it from the environment
    /// ([`is_shell_identifier`]); where `bash` is absent only the parse is checked for it.
    #[cfg(unix)]
    #[test]
    fn the_kde_open_stand_in_script_parses_and_logs_a_name_holding_a_parenthesis() {
        use std::os::unix::fs::PermissionsExt;

        let dir = crate::testutil::scratch_dir("opener-kde-stand-in-script");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("seen.log");
        let script = dir.join("kde-open");
        std::fs::write(&script, stand_in_program(&log, "kde-open")).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let parsed = std::process::Command::new("sh")
            .arg("-n")
            .arg(&script)
            .output()
            .unwrap();
        assert!(
            parsed.status.success(),
            "{}",
            String::from_utf8_lossy(&parsed.stderr)
        );
        let bash = std::process::Command::new("bash")
            .args(["-c", "exit 0"])
            .status()
            .is_ok_and(|status| status.success());
        let ran = std::process::Command::new(if bash { "bash" } else { "sh" })
            .arg(&script)
            .arg("https://example.invalid/")
            .env("ProgramFiles(x86)", "/safe/dir")
            .env("XDG_DATA_HOME", "/safe/dir")
            .output()
            .unwrap();
        let seen = std::fs::read_to_string(&log).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            ran.status.success(),
            "{}",
            String::from_utf8_lossy(&ran.stderr)
        );
        assert_eq!(
            seen.lines().collect::<Vec<_>>().first(),
            Some(&"ARGS https://example.invalid/"),
            "{seen}"
        );
        assert!(
            seen.lines()
                .any(|line| line == "SET XDG_DATA_HOME=/safe/dir"),
            "{seen}"
        );
        if bash {
            assert!(
                seen.lines()
                    .any(|line| line == "SET ProgramFiles(x86)=/safe/dir"),
                "{seen}"
            );
        } else {
            println!("skipped the ProgramFiles(x86) read-back: no bash on this host");
        }
        assert_eq!(single_quoted("a'b c"), "'a'\\''b c'");
    }

    /// What a stand-in for `program` (`kde-open` under `XDG_CURRENT_DESKTOP=KDE`, `gio` under
    /// `XDG_CURRENT_DESKTOP=GNOME`), started by the installed `xdg-open` that the opener starts, was
    /// given: the lines it logged, or `None` when it did not run.
    ///
    /// `variable` is set in the environment of a child run of this test binary (the command takes its
    /// environment from the process, so a test cannot set one) to a path in a directory a stage may
    /// write when `planted`, and to one in an installed directory otherwise. The child keeps the
    /// session directory out as a stage's, runs the opener against `opener`, and every other name
    /// of the opener's list is removed from its environment. For KDE, `KDE_SESSION_VERSION=6` makes
    /// `xdg-open`'s `open_kde` start `kde-open` (`xdg-open.in`), which is found by name on `PATH`,
    /// and the stand-in is first on it. For GNOME, `open_gnome3` runs `gio help open` and then
    /// `gio open`, both found the same way.
    #[cfg(unix)]
    fn stand_in_saw(
        test: &str,
        variable: &str,
        opener: &Path,
        planted: bool,
        desktop: &[(&str, &str)],
        program: &str,
    ) -> Option<String> {
        use std::os::unix::fs::PermissionsExt;

        let which = if planted { "planted" } else { "installed" };
        let name = variable.to_ascii_lowercase();
        let session =
            crate::testutil::scratch_dir(&format!("opener-{program}-{name}-{which}-session"));
        let installed =
            crate::testutil::scratch_dir(&format!("opener-{program}-{name}-{which}-installed"));
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&installed);
        std::fs::create_dir_all(&session).unwrap();
        std::fs::create_dir_all(&installed).unwrap();
        let log = installed.join("seen.log");
        let executable = |path: &Path, body: &str| {
            std::fs::write(path, body).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        executable(&installed.join(program), &stand_in_program(&log, program));
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
            "DBUS_SESSION_BUS_ADDRESS" => {
                std::fs::write(place.join("bus"), "").unwrap();
                format!("unix:path={}", place.join("bus").display())
            }
            // `COMSPEC` is a Windows name, so its file is only a file here: the stand-in
            // `kde-open` is what the child is checked through.
            "COMSPEC" => {
                std::fs::write(place.join("comspec"), "").unwrap();
                place.join("comspec").display().to_string()
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
            .env_remove("XDG_CURRENT_DESKTOP")
            .env_remove("KDE_SESSION_VERSION")
            .env_remove("XDG_SESSION_DESKTOP")
            .env_remove("DESKTOP_SESSION")
            .env_remove("GNOME_DESKTOP_SESSION_ID")
            .env_remove("KDE_FULL_SESSION")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY");
        for (listed, _) in CODE_SELECTORS {
            command.env_remove(listed);
        }
        for (chosen, set) in desktop {
            command.env(chosen, set);
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
    /// Both the installed `xdg-open` and the stand-in are `sh` scripts, and `dash` drops a name that
    /// is not a shell identifier ([`is_shell_identifier`]) from the environment it starts with. For
    /// such a name (`ProgramFiles(x86)`) the stand-in cannot show that a safe value was kept, or
    /// that a planted one was removed. The kept value is checked on the command the opener builds
    /// instead (it is not removed), the address is still required to open in both runs, and the
    /// removal of a planted value is pinned by
    /// `the_opener_removes_a_windows_name_that_selects_code_a_stage_may_write`.
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
        every_name_reaches_the_stand_in_only_when_safe(
            "opener::tests::a_name_a_stage_may_write_is_not_passed_to_the_kde_open_the_installed_xdg_open_starts",
            "kde-open",
            &[("XDG_CURRENT_DESKTOP", "KDE"), ("KDE_SESSION_VERSION", "6")],
        );
    }

    /// SANDBOX-30, GNOME mode, through the installed `xdg-open` and a stand-in `gio` first on `PATH`,
    /// for every name in the opener's list: the same observation as
    /// [`a_name_a_stage_may_write_is_not_passed_to_the_kde_open_the_installed_xdg_open_starts`].
    /// With `XDG_CURRENT_DESKTOP=GNOME` `xdg-open` 1.2.1 runs `gio help open` and then `gio open
    /// <address>` (`open_gnome3`), and the stand-in logs the second. With the name set to a path a stage
    /// may write the stand-in is still started with the address and does not see the name; with a path
    /// in an installed directory it sees the name, so a safe value of each of the five names
    /// (`BROWSER`, `XDG_CONFIG_DIRS`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_DATA_DIRS`) is kept
    /// and the address is still opened. The real `gio open` is run by
    /// [`an_xdg_directory_a_stage_may_write_is_not_read_by_the_installed_xdg_open_under_gnome`],
    /// which needs a `gio` that can launch a handler and so does not run on macOS. This one does
    /// not: the stand-in shows what the opener hands on, not what `gio` reads. It prints `skipped`
    /// and passes where there is no `xdg-open`, and on Linux with `CI` set it fails instead.
    #[cfg(unix)]
    #[test]
    fn a_name_a_stage_may_write_is_not_passed_to_the_gio_the_installed_xdg_open_starts_under_gnome()
    {
        if is_the_child_of_helper_run_by() {
            return;
        }
        every_name_reaches_the_stand_in_only_when_safe(
            "opener::tests::a_name_a_stage_may_write_is_not_passed_to_the_gio_the_installed_xdg_open_starts_under_gnome",
            "gio",
            &[("XDG_CURRENT_DESKTOP", "GNOME")],
        );
    }

    /// For each name of the opener's list: the stand-in `program` that the installed `xdg-open`
    /// starts under `desktop` is given the address, does not see the name set to a path a stage may
    /// write, and sees it set to an installed path (not at all for a [`Names::Options`] name).
    #[cfg(unix)]
    fn every_name_reaches_the_stand_in_only_when_safe(
        test: &str,
        program: &str,
        desktop: &[(&str, &str)],
    ) {
        let Some(opener) = bravebot_sandbox::programs::find(OsStr::new("xdg-open")) else {
            assert!(
                !the_runner_must_run_the_handlers(),
                "CI is set and xdg-open is not installed"
            );
            println!("skipped: no xdg-open on this host");
            return;
        };
        for name in [
            "BROWSER",
            "XDG_CONFIG_DIRS",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_DATA_DIRS",
        ] {
            assert!(
                CODE_SELECTORS.iter().any(|(listed, _)| *listed == name),
                "{name} is not in the opener's list, so this test would not look at it"
            );
        }
        for (variable, names) in CODE_SELECTORS {
            let Some(installed) = stand_in_saw(test, variable, &opener, false, desktop, program)
            else {
                assert!(
                    !the_runner_must_run_the_handlers(),
                    "{variable}: CI is set and the xdg-open here did not start the {program}"
                );
                println!("skipped {variable}: the xdg-open here did not start the {program}");
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
                    "{variable} reached {program}: {installed}"
                );
            } else if is_shell_identifier(variable) {
                assert_eq!(sets(&installed), [variable], "{variable} was not kept");
            } else {
                // `sh` may not pass the name on, so the kept value is read from the command.
                assert!(
                    !withheld(variable, "/safe/dir"),
                    "{variable} was removed from the opener's command with a safe value"
                );
            }
            let planted = stand_in_saw(test, variable, &opener, true, desktop, program)
                .unwrap_or_else(|| panic!("{variable}: the address was not opened"));
            assert_eq!(
                planted.lines().next(),
                Some("ARGS https://example.invalid/"),
                "{variable}: {planted}"
            );
            assert!(
                sets(&planted).is_empty(),
                "{variable} reached {program}: {planted}"
            );
        }
    }

    /// The names [`kde_open_marker_outcome`] sets to a program or to a directory that holds a marker
    /// handler, with what each is set to: `true` for a directory tree and `false` for the path of
    /// the marker program. A name that holds a plug-in directory, a plug-in name or a flag has no
    /// marker form (a marker program is not a plug-in), so it is not in the list and the test says
    /// which names it did not exercise.
    #[cfg(unix)]
    const KDE_OPEN_MARKER_NAMES: &[(&str, bool)] = &[
        ("BROWSER", false),
        ("GIO_LAUNCH_DESKTOP", false),
        ("KDESYCOCA", false),
        ("XDG_DATA_HOME", true),
        ("XDG_DATA_DIRS", true),
        ("XDG_CONFIG_HOME", true),
        ("XDG_CONFIG_DIRS", true),
        ("KDEHOME", true),
        ("KDEDIRS", true),
    ];

    /// Runs the installed `kde-open` on an address that only a marker handler opens, with the
    /// variable `variable` set to a marker (`None` runs the baseline, which sets none), and says
    /// which handler ran: the one the baseline selects (`genuine`), the one the variable names
    /// (`planted`), both, or neither. The third value is what `kde-open` ended with and printed on
    /// its two streams, for a failure message to carry.
    ///
    /// The environment is cleared and rebuilt, so that the host's desktop and the person's files
    /// select nothing: `HOME` and `XDG_CACHE_HOME` are scratch directories (the service cache is
    /// built there), `XDG_DATA_HOME` and `XDG_CONFIG_HOME` are empty directories, `XDG_DATA_DIRS`
    /// is a directory holding both handlers' entries and an `applications/mimeapps.list` that makes
    /// `genuine` the default for the address, and `XDG_CONFIG_DIRS` is the same directory as
    /// `XDG_CONFIG_HOME`, which holds only a minimal `menus/applications.menu` (see below). The
    /// session bus and runtime directory are passed on, in case `kde-open` needs a bus (not
    /// established), and
    /// `QT_QPA_PLATFORM=offscreen` lets it start without a display. A name that replaces one of
    /// those four is the variable under test, so the baseline's value for it is dropped.
    ///
    /// A marker tree holds the same files under every spelling the directory names read
    /// (`applications/`, the top directory, and `share/applications/`), and every `mimeapps.list`
    /// in it makes `planted` the default, so a name that makes `kde-open` read the tree runs
    /// `planted`.
    #[cfg(unix)]
    fn kde_open_marker_outcome(
        kde_open: &Path,
        variable: Option<(&str, bool)>,
    ) -> (bool, bool, String) {
        use std::os::unix::fs::PermissionsExt;

        let label = variable.map_or("baseline".to_string(), |(name, _)| {
            name.to_ascii_lowercase()
        });
        let root = crate::testutil::scratch_dir(&format!("opener-kde-marker-{label}"));
        let _ = std::fs::remove_dir_all(&root);
        let session = root.join("session");
        let home = root.join("home");
        let cache = root.join("cache");
        let empty_data = root.join("empty-data");
        let empty_config = root.join("empty-config");
        for directory in [&session, &home, &cache, &empty_data, &empty_config] {
            std::fs::create_dir_all(directory).unwrap();
        }
        // KService reads a menu file `menus/applications.menu` from a configuration directory when
        // it builds the service cache. With both configuration directories empty, `kde-open` exits
        // with status 1 and stderr `"applications.menu"  not found in  QList()`. The menu below names
        // the standard application directories only,
        // so which handler runs is still chosen by the data directories and `mimeapps.list`. It
        // is in `empty_config`, which is `XDG_CONFIG_HOME` and `XDG_CONFIG_DIRS` in every run except
        // the two where one of them is the variable under test, and the other still names it, so
        // every run reads the same menu.
        std::fs::create_dir_all(empty_config.join("menus")).unwrap();
        std::fs::write(
            empty_config.join("menus/applications.menu"),
            concat!(
                "<!DOCTYPE Menu PUBLIC \"-//freedesktop//DTD Menu 1.0//EN\"\n",
                " \"http://www.freedesktop.org/standards/menu-spec/menu-1.0.dtd\">\n",
                "<Menu>\n<Name>Applications</Name>\n<DefaultAppDirs/>\n",
                "<DefaultDirectoryDirs/>\n<DefaultMergeDirs/>\n",
                "<Include><All/></Include>\n</Menu>\n"
            ),
        )
        .unwrap();
        let scheme = "bravebot-test";
        let script = |marker: &str| {
            let path = root.join(format!("{marker}.sh"));
            std::fs::write(
                &path,
                format!("#!/bin/sh\n: > '{}'\n", session.join(marker).display()),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let entry = |directory: &Path, marker: &str| {
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(
                directory.join(format!("{marker}.desktop")),
                format!(
                    "[Desktop Entry]\nType=Application\nName={marker}\nExec={} %u\nMimeType=x-scheme-handler/{scheme};\nTerminal=false\n",
                    script(marker).display()
                ),
            )
            .unwrap();
        };
        let default = |directory: &Path, marker: &str| {
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(
                directory.join("mimeapps.list"),
                format!("[Default Applications]\nx-scheme-handler/{scheme}={marker}.desktop\n"),
            )
            .unwrap();
        };
        let baseline_data = root.join("baseline-data");
        for marker in ["genuine-ran", "planted-ran"] {
            entry(&baseline_data.join("applications"), marker);
        }
        default(&baseline_data.join("applications"), "genuine-ran");
        let tree = root.join("tree");
        for directory in [
            tree.clone(),
            tree.join("applications"),
            tree.join("share/applications"),
        ] {
            entry(&directory, "planted-ran");
            default(&directory, "planted-ran");
        }

        let mut command = std::process::Command::new(kde_open);
        command
            .env_clear()
            .arg(format!("{scheme}://example.invalid/"))
            .stdin(std::process::Stdio::null())
            .stdout(std::fs::File::create(root.join("kde-open.stdout")).unwrap())
            .stderr(std::fs::File::create(root.join("kde-open.stderr")).unwrap())
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &home)
            .env("XDG_CACHE_HOME", &cache)
            .env("XDG_DATA_HOME", &empty_data)
            .env("XDG_DATA_DIRS", &baseline_data)
            .env("XDG_CONFIG_HOME", &empty_config)
            .env("XDG_CONFIG_DIRS", &empty_config)
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("XDG_CURRENT_DESKTOP", "KDE")
            .env("KDE_FULL_SESSION", "true")
            .env("KDE_SESSION_VERSION", "6")
            .env("LANG", "C.UTF-8");
        for name in ["DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        if let Some((name, is_tree)) = variable {
            let value = if is_tree {
                tree.clone()
            } else if name == "BROWSER" {
                std::path::PathBuf::from(format!("{} %s", script("planted-ran").display()))
            } else {
                script("planted-ran")
            };
            command.env(name, value);
        }
        let mut child = command.spawn().unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut status = child.try_wait().unwrap();
        while status.is_none() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(50));
            status = child.try_wait().unwrap();
        }
        let ended = status.map_or_else(
            || "still running after 30 seconds, then killed".to_string(),
            |status| format!("ended with {status}"),
        );
        let _ = child.kill();
        let _ = child.wait();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !session.join("genuine-ran").exists()
            && !session.join("planted-ran").exists()
            && std::time::Instant::now() < until
        {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let printed = |stream: &str| {
            std::fs::read_to_string(root.join(format!("kde-open.{stream}")))
                .unwrap_or_else(|error| format!("<unreadable: {error}>"))
        };
        let outcome = (
            session.join("genuine-ran").exists(),
            session.join("planted-ran").exists(),
            format!(
                "{} {ended}; stdout: {:?}; stderr: {:?}",
                kde_open.display(),
                printed("stdout"),
                printed("stderr")
            ),
        );
        let _ = std::fs::remove_dir_all(&root);
        outcome
    }

    /// SANDBOX-30, `kde-open` run for real once per name: each name in
    /// [`KDE_OPEN_MARKER_NAMES`] is set, alone, to a marker handler, and the test prints `kde-open
    /// observed: <name> <outcome>` for each. A baseline run with no name set must run the `genuine`
    /// handler first, so a `planted` marker that did not run is not a run that could not start.
    ///
    /// Without `BRAVEBOT_OBSERVE_KDE_OPEN` the test prints `skipped` and passes where `kde-open`
    /// is absent or the baseline runs no handler, as can happen on a host without a session
    /// bus (whether `kde-open` needs one is not established). With it set, which the CI step that
    /// runs the test under `dbus-run-session` does, either
    /// of those fails the test, and so does a name that ran the `planted` handler but is not in
    /// [`CODE_SELECTORS`], since the opener would then pass on a name `kde-open` follows.
    ///
    /// The regression it rejects: a `kde-open` that selects a program through a variable the
    /// opener leaves in the environment. It does not show that the names it did not exercise
    /// (plug-in directories, plug-in names, `QT_STYLE_OVERRIDE`) select nothing.
    ///
    /// The baseline failure carries the path of the `kde-open` run, how it ended, and what it
    /// printed on stdout and stderr. With both configuration directories empty, `kde-open` ran
    /// neither handler, `(false, false)`, and ended with `/usr/bin/kde-open ended with exit status:
    /// 1`, stdout empty and stderr `"applications.menu"  not found in  QList()` followed by `Could
    /// not read file bravebot-test://example.invalid/.`. The harness writes `menus/applications.menu`
    /// into the empty configuration directory so that the baseline runs the genuine handler.
    #[cfg(unix)]
    #[test]
    fn kde_open_run_with_each_name_set_to_a_marker_runs_the_marker_only_for_names_the_opener_removes()
     {
        if is_the_child_of_helper_run_by() {
            return;
        }
        let observing = std::env::var_os("BRAVEBOT_OBSERVE_KDE_OPEN").is_some();
        let Some(kde_open) = ["kde-open", "kde-open5"]
            .into_iter()
            .find_map(|name| bravebot_sandbox::programs::find(OsStr::new(name)))
        else {
            assert!(
                !observing,
                "BRAVEBOT_OBSERVE_KDE_OPEN is set and kde-open is not installed"
            );
            println!("skipped: no kde-open on this host");
            return;
        };
        let (genuine, planted, seen) = kde_open_marker_outcome(&kde_open, None);
        if (genuine, planted) != (true, false) {
            assert!(
                !observing,
                "BRAVEBOT_OBSERVE_KDE_OPEN is set and the baseline kde-open ran (genuine, planted) = ({genuine}, {planted}): {seen}"
            );
            println!(
                "skipped: the kde-open here did not run the baseline handler: ({genuine}, {planted}): {seen}"
            );
            return;
        }
        for &(name, is_tree) in KDE_OPEN_MARKER_NAMES {
            let (genuine, planted, _) = kde_open_marker_outcome(&kde_open, Some((name, is_tree)));
            println!("kde-open observed: {name} genuine={genuine} planted={planted}");
            if planted && observing {
                assert!(
                    CODE_SELECTORS.iter().any(|(listed, _)| *listed == name),
                    "{name} made kde-open run a program and the opener does not remove it"
                );
            }
        }
        let tried: Vec<&str> = KDE_OPEN_MARKER_NAMES
            .iter()
            .map(|(name, _)| *name)
            .collect();
        let untried: Vec<&str> = CODE_SELECTORS
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| !tried.contains(name))
            .collect();
        println!("kde-open not exercised (no marker form): {untried:?}");
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

    /// SANDBOX-30, the Windows start (#3860): each directory name the start's list holds is removed
    /// when it names a directory a stage may write and kept when it names one outside them, and
    /// `PATHEXT` is removed whatever it holds. The regression it rejects: a name left out of the
    /// list, so a handler command written with `%LOCALAPPDATA%` or `%ProgramFiles%` that
    /// `ShellExecute` expands from the environment runs what a stage wrote; a name judged as an
    /// option, which removes a safe value; and `PATHEXT` kept or judged as a path. Checked on the
    /// hosts with Unix paths, since the values are `/stage/dir` and `/safe/dir`, which are not
    /// absolute paths on Windows; the case rule is checked on Windows by
    /// `a_windows_name_is_removed_in_whatever_case_it_is_set`. That the names choose the program
    /// was read in Wine and ReactOS source and not run; see [`CODE_SELECTORS`].
    #[cfg(unix)]
    #[test]
    fn the_opener_removes_a_windows_name_that_selects_code_a_stage_may_write() {
        assert!(withheld("SystemRoot", "/stage/dir"));
        assert!(withheld("SystemRoot", "relative"));
        assert!(!withheld("SystemRoot", "/safe/dir"));
        assert!(withheld("windir", "/stage/dir"));
        assert!(withheld("windir", "relative"));
        assert!(!withheld("windir", "/safe/dir"));
        assert!(withheld("ProgramFiles", "/stage/dir"));
        assert!(withheld("ProgramFiles", "relative"));
        assert!(!withheld("ProgramFiles", "/safe/dir"));
        assert!(withheld("ProgramW6432", "/stage/dir"));
        assert!(withheld("ProgramW6432", "relative"));
        assert!(!withheld("ProgramW6432", "/safe/dir"));
        assert!(withheld("ProgramFiles(x86)", "/stage/dir"));
        assert!(withheld("ProgramFiles(x86)", "relative"));
        assert!(!withheld("ProgramFiles(x86)", "/safe/dir"));
        assert!(withheld("LOCALAPPDATA", "/stage/dir"));
        assert!(withheld("LOCALAPPDATA", "relative"));
        assert!(!withheld("LOCALAPPDATA", "/safe/dir"));
        assert!(withheld("APPDATA", "/stage/dir"));
        assert!(withheld("APPDATA", "relative"));
        assert!(!withheld("APPDATA", "/safe/dir"));
        assert!(withheld("USERPROFILE", "/stage/dir"));
        assert!(withheld("USERPROFILE", "relative"));
        assert!(!withheld("USERPROFILE", "/safe/dir"));

        assert!(withheld("PATHEXT", ".COM;.EXE;.BAT;.CMD"));
        assert!(withheld("PATHEXT", ".CMD"));
        assert!(withheld("PATHEXT", "/safe/anything"));
        assert!(withheld("PATHEXT", ""));
    }

    /// SANDBOX-30, the Windows start (#3860), on the disk: `COMSPEC` names the program a batch file
    /// is started through, so it is one file. It is removed when it is a file in a directory a stage
    /// may write, a directory, a path that does not exist or a bare name, and kept when it is an
    /// existing file outside every such directory. The regression it rejects: `COMSPEC` left out of
    /// the list, judged as a directory or as an option, or judged by its name alone. It also pins
    /// `PATHEXT` as an option: an existing file outside the stage's directories is removed for it,
    /// which a `PATHEXT` judged as a file would keep.
    #[test]
    fn comspec_is_removed_for_a_program_a_stage_may_write_and_kept_for_an_installed_one() {
        let session = crate::testutil::scratch_dir("opener-comspec-session");
        let safe = crate::testutil::scratch_dir("opener-comspec-safe");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
        std::fs::create_dir_all(&session).unwrap();
        std::fs::create_dir_all(&safe).unwrap();
        let planted = session.join("cmd.exe");
        let genuine = safe.join("cmd.exe");
        std::fs::write(&planted, "").unwrap();
        std::fs::write(&genuine, "").unwrap();
        let session = session.canonicalize().unwrap();
        let safe = safe.canonicalize().unwrap();

        // Shadows the helper above, which judges by the name `/safe` and cannot see a file on disk.
        let withheld = |name: &str, value: &Path| {
            opening_against(
                &Resolved::found_earlier("/usr/bin/open"),
                &[],
                [(OsString::from(name), value.as_os_str().to_os_string())],
                &|path: &Path| !path.starts_with(&session),
            )
            .get_envs()
            .any(|(set, now)| set == OsStr::new(name) && now.is_none())
        };

        assert!(withheld("COMSPEC", &session.join("cmd.exe")));
        assert!(withheld("COMSPEC", &session.join("not-yet.exe")));
        assert!(withheld("COMSPEC", Path::new("cmd.exe")));
        assert!(withheld("COMSPEC", &safe));
        assert!(!withheld("COMSPEC", &safe.join("cmd.exe")));
        assert!(withheld("PATHEXT", &safe.join("cmd.exe")));

        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
    }

    /// SANDBOX-30, the Windows start (#3860): Windows holds a variable's name without regard to
    /// case, so `Windir` and `ComSpec` are the variables `windir` and `COMSPEC` are, and are
    /// removed. The regression it rejects: the names compared as written, which a process that
    /// spells `ComSpec` as Windows tools often do slips past.
    #[cfg(windows)]
    #[test]
    fn a_windows_name_is_removed_in_whatever_case_it_is_set() {
        // The command is told to remove the spelling the list holds, so the name is compared
        // without regard to case. Only a path under `C:\safe` is outside a stage's directories.
        let withheld = |name: &str, value: &str| {
            opening_against(
                &Resolved::found_earlier("rundll32"),
                &[],
                [(OsString::from(name), OsString::from(value))],
                &|path: &Path| path.starts_with(r"C:\safe"),
            )
            .get_envs()
            .any(|(set, now)| set.eq_ignore_ascii_case(name) && now.is_none())
        };

        assert!(withheld("Windir", r"C:\stage\dir"));
        assert!(withheld("LocalAppData", r"C:\stage\dir"));
        assert!(withheld("ProgramFiles", r"C:\stage\dir"));
        assert!(withheld("ProgramFiles(x86)", r"C:\stage\dir"));
        assert!(withheld("ComSpec", r"C:\stage\cmd.exe"));
        assert!(withheld("PathExt", ".CMD"));
        assert!(!withheld("Windir", r"C:\safe\dir"));
    }

    /// The directory a stage may write, in the environment of a child of
    /// [`the_windows_start_given_names_that_point_at_a_stage_written_helper_does_not_run_it`].
    #[cfg(windows)]
    const WINDOWS_START_STAGE: &str = "BRAVEBOT_TEST_WINDOWS_START_STAGE";
    /// The `rundll32` the child starts.
    #[cfg(windows)]
    const WINDOWS_START_PROGRAM: &str = "BRAVEBOT_TEST_WINDOWS_START_PROGRAM";
    /// The one name the child sets on the command again after the opener took it off, when it is
    /// the control.
    #[cfg(windows)]
    const WINDOWS_START_PUT_BACK: &str = "BRAVEBOT_TEST_WINDOWS_START_PUT_BACK";
    /// What every record the child prints begins with. The test harness writes `test <name> ... `
    /// to the same output without a newline before a test runs, so a record is found by this
    /// prefix at the start of its own line and not by its first words.
    #[cfg(windows)]
    const WINDOWS_START_RECORD: &str = "windows-start-record: ";

    /// The records in what the child printed, each without [`WINDOWS_START_RECORD`]: the lines
    /// that begin with it, whatever else the output holds.
    #[cfg(windows)]
    fn records_of_the_windows_start(printed: &str) -> Vec<&str> {
        printed
            .lines()
            .filter_map(|line| line.strip_prefix(WINDOWS_START_RECORD))
            .collect()
    }

    /// Whether this process is the child of the test below, which has made the start it was asked
    /// for. The child makes the opener's start against its own environment, which the parent set,
    /// prints a record `removed <name>` for each name the command takes off and a record
    /// `start ended: ..` for how `rundll32` ended, and runs it. It begins with a newline, which
    /// ends the line the test harness left open, so every record is at the start of a line.
    #[cfg(windows)]
    fn is_the_child_of_the_windows_start() -> bool {
        let (Some(stage), Some(program)) = (
            std::env::var_os(WINDOWS_START_STAGE),
            std::env::var_os(WINDOWS_START_PROGRAM),
        ) else {
            return false;
        };
        let stage = std::path::PathBuf::from(stage);
        let (_, arguments) = opener("windows", "https://example.invalid/");
        let mut command = opening_against(
            &Resolved::found_earlier(std::path::PathBuf::from(program)),
            &arguments,
            std::env::vars_os(),
            &|path: &Path| !path.starts_with(&stage),
        );
        if let Some(name) = std::env::var_os(WINDOWS_START_PUT_BACK) {
            // The control: the command removed the name, and this sets it again, which also
            // overrides the removal of `PATHEXT`.
            let value = std::env::var_os(&name).expect("the name to put back is not set");
            command.env(&name, value);
        }
        println!();
        for name in withheld_from(&command) {
            println!("{WINDOWS_START_RECORD}removed {name}");
        }
        println!("{WINDOWS_START_RECORD}start ended: {:?}", command.status());
        true
    }

    /// SANDBOX-30, the Windows start (#3860), run on Windows. `rundll32 url.dll,FileProtocolHandler`
    /// is started through the opener with each of the ten names of the Windows list set to a path
    /// in a directory a stage may write, `COMSPEC` to a batch file that only creates a marker file
    /// and `PATHEXT` to `.CMD`. The start is made in a child run of this test binary, because the
    /// command takes its environment from the process and a test cannot set one: the parent gives
    /// the child those names with `Command::env`, and the child calls the opener's start against
    /// its own environment, prints the names the command takes off and runs it. The test asserts
    /// that the child printed a record for every one of the ten and for how the start ended, each
    /// on a line of its own, and that the marker was not created. The
    /// helper is shown to create the marker when run directly through `cmd`, so an absent marker
    /// is not a helper that cannot run.
    ///
    /// For each of the ten names and for `PSModulePath`, which the list leaves out, the test then
    /// makes the same start with only that name set, and the child sets it on the command again
    /// after the opener took it off. It prints whether the marker appeared and how the start
    /// ended. That record is printed and not asserted: a start that does not run the helper is
    /// the result for a name `rundll32` does not read, and it ending badly with `SystemRoot` put
    /// back at a stage's directory says nothing about the opener. A green run therefore shows
    /// that the opener removes the ten names from the real start and that the marker did not
    /// appear, and shows nothing about which names select code; the printed record does.
    ///
    /// The regression it rejects: a name of the list that the command does not take off, such as
    /// `ProgramFiles` dropped from [`CODE_SELECTORS`], or a start that does not remove a name
    /// under the case the environment holds it in. It does not reject a name that selects code
    /// and is in neither the list nor this test, which is found by reading the program. It has
    /// not been run: no Windows host ran it for this change.
    #[cfg(windows)]
    #[test]
    fn the_windows_start_given_names_that_point_at_a_stage_written_helper_does_not_run_it() {
        if is_the_child_of_the_windows_start() {
            return;
        }
        let test = "opener::tests::the_windows_start_given_names_that_point_at_a_stage_written_helper_does_not_run_it";
        let appeared = |marker: &Path| {
            (0..20).any(|_| {
                if marker.exists() {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
                false
            })
        };
        let session = crate::testutil::scratch_dir("opener-windows-session");
        let _ = std::fs::remove_dir_all(&session);
        std::fs::create_dir_all(&session).unwrap();
        let session = session.canonicalize().unwrap();
        // `cmd` does not read a `\\?\` path, which `canonicalize` returns.
        let plain = |path: &Path| {
            std::path::PathBuf::from(path.to_string_lossy().trim_start_matches(r"\\?\"))
        };
        let marker = plain(&session.join("marker.txt"));
        let helper = plain(&session.join("helper.cmd"));
        std::fs::write(
            &helper,
            format!("@echo off\r\necho ran> \"{}\"\r\n", marker.display()),
        )
        .unwrap();
        let stage = plain(&session);
        let directory = stage.clone().into_os_string();
        let listed = [
            ("PATHEXT", OsString::from(".CMD")),
            ("COMSPEC", helper.clone().into_os_string()),
            ("SystemRoot", directory.clone()),
            ("windir", directory.clone()),
            ("ProgramFiles", directory.clone()),
            ("ProgramW6432", directory.clone()),
            ("ProgramFiles(x86)", directory.clone()),
            ("LOCALAPPDATA", directory.clone()),
            ("APPDATA", directory.clone()),
            ("USERPROFILE", directory.clone()),
        ];
        // Not in the list, so only the control sets it.
        let not_listed = [("PSModulePath", directory)];

        // The helper creates the marker when it is run, and the wait sees it.
        let environment: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        let cmd = Resolved::find(OsStr::new("cmd")).expect("cmd is not found");
        let direct = opening_against(
            &cmd,
            &["/c".to_string(), helper.to_string_lossy().into_owned()],
            environment,
            &|_| true,
        )
        .status()
        .unwrap();
        assert!(direct.success(), "{direct}");
        assert!(appeared(&marker), "the helper did not create the marker");
        std::fs::remove_file(&marker).unwrap();

        let program = Resolved::find(OsStr::new("rundll32")).expect("rundll32 is not found");
        // What the child printed when it made the start with `names` set over this process's
        // environment, putting `put_back` on the command again when that is given.
        let made = |names: &[(&str, OsString)], put_back: Option<&str>| {
            // nosemgrep: rust.lang.security.current-exe.current-exe
            let mut run = std::process::Command::new(std::env::current_exe().unwrap());
            run.args(["--exact", test, "--test-threads=1", "--nocapture"])
                .env(WINDOWS_START_STAGE, &stage)
                .env(WINDOWS_START_PROGRAM, program.as_path())
                .env_remove(WINDOWS_START_PUT_BACK);
            for (name, value) in names {
                run.env(name, value);
            }
            if let Some(name) = put_back {
                run.env(WINDOWS_START_PUT_BACK, name);
            }
            let output = run.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let printed = String::from_utf8_lossy(&output.stdout).into_owned();
            // A child that printed nothing has not made the start.
            assert!(
                records_of_the_windows_start(&printed)
                    .iter()
                    .any(|record| record.starts_with("start ended: ")),
                "the child did not print how the start ended: {printed}"
            );
            printed
        };

        let printed = made(&listed, None);
        println!("{printed}");
        for (name, _) in &listed {
            assert!(
                records_of_the_windows_start(&printed)
                    .iter()
                    .any(|record| record.eq_ignore_ascii_case(&format!("removed {name}"))),
                "{name} was not removed from the start: {printed}"
            );
        }
        assert!(
            !appeared(&marker),
            "the helper was run by the checked start"
        );

        for (name, value) in listed.iter().chain(&not_listed) {
            let one = [(*name, value.clone())];
            let printed = made(&one, Some(name));
            let ended = records_of_the_windows_start(&printed)
                .into_iter()
                .find(|record| record.starts_with("start ended: "))
                .expect("the child printed how the start ended");
            println!(
                "unchecked start with only {name} set: {ended}, helper run {}",
                appeared(&marker)
            );
            let _ = std::fs::remove_file(&marker);
        }
        let _ = std::fs::remove_dir_all(&session);
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

        assert!(withheld("XDG_CACHE_HOME", "/stage/cache"));
        assert!(withheld("XDG_CACHE_HOME", "cache"));
        assert!(!withheld("XDG_CACHE_HOME", "/safe/cache"));

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
            "XDG_SESSION_TYPE",
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
        #[cfg(unix)]
        {
            assert!(!withheld("XDG_RUNTIME_DIR", "/safe/run"));
            assert!(!withheld(
                "DBUS_SESSION_BUS_ADDRESS",
                "unix:path=/safe/run/bus"
            ));
        }
    }

    /// SANDBOX-30, `gio open`: the session bus it connects to. The regression it rejects: neither
    /// name in the opener's list, so a socket a stage listens on, named by `XDG_RUNTIME_DIR/bus` or
    /// by the address, is connected to by the program the opener starts; or `DBUS_SESSION_BUS_ADDRESS`
    /// removed whatever it holds, which takes the person's own bus from `gio open` too.
    #[cfg(unix)]
    #[test]
    fn the_opener_removes_the_runtime_directory_and_bus_address_that_lead_to_a_socket_a_stage_may_write()
     {
        assert!(withheld("XDG_RUNTIME_DIR", "/stage/run"));
        assert!(withheld("XDG_RUNTIME_DIR", "run"));
        assert!(withheld("XDG_RUNTIME_DIR", ""));
        assert!(!withheld("XDG_RUNTIME_DIR", "/safe/run"));

        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/stage/run/bus"
        ));
        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/safe/run/bus;unix:path=/stage/run/bus"
        ));
        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:abstract=/tmp/dbus-x"
        ));
        assert!(withheld("DBUS_SESSION_BUS_ADDRESS", ""));
        assert!(!withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/safe/run/bus"
        ));
        assert!(!withheld("DBUS_SESSION_BUS_ADDRESS", "unix:runtime=yes"));
    }

    /// SANDBOX-30, `gio open`: a bus address that is not text is removed and not judged by its
    /// lossy rendering. The regression it rejects: `unix:path=/safe/run/bus` followed by a byte that
    /// is not UTF-8 read as the path under `/safe` and kept. The same text with the replacement
    /// character written out is a path of its own and is kept. `XDG_RUNTIME_DIR` is a path, which is
    /// judged by its bytes whether or not it is text.
    #[cfg(unix)]
    #[test]
    fn a_bus_address_that_is_not_text_is_removed_and_not_judged_by_its_lossy_rendering() {
        use std::os::unix::ffi::OsStrExt;
        let name = "DBUS_SESSION_BUS_ADDRESS";

        assert!(withheld_bytes(
            name,
            OsStr::from_bytes(b"unix:path=/safe/run/bus\xff")
        ));
        assert!(!withheld(name, "unix:path=/safe/run/bus\u{fffd}"));
        assert!(withheld_bytes(
            "XDG_RUNTIME_DIR",
            OsStr::from_bytes(b"/stage/run\xff")
        ));
        assert!(!withheld_bytes(
            "XDG_RUNTIME_DIR",
            OsStr::from_bytes(b"/safe/run\xff")
        ));
    }

    /// SANDBOX-30, on the disk: a socket path is judged where it leads. The regression it rejects:
    /// the address compared as written, so a link under an installed directory to a socket a stage
    /// may write is kept, and a socket that does not exist yet, which whoever can write its
    /// directory makes first, is kept.
    #[cfg(unix)]
    #[test]
    fn a_bus_socket_is_judged_by_where_it_leads_on_the_disk() {
        let session = crate::testutil::scratch_dir("opener-bus-session");
        let safe = crate::testutil::scratch_dir("opener-bus-safe");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
        std::fs::create_dir_all(&session).unwrap();
        std::fs::create_dir_all(&safe).unwrap();
        let session = session.canonicalize().unwrap();
        let safe = safe.canonicalize().unwrap();
        let planted = session.join("bus");
        let genuine = safe.join("bus");
        let linked = safe.join("linked");
        std::fs::write(&planted, "").unwrap();
        std::fs::write(&genuine, "").unwrap();
        std::os::unix::fs::symlink(&planted, &linked).unwrap();

        let writable = vec![session.clone()];
        let removed = |name: &str, value: String| {
            opening_against(
                &Resolved::found_earlier("/usr/bin/open"),
                &[],
                [(OsString::from(name), OsString::from(value))],
                &|path: &Path| {
                    bravebot_sandbox::programs::outside_stage_writable_with(path, &writable, &[])
                },
            )
            .get_envs()
            .any(|(set, now)| set == OsStr::new(name) && now.is_none())
        };
        let address = |path: &Path| format!("unix:path={}", path.display());
        let name = "DBUS_SESSION_BUS_ADDRESS";

        assert!(!removed(name, address(&genuine)));
        assert!(removed(name, address(&planted)));
        assert!(removed(name, address(&linked)));
        assert!(removed(name, address(&safe.join("not-yet"))));
        assert!(!removed("XDG_RUNTIME_DIR", safe.display().to_string()));
        assert!(removed("XDG_RUNTIME_DIR", session.display().to_string()));
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&safe);
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

    /// Whether the planted handler and the installed handler ran, when the installed `xdg-open`
    /// is started with `variable` set to `value` after the opener has judged it. `value` is
    /// written with `@STAGE@` for the directory a stage may write. The variable is set, or left
    /// unset when the opener removed it, on a command whose environment is otherwise only what
    /// `xdg-open` needs, so the run shows what the opener's decision does to the real script.
    ///
    /// The layout is `<root>/stage` and `<root>/installed/{data/applications,config,home}`, so
    /// `../../../stage/` from `installed/data/applications` and `../../stage/` from
    /// `installed/config` reach the stage. The handler's `.desktop` file is in both, and the planted
    /// one is reached only through a `/` in the value. The address has the scheme `bravebot-test`,
    /// which no system handler opens.
    #[cfg(unix)]
    fn file_name_prefix_run(
        test: &str,
        variable: &str,
        value: &str,
        opener: &Path,
    ) -> (bool, bool) {
        use std::os::unix::fs::PermissionsExt;

        let root = crate::testutil::scratch_dir(&format!(
            "opener-{test}-{}",
            variable.to_ascii_lowercase()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let stage = root.join("stage");
        let installed = root.join("installed");
        let data = installed.join("data/applications");
        let config = installed.join("config");
        let log = root.join("log");
        for directory in [&stage, &data, &config, &installed.join("home"), &log] {
            std::fs::create_dir_all(directory).unwrap();
        }
        for (directory, marker) in [(&stage, "planted"), (&data, "genuine")] {
            let script = root.join(format!("{marker}.sh"));
            std::fs::write(
                &script,
                format!("#!/bin/sh\n: > '{}'\n", log.join(marker).display()),
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::write(
                directory.join("handler.desktop"),
                format!(
                    "[Desktop Entry]\nType=Application\nName=h\nExec={} %u\n",
                    script.display()
                ),
            )
            .unwrap();
        }
        let planted = "x-scheme-handler/bravebot-test=../../../stage/handler.desktop\n";
        let installed_default = "x-scheme-handler/bravebot-test=handler.desktop\n";
        let list = |default: &str| format!("[Default Applications]\n{default}");
        // The files a value without a `/` names inside the installed directories (the control),
        // and the ones a value with `../` reaches in the stage.
        std::fs::write(config.join("evil-mimeapps.list"), list(installed_default)).unwrap();
        std::fs::write(stage.join("evil-mimeapps.list"), list(planted)).unwrap();
        std::fs::write(data.join("evil-defaults.list"), installed_default).unwrap();
        std::fs::write(stage.join("defaults.list"), planted).unwrap();

        let value = value.replace("@STAGE@", &stage.display().to_string());
        let judged = opening_against(
            &Resolved::found_earlier(opener.to_str().unwrap()),
            &[],
            [(OsString::from(variable), OsString::from(&value))],
            &|path: &Path| path.starts_with("/safe"),
        );
        let removed = judged
            .get_envs()
            .any(|(set, now)| set == OsStr::new(variable) && now.is_none());
        let mut path = std::ffi::OsString::new();
        for program in ["xdg-open", "xdg-mime"] {
            if let Some(found) = bravebot_sandbox::programs::find(OsStr::new(program))
                && let Some(parent) = found.parent()
            {
                path.push(parent);
                path.push(":");
            }
        }
        path.push("/usr/bin:/bin");
        let mut command = std::process::Command::new(opener);
        command
            .env_clear()
            .env("PATH", path)
            .env("HOME", installed.join("home"))
            .env("XDG_DATA_DIRS", installed.join("data"))
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_CONFIG_DIRS", &config)
            .env("WAYLAND_DISPLAY", "bravebot-test-no-such-display")
            .arg("bravebot-test://example.invalid/");
        if !removed {
            command.env(variable, &value);
        }
        let _ = command.output().unwrap();
        let ran = (log.join("planted").exists(), log.join("genuine").exists());
        let _ = std::fs::remove_dir_all(&root);
        ran
    }

    /// SANDBOX-30, through the installed `xdg-open` 1.2.1 and `xdg-mime` in generic mode.
    /// `XDG_CURRENT_DESKTOP` (each `:` entry, lowercased, is the start of `<entry>-mimeapps.list`)
    /// and `XDG_MENU_PREFIX` (the start of `defaults.list` and `mimeinfo.cache`) are joined to a
    /// directory the opener has judged without stripping a `/`. The regression it rejects: the opener
    /// left a value with `../` as the person set it, so a `mimeapps.list` or `defaults.list` a stage
    /// wrote chose the handler `xdg-open` ran. The same variable with a value that has no `/` still
    /// reads a file in the installed directory, which shows the variable chooses the handler here and
    /// that a safe value is kept. It prints `skipped` and passes where the installed handler does
    /// not run for that control, except on Linux with `CI` set.
    #[cfg(unix)]
    #[test]
    fn a_file_name_prefix_with_a_slash_does_not_lead_the_installed_xdg_open_out_of_a_judged_directory()
     {
        const TEST: &str = "prefix";
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
        for (variable, safe, escaping) in [
            ("XDG_CURRENT_DESKTOP", "evil", "../../stage/evil"),
            ("XDG_MENU_PREFIX", "evil-", "../../../stage/"),
        ] {
            let control = file_name_prefix_run(TEST, variable, safe, &opener);
            if control != (false, true) {
                assert!(
                    !the_runner_must_run_the_handlers(),
                    "{variable}: the xdg-open here did not run the installed handler: {control:?}"
                );
                println!("skipped {variable}: the xdg-open here ignores it ({control:?})");
                continue;
            }
            let ran = file_name_prefix_run(TEST, variable, escaping, &opener);
            assert!(!ran.0, "{variable}={escaping}: the planted handler ran");
        }
    }

    /// The names `xdg-mime` and `xdg-open` join to a directory as the start of a file name are
    /// removed when they hold a `/`, and kept otherwise.
    #[test]
    fn a_file_name_prefix_is_removed_only_when_it_has_a_slash() {
        assert!(!FILE_NAME_PREFIXES.is_empty());
        for name in FILE_NAME_PREFIXES {
            assert!(withheld(name, "../../stage/x"), "{name}");
            assert!(withheld(name, "/stage/x"), "{name}");
            assert!(withheld(name, "GNOME:../x"), "{name}");
            assert!(!withheld(name, "GNOME"), "{name}");
            assert!(!withheld(name, "KDE"), "{name}");
            assert!(!withheld(name, "gnome-"), "{name}");
            assert!(!withheld(name, ""), "{name}");
        }
    }

    /// The slash check of a file name prefix runs on the bytes of the value. A value that is not
    /// UTF-8 and holds a `/` is removed, so it is not read as having none. One that is not UTF-8
    /// and holds no `/` is kept, as is the text with the replacement character written out.
    #[cfg(unix)]
    #[test]
    fn a_file_name_prefix_that_is_not_text_is_judged_by_its_bytes() {
        use std::os::unix::ffi::OsStrExt;

        for name in FILE_NAME_PREFIXES {
            assert!(
                withheld_bytes(name, OsStr::from_bytes(b"../\xff")),
                "{name}: a value that is not text and has a slash was passed on"
            );
            assert!(
                !withheld_bytes(name, OsStr::from_bytes(b"GNOME\xff")),
                "{name}: a value that is not text and has no slash was removed"
            );
            assert!(!withheld(name, "GNOME\u{fffd}"), "{name}");
        }
    }

    /// CHATGPT-13: the opener is started with the variables [CRED-14] removes left out and the
    /// arguments it was given. A start that inherited everything would hand this agent's own
    /// credentials to whatever `open` or `xdg-open` resolves to.
    ///
    /// The set is read in an environment holding no variable that selects code, so what the
    /// SANDBOX-30 check takes off for this process, such as an `LD_LIBRARY_PATH` it was started
    /// with, is not read as a credential. The start this process makes is then held to leaving out
    /// every one of them.
    #[test]
    fn the_opener_is_not_handed_this_agents_own_credentials() {
        let program = Resolved::found_earlier("/usr/bin/open");
        let arguments = vec!["https://example.invalid/".to_string()];

        let command = opening_against(&program, &arguments, std::iter::empty(), &|_: &Path| true);

        let mut expected = bravebot_config::scrub::own_credentials();
        expected.sort();
        assert_eq!(withheld_from(&command), expected);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["https://example.invalid/"]
        );

        let started = withheld_from(&opening(&program, &arguments));
        assert!(
            expected.iter().all(|name| started.contains(name)),
            "the opener this process starts was handed a credential: {started:?}"
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
        from_checkout: bool,
    ) {
        use std::os::unix::fs::PermissionsExt;
        const SESSION: &str = "BRAVEBOT_TEST_OPENER_SESSION";

        if let Some(session) = std::env::var_os(SESSION) {
            let session = std::path::PathBuf::from(session);
            std::fs::write(session.join("ran"), "").unwrap();
            if from_checkout {
                // The command ran in `<session>/sub`, below a checkout whose `bin` is first on
                // `PATH`; only the directory it ran in is named.
                bravebot_sandbox::programs::keep_out_with_checkout(&session.join("sub"));
            } else {
                bravebot_sandbox::programs::keep_out(&session);
            }

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
        let planted = if from_checkout {
            std::fs::create_dir_all(session.join(".git")).unwrap();
            std::fs::create_dir_all(session.join("sub")).unwrap();
            session.join("bin")
        } else {
            session.join(".venv/bin")
        };
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
            false,
        );
    }

    /// SANDBOX-30, the checkout: the command ran in a subdirectory of a checkout whose `bin` is first
    /// on `PATH` (`PATH_add bin`), as `bravebot auth login chatgpt` can. A file named `open` or
    /// `xdg-open` there is not the opener, though only the subdirectory was named. Rejects a record
    /// of the directory alone, which finds the planted opener in the checkout's `bin`.
    #[cfg(unix)]
    #[test]
    fn a_browser_opener_in_the_bin_of_the_checkout_holding_the_directory_is_not_the_one_started() {
        through_open_address(
            "opener::tests::a_browser_opener_in_the_bin_of_the_checkout_holding_the_directory_is_not_the_one_started",
            "checkout",
            |session| format!("#!/bin/sh\n: > '{session}/installed-opener-ran'\n"),
            "installed-opener-ran",
            true,
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
            false,
        );
    }
}
