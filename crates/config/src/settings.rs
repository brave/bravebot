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
//! whoever wrote the checkout wrote it. Nothing here distinguishes them for the blocks above,
//! because the resolution this copies does not. What that costs is written down under Known costs in
//! `docs/specs/backends.md` rather than mitigated here.
//!
//! `vetting` is the exception, and it is the exception because of what it decides. Every other name
//! here configures where a request goes or how the interface behaves; that one says whether a person
//! is asked before content nobody vouched for reaches the planner, so a line in a checkout's file
//! could turn the asking off for whoever opened the checkout. It is read from the home layer alone,
//! and a project file naming it is reported rather than obeyed.
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

/// The most of it worth reading.
///
/// A settings file is a handful of short strings. Bounded so a file that grew by accident, or was
/// replaced by something else entirely, is refused rather than parsed.
const MAX_BYTES: u64 = 64 * 1024;

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
    keybindings: BTreeMap<String, String>,
    attribution: Attribution,
    search: SearchCaps,
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
}

impl PermissionLists {
    /// Whether the block said anything.
    pub fn is_empty(&self) -> bool {
        self.deny.is_empty()
            && self.ask.is_empty()
            && self.allow.is_empty()
            && self.additional_directories.is_empty()
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

impl Settings {
    /// Read every settings layer in force for this user, in this directory, plus the file the
    /// command line named.
    pub fn load() -> Self {
        Self::layered(
            home(),
            std::env::current_dir().ok().as_deref(),
            NAMED.get().map(PathBuf::as_path),
        )
    }

    /// As [`Settings::load`], for a named home, working directory and command line, so a test needs
    /// no ambient ones.
    ///
    /// The working directory is where the process started and not an ancestor of it. A session begun
    /// in a subdirectory therefore reads no project settings, which is the same rule Claude Code
    /// applies and is the reason this walks nothing: a search upward would make what configures a
    /// session depend on which directory somebody happened to `cd` into, and the file it eventually
    /// found could sit above the thing being worked on.
    ///
    /// `named` is read last, so what it sets beats every file that was found. Taken as an argument
    /// rather than read from [`NAMED`] here, so the order and the merge rules are checked without a
    /// process-wide switch in force under every other test in this binary.
    pub fn layered(home: Option<PathBuf>, cwd: Option<&Path>, named: Option<&Path>) -> Self {
        let project = cwd.map(|cwd| cwd.join(PROJECT_DIR));
        let home_layer = home.map(|home| home.join(SETTINGS_FILE));
        let paths = [
            home_layer.clone(),
            project.as_ref().map(|dir| dir.join(SETTINGS_FILE)),
            project.as_ref().map(|dir| dir.join(LOCAL_SETTINGS_FILE)),
            named.map(Path::to_path_buf),
        ];

        let mut merged = serde_json::Map::new();
        let mut found = Vec::new();
        let mut winner = BTreeMap::new();
        let mut contested = BTreeMap::new();
        // Settled per layer rather than off the merged root, because the merge cannot say which
        // file a name came from and this is the one name where that decides whether it is obeyed.
        let mut vetting = None;
        let mut vetting_ignored = Vec::new();
        for path in paths.into_iter().flatten() {
            // A file already read as a layer above is not read again. Naming one of the three
            // explicitly is an ordinary thing to do, and reading it twice would report every name
            // in it as an override of itself, list it twice among the layers, and double every
            // entry in a list that unions rather than overrides.
            if found.contains(&path) {
                continue;
            }
            let Some(root) = read(&path) else { continue };
            if root.contains_key(VETTING_BLOCK) {
                match Some(&path) == home_layer.as_ref() {
                    true => vetting = auto_vetting(&root),
                    false => vetting_ignored.push(path.clone()),
                }
            }
            for name in env_names(&root) {
                // Whoever set it before lost it here, which is the only thing worth telling somebody:
                // a name one file sets needs no explanation of where it came from.
                if winner.insert(name.clone(), path.clone()).is_some() {
                    contested.insert(name, path.clone());
                }
            }
            found.push(path);
            merge(&mut merged, root);
        }

        let mut settings = Self::from_map(&merged);
        settings.layers = found;
        settings.contested = contested;
        // Overwritten rather than merged in, so that the only value here is the home layer's own.
        // A project file that set it has already been recorded as ignored above, and what it said
        // cannot reach this even where the home layer said nothing.
        settings.vetting = vetting;
        settings.vetting_ignored = vetting_ignored;
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
        let Ok(serde_json::Value::Object(root)) = serde_json::from_str(text) else {
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
            editor_mode: word(root, "editorMode"),
            // Read here so one file's worth can be parsed on its own, and overwritten by
            // [`Settings::layered`], which is the only caller that knows which layer this came
            // from and so the only one entitled to answer.
            vetting: auto_vetting(root),
            vetting_ignored: Vec::new(),
            keybindings: keybindings_block(root),
            attribution: attribution_block(root),
            search: search_caps(root),
            providers: crate::provider::Provider::all(root),
            layers: Vec::new(),
            contested: BTreeMap::new(),
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
    /// A default rather than the model: `/model` records a choice that outlives the session making
    /// it, and that choice wins. This is what answers for somebody who has never made one.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
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

    /// Whether anything was set at all.
    pub fn is_empty(&self) -> bool {
        self.env.is_empty()
            && self.scrub.is_empty()
            && self.permissions.is_empty()
            && self.model.is_none()
            && self.editor_mode.is_none()
            && self.vetting.is_none()
            && self.keybindings.is_empty()
            && self.attribution.is_empty()
            && self.search.is_empty()
            && self.providers.is_empty()
            // A file that named `vetting.auto` and was not obeyed still said something, and
            // `doctor` reports both facts about it. Reading it as absence would print "no
            // settings.json" one line above the path of the file that holds it.
            && self.vetting_ignored.is_empty()
    }

    /// The rule text and added directories the `permissions` block carried.
    pub fn permissions(&self) -> &PermissionLists {
        &self.permissions
    }

    /// The gateways these settings configured, in the order they were listed.
    pub fn providers(&self) -> &[crate::provider::Provider] {
        &self.providers
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
            .chain(self.editor_mode.is_some().then_some("editorMode"))
            .chain(self.vetting.is_some().then_some("vetting.auto"))
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
            .chain(self.env.keys().map(String::as_str))
    }
}

/// One layer's JSON, or `None` when there is nothing there worth reading.
///
/// Every failure is the same as absence, per layer rather than for the set: a missing file, an
/// oversized one, a syntax error, or a root that is not an object. A half-typed project file leaves
/// the layers under it in force, because the alternative is a mistake in a checkout deciding that a
/// person's own profile no longer applies.
pub(crate) fn read(path: &Path) -> Option<serde_json::Map<String, serde_json::Value>> {
    match std::fs::metadata(path) {
        Ok(found) if found.len() > MAX_BYTES => return None,
        Ok(_) => {}
        Err(_) => return None,
    }
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(serde_json::Value::Object(root)) => Some(root),
        _ => None,
    }
}

/// A top-level key holding one word, or `None` where the file said nothing usable.
///
/// Strings only, on the footing everything else here reads them: a number or a boolean where a word
/// belongs would have to be given a spelling nobody chose. Blank is absence rather than a choice of
/// nothing, since a key set to `""` is how somebody comments one out without deleting the line.
fn word(root: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
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

/// Lay one settings root over another, a name at a time.
///
/// One level deep, which is Claude Code's rule rather than a general merge: `env`, `provider` and
/// `attribution` combine per name, and a value inside one of those names is replaced whole. So a
/// project file may add a gateway or restate one, and cannot reach inside an inherited gateway to
/// change the host it points at while keeping the rest. A deeper merge would make a request's
/// destination the product of two files, and no single place to read would say where it goes.
///
/// `run.scrubEnv` unions instead, since a name there only ever takes a variable away from a
/// subprocess. Overriding would let a layer hand back something a weaker one withheld, which is a
/// direction this list is not for.
fn merge(
    base: &mut serde_json::Map<String, serde_json::Value>,
    over: serde_json::Map<String, serde_json::Value>,
) {
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
                under.extend(above);
            }
            // `run` holds one list that unions and nothing else that does, so a sibling added later
            // gets whatever this arm does by default, which is to override.
            (Some(serde_json::Value::Object(under)), serde_json::Value::Object(above))
                if key == "run" =>
            {
                merge_run(under, above);
            }
            // Every rule from every layer, which is the rule the tool this borrows from applies. A
            // layer that replaced the block could drop a `deny` a weaker one set, and a permission
            // taken away by a file somebody did not open is the one outcome worth ruling out.
            (Some(serde_json::Value::Object(under)), serde_json::Value::Object(above))
                if key == "permissions" =>
            {
                merge_permissions(under, above);
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// The `permissions` block, where every list unions and any other name overrides.
///
/// A rule is added by a layer and never removed by one, so `deny` still holds whatever the weakest
/// file said. `additionalDirectories` unions for the same reason it exists: a layer asks for
/// somewhere to work, and the strongest file asking for one place should not un-ask another.
fn merge_permissions(
    under: &mut serde_json::Map<String, serde_json::Value>,
    above: serde_json::Map<String, serde_json::Value>,
) {
    for (key, value) in above {
        match (under.get_mut(&key), value) {
            (Some(serde_json::Value::Array(kept)), serde_json::Value::Array(added)) => {
                kept.extend(added);
            }
            (_, value) => {
                under.insert(key, value);
            }
        }
    }
}

/// The `run` block, where `scrubEnv` unions and every other name overrides.
fn merge_run(
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
            (_, value) => {
                under.insert(key, value);
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

/// The `permissions` block: three lists of rule text, and the directories to open.
///
/// Strings only, and a malformed entry is dropped rather than refused, on the same footing as
/// everything else here. A rule that is not a string cannot be matched against anything, and
/// refusing the file over one would take away the rules that were readable.
///
/// `defaultMode` is read by nothing yet. A file setting it is not an error and not a warning here:
/// [`Settings::parse`] reads what the file says and reports it, and which modes exist is a
/// question for whoever consults them.
fn permission_lists(root: &serde_json::Map<String, serde_json::Value>) -> PermissionLists {
    let Some(serde_json::Value::Object(block)) = root.get("permissions") else {
        return PermissionLists::default();
    };
    PermissionLists {
        deny: strings(block, "deny"),
        ask: strings(block, "ask"),
        allow: strings(block, "allow"),
        additional_directories: strings(block, "additionalDirectories"),
    }
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
const PROFILE_VARIABLES: &[&str] = &["HOME", "USERPROFILE"];
#[cfg(not(windows))]
const PROFILE_VARIABLES: &[&str] = &["HOME"];

/// The global state directory, or `None` when the platform names no profile directory to look in.
///
/// No fallback to a relative `.bravebot`, which is the project layer and reached deliberately rather
/// than by a home directory going missing. Resolving the weakest layer to the strongest one's
/// location would silently read a checkout's file as though a person had put it in their own
/// directory.
fn home() -> Option<PathBuf> {
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

    /// A rule is a string. An entry that is not one cannot be matched against anything, and
    /// dropping it keeps the rules that were readable rather than losing the file over one.
    #[test]
    fn an_entry_that_is_not_a_rule_is_left_out() {
        let settings = Settings::parse(
            r#"{"permissions": {"deny": ["Read(./.env)", 1, true, null, "", "  ", []]}}"#,
        );
        assert_eq!(settings.permissions().deny, ["Read(./.env)"]);
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

    /// A gateway is replaced by name, and the others stay. Merging deeper would make one request's
    /// destination the product of two files, with no single place to read that says where it goes.
    #[test]
    fn a_project_layer_replaces_one_gateway_and_leaves_the_others() {
        let settings = Layers::new("provider-by-id")
            .global(
                r#"{"provider": {
                    "personal": {"options": {"baseURL": "https://personal.invalid/v1"}},
                    "shared": {"options": {"baseURL": "https://shared.invalid/v1"}}
                }}"#,
            )
            .project(
                r#"{"provider": {
                    "shared": {"options": {"baseURL": "https://this-checkout.invalid/v1"}}
                }}"#,
            )
            .read();

        let mut hosts: Vec<(&str, &str)> = settings
            .providers()
            .iter()
            .map(|provider| (provider.id.as_str(), provider.base_url.as_str()))
            .collect();
        hosts.sort_unstable();
        assert_eq!(
            hosts,
            [
                ("personal", "https://personal.invalid/v1"),
                ("shared", "https://this-checkout.invalid/v1"),
            ]
        );
    }

    /// A gateway entry is replaced whole, so a project file naming one has to name the host too. A
    /// deeper merge would leave an entry no single file describes.
    #[test]
    fn a_project_gateway_naming_no_host_replaces_one_that_did() {
        let settings = Layers::new("provider-whole")
            .global(
                r#"{"provider": {"gw": {"options": {"baseURL": "https://personal.invalid/v1"}}}}"#,
            )
            .project(r#"{"provider": {"gw": {"models": {"some-model": {}}}}}"#)
            .read();
        assert!(settings.providers().is_empty());
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
                .project(&format!(r#"{{"env": {spelling}, "model": "opus"}}"#))
                .read();
            assert_eq!(settings.get("AWS_PROFILE"), None, "{spelling}");
            assert_eq!(settings.get("AWS_REGION"), None, "{spelling}");
            assert_eq!(settings.model(), Some("opus"), "{spelling}");
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

    /// A model is one choice rather than a list, so the closest layer that names one wins: a checkout
    /// saying which model its work wants is the whole point of naming it there.
    #[test]
    fn the_closest_layer_that_named_a_model_wins() {
        let settings = Layers::new("model-override")
            .global(r#"{"model": "personal-choice"}"#)
            .project(r#"{"model": "this-checkout"}"#)
            .read();
        assert_eq!(settings.model(), Some("this-checkout"));
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
