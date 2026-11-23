//! Crash reports: what a panic leaves on disk, and sending it to Backtrace on a later launch.
//!
//! When a build that was given somewhere to send them panics, one small file is written to the
//! state directory: the version, the kind of machine, and the line of source that failed. The next
//! launch sends it and deletes it. Nothing is said by the process that panicked, nothing waits for
//! the launch that sends, and nothing in a report can be anything a prompt, a file, a key or a
//! server wrote (CRASH-1).
//!
//! # What turns it on and off
//!
//! On, in a build that has an address ([`bravebot_config::crash_report_endpoint`]) and a state
//! directory to leave the file in. Off, for the person, by `BRAVEBOT_CRASH_REPORTS=0` or by
//! `crashReports` set to `false` in their own settings file (CRASH-4). An incognito session is given
//! no state directory, so it leaves nothing and sends nothing (CRASH-5).
//!
//! # What this does not cover
//!
//! A panic, and only that. A fault the process dies of without one (a bad pointer in a library, a
//! stack overflow, being killed) leaves nothing here, and a minidump to cover it would hold the
//! memory of a process that holds credentials ([CRED-23]).
//!
//! [CRED-23]: ../../../docs/specs/credential-protection.md#CRED-23

#![forbid(unsafe_code)]

mod hook;
mod report;
mod send;
mod store;

use bravebot_config::Managed;
use bravebot_config::Settings;
use bravebot_config::crash_reports::{CrashReports, Inputs, resolve};
use bravebot_config::env_var::CRASH_REPORTS;
use bravebot_config::import::Destination;
use bravebot_config::install::{Install, installed_how};
use report::Program;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Which of this program's executables is reporting, so a crash in the desktop's host is told apart
/// from one in the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Process {
    /// `bravebot`, in any of its modes.
    Cli,
    /// `bravebot-host`.
    Host,
    /// `bravebot-rpc`.
    Rpc,
    /// `bravebot-acp`.
    Acp,
}

impl Process {
    /// The word a report says. Fixed in the source, so no value from outside can be one.
    fn word(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Host => "host",
            Self::Rpc => "rpc",
            Self::Acp => "acp",
        }
    }
}

/// How this copy was installed, as the word a report says. `other` is a build from source and
/// anything else nothing here can name.
fn install_word(how: Option<Install>) -> &'static str {
    match how {
        Some(Install::Npm) => "npm",
        Some(Install::Script) => "script",
        Some(Install::PowerShell) => "powershell",
        Some(Install::Homebrew) => "homebrew",
        Some(Install::Winget) => "winget",
        Some(Install::Apt) => "apt",
        Some(Install::Dnf) => "dnf",
        None => "other",
    }
}

/// The state directory of the launch that began reporting, kept for the notice.
static STARTED: OnceLock<PathBuf> = OnceLock::new();

/// Begin reporting for this process, if it is to.
///
/// The directory a report is left in is the one every other write to the state directory asks for
/// (`home::writable`), which is none for an incognito session and for a machine with no home, so
/// the answer about incognito is kept in that one place and no caller passes a directory of its
/// own. Called once, as early as incognito has been engaged and before the first thing that could
/// panic in a way worth reporting.
///
/// Does nothing when the person switched reports off, when the build has nowhere to send them, or
/// when there is no directory ([`plan`] is the whole of that decision).
pub fn start(process: Process) {
    start_with(process, bravebot_config::crash_report_endpoint());
}

/// As [`start`], with the address stated. Exists so the whole of a launch, the state directory and
/// incognito and the switches included, can be run against a server a test owns, since a test build
/// has no address of its own. The product has one answer for it and it is [`start`]'s.
#[doc(hidden)]
pub fn start_with(process: Process, endpoint: Option<String>) {
    let environment = std::env::var(CRASH_REPORTS).ok();
    if let Some((state, endpoint)) = plan(
        bravebot_agent::home::writable(),
        environment.as_deref(),
        endpoint,
        || answered(&Managed::load(), Settings::written_crash_reports),
    ) {
        begin(process, state, endpoint);
    }
}

/// Where a launch leaves reports and sends them, or `None` for a launch that does neither.
///
/// The cheapest question is asked first and the settings file is read last, so a launch that has
/// nowhere to write, that the environment switched off, or whose build has no address never opens
/// it. `setting` is that read, handed in so a test can say whether it was made.
fn plan(
    state: Option<PathBuf>,
    environment: Option<&str>,
    endpoint: Option<String>,
    setting: impl FnOnce() -> Option<bool>,
) -> Option<(PathBuf, String)> {
    let state = state?;
    if !enabled(None, environment) {
        return None;
    }
    let endpoint = endpoint?;
    if !enabled(setting(), environment) {
        return None;
    }
    Some((state, endpoint))
}

