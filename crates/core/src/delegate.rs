//! Delegated agents: a second planner, narrower than the first.
//!
//! A processor holds no capabilities at all, and the reason is written down in
//! [`crate::processor`]: a processor with one tool would be a second planner with untrusted
//! content in its context, which is the thing this design refuses. A delegate is the other half
//! of that sentence. It holds capabilities and it holds no untrusted content: it quarantines what
//! it may not read exactly as the first planner does, and gets a reference back. So the objection
//! to a processor with tools is not an objection to this.
//!
//! What it buys is a context nothing has to be told twice. A planner that runs the build reads
//! the whole log; one that asks a delegate to run the build is told what failed. The work happens
//! either way, and only one of them spends the conversation on it.
//!
//! Three things are fixed before a delegate exists, and none of them by the model:
//!
//! - **Its capabilities**, which are its kind's narrowed by the parent's. Delegation
//!   redistributes authority and never creates it, so a delegate can hold nothing the turn that
//!   spawned it did not already hold.
//! - **Its prompt**, which is a constant per kind. The planner names a kind and cannot describe
//!   one, so there is no sentence it can write that changes what a delegate is.
//! - **Its bound**, which its definition may choose beneath its kind's ceiling and the call never
//!   can. Nobody is watching a delegate the way a person watches a turn, and the thing being
//!   bounded is futility rather than danger.
//!
//! It still asks. Every write and every run a delegate performs passes the same gates with the
//! same single-use endorsements, so a person sees the path and the diff whoever proposed them.
//! What a delegate saves is context, never approval.

use crate::capability::{Capability, CapabilitySet, ServerAlias};
use crate::label::Confidentiality;
use crate::permissions::Permissions;

/// A kind of delegate: what it may hold, and how long it may go on.
///
/// Enumerated here rather than configured, so the planner selects from a list the driver wrote
/// and cannot describe a delegate of its own. That is the smaller half of the same decision the
/// prompt makes: a name that matches nothing in this enum resolves to nothing and the call is
/// refused, so there is no spelling of `kind` that reaches a capability set nobody chose.
///
/// The three are declared narrowest first and ordered by that, each holding what the one before
/// it holds and more, so the meet of two of them is the narrower one rather than a set no kind
/// goes by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Reads, lists, searches, and hands quarantined files to processors. Writes nothing and
    /// runs nothing.
    ///
    /// The shape whose value is mostly independence: a question about a tree, answered without
    /// the tree arriving in the asker's context.
    Reader,
    /// A reader that may also run programs and ask a language server, so it can build and test.
    ///
    /// Writes nothing, which is what makes it worth having separately: reporting that the tests
    /// fail does not need permission to change them, and a build log is the single most
    /// context-expensive thing a turn reads.
    Checker,
    /// A checker that may also write files.
    ///
    /// A whole sub-task, done somewhere else. Every write it makes is still shown to a person as
    /// a diff, and the endorsement is minted for the path they saw.
    Worker,
}

impl Kind {
    /// Every kind, in the order the planner is told about them: narrowest first.
    pub const NAMES: [&'static str; 3] = ["reader", "checker", "worker"];

    /// The kind this name selects, or nothing.
    ///
    /// A selection, not a lookup of anything the model wrote into a path or a table key. The set
    /// is this array, so a name naming a traversal, a capability, or anything else at all
    /// matches nothing.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "reader" => Some(Self::Reader),
            "checker" => Some(Self::Checker),
            "worker" => Some(Self::Worker),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reader => "reader",
            Self::Checker => "checker",
            Self::Worker => "worker",
        }
    }

    /// What this kind asks to hold, before the parent's own set narrows it.
    ///
    /// Every kind holds the one for reaching the network, because a planner *is* a model call and
    /// the request out is egress like any other. It is not a tool a delegate can point anywhere:
    /// nothing in any kind's tool set reaches it, so what it buys is the driver's ability to ask
    /// the endpoint on this delegate's behalf. A kind without it is a kind that cannot think.
    ///
    /// A language server goes with running programs, and never without: starting one runs the
    /// project's build tooling (LSP-5), so a kind that may not run a program may not start one.
    pub fn capabilities(self) -> CapabilitySet {
        match self {
            Self::Reader => CapabilitySet::from_iter([Capability::WebFetch, Capability::FileRead]),
            Self::Checker => CapabilitySet::from_iter([
                Capability::WebFetch,
                Capability::FileRead,
                Capability::ShellExec,
                Capability::LanguageServer,
            ]),
            Self::Worker => CapabilitySet::from_iter([
                Capability::WebFetch,
                Capability::FileRead,
                Capability::FileWrite,
                Capability::ShellExec,
                Capability::LanguageServer,
            ]),
        }
    }

    /// Whether this kind may hold the MCP servers its parent holds.
    ///
    /// A worker only. What a server's tool does is the server's to say, so a call to one may write
    /// or run anything, and a worker is the one kind already let write and run. No kind names a
    /// server, since which servers a session reached is not known until it starts; a definition
    /// may name which of them its worker keeps.
    pub fn holds_servers(self) -> bool {
        matches!(self, Self::Worker)
    }

    /// How many rounds of tool calls this kind may make before it has to answer, where its
    /// definition named no number of its own.
    ///
    /// Bounded for every kind, and the bound rises with what the kind can do rather than with
    /// how much anybody trusts it: a delegate that may not write has less to be part-way
    /// through. Not a safety property, exactly as the turn's own bound is not: a gate refuses on
    /// the last round what it refuses on the first.
    pub fn rounds(self) -> usize {
        match self {
            Self::Reader => 60,
            Self::Checker => 80,
            Self::Worker => 120,
        }
    }

    /// The most rounds a definition of this kind may ask for.
    ///
    /// Twice the kind's own, so a definition written for a long sub-task has room to finish it,
    /// and never more than a turn nobody is watching may make, since a delegate is one: the
    /// worker's is that turn's bound. A ceiling rather than whatever the file says, for the reason
    /// every kind is bounded at all.
    pub fn most_rounds(self) -> usize {
        match self {
            Self::Reader => 120,
            Self::Checker => 160,
            Self::Worker => 200,
        }
    }

    /// What to tell the planner this kind is for, in the tool's own schema.
    pub fn purpose(self) -> &'static str {
        match self {
            Self::Reader => {
                "reads, lists, searches and runs processors; writes nothing and runs nothing"
            }
            Self::Checker => {
                "a reader that may also run programs and ask a language server, so it can build, \
                 test and lint; writes nothing"
            }
            Self::Worker => {
                "a checker that may also write files and call the tools of the MCP servers you \
                 may, so it can finish a sub-task"
            }
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The tools no delegate is ever offered, whatever it holds.
///
/// Named rather than derived, because each is left out for a reason of its own rather than for
/// want of a capability: `fetch_url` because every kind holds the capability for reaching the
/// network so the driver can make its model call (and `download_url` for the same reason, since it
/// reaches the network and writes a file on top), `apply_checkout` because bringing a delegate's
/// work into the person's tree is the turn's to ask for and not that delegate's, and the rest
/// because their audience is the person watching the turn (`request_path` among them: a path a
/// planner's sub-task asked for is reach the person never set up). The list is here rather than beside the tool table so that a
/// definition naming one of them is answered by the same set the tool list is built from.
///
/// `spawn_agent` is not here. Whether a delegate may delegate is a question about where it sits,
/// not about what it is, so it is answered by [`MAX_DEPTH`] rather than by name.
pub const NEVER_DELEGATED: [&str; 8] = [
    "ask_user",
    "request_path",
    "apply_checkout",
    "todo_write",
    "schedule_next",
    "fetch_url",
    "download_url",
    "vet_content",
];

/// The capability a delegate needs before it is offered this tool, or `None` where the name is
/// not a tool a delegate is ever offered.
///
/// The rule the delegate tool list is built from, kept here so the kernel can apply it to the
/// tools a definition named without the tool table being in scope. The two are held to each
/// other by a test rather than by a comment, in both directions: a tool missing from this match
/// and a name here that is no longer a tool both fail it.
///
/// **A name this does not recognise selects nothing**, rather than falling to the weakest
/// capability any tool asks for. A definition written for another agent names that agent's tools,
/// and a fallback would start a delegate holding something for a list of names none of which is
/// a tool it gets. Answering `None` is what lets the caller say so.
pub fn gating_capability(tool: &str) -> Option<Capability> {
    match tool {
        "write_file" | "edit_file" => Some(Capability::FileWrite),
        "run" | "read_output" | "job_output" => Some(Capability::ShellExec),
        // LSP-9: asking a server is its own grant, so a delegate holding file reads has not
        // thereby been given one.
        "lsp" => Some(Capability::LanguageServer),
        "read_file" | "list_files" | "search" | "read_git" | "spawn_processor" | "load_skill" => {
            Some(Capability::FileRead)
        }
        // A delegate is a model call, and every kind holds this so it can make its own. What a
        // delegate it spawns may hold is its own set narrowed again, so this adds nothing to it.
        "spawn_agent" => Some(Capability::WebFetch),
        _ => None,
    }
}

/// What a definition holds whatever it named, and for the same reason in both cases.
///
/// Reaching the network, because a planner is a model call and one that cannot make a request
/// cannot think. And reading, because a write is a read of the file followed by a write of it, so
/// a definition naming `write_file` alone and holding no read would be a delegate whose one tool
/// is refused on every call. Neither is a widening: every kind holds both already, and no tool a
/// delegate is offered reaches the first, so what a definition narrows is what remains.
const HELD_WHATEVER_IT_NAMED: [Capability; 2] = [Capability::WebFetch, Capability::FileRead];

/// What a replacement is confined to: its own `tools:` line, or the servers it selects, met with
/// the one it replaced.
///
/// `None` means everything, so it is the wider of the two and loses to any list at all,
/// including to an empty one. Two lists meet name by name in the replacement's order, so a
/// replacement written for another agent, whose names are that agent's vocabulary, comes out with
/// nothing in common rather than with either side's list.
fn meet(asked: Option<&[String]>, replaced: Option<&[String]>) -> Option<Vec<String>> {
    match (asked, replaced) {
        (None, None) => None,
        (Some(only), None) | (None, Some(only)) => Some(only.to_vec()),
        (Some(asked), Some(replaced)) => Some(
            asked
                .iter()
                .filter(|&tool| replaced.contains(tool))
                .cloned()
                .collect(),
        ),
    }
}

/// What a tool a definition named reaches for a delegate, or nothing.
///
/// A name no delegate is ever offered reaches nothing here whatever capability it would otherwise
/// need, because the tool is left out by name rather than for want of one. So is a name that is
/// not a tool. Both are reported by [`Definition::tools_beyond_its_kind`].
fn reachable_by(tool: &str) -> Option<Capability> {
    if NEVER_DELEGATED.contains(&tool) {
        return None;
    }
    gating_capability(tool)
}

/// One kind of delegate as the driver resolved it for this turn.
///
/// A definition is what a name selects. Three of them are this program's own and correspond one
/// to one with a [`Kind`]; the rest came from a file somebody vouched for, and each of those
/// names a kind rather than describing one. That is the whole of the difference between this and
/// a configuration file that hands out authority: a definition may narrow what its kind holds and
/// there is no spelling of it that widens anything, so a checked-in file can choose what a
/// delegate is for and can never choose what it may do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    name: String,
    description: String,
    kind: Kind,
    /// The model requested for this delegate, where the definition named one.
    ///
    /// `None` leaves the delegate on the model the spawning turn is running on.
    model: Option<String>,
    /// The effort level requested for this delegate, where the definition named one.
    ///
    /// `None` leaves the delegate on the level the spawning turn runs at. The word is one of the
    /// five this program enumerates, settled where the file was read, because this crate names no
    /// level type: a word naming none of them is reported there and never reaches here.
    effort: Option<String>,
    /// The tools the definition asked for, where it asked for any.
    ///
    /// `None` is the kind's own set. `Some` is a narrowing and only a narrowing: what it selects
    /// is intersected with the kind's, so a name the kind does not reach is a name this
    /// definition loaded without.
    tools: Option<Vec<String>>,
    /// The skills the definition asked to be offered, where it asked for any.
    ///
    /// `None` is every skill the turn found. `Some` selects out of those by name, so a name the
    /// turn did not find selects nothing: a skill is guidance a planner may load, and a list of
    /// them chooses what a delegate is told about rather than anything it may do.
    skills: Option<Vec<String>>,
    /// The MCP servers the definition asked to call, where it asked for any.
    ///
    /// `None` is every server the parent holds, unless a `tools:` line was written, which names
    /// no server's tool. `Some` selects out of the parent's by alias, so a name the parent holds
    /// no grant for selects nothing, and a kind holding no server holds none whatever this says.
    servers: Option<Vec<String>>,
    /// The rounds the definition asked for, where it asked for a number.
    ///
    /// `None` is the kind's own bound. Held to the kind's ceiling where it is read rather than
    /// where it is set, so a replacement loaded as a narrower kind is held to that kind's.
    rounds: Option<usize>,
    /// Whether a run under it keeps a memory, a file in the working directory named after it.
    ///
    /// Set only for a name a path segment can be made of, which is decided where the file is read.
    /// Taken over with the body by a later definition of the same name, since a memory changes
    /// what a run knows and never what it may do.
    memory: bool,
    /// Whether it asked for a delegate of it to be given a checkout of its own.
    ///
    /// Met with the one it replaced, as the kind is, so a later definition can add one and cannot
    /// take one away. Held to the kind where it is read rather than where it is set, so one met
    /// down to a `reader` has none.
    checkout: bool,
    /// The path patterns its `writes:` lines limited a delegate's file writes to, one list per
    /// line that limited it (DELEGATE-28).
    ///
    /// Empty is no limit. A path must be inside every list, so a replacement keeps the lists of
    /// the definition it replaced beside its own and a later definition can add a limit and
    /// cannot take one away. Kept as written: reading a pattern needs the host's separators and
    /// case folding, which the kernel has from the rules in force when a delegate is started.
    writes: Vec<Vec<String>>,
    /// The standing part of what a delegate of this name is told about itself.
    ///
    /// Empty where the file had no body. Carried rather than read: the kernel never branches on
    /// it, and what makes it admissible in a planner's context at all is that it arrived through
    /// the trusted-content gate.
    prompt: String,
    /// Where it came from, for the audit trail. The driver's own words for the built-in ones.
    origin: String,
}

