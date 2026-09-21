//! The library's surface: what a front-end can ask for, and what it gets told.
//!
//! One method per protocol request, plus a callback events leave through. Nothing here
//! writes to stdout, reads stdin, or ends the process; a transport does that, and there
//! can be more than one.
//!
//! # Handles
//!
//! A session's id is unique only within its project directory, so calls do not carry
//! `(directory, id)` pairs. Opening one mints a short handle that stands for the pair for
//! as long as this process lives. Handles are not written down and a front-end must not
//! store one: `session.list` returns the durable `(directory, id)`, and opening converts
//! that into a handle again.

use crate::emit::{Emitter, Listener};
use crate::protocol::{ErrorCode, Event, Failure, Request};
use crate::running::{Running, State};
use crate::turn::{BridgeConfirmer, BridgeReporter, BridgeSink, Reply};
use crate::{store, wire};
use bravebot_agent::Workspace;
use bravebot_agent::turn::{self as agent_turn, Task, TurnError};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::trust::TrustStore;
use bravebot_net::Egress;
use bravebot_session::sessions::{Handle, Record, Standing};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

/// A session this process has open.
struct Open {
    project: PathBuf,
    /// What the session carries between turns. Behind a mutex because a worker takes it
    /// for the length of a turn.
    state: Arc<Mutex<State>>,
    /// Whether the user has answered the trust question for this session.
    ///
    /// `None` means unanswered, and a turn is refused until it is. Not a bool with a
    /// default: defaulting either way answers on behalf of somebody who was never asked,
    /// which is the mistake the whole trust design exists to avoid.
    answered_trust: bool,
    /// The turn in flight, if there is one.
    running: Option<Running>,
    model: Option<String>,
    watches: Arc<Mutex<bravebot_agent::watch::Watches>>,
}

/// Drives the agent for a front-end.
pub struct Bridge {
    open: HashMap<String, Open>,
    next_handle: u64,
    settings: Option<PathBuf>,
    emitter: Emitter,
}

impl Bridge {
    pub fn new(emit: Listener) -> Self {
        Self {
            open: HashMap::new(),
            next_handle: 0,
            settings: None,
            emitter: Emitter::new(emit),
        }
    }

    /// Preserve the selected path even if it disappeared: future requests must fail rather
    /// than silently fall back to another destination after a process restart.
    pub fn with_settings(mut self, path: Option<PathBuf>) -> Self {
        self.settings = path;
        self
    }

    /// Announce what this is, before anything is asked.
    pub fn ready(&mut self) {
        let info = self.info();
        self.emitter.send(Event::global("agent.ready", info));
    }

    /// Route one request.
    ///
    /// The match is exhaustive over the methods this version knows; an unknown one is a
    /// `bad_request` rather than a panic, since a newer front-end against an older bridge
    /// is a situation that will happen and should degrade rather than crash.
    pub fn dispatch(&mut self, request: &Request) -> Result<Value, Failure> {
        match request.method.as_str() {
            "agent.info" => {
                let mut info = self.info();
                if let Some(handle) = request.optional_string("session") {
                    let open = self.open.get(&handle).ok_or_else(Failure::no_such_session)?;
                    let config = crate::settings::config(Some(&open.project), self.settings.as_deref());
                    info["configured"] = json!(config.is_ok());
                    info["defaultModel"] = json!(config.ok().map(|c| c.default_model));
                }
                Ok(info)
            },
            "models.list" => {
                let directory = request.optional_string("directory").map(PathBuf::from);
                let config = crate::settings::config(directory.as_deref(), self.settings.as_deref())?;
                Ok(crate::models::list(&config))
            },
            "session.list" => self.list(request),
            "session.open" => self.open_session(request),
            "session.new" => self.new_session(request),
            "session.fork" => self.fork_session(request),
            "session.close" => self.close_session(request),
            "turn.send" => self.send_turn(request),
            "turn.cancel" => self.cancel_turn(request),
            "watches.list" | "watches.add" | "watches.stop" => self.watches(request),
            "watches.poll" => { self.poll_watches(); Ok(json!({})) },
            "confirm.reply" => self.reply_confirm(request),
            "run.reply" => self.reply_run(request),
            "output.reply" => self.reply_output(request),
            "vouch.reply" => self.reply_vouch(request),
            "vet.reply" => self.deliver(request, Reply::Vet(wire::decision(request.param("decision")))),
            "ask.reply" => self.reply_ask(request),
            "trust.reply" => self.reply_trust(request),
            "permissions.list" => self.permissions(request, false),
            "permissions.revoke" => self.permissions(request, true),
            "settings.inspect" => {
                let project = request.optional_string("session").and_then(|id| self.open.get(&id)).map(|s| s.project.as_path());
                Ok(crate::settings::report(project, self.settings.as_deref()))
            },
            "settings.select" => {
                let path = request.optional_string("path").map(PathBuf::from);
                if let Some(path) = &path { crate::settings::validate(path)?; }
                self.settings = path;
                Ok(crate::settings::report(None, self.settings.as_deref()))
            },
            "doctor" => Ok(json!({"found": true, "structured": true, "text": serde_json::to_string_pretty(&crate::settings::report(None, self.settings.as_deref())).unwrap_or_default()})),
            other => Err(Failure::bad_request(format!("unknown method `{other}`"))),
        }
    }

    /// Which build of the agent is behind this.
    ///
    /// Worth surfacing in the interface: a transcript is read after the fact, usually
    /// because something in it went wrong, and the first question is which code produced
    /// it.
    fn info(&self) -> Value {
        json!({
            "build": crate::agent_build(),
            "version": env!("CARGO_PKG_VERSION"),
            "defaultModel": crate::settings::config(None, self.settings.as_deref()).ok().map(|config| config.default_model),
            "configured": crate::settings::config(None, self.settings.as_deref()).is_ok(),
            "home": bravebot_session::store::directory().map(|d| d.display().to_string()),
        })
    }

    // ------------------------------------------------------------ sessions

