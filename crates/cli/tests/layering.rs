//! LAYER-1's table against the manifests it describes, and LAYER-3's list against that table.
//!
//! [LAYER-1] states the scope of the whole spec: a change to a crate's dependencies or its reach
//! is a change to this document. Nothing made that true. A crate added to the workspace compiles
//! and ships with no row anywhere, a row's dependency list drifts from the manifest it describes
//! without anything noticing, and a crate that shows released content is a surface [LAYER-3] has
//! to name before its marking rule reaches it. Each of those is a table and a manifest
//! disagreeing, which is a thing a program can read.
//!
//! The property belongs to the workspace rather than to any one crate, and it lives here for the
//! reason the unsafe declarations do: this is the crate the shipped binary is built from.
//!
//! The guards of the `test-hosts` seam are in `test_hosts_seam.rs`, and the scanner of the source a
//! release compiles, which both files and `account_sessions.rs` read it with, is in
//! `shipped/mod.rs`.
//!
//! [LAYER-1]: ../../../docs/specs/layering.md
//! [LAYER-3]: ../../../docs/specs/layering.md

mod shipped;

use shipped::{clause, rust_files, workspace};
use std::collections::BTreeSet;

/// Every crate in this workspace is named `bravebot-<member directory>`, which is what lets a row
/// write `` `core` `` in a dependency list and a manifest write `bravebot-core`.
const PREFIX: &str = "bravebot-";

/// How the constraint on a crate that shows released content to a person opens.
///
/// The cell says more after it, about what that crate may display and what it owns. This first
/// sentence is what makes the row one of the surfaces the marking rule is addressed to.
const PRESENTATION: &str = "Presentation";

/// Constraint openings that begin with that word and describe something else.
///
/// The message catalogs hold what a person reads and put none of it on a screen themselves, so
/// the word in that row is about the text rather than about a surface. Any other opening that
/// starts with it is an error rather than a guess: the two readings differ by exactly the rule
/// this file checks.
const NOT_A_SURFACE: [&str; 1] = ["Presentation text only"];

/// What a constraint says when the crate must reach no terminal library at all.
///
/// Two rows say it, and each is the reason its crate was separated out: a session record is read
/// back by whatever is resuming it, and the build stamp is written by every front end, so neither
/// may cost a caller that draws nothing a dependency on the terminal.
const DRAWS_NOTHING: &str = "Not presentation: draws nothing and links no terminal library";

/// The crates that draw a terminal, which the rows above promise not to reach.
///
/// Named rather than derived: what makes these the terminal is what they are for, and a list read
/// off the tree would be whatever the tree currently happens to contain.
///
/// Each name matches the crate itself and the family published under it, because both ship in
/// pieces: `ratatui` is a facade over `ratatui-core`, `ratatui-widgets` and a backend crate per
/// terminal, and `crossterm` has `crossterm_winapi` beneath it. An exact-name list would be
/// satisfied by a manifest that asked for the part it wanted instead of the whole.
const TERMINAL_LIBRARIES: [&str; 2] = ["ratatui", "crossterm"];

/// Whether a crate is one of the terminal libraries or a piece of one.
fn draws_a_terminal(name: &str) -> bool {
    TERMINAL_LIBRARIES.iter().any(|library| {
        name.strip_prefix(library)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_']))
    })
}

/// Whether a constraint cell promises the crate reaches no terminal library.
///
/// A cell that makes the promise in its own words is an error rather than a `false`, for the reason
/// [`is_a_surface`] rejects an unrecognised opening: the words are what addresses the rule to the
/// row, so a reword that still reads as the same promise to a person would quietly take the row out
/// of the check while leaving the claim on the page.
fn draws_nothing(constraint: &str) -> bool {
    if constraint.starts_with(DRAWS_NOTHING) {
        return true;
    }
    assert!(
        !constraint.contains("links no terminal library"),
        "a constraint promises it links no terminal library without opening `{DRAWS_NOTHING}`, \
         which is the wording the check for that promise reads. Open with it or drop the promise: \
         a row that claims it in other words is a row nothing holds to it"
    );
    false
}

/// The column headings LAYER-1's table carries.
///
/// The header is told from a data row by matching these rather than by being the first row
/// parsed, so a table that lost its header fails rather than silently skipping its first crate.
const HEADINGS: [&str; 4] = ["Crate", "Purpose", "Depends on", "Constraint"];

/// The manifest tables a dependency may be declared in.
///
/// A platform-specific one is `[target.'cfg(unix)'.dependencies]`, so a name is looked for
/// anywhere in a table's path rather than at the start of it.
const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// One row of LAYER-1's table: a crate, the crates it may depend on, and its constraint.
///
/// The purpose column is not read. It is prose for a person and nothing mechanical decides it.
#[derive(Debug, PartialEq)]
struct Row {
    /// The crate's package name, as the manifest spells it.
    name: String,
    /// The package names the row grants it, expanded from the short forms the cell uses.
    depends_on: BTreeSet<String>,
    /// The constraint cell, verbatim.
    constraint: String,
}

/// The cells of a markdown table row, or nothing if the line is not one.
///
/// A separator is not a row of data, so it is rejected here rather than by the caller counting
/// lines.
fn cells(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    let inner = line.strip_prefix('|')?.strip_suffix('|')?;
    let cells: Vec<String> = inner
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect();
    if cells.iter().all(|cell| cell.chars().all(|c| c == '-')) {
        return None;
    }
    Some(cells)
}

/// The package names a `Depends on` cell grants, expanded from the short forms it uses.
///
/// `none` is the empty set and is the one value spelled without backticks. Anything else that is
/// not a backticked name fails here, a blank cell included: a blank read as the empty set would
/// pass for the strongest constraint in the table.
fn depends_on(cell: &str) -> BTreeSet<String> {
    if cell == "none" {
        return BTreeSet::new();
    }
    cell.split(',')
        .map(|name| {
            let name = name.trim();
            let short = name
                .strip_prefix('`')
                .and_then(|rest| rest.strip_suffix('`'));
            let short = short.unwrap_or_else(|| {
                panic!(
                    "`{name}` in a dependency list is neither a backticked crate name nor the \
                     word `none`"
                )
            });
            format!("{PREFIX}{short}")
        })
        .collect()
}

/// Whether a constraint cell describes a surface that shows released content to a person.
fn is_a_surface(constraint: &str) -> bool {
    let sentence = constraint.split('.').next().unwrap_or(constraint).trim();
    if sentence == PRESENTATION {
        return true;
    }
    if !sentence.starts_with(PRESENTATION) {
        return false;
    }
    assert!(
        NOT_A_SURFACE.contains(&sentence),
        "a constraint opens `{sentence}`, which is neither `{PRESENTATION}.` nor one of the \
         openings that use the word about something else ({NOT_A_SURFACE:?}). Whether the row is \
         a surface decides whether the clause about marking has to name it, so say which it is \
         rather than leaving it to be read"
    );
    false
}

/// LAYER-1's table, as rows.
fn layering_table(spec: &str) -> Vec<Row> {
    let body = clause(spec, "LAYER-1");
    let mut rows = Vec::new();
    let mut headings = 0;
    for line in body.lines() {
        let Some(cells) = cells(line) else { continue };
        if cells == HEADINGS {
            headings += 1;
            continue;
        }
        assert_eq!(
            cells.len(),
            HEADINGS.len(),
            "a row of LAYER-1's table has {} cells rather than {}: {line}",
            cells.len(),
            HEADINGS.len()
        );
        let name = cells[0]
            .strip_prefix('`')
            .and_then(|rest| rest.strip_suffix('`'))
            .unwrap_or_else(|| panic!("the crate column of `{line}` is not `` `name` ``"))
            .to_string();
        rows.push(Row {
            name,
            depends_on: depends_on(&cells[2]),
            constraint: cells[3].clone(),
        });
    }
    assert_eq!(
        headings, 1,
        "LAYER-1's table has {headings} header rows reading {HEADINGS:?}, and one is what says \
         which column is which"
    );
    rows
}

/// The crates a clause names, as the package names its prose spells in backticks.
///
/// A `verified-by` line is backticked too, and names a test: the whole line is one piece, opening
/// with `verified-by` rather than with a package name, so it contributes nothing here.
fn named_crates(body: &str) -> BTreeSet<String> {
    let mut named = BTreeSet::new();
    for piece in body.split('`').skip(1).step_by(2) {
        if piece.starts_with(PREFIX) {
            named.insert(piece.to_string());
        }
    }
    named
}

/// The workspace members, read from the root manifest for the reason the unsafe check reads them
/// there: a directory that is not a member is not compiled, and a member outside `crates/` would
/// be missed by a walk while still shipping in the binary.
fn members(manifest: &str) -> Vec<String> {
    let mut workspace_table = false;
    let mut assignment = String::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            workspace_table = line == "[workspace]";
            continue;
        }
        if !workspace_table || (assignment.is_empty() && !line.starts_with("members")) {
            continue;
        }
        assignment.push_str(line);
        if line.contains(']') {
            break;
        }
    }
    assignment
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// The first quoted string in a fragment of TOML.
fn quoted(value: &str) -> Option<&str> {
    // Either quote, since TOML spells a string both ways and which one a manifest used is a
    // reformatting. Read as double-quoted only, `package = 'ratatui'` yields the version beside it
    // and the crate a rename points at goes unseen.
    let quote = value.chars().find(|c| *c == '"' || *c == '\'')?;
    let (_, rest) = value.split_once(quote)?;
    let (inner, _) = rest.split_once(quote)?;
    Some(inner)
}

/// The crate an inline dependency table renames itself from, if it renames itself.
fn renamed_from(value: &str) -> Option<&str> {
    let (_, rest) = value.split_once("package")?;
    let (_, rest) = rest.split_once('=')?;
    quoted(rest)
}

