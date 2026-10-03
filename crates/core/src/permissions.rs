//! Rules a person wrote down in advance about which actions to ask them about.
//!
//! The same three lists Claude Code keeps, with the same spellings, so a `permissions` block
//! copied out of `~/.claude/settings.json` governs this agent unedited. A rule is `Tool` or
//! `Tool(specifier)`, the lists are consulted deny, then ask, then allow, and the first match in
//! that order decides. See `docs/specs/permissions.md`.
//!
//! # What a rule may be about
//!
//! A specifier matches a **routing** field and nothing else: a path, or the argv of a stage.
//! Routing is `(T,pub)` before it reaches any gate, so matching on it is the driver deciding from
//! trusted input, which is what it is for. Nothing here is ever handed a file's contents, a
//! program's output, or anything else a turn observed. A rule that matched on those would be the
//! driver branching on untrusted bytes, whatever the rule said.
//!
//! # What an allow rule grants, and what it deliberately does not
//!
//! It answers a prompt, and that is all. It does **not** make a command's output trusted.
//! Pressing `a` at a run prompt grants those two things together, because a person looking at one
//! command can be asked to answer for both; a pattern like `curl *` covers commands nobody has
//! read, so it cannot carry the second claim. An allow rule that trusted output would let one
//! line in a settings file turn fetched bytes into routing, which is the whole thing the labels
//! exist to stop. So output keeps the label it would have had, and a rule only stops the asking.
//!
//! For the same reason an allow rule never answers the confidentiality question: a run that
//! releases the user's private data asks whatever the rules say, because vouching for a command
//! is not consenting to hand it that data.
//!
//! # Nothing here widens reach
//!
//! Rules decide what is asked about, never what is reachable. A path outside the workspace and
//! the directories the user opened is refused because it is out of reach, and no allow rule brings
//! it back: `additionalDirectories` is a separate statement, and one that asks for a directory
//! rather than opening it.

use crate::trust::is_absolute_key;
use std::fmt;

/// The family of tools a rule names.
///
/// Claude Code's own spellings, which are categories rather than tool names there too: its docs
/// have `Edit(...)` covering every tool that edits a file and reject a path rule written for
/// `Write` or `Glob`. So these are not this agent's tool names, and `Bash` in particular names no
/// shell: the planner has none, and a rule matches the argv of a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Subject {
    /// Every tool that reads a file or lists one: `read_file`, `list_files`, `search`.
    Read,
    /// Every tool that changes a file: `write_file`, `edit_file`.
    Edit,
    /// Running a program: every stage of a `run` pipeline.
    Bash,
    /// Fetching a URL: `fetch_url`, and every redirect hop it follows.
    WebFetch,
    /// Calling a tool of an MCP server a person declared, named `alias:tool`.
    Mcp,
}

impl Subject {
    /// The spelling accepted in a settings file.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "Read",
            Self::Edit => "Edit",
            Self::Bash => "Bash",
            Self::WebFetch => "WebFetch",
            Self::Mcp => "Mcp",
        }
    }

    /// The name in a rule, or `None` for anything this agent has no family for.
    fn parse(name: &str) -> Option<Self> {
        match name {
            "Read" => Some(Self::Read),
            "Edit" => Some(Self::Edit),
            "Bash" => Some(Self::Bash),
            "WebFetch" => Some(Self::WebFetch),
            "Mcp" => Some(Self::Mcp),
            _ => None,
        }
    }

    /// Whether a specifier for this family is a path pattern rather than a command pattern.
    fn takes_a_path(self) -> bool {
        matches!(self, Self::Read | Self::Edit)
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which list a rule came from, which is what it does when it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ruling {
    /// The action is refused outright.
    Deny,
    /// A person is asked, whatever else would have answered.
    Ask,
    /// No prompt.
    Allow,
}

impl Ruling {
    /// The list's own name, for the audit trail.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deny => "deny",
            Self::Ask => "ask",
            Self::Allow => "allow",
        }
    }
}

impl fmt::Display for Ruling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the rules had to say about one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// A rule matched.
    Ruled(Ruling),
    /// Nothing matched, so the rules decide nothing and the ordinary gates do.
    Unmatched,
}

/// Where a relative pattern in a rule is anchored.
///
/// Supplied by the caller because resolving `~` and the directory a settings file sits in is I/O,
/// and this crate does none. Absent entries make the patterns that need them match nothing, which
/// is what a machine with no home directory should get.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Anchors {
    /// The user's home directory, for a `~/` pattern.
    pub home: Option<String>,
    /// The directory the settings file sits in, for a single-slash pattern.
    pub settings_dir: Option<String>,
    /// Whether the host separates one segment of a path from the next with a backslash as well as
    /// with a slash. Supplied by the caller for the same reason the two directories are: this
    /// crate asks the host nothing, and the answer cannot be read off a pattern or a path, since a
    /// backslash is a legal filename byte where a slash is the only separator
    /// ([`crate::spelling`]).
    pub backslash_separates: bool,
    /// Whether the volume the paths live on holds two spellings that differ only in case as one
    /// file. Supplied by the caller because this crate asks the host nothing, and false when the
    /// caller cannot tell: folding reads a rule about `Docs` as also covering `docs`, which on a
    /// volume that keeps them apart is a different file.
    pub folds_case: bool,
}

impl Anchors {
    /// Anchors that resolve nothing, for a caller with no settings file.
    pub fn none() -> Self {
        Self::default()
    }
}

/// What a specifier matches against.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    /// Every use of the family, from a bare `Bash` or a `Bash(*)`.
    Everything,
    /// A path pattern, gitignore-shaped, against a workspace-relative path.
    Relative(PathPattern),
    /// A path pattern against an absolute path.
    Absolute(PathPattern),
    /// A command pattern, matched against one stage's argv.
    Command(String),
    /// A host, from a `domain:` specifier, matched against a URL's host and its subdomains.
    Domain(String),
    /// Every tool of one server, from `Mcp(weather)` or `Mcp(weather:*)`.
    Server(String),
    /// One tool of one server, from `Mcp(weather:get_forecast)`.
    Tool(String, String),
}

/// A path pattern and whether it was written anchored.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PathPattern {
    /// Segments, `**` and `*` intact.
    segments: Vec<String>,
    /// Whether the pattern names a single leading segment and so may float in a deny or ask rule.
    ///
    /// Claude Code's asymmetry: `Read(secrets/**)` as a deny rule catches a `secrets` directory at
    /// any depth, and the same pattern as an allow rule grants only the one at the top. A rule
    /// that restricts should cover the nested copy; a rule that grants should cover what it names.
    floats_when_restricting: bool,
}

/// One rule: a family, and what of it the rule is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    subject: Subject,
    pattern: Pattern,
    /// The path pattern again under each other name its literal prefix reaches once the links in
    /// it are followed ([`Permissions::follow_links`]). Part of this rule rather than rules of
    /// their own, since they are one rule about one file and a report counts what was written.
    landed: Vec<Pattern>,
}

impl Rule {
    /// Read one rule, or say why it was not one.
    ///
    /// A rule nobody can act on is dropped rather than guessed at, and the text says which it was
    /// so `doctor` can name it. Guessing would be worse than ignoring: a misread deny rule reads
    /// as protection that is not there.
    pub fn parse(text: &str, anchors: &Anchors) -> Result<Self, Rejected> {
        // A rejection names the entry in the spelling the file used, not the trimmed text that was
        // read. Two lines that differ only in surrounding space are two entries somebody has to
        // find in their file, and a report that trimmed them names neither.
        Self::parse_trimmed(text.trim(), anchors).map_err(|rejected| Rejected {
            text: text.to_string(),
            ..rejected
        })
    }

    /// [`Rule::parse`] on text with its surrounding space already removed.
    fn parse_trimmed(text: &str, anchors: &Anchors) -> Result<Self, Rejected> {
        if text.is_empty() {
            return Err(Rejected::new(text, Unreadable::Empty));
        }

        let (name, specifier) = match text.split_once('(') {
            None => (text, None),
            Some((name, rest)) => match rest.strip_suffix(')') {
                None => return Err(Rejected::new(text, Unreadable::UnclosedBracket)),
                Some(specifier) => (name.trim_end(), Some(specifier.trim())),
            },
        };

        let Some(subject) = Subject::parse(name) else {
            return Err(Rejected::new(text, Unreadable::UnknownFamily));
        };

        let pattern = match specifier {
            // A bare family name, and `(*)`, are the same rule and cover every use of it.
            None | Some("*") => Pattern::Everything,
            Some("") => {
                return Err(Rejected::new(text, Unreadable::EmptyBrackets));
            }
            Some(specifier) if subject.takes_a_path() => path_pattern(specifier, anchors)
                .ok_or_else(|| Rejected::new(text, Unreadable::Unanchored))?,
            Some(specifier) if subject == Subject::WebFetch => {
                // Claude Code's spelling, and the only one: a URL prefix would read as covering a
                // path, and a rule about a path on a host it does not also pin is not a rule
                // anybody could rely on.
                let Some(domain) = specifier.strip_prefix("domain:") else {
                    return Err(Rejected::new(text, Unreadable::NotADomainRule));
                };
                let domain = domain.trim().trim_start_matches('.').to_ascii_lowercase();
                if domain.is_empty() {
                    return Err(Rejected::new(text, Unreadable::NoDomainNamed));
                }
                Pattern::Domain(domain)
            }
            Some(specifier) if subject == Subject::Mcp => tool_pattern(specifier)
                .ok_or_else(|| Rejected::new(text, Unreadable::NotAToolRule))?,
            Some(specifier) => Pattern::Command(command_pattern(specifier)),
        };

        Ok(Self {
            subject,
            pattern,
            landed: Vec::new(),
        })
    }

    pub fn subject(&self) -> Subject {
        self.subject
    }

