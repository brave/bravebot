//! INPUT-51 through the built binary: a session whose terminal has been closed ends with the status
//! of a hangup, 129, and does not read the closed terminal for ever.
//!
//! The terminal is a pty the binary is handed as its standard input, output and error and holds no
//! session of. That is the case the clause is about, since a binary that has the pty as its
//! controlling terminal is sent SIGHUP when it closes and ends without the look. `script` makes the
//! binary's session, so a small Python program makes the pty and starts the binary in the test's own
//! session instead.
//!
//! These tests run on Linux and macOS, as `window_lookup.rs` does, since the home directory the
//! binary reads is set by `HOME`.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::process::Command;

mod chatgpt_run;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;
mod terminal;

use chatgpt_run::{Scratch, environment};
use chatgpt_stand_in::StandIn;

/// Starts the program it is given with a pty for its standard streams, waits until the interface has
/// drawn and gone quiet, closes the other end of the pty, and prints how the program ended: its
/// status, or `still running` where it had not ended thirty seconds later and was killed, or
/// `never drew` where it wrote nothing in sixty.
const DRIVER: &str = r#"
import os, pty, select, subprocess, sys, time

master, slave = pty.openpty()
child = subprocess.Popen(sys.argv[1:], stdin=slave, stdout=slave, stderr=slave)
os.close(slave)


def drawn_and_quiet():
    """Read what the program writes until it has written and then been quiet for a moment."""
    wait, wrote = 60.0, False
    while select.select([master], [], [], wait)[0]:
        try:
            if not os.read(master, 65536):
                break
        except OSError:
            break
        wait, wrote = 1.5, True
    return wrote


try:
    if not drawn_and_quiet():
        print("never drew")
    else:
        os.close(master)
        deadline = time.time() + 30
        while child.poll() is None and time.time() < deadline:
            time.sleep(0.1)
        print("still running" if child.poll() is None else child.returncode)
finally:
    if child.poll() is None:
        child.kill()
    child.wait()
"#;

/// INPUT-51: the interface that draws ends with status 129 once the other end of its terminal has
/// closed, and it looks from the start, so the questions it asks the terminal on the way in do not
/// keep it reading.
///
/// The binary is started with a pty and no session of its own, drawn on and left idle, and the other
/// end of the pty is then closed. A binary that does not look at its terminal reads the closed one
/// for ever, which the driver reports as `still running`. The regressions it rejects are a look that
/// is never started, one started somewhere the session does not pass, and a status other than the
/// one a shell reports for a program ended by SIGHUP.
#[test]
fn a_session_whose_terminal_closed_ends_with_the_status_of_a_hangup() {
    let home = Scratch::new("cli-hangup");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"model": "held/m", "provider": {"held": {"options": {"baseURL": "http://127.0.0.1:1/v1"}}}}"#,
    )
    .expect("write the settings");
    let stand_in = StandIn::new(vec![]);
    let mut command = Command::new("/usr/bin/python3");
    command
        .args(["-I", "-c", DRIVER, env!("CARGO_BIN_EXE_bravebot")])
        .arg("--dangerously-skip-permissions");
    environment(&mut command, &home, &stand_in);

    let output = command.output().expect("python runs the driver");
    let ended = String::from_utf8_lossy(&output.stdout);

    assert_eq!(
        ended.trim(),
        "129",
        "the session did not end with the status of a hangup: {ended}{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
