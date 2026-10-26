//! The text of one crash report, in the JSON Backtrace accepts, built from numbers and words that
//! were fixed in the source (CRASH-1).
//!
//! A report says which build panicked, on what kind of machine, and at which line of which source
//! file. It has no way to say anything else: every value is a number, a word the source spelled, or
//! a path that went through [`source_path`]. The text a panic carries is read only where its type
//! says the source wrote it (`&'static str`), and a message that was formatted at run time has
//! been built from values nobody here can vouch for, so it is left out.

use bravebot_stamp::BUILD;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

/// What a report says about the program that panicked, fixed when the hook is installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Program {
    /// Which executable: `cli`, `host`, `rpc` or `acp`.
    pub(crate) process: &'static str,
    /// How this copy was installed: one of the words [`crate::install_word`] spells.
    pub(crate) install: &'static str,
}

/// Where a panic happened and, where the source wrote it, what it said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Panic {
    file: String,
    line: u32,
    column: u32,
    message: Option<&'static str>,
}

/// The longest message kept. A literal is written by the source, so this is a bound on the size of
/// a report and not on what may be said.
const MESSAGE_LIMIT: usize = 200;

/// The longest path kept.
const PATH_LIMIT: usize = 240;

impl Panic {
    /// A panic at `file`:`line`:`column`. `message` is the payload only where it was a string the
    /// source wrote, which the caller reads off the payload's type.
    pub(crate) fn at(file: &str, line: u32, column: u32, message: Option<&'static str>) -> Self {
        Self {
            file: source_path(file),
            line,
            column,
            message: message.map(|text| shortened(text, MESSAGE_LIMIT)),
        }
    }
}

/// Which report this is: a fresh random identifier and the time it was written.
///
/// Nothing here is kept between reports, so two reports from one machine share no value a reader
/// could join them on (CRASH-10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub(crate) id: String,
    pub(crate) at: u64,
}

impl Identity {
    pub(crate) fn fresh() -> Self {
        Self {
            id: fresh_id(),
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
        }
    }
}

/// The report as the JSON a Backtrace project takes at its `/json` address.
///
/// The one frame is the line that panicked, named by its file and line so that the head of the
/// stack tells one panic site from another. A release binary is stripped, so the frames a stack
/// walk would add name nothing there, and the walk itself is work a panicking process should not
/// begin.
pub(crate) fn document(program: &Program, panic: &Panic, identity: &Identity) -> String {
    let frame = format!("{}:{}", panic.file, panic.line);
    let report = json!({
        "uuid": identity.id,
        "timestamp": identity.at,
        "lang": "Rust",
        "langVersion": "unknown",
        "agent": "bravebot-backtrace",
        "agentVersion": env!("CARGO_PKG_VERSION"),
        "mainThread": "0",
        "classifiers": ["panic"],
        "attributes": attributes(program, panic, identity),
        "threads": {
            "0": {
                "name": "panicking thread",
                "fault": true,
                // The line and column are strings, as the service documents them and as its own Rust
                // client sends them.
                "stack": [{
                    "funcName": frame,
                    "line": panic.line.to_string(),
                    "column": panic.column.to_string(),
                }],
            },
        },
    });
    serde_json::to_string(&report).unwrap_or_default()
}

fn attributes(program: &Program, panic: &Panic, identity: &Identity) -> Value {
    json!({
        "application": "bravebot",
        // A fresh value for each report rather than one for the install, so a count of distinct
        // `guid`s is a count of reports.
        "guid": identity.id,
        "version": env!("CARGO_PKG_VERSION"),
        "build": BUILD,
        "process": program.process,
        "install": program.install,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        // A debug build is a developer's, and the page filters on this to leave those out.
        "debug": cfg!(debug_assertions),
        "panic.file": panic.file,
        "panic.line": panic.line,
        "panic.column": panic.column,
        "error.message": panic.message.unwrap_or("no message the source wrote"),
    })
}

