//! What the planner is told of the programs `run` starts: the description, the notice of a
//! mode change and the sentences that end every profile line.
//!
//! Every function here is built from a sandbox mode, the platform, the network and the facts a
//! [`RunStatement`] carries. None takes a compiled step or a [`super::Confinement`], so no text a program
//! printed and no value the model supplied reaches a sentence.

use super::Askable;
use bravebot_sandbox::SandboxMode;
use bravebot_sandbox::base::Prelude;
use bravebot_sandbox::network::Network;
use std::borrow::Cow;

/// The facts the statements about a turn's programs are built from, passed by name so that the two
/// adjacent `bool`s cannot be swapped.
///
/// `confine_runs` says the turn asks for its programs to be confined; a turn that does not is
/// never told of a boundary.
///
/// `requests_offered` says the session would accept a `scopes` argument and a `request_path` call.
/// Where it would not, the description names neither, does not tell the planner on macOS to ask for
/// `loopback`, and says which routes the person has instead (SANDBOX-19).
///
/// `request_path_listed` says the turn's list holds `request_path`. Where it does not, no sentence
/// names the tool and each says what the person can do instead, or nothing (TOOL-6). `askable` says
/// whether anybody can be asked in this run (CLI-1), and where nobody can, no sentence names a
/// route only a person takes (SANDBOX-19).
///
/// The default is a turn whose list holds `request_path` and where somebody can be asked, with
/// neither confinement nor requests; a caller sets `confine_runs` and `requests_offered`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunStatement {
    pub confine_runs: bool,
    pub requests_offered: bool,
    pub request_path_listed: bool,
    pub askable: Askable,
}

impl Default for RunStatement {
    fn default() -> Self {
        Self {
            confine_runs: false,
            requests_offered: false,
            request_path_listed: true,
            askable: Askable::Somebody,
        }
    }
}

/// [`stated_to_the_planner_for`] for a turn whose list holds `request_path` and where somebody can
/// be asked.
#[cfg(test)]
pub fn stated_to_the_planner(
    confine_runs: bool,
    mode: SandboxMode,
    requests_offered: bool,
) -> Option<String> {
    stated_to_the_planner_for(
        mode,
        RunStatement {
            confine_runs,
            requests_offered,
            ..RunStatement::default()
        },
    )
}

/// What the planner is told about the programs `run` starts, for a turn that asks for them to be
/// confined, or `None` for one that does not.
///
/// A turn that does not confine returns `None`, so a planner is never told of a boundary its
/// programs do not have. Where the platform has no base to confine on, it is told the opposite.
///
/// The `off` mode returns `None` as well, for the same reason: the programs have no boundary to
/// state. [`stated_in_the_description_for`] adds the one sentence those two cases do carry.
///
/// What `facts` says is described on [`RunStatement`].
pub fn stated_to_the_planner_for(mode: SandboxMode, facts: RunStatement) -> Option<String> {
    let said = stated_listing(
        Prelude::current(),
        bravebot_config::run_network(),
        mode,
        facts,
    )?;
    Some(with_temporary_held_back_said(
        said,
        Prelude::current(),
        bravebot_config::holds_back_temporary_directory(),
    ))
}

/// What the `run` description says of its programs: [`stated_to_the_planner_for`], or for a turn
/// that does not confine its runs and for the mode `off` the one sentence that `request_path` is
/// refused and nobody is asked (SANDBOX-19).
///
/// The sentence is the same in both cases, in every permission mode and whether or not the
/// workspace is trusted, because the tool is refused in all of them. It says nothing of a boundary,
/// of the file tools, of `scopes` or of `unconfined`: the first two do not apply to programs that
/// start with no profile, and the last two are accepted and ignored there (SANDBOX-26). It is built
/// from the two arguments and nothing a program printed or a file held.
///
/// A turn whose list lacks `request_path` (`facts.request_path_listed`) is told nothing of it,
/// since a tool it cannot call needs no refusal: it gets no sentence, and a mode that confines gets
/// the text without the sentences that name the tool.
///
/// Where nobody can be asked (`Askable::Nobody`, CLI-1), the description names neither trusting the
/// workspace nor `/add-dir`, the same fact the sentence after a failed step reads (SANDBOX-19).
pub fn stated_in_the_description_for(mode: SandboxMode, facts: RunStatement) -> Option<String> {
    match !facts.confine_runs || mode == SandboxMode::Off {
        true => facts
            .request_path_listed
            .then(|| REQUEST_PATH_REFUSED_SENTENCE.to_string()),
        false => stated_to_the_planner_for(mode, facts),
    }
}

