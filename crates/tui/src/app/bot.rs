//! A session started as a bot. The layout addresses every turn to the row's definition and writes
//! that name into no record (BOT-10).

use super::{Unusable, definition_named};
use crate::state::Session;
use bravebot_agent::Workspace;
use bravebot_agent::bots;
use bravebot_core::permissions::Permissions;
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use bravebot_session::audit::Trail;
use bravebot_session::sessions::{self, Record};
use std::path::{Path, PathBuf};

pub(super) struct Opened {
    pub slug: String,
    pub definition: Option<String>,
    pub needs_migration: bool,
    pub resume: Option<Box<Record>>,
    pub map: Option<TrustStore>,
    pub folder: String,
    pub moved: bool,
    /// The workspace from before the move into the bot's folder, when this start moved.
    pub before_move: Option<Workspace>,
}

pub(super) enum Departure {
    Bot {
        slug: String,
        conversation: Option<String>,
        workspace: Workspace,
        sandbox: crate::sandbox_command::Sandboxed,
    },
    List {
        workspace: Workspace,
        sandbox: crate::sandbox_command::Sandboxed,
        notice: Option<String>,
    },
}

pub(super) fn open(
    workspace: &mut Workspace,
    slug: &str,
    conversation: Option<&str>,
) -> Result<Opened, String> {
    if bravebot_core::incognito::engaged() {
        return Err(t!(bots_incognito).to_string());
    }
    let Some(data) = data_folder() else {
        return Err(t!(bots_no_data_folder).to_string());
    };
    let listing = bots::list(&data);
    let Some(row) = listing.rows.into_iter().find(|row| row.slug == slug) else {
        return Err(t!(bots_no_such_bot, slug = slug).to_string());
    };
    let folder = match conversation {
        Some(id) => row
            .conversations
            .iter()
            .find(|each| each.id == id)
            .map(|each| each.directory.clone())
            .unwrap_or_else(|| row.home.clone()),
        None => row.home.clone(),
    };
    if folder == row.home
        && let Err(refused) = bots::ensure_home(&data, slug)
    {
        return Err(refused.to_string());
    }
    let needs_migration = row.definition.is_none();
    let resume = conversation.and_then(|id| sessions::load(Path::new(&folder), id));
    let map = resume
        .as_ref()
        .and_then(|record| record.trust_map(Path::new(&folder)));
    let before_move = workspace.clone();
    let moved = move_to(workspace, &folder)?;
    Ok(Opened {
        slug: row.slug,
        definition: row.definition,
        needs_migration,
        resume: resume.map(Box::new),
        map,
        folder,
        moved,
        before_move: moved.then_some(before_move),
    })
}

/// Migrate a bot that still has no definition, after the startup question was accepted.
pub(super) fn migrate_now(slug: &str) -> Result<Option<String>, String> {
    let Some(data) = data_folder() else {
        return Err(t!(bots_no_data_folder).to_string());
    };
    bots::migrate(&data, slug)
        .map(|row| row.definition)
        .map_err(|refused| refused.to_string())
}

fn move_to(workspace: &mut Workspace, folder: &str) -> Result<bool, String> {
    let target = Path::new(folder);
    let same = std::fs::canonicalize(target)
        .ok()
        .is_some_and(|canonical| canonical == workspace.root());
    if same {
        return Ok(false);
    }
    match workspace.change_root(folder) {
        Ok(_) => Ok(true),
        Err(error) => Err(t!(
            bots_folder_unusable,
            directory = folder,
            problem = error.to_string()
        )),
    }
}

/// Put the definition's model in force, or put back the model it replaced, once.
///
/// A definition that names no model, and never has during this session, leaves the session model
/// alone. That is what lets `/model` stick. The snapshot is taken at the moment a definition model
/// replaces whatever is in force, not at the first turn.
pub(super) fn settle_model_for_the_turn(session: &mut Session, named: Option<&str>) {
    let before = session.model().map(str::to_string);
    if named.is_some() && session.model_from_the_definition().is_none() {
        session.remember_model_beside_a_bot(before.clone());
    }
    if let Some(named) = named {
        session.use_model(Some(named.to_string()));
        session.note_model_from_the_definition(named.to_string());
        return;
    }
    if session.model_from_the_definition().is_some()
        && session.model_from_the_definition().as_deref() == before.as_deref()
    {
        if let Some(fallback) = session.model_beside_a_bot() {
            session.use_model(fallback);
        }
        session.clear_model_from_the_definition();
    }
}

