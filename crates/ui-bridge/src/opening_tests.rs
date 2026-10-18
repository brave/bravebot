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
