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
/// carry its own escapes. A newline stays one, and so does the pair `\r\n`, which is how a file
/// from another system ends its lines and is not a character anybody hid.
fn pictured(text: &str) -> String {
    text.replace("\r\n", "\n")
        .split('\n')
        .map(bravebot_approval::printable)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Text the model or a tool wrote, for a place an editor may draw as markdown: pictured, and with
/// what would make the editor fetch something unasked taken out of it.
fn plain(text: &str) -> String {
    without_loading(&pictured(text))
}

/// The tags that make a renderer open a connection to draw them.
const LOADING_TAGS: [&str; 14] = [
    "img", "image", "picture", "source", "video", "audio", "track", "iframe", "frame", "embed",
    "object", "link", "script", "svg",
];

/// `text` with no markdown image and no HTML tag that loads a resource. An editor draws an image
/// the moment a message arrives, with no click, so `![](https://host/?q=<what the model read>)`
/// would carry out whatever the model put in the address. Each is cut by a space, which is visible
/// and leaves the address in the message for a reader to see. A link is left alone, since drawing
/// one loads nothing.
fn without_loading(text: &str) -> String {
    let text = text.replace("![", "! [");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..=at]);
        rest = &rest[at + 1..];
        let name = rest
            .split(|c: char| !c.is_ascii_alphanumeric())
            .next()
            .unwrap_or_default();
        if LOADING_TAGS
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case(name))
        {
            out.push(' ');
        }
    }
    out.push_str(rest);
    out
}

/// `text` as a markdown code block no content of its own can end, so an editor shows it as it is
/// and an image or a tag in it is drawn as characters.
fn fenced(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let fence = "`".repeat((longest + 1).max(3));
    format!("{fence}\n{text}\n{fence}")
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
            "content": { "type": "text", "text": plain(text) },
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
    // A question this side cannot show is refused rather than shown as whatever the bridge sent,
    // since an approval given on a dump of fields is not one anybody read.
    let Some((title, kind, text)) = describe(stem, data) else {
        shared.queued.push(Queued {
            method: reply,
            params: json!({ "session": session, "request": request, "decision": "reject" }),
        });
        (shared.wake)();
        return;
    };
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
            "content": [{ "type": "content", "content": { "type": "text", "text": fenced(&text) } }],
        }),
        options(remember, "Allow once", "Reject"),
    );
}

/// The text of a write's question: the lines the terminal shows for the same write, read back out
/// of the bridge's own description of it.
fn write_text(data: &Value) -> String {
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

fn strings(list: &Value) -> Vec<String> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|line| line.as_str().map(str::to_string))
        .collect()
}

fn text_of<'a>(data: &'a Value, key: &str) -> &'a str {
    data[key].as_str().unwrap_or_default()
}

/// What a check made of the content, read from the verdict word the bridge sent. The checker's own
/// sentence is not read, for the reason the terminal leaves it out.
fn check_of(data: &Value) -> bravebot_approval::Check {
    bravebot_approval::Check::from_word(data["vetting"]["verdict"].as_str().unwrap_or_default())
}

/// The sentence for each access a line reaches that nothing holds. `None` where one is of a kind
/// this side does not know, since a question that leaves one out is not the question that was asked.
fn ambient_of(data: &Value) -> Option<Vec<String>> {
    data["ambient"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|spent| {
            let authority =
                bravebot_core::ambient::Authority::from_name(spent["authority"].as_str()?)?;
            Some(bravebot_agent::confirm::authority_sentence(
                authority,
                spent["named"].as_str().unwrap_or_default(),
            ))
        })
        .collect()
}

fn run_text(data: &Value) -> Option<String> {
    let steps: Vec<bravebot_approval::RunStep> = data["stages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|stage| bravebot_approval::RunStep {
            written: text_of(stage, "display").to_string(),
            binary: text_of(stage, "binary").to_string(),
        })
        .collect();
    let confinement = data["confinement"].is_object().then(|| {
        let confined = &data["confinement"];
        bravebot_approval::Confinement {
            heading: text_of(confined, "heading").to_string(),
            directories: strings(&confined["directories"]),
            sentences: strings(&confined["sentences"]),
        }
    });
    Some(
        bravebot_approval::run_lines(&bravebot_approval::Run {
            summary: text_of(data, "summary"),
            steps: &steps,
            writes: &strings(&data["writes"]),
            confinement: confinement.as_ref(),
            ambient: &ambient_of(data)?,
            releases_private: data["releasesPrivate"].as_bool() == Some(true),
        })
        .join("\n"),
    )
}

