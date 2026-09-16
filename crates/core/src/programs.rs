//! The commands a session's user has vouched for.
//!
//! An entry is a **program and its exact arguments**, and vouching for one is a statement by the
//! user about two things at once:
//!
//! 1. It may run without being asked again.
//! 2. What it prints is **trusted**.
//!
//! Both halves are the user's assertion, not an inference. Nothing here establishes that a command
//! is side-effect-free or that its output is free of influence, and nothing tries: `git log`
//! prints commit messages that whoever contributed to the repository wrote. The user saying "I
//! trust this command and its output" is what makes the output trusted, exactly as
//! [`crate::trust::TrustStore`] makes a directory's contents trusted because the user said so and
//! not because anything inspected them.
//!
//! That is the whole justification, so the prompt has to ask for it in those terms. A person
//! agreeing to this is agreeing that the command's side effects and its output are both theirs to
//! answer for. See `docs/specs/running-programs.md`.
//!
//! # Distinct from `crate::pure`
//!
//! [`crate::pure`] answers a different question and answers it by audit: whether a program,
//! given a particular argv, can read anything the label does not account for. Its table is
//! hand-checked against each program's full option surface, and nothing a user says extends it.
//! This module is the human-assertion route to the same label, and the two must not be confused:
//! one is a proof about a program, the other is a person taking responsibility for one.
//!
//! # Keyed by program and arguments, both exact
//!
//! Not by program alone. Vouching for `git log` says nothing about `git push`, and it must not:
//! the two do different things and produce different output, and an entry that covered both would
//! be granting far more than the person read.
//!
//! The program is the **resolved path**. `$PATH` and shell aliases decide what a name means, so
//! recording the string would let a later change inherit an assertion made about a different
//! binary. Resolution happens outside this crate, which performs no I/O; see `bravebot_agent::programs`.
//!
//! # The session, not the directory
//!
//! Kept in the session record and restored on resume, on the same reasoning as the trust map: the
//! person resuming is the person who gave it. A fresh session in the same directory starts empty
//! and asks, because a list kept per directory would grant this on behalf of a user who was never
//! asked.

use std::collections::BTreeSet;

/// One command a user vouched for: a resolved program and the exact arguments it runs with.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Command {
    /// The absolute path the program name resolved to.
    pub program: String,
    /// The arguments, in order. Empty is a real value and different from any non-empty list.
    pub args: Vec<String>,
}

impl Command {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }

    /// The command as a person should read it, with argument boundaries visible.
    pub fn display(&self) -> String {
        crate::command::Stage::new(self.program.clone(), self.args.clone()).display()
    }
}

/// The set of commands a session has vouched for.
///
/// Empty means every run asks and every run's output is untrusted, which is the state every
/// session starts in. Membership is granted, never assumed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustedPrograms {
    /// Ordered so a record written twice is written the same way.
    commands: BTreeSet<Command>,
}

impl TrustedPrograms {
    /// An empty set: nothing is vouched for.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that the user vouched for this exact command, side effects and output alike.
    pub fn trust(&mut self, command: Command) {
        self.commands.insert(command);
    }

    /// Forget one, so it is asked about again and its output is untrusted again.
    pub fn forget(&mut self, command: &Command) -> bool {
        self.commands.remove(command)
    }

    /// Whether this exact program and argument list was vouched for.
    pub fn contains(&self, program: &str, args: &[String]) -> bool {
        self.commands
            .iter()
            .any(|c| c.program == program && c.args == args)
    }

    /// Every command vouched for, in a stable order.
    pub fn iter(&self) -> impl Iterator<Item = &Command> {
        self.commands.iter()
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl FromIterator<Command> for TrustedPrograms {
    fn from_iter<I: IntoIterator<Item = Command>>(commands: I) -> Self {
        Self {
            commands: commands.into_iter().collect(),
        }
    }
}

/// The commands this session has already put to a person at a run prompt.
///
/// Not a grant, and no gate consults it. Membership stops no prompt, raises no label and reaches
/// no file: everything in this list was asked about and is asked about again. It exists so that a
/// prompt can say something the line in front of it does not, which is that this is a program
/// whose arguments differ from one run to the next and so a program no key at a prompt will ever
/// finish asking about.
///
/// Kept beside [`TrustedPrograms`] because it is keyed the same way, on the resolved path and the
/// exact arguments, and because the two are read at the same moment. It is deliberately not part
/// of [`crate::policy::Vouched`]: nothing here travels into a delegate or out of one, and nothing
/// here is written into the session record, since an advisory sentence is not something a person
/// carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AskedAbout {
    /// In the order they were asked about, with repeats collapsed.
    ///
    /// A list rather than a set because there is nothing to look a command up by: the question
    /// asked of it is about the entries that do **not** match, so every one is read anyway.
    asked: Vec<Command>,
}

impl AskedAbout {
    /// Nothing has been asked about yet, which is where every session starts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that this exact command was put to a person.
    pub fn record(&mut self, command: Command) {
        if !self.asked.contains(&command) {
            self.asked.push(command);
        }
    }

