//! A directory one process recorded as writable by a stage is left out of the search for programs
//! by a process that starts later (SANDBOX-30).
//!
//! The test starts this binary again for each role. `earlier_process` records a directory and
//! appends each line it is handed to a file, as the agent appends to the record in the state
//! directory. `later_process` reads that file with the loader and prints what two lookups find.
//! Both do nothing when no role is named, so a plain run of the suite passes them without effect.
#![cfg(unix)]
#![forbid(unsafe_code)]
#![allow(
    clippy::disallowed_methods,
    reason = "the test starts this test binary again for each role"
)]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_sandbox::programs::{find_on, follow_record, keep_out, load_record};
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "../test-support/scratch_root.rs"]
mod scratch_root;

const ROLE: &str = "BRAVEBOT_RECORD_TEST_ROLE";
const RECORD: &str = "BRAVEBOT_RECORD_TEST_FILE";
const DIRECTORY: &str = "BRAVEBOT_RECORD_TEST_DIRECTORY";
const SEARCHED: &str = "BRAVEBOT_RECORD_TEST_PATH";

/// A program in the recorded directory and in an installed one behind it.
const PLANTED_AND_INSTALLED: &str = "bravebot-across-first";
/// A program in a directory nothing recorded, and nowhere else.
const ONLY_IN_AN_UNRECORDED_DIRECTORY: &str = "bravebot-across-second";

fn role() -> Option<String> {
    std::env::var(ROLE).ok()
}

/// The sink the earlier process hands the record's lines to.
fn append_to_the_record(line: &[u8]) -> std::io::Result<()> {
    let file = std::env::var_os(RECORD).unwrap();
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?
        .write_all(line)
}

/// The earlier process: a session that records a directory a stage may write.
#[test]
fn earlier_process() {
    if role().as_deref() != Some("earlier") {
        return;
    }
    follow_record(append_to_the_record);
    keep_out(Path::new(&std::env::var_os(DIRECTORY).unwrap()));
}

/// A later process: reads the record where it has one, then prints what the lookups find.
fn later(reads_the_record: bool) {
    if reads_the_record {
        let contents = std::fs::read(std::env::var_os(RECORD).unwrap()).unwrap_or_default();
        load_record(&contents);
    }
    let path = std::env::var_os(SEARCHED).unwrap();
    for program in [PLANTED_AND_INSTALLED, ONLY_IN_AN_UNRECORDED_DIRECTORY] {
        let found = find_on(OsStr::new(program), &path)
            .map_or_else(|| "none".to_string(), |found| found.display().to_string());
        println!("found {program}={found}");
    }
}

#[test]
fn later_process_reading_the_record() {
    if role().as_deref() == Some("later") {
        later(true);
    }
}

#[test]
fn later_process_reading_no_record() {
    if role().as_deref() == Some("later-without") {
        later(false);
    }
}

struct Fixture {
    record: PathBuf,
    recorded: PathBuf,
    path: std::ffi::OsString,
    planted: PathBuf,
    installed: PathBuf,
    unrecorded: PathBuf,
}

fn executable(directory: &Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(directory).unwrap();
    let file = directory.join(name);
    std::fs::write(&file, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    file
}

fn fixture(name: &str) -> Fixture {
    let root = scratch_root::scratch_dir(&format!("stage-record-across-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let recorded = root.join("recorded");
    let planted = executable(&recorded.join("bin"), PLANTED_AND_INSTALLED);
    let unrecorded = executable(
        &root.join("unrecorded").join("bin"),
        ONLY_IN_AN_UNRECORDED_DIRECTORY,
    );
    let installed = executable(&root.join("installed").join("bin"), PLANTED_AND_INSTALLED);
    let path = std::env::join_paths([
        recorded.join("bin"),
        root.join("unrecorded").join("bin"),
        root.join("installed").join("bin"),
    ])
    .unwrap();
    Fixture {
        record: root.join("stage-writes"),
        recorded,
        path,
        planted,
        installed,
        unrecorded,
    }
}

/// Start this binary as `role` running `test`, and return what it printed.
fn started_as(role: &str, test: &str, fixture: &Fixture) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(ROLE, role)
        .env(RECORD, &fixture.record)
        .env(DIRECTORY, &fixture.recorded)
        .env(SEARCHED, &fixture.path)
        .output()
        .unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{role} failed: {said}");
    said
}

fn found(said: &str, program: &str) -> String {
    // The harness writes `test <name> ... ` before the first line a test prints, so the marker is
    // looked for inside the line.
    let marker = format!("found {program}=");
    said.lines()
        .find_map(|line| line.split_once(&marker).map(|(_, found)| found))
        .unwrap_or_else(|| panic!("no answer for {program}: {said}"))
        .to_string()
}

/// The record a session writes is what keeps a later process from starting the program a stage
/// planted in the directory the session recorded. The same process without the record finds the
/// planted program, which shows the lookup would take it, and a program in a directory nothing
/// recorded is still found, which shows the record does not leave out more than it holds.
/// Rejects a record that is never written, one that is never read, and one read into a set the
/// lookup does not use.
#[test]
fn a_later_process_leaves_out_a_directory_an_earlier_process_recorded() {
    let fixture = fixture("recorded");

    started_as("earlier", "earlier_process", &fixture);
    assert_eq!(
        std::fs::read_to_string(&fixture.record).unwrap(),
        format!("directory\t{}\n", fixture.recorded.display()),
    );

    let without = started_as("later-without", "later_process_reading_no_record", &fixture);
    assert_eq!(
        found(&without, PLANTED_AND_INSTALLED),
        fixture.planted.display().to_string(),
        "the lookup did not find the planted program with no record: {without}"
    );

    let with = started_as("later", "later_process_reading_the_record", &fixture);
    assert_eq!(
        found(&with, PLANTED_AND_INSTALLED),
        fixture.installed.display().to_string(),
        "the planted program was found with the record read: {with}"
    );
    assert_eq!(
        found(&with, ONLY_IN_AN_UNRECORDED_DIRECTORY),
        fixture.unrecorded.display().to_string(),
        "a directory nothing recorded was left out: {with}"
    );
}

/// A record with nothing usable in it leaves the lookup as it is. Rejects a loader that takes a
/// relative path, a word it does not know or a line with no path as a directory to leave out.
#[test]
fn a_later_process_reading_a_malformed_record_finds_what_it_would_have_found() {
    let fixture = fixture("malformed");
    std::fs::write(
        &fixture.record,
        "garbage\n\ndirectory\t\ndirectory\trecorded\nunknown\t/x\ndirectory /spaced\n",
    )
    .unwrap();

    let said = started_as("later", "later_process_reading_the_record", &fixture);

    assert_eq!(
        found(&said, PLANTED_AND_INSTALLED),
        fixture.planted.display().to_string(),
        "{said}"
    );
}

/// No record is the same as an empty one. Rejects a loader that fails on a missing file.
#[test]
fn a_later_process_with_no_record_file_finds_what_it_would_have_found() {
    let fixture = fixture("missing");
    assert!(!fixture.record.exists());

    let said = started_as("later", "later_process_reading_the_record", &fixture);

    assert_eq!(
        found(&said, PLANTED_AND_INSTALLED),
        fixture.planted.display().to_string(),
        "{said}"
    );
}
