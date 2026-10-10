//! `/sandbox`: the mode programs `run` starts are held to, changed from the next turn (SANDBOX-22).
//!
//! A person's word and nobody else's: the line is dispatched from the input box like every other
//! command (CMD-1), and nothing here reads a model reply, a skill, a hook or an `AGENTS.md` line.
//! The planner is told the mode in force, as the `run` description already is, and nothing about
//! the command.
//!
//! The decision is a pure function of what was typed, the mode held now and the managed file, so
//! what a refusal, a question and a change each turn on is tested without a terminal. Asking is the
//! caller's, because it needs the screen.

use bravebot_config::Managed;
use bravebot_config::sandbox::{Choice, Floor, Refused, Source, allowed_in_session, in_force};
use bravebot_core::ask::{Answer, Asking, Prompt, Row};
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
    /// Ask before moving to `off`, which starts programs with no profile.
    Ask,
    /// Move to this mode from the next turn.
    Move(SandboxMode),
}

/// The three words a person may type, as they are listed where a word is refused.
fn names() -> String {
    [SandboxMode::Strict, SandboxMode::Standard, SandboxMode::Off]
        .map(SandboxMode::name)
        .join(", ")
}

/// The mode in force and the flag or file that chose it, in the words `doctor` uses.
fn report(held: &Sandboxed) -> String {
    let mode = held.choice.mode.name();
    let detail = match (&held.choice.source, held.typed) {
        (_, true) => t!(session_sandbox_from_command, mode = mode).to_string(),
        (Source::Default, false) => t!(doctor_sandbox_default, mode = mode).to_string(),
        (Source::Flag, false) => t!(session_sandbox_from_flag, mode = mode).to_string(),
        (Source::File(path) | Source::Managed(path), false) => t!(
            doctor_sandbox_from,
            mode = mode,
            path = path.display().to_string()
        )
        .to_string(),
    };
    t!(session_sandbox_report, detail = detail).to_string()
}

/// The sentence a mode the managed file does not allow is refused with, naming the file.
fn refusal(refused: &Refused) -> String {
    let pinned_in = refused.pinned_in.display().to_string();
    match refused.because {
        Floor::Mode => t!(
            session_sandbox_refused_mode,
            asked = refused.asked.name(),
            pinned = refused.pinned.name(),
            pinned_in = pinned_in
        ),
        Floor::Network => t!(
            session_sandbox_refused_network,
            asked = refused.asked.name(),
            pinned_in = pinned_in
        ),
    }
    .to_string()
}

/// Decide what `word`, the argument after `/sandbox`, comes to.
///
/// The managed floor is checked before anything is asked, so a person is never put a question whose
/// answer cannot be obeyed.
pub fn decide(word: &str, held: &Sandboxed, managed: &Managed) -> Step {
    if word.is_empty() {
        return Step::Say(report(held));
    }
    let Some(mode) = SandboxMode::parse(word) else {
        return Step::Say(t!(session_sandbox_needs_a_mode, names = names()).to_string());
    };
    if let Err(refused) = allowed_in_session(mode, managed) {
        return Step::Say(refusal(&refused));
    }
    match mode {
        _ if mode == held.choice.mode => {
            Step::Say(t!(session_sandbox_already, mode = mode.name()).to_string())
        }
        SandboxMode::Off => Step::Ask,
        _ => Step::Move(mode),
    }
}

/// The question put before programs start with no profile.
///
/// The row that keeps the mode is first, so Enter on the question as drawn changes nothing, and a
/// reply typed in the person's own words is read as that row too.
pub fn question(keeping: SandboxMode) -> Asking {
    Asking {
        prompts: vec![Prompt {
            header: t!(ask_sandbox_off_header).to_string(),
            question: t!(ask_sandbox_off_question).to_string(),
            rows: vec![
                Row {
                    index: 0,
                    label: t!(ask_sandbox_off_no, mode = keeping.name()).to_string(),
                    detail: None,
                },
                Row {
                    index: 1,
                    label: t!(ask_sandbox_off_yes).to_string(),
                    detail: Some(t!(ask_sandbox_off_yes_detail).to_string()),
                },
            ],
            multiple: false,
            key: String::new(),
        }],
    }
}

/// Whether the answers to [`question`] say yes. Anything but the one row that does is a no.
pub fn said_yes(answers: &[Answer]) -> bool {
    matches!(answers, [Answer::Chosen(rows)] if rows.as_slice() == [1])
}

