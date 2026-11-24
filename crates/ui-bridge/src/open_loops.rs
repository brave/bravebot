// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A desktop session's open loops, worked out from the bridge's own records (OPEN-1).

use crate::running::State;
use bravebot_agent::Workspace;
use bravebot_agent::open_loops::Loops;

/// The loops `state` leaves open in `workspace`: the session's work record, the unstruck rows of
/// its newest task list, the checkouts it keeps and has not applied, and the files it wrote that
/// differ from `HEAD`.
///
/// Running jobs and the stopped goal are always empty. A desktop chat has no background job and
/// no goal (`background.rs`), so there is nothing for either to count.
pub(crate) fn of(state: &State, workspace: &Workspace) -> Loops {
    Loops {
        open_todos: state
            .todos
            .values()
            .next_back()
            .map_or(0, |rows| bravebot_agent::open_loops::open_todos(rows).len()),
        kept_checkouts: bravebot_agent::open_loops::kept_checkouts(workspace).len(),
        uncommitted_files: bravebot_agent::open_loops::uncommitted(
            workspace.root(),
            &state.written,
        )
        .len(),
        running_jobs: Vec::new(),
        stopped_goal: false,
        ..Loops::from_work(state.work)
    }
}
