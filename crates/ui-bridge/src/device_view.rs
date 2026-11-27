// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What a paired device is shown of a session (decision 19 in
//! `docs/design/mobile/decisions-and-tradeoffs.md`).
//!
//! A device is sent only the events named in [`EVENTS`], and its copy of the view has every row the
//! bridge marked as coming from somewhere nobody vouched for replaced by a placeholder that keeps
//! the row's place. A question marked so is not offered to it, and is answered on the computer.
//!
//! What decides is the bridge's own: an event's name, a row's kind, and the `untrusted` flag the
//! agent sets from a value's label. No text of a row is read.

use crate::turn::Kind;
use crate::view::{Row, RowKind, Update};
use serde_json::{Value, json};
use std::sync::Arc;

/// Whether a connection that is not a device can answer a question of the kind given, in the
/// session named. A question a device is not offered is put only where one can, and is otherwise
/// refused when it is asked, as one nobody can answer is.
pub type Locally = Arc<dyn Fn(&str, Kind) -> bool + Send + Sync>;

/// The events a device is sent. Any other is not: it either repeats what the view holds, or is a
/// window's alone. A name added to the bridge reaches no device until it is named here.
pub const EVENTS: &[&str] = &[
    "agent.ready",
    "session.view.initial",
    "session.view.update",
    "session.view.snapshot",
    "session.suspended",
    "session.closed",
    "trust.request",
];

/// The events that carry a view, which a device is sent its own copy of.
pub const VIEWS: &[&str] = &[
    "session.view.initial",
    "session.view.update",
    "session.view.snapshot",
];

/// Whether the bridge marked `data` as holding what came from somewhere nobody vouched for. Only
/// the flag is read: it is the agent's word, set from the value's label.
fn untrusted(data: &Value) -> bool {
    data.get("untrusted").and_then(Value::as_bool) == Some(true)
}

/// Whether a device is shown `row` only as a placeholder: a quarantined preview, an activity whose
/// target came from content nobody vouched for, and a question whose body did.
fn withheld(row: &Row) -> bool {
    match row.kind {
        RowKind::Quarantined => true,
        RowKind::Activity | RowKind::Approval => untrusted(&row.data),
        RowKind::Prompt | RowKind::Narration | RowKind::Reply | RowKind::Error => false,
    }
}

/// The placeholder a withheld row is shown as: its place and kind, and none of its content. A
/// question's says it is answered on the computer.
fn placeholder(row: &Row) -> Value {
    match row.kind {
        RowKind::Approval => json!({"withheld": true, "answer": "on the computer"}),
        _ => json!({"withheld": true}),
    }
}

