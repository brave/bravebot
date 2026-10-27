//! Consulting a bot from the window with `@name` ([CONSULT-2](../../../docs/specs/consulting-a-bot.md#CONSULT-2)).
//!
//! The rules are the terminal's: a line a person sent naming a bot in a session whose turns
//! address no definition goes to that bot, as a turn addressed to it on a conversation of its own,
//! and its reply is handed to the host by `bravebot_agent::consult::record`. A line naming several
//! consults them one after another (CONSULT-9).

use bravebot_agent::Workspace;
use bravebot_config::Config;
use bravebot_core::delegate::{Definition, Definitions};

/// The bot a line consults, as the turn addressed to it needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consulting {
    /// The bot's name, from the set this session resolved.
    pub bot: String,
    /// Where its definition was read from.
    pub definition: String,
    /// The model its definition names, as this machine requests it, where it names one.
    pub model: Option<String>,
}

/// The definitions the session's turns may address, read as a turn reads them.
pub fn resolved(
    workspace: &Workspace,
    trust: bravebot_core::trust::TrustStore,
    permissions: bravebot_core::permissions::Permissions,
) -> Definitions {
    bravebot_agent::agents::resolved(
        workspace,
        bravebot_agent::home::directory().as_deref(),
        trust,
        permissions,
        &mut bravebot_core::event::NullSink,
    )
}

/// The names of the bots in `definitions` (CONSULT-14).
pub fn bots(definitions: &Definitions) -> Vec<String> {
    bravebot_agent::agents::bots(definitions)
        .map(|definition| definition.name().to_string())
        .collect()
}

/// The consult to start now out of the bots a line named, in its order.
pub type Picked = bravebot_agent::consult::Picked<Consulting>;

/// The consult to start now out of the bots `line` names among `definitions`, keeping only the
/// bots in `only` where it is given (CONSULT-9, CONSULT-10).
///
/// `None` where no bot is left to consult.
pub fn pick(
    config: &Config,
    definitions: &Definitions,
    line: &str,
    only: Option<&[String]>,
) -> Option<Picked> {
    let named = bravebot_agent::consult::consulted(config, definitions, line, |found| {
        consulting(config, found)
    });
    bravebot_agent::consult::pick(named, only)
}

fn consulting(config: &Config, found: &Definition) -> Consulting {
    Consulting {
        definition: found.origin().to_string(),
        model: found.model().map(|written| config.model_named(written)),
        bot: found.name().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::delegate::Kind;

    fn definitions() -> Definitions {
        let read = |name: &str| {
            Definition::from_file(
                name,
                "Reviews.",
                Kind::Reader,
                None,
                "R",
                "~/.bravebot/agents",
            )
        };
        let mut definitions = Definitions::default();
        definitions.insert(read("reviewer").with_model("haiku"));
        definitions.insert(read("quiet").without_consults());
        definitions
    }

    fn config() -> Config {
        Config::from_lookup(|key| match key {
            "SERVICES_KEY_AICHAT" => Some("test-key".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
            "BRAVE_AI_CHAT_ENDPOINT" => Some("http://unused.invalid".into()),
            _ => None,
        })
        .expect("config")
    }

    fn picked(prompt: &str) -> Option<Picked> {
        pick(&config(), &definitions(), prompt, None)
    }

    /// CONSULT-2: a line naming a bot consults it, with the definition it was read from and its
    /// model, and a line naming none consults nothing.
    #[test]
    fn a_line_naming_a_bot_consults_it() {
        let config = config();
        let consulting = picked("ask @reviewer about it")
            .expect("a bot is named")
            .now
            .expect("and may be consulted");
        assert_eq!(consulting.bot, "reviewer");
        assert_eq!(consulting.definition, "~/.bravebot/agents");
        assert_eq!(consulting.model, Some(config.model_named("haiku")));
        assert!(picked("read @notes.md").is_none());
    }

    /// CONSULT-14: a bot that opted out is no bot, so naming it consults nothing.
    #[test]
    fn a_bot_that_opted_out_is_not_consulted() {
        assert!(picked("ask @quiet").is_none());
        assert_eq!(bots(&definitions()), vec!["reviewer".to_string()]);
    }

    /// CONSULT-9: the bots an earlier consult of the line left are the only ones picked from.
    #[test]
    fn only_the_bots_left_are_picked_from() {
        let mut definitions = definitions();
        definitions.insert(
            Definition::from_file(
                "writer",
                "Writes.",
                Kind::Reader,
                None,
                "W",
                "~/.bravebot/agents",
            )
            .with_model("haiku"),
        );
        let line = "@reviewer and @writer, look";
        let picked = pick(&config(), &definitions, line, None).expect("named");
        assert_eq!(picked.now.expect("consulted").bot, "reviewer");
        assert_eq!(picked.left, vec!["writer".to_string()]);

        let next = pick(&config(), &definitions, line, Some(&picked.left)).expect("named");
        assert_eq!(next.now.expect("consulted").bot, "writer");
    }
}
