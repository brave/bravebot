//! Leaving when the terminal is gone.
//!
//! A terminal that closes sends the program in front of it SIGHUP, and the default action ends the
//! program. A program that was given a pty only as its standard input and output, by a launcher or a
//! test that gave it no session of its own, is sent nothing when that pty closes. Reading the
//! terminal then returns nothing for ever, which crossterm takes for no event yet and reads again, so
//! the interface spins at a core's worth of CPU until something kills it.
//!
//! [`end_with_the_terminal`] looks at the terminal from a thread of its own and ends the program the
//! way the signal would have. It looks from another thread because the one that reads never comes
//! back from crossterm once the terminal is gone.

#[cfg(unix)]
use std::os::fd::{AsFd, OwnedFd};
#[cfg(unix)]
use std::time::Duration;

/// How often the terminal is looked at. Nothing needs the program gone sooner than this, and a look
/// is one system call.
#[cfg(unix)]
const LOOK_EVERY: Duration = Duration::from_millis(500);

/// The status of a program ended by SIGHUP as a shell reports it: 128 and the signal's number.
#[cfg(unix)]
const HUNG_UP: i32 = 129;

/// End the program once the terminal on its standard input has hung up.
///
/// Nothing is watched where standard input is not a terminal: a program reading a pipe or a file is
/// ended by the end of it.
#[cfg(unix)]
pub(crate) fn end_with_the_terminal() {
    use std::io::IsTerminal;

    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return;
    }
    let Ok(terminal) = stdin.as_fd().try_clone_to_owned() else {
        return;
    };
    // A thread that cannot start leaves the program as it was, which is all a missing look costs.
    drop(watch(terminal, LOOK_EVERY, || std::process::exit(HUNG_UP)));
}

#[cfg(not(unix))]
pub(crate) fn end_with_the_terminal() {}

/// Look at `terminal` every `every`, and call `then` once, the first time it has hung up.
#[cfg(unix)]
fn watch(
    terminal: OwnedFd,
    every: Duration,
    then: impl FnOnce() + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("terminal-hangup".to_string())
        .spawn(move || {
            loop {
                std::thread::sleep(every);
                if hung_up(&terminal) {
                    then();
                    return;
                }
            }
        })
}

/// Whether the other end of `terminal` has closed.
///
/// Asks for input, without waiting, because that is what makes a hangup show: macOS reports it only
/// when the request names `POLLIN`, and Linux reports it either way. A key waiting to be read is
/// `POLLIN` alone and is not a hangup.
#[cfg(unix)]
fn hung_up(terminal: &impl AsFd) -> bool {
    use rustix::event::{PollFd, PollFlags, Timespec, poll};

    let mut look = [PollFd::new(terminal, PollFlags::IN)];
    let now = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    matches!(poll(&mut look, Some(&now)), Ok(ready) if ready > 0)
        && look[0].revents().contains(PollFlags::HUP)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;

    /// The two ends of a connection, standing for the terminal and whatever holds its other side.
    fn ends() -> (OwnedFd, UnixStream) {
        let (terminal, other) = UnixStream::pair().expect("a pair of ends");
        (OwnedFd::from(terminal), other)
    }

    /// A terminal nobody has closed is not hung up, idle or with a key waiting to be read. Rejects a
    /// look that takes anything readable for a hangup, which would end a session at the first key.
    #[test]
    fn a_terminal_that_is_idle_or_has_a_key_waiting_has_not_hung_up() {
        let (terminal, mut other) = ends();
        assert!(!hung_up(&terminal), "an idle terminal read as hung up");

        other.write_all(b"k").expect("a key");
        assert!(!hung_up(&terminal), "a key waiting read as a hangup");
    }

    /// A terminal whose other end has closed has hung up, whether or not a key is left unread.
    /// Rejects a look that asks for no events, which macOS answers with nothing however the other end
    /// stands.
    #[test]
    fn a_terminal_whose_other_end_closed_has_hung_up() {
        let (terminal, other) = ends();
        drop(other);
        assert!(hung_up(&terminal));

        let (terminal, mut other) = ends();
        other.write_all(b"k").expect("a key");
        drop(other);
        assert!(hung_up(&terminal), "an unread key hid the hangup");
    }

    /// The watch calls once, after the hangup and not before it. Rejects a watch that never calls,
    /// one that calls at once, and one that calls on each look.
    #[test]
    fn a_watch_calls_once_the_terminal_hangs_up_and_not_before() {
        let (terminal, other) = ends();
        let (called, heard) = mpsc::channel();
        let watching = watch(terminal, Duration::from_millis(10), move || {
            called.send(()).expect("a listener");
        })
        .expect("a thread");

        assert!(
            heard.recv_timeout(Duration::from_millis(200)).is_err(),
            "the watch called with the terminal open"
        );
        drop(other);
        heard
            .recv_timeout(Duration::from_secs(10))
            .expect("the watch never called");
        watching.join().expect("a watch that ended");
        assert!(heard.try_recv().is_err(), "the watch called twice");
    }
}
