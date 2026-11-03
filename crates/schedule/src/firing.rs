//! What a run started for a stored entry keeps beside the entry: a mark that pauses it, the question
//! it stopped at, and a lock that keeps two starts of one entry from running together.
//!
//! The files are named for the entry and sit in the same directory as its own files, so deleting the
//! entry can delete them. Nothing here reads a file's contents to decide anything: whether a mark or
//! a question exists is the person's own state, and the question's lines are written out as they
//! were cleaned for a terminal and printed by `schedule pending` without being parsed (SCHEDULE-2).

use bravebot_agent::home;
use bravebot_core::ask::Asking;
use bravebot_session::search;
use std::path::{Path, PathBuf};

/// The end of the name of the file that marks an entry paused. Its content is never read.
const PAUSED: &str = ".paused";

/// The end of the name of the file that holds the question a run stopped at, as lines already
/// cleaned for a terminal. It is printed and never parsed.
const PENDING: &str = ".pending";

/// The end of the name of the file a running `run-now` holds a lock on, so a second start of the
/// same entry sees the first.
const LOCK: &str = ".lock";

/// The environment variable that names the stored schedule a run belongs to. `run-now` sets it for
/// the one-shot run it starts, and a run that finds it names a stored entry stops at the first
/// question and keeps it (SCHEDULE-2).
pub const FIRING_VARIABLE: &str = "BRAVEBOT_SCHEDULE_ENTRY";

/// The longest question line a pending file holds, in characters.
const LONGEST_QUESTION: usize = 500;

/// The longest choice line a pending file holds, in characters.
const LONGEST_CHOICE: usize = 160;

/// The files kept beside the entries of one directory.
pub struct Store {
    directory: PathBuf,
}

impl Store {
    pub fn at(directory: &Path) -> Self {
        Self {
            directory: directory.to_path_buf(),
        }
    }

    /// The file `name` has the ending `end` on.
    fn sidecar(&self, name: &str, end: &str) -> PathBuf {
        self.directory.join(format!("{name}{end}"))
    }

    pub fn is_paused(&self, name: &str) -> bool {
        self.sidecar(name, PAUSED).is_file()
    }

    pub fn mark_paused(&self, name: &str) -> std::io::Result<()> {
        home::write_file(&self.sidecar(name, PAUSED), b"")
    }

    /// Take the mark off. An entry with no mark is not an error.
    pub fn unmark_paused(&self, name: &str) -> std::io::Result<()> {
        match std::fs::remove_file(self.sidecar(name, PAUSED)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    /// Write the question a run stopped at, replacing an earlier one. `lines` are already cleaned.
    fn write_pending(&self, name: &str, lines: &[String]) -> std::io::Result<()> {
        let mut text = lines.join("\n");
        text.push('\n');
        home::replace(&self.sidecar(name, PENDING), text)
    }

    /// The lines of the question `name` is waiting on, cleaned again as they are read because the
    /// file is one a person could have edited. `None` where there is no such file.
    pub fn pending(&self, name: &str) -> Option<Vec<String>> {
        let text = std::fs::read_to_string(self.sidecar(name, PENDING)).ok()?;
        Some(
            text.lines()
                .map(|line| search::clean(line, LONGEST_QUESTION))
                .collect(),
        )
    }

    /// Delete the pending question of `name`, and say whether there was one. Whether there was one
    /// is the file's existence and not its content.
    pub fn clear_pending(&self, name: &str) -> bool {
        std::fs::remove_file(self.sidecar(name, PENDING)).is_ok()
    }

    /// Take the exclusive hold on `name` that a running `run-now` keeps until it ends, or `None`
    /// where another run of the same entry holds it. The hold is a lock on a file the operating
    /// system releases when the process ends however it ends, so a run that was killed leaves no
    /// hold behind.
    pub fn hold(&self, name: &str) -> std::io::Result<Option<std::fs::File>> {
        home::create_directory(&self.directory)?;
        let file = home::append_to_file(&self.sidecar(name, LOCK))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(file)),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(error)) => Err(error),
        }
    }

    /// Delete everything kept beside the entry `name`, so a later entry of the same name does not
    /// inherit a pause, a question or a lock.
    pub fn forget(&self, name: &str) {
        for end in [PAUSED, PENDING, LOCK] {
            std::fs::remove_file(self.sidecar(name, end)).ok();
        }
    }
}

/// The entry a run says it belongs to: `value` (the variable `run-now` sets, read by the caller)
/// when `is_stored` says it names a stored entry, and `None` otherwise, so one that names nothing
/// changes nothing.
pub fn named_by(value: Option<String>, is_stored: impl FnOnce(&str) -> bool) -> Option<String> {
    let value = value?;
    is_stored(&value).then_some(value)
}

