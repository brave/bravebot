//! The Agent Client Protocol, spoken for an editor that hosts a session.
//!
//! A projection of the bridge onto JSON-RPC 2.0, in the same shape [`crate::wire`] is a projection
//! of the agent's types onto the bridge's own protocol: a request from the editor becomes a bridge
//! request, and an event the bridge emits becomes a notification or a question for the editor. The
//! turn engine, the trust map and every prompt are the ones the desktop window uses. This module
//! owns no decision of its own, and no model output or tool result is ever read as a protocol
//! message: everything leaving here is built with `json!` from fields the bridge sent, as a string
//! value, and everything arriving is a line the editor wrote.
//!
//! Nothing here reads stdin, writes stdout or ends the process. The transport hands lines to
//! [`Acp::handle_line`] and supplies a writer and a waker, which is what keeps this file under the
//! crate's rule about the standard streams.

use crate::bridge::Bridge;
use crate::protocol::{Event, Request};
use bravebot_agent::diff::Change;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

/// Where a line of output goes. Called with the shared state locked, so it must not call back in.
pub type Writer = Box<dyn FnMut(Value) + Send>;

const INVALID_PARAMS: i64 = -32602;
const METHOD_NOT_FOUND: i64 = -32601;
const INTERNAL: i64 = -32603;
const PARSE: i64 = -32700;

const ALLOW_ONCE: &str = "allow-once";
const ALLOW_ALWAYS: &str = "allow-always";
const REJECT_ONCE: &str = "reject-once";

/// A tool call the editor was told about and has not been told the end of.
struct Tool {
    verb: String,
    target: String,
    id: String,
}

#[derive(Default)]
struct Session {
    /// What the trust question is about.
    directory: String,
    /// The editor's id for the `session/prompt` waiting on this session's turn.
    prompt: Option<Value>,
    /// Whether the editor asked to stop that turn, which decides how the prompt ends.
    cancelled: bool,
    tools: Vec<Tool>,
    next_tool: u64,
}

/// A question put to the editor and not yet answered.
enum Asked {
    /// Whether the working directory is trusted, with the prompt waiting on the answer.
    Trust { session: String, turn: Value },
    /// A question the bridge raised, and the request that answers it.
    Bridge {
        session: String,
        reply: String,
        request: u64,
        remember: bool,
    },
}

/// A call for the bridge that an event wants made, which only the thread holding the bridge may do.
struct Queued {
    method: String,
    params: Value,
}

struct Shared {
    write: Writer,
    wake: Box<dyn Fn() + Send>,
    sessions: HashMap<String, Session>,
    /// Sessions the bridge asked the trust question about before `session/new` had returned.
    unasked: HashSet<String>,
    asked: HashMap<u64, Asked>,
    next: u64,
    queued: Vec<Queued>,
}

impl Shared {
    fn send(&mut self, value: Value) {
        (self.write)(value);
    }

    fn notify(&mut self, session: &str, update: Value) {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": { "sessionId": session, "update": update },
        }));
    }

    fn reply(&mut self, id: Value, outcome: Result<Value, (i64, String)>) {
        self.send(match outcome {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
            }
        });
    }

    fn ask(&mut self, session: &str, asked: Asked, tool_call: Value, options: Value) {
        self.next += 1;
        let id = self.next;
        self.asked.insert(id, asked);
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session/request_permission",
            "params": { "sessionId": session, "toolCall": tool_call, "options": options },
        }));
    }
}

/// An ACP server over one bridge.
pub struct Acp {
    bridge: Bridge,
    shared: Arc<Mutex<Shared>>,
}

