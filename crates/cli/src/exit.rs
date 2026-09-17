//! How a run ends, in a form a program can act on.
//!
//! A command line tool has two channels a caller can read without parsing prose: the status the
//! process exits with, and an identifier in the text it printed. Everything a run can fail at is
//! one of the endings below, and each carries both.

use std::fmt::Display;
use std::io::Write;
use std::process::ExitCode;

/// Why a run ended, and what it exits with.
///
/// The set is small and closed on purpose. A caller writes a `case` over it, so an ending that
/// meant two things would be one it could not act on, and one ending per failure site would be a
/// list nothing could keep up with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// The run did what it was asked.
    Done,
    /// It failed for a reason none of the others name.
    Failed,
    /// An argument was refused, so nothing ran.
    Argument,
    /// The configuration cannot be used, so nothing ran.
    Configuration,
    /// A gate refused an effect while the turn was running.
    Refused,
    /// The request never reached the backend.
    Unreachable,
}

impl Ending {
    /// The status the process exits with.
    ///
    /// Allocated in the order the endings were added, and never renumbered: a script testing for
    /// a status is reading a number this program promised it.
    pub fn status(self) -> u8 {
        match self {
            Self::Done => 0,
            Self::Failed => 1,
            Self::Argument => 2,
            Self::Configuration => 3,
            Self::Refused => 4,
            Self::Unreachable => 5,
        }
    }

    /// The name a program reads this ending by.
    ///
    /// What goes in a result object, where a number alone would have every consumer keeping its
    /// own copy of this table.
    pub fn name(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Argument => "argument",
            Self::Configuration => "configuration",
            Self::Refused => "refused",
            Self::Unreachable => "unreachable",
        }
    }

    /// The identifier printed in front of the message, where the ending is a failure.
    ///
    /// The status in a form that survives being read off somebody else's screen. Every message
    /// this program prints is in the reader's own language, so a bug report carries a sentence
    /// nobody can search for and a status nobody wrote down; the identifier is neither.
    ///
    /// Derived from the status rather than allocated separately, because two numbering schemes
    /// for one set of endings is one of them going out of date.
    pub fn identifier(self) -> Option<String> {
        match self {
            Self::Done => None,
            failure => Some(format!("BB{}", 1000 + u16::from(failure.status()))),
        }
    }

    /// Whether the run did what it was asked.
    pub fn ok(self) -> bool {
        self == Self::Done
    }

    /// What the process returns.
    pub fn code(self) -> ExitCode {
        ExitCode::from(self.status())
    }

    /// The failure as a person reads it: the identifier, then the message.
    ///
    /// The identifier leads rather than follows, so it is still there when the message is long
    /// enough to be wrapped or cut off, which a backend's own account of a failure often is.
    ///
    /// Apart from [`Ending::say`] because not every failure ends the process: a turn that could not
    /// run inside a session is said and the session goes on, and it has to say the same identifier
    /// as the one that exits.
    pub fn told(self, message: impl Display) -> String {
        match self.identifier() {
            Some(identifier) => format!("{identifier}: {message}"),
            None => message.to_string(),
        }
    }

    /// Write the failure out, identifier first, and return what to exit with.
    pub fn say(self, to: &mut impl Write, message: impl Display) -> ExitCode {
        let _ = writeln!(to, "{}", self.told(message));
        self.code()
    }
}

/// Write a failure to stderr and return what to exit with.
///
/// Every failure this program ends on goes through here, which is what makes "each of them says an
/// identifier" a property of one function rather than of thirty call sites.
pub fn fail(ending: Ending, message: impl Display) -> ExitCode {
    ending.say(&mut std::io::stderr().lock(), message)
}

/// Which ending a turn that could not run has.
///
/// One failure is worth telling apart from the rest, and it is the one a caller can act on without
/// a person: a backend that was not there a moment ago may be there on the next attempt.
///
/// A manifest run carries the failure that stopped it rather than a sentence about one, so a step
/// that lost the backend is classified here exactly as a turn's own round is.
pub fn ending_of(error: &bravebot_agent::TurnError) -> Ending {
    match error {
        bravebot_agent::TurnError::Chat(backend) if backend.is_unreachable() => Ending::Unreachable,
        bravebot_agent::TurnError::Manifest { cause, .. } => ending_of(cause),
        _ => Ending::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole promise of the set: a caller branching on the status gets a different answer for
    /// each kind of failure. Two endings sharing a status would collapse back into the single
    /// "something went wrong" the exit code used to be.
    #[test]
    fn every_ending_has_a_status_of_its_own() {
        let endings = [
            Ending::Done,
            Ending::Failed,
            Ending::Argument,
            Ending::Configuration,
            Ending::Refused,
            Ending::Unreachable,
        ];

        let mut seen: Vec<u8> = endings.iter().map(|e| e.status()).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), endings.len(), "two endings share a status");

        let mut names: Vec<&str> = endings.iter().map(|e| e.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), endings.len(), "two endings share a name");
    }

    /// The identifier is what a bug report carries when the message in it is in a language the
    /// person reading the report does not have. A success has nothing to identify.
    #[test]
    fn a_failure_is_identified_and_a_success_is_not() {
        assert_eq!(Ending::Done.identifier(), None);
        assert_eq!(
            Ending::Configuration.identifier().as_deref(),
            Some("BB1003")
        );
        assert_eq!(Ending::Unreachable.identifier().as_deref(), Some("BB1005"));
    }

    /// Said in front of the message rather than instead of it: the sentence is what tells the
    /// person what to do, and the identifier is what makes the failure searchable.
    #[test]
    fn a_failure_says_its_identifier_in_front_of_the_message() {
        let mut written = Vec::new();
        Ending::Configuration.say(&mut written, "l'adresse n'a pas de schema");
        let said = String::from_utf8(written).expect("utf-8");

        assert_eq!(said, "BB1003: l'adresse n'a pas de schema\n");
    }

    /// A manifest run used to reach the command line as a sentence about what stopped it, so
    /// every one of them had the same status whatever had gone wrong. It now carries the failure
    /// itself, and is classified by it.
    ///
    /// Which backend failures are a backend that was not there is
    /// [`bravebot_agent::backend::BackendError::is_unreachable`], and is pinned beside it.
    #[test]
    fn a_manifest_run_is_classified_by_what_stopped_it() {
        use bravebot_agent::TurnError;
        use bravebot_agent::backend::BackendError;
        use bravebot_agent::manifest::Attempt;

        let nothing_ran = || {
            Box::new(Attempt {
                shape: None,
                proposed: None,
                plan: None,
                steps: Vec::new(),
            })
        };

        let unusable_plan = TurnError::Manifest {
            attempt: nothing_ran(),
            cause: Box::new(TurnError::Precommit(
                "step 2 had nothing to write".to_string(),
            )),
        };
        assert_eq!(ending_of(&unusable_plan), Ending::Failed);

        let no_credential = TurnError::Manifest {
            attempt: nothing_ran(),
            cause: Box::new(TurnError::Chat(BackendError::NoGatewayToken {
                provider: "openai".to_string(),
            })),
        };
        assert_eq!(ending_of(&no_credential), Ending::Failed);
    }
}
