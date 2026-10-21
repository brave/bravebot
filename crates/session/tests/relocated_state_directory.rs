#![cfg(unix)]
//! What a person gets by naming the state directory in `BRAVEBOT_HOME`: the prompt history and the
//! session records go into it, reachable by them alone, and nothing is written under the profile
//! directory (STATE-1, STATE-2).
//!
//! These run against real directories, because a mode is a property of one on disk, and each test
//! reads the mode back rather than trusting that the write happened where it was meant to. `HOME`
//! and `BRAVEBOT_HOME` are process-wide, so the tests hold one lock for as long as they own them.

use bravebot_agent::Conversation;
use bravebot_aichat::protocol::Message;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::trust::TrustStore;
use bravebot_session::sessions::{Front, Handle, Standing};
use bravebot_session::store::{self, Entry};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// The variable that names the state directory itself.
const STATE_VARIABLE: &str = "BRAVEBOT_HOME";

/// A scratch profile directory whose state directory is `elsewhere/state` inside it, so that the
/// directory above the state directory is one this test made and can read back.
struct Relocated {
    home: PathBuf,
    state: PathBuf,
    /// Dropped last, releasing the variables only once this test is finished with them.
    _lock: MutexGuard<'static, ()>,
}

impl Relocated {
    fn new(name: &str) -> Self {
        let lock = ENVIRONMENT.lock().unwrap_or_else(|held| held.into_inner());
        // Under the workspace build directory rather than the system temporary one, which is
        // shared between users and is the wrong place to prove something about privacy in.
        let home = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(format!("relocated-state-{name}"));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("create home");
        let state = home.join("elsewhere").join("state");
        // SAFETY: the lock is held for as long as this value lives, so no other test in this binary
        // is reading or writing either variable while this one owns them.
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var(STATE_VARIABLE, &state);
        }
        Self {
            home,
            state,
            _lock: lock,
        }
    }

    /// Leave the state directory, and the directory above it, open as a build with no opinion
    /// about modes would have.
    fn as_an_older_build_left_it(&self) {
        std::fs::create_dir_all(&self.state).expect("seed the directory");
        std::fs::write(
            self.state.join("history"),
            "1700000000\t\tan earlier prompt\n",
        )
        .expect("seed a file");
        for open in [self.state.parent().expect("a parent"), &self.state] {
            std::fs::set_permissions(open, std::fs::Permissions::from_mode(0o755)).expect("loosen");
        }
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
            .permissions()
            .mode()
            & 0o777
    }
}

impl Drop for Relocated {
    fn drop(&mut self) {
        // SAFETY: the lock is still held, since fields are dropped after this runs.
        unsafe { std::env::remove_var(STATE_VARIABLE) };
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// STATE-1, STATE-2: a person who names the state directory has the prompt history written there
/// with the modes it has under `~/.bravebot`. Nothing is written under the profile directory, which
/// a writer that kept to the old location would leave readable at the umask in a directory nobody
/// was told about.
#[test]
fn the_history_is_written_to_the_named_directory_readable_only_by_its_owner() {
    let scratch = Relocated::new("history");

    store::append_history(&Entry::sent("a prompt worth keeping private", None));

    assert_eq!(Relocated::mode_of(&scratch.state.join("history")), 0o600);
    assert_eq!(
        Relocated::mode_of(&scratch.state),
        0o700,
        "the directory holding it"
    );
    assert!(
        !scratch.home.join(".bravebot").exists(),
        "the history was written under the profile directory as well"
    );
}

/// STATE-1: narrowing walks up from what was written as far as the state directory, and the one a
/// person named is that directory. A bound still read as `~/.bravebot` would narrow nothing here, or
/// every directory up to the profile directory, which holds whatever else the person keeps there.
#[test]
fn a_named_directory_an_older_build_left_open_is_narrowed_and_nothing_above_it() {
    let scratch = Relocated::new("older-build");
    scratch.as_an_older_build_left_it();
    let above = scratch.state.parent().expect("a parent");
    assert_eq!(
        Relocated::mode_of(&scratch.state),
        0o755,
        "the test seeded nothing to narrow"
    );

    store::save_history(&[Entry::sent("a later prompt", None)]);

    assert_eq!(Relocated::mode_of(&scratch.state), 0o700);
    assert_eq!(Relocated::mode_of(&scratch.state.join("history")), 0o600);
    assert_eq!(
        Relocated::mode_of(above),
        0o755,
        "the directory the state directory sits in"
    );
}

/// STATE-2: a session record goes to the directory a person named too, and is found again there.
/// The session store resolves the directory through the same helper as the history, and a record
/// left under `~/.bravebot` would be a conversation the next run, given the same variable, cannot
/// resume.
#[test]
fn a_session_is_written_to_the_named_directory() {
    let scratch = Relocated::new("session");
    let project = scratch.home.join("project");
    std::fs::create_dir_all(&project).expect("create project");

    let mut conversation = Conversation::new();
    conversation.push(Message::user("a prompt worth keeping private"));
    let mut handle = Handle::begin(&project, Front::Terminal, "a-test-build");
    handle.save(
        "a prompt worth keeping private",
        Standing {
            history: None,
            conversation: &conversation.snapshot(),
            turns: 1,
            tokens: 1_200,
            spend: &BTreeMap::new(),
            timing: &BTreeMap::new(),
            model: None,
            todos: &BTreeMap::new(),
            asides: &[],
            trust: &TrustStore::new("/work"),
            programs: &TrustedPrograms::new(),
            directories: &[],
            manifest: None,
            rewind: &[],
            checkouts: &[],
        },
    );

    assert!(
        handle.resumable().is_some(),
        "no session was written, so this test proves nothing"
    );
    assert_eq!(Relocated::mode_of(&scratch.state.join("sessions")), 0o700);
    assert!(
        !scratch.home.join(".bravebot").exists(),
        "the session was written under the profile directory"
    );
}
