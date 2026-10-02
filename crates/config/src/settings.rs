//! The settings files, from `~/.bravebot` and from `.bravebot` beside the work.
//!
//! A file rather than only the process environment, because the values that select a backend are
//! long-lived: which AWS profile to assume and which model each tier names are properties of a
//! person's account, not of the shell a session happened to start in. Exporting them from a shell
//! profile works and keeps working; this exists so it is not the only way.
//!
//! Three files, because an account is not the only scope a value belongs to. A profile is a
//! property of the person, a gateway a particular checkout talks to is a property of that checkout,
//! and something one machine needs is neither. The order is Claude Code's, and so are the merge
//! rules: `~/.bravebot/settings.json`, then `.bravebot/settings.json`, then
//! `.bravebot/settings.local.json`, each overriding the one before it a name at a time rather than
//! wholesale. Copying that resolution rather than inventing one means somebody who knows where to
//! put a value for one of these tools knows it for the other.
//!
//! A fourth file is read where the command line named one, after all three and by the same rules.
//! Those three are properties of a person, a checkout and a machine, and none of them is a property
//! of one invocation, which is what a job configuring one run differently from the next has to be
//! able to say. [`name_a_settings_file`] is how the entry point says it.
//!
//! Blocks borrowing the shape of whichever tool already reads them, so that one copied from
//! elsewhere works unedited rather than being rewritten first. A different spelling for the same
//! values would be a second thing to learn for no gain:
//!
//! - `env`, a flat map of strings, spelled as Claude Code's `~/.claude/settings.json` spells it,
//!   down to the variable names. The switch is this program's own name, since it selects a backend
//!   for this program, but the model names are shared because those name someone's Bedrock
//!   deployment.
//! - `model`, for the same reason: it is where both Claude Code and opencode put that choice, and a
//!   key those tools honour that this one silently dropped is worse than one nobody writes.
//! - `effort`, this program's own name, beside `model` because it answers the same kind of question
//!   about the same request: how hard the model is asked to think. Nothing else reads it, so there
//!   was no spelling to borrow.
//! - `permissions`, whose rules Claude Code spells the same way.
//! - `provider`, in opencode's shape, read by [`crate::provider`].
//! - `attribution`, Claude Code's name for what a commit message or a pull request may carry, so
//!   that a checkout asking for none of it says so once in a file rather than in prose an agent
//!   has to be reading at the moment it writes one.
//! - `vetting`, this program's own, since nothing else has the idea. It is the one block read from
//!   the **home layer alone**: see [`Settings::auto_vetting`].
//!
//! They are independent. A file configuring one has nothing to say about the others, and reading any
//! of them does not depend on another being present.
//!
//! # What these files are trusted for
//!
//! Every name in a block is read, not a chosen subset. These are the user's own configuration
//! surface, on the footing [`crate`]'s callers already treat `~/.bravebot` as: a value is something
//! the person running the agent typed, and it is trusted exactly as far as a variable they exported
//! would be. Nothing a turn produces can write one, and no model output reaches one.
//!
//! A project file is a file in a checkout, which is a weaker claim than a file in a home directory:
//! whoever wrote the checkout wrote it. Nothing here distinguishes them for most of the blocks
//! above, because the resolution this copies does not. What that costs is written down under Known
//! costs in `docs/specs/backends.md` rather than mitigated here.
//!
//! Four names are the exception, and they are the exception because of what they decide. Every
//! other name here configures where a request goes or how the interface behaves.
//!
//! - `vetting.auto` says whether a person is asked before content nobody vouched for reaches the
//!   planner, so a line in a checkout's file could turn the asking off for whoever opened the
//!   checkout. It is read from the home layer alone, and every other layer naming it is reported
//!   rather than obeyed.
//! - `permissions.allow` answers an approval prompt, so a line in a checkout's file could run a
//!   program, write a file or fetch a URL without the question anybody would otherwise have seen.
//!   It is read from the home layer, and from a file the command line named that sits outside the
//!   workspace; a checkout's entry is dropped and reported. `deny` and `ask` are read from every
//!   layer, because both only narrow. See [`grants`] and `docs/specs/permissions.md`.
//! - `provider` names where a request is sent and which of the person's environment variables are
//!   read as its credential, so a line in a checkout's file could point a request at a host of the
//!   checkout's choosing and hand it whatever secret that name reached. It is read from the home
//!   layer alone; every other layer naming the block is dropped and reported.
//! - `model` picks which of those providers answers, so a line in a checkout's file could direct
//!   the person's traffic through a backend they never chose. It is read from the home layer
//!   alone, for the same reason.
//!
//! Two more names in that block are read from every layer and merged unlike anything else here.
//! `permissions.readsStayInWorkspace` and `permissions.bypassUnreachable` only ever refuse, so a
//! checkout asking for one takes nothing from whoever cloned it, and the merge takes the strictest
//! value any layer named rather than the value of the strongest layer: there is no raisable
//! counterpart for a file above to lift one with. See [`Narrowing`] and `docs/specs/permissions.md`.
//!
//! They do not become the process environment. Values are consulted where a variable would be
//! consulted, and handed to a subprocess only where that subprocess is the thing they configure.
//! Installing them globally would put every name in the block in front of every command `run`
//! ever starts, which is a much larger claim than "this is how I reach the backend".

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// The file each layer is named by, inside its own directory.
const SETTINGS_FILE: &str = "settings.json";

/// The machine-local layer, beside the one a checkout can carry.
///
/// A separate name rather than a flag inside the other, so that keeping it out of a checkout is one
/// line in an ignore file and needs no editing of a file somebody else also edits.
const LOCAL_SETTINGS_FILE: &str = "settings.local.json";

/// The directory a checkout keeps its own settings in.
const PROJECT_DIR: &str = ".bravebot";

/// The block that says whether a check's safe verdict may promote content without a prompt.
///
/// Named as a constant because two places read it: the per-layer look that decides whether the
/// file saying it was entitled to, and the parse of one root.
const VETTING_BLOCK: &str = "vetting";

/// The key that picks a backend, which the home layer alone may name.
///
/// Named as a constant for the same reason [`VETTING_BLOCK`] is: the per-layer look that decides
/// whether the file saying it was entitled to, and the parse of one root, both spell it here.
const PROVIDER_BLOCK: &str = "provider";

/// The block holding the rules and the two keys that refuse without naming what they refuse.
///
/// Named as a constant for the reason [`VETTING_BLOCK`] is: the rule lists and [`narrowing_stated`]
/// both look inside it, and a second spelling would be a key read out of a block nothing writes.
const PERMISSIONS_BLOCK: &str = "permissions";

/// The top-level keys this build reads, for naming the ones a layer wrote beside them (BACKEND-36).
///
/// `mcpServers` is among them and configures nothing: a layer that names it is already reported as a
/// file that tried to declare a server ([`Settings::mcp_declared`]), and a key named twice in one
/// report is one mistake somebody has to work out is not two.
const READ_KEYS: &[&str] = &[
    "attribution",
    "editorMode",
    "effort",
    "env",
    "keybindings",
    "mcp",
    "mcpServers",
    "model",
    PERMISSIONS_BLOCK,
    PROVIDER_BLOCK,
    "run",
    "search",
    VETTING_BLOCK,
];

/// The most of it worth reading.
///
/// A settings file is a handful of short strings. Bounded so a file that grew by accident, or was
/// replaced by something else entirely, is refused rather than parsed.
pub(crate) const MAX_BYTES: u64 = 64 * 1024;

/// The file the command line named, for the layer that sits above the three that are found.
///
/// Process-wide because the thing it is a property of is: `--settings` configures this run of the
/// program, and [`Settings::load`] answers the interface, a one-shot run, and the list of variables
/// a subprocess is built with, none of which is reached from the entry point that parsed the flag.
/// Threading the path to each of them would be the same value passed through code that has nothing
/// to say about it, and the one caller that was missed would read a different configuration from
/// the rest of the process.
///
/// [`Settings::layered`] takes the answer as an argument instead, so every rule about how the layer
/// resolves is checked without this being set under any of it.
static NAMED: OnceLock<PathBuf> = OnceLock::new();

/// Read `path` as a settings layer above the three that are found, for the rest of this process.
///
/// Called once, from the entry point, before anything has read a setting. First call wins: a second
/// one is a caller disagreeing with the first about how this process is configured, and the half of
/// the program that had already read the answer could not be told about the change anyway.
pub fn name_a_settings_file(path: PathBuf) {
    let _ = NAMED.set(path);
}

/// The file [`name_a_settings_file`] named, for a caller reading the layers of a directory the
/// process did not start in, which [`Settings::load`] cannot.
pub fn named_settings_file() -> Option<&'static Path> {
    NAMED.get().map(PathBuf::as_path)
}

/// The `env` block, or empty when there is no file or it cannot be read.
///
/// Every failure is the same as absence. A missing home directory, no file, a syntax error, a
/// value that is not a string: none of them is worth refusing to start over, because the process
/// environment and the built-in values still describe a working backend. A file nobody can parse
/// is reported by `doctor` rather than at startup, where the person who mistyped it is not
/// necessarily the person watching.
///
/// Not comparable, because a gateway block it read may carry a token and [`crate::Secret`]
/// refuses equality. What a test wants of one of these is a field of it rather than the whole.
///
/// Clears its `env` block when it goes, that block being a buffer this program owns a credential in
/// ([CRED-23](../../../docs/specs/credential-protection.md#CRED-23)).
#[derive(Debug, Clone, Default)]
pub struct Settings {
    env: BTreeMap<String, String>,
    scrub: Vec<String>,
    permissions: PermissionLists,
    /// What the top-level `model` key named, if it named anything.
    ///
    /// Separate from `env` because it is not a variable: nothing exports `model`, and folding it
    /// into that map would make it collide with a name someone's shell already uses.
    model: Option<String>,
    /// What the top-level `effort` key named, if it named anything.
    ///
    /// The word as the file spelled it, for the reason `editor_mode` below keeps one: which words
    /// name a level is a question for the protocol that carries the field, and this crate
    /// configures a backend. A word this program does not define is dropped where the level is
    /// settled rather than here, so one spelling rule answers for a hand-edited settings file and a
    /// hand-edited record alike.
    effort: Option<String>,
    /// Whether `model` was named by a layer above the person's own file, which a saved `/model`
    /// pick ranks as (BACKEND-11). Always false as of the home-only rule below: a file above the
    /// home one no longer names `model` at all, so nothing can outrank a pick through it. Kept so
    /// callers reading the answer need no change.
    model_outranks_a_pick: bool,
    /// Whether `effort` was, on the same footing (BACKEND-43).
    effort_outranks_a_pick: bool,
    /// What the top-level `editorMode` key named, if it named anything.
    ///
    /// The word as the file spelled it, not a mode. Which words name an editing style is a question
    /// for the interface that does the editing, and this crate configures a backend: a name it does
    /// not recognise has to reach the interface to be reported there rather than be dropped here as
    /// though the file had said nothing.
    editor_mode: Option<String>,
    /// What `vetting.auto` said, where the layer that said it was entitled to.
    ///
    /// Read from the **home** layer and no other, which is why [`Settings::layered`] settles this
    /// rather than [`Settings::from_map`] being trusted with it: what the key turns off is a person
    /// being asked before content nobody vouched for reaches the planner, and a checkout is a
    /// weaker claim than a home directory (see this module's own note on what these files are
    /// trusted for). A project file naming it is reported by `doctor` and not obeyed.
    vetting: Option<bool>,
    /// The layers that named `vetting.auto` and were not obeyed, weakest first, for `doctor`.
    ///
    /// Kept rather than dropped because a setting that looks like configuration and does nothing is
    /// the one worth saying out loud. Somebody who wrote it into a checkout has to be told it was
    /// ignored, not left to wonder why the prompt still appears.
    vetting_ignored: Vec<PathBuf>,
    /// The strictest thing any layer said about the two `permissions` keys that only refuse.
    ///
    /// Settled by [`Settings::layered`] rather than read off the merged root, because the merge
    /// takes the stronger layer's value a name at a time and these take the strictest value any
    /// layer named (PERM-18): a checkout asking for the restriction would otherwise be undone by a
    /// person's own file that had answered `false`, or by one read later that said nothing.
    narrowing: Narrowing,
    /// The layers that asked for one of those restrictions, with the key, weakest first.
    ///
    /// Kept because every refusal made under one of these keys has to name the reason, and the file
    /// that asked for it is the whole of what somebody can act on: a session refusing a path with
    /// no file named is a person searching four layers for a line they did not write.
    narrowed_by: Vec<(PathBuf, &'static str)>,
    /// The layers that named one of those keys as something other than a boolean, with the key.
    ///
    /// Kept for the reason `vetting_ignored` is kept: a key that looks like confinement and does
    /// nothing is the one worth saying out loud, and somebody who quoted `"true"` has to be told it
    /// was read as absence rather than left to believe the tools are confined.
    narrowing_unreadable: Vec<(PathBuf, &'static str)>,
    /// The `allow` entries a layer not entitled to grant one wrote, with the file each came from.
    ///
    /// Kept for the reason `vetting_ignored` is kept, and it matters more: an `allow` entry is the
    /// one thing in a settings file that stops a question being asked, so a person who wrote one
    /// into a checkout and is still asked has to be told it was dropped rather than conclude the
    /// rule is in force and the prompt is a separate fault.
    allow_ignored: Vec<(PathBuf, String)>,
    /// The layers that named a `provider` block and were not obeyed, weakest first, for `doctor`.
    ///
    /// Kept for the reason `vetting_ignored` is kept: a provider block decides where a request is
    /// sent and which of the person's environment variables are read as its credential, so a
    /// checkout naming one has to be told the block was dropped rather than left to conclude the
    /// gateway it named is in force.
    provider_ignored: Vec<PathBuf>,
    /// The layers that named the top-level `model` key and were not obeyed, weakest first.
    ///
    /// Kept for the same reason, the key picking which of the providers above answers.
    model_ignored: Vec<PathBuf>,
    /// The `permissions` blocks and rule lists a layer spelled as another shape, with the file each
    /// came from. The merge keeps the weaker block or list in their place, so the merged root no
    /// longer holds them and only the layer that wrote one can say it was ignored (PERM-11).
    misshapen: Vec<(PathBuf, String)>,
    /// The keys a layer declared an MCP server under, with the file each came from, for `doctor`.
    ///
    /// Recorded rather than read: a declaration lives in the person's own directory and nowhere
    /// else (SERVERS-1), so what a settings file says about a server is never obeyed. Kept because
    /// somebody who wrote one believes it works, and a line that looks like a server and starts
    /// nothing has to be named as a mistake rather than left to look like a server that failed.
    mcp_declared: Vec<(PathBuf, String)>,
    /// The aliases a layer's `mcp.request` named, with the file each came from (SERVERS-2).
    ///
    /// Every layer's entries, for BACKEND-24's reason: a request only names a server that then has
    /// to be declared and approved in the person's own directory, so no layer's entry widens what
    /// another's allowed.
    mcp_requested: Vec<(PathBuf, String)>,
    /// The top-level keys a layer set that nothing here reads, with the file each came from, weakest
    /// first (BACKEND-36).
    ///
    /// Kept for the reason `vetting_ignored` is kept, by a shorter route than any of the keys above:
    /// this file is documented as largely the shape of another tool's, so a pasted block arrives
    /// holding `sandbox` or `hooks`, and both read to whoever wrote them as a restriction in force.
    /// Nothing else would say the key was not read, so a typo and an entry written for a later
    /// release look the same from here.
    ///
    /// The children of `env` stay out of it, BACKEND-36 already naming each among the names the file
    /// set. Names only, for the reason [`Settings::names`] carries names: a value here may be a
    /// credential.
    unread: Vec<(PathBuf, String)>,
    keybindings: BTreeMap<String, String>,
    attribution: Attribution,
    search: SearchCaps,
    /// What `run.maxOutput` said, if it said anything.
    ///
    /// `None` is the built-in cap, which is the turn's to know for the reason [`SearchCaps`] gives
    /// about its own: answering with the number here would make this crate the second place it is
    /// written down.
    run_output: Option<usize>,
    /// What `run.defaultSeconds` and `run.maxSeconds` said, where they said anything.
    run_deadlines: RunDeadlines,
    providers: Vec<crate::provider::Provider>,
    layers: Vec<PathBuf>,
    contested: BTreeMap<String, PathBuf>,
}

/// The `attribution` block: what a commit message or a pull request this program writes may carry.
///
/// A string per destination, and the empty string is a value rather than absence. Saying to carry
/// nothing is the whole reason to write the block, so a blank cannot mean the same thing here as it
/// means for `model`, where it is how somebody comments a line out. `None` is the file having said
/// nothing about that destination, which is what leaves a weaker layer's answer standing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attribution {
    /// What a commit message may carry, if the settings said.
    pub commit: Option<String>,
    /// What a pull request may carry, if the settings said.
    pub pr: Option<String>,
}

impl Attribution {
    /// Whether the block said anything.
    pub fn is_empty(&self) -> bool {
        self.commit.is_none() && self.pr.is_none()
    }
}

/// The `search` block: what bounds a search of the workspace, where a file bounds it.
///
/// `None` per cap, meaning the built-in one stands. A number carries no way to say "leave this
/// alone", and a value reserved to mean it would be a second spelling of absence for whoever has
/// to remember which number it was.
///
/// Two independent caps rather than one budget: one bounds how much of the tree is walked, the
/// other how long is spent reading what the walk selected. A tree large enough to need one is not
/// always slow enough to need the other.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchCaps {
    /// How many files a search may walk, from `maxFiles`.
    pub files: Option<usize>,
    /// How long a search may spend opening them, from `maxSeconds`.
    pub time: Option<Duration>,
}

impl SearchCaps {
    /// Whether the block said anything.
    pub fn is_empty(&self) -> bool {
        self.files.is_none() && self.time.is_none()
    }
}

/// The `run` block's wall-clock figures: what a command that names no deadline gets, and the most
/// one may name.
///
/// `None` per figure, meaning the built-in one stands, for the reason [`SearchCaps`] answers `None`
/// per cap: the built-in numbers belong to the crate that runs a command, and repeating one here
/// would be a second place it is written down.
///
/// Two figures rather than one, because raising the default is not the decision raising the ceiling
/// is. The default is what a call gets for asking nothing, so somebody whose build takes eight
/// minutes needs that raised. The ceiling is the most a call may ask for, so somebody who wants one
/// twenty-minute integration run needs that raised and the default left where it is, and a program
/// that hangs is still given up on in five minutes. One key could not say both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunDeadlines {
    /// How long a command that names no deadline of its own gets, from `defaultSeconds`.
    pub default: Option<Duration>,
    /// The longest deadline a call may name for itself, from `maxSeconds`.
    pub ceiling: Option<Duration>,
}

impl RunDeadlines {
    /// Whether the file said anything about either figure.
    pub fn is_empty(&self) -> bool {
        self.default.is_none() && self.ceiling.is_none()
    }
}

/// The `permissions` block, as text, exactly as the file spelled it.
///
/// Rule text rather than parsed rules, because reading a rule needs to know where the settings
/// file sits and where home is, and this crate is where the file was found rather than where a
/// rule is matched. The kernel owns the rule language; this hands it the lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionLists {
    pub deny: Vec<String>,
    pub ask: Vec<String>,
    pub allow: Vec<String>,
    /// Directories a file asks to have opened, alongside the working directory.
    pub additional_directories: Vec<String>,
    /// Entries of the three rule lists that were not text at all, in the spelling the file used.
    ///
    /// A rule is a line, so an entry that is not a string is nothing this layer can hand on as
    /// one. Carried out beside the lists rather than dropped here, because PERM-11 has every
    /// entry that was dropped named where a person reads it, and a drop that ends inside the
    /// parser is a deny rule that reads as protection and is not there.
    pub unreadable: Vec<String>,
}

impl PermissionLists {
    /// Whether the block said anything.
    ///
    /// An entry nothing can act on still said something: a file whose only deny rule is mistyped
    /// is a file with a `permissions` block, and reading it as absence would have `doctor` report
    /// no settings one line above the rule it could not read.
    pub fn is_empty(&self) -> bool {
        self.deny.is_empty()
            && self.ask.is_empty()
            && self.allow.is_empty()
            && self.additional_directories.is_empty()
            && self.unreadable.is_empty()
    }
}

/// The user's own settings file inside `directory`, whether or not it exists yet.
///
/// The one place a rule about which commands to ask about is written by hand, so a prompt that
/// advises writing one has to name it. Here rather than in the caller because the name of the file
/// is this module's, and a second spelling of it would have a prompt sending somebody to a path
/// nothing reads.
///
/// The user's layer and not the project one. A rule in a checkout is a rule whoever wrote the
/// checkout wrote, and advice to put a standing permission there would be advice to trust a file
/// that arrives with a clone.
pub fn user_settings_file(directory: &Path) -> PathBuf {
    directory.join(SETTINGS_FILE)
}

/// The settings file a checkout shares with whoever clones it, whether or not it exists yet.
///
/// Named here for the same reason as [`user_settings_file`]: a command that writes a layer has to
/// write the file [`Settings::layered`] reads, and a second spelling of the path would be a write
/// nothing reads.
pub fn project_settings_file(checkout: &Path) -> PathBuf {
    checkout.join(PROJECT_DIR).join(SETTINGS_FILE)
}

/// The settings file a checkout keeps for this person alone, whether or not it exists yet.
pub fn local_settings_file(checkout: &Path) -> PathBuf {
    checkout.join(PROJECT_DIR).join(LOCAL_SETTINGS_FILE)
}

impl Settings {
    /// Read every settings layer in force for this user, in this directory, plus the file the
    /// command line named.
    pub fn load() -> Self {
        Self::layered(
            home(),
            std::env::current_dir().ok().as_deref(),
            named_settings_file(),
        )
    }

    /// As [`Settings::load`], for a named home, working directory and command line, so a test needs
    /// no ambient ones.
    ///
    /// The working directory is the session's own and not an ancestor of it. A session begun
    /// in a subdirectory therefore reads no project settings, which is the same rule Claude Code
    /// applies and is the reason this walks nothing: a search upward would make what configures a
    /// session depend on which directory somebody happened to `cd` into, and the file it eventually
    /// found could sit above the thing being worked on.
    ///
    /// `named` is read last, so what it sets beats every file that was found. Taken as an argument
    /// rather than read from [`NAMED`] here, so the order and the merge rules are checked without a
    /// process-wide switch in force under every other test in this binary.
    pub fn layered(home: Option<PathBuf>, cwd: Option<&Path>, named: Option<&Path>) -> Self {
        Self::layered_since(home, cwd, named, None)
    }

