//! `/sandbox`: the mode programs `run` starts are held to, changed from the next turn (SANDBOX-22).
//!
//! A person's word and nobody else's: the line is dispatched from the input box like every other
//! command (CMD-1), and nothing here reads a model reply, a skill, a hook or an `AGENTS.md` line.
//! The planner is told the mode in force, as the `run` description already is, and nothing about
//! the command.
//!
//! The decision is a pure function of what was typed, the mode held now and the managed file, so
//! what a refusal and a change each turn on is tested without a terminal. The word is the person's
//! own, so `off` is made when it is typed, as every other mode is, and nothing is asked.

use bravebot_config::Managed;
use bravebot_config::sandbox::{Choice, Floor, Refused, Source, allowed_in_session, in_force};
use bravebot_i18n::t;
use bravebot_sandbox::SandboxMode;

/// The mode a session holds and where it came from.
///
/// Held by the session and not read from [`in_force`] at each turn: that is what start-up settled
/// and cannot change, and this is what `/sandbox` moves. It is carried across `/resume`, so the
/// choice lasts until the process ends and no longer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandboxed {
    pub choice: Choice,
    /// Whether `/sandbox` set it, which the report says in place of the flag or file that chose the
    /// mode at start-up.
    pub typed: bool,
}

impl Sandboxed {
    /// What start-up chose.
    pub fn at_start() -> Self {
        Self {
            choice: in_force(),
            typed: false,
        }
    }

    /// What the command chose. It ranks as `--sandbox` does, which is the only place it is read.
    pub fn typed(mode: SandboxMode) -> Self {
        Self {
            choice: Choice {
                mode,
                source: Source::Flag,
            },
            typed: true,
        }
    }
}

/// What a `/sandbox` line comes to.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Say this and change nothing.
    Say(String),
    /// Move to this mode from the next turn.
    Move(SandboxMode),
}

/// The words a person may type, as they are listed where a word is refused.
fn names() -> String {
    SandboxMode::ALL.map(SandboxMode::name).join(", ")
}

/// The mode in force and the flag or file that chose it, in the words `doctor` uses.
fn report(held: &Sandboxed) -> String {
    let mode = held.choice.mode.name();
    let detail = match (&held.choice.source, held.typed) {
        (_, true) => t!(session_sandbox_from_command, mode = mode),
        (Source::Default, false) => t!(doctor_sandbox_default, mode = mode),
        (Source::Flag, false) => t!(session_sandbox_from_flag, mode = mode),
        (Source::File(path) | Source::Managed(path), false) => t!(
            doctor_sandbox_from,
            mode = mode,
            path = path.display().to_string()
        ),
    };
    t!(session_sandbox_report, detail = detail)
}

/// The sentence a mode the managed file does not allow is refused with, naming the file.
fn refusal(refused: &Refused) -> String {
    let pinned_in = refused.pinned_in.display().to_string();
    match refused.because {
        Floor::Mode => t!(
            session_sandbox_refused_mode,
            asked = refused.asked.name(),
            pinned = refused.pinned.name(),
            pinned_in = pinned_in,
            looser = bravebot_agent::sandbox_reason::looser(refused)
        ),
        Floor::Network => t!(
            session_sandbox_refused_network,
            asked = refused.asked.name(),
            pinned_in = pinned_in
        ),
    }
}

/// Decide what `word`, the argument after `/sandbox`, comes to.
///
/// The managed floor is checked first, so a mode it does not allow is refused with the file named
/// and is never moved to.
pub fn decide(word: &str, held: &Sandboxed, managed: &Managed) -> Step {
    if word.is_empty() {
        return Step::Say(report(held));
    }
    let Some(mode) = SandboxMode::parse(word) else {
        return Step::Say(t!(session_sandbox_needs_a_mode, names = names()));
    };
    if let Err(refused) = allowed_in_session(mode, managed) {
        return Step::Say(refusal(&refused));
    }
    if mode == held.choice.mode {
        return Step::Say(t!(session_sandbox_already, mode = mode.name()));
    }
    Step::Move(mode)
}