/// Said in the `run` description where the programs start with no profile, which is the mode `off`
/// and a turn that does not confine its runs. A `request_path` call is refused there before anyone
/// is asked (PATHREQ-5), so naming the tool saves the planner a call.
pub(super) const REQUEST_PATH_REFUSED_SENTENCE: &str =
    "The `request_path` tool is refused in this session and nobody is asked, so do not call it.";

/// What the planner is told when the person changes the sandbox mode while its turn runs, or `None`
/// where there is nothing to say (SANDBOX-22).
///
/// The `run` description the turn began with still states the mode it began in, so this says the
/// new one. A mode that confines is given the sentences the description would have carried under
/// it. `off` is given the fact that the boundary has ended, because the description the planner
/// has says nothing of that, and the fact that `request_path`, which that description may have
/// named, is now refused. A turn that confines none of its runs was never told of a boundary, so it
/// is told nothing.
///
/// `facts.request_path_listed` says the turn's list holds `request_path`. Where it does not, the
/// notice for `off` does not say the tool is now refused, and a confining mode is stated without
/// it. A run where nobody can be asked (`Askable::Nobody`, CLI-1) is stated as
/// [`stated_in_the_description_for`] states it.
///
/// Built from the two modes and the platform alone, so it carries nothing a program printed or a
/// file held.
pub fn change_notice_for(
    from: SandboxMode,
    to: SandboxMode,
    facts: RunStatement,
) -> Option<String> {
    if !facts.confine_runs || from == to {
        return None;
    }
    let said = match to {
        SandboxMode::Off => format!(
            " Programs `run` starts from now on start with no sandbox profile, so the limits its \
             description states do not apply to them.{}",
            if facts.request_path_listed {
                " The `request_path` tool is now refused and nobody is asked."
            } else {
                ""
            },
        ),
        mode => stated_to_the_planner_for(mode, facts)
            .map(|stated| {
                format!(
                    " It applies to the programs `run` starts from now on, in place of anything \
                     the `run` description says of them. {stated}"
                )
            })
            .unwrap_or_default(),
    };
    Some(format!(
        "The user changed the sandbox mode to `{}` while this turn was running.{said} A program \
         already started keeps the profile it started under.",
        to.name()
    ))
}

/// `said` with the sentence that the person holds the temporary directory back, where a platform
/// confines, the person did and the setting does what the sentence says.
///
/// On Windows a session with a directory of its own is given a `temp` directory inside it in place
/// of the account's temporary directory, and the setting changes nothing for that, so the sentence
/// would be untrue and nothing is said. A Windows session with no directory of its own does have
/// the account's directory held back and is not told so, which is less than is true.
pub(super) fn with_temporary_held_back_said(
    said: String,
    prelude: Option<Prelude>,
    held_back: bool,
) -> String {
    match prelude {
        Some(prelude) if held_back && prelude != Prelude::Windows => {
            format!("{said}{TEMPORARY_DIRECTORY_HELD_BACK_SENTENCE}")
        }
        _ => said,
    }
}

/// Said after the description where the person holds the temporary directory back, which makes
/// the sentence above that names it as written untrue (SANDBOX-22).
const TEMPORARY_DIRECTORY_HELD_BACK_SENTENCE: &str = " The person holds the temporary directory \
     back in this session: it is read and not written, so a program that makes a scratch file \
     there (a shell heredoc, `mktemp`, a compiler's intermediate files) is refused.";

