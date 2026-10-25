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
use crate::report::Reporter;
use crate::scratch::SessionScratch;
use crate::servers::EffectivePolicy;
use bravebot_aichat::protocol::{Tool, Usage};
use bravebot_config::mcp::{
    self as records, Approvals, Declaration, Declarations, Digest, Problem, Standing,
};
use bravebot_config::{Managed, Refusal, Rule};
use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_core::vetting::{Endorsed, Verdict};
use bravebot_i18n::t;
use bravebot_mcp::protocol::{WIRE_PREFIX, WIRE_SEPARATOR};
use bravebot_mcp::{
    HttpServer, Listing, McpError, McpResult, PromptListing, StdioServer, wire_name,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
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
        }
    }

    /// The same server, with the filters its declaration put on its list, which a list read again
    /// after the server announced a change is held to as the first was (MCP-12).
    pub fn filtered_by(mut self, tools: &[String], exclude_tools: &[String]) -> Self {
        self.tools = tools.to_vec();
        self.exclude_tools = exclude_tools.to_vec();
        self
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
    /// of a turn a person asked for, since that is where there is somebody to put the question to.
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
            let mut said = Said {
                reporter: &mut *reporter,
                notices: &mut settled.notices,
            };
            let (now, usage) =
                self.settle_one(policy, chat, confirmer, &mut said, mode, server, listing);
            settled.usage.add(usage);
            *held(&server.state) = now;
        }
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
    ) -> (State, Usage) {
        let alias = server.alias.as_str();
        let refused = listing.refused();
        let withheld = listing.withheld();
        let list = listing.list().clone();

        // Not made where every check is being bypassed, for the reason a vouch makes none there:
        // the mode answers this question yes, so nobody would read the word.
        let (verdict, reason, usage) = match mode != crate::PermissionMode::Bypass {
            true => {
                let spec = policy.before_vetting_a_tool_list(alias, list.clone());
                let checked = crate::vet::run(policy, chat, &mut *said.reporter, &spec);
                let reason = checked.reason.map(|reason| {
                    let proof = policy.authorise_display_release("what a check said about content");
                    reason.declassify(&proof)
                });
                (checked.verdict, reason, checked.usage)
            }
            false => (
                Verdict::Inconclusive("the check was not made"),
                None,
                Usage::default(),
            ),
        };

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
        (offered(promoted), usage)
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
            deferred: false,
            loaded: Vec::new(),
        }
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
        replace(&records::tools_file(directory), standing.to_text())
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
    ) -> McpResult<Labelled<String>> {
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

        let mut moved = HttpServer::new(alias, url.as_str()).starting_within(startup);
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
        if let (Some(directory), true) = (&self.0.directory, self.0.writable) {
            match record_a_move(directory, alias, &from, &declaration) {
                Ok(()) => reporter.notice(t!(mcp_move_moved, alias = alias)),
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
        *held(&server.declaration) = declaration.digest();
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

/// Rewrite `alias`'s declaration from `from` to `to`, and approve `to` in the same write
/// (SERVERS-11).
///
/// The approval is recorded in the approvals file whatever answered for the declaration it
/// replaces: a project a person said to use every server in answers for what a checkout requests,
/// and never for where a server went. Declarations first, then approvals, as every rewrite of the
/// pair is, so a write that stops between them leaves a declaration nobody approved, which asks.
pub fn record_a_move(
    directory: &Path,
    alias: &str,
    from: &Digest,
    to: &Declaration,
) -> Result<(), Unmoved> {
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
    // Where it is reached is what moved: the bounds the person gave it are still theirs.
    let to = &to.clone().timing(current.timeouts());
    let mut approvals = Approvals::to_change(directory).map_err(|why| {
        Unmoved::NotWritten(t!(
            mcp_unreadable,
            path = shown(&records::approvals_file(directory).display().to_string()),
            reason = unreadable_record(&why)
        ))
    })?;
    declarations.insert(alias, to);
    approvals.keep_only(&declarations);
    approvals.approve(alias, to.digest());
    replace(
        &records::declarations_file(directory),
        declarations.to_text(),
    )
    .and_then(|()| replace(&records::approvals_file(directory), approvals.to_text()))
    .map_err(|error| Unmoved::NotWritten(error.to_string()))
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

/// Write `text` over `path` through a temporary file beside it, so an interrupted write leaves the
/// file as it was.
/// Write a record whole, through a file of its own so a reader never sees half of it.
///
/// The temporary file is named for this process and this write, since two sessions answering at
/// once would otherwise write into one file and rename whatever the pair of them left there.
pub fn replace(path: &Path, text: impl AsRef<[u8]>) -> std::io::Result<()> {
    static WRITES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let write = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".{}.{write}.tmp", std::process::id()));
    let temporary = PathBuf::from(temporary);
    let written = crate::home::write_file(&temporary, text.as_ref())
        .and_then(|()| std::fs::rename(&temporary, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
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

/// The tools one turn offers, and the session they are called through.
#[derive(Debug, Clone)]
pub struct Offer {
    session: Session,
    tools: Vec<Offered>,
    /// Whether the tools are offered by name only until one is loaded (SERVERS-16).
    deferred: bool,
    /// The wire names loaded so far this turn, in the order they were loaded. Always names this
    /// offer holds, since [`Offer::load`] takes a name only by finding it in `tools`.
    loaded: Vec<String>,
}

impl Offer {
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
                let properties: Map<String, Value> = tool
                    .drawn
                    .arguments
                    .iter()
                    .map(|argument| (argument.name.clone(), argument.schema()))
                    .collect();
                let required: Vec<&str> = tool
                    .drawn
                    .arguments
                    .iter()
                    .filter(|argument| argument.required)
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

    /// The tool offered under this wire name, as its alias and its word.
    pub fn find(&self, wire: &str) -> Option<(&str, &str)> {
        self.tools
            .iter()
            .find(|tool| tool.wire == wire)
            .map(|tool| (tool.alias.as_str(), tool.drawn.name.as_str()))
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
        deferred: false,
        loaded: Vec::new(),
    }
}

/// How a call to a server's tool ended, before it is told to the planner.
pub(crate) enum Called {
    /// The server answered, with content nobody vouched for.
    Answered(Labelled<String>),
    /// The tool ran and reported a failure, with the server's words about it.
    Failed(Labelled<String>),
    /// Nothing reached the server, or nothing usable came back. The sentence is this process's own.
    Problem(String),
}

/// Put one call to the person and make it (SERVERS-7).
///
/// A `deny` rule refuses it before anybody is asked. Then it asks, unless a rule allows the tool or
/// the person said at an earlier call to stop asking for it in this project; a call carrying the
/// person's private data asks whatever was said (PERM-9). The arguments reach the server only past
/// the endorsement the answer minted.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the policy, offer, egress, confirmer, reporter and the alias, tool and arguments of a call"
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
    let standing = session.stands(alias, tool);
    if policy.mcp_call_needs_approval(alias, tool, labelled.label(), standing) {
        let request = McpCallRequest {
            alias: alias.to_string(),
            tool: tool.to_string(),
            arguments: pairs
                .iter()
                .map(|(key, value)| (key.clone(), value.to_string()))
                .collect(),
            description: offer.description(alias, tool),
            may_stand: session.may_stand(),
        };
        let answer = confirmer.confirm_mcp_call(&request);
        crate::tools::keep_the_note(
            policy,
            "call_server_tool",
            answer.decision == Decision::Approve,
            answer.note(),
        );
        let told = answer.note().is_some();
        let CallDecision {
            decision, stand, ..
        } = answer;
        if decision == Decision::Reject {
            policy.refuse_unapproved_call(alias, tool);
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
    let arguments = match policy.before_mcp_call(alias, tool, labelled) {
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
        Err(error) => Called::Problem(format!("error: calling {name} failed: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
