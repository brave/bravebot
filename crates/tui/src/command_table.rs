//! The slash commands: the word each is typed as, what it takes, what Enter on it does while a
//! turn runs, and completing a half-typed one.

use bravebot_i18n::t;

use crate::state::{Bound, Session};

/// The line that opens the model picker instead of starting a turn.
pub(crate) const MODEL_COMMAND: &str = "/model";

/// The line that opens the theme picker, or applies a theme named after the word.
pub(crate) const THEME_COMMAND: &str = "/theme";

/// The line that opens the effort picker, or takes a level named after the word.
pub(crate) const EFFORT_COMMAND: &str = "/effort";

/// The line that names the model the planner may consult, says which it may, or drops the choice.
pub(crate) const ADVISOR_COMMAND: &str = "/advisor";

/// The word after `/advisor` that drops the session's own choice.
pub(crate) const ADVISOR_OFF: &str = "off";

/// The line that lists the output styles, picks one by name, or clears the pick.
pub(crate) const STYLE_COMMAND: &str = "/style";

/// The word after `/style` that clears the session's pick.
pub(crate) const STYLE_OFF: &str = "off";

/// The line that reports the session's spend limit, sets it, or removes it.
pub(crate) const LIMIT_COMMAND: &str = "/limit";

/// The word after `/limit` that removes the limit.
pub(crate) const LIMIT_OFF: &str = "off";

/// The line that opens the panel of preferences about the interface itself.
pub(crate) const CONFIG_COMMAND: &str = "/config";

/// The line that opens another directory, taking the path to open as its argument.
pub(crate) const ADD_DIR_COMMAND: &str = "/add-dir";

/// The word after `/add-dir` that closes the directory named after it instead.
pub(crate) const ADD_DIR_CLOSE: &str = "close";

/// The line that moves the session to another working directory, taking the path as its argument.
pub(crate) const CD_COMMAND: &str = "/cd";

/// The line that withdraws the yes an earlier session kept about the working directory, so the
/// next session begun there is asked (TRUST-24).
pub(crate) const FORGET_TRUST_COMMAND: &str = "/forget-trust";

/// The line that lists, adds and removes the reach remembered for commands (SANDBOX-23).
pub(crate) const REACH_COMMAND: &str = "/reach";

/// The line that lists the MCP servers, starts one that is not running, or turns one off for the
/// session (SERVERS-19).
pub(crate) const MCP_COMMAND: &str = "/mcp";

/// The word after `/mcp` that lists the servers, as the bare command does.
pub(crate) const MCP_LIST: &str = "list";

/// The word after `/mcp` that starts the servers that are not running, or the one named after it.
pub(crate) const MCP_RECONNECT: &str = "reconnect";

/// The word after `/mcp` that turns the server named after it off for the session.
pub(crate) const MCP_DISABLE: &str = "disable";

/// The line that reports the sandbox mode, or sets it from the next turn (SANDBOX-22).
///
/// Typed while a turn runs, every word but `off` is carried out as typed, and the turn keeps the mode
/// it began with (CMD-8).
pub(crate) const SANDBOX_COMMAND: &str = "/sandbox";

/// The line that reports what this session is and what it may touch.
pub(crate) const STATUS_COMMAND: &str = "/status";

/// The line that reports what each turn of this session has spent.
///
/// A word of its own rather than more rows on the status panel, because the list grows with the
/// session: a panel that answers "what is this session" in fifteen rows would answer it in fifty
/// on the fiftieth turn, and the breakdown is asked for when one figure looks wrong rather than
/// every time somebody checks which directory they are in.
pub(crate) const COST_COMMAND: &str = "/cost";

/// The line that reports what fills the context window, by category, from the last request.
pub(crate) const CONTEXT_COMMAND: &str = "/context";

/// The line that opens a read-only view of the last request built for the planner.
pub(crate) const REQUEST_COMMAND: &str = "/request";

/// The line that summarises the conversation so far, in place of sending all of it.
pub(crate) const COMPACT_COMMAND: &str = "/compact";

/// The line that starts a new session in place of this one.
pub(crate) const CLEAR_COMMAND: &str = "/clear";

