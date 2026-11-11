//! The position of a question in a French catalog, in a process of their own because the locale is
//! chosen once per process (LOCALE-7).

use bravebot_i18n::ask::place;

/// The terminal, the line-by-line interface and the desktop all ask for this sentence, so a
/// catalog that words it differently has to reach all three and none of them can write its own.
#[test]
fn the_position_of_a_question_is_said_in_the_chosen_catalog() {
    bravebot_i18n::init(bravebot_i18n::resolve("fr"));
    assert_eq!(
        place(1, 3).as_deref(),
        Some("l'agent pose une question (2 sur 3)")
    );
    assert_eq!(place(0, 1), None);
}

/// The rows under a settled `ask_user` call are worded by the catalog and never by the front end,
/// so a catalog that words an outcome differently reaches all three front ends. The regression it
/// rejects: an outcome worded with a string literal in a front end, which stays English here.
#[test]
fn the_ways_a_question_ends_are_said_in_the_chosen_catalog() {
    use bravebot_i18n::ask::{
        settled_chose, settled_closed, settled_heading, settled_skipped, settled_typed,
    };
    bravebot_i18n::init(bravebot_i18n::resolve("fr"));
    assert_eq!(settled_heading("Nom", "Lequel ?"), "Nom : Lequel ?");
    assert_eq!(
        settled_chose(&["a".to_string(), "b".to_string()]),
        "choisi : a, b"
    );
    assert_eq!(settled_typed("autre"), "répondu : autre");
    assert_eq!(settled_skipped(), "passée");
    assert_eq!(
        settled_closed(),
        "fermée faute de réponse dans le temps imparti"
    );
}