    fn list(&mut self, request: &Request) -> Result<Value, Failure> {
        let listed = match request.optional_string("directory") {
            Some(directory) => store::list_project(&PathBuf::from(directory)),
            None => store::list_all(),
        };

        let sessions: Vec<Value> = listed
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.summary.id,
                    "directory": entry.project.display().to_string(),
                    "project": entry.project_name(),
                    "branch": entry.summary.branch,
                    "title": entry.summary.title,
                    "updated": entry.summary.updated,
                    "bytes": entry.summary.bytes,
                })
            })
            .collect();

        Ok(json!({ "sessions": sessions }))
    }

    fn open_session(&mut self, request: &Request) -> Result<Value, Failure> {
        let directory = PathBuf::from(request.string("directory")?);
        let id = request.string("id")?;

        let record = store::load(&directory, &id).ok_or_else(|| {
            Failure::new(
                ErrorCode::NoSuchSession,
                format!("no session `{id}` in {}", directory.display()),
            )
        })?;

        // A record that recorded a trust map was answered for by the person now resuming
        // it, and inherits it. One that did not is asked again: nothing recorded is not
        // the same as nothing trusted, and reading an absent map as an empty one would
        // answer on behalf of somebody who was never asked.
        let inherited = record.trust_map(&directory);
        let answered_trust = inherited.is_some();
        let state = State::resumed(&directory, &record, inherited.unwrap_or_else(|| TrustStore::new(&directory)));

        let handle = self.mint(Open {
            project: directory.clone(),
            state: Arc::new(Mutex::new(state)),
            answered_trust,
            running: None,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            model: None,
        });

        if !answered_trust {
            self.emitter.send(Event::new(
                "trust.request",
                &handle,
                json!({ "directory": directory.display().to_string() }),
            ));
        }

        Ok(self.recount(&handle, &directory, &record))
    }

    /// Everything a front-end needs to draw a session it did not watch happen.
    fn recount(&self, handle: &str, directory: &std::path::Path, record: &Record) -> Value {
        // Restored rather than read straight off the record, because restoring is what
        // adds the note saying the quarantine's references no longer name anything — and
        // `recounted` filters that note back out. Going around it would show a transcript
        // subtly unlike the one a resume produces.
        let conversation = bravebot_agent::Conversation::restored(record.conversation.clone());
        let said: Vec<Value> = conversation.recounted().iter().map(wire::said).collect();

        let todos = todos_json(&record.todo_rows());

        json!({
            "session": handle,
            "model": crate::settings::config(Some(directory), self.settings.as_deref()).ok().map(|config| config.default_model),
            "record": {
                "id": record.id,
                "directory": record.directory,
                "branch": record.branch,
                "title": record.title,
                "started": record.started,
                "updated": record.updated,
                "turns": record.turns,
                "tokens": record.tokens,
                "build": record.build,
            },
            "said": said,
            "context": record.conversation.context,
            "contextTokens": record.conversation.measured,
            // As on `turn.done`, and read straight off the record rather than off the restored
            // conversation: it is written down, so a session resumed in a new process knows what
            // compaction had already taken without having to watch it happen.
            "archived": record.conversation.archive.len(),
            "todos": todos,
            "trust": {
                "known": record.trust.is_some(),
                "rules": record.trust.as_ref().map(|rules| {
                    rules.iter().map(|rule| json!({
                        "path": rule.path,
                        "integrity": rule.integrity,
                    })).collect::<Vec<_>>()
                }),
            },
            "branchNote": bravebot_session::sessions::branch_note(
                record.branch.as_deref(),
                bravebot_session::sessions::branch_of(directory).as_deref(),
            ),
            "buildNote": bravebot_session::sessions::build_note(
                record.build.as_deref(),
                crate::agent_build(),
            ),
        })
    }

    fn new_session(&mut self, request: &Request) -> Result<Value, Failure> {
        let directory = PathBuf::from(request.string("directory")?);

        if !directory.is_dir() {
            return Err(Failure::new(
                ErrorCode::NotADirectory,
                format!("{} is not a directory", directory.display()),
            ));
        }
        if bravebot_session::store::directory().is_none() {
            return Err(Failure::new(ErrorCode::NoHome, "no home directory to store sessions in"));
        }

        let branch = bravebot_session::sessions::branch_of(&directory);
        let handle = self.mint(Open {
            project: directory.clone(),
            // An empty map until the user answers. Nothing runs before then, so this is
            // never the map a turn uses.
            state: Arc::new(Mutex::new(State::fresh(TrustStore::new(&directory)))),
            answered_trust: false,
            running: None,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            model: None,
        });

        // Nothing is written until the first turn. An opened-and-abandoned window should
        // leave no trace, which is also how `bravebot` behaves.
        self.emitter.send(Event::new(
            "trust.request",
            &handle,
            json!({ "directory": directory.display().to_string() }),
        ));

        Ok(json!({
            "session": handle,
            "model": crate::settings::config(Some(&directory), self.settings.as_deref()).ok().map(|config| config.default_model),
            "directory": directory.display().to_string(),
            "branch": branch,
        }))
    }

    /// Begin a session from part of another one.
    ///
    /// The cut is named by an ordinal over the prompts the transcript drew, and by the text of
    /// the prompt at that ordinal. Both, because they check each other: the ordinal says where,
    /// and the text says that the front-end's idea of where agrees with the conversation's. A
    /// window can count differently — the agent writes user-role messages of its own that a
    /// transcript draws but nobody typed — and a fork taken one prompt away from where somebody
    /// pointed is worse than one that did not happen.
    ///
    /// Nothing is written. The child's id is real and reserved from here, but its record appears
    /// on its first turn like any other session's, so a fork opened and abandoned leaves no
    /// trace. See `docs/phase-0-rpc-protocol.md` §7.1.
    fn fork_session(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        let ordinal = request.number("prompt")? as usize;
        let text = request.string("text")?;

        self.reap(&handle);

        let open = self.open.get(&handle).ok_or_else(Failure::no_such_session)?;
        // Refused rather than queued, and not out of tidiness: a worker holds the session's
        // state for the whole of its turn, and dispatch is one thread. A fork that waited for
        // the lock would stop this bridge answering anything — including the question the turn
        // is blocked on, which is the thing that would let it finish.
        if open.running.is_some() {
            return Err(Failure::new(
                ErrorCode::TurnInFlight,
                "a turn is running in the session being forked",
            ));
        }

        let project = open.project.clone();
        let answered_trust = open.answered_trust;

        // Everything needed is copied out under the lock and the lock is dropped before any of
        // it is used. A fork does no I/O and no thinking, but holding a session's state across
        // work is the habit that turns into a stall later.
        let (snapshot, said, trust, programs, directories, todos, parent_id, parent_title) = {
            let state = open
                .state
                .lock()
                .map_err(|_| Failure::new(ErrorCode::Internal, "session state is poisoned"))?;
            let Some(parent) = state.handle.as_ref() else {
                return Err(Failure::bad_request(
                    "this session has not been written down yet, so there is nothing to fork from",
                ));
            };
            (
                state.conversation.snapshot(),
                state.conversation.recounted(),
                state.trust.clone(),
                state.programs.clone(),
                state.directories.clone(),
                state.todos.clone(),
                parent.id().to_string(),
                parent.title().to_string(),
            )
        };

        let cut = crate::fork::cut(&snapshot, &said, ordinal).ok_or_else(|| {
            Failure::bad_request(format!("no prompt {ordinal} to fork in front of"))
        })?;
        if cut.prompt != text {
            return Err(Failure::bad_request(
                "the prompt at that position is not the one this fork names; \
                 reopen the session and fork again",
            ));
        }

        // What the child's own transcript reads as, from the same projection a resume uses, so
        // the front-end draws the fork from what the conversation says rather than from a slice
        // of what it happened to have on screen.
        let before = bravebot_agent::Conversation::restored(cut.before.clone()).recounted();
        let recounted: Vec<Value> = before.iter().map(wire::said).collect();
        // The first thing said in the history the child keeps, which is what titles it. Without
        // this a fork would be named after the prompt that replaced the one it was cut at, and a
        // list of forks would say nothing about where any of them came from.
        let first_prompt = crate::fork::prompts(&before)
            .first()
            .map(|prompt| (*prompt).to_string());

        // Cut to the same place the conversation was: a turn's plan belongs to the turn, and the
        // child has the turns before the cut and no others.
        let todos: std::collections::BTreeMap<_, _> =
            todos.into_iter().filter(|(turn, _)| *turn <= ordinal).collect();
        // The child knows its map exactly when the parent did. A parent still holding the
        // question — a record written before maps were kept — hands the question down.
        let known = answered_trust;
        let rules = rules_json(&trust);

        let begun = self.begin_unique(&project)?;
        let id = begun.id().to_string();

        let child = self.mint(Open {
            project: project.clone(),
            state: Arc::new(Mutex::new(State::forked(
                begun,
                cut.before,
                trust,
                programs,
                directories,
                ordinal,
                todos.clone(),
                first_prompt,
            ))),
            // Inherited along with the map itself: the same person, in the same directory, in
            // the same window, so asking again would be asking somebody to answer twice.
            answered_trust,
            running: None,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            model: None,
        });

        if !answered_trust {
            self.emitter.send(Event::new(
                "trust.request",
                &child,
                json!({ "directory": project.display().to_string() }),
            ));
        }

        let title = if parent_title.is_empty() {
            store::load(&project, &parent_id).map(|record| record.title)
        } else {
            Some(parent_title)
        };

        Ok(json!({
            "session": child,
            "id": id,
            "directory": project.display().to_string(),
            "branch": bravebot_session::sessions::branch_of(&project),
            "said": recounted,
            "prefill": cut.prompt,
            "context": snapshot.context,
            "contextTokens": snapshot.measured,
            "turns": ordinal,
            "todos": todos_json(&todos),
            "trust": { "known": known, "rules": if known { Value::from(rules) } else { Value::Null } },
            "parent": {
                "id": parent_id,
                "directory": project.display().to_string(),
                "title": title,
                "prompt": ordinal,
            },
        }))
    }

    fn close_session(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        let open = self.open.remove(&handle).ok_or_else(Failure::no_such_session)?;

        // Stop the work, then refuse whatever it was waiting on. Both, and in that order:
        // cancelling alone would leave a write blocked on an answer that is never coming,
        // and refusing alone would let the turn carry on past it.
        if let Some(running) = &open.running {
            running.cancel.cancel();
            running.refuse_pending();
        }
        Ok(json!({}))
    }


    // ------------------------------------------------------------ turns

    /// Start a turn, and return before it finishes.
    ///
    /// Everything the turn produces arrives as events. The response says only that it
    /// began, because a turn takes as long as a model does and a front-end that blocked
    /// on it would show nothing until it ended.
    fn send_turn(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        let prompt = request.string("prompt")?;
        let requested_model = crate::models::selection(request.params.get("model"))?;
        let files: Vec<String> = request
            .params
            .get("files")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        // The same shape as `files`, and a different promise. A named file is workspace-relative
        // and the agent reads it inside the project; a dropped one may sit anywhere, because the
        // path came from a gesture rather than from anything a model said. That is what carries a
        // bot's briefing, which lives beside this app's own settings and deliberately not inside
        // the checkout the planner may write to.
        let dropped: Vec<String> = request
            .params
            .get("dropped")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        // Whether this prompt is one a person will want back when they press up.
        //
        // `~/.bravebot/history` is recall, shared with the terminal front-end, and what belongs in
        // it is what somebody typed. A turn a front-end sends on its own account — this app asking
        // a bot to bring its memory up to date, after a compaction — is not that: putting it there
        // would mean a person scrolling their own history through boilerplate they never wrote, in
        // both front-ends, because one of them decided to do some house-keeping.
        //
        // Defaults to true, so every caller that predates this keeps the behaviour it had, and so
        // the ordinary case needs no ceremony. It also decides whether the prompt may *title* the
        // session, for the same reason and by the same argument: a name is another thing that
        // should say what a person asked for.
        let recall = request.flag("recall", true);

        self.reap(&handle);

        let open = self.open.get(&handle).ok_or_else(Failure::no_such_session)?;

        if !open.answered_trust {
            return Err(Failure::bad_request(
                "this session has not been asked whether the directory is trusted; \
                 send trust.reply first",
            ));
        }
        if open.running.is_some() {
            return Err(Failure::new(
                ErrorCode::TurnInFlight,
                "a turn is already running in this session",
            ));
        }

        let model = requested_model.or_else(|| open.model.clone());
        let config = crate::settings::config(Some(&open.project), self.settings.as_deref())?;
        let mut workspace = Workspace::new(open.project.clone())
            .map_err(|error| Failure::new(ErrorCode::Internal, error.to_string()))?;

        let project = open.project.clone();
        let state = Arc::clone(&open.state);
        let watches = Arc::clone(&open.watches);
        let (turn_number, directories) = state
            .lock()
            .map(|s| (s.turns + 1, s.directories.clone()))
            .unwrap_or((1, Vec::new()));

        // A workspace is built per turn and opens the project only, so the directories a
        // resumed session had open have to be opened again here. The rules about them came back
        // with the trust map, and a rule about a directory nothing can open refuses every path
        // under it for escaping the workspace — with nothing on screen to say why. One that has
        // since moved or been deleted cannot be reopened and is left closed: the refusal it
        // causes is the one that was already happening, and this protocol has no way to say so
        // outside a turn.
        for directory in &directories {
            let _ = workspace.add_directory(&directory.display().to_string());
        }

        // A fresh token and a fresh channel per turn. Reusing either could cancel a turn
        // before it started, or deliver yesterday's answer to today's question.
        let cancel = Cancel::new();
        let (answers_tx, answers_rx) = mpsc::channel();
        let pending: crate::turn::Pending = Arc::new(Mutex::new(None));

        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let running = Running {
            cancel: cancel.clone(),
            answers: answers_tx,
            pending: Arc::clone(&pending),
            turn: turn_number,
            finished: Arc::clone(&finished),
        };

        let emitter = self.emitter.clone();
        let session = handle.clone();

        let worker_model = model.clone();

        self.emitter.send(Event::new(
            "turn.started",
            &handle,
            json!({ "turn": turn_number }),
        ));

        if let Some(open) = self.open.get_mut(&handle) {
            open.running = Some(running);
            open.model = model;
        }
        thread::spawn(move || {
            work(Work {
                model: worker_model,
                watches,
                emitter,
                session,
                project,
                state,
                config,
                workspace,
                prompt,
                files,
                dropped,
                recall,
                turn: turn_number,
                cancel,
                pending,
                answers: answers_rx,
                finished,
            });
        });


        Ok(json!({ "turn": turn_number }))
    }

    /// Ask the turn to stop.
    ///
    /// Returns at once; the turn ends when the engine next looks at the token. A pending
    /// question wakes through the confirmer’s cancellation check and resolves to refusal.
    /// Cancellation never sends an approval or authorises a write.
    fn cancel_turn(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        let open = self.open.get(&handle).ok_or_else(Failure::no_such_session)?;
        if let Some(running) = &open.running {
            running.cancel.cancel();
        }
        if let Ok(mut watches) = open.watches.lock() { watches.stop_firing(); }
        // Cancelling when nothing is running is not an error: the turn may have finished
        // between the user pressing the key and this arriving.
        Ok(json!({}))
    }

    /// Carry an answer back to the write that is waiting for it.
    fn reply_confirm(&mut self, request: &Request) -> Result<Value, Failure> {
        let reply = Reply::Write(wire::decision(request.param("decision")));
        self.deliver(request, reply)
    }

    /// Answer a run, which is the one question with two answers.
    fn reply_run(&mut self, request: &Request) -> Result<Value, Failure> {
        let reply = Reply::Run(wire::run_decision(
            request.param("decision"),
            request.param("remember"),
        ));
        self.deliver(request, reply)
    }

    /// Answer whether the planner may read a command's output.
    fn reply_output(&mut self, request: &Request) -> Result<Value, Failure> {
        let reply = Reply::Output(wire::decision(request.param("decision")));
        self.deliver(request, reply)
    }

    /// Answer whether to vouch for a quarantined path.
    fn reply_vouch(&mut self, request: &Request) -> Result<Value, Failure> {
        let reply = Reply::Vouch(wire::decision(request.param("decision")));
        self.deliver(request, reply)
    }

    /// Answer a series of questions, one answer per question.
    fn reply_ask(&mut self, request: &Request) -> Result<Value, Failure> {
        let reply = Reply::Ask(wire::answers(request.param("answers")));
        self.deliver(request, reply)
    }

    /// Carry one answer to the turn that is waiting for it.
    ///
    /// Shared by every reply kind, because everything after "which question is this" is identical
    /// and the differences are all in the reading of the answer, above. Note what is *not*
    /// here: no check that the front-end sent the kind of reply matching what is
    /// outstanding. That is [`Running::answer`]'s job, and it is left there so there is one
    /// place where an id and a kind are compared against the question that was asked.
    fn deliver(&mut self, request: &Request, reply: Reply) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        let id = request.number("request")?;

        let open = self.open.get(&handle).ok_or_else(Failure::no_such_session)?;
        let Some(running) = &open.running else {
            return Err(Failure::new(
                ErrorCode::NoSuchRequest,
                "no turn is running in this session",
            ));
        };

        if running.answer(id, reply) {
            Ok(json!({}))
        } else {
            // Unknown, or already used. An approval is single-use and bound to the one
            // write it was shown for, so this changes nothing rather than being retried.
            Err(Failure::new(
                ErrorCode::NoSuchRequest,
                format!("request {id} is not waiting for an answer"),
            ))
        }
    }

    /// Record what the user answered about trusting the directory.
    fn reply_trust(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        let trusted = request
            .params
            .get("trusted")
            .and_then(Value::as_bool)
            .ok_or_else(|| Failure::bad_request("`trusted` must be a boolean"))?;

        let open = self.open.get_mut(&handle).ok_or_else(Failure::no_such_session)?;

        // Trusting records the workspace root, which covers everything beneath it.
        // Declining records nothing, leaving a map in which no path is trusted. The same
        // two outcomes the terminal offers, so an answer means the same in both.
        let mut trust = TrustStore::new(&open.project);
        if trusted {
            trust.trust(".");
        }
        if let Ok(mut state) = open.state.lock() {
            state.trust = trust;
        }
        open.answered_trust = true;

        Ok(json!({ "trusted": trusted }))
    }

    /// Inspect or reduce existing grants. No operation on this channel can add trust.
    fn permissions(&mut self, request: &Request, revoke: bool) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        self.reap(&handle);
        let open = self.open.get(&handle).ok_or_else(Failure::no_such_session)?;
        if open.running.is_some() {
            return Err(Failure::new(ErrorCode::TurnInFlight, "Stop the current turn before reviewing or revoking permissions."));
        }
        let mut state = open.state.lock().map_err(|_| Failure::new(ErrorCode::Internal, "Session state unavailable"))?;
        if revoke {
            match request.string("kind")?.as_str() {
                "path" => {
                    let path = request.string("path")?;
                    if !state.trust.rules().any(|(held, _)| held == path) {
                        return Err(Failure::bad_request("This path grant is no longer present."));
                    }
                    state.trust.distrust(&path);
                }
                "command" => {
                    let selected = request.params.get("command").ok_or_else(|| Failure::bad_request("Missing command"))?;
                    let command = state.programs.iter().find(|command| json!({"program": command.program, "args": command.args}) == *selected).cloned()
                        .ok_or_else(|| Failure::bad_request("This command grant is no longer present."))?;
                    state.programs.forget(&command);
                }
                _ => return Err(Failure::bad_request("Unknown permission kind")),
            }
            if state.handle.is_some() {
                let turn = state.turns;
                save(&open.project, &mut state, turn, &bravebot_session::audit::Trail::default());
            }
        }
        Ok(json!({
            "paths": rules_json(&state.trust),
            "commands": state.programs.iter().map(|command| json!({"program": command.program, "args": command.args, "display": command.display()})).collect::<Vec<_>>()
        }))
    }

    fn watches(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        self.reap(&handle);
        let open = self.open.get(&handle).ok_or_else(Failure::no_such_session)?;
        let mut watches = open.watches.lock().map_err(|_| Failure::bad_request("Watches unavailable."))?;
        if request.method == "watches.add" {
            if !open.answered_trust { return Err(Failure::bad_request("Answer the project trust question first.")); }
            if open.running.is_some() { return Err(Failure::new(ErrorCode::TurnInFlight, "Wait for the current turn before adding a watch.")); }
            let path = request.string("path")?;
            if path.is_empty() || path.chars().any(char::is_control) { return Err(Failure::bad_request("Choose a project file.")); }
            if !watches.live().iter().any(|w| w.path() == path) {
                let workspace = Workspace::new(open.project.clone()).map_err(|_| Failure::bad_request("Project unavailable."))?;
                watches.arm(path.clone(), 0, workspace.look(&path), std::time::Instant::now())
                    .map_err(|_| Failure::bad_request("Cannot watch this file. It must exist inside the project, with fewer than eight active watches."))?;
            }
        } else if request.method == "watches.stop" {
            if request.flag("all", false) { watches.stop_all(); }
            else { let number = request.param("number").as_u64().ok_or_else(|| Failure::bad_request("A watch number is required."))?; watches.stop(number as usize); }
        }
        let now = std::time::Instant::now();
        Ok(json!({"watches": watches.live().iter().map(|w| json!({"number": w.number(), "path": w.path(),
            "remainingSeconds": w.left(now).as_secs(), "armedBy": w.armed_by(),
            "state": if watches.firing() == Some(w.number()) { "running" } else { "watching" }
        })).collect::<Vec<_>>(), "busy": open.running.is_some()}))
    }

    /// Called by the desktop's clock, never a renderer-supplied generated prompt.
    fn poll_watches(&mut self) { self.poll_watches_at(std::time::Instant::now()); }

    fn poll_watches_at(&mut self, now: std::time::Instant) {
        let handles: Vec<_> = self.open.keys().cloned().collect();
        for handle in handles {
            self.reap(&handle);
            let Some(open) = self.open.get(&handle) else { continue };
            if open.running.is_some() || !open.answered_trust { continue; }
            if open.watches.lock().map(|w| w.is_empty()).unwrap_or(true) { continue; }
            let Ok(mut workspace) = Workspace::new(open.project.clone()) else {
                if let Ok(mut watches) = open.watches.lock() {
                    for watch in watches.live() { self.emitter.send(Event::new("watch.ended", &handle, json!({"number": watch.number(), "reason": "project-unavailable"}))); }
                    watches.stop_all();
                }
                continue;
            };
            if let Ok(state) = open.state.try_lock() { for directory in &state.directories { let _ = workspace.add_directory(&directory.display().to_string()); } }
            let due = {
                let Ok(mut watches) = open.watches.lock() else { continue };
                let ended = watches.look(now, |path| workspace.look(path));
                for (number, reason) in ended {
                    self.emitter.send(Event::new("watch.ended", &handle, json!({"number": number, "reason": match reason {
                        bravebot_agent::watch::Reaped::Aged => "expired", bravebot_agent::watch::Reaped::OutOfReach => "out-of-reach",
                    }})));
                }
                watches.due(now).map(|w| (w.number(), w.path().to_string()))
            };
            let Some((number, path)) = due else { continue };
            let prompt = bravebot_agent::watch::fired(number, &path);
            // Announce the cause before starting a worker, so early events follow it.
            self.emitter.send(Event::new("watch.fired", &handle, json!({"number": number, "path": path})));
            if let Ok(mut watches) = open.watches.lock() { watches.dispatched(number); }
            let request = Request::parse(&json!({"id": 0, "method": "turn.send", "params": {"session": handle, "prompt": prompt, "recall": false}}).to_string());
            if let Ok(request) = request && let Err(error) = self.send_turn(&request) {
                if let Some(open) = self.open.get(&handle) && let Ok(mut watches) = open.watches.lock() { watches.stop(number); }
                self.emitter.send(Event::new("watch.ended", &handle, json!({"number": number, "reason": "failed", "message": error.message})));
            }
        }
    }

    /// Forget a turn that has already finished.
    ///
    /// The worker owns the end of a turn and does not report back, so the dispatch thread
    /// notices lazily, from a flag the worker sets on its way out. It must not notice by
    /// probing the answer channel: anything sent down that to test whether it is still
    /// connected is a real decision arriving at a real write.
    fn reap(&mut self, handle: &str) {
        if let Some(open) = self.open.get_mut(handle)
            && open.running.as_ref().is_some_and(Running::is_finished)
        {
            open.running = None;
        }
    }

    // ------------------------------------------------------------ plumbing

    /// A handle for a new session whose id nothing else is using.
    ///
    /// The agent's ids are the second plus the process id, so two sessions begun in the same
    /// second in the same process *are* the same session as far as the store is concerned — the
    /// one saved second would overwrite the first. Resuming cannot reach that and starting
    /// sessions by hand barely can, since both need a turn's worth of time in between. Forking
    /// can: a fork is written down the moment it is asked for, and a fork of a fork is two ids
    /// minted by two clicks.
    ///
    /// A second is the whole resolution of the collision, so waiting one out is the whole fix.
    /// Taken means either a record already on disk or an id another open session is holding —
    /// the second matters because a fork nobody has spoken to yet has an id and no record, and
    /// that is exactly the case this exists for.
    fn begin_unique(&self, project: &std::path::Path) -> Result<Handle, Failure> {
        for attempt in 0..6 {
            if attempt > 0 {
                thread::sleep(std::time::Duration::from_millis(250));
            }
            let handle = Handle::begin(project, crate::agent_build());
            if !self.id_taken(project, handle.id()) {
                return Ok(handle);
            }
        }
        Err(Failure::new(
            ErrorCode::Internal,
            "could not find an unused session id for the fork",
        ))
    }

    fn id_taken(&self, project: &std::path::Path, id: &str) -> bool {
        if store::load(project, id).is_some() {
            return true;
        }
        self.open.values().any(|open| {
            open.project == project
                && open
                    .state
                    .lock()
                    .ok()
                    .and_then(|state| state.handle.as_ref().map(|held| held.id() == id))
                    .unwrap_or(false)
        })
    }

    fn mint(&mut self, session: Open) -> String {
        self.next_handle += 1;
        let handle = format!("s{}", self.next_handle);
        self.open.insert(handle.clone(), session);
        handle
    }
}

