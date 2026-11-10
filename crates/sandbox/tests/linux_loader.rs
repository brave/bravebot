//! SANDBOX-30: a checked start keeps the Linux dynamic loader from loading a library a stage wrote.
//!
//! The libraries are compiled with `cc` and each one records who it is in a marker file when the
//! loader runs its constructor. A control run of the same program with the variable set and no
//! check shows that the variable selects code at all, so a removal that is tested against a
//! variable the loader ignores cannot pass.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]
#![allow(clippy::disallowed_methods, reason = "the test names its programs")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_sandbox::programs::{
    CodeSelectors, Inherits, Resolved, SearchPath, keep_out, outside_stage_writable, start_against,
};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A library that records `WHO` in `MARKER` when the loader runs it, and exports what a program
/// linked against it, or an audit module, needs.
const LIBRARY: &str = r#"
#include <fcntl.h>
#include <string.h>
#include <unistd.h>

__attribute__((constructor)) static void run(void) {
    int fd = open(MARKER, O_WRONLY | O_CREAT | O_APPEND, 0600);
    if (fd >= 0) {
        const char who[] = WHO "\n";
        ssize_t written = write(fd, who, strlen(who));
        (void)written;
        close(fd);
    }
}

int needed(void) { return 0; }

#ifdef AUDIT
unsigned int la_version(unsigned int version) { return version; }
#endif
"#;

/// A program that loads nothing of its own but the C library.
const PLAIN: &str = "int main(void) { return 0; }\n";

/// A program linked against `libneeded.so`, which it finds through the `RUNPATH` it was linked
/// with, and which `LD_LIBRARY_PATH` is searched before.
const LINKED: &str = "int needed(void);\nint main(void) { return needed(); }\n";

/// A program that asks the C library to convert from a character set only a `gconv-modules` file
/// found through `GCONV_PATH` defines. The module the file names is loaded, and runs its
/// constructor, before the conversion is known to fail. The module exports no converter, so glibc
/// can abort the process while it releases the module, and the exit status is not asserted.
const CONVERTING: &str = r#"
#include <iconv.h>

int main(void) {
    iconv_t converter = iconv_open("UTF-8", "STAGECHARSET");
    if (converter != (iconv_t)-1) {
        iconv_close(converter);
    }
    return 0;
}
"#;

/// A program that reports the value of the variable named by its first argument, or `(unset)`. It
/// loads nothing of its own, so a run shows only what environment a start gave a real process.
const ECHOING: &str = r#"
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv) {
    const char *value = argc > 1 ? getenv(argv[1]) : NULL;
    puts(value ? value : "(unset)");
    return 0;
}
"#;

/// What the loader was asked to load, and the two places a library of the same name can be.
struct Fixture {
    marker: PathBuf,
    planted: PathBuf,
    genuine: PathBuf,
    plain: PathBuf,
    linked: PathBuf,
    converting: PathBuf,
    echoing: PathBuf,
}

