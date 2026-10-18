//! The network namespace a Linux stage runs in when it is given no network, or only a proxy.
//!
//! The stage is started in a new user namespace and a new network namespace, so the only
//! interface it has is its own loopback and it can reach nothing outside it, whatever the port or
//! protocol. A stage held to the allowed-hosts proxy ([SANDBOX-24]) gets one more thing: a
//! forwarder that listens on the proxy's port on that loopback and relays each connection to the
//! unix socket the proxy listens on, which crosses the namespace because it is a path.
//!
//! Everything here that runs between `fork` and `exec` is written against the C library only and
//! allocates nothing, because the parent may have other threads and its allocator may be locked
//! in the child. What it needs is built beforehand in [`Namespace::prepare`].
//!
//! [SANDBOX-24]: ../../../docs/specs/sandboxing.md

use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::OnceLock;

/// How much is moved between the two ends of a forwarded connection at a time.
const RELAY_BUFFER: usize = 16 * 1024;

/// The forwarder's side of the connection to the proxy, built before the fork.
struct Forward {
    /// The port the forwarder listens on, on the namespace's loopback.
    port: u16,
    socket: libc::sockaddr_un,
    socket_length: libc::socklen_t,
}

/// What a stage's namespace is made from, prepared before the fork.
pub(crate) struct Namespace {
    uid_map: Vec<u8>,
    gid_map: Vec<u8>,
    forward: Option<Forward>,
}

impl Namespace {
    /// A namespace holding nothing but its loopback, or one that also relays `port` to the unix
    /// socket at `forward_to`.
    ///
    /// The account is mapped to itself, so files keep the owner they have and the stage holds no
    /// capability once it has executed: an account other than root loses every one it had in the
    /// namespace at the exec.
    pub(crate) fn prepare(forward: Option<(u16, &Path)>) -> Result<Self, String> {
        let (uid, gid) = identity();
        let forward = forward
            .map(|(port, path)| {
                let mut socket: libc::sockaddr_un = zeroed_sockaddr();
                socket.sun_family = libc::AF_UNIX as libc::sa_family_t;
                let bytes = path.as_os_str().as_bytes();
                if bytes.is_empty() || bytes.len() >= socket.sun_path.len() || bytes.contains(&0) {
                    return Err(format!(
                        "the proxy's socket path `{}` does not fit a unix socket address",
                        path.display()
                    ));
                }
                for (slot, byte) in socket.sun_path.iter_mut().zip(bytes) {
                    *slot = *byte as libc::c_char;
                }
                Ok(Forward {
                    port,
                    socket,
                    socket_length: std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
                })
            })
            .transpose()?;
        Ok(Self {
            uid_map: format!("{uid} {uid} 1").into_bytes(),
            gid_map: format!("{gid} {gid} 1").into_bytes(),
            forward,
        })
    }

    /// Moves the calling process into the namespace and starts the forwarder. Called between the
    /// fork and the exec, so it is async-signal-safe and allocates nothing.
    ///
    /// Fails closed: an error leaves the process to be abandoned by the caller, which does not
    /// exec.
    pub(crate) fn enter(&self) -> io::Result<()> {
        isolate(&self.uid_map, &self.gid_map)?;
        match &self.forward {
            Some(forward) => start_forwarder(forward),
            None => Ok(()),
        }
    }
}

fn zeroed_sockaddr() -> libc::sockaddr_un {
    libc::sockaddr_un {
        sun_family: 0,
        sun_path: [0; 108],
    }
}

/// The account this process runs as.
#[allow(unsafe_code)]
fn identity() -> (libc::uid_t, libc::gid_t) {
    // SAFETY: neither call takes a pointer or can fail.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe { (libc::geteuid(), libc::getegid()) }
}

/// Whether a stage can be started in a namespace of its own here, and if not, why.
///
/// Answered once by doing it in a child that exits straight away. Reading a switch would not do:
/// a kernel can allow the call and a profile or a container's filter refuse it, and the only
/// answer that holds is the call's own.
pub(crate) fn available() -> &'static Result<(), String> {
    static ANSWER: OnceLock<Result<(), String>> = OnceLock::new();
    ANSWER.get_or_init(probe)
}