/// Said at the end of the profile line after a failed step where the turn holds `request_path` back
/// for `load_tool` and the session would accept a request (TOOL-7). The tool is not on the list, so
/// no other sentence names it (TOOL-6), and this is the situation it is for.
pub(super) const REQUEST_PATH_LOADABLE_SENTENCE: &str = " A path outside the workspace that a \
     command needs is asked for with the `request_path` tool: call `load_tool` with the name \
     request_path first, then call it. The person is asked, and a yes lasts for this session.";

/// Fixed text that ends every profile line, so the line is the same for every failure.
pub(super) const CREDENTIAL_LOCATIONS_SENTENCE: &str = " A credential location such as `~/.ssh`, `~/.aws` \
     or `~/.kube` cannot be added with `/add-dir` or `--add-dir`. Where the mode accepts a \
     request, a credential scope is how a line reaches one. Any other path a command needs is \
     asked for with the `request_path` tool: the person is asked, and a yes lasts for this \
     session.";

/// [`CREDENTIAL_LOCATIONS_SENTENCE`] under `read-only`, which refuses a request to write a path
/// before anyone is asked (SANDBOX-22).
pub(super) const READ_ONLY_CREDENTIAL_LOCATIONS_SENTENCE: &str = " A credential location such as `~/.ssh`, \
     `~/.aws` or `~/.kube` cannot be added with `/add-dir` or `--add-dir`. Where the mode accepts \
     a request, a credential scope is how a line reaches one. Any other path a command needs to \
     read is asked for with the `request_path` tool: the person is asked, and a yes lasts for this \
     session. A request for a path to write is refused in this mode before anyone is asked.";

/// The fixed text of [`CREDENTIAL_LOCATIONS_SENTENCE`] up to the sentence that names `request_path`,
/// which is all of it for a turn whose list lacks that tool, under either mode.
const CREDENTIAL_LOCATIONS_WITHOUT_REQUEST_PATH_SENTENCE: &str = " A credential location such as \
     `~/.ssh`, `~/.aws` or `~/.kube` cannot be added with `/add-dir` or `--add-dir`. Where the \
     mode accepts a request, a credential scope is how a line reaches one.";

/// Said in place of the sentences that name the `scopes` menu and `request_path` where the session
/// would accept neither (SANDBOX-19, PATHREQ-5). Names the routes the person has instead, and no
/// argument the session would turn down.
///
/// Says the workspace is not trusted because that is the only fact left to refuse on where this is
/// reached: a description or a failure sentence is written only for a turn that confines its runs
/// in a mode other than `off`. Under `read-only` a directory is added to read, and a request to
/// read a path is the one a trusted workspace accepts (SANDBOX-22).
///
/// `request_path_listed` says the turn's list holds the tool, which is named only then.
/// `askable` says whether anybody can be asked in this run (CLI-1). Where nobody can, neither
/// trusting the workspace nor `/add-dir` is named, since nobody is there to do either.
pub(super) fn requests_refused_sentence(
    mode: SandboxMode,
    request_path_listed: bool,
    askable: Askable,
) -> String {
    let (directory, request) = match mode {
        SandboxMode::ReadOnly => ("a directory to read", "a request to read a path"),
        _ => ("a directory", "a request"),
    };
    let refused = match request_path_listed {
        true => " and the `request_path` tool is refused, and nobody is asked",
        false => "",
    };
    if askable == Askable::Nobody {
        return format!("{NOBODY_REFUSES_SCOPES}{refused}{NOBODY_WIDENS_TAIL}");
    }
    format!(
        " This workspace is not trusted, so `run` accepts no `scopes` argument{refused}. \
         Say which path or credential the command needs and what for: the person can trust the \
         workspace, so that {request} is accepted, or add {directory} with `/add-dir` or \
         `--add-dir`."
    )
}

/// Opens the refusal of [`requests_refused_sentence`] where nobody can be asked in this run (CLI-1),
/// ahead of the clause naming `request_path` where the list holds it. It gives no reason, since the
/// workspace being untrusted and nobody being there to answer a request are each enough.
const NOBODY_REFUSES_SCOPES: &str = " `run` accepts no `scopes` argument in this run";

/// Closes [`NOBODY_REFUSES_SCOPES`].
const NOBODY_WIDENS_TAIL: &str = ". Nobody can be asked in this run, so nothing in it widens \
     what a step reaches: say in the reply which path or credential the command needs and what \
     for.";

