//! The directories a session works in: opening, closing and changing them, the checkouts it keeps,
//! and the ones a settings file names.

use super::*;

/// The trail of `/checkouts remove`: one event where `named`, the checkout asked for as the list
/// had it, is no longer in `now`, and nothing where it was kept or never listed (CHECKOUT-19).
pub(super) fn removal_trail(
    named: Option<&bravebot_agent::workspace::SessionCheckout>,
    now: &[bravebot_agent::workspace::SessionCheckout],
) -> Trail {
    let mut trail = Trail::new();
    if let Some(gone) = named.filter(|gone| !now.iter().any(|listed| listed.id == gone.id)) {
        bravebot_agent::workspace::record_checkout(
            &mut trail,
            bravebot_agent::workspace::Happened::Removed,
            &gone.path,
        );
    }
    trail
}

/// Remove checkout `id` of those the session keeps, asking first where the record shows something
/// was done in it (CHECKOUT-15). `ask` puts the question to the person and `remove` removes it.
///
/// Asked on the record alone. A checkout's status is not read, so a program that changed a file in
/// it is known only as a program having run there, and that is enough to ask.
pub(super) fn remove_checkout(
    session: &mut Session,
    kept: Vec<bravebot_agent::workspace::SessionCheckout>,
    id: &str,
    ask: impl FnOnce(&bravebot_agent::workspace::SessionCheckout) -> crate::confirm::Answer,
    remove: impl FnOnce(&str) -> Result<(), bravebot_agent::workspace::Unremoved>,
) {
    use bravebot_agent::workspace::Unremoved;
    let Some(listed) = kept.into_iter().find(|listed| listed.id == id) else {
        session.note_refused(t!(checkouts_no_such, id = id));
        return;
    };
    if listed.worked_in && ask(&listed) != crate::confirm::Answer::Approve {
        session.note_refused(t!(checkouts_kept, id = id));
        return;
    }
    let path = listed.path.display().to_string();
    match remove(id) {
        Ok(()) => session.note_done(t!(checkouts_removed, id = id, path = &path)),
        Err(Unremoved::NoSuch) => session.note_refused(t!(checkouts_no_such, id = id)),
        Err(Unremoved::WorkedFrom) => {
            session.note_refused(t!(checkouts_worked_from, id = id, path = &path))
        }
        Err(Unremoved::Stuck) => {
            session.note_refused(t!(checkouts_not_removed, id = id, path = &path))
        }
    }
}

/// Open another directory and vouch for it, for the rest of this session.
///
/// Two things happen together, and both are needed. The workspace makes the directory reachable at
/// all, since an absolute path is refused otherwise. The trust map records that the user vouched
/// for it, which is what the write gates consult. Doing only the first would leave every write
/// there asking; doing only the second would leave a rule about files nothing can open.
///
/// The path recorded is the canonical one, not the name typed: `~/notes/../notes` and a symlink
/// both name a directory whose rules should be about where it actually is.
///
/// Session-scoped on purpose. `docs/specs/trust-map.md` is explicit that trust is not sticky per directory,
/// so a later session starts without this and is asked again. It does survive `--resume`, since
/// that restores the map its own user gave.
pub(super) fn add_directory(
    session: &mut Session,
    workspace: &mut Workspace,
    trust: &mut TrustStore,
    directory: &str,
) {
    if directory.is_empty() {
        session.note_refused(t!(session_add_dir_needs_a_path));
        return;
    }

    // The workspace refuses a name that is not absolute, and what `~` and a relative name mean is
    // the session's to say, so the name is made absolute here.
    let outcome = named_directory(workspace.root(), directory)
        .and_then(|named| workspace.add_directory(&named));
    session.set_reference_sources(workspace.reference_sources());
    match outcome {
        Ok(added) => {
            trust.trust(&bravebot_agent::workspace::key_of(&added));
            session.note_done(t!(
                session_directory_added,
                directory = added.display().to_string()
            ));
            if workspace.ends_checkouts(&added) {
                session.note(t!(
                    session_directory_ends_checkouts,
                    directory = added.display().to_string()
                ));
            }
        }
        Err(error) => session.note_refused(t!(
            session_directory_not_added,
            directory = directory,
            problem = error
        )),
    }
}

