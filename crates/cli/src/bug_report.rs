//! `bravebot bug-report`: one text file a person can attach to a bug report (DIAG-8).
//!
//! The file holds what `doctor` prints and the name of the newest diagnostic log, and nothing the
//! program could not already say to a person asking `doctor`. `doctor` is run as this same program
//! started again rather than called, so the file holds the lines a person would have copied from
//! their terminal and the two cannot come to differ. The log's own lines are not copied in: nothing
//! in the program reads a log (DIAG-6), so the person attaches that file themselves.

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// The first file name tried, and the stem of the numbered ones after it.
const NAME: &str = "bravebot-bug-report";

/// What `doctor` wrote on each stream, or `None` where it could not be run.
type Said = Option<(String, String)>;

/// `bravebot bug-report`: write the file in the current directory and print its path.
pub(crate) fn command(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        return fail(Ending::Argument, t!(cli_bug_report_takes_nothing_else));
    }
    // The same two cases that give the diagnostic log no directory (DIAG-4): a report says where
    // the machine keeps its state, which an incognito session adds nothing to.
    let Some(home) = bravebot_agent::home::writable() else {
        return fail(Ending::Failed, t!(bug_report_no_state_directory));
    };
    let logs = home.join(bravebot_diag::DIRECTORY);
    // Named before `doctor` runs, because that process may write a log of its own and the one to
    // attach is the one the person's failure left.
    let log = bravebot_diag::newest(&logs);
    let text = report(
        bravebot_stamp::BUILD,
        &format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        doctor().as_ref(),
        log.as_deref(),
    );
    match write_new(Path::new("."), text.as_bytes()) {
        Ok(path) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        Err(problem) => fail(
            Ending::Failed,
            t!(bug_report_not_written, problem = problem),
        ),
    }
}

/// The file's text. Fixed headings in English: a maintainer reads it, not the person who made it.
fn report(
    build: &str,
    target: &str,
    doctor: Option<&(String, String)>,
    log: Option<&Path>,
) -> String {
    let mut text = format!("bravebot {build}\ntarget {target}\n\n== doctor ==\n");
    match doctor {
        Some((stdout, stderr)) => {
            text.push_str(stdout);
            if !stderr.is_empty() {
                text.push_str("\n== doctor, standard error ==\n");
                text.push_str(stderr);
            }
        }
        None => text.push_str("doctor could not be run\n"),
    }
    text.push_str("\n== newest diagnostic log, to attach separately ==\n");
    match log {
        Some(path) => text.push_str(&format!("{}\n", path.display())),
        None => text.push_str("none\n"),
    }
    text
}

/// `doctor`, run as this program started again, with nothing on its standard input.
fn doctor() -> Said {
    // The report is of this same program; nothing is trusted from where it says it is.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let me = std::env::current_exe().ok()?;
    let output = Command::new(me)
        .arg("doctor")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    Some((
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

/// Write `bytes` to a new file in `dir`, readable by its owner alone, and say where.
///
/// A name already taken is left as it is and the next number tried, so a report written earlier is
/// never overwritten.
fn write_new(dir: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    for number in 0u32.. {
        let name = match number {
            0 => format!("{NAME}.txt"),
            n => format!("{NAME}-{n}.txt"),
        };
        let path = dir.join(name);
        match create(&path) {
            Ok(mut file) => {
                file.write_all(bytes)?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    unreachable!("a counter of every number found none free")
}

#[cfg(unix)]
fn create(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A report that took a name already in the directory would destroy an earlier one, and one
    /// that kept the creation mode of the process would be readable by others.
    #[test]
    fn a_report_never_overwrites_one_and_is_private() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join("cli-bug-report-names");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        let first = write_new(&dir, b"one").expect("first");
        let second = write_new(&dir, b"two").expect("second");
        assert_ne!(first, second);
        assert_eq!(std::fs::read(&first).unwrap(), b"one");
        assert_eq!(std::fs::read(&second).unwrap(), b"two");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&first).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    /// The text holds the build, the target, both streams of `doctor` and the log's path, and a
    /// stream `doctor` left empty gets no heading.
    #[test]
    fn the_report_holds_the_build_both_streams_and_the_log_path() {
        let said = ("out line\n".to_string(), "err line\n".to_string());
        let text = report(
            "1.2.3",
            "linux-x86_64",
            Some(&said),
            Some(Path::new("/h/logs/a.log")),
        );
        for part in [
            "bravebot 1.2.3\n",
            "target linux-x86_64\n",
            "out line\n",
            "standard error ==\nerr line\n",
            "/h/logs/a.log\n",
        ] {
            assert!(text.contains(part), "missing {part:?} in {text}");
        }
        let quiet = ("out line\n".to_string(), String::new());
        let text = report("1", "t", Some(&quiet), None);
        assert!(!text.contains("standard error"), "{text}");
        assert!(text.ends_with("none\n"), "{text}");
    }
}
