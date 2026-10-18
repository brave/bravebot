//! The supervisor: the one process per account that starts a background session's process again
//! when it ends between turns (BG-11, BG-12).
//!
//! It decides from facts about processes and from the roster's own fields, and from nothing else
//! (BG-3). Whether a process is alive is the lock it holds ([`Roster::live_ids`]), what it was
//! doing is the state it last wrote, and whether a person ended it is the mark a stop leaves
//! ([`Roster::is_being_stopped`]). It reads no record, no prompt, no reply and no tool result,
//! and makes no model request.
//!
//! It is not the parent of the process a terminal starts. That process reads its configuration
//! from the environment of the terminal that ran `--bg`, and a parent that started it would give
//! it the environment of whichever terminal started the supervisor. What the supervisor watches is
//! every process it finds alive, whoever started it, so one that outlives a supervisor is picked
//! up by the next.

use bravebot_session::jobs::{Facts, Lease, Mode, Roster, State};
use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, ExitCode, Stdio};
use std::time::{Duration, Instant};

/// How often the roster is looked at.
const TICK: Duration = Duration::from_millis(500);

/// How many times a process is started again within [`WINDOW`] before the supervisor stops trying
/// (BG-12).
const RESTARTS: usize = 3;

/// The hour [`RESTARTS`] is counted over.
const WINDOW: Duration = Duration::from_secs(60 * 60);

/// How long a process that was started again has to show that it is alive before it counts as
/// having ended.
const STARTING: Duration = Duration::from_secs(15);

/// Every this many ticks the whole roster is looked at for a process not yet watched. In between,
/// only the processes already watched are looked at, so the cost of a tick does not grow with the
/// sessions that have ended and are listed still.
const SCAN_EVERY: u32 = 4;

/// How many times the lock is tried for, and how long between tries, before the supervisor takes
/// it that another one holds it. A program asking whether the lock is held takes it for an instant
/// to find out, and a claim made in that instant would end this supervisor for nothing.
const CLAIMS: usize = 5;
const BETWEEN_CLAIMS: Duration = Duration::from_millis(30);

/// What is done about a process that was alive and is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// It ended between turns and nobody asked it to: start it again.
    Restart,
    /// It ended on purpose, ended itself, or ended in the middle of a turn. The entry already
    /// reads as stopped or interrupted (BG-6) and nothing is run again (BG-12).
    Leave,
}

/// The times a process was started again within the last [`WINDOW`].
#[derive(Debug, Default)]
pub(crate) struct Restarts(Vec<Instant>);

impl Restarts {
    /// Whether the process may be started once more at `now`, counting it if so.
    fn allow(&mut self, now: Instant) -> bool {
        self.0
            .retain(|at| now.saturating_duration_since(*at) < WINDOW);
        if self.0.len() >= RESTARTS {
            return false;
        }
        self.0.push(now);
        true
    }
}

/// What to do about a process that ended, given the state it last wrote, whether a person was
/// stopping it, and how often it has been started again (BG-12).
///
/// Only a process that last said it was idle is started again: it was waiting for a prompt, so no
/// turn is repeated by running it. One that last said it was working or holding a question ended
/// in the middle of something, and one that said it was stopped ended on purpose.
pub(crate) fn outcome(
    said: State,
    stopped_by_a_person: bool,
    restarts: &mut Restarts,
    now: Instant,
) -> Outcome {
    if said != State::Idle || stopped_by_a_person {
        return Outcome::Leave;
    }
    match restarts.allow(now) {
        true => Outcome::Restart,
        false => Outcome::Leave,
    }
}

struct Watched {
    restarts: Restarts,
    /// Set while a process that was started again has until then to be alive.
    starting_until: Option<Instant>,
}

/// The processes found alive, and what is known about each.
pub(crate) struct Supervisor {
    roster: Roster,
    watched: HashMap<String, Watched>,
    ticks: u32,
}

impl Supervisor {
    pub(crate) fn new(roster: Roster) -> Self {
        Self {
            roster,
            watched: HashMap::new(),
            ticks: 0,
        }
    }

