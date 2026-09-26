//! The MCP servers a session reached, as a turn meets them (SERVERS-7, SERVERS-8, SERVERS-13).
//!
//! A server's list of tools is the server's own words, so nothing of it reaches the planner until
//! a person has read it as it is drawn and said yes, which is road 1 of issue #83. A yes records a
//! digest of the list beside the server's approval, so the same list asks nothing the next session
//! and a list that changed asks again. Every call to a tool on a list somebody vouched for is then
//! put to the person with three answers, the second of which stops asking for that one tool in that
//! one project.
//!
//! The servers themselves were started before the session opened, by whoever assembled it. What
//! this holds is each one's connection, the list it answered with, and what has been decided about
//! that list.

use crate::confirm::{
    CallDecision, Confirmer, Decision, ListedTool, McpCallRequest, ToolListRequest,
};
use crate::processor::Chat;
use crate::report::Reporter;
use bravebot_aichat::protocol::{Tool, Usage};
use bravebot_config::mcp::{self as records, Approvals, Digest, Standing};
use bravebot_core::capability::ServerAlias;
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_core::vetting::{Endorsed, Verdict};
use bravebot_i18n::t;
use bravebot_mcp::{HttpServer, Listing, McpResult, StdioServer, wire_name};
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
    declaration: Digest,
}

impl Reached {
    /// `declaration` is the digest of the declaration the server was started from, which a vouch
    /// for its list is recorded beside.
    pub fn new(connection: Connection, listing: Listing, declaration: Digest) -> Self {
        Self {
            connection,
            listing,
            declaration,
        }
    }

    /// The alias the person gave the server.
    pub fn alias(&self) -> &str {
        self.listing.alias()
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
    declaration: Digest,
    /// Whether this is a process the session confined, read once so nothing asking waits on a call.
    local: bool,
    /// Locked for a question's length only, so a screen asking how things stand is never kept
    /// waiting on a call.
    state: Mutex<State>,
    /// Locked for one call at a time.
    connection: Mutex<Connection>,
}

struct Shared {
    servers: Vec<Server>,
    /// The workspace root, which answer 2 of a call records and is read against. It moves with the
    /// session, since a standing answer is about the project a person is in.
    project: Mutex<PathBuf>,
    /// The state directory, where there is one.
    directory: Option<PathBuf>,
    /// Whether anything may be written into it, which an incognito session may not.
    writable: bool,
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
    /// this session may write into it.
    pub fn new(
        reached: Vec<Reached>,
        project: PathBuf,
        directory: Option<PathBuf>,
        writable: bool,
    ) -> Self {
        let servers = reached
            .into_iter()
            .map(|reached| Server {
                alias: reached.alias().to_string(),
                declaration: reached.declaration,
                local: matches!(reached.connection, Connection::Stdio(_)),
                state: Mutex::new(State::Unasked(reached.listing)),
                connection: Mutex::new(reached.connection),
            })
            .collect();
        Self(Arc::new(Shared {
            servers,
            project: Mutex::new(project),
            directory,
            writable,
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
        self.0.servers.iter().any(|server| server.local)
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

    /// Settle every list nobody has been asked about yet (SERVERS-8, SERVERS-13).
    ///
    /// A list that is the one somebody vouched for under this declaration before is offered with
    /// nobody asked. Every other list is checked, unless every check is being bypassed, and put to
    /// the person as it is drawn. A yes promotes it, and records its digest where the session may
    /// write and a person gave the answer; a no offers none of its tools for the rest of the
    /// session, and a turn stopped at the question leaves it unasked. Settled at the start of a turn a person asked for, since that is where there is
    /// somebody to put the question to.
    pub fn settle<S: Sink, C: Confirmer, R: Reporter>(
        &self,
        policy: &mut Policy<'_, S>,
        chat: &mut Chat<'_>,
        confirmer: &mut C,
        reporter: &mut R,
        mode: crate::PermissionMode,
    ) -> Settled {
        let mut settled = Settled::default();
        for server in &self.0.servers {
            if stopped(chat) {
                break;
            }
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

    #[allow(clippy::too_many_arguments)]
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
        let recorded = self
            .0
            .directory
            .as_deref()
            .and_then(|directory| Approvals::read(directory).vouched_list(&server.declaration));

        let (list, changed) = match recorded {
            Some(recorded) => match policy.promote_a_recorded_tool_list(
                alias,
                listing.list().clone(),
                &recorded.to_string(),
                |text| Digest::of_list(text).to_string(),
            ) {
                Ok(promoted) => return (offered(promoted), Usage::default()),
                Err(list) => (list, true),
            },
            None => (listing.list().clone(), false),
        };

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
            changed,
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
        // What bypassing answered records nothing, so the next session that asks anybody asks.
        if by == Endorsed::ByAPerson {
            self.record_list(said, server, &promoted);
        }
        (offered(promoted), usage)
    }

    /// Record a vouch for this list beside the declaration it was listed by, where the session may.
    fn record_list<R: Reporter>(
        &self,
        said: &mut Said<'_, R>,
        server: &Server,
        list: &Labelled<String>,
    ) {
        let (Some(directory), true) = (&self.0.directory, self.0.writable) else {
            return;
        };
        let Ok(text) = list.clone().into_trusted() else {
            return;
        };
        let written = Approvals::to_change(directory)
            .map_err(|why| unreadable_record(&why))
            .and_then(|mut approvals| {
                approvals.vouch_list(server.declaration, Digest::of_list(&text));
                replace(&records::approvals_file(directory), approvals.to_text())
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = written {
            said.notice(t!(
                mcp_tools_not_recorded,
                alias = server.alias.as_str(),
                error = error
            ));
        }
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

    fn call<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        egress: &bravebot_net::Egress,
        alias: &str,
        tool: &str,
        arguments: Value,
    ) -> McpResult<Labelled<String>> {
        let Some(server) = self.0.servers.iter().find(|server| server.alias == alias) else {
            return Err(bravebot_mcp::McpError::Transport(format!(
                "no server {alias} was reached"
            )));
        };
        match &mut *held(&server.connection) {
            Connection::Stdio(server) => server.call_tool(policy, tool, arguments),
            Connection::Http(server) => server.call_tool(policy, egress, tool, arguments),
        }
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
}

impl Offer {
    /// The functions a backend is sent, one per tool.
    ///
    /// The description is the server's words, which a person read and let through, so it is sent
    /// behind a margin and after a sentence of this process's own saying whose they are. It is a
    /// function's description and never a line of the system prompt (SERVERS-8).
    pub fn functions(&self) -> Vec<Tool> {
        self.tools
            .iter()
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
#[allow(clippy::too_many_arguments)]
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
        let CallDecision { decision, stand } = confirmer.confirm_mcp_call(&request);
        if decision == Decision::Reject {
            return Called::Problem(format!(
                "refused: the user did not approve calling {name}. Do not retry the same call; ask \
                 what they would prefer."
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
    match session.call(policy, egress, alias, tool, arguments) {
        Ok(result) => Called::Answered(result),
        Err(bravebot_mcp::McpError::ToolFailed { detail, .. }) => Called::Failed(detail),
        Err(error) => Called::Problem(format!("error: calling {name} failed: {error}")),
    }
}
