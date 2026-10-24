//! The sentences about the level a ChatGPT session keeps, in a French catalog, in a process of
//! their own because the locale is chosen once per process (LOCALE-7).

use bravebot_i18n::t;

/// The terminal says it when a level is chosen on a model of the account, the agent says it when a
/// skill asks for one there, and `/status` says it beside the level it names. Each is a sentence a
/// person reads, so each has a French one that is not the English one, and each keeps the commands
/// the person types and the words it was given. The regressions it rejects are a sentence missing
/// from the French catalog, which would be shown in English, and a translation that changes a
/// command or drops an argument.
#[test]
fn the_sentences_about_a_level_a_chatgpt_session_keeps_are_said_in_the_chosen_catalog() {
    bravebot_i18n::init(bravebot_i18n::resolve("fr"));

    let chosen = t!(
        session_effort_kept_for_account,
        began = "low",
        chosen = "high"
    )
    .to_string();
    assert_eq!(
        chosen,
        "une session ChatGPT garde le niveau d'effort avec lequel elle a commencé, low, donc \
         high s'applique à partir de la session suivante, que /clear commence"
    );

    let asked = t!(
        skill_effort_kept_for_account,
        skill = "tidy",
        effort = "high"
    )
    .to_string();
    assert_eq!(
        asked,
        "tidy demande l'effort high pour le reste de ce tour, mais une session ChatGPT garde le \
         niveau avec lequel elle a commencé, donc ce tour ne change pas"
    );

    let status = t!(status_effort_kept_for_account, chosen = "high").to_string();
    assert_eq!(
        status,
        "le niveau avec lequel cette session ChatGPT a commencé ; high, choisi avec /effort, \
         s'applique à partir de la session suivante"
    );
}
