//! The Rust source a release build compiles, read as text, for the tests that hold what the shipped
//! code names or builds.
//!
//! A source file is read as a release reads it: the files only a test module declares are left out,
//! and so are the items a test build adds, which are each module of tests and each field, `use`,
//! constant, variant and statement marked for tests, and the comments. A test that reads the
//! source this way fails for what the shipped code does and not for what its tests do.
#![allow(
    dead_code,
    reason = "each test file that includes this module uses some of it"
)]

use std::path::{Path, PathBuf};

/// The workspace root, two levels above this crate's manifest.
pub fn workspace() -> PathBuf {
    // CARGO_MANIFEST_DIR is `<workspace>/crates/cli`, so two pops reach the root.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path
}

/// A path as the repository names it: relative to the workspace and with `/` between components.
pub fn relative(path: &Path) -> String {
    path.strip_prefix(workspace())
        .expect("a path under the workspace")
        .to_string_lossy()
        .replace('\\', "/")
}

/// The body of one clause: everything from its anchor to the next anchor or the next heading.
///
/// Reading to the end of the file instead would take LAYER-4's mention of three crates for a
/// statement LAYER-3 makes, so the bound matters rather than being tidiness.
pub fn clause(spec: &str, id: &str) -> String {
    let anchor = format!("<a id=\"{id}\"></a>");
    let (_, rest) = spec
        .split_once(&anchor)
        .unwrap_or_else(|| panic!("{id} has no anchor in the spec"));
    let mut body = String::new();
    for line in rest.lines().skip(1) {
        if line.starts_with("<a id=") || line.starts_with("## ") {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    body
}

/// The files under `directory`, depth first, in no particular order.
pub fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory).expect("a source directory") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files
}

/// Whether a line gates the item after it on a test build: `#[cfg(test)]`, or `#[cfg(all(test, ...))]`
/// as `#[cfg(all(test, unix))]` does for a test that needs a Unix.
pub fn is_test_gate(line: &str) -> bool {
    let line = line.trim();
    line == "#[cfg(test)]" || line.starts_with("#[cfg(all(test,")
}

/// The name a line declares as a module, `mod name;` or `mod name {`, with any visibility before
/// it.
pub fn module_name(line: &str) -> Option<&str> {
    let mut rest = line.trim();
    for visibility in ["pub(crate) ", "pub(super) ", "pub "] {
        if let Some(after) = rest.strip_prefix(visibility) {
            rest = after;
            break;
        }
    }
    rest.strip_prefix("mod ")?
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .filter(|name| !name.is_empty())
}

/// The modules a source file gates with a test configuration and declares without a body, as the
/// `#[path]` it gives, if any, and the name.
pub fn declared_test_modules(source: &str) -> Vec<(Option<String>, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        if !is_test_gate(line) {
            continue;
        }
        let mut path = None;
        let mut next = at + 1;
        while let Some(attribute) = lines
            .get(next)
            .map(|attribute| attribute.trim())
            .filter(|attribute| attribute.starts_with("#["))
        {
            if let Some(value) = attribute
                .strip_prefix("#[path = \"")
                .and_then(|value| value.strip_suffix("\"]"))
            {
                path = Some(value.to_string());
            }
            next += 1;
        }
        if let Some(declared) = lines.get(next).filter(|l| l.trim_end().ends_with(';'))
            && let Some(name) = module_name(declared)
        {
            found.push((path, name.to_string()));
        }
    }
    found
}

/// The files the compiler reads a module declared in `declaring` from: the `#[path]` when there is
/// one, and otherwise `name.rs` and `name/mod.rs` beside it, or inside the directory named for it
/// when `declaring` is not the root of a crate or a `mod.rs`.
pub fn module_files(declaring: &Path, path: Option<&str>, name: &str) -> Vec<PathBuf> {
    let directory = declaring.parent().expect("a source file has a directory");
    if let Some(path) = path {
        return vec![directory.join(path)];
    }
    let stem = declaring
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let base = if matches!(stem, "lib" | "main" | "mod") {
        directory.to_path_buf()
    } else {
        directory.join(stem)
    };
    vec![
        base.join(format!("{name}.rs")),
        base.join(name).join("mod.rs"),
    ]
}

/// How far `line` is indented, in bytes.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// What a test gate took out of a source file.
pub struct Hidden {
    /// The line of the attribute.
    pub line: usize,
    /// The first line of the item it is on, after the attribute's own companions.
    pub item: String,
    /// Every line taken out, the attributes among them.
    pub text: String,
}

