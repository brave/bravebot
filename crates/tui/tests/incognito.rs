//! An incognito session adds nothing to `~/.bravebot`.
//!
//! These run against a real directory, redirected by `HOME`, because the claim is about the
//! filesystem afterwards rather than about which branch some function took. Every test here asks
//! the same question in a different place: after doing the thing that ordinarily writes, is there
//! anything on disk that was not there before? The one exception is the standing answer about
//! auto-vetting, which this mode refuses to read as well as to write, so the test for it asserts a
//! read that came back empty against a file that was there to be read.
//!
//! # Why a binary of its own
//!
//! Engaging is a one-way door for the life of a process, which is the property that makes it
//! trustworthy and also the reason these cannot share a binary with [`persist`]. An integration
//! test is its own process, so this file engages once at the top of each test and no other test
//! in the workspace is affected.
//!
//! # Why each test also reads
//!
//! A test asserting that nothing was written passes just as well when nothing was attempted: a
//! mistyped `HOME`, a helper that silently did nothing. So each test below first reads back
//! something it seeded, which fails unless the directory really is the one the code under test is
//! looking at. The read is the proof that the absence of a write means something.
//!
//! [`persist`]: ../persist.rs

use bravebot_session::sessions::{Front, Handle, Standing};
use bravebot_session::store;
use bravebot_session::store::Entry;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Serialises the tests in this binary, since `HOME` is process-wide.
static HOME: Mutex<()> = Mutex::new(());

/// A scratch home holding whatever a test seeded, so a test never touches the real one.
struct Scratch {
    home: PathBuf,
    project: PathBuf,
    /// Dropped last, releasing `HOME` only once this test's directory is gone.
    _lock: MutexGuard<'static, ()>,
}

impl Scratch {
    /// A scratch `HOME` with the mode already engaged.
    ///
    /// Engaged here rather than in each test so no test can forget, which would turn it into one
    /// that asserts the ordinary behaviour and passes for the wrong reason.
    fn incognito(name: &str) -> Self {
        let lock = HOME.lock().unwrap_or_else(|held| held.into_inner());
        let root = std::env::temp_dir().join(format!(
            "bravebot-incognito-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let project = root.join("project");
        std::fs::create_dir_all(&home).expect("create home");
        std::fs::create_dir_all(&project).expect("create project");
        // SAFETY: `HOME` is held for as long as this value lives, so no other test in this binary
        // is reading or writing the variable while this one owns it.
        unsafe { std::env::set_var("HOME", &home) };
        unsafe { std::env::remove_var("BRAVEBOT_HOME") };
        bravebot_core::incognito::engage();
        proxy_listener();
        Self {
            home,
            project,
            _lock: lock,
        }
    }

    /// The directory the mode is about.
    fn own_directory(&self) -> PathBuf {
        self.home.join(".bravebot")
    }

    /// Put a file in the user's own directory, as a previous ordinary session would have left it.
    fn seed(&self, name: &str, contents: &str) {
        let dir = self.own_directory();
        std::fs::create_dir_all(&dir).expect("seed the directory");
        std::fs::write(dir.join(name), contents).expect("seed a file");
    }

    /// Everything inside the user's own directory now, relative to it and sorted.
    ///
    /// A listing rather than a check of one path, so a write landing somewhere unexpected is
    /// caught by the same assertion as one landing where it was expected.
    fn contents(&self) -> Vec<String> {
        let root = self.own_directory();
        let mut found = Vec::new();
        walk(&root, &root, &mut found);
        found.sort();
        found
    }
}

/// Every file under `dir`, named relative to `root`.
fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, found);
        } else if let Ok(relative) = path.strip_prefix(root) {
            found.push(relative.display().to_string());
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.home.parent().expect("a root"));
    }
}

