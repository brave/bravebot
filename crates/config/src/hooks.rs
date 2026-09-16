//! The hooks file, `hooks.json` in the user's own state directory.
//!
//! A hook is a command a person asked to have run when something happens: format after an edit,
//! notify when a turn is over. This module reads the declarations and nothing else. Running one is
//! the turn loop's, because starting a process is not this crate's business.
//!
//! # Why not a settings layer
//!
//! `settings.json` is read from three places and two of them sit in a checkout, so a command named
//! in one would be a command that arrived with a clone. The settings reader also merges the three
//! a name at a time, which means "the user's own layer only" would be a rule about a merge table
//! rather than a property of the file: an edit to that table could hand the name back to a project
//! layer with nothing to notice. A separate file in the state directory cannot be carried by a
//! checkout at all, which is the same claim made without depending on anybody reading a merge rule
//! correctly.
//!
//! # An argument vector, not a line
//!
//! `run` names a program and its arguments, as a list. Nothing concatenates them into a string and
//! nothing re-parses one, so there is no shell between the file and the process and no quoting
//! question to get wrong. That is a deliberate difference from how Claude Code spells a hook, whose
//! `command` is a line a shell runs.
//!
//! # Every failure is per entry
//!
//! A missing file, an oversized one, a syntax error, an entry naming a moment nothing fires or
//! carrying no program: each is that much of the file not applying, and never a reason to refuse to
//! start. A mistake in a hook must not be a session that will not open.

use std::path::{Path, PathBuf};

/// The file, inside the state directory.
const HOOKS_FILE: &str = "hooks.json";

/// The most of it worth reading.
///
/// A hooks file is a handful of short argument vectors. Bounded so a file that grew by accident, or
/// was replaced by something else entirely, is passed over rather than parsed.
const MAX_BYTES: u64 = 64 * 1024;

/// Where a person's hooks are declared, for telling them so.
pub fn hooks_file(directory: &Path) -> PathBuf {
    directory.join(HOOKS_FILE)
}

/// A moment a hook can be attached to, in a person's terms.
///
/// Deliberately short. Each is something somebody can point at in a session that has happened:
/// the turn began, a call finished, the turn is over. The audit trail's events are a different
/// vocabulary answering a different question, and a person writing a hook should not have to learn
/// what a gate or a slot is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    /// The turn a person asked for has begun, before anything is sent.
    TurnStarted,
    /// One tool call has finished, whatever came of it.
    ToolFinished,
    /// The turn a person asked for is over, however it ended.
    TurnFinished,
}

impl Moment {
    /// The word a file spells it with, or `None` for a word this build does not fire.
    ///
    /// An unrecognised moment is an entry that never runs rather than an error: a file written for
    /// a later build should leave the hooks this one understands in force.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "turn-started" => Some(Self::TurnStarted),
            "tool-finished" => Some(Self::ToolFinished),
            "turn-finished" => Some(Self::TurnFinished),
            _ => None,
        }
    }

    /// The word, as a file spells it and as a hook is told it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TurnStarted => "turn-started",
            Self::ToolFinished => "tool-finished",
            Self::TurnFinished => "turn-finished",
        }
    }
}

/// One declaration: when to run, and what to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    moment: Moment,
    /// Which tool this is about, where the entry named one. `None` fires for every call.
    ///
    /// Read for every moment and consulted only by [`Moment::ToolFinished`], because a name is
    /// what the writer said and dropping it at read time would leave nothing able to report that
    /// it says nothing here.
    tool: Option<String>,
    /// The program and its arguments. Never empty: an entry with nothing to run is not kept.
    run: Vec<String>,
}

impl Hook {
    /// The moment this fires at.
    pub fn moment(&self) -> Moment {
        self.moment
    }

    /// The tool this is about, where it named one.
    pub fn tool(&self) -> Option<&str> {
        self.tool.as_deref()
    }

    /// The program and its arguments.
    pub fn run(&self) -> &[String] {
        &self.run
    }

    /// Whether this entry fires for `moment`, for a call on `tool` where there was one.
    fn fires(&self, moment: Moment, tool: Option<&str>) -> bool {
        if self.moment != moment {
            return false;
        }
        match (&self.tool, tool) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(wanted), Some(called)) => wanted == called,
        }
    }
}

/// Every hook in force for this user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hooks {
    hooks: Vec<Hook>,
}

