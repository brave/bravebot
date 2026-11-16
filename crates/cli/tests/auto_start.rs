//! AUTO-1 through the built binary: which mode an interactive session opens in.
//!
//! The binary is started in a terminal made with `script`, the question of whether to trust the
//! directory is answered with the key that keeps asking about every write, and the line under the
//! box is read once the first frame has drawn it. A session that asks draws no name for its mode, so
//! the run is told apart by whether `auto mode on` or `bypass permissions on` is on the screen.
//!
//! These tests run on Linux and macOS, as `window_lookup.rs` does.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::path::Path;
use std::time::{Duration, Instant};

mod chatgpt_run;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;
mod terminal;

use chatgpt_run::{Scratch, bravebot, environment, said};
use chatgpt_stand_in::StandIn;
use terminal::{Terminal, in_a_sized_terminal};

const AUTO: &str = "auto mode on";
const BYPASS: &str = "bypass permissions on";
/// The end of the line under the box, which the interface draws after the name of the mode.
const END_OF_THE_LINE: &str = "for shortcuts";
const ON: &str = r#"{"permissions": {"startInAuto": true}}"#;
const PROVIDER: &str =
    r#""model": "held/m", "provider": {"held": {"options": {"baseURL": "http://127.0.0.1:1/v1"}}}"#;

/// A home with a model nothing answers for, and `extra` written into its settings file.
fn home_with(name: &str, extra: &str) -> Scratch {
    let home = Scratch::new(name);
    let permissions = match extra.is_empty() {
        true => String::new(),
        false => format!(", {extra}"),
    };
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        format!("{{{PROVIDER}{permissions}}}"),
    )
    .expect("write the settings");
    home
}

/// What the line under the box said on the first frame of a session started with `arguments`.
fn the_line_under_the_box(home: &Scratch, arguments: &[&str]) -> String {
    let stand_in = StandIn::new(vec![]);
    let mut command = in_a_sized_terminal(arguments);
    environment(&mut command, home, &stand_in);
    let mut terminal = Terminal::start(command);
    let stop = Instant::now() + Duration::from_secs(60);
    let mut answered = false;
    while Instant::now() < stop && !terminal.shown.text().contains(END_OF_THE_LINE) {
        // One key alone, since keys that come together are not an answer.
        if !answered && terminal.shown.text().contains("trust this directory?") {
            terminal.press("n");
            answered = true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let ran = terminal.finish(Duration::from_millis(100));
    assert!(
        ran.shown.contains(END_OF_THE_LINE),
        "no line under the box was drawn for {arguments:?}: {:?}",
        ran.shown
    );
    ran.shown
}

fn opens_in_auto(home: &Scratch, arguments: &[&str]) -> bool {
    let shown = the_line_under_the_box(home, arguments);
    assert!(
        !shown.contains(BYPASS),
        "{arguments:?} opened in bypass: {shown:?}"
    );
    shown.contains(AUTO)
}

/// Store a session in the home's working directory, the way a person's earlier sitting leaves one.
fn store_a_session(home: &Scratch, id: &str) {
    let work = home.path.join("work");
    let project = work.to_str().expect("utf-8");
    let key: String = project
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' => c,
            _ => '-',
        })
        .collect();
    let directory = home.path.join(".bravebot/sessions").join(key);
    std::fs::create_dir_all(&directory).expect("the sessions directory");
    let record = serde_json::json!({
        "id": id, "directory": project, "title": "earlier", "started": 1, "updated": 1,
        "front": "terminal",
        "conversation": {"messages": [{"role": "user", "content": "earlier"}], "context": "trusted"},
    });
    std::fs::write(directory.join(format!("{id}.json")), record.to_string()).expect("the record");
}