pub(super) fn address_the_row(
    session: &mut Session,
    config: &mut bravebot_config::Config,
    workspace: &Workspace,
    trust: &TrustStore,
    permissions: &Permissions,
    name: &str,
) -> Result<(), String> {
    let addressed = definition_named(
        config,
        &bravebot_agent::agents::resolved(
            workspace,
            bravebot_agent::home::directory().as_deref(),
            trust.clone(),
            permissions.clone(),
            &mut Trail::new(),
        ),
        name,
    )
    .map_err(Unusable::into_message)?;
    let named_model = addressed.model.clone();
    let before = session.model().map(str::to_string);
    settle_model_for_the_turn(session, named_model.as_deref());
    let same = session
        .standing_definition()
        .is_some_and(|standing| standing.name == name);
    session.work_under(addressed);
    if session.model() != before.as_deref() {
        super::ask_for_the_budget_of_the_model_in_force(session, config);
    }
    if !same {
        session.note(t!(session_working_under, definition = name));
    }
    Ok(())
}

/// What a bot's turn may do. [`Ready`](TurnGate::Ready) carries the row's definition name.
pub(super) enum TurnGate {
    NotABot,
    Ready(String),
    Refused(String),
}

/// Whether the turn about to start may be sent, and under which definition.
pub(super) fn gate_turn(session: &mut Session, workspace: &Workspace) -> TurnGate {
    let Some(slug) = session.bot_slug().map(str::to_string) else {
        return TurnGate::NotABot;
    };
    let Some(data) = data_folder() else {
        return TurnGate::Refused(t!(bots_no_data_folder).to_string());
    };
    let Some(row) = bots::list(&data)
        .rows
        .into_iter()
        .find(|row| row.slug == slug)
    else {
        return TurnGate::Refused(t!(bots_no_such_bot, slug = slug.as_str()).to_string());
    };
    let Some(name) = row.definition.clone() else {
        return TurnGate::Refused(
            t!(bots_definition_missing, definition = row.slug.as_str()).to_string(),
        );
    };
    if blocks_the_definition(workspace, bravebot_agent::home::directory().as_deref()) {
        return TurnGate::Refused(t!(bots_home_held, definition = name.as_str()).to_string());
    }
    let directory = workspace.root();
    let Some(text) = directory.to_str() else {
        return TurnGate::Refused(t!(bots_folder_not_text).to_string());
    };
    let check = directory_to_record(directory, &row);
    let allowed =
        bots::works_in(&data, &slug, &check, &session.confirmed_directories()).unwrap_or(false);
    if !allowed {
        return TurnGate::Refused(t!(bots_folder_refused, directory = text).to_string());
    }
    TurnGate::Ready(name)
}

/// The row's spelling of `root`, when that spelling is the same directory and is not a
/// trailing slash or a dot segment. `works_in` compares the text it is given with the row.
pub(super) fn spelling_of_this_directory(root: &Path, recorded: &str) -> Option<String> {
    if !is_clean_absolute(recorded) {
        return None;
    }
    if root.to_str() == Some(recorded) {
        return Some(recorded.to_string());
    }
    let canonical = std::fs::canonicalize(recorded).ok()?;
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    (canonical == root).then(|| recorded.to_string())
}

/// The text a finished turn records, and the text `works_in` is given.
///
/// The row's spelling when that spelling is this directory, so `lastHome` and the conversation
/// entry match the row. Otherwise the workspace root in the ordinary spelling of a Windows drive
/// path: `canonicalize` prefixes one with `\\?\`, and [`bots::is_absolute_path`] rejects that
/// prefix, so the turn would be refused and the record would not be written.
pub(super) fn directory_to_record(root: &Path, row: &bots::Row) -> PathBuf {
    recorded_directory(root, row).unwrap_or_else(|| ordinary_drive_path(root))
}