fn vet_text(data: &Value) -> String {
    let picture = data["picture"]["path"].as_str().map(|path| {
        let pdf = data["picture"]["media"].as_str() == Some(bravebot_core::vetting::PDF);
        (path, pdf)
    });
    bravebot_approval::vet_lines(&bravebot_approval::Vet {
        summary: text_of(data, "summary"),
        expects: text_of(data, "expects"),
        check: check_of(data),
        content: text_of(data, "content"),
        picture: picture.map(|(path, pdf)| bravebot_approval::Picture { path, pdf }),
    })
    .join("\n")
}

fn tools_text(data: &Value) -> String {
    let arguments: Vec<Vec<String>> = data["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|tool| strings(&tool["arguments"]))
        .collect();
    let tools: Vec<bravebot_approval::Tool<'_>> = data["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .zip(&arguments)
        .map(|(tool, arguments)| bravebot_approval::Tool {
            name: text_of(tool, "name"),
            arguments,
            description: tool["description"].as_str(),
        })
        .collect();
    bravebot_approval::tools_lines(&bravebot_approval::Tools {
        alias: text_of(data, "alias"),
        tools: &tools,
        refused: data["refused"].as_u64().map_or(0, |n| n as usize),
        changed: data["changed"].as_bool() == Some(true),
        check: check_of(data),
    })
    .join("\n")
}

fn call_text(data: &Value) -> String {
    let arguments: Vec<(String, String)> = data["arguments"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|argument| {
            (
                text_of(argument, "name").to_string(),
                text_of(argument, "value").to_string(),
            )
        })
        .collect();
    bravebot_approval::call_lines(&bravebot_approval::Call {
        name: text_of(data, "name"),
        arguments: &arguments,
        description: data["description"].as_str(),
    })
    .join("\n")
}

/// What a question is about, for a person reading it: a title, the kind of call, and the text the
/// drawn prompt shows, which is the terminal's own lines for the same question. `None` for a
/// question there is no way to show.
fn describe(stem: &str, data: &Value) -> Option<(String, &'static str, String)> {
    let text = |key: &str| text_of(data, key);
    let (title, kind, lines) = match stem {
        "confirm" => (
            format!(
                "{} {}",
                data["intent"].as_str().unwrap_or("write"),
                text("path")
            ),
            "edit",
            write_text(data),
        ),
        "run" => (format!("run {}", text("plan")), "execute", run_text(data)?),
        "fetch" => (
            format!("fetch {}", text("url")),
            "fetch",
            bravebot_approval::fetch_lines(
                text("host"),
                text("url"),
                data["ambient"]
                    .as_array()
                    .is_some_and(|ambient| !ambient.is_empty()),
            )
            .join("\n"),
        ),
        "output" => (
            format!("read the output of {}", text("command")),
            "read",
            bravebot_approval::output_lines(text("summary"), check_of(data), text("output"))
                .join("\n"),
        ),
        "vouch" => (
            format!("trust {}", text("path")),
            "read",
            bravebot_approval::vouch_lines(text("path"), check_of(data), text("preview"))
                .join("\n"),
        ),
        "vet" => (format!("read {}", text("origin")), "read", vet_text(data)),
        "manifest" => (
            format!("run the plan for {}", text("task")),
            "execute",
            bravebot_approval::manifest_lines(text("task"), &strings(&data["steps"])).join("\n"),
        ),
        "exposure" => (
            format!("read {}, which holds a credential", text("path")),
            "read",
            bravebot_approval::exposure_lines(text("path"), &strings(&data["credentials"]))
                .join("\n"),
        ),
        "server" => (
            format!("start the {} language server", text("language")),
            "execute",
            bravebot_approval::server_lines(&bravebot_approval::Server {
                summary: text("summary"),
                program: text("program"),
                workspace: text("workspace"),
                runs_build_tooling: data["runsBuildTooling"].as_bool() == Some(true),
            })
            .join("\n"),
        ),
        "mcp-server" => (
            format!("start the MCP server {}", text("alias")),
            "execute",
            strings(&data["lines"])
                .iter()
                .map(|line| pictured(line))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        "mcp-tools" => (
            format!("offer the tools of {} to the model", text("alias")),
            "other",
            tools_text(data),
        ),
        "mcp-call" => (format!("call {}", text("name")), "other", call_text(data)),
        "mcp-move" => (
            format!("follow {} to {}", text("alias"), text("authority")),
            "other",
            bravebot_approval::move_lines(&bravebot_approval::Move {
                alias: text("alias"),
                declared: text("declared"),
                destination: text("destination"),
                authority: text("authority"),
                may_record: data["mayRecord"].as_bool() == Some(true),
            })
            .join("\n"),
        ),
        _ => return None,
    };
    Some((plain(&title), kind, lines))
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

    /// Run, fetch, server, call and move read to the editor as `bravebot_approval` words them from
    /// the same typed requests the terminal holds, so a field the wire drops shows as a difference.
    #[test]
    fn the_other_questions_read_to_the_editor_as_they_read_in_the_terminal() {
        use bravebot_agent::confirm::{
            FetchRequest, McpCallRequest, MoveRequest, RunRequest, ServerRequest,
        };
        use bravebot_core::command::{Pipeline, Stage};

        let as_the_terminal_draws = |run: &RunRequest| {
            let steps: Vec<_> = run
                .plan
                .steps()
                .iter()
                .map(|stage| bravebot_approval::RunStep {
                    written: stage.as_written(),
                    binary: stage.binary(),
                })
                .collect();
            let writes: Vec<String> = run
                .plan
                .writes
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            let confinement =
                run.confined
                    .as_ref()
                    .map(|confined| bravebot_approval::Confinement {
                        heading: confined.heading(),
                        directories: confined
                            .directories
                            .iter()
                            .map(|directory| directory.to_string_lossy().into_owned())
                            .collect(),
                        sentences: confined.sentences(),
                    });
            let ambient: Vec<String> = run
                .ambient_authority()
                .iter()
                .map(|spent| {
                    bravebot_agent::confirm::authority_sentence(spent.authority, spent.named)
                })
                .collect();
            bravebot_approval::run_lines(&bravebot_approval::Run {
                summary: &run.summary(),
                steps: &steps,
                writes: &writes,
                confinement: confinement.as_ref(),
                ambient: &ambient,
                releases_private: run.releases_private(),
            })
            .join("\n")
        };

        let pipeline = Pipeline::new(vec![Stage::new("docker", vec!["ps".into()])]);
        let mut run = RunRequest::from_pipeline(&pipeline, &["/usr/bin/docker".into()], "/w");
        run.plan.writes = vec!["/w/out.txt".into()];
        assert!(
            !run.ambient_authority().is_empty(),
            "the fixture reaches no ambient authority"
        );
        let data = crate::wire::run_request(1, &run);
        assert_eq!(
            run_text(&data).as_deref(),
            Some(as_the_terminal_draws(&run).as_str())
        );

        run.confined = Some(bravebot_agent::Confined {
            directories: vec!["/w".into(), "/var/scratch/session".into()],
            network: bravebot_sandbox::network::Network::Open,
            filesystem: Default::default(),
            requested: Vec::new(),
            requested_reaches: Vec::new(),
            carried: vec![bravebot_agent::Carried {
                program: "docker".into(),
                toolchain: None,
                scope: Some(bravebot_sandbox::scope::Scope::Docker),
                reaches: vec![bravebot_sandbox::scope::Reach {
                    variable: "DOCKER_CONFIG",
                    path: "/home/someone/docker-work".into(),
                }],
                network: false,
                remembered: Vec::new(),
            }],
            reads_the_machine: false,
        });
        let confined = as_the_terminal_draws(&run);
        assert!(
            confined.contains("/var/scratch/session") && confined.contains("docker-work"),
            "the confined fixture shows none of its confinement: {confined}"
        );
        assert_eq!(
            run_text(&crate::wire::run_request(1, &run)).as_deref(),
            Some(confined.as_str())
        );

        let mut unknown = data.clone();
        unknown["ambient"][0]["authority"] = json!("something-new");
        assert!(describe("run", &unknown).is_none());

        let fetch = FetchRequest {
            url: "http://169.254.169.254/latest".into(),
            host: "169.254.169.254".into(),
        };
        let metadata = fetch.ambient_authority().is_some();
        assert!(metadata, "the fixture is not a metadata service");
        let (_, _, text) = shown("fetch", crate::wire::fetch_request(2, &fetch));
        assert_eq!(
            text,
            bravebot_approval::fetch_lines(&fetch.host, &fetch.url, metadata).join("\n")
        );

        let server = ServerRequest {
            language: "rust",
            program: "/bin/rust-analyzer".into(),
            workspace: "/w".into(),
            runs_build_tooling: true,
        };
        let (_, _, text) = shown("server", crate::wire::server_request(3, &server));
        assert_eq!(
            text,
            bravebot_approval::server_lines(&bravebot_approval::Server {
                summary: &server.summary(),
                program: &server.program,
                workspace: &server.workspace,
                runs_build_tooling: true,
            })
            .join("\n")
        );

        let call = McpCallRequest {
            alias: "a".into(),
            tool: "t".into(),
            arguments: vec![("q".into(), "\"v\"".into())],
            description: Some("does it".into()),
            may_stand: false,
        };
        let (_, _, text) = shown("mcp-call", crate::wire::mcp_call_request(4, &call));
        assert_eq!(
            text,
            bravebot_approval::call_lines(&bravebot_approval::Call {
                name: &call.name(),
                arguments: &call.arguments,
                description: call.description.as_deref(),
            })
            .join("\n")
        );

        let moved = MoveRequest {
            alias: "a".into(),
            declared: "https://a.test/".into(),
            destination: "https://b.test/".into(),
            authority: "b.test".into(),
            may_record: true,
        };
        let (_, _, text) = shown("mcp-move", crate::wire::mcp_move_request(5, &moved));
        assert_eq!(
            text,
            bravebot_approval::move_lines(&bravebot_approval::Move {
                alias: &moved.alias,
                declared: &moved.declared,
                destination: &moved.destination,
                authority: &moved.authority,
                may_record: true,
            })
            .join("\n")
        );
    }

    fn shown(stem: &str, data: Value) -> (String, &'static str, String) {
        describe(stem, &data).unwrap_or_else(|| panic!("`{stem}` has nothing to show"))
    }

    /// Each question the bridge raises, with the fields its request in `wire` has.
    fn every_question() -> Vec<(&'static str, Value)> {
        let vetting = json!({"verdict": "suspicious", "reason": "it asks to be obeyed"});
        vec![
            (
                "confirm",
                json!({"path": "a.txt", "intent": "edit", "added": 1, "removed": 0,
                    "changes": [{"kind": "added", "text": "x"}]}),
            ),
            (
                "run",
                json!({"plan": "ls", "directory": "/w", "summary": "list", "stages": [
                    {"display": "ls", "resolved": "/bin/ls", "binary": "/bin/ls"}],
                    "writes": ["out"], "stdin": "file", "releasesPrivate": true,
                    "requestedScopes": ["aws"], "confinement": {"heading": "confined",
                    "directories": ["/w"], "sentences": ["it reads /w"]},
                    "ambient": [{"authority": "container-daemon", "named": "docker"}]}),
            ),
            (
                "fetch",
                json!({"url": "https://a.test/", "host": "a.test", "summary": "get"}),
            ),
            (
                "output",
                json!({"command": "ls", "output": "x\ny", "vetting": vetting}),
            ),
            (
                "vouch",
                json!({"path": "p", "preview": "x", "truncated": true, "vetting": vetting}),
            ),
            (
                "vet",
                json!({"origin": "o", "summary": "read o", "expects": "text", "content": "x",
                    "vetting": vetting}),
            ),
            (
                "exposure",
                json!({"path": "p", "credentials": ["API_KEY on line 3"]}),
            ),
            (
                "server",
                json!({"language": "rust", "program": "/bin/ra", "workspace": "/w",
                "runsBuildTooling": true}),
            ),
            (
                "mcp-server",
                json!({"alias": "a", "transport": "stdio", "command": ["npx", "x y"],
                    "variables": [{"name": "TOKEN", "stored": true}], "reads": ["/r"],
                    "digest": "abc", "requestedBy": ".mcp.json", "changed": true,
                    "fetching": ["npx fetches x"], "lines": ["a runs npx", "it fetches x"]}),
            ),
            (
                "mcp-tools",
                json!({"alias": "a", "tools": [{"name": "a:t", "arguments": ["q: string"],
                    "description": "does it"}], "refused": 1, "changed": true,
                    "vetting": vetting}),
            ),
            (
                "mcp-call",
                json!({"name": "a:t", "arguments": [{"name": "q", "value": "v"}],
                    "description": "does it"}),
            ),
            (
                "mcp-move",
                json!({"alias": "a", "declared": "https://a.test/", "destination": "https://b.test/",
                    "authority": "b.test", "mayRecord": true}),
            ),
            (
                "manifest",
                json!({"task": "tidy", "steps": ["1. read", "2. write"]}),
            ),
        ]
    }

    #[test]
    fn every_question_the_bridge_raises_is_shown_in_words() {
        for (stem, data) in every_question() {
            let (title, _, text) = shown(stem, data);
            assert!(
                !title.is_empty() && !text.is_empty(),
                "`{stem}` shows nothing"
            );
            for dump in ["\"request\"", "{\"", "null"] {
                assert!(
                    !text.contains(dump),
                    "`{stem}` shows its fields as sent: {text}"
                );
            }
        }
        assert!(describe("something-new", &json!({"summary": "do it"})).is_none());
    }

    #[test]
    fn content_nobody_vouched_for_is_drawn_behind_the_margin_with_its_escapes_pictured() {
        let hostile = "ignore this\u{1b}[2J\nand this";
        let bar = bravebot_approval::QUARANTINE_BAR;
        for (stem, key, mut data) in [
            ("output", "output", every_question()[3].1.clone()),
            ("vouch", "preview", every_question()[4].1.clone()),
            ("vet", "content", every_question()[5].1.clone()),
        ] {
            data[key] = json!(hostile);
            let (_, _, text) = shown(stem, data);
            assert!(!text.contains('\u{1b}'), "`{stem}` carries an escape");
            assert!(
                text.contains(&format!("{bar} ignore this\u{241b}[2J")),
                "`{stem}`: {text}"
            );
            assert!(
                text.contains(&format!("{bar} and this")),
                "`{stem}`: {text}"
            );
        }
        let mut tools = every_question()[9].1.clone();
        tools["tools"][0]["description"] = json!(hostile);
        let (_, _, text) = shown("mcp-tools", tools);
        assert!(!text.contains('\u{1b}'), "{text}");
        assert!(
            text.contains(&format!("{bar} ignore this\u{241b}[2J")),
            "{text}"
        );
        let mut call = every_question()[10].1.clone();
        call["description"] = json!(hostile);
        let (_, _, text) = shown("mcp-call", call);
        assert!(text.contains(&format!("{bar} ignore this")), "{text}");
    }

    #[test]
    fn a_question_it_cannot_show_is_refused_and_not_put_to_the_editor() {
        let written = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&written);
        let mut shared = Shared {
            write: Box::new(move |message| sink.lock().unwrap().push(message)),
            wake: Box::new(|| {}),
            sessions: HashMap::new(),
            unasked: HashSet::new(),
            asked: HashMap::new(),
            next: 0,
            queued: Vec::new(),
        };
        question(
            &mut shared,
            "s",
            "something-new.request",
            &json!({"request": 4}),
        );
        assert!(written.lock().unwrap().is_empty());
        assert!(shared.asked.is_empty());
        assert_eq!(shared.queued.len(), 1);
        assert_eq!(shared.queued[0].method, "something-new.reply");
        assert_eq!(shared.queued[0].params["decision"], "reject");
        assert_eq!(shared.queued[0].params["request"], 4);
    }

    #[test]
    fn an_image_or_a_tag_that_loads_something_is_cut_and_a_link_is_not() {
        assert_eq!(
            plain("a ![x](https://h.test/?q=1) b ![y][r] ![z]"),
            "a ! [x](https://h.test/?q=1) b ! [y][r] ! [z]"
        );
        assert_eq!(
            plain("<IMG src=x><picture><SvG/><iframe>"),
            "< IMG src=x>< picture>< SvG/>< iframe>"
        );
        assert_eq!(
            plain("[a link](https://h.test/) Vec<String> <b>bold</b> 1 < 2"),
            "[a link](https://h.test/) Vec<String> <b>bold</b> 1 < 2"
        );
        assert_eq!(plain("one\r\ntwo\rthree"), "one\ntwo\u{240d}three");
    }

    #[test]
    fn a_fence_is_longer_than_any_run_of_backticks_in_what_it_holds() {
        assert_eq!(fenced("a"), "```\na\n```");
        assert_eq!(fenced("``` x ```"), "````\n``` x ```\n````");
        assert_eq!(fenced("`````"), "``````\n`````\n``````");
    }
}