/// Everything a worker needs to run one turn.
///
/// A struct rather than a dozen arguments, because the list was the kind that grows one
/// parameter at a time until nobody can read the call.
struct Work {
    emitter: Emitter,
    session: String,
    project: PathBuf,
    state: Arc<Mutex<State>>,
    config: Config,
    watches: Arc<Mutex<bravebot_agent::watch::Watches>>,
    model: Option<String>,
    workspace: Workspace,
    prompt: String,
    files: Vec<String>,
    dropped: Vec<String>,
    /// Whether this prompt joins the shared recall history, and may name the session.
    recall: bool,
    turn: usize,
    cancel: Cancel,
    pending: crate::turn::Pending,
    answers: mpsc::Receiver<crate::turn::Reply>,
    finished: Arc<std::sync::atomic::AtomicBool>,
}

/// Run one turn to its end, whatever that end is.
///
/// The worker owns the whole of it: the call, writing the record afterwards, and saying
/// what happened. A turn that fails is still part of the conversation and is still
/// written down — the next question is usually about it.
fn work(work: Work) {
    let Work {
        emitter,
        session,
        project,
        state,
        config,
        watches,
        model,
        workspace,
        prompt,
        files,
        dropped,
        recall,
        turn,
        cancel,
        pending,
        answers,
        finished,
    } = work;

    // Held for the length of the turn. Nothing else contends for it: a session with a
    // turn in flight refuses another one.
    let Ok(mut state) = state.lock() else {
        finished.store(true, std::sync::atomic::Ordering::Release);
        return;
    };

    let history = bravebot_session::store::Entry::sent(
        &prompt,
        Some(project.display().to_string()),
    );
    let mut task = Task::new(&prompt).with_home(bravebot_agent::home::directory()).with_model(model);
    for file in &files {
        task = task.with_file(file);
    }
    for path in &dropped {
        task = task.with_dropped_text(path);
    }

    let free = watches.lock().map(|w| bravebot_agent::watch::MAX_LIVE.saturating_sub(w.live().len())).unwrap_or(0);
    task = task.arming(if free == 0 { bravebot_agent::watch::Arming::Full } else { bravebot_agent::watch::Arming::Allowed { free } });
    let mut reporter = BridgeReporter::new(emitter.clone(), &session);
    let mut confirmer = BridgeConfirmer::new(emitter.clone(), &session, pending, answers, cancel.clone());
    let mut sink = BridgeSink::new(emitter.clone(), &session, turn);
    let egress = Egress::new();

    // Cloned out before the call, because the conversation is borrowed mutably for the
    // duration and both of these are passed by value.
    let trust = state.trust.clone();
    let programs = state.programs.clone();
    let outcome = agent_turn::resume(
        &config,
        &egress,
        &workspace,
        &task,
        &mut state.conversation,
        &mut confirmer,
        &mut reporter,
        &mut sink,
        trust,
        programs,
        None, // Language-server approvals are not offered by this front-end.
        &cancel,
    );

    // The prompt joins the history the terminal also reads, so recall works across both
    // front-ends. Best-effort by design upstream, and nothing here depends on it.
    //
    // Unless the front-end said this was not a prompt a person typed. See `recall` where it is
    // parsed: the same flag holds back the session's name, because a conversation called after
    // some house-keeping would be a conversation named for the one thing nobody in it asked.
    if recall {
        bravebot_session::store::append_history(&history);
    }

    state.turns = turn;
    if state.first_prompt.is_none() && recall {
        state.first_prompt = Some(prompt.clone());
    }

    match outcome {
        Ok(outcome) => {
            if let Ok(mut watches) = watches.lock() {
                for path in &outcome.watches {
                    if !watches.live().iter().any(|w| w.path() == path) {
                        let _ = watches.arm(path.clone(), turn, workspace.look(path), std::time::Instant::now());
                    }
                }
            }
            // The map after the turn, which may differ from the one it started with: a
            // turn that writes untrusted data into a trusted path records that path as
            // untrusted, and the next turn must inherit that or it would read the data
            // back as trusted.
            state.trust = outcome.trust.clone();
            // Taken from the outcome rather than from whatever asked, so there is one copy
            // of the answer. Nothing is added while this front-end refuses every vouch, but
            // a set that came back smaller than it went in would be a lost permission.
            state.programs = outcome.programs.clone();
            state.tokens += outcome.tokens;
            // Added to rather than set: a turn that compacted part way through has already put
            // that cost here under the same number, and the breakdown has to add up to the total.
            *state.spend.entry(turn).or_insert(0) += outcome.tokens;
            state.timing.entry(turn).or_default().add(outcome.timing);
            // Left as it was when a turn never reached a server, so a record keeps the last model
            // that actually answered rather than forgetting it to a turn that failed early.
            if !outcome.model.is_empty() {
                state.model = Some(outcome.model.clone());
            }

            let archived = save(&project, &mut state, turn, sink.trail());

            let rules = rules_json(&state.trust);

            emitter.send(Event::new(
                "turn.done",
                &session,
                json!({
                    "turn": turn,
                    // The released reply, authorised inside the turn while the policy was
                    // still open. Never `outcome.reply`, which is the labelled value.
                    "reply": outcome.reply_for_display(),
                    "model": outcome.model,
                    "steps": outcome.steps,
                    "clean": outcome.clean,
                    "tokens": outcome.tokens,
                    "outputTokens": outcome.output_tokens,
                    "contextTokens": outcome.context_tokens,
                    "notices": outcome.notices,
                    "trust": { "rules": rules },
                    // The session's durable name, which is real from here and was not before:
                    // `save` above is what wrote the record, and until a record exists there is
                    // nothing for an id to point at. A front-end keeping its own note about a
                    // session — which is the only way to keep one, the agent's record having no
                    // field for anybody else's — learns it here rather than by guessing which
                    // row in the list is the one it just made.
                    "id": state.handle.as_ref().map(|handle| handle.id()),
                    // How many messages compaction has taken out of this conversation, in total.
                    // It only ever rises, and it rises exactly when the conversation stopped
                    // carrying what was said before the summary — which is the moment anything
                    // standing at the top of a session has to be said again. Reported rather than
                    // inferred from the `compacting` phase, which is emitted before compaction is
                    // attempted and so also fires when there was nothing worth compacting.
                    "archived": archived,
                }),
            ));
        }
        Err(error) => {
            let _ = save(&project, &mut state, turn, sink.trail());

            let ending = error.ending();
            let diagnosis = ending.diagnosis();
            let category = diagnosis.map(|d| d.category.name());
            // A configured gateway does not serve the built-in Brave default. Keep this
            // distinct from a missing token: re-entering the gateway key cannot fix routing.
            let chosen = task.model.as_deref().unwrap_or(&config.default_model);
            let category = if category == Some("unconfigured")
                && (!config.providers.is_empty() || config.bedrock.is_some())
                && !config.serves_aichat()
                && config.provider_for(chosen).is_none()
                && config.bedrock_for(chosen).is_none()
            { Some("model-unconfigured") } else { category };
            let attempts = match ending { bravebot_agent::outcome::Ending::Stopped { attempts } => attempts, _ => diagnosis.and_then(|d| d.attempts) };
            let kind = match &error {
                TurnError::Cancelled { .. } => "cancelled",
                TurnError::Precommit(_) => "precommit",
                TurnError::Workspace(_) => "workspace",
                TurnError::Chat(_) => "chat",
                // A manifest run is a plan frozen and then carried out unattended, and this
                // window has no way to ask for one: `turn.send` builds a `Task`. So this arm
                // is unreachable rather than unhandled, and it is named rather than swept into
                // a wildcard, because the day the protocol grows a manifest the compiler
                // should not stay quiet about the `attempt` this drops.
                TurnError::Manifest { .. } => "manifest",
            };
            emitter.send(Event::new(
                "turn.error",
                &session,
                json!({ "turn": turn, "kind": kind, "message": category.unwrap_or("cancelled"), "category": category, "attempts": attempts, "status": diagnosis.and_then(|d| d.status),
                    "contextTokens": state.conversation.last_request_tokens(),
                    "id": state.handle.as_ref().map(|handle| handle.id()) }),
            ));
        }
    }

    // Last, and after the record is on disk, so a front-end that reloads on being told
    // the turn ended reads the same thing this wrote.
    if let Ok(mut watches) = watches.lock() { watches.turn_ended(std::time::Instant::now()); }
    finished.store(true, std::sync::atomic::Ordering::Release);
}

