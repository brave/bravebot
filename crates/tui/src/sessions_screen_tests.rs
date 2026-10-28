//! The Sessions screen as the loop meets it: the key that opens it, what it takes while it is
//! open, and what the keys that act on a task ask the loop for (SCREEN-1, SCREEN-8).

use super::*;
use crate::sessions_screen::{Effect, SessionsScreen};
use bravebot_session::listing::{Grouping, Page, Status as TaskStatus, Task};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_in(session: &mut Session, text: &str) {
    for letter in text.chars() {
        handle_key(session, press(KeyCode::Char(letter)));
    }
}

fn task(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        directory: "/work/one".to_string(),
        title: format!("title {id}"),
        status,
        held: None,
        updated: 100,
        started: 90,
        model: Some("model-a".to_string()),
        branch: None,
        prompt: None,
        turns: 1,
        tokens: 0,
        read: 0,
        written: 0,
        mode: None,
        background: false,
        live: false,
        current: false,
        manifest: false,
        agent: None,
        bytes: 0,
        issue: None,
        pull_request: None,
        handed_off_from: None,
    }
}

fn opened_over(tasks: Vec<Task>) -> Session {
    let mut session = Session::new("kernel-enforced");
    let mut sessions_screen = SessionsScreen::new(
        Grouping::Together,
        Vec::new(),
        std::collections::HashMap::new(),
        None,
    );
    sessions_screen.replace(Page { tasks, more: false });
    session.open_sessions_screen(sessions_screen);
    session
}

/// SCREEN-1. Left with nothing typed has nowhere to go in the line, so it opens the Sessions screen.
#[test]
fn left_at_the_empty_box_opens_the_sessions_screen() {
    let mut session = Session::new("kernel-enforced");

    let action = handle_key(&mut session, press(KeyCode::Left));

    assert_eq!(action, Action::OpenCenter);
}

/// SCREEN-1. Left with something typed is the caret's, at the start of the line as anywhere else.
#[test]
fn left_with_text_in_the_box_moves_the_caret_and_opens_nothing() {
    let mut session = Session::new("kernel-enforced");
    type_in(&mut session, "ab");

    assert_ne!(
        handle_key(&mut session, press(KeyCode::Left)),
        Action::OpenCenter
    );
    assert_ne!(
        handle_key(&mut session, press(KeyCode::Left)),
        Action::OpenCenter
    );
    assert_eq!(session.caret(), 0);
    let at_the_start = handle_key(&mut session, press(KeyCode::Left));
    assert_ne!(
        at_the_start,
        Action::OpenCenter,
        "Left at the start of a line that holds text opened the Sessions screen"
    );
    assert_eq!(session.input(), "ab", "the line was changed");
}

/// SCREEN-1. Left held with Ctrl or Alt is a move by word, and is the line's whatever it holds.
#[test]
fn a_chord_on_left_is_not_the_key_that_opens_it() {
    for modifiers in [
        KeyModifiers::CONTROL,
        KeyModifiers::ALT,
        KeyModifiers::SHIFT,
    ] {
        let mut session = Session::new("kernel-enforced");
        let action = handle_key(&mut session, KeyEvent::new(KeyCode::Left, modifiers));
        assert_ne!(action, Action::OpenCenter, "{modifiers:?}-Left opened it");
    }
}

/// SCREEN-1. While a list of commands is offered or a command for the shell is typed, Left is the
/// box's.
#[test]
fn left_is_the_boxs_while_something_else_has_the_line() {
    let mut offering = Session::new("kernel-enforced");
    type_in(&mut offering, "/");
    assert_ne!(
        handle_key(&mut offering, press(KeyCode::Left)),
        Action::OpenCenter
    );

    let mut shell = Session::new("kernel-enforced");
    type_in(&mut shell, "!");
    assert!(
        shell.shell,
        "a bang on an empty line did not enter shell mode"
    );
    assert_ne!(
        handle_key(&mut shell, press(KeyCode::Left)),
        Action::OpenCenter
    );
}

/// SCREEN-1. While a turn runs Left with the box empty opens the Sessions screen, and Left with something
/// typed is the caret's, so the line is as editable as it always is.
#[test]
fn left_opens_the_sessions_screen_while_a_turn_runs_and_only_over_an_empty_box() {
    let mut working = Session::new("kernel-enforced");
    working.status = crate::state::Status::Working;
    assert_eq!(
        handle_key_while_working(&mut working, press(KeyCode::Left)),
        Action::OpenCenter,
        "Left did not open the Sessions screen while a turn ran"
    );

    let mut typing = Session::new("kernel-enforced");
    typing.status = crate::state::Status::Working;
    handle_key_while_working(&mut typing, press(KeyCode::Char('a')));
    assert_ne!(
        handle_key_while_working(&mut typing, press(KeyCode::Left)),
        Action::OpenCenter,
        "Left over a line with text in it opened the Sessions screen"
    );

    let mut command = Session::new("kernel-enforced");
    command.status = crate::state::Status::Running;
    assert_ne!(
        handle_key_while_working(&mut command, press(KeyCode::Left)),
        Action::OpenCenter,
        "Left opened the Sessions screen while a command the person typed ran"
    );
}

/// SCREEN-1. A press that arrived with others in one read is part of a paste, not a key.
#[test]
fn left_in_a_run_of_keys_is_not_the_key_that_opens_it() {
    let mut session = Session::new("kernel-enforced");
    session.key_arrived_alone = false;
    assert_ne!(
        handle_key(&mut session, press(KeyCode::Left)),
        Action::OpenCenter
    );
}

/// SCREEN-1. The Sessions screen takes every key: nothing reaches the box, and the box is as it was when it
/// is left.
#[test]
fn the_sessions_screen_takes_every_key_and_leaves_the_box_as_it_was() {
    let mut session = opened_over(vec![task("a", TaskStatus::Saved)]);

    for code in "abc /!?".chars().map(KeyCode::Char) {
        handle_key(&mut session, press(code));
    }
    handle_key(&mut session, press(KeyCode::Esc));
    handle_key(&mut session, press(KeyCode::Esc));

    assert_eq!(
        session.input(),
        "",
        "a key reached the box under the Sessions screen"
    );
    assert!(
        !session.showing_sessions(),
        "Escape did not leave the Sessions screen"
    );
}