    /// Add the spelling this rule's literal prefix lands on, where `land` names one that differs.
    ///
    /// The literal prefix is every segment before the first one holding a `*`, keyed as a gate holds
    /// a path: relative for a pattern about the workspace and in full for any other. A leading `**`
    /// also matches no segment, so the segments after it name a place at the anchor too: a bare
    /// name, read as `**/name`, is followed at the top of the workspace. A pattern that starts with
    /// any other star names no place. A prefix holding `..` is left as written, as a path holding one
    /// is (TRUST-10).
    ///
    /// The spelling added is rooted where it landed and never floats: the name a link reaches is one
    /// place. Only the place the prefix names is followed, not the nested copies PERM-4 floats a
    /// written pattern to, since finding those is a walk of the tree.
    fn follow_links(
        &mut self,
        land: &impl Fn(&str) -> Option<String>,
        backslash_separates: bool,
        folds_case: bool,
    ) {
        let (pattern, absolute) = match &self.pattern {
            Pattern::Relative(pattern) => (pattern, false),
            Pattern::Absolute(pattern) => (pattern, true),
            Pattern::Everything
            | Pattern::Command(_)
            | Pattern::Domain(_)
            | Pattern::Server(_)
            | Pattern::Tool(..) => return,
        };
        let from = usize::from(pattern.segments.first().is_some_and(|first| first == "**"));
        let literal = from
            + pattern.segments[from..]
                .iter()
                .take_while(|segment| !segment.contains('*'))
                .count();
        let prefix = &pattern.segments[from..literal];
        if prefix.is_empty() || prefix.iter().any(|segment| segment == "..") {
            return;
        }
        let key = match absolute {
            true => format!("/{}", prefix.join("/")),
            false => prefix.join("/"),
        };
        let Some(landing) = land(&key) else {
            return;
        };
        let landing = key_of(&landing, backslash_separates, folds_case);
        let lands_absolute = is_absolute_key(&landing);
        let mut segments = split(&landing);
        if lands_absolute == absolute && segments == prefix {
            return;
        }
        segments.extend_from_slice(&pattern.segments[literal..]);
        let respelled = PathPattern {
            segments,
            floats_when_restricting: false,
        };
        let respelled = match lands_absolute {
            true => Pattern::Absolute(respelled),
            false => Pattern::Relative(respelled),
        };
        if !self.landed.contains(&respelled) {
            self.landed.push(respelled);
        }
    }

    /// Whether this rule covers reading or editing `path`.
    ///
    /// `path` is workspace-relative or absolute, as the gates hold it. `restricting` selects the
    /// depth a single-segment pattern is matched at, which differs between the lists.
    ///
    /// `path` arrives already folded where the volume folds case, and the pattern was stored folded
    /// the same way, so the comparison here is byte-exact either way.
    fn covers_path(&self, path: &str, restricting: bool) -> bool {
        let absolute = is_absolute_key(path);
        let segments = segments_of(path);
        std::iter::once(&self.pattern)
            .chain(&self.landed)
            .any(|pattern| match pattern {
                Pattern::Everything => true,
                Pattern::Command(_)
                | Pattern::Domain(_)
                | Pattern::Server(_)
                | Pattern::Tool(..) => false,
                Pattern::Relative(pattern) => !absolute && pattern.matches(&segments, restricting),
                Pattern::Absolute(pattern) => absolute && pattern.matches(&segments, restricting),
            })
    }

    /// Whether this rule covers running one stage, `argv` its program word and arguments.
    ///
    /// `restricting` selects what a space in the pattern covers, which differs between the lists.
    fn covers_command(&self, argv: &[Place], restricting: bool) -> bool {
        match &self.pattern {
            Pattern::Everything => true,
            Pattern::Command(pattern) => command_matches(pattern, argv, restricting),
            Pattern::Relative(_)
            | Pattern::Absolute(_)
            | Pattern::Domain(_)
            | Pattern::Server(_)
            | Pattern::Tool(..) => false,
        }
    }

    /// Whether this rule covers fetching from `host`, which is already lowercased.
    ///
    /// A rule for `example.com` covers `docs.example.com`, which is what a person writing one
    /// means, and never `notexample.com`: the boundary has to be a label boundary or a rule about
    /// one site would silently cover somebody else's.
    fn covers_host(&self, host: &str) -> bool {
        match &self.pattern {
            Pattern::Everything => true,
            Pattern::Domain(domain) => {
                host == domain
                    || host
                        .strip_suffix(domain)
                        .is_some_and(|prefix| prefix.ends_with('.'))
            }
            Pattern::Relative(_)
            | Pattern::Absolute(_)
            | Pattern::Command(_)
            | Pattern::Server(_)
            | Pattern::Tool(..) => false,
        }
    }

    /// Whether this rule covers calling `tool` of the server declared as `alias`.
    ///
    /// Both names compared whole and as written: an alias is a name a person typed, and a rule
    /// about `weather` that also covered `weather2` would be a rule about a server nobody named.
    fn covers_tool(&self, alias: &str, tool: &str) -> bool {
        match &self.pattern {
            Pattern::Everything => true,
            Pattern::Server(server) => server == alias,
            Pattern::Tool(server, word) => server == alias && word == tool,
            Pattern::Relative(_)
            | Pattern::Absolute(_)
            | Pattern::Command(_)
            | Pattern::Domain(_) => false,
        }
    }
}

/// A rule that was not one, and what was wrong with it.
///
/// Carries the text so a report can name it. The text came from the user's own settings file, so
/// there is nothing untrusted in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejected {
    pub text: String,
    pub reason: Unreadable,
    /// The settings file the entry was written in, where whoever read it knew.
    pub file: Option<String>,
}

/// What was wrong with an entry of a permissions list, for whoever is going to say so.
///
/// Named rather than worded, because this is read by a person and what a person reads comes from a
/// catalog (LOCALE-1), which this crate has none of and prints nothing through. A front end turns
/// each of these into a sentence in the reader's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreadable {
    /// Not a line of text at all: a number, or a rule nested one array too deep.
    NotALine,
    /// A line with nothing on it.
    Empty,
    /// An opening bracket and no closing one.
    UnclosedBracket,
    /// A family of tools this build does not have.
    UnknownFamily,
    /// `Read()`, which says nothing that `Read` alone does not.
    EmptyBrackets,
    /// A path rule in a run with no home or settings directory to resolve it against.
    Unanchored,
    /// A `WebFetch` rule whose specifier does not begin `domain:`.
    NotADomainRule,
    /// `WebFetch(domain:)`, which names no host.
    NoDomainNamed,
    /// An `Mcp` rule whose specifier is not `alias` or `alias:tool`.
    NotAToolRule,
    /// A value where a list of rules belongs, or the `permissions` block holding them, that is
    /// neither: `null`, a string, a list in place of the block.
    NotAList,
}

impl Rejected {
    fn new(text: &str, reason: Unreadable) -> Self {
        Self {
            text: text.to_string(),
            reason,
            file: None,
        }
    }

    /// An entry of a permissions list that was never a line, so [`Rule::parse`] never saw it.
    ///
    /// A rule is text, and a settings file is JSON, so an entry can be a number, a boolean, or a
    /// rule nested one array too deep, which is the ordinary way this key is mistyped. Whoever
    /// read the file has the spelling it used and hands it here, because the reason belongs to the
    /// rule language rather than to the reader, and because a rule that went missing between the
    /// file and the parser has to arrive in the same report as one the parser refused (PERM-11).
    pub fn not_a_line(text: &str) -> Self {
        Self::new(text, Unreadable::NotALine)
    }

    /// A list of rules, or the block that holds them, spelled as some other shape in `file`.
    ///
    /// Named with its file, unlike an entry inside a list: every layer can spell the same block,
    /// and the value alone does not say which of up to four files to open.
    pub fn not_a_list(text: &str, file: &str) -> Self {
        Self {
            file: Some(file.to_string()),
            ..Self::new(text, Unreadable::NotAList)
        }
    }
}

/// The three lists, and the directories a settings file made reachable.
///
/// Empty is the state every session starts in and means the rules decide nothing at all: every
/// gate behaves exactly as it did before a settings file existed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Permissions {
    deny: Vec<Rule>,
    ask: Vec<Rule>,
    allow: Vec<Rule>,
    /// The host's answer, kept from the anchors the rules were read with, because a path arrives
    /// at [`Permissions::for_path`] long after the file was parsed and has to be spelled the way
    /// the patterns were. Defaults to a slash being the only separator, which is what a caller
    /// that never named a host gets.
    backslash_separates: bool,
    /// The volume's answer, kept for the same reason: a path arrives folded or not according to
    /// the volume the rules were read for, and the patterns were stored the way it says.
    folds_case: bool,
}

impl Permissions {
    /// No rules.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read three lists of rule text, keeping what parsed and reporting what did not.
    ///
    /// A file with one unreadable rule still gets the rest. Refusing the lot would mean a typo in
    /// an allow rule took away a deny rule's protection, which is the wrong way round.
    pub fn parse(
        deny: &[String],
        ask: &[String],
        allow: &[String],
        anchors: &Anchors,
    ) -> (Self, Vec<Rejected>) {
        let mut rejected = Vec::new();
        let mut read = |texts: &[String]| -> Vec<Rule> {
            texts
                .iter()
                .filter_map(|text| match Rule::parse(text, anchors) {
                    Ok(rule) => Some(rule),
                    Err(problem) => {
                        rejected.push(problem);
                        None
                    }
                })
                .collect()
        };
        let permissions = Self {
            deny: read(deny),
            ask: read(ask),
            allow: read(allow),
            backslash_separates: anchors.backslash_separates,
            folds_case: anchors.folds_case,
        };
        (permissions, rejected)
    }

    /// Whether any rule was written at all.
    pub fn is_empty(&self) -> bool {
        self.deny.is_empty() && self.ask.is_empty() && self.allow.is_empty()
    }

    /// How many rules are in force, for a report.
    pub fn len(&self) -> usize {
        self.deny.len() + self.ask.len() + self.allow.len()
    }

    /// Have every `deny` and `ask` path rule cover the name its literal prefix reaches as well as
    /// the name it was written with (PERM-7).
    ///
    /// `land` is given that prefix as a gate holds a path, relative for a rule about the workspace
    /// and in full for any other, and answers with the name a gate would hold the place it reaches
    /// by once every link on the way is followed, or `None` where no link is on the way or where
    /// that cannot be told. Asked of the
    /// caller because following a link is I/O and this crate does none.
    ///
    /// `allow` rules are left as written, since a grant covers only the spelling approved (PERM-7).
    pub fn follow_links(&mut self, land: impl Fn(&str) -> Option<String>) {
        let (backslash_separates, folds_case) = (self.backslash_separates, self.folds_case);
        for rule in self.deny.iter_mut().chain(self.ask.iter_mut()) {
            rule.follow_links(&land, backslash_separates, folds_case);
        }
    }

