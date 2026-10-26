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
//! # The one exception: a scope
//!
//! A second answer, for a line the table in [`scopes`] lists, records a named group of lines: the
//! sub-commands the table puts in the group, each with one argument left as a number slot, on the
//! one repository the line named. `gh pr view 1081 --repo brave/bravebot` becomes `gh pr view
//! <number> --repo brave/bravebot` and the same for `gh pr diff`, `gh pr checks` and `gh issue
//! view`. The slot admits a decimal integer and nothing else, every other field is compared as
//! before, and a line the table does not list can only be recorded exact. The table is written by
//! hand and never built from anything a run printed ([RUN-20]).
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

/// A named group of lines a person answered for with one key ([RUN-20]).
///
/// What the table in [`scopes`] made of the line on the screen: each sub-command the table puts in
/// the group, as an ordinary line with its number free. Held as those lines and nothing smaller, so
/// what a later line is covered by is exactly what the record spells out, and a table that grows
/// later does not widen an answer already given.
///
/// [RUN-20]: ../../../docs/specs/tools/run.md
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub kind: scopes::Kind,
    /// The repository every line names, as the prompt wrote it. Said where the scope is read back
    /// and decides nothing: what covers a line is the lines.
    pub repository: String,
    pub lines: Vec<RememberedLine>,
}

impl Scope {
    /// Whether a plan about to run is one of the lines this scope holds.
    pub fn covers(&self, plan: &Plan) -> bool {
        self.lines.iter().any(|line| line.covers(plan))
    }
}

/// One scope, and the session whose answer put it there.
///
/// The session decides nothing, as [`Entry::answered_in`] does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeEntry {
    pub scope: Scope,
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
    scopes: Vec<ScopeEntry>,
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

    /// Add a scope, as reading one more record of the file does.
    pub fn record_scope(&mut self, scope: Scope, answered_in: impl Into<String>) {
        self.scopes.push(ScopeEntry {
            scope,
            answered_in: answered_in.into(),
        });
    }

    /// Whether some entry is this exact plan, or some scope holds a line that is.
    ///
    /// Answers whether to ask and nothing else. A caller that has not already refused a line
    /// releasing private data, naming a file to write, or running outside the root must not reach
    /// this: those are asked about whatever is recorded, and an entry cannot speak for them.
    pub fn covers(&self, plan: &Plan) -> bool {
        self.entries.iter().any(|entry| entry.line.covers(plan))
            || self.scopes.iter().any(|entry| entry.scope.covers(plan))
    }

    /// Every entry, in the order they were recorded. A line a scope holds is not one of them: it
    /// is read back through [`Self::scopes`], as the one item it was answered as.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// Every scope, in the order they were recorded.
    pub fn scopes(&self) -> impl Iterator<Item = &ScopeEntry> {
        self.scopes.iter()
    }

    /// The answers, which is what the run prompt's record is counted in: one for a line and one for
    /// a scope, however many lines the scope holds.
    pub fn len(&self) -> usize {
        self.entries.len() + self.scopes.len()
    }

    /// Whether no line or scope is remembered. Says nothing of the files.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.scopes.is_empty()
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
            scopes: Vec::new(),
            files: Vec::new(),
        }
    }
}

/// The groups of sub-commands a person may answer for with one key ([RUN-20]).
///
/// Written by hand, one entry at a time, as the `git` options in [CMDLINE-8] are, and never built
/// from anything a run printed. An entry says: this group, this program, these two words, then one
/// decimal integer, then `--repo OWNER/REPO`, and nothing else. The number is the only thing left
/// free, and the repository stays a literal in each line the scope expands to. Any flag the entry
/// does not name makes the line one the table does not list, and it can only be recorded exact.
///
/// Each entry was checked against the program's own `--help` to be a read of one object that takes
/// no path to write to and reads no file named on the line.
///
/// The sentences the prompt puts to the person for a group (`run-remember-scope-*` in the message
/// catalog) say which sub-commands it holds, so a change to a group changes them too.
///
/// [RUN-20]: ../../../docs/specs/tools/run.md
/// [CMDLINE-8]: ../../../docs/specs/tools/command-line.md
pub mod scopes {
    use super::{RememberedArg, RememberedLine, RememberedStep, Scope, Shape, is_number};
    use crate::command::{Plan, Step, Steps};

