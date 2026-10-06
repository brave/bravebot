//! `run` under the kernel: a program a person asked for is held to the directories the session was
//! opened on, in the foreground and left running.
//!
//! Each test skips where this machine cannot apply a profile, which includes a suite run from
//! inside another profile.

use bravebot_agent::confine::Confinement;
use bravebot_agent::exec;
use bravebot_core::cancel::Cancel;
use bravebot_core::command::Plan;
use std::path::{Path, PathBuf};

/// A session directory and a second one beside it that the session was not opened on, both under
/// the build directory, which no row of the base reaches.
struct Places {
    session: PathBuf,
    beside: PathBuf,
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
        Self {
            session: session.canonicalize().expect("canonical session"),
            beside: beside.canonicalize().expect("canonical beside"),
        }
    }

    fn confinement(&self) -> Confinement {
        Confinement::here(
            vec![self.session.clone()],
            None,
            std::env::var_os("HOME").map(PathBuf::from).as_deref(),
        )
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

/// The regression it rejects: confinement passed down and never applied, so a confined line reads
/// whatever the account can. The unconfined run of the same line is the control that the file is
/// there to be read.
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
    let plan = places.plan(&format!(
        "cat {}",
        places.beside.join("outside.txt").display()
    ));
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
    assert!(
        !job.printed().lines().any(|line| line == "outside"),
        "{}",
        job.printed()
    );
    assert_ne!(job.codes(), [Some(0)], "the read succeeded");
}
