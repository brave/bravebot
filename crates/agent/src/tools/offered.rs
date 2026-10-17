//! The schema of every tool and the choice of which tools a turn is offered. Every string here is
//! read by the model, so none is looked up in a catalog (`tests/audience.rs`).
//!
//! None of it takes a `Labelled` value, a `Policy`, a `SlotStore` or a `Sink`: the tool bodies that
//! read, label or declassify content stay in `tools.rs`.

use super::{WHY, offer_runs};
use bravebot_aichat::protocol::Tool;
use bravebot_core::todo::Status;
use serde_json::json;

/// The statuses the schema advertises, taken from the kernel so the two cannot drift.
const TODO_STATUSES: [&str; 4] = Status::NAMES;

/// What a turn may say about when it runs again.
///
/// Four states rather than a flag, because "nobody is pacing this turn", "somebody else is" and
/// "nothing will ask this turn anything again" want three different answers. A turn nobody is
/// looping, on a caller that will put its line again, has to arrange its own later look or answer
/// a request to report a change from one read and stop. A tick of a loop the person timed has that
/// look coming already, so a tool for asking for one would have it schedule a second, and the
/// wait would be dropped: the interval decides when that loop runs, not the turn. A turn nothing
/// will ask again has the same wait dropped for the opposite reason, and the same answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheduling {
    /// Not a tick of anything, on a caller that will ask again. It may arrange the one later look
    /// a watch needs, which starts a loop repeating the line the person typed.
    ArrangingALook,
    /// A tick of a loop nobody gave an interval for, setting the pace of the next tick.
    PacingALoop,
    /// A tick of a loop the person gave an interval for. There is nothing here to decide.
    TheirInterval,
    /// Nothing will ask this turn's line again, whatever wait it names: a surface that runs one
    /// turn and exits, or a line this program wrote rather than the person.
    ///
    /// There is no later look to arrange, so the wait is discarded wherever one is asked for. A
    /// confirmation saying a later look is arranged and needs nothing from the person would be
    /// describing a watch that does not exist, which is why the tool is withheld here rather than
    /// answered.
    NoLaterLook,
}

impl Scheduling {
    /// Whether the tool is offered to the planner, and dispatched where it is called anyway.
    ///
    /// Withheld from a tick the person timed, which has nothing to decide, and from a turn
    /// nothing will ask again, which has nothing a wait could become. A call from either is
    /// answered the way any other name nobody offered is.
    pub fn offered(self) -> bool {
        matches!(self, Scheduling::ArrangingALook | Scheduling::PacingALoop)
    }
}

/// Whether this turn is offered `run`.
///
/// Read by the tool table and by dispatch, because both say what to do where `read_git` will not
/// open a repository. git answers nearly everything this reader declines and `run` is how git is
/// asked, so a turn holding one is sent there. A delegate that may read files and not run programs is
/// offered `read_git` and no `run`, and sending that one there costs a round on a name that is not on
/// its list and then a refusal it cannot act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Running {
    /// `run` is on this turn's list, so what is said may name it.
    Offered,
    /// It is not, so what is said names what can be done without it instead.
    Withheld,
}

impl Running {
    /// What a list of tools offers, read off the list rather than from the capability that gates
    /// `run`.
    ///
    /// A definition's own `tools:` line takes names off a list whose capabilities reach them, so
    /// the capability is the wider answer of the two and what a description may name is what is
    /// beside it on the list.
    pub fn on(offered: &[Tool]) -> Self {
        if offer_runs(offered) {
            Self::Offered
        } else {
            Self::Withheld
        }
    }

    /// Whether what is said may name `run`.
    pub fn offered(self) -> bool {
        matches!(self, Running::Offered)
    }
}

/// The tools the model may call.
///
/// `scheduling` says what this turn may say about when it runs again, which decides whether
/// `schedule_next` is offered at all and, where it is, which of two jobs its description describes.
/// `arming` says whether the session this turn belongs to keeps standing watches, which decides
/// whether `watch_file` is offered. `deadlines` is how long a command may run, which `run`'s
/// description quotes: a planner told 600 seconds where a person has made room for 1200 spends its
/// deadline on the figure it was told about and never asks for the run they paid for. `running` says
/// whether this list holds `run`, which `read_git`'s description names where a repository is one it
/// will not open.
pub fn available(
    scheduling: Scheduling,
    arming: crate::watch::Arming,
    deadlines: crate::exec::Deadlines,
    running: Running,
) -> Vec<Tool> {
    let mut tools = table(scheduling, arming, deadlines, running);
    for tool in &mut tools {
        ask_why(tool);
    }
    tools
}

/// What the planner is told of the refs and the stash its checkouts share, in the description of
/// `isolation` and in the answer to a spawn that made one (CHECKOUT-7).
pub(super) fn checkouts_share_refs(running: Running) -> String {
    let fetch = if running.offered() {
        "run it once yourself before starting them rather than asking each to"
    } else {
        "ask one of them for it rather than each"
    };
    format!(
        "Checkouts share remote-tracking refs and tags with your working directory and with each \
         other, so a git fetch in one updates them in all of them, and two fetches at the same \
         time can fail. Where delegates need a fetch, {fetch}. Checkouts also share one stash, so \
         changes git stash sets aside in one can be popped in any of them. Do not ask a delegate \
         in a checkout to use git stash."
    )
}

