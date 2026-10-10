//! `run` under the kernel: a program a person asked for writes only the directories the session was
//! opened on, in the foreground and left running, and on Linux and macOS reads the machine except
//! the places that hold a credential.
//!
//! Each test skips where this machine cannot apply a profile, which includes a suite run from
//! inside another profile. On Windows a test does not skip: a machine that cannot create a
//! container profile fails them, since a job that confined nothing must not report green.

use bravebot_agent::confine::Confinement;
use bravebot_agent::exec;
use bravebot_core::cancel::Cancel;
use bravebot_core::command::Plan;
use bravebot_sandbox::network::Network;
#[cfg(unix)]
use std::net::TcpListener;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};

/// `path` with its links followed, spelled as a session directory is: on Windows without the
/// `\\?\` prefix `canonicalize` adds, which a command line cannot carry.
fn resolved(path: &Path) -> PathBuf {
    let path = path.canonicalize().expect("a canonical path");
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(ordinary) if cfg!(windows) => PathBuf::from(ordinary),
        _ => path,
    }
}

/// A line that prints `file`. Windows runs `cmd.exe`, a native program: Git's `cat` is an MSYS
/// program, which expects a POSIX layer a container does not hold.
fn cat(file: impl std::fmt::Display) -> String {
    if cfg!(windows) {
        format!("cmd /c type '{file}'")
    } else {
        format!("cat '{file}'")
    }
}

/// A line that makes `file`, or on Windows a directory of that name, where there is none.
fn touch(file: impl std::fmt::Display) -> String {
    if cfg!(windows) {
        format!("cmd /c mkdir '{file}'")
    } else {
        format!("touch '{file}'")
    }
}

/// A session directory, a second one beside it that the session was not opened on, and a home
/// directory holding a credential of each kind, all under the build directory.
struct Places {
    session: PathBuf,
    beside: PathBuf,
    home: PathBuf,
}

impl Places {
    fn new(name: &str) -> Self {
        let top = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(format!("confine-{name}"));
        let _ = std::fs::remove_dir_all(&top);
        let session = top.join("session");
        let beside = top.join("beside");
        std::fs::create_dir_all(&session).expect("session directory");
        std::fs::create_dir_all(&beside).expect("beside directory");
        std::fs::write(session.join("inside.txt"), "inside\n").expect("inside file");
        std::fs::write(beside.join("outside.txt"), "outside\n").expect("outside file");
        let home = top.join("home");
        for (path, contents) in [
            (".aws/credentials", "aws secret\n"),
            (".bravebot/gateway-keys.json", "gateway secret\n"),
            (".ssh/id_ed25519", "ssh secret\n"),
            (".ssh/id_ed25519.pub", "ssh public\n"),
            (".ssh/config", "ssh config\n"),
            (".config/gh/hosts.yml", "gh login\n"),
            (".gitconfig", "git config\n"),
        ] {
            let file = home.join(path);
            std::fs::create_dir_all(file.parent().expect("a parent")).expect("home directory");
            std::fs::write(file, contents).expect("home file");
        }
        Self {
            session: resolved(&session),
            beside: resolved(&beside),
            home: resolved(&home),
        }
    }

    fn confinement(&self) -> Confinement {
        Confinement::here(vec![self.session.clone()], None, Some(&self.home))
            .expect("a platform with a base")
    }

    fn plan(&self, line: &str) -> Plan {
        bravebot_agent::cmdline::compile(line, &self.session, None, &mut |_, _| Ok(()))
            .unwrap_or_else(|error| panic!("`{line}` should compile: {error}"))
    }

    fn run(&self, line: &str, confinement: Option<&Confinement>) -> exec::Ran {
        exec::run_plan_observed(
            &self.plan(line),
            &Cancel::new(),
            exec::LIMIT,
            None,
            None,
            confinement,
            &mut |_| Ok(()),
        )
        .unwrap_or_else(|error| panic!("`{line}` should start: {error}"))
    }
}

#[cfg(unix)]
fn can_confine() -> bool {
    bravebot_sandbox::base::Prelude::current().is_some()
        && bravebot_sandbox::confinement_works_here()
}

