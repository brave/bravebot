//! `bravebot sessions`: the sessions that keep running after the terminal closes (BG-5).
//!
//! Everything drawn here is a thing the person typed, a fact about a process, or a word from a
//! fixed set. The roster has no field for anything else (BG-4), so there is nothing in it to
//! draw that a model or a tool wrote.

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use bravebot_session::jobs::{Held, Lookup, Roster, Seen, State, Stopped};
use std::io::Write;
use std::process::ExitCode;

/// How long a stop waits for a session to end before it is killed.
const GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// The characters of an id a line shows, which is enough to name it to `attach` and `reply`.
const ID_SHOWN: usize = 8;

/// `bravebot sessions [--json]` and `bravebot sessions stop <id>`.
pub(crate) fn sessions(args: &[String]) -> ExitCode {
    match args {
        [] => list(false),
        [flag] if flag == "--json" => list(true),
        [word, id] if word == "stop" => stop(id),
        _ => fail(Ending::Argument, t!(sessions_usage)),
    }
}

fn list(as_json: bool) -> ExitCode {
    let seen = Roster::readable()
        .map(|roster| roster.list())
        .unwrap_or_default();
    let mut out = std::io::stdout().lock();
    if as_json {
        let _ = writeln!(out, "{}", bravebot_session::jobs::as_json(&seen));
    } else if seen.is_empty() {
        let _ = writeln!(out, "{}", t!(sessions_none));
    } else {
        for line in lines_of(&seen) {
            let _ = writeln!(out, "{line}");
        }
    }
    ExitCode::SUCCESS
}

fn stop(typed: &str) -> ExitCode {
    let Some(roster) = Roster::writable() else {
        return fail(Ending::Failed, t!(sessions_no_home));
    };
    let seen = match roster.find(typed) {
        Ok(seen) => seen,
        Err(Lookup::Missing) => {
            return fail(Ending::Argument, t!(sessions_missing, id = shown(typed)));
        }
        Err(Lookup::Ambiguous) => {
            return fail(Ending::Argument, t!(sessions_ambiguous, id = shown(typed)));
        }
    };
    match roster.stop(&seen.job.id, GRACE) {
        Ok(Stopped::Stopped) => {
            println!("{}", t!(sessions_stopped, name = shown(&seen.job.name)));
            ExitCode::SUCCESS
        }
        Ok(Stopped::AlreadyNotRunning) => {
            println!("{}", t!(sessions_not_running, name = shown(&seen.job.name)));
            ExitCode::SUCCESS
        }
        Ok(Stopped::Missing) => fail(Ending::Argument, t!(sessions_missing, id = shown(typed))),
        Err(err) => fail(Ending::Failed, t!(sessions_stop_failed, problem = err)),
    }
}

/// Text the person typed, as it can be drawn: control characters pictured, and a tab a space, so
/// nothing typed can move the cursor, recolour the line or start another one.
fn shown(typed: &str) -> String {
    crate::progress::printable(typed).replace('\t', " ")
}

/// One line for each session: the start of its id, its name, its state, where it works, how long
/// since its last turn, and the last prompt the person typed.
///
/// A prompt that is held shows as needing input and the one word for its kind, and nothing of
/// what it asks (BG-5).
fn lines_of(seen: &[Seen]) -> Vec<String> {
    seen.iter()
        .map(|seen| {
            let job = &seen.job;
            let id: String = job.id.chars().take(ID_SHOWN).collect();
            let since =
                bravebot_session::sessions::how_long_ago(job.last_turn.unwrap_or(job.started));
            format!(
                "{id}  {}  {}  {}  {since}  {}",
                shown(&job.name),
                state_in_words(seen),
                shown(&job.directory),
                shown(&bravebot_session::sessions::title_from(&job.prompt)),
            )
        })
        .collect()
}

fn state_in_words(seen: &Seen) -> String {
    match (seen.state(), seen.held()) {
        (State::Working, _) => t!(sessions_state_working).to_string(),
        (State::NeedsInput, Some(held)) => t!(
            sessions_state_needs_input,
            kind = match held {
                Held::Write => t!(sessions_held_write),
                Held::Run => t!(sessions_held_run),
                Held::Read => t!(sessions_held_read),
                Held::Fetch => t!(sessions_held_fetch),
                Held::Server => t!(sessions_held_server),
                Held::Vouch => t!(sessions_held_vouch),
                Held::Tools => t!(sessions_held_tools),
                Held::Move => t!(sessions_held_move),
                Held::Manifest => t!(sessions_held_manifest),
                Held::Question => t!(sessions_held_question),
            }
        )
        .to_string(),
        (State::NeedsInput, None) => t!(sessions_state_needs_input_unnamed).to_string(),
        (State::Idle, _) => t!(sessions_state_idle).to_string(),
        (State::Stopped, _) => t!(sessions_state_stopped).to_string(),
        (State::Interrupted, _) => t!(sessions_state_interrupted).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_session::jobs::{Job, Mode};
    use std::path::Path;

    fn seen(prompt: &str, state: State, held: Option<Held>, live: bool) -> Seen {
        let mut job = Job::starting(
            "11111111-1111-4111-8111-111111111111".to_string(),
            Path::new("/work/project"),
            prompt,
            Mode::Ask,
        );
        job.is(state, held);
        Seen { job, live }
    }

    /// BG-5: a name, a prompt or a directory with a control character in it cannot start another
    /// line, move the cursor or recolour the screen.
    #[test]
    fn a_control_character_in_what_was_typed_cannot_write_a_row() {
        let mut one = seen("fix\x1b[31m the\nbuild\r\x07", State::Idle, None, true);
        one.job.directory = "/work/\x1b]0;title\x07/pro\nject".to_string();
        let rows = lines_of(&[one]);
        assert_eq!(rows.len(), 1, "{rows:?}");
        for row in &rows {
            assert!(!row.contains('\n'), "{row:?}");
            assert!(!row.chars().any(|c| c.is_control()), "{row:?}");
        }
        assert!(rows[0].contains('\u{241b}'), "{rows:?}");
    }

    /// BG-5: a held prompt shows as needing input and the word for its kind.
    #[test]
    fn a_held_prompt_shows_its_kind_and_nothing_else() {
        let rows = lines_of(&[seen(
            "fix the build",
            State::NeedsInput,
            Some(Held::Run),
            true,
        )]);
        assert!(
            rows[0].contains(&t!(sessions_held_run).to_string()),
            "{rows:?}"
        );
        assert!(rows[0].contains("fix the build"), "{rows:?}");
    }

    /// BG-6: the list says what is true of the process now. An entry that said it was working and
    /// has no process is listed as interrupted, and its kind is not listed at all.
    #[test]
    fn a_session_with_no_process_is_not_listed_as_working() {
        let working = t!(sessions_state_working).to_string();
        let held = t!(sessions_held_run).to_string();
        let gone = seen("fix the build", State::Working, None, false);
        let row = &lines_of(&[gone])[0];
        assert!(!row.contains(&working), "{row}");
        assert!(
            row.contains(&t!(sessions_state_interrupted).to_string()),
            "{row}"
        );

        let gone = seen("fix the build", State::NeedsInput, Some(Held::Run), false);
        let row = &lines_of(&[gone])[0];
        assert!(!row.contains(&held), "{row}");
    }
}
