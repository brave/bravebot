//! The checked start of the programs the interface runs for itself, with the variables Windows
//! PowerShell reads to choose code to run.
//!
//! On Windows the clipboard tool (`clipboard.rs`) and `/caffeinate` (`caffeinate.rs`) start
//! `powershell`. Both start their program through [`start`], so one list covers both
//! ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)). On Linux the same start runs
//! `systemd-inhibit` and `wl-copy`, which have lists of their own
//! ([`SYSTEMD_INHIBIT_CODE_SELECTORS`], [`WL_COPY_CODE_SELECTORS`]). Any other program started
//! through it, such as `xclip`, `xsel`, `pbcopy` and `clip`, is given the generic table alone.
//!
//! [`CODE_SELECTORS`] was written from documentation. It has not been shown by running `powershell`
//! on Windows that any variable in it changes what runs (#3808). The Linux lists were written from
//! source and then run on Linux (glibc 2.42, systemd 257, xdg-utils 1.2.1): the bus-address names
//! have a test that starts the real program, and SANDBOX-30 records what each program did.

use bravebot_sandbox::programs::{CodeSelectors, Inherits, Names, Resolved, SearchPath, selectors};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The variables Windows PowerShell and the .NET runtime it runs on read to choose code to run, and
/// what each names. They are judged on top of the generic names
/// ([`bravebot_sandbox::programs::CODE_SELECTORS`]) every checked start removes.
///
/// The list was read from PowerShell's `about_PSModulePath` topic and from the .NET documentation of
/// the profiling environment variables. It was not shown by running `powershell` on Windows that
/// any of these changes what runs, and it cannot show that no other variable does (#3808).
///
/// - `PSModulePath` is the list of directories PowerShell searches for modules, which it loads and
///   runs when a command or `Import-Module` names a module found in one of them.
/// - `COR_ENABLE_PROFILING` and `COR_PROFILER` turn on, and name by class id, a profiler that the
///   .NET Framework runtime (Windows PowerShell 5.1) loads into the process.
///   `CORECLR_ENABLE_PROFILING` and `CORECLR_PROFILER` do the same for the .NET runtime
///   (PowerShell 7). They name no path and these starts have no use for a profiler, so they are
///   removed whatever they hold.
/// - `COR_PROFILER_PATH` and `CORECLR_PROFILER_PATH`, and their `_32` and `_64` forms for a
///   process of that width, name the library the runtime loads as the profiler.
///
/// A value is kept only when it is outside every directory a stage may write
/// ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)). Both starts pass `-NoProfile`, so
/// no variable that names a profile script is listed.
const CODE_SELECTORS: [(&str, Names); 11] = [
    ("PSModulePath", Names::Directories),
    ("COR_ENABLE_PROFILING", Names::Options),
    ("COR_PROFILER", Names::Options),
    ("COR_PROFILER_PATH", Names::File),
    ("COR_PROFILER_PATH_32", Names::File),
    ("COR_PROFILER_PATH_64", Names::File),
    ("CORECLR_ENABLE_PROFILING", Names::Options),
    ("CORECLR_PROFILER", Names::Options),
    ("CORECLR_PROFILER_PATH", Names::File),
    ("CORECLR_PROFILER_PATH_32", Names::File),
    ("CORECLR_PROFILER_PATH_64", Names::File),
];

/// What `osascript` reads to choose code to run, judged on top of the generic names. The script
/// libraries (`~/Library/Script Libraries`) and scripting additions (`~/Library/ScriptingAdditions`)
/// of the user domain are found under `CFFIXED_USER_HOME` when it is set, and a compiled library
/// there runs when a script uses it. `HOME` does not move them (run on macOS 26.6; SANDBOX-30), so
/// it is not listed. Only `osascript` is given this list, since `CFFIXED_USER_HOME` is not shown to
/// be read by other programs.
const OSASCRIPT_CODE_SELECTORS: [(&str, Names); 1] = [("CFFIXED_USER_HOME", Names::Directory)];

/// What `systemd-inhibit` reads to choose code to run, judged on top of the generic names.
///
/// `systemd-inhibit` connects to the system bus with `sd_bus_default_system`, which reads
/// `DBUS_SYSTEM_BUS_ADDRESS` and accepts a `unixexec:` address, for which sd-bus runs the program
/// the address names (systemd v255 `src/login/inhibit.c` line 268, `src/libsystemd/sd-bus/sd-bus.c`
/// lines 1280 to 1290 and 1058, `src/libsystemd/sd-bus/bus-socket.c` lines 1066 to 1069). The value
/// is an address and not a path, which no [`Names`] kind judges, so it is removed whatever it holds
/// ([`Names::Options`]) and sd-bus uses its default, `unix:path=/run/dbus/system_bus_socket`. A
/// person whose system bus is elsewhere gets an inhibitor that exits at once, which `/caffeinate`
/// already reports as no hold.
///
/// `DBUS_STARTER_ADDRESS` and `DBUS_STARTER_BUS_TYPE` are read only by `sd_bus_default` and
/// `sd_bus_open`, which `inhibit.c` does not call. `SYSTEMD_PAGER` and `PAGER` are read by
/// `pager_open`, which `inhibit.c` calls only for `--list`, which no start passes. Run on Linux
/// (systemd 257): `DBUS_SYSTEM_BUS_ADDRESS=unixexec:path=<helper>` made `systemd-inhibit` run the
/// helper, and the other names it reads through `getenv` select no program or file
/// (SANDBOX-30, "What a checked start does not cover").
const SYSTEMD_INHIBIT_CODE_SELECTORS: [(&str, Names); 1] =
    [("DBUS_SYSTEM_BUS_ADDRESS", Names::Options)];

