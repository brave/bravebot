//! The open loops a session leaves, as the session holds them
//! ([OPEN](../../../../docs/specs/open-loops.md)).

use super::Session;
use bravebot_agent::open_loops::{Kind, Loops, Owner};
use bravebot_i18n::t;

impl Session {
    /// What every turn of this session did, added in order (OPEN-1).
    pub fn work(&self) -> bravebot_agent::Work {
        self.work
    }

    /// Take what a resumed session's record says its turns did, and show what that leaves open
    /// (OPEN-6). The planner is told on the first turn, since it has not been told yet.
    pub fn resume_open_loops(
        &mut self,
        work: bravebot_agent::Work,
        workspace: &bravebot_agent::Workspace,
    ) {
        self.work = work;
        self.say_open_loops(workspace);
    }

    /// The open loops now, found by OPEN-1's rules over what this session recorded.
    pub fn open_loops(&self, workspace: &bravebot_agent::Workspace) -> Loops {
        let written = self.files_landed().0;
        Loops {
            open_todos: self.plan().iter().filter(|row| !row.struck()).count(),
            running_jobs: self.running_job_names(),
            kept_checkouts: kept_checkouts(workspace).len(),
            uncommitted_files: bravebot_agent::open_loops::uncommitted(workspace.root(), &written)
                .len(),
            stopped_goal: self.goal_stopped(),
            ..Loops::from_work(self.work)
        }
    }

    /// Show the person one line where the open loops changed since the last turn ended (OPEN-3).
    pub fn say_open_loops(&mut self, workspace: &bravebot_agent::Workspace) {
        let loops = self.open_loops(workspace);
        if loops == self.loops_shown {
            return;
        }
        let line = open_line(&loops);
        self.loops_shown = loops;
        self.note(line);
    }

    /// What the planner is told at the start of the next turn, where the open loops changed since
    /// it was last told (OPEN-4). Read from the set the last turn ended with, so the turn does not
    /// ask git again.
    pub fn open_loops_for_the_planner(&mut self) -> Option<String> {
        if self.loops_shown == self.loops_told {
            return None;
        }
        self.loops_told = self.loops_shown.clone();
        self.loops_told.to_the_planner()
    }

    /// Carry out `/unfinished`: every open loop, with what to act on (OPEN-5). Starts no turn and
    /// sends nothing to the planner.
    pub fn report_unfinished(&mut self, workspace: &bravebot_agent::Workspace) {
        let loops = self.open_loops(workspace);
        if loops.is_empty() {
            self.note(t!(unfinished_nothing));
            return;
        }
        let (written, since_the_last_command) = self.files_landed();
        let mut said = Vec::new();
        if loops.unbuilt_changes {
            said.push(t!(unfinished_unbuilt).to_string());
            said.extend(
                since_the_last_command
                    .iter()
                    .map(|path| format!("  {}", path.display())),
            );
        }
        if loops.failed_command
            && let Some(last) = self.outputs().iter().rev().find(|row| row.job.is_none())
        {
            said.push(
                t!(
                    unfinished_failed,
                    command = &last.command,
                    outcome = last.outcome.summary()
                )
                .to_string(),
            );
        }
        if loops.open_todos > 0 {
            said.push(t!(unfinished_todos, count = loops.open_todos).to_string());
            let open: Vec<String> = self
                .plan()
                .iter()
                .filter(|row| !row.struck())
                .map(|row| format!("  {}", row.content))
                .collect();
            said.extend(open);
        }
        for checkout in kept_checkouts(workspace) {
            said.push(
                t!(
                    unfinished_checkout,
                    id = &checkout.id,
                    path = checkout.path.display().to_string()
                )
                .to_string(),
            );
        }
        let uncommitted = bravebot_agent::open_loops::uncommitted(workspace.root(), &written);
        if !uncommitted.is_empty() {
            said.push(t!(unfinished_uncommitted, count = uncommitted.len()).to_string());
            said.extend(
                uncommitted
                    .iter()
                    .map(|path| format!("  {}", path.display())),
            );
        }
        if let Some(goal) = self.goal().filter(|_| loops.stopped_goal) {
            said.push(match goal.is_paused() {
                true => t!(unfinished_goal_paused, condition = goal.condition()),
                false => t!(unfinished_goal_stalled, condition = goal.condition()),
            });
        }
        let running: Vec<String> = self
            .jobs()
            .filter(|(_, job)| job.is_running())
            .map(|(_, job)| {
                t!(
                    unfinished_job,
                    name = &job.name,
                    seconds = job.since.elapsed().as_secs()
                )
                .to_string()
            })
            .collect();
        said.extend(running);
        self.note(said.join("\n"));
    }

    /// Whether the goal is paused, or still set after it stopped sending the work back (OPEN-1).
    fn goal_stopped(&self) -> bool {
        self.goal()
            .is_some_and(|goal| goal.is_paused() || goal.is_stalled())
    }

    /// The files this session's finished write calls landed on, each once in the order first
    /// written, and those written since the last command that ran.
    fn files_landed(&self) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
        let mut all: Vec<std::path::PathBuf> = Vec::new();
        let mut since: Vec<std::path::PathBuf> = Vec::new();
        for activity in self
            .transcript
            .iter()
            .filter_map(|entry| entry.activity.as_ref())
        {
            if activity.failed || activity.is_running() {
                continue;
            }
            if activity.tool == "run" {
                since.clear();
            }
            if let Some(written) = &activity.written {
                for list in [&mut all, &mut since] {
                    if !list.contains(&written.landed) {
                        list.push(written.landed.clone());
                    }
                }
            }
        }
        (all, since)
    }
}

