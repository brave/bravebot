// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! `window.fault`: the desktop window's page or a process behind it failed (DIAG-10).
//!
//! The window names what failed in strings it chose, such as the name of the error it caught. None
//! of them is written. Each is matched against the words fixed in this file and the word from the
//! file is what the log gets, so a string the window made up, however it is spelled, becomes
//! `other`.

use bravebot_diag::Field;
use serde_json::{Value, json};

use crate::protocol::{Failure, Request};

/// What failed: the page while it drew, an error nothing caught, a promise nothing handled, the
/// window's own process, or another process of the app.
const KINDS: &[(&str, &str)] = &[
    ("render", "render"),
    ("uncaught", "uncaught"),
    ("rejection", "rejection"),
    ("renderer_gone", "renderer_gone"),
    ("child_gone", "child_gone"),
];

/// The name of the error that was caught: the language's own and the browser's, which is the
/// difference between a bad call and a page whose DOM was changed under it.
const NAMES: &[(&str, &str)] = &[
    ("Error", "Error"),
    ("EvalError", "EvalError"),
    ("RangeError", "RangeError"),
    ("ReferenceError", "ReferenceError"),
    ("SyntaxError", "SyntaxError"),
    ("TypeError", "TypeError"),
    ("URIError", "URIError"),
    ("AggregateError", "AggregateError"),
    ("NotFoundError", "NotFoundError"),
    ("HierarchyRequestError", "HierarchyRequestError"),
    ("InvalidStateError", "InvalidStateError"),
    ("InvalidCharacterError", "InvalidCharacterError"),
    ("NotSupportedError", "NotSupportedError"),
    ("SecurityError", "SecurityError"),
    ("QuotaExceededError", "QuotaExceededError"),
    ("DataCloneError", "DataCloneError"),
    ("AbortError", "AbortError"),
    ("TimeoutError", "TimeoutError"),
    ("NetworkError", "NetworkError"),
    ("IndexSizeError", "IndexSizeError"),
];

/// Why Electron says a process is gone.
const REASONS: &[(&str, &str)] = &[
    ("clean-exit", "clean-exit"),
    ("abnormal-exit", "abnormal-exit"),
    ("killed", "killed"),
    ("crashed", "crashed"),
    ("oom", "oom"),
    ("launch-failed", "launch-failed"),
    ("integrity-failure", "integrity-failure"),
    ("memory-eviction", "memory-eviction"),
];

/// Which process of the app is gone, when it is not the window's own. Electron spells some of
/// these with a space, which would end a field early.
const PROCESSES: &[(&str, &str)] = &[
    ("GPU", "gpu"),
    ("Utility", "utility"),
    ("Zygote", "zygote"),
    ("Sandbox helper", "sandbox_helper"),
    ("Pepper Plugin", "plugin"),
    ("Pepper Plugin Broker", "plugin"),
];

/// The word the file fixes for `given`, or `other` where `given` is not on the list.
fn pick(table: &[(&str, &'static str)], given: &str) -> &'static str {
    table
        .iter()
        .find(|(known, _)| *known == given)
        .map_or("other", |(_, written)| written)
}

/// The fields a request becomes. A field the window did not send is not written, so each line
/// holds what the window said and nothing it did not.
fn fields(request: &Request) -> Result<Vec<(&'static str, Field)>, Failure> {
    let mut out = vec![("kind", Field::word(pick(KINDS, &request.string("kind")?)))];
    for (key, table) in [("name", NAMES), ("reason", REASONS), ("process", PROCESSES)] {
        if let Some(given) = request.optional_string(key) {
            out.push((key, Field::word(pick(table, &given))));
        }
    }
    // A code is a count. Electron reports a negative one on some platforms, which is not one.
    if let Some(code) = request.param("exit").as_u64() {
        out.push(("exit", Field::num(code)));
    }
    Ok(out)
}

/// Writes the line. Only an error line is written, at every level, since nothing here is a
/// step that worked.
pub fn record(request: &Request) -> Result<Value, Failure> {
    bravebot_diag::error("window.fault", &fields(request)?);
    Ok(json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(params: Value) -> Request {
        Request {
            id: 1,
            method: "window.fault".to_string(),
            params,
        }
    }

    fn word(w: &'static str) -> Field {
        Field::word(w)
    }

    #[test]
    fn a_page_error_is_written_as_its_kind_and_the_name_of_the_error() {
        let got = fields(&asked(json!({"kind": "render", "name": "NotFoundError"}))).unwrap();
        assert_eq!(
            got,
            vec![("kind", word("render")), ("name", word("NotFoundError"))]
        );
    }

    #[test]
    fn a_process_that_is_gone_is_written_with_why_and_its_exit_code() {
        let got = fields(&asked(json!({
            "kind": "renderer_gone", "reason": "crashed", "exit": 11,
        })))
        .unwrap();
        assert_eq!(
            got,
            vec![
                ("kind", word("renderer_gone")),
                ("reason", word("crashed")),
                ("exit", Field::num(11u64)),
            ]
        );
    }

    #[test]
    fn a_process_name_with_a_space_is_written_without_one() {
        let got = fields(&asked(json!({
            "kind": "child_gone", "process": "Sandbox helper", "reason": "killed",
        })))
        .unwrap();
        assert!(
            got.contains(&("process", word("sandbox_helper"))),
            "{got:?}"
        );
    }

    #[test]
    fn a_string_that_is_on_no_list_is_written_as_other_whatever_it_says() {
        for hostile in [
            "NotFoundError\nERROR forged line",
            "notfounderror",
            " NotFoundError",
            "Error: the file /Users/a/secret.txt",
            "",
        ] {
            let got = fields(&asked(json!({
                "kind": hostile, "name": hostile, "reason": hostile, "process": hostile,
            })))
            .unwrap();
            assert_eq!(
                got,
                vec![
                    ("kind", word("other")),
                    ("name", word("other")),
                    ("reason", word("other")),
                    ("process", word("other")),
                ],
                "{hostile:?}"
            );
        }
    }

    #[test]
    fn a_field_that_is_not_text_or_a_count_is_left_out() {
        let got = fields(&asked(json!({
            "kind": "render", "name": 5, "reason": ["crashed"], "exit": -1,
        })))
        .unwrap();
        assert_eq!(got, vec![("kind", word("render"))]);
        let got = fields(&asked(json!({"kind": "render", "exit": "11"}))).unwrap();
        assert_eq!(got, vec![("kind", word("render"))]);
    }

    #[test]
    fn a_request_with_no_kind_is_refused() {
        assert!(fields(&asked(json!({"name": "NotFoundError"}))).is_err());
        assert!(fields(&asked(json!({"kind": 7}))).is_err());
    }
}