/// SCREEN-1. Ctrl-C leaves the Sessions screen and does nothing else, and Escape does too.
#[test]
fn ctrl_c_and_escape_leave_the_sessions_screen_and_do_not_quit() {
    for leave in [
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        press(KeyCode::Esc),
    ] {
        let mut session = opened_over(vec![task("a", TaskStatus::Saved)]);
        let action = handle_key(&mut session, leave);
        assert_eq!(action, Action::Redraw);
        assert!(!session.showing_sessions());
        assert!(
            !session.is_quitting(),
            "the key that leaves the Sessions screen quit the session"
        );
    }
}

/// SCREEN-8. What a key asks for beyond the Sessions screen's own state comes back as an action for the loop.
#[test]
fn a_key_that_acts_on_a_task_asks_the_loop_for_it() {
    let mut session = opened_over(vec![task("a", TaskStatus::Saved)]);
    let action = handle_key(&mut session, press(KeyCode::Enter));
    assert!(matches!(action, Action::SessionsScreen(Effect::Open(opened)) if opened.id == "a"));

    let mut session = opened_over(vec![task("a", TaskStatus::Saved)]);
    let action = handle_key(&mut session, press(KeyCode::Char('n')));
    assert_eq!(
        action,
        Action::SessionsScreen(Effect::New(Some("/work/one".to_string())))
    );
}

// ---- what the loop carries out, against real stores

use crate::state::Servers;
use bravebot_agent::Workspace;
use bravebot_agent::conversation::Conversation;
use bravebot_core::permissions::Permissions;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::trust::TrustStore;
use bravebot_session::sessions::{self, Front, Handle, Standing};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Everything the loop holds while a session is at the plain prompt, for a session in `root`.
struct Loop {
    session: Session,
    conversation: Conversation,
    stored: Handle,
    answers: super::opening::Answers,
    workspace: Workspace,
    scratch: Option<bravebot_agent::SessionScratch>,
    settings: bravebot_config::Settings,
    run_settings: RunSettings,
}

impl Loop {
    fn in_a_directory(root: &Path) -> Self {
        std::fs::create_dir_all(root).expect("create the directory");
        let settings = bravebot_config::Settings::default();
        Self {
            session: Session::new("none"),
            conversation: Conversation::new(),
            stored: Handle::begin(root, Front::Terminal, bravebot_stamp::BUILD),
            answers: super::opening::Answers::opening(
                TrustStore::new(root),
                TrustedPrograms::new(),
                super::opening::Rules {
                    sources: super::opening::RuleSources {
                        state: None,
                        named: None,
                        started: root.to_path_buf(),
                        managed: bravebot_config::Managed::default(),
                    },
                    permissions: Permissions::new(),
                },
            ),
            workspace: Workspace::new(root).expect("a workspace"),
            scratch: None,
            run_settings: RunSettings::from_settings(&settings),
            settings,
        }
    }

    fn held(&mut self) -> Held<'_> {
        Held {
            session: &mut self.session,
            conversation: &mut self.conversation,
            stored: &mut self.stored,
            answers: &mut self.answers,
            workspace: &mut self.workspace,
            scratch: &mut self.scratch,
            settings: &self.settings,
            run_settings: &self.run_settings,
        }
    }

    /// Open the Sessions screen and give back the task whose name is `title`.
    fn open_and_find(&mut self, title: &str) -> Task {
        self.held().open_sessions_screen();
        self.find(title)
    }

    fn find(&self, title: &str) -> Task {
        self.session
            .sessions_screen()
            .expect("the Sessions screen is open")
            .rows()
            .iter()
            .find(|row| row.task.title == title)
            .unwrap_or_else(|| panic!("no task called {title:?} on the list"))
            .task
            .clone()
    }

    fn says(&self) -> String {
        self.session
            .sessions_screen()
            .and_then(|sessions_screen| sessions_screen.notice())
            .unwrap_or_default()
            .to_string()
    }
}

/// A session of `root` that ended, so nothing holds its record.
fn left_behind_in(root: &Path, prompt: &str) -> String {
    let mut handle = Handle::begin(root, Front::Terminal, bravebot_stamp::BUILD);
    save(&mut handle, root, prompt);
    handle.id().to_string()
}

fn save(handle: &mut Handle, root: &Path, prompt: &str) {
    let snapshot = Conversation::new().snapshot();
    let mut trust = TrustStore::new(root);
    trust.trust(".");
    handle.save(
        prompt,
        Standing {
            history: None,
            conversation: &snapshot,
            turns: 1,
            tokens: 0,
            split: Default::default(),
            spend: &BTreeMap::new(),
            timing: &BTreeMap::new(),
            model: None,
            todos: &BTreeMap::new(),
            asides: &[],
            trust: &trust,
            programs: &TrustedPrograms::default(),
            directories: &[],
            answered_by_mode: false,
            manifest: None,
            rewind: &[],
            checkouts: &[],
            touched: None,
        },
    );
}

fn project(name: &str) -> PathBuf {
    crate::test_profile::project(name)
}

/// SCREEN-8. A task of the directory the session works in is carried on in this process: the Sessions screen
/// closes, the record is the one the loop starts the next session from, and the session left is the
/// one the terminal was in.
#[test]
fn a_task_of_this_directory_is_resumed_in_this_process() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-resume-here");
    let other = left_behind_in(&root, "the other one");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let task = here.open_and_find("the other one");
    let mut servers = Servers::default();
    let mut switch = None;

    let ended = here.held().open_task(&mut servers, &mut switch, &task);

    assert!(matches!(ended, Some(Ended::Left(_))), "{ended:?}");
    assert_eq!(switch.expect("a record to carry on in").record.id, other);
    assert!(
        !here.session.showing_sessions(),
        "the Sessions screen was left open under the new session"
    );
}

