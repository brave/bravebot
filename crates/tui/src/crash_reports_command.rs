//! `/crash-reports [on | off]`: whether crash reports are on, what holds them off, and the person's
//! own choice saved for the next start.
//!
//! The state is [`bravebot_backtrace::current_with`], the rule the desktop switch reads
//! (`crates/ui-bridge/src/crash_reports.rs`), and the write is [`bravebot_backtrace::save`], the
//! writer the switch uses. A change is refused wherever
//! [`CrashReports::person_may_change`] is false, as the switch refuses it (CRASH-11), so the two
//! front ends cannot disagree. Every line is a catalog message, and the only argument any of them
//! takes from outside is the word typed after the command.

use crate::Session;
use bravebot_backtrace::NotSaved;
use bravebot_config::Managed;
use bravebot_config::crash_reports::CrashReports;
use bravebot_i18n::t;

/// The word that turns reports on. Matched whole and in lower case.
const ON: &str = "on";
/// The word that turns reports off. Matched whole and in lower case.
const OFF: &str = "off";

/// Answer `/crash-reports` with `word` after it, saying each line in the transcript.
///
/// A change takes effect from the next start: the hook and the sender are installed once, when the
/// process starts, so this process keeps doing what it started doing until it exits.
pub fn run(session: &mut Session, word: &str, managed: &Managed) {
    let lines = answer(
        word,
        bravebot_backtrace::current_with(managed),
        bravebot_backtrace::save,
        || bravebot_backtrace::current_with(managed),
    );
    for line in lines {
        session.note(line);
    }
    if word.is_empty() {
        let reporting = bravebot_backtrace::notice_due() || bravebot_backtrace::notice_recorded();
        session.note(sending(reporting));
    }
}

/// The lines `/crash-reports` says, given the state `now`, the writer `save` and a re-read `after`
/// the write.
///
/// `save` is called only for `on` or `off` where the person's own choice decides, so a refused
/// change writes nothing.
fn answer(
    word: &str,
    now: CrashReports,
    save: impl FnOnce(bool) -> Result<(), NotSaved>,
    after: impl FnOnce() -> CrashReports,
) -> Vec<String> {
    let enabled = match word {
        "" => return vec![state(now)],
        ON => true,
        OFF => false,
        other => return vec![t!(session_crash_reports_usage, word = other).to_string()],
    };
    if !now.person_may_change() {
        return vec![refusal(now)];
    }
    let said = match save(enabled) {
        Ok(()) => match (enabled, after()) {
            (true, CrashReports::On) => t!(session_crash_reports_saved_on).to_string(),
            (false, CrashReports::OffByPerson) => t!(session_crash_reports_saved_off).to_string(),
            _ => t!(session_crash_reports_unsaved).to_string(),
        },
        Err(NotSaved::Unwritable) => t!(session_crash_reports_unsaved).to_string(),
        Err(NotSaved::NoDirectory) => t!(session_crash_reports_no_directory).to_string(),
    };
    vec![said]
}

/// One line on whether reports are on and, when they are off, what holds them off.
fn state(now: CrashReports) -> String {
    match now {
        CrashReports::On => t!(session_crash_reports_on).to_string(),
        CrashReports::OffByPerson => t!(session_crash_reports_off_by_person).to_string(),
        CrashReports::OffByEnvironment(variable) => t!(
            session_crash_reports_off_by_environment,
            variable = variable
        )
        .to_string(),
        CrashReports::OffByAdmin => t!(session_crash_reports_off_by_admin).to_string(),
        CrashReports::Incognito => t!(session_crash_reports_incognito).to_string(),
    }
}

/// Why a change was refused, naming what holds reports off.
fn refusal(now: CrashReports) -> String {
    match now {
        CrashReports::OffByEnvironment(variable) => t!(
            session_crash_reports_refused_environment,
            variable = variable
        )
        .to_string(),
        CrashReports::OffByAdmin => t!(session_crash_reports_refused_admin).to_string(),
        // `On` and `OffByPerson` are changeable, so no refusal is said for them.
        CrashReports::Incognito | CrashReports::On | CrashReports::OffByPerson => {
            t!(session_crash_reports_refused_incognito).to_string()
        }
    }
}