/// The first sentence of [`CREDENTIAL_LOCATIONS_SENTENCE`], which holds whether or not a request is
/// accepted.
const CREDENTIAL_LOCATIONS_NOT_ADDABLE_SENTENCE: &str = " A credential location such as `~/.ssh`, \
     `~/.aws` or `~/.kube` cannot be added with `/add-dir` or `--add-dir`.";

/// The fixed text that ends every profile line under `mode`, where the session would accept a
/// request (`requests_offered`) or would not, and where the turn's list holds `request_path`
/// (`request_path_listed`) or does not.
pub(super) fn credential_locations_sentence(
    mode: SandboxMode,
    requests_offered: bool,
    request_path_listed: bool,
    askable: Askable,
) -> Cow<'static, str> {
    match (requests_offered, mode) {
        (false, _) => Cow::Owned(format!(
            "{CREDENTIAL_LOCATIONS_NOT_ADDABLE_SENTENCE}{}",
            requests_refused_sentence(mode, request_path_listed, askable)
        )),
        (true, _) if !request_path_listed => {
            Cow::Borrowed(CREDENTIAL_LOCATIONS_WITHOUT_REQUEST_PATH_SENTENCE)
        }
        (true, SandboxMode::ReadOnly) => Cow::Borrowed(READ_ONLY_CREDENTIAL_LOCATIONS_SENTENCE),
        (true, _) => Cow::Borrowed(CREDENTIAL_LOCATIONS_SENTENCE),
    }
}

/// Said to the planner on macOS, whose profile refuses a listening socket that no line asked for,
/// in the description where the session accepts a `scopes` argument.
pub(super) const MACOS_LISTENING_SENTENCE: &str = " On macOS a program cannot listen on a port of this \
     machine unless its line asks for `loopback`, so `cargo test` on a crate whose tests bind \
     `127.0.0.1:0`, and any build through `sccache`, fail with `Operation not permitted` without \
     it.";

/// Said to the planner on macOS, whose profile refuses a write to a `.git` from every step whose
/// program is not git (SANDBOX-14), in the description and after a failed step alike.
pub(super) const MACOS_GIT_SENTENCE: &str = " On macOS a step whose program is not `git` cannot write any \
     `.git`: a script, `make`, `npm`, `gh` or any other program that runs git to commit, check out \
     or clone is refused that write, so run git as a step of its own.";

/// [`MACOS_GIT_SENTENCE`] under `read-only`, where a git step cannot write the session's `.git`
/// either, so running git on its own helps only in a place the mode writes.
pub(super) const MACOS_READ_ONLY_GIT_SENTENCE: &str = " On macOS a step whose program is not `git` cannot \
     write a `.git` even where this mode lets it write, as in a clone in the temporary directory: \
     a script, `make`, `npm`, `gh` or any other program that runs git to commit, check out or clone \
     there is refused that write, so there git is run as a step of its own.";

/// Said to the planner under `read-only`, in the description and after a failed step
/// (SANDBOX-19, SANDBOX-22).
pub(super) const READ_ONLY_COMMIT_SENTENCE: &str = " Outside the paths the person's own write grants name, a \
     build, a formatter or `git commit` that writes the directories the session was opened on \
     fails in this mode whichever step runs it, git on its own included.";

/// What the planner is told of a build or a commit in the session's directories under `mode`.
pub(super) fn commit_sentence(mode: SandboxMode) -> &'static str {
    match mode {
        SandboxMode::ReadOnly => READ_ONLY_COMMIT_SENTENCE,
        _ => "",
    }
}

/// What the planner is told on macOS of writing a `.git` under `mode`.
pub(super) fn macos_git_sentence(mode: SandboxMode) -> &'static str {
    match mode {
        SandboxMode::ReadOnly => MACOS_READ_ONLY_GIT_SENTENCE,
        _ => MACOS_GIT_SENTENCE,
    }
}