/// AUTO-1, MODE-1: `--auto` opens a session in auto and a start without it asks. The faults it
/// rejects are a flag the interface never receives (the session asks), and a default that opens in
/// auto with no flag, which would draw the same line on the second start.
#[test]
fn the_flag_opens_a_session_in_auto_and_a_start_without_it_asks() {
    let home = home_with("cli-auto-start-flag", "");
    assert!(
        opens_in_auto(&home, &["--auto"]),
        "--auto did not open in auto"
    );
    assert!(
        !opens_in_auto(&home, &[]),
        "a start with no flag and no setting opened in auto"
    );
}

/// AUTO-1: the setting in the person's own file, and in the file `--settings` names, opens a session
/// in auto. The faults it rejects are a key the interface never reads, and one read from the home
/// file and not from the named one.
#[test]
fn the_setting_in_the_persons_own_file_and_in_the_named_file_opens_a_session_in_auto() {
    let own = home_with(
        "cli-auto-start-setting-home",
        r#""permissions": {"startInAuto": true}"#,
    );
    assert!(
        opens_in_auto(&own, &[]),
        "the person's own setting did not open in auto"
    );

    let named = home_with("cli-auto-start-setting-named", "");
    let file = named.path.join("named.json");
    std::fs::write(&file, ON).expect("write the named file");
    assert!(
        opens_in_auto(&named, &["--settings", file.to_str().expect("utf-8")]),
        "the setting in the file --settings names did not open in auto"
    );
}

/// AUTO-1: a checkout's `settings.json` and `settings.local.json` that carry the setting open
/// nothing in auto, and `doctor` names each file as one whose setting was not obeyed. The fault it
/// rejects is the key read from a layer that arrives with a clone.
#[test]
fn a_checkouts_setting_opens_nothing_in_auto_and_doctor_names_the_file() {
    let home = home_with("cli-auto-start-setting-checkout", "");
    let layers = home.path.join("work/.bravebot");
    std::fs::create_dir_all(&layers).expect("a project layer");
    std::fs::write(layers.join("settings.json"), ON).expect("project");
    std::fs::write(layers.join("settings.local.json"), ON).expect("local");
    assert!(
        !opens_in_auto(&home, &[]),
        "a checkout's setting opened a session in auto"
    );

    let told = said(&bravebot(&home, &StandIn::new(vec![]), &["doctor"]));
    for file in ["settings.json", "settings.local.json"] {
        assert!(
            told.lines()
                .any(|line| line.contains("permissions.startInAuto")
                    && line.contains("not obeyed")
                    && line.contains(file)),
            "doctor did not name {file}: {told}"
        );
    }
}

/// AUTO-1: a mode typed on the line beats the setting. The bypass flag opens in bypass with the
/// setting on, and `--auto` opens in auto with the setting off. The faults it rejects are the
/// setting overriding the bypass flag and a `false` setting overriding `--auto`.
#[test]
fn a_mode_typed_on_the_line_beats_the_setting() {
    let on = home_with(
        "cli-auto-start-flag-over-setting",
        r#""permissions": {"startInAuto": true}"#,
    );
    let shown = the_line_under_the_box(&on, &["--dangerously-skip-permissions"]);
    assert!(shown.contains(BYPASS), "{shown:?}");
    assert!(!shown.contains(AUTO), "{shown:?}");

    let off = home_with(
        "cli-auto-start-flag-over-off",
        r#""permissions": {"startInAuto": false}"#,
    );
    assert!(opens_in_auto(&off, &["--auto"]));
}

/// AUTO-1, MODE-10: a session resumed or continued keeps no mode of its own, so it opens asking
/// unless the line says `--auto` or the person's setting says so, and then in auto. The faults it
/// rejects are a resume that opens in auto with neither, and one that ignores the flag or the
/// setting.
#[test]
fn a_resumed_session_asks_unless_the_line_or_the_setting_says_auto() {
    let home = home_with("cli-auto-start-resume", "");
    store_a_session(&home, "an-earlier-session");
    assert!(!opens_in_auto(&home, &["--resume", "an-earlier-session"]));
    assert!(opens_in_auto(
        &home,
        &["--resume", "an-earlier-session", "--auto"]
    ));
    assert!(opens_in_auto(&home, &["--auto", "--continue"]));
    assert!(!opens_in_auto(&home, &["--continue"]));

    let setting = home_with(
        "cli-auto-start-resume-setting",
        r#""permissions": {"startInAuto": true}"#,
    );
    store_a_session(&setting, "an-earlier-session");
    assert!(opens_in_auto(&setting, &["--resume", "an-earlier-session"]));
}

