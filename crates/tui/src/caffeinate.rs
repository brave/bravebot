//! Holding off system sleep while the session has work pending, for `/caffeinate`.
//!
//! The hold is the platform's own inhibitor program, started with a fixed argument vector and ended
//! by killing it. Nothing from a model, a file or a setting reaches that vector, and the one program
//! it names, on Linux, is an absolute path found where no confined stage could have written, so the
//! program is not one that runs code we did not write
//! ([sandboxing](../../../docs/specs/sandboxing.md)).
//!
//! Only idle sleep is held off. The display can still turn off and the screen can still lock, so a
//! machine left unattended locks as it always did; what it no longer does is stop the request in
//! flight by going to sleep under it.

use std::ffi::OsString;
use std::io;
use std::process::{Child, Command, Stdio};

/// The program that holds the machine awake, as a fixed path where the platform has one.
#[cfg(target_os = "macos")]
pub const PROGRAM: &str = "/usr/bin/caffeinate";

#[cfg(target_os = "windows")]
pub const PROGRAM: &str = "powershell";

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const PROGRAM: &str = "systemd-inhibit";

/// What the inhibitor is given, for the process `pid` it holds the machine awake for.
///
/// `-i` is idle sleep alone, and `-w` ends the hold when this process does, so a crash that skips
/// [`Held`]'s drop cannot leave the machine held awake.
#[cfg(target_os = "macos")]
pub fn arguments(pid: u32) -> io::Result<Vec<OsString>> {
    Ok(vec!["-i".into(), "-w".into(), pid.to_string().into()])
}

/// `cat` holds the inhibitor lock while it reads a pipe only this process writes, so the lock goes
/// when this process does, crash or not. `idle` alone, because logind's `sleep` would also refuse a
/// suspend the person asked for.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn arguments(_pid: u32) -> io::Result<Vec<OsString>> {
    systemd_inhibit_arguments(bravebot_sandbox::programs::find)
}

