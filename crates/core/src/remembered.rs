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
//! # The one exception: a family with its number free
//!
//! A second answer, for a line the table in [`families`] lists, records the line with one argument
//! left as a number slot: `gh pr view 1081 --repo brave/bravebot` becomes `gh pr view <number>
//! --repo brave/bravebot`. The slot admits a decimal integer and nothing else, every other field is
//! compared as before, and a line the table does not list can only be recorded exact. The table is
//! written by hand and never built from anything a run printed ([RUN-20]).
//!
//! [RUN-20]: ../../../docs/specs/tools/run.md
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
//! # Files
//!
//! The same record holds the files a person agreed, at a write prompt, that a write may create a
//! credential in ([`FileEntry`]). A separate list with a separate key, so no line can cover a file
//! and no file can cover a line.
//!
//! This crate performs no I/O, so where the entries are kept and how they are spelled on disk is
//! `bravebot_agent::remembered`'s.

use crate::command::{Joiner, Plan, Route, Step, Steps, quoted};
use std::path::PathBuf;

/// One argument of a remembered step: a value to be equal to, or a slot a number fills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RememberedArg {
    /// This text and no other.
    Literal(String),
    /// Any decimal integer, spelled with digits only.
    Number,
}

impl RememberedArg {
    /// Whether `seen`, an argument of a step about to run, is what this one admits.
    fn admits(&self, seen: &str) -> bool {
        match self {
            Self::Literal(text) => text == seen,
            Self::Number => is_number(seen),
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Literal(text) => quoted(text),
            // A literal `<number>` draws quoted, because it holds a `>`, so this cannot be spelled
            // by an argument.
            Self::Number => "<number>".to_string(),
        }
    }
}

