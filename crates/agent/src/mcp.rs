//! The MCP servers a session reached, as a turn meets them (SERVERS-7, SERVERS-8, SERVERS-13).
//!
//! A server's list of tools is the server's own words, so nothing of it reaches the planner until
//! a person has read it as it is drawn and said yes, which is road 1 of issue #83. The question is
//! asked every session and nothing is recorded, since a comparison with an earlier list would be a
//! decision taken from the server's bytes. Every call to a tool on a list somebody vouched for is
//! then put to the person with three answers, the second of which stops asking for that one tool in that
//! one project.
//!
//! The servers themselves were started before the session opened, by whoever assembled it. What
//! this holds is each one's connection, the list it answered with, and what has been decided about
//! that list.

use crate::confirm::{
    CallDecision, Confirmer, Decision, ListedTool, McpCallRequest, MoveRequest, ToolListRequest,
};
use crate::processor::Chat;
use crate::report::{IgnoreReports, Reporter};
use crate::scratch::SessionScratch;
use crate::servers::EffectivePolicy;
use bravebot_aichat::protocol::{Tool, Usage};
use bravebot_config::mcp::{
    self as records, Approvals, ChoiceArguments, Declaration, Declarations, Digest, Problem,
    Standing,
};
use bravebot_config::{Managed, Refusal, Rule};
use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
use bravebot_core::event::{RecordingSink, Sink};
use bravebot_core::policy::{McpStanding, Policy};
use bravebot_core::slot::{SlotId, SlotStore};
use bravebot_core::value::Labelled;
use bravebot_core::vetting::{Endorsed, Verdict, VettingSpec};
use bravebot_i18n::t;
use bravebot_mcp::protocol::{WIRE_PREFIX, WIRE_SEPARATOR, choice_key, with_choice};
use bravebot_mcp::{
    HttpServer, Listing, McpError, McpResult, PromptListing, StdioServer, ToolOutput, wire_name,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// How a started server is reached.
pub enum Connection {
    /// A process this session launched under confinement.
    Stdio(StdioServer),
    /// A remote server, every request to which goes through the egress gate.
    Http(HttpServer),
}

/// A server that completed its handshake and answered for its tools, with the declaration it was
/// started from.
pub struct Reached {
    connection: Connection,
    listing: Listing,
    prompts: PromptListing,
    /// What starting the server left to say to the person, which the session reports with the rest.
    notes: Vec<String>,
    declaration: Digest,
    home: Option<SessionScratch>,
    policy: Option<EffectivePolicy>,
    /// The filters the declaration put on the list, which a list read again is held to (MCP-12).
    tools: Vec<String>,
    exclude_tools: Vec<String>,
    /// Whether the declaration says every call is asked about (SERVERS-21).
    ask_every_call: bool,
    /// The arguments of its tools the declaration says take a choice (SERVERS-22).
    choice_arguments: ChoiceArguments,
    /// The headers and bearer token the declaration names, as the environment gave them when the
    /// server was started, which a server moved during a turn is reached with (SERVERS-18).
    headers: Vec<(String, String)>,
}

impl Reached {
    /// `declaration` is the digest of the declaration the server was started from, which a move
    /// is recorded over. The server offers no prompt until [`Reached::with_prompts`] says it does.
    pub fn new(connection: Connection, listing: Listing, declaration: Digest) -> Self {
        let prompts = PromptListing::none(listing.alias());
        Self {
            connection,
            listing,
            prompts,
            notes: Vec::new(),
            declaration,
            home: None,
            policy: None,
            tools: Vec::new(),
            exclude_tools: Vec::new(),
            ask_every_call: false,
            choice_arguments: ChoiceArguments::new(),
            headers: Vec::new(),
        }
    }

    /// The same server, with the headers its connection sends, which the connection it is moved to
    /// sends as well (SERVERS-11, SERVERS-18). The values are secrets: they are held and sent, and
    /// nothing reads or shows them.
    pub fn sending(mut self, headers: Vec<(String, String)>) -> Self {
        self.headers = headers;
        self
    }

    /// The same server, with whether its declaration says every call to it is asked about, which
    /// no rule and no earlier answer takes back (SERVERS-21).
    pub fn asking_every_call(mut self, ask: bool) -> Self {
        self.ask_every_call = ask;
        self
    }

    /// The same server, with the arguments of its tools its declaration says take a choice
    /// (SERVERS-22).
    pub fn choosing(mut self, choice_arguments: ChoiceArguments) -> Self {
        self.choice_arguments = choice_arguments;
        self
    }

    /// The same server, with the filters its declaration put on its list, which a list read again
    /// after the server announced a change is held to as the first was (MCP-12).
    pub fn filtered_by(mut self, tools: &[String], exclude_tools: &[String]) -> Self {
        self.tools = tools.to_vec();
        self.exclude_tools = exclude_tools.to_vec();
        self
    }

    /// The filters [`Reached::filtered_by`] holds, as `(tools, exclude_tools)`.
    #[cfg(test)]
    pub(crate) fn filters(&self) -> (&[String], &[String]) {
        (&self.tools, &self.exclude_tools)
    }

    /// The headers [`Reached::sending`] holds.
    #[cfg(test)]
    pub(crate) fn sent_headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// The same server, with the policy its process was started under (SERVERS-20). A server with
    /// one is a process the session confined.
    pub fn confined_by(mut self, policy: EffectivePolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    /// The same server, with the prompts it answered for (MCP-10).
    pub fn with_prompts(mut self, prompts: PromptListing) -> Self {
        self.prompts = prompts;
        self
    }

    /// The same server, with sentences to say to the person once it has started. The sentences are
    /// this process's own.
    pub fn noting(mut self, notes: Vec<String>) -> Self {
        self.notes.extend(notes);
        self
    }

    /// What the server offered as prompts, which is for a person's screen and never the planner's
    /// (MCP-10).
    pub fn prompts(&self) -> &PromptListing {
        &self.prompts
    }

    /// What starting the server left to say, taken so it is said once.
    pub(crate) fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// The same server, holding the directory it was handed as its home in a session that keeps
    /// nothing, so the directory goes once the server has stopped and not before.
    pub fn holding(mut self, home: SessionScratch) -> Self {
        self.home = Some(home);
        self
    }

    /// The alias the person gave the server.
    pub fn alias(&self) -> &str {
        self.listing.alias()
    }

    /// What the server offered and what was withheld or refused.
    pub fn listing(&self) -> &Listing {
        &self.listing
    }
}

/// What a session has decided about one server's list.
enum State {
    /// Nobody has been asked yet.
    Unasked(Listing),
    /// Somebody is being asked now.
    Asking,
    /// The list, as it was promoted.
    Offered(Vec<Drawn>),
    /// Nobody vouched for it, so none of its tools is offered this session.
    Declined,
}

/// One tool on a promoted list, as the client drew it.
#[derive(Debug, Clone, Deserialize)]
struct Drawn {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    arguments: Vec<Argument>,
}

/// One argument of a drawn tool.
#[derive(Debug, Clone, Deserialize)]
struct Argument {
    name: String,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    items: Option<String>,
    #[serde(default, rename = "enum")]
    choices: Option<Vec<String>>,
    #[serde(default)]
    required: bool,
}

impl Argument {
    /// `city_name (string, required)`, as a list draws it.
    fn drawn(&self) -> String {
        let mut said: Vec<String> = Vec::new();
        match (&self.kind, &self.items) {
            (Some(kind), Some(items)) => said.push(t!(
                mcp_tools_argument_list_of,
                kind = kind.as_str(),
                items = items.as_str()
            )),
            (Some(kind), None) => said.push(kind.clone()),
            (None, _) => {}
        }
        if let Some(choices) = &self.choices {
            said.push(choices.join(" | "));
        }
        if self.required {
            said.push(t!(mcp_tools_argument_required).to_string());
        }
        match said.is_empty() {
            true => self.name.clone(),
            false => format!("{} ({})", self.name, said.join(", ")),
        }
    }

    /// The schema a backend is sent for this argument.
    fn schema(&self) -> Value {
        let mut schema = Map::new();
        if let Some(kind) = &self.kind {
            schema.insert("type".into(), Value::from(kind.as_str()));
        }
        match (&self.kind, &self.items) {
            (_, Some(items)) => {
                schema.insert("items".into(), json!({ "type": items }));
            }
            // A backend refuses an array whose items are not described, and nothing here knows them.
            (Some(kind), None) if kind == "array" => {
                schema.insert("items".into(), json!({}));
            }
            _ => {}
        }
        if let Some(choices) = &self.choices {
            schema.insert("enum".into(), json!(choices));
        }
        Value::Object(schema)
    }
}

/// One server, for as long as the session runs.
struct Server {
    alias: String,
    /// Replaced when the person moves the server where its reply pointed (SERVERS-11).
    declaration: Mutex<Digest>,
    /// Locked for a question's length only, so a screen asking how things stand is never kept
    /// waiting on a call.
    state: Mutex<State>,
    /// The prompts the server offered when it started, which are a person's to run and no
    /// planner's to see (MCP-10).
    prompts: PromptListing,
    /// Locked for one call at a time.
    connection: Mutex<Connection>,
    /// What the process was started under, where it is one this session confined, which a local
    /// server is and a remote one is not (SERVERS-20). Held here so nothing asking waits on a call.
    policy: Option<EffectivePolicy>,
    /// The filters on its list, applied to a list read again (MCP-12).
    tools: Vec<String>,
    exclude_tools: Vec<String>,
    /// Whether every call to it is asked about, whatever a rule or an earlier answer says
    /// (SERVERS-21).
    ask_every_call: bool,
    /// The arguments of its tools the declaration says take a choice (SERVERS-22).
    choice_arguments: ChoiceArguments,
    /// The headers and bearer token its first connection sent, which a move sends to the new
    /// destination as well (SERVERS-18).
    headers: Vec<(String, String)>,
    /// After `connection`, since fields drop in order: the process is stopped before its home goes.
    _home: Option<SessionScratch>,
}

struct Shared {
    /// Shared with the sessions [`Session::joined_by`] and [`Session::without`] make, so a server
    /// one of them keeps is the process the other holds and is stopped when the last lets go.
    servers: Vec<Arc<Server>>,
    /// The workspace root, which answer 2 of a call records and is read against. It moves with the
    /// session, since a standing answer is about the project a person is in.
    project: Mutex<PathBuf>,
    /// The state directory, where there is one.
    directory: Option<PathBuf>,
    /// Whether anything may be written into it, which an incognito session may not.
    writable: bool,
    /// What this machine's administrator allows, which a server moved anywhere is held to as one
    /// started there is (SERVERS-12).
    managed: Managed,
}

/// The servers a session reached. Cloned into every turn of it, and every clone is the one set.
#[derive(Clone)]
pub struct Session(Arc<Shared>);

/// Names the servers and nothing any of them said.
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("servers", &self.aliases())
            .finish_non_exhaustive()
    }
}