/// SCREEN-8. A task of another directory is carried on by a process started there, with the id the
/// record has, and this session ends. A directory that is gone is said to be, and nothing ends.
#[test]
fn a_task_of_another_directory_is_carried_on_by_a_process_started_there() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-resume-there/here");
    let there = project("sessions-screen-resume-there/there");
    std::fs::create_dir_all(&there).expect("create");
    let id = left_behind_in(&there, "a task there");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let task = here.open_and_find("a task there");
    let mut servers = Servers::default();
    let mut switch = None;

    let ended = here.held().open_task(&mut servers, &mut switch, &task);
    let Some(Ended::Open(opening)) = ended else {
        panic!("the session did not end for another directory: {ended:?}");
    };
    assert_eq!(Path::new(&opening.directory), there);
    assert_eq!(opening.how, How::Resume(id));
    assert!(switch.is_none(), "a record was carried on in this process");

    std::fs::remove_dir_all(&there).expect("remove");
    let mut servers = Servers::default();
    let ended = here.held().open_task(&mut servers, &mut switch, &task);
    assert!(
        ended.is_none(),
        "the session ended for a directory that is gone"
    );
    assert!(here.says().contains("no longer there"), "{:?}", here.says());
}

/// SCREEN-8. A task a background process runs is joined and not resumed, whatever directory it is
/// in, since a second writer would fork the record.
#[test]
fn a_task_a_process_runs_is_joined_and_not_resumed() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-join");
    let id = left_behind_in(&root, "running elsewhere");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let mut task = here.open_and_find("running elsewhere");
    task.live = true;
    let mut servers = Servers::default();
    let mut switch = None;

    let ended = here.held().open_task(&mut servers, &mut switch, &task);

    let Some(Ended::Open(opening)) = ended else {
        panic!("{ended:?}");
    };
    assert_eq!(opening.how, How::Attach(id));
    assert!(switch.is_none());
}

/// SCREEN-8. A record another terminal has open is refused with the sentence `/resume` uses, the
/// sessions_screen stays open, and this session goes on.
#[test]
fn a_task_another_terminal_has_open_is_refused_and_the_sessions_screen_stays() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-open-elsewhere");
    let mut elsewhere = Handle::begin(&root, Front::Terminal, bravebot_stamp::BUILD);
    save(&mut elsewhere, &root, "open in another terminal");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let task = here.open_and_find("open in another terminal");
    assert_eq!(task.status, TaskStatus::OpenElsewhere);
    let mut servers = Servers::default();
    let mut switch = None;

    let ended = here.held().open_task(&mut servers, &mut switch, &task);

    assert!(ended.is_none());
    assert!(switch.is_none());
    assert!(
        here.session.showing_sessions(),
        "the Sessions screen was closed by a refusal"
    );
    assert_eq!(here.says(), crate::resume::Refusal::HeldByAnother.note());
}

/// SCREEN-8. Opening the session the terminal is in closes the Sessions screen and changes nothing else.
#[test]
fn opening_the_session_you_are_in_closes_the_sessions_screen() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-open-current");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let task = here.open_and_find("this one");
    assert!(task.current);
    let mut servers = Servers::default();
    let mut switch = None;

    let ended = here.held().open_task(&mut servers, &mut switch, &task);

    assert!(ended.is_none() && switch.is_none());
    assert!(!here.session.showing_sessions());
}

/// SCREEN-8. Forking here copies the record under a new id and carries on in the copy, and leaves
/// the original as it was. Forking from another directory is left to a process started there.
#[test]
fn forking_copies_the_record_and_carries_on_in_the_copy() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-fork/here");
    let there = project("sessions-screen-fork/there");
    std::fs::create_dir_all(&there).expect("create");
    let original = left_behind_in(&root, "the original");
    let far = left_behind_in(&there, "a far one");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let task = here.open_and_find("the original");
    let mut servers = Servers::default();
    let mut switch = None;

    let ended = here.held().fork_task(&mut servers, &mut switch, &task);

    assert!(matches!(ended, Some(Ended::Left(_))), "{ended:?}");
    let copy = switch.expect("a copy to carry on in").record;
    assert_ne!(copy.id, original);
    assert_eq!(copy.title, "the original (fork)");
    assert_eq!(
        sessions::load(&root, &original)
            .expect("the original is still there")
            .title,
        "the original"
    );

    let mut here = Loop::in_a_directory(&root);
    let task = here.open_and_find("a far one");
    let mut switch = None;
    let ended = here.held().fork_task(&mut servers, &mut switch, &task);
    let Some(Ended::Open(opening)) = ended else {
        panic!("{ended:?}");
    };
    assert_eq!(opening.how, How::Fork(far));
    assert!(switch.is_none());
}

/// SCREEN-8. A manifest run cannot be copied.
#[test]
fn a_manifest_run_is_not_forked() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-fork-manifest");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    here.held().open_sessions_screen();
    let mut task = task("m", TaskStatus::Saved);
    task.directory = root.display().to_string();
    task.manifest = true;
    let mut servers = Servers::default();
    let mut switch = None;

    let ended = here.held().fork_task(&mut servers, &mut switch, &task);

    assert!(ended.is_none() && switch.is_none());
    assert_eq!(here.says(), crate::resume::Refusal::Manifest.note());
}

/// SCREEN-8. A manifest run is not opened, and the sentence Enter gives for it is said.
#[test]
fn a_manifest_run_is_not_opened_and_says_why() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-open-manifest");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    here.held().open_sessions_screen();
    let mut task = task("m", TaskStatus::Saved);
    task.directory = root.display().to_string();
    task.manifest = true;
    let mut servers = Servers::default();
    let mut switch = None;

    let ended = here.held().open_task(&mut servers, &mut switch, &task);

    assert!(ended.is_none() && switch.is_none());
    assert_eq!(here.says(), crate::resume::Refusal::Manifest.note());
    assert!(
        here.session.showing_sessions(),
        "the refusal closed the screen"
    );
}

