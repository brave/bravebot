//! Opening a session and what it starts with: the rules proposed for it, the trust it keeps, and the
//! switches between sessions that begin another.

use super::*;

/// Where a session's opening trust map came from, which is what it says about it.
#[derive(Debug, Clone, Copy)]
pub(super) enum Whence {
    /// The person answered the startup question just now, or answered yes to it in an earlier
    /// session here and the yes was kept (TRUST-23). Either way they are here, and are asked about
    /// whatever else the tree proposes: what the yes kept was that question and no other.
    Asked,
    /// The record of the session being picked up, so the answer is that session's user's own.
    Resumed,
    /// Nobody was asked, because the mode in force answers this question too.
    Unasked,
}

/// The `allow` rules a checkout's settings files proposed, in the order the files wrote them.
///
/// A rule in one of those layers grants rather than narrows, so it is dropped where it is read
/// ([PERM-14]) and arrives here as a request instead. The order is the files' own, because that is
/// the order a person reads them in the box.
///
/// [PERM-14]: ../../docs/specs/permissions.md
pub(super) fn proposed_rules(
    settings: &bravebot_config::Settings,
) -> Vec<bravebot_agent::granted::Proposed> {
    // The rules among the dropped entries and not every dropped entry: one that is not a rule
    // decides nothing whoever writes it, so PERM-15 reports it under PERM-11 rather than offering
    // it, and the list this builds is both what the question offers and what the session reports
    // as dropped.
    bravebot_agent::permissions::proposed(settings, bravebot_agent::home::profile().as_deref())
}

/// What becomes of the `allow` rules a checkout proposed.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Granting {
    /// In force with nothing put to anybody, because something else answered.
    Granting(Vec<bravebot_agent::granted::Proposed>),
    /// The rules already granted for this workspace, and the rest put to the person as one question.
    Asking {
        already: Vec<bravebot_agent::granted::Proposed>,
        asking: Vec<bravebot_agent::granted::Proposed>,
    },
}

/// What becomes of those rules for a session that opened this way, given what the record holds.
///
/// Three ways a rule ends up in force, and they are the three [`Whence`] names:
///
/// - **Asked.** The person answered the workspace question just now, so they answer this one too,
///   bar the rules their own record already holds for this workspace, which they answered before and
///   are not asked about twice. A question already answered a box ago trains answering without
///   reading, which is [PERM-13]'s reasoning for a directory two layers both named.
/// - **Unasked.** The mode in force answers every question, so it answers this one, on the terms it
///   answers the workspace's. The record is not consulted and nothing is written to it: a grant
///   recorded for somebody who was never asked would be an answer on disk outliving the flag that
///   made it, and the mode grants these for the session it is in force for.
/// - **Resumed.** Nothing is put to it, and what holds is what that workspace's record already says,
///   which is [PERM-10]'s treatment of a resume that brought its own map. A rule the checkout has
///   added since is not one anybody granted, so it is not in force here either.
///
/// Separated from [`grant_proposed_rules`] so the decision can be made without a terminal, the way
/// [`named_directories`] is.
///
/// [PERM-10]: ../../docs/specs/permissions.md
/// [PERM-13]: ../../docs/specs/permissions.md
pub(super) fn granting_proposed_rules(
    whence: Whence,
    proposed: &[bravebot_agent::granted::Proposed],
    already: &[bravebot_agent::granted::Proposed],
) -> Granting {
    match whence {
        Whence::Resumed => Granting::Granting(already.to_vec()),
        Whence::Unasked => Granting::Granting(proposed.to_vec()),
        Whence::Asked => Granting::Asking {
            already: already.to_vec(),
            asking: proposed
                .iter()
                .filter(|rule| !already.contains(rule))
                .cloned()
                .collect(),
        },
    }
}