fn table(
    scheduling: Scheduling,
    arming: crate::watch::Arming,
    deadlines: crate::exec::Deadlines,
    running: Running,
) -> Vec<Tool> {
    // Which of the two answers a request about one file gets. Both exist wherever watches do, and
    // a description that named neither as the better one would leave the planner picking the one
    // it read first, which is the read's own paragraph and therefore always the loop.
    let watching = if arming.offered() {
        let mut said = " Where what is being waited on is this one file, watch_file is the better \
                         answer: it arms a standing watch that outlives this turn and reports the \
                         change with no turn of yours running at all."
            .to_string();
        // The second half only where there is a schedule_next to keep for something. A turn
        // offered no way to arrange a later look has nothing to save one for, and saying
        // otherwise sends it to a name that is not on its list.
        if scheduling.offered() {
            said.push_str(
                " Keep schedule_next for a wait that is not about one file, or where the user \
                 asked for their own line to be run again.",
            );
        }
        said
    } else {
        String::new()
    };
    // What a turn asked to report a change does about the next look, in the two descriptions that
    // route such a request: the read's change token for a file, the run's own output for a
    // program. Neither technique reaches past this turn by itself, so both have to say where the
    // later look comes from, and that differs by turn. A planner told to arrange one on a surface
    // offering no way to arrange it spends a round on a name that is not there, and then has a
    // refusal to explain to somebody who asked to be told when something changes.
    let (look_again_after_a_read, look_again_after_a_run) = match scheduling {
        Scheduling::ArrangingALook => (
            "The next look is yours to arrange: call schedule_next at the end of this turn and you are asked again after the wait, with the user's line sent unchanged, so a request to be told when a file changes is answered by reading it now and scheduling the look that would catch a change. Do that rather than telling somebody to arrange it themselves.",
            "So take the first look now and call schedule_next at the end of the turn, which has you asked again after the wait with the user's line unchanged.",
        ),
        Scheduling::PacingALoop | Scheduling::TheirInterval => (
            "Inside a loop the next tick is the next look, so there is nothing to arrange: report what this tick saw and leave the rest to the next one.",
            "Inside a loop that is already what happens, so take this tick's look, report what it saw and leave the rest to the next one.",
        ),
        Scheduling::NoLaterLook => (
            "Comparing a second token needs a later turn, and nothing will ask you again: this turn is the last one asked about the user's line. Where the request is to watch something or to be told when it changes, report what this look found and say plainly that no later look is coming, rather than promising a later report or telling the user to arrange one.",
            "Nothing will ask you again, though: this turn is the last one asked about the user's line, so take the look you can take inside it. Where the request is to watch something or to be told when it changes, say plainly that no later one is coming.",
        ),
    };
    // The two figures `run` quotes, in seconds, wherever its description names them. Written out
    // here because a description is the only place the planner can learn either: a call that names
    // nothing gets the first, and a call may raise its own deadline as far as the second.
    let (deadline_default, deadline_ceiling) =
        (deadlines.default.as_secs(), deadlines.ceiling.as_secs());
    // What read_git says to do where it will not answer: a repository or a working tree it may not
    // read, and a request none of its queries covers. git answers both and `run` is how git is
    // asked, so a turn holding one is sent there. A turn without one has no second way to that
    // history at all, and naming `run` to it costs what naming `schedule_next` on a surface
    // offering none costs: a round spent on a name that is not there, and then a question to report
    // back on unanswered.
    let (git_elsewhere, git_beyond) = if running.offered() {
        (
            "elsewhere it says so and you use run",
            "For --follow, blame or anything else use run.",
        )
    } else {
        (
            "elsewhere it says so, and no tool on your list reads that history instead",
            "--follow, blame and anything else this does not answer cannot be read here either. \
             Report what you could not read rather than describing how somebody with a shell would \
             read it.",
        )
    };
    // The tools ask_user sends the planner to for a fact about this machine. `run` is named only
    // where this turn holds one; a turn without it is sent to the others alone.
    let looking_tools = if running.offered() {
        "list_files, search, read_file or run"
    } else {
        "list_files, search or read_file"
    };
    let mut tools = vec![
        Tool::function(
            "read_file",
            format!(
                "Read a file from the workspace. Returns its lines. Ask for the whole \
             file: long ones come back one page at a time, and the result says so and gives the \
             offset to continue from. Name the file with path, or with path_ref where a listing \
             gave you a reference instead of a name. \
             \
             Every read comes back with a change token, an opaque string standing for the file as \
             it is now. The same token on a later read means nobody wrote the file in between; a \
             different one means somebody did. So answer a question about whether something \
             changes by reading it now, keeping the token, and comparing it with the token from \
             your next look. Take that baseline in this turn rather than describing how one would \
             be taken. For a file you may not be shown, the size in its reference is what there is \
             to compare instead. Two things to say when you report a comparison. A change shows up \
             at the look after it happened rather than when it happened, so say which looks you \
             compared and never a time of day, which you have no clock for. And a token that moved \
             says the file was written, not what changed: a rewrite of the same bytes moves it \
             too, and a change that leaves the file's modification time alone moves nothing. \
             {look_again_after_a_read} \
             Where the request is to watch something or to be told when it changes, and you \
             have taken a look and not scheduled another, say so, because no change with \
             nothing watching reads as a promise to report the next one. Any other request gets \
             no sentence about a later look.{watching} \
             \
             Ask for the same window of a file again while it is as it was and the answer is a \
             short notice carrying the same token, because the lines are already earlier in this \
             conversation. A different window, or a file somebody has written since, comes back as \
             lines. \
             \
             A picture or a PDF (.png, .jpg, .gif, .webp, .pdf) comes back as a reference rather \
             than as anything you can look at, whoever vouched for the directory it is in. Give \
             that reference to spawn_processor with a question about it and the answer comes back \
             the way any processor's does. That is how to check a screenshot or read a scanned \
             page."
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative path, e.g. src/main.rs. Give this \
                                        or path_ref, never both."
                    },
                    "path_ref": {
                        "type": "string",
                        "description": "A reference to a file whose name you were not shown, \
                                        e.g. \"ref:2\" from a listing. Only useful where that \
                                        file is one you may be shown: a reference to a \
                                        quarantined file already is the file, so reading it \
                                        returns nothing you do not have."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "1-based line to start at. Defaults to the start of \
                                        the file."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum lines to return, capped so one read cannot \
                                        fill the conversation. Omit it and you get the file \
                                        up to that cap, which is what you want almost always: \
                                        this is for stepping through something long, not for \
                                        sampling something short, and several windows of one \
                                        file cost more than the file did."
                    }
                },
                "required": []
            }),
        ),
        Tool::function(
            "list_files",
            format!(
                "List files in the workspace under a directory, recursively unless you give a \
                 depth. Give a glob pattern or a depth to narrow the result rather than listing \
                 everything: depth 1 is the one directory itself, the way ls reads it, and is \
                 what to use when you want to know what a project holds rather than every file \
                 it contains. A listing does not enter a directory with one of these names, at \
                 any depth, and does not report it, so its absence from a result does not mean \
                 the project has none: {}, and .claude/worktrees. Name one as directory to list \
                 it. In a directory you may not read, the names are quarantined and you get one \
                 reference per entry instead, saying of each whether it is a file or a directory \
                 the walk stopped at: use a file's as path_ref to read it, process it, and write \
                 it back, without ever being told what it is called.",
                bravebot_filetype::ignored_directories().join(", ")
            ),
            json!({
                "type": "object",
                "properties": {
                    "directory": {
                        "type": "string",
                        "description": "Workspace-relative directory. Use \".\" for the root."
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Optional glob, e.g. \"*.rs\" for Rust files at any \
                                        depth, \"src/**/*.rs\" to anchor it, or \
                                        \"**/*.{rs,toml}\" for either extension. Supports *, ?, \
                                        ** and brace groups. Character classes and extended \
                                        globs are not supported. A pattern with a / in it may \
                                        be written from the directory or from the workspace \
                                        root."
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Optional number of directory levels to descend. 1 \
                                        lists this directory and no deeper, naming each \
                                        subdirectory it stopped at so you can see where the \
                                        tree continues: with a trailing / where you are shown \
                                        the names, and as a reference that says it is a \
                                        directory where they are quarantined. Omit it to walk \
                                        the whole tree, which in a large project is thousands \
                                        of paths.",
                        "minimum": 1
                    }
                },
                "required": ["directory"]
            }),
        ),
        Tool::function(
            "write_file",
            "Write a UTF-8 text file in the workspace. Name the destination with path, or with \
             path_ref to write back to a file a listing gave you a reference to. Give either \
             the contents or a reference to quarantined content that becomes the contents. The \
             user must approve each write before it happens, so explain what you are changing; \
             a write to a path_ref is always shown, since the user is the only one who sees \
             which file it is.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative path, e.g. src/main.rs. Give this \
                                        or path_ref, never both."
                    },
                    "path_ref": {
                        "type": "string",
                        "description": "A reference to the file to write, e.g. \"ref:2\". Use \
                                        the reference a listing gave you to write back to a \
                                        file whose name you were never shown."
                    },
                    "contents": {
                        "type": "string",
                        "description": "The complete new contents of the file. Give this or \
                                        contents_ref, never both."
                    },
                    "contents_ref": {
                        "type": "string",
                        "description": "A reference whose quarantined content becomes the whole \
                                        file, e.g. \"ref:2\". This is how to write out something \
                                        you were never shown, such as what spawn_processor \
                                        produced."
                    }
                },
                "required": []
            }),
        ),
        Tool::function(
            "edit_file",
            "Replace an exact passage of text in an existing workspace file. Prefer this to \
             write_file when changing part of a file: the user approves a diff, which is \
             easier to review than a whole body. The user must approve each edit before it \
             happens, so explain what you are changing. To change several places in one \
             file, give them together in edits: the user approves one diff.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative path, e.g. src/main.rs. Give this \
                                        or path_ref, never both."
                    },
                    "path_ref": {
                        "type": "string",
                        "description": "A reference to the file to edit, e.g. \"ref:2\". Only \
                                        useful where that file is trusted, since an edit \
                                        locates a passage and that means reading it."
                    },
                    "old_text": {
                        "type": "string",
                        "description": "The exact text to replace, matched byte for byte \
                                        including whitespace and indentation. Must occur \
                                        exactly once unless replace_all is true. Include \
                                        enough surrounding lines to be unique."
                    },
                    "new_text": {
                        "type": "string",
                        "description": "The text to put in its place."
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Replace every occurrence instead of requiring \
                                        exactly one. Defaults to false."
                    },
                    "edits": {
                        "type": "array",
                        "description": "Several passages to replace in this one file, \
                                        instead of old_text and new_text. Applied in order, \
                                        each to the text the one before left, and all or \
                                        none: if one fails nothing is changed. The user \
                                        approves one diff of the result.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_text": {"type": "string"},
                                "new_text": {"type": "string"}
                            },
                            "required": ["old_text", "new_text"]
                        }
                    }
                },
                "required": []
            }),
        ),
        Tool::function(
            "apply_checkout",
            "Bring the files a delegate wrote in a checkout back into the user's working \
             directory. Name the checkout with the number its report gave, such as c1. Without \
             paths it takes every file the report said the driver recorded a write to by name; \
             with paths, only those. Each file is a write of the checkout's file to the same \
             path in the working directory, and the user is asked about each one and sees the \
             difference from their file as it is now, so explain what is coming back. A file \
             written through a reference, or by a program other than through a redirection, is \
             not brought back by this.",
            json!({
                "type": "object",
                "properties": {
                    "checkout": {
                        "type": "string",
                        "description": "The checkout's number, e.g. \"c1\", as a delegate's \
                                        report gave it."
                    },
                    "paths": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Optional. Paths relative to the checkout, taken from \
                                        the ones the report named. Leave it out to bring back \
                                        every file the driver recorded a write to."
                    }
                },
                "required": ["checkout"]
            }),
        ),
        Tool::function(
            "todo_write",
            "Record the task list for what you are doing, and keep it current. Send the whole \
             list every time: it replaces the previous one, so include finished tasks with \
             status completed rather than dropping them. Use it when the work takes several \
             steps, and update it as you go so the user can see progress. Skip it for a single \
             step or a question.",
            json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "description": "The complete list, in the order the work will happen.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": {
                                    "type": "string",
                                    "description": "The task, in a few words."
                                },
                                "status": {
                                    "type": "string",
                                    "enum": TODO_STATUSES,
                                    "description": "Mark exactly one task in_progress while \
                                                    work remains on it. Use cancelled for a \
                                                    task you have decided not to do, rather \
                                                    than dropping it or marking it completed."
                                }
                            },
                            "required": ["content", "status"]
                        }
                    }
                },
                "required": ["todos"]
            }),
        ),
        Tool::function(
            "search",
            "Find regular expressions in workspace files. Returns matching lines. Give every \
             spelling you would otherwise search for one at a time: a list of patterns costs \
             one call and matches a line matching any of them.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "description": "Regular expression to find. Supports ., *, +, ?, |, \
                                        (), (?:), [], \\d, \\w, \\s, ^, $, \\b, and (?i) to \
                                        ignore case. Counted repetition such as a{2,3}, \
                                        lookaround and other flags are not supported, and { is \
                                        an ordinary character. Escape a metacharacter with a \
                                        backslash to match it literally. May be a list, in \
                                        which case a line matches if it matches any of them: \
                                        prefer one call with every spelling you want over one \
                                        call each.",
                        "anyOf": [
                            {"type": "string"},
                            {"type": "array", "items": {"type": "string"}}
                        ]
                    },
                    "directory": {
                        "type": "string",
                        "description": "Workspace-relative directory or file to search. A \
                                        file is the only one searched and include is not \
                                        consulted for it. Defaults to \".\"."
                    },
                    "include": {
                        "type": "string",
                        "description": "Optional glob limiting which files are searched, \
                                        e.g. \"*.rs\" or \"**/*.{cc,h,mm}\". Supports *, ?, \
                                        ** and brace groups. Character classes and extended \
                                        globs are not supported. A pattern with a / in it may \
                                        be written from the directory or from the workspace \
                                        root."
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "description": "Whether case matters. Defaults to true. Set false \
                                        rather than shortening the pattern to dodge a capital."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "1-based match to start from. Defaults to the first. A \
                                        result that stopped at the match cap gives the offset to \
                                        continue from: use it rather than guessing a narrower \
                                        glob, which drops the matches you have not seen yet."
                    },
                    "context": {
                        "type": "integer",
                        "description": "Lines to show before and after each match, as grep -C \
                                        does. Defaults to 0, at most 10. Use it to read a hit \
                                        without a second call. Lines of context are not \
                                        matches and do not count toward the match cap or the \
                                        offset."
                    },
                    "output": {
                        "type": "string",
                        "enum": ["lines", "files", "count"],
                        "description": "What to return. 'lines' (the default) is the matching \
                                        lines. 'files' is only the paths of the files with a \
                                        match, each once. 'count' is how many lines match in \
                                        each file and in all. Use files or count to survey \
                                        where something occurs without paying for the lines; \
                                        they ignore offset and context and are not held to the \
                                        match cap."
                    }
                },
                "required": ["pattern"]
            }),
        ),
        Tool::function(
            "read_git",
            format!(
                "Read a repository's history from its .git directory without starting git: log lists \
             commits one per line, or each with its whole message, a page at a time; show \
             prints a commit with its diff or a file or directory at \
             a revision, diff compares two commits, and status lists staged, unstaged and \
             untracked paths as git status --short does. tags lists tags newest version first, \
             as git tag --sort=-v:refname does, and with a revision only those it reaches, as \
             --merged does; search finds the lines matching a regular expression in the files \
             at a revision, as git grep does. Works only where the whole of .git is \
             trusted, and for status the whole working tree, and where .git is a directory: it \
             declines the .git file a linked worktree or a submodule has, except at the root of \
             a checkout you were started in; {git_elsewhere}. Nothing git's \
             configuration names is applied: no diff drivers, textconv, \
             filters or signature checks, and no remote URL is ever returned. Status detects no \
             renames and lists a file an attribute would convert as not compared. {git_beyond}"
            ),
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "enum": ["log", "show", "diff", "status", "tags", "search"],
                        "description": "log, show, diff, status, tags or search."
                    },
                    "repository": {
                        "type": "string",
                        "description": "Workspace-relative directory holding the .git \
                                        directory. Defaults to \".\"."
                    },
                    "revision": {
                        "type": "string",
                        "description": "In git's syntax: a branch, tag, HEAD or an id or its \
                                        prefix, followed by ~N, ^N or ^{commit}. log takes one \
                                        revision or a range A..B and defaults to HEAD. show takes \
                                        one and defaults to HEAD; <revision>:<path> shows a file \
                                        or directory as it was. diff needs two, written A..B or \
                                        \"A B\". tags takes none, or one to list only the tags \
                                        it reaches. search takes one and defaults to HEAD."
                    },
                    "path": {
                        "type": "string",
                        "description": "Relative to the repository's root. Limits log to commits \
                                        that changed it, show, diff and status to changes \
                                        under it, and search to files under it."
                    },
                    "pattern": {
                        "type": "string",
                        "description": "search only, and required there: the regular expression \
                                        to look for, matched against each line."
                    },
                    "count": {
                        "type": "integer",
                        "description": "Commits a log lists, tags a list of tags shows, or lines \
                                        a search prints. Defaults to 20, at most 200."
                    },
                    "skip": {
                        "type": "integer",
                        "description": "log, tags and search: how many to pass over before \
                                        listing, as git log --skip does. An answer that stopped \
                                        with more left gives the skip that lists the next of \
                                        them."
                    },
                    "messages": {
                        "type": "boolean",
                        "description": "log only: print each commit's whole message beneath its \
                                        line, which is where a commit says why it was made and \
                                        what it closes. Defaults to false."
                    },
                    "since": {
                        "type": "string",
                        "description": "log only: skip commits made before this day, YYYY-MM-DD, \
                                        in UTC."
                    },
                    "until": {
                        "type": "string",
                        "description": "log only: skip commits made after this day, YYYY-MM-DD, \
                                        in UTC."
                    }
                },
                "required": ["query"]
            }),
        ),
        Tool::function(
            "repo_map",
            "Get a map of the code in a directory: the declarations (functions, types, classes, \
             constants) its source files make, each with the line it is on and the line that \
             opens it, the ones other files mention most first. Use it first on a project you \
             have not read, or to find where something lives, instead of listing the tree and \
             reading files one at a time. It reads Rust, Python, JavaScript and TypeScript, Go, \
             Java and Kotlin, and C and C++ files, needs no language server, and shows names \
             and signatures only, never bodies: read_file reads the lines it points at. In \
             Java and in C and C++ it finds types and macros but not functions or methods. It maps \
             only files you have been allowed to read; files nobody vouched for are counted, \
             never named, and a file that holds what looks like a credential is left out.",
            json!({
                "type": "object",
                "properties": {
                    "directory": {
                        "type": "string",
                        "description": "Workspace-relative directory to map. Defaults to \".\". \
                                        Map a subdirectory rather than raising budget when the \
                                        project is large."
                    },
                    "budget": {
                        "type": "integer",
                        "description": "Roughly how many tokens the map may take. Defaults to \
                                        1000, at least 100 and at most 8000. A map that did not \
                                        fit says how many declarations it left out."
                    }
                },
                "required": []
            }),
        ),
        Tool::function(
            "lsp",
            "Ask a language server about a symbol: where it is defined, what refers to it, what \
             implements it, what calls it. Use this instead of search when the question is about \
             a symbol rather than about text, because a search for a name finds every comment \
             and string that mentions it while this finds the declaration the compiler agrees \
             on. Give the position of the symbol you are asking about, taken from a line you \
             were already shown by a search or a read; nothing here works out where a name is \
             for you. Answers with the places it found, one per line. Where a definition is in \
             a dependency rather than in this project the line says so, and read_file will not \
             open it. hover answers with prose, and that prose always comes back as a reference \
             you cannot read rather than as text, because nothing in a server's answer says \
             which file wrote it. diagnostics takes a path and no position and reports what the \
             server has found wrong with that file: how many errors, the line each starts on, \
             and how many warnings. Use it after an edit to learn what the edit broke before \
             running a build. The server's messages come back as a reference you cannot read, \
             for the same reason as hover. A server that has not reported says so, which is not \
             the same as a file with no errors, and a report with no errors is not a passing \
             build: a server can report more once it has finished working.",
            json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "description": "Which question to ask.",
                        "enum": [
                            "goToDefinition",
                            "findReferences",
                            "hover",
                            "documentSymbol",
                            "workspaceSymbol",
                            "goToImplementation",
                            "incomingCalls",
                            "outgoingCalls",
                            "diagnostics"
                        ]
                    },
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative path to the file holding the symbol, \
                                        e.g. src/main.rs. Required for every operation but \
                                        workspaceSymbol. For diagnostics, the file to check."
                    },
                    "line": {
                        "type": "integer",
                        "description": "1-based line of the symbol, as a search or a read \
                                        reported it. A line that no longer holds the symbol \
                                        answers with nothing rather than failing. Not used by \
                                        diagnostics.",
                        "minimum": 1
                    },
                    "character": {
                        "type": "integer",
                        "description": "1-based column of the symbol on that line. Point at the \
                                        name itself rather than at the start of the line.",
                        "minimum": 1
                    },
                    "query": {
                        "type": "string",
                        "description": "The name to look for. Only for workspaceSymbol, which \
                                        searches a language server's index rather than starting \
                                        from a position, and so needs a server already running: \
                                        ask about a symbol in a file of that language first."
                    }
                },
                "required": ["operation"]
            }),
        ),
        Tool::function(
            "spawn_processor",
            "Transform quarantined content you were not shown. Spawns an isolated model with no \
             tools, no memory and nothing to read but the references you name; it follows your \
             instruction and its output is quarantined as a new reference. This is how to \
             change a file you cannot see: name the file's reference, say what has to be true \
             of it afterwards, then pass the reference that comes back to write_file as \
             contents_ref. You are not shown its output either, and nobody reads it before it \
             is written, so be exact about the shape of the answer: the complete document and \
             nothing else. What the change should be is its decision, not yours. It reads the \
             whole document, so it can work out what is wrong and where the fix goes, and leave \
             a file alone if it turns out not to be the one you are after. Once your context holds \
             private content, as it does after read_output or vet_content shows you some, after \
             a delegate reports some, and in a conversation resumed with some, every call is \
             refused for the rest of the conversation: what you write is private from then on, \
             and a processor takes no private instruction.",
            json!({
                "type": "object",
                "properties": {
                    "reads": {
                        "type": "array",
                        "description": "The references to give it, e.g. [\"ref:0\", \"ref:1\"]. \
                                        At least one, and usually every reference that bears on \
                                        the task: the input names each block by its reference, \
                                        so one that can see the whole set can tell which file is \
                                        which. It still returns one document.",
                        "items": {"type": "string"}
                    },
                    "about": {
                        "type": "string",
                        "description": "Which of the references in reads this call is about. \
                                        The answer replaces that document and may be written to \
                                        no other file, and an answer that marks no document \
                                        leaves that one standing, which is how a processor with \
                                        nothing to change says so. Required when reads names \
                                        more than one file: one answer has one destination, and \
                                        nothing else can say which."
                    },
                    "instruction": {
                        "type": "string",
                        "description": "What to do with them and what to produce. Where the \
                                        result is going into a file, ask for the whole document \
                                        and nothing else: no explanation, no summary, no code \
                                        fence. Whatever comes back is what gets written. Give \
                                        the symptom the user reported and ask for the cause to \
                                        be found; naming a remedy you have not verified makes \
                                        it apply your guess rather than diagnose. Include the \
                                        file's name and language if that matters, because the \
                                        processor knows nothing but what you tell it and what \
                                        the references hold. May be conditional: say what the \
                                        document must look like if it is the one the task is \
                                        about, and to say why and produce no document if it is \
                                        not."
                    }
                },
                "required": ["reads", "instruction"]
            }),
        ),
        Tool::function(
            "load_skill",
            "Read one of the skills listed for you, or one search_skills found. A skill is \
             instructions the user wrote for a kind of task. Load one before doing that kind of \
             work, and follow what it says. Only a name you were listed or that search_skills \
             returned exists; there is no path to give.",
            json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The name of a skill exactly as it was listed for you \
                                        or as search_skills returned it, e.g. commit-style."
                    }
                },
                "required": ["name"]
            }),
        ),
        Tool::function(
            "search_skills",
            "Find a skill by a word or phrase, among skills of the user's own and the \
             project's own that are not listed for you up front. Matches a skill whose name or \
             description holds every word of the query, and returns each match's name and \
             description in full. Load a match with load_skill before doing the kind of work it \
             names.",
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "One or more words. A skill matches only where every \
                                        word is in its name or its description, so a longer \
                                        query narrows the search rather than widening it."
                    }
                },
                "required": ["query"]
            }),
        ),
        Tool::function(
            "ask_user",
            format!(
                "Ask the user up to four questions and wait for their answers. Only for what you \
             cannot find out yourself: which of two approaches to take, whether something is in \
             scope, which of two plausible files they meant. Never for a fact about this machine. \
             A path, a filename, whether a program is installed, what something is called: go and \
             look with {looking_tools} instead, and note that a quarantined \
             result does not stop you asking afterwards, so looking first costs you nothing. Ask \
             everything the plan turns on in one call rather than a question per turn; they are \
             put to the user one at a time. Offer concrete options where you can; the user may \
             also answer in their own words or skip a question, and a skipped question is an \
             answer to work with rather than a reason to ask again."
            ),
            json!({
                "type": "object",
                "properties": {
                    "questions": {
                        "type": "array",
                        "description": "The questions to put, at most four. A limit rather than \
                                        a target: ask what the work turns on and no more.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "header": {
                                    "type": "string",
                                    "description": "Two or three words naming what this asks \
                                                    about, shown as a tag beside it, e.g. \
                                                    \"Cache layer\". It is how the user tells \
                                                    one question from the next."
                                },
                                "question": {
                                    "type": "string",
                                    "description": "The question, in one sentence."
                                },
                                "options": {
                                    "type": "array",
                                    "description": "The choices to offer. Give them here rather \
                                                    than listing them inside the question text: \
                                                    only these are shown as choices. Each may \
                                                    be a plain string, or an object with a \
                                                    'label'. The user can always answer in \
                                                    their own words instead, so there is no \
                                                    need to offer an 'other'. Omit only for a \
                                                    question that genuinely has no set answers.",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "label": {
                                                "type": "string",
                                                "description": "The option, in a few words."
                                            },
                                            "detail": {
                                                "type": "string",
                                                "description": "Optional line saying what \
                                                                choosing it means."
                                            }
                                        },
                                        "required": ["label"]
                                    }
                                },
                                "multiple": {
                                    "type": "boolean",
                                    "description": "Set true when this question asks for more \
                                                    than one answer, such as \"which of these\" \
                                                    or \"pick any that apply\". Left false the \
                                                    user can pick only one. Defaults to false."
                                }
                            },
                            "required": ["header", "question"]
                        }
                    }
                },
                "required": ["questions"]
            }),
        ),
        Tool::function(
            "run",
            // Joined rather than formatted: this description states shell syntax the planner may
            // not use, `${...}` and `{a,b}` among it, and a format string would have to escape
            // every brace in it to say so.
            "Run a command line. Write it the way you would type it: `grep -rn thing src/ | \
             head -30`. Pipes, &&, ||, ;, ( ), quoting, redirection (>, >>, <, 2>, 2>&1, &>), \
             globs and {a,b} all work in the operands. There is no shell: the line is compiled \
             here into the programs and arguments it names, and anything that cannot be worked \
             out from the line itself is refused rather than guessed at. $(...), backticks, $VAR \
             and ${...} are refused for that reason: write the value out. Name the program \
             outright for the same reason, since a pattern there stands for whichever file it \
             matches today: `./scr*.sh` is refused where `./script.sh` runs. Quoting settles all \
             of them, so '$HOME' is six characters and reaches the program as one argument. A \
             `$` followed by no name, brace or parenthesis, as the end anchor in `grep -E \"x$\"`, is \
             an ordinary character. \
             A \
             pattern the program should match itself is quoted for the same reason, so write \
             `find . -name '*.md'`: unquoted, `*.md` is replaced by the files it matches here, \
             and passed as written only when none matches. A pattern after the = of an option, as in `grep -r x . --include=*.md`, \
             is passed to the program as written. The user \
             approves the compiled plan before anything runs, so say what you are running and \
             why first. \
             \
             Narrowing the result inside the line is the cheapest thing you can do, and usually \
             the difference between a few lines and a few thousand tokens: pipe to `head -n`, \
             ask grep for `-l` to get names only or `-c` for a count, take a line range with \
             `sed -n 40,80p` rather than reading a whole file. Use whichever tool returns less, \
             and a command that filters usually returns less. \
             \
             Output comes back as text you can read where the user has vouched for every \
             command in the line; otherwise it comes back as a reference, like a file you may \
             not read: pass it to read_output to see it, hand it to \
             spawn_processor, or write it to a file with write_file. Do use it to compile and \
             test what you changed. \
             \
             A program meant to keep running, such as a server or a watcher, needs \
             background: true. Without it the line is waited on and stopped at its deadline \
             ("
            .to_string()
                + &format!(
                    "{deadline_default} seconds by default; set deadline_seconds to allow up to \
                     {deadline_ceiling}"
                )
                + "), so there is \
             no moment at which it is up and you can do anything with it. \
             \
             Asked to watch something, or to say when it changes, decide first what is being \
             watched, because a file and a program are watched by different means. A file takes no \
             command at all: read_file hands back a change token, and comparing the token from one \
             look with the token from the next is the whole of the technique. A program's own \
             output is watched here: start it with background: true and call job_output with \
             wait_seconds, which is one call covering a window rather than a look per turn. \
             Neither reaches past this turn by itself: a background job is killed when the turn \
             ends unless the account of its start says the session keeps it, and comparing a token \
             needs a later look. "
                + look_again_after_a_run
                + " And say which window you watched, or which looks you compared, rather than \
                   a time of day, which you have no clock for. Where the request is to watch \
                   something or to be told when it changes, and you have scheduled no further \
                   look, say that too. Any other request gets no sentence about a later look.",
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "One command line. Programs are looked up on PATH, or \
                                        taken as paths relative to the directory the line runs \
                                        in, which is where its arguments are resolved too. A \
                                        newline is not accepted, since this is one line and not \
                                        a script."
                    },
                    "stdin_ref": {
                        "type": "string",
                        "description": "A reference whose contents are fed to the first \
                                        program's standard input, e.g. \"ref:1\". This is how \
                                        sed, awk, grep or jq are run over a document you may \
                                        not read: you name the reference and the contents go \
                                        into the program without passing through you. The \
                                        program reads them as if they had been typed at it, so \
                                        do not also add a '<' redirection to the line, and do \
                                        not use it with background: true. What comes back is \
                                        quarantined the same way any other run's output is."
                    },
                    "scopes": {
                        "type": "array",
                        "items": { "type": "string", "enum": bravebot_sandbox::scope::Requested::MENU },
                        "description": "Credential scopes, toolchain lists and `loopback` to add to \
                                        every stage of this one line, by name from the list \
                                        given, for a script that runs `gh`, `aws`, `git commit` \
                                        or a build inside it and so shows no operation of its \
                                        own. `loopback` lets the line listen on and connect to \
                                        ports of this machine, which `cargo test` and `sccache` \
                                        need on macOS. `ssh-key` has the user choose again \
                                        which ssh key a `git push` offers its owner, after a \
                                        push was refused for permission. Only where the session \
                                        is told it accepts them. The user is asked about the line \
                                        every time with the names shown, and no answer to it is \
                                        remembered. A name outside the list is an error."
                    },
                    "unconfined": {
                        "type": "boolean",
                        "description": "Start this one line with no sandbox, for a program \
                                        that has to write where the sandbox does not let it, \
                                        such as a login that stores a credential in its own \
                                        directory, or for a line that redirects its output to \
                                        a file outside the workspace, which a sandboxed line \
                                        cannot do. Only where the session is told it accepts \
                                        it. The user is asked about the line every time, shown \
                                        the files it writes and told that it runs with no \
                                        sandbox, and no answer to it is remembered. What it \
                                        prints is not trusted. Do not combine it with \
                                        'scopes'."
                    },
                    "directory": {
                        "type": "string",
                        "description": "Directory to run the command in, relative to the \
                                        workspace or inside a directory the user added. \
                                        Defaults to \".\" on the first call, and the last one \
                                        given is where a later call with no directory runs. \
                                        Naming one asks the user every time, since where a \
                                        program runs decides as much as its arguments do."
                    },
                    "deadline_seconds": {
                        "type": "integer",
                        "description": format!(
                            "How long to wait for the command, in seconds. Defaults to \
                             {deadline_default}. Held to between {} and {deadline_ceiling} \
                             seconds, so anything outside that becomes the nearest of the two. A \
                             command that outlasts its deadline is stopped and what it printed \
                             comes back. Has no effect with background: true, which is not waited \
                             for at all.",
                            crate::exec::FLOOR.as_secs()
                        )
                    },
                    "background": {
                        "type": "boolean",
                        "description": "Leave it running instead of waiting for it, and hand \
                                        back a job name. For a program meant to keep going: a \
                                        server, a watcher, a log follower. Use it when you need \
                                        the program still up while you do something else, such \
                                        as starting a server and then fetching a page from it. \
                                        While this turn is still going you are told when it \
                                        ends, with how it ended and what it printed, so nothing \
                                        has to poll for that. Call job_output with the name to \
                                        see what it has printed before then. \
                                        Must be one pipeline with no redirection, and \
                                        it is killed when this turn ends. Defaults to false, \
                                        which waits and hands back the output."
                    },
                    "read": {
                        "type": "boolean",
                        "description": "Ask to read what the command prints in this result. \
                                        Where read_output would hand it to you without asking \
                                        the user or checking it, it comes back here as text you \
                                        can read, and there is no read_output call to make. \
                                        Output too long for one result still comes back as a \
                                        reference, with its size, and read_output is not held \
                                        to that size. \
                                        Everywhere else this changes nothing, and output you \
                                        may not read still comes back as a reference. Defaults \
                                        to false. Not with background: true, which has printed \
                                        nothing yet."
                    }
                },
                "required": ["command"]
            }),
        ),
        Tool::function(
            "job_output",
            "See what a background job has printed, and whether it has ended. Give the job name \
             run handed back. Each call reports what is new since the last one, so calling it \
             again after doing something else shows what happened in between rather than \
             repeating what you have seen. Output is quarantined exactly as a run's is: it comes \
             back as text where the user vouched for every command in the line, and otherwise as \
             a reference to pass to read_output, spawn_processor or write_file. \
             \
             Use wait_seconds to wait for the job rather than asking it again and again. Without \
             it a wait costs a whole turn per look: you call, are told nothing has happened, and \
             answer only to be asked the same question. It is still a wait inside this turn, so it \
             cannot tell you about anything that happens after the turn ends, and the account of the \
             job's start says whether it is killed then or kept.",
            json!({
                "type": "object",
                "properties": {
                    "job": {
                        "type": "string",
                        "description": "The job name run gave you, e.g. \"job:1\"."
                    },
                    "kill": {
                        "type": "boolean",
                        "description": "Stop the job after reading what it printed. Use it once \
                                        you are done with a server you started. Defaults to \
                                        false, which leaves it running."
                    },
                    "wait_seconds": {
                        "type": "integer",
                        "description": "Wait up to this many seconds instead of answering at \
                                        once. Returns as soon as the job prints something you \
                                        have not been shown or the job ends, and at the bound if \
                                        it stays silent, so a long wait costs nothing when the \
                                        thing you are waiting for happens early. Between 1 and \
                                        600. A value outside that is refused rather than \
                                        adjusted, so the wait you get is the wait you asked for. \
                                        Omit it to look and answer straight away."
                    }
                },
                "required": ["job"]
            }),
        ),
        Tool::function(
            "read_output",
            "Ask to be shown what a command printed. Give the reference a run handed back. Use it whenever you ran something to find something out, which is most of \
             the time: a run's output is quarantined by default, so `which`, `find`, `uname` and \
             the like tell you nothing until you ask for the result. Ask for the errors too when \
             a run fails, or you will not know why it failed and must not claim it succeeded. \
             Only for output from run; a quarantined file is not readable this way. For \
             anything else you are holding a reference to, vet_content is the question to ask. \
             \
             Output you were shown the beginning and end of, because the whole was too long for \
             one result, is the exception: you may read all of it, so nobody is asked, and it \
             comes back a page at a time from the offset you give. A pattern searches that output \
             instead of paging it: you get the matching lines, each numbered with the byte it \
             starts at, so a second call with that byte as the offset reads on from the match you \
             wanted. A pattern on output you may not read is refused rather than asked about.",
            json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "The reference a run gave you, e.g. \"ref:5\"."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "For output too long for one result, the byte to start \
                                        from: the sample you were shown says where its middle \
                                        begins, and each page names the offset of the next. \
                                        Defaults to 0. Not for output you were not shown."
                    },
                    "pattern": {
                        "anyOf": [
                            {"type": "string"},
                            {"type": "array", "items": {"type": "string"}}
                        ],
                        "description": "Regular expression to find in output you may already \
                                        read, in the subset search accepts. Answers with the \
                                        matching lines, each numbered with the byte it starts at, \
                                        and a little context either side. Use the byte of the \
                                        match you want as the offset of a second call to read on \
                                        from there. May be a list, in which case a line matches \
                                        if it matches any of them. Not with offset, and not for \
                                        output you were not shown."
                    }
                },
                "required": ["ref"]
            }),
        ),
        Tool::function(
            "vet_content",
            "Ask to be shown something you are holding a reference to, after a check has looked \
             at it for you. Give the reference and say what you expect it to hold. A second model \
             with no tools and no memory reads the content and says in one word whether it looks \
             like an attempt to give instructions to whoever reads it next; the user sees the \
             content, that word and the reason for it, and decides. If they agree, the content \
             comes back to you as text you can read. \
             \
             Use it for a page you fetched or a file nobody has vouched for, when working blind \
             on it is not enough and you actually need to know what it says. Say plainly in your \
             reply why you need to read it rather than pass it to spawn_processor, because that \
             is what the user is weighing. \
             \
             What a yes covers is this content, this once. Nothing about the file or the host is \
             vouched for, so reading the same path again asks again, and asking twice for the \
             same thing is a refusal you earned. It is also not a way around a refusal: if the \
             user said no, work with what you have.",
            json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "The reference you are holding, e.g. \"ref:5\"."
                    },
                    "expects": {
                        "type": "string",
                        "description": "What you think this holds and why you want it, in a \
                                        sentence, e.g. \"the changelog for version 2, to find \
                                        out what changed\". The check is told this so it can \
                                        say whether the content is that; the user reads it too. \
                                        Your own words, not anything you were shown."
                    }
                },
                "required": ["ref", "expects"]
            }),
        ),
        Tool::function(
            "spawn_agent",
            // No numbers for the depth or the ceiling, which delegates read here too: a model
            // told how many it has left spends them.
            "Hand a whole sub-task to a second agent and get back one report. It has its own \
             context, so everything it reads stays with it and only what it says at the end \
             reaches you. That is what this is for: running a build reads the whole log, and \
             asking a delegate to run the build tells you what failed. Use it when finding \
             something out would cost you more context than the answer is worth, or when a \
             sub-task is separable enough to describe in a paragraph. It cannot ask the user \
             anything, so give it everything it needs in the task. It may hand parts of that \
             task to delegates of its own, though not without end: the tree stops a few levels \
             down, and one turn starts only so many however they are arranged. Its writes, its \
             runs and its calls to an MCP server's tools are still shown to the user for \
             approval, so this saves you context and never an approval. Do not use it for \
             something one read would answer: it is a \
             whole second agent and costs like one.",
            json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "description": "Which kind of agent, narrowest first. \"reader\" reads, \
                                        lists, searches and runs processors: use it to find \
                                        something out. \"checker\" also runs programs and asks \
                                        a language server, so it can build, test and lint, but \
                                        writes nothing: use it to \
                                        find out whether something works. \"worker\" also \
                                        writes files and calls the tools of the MCP servers you \
                                        may: use it to finish a sub-task. Pick the narrowest \
                                        one that can do the job.",
                        "enum": ["reader", "checker", "worker"]
                    },
                    "task": {
                        "type": "string",
                        "description": "What it has to do, in your own words, as the whole of                                         what it will know. It cannot see this conversation, your                                         references, the user's prompt or anything you have read,                                         and it cannot come back for more, so name the paths, the                                         commands, the symptom and what a finished answer has to                                         contain. Say what you want reported, not just what you                                         want done."
                    },
                    "each": {
                        "type": "array",
                        "description": "Optional. Start one delegate per entry instead of one \
                                        in total, each told the task followed by its own entry. \
                                        Put what they share in task and only the differing part \
                                        here, e.g. one path per entry. Writing the same \
                                        paragraph out four times costs you the seconds it takes \
                                        to say it, and the delegates cannot start until you \
                                        have.",
                        "items": {"type": "string"}
                    },
                    "isolation": {
                        "type": "string",
                        "description": format!(
                            "Optional. \"checkout\" gives each delegate a new checkout of the \
                             last commit to work in, so that delegates writing at the same time \
                             do not edit or build in one working tree. Changes not yet committed \
                             are not in it, and what it changes stays there until it is brought \
                             back. Not for a \"reader\", and not for a delegate that is already \
                             in one. A definition may ask for one itself, and its delegates are \
                             then started as if this were set. {}",
                            checkouts_share_refs(running)
                        ),
                        "enum": ["checkout"]
                    },
                    "mcp_servers": {
                        "type": "array",
                        "description": "Optional. The MCP servers a \"worker\" keeps, each named \
                                        as the server's part of its tools' names \
                                        (mcp__<server>__<tool>), out of the ones you may call. \
                                        Leave it out and it keeps every one; [] keeps none. \
                                        Give it only the servers its task needs. A \"reader\" \
                                        and a \"checker\" keep none either way.",
                        "items": {"type": "string"}
                    }
                },
                "required": ["kind", "task"]
            }),
        ),
        Tool::function(
            "fetch_url",
            "Fetch an http or https URL. Not the way to read a github.com pull request or issue \
             where the prompt says the GitHub CLI is on PATH, which names the commands that are. \
             What comes back is quarantined, like a file nobody \
             vouched for: you get a reference rather than the text, and you cannot read it or be \
             told what it says. Hand the reference to spawn_processor to have a question answered \
             about it, or to write_file as contents_ref to save it. Where you have to read the \
             page yourself, vet_content has it checked and asks the user to show it to you. The \
             user is asked before anything leaves the machine.",
            json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The absolute http or https URL to fetch."
                    }
                },
                "required": ["url"]
            }),
        ),
        Tool::function(
            "download_url",
            "Save an http or https URL to a file in the workspace without reading it: binary \
             safe, and the bytes go from the network to the file and never into your context. \
             Use it for a picture, archive, PDF, font or any file fetch_url would corrupt. It \
             replaces an existing file at the path. The file is quarantined like anything fetched, so \
             you cannot read it back or be told what it says. The user is asked about the host and \
             about the destination before anything is written.",
            json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The absolute http or https URL to download."
                    },
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative path to save it to, e.g. \
                                        assets/logo.png."
                    }
                },
                "required": ["url", "path"]
            }),
        ),
    ];

    if running.offered() {
        tools.push(Tool::function(
            "request_path",
            "Ask the user to let the programs you start with run reach one path outside the \
             workspace, for the rest of this session. Use it when a run was refused or failed \
             because it could not read or write a path you need, and name that path. The user is \
             shown the path and your reason and answers yes or no. A yes lasts until the session \
             ends or the user ends it, applies to every later run, and, except as said below, \
             does not make the directory trusted: files in it are still not believed any more \
             than before. It never reaches a filesystem root, the home directory or a \
             directory above it, credential locations, or a pattern, and it is refused where \
             the user turned the sandbox off or the workspace is not trusted. It does not widen \
             read_file, write_file, edit_file or the other file tools, which stay inside the \
             workspace and the directories the user added with /add-dir, unless the user has \
             turned permission prompts off: there a request is granted without a question, also \
             opens the directory to those tools and trusts it, as /add-dir does, and the result \
             says whether they now reach it.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "One existing absolute path, or one beginning ~/, e.g. \
                                        /opt/toolchain/include. A file or a directory; a \
                                        directory covers what is inside it."
                    },
                    "write": {
                        "type": "boolean",
                        "description": "True when programs must also write there. Writing \
                                        implies reading. Defaults to false, which asks for \
                                        reading only."
                    }
                },
                "required": ["path"]
            }),
        ));
    }

    if arming.offered() {
        tools.push(Tool::function(
            "watch_file",
            "Be told when one file changes, with no turn of yours running to notice it. This \
             arms a standing watch: the file is looked at every few seconds from now on, and the \
             first look that finds its size or modification time moved, or finds it removed or \
             newly there, begins a new turn naming this watch and this path. The watch outlives \
             this turn and every turn after it.\n\n\
             Nothing is read, now or when it fires. A fire says the path looks written to, no \
             longer exists, or now exists, and says nothing else about the file, so read it then \
             if you need what is in it. Two things follow that are worth saying to the user: a \
             write that restores the same bytes fires the watch, and a change that leaves the \
             modification time alone does not.\n\n\
             Use this where somebody asked to be told when a file changes. Use schedule_next \
             instead where what is being waited on is not one file, or where they asked for \
             their own line to be run again.\n\n\
             One file, which may not exist yet, never a directory and never a pattern, and the \
             path cannot be changed once the watch is armed. A watch lives at most a week, at \
             most 8 are live at once, and the user sees every live one: they end one with \
             /watch stop <n>, and ctrl-c ends them all.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative path of the one file to watch, e.g. \
                                        src/main.rs. It may have nothing at it yet, in which \
                                        case the watch reports it appearing."
                    }
                },
                "required": ["path"]
            }),
        ));
    }

    if !scheduling.offered() {
        return tools;
    }
    let arranging = scheduling == Scheduling::ArrangingALook;
    tools.push(Tool::function(
        "schedule_next",
        if !arranging {
            "Say when this loop should run again. The user started a loop with no interval, so \
                 each turn sets the pace for the next one. Call this once, at the end of the turn, \
                 after the work is done. You are choosing only the moment: the prompt is the \
                 user's own line and it is sent again unchanged, so there is nothing here to say \
                 what the next turn asks. Pick the wait from what you are actually waiting on, not \
                 from a round number: something that takes ten minutes to change is not worth \
                 looking at in one. Once there is nothing left to watch, call it with stop true \
                 instead of a wait, which ends the loop now. A turn that calls neither is woken \
                 once more after twenty minutes before the loop ends."
        } else {
            "Arrange the next look at something, when one turn cannot answer what was asked. \
                 A request to be told when something changes is the case this exists for: take the \
                 first look now, answer from it, and call this at the end of the turn to be asked \
                 again after the wait. You are choosing only the moment. The user's own line is \
                 what gets sent again, unchanged, so there is nothing here to say what the next \
                 turn asks, and that turn is asked in the same way when to look after it. Pick the \
                 wait from what you are waiting on rather than from a round number. Do not call it \
                 where this turn has already answered the question: looking again at something \
                 settled is a loop somebody has to notice and stop."
        },
        {
            let mut parameters = json!({
            "type": "object",
            "properties": {
                "delay_seconds": {
                    "type": "integer",
                    "description": "How long to wait before the next run. Held to between \
                                    1 and 3600 seconds, so anything outside that becomes \
                                    the nearest of the two. The wait starts when this turn \
                                    ends, so a whole turn separates two looks however short \
                                    it is."
                },
                "reason": {
                    "type": "string",
                    "description": "What you are waiting on, in a few words, e.g. \"watching \
                                    the release build\". The user reads this; it is not sent \
                                    anywhere and nothing acts on it."
                },
                "noop": {
                    "type": "boolean",
                    "description": "True when this run found nothing to do and changed \
                                    nothing, false when something happened worth keeping: an \
                                    edit, a message, a finding. Runs of quiet ticks are \
                                    counted, so the user can see the loop is healthy without \
                                    reading every one."
                }
            },
            "required": ["delay_seconds", "noop"]
            });
            // Only a tick of a self-paced loop can end the loop it belongs to, and it ends it
            // without a wait, so the wait stops being required beside it (SCHED-4).
            if !arranging {
                parameters["properties"]["stop"] = json!({
                    "type": "boolean",
                    "description": "True when the work this loop watches is finished and there \
                                    is nothing left to look at: the loop ends now, with no \
                                    further tick. delay_seconds is not needed with it. It can \
                                    only end this loop, never start or change one."
                });
                parameters["required"] = json!(["noop"]);
            }
            parameters
        },
    ));
    tools
}

