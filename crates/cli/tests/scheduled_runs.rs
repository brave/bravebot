//! `bravebot schedule` against the built binary and a stand-in `crontab` (CLI-42).
//!
//! The stand-in keeps the table in a file the test names, so no test touches the account's real
//! crontab. `schedule add` needs a terminal, so it runs under `script`, which gives it a pty.
#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(clippy::unwrap_used, reason = "a test target")]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[allow(
    dead_code,
    reason = "open_files.rs is shared by several test binaries, and this one uses only bind_mock_server"
)]
#[path = "../../agent/test-support/open_files.rs"]
mod open_files;

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

/// The words of a line as a session's `/schedule` is given them.
fn words(line: &[&str]) -> Vec<String> {
    line.iter().map(|word| word.to_string()).collect()
}

/// The sentence `bravebot schedule` prints for a refused line is the sentence a session draws for
/// the same words, since both come from `bravebot_schedule`. None of these lines reads or
/// writes the state directory, so the test process's own home is not touched.
#[test]
fn a_session_is_refused_in_the_words_the_command_line_prints() {
    let scratch = Scratch::new("schedule-parity");
    for line in [
        vec!["frobnicate"],
        vec!["remove"],
        vec!["pause", "1", "2"],
        vec!["history", "1", "2"],
    ] {
        let mut arguments = vec!["schedule"];
        arguments.extend_from_slice(&line);
        let printed = scratch.run(&arguments);
        assert_eq!(printed.status.code(), Some(2), "{line:?}");
        let on_the_line = String::from_utf8_lossy(&printed.stderr).into_owned();
        let in_a_session = match bravebot_schedule::in_session(
            &words(&line),
            bravebot_schedule::TypedLine::of_a_person(),
            Path::new("."),
        ) {
            bravebot_schedule::InSession::Ended(reply) => reply,
            bravebot_schedule::InSession::Running(_) => panic!("{line:?} started a run"),
        };
        assert_eq!(in_a_session.outcome, bravebot_schedule::Outcome::Argument);
        let sentence = in_a_session.err.expect("a refusal has its sentence");
        assert!(!sentence.is_empty(), "{line:?}");
        assert!(
            on_the_line.contains(sentence.trim_end()),
            "{line:?}: the command line printed {on_the_line:?}, a session draws {sentence:?}"
        );
    }
    // A cron expression outside its range is refused by the same function, before the state
    // directory is read.
    let bad = "61 9 * * *";
    let printed = scratch.run_at_a_terminal(&[
        "schedule",
        "add",
        "--cron",
        bad,
        "--prompt",
        "list merged pull requests",
        "--scheduler",
        "crontab",
    ]);
    assert_eq!(printed.status.code(), Some(2));
    // Under `script` the terminal carries both streams.
    let on_the_line = out(&printed);
    let in_a_session = match bravebot_schedule::in_session(
        &words(&[
            "add",
            "--cron",
            bad,
            "--prompt",
            "list merged pull requests",
        ]),
        bravebot_schedule::TypedLine::of_a_person(),
        Path::new("."),
    ) {
        bravebot_schedule::InSession::Ended(reply) => reply,
        bravebot_schedule::InSession::Running(_) => panic!("add started a run"),
    };
    let sentence = in_a_session.err.expect("a refusal has its sentence");
    assert!(
        on_the_line.contains(sentence.trim_end()),
        "the command line printed {on_the_line:?}, a session draws {sentence:?}"
    );
    assert_eq!(scratch.table(), "");
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

/// A destination program that writes its standard input to `caught` and ends with `status`.
fn destination(scratch: &Scratch, name: &str, status: i32) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let program = scratch.path.join(name);
    let caught = scratch.path.join(format!("{name}-caught"));
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\n/bin/cat > '{}'\nexit {status}\n",
            caught.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    (program, caught)
}

