// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What a session can reach, and whether a device approved for one scope may keep seeing another.
//!
//! A device is approved for a ceiling when it is paired (decision 18 in
//! `docs/design/mobile/decisions-and-tradeoffs.md`). A change inside the ceiling needs nothing,
//! a change past it suspends the device, and a change that narrows never does. [`Scope::covers`]
//! is that comparison and decides nothing else.
//!
//! # Fails closed
//!
//! Every field is compared by a rule that can show the current value is no wider than the
//! ceiling's. A change the rule cannot show to be narrower counts as wider, so a new spelling of an
//! old grant suspends the device and never lets it keep more than it was approved for. A field with
//! no total order, the permission rules and the lists, has a rule of its own below.
//!
//! [`Scope::covers`] names every field of the ceiling with no `..`, so a field added to [`Scope`]
//! stops the build there. That is where to give it a [`Field`], a flag, a rule in
//! `spec::field_widened` and a mutation; the build does not check that the rule exists.
//!
//! # What the strings are
//!
//! Each entry is a key from something the person decided or wrote: a path they trusted or opened,
//! a command they vouched for, a rule or a server from their settings. The comparison is equality
//! of those keys. It reads no file and no content a turn observed, and its answer is a reason to
//! hold something back, never a reason to release it.

use crate::compare::same_text;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// How much of the machine a confined program reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Reads {
    /// Only what the program's profile lists.
    Listed,
    /// The machine, less the credential table.
    Machine,
    /// Everything: no confinement.
    Unconfined,
}

/// What a confined program writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Writes {
    /// Nothing in the session's directories.
    Nothing,
    /// The session's directories.
    Session,
    /// Anything: no confinement.
    Unconfined,
}

/// How much a program `run` starts is held to. Two axes, because the sandbox modes are not a line:
/// `strict` reads less than `read-only` and writes more, so neither is the narrower of the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub struct Confinement {
    pub reads: Reads,
    pub writes: Writes,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Reads {
    #[cfg_attr(verus_keep_ghost, verus_spec(r =>
        ensures
            r as int == crate::spec::reads_rank(self),
    ))]
    fn rank(self) -> u8 {
        if self == Self::Listed {
            0
        } else if self == Self::Machine {
            1
        } else {
            2
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Writes {
    #[cfg_attr(verus_keep_ghost, verus_spec(r =>
        ensures
            r as int == crate::spec::writes_rank(self),
    ))]
    fn rank(self) -> u8 {
        if self == Self::Nothing {
            0
        } else if self == Self::Session {
            1
        } else {
            2
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Approvals {
    #[cfg_attr(verus_keep_ghost, verus_spec(r =>
        ensures
            r as int == crate::spec::approvals_rank(self),
    ))]
    fn rank(self) -> u8 {
        if self == Self::Plan {
            0
        } else if self == Self::Ask {
            1
        } else if self == Self::AcceptEdits {
            2
        } else if self == Self::Auto {
            3
        } else {
            4
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Confinement {
    /// Whether a program held to `self` can read no more and write no more than one held to `than`.
    #[cfg_attr(verus_keep_ghost, verus_spec(r =>
        ensures
            r == crate::spec::confinement_within(self, than),
    ))]
    fn is_no_looser_than(self, than: Self) -> bool {
        self.reads.rank() <= than.reads.rank() && self.writes.rank() <= than.writes.rank()
    }
}

/// How much a session does without asking, from the least to the most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Approvals {
    /// Nothing is written.
    Plan,
    /// Every effect is put to the person.
    Ask,
    /// Writes go through unasked.
    AcceptEdits,
    /// Writes and the runs the sandbox confines go through on a classifier's word.
    Auto,
    /// Nothing is asked.
    Bypass,
}