impl Acp {
    /// `write` takes each message as one value, to be written as one line. `wake` is called, from
    /// any thread, when [`Acp::drain`] has something to do.
    pub fn new(
        write: Writer,
        wake: Box<dyn Fn() + Send>,
        settings: Option<std::path::PathBuf>,
    ) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            write,
            wake,
            sessions: HashMap::new(),
            unasked: HashSet::new(),
            asked: HashMap::new(),
            next: 0,
            queued: Vec::new(),
        }));
        let heard = Arc::clone(&shared);
        let bridge =
            Bridge::new(Box::new(move |event| on_event(&heard, event))).with_settings(settings);
        Self { bridge, shared }
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Make the calls events have asked for. Called when woken, and after every line.
    pub fn drain(&mut self) {
        let queued = std::mem::take(&mut self.lock().queued);
        for call in queued {
            let _ = self.call(&call.method, call.params);
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, crate::protocol::Failure> {
        self.bridge.dispatch(&Request {
            id: 0,
            method: method.to_string(),
            params,
        })
    }

    /// Read one line the editor wrote.
    pub fn handle_line(&mut self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            self.lock()
                .reply(Value::Null, Err((PARSE, "not JSON".into())));
            return;
        };
        match (
            message.get("method").and_then(Value::as_str),
            message.get("id"),
        ) {
            (Some(method), Some(id)) => {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                self.request(id.clone(), method, params);
            }
            (Some(method), None) => {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                self.notification(method, &params);
            }
            (None, Some(_)) => self.answer(&message),
            (None, None) => {}
        }
        self.drain();
    }

    fn request(&mut self, id: Value, method: &str, params: Value) {
        let outcome = match method {
            "initialize" => Ok(initialized()),
            "authenticate" => Ok(json!({})),
            "session/new" => self.new_session(&params),
            "session/set_mode" => self.set_mode(&params),
            "session/prompt" => {
                // Answered when the turn ends, or here if it cannot start.
                self.prompt(id, &params);
                return;
            }
            other => Err((METHOD_NOT_FOUND, format!("unknown method `{other}`"))),
        };
        self.lock().reply(id, outcome);
    }

    fn notification(&mut self, method: &str, params: &Value) {
        if method != "session/cancel" {
            return;
        }
        let Some(session) = params.get("sessionId").and_then(Value::as_str) else {
            return;
        };
        let waiting_on_trust = {
            let mut shared = self.lock();
            let Some(held) = shared.sessions.get_mut(session) else {
                return;
            };
            held.cancelled = true;
            let before = shared.asked.len();
            shared.asked.retain(
                |_, asked| !matches!(asked, Asked::Trust { session: waiting, .. } if waiting == session),
            );
            let waiting = shared.asked.len() != before;
            if waiting {
                finish(&mut shared, session, "cancelled");
            }
            waiting
        };
        if waiting_on_trust {
            // The question is still the bridge's to have answered, and a prompt that never starts
            // a turn is no reason to leave the session unable to start one later. Unanswered
            // means untrusted.
            let _ = self.call(
                "trust.reply",
                json!({ "session": session, "trusted": false }),
            );
        } else {
            let _ = self.call("turn.cancel", json!({ "session": session }));
        }
    }

    fn new_session(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let cwd = params
            .get("cwd")
            .and_then(Value::as_str)
            .filter(|cwd| Path::new(cwd).is_absolute())
            .ok_or((INVALID_PARAMS, "`cwd` must be an absolute path".to_string()))?;
        // Servers the editor lists are not started: a server is declared in a person's own
        // directory and asked about before it starts, and an editor's list is neither.
        let made = self
            .call("session.new", json!({ "directory": cwd }))
            .map_err(failed)?;
        let handle = made["session"].as_str().unwrap_or_default().to_string();
        let mut shared = self.lock();
        shared.sessions.insert(
            handle.clone(),
            Session {
                directory: cwd.to_string(),
                ..Session::default()
            },
        );
        Ok(json!({ "sessionId": handle, "modes": modes("ask") }))
    }

    fn set_mode(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let session = params.get("sessionId").and_then(Value::as_str);
        let mode = params.get("modeId").and_then(Value::as_str);
        let (Some(session), Some(mode)) = (session, mode) else {
            return Err((
                INVALID_PARAMS,
                "`sessionId` and `modeId` are required".into(),
            ));
        };
        self.call("session.mode", json!({ "session": session, "mode": mode }))
            .map_err(failed)?;
        Ok(json!({}))
    }

    fn prompt(&mut self, id: Value, params: &Value) {
        let session = params
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let turn = match turn_params(&session, params.get("prompt")) {
            Ok(turn) => turn,
            Err(message) => {
                self.lock().reply(id, Err((INVALID_PARAMS, message)));
                return;
            }
        };
        let trust = {
            let mut shared = self.lock();
            let Some(held) = shared.sessions.get_mut(&session) else {
                shared.reply(id, Err((INVALID_PARAMS, "unknown session".into())));
                return;
            };
            if held.prompt.is_some() {
                shared.reply(
                    id,
                    Err((INVALID_PARAMS, "a prompt is already running".into())),
                );
                return;
            }
            held.prompt = Some(id);
            held.cancelled = false;
            let directory = held.directory.clone();
            shared.unasked.remove(&session).then_some(directory)
        };
        if let Some(directory) = trust {
            let mut shared = self.lock();
            let title = format!("Trust {}", plain(&directory));
            shared.ask(
                &session.clone(),
                Asked::Trust { session, turn },
                json!({ "toolCallId": "trust", "title": title, "kind": "other", "status": "pending" }),
                options(false, "Trust this directory", "Do not trust"),
            );
            return;
        }
        self.start_turn(&session, turn);
    }

    fn start_turn(&mut self, session: &str, turn: Value) {
        // Answered before the turn starts, so a turn that ends first finds its prompt.
        let Err(failure) = self.call("turn.send", turn) else {
            return;
        };
        let mut shared = self.lock();
        if let Some(held) = shared.sessions.get_mut(session)
            && let Some(id) = held.prompt.take()
        {
            shared.reply(id, Err(failed(failure)));
        }
    }

    /// The editor's answer to a question this side put.
    fn answer(&mut self, message: &Value) {
        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            return;
        };
        let Some(asked) = self.lock().asked.remove(&id) else {
            return;
        };
        let chosen = chosen(message);
        match asked {
            Asked::Trust { session, turn } => {
                let trusted = chosen.is_some();
                let _ = self.call(
                    "trust.reply",
                    json!({ "session": session, "trusted": trusted }),
                );
                if self
                    .lock()
                    .sessions
                    .get(&session)
                    .is_some_and(|held| held.prompt.is_some())
                {
                    self.start_turn(&session, turn);
                }
            }
            Asked::Bridge {
                session,
                reply,
                request,
                remember,
            } => {
                let mut params = json!({
                    "session": session,
                    "request": request,
                    "decision": if chosen.is_some() { "approve" } else { "reject" },
                });
                // A standing answer only where the question offered one, and otherwise the single
                // yes the same choice means.
                if chosen == Some(ALLOW_ALWAYS) && remember {
                    params["remember"] = json!(true);
                }
                let _ = self.call(&reply, params);
            }
        }
    }
}