/// Write the session down, in the agent's own format.
///
/// The same `Handle` the terminal uses, so a session written here is one `bravebot --resume`
/// can pick up. Created on the first turn rather than when the window opened: an
/// abandoned window should leave nothing behind.
fn save(
    project: &std::path::Path,
    state: &mut State,
    turn: usize,
    trail: &bravebot_session::audit::Trail,
) -> usize {
    let handle = state
        .handle
        .get_or_insert_with(|| Handle::begin(project, crate::agent_build()));

    let first = state.first_prompt.clone().unwrap_or_default();
    // Taken once and lent to both readers below. A snapshot copies the whole conversation, and
    // the archive count wanted for `turn.done` is a field of the one being written down anyway —
    // asking for a second copy to read one number off it would double the cost of every turn.
    let snapshot = state.conversation.snapshot();
    let archived = snapshot.archive.len();
    handle.save(
        &first,
        Standing {
            // The UI derives its transcript from the conversation, including newly run turns.
            history: None,
            conversation: &snapshot,
            turns: state.turns,
            tokens: state.tokens,
            spend: &state.spend,
            timing: &state.timing,
            model: state.model.as_deref(),
            todos: &state.todos,
            asides: &state.asides,
            rewind: &state.rewind,
            trust: &state.trust,
            programs: &state.programs,
            directories: &state.directories,
            // Every session this window writes is a turn session. `Standing` carries the
            // manifest so the picker can mark a run that may be read and not continued, and
            // marking one of ours would be a claim about a session nobody can resume.
            manifest: None,
        },
    );
    handle.append_audit(turn, trail.events());
    archived
}

