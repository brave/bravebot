//! Which theme a stored name means, and where a theme file is looked for.
//!
//! Points HOME and the working directory at scratch directories, so a developer's own themes are
//! never read and a workspace the process happens to run in decides nothing.

use bravebot_session::store;
use bravebot_tui::theme;
use std::path::Path;
use std::sync::Mutex;

/// One lock for the file: HOME, the working directory and the theme in force are process-wide.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

const A_THEME: &str = "{\"primary\": \"#123456\"}";

/// HOME and the working directory as they were, put back when a test ends or panics, so one
/// failing test does not leave the next in a directory that is gone.
struct Restored {
    home: Option<std::ffi::OsString>,
    directory: std::path::PathBuf,
}

impl Drop for Restored {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.directory);
        // SAFETY: still inside the lock `started_in` holds.
        match self.home.take() {
            Some(value) => unsafe { std::env::set_var("HOME", value) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}

/// Run the body with HOME at `home` and the working directory at `workspace`, as a session started
/// in a checkout has them.
fn started_in(home: &Path, workspace: &Path, body: impl FnOnce()) {
    let _guard = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _restored = Restored {
        home: std::env::var_os("HOME"),
        directory: std::env::current_dir().expect("a working directory"),
    };

    // SAFETY: single-threaded within the lock, and restored when `_restored` is dropped.
    unsafe { std::env::set_var("HOME", home) };
    std::env::set_current_dir(workspace).expect("enter the workspace");

    body();
}

fn a_theme_file_in(directory: &Path, name: &str) {
    std::fs::create_dir_all(directory).expect("create the themes directory");
    std::fs::write(directory.join(format!("{name}.json")), A_THEME).expect("write the theme");
}

fn names_on_offer() -> Vec<String> {
    theme::offered().into_iter().map(|t| t.name).collect()
}

/// A workspace's `.bravebot/themes` is workspace content, which a person did not write. A palette
/// read from it would be a decision taken from untrusted bytes.
#[test]
fn a_theme_file_in_the_workspace_is_not_a_theme() {
    let home = tempfile::tempdir().expect("a home");
    let workspace = tempfile::tempdir().expect("a workspace");
    started_in(home.path(), workspace.path(), || {
        a_theme_file_in(&home.path().join(".bravebot").join("themes"), "mine");
        a_theme_file_in(&workspace.path().join(".bravebot").join("themes"), "theirs");

        let offered = names_on_offer();
        assert!(offered.contains(&"mine".to_string()), "{offered:?}");
        assert!(!offered.contains(&"theirs".to_string()), "{offered:?}");
        assert!(theme::find("theirs").is_none());
    });
}

/// The directory a person's own theme files are read from is `themes` inside the state directory.
#[test]
fn user_theme_files_are_read_from_the_themes_directory_of_the_state_directory() {
    let home = tempfile::tempdir().expect("a home");
    let workspace = tempfile::tempdir().expect("a workspace");
    started_in(home.path(), workspace.path(), || {
        assert_eq!(
            theme::user_themes_directory(),
            Some(home.path().join(".bravebot").join("themes"))
        );
    });
}

/// A choice saved under a name nothing offers is `brave`, whatever was in force before. Leaving
/// the previous theme in force would draw the session in a palette the person did not pick.
#[test]
fn a_stored_name_nothing_offers_is_brave() {
    let home = tempfile::tempdir().expect("a home");
    let workspace = tempfile::tempdir().expect("a workspace");
    started_in(home.path(), workspace.path(), || {
        let themes = home.path().join(".bravebot").join("themes");
        a_theme_file_in(&themes, "mine");
        a_theme_file_in(&workspace.path().join(".bravebot").join("themes"), "theirs");

        for (stored, resolves_to) in [
            ("mine", "mine"),
            ("theirs", theme::BRAVE),
            ("no-such-theme", theme::BRAVE),
            ("system", theme::BRAVE),
        ] {
            theme::apply(&theme::find("nord").expect("a built-in theme"));
            assert_eq!(theme::name(), "nord");

            store::save_theme(stored);
            theme::restore_saved();
            assert_eq!(theme::name(), resolves_to, "a stored name of {stored:?}");
        }

        std::fs::remove_file(themes.join("mine.json")).expect("remove the theme");
        theme::apply(&theme::find("nord").expect("a built-in theme"));
        store::save_theme("mine");
        theme::restore_saved();
        assert_eq!(theme::name(), theme::BRAVE, "a theme that is gone");
    });
}