/// The option the editor selected, if it selected one that approves.
///
/// Anything else is a refusal: no outcome, a cancelled one, an error from a client that cannot
/// answer, an option this side never offered, or a selection of the refusing option.
fn chosen(message: &Value) -> Option<&'static str> {
    let outcome = message.get("result")?.get("outcome")?;
    if outcome.get("outcome")?.as_str()? != "selected" {
        return None;
    }
    match outcome.get("optionId")?.as_str()? {
        ALLOW_ONCE => Some(ALLOW_ONCE),
        ALLOW_ALWAYS => Some(ALLOW_ALWAYS),
        _ => None,
    }
}

fn failed(failure: crate::protocol::Failure) -> (i64, String) {
    let code = match failure.code {
        crate::protocol::ErrorCode::BadRequest | crate::protocol::ErrorCode::NotADirectory => {
            INVALID_PARAMS
        }
        _ => INTERNAL,
    };
    (code, failure.message)
}

fn initialized() -> Value {
    json!({
        "protocolVersion": 1,
        "agentCapabilities": {
            "loadSession": false,
            "promptCapabilities": { "image": true, "audio": false, "embeddedContext": false },
            "mcpCapabilities": { "http": false, "sse": false },
        },
        "agentInfo": {
            "name": "bravebot",
            "title": "Bravebot",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "authMethods": [],
    })
}

fn modes(current: &str) -> Value {
    json!({
        "currentModeId": current,
        "availableModes": [
            { "id": "ask", "name": "Ask" },
            { "id": "acceptEdits", "name": "Accept edits" },
            { "id": "plan", "name": "Plan" },
        ],
    })
}

fn options(remember: bool, allow: &str, reject: &str) -> Value {
    let mut all = vec![json!({ "optionId": ALLOW_ONCE, "name": allow, "kind": "allow_once" })];
    if remember {
        all.push(json!({
            "optionId": ALLOW_ALWAYS,
            "name": "Allow and remember",
            "kind": "allow_always",
        }));
    }
    all.push(json!({ "optionId": REJECT_ONCE, "name": reject, "kind": "reject_once" }));
    Value::Array(all)
}

/// Control characters pictured, as the terminal pictures them, so text the editor draws cannot
/// carry its own escapes. A newline stays one.
fn plain(text: &str) -> String {
    text.split('\n')
        .map(bravebot_approval::printable)
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------- the prompt

/// The `turn.send` a `session/prompt` is.
///
/// The text blocks are the words a person typed, and are sent as such. Everything else an editor
/// attaches is carried as the bridge carries the same material from a window: a picture as a
/// pasted one, a linked file as a dropped one, and an embedded resource or audio is refused,
/// because the only way to carry its bytes would be to put them in the words.
fn turn_params(session: &str, blocks: Option<&Value>) -> Result<Value, String> {
    let blocks = blocks
        .and_then(Value::as_array)
        .ok_or("`prompt` must be a list of content blocks")?;
    let mut text = Vec::new();
    let mut images = Vec::new();
    let mut attachments = Vec::new();
    let mut dropped = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => text.push(
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or("a text block holds a string `text`")?
                    .to_string(),
            ),
            Some("image") => {
                let (Some(data), Some(media)) = (
                    block.get("data").and_then(Value::as_str),
                    block.get("mimeType").and_then(Value::as_str),
                ) else {
                    return Err("an image block holds a string `data` and `mimeType`".into());
                };
                images.push(json!({ "media": media, "data": data }));
            }
            Some("resource_link") => {
                let uri = block
                    .get("uri")
                    .and_then(Value::as_str)
                    .ok_or("a resource link holds a string `uri`")?;
                let path = path_of(uri)
                    .ok_or_else(|| format!("only a file: link can be attached, not {uri}"))?;
                if bravebot_agent::workspace::media_for(&path).is_some() {
                    attachments.push(path);
                } else {
                    dropped.push(path);
                }
            }
            Some(other) => {
                return Err(format!(
                    "a `{other}` block is not accepted; the agent advertises text, images and links"
                ));
            }
            None => return Err("a content block names its `type`".into()),
        }
    }
    Ok(json!({
        "session": session,
        "prompt": text.join("\n"),
        "images": images,
        "attachments": attachments,
        "dropped": dropped,
    }))
}

