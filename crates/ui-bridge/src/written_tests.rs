// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The files a turn's writes landed on, which the session asks git about for its open loops
//! (OPEN-1).

use super::*;
use crate::emit::Emitter;
use bravebot_agent::report::Written;

fn write_to(path: &str) -> Activity {
    Activity::running("Update", path).with_written(Some(Written {
        landed: PathBuf::from(path),
        ..Written::default()
    }))
}

/// A finished write's landed file is listed once, in the order first written, and a failed write's
/// file is not listed. The regression it rejects is a reporter that lists nothing, so the desktop
/// session never counts a file it wrote as uncommitted, or one that lists a write that failed.
#[test]
fn a_finished_write_is_listed_once_and_a_failed_one_is_not() {
    let mut reporter = BridgeReporter::new(Emitter::new(Box::new(|_| {})), "s1");
    reporter.tool_finished(write_to("b.rs").done("wrote"));
    reporter.tool_finished(write_to("a.rs").done("wrote"));
    reporter.tool_finished(write_to("b.rs").done("wrote again"));
    reporter.tool_finished(write_to("c.rs").failed("refused"));
    reporter.tool_finished(Activity::running("Run", "make").done("ok"));

    assert_eq!(
        reporter.written(),
        [PathBuf::from("b.rs"), PathBuf::from("a.rs")]
    );
}

/// A name that is not UTF-8 and its lossy lookalike are two files, so both are listed (PATH-003).
/// The regression it rejects is a dedupe that compares names through `to_string_lossy`, which
/// would drop the second and leave one of the session's writes out of its uncommitted files.
#[cfg(unix)]
#[test]
fn a_name_that_is_not_utf8_is_listed_beside_its_lossy_lookalike() {
    use std::os::unix::ffi::OsStrExt;
    let raw = PathBuf::from(std::ffi::OsStr::from_bytes(b"a-\xff"));
    let lookalike = PathBuf::from("a-\u{FFFD}");
    let landed_on = |landed: &PathBuf| {
        Activity::running("Update", "a-").with_written(Some(Written {
            landed: landed.clone(),
            ..Written::default()
        }))
    };
    let mut reporter = BridgeReporter::new(Emitter::new(Box::new(|_| {})), "s1");
    reporter.tool_finished(landed_on(&raw).done("wrote"));
    reporter.tool_finished(landed_on(&lookalike).done("wrote"));

    assert_eq!(reporter.written(), [raw, lookalike]);
}