/// A source file as a release build reads it: the lines that are code, with their numbers, and what
/// was taken out.
pub struct Source {
    /// The file.
    pub path: PathBuf,
    /// The path of the file from the workspace root.
    pub name: String,
    /// The lines of code, joined, with every item marked for tests taken out.
    pub code: String,
    /// The lines of code, joined, with only the test modules taken out: an item marked for tests that
    /// is not a module stays. A guard that reads more than a release compiles can only be stricter,
    /// so the guards of what a release may name read this and not [`Source::code`].
    pub without_test_modules: String,
    /// The number each line of `code` has in the file.
    pub numbers: Vec<usize>,
    pub hidden: Vec<Hidden>,
}

/// The code of a source file text: the text without its items marked for tests, which is each module
/// of tests and each statement a test build adds. A line of comment is left out as well, since a
/// doc that names a call is not one.
///
/// The item an attribute is on ends where `rustfmt` puts its end, so nothing is counted and a
/// bracket in a string does not matter. An item that fits on a line, a field or a `use` or a
/// constant, ends there, at its `;` or its `,`, and so does a block that closes on the line that
/// opens it. One that opens a block ends at the line of the same indentation that closes it, and
/// one that wraps its arguments ends at the line that closes them. An attribute on a field must not
/// take out the fields and the code after it.
pub fn code_of(text: &str) -> Source {
    let mut kept: Vec<(usize, &str)> = Vec::new();
    let mut with_gated_items: Vec<&str> = Vec::new();
    let mut hidden = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((number, line)) = lines.next() {
        if !is_test_gate(line) {
            if !line.trim_start().starts_with("//") {
                kept.push((number + 1, line));
                with_gated_items.push(line);
            }
            continue;
        }
        let at = indent(line);
        let mut taken = vec![line];
        let mut first = None;
        for (_, item) in lines.by_ref() {
            taken.push(item);
            if item.trim_start().starts_with("#[") {
                continue;
            }
            first = Some(item);
            break;
        }
        let Some(first) = first else { break };
        let one_line = first.trim_end().ends_with([';', ','])
            || (first.trim_end().ends_with('}')
                && first.matches('{').count() == first.matches('}').count());
        if !one_line {
            for (_, item) in lines.by_ref() {
                taken.push(item);
                let trimmed = item.trim();
                let closes = indent(item) == at
                    && (trimmed.starts_with('}')
                        || trimmed.ends_with(';')
                        || (trimmed.starts_with([')', ']']) && trimmed.ends_with([',', ';'])));
                if closes {
                    break;
                }
            }
        }
        if module_name(first).is_none() {
            with_gated_items.extend(
                taken
                    .iter()
                    .filter(|line| !line.trim_start().starts_with("//")),
            );
        }
        hidden.push(Hidden {
            line: number + 1,
            item: first.trim().to_string(),
            text: taken.join("\n"),
        });
    }
    let code = kept
        .iter()
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n");
    let numbers = kept.iter().map(|(number, _)| *number).collect();
    Source {
        path: PathBuf::new(),
        name: String::new(),
        code,
        without_test_modules: with_gated_items.join("\n"),
        numbers,
        hidden,
    }
}

/// Every file under a crate's `src` that a release build compiles, as the code it contributes. A
/// file that only a test module declares is a test and is left out with the files under its
/// directory, and the test items and comments of the others are left out of their text.
pub fn sources() -> Vec<Source> {
    let mut texts = Vec::new();
    for krate in std::fs::read_dir(workspace().join("crates")).expect("the crates directory") {
        let source = krate.expect("a crate").path().join("src");
        if !source.is_dir() {
            continue;
        }
        for file in rust_files(&source) {
            let text = std::fs::read_to_string(&file).expect("a source file");
            texts.push((file, text));
        }
    }
    let mut test_files = Vec::new();
    let mut test_directories = Vec::new();
    for (file, text) in &texts {
        for (path, name) in declared_test_modules(text) {
            for candidate in module_files(file, path.as_deref(), &name) {
                if !candidate.is_file() {
                    continue;
                }
                if candidate.file_name().is_some_and(|name| name == "mod.rs") {
                    test_directories
                        .push(candidate.parent().map(PathBuf::from).unwrap_or_default());
                } else {
                    test_directories.push(candidate.with_extension(""));
                }
                test_files.push(candidate);
            }
        }
    }
    let found: Vec<Source> = texts
        .into_iter()
        .filter(|(file, _)| {
            !test_files.contains(file)
                && !test_directories
                    .iter()
                    .any(|directory| file.starts_with(directory))
        })
        .map(|(file, text)| {
            let mut source = code_of(&text);
            source.name = relative(&file);
            source.path = file;
            source
        })
        .collect();
    assert!(
        found.len() > 100,
        "{} source files, so a test that reads them would pass by scanning almost nothing",
        found.len()
    );
    found
}
