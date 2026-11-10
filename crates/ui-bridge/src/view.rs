//! An opt-in display projection. Payloads are copied whole, including their labels.
//!
//! Only bridge-owned event names and identity/status fields select transitions. This module
//! never reads released text, formats it, or returns display data to execution. It takes each
//! row's serialized length to bound the bytes it keeps, and nothing else of it.

use crate::protocol::Event;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

/// How many of the most recent rows a view keeps for a snapshot. Older rows are left out of it,
/// and the snapshot says how many.
pub const RETAINED_ROWS: usize = 256;

/// How many bytes of serialized rows a view keeps for a snapshot, beside [`RETAINED_ROWS`].
pub const RETAINED_BYTES: usize = 4 << 20;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    AwaitingTrust,
    Idle,
    Running,
    Waiting,
    Completed,
    Failed,
    Cancelled,
    Detached,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    Prompt,
    Narration,
    Quarantined,
    Approval,
    Reply,
    Error,
    Activity,
}

/// A replacement at a stable position. `data` is the existing wire payload, without reshaping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub id: u64,
    pub turn: u64,
    pub kind: RowKind,
    pub event: Option<String>,
    pub data: Value,
    pub resolved: bool,
}

/// The exact question displayed. Unsupported kinds require a legacy local client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pending {
    pub row: u64,
    pub request: u64,
    pub kind: String,
    pub supported: bool,
    pub data: Value,
}

/// Initial state or a patch: replace listed rows and every status field, in sequence order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Update {
    pub sequence: u64,
    pub turn: u64,
    /// What a cancel names to stop the turn on screen (RPCVIEW-6); 0 before any turn has started.
    pub target: u64,
    pub status: Status,
    pub pending: Option<Pending>,
    pub rows: Vec<Row>,
    /// How many rows, from the first, the view no longer keeps. A client leaves those out too.
    pub omitted: u64,
}

/// The current control and display state, and the last [`RETAINED_ROWS`] rows in the order each
/// first appeared. The pending question is kept apart from the rows, so it is never left out of a
/// snapshot.
pub(crate) struct View {
    sequence: u64,
    turn: u64,
    target: u64,
    status: Status,
    next_row: u64,
    pending: Option<Pending>,
    /// Calls that have started and not finished, by the call's id, with the row each is drawn on.
    running_calls: HashMap<u64, u64>,
    /// The latest state of each row kept, and its length serialized, by id. Ids are given in the
    /// order rows first appear.
    retained: BTreeMap<u64, (Row, usize)>,
    /// The sum of the serialized lengths in `retained`.
    retained_bytes: usize,
    /// The id of the last row no longer kept. Rows are numbered from 1 and dropped oldest first,
    /// so this is also how many have been dropped.
    dropped_through: u64,
}

impl View {
    pub fn new(trusted: bool) -> Self {
        Self {
            sequence: 0,
            turn: 0,
            target: 0,
            status: if trusted {
                Status::Idle
            } else {
                Status::AwaitingTrust
            },
            next_row: 0,
            pending: None,
            running_calls: HashMap::new(),
            retained: BTreeMap::new(),
            retained_bytes: 0,
            dropped_through: 0,
        }
    }

    /// The current state with every row kept, for a client that was not there for all of it.
    pub fn snapshot(&self) -> Update {
        self.update(self.retained.values().map(|(row, _)| row.clone()).collect())
    }

    pub fn initial(&self) -> Update {
        self.update(Vec::new())
    }

    fn update(&self, rows: Vec<Row>) -> Update {
        Update {
            sequence: self.sequence,
            turn: self.turn,
            target: self.target,
            status: self.status,
            pending: self.pending.clone(),
            rows,
            omitted: self.dropped_through,
        }
    }

