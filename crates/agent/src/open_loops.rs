//! The unfinished business a session leaves (OPEN).
//!
//! Every loop is found by a fixed rule over bravebot's own records: the work record, the task
//! list, the job table, the checkout list, git's status of the files the session wrote, and the
//! goal. Nothing here reads what a model said, so nothing a model says opens or closes a loop.

use crate::outcome::Work;
use crate::tools::tally;

/// Whose move an open loop is (OPEN-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// The planner can build, rerun and finish its own todos.
    Planner,
    /// Applying a checkout, committing and restarting a goal change the person's files, history
    /// or what the session works towards.
    Person,
    /// A running job needs nobody until it ends.
    Wait,
}

/// The seven kinds of open loop (OPEN-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    UnbuiltChanges,
    FailedCommand,
    OpenTodos,
    RunningJobs,
    KeptCheckouts,
    UncommittedFiles,
    StoppedGoal,
}

impl Kind {
    /// Every kind, in the order the line and the planner's message name them.
    pub const ALL: [Kind; 7] = [
        Kind::UnbuiltChanges,
        Kind::FailedCommand,
        Kind::OpenTodos,
        Kind::KeptCheckouts,
        Kind::UncommittedFiles,
        Kind::StoppedGoal,
        Kind::RunningJobs,
    ];

    /// Whose move this kind is (OPEN-2).
    pub fn owner(self) -> Owner {
        match self {
            Kind::UnbuiltChanges | Kind::FailedCommand | Kind::OpenTodos => Owner::Planner,
            Kind::KeptCheckouts | Kind::UncommittedFiles | Kind::StoppedGoal => Owner::Person,
            Kind::RunningJobs => Owner::Wait,
        }
    }
}

/// The open loops of a session at one moment, as counts (OPEN-1).
///
/// The front end gathers the facts and these rules decide what is open. Two values are equal when
/// the same loops are open with the same counts, which is how the line and the planner's message
/// know the set changed (OPEN-3, OPEN-4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loops {
    /// A file has been written since the last command ran.
    pub unbuilt_changes: bool,
    /// The last command that ran ended badly.
    pub failed_command: bool,
    /// Tasks on the list that are neither done nor cancelled.
    pub open_todos: usize,
    /// The names of the background jobs still running, as bravebot minted them (RUN-15).
    pub running_jobs: Vec<String>,
    /// Checkouts kept and neither applied nor removed.
    pub kept_checkouts: usize,
    /// Files this session wrote that differ from `HEAD`, where `commit` is `off`.
    pub uncommitted_files: usize,
    /// A goal that is paused, or still set after it stopped sending the work back.
    pub stopped_goal: bool,
}

impl Loops {
    /// Read the two loops the work record decides, beside the counts the front end gathered.
    pub fn from_work(work: Work) -> Loops {
        Loops {
            unbuilt_changes: work.writes > 0 && !work.ran_after_last_write,
            failed_command: work.commands > 0 && work.last_command_failed,
            ..Loops::default()
        }
    }

    /// How many of `kind` are open: 0 or 1 for the kinds that are one fact.
    pub fn count(&self, kind: Kind) -> usize {
        match kind {
            Kind::UnbuiltChanges => usize::from(self.unbuilt_changes),
            Kind::FailedCommand => usize::from(self.failed_command),
            Kind::OpenTodos => self.open_todos,
            Kind::RunningJobs => self.running_jobs.len(),
            Kind::KeptCheckouts => self.kept_checkouts,
            Kind::UncommittedFiles => self.uncommitted_files,
            Kind::StoppedGoal => usize::from(self.stopped_goal),
        }
    }

    /// Whether nothing is open.
    pub fn is_empty(&self) -> bool {
        Kind::ALL.iter().all(|kind| self.count(*kind) == 0)
    }

    /// The open kinds that are `owner`'s, in order.
    pub fn of(&self, owner: Owner) -> Vec<Kind> {
        Kind::ALL
            .into_iter()
            .filter(|kind| kind.owner() == owner && self.count(*kind) > 0)
            .collect()
    }