/// Ask for the reason a call is made, on every tool alike.
///
/// Required, because a model asked for a reason in prose mostly sends calls without one, and a
/// person watching sees every call and otherwise nothing of what it was for. Content rather than
/// routing (TOOL-5): no tool reads it, so it is added here once rather than written into each
/// schema.
fn ask_why(tool: &mut Tool) {
    let parameters = &mut tool.function.parameters;
    parameters["properties"][WHY] = json!({
        "type": "string",
        "description": "One short line saying why you are making this call: what you want to \
                        find out, or what you are about to change. The user reads it beside the \
                        call, which already shows the tool and what it acts on, so give the \
                        reason rather than repeating those."
    });
    if let Some(required) = parameters["required"].as_array_mut() {
        required.push(json!(WHY));
    }
}

/// The tools a delegate of one kind is offered.
///
/// Filtered from the one table rather than written again, so a schema improved for the planner is
/// improved for a delegate on the same line. What a capability reaches is the rule, because a tool
/// offered to a run whose gates refuse it on every call is a tool the model has to be told to
/// ignore.
///
/// Five are left out by name, each for its own reason:
///
/// - `ask_user`, because a delegate's task came from a planner rather than from the person, so a
///   question about it asks somebody to arbitrate something they never set up.
/// - `todo_write`, because the list on the screen belongs to the turn the person is watching, and
///   a delegate writing to it would replace what they were reading with the steps of a sub-task
///   they did not ask about.
/// - `schedule_next`, because what it schedules is another turn of the session, sending the line
///   the person typed. A delegate was given a task by a planner and ends when it answers, so a
///   wait it asked for would either be dropped or would put the session back to work on something
///   nobody at the keyboard is waiting for.
/// - `vet_content`, because what crosses back from a delegate is its own set of rules and a
///   delegate has nobody watching it. The prompt it would draw belongs to the person who set the
///   sub-task going, about content they never asked to see, in the middle of work they are not
///   reading.
/// - `fetch_url`, because every kind holds the capability for reaching the network so the driver
///   can make its model call, and that is the whole of what it buys. A delegate pointing a
///   request at a host of its own would be egress nobody approved for this sub-task, and the
///   person shown the host would be answering for a task they never set. The capability being
///   held is what makes this one a name and not a capability check: no gate would refuse it.
///
/// `spawn_agent` is offered by where the delegate sits rather than by what it is: `delegating` is
/// the kinds it may start, and `None` is a delegate already [`MAX_DEPTH`] below the turn. The
/// kernel refuses one that asks there anyway, so this only spares the model a tool that cannot
/// work.
///
/// Two terms rather than one, and they are visible as two: the capability is the gate, and the
/// definition's list is a confinement inside it that can only ever remove a name. `None` is a
/// definition that named no tools, which is the kind's own set and is what every delegate had
/// before definitions existed.
///
/// [`MAX_DEPTH`]: bravebot_core::delegate::MAX_DEPTH
pub fn for_delegate(
    capabilities: &bravebot_core::capability::CapabilitySet,
    confined_to: Option<&[String]>,
    delegating: Option<&bravebot_core::delegate::Definitions>,
    deadlines: crate::exec::Deadlines,
) -> Vec<Tool> {
    use bravebot_core::delegate::{NEVER_DELEGATED, gating_capability};

    let offers = |name: &str| {
        if NEVER_DELEGATED.contains(&name) || !bravebot_core::tool_set::allows(name) {
            return false;
        }
        if name == "spawn_agent" && delegating.is_none() {
            return false;
        }
        if !gating_capability(name).is_some_and(|needs| capabilities.contains(&needs)) {
            return false;
        }
        // Applied after the capability rather than instead of it. A definition subtracts from
        // what its kind reaches and never adds, so a name here that the gate above dropped is a
        // name this delegate loaded without.
        confined_to.is_none_or(|named| named.iter().any(|tool| tool == name))
    };

    // Asked before the table is written as well as of each name in it, because a description that
    // says to use `run` is wrong on a list that does not hold one, and the question is the same
    // question either way round.
    let running = if offers("run") {
        Running::Offered
    } else {
        Running::Withheld
    };
    // NoLaterLook rather than ArrangingALook: a delegate is offered no `schedule_next` and nothing
    // asks it again, so the read and run descriptions say that instead of sending it to a tool its
    // list does not hold.
    let mut tools: Vec<Tool> = available(
        Scheduling::NoLaterLook,
        crate::watch::Arming::Unavailable,
        deadlines,
        running,
    )
    .into_iter()
    .filter(|tool| offers(tool.function.name.as_str()))
    .collect();
    if let Some(delegates) = delegating {
        offer_kinds(&mut tools, delegates);
    }
    tools
}