/// A path for a report: what names the source and nothing about whoever compiled it.
///
/// A workspace file is already relative (`crates/agent/src/tools.rs`) and stays as it is. A
/// dependency's is under the builder's cargo registry, which is cut to the crate's own directory,
/// and any other absolute path is cut to its last three components. Only `/rustc/<hash>/...`, the
/// standard library's, keeps its directories, since those name no machine. Written as `?` where
/// anything is not a visible ASCII character or the whole is too long, so a byte somebody chose
/// cannot reach the file.
pub(crate) fn source_path(file: &str) -> String {
    let unified = file.replace('\\', "/");
    let trimmed = match unified.split_once("/registry/src/") {
        Some((_, under_registry)) => under_registry
            .split_once('/')
            .map_or(under_registry, |(_index, within_crate)| within_crate),
        None => unified.as_str(),
    };
    let kept = if is_absolute(trimmed) && !trimmed.starts_with("/rustc/") {
        last_components(trimmed, 3)
    } else {
        trimmed.to_string()
    };
    if kept.is_empty() || kept.len() > PATH_LIMIT || !kept.chars().all(|c| c.is_ascii_graphic()) {
        return "?".to_string();
    }
    kept
}

fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || (bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'/')
}

fn last_components(path: &str, count: usize) -> String {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    parts[parts.len().saturating_sub(count)..].join("/")
}

/// `text` cut to at most `limit` bytes at a character boundary.
fn shortened(text: &'static str, limit: usize) -> &'static str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default()
}