/// Close one directory `/add-dir` opened, and withdraw the trust it was opened with (TRUST-9).
///
/// Both halves go, because the person closing it is withdrawing the answer that allowed both: a
/// rule left behind would go on vouching for every file there that reaches a turn some other way,
/// and the session record would carry it into a resume. Says whether a directory closed, since
/// the record then has to be written again.
pub(super) fn close_directory(
    session: &mut Session,
    workspace: &mut Workspace,
    trust: &mut TrustStore,
    directory: &str,
) -> bool {
    if directory.is_empty() {
        session.note_refused(t!(session_close_dir_needs_a_path));
        return false;
    }
    let outcome = named_directory(workspace.root(), directory)
        .and_then(|named| workspace.close_added_directory(&named));
    session.set_reference_sources(workspace.reference_sources());
    match outcome {
        Ok(closed) => {
            let directory = closed.display().to_string();
            // Where no trusted rule went, the directory answers as it did before the close, so
            // saying trust was withdrawn would tell the person something that did not happen.
            if trust.withdraw_trust(&bravebot_agent::workspace::key_of(&closed)) {
                session.note_done(t!(session_directory_withdrawn, directory = directory));
            } else {
                session.note_done(t!(session_directory_closed, directory = directory));
            }
            true
        }
        Err(error) => {
            session.note_refused(t!(
                session_directory_not_closed,
                directory = directory,
                problem = error
            ));
            false
        }
    }
}

/// Work somewhere else from now on, and vouch for it, for the rest of this session.
///
/// The primary root is what a relative path means, where commands run, and where `AGENTS.md` and
/// the project's skills are looked for, so moving it moves the whole of what the session is about.
/// Says whether it moved, since the session record has to move with it.
///
/// Three things happen together, and the order matters. The workspace moves, closing whatever
/// overlapped the new root. The trust map is re-spelled against the new root, so every rule the
/// session already held goes on saying what it said about the same files: a no given inside the
/// new directory is still a no, and a yes given for the old one does not become a yes for this
/// one. Only then is the new directory vouched for, which is the one thing this grants, and it is
/// granted for the same reason `/add-dir` grants it: a person typed the path themselves, and a
/// later decision replaces an earlier one.
///
/// Session-scoped like everything else the map holds. A later session in the directory the user
/// started in is asked the opening question there, exactly as it would have been.
///
/// The permission rules are read again for the new root, PERM-15: a rule granted for the old
/// checkout's file is not a rule anybody granted here, and this checkout's own `deny` rules are in
/// force from the first turn. What it proposes is put to the person through `ask`.
///
/// The answers to the credential question are dropped, TRUST-13: one about a file inside the old
/// root is remembered under its name from there, and from the new one that name is another file.
pub(super) fn change_directory(
    session: &mut Session,
    workspace: &mut Workspace,
    answers: &mut Answers,
    id: &str,
    directory: &str,
    ask: impl FnOnce(&[bravebot_agent::granted::Proposed], &mut String) -> Option<bool>,
) -> Changed {
    if directory.is_empty() {
        session.note_refused(t!(session_cd_needs_a_path));
        return Changed::Stayed;
    }

    // Against where the session is now, which is what changing directory means everywhere else: a
    // command that only took an absolute path would refuse `/cd crates/tui` and `/cd ..`, which are
    // the two ways anybody would think to say it.
    let moved = match named_directory(workspace.root(), directory)
        .and_then(|named| workspace.change_root(&named))
    {
        Ok(moved) => moved,
        Err(problem) => {
            session.note_refused(t!(
                session_directory_not_changed,
                directory = directory,
                problem = problem
            ));
            return Changed::Stayed;
        }
    };

    let Answers {
        trust,
        // Kept: an entry names the canonical directory it was vouched in, which the move does not
        // change, so it covers that tree and no other from anywhere.
        programs: _,
        servers,
        // Kept: it grants nothing.
        asked_about: _,
        exposed,
        rules,
    } = answers;
    *exposed = bravebot_core::credentials::Exposed::new();
    *trust = trust.rebased(
        std::path::Path::new(&bravebot_agent::workspace::key_of(&moved.root)),
        bravebot_agent::workspace::volume_folds_case(&moved.root),
    );
    trust.trust(".");
    // The servers go, and LSP-5 is why: what a person approved was a server reading *that* tree,
    // and a set indexes the root it was built with. Kept across the move it would answer questions
    // about the new directory out of the old one's index, and resolve the paths it opens against a
    // root the session has left. Dropping it shuts the processes down, and the first question asked
    // here starts one for this directory with the person asked again.
    *servers = None;
    session.now_in_workspace(&moved.root);
    session.adopt_repository(crate::issue_link::Repository::of(&moved.root));
    session.set_reference_sources(workspace.reference_sources());
    // An MCP server is not a tree's, so it stays; answer 2 is about a project, so it moves.
    if let Some(mcp) = &session.servers.session {
        mcp.now_in_workspace(&moved.root);
    }
    session.note_done(t!(
        session_directory_changed,
        directory = moved.root.display().to_string()
    ));
    // Said one directory at a time, because each one is something the session could read a moment
    // ago and now cannot. A person is entitled to know which, and to open it again by name.
    for closed in &moved.closed {
        session.note(t!(
            session_directory_closed,
            directory = closed.display().to_string()
        ));
    }
    // And the watches armed in the directory left behind, for the reason the servers go: what a
    // person agreed to was a standing report about a file in *that* tree. Ended here rather than
    // left to the next look, which is up to five seconds away and would let a watch that has
    // already seen a change fire about a path this directory resolves elsewhere.
    session.end_watches_left_behind(&moved.root);
    // Asked, since the person who typed the path is here to answer, unless the mode in force answers
    // every question. Never resumed: no record holds an answer for a directory the session was not in.
    let whence = match crate::trust_prompt::answered_by(session.permission_mode(), &moved.root) {
        Some(_) => Whence::Unasked,
        None => Whence::Asked,
    };
    if rules.read_for(session, &moved.root, whence, id, ask) {
        Changed::Moved
    } else {
        Changed::Left
    }
}

