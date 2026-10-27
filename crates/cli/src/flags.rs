//! Scanning the command line for the flags `main` takes out before it dispatches.
//!
//! Everything here reads the person's own arguments, never content.

use bravebot_agent::turn::SystemPrompts;
use bravebot_config::Managed;
use bravebot_i18n::t;
use bravebot_sandbox::SandboxMode;
use std::path::{Path, PathBuf};

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
pub(crate) const TASK_FLAGS: [&str; 20] = [
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
    "--input-format",
    "--trust-input-lines",
    "--output-schema",
    "--goal",
    "--max-rounds",
    "--max-tokens",
];

/// The commands that start neither a session nor a task, so a flag that shapes one is refused
/// beside them rather than taken and ignored, for the reason CLI-13 gives about a settings file.
const NON_SESSION_COMMANDS: [&str; 14] = [
    "doctor",
    "update",
    "bug-report",
    "auth",
    "mcp",
    "permissions",
    "sessions",
    "stats",
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

/// Whether the words after a flag that takes text are that text, rather than nothing or the next
/// flag.
///
/// Blank is nothing: a script whose variable expanded to nothing asked for words. Opening with `-`
/// and holding no whitespace is the next flag, and taken as the text it would be removed from the
/// arguments. A sentence opening with `-` holds a space and is text.
pub(crate) fn is_a_value(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && !(text.starts_with('-') && !text.contains(char::is_whitespace))
}

/// Why `-i` cannot go where the line put it, or `None` where it can.
///
/// It opens a new session, so it cannot go with another way of starting one. After `--fork` or
/// `--from-pr` the session would open and the prompt be dropped, after `--resume` or `--continue`
/// the same or a one-shot run, and after `--bg` the words would start a session nobody watches on a
/// prompt opening `-i`. Only those decide: a command's own words, as in `permissions check Bash grep
/// -i`, may hold one.
pub(crate) fn interactive_out_of_place(args: &[String]) -> Option<String> {
    let first = args.first()?;
    let another_start = matches!(
        first.as_str(),
        "--resume" | "-r" | "--continue" | "-c" | "--fork" | "-f" | "--from-pr" | "--bg"
    );
    let after_it = args[1..]
        .iter()
        .any(|arg| arg == "-i" || arg == "--interactive");
    if !(another_start && after_it) {
        return None;
    }
    Some(t!(
        cli_interactive_not_with_another_start,
        flag = first.as_str()
    ))
}

/// Take `--agent <name>` out of the arguments, answering with the definition it named.
///
/// Removed before dispatch, like `--settings`. If it is given twice the last name is used, as with
/// `--settings`, because every turn can work under only one definition. A blank name is refused, as
/// `--model` refuses one, because a script whose variable expanded to nothing asked for a
/// definition and would otherwise run under the planner. A name opening with `-` is refused too. It
/// is the next flag, taken as a name, and no definition's name may open with one.
///
/// Only the name is read here. The way of starting that follows matches it, because the set depends
/// on the directory's trust, which has not been asked about yet.
pub(crate) fn take_agent(args: &mut Vec<String>) -> Result<Option<String>, String> {
    let named = take_valued(args, &["--agent"], |_, word| match word.map(str::trim) {
        Some(name) if !name.is_empty() && !name.starts_with('-') => Ok(name.to_string()),
        _ => Err(t!(cli_agent_needs_a_name).to_string()),
    })?;
    Ok(named.into_iter().last())
}

/// Take `--issue <url>` and `--pr <url>` out of the arguments, answering with the links they gave.
///
/// Removed before dispatch, like `--agent`, and the last of two of a kind is used. A value that is
/// not one `http` or `https` address with a host in printable ASCII is refused with a sentence that
/// does not repeat it, as `/issue` and `/pr` refuse one (PANEL-12): it may hold the escape that
/// made it not one. A missing value is refused too, so a script whose variable expanded to nothing
/// is not run with no link, untold.
pub(crate) fn take_links(args: &mut Vec<String>) -> Result<bravebot_tui::app::Links, String> {
    use bravebot_session::sessions::{Link, Url};
    let taken = take_valued(args, &["--issue", "--pr"], |flag, word| {
        let kind = match flag {
            "--issue" => Link::Issue,
            _ => Link::PullRequest,
        };
        match word.and_then(Url::read) {
            Some(url) => Ok((kind, url)),
            None => Err(match kind {
                Link::Issue => t!(cli_issue_needs_a_link).to_string(),
                Link::PullRequest => t!(cli_pr_needs_a_link).to_string(),
            }),
        }
    })?;
    let mut links = bravebot_tui::app::Links::default();
    for (kind, url) in taken {
        match kind {
            Link::Issue => links.issue = Some(url),
            Link::PullRequest => links.pull_request = Some(url),
        }
    }
    Ok(links)
}

/// Whether the arguments left after the flags are taken open an interactive session, which is the
/// only thing a link from `--issue` or `--pr` can be given to: a one-shot run, a session in lines
/// and a background one would drop it, and so do the commands that start no session.
pub(crate) fn opens_a_session(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        None | Some("-i" | "--interactive" | "--fork" | "-f" | "--from-pr") => true,
        Some("--resume" | "-r" | "--continue" | "-c") => !continues_with_a_task(args),
        Some(_) => false,
    }
}