    /// What the planner is told at the start of a turn, where its loops or the person's are
    /// open (OPEN-4). Running jobs are left out: the planner is told of those already (RUN-15).
    ///
    /// Fixed English and counts. No path, no command and no todo text, so nothing a turn read or
    /// wrote speaks to the planner through it (OPEN-7).
    pub fn to_the_planner(&self) -> Option<String> {
        if self.of(Owner::Planner).is_empty() && self.of(Owner::Person).is_empty() {
            return None;
        }
        let said = |kinds: Vec<Kind>| -> Vec<String> {
            kinds
                .into_iter()
                .map(|kind| self.for_the_planner(kind))
                .collect()
        };
        let mine = said(self.of(Owner::Planner));
        let theirs = said(self.of(Owner::Person));
        let mut message = String::from("Open from earlier in this session");
        match mine.is_empty() {
            false => {
                message.push_str(&format!(
                    ": {}. Yours to act on if they still matter.",
                    mine.join("; ")
                ));
                if !theirs.is_empty() {
                    message.push_str(&format!(
                        " Also open, for the person: {}.",
                        theirs.join(", ")
                    ));
                }
            }
            true if !theirs.is_empty() => {
                message.push_str(&format!(
                    ", for the person: {}. These are theirs to act on, not yours.",
                    theirs.join(", ")
                ));
            }
            true => {}
        }
        Some(message)
    }

    /// How the planner's message names one open kind.
    fn for_the_planner(&self, kind: Kind) -> String {
        let n = self.count(kind);
        match kind {
            Kind::UnbuiltChanges => "changes not built since the last write".to_string(),
            Kind::FailedCommand => "the last command failed".to_string(),
            Kind::OpenTodos => match n {
                1 => "1 todo is open".to_string(),
                _ => format!("{n} todos are open"),
            },
            Kind::KeptCheckouts => tally(n, "kept checkout", "kept checkouts"),
            Kind::UncommittedFiles => tally(n, "file not committed", "files not committed"),
            Kind::StoppedGoal => "a goal that is paused or stopped".to_string(),
            Kind::RunningJobs => tally(n, "job running", "jobs running"),
        }
    }
}

