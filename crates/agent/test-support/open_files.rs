//! The open-file limit a test binary that starts many mock servers needs.
//!
//! A mock server in these binaries keeps its listener open until the process exits, and the tests
//! run in parallel, so the descriptors held grow with the number of tests. The default soft limit
//! of a macOS shell is 256, which a binary of dozens of such tests exceeds: `try_clone`, `bind` and
//! `connect` then fail with `TooManyOpenFiles` in a different set of tests on each run.

#[cfg(unix)]
use std::sync::Once;

/// The soft limit a test binary asks for, or the hard limit where that is lower.
#[cfg(unix)]
pub const WANTED_OPEN_FILES: u64 = 8192;

/// The soft limit this process runs under, or `None` where it is unlimited.
#[cfg(unix)]
pub fn open_file_limit() -> Option<u64> {
    rustix::process::getrlimit(rustix::process::Resource::Nofile).current
}

/// Raise the soft open-file limit to `WANTED_OPEN_FILES`, or to the hard limit where that is
/// lower, once per process. A limit already higher is left as it is, and a refusal leaves the
/// limit as it was. Called by [`bind_mock_server`] and safe to call from anywhere else.
#[cfg(unix)]
pub fn raise_open_file_limit() {
    static RAISED: Once = Once::new();
    RAISED.call_once(|| {
        let mut limit = rustix::process::getrlimit(rustix::process::Resource::Nofile);
        let wanted = limit
            .maximum
            .map_or(WANTED_OPEN_FILES, |hard| hard.min(WANTED_OPEN_FILES));
        if limit.current.is_some_and(|soft| soft >= wanted) {
            return;
        }
        limit.current = Some(wanted);
        let _ = rustix::process::setrlimit(rustix::process::Resource::Nofile, limit);
    });
}

/// Windows has no descriptor limit of this kind.
#[cfg(not(unix))]
pub fn raise_open_file_limit() {}

/// Bind the listener of a mock server on a free local port, after raising the open-file limit.
/// Every mock server of a test binary starts here, so a new one cannot forget the raise.
pub fn bind_mock_server() -> std::net::TcpListener {
    raise_open_file_limit();
    std::net::TcpListener::bind("127.0.0.1:0").expect("bind")
}