/// The tools a turn offers its planner, with the kinds of delegate this turn resolved.
///
/// The schema is otherwise the table's own, and this replaces one field of one tool: which names
/// `spawn_agent` accepts, and what each of them is for. Built here rather than threaded through
/// [`available`] because every other tool is the same whatever a person has written down.
///
/// `running` is the planner's own `Offered` on an ordinary turn, which holds the whole table. A turn
/// addressed to a definition is narrowed by a list read after this is built, so its caller asks
/// again with `Withheld` where that narrowing took `run` away.
pub fn for_planner(
    scheduling: Scheduling,
    arming: crate::watch::Arming,
    delegates: &bravebot_core::delegate::Definitions,
    deadlines: crate::exec::Deadlines,
    running: Running,
) -> Vec<Tool> {
    let mut tools = available(scheduling, arming, deadlines, running);
    offer_kinds(&mut tools, delegates);
    tools
}

/// Add the `advisor` tool to a planner's list, for a session that named an advisor model.
///
/// Built here rather than in the table because no session is offered it by default: it exists only
/// where somebody chose a model to consult, and a description naming a tool that is not on the
/// list sends the planner to a name that is not there.
pub fn offer_advisor(tools: &mut Vec<Tool>) {
    let mut advisor = Tool::function(
        "advisor",
        "Ask a stronger model for guidance. The advisor is given this whole conversation, \
         exactly as you have it, and the question you write; it has no tools and cannot look \
         at anything you have not. It returns advice as text, which is yours to weigh and act \
         on. Use it before committing to an approach you are unsure of, when you are stuck, or \
         before calling the work done. It costs a request to a larger model, and a turn may ask \
         it only a few times, so put the whole question in one call rather than several.",
        json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "What you want the advisor's view on, written so that it \
                                    can be answered from the conversation alone."
                }
            },
            "required": ["question"]
        }),
    );
    ask_why(&mut advisor);
    tools.push(advisor);
}

