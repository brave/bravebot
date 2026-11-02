//! `bravebot schedule` against the built binary and a stand-in `crontab` (CLI-42).
//!
//! The stand-in keeps the table in a file the test names, so no test touches the account's real
//! crontab. `schedule add` needs a terminal, so it runs under `script`, which gives it a pty.
#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(clippy::unwrap_used, reason = "a test target")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("bin")).unwrap();
        std::fs::create_dir_all(path.join("work")).unwrap();
        std::fs::create_dir_all(path.join("tmp")).unwrap();
        std::fs::create_dir_all(path.join("tmp")).unwrap();
        let crontab = path.join("bin/crontab");
        std::fs::write(
            &crontab,
            "#!/bin/sh\ncase \"$1\" in\n  -l) [ -f \"$CRONTAB_FILE\" ] && /bin/cat \"$CRONTAB_FILE\" || exit 1 ;;\n  -) /bin/cat > \"$CRONTAB_FILE\" ;;\nesac\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&crontab, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { path }
    }

    fn table(&self) -> String {
        std::fs::read_to_string(self.path.join("table")).unwrap_or_default()
    }

    fn state(&self) -> PathBuf {
        self.path.join("home/.bravebot/schedules")
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
        command
            .env_clear()
            .env("HOME", self.path.join("home"))
            .env("BRAVEBOT_LOCALE", "en-US")
            // The only directory on the path holds the stand-in, so a lookup that fails reaches
            // no real `crontab`. The system temporary directory is never searched, and the
            // scratch directory is under it, so the program reads another one.
            .env("PATH", self.path.join("bin"))
            .env("TMPDIR", self.path.join("tmp"))
            .env("CRONTAB_FILE", self.path.join("table"))
            .args(arguments)
            .current_dir(self.path.join("work"))
            .stdin(Stdio::null());
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().unwrap()
    }

    /// The same, with a terminal on stdin and stderr.
    fn run_at_a_terminal(&self, arguments: &[&str]) -> Output {
        let quoted: Vec<String> = std::iter::once(env!("CARGO_BIN_EXE_bravebot").to_string())
            .chain(arguments.iter().map(|a| a.to_string()))
            .map(|word| format!("'{}'", word.replace('\'', "'\\''")))
            .collect();
        let mut command = Command::new("/usr/bin/script");
        command
            .args(["-qec", &quoted.join(" "), "/dev/null"])
            .env_clear()
            .envs(
                self.command(&[])
                    .get_envs()
                    .filter_map(|(k, v)| Some((k, v?))),
            )
            .current_dir(self.path.join("work"))
            .stdin(Stdio::null());
        command.output().unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn out(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn added(scratch: &Scratch, extra: &[&str]) -> Output {
    let mut arguments = vec![
        "schedule",
        "add",
        "--cron",
        "0 9 * * 1-5",
        "--prompt",
        "list merged pull requests",
    ];
    arguments.extend_from_slice(extra);
    scratch.run_at_a_terminal(&arguments)
}

/// A run's tool has no terminal, so nothing a turn starts can add an entry that runs again.
#[test]
fn an_entry_cannot_be_added_without_a_terminal() {
    let scratch = Scratch::new("schedule-no-terminal");
    let output = scratch.run(&[
        "schedule",
        "add",
        "--cron",
        "0 9 * * *",
        "--prompt",
        "hello",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(scratch.table(), "");
    assert!(!scratch.state().exists());
}

#[test]
fn an_added_entry_is_one_crontab_line_and_listed_with_what_it_may_do() {
    let scratch = Scratch::new("schedule-add-list");
    let output = added(&scratch, &[]);
    assert!(
        out(&output).contains("Added schedule 1."),
        "{}",
        out(&output)
    );

    let table = scratch.table();
    assert_eq!(table.lines().count(), 1, "{table}");
    assert!(table.starts_with("0 9 * * 1-5 '"), "{table}");
    assert!(table.contains("schedule run-now 1 >/dev/null 2>&1 # bravebot-schedule:1"));

    let listed = out(&scratch.run(&["schedule", "list"]));
    assert!(listed.contains("reads and reports only"), "{listed}");
    assert!(listed.contains("list merged pull requests"), "{listed}");

    let write = added(&scratch, &["--accept-edits"]);
    assert!(out(&write).contains("Added schedule 2."), "{}", out(&write));
    let listed = out(&scratch.run(&["schedule", "list"]));
    assert!(listed.contains("may write files"), "{listed}");
    assert_eq!(scratch.table().lines().count(), 2);

    let bypass = added(&scratch, &["--dangerously-skip-permissions"]);
    assert!(out(&bypass).contains("may do anything"), "{}", out(&bypass));
    let listed = out(&scratch.run(&["schedule", "list"]));
    assert!(listed.contains("may do anything"), "{listed}");
}

#[test]
fn a_cron_expression_that_is_not_five_plain_fields_writes_nothing() {
    let scratch = Scratch::new("schedule-bad-cron");
    for bad in ["0 9 * *", "0 9 * * 1;2", "@daily"] {
        let output =
            scratch.run_at_a_terminal(&["schedule", "add", "--cron", bad, "--prompt", "hello"]);
        assert!(!output.status.success(), "{bad}");
        assert_eq!(scratch.table(), "", "{bad}");
    }
}

#[test]
fn removing_an_entry_takes_its_line_and_its_files_and_no_other() {
    let scratch = Scratch::new("schedule-remove");
    added(&scratch, &[]);
    added(&scratch, &[]);
    let output = scratch.run(&["schedule", "remove", "1"]);
    assert!(out(&output).contains("Removed schedule 1."));
    let table = scratch.table();
    assert_eq!(table.lines().count(), 1, "{table}");
    assert!(table.contains("bravebot-schedule:2"));
    assert!(!Path::new(&scratch.state().join("1.entry")).exists());
    assert!(scratch.state().join("2.entry").exists());
    assert_eq!(
        scratch.run(&["schedule", "remove", "1"]).status.code(),
        Some(2)
    );
}

/// The run the job starts is the one-shot path, and what it did is listed afterwards. With no
/// backend configured the run ends in a refusal, which `history` shows by its status.
#[test]
fn a_run_is_recorded_and_listed_by_history() {
    let scratch = Scratch::new("schedule-run-now");
    added(&scratch, &[]);
    let ran = scratch.run(&["schedule", "run-now", "1"]);
    let said = out(&ran);
    assert!(said.contains("schedule 1 ran, status "), "{said}");
    let history = out(&scratch.run(&["schedule", "history"]));
    assert!(history.starts_with("1  "), "{history}");
    assert!(history.contains("status "), "{history}");
    let status = said
        .split("status ")
        .nth(1)
        .unwrap()
        .split(',')
        .next()
        .unwrap();
    assert!(history.contains(&format!("status {status}")), "{history}");
}