/// What a session can reach, as the keys of the sources that bound it.
///
/// Each list holds distinct keys, in no order.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    verus_keep_ghost,
    verus_verify,
    verifier::allow(autoderive_clone_without_spec)
)]
pub struct Scope {
    /// The project directory. A session in another project is a different scope.
    pub project: String,
    /// The directories open beside the project.
    pub directories: Vec<String>,
    /// The subtrees the trust map trusts.
    pub trusted: Vec<String>,
    /// The subtrees the trust map distrusts, which only narrow.
    pub distrusted: Vec<String>,
    /// The subtrees the trust map leaves undecided. Undecided overrides a broader decision, so
    /// one inside a distrusted subtree lifts the distrust.
    pub undecided: Vec<String>,
    /// The commands a person vouched for.
    pub programs: Vec<String>,
    /// The files a person agreed the planner may be given despite the credential scan.
    pub exposed: Vec<String>,
    /// The servers the settings ask to start.
    pub mcp_servers: Vec<String>,
    /// The directories the settings name as references to read, by the path as written.
    pub references: Vec<String>,
    /// The directories the settings name to read skills from, as written.
    pub skill_directories: Vec<String>,
    /// The checkouts the session made and keeps, whose directories each turn's workspace holds.
    pub checkouts: Vec<String>,
    /// The programs of the language servers the session has started, which run with the person's
    /// own access.
    pub language_servers: Vec<String>,
    /// The tools of started servers a person told the session to stop asking about in this
    /// project, as `alias:tool`.
    pub standing_answers: Vec<String>,
    /// The `allow` permission rules.
    pub allow: Vec<String>,
    /// The `ask` permission rules.
    pub ask: Vec<String>,
    /// The `deny` permission rules.
    pub deny: Vec<String>,
    /// How much a program `run` starts is held to.
    pub confinement: Confinement,
    /// How much the session does without asking.
    pub approvals: Approvals,
    /// Whether the file tools are held to the project.
    pub reads_stay_in_workspace: bool,
    /// Whether a safe verdict releases quarantined content with nobody asked. Compared by equality:
    /// turning it on releases content unasked in every mode but bypass, and narrows bypass, which
    /// rejects what a check calls unsafe only while it is on.
    pub auto_vetting: bool,
    /// Whether confined `run` lines start without asking, which the person's yes to trusting the
    /// directory said.
    pub confined_lines: bool,
}

/// A field of [`Scope`], named for what widened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Field {
    Project,
    Directories,
    /// A subtree trusted that the ceiling did not trust.
    Trusted,
    /// A distrust of the ceiling's that is gone.
    Distrusted,
    /// An undecided boundary of the ceiling's that is gone, or one the ceiling did not hold.
    Undecided,
    Programs,
    Exposed,
    McpServers,
    References,
    SkillDirectories,
    Checkouts,
    LanguageServers,
    StandingAnswers,
    /// An `allow` rule the ceiling did not hold.
    Allow,
    /// An `ask` rule of the ceiling's that is gone.
    Ask,
    /// A `deny` rule of the ceiling's that is gone.
    Deny,
    Confinement,
    Approvals,
    ReadsStayInWorkspace,
    AutoVetting,
    ConfinedLines,
}

impl Field {
    /// Every field, in the order of [`Scope`].
    pub const ALL: [Self; 21] = [
        Self::Project,
        Self::Directories,
        Self::Trusted,
        Self::Distrusted,
        Self::Undecided,
        Self::Programs,
        Self::Exposed,
        Self::McpServers,
        Self::References,
        Self::SkillDirectories,
        Self::Checkouts,
        Self::LanguageServers,
        Self::StandingAnswers,
        Self::Allow,
        Self::Ask,
        Self::Deny,
        Self::Confinement,
        Self::Approvals,
        Self::ReadsStayInWorkspace,
        Self::AutoVetting,
        Self::ConfinedLines,
    ];