/// The line that has a brief written for the next piece of work, to start a new session from.
pub(crate) const HANDOFF_COMMAND: &str = "/handoff";

/// The line that copies this session and moves onto the copy, taking its name as an option.
pub(crate) const BRANCH_COMMAND: &str = "/branch";

/// The line that picks up another session of this directory, by id or from the picker (CMD-14).
pub(crate) const RESUME_COMMAND: &str = "/resume";

/// The line that hands this session to a background process and leaves this one (BG-2).
pub(crate) const BACKGROUND_COMMAND: &str = "/bg";

/// The line that renames this session, taking the new name as its argument.
pub(crate) const RENAME_COMMAND: &str = "/rename";

/// The line that adds a note to the session record, taking the note as its argument.
///
/// The note is kept beside the conversation and never in it (SESSION-36).
pub(crate) const NOTE_COMMAND: &str = "/note";

/// The lines that say which issue and which pull request the session is for, taking the link.
pub(crate) const ISSUE_COMMAND: &str = "/issue";
pub(crate) const PR_COMMAND: &str = "/pr";

/// The line that asks a question beside the work, taking the question as its argument.
pub(crate) const BTW_COMMAND: &str = "/btw";

/// The line that recaps the session so far, taking no argument.
pub(crate) const RECAP_COMMAND: &str = "/recap";

/// What the aside row shows as the question a recap asked.
pub(crate) const RECAP_LABEL: &str = "/recap";

/// The line that repeats a prompt, taking the prompt and any interval as its argument.
pub(crate) const LOOP_COMMAND: &str = "/loop";

/// The line that sets the condition a session works towards, taking that condition as its
/// argument.
pub(crate) const GOAL_COMMAND: &str = "/goal";

/// The line that lists the standing watches, and ends one by its number.
///
/// It cannot arm one. A watch is asked for in a prompt, and what a person needs a command for is
/// the half they cannot read off the transcript: which watches are live, and how to end one.
pub(crate) const WATCH_COMMAND: &str = "/watch";

/// The line that lists the turn's background jobs, and asks for one to be stopped by its name.
///
/// The stop is a request the turn carries out at its next step (RUN-27), so a person can stop a
/// job while the turn goes on with everything else.
pub(crate) const JOBS_COMMAND: &str = "/jobs";

/// The line that opens or closes the info panel, as its key does.
pub(crate) const PANEL_COMMAND: &str = "/panel";

/// The line that turns holding off system sleep on or off.
pub(crate) const CAFFEINATE_COMMAND: &str = "/caffeinate";

/// The line that lists the checkouts delegates kept, and removes one by its number.
///
/// A checkout something was done in outlives its delegate, and nothing else removes it.
pub(crate) const CHECKOUTS_COMMAND: &str = "/checkouts";

/// The one line that ends the session instead of starting a turn.
pub(crate) const EXIT_COMMAND: &str = "/exit";

/// The words other terminal agents leave on when they are typed alone, with no slash.
///
/// Not commands: [`EXIT_COMMAND`] is the only line that leaves, so a question about leaving stays a
/// question (CMD-2). A line that is only one of these is held once before it is sent as the prompt it
/// is.
pub(crate) const LEAVE_WORDS: [&str; 2] = ["exit", "quit"];

/// Whether the line is only one of [`LEAVE_WORDS`], in any case and with spaces around it.
pub(crate) fn is_a_lone_leave_word(line: &str) -> bool {
    let line = line.trim();
    LEAVE_WORDS
        .iter()
        .any(|word| line.eq_ignore_ascii_case(word))
}

/// The line that writes the transcript as a markdown file.
pub(crate) const EXPORT_COMMAND: &str = "/export";

/// The line that puts a reply on the clipboard as the transcript holds it, taking how far back.
///
/// A sweep with the mouse copies what was drawn, so it carries the marker and indent beside every
/// row and breaks a paragraph where the terminal wrapped it (CMD-11).
pub(crate) const COPY_COMMAND: &str = "/copy";

/// The line that rewinds the conversation and restores files changed in the last turn.
pub(crate) const UNDO_COMMAND: &str = "/undo";