/// The crates in this workspace a member depends on.
fn workspace_dependencies(manifest: &str) -> BTreeSet<String> {
    dependencies(manifest)
        .into_iter()
        .filter(|name| name.starts_with(PREFIX))
        .collect()
}

/// Every crate a member depends on, whatever section of its manifest asks for it and however that
/// section spells the request.
///
/// A dependency is an edge whether it was declared for the build, the tests, the build script or
/// one platform: what LAYER-1 grants is the reach of the crate, and a test that links the
/// terminal interface reaches it. The spellings matter for the same reason. Cargo takes a
/// dependency as one line in a table, as a table of its own, and under a name of the caller's
/// choosing with the real crate in a `package` key, so a reader that saw only the first of those
/// would be satisfied by a manifest that had merely been reformatted.
fn dependencies(manifest: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    // The dependency the current `[dependencies.<name>]` table is about. A `package` key inside
    // such a table renames it and may come anywhere in it, so the name is held until the table
    // ends rather than noted at its heading.
    let mut entry: Option<String> = None;
    let mut in_table = false;

    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if let Some(name) = entry.take() {
                found.insert(name);
            }
            let path = line.trim_matches(|c| c == '[' || c == ']');
            let parts: Vec<&str> = path
                .split('.')
                .map(|part| part.trim().trim_matches(|c| c == '"' || c == '\''))
                .collect();
            in_table = false;
            // `[workspace.dependencies]` is the catalogue of versions the members choose from,
            // not an edge belonging to whichever crate the file describes.
            if parts.first() == Some(&"workspace") {
                continue;
            }
            let at = parts
                .iter()
                .position(|part| DEPENDENCY_TABLES.contains(part));
            match at {
                Some(index) if index + 1 < parts.len() => entry = Some(parts[index + 1].into()),
                Some(_) => in_table = true,
                None => {}
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if let Some(name) = entry.as_mut() {
            if key == "package"
                && let Some(real) = quoted(value)
            {
                *name = real.to_string();
            }
            continue;
        }
        if !in_table {
            continue;
        }
        // `bravebot-tui.workspace = true` and `mcp.package = "bravebot-mcp"` are a table's two
        // keys written without the table.
        let mut parts = key.split('.').map(|part| part.trim().trim_matches('"'));
        let declared = parts.next().unwrap_or(key);
        let name = if parts.any(|part| part == "package") {
            quoted(value).unwrap_or(declared)
        } else {
            renamed_from(value).unwrap_or(declared)
        };
        found.insert(name.to_string());
    }
    if let Some(name) = entry.take() {
        found.insert(name);
    }
    found
}

/// LAYER-1's rows and the members and manifests they describe.
fn table_and_manifests() -> (Vec<Row>, Vec<(String, String)>) {
    let workspace = workspace();
    let spec = std::fs::read_to_string(workspace.join("docs/specs/layering.md"))
        .expect("the layering spec");
    let rows = layering_table(&spec);
    assert!(
        !rows.is_empty(),
        "no rows were read out of LAYER-1's table, so this test would pass by checking nothing. \
         The table under the LAYER-1 anchor is what it reads; follow it if it has moved"
    );

    let root = std::fs::read_to_string(workspace.join("Cargo.toml")).expect("the root manifest");
    let manifests = members(&root)
        .into_iter()
        .map(|member| {
            let manifest = std::fs::read_to_string(workspace.join(&member).join("Cargo.toml"))
                .expect("a workspace member's manifest");
            (member, manifest)
        })
        .collect();
    (rows, manifests)
}

/// A crate nothing states a constraint on is a crate that may do anything, and the table is where
/// every constraint in this workspace is written. A member added without a row compiles, tests and
/// ships exactly as well as one with a row, so nothing about adding the next crate surfaces the
/// omission. This is what surfaces it, in both directions: a row for a crate that is not a member
/// is a constraint on nothing, which reads like coverage and is not.
#[test]
fn every_workspace_member_is_a_row_in_the_layering_table() {
    let (rows, manifests) = table_and_manifests();
    let described: BTreeSet<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    let compiled: BTreeSet<String> = manifests
        .iter()
        .map(|(member, _)| {
            let directory = member.rsplit('/').next().expect("a member path");
            format!("{PREFIX}{directory}")
        })
        .collect();

    for name in &compiled {
        assert!(
            described.contains(name.as_str()),
            "{name} is a workspace member with no row in LAYER-1's table, so nothing anywhere \
             states what it may do. Give it a row: a purpose, the crates it depends on, and its \
             constraint"
        );
    }
    for name in &described {
        assert!(
            compiled.contains(*name),
            "LAYER-1's table has a row for {name}, which is not a member of this workspace. A \
             constraint on a crate nothing compiles reads as coverage and is none"
        );
    }
}

/// The dependency column is the reach of a crate, and reach is what the spec is about: the kernel
/// depending on nothing is what makes "owns every decision derived from content" checkable, and
/// the auth-only crates depending on nothing is what makes "carries no workspace content" more
/// than a promise. A manifest gaining an edge the table does not grant is that argument quietly
/// ceasing to hold, and an edge is one line in a file no reviewer of the spec is looking at.
#[test]
fn a_rows_dependency_list_is_what_the_manifest_asks_for() {
    let (rows, manifests) = table_and_manifests();
    for (member, manifest) in &manifests {
        let directory = member.rsplit('/').next().expect("a member path");
        let name = format!("{PREFIX}{directory}");
        let Some(row) = rows.iter().find(|row| row.name == name) else {
            // The other test names this, and failing twice over one omission says no more.
            continue;
        };
        let asked_for = workspace_dependencies(manifest);
        assert_eq!(
            row.depends_on, asked_for,
            "LAYER-1 grants {name} {:?} and its manifest asks for {asked_for:?}. The table is the \
             statement of what this crate may reach, so whichever of the two is wrong, they cannot \
             both stand",
            row.depends_on
        );
    }
}

/// What a member declares, split the way cargo compiles it: the edges its own targets link, the
/// ones only its tests, examples and benchmarks link, and the ones only its build script links.
///
/// Each part is the manifest's lines under the tables of that kind, so [`dependencies`] reads it
/// with every spelling it already understands. A platform's table counts as the kind it names.
fn dependencies_by_kind(manifest: &str) -> [BTreeSet<String>; 3] {
    manifest_parts(manifest).map(|part| workspace_dependencies(&part))
}

/// The lines of a manifest that belong to each kind of dependency table, in the order
/// [`dependencies_by_kind`] returns them. Everything outside a dependency table is in the first.
fn manifest_parts(manifest: &str) -> [String; 3] {
    let mut parts = [String::new(), String::new(), String::new()];
    let mut kind = 0;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            let path = trimmed.trim_matches(|c| c == '[' || c == ']');
            let names: Vec<&str> = path
                .split('.')
                .map(|part| part.trim().trim_matches(|c| c == '"' || c == '\''))
                .collect();
            kind = if names.contains(&"dev-dependencies") {
                1
            } else if names.contains(&"build-dependencies") {
                2
            } else {
                0
            };
        }
        parts[kind].push_str(line);
        parts[kind].push('\n');
    }
    parts
}

/// Source text with its comments taken out: what follows `//` to the end of its line, and each
/// `/* ... */` block, nested ones included, over as many lines as it spans. A block leaves its line
/// breaks and one space, so the code either side of it stays on its own lines and separate words.
/// String, raw string and character literals are kept whole and read for no comment, so a `/*` in
/// a glob such as `"src/*.rs"` does not open a block that would hide the code after it.
///
/// `shipped::code_of` is not reused here because it also removes the items marked `#[cfg(test)]`,
/// which would hide a dev-dependency's use in a test module under `src`.
fn without_comments(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let at = |i: usize| chars.get(i).copied();
    let mut code = String::with_capacity(source.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let word_before = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        if c == '/' && at(i + 1) == Some('/') {
            while at(i).is_some_and(|n| n != '\n') {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == Some('*') {
            let mut depth = 0usize;
            while i < chars.len() {
                if chars[i] == '/' && at(i + 1) == Some('*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && at(i + 1) == Some('/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if chars[i] == '\n' {
                        code.push('\n');
                    }
                    i += 1;
                }
            }
            code.push(' ');
        } else if c == 'r' && !word_before && matches!(at(i + 1), Some('"' | '#')) {
            let mut hashes = 0;
            while at(i + 1 + hashes) == Some('#') {
                hashes += 1;
            }
            if at(i + 1 + hashes) != Some('"') {
                code.push(c);
                i += 1;
                continue;
            }
            let start = i;
            i += 2 + hashes;
            while i < chars.len()
                && !(chars[i] == '"' && (1..=hashes).all(|h| at(i + h) == Some('#')))
            {
                i += 1;
            }
            i = (i + 1 + hashes).min(chars.len());
            code.extend(&chars[start..i]);
        } else if c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            i = (i + 1).min(chars.len());
            code.extend(&chars[start..i]);
        } else if c == '\'' && at(i + 1) == Some('\\') {
            let start = i;
            i += 2;
            while i < chars.len() && chars[i] != '\'' {
                i += 1;
            }
            i = (i + 1).min(chars.len());
            code.extend(&chars[start..i]);
        } else if c == '\'' && at(i + 2) == Some('\'') {
            code.extend(&chars[i..i + 3]);
            i += 3;
        } else {
            code.push(c);
            i += 1;
        }
    }
    code
}

/// Whether `code` names the crate `package` as a Rust path does, `bravebot_core` for
/// `bravebot-core`, as a whole word, so `bravebot_ui_files` is not a use of a crate called
/// `bravebot-ui`.
fn names_crate(code: &str, package: &str) -> bool {
    let ident = package.replace('-', "_");
    let word = |c: char| c.is_alphanumeric() || c == '_';
    code.match_indices(&ident).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let after = code[at + ident.len()..].chars().next();
        !before.is_some_and(word) && !after.is_some_and(word)
    })
}