    /// What the rules say about reading or editing `path`.
    ///
    /// Spelled from `/` before anything is matched against it, which is how the patterns were read,
    /// and a name rooted at a drive letter is keyed the way the trust map keys it, so it reads as
    /// the full path it is (PERM-3), on its drive whichever case the letter was written in. Here
    /// rather than at each gate, so a path reaches the rules one way whichever gate it came through
    /// and a gate added later cannot be the one that forgot.
    pub fn for_path(&self, subject: Subject, path: &str) -> Decision {
        let path = key_of(path, self.backslash_separates, self.folds_case);
        self.decide(|rule, restricting| {
            rule.subject == subject && rule.covers_path(&path, restricting)
        })
    }

    /// What the rules say about running one stage: its program word, then its arguments, each one
    /// word as the compiler split them.
    ///
    /// The words and not a line, because a line loses where one word ends. `"ls /x"`, one program
    /// word naming a script at `ls /x`, would read as `ls` given `/x` (PERM-5).
    pub fn for_command<S: AsRef<str>>(&self, argv: &[S]) -> Decision {
        let argv = places(argv);
        self.decide(|rule, restricting| {
            rule.subject == Subject::Bash && rule.covers_command(&argv, restricting)
        })
    }

    /// What the rules say about fetching from `host`.
    ///
    /// The host alone, never the path or the query: those are where a URL carries what somebody
    /// asked for, and a rule matching on them would be answering a different question each time.
    pub fn for_host(&self, host: &str) -> Decision {
        // A URL may spell a fully-qualified name with a trailing dot; that dot is spelling, not a
        // label, and a rule about `example.com` is about the same host either way.
        let host = host.to_ascii_lowercase();
        let host = host.strip_suffix('.').unwrap_or(&host);
        self.decide(|rule, _| rule.subject == Subject::WebFetch && rule.covers_host(host))
    }

    /// What the rules say about calling `tool` of the server declared as `alias`.
    ///
    /// The two names and never the arguments: the names are what a person typed and what a
    /// person was shown in a list they vouched for, and the arguments are what the planner wrote,
    /// in part from what it read, so a rule matching them would be the driver branching on
    /// content. SERVERS-7.
    pub fn for_mcp(&self, alias: &str, tool: &str) -> Decision {
        self.decide(|rule, _| rule.subject == Subject::Mcp && rule.covers_tool(alias, tool))
    }

    /// What the rules say about running a whole pipeline.
    ///
    /// Each stage is judged on its own and the strictest answer wins, which is the same rule
    /// Claude Code applies to a command joined by `&&` or `|`: a rule must match every part for
    /// the whole to be allowed, and matching any part is enough to restrict it. A pipeline is that
    /// shape by construction here, so there is no command string to split and no chance of
    /// splitting it differently from the shell.
    ///
    /// An empty pipeline is unmatched: there is nothing to have an opinion about.
    pub fn for_pipeline<S: AsRef<str>>(&self, stages: &[Vec<S>]) -> Decision {
        if stages.is_empty() {
            return Decision::Unmatched;
        }
        let each: Vec<Decision> = stages.iter().map(|argv| self.for_command(argv)).collect();

        // Restricting any stage restricts the pipeline: an unwanted program in the middle is
        // still an unwanted program.
        for ruling in [Ruling::Deny, Ruling::Ask] {
            if each.contains(&Decision::Ruled(ruling)) {
                return Decision::Ruled(ruling);
            }
        }
        // Granting needs every stage granted. One stage nobody wrote a rule for is a program
        // nobody has answered for, and it is what the next stage reads.
        if each.iter().all(|d| *d == Decision::Ruled(Ruling::Allow)) {
            return Decision::Ruled(Ruling::Allow);
        }
        Decision::Unmatched
    }

    /// Deny, then ask, then allow, first match wins.
    ///
    /// The order is the whole of the precedence, and specificity does not enter into it: a broad
    /// deny beats a narrow allow, so a deny rule cannot carry exceptions. That is what makes a
    /// deny rule readable as a statement about what will not happen.
    fn decide(&self, matches: impl Fn(&Rule, bool) -> bool) -> Decision {
        for (rules, ruling, restricting) in [
            (&self.deny, Ruling::Deny, true),
            (&self.ask, Ruling::Ask, true),
            (&self.allow, Ruling::Allow, false),
        ] {
            if rules.iter().any(|rule| matches(rule, restricting)) {
                return Decision::Ruled(ruling);
            }
        }
        Decision::Unmatched
    }
}

/// Read an `Mcp` specifier: `alias` for every tool of a server, `alias:tool` for one of them.
///
/// Each half is held to what an alias and a tool word may be, letters, digits, `-` and `_`, with
/// an alias starting on a letter or a digit. Anything else names nothing that could ever be
/// called, `weather:get_*` included, since the names are matched whole: kept, a deny rule like that
/// would read as protection that is not there, so it is refused where it is read.
fn tool_pattern(specifier: &str) -> Option<Pattern> {
    let word = |text: &str| {
        !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    };
    let alias = |text: &str| word(text) && text.starts_with(|c: char| c.is_ascii_alphanumeric());
    match specifier.split_once(':') {
        None => alias(specifier).then(|| Pattern::Server(specifier.to_string())),
        Some((server, "*")) => alias(server).then(|| Pattern::Server(server.to_string())),
        Some((server, tool)) => (alias(server) && word(tool))
            .then(|| Pattern::Tool(server.to_string(), tool.to_string())),
    }
}

/// Read a path specifier, resolving the anchor it was written with.
///
/// The four shapes Claude Code has, which differ only in where they start from:
/// `//x` the filesystem root, `~/x` the home directory, `/x` the directory the settings file sits
/// in, and `x` or `./x` the workspace. Where a backslash separates there is a fifth, `D:\x`, a full
/// path spelled the way the host spells one.
///
/// Spelled from `/` first, on the same terms as the path it will be matched against: somebody
/// writing a rule on a host that separates with a backslash writes the separator their own shell
/// and their own file manager use, and a pattern kept as they wrote it would be one segment while
/// the path it names is several. The anchors are read after the respelling, so `~\x` anchors where
/// `~/x` does. A path specifier alone: a command pattern is matched against argv, where a
/// backslash is an argument's own byte on every host.
///
/// A pattern is stored folded where the volume folds case, since the path arrives folded at match
/// time and both sides have to be in the one spelling.
///
/// A name rooted at a drive letter is keyed the way [`Permissions::for_path`] keys the path, both
/// as the directory a `~/` or `/` pattern is anchored at and as a specifier written without `//`,
/// since the host spells a full path that way and the path it names reads as one.
fn path_pattern(specifier: &str, anchors: &Anchors) -> Option<Pattern> {
    use crate::spelling::{to_key, to_slash};
    let backslash_separates = anchors.backslash_separates;
    let rooted = |key: &str| {
        let key = drive_in_upper_case(key, backslash_separates);
        Some(Pattern::Absolute(PathPattern::rooted(&fold(
            &key,
            anchors.folds_case,
        ))))
    };
    let specifier = &*to_slash(specifier, backslash_separates);
    if specifier.starts_with("//") {
        return rooted(&specifier[1..]);
    }
    if let Some(rest) = specifier.strip_prefix("~/") {
        let home = to_key(anchors.home.as_deref()?, backslash_separates);
        return rooted(&join(&home, rest));
    }
    if let Some(rest) = specifier.strip_prefix('/') {
        let base = to_key(anchors.settings_dir.as_deref()?, backslash_separates);
        return rooted(&join(&base, rest));
    }
    let keyed = to_key(specifier, backslash_separates);
    if is_absolute_key(&keyed) {
        return rooted(&keyed);
    }
    let rest = specifier.strip_prefix("./").unwrap_or(specifier);
    Some(Pattern::Relative(PathPattern::relative(&fold(
        rest,
        anchors.folds_case,
    ))))
}

/// `path` spelled as the rules match one: `/`-separated, its drive letter in upper case, and folded
/// where the volume folds case. One spelling for a path a gate asks about and a name a link lands on.
fn key_of(path: &str, backslash_separates: bool, folds_case: bool) -> String {
    let keyed = crate::spelling::to_key(path, backslash_separates);
    fold(
        &drive_in_upper_case(&keyed, backslash_separates),
        folds_case,
    )
}

/// `key` with the drive letter it is rooted at in upper case, where a backslash separates.
///
/// That host never tells `d:` from `D:`, whatever it does with the rest of a name, so a rule and a
/// path that differ only there are about one drive. Where a slash is the only separator `/d:` is a
/// directory like any other, and its name is kept as written.
fn drive_in_upper_case(key: &str, backslash_separates: bool) -> std::borrow::Cow<'_, str> {
    let bytes = key.as_bytes();
    let rooted_at_a_lower_case_drive = backslash_separates
        && bytes.first() == Some(&b'/')
        && bytes.get(1).is_some_and(u8::is_ascii_lowercase)
        && bytes.get(2) == Some(&b':')
        && matches!(bytes.get(3), None | Some(b'/'));
    if !rooted_at_a_lower_case_drive {
        return std::borrow::Cow::Borrowed(key);
    }
    let mut folded = key.to_string();
    folded[1..2].make_ascii_uppercase();
    std::borrow::Cow::Owned(folded)
}

/// Join two path pieces with a single slash, whatever slashes they came with.
fn join(base: &str, rest: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        rest.trim_start_matches('/')
    )
}

impl PathPattern {
    /// A pattern that starts where it says it starts, so it never floats.
    fn rooted(pattern: &str) -> Self {
        Self {
            segments: split(pattern),
            floats_when_restricting: false,
        }
    }

