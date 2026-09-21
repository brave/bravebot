//! The library must stay transport-agnostic, and must not write the state directory itself.
//!
//! Greps, which are worth more than a convention nobody re-checks. If the library starts
//! printing, a second front-end inherits lines it cannot intercept and a stray `println!`
//! interleaves with the protocol — the same reason the agent's kernel never prints.

use std::path::Path;

#[test]
fn nothing_below_bin_writes_to_stdout_or_ends_the_process() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offences = Vec::new();

    visit(&source, &mut |path, text| {
        // The transport is the one place allowed to do either.
        if path.components().any(|part| part.as_os_str() == "bin") {
            return;
        }
        for (number, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for forbidden in ["println!", "print!", "std::process::exit", "eprintln!"] {
                if code.contains(forbidden) {
                    offences.push(format!("{}:{}: {forbidden}", path.display(), number + 1));
                }
            }
        }
    });

    assert!(
        offences.is_empty(),
        "the library must not print:\n{}",
        offences.join("\n")
    );
}

/// Nothing here writes into `~/.bravebot` itself: every such write goes through the crate
/// that owns the store, which asks for the modes that directory is kept at.
///
/// The bytes there are one surface's prompts, conversations and standing permissions being read
/// by another, so the modes are the whole of what keeps them to one account. A second writer
/// creating a directory or opening a file for itself would land at the process umask, leaving
/// exactly the files a front end writes most readable by every local account. The mode of a
/// directory that already exists is not changed by creating it again either, so the first such
/// write would decide it for every session after.
#[test]
fn the_state_directory_is_written_only_through_the_crate_that_owns_it() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offences = Vec::new();

    visit(&source, &mut |path, text| {
        for (number, line) in outside_tests(text) {
            let code = line.split("//").next().unwrap_or("");
            for forbidden in [
                "fs::write",
                "fs::create_dir",
                "fs::remove_file",
                "fs::remove_dir",
                "File::create",
                "OpenOptions",
            ] {
                if code.contains(forbidden) {
                    offences.push(format!("{}:{number}: {forbidden}", path.display()));
                }
            }
        }
    });

    assert!(
        offences.is_empty(),
        "a write here would land at the umask rather than the store's modes:\n{}",
        offences.join("\n")
    );
}

/// The crate's own lines, numbered, with every `#[cfg(test)]` module left out.
///
/// A fixture writing a scratch file is not a second writer of the store, and a rule that
/// counted one would be a rule kept by writing fixtures somewhere less readable.
fn outside_tests(text: &str) -> Vec<(usize, &str)> {
    let mut kept = Vec::new();
    let mut depth: Option<i32> = None;
    for (index, line) in text.lines().enumerate() {
        match depth {
            None if line.trim() == "#[cfg(test)]" => depth = Some(0),
            None => kept.push((index + 1, line)),
            Some(level) => {
                let level =
                    level + line.matches('{').count() as i32 - line.matches('}').count() as i32;
                depth = if level <= 0 && line.contains('}') {
                    None
                } else {
                    Some(level)
                };
            }
        }
    }
    kept
}

fn visit(directory: &Path, each: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            visit(&path, each);
        } else if path.extension().is_some_and(|e| e == "rs")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            each(&path, &text);
        }
    }
}