/// The line that lists what a rewind would put back, and goes back to one of those points.
///
/// A word of its own rather than an argument to `/undo`, because `/undo` deliberately takes none:
/// the table's empty argument column is what keeps `/undo the last thing I asked for` a prompt
/// (CMD-2), and a command that takes a number cannot also do that.
pub(crate) const REWIND_COMMAND: &str = "/rewind";

/// The line that lists the hunks the last turn wrote, and puts one of them back by its number.
///
/// A word of its own rather than an argument to `/rewind`, which goes back whole turns and is read
/// as a count of them (SESSION-21).
pub(crate) const HUNKS_COMMAND: &str = "/hunks";

/// The line that plans one task in full before anything is read, taking the task as its argument.
///
/// Not a mode the session holds. A session is several turns over one conversation and this is one
/// run with a frozen plan, so the word starts a run and the session comes back to the turn loop
/// when it ends. See `manifest_animated` in `app.rs` and `docs/specs/manifest.md`.
pub(crate) const MANIFEST_COMMAND: &str = "/manifest";

/// The line that runs one of the person's own definitions on a task, taking its name and the task.
///
/// One word however many definitions a machine holds: a name is written by whoever wrote the file
/// and can read like an instruction, so it is an argument here and never a command word or a
/// completion row (ADDRESS-2, ADDRESS-6). See `docs/specs/addressing-a-definition.md`.
pub(crate) const AGENT_COMMAND: &str = "/agent";
/// Lists each definition's memory, where it is kept and whether it is withheld (MEMORY-12).
pub(crate) const MEMORY_COMMAND: &str = "/memory";

/// The line that has the planner draft an `AGENTS.md` for a project that has none.
///
/// Starts an ordinary turn with words the driver wrote, and takes no argument, so `/init the
/// project` stays a prompt (CMD-2). See [`crate::init_command`] and CMD-13.
pub(crate) const INIT_COMMAND: &str = "/init";

/// The line that has the planner review local changes or a pull request.
///
/// Starts an ordinary turn with words the driver wrote around the target the person named. See
/// [`crate::review_command`] and CMD-18.
pub(crate) const REVIEW_COMMAND: &str = "/review";

/// The line that sets plan mode, and with a task starts a turn on it.
///
/// The same endorsement as the key that cycles to plan mode (MODE-12): it only narrows what is
/// permitted. There is no word for bypassing, which MODE-5 leaves to the command line.
pub(crate) const PLAN_COMMAND: &str = "/plan";

/// One command, and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// The word typed, including the leading slash.
    pub name: &'static str,
    /// What it takes after the word, or empty where it takes nothing.
    pub argument: &'static str,
    /// One line, for the list shown while a command is being typed.
    pub description: &'static str,
    /// What Enter on it does while a turn is running (CMD-8).
    pub mid_turn: MidTurn,
}

impl Command {
    /// What is drawn after the command once a space is typed, where there is room to spell out the
    /// words it takes. The completion list keeps the short `argument` so its description column
    /// stays narrow.
    pub fn typed_hint(&self) -> String {
        match self.name {
            EFFORT_COMMAND => {
                let levels: Vec<&str> = bravebot_aichat::protocol::Effort::ALL
                    .iter()
                    .map(|level| level.as_str())
                    .collect();
                format!("[{}]", levels.join(" | "))
            }
            SANDBOX_COMMAND => {
                let modes: Vec<&str> = bravebot_sandbox::SandboxMode::ALL
                    .iter()
                    .map(|mode| mode.name())
                    .collect();
                format!("[{}]", modes.join(" | "))
            }
            LIMIT_COMMAND => format!("[<n>[k|m] | <n> credits | {LIMIT_OFF}]"),
            REVIEW_COMMAND => {
                use crate::review_command::{COMMIT, PULL_REQUEST, SINCE, STAGED};
                format!("[{STAGED} | {SINCE} <ref> | {COMMIT} <ref> | {PULL_REQUEST} <n>] [focus]")
            }
            REACH_COMMAND => {
                let scopes: Vec<&str> = bravebot_sandbox::scope::Scope::REACHABLE
                    .iter()
                    .map(|scope| scope.name())
                    .collect();
                format!(
                    "[<{} | dir> [write] [always] -- <command> | remove <n> | keys]",
                    scopes.join(" | ")
                )
            }
            _ => self.argument.to_string(),
        }
    }
}