/// Said on macOS where the session's network is open: a plain line in a directory the person trusted
/// at the startup question may start without a prompt, and then with no network access (RUN-5,
/// SANDBOX-20). Fixed text, whatever the settings say about which directory that is.
pub(super) const TRUSTED_DIRECTORY_LINE_SENTENCE: &str = " In a directory the person trusted at the startup \
     question, a plain line may start without a prompt, and then it has no network access: a \
     connection refused or a host that does not resolve for a program that is not a package \
     manager's fetch, `git` or `gh` with a remote operation, `curl`, `ssh` or a command that names \
     a remote credential scope was stopped by the sandbox, and a line that needs the network is \
     put to the person when it runs with no sandbox.";

/// Said after the closed network on macOS, where loopback is also how a line connects to a port of
/// this machine, in the description where the session accepts a `scopes` argument.
pub(super) const MACOS_CLOSED_LOOPBACK_SENTENCE: &str =
    " A line that asks for `loopback` may still connect to a port of this machine.";

/// When a macOS line needs `loopback` to connect to a port of this machine, in the `run`
/// description's `scopes` entry and in the confinement paragraph of the description. Every other
/// text that says when it is needed to connect uses these words, so the planner reads one rule.
///
/// True under every network setting and host list, which the description does not know: the
/// profile's egress already reaches a local port, and a stage held to a host list's proxy gets
/// listening from `loopback` and no port to connect to.
pub(crate) const LOOPBACK_CONNECT_RULE: &str = "Ask for `loopback` to connect to a port of this \
     machine only from a line that has no network access: it adds nothing to a connect from a \
     line that has network access, so do not ask for it there.";

/// What a step that asked for `loopback` could do, said in the paragraph after a failed step, or
/// `None` where the platform does nothing with the request, as Linux and Windows do.
///
/// Chosen from the platform, the network in force and whether a host list holds the stages that
/// have network access, which the paragraph knows and the description does not. It names listening
/// wherever the platform acts on the request. It names connecting where the request is how a step
/// connects, which is a step with no network access. With a closed network and no host list
/// every step connects. With a closed network and a host list a step that has network access is
/// held to the proxy's port and a step that has none is not held to a proxy, so the connecting is
/// said of a step with no network access. With an open network no step has none: without a host
/// list every step connects without the request, and with one every step is held to the proxy's
/// port, so it says listening alone ([`LOOPBACK_CONNECT_RULE`]).
pub(super) fn loopback_reach(
    prelude: Prelude,
    network: Network,
    hosts_set: bool,
) -> Option<&'static str> {
    match (prelude, network, hosts_set) {
        (Prelude::MacOs, Network::Closed, false) => Some(
            "a step that asked for `loopback` could listen on and connect to this machine's own \
             ports",
        ),
        (Prelude::MacOs, Network::Closed, true) => Some(
            "a step that asked for `loopback` could listen on this machine's own ports, and \
             connect to them if it has no network access",
        ),
        (Prelude::MacOs, Network::Open, _) => {
            Some("a step that asked for `loopback` could listen on this machine's own ports")
        }
        (Prelude::Linux | Prelude::Windows, _, _) => None,
    }
}

/// The sentence that names the menu, for the planner, and the person is asked about every time.
pub(super) fn requests_menu_sentence(mode: SandboxMode, request_path_listed: bool) -> String {
    let paths = match (mode, request_path_listed) {
        (_, false) => "",
        (SandboxMode::ReadOnly, true) => {
            " A path outside the workspace that a command needs to read is asked for with the \
             `request_path` tool instead, and a yes lasts for the session; a request for a path to \
             write is refused in this mode before anyone is asked."
        }
        (_, true) => {
            " A path outside the workspace that a command needs is asked for with the \
             `request_path` tool instead, and a yes lasts for the session."
        }
    };
    format!(
        "A line that needs more can ask for it with `run`'s `scopes` argument, by name from this \
         list: {}. The person is asked about the line every time, and no answer to it is \
         remembered.{paths}",
        bravebot_sandbox::scope::Requested::MENU.join(", ")
    )
}