impl Definition {
    /// The definition a kind is, with no file behind it.
    pub fn of_kind(kind: Kind) -> Self {
        Self {
            name: kind.as_str().to_string(),
            description: kind.purpose().to_string(),
            kind,
            model: None,
            effort: None,
            tools: None,
            skills: None,
            servers: None,
            rounds: None,
            memory: false,
            checkout: false,
            writes: Vec::new(),
            prompt: String::new(),
            origin: "built-in".to_string(),
        }
    }

    /// A definition read out of a file, after the gate that decided the file could be read.
    pub fn from_file(
        name: impl Into<String>,
        description: impl Into<String>,
        kind: Kind,
        tools: Option<Vec<String>>,
        prompt: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            kind,
            model: None,
            effort: None,
            tools,
            skills: None,
            servers: None,
            rounds: None,
            memory: false,
            checkout: false,
            writes: Vec::new(),
            prompt: prompt.into(),
            origin: origin.into(),
        }
    }

    /// What the planner names to select it.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What the planner decides from: when to use this rather than another.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The enumerated kind it names. Never one it describes.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The tools it asked for, before the kind's own set narrows them.
    pub fn tools(&self) -> Option<&[String]> {
        self.tools.as_deref()
    }

    /// The model requested for this delegate, where the definition named one.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Request a particular model for this delegate.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// The effort level requested for this delegate, where the definition named one.
    pub fn effort(&self) -> Option<&str> {
        self.effort.as_deref()
    }

    /// Request a particular effort level for this delegate. The caller has settled the word
    /// against the five levels.
    pub fn with_effort(mut self, effort: impl Into<String>) -> Self {
        self.effort = Some(effort.into());
        self
    }

    /// The skills it asked to be offered, before the turn's own catalogue narrows them.
    pub fn skills(&self) -> Option<&[String]> {
        self.skills.as_deref()
    }

    /// Offer this delegate only the skills of these names that the turn found.
    pub fn with_skills(mut self, skills: Vec<String>) -> Self {
        self.skills = Some(skills);
        self
    }

    /// The MCP servers it asked to call, before its kind and the parent's grants narrow them.
    pub fn servers(&self) -> Option<&[String]> {
        self.servers.as_deref()
    }

    /// Let this delegate call only the servers of these aliases that its parent holds.
    pub fn with_servers(mut self, servers: Vec<String>) -> Self {
        self.servers = Some(servers);
        self
    }

    /// How many rounds its delegate may make: the number it asked for held to its kind's
    /// ceiling, or its kind's own where it asked for none.
    pub fn rounds(&self) -> usize {
        match self.rounds {
            Some(asked) => asked.min(self.kind.most_rounds()),
            None => self.kind.rounds(),
        }
    }

    /// The number it asked for where that is above its kind's ceiling, for whoever wrote it to
    /// be told the delegate is held to less.
    pub fn rounds_beyond_its_kind(&self) -> Option<usize> {
        self.rounds.filter(|&asked| asked > self.kind.most_rounds())
    }

    /// Ask for this many rounds, which its kind's ceiling still holds.
    pub fn with_rounds(mut self, rounds: usize) -> Self {
        self.rounds = Some(rounds);
        self
    }

    /// Whether a run under it keeps a memory.
    pub fn keeps_memory(&self) -> bool {
        self.memory
    }

    /// Keep a memory for runs under it. The caller has held the name to a slug.
    pub fn with_memory(mut self) -> Self {
        self.memory = true;
        self
    }

    /// Whether a delegate of it is given a checkout of its own, which a `reader` never is.
    pub fn asks_for_checkout(&self) -> bool {
        self.checkout && self.kind != Kind::Reader
    }

    /// Whether it asked for a checkout and is a `reader`, for whoever wrote it to be told it
    /// loads without one.
    pub fn checkout_beyond_its_kind(&self) -> bool {
        self.checkout && self.kind == Kind::Reader
    }

    /// Give a delegate of it a checkout of its own.
    pub fn with_checkout(mut self) -> Self {
        self.checkout = true;
        self
    }

    /// The lists of path patterns a delegate of it may write inside, empty where it is limited
    /// to none (DELEGATE-28).
    pub fn write_limits(&self) -> &[Vec<String>] {
        &self.writes
    }

    /// Limit a delegate of it to writing the files these patterns cover. The caller has split the
    /// line into patterns; an empty list is a limit that covers nothing.
    pub fn with_writes(mut self, patterns: Vec<String>) -> Self {
        self.writes.push(patterns);
        self
    }

    /// The standing instruction, empty where the file had no body.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// What this definition asks to hold, before the parent's own set narrows it.
    ///
    /// The kind's, and where the definition named tools, only those of the kind's that one of
    /// those tools reaches. Reaching the network survives every narrowing, because a planner is a
    /// model call and a delegate that cannot make one cannot think; no tool reaches it, so
    /// keeping it grants nothing a tool list could spend.
    ///
    /// Read off this definition's own two fields, which is why [`Definitions::insert`] narrows
    /// those rather than keeping a ceiling beside them: a replacement that has been cut down to
    /// the kind and the tools of the one it replaced is one nothing else has to know was.
    pub fn capabilities(&self) -> CapabilitySet {
        let held = self.kind.capabilities();
        let Some(tools) = self.tools.as_deref() else {
            return held;
        };
        let named: CapabilitySet = held
            .iter()
            .filter(|capability| {
                HELD_WHATEVER_IT_NAMED.contains(capability)
                    || tools
                        .iter()
                        .any(|tool| reachable_by(tool).as_ref() == Some(capability))
            })
            .collect();
        // A language server goes with running programs and never without (DELEGATE-4, LSP-5):
        // `lsp` and `run` are selected independently by a `tools:` line, so a definition naming
        // the first alone is cut down to neither.
        if named.contains(&Capability::ShellExec) {
            named
        } else {
            named
                .iter()
                .filter(|capability| *capability != Capability::LanguageServer)
                .collect()
        }
    }

    /// The servers this definition selects, or `None` for every one its parent holds.
    ///
    /// An `mcpServers:` line where it wrote one. Otherwise a `tools:` line selects none, since it
    /// names this program's tools and no server's, so a definition that wrote one asked for none.
    fn servers_named(&self) -> Option<&[String]> {
        match (self.servers.as_deref(), self.tools.as_deref()) {
            (Some(named), _) => Some(named),
            (None, Some(_)) => Some(&[]),
            (None, None) => None,
        }
    }

    /// Whether a run of this definition holds the grant for this server, where its parent does.
    ///
    /// Where its kind holds servers and the definition selects this one. The alias is compared as
    /// written, which is the one comparison a definition's words take part in, and they may:
    /// the file came from a source somebody vouched for.
    pub fn holds_server(&self, alias: &ServerAlias) -> bool {
        self.kind.holds_servers()
            && self
                .servers_named()
                .is_none_or(|named| named.iter().any(|name| name == alias.as_str()))
    }

    /// What a run of this definition holds out of `parent`, the set of the run it is carved from.
    ///
    /// What [`Definition::capabilities`] asks for that `parent` holds, and every server `parent`
    /// holds that [`Definition::holds_server`]. Only ever a part of `parent`, so a definition
    /// takes away and never adds, and a server nobody put to the person for the parent is one
    /// this run cannot hold either, whatever its definition named.
    pub fn held_out_of(&self, parent: &CapabilitySet) -> CapabilitySet {
        let wanted = self.capabilities();
        parent
            .iter()
            .filter(|capability| match capability {
                Capability::McpCall(alias) => self.holds_server(alias),
                other => wanted.contains(other),
            })
            .collect()
    }

    /// The servers it named where its kind calls none, for the trail to say what was dropped.
    pub fn servers_beyond_its_kind(&self) -> &[String] {
        match self.servers.as_deref() {
            Some(named) if !self.kind.holds_servers() => named,
            _ => &[],
        }
    }

    /// The servers it named that `parent` holds no grant for, for the trail to say what was
    /// dropped.
    ///
    /// A name here is a server this session did not reach or one somebody's narrowing already
    /// took from the parent, and the run holds neither: a definition selects out of its parent's
    /// grants and cannot put a new one to anybody.
    pub fn servers_not_held(&self, parent: &CapabilitySet) -> Vec<&str> {
        if !self.kind.holds_servers() {
            return Vec::new();
        }
        self.servers
            .iter()
            .flatten()
            .filter(|name| !parent.contains(&Capability::McpCall(ServerAlias::new(name.as_str()))))
            .map(String::as_str)
            .collect()
    }

    /// The tools it named that a delegate of its kind does not get, for the trail to say what
    /// was dropped.
    ///
    /// Three ways a name lands here, and the notice matters most for the third: a name that is
    /// not a tool at all. A definition written for another agent names that agent's vocabulary,
    /// and without this it would start a delegate with no tools and nothing said anywhere about
    /// why.
    pub fn tools_beyond_its_kind(&self) -> Vec<&str> {
        let held = self.kind.capabilities();
        let Some(tools) = self.tools.as_deref() else {
            return Vec::new();
        };
        tools
            .iter()
            .filter(|tool| !reachable_by(tool).is_some_and(|needs| held.contains(&needs)))
            .map(String::as_str)
            .collect()
    }
}

/// What [`Definitions::insert`] did with a definition.
///
/// Reported rather than returned as a bare yes, because two of these are things whoever wrote
/// the file has to be told: a name that could not be taken, and a definition admitted for less
/// than it asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admitted {
    /// It is in the set as written.
    AsWritten,
    /// It is in the set, cut down to the definition of the same name it replaced.
    Narrowed(Narrowing),
    /// Not in the set: its name is one of the kinds' own.
    Refused,
}

/// What a replacement asked for and did not get, for whoever wrote it to be told.
///
/// The words are the loader's: this is the kernel, and what it has to hand over is which of its
/// parts moved rather than a sentence about it. [`Admitted::Narrowed`] is answered only where one
/// of them did, so a value of this always has something to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Narrowing {
    /// The kind the file named.
    pub named: Kind,
    /// The kind it is loaded as, which is the narrower of its own and the one it replaced.
    pub loaded: Kind,
    /// The tools it is confined to, where the one it replaced named fewer than it did.
    ///
    /// `None` where its own `tools:` line stands, which includes the case of neither file
    /// naming one. An empty list is the two lists having no name in common.
    pub confined_to: Option<Vec<String>>,
    /// The MCP servers it is confined to, where the one it replaced selected fewer than it did.
    ///
    /// `None` where what it selected stands, and wherever it is loaded as a kind that calls no
    /// server, since the kind has already said that. An empty list is no server at all.
    pub servers_confined_to: Option<Vec<String>>,
    /// Whether its delegate is given a checkout it did not ask for, because the one it replaced
    /// asked. Never where it is loaded as a `reader`, which is given none.
    pub given_a_checkout: bool,
    /// The write limits it is held to that its own file did not write, because the one it
    /// replaced did (DELEGATE-28). Empty where its own stand alone.
    pub writes_confined_to: Vec<Vec<String>>,
    /// Where the definition that cut it down came from.
    pub replaced: String,
}

/// The kinds of delegate this turn can select from.
///
/// Fixed before the turn and never added to while it runs. The three the program wrote are always
/// here; anything else arrived from a file that passed the trusted-content gate, so nothing an
/// attacker wrote is in the set a planner's name is compared against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definitions {
    entries: Vec<Definition>,
}

