use super::*;

/// A planner that asks the user for a path, a filename, or whether something is installed is
/// asking a person to do a lookup, and they answer less precisely than the filesystem does.
/// One session opened by asking where Brave was installed rather than looking.
#[test]
fn asking_is_described_as_a_last_resort_after_looking() {
    let tool = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "ask_user")
    .expect("ask_user is offered");
    let described = tool.function.description.to_lowercase();
    assert!(
        described.contains("cannot find out yourself"),
        "ask_user does not say to look first: {described}"
    );
    assert!(
        described.contains("never for a fact about this machine"),
        "ask_user does not rule out asking for discoverable facts: {described}"
    );
}

/// The reason looking first is safe, which the planner has no way to know otherwise. It used
/// to be told the opposite, that a question was refused once anything had been read, which is
/// what made front-loading questions look obligatory.
#[test]
fn ask_user_says_that_looking_first_does_not_forfeit_the_question() {
    let tool = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "ask_user")
    .expect("ask_user is offered");
    assert!(
        tool.function
            .description
            .contains("does not stop you asking afterwards"),
        "ask_user still implies reading forfeits the question: {}",
        tool.function.description
    );
}

/// The `ask_user` description as the turn it is read in is offered it.
fn ask_user_description(running: Running) -> String {
    available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        running,
    )
    .into_iter()
    .find(|t| t.function.name == "ask_user")
    .expect("ask_user is offered")
    .function
    .description
}

/// TOOL-6. The advice to look before asking names `run` only on a list that holds one. A turn
/// addressed to a definition that keeps `ask_user` and drops `run` reads this description, and
/// would otherwise be sent to a name it is refused.
#[test]
fn ask_user_names_run_only_where_the_turn_holds_one() {
    let offered = ask_user_description(Running::Offered);
    assert!(
        offered.contains("look with list_files, search, read_file or run instead"),
        "a turn with run is not sent to it: {offered}"
    );
    let withheld = ask_user_description(Running::Withheld);
    assert!(
        withheld.contains("look with list_files, search or read_file instead")
            && !withheld.contains("run"),
        "a turn without run is sent to it: {withheld}"
    );
}

/// ASK-12. Asked to send an email from a web mail account, a planner put two questions to the
/// person, and one of them was how the email should be sent, with "send it automatically" among
/// the options. The request was to send it, so the question had no open choice in it and the
/// person answered what they had already said. The description names the three questions that
/// are not asked: one the request answers, whether to do what was asked, and whether to do it
/// automatically. Each is a phrase of its own so that removing any one fails the test, and
/// each is checked on the description of a turn without `run`, which reads the same text.
#[test]
fn ask_user_says_not_to_ask_what_the_request_states_or_whether_to_do_what_was_asked() {
    for running in [Running::Offered, Running::Withheld] {
        let described = ask_user_description(running);
        for stated in [
            "Do not ask a question whose answer the request states.",
            "Do not ask whether to do what the user asked,",
            "or whether to do it automatically, because they asked for it.",
        ] {
            assert!(
                described.contains(stated),
                "ask_user does not say '{stated}' ({running:?}): {described}"
            );
        }
    }
}

/// ASK-12. What is left to ask is a choice the request leaves open and that differs in
/// consequence, with the example of a person who holds two accounts, which the planner cannot
/// settle from the request and the person can settle in a word. A description that only
/// forbade questions would leave a planner unsure whether any question was left to ask.
#[test]
fn ask_user_leaves_a_question_for_a_choice_the_request_leaves_open() {
    for running in [Running::Offered, Running::Withheld] {
        let described = ask_user_description(running);
        for stated in [
            "What is left to ask is a choice that the request leaves open and that differs \
             in consequence",
            "such as which of two accounts to send from.",
        ] {
            assert!(
                described.contains(stated),
                "ask_user does not say '{stated}' ({running:?}): {described}"
            );
        }
    }
}

/// ASK-12. Each question the planner asks costs the user's attention, and the four sentences
/// this change adds say that and nothing about where the question goes or what becomes of it.
/// A one-shot run declines every question (ASK-7) and a question closes with no answer where
/// `askIdleSeconds` is set (ASK-9), so an added sentence saying that a question is put to the
/// user and waits for an answer would be false on those turns, and would say whether a person
/// is shown a question, which the planner is told nothing about (AUTO-7). The older clause that
/// the questions are put to the user one at a time is unchanged and is not checked here. The
/// description names no mode, classifier or other thing that answers a prompt: no mode but
/// plan mode is named to the planner (MODE-3). A sentence written as "in auto mode a question
/// still goes to the person" tells a planner that a mode may answer for the person, and one
/// written as "whatever mode the session is in" tells it that the session has one.
#[test]
fn ask_user_says_every_question_costs_the_users_attention_and_names_no_mode() {
    for running in [Running::Offered, Running::Withheld] {
        let described = ask_user_description(running);
        assert!(
            described.contains("Each question you ask costs the user's attention."),
            "ask_user does not say that a question costs the user's attention ({running:?}): \
             {described}"
        );
        let lowered = described.to_lowercase();
        for unsaid in [
            "mode",
            "classifier",
            "permission",
            "approve",
            "answers prompts",
            "on your behalf",
            "waits for their answer",
            "every question you ask",
        ] {
            assert!(
                !lowered.contains(unsaid),
                "ask_user tells the planner about '{unsaid}' ({running:?}): {described}"
            );
        }
    }
}
