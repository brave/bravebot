//! Taking the prompt through a real editor.
//!
//! `$VISUAL` and `$EDITOR` are process-wide, so these tests are the only ones in the crate that
//! touch them and they are kept in a file of their own, in the same arrangement `HOME` gets.
//!
//! The editors here are ordinary programs standing in for one: `cp` saves over the file it was
//! given, `true` leaves without saving, `false` refuses the way `vi`'s `:cq` does. What is being
//! tested is the whole path a user takes, spawning included, which the unit tests cannot reach.

#![cfg(unix)]

use std::path::PathBuf;
use std::sync::Mutex;

/// One lock for the whole file, not one per test.
///
/// Both variables are process-wide, so every test here contends for the same thing. A mutex
/// declared inside each function would be a different mutex, and two tests would then be free to
/// run at once and see each other's editor.
static EDITOR_LOCK: Mutex<()> = Mutex::new(());

/// Run `body` with `$EDITOR` set to `command` and `$VISUAL` out of the way.
fn with_editor<T>(command: &str, body: impl FnOnce() -> T) -> T {
    with_both(None, Some(command), body)
}

/// Run `body` with both variables set to whatever is given.
fn with_both<T>(visual: Option<&str>, editor: Option<&str>, body: impl FnOnce() -> T) -> T {
    let _guard = EDITOR_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let previous = [
        ("VISUAL", std::env::var_os("VISUAL")),
        ("EDITOR", std::env::var_os("EDITOR")),
    ];
    // SAFETY: single-threaded within the lock, and restored before returning.
    unsafe {
        set("VISUAL", visual);
        set("EDITOR", editor);
    }

    let result = body();

    for (name, value) in previous {
        match value {
            Some(value) => unsafe { std::env::set_var(name, value) },
            None => unsafe { std::env::remove_var(name) },
        }
    }
    result
}

/// SAFETY: callers hold the lock and restore what was there.
unsafe fn set(name: &str, value: Option<&str>) {
    match value {
        Some(value) => unsafe { std::env::set_var(name, value) },
        None => unsafe { std::env::remove_var(name) },
    }
}

/// A program that saves `contents` over the file it is given, which is what an editor does.
fn saves(contents: &str, name: &str) -> PathBuf {
    let source =
        std::env::temp_dir().join(format!("bravebot-editor-{}-{name}", std::process::id()));
    std::fs::write(&source, contents).expect("scratch source");
    source
}

/// The whole point of the round trip: the line goes out to an editor and comes back changed.
#[test]
fn what_the_editor_saved_becomes_the_line() {
    let source = saves("the considered version\n", "saved");
    let edited = with_editor(&format!("cp {}", source.display()), || {
        bravebot_tui::editor::edit("the first thing that came to mind")
    })
    .expect("the editor ran");

    assert_eq!(edited, "the considered version");
    std::fs::remove_file(&source).ok();
}

/// Quitting without saving must not blank the prompt. The file already holds the line, so what
/// comes back is what went in: not saving costs the edits, never the prompt.
#[test]
fn an_editor_that_saved_nothing_gives_the_line_back() {
    let edited = with_editor("true", || bravebot_tui::editor::edit("half a thought"))
        .expect("the editor ran");

    assert_eq!(edited, "half a thought");
}

/// `vi` spells "discard this" as `:cq`, which exits 1. Honouring it is the difference between an
/// editor that can be abandoned and one whose window is a commitment.
#[test]
fn an_editor_that_refuses_leaves_the_line_alone() {
    let failure = with_editor("false", || bravebot_tui::editor::edit("half a thought"))
        .expect_err("a non-zero exit is not an edit");

    assert!(
        failure.to_string().contains("unchanged"),
        "said {failure}, which does not say the line survived"
    );
}

/// A configured editor is the user's answer to which one to use. Quietly running a different one
/// because theirs is not installed would edit their prompt with a program they did not choose,
/// and the message has to say why nothing happened rather than leave them guessing.
#[test]
fn a_configured_editor_that_is_missing_is_not_replaced_by_a_guess() {
    let failure = with_editor("no-such-editor-anywhere", || {
        bravebot_tui::editor::edit("a line")
    })
    .expect_err("a missing editor is not an edit");

    let said = failure.to_string();
    assert!(said.contains("was not found"), "said {said}");
    assert!(said.contains("nothing else was tried"), "said {said}");
}

/// A path with a space in it is a path. Most of `/Applications` has one, and reading the first
/// space as the end of the program would send a user's editor a file it never opened.
#[test]
fn an_editor_path_with_a_space_in_it_is_one_program() {
    let editor = std::env::temp_dir().join(format!(
        "bravebot editor with spaces {}.sh",
        std::process::id()
    ));
    std::fs::write(&editor, "#!/bin/sh\nprintf 'edited\\n' > \"$1\"\n").expect("scratch editor");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).expect("mode");

    let edited = with_editor(&editor.display().to_string(), || {
        bravebot_tui::editor::edit("a line")
    })
    .expect("the editor ran");

    assert_eq!(edited, "edited");
    std::fs::remove_file(&editor).ok();
}