/// Two handles are equal where they are the one session.
impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Session {}

/// How a server's tools stand, for a screen that asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offering {
    /// Nobody has been asked about the list yet.
    Unasked,
    /// This many tools are offered.
    Tools(usize),
    /// Nobody vouched for the list, so none is offered this session.
    Declined,
}

/// What settling cost, which a turn reports with what it spent, and what it said.
///
/// The notices are said to the reporter as they happen and handed back as well, since an
/// interface that draws a turn once it ends reads them off the outcome.
#[derive(Debug, Clone, Default)]
pub struct Settled {
    pub usage: Usage,
    pub notices: Vec<String>,
}

/// A reporter that keeps what it is told to say, for [`Settled::notices`].
struct Said<'a, R> {
    reporter: &'a mut R,
    notices: &'a mut Vec<String>,
}

impl<R: Reporter> Said<'_, R> {
    fn notice(&mut self, text: String) {
        self.reporter.notice(text.clone());
        self.notices.push(text);
    }
}

/// The most checks of tool lists made at the same time, the figure the reads of one round are
/// held to (TURN-10). A session with more servers than this starts the next check as each one
/// before it is joined, in server order.
const CHECKS_AT_ONCE: usize = crate::tools::READS_AT_ONCE;

fn stopped(chat: &Chat<'_>) -> bool {
    chat.cancel.is_some_and(|cancel| cancel.is_cancelled())
}