/// What Enter on a command does while a turn is running, which turns on what the command touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidTurn {
    /// Carried out as it is typed. It reads what the session keeps, ends something it has standing,
    /// sets a token the turn reads at its own next step, adds a file under the working directory, or
    /// changes only how the screen is drawn or what the box edits in, none of which changes what
    /// the turn is running under, and no line waiting ahead of it changes either.
    ///
    /// Where the bare word opens a picker, the picker is held as a mode of the session while the
    /// turn runs, so the turn's own questions are still drawn over it.
    Runs,
    /// Carried out as it is typed where it reads or ends what is standing, and waiting where it
    /// would start something: a loop or a goal armed mid-turn needs a look of its own.
    RunsUnlessItStarts,
    /// Carried out as it is typed when no line waiting ahead of it could change what it acts on. It
    /// changes only what the session keeps for itself or keeps for a later session, which the turn
    /// does not hold, but a line waiting ahead of it may replace that: `/clear` begins another
    /// session, `/cd` moves to another directory and a prompt starts a turn built with what it
    /// finds, and a change made first would land on the one being left or the turn begun under it.
    /// A waiting `/sandbox` line changes the mode and nothing else, so it holds none of them back.
    ///
    /// Where the bare word opens a picker, the picker is held as a mode of the session while the
    /// turn runs, so the turn's own questions are still drawn over it.
    Changes,
    /// Carried out as it is typed under the rule [`MidTurn::Changes`] gives, except where the form
    /// typed would ask the person a question, which waits: the screen is the turn's while it runs,
    /// and a yes given to a question drawn under another one is not one anybody read.
    ///
    /// `/sandbox`: a mode moves what the next turn is built with and nothing the turn in flight
    /// holds, and the move to `off` asks first (SANDBOX-22).
    ChangesUnlessItAsks,
    /// Waits for the turn to end, because it acts on the conversation, the workspace, the
    /// terminal or the network, and the turn holds all four.
    Waits,
}