/// The workspace edges of a member that none of the code able to use them names, each with the
/// kind of table it is declared in.
///
/// `library` is the source its `[dependencies]` reach, `tests` the source its `[dev-dependencies]`
/// reach in addition, and `build` its build script, each read without its comments. A dependency
/// renamed with `package` is named in code by its new name, so it is reported here: none is renamed
/// today, and a failure is the safe side of that.
fn unnamed_dependencies(manifest: &str, library: &str, tests: &str, build: &str) -> Vec<String> {
    let [normal, dev, build_only] = dependencies_by_kind(manifest);
    let (library, tests, build) = (
        &without_comments(library),
        &without_comments(tests),
        &without_comments(build),
    );
    let mut unnamed = Vec::new();
    for dependency in &normal {
        if !names_crate(library, dependency) {
            unnamed.push(format!("{dependency} in [dependencies]"));
        }
    }
    for dependency in &dev {
        if !names_crate(library, dependency) && !names_crate(tests, dependency) {
            unnamed.push(format!("{dependency} in [dev-dependencies]"));
        }
    }
    for dependency in &build_only {
        if !names_crate(build, dependency) {
            unnamed.push(format!("{dependency} in [build-dependencies]"));
        }
    }
    unnamed
}

/// The Rust files under the named directories of a crate, joined.
fn code_under(krate: &std::path::Path, directories: &[&str]) -> String {
    let mut code = String::new();
    for directory in directories {
        let path = krate.join(directory);
        let files = if path.is_dir() {
            rust_files(&path)
        } else if path.is_file() {
            vec![path]
        } else {
            continue;
        };
        for file in files {
            code.push_str(&std::fs::read_to_string(&file).expect("a source file"));
            code.push('\n');
        }
    }
    code
}

/// LAYER-1's dependency column is held to the manifests by the test above, so an edge the code
/// stopped using stays granted in the table for as long as it stays in the manifest, and nothing
/// about a build says so: the crate links it and compiles. That edge is reach the spec grants and
/// no code needs, which a later change can start using without the table changing. This reads each
/// member's code, comments left out, for the crate name of every workspace dependency it declares.
#[test]
fn every_workspace_dependency_a_crate_declares_is_named_by_its_code() {
    let (_, manifests) = table_and_manifests();
    let workspace = workspace();
    let mut checked = 0;
    let mut found = Vec::new();
    for (member, manifest) in &manifests {
        let krate = workspace.join(member);
        let library = code_under(&krate, &["src"]);
        let tests = code_under(&krate, &["tests", "test-support", "examples", "benches"]);
        let build = code_under(&krate, &["build.rs"]);
        let directory = member.rsplit('/').next().expect("a member path");
        for unnamed in unnamed_dependencies(manifest, &library, &tests, &build) {
            found.push(format!("{PREFIX}{directory} declares {unnamed}"));
        }
        checked += workspace_dependencies(manifest).len();
    }
    assert!(
        found.is_empty(),
        "{found:?}, and no code that could use them names the crate. Remove each edge from the \
         manifest and from the crate's row in LAYER-1, or use it"
    );
    assert!(
        checked > 20,
        "{checked} workspace edges were read out of the manifests, so this test would pass by \
         checking almost nothing"
    );
}