    /// Whether no process is being watched, which is when the supervisor ends (BG-11).
    pub(crate) fn is_done(&self) -> bool {
        self.watched.is_empty()
    }

    /// Look at the roster once at `now`: watch every process found alive, and for each watched one
    /// that is not alive, either start it again with `start` or let it go (BG-12).
    pub(crate) fn tick(
        &mut self,
        now: Instant,
        start: &mut dyn FnMut(&str, &Facts) -> std::io::Result<()>,
    ) {
        if self.ticks.is_multiple_of(SCAN_EVERY) {
            for id in self.roster.live_ids() {
                self.watched.entry(id).or_insert_with(|| Watched {
                    restarts: Restarts::default(),
                    starting_until: None,
                });
            }
        }
        self.ticks = self.ticks.wrapping_add(1);
        let ids: Vec<String> = self.watched.keys().cloned().collect();
        for id in ids {
            let Some(watched) = self.watched.get_mut(&id) else {
                continue;
            };
            // The mark is read before the entry. A stop makes the mark before it signals the
            // process and removes it only after the entry reads as stopped, so an end that was a
            // person's is seen with the mark or, once the mark is gone, with the entry already
            // saying stopped.
            let stopped_by_a_person = self.roster.is_being_stopped(&id);
            let Some(facts) = self.roster.facts(&id) else {
                self.watched.remove(&id);
                continue;
            };
            if facts.live {
                watched.starting_until = None;
                continue;
            }
            if watched.starting_until.is_some_and(|until| now < until) {
                continue;
            }
            let restart = outcome(facts.said, stopped_by_a_person, &mut watched.restarts, now)
                == Outcome::Restart
                && start(&id, &facts).is_ok();
            if restart {
                watched.starting_until = Some(now + STARTING);
            } else {
                self.watched.remove(&id);
            }
        }
    }
}