impl Default for Definitions {
    /// The three kinds and nothing else, which is what a turn that found no definition files has.
    fn default() -> Self {
        Self {
            entries: Kind::NAMES
                .iter()
                .map(|name| Definition::of_kind(Kind::from_name(name).expect("enumerated")))
                .collect(),
        }
    }
}

impl Definitions {
    /// Add one, replacing any of the same name and never widening what that name reaches.
    ///
    /// Later wins, and discovery visits the built-in kinds, then the user's own directory, then
    /// the project, so the project has the last word. That is [INSTR-4]'s rule and the same one
    /// the skill catalogue follows.
    ///
    /// [INSTR-4]: https://github.com/brave/bravebot/blob/main/docs/specs/instructions.md
    ///
    /// **Last word about what a name is for, and never about what it may do.** A replacement
    /// takes over the description, the body, the model, the skills, the rounds and the memory,
    /// and is cut down on the three fields that decide what it may do: it is loaded as the
    /// narrower of the two kinds, its `tools:` line is met with the one it replaced, and so are
    /// the servers it selects. So a project cannot turn a `reader` a person wrote into a `worker`, and cannot
    /// hand back a tool or a server that person's own definition had taken away. Widening it
    /// would make the checked-in file the author of authority rather than the person who
    /// vouched for the checkout, which is the sentence [`Definition`] is built around, and the
    /// vouch that let the file be read at all is a decision about the project rather than about
    /// this name.
    ///
    /// The skills are taken over rather than met because a skill is guidance, as the body is: a
    /// list of them chooses which of the turn's own skills a delegate is told about, and the turn
    /// found every one of those whichever file named them.
    ///
    /// The memory is taken over for the same reason as the body: it changes what a run knows and
    /// never what it may do, since keeping one adds no tool.
    ///
    /// A checkout is met as the kind is: either asking gives one, so a later definition can add
    /// one and cannot take one away (CHECKOUT-2).
    ///
    /// The write limits are kept whole beside the replacement's own, so a path has to be inside
    /// both: a later definition can add a limit and cannot take one away (DELEGATE-28).
    ///
    /// The rounds are taken over because a bound is not authority: a gate refuses on the last
    /// round what it refuses on the first. They are still held to the ceiling of the kind the
    /// replacement is loaded as, so a narrower kind brings its lower ceiling with it.
    ///
    /// **The fields rather than a ceiling beside them**, so that what a definition holds is
    /// still read off the definition, and the trail a delegate leaves names the kind, the tools
    /// and the servers it actually got. What it asked for and did not get comes back instead, so
    /// whoever wrote it is told rather than left to find the narrowing by running it.
    ///
    /// A name one of the three kinds already goes by is **refused**, so the three the program
    /// wrote are in every set and a `reader` is a reader wherever a session runs. A file free to
    /// claim one would be a file renaming the narrowest kind to the widest, and a planner
    /// choosing the narrowest thing that can do the job would be choosing from a list whose order
    /// had stopped being true.
    pub fn insert(&mut self, mut definition: Definition) -> Admitted {
        if !Self::may_be_named(&definition.name) {
            return Admitted::Refused;
        }
        let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.name == definition.name)
        else {
            self.entries.push(definition);
            return Admitted::AsWritten;
        };

        // Taken against what the one being replaced was left holding rather than against what
        // its own file named, so a third definition of the same name meets both of the first
        // two rather than only the second.
        let named = definition.kind;
        let loaded = named.min(existing.kind);
        // Read before the tools are met, because a `tools:` line is itself a selection of no
        // server, and stored whole so the one this replaces reads the same afterwards.
        let asked_servers = definition.servers_named().map(<[String]>::to_vec);
        let servers = meet(asked_servers.as_deref(), existing.servers_named());
        let servers_confined_to = (loaded.holds_servers() && servers != asked_servers)
            .then(|| servers.clone().unwrap_or_default());
        let asked = definition.tools.take();
        let tools = meet(asked.as_deref(), existing.tools.as_deref());
        // `None` is the kind's whole set, so it differs from any list at all, and a replacement
        // that named no tools and inherited one has been confined as surely as one whose own
        // list was cut down.
        let confined_to = (tools != asked).then(|| tools.clone().unwrap_or_default());
        let given_a_checkout = existing.checkout && !definition.checkout && loaded != Kind::Reader;
        let replaced = existing.origin.clone();
        // The limits of the one replaced come first and are kept whole, so a third definition
        // meets both of the first two. Only those its own lines did not repeat are reported.
        let writes_confined_to: Vec<Vec<String>> = existing
            .writes
            .iter()
            .filter(|limit| !definition.writes.contains(limit))
            .cloned()
            .collect();
        let mut writes = existing.writes.clone();
        for limit in std::mem::take(&mut definition.writes) {
            if !writes.contains(&limit) {
                writes.push(limit);
            }
        }

        definition.kind = loaded;
        definition.writes = writes;
        definition.tools = tools;
        definition.servers = servers;
        definition.checkout |= existing.checkout;
        *existing = definition;

        if named == loaded
            && confined_to.is_none()
            && servers_confined_to.is_none()
            && !given_a_checkout
            && writes_confined_to.is_empty()
        {
            return Admitted::AsWritten;
        }
        Admitted::Narrowed(Narrowing {
            named,
            loaded,
            confined_to,
            servers_confined_to,
            given_a_checkout,
            writes_confined_to,
            replaced,
        })
    }

    /// Whether a definition may go by this name, which is any name but a kind's own.
    ///
    /// Asked by the loader before the kernel is, so a file claiming one is reported to whoever
    /// wrote it rather than dropped in silence.
    pub fn may_be_named(name: &str) -> bool {
        !Kind::NAMES.contains(&name)
    }

    /// The definition that name selects, or nothing.
    ///
    /// A selection out of a set the driver resolved, not a lookup of anything a model wrote into
    /// a path or a table key, so a name naming a traversal or a capability matches nothing.
    pub fn get(&self, name: &str) -> Option<&Definition> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Every name, in the order the planner is told about them.
    pub fn names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Definition> {
        self.entries.iter()
    }

    /// Keep no memory for any of them, where the directory a memory would be kept in is one no
    /// write could leave untrusted.
    pub fn keep_no_memory(&mut self) {
        for entry in &mut self.entries {
            entry.memory = false;
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// How far below the turn a delegate may sit.
///
/// A delegate this deep is offered no way to delegate, and the kernel refuses one that asks
/// anyway. Three because that is as deep as a sub-task of a sub-task needs to go to fan out its
/// reads.
pub const MAX_DEPTH: usize = 3;

/// How many delegates one turn's whole tree may hold, however they are arranged.
///
/// On the tree rather than on each node, so the bound on a turn's delegated work is this many
/// delegates at their kinds' rounds and not a product of fan-outs at every level. Four full
/// fan-outs of a single call.
pub const MAX_DELEGATES: u32 = 32;

/// Which delegate a record is about.
///
/// Minted by the kernel, one per delegate, counting from one in the order its parent spawned
/// them. It is the driver's own number and nothing a model wrote: several delegates run at once,
/// and an interface or a trail working out whose line it was holding would be taking that
/// decision from prose.
///
/// A path rather than a count, so a delegate's number says where it sits: `d2.1` is the first
/// delegate the turn's second spawned. Two nested delegates with one count each would both be
/// `d1`.
///
/// Small and copyable because everything carrying one is on a hot path, and ordered because the
/// order they were spawned in is the order anything showing them uses. A position counts from
/// one, so the zeros past a path's end sort a parent before its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DelegateId {
    path: [u32; MAX_DEPTH],
    depth: u8,
}

impl DelegateId {
    /// The `n`th delegate of a turn, counting from one.
    pub fn nth(n: u32) -> Self {
        let mut path = [0; MAX_DEPTH];
        path[0] = n;
        Self { path, depth: 1 }
    }

    /// The delegate a [`Display`](std::fmt::Display) spelling names, `d2.1` and the like, or nothing
    /// for any other text, a position of zero or a path deeper than [`MAX_DEPTH`].
    ///
    /// For a record that wrote the number down: what it reads back is checked as a path, not
    /// trusted as one.
    pub fn parse(text: &str) -> Option<Self> {
        let mut positions = text.strip_prefix('d')?.split('.');
        let first = positions.next()?;
        let mut id = Self::nth(Self::position_of(first)?);
        for position in positions {
            id = id.child(Self::position_of(position)?)?;
        }
        Some(id)
    }

    fn position_of(text: &str) -> Option<u32> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        text.parse().ok().filter(|position| *position > 0)
    }

    /// The `n`th delegate this one spawned, or nothing where this one sits at [`MAX_DEPTH`].
    pub fn child(self, n: u32) -> Option<Self> {
        let at = usize::from(self.depth);
        let mut path = self.path;
        *path.get_mut(at)? = n;
        Some(Self {
            path,
            depth: self.depth + 1,
        })
    }

    /// How far below the turn it sits: one for a delegate the turn itself spawned.
    pub fn depth(self) -> usize {
        usize::from(self.depth)
    }

    /// Its position among the delegates its parent spawned, counting from one.
    pub fn position(self) -> u32 {
        self.path[self.depth() - 1]
    }

    /// Whether this one was spawned by `other`, or by a delegate beneath it.
    pub fn is_beneath(self, other: Self) -> bool {
        self.depth > other.depth && self.path[..other.depth()] == other.path[..other.depth()]
    }
}

/// How a planner names one when it asks about it again, and how a record names the run it
/// belongs to.
///
/// Short because it is typed back into a tool call, and prefixed because a bare number in an
/// argument reads as a count of something.
impl std::fmt::Display for DelegateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "d{}", self.path[0])?;
        for position in &self.path[1..self.depth()] {
            write!(f, ".{position}")?;
        }
        Ok(())
    }
}

/// The places left in one turn's tree of delegates.
///
/// One count, shared by every run in the tree, so siblings running at once draw on the same
/// [`MAX_DELEGATES`] rather than each on its own. Carried in a [`DelegateSpec`] and nowhere
/// else, so a run can only reach its tree's count through the kernel that spawned it.
#[derive(Clone, Default)]
pub(crate) struct Tree(std::sync::Arc<std::sync::atomic::AtomicU32>);

impl Tree {
    /// Take one place, or nothing where the tree is full.
    pub(crate) fn claim(&self) -> bool {
        use std::sync::atomic::Ordering;
        let mut held = self.0.load(Ordering::SeqCst);
        loop {
            if held >= MAX_DELEGATES {
                return false;
            }
            match self
                .0
                .compare_exchange_weak(held, held + 1, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return true,
                Err(now) => held = now,
            }
        }
    }
}

impl Tree {
    /// Give back a place [`Tree::claim`] took, for a delegate that was approved and then did not
    /// start. Never below none.
    pub(crate) fn release(&self) {
        use std::sync::atomic::Ordering;
        let mut held = self.0.load(Ordering::SeqCst);
        while held > 0 {
            match self
                .0
                .compare_exchange_weak(held, held - 1, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return,
                Err(now) => held = now,
            }
        }
    }
}

/// The same tree, not the same count: two turns that each spawned one hold different trees.
impl PartialEq for Tree {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Tree {}

impl std::fmt::Debug for Tree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let held = self.0.load(std::sync::atomic::Ordering::SeqCst);
        write!(f, "Tree({held} of {MAX_DELEGATES})")
    }
}

/// Why a delegate the gate approved was not started, because of the checkout it was to run in.
///
/// A fixed category chosen by the driver where it refuses, so the trail says which refusal it was
/// without holding the sentence the planner was told. A directory a person opened is the only
/// text carried, and a person typed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutRefusal {
    /// `isolation` held something other than `"checkout"`.
    UnknownIsolation,
    /// A reader writes nothing, so a checkout separates it from nobody.
    Reader,
    /// The run already works in a checkout, which its delegates share.
    AlreadyInCheckout,
    /// The directory for checkouts would sit inside the working directory.
    InsideWorkingDirectory,
    /// A directory opened beside the working directory holds the working directory.
    OpenedHoldsWorkingDirectory(String),
    /// A directory opened beside the working directory holds the directory for checkouts.
    OpenedHoldsCheckouts(String),
    /// The directory for checkouts could not be made, or no trust rule can be keyed under it.
    DirectoryUnusable,
    /// The working directory is not a repository a checkout can be made of, or the policy or the
    /// repository itself would not have it read.
    RepositoryRefused,
}