    /// As [`Settings::layered`], for a session in `cwd` that began in `started`.
    ///
    /// A named file inside either is a checkout's file: moving out of the checkout it sits in does
    /// not make it the person's.
    pub fn layered_since(
        home: Option<PathBuf>,
        cwd: Option<&Path>,
        named: Option<&Path>,
        started: Option<&Path>,
    ) -> Self {
        let home_layer = home.map(|home| user_settings_file(&home));
        let paths = [
            home_layer.clone(),
            cwd.map(project_settings_file),
            cwd.map(local_settings_file),
            named.map(Path::to_path_buf),
        ];

        let mut merged = Document::default();
        let mut found = Vec::new();
        let mut winner = BTreeMap::new();
        let mut contested = BTreeMap::new();
        // Settled per layer rather than off the merged root, because the merge cannot say which
        // file a name came from and this is the one name where that decides whether it is obeyed.
        let mut vetting = None;
        let mut vetting_ignored = Vec::new();
        // Settled per layer as well, and for the opposite reason: every layer may ask for one of
        // these, and what is wanted is the strictest thing any of them said rather than what the
        // last one to speak said. The merged root can only give the second (PERM-18).
        let mut narrowing = Narrowing::default();
        let mut narrowed_by: Vec<(PathBuf, &'static str)> = Vec::new();
        let mut narrowing_unreadable: Vec<(PathBuf, &'static str)> = Vec::new();
        // Settled per layer for the same reason, and for a stronger one: the merge unions every
        // list, so an `allow` entry a checkout wrote would otherwise be indistinguishable from one
        // the person wrote in their own file. See [`Settings::allow_ignored`].
        let mut allow = Vec::new();
        let mut allow_ignored = Vec::new();
        // Settled per layer for the same reason as `vetting`: a provider block or a `model` key a
        // checkout wrote picks where a request is sent and which of the person's variables are read
        // as its credential, so the merge must never see it. `model` and `provider` are removed
        // from the layer's own root below, before it reaches the merge.
        let mut provider_ignored = Vec::new();
        let mut model_ignored = Vec::new();
        let mut misshapen = Vec::new();
        let mut mcp_declared = Vec::new();
        let mut mcp_requested: Vec<(PathBuf, String)> = Vec::new();
        // Settled per layer because the merge cannot say which file a key came from, and a key
        // nothing reads is only worth reporting against the file somebody has to edit to remove it.
        let mut unread: Vec<(PathBuf, String)> = Vec::new();
        // Which kind of layer spelled `effort` last, which is the layer the merge lets answer for
        // it. The merged root cannot say, and it decides whether a saved pick outranks the answer.
        // The named file is read after all three and is the one layer above the home one that may
        // still name `model` and `provider`: the flag naming it is the person's own act
        // (BACKEND-24), where the project and local layers arrive with the clone.
        let named_path = named.map(Path::to_path_buf);
        let mut model_above_home = false;
        let mut effort_above_home = false;
        for path in paths.into_iter().flatten() {
            // A file already read as a layer above is not read again. Naming one of the three
            // explicitly is an ordinary thing to do, and reading it twice would report every name
            // in it as an override of itself, list it twice among the layers, and double every
            // entry in a list that unions rather than overrides.
            if found.contains(&path) {
                continue;
            }
            let Some(mut root) = read(&path) else {
                continue;
            };
            let own = Some(&path) == home_layer.as_ref();
            let named_here = Some(&path) == named_path.as_ref();
            if !own && !named_here {
                // The keys that pick a backend are the person's to pick, not a checkout's: removed
                // here rather than read and dropped later, because the merge unions every name a
                // layer set and a removed key cannot reach [`Settings::from_map`] at all.
                if root.remove("model") {
                    model_ignored.push(path.clone());
                }
                if root.remove(PROVIDER_BLOCK) {
                    provider_ignored.push(path.clone());
                }
            }
            if root.contains_key(VETTING_BLOCK) {
                match own {
                    true => vetting = auto_vetting(&root),
                    false => vetting_ignored.push(path.clone()),
                }
            }
            // Every layer, the person's own and a checkout's alike (PERM-18). Neither key can be
            // lifted by a layer above, so which file asked decides only what a refusal names.
            let (stated, unreadable) = narrowing_stated(&root);
            narrowing = narrowing.strictest(stated);
            if stated.reads_stay_in_workspace == Some(true) {
                narrowed_by.push((path.clone(), Narrowing::READS_STAY_IN_WORKSPACE));
            }
            if stated.bypass_unreachable == Some(true) {
                narrowed_by.push((path.clone(), Narrowing::BYPASS_UNREACHABLE));
            }
            for key in unreadable {
                narrowing_unreadable.push((path.clone(), key));
            }
            if root.contains_key("model") {
                model_above_home = !own;
            }
            if root.contains_key("effort") {
                effort_above_home = !own;
            }
            // Every layer, the person's own among them: what a settings file names about a server
            // is never a declaration, so which file said it decides only what the report names.
            for key in server_keys(&root) {
                mcp_declared.push((path.clone(), key));
            }
            for alias in requested_servers(&root) {
                if !mcp_requested.iter().any(|(_, held)| *held == alias) {
                    mcp_requested.push((path.clone(), alias));
                }
            }
            let granting = grants(&path, home_layer.as_deref(), named, cwd, started);
            for rule in permission_lists(&root).allow {
                // A blank entry goes through whatever layer wrote it. It grants nothing whoever
                // wrote it, since the rule language calls an empty rule empty and refuses it, and
                // reporting it here would say a rule was withheld where PERM-11 is already about
                // to say there was no rule.
                match granting || rule.trim().is_empty() {
                    true => allow.push(rule),
                    false => allow_ignored.push((path.clone(), rule)),
                }
            }
            for value in misshapen_rule_lists(&root) {
                misshapen.push((path.clone(), value));
            }
            for name in env_names(&root) {
                // Whoever set it before lost it here, which is the only thing worth telling somebody:
                // a name one file sets needs no explanation of where it came from.
                if winner.insert(name.clone(), path.clone()).is_some() {
                    contested.insert(name, path.clone());
                }
            }
            // Read off this layer's own root rather than the merged one: the merge keeps every key a
            // layer set, so a key two files wrote would be reported once against whichever of them
            // spoke last, and the other file would go unnamed.
            for key in unread_keys(&root) {
                unread.push((path.clone(), key));
            }
            found.push(path);
            merge(&mut merged, root.take());
        }

        let mut settings = Self::from_map(&merged);
        settings.layers = found;
        settings.contested = contested;
        // Overwritten rather than merged in, so that the only value here is the home layer's own.
        // A project file that set it has already been recorded as ignored above, and what it said
        // cannot reach this even where the home layer said nothing.
        settings.vetting = vetting;
        settings.vetting_ignored = vetting_ignored;
        // Overwritten for the reason above it: the merged root holds whichever layer spoke last
        // about either key, and what is in force is the strictest thing any of them said.
        settings.narrowing = narrowing;
        settings.narrowed_by = narrowed_by;
        settings.narrowing_unreadable = narrowing_unreadable;
        // Overwritten for the same reason, and the merged block is what it replaces: `deny` and
        // `ask` keep every layer's entries because both only ever narrow, and this one is put back
        // to the entries a layer entitled to grant wrote.
        settings.permissions.allow = allow;
        settings.allow_ignored = allow_ignored;
        settings.misshapen = misshapen;
        settings.mcp_declared = mcp_declared;
        settings.mcp_requested = mcp_requested;
        settings.unread = unread;
        // Overwritten rather than merged in, for the reason `provider_ignored` is kept: what every
        // layer but the named one stated has been dropped above, so what stands here is the home
        // layer's own or the file the person named, and a saved pick cannot be outranked by a file
        // it was never told about.
        settings.model_outranks_a_pick = model_above_home && settings.model.is_some();
        settings.effort_outranks_a_pick = effort_above_home && settings.effort.is_some();
        settings.provider_ignored = provider_ignored;
        settings.model_ignored = model_ignored;
        // `merged` goes here, and clears what every layer stated as it does: the settings hold what
        // they keep of it by now, so the rest is a spare copy of a gateway token.
        settings
    }

    /// Read one layer, for a home directory and nothing beside it.
    pub fn from_home(home: Option<PathBuf>) -> Self {
        Self::layered(home, None, None)
    }

    /// Read the `env` block, the `model` key and the scrub list out of settings JSON.
    ///
    /// Only string values are taken. JSON allows a number or a boolean where a variable wants a
    /// string, and coercing one would invent a spelling the writer did not choose: `1` and `true`
    /// are not obviously `"1"` and `"true"` to whoever has to debug it later.
    ///
    /// Each is read independently, so a file with one and not the others still supplies what it
    /// has: a `model` key is the whole of some people's settings, and requiring an `env` block
    /// beside it would discard it for being alone.
    pub fn parse(text: &str) -> Self {
        let Ok(root) = Document::of(text) else {
            return Self::default();
        };
        Self::from_map(&root)
    }

    /// The blocks in one settings root, which is a file or several merged into one.
    ///
    /// Merging happens before this, on the JSON, so that every block is read exactly once from a
    /// root that already holds what won. Reading each layer separately and combining the results
    /// afterwards would need this logic twice, once per block, and the second copy is where the two
    /// would drift.
    pub(crate) fn from_map(root: &serde_json::Map<String, serde_json::Value>) -> Self {
        let env = match root.get("env") {
            Some(serde_json::Value::Object(block)) => block
                .iter()
                .filter_map(|(name, value)| match value {
                    serde_json::Value::String(value) => Some((name.clone(), value.clone())),
                    _ => None,
                })
                .collect(),
            _ => BTreeMap::new(),
        };
        Self {
            env,
            scrub: scrub_list(root),
            permissions: permission_lists(root),
            model: word(root, "model"),
            effort: word(root, "effort"),
            // False here, one root being read as the person's own until [`Settings::layered`]
            // says which file it was.
            model_outranks_a_pick: false,
            effort_outranks_a_pick: false,
            editor_mode: word(root, "editorMode"),
            // Read here so one file's worth can be parsed on its own, and overwritten by
            // [`Settings::layered`], which is the only caller that knows which layer this came
            // from and so the only one entitled to answer.
            vetting: auto_vetting(root),
            vetting_ignored: Vec::new(),
            // Read here so one root's worth can be parsed on its own, which is what the managed
            // layer is, and overwritten by [`Settings::layered`] for the four layers it reads: the
            // strictest answer is a fact about the set of them rather than about one root.
            narrowing: narrowing_stated(root).0,
            narrowed_by: Vec::new(),
            narrowing_unreadable: Vec::new(),
            // Empty here, and filled by [`Settings::layered`] for the same reason: one root does
            // not say which file it was read out of, and that is the whole of what decides whether
            // an `allow` entry in it grants anything.
            allow_ignored: Vec::new(),
            // Filled by [`Settings::layered`], which knows which file each key was written in.
            misshapen: Vec::new(),
            mcp_declared: Vec::new(),
            mcp_requested: Vec::new(),
            unread: Vec::new(),
            keybindings: keybindings_block(root),
            attribution: attribution_block(root),
            search: search_caps(root),
            run_output: run_output_cap(root),
            run_deadlines: run_deadlines(root),
            providers: crate::provider::Provider::all(root),
            layers: Vec::new(),
            contested: BTreeMap::new(),
            provider_ignored: Vec::new(),
            model_ignored: Vec::new(),
        }
    }

    /// What the settings in force say about keybindings.
    ///
    /// Maps an action name (e.g. "stash", "scroller") to its configured key chord (e.g. "ctrl-s").
    pub fn keybindings(&self) -> &BTreeMap<String, String> {
        &self.keybindings
    }

    /// What the settings in force say a variable is, if they say anything.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.env.get(name).map(String::as_str)
    }

    /// The model the settings in force asked for, if they asked for one.
    ///
    /// Not always the model: a choice `/model` saved ranks as the person's own file does, so it
    /// outranks this where the person's own file is what named it.
    /// [`Settings::model_outranks_a_pick`] says which.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Whether [`Settings::model`] came from a file above the person's own: a checkout's, or the
    /// one `--settings` named. Always false now: such a file may not name `model` at all, so a
    /// saved pick is never outranked through it, and [`Settings::model_ignored`] is where a file
    /// that tried is reported.
    pub fn model_outranks_a_pick(&self) -> bool {
        self.model_outranks_a_pick
    }

    /// How hard the settings in force asked the model to think, if they asked for anything.
    ///
    /// The word the file spelled, unrecognised words and all, for the reason
    /// [`Settings::editor_mode`] answers with one. Ranked against a saved `/effort` pick the way
    /// [`Settings::model`] is, which [`Settings::effort_outranks_a_pick`] settles.
    pub fn effort(&self) -> Option<&str> {
        self.effort.as_deref()
    }

    /// Whether [`Settings::effort`] came from a file above the person's own (BACKEND-43).
    pub fn effort_outranks_a_pick(&self) -> bool {
        self.effort_outranks_a_pick
    }

    /// The editing style the settings in force asked for, if they asked for one.
    ///
    /// The word the file spelled, unrecognised words and all. A default rather than the style in
    /// force: the interface records a choice that outlives the session making it, and that choice
    /// wins. This is what answers for somebody who has never made one.
    pub fn editor_mode(&self) -> Option<&str> {
        self.editor_mode.as_deref()
    }

    /// What `vetting.auto` said in the home layer, if it said anything.
    ///
    /// `None` where no file named it, and `None` too where the only file that named it was a
    /// checkout's: the value a project file carried is not an answer this can give, and
    /// [`Settings::vetting_ignored`] is where such a file is reported instead.
    ///
    /// A default rather than the answer in force. What a person recorded for themselves outranks
    /// it and a flag outranks both; `bravebot_core::vetting::auto` is the whole of that rule.
    pub fn auto_vetting(&self) -> Option<bool> {
        self.vetting
    }

    /// The files that named `vetting.auto` and were not obeyed, weakest first, for `doctor`.
    pub fn vetting_ignored(&self) -> impl Iterator<Item = &Path> {
        self.vetting_ignored.iter().map(PathBuf::as_path)
    }

    /// The strictest thing any layer in force said about the two keys that only refuse.
    ///
    /// Every layer may say it, unlike `vetting.auto`, and no layer can lift what another asked for
    /// (PERM-18). A caller that also reads the managed layer takes
    /// [`Narrowing::strictest`] of the two, that file being another layer and the strongest.
    pub fn narrowing(&self) -> Narrowing {
        self.narrowing
    }

    /// The weakest layer that asked for `key`, where any layer asked for it.
    ///
    /// The file rather than the answer, because the answer is [`Settings::narrowing`] and this is
    /// what a refusal has to name: the point of a standing refusal is that nothing a session does
    /// explains it, so the line that asked for it is the only thing a person can act on. The
    /// weakest of them, since that is the file that established the restriction; a layer above
    /// restating it adds nothing to take away.
    pub fn narrowed_by(&self, key: &str) -> Option<&Path> {
        self.narrowed_by
            .iter()
            .find(|(_, named)| *named == key)
            .map(|(path, _)| path.as_path())
    }

    /// The layers that named one of those keys as something other than a boolean, with the key.
    ///
    /// For `doctor` and for the session that read the file. A value that is not a boolean is
    /// absence ([`Narrowing`]), and a refusal somebody believes is in force and is not is the one
    /// failure here worth interrupting them over, which is PERM-11's reasoning about a rule applied
    /// to a key.
    pub fn narrowing_unreadable(&self) -> impl Iterator<Item = (&Path, &str)> {
        self.narrowing_unreadable
            .iter()
            .map(|(path, key)| (path.as_path(), *key))
    }

    /// The `allow` entries that were dropped, and the file each was written in, weakest first.
    ///
    /// An `allow` entry answers a prompt, so reading one is granting a capability rather than
    /// narrowing one, and a checkout's file is not a claim this program lets anybody make on the
    /// person running it. The entries are dropped from [`Settings::permissions`] and reported
    /// here: on `doctor`, and in the session that read the file.
    ///
    /// The rule text as the file spelled it, because that is what the person who wrote it will
    /// search for. `deny` and `ask` are not here and never will be: both only ever narrow, so
    /// every layer's entries still hold.
    pub fn allow_ignored(&self) -> impl Iterator<Item = (&Path, &str)> {
        self.allow_ignored
            .iter()
            .map(|(path, rule)| (path.as_path(), rule.as_str()))
    }

    /// The `permissions` blocks and `deny`, `ask` and `allow` values that were not the shape rules
    /// are read from, and the file each was written in, weakest first.
    ///
    /// Each is spelled with the names that lead to it, `{"permissions":{"deny":null}}`, so the
    /// person can find it. None of them set or removed a rule: the weaker layers' lists stand.
    pub fn misshapen_rule_lists(&self) -> impl Iterator<Item = (&Path, &str)> {
        self.misshapen
            .iter()
            .map(|(path, value)| (path.as_path(), value.as_str()))
    }

    /// The keys a settings layer declared an MCP server under, and the file each was written in.
    ///
    /// None of them declares anything: a server is declared in `~/.bravebot/mcp.json` and nowhere
    /// else, because a stdio declaration is a command to run and two of these layers sit in a
    /// checkout the agent can write to. `doctor` reports each one as a parse error.
    pub fn mcp_declared(&self) -> impl Iterator<Item = (&Path, &str)> {
        self.mcp_declared
            .iter()
            .map(|(path, key)| (path.as_path(), key.as_str()))
    }

    /// The aliases a settings layer requested with `"mcp": { "request": [...] }`, each once, with the
    /// first file that named it, weakest first (SERVERS-2).
    ///
    /// A request grants nothing. It names a server the person has to have declared in their own
    /// directory, and approved there, before anything is started for it.
    pub fn mcp_requested(&self) -> impl Iterator<Item = (&Path, &str)> {
        self.mcp_requested
            .iter()
            .map(|(path, alias)| (path.as_path(), alias.as_str()))
    }

    /// The top-level keys the layers set that nothing here reads, with their files, weakest first.
    ///
    /// Names only, for the reason [`Settings::names`] reports names: a value here may be a
    /// credential, and a diagnostic that prints one is a diagnostic people paste into issues. The
    /// file still applies, as it does for every other key reported this way.
    pub fn unread_keys(&self) -> impl Iterator<Item = (&Path, &str)> {
        self.unread
            .iter()
            .map(|(path, key)| (path.as_path(), key.as_str()))
    }

    /// What the settings in force say a commit message and a pull request may carry.
    ///
    /// A name the block set is an answer even when it is empty, empty being how a file says to
    /// carry nothing. Nothing here writes either one: this is where a writer of one asks.
    pub fn attribution(&self) -> &Attribution {
        &self.attribution
    }

    /// What the settings in force put a search of the workspace under, cap by cap.
    ///
    /// A cap nobody named is `None` rather than the built-in number, because the built-in one is
    /// the workspace's to know: answering with it here would make this crate the second place the
    /// default is written down, and the two would drift.
    pub fn search(&self) -> &SearchCaps {
        &self.search
    }

    /// How much of what a program printed the settings in force let into the conversation.
    ///
    /// `None` where nobody named one, for the reason [`Settings::search`] answers `None` per cap:
    /// the built-in figure belongs to the turn that spends the context, and answering with it here
    /// would put a second copy of it in this crate.
    pub fn run_output_cap(&self) -> Option<usize> {
        self.run_output
    }

    /// How long the settings in force let a command run, and the most a call may name for itself.
    ///
    /// `None` per figure for the reason [`Settings::run_output_cap`] answers `None`: the built-in
    /// numbers belong to the crate that waits on the program, and answering with one here would put
    /// a second copy of it in this crate. What the two come to together, once the built-in figures
    /// are applied, is resolved by the caller that read these settings.
    pub fn run_deadlines(&self) -> RunDeadlines {
        self.run_deadlines
    }

    /// Whether anything was set at all.
    pub fn is_empty(&self) -> bool {
        self.env.is_empty()
            && self.scrub.is_empty()
            && self.permissions.is_empty()
            && self.model.is_none()
            && self.effort.is_none()
            && self.editor_mode.is_none()
            && self.vetting.is_none()
            && self.narrowing.is_empty()
            // A key named as something other than a boolean said something too, and `doctor` names
            // the file that holds it, so reading it as no settings at all would contradict the line
            // under it.
            && self.narrowing_unreadable.is_empty()
            && self.keybindings.is_empty()
            && self.attribution.is_empty()
            && self.search.is_empty()
            && self.run_output.is_none()
            && self.run_deadlines.is_empty()
            && self.providers.is_empty()
            // A file that named `vetting.auto` and was not obeyed still said something, and
            // `doctor` reports both facts about it. Reading it as absence would print "no
            // settings.json" one line above the path of the file that holds it.
            && self.vetting_ignored.is_empty()
            // And a file whose only `permissions` entry was an `allow` one that was dropped, for
            // the same reason: `doctor` names that file, so reporting it as no settings at all
            // would contradict the line under it.
            && self.allow_ignored.is_empty()
            // And a file whose only rule list was misshapen, which `doctor` names too.
            && self.misshapen.is_empty()
            // And a file that only tried to declare a server, which `doctor` names too.
            && self.mcp_declared.is_empty()
            && self.mcp_requested.is_empty()
            // And a file whose only key is one nothing here reads: `doctor` names that file, so
            // reporting it as no settings at all would contradict the line under it.
            && self.unread.is_empty()
            // And a file that named a backend or a model from a layer not entitled to, which
            // `doctor` names for the reason it names the two above.
            && self.provider_ignored.is_empty()
            && self.model_ignored.is_empty()
    }

    /// The rule text and added directories the `permissions` block carried.
    pub fn permissions(&self) -> &PermissionLists {
        &self.permissions
    }

    /// The gateways these settings configured, in the order they were listed.
    pub fn providers(&self) -> &[crate::provider::Provider] {
        &self.providers
    }

    /// The files that named a `provider` block from a layer not entitled to, weakest first.
    ///
    /// A provider block decides where a request is sent and which of the person's environment
    /// variables are read as its credential, so a checkout naming one is dropped here and named
    /// rather than obeyed.
    pub fn providers_ignored(&self) -> impl Iterator<Item = &Path> {
        self.provider_ignored.iter().map(PathBuf::as_path)
    }

    /// The files that named the top-level `model` key from a layer not entitled to, weakest first.
    pub fn model_ignored(&self) -> impl Iterator<Item = &Path> {
        self.model_ignored.iter().map(PathBuf::as_path)
    }

    /// The files that were read, weakest first, for `doctor` to report.
    ///
    /// Only the ones that existed and parsed. A layer nobody wrote is absence rather than an entry,
    /// because a diagnostic listing every place a file could have been is a diagnostic where the two
    /// that exist are the hard part to find.
    pub fn layers(&self) -> impl Iterator<Item = &Path> {
        self.layers.iter().map(PathBuf::as_path)
    }

    /// Variables more than one layer set, with the file that won, for `doctor` to report.
    ///
    /// Only the contested ones. A name a single file sets needs no explanation of where it came from,
    /// and listing every name against a path would bury the two that are surprising.
    pub fn overridden(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.contested
            .iter()
            .map(|(name, path)| (name.as_str(), path.as_path()))
    }

    /// Variables this file says to keep from a program the agent runs, beyond the built-in set.
    ///
    /// Names only, which is the whole reason this may live in a file at all: naming a variable
    /// takes something away from a subprocess and can grant nothing. A value here could put a
    /// credential in front of every command instead, which is what the `env` block declines to do.
    pub fn scrubbed(&self) -> impl Iterator<Item = &str> {
        self.scrub.iter().map(String::as_str)
    }

    /// Every name the file set, for `doctor` to report.
    ///
    /// Names only. The values include credentials on some machines, and a diagnostic that prints
    /// them is a diagnostic people paste into issues. The keys that are not variables are among them
    /// so a file that sets only one of those is not reported as setting nothing.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.model
            .is_some()
            .then_some("model")
            .into_iter()
            .chain(self.effort.is_some().then_some("effort"))
            .chain(self.editor_mode.is_some().then_some("editorMode"))
            .chain(self.vetting.is_some().then_some("vetting.auto"))
            .chain(self.narrowing.named())
            .chain((!self.keybindings.is_empty()).then_some("keybindings"))
            .chain(
                self.attribution
                    .commit
                    .is_some()
                    .then_some("attribution.commit"),
            )
            .chain(self.attribution.pr.is_some().then_some("attribution.pr"))
            .chain(self.search.files.is_some().then_some("search.maxFiles"))
            .chain(self.search.time.is_some().then_some("search.maxSeconds"))
            .chain(self.run_output.is_some().then_some("run.maxOutput"))
            .chain(
                self.run_deadlines
                    .default
                    .is_some()
                    .then_some("run.defaultSeconds"),
            )
            .chain(
                self.run_deadlines
                    .ceiling
                    .is_some()
                    .then_some("run.maxSeconds"),
            )
            .chain(self.env.keys().map(String::as_str))
    }

