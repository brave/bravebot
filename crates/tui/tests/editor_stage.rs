//! The files the editor is handed, against a program that can write the system temporary directory.
//!
//! A stage of `run` is the same account as the session and may write the temporary directory
//! (SANDBOX-12), so it lists the directory and replaces what it finds. The editor here is a script
//! that plays both parts: it replaces every file under the temporary directory the way a stage does,
//! then records what it was shown, saves, and replaces again. What it is shown has to be the text the
//! person had, and what the session reads back has to be what the script saved.
//!
//! The environment is process-wide, so every test takes one lock and points `TMPDIR` and `HOME` at a
//! directory of its own for as long as it holds it.

#![cfg(unix)]
#![allow(
    clippy::expect_used,
    reason = "a test target fails by panicking, and a helper here cannot carry #[test]"
)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// What a test is run against: the directory the stage may write, the account's home, and the
/// directory the editor script and its records live in, which is under neither.
struct World {
    temporary: PathBuf,
    home: PathBuf,
    kept: PathBuf,
}

/// Run `body` with `TMPDIR` and `HOME` in a directory of its own and `EDITOR` the script `script`
/// names, restoring all of them afterwards.
fn in_a_world<T>(script: impl FnOnce(&World) -> String, body: impl FnOnce(&World) -> T) -> T {
    let _guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
    let root = tempfile::tempdir().expect("a directory for the test");
    let world = World {
        temporary: root.path().join("tmp"),
        home: root.path().join("home"),
        kept: root.path().join("kept"),
    };
    for directory in [&world.temporary, &world.home, &world.kept] {
        std::fs::create_dir_all(directory).expect("a directory");
    }
    let editor = world.kept.join("editor");
    std::fs::write(&editor, script(&world)).expect("the editor script");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).expect("mode");
    }

    let names = [
        "TMPDIR",
        "HOME",
        "BRAVEBOT_HOME",
        "XDG_CACHE_HOME",
        "VISUAL",
        "EDITOR",
    ];
    let _restore = Restore(
        names
            .iter()
            .map(|name| (*name, std::env::var_os(name)))
            .collect(),
    );
    // SAFETY (each block below): the lock is held, nothing else in this binary reads the
    // environment outside it, and every variable is put back, by `Restore`, before it is let go.
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "six writes to the environment under one lock, which is one decision"
    )]
    // SAFETY: as stated above.
    unsafe {
        std::env::set_var("TMPDIR", &world.temporary);
        std::env::set_var("HOME", &world.home);
        std::env::remove_var("BRAVEBOT_HOME");
        std::env::remove_var("XDG_CACHE_HOME");
        std::env::remove_var("VISUAL");
        std::env::set_var("EDITOR", &editor);
    }
    body(&world)
}