/// Which proposed rules this session installs, or `None` where the person asked to leave.
///
/// A rule is granted for the file that proposed it, so what comes back is the text alone: which
/// layer wrote it decided whether to ask, and by here that is answered. `ask` puts the question.
fn grant_proposed_rules(
    session: &mut Session,
    whence: Whence,
    grants: Option<&bravebot_agent::granted::Store>,
    proposed: &[bravebot_agent::granted::Proposed],
    id: &str,
    ask: impl FnOnce(&[bravebot_agent::granted::Proposed], &mut String) -> Option<bool>,
) -> Option<Vec<bravebot_agent::granted::Proposed>> {
    // Nothing proposed is nothing to ask about and no record to read (PERM-12), which is every
    // session in a checkout that carries no such rule.
    if proposed.is_empty() {
        return Some(Vec::new());
    }
    // No state directory, or one that cannot be read, is a record that says nothing, and a record
    // that says nothing is a session that asks. Nothing here may fail a run.
    let held: Vec<bravebot_agent::granted::Proposed> = match (whence, grants) {
        // Not consulted where nothing is being put to anybody: the mode grants the whole list, so
        // what the record holds cannot change the answer and reading it would be work for nothing.
        (Whence::Unasked, _) | (_, None) => Vec::new(),
        (_, Some(store)) => store.granted(proposed).into_iter().cloned().collect(),
    };

    let (already, asking) = match granting_proposed_rules(whence, proposed, &held) {
        Granting::Granting(granting) => (granting, Vec::new()),
        Granting::Asking { already, asking } => (already, asking),
    };

    // Said before the box rather than after it, so a person who granted these already is told which
    // rules are in force without being asked again, and where that answer is kept. Named one at a
    // time, for the reason a dropped rule is: whoever wrote it is looking for their own line and a
    // count tells them nothing.
    //
    // Read off the record rather than off what is in force, because the two differ in the mode that
    // answers every question: there the whole list holds and the record holds none of it, so a line
    // saying the person allowed these before would name a file that does not mention them.
    if let Some(store) = grants {
        for rule in &held {
            session.note(t!(
                session_permission_allow_granted_before,
                rule = bravebot_agent::confirm::shown_text(&rule.rule),
                path = bravebot_agent::confirm::shown_path(&rule.path),
                record = bravebot_agent::confirm::shown_path(store.path())
            ));
        }
    }

    let mut granted = already;
    if asking.is_empty() {
        return Some(granted);
    }
    // Carried for the reason the other two questions carry it: words another program typed while this
    // was up are not dropped, they go to the box where somebody can read them.
    let mut carried = String::new();
    let granting = ask(&asking, &mut carried);
    if !carried.is_empty() {
        session.paste_text(&carried);
    }
    if !granting? {
        return Some(granted);
    }
    if let Some(store) = grants {
        store.grant(&asking.iter().collect::<Vec<_>>(), id);
    }
    granted.extend(asking);
    Some(granted)
}

/// The rules in force for a workspace whose layers read as `settings`, or `None` where the person
/// asked to leave at the question [`grant_proposed_rules`] puts.
pub(super) fn rules_from(
    session: &mut Session,
    settings: &bravebot_config::Settings,
    root: &std::path::Path,
    grants: Option<&bravebot_agent::granted::Store>,
    whence: Whence,
    id: &str,
    ask: impl FnOnce(&[bravebot_agent::granted::Proposed], &mut String) -> Option<bool>,
) -> Option<Permissions> {
    // The `allow` rules a checkout proposed, which decide something only where this session's own
    // user grants them. Settled before the rules are built, because a granted one is a rule and the
    // list has to be complete before anything is parsed out of it.
    let proposed = proposed_rules(settings);
    let granted = grant_proposed_rules(session, whence, grants, &proposed, id, ask)?;
    let (permissions, rejected) = bravebot_agent::permissions::with_granted(
        settings,
        &granted
            .iter()
            .map(|rule| rule.rule.clone())
            .collect::<Vec<_>>(),
        bravebot_agent::home::profile().as_deref(),
        root,
    );
    // Said out loud, because a rule that parses as nothing is a rule somebody believes is in
    // force. A misspelled deny rule reads as protection that is not there.
    for problem in &rejected {
        session.note(t!(
            session_permission_rule_ignored,
            problem = bravebot_agent::permissions::describe(problem)
        ));
    }
    // And said out loud for the same reason: an `allow` entry a checkout wrote that nobody granted
    // is dropped, so the prompt it was meant to answer still appears. Somebody told nothing reads
    // that prompt as a second fault rather than as the rule not being in force. One this session
    // granted is not named here, because it is in force and there is nothing to explain.
    //
    // Matched on the file as well as the text, since two layers may write one rule and only one of
    // them be granted: a comparison on the text alone would report neither, leaving an entry that
    // answers nothing with nothing said about it.
    for rule in proposed.iter().filter(|rule| !granted.contains(rule)) {
        session.note(t!(
            session_permission_allow_ignored,
            rule = bravebot_agent::confirm::shown_text(&rule.rule),
            path = bravebot_agent::confirm::shown_path(&rule.path)
        ));
    }
    Some(permissions)
}