fn held<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Session {
    /// `project` is the workspace root. `directory` is the state directory, and `writable` whether
    /// this session may write into it. `managed` is what this machine's administrator allows.
    pub fn new(
        reached: Vec<Reached>,
        project: PathBuf,
        directory: Option<PathBuf>,
        writable: bool,
        managed: Managed,
    ) -> Self {
        let servers = reached
            .into_iter()
            .map(|reached| {
                Arc::new(Server {
                    alias: reached.alias().to_string(),
                    declaration: Mutex::new(reached.declaration),
                    state: Mutex::new(State::Unasked(reached.listing)),
                    prompts: reached.prompts,
                    connection: Mutex::new(reached.connection),
                    policy: reached.policy,
                    tools: reached.tools,
                    exclude_tools: reached.exclude_tools,
                    ask_every_call: reached.ask_every_call,
                    choice_arguments: reached.choice_arguments,
                    headers: reached.headers,
                    _home: reached.home,
                })
            })
            .collect();
        Self(Arc::new(Shared {
            servers,
            project: Mutex::new(project),
            directory,
            writable,
            managed,
        }))
    }

    /// This session's servers and those of `other`, in that order, as a session of their own.
    ///
    /// The servers are the ones both hold, not copies: a list's answer already given stays given,
    /// and nothing is started again. `other` supplies no server whose alias this session holds.
    pub fn joined_by(&self, other: &Session) -> Session {
        let mut servers = self.0.servers.clone();
        servers.extend(
            other
                .0
                .servers
                .iter()
                .filter(|added| servers.iter().all(|kept| kept.alias != added.alias))
                .cloned()
                .collect::<Vec<_>>(),
        );
        self.with_servers(servers)
    }

    /// This session without the server called `alias`, as a session of its own, or `None` where
    /// that leaves no server. The grant naming it goes with it (SERVERS-9), and its process stops
    /// once no turn still holds the session this was made from.
    pub fn without(&self, alias: &str) -> Option<Session> {
        let servers: Vec<Arc<Server>> = self
            .0
            .servers
            .iter()
            .filter(|server| server.alias != alias)
            .cloned()
            .collect();
        (!servers.is_empty()).then(|| self.with_servers(servers))
    }

    fn with_servers(&self, servers: Vec<Arc<Server>>) -> Session {
        Self(Arc::new(Shared {
            servers,
            project: Mutex::new(held(&self.0.project).clone()),
            directory: self.0.directory.clone(),
            writable: self.0.writable,
            managed: self.0.managed.clone(),
        }))
    }

    /// Read and record answer 2 against `root` from now on, the session having moved there.
    pub fn now_in_workspace(&self, root: &Path) {
        *held(&self.0.project) = root.to_path_buf();
    }

    /// The aliases of the servers reached, in order.
    pub fn aliases(&self) -> Vec<String> {
        self.0
            .servers
            .iter()
            .map(|server| server.alias.clone())
            .collect()
    }

    /// One grant for each server reached, naming it and no other (SERVERS-9).
    pub fn grants(&self) -> Vec<ServerAlias> {
        self.0
            .servers
            .iter()
            .map(|server| ServerAlias::new(server.alias.as_str()))
            .collect()
    }

    /// Whether any server reached is a process this session confined, which a remote one is not.
    pub fn confined(&self) -> bool {
        self.0.servers.iter().any(|server| server.policy.is_some())
    }

    /// What each confined server was started under, in order. A server this session did not start
    /// as a process of its own is not among them (SERVERS-20).
    pub fn policies(&self) -> Vec<(String, EffectivePolicy)> {
        self.0
            .servers
            .iter()
            .filter_map(|server| Some((server.alias.clone(), server.policy.clone()?)))
            .collect()
    }

    /// How each server's tools stand, in order.
    pub fn offering(&self) -> Vec<(String, Offering)> {
        self.0
            .servers
            .iter()
            .map(|server| {
                let offering = match &*held(&server.state) {
                    State::Unasked(_) | State::Asking => Offering::Unasked,
                    State::Offered(tools) => Offering::Tools(tools.len()),
                    State::Declined => Offering::Declined,
                };
                (server.alias.clone(), offering)
            })
            .collect()
    }

    /// The prompts each server offers, drawn for a person's screen, in the order the servers are
    /// held and by word within one (MCP-10).
    ///
    /// Reshaped without being read and then released to the screen, as a tool list is when it is
    /// put to a person. What comes back is the server's own words, so it is for a menu a person
    /// reads and nothing is decided from it here.
    pub fn prompts<S: Sink>(&self, policy: &mut Policy<'_, S>) -> Vec<ListedPrompt> {
        let mut listed = Vec::new();
        for server in &self.0.servers {
            if server.prompts.offered() == 0 {
                continue;
            }
            let alias = server.alias.as_str();
            let shaped = policy.render_in_place("mcp_prompts", server.prompts.list(), |text| {
                bravebot_mcp::drawn_prompts(&text)
            });
            let drawn = {
                let proof = policy.authorise_display_release("the prompts an MCP server offers");
                shaped.declassify(&proof)
            };
            listed.extend(drawn.into_iter().map(|prompt| {
                ListedPrompt {
                    alias: alias.to_string(),
                    word: wire_name(alias, &prompt.name),
                    description: prompt.description,
                    arguments: prompt
                        .arguments
                        .into_iter()
                        .map(|argument| ListedArgument {
                            name: argument.name,
                            required: argument.required,
                        })
                        .collect(),
                }
            }));
        }
        listed
    }

    /// What a line a person typed asks of a server's prompts, or `None` where it asks nothing of
    /// any server this session reached.
    ///
    /// `/mcp__<alias>__<word>` and then the arguments as `name=value` pairs (MCP-11). Only the
    /// aliases a person declared are compared with what was typed, and the word is whatever the
    /// person typed: the server's own listing is not consulted, so what it says decides nothing
    /// here. A word is what [`records::is_tool_word`] accepts. Where two aliases both begin the
    /// word the line names neither, since either could be the server the person meant and the
    /// arguments go to one of them.
    pub fn prompt_request(&self, line: &str) -> Option<Result<PromptRequest, Unparsed>> {
        let rest = line.trim().strip_prefix('/')?.strip_prefix(WIRE_PREFIX)?;
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (named, typed) = rest.split_at(end);
        let mut matched = self.0.servers.iter().filter_map(|server| {
            let word = named
                .strip_prefix(server.alias.as_str())?
                .strip_prefix(WIRE_SEPARATOR)?;
            records::is_tool_word(word).then_some((server.alias.as_str(), word))
        });
        let (alias, word) = matched.next()?;
        if matched.next().is_some() {
            return Some(Err(Unparsed::Ambiguous(format!("/{WIRE_PREFIX}{named}"))));
        }
        Some(arguments_of(typed).map(|arguments| PromptRequest {
            alias: alias.to_string(),
            name: word.to_string(),
            arguments,
        }))
    }

    /// Settle every list nobody has been asked about yet of a server `holding` grants a call to
    /// (SERVERS-8, SERVERS-13).
    ///
    /// A server the turn holds no grant for is one it is offered nothing of, so its list waits
    /// for a turn that does rather than being put to a person who could not use the answer here.
    ///
    /// Every list is checked, unless every check is being bypassed, and put to the person as it is
    /// drawn, every session: the list is a server's bytes, and nothing here decides from them
    /// whether it was seen before. A yes promotes it; a no offers none of its tools for the rest
    /// of the session, and a turn stopped at the question leaves it unasked. Settled at the start
    /// of a turn a person asked for, and of a plan run, since that is where there is somebody to put
    /// the question to.
    ///
    /// The checks of every list to be put are made at the same time, each on a thread of its own
    /// under a branch of the policy, at most [`CHECKS_AT_ONCE`] of them, and the questions are
    /// drawn in server order as each check returns (SERVERS-8). A turn that has a subscription to
    /// spend makes them one after another, because the credential is lent to one request at a
    /// time. Which lists are checked and which thread runs which check depend on the servers and
    /// the mode only, never on what a list or a reply says.
    #[expect(
        clippy::too_many_lines,
        reason = "the checks are specified, run and drawn in server order, each step depending on the one before"
    )]
    pub fn settle<S: Sink, C: Confirmer, R: Reporter>(
        &self,
        policy: &mut Policy<'_, S>,
        chat: &mut Chat<'_>,
        confirmer: &mut C,
        reporter: &mut R,
        mode: crate::PermissionMode,
        holding: &CapabilitySet,
    ) -> Settled {
        let mut settled = Settled::default();
        // Every list to be put, in server order, taken out before any is checked.
        let mut pending: Vec<(&Server, Listing)> = Vec::new();
        for server in &self.0.servers {
            if stopped(chat) {
                break;
            }
            if !holding.contains(&Capability::McpCall(ServerAlias::new(
                server.alias.as_str(),
            ))) {
                continue;
            }
            self.reread(
                policy,
                server,
                &mut Said {
                    reporter: &mut *reporter,
                    notices: &mut settled.notices,
                },
            );
            // Taken out rather than held, so nothing is locked while the person reads the list.
            let listing = {
                let mut state = held(&server.state);
                match std::mem::replace(&mut *state, State::Asking) {
                    State::Unasked(listing) => listing,
                    other => {
                        *state = other;
                        continue;
                    }
                }
            };
            pending.push((server.as_ref(), listing));
        }

        // Not made where every check is being bypassed, for the reason a vouch makes none there:
        // the mode answers this question yes, so nobody would read the word.
        let checking = mode != crate::PermissionMode::Bypass;
        let specs: Vec<Option<VettingSpec>> = pending
            .iter()
            .map(|(server, listing)| {
                checking.then(|| {
                    policy.before_vetting_a_tool_list(&server.alias, listing.list().clone())
                })
            })
            .collect();
        let together = checking && pending.len() > 1 && chat.subscription.is_none();

        // What a thread needs to make a check of its own. The turn's chat is not lent to a thread,
        // since it may hold a credential that only one request can use at a time.
        let (config, egress, model, cancel) = (chat.config, chat.egress, chat.model, chat.cancel);
        let account = chat.session.clone();
        let mut base = RecordingSink::new();
        let branchable = policy.branch(&mut base);

        std::thread::scope(|scope| {
            // The check of the list at `index` on a thread of its own, where checks are made at
            // the same time. Which index is begun when depends on the number of servers alone.
            let start = |index: usize| {
                let spec = specs[index].as_ref().filter(|_| together)?;
                let branchable = &branchable;
                let account = &account;
                Some(scope.spawn(move || {
                    let mut record = RecordingSink::new();
                    let mut branch = branchable.branch(&mut record);
                    let mut chat = Chat::new(config, egress, account).on(model);
                    if let Some(cancel) = cancel {
                        chat = chat.stoppable_by(cancel);
                    }
                    let checked = crate::vet::run(&mut branch, &mut chat, &mut IgnoreReports, spec);
                    drop(branch);
                    (checked, record)
                }))
            };
            let mut started = specs.len().min(CHECKS_AT_ONCE);
            let mut running: Vec<Option<_>> = (0..specs.len())
                .map(|index| if index < started { start(index) } else { None })
                .collect();

            for (index, ((server, listing), spec)) in pending.into_iter().zip(&specs).enumerate() {
                if stopped(chat) {
                    // Not asked, so the next turn asks. A check that was already made is still
                    // recorded and still charged.
                    if let Some(handle) = running[index].take() {
                        let (checked, record) = handle
                            .join()
                            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
                        policy.rejoin(record);
                        settled.usage.add(checked.usage);
                    }
                    *held(&server.state) = State::Unasked(listing);
                    continue;
                }
                let checked = match (spec, running[index].take()) {
                    (None, _) => (
                        Verdict::Inconclusive("the check was not made"),
                        None,
                        Usage::default(),
                    ),
                    (Some(spec), handle) => {
                        let checked = match handle {
                            // Said around the wait, so the screen names the check the question
                            // below is waiting on and has stopped saying so before it is drawn.
                            Some(handle) => {
                                reporter.check_started(spec.checking());
                                let finished = handle.join();
                                reporter.check_finished();
                                let (checked, record) = finished
                                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
                                policy.rejoin(record);
                                // A place is free, so the next list in server order is checked.
                                if started < specs.len() && !stopped(chat) {
                                    running[started] = start(started);
                                    started += 1;
                                }
                                checked
                            }
                            None => crate::vet::run(policy, chat, &mut *reporter, spec),
                        };
                        let reason = checked.reason.map(|reason| {
                            let proof =
                                policy.authorise_display_release("what a check said about content");
                            reason.declassify(&proof)
                        });
                        (checked.verdict, reason, checked.usage)
                    }
                };
                let mut said = Said {
                    reporter: &mut *reporter,
                    notices: &mut settled.notices,
                };
                let (now, usage) = self.settle_one(
                    policy, chat, confirmer, &mut said, mode, server, listing, checked,
                );
                settled.usage.add(usage);
                *held(&server.state) = now;
            }
        });
        settled
    }

    /// Read `server`'s tools again where it announced that they changed, and put the list it now
    /// gives to the person as a first list is (MCP-12).
    ///
    /// What was vouched for was the list as it stood, so the change withdraws it: none of the
    /// server's tools is offered until the new list is vouched for, and the earlier answer, a no
    /// included, is not carried over by comparing the two lists, since that would be a decision
    /// taken from the server's bytes. A list that cannot be read offers none of its tools. Only a
    /// local server is read this way: a remote one has no stream a notice could arrive on between
    /// its requests.
    fn reread<S: Sink, R: Reporter>(
        &self,
        policy: &mut Policy<'_, S>,
        server: &Server,
        said: &mut Said<'_, R>,
    ) {
        let alias = server.alias.as_str();
        let listed = match &mut *held(&server.connection) {
            Connection::Stdio(stdio) => match stdio.take_tools_changed() {
                true => bravebot_mcp::listed_or_none(alias, stdio.list_tools()),
                false => return,
            },
            Connection::Http(_) => return,
        };
        policy.record_tool_list_change(alias);
        match listed {
            Ok(listing) => {
                let listing = listing.filtered(&server.tools, &server.exclude_tools);
                *held(&server.state) = State::Unasked(listing);
                said.notice(t!(mcp_tools_changed, alias = alias));
            }
            Err(error) => {
                *held(&server.state) = State::Declined;
                said.notice(t!(
                    mcp_tools_reread_failed,
                    alias = alias,
                    reason = error.to_string()
                ));
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the arguments are what `settle` holds for one server plus the list it took out, and it has one caller"
    )]
    fn settle_one<S: Sink, C: Confirmer, R: Reporter>(
        &self,
        policy: &mut Policy<'_, S>,
        chat: &mut Chat<'_>,
        confirmer: &mut C,
        said: &mut Said<'_, R>,
        mode: crate::PermissionMode,
        server: &Server,
        listing: Listing,
        (verdict, reason, usage): (Verdict, Option<String>, Usage),
    ) -> (State, Usage) {
        let alias = server.alias.as_str();
        let refused = listing.refused();
        let withheld = listing.withheld();
        let list = listing.list().clone();

        let shaped = policy.render_in_place("mcp_tools", &list, |text| listed(alias, &text));
        let tools = {
            let proof = policy.authorise_display_release("the tools an MCP server offers");
            shaped.declassify(&proof)
        };
        let request = ToolListRequest {
            alias: alias.to_string(),
            tools,
            refused,
            withheld,
            verdict,
            reason,
        };
        let answer = confirmer.confirm_tool_list(&request);
        // Stopping the turn at the question is not answering it, so the next turn asks again.
        if stopped(chat) {
            return (State::Unasked(listing), usage);
        }
        if answer == Decision::Reject {
            policy.refuse_unapproved_tool_list(alias);
            said.notice(t!(mcp_tools_declined, alias = alias));
            return (State::Declined, usage);
        }

        policy.endorse_tool_list(alias);
        let by = match mode {
            crate::PermissionMode::Bypass => Endorsed::ByBypassing,
            _ => Endorsed::ByAPerson,
        };
        let promoted = match policy.promote_a_tool_list(alias, &list, by) {
            Ok(promoted) => promoted,
            Err(denial) => {
                said.notice(t!(mcp_tools_refused, alias = alias, reason = denial));
                return (State::Declined, usage);
            }
        };
        let state = offered(promoted);
        if let State::Offered(tools) = &state {
            for tool in tools {
                for argument in server
                    .choice_arguments
                    .get(&tool.name)
                    .into_iter()
                    .flatten()
                {
                    if shadows_a_choice(tool, argument) {
                        said.notice(t!(
                            mcp_tools_choice_shadowed,
                            alias = alias,
                            tool = tool.name.as_str(),
                            argument = argument.as_str()
                        ));
                    }
                }
            }
        }
        (state, usage)
    }

    /// The tools this turn may offer: every tool on every list somebody vouched for.
    pub fn offer(&self) -> Offer {
        let mut tools: Vec<Offered> = Vec::new();
        for server in &self.0.servers {
            if let State::Offered(drawn) = &*held(&server.state) {
                tools.extend(drawn.iter().map(|drawn| Offered {
                    wire: wire_name(&server.alias, &drawn.name),
                    alias: server.alias.clone(),
                    drawn: drawn.clone(),
                }));
            }
        }
        // Two servers whose alias and word compose one wire name are neither of them: which of
        // the two a call reached would be the order they were listed in.
        let mut counted: std::collections::BTreeMap<String, usize> = Default::default();
        for tool in &tools {
            *counted.entry(tool.wire.clone()).or_default() += 1;
        }
        tools.retain(|tool| counted[&tool.wire] == 1);
        Offer {
            session: self.clone(),
            tools,
            round: Round::default(),
            deferred: false,
            loaded: Vec::new(),
            reading: false,
        }
    }

    /// Whether the declaration of the server reached as `alias` says every call to it is asked
    /// about (SERVERS-21).
    fn asks_every_call(&self, alias: &str) -> bool {
        self.0
            .servers
            .iter()
            .any(|server| server.alias == alias && server.ask_every_call)
    }

    /// The arguments of the tool `tool` of the server reached as `alias` that its declaration says
    /// take a choice (SERVERS-22).
    fn choice_arguments(&self, alias: &str, tool: &str) -> Option<&BTreeSet<String>> {
        let server = self.0.servers.iter().find(|server| server.alias == alias)?;
        server.choice_arguments.get(tool)
    }

    /// Whether answer 2 was given about this tool in this project.
    fn stands(&self, alias: &str, tool: &str) -> bool {
        self.0.directory.as_deref().is_some_and(|directory| {
            Standing::read(directory).covers(alias, tool, &held(&self.0.project))
        })
    }

    /// Whether answer 2 can be recorded, which needs a state directory this session may write.
    fn may_stand(&self) -> bool {
        self.0.directory.is_some() && self.0.writable
    }

    /// Record answer 2 about this tool in this project.
    fn stand(&self, alias: &str, tool: &str) -> std::io::Result<()> {
        let Some(directory) = self.0.directory.as_deref() else {
            return Ok(());
        };
        let mut standing = Standing::to_change(directory)
            .map_err(|why| std::io::Error::other(unreadable_record(&why)))?;
        let project = held(&self.0.project).clone();
        if !standing.add(alias, tool, &project) {
            return Err(std::io::Error::other(t!(mcp_call_path_not_one_line)));
        }
        crate::home::replace(&records::tools_file(directory), standing.to_text())
    }

    /// The server this session reached under `alias`.
    fn reached(&self, alias: &str) -> McpResult<&Arc<Server>> {
        self.0
            .servers
            .iter()
            .find(|server| server.alias == alias)
            .ok_or_else(|| McpError::Transport(format!("no server {alias} was reached")))
    }

    pub(crate) fn call<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        egress: &bravebot_net::Egress,
        alias: &str,
        tool: &str,
        arguments: Value,
    ) -> McpResult<ToolOutput> {
        let server = self.reached(alias)?;
        match &mut *held(&server.connection) {
            Connection::Stdio(server) => server.call_tool(policy, tool, arguments),
            Connection::Http(server) => server.call_tool(policy, egress, tool, arguments),
        }
    }

    /// Get a prompt's text of the server `alias` (MCP-11).
    pub(crate) fn prompt<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        egress: &bravebot_net::Egress,
        alias: &str,
        name: &str,
        arguments: &[(String, String)],
    ) -> McpResult<Labelled<String>> {
        let server = self.reached(alias)?;
        match &mut *held(&server.connection) {
            Connection::Stdio(server) => server.get_prompt(policy, name, arguments),
            Connection::Http(server) => server.get_prompt(policy, egress, name, arguments),
        }
    }

    /// Put where `alias`'s reply pointed to the person, and on a yes move the server there
    /// (SERVERS-11). Whether it moved, so the request may be made again to where it now is.
    ///
    /// The destination is the server's own bytes, so it is drawn and decides nothing until the
    /// person says it is where the server now is: only then is it read as a url, held to the
    /// managed layer, and reached. The handshake there comes before the declaration is rewritten,
    /// so a destination that does not answer is not written down.
    fn moved<S: Sink, C: Confirmer, R: Reporter>(
        &self,
        policy: &mut Policy<'_, S>,
        egress: &bravebot_net::Egress,
        confirmer: &mut C,
        reporter: &mut R,
        alias: &str,
        hop: &Labelled<String>,
    ) -> bool {
        let Some(server) = self.0.servers.iter().find(|server| server.alias == alias) else {
            return false;
        };
        let (declared, startup, bound) = match &*held(&server.connection) {
            Connection::Http(http) => (http.url().to_string(), http.startup(), http.bound()),
            Connection::Stdio(_) => return false,
        };
        let from = *held(&server.declaration);
        let shaped = policy.render_in_place("mcp_move", hop, |url| {
            let authority = bravebot_core::url::authority_of(&url).unwrap_or_default();
            (url, authority)
        });
        let (destination, authority) = {
            let proof = policy.authorise_display_release("where an MCP server's reply pointed");
            shaped.declassify(&proof)
        };
        let request = MoveRequest {
            alias: alias.to_string(),
            declared,
            destination,
            authority,
            may_record: self.may_stand(),
        };
        if confirmer.confirm_move(&request) == Decision::Reject {
            policy.refuse_unapproved_move(alias);
            reporter.notice(t!(mcp_move_declined, alias = alias));
            return false;
        }

        policy.endorse_server_move(alias);
        let Ok(Ok(url)) = policy
            .promote_a_server_move(alias, hop)
            .map(Labelled::into_trusted)
        else {
            return false;
        };
        let declaration = match Declaration::http(url.clone()) {
            Ok(declaration) => declaration,
            Err(problem) => {
                reporter.notice(t!(
                    mcp_move_undeclarable,
                    alias = alias,
                    problem = undeclarable(&problem)
                ));
                return false;
            }
        };
        if let Some(reason) =
            managed_refusal(&self.0.managed, bravebot_config::Server::Remote(&url))
        {
            reporter.notice(t!(
                mcp_move_refused_by_managed,
                alias = alias,
                reason = reason
            ));
            return false;
        }

        let mut moved = HttpServer::new(alias, url.as_str())
            .with_headers(server.headers.clone())
            .starting_within(startup);
        if let Err(error) = moved.initialize(policy, egress, "bravebot", env!("CARGO_PKG_VERSION"))
        {
            match policy.take_server_hop() {
                Some(_) => reporter.notice(t!(mcp_move_again, alias = alias)),
                None => reporter.notice(t!(
                    mcp_move_no_handshake,
                    alias = alias,
                    reason = error.to_string()
                )),
            }
            return false;
        }

        moved.set_bound(bound);
        // The digest of what is written where something is written, and otherwise of the address
        // alone, since a session that records nothing has no declaration to read the rest from.
        let mut digest = declaration.digest();
        if let (Some(directory), true) = (&self.0.directory, self.0.writable) {
            match record_a_move(directory, alias, &from, &url) {
                Ok(recorded) => {
                    digest = recorded.digest();
                    reporter.notice(t!(mcp_move_moved, alias = alias));
                }
                Err(Unmoved::Edited) => {
                    reporter.notice(t!(mcp_move_edited, alias = alias));
                    return false;
                }
                Err(Unmoved::NotWritten(error)) => {
                    reporter.notice(t!(mcp_move_not_recorded, alias = alias, error = error));
                }
            }
        } else {
            reporter.notice(t!(mcp_move_moved, alias = alias));
        }
        *held(&server.connection) = Connection::Http(moved);
        *held(&server.declaration) = digest;
        true
    }
}