impl CheckoutRefusal {
    /// The name a program reads this by, and the cause the trail records.
    pub fn name(&self) -> &'static str {
        match self {
            Self::UnknownIsolation => "unknown-isolation",
            Self::Reader => "reader-has-no-checkout",
            Self::AlreadyInCheckout => "already-in-a-checkout",
            Self::InsideWorkingDirectory => "inside-working-directory",
            Self::OpenedHoldsWorkingDirectory(_) => "opened-directory-holds-working-directory",
            Self::OpenedHoldsCheckouts(_) => "opened-directory-holds-checkouts",
            Self::DirectoryUnusable => "checkout-directory-unusable",
            Self::RepositoryRefused => "repository-refused",
        }
    }

    /// The directory a person opened that caused the refusal, where one did.
    pub fn directory(&self) -> Option<&str> {
        match self {
            Self::OpenedHoldsWorkingDirectory(dir) | Self::OpenedHoldsCheckouts(dir) => Some(dir),
            _ => None,
        }
    }
}

/// What the driver fixed about one delegate before it ran.
///
/// Only [`crate::policy::Policy::before_delegate`] constructs one, and nothing here can widen it
/// afterwards. The delegate never sees this value: what reaches it is a prompt chosen by its
/// kind, a task, and a tool list derived from the capabilities recorded here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateSpec {
    id: DelegateId,
    kind: Kind,
    /// The name the definition it came from goes by, which is the name the planner selected.
    ///
    /// The kind's own name where nothing was defined, so a session with no definition files
    /// describes its delegates exactly as it did before there were any.
    definition: String,
    /// The model this delegate was requested to run on, where its definition named one.
    model: Option<String>,
    /// The effort level this delegate was requested to run at, where its definition named one.
    effort: Option<String>,
    /// The tools its definition named, where it named any, already without the ones its kind
    /// does not reach.
    tools: Option<Vec<String>>,
    /// The skills its definition named, where it named any, not yet met with what the turn found.
    skills: Option<Vec<String>>,
    /// The standing part of what it is told about itself, from the definition that selected it.
    prompt: String,
    /// Whether the definition that selected it keeps a memory.
    memory: bool,
    /// Whether the definition that selected it gives it a checkout of its own, whatever the call
    /// asked.
    checkout: bool,
    /// The limits on the files it may write, read by the run that started it: those of every
    /// delegate above it and those of its own definition (DELEGATE-28).
    writes: Vec<Permissions>,
    task: String,
    capabilities: CapabilitySet,
    rounds: usize,
    /// The tree it belongs to, which is the one the delegates it spawns draw on.
    tree: Tree,
    /// What the context of the run that wrote its task held, which its own context starts
    /// holding: a task written by a planner shown private content may carry it.
    holds: Confidentiality,
}

impl DelegateSpec {
    pub(crate) fn new(
        id: DelegateId,
        definition: &Definition,
        task: impl Into<String>,
        capabilities: CapabilitySet,
        rounds: usize,
        tree: Tree,
    ) -> Self {
        let kind = definition.kind();
        let tools = definition.tools().map(|named| {
            named
                .iter()
                .filter(|tool| {
                    reachable_by(tool).is_some_and(|needs| kind.capabilities().contains(&needs))
                })
                .cloned()
                .collect()
        });
        Self {
            id,
            kind,
            definition: definition.name().to_string(),
            model: definition.model().map(str::to_string),
            effort: definition.effort().map(str::to_string),
            tools,
            skills: definition.skills().map(<[String]>::to_vec),
            prompt: definition.prompt().to_string(),
            memory: definition.keeps_memory(),
            checkout: definition.asks_for_checkout(),
            writes: Vec::new(),
            task: task.into(),
            capabilities,
            rounds,
            tree,
            holds: Confidentiality::Public,
        }
    }

    pub(crate) fn holding(mut self, held: Confidentiality) -> Self {
        self.holds = held;
        self
    }

    pub(crate) fn limited_to(mut self, limits: Vec<Permissions>) -> Self {
        self.writes = limits;
        self
    }

    pub(crate) fn write_limits(&self) -> &[Permissions] {
        &self.writes
    }

    pub(crate) fn holds(&self) -> Confidentiality {
        self.holds
    }

    /// Whether it may spawn a delegate of its own: it sits above [`MAX_DEPTH`], and a definition
    /// that named its tools named `spawn_agent` among them.
    ///
    /// Where it may not, it is offered no way to, and [`crate::policy::Policy::before_delegate`]
    /// refuses the call anyway.
    pub fn may_delegate(&self) -> bool {
        self.id.depth() < MAX_DEPTH && !self.named_out_delegating()
    }

    /// Whether its definition named the tools it may use and left `spawn_agent` out.
    pub(crate) fn named_out_delegating(&self) -> bool {
        self.tools
            .as_ref()
            .is_some_and(|named| !named.iter().any(|tool| tool == "spawn_agent"))
    }

    pub(crate) fn tree(&self) -> &Tree {
        &self.tree
    }

    /// What the planner named to get this, and what the person watching is shown.
    ///
    /// Content from a vouched file where a definition supplied it, which is the one reason it may
    /// be printed at all: a name nobody vouched for never entered the set this was selected from.
    pub fn definition(&self) -> &str {
        &self.definition
    }

    /// The model this delegate was requested to run on, where its definition named one.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// The effort level this delegate was requested to run at, where its definition named one.
    ///
    /// `None` is the level the spawning turn runs at.
    pub fn effort(&self) -> Option<&str> {
        self.effort.as_deref()
    }

    /// The tools its definition confined it to, where it named any, and `None` where it named
    /// none and the kind's whole set stands.
    ///
    /// Already narrowed to the kind's own reach here rather than where the tool list is built, so
    /// what a caller reads is a decision the kernel took.
    pub fn tools(&self) -> Option<&[String]> {
        self.tools.as_deref()
    }

    /// The skills its definition named, where it named any, and `None` where every skill the
    /// turn found is offered.
    pub fn skills(&self) -> Option<&[String]> {
        self.skills.as_deref()
    }

    /// The standing instruction its definition carried, empty where there was none.
    ///
    /// Bracketed by the driver's own words rather than replacing them: what a delegate may not do
    /// is said by its kind, and a file cannot tell one it may do what its kind cannot.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// Whether its definition keeps a memory, which the file of [`Self::definition`]'s name is.
    pub fn keeps_memory(&self) -> bool {
        self.memory
    }

    /// Whether its definition gives it a checkout of its own, so the planner cannot start it
    /// without one.
    pub fn asks_for_checkout(&self) -> bool {
        self.checkout
    }

    /// The delegate's name in the audit trail. Driver-minted, never derived from content.
    pub fn id(&self) -> DelegateId {
        self.id
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// What it was asked to do.
    ///
    /// The planner's own words, checked `(T,pub)` before the spec was built. Readable because a
    /// driver that could not hold it could not send it, and trusted because it is about to enter
    /// a planner's context.
    pub fn task(&self) -> &str {
        &self.task
    }

    /// What it holds: its kind's set, already narrowed by the parent's.
    ///
    /// Fixed here rather than looked up later, so nothing between this call and the run can
    /// widen it.
    pub fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    /// How many rounds it may make before it has to answer.
    pub fn rounds(&self) -> usize {
        self.rounds
    }

    /// The delegate as the audit trail describes it: what it is and what it holds, never the
    /// task, which can be long.
    pub fn describe(&self) -> String {
        let held: Vec<String> = self.capabilities.iter().map(|c| c.to_string()).collect();
        let held = if held.is_empty() {
            "nothing".to_string()
        } else {
            held.join(", ")
        };
        // The definition's name rather than the kind's, because "a reader" stops being the
        // useful word the moment three definitions are readers. Where nothing was defined the
        // two are the same string and the sentence is the one it always was.
        let what = if self.definition == self.kind.as_str() {
            format!("a {} delegate", self.kind)
        } else {
            format!("a {} delegate ({})", self.definition, self.kind)
        };
        format!(
            "{} is {what} holding {held} for at most {} rounds",
            self.id, self.rounds
        )
    }
}

/// How a delegate's run ended, as the run that collected it knows it.
///
/// Every part is something the driver holds: which way the result came back, the run's own
/// round count and clock, and a fixed name for a failure. Nothing a service or a tool said is
/// one of them, which is what lets the record it becomes hold no content (TRACE-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    /// It answered, having made `rounds` rounds of tool calls in `took`.
    Answered {
        took: std::time::Duration,
        rounds: usize,
    },
    /// Somebody stopped it before it answered.
    Stopped {
        took: std::time::Duration,
        rounds: usize,
    },
    /// It failed before it answered. `why` is the driver's fixed name for the failure.
    Failed {
        took: std::time::Duration,
        rounds: usize,
        why: &'static str,
    },
    /// Its thread ended without handing anything back, so neither its time nor its rounds are
    /// known.
    Lost,
}

/// The routing field a person's line names a definition under, where the line addressed one.
///
/// A routing field because routing is fixed before a turn observes anything, from what the
/// person submitted, and a keystroke is the only thing that may name a definition to address
/// (ADDRESS-3). No tool and no reply writes to the routing table.
pub const ADDRESSED: &str = "addressed";

/// What the kernel fixed about a turn a person addressed to a definition, before it ran.
///
/// Only [`crate::policy::Policy::address`] constructs one. It describes the person's own turn
/// working under a definition's prompt, narrowing and model, and not a second run: there is no
/// id, no task and no bound here, because the turn keeps its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Addressed {
    name: String,
    kind: Kind,
    model: Option<String>,
    prompt: String,
    skills: Option<Vec<String>>,
    memory: bool,
    checkout: bool,
    held: CapabilitySet,
    tools: Vec<String>,
}

impl Addressed {
    pub(crate) fn new(definition: &Definition, held: CapabilitySet, tools: Vec<String>) -> Self {
        Self {
            name: definition.name().to_string(),
            kind: definition.kind(),
            model: definition.model().map(str::to_string),
            prompt: definition.prompt().to_string(),
            skills: definition.skills().map(<[String]>::to_vec),
            memory: definition.keeps_memory(),
            checkout: definition.asks_for_checkout(),
            held,
            tools,
        }
    }

    /// What this turn holds once the definition has narrowed it.
    pub fn capabilities(&self) -> &CapabilitySet {
        &self.held
    }

    /// The name the kernel matched, which is what the reply is drawn under (ADDRESS-12).
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The model the definition named, where it named one.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// The definition's standing instruction, empty where its file had no body.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// The skills the definition named, where it named any (DELEGATE-23), and `None` where every
    /// skill the turn found is offered.
    pub fn skills(&self) -> Option<&[String]> {
        self.skills.as_deref()
    }

    /// Whether the definition keeps a memory, which the file of [`Self::name`]'s name is.
    pub fn keeps_memory(&self) -> bool {
        self.memory
    }

    /// Whether the definition asks for a checkout, which this turn does not apply: it is the
    /// person's own, in their working directory (CHECKOUT-2).
    pub fn asks_for_checkout(&self) -> bool {
        self.checkout
    }

