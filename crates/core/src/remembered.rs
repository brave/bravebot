//! The command lines a person asked to be remembered past the session.
//!
//! A third standing answer at the run prompt, beside the vouched list in [`crate::programs`] and
//! the rules in [`crate::permissions`], and the only one that outlives the session that gave it.
//! What it grants is the smaller half of a vouch: a covered line runs without anybody being asked,
//! and what it prints carries the label it would have carried anyway, which is untrusted and
//! private. Nothing here raises a label, reads the trust map or writes one.
//!
//! # A line, not a pattern
//!
//! An entry holds the plan as it was approved, field by field: each step's name, the file that name
//! resolved to, each argument on its own, each environment assignment with its name and its value
//! apart, and where each stream went. A later plan is covered when every one of those is equal and
//! in no other case, so nothing in an entry has a spelling that means "any text" and no entry can
//! reach a second line. That is the whole difference from a rule in the settings file, which is
//! matched against one string in a language where a character means anything, and which is the way
//! to cover a family of lines on purpose.
//!
//! The joins are part of an entry too. `a && b`, `a | b` and `a ; b` run the same programs and are
//! three different lines, so the shape is recorded and compared rather than flattened away.
//!
//! # What is deliberately not in an entry
//!
//! The directory. The record is kept per directory by whoever holds it, and a line is only ever
//! recorded where it runs at the workspace root, so the directory is the file the entry sits in
//! rather than a field inside it.
//!
//! The label of anything fed to the first step. That is the policy's own [`crate::command::Plan`]
//! field rather than something the line says, so an entry cannot account for it and the gate asks
//! about private input before it consults this at all.
//!
//! This crate performs no I/O, so where the entries are kept and how they are spelled on disk is
//! `bravebot_agent::remembered`'s.

use crate::command::{Joiner, Plan, Route, Step, Steps};

/// One step of a remembered line, in the fields a person read at the prompt.
///
/// The name as well as the file it resolved to, because the name is what they read and a second
/// name for the same binary is a line they have not seen. Equality of both is what says the name
/// still resolves to the same binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RememberedStep {
    /// The name the line used.
    pub program: String,
    /// The file that name resolved to, absolute.
    pub resolved: String,
    /// The arguments, in order, each its own value.
    pub args: Vec<String>,
    /// `NAME=value` written in front of the program, each name and value its own value.
    ///
    /// Part of the key rather than left out of it: an assignment decides what a program loads
    /// before its own arguments are looked at, so an entry without it would cover the line the
    /// person read with anything at all put in front of it.
    pub environment: Vec<(String, String)>,
    /// Where this step's streams went.
    ///
    /// Part of the key for the reason the assignments are: sending the errors somewhere else makes
    /// a different line, and an entry made while two streams were merged must not cover the same
    /// program with them apart.
    pub routes: Vec<Route>,
}

impl RememberedStep {
    /// The step as it was approved.
    pub fn of(step: &Step) -> Self {
        Self {
            program: step.program.clone(),
            resolved: step.resolved.to_string_lossy().to_string(),
            args: step.args.clone(),
            environment: step.environment.clone(),
            routes: step.routes.clone(),
        }
    }

    /// Whether a step about to run is the one this entry holds.
    pub fn matches(&self, step: &Step) -> bool {
        *self == Self::of(step)
    }

    /// The step as it was drawn at the prompt.
    ///
    /// The same rendering the prompt used, produced by the same code, because a reading back that
    /// spelled a line differently from the screen the person answered on would be describing
    /// something they have to translate before they can recognise it.
    pub fn display(&self) -> String {
        self.as_step().display()
    }

    fn as_step(&self) -> Step {
        Step {
            program: self.program.clone(),
            resolved: std::path::PathBuf::from(&self.resolved),
            args: self.args.clone(),
            environment: self.environment.clone(),
            routes: self.routes.clone(),
        }
    }
}

/// How the steps of a remembered line are joined.
///
/// The same shape [`Steps`] has. Kept rather than flattened because two lines running the same
/// programs in different arrangements are two lines, and a person answered about one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// Steps feeding one another, and a single step where the line had no pipe.
    Pipeline(Vec<RememberedStep>),
    /// Two of these joined by `&&`, `||` or `;`.
    Join {
        left: Box<Shape>,
        joiner: Joiner,
        right: Box<Shape>,
    },
    /// `( … )`, which groups and starts nothing of its own.
    Group(Box<Shape>),
}

impl Shape {
    /// The shape as it was approved.
    pub fn of(steps: &Steps) -> Self {
        match steps {
            Steps::Pipeline(list) => Self::Pipeline(list.iter().map(RememberedStep::of).collect()),
            Steps::Join {
                left,
                joiner,
                right,
            } => Self::Join {
                left: Box::new(Self::of(left)),
                joiner: *joiner,
                right: Box::new(Self::of(right)),
            },
            Steps::Group(inner) => Self::Group(Box::new(Self::of(inner))),
        }
    }

    /// Every step this shape holds, in the order the line writes them.
    pub fn steps(&self) -> Vec<&RememberedStep> {
        let mut out = Vec::new();
        self.gather(&mut out);
        out
    }