/// Whether a leading `--resume` or `--continue` carries a task, which makes it a one-shot run
/// carrying on an earlier session (CLI-25) rather than a request to open one.
///
/// A task is `-p` or `--print`, or a word that is not a flag. A flag alone, which opened the
/// session before and still does, is not one. `--resume` followed by a flag names no session, and
/// the task run refuses it by name rather than looking for a session called `-p`.
pub(crate) fn continues_with_a_task(args: &[String]) -> bool {
    let carries = |rest: &[String]| {
        rest.iter()
            .any(|arg| matches!(arg.as_str(), "-p" | "--print") || !arg.starts_with('-'))
    };
    match args.first().map(String::as_str) {
        Some("--resume" | "-r") => match args.get(1) {
            Some(id) => id.starts_with('-') || carries(&args[2..]),
            None => false,
        },
        Some("--continue" | "-c") => carries(&args[1..]),
        _ => false,
    }
}

/// Why `--agent` cannot go with the command line's first argument, or `None` where it can.
///
/// The commands other than a session or a task start neither, so a name they would not use is
/// refused instead of ignored, for the reason CLI-13 gives about a settings file.
pub(crate) fn without_a_definition(first: Option<&str>) -> Option<String> {
    starts_neither_a_session_nor_a_task(first?)
        .map(|command| t!(cli_agent_not_for_a_command, command = command))
}

/// Take `--system-prompt <prompt>` and `--append-system-prompt <prompt>` out of the arguments.
///
/// Removed before dispatch, like `--agent`, and the last of two is used (CLI-19). A blank value is
/// refused, as `--model` refuses one, because a script whose variable expanded to nothing asked for
/// words and would otherwise run without them. A value that opens with `-` and holds no whitespace
/// is refused too: it is the next flag, and taken as the text it would be removed from the
/// arguments and the run would answer in another format. A sentence opening with `-` holds a space
/// and is text.
pub(crate) fn take_system_prompts(args: &mut Vec<String>) -> Result<SystemPrompts, String> {
    let given = take_valued(
        args,
        &["--system-prompt", "--append-system-prompt"],
        |flag, word| match word.map(str::trim) {
            Some(text) if is_a_value(text) => Ok((flag == "--system-prompt", text.to_string())),
            _ => Err(t!(cli_system_prompt_needs_text, flag = flag)),
        },
    )?;
    let mut prompts = SystemPrompts::default();
    for (replacing, text) in given {
        if replacing {
            prompts.replacing = Some(text);
        } else {
            prompts.appending = Some(text);
        }
    }
    Ok(prompts)
}

/// The flag a run was given, where it was given either, for the message that refuses it.
pub(crate) fn flag_named(prompts: &SystemPrompts) -> Option<&'static str> {
    match (&prompts.replacing, &prompts.appending) {
        (Some(_), _) => Some("--system-prompt"),
        (None, Some(_)) => Some("--append-system-prompt"),
        (None, None) => None,
    }
}

/// Why the words a flag gave cannot go with the command line's first argument, or `None` where
/// they can.
///
/// A resumed session takes them: the record stores no system prompt (INSTR-5), so the words apply to
/// the turns this process sends. The commands that start neither a session nor a task are refused
/// rather than ignored, for the reason CLI-13 gives about a settings file.
pub(crate) fn without_a_prompt_to_give(flag: &str, first: Option<&str>) -> Option<String> {
    starts_neither_a_session_nor_a_task(first?).map(|command| {
        t!(
            cli_system_prompt_not_for_a_command,
            flag = flag,
            command = command
        )
    })
}

