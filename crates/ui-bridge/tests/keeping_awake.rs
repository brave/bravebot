// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The window's agreement to keep the computer awake is the terminal's `/caffeinate` agreement
//! (CMD-12): read from and written to `~/.bravebot/caffeinate`, so agreeing in either is agreeing
//! in both.
//!
//! Driven through the binary with a home of the test's own, since the file lives in the home and a
//! test in-process would be reading and writing the developer's.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};

/// A home of a test's own, under the build directory, removed when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join(".bravebot")).expect("a home directory");
        Self {
            path: path.canonicalize().expect("a real scratch directory"),
        }
    }

    fn agreement(&self) -> PathBuf {
        self.path.join(".bravebot/caffeinate")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct FrontEnd {
    child: Child,
    answers: BufReader<ChildStdout>,
    next: u64,
}

impl FrontEnd {
    fn start(home: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
            .env_clear()
            .env("HOME", home)
            .env("BRAVEBOT_LOCALE", "en-US")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the built binary runs");
        let answers = BufReader::new(child.stdout.take().expect("the front end answers"));
        Self {
            child,
            answers,
            next: 0,
        }
    }

    fn call(&mut self, method: &str) -> Value {
        self.next += 1;
        let id = self.next;
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            json!({"id": id, "method": method, "params": {}})
        )
        .unwrap();
        stdin.flush().unwrap();
        loop {
            let mut line = String::new();
            assert!(
                self.answers.read_line(&mut line).unwrap() > 0,
                "the front end ended before answering {method}"
            );
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message
                    .get("ok")
                    .cloned()
                    .unwrap_or_else(|| panic!("{method} failed: {message}"));
            }
        }
    }
}

impl Drop for FrontEnd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// CMD-12: a home where nobody has agreed answers no, and agreeing from the window writes the
/// word `/caffeinate` reads.
#[test]
fn agreeing_in_the_window_is_kept_where_caffeinate_reads_it() {
    let home = Scratch::new("keeping-awake-window");
    let mut front = FrontEnd::start(&home.path);
    assert_eq!(
        front.call("caffeinate.confirmed"),
        json!({"confirmed": false})
    );
    assert_eq!(front.call("caffeinate.confirm"), json!({"kept": true}));
    assert_eq!(
        std::fs::read_to_string(home.agreement()).unwrap(),
        "confirmed\n"
    );
    assert_eq!(
        front.call("caffeinate.confirmed"),
        json!({"confirmed": true})
    );
    assert!(bravebot_session::store::parse_caffeinate(
        &std::fs::read_to_string(home.agreement()).unwrap()
    ));
}

/// CMD-12: an agreement given to `/caffeinate` in the terminal is one the window does not ask
/// for again, and a file holding anything but the word is no agreement.
#[test]
fn an_agreement_from_the_terminal_is_the_windows() {
    let home = Scratch::new("keeping-awake-terminal");
    std::fs::write(home.agreement(), "confirmed\n").unwrap();
    assert_eq!(
        FrontEnd::start(&home.path).call("caffeinate.confirmed"),
        json!({"confirmed": true})
    );
    std::fs::write(home.agreement(), "maybe\n").unwrap();
    assert_eq!(
        FrontEnd::start(&home.path).call("caffeinate.confirmed"),
        json!({"confirmed": false})
    );
}