/// This test run's directory under cargo's own scratch directory, which is not the system
/// temporary directory a checked start treats as writable.
fn directory(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("linux-loader-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// Whether this machine can compile the fixtures. A CI machine that cannot fails, since a skip
/// there would report green for a loader that was never run.
fn compiler_present() -> bool {
    let present = Command::new("cc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !present {
        assert!(
            std::env::var_os("CI").is_none(),
            "a CI machine must run the loader and has no cc"
        );
        eprintln!("skipped: no C compiler");
    }
    present
}

fn compile(arguments: &[&OsStr]) {
    let output = Command::new("cc").args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "cc {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Set `name` on `command` to `value` unless the start removed it: the child inherits what the
/// parent holds, less what the start took off.
fn holding(command: &mut Command, name: &str, value: &Path) {
    let removed = command
        .get_envs()
        .any(|(set, now)| set == OsStr::new(name) && now.is_none());
    if !removed {
        command.env(name, value);
    }
}

fn define(name: &str, value: impl AsRef<OsStr>) -> OsString {
    let mut definition = OsString::from(format!("-D{name}=\""));
    definition.push(value);
    definition.push("\"");
    definition
}

/// Compile `LIBRARY` as `output`, which records `who`.
fn library(fixture: &Fixture, output: &Path, who: &str, audit: bool, soname: Option<&str>) {
    let source = output.with_extension("c");
    std::fs::write(&source, LIBRARY).unwrap();
    let mut arguments: Vec<OsString> = vec![
        "-shared".into(),
        "-fPIC".into(),
        define("MARKER", &fixture.marker),
        define("WHO", who),
        "-o".into(),
        output.into(),
        source.into(),
    ];
    if audit {
        arguments.push("-DAUDIT".into());
    }
    if let Some(soname) = soname {
        arguments.push(format!("-Wl,-soname,{soname}").into());
    }
    let arguments: Vec<&OsStr> = arguments.iter().map(OsString::as_os_str).collect();
    compile(&arguments);
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = directory(name);
        let fixture = Fixture {
            marker: root.join("marker"),
            planted: root.join("planted"),
            genuine: root.join("genuine"),
            plain: root.join("plain"),
            linked: root.join("linked"),
            converting: root.join("converting"),
            echoing: root.join("echoing"),
        };
        std::fs::create_dir_all(&fixture.planted).unwrap();
        std::fs::create_dir_all(&fixture.genuine).unwrap();
        for (dir, who) in [(&fixture.planted, "planted"), (&fixture.genuine, "genuine")] {
            library(&fixture, &dir.join("hook.so"), who, false, None);
            library(&fixture, &dir.join("audit.so"), who, true, None);
            library(
                &fixture,
                &dir.join("libneeded.so"),
                who,
                false,
                Some("libneeded.so"),
            );
            // A converter module of the character set `CONVERTING` asks for. The file names it
            // relative to its own directory, which glibc reads from each directory `GCONV_PATH`
            // lists.
            library(&fixture, &dir.join(format!("{who}.so")), who, false, None);
            std::fs::write(
                dir.join("gconv-modules"),
                format!("module  STAGECHARSET//  INTERNAL  {who}  1\n"),
            )
            .unwrap();
        }
        for (source, program) in [
            (PLAIN, &fixture.plain),
            (LINKED, &fixture.linked),
            (CONVERTING, &fixture.converting),
            (ECHOING, &fixture.echoing),
        ] {
            let file = program.with_extension("c");
            std::fs::write(&file, source).unwrap();
            let mut arguments: Vec<OsString> = vec![file.into(), "-o".into(), program.into()];
            if program == &fixture.linked {
                arguments.push("-L".into());
                arguments.push(fixture.genuine.clone().into_os_string());
                arguments.push("-lneeded".into());
                arguments.push("-Wl,--enable-new-dtags".into());
                let mut runpath = OsString::from("-Wl,-rpath,");
                runpath.push(&fixture.genuine);
                arguments.push(runpath);
            }
            let arguments: Vec<&OsStr> = arguments.iter().map(OsString::as_os_str).collect();
            compile(&arguments);
        }
        // The directory a stage writes. What is outside it is the person's own.
        keep_out(&fixture.planted);
        fixture
    }

    /// Start `program` holding `name` set to `value`, as a process that holds it would, and
    /// return what the libraries the loader ran recorded.
    fn run(&self, program: &Path, name: &str, value: &Path, checked: bool) -> Vec<String> {
        let _ = std::fs::remove_file(&self.marker);
        let command = self.start(program, name, value, checked);
        self.observe(command, program, name, value)
    }

    /// `program` started holding `name` set to `value`, through a checked start or, as the control,
    /// a kept one.
    fn start(&self, program: &Path, name: &str, value: &Path, checked: bool) -> Command {
        if checked {
            start_against(
                &Resolved::named_by_person(program),
                Inherits::All,
                SearchPath::Inherited,
                CodeSelectors::Checked(&[]),
                [(OsString::from(name), value.as_os_str().to_os_string())],
                &outside_stage_writable,
            )
        } else {
            start_against(
                &Resolved::named_by_person(program),
                Inherits::All,
                SearchPath::Inherited,
                // The control: the same start with nothing checked, to show the variable selects code.
                CodeSelectors::Kept,
                [(OsString::from(name), value.as_os_str().to_os_string())],
                &outside_stage_writable,
            )
        }
    }

    /// What a real process reports for `name` after a start that held it set to `value`: the value,
    /// or `(unset)` when the start removed it. This shows the environment a process was given and
    /// does not depend on what any loader does with the variable.
    fn reported(&self, name: &str, value: &Path, checked: bool) -> String {
        let mut command = self.start(&self.echoing, name, value, checked);
        holding(&mut command, name, value);
        let output = command.arg(name).output().unwrap();
        assert!(output.status.success(), "echoing {name}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn observe(
        &self,
        mut command: Command,
        program: &Path,
        name: &str,
        value: &Path,
    ) -> Vec<String> {
        // The child inherits what the parent holds, less what the start removed.
        holding(&mut command, name, value);
        let status = command.status().unwrap();
        assert!(
            status.success() || program == self.converting,
            "{} with {name}={}",
            program.display(),
            value.display()
        );
        std::fs::read_to_string(&self.marker)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

/// Whether a marker line recorded by `who` is present.
fn loaded(markers: &[String], who: &str) -> bool {
    markers.iter().any(|marker| marker == who)
}

/// The regression it rejects: a loader variable left in the environment of a checked start, so a
/// library a stage wrote is loaded into the program outside confinement, or one removed whatever
/// it names, so a person's own library is not loaded. Each variable is run three times: without
/// the check, which shows the variable selects code, and with it for a planted and a genuine
/// value. `LD_LIBRARY_PATH` is run against a program that has the genuine library on its
/// `RUNPATH`, so with the variable removed the genuine library still loads and the start succeeds.
///
/// Each assertion is about whether a marker is present. How many times a constructor runs or a
/// module is read depends on the loader (an audit library's constructor can run twice), so no
/// assertion compares a whole list of markers.
#[test]
fn a_loader_variable_naming_a_library_a_stage_may_write_is_removed_and_a_safe_one_still_loads() {
    if !compiler_present() {
        return;
    }
    let fixture = Fixture::new("variables");
    let cases: [(&str, &Path, PathBuf, PathBuf, bool); 4] = [
        (
            "LD_PRELOAD",
            &fixture.plain,
            fixture.planted.join("hook.so"),
            fixture.genuine.join("hook.so"),
            false,
        ),
        (
            "LD_AUDIT",
            &fixture.plain,
            fixture.planted.join("audit.so"),
            fixture.genuine.join("audit.so"),
            false,
        ),
        (
            "LD_LIBRARY_PATH",
            &fixture.linked,
            fixture.planted.clone(),
            fixture.genuine.clone(),
            true,
        ),
        // glibc loads the converter module a `gconv-modules` file in a listed directory names.
        (
            "GCONV_PATH",
            &fixture.converting,
            fixture.planted.clone(),
            fixture.genuine.clone(),
            false,
        ),
    ];

    for (name, program, planted, genuine, genuine_remains) in cases {
        let kept = fixture.run(program, name, &planted, false);
        assert!(
            loaded(&kept, "planted"),
            "{name} did not load the planted library when nothing checked it: {kept:?}"
        );
        let removed = fixture.run(program, name, &planted, true);
        assert!(
            !loaded(&removed, "planted"),
            "{name} loaded the planted library through a checked start: {removed:?}"
        );
        if genuine_remains {
            assert!(
                loaded(&removed, "genuine"),
                "{name} left no genuine library loading after the removal: {removed:?}"
            );
        }
        let safe = fixture.run(program, name, &genuine, true);
        assert!(
            loaded(&safe, "genuine"),
            "{name} did not load the genuine library through a checked start: {safe:?}"
        );
        assert!(
            !loaded(&safe, "planted"),
            "{name} loaded the planted library from a genuine value: {safe:?}"
        );
    }
}

/// The regression it rejects: `LOCPATH` or `NLSPATH` left in the environment of a checked start
/// when it names a place a stage may write, or `LOCPATH` removed when it names a directory outside
/// every one. A process the start made reports what it was given, so this shows the environment of
/// a real process. What a loader or locale code does with the variable was not run. The two names
/// are in the table because glibc's `unsecvars.h` lists them, and that file does not say whether
/// they select code. `NLSPATH` is a list of templates, so it is removed whatever it holds.
#[test]
fn locpath_and_nlspath_reach_a_process_only_when_they_name_a_directory_a_stage_cannot_write() {
    if !compiler_present() {
        return;
    }
    let fixture = Fixture::new("locale");

    // The control: a kept start hands both on, so the removal below is seen from a program that
    // does report them.
    for name in ["LOCPATH", "NLSPATH"] {
        assert_eq!(
            fixture.reported(name, &fixture.planted, false),
            fixture.planted.to_str().unwrap(),
            "{name} did not reach the process when nothing checked it"
        );
        assert_eq!(
            fixture.reported(name, &fixture.planted, true),
            "(unset)",
            "{name} reached the process through a checked start"
        );
    }
    assert_eq!(
        fixture.reported("LOCPATH", &fixture.genuine, true),
        fixture.genuine.to_str().unwrap(),
        "LOCPATH was removed though it names a directory outside every writable one"
    );
    assert_eq!(
        fixture.reported("NLSPATH", &fixture.genuine, true),
        "(unset)",
        "NLSPATH was kept, though a list of templates is not checked as a path"
    );
}
