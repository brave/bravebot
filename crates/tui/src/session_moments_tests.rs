//! HOOK-10 through the full-screen interface's start. The loop needs a real terminal, so these drive
//! the step it takes between the session beginning and the first turn being sent, with the drawing
//! it asks for handed in as a closure.
use super::*;
use bravebot_agent::hooks::SessionMoments;
use serde_json::json;
use std::path::{Path, PathBuf};

/// A scratch checkout and the state directory under it, both new.
fn a_checkout(name: &str) -> (PathBuf, PathBuf) {
    let root = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&root);
    let state = root.join("state");
    std::fs::create_dir_all(&state).expect("a state directory");
    (root, state)
}

/// Write `entries` as the hooks file in `state`.
fn declaring(state: &Path, entries: serde_json::Value) {
    std::fs::write(
        state.join("hooks.json"),
        json!({ "hooks": entries }).to_string(),
    )
    .expect("a hooks file");
}

/// A hook on `moment` that appends a line to `log`, started as a program with the log as its
/// argument so a path with a space in it is still one word.
fn appending_to(moment: &str, log: &Path) -> serde_json::Value {
    json!({
        "on": moment,
        "run": ["/bin/sh", "-c", "echo ran >> \"$1\"", "sh", log.display().to_string()],
    })
}

/// What the session put in front of the person, one entry per line.
fn said_in_the_transcript(session: &Session) -> Vec<String> {
    session
        .viewed()
        .iter()
        .map(|entry| entry.text.clone())
        .collect()
}