/// Digits only and small enough to be a number of an issue or a pull request.
///
/// `str::parse` alone would accept a leading `+`, and a leading `-` would read as a flag.
pub fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) && text.parse::<u32>().is_ok()
}

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
    ///
    /// The path, not a rendering of it. [RUN-19] covers a line only while the name still resolves to
    /// the same binary, and two binaries whose names differ only in bytes that are not valid UTF-8
    /// render to one string, so an entry holding a rendering would answer for both. How it is
    /// spelled in the file this record is kept in is [`crate::command::Spelling`].
    ///
    /// [RUN-19]: ../../../docs/specs/tools/run.md
    pub resolved: PathBuf,
    /// The path the step was started by, absolute and with no link in it followed.
    ///
    /// `python3` found through two entries in `$PATH` is one name and one file reached by two links,
    /// and a program can read which of them started it. So this path is held beside the name and
    /// the file.
    pub started_as: PathBuf,
    /// The arguments, in order, each its own value or a number slot.
    pub args: Vec<RememberedArg>,
    /// `NAME=value` written in front of the program, each name and value its own value.
    ///
    /// Part of the key rather than left out of it: an assignment decides what a program loads
    /// before its own arguments are looked at, so an entry without it would cover the line the
    /// person read with anything at all put in front of it.
    ///
    /// Nothing this crate writes fills it, because a line carrying an assignment is asked about
    /// before this record is reached and [`crate::policy::Policy::may_remember`] refuses the key
    /// for one. It is keyed on anyway, for the entries that were not written here: the record is a
    /// file in the home directory, and one arriving with an assignment left out of it covers
    /// nothing rather than covering too much.
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
    ///
    /// Destructured rather than read field by field, so that a field added to [`Step`] stops the
    /// build here instead of being left out of the entry. [`Self::matches`] compares the whole
    /// struct, so what this function copies across is exactly what the key is, and a field it
    /// silently skipped would be one an entry covers without having been answered for ([RUN-8]).
    ///
    /// [RUN-8]: ../../../docs/specs/tools/run.md
    pub fn of(step: &Step) -> Self {
        let Step {
            program,
            resolved,
            started_as,
            args,
            environment,
            routes,
        } = step;
        Self {
            program: program.clone(),
            resolved: resolved.clone(),
            started_as: started_as.clone(),
            args: args.iter().cloned().map(RememberedArg::Literal).collect(),
            environment: environment.clone(),
            routes: routes.clone(),
        }
    }

    /// Whether a step about to run is the one this entry holds.
    ///
    /// Every field [`Self::of`] carried across is part of the question, and the arguments are
    /// compared one for one: a literal by equality, a slot by being a number.
    pub fn matches(&self, step: &Step) -> bool {
        let Step {
            program,
            resolved,
            started_as,
            args,
            environment,
            routes,
        } = step;
        let Self {
            program: held_program,
            resolved: held_resolved,
            started_as: held_started_as,
            args: held_args,
            environment: held_environment,
            routes: held_routes,
        } = self;
        held_program == program
            && held_resolved == resolved
            && held_started_as == started_as
            && held_environment == environment
            && held_routes == routes
            && held_args.len() == args.len()
            && held_args
                .iter()
                .zip(args)
                .all(|(held, seen)| held.admits(seen))
    }

    /// This step with its number left free, where the table in [`families`] lists it.
    ///
    /// `None` for a step that has an assignment or a route, or whose arguments are not exactly the
    /// shape an entry of the table has.
    pub fn with_number_free(step: &Step) -> Option<Self> {
        let slot = families::number_slot(step)?;
        let mut held = Self::of(step);
        held.args[slot] = RememberedArg::Number;
        Some(held)
    }

    /// The step as it was drawn at the prompt.
    ///
    /// The same rendering the prompt used, produced by the same code, because a reading back that
    /// spelled a line differently from the screen the person answered on would be describing
    /// something they have to translate before they can recognise it.
    pub fn display(&self) -> String {
        self.as_step()
            .display_args(self.args.iter().map(RememberedArg::display).collect())
    }

    fn as_step(&self) -> Step {
        Step {
            program: self.program.clone(),
            resolved: self.resolved.clone(),
            started_as: self.started_as.clone(),
            args: Vec::new(),
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

    /// Whether the steps about to run are the ones this shape holds, joined the same way.
    pub fn matches(&self, steps: &Steps) -> bool {
        match (self, steps) {
            (Self::Pipeline(held), Steps::Pipeline(seen)) => {
                held.len() == seen.len()
                    && held.iter().zip(seen).all(|(held, seen)| held.matches(seen))
            }
            (
                Self::Join {
                    left: held_left,
                    joiner: held_joiner,
                    right: held_right,
                },
                Steps::Join {
                    left,
                    joiner,
                    right,
                },
            ) => held_joiner == joiner && held_left.matches(left) && held_right.matches(right),
            (Self::Group(held), Steps::Group(seen)) => held.matches(seen),
            _ => false,
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
        self.steps.matches(&plan.steps)
    }

    /// The line a plan would be recorded as with its number free, where it is a lone step the
    /// table in [`families`] lists. `None` for anything else, which can only be recorded exact.
    pub fn family_of(plan: &Plan) -> Option<Self> {
        let Steps::Pipeline(list) = &plan.steps else {
            return None;
        };
        let [step] = list.as_slice() else {
            return None;
        };
        Some(Self {
            steps: Shape::Pipeline(vec![RememberedStep::with_number_free(step)?]),
        })
    }

    /// Whether this line has a number slot, which is to say it is a family.
    pub fn is_family(&self) -> bool {
        self.steps
            .steps()
            .iter()
            .any(|step| step.args.contains(&RememberedArg::Number))
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

/// One file a person agreed, past the session, that a write may create a credential in
/// ([CRED-13]), and the session whose answer put it there.
///
/// The file is where a write lands, absolute, and is the whole key. Nothing about the value is in
/// it: the fingerprint that would name one is salted per process, and a value to compare against
/// is the credential itself.
///
/// [CRED-13]: ../../../docs/specs/credential-protection.md
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub file: PathBuf,
    /// The session the key was pressed in. Decides nothing, as [`Entry::answered_in`] does not.
    pub answered_in: String,
}

/// Every line remembered for one directory, and every file.
///
/// Read afresh wherever a run prompt would be drawn rather than once at the start, so a line
/// recorded a minute ago in another session is covered by this one. Empty is the ordinary state and
/// means every run asks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Remembered {
    entries: Vec<Entry>,
    files: Vec<FileEntry>,
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

    /// The lines, which is what the run prompt's record is counted in.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no line is remembered. Says nothing of the files.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add a file, as reading one more line of the record does.
    pub fn record_file(&mut self, file: impl Into<PathBuf>, answered_in: impl Into<String>) {
        self.files.push(FileEntry {
            file: file.into(),
            answered_in: answered_in.into(),
        });
    }

    /// Whether some entry is this exact file.
    ///
    /// Equality of the whole path and nothing looser: not a parent directory, not the same name
    /// under another root, and not a rendering of it.
    pub fn covers_file(&self, file: &std::path::Path) -> bool {
        self.files.iter().any(|entry| entry.file == file)
    }

    /// Every file, in the order they were recorded.
    pub fn files(&self) -> impl Iterator<Item = &FileEntry> {
        self.files.iter()
    }
}

impl FromIterator<Entry> for Remembered {
    fn from_iter<I: IntoIterator<Item = Entry>>(entries: I) -> Self {
        Self {
            entries: entries.into_iter().collect(),
            files: Vec::new(),
        }
    }
}

/// The sub-commands whose number a person may leave free when they ask for a line to be remembered
/// ([RUN-20]).
///
/// Written by hand, one entry at a time, as the `git` options in [CMDLINE-8] are, and never built
/// from anything a run printed. An entry says: this program, these two words, then one decimal
/// integer, then `--repo OWNER/REPO`, and nothing else. The number is the only thing left free,
/// and the repository stays a literal in the entry. Any flag the entry does not name makes the
/// line one the table does not list, and it can only be recorded exact.
///
/// Each entry was checked against the program's own `--help` to be a read of one object that takes
/// no path to write to and reads no file named on the line.
///
/// [RUN-20]: ../../../docs/specs/tools/run.md
/// [CMDLINE-8]: ../../../docs/specs/tools/command-line.md
pub mod families {
    use crate::command::Step;

    /// The program name and the two words after it.
    const TABLE: &[(&str, [&str; 2])] = &[
        ("gh", ["pr", "view"]),
        ("gh", ["pr", "diff"]),
        ("gh", ["pr", "checks"]),
        ("gh", ["issue", "view"]),
    ];

    /// Where the number sits in the argument list of every entry.
    const SLOT: usize = 2;

    /// The index of the argument that may be left free, where `step` is a line the table lists.
    pub fn number_slot(step: &Step) -> Option<usize> {
        let Step {
            program,
            resolved: _,
            started_as: _,
            args,
            environment,
            routes,
        } = step;
        if !environment.is_empty() || !routes.is_empty() {
            return None;
        }
        let [first, second, number, flag, repo] = args.as_slice() else {
            return None;
        };
        let listed = TABLE
            .iter()
            .any(|(name, words)| name == program && words[0] == first && words[1] == second);
        (listed && super::is_number(number) && flag == "--repo" && is_repository(repo))
            .then_some(SLOT)
    }

    /// `OWNER/REPO` and no other spelling: no host, no leading dash or dot, nothing a shell or a flag
    /// parser reads as more than a name.
    fn is_repository(text: &str) -> bool {
        let Some((owner, name)) = text.split_once('/') else {
            return false;
        };
        let part = |part: &str| {
            !part.is_empty()
                && !part.starts_with(['-', '.'])
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        };
        part(owner) && part(name)
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
            started_as: PathBuf::from(format!("/usr/bin/{program}")),
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
            started_as: PathBuf::from("/usr/bin/gmake"),
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
            started_as: PathBuf::from("/tmp/make"),
            args: vec!["check".to_string()],
            environment: Vec::new(),
            routes: Vec::new(),
        }]));
        assert!(!record.covers(&elsewhere));
    }

    /// RUN-19: nor onto another path that starts the same binary, under the same name.
    #[test]
    fn an_answer_does_not_follow_a_binary_onto_another_path_it_is_started_by() {
        let record = remembering(&one("make", &["check"]));
        let through_a_link = plan_of(Steps::Pipeline(vec![Step {
            started_as: PathBuf::from("/opt/tools/make"),
            ..step("make", &["check"])
        }]));
        assert!(!record.covers(&through_a_link));
    }

    /// RUN-19: onto the binary's own bytes. `to_string_lossy` maps every byte that is not valid
    /// UTF-8 onto one replacement character, so an entry holding a rendering answered for every
    /// binary whose name renders that way, and this answer outlives the session that gave it.
    #[cfg(unix)]
    #[test]
    fn an_answer_does_not_follow_a_rendering_onto_a_different_binary() {
        let resolving_to = |last: u8| {
            use std::os::unix::ffi::OsStrExt;
            let mut bytes = b"/work/make-".to_vec();
            bytes.push(last);
            plan_of(Steps::Pipeline(vec![Step {
                resolved: PathBuf::from(std::ffi::OsStr::from_bytes(&bytes)),
                ..step("make", &["check"])
            }]))
        };
        let record = remembering(&resolving_to(0xff));
        assert!(
            record.covers(&resolving_to(0xff)),
            "the line that was recorded was not covered"
        );
        assert!(
            !record.covers(&resolving_to(0xfe)),
            "an answer about one binary covered another that renders the same way"
        );
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

    /// CRED-13: a file entry covers that file and nothing near it, and the two lists never answer
    /// for each other.
    #[test]
    fn a_file_entry_covers_that_file_only() {
        let mut record = Remembered::new();
        record.record_file("/work/config/master.key", "today");
        assert!(record.covers_file(std::path::Path::new("/work/config/master.key")));
        for other in [
            "/work/config",
            "/work/config/master.key.bak",
            "/other/config/master.key",
            "config/master.key",
        ] {
            assert!(
                !record.covers_file(std::path::Path::new(other)),
                "{other} was covered"
            );
        }
        assert!(record.is_empty(), "a file entry was counted as a line");
        assert!(!record.covers(&one("cat", &["/work/config/master.key"])));

        let lines = remembering(&one("make", &["check"]));
        assert!(!lines.covers_file(std::path::Path::new("/usr/bin/make")));
    }

    fn gh(args: &[&str]) -> Plan {
        one("gh", args)
    }

    fn view(number: &str) -> Plan {
        gh(&["pr", "view", number, "--repo", "brave/bravebot"])
    }

    fn family(plan: &Plan) -> Remembered {
        let mut record = Remembered::new();
        record.record(
            RememberedLine::family_of(plan).expect("the table lists this line"),
            "session-1",
        );
        record
    }

    /// RUN-20: the exception. A family recorded from one number covers the same sub-command on the
    /// same repository with any other.
    #[test]
    fn a_family_covers_the_same_sub_command_with_another_number() {
        let record = family(&view("1081"));
        assert!(record.covers(&view("1081")));
        assert!(record.covers(&view("1082")));
        assert!(record.covers(&view("7")));
    }

    /// RUN-20: the number is the only thing free. Another repository, another sub-command, another
    /// program, an extra flag and a missing repository are each a line the person has not answered.
    #[test]
    fn a_family_frees_the_number_and_nothing_else() {
        let record = family(&view("1081"));
        for other in [
            gh(&["pr", "view", "1082", "--repo", "other/repo"]),
            gh(&["pr", "diff", "1082", "--repo", "brave/bravebot"]),
            gh(&["issue", "view", "1082", "--repo", "brave/bravebot"]),
            gh(&["pr", "view", "1082", "--repo", "brave/bravebot", "--web"]),
            gh(&["pr", "view", "1082", "--web", "--repo", "brave/bravebot"]),
            gh(&["pr", "view", "--repo", "brave/bravebot", "1082"]),
            gh(&["pr", "view", "1082"]),
            gh(&["pr", "view", "--repo", "brave/bravebot"]),
            gh(&["pr", "view", "1082", "1083", "--repo", "brave/bravebot"]),
            one("hub", &["pr", "view", "1082", "--repo", "brave/bravebot"]),
        ] {
            assert!(!record.covers(&other), "{} was covered", other.line);
        }
    }

    /// RUN-20: what fills the slot is a decimal integer. Text, a signed number, a number with
    /// something after it, an empty argument and a number too large for either are asked about.
    #[test]
    fn the_slot_admits_a_decimal_integer_and_no_other_argument() {
        let record = family(&view("1081"));
        for other in [
            "abc",
            "",
            "-1",
            "+1",
            "1e3",
            "0x10",
            "12 ",
            " 12",
            "1,2",
            "1.5",
            "１２",
            "99999999999999999999",
            "feature/branch",
            "https://github.com/brave/bravebot/pull/1",
        ] {
            assert!(!record.covers(&view(other)), "{other:?} filled the slot");
        }
    }

    /// RUN-20: an exact entry is still exact. Recording `gh pr view 1081 ...` with `r` does not
    /// cover another number.
    #[test]
    fn an_exact_entry_for_a_listed_line_does_not_free_its_number() {
        let record = remembering(&view("1081"));
        assert!(record.covers(&view("1081")));
        assert!(!record.covers(&view("1082")));
    }

    /// RUN-20: a line the table does not list has no family, however much it looks like one.
    #[test]
    fn a_line_the_table_does_not_list_has_no_family() {
        let mut with_assignment = step("gh", &["pr", "view", "1", "--repo", "brave/bravebot"]);
        with_assignment.environment = vec![("GH_HOST".to_string(), "example.com".to_string())];
        let mut with_route = step("gh", &["pr", "view", "1", "--repo", "brave/bravebot"]);
        with_route.routes = vec![Route::StderrToStdout];
        let joined = plan_of(Steps::Join {
            left: Box::new(Steps::Pipeline(vec![step(
                "gh",
                &["pr", "view", "1", "--repo", "brave/bravebot"],
            )])),
            joiner: Joiner::And,
            right: Box::new(Steps::Pipeline(vec![step("make", &["check"])])),
        });
        let piped = plan_of(Steps::Pipeline(vec![
            step("gh", &["pr", "view", "1", "--repo", "brave/bravebot"]),
            step("cat", &[]),
        ]));
        let grouped = plan_of(Steps::Group(Box::new(Steps::Pipeline(vec![step(
            "gh",
            &["pr", "view", "1", "--repo", "brave/bravebot"],
        )]))));
        for plan in [
            plan_of(Steps::Pipeline(vec![with_assignment])),
            plan_of(Steps::Pipeline(vec![with_route])),
            joined,
            piped,
            grouped,
            gh(&["pr", "view", "1", "--repo=brave/bravebot"]),
            gh(&["pr", "view", "1", "-R", "brave/bravebot"]),
            gh(&["pr", "view", "1", "--repo", "github.com/brave/bravebot"]),
            gh(&["pr", "view", "1", "--repo", "-x/bravebot"]),
            gh(&["pr", "view", "1", "--repo", "../bravebot"]),
            gh(&["pr", "view", "x", "--repo", "brave/bravebot"]),
            gh(&["pr", "merge", "1", "--repo", "brave/bravebot"]),
            gh(&["pr", "close", "1", "--repo", "brave/bravebot"]),
            one("make", &["check"]),
        ] {
            assert!(
                RememberedLine::family_of(&plan).is_none(),
                "{} has a family",
                plan.line
            );
        }
    }

    /// RUN-20: the name still has to resolve to the binary the person answered for.
    #[test]
    fn a_family_is_bound_to_the_binary_it_was_recorded_for() {
        let record = family(&view("1081"));
        let mut elsewhere = step("gh", &["pr", "view", "1082", "--repo", "brave/bravebot"]);
        elsewhere.resolved = PathBuf::from("/tmp/gh");
        assert!(!record.covers(&plan_of(Steps::Pipeline(vec![elsewhere]))));
    }

    /// RUN-20: the reading back draws the slot, so a family is not mistaken for the line.
    #[test]
    fn a_family_is_drawn_with_its_number_free() {
        let line = RememberedLine::family_of(&view("1081")).unwrap();
        assert!(line.is_family());
        assert_eq!(
            line.display(),
            "/usr/bin/gh pr view <number> --repo brave/bravebot"
        );
        assert!(!RememberedLine::of(&view("1081")).is_family());
    }
}
