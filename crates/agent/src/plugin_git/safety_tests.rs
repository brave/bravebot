//! Tests that run the real `git` against planted repositories and stand-in servers, to show that
//! each pinned option of the clone is honoured by git and that the person's environment cannot
//! choose code for it (SANDBOX-30, PLUGIN-3). They need `git` on `PATH` and a Unix shell.

use super::*;
use crate::confirm::FetchRequest;
use crate::plugins::{Asker, Refusal};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

mod environment_tests;
mod planted_source_tests;
mod support_tests;

use support_tests::{Markers, as_if_held, clone_as_if_held, home_with, planted};

fn real_git() -> Resolved {
    locate().expect("git must be on PATH for this test")
}

/// Writes an executable shell script `name` in `dir` and returns its path.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A loopback listener that counts the connections made to it. With a `reply` it answers each
/// connection with those bytes after reading the request, and with none it closes at once.
struct Counter {
    port: u16,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Counter {
    fn start(reply: Option<String>) -> Self {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let port = socket.local_addr().unwrap().port();
        let count = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (counted, stopped) = (count.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                match socket.accept() {
                    Ok((mut stream, _)) => {
                        counted.fetch_add(1, Ordering::SeqCst);
                        if let Some(reply) = &reply {
                            stream.set_nonblocking(false).ok();
                            stream
                                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                                .ok();
                            let mut head = [0_u8; 4096];
                            let _ = stream.read(&mut head);
                            stream.write_all(reply.as_bytes()).ok();
                        }
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                }
            }
        });
        Self {
            port,
            count,
            stop,
            thread: Some(thread),
        }
    }

    fn connections(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
    }
}

/// An asker that says yes to both questions and counts the install questions.
struct Yes {
    installs: usize,
}

impl Asker for Yes {
    fn anybody_there(&self) -> bool {
        true
    }

    fn confirm_fetch_host(&mut self, _request: &FetchRequest) -> bool {
        true
    }

    fn confirm_install(&mut self, _staged: &crate::plugins::Staged) -> bool {
        self.installs += 1;
        true
    }

    fn confirm_update(
        &mut self,
        _staged: &crate::plugins::Staged,
        _installed: &crate::plugins::Installed,
        _before: &crate::plugins::Before,
    ) -> bool {
        false
    }
}

fn source_at(url: &str, requested: Option<&str>) -> GitSource {
    GitSource {
        url: url.to_string(),
        host: "local.test".to_string(),
        requested_ref: requested.map(str::to_string),
    }
}

/// An asker that counts both questions and answers yes to each.
struct Tally {
    hosts: usize,
    installs: usize,
}

impl Asker for Tally {
    fn anybody_there(&self) -> bool {
        true
    }

    fn confirm_fetch_host(&mut self, _request: &FetchRequest) -> bool {
        self.hosts += 1;
        true
    }

    fn confirm_install(&mut self, _staged: &crate::plugins::Staged) -> bool {
        self.installs += 1;
        true
    }

    fn confirm_update(
        &mut self,
        _staged: &crate::plugins::Staged,
        _installed: &crate::plugins::Installed,
        _before: &crate::plugins::Before,
    ) -> bool {
        false
    }
}

