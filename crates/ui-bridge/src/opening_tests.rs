// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What opens a window's session: the map its answer or a kept answer starts it with (RUN-5).

use super::*;

/// RUN-5: a kept answer settles a window as it settles a terminal, the answer about confined
/// lines included where its entry says the question said so. Without it, the desktop would
/// keep asking in a directory the person answered about in the terminal.
#[test]
fn an_answer_allows_confined_lines_only_for_a_yes_to_a_question_that_said_so() {
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let project = std::env::temp_dir();
    for (trusted, said, allowed) in [
        (true, true, true),
        (true, false, false),
        (false, true, false),
        (false, false, false),
    ] {
        let map = map_for_an_answer(&project, trusted, said);
        assert_eq!(map.allows_confined_lines(), allowed, "{trusted} {said}");
        assert_eq!(map.is_trusted("src/main.rs"), trusted, "{trusted} {said}");
    }
}

/// RUN-5: a window settles from the entry's own answer about confined lines. An entry written before
/// the question said so must not settle the window as if it had.
#[test]
fn a_kept_answer_settles_a_window_with_what_its_entry_says() {
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let project = std::env::temp_dir();
    let kept = |confined_lines| trusted::Kept {
        root: project.clone(),
        session: "1".into(),
        at: 1,
        confined_lines,
    };
    let said = Opening::settled_by(&project, Some((kept(true), PathBuf::from("/kept"))));
    assert!(said.answered && said.remembered.is_some());
    assert!(said.trust.is_trusted("src/main.rs"));
    assert!(
        said.trust.allows_confined_lines(),
        "an entry that said so did not settle the window as saying so"
    );
    let unsaid = Opening::settled_by(&project, Some((kept(false), PathBuf::from("/kept"))));
    assert!(unsaid.trust.is_trusted("src/main.rs"));
    assert!(
        !unsaid.trust.allows_confined_lines(),
        "an entry written before the question said so settled the window as saying so"
    );
}

/// PROMPT-1: the response opening a session carries the directory and the record a yes is kept in
/// spelled for a person beside the ones held, so the window's startup question does not draw one
/// that ends in a space, or holds a byte that is not text, as another. A path that needs no
/// spelling is sent as it is, and the held values keep the bytes they had.
#[test]
fn the_response_opening_a_session_carries_the_directory_and_the_record_spelled_for_a_person() {
    let made = tempfile::tempdir().unwrap();
    let identity = trusted::Identity::of(made.path()).expect("a directory");
    let asked = |project: &Path, home: &Path| {
        Opening {
            trust: TrustStore::new(project),
            answered: false,
            remembered: None,
            keeping: Some((trusted::Store::new(home, project), identity)),
        }
        .json(project)
    };
    let plain = asked(Path::new("/work/proj"), Path::new("/home/me/state"));
    assert_eq!(plain["shownDirectory"], json!("/work/proj"));
    assert_eq!(plain["shownKeeping"], plain["keeping"]);

    let odd = asked(
        Path::new("/work/proj "),
        Path::new("/home/me/state\u{202e}"),
    );
    assert_eq!(odd["shownDirectory"], json!("\"/work/proj\u{2423}\""));
    let held = odd["keeping"].as_str().expect("a record");
    let shown = odd["shownKeeping"].as_str().expect("a spelled record");
    assert!(
        held.starts_with("/home/me/state\u{202e}/trusted/"),
        "{held}"
    );
    assert!(
        shown.starts_with("\"/home/me/state\\u{202e}/trusted/") && shown.ends_with(".jsonl\""),
        "{shown}"
    );

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let bytes = asked(
            Path::new(std::ffi::OsStr::from_bytes(b"/work/pr\xffoj")),
            Path::new("/home/me/state"),
        );
        // The name its replacement-character rendering spells has the same held `directory`, so
        // the spelled field is what tells the two apart (PATH-003).
        let lookalike = asked(Path::new("/work/pr\u{fffd}oj"), Path::new("/home/me/state"));
        assert_eq!(bytes["shownDirectory"], json!("\"/work/pr\\xffoj\""));
        assert_ne!(bytes["shownDirectory"], lookalike["shownDirectory"]);
    }

    let unasked = Opening {
        trust: TrustStore::new(Path::new("/work/proj")),
        answered: true,
        remembered: None,
        keeping: None,
    }
    .json(Path::new("/work/proj"));
    assert_eq!(unasked["keeping"], Value::Null);
    assert_eq!(unasked["shownKeeping"], Value::Null);
}

/// PROMPT-1: the answer kept about a directory reaches the window with the file it is kept in and
/// the directory it is about spelled for a person, so a record or a root whose name ends in a space
/// does not read as another in the notice above the transcript and in the permissions list.
#[test]
fn a_kept_answer_carries_its_file_and_its_root_spelled_for_a_person() {
    let kept = |root: &str| trusted::Kept {
        root: PathBuf::from(root),
        session: "1".into(),
        at: 7,
        confined_lines: false,
    };
    let plain = kept_json(&kept("/work/repo"), Path::new("/home/me/state/trusted"));
    assert_eq!(plain["shownPath"], plain["path"]);
    assert_eq!(plain["shownRoot"], plain["root"]);

    let odd = kept_json(&kept("/work/repo "), Path::new("/home/me/state /trusted"));
    assert_eq!(odd["path"], json!("/home/me/state /trusted"));
    assert_eq!(
        odd["shownPath"],
        json!("\"/home/me/state\u{2423}/trusted\"")
    );
    assert_eq!(odd["root"], json!("/work/repo "));
    assert_eq!(odd["shownRoot"], json!("\"/work/repo\u{2423}\""));
}