/// What `wl-copy` and the programs it starts read to choose code to run, judged on top of the
/// generic names.
///
/// The start passes no `--type`, so `wl-copy` runs `xdg-mime query filetype <file>` for every copy
/// (wl-clipboard v2.2.1 `src/wl-copy.c` lines 266 to 267, `src/util/files.c` lines 141 to 173).
/// `xdg-mime` is a shell script that runs `gio info`, `kmimetypefinder`, `qtxdg-mat` or `mimetype`
/// by name, depending on the desktop (xdg-utils v1.2.1 `scripts/xdg-mime.in` lines 63 to 129 and
/// 684 to 700).
///
/// - `GIO_EXTRA_MODULES` and `GIO_MODULE_DIR` are directories GLib loads modules from when `gio`
///   starts (GLib 2.80.0 `gio/giomodule.c` lines 1310 and 1249).
/// - `QT_PLUGIN_PATH` is the list of directories a Qt program loads plug-ins from
///   (Qt 5.15.12 `qcoreapplication.cpp` lines 2728 and 2939), as `kmimetypefinder` is.
/// - `PERL5LIB` and `PERLLIB` are directories Perl adds to `@INC` and loads modules from when it is
///   not in taint mode, as the Perl `mimetype` script is (run on Linux: a planted module in either
///   was loaded by `mimetype`, which is not in taint mode). `PERL5OPT` is in the generic table.
/// - `DBUS_SESSION_BUS_ADDRESS` is read by `dbus-send`, which `xdg-mime` runs from its desktop
///   detection when `XDG_CURRENT_DESKTOP`, `KDE_FULL_SESSION`, `GNOME_DESKTOP_SESSION_ID` and
///   `MATE_DESKTOP_SESSION_ID` are all unset (xdg-utils 1.2.1 `xdg-mime`, the `dbus-send` call in
///   `detectDE`). libdbus accepts a `unixexec:` address and runs the program its `path=` names.
///   Run on Linux: an address `unixexec:path=<helper>` made `xdg-mime query filetype` run the
///   helper. The address is a [`Names::BusAddress`], so the person's own `unix:path=` bus outside
///   every directory a stage may write is kept. `gio info` ran no helper for the same address, since
///   GDBus has no `unixexec:` transport.
///
/// The tools `xdg-mime` starts are not all run. `kmimetypefinder` and `qtxdg-mat` are not installed
/// on the host the run was made on, so the list can be short. Passing `--type` would stop
/// `xdg-mime` being run and is not done because it declares a type the content may not have.
/// `WL_SEAT` is not read by `wl-copy`. `wl-copy` itself was not installed on that host, so the
/// chain from `wl-copy` to `xdg-mime` is read from its source (SANDBOX-30, "What a checked start
/// does not cover").
const WL_COPY_CODE_SELECTORS: [(&str, Names); 6] = [
    selectors::GIO_EXTRA_MODULES,
    selectors::GIO_MODULE_DIR,
    selectors::QT_PLUGIN_PATH,
    ("PERL5LIB", Names::Directories),
    ("PERLLIB", Names::Directories),
    ("DBUS_SESSION_BUS_ADDRESS", Names::BusAddress),
];

/// Whether `program`, as a lookup resolved it, is PowerShell: a file named `powershell`, with or
/// without a Windows executable extension and in any case.
fn is_powershell(program: &Path) -> bool {
    bravebot_sandbox::programs::program_name(program).is_some_and(|name| name == "powershell")
}

/// Whether `program`, as a lookup resolved it, is `osascript`.
fn is_osascript(program: &Path) -> bool {
    bravebot_sandbox::programs::program_name(program).is_some_and(|name| name == "osascript")
}

/// Whether `program`, as a lookup resolved it, is `systemd-inhibit`.
fn is_systemd_inhibit(program: &Path) -> bool {
    bravebot_sandbox::programs::program_name(program).is_some_and(|name| name == "systemd-inhibit")
}

/// Whether `program`, as a lookup resolved it, is `wl-copy`.
fn is_wl_copy(program: &Path) -> bool {
    bravebot_sandbox::programs::program_name(program).is_some_and(|name| name == "wl-copy")
}

/// The command that starts the tool the lookup found at `program`: the person's whole environment,
/// and the `PATH` reduced to the entries the lookup searched, so a tool that is a script or a
/// launcher starts the programs it names by name from those alone
/// ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)).
///
/// The generic names are removed when their value names a file a stage may write. PowerShell has
/// [`CODE_SELECTORS`] removed as well, `osascript` has [`OSASCRIPT_CODE_SELECTORS`],
/// `systemd-inhibit` has [`SYSTEMD_INHIBIT_CODE_SELECTORS`], `wl-copy` has
/// [`WL_COPY_CODE_SELECTORS`], and no other tool has a list.
pub(crate) fn start(program: PathBuf) -> Command {
    start_against(
        &Resolved::found_earlier(program),
        std::env::vars_os(),
        &bravebot_sandbox::programs::outside_stage_writable,
    )
}

