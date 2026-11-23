//! The sentences about the level a ChatGPT session keeps, in a French catalog, in a process of
//! their own because the locale is chosen once per process (LOCALE-7).

use bravebot_i18n::t;

/// The terminal says it when a level is chosen on a model of the account, in one sentence for a
/// recorded level and another for a level chosen for this session only. The agent says it when a
/// skill asks for one there, and `/status` says it beside the level it names, in the same two
/// forms, and says beside a level chosen for this session only that it is. Each is a sentence a
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

    let session_only = t!(
        session_effort_kept_for_account_this_session,
        began = "low",
        chosen = "high"
    )
    .to_string();
    assert_eq!(
        session_only,
        "une session ChatGPT garde le niveau d'effort avec lequel elle a commencé, low, donc \
         high s'applique à partir de /clear, qui commence la session ChatGPT suivante, et n'est \
         pas enregistré"
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

    let status_session_only =
        t!(status_effort_kept_for_account_this_session, chosen = "high").to_string();
    assert_eq!(
        status_session_only,
        "le niveau avec lequel cette session ChatGPT a commencé ; high, choisi avec /effort \
         session, n'est pas enregistré et s'applique à partir de /clear"
    );

    let status_scope = t!(status_effort_chosen_this_session).to_string();
    assert_eq!(
        status_scope,
        "choisi avec /effort session, pour cette session seulement"
    );
}