/// Which of `written`, the files this session wrote, differ from `HEAD` in the repository holding
/// `root`, read in-process without starting git (OPEN-1).
///
/// Zero outside a repository, and where git's directory cannot be read as `read_git` reads one:
/// a status that cannot be read is not a loop.
pub fn uncommitted(
    root: &std::path::Path,
    written: &[std::path::PathBuf],
) -> Vec<std::path::PathBuf> {
    use crate::git::{Query, Repository, Request};
    if written.is_empty() {
        return Vec::new();
    }
    let Some(repository) = crate::servers::repository_holding(root) else {
        return Vec::new();
    };
    let git_dir = repository.join(".git");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let listing = crate::git::survey(&git_dir, Query::Status, deadline)
        .and_then(|_| Repository::open(&git_dir))
        .and_then(|opened| {
            opened.answer(
                &Request {
                    query: Query::Status,
                    revision: None,
                    path: None,
                    count: crate::git::DEFAULT_COUNT,
                    skip: 0,
                    messages: false,
                    pattern: None,
                    since: None,
                    until: None,
                    every_untracked: true,
                    deadline,
                },
                &|_| false,
            )
        })
        .ok()
        .and_then(|answer| answer.listing);
    let Some(listing) = listing else {
        return Vec::new();
    };
    let differs: std::collections::BTreeSet<&str> = listing
        .changed
        .iter()
        .chain(&listing.removed)
        .map(String::as_str)
        .collect();
    written
        .iter()
        .filter(|path| {
            path.strip_prefix(&repository)
                .ok()
                .and_then(|relative| relative.to_str())
                .is_some_and(|relative| differs.contains(relative))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(writes: usize, commands: usize, after: bool, failed: bool) -> Work {
        Work {
            writes,
            commands,
            ran_after_last_write: after,
            last_command_failed: failed,
        }
    }

    /// OPEN-1. A write after the last command leaves the change unbuilt, and a command after it
    /// closes the loop.
    #[test]
    fn a_write_after_the_last_command_is_unbuilt_changes() {
        assert!(Loops::from_work(work(1, 0, false, false)).unbuilt_changes);
        assert!(Loops::from_work(work(2, 1, false, false)).unbuilt_changes);
        assert!(!Loops::from_work(work(2, 1, true, false)).unbuilt_changes);
        assert!(Loops::from_work(Work::default()).is_empty());
    }

    /// OPEN-1. A command that ended badly is open until a later command ends well, and writing a
    /// fix does not close it.
    #[test]
    fn a_command_that_exited_non_zero_is_a_failed_command_until_another_runs() {
        let mut record = Work::default();
        record.count(false, true, true);
        assert!(Loops::from_work(record).failed_command);
        record.count(true, false, false);
        assert!(Loops::from_work(record).failed_command, "a write closed it");
        record.count(false, true, true);
        assert!(Loops::from_work(record).failed_command);
        record.count(false, true, false);
        assert!(!Loops::from_work(record).failed_command);
    }

    /// OPEN-1. A file this session wrote counts as uncommitted while it differs from `HEAD`, and
    /// no longer once it is committed. A file it did not write is not counted.
    #[cfg(unix)]
    #[test]
    fn a_file_written_and_committed_is_not_uncommitted() {
        let root = crate::testutil::scratch_dir("open-loops-uncommitted");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        let root = root.canonicalize().expect("canonical");
        let run = |arguments: &[&str]| {
            let status = std::process::Command::new("git")
                .args(arguments)
                .current_dir(&root)
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env_remove("GIT_INDEX_FILE")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .status()
                .expect("git ran");
            assert!(status.success(), "git {arguments:?}");
        };
        run(&["init", "-q"]);
        std::fs::write(root.join("a.txt"), "one").expect("a");
        std::fs::write(root.join("b.txt"), "one").expect("b");
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "base",
        ]);
        std::fs::write(root.join("a.txt"), "two").expect("a");
        std::fs::write(root.join("b.txt"), "two").expect("b");
        let written = [root.join("a.txt")];
        assert_eq!(uncommitted(&root, &written), written);
        run(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qam",
            "next",
        ]);
        assert!(uncommitted(&root, &written).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// OPEN-2. Every kind has exactly one owner, and the split is the one the spec gives.
    #[test]
    fn each_kind_has_one_owner() {
        let owned = |owner| {
            Kind::ALL
                .into_iter()
                .filter(|kind| kind.owner() == owner)
                .count()
        };
        assert_eq!(
            (
                owned(Owner::Planner),
                owned(Owner::Person),
                owned(Owner::Wait)
            ),
            (3, 3, 1)
        );
    }

    /// OPEN-4, OPEN-7. The planner reads its own loops as its work, the person's as theirs, and the
    /// jobs by the names bravebot minted, in fixed words and counts.
    #[test]
    fn the_planners_message_is_fixed_text_and_counts() {
        let loops = Loops {
            unbuilt_changes: true,
            failed_command: true,
            open_todos: 2,
            running_jobs: vec!["job:1".to_string()],
            kept_checkouts: 1,
            uncommitted_files: 3,
            stopped_goal: false,
        };
        assert_eq!(
            loops.to_the_planner().as_deref(),
            Some(
                "Open from earlier in this session: changes not built since the last write; the \
                 last command failed; 2 todos are open. Yours to act on if they still matter. Also \
                 open, for the person: 1 kept checkout, 3 files not committed."
            )
        );
        let theirs = Loops {
            uncommitted_files: 1,
            ..Loops::default()
        };
        assert_eq!(
            theirs.to_the_planner().as_deref(),
            Some(
                "Open from earlier in this session, for the person: 1 file not committed. These \
                 are theirs to act on, not yours."
            )
        );
        assert_eq!(Loops::default().to_the_planner(), None);
        let waiting = Loops {
            running_jobs: vec!["job:1".to_string()],
            ..Loops::default()
        };
        assert_eq!(waiting.to_the_planner(), None, "the jobs are told already");
    }
}