/// The command that starts the process for `id` again, idle, in the directory it worked in and in
/// the mode every session opens in (BG-8, BG-12).
fn restart_command(program: &Path, id: &str, facts: &Facts) -> std::process::Command {
    let mut command =
        crate::background::host_command(program, id, Some(Path::new(&facts.directory)), Mode::Ask);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Start the process for `id` again.
///
/// The line it starts with is empty, so the process opens idle: the supervisor starts no turn, and
/// it leaves the line itself because a process begins only if one was left for it (BG-2). Nothing
/// of the session is read to do it.
///
/// A line already left is a terminal starting the session at this moment, with a reply or for an
/// attach. That line is the person's and is not replaced; the process the terminal is starting is
/// the one that comes up.
fn restart(
    roster: &Roster,
    program: &Path,
    id: &str,
    facts: &Facts,
    children: &mut Vec<Child>,
) -> std::io::Result<()> {
    match roster.leave_first_prompt(id, "") {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(err) => return Err(err),
    }
    // The socket of the process that ended, which would otherwise be taken for the new one's.
    if let Some(socket) = roster.socket_of(id) {
        let _ = std::fs::remove_file(socket);
    }
    match restart_command(program, id, facts).spawn() {
        Ok(child) => {
            children.push(child);
            Ok(())
        }
        Err(err) => {
            let _ = roster.take_first_prompt(id);
            Err(err)
        }
    }
}

/// The supervisor's lock, or `None` where another supervisor holds it.
fn claim(roster: &Roster) -> Option<Lease> {
    claim_within(roster, CLAIMS, || std::thread::sleep(BETWEEN_CLAIMS))
}

/// The lock, tried for `attempts` times with `between` run before each try after the first.
fn claim_within(roster: &Roster, attempts: usize, mut between: impl FnMut()) -> Option<Lease> {
    for attempt in 0..attempts {
        if attempt > 0 {
            between();
        }
        match roster.claim_supervisor() {
            Ok(Some(lease)) => return Some(lease),
            Ok(None) => {}
            Err(_) => return None,
        }
    }
    None
}

/// The hidden command `__bg-supervisor`: watch the sessions' processes until there are none.
///
/// Ends at once where a supervisor is running already, and when nothing is alive to watch.
pub(crate) fn supervise() -> ExitCode {
    let Some(roster) = Roster::writable() else {
        return ExitCode::FAILURE;
    };
    let Some(mut lease) = claim(&roster) else {
        return ExitCode::SUCCESS;
    };
    // Its own session, so closing the terminal that started it does not end it.
    bravebot_session::jobs::leave_the_terminal();
    // The supervisor is this same program started again; nothing is trusted from where it says it is.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let Ok(program) = std::env::current_exe() else {
        return ExitCode::FAILURE;
    };
    let mut supervisor = Supervisor::new(roster.clone());
    let mut children: Vec<Child> = Vec::new();
    loop {
        supervisor.tick(Instant::now(), &mut |id, facts| {
            restart(&roster, &program, id, facts, &mut children)
        });
        // Collected, so a process that ended is not kept as a zombie.
        children.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
        if supervisor.is_done() {
            // Let go of the lock, then look once more: a terminal that found it held did not start
            // a supervisor of its own, and the process it started may have become alive since the
            // last look.
            drop(lease);
            if roster.live_ids().is_empty() {
                return ExitCode::SUCCESS;
            }
            match claim(&roster) {
                Some(again) => lease = again,
                None => return ExitCode::SUCCESS,
            }
            continue;
        }
        std::thread::sleep(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_session::jobs::{Held, Job};

    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "22222222-2222-4222-8222-222222222222";

    fn a_roster() -> Roster {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Roster::at(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-scratch")
                .join(format!(
                    "supervisor-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ))
                .join("jobs"),
        )
    }

    fn an_entry(id: &str, state: State, held: Option<Held>) -> Job {
        let mut job = Job::starting(
            id.to_string(),
            Path::new("/work"),
            "fix the build",
            Mode::Ask,
        );
        job.is(state, held);
        job
    }

    /// Run one tick, and give back the ids it started again.
    fn ticked(supervisor: &mut Supervisor, now: Instant) -> Vec<String> {
        let mut started = Vec::new();
        supervisor.tick(now, &mut |id, _| {
            started.push(id.to_string());
            Ok(())
        });
        started
    }

    /// BG-12: a process that ends while idle is started again, and one that ends while idle but
    /// that nobody has watched alive is not, since a supervisor that started again every idle entry
    /// it found would start the sessions a reboot ended.
    #[test]
    fn a_process_that_ends_idle_is_started_again_and_one_never_seen_alive_is_not() {
        let roster = a_roster();
        roster
            .publish(&an_entry(A, State::Idle, None))
            .expect("published");
        roster
            .publish(&an_entry(B, State::Idle, None))
            .expect("published");
        let lease = roster.claim(A).expect("claim").expect("free");
        let mut supervisor = Supervisor::new(roster.clone());
        let now = Instant::now();

        assert!(ticked(&mut supervisor, now).is_empty());
        assert!(!supervisor.is_done());
        drop(lease);
        assert_eq!(ticked(&mut supervisor, now), [A]);
    }

    /// BG-12: a process that ends in a turn, or holding a question, is not started again, because
    /// that would run the turn's writes and commands twice. Nor is one that said it stopped.
    #[test]
    fn a_process_that_ends_in_a_turn_or_a_question_is_not_started_again() {
        for (state, held) in [
            (State::Working, None),
            (State::NeedsInput, Some(Held::Run)),
            (State::Interrupted, None),
            (State::Stopped, None),
        ] {
            let roster = a_roster();
            roster
                .publish(&an_entry(A, state, held))
                .expect("published");
            let lease = roster.claim(A).expect("claim").expect("free");
            let mut supervisor = Supervisor::new(roster.clone());
            let now = Instant::now();
            assert!(ticked(&mut supervisor, now).is_empty());
            drop(lease);
            assert!(ticked(&mut supervisor, now).is_empty(), "{state:?}");
            assert!(supervisor.is_done(), "{state:?}");
        }
    }

    /// BG-12: at most three starts in an hour, and one more once the hour has gone by.
    #[test]
    fn a_process_is_started_again_three_times_in_an_hour() {
        let mut restarts = Restarts::default();
        let start = Instant::now();
        for minute in 0..3 {
            let at = start + Duration::from_secs(60 * minute);
            assert!(restarts.allow(at), "start {minute}");
        }
        assert!(!restarts.allow(start + Duration::from_secs(60 * 3)));
        assert!(!restarts.allow(start + WINDOW - Duration::from_secs(1)));
        assert!(restarts.allow(start + WINDOW));
    }

    /// BG-12: the limit is on the process, not on the supervisor's ticks. A process that keeps
    /// ending idle is started three times and then left, and the supervisor is done with it.
    #[test]
    fn a_process_that_keeps_ending_is_given_up_on_after_three_starts() {
        let roster = a_roster();
        roster
            .publish(&an_entry(A, State::Idle, None))
            .expect("published");
        let mut supervisor = Supervisor::new(roster.clone());
        let mut now = Instant::now();
        let mut lease = Some(roster.claim(A).expect("claim").expect("free"));
        assert!(ticked(&mut supervisor, now).is_empty());

        let mut started = 0;
        for _ in 0..6 {
            drop(lease.take());
            now += STARTING + Duration::from_secs(1);
            if ticked(&mut supervisor, now).len() == 1 {
                started += 1;
                lease = roster.claim(A).expect("claim");
                assert!(ticked(&mut supervisor, now).is_empty());
            } else {
                break;
            }
        }
        assert_eq!(started, RESTARTS);
        assert!(supervisor.is_done());
    }

    /// BG-12: a process that was started again has time to become alive. Looked at before then it
    /// is not started a second time, and once the time is up without it being alive it counts as
    /// having ended again.
    #[test]
    fn a_process_started_again_is_given_time_to_come_up() {
        let roster = a_roster();
        roster
            .publish(&an_entry(A, State::Idle, None))
            .expect("published");
        let lease = roster.claim(A).expect("claim").expect("free");
        let mut supervisor = Supervisor::new(roster.clone());
        let now = Instant::now();
        ticked(&mut supervisor, now);
        drop(lease);
        assert_eq!(ticked(&mut supervisor, now), [A]);

        assert!(ticked(&mut supervisor, now + STARTING / 2).is_empty());
        assert!(!supervisor.is_done());
        assert_eq!(ticked(&mut supervisor, now + STARTING + TICK), [A]);
    }

    /// BG-12: a process a person stopped is not started again, whether the stop is still running
    /// (the mark is there and the entry still says idle) or has finished (the entry says stopped).
    #[test]
    fn a_process_a_person_stopped_is_not_started_again() {
        let roster = a_roster();
        roster
            .publish(&an_entry(A, State::Idle, None))
            .expect("published");
        let lease = roster.claim(A).expect("claim").expect("free");
        let mut supervisor = Supervisor::new(roster.clone());
        let now = Instant::now();
        ticked(&mut supervisor, now);
        drop(lease);

        std::fs::write(
            roster
                .socket_of(A)
                .expect("a directory")
                .with_file_name("stopping"),
            "",
        )
        .expect("the mark a stop leaves");
        assert!(roster.is_being_stopped(A));
        assert!(ticked(&mut supervisor, now).is_empty());
        assert!(supervisor.is_done());
    }

    /// BG-12, BG-8: a process is started again as the hidden host in the directory it worked in, in
    /// the mode every session opens in, whatever it ran in before, with its output going nowhere.
    #[test]
    fn a_process_is_started_again_in_its_directory_in_the_mode_every_session_opens_in() {
        let mut accepting = an_entry(A, State::Idle, None);
        accepting.mode = Mode::AcceptEdits;
        let roster = a_roster();
        roster.publish(&accepting).expect("published");
        let facts = roster.facts(A).expect("facts");

        let command = restart_command(Path::new("/bin/bravebot"), A, &facts);
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(arguments, ["__bg-host", A, "ask"]);
        assert_eq!(command.get_current_dir(), Some(Path::new("/work")));
    }

    /// BG-11: the supervisor ends once nothing it watched is alive, and not before.
    #[test]
    fn the_supervisor_is_done_when_nothing_is_alive() {
        let roster = a_roster();
        let mut supervisor = Supervisor::new(roster.clone());
        assert!(supervisor.is_done());
        roster
            .publish(&an_entry(A, State::Stopped, None))
            .expect("published");
        let lease = roster.claim(A).expect("claim").expect("free");
        ticked(&mut supervisor, Instant::now());
        assert!(!supervisor.is_done());
        drop(lease);
        ticked(&mut supervisor, Instant::now());
        assert!(supervisor.is_done());
    }

    /// BG-11: a claim made while a program is looking at whether the lock is held still gets it. The
    /// look takes the lock for an instant, and a supervisor that gave up on that would end with no
    /// other supervisor running. The look ends between the first try and the second, where the
    /// supervisor waits.
    #[test]
    fn a_claim_made_while_the_lock_is_looked_at_is_tried_again() {
        let roster = a_roster();
        drop(roster.claim_supervisor().expect("claim").expect("free"));
        let path = roster
            .socket_of(A)
            .and_then(|socket| Some(socket.parent()?.parent()?.join("supervisor.lock")))
            .expect("the roster's directory");
        let looking = std::fs::File::open(&path).expect("the lock file");
        looking.lock_shared().expect("a look at the lock");
        let mut looking = Some(looking);

        let claimed = claim_within(&roster, CLAIMS, || drop(looking.take()));
        assert!(claimed.is_some(), "the supervisor gave up on a look");
        assert!(looking.is_none(), "the claim was not tried again");
    }

    /// BG-11: a claim made while another supervisor runs is still refused, after every try.
    #[test]
    fn a_claim_made_while_another_supervisor_runs_is_refused() {
        let roster = a_roster();
        let _first = roster.claim_supervisor().expect("claim").expect("free");
        let mut waited = 0;
        assert!(claim_within(&roster, CLAIMS, || waited += 1).is_none());
        assert_eq!(waited, CLAIMS - 1);
    }

    /// BG-12: a line a terminal has left for the process it is starting is not replaced by the
    /// supervisor's empty one, and no process is started beside the terminal's.
    #[test]
    fn a_restart_does_not_replace_the_line_a_terminal_is_starting_the_process_with() {
        let roster = a_roster();
        roster
            .publish(&an_entry(A, State::Idle, None))
            .expect("published");
        roster
            .leave_first_prompt(A, "and the tests")
            .expect("a terminal's line");
        // A directory that exists, so that a process started beside the terminal's would start.
        let facts = Facts {
            directory: "/".to_string(),
            ..roster.facts(A).expect("facts")
        };
        let mut children = Vec::new();

        restart(&roster, Path::new("/bin/sh"), A, &facts, &mut children).expect("left alone");

        assert_eq!(
            roster.take_first_prompt(A).as_deref(),
            Some("and the tests")
        );
        assert!(
            children.is_empty(),
            "a process was started beside the terminal's"
        );
    }

    /// BG-11: the roster is looked at in full every few ticks and only for the processes already
    /// watched in between, so a process that comes alive is watched within a few ticks and one that
    /// ends is noticed on the next.
    #[test]
    fn a_process_that_comes_alive_is_watched_within_a_few_ticks() {
        let roster = a_roster();
        roster
            .publish(&an_entry(A, State::Idle, None))
            .expect("published");
        let mut supervisor = Supervisor::new(roster.clone());
        let now = Instant::now();
        assert!(ticked(&mut supervisor, now).is_empty());
        assert!(supervisor.is_done());

        let lease = roster.claim(A).expect("claim").expect("free");
        let mut ticks = 0;
        while supervisor.is_done() {
            assert!(ticks < SCAN_EVERY, "never watched");
            ticked(&mut supervisor, now);
            ticks += 1;
        }
        drop(lease);
        assert_eq!(ticked(&mut supervisor, now), [A]);
    }

    /// BG-11: one supervisor per account. The second finds the lock held, and the lock is released
    /// with the first.
    #[test]
    fn a_second_supervisor_finds_the_first() {
        let roster = a_roster();
        assert!(!roster.has_supervisor());
        let first = roster.claim_supervisor().expect("claim").expect("free");
        assert!(roster.has_supervisor());
        assert!(roster.claim_supervisor().expect("claim").is_none());
        drop(first);
        assert!(!roster.has_supervisor());
        assert!(roster.claim_supervisor().expect("claim").is_some());
    }
}
