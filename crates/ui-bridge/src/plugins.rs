//! Plugins: the installed list, turning one on or off, and removing one, from the window.
//!
//! A projection over `bravebot_agent::plugins` and nothing more. What is installed is read from the
//! records alone, so listing hashes nothing (PLUGIN-9), and turning a plugin on or off or removing
//! it asks nothing (PLUGIN-11). A refusal is said in the words `/plugin` says it in.
//!
//! Adding and updating run as a job ([`Jobs`]) on a thread of their own, through the shared
//! `plugins::install_typed` and `plugins::update_typed`, which put the questions in their fixed
//! order. The window answers them through [`WindowAsker`]: each question is an event and waits for
//! `plugins.reply`, and every way out but a yes to that very question is a no (PLUGIN-3, PLUGIN-4).

// The install listing and the update difference the add and update job puts to the window.
mod listing;

use crate::emit::Emitter;
use crate::protocol::{ErrorCode, Event, Failure, Request};
use bravebot_agent::confirm::{FetchRequest, printable_spelled};
use bravebot_agent::plugin_source::{NotASource, Requested, Source, classify};
use bravebot_agent::plugins::{
    self, Asker, Before, Installed, ManageRefusal, Refusal, Staged, Updated,
};
use bravebot_agent::skills::Catalogue;
use bravebot_config::plugin as manifest;
use bravebot_core::delegate::Definitions;
use bravebot_i18n::t;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Every installed plugin, as its record says it.
pub fn list() -> Result<Value, Failure> {
    let home = state()?;
    Ok(listed(&home, bravebot_agent::home::writable().is_some()))
}

/// Turn the plugin `name` on or off, and say what is installed afterwards.
pub fn set_enabled(request: &Request, enabled: bool) -> Result<Value, Failure> {
    let home = writable_state()?;
    let name = request.string("name")?;
    enabled_in(&home, &name, enabled)
}

/// Delete the plugin `name` and its record, and say what is installed afterwards.
pub fn remove(request: &Request) -> Result<Value, Failure> {
    let home = writable_state()?;
    let name = request.string("name")?;
    removed_in(&home, &name)
}

fn listed(home: &Path, writable: bool) -> Value {
    json!({
        "state": home.display().to_string(),
        "writable": writable,
        "plugins": plugins::list(home).iter().map(drawn).collect::<Vec<_>>(),
    })
}

fn enabled_in(home: &Path, name: &str, enabled: bool) -> Result<Value, Failure> {
    plugins::set_enabled(home, name, enabled).map_err(|refusal| refused(name, &refusal))?;
    Ok(listed(home, true))
}

fn removed_in(home: &Path, name: &str) -> Result<Value, Failure> {
    plugins::remove(home, name).map_err(|refusal| refused(name, &refusal))?;
    Ok(listed(home, true))
}

/// One record as the window draws it. Every field but `enabled` is text the plugin or the person
/// wrote; the window draws it as such.
fn drawn(plugin: &Installed) -> Value {
    let (requested_ref, commit) = match &plugin.source {
        Source::Local(_) => (None, None),
        Source::Git {
            requested_ref,
            commit,
            ..
        } => (requested_ref.clone(), Some(commit.as_str().to_string())),
    };
    json!({
        "name": plugin.name,
        "version": plugin.version,
        "source": plugin.source.to_string(),
        "requestedRef": requested_ref,
        "commit": commit,
        "digest": plugin.digest,
        "enabled": plugin.enabled,
    })
}

/// The sentence `/plugin remove`, `enable` and `disable` say a refusal in. The name is the one the
/// window sent, drawn through [`printable_spelled`].
fn refused(name: &str, refusal: &ManageRefusal) -> Failure {
    let shown = printable_spelled(name);
    match refusal {
        ManageRefusal::BadName => {
            Failure::bad_request(t!(plugin_not_a_name, name = shown).to_string())
        }
        ManageRefusal::NotInstalled(_) => {
            Failure::bad_request(t!(plugin_not_installed, name = shown).to_string())
        }
        ManageRefusal::Io(_) => Failure::new(
            ErrorCode::Internal,
            t!(plugin_manage_failed, name = shown).to_string(),
        ),
    }
}