#[allow(unsafe_code)]
fn probe() -> Result<(), String> {
    let namespace = Namespace::prepare(None)?;
    // SAFETY: the child runs only `isolate`, which is async-signal-safe, and leaves with
    // `_exit`; the parent waits for exactly the child it made.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        let child = libc::fork();
        if child < 0 {
            return Err(format!("fork: {}", io::Error::last_os_error()));
        }
        if child == 0 {
            let code = match isolate(&namespace.uid_map, &namespace.gid_map) {
                Ok(()) => 0,
                Err(error) => error.raw_os_error().unwrap_or(1).clamp(1, 255),
            };
            libc::_exit(code);
        }
        let mut status = 0;
        loop {
            if libc::waitpid(child, &mut status, 0) >= 0 {
                break;
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(format!("waitpid: {error}"));
            }
        }
        if libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0 {
            return Ok(());
        }
        let code = if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else {
            1
        };
        Err(io::Error::from_raw_os_error(code).to_string())
    }
}

/// Creates the user and network namespaces, maps the account to itself and brings the loopback
/// up.
#[allow(unsafe_code)]
fn isolate(uid_map: &[u8], gid_map: &[u8]) -> io::Result<()> {
    // SAFETY: every call here is a plain system call on memory this function owns.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        if libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNET) != 0 {
            return Err(io::Error::last_os_error());
        }
        // The kernel refuses a gid map to a process that may still change its groups.
        write_file(c"/proc/self/setgroups", b"deny")?;
        write_file(c"/proc/self/uid_map", uid_map)?;
        write_file(c"/proc/self/gid_map", gid_map)?;
        loopback_up()
    }
}

#[allow(unsafe_code)]
fn write_file(path: &std::ffi::CStr, contents: &[u8]) -> io::Result<()> {
    // SAFETY: `path` is NUL-terminated and `contents` is a live slice.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        let fd = libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let written = libc::write(fd, contents.as_ptr().cast(), contents.len());
        let result = if written == contents.len() as isize {
            Ok(())
        } else if written < 0 {
            Err(io::Error::last_os_error())
        } else {
            Err(io::Error::from(io::ErrorKind::WriteZero))
        };
        libc::close(fd);
        result
    }
}

/// Sets `lo` up. A new network namespace has it down, and nothing in it, a forwarder included,
/// can bind an address until it is up.
#[allow(unsafe_code)]
fn loopback_up() -> io::Result<()> {
    // SAFETY: `request` is a zeroed `ifreq`, which is valid for every field, and the calls are
    // given its address and a socket this function closes.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        let socket = libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0);
        if socket < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut request: libc::ifreq = std::mem::zeroed();
        for (slot, byte) in request.ifr_name.iter_mut().zip(b"lo\0") {
            *slot = *byte as libc::c_char;
        }
        let mut result = Ok(());
        if libc::ioctl(socket, libc::SIOCGIFFLAGS, &mut request) != 0 {
            result = Err(io::Error::last_os_error());
        } else {
            request.ifr_ifru.ifru_flags |= (libc::IFF_UP | libc::IFF_RUNNING) as libc::c_short;
            if libc::ioctl(socket, libc::SIOCSIFFLAGS, &mut request) != 0 {
                result = Err(io::Error::last_os_error());
            }
        }
        libc::close(socket);
        result
    }
}

