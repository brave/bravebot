//! The comparisons a vouched entry and a remembered line are matched with, each with a contract.
//!
//! Verus has no model of a path, so [`same_path_buf`] is assumed to answer the uninterpreted
//! `spec::same_path_buf`, and [`is_number`] is assumed to answer `spec::is_number`, since
//! `remembered::is_number` parses. Verus proves the rest.

use crate::command::Route;
use std::path::PathBuf;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Whether two held paths are the same bytes.
#[cfg_attr(verus_keep_ghost, verus_verify(external_body))]
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == crate::spec::same_path_buf(*held, *seen),
))]
pub(crate) fn same_path_buf(held: &PathBuf, seen: &PathBuf) -> bool {
    held == seen
}

/// Whether two texts are the same.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == (a@ == b@),
))]
pub(crate) fn same_text(a: &str, b: &str) -> bool {
    a == b
}

/// Whether two argument lists are the same length and equal one for one.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == crate::spec::same_texts(a@, b@),
))]
pub(crate) fn same_texts(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut at = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            a.len() == b.len(),
            at <= a.len(),
            forall|i: int| 0 <= i < at ==> (#[trigger] a@[i])@ == b@[i]@,
        decreases a.len() - at,
    ))]
    while at < a.len() {
        let same = same_text(&a[at], &b[at]);
        if !same {
            return false;
        }
        at += 1;
    }
    true
}

/// Whether a text is a number a slot admits, as `remembered::is_number` says.
#[cfg_attr(verus_keep_ghost, verus_verify(external_body))]
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == crate::spec::is_number(text@),
))]
pub(crate) fn is_number(text: &str) -> bool {
    crate::remembered::is_number(text)
}

/// Whether two lists of assignments are the same length and equal one for one, name and value.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == crate::spec::same_environment(a@, b@),
))]
pub(crate) fn same_environment(a: &[(String, String)], b: &[(String, String)]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut at = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            a.len() == b.len(),
            at <= a.len(),
            forall|i: int|
                0 <= i < at ==> (#[trigger] a@[i]).0@ == b@[i].0@ && a@[i].1@ == b@[i].1@,
        decreases a.len() - at,
    ))]
    while at < a.len() {
        let (name, value) = &a[at];
        let (other_name, other_value) = &b[at];
        if !same_text(name, other_name) || !same_text(value, other_value) {
            return false;
        }
        at += 1;
    }
    true
}

/// Whether two routes are the same stream to the same path.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == crate::spec::same_route(*a, *b),
))]
pub(crate) fn same_route(a: &Route, b: &Route) -> bool {
    match (a, b) {
        (
            Route::Stdout { path, append },
            Route::Stdout {
                path: other,
                append: other_append,
            },
        )
        | (
            Route::Stderr { path, append },
            Route::Stderr {
                path: other,
                append: other_append,
            },
        ) => {
            let same_way = *append == *other_append;
            same_way && same_path_buf(path, other)
        }
        (Route::Stdin { path }, Route::Stdin { path: other })
        | (Route::Both { path }, Route::Both { path: other }) => same_path_buf(path, other),
        (Route::StderrToStdout, Route::StderrToStdout) => true,
        // Two different streams, or one stream sent to a file and one merged.
        _ => false,
    }
}

/// Whether two lists of routes are the same length and equal one for one.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == crate::spec::same_routes(a@, b@),
))]
pub(crate) fn same_routes(a: &[Route], b: &[Route]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut at = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            a.len() == b.len(),
            at <= a.len(),
            forall|i: int| 0 <= i < at ==> crate::spec::same_route(#[trigger] a@[i], b@[i]),
        decreases a.len() - at,
    ))]
    while at < a.len() {
        if !same_route(&a[at], &b[at]) {
            return false;
        }
        at += 1;
    }
    true
}