/// Where a workspace's rules are read from, bar the workspace itself.
///
/// The state directory holds the person's own settings layer and the record of what they granted,
/// and `named` is the file `--settings` named. Neither moves with the session, so rules read again
/// after `/cd` differ from the ones before it by the checkout's layers alone. `started` is the root
/// the session opened in, which keeps a named file inside it the checkout's wherever the session goes.
pub(super) struct RuleSources {
    pub(super) state: Option<std::path::PathBuf>,
    pub(super) named: Option<std::path::PathBuf>,
    pub(super) started: std::path::PathBuf,
    /// The administrator's file, whose `deny` and `ask` rules join every layer's own (PERM-19).
    pub(super) managed: bravebot_config::Managed,
}

impl RuleSources {
    /// The ones this process reads, for a session that opened in `started`.
    pub(super) fn ambient(started: &std::path::Path) -> Self {
        Self {
            state: bravebot_agent::home::directory(),
            named: bravebot_config::named_settings_file().map(std::path::Path::to_path_buf),
            started: started.to_path_buf(),
            managed: bravebot_config::Managed::load(),
        }
    }

    /// Every layer in force for a session in `root`.
    ///
    /// The root rather than the process's own directory, which `/cd` does not change.
    pub(super) fn settings(&self, root: &std::path::Path) -> bravebot_config::Settings {
        bravebot_config::Settings::layered_since(
            self.state.clone(),
            Some(root),
            self.named.as_deref(),
            Some(&self.started),
        )
        .under(&self.managed)
    }

    /// The record of what was granted in `root`.
    pub(super) fn grants(&self, root: &std::path::Path) -> Option<bravebot_agent::granted::Store> {
        self.state
            .as_deref()
            .map(|state| bravebot_agent::granted::Store::new(state, root))
    }
}

/// Whether a layer in force, or the administrator's pinned file, wrote
/// `permissions.bypassUnreachable` (PERM-17).
pub(super) fn bypass_is_unreachable(settings: &bravebot_config::Settings) -> bool {
    settings
        .narrowing()
        .strictest(bravebot_config::Managed::load().narrowing())
        .makes_bypass_unreachable()
}

/// The rules every turn is given, built from the settings of the workspace the session is in.
pub(super) struct Rules {
    pub(super) sources: RuleSources,
    pub(super) permissions: Permissions,
}

impl Rules {
    /// Replace these with the rules for `root`, or leave them and say `false` where the person asked
    /// to leave at the question.
    pub(super) fn read_for(
        &mut self,
        session: &mut Session,
        root: &std::path::Path,
        whence: Whence,
        id: &str,
        ask: impl FnOnce(&[bravebot_agent::granted::Proposed], &mut String) -> Option<bool>,
    ) -> bool {
        let settings = self.sources.settings(root);
        // Before the person is asked anything, so a checkout that forbids bypassing holds whatever
        // they answer about its rules.
        if bypass_is_unreachable(&settings) {
            session.make_bypass_unreachable();
        }
        let grants = self.sources.grants(root);
        let Some(permissions) =
            rules_from(session, &settings, root, grants.as_ref(), whence, id, ask)
        else {
            return false;
        };
        self.permissions = permissions;
        true
    }
}

/// The answers the event loop itself keeps past the prompt that gave them, with the rules it reads
/// for the root it is in. `/cd`, `/clear`, `/branch`, `/undo` and `/rewind` each take it apart with
/// no `..`, so a set added to [`bravebot_agent::answers::Answers`] does not build until each says
/// whether it is kept, reset, rebased or read again. The mode, vetting and the MCP servers are the
/// `Session`'s, and the directories `/add-dir` opened are the `Workspace`'s, so no pattern reaches
/// them.
pub(super) type Answers = bravebot_agent::answers::Answers<Rules>;

/// What a session brings to the startup question, which is all that decides whether it is put.
#[derive(Debug)]
pub(super) enum Beginning {
    /// A launch with nothing behind it, and `/clear`, which begins a session too. Nothing is
    /// carried in, whatever any earlier session in this directory answered.
    New,
    /// A resume, holding the map the record being picked up kept. `None` where that record
    /// predates maps being kept, which is asked about like anything else unanswered.
    Resumed(Option<TrustStore>),
}

