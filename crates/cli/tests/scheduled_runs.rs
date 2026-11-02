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
        // The other two schedulers' programs write the words they were given to a log and end
        // with the status the test names, so no test loads a job into the account's own.
        for name in ["launchctl", "systemctl"] {
            let stand_in = path.join("bin").join(name);
            std::fs::write(
                &stand_in,
                format!(
                    "#!/bin/sh\necho \"{name} $*\" >> \"$TOOL_LOG\"\nexit \"${{TOOL_STATUS:-0}}\"\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { path }
    }

    fn table(&self) -> String {
        std::fs::read_to_string(self.path.join("table")).unwrap_or_default()
    }

    /// What the stand-in `launchctl` and `systemctl` were asked, one call to a line.
    fn tool_log(&self) -> String {
        std::fs::read_to_string(self.path.join("tool-log")).unwrap_or_default()
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
            .env("TOOL_LOG", self.path.join("tool-log"))
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
        self.at_a_terminal(&self.command(arguments), arguments)
    }

    /// Runs the bravebot binary with `arguments` and the environment of `prepared`, under
    /// `script` so it has a terminal.
    fn at_a_terminal(&self, prepared: &Command, arguments: &[&str]) -> Output {
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
            .envs(prepared.get_envs().filter_map(|(k, v)| Some((k, v?))))
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
    // Each test names the scheduler it asserts on, so the host's usual one does not change it.
    if !extra.contains(&"--scheduler") {
        arguments.extend_from_slice(&["--scheduler", "crontab"]);
    }
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
        "--scheduler",
        "crontab",
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
            vec!["schedule", "pause", "1"],
            vec!["schedule", "resume", "1"],
            vec!["schedule", "pending"],
            vec!["schedule", "clear", "1"],
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
        let output = scratch.run_at_a_terminal(&[
            "schedule",
            "add",
            "--cron",
            bad,
            "--prompt",
            "hello",
            "--scheduler",
            "crontab",
        ]);
        assert_eq!(output.status.code(), Some(2), "{bad}");
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

#[test]
fn a_cron_field_outside_its_range_writes_nothing() {
    let scratch = Scratch::new("schedule-cron-range");
    for bad in [
        "60 * * * *",
        "* 24 * * *",
        "* * 0 * *",
        "* * * 13 *",
        "* * * * 8",
        "*/0 * * * *",
    ] {
        let output =
            scratch.run_at_a_terminal(&["schedule", "add", "--cron", bad, "--prompt", "hello"]);
        assert_eq!(output.status.code(), Some(2), "{bad}");
        assert_eq!(scratch.table(), "", "{bad}");
        assert!(!scratch.state().join("1.entry").exists(), "{bad}");
    }
}

/// SCHEDULE-1: a paused entry keeps its crontab line, is listed as paused, and a firing of it is
/// skipped without a run being recorded; resuming lets the next firing run.
#[test]
fn a_paused_entry_keeps_its_line_and_its_firing_is_skipped_until_it_is_resumed() {
    let scratch = Scratch::new("schedule-pause");
    added(&scratch, &[]);
    added(&scratch, &[]);
    let line = scratch.table();

    let paused = scratch.run(&["schedule", "pause", "1"]);
    assert!(paused.status.success());
    assert!(
        out(&paused).contains("Paused schedule 1."),
        "{}",
        out(&paused)
    );
    assert_eq!(scratch.table(), line, "the crontab line stays");
    let listed = out(&scratch.run(&["schedule", "list"]));
    let rows: Vec<&str> = listed.lines().collect();
    assert!(
        rows[0].starts_with("1  ") && rows[0].ends_with("paused"),
        "{listed}"
    );
    assert!(
        rows[1].starts_with("2  ") && rows[1].ends_with("active"),
        "{listed}"
    );
    assert_eq!(
        scratch.run(&["schedule", "pause", "1"]).status.code(),
        Some(2),
        "pausing a paused entry"
    );

    let skipped = scratch.run(&["schedule", "run-now", "1"]);
    assert_eq!(skipped.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&skipped.stderr).contains("paused"));
    assert!(
        !scratch.state().join("1.history").exists(),
        "a skipped firing is not a run"
    );

    let resumed = scratch.run(&["schedule", "resume", "1"]);
    assert!(
        out(&resumed).contains("Resumed schedule 1."),
        "{}",
        out(&resumed)
    );
    assert_eq!(
        scratch.run(&["schedule", "resume", "1"]).status.code(),
        Some(2),
        "resuming an entry that is not paused"
    );
    assert!(out(&scratch.run(&["schedule", "run-now", "1"])).contains("schedule 1 ran"));
    assert_eq!(
        scratch.run(&["schedule", "pause", "9"]).status.code(),
        Some(2),
        "an entry that is not set"
    );

    // Removing an entry takes the mark with it, so the next entry of that number starts active.
    scratch.run(&["schedule", "pause", "2"]);
    scratch.run(&["schedule", "remove", "2"]);
    assert!(!scratch.state().join("2.paused").exists());
}

/// SCHEDULE-3: `pending` prints the question a firing stopped at, `clear` deletes it and takes
/// no answer, and neither touches the entry.
#[test]
fn a_stored_question_is_shown_by_pending_and_deleted_by_clear() {
    let scratch = Scratch::new("schedule-pending");
    added(&scratch, &[]);
    added(&scratch, &[]);
    assert!(out(&scratch.run(&["schedule", "pending"])).contains("No schedule is waiting"));
    std::fs::write(
        scratch.state().join("1.pending"),
        "Pick: Which branch?\n1. main\n2. dev\n",
    )
    .unwrap();

    let all = out(&scratch.run(&["schedule", "pending"]));
    assert!(all.contains("Schedule 1 stopped at a question"), "{all}");
    assert!(
        all.contains("Pick: Which branch?") && all.contains("2. dev"),
        "{all}"
    );
    assert!(!all.contains("Schedule 2"), "{all}");
    let one = out(&scratch.run(&["schedule", "pending", "1"]));
    assert!(one.contains("Which branch?"), "{one}");
    assert!(out(&scratch.run(&["schedule", "pending", "2"])).contains("No schedule is waiting"));
    assert_eq!(
        scratch.run(&["schedule", "pending", "9"]).status.code(),
        Some(2)
    );

    // Clearing takes no text and reads nothing: an answer word is a usage error.
    assert_eq!(
        scratch
            .run(&["schedule", "clear", "1", "main"])
            .status
            .code(),
        Some(2)
    );
    assert!(scratch.state().join("1.pending").exists());
    let before = scratch.table();
    let cleared = scratch.run(&["schedule", "clear", "1"]);
    assert!(
        out(&cleared).contains("Cleared the question schedule 1"),
        "{}",
        out(&cleared)
    );
    assert!(!scratch.state().join("1.pending").exists());
    assert!(scratch.state().join("1.entry").exists());
    assert_eq!(scratch.table(), before);
    assert!(out(&scratch.run(&["schedule", "clear", "1"])).contains("no question to clear"));
    assert_eq!(
        scratch.run(&["schedule", "clear", "9"]).status.code(),
        Some(2)
    );

    // A question left by an earlier firing is dropped when the entry runs again.
    std::fs::write(scratch.state().join("1.pending"), "Pick: old?\n").unwrap();
    scratch.run(&["schedule", "run-now", "1"]);
    assert!(!scratch.state().join("1.pending").exists());
}

/// SCHEDULE-4: a firing that finds the previous firing of the same entry still running does
/// nothing. The hold is a lock on a file, so the test takes it itself.
#[test]
fn a_firing_is_skipped_while_the_previous_one_of_the_entry_still_runs() {
    let scratch = Scratch::new("schedule-overlap");
    added(&scratch, &[]);
    added(&scratch, &[]);
    let holder = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(scratch.state().join("1.lock"))
        .unwrap();
    holder.try_lock().unwrap();

    let skipped = scratch.run(&["schedule", "run-now", "1"]);
    assert!(skipped.status.success());
    assert!(out(&skipped).contains("still running"), "{}", out(&skipped));
    assert!(!scratch.state().join("1.history").exists());
    // Another entry is not held.
    assert!(out(&scratch.run(&["schedule", "run-now", "2"])).contains("schedule 2 ran"));

    drop(holder);
    assert!(out(&scratch.run(&["schedule", "run-now", "1"])).contains("schedule 1 ran"));
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

/// MODE-5, CLI-42: a settings layer that made the bypass mode unreachable refuses
/// `--dangerously-skip-permissions` at `schedule add`, because `main` refuses the flag for every
/// command before it dispatches. Nothing is stored. A firing of an entry that holds the bypass mode,
/// stored before the layer or edited in by hand, starts a run that carries the flag and is refused
/// the same way (status 2, recorded in the history).
#[test]
fn a_layer_that_made_the_bypass_unreachable_refuses_it_at_add_and_at_a_firing() {
    let scratch = Scratch::new("schedule-bypass-unreachable");
    let settings = scratch.path.join("home/.bravebot");
    std::fs::create_dir_all(&settings).unwrap();
    assert!(
        out(&added(&scratch, &["--dangerously-skip-permissions"])).contains("Added schedule 1.")
    );

    std::fs::write(
        settings.join("settings.json"),
        r#"{"permissions": {"bypassUnreachable": true}}"#,
    )
    .unwrap();
    let refused = added(&scratch, &["--dangerously-skip-permissions"]);
    assert!(
        out(&refused).contains("permissions.bypassUnreachable"),
        "{}",
        out(&refused)
    );
    assert!(
        !out(&refused).contains("Added schedule"),
        "{}",
        out(&refused)
    );
    assert!(!scratch.state().join("2.entry").exists());
    assert_eq!(scratch.table().lines().count(), 1);

    let fired = out(&scratch.run(&["schedule", "run-now", "1"]));
    assert!(fired.contains("schedule 1 ran, status 2"), "{fired}");
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
            scratch.at_a_terminal(&command, arguments)
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
            "--scheduler",
            "crontab",
        ],
        true,
    );
    assert!(!addition.status.success());
    assert!(!scratch.state().join("2.entry").exists());
    assert_eq!(scratch.table(), table);
}

/// A systemd entry is a service and a timer in the user's unit directory, enabled through
/// `systemctl --user`, and leaves the crontab alone.
#[test]
fn a_systemd_entry_is_a_timer_and_a_service_and_not_a_crontab_line() {
    let scratch = Scratch::new("schedule-systemd");
    let output = added(&scratch, &["--scheduler", "systemd"]);
    assert!(
        out(&output).contains("Added schedule 1."),
        "{}",
        out(&output)
    );
    assert_eq!(scratch.table(), "");

    let units = scratch.path.join("home/.config/systemd/user");
    let timer = std::fs::read_to_string(units.join("bravebot-schedule-1.timer")).unwrap();
    assert!(
        timer.contains("OnCalendar=Mon,Tue,Wed,Thu,Fri *-*-* 09:00:00\n"),
        "{timer}"
    );
    let service = std::fs::read_to_string(units.join("bravebot-schedule-1.service")).unwrap();
    assert!(service.contains("schedule run-now 1\n"), "{service}");
    let log = scratch.tool_log();
    assert!(
        log.contains("systemctl --user enable --now bravebot-schedule-1.timer\n"),
        "{log}"
    );

    let listed = out(&scratch.run(&["schedule", "list"]));
    assert!(listed.contains("systemd"), "{listed}");

    let removed = scratch.run(&["schedule", "remove", "1"]);
    assert!(out(&removed).contains("Removed schedule 1."));
    assert!(!units.join("bravebot-schedule-1.timer").exists());
    assert!(!units.join("bravebot-schedule-1.service").exists());
    assert!(!scratch.state().join("1.entry").exists());
    assert!(
        scratch
            .tool_log()
            .contains("systemctl --user disable --now bravebot-schedule-1.timer\n")
    );
}

/// A launchd entry is a plist in `~/Library/LaunchAgents`, loaded with `launchctl`, and removing it
/// unloads the job it loaded.
#[test]
fn a_launchd_entry_is_a_plist_loaded_and_unloaded_by_its_path() {
    let scratch = Scratch::new("schedule-launchd");
    let output = added(&scratch, &["--scheduler", "launchd"]);
    assert!(
        out(&output).contains("Added schedule 1."),
        "{}",
        out(&output)
    );
    assert_eq!(scratch.table(), "");

    let plist = scratch
        .path
        .join("home/Library/LaunchAgents/com.brave.bravebot.schedule.1.plist");
    let text = std::fs::read_to_string(&plist).unwrap();
    assert_eq!(text.matches("<key>Weekday</key>").count(), 5, "{text}");
    assert!(text.contains("<string>run-now</string>"), "{text}");
    let log = scratch.tool_log();
    assert!(
        log.contains(&format!("launchctl load -w {}\n", plist.display())),
        "{log}"
    );

    scratch.run(&["schedule", "remove", "1"]);
    assert!(!plist.exists());
    assert!(
        scratch
            .tool_log()
            .contains(&format!("launchctl unload -w {}\n", plist.display()))
    );
}

/// An entry is kept only where the scheduler took the job, whichever scheduler that is.
#[test]
fn an_entry_is_not_kept_where_launchctl_or_systemctl_refuses_the_job() {
    for scheduler in ["launchd", "systemd"] {
        let scratch = Scratch::new(&format!("schedule-{scheduler}-refuses"));
        let mut command = scratch.command(&[
            "schedule",
            "add",
            "--cron",
            "0 9 * * *",
            "--prompt",
            "hello",
            "--scheduler",
            scheduler,
        ]);
        command.env("TOOL_STATUS", "1");
        let output = scratch.at_a_terminal(
            &command,
            &[
                "schedule",
                "add",
                "--cron",
                "0 9 * * *",
                "--prompt",
                "hello",
                "--scheduler",
                scheduler,
            ],
        );
        assert!(!output.status.success(), "{scheduler}: {}", out(&output));
        let left = |directory: &str| {
            std::fs::read_dir(scratch.path.join("home").join(directory))
                .map(|dir| dir.flatten().count())
                .unwrap_or(0)
        };
        assert_eq!(left("Library/LaunchAgents"), 0, "{scheduler}");
        assert_eq!(left(".config/systemd/user"), 0, "{scheduler}");
        assert_eq!(left(".bravebot/schedules"), 0, "{scheduler}");
    }
}

/// A scheduler that no longer knows the job does not keep the entry: `remove` still deletes the
/// files and the entry.
#[test]
fn removing_a_job_the_scheduler_fails_to_stop_still_deletes_its_files_and_entry() {
    let scratch = Scratch::new("schedule-remove-tool-fails");
    assert!(
        added(&scratch, &["--scheduler", "launchd"])
            .status
            .success()
    );
    assert!(
        added(&scratch, &["--scheduler", "systemd"])
            .status
            .success()
    );
    let plist = scratch
        .path
        .join("home/Library/LaunchAgents/com.brave.bravebot.schedule.1.plist");
    let units = scratch.path.join("home/.config/systemd/user");
    assert!(plist.exists());
    assert!(units.join("bravebot-schedule-2.timer").exists());
    for id in ["1", "2"] {
        let mut command = scratch.command(&["schedule", "remove", id]);
        command.env("TOOL_STATUS", "1");
        let removal = command.output().unwrap();
        assert!(removal.status.success(), "{id}: {}", out(&removal));
        assert!(
            !scratch.state().join(format!("{id}.entry")).exists(),
            "{id}"
        );
    }
    assert!(!plist.exists());
    assert!(!units.join("bravebot-schedule-2.timer").exists());
    assert!(!units.join("bravebot-schedule-2.service").exists());
}

/// An entry that is named for one scheduler is refused a name that is none.
#[test]
fn an_unknown_scheduler_writes_nothing() {
    let scratch = Scratch::new("schedule-unknown-scheduler");
    let output = added(&scratch, &["--scheduler", "cron"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(scratch.table(), "");
    assert!(!scratch.state().exists());
}