/// The scan above has to tell a use from a mention: a dependency named only in a comment, a line
/// comment or a block comment over several lines, or as part of a longer crate name is unused, a
/// dev-dependency named in a test file is used, and a dependency of the library named only in a test
/// file is not a use of the library's edge.
#[test]
fn the_dependency_scan_skips_comments_and_reads_each_table_against_its_own_code() {
    let manifest = "\
[package]
name = \"bravebot-x\"

[dependencies]
bravebot-core = { path = \"../core\" }
bravebot-ui = { path = \"../ui\" }

[target.'cfg(unix)'.dependencies]
bravebot-sandbox = { path = \"../sandbox\" }

[dev-dependencies]
bravebot-session = { path = \"../session\" }

[build-dependencies]
bravebot-stamp = { path = \"../stamp\" }
";
    let library = "use bravebot_core::Label;\n// bravebot_ui draws nothing here\nuse bravebot_ui_files::open; // bravebot_ui\n/* bravebot_ui\n   is not used */ fn f() { bravebot_sandbox::spawn(); }\n/** bravebot_ui */";
    let library_after_a_glob = "const GLOB: &str = \"src/*.rs\";\nconst SLASH: char = '/';\nfn g() { bravebot_core::f(); bravebot_sandbox::g(); bravebot_ui::h(); }";
    let tests = "use bravebot_session::Record;";
    let build = "fn main() { bravebot_stamp::write(); }";
    assert_eq!(
        unnamed_dependencies(manifest, library, tests, build),
        vec!["bravebot-ui in [dependencies]".to_string()]
    );
    assert_eq!(
        unnamed_dependencies(manifest, "", "use bravebot_core::Label;", ""),
        vec![
            "bravebot-core in [dependencies]".to_string(),
            "bravebot-sandbox in [dependencies]".to_string(),
            "bravebot-ui in [dependencies]".to_string(),
            "bravebot-session in [dev-dependencies]".to_string(),
            "bravebot-stamp in [build-dependencies]".to_string(),
        ]
    );
    assert_eq!(
        unnamed_dependencies(manifest, library_after_a_glob, tests, build),
        Vec::<String>::new()
    );
}

/// A surface that shows released content to a person is the one place quarantined bytes are put in
/// front of somebody on purpose, and the rule that makes it safe is that the surface marks them:
/// the margin is drawn by the renderer and the control characters are replaced, so the content
/// cannot draw its own. That rule reaches a surface by naming it. A crate whose constraint says it
/// is presentation and which no clause names is a surface with released content on it and no rule
/// about how it is marked, and the thing that would have caught the omission is this comparison,
/// because the row and the clause are eighty lines apart in one file.
#[test]
fn every_presentation_crate_is_named_by_the_clause_that_marks_content() {
    let (rows, _) = table_and_manifests();
    let spec = std::fs::read_to_string(workspace().join("docs/specs/layering.md"))
        .expect("the layering spec");
    let marked = named_crates(&clause(&spec, "LAYER-3"));

    let surfaces: BTreeSet<&str> = rows
        .iter()
        .filter(|row| is_a_surface(&row.constraint))
        .map(|row| row.name.as_str())
        .collect();
    assert!(
        !surfaces.is_empty(),
        "no row of LAYER-1's table describes a presentation crate, so this test would pass by \
         checking nothing. A constraint opening `{PRESENTATION}.` is what it reads"
    );

    for name in &surfaces {
        assert!(
            marked.contains(*name),
            "LAYER-1 calls {name} presentation and the clause about marking released content does \
             not name it, so nothing says how what it displays is marked. Name it there, and give \
             the clause the test that pins the marking on that surface"
        );
    }
    for name in &marked {
        assert!(
            surfaces.contains(name.as_str()),
            "the clause about marking released content names {name}, whose row in LAYER-1's table \
             does not open with `{PRESENTATION}.`. One of the two has the wrong idea of what that \
             crate is for"
        );
    }
}

/// "Links no terminal library" is what a caller that draws nothing gets out of the row saying it,
/// and it is the one claim in the table a manifest can contradict without reading differently: an
/// edge added to such a crate is a line in a file, and the caller that pays for it is a front end
/// in another repository whose build is the first thing to say so. Transitively, because the cost
/// is what gets linked rather than what gets written down: a row that reached the terminal through
/// one more crate would have kept its wording and lost its meaning.
#[test]
fn a_crate_that_draws_nothing_reaches_no_terminal_library() {
    let (rows, manifests) = table_and_manifests();
    let asked_for: std::collections::BTreeMap<String, BTreeSet<String>> = manifests
        .iter()
        .map(|(member, manifest)| {
            let directory = member.rsplit('/').next().expect("a member path");
            (format!("{PREFIX}{directory}"), dependencies(manifest))
        })
        .collect();

    let drawing_nothing: BTreeSet<&str> = rows
        .iter()
        .filter(|row| draws_nothing(&row.constraint))
        .map(|row| row.name.as_str())
        .collect();
    assert!(
        !drawing_nothing.is_empty(),
        "no row of LAYER-1's table opens `{DRAWS_NOTHING}`, so this test would pass by checking \
         nothing. That opening is what it reads"
    );

    for name in &drawing_nothing {
        // Only this workspace's edges are followed: a crate outside it contributes none of ours to
        // follow. Order does not matter, since every reachable edge is visited either way.
        let mut seen = BTreeSet::new();
        let mut queue = vec![(*name).to_string()];
        while let Some(crate_name) = queue.pop() {
            let Some(edges) = asked_for.get(&crate_name) else {
                continue;
            };
            for edge in edges {
                assert!(
                    !draws_a_terminal(edge),
                    "LAYER-1 says {name} links no terminal library, and {crate_name} asks for \
                     {edge}. Whichever of the two is wrong, a caller that draws nothing is linking \
                     the terminal either way"
                );
                if seen.insert(edge.clone()) {
                    queue.push(edge.clone());
                }
            }
        }
    }
}

/// The standard-library modules that reach outside the process, and the macros that write to it.
const REACHES_OUT: [&str; 4] = ["fs", "io", "process", "env"];
const PRINTS: [&str; 5] = ["println!", "print!", "eprintln!", "eprint!", "dbg!"];

/// The `std::` paths and print macros a source file's non-test code uses that a crate promising
/// "no I/O" and "nothing prints" may not, one description per use.
///
/// Test modules are skipped, since a test reads fixtures. `use std::{fs, io}` and
/// `use std::{\n    fs,\n}` are read as the paths they are, so grouping the import is not a way past
/// it. Comments are skipped, because the crates say in prose what they do not do.
fn reaching_out(source: &str) -> Vec<String> {
    let code = shipped::code_of(source).without_test_modules;
    let mut found = Vec::new();
    for print in PRINTS {
        if code.contains(print) {
            found.push(print.to_string());
        }
    }
    for (at, _) in code.match_indices("std::") {
        let path = &code[at + "std::".len()..];
        let segments = match path.strip_prefix('{') {
            Some(group) => group.split('}').next().unwrap_or_default().to_string(),
            None => path
                .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
                .next()
                .unwrap_or_default()
                .to_string(),
        };
        for word in segments.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            if REACHES_OUT.contains(&word) || word == "TcpStream" || word == "UdpSocket" {
                found.push(format!("std::{segments}"));
                break;
            }
        }
    }
    found
}

/// LAYER-1 says the kernel does no I/O and prints nothing, and the file-type crate and the crate
/// that reads a processor's answer for a number do no I/O. A manifest cannot say it: they depend
/// on nothing outside the standard library and the kernel, which is where the file system, the
/// environment, child processes and the terminal live. So what holds the promise is the code never
/// naming them, read off the source with the tests left out.
#[test]
fn the_kernel_and_the_file_type_crate_do_no_io_and_print_nothing() {
    for krate in ["core", "filetype", "choice"] {
        let files = rust_files(&workspace().join("crates").join(krate).join("src"));
        assert!(
            files.len() > 1,
            "{krate} has {} source files, so this test would pass by scanning almost nothing",
            files.len()
        );
        for file in files {
            let source = std::fs::read_to_string(&file).expect("a source file");
            let found = reaching_out(&source);
            assert!(
                found.is_empty(),
                "LAYER-1 says {krate} does no I/O and prints nothing, and {} uses {found:?}",
                file.display()
            );
        }
    }
}

/// The crates LAYER-1's rows say do no I/O, each with the dependencies it may ask for, which are the
/// ones that do none: the kernel and the file-type crate ask for nothing, and the crate that reads a
/// processor's answer asks for the kernel and for the JSON parser, which reads the text it is handed
/// and opens nothing. A dependency is added here by deciding that the crate it names does no I/O,
/// which is the question the manifests test cannot ask: it holds a row's list to the manifest and
/// grants whatever the row grants.
const IO_FREE: [(&str, &[&str]); 3] = [
    ("core", &[]),
    ("filetype", &[]),
    ("choice", &["bravebot-core", "serde_json"]),
];

/// The crates a manifest asks for in the tables its own targets link, `[dependencies]` and the
/// platform ones, that `allowed` does not name. Every crate is read, not only this workspace's,
/// since a crate that does I/O may come from outside it. The tables of tests and of the build
/// script are not read: a test reads fixtures, and the promise is about what the crate does once
/// it is built.
fn dependencies_outside(manifest: &str, allowed: &[&str]) -> Vec<String> {
    let [normal, _, _] = manifest_parts(manifest);
    dependencies(&normal)
        .into_iter()
        .filter(|name| !allowed.contains(&name.as_str()))
        .collect()
}

/// A crate that does no I/O may not depend on one that does. The scan above holds the source of
/// these crates to no `std::fs`, and a dependency is how the same reach comes in without the source
/// naming it: `bravebot-sandbox` or `rustix` called from the kernel is a file read that no line of
/// the kernel spells.
#[test]
fn the_crates_that_do_no_io_depend_only_on_crates_that_do_none() {
    let (rows, manifests) = table_and_manifests();
    let promised: BTreeSet<String> = rows
        .iter()
        .filter(|row| row.constraint.to_lowercase().contains("no i/o"))
        .map(|row| row.name.clone())
        .collect();
    let listed: BTreeSet<String> = IO_FREE
        .iter()
        .map(|(directory, _)| format!("{PREFIX}{directory}"))
        .collect();
    assert_eq!(
        promised, listed,
        "the rows of LAYER-1 that say `no I/O` are not the crates this test holds to it. A row \
         that promises it needs an entry in IO_FREE naming what it may depend on, and an entry \
         for a crate whose row no longer promises it is a constraint nothing states"
    );
    for (directory, allowed) in IO_FREE {
        let (_, manifest) = manifests
            .iter()
            .find(|(member, _)| member.rsplit('/').next() == Some(directory))
            .unwrap_or_else(|| panic!("{directory} is not a workspace member"));
        let outside = dependencies_outside(manifest, allowed);
        assert!(
            outside.is_empty(),
            "LAYER-1 says {PREFIX}{directory} does no I/O, and its manifest asks for {outside:?}, \
             which IO_FREE does not name as a crate that does none ({allowed:?} are). A crate that \
             does I/O is reached through a dependency without any line of this one saying so"
        );
    }
}

/// The dependency check names the crate behind each spelling a manifest may ask for it in, and
/// reads neither the tables of tests nor a crate the row grants.
#[test]
fn a_dependency_on_a_crate_that_does_io_is_refused() {
    let granted = ["bravebot-core", "serde_json"];
    let base = "[package]\nname = \"bravebot-choice\"\n\n";
    let plain = format!(
        "{base}[dependencies]\nbravebot-core = {{ path = \"../core\" }}\n\
         serde_json = {{ workspace = true }}\n\n[dev-dependencies]\nrustix = {{ workspace = true }}\n"
    );
    assert!(
        dependencies_outside(&plain, &granted).is_empty(),
        "the granted crates, and a test-only one, were refused"
    );
    for (asked, refused) in [
        ("[dependencies]\nrustix = { workspace = true }\n", "rustix"),
        (
            "[dependencies]\nbravebot-sandbox = { path = \"../sandbox\" }\n",
            "bravebot-sandbox",
        ),
        ("[dependencies.libc]\nversion = \"0.2\"\n", "libc"),
        (
            "[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n",
            "libc",
        ),
        (
            "[dependencies]\nhost = { package = \"rustix\", workspace = true }\n",
            "rustix",
        ),
        (
            "[dependencies]\nbravebot-sandbox.workspace = true\n",
            "bravebot-sandbox",
        ),
    ] {
        let manifest = format!("{base}{asked}");
        assert_eq!(
            dependencies_outside(&manifest, &granted),
            [refused.to_string()],
            "a dependency that does I/O was not refused: {asked}"
        );
    }
}

/// What a path in a crate that does no I/O may start from, beside the crate's own declarations and
/// the crates its manifest grants: the standard library, and `vstd`, which only the Verus build
/// compiles (`#[cfg(verus_keep_ghost)]` or inside `verus!`) and which holds no I/O.
const ROOTS: [&str; 6] = ["std", "core", "alloc", "crate", "self", "super"];
const VERIFIER_ROOTS: [&str; 1] = ["vstd"];

/// `pub(crate) use a::b;` without the visibility, and the line unchanged where there is none.
fn without_visibility(line: &str) -> &str {
    let Some(rest) = line.strip_prefix("pub") else {
        return line;
    };
    if let Some(inner) = rest.strip_prefix('(') {
        return inner
            .split_once(')')
            .map_or(line, |(_, after)| after.trim_start());
    }
    if rest.starts_with(char::is_whitespace) {
        rest.trim_start()
    } else {
        line
    }
}

/// The parts of a use tree's braces that a comma separates, leaving the commas of nested braces.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (at, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// Whether what follows a root in a use tree binds the root itself under a name of the importer's
/// choosing or brings everything under it into scope, rather than naming a path below it:
/// `std as host`, `std::*` and `std::{self as host}` are those, `std::fmt` and `std::{fmt, io}`
/// are not.
fn binds_the_root(after_root: &str) -> bool {
    let Some(path) = after_root.trim_start().strip_prefix("::") else {
        return true;
    };
    let path = path.trim_start();
    match path.strip_prefix('{') {
        Some(group) => {
            let inner = group.rsplit_once('}').map_or(group, |(inner, _)| inner);
            split_top_level(inner).iter().any(|item| {
                let item = item.trim();
                item.starts_with('*')
                    || item
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .next()
                        == Some("self")
            })
        }
        None => path.starts_with('*'),
    }
}

/// The root of each path one use tree imports, with whether the tree binds that root itself.
fn roots_of(tree: &str, found: &mut Vec<(String, bool)>) {
    let tree = tree.trim().trim_start_matches("::").trim_start();
    if let Some(group) = tree.strip_prefix('{') {
        let inner = group.rsplit_once('}').map_or(group, |(inner, _)| inner);
        for part in split_top_level(inner) {
            roots_of(part, found);
        }
        return;
    }
    let root: String = tree
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if root.is_empty() {
        return;
    }
    let binds = binds_the_root(&tree[root.len()..]);
    found.push((root, binds));
}

/// The root of every path a source file's non-test code imports with `use` or `extern crate`, one
/// per path, with whether the import binds that root itself. Test modules and comments are
/// skipped, as in [`reaching_out`]. An import is read where a line starts with it, which is where
/// `rustfmt` puts it.
fn use_roots(source: &str) -> Vec<(String, bool)> {
    let code = without_comments(&shipped::code_of(source).without_test_modules);
    let lines: Vec<&str> = code.lines().collect();
    let mut found = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = without_visibility(lines[at].trim());
        at += 1;
        if let Some(rest) = line.strip_prefix("extern crate ") {
            let name = rest
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()
                .unwrap_or_default();
            found.push((name.to_string(), true));
            continue;
        }
        let Some(first) = line.strip_prefix("use ") else {
            continue;
        };
        let mut tree = first.to_string();
        while !tree.contains(';') && at < lines.len() {
            tree.push(' ');
            tree.push_str(lines[at].trim());
            at += 1;
        }
        roots_of(tree.split(';').next().unwrap_or_default(), &mut found);
    }
    found
}

/// Whole words that let a crate keep a value between calls: the macro that declares a per-thread
/// static, and the standard types that hold a value every call can reach.
///
/// The item keyword `static` is not in this list because [`keeping_state`] tells it from the
/// lifetime of the same name.
const KEEPS_STATE: [&str; 5] = ["thread_local", "Mutex", "RwLock", "OnceLock", "LazyLock"];

/// Parts of a camel-case name that let a crate keep a value between calls. A part is matched and a
/// substring is not, so `RefCell`, `OnceCell`, `UnsafeCell`, `AtomicU8` and `AtomicBool` are found
/// and `Cellar` and `Atomically` are not.
const KEEPS_STATE_PARTS: [&str; 2] = ["Cell", "Atomic"];

/// The parts of a name written in camel case or with underscores: `RefCell` is `Ref` and `Cell`,
/// and `AtomicU8` is `Atomic` and `U8`.
fn name_parts(word: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    for piece in word.split('_') {
        let mut start = 0;
        let mut before: Option<char> = None;
        for (at, c) in piece.char_indices() {
            if c.is_uppercase() && before.is_some_and(|b| b.is_lowercase() || b.is_ascii_digit()) {
                parts.push(&piece[start..at]);
                start = at;
            }
            before = Some(c);
        }
        parts.push(&piece[start..]);
    }
    parts.retain(|part| !part.is_empty());
    parts
}

/// The words of a source file's non-test code that let it keep a value between calls, each once and
/// as written.
///
/// What is refused is the item keyword `static` (the body of a `thread_local!` or a `lazy_static!`
/// declares one as well), `thread_local` with or without the `!`, `Mutex`, `RwLock`, `OnceLock`,
/// `LazyLock`, and any name with a `Cell` or an `Atomic` part. The lifetime `'static`, as in
/// `&'static str`, is allowed: it is the same word after a quote, a table of constants may use it,
/// and a reference with that lifetime holds nothing a call can change.
///
/// The scan reads text, and a word is a run of letters, digits and underscores. A refused word in a
/// string literal, in a comment that follows code on its line or inside a `/* */` comment is
/// reported, and rewording it is the fix. Test modules and whole lines that start with `//` are
/// skipped, as [`reaching_out`] skips them.
fn keeping_state(source: &str) -> Vec<String> {
    let code = shipped::code_of(source).without_test_modules;
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut found: Vec<String> = Vec::new();
    let mut rest = code.as_str();
    while let Some(start) = rest.find(is_word) {
        let from_word = &rest[start..];
        let end = from_word
            .find(|c: char| !is_word(c))
            .unwrap_or(from_word.len());
        let word = &from_word[..end];
        // A word is maximal, so the character before every word but the first is not part of one,
        // and the quote of a lifetime is that character.
        let is_a_lifetime = rest[..start].ends_with('\'');
        let refused = (word == "static" && !is_a_lifetime)
            || KEEPS_STATE.contains(&word)
            || name_parts(word)
                .iter()
                .any(|part| KEEPS_STATE_PARTS.contains(part));
        if refused && !found.iter().any(|seen| seen == word) {
            found.push(word.to_string());
        }
        rest = &from_word[end..];
    }
    found
}

/// The names a source file's non-test code declares as a module, type or trait. A path may start
/// from one of these: `use Integrity::*;` names an enum of the crate, and `pub use ask::Answer;`
/// a module of it.
fn declared_in(source: &str) -> BTreeSet<String> {
    let code = without_comments(&shipped::code_of(source).without_test_modules);
    let words: Vec<&str> = code
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .collect();
    words
        .windows(2)
        .filter(|pair| ["mod", "enum", "struct", "trait", "type", "union"].contains(&pair[0]))
        .map(|pair| pair[1].to_string())
        .collect()
}

/// The imports of `source` that start from a name a crate that does no I/O may not use, one
/// description per import: a root that is not the standard library, the crate itself, a name the
/// crate declares or a crate in `granted`, and the standard library bound under a name of the
/// importer's choosing.
fn imports_outside(source: &str, declared: &BTreeSet<String>, granted: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    for (root, binds) in use_roots(source) {
        let standard = ["std", "core", "alloc"].contains(&root.as_str());
        if standard && binds {
            found.push(format!("{root} bound as a name of its own"));
        } else if !(ROOTS.contains(&root.as_str())
            || VERIFIER_ROOTS.contains(&root.as_str())
            || declared.contains(&root)
            || granted.iter().any(|name| name.replace('-', "_") == root))
        {
            found.push(root);
        }
    }
    found
}

/// A call through a handle that never names `std::fs` is not seen by the scan above, which reads
/// `std::` paths: `use std as host;` then `host::fs::read(..)` names no `std::fs`. What can give a
/// crate such a handle is the name an import starts from, so the roots of the imports of these
/// crates are held to the standard library, the crate and what its manifest grants.
#[test]
fn the_crates_that_do_no_io_import_from_nothing_that_does() {
    for (directory, granted) in IO_FREE {
        let files = rust_files(&workspace().join("crates").join(directory).join("src"));
        assert!(
            files.len() > 1,
            "{directory} has {} source files, so this test would pass by scanning almost nothing",
            files.len()
        );
        let sources: Vec<(std::path::PathBuf, String)> = files
            .into_iter()
            .map(|file| {
                let source = std::fs::read_to_string(&file).expect("a source file");
                (file, source)
            })
            .collect();
        let declared: BTreeSet<String> = sources
            .iter()
            .flat_map(|(_, source)| declared_in(source))
            .collect();
        let imports: usize = sources
            .iter()
            .map(|(_, source)| use_roots(source).len())
            .sum();
        assert!(
            imports > 0,
            "no import was read out of {directory}, so this test would pass by checking nothing"
        );
        for (file, source) in &sources {
            let found = imports_outside(source, &declared, granted);
            assert!(
                found.is_empty(),
                "LAYER-1 says {directory} does no I/O, and {} imports from {found:?}. A handle that \
                 never names std::fs reaches the file system without the scan of its paths seeing it",
                file.display()
            );
        }
    }
}

/// The import scan sees the handles the path scan cannot, and leaves alone the imports of a crate
/// that reaches nothing.
#[test]
fn the_import_scan_sees_a_handle_the_path_scan_cannot() {
    let declared = BTreeSet::from(["tables".to_string(), "Integrity".to_string()]);
    let granted = ["bravebot-core", "serde_json"];
    for hidden in [
        "use std as host;\nfn f() { let _ = host::fs::read(p); }",
        "use std::{self as host};\nfn f() { let _ = host::fs::read(p); }",
        "use std::*;\nfn f() { let _ = fs::read(p); }",
        "extern crate std as host;\nfn f() { let _ = host::fs::read(p); }",
        "use rustix::fs::open;\nfn f() { open(p); }",
        "use {rustix::fs::open, std::fmt};\nfn f() { open(p); }",
        "pub(crate) use libc::read as read_file;\n",
        "use std::fmt;\nuse bravebot_sandbox::{\n    fs as host,\n};\n",
    ] {
        assert!(
            reaching_out(hidden).is_empty(),
            "the path scan sees this, so it does not show a handle it cannot: {hidden}"
        );
        assert!(
            !imports_outside(hidden, &declared, &granted).is_empty(),
            "an import of a handle that reaches outside was not seen: {hidden}"
        );
    }
    // A handle another module of the crate re-exports is read where the re-export names the
    // standard library.
    let reexported = "mod host {\n    pub use std::fs::read as read_file;\n}\n\
                      use crate::host::read_file;\nfn f() { read_file(p); }";
    assert!(
        !reaching_out(reexported).is_empty(),
        "a re-export of std::fs was not seen at the line that names it"
    );
    assert!(imports_outside(reexported, &declared, &granted).is_empty());
    for quiet in [
        "use std::collections::BTreeMap;\nuse std::{fmt, net::IpAddr};\nuse crate::label::Label;\n\
         use super::tables::{A, B};\nuse self::inner::x;\nuse core::fmt::Debug;\n\
         use bravebot_core::apply::{Entries, Shape};\nuse serde_json::Value;\n\
         use std::{\n    collections::BTreeMap,\n    fmt,\n};",
        "use tables::X;\npub use Integrity::*;",
        "// use rustix::fs;\nfn f() {}\n/* use libc::read; */\n",
        "fn f() {}\n#[cfg(test)]\nmod tests {\n    use rustix::fs::open;\n}\nfn h() {}",
    ] {
        assert!(
            imports_outside(quiet, &declared, &granted).is_empty(),
            "an import that reaches nowhere was reported: {quiet}"
        );
    }
}

/// PROC-16 says one byte is all of an answer that leaves the policy layer, and LAYER-1 says the crate
/// that reads an answer for a number does no I/O. The reader that crate hands the policy layer is a
/// `fn` pointer, which captures nothing of the caller's, so the type does not stop that function
/// from copying the text into a `static` and returning it from another function later. What stops
/// it is the crate's code never naming a way to keep a value: no `static`, no `thread_local!`, no
/// interior mutability (`Cell`, `RefCell`, `OnceCell`), no lock or lazily initialised cell
/// (`Mutex`, `RwLock`, `OnceLock`, `LazyLock`) and no atomic (`AtomicU8`, `AtomicBool`), read off the
/// source with the tests left out. The lifetime `'static` is allowed, because `&'static str` holds
/// nothing between calls and a table of constants may need one.
#[test]
fn the_reader_of_a_number_keeps_nothing_between_calls() {
    let files = rust_files(&workspace().join("crates").join("choice").join("src"));
    assert!(
        files.len() > 1,
        "choice has {} source files, so this test would pass by scanning almost nothing",
        files.len()
    );
    for file in files {
        let source = std::fs::read_to_string(&file).expect("a source file");
        let found = keeping_state(&source);
        assert!(
            found.is_empty(),
            "PROC-16 says the reader of a number keeps nothing between calls, and {} names \
             {found:?}, which can keep a value from one call to the next",
            shipped::relative(&file)
        );
    }
}

/// The ways a source file's non-test code starts a process, one description per use.
///
/// `std::process::id()` reads the helper's own id and starts nothing, so it is the one use of the
/// module that is let through. A grouped import (`use std::{fs, process::Command}`) is read as the
/// paths it is, `Command::new(` is found however `Command` was imported, and `CommandExt` is the
/// way to replace the process instead of starting a second. Test modules and comments are skipped.
fn starts_a_process(source: &str) -> Vec<String> {
    let code = shipped::code_of(source).without_test_modules;
    let mut found = Vec::new();
    for token in ["Command::new(", "CommandExt"] {
        if code.contains(token) {
            found.push(token.to_string());
        }
    }
    for (at, _) in code.match_indices("std::") {
        let path = &code[at + "std::".len()..];
        let used = match path.strip_prefix('{') {
            Some(group) => group.split('}').next().unwrap_or_default().to_string(),
            None => path
                .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
                .next()
                .unwrap_or_default()
                .to_string(),
        };
        let used = used.replace("process::id", "");
        if used
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|word| word == "process")
        {
            found.push(format!("std::{used}"));
        }
    }
    found
}

/// LAYER-1 says `bravebot-ui-files` runs no command, and TRUST-22 rests on it: the helper's bytes
/// reach a screen and no turn. Its dependency list cannot say so, since `std` is where a child
/// process lives, so the promise is held by the shipped source never starting one.
#[test]
fn the_ui_files_helper_starts_no_process() {
    let files = rust_files(&workspace().join("crates").join("ui-files").join("src"));
    assert!(
        files.len() > 1,
        "ui-files has {} source files, so this test would pass by scanning almost nothing",
        files.len()
    );
    for file in files {
        let source = std::fs::read_to_string(&file).expect("a source file");
        let found = starts_a_process(&source);
        assert!(
            found.is_empty(),
            "LAYER-1 says bravebot-ui-files runs no command, and {} uses {found:?}",
            file.display()
        );
    }
}

/// The scan above has to see each way of starting a process, and let through the process id the
/// helper reads, the fixture a test module builds, and a comment that says what it does not do.
#[test]
fn the_process_scan_sees_a_grouped_import_and_a_command_and_skips_tests_and_comments() {
    for starting in [
        "use std::process::Command;",
        "use std::process;",
        "use std::{\n    fs,\n    process::Command,\n};",
        "let _ = Command::new(\"tar\");",
        "let _ = std::process::Command::new(\"tar\").spawn();",
        "use std::os::unix::process::CommandExt;",
        "std::process::exit(1);",
    ] {
        assert!(
            !starts_a_process(starting).is_empty(),
            "a process start was not seen: {starting}"
        );
    }
    for quiet in [
        "let id = std::process::id();",
        "use std::fs;\nlet id = std::process::id();",
        "// runs no command: no Command::new and no std::process here\nfn f() {}",
        "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn g() { let _ = std::process::Command::new(\"cmd\"); }\n}\nfn h() {}",
    ] {
        assert!(
            starts_a_process(quiet).is_empty(),
            "something that starts no process was reported: {quiet}"
        );
    }
}

/// Whether `text` names the ChatGPT flow's `sign_in`.
///
/// Read as text, so it holds what a file has to write to reach the function and nothing about what
/// it resolves to: the module path and the function in one file, the module's path ahead of the
/// call whatever the crate is called (`use bravebot_skus as skus;` leaves `skus::chatgpt::sign_in(`
/// with no `bravebot_skus::chatgpt` in it), a glob or a group imported from the module under that
/// alias, and the module renamed. What it does not see is `sign_in` reached through a re-export in
/// another crate, which no crate has, and a file that builds the call from a name it computes.
/// `needs_sign_in` and the other functions of the same spelling in other modules are not the flow, so
/// a file naming `sign_in` alone is not a caller.
fn names_the_chatgpt_sign_in(text: &str) -> bool {
    (text.contains("bravebot_skus::chatgpt") && text.contains("sign_in"))
        || text.contains("chatgpt::sign_in")
        || (text.contains("chatgpt::*") && text.contains("sign_in("))
        || (text.contains("chatgpt::{") && text.contains("sign_in"))
        || (text.contains("chatgpt as ") && text.contains("sign_in("))
}

/// CHATGPT-1 says only a command a person types, or the desktop window's main process after its
/// person said yes (CHATGPT-15), signs in. The flow is `bravebot_skus::chatgpt`'s `sign_in`, which
/// opens a browser, holds a port and ends with a credential on disk, so a tool, a hook, a slash
/// command or a background host that called it would start that on content's say. The callers are
/// the command's own file and the bridge's accounts file, and a file that names the module and
/// `sign_in` together is a second, as is one that names the module and calls `sign_in` through the
/// crate's alias. A file a test module declares is a test, and is not read. A file that only reads
/// the sign-in file, as the agent's does to tell a first run what is configured, names neither.
#[test]
fn the_chatgpt_sign_in_is_started_by_the_command_and_nothing_else() {
    let callers: BTreeSet<String> = shipped::sources()
        .iter()
        .filter(|source| names_the_chatgpt_sign_in(&source.without_test_modules))
        .map(|source| source.name.clone())
        .collect();
    assert_eq!(
        callers,
        [
            "crates/cli/src/chatgpt.rs",
            "crates/ui-bridge/src/accounts.rs"
        ]
        .map(String::from)
        .into()
    );
}

/// The scan above reads the way a file names the flow, so the ways it can be named are held here:
/// each form that starts the sign-in is found, and the read of the sign-in file is not.
#[test]
fn the_scan_for_the_chatgpt_sign_in_finds_each_way_to_name_it_and_not_a_read_of_the_file() {
    for starts in [
        "bravebot_skus::chatgpt::sign_in(&hosts)",
        "use bravebot_skus::chatgpt;\nchatgpt::sign_in(&hosts)",
        "use bravebot_skus as skus;\nskus::chatgpt::sign_in(&hosts)",
        "use bravebot_skus::chatgpt::sign_in as start;\nstart(&hosts)",
        "use bravebot_skus::chatgpt::*;\nsign_in(&hosts)",
        "use bravebot_skus as skus;\nuse skus::chatgpt::*;\nsign_in(&hosts)",
        "use bravebot_skus as skus;\nuse skus::chatgpt::{Hosts, sign_in};\nsign_in(&hosts)",
        "use bravebot_skus as skus;\nuse skus::chatgpt as flow;\nflow::sign_in(&hosts)",
        "use bravebot_skus as skus;\nuse skus::chatgpt::sign_in as start;\nstart(&hosts)",
    ] {
        assert!(names_the_chatgpt_sign_in(starts), "{starts}");
    }
    for reads in [
        "use bravebot_skus::chatgpt::Account;\nAccount::load()",
        "use bravebot_skus as skus;\nskus::chatgpt::Account::load()",
        "fn needs_sign_in(model: &str) -> bool { false }",
        "use bravebot_skus as skus;\nuse skus::chatgpt::{Account, Hosts};\nAccount::load()",
        "use bravebot_skus as skus;\nuse skus::chatgpt::*;\nlet _ = Account::load();",
    ] {
        assert!(!names_the_chatgpt_sign_in(reads), "{reads}");
    }
}

/// Whether `text` names the xAI flow's `sign_in`.
///
/// Read as text, as [`names_the_chatgpt_sign_in`] is: the module path and the function in one file,
/// the module's path ahead of the call whatever the crate is called, a glob or a group imported from
/// the module under that alias, and the module renamed. A file naming `sign_in` alone is not a
/// caller, and neither is one that names the module and not the function.
fn names_the_xai_sign_in(text: &str) -> bool {
    (text.contains("bravebot_skus::xai") && text.contains("sign_in"))
        || text.contains("xai::sign_in")
        || (text.contains("xai::*") && text.contains("sign_in("))
        || (text.contains("xai::{") && text.contains("sign_in"))
        || (text.contains("xai as ") && text.contains("sign_in("))
}

/// XAI-7 says only a command a person types, or the desktop window's main process after its person
/// said yes (XAI-20), signs in. The flow is `bravebot_skus::xai`'s `sign_in`, which shows a code,
/// waits for it to be used for as long as it lasts and ends with a credential on disk, so a tool, a
/// hook, a slash command or a background host that called it would start that on content's say. The
/// callers are the command's own file and the bridge's accounts file, and a file that names the
/// module and `sign_in` together is a second. A file a test module declares is a test, and is not
/// read. A file that only reads the sign-in file or words its failures names the module and not
/// `sign_in`.
#[test]
fn the_xai_sign_in_is_started_by_the_command_and_nothing_else() {
    let callers: BTreeSet<String> = shipped::sources()
        .iter()
        .filter(|source| names_the_xai_sign_in(&source.without_test_modules))
        .map(|source| source.name.clone())
        .collect();
    assert_eq!(
        callers,
        ["crates/cli/src/xai.rs", "crates/ui-bridge/src/accounts.rs"]
            .map(String::from)
            .into()
    );
}

/// The scan above reads the way a file names the flow, so the ways it can be named are held here:
/// each form that starts the sign-in is found, and the read of the sign-in file is not.
#[test]
fn the_scan_for_the_xai_sign_in_finds_each_way_to_name_it_and_not_a_read_of_the_file() {
    for starts in [
        "bravebot_skus::xai::sign_in(&hosts)",
        "use bravebot_skus::xai;\nxai::sign_in(&hosts)",
        "use bravebot_skus as skus;\nskus::xai::sign_in(&hosts)",
        "use bravebot_skus::xai::sign_in as start;\nstart(&hosts)",
        "use bravebot_skus::xai::*;\nsign_in(&hosts)",
        "use bravebot_skus as skus;\nuse skus::xai::*;\nsign_in(&hosts)",
        "use bravebot_skus as skus;\nuse skus::xai::{Hosts, sign_in};\nsign_in(&hosts)",
        "use bravebot_skus as skus;\nuse skus::xai as flow;\nflow::sign_in(&hosts)",
        "use bravebot_skus as skus;\nuse skus::xai::sign_in as start;\nstart(&hosts)",
    ] {
        assert!(names_the_xai_sign_in(starts), "{starts}");
    }
    for reads in [
        "use bravebot_skus::xai::Account;\nAccount::load()",
        "use bravebot_skus as skus;\nskus::xai::Account::load()",
        "fn needs_sign_in(model: &str) -> bool { false }",
        "use bravebot_skus as skus;\nuse skus::xai::{Account, Hosts};\nAccount::load()",
        "use bravebot_skus as skus;\nuse skus::xai::*;\nlet _ = Account::load();",
        "use bravebot_skus::xai::{Class, TokenError};\nsaid_of_renewal(&failure)",
        "use bravebot_skus::chatgpt::sign_in;\nsign_in(&hosts)",
    ] {
        assert!(!names_the_xai_sign_in(reads), "{reads}");
    }
}

/// The forms a shipped file writes to put a path on a turn's `dropped_text` or `attachments` list,
/// which is what the turn reads unconfined and records a trust rule for. Read as text, so a file
/// that builds the call from a name it computes is not seen. The leading dot keeps out a local
/// variable that happens to be called `attachments`, as the ACP prompt's list is, which reaches the
/// bridge through `turn.send` and is minted there.
const SUPPLIES_A_DROP: [&str; 11] = [
    ".with_dropped_text(",
    ".with_attachment(",
    ".dropped_text.push(",
    ".dropped_text.extend(",
    ".dropped_text = ",
    ".attachments.push(",
    ".attachments.extend(",
    ".attachments = ",
    "turn::Attachment {",
    "dropped_text: ",
    "attachments: ",
];

/// Whether `text` names the unconfined read a drop's path is given, `read_dropped_text` or
/// `read_dropped_attachment`.
fn reads_a_drop(text: &str) -> bool {
    text.contains("read_dropped_text") || text.contains("read_dropped_attachment")
}

/// Whether `text` brings the turn's `Attachment` type into scope with a `use` line, which is what
/// lets a file write the literal `Attachment { .. }` without the path `turn::Attachment {` the list
/// above reads. A `use` group written over several lines is not seen.
fn imports_the_attachment_type(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("use ") && line.contains("turn::") && line.contains("Attachment")
    })
}