/// `confinement_works_here` starts `/bin/true`, which is not a Windows program, so it cannot say
/// whether a container can be created here and is not asked.
#[cfg(windows)]
fn can_confine() -> bool {
    bravebot_sandbox::for_current_platform()
        .expect("a container profile can be created on this machine");
    true
}

/// The regression it rejects, on a platform that lists what a program reaches: confinement passed
/// down and never applied, so a confined line reads whatever the account can. The unconfined run of
/// the same line is the control that the file is there to be read.
#[cfg(windows)]
#[test]
fn a_confined_program_cannot_read_a_file_outside_the_session() {
    if !can_confine() {
        return;
    }
    let places = Places::new("read");
    let line = cat(places.beside.join("outside.txt").display());

    let control = places.run(&line, None);
    let confined = places.run(&line, Some(&places.confinement()));

    assert!(control.ended_well, "the file is there to be read");
    assert_eq!(control.stdout, "outside\n");
    assert!(
        !confined.ended_well,
        "read outside the session: {confined:?}"
    );
    assert!(!confined.stdout.contains("outside"));
}

/// The regression it rejects: a profile that lists, so a script that starts a program the list
/// never heard of reaches nothing, or one that reads everything, so a credential is read by the
/// first program that asks. The unconfined run is the control that each file is there to be read.
#[cfg(unix)]
#[test]
fn a_confined_program_reads_the_machine_and_not_a_credential_location() {
    if !can_confine() {
        return;
    }
    let places = Places::new("read");
    let confinement = places.confinement();
    let reads = |path: &Path, confinement: Option<&Confinement>| {
        places.run(&format!("cat {}", path.display()), confinement)
    };

    for readable in [
        places.beside.join("outside.txt"),
        places.home.join(".gitconfig"),
        places.home.join(".config/gh/hosts.yml"),
        places.home.join(".ssh/config"),
        places.home.join(".ssh/id_ed25519.pub"),
    ] {
        let confined = reads(&readable, Some(&confinement));
        assert!(
            confined.ended_well && reads(&readable, None).stdout == confined.stdout,
            "{} was refused: {confined:?}",
            readable.display()
        );
    }
    for held_back in [
        places.home.join(".aws/credentials"),
        places.home.join(".ssh/id_ed25519"),
    ] {
        assert!(
            reads(&held_back, None).ended_well,
            "{} is there to be read",
            held_back.display()
        );
        let confined = reads(&held_back, Some(&confinement));
        assert!(
            !confined.ended_well && !confined.stdout.contains("secret"),
            "{} was read: {confined:?}",
            held_back.display()
        );
    }
}

/// SANDBOX-26: a strict stage that asked for `aws` reads the aws credential directory and nothing
/// else of the home, under the kernel. The controls are the same line with no request, which is
/// refused, and one that asked for another scope, which is too. The regression it rejects is a
/// request recorded and prompted for that never reaches the profile, or one that lifts every
/// credential.
#[cfg(unix)]
#[test]
fn a_strict_stage_that_asked_for_a_scope_reads_that_credential_only() {
    use bravebot_sandbox::SandboxMode;
    use bravebot_sandbox::scope::{Requested, Scope};
    if !can_confine() {
        return;
    }
    let places = Places::new("requested-scope");
    let strict = places.confinement().with_mode(SandboxMode::Strict);
    let read = |path: &str, confinement: &Confinement| {
        places.run(
            &format!("sh -c 'cat {}'", places.home.join(path).display()),
            Some(confinement),
        )
    };
    let asked = |scope| strict.clone().with_requested(&[Requested::Scope(scope)]);

    assert!(
        !read(".aws/credentials", &strict).ended_well,
        "the control read the credential with nothing asked for"
    );
    let lent = read(".aws/credentials", &asked(Scope::Aws));
    assert!(lent.ended_well, "{lent:?}");
    assert_eq!(lent.stdout, "aws secret\n");
    assert!(
        !read(".aws/credentials", &asked(Scope::Docker)).ended_well,
        "another scope lifted the aws directory"
    );
    assert!(
        !read(".ssh/id_ed25519", &asked(Scope::Aws)).ended_well,
        "a request for aws read a private key"
    );
}

