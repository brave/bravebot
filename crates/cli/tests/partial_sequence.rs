//! INPUT-53 through the built binary: the first bytes of an escape sequence do not hold the interface
//! in a read of the terminal until the rest arrives.
//!
//! The terminal is a pty the binary has as its controlling terminal, because a change of the
//! window's size reaches a program as SIGWINCH only through one. `script` offers no way to change the
//! size after the program starts, so a small Python program makes the pty and starts the binary on
//! it, as `hangup.rs` does for a pty with no session.
//!
//! These tests run on Linux and macOS, since the home directory the binary reads is set by `HOME`.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::process::Command;

mod chatgpt_run;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;
mod terminal;

use chatgpt_run::{Scratch, environment};
use chatgpt_stand_in::StandIn;

/// Starts the program it is given on a pty, waits until the interface has drawn and gone quiet,
/// writes `ESC [`, changes the size of the window and prints how many bytes the interface drew in
/// the next two and a half seconds: `redrawn <count>`, or `never drew` where it wrote nothing in
/// sixty. The sequence is never finished, so what is drawn is drawn while the interface still holds
/// the first two bytes of it.
const DRIVER: &str = r#"
import fcntl, os, pty, select, signal, struct, sys, termios, time

child, master = pty.fork()
if child == 0:
    os.environ["TERM"] = "xterm-256color"
    os.execv(sys.argv[1], sys.argv[1:])


def resize(rows, columns):
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))


def drawn_for(seconds):
    """How many bytes the program writes in `seconds`."""
    total, end = 0, time.time() + seconds
    while time.time() < end:
        if select.select([master], [], [], 0.1)[0]:
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            if not data:
                break
            total += len(data)
    return total


try:
    resize(40, 120)
    wait, wrote = 60.0, False
    while select.select([master], [], [], wait)[0]:
        try:
            data = os.read(master, 65536)
        except OSError:
            break
        if not data:
            break
        wait, wrote = 1.5, True
    if not wrote:
        print("never drew")
    else:
        os.write(master, b"\x1b[")
        time.sleep(0.5)
        resize(30, 100)
        print("redrawn", drawn_for(2.5))
finally:
    os.kill(child, signal.SIGKILL)
    os.waitpid(child, 0)
"#;

/// INPUT-53: a window resized while the interface holds the first two bytes of a sequence is drawn
/// again at once.
///
/// The regression it rejects is the tty read again, with nothing waiting, after a read that finished
/// no event. The tty is a blocking descriptor, so that read does not return until another byte
/// arrives, and the resize, which is handled by the thread the read holds, waits with it: the
/// interface draws nothing until the person types the rest of the sequence. A terminal splits a
/// sequence across two reads whenever it writes slowly, as an ssh link does.
#[test]
fn a_window_resized_while_half_a_sequence_is_waiting_is_drawn_again() {
    let home = Scratch::new("cli-partial-sequence");
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
    let said = String::from_utf8_lossy(&output.stdout);

    let drawn: usize = said
        .trim()
        .strip_prefix("redrawn ")
        .unwrap_or_else(|| {
            panic!(
                "the interface did not draw before the sequence began: {said}{}",
                String::from_utf8_lossy(&output.stderr)
            )
        })
        .parse()
        .expect("a count of bytes");
    assert!(
        drawn > 0,
        "nothing was drawn for the resize while half a sequence was waiting"
    );
}
