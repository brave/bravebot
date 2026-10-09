//! Skills: instructions the user keeps for a kind of task.
//!
//! A skill is one `SKILL.md` in a directory of its own, opening with frontmatter that names it
//! and says when it applies:
//!
//! ```text
//! ---
//! name: commit-style
//! description: How commit messages are written here. Use before writing one.
//! ---
//!
//! the body
//! ```
//!
//! Only the name and the description reach the planner up front. The body is read when the
//! planner asks for it by name, which keeps a directory of long skills from filling a context
//! that has room for the task instead.
//!
//! A file may also name the model its rounds are asked of and the effort they carry. Neither is
//! required, and neither is a reason to drop a skill: a value that cannot be used is reported and
//! the skill loads on whatever the session was already running.
//!
//! # What this module may and may not read
//!
//! Everything here parses **trusted** text. A caller reaches this only after
//! `Policy::read_trusted_content` has handed the bytes over, which it does for a file in the
//! user's own directory and refuses for anything else. That refusal is the whole design: a
//! skill's name and description go into the system prompt verbatim, so a skill nobody vouched
//! for is dropped entirely rather than quarantined. A reference in place of a name would be no
//! use to anyone, and a name from a file an attacker wrote would be untrusted content in the
//! planner's context.

use crate::workspace::Workspace;
use bravebot_aichat::protocol::Effort;
use bravebot_core::capability::Capability;
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_i18n::t;
use std::path::Path;

/// What a `SKILL.md` declares about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    /// What the planner calls the skill when it asks for the body.
    pub name: String,
    /// When to use it. This is what the planner decides from, so it says when rather than what.
    pub description: String,
    /// What follows the name when the skill is invoked, for an interface to show. `argument-hint`
    /// in the file, and absent when the file has none. Never advertised to the planner.
    pub argument_hint: Option<String>,
    /// The model its rounds are asked of, as the file wrote it, or nothing where it named none.
    ///
    /// Left as written rather than resolved here, the way a delegate definition's is: resolving an
    /// alias is configuration's business, and this module is given no configuration.
    pub model: Option<String>,
    /// The effort word its rounds carry, as the file wrote it, or nothing where it named none.
    ///
    /// The word rather than the level, so a word naming no level can be said back to whoever wrote
    /// it. [`runs_as`] is what turns one into a level.
    pub effort: Option<String>,
    /// Every key this does not read, in the order a sorted block declares them.
    ///
    /// Carried out rather than dropped: a key nothing reads is a line whose author believes it is
    /// in force, and the report `doctor` makes of these is the whole of what tells them otherwise.
    pub unread: Vec<String>,
}

/// Every key [`parse_frontmatter`] reads. Anything else is carried out on [`Frontmatter::unread`].
const READ: [&str; 5] = ["name", "description", "argument-hint", "model", "effort"];

/// How a skill asks the rounds that follow it to run.
///
/// Both halves optional, and absence in either leaves the session's own choice in force. Kept
/// together because a skill naming one usually names the other, and because what they have in
/// common is that neither is a reason to drop a skill: an unusable value is reported and the skill
/// still loads, where a missing name or description makes it unchoosable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunsAs {
    /// The model its rounds are asked of, as the file wrote it. Resolved where it is used.
    pub model: Option<String>,
    /// The effort level its rounds carry.
    pub effort: Option<Effort>,
}

impl RunsAs {
    /// Whether this asks for anything at all, which is what decides whether there is a switch.
    pub fn names_anything(&self) -> bool {
        self.model.is_some() || self.effort.is_some()
    }
}

/// The line that opens and closes a frontmatter block.
const MARKER: &str = "---";

/// Read the frontmatter at the top of a `SKILL.md`, if it has one.
///
/// Hand-written rather than a YAML dependency, per the conventions: this recognises `key: value`
/// on a line, and a value continued on the lines indented beneath it, and nothing else. There is
/// no parser to surprise us and nothing to backtrack. A key this does not read stops nothing: the
/// skill loads, which leaves room for a file written for another agent to work here too, and the
/// key is carried out on `unread` so a report can name it rather than leaving it a silent no-op.
///
/// `None` means "not a skill", and every caller drops the file on that answer. A half-declared
/// skill is included in that: a name with no description is one the planner cannot choose
/// between, and advertising it would be worse than leaving it out. Neither `model` nor `effort`
/// is in that: a skill with neither is choosable, so a value that cannot be used is reported and
/// the skill still loads.
pub fn parse_frontmatter(text: &str) -> Option<Frontmatter> {
    let declared = declarations(text)?;
    let name = declared.get("name").filter(|n| !n.is_empty())?;
    let description = declared.get("description").filter(|d| !d.is_empty())?;
    Some(Frontmatter {
        name: name.clone(),
        description: description.clone(),
        argument_hint: declared
            .get("argument-hint")
            .filter(|hint| !hint.is_empty())
            .cloned(),
        // `inherit` is how other agents' definitions name no model, so one ported from them keeps
        // meaning that rather than sending the word as a model name. The same rule a delegate
        // definition reads a model by, for the same reason.
        model: declared_word(&declared, "model")
            .filter(|model| !model.eq_ignore_ascii_case("inherit")),
        effort: declared_word(&declared, "effort"),
        unread: declared
            .keys()
            .filter(|key| !READ.contains(&key.as_str()))
            .cloned()
            .collect(),
    })
}

