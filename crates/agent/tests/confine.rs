//! `run` under the kernel: a program a person asked for writes only the directories the session was
//! opened on, in the foreground and left running, and on Linux and macOS reads the machine except
//! the places that hold a credential.
//!
//! Each test skips where this machine cannot apply a profile, which includes a suite run from
//! inside another profile.

use bravebot_agent::confine::Confinement;
use bravebot_agent::exec;
use bravebot_core::cancel::Cancel;
use bravebot_core::command::Plan;
use std::path::{Path, PathBuf};

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
            session: session.canonicalize().expect("canonical session"),
            beside: beside.canonicalize().expect("canonical beside"),
            home: home.canonicalize().expect("canonical home"),
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

fn can_confine() -> bool {
    bravebot_sandbox::base::Prelude::current().is_some()
        && bravebot_sandbox::confinement_works_here()
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
    let line = format!("cat {}", places.beside.join("outside.txt").display());

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

    let read = places.run("cat inside.txt", Some(&confinement));
    let wrote = places.run("touch made.txt", Some(&confinement));

    assert_eq!(read.stdout, "inside\n");
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

    let ran = places.run(
        &format!("touch {}", target.display()),
        Some(&places.confinement()),
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
    let plan = places.plan(&format!("touch {}", target.display()));
    let steps = plan.steps.unrouted_pipeline().expect("one pipeline");

    let mut job = exec::start_steps(steps, &places.session, None, Some(&places.confinement()))
        .expect("the job starts");
    for _ in 0..200 {
        if job.ended() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    assert!(job.ended(), "the job did not finish");
    assert!(!target.exists(), "the job wrote outside the session");
    assert_ne!(job.codes(), [Some(0)], "the write succeeded");
}
