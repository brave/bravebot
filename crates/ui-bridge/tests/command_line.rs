// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the transport binaries do with their command line. `bravebot-rpc` and `bravebot-acp` read
//! it through one parser, and each prints the answer under its own name and ends as the answer
//! says.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::process::{Command, Stdio};

/// Each stdio transport, the name it gives itself, what its usage line says it does, and whether
/// it writes a diagnostic log and so accepts `--log-level`.
const TRANSPORTS: [(&str, &str, &str, bool); 2] = [
    (
        env!("CARGO_BIN_EXE_bravebot-rpc"),
        "bravebot-rpc",
        "speak NDJSON on stdin/stdout",
        true,
    ),
    (
        env!("CARGO_BIN_EXE_bravebot-acp"),
        "bravebot-acp",
        "speak ACP on stdin/stdout",
        false,
    ),
];

struct Ran {
    status: Option<i32>,
    out: String,
    err: String,
}

fn run(binary: &str, args: &[&str]) -> Ran {
    let output = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs");
    Ran {
        status: output.status.code(),
        out: String::from_utf8_lossy(&output.stdout).into_owned(),
        err: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn version_line(name: &str) -> String {
    format!(
        "{name} {} (agent {})\n",
        env!("CARGO_PKG_VERSION"),
        bravebot_ui_bridge::agent_build()
    )
}

/// A script that asks a binary which build it is reads one line naming that binary, and the binary
/// ends without serving.
#[test]
fn each_transport_prints_its_own_version_line_and_ends() {
    for (binary, name, _, _) in TRANSPORTS {
        for flag in ["--version", "-V"] {
            let ran = run(binary, &[flag]);
            assert_eq!(ran.status, Some(0), "{name} {flag}: {}", ran.err);
            assert_eq!(ran.out, version_line(name), "{name} {flag}");
        }
    }
}

/// A command line a transport cannot serve ends it with status 2 and says why on standard error,
/// with the usage under the transport's own name. Standard output is the protocol's channel, so
/// nothing is written there; a caller reading it would take a refusal for a message. A transport
/// that writes no log refuses `--log-level` whatever word follows, and its usage does not name it.
#[test]
fn each_transport_refuses_a_command_line_it_cannot_serve_with_status_2() {
    for (binary, name, purpose, logs) in TRANSPORTS {
        let not_accepted = format!("--log-level is not accepted by {name}, which writes no log\n");
        let (no_word, unknown_word, usage) = if logs {
            (
                "--log-level requires error, info or debug\n".to_string(),
                "unknown log level: verbose\n".to_string(),
                format!("usage: {name} [--settings <path>] [--log-level <error|info|debug>]"),
            )
        } else {
            (
                not_accepted.clone(),
                not_accepted,
                format!("usage: {name} [--settings <path>]"),
            )
        };
        let cases: [(&[&str], String); 5] = [
            (&["--settings"], "--settings requires a file path\n".into()),
            (
                &["--settings", "first.json", "second.json"],
                "unexpected argument: second.json\n".into(),
            ),
            (&["--log-level"], no_word),
            (&["--log-level", "verbose"], unknown_word),
            (
                &["--sessions-root"],
                format!(
                    "unknown option: --sessions-root\n{usage}\n           {purpose}\n       {name} --version\n"
                ),
            ),
        ];
        for (args, said) in cases {
            let ran = run(binary, args);
            assert_eq!(ran.status, Some(2), "{name} {args:?}: {}", ran.err);
            assert!(
                ran.err.ends_with(&said),
                "{name} {args:?} said {:?}, not {said:?}",
                ran.err
            );
            assert_eq!(ran.out, "", "{name} {args:?} wrote to standard output");
        }
    }
}

/// The host prints its version in the same form as the two transports.
#[cfg(unix)]
#[test]
fn the_host_prints_its_version_line_in_the_same_form() {
    let ran = run(env!("CARGO_BIN_EXE_bravebot-host"), &["--version"]);
    assert_eq!(ran.status, Some(0), "{}", ran.err);
    assert_eq!(ran.out, version_line("bravebot-host"));
}