/// A random version 4 UUID, the form Backtrace asks for, from the operating system's generator.
///
/// Where the generator fails the bytes come from the clock and the process, which is poor
/// randomness and enough for the one thing it is used for: the service drops a second report that
/// carries a `uuid` it already holds, so a collision costs a report and nothing else.
fn fresh_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    if rand::rngs::OsRng.try_fill_bytes(&mut bytes).is_err() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        bytes.copy_from_slice(&(now ^ (u128::from(std::process::id()) << 96)).to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = |range: std::ops::Range<usize>| -> String {
        bytes[range]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    };
    format!(
        "{}-{}-{}-{}-{}",
        hex(0..4),
        hex(4..6),
        hex(6..8),
        hex(8..10),
        hex(10..16)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program() -> Program {
        Program {
            process: "cli",
            install: "npm",
        }
    }

    fn parsed(program: &Program, panic: &Panic) -> Value {
        let text = document(program, panic, &Identity::fresh());
        serde_json::from_str(&text).expect("a report is JSON")
    }

    /// CRASH-1: the whole of what a report may say, listed. A field added to the report that this
    /// list does not name is a field somebody decided to send, and fails here until they say so.
    #[test]
    fn a_report_holds_only_the_fields_it_lists() {
        let panic = Panic::at("crates/agent/src/tools.rs", 120, 9, Some("a literal"));
        let report = parsed(&program(), &panic);

        let mut top: Vec<&str> = report
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        top.sort_unstable();
        assert_eq!(
            top,
            [
                "agent",
                "agentVersion",
                "attributes",
                "classifiers",
                "lang",
                "langVersion",
                "mainThread",
                "threads",
                "timestamp",
                "uuid",
            ]
        );

        let mut attributes: Vec<&str> = report["attributes"]
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        attributes.sort_unstable();
        assert_eq!(
            attributes,
            [
                "application",
                "arch",
                "build",
                "debug",
                "error.message",
                "guid",
                "install",
                "os",
                "panic.column",
                "panic.file",
                "panic.line",
                "process",
                "version",
            ]
        );

        let thread = &report["threads"]["0"];
        let mut thread_keys: Vec<&str> = thread
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        thread_keys.sort_unstable();
        assert_eq!(thread_keys, ["fault", "name", "stack"]);
        let frames = thread["stack"].as_array().expect("an array");
        assert_eq!(frames.len(), 1);
        let mut frame_keys: Vec<&str> = frames[0]
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        frame_keys.sort_unstable();
        assert_eq!(frame_keys, ["column", "funcName", "line"]);
    }

    /// The values Backtrace groups and filters on, as they are read back out of the text.
    #[test]
    fn a_report_says_where_the_panic_was_and_what_kind_of_build_it_was() {
        let panic = Panic::at("crates/agent/src/tools.rs", 120, 9, Some("a literal"));
        let report = parsed(&program(), &panic);
        let attributes = &report["attributes"];
        assert_eq!(attributes["panic.file"], "crates/agent/src/tools.rs");
        assert_eq!(attributes["panic.line"], 120);
        assert_eq!(attributes["panic.column"], 9);
        assert_eq!(attributes["error.message"], "a literal");
        assert_eq!(attributes["process"], "cli");
        assert_eq!(attributes["install"], "npm");
        assert_eq!(attributes["application"], "bravebot");
        assert_eq!(attributes["os"], std::env::consts::OS);
        assert_eq!(attributes["arch"], std::env::consts::ARCH);
        assert_eq!(attributes["debug"], cfg!(debug_assertions));
        assert_eq!(attributes["build"], BUILD);
        assert_eq!(report["classifiers"], json!(["panic"]));
        assert_eq!(report["lang"], "Rust");
        // The head of the stack is what the triage view groups by, so it names the line.
        assert_eq!(
            report["threads"]["0"]["stack"][0]["funcName"],
            "crates/agent/src/tools.rs:120"
        );
        // Strings, which is how the service documents a frame's line and column.
        assert_eq!(report["threads"]["0"]["stack"][0]["line"], "120");
        assert_eq!(report["threads"]["0"]["stack"][0]["column"], "9");
        assert_eq!(report["threads"]["0"]["fault"], true);
        assert_eq!(report["mainThread"], "0");
        assert!(report["threads"].get("0").is_some());
    }

    /// CRASH-1: a message built at run time is left out, because what went into it is not known
    /// here. The report says there was none the source wrote rather than saying nothing.
    #[test]
    fn a_message_built_at_run_time_is_not_in_the_report() {
        let panic = Panic::at("crates/agent/src/tools.rs", 1, 1, None);
        let text = document(&program(), &panic, &Identity::fresh());
        let report: Value = serde_json::from_str(&text).expect("JSON");
        assert_eq!(
            report["attributes"]["error.message"],
            "no message the source wrote"
        );
    }

    /// CRASH-1: a message the source wrote is kept, and is kept to a bounded length.
    #[test]
    fn a_literal_message_is_kept_and_a_long_one_is_cut() {
        let kept = Panic::at(
            "a.rs",
            1,
            1,
            Some("called `Option::unwrap()` on a `None` value"),
        );
        assert_eq!(
            kept.message,
            Some("called `Option::unwrap()` on a `None` value")
        );

        // Three bytes to a character and a limit that is not a multiple of three, so the cut falls
        // inside one and has to move back to the boundary before it.
        const LONG: &str = "€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€€";
        assert!(LONG.len() > MESSAGE_LIMIT);
        assert_ne!(MESSAGE_LIMIT % 3, 0);
        let cut = Panic::at("a.rs", 1, 1, Some(LONG));
        let message = cut.message.expect("a message");
        assert_eq!(message.len(), 198);
        assert!(LONG.starts_with(message));
    }

    /// CRASH-1: a path in the workspace is reported as it is.
    #[test]
    fn a_path_in_the_workspace_is_kept() {
        assert_eq!(
            source_path("crates/agent/src/tools.rs"),
            "crates/agent/src/tools.rs"
        );
        assert_eq!(
            source_path(
                "/rustc/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/library/core/src/option.rs"
            ),
            "/rustc/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/library/core/src/option.rs"
        );
    }

    /// CRASH-1: a dependency's source is under the builder's home, and the report names the crate
    /// and not the builder. The deeper paths are the ones that tell this rule from the cut to three
    /// components that any other absolute path gets, which gives the same answer for a file one
    /// directory down and loses the crate's name for one further.
    #[test]
    fn a_path_in_the_cargo_registry_is_cut_to_the_crate() {
        let registry = "/Users/builder/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f";
        assert_eq!(
            source_path(&format!("{registry}/ureq-3.1.0/src/run.rs")),
            "ureq-3.1.0/src/run.rs"
        );
        assert_eq!(
            source_path(&format!("{registry}/rustls-0.23.0/src/crypto/ring/mod.rs")),
            "rustls-0.23.0/src/crypto/ring/mod.rs"
        );
        assert_eq!(
            source_path(&format!(
                "{registry}/gix-odb-0.85.0/src/store/load_index/mod.rs"
            )),
            "gix-odb-0.85.0/src/store/load_index/mod.rs"
        );
        assert_eq!(
            source_path(
                r"C:\Users\builder\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\rustls-0.23.0\src\crypto\ring\mod.rs"
            ),
            "rustls-0.23.0/src/crypto/ring/mod.rs"
        );
    }

    /// CRASH-1: any other absolute path is cut to its tail, whichever way the platform writes it,
    /// so a directory named for somebody's account does not reach the report.
    #[test]
    fn any_other_absolute_path_is_cut_to_its_last_three_components() {
        assert_eq!(
            source_path("/home/somebody/work/bravebot/target/release/build/out/generated.rs"),
            "build/out/generated.rs"
        );
        assert_eq!(
            source_path(r"C:\Users\somebody\work\bravebot\src\main.rs"),
            "bravebot/src/main.rs"
        );
        // Three components or fewer are all kept, which is why a file sitting directly in a home
        // directory is the one shape this does not hide.
        assert_eq!(source_path("/Users/somebody/x.rs"), "Users/somebody/x.rs");
    }

    /// CRASH-1: a path with a byte that is not a visible ASCII character, or one that is far too
    /// long, is written as `?`, as the diagnostic log writes a host it cannot vouch for.
    #[test]
    fn a_path_with_odd_bytes_or_too_long_is_written_as_a_question_mark() {
        assert_eq!(source_path("crates/a b/src/x.rs"), "?");
        assert_eq!(source_path("crates/ü/src/x.rs"), "?");
        assert_eq!(source_path("crates/a\nb/src/x.rs"), "?");
        assert_eq!(source_path(""), "?");
        assert_eq!(source_path(&"a/".repeat(200)), "?");
    }

    /// CRASH-10: nothing in a report is shared with the next one from the same machine, so two
    /// reports cannot be joined by what they say about whoever sent them.
    #[test]
    fn two_reports_from_one_machine_share_no_identifier() {
        let panic = Panic::at("crates/agent/src/tools.rs", 1, 1, None);
        let one: Value =
            serde_json::from_str(&document(&program(), &panic, &Identity::fresh())).expect("JSON");
        let two: Value =
            serde_json::from_str(&document(&program(), &panic, &Identity::fresh())).expect("JSON");
        assert_ne!(one["uuid"], two["uuid"]);
        assert_ne!(one["attributes"]["guid"], two["attributes"]["guid"]);
        // The service reads the first as the report's identity and the second as the sender's, so
        // each is the other's equal and neither outlives its report.
        assert_eq!(one["uuid"], one["attributes"]["guid"]);
    }

    /// The service rejects an identifier that is not a UUID.
    #[test]
    fn the_identifier_is_a_version_four_uuid() {
        let id = fresh_id();
        let groups: Vec<&str> = id.split('-').collect();
        assert_eq!(
            groups.iter().map(|group| group.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(id.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        assert!(groups[2].starts_with('4'));
        assert!(matches!(groups[3].as_bytes()[0], b'8' | b'9' | b'a' | b'b'));
        assert_ne!(fresh_id(), id);
    }
}