    /// Overwrite what the `env` block was set to, which is what the [`Drop`] below is.
    ///
    /// A method rather than the body of that drop, because a buffer this owns is unreachable once
    /// this is gone: what a test can run is the overwriting, and the drop calling it is one line.
    fn scrub_env(&mut self) {
        self.env.values_mut().for_each(crate::scrub);
    }
}

/// Clear the `env` block rather than return a credential in it to the allocator.
///
/// The block is variables, and one of the names a person may write there is the signing key: what
/// [`crate::Config`] builds out of that name is a [`crate::Secret`] that clears itself, and the map
/// it was read out of is a second buffer holding the same bytes for as long as the settings live
/// ([CRED-23](../../../docs/specs/credential-protection.md#CRED-23)).
///
/// Every value rather than the names a credential is known to arrive under, which is the reading a
/// parsed document already gets and is here for the same reason: the same block carries a region and
/// a model name, and what a person may put in it is anything. The names are left alone, a name being
/// what a value was called rather than the value.
///
/// The other fields hold no credential of their own. A gateway token among them is in a
/// [`crate::Secret`], which clears its own buffer as this goes, and so does each copy of one a
/// caller cloned out of here.
impl Drop for Settings {
    fn drop(&mut self) {
        self.scrub_env();
    }
}

/// One layer's JSON, or `None` when there is nothing there worth reading.
///
/// Every failure is the same as absence, per layer rather than for the set: a missing file, an
/// oversized one, a syntax error, or a root that is not an object. A half-typed project file leaves
/// the layers under it in force, because the alternative is a mistake in a checkout deciding that a
/// person's own profile no longer applies.
pub(crate) fn read(path: &Path) -> Option<Document> {
    match std::fs::metadata(path) {
        Ok(found) if found.len() > MAX_BYTES => return None,
        Ok(_) => {}
        Err(_) => return None,
    }
    let mut text = std::fs::read_to_string(path).ok()?;
    Document::parse(&mut text).ok()
}

/// A parsed settings document, which clears itself when it goes.
///
/// A settings file may state a gateway token, so the parse of one is a buffer this program owns a
/// credential in ([CRED-23](../../../docs/specs/credential-protection.md#CRED-23)) and the bytes go
/// to the allocator intact if it is simply dropped. A type that clears itself rather than a call at
/// each place a document dies: [`crate::Secret`] answers the same question the same way one level
/// down, and a call is a thing the next reader of a document here has to know to write.
#[derive(Default)]
pub(crate) struct Document {
    /// What the file stated.
    root: serde_json::Map<String, serde_json::Value>,
    /// What a merge took out of `root` and put something else in the place of.
    ///
    /// A stronger layer restating a gateway displaces an entry that may hold a token, and nothing
    /// reads that entry again. Kept here rather than dropped there, so that the whole of what a
    /// read parsed is cleared in one place, which is this document going.
    displaced: Vec<serde_json::Value>,
}

/// Why a file does not hold a settings document, for a caller that has to say which way it is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotADocument {
    /// It could not be read at all.
    Unreadable,
    /// It is not JSON.
    NotJson,
    /// It is JSON, and not an object.
    NotAnObject,
}

/// Whether a file holds a settings document, answered without leaving what it holds in a buffer.
///
/// The front end checks a file somebody chose before the agent reads it as a layer, and a check that
/// parsed the file itself would make a second copy of a token in it and drop that copy intact. The
/// answer comes from here instead, where the parse is behind [`Document`]
/// ([CRED-23](../../../docs/specs/credential-protection.md#CRED-23)).
///
/// The size limit is the caller's: a front end refusing a file has something to say about how large
/// one may be, and [`read`] treats an oversized file as absence rather than as a thing to report.
pub fn check_document(path: &Path) -> Result<(), NotADocument> {
    let mut text = std::fs::read_to_string(path).map_err(|_| NotADocument::Unreadable)?;
    Document::parse(&mut text).map(|_| ())
}

impl Document {
    /// The document some text holds, or which way the text is not one.
    fn of(text: &str) -> Result<Self, NotADocument> {
        match serde_json::from_str(text) {
            Ok(serde_json::Value::Object(root)) => Ok(Self {
                root,
                displaced: Vec::new(),
            }),
            // A parse that is not a document still holds whatever the file put in it, so it is
            // cleared here: this is the one exit that has a value and no document to keep it in.
            Ok(mut other) => {
                crate::scrub_value(&mut other);
                Err(NotADocument::NotAnObject)
            }
            Err(_) => Err(NotADocument::NotJson),
        }
    }

    /// The document a file's text holds, clearing the text whatever the parse made of it.
    ///
    /// Cleared before the parse is answered for, because a half-typed settings file is an ordinary
    /// thing and the token in one is still in the text. By reference rather than by value, so the
    /// buffer the caller owns is the buffer that gets cleared.
    fn parse(text: &mut String) -> Result<Self, NotADocument> {
        let document = Self::of(text.as_str());
        crate::scrub(text);
        document
    }

    /// The names out of this document, for a merge laying them into another one.
    ///
    /// What is taken is no longer this document's to clear, so the only caller is that merge, whose
    /// destination is a document too.
    fn take(&mut self) -> serde_json::Map<String, serde_json::Value> {
        std::mem::take(&mut self.root)
    }

    /// One name out of this document, or whether it named one.
    ///
    /// What came out is kept with the rest of what was displaced rather than handed back, so that
    /// a token in it is cleared where every other dropped value is ([CRED-23]). The caller gets a
    /// presence answer, which is all deciding whether the layer was entitled to the key needs.
    fn remove(&mut self, key: &str) -> bool {
        self.root.remove(key).is_some_and(|value| {
            self.displaced.push(value);
            true
        })
    }
}

impl std::ops::Deref for Document {
    type Target = serde_json::Map<String, serde_json::Value>;

    fn deref(&self) -> &Self::Target {
        &self.root
    }
}

impl Drop for Document {
    /// Every value the parse made, and every value a merge displaced.
    ///
    /// The names are left alone. A key cannot be reached through a [`serde_json::Map`] to write
    /// over, and a name is what a value was called rather than the value: what a settings file
    /// states as a credential is on the right of the colon.
    fn drop(&mut self) {
        crate::scrub_document(&mut self.root);
        self.displaced.iter_mut().for_each(crate::scrub_value);
    }
}

/// A top-level key holding one word, or `None` where the file said nothing usable.
///
/// Strings only, on the footing everything else here reads them: a number or a boolean where a word
/// belongs would have to be given a spelling nobody chose. Blank is absence rather than a choice of
/// nothing, since a key set to `""` is how somebody comments one out without deleting the line.
pub(crate) fn word(root: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    match root.get(key) {
        Some(serde_json::Value::String(word)) => Some(word.trim())
            .filter(|word| !word.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

/// The `vetting` block's `auto` key, as a boolean and nothing else.
///
/// A boolean rather than a word, because this is the one value here that is not a name being passed
/// on to something: it says whether a person is asked. `"true"` as a string, a number, and anything
/// else are absence, which leaves the layers under it in force. That is stricter than the reading
/// the `env` block gets, and deliberately: a file that meant to turn this on and mistyped the value
/// leaves the prompt appearing, which is the direction to be wrong in.
fn auto_vetting(root: &serde_json::Map<String, serde_json::Value>) -> Option<bool> {
    match root.get(VETTING_BLOCK) {
        Some(serde_json::Value::Object(block)) => match block.get("auto") {
            Some(serde_json::Value::Bool(auto)) => Some(*auto),
            _ => None,
        },
        _ => None,
    }
}

/// The `permissions` keys that only ever refuse, as one layer stated them.
///
/// Each is absence until a file says otherwise, the way `vetting.auto` is: a file naming neither
/// behaves exactly as one did before the keys existed (PERM-12). `true` is the restrictive answer,
/// and anything that is not a boolean is absence, which is the reading
/// [`auto_vetting`] gives its own key and for the same reason: a file that meant to ask for a
/// refusal and mistyped the value leaves the session as permissive as it was, and says so, rather
/// than confining it on the strength of a `"true"` somebody quoted.
///
/// Values rather than files, because a single root is where these are read from. Which file said
/// what is [`Settings::narrowed_by`], and it is a question only [`Settings::layered`] can answer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Narrowing {
    /// Whether the file tools refuse a path outside the workspace whatever else is written
    /// (PERM-16).
    pub reads_stay_in_workspace: Option<bool>,
    /// Whether the mode that asks about nothing is unreachable on this machine (PERM-17,
    /// MODE-5).
    pub bypass_unreachable: Option<bool>,
}

impl Narrowing {
    /// The key that keeps the file tools inside the workspace, as a `permissions` block spells it.
    pub const READS_STAY_IN_WORKSPACE: &'static str = "readsStayInWorkspace";

    /// The key that makes the bypass mode unreachable, as a `permissions` block spells it.
    pub const BYPASS_UNREACHABLE: &'static str = "bypassUnreachable";

    /// The strictest of what two files said, key by key.
    ///
    /// Not the stronger file's answer. Both keys only ever refuse, so there is no raisable
    /// counterpart for a stronger layer to lift one with, and the merge every other name here gets
    /// would let a file read later take away a refusal a file read earlier asked for. A weaker
    /// layer asking for the restriction gets it (PERM-18).
    #[must_use]
    pub fn strictest(self, other: Self) -> Self {
        Self {
            reads_stay_in_workspace: strictest(
                self.reads_stay_in_workspace,
                other.reads_stay_in_workspace,
            ),
            bypass_unreachable: strictest(self.bypass_unreachable, other.bypass_unreachable),
        }
    }

    /// Whether the file tools are to stay inside the workspace.
    pub fn keeps_reads_in_the_workspace(self) -> bool {
        self.reads_stay_in_workspace == Some(true)
    }

    /// Whether the bypass mode is to be unreachable.
    pub fn makes_bypass_unreachable(self) -> bool {
        self.bypass_unreachable == Some(true)
    }

    /// Whether either key was named at all.
    pub fn is_empty(self) -> bool {
        self.reads_stay_in_workspace.is_none() && self.bypass_unreachable.is_none()
    }

    /// The keys a file named, as a report names them, whichever way it answered.
    ///
    /// `false` is among them: a file that named a key said something, and a report that left it out
    /// would say the file set nothing one line above the path of the file that holds it.
    ///
    /// The lifetime is the caller's rather than `'static`, because these are chained onto a report's
    /// own borrowed names and a rigid `'static` there would be the one item type the rest of the
    /// chain could not be.
    pub fn named<'a>(self) -> impl Iterator<Item = &'a str> {
        self.reads_stay_in_workspace
            .is_some()
            .then_some("permissions.readsStayInWorkspace")
            .into_iter()
            .chain(
                self.bypass_unreachable
                    .is_some()
                    .then_some("permissions.bypassUnreachable"),
            )
    }
}

/// The stricter of two answers about a key that can only refuse.
///
/// `None` is nobody having named it, which is the weakest of the three: absence cannot beat a file
/// that said `false`, because a file that said `false` still named the key and a report has to say
/// so.
fn strictest(one: Option<bool>, other: Option<bool>) -> Option<bool> {
    match (one, other) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), _) | (_, Some(false)) => Some(false),
        (None, None) => None,
    }
}

/// The two refusing keys of one layer's `permissions` block, with every key it spelled as something
/// other than a boolean named beside them.
///
/// The unreadable ones are carried out rather than dropped, on PERM-11's reasoning applied to a key
/// rather than a rule: a mistyped `readsStayInWorkspace` reads to whoever wrote it as confinement
/// that is in force, and one dropped in silence is the failure worth interrupting somebody over.
/// The caller pairs each with the file it was written in, which is the half a root cannot say.
pub(crate) fn narrowing_stated(
    root: &serde_json::Map<String, serde_json::Value>,
) -> (Narrowing, Vec<&'static str>) {
    let Some(serde_json::Value::Object(block)) = root.get(PERMISSIONS_BLOCK) else {
        return (Narrowing::default(), Vec::new());
    };
    let mut unreadable = Vec::new();
    let mut stated = |key: &'static str| match block.get(key) {
        Some(serde_json::Value::Bool(asked)) => Some(*asked),
        Some(_) => {
            unreadable.push(key);
            None
        }
        None => None,
    };
    let reads_stay_in_workspace = stated(Narrowing::READS_STAY_IN_WORKSPACE);
    let bypass_unreachable = stated(Narrowing::BYPASS_UNREACHABLE);
    (
        Narrowing {
            reads_stay_in_workspace,
            bypass_unreachable,
        },
        unreadable,
    )
}

/// Whether a rule that *grants* may be read from this layer.
///
/// True for the home layer, and for a file the command line named that sits outside the workspace
/// and outside the one the session began in. False for `.bravebot/settings.json`, for
/// `.bravebot/settings.local.json`, and for a named file that resolves inside either.
///
/// The home layer is the person's own, on the footing this module's own note on what these files
/// are trusted for already states. A file the command line named is a path somebody typed at this
/// invocation, which is the same claim: the flag is how one run is configured differently from the
/// next. But it can name a file inside the checkout, which
/// `a_command_line_file_that_is_already_a_layer_is_read_once` exists because people do, so a README
/// saying `--settings ./tooling/bravebot.json` would be a route back in. A named file that resolves
/// inside the workspace is therefore the checkout's file under another name.
///
/// The local layer is a checkout's file too. Nothing stops one being committed, and its being
/// conventionally private is not a property this can rely on, which is the same reading `vetting`
/// already gives the pair.
fn grants(
    path: &Path,
    home_layer: Option<&Path>,
    named: Option<&Path>,
    cwd: Option<&Path>,
    started: Option<&Path>,
) -> bool {
    if Some(path) == home_layer {
        return true;
    }
    Some(path) == named && !inside(cwd, path) && !inside(started, path)
}

/// Whether `path` resolves to somewhere under `cwd`.
///
/// Both sides resolved, because the question is where the file *is* and not how it was spelled: a
/// link, a `..`, or a relative name reaches the same file under a different string, and comparing
/// the strings would answer about the spelling. A resolution that fails is read as inside, which is
/// the answer that grants nothing; a working directory nobody could name bounds nothing, so a file
/// is not inside it.
fn inside(cwd: Option<&Path>, path: &Path) -> bool {
    let Some(cwd) = cwd else { return false };
    match (std::fs::canonicalize(cwd), std::fs::canonicalize(path)) {
        (Ok(cwd), Ok(path)) => path.starts_with(cwd),
        _ => true,
    }
}

/// The keys one layer names a server under, as `doctor` should spell them back.
///
/// `mcpServers` is Claude Code's key, and a block copied from `.mcp.json` arrives under it. The `mcp`
/// block is where a checkout requests an alias (SERVERS-2), so `request` there names nothing to
/// run, and neither do `allow` and `deny`, which only keep a server from starting and are read from
/// the managed layer alone (SERVERS-12). Every other key in the block is a server, which is
/// opencode's shape for the same declaration. A block that is not an object at all is itself the
/// key, since whatever it holds is not a request.
fn server_keys(root: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    let mut keys = Vec::new();
    if root.contains_key("mcpServers") {
        keys.push("mcpServers".to_string());
    }
    match root.get("mcp") {
        Some(serde_json::Value::Object(block)) => keys.extend(
            block
                .keys()
                .filter(|key| !matches!(key.as_str(), "request" | "allow" | "deny"))
                .map(|key| format!("mcp.{key}")),
        ),
        Some(_) => keys.push("mcp".to_string()),
        None => {}
    }
    keys
}