/// Whether `text` puts a path on a turn's list of dropped files.
fn supplies_a_drop(text: &str) -> bool {
    SUPPLIES_A_DROP.iter().any(|form| text.contains(form))
        || (imports_the_attachment_type(text) && text.contains("Attachment {"))
}

/// DROP-1 says nothing but the terminal's drop handling and the bridge's `dropped` and `attachments`
/// lists mint a drop's trust grant. A drop is a path on `Task::dropped_text` or `Task::attachments`
/// that the turn reads with `read_dropped_text` or `read_dropped_attachment` and records a rule for,
/// and nothing about the path says a person dropped it. So the files that put a path on either list
/// are the task's own builders, the terminal's turn and side request, and the bridge, and the files
/// that name the reads are the turn, the attached-file reader and the workspace that defines them. A
/// third file that built a task with a path of its own, a hook, a prompt from a server or a command
/// line path, would give a model-proposed path the grant. A file a test module declares is a test,
/// and is not read.
#[test]
fn a_dropped_file_reaches_a_turn_from_the_terminal_and_the_bridge_and_nothing_else() {
    let sources = shipped::sources();
    let names = |found: fn(&str) -> bool| -> BTreeSet<String> {
        sources
            .iter()
            .filter(|source| found(&source.without_test_modules))
            .map(|source| source.name.clone())
            .collect()
    };
    assert_eq!(
        names(supplies_a_drop),
        [
            "crates/agent/src/turn/task.rs",
            "crates/tui/src/app/side_requests.rs",
            "crates/tui/src/app/turns.rs",
            "crates/ui-bridge/src/bridge.rs"
        ]
        .map(String::from)
        .into(),
        "a file that puts a path on a turn's dropped lists"
    );
    assert_eq!(
        names(reads_a_drop),
        [
            "crates/agent/src/attached.rs",
            "crates/agent/src/turn.rs",
            "crates/agent/src/workspace.rs"
        ]
        .map(String::from)
        .into(),
        "a file that reads a path as a drop"
    );
}