/// The local path a `file:` URI names.
fn path_of(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    // An authority is empty or `localhost`; another host names a file on another machine.
    let path = match rest.find('/') {
        Some(0) => rest,
        Some(at) if &rest[..at] == "localhost" => &rest[at..],
        _ => return None,
    };
    let mut bytes = Vec::new();
    let mut raw = path.bytes();
    while let Some(byte) = raw.next() {
        if byte == b'%' {
            let high = raw.next()?;
            let low = raw.next()?;
            let pair = [high, low];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&pair).ok()?, 16).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes)
        .ok()
        .filter(|path| !path.contains('\0'))
}

// ---------------------------------------------------------------- events

fn finish(shared: &mut Shared, session: &str, reason: &str) {
    // What the editor was still asked is moot with the turn, and an answer to it later has
    // nothing to answer.
    shared.asked.retain(|_, asked| match asked {
        Asked::Bridge { session: of, .. } | Asked::Trust { session: of, .. } => of != session,
    });
    let Some(held) = shared.sessions.get_mut(session) else {
        return;
    };
    held.tools.clear();
    // A prompt the editor asked to stop ends as stopped, whichever way its turn ended.
    let reason = if held.cancelled { "cancelled" } else { reason };
    if let Some(id) = held.prompt.take() {
        shared.reply(id, Ok(json!({ "stopReason": reason })));
    }
}