/// A listener standing in for the proxy `HTTPS_PROXY` names, so a request this process makes shows
/// up as a connection without a network.
///
/// The transport reads the variable once for the process, so every test here sets it as it
/// starts. A test that ran earlier without it would fix the transport on the real network, and a
/// request made by a later one would go past the listener. `127.0.0.1` is spelled out because
/// `localhost` does not resolve on every host these run on.
fn proxy_listener() -> &'static std::net::TcpListener {
    static LISTENER: std::sync::OnceLock<std::net::TcpListener> = std::sync::OnceLock::new();
    LISTENER.get_or_init(|| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a listener");
        listener.set_nonblocking(true).expect("a polling listener");
        let port = listener.local_addr().expect("an address").port();
        // SAFETY: this runs once, before any test in this binary has made a request.
        unsafe { std::env::set_var("HTTPS_PROXY", format!("http://127.0.0.1:{port}")) };
        listener
    })
}

/// Whether a connection reaches `listener` within `wait`.
fn connected_within(listener: &std::net::TcpListener, wait: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        if listener.accept().is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

/// A prompt sent now from nowhere in particular.
fn sent(prompt: &str) -> Entry {
    Entry::sent(prompt, None)
}

/// INCOG-1: a prompt typed in an incognito session is not written down.
#[test]
fn no_prompt_is_written_down() {
    let scratch = Scratch::incognito("history");
    scratch.seed("history", "1700000000\t\tan earlier prompt\n");

    // The read that proves the directory below is the one being written to.
    assert_eq!(
        store::load_history()
            .iter()
            .map(|entry| entry.prompt.clone())
            .collect::<Vec<_>>(),
        vec!["an earlier prompt".to_string()],
        "the seeded history was not read back, so this test proves nothing"
    );

    store::append_history(&sent("something private"));
    store::save_history(&[sent("something private"), sent("and another")]);

    assert_eq!(
        std::fs::read_to_string(scratch.own_directory().join("history")).expect("the seeded file"),
        "1700000000\t\tan earlier prompt\n",
        "an incognito session added a prompt to the history file"
    );
    assert_eq!(scratch.contents(), vec!["history".to_string()]);
}

/// INCOG-2: choices made in an incognito session apply to it and outlive nothing.
#[test]
fn a_choice_applies_to_the_session_and_is_not_recorded() {
    let scratch = Scratch::incognito("choices");
    scratch.seed("model", "claude-sonnet-4-5\n");
    scratch.seed("theme", "nord\n");
    scratch.seed("language", "fr\n");

    // Reading is the half that must keep working: a private session still opens with the model and
    // theme the person chose, which is what makes it a private session rather than a fresh install.
    assert_eq!(store::load_model().as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(store::load_theme().as_deref(), Some("nord"));
    assert_eq!(store::load_language().as_deref(), Some("fr"));

    store::save_model("some-other-model");
    store::save_theme("brave");
    store::save_effort(Some(bravebot_aichat::protocol::Effort::Xhigh));
    store::save_editing(bravebot_tui::vim::Editing::Vi.as_str());
    // The language is the one choice that applies from the next start, so a save that reports
    // having kept it is the only sign of a choice nothing recorded.
    assert!(
        !store::save_language(Some("en-US")),
        "an incognito session said it had saved a language"
    );
    assert!(
        !store::save_language(None),
        "an incognito session said it had forgotten the language"
    );

    assert_eq!(
        store::load_model().as_deref(),
        Some("claude-sonnet-4-5"),
        "an incognito session overwrote the recorded model"
    );
    assert_eq!(
        store::load_language().as_deref(),
        Some("fr"),
        "an incognito session replaced or removed the recorded language"
    );
    assert_eq!(
        store::load_theme().as_deref(),
        Some("nord"),
        "an incognito session overwrote the recorded theme"
    );
    assert_eq!(
        store::load_effort(),
        None,
        "an incognito session recorded an effort level"
    );
    assert_eq!(
        store::load_editing(),
        None,
        "an incognito session recorded a style of editing"
    );

    let mut expected = vec![
        "language".to_string(),
        "model".to_string(),
        "theme".to_string(),
    ];
    expected.sort();
    assert_eq!(scratch.contents(), expected);
}

/// INCOG-2: the theme an ordinary session recorded is the one an incognito session opens on.
///
/// `restore_saved` is the startup read, the only place the recorded theme is applied. The test
/// puts the default in force first and seeds a theme that is not the default, so a startup that
/// skipped the read under the mode (and so stayed on `brave`) is told apart from one that applied
/// the recorded name. The read that proves the seeded file is the one being looked at is the
/// `load_theme` before the call. The call sends no query to a terminal: it reads a file and puts
/// a palette in force.
#[test]
fn the_startup_opens_an_incognito_session_on_the_recorded_theme() {
    let scratch = Scratch::incognito("startup-theme");
    scratch.seed("theme", "nord\n");
    let seeded = scratch.contents();
    assert_eq!(store::load_theme().as_deref(), Some("nord"));

    bravebot_tui::theme::apply_brave();
    assert_eq!(
        bravebot_tui::theme::name(),
        "brave",
        "the default is not in force"
    );

    bravebot_tui::theme::restore_saved();

    assert_eq!(
        bravebot_tui::theme::name(),
        "nord",
        "an incognito session did not open on the recorded theme"
    );
    assert_eq!(
        scratch.contents(),
        seeded,
        "restoring the theme wrote to the directory"
    );
    assert_eq!(store::load_theme().as_deref(), Some("nord"));
    bravebot_tui::theme::apply_brave();
}

/// INCOG-2: the recorded choice an ordinary session made is what an incognito session starts on,
/// and a choice made inside it takes effect for the session while the files stay as they were.
///
/// The session persists, so the only thing between its setters and the files is the mode. A setter
/// that returned early under incognito would leave the session on the recorded choice, and a startup
/// that skipped the read would open it on the default; both are told apart here by choosing values
/// that differ from the recorded ones.
#[test]
fn a_choice_takes_effect_for_the_session_and_the_startup_reads_the_recorded_one() {
    use bravebot_aichat::protocol::Effort;
    use bravebot_tui::vim::Editing;

    let scratch = Scratch::incognito("session-choices");
    scratch.seed("model", "claude-sonnet-4-5\n");
    scratch.seed("effort", "low\n");
    scratch.seed("editor-mode", "vim\n");
    let seeded = scratch.contents();
    let config = bravebot_config::Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("a-signing-key".to_string()),
        "BRAVE_SERVICES_KEY_ID" => Some("a-key-id".to_string()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("https://example.invalid".to_string()),
        _ => None,
    })
    .expect("config");
    let settings = bravebot_config::Settings::default();

    let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
    session.adopt_model(&settings, &config);
    session.adopt_effort(&settings);
    session.adopt_editing(None);
    assert_eq!(
        session.model(),
        Some("claude-sonnet-4-5"),
        "the session did not start on the recorded model"
    );
    assert_eq!(
        session.effort(),
        Some(Effort::Low),
        "the session did not start on the recorded effort level"
    );
    assert_eq!(
        session.editing(),
        Editing::Vi,
        "the session did not start on the recorded style of editing"
    );

    session.choose_model("some-other-model", &config);
    session.choose_effort(Some(Effort::Xhigh));
    session.choose_editing(Editing::Ordinary);
    assert_eq!(
        session.model(),
        Some("some-other-model"),
        "an incognito session kept the model it started on after one was chosen"
    );
    assert_eq!(
        session.effort(),
        Some(Effort::Xhigh),
        "an incognito session kept the effort level it started on after one was chosen"
    );
    assert_eq!(
        session.editing(),
        Editing::Ordinary,
        "an incognito session kept the style of editing it started on after one was chosen"
    );

    // Asking for no level removes the record in an ordinary session, so it is the choice that would
    // reach a seeded file here if the mode did not hold.
    session.choose_effort(None);
    assert_eq!(session.effort(), None, "choosing no level did not apply");

    for (file, recorded) in [
        ("model", "claude-sonnet-4-5\n"),
        ("effort", "low\n"),
        ("editor-mode", "vim\n"),
    ] {
        assert_eq!(
            std::fs::read_to_string(scratch.own_directory().join(file)).expect("the seeded file"),
            recorded,
            "an incognito session changed the recorded {file}"
        );
    }
    assert_eq!(scratch.contents(), seeded);
}

/// INCOG-3: an incognito session leaves no record of itself, and none that it happened.
#[test]
fn no_session_record_is_written() {
    let scratch = Scratch::incognito("sessions");

    let mut handle = Handle::begin(&scratch.project, Front::Terminal, bravebot_stamp::BUILD);
    let conversation = {
        let mut conversation = bravebot_agent::Conversation::new();
        conversation.push(bravebot_aichat::protocol::Message::user(
            "something private",
        ));
        conversation.push(bravebot_aichat::protocol::Message::assistant("done"));
        conversation
    };

    handle.save(
        "something private",
        Standing {
            history: None,
            conversation: &conversation.snapshot(),
            turns: 1,
            tokens: 1_200,
            split: Default::default(),
            spend: &BTreeMap::new(),
            timing: &BTreeMap::new(),
            model: None,
            todos: &BTreeMap::new(),
            asides: &[],
            trust: &bravebot_core::trust::TrustStore::new("/work"),
            programs: &bravebot_core::programs::TrustedPrograms::from_iter([]),
            directories: &[],
            manifest: None,
            rewind: &[],
            checkouts: &[],
            touched: None,
            answered_by_mode: false,
        },
    );

    assert_eq!(
        handle.resumable(),
        None,
        "an incognito session offered itself for resuming, so something was written"
    );
    assert!(
        !scratch.own_directory().exists(),
        "an incognito session created {}, which records that a session happened at all",
        scratch.own_directory().display()
    );
}

/// INCOG-3: naming a session records nothing either.
///
/// Separate because the title takes its own path through the writer (read, amend, rewrite), and a
/// guard on `save` alone would leave it open.
#[test]
fn naming_a_session_records_nothing() {
    let scratch = Scratch::incognito("titles");

    let mut handle = Handle::begin(&scratch.project, Front::Terminal, bravebot_stamp::BUILD);
    handle.rename("a title worth keeping");

    assert!(
        !scratch.own_directory().exists(),
        "naming an incognito session wrote something"
    );
}

/// INCOG-4: the audit trail of an incognito session stays in the session.
///
/// The trail says which gates ran and on what paths. It is not content, but it is a record of a
/// session having happened and what it touched, which is the thing this mode is for not leaving.
#[test]
fn no_audit_trail_is_written() {
    let scratch = Scratch::incognito("audit");

    let handle = Handle::begin(&scratch.project, Front::Terminal, bravebot_stamp::BUILD);
    handle.append_audit(
        1,
        &[bravebot_session::audit::Stamped {
            at: 1_700_000_000,
            from: None,
            event: bravebot_core::event::Event::GatePassed {
                gate: "a-gate",
                detail: "a decision worth recording".to_string(),
            },
        }],
    );

    assert!(
        !scratch.own_directory().exists(),
        "an incognito session wrote an audit trail"
    );
}

/// INCOG-8 (#4609): the editor hand-off is still made in an incognito session, in a directory of its
/// own under the account's cache directory, and adds nothing to `~/.bravebot`. The editor here is a
/// script that records the path it was handed and saves over the file, so the test reads where the
/// file was made from the editor's own argument, and that what it saved came back.
///
/// The regressions it rejects are a file made under the state directory, which an incognito session
/// may not write and which refuses the hand-off, and a fallback to the system temporary directory
/// where the state directory is refused, which every stage of `run` may write.
#[cfg(unix)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
#[test]
fn the_editor_hand_off_is_made_under_the_cache_directory_and_adds_nothing_to_the_state_directory() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::incognito("editor");
    let record = scratch.project.join("handed");
    let editor = scratch.project.join("editor.sh");
    std::fs::write(
        &editor,
        format!(
            "#!/bin/sh\nprintf '%s' \"$1\" > '{}'\nprintf 'saved by the person' > \"$1\"\n",
            record.display()
        ),
    )
    .expect("an editor script");
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).expect("mode");
    // SAFETY: `HOME` is held for as long as `scratch` lives, and nothing else in this binary reads
    // these variables; `EDITOR` is removed and `XDG_CACHE_HOME` put back below.
    unsafe { std::env::remove_var("VISUAL") };
    let cache_variable = std::env::var_os("XDG_CACHE_HOME");
    // SAFETY: as above.
    unsafe { std::env::remove_var("XDG_CACHE_HOME") };
    // SAFETY: as above.
    unsafe { std::env::set_var("EDITOR", &editor) };

    let edited = bravebot_tui::editor::edit("a prompt nobody should see");
    let cache = bravebot_agent::home::cache();
    // SAFETY: as above.
    unsafe { std::env::remove_var("EDITOR") };
    if let Some(value) = cache_variable {
        // SAFETY: as above.
        unsafe { std::env::set_var("XDG_CACHE_HOME", value) };
    }

    assert_eq!(
        edited.expect("the hand-off was refused in an incognito session"),
        "saved by the person"
    );
    let handed = PathBuf::from(std::fs::read_to_string(&record).expect("the editor ran"));
    let cache = cache.expect("the scratch home names a cache directory");
    assert!(
        handed.starts_with(cache.join("bravebot").join("editor")),
        "the file was made at {}",
        handed.display()
    );
    assert!(!handed.exists(), "the file outlived the edit");
    assert!(
        !scratch.own_directory().exists(),
        "an incognito session made {}",
        scratch.own_directory().display()
    );
}