/// Every command, in the order they are offered.
///
/// The one place they are written down. The hint line, the completion list and the key handler all
/// read from here, so a command that is renamed or added cannot leave any of them advertising
/// something that no longer works.
pub fn commands() -> [Command; 48] {
    [
        Command {
            name: STATUS_COMMAND,
            argument: "",
            description: t!(command_status),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: COST_COMMAND,
            argument: "",
            description: t!(command_cost),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: CONTEXT_COMMAND,
            argument: "",
            description: t!(command_context),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: MODEL_COMMAND,
            argument: "",
            description: t!(command_model),
            mid_turn: MidTurn::Changes,
        },
        Command {
            name: THEME_COMMAND,
            argument: "[name]",
            description: t!(command_theme),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: EFFORT_COMMAND,
            argument: "[level]",
            description: t!(command_effort),
            mid_turn: MidTurn::Changes,
        },
        Command {
            name: CONFIG_COMMAND,
            argument: "",
            description: t!(command_config),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: ADD_DIR_COMMAND,
            argument: "<path> | close <path>",
            description: t!(command_add_dir),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: CD_COMMAND,
            argument: "<path>",
            description: t!(command_cd),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: RENAME_COMMAND,
            argument: "<name>",
            description: t!(command_rename),
            mid_turn: MidTurn::Changes,
        },
        Command {
            name: NOTE_COMMAND,
            argument: "<note>",
            description: t!(command_note),
            mid_turn: MidTurn::Changes,
        },
        Command {
            name: ADVISOR_COMMAND,
            argument: "[model | off]",
            description: t!(command_advisor),
            mid_turn: MidTurn::Changes,
        },
        Command {
            name: LIMIT_COMMAND,
            argument: "[<amount> | off]",
            description: t!(command_limit),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: STYLE_COMMAND,
            argument: "[name | off]",
            description: t!(command_style),
            mid_turn: MidTurn::Changes,
        },
        Command {
            name: COMPACT_COMMAND,
            argument: "[focus]",
            description: t!(command_compact),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: BTW_COMMAND,
            argument: "<question>",
            description: t!(command_btw),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: RECAP_COMMAND,
            argument: "",
            description: t!(command_recap),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: CLEAR_COMMAND,
            argument: "",
            description: t!(command_clear),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: HANDOFF_COMMAND,
            argument: "<next goal>",
            description: t!(command_handoff),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: BRANCH_COMMAND,
            argument: "[<name>]",
            description: t!(command_branch),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: RESUME_COMMAND,
            argument: "[<id>]",
            description: t!(command_resume),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: BACKGROUND_COMMAND,
            argument: "",
            description: t!(command_background),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: FORGET_TRUST_COMMAND,
            argument: "",
            description: t!(command_forget_trust),
            mid_turn: MidTurn::Changes,
        },
        Command {
            name: REACH_COMMAND,
            argument: "[<where> -- <command>]",
            description: t!(command_reach),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: SANDBOX_COMMAND,
            argument: "[<mode>]",
            description: t!(command_sandbox),
            mid_turn: MidTurn::ChangesUnlessItAsks,
        },
        Command {
            name: LOOP_COMMAND,
            argument: "[[interval] <prompt> | stop]",
            description: t!(command_loop),
            mid_turn: MidTurn::RunsUnlessItStarts,
        },
        Command {
            name: GOAL_COMMAND,
            argument: "[<condition> | clear]",
            description: t!(command_goal),
            mid_turn: MidTurn::RunsUnlessItStarts,
        },
        Command {
            name: WATCH_COMMAND,
            argument: "[stop <n>]",
            description: t!(command_watch),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: JOBS_COMMAND,
            argument: "[stop <name> [<delegate>]]",
            description: t!(command_jobs),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: PANEL_COMMAND,
            argument: "",
            description: t!(command_panel),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: CAFFEINATE_COMMAND,
            argument: "",
            description: t!(command_caffeinate),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: PR_COMMAND,
            argument: "[<url> | clear]",
            description: t!(command_pr),
            mid_turn: MidTurn::Changes,
        },
        Command {
            name: ISSUE_COMMAND,
            argument: "[<url> | clear]",
            description: t!(command_issue),
            mid_turn: MidTurn::Changes,
        },
        Command {
            name: CHECKOUTS_COMMAND,
            argument: "[apply <n> | remove <n>]",
            description: t!(command_checkouts),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: PLAN_COMMAND,
            argument: "[task]",
            description: t!(command_plan),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: MANIFEST_COMMAND,
            argument: "<task>",
            description: t!(command_manifest),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: AGENT_COMMAND,
            argument: "<name> <task>",
            description: t!(command_agent),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: MEMORY_COMMAND,
            argument: "",
            description: t!(command_memory),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: INIT_COMMAND,
            argument: "",
            description: t!(command_init),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: REVIEW_COMMAND,
            argument: "[target] [focus]",
            description: t!(command_review),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: EXPORT_COMMAND,
            argument: "[path]",
            description: t!(command_export),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: COPY_COMMAND,
            argument: "[n]",
            description: t!(command_copy),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: REQUEST_COMMAND,
            argument: "",
            description: t!(command_request),
            mid_turn: MidTurn::Runs,
        },
        Command {
            name: UNDO_COMMAND,
            argument: "",
            description: t!(command_undo),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: REWIND_COMMAND,
            argument: "[turns]",
            description: t!(command_rewind),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: HUNKS_COMMAND,
            argument: "[number]",
            description: t!(command_hunks),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: EXIT_COMMAND,
            argument: "",
            description: t!(command_exit),
            mid_turn: MidTurn::Waits,
        },
        Command {
            name: MCP_COMMAND,
            argument: "[reconnect | disable] [alias]",
            description: t!(command_mcp),
            mid_turn: MidTurn::Waits,
        },
    ]
}

/// The commands a half-typed line could still become, in the order they are offered.
///
/// Empty unless the line is a lone word starting with a slash: a command takes its argument after a
/// space, so once there is one the command is settled and there is no command left to complete. A
/// line that is not a command at all completes to nothing, which is what closes the list.
pub fn completions(line: &str) -> Vec<Command> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('/') || trimmed.contains(char::is_whitespace) {
        return Vec::new();
    }
    commands()
        .iter()
        .filter(|command| command.name.starts_with(trimmed))
        .copied()
        .collect()
}

