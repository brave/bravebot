//! Where in the workspace a credential's text and a child process meet.
//!
//! [CRED-24] says no credential is passed to a program as an argument. A command line is readable
//! by every account on the machine while the process lives, and nothing at the prompt prevents
//! that, so the rule has to hold in the code. What a test can see of it is where the two are put
//! together: a credential's text is only reachable by naming its accessor, and a program is only
//! started by naming a command. A source file that does both is the one that can put the first
//! into the second, and no file does today.
//!
//! This is a scan of one file at a time. A credential handed to a helper in another file, which
//! then starts the process, is not seen here; it is the known limit of reading source rather than
//! tracing a value.
//!
//! [CRED-24]: ../../../docs/specs/credential-protection.md

use std::path::{Path, PathBuf};

/// What reads a credential's text out of the type that holds it, in `bravebot-config` and in
/// `bravebot-skus`.
const READS_A_CREDENTIAL: &str = ".expose()";

/// What starts a child process.
const STARTS_A_PROCESS: &str = "Command::new(";

/// Every `.rs` file under a directory.
fn sources_under(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a source directory") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            sources_under(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The code of a file that ships, which is everything before its inline test module. An
/// out-of-line `mod testutil;` ends in a semicolon and does not end the shipped code.
fn shipped(source: &str) -> &str {
    let mut from = 0;
    while let Some(found) = source[from..].find("#[cfg(test)]\nmod ") {
        let start = from + found;
        if source[start..]
            .lines()
            .nth(1)
            .is_some_and(|line| line.ends_with('{'))
        {
            return &source[..start];
        }
        from = start + 1;
    }
    source
}

/// CRED-24: no file that ships both reads a credential and starts a process.
///
/// The scan is held to finding something first. A directory layout that moved, or a pattern that
/// matched nothing, would leave it passing over an empty list.
#[test]
fn no_file_that_starts_a_process_reads_a_credential() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates directory")
        .to_path_buf();

    let mut sources = Vec::new();
    for member in std::fs::read_dir(&crates).expect("the crates directory") {
        let source_directory = member.expect("a member").path().join("src");
        if source_directory.is_dir() {
            sources_under(&source_directory, &mut sources);
        }
    }

    let mut starts_a_process = Vec::new();
    let mut reads_a_credential = Vec::new();
    for path in &sources {
        let source = std::fs::read_to_string(path).expect("a source file");
        let code = shipped(&source);
        let name = path
            .strip_prefix(&crates)
            .expect("under the crates directory")
            .display()
            .to_string();
        if code.contains(STARTS_A_PROCESS) {
            starts_a_process.push(name.clone());
        }
        if code.contains(READS_A_CREDENTIAL) {
            reads_a_credential.push(name);
        }
    }

    assert!(
        starts_a_process.len() >= 5 && reads_a_credential.len() >= 5,
        "the scan found {} files that start a process and {} that read a credential, so it is \
         reading less of the workspace than it was written against",
        starts_a_process.len(),
        reads_a_credential.len()
    );
    let together: Vec<_> = starts_a_process
        .iter()
        .filter(|name| reads_a_credential.contains(name))
        .collect();
    assert!(
        together.is_empty(),
        "these files read a credential and start a process, and CRED-24 forbids the first \
         reaching the second as an argument: {together:?}"
    );
}
