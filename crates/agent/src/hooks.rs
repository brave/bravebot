//! Running the commands a person attached to a moment.
//!
//! What a hook is and where it is declared belongs to [`bravebot_config::hooks`]. This is what
//! happens when one fires: a process starts, it is given a short line on standard input saying
//! which moment this is, and it is waited for. Nothing it prints is read and nothing it returns is
//! consulted.
//!
//! # A hook decides nothing
//!
//! The exit status is recorded so the person can be told their hook is failing, and it is the only
//! use it is put to. No branch anywhere takes a hook's status, its output, or the fact that it ran
//! at all as a reason to do or not do something else.
//!
//! That is not caution about a person's own program. A hook runs beside a turn that is reading
//! files, so anything it says is a function of bytes nobody vouched for: a hook that grepped a file
//! and exited non-zero would be a file deciding whether the next tool call happens. Refusing an
//! effect is not the safe direction of that, either. A file that could stop exactly the writes it
//! did not want would be choosing what the agent does, which is the one thing this repository is
//! built to prevent.
//!
//! # Not confined
//!
//! Like a program `run` starts, and for the same reason: the person named this command in a file in
//! their own directory, so it gets the access their own shell would give it. `bravebot-sandbox`
//! confines processes running code we did not write, and this is code they wrote.
//!
//! # Nothing from the environment that a run would not get
//!
//! The environment is inherited less the names [`crate::scrub`] takes off, so this agent's own
//! credentials do not travel into a program the person attached to a moment any more than they
//! travel into one the planner asked for.

use bravebot_config::hooks::{Hook, Hooks, Moment};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long one hook may run before it is stopped.
///
/// Short, because every hook holds the turn open while it runs and a person watching a session has
/// no way to tell a slow hook from a slow model. A formatter or a notification finishes well inside
/// this; anything that does not wants to be a background job of its own rather than something a
/// turn waits on.
pub const LIMIT: Duration = Duration::from_secs(30);

/// How often the wait looks up to see whether the process is done.
const TICK: Duration = Duration::from_millis(50);

/// What became of one hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fired {
    /// The program, as the file named it. Its arguments are left out: this exists to be said back
    /// to whoever wrote the file, and the first word is what they will recognise.
    pub program: String,
    /// The moment that fired it, in the word the file spells it with.
    pub moment: &'static str,
    /// What went wrong, where something did. `None` is a hook that ran and ended well, which is
    /// nothing anybody needs to be told.
    pub trouble: Option<Trouble>,
}

/// The ways a hook can fail to be a hook that ran and ended well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trouble {
    /// The program could not be started, with what the operating system said about it: it is not
    /// there, or it is not executable. The reason is carried because the two are one sentence to a
    /// person and two different things to fix.
    NotStarted(String),
    /// It ran and ended badly, with the platform's own word for how. A status rather than a
    /// number because a process killed by a signal has no number, and inventing one for it would
    /// report a hook that was killed as a hook that chose to fail.
    Ended(String),
    /// It was still running when the bound ran out, and was stopped.
    Stopped,
}

/// Run every hook attached to `moment`, in the order the file declared them, and say what became
/// of each.
///
/// `tool` is the name of the call that just finished, where the moment is about one. It selects
/// which entries fire and reaches no process: what a hook is told is the moment and nothing else.
///
/// One at a time rather than all at once. Two hooks on one moment are two things a person asked
/// for in an order they wrote down, and a formatter racing a commit would be this deciding that
/// the order did not matter.
pub fn fire(hooks: &Hooks, moment: Moment, tool: Option<&str>, directory: &Path) -> Vec<Fired> {
    fire_within(hooks, moment, tool, directory, LIMIT)
}

/// [`fire`], for a stated bound rather than the one a turn uses.
///
/// So that a test of what happens to a hook that outstays its welcome does not have to wait the
/// real bound out. Nothing else states one: how long a turn will wait on a hook is this module's
/// decision and not a caller's.
pub fn fire_within(
    hooks: &Hooks,
    moment: Moment,
    tool: Option<&str>,
    directory: &Path,
    limit: Duration,
) -> Vec<Fired> {
    if hooks.is_empty() {
        return Vec::new();
    }
    let said = announcement(moment);
    hooks
        .firing(moment, tool)
        .map(|hook| Fired {
            program: hook.run().first().cloned().unwrap_or_default(),
            moment: moment.as_str(),
            trouble: run(hook, &said, directory, limit),
        })
        .collect()
}

/// The whole of what a hook is told, as one line of JSON.
///
/// Built from the moment's own word, which is a literal of this crate's. Nothing a model wrote and
/// nothing read out of a file is in it, so there is no question of what a hook is handed: the same
/// three lines, forever, whatever the turn was doing.
///
/// An object rather than the bare word so the line has somewhere to grow a second key without every
/// hook that reads the first having to be taught that the shape changed.
fn announcement(moment: Moment) -> String {
    format!("{{\"event\":\"{}\"}}\n", moment.as_str())
}

/// Start one hook, hand it the line, and wait for it.
fn run(hook: &Hook, said: &str, directory: &Path, limit: Duration) -> Option<Trouble> {
    let (program, arguments) = hook.run().split_first()?;
    let mut command = Command::new(program);
    // The vector, never a string. There is no shell between the file and the process, so an
    // argument holding a space or a semicolon is one argument and arrives as one.
    command.args(arguments).current_dir(directory);
    command
        .stdin(Stdio::piped())
        // Discarded rather than inherited or collected. Inherited, a hook would draw over the
        // interface; collected, something here would be holding output nothing is allowed to read.
        // A hook with something to say says it the way any other program on the machine does.
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::scrub::apply(&mut command);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(problem) => return Some(Trouble::NotStarted(problem.to_string())),
    };

    // Written and then dropped, so a hook reading to end-of-file gets one. The line is a few dozen
    // bytes, which no pipe buffer has ever been too small for, and a hook that exits without
    // reading it closes the pipe: that is a write that fails, and a hook which did not want to be
    // told is not a hook that went wrong.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(said.as_bytes());
    }

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return None,
            Ok(Some(status)) => return Some(Trouble::Ended(status.to_string())),
            // A child we cannot ask about is not one to keep waiting for.
            Err(problem) => return Some(Trouble::Ended(problem.to_string())),
            Ok(None) => {}
        }
        if started.elapsed() >= limit {
            let _ = child.kill();
            // Reaped rather than left. The turn goes on for as long as the session does, and a
            // hook firing every round would otherwise leave one zombie per round behind it.
            let _ = child.wait();
            return Some(Trouble::Stopped);
        }
        std::thread::sleep(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HOOK-5: the line is built from the moment's own word and holds nothing else.
    #[test]
    fn the_line_a_hook_is_told_is_built_from_the_moment_alone() {
        assert_eq!(
            announcement(Moment::TurnStarted),
            "{\"event\":\"turn-started\"}\n"
        );
        assert_eq!(
            announcement(Moment::ToolFinished),
            "{\"event\":\"tool-finished\"}\n"
        );
        assert_eq!(
            announcement(Moment::TurnFinished),
            "{\"event\":\"turn-finished\"}\n"
        );
    }

    /// HOOK-7: a turn with no hooks declared starts no process and looks at no moment.
    #[test]
    fn a_file_that_declared_nothing_fires_nothing() {
        let fired = fire(&Hooks::default(), Moment::TurnStarted, None, Path::new("."));
        assert!(fired.is_empty());
    }
}