/// Whether this session itself sends reports. A build with no address to send to (CRASH-3), or a
/// session started while reports were off, sends nothing whatever the switch now says.
fn sending(reporting: bool) -> String {
    if reporting {
        t!(session_crash_reports_sending).to_string()
    } else {
        t!(session_crash_reports_not_sending).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// The answer, with every call to `save` recorded and `save` answering `result`.
    fn said(
        word: &str,
        now: CrashReports,
        result: Result<(), NotSaved>,
        after: CrashReports,
    ) -> (Vec<String>, Vec<bool>) {
        let saved = RefCell::new(Vec::new());
        let lines = answer(
            word,
            now,
            |enabled| {
                saved.borrow_mut().push(enabled);
                result
            },
            || after,
        );
        (lines, saved.into_inner())
    }

    #[test]
    fn the_bare_command_says_on_when_nothing_turned_reports_off() {
        let (lines, saved) = said("", CrashReports::On, Ok(()), CrashReports::On);
        assert_eq!(lines, vec![t!(session_crash_reports_on).to_string()]);
        assert!(saved.is_empty());
    }

    #[test]
    fn each_holding_layer_is_named_and_a_change_is_refused_without_writing() {
        let variable = "BRAVEBOT_CRASH_REPORTS";
        for (now, shown, refused) in [
            (
                CrashReports::Incognito,
                t!(session_crash_reports_incognito).to_string(),
                t!(session_crash_reports_refused_incognito).to_string(),
            ),
            (
                CrashReports::OffByAdmin,
                t!(session_crash_reports_off_by_admin).to_string(),
                t!(session_crash_reports_refused_admin).to_string(),
            ),
            (
                CrashReports::OffByEnvironment(variable),
                t!(
                    session_crash_reports_off_by_environment,
                    variable = variable
                )
                .to_string(),
                t!(
                    session_crash_reports_refused_environment,
                    variable = variable
                )
                .to_string(),
            ),
        ] {
            let (lines, saved) = said("", now, Ok(()), now);
            assert_eq!(lines, vec![shown.clone()], "{now:?}");
            assert!(saved.is_empty(), "{now:?}");
            for word in [ON, OFF] {
                let (lines, saved) = said(word, now, Ok(()), CrashReports::On);
                assert_eq!(lines, vec![refused.clone()], "{now:?} {word}");
                assert!(saved.is_empty(), "{now:?} {word} wrote {saved:?}");
            }
        }
        let (lines, _) = said(
            ON,
            CrashReports::OffByEnvironment(variable),
            Ok(()),
            CrashReports::On,
        );
        assert!(lines[0].contains(variable), "{lines:?}");
    }

    #[test]
    fn the_persons_own_off_is_shown_and_can_be_changed() {
        let (lines, saved) = said("", CrashReports::OffByPerson, Ok(()), CrashReports::On);
        assert_eq!(
            lines,
            vec![t!(session_crash_reports_off_by_person).to_string()]
        );
        assert!(saved.is_empty());

        let (lines, saved) = said(ON, CrashReports::OffByPerson, Ok(()), CrashReports::On);
        assert_eq!(saved, vec![true]);
        assert_eq!(lines, vec![t!(session_crash_reports_saved_on).to_string()]);

        let (lines, saved) = said(OFF, CrashReports::On, Ok(()), CrashReports::OffByPerson);
        assert_eq!(saved, vec![false]);
        assert_eq!(lines, vec![t!(session_crash_reports_saved_off).to_string()]);
    }

    #[test]
    fn a_file_that_is_not_a_document_is_reported_unsaved() {
        let (lines, saved) = said(
            OFF,
            CrashReports::On,
            Err(NotSaved::Unwritable),
            CrashReports::On,
        );
        assert_eq!(saved, vec![false]);
        assert_eq!(lines, vec![t!(session_crash_reports_unsaved).to_string()]);

        let (lines, saved) = said(
            ON,
            CrashReports::OffByPerson,
            Err(NotSaved::NoDirectory),
            CrashReports::OffByPerson,
        );
        assert_eq!(saved, vec![true]);
        assert_eq!(
            lines,
            vec![t!(session_crash_reports_no_directory).to_string()]
        );

        // A write that reported success but left the file saying otherwise is not called saved.
        let (lines, _) = said(OFF, CrashReports::On, Ok(()), CrashReports::On);
        assert_eq!(lines, vec![t!(session_crash_reports_unsaved).to_string()]);
    }

    #[test]
    fn a_word_that_is_not_on_or_off_changes_nothing() {
        for word in ["OFF ", "false", "0", "On"] {
            let (lines, saved) = said(word, CrashReports::On, Ok(()), CrashReports::OffByPerson);
            assert!(saved.is_empty(), "{word:?} wrote {saved:?}");
            assert_eq!(
                lines,
                vec![t!(session_crash_reports_usage, word = word).to_string()],
                "{word:?}"
            );
        }
    }

    /// The command refuses exactly where the shared rule says the person's choice does not decide,
    /// across the CRASH-11 combinations, so it cannot diverge from the desktop switch.
    #[test]
    fn the_command_refuses_exactly_where_the_shared_rule_says_the_person_does_not_decide() {
        use bravebot_config::crash_reports::{Inputs, resolve};
        for incognito in [false, true] {
            for managed_off in [false, true] {
                for variable in [None, Some("0"), Some("1")] {
                    for setting in [None, Some(false), Some(true)] {
                        let now = resolve(Inputs {
                            incognito,
                            managed_off,
                            variable,
                            setting,
                        });
                        let (_, saved) = said(OFF, now, Ok(()), CrashReports::OffByPerson);
                        assert_eq!(
                            !saved.is_empty(),
                            now.person_may_change(),
                            "{now:?} from {incognito} {managed_off} {variable:?} {setting:?}"
                        );
                    }
                }
            }
        }
    }
}