/// The file that made the bypass mode unreachable, where a layer in force asked for it (PERM-17).
///
/// The strictest thing any layer said, the managed file among them: both keys only refuse, so there
/// is nothing for the strongest layer to lift and no ordering to resolve (PERM-18). The file is what
/// comes back rather than a boolean, because a flag refused without naming where the refusal was
/// written is a person reading the help for a flag that is documented and does not work.
pub(crate) fn bypass_made_unreachable(
    settings: &bravebot_config::Settings,
    managed: &Managed,
) -> Option<PathBuf> {
    if !settings
        .narrowing()
        .strictest(managed.narrowing())
        .makes_bypass_unreachable()
    {
        return None;
    }
    // The settings layers first, weakest of them first, and the managed file where none of them
    // said it: what is wanted is the file somebody can open, and a person told about their own
    // checkout's line has somewhere to go that a person told about `/etc` does not.
    settings
        .narrowed_by(bravebot_config::Narrowing::BYPASS_UNREACHABLE)
        .map(Path::to_path_buf)
        .or_else(|| managed.path().map(Path::to_path_buf))
}

/// Take `--settings <path>` out of the arguments, answering with the file it named.
///
/// Removed before dispatch for the reason `--incognito` and `--dangerously-skip-permissions` are:
/// the settings a run reads are a property of the run rather than of a task, so a session, a
/// resumed session and a one-shot all mean the same thing by it, and a flag read per subcommand
/// would be a flag silently ignored by whichever of them forgot it.
///
/// Given twice the last one is the file, which is how `--mode` and `--model` already resolve a
/// repeat. Refusing instead would be a rule about typing, and there is nothing to add: a second
/// file cannot be a fifth layer, since the flag names the layer above every other and two of those
/// is not an order anybody could read off the command line.
///
/// The arguments are rewritten only once the whole scan has succeeded, so a refusal leaves the list
/// that was typed rather than one this had half consumed.
pub(crate) fn take_settings(args: &mut Vec<String>) -> Result<Option<PathBuf>, String> {
    // A blank path is refused rather than read as no flag, on `--model`'s argument: a script
    // whose variable expanded to nothing asked for a settings file and would otherwise be run
    // under whatever the directory happened to carry, without being told.
    let named = take_valued(args, &["--settings"], |_, word| match word.map(str::trim) {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Err(t!(cli_settings_needs_a_path).to_string()),
    })?;
    Ok(named.into_iter().last())
}

/// Take `--run-network <open|closed>` out of the arguments, answering with the setting it named.
///
/// Removed before dispatch for the reason `--settings` is. A word that is neither is refused with
/// the two that are named, since a typo read as `open` would leave the network a person meant to
/// close open and say nothing. Given twice the last one wins, as `--settings` does. The arguments
/// are rewritten only once the whole scan has succeeded.
pub(crate) fn take_run_network(
    args: &mut Vec<String>,
) -> Result<Option<bravebot_sandbox::network::Network>, String> {
    let named = take_valued(args, &["--run-network"], |_, word| {
        let Some(word) = word else {
            return Err(t!(cli_run_network_needs_a_word).to_string());
        };
        bravebot_sandbox::network::Network::parse(word)
            .ok_or_else(|| t!(cli_run_network_unknown, word = word))
    })?;
    Ok(named.into_iter().last())
}

/// Take `--log-level <error|info|debug>` out of the arguments, answering with the level it named.
///
/// Removed before dispatch for the reason `--run-network` is, and refused on a missing or unknown
/// word for the same one: a typo read as the default would leave a person without the log they
/// asked for, discovered only when the failure they wanted it for had already happened.
pub(crate) fn take_log_level(
    args: &mut Vec<String>,
) -> Result<Option<bravebot_diag::Level>, String> {
    let named = take_valued(args, &["--log-level"], |_, word| {
        let Some(word) = word else {
            return Err(t!(cli_log_level_needs_a_word).to_string());
        };
        bravebot_diag::Level::parse(word).ok_or_else(|| t!(cli_log_level_unknown, word = word))
    })?;
    Ok(named.into_iter().last())
}