/// The regression it rejects: a profile built once and kept, so a file created after the first
/// stage started cannot be read by the second, or a listing made at one moment that a path made
/// later in a directory holding a credential location escapes.
#[cfg(unix)]
#[test]
fn a_file_created_between_two_stages_is_readable_by_the_second() {
    if !can_confine() {
        return;
    }
    let places = Places::new("created");
    let confinement = places.confinement();
    let late = places.home.join(".config/late.txt");

    let before = places.run("cat inside.txt", Some(&confinement));
    std::fs::write(&late, "late\n").expect("a file made between stages");
    let after = places.run(&format!("cat {}", late.display()), Some(&confinement));

    assert!(before.ended_well, "{before:?}");
    assert_eq!(after.stdout, "late\n", "{after:?}");
}

/// The regression it rejects: a link judged by where it stands, so a program that is refused a
/// credential by its path is handed it by a link to it from a directory it can read.
#[cfg(unix)]
#[test]
fn a_link_to_a_credential_is_judged_by_where_it_leads() {
    if !can_confine() {
        return;
    }
    let places = Places::new("link");
    let file = places.beside.join("to-the-key");
    let directory = places.beside.join("to-the-directory");
    std::os::unix::fs::symlink(places.home.join(".aws/credentials"), &file).expect("a link");
    std::os::unix::fs::symlink(places.home.join(".aws"), &directory).expect("a link");
    let confinement = places.confinement();

    let by_file = places.run(&format!("cat {}", file.display()), Some(&confinement));
    let by_directory = places.run(
        &format!("cat {}/credentials", directory.display()),
        Some(&confinement),
    );

    for ran in [by_file, by_directory] {
        assert!(
            !ran.ended_well && !ran.stdout.contains("secret"),
            "a link reached a credential: {ran:?}"
        );
    }
}

/// The regression it rejects: a profile that denies the session's own directory, which is every
/// program refused.
#[test]
fn a_confined_program_reads_and_writes_inside_the_session() {
    if !can_confine() {
        return;
    }
    let places = Places::new("inside");
    let confinement = places.confinement();

    let read = places.run(&cat("inside.txt"), Some(&confinement));
    let wrote = places.run(&touch("made.txt"), Some(&confinement));

    assert_eq!(read.stdout, "inside\n", "{read:?}");
    assert!(wrote.ended_well, "{wrote:?}");
    assert!(places.session.join("made.txt").exists());
}

/// The regression it rejects: a write row wider than the session, which lets a program change a
/// file it was never given.
#[test]
fn a_confined_program_cannot_write_outside_the_session() {
    if !can_confine() {
        return;
    }
    let places = Places::new("write");
    let target = places.beside.join("planted.txt");
    let control = places.run(&touch("control.txt"), Some(&places.confinement()));

    let ran = places.run(&touch(target.display()), Some(&places.confinement()));

    assert!(
        control.ended_well && places.session.join("control.txt").exists(),
        "the same command could not write inside the session, so a refusal outside it means nothing: {control:?}"
    );
    assert!(!ran.ended_well, "{ran:?}");
    assert!(!target.exists());
}