    fn gather<'a>(&'a self, out: &mut Vec<&'a RememberedStep>) {
        match self {
            Self::Pipeline(list) => out.extend(list.iter()),
            Self::Join { left, right, .. } => {
                left.gather(out);
                right.gather(out);
            }
            Self::Group(inner) => inner.gather(out),
        }
    }

    /// The shape as it was drawn at the prompt.
    pub fn display(&self) -> String {
        match self {
            Self::Pipeline(steps) => steps
                .iter()
                .map(RememberedStep::display)
                .collect::<Vec<_>>()
                .join(" | "),
            Self::Join {
                left,
                joiner,
                right,
            } => {
                let joiner = match joiner {
                    Joiner::And => "&&",
                    Joiner::Or => "||",
                    Joiner::Then => ";",
                };
                format!("{} {joiner} {}", left.display(), right.display())
            }
            Self::Group(inner) => format!("( {} )", inner.display()),
        }
    }
}

/// One command line a person asked to be remembered past the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RememberedLine {
    /// What runs, and how the parts are joined.
    pub steps: Shape,
}

impl RememberedLine {
    /// The line a plan would be recorded as.
    pub fn of(plan: &Plan) -> Self {
        Self {
            steps: Shape::of(&plan.steps),
        }
    }

    /// Whether a plan about to run is the line this entry holds.
    ///
    /// Every field of every step, and the shape they are joined in. Nothing partial: a line
    /// matching in all but one argument is a different line, and this is the whole of what says so.
    pub fn covers(&self, plan: &Plan) -> bool {
        self.steps == Shape::of(&plan.steps)
    }

    /// The line as it was drawn at the prompt.
    pub fn display(&self) -> String {
        self.steps.display()
    }
}

/// One entry, and the session whose answer put it there.
///
/// The session is not part of the key and decides nothing. It is there because the reading back of
/// this record has to say which answers a person is still carrying from an earlier session, and a
/// flat list cannot tell them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub line: RememberedLine,
    /// The session the key was pressed in.
    pub answered_in: String,
}

/// Every line remembered for one directory.
///
/// Read afresh wherever a run prompt would be drawn rather than once at the start, so a line
/// recorded a minute ago in another session is covered by this one. Empty is the ordinary state and
/// means every run asks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Remembered {
    entries: Vec<Entry>,
}

impl Remembered {
    /// Nothing is remembered.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entry, as reading one more line of the record does.
    pub fn record(&mut self, line: RememberedLine, answered_in: impl Into<String>) {
        self.entries.push(Entry {
            line,
            answered_in: answered_in.into(),
        });
    }

    /// Whether some entry is this exact plan.
    ///
    /// Answers whether to ask and nothing else. A caller that has not already refused a line
    /// releasing private data, naming a file to write, or running outside the root must not reach
    /// this: those are asked about whatever is recorded, and an entry cannot speak for them.
    pub fn covers(&self, plan: &Plan) -> bool {
        self.entries.iter().any(|entry| entry.line.covers(plan))
    }

    /// Every entry, in the order they were recorded.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl FromIterator<Entry> for Remembered {
    fn from_iter<I: IntoIterator<Item = Entry>>(entries: I) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn step(program: &str, args: &[&str]) -> Step {
        Step {
            program: program.to_string(),
            resolved: PathBuf::from(format!("/usr/bin/{program}")),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            environment: Vec::new(),
            routes: Vec::new(),
        }
    }

    fn plan_of(steps: Steps) -> Plan {
        Plan {
            line: String::new(),
            directory: PathBuf::from("/work"),
            steps,
            writes: Vec::new(),
            reads: Vec::new(),
            stdin: None,
        }
    }

    fn one(program: &str, args: &[&str]) -> Plan {
        plan_of(Steps::Pipeline(vec![step(program, args)]))
    }

    fn remembering(plan: &Plan) -> Remembered {
        let mut record = Remembered::new();
        record.record(RememberedLine::of(plan), "session-1");
        record
    }

    /// RUN-19: the state every directory starts in, and the one a record that cannot be read
    /// degrades to. Nothing is covered, so every run asks.
    #[test]
    fn an_empty_record_covers_nothing() {
        let record = Remembered::new();
        assert!(record.is_empty());
        assert!(!record.covers(&one("make", &["check"])));
    }

    /// RUN-19: the whole point. The same line in a later session runs without a prompt.
    #[test]
    fn the_line_that_was_recorded_is_covered() {
        let line = one("make", &["check"]);
        assert!(remembering(&line).covers(&line));
    }

    /// RUN-19: the arguments are each their own field, and nothing in an entry means "any text".
    /// `make check` says nothing about `make install`, and nothing about `make check -j4`.
    #[test]
    fn no_entry_reaches_a_second_argument_list() {
        let record = remembering(&one("make", &["check"]));
        for other in [
            one("make", &["install"]),
            one("make", &["check", "-j4"]),
            one("make", &[]),
            one("make", &["Check"]),
        ] {
            assert!(
                !record.covers(&other),
                "an entry for `make check` covered {}",
                other.display()
            );
        }
    }