/// [`start`] against a given environment and a given judgment of which files are outside the
/// directories a stage may write.
fn start_against(
    program: &Resolved,
    held: impl IntoIterator<Item = (OsString, OsString)>,
    outside_stage_writable: &impl Fn(&Path) -> bool,
) -> Command {
    if is_powershell(program.as_path()) {
        bravebot_sandbox::programs::start_against(
            program,
            Inherits::All,
            SearchPath::Reduced,
            CodeSelectors::Checked(&CODE_SELECTORS),
            held,
            outside_stage_writable,
        )
    } else if is_osascript(program.as_path()) {
        bravebot_sandbox::programs::start_against(
            program,
            Inherits::All,
            SearchPath::Reduced,
            CodeSelectors::Checked(&OSASCRIPT_CODE_SELECTORS),
            held,
            outside_stage_writable,
        )
    } else if is_systemd_inhibit(program.as_path()) {
        bravebot_sandbox::programs::start_against(
            program,
            Inherits::All,
            SearchPath::Reduced,
            CodeSelectors::Checked(&SYSTEMD_INHIBIT_CODE_SELECTORS),
            held,
            outside_stage_writable,
        )
    } else if is_wl_copy(program.as_path()) {
        bravebot_sandbox::programs::start_against(
            program,
            Inherits::All,
            SearchPath::Reduced,
            CodeSelectors::Checked(&WL_COPY_CODE_SELECTORS),
            held,
            outside_stage_writable,
        )
    } else {
        bravebot_sandbox::programs::start_against(
            program,
            Inherits::All,
            SearchPath::Reduced,
            // A clipboard tool other than PowerShell, `osascript` and `wl-copy`: the generic table
            // alone (SANDBOX-30, empty-list starts). `xclip` 0.13 and `xsel` 1.2.1 read no
            // variable of their own, and libX11 1.8.7, libxcb 1.16 and libXau 1.0.11 read only
            // names that select a socket, a credential file or data (`DISPLAY`, `XAUTHORITY`,
            // `XENVIRONMENT`, `XLOCALEDIR`, `XCOMPOSEFILE`). `pbcopy` and `clip` read none. Run
            // on Linux for `xclip` 0.13 under Xvfb; `xsel` was not installed. The sources and the
            // run are in the SANDBOX-30 bullet.
            CodeSelectors::Checked(&[]),
            held,
            outside_stage_writable,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::scratch_dir;
    use std::ffi::OsStr;

    /// A directory and a library a stage may write, and a genuine pair beside them that is outside
    /// every directory a stage may write.
    struct Fixture {
        session: PathBuf,
        safe: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let session = scratch_dir(&format!("powershell-{name}-session"));
            let safe = scratch_dir(&format!("powershell-{name}-safe"));
            for root in [&session, &safe] {
                let _ = std::fs::remove_dir_all(root);
                std::fs::create_dir_all(root.join("modules")).unwrap();
                std::fs::write(root.join("profiler.dll"), "").unwrap();
            }
            Self { session, safe }
        }

        fn planted_directory(&self) -> PathBuf {
            self.session.join("modules")
        }

        fn genuine_directory(&self) -> PathBuf {
            self.safe.join("modules")
        }

        fn planted_file(&self) -> PathBuf {
            self.session.join("profiler.dll")
        }

        fn genuine_file(&self) -> PathBuf {
            self.safe.join("profiler.dll")
        }
    }

    /// The command that starts `program` when the person's environment holds `value` for `name` and
    /// the session's directory is the one a stage may write.
    fn started(program: &str, name: &str, value: impl AsRef<OsStr>, fixture: &Fixture) -> Command {
        let writable = vec![fixture.session.clone()];
        start_against(
            &Resolved::found_earlier(program),
            [(OsString::from(name), value.as_ref().to_os_string())],
            &|file: &Path| {
                bravebot_sandbox::programs::outside_stage_writable_with(file, &writable, &[])
            },
        )
    }

    /// Whether the start of `powershell` takes `name` off the environment, when the person's
    /// environment holds `value` for it.
    fn withheld(name: &str, value: impl AsRef<OsStr>, fixture: &Fixture) -> bool {
        started("/windows/powershell.exe", name, value, fixture)
            .get_envs()
            .any(|(set, now)| set == OsStr::new(name) && now.is_none())
    }

    /// SANDBOX-30. The start of `program` takes `name` off the environment when the person's
    /// environment holds `value` for it.
    fn removed_by(program: &str, name: &str, value: impl AsRef<OsStr>, fixture: &Fixture) -> bool {
        started(program, name, value, fixture)
            .get_envs()
            .any(|(set, now)| set == OsStr::new(name) && now.is_none())
    }

    /// SANDBOX-30. `systemd-inhibit` connects to the system bus with sd-bus, which runs the
    /// program a `unixexec:` address names. The address is not a path, so it is removed whatever it
    /// holds, and sd-bus uses its default system bus. The list is from systemd v255 source and was
    /// run on Linux by the test below. The regression it rejects: `systemd-inhibit` started with the generic
    /// table alone, so a `unixexec:` address naming a program a stage wrote reaches it; the name
    /// classed as a directory list, so a value that is an absolute safe directory is kept; or the
    /// list given to every program started through [`start`].
    #[test]
    fn a_system_bus_address_is_removed_from_systemd_inhibit_whatever_it_holds() {
        let fixture = Fixture::new("system-bus");
        let withheld = |name: &str, value: &OsStr| {
            removed_by("/usr/bin/systemd-inhibit", name, value, &fixture)
        };
        let planted = format!("unixexec:path={}", fixture.planted_file().display());
        let genuine = format!("unixexec:path={}", fixture.genuine_file().display());

        assert!(withheld("DBUS_SYSTEM_BUS_ADDRESS", OsStr::new(&planted)));
        assert!(withheld("DBUS_SYSTEM_BUS_ADDRESS", OsStr::new(&genuine)));
        assert!(withheld(
            "DBUS_SYSTEM_BUS_ADDRESS",
            OsStr::new("unixexec:path=bravebot-bus-helper")
        ));
        assert!(withheld(
            "DBUS_SYSTEM_BUS_ADDRESS",
            OsStr::new("unix:path=/run/dbus/system_bus_socket")
        ));
        assert!(withheld(
            "DBUS_SYSTEM_BUS_ADDRESS",
            fixture.genuine_directory().as_os_str()
        ));
        assert!(withheld("DBUS_SYSTEM_BUS_ADDRESS", OsStr::new("")));

        // Only the name listed for it is removed, and only from `systemd-inhibit`.
        assert!(!withheld("DBUS_SESSION_BUS_ADDRESS", OsStr::new(&planted)));
        assert!(!withheld(
            "XDG_RUNTIME_DIR",
            fixture.planted_directory().as_os_str()
        ));
        for program in [
            "/usr/bin/xclip",
            "/usr/bin/xsel",
            "/usr/bin/wl-copy",
            "/windows/powershell.exe",
            "/usr/bin/osascript",
        ] {
            assert!(
                !removed_by(program, "DBUS_SYSTEM_BUS_ADDRESS", &planted, &fixture),
                "{program}"
            );
        }
        for found in ["systemd-inhibit", "/usr/bin/systemd-inhibit"] {
            assert!(is_systemd_inhibit(Path::new(found)), "{found}");
        }
        for found in ["systemd-inhibit-helper", "systemd-run", "inhibit", ""] {
            assert!(!is_systemd_inhibit(Path::new(found)), "{found}");
        }
    }

    /// SANDBOX-30. `wl-copy` is started with no `--type`, so it runs `xdg-mime`, which runs `gio
    /// info`, `kmimetypefinder` or the Perl `mimetype` by name. The list is from the source of
    /// wl-clipboard v2.2.1, xdg-utils v1.2.1, GLib 2.80.0, Qt 5.15.12 and perl v5.38.0, and the
    /// session bus name was run on Linux by the test below. The regression it rejects: `wl-copy`
    /// started with the generic table
    /// alone, so a module, plug-in or Perl library directory a stage wrote reaches the tools
    /// `xdg-mime` starts; a list name judged by its first entry or as one path, so a list with a
    /// stage-writable entry among safe ones is kept; `GIO_MODULE_DIR` read as a list, so two
    /// directories joined are kept; a name classed as options, so a safe directory is removed; or
    /// `DBUS_SESSION_BUS_ADDRESS` left out, so a `unixexec:` address naming a program a stage wrote
    /// reaches the `dbus-send` that `xdg-mime` starts, or classed as options, so the person's own
    /// session bus is removed from the copy.
    #[test]
    fn the_directories_xdg_mime_would_load_code_from_are_removed_from_wl_copy_when_a_stage_may_write_them()
     {
        let fixture = Fixture::new("wl-copy");
        let join = |entries: &[PathBuf]| std::env::join_paths(entries).unwrap();
        let planted = fixture.planted_directory();
        let genuine = fixture.genuine_directory();
        let mixed = join(&[genuine.clone(), planted.clone()]);
        let safe_list = join(&[genuine.clone(), fixture.safe.clone()]);
        let withheld =
            |name: &str, value: &OsStr| removed_by("/usr/bin/wl-copy", name, value, &fixture);

        for name in ["GIO_EXTRA_MODULES", "QT_PLUGIN_PATH", "PERL5LIB", "PERLLIB"] {
            assert!(withheld(name, planted.as_os_str()), "{name}");
            assert!(withheld(name, &mixed), "{name} kept {mixed:?}");
            assert!(withheld(name, OsStr::new("modules")), "{name}");
            assert!(withheld(name, OsStr::new("")), "{name}");
            assert!(!withheld(name, genuine.as_os_str()), "{name}");
            assert!(!withheld(name, &safe_list), "{name}");
        }
        assert!(withheld("GIO_EXTRA_MODULES", planted.as_os_str()));
        assert!(!withheld("GIO_EXTRA_MODULES", genuine.as_os_str()));
        assert!(withheld("QT_PLUGIN_PATH", planted.as_os_str()));
        assert!(!withheld("QT_PLUGIN_PATH", genuine.as_os_str()));
        assert!(withheld("PERL5LIB", planted.as_os_str()));
        assert!(!withheld("PERL5LIB", genuine.as_os_str()));
        assert!(withheld("PERLLIB", planted.as_os_str()));
        assert!(!withheld("PERLLIB", genuine.as_os_str()));

        // One directory.
        assert!(withheld("GIO_MODULE_DIR", planted.as_os_str()));
        assert!(withheld("GIO_MODULE_DIR", &safe_list));
        assert!(withheld("GIO_MODULE_DIR", OsStr::new("modules")));
        assert!(!withheld("GIO_MODULE_DIR", genuine.as_os_str()));

        // The session bus: an address that names a program, or a socket a stage may write, is
        // removed whole, and the person's own socket is kept.
        let address = |path: &Path| OsString::from(format!("unix:path={}", path.display()));
        let genuine_bus = address(&fixture.genuine_file());
        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            &OsString::from(format!(
                "unixexec:path={}",
                genuine.join("helper").display()
            ))
        ));
        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            &address(&fixture.planted_file())
        ));
        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            &OsString::from(format!(
                "{};unixexec:path=bravebot-bus-helper",
                genuine_bus.to_string_lossy()
            ))
        ));
        assert!(withheld("DBUS_SESSION_BUS_ADDRESS", OsStr::new("")));
        assert!(!withheld("DBUS_SESSION_BUS_ADDRESS", &genuine_bus));

        // Only `wl-copy` is given these names, and `wl-copy` is not given another program's.
        for program in [
            "/usr/bin/xclip",
            "/usr/bin/xsel",
            "/usr/bin/systemd-inhibit",
            "/windows/powershell.exe",
        ] {
            assert!(
                !removed_by(program, "PERL5LIB", planted.as_os_str(), &fixture),
                "{program}"
            );
            assert!(
                !removed_by(
                    program,
                    "DBUS_SESSION_BUS_ADDRESS",
                    OsStr::new(&format!("unixexec:path={}", planted.display())),
                    &fixture
                ),
                "{program}"
            );
        }
        assert!(!withheld("PSModulePath", planted.as_os_str()));
        assert!(!withheld("DBUS_SYSTEM_BUS_ADDRESS", planted.as_os_str()));
        // `wl-copy` does not read `WL_SEAT`, so it is left as set.
        assert!(!withheld("WL_SEAT", planted.as_os_str()));
        for found in ["wl-copy", "/usr/bin/wl-copy"] {
            assert!(is_wl_copy(Path::new(found)), "{found}");
        }
        for found in ["wl-paste", "wl-copy-helper", "xclip", ""] {
            assert!(!is_wl_copy(Path::new(found)), "{found}");
        }
    }

    /// SANDBOX-30. `xclip` and `xsel` read no variable of their own that selects code (xclip 0.13,
    /// xsel 1.2.1, libX11 1.8.7, libxcb 1.16, libXau 1.0.11 source), so they are given the generic
    /// table alone and the names that select data are left as set. The regression it rejects: the
    /// `wl-copy` or `systemd-inhibit` names given to the X tools, or `DISPLAY` and `XAUTHORITY`
    /// removed, so a person on X loses their clipboard.
    #[test]
    fn the_x_clipboard_tools_are_given_the_generic_names_alone() {
        let fixture = Fixture::new("x-tools");
        let planted = fixture.planted_directory();
        for program in ["/usr/bin/xclip", "/usr/bin/xsel"] {
            for name in [
                "DISPLAY",
                "XAUTHORITY",
                "XENVIRONMENT",
                "XLOCALEDIR",
                "XCOMPOSEFILE",
                "WAYLAND_DISPLAY",
                "GIO_EXTRA_MODULES",
                "PERL5LIB",
                "DBUS_SYSTEM_BUS_ADDRESS",
            ] {
                assert!(
                    !removed_by(program, name, planted.as_os_str(), &fixture),
                    "{name} was removed from the start of {program}"
                );
            }
            // The generic names are still removed.
            assert!(
                removed_by(program, "LD_PRELOAD", planted.join("lib.so"), &fixture),
                "{program}"
            );
        }
    }

    /// SANDBOX-30. The list is from documentation and not from running `powershell`. The regression
    /// it rejects: `PSModulePath` not listed, so PowerShell loads a module from a directory a stage
    /// wrote; `PSModulePath` judged by its first entry or as one path, so a list with a
    /// stage-writable entry among safe ones is kept; or classed as options, so a safe list is
    /// removed.
    #[test]
    fn a_module_path_with_a_directory_a_stage_may_write_is_removed_whole_and_a_safe_one_is_kept() {
        let fixture = Fixture::new("module-path");
        let join = |entries: &[PathBuf]| std::env::join_paths(entries).unwrap();
        let planted = fixture.planted_directory();
        let genuine = fixture.genuine_directory();

        assert!(withheld("PSModulePath", &planted, &fixture));
        assert!(withheld(
            "PSModulePath",
            join(&[genuine.clone(), planted.clone()]),
            &fixture
        ));
        assert!(withheld(
            "PSModulePath",
            join(&[planted, genuine.clone()]),
            &fixture
        ));
        assert!(withheld("PSModulePath", "modules", &fixture));
        assert!(withheld(
            "PSModulePath",
            fixture.safe.join("missing"),
            &fixture
        ));
        assert!(withheld("PSModulePath", "", &fixture));

        assert!(!withheld("PSModulePath", &genuine, &fixture));
        assert!(!withheld(
            "PSModulePath",
            join(&[genuine, fixture.safe.clone()]),
            &fixture
        ));
    }

    /// SANDBOX-30. The list is from documentation and not from running `powershell`. The regression
    /// it rejects: a profiler library name left out of the list, so the runtime loads a library a
    /// stage wrote; a name classed as a directory, so a directory passes for a file; or classed as
    /// options, so a library outside every directory a stage may write is removed.
    #[test]
    fn a_profiler_library_a_stage_may_write_is_removed_and_a_safe_file_is_kept() {
        let fixture = Fixture::new("profiler-path");
        let planted = fixture.planted_file();
        let genuine = fixture.genuine_file();

        assert!(withheld("COR_PROFILER_PATH", &planted, &fixture));
        assert!(!withheld("COR_PROFILER_PATH", &genuine, &fixture));
        assert!(withheld("COR_PROFILER_PATH_32", &planted, &fixture));
        assert!(!withheld("COR_PROFILER_PATH_32", &genuine, &fixture));
        assert!(withheld("COR_PROFILER_PATH_64", &planted, &fixture));
        assert!(!withheld("COR_PROFILER_PATH_64", &genuine, &fixture));
        assert!(withheld("CORECLR_PROFILER_PATH", &planted, &fixture));
        assert!(!withheld("CORECLR_PROFILER_PATH", &genuine, &fixture));
        assert!(withheld("CORECLR_PROFILER_PATH_32", &planted, &fixture));
        assert!(!withheld("CORECLR_PROFILER_PATH_32", &genuine, &fixture));
        assert!(withheld("CORECLR_PROFILER_PATH_64", &planted, &fixture));
        assert!(!withheld("CORECLR_PROFILER_PATH_64", &genuine, &fixture));

        for (variable, names) in CODE_SELECTORS {
            if names != Names::File {
                continue;
            }
            assert!(
                withheld(variable, fixture.genuine_directory(), &fixture),
                "{variable} accepted a directory for a file"
            );
            assert!(
                withheld(variable, "profiler.dll", &fixture),
                "{variable} accepted a relative path"
            );
            assert!(
                withheld(variable, fixture.safe.join("missing.dll"), &fixture),
                "{variable} accepted a path that does not exist"
            );
        }
    }

    /// SANDBOX-30. The list is from documentation and not from running `powershell`. The switches
    /// and class ids name no path, and the starts have no use for a profiler, so they are removed
    /// whatever they hold. The regression it rejects: one of them classed as a program or a file
    /// and kept for a value that reads as a bare name or a safe path.
    #[test]
    fn a_profiler_switch_or_class_id_is_removed_whatever_it_holds() {
        let fixture = Fixture::new("profiler-switch");
        let class_id = "{00000000-0000-0000-0000-000000000000}";
        let genuine = fixture.genuine_file();

        assert!(withheld("COR_ENABLE_PROFILING", "1", &fixture));
        assert!(withheld("COR_ENABLE_PROFILING", &genuine, &fixture));
        assert!(withheld("COR_PROFILER", class_id, &fixture));
        assert!(withheld("COR_PROFILER", &genuine, &fixture));
        assert!(withheld("COR_PROFILER", "profiler", &fixture));
        assert!(withheld("CORECLR_ENABLE_PROFILING", "1", &fixture));
        assert!(withheld("CORECLR_ENABLE_PROFILING", &genuine, &fixture));
        assert!(withheld("CORECLR_PROFILER", class_id, &fixture));
        assert!(withheld("CORECLR_PROFILER", &genuine, &fixture));
        assert!(withheld("CORECLR_PROFILER", "profiler", &fixture));
    }

    /// The regression it rejects: the list applied by removing every variable, or a variable added
    /// to it that these starts need, so a person who set `APPDATA` or `HOME` loses it.
    #[test]
    fn a_variable_that_selects_no_code_is_left_as_set_for_powershell() {
        let fixture = Fixture::new("unrelated");
        let planted = fixture.planted_directory();

        for name in [
            "APPDATA",
            "LOCALAPPDATA",
            "USERPROFILE",
            "HOME",
            "TEMP",
            "PSExecutionPolicyPreference",
            "COR_NOT_A_PROFILER_VARIABLE",
        ] {
            assert!(!withheld(name, &planted, &fixture), "{name} was removed");
        }
    }

    /// SANDBOX-30. The regression it rejects: the PowerShell list given to every program started
    /// through [`start`], so a person's `PSModulePath` is removed from the other clipboard tools,
    /// which do not read it.
    #[test]
    fn a_program_other_than_powershell_is_not_given_the_powershell_names() {
        let fixture = Fixture::new("not-powershell");
        for program in ["/usr/bin/xclip", "/windows/clip.exe", "/usr/bin/osascript"] {
            for (variable, _) in CODE_SELECTORS {
                let command = started(
                    program,
                    variable,
                    fixture.planted_directory().into_os_string(),
                    &fixture,
                );
                assert!(
                    command
                        .get_envs()
                        .all(|(set, now)| set != OsStr::new(variable) || now.is_some()),
                    "{variable} was removed from the start of {program}"
                );
            }
        }
    }

    /// SANDBOX-30, run on macOS 26.6: `osascript` loads a compiled script library from
    /// `Library/Script Libraries` under `CFFIXED_USER_HOME`. The regression it rejects: the name
    /// not listed for `osascript`, so a library a stage wrote runs; classed as options, so a safe
    /// directory is removed; or listed as `HOME`, which `osascript` does not follow.
    #[test]
    fn osascript_has_a_user_home_it_would_load_script_libraries_from_removed_when_a_stage_may_write_it()
     {
        let fixture = Fixture::new("osascript-home");
        let planted = fixture.planted_directory();
        let genuine = fixture.genuine_directory();
        let removed = |program: &str, name: &str, value: &Path| {
            started(program, name, value, &fixture)
                .get_envs()
                .any(|(set, now)| set == OsStr::new(name) && now.is_none())
        };
        let withheld = |name: &str, value: &Path| removed("/usr/bin/osascript", name, value);

        assert!(withheld("CFFIXED_USER_HOME", &planted));
        assert!(withheld("CFFIXED_USER_HOME", Path::new("modules")));
        assert!(withheld("CFFIXED_USER_HOME", &fixture.safe.join("missing")));
        assert!(!withheld("CFFIXED_USER_HOME", &genuine));
        // `HOME` is not what `osascript` follows, so it is left as set.
        assert!(!withheld("HOME", &planted));
        // Another tool, PowerShell included, is not given the name.
        for program in ["/windows/powershell.exe", "/usr/bin/xclip"] {
            assert!(
                !removed(program, "CFFIXED_USER_HOME", &planted),
                "{program}"
            );
        }
        // `osascript` is not given PowerShell's names.
        assert!(!withheld("PSModulePath", &planted));
        assert!(is_osascript(Path::new("/usr/bin/osascript")));
        assert!(!is_osascript(Path::new("/usr/bin/osascript-helper")));
    }

    /// SANDBOX-30, run on macOS: a compiled script library a stage wrote under
    /// `CFFIXED_USER_HOME` runs when `osascript` uses it, and the checked start does not let that
    /// happen. The control (the name set, the start not applied) must run the library, so the check
    /// cannot pass because the library never loads. Prints `skipped` and passes where `osascript`
    /// or `osacompile` is absent, and fails under `CI` on macOS.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_script_library_under_a_planted_user_home_does_not_run_through_the_checked_start() {
        let (osascript, osacompile) = (
            Path::new("/usr/bin/osascript"),
            Path::new("/usr/bin/osacompile"),
        );
        if !osascript.exists() || !osacompile.exists() {
            assert!(
                std::env::var_os("CI").is_none(),
                "osascript or osacompile is absent under CI"
            );
            eprintln!("skipped: osascript or osacompile is absent");
            return;
        }
        let base = scratch_dir("osascript-real-run");
        let _ = std::fs::remove_dir_all(&base);
        let home = base.join("home");
        let libraries = home.join("Library").join("Script Libraries");
        std::fs::create_dir_all(&libraries).unwrap();
        let marker = base.join("marker");
        let source = base.join("Marker3809.applescript");
        std::fs::write(
            &source,
            format!(
                "on ping()\n  do shell script \"echo ran >> '{}'\"\n  return 1\nend ping\n",
                marker.display()
            ),
        )
        .unwrap();
        let compiled = Command::new(osacompile)
            .arg("-o")
            .arg(libraries.join("Marker3809.scpt"))
            .arg(&source)
            .env_clear()
            .output()
            .unwrap();
        assert!(compiled.status.success(), "{compiled:?}");
        let home = home.canonicalize().unwrap();
        let session = base.canonicalize().unwrap();
        let args = [
            "-e",
            "use lib : script \"Marker3809\"",
            "-e",
            "return lib's ping()",
        ];

        // Control: the name set as the person would set it, with no start applied.
        let control = Command::new(osascript)
            .args(args)
            .env_clear()
            .env("CFFIXED_USER_HOME", &home)
            .output()
            .unwrap();
        assert!(control.status.success(), "{control:?}");
        assert!(marker.exists(), "the control did not run the library");
        std::fs::remove_file(&marker).unwrap();

        // The checked start, with the session the one directory a stage may write.
        let writable = vec![session];
        let mut start = start_against(
            &Resolved::found_earlier(osascript),
            [(
                OsString::from("CFFIXED_USER_HOME"),
                home.clone().into_os_string(),
            )],
            &|file: &Path| {
                bravebot_sandbox::programs::outside_stage_writable_with(file, &writable, &[])
            },
        );
        let removed = start
            .get_envs()
            .any(|(set, now)| set == OsStr::new("CFFIXED_USER_HOME") && now.is_none());
        assert!(removed, "the start kept CFFIXED_USER_HOME");
        let checked = start.args(args).output().unwrap();
        assert!(!checked.status.success(), "{checked:?}");
        assert!(
            !marker.exists(),
            "the library ran through the checked start"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// SANDBOX-30. The list is given to the program the lookup resolved to, which on Windows keeps
    /// its extension and may be in any case. The regression it rejects: the name compared as
    /// written, so `powershell.exe` is not PowerShell and the generic table alone applies.
    #[test]
    fn powershell_is_known_by_its_name_whatever_extension_or_case_the_lookup_found() {
        for found in [
            "powershell",
            "powershell.exe",
            "POWERSHELL.EXE",
            "/windows/system32/WindowsPowerShell/v1.0/powershell.exe",
        ] {
            assert!(is_powershell(Path::new(found)), "{found}");
        }
        for found in [
            "clip.exe",
            "xclip",
            "not-powershell.exe",
            "powershell-helper",
            "",
        ] {
            assert!(!is_powershell(Path::new(found)), "{found}");
        }
    }

    /// SANDBOX-30. Every start of a program in `clipboard.rs` and `caffeinate.rs`, which hold the
    /// starts of `powershell`, goes through [`start`]. The regression it rejects: a start written
    /// out again in either file with `CodeSelectors::Checked(&[])`, which removes the generic names
    /// alone and leaves `PSModulePath` and the profiler variables to reach PowerShell.
    #[test]
    fn the_clipboard_tool_and_the_inhibitor_start_powershell_through_the_checked_start() {
        for (file, source) in [
            ("clipboard.rs", include_str!("clipboard.rs")),
            ("caffeinate.rs", include_str!("caffeinate.rs")),
        ] {
            assert!(
                source.contains("crate::powershell::start("),
                "{file} does not start its program through powershell::start"
            );
            assert!(
                !source.contains("CodeSelectors::Checked(&[])"),
                "{file} starts a program with only the generic names checked"
            );
            assert!(
                !source.contains(concat!("programs", "::start(")),
                "{file} starts a program without powershell::start"
            );
        }
    }

    /// Start `program_name` with `arguments` holding a `unixexec:` address that names a helper in
    /// `name`, and say whether the helper ran, or `None` where a program it needs is not installed.
    ///
    /// With `checked` false the start keeps the environment as a person's would hold it. With
    /// `checked` true the start is the one `/caffeinate` and the clipboard make, and the variable
    /// reaches the program only if that start did not remove it. A `stand_in` is a script written
    /// as `program_name` in the session directory, for a program that is not installed.
    #[cfg(target_os = "linux")]
    fn a_unixexec_address_runs_the_helper(
        label: &str,
        program_name: &str,
        stand_in: Option<&str>,
        arguments: &[&str],
        name: &str,
        checked: bool,
    ) -> Option<bool> {
        use std::os::unix::fs::PermissionsExt;
        let executable = |path: &Path, body: &str| {
            std::fs::write(path, body).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let session = scratch_dir(&format!("powershell-real-{label}"));
        let _ = std::fs::remove_dir_all(&session);
        std::fs::create_dir_all(session.join("bin")).unwrap();
        let session = session.canonicalize().unwrap();
        let program = match stand_in {
            Some(body) => {
                let program = session.join("bin").join(program_name);
                executable(&program, body);
                program
            }
            None => bravebot_sandbox::programs::find(OsStr::new(program_name))?,
        };
        let marker = session.join("marker");
        let helper = session.join("helper");
        executable(
            &helper,
            &format!("#!/bin/sh\n: >> '{}'\nexit 1\n", marker.display()),
        );
        std::fs::write(session.join("content.txt"), "text\n").unwrap();
        let address = OsString::from(format!("unixexec:path={}", helper.display()));

        let mut command = if checked {
            let writable = vec![session.clone()];
            let mut command = start_against(
                &Resolved::found_earlier(&program),
                [(OsString::from(name), address.clone())],
                &|file: &Path| {
                    bravebot_sandbox::programs::outside_stage_writable_with(file, &writable, &[])
                },
            );
            // The start settles what a program inherits from this process, which holds no such
            // variable, so the variable reaches the program unless the start removed it.
            let removed = command
                .get_envs()
                .any(|(set, now)| set == OsStr::new(name) && now.is_none());
            if !removed {
                command.env(name, &address);
            }
            command
        } else {
            let mut command = bravebot_sandbox::programs::start(
                &Resolved::found_earlier(&program),
                Inherits::All,
                SearchPath::Inherited,
                CodeSelectors::Kept,
            );
            command.env(name, &address);
            command
        };
        // `xdg-mime` asks `dbus-send` about the session only when no desktop is named.
        for desktop in [
            "XDG_CURRENT_DESKTOP",
            "KDE_FULL_SESSION",
            "GNOME_DESKTOP_SESSION_ID",
            "MATE_DESKTOP_SESSION_ID",
        ] {
            command.env_remove(desktop);
        }
        let _ = command
            .args(arguments)
            .current_dir(&session)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        let ran = marker.exists();
        let _ = std::fs::remove_dir_all(&session);
        Some(ran)
    }

    /// SANDBOX-30, run on Linux (systemd 257): `systemd-inhibit` runs the program a `unixexec:`
    /// address in `DBUS_SYSTEM_BUS_ADDRESS` names. The control starts the real program with the
    /// address and must run the helper, so the check cannot pass because the program never reads
    /// the address. Prints `skipped` and passes where `systemd-inhibit` is not installed. The
    /// regression it rejects: the name left out of `SYSTEMD_INHIBIT_CODE_SELECTORS`, so the
    /// inhibitor `/caffeinate` starts runs a helper a stage wrote.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_unixexec_system_bus_address_runs_a_helper_through_systemd_inhibit_unless_the_start_removes_it()
     {
        let arguments = ["--what=idle", "--who=bravebot-test", "--why=test", "true"];
        let run = |checked| {
            a_unixexec_address_runs_the_helper(
                "systemd-inhibit",
                "systemd-inhibit",
                None,
                &arguments,
                "DBUS_SYSTEM_BUS_ADDRESS",
                checked,
            )
        };
        let Some(control) = run(false) else {
            eprintln!("skipped: systemd-inhibit is not installed");
            return;
        };
        assert!(
            control,
            "systemd-inhibit did not run the helper a unixexec address names"
        );
        assert_eq!(
            run(true),
            Some(false),
            "the checked start left the address for systemd-inhibit"
        );
    }

    /// SANDBOX-30, run on Linux (xdg-utils 1.2.1): `wl-copy` starts `xdg-mime query filetype`, and
    /// with no desktop named `xdg-mime` asks `dbus-send` about the session bus, which runs the
    /// program a `unixexec:` address in `DBUS_SESSION_BUS_ADDRESS` names. A stand-in `wl-copy` that
    /// runs that query stands for the program, which is not installed on the host. The control
    /// starts it without the checked start and must run the helper. Prints `skipped` and passes
    /// where `xdg-mime` or `dbus-send` is not installed. The regression it rejects: the name left
    /// out of `WL_COPY_CODE_SELECTORS`, so a helper a stage wrote runs on every copy.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_unixexec_session_bus_address_runs_a_helper_through_the_xdg_mime_wl_copy_starts_unless_the_start_removes_it()
     {
        let installed = |name: &str| bravebot_sandbox::programs::find(OsStr::new(name)).is_some();
        if !(installed("xdg-mime") && installed("dbus-send")) {
            eprintln!("skipped: xdg-mime or dbus-send is not installed");
            return;
        }
        let stand_in = "#!/bin/sh\nexec xdg-mime query filetype content.txt\n";
        let run = |checked| {
            a_unixexec_address_runs_the_helper(
                "wl-copy",
                "wl-copy",
                Some(stand_in),
                &[],
                "DBUS_SESSION_BUS_ADDRESS",
                checked,
            )
        };
        assert_eq!(
            run(false),
            Some(true),
            "xdg-mime did not run the helper a unixexec address names"
        );
        assert_eq!(
            run(true),
            Some(false),
            "the checked start left the address for the programs wl-copy starts"
        );
    }
}