/// What `/resume` does with the session it leaves once a record is chosen: what was opened here is
/// this session's, and the record chosen restores its own, as a start with `--resume` does. The
/// paths programs were let reach and the hosts answered go with it, and the workspace handed on is
/// a clone that shares them, so it is cleared first.
pub(super) fn switch_session(
    session: &mut Session,
    workspace: &mut Workspace,
    servers: &mut crate::state::Servers,
    record: Box<bravebot_session::sessions::Record>,
) -> Switch {
    workspace.end_session();
    *servers = std::mem::take(&mut session.servers);
    Switch {
        record,
        workspace: workspace.clone(),
        sandbox: session.sandbox().clone(),
    }
}

/// What `/clear` and a handoff do before the new session is asked anything: the context goes, the
/// record gets a new id so the session so far keeps its own files and stays resumable, and the
/// directories opened, the checkouts kept and the paths and hosts answered go with the context,
/// since opening one is a grant and leaving it reachable with nothing vouching for it would
/// outlive its answer. `handed_from` is the session a handoff came from, which the new record
/// links back to.
pub(super) fn clear_session(
    session: &mut Session,
    workspace: &mut Workspace,
    handed_from: Option<&str>,
) -> (Conversation, bravebot_session::sessions::Handle) {
    session.clear();
    let mut stored = handle_after_clear(workspace.root(), session);
    match handed_from {
        Some(from) => {
            stored.hand_off_from(from);
            session.note(t!(handoff_started, id = from));
        }
        None => session.note(t!(session_cleared)),
    }
    workspace.end_session();
    (Conversation::new(), stored)
}

/// The record a cleared session writes to: a new id, and the definition every turn is still
/// addressed to, because it outlives the context it was working in.
pub(super) fn handle_after_clear(
    root: &std::path::Path,
    session: &Session,
) -> bravebot_session::sessions::Handle {
    let mut handle = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    handle.set_agent(session.standing_definition().map(|one| one.name.clone()));
    handle.set_skill_mode(session.skill_mode().map(str::to_string));
    handle
}

/// Work the session under the definition [`settle_definition`] settles on, writing the name into
/// its record, or say why it cannot start.
///
/// The `agent` setting is the last source (ADDRESS-13). A name it gives that matches nothing is
/// said in the transcript and the session goes on without one.
///
/// A name typed on this command line that matches nothing ends the session before it begins. A
/// recorded name that matches nothing is said in the transcript, with the definition named and
/// that the narrowing is gone, and the session goes on without it and without recording it: the
/// turns from here are not the definition's, so a later resume must not claim they were.
#[expect(
    clippy::too_many_arguments,
    reason = "each is a separate field of the event loop's state, borrowed mutably or not independently, so one parameter struct would borrow them all mutably"
)]
pub(super) fn settle_under(
    session: &mut Session,
    config: &mut Config,
    workspace: &Workspace,
    home: Option<&std::path::Path>,
    trust: &TrustStore,
    permissions: &bravebot_core::permissions::Permissions,
    stored: &mut bravebot_session::sessions::Handle,
    typed: Option<&str>,
    recorded: Option<&str>,
    configured: Option<&str>,
) -> Result<(), String> {
    match settle_definition(typed, recorded, configured, |name| {
        work_under(session, config, workspace, home, trust, permissions, name)
    })? {
        Settled::Nothing => {}
        Settled::Under {
            name, by_setting, ..
        } => {
            if by_setting {
                session.note(t!(session_working_under_by_setting));
                session.standing_chosen_by_setting();
            }
            stored.set_agent(Some(name));
        }
        Settled::SettingGone { name, why } => {
            session.note(t!(session_agent_setting_gone, definition = name.as_str()));
            session.note(why);
        }
        Settled::Gone { name, why } => {
            session.note(t!(
                session_recorded_definition_gone,
                definition = name.as_str()
            ));
            session.note(why);
            stored.set_agent(None);
        }
    }
    Ok(())
}

/// What the way a session was started leaves it holding.
///
/// Only a resume brings a map, and the map it brings is the record of the session it is picking
/// up, so the answer honoured is the one that session's own user gave. The directory's other
/// records are not read: a map taken from one of those would be standing permission granted on
/// behalf of somebody who was never asked.
pub(super) fn beginning_of(start: &Start, root: &std::path::Path) -> Beginning {
    match start {
        // Choosing has already resolved into one of the other two by the time this runs.
        Start::Fresh
        | Start::Sending(..)
        | Start::Prefilling(..)
        | Start::Choose
        | Start::ChooseFromPr(_) => Beginning::New,
        // Read under the directory being resumed into rather than the one recorded, so a project
        // that was moved or renamed since resumes with its rules about the same files.
        Start::Resuming(record) => Beginning::Resumed(record.trust_map(root)),
    }
}

