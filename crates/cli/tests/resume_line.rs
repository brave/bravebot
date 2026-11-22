//! The line the interface that draws prints when it is left, which repeats the flags the session
//! was started with ([SESSION-8](../../../docs/specs/sessions.md#SESSION-8)).
//!
//! The binary is run in a pty by `contrib/drive_tui.py` against a model service of the test's own,
//! with a home and a checkout of its own, so each flag is read by the real command line scan and
//! the line is the one the person sees. A test that built the flag list by hand would not show
//! that a flag typed on the command line reaches the line.
//!
//! These tests run on Linux and macOS, since a terminal is made with a pty.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[allow(
    dead_code,
    reason = "open_files.rs is shared by several test binaries, and this one uses only bind_mock_server"
)]
#[path = "../../agent/test-support/open_files.rs"]
mod open_files;

/// A home with the settings that send a run to a model service of the test's own, and a checkout
/// to run in.
struct Scratch {
    path: PathBuf,
    port: u16,
}

impl Scratch {
    fn new(name: &str, port: u16) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/resume-line")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("home/.bravebot")).expect("the state directory");
        std::fs::create_dir_all(path.join("work/.git")).expect("a checkout to run in");
        let scratch = Self {
            path: path.canonicalize().expect("a canonical scratch"),
            port,
        };
        scratch.write_home_settings("");
        scratch
    }

    /// The person's own settings file, with `extra` (members of the top-level object, with a comma
    /// in front) after the model service the run is sent to.
    fn write_home_settings(&self, extra: &str) {
        let port = self.port;
        std::fs::write(
            self.path.join("home/.bravebot/settings.json"),
            format!(
                r#"{{
                    "provider": {{
                        "openrouter": {{
                            "env": ["OPENROUTER_API_KEY"],
                            "options": {{"baseURL": "http://127.0.0.1:{port}/api/v1", "maxRetries": 2}}
                        }}
                    }},
                    "model": "openrouter/reasons-only"{extra}
                }}"#
            ),
        )
        .expect("the settings file");
    }

    /// A file of `text` in the scratch directory, outside the checkout, and where it is.
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.path.join(name);
        std::fs::write(&path, text).expect("a file of the test's own");
        path
    }

    /// A directory of the given name in the scratch directory, and where it is.
    fn directory(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        std::fs::create_dir_all(&path).expect("a directory of the test's own");
        path
    }
}

/// `path` as the line prints it. The scratch directories are made of characters a shell leaves
/// alone, so a path is one word as it is; a path that is not fails here rather than in a way that
/// reads as the line being wrong.
fn plain(path: &Path) -> String {
    let text = path.to_str().expect("a path of text").to_string();
    assert!(
        text.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@+:,./-".contains(c)),
        "{text} needs quoting, which these tests do not expect of their own directories"
    );
    text
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn http(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} \r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// One streamed reply: the words, the frame that ends the turn and the sentinel.
fn streamed(reply: &str) -> String {
    let chunk = |delta: &str| {
        format!(
            r#"data: {{"id": "one", "object": "chat.completion.chunk", "model": "reasons-only", "choices": [{delta}]}}"#
        )
    };
    let words = chunk(&format!(
        r#"{{"index": 0, "delta": {{"role": "assistant", "content": "{reply}"}}}}"#
    ));
    let ending = chunk(r#"{"index": 0, "delta": {}, "finish_reason": "stop"}"#);
    let body = format!("{words}\n\n{ending}\n\ndata: [DONE]\n\n");
    format!(
        "HTTP/1.1 200 \r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// A model service on loopback with one model, which answers every chat request "all done". The
/// port it listens on.
fn a_model_service() -> u16 {
    let listing = r#"{"data": [{"id": "reasons-only", "context_length": 262144, "supported_parameters": ["tools"]}]}"#;
    let listener = open_files::bind_mock_server();
    let port = listener.local_addr().expect("an address").port();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().expect("a clone"));
            let mut request = String::new();
            let _ = reader.read_line(&mut request);
            let mut content_length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.trim().eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let reply = match request.starts_with("GET") {
                true => http(200, listing),
                false => streamed("all done"),
            };
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
        }
    });
    port
}

/// What the terminal showed while the binary ran with `arguments` and was sent `script`, one step
/// to a line: how long to wait, then the keys. The driver types the keys at the pace a person does.
fn driven(scratch: &Scratch, arguments: &[&str], script: &str) -> String {
    drive(scratch, arguments, script, None)
}