    /// Whether some binary `line` names was asked about under an argument list `line` does not
    /// hold.
    ///
    /// The whole of what a prompt can establish about a line whose arguments vary. It is an
    /// observation and not an inference: the person was asked twice about one program and read two
    /// different argument lists, which is the variation itself rather than a guess at which
    /// position carries it. Nothing here says which argument differed, because deciding that is
    /// the judgment [RUN-20] refuses to make.
    ///
    /// The whole line at once rather than a step at a time, and the entries the line itself holds
    /// are not what it differs from. `grep TODO src | grep -v test` names one binary under two
    /// argument lists, so a step-by-step reading would have that line, asked about a second time,
    /// varying from itself.
    ///
    /// [RUN-20]: ../../../docs/specs/tools/run.md
    pub fn arguments_have_varied(&self, line: &[Command]) -> bool {
        self.asked.iter().any(|seen| {
            line.iter().any(|command| seen.program == command.program) && !line.contains(seen)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_log() -> Command {
        Command::new("/usr/bin/git", vec!["log".into()])
    }

    /// Every session starts asking about everything, and trusting no output. Membership is
    /// granted, never assumed from silence, which is the rule the trust map lives by.
    #[test]
    fn nothing_is_vouched_for_to_begin_with() {
        let programs = TrustedPrograms::new();
        assert!(programs.is_empty());
        assert!(!programs.contains("/usr/bin/git", &["log".to_string()]));
    }

    #[test]
    fn a_command_that_was_vouched_for_is_recognised() {
        let mut programs = TrustedPrograms::new();
        programs.trust(git_log());
        assert!(programs.contains("/usr/bin/git", &["log".to_string()]));
    }

    /// The reason entries are not keyed by program alone. Vouching for `git log` says nothing
    /// about `git push`: they do different things and print different things, and one entry
    /// covering both would grant far more than the person read.
    #[test]
    fn vouching_for_one_command_says_nothing_about_another_of_the_same_program() {
        let mut programs = TrustedPrograms::new();
        programs.trust(git_log());
        assert!(
            !programs.contains("/usr/bin/git", &["push".to_string()]),
            "an assertion about one command covered a different one"
        );
    }

    /// Arguments are matched exactly and in order, since a different argument list is a different
    /// command with different output.
    #[test]
    fn the_arguments_must_match_exactly() {
        let mut programs = TrustedPrograms::new();
        programs.trust(Command::new(
            "/usr/bin/git",
            vec!["log".into(), "-n".into(), "5".into()],
        ));
        for other in [
            vec!["log".to_string()],
            vec!["log".to_string(), "-n".to_string()],
            vec!["log".to_string(), "-n".to_string(), "50".to_string()],
            vec!["-n".to_string(), "5".to_string(), "log".to_string()],
        ] {
            assert!(
                !programs.contains("/usr/bin/git", &other),
                "{other:?} matched an entry it is not"
            );
        }
    }

    /// A command with no arguments is a real entry, distinct from any command with some.
    #[test]
    fn no_arguments_is_its_own_entry() {
        let mut programs = TrustedPrograms::new();
        programs.trust(Command::new("/bin/pwd", Vec::new()));
        assert!(programs.contains("/bin/pwd", &[]));
        assert!(!programs.contains("/bin/pwd", &["-L".to_string()]));
    }

    /// Matched on the resolved path, so an assertion does not follow a name onto a different
    /// binary when `$PATH` or an alias changes what the name means.
    #[test]
    fn the_same_name_at_a_different_path_is_a_different_program() {
        let mut programs = TrustedPrograms::new();
        programs.trust(Command::new("/usr/bin/grep", vec!["x".into()]));
        assert!(
            !programs.contains("/opt/homebrew/bin/grep", &["x".to_string()]),
            "an assertion followed a name onto a different binary"
        );
    }

    #[test]
    fn vouching_twice_records_one_command() {
        let mut programs = TrustedPrograms::new();
        programs.trust(git_log());
        programs.trust(git_log());
        assert_eq!(programs.len(), 1);
    }

    #[test]
    fn a_command_can_be_forgotten() {
        let mut programs = TrustedPrograms::new();
        programs.trust(git_log());
        assert!(programs.forget(&git_log()));
        assert!(!programs.contains("/usr/bin/git", &["log".to_string()]));
        assert!(
            !programs.forget(&git_log()),
            "forgetting twice found nothing"
        );
    }

    /// The prompt and the status report both show entries, so the rendering has to keep argument
    /// boundaries visible the way the approval prompt does.
    #[test]
    fn a_command_reads_back_with_its_argument_boundaries() {
        let command = Command::new(
            "/usr/bin/git",
            vec!["commit".into(), "-m".into(), "two words".into()],
        );
        assert_eq!(command.display(), "/usr/bin/git commit -m 'two words'");
    }

    /// Written down and read back the same way, so a resumed session vouches for what the record
    /// says and nothing more.
    #[test]
    fn a_set_survives_being_written_down_and_read_back() {
        let programs =
            TrustedPrograms::from_iter([git_log(), Command::new("/bin/ls", vec!["-la".into()])]);
        let written: Vec<Command> = programs.iter().cloned().collect();
        assert_eq!(TrustedPrograms::from_iter(written), programs);
        assert_eq!(
            programs.iter().next().map(Command::display),
            Some("/bin/ls -la".to_string()),
            "order is stable"
        );
    }
}

/// What a prompt may say about a line whose arguments will differ next time.
///
/// RUN-20 refuses to let any key at a prompt cover a second argument list, and says the prompt
/// gives advice instead where the line in front of the person is one of those. These are the whole
/// of what decides which lines those are.
#[cfg(test)]
mod varying {
    use super::*;

    fn commit(message: &str) -> Command {
        Command::new(
            "/usr/bin/git",
            vec!["commit".into(), "-m".into(), message.into()],
        )
    }

    /// The case the advice exists for. A commit message is different every time, so the person is
    /// asked about `git commit` again in this session and in every later one, and no key they can
    /// press at the prompt will ever stop it. Two argument lists for one binary is the whole of
    /// what establishes that, and it is an observation rather than a guess at which argument
    /// carries the message.
    #[test]
    fn a_second_argument_list_for_one_binary_is_a_line_whose_arguments_vary() {
        let mut asked = AskedAbout::new();
        asked.record(commit("first"));
        assert!(asked.arguments_have_varied(&[commit("second")]));
    }

    /// The line that repeats is the one RUN-19's key answers in full, so advice about a settings
    /// file would be telling somebody to write a pattern where pressing a key would do.
    #[test]
    fn a_line_asked_about_twice_is_not_a_line_whose_arguments_vary() {
        let mut asked = AskedAbout::new();
        asked.record(commit("first"));
        asked.record(commit("first"));
        assert!(!asked.arguments_have_varied(&[commit("first")]));
    }

    /// A line naming one binary under two argument lists is still one line, and a second prompt for
    /// it is the repeat RUN-19's key answers. Reading the steps one at a time would have it varying
    /// from itself, and would advise a pattern for every pipeline that feeds a program into itself.
    #[test]
    fn a_line_naming_one_binary_twice_does_not_vary_from_itself() {
        let line = [
            Command::new("/usr/bin/grep", vec!["TODO".into(), "src".into()]),
            Command::new("/usr/bin/grep", vec!["-v".into(), "test".into()]),
        ];
        let mut asked = AskedAbout::new();
        for command in &line {
            asked.record(command.clone());
        }
        assert!(!asked.arguments_have_varied(&line));
    }

    /// Nothing has varied until something has been asked about twice, so the first prompt of a
    /// session says nothing about patterns. Advice on every prompt would be noise that hides the
    /// case it is for.
    #[test]
    fn a_line_nothing_has_been_asked_about_has_no_arguments_that_have_varied() {
        assert!(!AskedAbout::new().arguments_have_varied(&[commit("first")]));
    }

    /// Keyed on the resolved path for the reason a vouch is: `$PATH` and aliases decide what a
    /// name means, and two binaries asked about under one name are two programs rather than one
    /// whose arguments moved.
    #[test]
    fn a_second_binary_is_not_the_first_ones_arguments_varying() {
        let mut asked = AskedAbout::new();
        asked.record(Command::new("/usr/bin/grep", vec!["x".into()]));
        assert!(
            !asked
                .arguments_have_varied(&[Command::new("/opt/homebrew/bin/grep", vec!["y".into()])]),
            "two binaries sharing a name were read as one program"
        );
    }

    /// A list is a list of lines put to somebody, not a grant, so nothing in it stops a prompt or
    /// vouches for anything. A reader who mistook the two would have a session growing an
    /// allowlist out of the questions it asked.
    #[test]
    fn asking_about_a_line_vouches_for_nothing() {
        let mut asked = AskedAbout::new();
        asked.record(commit("first"));
        let programs = TrustedPrograms::new();
        assert!(programs.is_empty());
        assert!(!programs.contains("/usr/bin/git", &["commit".to_string()]));
    }
}