impl Hooks {
    /// Read the file in a state directory, or nothing where there is no directory or no file.
    pub fn load(home: Option<&Path>) -> Self {
        let Some(home) = home else {
            return Self::default();
        };
        let path = hooks_file(home);
        match std::fs::metadata(&path) {
            Ok(found) if found.len() > MAX_BYTES => return Self::default(),
            Ok(_) => {}
            Err(_) => return Self::default(),
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text),
            Err(_) => Self::default(),
        }
    }

    /// Read the declarations out of hooks JSON.
    ///
    /// The root is an object with a `hooks` array, so that the file has somewhere to grow a second
    /// key without every reader of the first having to be taught that the root changed shape.
    pub fn parse(text: &str) -> Self {
        let Ok(serde_json::Value::Object(root)) = serde_json::from_str::<serde_json::Value>(text)
        else {
            return Self::default();
        };
        let Some(serde_json::Value::Array(entries)) = root.get("hooks") else {
            return Self::default();
        };
        Self {
            hooks: entries.iter().filter_map(entry).collect(),
        }
    }

    /// Whether anything is declared at all, so a turn that has no hooks can say so without
    /// looking at a moment.
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    /// The hooks that fire for `moment`, in the order the file declared them.
    ///
    /// `tool` is the name of the call that just finished, where the moment is about one. Two
    /// entries matching the same call both fire: a file naming the same moment twice asked for
    /// two commands, not for the later one to replace the earlier.
    pub fn firing(&self, moment: Moment, tool: Option<&str>) -> impl Iterator<Item = &Hook> {
        self.hooks
            .iter()
            .filter(move |hook| hook.fires(moment, tool))
    }
}