/// The scan above reads the way a file names the grant, so the ways it can be named are held here,
/// run over the code the scan reads: each builder and each read is found, and a comment, a test
/// module, a local list that is not the task's and a list that is emptied are not.
#[test]
fn the_scan_for_the_drop_grant_finds_each_way_to_name_it_and_not_a_comment_or_a_test_module() {
    for supplies in [
        "task = task.with_dropped_text(path);",
        "task = task.with_attachment(path, media);",
        "self.dropped_text.push(path.into());",
        "task.dropped_text.extend(paths);",
        "task.dropped_text = paths;",
        "self.attachments.push(Attachment { name, media });",
        "task.attachments.extend(found);",
        "task.attachments = found;",
        "Some(bravebot_agent::turn::Attachment { name, media })",
        "Task { dropped_text: paths, ..Task::new(prompt) }",
        "Task { attachments: found, ..Task::new(prompt) }",
        // The literal written bare, after the type was imported, which names no `turn::` path.
        "use bravebot_agent::turn::Attachment;\nlet one = Attachment { path, media };",
        "use bravebot_agent::turn::{Attachment, Task};\nvec![Attachment { path, media }]",
    ] {
        let code = shipped::code_of(supplies).without_test_modules;
        assert!(supplies_a_drop(&code), "{supplies}");
    }
    for quiet in [
        "// task.with_dropped_text(path)\nfn f() {}",
        "/// the task's `.attachments.push(` is the builder\nfn f() {}",
        "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn g() { Task::new(\"x\").with_attachment(\"a.png\", \"image/png\"); }\n}\nfn h() {}",
        "let mut attachments = Vec::new();\nattachments.push(path);",
        "bare.attachments.clear();\nbare.dropped_text.clear();",
        "let seen = task.dropped_text.len() + task.attachments.len();",
        "let same = task.attachments == other.attachments;",
        "use super::attachments::previews_restaged;\nsuper::attachments::lines_in(&text);",
        // A type of the same name that is not the turn's, and an import that builds nothing.
        "struct Attachment { stream: u8 }\nlet one = Attachment { stream: 1 };",
        "use crate::turn::{Attachment, TurnError};\nfn read(carried: &[Attachment]) {}",
    ] {
        let code = shipped::code_of(quiet).without_test_modules;
        assert!(!supplies_a_drop(&code), "{quiet}");
    }
    for reads in [
        "workspace.read_dropped_text(policy, &path)?;",
        "workspace.read_dropped_attachment(policy, &path, media)",
    ] {
        let code = shipped::code_of(reads).without_test_modules;
        assert!(reads_a_drop(&code), "{reads}");
    }
    for quiet in [
        "/// See [`Workspace::read_dropped_text`] for why that is the boundary.\nfn f() {}",
        "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn g() { w.read_dropped_text(p, q); }\n}\nfn h() {}",
        "workspace.read_text(policy, &path)",
    ] {
        let code = shipped::code_of(quiet).without_test_modules;
        assert!(!reads_a_drop(&code), "{quiet}");
    }
}