/// HOOK-10: where a hook will run, the opening screen is up before it does. The regressions it
/// rejects are a screen drawn after the hook, which leaves the person on a blank terminal for as long
/// as the hook holds the first prompt back, and a start that never runs the hook.
#[test]
fn the_opening_screen_is_drawn_before_a_start_hook_holds_the_first_prompt_back() {
    let (root, state) = a_checkout("session-moments-drawn-first");
    let log = root.join("log");
    declaring(&state, json!([appending_to("session-started", &log)]));
    let mut session = Session::new("none");
    let mut moments = SessionMoments::read(Some(&state), &root);
    let mut hook_had_run_at_each_draw = Vec::new();

    start_the_session(
        &mut session,
        &mut moments,
        &mut (),
        |(), _| {
            hook_had_run_at_each_draw.push(log.exists());
            Ok(())
        },
        |(), moments| moments.started(),
    )
    .expect("the start");

    assert_eq!(
        hook_had_run_at_each_draw,
        [false],
        "the opening screen was not drawn once, ahead of the hook"
    );
    assert_eq!(
        std::fs::read_to_string(&log).expect("the hook ran"),
        "ran\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// HOOK-10: where no hook will run there is no early frame, because the loop draws its own first
/// frame and a second would be a flicker. The regression it rejects is drawing before every start.
#[test]
fn no_early_frame_is_drawn_where_no_start_hook_will_run() {
    let (root, state) = a_checkout("session-moments-no-early-frame");
    let log = root.join("log");
    declaring(
        &state,
        json!([
            appending_to("turn-started", &log),
            appending_to("session-ended", &log),
        ]),
    );
    let mut session = Session::new("none");
    let mut moments = SessionMoments::read(Some(&state), &root);
    let mut frames = 0;

    start_the_session(
        &mut session,
        &mut moments,
        &mut (),
        |(), _| {
            frames += 1;
            Ok(())
        },
        |(), moments| moments.started(),
    )
    .expect("the start");

    assert_eq!(
        frames, 0,
        "a frame was drawn ahead of a hook that is not there"
    );
    assert!(
        !log.exists(),
        "a hook written for another moment ran at the start"
    );
    assert!(said_in_the_transcript(&session).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

/// HOOK-10, HOOK-7: what a start hook that could not run said is in the transcript when the
/// interface opens, naming the moment and the program, and is said once for the interface however
/// many conversations `/resume` opens in it. The regressions it rejects are a sentence dropped on the
/// way to the screen, which leaves a hook that quietly stopped working, and a start fired again for
/// each conversation.
#[test]
fn a_start_hook_that_could_not_run_is_noted_once_however_many_conversations_open() {
    let (root, state) = a_checkout("session-moments-noted-once");
    declaring(
        &state,
        json!([{"on": "session-started", "run": ["/no/such/opener"]}]),
    );
    let mut moments = SessionMoments::read(Some(&state), &root);

    let mut first = Session::new("none");
    start_the_session(
        &mut first,
        &mut moments,
        &mut (),
        |(), _| Ok(()),
        |(), moments| moments.started(),
    )
    .expect("the start");
    let mut resumed = Session::new("none");
    start_the_session(
        &mut resumed,
        &mut moments,
        &mut (),
        |(), _| Ok(()),
        |(), moments| moments.started(),
    )
    .expect("the second start");

    let said = said_in_the_transcript(&first);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("session-started") && said[0].contains("/no/such/opener"),
        "{said:?}"
    );
    assert!(
        said_in_the_transcript(&resumed).is_empty(),
        "a second conversation said the start again: {:?}",
        said_in_the_transcript(&resumed)
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// PLUGIN-7, S10, S11 in the full-screen interface at `session-started`: a plugin's hook on it is
/// asked about, after the opening screen is drawn (so the question has a screen under it), and runs
/// after the yes and not before. The question is put through the confirmer the interface hands the
/// moment, here one that records the question and the screen state when it came. The regressions it
/// rejects are a question put before any screen was drawn, a hook run before it was answered, and a
/// start that never reaches the plugin's hook.
#[cfg(unix)]
#[test]
fn a_plugin_hook_at_the_start_is_asked_after_the_screen_is_up_and_runs_after_the_yes() {
    use bravebot_agent::confirm::{AnswersHooks, Decision};
    use std::os::unix::fs::PermissionsExt;
    let (root, state) = a_checkout("session-moments-plugin-start");
    let marker = root.join("marker");
    let source = root.join("source");
    std::fs::create_dir_all(&source).expect("source");
    std::fs::write(
        source.join("plugin.toml"),
        "name = \"demo\"\nversion = \"1.0.0\"\ndescription = \"A demo\"\nhooks = \"hooks.json\"\n",
    )
    .expect("manifest");
    std::fs::write(
        source.join("hooks.json"),
        r#"{"hooks":[{"on":"session-started","run":["${PLUGIN_ROOT}/hook.sh"]}]}"#,
    )
    .expect("hooks");
    let script = source.join("hook.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\necho ran >> '{}'\n", marker.display()),
    )
    .expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("executable");
    bravebot_agent::plugins::install(&source, &state).expect("install");

    let mut session = Session::new("none");
    let mut moments = SessionMoments::read(Some(&state), &root);
    moments.keep_answers(session.account_session().clone());
    let mut person = AnswersHooks::new(Decision::Approve);
    let mut drawn_before_the_question = None;
    let mut ran_before_the_question = None;

    start_the_session(
        &mut session,
        &mut moments,
        &mut (
            &mut person,
            &mut drawn_before_the_question,
            &mut ran_before_the_question,
        ),
        |(_, drawn, _), _| {
            **drawn = Some(true);
            Ok(())
        },
        |(person, drawn, ran), moments| {
            **ran = Some(marker.exists());
            let _ = drawn;
            moments.started_asking(&mut **person)
        },
    )
    .expect("the start");

    assert_eq!(
        drawn_before_the_question,
        Some(true),
        "no screen was drawn first"
    );
    assert_eq!(
        ran_before_the_question,
        Some(false),
        "ran before it was answered"
    );
    assert_eq!(person.asked.len(), 1, "{:?}", person.asked);
    assert_eq!(person.asked[0].moment, "session-started");
    assert!(marker.exists(), "the hook did not run after the yes");
    let _ = std::fs::remove_dir_all(&root);
}

/// PLUGIN-7 in the full-screen interface at `file-changed`: the loop puts a plugin's question only
/// where the box holds no draft, so a key typed into a draft is never taken as the answer, and not
/// over the Sessions screen. The loop needs a real terminal, so this is the condition it tests. The
/// regressions it rejects are a question put over a draft in the box, one put over the Sessions
/// screen, and one never put at all.
#[test]
fn a_plugin_question_is_put_only_with_an_empty_box_and_no_sessions_screen() {
    let mut session = Session::new("none");
    assert!(
        may_put_a_plugin_question(&session),
        "an empty box was not served"
    );

    session.paste("half a sentence");
    assert!(
        !may_put_a_plugin_question(&session),
        "a question was put over a draft"
    );

    let mut over_the_sessions_screen = Session::new("none");
    over_the_sessions_screen.open_sessions_screen(
        crate::sessions_screen::fixtures::sessions_screen_of(Vec::new()),
    );
    assert!(over_the_sessions_screen.input().is_empty());
    assert!(
        !may_put_a_plugin_question(&over_the_sessions_screen),
        "a question was put over the Sessions screen"
    );
}