/// [`stated_listing`] for a turn whose list holds `request_path`, which is the text the tests
/// below were written against.
#[cfg(test)]
pub(super) fn stated(
    confine_runs: bool,
    prelude: Option<Prelude>,
    network: Network,
    mode: SandboxMode,
    requests_offered: bool,
) -> Option<String> {
    stated_listing(
        prelude,
        network,
        mode,
        RunStatement {
            confine_runs,
            requests_offered,
            ..RunStatement::default()
        },
    )
}

/// The clauses of the opening sentences of [`stated_listing`] that name a route only a person
/// takes. [`widens`] says [`NOBODY_WIDENS_SENTENCE`] in place of each where nobody can be asked in
/// this run (CLI-1).
const READ_ONLY_PERSON_WIDENS_CLAUSE: &str = "Only the person widens it: `/add-dir` and \
     `--add-dir` add a directory to read, and choosing another sandbox mode lets programs write the \
     project.";

/// [`READ_ONLY_PERSON_WIDENS_CLAUSE`] for a mode in which a command may ask for nothing more.
const STRICT_PERSON_WIDENS_CLAUSE: &str =
    "Only the person widens it (`/add-dir`, `--add-dir`); a command cannot ask for more.";

/// [`READ_ONLY_PERSON_WIDENS_CLAUSE`] for `workspace-write` on Linux and macOS.
const PERSON_WIDENS_CLAUSE: &str = "The person widens it (`/add-dir`, `--add-dir`).";

/// Said in place of each person-widens clause where nobody can be asked in this run.
const NOBODY_WIDENS_SENTENCE: &str =
    "Nobody can be asked in this run, so nothing in it widens what a step reaches.";

