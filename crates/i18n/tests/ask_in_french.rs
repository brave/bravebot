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
