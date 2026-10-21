//! Scanning the command line for the flags `main` takes out before it dispatches.
//!
//! Everything here reads the person's own arguments, never content.

/// `--incognito`, which engages the mode before anything that could write is reached.
pub(crate) const INCOGNITO: &str = "--incognito";
/// `--safe`, which leaves out what a session loads from the directory it opens in.
pub(crate) const SAFE: &str = "--safe";
/// `--vet`, which turns auto-vetting on for this run.
pub(crate) const VET: &str = "--vet";
/// `--dangerously-skip-permissions`, which answers every question the permission modes ask.
pub(crate) const SKIP_PERMISSIONS: &str = "--dangerously-skip-permissions";
/// `--dangerously-bypass-approvals-and-sandbox`, which is `--dangerously-skip-permissions` with
/// `--sandbox off`.
pub(crate) const BYPASS_APPROVALS_AND_SANDBOX: &str = "--dangerously-bypass-approvals-and-sandbox";
/// `--accept-edits`, which answers the write prompt of a one-shot run and no other (CLI-1).
pub(crate) const ACCEPT_EDITS: &str = "--accept-edits";

/// The flags that begin a one-shot run when one leads the line, so that `main` dispatches on this
/// list and [`is_a_one_shot`] reads the same one.
pub(crate) const TASK_FLAGS: [&str; 16] = [
    "-p",
    "--print",
    "--mode",
    "--model",
    "--advisor",
    "--processor-model",
    "--effort",
    "--file",
    "--session-ref",
    "--add-dir",
    "--trust-workspace",
    "--trace",
    "--json",
    "--json-stream",
    "--output-schema",
    "--goal",
];

/// The commands that start neither a session nor a task, so a flag that shapes one is refused
/// beside them rather than taken and ignored, for the reason CLI-13 gives about a settings file.
const NON_SESSION_COMMANDS: [&str; 13] = [
    "doctor",
    "update",
    "bug-report",
    "auth",
    "mcp",
    "permissions",
    "sessions",
    "attach",
    "reply",
    "import-leo-creds",
    "import-providers",
    "completion",
    "shell-init",
];

/// The command in [`NON_SESSION_COMMANDS`] that `first` names, or `None` where it names another.
pub(crate) fn starts_neither_a_session_nor_a_task(first: &str) -> Option<&'static str> {
    NON_SESSION_COMMANDS
        .iter()
        .copied()
        .find(|command| *command == first)
}

/// Whether the arguments left after the flags are taken start a one-shot run, which is what
/// [`ACCEPT_EDITS`] is for: a session has a mode key to say the same thing with, and the commands
/// that start no run have no write to answer.
///
/// `continues_with_a_task` is whether a leading `--resume` or `--continue` carries a task.
pub(crate) fn is_a_one_shot(args: &[String], continues_with_a_task: bool) -> bool {
    match args.first().map(String::as_str) {
        None => false,
        Some(first) if TASK_FLAGS.contains(&first) => true,
        Some("--resume" | "-r" | "--continue" | "-c") => continues_with_a_task,
        Some(first) if first.starts_with('-') => false,
        Some(first) => {
            starts_neither_a_session_nor_a_task(first).is_none()
                && !matches!(first, "__bg-host" | "__bg-supervisor")
        }
    }
}

/// Take the flag `name` out of the arguments, reporting whether it was there.
///
/// Repeats are one flag rather than an error: a flag that is already on cannot be asked for again,
/// and refusing the second would be a rule about typing.
pub(crate) fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    let asked = args.len();
    args.retain(|arg| arg != name);
    args.len() != asked
}

/// Take each of `flags` out of the arguments with the word after it, answering with what `each`
/// made of every one, in the order they were typed.
///
/// `each` is given the flag and the next word, which is `None` where the flag ends the line, and
/// either answers or refuses. A refusal is returned at once and leaves `args` as it was typed: the
/// arguments are rewritten only after the whole scan has succeeded. The word after a listed flag is
/// its value whatever it looks like and is not scanned again, so a flag that is another flag's
/// value is not taken. A caller that lets the last repeat win takes the last answer.
pub(crate) fn take_valued<T>(
    args: &mut Vec<String>,
    flags: &[&str],
    mut each: impl FnMut(&str, Option<&str>) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let mut taken = Vec::new();
    let mut kept = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        if !flags.contains(&flag) {
            kept.push(args[index].clone());
            index += 1;
            continue;
        }
        let word = args.get(index + 1).map(String::as_str);
        taken.push(each(flag, word)?);
        index += 1 + usize::from(word.is_some());
    }
    *args = kept;
    Ok(taken)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(typed: &[&str]) -> Vec<String> {
        typed.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn a_refused_value_leaves_the_arguments_as_they_were_typed() {
        let mut args = words(&["-p", "--a", "one", "task", "--a", "", "--b", "two"]);
        let typed = args.clone();
        let refused = take_valued(&mut args, &["--a", "--b"], |_, word| match word {
            Some("") => Err("blank".to_string()),
            Some(word) => Ok(word.to_string()),
            None => Err("none".to_string()),
        });
        assert_eq!(refused, Err("blank".to_string()));
        assert_eq!(args, typed);
    }

    #[test]
    fn every_listed_flag_is_taken_with_its_value_in_the_order_typed() {
        let mut args = words(&["x", "--a", "1", "y", "--b", "2", "--a", "3", "z"]);
        let taken = take_valued(&mut args, &["--a", "--b"], |flag, word| {
            Ok((flag.to_string(), word.map(str::to_string)))
        })
        .unwrap();
        let pair = |flag: &str, value: &str| (flag.to_string(), Some(value.to_string()));
        assert_eq!(
            taken,
            [pair("--a", "1"), pair("--b", "2"), pair("--a", "3")]
        );
        assert_eq!(args, words(&["x", "y", "z"]));
    }

    #[test]
    fn a_listed_flag_given_as_anothers_value_is_a_value_and_not_taken() {
        let mut args = words(&["--a", "--b", "--b", "two"]);
        let taken = take_valued(&mut args, &["--a", "--b"], |flag, word| {
            Ok(format!("{flag}={}", word.unwrap_or("")))
        })
        .unwrap();
        assert_eq!(taken, ["--a=--b", "--b=two"]);
        assert!(args.is_empty());
    }

    #[test]
    fn a_flag_ending_the_line_reaches_the_closure_with_no_word() {
        let mut args = words(&["go", "--a"]);
        let taken = take_valued(&mut args, &["--a"], |_, word| Ok(word.is_none())).unwrap();
        assert_eq!(taken, [true]);
        assert_eq!(args, words(&["go"]));
    }

    #[test]
    fn the_flag_is_taken_by_its_whole_spelling_and_repeats_are_one_flag() {
        let mut args = words(&["--safe-mode", "--safe", "--safe", "x"]);
        assert!(take_flag(&mut args, SAFE));
        assert_eq!(args, words(&["--safe-mode", "x"]));
        assert!(!take_flag(&mut args, SAFE));
    }

    #[test]
    fn exactly_the_thirteen_commands_that_start_no_session_are_named() {
        for command in NON_SESSION_COMMANDS {
            assert_eq!(starts_neither_a_session_nor_a_task(command), Some(command));
        }
        assert_eq!(NON_SESSION_COMMANDS.len(), 13);
        for other in ["", "-p", "--resume", "resume", "Doctor", "doctor "] {
            assert_eq!(
                starts_neither_a_session_nor_a_task(other),
                None,
                "{other:?}"
            );
        }
    }
}