/// The view as a device is shown it: each withheld row's content replaced by a placeholder with
/// the same id, turn, kind, event and resolution, and a question whose body nobody vouched for left
/// out of what waits on it. Returns the number of that question, which the device may not answer.
pub fn for_a_device(update: &mut Update) -> Option<u64> {
    for row in update.rows.iter_mut().filter(|row| withheld(row)) {
        row.data = placeholder(row);
    }
    let question = update
        .pending
        .as_ref()
        .filter(|pending| untrusted(&pending.data))
        .map(|pending| pending.request);
    if question.is_some() {
        update.pending = None;
    }
    question
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::{Pending, Status};

    const SENTINEL: &str = "SENTINEL-the-device-must-not-see-this";

    fn row(id: u64, kind: RowKind, data: Value) -> Row {
        Row {
            id,
            turn: 1,
            kind,
            event: Some("an.event".into()),
            data,
            resolved: false,
        }
    }

    fn an_update(rows: Vec<Row>, pending: Option<Pending>) -> Update {
        Update {
            sequence: 7,
            turn: 1,
            target: 1,
            status: Status::Running,
            pending,
            rows,
            omitted: 0,
            actor: None,
        }
    }

    /// A quarantined preview, an untrusted activity and an untrusted question reach a device as
    /// placeholders that keep their place, and the text marked with a sentinel is nowhere in what
    /// it is sent. Rows nothing marked are sent whole.
    #[test]
    fn what_nobody_vouched_for_reaches_a_device_as_a_placeholder_in_its_place() {
        let labelled = json!({"label": "(U,pub)", "preview": [SENTINEL], "lines": 1});
        let mut update = an_update(
            vec![
                row(1, RowKind::Prompt, json!({"prompt": "look"})),
                row(2, RowKind::Quarantined, labelled),
                row(
                    3,
                    RowKind::Activity,
                    json!({"target": SENTINEL, "untrusted": true}),
                ),
                row(
                    4,
                    RowKind::Activity,
                    json!({"target": "src/lib.rs", "untrusted": false}),
                ),
                row(
                    5,
                    RowKind::Approval,
                    json!({"request": 9, "path": SENTINEL, "untrusted": true}),
                ),
                row(6, RowKind::Narration, json!({"text": "reading"})),
            ],
            Some(Pending {
                row: 5,
                request: 9,
                kind: "confirm".into(),
                supported: true,
                data: json!({"request": 9, "path": SENTINEL, "untrusted": true}),
            }),
        );
        let kept: Vec<(u64, u64, RowKind, Option<String>, bool)> = update
            .rows
            .iter()
            .map(|row| (row.id, row.turn, row.kind, row.event.clone(), row.resolved))
            .collect();

        assert_eq!(for_a_device(&mut update), Some(9));

        let sent = serde_json::to_string(&update).unwrap();
        assert!(!sent.contains(SENTINEL), "{sent}");
        assert_eq!(
            update
                .rows
                .iter()
                .map(|row| (row.id, row.turn, row.kind, row.event.clone(), row.resolved))
                .collect::<Vec<_>>(),
            kept
        );
        assert_eq!(update.rows[1].data, json!({"withheld": true}));
        assert_eq!(update.rows[2].data, json!({"withheld": true}));
        assert_eq!(
            update.rows[4].data,
            json!({"withheld": true, "answer": "on the computer"})
        );
        assert_eq!(update.rows[0].data, json!({"prompt": "look"}));
        assert_eq!(update.rows[3].data["target"], "src/lib.rs");
        assert_eq!(update.rows[5].data["text"], "reading");
        assert!(update.pending.is_none());
        assert_eq!(update.sequence, 7);
    }

    /// A question nothing marked stays waiting for the device, and no number is withheld.
    #[test]
    fn a_question_nothing_marked_is_offered_to_a_device() {
        let pending = Pending {
            row: 1,
            request: 4,
            kind: "run".into(),
            supported: true,
            data: json!({"request": 4, "line": "git status"}),
        };
        let mut update = an_update(
            vec![row(1, RowKind::Approval, pending.data.clone())],
            Some(pending),
        );
        assert_eq!(for_a_device(&mut update), None);
        assert_eq!(
            update.pending.as_ref().map(|pending| pending.request),
            Some(4)
        );
        assert_eq!(update.rows[0].data["line"], "git status");
    }

    /// Only a flag set to true withholds: a missing flag, a false one, and one that is not a
    /// boolean, such as the string "true", leave the row whole, since none is the agent's mark.
    #[test]
    fn only_the_flag_the_agent_sets_withholds_a_row() {
        for (data, withholds) in [
            (json!({"untrusted": true}), true),
            (json!({"untrusted": false}), false),
            (json!({}), false),
            (json!({"untrusted": "true"}), false),
        ] {
            assert_eq!(
                withheld(&row(1, RowKind::Activity, data.clone())),
                withholds,
                "{data}"
            );
        }
    }

    /// The events a device is sent are these and no others, and each that carries a view is among
    /// them. A name added here sends a device something new, so it is a change a reviewer sees.
    #[test]
    fn a_device_is_sent_only_these_events() {
        assert_eq!(
            EVENTS,
            [
                "agent.ready",
                "session.view.initial",
                "session.view.update",
                "session.view.snapshot",
                "session.suspended",
                "session.closed",
                "trust.request",
            ]
        );
        for view in VIEWS {
            assert!(EVENTS.contains(view), "{view}");
        }
    }
}
