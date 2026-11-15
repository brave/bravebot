//! `/mcp`: the MCP servers a session holds, listed, started again, turned off, or removed (SERVERS-19).
//!
//! A person's word and nobody else's: the line is dispatched from the input box like every other
//! command (CMD-1), and what a server said is never read to decide anything here. The person names
//! the alias, and what is started is what the person's own declarations say it is, through the road
//! a session takes as it opens, so a question that road asks is asked here and by the same asker.

use bravebot_agent::mcp::Offering;
use bravebot_agent::servers::{self, Asker, Asking, Machine};
use bravebot_config::Settings;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use std::path::Path;

use crate::command_table::{MCP_DISABLE, MCP_LIST, MCP_RECONNECT, MCP_REMOVE};
use crate::state::{Servers, Session};

/// The terminal's side of the questions a start puts: drawn over the screen, answered by key.
pub struct TerminalAsker<'t, B: Backend> {
    pub terminal: &'t mut Terminal<B>,
}

impl<B: Backend> Asker for TerminalAsker<'_, B> {
    fn anybody_there(&self) -> bool {
        true
    }

    fn ask_to_start(&mut self, question: &servers::Question<'_>) -> servers::Answer {
        crate::confirm::ask_to_start(self.terminal, question)
    }

    fn ask_to_move(&mut self, request: &bravebot_agent::confirm::MoveRequest) -> bool {
        crate::confirm::ask_move(self.terminal, request).decision()
            == bravebot_agent::confirm::Decision::Approve
    }
}

/// Carry out `argument`, the words typed after `/mcp`, saying what happened in the transcript.
///
/// `settings` says which servers this session requests and `root` is the directory it works in.
/// `asking` is who answers a question a start puts, as at the session's opening (SERVERS-13).
pub fn run<A: Asker + ?Sized>(
    session: &mut Session,
    settings: &Settings,
    root: &Path,
    argument: &str,
    asking: Asking,
    asker: &mut A,
) {
    run_on(
        session,
        settings,
        root,
        argument,
        asking,
        asker,
        &Machine::current(),
    );
}

/// As [`run`], reading the declarations and approvals `machine` holds.
fn run_on<A: Asker + ?Sized>(
    session: &mut Session,
    settings: &Settings,
    root: &Path,
    argument: &str,
    asking: Asking,
    asker: &mut A,
    machine: &Machine,
) {
    run_with(
        session,
        settings,
        root,
        argument,
        asking,
        asker,
        &Ways {
            machine,
            programs: &through_the_program,
        },
    );
}

/// What `/mcp` reads and does outside the session: the machine's declarations and approvals, and the
/// program that writes them.
struct Ways<'a> {
    machine: &'a Machine,
    programs: Programs<'a>,
}

/// Run `bravebot` with the arguments given, in the directory given, and say what came of it.
type Programs<'a> = &'a dyn Fn(&Path, &[&str]) -> Ran;

/// What a run of the program came to: whether it succeeded, and the lines it printed.
struct Ran {
    succeeded: bool,
    lines: Vec<String>,
}

fn run_with<A: Asker + ?Sized>(
    session: &mut Session,
    settings: &Settings,
    root: &Path,
    argument: &str,
    asking: Asking,
    asker: &mut A,
    ways: &Ways<'_>,
) {
    let machine = ways.machine;
    let words: Vec<&str> = argument.split_whitespace().collect();
    match words.as_slice() {
        [] => list(session, settings),
        [word] if *word == MCP_LIST => list(session, settings),
        [word] if *word == MCP_RECONNECT => {
            reconnect(session, settings, root, None, asking, asker, machine)
        }
        [word, alias] if *word == MCP_RECONNECT => {
            reconnect(session, settings, root, Some(alias), asking, asker, machine)
        }
        [word] if *word == MCP_DISABLE || *word == MCP_REMOVE => {
            session.note(t!(mcp_command_needs_an_alias, command = *word))
        }
        [word, pattern]
            if *word == MCP_DISABLE && bravebot_config::mcp::is_alias_pattern(pattern) =>
        {
            disable_matching(session, settings, pattern)
        }
        [word, pattern]
            if *word == MCP_REMOVE && bravebot_config::mcp::is_alias_pattern(pattern) =>
        {
            remove_matching(session, settings, root, pattern, ways)
        }
        [word, alias] if *word == MCP_DISABLE => disable(session, settings, alias),
        [word, alias] if *word == MCP_REMOVE => remove(session, root, alias, ways),
        _ => session.note(t!(mcp_command_usage)),
    }
}

/// The aliases this session requests, each once.
fn requested(settings: &Settings) -> Vec<String> {
    let mut aliases: Vec<String> = Vec::new();
    for (_, alias) in servers::requested_by(settings) {
        if !aliases.contains(&alias) {
            aliases.push(alias);
        }
    }
    aliases
}

/// What the session holds now, by alias.
fn held(session: &Session) -> Vec<String> {
    session
        .servers
        .session
        .as_ref()
        .map(bravebot_agent::mcp::Session::aliases)
        .unwrap_or_default()
}

