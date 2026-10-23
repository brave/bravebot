//! Consulting a bot from the window with `@name` ([CONSULT-2](../../../docs/specs/consulting-a-bot.md#CONSULT-2)).
//!
//! The rules are the terminal's: a line a person sent naming a bot in a session whose turns
//! address no definition goes to that bot, as a turn addressed to it on a conversation of its own,
//! and its reply is handed to the host by `bravebot_agent::consult::record`. A line naming several
//! consults them one after another (CONSULT-9).

use crate::protocol::Failure;
use bravebot_agent::Workspace;
use bravebot_config::Config;
use bravebot_core::delegate::{Definition, Definitions};
use bravebot_i18n::t;

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

/// Every bot `prompt` names among `definitions`, in the order it names them, each with the consult
/// it starts or why it cannot be consulted.
///
/// Empty for a line that names no bot. A bot whose model this machine does not request, or needs a
/// sign-in for, is refused before anything runs, and the session's model is not put in its place
/// (CONSULT-10).
pub fn consulted(
    config: &Config,
    definitions: &Definitions,
    prompt: &str,
) -> Vec<(String, Result<Consulting, String>)> {
    let bots = bots(definitions);
    let names: Vec<&str> = bots.iter().map(String::as_str).collect();
    bravebot_mentions::consulted(prompt, &names)
        .into_iter()
        .filter_map(|named| {
            let found = definitions.get(&named)?;
            let consulting = consulting(config, found, &named);
            Some((named, consulting))
        })
        .collect()
}

fn consulting(config: &Config, found: &Definition, named: &str) -> Result<Consulting, String> {
    if let Some(written) = found.model()
        && let Some((file, why)) = config.model_refused(written)
    {
        return Err(t!(
            delegate_model_refused,
            definition = named,
            model = written,
            reason = bravebot_agent::backend::refusal_reason(file, why)
        )
        .to_string());
    }
    if let Some(written) = found.model()
        && bravebot_agent::backend::Backend::needs_sign_in(config, &config.model_named(written))
    {
        return Err(t!(
            delegate_model_needs_sign_in,
            definition = named,
            model = written
        )
        .to_string());
    }
    Ok(Consulting {
        definition: found.origin().to_string(),
        model: found.model().map(|written| config.model_named(written)),
        bot: named.to_string(),
    })
}

/// The consult to start now out of the bots a line named, in its order.
#[derive(Debug)]
pub struct Picked {
    /// The first bot that may be consulted, or where none may, why the last one cannot be.
    pub now: Result<Consulting, Failure>,
    /// Why each bot before it cannot be consulted. They are noted and skipped (CONSULT-10).
    pub skipped: Vec<String>,
    /// The bots named after it, to be consulted once it replies (CONSULT-9).
    pub left: Vec<String>,
}

/// Pick the consult to start now out of `named`, keeping only the bots in `only` where it is given.
///
/// `None` where no bot is left to consult.
pub fn pick(
    named: Vec<(String, Result<Consulting, String>)>,
    only: Option<&[String]>,
) -> Option<Picked> {
    let mut named = named
        .into_iter()
        .filter(|(bot, _)| only.is_none_or(|only| only.contains(bot)));
    let mut skipped = Vec::new();
    let mut last = None;
    for (_, consulting) in named.by_ref() {
        match consulting {
            Ok(consulting) => {
                skipped.extend(last.take());
                return Some(Picked {
                    now: Ok(consulting),
                    skipped,
                    left: named.map(|(bot, _)| bot).collect(),
                });
            }
            Err(refused) => skipped.extend(last.replace(refused)),
        }
    }
    last.map(|refused| Picked {
        now: Err(Failure::bad_request(refused)),
        skipped,
        left: Vec::new(),
    })
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
        pick(consulted(&config(), &definitions(), prompt), None)
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

    fn several() -> Vec<(String, Result<Consulting, String>)> {
        let bot = |name: &str| {
            Ok(Consulting {
                bot: name.into(),
                definition: "~/.bravebot/agents".into(),
                model: None,
            })
        };
        vec![
            ("first".into(), Err("first needs a sign-in".into())),
            ("second".into(), bot("second")),
            ("third".into(), Err("third needs a sign-in".into())),
            ("fourth".into(), bot("fourth")),
        ]
    }

    /// CONSULT-9: bots named together are consulted in the line's order, and the ones after the
    /// first are left for once it replies.
    #[test]
    fn bots_named_together_are_consulted_in_the_order_the_line_names_them() {
        let picked = pick(several(), None).expect("named");
        assert_eq!(picked.now.expect("consulted").bot, "second");
        assert_eq!(picked.left, vec!["third".to_string(), "fourth".to_string()]);

        let next = pick(several(), Some(&picked.left)).expect("named");
        assert_eq!(next.now.expect("consulted").bot, "fourth");
        assert!(next.left.is_empty());
    }

    /// CONSULT-9 and CONSULT-10: a bot that may not be consulted is noted and skipped, and the
    /// next one is consulted. Where none may, the last refusal is the turn's.
    #[test]
    fn a_refused_bot_is_skipped_and_the_next_is_consulted() {
        let picked = pick(several(), None).expect("named");
        assert_eq!(picked.skipped, vec!["first needs a sign-in".to_string()]);

        let refused = pick(several(), Some(&["third".to_string()])).expect("named");
        assert!(refused.now.is_err());
        assert!(refused.skipped.is_empty());

        let only = ["first".to_string(), "third".to_string()];
        let refused = pick(several(), Some(&only)).expect("named");
        assert_eq!(refused.skipped, vec!["first needs a sign-in".to_string()]);
        assert!(pick(several(), Some(&[])).is_none());
    }
}