/// As [`driven`], with what the binary wrote to its standard error kept in a file and not on the
/// terminal, so a line said on one stream can be told from a line said on the other. What the
/// terminal showed, then what standard error held.
fn driven_with_stderr(scratch: &Scratch, arguments: &[&str], script: &str) -> (String, String) {
    let errors = scratch.path.join("standard-error.txt");
    let shown = drive(scratch, arguments, script, Some(&errors));
    (shown, std::fs::read_to_string(&errors).unwrap_or_default())
}

fn drive(scratch: &Scratch, arguments: &[&str], script: &str, errors: Option<&Path>) -> String {
    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contrib/drive_tui.py");
    let mut command = Command::new("/usr/bin/env");
    command
        .env_clear()
        .env("HOME", scratch.path.join("home"))
        .env("PATH", "/usr/bin:/bin")
        .env("TERM", "xterm-256color")
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("BRAVEBOT_USAGE_PING", "0")
        .env("OLLAMA_HOST", "127.0.0.1:1")
        .env("BRAVEBOT_LM_STUDIO_HOST", "127.0.0.1:1")
        .env("LLAMA_ARG_PORT", "1")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1")
        .env("OPENROUTER_API_KEY", "a-token")
        .current_dir(scratch.path.join("work"));
    command.arg("python3").arg(&driver).args(["-", "--"]);
    if let Some(errors) = errors {
        // The shell only moves the binary's standard error to the file and starts it.
        command.env("RESUME_LINE_STANDARD_ERROR", errors).args([
            "/bin/sh",
            "-c",
            r#"exec "$@" 2>"$RESUME_LINE_STANDARD_ERROR""#,
            "sh",
        ]);
    }
    let mut driving = command
        .arg(env!("CARGO_BIN_EXE_bravebot"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 runs the driver");
    driving
        .stdin
        .take()
        .expect("the script")
        .write_all(script.as_bytes())
        .expect("the script is written");
    let driven = driving.wait_with_output().expect("the driver ends");
    String::from_utf8_lossy(&driven.stdout).into_owned()
}

/// The command line printed under the heading that says how to resume, and nothing else.
fn the_line_printed(shown: &str) -> String {
    line_after(shown, "Resume this session with:")
}

/// The command line printed under `heading`, the last time it was said in `shown`.
fn line_after(shown: &str, heading: &str) -> String {
    let (_, after) = shown
        .rsplit_once(heading)
        .unwrap_or_else(|| panic!("no line saying how to resume: {shown}"));
    let lines: Vec<&str> = after.lines().map(str::trim_end).collect();
    let line = lines
        .iter()
        .find(|line| !line.is_empty())
        .unwrap_or_else(|| panic!("the heading names no command: {shown}"));
    line.to_string()
}

/// Waits for the turn a prompt given with `-i` starts, then leaves the interface from an empty
/// box, which takes two presses.
const THE_TURN_THEN_LEAVING: &str = "60\n10 \\x03\n10 \\x03\n";

/// A prompt typed and sent after the question about the directory, then leaving the same way.
const A_PROMPT_THEN_LEAVING: &str = "60 say hi\\r\n10 \\x03\n10 \\x03\n";

/// The question about the directory answered no, then a prompt sent and the interface left. For a
/// session that is not opened on a prompt, and is not told the directory may be trusted.
const A_PROMPT_AFTER_THE_QUESTION: &str = "10 n\n60 say hi\\r\n10 \\x03\n10 \\x03\n";

/// Leaving from an empty box at once, which takes two presses, for a session that was resumed.
const LEAVING_AT_ONCE: &str = "10 \\x03\n10 \\x03\n";

/// The id a printed line names.
fn id_of(line: &str) -> String {
    line.strip_prefix("bravebot --resume ")
        .and_then(|rest| rest.split(' ').next())
        .unwrap_or_else(|| panic!("not a resume line: {line}"))
        .to_string()
}

/// A session that answered the question about the directory and was started with no flag prints
/// the line it always has: the id and nothing after it. A flag that is carried by default would
/// show here.
#[test]
fn a_session_started_with_no_flag_prints_the_id_and_nothing_after_it() {
    let scratch = Scratch::new("none", a_model_service());
    let shown = driven(&scratch, &[], &format!("10 n\n{A_PROMPT_THEN_LEAVING}"));
    let line = the_line_printed(&shown);

    assert_eq!(
        line,
        format!("bravebot --resume {}", id_of(&line)),
        "{shown}"
    );
}

/// The flags the request names, typed on the command line of a session that opened on a prompt
/// with `-i`, are on the line the interface prints when it is left, and the prompt and `-i` are
/// not.
#[test]
fn the_bypass_flag_and_the_sandbox_mode_typed_on_the_command_line_reach_the_line() {
    let scratch = Scratch::new("bypass", a_model_service());
    let shown = driven(
        &scratch,
        &[
            "-i",
            "say hi",
            "--dangerously-skip-permissions",
            "--sandbox",
            "strict",
        ],
        THE_TURN_THEN_LEAVING,
    );
    let line = the_line_printed(&shown);

    assert_eq!(
        line,
        format!(
            "bravebot --resume {} --dangerously-skip-permissions --sandbox strict",
            id_of(&line)
        ),
        "{shown}"
    );
}

/// Every flag that shapes the session reaches the line as the command line resolved it: a flag
/// typed twice once, the last of two values, each path of a list flag, and a value that is not
/// plain as one shell word.
#[test]
fn every_flag_that_shapes_the_session_reaches_the_line_as_the_scan_resolved_it() {
    let scratch = Scratch::new("shaped", a_model_service());
    let shown = driven(
        &scratch,
        &[
            "-i",
            "say hi",
            "--safe",
            "--safe",
            "--dangerously-skip-permissions",
            "--dangerously-skip-permissions",
            "--sandbox",
            "strict",
            "--sandbox",
            "read-only",
            "--run-network",
            "open",
            "--run-network",
            "closed",
            "--sandbox-deny-read",
            "/a",
            "--sandbox-deny-read",
            "/b",
            "--sandbox-deny-write",
            "/tmp/a b's $x",
        ],
        THE_TURN_THEN_LEAVING,
    );
    let line = the_line_printed(&shown);

    assert_eq!(
        line,
        format!(
            "bravebot --resume {} --dangerously-skip-permissions --sandbox read-only \
             --run-network closed --sandbox-deny-read /a --sandbox-deny-read /b \
             --sandbox-deny-write '/tmp/a b'\\''s $x' --safe",
            id_of(&line)
        ),
        "{shown}"
    );
}

/// A session resumed by id carries the flags it was resumed with, and the line names the session's
/// own id once, with neither `--resume` nor the old id repeated.
#[test]
fn a_resumed_session_prints_its_own_id_once_with_the_flags_it_was_resumed_with() {
    let scratch = Scratch::new("resumed", a_model_service());
    let first = driven(
        &scratch,
        &["-i", "say hi", "--dangerously-skip-permissions"],
        THE_TURN_THEN_LEAVING,
    );
    let first = the_line_printed(&first);
    let id = id_of(&first);

    let shown = driven(
        &scratch,
        &[
            "--resume",
            &id,
            "--dangerously-skip-permissions",
            "--sandbox",
            "strict",
            "--system-prompt",
            "be brief",
            "--log-level",
            "info",
        ],
        LEAVING_AT_ONCE,
    );
    let line = the_line_printed(&shown);

    assert_eq!(
        line,
        format!(
            "bravebot --resume {id} --dangerously-skip-permissions --sandbox strict \
             --log-level info --system-prompt 'be brief'"
        ),
        "{shown}"
    );
}

/// `--dangerously-bypass-approvals-and-sandbox` is carried with that spelling and neither as
/// `--dangerously-skip-permissions` nor with a `--sandbox off` the person did not type, beside the
/// flags that go with it: both ways of reaching a path, `--vet`, the tool list and the three that
/// are text. Each value is the one the scan resolved, and the line is the one a pasted command
/// would take back.
#[test]
fn the_bypass_flag_that_unconfines_is_carried_as_typed_beside_the_flags_that_go_with_it() {
    let scratch = Scratch::new("unconfined", a_model_service());
    let read = plain(&scratch.directory("readable"));
    let write = plain(&scratch.directory("writable"));
    let shown = driven(
        &scratch,
        &[
            "-i",
            "say hi",
            "--dangerously-bypass-approvals-and-sandbox",
            "--sandbox-allow-read",
            &read,
            "--sandbox-allow-write",
            &write,
            "--vet",
            "--tools",
            "read_file,search",
            "--system-prompt",
            "be brief",
            "--append-system-prompt",
            "end politely",
            "--log-level",
            "debug",
        ],
        THE_TURN_THEN_LEAVING,
    );
    let line = the_line_printed(&shown);

    assert_eq!(
        line,
        format!(
            "bravebot --resume {} --dangerously-bypass-approvals-and-sandbox \
             --sandbox-allow-read {read} --sandbox-allow-write {write} --vet \
             --tools read_file,search --log-level debug --system-prompt 'be brief' \
             --append-system-prompt 'end politely'",
            id_of(&line)
        ),
        "{shown}"
    );
}

/// `--settings` is carried with the file it named, and a sandbox path that a settings file holds is
/// not: no sandbox path is typed here, the home settings file and the file `--settings` named both
/// hold entries, and the line carries none of them. The lists on the line are the ones typed on the
/// command line. Each of `--settings`, `--log-level`, `--system-prompt` and
/// `--append-system-prompt` is typed twice, and the line carries the last value of each and not the
/// first, as the scan resolved them.
#[test]
fn a_settings_file_typed_is_carried_and_the_sandbox_paths_a_settings_file_holds_are_not() {
    let scratch = Scratch::new("settings", a_model_service());
    scratch.write_home_settings(
        r#", "sandbox": {"filesystem": {"denyWrite": ["/home-file-deny-write"], "allowRead": ["/home-file-allow-read"]}}"#,
    );
    let first = plain(&scratch.file("first-settings.json", "{}"));
    let named = plain(&scratch.file(
        "named-settings.json",
        r#"{"sandbox": {"filesystem": {"denyRead": ["/named-file-deny-read"]}}}"#,
    ));
    let shown = driven(
        &scratch,
        &[
            "-i",
            "say hi",
            "--dangerously-skip-permissions",
            "--settings",
            &first,
            "--settings",
            &named,
            "--log-level",
            "debug",
            "--log-level",
            "info",
            "--system-prompt",
            "first prompt",
            "--system-prompt",
            "second prompt",
            "--append-system-prompt",
            "first ending",
            "--append-system-prompt",
            "second ending",
        ],
        THE_TURN_THEN_LEAVING,
    );
    let line = the_line_printed(&shown);

    assert_eq!(
        line,
        format!(
            "bravebot --resume {} --dangerously-skip-permissions --settings {named} \
             --log-level info --system-prompt 'second prompt' \
             --append-system-prompt 'second ending'",
            id_of(&line)
        ),
        "{shown}"
    );
}

/// `--auto`, `--locked` and `--no-shell` are carried, `--locked` without the `--safe` it also turns
/// on, and a path typed twice for one list flag is carried twice. They go on a session that was not
/// opened on a prompt, since `--auto` and `--locked` refuse the flags the other tests type.
#[test]
fn auto_locked_and_no_shell_are_carried_and_a_path_typed_twice_is_carried_twice() {
    let scratch = Scratch::new("auto-locked", a_model_service());
    let denied = plain(&scratch.directory("denied"));
    let shown = driven(
        &scratch,
        &[
            "--auto",
            "--locked",
            "--no-shell",
            "--sandbox-deny-write",
            &denied,
            "--sandbox-deny-write",
            &denied,
        ],
        A_PROMPT_AFTER_THE_QUESTION,
    );
    let line = the_line_printed(&shown);

    assert_eq!(
        line,
        format!(
            "bravebot --resume {} --auto --sandbox-deny-write {denied} --sandbox-deny-write {denied} \
             --locked --no-shell",
            id_of(&line)
        ),
        "{shown}"
    );
}

/// A session that moved with `/cd` names the directory it moved to, and each relative path it was
/// started with is the absolute path it was read as at the start, so running the line from the new
/// directory configures and confines the places the session was. A relative value that stayed as
/// typed would name a file or a directory of the new directory.
#[test]
fn a_session_moved_with_cd_names_the_relative_paths_it_was_started_with_as_they_were_read() {
    let scratch = Scratch::new("moved", a_model_service());
    let work = scratch.path.join("work");
    scratch.file("work/rel.json", "{}");
    let elsewhere = plain(&scratch.directory("elsewhere"));
    // Moved to by a path relative to the session, which is short enough to be typed as typing, and
    // sent in a step of its own so the Enter is a press.
    let shown = driven(
        &scratch,
        &["--sandbox-deny-write", "out", "--settings", "rel.json"],
        "10 n\n60 say hi\\r\n20 /cd ../elsewhere\n10 \\r\n10 \\x03\n10 \\x03\n",
    );
    let line = line_after(&shown, "Resume it from there with:");
    let work = plain(&work);

    assert!(
        shown.contains(&format!("This session moved to {elsewhere}.")),
        "{shown}"
    );
    assert_eq!(
        line,
        format!(
            "bravebot --resume {} --sandbox-deny-write {work}/out --settings {work}/rel.json",
            id_of(&line)
        ),
        "{shown}"
    );
}

/// A session carried on with `--continue` and one forked with `--fork <id>` print their own id once,
/// with the flags of that start and without `--continue`, `--fork` or the id the fork was made
/// from.
#[test]
fn a_continued_or_forked_session_prints_its_own_id_once_and_not_the_way_it_was_started() {
    let scratch = Scratch::new("continued", a_model_service());
    let first = driven(
        &scratch,
        &["-i", "say hi", "--dangerously-skip-permissions"],
        THE_TURN_THEN_LEAVING,
    );
    let id = id_of(&the_line_printed(&first));

    let continued = driven(&scratch, &["--continue", "--vet"], LEAVING_AT_ONCE);
    assert_eq!(
        the_line_printed(&continued),
        format!("bravebot --resume {id} --vet"),
        "{continued}"
    );

    let forked = driven(&scratch, &["--fork", &id, "--safe"], LEAVING_AT_ONCE);
    let line = the_line_printed(&forked);
    let copy = id_of(&line);
    assert_eq!(line, format!("bravebot --resume {copy} --safe"), "{forked}");
    assert_ne!(copy, id, "the fork is named by the id it was copied from");
}

/// `--incognito` writes no record, so a session started with it prints no line, and one that
/// resumes a record an ordinary session left does print one, with `--incognito` on it, because the
/// session is the one the record came from and running the line should not write what this one did
/// not.
#[test]
fn an_incognito_session_prints_no_line_unless_it_resumed_a_record_and_then_carries_the_flag() {
    let scratch = Scratch::new("incognito", a_model_service());
    let ordinary = driven(
        &scratch,
        &["-i", "say hi"],
        &format!("10 n\n{THE_TURN_THEN_LEAVING}"),
    );
    let id = id_of(&the_line_printed(&ordinary));

    let resumed = driven(&scratch, &["--incognito", "--resume", &id], LEAVING_AT_ONCE);
    assert_eq!(
        the_line_printed(&resumed),
        format!("bravebot --resume {id} --incognito"),
        "{resumed}"
    );

    let fresh = driven(
        &scratch,
        &[
            "-i",
            "say hi",
            "--incognito",
            "--dangerously-skip-permissions",
        ],
        THE_TURN_THEN_LEAVING,
    );
    assert!(
        !fresh.contains("Resume this session with:"),
        "an incognito session that wrote no record named one: {fresh}"
    );
    assert!(
        fresh.contains("turn 1 done"),
        "the turn did not run, so the absence of a line says nothing: {fresh}"
    );
}

/// A hand-over to the background that fails says the line on standard error and not on standard
/// output, with the flags the session was started with. `--dangerously-skip-permissions` is the flag
/// a session can be handed over holding, once the mode key has left bypass; the roster's directory
/// is a file here, so the hand-over cannot start.
#[test]
fn a_hand_over_to_the_background_that_fails_says_the_line_with_the_flags_on_standard_error() {
    let scratch = Scratch::new("handed-over", a_model_service());
    scratch.file(
        "home/.bravebot/jobs",
        "a file where the roster's directory goes",
    );
    let (shown, errors) = driven_with_stderr(
        &scratch,
        &["-i", "say hi", "--dangerously-skip-permissions"],
        "60\n5 \\e[Z\n20 /bg\n10 \\r\n",
    );
    assert!(
        errors.contains("Resume this session with:"),
        "nothing said how to resume on standard error: {errors:?}; the terminal showed: {shown}"
    );
    let line = line_after(&errors, "Resume this session with:");

    assert_eq!(
        line,
        format!(
            "bravebot --resume {} --dangerously-skip-permissions",
            id_of(&line)
        ),
        "{errors}"
    );
    assert!(
        !shown.contains("Resume this session with:"),
        "the line was written to standard output: {shown}"
    );
}