/// The task lists, keyed by turn as a string, because JSON object keys are.
fn todos_json(
    todos: &std::collections::BTreeMap<usize, Vec<bravebot_core::todo::Row>>,
) -> HashMap<String, Vec<Value>> {
    todos
        .iter()
        .map(|(turn, rows)| (turn.to_string(), rows.iter().map(wire::row).collect()))
        .collect()
}

/// A trust map as a front-end reads it.
///
/// The same two words the record is written with, so a rule reads the same whether it came off
/// disk, out of a finished turn, or out of the session a fork inherited it from.
fn rules_json(trust: &TrustStore) -> Vec<Value> {
    trust
        .rules()
        .map(|(path, integrity)| {
            let integrity = match integrity {
                bravebot_core::label::Integrity::Trusted => "trusted",
                bravebot_core::label::Integrity::Untrusted => "untrusted",
            };
            json!({ "path": path, "integrity": integrity })
        })
        .collect()
}

#[cfg(test)]
mod watch_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_failed_automatic_turn_stops_its_watch_and_never_reads_file_content_into_the_prompt() {
        let mut builder = tempfile::Builder::new();
        builder.prefix("bravebot-watch-poll-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().unwrap();
        let root = directory.path().to_path_buf();
        std::fs::write(root.join("watched"), "old").unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let held = Arc::clone(&events);
        let mut bridge = Bridge::new(Box::new(move |e| held.lock().unwrap().push(e)));
        // A removed override must fail before making any model request.
        bridge.settings = Some(root.join("missing.json"));
        let now = Instant::now();
        let mut watches = bravebot_agent::watch::Watches::new();
        let workspace = Workspace::new(root.clone()).unwrap();
        watches.arm("watched".into(), 1, workspace.look("watched"), now).unwrap();
        let watches = Arc::new(Mutex::new(watches));
        let handle = bridge.mint(Open {
            project: root.clone(), state: Arc::new(Mutex::new(State::fresh(TrustStore::new(&root)))),
            answered_trust: true, running: None, model: None, watches: Arc::clone(&watches),
        });
        std::fs::write(root.join("watched"), "PRIVATE FILE CONTENT MUST NOT BE SENT").unwrap();
        bridge.poll_watches_at(now + Duration::from_secs(6));
        assert!(watches.lock().unwrap().is_empty());
        let events = events.lock().unwrap();
        assert_eq!(events.iter().filter(|e| e.name == "watch.fired").count(), 1);
        assert_eq!(events.last().unwrap().name, "watch.ended");
        assert!(events.iter().all(|e| e.session.as_deref() == Some(&handle)));
        assert!(!events.iter().any(|e| e.data.to_string().contains("PRIVATE FILE")));
    }

