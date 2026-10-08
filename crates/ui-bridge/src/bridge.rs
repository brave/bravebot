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
use crate::running::{Mcp, Running, State};
use crate::turn::{BridgeConfirmer, BridgeReporter, BridgeSink, Reply};
use crate::{store, wire};
use bravebot_agent::confirm::Decision;
use bravebot_agent::trusted;
use bravebot_agent::turn::{self as agent_turn, Task, TurnError};
use bravebot_agent::workspace::WorkspaceError;
use bravebot_agent::{Confining, PermissionMode, Workspace};
use bravebot_config::{Config, Settings};
use bravebot_core::cancel::Cancel;
use bravebot_core::trust::TrustStore;
use bravebot_net::Egress;
use bravebot_session::sessions::{Handle, Record, Standing};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
    /// The record a "trust and remember" answer is kept in, while the question that offered it is
    /// waiting (TRUST-23). `None` where no question is waiting or remembering is not on offer.
    keeping: Option<(trusted::Store, trusted::Identity)>,
    /// The turn in flight, if there is one.
    running: Option<Running>,
    model: Option<String>,
    watches: Arc<Mutex<bravebot_agent::watch::Watches>>,
    /// Settled once, when the session opened, as the terminal does: a session that opened with
    /// the mode on said so at the top, and a later change to the file would make that untrue.
    auto_vetting: bool,
    /// The definition this session's conversation belongs to, once a turn has said so
    /// ([MEMORY-10](../../../docs/specs/definition-memory.md#MEMORY-10)).
    ///
    /// Set by a `turn.send` from the desktop's main process, which takes it from the bot's row.
    /// It is read for a turn the bridge starts on its own, the fire of a watch a person armed,
    /// which has no request to carry one.
    definition: Option<String>,
    /// What the next turn asks before it acts. Asking whenever a session opens, a resume or a fork
    /// included, since a mode is never read from a record (MODE-10).
    ///
    /// Held here rather than sent with each `turn.send`, because a watch fires a turn that no
    /// request asked for, and that turn must run in the mode the window shows.
    permission_mode: PermissionMode,
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
                    let open = self
                        .open
                        .get(&handle)
                        .ok_or_else(Failure::no_such_session)?;
                    let config =
                        crate::settings::config(Some(&open.project), self.settings.as_deref());
                    info["configured"] = json!(config.is_ok());
                    info["defaultModel"] = json!(config.ok().map(|c| c.default_model));
                }
                Ok(info)
            }
            "models.list" => {
                let directory = request.optional_string("directory").map(PathBuf::from);
                let config =
                    crate::settings::config(directory.as_deref(), self.settings.as_deref())?;
                Ok(crate::models::list(&config))
            }
            "session.list" => self.list(request),
            "session.open" => self.open_session(request),
            "session.new" => self.new_session(request),
            "session.view.start" => self.start_view(request),
            "session.fork" => self.fork_session(request),
            "session.delete" => self.delete_session(request),
            "session.close" => self.close_session(request),
            "session.mode" => self.set_permission_mode(request),
            "session.rewind" => self.rewind_session(request),
            "turn.send" => self.send_turn(request),
            "mentions.offer" => {
                let open = self
                    .open
                    .get(&request.string("session")?)
                    .ok_or_else(Failure::no_such_session)?;
                let cursor = request.param("cursor").as_u64().unwrap_or(0);
                Ok(crate::mentions::offer(
                    &open.project,
                    &request.string("line")?,
                    usize::try_from(cursor).unwrap_or(usize::MAX),
                ))
            }
            "mentions.named" => {
                let open = self
                    .open
                    .get(&request.string("session")?)
                    .ok_or_else(Failure::no_such_session)?;
                let settings =
                    crate::settings::layers(Some(&open.project), self.settings.as_deref());
                let workspace = session_workspace(open, &settings)?;
                let files = crate::mentions::named(&workspace, &request.string("prompt")?)?;
                Ok(json!({ "files": files }))
            }
            "turn.cancel" => self.cancel_turn(request),
            "watches.list" | "watches.add" | "watches.stop" => self.watches(request),
            "watches.poll" => {
                self.poll_watches();
                Ok(json!({}))
            }
            "confirm.reply" => self.reply_confirm(request),
            "run.reply" => self.reply_run(request),
            "output.reply" => self.reply_output(request),
            "vouch.reply" => self.reply_vouch(request),
            "vet.reply" => self.deliver(
                request,
                Reply::Vet(wire::decision(request.param("decision"))),
            ),
            "fetch.reply" => self.reply_decision(request, Reply::Fetch),
            "server.reply" => self.reply_decision(request, Reply::Server),
            "manifest.run" => self.start_manifest(request),
            "manifest.read" => crate::manifest::read(request),
            "manifest.reply" => self.reply_decision(request, Reply::Manifest),
            "exposure.reply" => self.reply_decision(request, Reply::Exposure),
            "mcp-server.reply" => self.deliver(
                request,
                Reply::McpServer(wire::start_answer(
                    request.param("decision"),
                    request.param("remember"),
                )),
            ),
            "mcp-tools.reply" => self.reply_decision(request, Reply::McpTools),
            "mcp-call.reply" => self.deliver(
                request,
                Reply::McpCall(wire::call_decision(
                    request.param("decision"),
                    request.param("remember"),
                )),
            ),
            "mcp-move.reply" => self.reply_decision(request, Reply::McpMove),
            "ask.reply" => self.reply_ask(request),
            "trust.reply" => self.reply_trust(request),
            "permissions.list" => self.permissions(request, false),
            "permissions.revoke" => self.permissions(request, true),
            "settings.inspect" => {
                let project = request
                    .optional_string("session")
                    .and_then(|id| self.open.get(&id))
                    .map(|s| s.project.as_path());
                Ok(crate::settings::report(project, self.settings.as_deref()))
            }
            "settings.select" => {
                let path = request.optional_string("path").map(PathBuf::from);
                if let Some(path) = &path {
                    crate::settings::validate(path)?;
                }
                self.settings = path;
                Ok(crate::settings::report(None, self.settings.as_deref()))
            }
            "connectors.list" => crate::connectors::list(),
            "connectors.preview" => crate::connectors::preview(request),
            "connectors.connect" => crate::connectors::connect(request),
            "connectors.disconnect" => crate::connectors::disconnect(request),
            "connectors.remove" => crate::connectors::remove(request),
            "hooks.inspect" => crate::hooks::inspect(),
            "bot.define" => crate::definitions::make(request),
            "bot.migrate" => crate::definitions::migrate(request),
            "bot.redefine" => crate::definitions::remake(request),
            "doctor" => Ok(
                json!({"found": true, "structured": true, "text": serde_json::to_string_pretty(&crate::settings::report(None, self.settings.as_deref())).unwrap_or_default()}),
            ),
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
            "capabilities": { "sessionView": crate::view::capability(), "actionTargets": crate::view::action_targets() },
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
                    // Whether the record is a manifest run. A run has no conversation, so it
                    // is read with `manifest.read` and `session.open` refuses it.
                    "manifest": entry.summary.manifest,
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
        // A manifest run has no conversation to resume (MANIFEST-11). Opening one as a session
        // would offer to continue a run that cannot be continued, so it is refused, as the
        // terminal's picker refuses it.
        if record.manifest.is_some() {
            return Err(Failure::bad_request(format!(
                "`{id}` is a manifest run, which cannot be continued. Read it with manifest.read"
            )));
        }

        // A record that recorded a trust map was answered for by the person now resuming
        // it, and inherits it. One that did not is asked again: nothing recorded is not
        // the same as nothing trusted, and reading an absent map as an empty one would
        // answer on behalf of somebody who was never asked.
        let opening = Opening::of(&directory, record.trust_map(&directory));
        let reported = opening.json();
        // A kept answer settles a record that kept no map, and the session then opens known, as a
        // fork of it does: `known: false` is the one sign that the question was put.
        let settled = opening
            .remembered
            .is_some()
            .then(|| json!({ "known": true, "rules": rules_json(&opening.trust) }));
        let state = State::resumed(&directory, &record, opening.trust);

        let auto_vetting = self.auto_vetting(&directory);
        let handle = self.mint(Open {
            project: directory.clone(),
            state: Arc::new(Mutex::new(state)),
            answered_trust: opening.answered,
            keeping: opening.keeping,
            running: None,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            model: None,
            auto_vetting,
            definition: None,
            permission_mode: PermissionMode::Ask,
        });
        self.ask_about_trust(&handle);
        let rules = self.open_under_rules(&handle, None);

        let mut opened = self.recount(&handle, &directory, &record, auto_vetting);
        opened["settingsRules"] = rules;
        opened["scratch"] = self.scratch_report(&handle);
        if let Some(open) = self.open.get(&handle)
            && let Ok(state) = open.state.lock()
        {
            opened["rewind"] = json!(rewind_json(&state, &directory));
        }
        if let Some(settled) = settled {
            opened["trust"] = settled;
        }
        merge(&mut opened, reported);
        Ok(opened)
    }

    /// Settle the permission rules the session opened as `handle` runs under, and return them
    /// as a front end reads them.
    ///
    /// Read from the settings files once, here, and kept by the session (PERM-12). `inherited`
    /// is a parent's rules, which a fork takes in place of reading the files again.
    fn open_under_rules(
        &self,
        handle: &str,
        inherited: Option<crate::rules::SettingsRules>,
    ) -> Value {
        let Some(open) = self.open.get(handle) else {
            return Value::Null;
        };
        let rules = inherited.unwrap_or_else(|| {
            crate::rules::SettingsRules::read(
                &crate::settings::layers(Some(&open.project), self.settings.as_deref()),
                &open.project,
            )
        });
        let reported = rules.json();
        if let Ok(mut state) = open.state.lock() {
            state.rules = rules;
        }
        reported
    }

    /// Put the trust question to the window, where the session opened as `handle` is waiting on it.
    fn ask_about_trust(&self, handle: &str) {
        let Some(open) = self.open.get(handle) else {
            return;
        };
        if open.answered_trust {
            return;
        }
        self.emitter.send(Event::new(
            "trust.request",
            handle,
            json!({
                "directory": open.project.display().to_string(),
                "keeping": open.keeping.as_ref().map(|(store, _)| store.path().display().to_string()),
            }),
        ));
    }

    /// What the session opened as `handle` reports about its own directory outside the project
    /// (TRUST-14): the path, or that it has none and why.
    fn scratch_report(&self, handle: &str) -> Value {
        self.open
            .get(handle)
            .and_then(|open| open.state.lock().ok().map(|state| state.scratch.report()))
            .unwrap_or(Value::Null)
    }

    fn auto_vetting(&self, project: &std::path::Path) -> bool {
        crate::settings::auto_vetting(&crate::settings::layers(
            Some(project),
            self.settings.as_deref(),
        ))
    }

    /// Everything a front-end needs to draw a session it did not watch happen.
    fn recount(
        &self,
        handle: &str,
        directory: &std::path::Path,
        record: &Record,
        auto_vetting: bool,
    ) -> Value {
        // Restored rather than read straight off the record, because restoring is what
        // adds the note saying the quarantine's references no longer name anything — and
        // `recounted` filters that note back out. Going around it would show a transcript
        // subtly unlike the one a resume produces.
        let conversation = bravebot_agent::Conversation::restored(record.conversation.clone());
        let said = wire::recounted(&conversation.recounted());

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
                "front": record.front,
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
            "frontNote": bravebot_session::sessions::front_note(
                record.front.as_deref(),
                crate::FRONT,
            ),
            "autoVetting": auto_vetting,
            "permissionMode": wire::permission_mode_name(PermissionMode::Ask),
        })
    }

    fn start_view(&mut self, request: &Request) -> Result<Value, Failure> {
        if request.number("version")? != 1 {
            return Err(Failure::bad_request("unsupported session view version"));
        }
        let handle = request.string("session")?;
        let open = self
            .open
            .get(&handle)
            .ok_or_else(Failure::no_such_session)?;
        if open.running.is_some() {
            return Err(Failure::bad_request("start the view before the first turn"));
        }
        let state = open
            .state
            .lock()
            .map_err(|_| Failure::bad_request("session unavailable"))?;
        if state.handle.is_some() || state.turns != 0 {
            return Err(Failure::bad_request("the view requires a fresh session"));
        }
        self.emitter.start_view(&handle, open.answered_trust)?;
        Ok(json!({"version": 1}))
    }

    fn new_session(&mut self, request: &Request) -> Result<Value, Failure> {
        let directory = PathBuf::from(request.string("directory")?);

        if !directory.is_dir() {
            return Err(Failure::new(
                ErrorCode::NotADirectory,
                format!("{} is not a directory", directory.display()),
            ));
        }

        let branch = bravebot_session::sessions::branch_of(&directory);
        let auto_vetting = self.auto_vetting(&directory);
        // Nothing carried in, whatever an earlier session here answered, unless the person said to
        // remember that answer. Unanswered, the map is empty until they answer; nothing runs before
        // then, so it is never the map a turn uses.
        let opening = Opening::of(&directory, None);
        let reported = opening.json();
        let handle = self.mint(Open {
            project: directory.clone(),
            state: Arc::new(Mutex::new(State::fresh(opening.trust))),
            answered_trust: opening.answered,
            keeping: opening.keeping,
            running: None,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            model: None,
            auto_vetting,
            definition: None,
            permission_mode: PermissionMode::Ask,
        });

        // Nothing is written until the first turn. An opened-and-abandoned window should
        // leave no trace, which is also how `bravebot` behaves.
        self.ask_about_trust(&handle);
        let rules = self.open_under_rules(&handle, None);

        let mut made = json!({
            "session": handle,
            "settingsRules": rules,
            "model": crate::settings::config(Some(&directory), self.settings.as_deref()).ok().map(|config| config.default_model),
            "directory": directory.display().to_string(),
            "branch": branch,
            "autoVetting": auto_vetting,
            "scratch": self.scratch_report(&handle),
            "permissionMode": wire::permission_mode_name(PermissionMode::Ask),
        });
        merge(&mut made, reported);
        Ok(made)
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

        let open = self
            .open
            .get(&handle)
            .ok_or_else(Failure::no_such_session)?;
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
        let auto_vetting = open.auto_vetting;

        // Everything needed is copied out under the lock and the lock is dropped before any of
        // it is used. A fork does no I/O and no thinking, but holding a session's state across
        // work is the habit that turns into a stall later.
        let (snapshot, said, trust, programs, directories, todos, parent, inherited) = {
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
                parent.clone(),
                state.rules.clone(),
            )
        };

        if crate::fork::imported_in(&snapshot.archive)
            && ordinal < crate::fork::prompts(&said).len()
        {
            return Err(Failure::bad_request(
                "this session holds words copied from another program, \
                 which a fork does not carry into a request",
            ));
        }
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
        let recounted = wire::recounted(&before);
        // The first thing said in the history the child keeps, which is what titles it. Without
        // this a fork would be named after the prompt that replaced the one it was cut at, and a
        // list of forks would say nothing about where any of them came from.
        let first_prompt = crate::fork::prompts(&before)
            .first()
            .map(|prompt| (*prompt).to_string());

        // Cut to the same place the conversation was: a turn's plan belongs to the turn, and the
        // child has the turns before the cut and no others.
        let todos: std::collections::BTreeMap<_, _> = todos
            .into_iter()
            .filter(|(turn, _)| *turn <= ordinal)
            .collect();
        // The child knows its map exactly when the parent did. A parent still holding the
        // question (a record written before maps were kept) hands the question down, which a
        // kept answer about the directory settles as it would for the parent.
        let opening = Opening::of(&project, answered_trust.then_some(trust));
        let reported = opening.json();
        let known = opening.answered;
        let rules = rules_json(&opening.trust);

        let mut begun = self.begin_unique(&project)?;
        begun.inherit_rewind_warnings(&parent);
        let parent_id = parent.id().to_string();
        let parent_title = parent.title().to_string();
        let id = begun.id().to_string();

        let child = self.mint(Open {
            project: project.clone(),
            state: Arc::new(Mutex::new(State::forked(
                begun,
                cut.before,
                opening.trust,
                programs,
                directories,
                ordinal,
                todos.clone(),
                first_prompt,
            ))),
            // Inherited along with the map itself: the same person, in the same directory, in
            // the same window, so asking again would be asking somebody to answer twice.
            answered_trust: opening.answered,
            keeping: opening.keeping,
            running: None,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            model: None,
            // The parent's, not read again: the child's transcript is the parent's up to the cut,
            // and the notice at its top says what the parent opened under.
            auto_vetting,
            definition: None,
            // Not the parent's. A fork is a session opened again, and opens asking as one does.
            permission_mode: PermissionMode::Ask,
        });
        self.ask_about_trust(&child);
        // The parent's, not read again, for the reason auto-vetting is: a fork carries on the
        // session it was cut from.
        let settings_rules = self.open_under_rules(&child, Some(inherited));

        let title = if parent_title.is_empty() {
            store::load(&project, &parent_id).map(|record| record.title)
        } else {
            Some(parent_title)
        };

        let mut forked = json!({
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
            "autoVetting": auto_vetting,
            "permissionMode": wire::permission_mode_name(PermissionMode::Ask),
            "settingsRules": settings_rules,
            "scratch": self.scratch_report(&child),
            "parent": {
                "id": parent_id,
                "directory": project.display().to_string(),
                "title": title,
                "prompt": ordinal,
            },
        });
        merge(&mut forked, reported);
        Ok(forked)
    }

    /// Remove a stored session from disk (SESSION-30).
    ///
    /// Keyed on the `directory` and `id` the list gave, like `session.open`, and nothing read out
    /// of a record decides any part of it. A session this bridge has open is refused rather than
    /// closed from under its window: a turn that saved afterwards would write the record again.
    fn delete_session(&mut self, request: &Request) -> Result<Value, Failure> {
        let directory = PathBuf::from(request.string("directory")?);
        let id = request.string("id")?;

        let handles: Vec<String> = self.open.keys().cloned().collect();
        for handle in &handles {
            self.reap(handle);
        }
        let key = bravebot_session::sessions::key_for(&directory);
        // A worker keeps a session's lock for the whole of its turn, so the id of a session with a
        // turn running cannot be read. It is set apart from a session known to hold `id`.
        let mut holder = None;
        let mut busy = None;
        for open in self
            .open
            .values()
            .filter(|open| bravebot_session::sessions::key_for(&open.project) == key)
        {
            match open.state.try_lock() {
                Ok(state) if state.handle.as_ref().is_some_and(|held| held.id() == id) => {
                    holder = Some(open);
                    break;
                }
                Ok(_) => {}
                Err(_) => busy = busy.or(Some(open)),
            }
        }
        if let Some(open) = holder {
            return Err(if open.running.is_some() {
                Failure::new(
                    ErrorCode::TurnInFlight,
                    "a turn is running in the session being deleted",
                )
            } else {
                Failure::bad_request(
                    "the session is open; close it with session.close before deleting it",
                )
            });
        }
        if let Some(open) = busy {
            return Err(if open.running.is_some() {
                Failure::new(
                    ErrorCode::TurnInFlight,
                    "a turn is running in a session of this project, so the session being deleted cannot be told apart from it; wait for the turn to end",
                )
            } else {
                Failure::bad_request(
                    "a session of this project is in use, so the session being deleted cannot be told apart from it; try again",
                )
            });
        }

        store::delete(&directory, &id).map_err(|refusal| match refusal {
            bravebot_session::sessions::Deletion::Invalid => {
                Failure::bad_request(format!("`{id}` is not a session name"))
            }
            bravebot_session::sessions::Deletion::NotFound => Failure::new(
                ErrorCode::NoSuchSession,
                format!("no session `{id}` in {}", directory.display()),
            ),
            bravebot_session::sessions::Deletion::Failed(error) => Failure::new(
                ErrorCode::Internal,
                format!("could not delete session `{id}`: {error}"),
            ),
        })?;
        Ok(json!({ "deleted": true }))
    }

    fn close_session(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        let open = self
            .open
            .remove(&handle)
            .ok_or_else(Failure::no_such_session)?;

        self.emitter.detach_view(&handle);

        // Stop the work, then refuse whatever it was waiting on. Both, and in that order:
        // cancelling alone would leave a write blocked on an answer that is never coming,
        // and refusing alone would let the turn carry on past it.
        if let Some(running) = &open.running {
            running.cancel.cancel();
            running.refuse_pending();
        }
        Ok(json!({}))
    }

    /// Choose what the session's next turn asks before it acts.
    ///
    /// Accepted while a turn runs, and it changes nothing about that turn: the turn read its mode
    /// when it was accepted and keeps it (MODE-8). A write already on screen keeps its question.
    fn set_permission_mode(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        let mode = wire::permission_mode(request.param("mode"))?;
        let open = self
            .open
            .get_mut(&handle)
            .ok_or_else(Failure::no_such_session)?;
        open.permission_mode = mode;
        Ok(json!({ "permissionMode": wire::permission_mode_name(mode) }))
    }

    // ------------------------------------------------------------ turns

    /// Start a turn, and return before it finishes.
    ///
    /// Everything the turn produces arrives as events. The response says only that it
    /// began, because a turn takes as long as a model does and a front-end that blocked
    /// on it would show nothing until it ended.
    fn send_turn(&mut self, request: &Request) -> Result<Value, Failure> {
        // The one tag a request may claim, and every other word refused. A front end composes
        // prompts of its own, so it has to be able to say which those are or its transcript is
        // back to recognising them by their wording; what it may not do is claim one of the
        // agent's, which is [`wire::composed`]'s whole job.
        let composed = crate::wire::composed(request.params.get("composed"))?;
        self.start_turn(request, composed)
    }

    /// The same, for a turn this crate asked for and knows what it composed.
    ///
    /// The agent's own tags are a parameter here and not a field of the request, and that is the
    /// point: a front end able to say the *agent* composed a prompt would be a front end able to
    /// have a transcript draw a file's row, or a watch's, around a line a person typed, which is
    /// the thing the tag exists to stop anyone doing. The tag for a prompt the front end composed
    /// itself is a different claim, and [`wire::composed`] is where a request may make it.
    fn start_turn(
        &mut self,
        request: &Request,
        composed: Option<bravebot_agent::conversation::Composed>,
    ) -> Result<Value, Failure> {
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
        let dropped = dropped_paths(request)?;
        let attachments = crate::attached::dropped(request)?;
        let images = crate::attached::pasted(request)?;
        // The definition a bot's conversation is addressed to (MEMORY-10). Named by the desktop's
        // main process from the bot's row, never by a window, and checked to be a name a
        // definition can carry. Once a session has one, every later turn in it is addressed to it
        // whether or not the request repeats the name: a turn with none would hold the session's
        // whole reach, wider than the bot's own, and the fire of a watch has no request to carry
        // one at all.
        let named = crate::wire::definition(request.params.get("definition"))?;
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

        let open = self
            .open
            .get(&handle)
            .ok_or_else(Failure::no_such_session)?;

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
        // The name the session was given, kept so a turn with no request behind it is addressed
        // too.
        let addressing = named.or_else(|| open.definition.clone());

        // The layers the configuration below comes from, read here rather than in the worker so
        // what they say is what stood when the turn was asked for. Three answers come off them:
        // the workspace the turn reads through, what a commit message or a pull request this turn
        // writes may carry (BACKEND-30), and the caps a search this turn makes runs under
        // (SEARCH-9).
        let settings = crate::settings::layers(Some(&open.project), self.settings.as_deref());
        let workspace = session_workspace(open, &settings)?;
        // The files the prompt names with `@` (NAME-9), read out of the prompt here rather than
        // taken from the front end, and each one surveyed by the read the turn will make, so a
        // name that would end the turn refuses the send before anything starts. A prompt nobody
        // typed names nothing, the same line the terminal draws.
        let mut files = files;
        if composed.is_none() {
            for named in crate::mentions::named(&workspace, &prompt)? {
                if !files.contains(&named) {
                    files.push(named);
                }
            }
        }

        let model = requested_model.or_else(|| open.model.clone());
        let config = crate::settings::config(Some(&open.project), self.settings.as_deref())?;
        // A model the machine-level layer refuses is not requested, whichever service would have
        // answered for it and whoever asked. The roster this window draws leaves such a model out,
        // so reaching here means the name came from somewhere else (BACKEND-48).
        if let Some(refused) = crate::models::refused(&config, model.as_deref()) {
            return Err(Failure::bad_request(refused));
        }
        let attribution = settings.attribution().clone();
        let output_cap = settings.run_output_cap();
        let deadlines = bravebot_agent::exec::Deadlines::resolve(settings.run_deadlines());
        let auto_vetting = open.auto_vetting;
        // Read once, here, and carried to the worker: the mode the planner is told and the mode its
        // prompts are answered in are then the same one, whatever the window chooses while the turn
        // runs (MODE-8).
        let permission_mode = open.permission_mode;
        // Each MCP server the settings request, with the file that requested it (SERVERS-2). Read
        // on every turn and used by the first, which is the one that starts them.
        let mcp_requested: Vec<(PathBuf, String)> = settings
            .mcp_requested()
            .map(|(file, alias)| (file.to_path_buf(), alias.to_string()))
            .collect();
        let project = open.project.clone();
        let state = Arc::clone(&open.state);
        let watches = Arc::clone(&open.watches);
        let turn_number = state.lock().map(|s| s.turns + 1).unwrap_or(1);

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

        self.emitter.view_started(
            &handle,
            turn_number as u64,
            wire::submitted(&prompt, composed.as_ref()),
        );
        self.emitter.send(Event::new(
            "turn.started",
            &handle,
            json!({
                "turn": turn_number,
                "mode": wire::permission_mode_name(permission_mode),
            }),
        ));

        if let Some(open) = self.open.get_mut(&handle) {
            open.running = Some(running);
            open.model = model;
            open.definition = addressing.clone();
        }
        thread::spawn(move || {
            work(Work {
                model: worker_model,
                addressing,
                watches,
                emitter,
                session,
                project,
                state,
                config,
                attribution,
                output_cap,
                deadlines,
                auto_vetting,
                permission_mode,
                mcp_requested,
                workspace,
                prompt,
                composed,
                files,
                dropped,
                attachments,
                images,
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

    /// Start a manifest run, and return before it finishes (MANIFEST-11).
    ///
    /// A run plans the whole task, puts the frozen plan to the person, and walks it. It is not a
    /// turn: the session's conversation is neither sent nor changed, and the run is saved as its
    /// own record. The session is busy until the run ends, so a turn or a second run is refused.
    ///
    /// `turn.cancel` stops a run, and the reply methods answer its questions, as for a turn.
    fn start_manifest(&mut self, request: &Request) -> Result<Value, Failure> {
        if self.emitter.has_view(&request.string("session")?) {
            return Err(Failure::bad_request(
                "session view version 1 does not support manifest runs",
            ));
        }
        let handle = request.string("session")?;
        let task = request.string("task")?;
        if task.trim().is_empty() {
            return Err(Failure::bad_request("a manifest run needs a task to plan"));
        }
        // A named file is context a turn reads before it decides. A run fixes its plan before
        // anything is read (MANIFEST-9), so the file would be dropped. Refuse instead.
        for named in ["files", "dropped", "attachments", "images"] {
            let given = request
                .params
                .get(named)
                .and_then(Value::as_array)
                .is_some_and(|entries| !entries.is_empty());
            if given {
                return Err(Failure::bad_request(format!(
                    "a manifest run takes no `{named}`: its plan is fixed before anything is \
                     read. Name the file in the task instead"
                )));
            }
        }
        let requested_model = crate::models::selection(request.params.get("model"))?;

        self.reap(&handle);

        let open = self
            .open
            .get(&handle)
            .ok_or_else(Failure::no_such_session)?;
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
        // A model the machine-level layer refuses is not requested, whichever service would have
        // answered for it and whoever asked. The roster this window draws leaves such a model out,
        // so reaching here means the name came from somewhere else (BACKEND-48).
        if let Some(refused) = crate::models::refused(&config, model.as_deref()) {
            return Err(Failure::bad_request(refused));
        }
        let settings = crate::settings::layers(Some(&open.project), self.settings.as_deref());
        let attribution = settings.attribution().clone();
        let output_cap = settings.run_output_cap();
        let deadlines = bravebot_agent::exec::Deadlines::resolve(settings.run_deadlines());
        // Read when the run is accepted and kept to its end, as a turn keeps its own (MODE-8).
        let permission_mode = open.permission_mode;
        let workspace = session_workspace(open, &settings)?;

        let project = open.project.clone();
        let state = Arc::clone(&open.state);
        let (run, turns) = state
            .lock()
            .map(|mut s| {
                // A run writes files as a turn does, so the same coverage gap applies.
                s.rewind
                    .record_gap(bravebot_agent::rewind::CoverageGap::Desktop);
                s.runs += 1;
                (s.runs, s.turns)
            })
            .unwrap_or((1, 0));

        let cancel = Cancel::new();
        let (answers_tx, answers_rx) = mpsc::channel();
        let pending: crate::turn::Pending = Arc::new(Mutex::new(None));
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let running = Running {
            cancel: cancel.clone(),
            answers: answers_tx,
            pending: Arc::clone(&pending),
            // The session's last turn. A run is not a turn and takes no number of its own.
            turn: turns,
            finished: Arc::clone(&finished),
        };

        self.emitter.send(Event::new(
            "manifest.started",
            &handle,
            json!({ "run": run, "mode": wire::permission_mode_name(permission_mode) }),
        ));

        if let Some(open) = self.open.get_mut(&handle) {
            open.running = Some(running);
            open.model = model.clone();
        }
        let emitter = self.emitter.clone();
        let session = handle.clone();
        thread::spawn(move || {
            crate::manifest::walk(crate::manifest::Walk {
                emitter,
                session,
                project,
                state,
                config,
                attribution,
                output_cap,
                deadlines,
                model,
                permission_mode,
                workspace,
                task,
                run,
                cancel,
                pending,
                answers: answers_rx,
                finished,
            });
        });

        Ok(json!({ "run": run }))
    }

    /// Put the session back to where it stood `steps` turns ago, on disk and in the conversation
    /// together (SESSION-19), as the terminal's `/undo` and `/rewind` do.
    ///
    /// Refused while a turn runs, for the reason a fork is: the worker holds the state, and only
    /// a turn that has finished has joined its delegates and kept its backups.
    fn rewind_session(&mut self, request: &Request) -> Result<Value, Failure> {
        use bravebot_agent::rewind::CoverageGap;
        let handle = request.string("session")?;
        let steps = request.number("steps")? as usize;
        self.reap(&handle);
        let open = self
            .open
            .get(&handle)
            .ok_or_else(Failure::no_such_session)?;
        if open.running.is_some() {
            return Err(Failure::new(
                ErrorCode::TurnInFlight,
                "Stop the current turn before undoing one.",
            ));
        }
        let project = open.project.clone();
        let settings = crate::settings::layers(Some(&project), self.settings.as_deref());
        let mut workspace = turn_workspace(
            project.clone(),
            &settings,
            &bravebot_config::Managed::load(),
        )
        .map_err(|error| Failure::new(ErrorCode::Internal, error.to_string()))?;
        let mut guard = open
            .state
            .lock()
            .map_err(|_| Failure::new(ErrorCode::Internal, "session state is poisoned"))?;
        let state = &mut *guard;
        // Opened as a turn opens them, so a path a rewound turn wrote in one goes back there.
        for directory in &state.directories {
            let _ = workspace.add_directory(&directory.display().to_string());
        }

        state.rewind.bind_coverage(&workspace);
        let Some(point) = state.rewind.take(steps) else {
            return Err(Failure::bad_request(match state.rewind.points().len() {
                0 => "There is nothing to undo.".to_string(),
                1 => "This session can go back 1 turn at most.".to_string(),
                kept => format!("This session can go back {kept} turns at most."),
            }));
        };
        if let Some(stored) = state.handle.as_mut() {
            stored.retain_rewind_coverage(&point.coverage);
        }
        let gaps = point.coverage.gaps();
        let snapshot = point.snapshot;
        let refused = bravebot_agent::rewind::restore(
            &workspace,
            point.backups,
            &mut state.trust,
            &snapshot.trust,
            &mut state.servers,
            bravebot_agent::home::directory().as_deref(),
        );
        if !refused.is_empty() {
            state.rewind.record_gap(CoverageGap::BackupUnavailable);
        }

        state.conversation = bravebot_agent::Conversation::restored(snapshot.conversation);
        state.turns = snapshot.turns;
        state.tokens = snapshot.tokens;
        state.spend = snapshot.spend;
        state.timing = snapshot.timing;
        // The snapshot's approvals that are still held. The terminal takes the snapshot's whole,
        // which is the same set there, since its approvals only grow; here one can be revoked
        // after the turn that is being undone, and a rewind must not hand it back.
        let mut programs = snapshot.programs;
        let revoked: Vec<_> = programs
            .iter()
            .filter(|command| !state.programs.iter().any(|held| held == *command))
            .cloned()
            .collect();
        for command in &revoked {
            programs.forget(command);
        }
        state.programs = programs;
        if let Some(history) = state.history.as_mut() {
            history.retain(|turn| turn.number <= snapshot.turns);
        }
        state.todos.retain(|turn, _| *turn <= snapshot.turns);

        let discard = snapshot.turns == 0
            && !snapshot.was_wrote
            && refused.is_empty()
            && gaps.is_empty()
            && state.trust == snapshot.trust;
        if let Some(stored) = state.handle.as_mut() {
            stored.truncate_audit(snapshot.turns + 1);
            if discard {
                stored.discard_unwritten(&snapshot.title);
            }
        }
        if discard {
            // The record is gone, so the next prompt names the session again.
            state.first_prompt = None;
        } else if state.handle.is_some() {
            save(
                &project,
                state,
                snapshot.turns,
                &bravebot_session::audit::Trail::default(),
            );
        }

        let standing = state.conversation.snapshot();
        Ok(json!({
            "session": handle,
            // The turn the session now stands before, and the prompt that began it, which a
            // window can offer to send again.
            "turn": snapshot.turns + 1,
            "text": point.prompt,
            "refused": refused
                .iter()
                .map(|path| shown_path(path, &project))
                .collect::<Vec<_>>(),
            "gaps": gaps,
            "said": wire::recounted(&state.conversation.recounted()),
            "context": standing.context,
            "contextTokens": standing.measured,
            "archived": standing.archive.len(),
            "todos": todos_json(&state.todos),
            "trust": { "rules": rules_json(&state.trust) },
            "rewind": rewind_json(state, &project),
        }))
    }

    /// Ask the turn to stop.
    ///
    /// Returns at once; the turn ends when the engine next looks at the token. A pending
    /// question wakes through the confirmer’s cancellation check and resolves to refusal.
    /// Cancellation never sends an approval or authorises a write.
    fn cancel_turn(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        // The turn the caller meant to stop (RPCVIEW-6). Absent, the request means whatever is
        // running, as it always has. Present, it stops that turn and no other.
        let expected = match request.params.get("turn") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_u64()
                    .ok_or_else(|| Failure::bad_request("`turn` must be a number"))?,
            ),
        };
        let open = self
            .open
            .get(&handle)
            .ok_or_else(Failure::no_such_session)?;
        if let Some(expected) = expected {
            let running = open
                .running
                .as_ref()
                .filter(|running| !running.is_finished() && running.turn as u64 == expected);
            let Some(running) = running else {
                // An old cancel for a turn that is over reaches a later turn as nothing at all, and
                // does not stop the watches either.
                return Ok(json!({ "cancelled": false }));
            };
            running.cancel.cancel();
            if let Ok(mut watches) = open.watches.lock() {
                watches.stop_firing();
            }
            return Ok(json!({ "cancelled": true }));
        }
        if let Some(running) = &open.running {
            running.cancel.cancel();
        }
        if let Ok(mut watches) = open.watches.lock() {
            watches.stop_firing();
        }
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

    /// Answer a question that is a yes or a no and nothing besides.
    ///
    /// `asked` is the kind the method was called for, so which question a reply answers is
    /// decided by the method a front end called and never by a field it sent. The decision is
    /// read by [`wire::decision`], where only the exact word approves.
    fn reply_decision(
        &mut self,
        request: &Request,
        asked: fn(Decision) -> Reply,
    ) -> Result<Value, Failure> {
        self.deliver(request, asked(wire::decision(request.param("decision"))))
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

        let open = self
            .open
            .get(&handle)
            .ok_or_else(Failure::no_such_session)?;
        let Some(running) = &open.running else {
            return Err(Failure::new(
                ErrorCode::NoSuchRequest,
                "no turn is running in this session",
            ));
        };

        if running.answer_with(id, reply, || self.emitter.view_answered(&handle, id)) {
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
        let remember = match request.params.get("remember") {
            None | Some(Value::Null) => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| Failure::bad_request("`remember` must be a boolean"))?,
        };
        if remember && !trusted {
            return Err(Failure::bad_request("only a yes can be remembered"));
        }

        let open = self
            .open
            .get_mut(&handle)
            .ok_or_else(Failure::no_such_session)?;
        // The question is answered once (RPCVIEW-6). A repeat is refused before the session's
        // state is touched, so it cannot replace the trust map, or hold up a cancel behind the lock.
        if open.answered_trust {
            return Err(Failure::new(
                ErrorCode::NoSuchRequest,
                "the trust question is not waiting for an answer",
            ));
        }
        // Kept only where the question offered it, so an answer the window was never shown the
        // record for, or one given where it may not be kept, writes nothing (TRUST-23).
        if remember && open.keeping.is_none() {
            return Err(Failure::bad_request(
                "remembering this answer was not offered for this session",
            ));
        }

        // Trusting records the workspace root, which covers everything beneath it.
        // Declining records nothing, leaving a map in which no path is trusted. The same
        // outcomes the terminal offers, so an answer means the same in both.
        let mut trust = bravebot_agent::workspace::trust_store(&open.project);
        if trusted {
            trust.trust(".");
        }
        // A session that has not had a turn has no name yet, and the name a line carries decides
        // nothing, so such a line names none.
        let mut named = String::new();
        if let Ok(mut state) = open.state.lock() {
            state.trust = trust;
            if let Some(handle) = state.handle.as_ref() {
                named = handle.id().to_string();
            }
        }
        open.answered_trust = true;
        self.emitter.view_trusted(&handle);
        // The question is answered, so what it offered is spent.
        let keeping = open.keeping.take();
        let kept = match keeping {
            Some((store, identity)) if remember => {
                Some(store.keep(&identity, &named, seconds_now()))
            }
            _ => None,
        };

        Ok(json!({ "trusted": trusted, "kept": kept }))
    }

    /// Inspect or reduce existing grants. No operation on this channel can add trust.
    fn permissions(&mut self, request: &Request, revoke: bool) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        self.reap(&handle);
        let open = self
            .open
            .get(&handle)
            .ok_or_else(Failure::no_such_session)?;
        if open.running.is_some() {
            return Err(Failure::new(
                ErrorCode::TurnInFlight,
                "Stop the current turn before reviewing or revoking permissions.",
            ));
        }
        let mut state = open
            .state
            .lock()
            .map_err(|_| Failure::new(ErrorCode::Internal, "Session state unavailable"))?;
        if revoke {
            let kind = request.string("kind")?;
            match kind.as_str() {
                "path" => {
                    let path = request.string("path")?;
                    if !state.trust.rules().any(|(held, integrity)| {
                        held == path && integrity == Some(bravebot_core::label::Integrity::Trusted)
                    }) {
                        return Err(Failure::bad_request(
                            "This path grant is no longer present.",
                        ));
                    }
                    state.trust.distrust(&path);
                }
                "command" => {
                    let selected = request
                        .params
                        .get("command")
                        .ok_or_else(|| Failure::bad_request("Missing command"))?;
                    let command = state
                        .programs
                        .iter()
                        .find(|command| {
                            json!({"program": command.program, "startedAs": command.started_as, "args": command.args})
                                == *selected
                        })
                        .cloned()
                        .ok_or_else(|| {
                            Failure::bad_request("This command grant is no longer present.")
                        })?;
                    state.programs.forget(&command);
                }
                // Withdrawn for the directory rather than this session, which keeps the map it
                // has: the next session started there asks (TRUST-24).
                "remembered" => forget_trust(&open.project)?,
                _ => return Err(Failure::bad_request("Unknown permission kind")),
            }
            // The session's own record changes only with its own grants.
            if state.handle.is_some() && kind != "remembered" {
                let turn = state.turns;
                save(
                    &open.project,
                    &mut state,
                    turn,
                    &bravebot_session::audit::Trail::default(),
                );
            }
        }
        Ok(json!({
            "paths": rules_json(&state.trust),
            "commands": state.programs.iter().map(|command| json!({"program": command.program, "startedAs": command.started_as, "args": command.args, "display": command.display()})).collect::<Vec<_>>(),
            // Read now rather than when the session opened: the record belongs to every session
            // begun in the directory, and another may have kept or withdrawn the answer since.
            "remembered": remembered_json(&open.project),
            // The rules this session opened under. They are read from settings files and
            // cannot be revoked here: editing the file changes them for the next session.
            "settingsRules": state.rules.json(),
        }))
    }

    fn watches(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        self.reap(&handle);
        let open = self
            .open
            .get(&handle)
            .ok_or_else(Failure::no_such_session)?;
        let mut watches = open
            .watches
            .lock()
            .map_err(|_| Failure::bad_request("Watches unavailable."))?;
        if request.method == "watches.add" {
            if !open.answered_trust {
                return Err(Failure::bad_request(
                    "Answer the project trust question first.",
                ));
            }
            if open.running.is_some() {
                return Err(Failure::new(
                    ErrorCode::TurnInFlight,
                    "Wait for the current turn before adding a watch.",
                ));
            }
            let path = request.string("path")?;
            if path.is_empty() || path.chars().any(char::is_control) {
                return Err(Failure::bad_request("Choose a project file."));
            }
            if !watches.live().iter().any(|w| w.path() == path) {
                let workspace = Workspace::new(open.project.clone())
                    .map_err(|_| Failure::bad_request("Project unavailable."))?;
                watches.arm(path.clone(), workspace.root().to_path_buf(), 0, workspace.look(&path, workspace.root()), std::time::Instant::now())
                    .map_err(|_| Failure::bad_request("Cannot watch this file. It must be inside the project, with fewer than eight active watches."))?;
            }
        } else if request.method == "watches.stop" {
            if request.flag("all", false) {
                watches.stop_all();
            } else {
                let number = request
                    .param("number")
                    .as_u64()
                    .ok_or_else(|| Failure::bad_request("A watch number is required."))?;
                watches.stop(number as usize);
            }
        }
        let now = std::time::Instant::now();
        Ok(
            json!({"watches": watches.live().iter().map(|w| json!({"number": w.number(), "path": w.path(),
            "remainingSeconds": w.left(now).as_secs(), "armedBy": w.armed_by(),
            "state": if watches.firing() == Some(w.number()) { "running" } else { "watching" }
        })).collect::<Vec<_>>(), "busy": open.running.is_some()}),
        )
    }

    /// Called by the desktop's clock, never a renderer-supplied generated prompt.
    fn poll_watches(&mut self) {
        self.poll_watches_at(std::time::Instant::now());
    }

    fn poll_watches_at(&mut self, now: std::time::Instant) {
        let handles: Vec<_> = self.open.keys().cloned().collect();
        for handle in handles {
            self.reap(&handle);
            let Some(open) = self.open.get(&handle) else {
                continue;
            };
            if open.running.is_some() || !open.answered_trust {
                continue;
            }
            if open.watches.lock().map(|w| w.is_empty()).unwrap_or(true) {
                continue;
            }
            let Ok(mut workspace) = Workspace::new(open.project.clone()) else {
                if let Ok(mut watches) = open.watches.lock() {
                    for watch in watches.live() {
                        self.emitter.send(Event::new(
                            "watch.ended",
                            &handle,
                            json!({"number": watch.number(), "reason": "project-unavailable"}),
                        ));
                    }
                    watches.stop_all();
                }
                continue;
            };
            if let Ok(state) = open.state.try_lock() {
                for directory in &state.directories {
                    let _ = workspace.add_directory(&directory.display().to_string());
                }
            }
            let due = {
                let Ok(mut watches) = open.watches.lock() else {
                    continue;
                };
                let ended = watches.look(now, |path, under| workspace.look(path, under));
                for (number, reason) in ended {
                    self.emitter.send(Event::new("watch.ended", &handle, json!({"number": number, "reason": match reason {
                        bravebot_agent::watch::Reaped::Aged => "expired", bravebot_agent::watch::Reaped::OutOfReach => "out-of-reach",
                    }})));
                }
                watches
                    .due(now)
                    .and_then(|w| Some((w.number(), w.path().to_string(), w.change()?)))
            };
            let Some((number, path, change)) = due else {
                continue;
            };
            let prompt = bravebot_agent::watch::fired(number, &path, change);
            // Announce the cause before starting a worker, so early events follow it.
            self.emitter.send(Event::new(
                "watch.fired",
                &handle,
                json!({"number": number, "path": path}),
            ));
            if let Ok(mut watches) = open.watches.lock() {
                watches.dispatched(number);
            }
            let request = Request::parse(&json!({"id": 0, "method": "turn.send", "params": {"session": handle, "prompt": prompt, "recall": false}}).to_string());
            if let Ok(request) = request
                && let Err(error) = self.start_turn(
                    &request,
                    Some(bravebot_agent::conversation::Composed::Watch {
                        number,
                        path: path.clone(),
                    }),
                )
            {
                if let Some(open) = self.open.get(&handle)
                    && let Ok(mut watches) = open.watches.lock()
                {
                    watches.stop(number);
                }
                self.emitter.send(Event::new(
                    "watch.ended",
                    &handle,
                    json!({"number": number, "reason": "failed", "message": error.message}),
                ));
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
            let handle = Handle::begin(project, crate::FRONT, crate::agent_build());
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

/// The files a turn is told a person dropped, each one accounted for as far as a string can be.
///
/// The same shape as `files`, and a different promise. A named file is workspace-relative and the
/// agent reads it inside the project; a dropped one may sit anywhere, because the path came from a
/// gesture rather than from anything a model said. That is what carries a bot's briefing, which
/// lives beside this app's own settings and deliberately not inside the checkout the planner may
/// write to.
///
/// DROP-1 puts the justification for that reach at the call site, and this is the second one: the
/// terminal's drop handling is the other. A front end is a separate process, so the gesture is not
/// visible from here and DROP-10 says what its caller owes. What this decides is the half of a
/// drop that leaves a trace on the disk: the path names a file that is there, and names it
/// absolutely, because an operating system reports a drop that way and because `files` beside it
/// is the parameter for a path inside the project.
///
/// A turn naming anything else is refused rather than run without it, for the reason the front end
/// refuses one whose briefing it could not write: a turn that lost the file it was sent with is
/// not a smaller turn, it is one that reports success for work it could not do.
///
/// An entry that is not a string, and a `dropped` that is not a list at all, are left out rather
/// than refused. Those are a front end built against a different version of this protocol, which
/// is the case the leniency exists for.
fn dropped_paths(request: &Request) -> Result<Vec<String>, Failure> {
    let Some(entries) = request.params.get("dropped").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    for named in entries.iter().filter_map(Value::as_str) {
        let path = std::path::Path::new(named);
        if !path.is_absolute() {
            return Err(Failure::bad_request(format!(
                "a dropped file is named by an absolute path, and {named} is not one; \
                 a path inside the project belongs in `files`"
            )));
        }
        if path.is_dir() {
            return Err(Failure::bad_request(format!(
                "{named} is a directory, and dropping one attaches nothing"
            )));
        }
        if !path.is_file() {
            return Err(Failure::bad_request(format!(
                "{named} names no file to drop"
            )));
        }
        paths.push(named.to_string());
    }
    Ok(paths)
}

/// The workspace one turn runs on, under the caps the settings in force put a search under.
///
/// SEARCH-9 has the caps handed to the workspace by whoever read the settings, because a
/// workspace that read them itself would answer differently on a machine whose owner had
/// configured them. That makes this the whole of the clause for this front end: a cap named in a
/// settings file reaches a search here or nowhere. A cap nobody named is `None`, which leaves the
/// built-in one standing.
///
/// Whether the file tools are held to the project comes over the same way (PERM-16), and from the
/// managed file as well as the settings layers: both are layers that may ask for it, and the
/// strictest thing either said is what holds (PERM-18).
///
/// A function of its own rather than three lines at the call site, so a test can build the
/// workspace a turn is given without a backend to run one against.
pub fn turn_workspace(
    project: PathBuf,
    settings: &Settings,
    managed: &bravebot_config::Managed,
) -> Result<Workspace, WorkspaceError> {
    let caps = settings.search();
    let inside = settings
        .narrowing()
        .strictest(managed.narrowing())
        .keeps_reads_in_the_workspace();
    Ok(Workspace::new(project)?
        .with_search_caps(caps.files, caps.time)
        .with_reads_kept_inside(inside))
}

/// The workspace a turn in this session reads through, as the session stands now.
///
/// A workspace is built per turn and opens the project only, so the directories a resumed session
/// had open are opened again here. The rules about them came back with the trust map, and a rule
/// about a directory nothing can open refuses every path under it for escaping the workspace, with
/// nothing on screen to say why. One that has since moved or been deleted cannot be reopened and
/// is left closed: the refusal it causes is the one that was already happening, and this protocol
/// has no way to say so outside a turn.
///
/// The session's own directory outside the project, made as the session opened, is set by the
/// code that holds it, so a turn cannot widen its own reach (TRUST-14).
fn session_workspace(open: &Open, settings: &Settings) -> Result<Workspace, Failure> {
    let mut workspace = turn_workspace(
        open.project.clone(),
        settings,
        &bravebot_config::Managed::load(),
    )
    .map_err(|error| Failure::new(ErrorCode::Internal, error.to_string()))?;
    let (directories, scratch) = open
        .state
        .lock()
        .map(|s| {
            (
                s.directories.clone(),
                s.scratch.path().map(Path::to_path_buf),
            )
        })
        .unwrap_or((Vec::new(), None));
    workspace.open_scratch(scratch);
    for directory in &directories {
        let _ = workspace.add_directory(&directory.display().to_string());
    }
    Ok(workspace)
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
    /// What the settings say a commit message and a pull request this turn writes may carry.
    attribution: bravebot_config::Attribution,
    /// What the settings say a command's output may spend of this turn's conversation, where they
    /// said anything (RUN-21). Read where the attribution was and carried the same way.
    output_cap: Option<usize>,
    /// How long a command this turn runs may take, and the most one call may ask for (RUN-23). Read
    /// where the cap was and carried the same way.
    deadlines: bravebot_agent::exec::Deadlines,
    /// The session's, settled when it opened.
    auto_vetting: bool,
    /// The session's as it stood when the turn was accepted.
    permission_mode: PermissionMode,
    /// Each MCP server the settings request, with the file that requested it.
    mcp_requested: Vec<(PathBuf, String)>,
    watches: Arc<Mutex<bravebot_agent::watch::Watches>>,
    model: Option<String>,
    /// The definition this turn is addressed to, where it is a turn in a bot's conversation
    /// (MEMORY-10).
    addressing: Option<String>,
    workspace: Workspace,
    prompt: String,
    /// What this prompt was composed for, where nobody typed it.
    composed: Option<bravebot_agent::conversation::Composed>,
    files: Vec<String>,
    dropped: Vec<String>,
    /// Pictures and PDFs a person dropped, each with the type its extension names.
    attachments: Vec<(String, &'static str)>,
    /// Pictures a person pasted.
    images: Vec<bravebot_agent::turn::PastedImage>,
    /// Whether this prompt joins the shared recall history, and may name the session.
    recall: bool,
    turn: usize,
    cancel: Cancel,
    pending: crate::turn::Pending,
    answers: mpsc::Receiver<crate::turn::Reply>,
    finished: Arc<std::sync::atomic::AtomicBool>,
}

/// Where the prompt a turn carried landed among the things the user said.
///
/// The coordinate `session.fork` cuts on, answered by the side that assigns it. A window cannot
/// count it: a turn nudged for spending its tool budget adds a user message of its own, and no
/// event tells a window about one. A window counting its own bubbles is short by one for each,
/// and `session.fork` checks the text against the ordinal and refuses rather than cutting in the
/// wrong place.
///
/// The last message carrying this text, because that is the one this turn has just added.
fn prompt_ordinal(conversation: &bravebot_agent::Conversation, prompt: &str) -> Option<usize> {
    crate::fork::prompts(&conversation.recounted())
        .iter()
        .rposition(|said| *said == prompt)
}

/// The points `state` can rewind to, newest first, as a window offers them.
///
/// `prompt` is the ordinal `session.fork` uses, found after the prompts the point's snapshot
/// already held, so a prompt asked twice anchors on the turn that asked it. `null` where the
/// conversation no longer holds it.
fn rewind_json(state: &State, project: &Path) -> Vec<Value> {
    let recounted = state.conversation.recounted();
    let prompts = crate::fork::prompts(&recounted);
    state
        .rewind
        .points()
        .iter()
        .rev()
        .enumerate()
        .map(|(back, point)| {
            let before =
                bravebot_agent::Conversation::restored(point.snapshot.conversation.clone())
                    .recounted();
            let before = crate::fork::prompts(&before).len();
            let ordinal = prompts
                .iter()
                .skip(before)
                .position(|said| *said == point.prompt)
                .map(|at| at + before);
            json!({
                "steps": back + 1,
                "turn": point.snapshot.turns + 1,
                "prompt": ordinal,
                "text": point.prompt,
                "paths": point
                    .backups
                    .iter()
                    .map(|backup| shown_path(&backup.path, project))
                    .collect::<Vec<_>>(),
                "gaps": point.coverage.gaps(),
            })
        })
        .collect()
}

/// A path as a person reads it: under the project where it is, whole where it is not.
///
/// Backups are named under the workspace root, which is the project with its links resolved.
fn shown_path(path: &Path, project: &Path) -> String {
    let resolved = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    path.strip_prefix(&resolved)
        .or_else(|_| path.strip_prefix(project))
        .unwrap_or(path)
        .display()
        .to_string()
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
        attribution,
        output_cap,
        deadlines,
        auto_vetting,
        permission_mode,
        mcp_requested,
        watches,
        model,
        addressing,
        workspace,
        prompt,
        composed,
        files,
        dropped,
        attachments,
        images,
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

    let history =
        bravebot_session::store::Entry::sent(&prompt, Some(project.display().to_string()));
    let mut confirmer = BridgeConfirmer::new(
        emitter.clone(),
        &session,
        pending,
        answers,
        Arc::clone(&state.question_ids),
        cancel.clone(),
    );

    // The session's MCP servers (SERVERS-9), started by its first turn and held until it closes.
    // Here rather than when the session opened, so that the questions about them come after the
    // person has said whether the directory is trusted, and reach the window through this turn.
    if matches!(state.mcp, Mcp::Unstarted)
        && let Some(started) = start_mcp(
            &emitter,
            &session,
            &project,
            &mcp_requested,
            &mut confirmer,
            &cancel,
        )
    {
        state.mcp = Mcp::Started(started);
    }
    let mcp = match &state.mcp {
        Mcp::Started(started) => started.clone(),
        Mcp::Unstarted => None,
    };
    let mut task = Task::new(&prompt)
        .already_asked_about(state.asked_about.clone())
        .already_exposed(state.exposed.clone())
        .with_home(bravebot_agent::home::directory())
        // What a leading `~` in a run line stands for, which is the home directory and not the
        // state directory above (CMDLINE-4). Without it the line is refused as having no home.
        .with_profile(bravebot_agent::home::profile())
        .with_cache(bravebot_agent::home::cache())
        // No bound on the rounds, as the terminal passes: there is a person in front of this
        // window, they see what the turn is doing, and `turn.cancel` reaches it mid-round. A
        // number would only interrupt work that was going fine (TURN-2).
        .with_rounds(None)
        .with_model(model)
        .with_attribution(attribution)
        .with_output_cap(output_cap)
        .with_deadlines(deadlines)
        .with_confined_runs(true)
        .with_auto_vetting(auto_vetting)
        .with_permission_mode(permission_mode)
        // The rules the session opened under, and not the files as they are now (PERM-12).
        .with_permissions(state.rules.permissions.clone())
        .with_mcp(mcp)
        // A turn in a bot's conversation is addressed to the bot's definition whatever composed
        // it (MEMORY-10). The name is the session's, set from the bot's row, so nothing the turn
        // produced chooses it; the kernel compares it against the set this session resolved.
        .addressing(addressing);
    if let Some(composed) = composed {
        task = task.composed_rather_than_typed(composed);
    }
    for file in &files {
        task = task.with_file(file);
    }
    for path in &dropped {
        task = task.with_dropped_text(path);
    }
    for (path, media) in attachments {
        task = task.with_attachment(path, media);
    }
    for image in images {
        task = task.with_image(image);
    }

    let free = watches
        .lock()
        .map(|w| bravebot_agent::watch::MAX_LIVE.saturating_sub(w.live().len()))
        .unwrap_or(0);
    task = task.arming(if free == 0 {
        bravebot_agent::watch::Arming::Full
    } else {
        bravebot_agent::watch::Arming::Allowed { free }
    });
    let mut reporter = BridgeReporter::new(emitter.clone(), &session);
    let mut sink = BridgeSink::new(emitter.clone(), &session, turn);
    let egress = Egress::new();

    // Cloned out before the call, because the conversation is borrowed mutably for the
    // duration and both of these are passed by value.
    let trust = state.trust.clone();
    let file_authority = bravebot_core::file_authority::FileAuthority::new(trust.clone());
    let task = task.with_file_authority(file_authority.clone());
    let programs = state.programs.clone();
    // The session's language servers (LSP-8), taken for this turn and put back after it. The
    // first turn builds the set. Building it starts no server: one starts on the first question
    // that needs it, after the person approves.
    let mut servers = state.servers.take().unwrap_or_else(|| {
        // Use the task's home so the index is cached with the rest of the session's state.
        bravebot_agent::lsp::LanguageServers::new(workspace.root().to_path_buf(), task.home.clone())
    });
    // Where this turn begins, in the recounted conversation the record's boundaries are offsets into.
    let begins = state.conversation.recounted().len();
    // Wrapped here and not before the servers start. Their questions go through `servers::Asker`,
    // which no mode a window can choose answers: only bypassing does (SERVERS-13), and a window
    // cannot reach it. The screening value is the task's, so the confirmer and the tools that fill
    // in a verdict read the same answer.
    let mut confirmer = Confining::new(&mut confirmer, permission_mode, task.auto_vetting);
    open_rewind_point(&mut state, &workspace, &prompt);
    let completed = agent_turn::resume(
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
        Some(&mut servers),
        &cancel,
    );
    // Put the set back even if the turn failed or was cancelled. Otherwise its servers would
    // stop here, and the next turn would ask about the same language again.
    state.servers = Some(servers);
    // Kept whatever the outcome: a failed or cancelled turn may still have written.
    state.rewind.keep_backups(workspace.take_backups());

    // Cleanup has finished on every return, including cancellation and request errors.
    // Taken apart with no `..`, so an answer a turn learns to remember does not build until this
    // front-end says what becomes of it.
    let agent_turn::Decisions {
        trust,
        programs,
        asked_about,
        exposed,
    } = completed.decisions;
    state.trust = trust;
    state.programs = programs;
    state.asked_about = asked_about;
    state.exposed = exposed;
    let outcome = completed.outcome;

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
    let ended = match &outcome {
        Ok(_) => bravebot_session::sessions::StoredOutcome::Completed,
        Err(error) => {
            bravebot_session::sessions::StoredOutcome::ended(turn, error.ending(), error.cut_off())
        }
    };
    record_turn(
        &mut state,
        turn,
        &prompt,
        begins,
        reporter.prompt_at(),
        ended,
    );
    if state.first_prompt.is_none() && recall {
        state.first_prompt = Some(prompt.clone());
    }

    let event = match outcome {
        Ok(outcome) => {
            if let Ok(mut watches) = watches.lock() {
                for path in &outcome.watches {
                    if !watches.live().iter().any(|w| w.path() == path) {
                        let _ = watches.arm(
                            path.clone(),
                            workspace.root().to_path_buf(),
                            turn,
                            workspace.look(path, workspace.root()),
                            std::time::Instant::now(),
                        );
                    }
                }
            }
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

            Event::new(
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
                    // Where this turn's prompt landed among the things the user said, which is
                    // the coordinate `session.fork` cuts on. `null` where the conversation does
                    // not hold it, which is a prompt that cannot be forked rather than one to
                    // guess a place for.
                    "prompt": prompt_ordinal(&state.conversation, &prompt),
                    // The turns this session can now be put back to (SESSION-19).
                    "rewind": rewind_json(&state, &project),
                }),
            )
        }
        Err(error) => {
            let _ = save(&project, &mut state, turn, sink.trail());

            let chosen = task.model.as_deref().unwrap_or(&config.default_model);
            let mut data = failure_fields(&error, &config, chosen);
            merge(
                &mut data,
                json!({ "turn": turn,
                    "cutOff": wire::cut_off(error.cut_off()),
                    "contextTokens": state.conversation.last_request_tokens(),
                    // What the turn said about itself before it failed, as `turn.done` carries for a
                    // turn that answered. There is no outcome here to take them from, and a hook
                    // that could not be started is the person's own to hear about (HOOK-7).
                    "notices": reporter.notices(),
                    // As on `turn.done`. A turn that failed still said what it was asked, so the
                    // prompt is in the conversation and is still a place a fork can be cut at.
                    "prompt": prompt_ordinal(&state.conversation, &prompt),
                    "rewind": rewind_json(&state, &project),
                    "id": state.handle.as_ref().map(|handle| handle.id()) }),
            );
            Event::new("turn.error", &session, data)
        }
    };

    // Finish bookkeeping and release the session before inviting the next request.
    if let Ok(mut watches) = watches.lock() {
        watches.turn_ended(std::time::Instant::now());
    }
    drop(state);
    emitter.finish(event, &finished);
}

/// Start the MCP servers `requested` names, putting the questions about them to the window.
///
/// The terminal's own road (SERVERS-4, SERVERS-10, SERVERS-11, SERVERS-12): each request resolves
/// against the person's declarations, one the managed layer refuses goes no further, the rest are
/// asked about where no answer of theirs covers them, and each is started confined or reached
/// through the egress gate. A server's stderr is discarded, as the full-screen terminal discards
/// it, because this process has no screen of its own to put it on.
///
/// `mcp.starting` goes out first, since a server has up to a minute to answer its handshake and
/// the turn says nothing else while it waits, and `mcp.started` after, naming what started and
/// saying why each other request did not. `None` where the turn was stopped part way, which
/// starts nothing and leaves the servers to the next turn, so a stop is not read as a no.
fn start_mcp(
    emitter: &Emitter,
    session: &str,
    project: &Path,
    requested: &[(PathBuf, String)],
    confirmer: &mut BridgeConfirmer,
    cancel: &Cancel,
) -> Option<Option<bravebot_agent::mcp::Session>> {
    if requested.is_empty() {
        return Some(None);
    }
    let aliases: Vec<&str> = requested.iter().map(|(_, alias)| alias.as_str()).collect();
    emitter.send(Event::new(
        "mcp.starting",
        session,
        json!({ "servers": aliases }),
    ));
    let home = bravebot_agent::servers::Home {
        directory: bravebot_agent::home::directory(),
        writable: bravebot_agent::home::writable().is_some(),
    };
    let project = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    let reached = bravebot_agent::servers::reach(
        requested,
        &project,
        &home,
        &bravebot_config::Managed::load(),
        bravebot_agent::servers::Asking::Person,
        confirmer,
        bravebot_sandbox::Stream::Null,
    );
    if cancel.is_cancelled() {
        return None;
    }
    emitter.send(Event::new(
        "mcp.started",
        session,
        json!({
            "servers": reached.aliases(),
            "confined": reached.confined(),
            "notes": reached.notes,
        }),
    ));
    Some(reached.session())
}

/// How a failure is reported to a front end: `kind`, `message`, `category`, `attempts`, `status`.
///
/// The category and the counts come from the agent's diagnosis. Raw backend text is not sent.
/// `chosen` is the model that was asked for, which decides one category below.
pub(crate) fn failure_fields(error: &TurnError, config: &Config, chosen: &str) -> Value {
    let ending = error.ending();
    let diagnosis = ending.diagnosis();
    let category = diagnosis.map(|d| d.category.name());
    // A configured gateway does not serve the built-in Brave default. Keep this
    // distinct from a missing token: re-entering the gateway key cannot fix routing.
    let category = if category == Some("unconfigured")
        && (!config.providers.is_empty() || config.bedrock.is_some())
        && !config.serves_aichat()
        && config.provider_for(chosen).is_none()
        && config.bedrock_for(chosen).is_none()
    {
        Some("model-unconfigured")
    } else {
        category
    };
    let attempts = match ending {
        bravebot_agent::outcome::Ending::Stopped { attempts } => attempts,
        _ => diagnosis.and_then(|d| d.attempts),
    };
    let kind = match error {
        TurnError::Cancelled { .. } => "cancelled",
        TurnError::Precommit(_) => "precommit",
        TurnError::Workspace(_) => "workspace",
        TurnError::Chat(_) => "chat",
        // Only a manifest run fails this way, and `crate::manifest` reports the cause inside
        // it, so a turn never sends this kind.
        TurnError::Manifest { .. } => "manifest",
    };
    json!({ "kind": kind, "message": category.unwrap_or("cancelled"), "category": category,
        "attempts": attempts, "status": diagnosis.and_then(|d| d.status) })
}

/// Open the point the turn about to run can be rewound to (SESSION-19), as the terminal does.
///
/// The workspace is built per turn, so a language server an earlier turn started is not on it.
/// The record remembers one, and the warning is put back before the point binds to it.
fn open_rewind_point(state: &mut State, workspace: &Workspace, prompt: &str) {
    use bravebot_agent::rewind::CoverageGap;
    if state
        .handle
        .as_ref()
        .is_some_and(Handle::server_children_may_run)
    {
        workspace.mark_rewind_gap(CoverageGap::LanguageServer);
    }
    let snapshot = bravebot_session::sessions::TurnSnapshot {
        conversation: state.conversation.snapshot(),
        turns: state.turns,
        tokens: state.tokens,
        spend: state.spend.clone(),
        timing: state.timing.clone(),
        cached: None,
        cached_prompt_tokens: None,
        // With the record's rules, so a yes this turn gives to a recorded memory does not outlive
        // rewinding past it.
        trust: bravebot_agent::memory::with_recorded(
            &state.trust,
            workspace,
            bravebot_agent::home::directory().as_deref(),
        ),
        programs: state.programs.clone(),
        // A terminal index the desktop has no transcript for. Not stored; a terminal resuming the
        // record places each point again.
        transcript_len: 0,
        title: state
            .handle
            .as_ref()
            .map(|handle| handle.title().to_string())
            .unwrap_or_default(),
        was_wrote: state
            .handle
            .as_ref()
            .is_some_and(|handle| handle.resumable().is_some()),
    };
    state.rewind.open(snapshot, prompt.to_string());
    state.rewind.bind_coverage(workspace);
}

/// Add the turn that just ran to the session's history, if it keeps one (SESSION-23).
///
/// `begins` is the length of the recounted conversation before the turn, and `prompt_at` where the
/// submitted prompt entered it. A turn that lost the conversation ends before it began, and then
/// its range restarts at nothing, as the terminal records it.
fn record_turn(
    state: &mut State,
    number: usize,
    prompt: &str,
    begins: usize,
    prompt_at: Option<usize>,
    outcome: bravebot_session::sessions::StoredOutcome,
) {
    let end = state.conversation.recounted().len();
    let Some(history) = state.history.as_mut() else {
        return;
    };
    let reset_context = end < begins;
    history.push(bravebot_session::sessions::StoredTurn {
        number,
        prompt: Some(prompt.to_string()),
        start: if reset_context { 0 } else { begins },
        end,
        reset_context,
        prompt_offset: prompt_at
            .filter(|at| !reset_context && *at >= begins && *at < end)
            .map(|at| at - begins),
        outcome: Some(outcome),
    });
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
        .get_or_insert_with(|| Handle::begin(project, crate::FRONT, crate::agent_build()));

    let first = state.first_prompt.clone().unwrap_or_default();
    // Taken once and lent to both readers below. A snapshot copies the whole conversation, and
    // the archive count wanted for `turn.done` is a field of the one being written down anyway —
    // asking for a second copy to read one number off it would double the cost of every turn.
    let snapshot = state.conversation.snapshot();
    let archived = snapshot.archive.len();
    handle.save(
        &first,
        Standing {
            history: state.history.as_deref(),
            conversation: &snapshot,
            turns: state.turns,
            tokens: state.tokens,
            spend: &state.spend,
            timing: &state.timing,
            model: state.model.as_deref(),
            todos: &state.todos,
            asides: &state.asides,
            rewind: state.rewind.points(),
            trust: &state.trust,
            programs: &state.programs,
            directories: &state.directories,
            // Every session this window writes is a turn session. `Standing` carries the
            // manifest so the picker can mark a run that may be read and not continued, and
            // marking one of ours would be a claim about a session nobody can resume.
            manifest: None,
            // The desktop lists, resumes and applies no checkout yet, so it holds none itself;
            // the ones a resumed record carries are written back by `Handle` (CHECKOUT-16).
            checkouts: &[],
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
/// The same decisions the record is written with, so a rule reads the same whether it came off
/// disk, out of a finished turn, or out of the session a fork inherited it from.
pub(crate) fn rules_json(trust: &TrustStore) -> Vec<Value> {
    trust
        .rules()
        .map(|(path, integrity)| {
            let integrity = match integrity {
                Some(bravebot_core::label::Integrity::Trusted) => "trusted",
                Some(bravebot_core::label::Integrity::Untrusted) => "untrusted",
                None => "undecided",
            };
            json!({ "path": path, "integrity": integrity })
        })
        .collect()
}

/// How a session's trust stands as it opens.
struct Opening {
    /// The map it starts with. Empty where nobody has answered, and no turn runs on it then.
    trust: TrustStore,
    /// Whether anything has answered: what the session carries, or a kept answer.
    answered: bool,
    /// The kept answer that settled it, and the file it is kept in.
    remembered: Option<(trusted::Kept, PathBuf)>,
    /// Where the question is put, the record a "trust and remember" answer would be kept in.
    keeping: Option<(trusted::Store, trusted::Identity)>,
}

impl Opening {
    /// A session in `project` carrying `carried`, the map its own record kept or its parent held,
    /// or `None` where it carries none.
    ///
    /// What it carries answers first, since that is the answer this conversation's own user gave.
    /// Then an answer the person said to remember about this exact directory (TRUST-23), which starts
    /// the session from the rule a yes writes and nothing else. Then the question, offering to keep
    /// its answer only where it may be written and later honoured.
    fn of(project: &Path, carried: Option<TrustStore>) -> Self {
        if let Some(trust) = carried {
            return Self {
                trust,
                answered: true,
                remembered: None,
                keeping: None,
            };
        }
        let record = remembering(project);
        let kept = record.as_ref().and_then(|(store, identity)| {
            Some((store.kept(identity)?, store.path().to_path_buf()))
        });
        let mut trust = bravebot_agent::workspace::trust_store(project);
        match kept {
            Some(remembered) => {
                trust.trust(".");
                Self {
                    trust,
                    answered: true,
                    remembered: Some(remembered),
                    keeping: None,
                }
            }
            // An incognito session reads the record and adds nothing to it, so it offers nothing.
            None => Self {
                trust,
                answered: false,
                remembered: None,
                keeping: record.filter(|_| trusted::may_be_written()),
            },
        }
    }

    /// What the response opening the session adds about it, for the window to say in turn.
    fn json(&self) -> Value {
        json!({
            "remembered": self.remembered.as_ref().map(|(kept, path)| kept_json(kept, path)),
            "keeping": self.keeping.as_ref().map(|(store, _)| store.path().display().to_string()),
        })
    }
}

/// Add the fields of the object `extra` to the object `into`.
pub(crate) fn merge(into: &mut Value, extra: Value) {
    if let (Some(into), Value::Object(extra)) = (into.as_object_mut(), extra) {
        into.extend(extra);
    }
}

/// The record of the answer kept about `project` (TRUST-23), or `None` where none may be kept or
/// honoured.
///
/// Read under the name a turn there resolves the directory to, which is the name the terminal
/// reads it under, so one directory has one record whichever front end answered.
fn remembering(project: &Path) -> Option<(trusted::Store, trusted::Identity)> {
    trusted::record_for(
        bravebot_agent::home::directory().as_deref(),
        bravebot_agent::home::profile().as_deref(),
        &project.canonicalize().ok()?,
    )
}

/// A kept answer as a window is told about it: when it was given, and the file it is kept in.
fn kept_json(kept: &trusted::Kept, path: &Path) -> Value {
    json!({ "at": kept.at, "path": path.display().to_string() })
}

/// The answer kept about `project` as the record says now, or null where none answers.
fn remembered_json(project: &Path) -> Value {
    remembering(project)
        .and_then(|(store, identity)| Some(kept_json(&store.kept(&identity)?, store.path())))
        .unwrap_or(Value::Null)
}

/// Withdraw every answer kept about `project` (TRUST-24), as the terminal's `/forget-trust` does.
///
/// Whichever directory was at the path when each was given: a person taking the answer back means
/// the name. A failure names the file, so the person can remove the line by hand.
fn forget_trust(project: &Path) -> Result<(), Failure> {
    let nothing = || Failure::bad_request("No answer about this directory is kept any longer.");
    let home = bravebot_agent::home::directory().ok_or_else(nothing)?;
    let root = project.canonicalize().map_err(|_| nothing())?;
    let store = trusted::Store::new(&home, &root);
    match store.forget() {
        Ok(true) => Ok(()),
        Ok(false) => Err(nothing()),
        Err(error) => Err(Failure::new(
            ErrorCode::NoHome,
            format!(
                "The answer kept in {} could not be removed: {error}",
                store.path().display()
            ),
        )),
    }
}

/// Seconds since the epoch, which is how a kept answer says when it was given.
fn seconds_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

#[cfg(test)]
mod coverage_tests {
    use super::*;

    /// A command approval revoked after a turn stays revoked when that turn is undone, though the
    /// point the turn opened was taken while the approval held.
    #[test]
    fn undoing_a_turn_does_not_hand_back_a_revoked_command() {
        if !test_profile::in_isolated_profile() {
            return;
        }
        let directory = test_profile::project("rewind-revoked-command");
        std::fs::create_dir_all(&directory).unwrap();
        let command = bravebot_core::programs::Command::new("/bin/ls", Vec::new(), &directory);
        let mut state = State::fresh(TrustStore::new(&directory));
        state.programs.trust(command.clone());
        let workspace = Workspace::new(&directory).unwrap();
        open_rewind_point(&mut state, &workspace, "list the files");
        let mut bridge = Bridge::new(Box::new(|_| {}));
        let handle = bridge.mint(Open {
            project: directory.clone(),
            state: Arc::new(Mutex::new(state)),
            answered_trust: true,
            keeping: None,
            running: None,
            model: None,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            auto_vetting: false,
            definition: None,
            permission_mode: PermissionMode::Ask,
        });
        let call = |bridge: &mut Bridge, method: &str, params: Value| {
            let line = json!({"id": 1, "method": method, "params": params}).to_string();
            bridge.dispatch(&Request::parse(&line).unwrap()).unwrap()
        };
        call(
            &mut bridge,
            "permissions.revoke",
            json!({"session": handle, "kind": "command", "command": {
                "program": command.program, "startedAs": command.started_as, "args": command.args,
            }}),
        );

        call(
            &mut bridge,
            "session.rewind",
            json!({"session": handle, "steps": 1}),
        );

        let listed = call(&mut bridge, "permissions.list", json!({"session": handle}));
        assert_eq!(
            listed["commands"],
            json!([]),
            "the revoked command came back"
        );
    }

    /// A turn's workspace is new, so the server warning the record kept has to reach the point a
    /// later turn opens, even once every earlier point has been rewound past.
    #[test]
    fn a_desktop_turn_after_the_last_point_keeps_the_server_warning() {
        use bravebot_agent::rewind::CoverageGap;
        if !test_profile::in_isolated_profile() {
            return;
        }
        let directory = test_profile::project("desktop-point-coverage");
        std::fs::create_dir_all(&directory).unwrap();
        let mut state = State::fresh(TrustStore::new(&directory));
        state.handle = Some(Handle::begin(
            &directory,
            crate::FRONT,
            crate::agent_build(),
        ));

        let first = Workspace::new(&directory).unwrap();
        open_rewind_point(&mut state, &first, "start a server");
        state.rewind.record_gap(CoverageGap::LanguageServer);
        let point = state.rewind.take(1).unwrap();
        state
            .handle
            .as_mut()
            .unwrap()
            .retain_rewind_coverage(&point.coverage);

        let second = Workspace::new(&directory).unwrap();
        open_rewind_point(&mut state, &second, "after the rewind");

        assert_eq!(
            state.rewind.points()[0].coverage.gaps(),
            [CoverageGap::LanguageServer].into()
        );
    }

    /// A fork cannot prove that server descendants from the parent stopped writing.
    #[test]
    fn a_desktop_fork_keeps_the_server_warning_without_checkpoints() {
        if !test_profile::in_isolated_profile() {
            return;
        }
        let directory = test_profile::project("coverage");
        std::fs::create_dir_all(&directory).unwrap();
        let root = directory.as_path();
        let mut conversation = bravebot_agent::Conversation::new();
        conversation.push(bravebot_aichat::protocol::Message::user("work"));
        let record: Record = serde_json::from_value(json!({
            "id": "server-parent", "directory": root, "title": "work",
            "started": 1, "updated": 1, "server_children_may_run": true,
            "conversation": conversation.snapshot()
        }))
        .unwrap();
        let mut bridge = Bridge::new(Box::new(|_| {}));
        let parent = bridge.mint(Open {
            project: root.to_path_buf(),
            state: Arc::new(Mutex::new(State::resumed(
                root,
                &record,
                TrustStore::new(root),
            ))),
            answered_trust: true,
            keeping: None,
            running: None,
            model: None,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            auto_vetting: false,
            definition: None,
            permission_mode: PermissionMode::Ask,
        });
        let request = Request::parse(
            &json!({"id": 1, "method": "session.fork", "params": {
                "session": parent, "prompt": 0, "text": "work"
            }})
            .to_string(),
        )
        .unwrap();
        let forked = bridge.dispatch(&request).unwrap();
        let child = bridge
            .open
            .get(forked["session"].as_str().unwrap())
            .unwrap();
        let mut state = child.state.lock().unwrap();
        assert!(state.rewind.is_empty());
        save(
            root,
            &mut state,
            0,
            &bravebot_session::audit::Trail::default(),
        );
        let loaded =
            bravebot_session::sessions::load(root, forked["id"].as_str().unwrap()).unwrap();
        assert!(loaded.rewind.is_empty());
        assert!(loaded.server_children_may_run());
        state.handle.as_mut().unwrap().discard_unwritten("");
    }

    /// Switching front ends must not drop the warning once undo has consumed every checkpoint.
    #[test]
    fn a_resumed_server_warning_survives_a_desktop_save_without_checkpoints() {
        if !test_profile::in_isolated_profile() {
            return;
        }
        let directory = test_profile::project("coverage");
        std::fs::create_dir_all(&directory).unwrap();
        let root = directory.as_path();
        let record: Record = serde_json::from_value(json!({
            "id": "server-warning", "directory": root, "title": "work",
            "started": 1, "updated": 1, "server_children_may_run": true,
            "conversation": bravebot_agent::Conversation::new().snapshot()
        }))
        .unwrap();
        let mut state = State::resumed(root, &record, TrustStore::new(root));
        assert!(state.rewind.is_empty());
        save(
            root,
            &mut state,
            0,
            &bravebot_session::audit::Trail::default(),
        );
        let loaded = bravebot_session::sessions::load(root, &record.id).unwrap();
        assert!(loaded.rewind.is_empty());
        assert!(loaded.server_children_may_run());
        state.handle.as_mut().unwrap().discard_unwritten("");
    }
}

#[cfg(test)]
mod permissions_tests {
    use super::*;

    /// A command grant is picked by every path in its key, so revoking the one started through a
    /// link leaves the one started by the file it leads to.
    #[test]
    fn revoking_a_command_picks_it_by_the_path_that_starts_it() {
        let file =
            bravebot_core::programs::Command::new("/usr/bin/python3", vec!["-V".into()], "/work");
        let link = file.clone().started_as("/work/.venv/bin/python");
        let mut state = State::fresh(TrustStore::new("/work"));
        state.programs = bravebot_core::programs::TrustedPrograms::from_iter([file, link]);
        let mut bridge = Bridge::new(Box::new(|_| {}));
        let handle = bridge.mint(Open {
            project: "/work".into(),
            state: Arc::new(Mutex::new(state)),
            answered_trust: true,
            keeping: None,
            running: None,
            model: None,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            auto_vetting: false,
            definition: None,
            permission_mode: PermissionMode::Ask,
        });
        let revoke = Request::parse(
            &json!({"id": 1, "method": "permissions.revoke", "params": {
                "session": handle, "kind": "command", "command": {
                    "program": "/usr/bin/python3",
                    "startedAs": "/work/.venv/bin/python",
                    "args": ["-V"],
                }
            }})
            .to_string(),
        )
        .unwrap();
        let after = bridge.dispatch(&revoke).unwrap();
        assert_eq!(
            after["commands"].as_array().map(Vec::len),
            Some(1),
            "{after}"
        );
        assert_eq!(
            after["commands"][0]["startedAs"], "/usr/bin/python3",
            "{after}"
        );
    }

    #[test]
    fn an_undecided_boundary_is_visible_but_cannot_be_revoked() {
        let mut trust = TrustStore::new("/work");
        trust.distrust("vendor");
        trust.undecide("vendor/ours");
        let mut bridge = Bridge::new(Box::new(|_| {}));
        let handle = bridge.mint(Open {
            project: "/work".into(),
            state: Arc::new(Mutex::new(State::fresh(trust))),
            answered_trust: true,
            keeping: None,
            running: None,
            model: None,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            auto_vetting: false,
            definition: None,
            permission_mode: PermissionMode::Ask,
        });
        let list = Request::parse(
            &json!({"id": 1, "method": "permissions.list", "params": {"session": handle}})
                .to_string(),
        )
        .unwrap();
        let before = bridge.dispatch(&list).unwrap();
        assert_eq!(
            before["paths"],
            json!([
                {"path": "vendor", "integrity": "untrusted"},
                {"path": "vendor/ours", "integrity": "undecided"}
            ])
        );
        let revoke = Request::parse(
            &json!({"id": 2, "method": "permissions.revoke", "params": {
                "session": handle, "kind": "path", "path": "vendor/ours"
            }})
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            bridge.dispatch(&revoke).unwrap_err().code,
            ErrorCode::BadRequest
        );
        assert_eq!(bridge.dispatch(&list).unwrap(), before);
    }
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
        watches
            .arm(
                "watched".into(),
                workspace.root().to_path_buf(),
                1,
                workspace.look("watched", workspace.root()),
                now,
            )
            .unwrap();
        let watches = Arc::new(Mutex::new(watches));
        let handle = bridge.mint(Open {
            project: root.clone(),
            state: Arc::new(Mutex::new(State::fresh(TrustStore::new(&root)))),
            answered_trust: true,
            keeping: None,
            running: None,
            model: None,
            watches: Arc::clone(&watches),
            auto_vetting: false,
            definition: None,
            permission_mode: PermissionMode::Ask,
        });
        std::fs::write(
            root.join("watched"),
            "PRIVATE FILE CONTENT MUST NOT BE SENT",
        )
        .unwrap();
        bridge.poll_watches_at(now + Duration::from_secs(6));
        assert!(watches.lock().unwrap().is_empty());
        let events = events.lock().unwrap();
        assert_eq!(events.iter().filter(|e| e.name == "watch.fired").count(), 1);
        assert_eq!(events.last().unwrap().name, "watch.ended");
        assert!(events.iter().all(|e| e.session.as_deref() == Some(&handle)));
        assert!(
            !events
                .iter()
                .any(|e| e.data.to_string().contains("PRIVATE FILE"))
        );
    }

    /// Arms a watch on `watched` while it holds `before` (absent for `None`), applies `change`,
    /// polls past the first look and returns the names of the events sent.
    fn events_after(before: Option<&str>, change: impl FnOnce(&std::path::Path)) -> Vec<String> {
        let mut builder = tempfile::Builder::new();
        builder.prefix("bravebot-watch-presence-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().unwrap();
        let root = directory.path().to_path_buf();
        if let Some(text) = before {
            std::fs::write(root.join("watched"), text).unwrap();
        }
        let events = Arc::new(Mutex::new(Vec::new()));
        let held = Arc::clone(&events);
        let mut bridge = Bridge::new(Box::new(move |e| held.lock().unwrap().push(e)));
        // A removed override fails the turn before any model request, which leaves the fire
        // as the only thing to observe.
        bridge.settings = Some(root.join("missing.json"));
        let now = Instant::now();
        let mut watches = bravebot_agent::watch::Watches::new();
        let workspace = Workspace::new(root.clone()).unwrap();
        watches
            .arm(
                "watched".into(),
                workspace.root().to_path_buf(),
                1,
                workspace.look("watched", workspace.root()),
                now,
            )
            .unwrap();
        bridge.mint(Open {
            project: root.clone(),
            state: Arc::new(Mutex::new(State::fresh(TrustStore::new(&root)))),
            answered_trust: true,
            keeping: None,
            running: None,
            model: None,
            watches: Arc::new(Mutex::new(watches)),
            auto_vetting: false,
            definition: None,
            permission_mode: PermissionMode::Ask,
        });
        change(&root);
        bridge.poll_watches_at(now + Duration::from_secs(6));
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.name.to_string())
            .collect()
    }

    #[test]
    fn a_watch_armed_on_an_absent_path_fires_when_the_file_appears() {
        let names = events_after(None, |root| {
            std::fs::write(root.join("watched"), "new").unwrap();
        });
        assert_eq!(names.iter().filter(|n| *n == "watch.fired").count(), 1);
    }

    #[test]
    fn a_watch_armed_on_an_absent_path_stays_quiet_while_nothing_appears() {
        let names = events_after(None, |_| {});
        assert!(names.is_empty());
    }

    #[test]
    fn a_watch_fires_when_the_file_it_watches_is_removed() {
        let names = events_after(Some("old"), |root| {
            std::fs::remove_file(root.join("watched")).unwrap();
        });
        assert_eq!(names.iter().filter(|n| *n == "watch.fired").count(), 1);
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
        let first = watches
            .arm(
                "first".into(),
                project.clone(),
                1,
                bravebot_agent::watch::Looked::Saw("a".into()),
                now,
            )
            .unwrap();
        let second = watches
            .arm(
                "second".into(),
                project.clone(),
                1,
                bravebot_agent::watch::Looked::Saw("b".into()),
                now,
            )
            .unwrap();
        watches.dispatched(first);
        let watches = Arc::new(Mutex::new(watches));
        let cancel = Cancel::new();
        let (answers, _receiver) = mpsc::channel();
        let running = Running {
            cancel: cancel.clone(),
            answers,
            pending: Arc::new(Mutex::new(None)),
            turn: 1,
            finished: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let mut bridge = Bridge::new(Box::new(|_| {}));
        let handle = bridge.mint(Open {
            project: project.clone(),
            state: Arc::new(Mutex::new(State::fresh(TrustStore::new(&project)))),
            answered_trust: true,
            keeping: None,
            running: Some(running),
            model: None,
            watches: Arc::clone(&watches),
            auto_vetting: false,
            definition: None,
            permission_mode: PermissionMode::Ask,
        });
        let request = Request::parse(
            &json!({"id": 1, "method": "turn.cancel", "params": {"session": handle}}).to_string(),
        )
        .unwrap();
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
        let workspace = Workspace::new(root.clone()).unwrap();
        watches
            .arm(
                "file".into(),
                workspace.root().to_path_buf(),
                1,
                workspace.look("file", workspace.root()),
                now,
            )
            .unwrap();
        let watches = Arc::new(Mutex::new(watches));
        let events = Arc::new(Mutex::new(Vec::new()));
        let held = Arc::clone(&events);
        let mut bridge = Bridge::new(Box::new(move |e| held.lock().unwrap().push(e)));
        bridge.mint(Open {
            project: root.clone(),
            state: Arc::new(Mutex::new(State::fresh(TrustStore::new(&root)))),
            answered_trust: true,
            keeping: None,
            running: None,
            model: None,
            watches: Arc::clone(&watches),
            auto_vetting: false,
            definition: None,
            permission_mode: PermissionMode::Ask,
        });
        bridge.poll_watches_at(now + Duration::from_secs(7 * 24 * 60 * 60));
        assert!(watches.lock().unwrap().is_empty());
        assert_eq!(events.lock().unwrap()[0].data["reason"], "expired");
    }
}

#[cfg(test)]
#[path = "completion_tests.rs"]
mod completion_tests;

#[cfg(test)]
#[path = "targets_tests.rs"]
mod targets_tests;

#[cfg(test)]
#[path = "../tests/retention/worker.rs"]
mod retention_tests;