    /// A group of the table.
    ///
    /// A name for the words the prompt and the reading back put to it, which live with the
    /// interface that draws them and not here.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Kind {
        /// Viewing a pull request or an issue, and reading a pull request's diff and checks, on one
        /// repository.
        ReadPullRequestsAndIssues,
    }

    /// One sub-command of a group.
    struct Member {
        kind: Kind,
        /// The program name and the two words after it.
        program: &'static str,
        words: [&'static str; 2],
    }

    const TABLE: &[Member] = &[
        Member {
            kind: Kind::ReadPullRequestsAndIssues,
            program: "gh",
            words: ["pr", "view"],
        },
        Member {
            kind: Kind::ReadPullRequestsAndIssues,
            program: "gh",
            words: ["pr", "diff"],
        },
        Member {
            kind: Kind::ReadPullRequestsAndIssues,
            program: "gh",
            words: ["pr", "checks"],
        },
        Member {
            kind: Kind::ReadPullRequestsAndIssues,
            program: "gh",
            words: ["issue", "view"],
        },
    ];

    /// The scope a plan is in, with every line it expands to, where the plan is a line the table
    /// lists.
    ///
    /// `None` for a plan that is not one lone step, for a step that has an assignment or a route,
    /// and for one whose arguments are not exactly the shape of a member of the table. The binary
    /// every expanded line names is the one the line on the screen resolved to: the person read
    /// `gh`, and the group is the other sub-commands of that `gh`.
    pub fn offered_for(plan: &Plan) -> Option<Scope> {
        let Steps::Pipeline(list) = &plan.steps else {
            return None;
        };
        let [step] = list.as_slice() else {
            return None;
        };
        let Step {
            program,
            resolved,
            started_as,
            args,
            environment,
            routes,
        } = step;
        if !environment.is_empty() || !routes.is_empty() {
            return None;
        }
        let [first, second, number, flag, repository] = args.as_slice() else {
            return None;
        };
        let listed = TABLE
            .iter()
            .find(|member| {
                member.program == program && member.words[0] == first && member.words[1] == second
            })?
            .kind;
        if !(is_number(number) && flag == "--repo" && is_repository(repository)) {
            return None;
        }
        let lines = TABLE
            .iter()
            .filter(|member| member.kind == listed && member.program == program)
            .map(|member| RememberedLine {
                steps: Shape::Pipeline(vec![RememberedStep {
                    program: program.clone(),
                    resolved: resolved.clone(),
                    started_as: started_as.clone(),
                    args: vec![
                        RememberedArg::Literal(member.words[0].to_string()),
                        RememberedArg::Literal(member.words[1].to_string()),
                        RememberedArg::Number,
                        RememberedArg::Literal(flag.clone()),
                        RememberedArg::Literal(repository.clone()),
                    ],
                    environment: Vec::new(),
                    routes: Vec::new(),
                }]),
            })
            .collect();
        Some(Scope {
            kind: listed,
            repository: repository.clone(),
            lines,
        })
    }

    /// Whether a scope read back from the record is what its name says: at least one line, and every
    /// line a sub-command the table puts in its group, with its number free, on its repository and
    /// nothing else.
    ///
    /// The lines are what covers, and the name and the repository are what the reading back says
    /// about them, so an entry whose lines are something else is read back as a grant it is not.
    /// It covers nothing instead. A group holding fewer sub-commands than the table lists now is
    /// whole: it is what the person answered for when a build with the smaller table asked.
    pub fn holds_only_its_group(scope: &Scope) -> bool {
        is_repository(&scope.repository)
            && !scope.lines.is_empty()
            && scope.lines.iter().all(|line| {
                let Shape::Pipeline(steps) = &line.steps else {
                    return false;
                };
                let [step] = steps.as_slice() else {
                    return false;
                };
                let RememberedStep {
                    program,
                    resolved: _,
                    started_as: _,
                    args,
                    environment,
                    routes,
                } = step;
                let [
                    RememberedArg::Literal(first),
                    RememberedArg::Literal(second),
                    RememberedArg::Number,
                    RememberedArg::Literal(flag),
                    RememberedArg::Literal(repository),
                ] = args.as_slice()
                else {
                    return false;
                };
                environment.is_empty()
                    && routes.is_empty()
                    && flag == "--repo"
                    && *repository == scope.repository
                    && TABLE.iter().any(|member| {
                        member.kind == scope.kind
                            && member.program == program
                            && member.words[0] == first
                            && member.words[1] == second
                    })
            })
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

    /// The four lines the table lists, with `number` in the slot, on `repository`.
    fn listed(number: &str, repository: &str) -> [Plan; 4] {
        [
            gh(&["pr", "view", number, "--repo", repository]),
            gh(&["pr", "diff", number, "--repo", repository]),
            gh(&["pr", "checks", number, "--repo", repository]),
            gh(&["issue", "view", number, "--repo", repository]),
        ]
    }

    fn scope(plan: &Plan) -> Remembered {
        let mut record = Remembered::new();
        record.record_scope(
            scopes::offered_for(plan).expect("the table lists this line"),
            "session-1",
        );
        record
    }

    /// RUN-20: the exception. A scope recorded from one line covers each sub-command of its group on
    /// the same repository with any number, which is more than the sub-command that was on the
    /// screen.
    #[test]
    fn a_scope_covers_every_sub_command_of_its_group_with_any_number() {
        let record = scope(&view("1081"));
        for number in ["1081", "1082", "7"] {
            for plan in listed(number, "brave/bravebot") {
                assert!(record.covers(&plan), "{} was asked about", plan.line);
            }
        }
    }

    /// RUN-20: the same group whichever of its sub-commands was on the screen, so the answer does
    /// not depend on which one the model happened to ask for first.
    #[test]
    fn the_scope_is_the_same_whichever_sub_command_was_asked_about() {
        let from_view = scopes::offered_for(&view("1081")).expect("listed");
        assert_eq!(from_view.lines.len(), 4);
        for plan in listed("1", "brave/bravebot") {
            assert_eq!(scopes::offered_for(&plan).as_ref(), Some(&from_view));
        }
    }

    /// RUN-20: the number is the only thing free. Another repository, a sub-command that writes, a
    /// sub-command that lists, another program, an extra flag and a missing repository are each a
    /// line the person has not answered.
    #[test]
    fn a_scope_frees_the_number_and_nothing_else() {
        let record = scope(&view("1081"));
        for other in [
            gh(&["pr", "view", "1082", "--repo", "other/repo"]),
            gh(&["issue", "view", "1082", "--repo", "other/repo"]),
            gh(&["pr", "merge", "1082", "--repo", "brave/bravebot"]),
            gh(&["pr", "close", "1082", "--repo", "brave/bravebot"]),
            gh(&["pr", "comment", "1082", "--repo", "brave/bravebot"]),
            gh(&["issue", "close", "1082", "--repo", "brave/bravebot"]),
            gh(&["issue", "list", "--repo", "brave/bravebot"]),
            gh(&["pr", "list", "--repo", "brave/bravebot"]),
            gh(&["pr", "view", "1082", "--repo", "brave/bravebot", "--web"]),
            gh(&["pr", "view", "1082", "--web", "--repo", "brave/bravebot"]),
            gh(&["pr", "view", "--repo", "brave/bravebot", "1082"]),
            gh(&["pr", "view", "1082"]),
            gh(&["pr", "view", "--repo", "brave/bravebot"]),
            gh(&["pr", "view", "1082", "1083", "--repo", "brave/bravebot"]),
            gh(&[
                "issue",
                "view",
                "1082",
                "--repo",
                "brave/bravebot",
                "--json",
                "body",
            ]),
            one("hub", &["pr", "view", "1082", "--repo", "brave/bravebot"]),
        ] {
            assert!(!record.covers(&other), "{} was covered", other.line);
        }
    }

    /// RUN-20: what fills the slot is a decimal integer, in every sub-command of the group. Text, a
    /// signed number, a number with something after it, an empty argument and a number too large for
    /// either are asked about.
    #[test]
    fn the_slot_admits_a_decimal_integer_and_no_other_argument() {
        let record = scope(&view("1081"));
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
            for plan in listed(other, "brave/bravebot") {
                assert!(
                    !record.covers(&plan),
                    "{:?} filled the slot: {}",
                    other,
                    plan.line
                );
            }
        }
    }

    /// RUN-20: an exact entry is still exact. Recording `gh pr view 1081 ...` with `r` does not
    /// cover another number, or another sub-command.
    #[test]
    fn an_exact_entry_for_a_listed_line_does_not_free_its_number() {
        let record = remembering(&view("1081"));
        assert!(record.covers(&view("1081")));
        assert!(!record.covers(&view("1082")));
        assert!(!record.covers(&listed("1081", "brave/bravebot")[1]));
    }

    /// RUN-20: a line the table does not list is in no scope, however much it looks like one.
    #[test]
    fn a_line_the_table_does_not_list_is_in_no_scope() {
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
            gh(&["issue", "list", "--repo", "brave/bravebot"]),
            one("make", &["check"]),
        ] {
            assert!(
                scopes::offered_for(&plan).is_none(),
                "{} is in a scope",
                plan.line
            );
        }
    }

    /// RUN-20: every sub-command of the group still has to resolve to the binary the person
    /// answered for, so a `gh` found elsewhere asks.
    #[test]
    fn a_scope_is_bound_to_the_binary_it_was_recorded_for() {
        let record = scope(&view("1081"));
        for sub_command in [
            ["pr", "view"],
            ["pr", "diff"],
            ["pr", "checks"],
            ["issue", "view"],
        ] {
            let [first, second] = sub_command;
            let mut elsewhere = step("gh", &[first, second, "1082", "--repo", "brave/bravebot"]);
            elsewhere.resolved = PathBuf::from("/tmp/gh");
            assert!(
                !record.covers(&plan_of(Steps::Pipeline(vec![elsewhere]))),
                "{first} {second} followed the name onto another binary"
            );
        }
    }

    /// RUN-20: a scope is one answer however many lines it holds, so the reading back lists it as
    /// one item and the lines it holds are not also listed as lines of their own.
    #[test]
    fn a_scope_is_one_answer_however_many_lines_it_holds() {
        let record = scope(&view("1081"));
        assert_eq!(record.len(), 1);
        assert!(!record.is_empty());
        assert_eq!(
            record.iter().count(),
            0,
            "a scope's lines were listed alone"
        );
        let [held] = record.scopes().collect::<Vec<_>>()[..] else {
            panic!(
                "the record holds {} scopes, not one",
                record.scopes().count()
            );
        };
        assert_eq!(held.answered_in, "session-1");
        assert_eq!(held.scope.repository, "brave/bravebot");
        assert_eq!(held.scope.lines.len(), 4);
    }

    /// RUN-20: the reading back draws the slot, so a line of a scope is not mistaken for the line
    /// that was answered.
    #[test]
    fn a_scope_draws_each_line_with_its_number_free() {
        let held = scopes::offered_for(&view("1081")).unwrap();
        let drawn: Vec<String> = held.lines.iter().map(RememberedLine::display).collect();
        assert_eq!(
            drawn,
            [
                "/usr/bin/gh pr view <number> --repo brave/bravebot",
                "/usr/bin/gh pr diff <number> --repo brave/bravebot",
                "/usr/bin/gh pr checks <number> --repo brave/bravebot",
                "/usr/bin/gh issue view <number> --repo brave/bravebot",
            ]
        );
        assert_eq!(
            RememberedLine::of(&view("1081")).display(),
            "/usr/bin/gh pr view 1081 --repo brave/bravebot"
        );
    }

    /// RUN-2: the reading back is drawn by the code that draws the plan, so a line started through
    /// a link reads back as the link and the file it leads to, and not as the file alone.
    #[test]
    fn a_line_started_through_a_link_is_read_back_as_the_plan_draws_it() {
        let mut cargo = step("cargo", &["test"]);
        cargo.started_as = PathBuf::from("/home/me/.cargo/bin/cargo");
        cargo.resolved = PathBuf::from("/home/me/.cargo/bin/rustup");
        let plan = plan_of(Steps::Pipeline(vec![cargo]));
        assert_eq!(
            RememberedLine::of(&plan).display(),
            "/home/me/.cargo/bin/cargo -> /home/me/.cargo/bin/rustup test"
        );
        assert_eq!(RememberedLine::of(&plan).display(), plan.display());
    }
}