/// Listens on the namespace's loopback port and leaves a process behind that relays each
/// connection to the proxy's socket.
///
/// The listener exists before this returns, so a program that connects the moment it starts is
/// queued and not refused, and a failure to bind is the stage's failure to start. The forwarder
/// is forked before the filesystem rules are installed, so it is not confined by them and the
/// stage needs no grant to the proxy's socket. It is not a process the stage may attach to: the
/// ruleset keeps a confined process from tracing one outside its domain, and it is not
/// dumpable.
#[allow(unsafe_code)]
fn start_forwarder(forward: &Forward) -> io::Result<()> {
    // SAFETY: system calls on memory this function owns; the forked child runs only
    // `serve_forever`, which is async-signal-safe and never returns.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        let listener = libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
        if listener < 0 {
            return Err(io::Error::last_os_error());
        }
        let address = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: forward.port.to_be(),
            sin_addr: libc::in_addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            sin_zero: [0; 8],
        };
        if libc::bind(
            listener,
            std::ptr::addr_of!(address).cast(),
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        ) != 0
            || libc::listen(listener, 128) != 0
        {
            let error = io::Error::last_os_error();
            libc::close(listener);
            return Err(error);
        }
        let stage = libc::getpid();
        match libc::fork() {
            -1 => {
                let error = io::Error::last_os_error();
                libc::close(listener);
                Err(error)
            }
            0 => serve_forever(listener, stage, forward),
            _ => {
                // Closed here as well as by the exec, so nothing between the two sees it.
                libc::close(listener);
                Ok(())
            }
        }
    }
}

/// Makes the calling process die with `parent` and not be one a program may read or write the
/// memory of. A parent that has already gone ends it at once.
#[allow(unsafe_code)]
fn tie_to(parent: libc::pid_t) {
    // SAFETY: `prctl` and `getppid` take integers.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        libc::prctl(
            libc::PR_SET_PDEATHSIG,
            libc::SIGKILL as libc::c_ulong,
            0,
            0,
            0,
        );
        libc::prctl(libc::PR_SET_DUMPABLE, 0 as libc::c_ulong, 0, 0, 0);
        if libc::getppid() != parent {
            libc::_exit(1);
        }
    }
}

/// The forwarder: accepts each connection and hands it to a process of its own.
#[allow(unsafe_code)]
fn serve_forever(listener: libc::c_int, stage: libc::pid_t, forward: &Forward) -> ! {
    // SAFETY: system calls on integers and on memory this function owns.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        tie_to(stage);
        libc::signal(libc::SIGCHLD, libc::SIG_IGN);
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
        // The descriptors the stage had open are not this process's to hold.
        if listener > 0 {
            libc::syscall(
                libc::SYS_close_range,
                0 as libc::c_long,
                libc::c_long::from(listener - 1),
                0 as libc::c_long,
            );
        }
        libc::syscall(
            libc::SYS_close_range,
            libc::c_long::from(listener + 1),
            libc::c_long::from(u32::MAX),
            0 as libc::c_long,
        );
        let forwarder = libc::getpid();
        loop {
            let connection = libc::accept4(
                listener,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                libc::SOCK_CLOEXEC,
            );
            if connection < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                libc::_exit(1);
            }
            match libc::fork() {
                0 => {
                    libc::close(listener);
                    tie_to(forwarder);
                    relay(connection, forward);
                    libc::_exit(0);
                }
                _ => {
                    libc::close(connection);
                }
            }
        }
    }
}

