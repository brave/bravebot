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

    start_the_session(&mut session, &mut moments, |_| {
        hook_had_run_at_each_draw.push(log.exists());
        Ok(())
    })
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

    start_the_session(&mut session, &mut moments, |_| {
        frames += 1;
        Ok(())
    })
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
    start_the_session(&mut first, &mut moments, |_| Ok(())).expect("the start");
    let mut resumed = Session::new("none");
    start_the_session(&mut resumed, &mut moments, |_| Ok(())).expect("the second start");

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
