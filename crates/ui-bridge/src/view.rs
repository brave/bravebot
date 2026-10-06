//! An opt-in display projection. Payloads are copied whole, including their labels.
//!
//! Only bridge-owned event names and identity/status fields select transitions. This module
//! never reads released text, formats it, or returns display data to execution.

use crate::protocol::Event;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
    pub status: Status,
    pub pending: Option<Pending>,
    pub rows: Vec<Row>,
}

/// Retains only current control/display metadata. Clients retain their own transcript rows.
/// No history snapshot or reconnect is offered by version 1.
pub(crate) struct View {
    sequence: u64,
    turn: u64,
    status: Status,
    next_row: u64,
    pending: Option<Pending>,
}

impl View {
    pub fn new(trusted: bool) -> Self {
        Self {
            sequence: 0,
            turn: 0,
            status: if trusted {
                Status::Idle
            } else {
                Status::AwaitingTrust
            },
            next_row: 0,
            pending: None,
        }
    }

    pub fn initial(&self) -> Update {
        self.update(Vec::new())
    }

    fn update(&self, rows: Vec<Row>) -> Update {
        Update {
            sequence: self.sequence,
            turn: self.turn,
            status: self.status,
            pending: self.pending.clone(),
            rows,
        }
    }

    fn changed(&mut self, rows: Vec<Row>) -> Update {
        self.sequence += 1;
        self.update(rows)
    }

    fn row(&mut self, kind: RowKind, event: Option<&str>, data: Value) -> Row {
        self.next_row += 1;
        Row {
            id: self.next_row,
            turn: self.turn,
            kind,
            event: event.map(str::to_string),
            data,
            resolved: false,
        }
    }

    pub fn started(&mut self, turn: u64, prompt: Value) -> Update {
        self.turn = turn;
        self.status = Status::Running;
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
            .map(|p| Row {
                id: p.row,
                turn: self.turn,
                kind: RowKind::Approval,
                event: Some(format!("{}.request", p.kind)),
                data: p.data,
                resolved: true,
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
            "tool.started" | "tool.finished" => RowKind::Activity,
            "turn.done" | "turn.error" => {
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

pub fn capability() -> Value {
    json!({"version": 1, "start": "session.view.start", "scope": "fresh_session",
        "approvals": ["confirm", "run", "fetch", "ask"], "reconnect": false})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every approval carrier retains all fields, including labels the projection does not know.
    #[test]
    fn approval_replacements_preserve_the_payload_and_kind() {
        for kind in ["confirm", "run", "fetch", "ask", "output"] {
            let mut view = View::new(true);
            view.started(4, json!({"text": "prompt"}));
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

    /// A failed model request must not leave a client showing a running turn.
    #[test]
    fn failures_have_authoritative_terminal_status() {
        let mut view = View::new(true);
        view.started(1, json!({"text": "prompt"}));
        let failed = view
            .event(&Event::new("turn.error", "s1", json!({"kind": "backend"})))
            .unwrap();
        assert_eq!(failed.status, Status::Failed);
        assert_eq!(failed.rows[0].kind, RowKind::Error);
        assert!(failed.pending.is_none());
    }
}