/// T4, S3 to S5, PLUGIN-3, issue test 4: an archive URL, an npm name, a plain URL, a `file://` URL
/// and a URL of another scheme are refused by `install_typed` with no host question, no install
/// question, no program started and no connection, although a `git` is available and every
/// protocol is allowed to it. The `git` given is a script that writes a marker, so a start shows.
///
/// The control is a git URL for the same host, which does put the host question and starts the
/// program: a build that refused everything would pass the rest.
///
/// Rejects: a refusal made after the host question, one made after the program was started, and a
/// classifier that lets an archive or an npm name through as a git source.
#[test]
fn an_archive_an_npm_name_and_a_plain_url_start_no_program_and_ask_nothing() {
    let markers = Markers::new();
    let listener = Counter::start(None);
    let program = tempfile::tempdir().unwrap();
    let stand_in = markers.script(program.path(), "git", "git-started", 1);
    let git = Resolved::found_earlier(&stand_in);
    let root = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let port = listener.port;
    let mut asker = Tally {
        hosts: 0,
        installs: 0,
    };
    for typed in [
        format!("https://127.0.0.1:{port}/x.tar.gz"),
        format!("https://127.0.0.1:{port}/x.zip#main"),
        "@scope/pkg".to_string(),
        "npm:left-pad".to_string(),
        format!("https://127.0.0.1:{port}/docs"),
        format!("http://127.0.0.1:{port}/x.git"),
        format!("file:///tmp/x.git:{port}"),
        format!("ftp://127.0.0.1:{port}/x.git"),
        format!("git://127.0.0.1:{port}/x.git"),
    ] {
        let answered = crate::plugins::install_typed_with(
            &typed,
            root.path(),
            &home,
            &mut asker,
            Some(git.clone()),
            &["file", "http", "https", "ssh", "git"],
            parent.path(),
        );
        assert!(
            matches!(answered, Err(Refusal::NotASource(_))),
            "{typed}: {answered:?}"
        );
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!((asker.hosts, asker.installs), (0, 0));
    assert_eq!(markers.seen(), Vec::<String>::new());
    assert_eq!(listener.connections(), 0);
    assert!(!home.exists());
    assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);

    let answered = crate::plugins::install_typed_with(
        &format!("https://127.0.0.1:{port}/x.git"),
        root.path(),
        &home,
        &mut asker,
        Some(git),
        &["https"],
        parent.path(),
    );
    assert_eq!(answered, Err(Refusal::CloneFailed));
    assert_eq!(asker.hosts, 1);
    assert_eq!(markers.seen(), vec!["git-started".to_string()]);
}

/// PLUGIN-3, SANDBOX-30: a protocol that is not on the list given to the clone is refused by git
/// itself, and one that is on it is used. A local path is fetched over `file` only when the list
/// names it.
///
/// Rejects: a start that gives git every protocol (`protocol.allow` left at its default), which
/// fetches over `file`, `git` or `ext` although the person was asked about an `https` host.
#[test]
fn a_protocol_that_is_not_on_the_list_is_refused_by_git() {
    let remote = crate::plugin_git::fixtures::remote();
    let url = remote.path.to_string_lossy().into_owned();
    let (refused, dir) = clone_as_if_held(&real_git(), &[], &url, None, &["https", "ssh"]);
    assert_eq!(refused.err(), Some(CloneFailed));
    assert!(!dir.path().join("tree").join("plugin.toml").exists());
    let (allowed, _) = clone_as_if_held(&real_git(), &[], &url, None, &["file"]);
    assert!(allowed.is_ok(), "the control: the same clone over file");
}

/// S8, issue test 3, FETCH-4: a source that answers with a redirect to another host is refused and
/// the other host sees no connection. The first listener is the host named in the URL and answers
/// 301 to `localhost:<port>`, which is a second listener that counts what reaches it.
///
/// git follows a redirect of its first request by default, so the second listener is reached unless
/// `http.followRedirects=false` is on the command line.
///
/// Rejects: a clone that leaves git's default redirect following on (the second listener counts a
/// connection), and a redirect read as an answer (the install is refused with nothing written). The
/// first listener's count above zero is the control: a build that never reached the host would
/// pass the rest.
#[test]
fn a_redirect_to_another_host_is_not_followed_and_the_install_is_refused() {
    let second = Counter::start(None);
    let first = Counter::start(Some(format!(
        "HTTP/1.1 301 Moved Permanently\r\nLocation: http://localhost:{}/demo.git/info/refs?service=git-upload-pack\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        second.port
    )));
    let source = source_at(&format!("http://127.0.0.1:{}/demo.git", first.port), None);
    let parent = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let home = scratch.path().join("home");
    let mut asker = Yes { installs: 0 };
    let answered = crate::plugins::install_git(
        &source,
        &home,
        &mut asker,
        Some(real_git()),
        &["http"],
        parent.path(),
    );
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(answered, Err(Refusal::CloneFailed));
    assert!(first.connections() > 0, "the first host was never reached");
    assert_eq!(
        second.connections(),
        0,
        "a redirect to another host was followed"
    );
    assert_eq!(asker.installs, 0);
    assert!(!home.exists());
    assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
}