/// Take `--sandbox <mode>` out of the arguments, answering with the mode it named.
///
/// Removed before dispatch for the reason `--settings` is: how far a program may reach is a
/// property of the run, and a flag read per subcommand would be one silently ignored by whichever
/// forgot it. Given twice the last one wins, as `--settings` does. A blank or unknown word is
/// refused rather than read as the default, because the modes differ in what a program may reach
/// and a script whose variable expanded to nothing asked for one of them.
pub(crate) fn take_sandbox(args: &mut Vec<String>) -> Result<Option<SandboxMode>, String> {
    let named = take_valued(args, &["--sandbox"], |_, word| {
        word.and_then(SandboxMode::parse).ok_or_else(|| {
            t!(
                cli_sandbox_needs_a_mode,
                names = SandboxMode::ALL.map(SandboxMode::name).join(", ")
            )
        })
    })?;
    Ok(named.into_iter().last())
}

/// Whether the command line's first argument names a command that starts no program `run` would
/// confine, so that a refused mode does not stop it.
pub(crate) fn starts_no_program(first: Option<&str>) -> bool {
    matches!(
        first,
        Some(
            "doctor"
                | "bug-report"
                | "update"
                | "--help"
                | "-h"
                | "--version"
                | "-V"
                | "auth"
                | "permissions"
                | "sessions"
                | "stats"
                | "attach"
                | "completion"
                | "import-leo-creds"
                | "import-providers"
        )
    )
}

/// What a refused mode says: the file that asked, when it was a file, and the file that holds the
/// floor.
pub(crate) fn sandbox_refusal(refused: &bravebot_config::sandbox::Refused) -> String {
    use bravebot_config::sandbox::Floor;
    let pinned_in = refused.pinned_in.display().to_string();
    match (&refused.asked_in, refused.because) {
        (Some(file), Floor::Mode) => t!(
            cli_sandbox_refused_file,
            asked = refused.asked.name(),
            asked_in = file.display().to_string(),
            pinned = refused.pinned.name(),
            pinned_in = pinned_in,
            looser = bravebot_agent::sandbox_reason::looser(refused)
        ),
        (None, Floor::Mode) => t!(
            cli_sandbox_refused_flag,
            asked = refused.asked.name(),
            pinned = refused.pinned.name(),
            pinned_in = pinned_in,
            looser = bravebot_agent::sandbox_reason::looser(refused)
        ),
        (Some(file), Floor::Network) => t!(
            cli_sandbox_refused_network_file,
            asked = refused.asked.name(),
            asked_in = file.display().to_string(),
            pinned_in = pinned_in
        ),
        (None, Floor::Network) => t!(
            cli_sandbox_refused_network_flag,
            asked = refused.asked.name(),
            pinned_in = pinned_in
        ),
    }
}

/// Take `--sandbox-allow-read`, `--sandbox-deny-read`, `--sandbox-allow-write` and
/// `--sandbox-deny-write` out of the arguments, each with the path that follows it, answering with
/// the lists they came to. Each may be given any number of times.
///
/// Removed before dispatch for the reason `--settings` is. A flag with nothing after it is refused,
/// since a path read as the next flag would be an entry nobody wrote. The arguments are rewritten
/// only once the whole scan has succeeded.
pub(crate) fn take_sandbox_filesystem(
    args: &mut Vec<String>,
) -> Result<bravebot_sandbox::rules::Lists, String> {
    use bravebot_sandbox::rules::{Entry, List};
    const FLAGS: [(&str, List); 4] = [
        ("--sandbox-allow-read", List::AllowRead),
        ("--sandbox-deny-read", List::DenyRead),
        ("--sandbox-allow-write", List::AllowWrite),
        ("--sandbox-deny-write", List::DenyWrite),
    ];
    let names = FLAGS.map(|(flag, _)| flag);
    let taken = take_valued(args, &names, |flag, word| {
        let Some(path) = word.filter(|path| !path.trim().is_empty()) else {
            return Err(t!(cli_sandbox_flag_needs_a_path, flag = flag));
        };
        let list = FLAGS
            .iter()
            .find(|(name, _)| *name == flag)
            .map(|(_, list)| *list);
        let entry = Entry {
            path: path.to_string(),
            by: None,
            pinned: false,
        };
        Ok((list, entry))
    })?;
    let mut lists = bravebot_sandbox::rules::Lists::default();
    for (list, entry) in taken {
        match list {
            Some(List::AllowRead) => lists.allow_read.push(entry),
            Some(List::DenyRead) => lists.deny_read.push(entry),
            Some(List::AllowWrite) => lists.allow_write.push(entry),
            Some(List::DenyWrite) => lists.deny_write.push(entry),
            None => {}
        }
    }
    Ok(lists)
}