/// UPDATE-11: with the check off the recorded answer is not read, so no line is said.
///
/// The same seeded record as the test below says a line with the check left on, which is what
/// shows the silence is the setting and not an absent record.
#[test]
fn a_check_turned_off_says_nothing_though_a_newer_version_is_recorded() {
    let scratch = Scratch::incognito("updates-off");
    let running = std::env::current_exe().expect("the test binary");
    scratch.seed("installed-by", &format!("{}\n", running.display()));
    scratch.seed("update-check", "1700000000\treleases\t99.0.0\n");

    assert!(bravebot_update::at_startup(Some(false)).is_none());
    assert!(
        bravebot_update::at_startup(Some(true)).is_some(),
        "the seeded record does not say a line, so the silence above proves nothing"
    );
}

/// UPDATE-8: an incognito session still reads what an ordinary one wrote down about updating.
///
/// The mode is about what survives a session rather than about what it may know, and a private
/// session that stopped telling somebody their bravebot was superseded would be crippled rather
/// than private. Nothing new is recorded and the answer already there is left as it was: asking
/// again is decided by whether this session may write at all, and the write itself resolves the
/// directory through the writing answer, as every other write to it does.
#[test]
fn the_answer_about_updating_is_still_read() {
    let scratch = Scratch::incognito("updates");
    // The binary this test is running as, which is what makes the seeded record name it.
    let running = std::env::current_exe().expect("the test binary");
    scratch.seed("installed-by", &format!("{}\n", running.display()));
    scratch.seed("update-check", "1700000000\treleases\t99.0.0\n");

    // A line is said only once a record is read, since without one there is no version to be behind.
    assert!(
        bravebot_update::at_startup(None).is_some(),
        "the recorded answer was not read back, so a version this far ahead went unannounced"
    );

    let mut expected = vec!["installed-by".to_string(), "update-check".to_string()];
    expected.sort();
    assert_eq!(
        scratch.contents(),
        expected,
        "an incognito session recorded something about updating"
    );
    assert_eq!(
        std::fs::read_to_string(scratch.own_directory().join("update-check"))
            .expect("the seeded file"),
        "1700000000\treleases\t99.0.0\n",
        "an incognito session overwrote the recorded answer"
    );
}