/// Why a url a person approved a server moving to cannot be declared. [`Declaration::http`] refuses
/// one for nothing else.
fn undeclarable(problem: &Problem) -> String {
    match problem {
        Problem::Credentials => t!(mcp_problem_credentials).to_string(),
        _ => t!(mcp_problem_url).to_string(),
    }
}

/// Why rewriting a server's declaration to where it moved was not done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unmoved {
    /// The declaration is no longer the one the server was started from, so what the person was
    /// asked about is not what the file says, and the file is left as it is.
    Edited,
    /// A file could not be read or written. The sentence says which, and why.
    NotWritten(String),
}

/// Rewrite `alias`'s declaration from `from` to the same declaration at `url`, and approve the
/// result in the same write (SERVERS-11). The declaration written is returned.
///
/// Where it is reached is what moved. Everything else is the declaration the person wrote and was
/// asked about: its tool filters, the variables that name its headers and its bearer token, its
/// bounds, and whether every call to it is asked about (SERVERS-21).
///
/// The approval is recorded in the approvals file whatever answered for the declaration it
/// replaces: a project a person said to use every server in answers for what a checkout requests,
/// and never for where a server went. Declarations first, then approvals, as every rewrite of the
/// pair is, so a write that stops between them leaves a declaration nobody approved, which asks.
pub fn record_a_move(
    directory: &Path,
    alias: &str,
    from: &Digest,
    url: &str,
) -> Result<Declaration, Unmoved> {
    let mut declarations = Declarations::read(directory).map_err(|why| {
        Unmoved::NotWritten(t!(
            mcp_unreadable,
            path = shown(&records::declarations_file(directory).display().to_string()),
            reason = unreadable(&why)
        ))
    })?;
    let Some(current) = declarations
        .get(alias)
        .and_then(|entry| entry.declaration.ok())
        .filter(|declaration| declaration.digest() == *from)
    else {
        return Err(Unmoved::Edited);
    };
    let to = current
        .moved_to(url.to_string())
        .map_err(|problem| Unmoved::NotWritten(undeclarable(&problem)))?;
    let mut approvals = Approvals::to_change(directory).map_err(|why| {
        Unmoved::NotWritten(t!(
            mcp_unreadable,
            path = shown(&records::approvals_file(directory).display().to_string()),
            reason = unreadable_record(&why)
        ))
    })?;
    declarations.insert(alias, &to);
    approvals.keep_only(&declarations);
    approvals.approve(alias, to.digest());
    crate::home::replace(
        &records::declarations_file(directory),
        declarations.to_text(),
    )
    .and_then(|()| crate::home::replace(&records::approvals_file(directory), approvals.to_text()))
    .map_err(|error| Unmoved::NotWritten(error.to_string()))?;
    Ok(to)
}

/// The reason the machine's managed layer gives for keeping `server` from starting, naming its
/// file, where it gives one (SERVERS-12).
pub fn managed_refusal(managed: &Managed, server: bravebot_config::Server<'_>) -> Option<String> {
    let (path, refusal) = managed.refuses(server)?;
    let path = path.display().to_string();
    Some(match refusal {
        Refusal::NotAllowed => t!(managed_not_allowed, path = path),
        Refusal::Denied(rule) => t!(managed_denied, path = path, entry = entry(rule)),
        Refusal::HostUnread => t!(managed_host_unread, path = path),
    })
}