/// One declared value with the whitespace off, or nothing where the key named nothing.
///
/// A blank value is absence rather than a choice of the empty string: `model:` with nothing after
/// it is a line somebody started and did not finish, and sending the empty string as a model name
/// would be answered by whatever the service substitutes.
fn declared_word(
    declared: &std::collections::BTreeMap<String, String>,
    key: &str,
) -> Option<String> {
    declared
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// What a skill asks its rounds to run as, and a notice for anything it asked for that cannot be.
///
/// The effort word is settled here because a level is one of five this program enumerates, and a
/// word naming none of them is a fact about the file that nothing downstream could recover: an
/// unrecognised word must not become a request field. The model is left as written, because whether
/// a name resolves is a question about this machine's configuration and this module has none.
///
/// A skill still loads either way. The notice names the file and the word it wrote, both of which
/// are safe to print: this is reached only for a source somebody vouched for.
fn runs_as(front: &Frontmatter, origin: &str, notices: &mut Vec<Notice>) -> RunsAs {
    let effort = match front.effort.as_deref() {
        None => None,
        Some(word) => match Effort::named(word) {
            Some(level) => Some(level),
            None => {
                notices.push(Notice::new(t!(
                    skill_effort_not_a_level,
                    skill = origin,
                    effort = word,
                    levels = Effort::ALL.map(Effort::as_str).join(", ")
                )));
                None
            }
        },
    };
    RunsAs {
        model: front.model.clone(),
        effort,
    }
}

/// Every `key: value` a frontmatter block declares, wrapped values joined.
///
/// The one dialect, so a delegate definition is read the way a skill is and a file written for
/// another agent parses the same either way. Keys nothing here looks for are returned rather than
/// dropped, and callers ignore what they do not want: a strict schema would refuse a file written
/// for a second agent, and being one directory two agents can read is the point.
///
/// `None` means "not frontmatter", and every caller drops the file on that answer.
pub fn declarations(text: &str) -> Option<std::collections::BTreeMap<String, String>> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some(MARKER) {
        return None;
    }

    let mut block = Vec::new();
    let mut closed = false;
    for line in lines {
        if line.trim_end() == MARKER {
            closed = true;
            break;
        }
        block.push(line);
    }

    // An unterminated block is not frontmatter that happens to run long: it is a file whose whole
    // contents would otherwise be read as declarations.
    if !closed {
        return None;
    }

    let mut declared = std::collections::BTreeMap::new();

    let mut at = 0;
    while at < block.len() {
        let line = block[at];
        at += 1;

        // Taken whether or not this line declares anything we want. A continuation belongs to
        // the key above it however that key is spelled, and leaving one unconsumed is how a
        // wrapped sentence holding a colon becomes a key of its own.
        let opened_at = indent_of(line);
        let mut wrapped = Vec::new();
        while let Some(next) = block.get(at) {
            if !next.trim().is_empty() && indent_of(next) <= opened_at {
                break;
            }
            wrapped.push(*next);
            at += 1;
        }

        let Some((key, first)) = line.split_once(':') else {
            continue;
        };
        // Last wins, as the pair of `let`s this replaced did. A file declaring a key twice is
        // malformed YAML rather than a shape to support, and reading the second is what every
        // caller here did before there was a map.
        declared.insert(key.trim().to_string(), value_of(first, &wrapped));
    }

    Some(declared)
}

/// How many columns a line is indented by, which is what says whether it continues the one above.
pub(crate) fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// One value, from what followed the colon and the lines indented under it.
///
/// A description is a sentence long enough that people wrap it, and every way of wrapping one
/// ends up here: a folded or literal block introduced by `>` or `|`, a quoted scalar carried over
/// several lines, or plain text simply continued. Folded because that is what wrapping means:
/// the line breaks were the file's, not the sentence's, so they become spaces. A literal block
/// asked for its newlines and keeps them.
fn value_of(first: &str, wrapped: &[&str]) -> String {
    let first = first.trim();
    let (joiner, first) = match first {
        "|" | "|-" | "|+" => ("\n", ""),
        ">" | ">-" | ">+" => (" ", ""),
        _ => (" ", first),
    };

    let mut parts = Vec::new();
    if !first.is_empty() {
        parts.push(first);
    }
    if joiner == "\n" {
        // A blank line inside a literal block is one of the newlines it asked for. Those before
        // the first line and after the last belong to the file's layout and are dropped.
        let lines: Vec<&str> = wrapped.iter().map(|l| l.trim()).collect();
        let from = lines
            .iter()
            .position(|l| !l.is_empty())
            .unwrap_or(lines.len());
        let to = lines
            .iter()
            .rposition(|l| !l.is_empty())
            .map_or(from, |at| at + 1);
        parts.extend(&lines[from..to]);
    } else {
        parts.extend(wrapped.iter().map(|l| l.trim()).filter(|l| !l.is_empty()));
    }

    unquoted(&parts.join(joiner))
}

/// A quoted scalar without its quotes, and anything else unchanged.
///
/// The quotes are YAML's, put there so a value may open with a character that would otherwise
/// mean something. They are not part of what the planner is choosing from, and leaving them in
/// puts a stray apostrophe at each end of every description on the screen.
fn unquoted(value: &str) -> String {
    let opens = value.chars().next();
    let closes = value.chars().last();
    match (opens, closes) {
        (Some('\''), Some('\'')) if value.len() >= 2 => {
            value[1..value.len() - 1].replace("''", "'")
        }
        (Some('"'), Some('"')) if value.len() >= 2 => {
            value[1..value.len() - 1].replace("\\\"", "\"")
        }
        _ => value.to_string(),
    }
}