/// What the settings say about reports: the managed layer's `false`, and otherwise the person's own
/// file, which is not read once the managed layer has said no (CRASH-4).
///
/// The managed layer can only turn reports off, so its `false` outranks the person's `true` and
/// nothing it writes turns them on.
fn answered(managed: &Managed, own: impl FnOnce() -> Option<bool>) -> Option<bool> {
    match managed.crash_reports_off() {
        true => Some(false),
        false => own(),
    }
}

/// Whether reports are on, given what the person's settings file and the environment said.
///
/// Only a boolean `false` and the exact value `0` turn them off, so a misspelling leaves them on,
/// as it does for the update check, and neither can turn them back on against the other (CRASH-4).
fn enabled(setting: Option<bool>, environment: Option<&str>) -> bool {
    resolve(Inputs {
        variable: environment,
        setting,
        ..Inputs::default()
    })
    .is_on()
}

/// Whether reports are on for this machine and person, and what turned them off if they are not.
///
/// The answer the desktop settings show. Every switch is read, in the order
/// [`bravebot_config::crash_reports::resolve`] ranks them, which is the order [`start`] applies, so
/// the control and the launch cannot disagree. It says nothing about whether this build has an
/// address to send to (CRASH-3): the switch belongs to the person and to every build that shares
/// their state directory.
pub fn current() -> CrashReports {
    current_with(&Managed::load())
}

/// As [`current`], with the managed file stated. Exists so a test can say what the managed layer
/// holds, since [`Managed::load`] reads a fixed path a test cannot redirect. The product has one
/// answer and it is [`current`]'s.
#[doc(hidden)]
pub fn current_with(managed: &Managed) -> CrashReports {
    let environment = std::env::var(CRASH_REPORTS).ok();
    resolve(Inputs {
        incognito: bravebot_agent::home::writable().is_none(),
        managed_off: managed.crash_reports_off(),
        variable: environment.as_deref(),
        setting: Settings::written_crash_reports(),
    })
}

/// Why [`save`] left the settings file as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotSaved {
    /// The session has no state directory to write to.
    NoDirectory,
    /// The file is there and is not a settings document, is too large, or changed while it was
    /// being rewritten, so rewriting it would lose what the person wrote.
    Unwritable,
}

/// Write `crashReports` into the person's own settings file, leaving every other key as it is.
///
/// Through the writer `bravebot mcp enable` and the sign-in flows use, so the file is replaced
/// whole by a rename, through a link where it is one. Only the home layer is written: a checkout's
/// file is neither read nor written for this key (CRASH-4). `true` is written as `true`, which the
/// reader takes as saying nothing, so it turns on nothing a stronger layer turned off.
pub fn save(enabled: bool) -> Result<(), NotSaved> {
    let state = bravebot_agent::home::writable().ok_or(NotSaved::NoDirectory)?;
    save_in(&state, enabled)
}

/// As [`save`], for a named state directory, so a test needs no ambient one.
fn save_in(state: &Path, enabled: bool) -> Result<(), NotSaved> {
    let file = bravebot_config::user_settings_file(state);
    let mut destination = Destination::open(&file).map_err(|_| NotSaved::Unwritable)?;
    destination.set_crash_reports(enabled);
    bravebot_agent::accounts::write_settings(&destination).map_err(|_| NotSaved::Unwritable)
}

/// As [`start`], with the directory and the address stated and no questions asked of the person's
/// settings. Exists so the whole of what a launch does can be run against a server a test owns;
/// the product has one answer for each and it is [`start`]'s.
///
/// The second call in one process does nothing, so a hook is never installed over itself.
#[doc(hidden)]
pub fn begin(process: Process, state: PathBuf, endpoint: String) {
    if STARTED.set(state.clone()).is_err() {
        return;
    }
    let program = Program {
        process: process.word(),
        install: install_word(installed_how()),
    };
    hook::install(program, state.clone());
    send::in_background(state, endpoint);
}

/// Whether this launch is reporting and has not yet told the person so.
///
/// The terminal says it once, on the startup screen, and calls [`notice_given`] when it has
/// (CRASH-9). The desktop window asks the same question through the bridge and records it the same
/// way, so a person told in one front end is not told in the other. False for a launch that is not
/// reporting, so a person who switched it off is told nothing about it.
pub fn notice_due() -> bool {
    STARTED
        .get()
        .is_some_and(|state| !store::notice_was_given(state))
}

/// Record that the person was told, so no later launch says it again.
pub fn notice_given() {
    if let Some(state) = STARTED.get() {
        store::note_notice_given(state);
    }
}