/// The regression it rejects: the background path spawning the plain command. A job left running
/// is the one nobody is watching.
#[test]
fn a_program_left_running_is_confined_as_well() {
    if !can_confine() {
        return;
    }
    let places = Places::new("background");
    let target = places.beside.join("planted.txt");
    let finished = |line: &str| {
        let plan = places.plan(line);
        let steps = plan.steps.unrouted_pipeline().expect("one pipeline");
        let mut job = exec::start_steps(steps, &places.session, None, Some(&places.confinement()))
            .expect("the job starts");
        for _ in 0..200 {
            if job.ended() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        job
    };

    let mut control = finished(&touch("control.txt"));
    let control_ended = control.ended();
    let mut job = finished(&touch(target.display()));

    assert!(
        control_ended && places.session.join("control.txt").exists(),
        "a job could not write inside the session, so a refusal outside it means nothing"
    );
    assert!(job.ended(), "the job did not finish");
    assert!(!target.exists(), "the job wrote outside the session");
    assert_ne!(job.codes(), [Some(0)], "the write succeeded");
}

fn can_close_the_network() -> bool {
    can_confine()
        && bravebot_sandbox::for_current_platform()
            .is_ok_and(|sandbox| sandbox.capabilities().network_denial_enforced)
}

/// Whether anything connected to a loopback listener while `line` ran under `confinement`.
///
/// What is observed is the listener's accept, not the program's exit status: a program that is
/// refused and one that finds nothing listening exit alike, so only a connection that arrived says
/// the network was reached.
#[cfg(unix)]
fn reached_a_listener(
    places: &Places,
    line: impl FnOnce(u16) -> String,
    confinement: Option<&Confinement>,
) -> bool {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.set_nonblocking(true).expect("a polling listener");
    let port = listener.local_addr().expect("the bound address").port();
    let stop = Arc::new(AtomicBool::new(false));
    let watching = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut seen = false;
            while !stop.load(Ordering::Relaxed) {
                if listener.accept().is_ok() {
                    seen = true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            seen || listener.accept().is_ok()
        })
    };
    let _ = places.run(&line(port), confinement);
    stop.store(true, Ordering::Relaxed);
    watching.join().expect("the watcher")
}

/// The regression it rejects: a closed setting that is read and never applied, so a program with
/// no reason to reach the network reaches it. The same line under the open setting is the control
/// that this machine lets the connection through, without which the refusal means nothing.
#[cfg(unix)]
#[test]
fn a_closed_network_stops_a_program_with_no_reason_to_reach_it() {
    if !can_close_the_network() {
        return;
    }
    let places = Places::new("network-closed");
    let line = |port: u16| format!("nc -z 127.0.0.1 {port}");

    let control = reached_a_listener(&places, line, Some(&places.confinement()));
    let closed = places.confinement().with_network(Network::Closed);
    let refused = reached_a_listener(&places, line, Some(&closed));

    assert!(control, "the open network did not reach the listener");
    assert!(!refused, "a closed network let a connection through");
}

/// The regression it rejects: the closed setting taking egress from the stages it exists to leave
/// it with. A `git ls-remote` carries the remote scope and `curl` exists to fetch, and both reach
/// the listener with the network closed.
#[cfg(unix)]
#[test]
fn a_closed_network_keeps_a_remote_stage_and_curl_reaching_it() {
    if !can_close_the_network() {
        return;
    }
    let places = Places::new("network-remote");
    // Its own repository, since git finds the one above the build directory otherwise and is
    // refused it for being outside the session.
    let initialised = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&places.session)
        .status()
        .expect("git runs");
    assert!(initialised.success());
    let closed = places.confinement().with_network(Network::Closed);

    let remote = reached_a_listener(
        &places,
        |port| format!("git ls-remote git://127.0.0.1:{port}/repository.git"),
        Some(&closed),
    );
    let fetched = reached_a_listener(
        &places,
        |port| format!("curl -s --noproxy * -m 3 http://127.0.0.1:{port}/"),
        Some(&closed),
    );

    assert!(remote, "a remote stage lost its network");
    assert!(fetched, "curl lost its network");
}

/// The regression it rejects: a platform that cannot deny the network running the stage with it
/// open. It refuses, and the refusal names the setting.
#[test]
fn a_platform_that_cannot_close_the_network_refuses_the_stage() {
    if !can_confine() || can_close_the_network() {
        return;
    }
    let places = Places::new("network-refused");
    let closed = places.confinement().with_network(Network::Closed);

    let refused = exec::run_plan_observed(
        &places.plan("cat inside.txt"),
        &Cancel::new(),
        exec::LIMIT,
        None,
        None,
        Some(&closed),
        &mut |_| Ok(()),
    );

    assert!(
        matches!(&refused, Err(exec::ExecError::NotConfined { detail, .. }) if detail.contains("run.network")),
        "{refused:?}"
    );
}