/// Everything after the frontmatter block, which is what the planner is given when it asks.
///
/// The frontmatter itself is left out because the planner has already been told the name and the
/// description; sending them again spends context on what it used to make the call.
pub fn body_after_frontmatter(text: &str) -> &str {
    let mut offset = 0;
    let mut lines = text.split_inclusive('\n');

    let Some(first) = lines.next() else {
        return "";
    };
    if first.trim_end() != MARKER {
        return text;
    }
    offset += first.len();

    for line in lines {
        offset += line.len();
        if line.trim_end() == MARKER {
            return text[offset..].trim_start_matches('\n');
        }
    }
    // No closing marker, so there was no frontmatter to strip.
    text
}

/// A skill the planner may ask for.
///
/// Deliberately not comparable: it holds a `Labelled`, which has no `PartialEq` precisely so
/// that content cannot be decided from by comparing it.
#[derive(Debug, Clone)]
pub struct Skill {
    /// What the planner names to load it.
    pub name: String,
    /// When to use it, which is what the planner decides from.
    pub description: String,
    /// What follows the name when it is invoked, for an interface to draw after the name. Not
    /// part of what the planner is advertised.
    pub argument_hint: Option<String>,
    /// Where it came from, for the audit trail and for what the user is told.
    pub origin: String,
    /// Which of the three places it came from, for an interface saying so beside its name.
    pub source: Source,
    /// The model and the effort its rounds run at, where its file named either.
    pub runs_as: RunsAs,
    /// The keys its file declared that nothing here reads, for a report a person asks for.
    pub unread: Vec<String>,
    /// The instructions themselves, still carrying the label they were read with.
    ///
    /// Kept labelled rather than as bare text so the planner is shown them through
    /// `Policy::present` like any other content. Nothing here re-labels: the value is the one
    /// the source produced, reshaped in the kernel to drop the frontmatter.
    body: Labelled<String>,
}

/// The three places a skill can come from, least specific first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Written into this program.
    BuiltIn,
    /// The user's own directory, `~/.bravebot/skills`.
    Home,
    /// The project's own skill directories, which the trust map vouched for.
    Workspace,
}

impl Skill {
    /// The instructions, which reach the planner only when it asks for them by name.
    pub fn body(&self) -> &Labelled<String> {
        &self.body
    }
}

/// Why an `@path` import was not expanded.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ImportRefusal {
    /// Nested past the depth limit, or past the count limit for one instructions file.
    TooDeep,
    /// A file above it on the way down is already importing it.
    Cycle,
    /// Outside the project, unreadable, or not trusted.
    NotLoaded,
}

/// Something the user should be told about discovery, in words a person reads.
///
/// A skill that was skipped is worth a line: silence would read as "you have no skills" to
/// someone who just wrote one, and the reason is usually a typo in the frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub message: String,
}

impl Notice {
    /// Build a notice from words the driver wrote.
    ///
    /// Public so the preamble can report a refusal of its own. The message is always the
    /// driver's own text, never content, which is what makes it safe to put on a screen.
    pub fn from_message(message: impl Into<String>) -> Self {
        Self::new(message)
    }

    /// That `origin` was left out because a `deny` rule covers it (PERM-7).
    ///
    /// Here rather than beside each source because the preamble's own words are the planner's and
    /// may not come from a catalog, and this sentence is the person's.
    pub(crate) fn denied_by_rule(origin: &str) -> Self {
        Self::new(t!(source_denied_by_rule, source = origin))
    }

    /// That an entry of the `references` block was not offered to the planner, and why (REFER-3).
    ///
    /// Here for the reason [`Notice::denied_by_rule`] is: the preamble's own words are the
    /// planner's, and this sentence is the person's.
    pub(crate) fn reference_not_used(
        alias: &str,
        problem: &crate::workspace::ReferenceProblem,
    ) -> Self {
        use crate::workspace::ReferenceProblem;
        use bravebot_config::ReferenceFault;
        Self::new(match problem {
            ReferenceProblem::Unusable(ReferenceFault::BadAlias) => {
                t!(reference_bad_alias, alias = alias)
            }
            ReferenceProblem::Unusable(ReferenceFault::RepositoryNotFetched) => {
                t!(reference_repository_not_fetched, alias = alias)
            }
            ReferenceProblem::Unusable(ReferenceFault::NoPath) => {
                t!(reference_no_path, alias = alias)
            }
            ReferenceProblem::NotOpened(problem) => {
                t!(reference_not_opened, alias = alias, problem = problem)
            }
        })
    }

    /// That an `@path` import in an instructions file was left as written, and why (INSTR-11).
    pub(crate) fn import_refused(import: &str, why: ImportRefusal) -> Self {
        Self::new(match why {
            ImportRefusal::TooDeep => t!(import_too_deep, import = import),
            ImportRefusal::Cycle => t!(import_cycle, import = import),
            ImportRefusal::NotLoaded => t!(import_not_loaded, import = import),
        })
    }

    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// The skills available this turn, in the order they are offered to the planner.
#[derive(Debug, Clone, Default)]
pub struct Catalogue {
    entries: Vec<Skill>,
}

impl Catalogue {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Skill> {
        self.entries.iter()
    }

