//! Whether the conditions on code and dependencies in this workspace name only an operating system.
//!
//! CI lints one target per operating system the project ships for: the host of the Linux job, the
//! macOS runner and `x86_64-pc-windows-gnu`. Those cover the arms of the other architectures
//! (`aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-pc-windows-gnullvm`) only while no
//! condition depends on anything but the operating system. A `#[cfg(target_arch = "aarch64")]` arm,
//! or one for an operating system with no lint job, is compiled by no check, and the lint
//! `allow_attributes_without_reason` cannot report on it. This fails the commit that adds one, so
//! the change either adds a lint target to CI and `docs/development/checks.md` or does not need the
//! condition.
//!
//! The property belongs to the workspace rather than to any one crate, and it lives here because
//! this is the crate the shipped binary is built from.

use std::path::Path;
use std::path::PathBuf;

/// Conditions that depend on the architecture, the environment or the layout of the target and not
/// on its operating system.
const BEYOND_THE_OPERATING_SYSTEM: [&str; 9] = [
    "target_arch",
    "target_env",
    "target_vendor",
    "target_family",
    "target_pointer_width",
    "target_endian",
    "target_feature",
    "target_abi",
    "target_has_atomic",
];

/// The operating systems that have a lint job.
const LINTED_OPERATING_SYSTEMS: [&str; 3] = ["macos", "linux", "windows"];

/// The conditions in `source` that a lint job per operating system does not cover.
fn uncovered(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for predicate in BEYOND_THE_OPERATING_SYSTEM {
        if source.contains(predicate) {
            found.push(predicate.to_string());
        }
    }
    let mut rest = source;
    while let Some(at) = rest.find("target_os") {
        rest = &rest[at + "target_os".len()..];
        let name = rest
            .trim_start()
            .strip_prefix('=')
            .map(str::trim_start)
            .map(|tail| tail.strip_prefix('\\').unwrap_or(tail))
            .and_then(|tail| tail.strip_prefix('"'))
            .and_then(|tail| tail.split_once('"'))
            .map(|(name, _)| name.trim_end_matches('\\'));
        match name {
            Some(name) if LINTED_OPERATING_SYSTEMS.contains(&name) => {}
            Some(name) => found.push(format!("target_os = \"{name}\"")),
            None => found.push("target_os".to_string()),
        }
    }
    found
}

fn sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a directory under crates/") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                sources(&path, found);
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension == "rs" || extension == "toml")
        {
            found.push(path);
        }
    }
}

#[test]
fn no_condition_in_the_crates_depends_on_more_than_the_operating_system() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("this crate is two directories below the workspace root");
    let this_file = Path::new(file!())
        .file_name()
        .expect("this file has a name");
    let mut files = Vec::new();
    sources(&root.join("crates"), &mut files);
    assert!(
        files.len() > 100,
        "the crates were not found: {}",
        files.len()
    );
    let mut offending = Vec::new();
    for file in files
        .iter()
        .filter(|file| file.file_name() != Some(this_file))
    {
        let source = std::fs::read_to_string(file).expect("a source or manifest file");
        for condition in uncovered(&source) {
            offending.push(format!(
                "{}: {condition}",
                file.strip_prefix(root)
                    .expect("a path under the root")
                    .display()
            ));
        }
    }
    assert!(
        offending.is_empty(),
        "no lint job compiles the arm these conditions select. Add a clippy target for it to CI \
         and to docs/development/checks.md, then list it here:\n{}",
        offending.join("\n")
    );
}

#[test]
fn a_condition_beyond_the_operating_system_is_found_in_every_spelling() {
    let cases = [
        ("#[cfg(target_arch = \"aarch64\")]", "target_arch"),
        ("#[cfg(not(target_env = \"musl\"))]", "target_env"),
        (
            "#[cfg(target_pointer_width = \"32\")]",
            "target_pointer_width",
        ),
        ("#[cfg(target_family = \"wasm\")]", "target_family"),
        (
            "[target.'cfg(target_arch = \"x86_64\")'.dependencies]",
            "target_arch",
        ),
        ("#[cfg(target_os = \"freebsd\")]", "target_os = \"freebsd\""),
        ("#[cfg(target_os=\"android\")]", "target_os = \"android\""),
        ("#[cfg(target_os = \n \"ios\")]", "target_os = \"ios\""),
        ("#[cfg(target_os)]", "target_os"),
    ];
    for (source, expected) in cases {
        assert_eq!(uncovered(source), [expected], "{source}");
    }
}

#[test]
fn the_three_linted_operating_systems_and_the_unix_and_windows_families_are_covered() {
    let covered = [
        "#[cfg(target_os = \"macos\")]",
        "#[cfg(target_os = \"linux\")]",
        "#[cfg(target_os = \"windows\")]",
        "#[cfg(all(unix, not(target_os = \"macos\")))]",
        "#[cfg(windows)]",
        "[target.'cfg(target_os = \"linux\")'.dev-dependencies]",
        // A fixture that holds the attribute in an escaped string literal.
        "\"#[cfg(all(test, target_os = \\\"linux\\\"))]\"",
    ];
    for source in covered {
        assert_eq!(uncovered(source), Vec::<String>::new(), "{source}");
    }
}