/// SCREEN-13. `/resume` with nothing after it opens the Sessions screen over the session, and
/// leaves the session where it was; one with an id after it is not the screen's.
#[test]
fn a_bare_resume_opens_the_sessions_screen_and_a_named_one_does_not() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-bare-resume");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let id = here.stored.id().to_string();

    let record = super::session_commands::record_to_resume(&mut here.session, &root, &id, "");

    assert!(
        record.is_none(),
        "a bare /resume chose a record before the screen was used"
    );
    assert!(here.session.showing_sessions(), "the screen is not open");

    here.session.close_sessions_screen();
    let said_before = here.session.transcript.len();
    let record =
        super::session_commands::record_to_resume(&mut here.session, &root, &id, "no-such");
    assert!(record.is_none());
    assert!(
        !here.session.showing_sessions(),
        "a named resume opened the screen"
    );
    assert_eq!(
        here.session.transcript.len(),
        said_before + 1,
        "the refusal was not said in the transcript"
    );
}

/// SCREEN-8. A new session is begun in this process where the cursor's task ran here or nowhere is
/// named, and by a process started there where it ran in another directory.
#[test]
fn a_new_session_is_begun_where_the_task_under_the_cursor_ran() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-new/here");
    let there = project("sessions-screen-new/there");
    std::fs::create_dir_all(&there).expect("create");
    let mut here = Loop::in_a_directory(&root);

    assert!(
        here.held().fresh_elsewhere(None).is_none(),
        "no directory named"
    );
    assert!(
        here.held()
            .fresh_elsewhere(Some(root.display().to_string()))
            .is_none(),
        "this directory is begun in this process"
    );
    let ended = here
        .held()
        .fresh_elsewhere(Some(there.display().to_string()))
        .expect("another directory is not begun in this process");
    let Some(Ended::Open(opening)) = ended else {
        panic!("{ended:?}");
    };
    assert_eq!(
        (Path::new(&opening.directory), opening.how),
        (there.as_path(), How::Fresh)
    );

    std::fs::remove_dir_all(&there).expect("remove");
    here.held().open_sessions_screen();
    let ended = here
        .held()
        .fresh_elsewhere(Some(there.display().to_string()))
        .expect("another directory");
    assert!(ended.is_none(), "a directory that is gone was left for");
    assert!(here.says().contains("no longer there"));
}

/// SCREEN-9. Rename writes the record of a session nothing holds, the list shows the new name, and
/// a record a process holds is refused with a sentence and left as it was.
#[test]
fn rename_writes_the_record_unless_a_process_holds_it() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-rename");
    let free = left_behind_in(&root, "a free session");
    let mut held_elsewhere = Handle::begin(&root, Front::Terminal, bravebot_stamp::BUILD);
    save(&mut held_elsewhere, &root, "a held session");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");

    let task = here.open_and_find("a free session");
    super::sessions_screen::rename_other(&mut here.session, &task, "a better name");
    assert_eq!(
        sessions::load(&root, &free).expect("record").title,
        "a better name"
    );
    here.find("a better name");
    assert!(here.says().contains("Renamed to"), "{:?}", here.says());

    let task = here.find("a held session");
    super::sessions_screen::rename_other(&mut here.session, &task, "taken");
    assert_eq!(
        sessions::load(&root, &task.id).expect("record").title,
        "a held session",
        "a record a process holds was renamed"
    );
    assert!(
        here.says().contains("is open in a process"),
        "{:?}",
        here.says()
    );
}

/// SCREEN-9. Renaming the session you are in names it as the rename command does, so the title, the
/// record and the list agree.
#[test]
fn renaming_this_session_is_what_the_rename_command_does() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-rename-current");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    here.open_and_find("this one");

    here.held()
        .rename_this_session("named from the Sessions screen");

    assert_eq!(here.stored.title(), "named from the Sessions screen");
    assert_eq!(
        sessions::load(&root, here.stored.id())
            .expect("record")
            .title,
        "named from the Sessions screen"
    );
    here.find("named from the Sessions screen");
}

/// SCREEN-10. Archiving takes a task off the list and keeps its record, the list leaves it out on
/// every later read, and writing the session again brings it back without anything un-archiving it.
#[test]
fn an_archived_task_stays_off_the_list_until_its_session_is_written_again() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-archive");
    let id = left_behind_in(&root, "to be archived");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let task = here.open_and_find("to be archived");

    super::sessions_screen::carry_out(&mut here.session, Effect::Archive(Box::new(task)));

    assert!(
        sessions::load(&root, &id).is_some(),
        "archiving removed the record"
    );
    assert!(
        bravebot_session::listing::load_archived().contains_key(&id),
        "nothing was saved"
    );
    super::sessions_screen::read_sessions_screen(&mut here.session, Instant::now());
    assert!(
        !here
            .session
            .sessions_screen()
            .expect("open")
            .rows()
            .iter()
            .any(|row| row.task.id == id),
        "a read brought an archived task back"
    );

    // Written again a second later: the clock of a record is in whole seconds.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    sessions::rename_stored(&root, &id, "used again").expect("written");
    here.held().open_sessions_screen();
    here.find("used again");
}