/// UPDATE-8: an incognito session makes no request to the registry, though the record it reads is
/// old enough that an ordinary session would ask.
///
/// The record is the one an ordinary session left, so the line is still said and the stamp is
/// stale: the only thing keeping the request back is the session being unable to write. A
/// proxy that records connections is what sees a request, because nothing written afterwards
/// can show one that was thrown away on arrival. The ordinary session in `persist.rs` makes the
/// same call and does connect, so a silence here is the mode and not a listener nothing reaches.
#[test]
fn a_session_that_records_nothing_makes_no_request_for_a_newer_version() {
    let scratch = Scratch::incognito("updates-ask");
    let running = std::env::current_exe().expect("the test binary");
    scratch.seed("installed-by", &format!("{}\n", running.display()));
    scratch.seed("update-check", "1700000000\treleases\t99.0.0\n");
    let listener = proxy_listener();

    assert!(
        bravebot_update::at_startup(None).is_some(),
        "the recorded answer was not read back, so the silence below proves nothing"
    );

    assert!(
        !connected_within(listener, std::time::Duration::from_millis(1500)),
        "an incognito session asked the registry for a version it has nowhere to record"
    );
}

/// INCOG-5, CHECK-11: the standing answer about auto-vetting is the one read this mode refuses.
///
/// Reading the model back is what makes a private session the one somebody configured, because the
/// model decides what it looks like. This answer decides whether they are asked before content
/// nobody vouched for reaches the planner, so a session that inherited it would have stopped asking
/// without having been told to, and the person would never see the question that was not put. The
/// seeded model is read back in the same process so that `None` here means the read was refused
/// rather than that the directory under test was somewhere else.
#[test]
fn the_standing_answer_about_vetting_is_not_read() {
    let scratch = Scratch::incognito("vetting");
    scratch.seed("model", "claude-sonnet-4-5\n");
    scratch.seed("vetting", "on\n");

    assert_eq!(
        store::load_model().as_deref(),
        Some("claude-sonnet-4-5"),
        "the seeded directory is not the one being read"
    );
    assert_eq!(
        store::load_vetting(),
        None,
        "an incognito session inherited a standing answer about auto-vetting"
    );

    // And an answer given here reaches the file no more than a model chosen here does.
    store::save_vetting(false);
    assert_eq!(
        std::fs::read_to_string(scratch.own_directory().join("vetting")).expect("the seeded file"),
        "on\n",
        "an incognito session overwrote the recorded answer about auto-vetting"
    );
    let mut expected = vec!["model".to_string(), "vetting".to_string()];
    expected.sort();
    assert_eq!(scratch.contents(), expected);
}