/// Carry out `/sandbox <word>`, asking through `ask` where the move is to `off`.
pub fn run(
    session: &mut crate::state::Session,
    word: &str,
    managed: &Managed,
    ask: impl FnOnce(&Asking) -> Vec<Answer>,
) {
    let kept = session.sandbox_mode();
    let moved = match decide(word, session.sandbox(), managed) {
        Step::Say(sentence) => {
            session.note(sentence);
            return;
        }
        Step::Ask => {
            if !said_yes(&ask(&question(kept))) {
                session.note(t!(session_sandbox_kept, mode = kept.name()).to_string());
                return;
            }
            SandboxMode::Off
        }
        Step::Move(mode) => mode,
    };
    session.set_sandbox_mode(moved);
    session.note(match moved {
        SandboxMode::Off => t!(session_sandbox_set_off).to_string(),
        mode => t!(session_sandbox_set, mode = mode.name()).to_string(),
    });
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

    fn never_asked(_: &Asking) -> Vec<Answer> {
        panic!("a question was put where none was owed");
    }

    fn last_note(session: &Session) -> String {
        session
            .transcript
            .last()
            .map(|entry| entry.text.clone())
            .unwrap_or_default()
    }

    /// SANDBOX-22: `/sandbox strict` and `/sandbox standard` move the mode the session hands its
    /// next turn, with no question. The regression it rejects is a command that reports a change
    /// and leaves the mode where start-up put it, which is what every turn builder read before
    /// this command existed.
    #[test]
    fn a_named_mode_is_the_mode_the_next_turn_is_built_with() {
        let mut session = Session::new("kernel-enforced");
        assert_eq!(session.sandbox_mode(), SandboxMode::Standard);

        run(&mut session, "strict", &Managed::default(), never_asked);
        assert_eq!(session.sandbox_mode(), SandboxMode::Strict);
        assert!(
            last_note(&session).contains("strict"),
            "{}",
            last_note(&session)
        );

        run(&mut session, "standard", &Managed::default(), never_asked);
        assert_eq!(session.sandbox_mode(), SandboxMode::Standard);
    }

    /// SANDBOX-22: `off` asks first, and a no, a decline and a reply in the person's own words all
    /// keep the mode. The regression it rejects is a move to `off` made before the question is
    /// answered, which unconfines programs on the strength of a line that may have been a slip.
    #[test]
    fn a_move_to_off_asks_and_anything_but_a_yes_keeps_the_mode() {
        for answers in [
            vec![Answer::Chosen(vec![0])],
            vec![Answer::Declined],
            vec![Answer::Typed("yes".to_string())],
            Vec::new(),
        ] {
            let mut session = Session::new("kernel-enforced");
            run(&mut session, "strict", &Managed::default(), never_asked);
            let mut asked = 0;
            run(&mut session, "off", &Managed::default(), |asking| {
                asked += 1;
                assert!(
                    asking.prompts[0].question.contains("no profile"),
                    "the question does not say what off does"
                );
                answers.clone()
            });
            assert_eq!(asked, 1, "{answers:?}");
            assert_eq!(session.sandbox_mode(), SandboxMode::Strict, "{answers:?}");
        }

        let mut session = Session::new("kernel-enforced");
        run(&mut session, "off", &Managed::default(), |_| {
            vec![Answer::Chosen(vec![1])]
        });
        assert_eq!(session.sandbox_mode(), SandboxMode::Off);
        assert!(
            last_note(&session).contains("no profile"),
            "{}",
            last_note(&session)
        );
    }

    /// SANDBOX-22: the question's first row, the one Enter lands on, is the one that keeps the
    /// mode. The regression it rejects is the yes row first, which makes a held Enter unconfine.
    #[test]
    fn the_row_the_cursor_starts_on_keeps_the_mode() {
        let asking = question(SandboxMode::Strict);
        let rows = &asking.prompts[0].rows;
        assert_eq!(rows[0].index, 0);
        assert!(rows[0].label.contains("strict"), "{}", rows[0].label);
        assert!(!said_yes(&[Answer::Chosen(vec![rows[0].index])]));
        assert!(said_yes(&[Answer::Chosen(vec![rows[1].index])]));
    }

    /// SANDBOX-22: the managed floor holds for the command as it does for the flag, the refusal
    /// names the file, no question is put for a move that would be refused, and the mode stays. The
    /// regression it rejects is a command that skips the start-up check.
    #[test]
    fn the_managed_floor_refuses_a_looser_mode_naming_the_file_and_asks_nothing() {
        let strict = pinned("strict", r#"{"sandbox": {"mode": "strict"}}"#);
        for word in ["standard", "off"] {
            let mut session = Session::new("kernel-enforced");
            session.set_sandbox_mode(SandboxMode::Strict);
            run(&mut session, word, &strict, never_asked);
            assert_eq!(session.sandbox_mode(), SandboxMode::Strict, "{word}");
            let said = last_note(&session);
            assert!(
                said.contains("managed.json") && said.contains("strict"),
                "{said}"
            );
        }

        let closed = pinned("closed", r#"{"run": {"network": "closed"}}"#);
        let mut session = Session::new("kernel-enforced");
        run(&mut session, "off", &closed, never_asked);
        assert_eq!(session.sandbox_mode(), SandboxMode::Standard);
        let said = last_note(&session);
        assert!(
            said.contains("managed.json") && said.contains("run.network"),
            "{said}"
        );
        run(&mut session, "strict", &closed, never_asked);
        assert_eq!(session.sandbox_mode(), SandboxMode::Strict);
    }

    /// SANDBOX-22: a word that is not exactly a mode changes nothing and lists the three, and the
    /// bare word reports without changing. The regression it rejects is a near miss, `Strict` or
    /// `strict now`, read as the mode it resembles.
    #[test]
    fn a_word_that_is_not_a_mode_changes_nothing_and_the_bare_word_only_reports() {
        let mut session = Session::new("kernel-enforced");
        for word in ["Strict", "strict now", "none", "--strict"] {
            run(&mut session, word, &Managed::default(), never_asked);
            assert_eq!(session.sandbox_mode(), SandboxMode::Standard, "{word}");
            let said = last_note(&session);
            assert!(
                said.contains("strict") && said.contains("standard") && said.contains("off"),
                "{said}"
            );
        }
        run(&mut session, "", &Managed::default(), never_asked);
        assert_eq!(session.sandbox_mode(), SandboxMode::Standard);
        assert!(
            last_note(&session).contains("standard"),
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

    /// SANDBOX-22: asking for the mode already held says so and asks nothing, `off` included.
    #[test]
    fn the_mode_already_held_is_said_and_not_asked_about() {
        let mut session = Session::new("kernel-enforced");
        run(&mut session, "standard", &Managed::default(), never_asked);
        assert!(
            last_note(&session).contains("already"),
            "{}",
            last_note(&session)
        );
        session.set_sandbox_mode(SandboxMode::Off);
        run(&mut session, "off", &Managed::default(), never_asked);
        assert_eq!(session.sandbox_mode(), SandboxMode::Off);
    }
}