/// Carry out `/sandbox <word>`.
pub fn run(session: &mut crate::state::Session, word: &str, managed: &Managed) {
    let moved = match decide(word, session.sandbox(), managed) {
        Step::Say(sentence) => {
            session.note(sentence);
            return;
        }
        Step::Move(mode) => mode,
    };
    session.set_sandbox_mode(moved);
    session.note(match moved {
        SandboxMode::Off => t!(session_sandbox_set_off).to_string(),
        mode => t!(session_sandbox_set, mode = mode.name()),
    });
    // A job keeps the profile it started under, so a move to `read-only` leaves each running one
    // writing where it did. The names are the driver's own (`job:1`), never a job's output.
    let running = session.running_job_names();
    if moved == SandboxMode::ReadOnly && !running.is_empty() {
        session.note(t!(
            session_sandbox_jobs_keep_their_mode,
            jobs = running.join(", ")
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Session;

    fn pinned(name: &str, text: &str) -> Managed {
        let dir = crate::testutil::scratch_dir(&format!("sandbox-command-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        let file = dir.join("managed.json");
        std::fs::write(&file, text).expect("managed file");
        Managed::at(&file)
    }

    fn last_note(session: &Session) -> String {
        session
            .transcript
            .last()
            .map(|entry| entry.text.clone())
            .unwrap_or_default()
    }

    /// SANDBOX-22: `/sandbox strict` and `/sandbox workspace-write` move the mode the session hands its
    /// next turn, with no question. The regression it rejects is a command that reports a change
    /// and leaves the mode where start-up put it, which is what every turn builder read before
    /// this command existed.
    #[test]
    fn a_named_mode_is_the_mode_the_next_turn_is_built_with() {
        let mut session = Session::new("kernel-enforced");
        assert_eq!(session.sandbox_mode(), SandboxMode::WorkspaceWrite);

        run(&mut session, "strict", &Managed::default());
        assert_eq!(session.sandbox_mode(), SandboxMode::Strict);
        assert!(
            last_note(&session).contains("strict"),
            "{}",
            last_note(&session)
        );

        run(&mut session, "read-only", &Managed::default());
        assert_eq!(session.sandbox_mode(), SandboxMode::ReadOnly);
        assert!(
            last_note(&session).contains("read-only"),
            "{}",
            last_note(&session)
        );

        run(&mut session, "workspace-write", &Managed::default());
        assert_eq!(session.sandbox_mode(), SandboxMode::WorkspaceWrite);
    }

    /// SANDBOX-22: a move to `read-only` names each background job still running, which keeps the
    /// mode it started under, and says `/jobs stop` ends it. The regression it rejects is a reply
    /// that says nothing of a job still writing the project, or that names a job that has ended.
    #[test]
    fn a_move_to_read_only_names_the_jobs_that_keep_the_earlier_mode() {
        let start = |session: &mut Session, name: &str| {
            session.job(bravebot_agent::report::JobEvent::Started {
                name: name.to_string(),
                line: "while true; do date >> log.txt; done".to_string(),
                moved_after: None,
                stop: bravebot_core::cancel::JobStop::new(),
            });
        };
        let mut session = Session::new("kernel-enforced");
        run(&mut session, "read-only", &Managed::default());
        assert!(
            !last_note(&session).contains("/jobs stop"),
            "{}",
            last_note(&session)
        );

        let mut session = Session::new("kernel-enforced");
        start(&mut session, "job:1");
        start(&mut session, "job:2");
        session.job(bravebot_agent::report::JobEvent::Ended {
            name: "job:2".to_string(),
            outcome: bravebot_agent::report::Outcome::Succeeded,
        });
        run(&mut session, "read-only", &Managed::default());
        let note = last_note(&session);
        assert!(
            note.contains("job:1") && note.contains("/jobs stop"),
            "{note}"
        );
        assert!(!note.contains("job:2"), "{note}");

        let mut session = Session::new("kernel-enforced");
        start(&mut session, "job:1");
        run(&mut session, "strict", &Managed::default());
        assert!(
            !last_note(&session).contains("job:1"),
            "{}",
            last_note(&session)
        );
    }

    /// SANDBOX-22, COMPAT-001: `/sandbox standard`, the earlier word for `workspace-write`, still
    /// moves the session to it and the report names the current word. The regression it rejects is
    /// a command that answers a word people typed for as long as the mode existed with the list of
    /// modes.
    #[test]
    fn the_earlier_word_for_workspace_write_still_moves_the_mode() {
        let mut session = Session::new("kernel-enforced");
        run(&mut session, "strict", &Managed::default());
        assert_eq!(session.sandbox_mode(), SandboxMode::Strict);

        run(&mut session, "standard", &Managed::default());
        assert_eq!(session.sandbox_mode(), SandboxMode::WorkspaceWrite);
        assert!(
            last_note(&session).contains("workspace-write"),
            "{}",
            last_note(&session)
        );
    }

    /// SANDBOX-22: the session that `/resume` or `/clear` starts is built from the held choice of
    /// the one that ended, and reports it as the command's. The regression it rejects is a new
    /// session that takes `at_start()` again, which puts back the mode start-up chose. The loop's
    /// two hand-offs (`Switch.sandbox` in `run`, `with_sandbox` in `event_loop`) are reasoned only:
    /// they need a terminal.
    #[test]
    fn a_session_started_from_the_held_choice_keeps_the_commands_mode() {
        let mut first = Session::new("kernel-enforced");
        run(&mut first, "strict", &Managed::default());

        let next = Session::new("kernel-enforced").with_sandbox(first.sandbox().clone());
        assert_eq!(next.sandbox_mode(), SandboxMode::Strict);
        assert!(
            next.sandbox().typed,
            "the report must still name the command"
        );
        assert_eq!(next.sandbox(), first.sandbox());
    }

    /// SANDBOX-22: `/sandbox off` moves the mode the next turn is built with at once, from every
    /// mode and with nothing to answer, and the reply says that programs then have no profile. The
    /// regression it rejects is a move to `off` that stops at a question, which leaves the mode where
    /// it was for a person who typed the word on purpose.
    #[test]
    fn a_move_to_off_is_made_when_typed_and_asks_nothing() {
        for held in [
            SandboxMode::Strict,
            SandboxMode::ReadOnly,
            SandboxMode::WorkspaceWrite,
        ] {
            let mut session = Session::new("kernel-enforced");
            session.set_sandbox_mode(held);

            run(&mut session, "off", &Managed::default());

            assert_eq!(session.sandbox_mode(), SandboxMode::Off, "from {held:?}");
            assert_eq!(
                last_note(&session),
                t!(session_sandbox_set_off).to_string(),
                "from {held:?}"
            );
            assert!(
                session.sandbox().typed,
                "the report must name the command, from {held:?}"
            );
        }
    }

    /// SANDBOX-22: the managed floor holds for the command as it does for the flag, the refusal
    /// names the file, and the mode stays, `off` included. The regression it rejects is a command
    /// that skips the start-up check, which would now move to `off` on the strength of the word alone.
    #[test]
    fn the_managed_floor_refuses_a_looser_mode_naming_the_file() {
        let strict = pinned("strict", r#"{"sandbox": {"mode": "strict"}}"#);
        for word in ["workspace-write", "off"] {
            let mut session = Session::new("kernel-enforced");
            session.set_sandbox_mode(SandboxMode::Strict);
            run(&mut session, word, &strict);
            assert_eq!(session.sandbox_mode(), SandboxMode::Strict, "{word}");
            let said = last_note(&session);
            assert!(
                said.contains("managed.json") && said.contains("strict"),
                "{said}"
            );
        }

        let closed = pinned("closed", r#"{"run": {"network": "closed"}}"#);
        let mut session = Session::new("kernel-enforced");
        run(&mut session, "off", &closed);
        assert_eq!(session.sandbox_mode(), SandboxMode::WorkspaceWrite);
        let said = last_note(&session);
        assert!(
            said.contains("managed.json") && said.contains("run.network"),
            "{said}"
        );
        run(&mut session, "strict", &closed);
        assert_eq!(session.sandbox_mode(), SandboxMode::Strict);
    }

    /// SANDBOX-22: the refusal says which of reading and writing the asked mode does more of than
    /// the pin. The modes are not a line, so the fault it rejects is a sentence that calls a mode
    /// looser and leaves a person under a `strict` pin guessing why `read-only` is.
    #[test]
    fn a_refusal_says_which_of_reading_and_writing_the_mode_does_more_of() {
        for (pin, held, word, said) in [
            (
                "read-only",
                SandboxMode::ReadOnly,
                "strict",
                "strict writes more than read-only.",
            ),
            (
                "strict",
                SandboxMode::Strict,
                "read-only",
                "read-only reads more of the machine than strict.",
            ),
            (
                "strict",
                SandboxMode::Strict,
                "workspace-write",
                "workspace-write reads more of the machine and writes more than strict.",
            ),
        ] {
            let managed = pinned(
                &format!("looser-{pin}-{word}"),
                &format!(r#"{{"sandbox": {{"mode": "{pin}"}}}}"#),
            );
            let mut session = Session::new("kernel-enforced");
            session.set_sandbox_mode(held);
            run(&mut session, word, &managed);
            assert_eq!(session.sandbox_mode(), held, "{word}");
            let note = last_note(&session);
            assert!(note.contains(said), "{note}");
            assert!(!note.contains("stricter"), "{note}");
        }
    }

    /// SANDBOX-22: a word that is not exactly a mode changes nothing and lists the four, and the
    /// bare word reports without changing. The regression it rejects is a near miss, `Strict` or
    /// `strict now`, read as the mode it resembles.
    #[test]
    fn a_word_that_is_not_a_mode_changes_nothing_and_the_bare_word_only_reports() {
        let mut session = Session::new("kernel-enforced");
        for word in ["Strict", "strict now", "none", "--strict"] {
            run(&mut session, word, &Managed::default());
            assert_eq!(
                session.sandbox_mode(),
                SandboxMode::WorkspaceWrite,
                "{word}"
            );
            let said = last_note(&session);
            assert!(
                said.contains("strict")
                    && said.contains("read-only")
                    && said.contains("workspace-write")
                    && said.contains("off"),
                "{said}"
            );
        }
        run(&mut session, "", &Managed::default());
        assert_eq!(session.sandbox_mode(), SandboxMode::WorkspaceWrite);
        assert!(
            last_note(&session).contains("workspace-write"),
            "{}",
            last_note(&session)
        );
    }

    /// SANDBOX-22: the report says what chose the mode, and after the command says the command did.
    /// The regression it rejects is a report that keeps naming the start-up choice once `/sandbox`
    /// has overruled it.
    #[test]
    fn the_report_names_what_chose_the_mode() {
        let from_file = Sandboxed {
            choice: Choice {
                mode: SandboxMode::Strict,
                source: Source::File("/home/a/.bravebot/settings.json".into()),
            },
            typed: false,
        };
        let said = match decide("", &from_file, &Managed::default()) {
            Step::Say(said) => said,
            other => panic!("{other:?}"),
        };
        assert!(
            said.contains("strict") && said.contains("settings.json"),
            "{said}"
        );

        let typed = Sandboxed {
            typed: true,
            ..from_file.clone()
        };
        let said = match decide("", &typed, &Managed::default()) {
            Step::Say(said) => said,
            other => panic!("{other:?}"),
        };
        assert!(
            said.contains("/sandbox") && !said.contains("settings.json"),
            "{said}"
        );
    }

    /// SANDBOX-22: asking for the mode already held says so and changes nothing, `off` included.
    #[test]
    fn the_mode_already_held_is_said_and_not_moved_to() {
        let mut session = Session::new("kernel-enforced");
        run(&mut session, "workspace-write", &Managed::default());
        assert!(
            last_note(&session).contains("already"),
            "{}",
            last_note(&session)
        );
        session.set_sandbox_mode(SandboxMode::Off);
        run(&mut session, "off", &Managed::default());
        assert_eq!(session.sandbox_mode(), SandboxMode::Off);
        assert_eq!(
            last_note(&session),
            t!(session_sandbox_already, mode = "off").to_string()
        );
    }
}