/// `clause`, or [`NOBODY_WIDENS_SENTENCE`] where nobody can be asked in this run.
fn widens(clause: &'static str, askable: Askable) -> &'static str {
    match askable {
        Askable::Nobody => NOBODY_WIDENS_SENTENCE,
        Askable::Somebody => clause,
    }
}

/// The statement of a turn's programs for a turn whose list holds `request_path`
/// (`facts.request_path_listed`) or does not, in a run where nobody can be asked
/// (`Askable::Nobody`) or somebody can.
pub(super) fn stated_listing(
    prelude: Option<Prelude>,
    network: Network,
    mode: SandboxMode,
    facts: RunStatement,
) -> Option<String> {
    let RunStatement {
        confine_runs,
        requests_offered,
        request_path_listed,
        askable,
    } = facts;
    if !confine_runs || mode == SandboxMode::Off {
        return None;
    }
    let mut said = match prelude {
        Some(Prelude::Windows) if mode == SandboxMode::ReadOnly => format!(
            "Programs this tool starts are confined and cannot write the project. Each may reach \
             only the directories the session was opened on and the scratch directory, read, the \
             temporary directory, the paths the person's own `allowWrite` entries and remembered \
             write grants name, read and written, the system and program directories and git's configuration files, \
             read, and the credential scope its command names. A path outside those, and a write \
             anywhere else, the directories the session was opened on included, is refused by the \
             operating system as `Operation not permitted` or `Permission denied`, so a program \
             that reports either for such a path was stopped by the sandbox and not by a fault in \
             the machine. {} The \
             file tools write nothing in this mode either, and a line that redirects output into \
             a file is refused.",
            widens(READ_ONLY_PERSON_WIDENS_CLAUSE, askable)
        ),
        Some(Prelude::Windows) => format!(
            "Programs this tool starts are confined. Each may reach only the directories the \
             session was opened on, the scratch directory and the temporary directory, all read \
             and written, the system and program directories and git's configuration files, \
             read, the caches of the toolchain it belongs to, and the credential scope its \
             command names. A path outside those is refused by the operating system as \
             `Operation not permitted` or `Permission denied`, so a program that reports either \
             for such a path was stopped by the sandbox and not by a fault in the machine. {}",
            widens(STRICT_PERSON_WIDENS_CLAUSE, askable)
        ),
        Some(Prelude::Linux | Prelude::MacOs) if mode == SandboxMode::Strict => format!(
            "Programs this tool starts are confined. Each may reach only the directories the \
             session was opened on, the scratch directory and the temporary directory, all read \
             and written, the system and program directories and git's configuration files, \
             read, the caches of the toolchain it belongs to, and the credential scope its \
             command names. A path outside those is refused by the operating system as \
             `Operation not permitted` or `Permission denied`, so a program that reports either \
             for such a path was stopped by the sandbox and not by a fault in the machine. {}",
            widens(STRICT_PERSON_WIDENS_CLAUSE, askable)
        ),
        Some(Prelude::Linux | Prelude::MacOs) if mode == SandboxMode::ReadOnly => format!(
            "Programs this tool starts are confined and cannot write the project. Each may read \
             this machine except the places that hold a credential: ssh private keys, cloud and \
             container logins, keychains, browser profiles and password stores. It may write the \
             temporary directory, the paths the person's own `allowWrite` entries and remembered \
             write grants name, and reads a credential directory only where the command's scope names it. Any other \
             write, the directories the session was opened on and the toolchain caches included, \
             and a path outside those it may reach, is refused by the operating system as \
             `Operation not permitted` or `Permission denied`, so a program that reports either \
             for such a path was stopped by the sandbox and not by a fault in the machine. {} \
             The file tools write \
             nothing in this mode either, and a line that redirects output into a file is \
             refused.",
            widens(READ_ONLY_PERSON_WIDENS_CLAUSE, askable)
        ),
        Some(Prelude::Linux | Prelude::MacOs) => format!(
            "Programs this tool starts are confined. Each may read this machine except the places \
             that hold a credential: ssh private keys, cloud and container logins, keychains, \
             browser profiles and password stores. It may write only the directories the session \
             was opened on, the scratch directory, the temporary directory and the toolchain \
             caches, and reads a credential directory only where the command's scope names it. \
             A path outside those is refused by the operating system as `Operation not \
             permitted` or `Permission denied`, so a program that reports either for such a path \
             was stopped by the sandbox and not by a fault in the machine. {}",
            widens(PERSON_WIDENS_CLAUSE, askable)
        ),
        None => String::from(
            "Programs this tool starts are not confined on this platform: they run with the \
             access of the person's own account.",
        ),
    };
    if prelude.is_some() {
        said.push_str(commit_sentence(mode));
        match requests_offered {
            true => {
                said.push(' ');
                said.push_str(&requests_menu_sentence(mode, request_path_listed));
            }
            false => said.push_str(&requests_refused_sentence(
                mode,
                request_path_listed,
                askable,
            )),
        }
    }
    // The three sentences that ask for `loopback` are said where `run` accepts a `scopes`
    // argument, and nowhere the description says it accepts none.
    if prelude == Some(Prelude::MacOs) {
        if requests_offered {
            said.push_str(MACOS_LISTENING_SENTENCE);
            said.push(' ');
            said.push_str(LOOPBACK_CONNECT_RULE);
        }
        said.push_str(macos_git_sentence(mode));
    }
    if prelude.is_some() && mode == SandboxMode::ReadOnly {
        said.push_str(
            " The network is closed in this mode whatever the settings say, and the remote \
             credential scope is not lent: `git push`, `gh pr create`, `gh issue comment`, \
             `curl` and `ssh` fail, and so does a package manager's fetch. A connection refused \
             or a host that does not resolve was stopped by the sandbox, so do not retry it; \
             choosing another sandbox mode lets a program reach a remote.",
        );
        if prelude == Some(Prelude::MacOs) && requests_offered {
            said.push_str(MACOS_CLOSED_LOOPBACK_SENTENCE);
        }
    } else if prelude.is_some() && network.is_closed() {
        said.push_str(
            " The network is closed: a program has no network access unless it is a package \
             manager's fetch, `git` or `gh` with a remote operation, `curl`, `ssh`, or a \
             command that names a remote credential scope. A connection refused or a host that \
             does not resolve for any other program was stopped by the sandbox.",
        );
        if prelude == Some(Prelude::MacOs) && requests_offered {
            said.push_str(MACOS_CLOSED_LOOPBACK_SENTENCE);
        }
    } else if prelude == Some(Prelude::MacOs) {
        said.push_str(TRUSTED_DIRECTORY_LINE_SENTENCE);
    }
    Some(said)
}
