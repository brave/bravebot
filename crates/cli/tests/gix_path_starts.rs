//! SANDBOX-30: nothing reaches the programs `gix-path` starts.
//!
//! `gix-path` 0.13.0 is compiled into bravebot through the gix crates. Its `env` module starts
//! `/bin/sh` (`shell_command`) and `git` (`core_dir` and what is built on it, and
//! `installation_config` and its relatives) by bare name, with the whole environment and the `PATH`
//! the process has, and without `bravebot_sandbox::programs::start`. `clippy.toml` cannot refuse a
//! call inside a dependency, so the clause is held by this file: no crate in the resolved graph
//! that depends on `gix-path`, and no crate of this workspace, names the `env` module.
//!
//! The graph is read with `cargo metadata --locked`, so the crates checked are the ones the lock
//! selects and the directories are where cargo unpacked them, whatever `CARGO_HOME` is. Cargo needs
//! the manifest of every package in the lock, including packages of other platforms and the crates
//! of `fuzz/Cargo.lock` that the test jobs never build, so on a `CARGO_HOME` that lacks one it
//! downloads it. The test then needs the network. A machine that has neither the crates nor the
//! network skips the scan through `skipped`, for both graphs.
//! The scan reads source. It does not see a call made through a macro, a trait object or a name
//! the source computes, and it does not run the binary.
//!
//! `gix-path`'s own source is read as well: the set of functions in it that name `Command` is
//! compared with [`AUDITED_STARTS`], so a raise of `gix-path` that adds a start, or makes a function
//! this file treats as starting nothing start one, fails until the new version is audited.
//!
//! The property is the workspace's, and it lives here because this is the crate the shipped binary
//! is built from.
//!
//! A scan that cannot run is reported by `skipped`: locally it prints why, and where the `CI`
//! environment variable is set it fails.

#![allow(
    clippy::disallowed_methods,
    reason = "the test starts the cargo that is running it, by the path cargo gave this build"
)]

mod shipped;

use serde_json::Value;
use shipped::{code_of, workspace};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The package whose `env` module starts programs.
const CRATE: &str = "gix-path";

/// How the crate is named in source.
const IDENT: &str = "gix_path";

/// The module of `gix-path` that holds every start, as a name in a path.
const STARTING_MODULE: &str = "env";

/// The functions of that module in 0.13.0 that reach no start on any platform: `home_dir` and `var`
/// read the environment and the user database, and `xdg_config` builds a path from the closure it
/// is given. A function not listed here is reported, whether or not it starts anything.
const NON_STARTING: [&str; 3] = ["home_dir", "var", "xdg_config"];

/// Every place in `gix-path` 0.13.0 that names `Command`, as the file under `src` and the item the
/// name is in, read from the source by hand. `fn git_cmd` and `fn shell_command` build the commands,
/// `static GIT_CORE_DIR` builds and runs `git --exec-path` in the closure that initialises it, and
/// `use` is the import at the top of `env/git/mod.rs`. `home_dir`, `var` and `xdg_config` are not
/// here because they name no `Command`, which is what [`NON_STARTING`] relies on. When `gix-path` is
/// raised, the scan reports any difference from this list and the source has to be read again before
/// the list is changed.
const AUDITED_STARTS: [(&str, &str); 4] = [
    ("env/git/mod.rs", "fn git_cmd"),
    ("env/git/mod.rs", "use"),
    ("env/mod.rs", "fn shell_command"),
    ("env/mod.rs", "static GIT_CORE_DIR"),
];

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The identifier `text` opens with.
fn word(text: &str) -> &str {
    let end = text.find(|c: char| !is_word(c)).unwrap_or(text.len());
    &text[..end]
}