/// The line `/effort ` or `/sandbox ` plus the start of one of its words becomes, or `None` where
/// Tab has nothing to add.
///
/// The words are this program's own, so completing one reads nothing a person did not type. A
/// start that fits one word becomes that word, and a start that fits several is carried as far as
/// they agree.
pub(crate) fn complete_argument(line: &str) -> Option<String> {
    let line = line.trim_start();
    if let Some(typed) = line.strip_prefix(EFFORT_COMMAND) {
        let levels: Vec<&str> = bravebot_aichat::protocol::Effort::ALL
            .iter()
            .map(|level| level.as_str())
            .collect();
        return complete_word(EFFORT_COMMAND, typed, &levels);
    }
    let typed = line.strip_prefix(SANDBOX_COMMAND)?;
    let modes: Vec<&str> = bravebot_sandbox::SandboxMode::ALL
        .iter()
        .map(|mode| mode.name())
        .collect();
    complete_word(SANDBOX_COMMAND, typed, &modes)
}

fn complete_word(command: &str, after: &str, words: &[&str]) -> Option<String> {
    let typed = after.strip_prefix(' ')?;
    if typed.is_empty() || typed.contains(char::is_whitespace) {
        return None;
    }
    let typed = typed.to_ascii_lowercase();
    let mut fits = words.iter().filter(|word| word.starts_with(&typed));
    let mut shared = fits.next()?.to_string();
    for word in fits {
        let agreed = shared
            .bytes()
            .zip(word.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        shared.truncate(agreed);
    }
    (shared != typed).then(|| format!("{command} {shared}"))
}

/// The argument given to `command`, if that is what the line is.
///
/// `None` for anything else, so a prompt that merely mentions the word is still a prompt, and so is
/// a longer word that happens to start with it. The bare command with no argument is `Some("")`,
/// which the caller answers by saying what it needs rather than silently doing nothing.
pub(crate) fn argument_to<'a>(line: &'a str, command: &str) -> Option<&'a str> {
    let rest = line.trim().strip_prefix(command)?;
    if rest.is_empty() {
        return Some("");
    }
    // A following character that is not a space means a longer word, not this command.
    rest.strip_prefix(' ').map(str::trim)
}

/// The command word the line is, or `None` where the line is a prompt.
///
/// Read off the table rather than word by word, because the question is only which command was
/// typed and not what it does. A caller deciding what to do about one needs an arm each; a caller
/// that has only to recognise one gets every word the table names, so a command added there is
/// recognised here without anybody having to remember to.
///
/// A command the table gives no argument is only ever itself: the ladder dispatches those on the
/// bare word, so `/undo the last change` is a prompt there and has to be one here too. The
/// argument the table names is what says which of the two a word is, so the two agree by reading
/// the same column rather than by anybody keeping two lists in step.
pub(crate) fn command_typed(line: &str) -> Option<&'static str> {
    row_typed(line).map(|command| command.name)
}

/// The table's row for the command word the line is, or `None` where the line is a prompt.
fn row_typed(line: &str) -> Option<Command> {
    commands()
        .into_iter()
        .find(|command| match argument_to(line, command.name) {
            Some(argument) => !command.argument.is_empty() || argument.is_empty(),
            None => false,
        })
}

/// Whether the command a line is gets carried out as it is typed while a turn runs (CMD-8).
///
/// Read off the table's column, and for the two words that both read and start, off the form the
/// argument takes: `/loop stop` ends what is standing, and `/loop 5m check the deploy` would start
/// a loop while the turn in flight is still the session's work.
pub(crate) fn runs_while_working(line: &str) -> bool {
    let Some(command) = row_typed(line) else {
        return false;
    };
    match command.mid_turn {
        MidTurn::Runs | MidTurn::Changes => true,
        MidTurn::Waits => false,
        MidTurn::ChangesUnlessItAsks => {
            let argument = argument_to(line, command.name).unwrap_or_default();
            bravebot_sandbox::SandboxMode::parse(argument)
                != Some(bravebot_sandbox::SandboxMode::Off)
        }
        MidTurn::RunsUnlessItStarts => {
            let argument = argument_to(line, command.name).unwrap_or_default();
            match command.name {
                LOOP_COMMAND => {
                    !matches!(crate::loops::parse(argument), crate::loops::Asked::Start(_))
                }
                GOAL_COMMAND => {
                    !matches!(crate::goals::parse(argument), crate::goals::Asked::Set(_))
                }
                _ => false,
            }
        }
    }
}