    /// The skill by that name, or `None`.
    ///
    /// The planner selects from this set rather than naming a path, so whatever it asks for
    /// either matches something the driver enumerated or matches nothing at all. That is what
    /// keeps a proposed name from reaching the filesystem.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.entries.iter().find(|s| s.name == name)
    }

    /// The skills of these names and no others, in the order they were found.
    ///
    /// A selection out of what was already found, so a name matching none of it adds nothing:
    /// what this can leave a planner with is a shorter list of the same skills.
    pub fn only(mut self, names: &[String]) -> Self {
        self.entries.retain(|skill| names.contains(&skill.name));
        self
    }

    /// Add a skill, replacing one of the same name.
    ///
    /// Later wins, and discovery visits the home directory before the workspace, so a project's
    /// own skill shadows a global one. That is the same "most specific wins" the trust map uses.
    fn insert(&mut self, skill: Skill) {
        match self.entries.iter_mut().find(|s| s.name == skill.name) {
            Some(existing) => *existing = skill,
            None => self.entries.push(skill),
        }
    }

    /// The lines that go in the system prompt: one per skill, name and when to use it.
    ///
    /// Never the body. A directory of long skills would otherwise fill a context that has room
    /// for the task instead, which is the whole reason the body waits to be asked for.
    pub fn describe_for_prompt(&self) -> String {
        let mut out = String::new();
        for skill in &self.entries {
            out.push_str(&format!("- {}: {}\n", skill.name, skill.description));
        }
        out
    }
}

/// A skill written into this program rather than found on a disk.
///
/// The name is a string literal here, which is what makes it different in kind from every other
/// skill. A name read out of a directory is content: it comes from whoever wrote the directory,
/// it may not be trusted, and it is why an untrusted skill is counted rather than named. A name
/// written here comes from this repository, cannot be added to or changed by anything a turn
/// does, and is the only kind that may also be a word the interface itself claims.
struct BuiltIn {
    name: &'static str,
    description: &'static str,
    body: &'static str,
}

/// The skills every session has, whatever is on the disk.
///
/// A description says when to load the skill, and the loop instructions apply where something else
/// is already supplying the repetition. Naming a watch request here would advertise them to a
/// session that is not a loop, where the account of a tick reads as an instruction to look once and
/// say what changed, and where the tool it offers for pacing the next tick is not in the table.
const BUILT_IN: [BuiltIn; 1] = [BuiltIn {
    name: "loop",
    description: "How a repeating turn works: what one tick is, what to do in it, and how to \
                  say when the next is due. Load it when this turn is a tick of a loop.",
    body: LOOP,
}];

/// What the planner is told about being inside a loop.
const LOOP: &str = "\
A loop repeats one prompt: the line the user typed when they started it. Every tick sends that \
same line again, unchanged, and nothing you do alters it. You are being asked the same question \
about a world that may have moved since the last time.

Each tick is a turn of its own in the same conversation, so what earlier ticks did is still \
there to read. Read it. A tick that repeats work the last one did has spent a request to learn \
nothing, and a tick that answers as though the question were new is the commonest way to waste \
one.

Do this tick's work, and only this tick's. A loop is not a way to ask for the whole job at once: \
if the line says to watch something, look at it as it is now and say what changed.

Keep the answer short. A tick that found nothing says so in a line rather than restating the \
situation, and somebody reading twenty of these should be able to see at a glance which one \
mattered.

There are two kinds of loop, and the turn you are in says which this is.

Where the user gave an interval, the timing is theirs. There is nothing for you to decide about \
it and no tool for it, so do not go looking for one and do not tell the user a tool is missing: \
the interval is already keeping time. Work the tick and answer.

Where they gave none, you are offered schedule_next, and the loop runs for exactly as long as \
you keep calling it with a wait. Call it once, at the end of the turn, after the work is done:

- delay_seconds from what you are actually waiting on rather than from a round number. Something \
  that takes ten minutes to change is not worth looking at in sixty seconds, and something that \
  changes hourly is not worth looking at in five minutes. The wait is held to between a second \
  and an hour, and it starts when this turn ends, so a whole turn separates two looks however \
  short you make it: asking for the floor buys you the pace of a turn, not the pace you named.
- noop true where this tick found nothing to do and changed nothing, false where something \
  happened worth keeping: an edit, a message, a finding. Runs of quiet ticks are counted and \
  shown to the user as a single line, so an honest noop is what keeps a long watch readable.
- reason in a few words, saying what you are waiting on. The user reads it.

Once there is nothing left to watch, call it with stop true instead of a delay_seconds, which \
ends the loop now. Say so in your answer rather than scheduling a tick to say it again. A turn \
that calls it with neither is woken once more after twenty minutes before the loop ends.

A turn that says no tool sets the pace is not offered schedule_next: work the tick and answer, and \
the loop ends with it.

Either kind stops when the user stops it. You never need to ask them to.
";

/// The directory holding skills, inside the user's own directory and inside a project.
const SKILLS: &str = "skills";
const WORKSPACE_SKILLS: &str = ".bravebot/skills";

/// The project directories holding skills, least specific first.
///
/// `.bravebot/skills` is read last, so a skill there shadows one of the same name under a
/// directory another agent keeps its skills in. That is the same "most specific wins" every other
/// source follows, and it leaves a project able to override one ported skill without moving the
/// rest. A project that keeps its skills for another agent offers them here without anybody
/// copying or symlinking each one, which is the same reason `CLAUDE.md` is read where `AGENTS.md`
/// is absent.
///
/// The project root only. Each name is relative to the root and holds no `..`, so there is no
/// search of parent directories and no nested skills directory, and the paths are string literals
/// in this file rather than anything read from a disk.
const WORKSPACE_SKILL_ROOTS: [&str; 3] = [".agents/skills", ".claude/skills", WORKSPACE_SKILLS];