/// The lists a person writes, as the command line and the settings file hand them over: each entry a
/// spelling and nothing else.
fn written(
    allow_read: &[&str],
    deny_read: &[&str],
    allow_write: &[&str],
    deny_write: &[&str],
) -> bravebot_sandbox::rules::Lists {
    let entries = |paths: &[&str]| {
        paths
            .iter()
            .map(|path| bravebot_sandbox::rules::Entry {
                path: (*path).to_string(),
                by: None,
                pinned: false,
            })
            .collect()
    };
    bravebot_sandbox::rules::Lists {
        allow_read: entries(allow_read),
        deny_read: entries(deny_read),
        allow_write: entries(allow_write),
        deny_write: entries(deny_write),
    }
}

/// The regression it rejects: a `denyRead` that is read and never applied, or applied to a path
/// other than the one it names. A file the session may read is refused by name and by glob, its
/// neighbour is still read, and the stage without the list reads both, which is the control that the
/// refusal is the list's.
#[cfg(unix)]
#[test]
fn a_denied_read_is_refused_by_name_and_by_glob_and_its_neighbour_is_not() {
    if !can_confine() {
        return;
    }
    let places = Places::new("filesystem-deny-read");
    std::fs::create_dir_all(places.session.join("sub")).expect("a directory");
    for file in ["secret.txt", "sub/prod.env", "sub/plain.txt"] {
        std::fs::write(places.session.join(file), "held\n").expect("a file");
    }
    let listed =
        places
            .confinement()
            .with_filesystem(&written(&[], &["secret.txt", "**/*.env"], &[], &[]));

    for held_back in ["secret.txt", "sub/prod.env"] {
        let line = format!("cat {held_back}");
        assert!(places.run(&line, None).ended_well, "{held_back} is there");
        let ran = places.run(&line, Some(&listed));
        assert!(
            !ran.ended_well && !ran.stdout.contains("held"),
            "{held_back} was read: {ran:?}"
        );
    }
    let neighbour = places.run("cat sub/plain.txt", Some(&listed));
    assert_eq!(neighbour.stdout, "held\n", "{neighbour:?}");
}

/// The regression it rejects: a refusal that holds only where the table already did. The `gh`
/// configuration is read by every stage, since the base does not refuse it, and the person's own
/// refusal of the directory takes it from the same stage; without the list it is there to be read.
#[cfg(unix)]
#[test]
fn a_denied_read_holds_back_a_directory_the_base_reads() {
    if !can_confine() {
        return;
    }
    let places = Places::new("filesystem-deny-scope");
    let target = places.home.join(".config/gh/hosts.yml");
    let line = format!("cat {}", target.display());
    let listed = places.confinement().with_filesystem(&written(
        &[],
        &[&places.home.join(".config/gh").display().to_string()],
        &[],
        &[],
    ));

    assert!(
        places.run(&line, Some(&places.confinement())).ended_well,
        "the file is read without the list"
    );
    let ran = places.run(&line, Some(&listed));
    assert!(!ran.ended_well, "{ran:?}");
}

/// The regression it rejects: an `allowRead` that cannot lift a row of the built-in table, and one
/// that lifts the one row no list may, a private key. Both are asked for; the credential directory
/// comes back and the key does not.
#[cfg(unix)]
#[test]
fn an_allowed_read_lifts_the_table_and_never_reaches_a_private_key() {
    if !can_confine() {
        return;
    }
    let places = Places::new("filesystem-allow-read");
    let home = &places.home;
    let listed = places.confinement().with_filesystem(&written(
        &[
            &home.join(".aws").display().to_string(),
            &home.join(".ssh/id_ed25519").display().to_string(),
        ],
        &[],
        &[],
        &[],
    ));

    let lifted = places.run(
        &format!("cat {}", home.join(".aws/credentials").display()),
        Some(&listed),
    );
    let key = places.run(
        &format!("cat {}", home.join(".ssh/id_ed25519").display()),
        Some(&listed),
    );

    assert_eq!(lifted.stdout, "aws secret\n", "{lifted:?}");
    assert!(
        !key.ended_well && !key.stdout.contains("secret"),
        "a list lifted a private key: {key:?}"
    );
}