/// An incognito session sends no usage ping, whatever the switch's file says, and a choice made
/// there does not reach the file. The seeded model is read back so that `None` means the read was
/// refused rather than that the directory under test was somewhere else.
#[test]
fn the_usage_ping_is_off_and_its_switch_is_neither_read_nor_written() {
    let scratch = Scratch::incognito("usage-ping");
    scratch.seed("model", "claude-sonnet-4-5\n");
    scratch.seed("usage-ping", "on\n");

    assert_eq!(
        store::load_model().as_deref(),
        Some("claude-sonnet-4-5"),
        "the seeded directory is not the one being read"
    );
    assert_eq!(store::load_usage_ping(), None, "the switch was read");
    assert_eq!(
        store::usage_ping(),
        bravebot_config::usage_ping::UsagePing::Incognito
    );

    store::save_usage_ping(false);
    assert_eq!(
        std::fs::read_to_string(scratch.own_directory().join("usage-ping"))
            .expect("the seeded file"),
        "on\n",
        "an incognito session overwrote the switch"
    );
    let mut expected = vec!["model".to_string(), "usage-ping".to_string()];
    expected.sort();
    assert_eq!(scratch.contents(), expected);
}

/// An incognito session sends no usage ping, so it says nothing about one and leaves no record that
/// it did: a record written there would stop the next ordinary start telling the person before its
/// first ping. A record already there is not read either.
#[test]
fn the_usage_ping_notice_is_neither_said_nor_recorded() {
    let scratch = Scratch::incognito("usage-ping-notice");
    scratch.seed("model", "claude-sonnet-4-5\n");

    assert_eq!(store::take_usage_ping_notice(), None, "said in incognito");
    store::mark_usage_ping_notice_shown();
    assert_eq!(scratch.contents(), vec!["model".to_string()]);

    scratch.seed("usage-ping-notice", "shown\n");
    assert!(!store::usage_ping_notice_shown(), "the record was read");
}