/// Add `load_tool` to the list of a turn whose server tools are offered by name only (SERVERS-16).
///
/// Built here rather than in the table because it exists only where tools are deferred, and a
/// description naming a tool that is not on the list sends the planner to a name that is not
/// there. `names` are the `alias:tool` of tools a person vouched for, so nothing a server wrote
/// decides what is listed.
pub fn offer_tool_loader(tools: &mut Vec<Tool>, names: &[String]) {
    let mut loader = Tool::function(
        "load_tool",
        format!(
            "Load a tool of an MCP server the user connected, so that it is offered to you from \
             your next request on. These tools exist but are not shown to you until you name one, \
             to keep this conversation short. Only the names listed here exist; there is nothing \
             to search. The user is still asked before each call to a loaded tool. The tools: {}.",
            names.join(", ")
        ),
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name of a tool exactly as it is listed here, written \
                                    alias:tool."
                }
            },
            "required": ["name"]
        }),
    );
    ask_why(&mut loader);
    tools.push(loader);
}

/// Say in the `run` description what its programs are held to, for a turn that confines them.
///
/// Appended to the table's description rather than a parameter of it, so the many callers that
/// build a table for another reason are untouched. Nothing where the list does not offer `run` or
/// the turn does not confine, since a planner is not told of a boundary its programs do not have.
pub fn state_confinement(
    tools: &mut [Tool],
    confine_runs: bool,
    sandbox: bravebot_sandbox::SandboxMode,
) {
    let Some(stated) = crate::confine::stated_to_the_planner(confine_runs, sandbox) else {
        return;
    };
    if let Some(run) = tools.iter_mut().find(|tool| tool.function.name == "run") {
        run.function.description.push(' ');
        run.function.description.push_str(&stated);
    }
}

