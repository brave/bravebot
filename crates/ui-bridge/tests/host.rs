// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The host over a channel, without a model or a home: what it refuses, who it says it is, and
//! what it does with a line that is too long or a client that returns.
//!
//! Sessions, the store and a turn are in `host_process.rs`, which gives the host a home of its own.

#![cfg(unix)]

use bravebot_ui_bridge::host::{Ended, Host, MAX_LINE, Setup};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(30);

fn a_host(workspaces: Vec<std::path::PathBuf>) -> Host {
    Host::new(Setup {
        workspaces,
        settings: None,
        inspection: "refused",
    })
}

/// A client of a host running on a thread, and the host back when the client has gone.
struct Client {
    stream: UnixStream,
    said: mpsc::Receiver<Value>,
    served: JoinHandle<(Host, Ended)>,
    next: u64,
}

impl Client {
    fn attach(mut host: Host) -> Self {
        let (near, far) = UnixStream::pair().unwrap();
        let reader = BufReader::new(near.try_clone().unwrap());
        let (sender, said) = mpsc::channel();
        std::thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    let _ = sender.send(value);
                }
            }
        });
        let served = std::thread::spawn(move || {
            let ended = host.serve(far).expect("the channel serves");
            (host, ended)
        });
        Self {
            stream: near,
            said,
            served,
            next: 0,
        }
    }

    fn answered(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        let line = json!({"id": id, "method": method, "params": params});
        writeln!(self.stream, "{line}").unwrap();
        loop {
            let message = self.said.recv_timeout(PATIENCE).expect("an answer");
            if message["id"] == id {
                return message;
            }
        }
    }

    /// The client goes away, and the host comes back with how the connection ended.
    fn leave(self) -> (Host, Ended) {
        // The reader thread holds another descriptor for the same socket, so dropping this one
        // would not end the connection.
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        self.served.join().expect("the host did not panic")
    }
}

/// Which host a client reaches is something it can tell, and a host that starts again is another.
#[test]
fn each_host_has_an_instance_of_its_own_and_says_it_without_its_state_directory() {
    let mut client = Client::attach(a_host(Vec::new()));
    let info = client.answered("agent.info", json!({}));
    let first = info["ok"]["instance"].as_str().expect("an instance");
    assert!(!first.is_empty());
    assert_eq!(info["ok"]["inspection"], "refused");
    assert!(
        info["ok"].get("home").is_none(),
        "the state directory leaked: {info}"
    );
    let listed = client.answered("instance.info", json!({}));
    assert_eq!(listed["ok"]["instance"], first);
    assert_eq!(listed["ok"]["sessions"], json!([]));
    let (host, _) = client.leave();
    assert_eq!(host.instance(), first);

    let other = a_host(Vec::new());
    assert_ne!(other.instance(), first, "two hosts shared an instance");
}

/// The ready event is the first thing a client reads. It carries the same facts, and no path.
#[test]
fn the_ready_event_names_the_instance_and_not_the_state_directory() {
    let mut client = Client::attach(a_host(Vec::new()));
    let ready = loop {
        let message = client.said.recv_timeout(PATIENCE).expect("a message");
        if message["event"] == "agent.ready" {
            break message;
        }
    };
    assert!(ready["data"].get("home").is_none(), "{ready}");
    client.answered("instance.info", json!({}));
    client.leave();
}

#[test]
fn a_method_the_host_does_not_offer_is_refused_and_does_nothing() {
    let mut client = Client::attach(a_host(Vec::new()));
    for (method, params) in [
        ("session.list", json!({})),
        ("settings.select", json!({"path": "/tmp/anything.json"})),
        ("session.open", json!({"directory": "/", "id": "x"})),
        ("doctor", json!({})),
        ("a.method.nobody.has", json!({})),
    ] {
        let answered = client.answered(method, params);
        assert_eq!(
            answered["error"]["code"], "not_allowed",
            "{method}: {answered}"
        );
        assert!(
            answered.get("ok").is_none(),
            "{method} was served: {answered}"
        );
    }
    client.leave();
}

#[test]
fn a_session_cannot_be_opened_outside_the_workspaces() {
    let configured = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let link = configured.path().join("to-elsewhere");
    std::os::unix::fs::symlink(elsewhere.path(), &link).unwrap();
    let mut client = Client::attach(a_host(vec![configured.path().to_path_buf()]));

    for directory in [
        elsewhere.path().to_path_buf(),
        link,
        configured
            .path()
            .join("..")
            .join(elsewhere.path().file_name().unwrap()),
        configured.path().join("does-not-exist"),
        std::path::PathBuf::from("/"),
    ] {
        let answered = client.answered("session.new", json!({"directory": directory}));
        assert_eq!(
            answered["error"]["code"],
            "not_allowed",
            "{}: {answered}",
            directory.display()
        );
    }
    let listed = client.answered("instance.info", json!({}));
    assert_eq!(
        listed["ok"]["sessions"],
        json!([]),
        "a refused open left a session"
    );
    client.leave();
}