/// CMD-12: an incognito session neither reads the `/caffeinate` agreement nor writes it.
///
/// The sessions persist, so the only thing between them and the file is the mode: a swapped
/// condition in the read or the write would hand a private session a warning somebody else
/// acknowledged. The seeded model read back first shows the directory is the one being looked at.
#[test]
fn the_caffeinate_agreement_is_neither_read_nor_written() {
    let scratch = Scratch::incognito("caffeinate");
    scratch.seed("model", "claude-sonnet-4-5\n");
    scratch.seed("caffeinate", "confirmed\n");
    assert_eq!(
        store::load_model().as_deref(),
        Some("claude-sonnet-4-5"),
        "the seeded directory is not the one being read"
    );

    let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
    session.adopt_caffeinate();
    session.toggle_caffeinate();
    let said = session.transcript.last().expect("a note").text.clone();
    assert_eq!(
        said,
        bravebot_i18n::t!(caffeinate_explained),
        "an incognito session inherited the agreement"
    );

    // Agreeing here reaches the file no more than reading it did.
    std::fs::remove_file(scratch.own_directory().join("caffeinate")).expect("remove the seed");
    session.toggle_caffeinate();
    let said = session.transcript.last().expect("a note").text.clone();
    assert_eq!(said, bravebot_i18n::t!(caffeinate_on));
    assert_eq!(scratch.contents(), vec!["model".to_string()]);
}