/// Say in the `run` description which variables its programs are not handed (RUN-12).
///
/// Appended to the table's description for the reason [`state_confinement`] is. Nothing where the
/// list does not offer `run`, and nothing where the filtering is switched off: a planner is not told
/// of a removal that does not happen.
///
/// Only the names fixed in the source are listed, from [`bravebot_config::scrub::builtin_credentials`].
/// A name a settings file wrote is removed from the program and never said here: an entry in a
/// provider's `env` is a free-form string that may be a key pasted into the wrong field, and
/// `run.scrubEnv` takes names from a checkout's layer as well. The statement says such variables
/// exist, which is what lets a planner read an unset report from one of them correctly.
///
/// A fact about this machine and what follows from it, with no instruction. Without the fact a
/// program reporting one of these unset reads as a gap in the person's setup, and a planner reports
/// it as one.
pub fn state_withheld_variables(tools: &mut [Tool]) {
    state_withheld(tools, &bravebot_config::scrub::builtin_credentials());
}

/// [`state_withheld_variables`], for the names it is handed.
pub(super) fn state_withheld(tools: &mut [Tool], names: &[&str]) {
    if names.is_empty() {
        return;
    }
    if let Some(run) = tools.iter_mut().find(|tool| tool.function.name == "run") {
        run.function.description.push(' ');
        run.function
            .description
            .push_str(&withheld_variables_sentence(names));
    }
}