    /// Every tool this turn is offered, already narrowed by what it holds and by what the
    /// definition named.
    pub fn tools(&self) -> &[String] {
        &self.tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::ServerAlias;

    /// A record writes a delegate's number down and reads it back, and what it reads is checked as
    /// a path: a position of zero, a sign, a path past the depth limit and stray text are none.
    #[test]
    fn a_delegate_number_reads_back_as_it_is_spelled_and_nothing_else_does() {
        let nested = DelegateId::nth(2)
            .child(1)
            .and_then(|id| id.child(7))
            .unwrap();
        for id in [DelegateId::nth(1), DelegateId::nth(12), nested] {
            assert_eq!(DelegateId::parse(&id.to_string()), Some(id));
        }
        for text in [
            "",
            "d",
            "d0",
            "d1.0",
            "d+1",
            "d-1",
            "d1.",
            "d.1",
            "D1",
            "1",
            "d1.2.3.4",
            "d1 ",
            "d99999999999",
        ] {
            assert_eq!(
                DelegateId::parse(text),
                None,
                "{text:?} was read as a delegate"
            );
        }
    }

    /// The planner selects from the driver's list. Anything else has to resolve to nothing, or
    /// `kind` would be a field the model could write a capability set into.
    #[test]
    fn a_kind_is_selected_from_the_enumerated_set_and_nothing_else() {
        for name in Kind::NAMES {
            assert!(Kind::from_name(name).is_some(), "{name} must be a kind");
        }
        for name in [
            "",
            "Reader",
            "worker ",
            "../worker",
            "planner",
            "file_write",
        ] {
            assert!(Kind::from_name(name).is_none(), "{name} must not be a kind");
        }
    }

    /// The names the planner is shown are the names that resolve, so a list in a tool schema
    /// cannot drift from the set behind it.
    #[test]
    fn every_advertised_name_resolves_to_the_kind_it_names() {
        for name in Kind::NAMES {
            let kind = Kind::from_name(name).expect("advertised");
            assert_eq!(kind.as_str(), name);
        }
    }

    /// A wider kind holds everything a narrower one does. A delegate chosen for being able to
    /// build must not lose the ability to read in exchange.
    #[test]
    fn the_kinds_are_ordered_by_what_they_hold() {
        let reader = Kind::Reader.capabilities();
        let checker = Kind::Checker.capabilities();
        let worker = Kind::Worker.capabilities();

        for capability in reader.iter() {
            assert!(checker.contains(&capability), "checker lost {capability}");
        }
        for capability in checker.iter() {
            assert!(worker.contains(&capability), "worker lost {capability}");
        }
        assert!(!reader.contains(&Capability::FileWrite));
        assert!(!reader.contains(&Capability::ShellExec));
        assert!(!checker.contains(&Capability::FileWrite));

        // `insert` takes the meet of two kinds with `min`, so the derived order has to be this
        // containment. A variant moved in the enum would leave it picking the wider one.
        for (narrower, wider) in [
            (Kind::Reader, Kind::Checker),
            (Kind::Checker, Kind::Worker),
            (Kind::Reader, Kind::Worker),
        ] {
            assert_eq!(
                narrower.min(wider),
                narrower,
                "the order the kinds compare in stopped being the order of what they hold"
            );
        }
    }

    /// LSP-9: starting a language server runs the project's build tooling (LSP-5), so the kinds
    /// that hold one are exactly the kinds that may already run a program.
    #[test]
    fn a_kind_holds_a_language_server_exactly_where_it_may_run_programs() {
        for name in Kind::NAMES {
            let held = Kind::from_name(name).expect("advertised").capabilities();
            assert_eq!(
                held.contains(&Capability::LanguageServer),
                held.contains(&Capability::ShellExec),
                "{name}"
            );
        }
        assert!(
            !Kind::Reader
                .capabilities()
                .contains(&Capability::LanguageServer)
        );
        assert!(
            Kind::Checker
                .capabilities()
                .contains(&Capability::LanguageServer)
        );
        assert!(
            Kind::Worker
                .capabilities()
                .contains(&Capability::LanguageServer)
        );
    }

    /// A planner is a model call, so every kind can reach the endpoint and no kind can reach
    /// anything else off this machine. Without the first a delegate cannot think; with more than
    /// the first, a grant would exist that nothing a person approved asked for.
    #[test]
    fn every_kind_can_reach_the_endpoint_and_nothing_else_remote() {
        for name in Kind::NAMES {
            let kind = Kind::from_name(name).expect("advertised");
            let held = kind.capabilities();
            assert!(
                held.contains(&Capability::WebFetch),
                "a {name} could not have made its own requests"
            );
            // No server, rather than no particular one: a grant names the server it is
            // about, so asking about a single alias would leave every other one unasked. A
            // kind's own set names none because which servers a session reached is not known
            // until it starts; which of its parent's a run holds is `held_out_of`'s to say.
            assert!(
                !held
                    .iter()
                    .any(|capability| matches!(capability, Capability::McpCall(_))),
                "{name}"
            );
            assert!(!held.contains(&Capability::GitWrite), "{name}");
        }
    }

    /// Every capability that is not a server's.
    fn built_in() -> impl Iterator<Item = Capability> {
        Capability::all()
            .into_iter()
            .filter(|capability| !matches!(capability, Capability::McpCall(_)))
    }

    /// A parent holding everything, two servers among it.
    fn holding_two_servers() -> CapabilitySet {
        built_in()
            .chain([
                Capability::McpCall(ServerAlias::new("weather")),
                Capability::McpCall(ServerAlias::new("notes")),
            ])
            .collect()
    }

    fn servers_in(held: &CapabilitySet) -> Vec<String> {
        held.iter()
            .filter(|capability| matches!(capability, Capability::McpCall(_)))
            .map(|capability| capability.to_string())
            .collect()
    }

    /// A worker carries every server its parent holds, and a reader and a checker carry none: a
    /// server's tool may write or run anything, which neither of the two may.
    #[test]
    fn only_a_worker_holds_the_servers_of_the_run_it_is_carved_from() {
        let parent = holding_two_servers();
        for name in Kind::NAMES {
            let kind = Kind::from_name(name).expect("advertised");
            let definition = Definition::from_file(name, "built in", kind, None, "", "test");
            let held = definition.held_out_of(&parent);
            let expected: Vec<String> = match kind {
                Kind::Worker => vec!["mcp_call:notes".into(), "mcp_call:weather".into()],
                Kind::Reader | Kind::Checker => Vec::new(),
            };
            assert_eq!(servers_in(&held), expected, "{name}");
            assert_eq!(kind.holds_servers(), kind == Kind::Worker, "{name}");
            // What the kind asks for is unchanged by the servers beside it.
            for capability in held.iter() {
                assert!(
                    matches!(capability, Capability::McpCall(_))
                        || definition.capabilities().contains(&capability),
                    "a {name} holds {capability}, which it never asked for"
                );
            }
        }
    }

    /// A server is carried from the parent and never granted: a worker spawned by a run holding
    /// one server holds that one, and one spawned by a run holding none holds none.
    #[test]
    fn a_worker_holds_no_server_its_parent_does_not() {
        let worker = Definition::from_file("worker", "built in", Kind::Worker, None, "", "test");
        let weather: CapabilitySet = built_in()
            .chain([Capability::McpCall(ServerAlias::new("weather"))])
            .collect();
        assert_eq!(
            servers_in(&worker.held_out_of(&weather)),
            ["mcp_call:weather"]
        );
        let none: CapabilitySet = built_in().collect();
        assert!(servers_in(&worker.held_out_of(&none)).is_empty());
    }

    /// A `tools:` line names this program's tools and no server's, so a worker that wrote one
    /// and no `mcpServers:` line asked for no server, however wide the tools line is.
    #[test]
    fn a_worker_naming_its_tools_and_no_server_holds_none() {
        let named = Definition::from_file(
            "editor",
            "edits",
            Kind::Worker,
            Some(vec!["read_file".into(), "write_file".into(), "run".into()]),
            "",
            "test",
        );
        assert!(servers_in(&named.held_out_of(&holding_two_servers())).is_empty());
        let empty = Definition::from_file("idle", "idles", Kind::Worker, Some(vec![]), "", "test");
        assert!(servers_in(&empty.held_out_of(&holding_two_servers())).is_empty());
    }

    /// A worker's `mcpServers:` line selects out of its parent's grants by alias. The server it
    /// named and the parent holds is held, the one it left off is not, and the one the parent
    /// holds no grant for is still not held: the line narrows and never grants.
    #[test]
    fn a_worker_naming_servers_holds_only_those_of_them_its_parent_holds() {
        let named =
            Definition::from_file("forecaster", "forecasts", Kind::Worker, None, "", "test")
                .with_servers(vec!["weather".into(), "calendar".into()]);
        let held = named.held_out_of(&holding_two_servers());
        assert_eq!(servers_in(&held), ["mcp_call:weather"]);
        assert_eq!(named.servers_not_held(&holding_two_servers()), ["calendar"]);
        assert!(named.servers_beyond_its_kind().is_empty());
        // Everything else the kind holds is untouched by which servers it named.
        let unnamed = Definition::from_file("worker", "built in", Kind::Worker, None, "", "test");
        let others = |set: &CapabilitySet| -> Vec<Capability> {
            set.iter()
                .filter(|capability| !matches!(capability, Capability::McpCall(_)))
                .collect()
        };
        assert_eq!(
            others(&held),
            others(&unnamed.held_out_of(&holding_two_servers()))
        );
    }

    /// An `mcpServers:` line on a kind that calls no server selects nothing, and the names it
    /// wrote are what the trail reports: the line chooses among what a worker would hold, and a
    /// reader or a checker holds none of it.
    #[test]
    fn a_reader_or_a_checker_naming_servers_holds_none() {
        for kind in [Kind::Reader, Kind::Checker] {
            let named = Definition::from_file("looker", "looks", kind, None, "", "test")
                .with_servers(vec!["weather".into()]);
            assert!(
                servers_in(&named.held_out_of(&holding_two_servers())).is_empty(),
                "{kind}"
            );
            assert_eq!(named.servers_beyond_its_kind(), ["weather"], "{kind}");
            assert!(
                named.servers_not_held(&CapabilitySet::none()).is_empty(),
                "{kind}"
            );
        }
    }

    /// Naming tools and naming servers are two lines, and neither empties the other: a worker
    /// confined to reading keeps the one server it named, and an empty `mcpServers:` line is a
    /// selection of no server even where no `tools:` line was written.
    #[test]
    fn a_servers_line_selects_servers_whatever_the_tools_line_says() {
        let both = Definition::from_file(
            "note-reader",
            "reads notes",
            Kind::Worker,
            Some(vec!["read_file".into()]),
            "",
            "test",
        )
        .with_servers(vec!["notes".into()]);
        let held = both.held_out_of(&holding_two_servers());
        assert_eq!(servers_in(&held), ["mcp_call:notes"]);
        assert!(!held.contains(&Capability::FileWrite));

        let none = Definition::from_file("offline", "offline", Kind::Worker, None, "", "test")
            .with_servers(Vec::new());
        assert!(servers_in(&none.held_out_of(&holding_two_servers())).is_empty());
        assert!(
            none.held_out_of(&holding_two_servers())
                .contains(&Capability::FileWrite)
        );
    }

    /// A replacement's servers are met with those of the one it replaces, as its tools are: a
    /// project cannot hand back a server a person's own definition left off, whether that one
    /// named its servers or only its tools, and says which it is loaded with.
    #[test]
    fn a_later_definition_cannot_hand_back_a_server_the_one_it_replaces_left_off() {
        let mut definitions = Definitions::default();
        definitions.insert(
            Definition::from_file("forecaster", "home", Kind::Worker, None, "", "~/a.md")
                .with_servers(vec!["weather".into(), "notes".into()]),
        );
        let admitted = definitions.insert(
            Definition::from_file("forecaster", "project", Kind::Worker, None, "", ".b.md")
                .with_servers(vec!["notes".into(), "calendar".into()]),
        );
        let found = definitions.get("forecaster").expect("selectable");
        assert_eq!(found.servers(), Some(["notes".to_string()].as_slice()));
        assert_eq!(
            servers_in(&found.held_out_of(&holding_two_servers())),
            ["mcp_call:notes"]
        );
        assert_eq!(
            admitted,
            Admitted::Narrowed(Narrowing {
                named: Kind::Worker,
                loaded: Kind::Worker,
                confined_to: None,
                servers_confined_to: Some(vec!["notes".to_string()]),
                given_a_checkout: false,
                writes_confined_to: Vec::new(),
                replaced: "~/a.md".to_string(),
            })
        );

        // A person's `tools:` line selected no server, so a project naming one gets none.
        let mut definitions = Definitions::default();
        definitions.insert(Definition::from_file(
            "forecaster",
            "home",
            Kind::Worker,
            Some(vec!["read_file".into()]),
            "",
            "~/a.md",
        ));
        let admitted = definitions.insert(
            Definition::from_file("forecaster", "project", Kind::Worker, None, "", ".b.md")
                .with_servers(vec!["weather".into()]),
        );
        let found = definitions.get("forecaster").expect("selectable");
        assert!(servers_in(&found.held_out_of(&holding_two_servers())).is_empty());
        assert_eq!(
            admitted,
            Admitted::Narrowed(Narrowing {
                named: Kind::Worker,
                loaded: Kind::Worker,
                confined_to: Some(vec!["read_file".to_string()]),
                servers_confined_to: Some(Vec::new()),
                given_a_checkout: false,
                writes_confined_to: Vec::new(),
                replaced: "~/a.md".to_string(),
            })
        );

        // And a replacement asking for no more than the one before it is admitted as written.
        let admitted = definitions.insert(Definition::from_file(
            "forecaster",
            "third",
            Kind::Worker,
            Some(vec!["read_file".into()]),
            "",
            ".c.md",
        ));
        assert_eq!(admitted, Admitted::AsWritten);
    }

    /// Every kind is bounded. An unbounded delegate has nothing watching it: the person is
    /// watching the turn, and the turn is blocked.
    #[test]
    fn every_kind_carries_a_bound() {
        for name in Kind::NAMES {
            let kind = Kind::from_name(name).expect("advertised");
            assert!(kind.rounds() > 0, "{name} must be bounded");
        }
    }

    /// A ceiling at the kind's own bound would let a definition shorten a delegate and never
    /// lengthen one, which leaves the long sub-task cut off exactly where it was.
    #[test]
    fn a_definition_may_ask_its_kind_for_more_rounds_than_its_own() {
        for name in Kind::NAMES {
            let kind = Kind::from_name(name).expect("advertised");
            assert!(
                kind.most_rounds() > kind.rounds(),
                "a {name} definition cannot ask for more than the {} a {name} gets anyway",
                kind.rounds()
            );
        }
        assert!(Kind::Reader.most_rounds() < Kind::Checker.most_rounds());
        assert!(Kind::Checker.most_rounds() < Kind::Worker.most_rounds());
    }

    /// The figures the specification and the page on definitions give, so a change to one is a
    /// change to what a person was told they may write.
    #[test]
    fn each_kinds_bound_and_ceiling_are_the_figures_a_person_is_told() {
        let bounds = Kind::NAMES.map(|name| {
            let kind = Kind::from_name(name).expect("advertised");
            (kind.rounds(), kind.most_rounds())
        });
        assert_eq!(bounds, [(60, 120), (80, 160), (120, 200)]);
    }

    /// A definition that asks for nothing is bounded as its kind is.
    #[test]
    fn a_definition_naming_no_rounds_keeps_its_kinds_own() {
        for name in Kind::NAMES {
            let kind = Kind::from_name(name).expect("advertised");
            let definition = Definition::from_file("helper", "helps", kind, None, "", "test");
            assert_eq!(definition.rounds(), kind.rounds(), "{name}");
            assert_eq!(definition.rounds_beyond_its_kind(), None, "{name}");
        }
    }

    /// Below the ceiling the file's number is the bound, in either direction from the kind's.
    #[test]
    fn a_definition_may_set_its_own_bound_beneath_its_kinds_ceiling() {
        for asked in [1, 10, Kind::Worker.rounds() + 1, Kind::Worker.most_rounds()] {
            let definition =
                Definition::from_file("fixer", "fixes", Kind::Worker, None, "", "test")
                    .with_rounds(asked);
            assert_eq!(definition.rounds(), asked);
            assert_eq!(definition.rounds_beyond_its_kind(), None, "{asked}");
        }
    }

    /// The ceiling is what keeps a delegate nobody is watching from running as long as a file
    /// says, so a number above it is held to it, and what was asked is kept to be said.
    #[test]
    fn a_definition_asking_past_its_kinds_ceiling_is_held_to_it() {
        for name in Kind::NAMES {
            let kind = Kind::from_name(name).expect("advertised");
            for asked in [kind.most_rounds() + 1, usize::MAX] {
                let definition = Definition::from_file("long", "runs long", kind, None, "", "test")
                    .with_rounds(asked);
                assert_eq!(
                    definition.rounds(),
                    kind.most_rounds(),
                    "{name} asking {asked}"
                );
                assert_eq!(definition.rounds_beyond_its_kind(), Some(asked), "{name}");
            }
        }
    }

    /// A bound is not authority, so a replacement's own number is the one in force, as its body
    /// is. It is held to the ceiling of the kind it is loaded as, so a project that could not
    /// widen a `reader` to a `worker` cannot have a worker's ceiling either.
    #[test]
    fn a_later_definition_takes_over_the_rounds_under_the_kind_it_is_loaded_as() {
        let mut definitions = Definitions::default();
        definitions.insert(
            Definition::from_file("reviewer", "global", Kind::Reader, None, "", "home")
                .with_rounds(30),
        );
        definitions.insert(
            Definition::from_file("reviewer", "project", Kind::Reader, None, "", "project")
                .with_rounds(90),
        );
        assert_eq!(
            definitions.get("reviewer").expect("selectable").rounds(),
            90
        );

        definitions.insert(
            Definition::from_file("reviewer", "wider", Kind::Worker, None, "", "wider")
                .with_rounds(Kind::Worker.most_rounds()),
        );
        let found = definitions.get("reviewer").expect("selectable");
        assert_eq!(found.kind(), Kind::Reader);
        assert_eq!(
            found.rounds(),
            Kind::Reader.most_rounds(),
            "a replacement kept the ceiling of a kind it was not loaded as"
        );
        assert_eq!(
            found.rounds_beyond_its_kind(),
            Some(Kind::Worker.most_rounds())
        );

        definitions.insert(Definition::from_file(
            "reviewer",
            "again",
            Kind::Reader,
            None,
            "",
            "again",
        ));
        assert_eq!(
            definitions.get("reviewer").expect("selectable").rounds(),
            Kind::Reader.rounds()
        );
    }

    /// A definition narrows its kind and there is no spelling of `tools:` that adds anything.
    /// A file that could name a capability set would make a checked-in file the author of
    /// authority, which is the one thing delegation may never do.
    #[test]
    fn a_definition_can_only_narrow_what_its_kind_holds() {
        for name in Kind::NAMES {
            let kind = Kind::from_name(name).expect("enumerated");
            let asking_for_everything = Definition::from_file(
                "greedy",
                "asks for what it cannot have",
                kind,
                Some(
                    ["read_file", "run", "write_file", "lsp", "spawn_agent"]
                        .map(str::to_string)
                        .to_vec(),
                ),
                "",
                "test",
            );

            for capability in asking_for_everything.capabilities().iter() {
                assert!(
                    kind.capabilities().contains(&capability),
                    "a {name} definition gained {capability}"
                );
            }
        }
    }

    /// A definition that names tools holds only what those tools reach, so a `worker` confined to
    /// reading is a delegate the write gate refuses rather than one the tool list merely leaves
    /// a write out of.
    #[test]
    fn naming_tools_drops_the_capabilities_no_named_tool_reaches() {
        let reading = Definition::from_file(
            "rule-reviewer",
            "reads a diff",
            Kind::Worker,
            Some(["read_file", "list_files"].map(str::to_string).to_vec()),
            "",
            "test",
        );

        let held = reading.capabilities();
        assert!(held.contains(&Capability::FileRead));
        assert!(
            !held.contains(&Capability::FileWrite),
            "a definition that names no write tool still held file_write"
        );
        assert!(
            !held.contains(&Capability::ShellExec),
            "a definition that names no program still held shell_exec"
        );
    }

    /// A language server goes with running programs: starting one runs the project's build
    /// tooling, so a definition naming `lsp` without `run` holds and is offered neither.
    #[test]
    fn naming_lsp_without_run_holds_no_language_server() {
        for kind in [Kind::Checker, Kind::Worker] {
            let lsp_only = Definition::from_file(
                "lsp-only",
                "asks a server",
                kind,
                Some(["read_file", "lsp"].map(str::to_string).to_vec()),
                "",
                "test",
            );
            let held = lsp_only.capabilities();
            assert!(
                !held.contains(&Capability::LanguageServer),
                "a definition of kind {} naming lsp without run held a language server",
                kind.as_str()
            );
            assert!(!held.contains(&Capability::ShellExec));
            assert!(
                !lsp_only
                    .held_out_of(&kind.capabilities())
                    .contains(&Capability::LanguageServer),
                "the parent's set re-granted the language server"
            );

            let both = Definition::from_file(
                "lsp-and-run",
                "asks a server",
                kind,
                Some(["read_file", "lsp", "run"].map(str::to_string).to_vec()),
                "",
                "test",
            );
            assert!(both.capabilities().contains(&Capability::LanguageServer));
        }
    }

    /// A planner is a model call, so a definition narrowed to one read tool still has to be able
    /// to make one. No tool reaches the network, so keeping it spends nothing.
    #[test]
    fn a_definition_narrowed_to_one_tool_can_still_reach_the_endpoint() {
        let narrow = Definition::from_file(
            "one-tool",
            "reads one file",
            Kind::Reader,
            Some(vec!["read_file".to_string()]),
            "",
            "test",
        );

        assert!(
            narrow.capabilities().contains(&Capability::WebFetch),
            "a narrowed definition could not have made its own requests"
        );
    }

    /// A definition naming no tools is its kind, exactly as every delegate was before there were
    /// definitions.
    #[test]
    fn a_definition_that_names_no_tools_holds_its_kinds_own_set() {
        for name in Kind::NAMES {
            let kind = Kind::from_name(name).expect("enumerated");
            assert_eq!(
                Definition::of_kind(kind).capabilities(),
                kind.capabilities()
            );
            assert!(Definition::of_kind(kind).tools().is_none());
        }
    }

    /// What the trail says was dropped. A tool no delegate ever gets counts as beyond the kind
    /// too: `fetch_url` is left out by name rather than for want of a capability, so a definition
    /// asking for it must not be told it has one.
    #[test]
    fn the_tools_a_kind_cannot_reach_are_the_ones_reported_dropped() {
        let asking = Definition::from_file(
            "greedy",
            "asks for what it cannot have",
            Kind::Reader,
            Some(
                ["read_file", "write_file", "run", "fetch_url"]
                    .map(str::to_string)
                    .to_vec(),
            ),
            "",
            "test",
        );

        let mut beyond = asking.tools_beyond_its_kind();
        beyond.sort_unstable();
        assert_eq!(beyond, ["fetch_url", "run", "write_file"]);

        // And a name reported as dropped selects nothing either, so a definition is never left
        // holding a capability nothing it is offered can spend.
        let only_fetch = Definition::from_file(
            "fetcher",
            "asks for the one no delegate gets",
            Kind::Reader,
            Some(vec!["fetch_url".to_string()]),
            "",
            "test",
        );
        assert_eq!(
            only_fetch.capabilities(),
            CapabilitySet::from_iter(HELD_WHATEVER_IT_NAMED),
            "a definition naming only a tool no delegate gets held more than every one holds"
        );
    }

    /// The three kinds are in every set, whatever anybody wrote down, so a session that found no
    /// files selects exactly what it always did.
    #[test]
    fn the_three_kinds_are_in_every_set() {
        let definitions = Definitions::default();
        assert_eq!(definitions.names(), Kind::NAMES.to_vec());
        for name in Kind::NAMES {
            let found = definitions.get(name).expect("a kind is always selectable");
            assert_eq!(found.kind().as_str(), name);
            assert!(found.tools().is_none());
            assert!(found.prompt().is_empty());
        }
    }

    /// Most specific wins, as it does in the trust map and in the skill catalogue. A project that
    /// ships its own version of a definition means it.
    ///
    /// The kinds go the narrowing way round, which is the direction a replacement is free to
    /// take: the widening one is
    /// [`a_later_definition_cannot_widen_the_kind_the_one_it_replaces_named`].
    #[test]
    fn a_later_definition_replaces_one_of_the_same_name() {
        let mut definitions = Definitions::default();
        definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the global one",
            Kind::Worker,
            None,
            "global",
            "~/.bravebot/agents/rule-reviewer.md",
        ));
        definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the project one",
            Kind::Checker,
            None,
            "local",
            ".bravebot/agents/rule-reviewer.md",
        ));

        assert_eq!(definitions.len(), Kind::NAMES.len() + 1);
        let found = definitions.get("rule-reviewer").expect("selectable");
        assert_eq!(found.prompt(), "local");
        assert_eq!(found.description(), "the project one");
        assert_eq!(found.origin(), ".bravebot/agents/rule-reviewer.md");
        assert_eq!(found.kind(), Kind::Checker);
        assert!(!found.capabilities().contains(&Capability::FileWrite));
    }

    /// A replacement has the last word about what a name is *for* and none at all about what it
    /// may do. The source that gets the last word is the project, and the vouch that let its
    /// file be read is a decision about the checkout rather than about this name, so a `reader`
    /// somebody wrote in their own directory stays a reader.
    #[test]
    fn a_later_definition_cannot_widen_the_kind_the_one_it_replaces_named() {
        let mut definitions = Definitions::default();
        definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the global one",
            Kind::Reader,
            None,
            "global",
            "~/.bravebot/agents/rule-reviewer.md",
        ));
        let admitted = definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the project one",
            Kind::Worker,
            None,
            "local",
            ".bravebot/agents/rule-reviewer.md",
        ));

        let found = definitions.get("rule-reviewer").expect("selectable");
        assert_eq!(found.prompt(), "local", "the replacement did not take");
        assert_eq!(
            found.kind(),
            Kind::Reader,
            "the project widened a name the person's own file defined as a reader"
        );
        let held = found.capabilities();
        assert!(held.contains(&Capability::FileRead));
        assert!(!held.contains(&Capability::FileWrite));
        assert!(!held.contains(&Capability::ShellExec));

        // Said rather than dropped quietly: the file that asked is the one whose author has to
        // be told, and the file that cut it down is the answer to why.
        assert_eq!(
            admitted,
            Admitted::Narrowed(Narrowing {
                named: Kind::Worker,
                loaded: Kind::Reader,
                confined_to: None,
                servers_confined_to: None,
                given_a_checkout: false,
                writes_confined_to: Vec::new(),
                replaced: "~/.bravebot/agents/rule-reviewer.md".to_string(),
            })
        );
    }

    /// The other spelling of the same widening, and the one no kind moves in. A `tools:` line is
    /// a narrowing of its own kind, so a replacement of the same kind that names no tools would
    /// hand back everything the earlier file had taken away.
    #[test]
    fn a_later_definition_cannot_undo_the_tools_the_one_it_replaces_named() {
        let mut definitions = Definitions::default();
        definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the global one",
            Kind::Worker,
            Some(vec!["read_file".to_string()]),
            "global",
            "~/.bravebot/agents/rule-reviewer.md",
        ));
        let admitted = definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the project one",
            Kind::Worker,
            None,
            "local",
            ".bravebot/agents/rule-reviewer.md",
        ));

        let found = definitions.get("rule-reviewer").expect("selectable");
        assert_eq!(found.prompt(), "local", "the replacement did not take");
        assert_eq!(
            found.kind(),
            Kind::Worker,
            "neither file named a wider kind"
        );
        let held = found.capabilities();
        assert!(held.contains(&Capability::FileRead));
        assert!(!held.contains(&Capability::FileWrite));
        assert!(!held.contains(&Capability::ShellExec));
        // Its own file selected every server its parent holds, and the `tools:` line it
        // inherited selects none, so that is said too rather than left to the tools phrase.
        assert!(servers_in(&found.held_out_of(&holding_two_servers())).is_empty());
        assert_eq!(
            admitted,
            Admitted::Narrowed(Narrowing {
                named: Kind::Worker,
                loaded: Kind::Worker,
                confined_to: Some(vec!["read_file".to_string()]),
                servers_confined_to: Some(Vec::new()),
                given_a_checkout: false,
                writes_confined_to: Vec::new(),
                replaced: "~/.bravebot/agents/rule-reviewer.md".to_string(),
            })
        );
    }

    /// The tool list is met name by name, not only capability by capability. A replacement
    /// naming every tool one capability reaches costs nothing at the capability level and still
    /// hands a delegate its author confined to one tool the other four.
    #[test]
    fn a_later_definition_cannot_widen_a_tool_list_within_one_capability() {
        let mut definitions = Definitions::default();
        definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the global one",
            Kind::Reader,
            Some(vec!["read_file".to_string()]),
            "global",
            "~/.bravebot/agents/rule-reviewer.md",
        ));
        let admitted = definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the project one",
            Kind::Reader,
            Some(vec![
                "read_file".to_string(),
                "search".to_string(),
                "spawn_processor".to_string(),
            ]),
            "local",
            ".bravebot/agents/rule-reviewer.md",
        ));

        let found = definitions.get("rule-reviewer").expect("selectable");
        assert_eq!(found.tools(), Some(["read_file".to_string()].as_slice()));
        assert_eq!(
            found.capabilities(),
            CapabilitySet::from_iter(HELD_WHATEVER_IT_NAMED),
            "the tool names moved and the capability set did not, so nothing else would notice"
        );
        assert_eq!(
            admitted,
            Admitted::Narrowed(Narrowing {
                named: Kind::Reader,
                loaded: Kind::Reader,
                confined_to: Some(vec!["read_file".to_string()]),
                servers_confined_to: None,
                given_a_checkout: false,
                writes_confined_to: Vec::new(),
                replaced: "~/.bravebot/agents/rule-reviewer.md".to_string(),
            })
        );
    }

    /// Two lists with no name in common leave a delegate with no tools, which is what a list
    /// naming another agent's vocabulary already produces. The alternative is falling back to
    /// one side's list, and either side is a widening of the other.
    #[test]
    fn tool_lists_with_nothing_in_common_meet_at_nothing() {
        let mut definitions = Definitions::default();
        definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the global one",
            Kind::Worker,
            Some(vec!["read_file".to_string()]),
            "global",
            "~/.bravebot/agents/rule-reviewer.md",
        ));
        let admitted = definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the project one",
            Kind::Worker,
            Some(vec!["run".to_string()]),
            "local",
            ".bravebot/agents/rule-reviewer.md",
        ));

        let found = definitions.get("rule-reviewer").expect("selectable");
        assert_eq!(found.tools(), Some([].as_slice()));
        assert_eq!(
            found.capabilities(),
            CapabilitySet::from_iter(HELD_WHATEVER_IT_NAMED)
        );
        assert_eq!(
            admitted,
            Admitted::Narrowed(Narrowing {
                named: Kind::Worker,
                loaded: Kind::Worker,
                confined_to: Some(Vec::new()),
                servers_confined_to: None,
                given_a_checkout: false,
                writes_confined_to: Vec::new(),
                replaced: "~/.bravebot/agents/rule-reviewer.md".to_string(),
            })
        );
    }

    /// A replacement is cut down against what the definition it replaced was left holding, not
    /// against what that one's own file named, so the narrowing carries through a third of the
    /// same name. Two files in one directory resolve by file name, so three of them is reachable
    /// with one project and a home directory.
    #[test]
    fn a_narrowing_carries_through_a_third_definition_of_the_same_name() {
        let mut definitions = Definitions::default();
        definitions.insert(Definition::from_file(
            "rule-reviewer",
            "the person's own",
            Kind::Worker,
            Some(vec!["read_file".to_string()]),
            "",
            "~/.bravebot/agents/a-first.md",
        ));
        for origin in ["~/.bravebot/agents/z-last.md", ".bravebot/agents/rule.md"] {
            definitions.insert(Definition::from_file(
                "rule-reviewer",
                "a later one",
                Kind::Worker,
                None,
                "",
                origin,
            ));
        }

        let found = definitions.get("rule-reviewer").expect("selectable");
        assert_eq!(found.origin(), ".bravebot/agents/rule.md");
        assert_eq!(found.tools(), Some(["read_file".to_string()].as_slice()));
        let held = found.capabilities();
        assert!(held.contains(&Capability::FileRead));
        assert!(!held.contains(&Capability::FileWrite));
        assert!(!held.contains(&Capability::ShellExec));
    }

    /// A name nothing in the set carries selects nothing, which is what keeps `kind` from being a
    /// field a planner can write a capability set into.
    #[test]
    fn a_name_no_definition_carries_selects_nothing() {
        let definitions = Definitions::default();
        for name in ["", "Reader", "worker ", "../worker", "rule-reviewer"] {
            assert!(
                definitions.get(name).is_none(),
                "{name} must select nothing"
            );
        }
    }

    /// A name that is not a tool selects nothing, rather than falling to the weakest capability
    /// any tool asks for. A definition written for another agent names that agent's vocabulary,
    /// and a fallback would leave a delegate holding something for a list of names none of which
    /// is a tool it gets.
    #[test]
    fn a_name_that_is_not_a_tool_selects_no_capability() {
        for name in ["Bash(git *)", "Read", "Grep", "*", "", "fetch_url"] {
            assert_eq!(gating_capability(name), None, "'{name}' selected something");
        }
        assert_eq!(gating_capability("read_file"), Some(Capability::FileRead));
        assert_eq!(gating_capability("write_file"), Some(Capability::FileWrite));
        assert_eq!(gating_capability("run"), Some(Capability::ShellExec));
        assert_eq!(gating_capability("lsp"), Some(Capability::LanguageServer));
    }

    /// A definition whose whole `tools:` line is another agent's vocabulary starts a delegate
    /// with no tools, and every one of those names is reported, so the trail says why rather
    /// than leaving somebody with a delegate that answers having done nothing.
    #[test]
    fn a_tools_line_written_for_another_agent_is_reported_name_by_name() {
        let foreign = Definition::from_file(
            "ported",
            "written for something else",
            Kind::Worker,
            Some(
                ["Read", "Grep", "Bash(git log)"]
                    .map(str::to_string)
                    .to_vec(),
            ),
            "",
            "test",
        );

        assert_eq!(
            foreign.tools_beyond_its_kind(),
            ["Read", "Grep", "Bash(git log)"],
            "a name that is not a tool was not reported as dropped"
        );
        assert_eq!(
            foreign.capabilities(),
            CapabilitySet::from_iter(HELD_WHATEVER_IT_NAMED),
            "a list of names that are not tools still selected something"
        );
    }

    /// A write is a read of the file followed by a write of it, so a definition naming only a
    /// write tool has to keep reading or its one tool is refused on every call. Not a widening:
    /// every kind holds reading already.
    #[test]
    fn a_definition_that_names_only_a_write_tool_can_still_read() {
        let writing = Definition::from_file(
            "fixer",
            "writes one file",
            Kind::Worker,
            Some(vec!["edit_file".to_string()]),
            "",
            "test",
        );

        let held = writing.capabilities();
        assert!(held.contains(&Capability::FileWrite));
        assert!(
            held.contains(&Capability::FileRead),
            "a definition naming a write tool could not read the file it edits"
        );
        assert!(!held.contains(&Capability::ShellExec));
    }

    /// The three kinds keep their names whatever anybody writes down. A file free to claim one
    /// would be a file renaming the narrowest kind to the widest, and a planner picking the
    /// narrowest thing that can do the job would be picking from a list whose order had stopped
    /// being true.
    #[test]
    fn a_definition_cannot_take_a_kinds_own_name() {
        let mut definitions = Definitions::default();
        for name in Kind::NAMES {
            assert!(!Definitions::may_be_named(name));
            assert_eq!(
                definitions.insert(Definition::from_file(
                    name,
                    "pretending to be a kind",
                    Kind::Worker,
                    None,
                    "",
                    ".bravebot/agents/escalate.md",
                )),
                Admitted::Refused,
                "'{name}' was taken over by a definition"
            );
        }

        assert_eq!(definitions.len(), Kind::NAMES.len());
        assert_eq!(
            definitions
                .get("reader")
                .expect("a kind is always there")
                .kind(),
            Kind::Reader,
            "reader stopped being a reader"
        );
        assert!(Definitions::may_be_named("rule-reviewer"));
    }

    #[test]
    fn a_description_names_what_it_holds_but_never_the_task() {
        let spec = DelegateSpec::new(
            DelegateId::nth(1),
            &Definition::of_kind(Kind::Checker),
            "find out whether the tests pass",
            Kind::Checker.capabilities(),
            80,
            Tree::default(),
        );

        let described = spec.describe();
        assert!(
            described.starts_with("d1 "),
            "the description does not say which delegate it is about: {described}"
        );
        assert!(described.contains("checker"));
        assert!(described.contains("file_read"));
        assert!(described.contains("shell_exec"));
        assert!(described.contains("80"));
        assert!(!described.contains("whether the tests pass"));
    }

    /// "A reader" stops being the useful word the moment two definitions are readers, so the
    /// trail names the definition and says what kind it is beside it.
    #[test]
    fn a_description_names_the_definition_and_the_kind_behind_it() {
        let spec = DelegateSpec::new(
            DelegateId::nth(2),
            &Definition::from_file(
                "rule-reviewer",
                "checks a diff",
                Kind::Reader,
                None,
                "",
                ".bravebot/agents/rule-reviewer.md",
            ),
            "check the diff",
            Kind::Reader.capabilities(),
            60,
            Tree::default(),
        );

        let described = spec.describe();
        assert!(
            described.contains("rule-reviewer"),
            "the description does not say which definition it is: {described}"
        );
        assert!(
            described.contains("reader"),
            "the description does not say what kind it is: {described}"
        );
        assert!(!described.contains("check the diff"));
    }

    /// The spec carries the tools its definition named, already without the ones its kind cannot
    /// reach, so nothing downstream has to know what a kind reaches to keep the narrowing.
    #[test]
    fn a_spec_carries_only_the_named_tools_its_kind_reaches() {
        let spec = DelegateSpec::new(
            DelegateId::nth(1),
            &Definition::from_file(
                "rule-reviewer",
                "checks a diff",
                Kind::Reader,
                Some(
                    ["read_file", "write_file", "fetch_url"]
                        .map(str::to_string)
                        .to_vec(),
                ),
                "",
                ".bravebot/agents/rule-reviewer.md",
            ),
            "check the diff",
            Kind::Reader.capabilities(),
            60,
            Tree::default(),
        );

        assert_eq!(spec.tools(), Some(["read_file".to_string()].as_slice()));
    }

    /// A definition may name a model to run on, and the spec carries it.
    #[test]
    fn a_definition_may_name_a_model_and_the_spec_carries_it() {
        let definition = Definition::from_file(
            "cheap-reader",
            "reads with a small model",
            Kind::Reader,
            None,
            "",
            "test",
        )
        .with_model("haiku");

        assert_eq!(definition.model(), Some("haiku"));

        let spec = DelegateSpec::new(
            DelegateId::nth(1),
            &definition,
            "read something",
            Kind::Reader.capabilities(),
            60,
            Tree::default(),
        );
        assert_eq!(spec.model(), Some("haiku"));
    }

    /// A definition may name an effort level to run at, and the spec carries it. `None` is the
    /// level the spawning turn runs at, so the two stay apart on the spec as well.
    #[test]
    fn a_definition_may_name_an_effort_and_the_spec_carries_it() {
        let spec_for = |definition: Definition| {
            DelegateSpec::new(
                DelegateId::nth(1),
                &definition,
                "read something",
                Kind::Reader.capabilities(),
                60,
                Tree::default(),
            )
        };
        let plain =
            || Definition::from_file("eager", "thinks hard", Kind::Reader, None, "", "test");

        let definition = plain().with_effort("low");
        assert_eq!(definition.effort(), Some("low"));
        assert_eq!(spec_for(definition).effort(), Some("low"));
        assert_eq!(plain().effort(), None);
        assert_eq!(spec_for(plain()).effort(), None);
    }

    /// A definition may name the skills its delegate is offered, and the spec carries the list
    /// as written: which of them exist is the turn's to answer, since the turn found them.
    #[test]
    fn a_definition_may_name_skills_and_the_spec_carries_them() {
        let named = |skills: Option<Vec<String>>| {
            let definition =
                Definition::from_file("reviewer", "reviews", Kind::Reader, None, "", "test");
            let definition = match skills {
                Some(skills) => definition.with_skills(skills),
                None => definition,
            };
            DelegateSpec::new(
                DelegateId::nth(1),
                &definition,
                "review something",
                Kind::Reader.capabilities(),
                60,
                Tree::default(),
            )
        };

        let listed = vec!["review-style".to_string(), "no-such-skill".to_string()];
        assert_eq!(
            named(Some(listed.clone())).skills(),
            Some(listed.as_slice())
        );
        assert_eq!(named(Some(Vec::new())).skills(), Some([].as_slice()));
        assert_eq!(named(None).skills(), None);
    }

    /// A skill is guidance, as the body is, so a replacement's own `skills:` line is the one in
    /// force, and one that names none offers every skill the turn found. Nothing it names can be
    /// something the turn did not find, so taking it over hands back no authority.
    #[test]
    fn a_later_definition_takes_over_the_skills_the_one_it_replaces_named() {
        let mut definitions = Definitions::default();
        definitions.insert(
            Definition::from_file("reviewer", "global", Kind::Reader, None, "", "home")
                .with_skills(vec!["review-style".to_string()]),
        );
        let admitted = definitions.insert(
            Definition::from_file("reviewer", "project", Kind::Reader, None, "", "project")
                .with_skills(vec!["commit-style".to_string()]),
        );
        assert_eq!(admitted, Admitted::AsWritten);
        assert_eq!(
            definitions.get("reviewer").expect("selectable").skills(),
            Some(["commit-style".to_string()].as_slice())
        );

        definitions.insert(Definition::from_file(
            "reviewer",
            "again",
            Kind::Reader,
            None,
            "",
            "again",
        ));
        assert_eq!(
            definitions.get("reviewer").expect("selectable").skills(),
            None
        );
    }

    /// MEMORY-2: whether a memory is kept is the latest definition's, in both directions, because
    /// keeping one changes what a run knows and adds nothing it may do. A project keeping none
    /// stops a person's own habit from writing into its checkout, and one keeping a memory its
    /// replaced definition did not starts one.
    #[test]
    fn a_later_definition_takes_over_whether_a_memory_is_kept() {
        let mut definitions = Definitions::default();
        definitions.insert(
            Definition::from_file("reviewer", "global", Kind::Reader, None, "", "home")
                .with_memory(),
        );
        let admitted = definitions.insert(Definition::from_file(
            "reviewer",
            "project",
            Kind::Reader,
            None,
            "",
            "project",
        ));
        assert_eq!(admitted, Admitted::AsWritten);
        assert!(
            !definitions
                .get("reviewer")
                .expect("selectable")
                .keeps_memory()
        );

        let admitted = definitions.insert(
            Definition::from_file("reviewer", "again", Kind::Reader, None, "", "again")
                .with_memory(),
        );
        assert_eq!(admitted, Admitted::AsWritten, "keeping one widens nothing");
        assert!(
            definitions
                .get("reviewer")
                .expect("selectable")
                .keeps_memory()
        );
    }

    /// DELEGATE-28: a write limit is met as the tools are. A replacement writing none keeps the
    /// one the definition it replaced wrote and is reported as narrowed, one writing its own is
    /// held to both, and one repeating the limit is admitted as written because it changed
    /// nothing.
    #[test]
    fn a_later_definition_cannot_take_a_write_limit_away() {
        let limit = |patterns: &[&str]| patterns.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        let mut definitions = Definitions::default();
        definitions.insert(
            Definition::from_file("scribe", "home", Kind::Worker, None, "", "~/a.md")
                .with_writes(limit(&["docs/**"])),
        );

        let dropped = definitions.insert(Definition::from_file(
            "scribe",
            "project",
            Kind::Worker,
            None,
            "",
            ".b.md",
        ));
        assert_eq!(
            definitions
                .get("scribe")
                .expect("selectable")
                .write_limits(),
            &[limit(&["docs/**"])],
            "a later definition writing no limit lifted the earlier one"
        );
        assert_eq!(
            dropped,
            Admitted::Narrowed(Narrowing {
                named: Kind::Worker,
                loaded: Kind::Worker,
                confined_to: None,
                servers_confined_to: None,
                given_a_checkout: false,
                writes_confined_to: vec![limit(&["docs/**"])],
                replaced: "~/a.md".to_string(),
            })
        );

        let wider = definitions.insert(
            Definition::from_file("scribe", "project", Kind::Worker, None, "", ".c.md")
                .with_writes(limit(&["**"])),
        );
        assert_eq!(
            definitions
                .get("scribe")
                .expect("selectable")
                .write_limits(),
            &[limit(&["docs/**"]), limit(&["**"])],
            "a later definition's wider limit replaced the earlier one instead of joining it"
        );
        assert!(matches!(wider, Admitted::Narrowed(_)), "{wider:?}");

        let repeated = definitions.insert(
            Definition::from_file("scribe", "project", Kind::Worker, None, "", ".d.md")
                .with_writes(limit(&["docs/**"]))
                .with_writes(limit(&["**"])),
        );
        assert_eq!(repeated, Admitted::AsWritten);
        assert_eq!(
            definitions
                .get("scribe")
                .expect("selectable")
                .write_limits()
                .len(),
            2,
            "a repeated limit was kept twice"
        );
    }

    /// CHECKOUT-2: a checkout is met as the kind is. A replacement asking for none keeps the one
    /// the definition it replaced asked for, and is reported as narrowed, and one asking for a
    /// checkout its replaced definition did not is admitted as written, because a checkout gives
    /// a delegate nothing more it may do.
    #[test]
    fn a_later_definition_can_give_a_checkout_and_cannot_take_one_away() {
        let mut definitions = Definitions::default();
        definitions.insert(
            Definition::from_file("migrator", "home", Kind::Worker, None, "", "~/a.md")
                .with_checkout(),
        );
        let admitted = definitions.insert(Definition::from_file(
            "migrator",
            "project",
            Kind::Worker,
            None,
            "",
            ".b.md",
        ));
        assert!(
            definitions
                .get("migrator")
                .expect("selectable")
                .asks_for_checkout(),
            "a later definition took a checkout away"
        );
        assert_eq!(
            admitted,
            Admitted::Narrowed(Narrowing {
                named: Kind::Worker,
                loaded: Kind::Worker,
                confined_to: None,
                servers_confined_to: None,
                given_a_checkout: true,
                writes_confined_to: Vec::new(),
                replaced: "~/a.md".to_string(),
            })
        );

        let mut definitions = Definitions::default();
        definitions.insert(Definition::from_file(
            "migrator",
            "home",
            Kind::Worker,
            None,
            "",
            "~/a.md",
        ));
        let admitted = definitions.insert(
            Definition::from_file("migrator", "project", Kind::Worker, None, "", ".b.md")
                .with_checkout(),
        );
        assert_eq!(
            admitted,
            Admitted::AsWritten,
            "asking for one widens nothing"
        );
        let found = definitions.get("migrator").expect("selectable");
        assert!(found.asks_for_checkout());
        assert!(
            DelegateSpec::new(
                DelegateId::nth(1),
                found,
                "migrate",
                Kind::Worker.capabilities(),
                60,
                Tree::default(),
            )
            .asks_for_checkout()
        );
    }

    /// CHECKOUT-2: a reader is never given a checkout, whether its own file asked for one or it
    /// was met down to a reader from a worker that did, and a replacement is not told it was
    /// given one it will not have.
    #[test]
    fn a_reader_is_given_no_checkout_whatever_a_definition_asks() {
        let mut definitions = Definitions::default();
        definitions.insert(
            Definition::from_file("reviewer", "d", Kind::Reader, None, "", "home").with_checkout(),
        );
        let found = definitions.get("reviewer").expect("selectable");
        assert!(!found.asks_for_checkout());
        assert!(found.checkout_beyond_its_kind());
        assert!(
            !DelegateSpec::new(
                DelegateId::nth(1),
                found,
                "review",
                Kind::Reader.capabilities(),
                60,
                Tree::default(),
            )
            .asks_for_checkout()
        );

        let mut definitions = Definitions::default();
        definitions.insert(
            Definition::from_file("migrator", "home", Kind::Worker, None, "", "~/a.md")
                .with_checkout(),
        );
        let admitted = definitions.insert(Definition::from_file(
            "migrator",
            "project",
            Kind::Reader,
            None,
            "",
            ".b.md",
        ));
        let found = definitions.get("migrator").expect("selectable");
        assert_eq!(found.kind(), Kind::Reader);
        assert!(!found.asks_for_checkout());
        assert!(found.checkout_beyond_its_kind());
        assert_eq!(
            admitted,
            Admitted::AsWritten,
            "a reader was told it is given a checkout"
        );
    }

    /// MEMORY-2: a working directory whose memory would sit in the person's own directory keeps
    /// none for any definition, and every other field of each stays as it was.
    #[test]
    fn keeping_no_memory_clears_it_from_every_definition_and_nothing_else() {
        let mut definitions = Definitions::default();
        definitions.insert(
            Definition::from_file("reviewer", "d", Kind::Reader, None, "", "home").with_memory(),
        );
        definitions.insert(
            Definition::from_file("migrator", "d", Kind::Worker, None, "body", "home")
                .with_memory(),
        );

        definitions.keep_no_memory();

        assert!(definitions.iter().all(|d| !d.keeps_memory()));
        let migrator = definitions.get("migrator").expect("still selectable");
        assert_eq!(migrator.kind(), Kind::Worker);
        assert_eq!(migrator.prompt(), "body");
    }

    /// A number says where its delegate sits, so a trail and a screen can name a grandchild
    /// without reading anything it wrote, and two delegates at different depths with the same
    /// position are never one run.
    #[test]
    fn a_delegates_number_is_its_path_from_the_turn() {
        let second = DelegateId::nth(2);
        let beneath = second.child(1).expect("one below the turn may delegate");
        let deepest = beneath.child(3).expect("two below the turn may delegate");

        assert_eq!(
            [second, beneath, deepest].map(|id| id.to_string()),
            ["d2", "d2.1", "d2.1.3"]
        );
        assert_eq!([second, beneath, deepest].map(DelegateId::depth), [1, 2, 3]);
        assert_eq!(deepest.position(), 3);
        assert_eq!(
            deepest.child(1),
            None,
            "a delegate {MAX_DEPTH} below the turn was given a number for a child"
        );

        assert_ne!(beneath, DelegateId::nth(1), "d2.1 and d1 are one number");
        assert_ne!(
            deepest,
            DelegateId::nth(2).child(3).expect("in range"),
            "d2.1.3 and d2.3 are one number"
        );
        assert!(second < beneath && beneath < DelegateId::nth(3));
    }

    /// Beneath means spawned by, at any distance. A sibling, the run itself and anything above
    /// it are not, and those are the three a handle relaying attribution must refuse to name.
    #[test]
    fn only_a_descendant_is_beneath_a_delegate() {
        let first = DelegateId::nth(1);
        let child = first.child(2).expect("in range");
        let grandchild = child.child(1).expect("in range");

        assert!(child.is_beneath(first));
        assert!(grandchild.is_beneath(first));
        assert!(grandchild.is_beneath(child));

        assert!(!first.is_beneath(first), "a delegate is beneath itself");
        assert!(!first.is_beneath(child), "a parent is beneath its child");
        assert!(
            !DelegateId::nth(2).is_beneath(first),
            "a sibling is beneath"
        );
        assert!(
            !DelegateId::nth(2)
                .child(2)
                .expect("in range")
                .is_beneath(first),
            "a sibling's child is beneath"
        );
        assert!(
            !first.child(1).expect("in range").is_beneath(child),
            "a sibling at the same depth is beneath"
        );
    }

    /// One count for the tree, shared by every handle on it, and it stops at the bound rather
    /// than wrapping or going over.
    #[test]
    fn a_tree_holds_at_most_its_bound_across_every_handle() {
        let tree = Tree::default();
        let sibling = tree.clone();
        let claimed = (0..MAX_DELEGATES * 2)
            .filter(|n| {
                if n % 2 == 0 {
                    tree.claim()
                } else {
                    sibling.claim()
                }
            })
            .count();
        assert_eq!(claimed, MAX_DELEGATES as usize);
        assert!(!tree.claim() && !sibling.claim());
        assert!(
            Tree::default().claim(),
            "another turn's tree shared this one's count"
        );
    }
}