/// How often a question that waits for its answer looks again whether the window has gone.
const LOOK_AGAIN: Duration = Duration::from_millis(200);

/// The add or update job running, if there is one. One at a time, since each puts its questions to
/// the one person at the window and two installs could take the same name.
#[derive(Default)]
pub(crate) struct Jobs {
    running: Arc<Mutex<Option<Running>>>,
    next: u64,
}

struct Running {
    job: String,
    /// Where `plugins.reply` sends the answer. A cancel drops it, so the question open now and
    /// every later one reads no.
    replies: Option<Sender<Reply>>,
}

/// The answer the window gave to the question numbered `question`.
struct Reply {
    question: u64,
    yes: bool,
}

/// How a job ended: the word the window keys on, and the sentence it shows.
type Ended = (&'static str, String);

impl Jobs {
    /// `plugins.add {source}`: answer with the job at once, and put its questions and its end as
    /// events. The source is what the person typed or picked. A relative path is refused, since the
    /// window has no workspace to read it from.
    pub(crate) fn add(&mut self, request: &Request, emitter: &Emitter) -> Result<Value, Failure> {
        let home = writable_state()?;
        let source = request.string("source")?;
        if let Ok(Requested::Local(path)) = classify(&source)
            && path.is_relative()
        {
            return Err(Failure::bad_request(
                t!(
                    desktop_plugin_add_not_absolute,
                    path = printable_spelled(&source)
                )
                .to_string(),
            ));
        }
        // The home directory is the root, though no relative path reaches it.
        self.spawn(home.clone(), emitter, move |asker| {
            added(&source, &home, &home, asker)
        })
    }

    /// `plugins.update {name}`: as `plugins.add`, for the installed plugin `name`.
    pub(crate) fn update(
        &mut self,
        request: &Request,
        emitter: &Emitter,
    ) -> Result<Value, Failure> {
        let home = writable_state()?;
        let name = request.string("name")?;
        self.spawn(home.clone(), emitter, move |asker| {
            updated(&name, &home, asker)
        })
    }

    fn spawn(
        &mut self,
        home: PathBuf,
        emitter: &Emitter,
        work: impl FnOnce(&mut WindowAsker) -> Ended + Send + 'static,
    ) -> Result<Value, Failure> {
        let mut running = self.running.lock().map_err(|_| {
            Failure::new(ErrorCode::Internal, "the plugin job state is unavailable")
        })?;
        if running.is_some() {
            return Err(Failure::bad_request(
                t!(desktop_plugin_job_running).to_string(),
            ));
        }
        self.next += 1;
        let job = self.next.to_string();
        let (sender, receiver) = mpsc::channel();
        *running = Some(Running {
            job: job.clone(),
            replies: Some(sender),
        });
        drop(running);
        let slot = Arc::clone(&self.running);
        let mut asker = WindowAsker {
            job: job.clone(),
            asked: 0,
            emitter: emitter.clone(),
            replies: receiver,
            home: home.clone(),
        };
        // Not joined at shutdown: the bridge ending drops the reply channel, so a question open
        // then reads no and the job writes nothing it was waiting to write.
        std::thread::spawn(move || {
            let (outcome, sentence) = work(&mut asker);
            if let Ok(mut running) = slot.lock()
                && running
                    .as_ref()
                    .is_some_and(|running| running.job == asker.job)
            {
                *running = None;
            }
            asker.emitter.send(Event::global(
                "plugins.ended",
                json!({
                    "job": asker.job,
                    "outcome": outcome,
                    // The terminal prints these without a stop; the window shows them as sentences.
                    "sentence": format!("{sentence}."),
                    "plugins": listed(&home, true)["plugins"],
                }),
            ));
        });
        Ok(json!({ "job": job }))
    }

    /// `plugins.reply {job, question, yes}`: hand the answer to the job named. A reply to a job that
    /// has ended is not an error, since the end may have crossed it.
    pub(crate) fn reply(&self, request: &Request) -> Result<Value, Failure> {
        let job = request.string("job")?;
        let question = request
            .params
            .get("question")
            .and_then(Value::as_u64)
            .ok_or_else(|| Failure::bad_request("`question` must be a number"))?;
        let yes = request
            .params
            .get("yes")
            .and_then(Value::as_bool)
            .ok_or_else(|| Failure::bad_request("`yes` must be true or false"))?;
        if let Ok(running) = self.running.lock()
            && let Some(running) = running.as_ref()
            && running.job == job
            && let Some(replies) = &running.replies
        {
            let _ = replies.send(Reply { question, yes });
        }
        Ok(json!({}))
    }

    /// `plugins.cancel {job}`: the open question and every later one of the job named read no.
    pub(crate) fn cancel(&self, request: &Request) -> Result<Value, Failure> {
        let job = request.string("job")?;
        if let Ok(mut running) = self.running.lock()
            && let Some(running) = running.as_mut()
            && running.job == job
        {
            running.replies = None;
        }
        Ok(json!({}))
    }

    /// The bridge is ending: whatever job runs reads no to every question, and nobody waits for it.
    pub(crate) fn stop_running(&self) {
        if let Ok(mut running) = self.running.lock()
            && let Some(running) = running.as_mut()
        {
            running.replies = None;
        }
    }
}

/// The window's side of the questions an add or an update puts (PLUGIN-3, PLUGIN-4, PLUGIN-9).
///
/// Each question is a `plugins.question` event, numbered within the job, and waits for the
/// `plugins.reply` that carries its number. A reply to another number, a cancel, the bridge ending
/// and the window going quiet are each a no. No permission mode, rule or setting is read here.
struct WindowAsker {
    job: String,
    /// The number of the last question put.
    asked: u64,
    emitter: Emitter,
    replies: Receiver<Reply>,
    /// The state directory, whose own skills and definitions the shadow marks are checked against.
    home: PathBuf,
}

impl WindowAsker {
    fn ask(&mut self, kind: &str, listing: Value) -> bool {
        if self.emitter.is_quiet() {
            return false;
        }
        // An answer sent before this question was put is not an answer to it.
        while self.replies.try_recv().is_ok() {}
        self.asked += 1;
        self.emitter.send(Event::global(
            "plugins.question",
            json!({
                "job": self.job,
                "question": self.asked,
                "kind": kind,
                "listing": listing,
            }),
        ));
        loop {
            match self.replies.recv_timeout(LOOK_AGAIN) {
                Ok(reply) => return reply.question == self.asked && reply.yes,
                Err(RecvTimeoutError::Timeout) if !self.emitter.is_quiet() => {}
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return false,
            }
        }
    }

    /// The skills and definitions the state directory itself offers. The window has no session to
    /// read a project's from, so a project skill that would shadow a plugin's is not marked, and
    /// the question says so (`homeSourcesOnly`).
    fn offered(&self) -> (Catalogue, Definitions) {
        let Ok(workspace) = bravebot_agent::workspace::Workspace::new(&self.home) else {
            return (Catalogue::default(), Definitions::default());
        };
        let trust = || bravebot_core::trust::TrustStore::new(&self.home);
        let permissions = bravebot_core::permissions::Permissions::default;
        let mut trail = bravebot_session::audit::Trail::default();
        let catalogue = bravebot_agent::skills::resolved(
            &workspace,
            Some(&self.home),
            trust(),
            permissions(),
            &mut trail,
        );
        let definitions = bravebot_agent::agents::resolved(
            &workspace,
            Some(&self.home),
            trust(),
            permissions(),
            &mut trail,
        );
        (catalogue, definitions)
    }
}

impl Asker for WindowAsker {
    fn anybody_there(&self) -> bool {
        !self.emitter.is_quiet()
    }

    fn confirm_fetch_host(&mut self, request: &FetchRequest) -> bool {
        self.ask("host", json!({ "host": request.host, "url": request.url }))
    }

    fn confirm_install(&mut self, staged: &Staged) -> bool {
        let (catalogue, definitions) = self.offered();
        let mut shown = listing::install(staged, &catalogue, &definitions);
        shown["homeSourcesOnly"] = json!(true);
        self.ask("install", shown)
    }

    fn confirm_update(&mut self, staged: &Staged, installed: &Installed, before: &Before) -> bool {
        let (catalogue, definitions) = self.offered();
        let mut shown = listing::update(staged, installed, before, &catalogue, &definitions);
        shown["homeSourcesOnly"] = json!(true);
        self.ask("update", shown)
    }
}

/// An add, said as `/plugin add` says it.
fn added<A: Asker + ?Sized>(source: &str, root: &Path, home: &Path, asker: &mut A) -> Ended {
    let shown = printable_spelled(source);
    match plugins::install_typed(source, root, home, asker) {
        Ok(name) => (
            "installed",
            t!(plugin_added, name = printable_spelled(&name), path = shown).to_string(),
        ),
        Err(refusal) => (
            ended_word(&refusal),
            t!(plugin_add_refused, path = shown, why = why(&refusal)).to_string(),
        ),
    }
}

/// An update, said as `/plugin update` says it.
fn updated<A: Asker + ?Sized>(name: &str, home: &Path, asker: &mut A) -> Ended {
    let shown = printable_spelled(name);
    match plugins::update_typed(name, home, asker) {
        Ok(Updated::Replaced) => ("updated", t!(plugin_update_done, name = shown).to_string()),
        Ok(Updated::Unchanged {
            newer_commit: false,
        }) => (
            "unchanged",
            t!(plugin_update_unchanged, name = shown).to_string(),
        ),
        Ok(Updated::Unchanged { newer_commit: true }) => (
            "unchanged",
            t!(plugin_update_unchanged_newer_commit, name = shown).to_string(),
        ),
        Err(refusal) => {
            let sentence = match &refusal {
                Refusal::Manage(ManageRefusal::BadName) => {
                    t!(plugin_not_a_name, name = shown).to_string()
                }
                Refusal::Manage(ManageRefusal::NotInstalled(_)) => {
                    t!(plugin_not_installed, name = shown).to_string()
                }
                Refusal::Declined => t!(plugin_update_declined, name = shown).to_string(),
                Refusal::NobodyToAsk => t!(plugin_update_nobody_to_ask, name = shown).to_string(),
                other => t!(plugin_update_refused, name = shown, why = why(other)).to_string(),
            };
            (ended_word(&refusal), sentence)
        }
    }
}

/// The word `plugins.ended` carries for a refusal: whether the person said no, nobody could be
/// asked, or the job was refused.
fn ended_word(refusal: &Refusal) -> &'static str {
    match refusal {
        Refusal::Declined => "declined",
        Refusal::NobodyToAsk => "nobody",
        _ => "refused",
    }
}