/// The regression it rejects: an `allowWrite` that is not a write row, or a `denyWrite` that leaves
/// the row it narrows whole, or that lets a wider rule decide over a narrower one. With
/// `allowWrite` on a directory and `denyWrite` on a directory inside it, a file in the first is made,
/// one in the second is not, and the second is still read.
#[cfg(unix)]
#[test]
fn the_narrower_of_an_allowed_and_a_denied_write_decides() {
    if !can_confine() {
        return;
    }
    let places = Places::new("filesystem-write");
    let free = places.beside.join("free");
    let locked = places.beside.join("locked");
    std::fs::create_dir_all(&free).expect("a directory");
    std::fs::create_dir_all(&locked).expect("a directory");
    std::fs::write(locked.join("kept.txt"), "kept\n").expect("a file");
    let beside = places.beside.display().to_string();
    let listed = places.confinement().with_filesystem(&written(
        &[],
        &[],
        &[&beside],
        &[&locked.display().to_string()],
    ));

    let control = places.run(
        &format!("touch {}", free.join("made.txt").display()),
        Some(&places.confinement()),
    );
    let made = places.run(
        &format!("touch {}", free.join("made.txt").display()),
        Some(&listed),
    );
    let refused = places.run(
        &format!("touch {}", locked.join("planted.txt").display()),
        Some(&listed),
    );
    let read = places.run(
        &format!("cat {}", locked.join("kept.txt").display()),
        Some(&listed),
    );

    assert!(
        !control.ended_well,
        "the directory is writable without the list"
    );
    assert!(
        made.ended_well && free.join("made.txt").exists(),
        "{made:?}"
    );
    assert!(
        !refused.ended_well && !locked.join("planted.txt").exists(),
        "{refused:?}"
    );
    assert_eq!(
        read.stdout, "kept\n",
        "a refusal of writes took the read: {read:?}"
    );
}

/// The regression it rejects: a `denyWrite` that spares the session's own directory, which is the
/// case the list exists for: one file inside the project the program may not change.
#[cfg(unix)]
#[test]
fn a_denied_write_holds_back_a_file_inside_a_session_directory() {
    if !can_confine() {
        return;
    }
    let places = Places::new("filesystem-deny-write-session");
    std::fs::create_dir_all(places.session.join("sub")).expect("a directory");
    std::fs::write(places.session.join("sub/.env"), "VALUE=1\n").expect("a file");
    std::fs::write(places.session.join("sub/other.txt"), "other\n").expect("a file");
    let listed = places
        .confinement()
        .with_filesystem(&written(&[], &[], &[], &["sub/.env"]));
    let append = |file: &str, confinement: &Confinement| {
        places.run(&format!("tee -a sub/{file}"), Some(confinement))
    };

    assert!(
        append(".env", &places.confinement()).ended_well,
        "the file is writable without the list"
    );
    let refused = append(".env", &listed);
    let neighbour = append("other.txt", &listed);

    assert!(!refused.ended_well, "{refused:?}");
    assert!(neighbour.ended_well, "{neighbour:?}");
}

/// The regression it rejects: a refusal judged by the spelling of the path rather than by where it
/// leads. The refusal is written through a link, and the file is read by its own name.
#[cfg(unix)]
#[test]
fn a_denied_read_written_through_a_link_holds_where_the_link_leads() {
    if !can_confine() {
        return;
    }
    let places = Places::new("filesystem-deny-link");
    let link = places.session.join("shortcut");
    std::os::unix::fs::symlink(&places.beside, &link).expect("a link");
    let listed = places
        .confinement()
        .with_filesystem(&written(&[], &["shortcut"], &[], &[]));
    let by_name = format!("cat {}", places.beside.join("outside.txt").display());

    assert!(places.run(&by_name, None).ended_well, "the file is there");
    let ran = places.run(&by_name, Some(&listed));
    assert!(!ran.ended_well, "a link evaded the refusal: {ran:?}");
}