/// Where a session's opening map comes from, or that there is nobody to take it from.
#[derive(Debug)]
pub(super) enum Opening {
    /// Settled without asking, and what that says about where it came from.
    Settled(TrustStore, Whence),
    /// Settled by a yes an earlier session here gave and kept (TRUST-23), which is what the session
    /// says it started from.
    Remembered(TrustStore, bravebot_agent::trusted::Kept),
    /// Nothing has answered, so the person is.
    Ask,
}

/// Where the map comes from for a session that began this way, under this mode.
///
/// Everything about the answer bar the terminal it is put on, separated from [`opening_trust`] so
/// it can be decided without one, the way [`crate::trust_prompt::answered_by`] is. `kept` reads the
/// remembered answer, and is called only where nothing more specific has answered.
pub(super) fn opening_for(
    beginning: Beginning,
    mode: bravebot_agent::PermissionMode,
    root: &std::path::Path,
    kept: impl FnOnce() -> Option<bravebot_agent::trusted::Kept>,
) -> Opening {
    match beginning {
        // Before the mode is consulted, because the question is not being put in either case and
        // the map this session's own user gave is the more specific record.
        Beginning::Resumed(Some(trust)) => Opening::Settled(trust, Whence::Resumed),
        Beginning::New | Beginning::Resumed(None) => {
            match crate::trust_prompt::answered_by(mode, root) {
                Some(trust) => Opening::Settled(trust, Whence::Unasked),
                // After the mode, so a session bypassing every permission says the flag answered:
                // the map is the same either way, and the flag is what is in force.
                None => match kept() {
                    Some(kept) => Opening::Remembered(
                        crate::trust_prompt::trusting_as_kept(root, &kept),
                        kept,
                    ),
                    None => Opening::Ask,
                },
            }
        }
    }
}

/// The record a yes about `root` is kept in, and which directory is at `root` now, or `None` where
/// no answer about it may be kept ([`bravebot_agent::trusted::record_to_keep`]).
fn keeping_in(
    root: &std::path::Path,
) -> Option<(
    bravebot_agent::trusted::Store,
    bravebot_agent::trusted::Identity,
)> {
    bravebot_agent::trusted::record_to_keep(
        bravebot_agent::home::directory().as_deref(),
        bravebot_agent::home::profile().as_deref(),
        root,
    )
}

/// The answer kept about `root` or the git worktree around it, which settles a session started
/// there ([`bravebot_agent::trusted::honoured`]).
pub(super) fn honoured(root: &std::path::Path) -> Option<bravebot_agent::trusted::Honoured> {
    bravebot_agent::trusted::honoured(
        bravebot_agent::home::directory().as_deref(),
        bravebot_agent::home::profile().as_deref(),
        root,
    )
}

/// What the answer to the startup question does to the record, as the line the session says, or
/// `None` where nothing was kept: only `Answer::Trust` is a yes, so declining and leaving are a no
/// (TRUST-23), and a yes with no record has nowhere to be written.
///
/// Everything [`opening_trust`] does with an answer once the question is closed, apart from the
/// map, so a test drives it with each [`crate::trust_prompt::Answer`] and no terminal.
pub(super) fn keep_the_answer(
    answer: crate::trust_prompt::Answer,
    record: Option<&(
        bravebot_agent::trusted::Store,
        bravebot_agent::trusted::Identity,
    )>,
    session: &str,
    root: &std::path::Path,
) -> Option<String> {
    let yes = answer == crate::trust_prompt::Answer::Trust;
    let kept = bravebot_agent::trusted::keep_a_yes(record, yes, session, true)?;
    let directory = root.display();
    Some(match kept {
        true => t!(session_trust_kept, directory = directory),
        false => t!(
            session_trust_not_kept,
            directory = directory,
            path = record.map_or(String::new(), |(store, _)| store
                .path()
                .display()
                .to_string())
        ),
    })
}