    /// What a person reads when this is what went past the ceiling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Project => "the project directory",
            Self::Directories => "the open directories",
            Self::Trusted => "the trusted paths",
            Self::Distrusted => "the distrusted paths",
            Self::Undecided => "the undecided paths",
            Self::Programs => "the programs vouched for",
            Self::Exposed => "the files agreed to be shown",
            Self::McpServers => "the servers asked for",
            Self::References => "the references",
            Self::SkillDirectories => "the skill directories",
            Self::Checkouts => "the checkouts kept",
            Self::LanguageServers => "the language servers started",
            Self::StandingAnswers => "the tools no longer asked about",
            Self::Allow => "the allow rules",
            Self::Ask => "the ask rules",
            Self::Deny => "the deny rules",
            Self::Confinement => "the sandbox mode",
            Self::Approvals => "the permission mode",
            Self::ReadsStayInWorkspace => "whether reads stay in the workspace",
            Self::AutoVetting => "automatic vetting",
            Self::ConfinedLines => "whether confined run lines start unasked",
        }
    }
}

/// Whether a scope is inside a ceiling, and if not, which fields went past it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    verus_keep_ghost,
    verus_verify,
    verifier::allow(autoderive_clone_without_spec)
)]
pub enum Coverage {
    /// Nothing went past the ceiling.
    Covered,
    /// These fields did, in the order of [`Scope`].
    Widened(Vec<Field>),
}

impl Coverage {
    pub fn is_covered(&self) -> bool {
        matches!(self, Self::Covered)
    }
}

/// Whether `key` is one of `have`.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == crate::spec::holds(have@, key@),
))]
fn holds(have: &[String], key: &str) -> bool {
    let mut at = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            at <= have.len(),
            forall|i: int| 0 <= i < at ==> (#[trigger] have@[i])@ != key@,
        decreases have.len() - at,
    ))]
    while at < have.len() {
        if same_text(&have[at], key) {
            return true;
        }
        at += 1;
    }
    false
}

/// Whether every key in `wanted` is in `have`.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == crate::spec::holds_all(have@, wanted@),
))]
fn holds_all(have: &[String], wanted: &[String]) -> bool {
    let mut at = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            at <= wanted.len(),
            forall|j: int| 0 <= j < at ==> crate::spec::holds(have@, (#[trigger] wanted@[j])@),
        decreases wanted.len() - at,
    ))]
    while at < wanted.len() {
        if !holds(have, &wanted[at]) {
            #[cfg(verus_keep_ghost)]
            proof! {
                reveal(crate::spec::holds_all);
            }
            return false;
        }
        at += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(crate::spec::holds_all);
    }
    true
}

/// Whether every key in `wanted` is in `first` or in `second`.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == crate::spec::holds_all_between(first@, second@, wanted@),
))]
fn holds_all_between(first: &[String], second: &[String], wanted: &[String]) -> bool {
    let mut at = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            at <= wanted.len(),
            forall|j: int|
                0 <= j < at ==> crate::spec::holds(first@, (#[trigger] wanted@[j])@)
                    || crate::spec::holds(second@, wanted@[j]@),
        decreases wanted.len() - at,
    ))]
    while at < wanted.len() {
        if !(holds(first, &wanted[at]) || holds(second, &wanted[at])) {
            #[cfg(verus_keep_ghost)]
            proof! {
                reveal(crate::spec::holds_all_between);
            }
            return false;
        }
        at += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(crate::spec::holds_all_between);
    }
    true
}

/// Whether each field of a scope went past the ceiling.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag for each field of a scope"
)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct Flags {
    project: bool,
    directories: bool,
    trusted: bool,
    distrusted: bool,
    undecided: bool,
    programs: bool,
    exposed: bool,
    mcp_servers: bool,
    references: bool,
    skill_directories: bool,
    checkouts: bool,
    language_servers: bool,
    standing_answers: bool,
    allow: bool,
    ask: bool,
    deny: bool,
    confinement: bool,
    approvals: bool,
    reads_stay_in_workspace: bool,
    auto_vetting: bool,
    confined_lines: bool,
}

