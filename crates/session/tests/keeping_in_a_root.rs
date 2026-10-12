// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Choosing the sessions root changes it for the whole process, so these run in a process of their
//! own and not beside the unit tests that read the default.

use bravebot_session::sessions;
use std::path::PathBuf;

/// A second choice is refused and says which root is in force, so whoever reports it names the root
/// that is being used and not the one that was turned down.
#[test]
fn a_second_choice_is_refused_and_names_the_root_in_force() {
    let first = PathBuf::from("/srv/the-first-root");
    let second = PathBuf::from("/srv/the-second-root");

    assert_eq!(sessions::keep_in(first.clone()), Ok(()));
    assert_eq!(sessions::keep_in(second), Err(first.clone()));
    assert_eq!(sessions::sessions_root(), Some(first));
}