/// SCHEDULE-7: every scheduler's job is the same `schedule run-now <id>` line, so a firing
/// started by cron, launchd or systemd reaches the delivery the one path holds. The destination is
/// chosen at add, listed, and handed the run's stdout and not the line `run-now` prints for the
/// person who started it.
#[test]
fn a_run_is_handed_to_the_destination_chosen_at_add_whichever_scheduler_started_it() {
    for scheduler in ["crontab", "launchd", "systemd"] {
        let scratch = Scratch::new(&format!("schedule-deliver-{scheduler}"));
        let (program, caught) = destination(&scratch, "notify", 0);
        let program_text = program.to_str().unwrap();
        let added = added(
            &scratch,
            &["--scheduler", scheduler, "--deliver-to", program_text],
        );
        assert!(
            out(&added).contains("Added schedule 1."),
            "{scheduler}: {}",
            out(&added)
        );
        let listed = out(&scratch.run(&["schedule", "list"]));
        assert!(
            listed.contains(&format!("delivers to {program_text}")),
            "{listed}"
        );
        // The job each scheduler installed starts this command, which is the path under test.
        let installed = match scheduler {
            "crontab" => scratch.table(),
            "launchd" => std::fs::read_to_string(
                scratch
                    .path
                    .join("home/Library/LaunchAgents/com.brave.bravebot.schedule.1.plist"),
            )
            .unwrap(),
            _ => std::fs::read_to_string(
                scratch
                    .path
                    .join("home/.config/systemd/user/bravebot-schedule-1.service"),
            )
            .unwrap(),
        };
        assert!(installed.contains("run-now"), "{scheduler}: {installed}");
        assert!(
            !installed.contains(program_text),
            "the destination is not in the job: {installed}"
        );

        let ran = scratch.run(&["schedule", "run-now", "1"]);
        assert!(out(&ran).contains("schedule 1 ran"), "{}", out(&ran));
        let delivered = std::fs::read(&caught).expect("the destination was started");
        assert!(
            !String::from_utf8_lossy(&delivered).contains("schedule 1 ran"),
            "{scheduler}"
        );
    }
}

/// SCHEDULE-7: a paused entry and a firing skipped as still running deliver nothing, and a
/// destination that fails changes neither the line printed, the recorded run nor the exit status.
#[test]
fn a_skipped_firing_delivers_nothing_and_a_failing_destination_changes_nothing() {
    let scratch = Scratch::new("schedule-deliver-skips");
    let (failing, caught) = destination(&scratch, "fails", 7);
    assert!(
        added(&scratch, &["--deliver-to", failing.to_str().unwrap()])
            .status
            .success()
    );
    let (plain_entry_ran, _) = (scratch.run(&["schedule", "pause", "1"]), ());
    assert!(plain_entry_ran.status.success());
    let paused = scratch.run(&["schedule", "run-now", "1"]);
    assert_eq!(paused.status.code(), Some(4));
    assert!(!caught.exists(), "a paused entry delivered");
    scratch.run(&["schedule", "resume", "1"]);

    let holder = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(scratch.state().join("1.lock"))
        .unwrap();
    holder.try_lock().unwrap();
    let skipped = scratch.run(&["schedule", "run-now", "1"]);
    assert!(out(&skipped).contains("still running"));
    assert!(!caught.exists(), "a skipped firing delivered");
    drop(holder);

    // The same entry without a destination, for what a run does on its own here.
    let alone = Scratch::new("schedule-deliver-alone");
    added(&alone, &[]);
    let without = alone.run(&["schedule", "run-now", "1"]);

    let with = scratch.run(&["schedule", "run-now", "1"]);
    assert!(caught.exists(), "the destination was not started");
    assert_eq!(with.status.code(), without.status.code());
    assert_eq!(out(&with), out(&without));
    let notice = String::from_utf8_lossy(&with.stderr).into_owned();
    assert!(notice.contains("not delivered"), "{notice}");
    assert!(notice.contains(failing.to_str().unwrap()), "{notice}");
    let history = std::fs::read_to_string(scratch.state().join("1.history")).unwrap();
    let alone_history = std::fs::read_to_string(alone.state().join("1.history")).unwrap();
    assert_eq!(history.lines().count(), 1, "{history}");
    assert_eq!(
        history.split(' ').nth(1),
        alone_history.split(' ').nth(1),
        "the recorded status"
    );
}