/// The comma-separated items of a `{ ... }` group whose opening brace has been taken off, at the
/// top level of the group. Reading stops at the brace that closes it.
fn group_items(group: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth = 0_usize;
    for c in group.chars() {
        match c {
            '{' => {
                depth += 1;
                current.push(c);
            }
            '}' if depth == 0 => break,
            '}' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => items.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    items.push(current);
    items
}

/// What `rest`, which opens with the word `env`, names of that module, or `None` when it names only
/// functions in [`NON_STARTING`]. The module imported whole, a glob, a group item that is not one of
/// them and any other function are reported, since each can reach a start.
fn env_reach(rest: &str) -> Option<String> {
    let after = rest[word(rest).len()..].trim_start();
    let Some(tail) = after.strip_prefix("::") else {
        return Some(STARTING_MODULE.to_string());
    };
    let tail = tail.trim_start();
    if let Some(group) = tail.strip_prefix('{') {
        let named: Vec<String> = group_items(group)
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !NON_STARTING.contains(&word(item)))
            .collect();
        return (!named.is_empty()).then(|| format!("{STARTING_MODULE}::{{{}}}", named.join(", ")));
    }
    if NON_STARTING.contains(&word(tail)) {
        return None;
    }
    let name = if tail.starts_with('*') {
        "*"
    } else {
        word(tail)
    };
    Some(format!("{STARTING_MODULE}::{name}"))
}

/// The ways non-test code in `source` can name the `env` module of `gix-path` other than for the
/// functions in [`NON_STARTING`], one description per use. Other paths of the crate (`realpath`,
/// `RelativePath`, the conversions) start nothing.
///
/// A path through `env`, a grouped import that includes it, a glob, and any use of the crate's name
/// that is not followed by a path (`use gix_path;`, `use gix_path as paths;`, `extern crate`) are
/// all reported: the last three can reach `env` under a name this scan does not follow. Comments
/// and test modules are left out.
fn reaches_env(source: &str) -> Vec<String> {
    let code = code_of(source).without_test_modules;
    let mut found = Vec::new();
    for (at, _) in code.match_indices(IDENT) {
        let after = &code[at + IDENT.len()..];
        if code[..at].chars().next_back().is_some_and(is_word)
            || after.chars().next().is_some_and(is_word)
        {
            continue;
        }
        let Some(path) = after.trim_start().strip_prefix("::") else {
            found.push(format!("{IDENT} named without a path inside it"));
            continue;
        };
        let path = path.trim_start();
        if let Some(group) = path.strip_prefix('{') {
            for item in group_items(group) {
                let item = item.trim();
                if item.starts_with('*') || word(item) == "self" {
                    found.push(format!("{IDENT}::{{{item}}}"));
                } else if word(item) == STARTING_MODULE
                    && let Some(reach) = env_reach(item)
                {
                    found.push(format!("{IDENT}::{{{reach}}}"));
                }
            }
        } else if path.starts_with('*') {
            found.push(format!("{IDENT}::*"));
        } else if word(path) == STARTING_MODULE
            && let Some(reach) = env_reach(path)
        {
            found.push(format!("{IDENT}::{reach}"));
        }
    }
    found
}

/// Where the source of `gix-path` can start a program: the standard library's `Command` is the one
/// way it has to, and the type has to be named whatever path or alias brought it in.
fn starts_a_program(source: &str) -> bool {
    code_of(source).without_test_modules.contains("Command")
}

/// `line` with the contents of string and character literals and a trailing `//` comment taken out,
/// so a brace or the name `Command` inside text is not read as code. A string that continues on the
/// next line is not followed.
fn bare(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    let mut in_string = false;
    while at < chars.len() {
        let c = chars[at];
        if in_string {
            if c == '\\' {
                at += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
                out.push(c);
            }
            at += 1;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.get(at + 1) == Some(&'/') => break,
            '\'' if chars.get(at + 2) == Some(&'\'') => {
                at += 3;
                continue;
            }
            '\'' if chars.get(at + 1) == Some(&'\\') && chars.get(at + 3) == Some(&'\'') => {
                at += 4;
                continue;
            }
            _ => out.push(c),
        }
        at += 1;
    }
    out
}

/// The name a line declares as a function, if it does: the word after the last `fn ` that is not the
/// end of a longer word.
fn fn_name(line: &str) -> Option<String> {
    let mut found = None;
    for (at, _) in line.match_indices("fn ") {
        if line[..at].chars().next_back().is_some_and(is_word) {
            continue;
        }
        let name = word(line[at + 3..].trim_start());
        if !name.is_empty() {
            found = Some(name.to_string());
        }
    }
    found
}

/// What the first line of a top-level item is called in [`AUDITED_STARTS`]: `use` for an import, and
/// the keyword and the name for the rest (`static GIT_CORE_DIR`, `fn git_cmd`).
fn item_label(line: &str) -> String {
    let mut words = line.split(|c: char| !is_word(c)).filter(|each| {
        !each.is_empty() && !["pub", "crate", "super", "in", "unsafe", "async"].contains(each)
    });
    let keyword = words.next().unwrap_or_default();
    match (keyword, words.next()) {
        ("use", _) | (_, None) => keyword.to_string(),
        (_, Some(name)) => format!("{keyword} {name}"),
    }
}

/// The items of `source` that name `Command`, each as `fn name` for the innermost function the name
/// is in (its signature included) and otherwise as the label of the top-level item it is in.
///
/// The attribution counts braces on lines with string literals, character literals and trailing
/// comments taken out, and the lines of comment and test modules that [`code_of`] leaves out. It
/// cannot see a start in a macro body or a macro expansion, code that a `cfg` removes on this
/// platform (the text is read whatever the target, so it does see the Windows branches), a
/// `Command` made by a function in another item that this one calls, a type alias or a re-export
/// that hides the name `Command`, a block comment, or a string literal that continues on another
/// line. It reads `Command` as text, so `CommandExt` is a use of it too.
fn starting_items(source: &str) -> BTreeSet<String> {
    let code = code_of(source).without_test_modules;
    let mut found = BTreeSet::new();
    let mut depth = 0_usize;
    let mut bodies: Vec<(usize, String)> = Vec::new();
    let mut signature: Option<String> = None;
    let mut item: Option<String> = None;
    for raw in code.lines() {
        let line = bare(raw);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if item.is_none() && depth == 0 {
            if trimmed.starts_with('#') {
                continue;
            }
            item = Some(item_label(trimmed));
        }
        if let Some(name) = fn_name(trimmed) {
            signature = Some(name);
        }
        if trimmed.contains("Command") {
            let inside = signature
                .clone()
                .or_else(|| bodies.last().map(|(_, name)| name.clone()));
            found.insert(match inside {
                Some(name) => format!("fn {name}"),
                None => item.clone().unwrap_or_default(),
            });
        }
        for c in trimmed.chars() {
            match c {
                '{' => {
                    depth += 1;
                    if let Some(name) = signature.take() {
                        bodies.push((depth, name));
                    }
                }
                '}' => {
                    while bodies.last().is_some_and(|(inner, _)| *inner == depth) {
                        bodies.pop();
                    }
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        if trimmed.ends_with(';') {
            signature = None;
        }
        if depth == 0 && (trimmed.ends_with('}') || trimmed.ends_with(';')) {
            item = None;
        }
    }
    found
}

/// The difference between the (file, item) pairs `found` in `gix-path` and `audited`, or `None` when
/// they are the same set. A pair in only one of them is reported either way, since a start that went
/// away also means the source was not the source that was read.
fn audit_difference(
    found: &BTreeSet<(String, String)>,
    audited: &[(&str, &str)],
) -> Option<String> {
    let audited: BTreeSet<(String, String)> = audited
        .iter()
        .map(|(file, item)| ((*file).to_string(), (*item).to_string()))
        .collect();
    let describe = |pairs: Vec<&(String, String)>| {
        pairs
            .iter()
            .map(|(file, item)| format!("{file} {item}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let new = describe(found.difference(&audited).collect());
    let gone = describe(audited.difference(found).collect());
    (!new.is_empty() || !gone.is_empty()).then(|| {
        format!("starts not in the audit: [{new}]; audited starts no longer found: [{gone}]")
    })
}

/// A scan that cannot run on this machine says so. On a CI machine it fails instead, since a skip
/// there reports green for a check that never ran. Same rule as `skipped` in
/// `crates/agent/tests/usability.rs`.
fn skipped(why: &str) {
    assert!(
        std::env::var_os("CI").is_none(),
        "a CI machine must run this check, and cannot: {why}"
    );
    eprintln!("skipped: {why}");
}

/// Every file under `directory` with one of the extensions, leaving out a `target` directory.
fn files(directory: &Path, extensions: &[&str], found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                files(&path, extensions, found);
            }
        } else if path
            .extension()
            .is_some_and(|extension| extensions.iter().any(|wanted| extension == *wanted))
        {
            found.push(path);
        }
    }
}

/// The graph cargo resolves for `manifest`, from the lock. Cargo downloads any crate the lock names
/// that `CARGO_HOME` does not hold, so this needs the network where one is missing.
fn graph(manifest: &Path) -> Result<Value, String> {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(manifest)
        .current_dir(workspace())
        .output()
        .map_err(|error| format!("cargo could not be started: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

/// A package of the graph that depends on `gix-path`.
struct Dependent {
    name: String,
    version: String,
    directory: PathBuf,
    /// The names the dependency is given in this package's manifest, where it is not `gix-path`.
    renamed: Vec<String>,
}

fn packages(graph: &Value) -> &[Value] {
    graph["packages"]
        .as_array()
        .expect("cargo metadata lists packages")
}

fn directory_of(package: &Value) -> PathBuf {
    Path::new(package["manifest_path"].as_str().expect("a manifest path"))
        .parent()
        .expect("a manifest is in a directory")
        .to_path_buf()
}

/// The packages other than `gix-path` that declare a dependency on it, of any kind and for any
/// target.
fn dependents(graph: &Value) -> Vec<Dependent> {
    let mut found = Vec::new();
    for package in packages(graph) {
        if package["name"] == CRATE {
            continue;
        }
        let on_it: Vec<&Value> = package["dependencies"]
            .as_array()
            .map(|all| all.iter().filter(|each| each["name"] == CRATE).collect())
            .unwrap_or_default();
        if on_it.is_empty() {
            continue;
        }
        found.push(Dependent {
            name: package["name"].as_str().unwrap_or_default().to_string(),
            version: package["version"].as_str().unwrap_or_default().to_string(),
            directory: directory_of(package),
            renamed: on_it
                .iter()
                .filter_map(|each| each["rename"].as_str())
                .map(str::to_string)
                .collect(),
        });
    }
    found
}

fn gix_path_directories(graph: &Value) -> Vec<PathBuf> {
    packages(graph)
        .iter()
        .filter(|package| package["name"] == CRATE)
        .map(directory_of)
        .collect()
}

/// The two resolved graphs: the workspace and the fuzz workspace, which has its own lock and its own
/// dependencies. A graph that `cargo metadata` cannot read is skipped, with the cause in the
/// message: a crate the lock names is not in `CARGO_HOME` and the network is not reachable, or the
/// lock is out of date.
fn graphs() -> Vec<(&'static str, Value)> {
    let mut found = Vec::new();
    for (label, manifest) in [
        ("the workspace", workspace().join("Cargo.toml")),
        (
            "the fuzz workspace",
            workspace().join("fuzz").join("Cargo.toml"),
        ),
    ] {
        match graph(&manifest) {
            Ok(value) => found.push((label, value)),
            Err(why) => skipped(&format!(
                "{label}: `cargo metadata --locked` could not read its graph, which needs the \
                 crates the lock names in CARGO_HOME or the network to fetch them: {why}"
            )),
        }
    }
    found
}

/// SANDBOX-30: no source of this repository names `gix-path`.
///
/// The crates have no manifest line and no `use` for it. A crate of this tree that asks for
/// `gix-path` is the one way a start in it is reached on purpose, and that is a decision SANDBOX-30
/// has to record before the code lands. The scan reads every Rust source and manifest under
/// `crates/`, `fuzz/` and `third_party/` and the workspace manifest, tests included, and counts a
/// mention anywhere in them, comment or not. The one file it leaves out is this one, which has to
/// name the crate.
///
/// The regressions it rejects are a manifest that adds `gix-path` (or renames it, which the name
/// still shows), and code that calls `gix_path::env::shell_command()` or reaches `git` through
/// `gix_path::env::core_dir()`. It does not reject a dependency that brings the crate in, which
/// the next test reads.
#[test]
fn nothing_in_this_repository_names_gix_path() {
    let root = workspace();
    let this_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("gix_path_starts.rs");
    let mut found = vec![root.join("Cargo.toml")];
    for directory in ["crates", "fuzz", "third_party"] {
        files(&root.join(directory), &["rs", "toml"], &mut found);
    }
    assert!(
        found.len() > 100,
        "the sources were not found: {}",
        found.len()
    );
    let mut naming = Vec::new();
    for file in found.iter().filter(|file| **file != this_file) {
        let text = std::fs::read_to_string(file).expect("a source or manifest file");
        if text.contains(IDENT) || text.contains(CRATE) {
            naming.push(shipped::relative(file));
        }
    }
    assert!(
        naming.is_empty(),
        "SANDBOX-30: gix-path starts /bin/sh and git by bare name from its env module. These name it; \
         record the start in the clause before using the crate:\n{}",
        naming.join("\n")
    );
}

/// SANDBOX-30: no crate in the resolved graph that depends on `gix-path` names its `env` module.
///
/// Each package that declares a dependency on `gix-path`, in either graph, has its Rust source
/// scanned for `gix_path::env`, a group or a glob import that includes it, and a use of the
/// crate's name with no path after it. A dependency renamed in a manifest is reported too, since
/// the source then names it differently. Failing here after a gix crate is raised means the new
/// version calls into the module: read the call, then decide in SANDBOX-30 whether the start is
/// accepted. A package whose directory is not on disk is named in the output as skipped.
///
/// The regression it rejects is a gix crate that starts calling `gix_path::env::core_dir` or
/// `shell_command`, which is how `/bin/sh` or a bare `git` would be started with the whole
/// environment.
#[test]
fn no_crate_that_depends_on_gix_path_names_its_env_module() {
    let mut scanned = 0;
    let mut offending = Vec::new();
    for (label, graph) in graphs() {
        let found = dependents(&graph);
        assert!(
            !found.is_empty(),
            "no package in {label} depends on {CRATE}, so this scan would read nothing"
        );
        for dependent in found {
            if !dependent.directory.is_dir() {
                skipped(&format!(
                    "{} {} in {label}: its source is not at {}",
                    dependent.name,
                    dependent.version,
                    dependent.directory.display()
                ));
                continue;
            }
            for name in &dependent.renamed {
                offending.push(format!(
                    "{} {} in {label}: depends on {CRATE} as `{name}`",
                    dependent.name, dependent.version
                ));
            }
            let mut sources = Vec::new();
            files(&dependent.directory, &["rs"], &mut sources);
            assert!(
                !sources.is_empty(),
                "{} {} has no Rust source at {}, so it was not scanned",
                dependent.name,
                dependent.version,
                dependent.directory.display()
            );
            scanned += 1;
            for file in sources {
                let text = std::fs::read_to_string(&file).unwrap_or_default();
                for how in reaches_env(&text) {
                    offending.push(format!(
                        "{} {} in {label}: {}: {how}",
                        dependent.name,
                        dependent.version,
                        file.strip_prefix(&dependent.directory)
                            .unwrap_or(&file)
                            .display()
                    ));
                }
            }
        }
    }
    if scanned == 0 {
        skipped(&format!(
            "no package that depends on {CRATE} has its source on disk"
        ));
    } else {
        eprintln!("scanned {scanned} packages that depend on {CRATE}");
    }
    assert!(
        offending.is_empty(),
        "SANDBOX-30: these reach the env module of {CRATE}, which starts /bin/sh and git by bare \
         name:\n{}",
        offending.join("\n")
    );
}

/// SANDBOX-30: the programs `gix-path` starts are in its `env` module and nowhere else in it.
///
/// The test above treats the module as the only way to a start. This reads `gix-path` itself and
/// fails where a file outside `src/env` contains the text `Command` in code, whatever path or alias
/// brought the type in. Comments and test modules are left out. It does not check for the standard
/// library's `process` module as such, so `std::process::exit` is accepted, and a start made
/// without naming `Command` is not seen. A version that starts a program from `realpath` or a
/// conversion function through `Command` is not read as harmless.
///
/// The regression it rejects is a raise of `gix-path` that adds a start outside the module the
/// other test watches.
#[test]
fn gix_path_starts_a_program_only_in_its_env_module() {
    let mut checked = 0;
    let mut starting = Vec::new();
    for (label, graph) in graphs() {
        let directories = gix_path_directories(&graph);
        assert!(
            !directories.is_empty(),
            "{label} has no {CRATE} package, so the programs it starts were not looked for"
        );
        for directory in directories {
            let mut sources = Vec::new();
            files(&directory.join("src"), &["rs"], &mut sources);
            if sources.is_empty() {
                skipped(&format!(
                    "{CRATE} in {label}: its source is not at {}",
                    directory.display()
                ));
                continue;
            }
            checked += 1;
            for file in sources {
                let inside = file.starts_with(directory.join("src").join(STARTING_MODULE));
                let text = std::fs::read_to_string(&file).unwrap_or_default();
                if !inside && starts_a_program(&text) {
                    starting.push(format!(
                        "{label}: {}",
                        file.strip_prefix(&directory).unwrap_or(&file).display()
                    ));
                }
            }
        }
    }
    if checked == 0 {
        skipped(&format!("the source of {CRATE} is not on disk"));
    }
    assert!(
        starting.is_empty(),
        "SANDBOX-30: these start a program from outside the env module:\n{}",
        starting.join("\n")
    );
}

/// The (file, item) pairs that name `Command` in `sources`, each a path under `src` with `/` between
/// components and the text of that file.
fn start_pairs(sources: &[(String, String)]) -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    for (file, text) in sources {
        for item in starting_items(text) {
            found.insert((file.clone(), item));
        }
    }
    found
}

/// The `.rs` files under `src`, as a path under it and the text.
fn read_sources(src: &Path) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    files(src, &["rs"], &mut paths);
    paths
        .iter()
        .map(|path| {
            let name = path
                .strip_prefix(src)
                .unwrap_or(path)
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            (name, std::fs::read_to_string(path).unwrap_or_default())
        })
        .collect()
}

/// SANDBOX-30: the functions of `gix-path` that start a program are the ones audited.
///
/// [`NON_STARTING`] exempts `home_dir`, `var` and `xdg_config` because they were read in 0.13.0 and
/// start nothing. Nothing else re-reads them when `gix-path` is raised, so this compares the (file,
/// function) pairs that name `Command` with [`AUDITED_STARTS`]. A new pair, a pair that is gone, and
/// a function in [`NON_STARTING`] that begins to name `Command` all fail it. What the attribution
/// cannot see is listed on [`starting_items`].
///
/// The regression it rejects is a raise of `gix-path` in which a function the audit read as
/// harmless starts a program, or a new function starts one, in a file the other tests accept.
#[test]
fn the_functions_of_gix_path_that_start_a_program_are_the_audited_ones() {
    let mut checked = 0;
    let mut differences = Vec::new();
    for (label, graph) in graphs() {
        let directories = gix_path_directories(&graph);
        assert!(
            !directories.is_empty(),
            "{label} has no {CRATE} package, so the functions it starts programs from were not looked for"
        );
        for directory in directories {
            let sources = read_sources(&directory.join("src"));
            if sources.is_empty() {
                skipped(&format!(
                    "{CRATE} in {label}: its source is not at {}",
                    directory.display()
                ));
                continue;
            }
            checked += 1;
            if let Some(difference) = audit_difference(&start_pairs(&sources), &AUDITED_STARTS) {
                differences.push(format!("{label}, {}: {difference}", directory.display()));
            }
        }
    }
    if checked == 0 {
        skipped(&format!("the source of {CRATE} is not on disk"));
    }
    assert!(
        differences.is_empty(),
        "SANDBOX-30: the functions of {CRATE} that start a program differ from AUDITED_STARTS in \
         crates/cli/tests/gix_path_starts.rs. A new audit is needed: read the source, decide whether \
         each start is accepted, then change AUDITED_STARTS, NON_STARTING and the clause together:\n{}",
        differences.join("\n")
    );
}

/// Source standing for `gix-path` 0.13.0 in the shape the audit read: the starts are `shell_command`,
/// the static that runs `git --exec-path`, the import in `env/git/mod.rs` and `git_cmd`, and the
/// functions in [`NON_STARTING`] start nothing. A brace and the word `Command` in a string and a
/// comment are in it as well, and a test module that names `Command`.
fn audited_shape() -> Vec<(String, String)> {
    vec![
        (
            "env/mod.rs".to_string(),
            "use std::path::Path;

/// Not a start: Command in a doc comment.
pub fn shell_command() -> std::process::Command {
    let mut command = std::process::Command::new(\"sh\");
    command
}

static GIT_CORE_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    let mut cmd = std::process::Command::new(\"git\");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
    }
    cmd.output().ok()?;
    None
});

pub fn home_dir() -> Option<PathBuf> {
    let _unbalanced = \"{ Command\";
    std::env::var_os(\"HOME\").map(Into::into)
}

pub fn var(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

pub fn xdg_config(file: &str) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use std::process::Command;
}
"
            .to_string(),
        ),
        (
            "env/git/mod.rs".to_string(),
            "use std::{
    env,
    process::{Command, Stdio},
};

fn git_cmd(executable: PathBuf, show_scope: bool) -> Command {
    let mut cmd = Command::new(executable);
    cmd
}

pub(super) fn install_config_path() -> Option<&'static BStr> {
    None
}
"
            .to_string(),
        ),
        ("lib.rs".to_string(), "pub fn realpath() {}\n".to_string()),
    ]
}

/// The audit passes on source shaped as the audit read it, with the starts in the places listed.
///
/// The regression it rejects is an attribution that reports a start in a function that has none, or
/// that loses one the audit lists, which would make the real scan fail on the version it was read
/// from or pass on a version it should not.
#[test]
fn the_audit_of_gix_path_passes_on_the_audited_shape() {
    let found = start_pairs(&audited_shape());
    assert_eq!(audit_difference(&found, &AUDITED_STARTS), None, "{found:?}");
    assert_eq!(found.len(), AUDITED_STARTS.len());
}

/// The audit fails when a function the audit did not list starts a program, whether in a file that
/// had a start or in a new one.
///
/// The regression it rejects is a raise of `gix-path` that adds `Command::new` to a function the
/// audit never read, which no other test notices while the file is under `src/env`.
#[test]
fn the_audit_of_gix_path_fails_when_a_new_function_starts_a_program() {
    let mut grown = audited_shape();
    grown[0].1.push_str(
        "\npub fn core_dir_program(name: &str) -> Option<PathBuf> {\n    \
         let c = std::process::Command::new(name);\n    None\n}\n",
    );
    let difference = audit_difference(&start_pairs(&grown), &AUDITED_STARTS)
        .expect("a new starting function was not reported");
    assert!(
        difference.contains("[env/mod.rs fn core_dir_program]"),
        "{difference}"
    );

    let mut elsewhere = audited_shape();
    elsewhere[2].1 = "pub fn run() { std::process::Command::new(\"x\"); }\n".to_string();
    let difference = audit_difference(&start_pairs(&elsewhere), &AUDITED_STARTS)
        .expect("a start in a new file was not reported");
    assert!(difference.contains("[lib.rs fn run]"), "{difference}");
}

/// The audit fails when a function it read as starting nothing begins to, and when an audited start
/// is gone.
///
/// The regression it rejects is [`NON_STARTING`] staying true by default: `home_dir` gaining a
/// `Command` in a later `gix-path` is the case the exemption was written for.
#[test]
fn the_audit_of_gix_path_fails_when_an_audited_non_starting_function_starts_a_program() {
    for name in NON_STARTING {
        let mut changed = audited_shape();
        let marker = format!("pub fn {name}(");
        let at = changed[0]
            .1
            .find(&marker)
            .expect("the function is in the shape");
        let body = at + changed[0].1[at..].find('{').expect("a body") + 1;
        changed[0]
            .1
            .insert_str(body, "\n    std::process::Command::new(\"git\");");
        let difference = audit_difference(&start_pairs(&changed), &AUDITED_STARTS)
            .unwrap_or_else(|| panic!("{name} starting a program was not reported"));
        assert!(
            difference.contains(&format!("[env/mod.rs fn {name}]")),
            "{difference}"
        );
    }

    let mut removed = audited_shape();
    removed[1].1 = "fn git_cmd() {}\n".to_string();
    let difference = audit_difference(&start_pairs(&removed), &AUDITED_STARTS)
        .expect("a start that went away was not reported");
    assert!(
        difference.contains("env/git/mod.rs fn git_cmd"),
        "{difference}"
    );
}

/// The scan has to see each way of reaching the module, and let through the paths of `gix-path`
/// that start nothing and the text that is not code.
///
/// The regression it rejects is a scan that reads only `gix_path::env::` on one line: it would pass
/// a grouped import that spans lines, a glob, an alias and a re-export, and it would pass on a
/// tree that is not scanned at all.
#[test]
fn the_scan_sees_each_way_to_reach_the_env_module_and_not_the_paths_that_start_nothing() {
    for reaching in [
        "let shell = gix_path::env::shell_command();",
        "use gix_path::env;",
        "use gix_path::env::core_dir;",
        "use gix_path::env::{core_dir, shell};",
        "use gix_path::{realpath, env};",
        "use gix_path::{\n    RelativePath,\n    env::installation_config,\n};",
        "use gix_path::{\n    realpath::{self, MAX_SYMLINKS},\n    env as environment,\n};",
        "use gix_path::{self, RelativePath};",
        "use gix_path::*;",
        "use gix_path::{realpath, *};",
        "use gix_path as paths;",
        "use gix_path;",
        "extern crate gix_path;",
        "pub use gix_path::env::shell;",
        "pub use gix_path::env::shell as sh;",
        "let dir = gix_path\n    ::env\n    ::core_dir();",
        "fn f() {}\n#[cfg(test)]\nuse gix_path::env;\nfn g() {}",
        "use gix_path::env::{home_dir, core_dir};",
        "let h = gix_path::env::home_dir; let c = gix_path::env::core_dir();",
        "use gix_path::{env::{self, var}, realpath};",
    ] {
        assert!(
            !reaches_env(reaching).is_empty(),
            "a way to reach the env module was not seen: {reaching}"
        );
    }
    for quiet in [
        "use gix_path::realpath::MAX_SYMLINKS;",
        "use gix_path::RelativePath;",
        "use gix_path::{RelativePath, realpath::MAX_SYMLINKS};",
        "let b = gix_path::into_bstr(path);",
        "let p = gix_path::from_bstr(name);",
        "let p = gix_path::realpath_opts(&dir, cwd, gix_path::realpath::MAX_SYMLINKS)?;",
        "// gix_path::env::shell_command() is what this does not call\nfn f() {}",
        "/// Uses `gix_path::env::core_dir`.\nfn f() {}",
        "fn f() {}\n#[cfg(test)]\nmod tests {\n    use gix_path::env;\n}\nfn g() {}",
        "let xgix_path = 1; let gix_path_len = 2; let env = gix_pathx::env::shell();",
        "let home = std::env::var_os(\"HOME\");",
        "let h = gix_path::env::home_dir();",
        "use gix_path::env::{home_dir, var};",
        "let v = gix_path::env::var(\"PATH\");\nlet x = gix_path::env::xdg_config(\"ignore\", &mut f);",
        "use gix_path::{env::{home_dir, xdg_config}, realpath};",
    ] {
        assert_eq!(
            reaches_env(quiet),
            Vec::<String>::new(),
            "something that does not reach the env module was reported: {quiet}"
        );
    }
}

/// The program-start scan of `gix-path` sees the `Command` type under any path or alias and not a
/// comment or a test module.
///
/// The regression it rejects is a scan that reads only `Command::new(`, which a `use
/// std::process::Command as Spawn;` followed by `Spawn::new` in another file would get past.
#[test]
fn the_scan_of_gix_path_sees_a_command_under_any_name_and_not_a_comment() {
    for starting in [
        "let c = std::process::Command::new(\"git\");",
        "use std::process::Command;",
        "use std::process::Command as Spawn;\nlet c = Spawn::new(\"git\");",
        "fn f() -> std::process::Command { todo() }",
        "use std::os::unix::process::CommandExt;",
    ] {
        assert!(starts_a_program(starting), "{starting}");
    }
    for quiet in [
        "// starts no std::process::Command here\nfn f() {}",
        "let c = std::env::current_dir();",
        "std::process::exit(1);",
        "fn f() {}\n#[cfg(test)]\nmod tests {\n    use std::process::Command;\n}\n",
    ] {
        assert!(!starts_a_program(quiet), "{quiet}");
    }
}

/// A dependency renamed in a manifest is reported, because the source then names the crate by
/// another name and the scan of the source would not find it.
///
/// The regression it rejects is `dependents` reading only the package name of each dependency and
/// ignoring `rename`.
#[test]
fn a_dependency_on_gix_path_under_another_name_is_found_and_reported() {
    let planted = serde_json::json!({
        "packages": [
            {
                "name": "gix-ref",
                "version": "1.0.0",
                "manifest_path": "/registry/gix-ref-1.0.0/Cargo.toml",
                "dependencies": [
                    { "name": "gix-path", "rename": null },
                    { "name": "bstr", "rename": null }
                ]
            },
            {
                "name": "sneaky",
                "version": "0.1.0",
                "manifest_path": "/registry/sneaky-0.1.0/Cargo.toml",
                "dependencies": [ { "name": "gix-path", "rename": "paths" } ]
            },
            {
                "name": "gix-path",
                "version": "1.0.0",
                "manifest_path": "/registry/gix-path-1.0.0/Cargo.toml",
                "dependencies": []
            },
            {
                "name": "unrelated",
                "version": "1.0.0",
                "manifest_path": "/registry/unrelated-1.0.0/Cargo.toml",
                "dependencies": [ { "name": "bstr", "rename": "gix-path" } ]
            }
        ]
    });
    let found = dependents(&planted);
    let names: Vec<&str> = found.iter().map(|each| each.name.as_str()).collect();
    assert_eq!(names, ["gix-ref", "sneaky"]);
    assert!(found[0].renamed.is_empty());
    assert_eq!(found[1].renamed, ["paths"]);
    assert_eq!(found[1].directory, PathBuf::from("/registry/sneaky-0.1.0"));
    assert_eq!(
        gix_path_directories(&planted),
        [PathBuf::from("/registry/gix-path-1.0.0")]
    );
}