fn on_event(shared: &Arc<Mutex<Shared>>, event: Event) {
    let mut shared = shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(session) = event.session.clone() else {
        return;
    };
    let data = &event.data;
    match event.name {
        "trust.request" => {
            shared.unasked.insert(session);
        }
        "narration" => {
            if let Some(text) = data["text"].as_str() {
                say(&mut shared, &session, text);
            }
        }
        "todos" => {
            let entries: Vec<Value> = data["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|row| {
                    let status = match row["status"].as_str() {
                        Some("active") => "in_progress",
                        Some("done") => "completed",
                        _ => "pending",
                    };
                    json!({
                        "content": plain(row["content"].as_str().unwrap_or_default()),
                        "priority": "medium",
                        "status": status,
                    })
                })
                .collect();
            shared.notify(
                &session,
                json!({ "sessionUpdate": "plan", "entries": entries }),
            );
        }
        "tool.started" => {
            let verb = data["verb"].as_str().unwrap_or_default().to_string();
            let target = data["target"].as_str().unwrap_or_default().to_string();
            let Some(held) = shared.sessions.get_mut(&session) else {
                return;
            };
            held.next_tool += 1;
            let id = format!("tool-{}", held.next_tool);
            held.tools.push(Tool {
                verb: verb.clone(),
                target: target.clone(),
                id: id.clone(),
            });
            shared.notify(
                &session,
                json!({
                    "sessionUpdate": "tool_call",
                    "toolCallId": id,
                    "title": plain(&format!("{verb} {target}")),
                    "kind": "other",
                    "status": "in_progress",
                }),
            );
        }
        "tool.finished" => {
            let verb = data["verb"].as_str().unwrap_or_default();
            let target = data["target"].as_str().unwrap_or_default();
            let status = if data["failed"].as_bool() == Some(true) {
                "failed"
            } else {
                "completed"
            };
            let Some(held) = shared.sessions.get_mut(&session) else {
                return;
            };
            let Some(at) = held
                .tools
                .iter()
                .position(|tool| tool.verb == verb && tool.target == target)
            else {
                return;
            };
            let tool = held.tools.remove(at);
            shared.notify(
                &session,
                json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": tool.id,
                    "status": status,
                }),
            );
        }
        "turn.done" => {
            if let Some(text) = data["reply"].as_str() {
                say(&mut shared, &session, text);
            }
            finish(&mut shared, &session, "end_turn");
        }
        "turn.error" => {
            let cancelled = data["kind"].as_str() == Some("cancelled");
            let stopped = shared
                .sessions
                .get(&session)
                .is_some_and(|held| held.cancelled);
            if cancelled || stopped {
                finish(&mut shared, &session, "cancelled");
                return;
            }
            let why = data["message"].as_str().unwrap_or("the turn failed");
            let message = format!("the turn failed: {}", plain(why));
            shared.asked.retain(|_, asked| match asked {
                Asked::Bridge { session: of, .. } | Asked::Trust { session: of, .. } => {
                    *of != session
                }
            });
            let Some(held) = shared.sessions.get_mut(&session) else {
                return;
            };
            held.tools.clear();
            if let Some(id) = held.prompt.take() {
                shared.reply(id, Err((INTERNAL, message)));
            }
        }
        name if name.ends_with(".request") => question(&mut shared, &session, name, data),
        _ => {}
    }
}