/// The words a refusal is said in, as `refused` in `crates/tui/src/plugin_command.rs` says them.
/// Only the shape of the refusal is read; a name from a manifest is drawn printable.
fn why(refusal: &Refusal) -> String {
    let printable = |text: &str| printable_spelled(text);
    match refusal {
        Refusal::Manifest(manifest::Refusal::NoManifest | manifest::Refusal::NoPluginToml(_)) => {
            t!(plugin_refused_no_manifest).to_string()
        }
        Refusal::Manifest(other) => match other.named_key() {
            Some(key) => t!(plugin_refused_manifest_key, key = printable(&key)).to_string(),
            None => t!(plugin_refused_manifest).to_string(),
        },
        Refusal::PartMissing(part) => t!(
            plugin_refused_part_missing,
            key = manifest::Field::Part(*part).key()
        )
        .to_string(),
        Refusal::NotASource(why) => match why {
            NotASource::Archive => t!(plugin_refused_archive).to_string(),
            NotASource::NpmPackage => t!(plugin_refused_npm_package).to_string(),
            NotASource::PlainUrl => t!(plugin_refused_plain_url).to_string(),
            NotASource::FileUrl => t!(plugin_refused_file_url).to_string(),
            NotASource::UnsupportedScheme => t!(plugin_refused_unsupported_scheme).to_string(),
            NotASource::BadUrl => t!(plugin_refused_bad_url).to_string(),
            NotASource::BadRef => t!(plugin_refused_bad_ref).to_string(),
        },
        Refusal::GitNotInstalled => t!(plugin_refused_git_not_installed).to_string(),
        Refusal::CloneFailed => t!(plugin_refused_clone_failed).to_string(),
        Refusal::Source(_) | Refusal::SourceNotText | Refusal::Io(_) => {
            t!(plugin_refused_unreadable).to_string()
        }
        Refusal::AlreadyInstalled { name, .. } => {
            t!(plugin_refused_already_installed, name = printable(name)).to_string()
        }
        Refusal::SameSource { name, .. } => {
            t!(plugin_refused_same_source, name = printable(name)).to_string()
        }
        Refusal::NameOffered {
            offered,
            new_plugin,
            existing_plugin,
        } => t!(
            plugin_refused_name_offered,
            offered = printable(offered),
            new = printable(new_plugin),
            existing = printable(existing_plugin)
        )
        .to_string(),
        Refusal::Manage(ManageRefusal::NotInstalled(name)) => {
            t!(plugin_not_installed, name = printable(name)).to_string()
        }
        Refusal::Manage(ManageRefusal::BadName | ManageRefusal::Io(_)) => {
            t!(plugin_refused_unreadable).to_string()
        }
        Refusal::RecordUnreadable { .. } => t!(plugin_refused_record_unreadable).to_string(),
        Refusal::NothingToUpdateTo { .. } => t!(plugin_refused_nothing_to_update_to).to_string(),
        Refusal::UpdateRenamed { found, .. } => {
            t!(plugin_refused_update_renamed, found = printable(found)).to_string()
        }
        Refusal::NobodyToAsk => t!(plugin_refused_nobody_to_ask).to_string(),
        Refusal::Declined => t!(plugin_refused_declined).to_string(),
    }
}

