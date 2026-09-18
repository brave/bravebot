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
//! The path itself and not a rendering of it, for the same reason: a path is bytes, and
//! `to_string_lossy` maps every byte it cannot read to one replacement character, so two binaries
//! whose names differ only in such bytes would share one entry while a run spawns whichever of them
//! was named. See [`crate::command::Spelling`].
//!
//! # And the tree the vouch was given in
//!
//! A prompt shows three things: the resolved binary, the argv, and the directory the line runs in.
//! An entry holds all three, because an entry that held two of them would record less than the
//! question asked. `sh check.sh` in `sub/` and `sh check.sh` at the root name different files, so
//! an answer about one of them is not an answer about the other, and `git clean -fd` is a
//! different proposition in two different trees.
//!
//! One directory and not its descendants. A subtree key would put the same hole one level down: an
//! entry given in `sub/` would cover a `check.sh` that appears later in `sub/nested/`, by the same
//! relative-argument trick it exists to stop.
//!
//! The absolute path, canonical, as the only spelling. `sub`, `./sub` and a symlink pointing at
//! `sub` are one tree, and three keys for it would be three chances to miss a match; resolution
//! happens outside this crate, which performs no I/O, and `bravebot_agent::workspace` is where it
//! is done.
//!
//! # The session, not the directory
//!
//! Kept in the session record and restored on resume, on the same reasoning as the trust map: the
//! person resuming is the person who gave it. A fresh session in the same directory starts empty
//! and asks, because a list kept per directory would grant this on behalf of a user who was never
//! asked. The tree in an entry is what the entry covers, not where the list lives.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// One command a user vouched for: a resolved program, the exact arguments it runs with, and the
/// tree it was vouched for in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Command {
    /// The absolute path the program name resolved to.
    pub program: PathBuf,
    /// The arguments, in order. Empty is a real value and different from any non-empty list.
    pub args: Vec<String>,
    /// The directory the vouch was given in, absolute and canonical.
    ///
    /// The third thing the prompt showed. An entry grants in this tree and in no other, not in a
    /// directory below it and not in the one above.
    pub directory: PathBuf,
}

