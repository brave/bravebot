//! `propose_consult`: the planner proposes asking a bot by name, and a person sends or declines the
//! brief it wrote (CONSULT-12).
//!
//! The bot's name is routing, read as the planner's own words from a trusted context and matched
//! against the bots this turn resolved. The brief is the planner's words at the label of the
//! context it wrote them in, so the card shows it as it is and the consult is held to its label.
//! Nothing is sent without the person's yes, and no mode answers for them.

use super::{Produced, Tools, named_argument};
use crate::confirm::{Confirmer, ConsultRequest, Decision};
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_i18n::t;
use serde_json::Value;

/// A consult a person sent from the card, for the turn to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposed {
    /// The bot's name, as the definitions spell it.
    pub bot: String,
    /// The brief the person was shown and sent.
    pub brief: String,
    /// The label the brief carries, which the consult's context starts at.
    pub label: Label,
}

/// Put the planner's brief for the bot it named to the person, and hand back what they chose.
///
/// Every refusal before the card is drawn is the driver's own words. A name that is no bot, a bot
/// that opted out among them, is answered with the names that are, so a near miss is corrected
/// rather than guessed at. A bot whose model this machine cannot use is put on the card as refused,
/// and nothing is sent whatever the answer (CONSULT-10).
pub(super) fn propose_consult<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    let (Some(bot), Some(brief)) = (
        named_argument(arguments, "bot"),
        named_argument(arguments, "brief"),
    ) else {
        return Produced::problem(
            "error: 'bot' and 'brief' are required and must be strings: the name of the bot, and \
             what to ask it",
        );
    };
    let asked_for = match policy.read_planner_argument("propose_consult", "bot", &bot) {
        Ok(name) => name,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let brief = match policy.adopt_model_output("propose_consult", brief) {
        Ok(brief) => brief,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let label = brief.label();
    let text = match policy.read_trusted_content("propose_consult", &brief) {
        Ok(text) => text,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    let bots: Vec<_> = crate::agents::bots(policy.delegates()).collect();
    let Some(definition) = bots
        .iter()
        .find(|definition| definition.name() == asked_for)
    else {
        let names: Vec<&str> = bots.iter().map(|definition| definition.name()).collect();
        return Produced::problem(format!(
            "refused: no bot that may be consulted is named '{asked_for}'. Nothing was sent and \
             nobody was asked. The names that match a bot: {}.",
            names.join(", ")
        ));
    };

    let config = tools.chat.config;
    let model = definition
        .model()
        .map(str::to_string)
        .or_else(|| tools.chat.model.map(str::to_string))
        .unwrap_or_else(|| config.default_model.clone());
    // Only a model the definition wrote can be refused here: the host is already running on its
    // own, and the host's is never put in place of the bot's (CONSULT-10). The machine-level layer
    // first, as a typed tag is refused.
    let refused = definition.model().and_then(|written| {
        if let Some((file, why)) = config.model_refused(written) {
            return Some(
                t!(
                    delegate_model_refused,
                    definition = definition.name(),
                    model = written,
                    reason = crate::backend::refusal_reason(file, why)
                )
                .to_string(),
            );
        }
        crate::backend::Backend::needs_sign_in(config, &config.model_named(written)).then(|| {
            t!(
                delegate_model_needs_sign_in,
                definition = definition.name(),
                model = written
            )
            .to_string()
        })
    });
    let request = ConsultRequest {
        bot: definition.name().to_string(),
        definition: definition.origin().to_string(),
        model,
        rounds: definition.rounds(),
        budget: definition.budget(),
        brief: text.clone(),
        label,
        refused,
    };
    let decision = confirmer.confirm_consult(&request);
    if let Some(refused) = &request.refused {
        return Produced::problem(format!(
            "refused: {refused}. Nothing was sent to {}, and no other model was asked in its place.",
            request.bot
        ));
    }
    if decision == Decision::Reject {
        // The planner is not told why, for the reason a declined message says nothing more: a
        // refusal it could read a reason into is one it could argue with.
        return Produced::problem(format!(
            "the person declined the consult, so nothing was sent to {}",
            request.bot
        ));
    }

    let mut produced = Produced::new(
        Labelled::trusted(String::new()),
        String::new(),
        format!("consult sent to {}", request.bot),
    );
    produced.consult = Some(Proposed {
        bot: request.bot,
        brief: text,
        label,
    });
    produced
}
