//! The answers a session keeps past the prompt that gave them.
//!
//! One value, so that every transition a front end makes takes it apart with no `..`: a set added
//! here does not build until each transition says whether it is kept, reset, rebased or read again.
//! The terminal's event loop holds one for `/cd`, `/clear`, `/branch`, `/undo` and `/rewind`. The
//! desktop bridge holds one for opening a stored session, forking one, undoing a turn and the end of
//! each turn, and a session in lines for opening and the end of each turn.
//!
//! The mode, vetting and the MCP servers belong to the front end's session, and the directories
//! `/add-dir` opened belong to the workspace, so no pattern here reaches them.

use crate::lsp::LanguageServers;
use crate::turn::Decisions;
use bravebot_core::credentials::Exposed;
use bravebot_core::programs::{AskedAbout, TrustedPrograms};
use bravebot_core::trust::TrustStore;

/// The answers a session keeps. `Rules` is what the front end reads for the root it is in, and `()`
/// for one that keeps none.
pub struct Answers<Rules = ()> {
    pub trust: TrustStore,
    pub programs: TrustedPrograms,
    /// The language servers this session has started, LSP-8. Held here rather than inside a turn
    /// because the process is the session's: one built per turn would be shut down at the end of
    /// the turn that started it, so the next message would ask the same person about the same
    /// language and wait for a second index of the same tree. `None` until the first turn builds
    /// one, and again after `/clear`, which ends the session the approval belonged to, and after
    /// `/cd`, which leaves the tree it was approved for.
    pub servers: Option<LanguageServers>,
    /// Every run prompt this session has drawn, so a second prompt for one binary under other
    /// arguments can say that a settings file is what ends the asking. It grants nothing and is not
    /// written down anywhere: a session that ends forgets what it asked, which is the same lifetime
    /// the list of programs it vouched for has.
    pub asked_about: AskedAbout,
    /// Every file this session has agreed the planner may be given despite the credential scan.
    /// The same lifetime, except that `/cd` drops it: an answer is kept under the file's name from
    /// the working directory, which after a move names another file.
    pub exposed: Exposed,
    pub rules: Rules,
}

impl<Rules> Answers<Rules> {
    /// What a session holds on opening, before any turn has asked anything.
    pub fn opening(trust: TrustStore, programs: TrustedPrograms, rules: Rules) -> Self {
        Self {
            trust,
            programs,
            servers: None,
            asked_about: AskedAbout::new(),
            exposed: Exposed::new(),
            rules,
        }
    }

    /// Take on what a turn ended with. The map, the programs, the prompts drawn and the files agreed
    /// to are the turn's to return. The servers were lent separately and are put back by whoever lent
    /// them, and the rules are read for the root, which a turn does not move.
    pub fn adopt(&mut self, decisions: Decisions) {
        let Answers {
            trust,
            programs,
            asked_about,
            exposed,
            // Kept: lent to the turn by reference and put back by the caller.
            servers: _,
            // Kept: read for the root.
            rules: _,
        } = self;
        let Decisions {
            trust: decided_trust,
            programs: decided_programs,
            asked_about: decided_asked_about,
            exposed: decided_exposed,
        } = decisions;
        *trust = decided_trust;
        *programs = decided_programs;
        *asked_about = decided_asked_about;
        *exposed = decided_exposed;
    }

    /// What a copy of the session made from its record carries on with: the map, the programs
    /// vouched for and the rules. `/branch` and a fork of a desktop session are such copies.
    pub fn what_a_record_keeps(&self) -> (&TrustStore, &TrustedPrograms, &Rules) {
        let Answers {
            trust,
            programs,
            rules,
            // Not in the record: a server approved, a run prompt drawn and a file agreed to hold a
            // secret were answers given to the session the copy was made from, and `--resume` of
            // the copy would start without them.
            servers: _,
            asked_about: _,
            exposed: _,
        } = self;
        (trust, programs, rules)
    }

    /// The copy `/branch` carries on in holds what the record keeps and nothing else.
    pub fn forget_what_no_record_keeps(&mut self) {
        let Answers {
            trust: _,
            programs: _,
            rules: _,
            servers,
            asked_about,
            exposed,
        } = self;
        *servers = None;
        *asked_about = AskedAbout::new();
        *exposed = Exposed::new();
    }

    /// Forget what the session agreed to, for one beginning again in this process (`/clear`).
    pub fn forget_what_was_agreed(&mut self) {
        let Answers {
            // Replaced by the `/clear` arm before it calls this: the map the new session was asked
            // for, TRUST-6, and the rules read again for the root it opens in, PERM-15.
            trust: _,
            rules: _,
            programs,
            servers,
            asked_about,
            exposed,
        } = self;
        // A new session vouches for no program, on the same reasoning as the map: the list is a
        // standing permission, and this begins a session that was never asked.
        *programs = TrustedPrograms::new();
        // The servers go with it, LSP-8: what was approved was a process for the session, so
        // dropping the set shuts them down and the session beginning here is asked again before
        // one starts.
        *servers = None;
        // And nothing has been asked about, since the questions this list holds were put in a
        // session that is over.
        *asked_about = AskedAbout::new();
        // And nothing agreed to be shown, and no file agreed to hold a secret, for the same reason.
        *exposed = Exposed::new();
    }