/// Withdraw every answer kept about `root` inside `home`, and say what became of it (TRUST-24).
///
/// Whichever directory was at the path when each was given: a person taking the answer back means
/// the name, and a line left about an earlier directory there would still be read as theirs.
/// Refused in an incognito session, which writes nothing, and the line names the file so the person
/// can remove it by hand.
pub(super) fn forget_trust(home: Option<&std::path::Path>, root: &std::path::Path) -> String {
    use bravebot_agent::trusted;
    let directory = root.display();
    let Some(home) = home else {
        return t!(session_trust_nothing_to_forget, directory = directory);
    };
    let store = trusted::Store::new(home, root);
    if !trusted::may_be_written() {
        return t!(
            session_trust_forget_incognito,
            path = store.path().display()
        );
    }
    // The answer about the worktree root above, read before anything is withdrawn: it settles this
    // directory too, so taking back only the directory's own would leave the next session unasked.
    let above =
        trusted::honoured_by_root(Some(home), bravebot_agent::home::profile().as_deref(), root);
    let own = store.forget();
    let Some(above) = above else {
        return match own {
            Ok(true) => t!(session_trust_forgotten, directory = directory),
            Ok(false) => t!(session_trust_nothing_to_forget, directory = directory),
            Err(error) => t!(
                session_trust_not_forgotten,
                path = store.path().display(),
                error = error
            ),
        };
    };
    match own.and_then(|_| above.store.forget()) {
        Ok(_) => t!(
            session_trust_forgotten_root,
            directory = directory,
            root = above.kept.root.display()
        ),
        Err(error) => t!(
            session_trust_not_forgotten,
            path = above.store.path().display(),
            error = error
        ),
    }
}

/// The trust map the session starts with, or nothing if the user asked to leave.
///
/// A fresh session asks, unless an earlier session begun in this exact directory, or below the root
/// of its git worktree, was answered yes (TRUST-23). The question grants standing permission, and a
/// launch that skipped it because someone said yes last week would be granting that permission on
/// behalf of a user who was never asked; a person who pressed the yes was asked, and was told what it
/// keeps and where.
///
/// Resuming does not ask either, and it is not an exception to that: the map comes out of the record
/// of the very session being picked up, so the answer being honoured is the one its own user gave.
/// It carries the rules that session's writes recorded too, which is what stops a resumed turn
/// reading back a file an earlier turn poisoned. A record from before the map was kept has none, and
/// is asked about.
///
/// A session bypassing every permission is not asked either, and takes the map a yes would have
/// written. Resuming still wins over that: the question is not being put in either case, so there
/// is nothing for the mode to answer, and the map its own user gave is the more specific record.
/// Nothing is written in that mode (MODE-4), since the question is never put to anyone.
pub(super) fn opening_trust(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    root: &std::path::Path,
    beginning: Beginning,
    id: &str,
) -> Option<(TrustStore, Whence)> {
    let record = keeping_in(root);
    let where_it_is = root.display();
    let kept = || honoured(root).map(|honoured| honoured.kept);
    let (trust, whence) = match opening_for(beginning, session.permission_mode(), root, kept) {
        Opening::Settled(trust, whence) => (trust, whence),
        // Asked as a session asked just now would be about what else the tree proposes: the person
        // is here, and what the yes kept was this question and no other.
        Opening::Remembered(trust, kept) => {
            let when = bravebot_session::sessions::how_long_ago(kept.at);
            session.note(if kept.root == root {
                t!(session_trusting_kept, directory = where_it_is, when = when)
            } else {
                t!(
                    session_trusting_kept_root,
                    directory = where_it_is,
                    root = kept.root.display(),
                    when = when
                )
            });
            return Some((trust, Whence::Asked));
        }
        Opening::Ask => {
            // Offered only where it can be written, so a yes that says it is kept never writes
            // nothing: an incognito session reads the record and adds nothing to it.
            let keeping = record.as_ref().map(|(store, _)| store.path());
            // What the question refused to answer on, which is words another program typed at the
            // terminal while it was up. Put in the box rather than dropped, so a person who came back
            // to a question still waiting and a virtualenv activated can see what did it (#403).
            let mut carried = String::new();
            let (trust, answer) = crate::trust_prompt::ask(terminal, root, keeping, &mut carried)?;
            if !carried.is_empty() {
                session.paste_text(&carried);
            }
            if let Some(said) = keep_the_answer(answer, record.as_ref(), id, root) {
                session.note(said);
                return Some((trust, Whence::Asked));
            }
            (trust, Whence::Asked)
        }
    };

    if !trust.is_trusted(".") {
        session.note(t!(session_not_trusting));
        return Some((trust, whence));
    }
    // Named, because two of the three are a grant nobody made just now, and this line is the only
    // place that says where it came from.
    session.note(match whence {
        Whence::Asked => t!(session_trusting, directory = where_it_is),
        Whence::Resumed => t!(session_trusting_as_left, directory = where_it_is),
        Whence::Unasked => t!(session_trusting_unasked, directory = where_it_is),
    });
    Some((trust, whence))
}