fn ordinary_drive_path(root: &Path) -> PathBuf {
    bravebot_sandbox::rules::without_verbatim_prefix(root.to_path_buf())
}

fn recorded_directory(root: &Path, row: &bots::Row) -> Option<PathBuf> {
    std::iter::once(row.home.as_str())
        .chain(row.conversations.iter().map(|each| each.directory.as_str()))
        .find_map(|recorded| spelling_of_this_directory(root, recorded))
        .map(PathBuf::from)
}

fn is_clean_absolute(path: &str) -> bool {
    if !bots::is_absolute_path(path) || path.ends_with('/') || path.ends_with('\\') {
        return false;
    }
    path.split(['/', '\\'])
        .all(|part| part != "." && part != "..")
}

pub(super) fn prompt_ordinal(
    conversation: &bravebot_agent::conversation::Conversation,
    sent: &str,
) -> Option<u64> {
    let recounted = conversation.recounted();
    let prompts: Vec<&str> = recounted
        .iter()
        .filter_map(|entry| match entry {
            bravebot_agent::conversation::Said::User(text, _) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let index = prompts.iter().rposition(|text| *text == sent)?;
    u64::try_from(index).ok()
}

pub(super) fn record_finished_turn(
    session: &mut Session,
    id: &str,
    directory: &Path,
    failed: bool,
    prompt: Option<u64>,
) {
    let Some(slug) = session.bot_slug().map(str::to_string) else {
        return;
    };
    let Some(data) = data_folder() else {
        return;
    };
    let spelled = bots::list(&data)
        .rows
        .into_iter()
        .find(|row| row.slug == slug)
        .map(|row| directory_to_record(directory, &row));
    let Some(directory) = spelled.as_deref().unwrap_or(directory).to_str() else {
        return;
    };
    let prompt = (!failed).then_some(prompt).flatten();
    if let Err(refused) = bots::record_conversation(&data, &slug, id, directory, prompt) {
        session.note(refused.to_string());
    }
}

pub(super) fn blocks_the_definition(workspace: &Workspace, home: Option<&Path>) -> bool {
    let Some(home) = home else {
        return false;
    };
    let home = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    let holds = |directory: &Path| directory != home && home.starts_with(directory);
    if holds(workspace.root()) {
        return true;
    }
    workspace
        .added_directories()
        .iter()
        .any(|directory| directory.as_path() == home || holds(directory))
}

fn data_folder() -> Option<PathBuf> {
    bots::data_folder(bots::System::current(), |name| std::env::var_os(name))
}

#[cfg(test)]
mod tests {
    use super::{
        blocks_the_definition, directory_to_record, prompt_ordinal, settle_model_for_the_turn,
        spelling_of_this_directory,
    };
    use bravebot_agent::Workspace;

    #[test]
    fn a_folder_that_holds_the_home_blocks_the_definition_and_the_home_itself_does_not() {
        let root = crate::testutil::scratch_dir("bot-holds-home");
        let project = root.join("project");
        let home = root.join("state");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(&home).expect("home");
        let ancestor = Workspace::new(&root).expect("ancestor");
        assert!(blocks_the_definition(&ancestor, Some(&home)));
        let at_home = Workspace::new(&home).expect("home workspace");
        assert!(!blocks_the_definition(&at_home, Some(&home)));
        let mut beside = Workspace::new(&project).expect("project");
        beside
            .add_directory(home.to_str().expect("utf-8"))
            .expect("opened");
        assert!(blocks_the_definition(&beside, Some(&home)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_spelling_of_the_same_directory_is_kept_and_a_slash_or_a_dot_is_not() {
        let root = crate::testutil::scratch_dir("bot-spelling");
        let _ = std::fs::remove_dir_all(&root);
        let real = root.join("real");
        let link = root.join("link");
        std::fs::create_dir_all(&real).expect("real");
        std::os::unix::fs::symlink(&real, &link).expect("link");
        let canonical = std::fs::canonicalize(&link).expect("canonical");
        let linked = link.to_str().expect("utf-8");
        assert_eq!(
            spelling_of_this_directory(&canonical, linked).as_deref(),
            Some(linked)
        );
        assert_eq!(
            spelling_of_this_directory(&canonical, canonical.to_str().expect("utf-8")).as_deref(),
            Some(canonical.to_str().expect("utf-8"))
        );
        let slashed = format!("{linked}/");
        assert_eq!(spelling_of_this_directory(&canonical, &slashed), None);
        let dotted = root.join("link").join(".").join("..").join("real");
        let dotted = dotted.to_str().expect("utf-8");
        assert_eq!(spelling_of_this_directory(&canonical, dotted), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_ordinal_is_the_last_user_message_whose_text_is_the_prompt() {
        let mut conversation = bravebot_agent::conversation::Conversation::new();
        conversation.push(bravebot_aichat::protocol::Message::user("first"));
        conversation.push(bravebot_aichat::protocol::Message::assistant("ok"));
        conversation.push(bravebot_aichat::protocol::Message::user("second"));
        assert_eq!(prompt_ordinal(&conversation, "second"), Some(1));
        assert_eq!(prompt_ordinal(&conversation, "first"), Some(0));
        conversation.push(bravebot_aichat::protocol::Message::user("first"));
        assert_eq!(prompt_ordinal(&conversation, "first"), Some(2));
        conversation.push_composed(
            bravebot_aichat::protocol::Message::user("carry on"),
            bravebot_agent::conversation::Composed::Written,
        );
        assert_eq!(prompt_ordinal(&conversation, "carry on"), None);
        assert_eq!(prompt_ordinal(&conversation, "second"), Some(1));
    }

    #[test]
    fn leaving_keeps_a_confirmed_directory_and_restores_the_one_from_before() {
        let root = crate::testutil::scratch_dir("bot-leave");
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let project = root.join("project");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(&project).expect("project");
        let before = Workspace::new(&project).expect("project");
        let moved = Workspace::new(&home).expect("home");
        let mut session = crate::state::Session::new("none");
        session.remember_workspace_before_the_bot(before.clone());
        assert_eq!(
            session.workspace_to_leave_with(moved.clone()).root(),
            before.root()
        );
        session.note_confirmed_directory(moved.root().to_path_buf());
        assert_eq!(
            session.workspace_to_leave_with(moved.clone()).root(),
            moved.root()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_turn_in_a_symlinked_home_is_recorded_under_the_rows_spelling() {
        let root = crate::testutil::scratch_dir("bot-record-spelling");
        let _ = std::fs::remove_dir_all(&root);
        let state = root.join("state");
        let real = root.join("real-data");
        let link = root.join("link-data");
        std::fs::create_dir_all(&state).expect("state");
        std::fs::create_dir_all(&real).expect("data");
        std::os::unix::fs::symlink(&real, &link).expect("link");
        let saved = bravebot_agent::bots::make_in(
            &state,
            &link,
            bravebot_agent::bots::NewBot {
                name: "Keeper".to_string(),
                purpose: "Keeps notes.".to_string(),
                model: None,
                avatar: None,
            },
            1.0,
            std::time::Duration::from_secs(1),
        )
        .expect("a bot");
        let canonical = std::fs::canonicalize(&saved.row.home).expect("home exists");
        let recorded = directory_to_record(&canonical, &saved.row);
        assert_eq!(recorded.to_str(), Some(saved.row.home.as_str()));
        let noted = bravebot_agent::bots::record_conversation_in(
            &state,
            &link,
            &saved.row.slug,
            "s-1",
            recorded.to_str().expect("utf-8"),
            Some(2),
            2.0,
            std::time::Duration::from_secs(1),
        )
        .expect("recorded");
        let row = noted.row.expect("a row");
        assert_eq!(row.last_home.as_ref().map(|home| home.prompt), Some(2));
        assert_eq!(row.conversations.len(), 1);
        assert_eq!(row.conversations[0].directory, saved.row.home);
        let canonical_text = canonical.to_str().expect("utf-8");
        let again = bravebot_agent::bots::record_conversation_in(
            &state,
            &link,
            &saved.row.slug,
            "s-2",
            canonical_text,
            Some(0),
            3.0,
            std::time::Duration::from_secs(1),
        )
        .expect("recorded the canonical text");
        let row = again.row.expect("a row");
        assert_eq!(row.last_home.as_ref().map(|home| home.prompt), Some(2));
        assert_eq!(row.conversations.len(), 2);
        assert_ne!(row.conversations[1].directory, saved.row.home);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_definition_model_is_put_back_once_and_a_later_pick_stays() {
        let mut session = crate::state::Session::new("none");
        session.use_model(Some("opening".to_string()));
        settle_model_for_the_turn(&mut session, None);
        assert_eq!(session.model(), Some("opening"));

        session.use_model(Some("picked".to_string()));
        settle_model_for_the_turn(&mut session, None);
        assert_eq!(session.model(), Some("picked"));

        settle_model_for_the_turn(&mut session, Some("defined"));
        assert_eq!(session.model(), Some("defined"));
        settle_model_for_the_turn(&mut session, None);
        assert_eq!(session.model(), Some("picked"));

        session.use_model(Some("again".to_string()));
        settle_model_for_the_turn(&mut session, None);
        assert_eq!(session.model(), Some("again"));

        session.use_model(None);
        settle_model_for_the_turn(&mut session, Some("defined"));
        settle_model_for_the_turn(&mut session, None);
        assert_eq!(session.model(), None);
    }

    #[test]
    fn a_verbatim_drive_root_is_given_as_the_ordinary_spelling() {
        let row = bravebot_agent::bots::Row {
            slug: "keeper".to_string(),
            name: "Keeper".to_string(),
            purpose: "Keeps notes.".to_string(),
            avatar: "keeper".to_string(),
            model: None,
            home: "/bot/home".to_string(),
            definition: Some("keeper".to_string()),
            session: None,
            conversations: Vec::new(),
            last_home: None,
            archived: 0,
            retired: 0.0,
            created: 1.0,
            updated: 1.0,
        };
        let verbatim = std::path::PathBuf::from(r"\\?\C:\work");
        let recorded = directory_to_record(&verbatim, &row);
        assert_eq!(recorded, std::path::PathBuf::from(r"C:\work"));
        assert!(bravebot_agent::bots::is_absolute_path(
            recorded.to_str().expect("utf-8")
        ));
        assert!(!bravebot_agent::bots::is_absolute_path(
            verbatim.to_str().expect("utf-8")
        ));

        let session = crate::state::Session::new("none");
        session.note_confirmed_directory(verbatim.clone());
        assert_eq!(
            session.confirmed_directories(),
            vec![std::path::PathBuf::from(r"C:\work")]
        );
        assert!(session.root_was_confirmed(&verbatim));
        assert!(session.root_was_confirmed(std::path::Path::new(r"C:\work")));
        assert!(!session.root_was_confirmed(std::path::Path::new(r"C:\other")));

        let root = crate::testutil::scratch_dir("bot-verbatim");
        let _ = std::fs::remove_dir_all(&root);
        let state = root.join("state");
        let data = root.join("data");
        std::fs::create_dir_all(&state).expect("state");
        std::fs::create_dir_all(&data).expect("data");
        let saved = bravebot_agent::bots::make_in(
            &state,
            &data,
            bravebot_agent::bots::NewBot {
                name: "Keeper".to_string(),
                purpose: "Keeps notes.".to_string(),
                model: None,
                avatar: None,
            },
            1.0,
            std::time::Duration::from_secs(1),
        )
        .expect("a bot");
        let ordinary = std::path::Path::new(r"C:\work");
        let noted = bravebot_agent::bots::record_conversation_in(
            &state,
            &data,
            &saved.row.slug,
            "s-1",
            ordinary.to_str().expect("utf-8"),
            Some(1),
            2.0,
            std::time::Duration::from_secs(1),
        )
        .expect("recorded");
        let noted = noted.row.expect("a row");
        assert_eq!(noted.conversations[0].directory, r"C:\work");
        assert!(noted.last_home.is_none());
        let refused = bravebot_agent::bots::record_conversation_in(
            &state,
            &data,
            &saved.row.slug,
            "s-2",
            verbatim.to_str().expect("utf-8"),
            Some(1),
            3.0,
            std::time::Duration::from_secs(1),
        );
        assert!(matches!(
            refused,
            Err(bravebot_agent::bots::Refused::Conversation)
        ));
        let _ = std::fs::remove_dir_all(&root);
    }
}