    /// A workspace-relative pattern.
    ///
    /// Two gitignore properties decide the depth. A pattern with no slash in it is a name and
    /// matches at any depth, in every list, so `Read(.env)` and `Read(**/.env)` are one rule. A
    /// pattern whose first segment is a plain name and which has more after it is the case Claude
    /// Code treats asymmetrically, and `floats_when_restricting` carries that.
    fn relative(pattern: &str) -> Self {
        let segments = split(pattern);
        let is_a_bare_name = segments.len() == 1;
        let starts_at_a_named_segment = segments
            .first()
            .is_some_and(|first| first != "**" && !first.contains('*'));
        Self {
            floats_when_restricting: !is_a_bare_name && starts_at_a_named_segment,
            segments: if is_a_bare_name {
                // A name matches at any depth, which is a leading `**` and nothing else.
                let mut floated = vec!["**".to_string()];
                floated.extend(segments);
                floated
            } else {
                segments
            },
        }
    }

    /// Whether this pattern covers `path`, already split into segments.
    fn matches(&self, path: &[&str], restricting: bool) -> bool {
        if segments_match(&self.segments, path) {
            return true;
        }
        // A restricting rule also catches the nested copy of a directory it named.
        if restricting && self.floats_when_restricting {
            let mut floated = vec!["**".to_string()];
            floated.extend(self.segments.iter().cloned());
            return segments_match(&floated, path);
        }
        false
    }
}

/// Split a path or pattern into segments, dropping the empties a leading or doubled slash leaves.
fn split(text: &str) -> Vec<String> {
    text.split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .map(str::to_string)
        .collect()
}

/// The same, borrowing, for the path being tested.
fn segments_of(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect()
}

/// Fold a path or a pattern's letter case, one segment at a time, where `folds` says the volume
/// holds two spellings that differ only in case as one file.
///
/// There a planner's `.ENV` opens the same file as the `.env` a deny rule names, so both sides of
/// a match are folded before they are compared, upper case first as
/// [`fold_case`](crate::trust::fold_case) does. Folding segment-wise leaves the separators, and
/// with them the anchor and relative/absolute logic and the `*` and `**` semantics, exactly as
/// they were. Nothing is folded where the volume keeps the spellings apart, since there it would
/// make one rule cover another file.
fn fold(path: &str, folds: bool) -> String {
    if !folds {
        return path.to_string();
    }
    path.split('/')
        .map(crate::trust::fold_case)
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether a pattern's segments cover a path's, with `**` crossing directories and `*` not.
///
/// Iterative, with one remembered `**` to fall back to, so a pattern full of stars costs time in
/// the length of the path rather than exponentially in the number of them. A pattern arrives from
/// a settings file, which is the user's own, but a matcher that can be made to hang is worth not
/// writing whoever supplies the input.
///
/// A trailing `**` matches the directory it hangs off as well as everything under it, which is
/// what makes `Edit(src/**)` cover `src` itself.
fn segments_match(pattern: &[String], path: &[&str]) -> bool {
    let mut p = 0;
    let mut s = 0;
    // Where to resume from if a `**` turns out to have swallowed too little.
    let mut star: Option<(usize, usize)> = None;

    while s < path.len() {
        match pattern.get(p) {
            Some(segment) if segment == "**" => {
                star = Some((p, s));
                p += 1;
            }
            Some(segment) if wildcard_matches(segment, path[s]) => {
                p += 1;
                s += 1;
            }
            _ => match star {
                // Let the last `**` take one more segment and try again.
                Some((star_p, star_s)) => {
                    p = star_p + 1;
                    s = star_s + 1;
                    star = Some((star_p, s));
                }
                None => return false,
            },
        }
    }

    // Trailing `**`s match the nothing that is left, which is what makes `src/**` cover `src`.
    pattern[p..].iter().all(|segment| segment == "**")
}

/// Whether one pattern segment covers one path segment, `*` standing in for any text within it.
fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let text: Vec<char> = text.chars().collect();
    starred_matches(pattern, &text, |c, t| c == *t)
}

/// Whether `pattern` covers `text`, `*` standing in for any run of it and every other character
/// covering what `covers` says it does.
///
/// Two pointers with a single remembered star, for the same reason as [`segments_match`]: no
/// recursion, and no pattern that costs more than the product of the two lengths.
fn starred_matches<T>(pattern: &str, text: &[T], covers: impl Fn(char, &T) -> bool) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let mut p = 0;
    let mut t = 0;
    let mut star: Option<(usize, usize)> = None;

    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(c) if covers(*c, &text[t]) => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((star_p, star_t)) => {
                    p = star_p + 1;
                    t = star_t + 1;
                    star = Some((star_p, t));
                }
                None => return false,
            },
        }
    }

    pattern[p..].iter().all(|c| *c == '*')
}

/// Read a command specifier, normalising the one spelling that is a synonym.
///
/// `ls:*` is Claude Code's other way of writing a trailing wildcard, recognised only at the end:
/// a colon anywhere else is a literal, so `git:* push` matches a command with a colon in it and
/// not a git subcommand.
fn command_pattern(specifier: &str) -> String {
    match specifier.strip_suffix(":*") {
        Some(head) => format!("{head} *"),
        None => specifier.to_string(),
    }
}

/// One place in a stage's argv as a command pattern reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// A character of one word.
    Char(char),
    /// Where one word ends and the next begins.
    Gap,
}

/// A stage's words as the places a pattern is matched against, a gap between each two.
fn places<S: AsRef<str>>(argv: &[S]) -> Vec<Place> {
    let mut places = Vec::new();
    for (at, word) in argv.iter().enumerate() {
        if at > 0 {
            places.push(Place::Gap);
        }
        places.extend(word.as_ref().chars().map(Place::Char));
    }
    places
}

/// Whether one character of a command pattern covers one place in the argv.
///
/// A space covers a gap between two words. In a rule that grants it covers nothing else, so
/// `Bash(ls *)` names a program called `ls` and not one called `ls /x`. In a rule that restricts it
/// covers a space inside a word as well, so a deny or ask rule refuses everything it refused when
/// it was matched against the words run together into a line. The asymmetry is PERM-4's: a rule
/// that restricts should cover more, and a rule that grants should cover what it names.
fn covers_place(c: char, place: &Place, restricting: bool) -> bool {
    match *place {
        Place::Char(found) => c == found && (c != ' ' || restricting),
        Place::Gap => c == ' ',
    }
}