    fn changed(&mut self, rows: Vec<Row>) -> Update {
        self.sequence += 1;
        // A later state of a row already dropped is sent and not kept, so the rows kept stay the
        // ones after `dropped_through`.
        for row in rows.iter().filter(|row| row.id > self.dropped_through) {
            let bytes = serialized_len(row);
            if let Some((_, was)) = self.retained.insert(row.id, (row.clone(), bytes)) {
                self.retained_bytes -= was;
            }
            self.retained_bytes += bytes;
        }
        // Ids rise with first appearance, so the first rows are the oldest. A row this update sends
        // is kept whatever its size while the rows it does not send fit, so a client asking straight
        // after a long reply is sent it. A sent row older than more than the budget of other rows is
        // dropped with them. The rows kept are never more than the budget and this update's rows.
        let sent: Vec<u64> = rows.iter().map(|row| row.id).collect();
        let mut sent_bytes: usize = sent
            .iter()
            .filter_map(|id| self.retained.get(id))
            .map(|(_, bytes)| bytes)
            .sum();
        while self.retained.len() > RETAINED_ROWS
            || (self.retained_bytes > RETAINED_BYTES
                && self.retained.first_key_value().is_some_and(|(id, _)| {
                    !sent.contains(id) || self.retained_bytes - sent_bytes > RETAINED_BYTES
                }))
        {
            if let Some((id, (_, bytes))) = self.retained.pop_first() {
                if sent.contains(&id) {
                    sent_bytes -= bytes;
                }
                self.retained_bytes -= bytes;
                self.dropped_through = id;
            }
        }
        self.update(rows)
    }

    fn row(&mut self, kind: RowKind, event: Option<&str>, data: Value) -> Row {
        self.next_row += 1;
        self.row_with(self.next_row, kind, event, data, false)
    }

    /// A row with an id already assigned, for a later state of a row that exists.
    fn row_with(
        &self,
        id: u64,
        kind: RowKind,
        event: Option<&str>,
        data: Value,
        resolved: bool,
    ) -> Row {
        Row {
            id,
            turn: self.turn,
            kind,
            event: event.map(str::to_string),
            data,
            resolved,
        }
    }

    pub fn started(&mut self, turn: u64, target: u64, prompt: Value) -> Update {
        self.turn = turn;
        self.target = target;
        self.status = Status::Running;
        self.running_calls.clear();
        let row = self.row(RowKind::Prompt, None, prompt);
        self.changed(vec![row])
    }

    pub fn trusted(&mut self) -> Option<Update> {
        if self.status != Status::AwaitingTrust {
            return None;
        }
        self.status = Status::Idle;
        Some(self.changed(Vec::new()))
    }

    fn resolve(&mut self) -> Vec<Row> {
        self.pending
            .take()
            .map(|p| {
                let event = format!("{}.request", p.kind);
                self.row_with(p.row, RowKind::Approval, Some(&event), p.data, true)
            })
            .into_iter()
            .collect()
    }

    pub fn answered(&mut self, request: u64) -> Option<Update> {
        if self.pending.as_ref().is_none_or(|p| p.request != request) {
            return None;
        }
        let rows = self.resolve();
        self.status = Status::Running;
        Some(self.changed(rows))
    }

    pub fn detach(&mut self) -> Update {
        let rows = self.resolve();
        self.status = Status::Detached;
        self.changed(rows)
    }

    pub fn event(&mut self, event: &Event) -> Option<Update> {
        let kind = match event.name {
            "narration" => RowKind::Narration,
            "quarantined" => RowKind::Quarantined,
            "tool.started" => {
                let row = self.row(RowKind::Activity, Some(event.name), event.data.clone());
                // The call id is the engine's dispatch counter, written by the bridge, never by
                // content. A start without one is not waited on.
                if let Some(call) = event.data["call"].as_u64() {
                    self.running_calls.insert(call, row.id);
                }
                return Some(self.changed(vec![row]));
            }
            "tool.finished" => {
                // A finish replaces the row of the call with its id. Nothing in a call's content
                // decides the row, because a call's target can be content from outside. A finish
                // for no call still running is a row of its own.
                let running = event.data["call"]
                    .as_u64()
                    .and_then(|call| self.running_calls.remove(&call));
                let row = match running {
                    Some(id) => self.row_with(
                        id,
                        RowKind::Activity,
                        Some(event.name),
                        event.data.clone(),
                        false,
                    ),
                    None => self.row(RowKind::Activity, Some(event.name), event.data.clone()),
                };
                return Some(self.changed(vec![row]));
            }
            "turn.done" | "turn.error" => {
                self.running_calls.clear();
                let mut rows = self.resolve();
                self.status = match event.name {
                    "turn.done" => Status::Completed,
                    _ if event.data["kind"] == "cancelled" => Status::Cancelled,
                    _ => Status::Failed,
                };
                let kind = if event.name == "turn.done" {
                    RowKind::Reply
                } else {
                    RowKind::Error
                };
                rows.push(self.row(kind, Some(event.name), event.data.clone()));
                return Some(self.changed(rows));
            }
            name if name.ends_with(".request") && name != "trust.request" => {
                // The request id and event name are written by BridgeConfirmer, never by content.
                let request = event.data["request"].as_u64()?;
                let kind = name.trim_end_matches(".request");
                let row = self.row(RowKind::Approval, Some(event.name), event.data.clone());
                self.pending = Some(Pending {
                    row: row.id,
                    request,
                    kind: kind.to_string(),
                    supported: matches!(kind, "confirm" | "run" | "fetch" | "ask"),
                    data: event.data.clone(),
                });
                self.status = Status::Waiting;
                return Some(self.changed(vec![row]));
            }
            _ => return None,
        };
        let row = self.row(kind, Some(event.name), event.data.clone());
        Some(self.changed(vec![row]))
    }
}

