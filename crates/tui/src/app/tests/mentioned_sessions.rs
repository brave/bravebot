// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! NAME-10: the rows the `@` list offers for earlier sessions of the directory.

use super::*;

/// The row for a session says how long ago it was last written, and the rows come newest first.
/// Two sessions with different ages are offered, the older one created second so that neither the
/// order they were made in nor the order of the files can stand for the age.
///
/// Rejects a row that leaves the age out, an age taken from another session, and a list reversed
/// from `sessions::list`'s newest-first order.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn the_at_list_says_how_old_each_session_is_and_offers_the_newest_first() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-offer-sessions-by-age");
    std::fs::create_dir_all(&root).expect("create");
    let (_, _, newer, _, _) = a_session_to_branch(&root);
    let (_, _, older, _, _) = a_session_to_branch(&root);
    let now = bravebot_session::sessions::now();
    change_stored_session(&root, newer.id(), |record| {
        record.updated = now - 3 * 60 * 60;
    });
    change_stored_session(&root, older.id(), |record| {
        record.updated = now - 2 * 24 * 60 * 60;
    });

    let rows = past_sessions_offered(&root, "another-session");

    let offered: Vec<&str> = rows.iter().map(|row| row.path.as_str()).collect();
    assert_eq!(
        offered,
        [
            format!("session:{}", newer.id()),
            format!("session:{}", older.id())
        ],
        "the rows are not newest first"
    );
    let title = "write a line saying hello into notes.txt";
    for (row, when) in rows.iter().zip(["3 hours ago", "2 days ago"]) {
        assert_eq!(
            row.note.as_deref(),
            Some(
                t!(
                    session_mention_row,
                    title = title,
                    when = when,
                    chars = bravebot_session::excerpt::BOUND
                )
                .as_str()
            ),
            "the row for {} does not say {when}",
            row.path
        );
    }
}