/// The variables a test changed, put back when it ends, including by a panic: a test that fails
/// must not leave the next one a `TMPDIR` that is gone.
struct Restore(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl Drop for Restore {
    fn drop(&mut self) {
        for (name, value) in self.0.drain(..) {
            match value {
                // SAFETY: the lock of `in_a_world` is still held, since this is dropped before it.
                Some(value) => unsafe { std::env::set_var(name, value) },
                // SAFETY: as above.
                None => unsafe { std::env::remove_var(name) },
            }
        }
    }
}

/// An editor that acts as a stage first: it replaces every file under the temporary directory, which
/// is everything a stage may write, and counts them. Then it records what it was handed and shown,
/// saves `saved` over the file, and acts as the stage again. `cp` is what an editor's own read is,
/// and the records are outside the directory the stage writes.
fn the_editor_and_a_stage(world: &World, saved: &str) -> String {
    let temporary = world.temporary.display();
    let kept = world.kept.display();
    format!(
        "#!/bin/sh\n\
         stage() {{\n\
           find '{temporary}' -type f | while read -r found; do\n\
             rm -f \"$found\" && printf 'STAGE' > \"$found\" && echo x >> '{kept}/replaced'\n\
           done\n\
         }}\n\
         stage\n\
         printf '%s' \"$1\" > '{kept}/handed'\n\
         cp \"$1\" '{kept}/seen'\n\
         printf '%s' '{saved}' > \"$1\"\n\
         stage\n"
    )
}

/// A file a stage lists and replaces, which is what shows the stage in the script works: it is
/// replaced in every run, so a run in which nothing else is replaced is not one where the stage did
/// nothing.
fn plant_a_file_in_the_stages_reach(world: &World) -> PathBuf {
    let control = world.temporary.join("bravebot-control");
    std::fs::write(&control, "kept").expect("a control file");
    control
}

fn text_of(path: &Path) -> String {
    std::fs::read_to_string(path).expect("a record the editor wrote")
}

/// Every file under `directory`, so a scratch file left anywhere in a world is found.
fn files_under(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("a directory").flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// R1, R2, R3, R4 (#4609): the prompt is shown as the person typed it and comes back as the editor
/// saved it, with a program replacing every file it can write before the editor reads and again
/// after it saves.
///
/// The regression it rejects is a scratch file in a directory the stage writes, however its name is
/// chosen, and a directory under the temporary directory with mode 0700, which the same account
/// enters. The stage replacing the file before the editor opens it shows as the editor being shown
/// `STAGE`, and after the editor saves as `STAGE` coming back.
#[test]
fn a_stage_replacing_what_it_can_write_changes_neither_what_the_editor_shows_of_the_prompt_nor_what_comes_back()
 {
    in_a_world(
        |world| the_editor_and_a_stage(world, "saved by the person\n"),
        |world| {
            let control = plant_a_file_in_the_stages_reach(world);

            let edited = bravebot_tui::editor::edit("the person's own words")
                .expect("the editor runs and its answer is read");

            assert_eq!(
                text_of(&world.kept.join("seen")),
                "the person's own words",
                "the editor was shown text the stage chose"
            );
            assert_eq!(
                edited, "saved by the person",
                "what came back is text the stage chose"
            );
            assert_eq!(
                text_of(&control),
                "STAGE",
                "the stage did not replace the control, so it shows nothing"
            );
        },
    );
}

/// R4: the body of a proposed write, which comes back exactly as saved and is what the write prompt
/// puts to the person again.
#[test]
fn a_stage_replacing_what_it_can_write_changes_neither_what_the_editor_shows_of_a_write_body_nor_what_comes_back()
 {
    in_a_world(
        |world| the_editor_and_a_stage(world, "fn saved() {}\n"),
        |world| {
            let control = plant_a_file_in_the_stages_reach(world);

            let edited = bravebot_tui::editor::edit_body("fn proposed() {}\n", "rs")
                .expect("the editor runs and its answer is read");

            assert_eq!(
                text_of(&world.kept.join("seen")),
                "fn proposed() {}\n",
                "the editor was shown a body the stage chose"
            );
            assert_eq!(
                edited, "fn saved() {}\n",
                "the body that came back is one the stage chose"
            );
            assert_eq!(text_of(&control), "STAGE", "the stage shows nothing");
        },
    );
}

/// R3: the transcript is shown as it was and nothing is read back.
#[test]
fn a_stage_replacing_what_it_can_write_changes_nothing_the_editor_shows_of_the_transcript() {
    in_a_world(
        |world| the_editor_and_a_stage(world, "what the editor saved"),
        |world| {
            let control = plant_a_file_in_the_stages_reach(world);

            bravebot_tui::editor::show("what happened").expect("the editor runs");

            assert_eq!(
                text_of(&world.kept.join("seen")),
                "what happened",
                "the editor was shown a transcript the stage chose"
            );
            assert_eq!(text_of(&control), "STAGE", "the stage shows nothing");
        },
    );
}

/// R8 (#4609): the file is gone from wherever it was made once the edit is over, down each way out:
/// saved, not saved, an editor that refuses, and a transcript. The listing is of the account's whole
/// home and of the temporary directory, which is where the file may have been put, so a file left in
/// either is found without the test naming the directory.
///
/// The regression it rejects is a removal that still names the old directory: a listing of the
/// temporary directory alone passes once the file has moved.
#[test]
fn the_file_does_not_outlive_the_edit_wherever_it_was_made() {
    for (editor, what) in [
        ("true", "an editor that saved nothing"),
        ("false", "an editor that refused"),
    ] {
        in_a_world(
            |_| format!("#!/bin/sh\nexec {editor}\n"),
            |world| {
                // What each call answers is not what is checked: the listing below is.
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the editor may save or refuse, and what is checked is what is left"
                )]
                let _ = bravebot_tui::editor::edit("something private");
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the editor may save or refuse, and what is checked is what is left"
                )]
                let _ = bravebot_tui::editor::edit_body("something private", "rs");
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the editor may save or refuse, and what is checked is what is left"
                )]
                let _ = bravebot_tui::editor::show("something private");
                for directory in [&world.temporary, &world.home] {
                    assert_eq!(
                        files_under(directory),
                        Vec::<PathBuf>::new(),
                        "{what}: a file was left under {}",
                        directory.display()
                    );
                }
            },
        );
    }
}