/// The arguments `systemd-inhibit` is given, with `cat` as the path `find` returns for it.
///
/// `systemd-inhibit` starts its last argument by looking the name up on the `PATH` it inherited
/// from this process, which is not the search SANDBOX-30 describes, so the name is resolved here and
/// passed as an absolute path. Nothing is started when `find` has no `cat`.
#[cfg(any(all(test, unix), not(any(target_os = "macos", target_os = "windows"))))]
fn systemd_inhibit_arguments(
    find: impl Fn(&std::ffi::OsStr) -> Option<std::path::PathBuf>,
) -> io::Result<Vec<OsString>> {
    let cat = find(std::ffi::OsStr::new("cat"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cat: not found"))?;
    let mut arguments: Vec<OsString> = [
        "--what=idle",
        "--who=bravebot",
        "--why=/caffeinate",
        "--mode=block",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    arguments.push(cat.into_os_string());
    Ok(arguments)
}

/// The script sets `ES_CONTINUOUS | ES_SYSTEM_REQUIRED` on its own thread and then reads standard
/// input to its end, which comes when this process closes the pipe or exits. Encoded, so no quoting
/// rule of the shell that receives it can change what runs.
#[cfg(target_os = "windows")]
pub fn arguments(_pid: u32) -> io::Result<Vec<OsString>> {
    use base64::Engine;
    const SCRIPT: &str = "$t = Add-Type -Name Power -Namespace Bravebot -PassThru -MemberDefinition \
                          '[DllImport(\"kernel32.dll\")] public static extern uint \
                          SetThreadExecutionState(uint flags);'; \
                          [void]$t::SetThreadExecutionState([uint32]2147483649); \
                          [void][Console]::In.ReadToEnd()";
    let wide: Vec<u8> = SCRIPT.encode_utf16().flat_map(u16::to_le_bytes).collect();
    Ok(vec![
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-EncodedCommand".into(),
        base64::engine::general_purpose::STANDARD
            .encode(wide)
            .into(),
    ])
}

/// A running inhibitor. Dropping it ends the hold.
#[derive(Debug)]
pub struct Held {
    child: Option<Child>,
}

impl Held {
    /// Whether the inhibitor exited by itself, as `systemd-inhibit` does on a machine with no
    /// logind. A hold that ended is no hold, and the person who asked for one is told.
    pub fn ended(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| !matches!(child.try_wait(), Ok(None)))
    }

    /// A hold with no process behind it, for a test that must not start one.
    #[cfg(test)]
    pub fn stand_in() -> Self {
        Self { child: None }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Start `program` with `arguments`, its standard input a pipe this process holds open.
pub fn start_program(program: &str, arguments: &[OsString]) -> io::Result<Held> {
    let program = bravebot_sandbox::programs::find(std::ffi::OsStr::new(program))
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(Held { child: Some(child) })
}

/// Start the platform's inhibitor for this process.
pub fn start() -> io::Result<Held> {
    start_program(PROGRAM, &arguments(std::process::id())?)
}

/// How a hold is started, so a test can stand in for the platform's program.
pub type Start = fn() -> io::Result<Held>;

/// What a `/caffeinate` came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggled {
    /// The first press: what it does was said, and nothing was turned on.
    Explained,
    /// The press after the explanation: turned on, and the answer is worth keeping.
    Confirmed,
    /// Turned on by someone who had already agreed.
    On,
    /// Turned off, and any hold released.
    Off,
}

/// Why a hold could not be kept.
#[derive(Debug)]
pub enum Unavailable {
    /// The program would not start.
    Missing(io::Error),
    /// The program started and then exited by itself.
    Ended,
}

/// Whether `/caffeinate` is on, and the hold it has where there is work pending.
#[derive(Debug)]
pub struct KeepAwake {
    on: bool,
    /// Whether the person has read the explanation and turned it on once, here or before.
    confirmed: bool,
    /// Whether the explanation has been shown in this session.
    explained: bool,
    held: Option<Held>,
    start: Start,
}

impl Default for KeepAwake {
    fn default() -> Self {
        Self::starting_with(start)
    }
}

impl KeepAwake {
    /// Off, unconfirmed, and starting holds with `start`.
    pub fn starting_with(start: Start) -> Self {
        Self {
            on: false,
            confirmed: false,
            explained: false,
            held: None,
            start,
        }
    }

    /// Record that the person agreed in an earlier session, so the next press turns it on.
    pub fn confirm(&mut self) {
        self.confirmed = true;
    }

    /// Whether `/caffeinate` is on.
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Whether an inhibitor is running.
    pub fn holding(&self) -> bool {
        self.held.is_some()
    }

    /// Carry out one `/caffeinate`.
    ///
    /// The first press in a session that has never agreed explains and changes nothing, so the
    /// warning about a machine left running unlocked is read before the machine is left running.
    pub fn toggle(&mut self) -> Toggled {
        if self.on {
            self.on = false;
            self.held = None;
            return Toggled::Off;
        }
        if self.confirmed {
            self.on = true;
            return Toggled::On;
        }
        if !self.explained {
            self.explained = true;
            return Toggled::Explained;
        }
        self.confirmed = true;
        self.on = true;
        Toggled::Confirmed
    }

    /// Hold the machine awake while `busy`, and release it otherwise.
    ///
    /// A hold that cannot be started or that ended by itself turns `/caffeinate` off, so the
    /// failure is reported once rather than on every pass.
    pub fn follow(&mut self, busy: bool) -> Result<(), Unavailable> {
        if !(self.on && busy) {
            self.held = None;
            return Ok(());
        }
        if let Some(held) = self.held.as_mut() {
            if held.ended() {
                self.on = false;
                self.held = None;
                return Err(Unavailable::Ended);
            }
            return Ok(());
        }
        match (self.start)() {
            Ok(held) => {
                self.held = Some(held);
                Ok(())
            }
            Err(error) => {
                self.on = false;
                Err(Unavailable::Missing(error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stand_in() -> io::Result<Held> {
        Ok(Held::stand_in())
    }

    fn missing() -> io::Result<Held> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    fn on(start: Start) -> KeepAwake {
        let mut keep = KeepAwake::starting_with(start);
        keep.confirm();
        assert_eq!(keep.toggle(), Toggled::On);
        keep
    }

    /// Nothing pending is nothing to hold the machine awake for, and a hold taken whenever the
    /// command is on would keep an idle session's laptop from ever sleeping.
    #[test]
    fn an_idle_session_holds_nothing_while_caffeinate_is_on() {
        let mut keep = on(stand_in);
        keep.follow(false).unwrap();
        assert!(!keep.holding());
    }

    /// The point of the command: work pending is held awake, and the hold goes when the work does.
    #[test]
    fn pending_work_is_held_and_released_once_it_is_done() {
        let mut keep = on(stand_in);
        keep.follow(true).unwrap();
        assert!(keep.holding());
        keep.follow(false).unwrap();
        assert!(!keep.holding());
    }

    /// Off is off whatever is pending, and turning it off mid-turn ends the hold at once.
    #[test]
    fn caffeinate_off_holds_nothing_and_turning_it_off_releases() {
        let mut keep = KeepAwake::starting_with(stand_in);
        keep.follow(true).unwrap();
        assert!(!keep.holding());
        let mut keep = on(stand_in);
        keep.follow(true).unwrap();
        assert_eq!(keep.toggle(), Toggled::Off);
        assert!(!keep.holding());
        keep.follow(true).unwrap();
        assert!(!keep.holding());
    }

    /// A missing program is said, and the command goes off, so a person does not walk away from a
    /// machine they think is held.
    #[test]
    fn a_missing_inhibitor_is_reported_and_turns_caffeinate_off() {
        let mut keep = on(missing);
        assert!(matches!(keep.follow(true), Err(Unavailable::Missing(_))));
        assert!(!keep.is_on());
        keep.follow(true).unwrap();
    }

    /// The warning that the screen still locks and the machine keeps running is read before it is
    /// turned on, once; a person who agreed is not asked again.
    #[test]
    fn the_first_caffeinate_explains_and_the_second_turns_it_on() {
        let mut keep = KeepAwake::starting_with(stand_in);
        assert_eq!(keep.toggle(), Toggled::Explained);
        assert!(!keep.is_on());
        assert_eq!(keep.toggle(), Toggled::Confirmed);
        assert!(keep.is_on());
        assert_eq!(keep.toggle(), Toggled::Off);
        assert_eq!(keep.toggle(), Toggled::On);
    }

    /// The real spawn, against a program that is not there, never passes for a hold. Where the C
    /// library reports the missing program as a child exiting 127 rather than as a spawn error, the
    /// hold has ended, which [`KeepAwake::follow`] reports in the same way.
    #[test]
    fn starting_a_program_that_does_not_exist_fails() {
        let Ok(mut held) = start_program("an-inhibitor-that-does-not-exist", &[]) else {
            return;
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !held.ended() {
            assert!(
                std::time::Instant::now() < deadline,
                "a missing program passed for a hold"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// An inhibitor that exits by itself, as `systemd-inhibit` does with no logind, holds nothing,
    /// and is reported rather than counted as a hold.
    #[cfg(unix)]
    #[test]
    fn an_inhibitor_that_ended_by_itself_is_reported() {
        fn exits() -> io::Result<Held> {
            let mut held = start_program("true", &[])?;
            if let Some(child) = held.child.as_mut() {
                child.wait()?;
            }
            Ok(held)
        }
        let mut keep = on(exits);
        keep.follow(true).unwrap();
        assert!(matches!(keep.follow(true), Err(Unavailable::Ended)));
        assert!(!keep.is_on());
    }

    /// A program that does `body`, for the tests that stand in for programs found on `PATH`.
    #[cfg(unix)]
    fn script(directory: &std::path::Path, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let file = directory.join(name);
        std::fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Start the Linux inhibitor through `start`, which is what a person's `/caffeinate` runs. A
    /// host whose inhibitor is another program starts `systemd-inhibit` with the same arguments.
    #[cfg(unix)]
    fn start_linux_inhibitor() -> io::Result<Held> {
        if cfg!(target_os = "macos") {
            let arguments = systemd_inhibit_arguments(bravebot_sandbox::programs::find)?;
            start_program("systemd-inhibit", &arguments)
        } else {
            start()
        }
    }

    /// Run `test` again in a child whose `PATH` is `path` and whose environment names `session`,
    /// and report whether the child reached the test, whether it passed, and what it printed.
    #[cfg(unix)]
    fn again_with_path(
        test: &str,
        variable: &str,
        session: &std::path::Path,
        path: &std::ffi::OsStr,
    ) -> (bool, bool, String) {
        // nosemgrep: rust.lang.security.current-exe.current-exe
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--test-threads=1", "--nocapture"])
            .env(variable, session)
            .env("PATH", path)
            .output()
            .unwrap();
        (
            session.join("ran").exists(),
            child.status.success(),
            String::from_utf8_lossy(&child.stderr).into_owned(),
        )
    }

    /// SANDBOX-30: the program the inhibitor starts is the installed `cat`, whatever precedes it on
    /// `PATH`. The test binary runs again with `PATH` holding a directory the session keeps out,
    /// with a `cat` in it, ahead of an installed one, and a `systemd-inhibit` that starts its last
    /// argument the way the real one does. A bare `cat`, and a `cat` chosen by `PATH` order alone,
    /// both start the file in the kept-out directory.
    #[cfg(unix)]
    #[test]
    fn the_inhibitor_runs_the_installed_cat_and_never_one_a_stage_may_have_written() {
        use std::path::PathBuf;
        const TEST: &str = "caffeinate::tests::the_inhibitor_runs_the_installed_cat_and_never_one_a_stage_may_have_written";
        const SESSION: &str = "BRAVEBOT_TEST_CAFFEINATE_KEPT_OUT_SESSION";

        if let Some(session) = std::env::var_os(SESSION) {
            let session = PathBuf::from(session);
            bravebot_sandbox::programs::keep_out(&session);
            std::fs::write(session.join("ran"), "").unwrap();

            let mut held = start_linux_inhibitor().unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !held.ended() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the inhibitor never ran its program"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            assert!(
                !session.join("planted-ran").exists(),
                "the inhibitor ran a cat a stage could have written"
            );
            assert!(
                session.join("installed-ran").exists(),
                "the inhibitor did not run the installed cat"
            );
            return;
        }

        let session = crate::testutil::scratch_dir("caffeinate-kept-out-session");
        let system = crate::testutil::scratch_dir("caffeinate-kept-out-system");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&system);
        let planted = session.join(".venv/bin");
        std::fs::create_dir_all(&planted).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        script(
            &planted,
            "cat",
            &format!(": > '{}'", session.join("planted-ran").display()),
        );
        script(
            &system,
            "cat",
            &format!(": > '{}'", session.join("installed-ran").display()),
        );
        script(
            &system,
            "systemd-inhibit",
            "while [ $# -gt 0 ]; do case \"$1\" in --*) shift;; *) break;; esac; done\n\
             exec \"$@\" </dev/null",
        );
        let path = std::env::join_paths([planted, system.clone()]).unwrap();

        let (ran, passed, report) = again_with_path(TEST, SESSION, &session, &path);

        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&system);
        assert!(ran, "the child did not run the test: {report}");
        assert!(passed, "{report}");
    }

    /// SANDBOX-30: with a `cat` only where a stage may write there is no `cat` to give the
    /// inhibitor, and it is not started, rather than started with a bare name that `PATH` order
    /// would resolve to the planted file.
    #[cfg(unix)]
    #[test]
    fn an_inhibitor_with_no_cat_but_a_planted_one_is_not_started() {
        use std::path::PathBuf;
        const TEST: &str =
            "caffeinate::tests::an_inhibitor_with_no_cat_but_a_planted_one_is_not_started";
        const SESSION: &str = "BRAVEBOT_TEST_CAFFEINATE_PLANTED_SESSION";

        if let Some(session) = std::env::var_os(SESSION) {
            let session = PathBuf::from(session);
            bravebot_sandbox::programs::keep_out(&session);
            std::fs::write(session.join("ran"), "").unwrap();

            let error = start_linux_inhibitor()
                .expect_err("a cat found only where a stage may write was given to the inhibitor");
            assert_eq!(error.kind(), io::ErrorKind::NotFound);
            assert!(
                error.to_string().contains("cat"),
                "the report does not name the program that is missing: {error}"
            );
            return;
        }

        let session = crate::testutil::scratch_dir("caffeinate-planted-session");
        let _ = std::fs::remove_dir_all(&session);
        let planted = session.join("node_modules/.bin");
        std::fs::create_dir_all(&planted).unwrap();
        script(&planted, "cat", ":");

        let (ran, passed, report) = again_with_path(TEST, SESSION, &session, planted.as_os_str());

        let _ = std::fs::remove_dir_all(&session);
        assert!(ran, "the child did not run the test: {report}");
        assert!(passed, "{report}");
    }

    /// Idle sleep only, and the program the inhibitor starts is the file `find` returned, byte for
    /// byte. A name that is not UTF-8 and the name its lossy rendering spells are two different
    /// files, and an argument built from the rendering starts the second.
    #[cfg(unix)]
    #[test]
    fn the_inhibitor_holds_idle_sleep_and_starts_the_cat_it_was_found_at_byte_for_byte() {
        use std::cell::RefCell;
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        use std::path::PathBuf;

        let invalid = PathBuf::from(OsStr::from_bytes(b"/opt/w\xff/cat"));
        let lookalike = PathBuf::from("/opt/w\u{FFFD}/cat");
        let asked = RefCell::new(Vec::new());

        let arguments = systemd_inhibit_arguments(|name| {
            asked.borrow_mut().push(name.to_os_string());
            Some(invalid.clone())
        })
        .unwrap();

        assert_eq!(asked.into_inner(), [OsString::from("cat")]);
        assert_ne!(
            arguments.last().map(OsString::as_os_str),
            Some(lookalike.as_os_str())
        );
        let expected: Vec<OsString> = vec![
            "--what=idle".into(),
            "--who=bravebot".into(),
            "--why=/caffeinate".into(),
            "--mode=block".into(),
            invalid.into_os_string(),
        ];
        assert_eq!(arguments, expected);
    }

    /// Idle sleep only, bound to this process: `-d` would keep the display on, which the command
    /// says it does not, and without `-w` a crash would leave the machine held.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_macos_inhibitor_holds_idle_sleep_for_this_process_alone() {
        assert_eq!(arguments(42).unwrap(), ["-i", "-w", "42"]);
    }

    /// A real hold, started and ended, on the platform that runs the tests here.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_platform_inhibitor_starts_and_stops() {
        let mut held = start().unwrap();
        assert!(!held.ended());
        drop(held);
    }
}