/// The regression it rejects: a refusal that cannot be applied being dropped, so a stage starts and
/// reaches the path the person meant to hold back. It is not started, and the refusal names the key
/// and the entry.
#[cfg(unix)]
#[test]
fn a_denial_that_cannot_be_applied_stops_the_stage() {
    if !can_confine() {
        return;
    }
    let places = Places::new("filesystem-unapplied");
    let listed = places
        .confinement()
        .with_filesystem(&written(&[], &["../escape"], &[], &[]));

    let refused = exec::run_plan_observed(
        &places.plan("cat inside.txt"),
        &Cancel::new(),
        exec::LIMIT,
        None,
        None,
        Some(&listed),
        &mut |_| Ok(()),
    );

    assert!(
        matches!(&refused, Err(exec::ExecError::NotConfined { detail, .. })
            if detail.contains("denyRead") && detail.contains("../escape")),
        "{refused:?}"
    );
}

fn hosts(allowed: Option<&[&str]>, denied: &[&str]) -> bravebot_config::sandbox_network::Hosts {
    use bravebot_config::sandbox_network::{HostEntry, Hosts};
    let entry = |name: &&str| HostEntry {
        entry: name.to_string(),
        by: None,
    };
    Hosts {
        allowed: allowed.map(|names| names.iter().map(entry).collect()),
        denied: denied.iter().map(entry).collect(),
        on_unlisted: None,
    }
}

/// A line that prints `HTTPS_PROXY`, behind `assignment` when it is not empty. Windows runs
/// `cmd.exe`, a native program: Git's `printenv` is an MSYS program, which a container cannot start,
/// so the variable would be read as empty. A variable that is not set prints as `%HTTPS_PROXY%`,
/// which is the same for a control and a confined run.
fn print_proxy(assignment: &str) -> String {
    let program = if cfg!(windows) {
        "cmd /c echo %HTTPS_PROXY%"
    } else {
        "printenv HTTPS_PROXY"
    };
    if assignment.is_empty() {
        program.to_string()
    } else {
        format!("{assignment} {program}")
    }
}

/// What a confined `printenv HTTPS_PROXY` printed.
fn proxy_variable(places: &Places, line: &str, confinement: Option<&Confinement>) -> String {
    places.run(line, confinement).stdout.trim().to_string()
}

/// What `CONNECT host:443` through the proxy at `address` was answered with, as a first line.
fn connect_through(address: &str, host: &str) -> String {
    use std::io::{BufRead, BufReader, Write};
    let mut stream = std::net::TcpStream::connect(address).expect("the proxy listens");
    write!(
        stream,
        "CONNECT {host}:443 HTTP/1.1\r\nHost: {host}:443\r\n\r\n"
    )
    .expect("a request");
    let mut first = String::new();
    BufReader::new(stream)
        .read_line(&mut first)
        .expect("an answer");
    first.trim().to_string()
}

/// The regression it rejects: a host list that is read and never applied, so a stage with egress
/// keeps the open network, or one whose variables name a port that decides with another list. The
/// port the stage is told refuses the unlisted name and the listed one is not refused by it.
#[test]
fn a_stage_with_a_host_list_is_pointed_at_the_proxy_that_applies_it() {
    if !can_confine() {
        return;
    }
    let places = Places::new("hosts-proxy");
    let list = hosts(Some(&["mine.example"]), &[]);
    let listed = places.confinement().with_hosts(Some(&list));

    let told = proxy_variable(&places, &print_proxy(""), Some(&listed));
    let address = told.strip_prefix("http://").expect("a proxy url");
    assert!(address.starts_with("127.0.0.1:"), "{told}");
    assert!(connect_through(address, "evil.example").contains("403"));
    assert!(!connect_through(address, "mine.example").contains("403"));
}

/// The regression it rejects: the proxy variables applied before the stage's own assignment, so a
/// model-written `HTTPS_PROXY=` points the stage at a proxy of its choosing.
#[test]
fn an_assignment_in_the_line_cannot_point_a_stage_elsewhere() {
    if !can_confine() {
        return;
    }
    let places = Places::new("hosts-assignment");
    let list = hosts(Some(&["mine.example"]), &[]);
    let listed = places.confinement().with_hosts(Some(&list));

    let plain = proxy_variable(&places, &print_proxy(""), Some(&listed));
    let assigned = proxy_variable(
        &places,
        &print_proxy("HTTPS_PROXY=http://elsewhere.example:3128"),
        Some(&listed),
    );
    assert!(plain.starts_with("http://127.0.0.1:"), "{plain}");
    assert_eq!(assigned, plain);
}