/// INCOG-2: `/language` in an incognito session saves nothing and says so.
///
/// A language applies from the next start and nothing in this session changes with it, so the
/// command's only effects are the file and the note. The session persists, so the only thing
/// between the command and the file is the mode. The recorded language is read back first, so the
/// unchanged file means the command did not reach the directory this test is looking at, and the
/// listing is compared whole, so a write anywhere else under the directory fails the same assertion.
/// The note is the one for a choice that could not be kept; the one for a choice that was kept, and
/// the one for a choice given back, would each be a false report of a write.
#[test]
fn a_language_chosen_is_not_saved_and_the_command_says_so() {
    let scratch = Scratch::incognito("language-command");
    scratch.seed("language", "fr\n");
    let seeded = scratch.contents();
    assert_eq!(
        store::load_language().as_deref(),
        Some("fr"),
        "the seeded directory is not the one being read"
    );

    let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
    bravebot_tui::app::set_language(&mut session, "en-US");
    bravebot_tui::app::set_language(&mut session, "auto");

    let notes: Vec<String> = session
        .transcript
        .iter()
        .map(|entry| entry.text.clone())
        .collect();
    let not_saved = bravebot_i18n::t!(session_language_not_saved).to_string();
    assert_eq!(
        notes,
        vec![not_saved.clone(), not_saved],
        "a language chosen in an incognito session was not said to be unsaved"
    );
    assert!(
        !notes.contains(&bravebot_i18n::t!(session_language_set, language = "en-US").to_string()),
        "the command said a language would be used from the next start"
    );
    assert!(
        !notes.contains(&bravebot_i18n::t!(session_language_auto).to_string()),
        "the command said the choice was given back"
    );
    assert_eq!(
        std::fs::read_to_string(scratch.own_directory().join("language")).expect("the seeded file"),
        "fr\n",
        "an incognito session replaced or removed the recorded language"
    );
    assert_eq!(scratch.contents(), seeded);
}