/// SCREEN-10. Deleting removes the record and everything beside it. The session you are in and a
/// record another terminal has open are left, with a sentence saying why.
#[test]
fn delete_removes_a_free_record_and_leaves_the_current_one_and_an_open_one() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-delete");
    let gone = left_behind_in(&root, "to be deleted");
    let mut open = Handle::begin(&root, Front::Terminal, bravebot_stamp::BUILD);
    save(&mut open, &root, "open elsewhere");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let doomed = here.open_and_find("to be deleted");

    super::sessions_screen::carry_out(&mut here.session, Effect::Delete(Box::new(doomed)));
    assert!(
        sessions::load(&root, &gone).is_none(),
        "the record is still there"
    );
    assert!(here.says().contains("Deleted"), "{:?}", here.says());
    assert!(
        !here
            .session
            .sessions_screen()
            .expect("open")
            .rows()
            .iter()
            .any(|row| row.task.id == gone)
    );

    let held = here.find("open elsewhere");
    super::sessions_screen::carry_out(&mut here.session, Effect::Delete(Box::new(held.clone())));
    assert!(
        sessions::load(&root, &held.id).is_some(),
        "a record another terminal has open was deleted"
    );
    assert!(
        here.says().contains("another terminal"),
        "{:?}",
        here.says()
    );

    let current = here.find("this one");
    super::sessions_screen::carry_out(&mut here.session, Effect::Delete(Box::new(current.clone())));
    assert!(
        sessions::load(&root, &current.id).is_some(),
        "the session in use was deleted"
    );
    assert!(
        here.says().contains("session you are in"),
        "{:?}",
        here.says()
    );
    super::sessions_screen::carry_out(
        &mut here.session,
        Effect::Archive(Box::new(current.clone())),
    );
    assert!(
        here.says().contains("session you are in"),
        "{:?}",
        here.says()
    );
    assert!(
        here.session
            .sessions_screen()
            .expect("open")
            .rows()
            .iter()
            .any(|row| row.task.id == current.id)
    );
}

/// SCREEN-5. What the person chose about the grouping and the pins is saved, and the Sessions screen opens
/// with it the next time.
#[test]
fn the_grouping_and_the_pins_are_saved_and_the_next_sessions_screen_opens_with_them() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-preferences");
    let id = left_behind_in(&root, "pinned one");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    here.held().open_sessions_screen();
    assert_eq!(
        here.session.sessions_screen().expect("open").grouping(),
        Grouping::Together,
        "the list opens together until the person chooses otherwise"
    );

    super::sessions_screen::carry_out(&mut here.session, Effect::Grouping(Grouping::Project));
    super::sessions_screen::carry_out(&mut here.session, Effect::Pins(vec![id.clone()]));
    here.session.close_sessions_screen();
    here.held().open_sessions_screen();

    let sessions_screen = here.session.sessions_screen().expect("open");
    assert_eq!(sessions_screen.grouping(), Grouping::Project);
    assert_eq!(sessions_screen.pinned(), [id]);
}

/// SCREEN-8. Ending the process of a task nothing runs says so, and ends nothing.
#[test]
fn stopping_a_task_nothing_runs_says_so() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-stop-nothing");
    left_behind_in(&root, "not running");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let mut task = here.open_and_find("not running");
    task.live = true;

    super::sessions_screen::carry_out(&mut here.session, Effect::Stop(Box::new(task)));

    assert!(
        here.says().contains("Nothing is running"),
        "{:?}",
        here.says()
    );
}

/// SCREEN-8. What changes the session being run is given back for the loop that holds it, and the
/// rest is done where it was asked.
#[test]
fn what_ends_the_session_is_given_back_and_the_rest_is_done_where_it_is_asked() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = project("sessions-screen-given-back");
    let mut here = Loop::in_a_directory(&root);
    save(&mut here.stored, &root, "this one");
    let task = here.open_and_find("this one");

    for effect in [
        Effect::Open(Box::new(task.clone())),
        Effect::Fork(Box::new(task.clone())),
        Effect::New(None),
        Effect::Rename {
            task: Box::new(task.clone()),
            name: "x".to_string(),
        },
    ] {
        assert_eq!(
            super::sessions_screen::carry_out(&mut here.session, effect.clone()),
            Some(effect.clone()),
            "{effect:?} was done where the loop cannot end the session"
        );
    }
    for effect in [
        Effect::Grouping(Grouping::Together),
        Effect::Pins(Vec::new()),
        Effect::ShowMore,
    ] {
        assert_eq!(
            super::sessions_screen::carry_out(&mut here.session, effect.clone()),
            None,
            "{effect:?} was handed on"
        );
    }
}

/// SCREEN-1. The command opens the Sessions screen as the key does, at rest and while a turn runs, and takes
/// nothing after the word into the box or a turn.
#[test]
fn the_sessions_screen_command_opens_the_sessions_screen_as_the_key_does() {
    use crate::command_table::SESSIONS_COMMAND;

    let mut session = Session::new("kernel-enforced");
    let commanded = crate::state::Commanded {
        line: SESSIONS_COMMAND.to_string(),
        pasted: Vec::new(),
        attached: Vec::new(),
        typed_at: std::time::Instant::now(),
    };
    assert_eq!(
        dispatch_command(&mut session, commanded),
        Action::OpenCenter
    );
    assert_eq!(session.input(), "");

    let table = crate::command_table::commands();
    let command = table
        .iter()
        .find(|command| command.name == SESSIONS_COMMAND)
        .expect("the command is in the table");
    assert_eq!(command.mid_turn, crate::command_table::MidTurn::Runs);
    assert!(command.argument.is_empty(), "the command takes nothing");
}

/// SCREEN-1. A half-typed command word is completed to it, like any other.
#[test]
fn the_sessions_screen_command_is_offered_while_it_is_typed() {
    let mut session = Session::new("kernel-enforced");
    type_in(&mut session, "/sess");
    assert!(session.is_completing());
    let action = handle_key(&mut session, press(KeyCode::Tab));
    assert_eq!(action, Action::Redraw);
    assert!(
        session.input().starts_with("/sessions"),
        "{:?}",
        session.input()
    );
}

// ---- the pointer

use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

/// The frame of `session` drawn at `columns` by `rows`, with its layout recorded as the loop does.
fn drawn(session: &mut Session, columns: u16, rows: u16) -> ratatui::buffer::Buffer {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(columns, rows))
        .expect("a terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = crate::render::draw(frame, session))
        .expect("the frame draws");
    session.note_layout(laid);
    terminal.backend().buffer().clone()
}