/// The one file that makes a directory a skill.
const SKILL_FILE: &str = "SKILL.md";

/// Find the skills available to this turn.
///
/// Sources are visited least specific first so the more specific shadows them: the user's own
/// directory, whose contents are trusted for being the user's own, and then the project's own
/// skill directories, whose contents are trusted only if the trust map says so.
///
/// A skill from a path nobody vouched for is **dropped, not quarantined**. Its name and its
/// description would go into the system prompt verbatim, so offering a reference in their place
/// would be no use to the planner, and offering the strings themselves would be untrusted
/// content in the planner's context. There is no third option, and dropping it is the one that
/// holds the rule.
pub fn discover<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    home: Option<&Path>,
) -> (Catalogue, Vec<Notice>) {
    let mut catalogue = Catalogue::default();
    let mut notices = Vec::new();

    // First, so that a skill of the user's own with the same name shadows one of these, which is
    // the same "most specific wins" every other source follows. There is no gate to pass and
    // nothing to vouch for: this is the program's own text, not a source somebody supplied, so it
    // can neither be refused nor be missing, and there is never a notice about it.
    for built_in in BUILT_IN {
        catalogue.insert(Skill {
            name: built_in.name.to_string(),
            description: built_in.description.to_string(),
            argument_hint: None,
            // A string literal in this file, derived from nothing that arrived from anywhere. It
            // is trusted for being this program's own words, which is what the label says.
            body: Labelled::trusted(built_in.body.to_string()),
            origin: "built-in".to_string(),
            source: Source::BuiltIn,
            // A built-in is this program's own text, so there is no file to have named a model or
            // an effort and no key nothing reads: it runs as the session does.
            runs_as: RunsAs::default(),
            unread: Vec::new(),
        });
    }

    // The program's own skills above are not customizations and stay. Everything below is a file
    // somebody wrote.
    if bravebot_core::safe::engaged() {
        return (catalogue, notices);
    }

    if let Some(home) = home {
        discover_home(policy, &home.join(SKILLS), &mut catalogue, &mut notices);
    }
    discover_workspace(policy, workspace, &mut catalogue, &mut notices);

    (catalogue, notices)
}

/// The set a turn starting now would resolve, for an interface offering the names as they are typed.
///
/// Read the way a turn reads it, through a policy holding only the read, so what the box offers is
/// what the planner would be advertised: the same gate, the same trust map, the same rules, the same
/// shadowing. An untrusted project's skills are dropped here exactly as they are there, and so is a
/// skill `permissions` denies reading, so no name the turn would refuse is ever drawn.
pub fn resolved<S: Sink>(
    workspace: &Workspace,
    home: Option<&Path>,
    trust: bravebot_core::trust::TrustStore,
    permissions: bravebot_core::permissions::Permissions,
    sink: &mut S,
) -> Catalogue {
    let mut routing = bravebot_core::policy::Routing::new();
    routing.insert_trusted("skills", WORKSPACE_SKILL_ROOTS.join(", "));
    let Ok(policy) = Policy::begin(
        routing,
        bravebot_core::policy::ReleasePlan::new(),
        bravebot_core::capability::CapabilitySet::from_iter([Capability::FileRead]),
        sink,
    ) else {
        return Catalogue::default();
    };
    let mut policy = policy
        .with_trust(trust)
        .with_permissions(permissions)
        .with_root(workspace.root())
        .with_backslash_separates(crate::workspace::BACKSLASH_SEPARATES);
    discover(&mut policy, workspace, home).0
}

/// Skills from `~/.bravebot/skills`, labelled from where they sit.
fn discover_home<S: Sink>(
    policy: &mut Policy<'_, S>,
    root: &Path,
    catalogue: &mut Catalogue,
    notices: &mut Vec<Notice>,
) {
    if policy.before_capability(Capability::FileRead).is_err() {
        return;
    }

    for name in skill_directories(root) {
        let file = root.join(&name).join(SKILL_FILE);
        let origin = format!("~/.bravebot/{SKILLS}/{name}/{SKILL_FILE}");

        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };

        // Labelled here and gated below rather than used directly. The gate is the same one a
        // workspace skill passes through, so there is one way into the system prompt and it
        // refuses, and the trail records both halves.
        let labelled = policy.label_user_configuration(&origin, text);
        let Ok(text) = policy.read_trusted_content("skills", &labelled) else {
            continue;
        };

        match parse_frontmatter(&text) {
            Some(front) => {
                let body = policy.render_in_place("skills", &labelled, |whole| {
                    body_after_frontmatter(&whole).to_string()
                });
                let runs_as = runs_as(&front, &origin, notices);
                catalogue.insert(Skill {
                    name: front.name,
                    description: front.description,
                    argument_hint: front.argument_hint,
                    body,
                    origin,
                    source: Source::Home,
                    runs_as,
                    unread: front.unread,
                });
            }
            None => notices.push(Notice::new(format!(
                "{origin} was skipped: it needs a name and a description in its frontmatter"
            ))),
        }
    }
}