/// What the screen showed once a `bravebot://open` link had opened a session and the person had
/// cleared the words it put in the box. The line under the box names a mode only after that, since
/// it says `Prompt from an external link` while the words wait.
fn the_line_under_the_box_after_a_link(home: &Scratch) -> String {
    let stand_in = StandIn::new(vec![]);
    let mut command = in_a_sized_terminal(&["--handle-uri", "bravebot://open?prompt=hello"]);
    environment(&mut command, home, &stand_in);
    let mut terminal = Terminal::start(command);
    let stop = Instant::now() + Duration::from_secs(60);
    let mut answered = false;
    while Instant::now() < stop && !terminal.shown.text().contains("not sent yet") {
        // One key alone, since keys that come together are not an answer.
        if !answered && terminal.shown.text().contains("trust this directory?") {
            terminal.press("n");
            answered = true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        terminal.shown.text().contains("not sent yet"),
        "the link opened no session: {:?}",
        terminal.shown.text()
    );
    for _ in 0.."hello".len() {
        terminal.press("\x7f");
        std::thread::sleep(Duration::from_millis(50));
    }
    // The frames after the words are gone, which are drawn as changes to the frame before, so the
    // name of a mode on the session shows in them as a word of its own.
    std::thread::sleep(Duration::from_secs(1));
    terminal.finish(Duration::from_millis(100)).shown
}

/// LINK-2, AUTO-1: a session a `bravebot://open` link starts asks where the person's own setting
/// opens a session they started themselves in auto. The faults it rejects are a link session that
/// reads the setting, and a setting that opens nothing in auto at all (the first start is the
/// control, which has to open in auto for the second to mean anything).
#[test]
fn a_session_a_link_starts_asks_where_the_setting_opens_others_in_auto() {
    let home = home_with(
        "cli-auto-start-link",
        r#""permissions": {"startInAuto": true}"#,
    );
    assert!(
        opens_in_auto(&home, &[]),
        "the setting did not open a session in auto"
    );
    let shown = the_line_under_the_box_after_a_link(&home);
    assert!(!shown.contains(AUTO), "a link's session opened in auto");
}

/// The files outside the tests that call `Settings::start_in_auto`: the settings themselves, the
/// managed file's report that the key is not read there, the terminal client and the window's
/// bridge. AUTO-1 gives the setting to the two interfaces that open a session for a person, so a
/// one-shot run, a schedule entry, a trigger, `skill eval` and a background session never read it.
const READERS: [&str; 4] = [
    "crates/config/src/managed.rs",
    "crates/config/src/settings.rs",
    "crates/tui/src/app.rs",
    "crates/ui-bridge/src/bridge.rs",
];

/// AUTO-1: nothing but the terminal client and the window's bridge reads the setting. The fault it
/// rejects is a new caller that opens a run, a schedule, a trigger, a skill evaluation or a
/// background session in auto from a person's file, which no clause allows.
#[test]
fn only_the_terminal_client_and_the_windows_bridge_read_the_setting() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut readers = Vec::new();
    let mut pending = vec![root.join("crates")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .expect("a directory")
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && std::fs::read_to_string(&path)
                    .is_ok_and(|text| text.contains(".start_in_auto()"))
            {
                readers.push(
                    path.strip_prefix(&root)
                        .expect("under the root")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    readers.retain(|path| !path.contains("/tests/") && !path.contains("test-support"));
    readers.sort();
    assert_eq!(readers, READERS, "who reads the setting changed");
}