/// What `/cd` did.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Changed {
    /// Nothing, because the path named nowhere the session could go.
    Stayed,
    /// Moved, under the new directory's rules.
    Moved,
    /// Moved, and the person asked to leave at the new directory's rules question.
    Left,
}

/// The name a person or a settings file gave a directory, as an absolute path.
///
/// A leading `~` is the home directory first, and then a relative name means a path under the
/// working directory `root`. That order keeps `~/notes` the home directory and not a directory
/// called `~` under the project, and it is the order `/cd` and a settings file's names are read in.
/// `../shared` in a settings file says the directory it wants is next to the project rather than
/// next to wherever the agent happened to be started from, and `/add-dir`, `/add-dir close` and
/// `/cd` take a name against the working directory the session is in the same way.
///
/// The leading `.` and `..` of a relative name are cleaned against `root` here, and the remaining
/// components are appended in order. The path of `root` is canonical, so it holds no link for a
/// `..` to step over, and `../notes` becomes `<parent of root>/notes`, which is the spelling an
/// opened directory is recorded under and so the one `/add-dir close` matches before it asks the
/// file system where the name leads. A `..` after a name is left alone, since that name may be a
/// link and the file system takes the `..` from the link's target (TRUST-10).
///
/// A relative name against a `root` that is not text is refused. The `String` this returns becomes
/// a reach grant and a trust key, and a rendering of `root` that replaces the bytes it cannot show
/// names another directory (PATH-002). A name that is already absolute needs no `root`.
pub(super) fn named_directory(
    root: &std::path::Path,
    typed: &str,
) -> Result<String, bravebot_agent::WorkspaceError> {
    use std::path::Component;

    let expanded = expand_home(typed);
    if std::path::Path::new(&expanded).is_absolute() {
        return Ok(expanded);
    }
    let not_text = || bravebot_agent::WorkspaceError::Invalid {
        path: typed.to_string(),
        reason: t!(session_directory_root_not_text),
    };
    if root.to_str().is_none() {
        return Err(not_text());
    }

    let mut resolved = root.to_path_buf();
    let mut components = std::path::Path::new(&expanded).components().peekable();
    while let Some(leading) = components.peek() {
        match leading {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            _ => break,
        }
        components.next();
    }
    resolved.extend(components);
    resolved
        .into_os_string()
        .into_string()
        .map_err(|_| not_text())
}