/// Skills from a project's skill directories, labelled by the trust map.
///
/// Each root is visited in [`WORKSPACE_SKILL_ROOTS`] order, least specific first, so a skill in
/// `.bravebot/skills` replaces one of the same name found under another agent's directory.
fn discover_workspace<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    catalogue: &mut Catalogue,
    notices: &mut Vec<Notice>,
) {
    // The skipped names are held back rather than reported as each root is read, because the roots
    // are read least specific first and a more specific one later in the list may offer the same
    // skill. Reporting in place would tell a person a skill was not loaded while they can see that
    // it was: the layout `make init` creates in this repository symlinks the same skills into all
    // three, so that notice would be wrong on every turn.
    let mut skipped: Vec<(String, Vec<String>)> = Vec::new();
    for root in WORKSPACE_SKILL_ROOTS {
        discover_workspace_root(policy, workspace, root, catalogue, notices, &mut skipped);
    }
    // After the root's, so a skill under a directory the session has worked in shadows the
    // project's of the same name, the deeper directory last (INSTR-14).
    if !bravebot_core::safe::engaged() {
        for directory in workspace.touched_directories() {
            let root = format!("{directory}/{WORKSPACE_SKILLS}");
            discover_workspace_root(policy, workspace, &root, catalogue, notices, &mut skipped);
        }
    }
    for (root, names) in skipped {
        let missing = names.len()
            - names
                .iter()
                .filter(|name| catalogue.get(name).is_some())
                .count();
        if missing == 0 {
            continue;
        }
        let (count, verb) = counted(missing);
        notices.push(Notice::new(format!(
            "{count} in {root} {verb} not loaded: this directory is not trusted"
        )));
    }
}

/// Skills from one of a project's skill directories, labelled by the trust map.
fn discover_workspace_root<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    skills_root: &str,
    catalogue: &mut Catalogue,
    notices: &mut Vec<Notice>,
    skipped: &mut Vec<(String, Vec<String>)>,
) {
    let root = workspace.root().join(skills_root);
    let names = skill_directories(&root);
    if names.is_empty() {
        return;
    }

    // Checked before anything is enumerated, and checked on the label rather than on any
    // content. A directory name is content too: a skill directory in a project nobody vouched
    // for could be named to read like an instruction, and it would reach the user's screen in a
    // notice even if it never reached the prompt.
    if !policy.trusts_path(skills_root) {
        // The names, so the caller can drop the ones a more specific root went on to offer. Held
        // here and never put in a notice: a directory name in a project nobody vouched for is
        // content, and only the count of it reaches a screen (SKILL-6).
        skipped.push((skills_root.to_string(), names));
        return;
    }

    // Counted rather than named, as the directory above is (SKILL-6). The directory is vouched
    // for, but a skill file inside it can still be distrusted, and the name of a directory a
    // turn that acted on untrusted content created is that content's to choose.
    let mut denied = 0;
    let mut distrusted = 0;

    for name in names {
        let relative = format!("{skills_root}/{name}/{SKILL_FILE}");

        if workspace.rule_denies_reading(policy, &relative) {
            denied += 1;
            continue;
        }
        let Ok(contents) = workspace.read(policy, &Labelled::trusted(relative.clone())) else {
            continue;
        };

        // Asked of the label before it is asked of the gate. The gate is still the only thing
        // that hands bytes over, and it still runs whenever this proceeds; what this avoids is
        // recording a denial for a condition that is ordinary and expected, which would mark
        // every turn in an untrusted directory as one where something was refused and teach the
        // user to ignore the times it means something.
        if !contents.label().is_trusted() {
            distrusted += 1;
            continue;
        }
        let Ok(text) = policy.read_trusted_content("skills", &contents) else {
            continue;
        };

        match parse_frontmatter(&text) {
            Some(front) => {
                let body = policy.render_in_place("skills", &contents, |whole| {
                    body_after_frontmatter(&whole).to_string()
                });
                let runs_as = runs_as(&front, &relative, notices);
                catalogue.insert(Skill {
                    name: front.name,
                    description: front.description,
                    argument_hint: front.argument_hint,
                    body,
                    origin: relative,
                    source: Source::Workspace,
                    runs_as,
                    unread: front.unread,
                });
            }
            None => notices.push(Notice::new(format!(
                "{relative} was skipped: it needs a name and a description in its frontmatter"
            ))),
        }
    }

    for (n, why) in [
        (distrusted, "the file is not trusted"),
        (denied, "a deny rule in your settings covers the file"),
    ] {
        if n > 0 {
            let (count, verb) = counted(n);
            notices.push(Notice::new(format!(
                "{count} in {skills_root} {verb} not loaded: {why}"
            )));
        }
    }
}

/// A count of skills, and the verb that agrees with it, so a line does not read "1 skills were"
/// or "1 skill were".
fn counted(n: usize) -> (String, &'static str) {
    if n == 1 {
        ("1 skill".to_string(), "was")
    } else {
        (format!("{n} skills"), "were")
    }
}