/// The checkouts kept and neither applied nor removed (OPEN-1). A removed one is no longer listed,
/// and one whose delegate still runs has no size yet.
fn kept_checkouts(
    workspace: &bravebot_agent::Workspace,
) -> Vec<bravebot_agent::workspace::SessionCheckout> {
    workspace
        .session_checkouts()
        .into_iter()
        .filter(|checkout| checkout.size.is_some() && !checkout.applied)
        .collect()
}

/// The person's line: each open kind with its count, grouped by whose move it is (OPEN-3).
fn open_line(loops: &Loops) -> String {
    if loops.is_empty() {
        return t!(open_loops_nothing).to_string();
    }
    let named = |kinds: Vec<Kind>| -> String {
        kinds
            .into_iter()
            .map(|kind| {
                let count = loops.count(kind);
                match kind {
                    Kind::UnbuiltChanges => t!(open_loops_unbuilt).to_string(),
                    Kind::FailedCommand => t!(open_loops_failed).to_string(),
                    Kind::OpenTodos => t!(open_loops_todos, count = count).to_string(),
                    Kind::KeptCheckouts => t!(open_loops_checkouts, count = count).to_string(),
                    Kind::UncommittedFiles => t!(open_loops_uncommitted, count = count).to_string(),
                    Kind::StoppedGoal => t!(open_loops_goal).to_string(),
                    Kind::RunningJobs => t!(open_loops_jobs, count = count).to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut groups = Vec::new();
    let planner = loops.of(Owner::Planner);
    if !planner.is_empty() {
        groups.push(t!(open_loops_planner, items = named(planner)).to_string());
    }
    let person = loops.of(Owner::Person);
    if !person.is_empty() {
        groups.push(t!(open_loops_person, items = named(person)).to_string());
    }
    let waiting = loops.of(Owner::Wait);
    if !waiting.is_empty() {
        groups.push(named(waiting));
    }
    t!(open_loops_line, groups = groups.join("; ")).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session and a workspace that is no repository, so nothing is uncommitted.
    fn session_in(name: &str) -> (Session, bravebot_agent::Workspace) {
        let directory =
            std::env::temp_dir().join(format!("bravebot-open-loops-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("directory");
        (
            Session::new("none"),
            bravebot_agent::Workspace::new(&directory).expect("workspace"),
        )
    }

    /// A turn that wrote a file and then ran a command that failed.
    fn wrote_then_failed() -> bravebot_agent::Work {
        let mut work = bravebot_agent::Work::default();
        work.count(true, false, false);
        work.count(false, true, true);
        work.count(true, false, false);
        work
    }

    fn notes(session: &Session) -> Vec<String> {
        session
            .transcript
            .iter()
            .map(|entry| entry.text.clone())
            .collect()
    }

    /// OPEN-3. A turn that leaves loops open ends with one line naming each kind and its count,
    /// grouped by whose move it is, and no path, command or todo text. The same set is not said
    /// again, and closing the last loop says nothing is open.
    #[test]
    fn a_turn_that_leaves_loops_open_ends_with_one_line_naming_them() {
        let (mut session, workspace) = session_in("line");
        session.worked(wrote_then_failed());
        session.say_open_loops(&workspace);
        let said = notes(&session);
        assert_eq!(
            said.last().map(String::as_str),
            Some("open: changes not built, the last command failed (planner)")
        );
        session.say_open_loops(&workspace);
        assert_eq!(
            notes(&session).len(),
            said.len(),
            "the same set was said twice"
        );
        let mut built = bravebot_agent::Work::default();
        built.count(false, true, false);
        session.worked(built);
        session.say_open_loops(&workspace);
        assert_eq!(
            notes(&session).last().map(String::as_str),
            Some("open: nothing")
        );
    }

    /// OPEN-4. The planner is told once of a set it has not been told, and not again until the
    /// set changes.
    #[test]
    fn loops_the_planner_was_already_told_about_are_not_repeated() {
        let (mut session, workspace) = session_in("planner");
        assert_eq!(session.open_loops_for_the_planner(), None);
        session.worked(wrote_then_failed());
        session.say_open_loops(&workspace);
        let told = session.open_loops_for_the_planner().expect("told");
        assert!(told.contains("the last command failed"), "{told}");
        assert_eq!(session.open_loops_for_the_planner(), None);
    }

    /// OPEN-6. A resumed session shows what its record says it left open, and the planner is told
    /// on its first turn.
    #[test]
    fn a_resumed_session_shows_what_it_left_open() {
        let (mut session, workspace) = session_in("resume");
        session.resume_open_loops(wrote_then_failed(), &workspace);
        assert_eq!(
            notes(&session).last().map(String::as_str),
            Some("open: changes not built, the last command failed (planner)")
        );
        assert!(session.open_loops_for_the_planner().is_some());
    }

    /// OPEN-5, OPEN-1. `/unfinished` lists each open loop with what to act on, a paused goal
    /// among them, and with nothing open says so. It starts no turn.
    #[test]
    fn unfinished_lists_each_open_loop_with_what_closes_it() {
        let (mut session, workspace) = session_in("unfinished");
        session.report_unfinished(&workspace);
        assert_eq!(
            notes(&session).last().map(String::as_str),
            Some(t!(unfinished_nothing).as_ref())
        );
        session.worked(wrote_then_failed());
        session.start_goal("cargo test exits 0".to_string());
        session.pause_goal();
        session.report_unfinished(&workspace);
        let said = notes(&session).last().cloned().unwrap_or_default();
        assert!(said.contains("changes not built"), "{said}");
        assert!(
            said.contains("the goal is paused: cargo test exits 0"),
            "{said}"
        );
        assert_eq!(session.turns, 0, "a turn started");
    }
}