/// A chat gateway on loopback that lists one model and answers every chat request with a call to
/// `ask_user`, so a run sent to it reaches a question. Returns its port.
fn a_gateway_asking_which_cache() -> u16 {
    let listener = open_files::bind_mock_server();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            let _ = reader.read_line(&mut request);
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.trim().eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            let (kind, text) = if request.starts_with("GET") {
                (
                    "application/json",
                    r#"{"data": [{"id": "reasons-only", "context_length": 262144, "supported_parameters": ["tools"]}]}"#.to_string(),
                )
            } else {
                let arguments = serde_json::json!({"questions": [{
                    "header": "Cache",
                    "question": "Which cache?",
                    "options": [{"label": "HTTP"}, {"label": "Query"}]
                }]})
                .to_string();
                let frame = serde_json::json!({"model": "reasons-only", "choices": [{
                    "index": 0,
                    "delta": {"role": "assistant", "tool_calls": [{
                        "index": 0, "id": "call-1", "type": "function",
                        "function": {"name": "ask_user", "arguments": arguments}}]},
                    "finish_reason": "tool_calls"}]});
                (
                    "text/event-stream",
                    format!("data: {frame}\n\ndata: [DONE]\n\n"),
                )
            };
            let reply = format!(
                "HTTP/1.1 200 \r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                text.len()
            );
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
        }
    });
    port
}

/// A scratch whose runs are sent to `a_gateway_asking_which_cache`, so a run of its entry stops at
/// the question "Which cache?" (SCHEDULE-2).
fn at_a_gateway_asking(name: &str) -> (Scratch, Vec<(&'static str, String)>) {
    let scratch = Scratch::new(name);
    let port = a_gateway_asking_which_cache();
    let home = scratch.path.join("home/.bravebot");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("settings.json"),
        format!(
            r#"{{
                "provider": {{"openrouter": {{
                    "env": ["OPENROUTER_API_KEY"],
                    "options": {{"baseURL": "http://127.0.0.1:{port}/api/v1"}}
                }}}},
                "model": "openrouter/reasons-only"
            }}"#
        ),
    )
    .unwrap();
    let environment = vec![
        (
            "SERVICES_KEY_AICHAT",
            "a-services-key-ZZSECRETZZ".to_string(),
        ),
        ("BRAVE_SERVICES_KEY_ID", "a-key-id-ZZSECRETZZ".to_string()),
        (
            "BACKTRACE_SUBMIT_URL",
            "http://127.0.0.1:1/submit-ZZSECRETZZ".to_string(),
        ),
        ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1".to_string()),
        ("OPENROUTER_API_KEY", "a-token".to_string()),
    ];
    (scratch, environment)
}