    /// RUN-19: the name is recorded as well as the binary, because a name is what the person read
    /// and a second name for the same binary is a line they have not seen.
    #[test]
    fn a_second_name_for_the_same_binary_is_not_the_line_that_was_read() {
        let record = remembering(&one("make", &["check"]));
        let renamed = plan_of(Steps::Pipeline(vec![Step {
            program: "gmake".to_string(),
            resolved: PathBuf::from("/usr/bin/make"),
            args: vec!["check".to_string()],
            environment: Vec::new(),
            routes: Vec::new(),
        }]));
        assert!(!record.covers(&renamed));
    }

    /// RUN-19: `$PATH` decides what a name means, so an answer must not follow a name onto a
    /// different binary. The same rule RUN-8 makes of a vouch, and it matters more here because
    /// the answer outlives the session.
    #[test]
    fn an_answer_does_not_follow_a_name_onto_a_different_binary() {
        let record = remembering(&one("make", &["check"]));
        let elsewhere = plan_of(Steps::Pipeline(vec![Step {
            program: "make".to_string(),
            resolved: PathBuf::from("/tmp/make"),
            args: vec!["check".to_string()],
            environment: Vec::new(),
            routes: Vec::new(),
        }]));
        assert!(!record.covers(&elsewhere));
    }

    /// RUN-19: an assignment decides what a program loads before its own arguments are looked at,
    /// so it is part of the key. Without it the entry would cover the line the person read with
    /// anything at all put in front of it.
    #[test]
    fn an_environment_assignment_makes_a_different_line() {
        let record = remembering(&one("make", &["check"]));
        let assigned = plan_of(Steps::Pipeline(vec![Step {
            environment: vec![("LD_PRELOAD".to_string(), "/tmp/x.so".to_string())],
            ..step("make", &["check"])
        }]));
        assert!(!record.covers(&assigned));

        let record = remembering(&assigned);
        assert!(record.covers(&assigned), "the assignment was not recorded");
        assert!(
            !record.covers(&one("make", &["check"])),
            "an entry made with an assignment covered the line without it"
        );
    }

    /// RUN-19: where the output went is part of the key, for the reason RUN-6 gives about a
    /// redirection being in neither the program nor the arguments.
    #[test]
    fn sending_the_streams_somewhere_else_makes_a_different_line() {
        let record = remembering(&one("make", &["check"]));
        let merged = plan_of(Steps::Pipeline(vec![Step {
            routes: vec![Route::StderrToStdout],
            ..step("make", &["check"])
        }]));
        assert!(!record.covers(&merged));

        let record = remembering(&merged);
        assert!(
            !record.covers(&one("make", &["check"])),
            "an entry made with the streams merged covered the same program with them apart"
        );
    }

    /// RUN-19: a pipeline records every stage and is covered only where every stage is, the same
    /// requirement RUN-8 makes of a vouch.
    #[test]
    fn a_pipeline_is_covered_only_where_every_stage_is() {
        let line = plan_of(Steps::Pipeline(vec![
            step("git", &["log"]),
            step("head", &["-20"]),
        ]));
        let record = remembering(&line);
        assert!(record.covers(&line));
        for other in [
            plan_of(Steps::Pipeline(vec![step("git", &["log"])])),
            plan_of(Steps::Pipeline(vec![
                step("git", &["log"]),
                step("head", &["-40"]),
            ])),
            plan_of(Steps::Pipeline(vec![
                step("head", &["-20"]),
                step("git", &["log"]),
            ])),
        ] {
            assert!(!record.covers(&other), "{} was covered", other.display());
        }
    }

    /// RUN-19: two lines running the same programs in different arrangements are two lines, so the
    /// joins are recorded and compared rather than flattened away.
    #[test]
    fn how_the_steps_are_joined_is_part_of_the_line() {
        let joined = |joiner| {
            plan_of(Steps::Join {
                left: Box::new(Steps::Pipeline(vec![step("make", &["build"])])),
                joiner,
                right: Box::new(Steps::Pipeline(vec![step("make", &["check"])])),
            })
        };
        let record = remembering(&joined(Joiner::And));
        assert!(record.covers(&joined(Joiner::And)));
        assert!(!record.covers(&joined(Joiner::Or)));
        assert!(!record.covers(&joined(Joiner::Then)));
        assert!(!record.covers(&plan_of(Steps::Pipeline(vec![
            step("make", &["build"]),
            step("make", &["check"]),
        ]))));
    }

    /// RUN-19: the reading back has to say which answers came from an earlier session, so the
    /// entries carry the session the key was pressed in.
    #[test]
    fn an_entry_says_which_session_answered_it() {
        let mut record = Remembered::new();
        record.record(RememberedLine::of(&one("make", &["check"])), "yesterday");
        record.record(RememberedLine::of(&one("cargo", &["test"])), "today");
        let sessions: Vec<&str> = record
            .iter()
            .map(|entry| entry.answered_in.as_str())
            .collect();
        assert_eq!(sessions, ["yesterday", "today"]);
    }
}