    /// The answers [`crate::rewind::restore`] reaches, for an undo and a rewind (SESSION-19). It
    /// meets the map with the point's, returns the programs to the point's and stops the servers.
    /// The others are kept.
    pub fn what_a_rewind_takes_back(
        &mut self,
    ) -> (
        &mut TrustStore,
        &mut TrustedPrograms,
        &mut Option<LanguageServers>,
    ) {
        let Answers {
            trust,
            programs,
            servers,
            // Kept: it grants nothing.
            asked_about: _,
            // Kept: CRED-13 and CRED-15 give a file agreed to be shown or to hold a secret the
            // session's lifetime, and no rewind point holds one.
            exposed: _,
            // Kept: read for the root, which a rewind does not move.
            rules: _,
        } = self;
        (trust, programs, servers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::programs::Command;

    fn command(program: &str) -> Command {
        Command::new(program, Vec::new(), "/work")
    }

    /// A session that has been asked and answered everything it keeps.
    fn holding_something() -> Answers<&'static str> {
        let mut trust = TrustStore::new("/work");
        trust.trust(".");
        let mut programs = TrustedPrograms::new();
        programs.trust(command("/bin/ls"));
        let mut answers = Answers::opening(trust, programs, "read for /work");
        answers.servers = Some(LanguageServers::new("/work", None));
        answers.asked_about.record(command("/bin/ls"));
        answers.exposed.allow(".env");
        answers
            .exposed
            .allow_creating(std::path::Path::new("/work/key.pem"));
        answers
    }

    /// What a person would be asked again about, one flag per answer, in the order the table lists
    /// them. `true` is an answer still held, so the same effect goes unasked.
    fn still_held(answers: &Answers<&'static str>) -> [bool; 7] {
        let Answers {
            trust,
            programs,
            servers,
            asked_about,
            exposed,
            rules,
        } = answers;
        [
            trust.is_trusted("a.txt"),
            programs.contains(
                std::path::Path::new("/bin/ls"),
                std::path::Path::new("/bin/ls"),
                &[],
                std::path::Path::new("/work"),
            ),
            servers.is_some(),
            *asked_about != AskedAbout::new(),
            exposed.holds(".env"),
            exposed.allows_creating(std::path::Path::new("/work/key.pem")),
            *rules == "read for /work",
        ]
    }

    const ANSWERS: [&str; 7] = [
        "the map",
        "programs vouched for",
        "language servers",
        "run prompts drawn",
        "files agreed to be shown",
        "writes agreed to create a credential",
        "the rules read for the root",
    ];

    /// Every transition against every answer: what is still held afterwards, with nothing handed
    /// back by whoever else takes part. A turn that learned nothing returns an empty set of each, a
    /// rewind meets the map with a point that trusted nothing, and `/clear` has its own arm replace
    /// the map and the rules before it calls in here.
    ///
    /// The answer is used again by asking whether it is still held, because an answer still held is
    /// one the same effect goes unasked under, and a transition that forgets it is how the person
    /// comes to be asked.
    #[test]
    fn every_transition_decides_every_answer_and_a_forgotten_one_is_asked_again() {
        const HELD: bool = true;
        const ASKED: bool = false;
        type Transition = (&'static str, fn(&mut Answers<&'static str>), [bool; 7]);
        let transitions: [Transition; 5] = [
            (
                "/branch, which carries on in a copy",
                |answers| answers.forget_what_no_record_keeps(),
                [HELD, HELD, ASKED, ASKED, ASKED, ASKED, HELD],
            ),
            (
                "/clear, which begins a session again",
                |answers| answers.forget_what_was_agreed(),
                [HELD, ASKED, ASKED, ASKED, ASKED, ASKED, HELD],
            ),
            (
                "/undo and /rewind",
                |answers| {
                    let (trust, programs, servers) = answers.what_a_rewind_takes_back();
                    *trust = TrustStore::new("/work");
                    *programs = TrustedPrograms::new();
                    *servers = None;
                },
                [ASKED, ASKED, ASKED, HELD, HELD, HELD, HELD],
            ),
            (
                "the end of a turn that learned nothing",
                |answers| {
                    answers.adopt(Decisions {
                        trust: TrustStore::new("/work"),
                        programs: TrustedPrograms::new(),
                        asked_about: AskedAbout::new(),
                        exposed: Exposed::new(),
                    })
                },
                [ASKED, ASKED, HELD, ASKED, ASKED, ASKED, HELD],
            ),
            (
                "a fork, which opens from what the record keeps",
                |answers| {
                    let (trust, programs, rules) = answers.what_a_record_keeps();
                    *answers = Answers::opening(trust.clone(), programs.clone(), *rules);
                },
                [HELD, HELD, ASKED, ASKED, ASKED, ASKED, HELD],
            ),
        ];
        let before = still_held(&holding_something());
        assert_eq!(before, [HELD; 7], "the fixture must hold every answer");

        for (name, apply, expected) in transitions {
            let mut answers = holding_something();
            apply(&mut answers);
            let after = still_held(&answers);
            for (index, answer) in ANSWERS.iter().enumerate() {
                assert_eq!(
                    after[index],
                    expected[index],
                    "{name}: {answer} is {} but should be {}",
                    if after[index] {
                        "still held"
                    } else {
                        "asked again"
                    },
                    if expected[index] {
                        "still held"
                    } else {
                        "asked again"
                    },
                );
            }
        }
    }
}