/// What [`state_withheld`] appends.
fn withheld_variables_sentence(names: &[&str]) -> String {
    let listed: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    format!(
        "The credentials this agent holds for itself ({}), and the variables the person's settings \
         name as a gateway's token or to be withheld, which are not listed, are removed from the \
         environment of every program this tool starts, whatever the person's shell holds. A program \
         that reports one of them unset, or fails for want of one, says nothing about whether the \
         person has set it.",
        listed.join(", ")
    )
}

/// Say in the `read_output` description who answers a read of what a run printed, for the session
/// as it stands when the request is built (RUN-14).
///
/// `answer` is the sentence for that case. It is appended to the table's description for the
/// reason [`state_confinement`] is, and nothing where the list does not offer `read_output`, so a
/// turn left without it is not pointed at a tool it has not got. Called on each request's copy of
/// the list and not on the list itself, so a mode the person changes in the middle of a turn is
/// what the next request says, and a statement is never appended twice.
pub fn state_read_output_answer(tools: &mut [Tool], answer: &str) {
    if let Some(read_output) = tools
        .iter_mut()
        .find(|tool| tool.function.name == "read_output")
    {
        read_output.function.description.push(' ');
        read_output.function.description.push_str(answer);
    }
}

/// Replace which names `spawn_agent` accepts, and what each is for, with the kinds this turn
/// resolved. Nothing where the list does not offer the tool.
fn offer_kinds(tools: &mut [Tool], delegates: &bravebot_core::delegate::Definitions) {
    let Some(spawn) = tools
        .iter_mut()
        .find(|tool| tool.function.name == "spawn_agent")
    else {
        return;
    };
    let Some(kind) = spawn
        .function
        .parameters
        .get_mut("properties")
        .and_then(|properties| properties.get_mut("kind"))
    else {
        return;
    };

    // Name and description together, because a name on its own says nothing about when to pick
    // it. Both are the definition's own words, from a file that passed the trusted-content gate
    // or from this program: a name nobody vouched for never entered the set. A checkout is said
    // beside them, since it decides which tree the delegate's report is about.
    let described = delegates
        .iter()
        .map(|definition| {
            let apart = match definition.asks_for_checkout() {
                true => {
                    " Its delegates work in a checkout of the last commit, which holds no change \
                     not yet committed."
                }
                false => "",
            };
            format!(
                "\n- {}: {}{apart}",
                definition.name(),
                definition.description()
            )
        })
        .collect::<String>();
    kind["description"] = json!(format!(
        "Which kind of agent. Pick the narrowest one that can do the job; they are listed \
         narrowest first.{described}"
    ));
    kind["enum"] = json!(delegates.names());
}