fn say(shared: &mut Shared, session: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    shared.notify(
        session,
        json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": text },
        }),
    );
}

/// Put a bridge question to the editor, or refuse it where it cannot be put.
fn question(shared: &mut Shared, session: &str, name: &str, data: &Value) {
    let Some(request) = data["request"].as_u64() else {
        return;
    };
    let Some(stem) = name.strip_suffix(".request") else {
        return;
    };
    let reply = format!("{stem}.reply");
    // A question of the planner's own is not a permission, and nothing here can put its rows to
    // an editor as one. Saying nothing is how that question says nobody could be asked.
    if stem == "ask" {
        shared.queued.push(Queued {
            method: reply,
            params: json!({ "session": session, "request": request, "answers": [] }),
        });
        (shared.wake)();
        return;
    }
    let remember = match stem {
        "run" => data["canBeRemembered"].as_bool() == Some(true),
        "mcp-call" => data["mayStand"].as_bool() == Some(true),
        _ => false,
    };
    let (title, kind, text) = describe(stem, data);
    shared.ask(
        session,
        Asked::Bridge {
            session: session.to_string(),
            reply,
            request,
            remember,
        },
        json!({
            "toolCallId": format!("question-{request}"),
            "title": title,
            "kind": kind,
            "status": "pending",
            "content": [{ "type": "content", "content": { "type": "text", "text": text } }],
            "rawInput": data,
        }),
        options(remember, "Allow once", "Reject"),
    );
}

/// The text of a write's question: the lines the terminal shows for the same write, read back out
/// of the bridge's own description of it.
fn write_text(data: &Value) -> String {
    let strings = |list: &Value| -> Vec<String> {
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(|line| line.as_str().map(str::to_string))
            .collect()
    };
    let changes: Vec<Change> = data["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|held| {
            let text = || held["text"].as_str().unwrap_or_default().to_string();
            match held["kind"].as_str() {
                Some("added") => Change::Added(text()),
                Some("removed") => Change::Removed(text()),
                Some("kept") => Change::Kept(text()),
                _ => Change::Elided(held["lines"].as_u64().map_or(0, |n| n as usize)),
            }
        })
        .collect();
    let remark = data["remark"]["preview"]
        .is_array()
        .then(|| strings(&data["remark"]["preview"]));
    let credentials = strings(&data["credentials"]);
    let count = |key: &str| data[key].as_u64().map_or(0, |n| n as usize);
    bravebot_approval::write_lines(&bravebot_approval::Write {
        untrusted: data["untrusted"].as_bool() == Some(true),
        remark: remark.as_deref(),
        credentials: &credentials,
        written_since_checkout: data["writtenSinceCheckout"].as_bool() == Some(true),
        line_endings: data["lineEndings"].as_str(),
        exact: data["exact"].as_bool() != Some(false),
        added: count("added"),
        removed: count("removed"),
        changes: &changes,
    })
    .join("\n")
}