/// A managed entry as its file's author would find it again.
pub fn entry(rule: &Rule) -> String {
    match rule {
        Rule::Host(host) => format!("host {}", shown(host)),
        Rule::Command(argv) => {
            let words: Vec<String> = argv.iter().map(|word| shown(word)).collect();
            format!("command {}", words.join(" "))
        }
    }
}

/// A word as it is safe to draw: as it is where every character is one a terminal draws as itself
/// and none would blur where the word ends, and quoted with its escapes otherwise.
pub fn shown(word: &str) -> String {
    let plain = !word.is_empty()
        && word.chars().all(|c| match c.is_ascii() {
            true => c.is_ascii_graphic() && !matches!(c, '"' | '\'' | '\\'),
            false => c.is_alphanumeric(),
        });
    match plain {
        true => word.to_string(),
        false => format!("{word:?}"),
    }
}

/// Why the declarations file cannot be read, and so cannot be rewritten either.
pub fn unreadable(why: &records::Unreadable) -> String {
    match why {
        records::Unreadable::TooLarge => t!(mcp_unreadable_too_large).to_string(),
        records::Unreadable::NotRead => t!(mcp_unreadable_not_read).to_string(),
        records::Unreadable::NotJson => t!(mcp_unreadable_not_json).to_string(),
        records::Unreadable::NotAnObject => t!(mcp_unreadable_not_an_object).to_string(),
        records::Unreadable::Servers => t!(mcp_unreadable_servers).to_string(),
        records::Unreadable::Key(key) => t!(mcp_unreadable_key, key = shown(key)),
    }
}

/// Whether the server lists the tool `tool` with an argument of the name a choice for `argument`
/// is given by, `<argument>_ref`, which is then the server's own.
fn shadows_a_choice(tool: &Drawn, argument: &str) -> bool {
    let key = choice_key(argument);
    tool.arguments.iter().any(|own| own.name == key)
}

/// The arguments of `tool` that are offered as a choice, from the `declared` ones: those the
/// server's list does not already give an argument `<argument>_ref` for.
fn choosable<'a>(declared: Option<&'a BTreeSet<String>>, tool: &Drawn) -> Vec<&'a String> {
    declared
        .into_iter()
        .flatten()
        .filter(|argument| !shadows_a_choice(tool, argument))
        .collect()
}

/// A promoted list, as the tools it offers.
///
/// A list that will not read as one offers nothing. The client drew it, so that cannot happen short
/// of a bug, and the direction to be wrong in is offering too little.
fn offered(promoted: Labelled<String>) -> State {
    let tools = promoted
        .into_trusted()
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Drawn>>(&text).ok())
        .unwrap_or_default();
    State::Offered(tools)
}

/// A list as it is drawn for a person. Called inside the policy's reshape, where the text may be
/// shaped and not read for any decision.
fn listed(alias: &str, text: &str) -> Vec<ListedTool> {
    serde_json::from_str::<Vec<Drawn>>(text)
        .unwrap_or_default()
        .into_iter()
        .map(|drawn| ListedTool {
            name: format!("{alias}:{}", drawn.name),
            arguments: drawn.arguments.iter().map(Argument::drawn).collect(),
            description: drawn.description,
        })
        .collect()
}

/// Why a record read to be written back was left as it is.
pub fn unreadable_record(why: &records::Unreadable) -> String {
    match why {
        records::Unreadable::TooLarge => t!(mcp_record_too_large).to_string(),
        _ => t!(mcp_record_not_read).to_string(),
    }
}

/// One prompt of a server, as a person reads it in the menu a slash opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedPrompt {
    /// The server, by the alias a person gave it.
    pub alias: String,
    /// What a person types after the slash, `mcp__weather__forecast`.
    pub word: String,
    /// The server's sentence about the prompt, cleaned of what a terminal would act on.
    pub description: Option<String>,
    /// The arguments it takes, by name, in name order.
    pub arguments: Vec<ListedArgument>,
}

/// One argument of a [`ListedPrompt`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedArgument {
    pub name: String,
    /// Whether the prompt cannot run without it.
    pub required: bool,
}

/// A line a person typed that runs a prompt of a server this session reached.
///
/// Every word of it is the person's own: the alias is one they declared, and the name and the
/// arguments are what they typed. A turn carries it as it is and has the server's text fetched
/// from it (MCP-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRequest {
    pub alias: String,
    pub name: String,
    /// `name=value` pairs, in the order they were typed.
    pub arguments: Vec<(String, String)>,
}

/// Why the words typed after a prompt's name are not arguments. Each holds what the person typed,
/// so a note can say which word it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unparsed {
    /// A word with no `=` in it, which could be an argument's value and could not say whose.
    NotAPair(String),
    /// A word that does not start with a name an argument may have.
    BadName(String),
    /// The same argument named twice.
    Twice(String),
    /// A quoted value that does not end, or goes on past its closing quote.
    Quote(String),
    /// The first word, which two of the servers this session reached both begin.
    Ambiguous(String),
}

/// The `name=value` pairs of what followed a prompt's name.
///
/// A value is the characters up to the next space, or what is between double quotes, where `\"`
/// and `\\` stand for a quote and a backslash. A value may be empty.
fn arguments_of(typed: &str) -> Result<Vec<(String, String)>, Unparsed> {
    let mut arguments: Vec<(String, String)> = Vec::new();
    let mut rest = typed.trim_start();
    while !rest.is_empty() {
        let end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let name = &rest[..end];
        let Some(after) = rest[end..].strip_prefix('=') else {
            let word = rest.split_whitespace().next().unwrap_or(rest);
            return Err(Unparsed::NotAPair(word.to_string()));
        };
        if !bravebot_mcp::protocol::is_argument_name(name) {
            let word = rest.split_whitespace().next().unwrap_or(rest);
            return Err(Unparsed::BadName(word.to_string()));
        }
        if arguments.iter().any(|(seen, _)| seen == name) {
            return Err(Unparsed::Twice(name.to_string()));
        }
        let (value, tail) = match after.strip_prefix('"') {
            Some(quoted) => {
                quoted_value(quoted).ok_or_else(|| Unparsed::Quote(name.to_string()))?
            }
            None => {
                let end = after.find(char::is_whitespace).unwrap_or(after.len());
                (after[..end].to_string(), &after[end..])
            }
        };
        if !tail.is_empty() && !tail.starts_with(char::is_whitespace) {
            return Err(Unparsed::Quote(name.to_string()));
        }
        arguments.push((name.to_string(), value));
        rest = tail.trim_start();
    }
    Ok(arguments)
}

/// A value up to its closing quote, and what follows the quote. `None` where it never closes.
fn quoted_value(text: &str) -> Option<(String, &str)> {
    let mut value = String::new();
    let mut characters = text.char_indices();
    while let Some((at, character)) = characters.next() {
        match character {
            '"' => return Some((value, &text[at + 1..])),
            '\\' => match characters.clone().next() {
                Some((_, escaped @ ('"' | '\\'))) => {
                    value.push(escaped);
                    characters.next();
                }
                _ => value.push('\\'),
            },
            other => value.push(other),
        }
    }
    None
}

/// What fetching a prompt came to.
pub(crate) enum Fetched {
    /// The server's text, held as untrusted and private (MCP-11).
    Text(Labelled<String>),
    /// Nothing reached the server or nothing usable came back. The sentence is this process's own.
    Problem(String),
}

/// Fetch the prompt a person ran (MCP-11).
///
/// A `deny` rule refuses it before anything is sent, matched on the alias and the word as a tool
/// call is. Nobody is asked: the line is the person's own and carries its arguments, which are
/// what the server is sent. A reply that pointed off the declaration is put to the person as a
/// tool call's is.
pub(crate) fn fetch_prompt<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    session: &Session,
    egress: &bravebot_net::Egress,
    confirmer: &mut C,
    reporter: &mut R,
    request: &PromptRequest,
) -> Fetched {
    let PromptRequest {
        alias,
        name,
        arguments,
    } = request;
    if let Err(denial) = policy.before_mcp_call_rules(alias, name) {
        return Fetched::Problem(format!("refused: {denial}"));
    }
    let answered = following_a_move(
        policy,
        session,
        egress,
        confirmer,
        reporter,
        alias,
        |policy| session.prompt(policy, egress, alias, name, arguments),
    );
    match answered {
        Ok(text) => Fetched::Text(text),
        Err(error) => Fetched::Problem(t!(
            mcp_prompt_failed,
            alias = alias.as_str(),
            name = name.as_str(),
            reason = error.to_string()
        )),
    }
}

/// Ask `ask`, and where a reply pointed off the server's declaration put that to the person and, on
/// a yes, ask once more of where the server now is. A second hop is refused as the first was
/// (SERVERS-11).
fn following_a_move<S: Sink, C: Confirmer, R: Reporter, T>(
    policy: &mut Policy<'_, S>,
    session: &Session,
    egress: &bravebot_net::Egress,
    confirmer: &mut C,
    reporter: &mut R,
    alias: &str,
    ask: impl Fn(&mut Policy<'_, S>) -> McpResult<T>,
) -> McpResult<T> {
    match ask(policy) {
        Err(McpError::Denied(denial)) => match policy.take_server_hop() {
            Some(hop) if session.moved(policy, egress, confirmer, reporter, alias, &hop) => {
                let again = ask(policy);
                if matches!(again, Err(McpError::Denied(_))) && policy.take_server_hop().is_some() {
                    reporter.notice(t!(mcp_move_again, alias = alias));
                }
                again
            }
            _ => Err(McpError::Denied(denial)),
        },
        other => other,
    }
}

/// One tool a turn offers.
#[derive(Debug, Clone)]
struct Offered {
    wire: String,
    alias: String,
    drawn: Drawn,
}

/// A tool a plan may name, drawn from a list a person vouched for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Callable {
    pub alias: String,
    pub tool: String,
    pub arguments: Vec<CallableArgument>,
}