/// Whether this launch is reporting and the person has been told so, by this front end or another.
///
/// False for a launch that is not reporting, so an answer about the notice never says it was shown
/// for reports that are not being sent.
pub fn notice_recorded() -> bool {
    STARTED
        .get()
        .is_some_and(|state| store::notice_was_given(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CRASH-4: either switch is enough, and neither turns reports back on against the other.
    /// Only a boolean `false` and the exact value `0` count, so a misspelling leaves them on.
    #[test]
    fn either_switch_turns_reports_off() {
        assert!(enabled(None, None));
        assert!(enabled(Some(true), None));
        assert!(!enabled(Some(false), None));
        assert!(!enabled(None, Some("0")));
        assert!(!enabled(Some(false), Some("0")));
        // One switch off is off whatever the other says.
        assert!(!enabled(Some(true), Some("0")));
        assert!(!enabled(Some(false), Some("1")));
        for loose in ["", "1", "false", "off", "no", " 0", "0 ", "00"] {
            assert!(enabled(None, Some(loose)), "{loose:?} turned reports off");
        }
    }

    fn managed_saying(text: &str) -> Managed {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let file = dir.path().join("managed.json");
        std::fs::write(&file, text).expect("a managed file");
        Managed::at(&file)
    }

    /// CRASH-4: a managed `crashReports: false` turns reports off whatever the person's file says,
    /// and the person's file is not read once it has. Nothing the managed layer writes turns them
    /// on. The faults this rejects are the person's `true` outranking the pin, the managed layer
    /// having no say, and a managed `true` overriding the person's `false`.
    #[test]
    fn a_managed_false_outranks_the_persons_own_file() {
        let off = managed_saying(r#"{"crashReports": false}"#);
        let never = || -> Option<bool> { panic!("the person's file was read after a managed no") };
        assert_eq!(answered(&off, never), Some(false));
        assert!(!enabled(answered(&off, never), None));

        let silent = managed_saying("{}");
        assert_eq!(answered(&silent, || Some(true)), Some(true));
        assert_eq!(answered(&silent, || Some(false)), Some(false));
        assert_eq!(answered(&silent, || None), None);

        let on = managed_saying(r#"{"crashReports": true}"#);
        assert_eq!(answered(&on, || Some(false)), Some(false));
        assert_eq!(answered(&on, || None), None);

        let quoted = managed_saying(r#"{"crashReports": "false"}"#);
        assert_eq!(answered(&quoted, || Some(true)), Some(true));
    }

    /// CRASH-4: the launch and the window's switch give the same answer for every combination of
    /// the layers, because both rank them through `resolve`. The fault this rejects is one path
    /// ranking a layer differently from the other, such as the launch reporting while the window
    /// shows the switch off, or the reverse.
    #[test]
    fn the_launch_and_the_windows_switch_agree_for_every_combination_of_layers() {
        let managed = [false, true];
        let variables = [None, Some("0"), Some("1"), Some("false")];
        let settings = [None, Some(true), Some(false)];
        for managed_off in managed {
            for variable in variables {
                for setting in settings {
                    let layer = if managed_off {
                        managed_saying(r#"{"crashReports": false}"#)
                    } else {
                        managed_saying("{}")
                    };
                    let launch = plan(
                        Some(PathBuf::from("state")),
                        variable,
                        Some("https://example.invalid/x".to_string()),
                        || answered(&layer, || setting),
                    )
                    .is_some();
                    let window = resolve(Inputs {
                        incognito: false,
                        managed_off: layer.crash_reports_off(),
                        variable,
                        setting,
                    })
                    .is_on();
                    assert_eq!(
                        launch, window,
                        "managed off {managed_off}, variable {variable:?}, setting {setting:?}"
                    );
                }
            }
        }
    }

    fn scratch_file(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let file = bravebot_config::user_settings_file(dir.path());
        std::fs::write(&file, text).expect("a settings file");
        (dir, file)
    }

    /// The window's switch writes the `crashReports` key of the person's own file and nothing else
    /// in it, in a form the launch's reader takes back. The faults this rejects are a write that
    /// drops the other keys, one that writes a string the reader ignores, and one that writes
    /// somewhere the reader does not look.
    #[test]
    fn saving_writes_the_one_key_and_keeps_the_rest_of_the_file() {
        let (state, file) = scratch_file(r#"{"model": "m", "usagePing": false}"#);
        save_in(state.path(), false).expect("written");
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(
            written,
            serde_json::json!({"model": "m", "usagePing": false, "crashReports": false})
        );
        save_in(state.path(), true).expect("written");
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(written["crashReports"], serde_json::json!(true));
        assert_eq!(written["model"], "m");
    }

    /// A file that is not a settings document is left as it was, since rewriting it would lose what
    /// the person wrote, and a state directory with no file gets one holding only the key.
    #[test]
    fn saving_leaves_a_file_that_is_not_a_document_and_makes_a_missing_one() {
        for text in ["// comment\n{}", "[]", "not json"] {
            let (state, file) = scratch_file(text);
            assert_eq!(
                save_in(state.path(), false),
                Err(NotSaved::Unwritable),
                "{text}"
            );
            assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        }
        let state = tempfile::tempdir().unwrap();
        save_in(state.path(), false).expect("written");
        let file = bravebot_config::user_settings_file(state.path());
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        assert_eq!(written, serde_json::json!({"crashReports": false}));
    }

    /// A value the key held that the reader ignores is replaced by the choice.
    #[test]
    fn saving_replaces_a_value_the_reader_ignores() {
        let (state, file) = scratch_file(r#"{"crashReports": "false"}"#);
        save_in(state.path(), false).expect("written");
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        assert_eq!(written, serde_json::json!({"crashReports": false}));
    }

    /// Each executable has a word of its own and each install channel has one, and no two share
    /// it: a report that could not say which it came from would not say where to look.
    #[test]
    fn each_process_and_each_install_has_a_word_of_its_own() {
        let processes =
            [Process::Cli, Process::Host, Process::Rpc, Process::Acp].map(Process::word);
        let mut sorted = processes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), processes.len());

        let installs = [
            Some(Install::Npm),
            Some(Install::Script),
            Some(Install::PowerShell),
            Some(Install::Homebrew),
            Some(Install::Winget),
            Some(Install::Apt),
            Some(Install::Dnf),
            None,
        ]
        .map(install_word);
        let mut sorted = installs.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), installs.len());
        assert_eq!(install_word(None), "other");
        assert_eq!(install_word(Some(Install::Apt)), "apt");
        assert_eq!(install_word(Some(Install::Dnf)), "dnf");
    }

    fn here() -> PathBuf {
        PathBuf::from("/state")
    }

    fn address() -> Option<String> {
        Some("https://submit.backtrace.invalid/brave/token/json".to_string())
    }

    /// CRASH-4: a launch with everything in place reports, to the address the build was given and
    /// into the directory the caller named.
    #[test]
    fn a_launch_with_a_directory_an_address_and_no_switch_reports() {
        assert_eq!(
            plan(Some(here()), None, address(), || None),
            Some((here(), address().unwrap()))
        );
        assert_eq!(
            plan(Some(here()), None, address(), || Some(true)),
            Some((here(), address().unwrap()))
        );
    }

    /// CRASH-5: with no state directory, which is what an incognito session and a machine with no
    /// home are given, nothing starts. The launch itself, with incognito engaged and the directory
    /// the environment names, is `tests/incognito.rs`.
    #[test]
    fn with_no_state_directory_nothing_starts() {
        assert_eq!(plan(None, None, address(), || None), None);
        assert_eq!(plan(None, Some("1"), address(), || Some(true)), None);
        assert!(STARTED.get().is_none());
        assert!(!notice_due());
    }

    /// CRASH-9: a launch that did not start reporting has no record of the notice, and recording
    /// it does nothing, so no answer says the notice was shown for reports that are not sent.
    #[test]
    fn with_nothing_started_no_notice_is_recorded() {
        assert!(STARTED.get().is_none());
        notice_given();
        assert!(!notice_recorded());
    }

    /// CRASH-3: a build that was given nowhere to send reports reports nothing, and does not leave
    /// files that nothing will send.
    #[test]
    fn a_build_with_no_address_reports_nothing() {
        assert_eq!(plan(Some(here()), None, None, || None), None);
    }

    /// CRASH-4: each switch stops a launch that would otherwise report.
    #[test]
    fn either_switch_stops_a_launch_that_would_report() {
        assert_eq!(plan(Some(here()), Some("0"), address(), || None), None);
        assert_eq!(plan(Some(here()), None, address(), || Some(false)), None);
        assert_eq!(
            plan(Some(here()), Some("0"), address(), || Some(true)),
            None
        );
        // A misspelled value is no switch.
        assert!(plan(Some(here()), Some("false"), address(), || None).is_some());
    }

    /// CRASH-4: the settings file is the last thing read. A launch that is already not going to
    /// report, for any other reason, never opens it.
    #[test]
    fn the_settings_file_is_not_read_when_another_answer_has_already_said_no() {
        let read = |why: &str| {
            let why = why.to_string();
            move || -> Option<bool> { panic!("the settings file was read for {why}") }
        };
        assert_eq!(plan(None, None, address(), read("no directory")), None);
        assert_eq!(
            plan(Some(here()), Some("0"), address(), read("the environment")),
            None
        );
        assert_eq!(plan(Some(here()), None, None, read("no address")), None);
    }
}
