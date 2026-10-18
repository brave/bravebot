// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Every method the bridge answers is either offered to a client of the host or denied by name.
//!
//! The host refuses what it does not name, so a method added to the bridge is unreachable until it
//! is classified. This test is what makes classifying it a decision somebody takes rather than a
//! default: it fails while the bridge has a method the allowlist does not mention, and while the
//! allowlist mentions one the bridge no longer has.

use bravebot_ui_bridge::allowlist::{BY_THE_HOST, DENIED, OFFERED};
use std::collections::BTreeSet;
use std::path::Path;

/// The methods named by the arms of `Bridge::dispatch`.
fn dispatched(source: &str) -> BTreeSet<String> {
    let start = source
        .find("pub fn dispatch(")
        .expect("the bridge has a dispatch");
    let rest = &source[start..];
    let end = rest
        .find("unknown method")
        .expect("the dispatch refuses an unknown method");
    let mut methods = BTreeSet::new();
    for line in rest[..end].lines() {
        let line = line.trim();
        let Some((arm, _)) = line.split_once("=>") else {
            continue;
        };
        if !arm.starts_with('"') {
            continue;
        }
        methods.extend(
            arm.split('"')
                .enumerate()
                .filter(|(at, _)| at % 2 == 1)
                .map(|(_, name)| name.to_string()),
        );
    }
    methods
}

#[test]
fn the_allowlist_classifies_every_method_the_bridge_answers() {
    let source =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bridge.rs"))
            .expect("the bridge source");
    let answered = dispatched(&source);
    assert!(
        answered.len() > 40,
        "the scan found only {} methods, so it is reading the wrong thing: {answered:?}",
        answered.len()
    );

    let classified: BTreeSet<String> = OFFERED
        .iter()
        .map(|operation| operation.method)
        .filter(|method| !BY_THE_HOST.contains(method))
        .chain(DENIED.iter().copied())
        .map(str::to_string)
        .collect();

    let unclassified: Vec<_> = answered.difference(&classified).collect();
    let stale: Vec<_> = classified.difference(&answered).collect();
    assert!(
        unclassified.is_empty(),
        "the bridge answers methods the host allowlist neither offers nor denies: {unclassified:?}"
    );
    assert!(
        stale.is_empty(),
        "the host allowlist names methods the bridge does not answer: {stale:?}"
    );
}

#[test]
fn the_scan_reads_arms_with_several_names() {
    let source = "pub fn dispatch(&mut self) {\n    match m {\n        \"a.one\" | \"a.two\" => x,\n        \"b\" => self.go(\n            \"not.an.arm\",\n        ),\n        other => Err(Failure::bad_request(format!(\"unknown method `{other}`\"))),\n    }\n";
    let found = dispatched(source);
    assert_eq!(
        found.into_iter().collect::<Vec<_>>(),
        ["a.one", "a.two", "b"]
    );
}
