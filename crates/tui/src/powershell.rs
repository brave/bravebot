//! The checked start of the programs the interface runs for itself, with the variables Windows
//! PowerShell reads to choose code to run.
//!
//! On Windows the clipboard tool (`clipboard.rs`) and `/caffeinate` (`caffeinate.rs`) start
//! `powershell`. Both start their program through [`start`], so one list covers both
//! ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)). Any other program started through
//! it, such as the Linux and macOS clipboard tools, is given the generic table alone.
//!
//! [`CODE_SELECTORS`] was written from documentation. It has not been shown by running `powershell`
//! on Windows that any variable in it changes what runs (#3808).

use bravebot_sandbox::programs::{CodeSelectors, Inherits, Names, Resolved, SearchPath};
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

/// Whether `program`, as a lookup resolved it, is PowerShell: a file named `powershell`, with or
/// without a Windows executable extension and in any case.
fn is_powershell(program: &Path) -> bool {
    bravebot_sandbox::programs::program_name(program).is_some_and(|name| name == "powershell")
}

/// Whether `program`, as a lookup resolved it, is `osascript`.
fn is_osascript(program: &Path) -> bool {
    bravebot_sandbox::programs::program_name(program).is_some_and(|name| name == "osascript")
}

/// The command that starts the tool the lookup found at `program`: the person's whole environment,
/// and the `PATH` reduced to the entries the lookup searched, so a tool that is a script or a
/// launcher starts the programs it names by name from those alone
/// ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)).
///
/// The generic names are removed when their value names a file a stage may write. PowerShell has
/// [`CODE_SELECTORS`] removed as well, `osascript` has [`OSASCRIPT_CODE_SELECTORS`], and no other
/// tool has a list.
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
    } else {
        bravebot_sandbox::programs::start_against(
            program,
            Inherits::All,
            SearchPath::Reduced,
            // A clipboard or inhibitor tool other than PowerShell and `osascript`: the generic
            // table alone (SANDBOX-30, empty-list starts).
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
}