/// Whether a command pattern covers one stage's argv.
///
/// A trailing ` *` also matches the bare command, so `Bash(ls *)` covers `ls`. That holds only
/// when the trailing star is the pattern's only one, which is what separates `Bash(ls *)` from
/// `Bash(* --help *)`: the second says there is an argument, the first says there may be.
///
/// The space before a trailing star is part of the pattern. `Bash(ls *)` does not match `lsof`,
/// and `Bash(ls*)` does, which is the difference between naming a command and naming a prefix.
/// A star covers any places at all, gaps and spaces inside a word among them, so `Bash(ls*)` also
/// covers a program called `ls /x`.
fn command_matches(pattern: &str, argv: &[Place], restricting: bool) -> bool {
    let covers = |c: char, place: &Place| covers_place(c, place, restricting);
    if let Some(head) = pattern.strip_suffix(" *")
        && !head.contains('*')
        && starred_matches(head, argv, covers)
    {
        return true;
    }
    starred_matches(pattern, argv, covers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchors() -> Anchors {
        Anchors {
            home: Some("/home/someone".to_string()),
            settings_dir: Some("/home/someone/.bravebot".to_string()),
            backslash_separates: false,
            folds_case: false,
        }
    }

    /// The same rules read for a volume with the given answer about case. The answer is the
    /// caller's and no test here asks the machine it runs on, so the two outcomes are exercised on
    /// every host.
    fn rules_on_a_volume_that_folds_case(
        folds_case: bool,
        deny: &[&str],
        ask: &[&str],
        allow: &[&str],
    ) -> Permissions {
        let anchors = Anchors {
            folds_case,
            ..anchors()
        };
        read_with(&anchors, deny, ask, allow)
    }

    /// A line split at each space, for a stage none of whose words holds one.
    fn words(line: &str) -> Vec<&str> {
        line.split(' ').collect()
    }

    type Argvs<'a> = &'a [&'a [&'a str]];

    /// Each allow rule against the argvs it should cover and the argvs it should not.
    fn assert_allow_rules_cover(cases: &[(&str, Argvs, Argvs)]) {
        for (rule, matching, not_matching) in cases {
            let permissions = rules(&[], &[], &[rule]);
            for argv in *matching {
                assert_eq!(
                    permissions.for_command(argv),
                    Decision::Ruled(Ruling::Allow),
                    "{rule} should match {argv:?}"
                );
            }
            for argv in *not_matching {
                assert_eq!(
                    permissions.for_command(argv),
                    Decision::Unmatched,
                    "{rule} should not match {argv:?}"
                );
            }
        }
    }

    fn rules(deny: &[&str], ask: &[&str], allow: &[&str]) -> Permissions {
        read_with(&anchors(), deny, ask, allow)
    }

    /// The same rules read as a host that separates with a backslash reads them. The answer is the
    /// host's and these tests run on one host, so both answers are asked for by hand: a respelling
    /// that ignored the question would read as correct from whichever host happened to run them.
    fn rules_where_a_backslash_separates(
        deny: &[&str],
        ask: &[&str],
        allow: &[&str],
    ) -> Permissions {
        let anchors = Anchors {
            backslash_separates: true,
            ..anchors()
        };
        read_with(&anchors, deny, ask, allow)
    }

    fn read_with(anchors: &Anchors, deny: &[&str], ask: &[&str], allow: &[&str]) -> Permissions {
        let owned = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (permissions, rejected) =
            Permissions::parse(&owned(deny), &owned(ask), &owned(allow), anchors);
        assert_eq!(rejected, Vec::new(), "a rule in this test did not parse");
        permissions
    }

    /// The point of the whole module: the block in Claude Code's own documentation governs this
    /// agent without being rewritten first.
    #[test]
    fn a_block_copied_from_claude_code_is_read() {
        let permissions = rules(
            &["Read(./.env)", "Read(./.env.*)"],
            &["Bash(git push *)"],
            &["Bash(git diff *)", "Bash(npm test *)"],
        );
        assert_eq!(
            permissions.for_path(Subject::Read, ".env"),
            Decision::Ruled(Ruling::Deny)
        );
        assert_eq!(
            permissions.for_path(Subject::Read, ".env.local"),
            Decision::Ruled(Ruling::Deny)
        );
        assert_eq!(
            permissions.for_command(&words("git push origin main")),
            Decision::Ruled(Ruling::Ask)
        );
        assert_eq!(
            permissions.for_command(&words("git diff --stat")),
            Decision::Ruled(Ruling::Allow)
        );
        assert_eq!(
            permissions.for_command(&words("rm -rf /")),
            Decision::Unmatched
        );
    }

    /// No rules must mean no change to anything, or adding the feature would have altered every
    /// session that never asked for it.
    #[test]
    fn no_rules_decide_nothing() {
        let permissions = Permissions::new();
        assert!(permissions.is_empty());
        assert_eq!(
            permissions.for_path(Subject::Read, "src/main.rs"),
            Decision::Unmatched
        );
        assert_eq!(permissions.for_command(&words("ls")), Decision::Unmatched);
    }

    /// The precedence is the whole of the rule, and specificity is not part of it. A deny rule
    /// that could be narrowed by an allow rule would not be readable as a statement about what
    /// will not happen.
    #[test]
    fn deny_beats_ask_and_ask_beats_allow_however_specific_the_loser() {
        let permissions = rules(&["Bash(aws *)"], &[], &["Bash(aws s3 ls)"]);
        assert_eq!(
            permissions.for_command(&words("aws s3 ls")),
            Decision::Ruled(Ruling::Deny)
        );

        let permissions = rules(&[], &["Bash(git push *)"], &["Bash(git push origin main)"]);
        assert_eq!(
            permissions.for_command(&words("git push origin main")),
            Decision::Ruled(Ruling::Ask)
        );
    }

    /// A bare family name and `(*)` are the same rule, and both cover every use.
    #[test]
    fn a_bare_family_name_covers_every_use_of_it() {
        for text in ["Bash", "Bash(*)"] {
            let permissions = rules(&[text], &[], &[]);
            assert_eq!(
                permissions.for_command(&words("anything at all")),
                Decision::Ruled(Ruling::Deny),
                "{text} did not cover every command"
            );
        }
        let permissions = rules(&["Read"], &[], &[]);
        assert_eq!(
            permissions.for_path(Subject::Read, "src/main.rs"),
            Decision::Ruled(Ruling::Deny)
        );
        // A family names one family. Denying reads says nothing about writes.
        assert_eq!(
            permissions.for_path(Subject::Edit, "src/main.rs"),
            Decision::Unmatched
        );
    }

    /// The table in Claude Code's documentation, which is the specification of where a `*` goes
    /// and what it stands in for.
    #[test]
    fn a_command_pattern_matches_where_the_documented_table_says() {
        let cases: &[(&str, &[&str], &[&str])] = &[
            (
                "Bash(npm run build)",
                &["npm run build"],
                &["npm run build --watch"],
            ),
            (
                "Bash(npm run *)",
                &["npm run build", "npm run test --watch", "npm run"],
                &["npm install"],
            ),
            (
                "Bash(git log * main)",
                &["git log --oneline main", "git log -5 main"],
                &["git log main", "git push origin main"],
            ),
            (
                "Bash(git * main)",
                &["git merge main", "git push origin main"],
                &["git log"],
            ),
            ("Bash(* --version)", &["node --version"], &["node -v"]),
            ("Bash(ls *)", &["ls -la", "ls"], &["lsof"]),
            ("Bash(ls*)", &["ls -la", "lsof"], &[]),
            ("Bash(* --help *)", &["npm --help x"], &["npm --help"]),
        ];
        for (rule, matching, not_matching) in cases {
            let permissions = rules(&[], &[], &[rule]);
            for command in *matching {
                assert_eq!(
                    permissions.for_command(&words(command)),
                    Decision::Ruled(Ruling::Allow),
                    "{rule} should match {command:?}"
                );
            }
            for command in *not_matching {
                assert_eq!(
                    permissions.for_command(&words(command)),
                    Decision::Unmatched,
                    "{rule} should not match {command:?}"
                );
            }
        }
    }

    /// The other spelling of a trailing wildcard, and the reason it is only read at the end: a
    /// colon in the middle is a character in a command, not a wildcard.
    #[test]
    fn a_trailing_colon_star_is_a_trailing_wildcard_and_a_colon_elsewhere_is_not() {
        let permissions = rules(&[], &[], &["Bash(ls:*)"]);
        for command in ["ls", "ls -la"] {
            assert_eq!(
                permissions.for_command(&words(command)),
                Decision::Ruled(Ruling::Allow),
                "{command} did not match"
            );
        }

        let permissions = rules(&[], &[], &["Bash(git:* push)"]);
        assert_eq!(
            permissions.for_command(&words("git push")),
            Decision::Unmatched
        );
        assert_eq!(
            permissions.for_command(&words("git:anything push")),
            Decision::Ruled(Ruling::Allow)
        );
    }

    /// A space in an allow rule is where one word of the argv ends and the next begins. Matched
    /// against the words run together, `Bash(ls *)` covered `"ls /x"`, one program word naming a
    /// script in a directory called `ls `, and the script ran with nobody asked. The same holds
    /// for an argument, so a rule naming a script names that script and no file whose name starts
    /// with it.
    #[test]
    fn a_space_in_an_allow_rule_covers_only_the_gap_between_two_words() {
        assert_allow_rules_cover(&[
            ("Bash(ls *)", &[&["ls", "/x"], &["ls"]], &[&["ls /x"]]),
            ("Bash(ls:*)", &[&["ls", "/x"]], &[&["ls /x"]]),
            (
                "Bash(python3 script.py *)",
                &[&["python3", "script.py", "x"]],
                &[&["python3", "script.py x"]],
            ),
            (
                "Bash(npm run build)",
                &[&["npm", "run", "build"]],
                &[&["npm", "run build"], &["npm run", "build"]],
            ),
            ("Bash(npm run *)", &[&["npm", "run"]], &[&["npm run"]]),
            (
                "Bash(git commit -m *)",
                &[&["git", "commit", "-m", "fix the build"]],
                &[&["git", "commit -m", "x"]],
            ),
        ]);
    }

    /// A star covers any text, a space inside a word among it, so `Bash(ls*)` covers a program
    /// called `ls /x` as it covers `lsof`. A space after a star is still only a gap: were it to
    /// cover a space inside a word once a star had been passed, `Bash(* --help)` would cover a
    /// program called `npm --help`, and a rule opening with a star would reach a checkout's script
    /// the way `Bash(ls *)` did.
    #[test]
    fn a_star_in_an_allow_rule_covers_any_text_and_a_space_after_it_only_a_gap() {
        assert_allow_rules_cover(&[
            ("Bash(ls*)", &[&["ls /x"], &["lsof"]], &[]),
            ("Bash(* --help)", &[&["npm", "--help"]], &[&["npm --help"]]),
            (
                "Bash(git * main)",
                &[&["git", "push", "origin", "main"]],
                &[&["git", "x main"]],
            ),
        ]);
    }

    /// The other half: a deny or ask rule still covers a space inside a word, so it refuses
    /// everything it refused when the words were run together. A rule that restricts covering
    /// less than it used to would let through a line somebody wrote it to stop.
    #[test]
    fn a_space_in_a_deny_or_ask_rule_covers_a_space_inside_a_word_too() {
        for argv in [&["ls /x"][..], &["ls", "/x"]] {
            assert_eq!(
                rules(&["Bash(ls *)"], &[], &[]).for_command(argv),
                Decision::Ruled(Ruling::Deny),
                "the deny rule did not cover {argv:?}"
            );
        }
        for argv in [&["git push", "origin"][..], &["git", "push origin"]] {
            assert_eq!(
                rules(&[], &["Bash(git push *)"], &[]).for_command(argv),
                Decision::Ruled(Ruling::Ask),
                "the ask rule did not cover {argv:?}"
            );
        }
        assert_eq!(
            rules(&["Bash(npm run *)"], &[], &[]).for_command(&["npm run"]),
            Decision::Ruled(Ruling::Deny),
            "the bare command a deny rule names was not covered when it came as one word"
        );
    }

    /// Gitignore semantics: a specifier with no slash in it is a name and matches wherever it
    /// turns up, so the two spellings are one rule.
    #[test]
    fn a_bare_name_matches_at_any_depth_in_every_list() {
        for text in ["Read(.env)", "Read(**/.env)"] {
            let permissions = rules(&[text], &[], &[]);
            for path in [".env", "src/.env", "a/b/c/.env"] {
                assert_eq!(
                    permissions.for_path(Subject::Read, path),
                    Decision::Ruled(Ruling::Deny),
                    "{text} should cover {path}"
                );
            }
            assert_eq!(
                permissions.for_path(Subject::Read, "env"),
                Decision::Unmatched,
                "{text} should not cover a different name"
            );
        }
    }

    /// A volume that folds case opens `.ENV` when it is asked for `.env`, so a deny rule written
    /// in one spelling covers the other, whichever the planner writes.
    #[test]
    fn a_deny_rule_covers_the_case_spelling_a_folding_volume_would_open() {
        let permissions = rules_on_a_volume_that_folds_case(true, &["Read(.env)"], &[], &[]);
        for path in [".ENV", ".Env"] {
            assert_eq!(
                permissions.for_path(Subject::Read, path),
                Decision::Ruled(Ruling::Deny),
                "{path} opens .env on this volume and was not covered"
            );
        }
        assert_eq!(
            permissions.for_path(Subject::Read, ".en v"),
            Decision::Unmatched
        );
    }

    /// The same folding across a tree: an anchored rule about `src` also covers `SRC`, whose
    /// segments reach the matcher lowered on both sides.
    #[test]
    fn a_tree_rule_covers_the_folded_spelling_of_its_path() {
        let permissions = rules_on_a_volume_that_folds_case(true, &["Read(src/**)"], &[], &[]);
        for path in ["src/x/y", "SRC/x/y", "Src/x/y", "\u{17f}rc/x/y"] {
            assert_eq!(
                permissions.for_path(Subject::Read, path),
                Decision::Ruled(Ruling::Deny),
                "{path} was not covered"
            );
        }
    }

    /// A rule that grants is folded like one that restricts, because on a volume that folds the two
    /// spellings are one file.
    #[test]
    fn an_allow_rule_covers_the_folded_spelling_on_a_folding_volume() {
        let permissions = rules_on_a_volume_that_folds_case(true, &[], &[], &["Edit(Docs/**)"]);
        for path in ["Docs/a.md", "docs/a.md", "DOCS/a.md"] {
            assert_eq!(
                permissions.for_path(Subject::Edit, path),
                Decision::Ruled(Ruling::Allow),
                "{path} is the same file on this volume and was not granted"
            );
        }
    }

    /// On a volume that keeps `Docs` and `docs` apart they are two files, so a rule about one
    /// decides nothing about the other, in both polarities. Folding here would widen a grant to a
    /// file the person never named.
    #[test]
    fn a_rule_does_not_cover_a_case_variant_on_a_volume_that_keeps_them_apart() {
        let allowing = rules_on_a_volume_that_folds_case(false, &[], &[], &["Edit(Docs/**)"]);
        assert_eq!(
            allowing.for_path(Subject::Edit, "Docs/a.md"),
            Decision::Ruled(Ruling::Allow)
        );
        for path in ["docs/a.md", "DOCS/a.md"] {
            assert_eq!(
                allowing.for_path(Subject::Edit, path),
                Decision::Unmatched,
                "{path} is a different file on this volume and was granted"
            );
        }

        let denying = rules_on_a_volume_that_folds_case(false, &["Read(.env)"], &[], &[]);
        assert_eq!(
            denying.for_path(Subject::Read, ".env"),
            Decision::Ruled(Ruling::Deny)
        );
        assert_eq!(
            denying.for_path(Subject::Read, ".ENV"),
            Decision::Unmatched,
            ".ENV is a different file on this volume and was denied"
        );
    }

    /// Rules read with no answer about the volume compare bytes, which is what a caller that
    /// cannot tell gets: a probe that fails must not widen a rule.
    #[test]
    fn rules_read_without_an_answer_about_the_volume_compare_bytes() {
        let permissions = rules(&[], &[], &["Edit(Docs/**)"]);
        assert_eq!(
            permissions.for_path(Subject::Edit, "docs/a.md"),
            Decision::Unmatched
        );
        assert!(!Anchors::none().folds_case);
    }

    /// The documented asymmetry, and the reason for it: a rule that restricts should cover the
    /// nested copy of what it named, and a rule that grants should cover what it named.
    #[test]
    fn a_single_segment_directory_floats_when_it_restricts_and_not_when_it_grants() {
        let denying = rules(&["Edit(src/**)"], &[], &[]);
        assert_eq!(
            denying.for_path(Subject::Edit, "src/app.ts"),
            Decision::Ruled(Ruling::Deny)
        );
        assert_eq!(
            denying.for_path(Subject::Edit, "vendor/pkg/src/lib.js"),
            Decision::Ruled(Ruling::Deny)
        );

        let allowing = rules(&[], &[], &["Edit(src/**)"]);
        assert_eq!(
            allowing.for_path(Subject::Edit, "src/app.ts"),
            Decision::Ruled(Ruling::Allow)
        );
        assert_eq!(
            allowing.for_path(Subject::Edit, "vendor/pkg/src/lib.js"),
            Decision::Unmatched
        );
    }

    /// An anchored pattern means the place it names, in every list, which is how somebody pins a
    /// rule to one directory when the floating kind would have caught more.
    #[test]
    fn an_anchored_pattern_matches_only_where_it_is_anchored() {
        for list in 0..3 {
            let rule = "Edit(/src/**)";
            let permissions = match list {
                0 => rules(&[rule], &[], &[]),
                1 => rules(&[], &[rule], &[]),
                _ => rules(&[], &[], &[rule]),
            };
            // A single slash anchors at the settings file's own directory, which is not the
            // workspace: this is the trap Claude Code's documentation warns about.
            assert_eq!(
                permissions.for_path(Subject::Edit, "src/app.ts"),
                Decision::Unmatched,
                "list {list}: a settings-anchored rule matched a workspace path"
            );
            assert_eq!(
                permissions.for_path(Subject::Edit, "/home/someone/.bravebot/src/app.ts"),
                Decision::Ruled(match list {
                    0 => Ruling::Deny,
                    1 => Ruling::Ask,
                    _ => Ruling::Allow,
                }),
                "list {list}: a settings-anchored rule missed its own directory"
            );
        }
    }

    /// The four anchors, each pointing where its own leader says.
    #[test]
    fn each_anchor_points_where_its_leader_says() {
        let permissions = rules(
            &[
                "Read(//etc/shadow)",
                "Read(~/.ssh/**)",
                "Read(/kept/**)",
                "Read(./local/**)",
            ],
            &[],
            &[],
        );
        for path in [
            "/etc/shadow",
            "/home/someone/.ssh/id_rsa",
            "/home/someone/.bravebot/kept/thing",
            "local/thing",
        ] {
            assert_eq!(
                permissions.for_path(Subject::Read, path),
                Decision::Ruled(Ruling::Deny),
                "{path} was not covered"
            );
        }
        // A single leading slash is not the filesystem root, which is the documented trap.
        assert_eq!(
            permissions.for_path(Subject::Read, "/kept/thing"),
            Decision::Unmatched
        );
    }

    /// A relative rule and an absolute one are about different paths, and neither reaches into
    /// the other's namespace. A pattern is matched against the path as a gate holds it, and a rule
    /// written one way says nothing about a path spelled the other.
    #[test]
    fn a_relative_rule_says_nothing_about_an_absolute_path() {
        let permissions = rules(&["Read(secrets/**)"], &[], &[]);
        assert_eq!(
            permissions.for_path(Subject::Read, "secrets/key"),
            Decision::Ruled(Ruling::Deny)
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "/secrets/key"),
            Decision::Unmatched
        );
    }

    /// PERM-7: a `deny` or `ask` path rule is followed from the segments before its first `*`, keyed
    /// as a gate holds a path, and a name or a leading `**` from the segments after it. A pattern
    /// that starts with any other star, a prefix holding `..`, a rule that is not about a path and an
    /// `allow` rule have nothing to follow.
    #[test]
    fn a_restricting_path_rule_is_followed_from_the_segments_before_its_first_star() {
        let mut permissions = rules(
            &[
                "Read(linked/*/key)",
                "Read(.env)",
                "Read(*.log)",
                "Read(**/x)",
                "Read(../up/**)",
                "Read(//abs/dir/**)",
                "Read(~/notes.md)",
                "Bash(ls)",
            ],
            &["Edit(a/b.md)"],
            &["Read(granted/**)"],
        );
        let asked = std::cell::RefCell::new(Vec::new());
        permissions.follow_links(|prefix| {
            asked.borrow_mut().push(prefix.to_string());
            None
        });
        assert_eq!(
            asked.into_inner(),
            [
                "linked",
                ".env",
                "x",
                "/abs/dir",
                "/home/someone/notes.md",
                "a/b.md"
            ],
            "a rule was followed from the wrong segments, or one with nothing to follow was asked about"
        );
    }

    /// The name a prefix lands on is spelled the way the path it is matched against is, so a
    /// canonical name with backslashes and a drive prefix, or in the case a folding volume stores,
    /// reaches the same file as the spelling a gate asks about.
    #[test]
    fn a_landing_is_read_in_the_spelling_a_path_is_matched_in() {
        let mut on_a_drive = rules_where_a_backslash_separates(&["Read(//D:/linked/**)"], &[], &[]);
        on_a_drive.follow_links(|_| Some(r"\\?\d:\real".to_string()));
        assert_eq!(
            on_a_drive.for_path(Subject::Read, r"D:\real\secret"),
            Decision::Ruled(Ruling::Deny),
            "a landing named with backslashes and a lower-case drive missed the file it names"
        );

        let mut folding = rules_on_a_volume_that_folds_case(true, &["Read(linked/**)"], &[], &[]);
        folding.follow_links(|_| Some("Real".to_string()));
        assert_eq!(
            folding.for_path(Subject::Read, "REAL/secret"),
            Decision::Ruled(Ruling::Deny),
            "a landing on a volume that folds case missed another case of the name it reaches"
        );
    }

    /// `*` stays inside a segment and `**` crosses them. Without that a rule about one directory
    /// would quietly cover the tree beneath it.
    #[test]
    fn one_star_stays_in_a_segment_and_two_cross_them() {
        let permissions = rules(&["Read(/*.pdf)"], &[], &[]);
        assert_eq!(
            permissions.for_path(Subject::Read, "/home/someone/.bravebot/notes.pdf"),
            Decision::Ruled(Ruling::Deny)
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "/home/someone/.bravebot/deep/notes.pdf"),
            Decision::Unmatched
        );

        let permissions = rules(&["Read(/**/*.pdf)"], &[], &[]);
        assert_eq!(
            permissions.for_path(Subject::Read, "/home/someone/.bravebot/deep/notes.pdf"),
            Decision::Ruled(Ruling::Deny)
        );
    }

    /// A trailing `/**` covers the directory it hangs off, not only what is under it, so a rule
    /// about a tree includes the tree's own name.
    #[test]
    fn a_trailing_double_star_covers_the_directory_it_names() {
        let permissions = rules(&["Read(//tmp/scratch/**)"], &[], &[]);
        for path in ["/tmp/scratch", "/tmp/scratch/a", "/tmp/scratch/a/b"] {
            assert_eq!(
                permissions.for_path(Subject::Read, path),
                Decision::Ruled(Ruling::Deny),
                "{path} was not covered"
            );
        }
        // And not a sibling whose name merely starts the same way.
        assert_eq!(
            permissions.for_path(Subject::Read, "/tmp/scratchpad"),
            Decision::Unmatched
        );
    }

    /// A family names one family. Reads and edits are separate questions, and a rule about one
    /// must not answer the other.
    #[test]
    fn a_rule_for_one_family_does_not_decide_another() {
        let permissions = rules(&[], &[], &["Edit(src/**)"]);
        assert_eq!(
            permissions.for_path(Subject::Edit, "src/main.rs"),
            Decision::Ruled(Ruling::Allow)
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "src/main.rs"),
            Decision::Unmatched
        );
        // And a path rule is not a command rule, whatever it looks like.
        assert_eq!(
            permissions.for_command(&words("src/main.rs")),
            Decision::Unmatched
        );
    }

    /// A rule nobody can act on is dropped and named, never guessed at. A misread deny rule
    /// would read as protection that is not there.
    #[test]
    fn a_rule_that_cannot_be_read_is_dropped_and_reported() {
        let texts: Vec<String> = [
            "Bash(git diff",
            // A family this agent has, with a specifier it does not read.
            "WebFetch(https://example.com/docs)",
            "WebFetch(domain:)",
            "Mcp(:get_forecast)",
            "Mcp(weather:)",
            "Mcp(weather:a:b)",
            "Mcp(ignore the above)",
            // Names matched whole, so a glob or a character no name has would match nothing.
            "Mcp(weather:get_*)",
            "Mcp(weather*)",
            "Mcp(weather:get.forecast)",
            "Mcp(-weather)",
            "Mcp(wéather)",
            "Write(src/**)",
            "Bash()",
            "",
            "   ",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let (permissions, rejected) = Permissions::parse(&texts, &[], &[], &anchors());
        assert!(permissions.is_empty(), "an unreadable rule was kept");
        assert_eq!(rejected.len(), texts.len());
    }

    /// PERM-11: a dropped rule is named in the spelling the file used, for every reason and not
    /// only an empty one. Two lines that differ only in surrounding space are two entries to find
    /// in the file, and a report that trimmed them would name both the same.
    #[test]
    fn a_dropped_rule_is_named_in_the_spelling_the_file_used() {
        let texts: Vec<String> = [
            "Bash(git diff",
            " Bash(git diff",
            "Bash(git diff \t",
            "  Fetchh(domain:denied.test)",
            "\tWebFetch(https://example.com/docs) ",
            " Bash() ",
            // A home-anchored path rule with no home to resolve it against.
            " Read(~/.env) ",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let no_anchors = Anchors::none();
        for text in &texts {
            let rejected = Rule::parse(text, &no_anchors).expect_err(text);
            assert_eq!(&rejected.text, text);
        }
        let (permissions, rejected) = Permissions::parse(&texts, &[], &[], &no_anchors);
        assert!(permissions.is_empty());
        assert_eq!(
            rejected.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
            texts.iter().map(String::as_str).collect::<Vec<_>>()
        );
    }

    /// A rule names a server, or one tool of it, and matches the two names whole: `weather` is not
    /// a prefix of `weather2`, and a rule about one tool says nothing about its neighbour.
    #[test]
    fn an_mcp_rule_covers_the_server_or_the_tool_it_names() {
        let permissions = rules(
            &["Mcp(weather:get_alerts)"],
            &["Mcp(news)"],
            &["Mcp(weather:*)"],
        );
        assert_eq!(
            permissions.for_mcp("weather", "get_alerts"),
            Decision::Ruled(Ruling::Deny)
        );
        assert_eq!(
            permissions.for_mcp("weather", "get_forecast"),
            Decision::Ruled(Ruling::Allow)
        );
        assert_eq!(
            permissions.for_mcp("news", "lookup"),
            Decision::Ruled(Ruling::Ask)
        );
        for (alias, tool) in [("weather2", "get_alerts"), ("new", "lookup"), ("", "")] {
            assert_eq!(
                permissions.for_mcp(alias, tool),
                Decision::Unmatched,
                "{alias}:{tool} was matched"
            );
        }
    }

    /// A bare `Mcp` covers every tool of every server, as a bare name does for every family, and
    /// an `Mcp` rule decides nothing about a command or a host.
    #[test]
    fn an_mcp_rule_is_its_own_family() {
        let permissions = rules(
            &["Mcp"],
            &[],
            &["Bash(weather *)", "WebFetch(domain:weather)"],
        );
        assert_eq!(
            permissions.for_mcp("weather", "get_forecast"),
            Decision::Ruled(Ruling::Deny)
        );
        let fetching = rules(&[], &[], &["WebFetch", "Bash", "Read"]);
        assert_eq!(
            fetching.for_mcp("weather", "get_forecast"),
            Decision::Unmatched
        );
        let calling = rules(&["Mcp(weather)"], &[], &[]);
        assert_eq!(calling.for_host("weather"), Decision::Unmatched);
        assert_eq!(calling.for_command(&words("weather")), Decision::Unmatched);
    }

    /// A rule for a domain covers that host and anything under it, which is what somebody writing
    /// one means by it.
    #[test]
    fn a_domain_rule_covers_the_host_and_its_subdomains() {
        let permissions = rules(&[], &[], &["WebFetch(domain:example.com)"]);
        for host in ["example.com", "docs.example.com", "a.b.example.com"] {
            assert_eq!(
                permissions.for_host(host),
                Decision::Ruled(Ruling::Allow),
                "{host} was not covered"
            );
        }
    }

    /// The boundary has to be a label boundary. Matching on the suffix alone makes a rule about
    /// one site cover every domain somebody registers ending in the same letters, which is a rule
    /// nobody wrote and the sort of hole a deny rule would be trusted not to have.
    #[test]
    fn a_domain_rule_stops_at_a_label_boundary() {
        let permissions = rules(&["WebFetch(domain:example.com)"], &[], &[]);
        for host in ["notexample.com", "example.com.evil.test", "example.co"] {
            assert_eq!(
                permissions.for_host(host),
                Decision::Unmatched,
                "{host} was matched by a rule about example.com"
            );
        }
        assert_eq!(
            permissions.for_host("example.com"),
            Decision::Ruled(Ruling::Deny)
        );
    }

    /// A host arrives from a URL, where case says nothing, so the comparison cannot depend on it.
    #[test]
    fn a_domain_rule_ignores_case() {
        let permissions = rules(&[], &[], &["WebFetch(domain:Example.COM)"]);
        assert_eq!(
            permissions.for_host("DOCS.example.com"),
            Decision::Ruled(Ruling::Allow)
        );
    }

    /// A bare family name covers every use of it, as it does for the other families.
    #[test]
    fn a_bare_web_fetch_rule_covers_every_host() {
        let permissions = rules(&["WebFetch"], &[], &[]);
        assert_eq!(
            permissions.for_host("anywhere.test"),
            Decision::Ruled(Ruling::Deny)
        );
    }

    /// A family names one family, so a rule about fetching decides nothing about reading a file
    /// and a rule about a path decides nothing about a host.
    #[test]
    fn a_web_fetch_rule_decides_nothing_about_other_families() {
        let permissions = rules(&[], &[], &["WebFetch(domain:example.com)"]);
        assert_eq!(
            permissions.for_path(Subject::Read, "example.com"),
            Decision::Unmatched
        );
        assert_eq!(
            permissions.for_command(&words("example.com")),
            Decision::Unmatched
        );

        let paths = rules(&[], &[], &["Read(src/**)"]);
        assert_eq!(paths.for_host("example.com"), Decision::Unmatched);
    }

    /// One bad rule must not take the others down with it, or a typo in an allow rule would
    /// quietly remove a deny rule's protection.
    #[test]
    fn one_unreadable_rule_does_not_discard_the_others() {
        let texts: Vec<String> = ["Read(.env)", "Nonsense(x)", "Read(.pem)"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (permissions, rejected) = Permissions::parse(&texts, &[], &[], &anchors());
        assert_eq!(permissions.len(), 2);
        assert_eq!(rejected.len(), 1);
        assert_eq!(
            permissions.for_path(Subject::Read, ".env"),
            Decision::Ruled(Ruling::Deny)
        );
    }

    /// A machine with no home directory cannot say where `~/` points, and a rule that silently
    /// matched nothing under those circumstances would be worse than one reported as unusable.
    #[test]
    fn a_pattern_whose_anchor_is_unknown_is_reported_rather_than_matching_nothing() {
        let (permissions, rejected) = Permissions::parse(
            &["Read(~/.ssh/**)".to_string(), "Read(/kept/**)".to_string()],
            &[],
            &[],
            &Anchors::none(),
        );
        assert!(permissions.is_empty());
        assert_eq!(rejected.len(), 2);
    }

    /// A rule must cover every stage for a pipeline to be allowed. An unvouched stage in the
    /// middle is a transformation nobody answered for, and its output is what the next stage
    /// reads.
    #[test]
    fn a_pipeline_is_allowed_only_when_every_stage_is() {
        let permissions = rules(&[], &[], &["Bash(git log *)", "Bash(sed *)"]);
        let allowed = [words("git log --oneline"), words("sed -n 1,10p")];
        assert_eq!(
            permissions.for_pipeline(&allowed),
            Decision::Ruled(Ruling::Allow)
        );

        let one_unruled = [words("git log --oneline"), words("curl example.com")];
        assert_eq!(permissions.for_pipeline(&one_unruled), Decision::Unmatched);
    }

    /// The other half: restricting one stage restricts the pipeline, since an unwanted program in
    /// the middle is still an unwanted program.
    #[test]
    fn restricting_one_stage_restricts_the_whole_pipeline() {
        let permissions = rules(&["Bash(curl *)"], &[], &["Bash(git log *)"]);
        let stages = [words("git log --oneline"), words("curl example.com")];
        assert_eq!(
            permissions.for_pipeline(&stages),
            Decision::Ruled(Ruling::Deny)
        );

        let permissions = rules(&[], &["Bash(curl *)"], &["Bash(git log *)"]);
        assert_eq!(
            permissions.for_pipeline(&stages),
            Decision::Ruled(Ruling::Ask)
        );
    }

    /// A pattern of nothing but wildcards must cost time in the length of what it is matched
    /// against, not exponentially in the number of stars. The matcher is iterative for this.
    #[test]
    fn a_pattern_full_of_wildcards_still_finishes() {
        let pattern = format!("Bash({}b)", "*a".repeat(24));
        let permissions = rules(&[], &[], &[&pattern]);
        let command = "a".repeat(2048);
        assert_eq!(permissions.for_command(&[&command]), Decision::Unmatched);
    }
    /// A rule a person wrote about a path applies to the path they wrote it for on every host this
    /// ships to. A pattern is matched segment by segment against a name split on `/` and a host
    /// hands a path back separated its own way, so without one spelling a name below the workspace
    /// root is a single opaque segment: `Read(src/**)` covers nothing under `src`, and
    /// `Read(.env)`, which matches a bare name at any depth, covers nothing called `.env` below the
    /// top. A `deny` refuses outright rather than prompting (PERM-2), so a rule that fails to match
    /// has nothing standing behind it.
    #[test]
    fn a_path_rule_covers_the_file_it_names_wherever_a_backslash_separates() {
        let permissions =
            rules_where_a_backslash_separates(&["Read(src/**)", "Read(.env)"], &[], &[]);

        for named in ["src\\main.rs", "config\\.env"] {
            assert_eq!(
                permissions.for_path(Subject::Read, named),
                Decision::Ruled(Ruling::Deny),
                "a deny rule did not reach '{named}' where a backslash separates"
            );
        }
    }

    /// Where a slash is the only separator a backslash is a legal filename byte, so a file whose
    /// name holds one is a file at the top of the project rather than a file below a directory.
    /// Taking it apart there would put it under a rule written about a directory nobody has, and an
    /// `allow` reaching further than it was written for is a grant nobody gave.
    #[test]
    fn a_name_holding_a_backslash_is_one_segment_where_a_slash_is_the_only_separator() {
        let permissions = rules(&[], &[], &["Read(src/**)"]);
        assert_eq!(
            permissions.for_path(Subject::Read, "src\\main.rs"),
            Decision::Unmatched,
            "a rule about a directory granted a file whose whole name is one segment"
        );

        let permissions = rules(&["Read(src/**)"], &[], &[]);
        assert_eq!(
            permissions.for_path(Subject::Read, "src\\main.rs"),
            Decision::Unmatched,
            "a rule about a directory refused a file whose whole name is one segment"
        );
    }

    /// A person writes a rule with the separator their own shell and file manager use, so a pattern
    /// is read on the same terms as the path it will be matched against. Otherwise the rule that
    /// reads as protection is one segment while the path it names is several, and it matches
    /// nothing.
    #[test]
    fn a_pattern_written_with_the_hosts_own_separator_is_the_same_rule() {
        let permissions = rules_where_a_backslash_separates(&["Read(src\\**)"], &[], &[]);

        for named in ["src\\main.rs", "src/main.rs"] {
            assert_eq!(
                permissions.for_path(Subject::Read, named),
                Decision::Ruled(Ruling::Deny),
                "a deny rule written with a backslash did not reach '{named}'"
            );
        }
    }

    /// A path carrying a root of its own never reads as a name below the workspace, and a
    /// drive-letter name reads as the full path it is. Cut into segments while still reading as
    /// relative, it would be matched against the patterns written about the workspace: a rule
    /// anchored at the project would then grant a file that is not in the project, which is the
    /// direction that fails open.
    #[test]
    fn a_workspace_rule_does_not_reach_a_path_carrying_a_root_of_its_own() {
        let permissions = rules_where_a_backslash_separates(
            &[],
            &[],
            &["Read(.env)", "Read(Desktop/**)", "Edit(*.md)"],
        );

        for named in [
            "C:\\Users\\someone\\Desktop\\.env",
            "D:\\added\\.env",
            "D:/added/.env",
        ] {
            assert_eq!(
                permissions.for_path(Subject::Read, named),
                Decision::Unmatched,
                "a rule anchored at the workspace granted '{named}', which is outside it"
            );
        }
        assert_eq!(
            permissions.for_path(Subject::Edit, "D:\\added\\a.md"),
            Decision::Unmatched,
            "a rule about the workspace's names granted a file on another drive"
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "Desktop\\.env"),
            Decision::Ruled(Ruling::Allow),
            "a name below the workspace root stopped being respelled"
        );
    }

    /// A rule about a full path on a drive has to meet that file however the host spells it, or a
    /// `deny` written about a directory outside the project refuses nothing. The spelling a person
    /// types, the one with forward slashes and the one the platform canonicalises to are one file.
    #[test]
    fn a_rule_about_a_full_path_on_a_drive_covers_the_file_it_names() {
        let permissions = rules_where_a_backslash_separates(&["Read(//D:/added/**)"], &[], &[]);

        for named in [
            "D:\\added\\secret",
            "D:/added/secret",
            "/D:/added/secret",
            "\\\\?\\D:\\added\\secret",
        ] {
            assert_eq!(
                permissions.for_path(Subject::Read, named),
                Decision::Ruled(Ruling::Deny),
                "a deny rule about D:/added did not reach '{named}'"
            );
        }
        assert_eq!(
            permissions.for_path(Subject::Read, "D:\\other\\secret"),
            Decision::Unmatched,
            "a rule about one directory reached another on the same drive"
        );
    }

    /// Where there are drive letters the home directory is on one, so a `~/` rule is about a full
    /// path on that drive, and so is a `/` rule about the settings directory below it. Anchored at
    /// a directory spelled any other way, the rule most people write first, the one fencing their
    /// keys, refuses nothing.
    #[test]
    fn a_rule_anchored_at_a_home_on_a_drive_covers_the_file_it_names() {
        let anchors = Anchors {
            home: Some("C:\\Users\\someone".to_string()),
            settings_dir: Some("C:\\Users\\someone/.bravebot".to_string()),
            backslash_separates: true,
            folds_case: false,
        };
        let permissions = read_with(
            &anchors,
            &["Read(~/.ssh/**)", "Read(/secrets/**)"],
            &[],
            &[],
        );

        for named in [
            "C:\\Users\\someone\\.ssh\\id_ed25519",
            "C:/Users/someone/.ssh/id_ed25519",
            "/C:/Users/someone/.ssh/id_ed25519",
            "C:\\Users\\someone\\.bravebot\\secrets\\key",
        ] {
            assert_eq!(
                permissions.for_path(Subject::Read, named),
                Decision::Ruled(Ruling::Deny),
                "a rule anchored at the home or settings directory did not reach '{named}'"
            );
        }
    }

    /// A person on a host with drive letters writes a full path the way their shell spells one,
    /// without the `//`. Read as a pattern about the workspace such a rule meets no file on the
    /// drive, so a `deny` written that way would refuse nothing.
    #[test]
    fn a_rule_written_from_a_drive_letter_is_about_the_full_path_it_names() {
        let permissions = rules_where_a_backslash_separates(
            &["Read(D:\\added\\**)", "Read(E:/kept/**)"],
            &[],
            &[],
        );

        for named in [
            "D:\\added\\secret",
            "D:/added/secret",
            "E:\\kept\\secret",
            "/E:/kept/secret",
        ] {
            assert_eq!(
                permissions.for_path(Subject::Read, named),
                Decision::Ruled(Ruling::Deny),
                "a deny rule written from a drive letter did not reach '{named}'"
            );
        }
    }

    /// A host with drive letters never tells `d:` from `D:`, so a rule written with one case has to
    /// meet a path named with the other, in either direction, or a `deny` refuses a file under one
    /// spelling and hands it over under the other. A host where a slash is the only separator has
    /// no drive letters, and there `/d:` and `/D:` are two directories.
    #[test]
    fn a_drive_letter_names_one_drive_whichever_case_it_is_written_in() {
        let permissions = rules_where_a_backslash_separates(
            &["Read(//D:/added/**)", "Read(//e:/kept/**)"],
            &[],
            &[],
        );
        for named in [
            "d:\\added\\secret",
            "d:/added/secret",
            "E:\\kept\\secret",
            "/E:/kept/secret",
        ] {
            assert_eq!(
                permissions.for_path(Subject::Read, named),
                Decision::Ruled(Ruling::Deny),
                "a deny rule about a drive did not reach '{named}', the same drive in another case"
            );
        }
    }

    /// Where the filesystem folds case `/d:` and `/D:` are one directory whatever the separator is,
    /// so this holds only on a case-sensitive host.
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn a_directory_named_d_colon_is_not_a_drive_where_a_slash_is_the_only_separator() {
        let slash_only = rules(&["Read(//D:/added/**)"], &[], &[]);
        assert_eq!(
            slash_only.for_path(Subject::Read, "/d:/added/secret"),
            Decision::Unmatched,
            "a directory named d: was read as D: where a slash is the only separator"
        );
    }

    /// A command rule is matched against argv, where a backslash is an argument's own byte on every
    /// host: there is no shell here (PERM-1), so nothing has separated anything. Respelling a
    /// command pattern would make a rule naming one file name another.
    #[test]
    fn a_command_rule_keeps_a_backslash_where_a_path_rule_would_not() {
        let permissions =
            rules_where_a_backslash_separates(&["Bash(type C:\\secrets.txt)"], &[], &[]);

        assert_eq!(
            permissions.for_command(&words("type C:\\secrets.txt")),
            Decision::Ruled(Ruling::Deny),
            "a command rule stopped matching the line it names"
        );
        assert_eq!(
            permissions.for_command(&words("type C:/secrets.txt")),
            Decision::Unmatched,
            "a command rule matched a line it does not name"
        );
    }

    /// A fully-qualified spelling of a host carries a trailing dot, and a rule about the domain
    /// decides the same host either way; the dot is spelling, not a label.
    #[test]
    fn a_domain_rule_matches_the_trailing_dot_spelling() {
        let permissions = rules(&["WebFetch(domain:example.com)"], &[], &[]);
        assert_eq!(
            permissions.for_host("example.com."),
            Decision::Ruled(Ruling::Deny),
            "the FQDN spelling was not covered"
        );
        for host in ["notexample.com", "example.com.evil.test"] {
            assert_eq!(
                permissions.for_host(&format!("{host}.")),
                Decision::Unmatched,
                "{host} was matched by a rule about example.com"
            );
        }
        assert_eq!(
            permissions.for_host("docs.example.com."),
            Decision::Ruled(Ruling::Deny)
        );
    }

    /// The dot is stripped from the host before it reaches the rules, spelled in any case.
    #[test]
    fn for_host_strips_the_trailing_dot_and_ignores_case() {
        let permissions = rules(&[], &[], &["WebFetch(domain:EVIL.example)"]);
        assert_eq!(
            permissions.for_host("EVIL.example."),
            Decision::Ruled(Ruling::Allow)
        );
    }

    /// A host that is only the dot is still a host: a rule for every host covers it, as it did
    /// before the dot was stripped.
    #[test]
    fn a_bare_web_fetch_deny_still_covers_a_host_of_only_a_dot() {
        let permissions = rules(&["WebFetch"], &[], &[]);
        assert_eq!(
            permissions.for_host("."),
            Decision::Ruled(Ruling::Deny),
            "a bare deny stopped covering a host that reduces to nothing"
        );
    }
}