/// `run-now` of entry 1 in `scratch` with `environment` set on the child only.
fn run_now_with(scratch: &Scratch, environment: &[(&'static str, String)]) -> Output {
    let mut command = scratch.command(&["schedule", "run-now", "1"]);
    for (name, value) in environment {
        command.env(name, value);
    }
    command.output().unwrap()
}

/// SCHEDULE-7, SCHEDULE-2: a run that stopped at a question is delivered like any
/// other. The destination receives the run's captured standard output, which is its one
/// `--json` result line reporting the refusal at the scheduled-question gate (status 4). The
/// question the run kept for `pending` is in the pending file and is not in what the destination
/// received.
#[test]
fn a_run_stopped_at_a_question_delivers_its_stdout_and_not_the_kept_question() {
    let (scratch, environment) = at_a_gateway_asking("schedule-deliver-stopped");
    let (program, caught) = destination(&scratch, "notify", 0);
    let program_text = program.to_str().unwrap();
    assert!(
        out(&added(&scratch, &["--deliver-to", program_text])).contains("Added schedule 1."),
        "add"
    );

    let ran = run_now_with(&scratch, &environment);
    let said = out(&ran);
    assert!(said.contains("schedule 1 ran"), "{said}");

    // The run stopped at its question and kept it.
    let pending = out(&scratch.run(&["schedule", "pending", "1"]));
    assert!(pending.contains("Which cache?"), "{pending}");
    assert!(
        std::fs::read_to_string(scratch.state().join("1.pending"))
            .unwrap()
            .contains("Which cache?")
    );

    let delivered = String::from_utf8(std::fs::read(&caught).expect("delivered")).unwrap();
    // Exactly one line: the run's `--json` result, ending in a newline.
    assert_eq!(delivered.lines().count(), 1, "{delivered:?}");
    assert!(delivered.ends_with('\n'), "{delivered:?}");
    let result: serde_json::Value = serde_json::from_str(delivered.trim_end()).unwrap();
    // It is the result of the run that stopped: refused at the scheduled-question gate, and the
    // status the history recorded for the firing.
    assert_eq!(result["ok"], false, "{delivered}");
    assert_eq!(result["status"], 4, "{delivered}");
    assert_eq!(result["reason"], "refused", "{delivered}");
    assert_eq!(result["calls"][0]["tool"], "ask_user", "{delivered}");
    assert_eq!(
        result["refusals"][0]["gate"], "scheduled_question",
        "{delivered}"
    );
    let history = std::fs::read_to_string(scratch.state().join("1.history")).unwrap();
    assert_eq!(history.lines().count(), 1, "{history}");
    assert_eq!(history.split(' ').nth(1), Some("4"), "{history}");
    // What the run kept is not part of what was delivered, and neither is the line printed for
    // the person who started it.
    for kept in ["Which cache?", "HTTP", "Query", "Pick:"] {
        assert!(
            !delivered.contains(kept),
            "{kept} was delivered: {delivered}"
        );
    }
    assert!(!delivered.contains("schedule 1 ran"), "{delivered}");
}

/// SCHEDULE-7: the credentials this agent holds are not in the environment of the
/// destination, though the run it was handed the output of had them. They are set on the child
/// command of the harness and not in this process.
#[test]
fn the_destination_is_started_without_the_agents_own_credentials() {
    use std::os::unix::fs::PermissionsExt;
    let (scratch, environment) = at_a_gateway_asking("schedule-deliver-environment");
    let program = scratch.path.join("show-env");
    let seen = scratch.path.join("env-seen");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\n/bin/cat > /dev/null\n/usr/bin/env > '{}'\n",
            seen.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        out(&added(
            &scratch,
            &["--deliver-to", program.to_str().unwrap()]
        ))
        .contains("Added schedule 1.")
    );

    let mut command = scratch.command(&["schedule", "run-now", "1"]);
    for (name, value) in &environment {
        command.env(name, value);
    }
    // A variable that is not this agent's own is handed on, so an empty environment would not
    // pass for a scrubbed one.
    command.env("A_PERSONS_OWN_VARIABLE", "kept");
    let ran = command.output().unwrap();
    assert!(out(&ran).contains("schedule 1 ran"), "{}", out(&ran));

    let seen = std::fs::read_to_string(&seen).expect("the destination was started");
    for withheld in [
        "SERVICES_KEY_AICHAT",
        "BRAVE_SERVICES_KEY_ID",
        "BACKTRACE_SUBMIT_URL",
    ] {
        assert!(
            !seen
                .lines()
                .any(|line| line.starts_with(&format!("{withheld}="))),
            "{withheld} reached the destination: {seen}"
        );
    }
    assert!(!seen.contains("ZZSECRETZZ"), "{seen}");
    assert!(seen.contains("A_PERSONS_OWN_VARIABLE=kept"), "{seen}");
    // The settings name this variable as a gateway's token, which is withheld as well.
    assert!(!seen.contains("OPENROUTER_API_KEY"), "{seen}");
    assert!(seen.contains("BRAVE_AI_CHAT_ENDPOINT="), "{seen}");
}

/// SCHEDULE-7: a question a run stopped at on an earlier firing is not handed over again by the
/// next. What is delivered is the stdout of the firing that ran, and `pending` is not read for it.
#[test]
fn an_earlier_question_is_not_delivered_again_by_the_next_firing() {
    let scratch = Scratch::new("schedule-deliver-question");
    let (program, caught) = destination(&scratch, "notify", 0);
    added(&scratch, &["--deliver-to", program.to_str().unwrap()]);
    std::fs::write(
        scratch.state().join("1.pending"),
        "Pick: Which branch ZZQUESTIONZZ?\n",
    )
    .unwrap();
    scratch.run(&["schedule", "run-now", "1"]);
    let delivered = String::from_utf8_lossy(&std::fs::read(&caught).unwrap()).into_owned();
    assert!(!delivered.contains("ZZQUESTIONZZ"), "{delivered}");
}

/// A `trigger serve` process, ended when this is dropped.
struct Listener {
    child: std::process::Child,
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// SCHEDULE-7: a trigger firing is not a scheduled run and hands its output to no destination.
/// A schedule entry with a destination is stored, a trigger with the same id is fired by a signed
/// delivery to `trigger serve`, and the run it starts stops at its question and keeps it for `trigger
/// pending`. The destination program is never started, and the schedule entry has no run
/// recorded. A wrong implementation that delivered the output of a trigger's run to a
/// destination, or ran the entry of the same id, would leave the destination's file behind.
/// The check that the file is absent waits a few seconds after the run recorded its question,
/// since a delivery would start after the run ended.
#[test]
fn a_trigger_firing_delivers_nothing_to_a_schedule_entrys_destination() {
    let (scratch, environment) = at_a_gateway_asking("schedule-deliver-trigger");
    let (program, caught) = destination(&scratch, "notify", 0);
    assert!(
        out(&added(
            &scratch,
            &["--deliver-to", program.to_str().unwrap()]
        ))
        .contains("Added schedule 1.")
    );
    let registered = scratch.run(&[
        "trigger",
        "add",
        "1",
        "--event",
        "push",
        "--",
        "look at the delivery",
    ]);
    assert!(registered.status.success(), "{}", out(&registered));
    let secret = out(&registered)
        .lines()
        .find_map(|line| line.strip_prefix("Secret: "))
        .and_then(|rest| rest.split_whitespace().next())
        .expect("add printed a secret")
        .to_string();

    let mut command = scratch.command(&["trigger", "serve", "--port", "0"]);
    for (name, value) in &environment {
        command.env(name, value);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("serve starts");
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .expect("serve says where it listens");
    let _listener = Listener { child };
    let address = line
        .split_whitespace()
        .nth(2)
        .unwrap_or_else(|| panic!("no address in {line:?}"))
        .trim_end_matches('.')
        .to_string();

    // The firing: a delivery signed for the event the trigger was registered for.
    let body = b"{}";
    let signature = bravebot_signing::webhook::signature(&secret, "push", body);
    let mut stream = std::net::TcpStream::connect(&address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .unwrap();
    let head = format!(
        "POST /1 HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nX-Bravebot-Signature: {signature}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("an answer");
    assert!(answer.starts_with("HTTP/1.1 202 "), "{answer}");

    // The run it started was recorded: it stopped at its question and kept it.
    let mut pending = String::new();
    for _ in 0..120 {
        pending = out(&scratch.run(&["trigger", "pending"]));
        if pending.contains("Which cache?") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    assert!(pending.contains("1 stopped at a question"), "{pending}");
    assert!(pending.contains("Which cache?"), "{pending}");

    // A delivery would start once the run ended, which is just after it kept the question.
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(
        !caught.exists(),
        "a trigger firing started the schedule entry's destination"
    );
    assert!(
        !scratch.state().join("1.history").exists(),
        "a trigger firing was recorded as a scheduled run"
    );
}