/// Keep the question a run of `name` stopped at, where it stopped at one. Best effort: a run that
/// cannot write here has still run.
pub fn record_firing(store: &Store, name: &str, asked: Option<&Asking>) {
    if let Some(asking) = asked {
        store.write_pending(name, &question_lines(asking)).ok();
    }
}

/// The lines of a question for a terminal. The text is the planner's, already released for display
/// by the tool that put the question, and it is only written out here: nothing below chooses
/// between its lines, and each is cleaned of what a terminal would act on.
fn question_lines(asking: &Asking) -> Vec<String> {
    asking
        .prompts
        .iter()
        .flat_map(|prompt| {
            let head = format!("{}: {}", prompt.header, prompt.question);
            let rows = prompt.rows.iter().map(|row| {
                format!(
                    "{}. {}",
                    row.index,
                    search::clean(&row.label, LONGEST_CHOICE)
                )
            });
            std::iter::once(search::clean(&head, LONGEST_QUESTION)).chain(rows)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            // nosemgrep: rust.lang.security.temp-dir.temp-dir
            let path = std::env::temp_dir().join(format!(
                "bravebot-firing-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::remove_dir_all(&path).ok();
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn a_question(text: &str) -> Asking {
        Asking {
            prompts: vec![bravebot_core::ask::Prompt {
                header: "Pick".to_string(),
                question: text.to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_pause_mark_is_put_on_and_taken_off() {
        let scratch = Scratch::new("pause");
        let store = Store::at(&scratch.0);
        assert!(!store.is_paused("3"));
        store.mark_paused("3").expect("marks");
        assert!(store.is_paused("3"));
        assert!(!store.is_paused("4"));
        store.unmark_paused("3").expect("unmarks");
        assert!(!store.is_paused("3"));
        store
            .unmark_paused("3")
            .expect("an unmarked entry is not an error");
    }

    #[test]
    fn a_question_is_kept_replaced_read_cleaned_and_cleared() {
        let scratch = Scratch::new("pending");
        let store = Store::at(&scratch.0);
        assert_eq!(store.pending("3"), None);
        record_firing(&store, "3", Some(&a_question("Which branch?")));
        assert_eq!(
            store.pending("3"),
            Some(vec!["Pick: Which branch?".to_string()])
        );
        record_firing(&store, "3", Some(&a_question("Which remote?")));
        assert_eq!(
            store.pending("3"),
            Some(vec!["Pick: Which remote?".to_string()])
        );
        // A run that asked nothing leaves an earlier question as it was.
        record_firing(&store, "3", None);
        assert!(store.pending("3").is_some());
        // The file is one a person could edit, so what is read back is cleaned again.
        std::fs::write(scratch.0.join("3.pending"), "ok\n\u{1b}[31mred\n").expect("writes");
        let lines = store.pending("3").expect("reads");
        assert!(
            lines.iter().all(|line| !line.contains('\u{1b}')),
            "{lines:?}"
        );
        assert!(store.clear_pending("3"));
        assert!(!store.clear_pending("3"), "nothing was left to clear");
        assert_eq!(store.pending("3"), None);
    }

    #[test]
    fn a_second_hold_on_one_entry_is_refused_until_the_first_is_dropped() {
        let scratch = Scratch::new("hold");
        let store = Store::at(&scratch.0);
        let first = store.hold("3").expect("locks").expect("the first hold");
        assert!(store.hold("3").expect("locks").is_none());
        assert!(
            store.hold("4").expect("locks").is_some(),
            "another entry is not held"
        );
        drop(first);
        assert!(store.hold("3").expect("locks").is_some());
    }

    #[test]
    fn forgetting_an_entry_deletes_what_was_kept_beside_it() {
        let scratch = Scratch::new("forget");
        let store = Store::at(&scratch.0);
        store.mark_paused("3").expect("marks");
        record_firing(&store, "3", Some(&a_question("Which?")));
        drop(store.hold("3").expect("locks"));
        store.mark_paused("30").expect("marks");
        store.forget("3");
        assert!(!store.is_paused("3"));
        assert_eq!(store.pending("3"), None);
        assert!(!scratch.0.join("3.lock").exists());
        assert!(store.is_paused("30"), "another entry keeps its mark");
    }

    #[test]
    fn an_entry_named_in_the_variable_must_be_stored() {
        let stored = |value: &str| value == "3";
        assert_eq!(
            named_by(None, |_| true),
            None,
            "an unset variable names nothing"
        );
        assert_eq!(
            named_by(Some("9".into()), stored),
            None,
            "a value that names no stored entry changes nothing"
        );
        assert_eq!(named_by(Some("3".into()), stored), Some("3".into()));
    }
}