/// BACKEND-70 says the window's AWS sign-in is started by its main process after its person said
/// yes. The sign-in that can be stopped from another thread is that one: the agent's wrapper names
/// it to reach the AWS crate, and the bridge's file for the AWS row is its only caller. The sign-in
/// a turn makes before it starts (BACKEND-9) and the command's own are the other, uncancellable
/// one.
#[test]
fn the_cancellable_aws_sign_in_is_started_by_the_window_and_nothing_else() {
    let callers = shipped::sources()
        .iter()
        .filter(|source| {
            let text = &source.without_test_modules;
            text.contains("sign_in_to_until") || text.contains("sign_in_if_needed_until")
        })
        .map(|source| source.name.clone())
        .collect::<BTreeSet<String>>();
    assert_eq!(
        callers,
        [
            "crates/agent/src/backend.rs",
            "crates/bedrock/src/credentials.rs",
            "crates/ui-bridge/src/accounts_bedrock.rs"
        ]
        .map(String::from)
        .into()
    );
}

/// The scan that holds the kernel to no I/O has to see each way of reaching out, or the test
/// above passes on a crate that has found one it does not look for.
#[test]
fn the_io_scan_sees_a_grouped_import_a_macro_and_skips_tests_and_comments() {
    for reaching in [
        "use std::fs;",
        "let _ = std::env::var(\"HOME\");",
        "use std::{\n    collections::BTreeMap,\n    io,\n};",
        "use std::process::Command;",
        "println!(\"{x}\");",
        "eprintln!(\"{x}\");",
        "let _ = std::net::TcpStream::connect(a);",
    ] {
        assert!(
            !reaching_out(reaching).is_empty(),
            "a use that reaches out was not seen: {reaching}"
        );
    }
    for quiet in [
        "use std::collections::BTreeMap;\nuse std::net::IpAddr;\nuse std::path::Path;",
        "// reads nothing: no std::fs here, and no println! either\nfn f() {}",
        "fn f() {}\n#[cfg(test)]\nmod tests {\n    use std::fs;\n    fn g() { println!(\"x\"); }\n}\nfn h() {}",
    ] {
        assert!(
            reaching_out(quiet).is_empty(),
            "a use that reaches nowhere was reported: {quiet}"
        );
    }
}