/// The names of the directories under a skills root, sorted.
///
/// Sorted so a turn offers the same skills in the same order every time. An order that came from
/// the filesystem would vary by machine, which would make the prompt vary with it.
fn skill_directories(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| root.join(name).join(SKILL_FILE).is_file())
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The planner chooses a skill from its name and description alone, so a file declaring only
    /// one of them offers nothing to choose on and must not be advertised at all.
    #[test]
    fn frontmatter_without_a_name_or_description_is_skipped() {
        assert_eq!(parse_frontmatter("---\nname: only\n---\nbody\n"), None);
        assert_eq!(
            parse_frontmatter("---\ndescription: only\n---\nbody\n"),
            None
        );
        assert_eq!(parse_frontmatter("---\nname:\ndescription: x\n---\n"), None);
    }

    /// An ordinary markdown file in a skills directory is not a skill. Treating one as a skill
    /// would put a heading in the system prompt as though the user had declared it.
    #[test]
    fn a_file_with_no_frontmatter_is_not_a_skill() {
        assert_eq!(parse_frontmatter("# notes\n\nsome prose\n"), None);
        assert_eq!(parse_frontmatter(""), None);
    }

    /// Without a closing marker there is no bounded block, and every line of the file would be
    /// read as a declaration. Refusing is the safe reading of an ambiguous file.
    #[test]
    fn an_unterminated_frontmatter_block_is_skipped_rather_than_swallowing_the_body() {
        let text = "---\nname: runaway\ndescription: never closed\n\nbody: text\n";
        assert_eq!(parse_frontmatter(text), None);
    }

    /// A file written for another agent may carry keys this does not know. Loading it anyway is
    /// what lets one skill directory serve more than one tool, and carrying the key out is what
    /// keeps it from being a line whose author is never told it did nothing.
    #[test]
    fn a_key_nothing_here_reads_does_not_stop_a_skill_and_is_carried_out() {
        let text = "---\nname: shared\nlicense: MPL-2.0\ndescription: works \
                    anyway\nargument-hint: '[x]'\n---\nbody\n";
        assert_eq!(
            parse_frontmatter(text),
            Some(Frontmatter {
                name: "shared".to_string(),
                description: "works anyway".to_string(),
                argument_hint: Some("[x]".to_string()),
                model: None,
                effort: None,
                unread: vec!["license".to_string()],
            })
        );
    }

    /// The two keys beyond the name and the description, and neither of them counted as unread.
    /// A key read into a field and reported as unread at the same time would have `doctor` telling
    /// somebody their line does nothing while the turn was running on it.
    #[test]
    fn a_skill_reads_the_model_and_the_effort_it_names() {
        let parsed = parse_frontmatter(
            "---\nname: n\ndescription: d\nmodel: haiku\neffort: HIGH\n---\nbody\n",
        )
        .expect("parses");

        assert_eq!(parsed.model.as_deref(), Some("haiku"));
        assert_eq!(parsed.effort.as_deref(), Some("HIGH"));
        assert!(parsed.unread.is_empty(), "got: {:?}", parsed.unread);
    }

    /// A line somebody started and did not finish names nothing, and `inherit` is how another
    /// agent's file spells naming no model. Sending either as a model name would have the service
    /// answer with whatever it substitutes for a name it has never heard of.
    #[test]
    fn a_model_that_names_nothing_leaves_the_session_its_own() {
        for line in [
            "model:\n",
            "model: '   '\n",
            "model: inherit\n",
            "model: Inherit\n",
        ] {
            let parsed = parse_frontmatter(&format!("---\nname: n\ndescription: d\n{line}---\n"))
                .expect("parses");
            assert_eq!(parsed.model, None, "model line: {line:?}");
        }
    }

    /// The five words this program enumerates, in either case, and a word naming none of them.
    /// An unrecognised word must not become a request field, and the skill is still loaded, so the
    /// level has to come back as absence rather than as a guess.
    #[test]
    fn an_effort_word_that_names_no_level_leaves_the_session_its_own() {
        let level = |word: &str| {
            let front = parse_frontmatter(&format!(
                "---\nname: n\ndescription: d\neffort: {word}\n---\n"
            ))
            .expect("parses");
            let mut notices = Vec::new();
            let chosen = runs_as(&front, "SKILL.md", &mut notices);
            (chosen.effort, notices)
        };

        assert_eq!(level("max").0, Some(Effort::Max));
        assert_eq!(level("Low").0, Some(Effort::Low));
        for word in ["highest", "0.5", "none", "very high"] {
            let (chosen, notices) = level(word);
            assert_eq!(chosen, None, "'{word}' was read as a level");
            assert_eq!(notices.len(), 1, "'{word}' said nothing: {notices:?}");
            assert!(
                notices[0].message.contains(word) && notices[0].message.contains("SKILL.md"),
                "the notice names neither the word nor the file: {}",
                notices[0].message
            );
        }
        assert!(level("max").1.is_empty(), "a level said something");
    }

    /// A description is a sentence, and sentences contain colons. Splitting on the last one, or
    /// refusing the line, would mangle exactly the text the planner decides from.
    #[test]
    fn a_value_may_contain_the_separator() {
        let text = "---\nname: n\ndescription: use this: always\n---\n";
        let parsed = parse_frontmatter(text).expect("parses");
        assert_eq!(parsed.description, "use this: always");
    }

    /// A description says when to use a skill, so it runs to a sentence or two and people wrap
    /// it. Every real skill file in this repository does, and reading only the first line of one
    /// left the value empty and the skill silently dropped.
    #[test]
    fn a_value_wrapped_over_several_lines_is_one_value() {
        let text = "---\nname: n\ndescription:\n  'Check the specs, clause by clause. Runs the\n  \
                    mechanical pass. Triggers on: check spec, spec drift.'\nargument-hint: '[x]'\n---\n";
        let parsed = parse_frontmatter(text).expect("parses");
        assert_eq!(
            parsed.description,
            "Check the specs, clause by clause. Runs the mechanical pass. Triggers on: check \
             spec, spec drift."
        );
    }

    /// The hint is for the person typing the skill's name. A file without one has none, and an
    /// empty one is none rather than a blank drawn after the name.
    #[test]
    fn an_argument_hint_is_read_when_the_file_has_one() {
        let with =
            parse_frontmatter("---\nname: n\ndescription: d\nargument-hint: '[a] <b>'\n---\n")
                .expect("parses");
        assert_eq!(with.argument_hint.as_deref(), Some("[a] <b>"));
        let without = parse_frontmatter("---\nname: n\ndescription: d\n---\n").expect("parses");
        assert_eq!(without.argument_hint, None);
        let empty = parse_frontmatter("---\nname: n\ndescription: d\nargument-hint:\n---\n")
            .expect("parses");
        assert_eq!(empty.argument_hint, None);
    }

    /// A wrapped sentence contains colons, and a continuation line is not a declaration. Reading
    /// one as a key ends the value early and puts half a sentence in the prompt.
    #[test]
    fn a_continuation_line_holding_a_colon_does_not_start_a_new_key() {
        let text = "---\nname: n\ndescription: use it when\n  this holds: always\n---\n";
        let parsed = parse_frontmatter(text).expect("parses");
        assert_eq!(parsed.description, "use it when this holds: always");
    }

    /// The quotes are YAML's, put there so a value may open with a character that would otherwise
    /// mean something. Leaving them in shows the planner an apostrophe at each end.
    #[test]
    fn the_quotes_around_a_scalar_are_not_part_of_it() {
        let text = "---\nname: 'n'\ndescription: \"say when\"\n---\n";
        let parsed = parse_frontmatter(text).expect("parses");
        assert_eq!(parsed.name, "n");
        assert_eq!(parsed.description, "say when");
    }

    /// `>` folds and `|` keeps its newlines, which is the whole difference between the two and
    /// the only reason a file would choose one.
    #[test]
    fn a_folded_block_becomes_one_line_and_a_literal_block_keeps_its_own() {
        let folded = parse_frontmatter("---\nname: n\ndescription: >\n  one\n  two\n---\n");
        assert_eq!(folded.expect("parses").description, "one two");
        let literal = parse_frontmatter("---\nname: n\ndescription: |\n  one\n  two\n---\n");
        assert_eq!(literal.expect("parses").description, "one\ntwo");
    }

    /// A blank line inside a literal block is a newline the file asked for, so a paragraph break
    /// survives. Blank lines around the block are layout and do not add newlines to the value,
    /// and a folded block has no paragraph break to keep.
    #[test]
    fn a_blank_line_inside_a_literal_block_is_kept() {
        let literal =
            parse_frontmatter("---\nname: n\ndescription: |\n  one\n\n  two\n\nother: x\n---\n");
        assert_eq!(literal.expect("parses").description, "one\n\ntwo");
        let folded = parse_frontmatter("---\nname: n\ndescription: >\n  one\n\n  two\n---\n");
        assert_eq!(folded.expect("parses").description, "one two");
    }

    /// The planner already has the name and the description. Sending them again spends context
    /// on what it used to make the call.
    #[test]
    fn the_body_is_everything_after_the_closing_marker() {
        let text = "---\nname: n\ndescription: d\n---\n\nline one\nline two\n";
        assert_eq!(body_after_frontmatter(text), "line one\nline two\n");
    }

    /// A body may itself contain a horizontal rule. Stripping at the last marker rather than the
    /// first would swallow the part of the skill above it.
    #[test]
    fn a_marker_inside_the_body_is_left_alone() {
        let text = "---\nname: n\ndescription: d\n---\nabove\n\n---\n\nbelow\n";
        assert_eq!(body_after_frontmatter(text), "above\n\n---\n\nbelow\n");
    }

    /// A file with no frontmatter is not a skill, but asking for its body must still return the
    /// file rather than nothing, since a caller uses this to decide what to show.
    #[test]
    fn a_file_without_frontmatter_is_all_body() {
        assert_eq!(body_after_frontmatter("just prose\n"), "just prose\n");
        assert_eq!(body_after_frontmatter(""), "");
    }

    /// A skill body is instructions this agent follows, so loading the nearest name to the one
    /// asked for puts a file nobody chose into the context. A miss is cheap by comparison: the
    /// planner is told there is no such skill and picks from the names it was advertised.
    #[test]
    fn a_name_one_character_off_selects_no_skill() {
        let mut catalogue = Catalogue::default();
        catalogue.insert(Skill {
            name: "commit-style".to_string(),
            description: "how commit messages are written here".to_string(),
            argument_hint: None,
            origin: "SKILL.md".to_string(),
            source: Source::Home,
            runs_as: RunsAs::default(),
            unread: Vec::new(),
            body: Labelled::trusted("sign them".to_string()),
        });

        assert!(
            catalogue.get("commit-style").is_some(),
            "the exact name missed"
        );
        // A character too many, a character too few, the other case, a prefix, and a name that
        // contains the real one. Every spelling a nearest-match lookup would answer.
        for near in [
            "commit-styles",
            "commit-styl",
            "Commit-Style",
            "commit",
            "the commit-style skill",
        ] {
            assert!(
                catalogue.get(near).is_none(),
                "'{near}' selected a skill nobody named"
            );
        }
    }
}
