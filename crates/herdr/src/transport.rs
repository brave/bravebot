//! One request to herdr over its socket: connect, write a line, read the answer's line, close.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use rustix::io::{FdFlags, fcntl_setfd, ioctl_fionbio};
use rustix::net::{AddressFamily, SocketAddrUnix, SocketType, connect, socket};

/// The most of herdr's answer that is read. Nothing in it is used, so this only bounds the read.
const MOST_ANSWER_BYTES: u64 = 64 * 1024;

/// Sends `line` and waits for herdr's answer, all within `within`.
///
/// The answer is read so a release on exit has reached herdr before the process ends; what it says
/// is not looked at, since nothing here acts on a refusal.
pub(crate) fn exchange(socket: &Path, line: &str, within: Duration) -> io::Result<()> {
    let deadline = Instant::now() + within;
    let mut stream = connected(socket)?;
    stream.set_write_timeout(Some(left(deadline)?))?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.set_read_timeout(Some(left(deadline)?))?;
    let mut answer = Vec::new();
    BufReader::new((&stream).take(MOST_ANSWER_BYTES)).read_until(b'\n', &mut answer)?;
    Ok(())
}

/// A stream connected to the socket at `path`, which no program started afterwards inherits.
///
/// The standard library's connect creates the socket and marks it close-on-exec in two steps on
/// macOS, so a program another thread starts between them would hold the connection. The socket is
/// therefore created and marked under the gate every start of a program takes. It is connected
/// outside the gate and without blocking, so a herdr that has stopped accepting cannot hold up the
/// programs waiting at the gate: a full backlog is a failed connect rather than a wait.
pub(crate) fn connected(path: &Path) -> io::Result<UnixStream> {
    let address = SocketAddrUnix::new(path)?;
    let fd: OwnedFd = bravebot_sandbox::process::creating_a_channel(|| {
        let fd = socket(AddressFamily::UNIX, SocketType::STREAM, None)?;
        fcntl_setfd(&fd, FdFlags::CLOEXEC)?;
        Ok::<_, rustix::io::Errno>(fd)
    })?;
    ioctl_fionbio(&fd, true)?;
    connect(&fd, &address)?;
    ioctl_fionbio(&fd, false)?;
    Ok(UnixStream::from(fd))
}

fn left(deadline: Instant) -> io::Result<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(io::ErrorKind::TimedOut.into());
    }
    Ok(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// A program a session starts must not hold the connection to herdr: herdr's socket can type
    /// into every pane, and a confined program refused the socket's path would reach it anyway.
    #[test]
    fn the_connection_to_herdr_is_closed_on_exec() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("herdr.sock");
        let _listener = UnixListener::bind(&path).unwrap();
        let stream = connected(&path).unwrap();
        let flags = rustix::io::fcntl_getfd(&stream).unwrap();
        assert!(flags.contains(FdFlags::CLOEXEC));
    }

    /// Creating the socket outside the gate leaves a window in which a program another thread starts
    /// inherits it, which is the descriptor the clause above keeps from every program.
    #[test]
    fn a_connection_to_herdr_waits_for_a_program_being_started() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("herdr.sock");
        let _listener = UnixListener::bind(&path).unwrap();
        bravebot_sandbox::process::start_waits_at_the_gate(move || {
            let _ = connected(&path);
        });
    }

    /// The bound is what lets a release run on the way out of the program.
    #[test]
    fn an_exchange_with_a_herdr_that_never_answers_ends_at_its_bound() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("herdr.sock");
        let _listener = UnixListener::bind(&path).unwrap();
        let started = Instant::now();
        let outcome = exchange(&path, "{}", Duration::from_millis(200));
        assert!(outcome.is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