fn state() -> Result<PathBuf, Failure> {
    bravebot_agent::home::directory()
        .ok_or_else(|| Failure::new(ErrorCode::NoHome, t!(plugin_command_no_home).to_string()))
}

/// The state directory, where this run may write it, which an incognito one may not.
fn writable_state() -> Result<PathBuf, Failure> {
    let home = state()?;
    bravebot_agent::home::writable()
        .map(|_| home)
        .ok_or_else(|| Failure::bad_request(t!(plugin_manage_not_writable).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn scratch() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("bravebot-bridge-plugins-")
            .tempdir()
            .expect("a scratch state directory")
    }

    /// An installed plugin as `commit_install` leaves one: its directory and the record beside it.
    fn installed(home: &Path, name: &str, record: &str) {
        let plugins = home.join("plugins");
        std::fs::create_dir_all(plugins.join(name).join("skills")).unwrap();
        std::fs::write(plugins.join(name).join("plugin.toml"), "name = \"x\"\n").unwrap();
        std::fs::write(plugins.join(format!("{name}.record")), record).unwrap();
    }

    fn local(home: &Path, name: &str) {
        installed(
            home,
            name,
            "source = /src/local\nversion = 1.0.0\nenabled = true\ndigest = abc\n",
        );
    }

    fn git(home: &Path, name: &str) {
        installed(
            home,
            name,
            &format!(
                "source = https://example.com/p.git\nrequested-ref = v2\ncommit = {COMMIT}\n\
                 version = 2.3.4\nenabled = false\ndigest = def\n"
            ),
        );
    }

    /// PLUGIN-9: the list is the records' fields, and only a git source has a commit or a ref.
    #[test]
    fn the_list_is_what_each_record_says_and_only_git_has_a_commit() {
        let home = scratch();
        local(home.path(), "alpha");
        git(home.path(), "beta");

        let shown = listed(home.path(), true);

        assert_eq!(shown["state"], json!(home.path().display().to_string()));
        assert_eq!(shown["writable"], json!(true));
        assert_eq!(
            shown["plugins"],
            json!([
                {"name": "alpha", "version": "1.0.0", "source": "/src/local", "requestedRef": null,
                 "commit": null, "digest": "abc", "enabled": true},
                {"name": "beta", "version": "2.3.4", "source": "https://example.com/p.git",
                 "requestedRef": "v2", "commit": COMMIT, "digest": "def", "enabled": false},
            ])
        );
    }

    /// An asker whose window answers each question with what `answer` returns for its number, and
    /// the questions it was put. `None` sends nothing. The sender is held by the window alone, so
    /// dropping it from the script drops the channel.
    fn window(
        answer: impl Fn(u64) -> Option<Reply> + Send + 'static,
    ) -> (WindowAsker, Arc<Mutex<Vec<Value>>>) {
        let (sender, replies) = mpsc::channel();
        let put = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&put);
        let sender = Mutex::new(Some(sender));
        let emitter = Emitter::new(Box::new(move |event: Event| {
            if event.name != "plugins.question" {
                return;
            }
            let question = event.data["question"].as_u64().unwrap();
            seen.lock().unwrap().push(event.data);
            let mut held = sender.lock().unwrap();
            match answer(question) {
                Some(reply) => {
                    held.as_ref().map(|sender| sender.send(reply));
                }
                None => *held = None,
            }
        }));
        (
            WindowAsker {
                job: "7".to_string(),
                asked: 0,
                emitter,
                replies,
                home: PathBuf::new(),
            },
            put,
        )
    }

    fn yes(question: u64) -> Option<Reply> {
        Some(Reply {
            question,
            yes: true,
        })
    }

    /// PLUGIN-4: only a yes to the question put is a yes. The question carries the job, its number
    /// and its kind. Rejects an asker that takes any yes, or one that numbers no question.
    #[test]
    fn only_a_yes_to_the_question_put_is_a_yes() {
        let (mut asker, put) = window(yes);
        assert!(asker.anybody_there());
        assert!(asker.ask("install", json!({"x": 1})));
        assert_eq!(
            put.lock().unwrap()[0],
            json!({"job": "7", "question": 1, "kind": "install", "listing": {"x": 1}})
        );

        let (mut asker, _) = window(|question| yes(question + 1));
        assert!(!asker.ask("host", json!({})), "a yes to another number");

        let (mut asker, _) = window(|question| {
            Some(Reply {
                question,
                yes: false,
            })
        });
        assert!(!asker.ask("update", json!({})));
    }

    /// A yes the window sent before the question was put does not answer it.
    #[test]
    fn a_reply_sent_before_the_question_is_not_its_answer() {
        let (sender, replies) = mpsc::channel();
        let (mut asker, put) = window(|_| None);
        asker.replies = replies;
        sender.send(yes(1).unwrap()).unwrap();
        let waiting = std::thread::spawn(move || asker.ask("install", json!({})));
        std::thread::sleep(Duration::from_millis(300));
        drop(sender);
        assert!(!waiting.join().unwrap());
        assert_eq!(put.lock().unwrap().len(), 1);
    }

    /// A dropped channel, which is what a cancel and the bridge ending leave, is a no.
    #[test]
    fn a_dropped_channel_is_a_no() {
        let (mut asker, put) = window(|_| None);
        assert!(!asker.ask("install", json!({})));
        assert_eq!(put.lock().unwrap().len(), 1);
    }

    /// A window that has gone is nobody: nothing is asked, and a question open when it goes is a no.
    #[test]
    fn a_quiet_window_is_nobody_and_a_question_open_when_it_goes_is_a_no() {
        let (mut asker, put) = window(|_| {
            Some(Reply {
                question: 0,
                yes: false,
            })
        });
        asker.emitter.go_quiet();
        assert!(!asker.anybody_there());
        assert!(!asker.ask("install", json!({})));
        assert!(put.lock().unwrap().is_empty());

        // A window that never answers, and then goes.
        let (sender, replies) = mpsc::channel::<Reply>();
        let (mut asker, _) = window(|_| {
            Some(Reply {
                question: 0,
                yes: false,
            })
        });
        asker.replies = replies;
        let emitter = asker.emitter.clone();
        let waiting = std::thread::spawn(move || asker.ask("install", json!({})));
        std::thread::sleep(Duration::from_millis(300));
        emitter.go_quiet();
        assert!(!waiting.join().unwrap());
        drop(sender);
    }

    /// PLUGIN-4: with no window, an add reads nothing, asks nothing and writes nothing.
    #[test]
    fn with_no_window_an_add_is_nobody_to_ask_and_writes_nothing() {
        let (mut asker, put) = window(yes);
        asker.emitter.go_quiet();
        let home = scratch();
        let (outcome, sentence) = added("/nowhere/plugin", home.path(), home.path(), &mut asker);
        assert_eq!(outcome, "nobody");
        assert_eq!(
            sentence,
            t!(
                plugin_add_refused,
                path = "/nowhere/plugin",
                why = t!(plugin_refused_nobody_to_ask).to_string()
            )
            .to_string()
        );
        assert!(put.lock().unwrap().is_empty());
        assert!(!home.path().join("plugins").exists());
    }

    #[test]
    fn nothing_installed_is_an_empty_list() {
        let home = scratch();
        assert_eq!(listed(home.path(), false)["plugins"], json!([]));
    }

    /// PLUGIN-11: disabling then enabling changes the enabled state and nothing else in the record.
    #[test]
    fn disabling_then_enabling_flips_only_enabled() {
        let home = scratch();
        git(home.path(), "beta");
        let record = home.path().join("plugins").join("beta.record");
        let before = listed(home.path(), true)["plugins"][0].clone();

        let enabled = enabled_in(home.path(), "beta", true).expect("enabled");
        let mut expected = before.clone();
        expected["enabled"] = json!(true);
        assert_eq!(enabled["plugins"], json!([expected]));

        let disabled = enabled_in(home.path(), "beta", false).expect("disabled");
        assert_eq!(disabled["plugins"], json!([before]));
        assert!(home.path().join("plugins").join("beta").is_dir());
        assert!(
            std::fs::read_to_string(&record)
                .unwrap()
                .contains(&format!("commit = {COMMIT}"))
        );
    }

    /// PLUGIN-9: removing deletes the directory and the record, and the answer lists the rest.
    #[test]
    fn removing_deletes_the_directory_and_the_record() {
        let home = scratch();
        local(home.path(), "alpha");
        git(home.path(), "beta");

        let after = removed_in(home.path(), "alpha").expect("removed");

        let plugins = home.path().join("plugins");
        assert!(!plugins.join("alpha").exists());
        assert!(!plugins.join("alpha.record").exists());
        assert!(plugins.join("beta").is_dir());
        assert_eq!(after["plugins"].as_array().map(Vec::len), Some(1));
        assert_eq!(after["plugins"][0]["name"], json!("beta"));
    }

    /// A name that is not a plugin's, or one not installed, is refused in `/plugin`'s sentence and
    /// changes nothing. The name is drawn printable, so a control character in it is pictured.
    #[test]
    fn a_bad_or_missing_name_is_refused_in_a_sentence() {
        let home = scratch();
        local(home.path(), "alpha");

        let bad = enabled_in(home.path(), "../alpha\u{1b}", false).expect_err("bad name");
        assert_eq!(bad.code, ErrorCode::BadRequest);
        assert_eq!(
            bad.message,
            t!(
                plugin_not_a_name,
                name = printable_spelled("../alpha\u{1b}")
            )
            .to_string()
        );
        assert!(!bad.message.contains('\u{1b}'));

        let missing = removed_in(home.path(), "gamma").expect_err("not installed");
        assert_eq!(missing.code, ErrorCode::BadRequest);
        assert_eq!(
            missing.message,
            t!(plugin_not_installed, name = "gamma").to_string()
        );
        let missing = enabled_in(home.path(), "gamma", true).expect_err("not installed");
        assert_eq!(
            missing.message,
            t!(plugin_not_installed, name = "gamma").to_string()
        );

        assert_eq!(
            listed(home.path(), true)["plugins"][0]["enabled"],
            json!(true)
        );
    }
}