    #[test]
    fn cancelling_an_automatic_turn_stops_only_its_originating_watch() {
        let mut builder = tempfile::Builder::new();
        builder.prefix("bravebot-watch-cancel-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().unwrap();
        let project = directory.path().to_path_buf();
        let now = Instant::now();
        let mut watches = bravebot_agent::watch::Watches::new();
        let first = watches.arm("first".into(), 1, bravebot_agent::watch::Looked::Saw("a".into()), now).unwrap();
        let second = watches.arm("second".into(), 1, bravebot_agent::watch::Looked::Saw("b".into()), now).unwrap();
        watches.dispatched(first);
        let watches = Arc::new(Mutex::new(watches));
        let cancel = Cancel::new();
        let (answers, _receiver) = mpsc::channel();
        let running = Running { cancel: cancel.clone(), answers, pending: Arc::new(Mutex::new(None)), turn: 1,
            finished: Arc::new(std::sync::atomic::AtomicBool::new(false)) };
        let mut bridge = Bridge::new(Box::new(|_| {}));
        let handle = bridge.mint(Open { project: project.clone(),
            state: Arc::new(Mutex::new(State::fresh(TrustStore::new(&project)))),
            answered_trust: true, running: Some(running), model: None, watches: Arc::clone(&watches) });
        let request = Request::parse(&json!({"id": 1, "method": "turn.cancel", "params": {"session": handle}}).to_string()).unwrap();
        bridge.dispatch(&request).unwrap();
        assert!(cancel.is_cancelled());
        let watches = watches.lock().unwrap();
        assert_eq!(watches.live().len(), 1);
        assert_eq!(watches.live()[0].number(), second);
    }

    #[test]
    fn polling_expires_watches_without_starting_a_turn() {
        let mut builder = tempfile::Builder::new();
        builder.prefix("bravebot-watch-expiry-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().unwrap();
        let root = directory.path().to_path_buf();
        std::fs::write(root.join("file"), "original").unwrap();
        let now = Instant::now();
        let mut watches = bravebot_agent::watch::Watches::new();
        watches.arm("file".into(), 1, Workspace::new(root.clone()).unwrap().look("file"), now).unwrap();
        let watches = Arc::new(Mutex::new(watches));
        let events = Arc::new(Mutex::new(Vec::new()));
        let held = Arc::clone(&events);
        let mut bridge = Bridge::new(Box::new(move |e| held.lock().unwrap().push(e)));
        bridge.mint(Open { project: root.clone(), state: Arc::new(Mutex::new(State::fresh(TrustStore::new(&root)))),
            answered_trust: true, running: None, model: None, watches: Arc::clone(&watches) });
        bridge.poll_watches_at(now + Duration::from_secs(7 * 24 * 60 * 60));
        assert!(watches.lock().unwrap().is_empty());
        assert_eq!(events.lock().unwrap()[0].data["reason"], "expired");
    }
}