/// `$VISUAL` is what the user wants when the terminal can do more than print a line at a time,
/// which is every terminal this runs in, so it answers before `$EDITOR` does.
#[test]
fn visual_is_the_one_that_runs() {
    let wanted = saves("from visual\n", "visual");
    let unwanted = saves("from editor\n", "editor");

    let edited = with_both(
        Some(&format!("cp {}", wanted.display())),
        Some(&format!("cp {}", unwanted.display())),
        || bravebot_tui::editor::edit("a line"),
    )
    .expect("the editor ran");

    assert_eq!(edited, "from visual");
    std::fs::remove_file(&wanted).ok();
    std::fs::remove_file(&unwanted).ok();
}

/// Every scratch file this makes, sorted, so two listings can be compared.
///
/// The name is generated inside the module under test, so a caller cannot know it in advance and
/// watches the directory instead. Only this process's files count: the name carries the pid, and
/// the temporary directory is shared with every other test process running at the same time,
/// whose edits the editor lock here cannot hold off.
fn scratch_files() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return Vec::new();
    };
    let pid = std::process::id();
    let prefixes = [
        format!("bravebot-prompt-{pid}-"),
        format!("bravebot-transcript-{pid}-"),
    ];
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| prefixes.iter().any(|prefix| name.starts_with(prefix)))
        })
        .collect();
    found.sort();
    found
}

/// The prompt does not outlive the edit, down any of the three ways out of one.
///
/// Snapshotted inside the closure because that is where the editor lock is held: another test in
/// this binary part-way through its own edit would otherwise look like a file left behind.
///
/// Worth pinning on its own account, and named by INCOG-8, which states this hand-off as the one
/// thing an incognito session still writes and leans on it being gone by the time the editor
/// closes.
#[test]
fn the_scratch_file_does_not_outlive_the_edit() {
    with_editor("true", || {
        let before = scratch_files();
        let _ = bravebot_tui::editor::edit("something private");
        assert_eq!(
            scratch_files(),
            before,
            "a prompt the editor did not save was left in the temporary directory"
        );
    });

    with_editor("false", || {
        let before = scratch_files();
        let _ = bravebot_tui::editor::edit("something private");
        assert_eq!(
            scratch_files(),
            before,
            "a prompt was left behind when the editor refused"
        );
    });

    with_editor("true", || {
        let before = scratch_files();
        let _ = bravebot_tui::editor::show("a transcript worth not keeping");
        assert_eq!(
            scratch_files(),
            before,
            "a transcript was left in the temporary directory"
        );
    });
}

/// A directory holding a script named `name` that writes the arguments it is given, one to a line,
/// into `record`, and then writes `then` after them. It stands in for the editor of that name.
fn an_editor_named(name: &str, scratch: &str, then: &str) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!(
        "bravebot-editor-{}-{scratch}-directory",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a directory");
    let record = directory.join("record");
    let editor = directory.join(name);
    std::fs::write(
        &editor,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n{then}\n",
            record.display()
        ),
    )
    .expect("a script");
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).expect("mode");
    (editor, record)
}

/// What the script was given, one argument to a line.
fn given(record: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(record)
        .expect("the editor ran")
        .lines()
        .map(str::to_string)
        .collect()
}

/// A file opened from a click is handed to the user's editor in that editor's words for a line: the
/// whole path a person takes, spawning included, which the unit test of the table cannot reach.
#[test]
fn a_file_is_opened_at_its_line_in_the_words_of_the_editor_that_takes_one() {
    let file = saves("one\ntwo\n", "placed");
    let shown = file.display().to_string();
    for (editor, expected) in [
        ("vim", vec!["+12".to_string(), shown.clone()]),
        ("nano", vec!["+12,5".to_string(), shown.clone()]),
        ("emacs", vec!["+12:5".to_string(), shown.clone()]),
        (
            "code",
            vec![
                "--wait".to_string(),
                "--goto".to_string(),
                format!("{shown}:12:5"),
            ],
        ),
        ("subl", vec!["--wait".to_string(), format!("{shown}:12:5")]),
        ("some-other-editor", vec![shown.clone()]),
    ] {
        let (script, record) = an_editor_named(editor, &format!("placed-{editor}"), "");
        with_editor(&script.display().to_string(), || {
            bravebot_tui::editor::open_file(&file, Some(12), Some(5))
        })
        .expect("the editor ran");
        assert_eq!(given(&record), expected, "{editor}");
        std::fs::remove_dir_all(script.parent().expect("a directory")).ok();
    }
    std::fs::remove_file(&file).ok();
}

/// With no line, the editor is handed the file alone, and a column without a line is no place.
#[test]
fn a_file_with_no_line_is_opened_at_the_top() {
    let file = saves("one\n", "unplaced");
    let shown = file.display().to_string();
    for (line, column) in [(None, None), (None, Some(5))] {
        let (script, record) = an_editor_named("vim", "unplaced", "");
        with_editor(&script.display().to_string(), || {
            bravebot_tui::editor::open_file(&file, line, column)
        })
        .expect("the editor ran");
        assert_eq!(given(&record), vec![shown.clone()], "{line:?} {column:?}");
        std::fs::remove_dir_all(script.parent().expect("a directory")).ok();
    }
    std::fs::remove_file(&file).ok();
}