/// One argument of a [`Callable`]: its name, its type where the list gave one, and whether the
/// tool requires it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallableArgument {
    pub name: String,
    pub kind: Option<String>,
    pub required: bool,
}

/// The calls to a server's tool that the round now running holds, and the answers one question
/// already gave about them (SERVERS-7).
///
/// The calls are the planner's own words, taken from the round before the first of them runs, each
/// under its position in the round. A question about one lists the later calls to the same tool,
/// and a yes or a no to it covers those calls and only those: a later call is covered when its
/// position is one the person was shown and the label of its arguments is the one they were asked
/// under. Nothing here compares an argument's value with anything, since a value the planner wrote
/// may derive from private content (LABEL-5). A clone starts empty, so a delegate's turn never
/// reads the round of the turn that started it.
#[derive(Debug, Default)]
pub struct Round(Mutex<RoundCalls>);

#[derive(Debug, Default)]
struct RoundCalls {
    /// The position in the round of the call now being dispatched, where it is a call to a tool of
    /// a server.
    current: Option<usize>,
    upcoming: Vec<Upcoming>,
    covered: Vec<Covered>,
}

#[derive(Debug)]
struct Upcoming {
    index: usize,
    alias: String,
    tool: String,
    arguments: Value,
}

#[derive(Debug)]
struct Covered {
    index: usize,
    private: bool,
    approved: bool,
}

impl Clone for Round {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Round {
    /// The calls this round holds to a server's tool, in order, replacing whatever the last round
    /// held.
    fn begin(&self, calls: Vec<(usize, String, String, Value)>) {
        let mut held = held(&self.0);
        held.current = None;
        held.covered.clear();
        held.upcoming = calls
            .into_iter()
            .map(|(index, alias, tool, arguments)| Upcoming {
                index,
                alias,
                tool,
                arguments,
            })
            .collect();
    }

    /// Names the position in the round of the call about to be dispatched, or none once it has
    /// been.
    fn at(&self, index: Option<usize>) {
        held(&self.0).current = index;
    }

    /// The positions and arguments of the later calls of the round to the same tool as the call
    /// being dispatched. Found by position and by the alias and word of the tool, never by an
    /// argument.
    fn later(&self, alias: &str, tool: &str) -> Vec<(usize, Value)> {
        let held = held(&self.0);
        let Some(now) = held.current else {
            return Vec::new();
        };
        let here = held
            .upcoming
            .iter()
            .any(|call| call.index == now && call.alias == alias && call.tool == tool);
        if !here {
            return Vec::new();
        }
        held.upcoming
            .iter()
            .filter(|call| call.index > now && call.alias == alias && call.tool == tool)
            .map(|call| (call.index, call.arguments.clone()))
            .collect()
    }

    /// Records the answer to a question that listed the calls at these positions.
    fn cover(&self, listed: &[usize], private: bool, approved: bool) {
        held(&self.0)
            .covered
            .extend(listed.iter().map(|index| Covered {
                index: *index,
                private,
                approved,
            }));
    }

    /// What an earlier question in this round answered for the call being dispatched, once.
    fn answered(&self, private: bool) -> Option<bool> {
        let mut held = held(&self.0);
        let now = held.current?;
        let at = held
            .covered
            .iter()
            .position(|call| call.index == now && call.private == private)?;
        Some(held.covered.remove(at).approved)
    }
}

/// The tools one turn offers, and the session they are called through.
#[derive(Debug, Clone)]
pub struct Offer {
    session: Session,
    tools: Vec<Offered>,
    /// What the round now running holds for the tools of servers.
    round: Round,
    /// Whether the tools are offered by name only until one is loaded (SERVERS-16).
    deferred: bool,
    /// The wire names loaded so far this turn, in the order they were loaded. Always names this
    /// offer holds, since [`Offer::load`] takes a name only by finding it in `tools`.
    loaded: Vec<String>,
    /// Whether each tool is offered a `read` argument, which asks for what the call returns to be
    /// checked and released in its result. Set where `vet_content` is on the turn's list, since
    /// that is the release the argument asks for a round early.
    reading: bool,
}

/// The name of the argument that asks for what a call returned to be read in its result.
pub const READ_ARGUMENT: &str = "read";

impl Offer {
    /// This offer with each tool given a `read` argument, or without one.
    pub fn reading(mut self, reading: bool) -> Self {
        self.reading = reading;
        self
    }

    /// Whether the call to the tool `tool` of the server `alias` takes a `read` argument of this
    /// process's own: the offer gives tools one, and the server did not list an argument of that
    /// name itself. A server's own `read` is the server's, and a call that gives it is sent as
    /// written.
    pub fn takes_read(&self, alias: &str, tool: &str) -> bool {
        self.reading
            && self
                .tools
                .iter()
                .find(|offered| offered.alias == alias && offered.drawn.name == tool)
                .is_some_and(|offered| {
                    !offered
                        .drawn
                        .arguments
                        .iter()
                        .any(|argument| argument.name == READ_ARGUMENT)
                })
    }

    /// The functions a backend is sent, one per tool.
    ///
    /// The description is the server's words, which a person read and let through, so it is sent
    /// behind a margin and after a sentence of this process's own saying whose they are. It is a
    /// function's description and never a line of the system prompt (SERVERS-8).
    ///
    /// Where the offer is deferred, only the tools loaded so far (SERVERS-16).
    pub fn functions(&self) -> Vec<Tool> {
        self.tools
            .iter()
            .filter(|tool| !self.deferred || self.loaded.contains(&tool.wire))
            .map(|tool| {
                let name = format!("{}:{}", tool.alias, tool.drawn.name);
                let mut description = format!(
                    "{name}, a tool of the MCP server the user named {}. Every call is put to the \
                     user first.",
                    tool.alias
                );
                if let Some(said) = &tool.drawn.description {
                    description.push_str(" What the server says about it:");
                    for line in said.lines() {
                        description.push_str("\n│ ");
                        description.push_str(line);
                    }
                }
                let mut properties: Map<String, Value> = tool
                    .drawn
                    .arguments
                    .iter()
                    .map(|argument| (argument.name.clone(), argument.schema()))
                    .collect();
                // An argument the declaration says takes a choice is offered a second way to be
                // given, beside the first, and neither is required: the planner gives one of the
                // two (MCP-14). Not where the server lists an argument of that second name itself.
                let choosing = choosable(
                    self.choice_arguments(&tool.alias, &tool.drawn.name),
                    &tool.drawn,
                );
                for argument in &choosing {
                    properties.insert(
                        choice_key(argument),
                        json!({
                            "type": "string",
                            "description": format!(
                                "A reference such as ref:3 to a choice an isolated processor \
                                 made, in place of {argument}. Give {argument} or this, not both. \
                                 The user is shown the number and asked before the call is made."
                            ),
                        }),
                    );
                }
                if self.takes_read(&tool.alias, &tool.drawn.name) {
                    properties.insert(
                        READ_ARGUMENT.to_string(),
                        json!({
                            "type": "boolean",
                            "description": "Ask to have what this call returns checked and \
                                            released in this result, as vet_content would, so \
                                            there is no vet_content call to make. Where it \
                                            would be kept back, or the content is too long \
                                            for one result, it still comes back as a \
                                            reference. Not sent to the server. Defaults to \
                                            false.",
                        }),
                    );
                }
                let required: Vec<&str> = tool
                    .drawn
                    .arguments
                    .iter()
                    .filter(|argument| argument.required)
                    .filter(|argument| !choosing.contains(&&argument.name))
                    .map(|argument| argument.name.as_str())
                    .collect();
                Tool::function(
                    tool.wire.clone(),
                    description,
                    json!({ "type": "object", "properties": properties, "required": required }),
                )
            })
            .collect()
    }

    /// What offering every tool in full would cost the context, in tokens.
    ///
    /// An estimate from the length of the definitions a backend would be sent, at four characters
    /// a token. It reads text a person vouched for and decides only how much is offered at once.
    pub fn weight(&self) -> u64 {
        let all = Offer {
            deferred: false,
            ..self.clone()
        };
        all.functions()
            .iter()
            .map(|tool| serde_json::to_string(tool).map_or(0, |text| text.len() as u64))
            .sum::<u64>()
            .div_ceil(4)
    }

    /// Offer the tools by name only when offering them in full would cost more than `threshold`
    /// tokens (SERVERS-16).
    pub fn deferring_above(mut self, threshold: u64) -> Self {
        self.deferred = self.weight() > threshold;
        self
    }

    /// Whether the tools are offered by name only until one is loaded.
    pub fn is_deferred(&self) -> bool {
        self.deferred
    }

    /// The names a deferred offer has not yet loaded, each as `alias:tool`.
    pub fn unloaded(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter(|tool| !self.loaded.contains(&tool.wire))
            .map(|tool| format!("{}:{}", tool.alias, tool.drawn.name))
            .collect()
    }

    /// The wire name of the tool called `name` as `alias:tool`, compared exactly, and whether it
    /// is loaded already. Nothing falls back to a prefix, a case-insensitive match or a nearest
    /// one (SERVERS-16).
    pub fn named(&self, name: &str) -> Option<(&str, bool)> {
        self.tools
            .iter()
            .find(|tool| format!("{}:{}", tool.alias, tool.drawn.name) == name)
            .map(|tool| (tool.wire.as_str(), self.loaded.contains(&tool.wire)))
    }