/// A client that sends a line that never ends cannot make the host buffer it without bound. The
/// connection ends, and a request that fits is not affected by an earlier one that did not.
#[test]
fn a_line_that_does_not_end_ends_the_connection() {
    let mut client = Client::attach(a_host(Vec::new()));
    client.answered("instance.info", json!({}));
    let chunk = vec![b'a'; 1024 * 1024];
    // One byte past the limit, with no newline. The peer may close its end while this is still
    // being written, so a failed write is the host having stopped reading.
    for _ in 0..=(MAX_LINE / chunk.len()) {
        if client.stream.write_all(&chunk).is_err() {
            break;
        }
    }
    let _ = client.stream.write_all(b"a");
    let (_, ended) = client.leave();
    assert_eq!(ended, Ended::Oversized);
}

/// A line of the largest size is read, so the limit is not met by a request that fits.
#[test]
fn a_request_at_the_limit_is_read() {
    let mut client = Client::attach(a_host(Vec::new()));
    let padding = "x".repeat(MAX_LINE - 100);
    let answered = client.answered("instance.info", json!({ "padding": padding }));
    assert_eq!(answered["error"]["code"], "not_allowed");
    assert!(
        answered["error"]["message"]
            .as_str()
            .unwrap()
            .contains("padding"),
        "the request was not read whole: {answered}"
    );
    let (_, ended) = client.leave();
    assert_eq!(ended, Ended::Closed);
}

/// The host is what persists: a client that leaves and one that comes after it reach the same
/// instance.
#[test]
fn a_second_client_reaches_the_host_the_first_left() {
    let mut first = Client::attach(a_host(Vec::new()));
    let instance = first.answered("instance.info", json!({}))["ok"]["instance"].clone();
    let (host, ended) = first.leave();
    assert_eq!(ended, Ended::Closed);

    let mut second = Client::attach(host);
    let again = second.answered("instance.info", json!({}))["ok"]["instance"].clone();
    assert_eq!(again, instance);
    second.leave();
}

/// A client that stops reading must not stop the host: the host is blocked writing to it, so it
/// would never read the client's end of the connection or see it go. Here the client sends more
/// than the replies can fit in the socket, reads none of them, and closes its sending side.
#[test]
fn a_client_that_never_reads_does_not_wedge_the_host() {
    let (near, far) = UnixStream::pair().unwrap();
    let mut host = a_host(Vec::new());
    let (ended, ending) = mpsc::channel();
    std::thread::spawn(move || {
        let outcome = host.serve(far).expect("the channel serves");
        let _ = ended.send(outcome);
    });
    let mut writer = near.try_clone().unwrap();
    std::thread::spawn(move || {
        let line = json!({"id": 1, "method": "instance.info", "params": {}}).to_string();
        for _ in 0..20_000 {
            if writeln!(writer, "{line}").is_err() {
                return;
            }
        }
        let _ = writer.shutdown(std::net::Shutdown::Write);
    });
    let outcome = ending
        .recv_timeout(Duration::from_secs(40))
        .expect("the host never got past writing to a client that does not read");
    assert_eq!(
        outcome,
        Ended::Unwritable,
        "requests went on being executed with nobody able to hear the answers"
    );
    drop(near);
}

/// A request cut off by the client going away is not run, whether or not what arrived happens to
/// parse: nobody is left to hear the answer, and a half-sent request must not behave differently
/// from one that does not parse.
#[test]
fn a_last_line_with_no_newline_is_not_run() {
    let (near, far) = UnixStream::pair().unwrap();
    let mut host = a_host(Vec::new());
    let served = std::thread::spawn(move || host.serve(far).expect("the channel serves"));
    let mut writer = near.try_clone().unwrap();
    write!(
        writer,
        r#"{{"id": 7, "method": "instance.info", "params": {{}}}}"#
    )
    .unwrap();
    writer.shutdown(std::net::Shutdown::Write).unwrap();

    let said: Vec<Value> = BufReader::new(near)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect();
    served.join().unwrap();
    assert!(
        said.iter().all(|message| message["id"] != 7),
        "a request with no end of line was answered: {said:?}"
    );
}