/// Connects to the proxy's socket and copies bytes between it and `client` until both directions
/// have ended.
#[allow(unsafe_code)]
fn relay(client: libc::c_int, forward: &Forward) {
    // SAFETY: system calls on descriptors this function owns and on a buffer it owns.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        let proxy = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
        if proxy < 0
            || libc::connect(
                proxy,
                std::ptr::addr_of!(forward.socket).cast(),
                forward.socket_length,
            ) != 0
        {
            return;
        }
        let mut buffer = [0u8; RELAY_BUFFER];
        let mut ends = [client, proxy];
        while ends[0] >= 0 || ends[1] >= 0 {
            let mut wait = [
                libc::pollfd {
                    fd: ends[0],
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: ends[1],
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            if libc::poll(wait.as_mut_ptr(), 2, -1) < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return;
            }
            for side in 0..2 {
                if ends[side] < 0 || wait[side].revents == 0 {
                    continue;
                }
                let other = [client, proxy][1 - side];
                let read = libc::read(ends[side], buffer.as_mut_ptr().cast(), buffer.len());
                if read < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                if read <= 0 {
                    libc::shutdown(other, libc::SHUT_WR);
                    ends[side] = -1;
                    continue;
                }
                let mut sent = 0;
                while sent < read {
                    let wrote = libc::write(
                        other,
                        buffer.as_ptr().add(sent as usize).cast(),
                        (read - sent) as usize,
                    );
                    if wrote < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted
                    {
                        continue;
                    }
                    if wrote <= 0 {
                        return;
                    }
                    sent += wrote;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hosts::HostList;
    use crate::proxy::{Proxy, ProxyConfig};
    use std::ffi::OsStr;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    /// The proxy's socket address holds the bytes of its path, so a directory whose name is not
    /// UTF-8 is reached and is not mistaken for the one its replacement-character spelling names.
    /// The regression it rejects: a lossy conversion of the path, which would send the forwarder
    /// to a socket that is not the proxy's.
    #[test]
    fn the_socket_address_holds_the_bytes_of_a_path_that_is_not_utf8() {
        let invalid = Path::new(OsStr::from_bytes(b"/tmp/proxy-\xff.sock"));
        let lookalike = Path::new("/tmp/proxy-\u{FFFD}.sock");
        let address = |path: &Path| {
            let namespace = Namespace::prepare(Some((8123, path))).expect("a forward");
            namespace
                .forward
                .as_ref()
                .expect("a forward")
                .socket
                .sun_path
        };

        let held = address(invalid);
        let bytes = invalid.as_os_str().as_bytes();
        let expected: Vec<libc::c_char> = bytes.iter().map(|byte| *byte as libc::c_char).collect();
        assert_eq!(held[..bytes.len()], expected[..]);
        assert_eq!(held[bytes.len()], 0, "the address ends where the path does");
        assert_ne!(held, address(lookalike));
    }

    /// The forwarder on its own, in the host's network, which needs no namespace and so runs on a
    /// kernel that refuses one: a connection to its port is relayed to the proxy's socket and
    /// answered by the proxy, and it is gone once the process that started it is.
    ///
    /// The regressions it rejects: a relay to the wrong socket or none, a forwarder that lives on
    /// after its stage, and one that does not take a second connection.
    #[test]
    #[allow(unsafe_code)]
    fn the_forwarder_relays_to_the_proxy_and_ends_with_the_process_that_started_it() {
        let proxy = Proxy::start(ProxyConfig::new(HostList::default())).expect("a proxy");
        let socket = proxy.socket_path().expect("a unix socket").to_path_buf();
        let port = {
            let held = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("a port");
            held.local_addr().expect("an address").port()
        };
        let namespace = Namespace::prepare(Some((port, &socket))).expect("a forward");
        let forward = namespace.forward.as_ref().expect("a forward");

        // SAFETY: the child runs only `start_forwarder`, which is async-signal-safe, and then
        // waits for a signal; the parent kills it and waits for exactly it.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let stage = unsafe {
            let stage = libc::fork();
            assert!(stage >= 0, "fork");
            if stage == 0 {
                if start_forwarder(forward).is_err() {
                    libc::_exit(2);
                }
                loop {
                    libc::pause();
                }
            }
            stage
        };

        let ask = |host: &str| -> Option<String> {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match TcpStream::connect(("127.0.0.1", port)) {
                    Ok(stream) => break stream,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(_) => return None,
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("a timeout");
            write!(stream, "CONNECT {host}:443 HTTP/1.1\r\n\r\n").expect("a request");
            let mut answer = String::new();
            let _ = stream.read_to_string(&mut answer);
            Some(answer)
        };
        let first = ask("first.example").expect("the forwarder listened");
        let second = ask("second.example").expect("the forwarder took a second connection");

        // SAFETY: a signal to, and the wait for, the child made above.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe {
            libc::kill(stage, libc::SIGKILL);
            libc::waitpid(stage, std::ptr::null_mut(), 0);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while TcpStream::connect(("127.0.0.1", port)).is_ok() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let still_listening = TcpStream::connect(("127.0.0.1", port)).is_ok();

        assert!(first.starts_with("HTTP/1.1 403"), "{first}");
        assert!(second.starts_with("HTTP/1.1 403"), "{second}");
        let asked: Vec<_> = proxy
            .decisions()
            .into_iter()
            .filter_map(|decision| decision.host)
            .collect();
        assert_eq!(asked, ["first.example", "second.example"]);
        assert!(!still_listening, "the forwarder outlived its stage");
    }
}