/// One entry, or `None` where it says nothing this build can run.
///
/// Dropped rather than refused, and dropped one at a time: a typo in the third entry leaves the
/// other two firing.
fn entry(value: &serde_json::Value) -> Option<Hook> {
    let object = value.as_object()?;
    let moment = Moment::parse(object.get("on")?.as_str()?)?;
    let tool = match object.get("tool") {
        Some(serde_json::Value::String(name)) if !name.trim().is_empty() => {
            Some(name.trim().to_string())
        }
        _ => None,
    };
    let serde_json::Value::Array(words) = object.get("run")? else {
        return None;
    };
    let mut run = Vec::with_capacity(words.len());
    for word in words {
        // Strings only. A number or a boolean where an argument belongs would have to be given a
        // spelling nobody chose, and an argument vector is the one place in this file where
        // guessing at what somebody meant decides what gets executed.
        run.push(word.as_str()?.to_string());
    }
    if run.first().map(|program| program.trim().is_empty()) != Some(false) {
        return None;
    }
    Some(Hook { moment, tool, run })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::scratch_dir;

    fn programs(hooks: &Hooks, moment: Moment, tool: Option<&str>) -> Vec<Vec<String>> {
        hooks
            .firing(moment, tool)
            .map(|hook| hook.run().to_vec())
            .collect()
    }

    /// HOOK-2: the three moments are the vocabulary, and each is spelled the way the file spells it.
    #[test]
    fn each_moment_is_named_by_the_word_a_file_spells_it_with() {
        for moment in [
            Moment::TurnStarted,
            Moment::ToolFinished,
            Moment::TurnFinished,
        ] {
            assert_eq!(Moment::parse(moment.as_str()), Some(moment));
        }
    }

    /// HOOK-2: a file written for a build that fires more moments than this one leaves the hooks
    /// this build does understand in force.
    #[test]
    fn a_moment_this_build_does_not_fire_is_not_a_hook() {
        let hooks = Hooks::parse(
            r#"{"hooks": [
                {"on": "file-opened", "run": ["notify"]},
                {"on": "turn-finished", "run": ["say", "done"]}
            ]}"#,
        );
        assert_eq!(
            programs(&hooks, Moment::TurnFinished, None),
            vec![vec!["say".to_string(), "done".to_string()]]
        );
    }

    /// HOOK-3: a hook is a program and its arguments, kept as written.
    #[test]
    fn a_hook_is_the_argument_vector_the_file_listed() {
        let hooks =
            Hooks::parse(r#"{"hooks": [{"on": "turn-started", "run": ["echo", "a b; c"]}]}"#);
        assert_eq!(
            programs(&hooks, Moment::TurnStarted, None),
            vec![vec!["echo".to_string(), "a b; c".to_string()]]
        );
    }

    /// HOOK-3: a line for a shell to parse is not an argument vector, and is not run as one.
    #[test]
    fn a_command_written_as_one_string_is_not_a_hook() {
        let hooks = Hooks::parse(r#"{"hooks": [{"on": "turn-started", "run": "echo hi"}]}"#);
        assert!(hooks.is_empty());
    }

    /// HOOK-3: nothing is coerced into an argument, because an argument vector is where guessing
    /// at what somebody meant decides what runs.
    #[test]
    fn an_argument_that_is_not_a_string_drops_the_entry() {
        let hooks = Hooks::parse(r#"{"hooks": [{"on": "turn-started", "run": ["sleep", 3]}]}"#);
        assert!(hooks.is_empty());
    }

    /// HOOK-3: an entry with no program names nothing to run.
    #[test]
    fn an_entry_with_no_program_is_not_a_hook() {
        for text in [
            r#"{"hooks": [{"on": "turn-started", "run": []}]}"#,
            r#"{"hooks": [{"on": "turn-started", "run": ["   "]}]}"#,
            r#"{"hooks": [{"on": "turn-started"}]}"#,
        ] {
            assert!(Hooks::parse(text).is_empty(), "{text}");
        }
    }

    /// HOOK-2: a tool moment with a name fires for that call and no other.
    #[test]
    fn a_hook_naming_a_tool_fires_for_that_tool_alone() {
        let hooks = Hooks::parse(
            r#"{"hooks": [{"on": "tool-finished", "tool": "write_file", "run": ["fmt"]}]}"#,
        );
        assert_eq!(
            programs(&hooks, Moment::ToolFinished, Some("write_file")),
            vec![vec!["fmt".to_string()]]
        );
        assert!(programs(&hooks, Moment::ToolFinished, Some("read_file")).is_empty());
    }

    /// HOOK-2: a tool moment that names none fires for every call.
    #[test]
    fn a_hook_naming_no_tool_fires_for_every_call() {
        let hooks = Hooks::parse(r#"{"hooks": [{"on": "tool-finished", "run": ["log"]}]}"#);
        assert_eq!(
            programs(&hooks, Moment::ToolFinished, Some("search")).len(),
            1
        );
        assert_eq!(programs(&hooks, Moment::ToolFinished, Some("run")).len(), 1);
    }

    /// HOOK-2: naming a tool says which calls an entry is about, so an entry naming one on a
    /// moment that is not about a call is about no call there is.
    #[test]
    fn a_tool_named_on_a_turn_moment_fires_for_nothing() {
        let hooks = Hooks::parse(
            r#"{"hooks": [{"on": "turn-started", "tool": "write_file", "run": ["fmt"]}]}"#,
        );
        assert!(programs(&hooks, Moment::TurnStarted, None).is_empty());
    }

    /// HOOK-2: a moment fires the hooks written for it and nothing written for another.
    #[test]
    fn a_moment_fires_only_the_hooks_written_for_it() {
        let hooks = Hooks::parse(
            r#"{"hooks": [
                {"on": "turn-started", "run": ["begin"]},
                {"on": "turn-finished", "run": ["end"]}
            ]}"#,
        );
        assert_eq!(
            programs(&hooks, Moment::TurnStarted, None),
            vec![vec!["begin".to_string()]]
        );
        assert_eq!(
            programs(&hooks, Moment::TurnFinished, None),
            vec![vec!["end".to_string()]]
        );
    }

    /// HOOK-2: two entries for one moment are two commands, in the order the file listed them.
    #[test]
    fn two_entries_for_one_moment_both_fire_in_order() {
        let hooks = Hooks::parse(
            r#"{"hooks": [
                {"on": "turn-finished", "run": ["first"]},
                {"on": "turn-finished", "run": ["second"]}
            ]}"#,
        );
        assert_eq!(
            programs(&hooks, Moment::TurnFinished, None),
            vec![vec!["first".to_string()], vec!["second".to_string()]]
        );
    }

    /// HOOK-7: one unusable entry is that much of the file not applying, not a file that is
    /// refused.
    #[test]
    fn an_unusable_entry_leaves_the_others_firing() {
        let hooks = Hooks::parse(
            r#"{"hooks": [
                {"on": "turn-started", "run": 7},
                {"on": "turn-started", "run": ["still-here"]}
            ]}"#,
        );
        assert_eq!(
            programs(&hooks, Moment::TurnStarted, None),
            vec![vec!["still-here".to_string()]]
        );
    }

    /// HOOK-7: a file nobody can parse leaves a session that starts with no hooks, rather than one
    /// that will not start.
    #[test]
    fn an_unparseable_file_declares_no_hooks() {
        for text in ["", "not json", "[]", r#"{"hooks": {}}"#] {
            assert!(Hooks::parse(text).is_empty(), "{text}");
        }
    }

    /// HOOK-1: the declarations are read from the state directory, under its own name.
    #[test]
    fn the_declarations_are_read_from_the_state_directory() {
        let home = scratch_dir("hooks-from-the-file");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("a scratch state directory");
        std::fs::write(
            hooks_file(&home),
            r#"{"hooks": [{"on": "turn-started", "run": ["from-the-file"]}]}"#,
        )
        .expect("write a hooks file");

        let hooks = Hooks::load(Some(&home));
        std::fs::remove_dir_all(&home).ok();

        assert_eq!(
            programs(&hooks, Moment::TurnStarted, None),
            vec![vec!["from-the-file".to_string()]]
        );
    }

    /// HOOK-1: a machine whose platform names no state directory has no hooks rather than hooks
    /// read from somewhere nobody chose.
    #[test]
    fn no_state_directory_means_no_hooks() {
        assert!(Hooks::load(None).is_empty());
    }

    /// HOOK-7: a file too large to be the handful of short vectors this reads is passed over.
    #[test]
    fn an_oversized_file_declares_no_hooks() {
        let home = scratch_dir("hooks-oversized");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("a scratch state directory");
        let padding = " ".repeat(MAX_BYTES as usize + 1);
        std::fs::write(
            hooks_file(&home),
            format!(r#"{{"hooks": [{{"on": "turn-started", "run": ["x"]}}]{padding}}}"#),
        )
        .expect("write a hooks file");

        let hooks = Hooks::load(Some(&home));
        std::fs::remove_dir_all(&home).ok();

        assert!(hooks.is_empty());
    }
}
