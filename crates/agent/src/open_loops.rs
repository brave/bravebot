//! The unfinished business a session leaves (OPEN).
//!
//! Every loop is found by a fixed rule over bravebot's own records: the work record, the task
//! list, the job table, the checkout list, git's status of the files the session wrote, and the
//! goal. Nothing here reads what a model said, so nothing a model says opens or closes a loop.

use crate::outcome::Work;
use crate::tools::tally;
use crate::workspace::{SessionCheckout, Workspace};
use bravebot_i18n::t;

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

    /// The person's line: each open kind with its count, grouped by whose move it is, or
    /// `open: nothing` (OPEN-3). No path, command, todo text or job name.
    pub fn to_the_person(&self) -> String {
        if self.is_empty() {
            return t!(open_loops_nothing).to_string();
        }
        let named = |kinds: Vec<Kind>| -> String {
            kinds
                .into_iter()
                .map(|kind| {
                    let count = self.count(kind);
                    match kind {
                        Kind::UnbuiltChanges => t!(open_loops_unbuilt).to_string(),
                        Kind::FailedCommand => t!(open_loops_failed).to_string(),
                        Kind::OpenTodos => t!(open_loops_todos, count = count),
                        Kind::KeptCheckouts => t!(open_loops_checkouts, count = count),
                        Kind::UncommittedFiles => t!(open_loops_uncommitted, count = count),
                        Kind::StoppedGoal => t!(open_loops_goal).to_string(),
                        Kind::RunningJobs => t!(open_loops_jobs, count = count),
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut groups = Vec::new();
        let planner = self.of(Owner::Planner);
        if !planner.is_empty() {
            groups.push(t!(open_loops_planner, items = named(planner)));
        }
        let person = self.of(Owner::Person);
        if !person.is_empty() {
            groups.push(t!(open_loops_person, items = named(person)));
        }
        let waiting = self.of(Owner::Wait);
        if !waiting.is_empty() {
            groups.push(named(waiting));
        }
        t!(open_loops_line, groups = groups.join("; "))
    }

    /// What the planner is told at the start of a turn, where any loop is open (OPEN-4).
    ///
    /// Fixed English, counts and the job names bravebot minted. No path, no command and no todo text, so nothing a turn read or
    /// wrote speaks to the planner through it (OPEN-7).
    pub fn to_the_planner(&self) -> Option<String> {
        if self.is_empty() {
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
            true => message.push(':'),
        }
        // The job names are ones bravebot minted, so they hold nothing a command printed (RUN-15).
        match self.running_jobs.as_slice() {
            [] => {}
            [one] => message.push_str(&format!(" 1 job is still running: {one}.")),
            many => message.push_str(&format!(
                " {} jobs are still running: {}.",
                many.len(),
                many.join(", ")
            )),
        }
        Some(message)
    }

    /// The counts of [`Loops::to_the_person`] as one sentence, for a window that shows what a resumed session left open as
    /// a notice (OPEN-6): the planner's loops with no owner named, then the person's, said to be
    /// theirs. `None` where nothing is open, since a resume with nothing open shows nothing.
    pub fn left_open(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut groups = Vec::new();
        groups.extend(self.named(Owner::Planner));
        if let Some(items) = self.named(Owner::Person) {
            groups.push(t!(open_loops_left_open_person, items = items));
        }
        groups.extend(self.named(Owner::Wait));
        Some(t!(open_loops_left_open, groups = groups.join("; ")))
    }

    /// Each open kind of `owner`'s with its count, as the catalog names it, or `None` where none
    /// is open.
    fn named(&self, owner: Owner) -> Option<String> {
        let kinds = self.of(owner);
        if kinds.is_empty() {
            return None;
        }
        let named: Vec<String> = kinds
            .into_iter()
            .map(|kind| {
                let count = self.count(kind);
                match kind {
                    Kind::UnbuiltChanges => t!(open_loops_unbuilt).to_string(),
                    Kind::FailedCommand => t!(open_loops_failed).to_string(),
                    Kind::OpenTodos => t!(open_loops_todos, count = count),
                    Kind::KeptCheckouts => t!(open_loops_checkouts, count = count),
                    Kind::UncommittedFiles => t!(open_loops_uncommitted, count = count),
                    Kind::StoppedGoal => t!(open_loops_goal).to_string(),
                    Kind::RunningJobs => t!(open_loops_jobs, count = count),
                }
            })
            .collect();
        Some(named.join(", "))
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

/// The open loops a session's last turn ended with, and the ones the planner was last told of
/// (OPEN-3, OPEN-4).
///
/// Every front end decides "changed since the last turn" and "changed since the planner was last
/// told" with this one rule.
#[derive(Debug, Clone, Default)]
pub struct Told {
    ended: Loops,
    told: Loops,
}

impl Told {
    /// Record the set a turn ended with, and return whether it differs from the set the last turn
    /// ended with, which is when the person is shown a line (OPEN-3).
    pub fn ended_with(&mut self, loops: Loops) -> bool {
        if loops == self.ended {
            return false;
        }
        self.ended = loops;
        true
    }

    /// What the planner is told at the start of the next turn, where the set the last turn ended
    /// with differs from the set it was last told of (OPEN-4). `None` when nothing changed, and
    /// when nothing is open.
    pub fn for_the_planner(&mut self) -> Option<String> {
        if self.ended == self.told {
            return None;
        }
        self.told = self.ended.clone();
        self.told.to_the_planner()
    }
}

/// The rows of a task list still open: the ones not struck through (OPEN-1). A finished or a
/// dropped task is struck, so what is left is the work still to come.
pub fn open_todos(rows: &[bravebot_core::todo::Row]) -> Vec<&bravebot_core::todo::Row> {
    rows.iter().filter(|row| !row.struck()).collect()
}

/// The checkouts `workspace` keeps for the session and has neither applied nor removed (OPEN-1).
/// A removed one is no longer listed, and one whose delegate still runs has no size yet.
pub fn kept_checkouts(workspace: &Workspace) -> Vec<SessionCheckout> {
    workspace
        .session_checkouts()
        .into_iter()
        .filter(|checkout| checkout.size.is_some() && !checkout.applied)
        .collect()
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
                    beside: crate::tools::Beside::every(),
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
    // Compared as bytes, so a name that is not UTF-8 never matches the listing's lossy lookalike
    // (PATH-001). git's status reader lists only names that are text, so such a name counts as
    // nothing uncommitted.
    let differs: std::collections::BTreeSet<&[u8]> = listing
        .changed
        .iter()
        .chain(&listing.removed)
        .map(String::as_bytes)
        .collect();
    written
        .iter()
        .filter(|path| {
            path.strip_prefix(&repository)
                .is_ok_and(|relative| differs.contains(relative.as_os_str().as_encoded_bytes()))
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

    /// OPEN-4. A tracker no turn has ended into tells the planner nothing.
    #[test]
    fn a_tracker_with_nothing_ended_tells_nothing() {
        assert_eq!(Told::default().for_the_planner(), None);
    }

    /// OPEN-4. The planner is told of a changed set once, and not again until the set changes.
    #[test]
    fn the_planner_is_told_once_per_changed_set() {
        let mut told = Told::default();
        let failed = Loops::from_work(work(0, 1, true, true));
        told.ended_with(failed.clone());
        // Nothing told reads as empty, which the first assertion rejects.
        let said = told.for_the_planner().unwrap_or_default();
        assert!(
            said.starts_with("Open from earlier in this session"),
            "{said}"
        );
        assert!(said.contains("the last command failed"), "{said}");
        assert_eq!(told.for_the_planner(), None, "told twice");

        told.ended_with(failed.clone());
        assert_eq!(told.for_the_planner(), None, "the same set was told again");

        let more = Loops {
            open_todos: 2,
            ..failed
        };
        told.ended_with(more);
        let said = told.for_the_planner().unwrap_or_default();
        assert!(said.contains("2 todos"), "{said}");
        assert_eq!(told.for_the_planner(), None);
    }

    /// OPEN-3, OPEN-4. A set that closes every loop is a change, and it tells the planner
    /// nothing, since nothing is open.
    #[test]
    fn a_set_that_empties_tells_nothing() {
        let mut told = Told::default();
        told.ended_with(Loops::from_work(work(0, 1, true, true)));
        assert!(told.for_the_planner().is_some());
        assert!(told.ended_with(Loops::default()));
        assert_eq!(told.for_the_planner(), None);
        told.ended_with(Loops::from_work(work(0, 1, true, true)));
        assert!(told.for_the_planner().is_some(), "the loop reopened");
    }

    /// OPEN-3. `ended_with` says the set changed only when it differs from the last one ended.
    #[test]
    fn ending_with_a_set_says_whether_it_changed() {
        let mut told = Told::default();
        assert!(!told.ended_with(Loops::default()));
        let failed = Loops::from_work(work(0, 1, true, true));
        assert!(told.ended_with(failed.clone()));
        assert!(!told.ended_with(failed));
        assert!(told.ended_with(Loops::default()));
    }

    /// OPEN-6. The sentence a resumed session's notice shows names the same counts, the planner's
    /// with no owner and the person's as theirs, and there is none for an empty set.
    #[test]
    fn the_sentence_a_resume_shows_names_the_same_counts() {
        assert_eq!(
            Loops::from_work(work(2, 1, false, true))
                .left_open()
                .as_deref(),
            Some("Left open from earlier: changes not built, the last command failed.")
        );
        assert_eq!(Loops::default().left_open(), None);
        let resumed = Loops {
            open_todos: 2,
            kept_checkouts: 1,
            uncommitted_files: 3,
            ..Loops::from_work(work(2, 1, false, true))
        };
        assert_eq!(
            resumed.left_open().as_deref(),
            Some(
                "Left open from earlier: changes not built, the last command failed, 2 todos; \
                 1 kept checkout, 3 files not committed, for you to act on."
            )
        );
        let yours = Loops {
            kept_checkouts: 1,
            ..Loops::default()
        };
        assert_eq!(
            yours.left_open().as_deref(),
            Some("Left open from earlier: 1 kept checkout, for you to act on.")
        );
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

    /// OPEN-1, OPEN-6, FILES-1. A written file outside the repository, such as one in the
    /// session's own directory that a resumed session reloads, is compared with nothing and so is
    /// not uncommitted, while a changed file inside the repository still is.
    #[cfg(unix)]
    #[test]
    fn a_written_file_outside_the_repository_is_not_uncommitted() {
        let base = crate::testutil::scratch_dir("open-loops-outside");
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("repo");
        let outside = base.join("session");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::create_dir_all(&outside).expect("outside");
        let root = root.canonicalize().expect("canonical");
        let outside = outside.canonicalize().expect("canonical");
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .expect("git ran");
        assert!(status.success());
        std::fs::write(root.join("a.txt"), "one").expect("a");
        std::fs::write(outside.join("note.txt"), "one").expect("note");
        let written = [root.join("a.txt"), outside.join("note.txt")];
        assert_eq!(uncommitted(&root, &written), [root.join("a.txt")]);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// OPEN-1, PATH-003. A written name that is not UTF-8 is never taken for its lossy lookalike,
    /// in either direction.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_name_that_is_not_utf8_is_told_apart_from_its_lossy_lookalike() {
        use std::os::unix::ffi::OsStrExt;
        let root = crate::testutil::scratch_dir("open-loops-not-utf8");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        let root = root.canonicalize().expect("canonical");
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .expect("git ran");
        assert!(status.success());
        let raw = root.join(std::ffi::OsStr::from_bytes(b"tool-\xff"));
        let lookalike = root.join("tool-\u{FFFD}");
        std::fs::write(&raw, "raw").expect("raw");
        std::fs::write(&lookalike, "lookalike").expect("lookalike");
        assert_eq!(
            uncommitted(&root, std::slice::from_ref(&lookalike)),
            [lookalike]
        );
        assert!(
            uncommitted(&root, std::slice::from_ref(&raw)).is_empty(),
            "the raw name was taken for its lookalike"
        );
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

    /// OPEN-3. The person's line names each open kind with its count, grouped by whose move it
    /// is, with no path, command, todo text or job name.
    #[test]
    fn the_persons_line_groups_kinds_by_owner_and_names_no_path() {
        let loops = Loops {
            unbuilt_changes: true,
            failed_command: true,
            open_todos: 2,
            running_jobs: vec!["job:1".to_string()],
            kept_checkouts: 1,
            uncommitted_files: 3,
            stopped_goal: true,
        };
        assert_eq!(
            loops.to_the_person(),
            "open: changes not built, the last command failed, 2 todos (planner); 1 kept \
             checkout, 3 files not committed, a stopped goal (you); 1 job running"
        );
    }

    /// OPEN-3. With nothing open, the line says nothing is open.
    #[test]
    fn the_persons_line_with_nothing_open_is_open_nothing() {
        assert_eq!(Loops::default().to_the_person(), "open: nothing");
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
                 open, for the person: 1 kept checkout, 3 files not committed. 1 job is still \
                 running: job:1."
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
        assert_eq!(
            waiting.to_the_planner().as_deref(),
            Some("Open from earlier in this session: 1 job is still running: job:1.")
        );
    }
}