/// Replace a leading `~` with the user's home directory.
///
/// Only a leading one, and only when it is the whole first segment, so a directory genuinely called
/// `~notes` is left alone. Without a home to expand to, the path is passed through as it was typed,
/// and the caller reads it as a name under the working directory.
///
/// The directory comes from [`bravebot_agent::home::profile`] rather than from `HOME` read here,
/// so that a `~` somebody types and a `~` the planner writes in a command line name the same file
/// (CMDLINE-4). Two readings of the environment are two answers waiting to differ.
pub(super) fn expand_home(directory: &str) -> String {
    let Some(rest) = directory.strip_prefix('~') else {
        return directory.to_string();
    };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return directory.to_string();
    }
    match bravebot_agent::home::profile() {
        Some(home) => format!("{}{rest}", home.display()),
        None => directory.to_string(),
    }
}

/// What becomes of the directories a settings file named.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Named {
    /// Opened with nothing put to anybody.
    Opening(Vec<String>),
    /// Put to the person, one question per directory.
    Asking(Vec<String>),
}

/// What becomes of the directories a settings file named, for a session that opened this way.
///
/// A file names them and a person grants them, so whoever answered for the working directory
/// answers for these: a person who was asked about it is asked about each of these, and the mode
/// that answers every question answers these too, on the terms it answers that one.
///
/// A resumed session opens none of them. Nothing is put to it, and the directories it has open are
/// the ones its own record reopened, so a name in a file that has been edited since is not a grant
/// that session's user made.
pub(super) fn named_directories(whence: Whence, named: &[String]) -> Named {
    match whence {
        Whence::Asked => Named::Asking(named.to_vec()),
        Whence::Unasked => Named::Opening(named.to_vec()),
        // Nothing is resolved for this one either: a name is a path on the filesystem, and
        // reading the disk to decide about directories this session will not open is work done
        // for a question nobody is being asked.
        Whence::Resumed => Named::Opening(Vec::new()),
    }
}

/// The directories the names in a settings file would open, in the order they were named.
///
/// Resolved before anything is asked, because what a person is shown has to be what they would be
/// granting: a name is canonicalized when it is opened, so `shared` pointing somewhere else opens
/// the tree at the other end of it, and a box showing the spelling would collect an answer to a
/// different question. A name that cannot be opened at all is said so rather than asked about, and
/// a path two layers both named is one request rather than the same question twice.
pub(super) fn named_to_open(
    session: &mut Session,
    workspace: &Workspace,
    named: &[String],
) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for directory in named {
        match named_directory(workspace.root(), directory)
            .and_then(|candidate| workspace.resolve_directory(&candidate))
        {
            Ok(path) => {
                let shown = path.display().to_string();
                if !paths.contains(&shown) {
                    paths.push(shown);
                }
            }
            Err(problem) => session.note_refused(t!(
                session_directory_not_added,
                directory = directory,
                problem = problem
            )),
        }
    }
    paths
}

/// Open and vouch for each directory that was accepted, and nothing else.
///
/// Separated so the other half can be tested where it bites: a name nobody accepted leaves its
/// path as unreachable and as unvouched for as any other outside the workspace.
pub(super) fn open_named(
    session: &mut Session,
    workspace: &mut Workspace,
    trust: &mut TrustStore,
    accepted: &[String],
) {
    for directory in accepted {
        add_directory(session, workspace, trust, directory);
    }
}