/// What a question is about, for a person reading it: a title, the kind of call, and the text the
/// drawn prompt shows.
fn describe(stem: &str, data: &Value) -> (String, &'static str, String) {
    match stem {
        "confirm" => {
            let path = data["path"].as_str().unwrap_or_default();
            let intent = data["intent"].as_str().unwrap_or("write");
            (plain(&format!("{intent} {path}")), "edit", write_text(data))
        }
        "run" => {
            let plan = data["plan"].as_str().unwrap_or_default();
            let mut text = plan.to_string();
            if let Some(directory) = data["directory"].as_str() {
                text.push_str(&format!("\nin {directory}"));
            }
            if let Some(summary) = data["summary"].as_str() {
                text.push_str(&format!("\n{summary}"));
            }
            (plain(&format!("run {plan}")), "execute", plain(&text))
        }
        "fetch" => {
            let url = data["url"].as_str().unwrap_or_default();
            (plain(&format!("fetch {url}")), "fetch", plain(url))
        }
        other => {
            let text = data["summary"]
                .as_str()
                .map_or_else(|| data.to_string(), str::to_string);
            (plain(other), "other", plain(&text))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_link_names_the_path_it_spells_and_no_other_host_is_read() {
        assert_eq!(
            path_of("file:///home/me/my%20notes.txt").as_deref(),
            Some("/home/me/my notes.txt")
        );
        assert_eq!(
            path_of("file://localhost/home/me/a.png").as_deref(),
            Some("/home/me/a.png")
        );
        assert_eq!(path_of("file://other-host/home/me/a.png"), None);
        assert_eq!(path_of("https://example.com/a.png"), None);
        assert_eq!(path_of("file:///bad%zz"), None);
    }

    #[test]
    fn a_file_link_that_is_not_utf8_names_no_path_and_its_lossy_lookalike_names_its_own() {
        assert_eq!(path_of("file:///x/tool-%ff"), None);
        assert_eq!(
            path_of("file:///x/tool-%EF%BF%BD").as_deref(),
            Some("/x/tool-\u{fffd}")
        );
    }

    #[test]
    fn only_a_selected_approving_option_approves() {
        let selected = |option: &str| json!({"result": {"outcome": {"outcome": "selected", "optionId": option}}});
        assert_eq!(chosen(&selected(ALLOW_ONCE)), Some(ALLOW_ONCE));
        assert_eq!(chosen(&selected(ALLOW_ALWAYS)), Some(ALLOW_ALWAYS));
        assert_eq!(chosen(&selected(REJECT_ONCE)), None);
        assert_eq!(chosen(&selected("ALLOW-ONCE")), None);
        assert_eq!(
            chosen(
                &json!({"result": {"outcome": {"outcome": "cancelled", "optionId": ALLOW_ONCE}}})
            ),
            None
        );
        assert_eq!(
            chosen(&json!({"error": {"code": -32601, "message": "no"}})),
            None
        );
        assert_eq!(chosen(&json!({})), None);
    }

    /// The terminal's lines for a write come from `bravebot_approval`, and the editor's text is
    /// those lines read back out of the bridge's JSON, so every field of the request has to cross.
    #[test]
    fn a_write_question_reads_to_the_editor_as_it_reads_in_the_terminal() {
        use bravebot_agent::confirm::{Intent, Remark, WriteRequest};
        use bravebot_agent::diff::Diff;

        let old: String = (0..30).map(|n| format!("keep {n}\n")).collect();
        let new = format!("{old}added\u{1b}[2J\n").replacen("keep 0\n", "changed\n", 1);
        let request = WriteRequest {
            written_since_checkout: true,
            path: "notes.md".to_string(),
            contents: new.clone(),
            existing: Some(old.clone()),
            diff: Diff::compute(&old, &new),
            intent: Intent::Edit,
            untrusted: true,
            remark: Some(Remark {
                preview: vec!["fixed a typo".to_string()],
                lines: 1,
                label: "untrusted".to_string(),
            }),
            credentials: vec!["API_KEY on line 31".to_string()],
            may_always: false,
            record: None,
        };
        let changes = request.diff.condensed(2);
        assert!(
            changes.iter().any(|held| matches!(held, Change::Elided(_))),
            "the fixture has no elided run, so it cannot show one crossing"
        );
        let line_endings = request.line_endings_note();
        let expected = bravebot_approval::write_lines(&bravebot_approval::Write {
            untrusted: true,
            remark: Some(&["fixed a typo".to_string()]),
            credentials: &["API_KEY on line 31".to_string()],
            written_since_checkout: true,
            line_endings: line_endings.as_deref(),
            exact: true,
            added: request.diff.added(),
            removed: request.diff.removed(),
            changes: &changes,
        })
        .join("\n");

        let data = crate::wire::write_request(7, &request);
        assert_eq!(write_text(&data), expected);
        assert!(expected.contains("\u{241b}[2J") && !expected.contains('\u{1b}'));
    }
}
