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
//! - **Its bound**, which is its kind's. Nobody is watching a delegate the way a person watches
//!   a turn, and the thing being bounded is futility rather than danger.
//!
//! It still asks. Every write and every run a delegate performs passes the same gates with the
//! same single-use endorsements, so a person sees the path and the diff whoever proposed them.
//! What a delegate saves is context, never approval.

use crate::capability::{Capability, CapabilitySet};

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
    /// A reader that may also run programs, so it can build and test.
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
    pub fn capabilities(self) -> CapabilitySet {
        match self {
            Self::Reader => CapabilitySet::from_iter([Capability::WebFetch, Capability::FileRead]),
            Self::Checker => CapabilitySet::from_iter([
                Capability::WebFetch,
                Capability::FileRead,
                Capability::ShellExec,
            ]),
            Self::Worker => CapabilitySet::from_iter([
                Capability::WebFetch,
                Capability::FileRead,
                Capability::FileWrite,
                Capability::ShellExec,
            ]),
        }
    }

    /// How many rounds of tool calls this kind may make before it has to answer.
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

    /// What to tell the planner this kind is for, in the tool's own schema.
    pub fn purpose(self) -> &'static str {
        match self {
            Self::Reader => {
                "reads, lists, searches and runs processors; writes nothing and runs nothing"
            }
            Self::Checker => {
                "a reader that may also run programs, so it can build, test and lint; writes \
                 nothing"
            }
            Self::Worker => "a checker that may also write files, so it can finish a sub-task",
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
/// network so the driver can make its model call, and the rest because their audience is the
/// person watching the turn. The list is here rather than beside the tool table so that a
/// definition naming one of them is answered by the same set the tool list is built from.
///
/// `spawn_agent` is not here. Whether a delegate may delegate is a question about where it sits,
/// not about what it is, so it is answered by [`MAX_DEPTH`] rather than by name.
pub const NEVER_DELEGATED: [&str; 5] = [
    "ask_user",
    "todo_write",
    "schedule_next",
    "fetch_url",
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
        "read_file" | "list_files" | "search" | "spawn_processor" | "load_skill" => {
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

/// The tools a replacement is confined to: its own `tools:` line met with the one it replaced.
///
/// `None` means the kind's whole set, so it is the wider of the two and loses to any list at all,
/// including to an empty one. Two lists meet name by name in the replacement's order, so a
/// replacement written for another agent, whose names are that agent's vocabulary, comes out with
/// nothing in common rather than with either side's list.
fn meet_tools(asked: Option<&[String]>, replaced: Option<&[String]>) -> Option<Vec<String>> {
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
            tools: None,
            skills: None,
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
            tools,
            skills: None,
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

    /// The skills it asked to be offered, before the turn's own catalogue narrows them.
    pub fn skills(&self) -> Option<&[String]> {
        self.skills.as_deref()
    }

    /// Offer this delegate only the skills of these names that the turn found.
    pub fn with_skills(mut self, skills: Vec<String>) -> Self {
        self.skills = Some(skills);
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
        held.iter()
            .filter(|capability| {
                HELD_WHATEVER_IT_NAMED.contains(capability)
                    || tools
                        .iter()
                        .any(|tool| reachable_by(tool).as_ref() == Some(capability))
            })
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
/// The words are the loader's: this is the kernel, and what it has to hand over is which of the
/// two axes moved rather than a sentence about it. [`Admitted::Narrowed`] is answered only where
/// one of them did, so a value of this always has something to say.
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
    /// takes over the description, the body, the model and the skills, and is cut down on the two
    /// fields that decide what it may do: it is loaded as the narrower of the two kinds, and its
    /// `tools:` line is met with the one it replaced. So a project cannot turn a `reader` a
    /// person wrote into a `worker`, and cannot hand back a tool that person's own `tools:` line
    /// had taken away. Widening it would make the checked-in file the author of authority rather
    /// than the person who vouched for the checkout, which is the sentence [`Definition`] is
    /// built around, and the vouch that let the file be read at all is a decision about the
    /// project rather than about this name.
    ///
    /// The skills are taken over rather than met because a skill is guidance, as the body is: a
    /// list of them chooses which of the turn's own skills a delegate is told about, and the turn
    /// found every one of those whichever file named them.
    ///
    /// **Both fields rather than a ceiling beside them**, so that what a definition holds is
    /// still read off the definition, and the trail a delegate leaves names the kind and the
    /// tools it actually got. What it asked for and did not get comes back instead, so whoever
    /// wrote it is told rather than left to find the narrowing by running it.
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
        let asked = definition.tools.take();
        let tools = meet_tools(asked.as_deref(), existing.tools.as_deref());
        // `None` is the kind's whole set, so it differs from any list at all, and a replacement
        // that named no tools and inherited one has been confined as surely as one whose own
        // list was cut down.
        let confined_to = (tools != asked).then(|| tools.clone().unwrap_or_default());
        let replaced = existing.origin.clone();

        definition.kind = loaded;
        definition.tools = tools;
        *existing = definition;

        if named == loaded && confined_to.is_none() {
            return Admitted::AsWritten;
        }
        Admitted::Narrowed(Narrowing {
            named,
            loaded,
            confined_to,
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
        self.0
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |held| {
                (held < MAX_DELEGATES).then_some(held + 1)
            })
            .is_ok()
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
    /// The tools its definition named, where it named any, already without the ones its kind
    /// does not reach.
    tools: Option<Vec<String>>,
    /// The skills its definition named, where it named any, not yet met with what the turn found.
    skills: Option<Vec<String>>,
    /// The standing part of what it is told about itself, from the definition that selected it.
    prompt: String,
    task: String,
    capabilities: CapabilitySet,
    rounds: usize,
    /// The tree it belongs to, which is the one the delegates it spawns draw on.
    tree: Tree,
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
            tools,
            skills: definition.skills().map(<[String]>::to_vec),
            prompt: definition.prompt().to_string(),
            task: task.into(),
            capabilities,
            rounds,
            tree,
        }
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
        let held: Vec<&str> = self.capabilities.iter().map(|c| c.as_str()).collect();
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

    /// Every tool this turn is offered, already narrowed by what it holds and by what the
    /// definition named.
    pub fn tools(&self) -> &[String] {
        &self.tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            // about, so asking about a single alias would leave every other one unasked.
            assert!(
                !held
                    .iter()
                    .any(|capability| matches!(capability, Capability::McpCall(_))),
                "{name}"
            );
            assert!(!held.contains(&Capability::GitWrite), "{name}");
        }
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
        assert_eq!(
            admitted,
            Admitted::Narrowed(Narrowing {
                named: Kind::Worker,
                loaded: Kind::Worker,
                confined_to: Some(vec!["read_file".to_string()]),
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