/// Make what `/status` and the panel read say what the session holds.
fn refresh(servers: &mut Servers) {
    servers.started = servers
        .session
        .as_ref()
        .map(bravebot_agent::mcp::Session::aliases)
        .unwrap_or_default();
    servers.confined = servers
        .session
        .as_ref()
        .is_some_and(bravebot_agent::mcp::Session::confined);
}

fn list(session: &mut Session, settings: &Settings) {
    let running: Vec<(String, Offering)> = session
        .servers
        .session
        .as_ref()
        .map(bravebot_agent::mcp::Session::offering)
        .unwrap_or_default();
    let others: Vec<String> = requested(settings)
        .into_iter()
        .filter(|alias| running.iter().all(|(held, _)| held != alias))
        .collect();
    if running.is_empty() && others.is_empty() {
        session.note(t!(mcp_command_none));
        return;
    }
    if !running.is_empty() {
        let manifest_only = session
            .servers
            .session
            .as_ref()
            .map(crate::status::manifest_only)
            .unwrap_or_default();
        session.note(t!(mcp_command_running));
        for server in running {
            session.note(format!(
                "  {}",
                crate::status::offering(std::slice::from_ref(&server), &manifest_only)
            ));
        }
    }
    if !others.is_empty() {
        session.note(t!(mcp_command_not_running));
        for alias in others {
            let row = match session.servers.disabled.contains(&alias) {
                true => t!(mcp_command_off, alias = alias.as_str()),
                false => t!(mcp_command_not_connected, alias = alias.as_str()),
            };
            session.note(format!("  {row}"));
        }
    }
    session.note(t!(mcp_command_remove_hint));
}

fn reconnect<A: Asker + ?Sized>(
    session: &mut Session,
    settings: &Settings,
    root: &Path,
    named: Option<&str>,
    asking: Asking,
    asker: &mut A,
    machine: &Machine,
) {
    let running = held(session);
    let wanted = requested(settings);
    let targets: Vec<String> = match named {
        Some(alias) if running.iter().any(|held| held == alias) => {
            session.note(t!(mcp_command_already_running, alias = alias));
            return;
        }
        Some(alias) if !wanted.iter().any(|requested| requested == alias) => {
            session.note(t!(mcp_command_not_requested, alias = alias));
            return;
        }
        Some(alias) => vec![alias.to_string()],
        None => wanted
            .into_iter()
            .filter(|alias| !running.contains(alias) && !session.servers.disabled.contains(alias))
            .collect(),
    };
    if targets.is_empty() {
        session.note(t!(mcp_command_nothing_to_reconnect));
        return;
    }
    session
        .servers
        .disabled
        .retain(|off| !targets.contains(off));

    let reached = servers::reconnect_on(
        settings,
        root,
        &targets,
        machine,
        asking,
        asker,
        bravebot_sandbox::Stream::Null,
    );
    for note in &reached.notes {
        session.note(note.clone());
    }
    let Some(started) = reached.session() else {
        return;
    };
    for alias in started.aliases() {
        session.note(t!(mcp_command_reconnected, alias = alias.as_str()));
    }
    session.servers.session = Some(match session.servers.session.take() {
        Some(kept) => kept.joined_by(&started),
        None => started,
    });
    refresh(&mut session.servers);
}

/// Turn off each server the session holds or requests whose alias `pattern` names, as `disable`
/// does for one, in alphabetical order.
fn disable_matching(session: &mut Session, settings: &Settings, pattern: &str) {
    let mut named: Vec<String> = held(session);
    for alias in requested(settings) {
        if !named.contains(&alias) {
            named.push(alias);
        }
    }
    named.retain(|alias| bravebot_config::mcp::alias_matches(pattern, alias));
    named.sort();
    if named.is_empty() {
        session.note(t!(
            mcp_command_no_match,
            pattern = crate::render::printable(pattern)
        ));
        return;
    }
    for alias in named {
        disable(session, settings, &alias);
    }
}

fn disable(session: &mut Session, settings: &Settings, alias: &str) {
    if session.servers.disabled.iter().any(|off| off == alias) {
        session.note(t!(mcp_command_already_off, alias = alias));
        return;
    }
    let running = held(session).iter().any(|held| held == alias);
    if !running
        && !requested(settings)
            .iter()
            .any(|requested| requested == alias)
    {
        session.note(t!(mcp_command_not_requested, alias = alias));
        return;
    }
    if let Some(kept) = session.servers.session.take() {
        session.servers.session = kept.without(alias);
    }
    session.servers.disabled.push(alias.to_string());
    refresh(&mut session.servers);
    session.note(t!(mcp_command_disabled, alias = alias));
}

/// Take `alias` out of every settings file that asks for it and delete its declaration and approval,
/// so no session starts it, and stop it in this one.
///
/// `bravebot mcp disable` and `bravebot mcp remove` do the writing, run as the program itself, since
/// the command line is the one writer of a declaration and of the settings files that request one
/// (layering). Typing `/mcp remove` is the person's word for it, as typing those is. Nothing a server
/// said is read: the person names the alias, which has to be one, so it cannot be taken for a flag.
/// Refused in an incognito session, which writes nothing, and the program is not asked, since it
/// cannot know the session is incognito.
fn remove(session: &mut Session, root: &Path, alias: &str, ways: &Ways<'_>) {
    if !bravebot_config::mcp::is_alias(alias) {
        session.note(t!(
            mcp_command_not_an_alias,
            alias = crate::render::printable(alias)
        ));
        return;
    }
    if may_remove(session, ways) {
        remove_one(session, root, alias, ways);
    }
}