impl Command {
    pub fn new(
        program: impl Into<PathBuf>,
        args: Vec<String>,
        directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            program: program.into(),
            args,
            directory: directory.into(),
        }
    }

    /// The command as a person should read it, with argument boundaries visible.
    ///
    /// The program and its arguments, without the tree. Whoever draws this has the tree beside it
    /// and renders it the way that screen renders a path: `/status` and a run prompt both show one
    /// relative to the workspace where they can, and this crate cannot, since it does not know
    /// where the workspace is.
    ///
    /// Lossy, as every rendering of a path is. This is the one thing here that is read rather than
    /// matched on, and a path with no text spelling still has to reach the screen.
    pub fn display(&self) -> String {
        crate::command::Stage::new(
            self.program.to_string_lossy().to_string(),
            self.args.clone(),
        )
        .display()
    }

    /// Whether this is the same program under the same arguments, wherever either was run.
    ///
    /// The question [`AskedAbout`] asks, and deliberately not the one a grant asks. RUN-20 is
    /// about a line whose *arguments* differ from one run to the next, and a line run in a second
    /// tree is the same argument list rather than a changed one: reading the tree here would have
    /// `cargo test` at the root and `cargo test` in `sub/` advising somebody to write a pattern
    /// for arguments that never moved.
    fn is_the_same_call(&self, other: &Self) -> bool {
        self.program == other.program && self.args == other.args
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

    /// Whether this exact program and argument list was vouched for, in this exact tree.
    ///
    /// All three, and `directory` by equality rather than by prefix: an entry given in `sub/`
    /// covers `sub/` and neither the root above it nor a `nested/` below it. A prefix test would
    /// leave the hole it closes one level down, since a relative argument names a different file
    /// in every tree it is read in.
    pub fn contains(&self, program: &Path, args: &[String], directory: &Path) -> bool {
        self.commands
            .iter()
            .any(|c| c.program == program && c.args == args && c.directory == directory)
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
/// Kept beside [`TrustedPrograms`] because it holds the same value and the two are read at the
/// same moment, and read on the resolved path and the exact arguments alone: an entry here carries
/// the tree its prompt was drawn in, as every [`Command`] does, and nothing here consults it, for
/// the reason [`Command::is_the_same_call`] gives. It is deliberately not part of
/// [`crate::policy::Vouched`]: nothing here travels into a delegate or out of one, and nothing
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
            line.iter().any(|command| seen.program == command.program)
                && !line.iter().any(|command| seen.is_the_same_call(command))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The workspace root these tests vouch in, so a test about a directory has a second tree to
    /// name and the ordinary ones read as they did.
    const ROOT: &str = "/work";

    fn root() -> &'static Path {
        Path::new(ROOT)
    }

    fn git_log() -> Command {
        Command::new("/usr/bin/git", vec!["log".into()], ROOT)
    }

    /// Every session starts asking about everything, and trusting no output. Membership is
    /// granted, never assumed from silence, which is the rule the trust map lives by.
    #[test]
    fn nothing_is_vouched_for_to_begin_with() {
        let programs = TrustedPrograms::new();
        assert!(programs.is_empty());
        assert!(!programs.contains(Path::new("/usr/bin/git"), &["log".to_string()], root()));
    }

    #[test]
    fn a_command_that_was_vouched_for_is_recognised() {
        let mut programs = TrustedPrograms::new();
        programs.trust(git_log());
        assert!(programs.contains(Path::new("/usr/bin/git"), &["log".to_string()], root()));
    }

    /// The reason entries are not keyed by program alone. Vouching for `git log` says nothing
    /// about `git push`: they do different things and print different things, and one entry
    /// covering both would grant far more than the person read.
    #[test]
    fn vouching_for_one_command_says_nothing_about_another_of_the_same_program() {
        let mut programs = TrustedPrograms::new();
        programs.trust(git_log());
        assert!(
            !programs.contains(Path::new("/usr/bin/git"), &["push".to_string()], root()),
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
            ROOT,
        ));
        for other in [
            vec!["log".to_string()],
            vec!["log".to_string(), "-n".to_string()],
            vec!["log".to_string(), "-n".to_string(), "50".to_string()],
            vec!["-n".to_string(), "5".to_string(), "log".to_string()],
        ] {
            assert!(
                !programs.contains(Path::new("/usr/bin/git"), &other, root()),
                "{other:?} matched an entry it is not"
            );
        }
    }

    /// A command with no arguments is a real entry, distinct from any command with some.
    #[test]
    fn no_arguments_is_its_own_entry() {
        let mut programs = TrustedPrograms::new();
        programs.trust(Command::new("/bin/pwd", Vec::new(), ROOT));
        assert!(programs.contains(Path::new("/bin/pwd"), &[], root()));
        assert!(!programs.contains(Path::new("/bin/pwd"), &["-L".to_string()], root()));
    }

    /// Matched on the resolved path, so an assertion does not follow a name onto a different
    /// binary when `$PATH` or an alias changes what the name means.
    #[test]
    fn the_same_name_at_a_different_path_is_a_different_program() {
        let mut programs = TrustedPrograms::new();
        programs.trust(Command::new("/usr/bin/grep", vec!["x".into()], ROOT));
        assert!(
            !programs.contains(
                Path::new("/opt/homebrew/bin/grep"),
                &["x".to_string()],
                root()
            ),
            "an assertion followed a name onto a different binary"
        );
    }

    /// And on the path's own bytes, not on a rendering of them. `to_string_lossy` maps every byte
    /// that is not valid UTF-8 onto one replacement character, so two binaries whose paths differ
    /// only there shared one entry while a run spawns whichever of them the plan named.
    #[cfg(unix)]
    #[test]
    fn two_binaries_differing_only_in_unrenderable_bytes_are_different_programs() {
        let at = |last: u8| {
            use std::os::unix::ffi::OsStrExt;
            let mut bytes = b"/work/prog-".to_vec();
            bytes.push(last);
            PathBuf::from(std::ffi::OsStr::from_bytes(&bytes))
        };
        let mut programs = TrustedPrograms::new();
        programs.trust(Command::new(at(0xff), vec!["x".into()], ROOT));
        assert!(
            programs.contains(&at(0xff), &["x".to_string()], root()),
            "the binary that was vouched for was not recognised"
        );
        assert!(
            !programs.contains(&at(0xfe), &["x".to_string()], root()),
            "an assertion about one binary covered another that renders the same way"
        );
    }

    /// The tree is a path too, and it is the whole question in a line whose arguments are relative,
    /// so two trees that render the same way must not share an entry either.
    #[cfg(unix)]
    #[test]
    fn two_trees_differing_only_in_unrenderable_bytes_are_different_trees() {
        let tree = |last: u8| {
            use std::os::unix::ffi::OsStrExt;
            let mut bytes = b"/work/sub-".to_vec();
            bytes.push(last);
            PathBuf::from(std::ffi::OsStr::from_bytes(&bytes))
        };
        let mut programs = TrustedPrograms::new();
        programs.trust(Command::new("/bin/sh", vec!["check.sh".into()], tree(0xff)));
        assert!(
            programs.contains(Path::new("/bin/sh"), &["check.sh".to_string()], &tree(0xff)),
            "the tree the vouch was given in was not recognised"
        );
        assert!(
            !programs.contains(Path::new("/bin/sh"), &["check.sh".to_string()], &tree(0xfe)),
            "a vouch given in one tree covered another that renders the same way"
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
        assert!(!programs.contains(Path::new("/usr/bin/git"), &["log".to_string()], root()));
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
            ROOT,
        );
        assert_eq!(command.display(), "/usr/bin/git commit -m 'two words'");
    }

    /// The reason the tree is part of the key. `sh check.sh` names a different file in every tree
    /// it is read in, so an answer given about the one in `sub/` is not an answer about the one at
    /// the root, and an entry that covered both would run a file nobody was ever shown.
    #[test]
    fn vouching_in_one_tree_says_nothing_about_the_same_command_in_another() {
        let mut programs = TrustedPrograms::new();
        programs.trust(Command::new(
            "/bin/sh",
            vec!["check.sh".into()],
            "/work/sub",
        ));
        assert!(programs.contains(
            Path::new("/bin/sh"),
            &["check.sh".to_string()],
            Path::new("/work/sub")
        ));
        assert!(
            !programs.contains(Path::new("/bin/sh"), &["check.sh".to_string()], root()),
            "an answer given in a subdirectory covered a different file at the root"
        );
    }

    /// One directory and not its descendants. A prefix test would leave the same hole one level
    /// down: a file that appears later in `sub/nested/` would be covered by an answer somebody gave
    /// about `sub/`, which is the trick the tree is in the key to stop.
    #[test]
    fn an_entry_does_not_cover_a_directory_below_the_one_it_names() {
        let mut programs = TrustedPrograms::new();
        programs.trust(Command::new(
            "/bin/sh",
            vec!["check.sh".into()],
            "/work/sub",
        ));
        assert!(
            !programs.contains(
                Path::new("/bin/sh"),
                &["check.sh".to_string()],
                Path::new("/work/sub/nested")
            ),
            "an entry about one tree covered a tree below it"
        );
    }

    /// Two trees are two entries, not one entry read twice. A person asked in each place answered
    /// twice, and [`TrustedPrograms`] has to be able to say so, since `/status` is what reads the
    /// grants back (RUN-9).
    #[test]
    fn the_same_command_vouched_for_in_two_trees_is_two_entries() {
        let mut programs = TrustedPrograms::new();
        programs.trust(Command::new("/usr/bin/make", vec!["check".into()], ROOT));
        programs.trust(Command::new(
            "/usr/bin/make",
            vec!["check".into()],
            "/work/sub",
        ));
        assert_eq!(programs.len(), 2);
    }

    /// Written down and read back the same way, so a resumed session vouches for what the record
    /// says and nothing more.
    #[test]
    fn a_set_survives_being_written_down_and_read_back() {
        let programs = TrustedPrograms::from_iter([
            git_log(),
            Command::new("/bin/ls", vec!["-la".into()], ROOT),
        ]);
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
        commit_in(message, "/work")
    }

    fn commit_in(message: &str, tree: &str) -> Command {
        Command::new(
            "/usr/bin/git",
            vec!["commit".into(), "-m".into(), message.into()],
            tree,
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
            Command::new("/usr/bin/grep", vec!["TODO".into(), "src".into()], "/work"),
            Command::new("/usr/bin/grep", vec!["-v".into(), "test".into()], "/work"),
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
        asked.record(Command::new("/usr/bin/grep", vec!["x".into()], "/work"));
        assert!(
            !asked.arguments_have_varied(&[Command::new(
                "/opt/homebrew/bin/grep",
                vec!["y".into()],
                "/work"
            )]),
            "two binaries sharing a name were read as one program"
        );
    }

    /// RUN-20 is about arguments that differ from one run to the next, and a tree is not an
    /// argument. The same line put to somebody twice in two directories has one argument list, so
    /// advice to write a pattern for it would name an argument that never moved, and a key at the
    /// prompt does finish the asking for each tree, which is exactly what the advice denies.
    #[test]
    fn the_same_line_asked_about_in_two_trees_has_no_arguments_that_varied() {
        let mut asked = AskedAbout::new();
        asked.record(commit_in("first", "/work"));
        assert!(!asked.arguments_have_varied(&[commit_in("first", "/work/sub")]));
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
        assert!(!programs.contains(
            Path::new("/usr/bin/git"),
            &["commit".to_string()],
            Path::new("/work")
        ));
    }
}