/// Whether a waiting line is a `/sandbox` one, which moves the mode the next turn is built with and
/// writes nothing else (SANDBOX-22). The mode is read where a turn is built, by `/resume` and
/// `/status`, and when the transcript is drawn, and by no command that changes something, so
/// each of those gives the same result before it as after it. `/sandbox` itself is held back by
/// the rule for a line of the same command.
fn only_moves_the_sandbox(waiting: &str) -> bool {
    command_typed(waiting) == Some(SANDBOX_COMMAND)
}

/// The line, among those waiting ahead of position `at` in the queue, that a command carried out
/// mid-turn has to wait behind, with where that line is going. `at` is the command's own position
/// where it is already queued, and the length of the queue where it is about to join it.
///
/// Behind a waiting line of the same command: `/loop stop` typed after a waiting
/// `/loop 5m check the deploy` would find no loop to stop, and the loop would start after it. And a
/// command that changes something waits behind a line that could change what it acts on, so it
/// lands where it was typed: `/rename` ahead of a waiting `/clear` would name the session being
/// left, and `/effort` ahead of a waiting prompt would change the level that prompt was typed
/// under. Every line could, but a waiting `/sandbox` ([`only_moves_the_sandbox`]).
pub(crate) fn held_back_by<'a>(
    session: &'a Session,
    at: usize,
    line: &str,
) -> Option<(Bound, &'a str)> {
    let command = row_typed(line)?;
    let changes = matches!(
        command.mid_turn,
        MidTurn::Changes | MidTurn::ChangesUnlessItAsks
    );
    session.queued_ahead(at).find(|(goes, waiting)| match goes {
        Bound::CarriedOut => {
            command_typed(waiting) == Some(command.name)
                || (changes && !only_moves_the_sandbox(waiting))
        }
        Bound::IntoThisTurn | Bound::ItsOwnTurn | Bound::IntoTheNextTurn | Bound::Run => changes,
    })
}

/// Whether a line that would be carried out mid-turn waits anyway, for what is queued ahead of it.
pub(crate) fn waits_behind_the_queue(session: &Session, line: &str) -> bool {
    held_back_by(session, session.queued.len(), line).is_some()
}

/// A waiting line as the words that say a command waits behind it name it.
///
/// A command by the line that was typed, so a person can find it in the queue. A prompt and a shell
/// line by what they are, since either may run to a paragraph and neither is the reason.
pub(crate) fn named(goes: Bound, line: &str) -> String {
    const ROOM: usize = 40;
    match goes {
        Bound::CarriedOut => crate::render::printable(&crate::render::head_of(line, ROOM)),
        Bound::Run => t!(queued_a_shell_line).to_string(),
        Bound::IntoThisTurn | Bound::ItsOwnTurn | Bound::IntoTheNextTurn => {
            t!(queued_a_prompt).to_string()
        }
    }
}

/// What is said under the turn when a command typed while it runs is queued, so the line does not
/// wait without a reason (CMD-8). Read before the line is taken off the box, and from the same
/// facts [`runs_while_working`] and [`held_back_by`] decided it with.
pub(crate) fn why_it_waits(session: &Session, line: &str) -> Option<String> {
    let command = row_typed(line)?;
    let word = command.name;
    Some(if runs_while_working(line) {
        let (goes, ahead) = held_back_by(session, session.queued.len(), line)?;
        t!(
            command_waits_behind,
            command = word,
            line = named(goes, ahead)
        )
        .to_string()
    } else if command.mid_turn == MidTurn::ChangesUnlessItAsks {
        t!(command_waits_to_ask, command = word).to_string()
    } else {
        t!(command_waits_for_the_turn, command = word).to_string()
    })
}