/// The regressions it rejects: a proxy started for a session that set no list, and one started for
/// a list whose only key is a denial, which takes reach away and gives none. Each stage is
/// compared with the unconfined control, so a proxy variable the machine already has is not read
/// as ours.
#[test]
fn no_allowed_list_means_no_proxy() {
    if !can_confine() {
        return;
    }
    let places = Places::new("hosts-absent");
    let control = proxy_variable(&places, &print_proxy(""), None);
    for list in [None, Some(hosts(None, &["bad.example"]))] {
        let confinement = places.confinement().with_hosts(list.as_ref());
        assert_eq!(
            proxy_variable(&places, &print_proxy(""), Some(&confinement)),
            control
        );
    }
}

/// The regression it rejects: a proxy handed to a stage that has no egress, which gives the stage
/// a route the closed setting took away. It needs a platform that enforces the closed setting.
#[test]
fn a_stage_with_no_egress_is_given_no_proxy() {
    if !can_close_the_network() {
        return;
    }
    let places = Places::new("hosts-closed");
    let control = proxy_variable(&places, &print_proxy(""), None);
    let list = hosts(Some(&["mine.example"]), &[]);
    let closed = places
        .confinement()
        .with_network(Network::Closed)
        .with_hosts(Some(&list));
    assert_eq!(
        proxy_variable(&places, &print_proxy(""), Some(&closed)),
        control
    );
}

/// The regression it rejects: a `denyRead` or `denyWrite` entry under a session directory being
/// dropped on a platform that cannot subtract from a grant, so the stage starts and the program
/// reads and writes the file the person denied. SANDBOX-25 says such a stage is not started.
#[cfg(windows)]
#[test]
#[ignore = "exposes #1898: a deny entry is resolved with the verbatim prefix and the grants are not, so no entry is judged to be under a grant"]
fn a_denied_entry_under_the_session_stops_the_stage() {
    if !can_confine() {
        return;
    }
    let places = Places::new("filesystem-deny-windows");
    std::fs::write(places.session.join("secret.env"), "VALUE=1\n").expect("a file");
    let listed =
        places
            .confinement()
            .with_filesystem(&written(&[], &["secret.env"], &[], &["secret.env"]));

    let refused = exec::run_plan_observed(
        &places.plan("cat secret.env"),
        &Cancel::new(),
        exec::LIMIT,
        None,
        None,
        Some(&listed),
        &mut |_| Ok(()),
    );

    assert!(
        matches!(&refused, Err(exec::ExecError::NotConfined { .. })),
        "the stage started: {refused:?}"
    );
}

/// The regression it rejects: the credential table skipped on Windows, so a session opened on the
/// home directory grants the directories that hold a key. The home directory's own files are the
/// control that the session is granted at all.
#[cfg(windows)]
#[test]
#[ignore = "exposes #1899: the Windows run base names no credential location, so a session grant over the home directory covers them"]
fn a_session_opened_on_the_home_directory_is_refused_the_credential_locations() {
    if !can_confine() {
        return;
    }
    let places = Places::new("home-session");
    let confinement = Confinement::here(vec![places.home.clone()], None, Some(&places.home))
        .expect("a platform with a base");
    let read = |row: &str| {
        let line = cat(places.home.join(row).display());
        let plan = bravebot_agent::cmdline::compile(&line, &places.home, None, &mut |_, _| Ok(()))
            .unwrap_or_else(|error| panic!("`{line}` should compile: {error}"));
        exec::run_plan_observed(
            &plan,
            &Cancel::new(),
            exec::LIMIT,
            None,
            None,
            Some(&confinement),
            &mut |_| Ok(()),
        )
        .unwrap_or_else(|error| panic!("`{line}` should start: {error}"))
    };

    assert!(
        read(".gitconfig").ended_well,
        "the home directory is granted"
    );
    for row in [
        ".bravebot\\gateway-keys.json",
        ".ssh\\id_ed25519",
        ".aws\\credentials",
    ] {
        let ran = read(row);
        assert!(
            !ran.ended_well && !ran.stdout.contains("secret"),
            "{row} was read: {ran:?}"
        );
    }
}