/// Every tool name `--tools` may name: the ones a turn is offered, wherever a surface offers them,
/// and the `advisor`, `message_session` and `propose_plan` a session may add.
pub(crate) fn nameable_tools() -> Vec<String> {
    use bravebot_agent::tools::{Running, Scheduling, available};
    let mut names: Vec<String> = [Scheduling::ArrangingALook, Scheduling::PacingALoop]
        .into_iter()
        .flat_map(|scheduling| {
            available(
                scheduling,
                bravebot_agent::watch::Arming::Allowed { free: 1 },
                bravebot_agent::exec::Deadlines::BUILT_IN,
                Running::Offered,
            )
        })
        .map(|tool| tool.function.name)
        .collect();
    names.push("advisor".to_string());
    names.push("message_session".to_string());
    names.push("propose_plan".to_string());
    names.sort();
    names.dedup();
    names
}

/// Take `--tools <a,b,c>` and `--no-shell` out of the arguments, answering with what they limit.
///
/// Removed before dispatch for the reason `--incognito` is: every way of starting builds the same
/// list of tools. A name that is no tool is refused with the ones that are, since a typo read as
/// "offer nothing of that" would leave a run holding a smaller set than its author thought and say
/// nothing, and a list that names nothing is refused for the reason a blank `--model` is. Given
/// twice, `--tools` takes the last list. The arguments are rewritten only once the scan has
/// succeeded.
pub(crate) fn take_tool_limit(
    args: &mut Vec<String>,
) -> Result<bravebot_core::tool_set::Limit, String> {
    let lists = take_valued(args, &["--tools"], |_, word| {
        let Some(list) = word.filter(|list| !list.trim().is_empty()) else {
            return Err(t!(cli_tools_needs_a_list).to_string());
        };
        let known = nameable_tools();
        let mut named = Vec::new();
        for name in list.split(',').map(str::trim) {
            if !known.iter().any(|tool| tool == name) {
                return Err(t!(cli_tools_unknown, name = name, known = known.join(", ")));
            }
            named.push(name.to_string());
        }
        Ok(named)
    })?;
    // After the list, so that `--tools --no-shell` still names `--no-shell` as the unknown tool it
    // was taken for, and not as a list that is missing.
    let no_shell = take_flag(args, "--no-shell");
    Ok(bravebot_core::tool_set::Limit {
        no_shell,
        only: lists.into_iter().last(),
    })
}

/// Why the tool limit cannot go with the command line's first argument, or `None` where it can.
///
/// The commands that start no session and no task offer no tools, so the flags would be taken and do
/// nothing. Refused rather than ignored, for the reason CLI-13 gives about a settings file.
pub(crate) fn without_a_tool_set_to_limit(first: &str) -> Option<String> {
    // CLI-26 lists the commands without `bug-report`, where CLI-19 names it, so it is left out here.
    starts_neither_a_session_nor_a_task(first)
        .filter(|command| *command != "bug-report")
        .map(|command| t!(cli_tools_not_for_a_command, command = command))
}

/// Whether a result object was asked for, read straight off the command line.
///
/// Only for the case where the parse failed, which is the one moment the parsed invocation cannot
/// answer. `--input-format lines` is a `--json-stream` run (CLI-36), so it asks for one too.
pub(crate) fn wants_json(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg == "--json" || arg == "--json-stream")
        || args
            .windows(2)
            .any(|pair| pair[0] == "--input-format" && pair[1] == "lines")
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
    fn exactly_the_fourteen_commands_that_start_no_session_are_named() {
        for command in NON_SESSION_COMMANDS {
            assert_eq!(starts_neither_a_session_nor_a_task(command), Some(command));
        }
        assert_eq!(NON_SESSION_COMMANDS.len(), 14);
        for other in ["", "-p", "--resume", "resume", "Doctor", "doctor "] {
            assert_eq!(
                starts_neither_a_session_nor_a_task(other),
                None,
                "{other:?}"
            );
        }
    }
}
