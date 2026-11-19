//! TRACE-12 through the terminal front end. Each test sets the mode the way the terminal does, runs
//! a turn on the session's own handle, and reads the entries the trail holds, so the cause each
//! caller passes is read as the words a person reading the trail sees.
use super::*;

use super::undo_tests::endpoint;

/// The entries under the three mode gates that one turn on `session` writes, in order.
fn the_mode_entries_of_a_turn(name: &str, session: &Session) -> Vec<String> {
    let root = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch");
    let workspace = Workspace::new(&root).expect("workspace");
    let (config, _requests, server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let task = Task::new("say hello").with_permission_mode(session.live_permission_mode());
    let mut trail = Trail::new();
    turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &task,
        &mut Conversation::new(),
        &mut bravebot_agent::confirm::Unattended,
        &mut bravebot_agent::IgnoreReports,
        &mut trail,
        bravebot_agent::workspace::trust_store(workspace.root()),
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome
    .expect("the turn runs");
    server.join().expect("the model finished");
    std::fs::remove_dir_all(&root).ok();
    trail
        .events()
        .iter()
        .filter_map(|stamped| match &stamped.event {
            bravebot_core::event::Event::GatePassed {
                gate: "permission_mode" | "permission_mode_opened" | "permission_mode_changed",
                detail,
            } => Some(detail.clone()),
            _ => None,
        })
        .collect()
}

/// TRACE-12: the key, a click on the mode and `/plan` each name themselves as the cause of the
/// change they make. The faults it rejects are a driver passing another driver's cause (the key
/// labelled as a click, or the command as the key), and a driver passing no cause, which the trail
/// words as `without a cause being given`. The key and the click land on the same mode from asking,
/// so only the cause tells them apart.
#[test]
fn the_key_a_click_and_the_plan_command_each_name_themselves_in_the_trail() {
    type Drive = fn(&mut Session);
    let rows: [(&str, Drive, &str); 3] = [
        (
            "bravebot-mode-trail-key",
            |session| session.cycle_permission_mode(),
            "the permission mode changed from ask to accept-edits by the mode key",
        ),
        (
            "bravebot-mode-trail-click",
            |session| session.cycle_permission_mode_by_click(),
            "the permission mode changed from ask to accept-edits by a click on the mode",
        ),
        (
            "bravebot-mode-trail-plan",
            |session| session.enter_plan_mode(),
            "the permission mode changed from ask to plan by the /plan command",
        ),
    ];
    for (name, drive, expected) in rows {
        let mut session = Session::new("none");
        drive(&mut session);
        let entries = the_mode_entries_of_a_turn(name, &session);
        let began = format!(
            "the turn began in {} mode",
            session.permission_mode().name()
        );
        assert_eq!(
            entries,
            [
                "the session opened in ask mode by default",
                expected,
                began.as_str()
            ],
            "{name}"
        );
    }
}

/// TRACE-12, MODE-1, AUTO-1: a session opened by the terminal names what chose the mode it opened
/// in: the bypass flag, `--auto`, the person's `permissions.startInAuto`, or nothing. The flag
/// outranks the setting, and a session a link started ignores the setting and opens asking by
/// default. The faults it rejects are the two arms of the choice between flag and setting swapped,
/// a setting labelled as a flag, and a link's session labelled as opened by the setting it did not
/// read.
#[test]
fn a_session_the_terminal_opens_names_what_chose_its_mode_in_the_trail() {
    let on = r#"{"permissions": {"startInAuto": true}}"#;
    // (bypass flag, --auto, settings, started by a link, expected entries)
    let rows: [(bool, bool, &str, bool, [&str; 2]); 6] = [
        (
            false,
            false,
            "{}",
            false,
            [
                "the session opened in ask mode by default",
                "the turn began in ask mode",
            ],
        ),
        (
            false,
            true,
            "{}",
            false,
            [
                "the session opened in auto mode by a command-line flag",
                "the turn began in auto mode",
            ],
        ),
        (
            false,
            false,
            on,
            false,
            [
                "the session opened in auto mode by the startInAuto setting",
                "the turn began in auto mode",
            ],
        ),
        (
            false,
            true,
            on,
            false,
            [
                "the session opened in auto mode by a command-line flag",
                "the turn began in auto mode",
            ],
        ),
        (
            true,
            false,
            "{}",
            false,
            [
                "the session opened in bypass mode by a command-line flag",
                "the turn began in bypass mode",
            ],
        ),
        (
            false,
            false,
            on,
            true,
            [
                "the session opened in ask mode by default",
                "the turn began in ask mode",
            ],
        ),
    ];
    for (at, (skip_permissions, asked_for_auto, text, from_link, expected)) in
        rows.into_iter().enumerate()
    {
        let settings = bravebot_config::Settings::parse(text);
        let session = open_in_the_mode_asked_for(
            Session::new("none"),
            skip_permissions,
            asked_for_auto,
            from_link,
            &settings,
        );
        let entries =
            the_mode_entries_of_a_turn(&format!("bravebot-mode-trail-open-{at}"), &session);
        assert_eq!(
            entries, expected,
            "bypass flag {skip_permissions}, --auto {asked_for_auto}, settings {text}, link {from_link}"
        );
    }
}

/// TRACE-12, PERM-17: a session the flag opened in bypass and a settings layer then took bypass away
/// from writes the opening in bypass by the flag, then a change to asking that names the settings.
/// The faults it rejects are the opening reported as asking (the change written as the opening),
/// the change reported with the flag as its cause, and no change written at all.
#[test]
fn bypass_made_unreachable_after_the_flag_is_an_opening_and_then_a_change() {
    let settings =
        bravebot_config::Settings::parse(r#"{"permissions": {"bypassUnreachable": true}}"#);
    let session = open_in_the_mode_asked_for(Session::new("none"), true, false, false, &settings);
    assert_eq!(
        the_mode_entries_of_a_turn("bravebot-mode-trail-bypass-unreachable", &session),
        [
            "the session opened in bypass mode by a command-line flag",
            "the permission mode changed from bypass to ask because settings made bypass unreachable",
            "the turn began in ask mode",
        ]
    );
}