/// Whether anything may be removed here, saying why not where it may not.
fn may_remove(session: &mut Session, ways: &Ways<'_>) -> bool {
    let home = &ways.machine.home;
    if home.directory.is_none() {
        session.note(t!(mcp_command_no_state_directory));
        return false;
    }
    if !home.writable {
        session.note(t!(mcp_not_while_incognito, command = MCP_REMOVE));
        return false;
    }
    true
}

/// Remove each server whose alias `pattern` names, as `remove` does for one, among the aliases the
/// person declared, the settings request, and the session holds.
///
/// The pattern is matched against those names and nothing else, so it can name no more than the
/// person's own files and session hold, and each alias reaches the program only as an alias. A
/// removal that fails is said, and the next alias is still tried.
fn remove_matching(
    session: &mut Session,
    settings: &Settings,
    root: &Path,
    pattern: &str,
    ways: &Ways<'_>,
) {
    if !may_remove(session, ways) {
        return;
    }
    let mut named: Vec<String> = held(session);
    let declared = ways
        .machine
        .home
        .directory
        .as_deref()
        .and_then(|directory| bravebot_config::mcp::Declarations::read(directory).ok())
        .map(|declarations| {
            declarations
                .entries()
                .into_iter()
                .map(|entry| entry.alias)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for alias in requested(settings).into_iter().chain(declared) {
        if !named.contains(&alias) {
            named.push(alias);
        }
    }
    named.retain(|alias| {
        bravebot_config::mcp::is_alias(alias) && bravebot_config::mcp::alias_matches(pattern, alias)
    });
    named.sort();
    if named.is_empty() {
        session.note(t!(
            mcp_command_no_match,
            pattern = crate::render::printable(pattern)
        ));
        return;
    }
    for alias in named {
        remove_one(session, root, &alias, ways);
    }
}

/// `remove` for an alias already checked and a machine already found writable.
fn remove_one(session: &mut Session, root: &Path, alias: &str, ways: &Ways<'_>) {
    // The request first: a declaration removed under a request that stays is reported at the start
    // of every session as a server nobody declared.
    let withdrawn = (ways.programs)(root, &["mcp", MCP_DISABLE, alias]);
    for line in &withdrawn.lines {
        session.note(crate::render::printable(line));
    }
    // A request that stays would be left under a declaration that goes.
    if !withdrawn.succeeded {
        return;
    }
    let removed = (ways.programs)(root, &["mcp", MCP_REMOVE, alias]);
    for line in &removed.lines {
        session.note(crate::render::printable(line));
    }
    if !removed.succeeded {
        return;
    }
    if let Some(kept) = session.servers.session.take() {
        session.servers.session = kept.without(alias);
    }
    session.servers.disabled.retain(|off| off != alias);
    refresh(&mut session.servers);
}

/// Run this program with `args` in `root`, as a person typing them at a shell would.
///
/// No input, so a question it could put is answered no: `disable` and `remove` put none. What it
/// printed and what it complained of are both kept, in that order.
fn through_the_program(root: &Path, args: &[&str]) -> Ran {
    // This program started again, with the environment it has, as `doctor` is for a bug report.
    let ran = bravebot_sandbox::programs::Resolved::own_executable().and_then(|me| {
        bravebot_sandbox::programs::start(
            &me,
            bravebot_sandbox::programs::Inherits::All,
            bravebot_sandbox::programs::SearchPath::Inherited,
            // This program started again for the person; it keeps the environment they set (SANDBOX-30).
            bravebot_sandbox::programs::CodeSelectors::Kept,
        )
        .args(args)
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .output()
    });
    match ran {
        Ok(output) => Ran {
            succeeded: output.status.success(),
            lines: [output.stdout, output.stderr]
                .iter()
                .flat_map(|text| {
                    String::from_utf8_lossy(text)
                        .lines()
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .collect(),
        },
        Err(error) => Ran {
            succeeded: false,
            lines: vec![error.to_string()],
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::state::Speaker;

    /// Nobody to ask: a command that puts a question here is a command that reached the road a start
    /// takes, which none of these may.
    pub(crate) struct Never;

    impl Asker for Never {
        fn anybody_there(&self) -> bool {
            panic!("a question was put")
        }
        fn ask_to_start(&mut self, _: &servers::Question<'_>) -> servers::Answer {
            panic!("a question was put")
        }
        fn ask_to_move(&mut self, _: &bravebot_agent::confirm::MoveRequest) -> bool {
            panic!("a question was put")
        }
    }

    /// Settings for a directory whose checkout requests `aliases`.
    fn requesting(name: &str, aliases: &[&str]) -> (Settings, std::path::PathBuf) {
        let directory = tempfile::Builder::new()
            .prefix(name)
            .tempdir()
            .expect("directory")
            .keep();
        std::fs::create_dir_all(directory.join(".bravebot")).expect("settings directory");
        std::fs::write(
            directory.join(".bravebot/settings.json"),
            serde_json::json!({ "mcp": { "request": aliases } }).to_string(),
        )
        .expect("settings");
        (Settings::layered(None, Some(&directory), None), directory)
    }

    /// What the command said, one note after another.
    fn said(session: &Session) -> Vec<String> {
        session
            .transcript
            .iter()
            .filter(|entry| entry.speaker == Speaker::System)
            .map(|entry| entry.text.clone())
            .collect()
    }

    fn typed(session: &mut Session, settings: &Settings, root: &Path, argument: &str) {
        run(
            session,
            settings,
            root,
            argument,
            Asking::Person,
            &mut Never,
        );
    }

    /// The list says what each requested server is, so a person who sees a server missing from the
    /// session finds it named with the way to try it again, and one they turned off says so.
    #[test]
    fn the_list_names_each_requested_server_and_how_to_start_the_one_that_is_not_running() {
        let (settings, root) = requesting("mcp-list", &["weather", "docs"]);
        let mut session = Session::new("none");
        session.servers.disabled.push("docs".to_string());

        typed(&mut session, &settings, &root, "");

        assert_eq!(
            said(&session),
            vec![
                t!(mcp_command_not_running).to_string(),
                format!("  {}", t!(mcp_command_not_connected, alias = "weather")),
                format!("  {}", t!(mcp_command_off, alias = "docs")),
                t!(mcp_command_remove_hint).to_string(),
            ]
        );
        let again = said(&session).len();
        typed(&mut session, &settings, &root, "list");
        assert_eq!(said(&session)[again..], said(&session)[..again]);
    }

    #[test]
    fn the_list_says_so_where_nothing_is_requested_or_running() {
        let (settings, root) = requesting("mcp-none", &[]);
        let mut session = Session::new("none");

        typed(&mut session, &settings, &root, "list");

        assert_eq!(said(&session), vec![t!(mcp_command_none).to_string()]);
    }

    /// A word that is not one of the three, and a word missing what it needs, change nothing and say
    /// what the command takes. Neither reaches a start, so no question is put.
    #[test]
    fn a_word_it_does_not_know_and_a_missing_alias_change_nothing() {
        let (settings, root) = requesting("mcp-usage", &["weather"]);
        for (argument, reply) in [
            ("restart weather", t!(mcp_command_usage).to_string()),
            ("reconnect weather docs", t!(mcp_command_usage).to_string()),
            (
                "disable",
                t!(mcp_command_needs_an_alias, command = MCP_DISABLE).to_string(),
            ),
            (
                "remove",
                t!(mcp_command_needs_an_alias, command = MCP_REMOVE).to_string(),
            ),
        ] {
            let mut session = Session::new("none");

            typed(&mut session, &settings, &root, argument);

            assert_eq!(said(&session), vec![reply], "{argument}");
            assert!(session.servers.disabled.is_empty(), "{argument}");
        }
    }

    /// A server the settings do not request is not started by being named: what a checkout
    /// requests is what a session may start (SERVERS-2), and the command adds nothing to it.
    #[test]
    fn a_server_nothing_requests_is_neither_started_nor_turned_off() {
        let (settings, root) = requesting("mcp-unrequested", &["weather"]);
        for argument in ["reconnect mail", "disable mail"] {
            let mut session = Session::new("none");

            typed(&mut session, &settings, &root, argument);

            assert_eq!(
                said(&session),
                vec![t!(mcp_command_not_requested, alias = "mail").to_string()],
                "{argument}"
            );
            assert!(session.servers.disabled.is_empty(), "{argument}");
        }
    }

    /// Turning off a requested server that is not running keeps a bare `reconnect` from starting
    /// it, and turning it off twice says it is already off. Naming it to `reconnect` clears the
    /// turn-off in `a_start_the_person_refuses_joins_nothing_and_clears_the_turn_off`.
    #[test]
    fn a_server_turned_off_stays_off_until_it_is_named() {
        let (settings, root) = requesting("mcp-off", &["weather"]);
        let mut session = Session::new("none");

        typed(&mut session, &settings, &root, "disable weather");
        typed(&mut session, &settings, &root, "disable weather");
        typed(&mut session, &settings, &root, "reconnect");

        assert_eq!(
            said(&session),
            vec![
                t!(mcp_command_disabled, alias = "weather").to_string(),
                t!(mcp_command_already_off, alias = "weather").to_string(),
                t!(mcp_command_nothing_to_reconnect).to_string(),
            ]
        );
        assert_eq!(session.servers.disabled, vec!["weather".to_string()]);
    }

    /// What a start put to the person, answered with `answer` each time.
    struct Answering {
        answer: servers::Answer,
        asked: usize,
    }

    impl Asker for Answering {
        fn anybody_there(&self) -> bool {
            true
        }
        fn ask_to_start(&mut self, _: &servers::Question<'_>) -> servers::Answer {
            self.asked += 1;
            self.answer
        }
        fn ask_to_move(&mut self, _: &bravebot_agent::confirm::MoveRequest) -> bool {
            false
        }
    }

    /// A server answering the handshake and the list, for as long as the test runs.
    fn serve(alias: &'static str) -> String {
        use std::io::{BufRead, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("addr").port()
        );
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let request: serde_json::Value =
                    serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
                let result = match request.get("method").and_then(serde_json::Value::as_str) {
                    Some("initialize") => serde_json::json!({
                        "protocolVersion": "2025-06-18",
                        "capabilities": {"prompts": {}},
                        "serverInfo": {"name": alias, "version": "1"},
                    }),
                    Some("tools/list") => serde_json::json!({"tools": [{
                        "name": "lookup",
                        "description": "Look something up.",
                        "inputSchema": {"type": "object", "properties": {}},
                    }]}),
                    Some("prompts/list") => serde_json::json!({"prompts": [{
                        "name": "summarise_diff",
                        "description": "Summarise the staged diff.",
                        "arguments": [
                            {"name": "depth"},
                            {"name": "file", "required": true},
                            {"name": "style", "required": true},
                        ],
                    }]}),
                    _ => serde_json::json!({}),
                };
                let reply = serde_json::json!({
                    "jsonrpc": "2.0", "id": request.get("id"), "result": result
                })
                .to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
            }
        });
        url
    }

    /// `alias` after its handshake and its list, as a session that started it holds it.
    fn reached(alias: &'static str) -> bravebot_agent::mcp::Reached {
        use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
        use bravebot_core::event::RecordingSink;
        use bravebot_core::policy::{Policy, ReleasePlan, Routing};
        let mut server = bravebot_mcp::HttpServer::new(alias, serve(alias));
        let egress = bravebot_net::Egress::new();
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "reach an MCP server");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([
                Capability::WebFetch,
                Capability::McpCall(ServerAlias::new(alias)),
            ]),
            &mut sink,
        )
        .expect("policy");
        server
            .initialize(&mut policy, &egress, "bravebot", "0")
            .expect("handshake");
        let listing = server.list_tools(&mut policy, &egress).expect("listed");
        let prompts = server
            .list_prompts(&mut policy, &egress)
            .expect("prompts listed");
        let digest = bravebot_config::mcp::Digest::parse(&"d".repeat(64)).expect("a digest");
        bravebot_agent::mcp::Reached::new(
            bravebot_agent::mcp::Connection::Http(server),
            listing,
            digest,
        )
        .with_prompts(prompts)
    }

    /// A session holding `weather` as running, which offers the prompt `summarise_diff`.
    pub(crate) fn holding_a_server_with_prompts() -> Session {
        holding(&["weather"])
    }

    /// A session holding each of `aliases` as running, each offering the prompt `summarise_diff`.
    pub(crate) fn holding_servers(aliases: &[&'static str]) -> Session {
        holding(aliases)
    }

    /// A session holding each of `aliases` as running, with `servers` reading it.
    pub(crate) fn holding(aliases: &[&'static str]) -> Session {
        let mut session = Session::new("none");
        session.servers.session = Some(bravebot_agent::mcp::Session::new(
            aliases.iter().map(|alias| reached(alias)).collect(),
            std::env::temp_dir(),
            None,
            false,
            bravebot_config::Managed::default(),
        ));
        refresh(&mut session.servers);
        session
    }

    /// A session holding `alias` with `policy` against it, on the same stand-in connection `holding`
    /// uses: what is under test is what a screen draws from a session's policies, and a launch that
    /// makes them is checked in `bravebot_agent::servers`.
    pub(crate) fn holding_started_under(
        alias: &'static str,
        policy: servers::EffectivePolicy,
    ) -> Session {
        let mut session = Session::new("none");
        session.servers.session = Some(bravebot_agent::mcp::Session::new(
            vec![reached(alias).confined_by(policy)],
            std::path::PathBuf::from("/work"),
            None,
            false,
            bravebot_config::Managed::default(),
        ));
        refresh(&mut session.servers);
        session
    }

    /// A session holding `weather` with its list put to a person who answered `yes`, so its list
    /// stands as offered, or as declined.
    pub(crate) fn holding_a_list_answered(yes: bool) -> Session {
        use bravebot_agent::confirm::Unattended;
        use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
        use bravebot_core::event::RecordingSink;
        use bravebot_core::policy::{Policy, ReleasePlan, Routing};
        let session = holding(&["weather"]);
        let config = bravebot_config::Config::from_lookup(|name| match name {
            "SERVICES_KEY_AICHAT" => Some("test-key".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
            "BRAVE_AI_CHAT_ENDPOINT" => Some("https://ai-chat.bsg.brave.com".into()),
            _ => None,
        })
        .expect("config");
        let egress = bravebot_net::Egress::new();
        let mut chat = bravebot_agent::processor::Chat::new(
            &config,
            &egress,
            &bravebot_agent::chatgpt::AccountSession::begin(),
        );
        let holding = CapabilitySet::from_iter([Capability::McpCall(ServerAlias::new("weather"))]);
        let mut routing = Routing::new();
        routing.insert_trusted("task", "reach an MCP server");
        let mut sink = RecordingSink::new();
        let mut policy =
            Policy::begin(routing, ReleasePlan::new(), holding.clone(), &mut sink).expect("policy");
        // Bypassing is the mode that answers a list's question yes without a person, and makes no
        // check, so no model is reached. Unattended is the one that answers no.
        let mode = bravebot_agent::PermissionMode::Bypass;
        let mut reporter = bravebot_agent::IgnoreReports;
        let mcp = session.servers.session.as_ref().expect("servers");
        match yes {
            true => mcp.settle(
                &mut policy,
                &mut chat,
                &mut bravebot_agent::Confining::new(&mut Unattended, mode, false),
                &mut reporter,
                mode,
                &holding,
            ),
            false => mcp.settle(
                &mut policy,
                &mut chat,
                &mut Unattended,
                &mut reporter,
                mode,
                &holding,
            ),
        };
        session
    }

    /// A machine whose state directory declares `weather` as a remote server nobody approved.
    fn declaring_weather(name: &str) -> Machine {
        declaring(name, &["weather"])
    }

    /// A machine whose state directory declares each of `aliases` as a remote server nobody approved.
    fn declaring(name: &str, aliases: &[&str]) -> Machine {
        use bravebot_config::mcp::{Declaration, Declarations};
        let home = tempfile::Builder::new()
            .prefix(name)
            .tempdir()
            .expect("directory")
            .keep();
        let mut declarations = Declarations::default();
        for alias in aliases {
            declarations.insert(
                alias,
                &Declaration::http(format!("https://{alias}.example/mcp")).expect("declaration"),
            );
        }
        std::fs::write(
            bravebot_config::mcp::declarations_file(&home),
            declarations.to_text(),
        )
        .expect("mcp.json");
        Machine {
            managed: bravebot_config::Managed::at(&home.join("managed.json")),
            home: servers::Home {
                directory: Some(home),
                writable: false,
            },
        }
    }

    /// A start the person answers No joins nothing, says it was declined and not that it was
    /// reached, and the server is asked about again on the next `reconnect`. Naming the alias also
    /// clears a turn-off, and the refused start does not put it back.
    #[test]
    fn a_start_the_person_refuses_joins_nothing_and_clears_the_turn_off() {
        let (settings, root) = requesting("mcp-refused", &["weather"]);
        let machine = declaring_weather("mcp-refused-home");
        let mut session = Session::new("none");
        let mut asker = Answering {
            answer: servers::Answer::No,
            asked: 0,
        };

        run_on(
            &mut session,
            &settings,
            &root,
            "disable weather",
            Asking::Person,
            &mut asker,
            &machine,
        );
        assert_eq!(asker.asked, 0, "turning a server off put a question");
        run_on(
            &mut session,
            &settings,
            &root,
            "reconnect",
            Asking::Person,
            &mut asker,
            &machine,
        );
        assert_eq!(asker.asked, 0, "a server that is off was started");
        run_on(
            &mut session,
            &settings,
            &root,
            "reconnect weather",
            Asking::Person,
            &mut asker,
            &machine,
        );

        assert_eq!(asker.asked, 1);
        let said = said(&session);
        assert!(
            said.contains(&t!(servers_declined, alias = "weather").to_string()),
            "{said:?}"
        );
        assert!(
            !said.contains(&t!(mcp_command_reconnected, alias = "weather").to_string()),
            "{said:?}"
        );
        assert!(session.servers.session.is_none());
        assert!(session.servers.started.is_empty());
        assert!(session.servers.disabled.is_empty(), "naming it left it off");
    }

    /// What `remove` asked the program to do, in the order, and the directory it asked from, for a
    /// session that holds `weather` and a program that `succeeds` or does not.
    fn removing(
        session: &mut Session,
        argument: &str,
        machine: &Machine,
        succeeds: impl Fn(&[&str]) -> bool,
    ) -> (std::path::PathBuf, Vec<Vec<String>>) {
        removing_requested(session, argument, machine, &["weather"], succeeds)
    }

    /// As `removing`, for a checkout whose settings request `aliases`.
    fn removing_requested(
        session: &mut Session,
        argument: &str,
        machine: &Machine,
        aliases: &[&str],
        succeeds: impl Fn(&[&str]) -> bool,
    ) -> (std::path::PathBuf, Vec<Vec<String>>) {
        let (settings, root) = requesting("mcp-remove", aliases);
        let calls = std::cell::RefCell::new(Vec::new());
        let programs = |directory: &Path, args: &[&str]| {
            assert_eq!(directory, root, "the program ran somewhere else");
            calls
                .borrow_mut()
                .push(args.iter().map(ToString::to_string).collect::<Vec<_>>());
            Ran {
                succeeded: succeeds(args),
                lines: vec![format!("ran {}", args.join(" "))],
            }
        };
        run_with(
            session,
            &settings,
            &root,
            argument,
            Asking::Person,
            &mut Never,
            &Ways {
                machine,
                programs: &programs,
            },
        );
        (root, calls.into_inner())
    }

    /// A machine whose state directory may be written, which is what removing needs.
    fn writable_machine(name: &str) -> Machine {
        let mut machine = declaring_weather(name);
        machine.home.writable = true;
        machine
    }

    /// The servers of the issue: two whose aliases share a prefix and one that does not. The session
    /// holds all three, the settings request one of the pair, and the machine declares the pair
    /// and a third of the prefix that nothing holds or requests.
    const TABS: [&str; 4] = [
        "brave-page-read",
        "brave-tabs-extra",
        "brave-tabs-manage",
        "brave-tabs-read",
    ];

    fn call(words: [&str; 3]) -> Vec<String> {
        words.map(String::from).to_vec()
    }

    /// SERVERS-19. `remove brave-tabs*` removes each server that fits, as `remove <alias>` does for
    /// one: held, requested or only declared, in alphabetical order, each by the program's own two
    /// commands, and leaves the server it does not fit running. Rejects a pattern that matches only
    /// what the session holds, one that reaches `brave-page-read`, and one that passes the `*` to the
    /// program.
    #[test]
    fn removing_by_a_pattern_removes_each_server_it_names_and_no_other() {
        let mut machine = declaring("mcp-remove-pattern-home", &TABS);
        machine.home.writable = true;
        let mut session = holding(&["brave-page-read", "brave-tabs-manage", "brave-tabs-read"]);

        let (_, calls) = removing_requested(
            &mut session,
            "remove brave-tabs*",
            &machine,
            &["brave-tabs-read"],
            |_| true,
        );

        assert_eq!(
            calls,
            [
                call(["mcp", "disable", "brave-tabs-extra"]),
                call(["mcp", "remove", "brave-tabs-extra"]),
                call(["mcp", "disable", "brave-tabs-manage"]),
                call(["mcp", "remove", "brave-tabs-manage"]),
                call(["mcp", "disable", "brave-tabs-read"]),
                call(["mcp", "remove", "brave-tabs-read"]),
            ]
        );
        assert_eq!(session.servers.started, ["brave-page-read"]);
    }

    /// SERVERS-19. A removal that fails for one server says so and leaves it running, and the
    /// servers after it are still removed. Rejects stopping at the first failure, and stopping a
    /// server whose declaration is still there.
    #[test]
    fn a_pattern_removal_that_fails_for_one_server_still_removes_the_others() {
        let mut machine = declaring("mcp-remove-pattern-fails-home", &TABS);
        machine.home.writable = true;
        let mut session = holding(&["brave-tabs-manage", "brave-tabs-read"]);

        let (_, calls) =
            removing_requested(&mut session, "remove brave-tabs-*", &machine, &[], |args| {
                args[2] != "brave-tabs-manage"
            });

        assert!(calls.contains(&call(["mcp", "remove", "brave-tabs-read"])));
        assert!(!calls.contains(&call(["mcp", "remove", "brave-tabs-manage"])));
        assert_eq!(session.servers.started, ["brave-tabs-manage"]);
    }

    /// SERVERS-19. A pattern that names nothing runs no program and says so. A pattern in an
    /// incognito session is refused as a removal is, and nothing is run.
    #[test]
    fn a_pattern_that_names_nothing_or_is_typed_where_nothing_may_be_written_runs_no_program() {
        let mut incognito = declaring("mcp-remove-pattern-incognito-home", &TABS);
        incognito.home.writable = false;
        let mut writable = declaring("mcp-remove-pattern-none-home", &TABS);
        writable.home.writable = true;
        for (argument, machine, reply) in [
            (
                "remove nothing-*",
                writable,
                t!(mcp_command_no_match, pattern = "nothing-*").to_string(),
            ),
            (
                "remove brave-tabs*",
                incognito,
                t!(mcp_not_while_incognito, command = MCP_REMOVE).to_string(),
            ),
        ] {
            let mut session = holding(&["brave-tabs-read"]);

            let (_, calls) = removing_requested(&mut session, argument, &machine, &[], |_| true);

            assert!(calls.is_empty(), "{argument}: {calls:?}");
            assert_eq!(said(&session), [reply], "{argument}");
            assert_eq!(session.servers.started, ["brave-tabs-read"], "{argument}");
        }
    }

    /// SERVERS-19. `disable brave-tabs*` turns off each held or requested server that fits, as
    /// `disable <alias>` does for one, and writes nothing. Rejects touching the server it does not
    /// fit, and a pattern with no match changing anything.
    #[test]
    fn disabling_by_a_pattern_turns_off_each_server_it_names_and_no_other() {
        let (settings, root) = requesting("mcp-disable-pattern", &["brave-tabs-extra"]);
        let mut session = holding(&["brave-page-read", "brave-tabs-manage", "brave-tabs-read"]);

        typed(&mut session, &settings, &root, "disable brave-tabs*");

        assert_eq!(session.servers.started, ["brave-page-read"]);
        assert_eq!(
            session.servers.disabled,
            ["brave-tabs-extra", "brave-tabs-manage", "brave-tabs-read"]
        );
        assert_eq!(
            said(&session),
            [
                t!(mcp_command_disabled, alias = "brave-tabs-extra").to_string(),
                t!(mcp_command_disabled, alias = "brave-tabs-manage").to_string(),
                t!(mcp_command_disabled, alias = "brave-tabs-read").to_string(),
            ]
        );

        let mut untouched = holding(&["brave-page-read"]);
        typed(&mut untouched, &settings, &root, "disable nothing-*");
        assert_eq!(
            said(&untouched),
            [t!(mcp_command_no_match, pattern = "nothing-*").to_string()]
        );
        assert_eq!(untouched.servers.started, ["brave-page-read"]);
        assert!(untouched.servers.disabled.is_empty());
    }

    /// SERVERS-19. `/mcp remove` takes the request out first and then deletes the declaration, both
    /// by the program's own commands, says what each printed, and stops the server in this session.
    /// Rejects deleting the declaration under a request that stays, which is reported as a server
    /// nobody declared at every start, and a server left running after it was removed.
    #[test]
    fn removing_a_server_withdraws_the_request_then_deletes_the_declaration_and_stops_it() {
        let machine = writable_machine("mcp-remove-home");
        let mut session = holding(&["weather"]);
        session.servers.disabled.push("weather".to_string());

        let (_, calls) = removing(&mut session, "remove weather", &machine, |_| true);

        assert_eq!(
            calls,
            [
                ["mcp", "disable", "weather"].map(String::from).to_vec(),
                ["mcp", "remove", "weather"].map(String::from).to_vec(),
            ]
        );
        assert_eq!(
            said(&session),
            ["ran mcp disable weather", "ran mcp remove weather"]
        );
        assert!(session.servers.session.is_none(), "the server still runs");
        assert!(session.servers.started.is_empty());
        assert!(session.servers.disabled.is_empty());
    }

    /// SERVERS-19. A removal the program reports failing leaves the server running and says why.
    /// Rejects stopping a server whose declaration is still there.
    #[test]
    fn a_removal_that_fails_leaves_the_server_running_and_says_why() {
        let machine = writable_machine("mcp-remove-fails-home");
        let mut session = holding(&["weather"]);

        removing(&mut session, "remove weather", &machine, |_| false);

        assert_eq!(session.servers.started, ["weather"]);
        assert!(session.servers.session.is_some());
        assert_eq!(said(&session), ["ran mcp disable weather"]);
    }

    /// SERVERS-19. A request the program fails to withdraw leaves the declaration alone, so no
    /// later session reports a server nobody declared. Rejects deleting it under a request that stays.
    #[test]
    fn a_request_that_cannot_be_withdrawn_keeps_the_declaration() {
        let machine = writable_machine("mcp-remove-disable-fails-home");
        let mut session = holding(&["weather"]);

        let (_, calls) = removing(&mut session, "remove weather", &machine, |args| {
            args[1] != "disable"
        });

        assert_eq!(
            calls,
            [["mcp", "disable", "weather"].map(String::from).to_vec()],
            "remove was called after disable failed"
        );
        assert_eq!(session.servers.started, ["weather"]);
        assert!(session.servers.session.is_some());
    }

    /// SERVERS-19. A word that is not an alias, a session that writes nothing and a machine with no
    /// state directory run no program. Rejects a word like `--help` or `../x` reaching the program as
    /// an argument, and a removal in an incognito session, whose program cannot know it is one.
    #[test]
    fn a_removal_runs_no_program_for_a_word_that_is_no_alias_or_where_nothing_may_be_written() {
        let mut incognito = writable_machine("mcp-remove-incognito-home");
        incognito.home.writable = false;
        let mut stateless = writable_machine("mcp-remove-stateless-home");
        stateless.home.directory = None;
        for (argument, machine, reply) in [
            (
                "remove --help",
                writable_machine("mcp-remove-flag-home"),
                t!(mcp_command_not_an_alias, alias = "--help").to_string(),
            ),
            (
                "remove ../weather",
                writable_machine("mcp-remove-path-home"),
                t!(mcp_command_not_an_alias, alias = "../weather").to_string(),
            ),
            (
                "remove weather",
                incognito,
                t!(mcp_not_while_incognito, command = MCP_REMOVE).to_string(),
            ),
            (
                "remove weather",
                stateless,
                t!(mcp_command_no_state_directory).to_string(),
            ),
        ] {
            let mut session = holding(&["weather"]);

            let (_, calls) = removing(&mut session, argument, &machine, |_| true);

            assert!(calls.is_empty(), "{argument}: {calls:?}");
            assert_eq!(said(&session), [reply], "{argument}");
            assert_eq!(session.servers.started, ["weather"], "{argument}");
        }
    }

    /// Naming a server that is running starts nothing and asks nothing.
    #[test]
    fn reconnecting_a_running_server_says_so_and_changes_nothing() {
        let (settings, root) = requesting("mcp-running", &["weather"]);
        let mut session = holding(&["weather"]);

        typed(&mut session, &settings, &root, "reconnect weather");

        assert_eq!(
            said(&session),
            vec![t!(mcp_command_already_running, alias = "weather").to_string()]
        );
        assert_eq!(session.servers.started, vec!["weather".to_string()]);
    }

    /// Turning off a running server stops it and says so, and the last one leaves the session with
    /// none rather than an empty one. Another that is running stays.
    #[test]
    fn turning_off_a_running_server_removes_it_and_the_last_leaves_none() {
        let (settings, root) = requesting("mcp-disable", &["weather", "docs"]);
        let mut session = holding(&["weather", "docs"]);

        typed(&mut session, &settings, &root, "disable weather");

        assert_eq!(session.servers.started, vec!["docs".to_string()]);
        assert_eq!(held(&session), vec!["docs".to_string()]);
        assert_eq!(session.servers.disabled, vec!["weather".to_string()]);

        typed(&mut session, &settings, &root, "disable docs");

        assert!(session.servers.session.is_none());
        assert!(session.servers.started.is_empty());
        assert!(!session.servers.confined);
        assert_eq!(
            said(&session),
            vec![
                t!(mcp_command_disabled, alias = "weather").to_string(),
                t!(mcp_command_disabled, alias = "docs").to_string(),
            ]
        );
    }
}