/// The scan that holds the crate reading an answer to keeping nothing has to see each way of keeping
/// a value, or the test of that crate passes on a source that has found one it does not look for.
/// It also has to leave alone what keeps nothing: a lifetime named `'static`, a name that only
/// contains a refused word, comment lines and test modules. A scan that refused those would make
/// the crate's author reword code that is right, and the test would be switched off to pass.
#[test]
fn the_state_scan_sees_each_refused_word_and_skips_the_lifetime_tests_and_comments() {
    let kept: &[(&str, &[&str])] = &[
        ("static KEPT: u8 = 0;", &["static"]),
        ("static mut N: u8 = 0;", &["static"]),
        ("pub static N: u8 = 0;", &["static"]),
        ("static\nKEPT: u8 = 0;", &["static"]),
        (
            "static KEPT: Mutex<String> = Mutex::new(String::new());",
            &["static", "Mutex"],
        ),
        ("thread_local!(N);", &["thread_local"]),
        (
            "thread_local! {\n    static N: u8 = 0;\n}",
            &["thread_local", "static"],
        ),
        ("let c: Cell<u8>;", &["Cell"]),
        ("let c: RefCell<u8>;", &["RefCell"]),
        ("let c: std::cell::OnceCell<u8>;", &["OnceCell"]),
        ("let c: UnsafeCell<u8>;", &["UnsafeCell"]),
        ("let a = AtomicU8::new(0);", &["AtomicU8"]),
        ("let a: std::sync::atomic::AtomicBool;", &["AtomicBool"]),
        ("let a: AtomicUsize;", &["AtomicUsize"]),
        ("let l: RwLock<u8>;", &["RwLock"]),
        ("let o: OnceLock<u8>;", &["OnceLock"]),
        ("let l: LazyLock<u8>;", &["LazyLock"]),
        ("use std::sync::{Arc, Mutex};", &["Mutex"]),
        (
            "let s: &'static str = \"a\"; static N: u8 = 0;",
            &["static"],
        ),
        (
            "static A: u8 = 0; static B: u8 = 0; RefCell RefCell AtomicU8",
            &["static", "RefCell", "AtomicU8"],
        ),
        (
            "fn f() {}\n#[cfg(test)]\nmod tests {\n    static N: u8 = 0;\n}\nstatic KEPT: u8 = 0;",
            &["static"],
        ),
    ];
    for &(source, expected) in kept {
        assert_eq!(
            keeping_state(source),
            expected,
            "the words that keep a value were not the ones in: {source}"
        );
    }

    for quiet in [
        "fn f(s: &'static str) -> &'static str { s }",
        "const T: [&'static str; 1] = [\"a\"];",
        "impl<T: 'static> X for T {}",
        "struct Cellar;\nstruct Atomically;",
        "let (head, tail) = value.split_once(':')?;",
        "let statics = 1;\nlet static_table = 2;",
        "// static KEPT: Mutex<u8>; thread_local! RefCell AtomicU8 OnceLock\nfn f() {}",
        "/// A static table, a Mutex, a Cell and a LazyLock.\nfn f() {}",
        "struct S {\n    /// A Cell of an AtomicBool.\n    a: u8,\n}",
        "fn f() {}\n#[cfg(test)]\nmod tests {\n    static N: u8 = 0;\n    use std::cell::Cell;\n    use std::sync::Mutex;\n}\nfn h() {}",
    ] {
        assert_eq!(
            keeping_state(quiet),
            Vec::<String>::new(),
            "a source that keeps nothing was reported: {quiet}"
        );
    }

    assert_eq!(name_parts("RefCell"), ["Ref", "Cell"]);
    assert_eq!(name_parts("OnceCell"), ["Once", "Cell"]);
    assert_eq!(name_parts("AtomicU8"), ["Atomic", "U8"]);
    assert_eq!(name_parts("AtomicBool"), ["Atomic", "Bool"]);
    assert_eq!(name_parts("Cellar"), ["Cellar"]);
    assert_eq!(name_parts("Atomically"), ["Atomically"]);
    assert_eq!(name_parts("split_once"), ["split", "once"]);
}

/// A reader that accepts the wrong shapes enforces nothing, and the wrong shapes here are the ones
/// a table drifting out of the spec's own format would have: a separator taken for data, a column
/// added or removed, and a line that is no table row at all.
#[test]
fn the_table_reader_rejects_what_is_not_a_row() {
    assert_eq!(cells("|---|---|---|---|"), None);
    assert_eq!(cells("not a table row"), None);
    assert_eq!(
        cells("| a | b |"),
        Some(vec!["a".to_string(), "b".to_string()])
    );

    assert_eq!(depends_on("none"), BTreeSet::new());
    assert_eq!(
        depends_on("`core`, `net`"),
        BTreeSet::from(["bravebot-core".to_string(), "bravebot-net".to_string()])
    );

    let header = "| Crate | Purpose | Depends on | Constraint |\n|---|---|---|---|\n";
    let spec = format!("<a id=\"LAYER-1\"></a>\n### LAYER-1\n\n{header}");
    assert_eq!(layering_table(&spec), Vec::new());
}

/// A cell nobody filled in is not a crate that depends on nothing. Read as the empty set it would
/// make the row say what the kernel's row says, which is the strongest claim the table makes.
#[test]
#[should_panic(expected = "neither a backticked crate name nor the word `none`")]
fn a_blank_dependency_cell_is_not_a_crate_that_depends_on_nothing() {
    depends_on("");
}

/// The header says which column is which, so a table that lost it has no row meaning what this
/// reader takes it to mean. Skipping the first row parsed instead would quietly drop the kernel's
/// row, which is the one row whose dependency list carries an argument.
#[test]
#[should_panic(expected = "0 header rows")]
fn a_table_that_lost_its_header_is_not_read() {
    let rows = "| `bravebot-core` | The kernel | none | No I/O |\n";
    layering_table(&format!("<a id=\"LAYER-1\"></a>\n### LAYER-1\n\n{rows}"));
}

/// Whether a row is a surface decides whether the clause about marking has to name it, and the
/// word appears in the table about two different things: a crate that draws released content, and
/// the catalogs, which hold text a person reads and put none of it on a screen themselves.
#[test]
fn a_constraint_says_whether_its_crate_is_a_surface() {
    assert!(is_a_surface("Presentation. May display released content"));
    assert!(!is_a_surface("Presentation text only. Holds nothing"));
    assert!(!is_a_surface("Auth only. Carries no workspace content"));
    assert!(!is_a_surface("No I/O, and nothing prints"));
}

/// An opening that uses the word for something this reader has not been told about is the case
/// where guessing is worst: read as a surface it demands a clause that may be wrong, and read as
/// anything else it drops the marking rule from a crate that draws content.
#[test]
#[should_panic(expected = "which is neither")]
fn a_constraint_that_uses_the_word_for_something_else_is_an_error() {
    is_a_surface("Presentation layer. May display released content");
}

/// The dependency reader has to see an edge however the manifest spells it, and has to see only
/// edges. Cargo takes a dependency as one line, as a table of its own, under a caller's own name
/// with the real crate in a `package` key, and per platform, and every one of those is the same
/// reach granted. A crate named in a comment, in the repository URL, as the package's own name, or
/// in the workspace's catalogue of versions is not an edge of this crate at all.
#[test]
fn the_manifest_reader_sees_an_edge_and_not_a_mention() {
    let manifest = "\
[package]
name = \"bravebot-cli\"
repository = \"https://github.com/brave/bravebot\"

[workspace.dependencies]
bravebot-signing = { path = \"crates/signing\" }

[dependencies]
# bravebot-skus is deliberately absent.
bravebot-agent = { path = \"../agent\" }
bravebot-tui.workspace = true
terminal = { package = \"bravebot-net\", path = \"../net\" }

[dependencies.bravebot-config]
path = \"../config\"

[dependencies.catalogs]
package = \"bravebot-i18n\"
path = \"../i18n\"

[target.'cfg(unix)'.dependencies]
bravebot-sandbox = { path = \"../sandbox\" }

[dev-dependencies]
bravebot-core = { path = \"../core\", features = [\"testing\"] }

[build-dependencies]
bravebot-lsp = { path = \"../lsp\" }
ratatui = { workspace = true }
term = { package = 'ratatui-core', version = \"0.1\" }
";
    assert_eq!(
        workspace_dependencies(manifest),
        BTreeSet::from([
            "bravebot-agent".to_string(),
            "bravebot-config".to_string(),
            "bravebot-core".to_string(),
            "bravebot-i18n".to_string(),
            "bravebot-lsp".to_string(),
            "bravebot-net".to_string(),
            "bravebot-sandbox".to_string(),
            "bravebot-tui".to_string(),
        ])
    );
    // A crate from outside this workspace is an edge too, and the rule about reaching no terminal
    // library is entirely about those: a reader that dropped them would report the promise kept by
    // every crate that broke it. A rename spelled with TOML's other quote is the same edge, and
    // read as double-quoted only it comes back as the version string beside it.
    assert_eq!(
        dependencies(manifest)
            .into_iter()
            .filter(|name| !name.starts_with(PREFIX))
            .collect::<BTreeSet<String>>(),
        BTreeSet::from(["ratatui".to_string(), "ratatui-core".to_string()])
    );
}

/// A clause body is bounded by the next clause, so a crate named further down the spec is not a
/// crate this one named. Unbounded, the list of surfaces that mark their content would pick up
/// every crate the unsafe clause mentions and never fail.
#[test]
fn a_clause_body_stops_at_the_next_clause() {
    let spec = "\
<a id=\"LAYER-3\"></a>
### LAYER-3: a heading

`bravebot-tui` shows content.

<a id=\"LAYER-4\"></a>
### LAYER-4: another heading

`bravebot-sandbox` names a site.
";
    assert_eq!(
        named_crates(&clause(spec, "LAYER-3")),
        BTreeSet::from(["bravebot-tui".to_string()])
    );
}