/// What the editor saved is what the file holds, and nothing is copied or read back: no scratch
/// file is made, and the file is the one the editor was handed.
#[test]
fn what_the_editor_saves_is_the_file_and_nothing_is_read_back() {
    let file = saves("one\n", "saved-in-place");
    let (script, _) = an_editor_named(
        "vim",
        "saved-in-place",
        &format!("printf 'two\\n' >> '{}'", file.display()),
    );
    with_editor(&script.display().to_string(), || {
        let before = scratch_files();
        bravebot_tui::editor::open_file(&file, Some(1), None).expect("the editor ran");
        assert_eq!(scratch_files(), before, "a scratch file was made");
    });
    assert_eq!(
        std::fs::read_to_string(&file).expect("a file"),
        "one\ntwo\n"
    );
    std::fs::remove_dir_all(script.parent().expect("a directory")).ok();
    std::fs::remove_file(&file).ok();
}

/// An editor that exits with a status is not said to have left the file unchanged, which it may not
/// have: the file is the person's and may have been saved before the editor was quit.
#[test]
fn an_editor_that_exits_badly_does_not_claim_the_file_is_unchanged() {
    let file = saves("one\n", "exits-badly");
    let failure = with_editor("false", || {
        bravebot_tui::editor::open_file(&file, Some(1), None)
    })
    .expect_err("a non-zero exit is reported");
    let said = failure.to_string();
    assert!(said.contains("exited with status"), "said {said}");
    assert!(!said.contains("unchanged"), "said {said}");
    std::fs::remove_file(&file).ok();
}

/// A configured editor that is not there is said so and is not replaced with another.
#[test]
fn a_file_is_not_opened_in_an_editor_nobody_chose() {
    let file = saves("one\n", "no-such-editor");
    let failure = with_editor("no-such-editor-anywhere", || {
        bravebot_tui::editor::open_file(&file, Some(1), None)
    })
    .expect_err("a missing editor does not open a file");
    let said = failure.to_string();
    assert!(said.contains("was not found"), "said {said}");
    assert!(said.contains("nothing else was tried"), "said {said}");
    std::fs::remove_file(&file).ok();
}

/// Whether `environment`, the output of `env`, holds a variable called `name`.
fn holds(environment: &str, name: &str) -> bool {
    environment
        .lines()
        .any(|line| line.starts_with(&format!("{name}=")))
}

/// The editor a ctrl-click starts is handed the person's environment without this agent's own
/// credentials, since the file it opens is one the workspace holds and a checkout may have written
/// ([CRED-14]). The editor the prompt is edited in is started with all of it, as the person's own
/// program. Both runs are given the same variables, so a name missing from the first is withheld and
/// not unset.
#[test]
fn an_editor_a_click_opens_is_not_handed_this_agents_credentials_and_the_prompt_editor_is() {
    let names = bravebot_config::scrub::builtin_credentials();
    assert!(!names.is_empty(), "no credential is withheld to be found");
    let file = saves("one\n", "environment");
    let directory = std::env::temp_dir().join(format!(
        "bravebot-editor-{}-environment-record",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a directory");
    let click = directory.join("click");
    let prompt = directory.join("prompt");
    let dump = |into: &std::path::Path| {
        let (script, _) = an_editor_named(
            "vim",
            &format!(
                "environment-{}",
                into.file_name().unwrap_or_default().to_string_lossy()
            ),
            &format!("env > '{}'", into.display()),
        );
        script
    };
    let for_click = dump(&click);
    let for_prompt = dump(&prompt);

    with_editor(&for_click.display().to_string(), || {
        for name in &names {
            // SAFETY: the lock is held and the variables are removed before it is let go.
            unsafe { std::env::set_var(name, "a-secret-value") };
        }
        bravebot_tui::editor::open_file(&file, Some(1), None).expect("the editor ran");
    });
    with_editor(&for_prompt.display().to_string(), || {
        bravebot_tui::editor::edit("a line").expect("the editor ran");
        for name in &names {
            // SAFETY: the lock is held.
            unsafe { std::env::remove_var(name) };
        }
    });

    let from_click = std::fs::read_to_string(&click).expect("the editor ran");
    let from_prompt = std::fs::read_to_string(&prompt).expect("the editor ran");
    for name in &names {
        assert!(
            !holds(&from_click, name),
            "the editor a click opened was handed {name}"
        );
        assert!(
            holds(&from_prompt, name),
            "the editor the prompt is edited in was not handed {name}, so the click's test shows nothing"
        );
    }
    assert!(
        holds(&from_click, "PATH") || holds(&from_click, "HOME"),
        "the editor a click opened was handed no environment at all"
    );
    for directory in [for_click, for_prompt] {
        std::fs::remove_dir_all(directory.parent().expect("a directory")).ok();
    }
    std::fs::remove_dir_all(&directory).ok();
    std::fs::remove_file(&file).ok();
}