/// The fields the flags mark, in the order of [`Field::ALL`].
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        (r@.len() == 0) == (!flags.project && !flags.directories && !flags.trusted && !flags.distrusted && !flags.undecided && !flags.programs && !flags.exposed && !flags.mcp_servers && !flags.references && !flags.skill_directories && !flags.checkouts && !flags.language_servers && !flags.standing_answers && !flags.allow && !flags.ask && !flags.deny && !flags.confinement && !flags.approvals && !flags.reads_stay_in_workspace && !flags.auto_vetting && !flags.confined_lines),
))]
fn flagged(flags: &Flags) -> Vec<Field> {
    let mut named = Vec::new();
    if flags.project {
        named.push(Field::Project);
    }
    if flags.directories {
        named.push(Field::Directories);
    }
    if flags.trusted {
        named.push(Field::Trusted);
    }
    if flags.distrusted {
        named.push(Field::Distrusted);
    }
    if flags.undecided {
        named.push(Field::Undecided);
    }
    if flags.programs {
        named.push(Field::Programs);
    }
    if flags.exposed {
        named.push(Field::Exposed);
    }
    if flags.mcp_servers {
        named.push(Field::McpServers);
    }
    if flags.references {
        named.push(Field::References);
    }
    if flags.skill_directories {
        named.push(Field::SkillDirectories);
    }
    if flags.checkouts {
        named.push(Field::Checkouts);
    }
    if flags.language_servers {
        named.push(Field::LanguageServers);
    }
    if flags.standing_answers {
        named.push(Field::StandingAnswers);
    }
    if flags.allow {
        named.push(Field::Allow);
    }
    if flags.ask {
        named.push(Field::Ask);
    }
    if flags.deny {
        named.push(Field::Deny);
    }
    if flags.confinement {
        named.push(Field::Confinement);
    }
    if flags.approvals {
        named.push(Field::Approvals);
    }
    if flags.reads_stay_in_workspace {
        named.push(Field::ReadsStayInWorkspace);
    }
    if flags.auto_vetting {
        named.push(Field::AutoVetting);
    }
    if flags.confined_lines {
        named.push(Field::ConfinedLines);
    }
    named
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Scope {
    /// Whether `current` is inside this scope, taken as the ceiling a device was approved for.
    ///
    /// Each field has the rule below, and a field that rule cannot show to be no wider is reported:
    ///
    /// - the project directory is equal;
    /// - a list that grants (every list but the distrusted paths, the undecided paths, and the
    ///   `ask` and `deny` rules) holds nothing the ceiling's did not;
    /// - a list that restricts (distrusted paths, `deny` rules) holds everything the ceiling's did;
    /// - an undecided path of the ceiling's is kept or replaced by a distrust, and no other path is
    ///   undecided, since an undecided path lifts a broader distrust;
    /// - an `ask` rule of the ceiling's is kept or replaced by a `deny`;
    /// - the sandbox reads no more and writes no more, the permission mode is no further along its
    ///   order, `reads_stay_in_workspace` is not turned off, `confined_lines` is not turned on, and
    ///   automatic vetting is not changed either way.
    ///
    /// A path trusted under a path the ceiling trusts is reported, because the rule compares keys and
    /// does not read one as lying under another.
    #[cfg_attr(verus_keep_ghost, verus_spec(r =>
        ensures
            (r is Covered) == crate::spec::scope_covered(*self, *current),
    ))]
    pub fn covers(&self, current: &Scope) -> Coverage {
        // Names every field with no `..`, so a field added to `Scope` stops the build here, which
        // is the place to give it a `Field`, a flag and a rule. The proof side does the same in
        // `spec::scope_covered`.
        #[cfg(not(verus_keep_ghost))]
        let Scope {
            project: _,
            directories: _,
            trusted: _,
            distrusted: _,
            undecided: _,
            programs: _,
            exposed: _,
            mcp_servers: _,
            references: _,
            skill_directories: _,
            checkouts: _,
            language_servers: _,
            standing_answers: _,
            allow: _,
            ask: _,
            deny: _,
            confinement: _,
            approvals: _,
            reads_stay_in_workspace: _,
            auto_vetting: _,
            confined_lines: _,
        } = self;
        let flags = Flags {
            project: !same_text(&self.project, &current.project),
            directories: !holds_all(&self.directories, &current.directories),
            trusted: !holds_all(&self.trusted, &current.trusted),
            distrusted: !holds_all(&current.distrusted, &self.distrusted),
            undecided: !holds_all_between(&current.undecided, &current.distrusted, &self.undecided)
                || !holds_all(&self.undecided, &current.undecided),
            programs: !holds_all(&self.programs, &current.programs),
            exposed: !holds_all(&self.exposed, &current.exposed),
            mcp_servers: !holds_all(&self.mcp_servers, &current.mcp_servers),
            references: !holds_all(&self.references, &current.references),
            skill_directories: !holds_all(&self.skill_directories, &current.skill_directories),
            checkouts: !holds_all(&self.checkouts, &current.checkouts),
            language_servers: !holds_all(&self.language_servers, &current.language_servers),
            standing_answers: !holds_all(&self.standing_answers, &current.standing_answers),
            allow: !holds_all(&self.allow, &current.allow),
            ask: !holds_all_between(&current.ask, &current.deny, &self.ask),
            deny: !holds_all(&current.deny, &self.deny),
            confinement: !current.confinement.is_no_looser_than(self.confinement),
            approvals: current.approvals.rank() > self.approvals.rank(),
            reads_stay_in_workspace: self.reads_stay_in_workspace
                && !current.reads_stay_in_workspace,
            auto_vetting: current.auto_vetting != self.auto_vetting,
            confined_lines: current.confined_lines && !self.confined_lines,
        };
        let widened = flagged(&flags);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(crate::spec::field_widened(*self, *current, Field::Project) == flags.project);
            assert(crate::spec::field_widened(*self, *current, Field::Directories) == flags.directories);
            assert(crate::spec::field_widened(*self, *current, Field::Trusted) == flags.trusted);
            assert(crate::spec::field_widened(*self, *current, Field::Distrusted) == flags.distrusted);
            assert(crate::spec::field_widened(*self, *current, Field::Undecided) == flags.undecided);
            assert(crate::spec::field_widened(*self, *current, Field::Programs) == flags.programs);
            assert(crate::spec::field_widened(*self, *current, Field::Exposed) == flags.exposed);
            assert(crate::spec::field_widened(*self, *current, Field::McpServers) == flags.mcp_servers);
            assert(crate::spec::field_widened(*self, *current, Field::References) == flags.references);
            assert(crate::spec::field_widened(*self, *current, Field::SkillDirectories) == flags.skill_directories);
            assert(crate::spec::field_widened(*self, *current, Field::Checkouts) == flags.checkouts);
            assert(crate::spec::field_widened(*self, *current, Field::LanguageServers) == flags.language_servers);
            assert(crate::spec::field_widened(*self, *current, Field::StandingAnswers) == flags.standing_answers);
            assert(crate::spec::field_widened(*self, *current, Field::Allow) == flags.allow);
            assert(crate::spec::field_widened(*self, *current, Field::Ask) == flags.ask);
            assert(crate::spec::field_widened(*self, *current, Field::Deny) == flags.deny);
            assert(crate::spec::field_widened(*self, *current, Field::Confinement) == flags.confinement);
            assert(crate::spec::field_widened(*self, *current, Field::Approvals) == flags.approvals);
            assert(crate::spec::field_widened(*self, *current, Field::ReadsStayInWorkspace) == flags.reads_stay_in_workspace);
            assert(crate::spec::field_widened(*self, *current, Field::AutoVetting) == flags.auto_vetting);
            assert(crate::spec::field_widened(*self, *current, Field::ConfinedLines) == flags.confined_lines);
            crate::spec::a_scope_is_covered_where_no_field_is_widened(*self, *current);
        }
        if widened.is_empty() {
            Coverage::Covered
        } else {
            Coverage::Widened(widened)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    /// A scope with something in every list, so a field can be narrowed as well as widened.
    fn base() -> Scope {
        Scope {
            project: "/work/app".to_string(),
            directories: keys(&["/work/lib"]),
            trusted: keys(&["/work/app"]),
            distrusted: keys(&["/work/app/secrets"]),
            undecided: keys(&["/work/app/vendor"]),
            programs: keys(&["cargo test"]),
            exposed: keys(&["/work/app/.env.example"]),
            mcp_servers: keys(&["docs"]),
            references: keys(&["/work/notes"]),
            skill_directories: keys(&["~/.claude/skills"]),
            checkouts: keys(&["/tmp/checkouts/c1"]),
            language_servers: keys(&["rust-analyzer"]),
            standing_answers: keys(&["weather:get_current_conditions"]),
            allow: keys(&["Read(src/**)"]),
            ask: keys(&["Write(*)"]),
            deny: keys(&["Run(rm *)"]),
            confinement: Confinement {
                reads: Reads::Machine,
                writes: Writes::Session,
            },
            approvals: Approvals::Ask,
            reads_stay_in_workspace: true,
            auto_vetting: false,
            confined_lines: false,
        }
    }

    /// `scope` with `field` past the ceiling `base` holds. A field added to [`Field`] has no arm here
    /// until somebody says how it widens.
    fn widen(mut scope: Scope, field: Field) -> Scope {
        match field {
            Field::Project => scope.project = "/work/other".to_string(),
            Field::Directories => scope.directories.push("/work/extra".to_string()),
            Field::Trusted => scope.trusted.push("/work/extra".to_string()),
            Field::Distrusted => scope.distrusted.clear(),
            Field::Undecided => scope.undecided.clear(),
            Field::Programs => scope.programs.push("make".to_string()),
            Field::Exposed => scope.exposed.push("/work/app/.env".to_string()),
            Field::McpServers => scope.mcp_servers.push("shell".to_string()),
            Field::References => scope.references.push("/etc".to_string()),
            Field::SkillDirectories => scope.skill_directories.push("/tmp/skills".to_string()),
            Field::Checkouts => scope.checkouts.push("/tmp/checkouts/c2".to_string()),
            Field::LanguageServers => scope.language_servers.push("pyright".to_string()),
            Field::StandingAnswers => scope.standing_answers.push("shell:run".to_string()),
            Field::Allow => scope.allow.push("Run(*)".to_string()),
            Field::Ask => scope.ask.clear(),
            Field::Deny => scope.deny.clear(),
            Field::Confinement => {
                scope.confinement = Confinement {
                    reads: Reads::Unconfined,
                    writes: Writes::Unconfined,
                }
            }
            Field::Approvals => scope.approvals = Approvals::Bypass,
            Field::ReadsStayInWorkspace => scope.reads_stay_in_workspace = false,
            Field::AutoVetting => scope.auto_vetting = true,
            Field::ConfinedLines => scope.confined_lines = true,
        }
        scope
    }

    /// `scope` with `field` narrower than `base` holds it, or `None` where `base` has no narrower
    /// value for the field.
    fn narrow(mut scope: Scope, field: Field) -> Option<Scope> {
        match field {
            Field::Project => return None,
            Field::Directories => scope.directories.clear(),
            Field::Trusted => scope.trusted.clear(),
            Field::Distrusted => scope.distrusted.push("/work/app/more".to_string()),
            Field::Undecided => {
                scope.distrusted.push("/work/app/vendor".to_string());
                scope.undecided.clear();
            }
            Field::Programs => scope.programs.clear(),
            Field::Exposed => scope.exposed.clear(),
            Field::McpServers => scope.mcp_servers.clear(),
            Field::References => scope.references.clear(),
            Field::SkillDirectories => scope.skill_directories.clear(),
            Field::Checkouts => scope.checkouts.clear(),
            Field::LanguageServers => scope.language_servers.clear(),
            Field::StandingAnswers => scope.standing_answers.clear(),
            Field::Allow => scope.allow.clear(),
            Field::Ask => {
                scope.deny.push("Write(*)".to_string());
                scope.ask.clear();
            }
            Field::Deny => scope.deny.push("Run(*)".to_string()),
            Field::Confinement => {
                scope.confinement = Confinement {
                    reads: Reads::Listed,
                    writes: Writes::Nothing,
                }
            }
            Field::Approvals => scope.approvals = Approvals::Plan,
            Field::ReadsStayInWorkspace => return None,
            Field::AutoVetting => return None,
            Field::ConfinedLines => return None,
        }
        Some(scope)
    }

    #[test]
    fn a_scope_covers_itself() {
        assert_eq!(base().covers(&base()), Coverage::Covered);
    }

    /// The place of `field` in [`Field::ALL`]. A field added to [`Field`] has no arm here until
    /// somebody gives it one.
    fn place(field: Field) -> usize {
        match field {
            Field::Project => 0,
            Field::Directories => 1,
            Field::Trusted => 2,
            Field::Distrusted => 3,
            Field::Undecided => 4,
            Field::Programs => 5,
            Field::Exposed => 6,
            Field::McpServers => 7,
            Field::References => 8,
            Field::SkillDirectories => 9,
            Field::Checkouts => 10,
            Field::LanguageServers => 11,
            Field::StandingAnswers => 12,
            Field::Allow => 13,
            Field::Ask => 14,
            Field::Deny => 15,
            Field::Confinement => 16,
            Field::Approvals => 17,
            Field::ReadsStayInWorkspace => 18,
            Field::AutoVetting => 19,
            Field::ConfinedLines => 20,
        }
    }

    #[test]
    fn the_list_of_fields_holds_every_field_once_in_its_place() {
        for (index, field) in Field::ALL.into_iter().enumerate() {
            assert_eq!(place(field), index, "{field:?} is in another place");
        }
        assert_eq!(Field::ALL.len(), place(Field::ConfinedLines) + 1);
        for field in Field::ALL {
            assert_ne!(
                widen(base(), field),
                base(),
                "{field:?} has no widening that changes the scope"
            );
        }
    }

    #[test]
    fn widening_one_field_reports_that_field_and_no_other() {
        for field in Field::ALL {
            let current = widen(base(), field);
            assert_eq!(
                base().covers(&current),
                Coverage::Widened(vec![field]),
                "{field:?}"
            );
        }
    }

    #[test]
    fn widening_several_fields_reports_each_in_the_order_of_the_scope() {
        let mut current = widen(base(), Field::AutoVetting);
        current = widen(current, Field::Directories);
        current = widen(current, Field::Deny);
        assert_eq!(
            base().covers(&current),
            Coverage::Widened(vec![Field::Directories, Field::Deny, Field::AutoVetting])
        );
    }

    #[test]
    fn narrowing_a_field_is_covered() {
        for field in Field::ALL {
            let Some(current) = narrow(base(), field) else {
                continue;
            };
            assert_ne!(current, base(), "{field:?} narrowed nothing");
            assert_eq!(base().covers(&current), Coverage::Covered, "{field:?}");
        }
    }

    #[test]
    fn narrowing_one_field_while_widening_another_reports_only_the_widened_one() {
        let narrowed = narrow(base(), Field::Directories);
        assert!(narrowed.is_some(), "the directories narrow");
        let current = widen(narrowed.unwrap_or_else(base), Field::Programs);
        assert_eq!(
            base().covers(&current),
            Coverage::Widened(vec![Field::Programs])
        );
    }

    #[test]
    fn a_scope_that_was_widened_covers_the_one_it_widened() {
        for field in Field::ALL {
            let wider = widen(base(), field);
            let expected = match field {
                // A ceiling for one project covers no other, in either direction.
                Field::Project => Coverage::Widened(vec![Field::Project]),
                // Turning it on or off both widen, since which way narrows depends on the mode.
                Field::AutoVetting => Coverage::Widened(vec![Field::AutoVetting]),
                // An undecided path the ceiling did not hold lifts a distrust.
                Field::Undecided => Coverage::Widened(vec![Field::Undecided]),
                _ => Coverage::Covered,
            };
            assert_eq!(wider.covers(&base()), expected, "{field:?}");
        }
    }

    #[test]
    fn strict_and_read_only_each_widen_the_other() {
        let strict = Confinement {
            reads: Reads::Listed,
            writes: Writes::Session,
        };
        let read_only = Confinement {
            reads: Reads::Machine,
            writes: Writes::Nothing,
        };
        let held = |confinement| Scope {
            confinement,
            ..base()
        };
        assert_eq!(
            held(strict).covers(&held(read_only)),
            Coverage::Widened(vec![Field::Confinement])
        );
        assert_eq!(
            held(read_only).covers(&held(strict)),
            Coverage::Widened(vec![Field::Confinement])
        );
    }

    #[test]
    fn an_undecided_boundary_may_become_a_distrust_and_a_distrust_may_not_become_undecided() {
        let ceiling = Scope {
            undecided: keys(&["/work/app/vendor"]),
            distrusted: keys(&["/work/app/secrets"]),
            ..base()
        };
        let stronger = Scope {
            undecided: vec![],
            distrusted: keys(&["/work/app/secrets", "/work/app/vendor"]),
            ..base()
        };
        let weaker = Scope {
            distrusted: vec![],
            undecided: keys(&["/work/app/vendor", "/work/app/secrets"]),
            ..base()
        };
        assert_eq!(ceiling.covers(&stronger), Coverage::Covered);
        assert_eq!(
            ceiling.covers(&weaker),
            Coverage::Widened(vec![Field::Distrusted, Field::Undecided])
        );
    }

    #[test]
    fn an_undecided_path_the_ceiling_did_not_hold_lifts_a_distrust_and_is_wider() {
        let ceiling = Scope {
            distrusted: keys(&["/work/app/secrets"]),
            undecided: vec![],
            ..base()
        };
        let current = Scope {
            distrusted: keys(&["/work/app/secrets"]),
            undecided: keys(&["/work/app/secrets/inner"]),
            ..base()
        };
        assert_eq!(
            ceiling.covers(&current),
            Coverage::Widened(vec![Field::Undecided])
        );
    }

    #[test]
    fn an_ask_rule_may_become_a_deny_and_not_the_reverse() {
        let ceiling = Scope {
            ask: keys(&["Write(*)"]),
            deny: vec![],
            ..base()
        };
        let stronger = Scope {
            ask: vec![],
            deny: keys(&["Write(*)"]),
            ..base()
        };
        let removed = Scope {
            ask: vec![],
            deny: vec![],
            ..base()
        };
        assert_eq!(ceiling.covers(&stronger), Coverage::Covered);
        assert_eq!(
            ceiling.covers(&removed),
            Coverage::Widened(vec![Field::Ask])
        );
    }

    #[test]
    fn the_permission_modes_widen_in_the_order_of_what_they_do_unasked() {
        let order = [
            Approvals::Plan,
            Approvals::Ask,
            Approvals::AcceptEdits,
            Approvals::Auto,
            Approvals::Bypass,
        ];
        for (at, ceiling) in order.iter().enumerate() {
            for (to, current) in order.iter().enumerate() {
                let held = |approvals| Scope {
                    approvals,
                    ..base()
                };
                let covered = held(*ceiling).covers(&held(*current)).is_covered();
                assert_eq!(covered, to <= at, "{ceiling:?} then {current:?}");
            }
        }
    }

    #[test]
    fn a_path_trusted_beneath_a_trusted_path_is_reported_because_keys_are_compared_not_read() {
        let current = Scope {
            trusted: keys(&["/work/app", "/work/app/src"]),
            ..base()
        };
        assert_eq!(
            base().covers(&current),
            Coverage::Widened(vec![Field::Trusted])
        );
    }

    #[test]
    fn every_field_has_a_name_a_person_can_read() {
        for field in Field::ALL {
            assert!(!field.name().is_empty(), "{field:?}");
        }
    }
}
