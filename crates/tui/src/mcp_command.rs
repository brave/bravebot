//! `/mcp`: the MCP servers a session holds, listed, started again, or turned off (SERVERS-19).
//!
//! A person's word and nobody else's: the line is dispatched from the input box like every other
//! command (CMD-1), and what a server said is never read to decide anything here. The person names
//! the alias, and what is started is what the person's own declarations say it is, through the road
//! a session takes as it opens, so a question that road asks is asked here and by the same asker.

use bravebot_agent::mcp::Offering;
use bravebot_agent::servers::{self, Asker, Asking};
use bravebot_config::Settings;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use std::path::Path;

use crate::command_table::{MCP_DISABLE, MCP_LIST, MCP_RECONNECT};
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
    let words: Vec<&str> = argument.split_whitespace().collect();
    match words.as_slice() {
        [] => list(session, settings),
        [word] if *word == MCP_LIST => list(session, settings),
        [word] if *word == MCP_RECONNECT => reconnect(session, settings, root, None, asking, asker),
        [word, alias] if *word == MCP_RECONNECT => {
            reconnect(session, settings, root, Some(alias), asking, asker)
        }
        [word] if *word == MCP_DISABLE => {
            session.note(t!(mcp_command_needs_an_alias, command = MCP_DISABLE))
        }
        [word, alias] if *word == MCP_DISABLE => disable(session, settings, alias),
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
        session.note(t!(mcp_command_running));
        for server in running {
            session.note(format!(
                "  {}",
                crate::status::offering(std::slice::from_ref(&server))
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
}

fn reconnect<A: Asker + ?Sized>(
    session: &mut Session,
    settings: &Settings,
    root: &Path,
    named: Option<&str>,
    asking: Asking,
    asker: &mut A,
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

    let reached = servers::reconnect(
        settings,
        root,
        &targets,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Speaker;

    /// Nobody to ask: a command that puts a question here is a command that reached the road a start
    /// takes, which none of these may.
    struct Never;

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

    /// Turning off a requested server that is not running keeps `reconnect` from starting it, says
    /// nothing was written, and `reconnect` naming it clears that. Turning it off twice says it is
    /// already off.
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
}
