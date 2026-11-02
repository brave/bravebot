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
        // util-linux takes the command as `-c` and the typescript last; BSD (macOS) takes the
        // typescript first and then the command's words.
        #[cfg(target_os = "macos")]
        command.args(["-q", "/dev/null", "/bin/sh", "-c", &quoted.join(" ")]);
        #[cfg(not(target_os = "macos"))]
        command.args(["-qec", &quoted.join(" "), "/dev/null"]);
        command
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
fn no_command_but_add_accepts_a_write_flag() {
    let scratch = Scratch::new("schedule-flags-elsewhere");
    assert!(out(&added(&scratch, &[])).contains("Added schedule 1."));
    let before = scratch.table();
    for flag in ["--accept-edits", "--dangerously-skip-permissions"] {
        for words in [
            vec!["schedule", "list"],
            vec!["schedule", "remove", "1"],
            vec!["schedule", "run-now", "1"],
            vec!["schedule", "history"],
            vec!["schedule", "history", "1"],
        ] {
            let mut arguments = words.clone();
            arguments.push(flag);
            let output = scratch.run(&arguments);
            assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        }
    }
    assert_eq!(scratch.table(), before);
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

/// Ids are never handed out again, since the history is kept under the id and would otherwise be
/// read as the new entry's.
#[test]
fn a_removed_entrys_id_is_not_given_to_the_next_entry() {
    let scratch = Scratch::new("schedule-id-reuse");
    added(&scratch, &[]);
    scratch.run(&["schedule", "run-now", "1"]);
    assert!(scratch.state().join("1.history").exists());
    scratch.run(&["schedule", "remove", "1"]);

    let again = added(&scratch, &[]);
    assert!(out(&again).contains("Added schedule 2."), "{}", out(&again));
    let history = out(&scratch.run(&["schedule", "history"]));
    assert!(!history.contains("status "), "{history}");
}

/// The state directory the sessions of the entry's directory are kept in.
fn sessions_of(scratch: &Scratch, id: &str) -> PathBuf {
    let entry = std::fs::read_to_string(scratch.state().join(format!("{id}.entry"))).unwrap();
    let directory = entry
        .lines()
        .find_map(|line| line.strip_prefix("dir="))
        .unwrap();
    let key: String = directory
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' => c,
            _ => '-',
        })
        .collect();
    scratch.path.join("home/.bravebot/sessions").join(key)
}

#[test]
fn history_is_newest_first_and_shows_the_tokens_the_session_spent() {
    let scratch = Scratch::new("schedule-history-order");
    added(&scratch, &[]);
    // Written oldest first, as a run appends them.
    std::fs::write(
        scratch.state().join("1.history"),
        "1700000000 0 -\n1800000000 3 20260101T000000-ab\n",
    )
    .unwrap();
    let sessions = sessions_of(&scratch, "1");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(
        sessions.join("20260101T000000-ab.json"),
        r#"{"id":"20260101T000000-ab","directory":"/x","title":"t","started":1,"updated":1,"tokens":4321,"conversation":{"messages":[],"context":"trusted"}}"#,
    )
    .unwrap();

    let history = out(&scratch.run(&["schedule", "history"]));
    let rows: Vec<&str> = history.lines().collect();
    assert_eq!(rows.len(), 2, "{history}");
    assert!(
        rows[0].contains("status 3")
            && rows[0].contains("session 20260101T000000-ab")
            && rows[0].contains("tokens 4321"),
        "{history}"
    );
    assert!(
        rows[1].contains("status 0") && rows[1].contains("tokens -"),
        "{history}"
    );
}

/// An entry is kept only where the line was installed.
#[test]
fn an_entry_is_not_kept_where_the_crontab_would_not_take_the_line() {
    let scratch = Scratch::new("schedule-install-fails");
    let crontab = scratch.path.join("bin/crontab");
    std::fs::write(
        &crontab,
        "#!/bin/sh\ncase \"$1\" in\n  -l) exit 1 ;;\n  -) /bin/cat > /dev/null; exit 1 ;;\nesac\n",
    )
    .unwrap();
    let output = added(&scratch, &[]);
    assert!(!output.status.success(), "{}", out(&output));
    assert_eq!(scratch.table(), "");
    let kept: Vec<_> = std::fs::read_dir(scratch.state())
        .map(|dir| dir.flatten().map(|f| f.file_name()).collect())
        .unwrap_or_default();
    assert!(kept.is_empty(), "{kept:?}");
}

#[test]
fn a_host_with_no_crontab_refuses_add_and_remove() {
    let scratch = Scratch::new("schedule-no-crontab");
    added(&scratch, &[]);
    let table = scratch.table();
    std::fs::create_dir_all(scratch.path.join("empty")).unwrap();
    let without = |arguments: &[&str], terminal: bool| {
        let mut command = scratch.command(arguments);
        command.env("PATH", scratch.path.join("empty"));
        if terminal {
            let quoted: Vec<String> = std::iter::once(env!("CARGO_BIN_EXE_bravebot").to_string())
                .chain(arguments.iter().map(|a| a.to_string()))
                .map(|word| format!("'{}'", word.replace('\'', "'\\''")))
                .collect();
            let mut wrapped = Command::new("/usr/bin/script");
            wrapped
                .args(["-qec", &quoted.join(" "), "/dev/null"])
                .env_clear()
                .envs(command.get_envs().filter_map(|(k, v)| Some((k, v?))))
                .current_dir(scratch.path.join("work"))
                .stdin(Stdio::null());
            wrapped.output().unwrap()
        } else {
            command.output().unwrap()
        }
    };
    let removal = without(&["schedule", "remove", "1"], false);
    assert!(!removal.status.success());
    assert!(scratch.state().join("1.entry").exists());
    let addition = without(
        &[
            "schedule",
            "add",
            "--cron",
            "0 9 * * *",
            "--prompt",
            "hello",
        ],
        true,
    );
    assert!(!addition.status.success());
    assert!(!scratch.state().join("2.entry").exists());
    assert_eq!(scratch.table(), table);
}