/// The aliases one layer's `mcp.request` lists. An entry that is not a string names nothing.
fn requested_servers(root: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    match root.get("mcp").and_then(|block| block.get("request")) {
        Some(serde_json::Value::Array(names)) => names
            .iter()
            .filter_map(|name| name.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// The variables one layer's `env` block sets, for working out which layer won a name.
fn env_names(root: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    match root.get("env") {
        Some(serde_json::Value::Object(block)) => block
            .iter()
            .filter(|(_, value)| value.is_string())
            .map(|(name, _)| name.clone())
            .collect(),
        _ => Vec::new(),
    }
}

/// The top-level keys one layer set that nothing here reads (BACKEND-36).
///
/// Names only, whatever each holds: the value may be a credential, and the key is the whole of what
/// somebody needs to find the line and decide whether they meant to write it.
///
/// Every key, however it is spelled. A block spelled as the wrong type is a key this build reads and
/// is reported for that by [`Settings::misshapen_rule_lists`] instead, so the shape of a value never
/// reaches this.
fn unread_keys(root: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    root.keys()
        .filter(|key| !READ_KEYS.contains(&key.as_str()))
        .cloned()
        .collect()
}

/// Lay one settings root over another, a name at a time.
///
/// One level deep, which is Claude Code's rule rather than a general merge: `env`, `provider` and
/// `attribution` combine per name, and a value inside one of those names is replaced whole. So a
/// project file may restate an inherited `env` name or move a keybinding, and cannot reach inside
/// an inherited entry to change part of it while keeping the rest. A deeper merge would make what a
/// request carries the product of two files, and no single place to read would say where it came
/// from. (`provider` is merged here too, but a layer other than the home one has already had its
/// block removed before the merge sees it; this arm only answers for the home layer's own file.)
///
/// `run.scrubEnv` unions instead, since a name there only ever takes a variable away from a
/// subprocess. Overriding would let a layer hand back something a weaker one withheld, which is a
/// direction this list is not for.
fn merge(document: &mut Document, over: serde_json::Map<String, serde_json::Value>) {
    let Document {
        root: base,
        displaced,
    } = document;
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            // `env`, `provider`, `attribution` and `keybindings`: per-name, one level down. The
            // names under `attribution` are two unrelated destinations, so a file answering for one
            // must not answer for the other by omission: a project file naming what a pull request
            // carries would otherwise hand back the commit trailer a person's own file had turned
            // off. The chords are per-name for the same reason, a file moving one action's key
            // being no statement about the other six.
            (Some(serde_json::Value::Object(under)), serde_json::Value::Object(above))
                if key == "env"
                    || key == "provider"
                    || key == "attribution"
                    || key == "keybindings"
                    || key == "search" =>
            {
                for (name, value) in above {
                    lay_over(displaced, under, name, value);
                }
            }
            // `run` holds one list that unions and nothing else that does, so a sibling added later
            // gets whatever this arm does by default, which is to override.
            (Some(serde_json::Value::Object(under)), serde_json::Value::Object(above))
                if key == "run" =>
            {
                merge_run(displaced, under, above);
            }
            // Every rule from every layer, which is the rule the tool this borrows from applies. A
            // layer that replaced the block could drop a `deny` a weaker one set, and a permission
            // taken away by a file somebody did not open is the one outcome worth ruling out.
            (Some(serde_json::Value::Object(under)), serde_json::Value::Object(above))
                if key == "permissions" =>
            {
                merge_permissions(displaced, under, above);
            }
            // Either block spelled as anything but an object holds no list, so it removes none.
            (Some(serde_json::Value::Object(_)), value) if key == "run" || key == "permissions" => {
                displaced.push(value);
            }
            (_, value) => {
                lay_over(displaced, base, key, value);
            }
        }
    }
}

/// Lay one name over whatever a weaker layer set it to, keeping what it displaced.
///
/// A file restating a gateway replaces an entry that may hold a token, and that entry is a buffer
/// this program owns a credential in
/// ([CRED-23](../../../docs/specs/credential-protection.md#CRED-23)). Nothing reads it again, so
/// what [`serde_json::Map::insert`] does with it otherwise, which is to drop it, leaves the bytes
/// for the allocator.
///
/// It stays with the document instead of being cleared here, so that everything one read parsed is
/// cleared in the one place, which is [`Document`] going.
fn lay_over(
    displaced: &mut Vec<serde_json::Value>,
    base: &mut serde_json::Map<String, serde_json::Value>,
    key: String,
    value: serde_json::Value,
) {
    if let Some(entry) = base.insert(key, value) {
        displaced.push(entry);
    }
}

/// The `permissions` block, where every list unions and any other name overrides.
///
/// A rule is added by a layer and never removed by one, so `deny` still holds whatever the weakest
/// file said. `additionalDirectories` unions for the same reason it exists: a layer asks for
/// somewhere to work, and the strongest file asking for one place should not un-ask another.
///
/// `allow` unions here and is then replaced by [`Settings::layered`], which is the only caller that
/// knows which layer each entry came from. Unioning it and no more would let a checkout's file
/// answer a prompt, which [`grants`] is the rule against.
fn merge_permissions(
    displaced: &mut Vec<serde_json::Value>,
    under: &mut serde_json::Map<String, serde_json::Value>,
    above: serde_json::Map<String, serde_json::Value>,
) {
    for (key, value) in above {
        match (under.get_mut(&key), value) {
            (Some(serde_json::Value::Array(kept)), serde_json::Value::Array(added)) => {
                kept.extend(added);
            }
            // A list spelled as anything but an array adds no entry, so it removes none either. Only
            // the lists: a weaker file's `"defaultMode": ["plan"]` must not outrank a stronger mode.
            (Some(serde_json::Value::Array(_)), value)
                if matches!(
                    key.as_str(),
                    "deny" | "ask" | "allow" | "additionalDirectories"
                ) =>
            {
                displaced.push(value);
            }
            (_, value) => {
                lay_over(displaced, under, key, value);
            }
        }
    }
}

/// The `run` block, where `scrubEnv` unions and every other name overrides.
fn merge_run(
    displaced: &mut Vec<serde_json::Value>,
    under: &mut serde_json::Map<String, serde_json::Value>,
    above: serde_json::Map<String, serde_json::Value>,
) {
    for (key, value) in above {
        match (under.get_mut(&key), value) {
            (Some(serde_json::Value::Array(kept)), serde_json::Value::Array(added))
                if key == "scrubEnv" =>
            {
                kept.extend(added);
            }
            (Some(serde_json::Value::Array(_)), value) if key == "scrubEnv" => {
                displaced.push(value);
            }
            (_, value) => {
                lay_over(displaced, under, key, value);
            }
        }
    }
}

/// The `run.scrubEnv` array: names a file says to keep from a program the agent runs.
///
/// Strings only, and empty where the block is absent or shaped differently. A malformed entry is
/// dropped rather than refused, on the same footing as everything else here: a half-typed file must
/// not stop a session, and the built-in set still holds whatever this says.
fn scrub_list(root: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    let Some(serde_json::Value::Object(run)) = root.get("run") else {
        return Vec::new();
    };
    let Some(serde_json::Value::Array(names)) = run.get("scrubEnv") else {
        return Vec::new();
    };
    names
        .iter()
        .filter_map(|name| match name {
            serde_json::Value::String(name) if !name.trim().is_empty() => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// The `attribution` block: what a commit message and a pull request may carry.
///
/// The string exactly as written, empty ones included, because empty is the value that says to
/// carry nothing. Anything that is not a string is absence, on the same footing as everything else
/// here: a half-typed file leaves the layers under it in force rather than refusing to start.
fn attribution_block(root: &serde_json::Map<String, serde_json::Value>) -> Attribution {
    let Some(serde_json::Value::Object(block)) = root.get("attribution") else {
        return Attribution::default();
    };
    let text = |name: &str| match block.get(name) {
        Some(serde_json::Value::String(value)) => Some(value.clone()),
        _ => None,
    };
    Attribution {
        commit: text("commit"),
        pr: text("pr"),
    }
}

/// The `keybindings` block: an action by name, and the chord it is to answer.
///
/// Strings only, and an entry that is not one is dropped rather than refused, on the same footing
/// as the rest of this file. What the chord means is the interface's to decide, so a spelling
/// nothing can read is carried this far and left on its default there.
fn keybindings_block(
    root: &serde_json::Map<String, serde_json::Value>,
) -> BTreeMap<String, String> {
    let Some(serde_json::Value::Object(block)) = root.get("keybindings") else {
        return BTreeMap::new();
    };
    block
        .iter()
        .filter_map(|(action, chord)| Some((action, chord.as_str()?)))
        .map(|(action, chord)| (action.trim().to_ascii_lowercase(), chord.trim().to_string()))
        .filter(|(action, chord)| !action.is_empty() && !chord.is_empty())
        .collect()
}

/// The `search` block: how many files a search may walk, and how long it may spend reading them.
///
/// Numbers rather than the strings the rest of this file reads, because a cap is a quantity and
/// there is no spelling of one worth carrying through unrecognised. Whole and positive: anything
/// else is absence, on the same footing as everything else here, so a half-typed file leaves the
/// built-in cap in force rather than refusing to start.
///
/// Zero is absence too. It is the number somebody writes meaning "no cap", and read literally it
/// is a search permitted to open no file at all, which answers every pattern with nothing found.
fn search_caps(root: &serde_json::Map<String, serde_json::Value>) -> SearchCaps {
    let Some(serde_json::Value::Object(block)) = root.get("search") else {
        return SearchCaps::default();
    };
    let count = |name: &str| {
        block
            .get(name)
            .and_then(serde_json::Value::as_u64)
            .filter(|cap| *cap > 0)
    };
    SearchCaps {
        files: count("maxFiles").and_then(|files| usize::try_from(files).ok()),
        time: count("maxSeconds").map(Duration::from_secs),
    }
}

/// The `run.maxOutput` value: how much of what a program printed may enter the conversation.
///
/// Read the way a search cap is, and absent on the same terms: zero is the number somebody writes
/// meaning "no cap", and read literally it is a program permitted to say nothing into the
/// conversation, which answers every command with a sample of no bytes. A value that is not a whole
/// count is absence too, so a half-typed file leaves the built-in cap in force rather than stopping
/// a session.
fn run_output_cap(root: &serde_json::Map<String, serde_json::Value>) -> Option<usize> {
    let serde_json::Value::Object(run) = root.get("run")? else {
        return None;
    };
    run.get("maxOutput")
        .and_then(serde_json::Value::as_u64)
        .filter(|cap| *cap > 0)
        .and_then(|cap| usize::try_from(cap).ok())
}

/// The `run.defaultSeconds` and `run.maxSeconds` figures: how long a command may run.
///
/// Read the way a search cap is, and absent on the same terms: zero is the number somebody writes
/// meaning "no limit", and read literally it is a command given no time to run at all, which is
/// every command stopped before it has printed anything. A value that is not a whole count of
/// seconds is absence too, so a half-typed file leaves the built-in figures in force rather than
/// stopping a session.
///
/// Each is read on its own. A file raising the ceiling says nothing about the default, which is what
/// lets somebody allow one long run without giving up on noticing a program that hangs.
fn run_deadlines(root: &serde_json::Map<String, serde_json::Value>) -> RunDeadlines {
    let Some(serde_json::Value::Object(run)) = root.get("run") else {
        return RunDeadlines::default();
    };
    let seconds = |name: &str| {
        run.get(name)
            .and_then(serde_json::Value::as_u64)
            .filter(|count| *count > 0)
            .map(Duration::from_secs)
    };
    RunDeadlines {
        default: seconds("defaultSeconds"),
        ceiling: seconds("maxSeconds"),
    }
}

/// The `permissions` block: three lists of rule text, and the directories to open.
///
/// A malformed entry is dropped rather than refused, on the same footing as everything else here:
/// refusing the file over one would take away the rules that were readable. Dropped from the list
/// and not from the block, though: an entry that is not text lands in
/// [`PermissionLists::unreadable`], so that whoever reads the rules can name it (PERM-11).
///
/// `defaultMode` is read by nothing yet. A file setting it is not an error and not a warning here:
/// [`Settings::parse`] reads what the file says and reports it, and which modes exist is a
/// question for whoever consults them.
///
/// The two keys of the block that refuse without naming what they refuse are read by
/// [`narrowing_stated`] instead, and are not rules: neither names a family, a path or a host, so
/// neither is a line this hands to the rule language. See [`Narrowing`].
fn permission_lists(root: &serde_json::Map<String, serde_json::Value>) -> PermissionLists {
    let Some(serde_json::Value::Object(block)) = root.get(PERMISSIONS_BLOCK) else {
        return PermissionLists::default();
    };
    let mut unreadable = Vec::new();
    let deny = rule_texts(block, "deny", &mut unreadable);
    let ask = rule_texts(block, "ask", &mut unreadable);
    let allow = rule_texts(block, "allow", &mut unreadable);
    PermissionLists {
        deny,
        ask,
        allow,
        // Strings only, and silently: a directory is not a rule, and a name that is not text is
        // nothing to put to a person as a directory to open. PERM-10 answers for this key.
        additional_directories: strings(block, "additionalDirectories"),
        unreadable,
    }
}

/// One layer's `permissions` block or rule list that is not the shape rules are read from, spelled
/// with the names that lead to it so a person can find it in the file (PERM-11).
///
/// Written out from the parsed value rather than through a new one, which would be a copy of it that
/// is dropped uncleared (CRED-23).
fn misshapen_rule_lists(root: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    let block = match root.get("permissions") {
        None => return Vec::new(),
        Some(serde_json::Value::Object(block)) => block,
        Some(other) => return vec![format!(r#"{{"permissions":{other}}}"#)],
    };
    ["deny", "ask", "allow"]
        .into_iter()
        .filter_map(|name| match block.get(name) {
            None | Some(serde_json::Value::Array(_)) => None,
            Some(other) => Some(format!(r#"{{"permissions":{{"{name}":{other}}}}}"#)),
        })
        .collect()
}

/// One array of rule text out of a block, with every entry that is not text put in `unreadable`.
///
/// Blank text is carried rather than filtered, because the rule language already has a word for
/// it: `Rule::parse` calls an empty rule empty, and a filter here is what stopped it ever being
/// asked. Nothing that reaches a list here is a rule yet, since which of them is one is the
/// kernel's question, and this drops only what could not be put to it.
fn rule_texts(
    block: &serde_json::Map<String, serde_json::Value>,
    name: &str,
    unreadable: &mut Vec<String>,
) -> Vec<String> {
    let Some(serde_json::Value::Array(entries)) = block.get(name) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| match entry {
            serde_json::Value::String(text) => Some(text.clone()),
            other => {
                unreadable.push(other.to_string());
                None
            }
        })
        .collect()
}

/// One array of non-empty strings out of a block, or empty for every other shape.
fn strings(block: &serde_json::Map<String, serde_json::Value>, name: &str) -> Vec<String> {
    let Some(serde_json::Value::Array(entries)) = block.get(name) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| match entry {
            serde_json::Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// The variables the platform states the user's profile directory in, in the order they answer.
///
/// Spelled here as well as in the crates above this one, because `docs/specs/layering.md` forbids
/// this one the dependency on the crate that holds the answer. What has to hold across the copies is
/// the name of the directory, the variables, and the refusal to invent one.
///
/// `HOME` on either platform: it is the one Unix sets, and a Windows shell environment that sets one
/// has been told where the profile is. `USERPROFILE` is the one stock Windows sets, and is read there
/// only, since on Unix it is not a name the platform states anything in.
#[cfg(windows)]
pub(crate) const PROFILE_VARIABLES: &[&str] = &["HOME", "USERPROFILE"];
#[cfg(not(windows))]
pub(crate) const PROFILE_VARIABLES: &[&str] = &["HOME"];

/// The global state directory, or `None` when the platform names no profile directory to look in.
///
/// No fallback to a relative `.bravebot`, which is the project layer and reached deliberately rather
/// than by a home directory going missing. Resolving the weakest layer to the strongest one's
/// location would silently read a checkout's file as though a person had put it in their own
/// directory.
pub(crate) fn home() -> Option<PathBuf> {
    home_named(PROFILE_VARIABLES.iter().map(std::env::var_os))
}

/// The same answer, from the values rather than from the variables.
///
/// Split from the read so the rule is testable without a process-wide variable. A test that set
/// `HOME` would have to take a lock against every other test in this binary, restore what was
/// there, and step outside safe Rust to do it, all to check a rule that is a function of a couple
/// of strings.
///
/// The values arrive in the order [`PROFILE_VARIABLES`] names them, and the first that names
/// something answers. An empty one names nothing: joining onto it would resolve the user's own
/// settings to `/.bravebot`, and stopping there would lose a profile directory the platform does
/// name to a variable some shell exported empty.
fn home_named(named: impl IntoIterator<Item = Option<std::ffi::OsString>>) -> Option<PathBuf> {
    let home = named
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())?;
    Some(PathBuf::from(home).join(".bravebot"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A home as the environment hands one over.
    fn named(home: &str) -> Option<std::ffi::OsString> {
        Some(std::ffi::OsString::from(home))
    }

    /// A gateway block as a file states one, for the token in it.
    fn gateway(token: &str) -> serde_json::Value {
        serde_json::json!({"options": {"baseURL": "https://example.invalid/v1", "apiKey": token}})
    }

    /// One layer as a file states it, for a merge to lay over another.
    fn layer(stated: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        match stated {
            serde_json::Value::Object(root) => root,
            _ => panic!("a layer is a document"),
        }
    }

    /// CRED-23: the buffer a settings file is read into holds the token the file states, and it is a
    /// buffer this program owns.
    ///
    /// Taking the value out of the parse leaves this one behind, so a crash writes the token from
    /// here even once every copy downstream of it is in a `Secret`.
    ///
    /// A file that does not parse is the second case, and the one the ordering in
    /// [`Document::parse`] is for: a half-typed settings file is an ordinary thing, and the token in
    /// one is in the text whatever the parse made of it.
    #[test]
    fn the_text_a_layer_was_parsed_from_is_cleared() {
        let token = "sk-live-0123456789";
        for (text, is_a_document) in [
            (
                serde_json::json!({"provider": {"gw": gateway(token)}}).to_string(),
                true,
            ),
            (
                format!("{{\"provider\": {{\"gw\": {{\"apiKey\": \"{token}\""),
                false,
            ),
        ] {
            let mut text = text;
            let length = text.len();

            let parsed = Document::parse(&mut text);

            assert_eq!(
                parsed.is_ok(),
                is_a_document,
                "the fixture parsed the other way, so it says nothing about clearing"
            );
            assert_eq!(
                text.as_bytes(),
                vec![0u8; length],
                "the file's bytes are still in the buffer it was read into"
            );
        }
    }

    /// CRED-23: two layers may state the same gateway, and the entry the stronger one replaces is
    /// the last place the weaker one's token is. Dropping it where it is displaced puts it beyond
    /// the clearing the document does when it goes, so the merge keeps it instead.
    ///
    /// Through [`merge`] rather than the helper it calls, because which arm handles a name is what
    /// decides whether the entry is kept. Every case displaces: a project file restating a gateway a
    /// home file stated, one setting `provider` to something that is not a block and so replacing
    /// every gateway at once, and one spelling `permissions`, `permissions.deny`, `run` or
    /// `run.scrubEnv` as another shape, which is set aside while the weaker block or list stays.
    #[test]
    fn a_merge_keeps_the_entry_a_stronger_layer_displaced() {
        let token = "sk-home-0123456789";
        let home = serde_json::json!({
            "provider": {"gw": gateway(token)},
            "permissions": {"deny": ["Read(./.env)"]},
            "run": {"scrubEnv": ["MY_TOKEN"]}
        });
        for (above, displaced) in [
            (
                serde_json::json!({"provider": {"gw": gateway("sk-project-0123456789")}}),
                vec![gateway(token)],
            ),
            (
                serde_json::json!({"provider": "off"}),
                vec![serde_json::json!({"gw": gateway(token)})],
            ),
            (
                serde_json::json!({"permissions": "everything"}),
                vec![serde_json::json!("everything")],
            ),
            (
                serde_json::json!({"permissions": {"deny": "everything"}}),
                vec![serde_json::json!("everything")],
            ),
            (
                serde_json::json!({"run": "all of them"}),
                vec![serde_json::json!("all of them")],
            ),
            (
                serde_json::json!({"run": {"scrubEnv": "all of them"}}),
                vec![serde_json::json!("all of them")],
            ),
        ] {
            let mut merged = Document::default();
            merge(&mut merged, layer(home.clone()));
            assert!(
                merged.displaced.is_empty(),
                "the weakest layer displaced something, so the fixture proves nothing"
            );

            merge(&mut merged, layer(above.clone()));

            assert_eq!(
                merged.displaced, displaced,
                "{above} replaced an entry and left nothing holding it"
            );
        }
    }

    /// The front end tells somebody which way the file they chose is wrong, so the answers it maps
    /// to its three messages have to be distinguishable. It asks here rather than parsing the file
    /// itself, which is CRED-23: a parse it made would be a copy of a token it then dropped.
    #[test]
    fn a_chosen_file_is_answered_for_each_way_it_is_not_settings() {
        let dir = crate::testutil::scratch_dir("settings-chosen-file");
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        for (name, text, answer) in [
            ("settings.json", r#"{"model": "opus"}"#, Ok(())),
            ("half-typed.json", "{not json", Err(NotADocument::NotJson)),
            ("a-list.json", "[]", Err(NotADocument::NotAnObject)),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, text).expect("a file to check");
            assert_eq!(check_document(&path), answer, "{text}");
        }
        assert_eq!(
            check_document(&dir.join("nothing-here.json")),
            Err(NotADocument::Unreadable),
            "a file that is not there is not a file that is not JSON"
        );
    }

    /// STATE-2: this crate resolves the state directory itself, since it sits below the one that
    /// answers where it is. What has to hold across the resolvers is the name, so a layer reading
    /// a settings file finds the same directory a layer writing history does.
    #[test]
    fn the_state_directory_is_the_home_the_environment_names() {
        assert_eq!(
            home_named([named("/somebody/else")]),
            Some(PathBuf::from("/somebody/else/.bravebot"))
        );
    }

    /// STATE-2: the other half of the same rule. A fallback here would read settings out of a
    /// directory nobody chose, and settings are what decide which model answers and which
    /// commands run without being asked about.
    #[test]
    fn an_absent_or_empty_home_yields_no_directory_rather_than_a_guess() {
        assert_eq!(home_named([None]), None);
        assert_eq!(
            home_named([named("")]),
            None,
            "an empty home was joined onto anyway"
        );
    }

    /// STATE-2: stock Windows sets no `HOME`, so the settings a person keeps outside a checkout are
    /// read from the profile directory the platform does name, or from nowhere at all. The order is
    /// the same in every resolver, so the layer reading settings and the layer writing history agree
    /// about which variable won.
    #[test]
    fn the_profile_directory_answers_where_no_home_is_named() {
        // In the order `PROFILE_VARIABLES` names them: no `HOME`, then the profile directory stock
        // Windows names in `USERPROFILE`.
        assert_eq!(
            home_named([None, named("C:\\Users\\someone")]),
            Some(Path::new("C:\\Users\\someone").join(".bravebot"))
        );
        assert_eq!(
            home_named([named(""), named("C:\\Users\\someone")]),
            Some(Path::new("C:\\Users\\someone").join(".bravebot")),
            "a variable exported empty took away a profile directory the platform names"
        );
    }

    /// STATE-2: a shell environment that sets `HOME` has been told where the profile is, and every
    /// other tool run from it reads that. Settings read from somewhere else would be a file the
    /// person cannot find from the shell they configured.
    #[test]
    fn a_named_home_answers_before_the_profile_directory() {
        assert_eq!(
            home_named([named("/somebody"), named("C:\\Users\\someone")]),
            Some(PathBuf::from("/somebody/.bravebot"))
        );
    }

    /// The point of the file: a block copied from `~/.claude/settings.json` configures this agent
    /// without being rewritten first.
    #[test]
    fn an_env_block_is_read() {
        let settings = Settings::parse(
            r#"{"env": {"AWS_REGION": "us-west-2", "AWS_PROFILE": "some-profile"}}"#,
        );
        assert_eq!(settings.get("AWS_REGION"), Some("us-west-2"));
        assert_eq!(settings.get("AWS_PROFILE"), Some("some-profile"));
    }

    /// CRED-23: a signing key is one of the names a person may write into the `env` block, so that
    /// block is a buffer this program owns a credential in.
    ///
    /// The `Secret` the configuration builds out of the name clears itself, and this map is a second
    /// buffer holding the same bytes for as long as the settings live. Both halves of the assertion
    /// are needed and neither says it alone: a buffer of zeros at a fresh address leaves the value
    /// where the allocator can hand it on, and a buffer that has not moved and still reads as the
    /// value is what dropping the string produces.
    ///
    /// Every value rather than the credential name alone, because the same block carries a region
    /// and a model name and what a person may put there is anything.
    #[test]
    fn what_the_env_block_was_set_to_is_overwritten_where_it_lies() {
        let key = "sk-live-0123456789abcdef";
        let mut settings = Settings::parse(
            &serde_json::json!({
                "env": {crate::env_var::SIGNING_KEY: key, "AWS_REGION": "us-west-2"}
            })
            .to_string(),
        );
        let addresses: Vec<(String, *const u8, usize)> = settings
            .env
            .iter()
            .map(|(name, value)| (name.clone(), value.as_ptr(), value.len()))
            .collect();
        assert_eq!(addresses.len(), 2, "the fixture set two names");

        settings.scrub_env();

        for (name, address, length) in addresses {
            let value = settings.get(&name).expect("the name is still in the block");
            assert_eq!(
                value.as_ptr(),
                address,
                "{name}'s buffer moved, so its value is still in the one left behind"
            );
            assert_eq!(
                value.as_bytes(),
                vec![0u8; length],
                "the buffer {name} was in still holds bytes of its value"
            );
        }
    }

    /// Every name is read rather than a chosen subset. The file is the user's own configuration
    /// surface, and a settings file that silently drops what it was told is worse than one that
    /// reads a name nothing happens to consult.
    #[test]
    fn a_name_this_crate_does_not_know_is_still_read() {
        let settings = Settings::parse(r#"{"env": {"SOMETHING_ELSE": "value"}}"#);
        assert_eq!(settings.get("SOMETHING_ELSE"), Some("value"));
    }

    /// Settings files carry other blocks. One this crate does not read must not stop it finding
    /// the ones it does.
    #[test]
    fn other_blocks_are_ignored() {
        let settings = Settings::parse(
            r#"{"permissions": {"allow": []}, "env": {"AWS_REGION": "us-west-2"}}"#,
        );
        assert_eq!(settings.get("AWS_REGION"), Some("us-west-2"));
    }

    /// The key Claude Code and opencode both use for this. Read because a file written for either
    /// of them is the file people have, and a key those tools honour that this one dropped looks
    /// like the setting doing nothing.
    #[test]
    fn a_top_level_model_key_is_read() {
        let settings = Settings::parse(r#"{"model": "opus"}"#);
        assert_eq!(settings.model(), Some("opus"));
    }

    /// The two are independent. A `model` key is the whole of some people's settings, and
    /// requiring an `env` block beside it would discard it for being alone.
    #[test]
    fn a_model_and_an_env_block_are_read_from_the_same_file() {
        let settings = Settings::parse(r#"{"model": "opus", "env": {"AWS_REGION": "us-west-2"}}"#);
        assert_eq!(settings.model(), Some("opus"));
        assert_eq!(settings.get("AWS_REGION"), Some("us-west-2"));
    }

    /// A blank is absence. Sending `""` as a model would be reset by the server anyway, and here it
    /// would shadow the value a release has built in for no stated reason.
    #[test]
    fn a_model_that_is_blank_or_not_a_string_names_nothing() {
        for text in [
            r#"{"model": ""}"#,
            r#"{"model": "   "}"#,
            r#"{"model": 1}"#,
            r#"{"model": true}"#,
            r#"{"model": null}"#,
            r#"{"model": ["opus"]}"#,
            r#"{"model": {"name": "opus"}}"#,
        ] {
            assert_eq!(Settings::parse(text).model(), None, "{text} named a model");
        }
    }

    /// Surrounding space is the shape a hand-edited file has, and a model name with a newline in it
    /// is not one either backend serves.
    #[test]
    fn a_model_is_trimmed() {
        assert_eq!(
            Settings::parse("{\"model\": \" opus\\n\"}").model(),
            Some("opus")
        );
    }

    /// A file that sets only a model has set something, so `doctor` must not report it as absent.
    #[test]
    fn a_file_with_only_a_model_is_not_empty() {
        let settings = Settings::parse(r#"{"model": "opus"}"#);
        assert!(!settings.is_empty());
        assert_eq!(settings.names().collect::<Vec<_>>(), ["model"]);
    }

    /// The whole point of the key (BACKEND-43): a machine where nobody ever opens the interface has
    /// no other route to a level, and two checkouts cannot ask for different ones through a record
    /// stored once per person.
    #[test]
    fn a_top_level_effort_key_is_read() {
        let settings = Settings::parse(r#"{"effort": "high"}"#);
        assert_eq!(settings.effort(), Some("high"));
        assert!(!settings.is_empty());
        assert_eq!(settings.names().collect::<Vec<_>>(), ["effort"]);
    }

    /// The word as written, because which words name a level is not this crate's question: a name it
    /// does not recognise has to reach the place the level is settled to be dropped by the one rule
    /// that drops an unrecognised recorded word too.
    #[test]
    fn an_effort_word_is_read_as_the_file_spelled_it() {
        assert_eq!(
            Settings::parse(r#"{"effort": "HIGHEST"}"#).effort(),
            Some("HIGHEST")
        );
    }

    /// A blank is how a line is commented out everywhere else here, and surrounding space is the
    /// shape a hand-edited file has. Every other shape is absence on the footing the `model` key's
    /// is, which leaves whatever was recorded in force rather than stopping a session.
    #[test]
    fn an_effort_that_is_blank_or_not_a_string_names_nothing() {
        for text in [
            r#"{"effort": ""}"#,
            r#"{"effort": "   "}"#,
            r#"{"effort": 1}"#,
            r#"{"effort": true}"#,
            r#"{"effort": null}"#,
            r#"{"effort": ["high"]}"#,
            r#"{"effort": {"level": "high"}}"#,
        ] {
            assert_eq!(Settings::parse(text).effort(), None, "{text} named a level");
        }
        assert_eq!(
            Settings::parse("{\"effort\": \" high\\n\"}").effort(),
            Some("high")
        );
    }

    /// A variable is a string. Coercing a number or a boolean would invent a spelling the writer
    /// did not choose, and `1` is not obviously `"1"` to whoever debugs it later.
    #[test]
    fn a_value_that_is_not_a_string_is_left_out() {
        let settings = Settings::parse(r#"{"env": {"A": 1, "B": true, "C": null, "D": "yes"}}"#);
        assert_eq!(settings.get("A"), None);
        assert_eq!(settings.get("B"), None);
        assert_eq!(settings.get("C"), None);
        assert_eq!(settings.get("D"), Some("yes"));
    }

    /// Every failure reads as absence, because the process environment and the built-in values
    /// still describe a working backend. A half-typed settings file must not stop a session.
    #[test]
    fn anything_unparseable_reads_as_no_settings_at_all() {
        for text in [
            "",
            "   ",
            "not json",
            "{",
            "[]",
            "null",
            r#"{"env": "not a block"}"#,
            r#"{"env": []}"#,
            r#"{"no_env_here": {"AWS_REGION": "us-west-2"}}"#,
        ] {
            assert!(
                Settings::parse(text).is_empty(),
                "{text:?} was read as settings"
            );
        }
    }

    /// A machine with no home directory has no settings file, and that is not an error.
    #[test]
    fn no_home_directory_is_not_an_error() {
        assert!(Settings::from_home(None).is_empty());
    }

    /// Nothing is there to read on a fresh machine, which is the common case and must be quiet.
    #[test]
    fn a_missing_file_is_not_an_error() {
        let missing = crate::testutil::scratch_dir("bravebot-settings-absent");
        assert!(Settings::from_home(Some(missing)).is_empty());
    }

    /// The escape hatch for a setup that needs a variable the built-in set removes: a person names
    /// their own, and those are kept from a program the agent runs too.
    #[test]
    fn a_file_may_name_variables_to_keep_from_a_program() {
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["MY_TOKEN", "OTHER_SECRET"]}}"#);
        let named: Vec<&str> = settings.scrubbed().collect();
        assert_eq!(named, ["MY_TOKEN", "OTHER_SECRET"]);
    }

    /// The two blocks are independent. A file that only says what to withhold from a subprocess
    /// configures no backend, and reading it must not depend on an `env` block being present.
    #[test]
    fn a_scrub_list_is_read_without_an_env_block() {
        let settings = Settings::parse(r#"{"run": {"scrubEnv": ["MY_TOKEN"]}}"#);
        assert_eq!(settings.get("AWS_REGION"), None);
        assert_eq!(settings.scrubbed().collect::<Vec<_>>(), ["MY_TOKEN"]);
        assert!(!settings.is_empty());
    }

    /// A name is a string, and an entry that is not one is dropped rather than refusing the file:
    /// the built-in set still holds whatever else the block says.
    #[test]
    fn a_scrub_entry_that_is_not_a_name_is_left_out() {
        let settings =
            Settings::parse(r#"{"run": {"scrubEnv": ["KEEP", 1, true, null, "", "  ", "ALSO"]}}"#);
        assert_eq!(settings.scrubbed().collect::<Vec<_>>(), ["KEEP", "ALSO"]);
    }

    /// Every shape that is not a list of names reads as an empty list, on the same footing as the
    /// rest of this file: a half-typed settings file must not stop a session.
    #[test]
    fn a_malformed_scrub_block_names_nothing() {
        for text in [
            r#"{"run": {}}"#,
            r#"{"run": {"scrubEnv": {}}}"#,
            r#"{"run": {"scrubEnv": "MY_TOKEN"}}"#,
            r#"{"run": "not a block"}"#,
            r#"{"run": []}"#,
            r#"{"scrubEnv": ["MY_TOKEN"]}"#,
        ] {
            assert_eq!(
                Settings::parse(text).scrubbed().count(),
                0,
                "{text:?} named something"
            );
        }
    }

    /// A tree where the built-in caps are the wrong numbers is the only thing that can say so, so
    /// the block has to reach the code that walks it: without it there is no way to search a
    /// repository larger than the default walks.
    #[test]
    fn a_file_may_cap_a_search_of_a_large_tree() {
        let settings = Settings::parse(r#"{"search": {"maxFiles": 500000, "maxSeconds": 60}}"#);
        assert_eq!(settings.search().files, Some(500_000));
        assert_eq!(settings.search().time, Some(Duration::from_secs(60)));
        assert!(!settings.is_empty());
        assert_eq!(
            settings.names().collect::<Vec<_>>(),
            ["search.maxFiles", "search.maxSeconds"]
        );
    }

    /// The two caps bound different things, so a file raising the walk says nothing about how long
    /// a read may take: one named alone leaves the other on its built-in number.
    #[test]
    fn one_search_cap_is_read_without_the_other() {
        let files = Settings::parse(r#"{"search": {"maxFiles": 400000}}"#);
        assert_eq!(files.search().files, Some(400_000));
        assert_eq!(files.search().time, None);

        let time = Settings::parse(r#"{"search": {"maxSeconds": 45}}"#);
        assert_eq!(time.search().files, None);
        assert_eq!(time.search().time, Some(Duration::from_secs(45)));
    }

    /// Zero is what somebody writes meaning "no cap", and honoured literally it is a search
    /// permitted to open no file at all: every pattern would come back absent from a tree that
    /// holds it. Absence leaves the built-in cap in force instead.
    #[test]
    fn a_search_cap_of_zero_leaves_the_built_in_one_in_force() {
        let settings = Settings::parse(r#"{"search": {"maxFiles": 0, "maxSeconds": 0}}"#);
        assert!(settings.search().is_empty());
        assert!(settings.is_empty());
    }

    /// Every other shape is absence, on the same footing as the rest of this file: a half-typed
    /// settings file leaves the built-in cap in force rather than stopping a session.
    #[test]
    fn a_search_cap_that_is_not_a_whole_count_is_absence() {
        for text in [
            r#"{"search": {"maxFiles": "500000"}}"#,
            r#"{"search": {"maxFiles": 500000.5, "maxSeconds": 1.5}}"#,
            r#"{"search": {"maxFiles": -1, "maxSeconds": -1}}"#,
            r#"{"search": {"maxFiles": true, "maxSeconds": null}}"#,
            r#"{"search": {"maxFiles": [500000]}}"#,
            r#"{"search": "wide"}"#,
            r#"{"maxFiles": 500000}"#,
        ] {
            assert!(
                Settings::parse(text).search().is_empty(),
                "{text:?} capped something"
            );
        }
    }

    /// A build log worth reading is worth more than the built-in cap on some trees, and the number
    /// cannot be changed from anywhere else, so the block has to reach the turn that spends the
    /// context (RUN-21).
    #[test]
    fn a_settings_file_names_what_a_commands_output_may_spend() {
        let settings = Settings::parse(r#"{"run": {"maxOutput": 65536}}"#);
        assert_eq!(settings.run_output_cap(), Some(65_536));
        assert!(!settings.is_empty());
        assert_eq!(settings.names().collect::<Vec<_>>(), ["run.maxOutput"]);
    }

    /// Zero is the number somebody writes meaning "no cap", and read literally it is a command
    /// permitted to say nothing into the conversation. Every other shape is absence for the reason
    /// a search cap's is: a half-typed file leaves the built-in cap in force rather than stopping a
    /// session.
    #[test]
    fn a_cap_of_zero_or_of_nonsense_leaves_the_built_in_one() {
        for text in [
            r#"{"run": {"maxOutput": 0}}"#,
            r#"{"run": {"maxOutput": "65536"}}"#,
            r#"{"run": {"maxOutput": 65536.5}}"#,
            r#"{"run": {"maxOutput": -1}}"#,
            r#"{"run": {"maxOutput": true}}"#,
            r#"{"run": {"maxOutput": null}}"#,
            r#"{"run": {"maxOutput": [65536]}}"#,
            r#"{"run": "wide"}"#,
            r#"{"maxOutput": 65536}"#,
        ] {
            assert_eq!(
                Settings::parse(text).run_output_cap(),
                None,
                "{text:?} capped something"
            );
        }
    }

    /// The cap is one number rather than a list, so the nearest layer that named one wins and a
    /// project does not inherit a figure somebody set for everything else they do. `run.scrubEnv`
    /// beside it unions, and this must not be swept up in that.
    #[test]
    fn the_nearest_layer_that_named_an_output_cap_wins() {
        let settings = Layers::new("output-cap-layers")
            .global(r#"{"run": {"maxOutput": 32768, "scrubEnv": ["MY_TOKEN"]}}"#)
            .project(r#"{"run": {"maxOutput": 65536}}"#)
            .read();
        assert_eq!(settings.run_output_cap(), Some(65_536));
        assert_eq!(
            settings.scrubbed().collect::<Vec<_>>(),
            ["MY_TOKEN"],
            "the union beside the cap was lost"
        );

        let only_global = Layers::new("output-cap-global")
            .global(r#"{"run": {"maxOutput": 32768}}"#)
            .project(r#"{"env": {"AWS_PROFILE": "this-checkout"}}"#)
            .read();
        assert_eq!(
            only_global.run_output_cap(),
            Some(32_768),
            "a project file that said nothing about the cap dropped it"
        );

        // A zero in the nearer layer is absence, and absence is the built-in cap rather than the
        // figure a weaker layer named.
        let zeroed = Layers::new("output-cap-zeroed")
            .global(r#"{"run": {"maxOutput": 32768}}"#)
            .project(r#"{"run": {"maxOutput": 0}}"#)
            .read();
        assert_eq!(
            zeroed.run_output_cap(),
            None,
            "a project file that set the cap to zero was handed the home layer's figure"
        );
    }

    /// A build a person runs every day outlasts the built-in default on some trees, and the figure
    /// cannot be changed from anywhere else, so both have to reach the crate that waits on the
    /// program (RUN-23).
    #[test]
    fn a_settings_file_names_how_long_a_command_may_run() {
        let settings = Settings::parse(r#"{"run": {"defaultSeconds": 900, "maxSeconds": 1800}}"#);
        assert_eq!(
            settings.run_deadlines(),
            RunDeadlines {
                default: Some(Duration::from_secs(900)),
                ceiling: Some(Duration::from_secs(1800)),
            }
        );
        assert!(!settings.is_empty());
        assert_eq!(
            settings.names().collect::<Vec<_>>(),
            ["run.defaultSeconds", "run.maxSeconds"]
        );
    }

    /// The two are separate decisions, so a file naming one must say nothing about the other:
    /// somebody allowing one twenty-minute integration run wants a program that hangs given up on
    /// in five minutes all the same.
    #[test]
    fn one_run_deadline_is_read_without_the_other() {
        assert_eq!(
            Settings::parse(r#"{"run": {"maxSeconds": 1800}}"#).run_deadlines(),
            RunDeadlines {
                default: None,
                ceiling: Some(Duration::from_secs(1800)),
            }
        );
        assert_eq!(
            Settings::parse(r#"{"run": {"defaultSeconds": 900}}"#).run_deadlines(),
            RunDeadlines {
                default: Some(Duration::from_secs(900)),
                ceiling: None,
            }
        );
    }

    /// Zero is the number somebody writes meaning "no limit", and read literally it is a command
    /// given no time to run at all. Every other shape is absence for the reason a search cap's is: a
    /// half-typed file leaves the built-in figures in force rather than stopping a session.
    #[test]
    fn a_run_deadline_of_zero_or_of_nonsense_leaves_the_built_in_ones() {
        for text in [
            r#"{"run": {"defaultSeconds": 0, "maxSeconds": 0}}"#,
            r#"{"run": {"defaultSeconds": "900", "maxSeconds": "1800"}}"#,
            r#"{"run": {"defaultSeconds": 900.5, "maxSeconds": 1800.5}}"#,
            r#"{"run": {"defaultSeconds": -1, "maxSeconds": -1}}"#,
            r#"{"run": {"defaultSeconds": true, "maxSeconds": true}}"#,
            r#"{"run": {"defaultSeconds": null, "maxSeconds": null}}"#,
            r#"{"run": {"defaultSeconds": [900], "maxSeconds": [1800]}}"#,
            r#"{"run": "slow"}"#,
            r#"{"defaultSeconds": 900, "maxSeconds": 1800}"#,
        ] {
            assert_eq!(
                Settings::parse(text).run_deadlines(),
                RunDeadlines::default(),
                "{text:?} named a figure"
            );
        }
    }

    /// Each figure is one number rather than a list, so the nearest layer that named one wins and a
    /// checkout does not inherit the time somebody allowed for a different tree. The two are
    /// independent inside that, a file raising the ceiling saying nothing about the default.
    #[test]
    fn the_nearest_layer_that_named_a_run_deadline_wins() {
        let settings = Layers::new("run-deadline-layers")
            .global(r#"{"run": {"defaultSeconds": 450, "maxSeconds": 900}}"#)
            .project(r#"{"run": {"defaultSeconds": 900}}"#)
            .read();
        assert_eq!(
            settings.run_deadlines(),
            RunDeadlines {
                default: Some(Duration::from_secs(900)),
                ceiling: Some(Duration::from_secs(900)),
            },
            "a project file that raised the default lost the ceiling beside it"
        );

        // A zero in the nearer layer is absence, and absence is the built-in figure rather than the
        // one a weaker layer named.
        let zeroed = Layers::new("run-deadline-zeroed")
            .global(r#"{"run": {"defaultSeconds": 450}}"#)
            .project(r#"{"run": {"defaultSeconds": 0}}"#)
            .read();
        assert_eq!(
            zeroed.run_deadlines(),
            RunDeadlines::default(),
            "a project file that set the default to zero was handed the home layer's figure"
        );
    }

    /// The block a person copies out of `~/.claude/settings.json`, read without being rewritten
    /// first, which is the whole reason this file has the shape it has.
    #[test]
    fn a_permissions_block_is_read() {
        let settings = Settings::parse(
            r#"{
              "permissions": {
                "defaultMode": "acceptEdits",
                "allow": ["Bash(git diff *)", "Bash(npm test *)"],
                "ask": ["Bash(git push *)"],
                "deny": ["Read(./.env)", "Read(./.env.*)"],
                "additionalDirectories": ["../shared"]
              }
            }"#,
        );
        let permissions = settings.permissions();
        assert_eq!(permissions.allow, ["Bash(git diff *)", "Bash(npm test *)"]);
        assert_eq!(permissions.ask, ["Bash(git push *)"]);
        assert_eq!(permissions.deny, ["Read(./.env)", "Read(./.env.*)"]);
        assert_eq!(permissions.additional_directories, ["../shared"]);
        assert!(!settings.is_empty());
    }

    /// Each list stands alone. A file that only refuses things configures no backend and grants
    /// nothing, and reading it must not depend on the other lists being there.
    #[test]
    fn one_list_is_read_without_the_others() {
        let settings = Settings::parse(r#"{"permissions": {"deny": ["Read(./.env)"]}}"#);
        let permissions = settings.permissions();
        assert_eq!(permissions.deny, ["Read(./.env)"]);
        assert!(permissions.allow.is_empty());
        assert!(permissions.ask.is_empty());
        assert!(!settings.is_empty());
    }

    /// A rule is a line. An entry that is not one cannot be matched against anything, and dropping
    /// it keeps the rules that were readable rather than losing the file over one. It is dropped
    /// from the list and not from the block, though, because the rule nobody can act on is exactly
    /// the one PERM-11 has named where a person reads it.
    ///
    /// Blank text is carried on into the list instead. It is a line, and what an empty rule is
    /// belongs to the language rather than to the reader.
    #[test]
    fn an_entry_that_is_not_a_rule_is_carried_out_to_be_reported() {
        let settings = Settings::parse(
            r#"{"permissions":
                 {"deny": ["Read(./.env)", 1, true, null, "", "  ", ["Read(./.env)"]]}}"#,
        );
        let permissions = settings.permissions();
        assert_eq!(permissions.deny, ["Read(./.env)", "", "  "]);
        assert_eq!(
            permissions.unreadable,
            ["1", "true", "null", r#"["Read(./.env)"]"#]
        );
    }

    /// One report for the three rule lists, since one is what `doctor` and a session print. A
    /// directory is not a rule and stays out of it: an entry that is not text is nothing to put to
    /// a person as a directory to open, and PERM-10 rather than PERM-11 answers for that key.
    ///
    /// A file whose every rule is mistyped still said something. Reading it as an empty block
    /// would have `doctor` report no settings one line above the rules it could not read.
    #[test]
    fn an_unreadable_entry_is_carried_out_of_whichever_rule_list_held_it() {
        let settings = Settings::parse(
            r#"{"permissions": {"deny": [1], "ask": [2], "allow": [3],
                                "additionalDirectories": [4]}}"#,
        );
        assert_eq!(settings.permissions().unreadable, ["1", "2", "3"]);
        assert!(!settings.permissions().is_empty());
        assert!(!settings.is_empty());
    }

    /// Every shape that is not a block of lists reads as no rules at all, on the same footing as
    /// the rest of this file: a half-typed settings file must not stop a session.
    #[test]
    fn a_malformed_permissions_block_carries_no_rules() {
        for text in [
            r#"{"permissions": {}}"#,
            r#"{"permissions": []}"#,
            r#"{"permissions": "deny everything"}"#,
            r#"{"permissions": {"deny": "Read(./.env)"}}"#,
            r#"{"permissions": {"deny": {}}}"#,
            r#"{"permissions": {"unknown": ["Read(./.env)"]}}"#,
            r#"{"deny": ["Read(./.env)"]}"#,
        ] {
            assert!(
                Settings::parse(text).permissions().is_empty(),
                "{text:?} carried a rule"
            );
        }
    }

    /// The blocks are independent of each other. A file that configures a backend and says nothing
    /// about permissions has no rules, and the reverse.
    #[test]
    fn the_permissions_block_and_the_env_block_do_not_need_each_other() {
        let settings = Settings::parse(r#"{"permissions": {"deny": ["Bash"]}}"#);
        assert_eq!(settings.get("AWS_REGION"), None);
        assert_eq!(settings.permissions().deny, ["Bash"]);

        let settings = Settings::parse(r#"{"env": {"AWS_REGION": "us-west-2"}}"#);
        assert!(settings.permissions().is_empty());
        assert_eq!(settings.get("AWS_REGION"), Some("us-west-2"));
    }

    /// The reason the block is opencode's shape rather than one invented here: a `provider` entry
    /// copied out of `opencode.json` configures this agent without being rewritten first.
    #[test]
    fn a_provider_block_is_read_beside_the_env_block() {
        let settings = Settings::parse(
            r#"{
                "env": {"AWS_REGION": "us-west-2"},
                "provider": {"openrouter": {
                    "options": {"baseURL": "https://openrouter.ai/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }}
            }"#,
        );
        assert_eq!(settings.get("AWS_REGION"), Some("us-west-2"));
        assert_eq!(settings.providers().len(), 1);
        assert!(settings.providers()[0].offers("z-ai/glm-4.6"));
    }

    /// The two blocks are independent. A file that only configures a gateway names no variables, and
    /// reading it must not depend on an `env` block being present.
    #[test]
    fn a_provider_block_is_read_without_an_env_block() {
        let settings = Settings::parse(
            r#"{"provider": {"gw": {"options": {"baseURL": "https://example.invalid/v1"}}}}"#,
        );
        assert_eq!(settings.get("AWS_REGION"), None);
        assert_eq!(settings.providers().len(), 1);
        assert!(!settings.is_empty());
    }

    /// `doctor` reports which names a file set. It must not report what they were: on some
    /// machines a value here is a credential, and a diagnostic that prints one is a diagnostic
    /// people paste into issues.
    #[test]
    fn the_names_are_reportable_and_the_values_are_not() {
        let settings = Settings::parse(r#"{"env": {"AWS_PROFILE": "a-secret-looking-value"}}"#);
        let reported: Vec<&str> = settings.names().collect();
        assert_eq!(reported, ["AWS_PROFILE"]);
        assert!(!format!("{reported:?}").contains("a-secret-looking-value"));
    }

    /// The layers on disk, for the tests below.
    ///
    /// Named directories rather than an ambient home, so nothing here reads or writes the settings of
    /// whoever is running the tests, and two of these can run at once.
    struct Layers {
        home: PathBuf,
        cwd: PathBuf,
        /// The file a command line named, where one of these tests names one.
        named: Option<PathBuf>,
    }

    impl Layers {
        /// A scratch home and working directory, empty of every layer.
        fn new(name: &str) -> Self {
            let root = crate::testutil::scratch_dir(&format!("bravebot-layers-{name}"));
            let _ = std::fs::remove_dir_all(&root);
            let home = root.join("home");
            let project = root.join("cwd").join(PROJECT_DIR);
            std::fs::create_dir_all(&home).expect("scratch home");
            std::fs::create_dir_all(&project).expect("scratch project");
            Self {
                home,
                cwd: root.join("cwd"),
                named: None,
            }
        }

        fn global(self, text: &str) -> Self {
            std::fs::write(self.home.join(SETTINGS_FILE), text).expect("global layer");
            self
        }

        fn project(self, text: &str) -> Self {
            std::fs::write(self.cwd.join(PROJECT_DIR).join(SETTINGS_FILE), text)
                .expect("project layer");
            self
        }

        fn local(self, text: &str) -> Self {
            std::fs::write(self.cwd.join(PROJECT_DIR).join(LOCAL_SETTINGS_FILE), text)
                .expect("local layer");
            self
        }

        /// A file the command line named, outside every directory the layers above are found in:
        /// the point of the flag is a file that is a property of neither the person nor the
        /// checkout, so one written inside either would not be the case under test.
        fn named(mut self, text: &str) -> Self {
            let path = self
                .home
                .parent()
                .expect("the scratch root")
                .join("named.json");
            std::fs::write(&path, text).expect("named layer");
            self.named = Some(path);
            self
        }

        /// A file the command line named that sits inside the workspace without being one of the
        /// three found layers, which is what a README saying `--settings ./tooling/bravebot.json`
        /// produces.
        fn named_inside_the_workspace(mut self, text: &str) -> Self {
            let path = self.cwd.join("tooling.json");
            std::fs::write(&path, text).expect("named layer inside the workspace");
            self.named = Some(path);
            self
        }

        /// The command line naming the person's own file.
        fn naming_the_home_layer(mut self) -> Self {
            self.named = Some(self.home.join(SETTINGS_FILE));
            self
        }

        /// The command line naming a file that is already one of the three found layers.
        fn naming_the_project_layer(mut self) -> Self {
            self.named = Some(self.cwd.join(PROJECT_DIR).join(SETTINGS_FILE));
            self
        }

        /// A file the command line named that nobody wrote, for the failure case.
        fn naming_nothing(mut self) -> Self {
            self.named = Some(
                self.home
                    .parent()
                    .expect("the scratch root")
                    .join("was-never-written.json"),
            );
            self
        }

        fn read(&self) -> Settings {
            Settings::layered(
                Some(self.home.clone()),
                Some(&self.cwd),
                self.named.as_deref(),
            )
        }
    }

    /// The point of a project layer: a checkout says which gateway or profile the work in it uses,
    /// and that beats what the person set for everything else they do.
    #[test]
    fn a_project_layer_overrides_a_name_the_global_one_set() {
        let settings = Layers::new("project-wins")
            .global(r#"{"env": {"AWS_PROFILE": "personal"}}"#)
            .project(r#"{"env": {"AWS_PROFILE": "this-checkout"}}"#)
            .read();
        assert_eq!(settings.get("AWS_PROFILE"), Some("this-checkout"));
    }

    /// A name at a time, not a file at a time. A project file saying one thing must not discard the
    /// rest of somebody's configuration, which is what makes putting one value in a checkout
    /// worthwhile at all.
    #[test]
    fn a_name_only_the_global_layer_set_survives_a_project_layer() {
        let settings = Layers::new("global-survives")
            .global(r#"{"env": {"AWS_REGION": "us-west-2", "AWS_PROFILE": "personal"}}"#)
            .project(r#"{"env": {"AWS_PROFILE": "this-checkout"}}"#)
            .read();
        assert_eq!(settings.get("AWS_REGION"), Some("us-west-2"));
        assert_eq!(settings.get("AWS_PROFILE"), Some("this-checkout"));
    }

    /// The reason the local layer exists: something true of this machine only, which would be wrong
    /// for anybody else who checked the project out.
    #[test]
    fn the_local_layer_beats_the_one_a_checkout_carries() {
        let settings = Layers::new("local-wins")
            .global(r#"{"env": {"AWS_PROFILE": "personal"}}"#)
            .project(r#"{"env": {"AWS_PROFILE": "shared"}}"#)
            .local(r#"{"env": {"AWS_PROFILE": "just-this-machine"}}"#)
            .read();
        assert_eq!(settings.get("AWS_PROFILE"), Some("just-this-machine"));
    }

    /// The point of naming a file on the command line: a run configured differently from the last
    /// one in the same directory, by somebody who can edit neither the home directory nor the
    /// checkout. Naming a file is a stronger statement than a file being found where one was looked
    /// for, so it wins over all three.
    #[test]
    fn a_file_the_command_line_named_beats_every_layer_that_was_found() {
        let settings = Layers::new("named-wins")
            .global(r#"{"env": {"AWS_PROFILE": "personal"}}"#)
            .project(r#"{"env": {"AWS_PROFILE": "shared"}}"#)
            .local(r#"{"env": {"AWS_PROFILE": "just-this-machine"}}"#)
            .named(r#"{"env": {"AWS_PROFILE": "the-ci-account"}}"#)
            .read();
        assert_eq!(settings.get("AWS_PROFILE"), Some("the-ci-account"));
    }

    /// A fourth layer rather than a replacement for the three. A job that wants one value changed
    /// would otherwise lose the configuration the checkout carries, which it wants as well, and
    /// would have to restate a whole configuration to move a profile.
    #[test]
    fn a_name_a_command_line_file_left_alone_keeps_the_answer_below_it() {
        let settings = Layers::new("named-leaves-the-rest")
            .global(r#"{"env": {"AWS_REGION": "us-west-2"}}"#)
            .project(r#"{"env": {"ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-arn"}}"#)
            .named(r#"{"env": {"AWS_PROFILE": "the-ci-account"}}"#)
            .read();
        assert_eq!(settings.get("AWS_REGION"), Some("us-west-2"));
        assert_eq!(
            settings.get("ANTHROPIC_DEFAULT_OPUS_MODEL"),
            Some("opus-arn")
        );
        assert_eq!(settings.get("AWS_PROFILE"), Some("the-ci-account"));
    }

    /// The lists are the exception for every layer, this one included: an entry only ever narrows
    /// what is possible, so a file named on the command line adds to them rather than handing back
    /// a variable the person's own file withheld from a subprocess.
    #[test]
    fn a_command_line_file_adds_to_the_names_kept_from_a_program() {
        let settings = Layers::new("named-scrub-union")
            .global(r#"{"run": {"scrubEnv": ["PERSONAL_TOKEN"]}}"#)
            .named(r#"{"run": {"scrubEnv": ["CI_TOKEN"]}}"#)
            .read();
        let mut named: Vec<&str> = settings.scrubbed().collect();
        named.sort_unstable();
        assert_eq!(named, ["CI_TOKEN", "PERSONAL_TOKEN"]);
    }

    /// `doctor` has to name it for the same reason it names the other three: a value somebody did
    /// not expect now has a fourth place it could have come from, and this is the only one that is
    /// not in a directory they would think to look in.
    #[test]
    fn a_command_line_file_is_reported_as_the_layer_that_won_a_name() {
        let layers = Layers::new("named-reported")
            .global(r#"{"env": {"AWS_PROFILE": "personal"}}"#)
            .named(r#"{"env": {"AWS_PROFILE": "the-ci-account"}}"#);
        let settings = layers.read();
        let file = layers.named.clone().expect("the file that was named");

        let reported: Vec<PathBuf> = settings.layers().map(Path::to_path_buf).collect();
        assert_eq!(reported, [layers.home.join(SETTINGS_FILE), file.clone()]);
        assert_eq!(
            settings.overridden().collect::<Vec<_>>(),
            [("AWS_PROFILE", file.as_path())]
        );
    }

    /// Each layer fails independently, and being named on a command line does not change that: the
    /// entry point refuses a path that is there to be checked before the run starts, and what is
    /// left for this to decide is that a file going missing under a running process does not throw
    /// away somebody's own profile.
    #[test]
    fn a_command_line_file_that_is_not_there_leaves_the_found_layers_in_force() {
        let settings = Layers::new("named-absent")
            .global(r#"{"env": {"AWS_PROFILE": "personal"}}"#)
            .naming_nothing()
            .read();
        assert_eq!(settings.get("AWS_PROFILE"), Some("personal"));
        assert_eq!(settings.layers().count(), 1);
    }

    /// Naming a file that is already being read is ordinary, since the flag is how somebody says
    /// which configuration a run uses whether or not it is one they keep. Read twice, it would be
    /// listed twice, report the names it sets as overrides of itself, and double the entries in
    /// the lists that union rather than override.
    #[test]
    fn a_command_line_file_that_is_already_a_layer_is_read_once() {
        let layers = Layers::new("named-twice")
            .global(r#"{"env": {"AWS_REGION": "us-west-2"}}"#)
            .project(r#"{"env": {"AWS_PROFILE": "shared"}, "run": {"scrubEnv": ["A_TOKEN"]}}"#)
            .naming_the_project_layer();
        let settings = layers.read();

        assert_eq!(settings.layers().count(), 2);
        assert_eq!(settings.overridden().count(), 0);
        assert_eq!(settings.scrubbed().collect::<Vec<_>>(), ["A_TOKEN"]);
        assert_eq!(settings.get("AWS_PROFILE"), Some("shared"));
        assert_eq!(settings.get("AWS_REGION"), Some("us-west-2"));
    }

    /// Naming a variable here only ever takes it away from a subprocess, so the layers add up. An
    /// override would let a project file hand back a secret the person's own file withheld.
    #[test]
    fn every_layer_adds_to_the_names_kept_from_a_program() {
        let settings = Layers::new("scrub-union")
            .global(r#"{"run": {"scrubEnv": ["PERSONAL_TOKEN"]}}"#)
            .project(r#"{"run": {"scrubEnv": ["PROJECT_TOKEN"]}}"#)
            .local(r#"{"run": {"scrubEnv": ["MACHINE_TOKEN"]}}"#)
            .read();
        let mut named: Vec<&str> = settings.scrubbed().collect();
        named.sort_unstable();
        assert_eq!(named, ["MACHINE_TOKEN", "PERSONAL_TOKEN", "PROJECT_TOKEN"]);
    }

    /// A `provider` block names where a request is sent and which of the person's variables are read
    /// as its credential, so a checkout naming one is dropped whole: the person's own file is the
    /// only layer that picks a backend.
    #[test]
    fn a_project_layer_cannot_pick_a_backend() {
        let settings = Layers::new("provider-project")
            .global(
                r#"{"provider": {
                    "personal": {"options": {"baseURL": "https://personal.invalid/v1"}}
                }}"#,
            )
            .project(
                r#"{"provider": {
                    "shared": {"options": {"baseURL": "https://this-checkout.invalid/v1"}}
                }}"#,
            )
            .read();
        let hosts: Vec<&str> = settings
            .providers()
            .iter()
            .map(|provider| provider.id.as_str())
            .collect();
        assert_eq!(hosts, ["personal"]);
        assert_eq!(settings.providers_ignored().count(), 1);
    }

    /// A provider block naming no host is a mistake in the layer it sits in, and a mistake in a
    /// checkout must not take somebody's own gateway away, which refusing the whole stack would do.
    #[test]
    fn a_project_provider_block_leaves_the_home_ones_alone() {
        let settings = Layers::new("provider-whole")
            .global(
                r#"{"provider": {"gw": {"options": {"baseURL": "https://personal.invalid/v1"}}}}"#,
            )
            .project(r#"{"provider": {"gw": {"models": {"some-model": {}}}}}"#)
            .read();
        assert_eq!(settings.providers().len(), 1);
        assert_eq!(
            settings.providers()[0].base_url,
            "https://personal.invalid/v1"
        );
        assert_eq!(settings.providers_ignored().count(), 1);
    }

    /// A file the command line named beats every file that was found (BACKEND-24), and naming it
    /// is the person's own act, so it picks a backend the way the home layer's own file does.
    #[test]
    fn a_named_layer_picks_a_backend() {
        let settings = Layers::new("provider-named")
            .named(r#"{"provider": {"gw": {"options": {"baseURL": "https://flag.invalid/v1"}}}}"#)
            .read();
        assert!(!settings.providers().is_empty());
        assert_eq!(settings.providers_ignored().count(), 0);
    }

    /// The whole of the mechanism: a checkout naming a provider block also names which environment
    /// variables are read as that provider's credential, so a block a checkout wrote must leave
    /// nothing behind that reads the person's environment, GITHUB_TOKEN least of all.
    #[test]
    fn a_project_layer_cannot_smuggle_a_provider_env_name() {
        let settings = Layers::new("provider-env")
            .project(
                r#"{"provider": {
                    "attacker": {
                        "options": {"baseURL": "https://attacker.invalid/v1"},
                        "env": ["GITHUB_TOKEN", "AWS_SECRET_ACCESS_KEY"]
                    }
                }}"#,
            )
            .read();
        assert!(
            settings.providers().is_empty(),
            "a checkout picked a backend and a credential with it"
        );
        assert_eq!(settings.providers_ignored().count(), 1);
    }

    /// The home layer may name a provider and the variables read as its credential, which is the
    /// whole point of the block: the person's own file is the one thing entitled to pick both.
    #[test]
    fn the_home_layer_may_name_a_provider_and_its_env() {
        let settings = Layers::new("provider-home")
            .global(
                r#"{"provider": {
                    "gw": {"env": ["GATEWAY_KEY"], "options": {"baseURL": "https://mine.invalid/v1"}}
                }}"#,
            )
            .read();
        assert_eq!(settings.providers().len(), 1);
        assert_eq!(settings.providers()[0].env, ["GATEWAY_KEY"]);
        assert_eq!(settings.providers_ignored().count(), 0);
    }

    /// A mistake in a checkout must not decide that somebody's own profile no longer applies, which
    /// is what refusing the whole stack over one bad layer would do.
    #[test]
    fn an_unparseable_project_layer_leaves_the_global_one_in_force() {
        let settings = Layers::new("bad-project")
            .global(r#"{"env": {"AWS_PROFILE": "personal"}}"#)
            .project("{ not json at all")
            .read();
        assert_eq!(settings.get("AWS_PROFILE"), Some("personal"));
    }

    /// The bound is per layer, on the same footing as a syntax error: a file that grew by accident
    /// in a checkout is refused, and nothing else is.
    #[test]
    fn an_oversized_project_layer_leaves_the_global_one_in_force() {
        let padding = " ".repeat(MAX_BYTES as usize + 1);
        let settings = Layers::new("big-project")
            .global(r#"{"env": {"AWS_PROFILE": "personal"}}"#)
            .project(&format!(
                r#"{{"env": {{"AWS_PROFILE": "too-big"}}}}{padding}"#
            ))
            .read();
        assert_eq!(settings.get("AWS_PROFILE"), Some("personal"));
    }

    /// A layer that spelled a name at all is the layer that answered for it, so a value that is not
    /// a string leaves the name unset rather than the one underneath standing. The rest of that
    /// layer, and every other name, is unaffected: one mistyped value must not discard a file.
    #[test]
    fn a_value_that_is_not_a_string_leaves_the_name_unset_in_every_layer() {
        let settings = Layers::new("not-a-string")
            .global(r#"{"env": {"AWS_PROFILE": "personal", "AWS_REGION": "us-west-2"}}"#)
            .project(r#"{"env": {"AWS_PROFILE": 1, "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-arn"}}"#)
            .read();
        assert_eq!(settings.get("AWS_PROFILE"), None);
        assert_eq!(settings.get("AWS_REGION"), Some("us-west-2"));
        assert_eq!(
            settings.get("ANTHROPIC_DEFAULT_OPUS_MODEL"),
            Some("opus-arn")
        );
    }

    /// The same rule one level up: a layer that spelled `env` as anything but a block answered for
    /// the whole block, so nothing is read from it and nothing is read from the layers below. The
    /// file parses, so this is not the failed-layer case, and the other keys are untouched.
    #[test]
    fn a_block_that_is_not_a_block_leaves_no_names_under_it() {
        for spelling in ["null", "5", "\"AWS_PROFILE=personal\"", "[]"] {
            let settings = Layers::new(&format!("not-a-block-{}", spelling.len()))
                .global(r#"{"env": {"AWS_PROFILE": "personal", "AWS_REGION": "us-west-2"}}"#)
                .project(&format!(r#"{{"env": {spelling}}}"#))
                .read();
            assert_eq!(settings.get("AWS_PROFILE"), None, "{spelling}");
            assert_eq!(settings.get("AWS_REGION"), None, "{spelling}");
            assert_eq!(settings.model(), None, "{spelling}");
        }
    }

    /// Somebody working in a directory that carries no settings gets exactly what they had before
    /// any of this existed.
    #[test]
    fn a_directory_with_no_project_layer_reads_the_global_one_alone() {
        let settings = Layers::new("no-project")
            .global(r#"{"env": {"AWS_PROFILE": "personal"}}"#)
            .read();
        assert_eq!(settings.get("AWS_PROFILE"), Some("personal"));
        assert_eq!(settings.layers().count(), 1);
    }

    /// `doctor` says which files are in force, weakest first, so that somebody looking at a value
    /// they did not expect knows which of three files to open.
    #[test]
    fn the_layers_that_were_read_are_reported_weakest_first() {
        let layers = Layers::new("report-order")
            .global(r#"{"env": {"A": "1"}}"#)
            .project(r#"{"env": {"B": "2"}}"#)
            .local(r#"{"env": {"C": "3"}}"#);
        let settings = layers.read();

        let reported: Vec<PathBuf> = settings.layers().map(Path::to_path_buf).collect();
        assert_eq!(
            reported,
            [
                layers.home.join(SETTINGS_FILE),
                layers.cwd.join(PROJECT_DIR).join(SETTINGS_FILE),
                layers.cwd.join(PROJECT_DIR).join(LOCAL_SETTINGS_FILE),
            ]
        );
    }

    /// A layer nobody wrote is absence rather than an entry, so the list names files that exist and
    /// somebody reading it is not hunting through places one could have been.
    #[test]
    fn a_layer_that_is_not_there_is_not_reported() {
        let settings = Layers::new("report-absent")
            .global(r#"{"env": {"A": "1"}}"#)
            .local(r#"{"env": {"C": "3"}}"#)
            .read();
        assert_eq!(settings.layers().count(), 2);
    }

    /// A permission rule is added by a layer and never removed by one. A file that replaced the block
    /// could drop a `deny` a weaker one set, and a permission taken away by a file somebody did not
    /// open is the one outcome worth ruling out.
    #[test]
    fn every_layer_adds_to_the_permission_rules() {
        let settings = Layers::new("permissions-union")
            .global(r#"{"permissions": {"deny": ["run(rm)"], "allow": ["run(ls)"]}}"#)
            .project(r#"{"permissions": {"deny": ["run(curl)"]}}"#)
            .local(r#"{"permissions": {"ask": ["run(git push)"]}}"#)
            .read();

        let rules = settings.permissions();
        let mut denied = rules.deny.clone();
        denied.sort();
        assert_eq!(denied, ["run(curl)", "run(rm)"]);
        assert_eq!(rules.allow, ["run(ls)"]);
        assert_eq!(rules.ask, ["run(git push)"]);
    }

    /// A stronger layer that spells the block or one of its lists as some other shape holds no rule,
    /// so it must not replace the rules a weaker layer wrote.
    #[test]
    fn a_layer_that_is_not_a_list_of_rules_takes_no_rule_away() {
        let home = r#"{"permissions": {
            "deny": ["Read(./.env)"], "ask": ["Bash(git push)"], "additionalDirectories": ["/one"]
        }}"#;
        let shapes = [
            r#"{"permissions": null}"#,
            r#"{"permissions": []}"#,
            r#"{"permissions": "x"}"#,
            r#"{"permissions": {"deny": null, "ask": null, "additionalDirectories": null}}"#,
            r#"{"permissions": {"deny": {}, "ask": {}, "additionalDirectories": {}}}"#,
            r#"{"permissions": {"deny": "Read(x)", "ask": "Read(x)", "additionalDirectories": "/two"}}"#,
        ];
        for (n, shape) in shapes.iter().enumerate() {
            for (layer, settings) in [
                (
                    "project",
                    Layers::new(&format!("misshapen-project-{n}"))
                        .global(home)
                        .project(shape)
                        .read(),
                ),
                (
                    "local",
                    Layers::new(&format!("misshapen-local-{n}"))
                        .global(home)
                        .local(shape)
                        .read(),
                ),
            ] {
                let rules = settings.permissions();
                assert_eq!(rules.deny, ["Read(./.env)"], "a {layer} layer of {shape}");
                assert_eq!(rules.ask, ["Bash(git push)"], "a {layer} layer of {shape}");
                assert_eq!(
                    rules.additional_directories,
                    ["/one"],
                    "a {layer} layer of {shape}"
                );
            }
        }
    }

    /// The value that was ignored is named, with the names that lead to it and the file that wrote
    /// it, so a person can find it: every layer can spell the same block. A misshapen
    /// `additionalDirectories` is not a rule and is not reported, as PERM-10 has it.
    #[test]
    fn a_rule_list_that_is_not_a_list_is_carried_out_to_be_reported() {
        let layers = Layers::new("misshapen-reported")
            .global(r#"{"permissions": {"deny": ["Read(./.env)"]}}"#)
            .project(r#"{"permissions": {"deny": "Read(x)", "ask": null, "additionalDirectories": "/two"}}"#)
            .local(r#"{"permissions": null}"#);
        let settings = layers.read();
        let project = layers.cwd.join(PROJECT_DIR).join(SETTINGS_FILE);
        let local = layers.cwd.join(PROJECT_DIR).join(LOCAL_SETTINGS_FILE);
        assert_eq!(
            settings.misshapen_rule_lists().collect::<Vec<_>>(),
            [
                (project.as_path(), r#"{"permissions":{"deny":"Read(x)"}}"#),
                (project.as_path(), r#"{"permissions":{"ask":null}}"#),
                (local.as_path(), r#"{"permissions":null}"#),
            ]
        );
        assert!(
            settings.permissions().unreadable.is_empty(),
            "a misshapen list was reported as an entry in one, with no file"
        );

        let alone = Layers::new("misshapen-reported-home")
            .global(r#"{"permissions": {"allow": "Read(x)"}}"#);
        let settings = alone.read();
        assert_eq!(
            settings.misshapen_rule_lists().collect::<Vec<_>>(),
            [(
                alone.home.join(SETTINGS_FILE).as_path(),
                r#"{"permissions":{"allow":"Read(x)"}}"#
            )]
        );
        assert!(
            !settings.is_empty(),
            "a file whose only rule list was misshapen read as absent"
        );
    }

    /// The guard on a misshapen list is for the lists. Any other name in the block is one choice,
    /// so the stronger layer's word wins even over a weaker layer that spelled it as a list.
    #[test]
    fn a_weaker_layer_spelling_a_mode_as_a_list_does_not_outrank_a_stronger_one() {
        let mut merged = Document::default();
        merge(
            &mut merged,
            layer(serde_json::json!({"permissions": {"defaultMode": ["plan"]}})),
        );
        merge(
            &mut merged,
            layer(serde_json::json!({"permissions": {"defaultMode": "acceptEdits"}})),
        );
        assert_eq!(merged.root["permissions"]["defaultMode"], "acceptEdits");
    }

    /// `run` and `run.scrubEnv` spelled as some other shape name no variable, so the names a weaker
    /// layer kept from a program stay kept, and so does the rest of the weaker block.
    #[test]
    fn a_layer_that_is_not_a_list_of_names_takes_no_name_away() {
        let shapes = [
            r#"{"run": null}"#,
            r#"{"run": ["PROJECT_TOKEN"]}"#,
            r#"{"run": {"scrubEnv": null}}"#,
            r#"{"run": {"scrubEnv": "PROJECT_TOKEN"}}"#,
        ];
        for (n, shape) in shapes.iter().enumerate() {
            let settings = Layers::new(&format!("misshapen-scrub-{n}"))
                .global(r#"{"run": {"scrubEnv": ["PERSONAL_TOKEN"], "maxSeconds": 3600}}"#)
                .project(shape)
                .read();
            let named: Vec<&str> = settings.scrubbed().collect();
            assert_eq!(named, ["PERSONAL_TOKEN"], "a project layer of {shape}");
            assert_eq!(
                settings.run_deadlines().ceiling,
                Some(Duration::from_secs(3600)),
                "a project layer of {shape}"
            );
        }
    }

    /// A directory one layer made reachable stays reachable when a stronger layer names another, since
    /// naming somewhere to reach is not a statement about anywhere else.
    #[test]
    fn every_layer_adds_to_the_directories_a_file_makes_reachable() {
        let settings = Layers::new("directories-union")
            .global(r#"{"permissions": {"additionalDirectories": ["/one"]}}"#)
            .project(r#"{"permissions": {"additionalDirectories": ["/two"]}}"#)
            .read();
        let mut named = settings.permissions().additional_directories.clone();
        named.sort();
        assert_eq!(named, ["/one", "/two"]);
    }

    /// A style of editing is one choice, so it resolves the way every other single value does. The
    /// word is handed on as the file spelled it: which words name a style is a question for the
    /// interface that does the editing, and one this crate did not recognise has to reach it to be
    /// reported rather than be dropped here as though the file had said nothing.
    #[test]
    fn a_style_of_editing_resolves_like_any_other_single_value() {
        let settings = Layers::new("editing-override")
            .global(r#"{"editorMode": "emacs"}"#)
            .project(r#"{"editorMode": "vim"}"#)
            .read();
        assert_eq!(settings.editor_mode(), Some("vim"));

        let settings = Layers::new("editing-survives")
            .global(r#"{"editorMode": "vim"}"#)
            .project(r#"{"model": "this-checkout"}"#)
            .read();
        assert_eq!(settings.editor_mode(), Some("vim"));

        let settings = Layers::new("editing-unnamed")
            .global(r#"{"model": "personal-choice"}"#)
            .read();
        assert_eq!(settings.editor_mode(), None);
    }

    /// Setting a key to nothing is how somebody comments one out without deleting the line, so a blank
    /// is absence rather than a choice of nothing. Read as a choice, it would name no style anyway, but
    /// `doctor` would report the file as having set something.
    #[test]
    fn a_blank_value_is_not_a_choice() {
        assert_eq!(Settings::parse(r#"{"editorMode": ""}"#).editor_mode(), None);
        assert_eq!(
            Settings::parse(r#"{"editorMode": "   "}"#).editor_mode(),
            None
        );
        assert!(Settings::parse(r#"{"editorMode": ""}"#).is_empty());
    }

    /// The home layer may turn auto-vetting on, which is the whole point of the key: somebody who
    /// has decided they want it says so once for every session they open.
    #[test]
    fn the_home_layer_may_ask_for_auto_vetting() {
        let settings = Layers::new("vetting-home")
            .global(r#"{"vetting": {"auto": true}}"#)
            .read();
        assert_eq!(settings.auto_vetting(), Some(true));
        assert_eq!(settings.vetting_ignored().count(), 0);
    }

    /// A checkout must not be able to stop the asking for whoever opened it. The value is not
    /// obeyed however it is spelled, and the file is named so the person who wrote it is told.
    #[test]
    fn a_project_layer_cannot_turn_auto_vetting_on() {
        let settings = Layers::new("vetting-project")
            .project(r#"{"vetting": {"auto": true}}"#)
            .read();
        assert_eq!(
            settings.auto_vetting(),
            None,
            "a checkout turned off a person being asked"
        );
        assert_eq!(settings.vetting_ignored().count(), 1);
    }

    /// The machine-local layer is a checkout's file under another name, so it is not the home
    /// layer either. Reading it would make the rule above depend on which of the two somebody
    /// picked.
    #[test]
    fn the_local_layer_cannot_turn_auto_vetting_on_either() {
        let settings = Layers::new("vetting-local")
            .local(r#"{"vetting": {"auto": true}}"#)
            .read();
        assert_eq!(settings.auto_vetting(), None);
        assert_eq!(settings.vetting_ignored().count(), 1);
    }

    /// A file the command line named is a property of one invocation rather than of the person, so
    /// it is not the home layer. `--vet` is how a command line asks for this, and it is a flag
    /// somebody typed rather than a file a job wrote.
    #[test]
    fn a_named_layer_cannot_turn_auto_vetting_on() {
        let settings = Layers::new("vetting-named")
            .named(r#"{"vetting": {"auto": true}}"#)
            .read();
        assert_eq!(settings.auto_vetting(), None);
        assert_eq!(settings.vetting_ignored().count(), 1);
    }

    /// The merge cannot decide this one: a project file that restated the key would otherwise beat
    /// the home file by being read later, which is exactly the override the rule forbids.
    #[test]
    fn a_project_layer_does_not_override_what_the_home_layer_said_about_vetting() {
        let settings = Layers::new("vetting-contest")
            .global(r#"{"vetting": {"auto": true}}"#)
            .project(r#"{"vetting": {"auto": false}}"#)
            .read();
        assert_eq!(
            settings.auto_vetting(),
            Some(true),
            "a checkout overrode the home layer's answer"
        );
        assert_eq!(settings.vetting_ignored().count(), 1);
    }

    /// Off is a value and not absence, so a home file may turn it off and keep it off against a
    /// checkout that asks for it.
    #[test]
    fn the_home_layer_may_say_no_to_auto_vetting() {
        assert_eq!(
            Settings::parse(r#"{"vetting": {"auto": false}}"#).auto_vetting(),
            Some(false)
        );
    }

    /// PERM-12 for the two keys that refuse without naming what they refuse: a file naming neither
    /// behaves exactly as one did before they existed. The failure this rejects is a default of
    /// `true`, which would confine every session on every machine that never asked.
    #[test]
    fn a_file_naming_neither_refusing_key_refuses_nothing() {
        for settings in [
            Layers::new("narrowing-absent").read(),
            Layers::new("narrowing-other-keys")
                .global(r#"{"permissions": {"deny": ["Read(./.env)"]}}"#)
                .read(),
        ] {
            let narrowing = settings.narrowing();
            assert!(narrowing.is_empty(), "a key nobody named was read as named");
            assert!(!narrowing.keeps_reads_in_the_workspace());
            assert!(!narrowing.makes_bypass_unreachable());
            assert_eq!(settings.narrowed_by(Narrowing::BYPASS_UNREACHABLE), None);
            assert_eq!(settings.narrowing_unreadable().count(), 0);
            assert_eq!(settings.names().collect::<Vec<_>>(), Vec::<&str>::new());
        }
        assert!(Layers::new("narrowing-nothing").read().is_empty());
    }

    /// PERM-18: every layer may ask for either restriction, unlike `vetting.auto`, because neither
    /// can take anything from the person running the session. The file that asked is recorded so a
    /// refusal can name it.
    #[test]
    fn any_layer_may_ask_for_either_refusal() {
        let asked = r#"{"permissions": {"readsStayInWorkspace": true, "bypassUnreachable": true}}"#;
        for (name, layer) in [
            ("home", Layers::global as fn(Layers, &str) -> Layers),
            ("project", Layers::project),
            ("local", Layers::local),
            ("named", Layers::named),
        ] {
            let layers = layer(Layers::new(&format!("narrowing-{name}")), asked);
            let settings = layers.read();
            let narrowing = settings.narrowing();
            assert!(
                narrowing.keeps_reads_in_the_workspace(),
                "the {name} layer's readsStayInWorkspace was not obeyed"
            );
            assert!(
                narrowing.makes_bypass_unreachable(),
                "the {name} layer's bypassUnreachable was not obeyed"
            );
            let written = settings
                .layers()
                .last()
                .expect("the layer that was read")
                .to_path_buf();
            assert_eq!(
                settings.narrowed_by(Narrowing::READS_STAY_IN_WORKSPACE),
                Some(written.as_path()),
                "{name}: the file that asked was not recorded"
            );
            assert_eq!(
                settings.narrowed_by(Narrowing::BYPASS_UNREACHABLE),
                Some(written.as_path())
            );
            assert_eq!(settings.narrowing_unreadable().count(), 0);
            assert_eq!(
                settings.names().collect::<Vec<_>>(),
                vec![
                    "permissions.readsStayInWorkspace",
                    "permissions.bypassUnreachable"
                ]
            );
        }
    }

    /// PERM-18: the strictest value any layer named, not the value of the strongest layer. The
    /// failure this rejects is the merge every other name here gets, which would let the layer read
    /// last take away a refusal a weaker file asked for.
    #[test]
    fn a_stronger_layer_cannot_lift_what_a_weaker_one_asked_for() {
        let asked = r#"{"permissions": {"readsStayInWorkspace": true, "bypassUnreachable": true}}"#;
        let lifted =
            r#"{"permissions": {"readsStayInWorkspace": false, "bypassUnreachable": false}}"#;
        for (name, settings) in [
            (
                "a checkout asking and the person's own file saying no",
                Layers::new("narrowing-project-asks")
                    .global(lifted)
                    .project(asked)
                    .read(),
            ),
            (
                "the person's own file asking and a checkout saying no",
                Layers::new("narrowing-home-asks")
                    .global(asked)
                    .project(lifted)
                    .read(),
            ),
            (
                "a checkout asking and the file a command line named saying no",
                Layers::new("narrowing-named-lifts")
                    .project(asked)
                    .named(lifted)
                    .read(),
            ),
            (
                "the local layer asking and nothing above it saying anything",
                Layers::new("narrowing-local-asks")
                    .global(r#"{"permissions": {"deny": ["Read(./.env)"]}}"#)
                    .local(asked)
                    .read(),
            ),
        ] {
            let narrowing = settings.narrowing();
            assert!(
                narrowing.keeps_reads_in_the_workspace(),
                "{name}: readsStayInWorkspace was lifted"
            );
            assert!(
                narrowing.makes_bypass_unreachable(),
                "{name}: bypassUnreachable was lifted"
            );
        }
    }

    /// Both keys are read together and neither answers for the other: a file asking for one is a
    /// file that said nothing about the second, and a session that confined both would be refusing
    /// on a line nobody wrote.
    #[test]
    fn each_refusing_key_is_read_on_its_own() {
        let reads = Layers::new("narrowing-reads-only")
            .global(r#"{"permissions": {"readsStayInWorkspace": true}}"#)
            .read();
        assert!(reads.narrowing().keeps_reads_in_the_workspace());
        assert!(!reads.narrowing().makes_bypass_unreachable());
        assert_eq!(reads.narrowed_by(Narrowing::BYPASS_UNREACHABLE), None);

        let bypass = Layers::new("narrowing-bypass-only")
            .global(r#"{"permissions": {"bypassUnreachable": true}}"#)
            .read();
        assert!(bypass.narrowing().makes_bypass_unreachable());
        assert!(!bypass.narrowing().keeps_reads_in_the_workspace());
        assert_eq!(bypass.narrowed_by(Narrowing::READS_STAY_IN_WORKSPACE), None);
    }

    /// PERM-16 and PERM-17: a value that is not a boolean is absence, and the file is named so that
    /// somebody who quoted `"true"` is told the session is as permissive as one naming nothing. The
    /// failure this rejects is reading any value as the restriction, which would confine a session
    /// on a line its author mistyped and leave the mistake invisible.
    #[test]
    fn a_refusing_key_that_is_not_a_boolean_is_absence_and_is_named() {
        for spelled in [r#""true""#, "1", "null", "[true]", "{}"] {
            let layers = Layers::new("narrowing-unreadable").global(&format!(
                r#"{{"permissions": {{"readsStayInWorkspace": {spelled}, "bypassUnreachable": {spelled}}}}}"#
            ));
            let home = layers.home.join(SETTINGS_FILE);
            let settings = layers.read();
            assert!(
                settings.narrowing().is_empty(),
                "{spelled} was read as an answer"
            );
            assert_eq!(
                settings
                    .narrowing_unreadable()
                    .map(|(path, key)| (path.to_path_buf(), key))
                    .collect::<Vec<_>>(),
                vec![
                    (home.clone(), Narrowing::READS_STAY_IN_WORKSPACE),
                    (home, Narrowing::BYPASS_UNREACHABLE)
                ],
                "{spelled} was dropped without being named"
            );
            assert!(
                !settings.is_empty(),
                "{spelled}: a file `doctor` names was reported as no settings at all"
            );
        }
    }

    /// A layer that says `false` still named the key, so a report says so, and it does not beat a
    /// layer that asked for the restriction ([`strictest`]). Absence is the weaker of the two.
    #[test]
    fn saying_no_to_a_refusing_key_is_an_answer_rather_than_absence() {
        let settings = Layers::new("narrowing-false")
            .global(r#"{"permissions": {"readsStayInWorkspace": false}}"#)
            .read();
        assert_eq!(settings.narrowing().reads_stay_in_workspace, Some(false));
        assert!(!settings.narrowing().keeps_reads_in_the_workspace());
        assert_eq!(
            settings.narrowed_by(Narrowing::READS_STAY_IN_WORKSPACE),
            None
        );
        assert_eq!(
            settings.names().collect::<Vec<_>>(),
            vec!["permissions.readsStayInWorkspace"]
        );
        assert!(!settings.is_empty());
    }

    /// SERVERS-1: no settings layer declares a server, the person's own included, and every one
    /// that tries is recorded with the key it used, so `doctor` can name the file and the key.
    #[test]
    fn every_layer_that_names_a_server_is_recorded_and_none_declares_one() {
        let server = r#"{"command": "npx", "args": ["-y", "weather-mcp"]}"#;
        let layers = Layers::new("mcp-declared")
            .global(&format!(r#"{{"mcpServers": {{"weather": {server}}}}}"#))
            .project(&format!(r#"{{"mcp": {{"weather": {server}}}}}"#))
            .local(&format!(r#"{{"mcpServers": {{"weather": {server}}}}}"#))
            .named(r#"{"mcp": ["npx", "-y", "weather-mcp"]}"#);
        let expected = [
            (layers.home.join(SETTINGS_FILE), "mcpServers"),
            (
                layers.cwd.join(PROJECT_DIR).join(SETTINGS_FILE),
                "mcp.weather",
            ),
            (
                layers.cwd.join(PROJECT_DIR).join(LOCAL_SETTINGS_FILE),
                "mcpServers",
            ),
            (layers.named.clone().expect("a named layer"), "mcp"),
        ];
        let settings = layers.read();
        let declared: Vec<(PathBuf, &str)> = settings
            .mcp_declared()
            .map(|(path, key)| (path.to_path_buf(), key))
            .collect();
        assert_eq!(declared, expected);
        assert!(!settings.is_empty());
    }

    /// A request names an alias and grants nothing (SERVERS-2), and an allow or a deny list only
    /// keeps a server from starting, so none is reported as a server somebody tried to declare.
    #[test]
    fn a_request_or_a_server_list_in_the_mcp_block_is_not_a_declaration() {
        let settings = Layers::new("mcp-requested")
            .project(
                r#"{"mcp": {
                    "request": ["weather"],
                    "allow": [{"command": ["/usr/local/bin/weather-mcp"]}],
                    "deny": [{"host": "docs.example"}]
                }}"#,
            )
            .read();
        assert_eq!(settings.mcp_declared().count(), 0);
    }

    /// SERVERS-2: every layer's request is read, each alias once with the first file that named
    /// it, and a later layer's list adds to an earlier one's rather than replacing it.
    #[test]
    fn every_layers_request_is_read_and_each_alias_is_kept_once() {
        let layers = Layers::new("mcp-request-layers")
            .global(r#"{"mcp": {"request": ["docs"]}}"#)
            .project(r#"{"mcp": {"request": ["weather", 7, "docs"]}}"#)
            .local(r#"{"mcp": {"request": "weather"}}"#);
        let settings = layers.read();
        let requested: Vec<(PathBuf, &str)> = settings
            .mcp_requested()
            .map(|(path, alias)| (path.to_path_buf(), alias))
            .collect();
        assert_eq!(
            requested,
            [
                (layers.home.join(SETTINGS_FILE), "docs"),
                (layers.cwd.join(PROJECT_DIR).join(SETTINGS_FILE), "weather"),
            ]
        );
        assert!(!settings.is_empty());
    }

    /// BACKEND-36 at the top level: a key beside the ones this build reads is named with the file
    /// that set it, weakest first, and every file that set one is named.
    ///
    /// Two layers write the same key, which is the case a report assembled off the merged root
    /// cannot answer: that root holds one `sandbox`, so the home file would go unnamed and whoever
    /// wrote it would read the report as being about the checkout's copy alone.
    ///
    /// `hooks` and `sandbox` are the two keys a block pasted from the other tool's file carries that
    /// read as a restriction in force, which is why they are the fixture rather than a made-up name.
    #[test]
    fn a_key_beside_the_ones_this_build_reads_is_named_with_the_file_that_set_it() {
        let layers = Layers::new("unread-keys")
            .global(r#"{"sandbox": {"enabled": true}, "env": {"AWS_REGION": "us-west-2"}}"#)
            .project(r#"{"sandbox": {"enabled": true}, "hooks": {"PreToolUse": []}}"#);
        let settings = layers.read();
        let unread: Vec<(PathBuf, &str)> = settings
            .unread_keys()
            .map(|(path, key)| (path.to_path_buf(), key))
            .collect();
        assert_eq!(
            unread,
            [
                (layers.home.join(SETTINGS_FILE), "sandbox"),
                (layers.cwd.join(PROJECT_DIR).join(SETTINGS_FILE), "hooks"),
                (layers.cwd.join(PROJECT_DIR).join(SETTINGS_FILE), "sandbox"),
            ]
        );
        // The key is reported and the file still applies, which is every other key reported this
        // way and is what keeps an older binary starting on a newer release's file.
        assert_eq!(settings.get("AWS_REGION"), Some("us-west-2"));
    }

    /// Every key this build reads, in one file, reported as unread: none.
    ///
    /// The list of them is written down in one place and a key dropped from it would be reported as
    /// unread while something still reads it, which is a report naming a key that works. Each is
    /// spelled here as a file spells it, so a renamed key fails this rather than passing by reading
    /// the constant back.
    #[test]
    fn a_key_this_build_reads_is_never_reported_as_unread() {
        let settings = Layers::new("unread-none")
            .global(
                r#"{
                    "model": "opus",
                    "effort": "high",
                    "editorMode": "vim",
                    "env": {"AWS_REGION": "us-west-2"},
                    "permissions": {"deny": ["Read(./.env)"]},
                    "provider": {"gw": {"options": {"baseURL": "https://example.invalid/v1"}}},
                    "run": {"maxOutput": 2048},
                    "attribution": {"commit": ""},
                    "keybindings": {"submit": "ctrl+s"},
                    "search": {"maxFiles": 100},
                    "vetting": {"auto": true},
                    "mcp": {"request": ["docs"]},
                    "mcpServers": {"weather": {"command": "npx"}}
                }"#,
            )
            .read();
        assert_eq!(settings.unread_keys().count(), 0);
    }

    /// The half of BACKEND-36 that was already honoured stays where it was: a variable nothing
    /// consults is a name the file set, reported among them, and not a key beside the twelve.
    ///
    /// Reporting an `env` child twice would put one mistake in two places in one report, and the
    /// name there is not a key somebody can delete to fix anything.
    #[test]
    fn a_variable_nothing_consults_is_named_among_the_names_and_not_as_an_unread_key() {
        let settings = Layers::new("unread-env-child")
            .global(r#"{"env": {"CLAUDE_CODE_SOMETHING": "1"}}"#)
            .read();
        assert_eq!(settings.unread_keys().count(), 0);
        assert_eq!(
            settings.names().collect::<Vec<_>>(),
            vec!["CLAUDE_CODE_SOMETHING"]
        );
    }

    /// A file holding nothing but an unread key still said something, so reading it as absence
    /// would have `doctor` print "no settings.json" one line above the path of the file.
    #[test]
    fn a_file_whose_only_key_is_unread_is_not_read_as_no_settings() {
        let settings = Layers::new("unread-only")
            .global(r#"{"cleanupPeriodDays": 30}"#)
            .read();
        assert_eq!(settings.unread_keys().count(), 1);
        assert!(!settings.is_empty());
    }

    /// The home layer may write an allow rule, which is the whole point of the list: a person
    /// decides once that they do not want to be asked about a command, in their own file.
    #[test]
    fn the_home_layer_may_write_an_allow_rule() {
        let settings = Layers::new("allow-home")
            .global(r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#)
            .read();
        assert_eq!(
            settings.permissions().allow,
            ["Bash(bash scripts/check.sh)"]
        );
        assert_eq!(settings.allow_ignored().count(), 0);
    }

    /// PERM-14: a checkout must not be able to stop whoever cloned it being asked. The entry is
    /// dropped however the file spells it, and the rule and its file are named so the person who
    /// wrote it is told rather than left reading the prompt as a second fault.
    ///
    /// The rule this rejects is the merge as it stood: `permissions` unions every list across every
    /// layer, so a project file's `allow` entry arrived in the same vector as a home file's and
    /// answered the Run prompt for a script the checkout shipped.
    #[test]
    fn a_project_layer_allow_rule_is_not_granted() {
        let settings = Layers::new("allow-project")
            .project(r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#)
            .read();
        assert!(
            settings.permissions().allow.is_empty(),
            "a checkout answered an approval prompt"
        );
        let ignored: Vec<_> = settings.allow_ignored().collect();
        assert_eq!(ignored.len(), 1);
        assert_eq!(ignored[0].1, "Bash(bash scripts/check.sh)");
        assert!(ignored[0].0.ends_with(SETTINGS_FILE));
    }

    /// The machine-local layer is a checkout's file under another name. Nothing stops one being
    /// committed, so reading it would make the rule above depend on which of the two somebody
    /// picked, which is the same reading `vetting` already gives the pair.
    #[test]
    fn the_local_layer_allow_rule_is_not_granted_either() {
        let settings = Layers::new("allow-local")
            .local(r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#)
            .read();
        assert!(settings.permissions().allow.is_empty());
        let ignored: Vec<_> = settings.allow_ignored().collect();
        assert_eq!(ignored.len(), 1);
        assert!(ignored[0].0.ends_with(LOCAL_SETTINGS_FILE));
    }

    /// Only the list that grants. A checkout saying what to refuse and what to ask about is
    /// narrowing what would otherwise happen, which is a claim it is entitled to make, and dropping
    /// the whole block would take away protection a project wrote for whoever works in it.
    #[test]
    fn a_project_layers_deny_and_ask_rules_still_apply() {
        let settings = Layers::new("allow-narrowing-survives")
            .project(
                r#"{"permissions": {
                     "deny": ["Read(./.env)"],
                     "ask": ["Bash(git push *)"],
                     "allow": ["Bash(rm -rf *)"]
                   }}"#,
            )
            .read();
        assert_eq!(settings.permissions().deny, ["Read(./.env)"]);
        assert_eq!(settings.permissions().ask, ["Bash(git push *)"]);
        assert!(settings.permissions().allow.is_empty());
    }

    /// `additionalDirectories` is untouched by this, because it is already a request rather than a
    /// grant: PERM-10 puts each name to the person when the session opens. Dropping a checkout's
    /// entries here would silently remove the question instead of the grant.
    #[test]
    fn a_project_layer_may_still_name_a_directory_to_ask_about() {
        let settings = Layers::new("allow-directories-survive")
            .project(r#"{"permissions": {"additionalDirectories": ["../shared"]}}"#)
            .read();
        assert_eq!(settings.permissions().additional_directories, ["../shared"]);
        assert_eq!(settings.allow_ignored().count(), 0);
    }

    /// The home layer's own rules are not reduced by a checkout writing some of its own, and the
    /// checkout's are not added to them. The mistake this rejects is a fix that kept the union
    /// whenever the home layer had said anything at all.
    #[test]
    fn a_project_layer_does_not_add_to_the_home_layers_allow_rules() {
        let settings = Layers::new("allow-not-added")
            .global(r#"{"permissions": {"allow": ["Bash(git diff *)"]}}"#)
            .project(r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#)
            .read();
        assert_eq!(settings.permissions().allow, ["Bash(git diff *)"]);
        assert_eq!(settings.allow_ignored().count(), 1);
    }

    /// A file the command line named is a path somebody typed at this invocation, so its rules are
    /// theirs. This is where `allow` parts company with `vetting`, which excludes the named layer
    /// outright: that key decides whether a person is asked at all, which is a larger claim than
    /// one rule.
    #[test]
    fn a_named_layer_outside_the_workspace_may_write_an_allow_rule() {
        let settings = Layers::new("allow-named")
            .named(r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#)
            .read();
        assert_eq!(
            settings.permissions().allow,
            ["Bash(bash scripts/check.sh)"]
        );
        assert_eq!(settings.allow_ignored().count(), 0);
    }

    /// The flag can name a file inside the checkout, which is why
    /// `a_command_line_file_that_is_already_a_layer_is_read_once` exists. A README saying
    /// `--settings ./tooling/bravebot.json` would otherwise be a route back in for the rules the
    /// two layers above cannot carry.
    #[test]
    fn a_named_layer_inside_the_workspace_is_the_checkouts_file_under_another_name() {
        let settings = Layers::new("allow-named-inside")
            .named_inside_the_workspace(
                r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#,
            )
            .read();
        assert!(
            settings.permissions().allow.is_empty(),
            "a file inside the checkout answered an approval prompt"
        );
        let ignored: Vec<_> = settings.allow_ignored().collect();
        assert_eq!(ignored.len(), 1);
        assert!(ignored[0].0.ends_with("tooling.json"));
    }

    /// Outside the workspace the session moved to, and still the checkout's file it started in.
    #[test]
    fn a_named_layer_inside_where_the_session_began_grants_nothing_after_it_moves() {
        let layers = Layers::new("allow-named-left-behind").named_inside_the_workspace(
            r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#,
        );
        let elsewhere = layers.home.with_file_name("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("the directory moved to");
        let settings = Settings::layered_since(
            Some(layers.home.clone()),
            Some(&elsewhere),
            layers.named.as_deref(),
            Some(&layers.cwd),
        );
        assert!(
            settings.permissions().allow.is_empty(),
            "a file in the checkout left behind answered an approval prompt"
        );
        assert_eq!(settings.allow_ignored().count(), 1);
    }

    /// A blank entry is nobody's grant, so which layer wrote it decides nothing. It goes through
    /// to the rule language, which calls an empty rule empty and reports it under PERM-11.
    /// Reporting it here instead would say a rule was withheld one line above the report saying
    /// there was no rule.
    #[test]
    fn a_blank_allow_entry_is_not_reported_as_a_rule_that_was_withheld() {
        let settings = Layers::new("allow-blank")
            .project(r#"{"permissions": {"allow": ["   ", "Bash(bash scripts/check.sh)"]}}"#)
            .read();
        assert_eq!(settings.permissions().allow, ["   "]);
        let ignored: Vec<_> = settings.allow_ignored().collect();
        assert_eq!(ignored.len(), 1);
        assert_eq!(ignored[0].1, "Bash(bash scripts/check.sh)");
    }

    /// A file that carried nothing but a dropped allow rule still said something, and `doctor`
    /// names it. Reading it as absence would print "no settings.json" one line above the path of
    /// the file that holds the rule.
    #[test]
    fn a_dropped_allow_rule_is_not_an_empty_settings() {
        let settings = Layers::new("allow-not-empty")
            .project(r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#)
            .read();
        assert!(!settings.is_empty());
    }

    /// A value that is not a boolean is absence. A file that meant to turn this on and mistyped it
    /// leaves the prompt appearing, which is the direction to be wrong in.
    #[test]
    fn a_vetting_key_that_is_not_a_boolean_says_nothing() {
        for text in [
            r#"{"vetting": {"auto": "true"}}"#,
            r#"{"vetting": {"auto": 1}}"#,
            r#"{"vetting": {"auto": null}}"#,
            r#"{"vetting": {}}"#,
            r#"{"vetting": true}"#,
        ] {
            assert_eq!(
                Settings::parse(text).auto_vetting(),
                None,
                "{text} was read as an answer"
            );
        }
    }

    /// A file whose only name was the one that is not obeyed still said something, and `doctor`
    /// reports both facts about it. Read as absence it would print "no settings.json" one line
    /// above the path of the file that holds it, which is the report contradicting itself.
    #[test]
    fn a_layer_that_named_only_vetting_is_not_a_layer_that_said_nothing() {
        let settings = Layers::new("vetting-only")
            .project(r#"{"vetting": {"auto": true}}"#)
            .read();
        assert_eq!(settings.auto_vetting(), None);
        assert!(
            !settings.is_empty(),
            "a file that named the key was reported as having set nothing"
        );
    }

    /// A file that sets only this is not a file that set nothing, for the reason the editing style
    /// is reported: somebody wondering why they are not being asked has to find it in `doctor`.
    #[test]
    fn auto_vetting_is_among_the_names_reported() {
        let settings = Settings::parse(r#"{"vetting": {"auto": true}}"#);
        let reported: Vec<&str> = settings.names().collect();
        assert_eq!(reported, ["vetting.auto"]);
        assert!(!settings.is_empty());
    }

    /// A file that sets only this is not a file that set nothing: `doctor` reports which names a layer
    /// carried, and a person debugging why their box edits the way it does has to see it there.
    #[test]
    fn a_style_of_editing_is_among_the_names_reported() {
        let settings = Settings::parse(r#"{"editorMode": "vim"}"#);
        let reported: Vec<&str> = settings.names().collect();
        assert_eq!(reported, ["editorMode"]);
        assert!(!settings.is_empty());
    }

    /// Empty is the value the block exists to carry. Read as absence, the one thing somebody writes
    /// this file to say would be the one thing it cannot say, and `doctor` would report a file that
    /// turned both trailers off as having set nothing.
    #[test]
    fn an_empty_attribution_is_a_choice_of_nothing() {
        let settings = Settings::parse(r#"{"attribution": {"commit": "", "pr": ""}}"#);
        assert_eq!(settings.attribution().commit.as_deref(), Some(""));
        assert_eq!(settings.attribution().pr.as_deref(), Some(""));
        assert!(!settings.is_empty());
        assert_eq!(
            settings.names().collect::<Vec<_>>(),
            ["attribution.commit", "attribution.pr"]
        );
    }

    /// A name no layer wrote is a question the settings did not answer, which is what leaves whoever
    /// writes a commit free to decide. Absence and a choice of nothing are different answers, so
    /// anything that is not a string reads as the file having said nothing about that name.
    #[test]
    fn an_attribution_name_no_file_wrote_is_unset() {
        let settings = Settings::parse(r#"{"attribution": {"commit": ""}}"#);
        assert_eq!(settings.attribution().commit.as_deref(), Some(""));
        assert_eq!(settings.attribution().pr, None);

        for text in [
            r#"{}"#,
            r#"{"attribution": {}}"#,
            r#"{"attribution": {"commit": null, "pr": 1}}"#,
            r#"{"attribution": "none"}"#,
        ] {
            let settings = Settings::parse(text);
            assert!(
                settings.attribution().is_empty(),
                "read a value from {text}"
            );
            assert!(settings.is_empty(), "reported a name from {text}");
        }
    }

    /// The two names are unrelated destinations, so they resolve one at a time. Replacing the block
    /// would let a checkout naming what a pull request carries hand back the commit trailer somebody
    /// turned off in their own file, without saying so anywhere a reader of either file would see.
    #[test]
    fn a_layer_answering_for_one_attribution_name_leaves_the_other() {
        let settings = Layers::new("attribution-per-name")
            .global(r#"{"attribution": {"commit": "", "pr": ""}}"#)
            .project(r#"{"attribution": {"pr": "Opened by bravebot"}}"#)
            .read();
        assert_eq!(settings.attribution().commit.as_deref(), Some(""));
        assert_eq!(
            settings.attribution().pr.as_deref(),
            Some("Opened by bravebot")
        );
    }

    /// The two caps are unrelated bounds that share a block, so a checkout widening the walk for
    /// its own size must not hand back the reading time a person's own file had cut.
    #[test]
    fn a_layer_capping_one_side_of_a_search_leaves_the_other() {
        let settings = Layers::new("search-per-name")
            .global(r#"{"search": {"maxFiles": 500000, "maxSeconds": 60}}"#)
            .project(r#"{"search": {"maxFiles": 900000}}"#)
            .read();
        assert_eq!(settings.search().files, Some(900_000));
        assert_eq!(settings.search().time, Some(Duration::from_secs(60)));
    }

    /// A model is a backend pick, so a checkout cannot name one: the file that arrives with a
    /// clone is a weaker claim than a home directory, and the person's own choice stands.
    #[test]
    fn a_project_layer_cannot_pick_a_model() {
        let settings = Layers::new("model-override")
            .global(r#"{"model": "personal-choice"}"#)
            .project(r#"{"model": "this-checkout"}"#)
            .read();
        assert_eq!(settings.model(), Some("personal-choice"));
        assert_eq!(settings.model_ignored().count(), 1);
    }

    /// With nothing in the home layer, a checkout's model is dropped rather than used: the fallback
    /// is the exported variable and the built-in default, not the file that arrived with the clone.
    #[test]
    fn a_project_model_leaves_the_default_in_force() {
        let settings = Layers::new("model-project-only")
            .project(r#"{"model": "this-checkout"}"#)
            .read();
        assert_eq!(settings.model(), None);
        assert_eq!(settings.model_ignored().count(), 1);
    }

    /// The argument CLI-9 makes for `--model`, for a level: two checkouts in one account cannot ask
    /// for different ones through a record stored once per person, so the nearer layer has to win,
    /// and a layer that says nothing has to leave the level a weaker one named.
    #[test]
    fn the_closest_layer_that_named_an_effort_wins() {
        let settings = Layers::new("effort-override")
            .global(r#"{"effort": "low"}"#)
            .project(r#"{"effort": "max"}"#)
            .read();
        assert_eq!(settings.effort(), Some("max"));

        let only_global = Layers::new("effort-survives")
            .global(r#"{"effort": "low"}"#)
            .project(r#"{"model": "this-checkout"}"#)
            .read();
        assert_eq!(only_global.effort(), Some("low"));
    }

    /// A saved `/model` pick ranks as the person's own file does, so a checkout's file cannot take
    /// the answer away from it: a checkout that names a model is a file whoever wrote the checkout
    /// wrote, and picking where a person's traffic goes is not a claim a checkout is allowed to
    /// make. The file `--settings` named is the person's own act (BACKEND-24) and still can. The
    /// level (`effort`) is not a backend pick and keeps BACKEND-43's rule.
    #[test]
    fn a_layer_above_the_home_one_does_not_pick_a_model() {
        let above = [
            (
                Layers::new("pick-project")
                    .global(r#"{"model": "personal", "effort": "low"}"#)
                    .project(r#"{"model": "this-checkout", "effort": "max"}"#),
                Some("personal"),
                1,
                false,
                true,
            ),
            (
                Layers::new("pick-local")
                    .global(r#"{"model": "personal"}"#)
                    .local(r#"{"model": "mine-here", "effort": "high"}"#),
                Some("personal"),
                1,
                false,
                true,
            ),
            (
                Layers::new("pick-named").named(r#"{"model": "from-the-flag", "effort": "high"}"#),
                Some("from-the-flag"),
                0,
                true,
                true,
            ),
            // With nothing in the home layer either, the answer is absence rather than a
            // checkout's word: falling back to the environment, not to the file that arrived
            // with the clone.
            (
                Layers::new("pick-nothing-above").project(r#"{"model": "this-checkout"}"#),
                None,
                1,
                false,
                false,
            ),
        ];
        for (layers, model, dropped, model_outranks, level_outranks) in &above {
            let settings = layers.read();
            let seen = settings.layers().collect::<Vec<_>>();
            assert_eq!(settings.model(), *model, "{seen:?}");
            assert_eq!(
                settings.model_outranks_a_pick(),
                *model_outranks,
                "{seen:?}"
            );
            assert_eq!(settings.model_ignored().count(), *dropped, "{seen:?}");
            assert_eq!(
                settings.effort_outranks_a_pick(),
                *level_outranks,
                "{seen:?}"
            );
        }
    }

    /// The person's own file is where the pick ranks, and the pick is the later of the two things
    /// they said there, so it stands. Naming that file with `--settings` does not move it up.
    #[test]
    fn the_home_layer_does_not_outrank_a_saved_pick() {
        let own = [
            Layers::new("pick-home")
                .global(r#"{"model": "personal", "effort": "low"}"#)
                .project(r#"{"env": {"AWS_PROFILE": "this-checkout"}}"#),
            Layers::new("pick-home-named")
                .global(r#"{"model": "personal", "effort": "low"}"#)
                .naming_the_home_layer(),
        ];
        for layers in &own {
            let settings = layers.read();
            assert_eq!(settings.model(), Some("personal"));
            assert!(
                !settings.model_outranks_a_pick(),
                "{:?}",
                settings.layers().collect::<Vec<_>>()
            );
            assert!(
                !settings.effort_outranks_a_pick(),
                "{:?}",
                settings.layers().collect::<Vec<_>>()
            );
        }
    }

    /// A blank value, or one that is not a word, names nothing, and a pick is not outranked by
    /// nothing. A blank `model` in a checkout is dropped with the rest of that layer's say in the
    /// key, leaving the home layer's own word standing.
    #[test]
    fn a_layer_above_that_names_nothing_does_not_outrank_a_saved_pick() {
        let settings = Layers::new("pick-blank")
            .global(r#"{"model": "personal", "effort": "low"}"#)
            .project(r#"{"model": "  ", "effort": 3}"#)
            .read();
        assert_eq!(settings.model(), Some("personal"));
        assert_eq!(settings.model_ignored().count(), 1);
        assert!(!settings.model_outranks_a_pick());
        assert!(!settings.effort_outranks_a_pick());
    }

    /// A layer that says nothing about the model leaves the one a weaker layer named, on the same
    /// footing as every other name.
    #[test]
    fn a_layer_naming_no_model_leaves_the_one_below_it() {
        let settings = Layers::new("model-survives")
            .global(r#"{"model": "personal-choice"}"#)
            .project(r#"{"env": {"AWS_PROFILE": "this-checkout"}}"#)
            .read();
        assert_eq!(settings.model(), Some("personal-choice"));
    }

    /// The reason to report an override at all: somebody seeing a value they did not set has three
    /// files it could be in, and only the winning path narrows it to one.
    #[test]
    fn a_name_more_than_one_layer_set_reports_the_file_that_won() {
        let layers = Layers::new("report-override")
            .global(r#"{"env": {"AWS_PROFILE": "personal", "AWS_REGION": "us-west-2"}}"#)
            .project(r#"{"env": {"AWS_PROFILE": "this-checkout"}}"#);
        let settings = layers.read();

        let reported: Vec<(&str, PathBuf)> = settings
            .overridden()
            .map(|(name, path)| (name, path.to_path_buf()))
            .collect();
        assert_eq!(
            reported,
            [(
                "AWS_PROFILE",
                layers.cwd.join(PROJECT_DIR).join(SETTINGS_FILE)
            )]
        );
    }

    /// A name only one layer set needs no explanation of where it came from, and listing every name
    /// against a path would bury the one that is surprising.
    #[test]
    fn a_name_a_single_layer_set_is_not_reported_as_overridden() {
        let settings = Layers::new("report-uncontested")
            .global(r#"{"env": {"AWS_REGION": "us-west-2"}}"#)
            .project(r#"{"env": {"AWS_PROFILE": "this-checkout"}}"#)
            .read();
        assert_eq!(settings.overridden().count(), 0);
    }

    /// `doctor` prints these, so an override must say which file won and never what it said: on some
    /// machines the value is a credential.
    #[test]
    fn an_override_reports_the_name_and_the_file_and_never_the_value() {
        let settings = Layers::new("report-override-values")
            .global(r#"{"env": {"AWS_PROFILE": "the-old-value"}}"#)
            .project(r#"{"env": {"AWS_PROFILE": "a-secret-looking-value"}}"#)
            .read();

        let reported = format!("{:?}", settings.overridden().collect::<Vec<_>>());
        assert!(reported.contains("AWS_PROFILE"));
        assert!(!reported.contains("a-secret-looking-value"));
        assert!(!reported.contains("the-old-value"));
    }

    #[test]
    fn a_keybindings_block_is_read_from_settings() {
        let settings =
            Settings::parse(r#"{"keybindings": {"stash": "alt-s", "scroller": "alt-o"}}"#);
        assert_eq!(
            settings.keybindings().get("stash"),
            Some(&"alt-s".to_string())
        );
        assert_eq!(
            settings.keybindings().get("scroller"),
            Some(&"alt-o".to_string())
        );
    }

    /// An entry that is not a string is dropped, the way a malformed permission rule is, and the
    /// block reads as though the file had not named that action.
    #[test]
    fn a_keybindings_entry_that_is_not_a_chord_is_dropped() {
        let settings = Settings::parse(
            r#"{"keybindings": {"stash": 7, "scroller": "", "trail": " ctrl-x ", "watch": "alt-l"}}"#,
        );
        assert_eq!(settings.keybindings().get("stash"), None);
        assert_eq!(settings.keybindings().get("scroller"), None);
        assert_eq!(
            settings.keybindings().get("trail"),
            Some(&"ctrl-x".to_string())
        );
        assert_eq!(
            settings.keybindings().get("watch"),
            Some(&"alt-l".to_string())
        );
    }

    #[test]
    fn a_project_layer_overrides_keybindings_per_name() {
        let settings = Layers::new("keybindings-override")
            .global(r#"{"keybindings": {"stash": "alt-s", "scroller": "alt-o"}}"#)
            .project(r#"{"keybindings": {"stash": "ctrl-x"}}"#)
            .read();
        assert_eq!(
            settings.keybindings().get("stash"),
            Some(&"ctrl-x".to_string())
        );
        assert_eq!(
            settings.keybindings().get("scroller"),
            Some(&"alt-o".to_string())
        );
    }

    #[test]
    fn a_local_layer_overrides_project_and_global_keybindings() {
        let settings = Layers::new("keybindings-local-override")
            .global(
                r#"{"keybindings": {"stash": "alt-s", "scroller": "alt-o", "editor": "alt-e"}}"#,
            )
            .project(r#"{"keybindings": {"stash": "ctrl-x", "scroller": "ctrl-u"}}"#)
            .local(r#"{"keybindings": {"stash": "ctrl-p"}}"#)
            .read();
        assert_eq!(
            settings.keybindings().get("stash"),
            Some(&"ctrl-p".to_string())
        );
        assert_eq!(
            settings.keybindings().get("scroller"),
            Some(&"ctrl-u".to_string())
        );
        assert_eq!(
            settings.keybindings().get("editor"),
            Some(&"alt-e".to_string())
        );
    }
}