    /// Takes the calls of the round about to run, by their position in the round, the wire name and
    /// the arguments the planner wrote, so a question about one call can list the later calls to
    /// the same tool.
    pub fn begin_round<'c>(&self, calls: impl IntoIterator<Item = (usize, &'c str, Value)>) {
        self.round.begin(
            calls
                .into_iter()
                .filter_map(|(index, wire, arguments)| {
                    self.find(wire).map(|(alias, tool)| {
                        (index, alias.to_string(), tool.to_string(), arguments)
                    })
                })
                .collect(),
        );
    }

    /// Names the position in the round of the call about to be dispatched, and none once it has
    /// been, so the call is found by its position and not by what it says.
    pub fn dispatching(&self, index: Option<usize>) {
        self.round.at(index);
    }

    /// Whether a deferred offer holds the tool under this wire name without having loaded it.
    pub fn is_unloaded(&self, wire: &str) -> bool {
        self.deferred
            && !self.loaded.iter().any(|loaded| loaded == wire)
            && self.tools.iter().any(|tool| tool.wire == wire)
    }

    /// Load the tool under this wire name, so [`Offer::functions`] holds its definition from now on.
    /// Nothing where this offer holds no such tool.
    pub fn load(&mut self, wire: &str) -> Option<Tool> {
        if !self.tools.iter().any(|tool| tool.wire == wire) {
            return None;
        }
        if !self.loaded.iter().any(|loaded| loaded == wire) {
            self.loaded.push(wire.to_string());
        }
        let mut only = self.clone();
        only.loaded = vec![wire.to_string()];
        only.functions().into_iter().next()
    }

    /// This offer less the tools of every server `held` names no grant for (SERVERS-9).
    pub fn holding(mut self, held: &CapabilitySet) -> Self {
        self.tools.retain(|tool| {
            held.contains(&Capability::McpCall(ServerAlias::new(tool.alias.as_str())))
        });
        self
    }

    /// Whether the declaration of the server `alias` says every call to it is asked about
    /// (SERVERS-21).
    pub fn asks_every_call(&self, alias: &str) -> bool {
        self.session.asks_every_call(alias)
    }

    /// The arguments of the tool `tool` of the server `alias` that its declaration says take a
    /// choice, where it names any (SERVERS-22).
    pub fn choice_arguments(&self, alias: &str, tool: &str) -> Option<&BTreeSet<String>> {
        self.session.choice_arguments(alias, tool)
    }

    /// The arguments of the tool `tool` of the server `alias` that are offered as a choice: those
    /// its declaration names, less those the server lists an argument `<argument>_ref` for itself
    /// (SERVERS-22). That argument is the server's, and a call that gives it is sent as written.
    fn choosable(&self, alias: &str, tool: &str) -> Vec<&String> {
        let Some(offered) = self
            .tools
            .iter()
            .find(|offered| offered.alias == alias && offered.drawn.name == tool)
        else {
            return Vec::new();
        };
        choosable(self.choice_arguments(alias, tool), &offered.drawn)
    }

    /// The tool offered under this wire name, as its alias and its word.
    pub fn find(&self, wire: &str) -> Option<(&str, &str)> {
        self.tools
            .iter()
            .find(|tool| tool.wire == wire)
            .map(|tool| (tool.alias.as_str(), tool.drawn.name.as_str()))
    }

    /// Each tool this offer holds, as a plan may name it (MANIFEST-15): the alias, the tool's word
    /// and the names of its arguments. A description is the server's prose and a plan needs none
    /// of it, so none is given.
    pub fn callable(&self) -> Vec<Callable> {
        self.tools
            .iter()
            .map(|tool| Callable {
                alias: tool.alias.clone(),
                tool: tool.drawn.name.clone(),
                arguments: tool
                    .drawn
                    .arguments
                    .iter()
                    .map(|argument| CallableArgument {
                        name: argument.name.clone(),
                        kind: argument.kind.clone(),
                        required: argument.required,
                    })
                    .collect(),
            })
            .collect()
    }

    fn description(&self, alias: &str, tool: &str) -> Option<String> {
        self.tools
            .iter()
            .find(|offered| offered.alias == alias && offered.drawn.name == tool)
            .and_then(|offered| offered.drawn.description.clone())
    }

    /// Whether this offers no tool.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

/// An offer for a test that only needs [`Offer::find`] to answer, built without reaching a
/// server. Each entry is the wire name a call is made under, the alias `find` answers with and
/// the tool's own word; the three are not derived from one another here, so a test can tell a
/// lookup that used the offer apart from one that fell back to parsing the wire name itself.
#[cfg(test)]
pub(crate) fn offer_for_test(entries: &[(&str, &str, &str)]) -> Offer {
    Offer {
        session: Session::new(Vec::new(), PathBuf::new(), None, false, Managed::default()),
        tools: entries
            .iter()
            .map(|(wire, alias, tool)| Offered {
                wire: wire.to_string(),
                alias: alias.to_string(),
                drawn: Drawn {
                    name: tool.to_string(),
                    description: None,
                    arguments: Vec::new(),
                },
            })
            .collect(),
        round: Round::default(),
        deferred: false,
        loaded: Vec::new(),
        reading: false,
    }
}

/// How a call to a server's tool ended, before it is told to the planner.
pub(crate) enum Called {
    /// The server answered, with content nobody vouched for: its text, and the pictures it sent
    /// that the transport took.
    Answered(ToolOutput),
    /// The tool ran and reported a failure, with the server's words about it.
    Failed(Labelled<String>),
    /// A reply arrived that this client could not take as a result: a JSON-RPC error, a reply it
    /// could not read, a line past the bound or an HTTP status that is not a success. The sentence
    /// is this process's own and holds nothing the server wrote.
    Unusable(String),
    /// The call was refused, declined or not asked about, was not made, or no reply to it arrived.
    /// The sentence is this process's own.
    Problem(String),
}

/// Where the planner is told a server's reply came from, which is also what the screen draws
/// beside it (MCP-15).
///
/// Built from the alias and the word the person read on the tool's list and from which arm of
/// [`Called`] the call ended in, which the transport settles from the protocol's own `isError`
/// field. Nothing the server wrote is in it. An answered call says the server reported no failure,
/// so a planner can tell a call the server completed from one it did not without reading the reply.
/// It says that and no more: it is the server's report and not a check that the effect happened.
pub(crate) fn origin_of_reply(name: &str, failed: bool) -> String {
    if failed {
        format!("what {name} said about its failure")
    } else {
        format!("what {name} returned with no failure reported")
    }
}

/// The argument of a call that the planner filled from a choice a processor made (MCP-14).
struct Filling<'a> {
    /// The argument the declaration says takes a choice, and the call fills.
    argument: &'a str,
    /// What the planner named the choice by: the argument and `_ref`.
    key: String,
    /// The reference to the choice.
    slot: SlotId,
    /// The choices of the turn making the call.
    slots: &'a mut SlotStore,
}

impl Filling<'_> {
    /// The number and where it came from, as the question draws the argument it will fill.
    fn drawn<S: Sink>(&self, policy: &mut Policy<'_, S>) -> Result<String, String> {
        policy
            .choice_for_display(&self.slot, self.slots)
            .map(|number| {
                t!(
                    mcp_call_choice_drawn,
                    number = number.as_str(),
                    reference = self.slot.as_str()
                )
            })
            .map_err(|denial| format!("refused: {denial}"))
    }
}

/// Which argument of this call the planner filled from a choice, if it did (MCP-14).
///
/// A call names one by writing `<argument>_ref` for an argument the declaration says takes a
/// choice. Which keys those are comes from the declaration and the planner's own keys; nothing
/// here reads a choice. A call that names one is refused, with nobody asked, where it also gives
/// the argument itself or names more than one choice, where a plan makes it (a plan holds no
/// choice), where its value is no string, where the context has met untrusted content, and where
/// the reference is not a choice nobody has used.
fn fill_from_a_choice<'a, S: Sink>(
    policy: &mut Policy<'_, S>,
    offer: &'a Offer,
    alias: &str,
    tool: &str,
    pairs: &Map<String, Value>,
    slots: Option<&'a mut SlotStore>,
) -> Result<Option<Filling<'a>>, String> {
    let named: Vec<&String> = offer
        .choosable(alias, tool)
        .into_iter()
        .filter(|argument| pairs.contains_key(&choice_key(argument)))
        .collect();
    let argument = match named.as_slice() {
        [] => return Ok(None),
        [argument] => *argument,
        _ => {
            return Err(format!(
                "error: {alias}:{tool} names more than one choice, and a call can be filled from \
                 one"
            ));
        }
    };
    let key = choice_key(argument);
    let Some(slots) = slots else {
        return Err(format!(
            "refused: {alias}:{tool} names a choice in {key}, and a plan holds no choice"
        ));
    };
    if pairs.contains_key(argument.as_str()) {
        return Err(format!(
            "error: {alias}:{tool} was given both {argument} and {key}: give one of the two"
        ));
    }
    let Some(reference) = pairs.get(&key).and_then(Value::as_str) else {
        return Err(format!(
            "error: {key} must be a reference such as \"ref:3\" to a choice a processor made"
        ));
    };
    let named = Labelled::new(
        reference.to_string(),
        bravebot_core::label::Label::untrusted_public(),
    );
    let slot = crate::tools::accept_named_reference(
        policy,
        slots,
        "mcp_call",
        &key,
        &named,
        crate::tools::NamesToList::Choices,
    )?;
    Ok(Some(Filling {
        argument,
        key,
        slot,
        slots,
    }))
}