/// The first cell of the first row of `buffer` that begins `text`.
fn cell_of(buffer: &ratatui::buffer::Buffer, text: &str) -> (u16, u16) {
    let area = buffer.area;
    for row in 0..area.height {
        // The list only: the details beside it repeat the name of the task under the cursor.
        let line: Vec<&str> = (0..area.width.min(76))
            .map(|column| buffer[(column, row)].symbol())
            .collect();
        let joined = line.concat();
        if let Some(at) = joined.find(text) {
            let column = line
                .iter()
                .scan(0, |bytes, symbol| {
                    let start = *bytes;
                    *bytes += symbol.len();
                    Some(start)
                })
                .position(|start| start == at)
                .expect("a cell");
            return (row, column as u16);
        }
    }
    panic!("{text:?} is not on the screen");
}

fn mouse(kind: MouseEventKind, row: u16, column: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// A left press and its release on one cell, which is a click, and what the release asked for.
fn click(session: &mut Session, row: u16, column: u16) -> Action {
    handle_mouse(
        session,
        mouse(MouseEventKind::Down(MouseButton::Left), row, column),
    );
    handle_mouse(
        session,
        mouse(MouseEventKind::Up(MouseButton::Left), row, column),
    )
}

fn selected_title(session: &Session) -> String {
    session
        .sessions_screen()
        .and_then(|sessions_screen| sessions_screen.selected_row())
        .map(|row| row.title.clone())
        .unwrap_or_default()
}

/// SCREEN-12. A click on a tab chooses it, and a click on a row puts the cursor on it and does
/// nothing else.
#[test]
fn a_click_on_a_tab_chooses_it_and_a_click_on_a_row_puts_the_cursor_on_it() {
    let mut session = opened_over(vec![
        task("a", TaskStatus::Saved),
        task("b", TaskStatus::Working),
        task("c", TaskStatus::NeedsInput),
    ]);
    let buffer = drawn(&mut session, 120, 30);

    let (row, column) = cell_of(&buffer, "Working 1");
    assert_eq!(click(&mut session, row, column + 2), Action::Redraw);
    assert_eq!(session.sessions_screen().expect("open").tab(), 2);
    let buffer = drawn(&mut session, 120, 30);
    assert!(
        (0..30).all(|row| cell_of_opt(&buffer, row).is_none_or(|line| !line.contains("title a"))),
        "a task outside the tab is still listed"
    );

    let (row, column) = cell_of(&buffer, "All 3");
    click(&mut session, row, column + 1);
    assert_eq!(session.sessions_screen().expect("open").tab(), 0);
    let buffer = drawn(&mut session, 120, 30);
    let (row, column) = cell_of(&buffer, "title b");
    let action = click(&mut session, row, column);
    assert_eq!(
        action,
        Action::Redraw,
        "a single click asked for more than the cursor"
    );
    assert_eq!(selected_title(&session), "title b");
}

fn cell_of_opt(buffer: &ratatui::buffer::Buffer, row: u16) -> Option<String> {
    let area = buffer.area;
    (row < area.height).then(|| {
        (0..area.width)
            .map(|column| buffer[(column, row)].symbol())
            .collect()
    })
}

/// SCREEN-12. A second click on a row opens it, as Enter does, and a second click on "Show more"
/// asks for more.
#[test]
fn a_second_click_on_a_row_opens_it_as_enter_does() {
    let mut session = opened_over(vec![
        task("a", TaskStatus::Saved),
        task("b", TaskStatus::Working),
    ]);
    let buffer = drawn(&mut session, 120, 30);
    let (row, column) = cell_of(&buffer, "title a");

    let first = click(&mut session, row, column);
    let second = click(&mut session, row, column);

    assert_eq!(first, Action::Redraw);
    assert!(
        matches!(&second, Action::SessionsScreen(Effect::Open(opened)) if opened.id == "a"),
        "{second:?}"
    );
}

/// SCREEN-12. A press on a heading, on the line between two groups, on the header or below the last
/// row does nothing.
#[test]
fn a_press_that_is_on_no_row_does_nothing() {
    let mut session = opened_over(vec![
        task("a", TaskStatus::Saved),
        task("b", TaskStatus::Working),
    ]);
    let buffer = drawn(&mut session, 120, 30);
    let before = selected_title(&session);

    for (row, column) in [
        cell_of(&buffer, "Sessions"),
        cell_of(&buffer, "Updated"),
        (28, 10),
        (3, 90),
    ] {
        let action = click(&mut session, row, column);
        assert_ne!(
            action,
            Action::Redraw,
            "({row}, {column}) was taken for a row or a tab"
        );
        assert!(!matches!(action, Action::SessionsScreen(_)));
    }
    assert_eq!(selected_title(&session), before);
}

/// SCREEN-12. The wheel walks the cursor and stops at the first row and the last, and it scrolls
/// nothing else.
#[test]
fn the_wheel_walks_the_cursor_and_stops_at_the_ends() {
    let mut session = opened_over(
        (0..6)
            .map(|at| task(&format!("{at}"), TaskStatus::Saved))
            .collect(),
    );
    drawn(&mut session, 120, 30);
    session.adopt_wheel_rows(Some(2));
    let first = selected_title(&session);

    handle_mouse(&mut session, mouse(MouseEventKind::ScrollDown, 10, 10));
    let one = selected_title(&session);
    for _ in 0..5 {
        handle_mouse(&mut session, mouse(MouseEventKind::ScrollDown, 10, 10));
    }
    let last = selected_title(&session);
    handle_mouse(&mut session, mouse(MouseEventKind::ScrollDown, 10, 10));
    assert_eq!(
        selected_title(&session),
        last,
        "the wheel wrapped past the last row"
    );
    for _ in 0..8 {
        handle_mouse(&mut session, mouse(MouseEventKind::ScrollUp, 10, 10));
    }

    assert_ne!(one, first, "a turn of the wheel did not move the cursor");
    assert_eq!(last, "title 5");
    assert_eq!(
        selected_title(&session),
        first,
        "the wheel wrapped past the first row"
    );
}

/// SCREEN-12. Nothing the plain frame drew stands under the Sessions screen: every target recorded for the
/// frame is the Sessions screen's, and a press where the box was reaches nothing.
#[test]
fn a_press_on_the_sessions_screen_reaches_nothing_beneath_it() {
    let mut session = Session::new("kernel-enforced");
    drawn(&mut session, 120, 30);
    assert!(
        session
            .laid
            .targets
            .iter()
            .any(|target| !matches!(target.hit, crate::state::Hit::SessionsScreen(_))),
        "the plain frame has no targets to be covered"
    );
    let mut sessions_screen = crate::sessions_screen::SessionsScreen::new(
        Grouping::Together,
        Vec::new(),
        std::collections::HashMap::new(),
        None,
    );
    sessions_screen.replace(Page {
        tasks: vec![task("a", TaskStatus::Saved)],
        more: false,
    });
    session.open_sessions_screen(sessions_screen);

    // Before a frame is drawn, the targets of the plain frame are still recorded.
    assert!(
        session
            .target_at(28, 10)
            .is_none_or(|target| matches!(target.hit, crate::state::Hit::SessionsScreen(_)))
    );
    drawn(&mut session, 120, 30);

    assert!(
        session
            .laid
            .targets
            .iter()
            .all(|target| matches!(target.hit, crate::state::Hit::SessionsScreen(_))),
        "a target of the plain frame is recorded under the Sessions screen"
    );
}

/// SCREEN-10. A task hidden stays hidden when the Sessions screen is closed and opened again in the same
/// session, and a session of its own starts with nothing hidden.
#[test]
fn a_hidden_task_stays_hidden_the_next_time_the_sessions_screen_is_opened_in_this_session() {
    let tasks = || vec![task("a", TaskStatus::Saved), task("b", TaskStatus::Saved)];
    let sessions_screen = || {
        let mut sessions_screen = SessionsScreen::new(
            Grouping::Together,
            Vec::new(),
            std::collections::HashMap::new(),
            None,
        );
        sessions_screen.replace(Page {
            tasks: tasks(),
            more: false,
        });
        sessions_screen
    };
    let mut session = Session::new("kernel-enforced");
    session.open_sessions_screen(sessions_screen());
    let hidden = selected_title(&session);

    handle_key(&mut session, press(KeyCode::Char('h')));
    handle_key(&mut session, press(KeyCode::Esc));
    assert!(!session.showing_sessions());
    session.open_sessions_screen(sessions_screen());

    let listed: Vec<String> = session
        .sessions_screen()
        .expect("open")
        .rows()
        .iter()
        .map(|row| row.title.clone())
        .collect();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert!(!listed.contains(&hidden), "a hidden task is listed again");

    let mut another = Session::new("kernel-enforced");
    another.open_sessions_screen(sessions_screen());
    assert_eq!(another.sessions_screen().expect("open").rows().len(), 2);
}

/// SCREEN-12. While a question is up, a click or a turn of the wheel leaves the list as it is, so
/// the answer is to the question that was asked and not to a row clicked since.
#[test]
fn a_click_or_the_wheel_while_a_question_is_up_leaves_the_list_alone() {
    let mut session = opened_over(vec![
        task("a", TaskStatus::Saved),
        task("b", TaskStatus::Working),
    ]);
    let first = selected_title(&session);
    handle_key(&mut session, press(KeyCode::Char('a')));
    assert!(
        session
            .sessions_screen()
            .and_then(|sessions_screen| sessions_screen.confirming())
            .is_some()
    );
    let buffer = drawn(&mut session, 120, 30);
    let other = if first == "title a" {
        "title b"
    } else {
        "title a"
    };
    let (row, column) = cell_of(&buffer, other);

    click(&mut session, row, column);
    let second = click(&mut session, row, column);
    handle_mouse(&mut session, mouse(MouseEventKind::ScrollDown, 10, 10));

    assert_eq!(
        selected_title(&session),
        first,
        "a click moved the cursor under a question"
    );
    assert!(
        !matches!(second, Action::SessionsScreen(_)),
        "a second click opened a row under a question"
    );
    assert!(
        session
            .sessions_screen()
            .and_then(|sessions_screen| sessions_screen.confirming())
            .is_some()
    );
}

/// SCREEN-1. While the Sessions screen is open over a turn, Escape and Ctrl-C are the Sessions screen's and not the
/// turn's: neither stops the turn, and once the Sessions screen is put away the press after it does.
#[test]
fn escape_and_ctrl_c_over_a_running_turn_leave_the_sessions_screen_and_do_not_stop_it() {
    use super::keys::{stops_a_command, stops_the_turn};

    for leave in [
        press(KeyCode::Esc),
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    ] {
        let mut session = Session::new("kernel-enforced");
        session.status = crate::state::Status::Working;
        assert!(
            stops_the_turn(&session, leave),
            "{leave:?} does not stop a turn at all"
        );
        session.open_sessions_screen(crate::sessions_screen::fixtures::sessions_screen_of(vec![
            task("a", TaskStatus::Saved),
        ]));

        assert!(
            !stops_the_turn(&session, leave),
            "{leave:?} would stop the turn under the Sessions screen"
        );
        assert!(
            !stops_a_command(&session, leave),
            "{leave:?} would stop a command under the Sessions screen"
        );
        handle_key_while_working(&mut session, leave);

        assert!(
            !session.showing_sessions(),
            "{leave:?} did not leave the Sessions screen"
        );
        assert!(
            stops_the_turn(&session, leave),
            "{leave:?} no longer stops the turn once it is left"
        );
    }
}

/// SCREEN-14. The keys a settings file moved reach the screen through the settings a session reads
/// once at startup, and a key it asked for that cannot be had is said when the screen opens.
#[test]
fn the_keys_a_settings_file_moved_reach_the_screen_through_the_adopted_settings() {
    use crate::sessions_keys::Action;
    let settings = bravebot_config::Settings::parse(
        r#"{"sessionsKeys": {"stop": "ctrl-x", "archive": "tab"}}"#,
    );
    let mut session = Session::new("none");
    super::adopt_settings(&mut session, &settings, None);
    session.open_sessions_screen(crate::sessions_screen::fixtures::sessions_screen_of(
        Vec::new(),
    ));

    let screen = session.sessions_screen().expect("the screen is open");
    assert_eq!(screen.keys().name(Action::Stop), "ctrl-x");
    assert_eq!(screen.keys().name(Action::Archive), "a");
    let notice = screen.notice().expect("what was refused is said");
    assert!(
        notice.contains("archive") && notice.contains("tab"),
        "{notice:?}"
    );
}

/// The rows of `buffer` that hold a session of `scrolled_list`, as the row, the column and the id.
fn drawn_titles(buffer: &ratatui::buffer::Buffer) -> Vec<(u16, u16, String)> {
    let mut found = Vec::new();
    for row in 0..buffer.area.height {
        let line: String = (0..buffer.area.width.min(76))
            .map(|column| buffer[(column, row)].symbol())
            .collect();
        if let Some(at) = line.find("title ") {
            let id: String = line[at + 6..].chars().take(2).collect();
            // The column in cells, which is not the byte the match is at past a marker.
            found.push((
                row,
                u16::try_from(line[..at].chars().count()).unwrap_or(0),
                id,
            ));
        }
    }
    found
}

/// `count` saved sessions named `title 00`, `title 01` and so on, with the Sessions screen open over
/// them, and the cursor sent to the end when `scrolled` so that the list has slid.
fn scrolled_list(count: usize, scrolled: bool) -> Session {
    let mut session = opened_over(
        (0..count)
            .map(|at| task(&format!("{at:02}"), TaskStatus::Saved))
            .collect(),
    );
    drawn(&mut session, 120, 16);
    if scrolled {
        handle_key(&mut session, press(KeyCode::End));
    }
    session
}

/// MOUSE-15. A double click in place opens the session drawn at the cell, on every row of a list
/// that has slid as well as one that has not, and the first press does not slide the list.
#[test]
fn a_double_click_in_place_opens_the_session_pointed_at_on_every_row_of_a_list_scrolled_or_not() {
    for (scrolled, count) in [(false, 3), (true, 30)] {
        let before = drawn_titles(&drawn(&mut scrolled_list(count, scrolled), 120, 16));
        assert!(before.len() >= 3, "{before:?}");
        assert_eq!(
            before.len() < count,
            scrolled,
            "the list is not scrolled as the case says: {before:?}"
        );
        for (row, column, id) in &before {
            let mut session = scrolled_list(count, scrolled);
            drawn(&mut session, 120, 16);

            let first = click(&mut session, *row, *column);
            assert_eq!(first, Action::Redraw, "scrolled {scrolled}, {id}");
            assert_eq!(selected_title(&session), format!("title {id}"));
            let after_the_press = drawn_titles(&drawn(&mut session, 120, 16));
            let second = click(&mut session, *row, *column);

            assert!(
                matches!(&second, Action::SessionsScreen(Effect::Open(opened)) if opened.id == *id),
                "scrolled {scrolled}, {id}: {second:?}"
            );
            assert_eq!(
                after_the_press, before,
                "scrolled {scrolled}, {id}: the press slid the list"
            );
        }
    }
}

/// MOUSE-15. A press where a turn of the wheel has since drawn another session is a first press on
/// it and opens nothing, though it is on the cell of the press before.
#[test]
fn a_press_where_the_wheel_has_since_drawn_another_session_is_a_first_press_on_it() {
    let mut session = scrolled_list(40, true);
    let before = drawn_titles(&drawn(&mut session, 120, 16));
    let (row, column, was) = before.last().cloned().expect("a row");

    let first = click(&mut session, row, column);
    assert_eq!(first, Action::Redraw);
    session.adopt_wheel_rows(Some(25));
    handle_mouse(&mut session, mouse(MouseEventKind::ScrollUp, 10, 10));
    let now = drawn_titles(&drawn(&mut session, 120, 16));
    let (_, _, there) = now
        .iter()
        .find(|(r, _, _)| *r == row)
        .cloned()
        .expect("a row");
    assert_ne!(
        there, was,
        "the wheel did not draw another session at the cell"
    );

    let second = click(&mut session, row, column);

    assert_eq!(
        second,
        Action::Redraw,
        "it opened the session the wheel drew"
    );
    assert_eq!(selected_title(&session), format!("title {there}"));
}

/// MOUSE-15. A second press on "Show more" asks for more in the loop that has the mouse, whichever
/// it is, and does not hand the effect on to a loop that cannot do it.
#[test]
fn a_second_click_on_show_more_asks_for_more_where_the_click_is_taken() {
    let mut session = opened_over(
        (0..3)
            .map(|at| task(&format!("{at:02}"), TaskStatus::Saved))
            .collect(),
    );
    session.sessions_screen_mut().expect("open").replace(Page {
        tasks: (0..3)
            .map(|at| task(&format!("{at:02}"), TaskStatus::Saved))
            .collect(),
        more: true,
    });
    let buffer = drawn(&mut session, 120, 16);
    let (row, column) = cell_of(&buffer, "Show more");
    let asked = || session.sessions_screen().map(|screen| screen.wanted());
    assert_eq!(asked(), Some(crate::sessions_screen::PAGE));

    assert_eq!(click(&mut session, row, column), Action::Redraw);
    let second = click(&mut session, row, column);

    assert_eq!(
        second,
        Action::Redraw,
        "the effect was handed to another loop"
    );
    assert_eq!(
        session.sessions_screen().map(|screen| screen.wanted()),
        Some(2 * crate::sessions_screen::PAGE)
    );
}
