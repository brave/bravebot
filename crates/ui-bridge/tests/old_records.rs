//! What a desktop front end removes from the state directory as it starts (STATE-4).
//!
//! Driven through the binary, because the sweep is the process's own and the state directory is
//! read from its environment. The process is kept running until the test has looked, so the result
//! does not depend on whether the sweep or the process was quicker to finish.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

const PATIENCE: Duration = Duration::from_secs(60);

/// A home of a test's own, under the build directory, removed when the test ends.
struct Home {
    path: PathBuf,
}

impl Home {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a home");
        Self {
            path: path.canonicalize().expect("a real scratch directory"),
        }
    }

    /// A file under the state directory, last written `days` ago.
    fn put(&self, relative: &str, days: u64) -> PathBuf {
        let path = self.path.join(".bravebot").join(relative);
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("directories");
        std::fs::write(&path, "{}").expect("write");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open the file")
            .set_modified(SystemTime::now() - Duration::from_secs(days * 86_400))
            .expect("age the file");
        path
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn front_end(home: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .env_clear()
        .env("HOME", home)
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1")
        .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", "http://127.0.0.1:1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the built binary runs")
}

fn lines(stdout: ChildStdout) -> mpsc::Receiver<Value> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && sender.send(value).is_err()
            {
                break;
            }
        }
    });
    receiver
}

/// Wait, while the front end runs, until `gone` has been removed. False if it was not within the
/// patience this test has.
fn removed_while_running(gone: &Path) -> bool {
    let until = Instant::now() + PATIENCE;
    while gone.exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    !gone.exists()
}

/// STATE-4: a front end that starts with a record nothing has written for longer than the period in
/// the state directory removes it, and what it must not remove is still there afterwards: a record
/// written inside the period, and the answers a person asked to keep, however old they are.
#[test]
fn a_desktop_front_end_removes_old_records_as_it_starts_and_nothing_a_person_kept() {
    let home = Home::new("bridge-old-records");
    let old = home.put("sessions/project/old-session.json", 400);
    let recent = home.put("sessions/project/recent-session.json", 3);
    let kept = [
        home.put("trusted/project.jsonl", 400),
        home.put("granted/project.jsonl", 400),
        home.put("history", 400),
    ];

    let mut child = front_end(&home.path);
    let said = lines(child.stdout.take().expect("the front end answers"));
    let ready = said.recv_timeout(PATIENCE).expect("the front end starts");
    let removed = removed_while_running(&old);
    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(ready["event"], "agent.ready", "{ready}");
    assert!(removed, "the record 400 days old was still there");
    assert!(
        recent.exists(),
        "a record written three days ago was removed"
    );
    for path in kept {
        assert!(path.exists(), "{} was removed", path.display());
    }
}

/// STATE-4: a front end whose person's settings state a period that cannot be read removes nothing,
/// so the built-in period is not put in place of the one they meant.
#[test]
fn a_desktop_front_end_with_an_unreadable_period_removes_nothing() {
    let home = Home::new("bridge-unreadable-period");
    let old = home.put("sessions/project/old-session.json", 400);
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"retentionDays": "soon"}"#,
    )
    .expect("settings");
    // A second front end, started on a home whose period reads, shows the sweep is what removes
    // what the first leaves, so the first's silence is the setting and not a sweep that never ran.
    let control = Home::new("bridge-unreadable-period-control");
    let control_old = control.put("sessions/project/old-session.json", 400);

    let mut paused = front_end(&home.path);
    let mut running = front_end(&control.path);
    let paused_said = lines(paused.stdout.take().expect("the front end answers"));
    let running_said = lines(running.stdout.take().expect("the front end answers"));
    paused_said
        .recv_timeout(PATIENCE)
        .expect("the first starts");
    running_said
        .recv_timeout(PATIENCE)
        .expect("the second starts");
    let control_swept = removed_while_running(&control_old);
    let survived = old.exists();
    let _ = paused.kill();
    let _ = running.kill();
    let _ = paused.wait();
    let _ = running.wait();

    assert!(control_swept, "the sweep never ran, so nothing was shown");
    assert!(survived, "a period that cannot be read removed a record");
}