/// Put one call to the person and make it (SERVERS-7).
///
/// A `deny` rule refuses it before anybody is asked. Then it asks, unless a rule allows the tool or
/// the person said at an earlier call to stop asking for it in this project; a call carrying the
/// person's private data asks whatever was said (PERM-9). The arguments reach the server only past
/// the endorsement the answer minted.
///
/// A call that names a choice a processor made, as `<argument>_ref` for an argument the server's
/// declaration says takes one, always asks. The question draws the number, and the number is put
/// into the argument only after the answer, by [`Policy::carry_a_choice`] (ROUTE-10, MCP-14).
/// `slots` holds the choices of the turn making the call, and a plan has none to give.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the policy, offer, egress, confirmer, reporter and the alias, tool and arguments of a call"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the gates of one call in the order they apply, each returning its own refusal"
)]
pub(crate) fn call<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    offer: &Offer,
    egress: &bravebot_net::Egress,
    confirmer: &mut C,
    reporter: &mut R,
    alias: &str,
    tool: &str,
    arguments: &Value,
    slots: Option<&mut SlotStore>,
) -> Called {
    let name = format!("{alias}:{tool}");
    if let Err(denial) = policy.before_mcp_call_rules(alias, tool) {
        return Called::Problem(format!(
            "refused: {denial}. Do not retry {name} and do not look for another route to what it \
             does: say in your reply what you needed from it."
        ));
    }
    let Some(pairs) = arguments.as_object() else {
        return Called::Problem(format!(
            "error: the arguments to {name} must be a JSON object"
        ));
    };
    // The later calls of this round to this tool, listed in the one question about this call.
    let later = offer.round.later(alias, tool);

    // The planner's own words, read as such: shown to the person and sent to the server, and
    // refused both where the context they were written in has met untrusted content.
    let written = Labelled::new(
        arguments.to_string(),
        bravebot_core::label::Label::untrusted_public(),
    );
    if let Err(denial) = policy.read_planner_argument("mcp_call", "arguments", &written) {
        return Called::Problem(format!("refused: {denial}"));
    }
    let labelled = policy.label_model_output("mcp_call", arguments.clone());

    let session = &offer.session;
    let filling = match fill_from_a_choice(policy, offer, alias, tool, pairs, slots) {
        Ok(filling) => filling,
        Err(refusal) => return Called::Problem(refusal),
    };
    // A call that names a choice is asked about whatever the declaration of the server says.
    let every_call = session.asks_every_call(alias) || filling.is_some();
    // An answer recorded for the tool before the declaration said so is not read.
    let standing = if every_call {
        McpStanding::AskedEveryCall
    } else if session.stands(alias, tool) {
        McpStanding::StoppedAsking
    } else {
        McpStanding::Nothing
    };
    // The label of the arguments and nothing of their bytes. A call asked because they are
    // private is not covered by any stored answer, so answer 2 would never stop the questions.
    let private = !labelled.label().is_public();
    // A call that names a choice, or that is asked whatever was said, is asked on its own.
    let batched = filling.is_none() && !every_call;
    // An earlier question of this round listed this call and was answered for it (SERVERS-7).
    // That is read first so the trail does not say a call was asked that nobody was asked about.
    let covered = match batched {
        true => offer.round.answered(private),
        false => None,
    };
    let asked = match covered {
        Some(approved) => {
            policy.record_covered_mcp_call(alias, tool, approved);
            true
        }
        None => policy.mcp_call_needs_approval(alias, tool, labelled.label(), standing),
    };
    if covered == Some(false) {
        policy.refuse_unapproved_call(alias, tool);
        return Called::Problem(format!(
            "refused: the user did not approve calling {name}. Do not retry the same \
             call; ask what they would prefer."
        ));
    }
    if asked && covered.is_none() {
        let mut shown: Vec<(String, String)> = pairs
            .iter()
            .map(|(key, value)| (key.clone(), value.to_string()))
            .collect();
        if let Some(filling) = &filling {
            match filling.drawn(policy) {
                Ok(drawn) => shown
                    .iter_mut()
                    .filter(|(key, _)| *key == filling.key)
                    .for_each(|pair| *pair = (filling.argument.to_string(), drawn.clone())),
                Err(refusal) => return Called::Problem(refusal),
            }
        }
        // The later calls of this round to this tool that this question also covers.
        let listed: Vec<(usize, &Value)> = match batched {
            true => later
                .iter()
                .filter(|(_, other)| {
                    other.as_object().is_some_and(|object| {
                        !offer
                            .choosable(alias, tool)
                            .iter()
                            .any(|argument| object.contains_key(&choice_key(argument)))
                    })
                })
                .map(|(index, other)| (*index, other))
                .collect(),
            false => Vec::new(),
        };
        let also: Vec<Vec<(String, String)>> = listed
            .iter()
            .filter_map(|(_, other)| other.as_object())
            .map(|object| {
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_string()))
                    .collect()
            })
            .collect();
        let request = McpCallRequest {
            alias: alias.to_string(),
            tool: tool.to_string(),
            arguments: shown,
            also,
            description: offer.description(alias, tool),
            may_stand: session.may_stand() && !every_call && !private,
            asked_for_private_arguments: private && !every_call,
            asked_every_call: every_call,
        };
        let answer = confirmer.confirm_mcp_call(&request);
        if !answer.unasked {
            offer.round.cover(
                &listed.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
                private,
                answer.decision == Decision::Approve,
            );
        }
        crate::tools::keep_the_note(
            policy,
            "call_server_tool",
            answer.decision == Decision::Approve,
            answer.note(),
        );
        let told = answer.note().is_some();
        let CallDecision {
            decision,
            stand,
            unasked,
            ..
        } = answer;
        if decision == Decision::Reject {
            policy.refuse_unapproved_call(alias, tool);
            if unasked {
                reporter.notice(t!(mcp_call_cannot_be_asked_here, tool = name.as_str()));
                return Called::Problem(format!(
                    "refused: every call to {alias} is put to the user, and this client has no \
                     way to ask, so nobody was asked. Do not retry {name}: say that it needs a \
                     client that can ask."
                ));
            }
            return Called::Problem(format!(
                "refused: the user did not approve calling {name}. Do not retry the same \
                 call{}",
                if told {
                    "."
                } else {
                    "; ask what they would prefer."
                }
            ));
        }
        if stand && request.may_stand {
            // The call was still approved; only the answer to stop asking was not kept.
            if let Err(error) = session.stand(alias, tool) {
                reporter.notice(t!(
                    mcp_call_not_recorded,
                    tool = name.as_str(),
                    error = error
                ));
            }
        }
    }

    policy.endorse_mcp_call(alias, tool);
    let carried = match filling {
        None => policy.before_mcp_call(alias, tool, labelled),
        Some(Filling {
            argument,
            slot,
            slots,
            ..
        }) => policy
            .carry_a_choice(alias, tool, labelled, &slot, slots)
            .map(|(arguments, number)| with_choice(arguments, argument, number)),
    };
    let arguments = match carried {
        Ok(arguments) => arguments,
        Err(denial) => return Called::Problem(format!("refused: {denial}")),
    };
    // A reply that pointed off the declaration is put to the person, and on a yes the call is made
    // once more, to where the server now is.
    let answered = following_a_move(
        policy,
        session,
        egress,
        confirmer,
        reporter,
        alias,
        |policy| session.call(policy, egress, alias, tool, arguments.clone()),
    );
    match answered {
        Ok(result) => Called::Answered(result),
        Err(McpError::ToolFailed { detail, .. }) => Called::Failed(detail),
        Err(error @ (McpError::Server { .. } | McpError::Unusable(_))) => {
            Called::Unusable(format!("error: calling {name} failed: {error}"))
        }
        Err(error) => Called::Problem(format!("error: calling {name} failed: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MCP-15: a reply the server did not report as failed says so, in the driver's own words, and
    /// a reply it reported as failed does not. The origin is built from the name alone, so it holds
    /// nothing the server wrote.
    #[test]
    fn a_reply_with_no_failure_reported_says_so_and_a_failed_one_does_not() {
        let answered = origin_of_reply("weather:get_forecast", false);
        assert_eq!(
            answered,
            "what weather:get_forecast returned with no failure reported"
        );
        let failed = origin_of_reply("weather:get_forecast", true);
        assert_eq!(failed, "what weather:get_forecast said about its failure");
        assert!(
            !failed.contains("no failure reported"),
            "a failed call was described as reporting no failure: {failed}"
        );
    }

    fn pairs(typed: &str) -> Vec<(String, String)> {
        arguments_of(typed).unwrap_or_else(|why| panic!("{typed:?} did not parse: {why:?}"))
    }

    fn pair(name: &str, value: &str) -> (String, String) {
        (name.to_string(), value.to_string())
    }

    /// An argument is a name, an equals sign and a value up to the next space, and a value with
    /// spaces in it is between double quotes. Nothing is read out of a value: a quoted `=` is
    /// text, and so is a word that looks like another argument.
    #[test]
    fn arguments_are_pairs_and_a_quoted_value_may_hold_spaces() {
        assert_eq!(pairs(""), []);
        assert_eq!(pairs("   "), []);
        assert_eq!(pairs("city=Paris"), [pair("city", "Paris")]);
        assert_eq!(
            pairs("  city=Paris   units=metric "),
            [pair("city", "Paris"), pair("units", "metric")]
        );
        assert_eq!(
            pairs(r#"topic="two words" note="a=b c=d""#),
            [pair("topic", "two words"), pair("note", "a=b c=d")]
        );
        assert_eq!(
            pairs("empty= next=x"),
            [pair("empty", ""), pair("next", "x")]
        );
        assert_eq!(pairs("path=a=b"), [pair("path", "a=b")]);
    }

    /// `\"` and `\\` stand for a quote and a backslash inside quotes, and any other backslash is
    /// itself, so a Windows path in quotes arrives as typed.
    #[test]
    fn a_quoted_value_unescapes_a_quote_and_a_backslash_and_nothing_else() {
        assert_eq!(
            pairs(r#"text="say \"hi\" \\ done""#),
            [pair("text", r#"say "hi" \ done"#)]
        );
        assert_eq!(
            pairs(r#"path="C:\work\notes""#),
            [pair("path", r"C:\work\notes")]
        );
    }

    /// Words that are not pairs are refused with the word the person typed, since the box is the
    /// only place they can be corrected, and a value is not guessed at the argument it might be for.
    #[test]
    fn words_that_are_not_arguments_are_refused_naming_the_word() {
        for (typed, why) in [
            ("Paris", Unparsed::NotAPair("Paris".to_string())),
            ("city=Paris Paris", Unparsed::NotAPair("Paris".to_string())),
            ("=Paris", Unparsed::BadName("=Paris".to_string())),
            ("a b=c", Unparsed::NotAPair("a".to_string())),
            ("ci/ty=Paris", Unparsed::BadName("ci/ty=Paris".to_string())),
            ("city=Paris city=Rome", Unparsed::Twice("city".to_string())),
            (r#"city="Paris"#, Unparsed::Quote("city".to_string())),
            (r#"city="Paris"x"#, Unparsed::Quote("city".to_string())),
        ] {
            assert_eq!(arguments_of(typed), Err(why), "{typed:?}");
        }
    }
}