/// How long `row` is written as JSON. Only the length is taken: nothing in the row is read.
fn serialized_len(row: &Row) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    // Writing a `Value` to a writer that never fails cannot fail.
    let written = serde_json::to_writer(&mut count, row);
    debug_assert!(written.is_ok(), "a row could not be written as JSON");
    count.0
}

/// What a bridge promises about the targets of the actions that answer it (RPCVIEW-6). A bridge
/// that does not advertise it numbers questions per turn, takes a cancel for whatever is running,
/// and accepts a repeated trust answer.
pub fn action_targets() -> Value {
    json!({"version": 1, "questionIds": "session", "cancel": "expected_target", "trust": "once"})
}

/// The method a client calls for a new snapshot when it has missed an update (RPCVIEW-9).
pub const RESYNC: &str = "session.view.resync";

pub fn capability() -> Value {
    json!({"version": 1, "start": "session.view.start", "scope": "fresh_session",
        "approvals": ["confirm", "run", "fetch", "ask"], "resync": RESYNC,
        "retainedRows": RETAINED_ROWS})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every approval carrier retains all fields, including labels the projection does not know.
    #[test]
    fn approval_replacements_preserve_the_payload_and_kind() {
        for kind in ["confirm", "run", "fetch", "ask", "output"] {
            let mut view = View::new(true);
            view.started(4, 4, json!({"text": "prompt"}));
            let data = json!({"request": 7, "remark": {"label": "(U,priv)",
                "preview": ["\u{1b}[2J forged chrome"]}, "future": [1, 2]});
            let name = match kind {
                "confirm" => "confirm.request",
                "run" => "run.request",
                "fetch" => "fetch.request",
                "ask" => "ask.request",
                _ => "output.request",
            };
            let waiting = view.event(&Event::new(name, "s1", data.clone())).unwrap();
            let pending = waiting.pending.unwrap();
            assert_eq!(pending.supported, kind != "output");
            assert_eq!(pending.data, data);
            assert!(view.answered(8).is_none());
            let resolved = view.answered(7).unwrap();
            assert_eq!(resolved.rows[0].id, pending.row);
            assert_eq!(resolved.rows[0].event.as_deref(), Some(name));
            assert_eq!(resolved.rows[0].data, data);
            assert!(resolved.rows[0].resolved);
            assert!(resolved.pending.is_none());
            assert!(view.answered(7).is_none());
        }
    }

    fn status_name(status: Status) -> &'static str {
        // Exhaustive on purpose: a new variant stops compiling here until it is listed, and the
        // contract file below then changes with it.
        match status {
            Status::AwaitingTrust => "awaiting_trust",
            Status::Idle => "idle",
            Status::Running => "running",
            Status::Waiting => "waiting",
            Status::Completed => "completed",
            Status::Failed => "failed",
            Status::Cancelled => "cancelled",
            Status::Detached => "detached",
        }
    }

    fn row_kind_name(kind: RowKind) -> &'static str {
        match kind {
            RowKind::Prompt => "prompt",
            RowKind::Narration => "narration",
            RowKind::Quarantined => "quarantined",
            RowKind::Approval => "approval",
            RowKind::Reply => "reply",
            RowKind::Error => "error",
            RowKind::Activity => "activity",
        }
    }

    /// What the TypeScript client is written against: the capability, every status and row kind
    /// as serde writes them, one update using every field, and a snapshot of the same state that
    /// leaves rows out.
    fn wire_contract() -> Value {
        use RowKind::*;
        let statuses = [
            Status::AwaitingTrust,
            Status::Idle,
            Status::Running,
            Status::Waiting,
            Status::Completed,
            Status::Failed,
            Status::Cancelled,
            Status::Detached,
        ];
        let kinds = [
            Prompt,
            Narration,
            Quarantined,
            Approval,
            Reply,
            Error,
            Activity,
        ];
        for status in statuses {
            assert_eq!(json!(status), json!(status_name(status)));
        }
        for kind in kinds {
            assert_eq!(json!(kind), json!(row_kind_name(kind)));
        }
        let data = json!({"label": "(U,pub)", "text": "released"});
        let rows: Vec<Row> = kinds
            .iter()
            .enumerate()
            .map(|(at, kind)| Row {
                id: at as u64 + 1,
                turn: 1,
                kind: *kind,
                event: (*kind != Prompt).then(|| "tool.started".to_string()),
                data: data.clone(),
                resolved: *kind == Approval,
            })
            .collect();
        let update = Update {
            sequence: 3,
            turn: 1,
            target: 7,
            status: Status::Waiting,
            pending: Some(Pending {
                row: 4,
                request: 7,
                kind: "fetch".into(),
                supported: true,
                data: json!({"request": 7, "url": "https://example.test/", "host": "example.test"}),
            }),
            rows,
            omitted: 2,
        };
        json!({
            "capability": capability(),
            "actionTargets": action_targets(),
            "statuses": statuses.map(status_name),
            "rowKinds": kinds.map(row_kind_name),
            "snapshot": Update { omitted: 4, ..update.clone() },
            "update": update,
        })
    }

    /// The client package's wire types are checked against these Rust types through this file.
    /// Regenerate it with `WRITE_WIRE_CONTRACT=1 cargo test -p bravebot-ui-bridge --lib wire_contract`.
    #[test]
    fn the_client_wire_contract_matches_the_rust_types() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/agent-client/test-fixtures/wire-contract.json");
        let actual = wire_contract();
        if std::env::var_os("WRITE_WIRE_CONTRACT").is_some() {
            let text = serde_json::to_string_pretty(&actual).unwrap() + "\n";
            std::fs::write(&path, text).expect("the contract file is written");
        }
        let text = std::fs::read_to_string(&path).expect("the contract file exists");
        let expected: Value = serde_json::from_str(&text).expect("the contract file is JSON");
        assert_eq!(
            actual, expected,
            "the Rust view types changed; regenerate {path:?} and update packages/agent-client"
        );
    }

    /// One call is one row: a finish replaces the row of the call with its id, whichever order
    /// overlapping calls finish in, and a finish for no call still running stands alone.
    #[test]
    fn a_finished_call_replaces_the_row_of_the_call_it_ends() {
        let mut view = View::new(true);
        view.started(1, 1, json!({"text": "prompt"}));
        // Every call reads the same file, so only the id can tell them apart.
        let started = |view: &mut View, call: Value| {
            let data = json!({"call": call, "verb": "read", "target": "a.rs", "note": null});
            view.event(&Event::new("tool.started", "s1", data))
                .unwrap()
                .rows[0]
                .id
        };
        let finish = |view: &mut View, call: Value, note: &str| {
            let data = json!({"call": call, "verb": "read", "target": "a.rs", "note": note});
            view.event(&Event::new("tool.finished", "s1", data))
                .unwrap()
        };

        let first = started(&mut view, json!(1));
        let finished = finish(&mut view, json!(1), "2 lines");
        assert_eq!(finished.rows.len(), 1);
        assert_eq!(finished.rows[0].id, first);
        assert_eq!(finished.rows[0].event.as_deref(), Some("tool.finished"));
        assert_eq!(finished.rows[0].data["note"], "2 lines");

        // Two calls in flight, the earlier finishing first and then the later finishing first.
        let earlier = started(&mut view, json!(2));
        let later = started(&mut view, json!(3));
        assert_eq!(finish(&mut view, json!(2), "earlier").rows[0].id, earlier);
        assert_eq!(finish(&mut view, json!(3), "later").rows[0].id, later);
        let earlier = started(&mut view, json!(4));
        let later = started(&mut view, json!(5));
        assert_eq!(finish(&mut view, json!(5), "later").rows[0].id, later);
        assert_eq!(finish(&mut view, json!(4), "earlier").rows[0].id, earlier);

        // A finish for a call already finished, for one never started, or without an id gets a
        // row of its own, after every earlier one, and leaves a call still running in place.
        let running = started(&mut view, json!(6));
        for call in [json!(4), json!(99), Value::Null] {
            let alone = finish(&mut view, call, "alone");
            assert!(alone.rows[0].id > running);
        }
        assert_eq!(finish(&mut view, json!(6), "done").rows[0].id, running);

        // A start without an id is not waited on, so a finish without one does not take its row.
        let unnumbered = started(&mut view, Value::Null);
        assert!(finish(&mut view, Value::Null, "alone").rows[0].id > unnumbered);

        // A turn that ends with a call unfinished forgets it, so its finish after the end is a row
        // of its own and does not reuse the row of the call the turn left running.
        let unfinished = started(&mut view, json!(7));
        view.event(&Event::new("turn.done", "s1", json!({"reply": "ok"})));
        assert!(finish(&mut view, json!(7), "late").rows[0].id > unfinished);

        // A new turn forgets a call the earlier one left running, even when that turn never
        // reported an end.
        view.started(2, 2, json!({"text": "again"}));
        let left = started(&mut view, json!(8));
        view.started(3, 3, json!({"text": "once more"}));
        assert!(finish(&mut view, json!(8), "late").rows[0].id > left);
    }

    /// A snapshot holds the most recent rows in the order each first appeared, each in its latest
    /// state, and says how many earlier rows it leaves out. A later state of a row it left out is
    /// sent to clients that hold it and does not come back into the snapshot out of order. The
    /// question waiting is in the snapshot whole even when its row is not.
    #[test]
    fn a_snapshot_keeps_the_most_recent_rows_and_the_question_waiting() {
        let mut view = View::new(true);
        view.started(1, 1, json!({"text": "prompt"}));
        let asked = json!({"request": 3, "path": "a.rs", "remark": {"label": "(U,priv)"}});
        let question = view
            .event(&Event::new("confirm.request", "s1", asked.clone()))
            .unwrap()
            .rows[0]
            .id;
        let call = |call: u64| json!({"call": call, "verb": "read", "target": "a.rs"});
        let first_call = view
            .event(&Event::new("tool.started", "s1", call(1)))
            .unwrap()
            .rows[0]
            .id;
        for at in 0..RETAINED_ROWS as u64 {
            view.event(&Event::new("narration", "s1", json!({"text": at})));
        }
        let last_call = view
            .event(&Event::new("tool.started", "s1", call(2)))
            .unwrap()
            .rows[0]
            .id;

        let snapshot = view.snapshot();
        assert_eq!(snapshot.rows.len(), RETAINED_ROWS);
        assert_eq!(snapshot.omitted, last_call - RETAINED_ROWS as u64);
        assert_eq!(snapshot.rows[0].id, snapshot.omitted + 1);
        assert_eq!(snapshot.rows.last().unwrap().id, last_call);
        assert!(snapshot.rows.iter().all(|row| row.id != question));
        let pending = snapshot.pending.expect("the question is kept");
        assert_eq!((pending.row, pending.data), (question, asked.clone()));

        // Both calls finish: the one left out is sent and stays out, the one kept is replaced.
        let finished = |view: &mut View, at: u64| {
            view.event(&Event::new("tool.finished", "s1", call(at)))
                .unwrap()
                .rows[0]
                .id
        };
        assert_eq!(finished(&mut view, 1), first_call);
        assert_eq!(finished(&mut view, 2), last_call);
        let resolved = view.answered(3).unwrap();
        assert_eq!(resolved.rows[0].id, question);
        assert_eq!(resolved.rows[0].data, asked);

        let snapshot = view.snapshot();
        let ids: Vec<u64> = snapshot.rows.iter().map(|row| row.id).collect();
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "{ids:?}");
        assert_eq!(snapshot.omitted, last_call - RETAINED_ROWS as u64);
        assert_eq!(ids[0], snapshot.omitted + 1);
        assert!(!ids.contains(&first_call) && !ids.contains(&question));
        let last = snapshot.rows.last().unwrap();
        assert_eq!(
            (last.id, last.event.as_deref()),
            (last_call, Some("tool.finished"))
        );
        assert!(snapshot.pending.is_none());
        assert_eq!(snapshot.sequence, view.initial().sequence);
    }

    fn serialized(rows: &[Row]) -> usize {
        rows.iter()
            .map(|row| serde_json::to_vec(row).unwrap().len())
            .sum()
    }

    fn narrate(view: &mut View, bytes: usize) -> Update {
        view.event(&Event::new(
            "narration",
            "s1",
            json!({"text": "x".repeat(bytes)}),
        ))
        .unwrap()
    }

    /// Rows are kept within a byte budget as well as a count, so long replies cannot hold memory
    /// without limit. The most recent rows are kept, and each update says how many rows the view
    /// no longer keeps, so a client following the updates leaves out the same rows.
    #[test]
    fn a_view_keeps_its_most_recent_rows_within_a_byte_budget_and_says_how_many_it_left_out() {
        let mut view = View::new(true);
        view.started(1, 1, json!({"text": "prompt"}));
        let mut last = None;
        for _ in 0..20 {
            last = Some(narrate(&mut view, RETAINED_BYTES / 8));
        }
        let last = last.unwrap();
        let newest = last.rows[0].id;

        let snapshot = view.snapshot();
        assert!(serialized(&snapshot.rows) <= RETAINED_BYTES);
        assert!(
            snapshot.rows.len() < 20,
            "{} rows kept",
            snapshot.rows.len()
        );
        assert_eq!(snapshot.rows.last().unwrap().id, newest);
        assert_eq!(snapshot.rows[0].id, snapshot.omitted + 1);
        assert_eq!(snapshot.omitted, newest - snapshot.rows.len() as u64);
        assert_eq!(last.omitted, snapshot.omitted);
    }

    /// A row is kept whatever its size by the update that sends it, so a client that resyncs
    /// straight after a long reply is sent it. The next row drops it.
    #[test]
    fn a_row_larger_than_the_byte_budget_is_kept_only_while_it_is_the_newest() {
        let mut view = View::new(true);
        view.started(1, 1, json!({"text": "prompt"}));
        let large = narrate(&mut view, RETAINED_BYTES + 1).rows[0].id;
        let snapshot = view.snapshot();
        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!((snapshot.rows[0].id, snapshot.omitted), (large, large - 1));

        let after = narrate(&mut view, 10);
        assert_eq!(after.omitted, large);
        let snapshot = view.snapshot();
        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!((snapshot.rows[0].id, snapshot.omitted), (large + 1, large));
    }

    /// A row longer than the budget is dropped by an update that does not send it, even when it is
    /// the only row kept: one that sends no rows, or only a later state of a row already dropped.
    #[test]
    fn a_row_larger_than_the_byte_budget_is_dropped_by_an_update_that_does_not_send_it() {
        let mut view = View::new(true);
        view.started(1, 1, json!({"text": "prompt"}));
        let call = json!({"call": 1, "verb": "read", "target": "x"});
        view.event(&Event::new("tool.started", "s1", call.clone()));
        let large = narrate(&mut view, RETAINED_BYTES + 1).rows[0].id;
        let finished = view
            .event(&Event::new("tool.finished", "s1", call))
            .unwrap();
        assert_eq!(finished.rows[0].id, large - 1);
        assert_eq!(finished.omitted, large);
        assert!(view.snapshot().rows.is_empty());

        let mut view = View::new(true);
        view.started(1, 1, json!({"text": "prompt"}));
        let large = narrate(&mut view, RETAINED_BYTES + 1).rows[0].id;
        let detached = view.detach();
        assert!(detached.rows.is_empty());
        assert_eq!(detached.omitted, large);
        assert!(view.snapshot().rows.is_empty());
    }

    /// A row replaced by a later state counts once, at its latest size, so finishing calls does
    /// not drop rows that fit.
    #[test]
    fn a_row_replaced_by_a_later_state_counts_once_toward_the_byte_budget() {
        let mut view = View::new(true);
        view.started(1, 1, json!({"text": "prompt"}));
        let call = |call: u64| json!({"call": call, "verb": "read", "target": "x".repeat(RETAINED_BYTES / 8)});
        for at in 1..=6 {
            view.event(&Event::new("tool.started", "s1", call(at)));
        }
        for at in 1..=6 {
            let finished = view
                .event(&Event::new("tool.finished", "s1", call(at)))
                .unwrap();
            assert_eq!(finished.omitted, 0);
        }
        let snapshot = view.snapshot();
        assert_eq!((snapshot.rows.len(), snapshot.omitted), (7, 0));
    }

    /// A later state too long for the budget is kept until a later update, so the client it is sent
    /// to keeps it, and so does a snapshot taken straight after it.
    #[test]
    fn a_long_later_state_of_an_older_row_is_kept_by_the_update_that_sends_it() {
        let mut view = View::new(true);
        view.started(1, 1, json!({"text": "prompt"}));
        let call = |target: String| json!({"call": 1, "verb": "read", "target": target});
        view.event(&Event::new("tool.started", "s1", call("x".into())));
        narrate(&mut view, 10);
        let finished = view
            .event(&Event::new(
                "tool.finished",
                "s1",
                call("x".repeat(RETAINED_BYTES)),
            ))
            .unwrap();
        assert_eq!((finished.rows[0].id, finished.omitted), (2, 1));
        let snapshot = view.snapshot();
        assert_eq!(
            snapshot.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            [2, 3]
        );
        assert_eq!(narrate(&mut view, 10).omitted, 2);
    }

    /// Rows sent again beside long new ones cannot keep the rows after them past the budget: the
    /// rows kept are never more than the budget and the rows of the latest update.
    #[test]
    fn rows_kept_stay_within_the_byte_budget_and_the_latest_update() {
        let mut view = View::new(true);
        let text = |bytes: usize| json!({"text": "x".repeat(bytes)});
        let old = view.row(RowKind::Approval, None, text(10));
        view.changed(vec![old.clone()]);
        for _ in 0..6 {
            let long = view.row(RowKind::Reply, None, text(RETAINED_BYTES * 3 / 5));
            let update = view.changed(vec![old.clone(), long]);
            let kept = serialized(&view.snapshot().rows);
            assert!(
                kept <= RETAINED_BYTES + serialized(&update.rows),
                "{kept} bytes kept after an update of {}",
                serialized(&update.rows)
            );
        }
    }

    /// A failed model request must not leave a client showing a running turn.
    #[test]
    fn failures_have_authoritative_terminal_status() {
        let mut view = View::new(true);
        view.started(1, 1, json!({"text": "prompt"}));
        let failed = view
            .event(&Event::new("turn.error", "s1", json!({"kind": "backend"})))
            .unwrap();
        assert_eq!(failed.status, Status::Failed);
        assert_eq!(failed.rows[0].kind, RowKind::Error);
        assert!(failed.pending.is_none());
    }

    /// A question left on the view after the turn that asked it has ended is one nothing can
    /// answer, and a client that keeps showing it offers a choice that grants nothing. Every way
    /// a turn ends takes the question off the view and replaces its row as resolved, and so does
    /// closing the session (RPCVIEW-2).
    #[test]
    fn a_terminal_update_clears_the_question_still_waiting() {
        let waiting_on_a_question = || {
            let mut view = View::new(true);
            view.started(1, 1, json!({"text": "prompt"}));
            let asked = json!({"request": 7, "path": "a.rs"});
            let waiting = view
                .event(&Event::new("confirm.request", "s1", asked.clone()))
                .unwrap();
            assert_eq!(waiting.status, Status::Waiting);
            let question = waiting.pending.expect("a question is waiting").row;
            (view, asked, question)
        };

        for (name, data, status) in [
            ("turn.done", json!({"reply": "ok"}), Status::Completed),
            ("turn.error", json!({"kind": "backend"}), Status::Failed),
            (
                "turn.error",
                json!({"kind": "cancelled"}),
                Status::Cancelled,
            ),
        ] {
            let (mut view, asked, question) = waiting_on_a_question();

            let ended = view.event(&Event::new(name, "s1", data.clone())).unwrap();

            assert_eq!(ended.status, status, "{name} {data}");
            assert!(ended.pending.is_none(), "{name} {data} left the question");
            assert_eq!(ended.rows.len(), 2, "{name} {data}");
            assert_eq!(ended.rows[0].id, question, "{name} {data}");
            assert!(ended.rows[0].resolved, "{name} {data}");
            assert_eq!(ended.rows[0].data, asked, "{name} {data}");
            assert!(ended.rows[1].id > question, "{name} {data}");
            assert!(view.answered(7).is_none(), "{name} {data}");
        }

        let (mut view, asked, question) = waiting_on_a_question();
        let detached = view.detach();
        assert_eq!(detached.status, Status::Detached);
        assert!(detached.pending.is_none(), "closing left the question");
        assert_eq!(detached.rows.len(), 1);
        assert_eq!(detached.rows[0].id, question);
        assert!(detached.rows[0].resolved);
        assert_eq!(detached.rows[0].data, asked);
        assert!(view.answered(7).is_none());
    }
}
