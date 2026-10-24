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
//! [LAYER-1]: ../../../docs/specs/layering.md
//! [LAYER-3]: ../../../docs/specs/layering.md

use std::collections::BTreeSet;
use std::path::PathBuf;

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

/// The workspace root, two levels above this crate's manifest.
fn workspace() -> PathBuf {
    // CARGO_MANIFEST_DIR is `<workspace>/crates/cli`, so two pops reach the root.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path
}

/// The body of one clause: everything from its anchor to the next anchor or the next heading.
///
/// Reading to the end of the file instead would take LAYER-4's mention of three crates for a
/// statement LAYER-3 makes, so the bound matters rather than being tidiness.
fn clause(spec: &str, id: &str) -> String {
    let anchor = format!("<a id=\"{id}\"></a>");
    let (_, rest) = spec
        .split_once(&anchor)
        .unwrap_or_else(|| panic!("{id} has no anchor in layering.md"));
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

/// A source file as a release build reads it: the test modules left out, and the comments.
///
/// A test module is `#[cfg(test)]` and then `mod`, with attributes such as `#[path]` between. One
/// with a body is closed by rustfmt at column 0, so the first bare brace ends it. One declared as
/// `mod name;` is the line alone, and the code after it is read. A line comment is skipped whole,
/// since the crates say in prose what they do not do.
fn production_code(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut code = String::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        if is_test_gate(line) {
            let mut declared = at + 1;
            while lines
                .get(declared)
                .is_some_and(|next| next.trim_start().starts_with("#["))
            {
                declared += 1;
            }
            if let Some(module) = lines
                .get(declared)
                .filter(|next| module_name(next).is_some())
            {
                if module.trim_end().ends_with(';') {
                    at = declared + 1;
                    continue;
                }
                if !module.starts_with(char::is_whitespace) {
                    at = declared + 1;
                    if !module.trim_end().ends_with('}') {
                        while at < lines.len() && lines[at] != "}" {
                            at += 1;
                        }
                        at += 1;
                    }
                    continue;
                }
            }
        }
        if !line.trim().starts_with("//") {
            code.push_str(line);
            code.push('\n');
        }
        at += 1;
    }
    code
}

/// Whether a line gates the item after it on a test build: `#[cfg(test)]`, or `#[cfg(all(test, ...))]`
/// as `#[cfg(all(test, unix))]` does for a test that needs a Unix.
fn is_test_gate(line: &str) -> bool {
    let line = line.trim();
    line == "#[cfg(test)]" || line.starts_with("#[cfg(all(test,")
}

/// The name a line declares as a module, `mod name;` or `mod name {`, with any visibility before
/// it.
fn module_name(line: &str) -> Option<&str> {
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
fn declared_test_modules(source: &str) -> Vec<(Option<String>, String)> {
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
fn module_files(declaring: &std::path::Path, path: Option<&str>, name: &str) -> Vec<PathBuf> {
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

/// Every file under a crate's `src` that a release build compiles, with the code it contributes. A
/// file that only a test module declares is a test and is left out with the files under its
/// directory, and the test modules and comments of the others are left out of their text.
fn release_sources() -> Vec<(PathBuf, String)> {
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
    texts
        .into_iter()
        .filter(|(file, _)| {
            !test_files.contains(file)
                && !test_directories
                    .iter()
                    .any(|directory| file.starts_with(directory))
        })
        .map(|(file, text)| (file, production_code(&text)))
        .collect()
}

/// The `std::` paths and print macros a source file's non-test code uses that a crate promising
/// "no I/O" and "nothing prints" may not, one description per use.
///
/// Test modules are skipped, since a test reads fixtures. `use std::{fs, io}` and
/// `use std::{\n    fs,\n}` are read as the paths they are, so grouping the import is not a way past
/// it. Comments are skipped, because the crates say in prose what they do not do.
fn reaching_out(source: &str) -> Vec<String> {
    let code = production_code(source);
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

/// The files under `directory`, depth first, in no particular order.
fn rust_files(directory: &std::path::Path) -> Vec<PathBuf> {
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

/// LAYER-1 says the kernel does no I/O and prints nothing, and the file-type crate does no I/O. A
/// manifest cannot say it: both depend on nothing outside the standard library, which is where
/// the file system, the environment, child processes and the terminal live. So what holds the
/// promise is the code never naming them, read off the source with the tests left out.
#[test]
fn the_kernel_and_the_file_type_crate_do_no_io_and_print_nothing() {
    for krate in ["core", "filetype"] {
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

/// A path as the repository names it: relative to the workspace and with `/` between components.
fn relative(path: &std::path::Path) -> String {
    path.strip_prefix(workspace())
        .expect("a path under the workspace")
        .to_string_lossy()
        .replace('\\', "/")
}

/// CHATGPT-1 says only a command a person types signs in. The flow is `bravebot_skus::chatgpt`'s
/// `sign_in`, which opens a browser, holds a port and ends with a credential on disk, so a tool, a
/// hook, a slash command or a background host that called it would start that on content's say. The
/// one caller is the command's own file, and a file that names the module and `sign_in` together is
/// a second, as is one that names the module and calls `sign_in` through the crate's alias. A file a
/// test module declares is a test, and is not read. A file that only reads the sign-in file, as the
/// agent's does to tell a first run what is configured, names neither.
#[test]
fn the_chatgpt_sign_in_is_started_by_the_command_and_nothing_else() {
    let sources = release_sources();
    assert!(
        sources.len() > 100,
        "{} source files, so this test would pass by scanning almost nothing",
        sources.len()
    );
    let callers: Vec<String> = sources
        .iter()
        .filter(|(_, code)| names_the_chatgpt_sign_in(code))
        .map(|(file, _)| relative(file))
        .collect();
    assert_eq!(callers, ["crates/cli/src/chatgpt.rs"]);
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

/// The features that exist so a test can point a flow at a server of its own
/// ([CHATGPT-2](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2)). A build with one reads a variable
/// that names where a token goes, so no build that ships may have it.
const TEST_SEAMS: [&str; 1] = ["test-hosts"];

/// What a Cargo file says about the test seams: the lines that name one anywhere but in a table of
/// dev-dependencies or the seam's own declaration, `<seam> = []` under `[features]`, and the lines
/// that name one in a table of dev-dependencies. The escapes of a TOML string that write a character
/// by its number are decoded first, since the workspace has no TOML parser to do it (the crate of
/// this test depends on none, and this change adds none).
///
/// A seam a crate's `[dependencies]`, its build dependencies, the workspace's catalogue, a feature of
/// any crate (`default` included) or a Cargo configuration file enables is on in the build that ships,
/// where cargo builds a crate's dev-dependencies only for its tests. A declaration that enables
/// something else would turn that on with it.
fn seam_mentions(manifest: &str) -> (Vec<String>, Vec<String>) {
    let (mut outside, mut inside) = (Vec::new(), Vec::new());
    let mut table: Vec<String> = Vec::new();
    // A TOML string may write a character by its number, so `"test\u002dhosts"` is the feature's
    // name, and no parser here decodes it.
    let manifest = with_escapes_decoded(manifest);
    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            table = line
                .trim_matches(|c| c == '[' || c == ']')
                .split('.')
                .map(|part| {
                    part.trim()
                        .trim_matches(|c| c == '"' || c == '\'')
                        .to_string()
                })
                .collect();
            if !TEST_SEAMS.iter().any(|seam| line.contains(seam)) {
                continue;
            }
        }
        let Some(seam) = TEST_SEAMS.iter().find(|seam| line.contains(*seam)) else {
            continue;
        };
        let for_tests = table.iter().any(|part| part == "dev-dependencies");
        let declared = table == ["features"]
            && line
                .split_once('=')
                .is_some_and(|(key, value)| key.trim() == *seam && value.trim() == "[]");
        if for_tests {
            inside.push(line.to_string());
        } else if !declared {
            outside.push(line.to_string());
        }
    }
    (outside, inside)
}

/// Directories no walk of the repository goes into: built output, installed packages, git's own
/// files, a sibling checkout kept under `.worktrees` and a bytecode cache.
const NOT_WALKED: [&str; 5] = [
    "target",
    "node_modules",
    ".git",
    ".worktrees",
    "__pycache__",
];

/// Every file under `directory` that `keep` accepts, by path, depth first.
fn files_where(
    directory: &std::path::Path,
    keep: &impl Fn(&std::path::Path) -> bool,
) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !NOT_WALKED.iter().any(|skipped| name == *skipped) {
                found.extend(files_where(&path, keep));
            }
        } else if keep(&path) {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// Every manifest in the repository, and every Cargo configuration file, whichever workspace it
/// belongs to: the fuzz package is a workspace of its own and the root's member list does not name
/// it.
fn cargo_files() -> Vec<(String, String)> {
    files_where(&workspace(), &|path| {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let in_cargo_directory = path
            .parent()
            .and_then(std::path::Path::file_name)
            .is_some_and(|directory| directory == ".cargo");
        name == "Cargo.toml" || (in_cargo_directory && name.starts_with("config"))
    })
    .into_iter()
    .map(|path| {
        let text = std::fs::read_to_string(&path).expect("a Cargo file");
        (relative(&path), text)
    })
    .collect()
}

/// The manifests whose `[dev-dependencies]` turn a seam on, which are the crates whose tests run a
/// flow against a stand-in: the command's, for the built binary, and the two front ends', for what
/// each writes of a failed turn. A crate added to this list is a decision about who may name a host
/// in a test.
const ENABLE_A_SEAM_FOR_TESTS: [&str; 3] = [
    "crates/cli/Cargo.toml",
    "crates/tui/Cargo.toml",
    "crates/ui-bridge/Cargo.toml",
];

/// CHATGPT-2: a test seam is turned on by dev-dependencies and by nothing a release build reads.
/// Cargo builds a crate's dev-dependencies, and the features they ask for, only for that crate's
/// tests, so a seam enabled there is in the binary `cargo test` runs and in no binary a release
/// builds. A manifest that enabled it under `[dependencies]`, in `default` or in another feature
/// would put a variable that chooses where a token goes into the shipped binary. Every `Cargo.toml`
/// in the repository is read, the fuzz package's included, and so is every Cargo configuration file,
/// which could pass the feature as a flag. The seams are declared and enabled somewhere, so the test
/// is not satisfied by a tree that dropped them.
#[test]
fn a_test_seam_is_enabled_by_dev_dependencies_alone() {
    let files = cargo_files();
    let names: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    assert!(
        files.len() > 20
            && names.contains(&"Cargo.toml")
            && names.contains(&"fuzz/Cargo.toml")
            && names.contains(&"crates/skus/Cargo.toml"),
        "{names:?}, so this test would pass by reading almost nothing"
    );

    let mut outside: Vec<(String, String)> = Vec::new();
    let mut enabling: Vec<&str> = Vec::new();
    for (path, manifest) in &files {
        let (named_outside, named_inside) = seam_mentions(manifest);
        for line in named_outside {
            outside.push((path.clone(), line));
        }
        if !named_inside.is_empty() {
            enabling.push(path);
        }
    }
    assert!(
        outside.is_empty(),
        "a test seam is named where a build that ships would read it: {outside:?}"
    );
    // And no manifest or configuration file turns debug assertions on, makes a panic unwind,
    // overrides the release profile or defines a profile cargo does not have of its own.
    let mut bypassing = Vec::new();
    for (path, manifest) in &files {
        for spelling in bypasses_of_the_refusal(manifest) {
            bypassing.push((path.as_str(), spelling.to_string()));
        }
        for name in custom_profiles(path, manifest) {
            bypassing.push((path.as_str(), format!("the profile {name}")));
        }
    }
    assert!(
        bypassing.is_empty(),
        "a Cargo file undoes what makes a release build refuse the test-hosts seam: {bypassing:?}"
    );
    assert_eq!(
        enabling, ENABLE_A_SEAM_FOR_TESTS,
        "the crates whose tests turn a seam on are the ones that run a flow against a stand-in, \
         and a crate added to them is a decision about who may name a host in a test"
    );

    for seam in TEST_SEAMS {
        let mut declaring: Vec<&str> = files
            .iter()
            .filter(|(_, manifest)| {
                manifest
                    .lines()
                    .any(|line| line.trim().replace(' ', "") == format!("{seam}=[]"))
            })
            .map(|(path, _)| path.as_str())
            .collect();
        declaring.sort_unstable();
        assert_eq!(
            declaring,
            ["crates/config/Cargo.toml", "crates/skus/Cargo.toml"],
            "{seam}"
        );
    }
}

/// The reader of the test seams finds one in every place a shipping build would read it, and none
/// in a table of dev-dependencies or the seam's own declaration.
#[test]
fn the_seam_reader_finds_a_seam_wherever_a_build_that_ships_reads_it() {
    let manifest = "\
[workspace.dependencies]
bravebot-skus = { path = \"crates/skus\", features = [\"test-hosts\"] }

[features]
test-hosts = []
default = [\"test-hosts\"]
everything = [\"bravebot-skus/test-hosts\"]

[dependencies]
bravebot-config = { path = \"../config\", features = [\"test-hosts\"] }

[dependencies.bravebot-skus]
features = ['test-hosts']

[target.'cfg(unix)'.build-dependencies]
bravebot-net = { path = \"../net\", features = [\"test-hosts\"] }

[dev-dependencies]
bravebot-skus = { path = \"../skus\", features = [\"test-hosts\"] }

[target.'cfg(unix)'.dev-dependencies.bravebot-config]
features = [\"test-hosts\"]
";
    let (outside, inside) = seam_mentions(manifest);
    assert_eq!(
        outside,
        [
            "bravebot-skus = { path = \"crates/skus\", features = [\"test-hosts\"] }",
            "default = [\"test-hosts\"]",
            "everything = [\"bravebot-skus/test-hosts\"]",
            "bravebot-config = { path = \"../config\", features = [\"test-hosts\"] }",
            "features = ['test-hosts']",
            "bravebot-net = { path = \"../net\", features = [\"test-hosts\"] }",
        ]
    );
    assert_eq!(
        inside,
        [
            "bravebot-skus = { path = \"../skus\", features = [\"test-hosts\"] }",
            "features = [\"test-hosts\"]",
        ]
    );
    assert_eq!(
        seam_mentions("[features]\ntest-hosts = [\"bravebot-net/proxy\"]\n").0,
        ["test-hosts = [\"bravebot-net/proxy\"]"]
    );
    // A feature written with the escapes of a TOML string is the feature.
    for manifest in [
        "[dependencies]\nbravebot-skus = { path = \"../skus\", features = [\"test\\u002dhosts\"] }\n",
        "[dependencies]\nbravebot-skus = { path = \"../skus\", features = [\"\\u0074est-hosts\"] }\n",
        "[features]\ndefault = [\"bravebot-skus/test\\U0000002dhosts\"]\n",
        "[dependencies]\nbravebot-skus = { path = \"../skus\", features = [\"test\\x2dhosts\"] }\n",
    ] {
        assert_eq!(seam_mentions(manifest).0.len(), 1, "{manifest}");
    }
    // The same in a table of dev-dependencies is the use that is allowed.
    let (outside, inside) = seam_mentions(
        "[dev-dependencies]\nbravebot-skus = { path = \"../skus\", features = [\"test\\u002dhosts\"] }\n",
    );
    assert!(outside.is_empty() && inside.len() == 1);
    // A Cargo configuration file has no dependency tables, so a flag in one is a use.
    assert_eq!(
        seam_mentions("[build]\nrustflags = [\"--cfg\", \"feature=\\\"test-hosts\\\"\"]\n")
            .0
            .len(),
        1
    );
    assert_eq!(
        seam_mentions("[alias]\nb = \"build --features bravebot-skus/test-hosts\"\n")
            .0
            .len(),
        1
    );
}

/// The attribute and the `compile_error!` under it that refuse a test seam, as a crate's `lib.rs`
/// writes them: from the `#[cfg(` line above the macro to the line that ends it.
fn seam_refusal(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let refusal = lines
        .iter()
        .position(|line| line.trim_start().starts_with("compile_error!("))
        .expect("a compile_error! that refuses the seam");
    let attribute = (0..refusal)
        .rev()
        .find(|at| lines[*at].starts_with("#[cfg("))
        .expect("a cfg above the compile_error!");
    let end = (refusal..lines.len())
        .find(|at| lines[*at].trim_end().ends_with(");"))
        .expect("the end of the compile_error!");
    lines[attribute..=end].join("\n")
}

/// Whether the compiler refuses `refusal` under `flags`, with the feature on or off: it is built as a
/// crate of its own, so only the attribute's own condition decides. Returns what the compiler said
/// where it did, and `None` where the crate compiled.
#[allow(
    clippy::disallowed_methods,
    reason = "the test runs the compiler the tree is built with, to see what the attribute does under flags a test cannot set on itself"
)]
fn refused_under(refusal: &str, flags: &[&str], feature_on: bool) -> Option<String> {
    let directory = workspace().join("target/test-scratch/chatgpt-seam-refusal");
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    let source = directory.join("refusal.rs");
    std::fs::write(&source, format!("{refusal}\nfn main() {{}}\n")).expect("the refusal");
    let mut command =
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    command
        .args([
            "--edition",
            "2024",
            "--crate-type",
            "bin",
            "--emit=metadata",
        ])
        .arg("--out-dir")
        .arg(&directory)
        .args(flags);
    if feature_on {
        command.args(["--cfg", "feature=\"test-hosts\""]);
    }
    let output = command
        .arg(&source)
        .output()
        .expect("the compiler runs: it is the one that builds this test");
    (!output.status.success()).then(|| String::from_utf8_lossy(&output.stderr).into_owned())
}

/// CHATGPT-2: a release build refuses the seam. Rust has no `cfg` for the release profile, so the
/// refusal reads the two settings that profile has and a test build does not: debug assertions off,
/// and a panic that aborts. A build with debug assertions on and a panic that aborts is the release
/// profile given `debug-assertions = true`, and a build with them off that unwinds is
/// `cargo test --release`; the first is refused by the abort and the second by the assertions. The
/// attribute is compiled under each combination, with the feature on and off, and the root manifest's
/// release profile is read for the setting the abort rests on.
#[test]
fn a_build_that_aborts_on_a_panic_or_asserts_nothing_refuses_the_seam() {
    let read = |path: &str| std::fs::read_to_string(workspace().join(path)).expect("a source");
    let refusal = seam_refusal(&read("crates/skus/src/lib.rs"));
    assert_eq!(
        refusal,
        seam_refusal(&read("crates/config/src/lib.rs")),
        "the two crates that hold the seam refuse it under different conditions"
    );

    let cases: [(&str, &[&str], bool, bool); 7] = [
        (
            "a test or dev build",
            &["-C", "debug-assertions=on"],
            true,
            false,
        ),
        (
            "a dev build whose dependencies are optimised",
            &["-C", "debug-assertions=on", "-C", "opt-level=2"],
            true,
            false,
        ),
        (
            "the release profile",
            &[
                "-C",
                "debug-assertions=off",
                "-C",
                "panic=abort",
                "-C",
                "opt-level=3",
            ],
            true,
            true,
        ),
        (
            "the release profile with debug assertions turned on",
            &[
                "-C",
                "debug-assertions=on",
                "-C",
                "panic=abort",
                "-C",
                "opt-level=3",
            ],
            true,
            true,
        ),
        (
            "a build with debug assertions on and a panic that aborts",
            &["-C", "debug-assertions=on", "-C", "panic=abort"],
            true,
            true,
        ),
        (
            "tests built with the release profile",
            &["-C", "debug-assertions=off", "-C", "opt-level=3"],
            true,
            true,
        ),
        (
            "a release build without the feature",
            &["-C", "debug-assertions=off", "-C", "panic=abort"],
            false,
            false,
        ),
    ];
    for (build, flags, feature_on, refused) in cases {
        let said = refused_under(&refusal, flags, feature_on);
        assert_eq!(
            said.is_some(),
            refused,
            "{build}: expected the seam to be {}, and the compiler said {said:?}",
            if refused { "refused" } else { "accepted" }
        );
        if let Some(said) = said {
            assert!(
                said.contains("the test-hosts feature is for tests"),
                "{build}: the compiler refused for another reason: {said}"
            );
        }
    }

    // The abort the refusal reads as the release profile is the manifest's.
    let manifest = read("Cargo.toml");
    let mut in_release = false;
    let mut release: Vec<&str> = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_release = line == "[profile.release]";
        } else if in_release && !line.is_empty() && !line.starts_with('#') {
            release.push(line);
        }
    }
    assert!(
        release.contains(&"panic = \"abort\""),
        "the release profile does not abort on a panic, which is how the test-hosts refusal tells \
         it from a test build: {release:?}"
    );
    assert!(
        !release
            .iter()
            .any(|line| line.starts_with("debug-assertions")),
        "the release profile states debug assertions: {release:?}"
    );
}

/// The directories the walk for commands does not go into, beyond [`NOT_WALKED`]: the output of a
/// build, which a checkout may hold and a clean one does not.
const NOT_WALKED_FOR_COMMANDS: [&str; 4] = ["dist", "out", "build", ".docusaurus"];

/// The directory the walk for commands leaves out because it says in words what the guards look for.
const SPECS: &str = "docs/specs";

/// The files that list what a lock pins, which a tool writes and which hold no command. Named in
/// full: a name that only contains `lock` would leave out a script called `block.sh`.
const LOCK_FILES: [&str; 5] = [
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "Podfile.lock",
];

/// Whether `path`, relative to the workspace, is a file the walk for commands reads: every file
/// there is, whatever it is called and whatever it ends in, but a Rust source, a lock file, a
/// `Cargo.toml` and what is under [`SPECS`].
///
/// A Rust source is read for who may name a host by the tests of what a release compiles, and a
/// `Cargo.toml` by the manifest guard, which reads its tables. A file that is not text is left out
/// where it is read, in [`command_files`]. The name of a file says nothing of whether it holds a
/// command, so no other name is left out: a script that ends in `.bash`, a `Justfile` and a
/// `Makefile.local` are read as a `Makefile` is.
fn holds_a_command(path: &std::path::Path) -> bool {
    let relative = relative(path);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    !(relative.starts_with(&format!("{SPECS}/"))
        || name == "Cargo.toml"
        || LOCK_FILES.contains(&name)
        || path.extension().is_some_and(|extension| extension == "rs"))
}

/// Every file the walk for commands reads, by path relative to the workspace and its text. A file
/// with a NUL byte in it is an image, an archive or a font, and holds no command.
fn command_files() -> Vec<(String, String)> {
    fn walk(directory: &std::path::Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                let skipped = NOT_WALKED
                    .iter()
                    .chain(NOT_WALKED_FOR_COMMANDS.iter())
                    .any(|skipped| name == *skipped);
                if !skipped {
                    walk(&path, found);
                }
            } else if holds_a_command(&path) {
                found.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(&workspace(), &mut paths);
    paths.sort();
    paths
        .iter()
        .filter_map(|path| {
            let bytes = std::fs::read(path).ok()?;
            (!bytes.contains(&0))
                .then(|| (relative(path), String::from_utf8_lossy(&bytes).into_owned()))
        })
        .collect()
}

/// The commands in the text of a file, one per logical line: a line that ends in a backslash goes on
/// with the next, as a shell and a Makefile read it.
fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut joined = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        match trimmed.strip_suffix('\\') {
            Some(continued) => {
                joined.push_str(continued.trim_end());
                joined.push(' ');
            }
            None => {
                joined.push_str(trimmed);
                lines.push(std::mem::take(&mut joined));
            }
        }
    }
    if !joined.is_empty() {
        lines.push(joined);
    }
    lines
}

/// `text` with the escapes of a TOML or a JSON string that write a character by its number
/// (`\u002d`, `\U0000002d`, `\x2d`) written out as the character. A manifest has no TOML parser here
/// to do it, and a `package.json` command is a JSON string: either could spell a feature's name with
/// one. A backslash before a backslash is a backslash, so `\\u002d` is left as it is.
fn with_escapes_decoded(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut decoded = String::with_capacity(text.len());
    let mut at = 0;
    while at < characters.len() {
        if characters[at] == '\\' && at + 1 < characters.len() {
            let digits = match characters[at + 1] {
                'u' => 4,
                'U' => 8,
                'x' => 2,
                '\\' => {
                    decoded.push_str("\\\\");
                    at += 2;
                    continue;
                }
                _ => 0,
            };
            let end = at + 2 + digits;
            if digits > 0 && end <= characters.len() {
                let number: String = characters[at + 2..end].iter().collect();
                if let Some(character) = u32::from_str_radix(&number, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    decoded.push(character);
                    at = end;
                    continue;
                }
            }
        }
        decoded.push(characters[at]);
        at += 1;
    }
    decoded
}

/// `text` with what separates the pieces of one word in a command taken out: white space, quotes,
/// backslashes, commas and plus signs, in lower case with `_` read as `-`, so that a name split over
/// lines, written as a list of arguments, quoted in any way, written with the escapes of a string or
/// spelled as an environment variable is the same text as the name.
fn squeezed(text: &str) -> String {
    with_escapes_decoded(text)
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '`' | '\\' | ',' | '+'))
        .map(|c| {
            if c == '_' {
                '-'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// Whether `text` names a test seam in any way a command could: the name is not a word of any
/// command a release runs, so a file that holds it, in a line or beside the flag or on a line of its
/// own, is one a person has to look at.
fn names_a_test_seam(text: &str) -> bool {
    let squeezed = squeezed(text);
    TEST_SEAMS
        .iter()
        .chain(SEAM_VARIABLE_WORDS.iter())
        .any(|word| squeezed.contains(word))
}

/// What the variable that names a stand-in is called, in the form `squeezed` gives it, which is also
/// the part of its name no other variable has. A command that sets it names the seam as a flag does.
const SEAM_VARIABLE_WORDS: [&str; 1] = ["chatgpt-stand-in"];

/// The logical lines of `text` that ask for every feature, which enables each seam for every crate
/// the build selects: `--all-features`, cargo-deny's `all-features = true` and the spellings
/// `squeezed` makes of them.
fn all_features_lines(text: &str) -> Vec<String> {
    logical_lines(text)
        .into_iter()
        .filter(|line| squeezed(line).contains("all-features"))
        .collect()
}

/// The spellings, in the form [`squeezed`] gives them, of the settings that let a release build
/// compile the seam. The refusal reads two things a release has and a test build does not: debug
/// assertions off and a panic that aborts. Turning the assertions on, making a panic unwind, or
/// overriding the release profile from the environment undoes one or the other, with the feature on
/// or without it. Each is the spelling of a flag (`-C debug-assertions`, `-C panic=unwind`), of a
/// profile setting (`debug-assertions = true`, `panic = "unwind"`) or of a variable
/// (`CARGO_PROFILE_RELEASE_*`), in a command, a workflow or a configuration file.
const BYPASSES_OF_THE_REFUSAL: [(&str, &str); 3] = [
    (
        "debug-assertions",
        "turns debug assertions on or states them",
    ),
    ("panic=unwind", "makes a panic unwind"),
    (
        "cargo-profile-release",
        "overrides a setting of the release profile from the environment",
    ),
];

/// Each of [`BYPASSES_OF_THE_REFUSAL`] that `text` spells, however it is split, quoted or capitalised.
fn bypasses_of_the_refusal(text: &str) -> Vec<&'static str> {
    let squeezed = squeezed(text);
    BYPASSES_OF_THE_REFUSAL
        .iter()
        .filter(|(spelling, _)| squeezed.contains(spelling))
        .map(|(spelling, _)| *spelling)
        .collect()
}

/// The profiles cargo has of its own. A profile of another name states what it inherits, and one that
/// inherits `dev` or `release` and is built with `--all-targets` or `--profile <name>` has the
/// settings of one under a name no refusal reads.
const BUILT_IN_PROFILES: [&str; 4] = ["dev", "release", "test", "bench"];

/// What `--profile` names in a file that is not cargo's, by the file and the word after it: the
/// install profile of `rustup`, and a profile of the AWS CLI in a message.
const NOT_A_CARGO_PROFILE: [(&str, &str); 3] = [
    ("Makefile", "minimal"),
    ("Makefile", "<name>"),
    ("docs/development/fuzzing.md", "minimal"),
];

/// Whether `path` is a file cargo reads its settings from: a manifest, or a configuration file.
fn is_a_cargo_file(path: &str) -> bool {
    let (directory, name) = path.rsplit_once('/').unwrap_or(("", path));
    name == "Cargo.toml" || (directory.ends_with(".cargo") && name.starts_with("config"))
}

/// The profiles a cargo file defines, by name: the second part of a `[profile.<name>]` table or of a
/// dotted key that starts `profile.`, and the first part of a key under `[profile]`. A `profile` table
/// written as one inline value cannot be read here, so it is named as what it is.
fn profile_tables(text: &str) -> Vec<String> {
    let unquoted = |part: &str| {
        part.trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string()
    };
    let mut found = Vec::new();
    // Before the first table header a key is at the root of the file, where `profile` is the table.
    let mut at_root = true;
    let mut under_profile = false;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            at_root = false;
            let parts: Vec<String> = line
                .trim_matches(|c| c == '[' || c == ']')
                .split('.')
                .map(unquoted)
                .collect();
            under_profile = parts == ["profile"];
            if parts.first().is_some_and(|first| first == "profile") && parts.len() > 1 {
                found.push(parts[1].clone());
            }
        } else if let Some(key) = line.split('=').next().map(unquoted) {
            if at_root && key == "profile" {
                found.push("an inline table".to_string());
            } else if let Some(rest) = key.strip_prefix("profile.").filter(|_| at_root) {
                found.push(unquoted(rest.split('.').next().unwrap_or_default()));
            } else if under_profile {
                found.push(unquoted(key.split('.').next().unwrap_or_default()));
            }
        }
    }
    found
}

/// The profiles `text`, the file at `path`, asks for or defines that cargo does not have of its own:
/// the word after `--profile`, the profile of a `--config profile.<name>`, the name in a
/// `CARGO_PROFILE_<NAME>_` variable and, in a file cargo reads, the profile a table defines. A
/// comment is not a command, so a line that starts `#` is not read for a flag.
fn custom_profiles(path: &str, text: &str) -> Vec<String> {
    let decoded = with_escapes_decoded(text);
    let commands: String = decoded
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let tokens: Vec<&str> = commands
        .split(|c: char| c.is_whitespace() || "\"'`,=()[]{};".contains(c))
        .filter(|token| !token.is_empty())
        .collect();
    let mut found = Vec::new();
    for (at, token) in tokens.iter().enumerate() {
        let next = tokens.get(at + 1).copied().unwrap_or_default();
        match *token {
            "--profile" => found.push(next.to_string()),
            "--config" => {
                if let Some(rest) = next.strip_prefix("profile.") {
                    found.push(rest.split('.').next().unwrap_or_default().to_string());
                }
            }
            _ => {}
        }
    }
    let squeezed = squeezed(text);
    for (at, _) in squeezed.match_indices("cargo-profile-") {
        let name = squeezed[at + "cargo-profile-".len()..]
            .split('-')
            .next()
            .unwrap_or_default();
        found.push(name.to_string());
    }
    if is_a_cargo_file(path) {
        found.extend(profile_tables(&decoded));
    }
    found.retain(|name| {
        !BUILT_IN_PROFILES.contains(&name.as_str())
            && !NOT_A_CARGO_PROFILE.contains(&(path, name.as_str()))
    });
    found
}

/// The commands that may ask for every feature, each by the file, the text that names it, how many
/// times that text occurs in the file, and why it ships nothing. A command added to a build or a
/// release is not in this list and is refused, so a new use is added here by name, with the reason.
const MAY_ASK_FOR_ALL_FEATURES: [(&str, &str, usize, &str); 13] = [
    (
        "Makefile",
        "cargo clippy --all-targets --all-features -- -D warnings",
        2,
        "a lint run",
    ),
    (
        "Makefile",
        "cargo clippy --target x86_64-pc-windows-gnu --all-targets --all-features --locked",
        1,
        "a lint run",
    ),
    (
        ".github/workflows/ci.yml",
        "cargo clippy --all-targets --all-features -- -D warnings",
        2,
        "a lint run",
    ),
    (
        ".github/workflows/ci.yml",
        "cargo clippy --target x86_64-pc-windows-gnu --all-targets --all-features --locked",
        1,
        "a lint run",
    ),
    (
        ".githooks/pre-commit",
        "cargo clippy --all-targets --all-features -- -D warnings",
        1,
        "a lint run",
    ),
    (
        "contrib/check-lint-increase.py",
        "\"cargo\", \"clippy\", \"--workspace\", \"--lib\", \"--bins\", \"--all-features\"",
        1,
        "a lint run",
    ),
    (
        "agents/pr_worktree.py",
        "\"cargo\", \"clippy\", *flags, \"--all-targets\", \"--all-features\"",
        1,
        "a lint run",
    ),
    (
        "agents/skills/maintenance/maintenance.py",
        "\"cargo\", \"clippy\", \"--workspace\", \"--lib\", \"--bins\", \"--all-features\"",
        1,
        "a lint run",
    ),
    (
        "agents/pr_worktree_selftest.py",
        "\"--all-targets\", \"--all-features\", \"--\", \"-D\", \"warnings\"",
        1,
        "the plan that script is checked against, which runs nothing",
    ),
    (
        "ui/docs/testing.md",
        "cargo clippy --all-targets --all-features -- -D warnings",
        2,
        "a lint run, described",
    ),
    (
        "ui/docs/development.md",
        "cargo clippy --all-targets --all-features -- -D warnings",
        1,
        "a lint run, described",
    ),
    (
        "ui/docs/v0.9.0-implementation.md",
        "cargo clippy --all-targets --all-features -- -D warnings",
        1,
        "a lint run, described",
    ),
    (
        "deny.toml",
        "all-features = true",
        1,
        "cargo-deny reads the dependency graph with every feature on and compiles nothing",
    ),
];

/// A release is made by `make` targets that run `cargo build --release` and `cargo zigbuild
/// --release`, and CHATGPT-2 lets no flag turn on a feature that names a host. A manifest cannot
/// say what a build command passes, so this reads every text file of the repository
/// ([`holds_a_command`]). None may name a seam at all, in a line, a list of arguments, a
/// variable or a line of its own, and `all-features`, which would enable each, is asked for only
/// by the runs [`MAY_ASK_FOR_ALL_FEATURES`] names, and by no spelling that hides it from the lines.
/// None turns debug assertions on, makes a panic unwind or overrides the release profile
/// ([`BYPASSES_OF_THE_REFUSAL`]), and none builds with a profile cargo does not have
/// ([`custom_profiles`]).
#[test]
fn no_build_or_release_command_passes_a_seam_or_all_features_but_the_lint_runs_by_name() {
    let texts = command_files();

    // The files that build a release, and each place a command is kept, are among those read.
    let paths: Vec<&str> = texts.iter().map(|(path, _)| path.as_str()).collect();
    for needed in [
        "Makefile",
        "Dockerfile.cross",
        "deny.toml",
        "install.sh",
        "install.ps1",
        "package.json",
        "README.md",
        ".github/workflows/ci.yml",
        ".githooks/pre-commit",
        "ui/scripts/build-bridge.sh",
        "ui/package.json",
        "ui/README.md",
        "mobile/package.json",
        "npm/scripts/stage-platform-package.js",
        "extension/package.json",
        "packages/agent-client/package.json",
        "packages/agent-client/README.md",
        "docs/development/releasing.md",
        "docs/website/package.json",
        "docs/website/docs/development.md",
        "contrib/check-lint-increase.py",
        ".envrc.example",
        "crates/i18n/locales/en-US.ftl",
        "ui/src/renderer/App.tsx",
    ] {
        assert!(paths.contains(&needed), "{needed} was not read: {paths:?}");
    }
    let has = |file: &str, command: &str| {
        texts
            .iter()
            .filter(|(path, _)| path == file)
            .any(|(_, text)| {
                logical_lines(text)
                    .iter()
                    .any(|line| line.contains(command))
            })
    };
    assert!(
        has("Makefile", "cargo build --release --locked")
            && has("Dockerfile.cross", "cargo zigbuild --release --locked"),
        "the commands that build a release were not found, so this test would pass by reading none"
    );

    let mut naming = Vec::new();
    for (path, text) in &texts {
        if names_a_test_seam(text) {
            naming.push(path.as_str());
        }
    }
    assert!(
        naming.is_empty(),
        "{naming:?} name a test seam, which no command a release runs does: a flag, a variable or a \
         list of arguments with it would put a way to name a host into whatever the command builds"
    );

    // Nothing turns debug assertions on, makes a panic unwind or overrides the release profile, any of
    // which would let a release build compile the seam, and nothing builds with a profile that cargo
    // does not have of its own, which can inherit the settings of `dev`.
    let mut bypassing = Vec::new();
    let mut profiles = Vec::new();
    for (path, text) in &texts {
        for spelling in bypasses_of_the_refusal(text) {
            bypassing.push((path.as_str(), spelling));
        }
        for name in custom_profiles(path, text) {
            profiles.push((path.as_str(), name));
        }
    }
    assert!(
        bypassing.is_empty(),
        "{bypassing:?} turn off what makes a release build refuse the test-hosts seam: {BYPASSES_OF_THE_REFUSAL:?}"
    );
    assert!(
        profiles.is_empty(),
        "{profiles:?} name a profile that cargo does not have of its own, which may inherit `dev` and \
         is built without the refusal that a release has"
    );

    let mut allowed = 0;
    for (file, needle, times, why) in MAY_ASK_FOR_ALL_FEATURES {
        allowed += times;
        let found = texts
            .iter()
            .filter(|(path, _)| path == file)
            .flat_map(|(_, text)| all_features_lines(text))
            .filter(|line| line.contains(needle))
            .count();
        assert_eq!(
            found, times,
            "{file}: `{needle}` ({why}) was expected {times} times and found {found}"
        );
    }
    let mut every: Vec<(String, usize)> = Vec::new();
    for (path, text) in &texts {
        let lines = all_features_lines(text).len();
        // The word where it is written over lines or in pieces is a use that the lines do not show.
        let hidden = squeezed(text).matches("all-features").count();
        assert_eq!(
            lines, hidden,
            "{path} asks for every feature in a form the lines of it do not show"
        );
        if lines > 0 {
            every.push((path.clone(), lines));
        }
    }
    assert_eq!(
        every.iter().map(|(_, lines)| lines).sum::<usize>(),
        allowed,
        "a command asks for every feature that is not named in MAY_ASK_FOR_ALL_FEATURES, and that \
         enables each test seam for every crate the build selects: {every:#?}"
    );
}

/// The command reader has to see each way a build can be given a seam, in a line, a list of
/// arguments, a variable, a line of its own and a word split over lines, and has to see a request
/// for every feature however it is spelled, or the test above passes on a command it cannot read.
/// It has to pass over a command that names neither.
#[test]
fn the_command_reader_sees_a_seam_and_every_feature_however_a_command_spells_them() {
    for (command, spelling) in [
        (
            "cargo build --release --features bravebot-skus/test-hosts",
            "a feature of a crate",
        ),
        (
            "cargo zigbuild --release --target x --features test-hosts",
            "a feature of the package",
        ),
        (
            "cargo build --release -p bravebot-config -F test-hosts",
            "the short flag",
        ),
        (
            "RUSTFLAGS='--cfg feature=\"test-hosts\"' cargo build --release",
            "a cfg of the feature",
        ),
        (
            "cargo build --release \\\n    -p bravebot-cli \\\n    --features=bravebot-skus/test-hosts",
            "a command over three lines",
        ),
        (
            "cargo\", \"build\", \"--features\", \"bravebot-skus/test-hosts\"",
            "a list of arguments",
        ),
        (
            "args:\n  - --features\n  - test-hosts\n",
            "a list of arguments in a workflow, the name on a line of its own",
        ),
        (
            "FEATURES=(\n  --features\n  \"test-hosts\"\n)\n",
            "a shell array, the name quoted on a line of its own",
        ),
        (
            "features = [\n  \"bravebot-skus/test-hosts\",\n]\n",
            "a list in a manifest",
        ),
        (
            "export CARGO_FEATURE_TEST_HOSTS=1",
            "the variable cargo sets for a feature",
        ),
        (
            "cargo build --features test_hosts",
            "the name written with an underscore",
        ),
        ("cargo build --features TEST-HOSTS", "the name in capitals"),
        (
            "{\"scripts\": {\"build\": \"cargo build --release --features test\\u002dhosts\"}}",
            "the name written with a JSON escape",
        ),
        (
            "cargo build --features test\\U0000002dhosts",
            "the name written with a long escape",
        ),
        (
            "cargo build --features test\\x2dhosts",
            "the name written with a byte escape",
        ),
        (
            "cargo build --features \\u0074est-hosts",
            "a letter of the name written with an escape",
        ),
        (
            "cargo build --features test-\\\nhosts",
            "the name split by a continuation",
        ),
        (
            "cargo build --features 'test-' 'hosts'",
            "the name quoted in two pieces",
        ),
    ] {
        assert!(
            names_a_test_seam(command),
            "{spelling} was not seen: {command}"
        );
    }
    for command in [
        "cargo build --release --locked",
        "cargo zigbuild --release --locked --target x86_64-unknown-linux-musl",
        "pnpm run test",
        // A backslash before a backslash is a backslash, so this is not an escape.
        "cargo build --features test\\\\u002dhosts",
    ] {
        assert!(!names_a_test_seam(command), "{command}");
        assert!(all_features_lines(command).is_empty(), "{command}");
    }
    for (command, spelling) in [
        (
            "cargo build --release --workspace --all-features",
            "the flag",
        ),
        (
            "cargo build --release \\\n  --workspace \\\n  --all-features",
            "the flag over three lines",
        ),
        ("[graph]\nall-features = true\n", "cargo-deny's setting"),
        ("args:\n  - --all-features\n", "a list of arguments"),
        ("cargo build --all_features", "an underscore"),
        ("cargo build --ALL-FEATURES", "capitals"),
    ] {
        assert!(
            !all_features_lines(command).is_empty(),
            "{spelling} was not seen: {command}"
        );
    }
    let (joined, _) = (
        all_features_lines("cargo build --release \\\n  --workspace \\\n  --all-features"),
        (),
    );
    assert_eq!(joined, ["cargo build --release --workspace --all-features"]);
    // A word split by a line continuation is one logical line that holds it once, so the lines show
    // it as often as the whole text does, which is what the test above compares.
    let split = "cargo build --all-\\\nfeatures";
    assert_eq!(squeezed(split).matches("all-features").count(), 1);
    assert_eq!(all_features_lines(split).len(), 1);
}

/// The reader of what undoes the refusal and of the profiles cargo does not have has to see each way
/// a command, a workflow or a configuration file spells them, and has to pass over what is not one.
/// Each is the verifier's evasion of a guard that read only the feature's name.
#[test]
fn the_command_reader_sees_what_undoes_the_refusal_and_a_profile_of_its_own() {
    for (text, spelling) in [
        (
            "RUSTFLAGS=\"-C panic=unwind\" cargo build --release",
            "RUSTFLAGS with an unwinding panic",
        ),
        (
            "RUSTFLAGS='-Cdebug-assertions=on' cargo build --release",
            "RUSTFLAGS with the short form of the flag",
        ),
        (
            "env:\n  CARGO_ENCODED_RUSTFLAGS: \"-Cdebug-assertions\"",
            "CARGO_ENCODED_RUSTFLAGS in a workflow",
        ),
        (
            "export CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true",
            "the release profile's variable",
        ),
        (
            "export CARGO_PROFILE_RELEASE_PANIC=unwind",
            "the release profile's panic",
        ),
        (
            "cargo build --release --config 'profile.release.panic=\"unwind\"'",
            "a --config of the release profile",
        ),
        (
            "[profile.release]\npanic = \"unwind\"\n",
            "a table of the release profile",
        ),
        (
            "[build]\nrustflags = [\"-C\", \"debug-assertions=on\"]\n",
            "a configuration file's rustflags",
        ),
        (
            "cargo build --release \\\n  -C debug\\\n-assertions",
            "a word split over lines",
        ),
        (
            "RUSTFLAGS=-C DEBUG_ASSERTIONS=1",
            "capitals and an underscore",
        ),
    ] {
        assert!(
            !bypasses_of_the_refusal(text).is_empty(),
            "{spelling} was not seen: {text}"
        );
    }
    for text in [
        "cargo build --release --locked",
        "RUSTFLAGS=\"-D warnings\" cargo build --release",
        "[profile.release]\npanic = \"abort\"\nlto = true\n",
        "env:\n  CARGO_PROFILE_DEV_DEBUG: line-tables-only",
    ] {
        assert!(bypasses_of_the_refusal(text).is_empty(), "{text}");
    }

    let profiles = |path: &str, text: &str| custom_profiles(path, text);
    for (path, text, spelling) in [
        (
            "build.sh",
            "cargo build --profile shipping --all-targets",
            "--profile",
        ),
        ("build.sh", "cargo build --profile=shipping", "--profile="),
        (
            "ci.yml",
            "args:\n  - cargo\n  - build\n  - --profile\n  - shipping\n",
            "a list of arguments",
        ),
        (
            "build.sh",
            "cargo build --config 'profile.shipping.inherits=\"dev\"'",
            "--config",
        ),
        (
            "build.sh",
            "export CARGO_PROFILE_SHIPPING_INHERITS=dev",
            "a variable",
        ),
        (
            "crates/x/Cargo.toml",
            "[profile.shipping]\ninherits = \"dev\"\n",
            "a table",
        ),
        (
            ".cargo/config.toml",
            "[profile.shipping.package.\"*\"]\nopt-level = 3\n",
            "a table of a package of a profile",
        ),
        (
            "Cargo.toml",
            "[profile]\nshipping.inherits = \"dev\"\n",
            "a key under [profile]",
        ),
        (
            "Cargo.toml",
            "profile.shipping.inherits = \"dev\"\n",
            "a dotted key",
        ),
        (
            "Cargo.toml",
            "profile = { shipping = { inherits = \"dev\" } }\n",
            "an inline table",
        ),
        (
            "other.sh",
            "rustup toolchain install stable --profile minimal",
            "a profile of another tool, in a file that does not name it",
        ),
    ] {
        assert!(
            !profiles(path, text).is_empty(),
            "{spelling} was not seen: {text}"
        );
    }
    for (path, text) in [
        ("build.sh", "cargo build --release --locked"),
        ("build.sh", "cargo build --profile release"),
        ("build.sh", "cargo test --profile dev"),
        ("build.sh", "# cargo build --profile shipping"),
        (
            "ci.yml",
            "env:\n  CARGO_PROFILE_DEV_DEBUG: line-tables-only",
        ),
        ("Cargo.toml", "[profile.release]\npanic = \"abort\"\n"),
        ("Cargo.toml", "[profile.dev.package.\"*\"]\nopt-level = 2\n"),
        ("Cargo.toml", "[dependencies]\nprofile = \"1\"\n"),
        (
            "Makefile",
            "rustup toolchain install nightly --profile minimal",
        ),
        (
            "crates/a/src/profile.json",
            "{\"profile.json\": 1, \"user.profile.name\": 2}",
        ),
    ] {
        // A key named `profile` under `[dependencies]` is not a profile table.
        let found = profiles(path, text);
        assert!(found.is_empty(), "{path}: {text}: {found:?}");
    }
}

/// CHATGPT-2 says what the guards read, and the lists they read are the lists the clause names:
/// every directory the walk leaves out, every kind of file it leaves out, the spellings it refuses
/// and the profiles it allows.
#[test]
fn the_clause_names_what_the_command_guard_reads_and_leaves_out() {
    let spec = std::fs::read_to_string(workspace().join("docs/specs/chatgpt-sign-in.md"))
        .expect("the spec");
    let body = clause(&spec, "CHATGPT-2");
    let mut missing = Vec::new();
    for name in NOT_WALKED
        .iter()
        .chain(NOT_WALKED_FOR_COMMANDS.iter())
        .chain(LOCK_FILES.iter())
        .chain(BUILT_IN_PROFILES.iter())
        .chain([SPECS, "Cargo.toml", ".rs"].iter())
    {
        if !body.contains(&format!("`{name}`")) {
            missing.push((*name).to_string());
        }
    }
    for (spelling, _) in BYPASSES_OF_THE_REFUSAL {
        // The spelling is in the form `squeezed` gives it, and the clause names the flag or the
        // setting, so what is looked for is the part of it a person would write.
        let written = spelling.replace("cargo-profile-release", "CARGO_PROFILE_RELEASE");
        if !body.contains(&written) {
            missing.push(written);
        }
    }
    for (file, word) in NOT_A_CARGO_PROFILE {
        if !body.contains(&format!("`{file}`")) || !body.contains(&format!("`{word}`")) {
            missing.push(format!("{file}: {word}"));
        }
    }
    assert!(
        missing.is_empty(),
        "CHATGPT-2 does not name what the guard reads or leaves out: {missing:?}"
    );
    // And what the clause allows `--all-features` to is what the test allows.
    for (file, _, _, _) in MAY_ASK_FOR_ALL_FEATURES {
        assert!(
            body.contains(&format!("`{file}`")),
            "CHATGPT-2 does not name `{file}`, which may ask for every feature"
        );
    }
}

/// The walk for commands reads a file whatever it is called, which the verifier showed it did not:
/// a script that ends in `.bash`, a `Justfile`, a `Makefile.local`. It leaves out the kinds of file
/// the clause names and no other.
#[test]
fn the_command_walk_reads_a_file_whatever_it_is_called_and_leaves_out_what_the_clause_names() {
    let at = |path: &str| workspace().join(path);
    for read in [
        "build.bash",
        "Justfile",
        "Makefile.local",
        "scripts/release.zsh",
        "deploy",
        "ci/build.cmd",
        "build.sh.in",
        "block.sh",
        "clock.py",
        "build.lock",
        "tools/Taskfile.yml",
        "fuzz/run",
    ] {
        assert!(
            holds_a_command(&at(read)),
            "{read} is a file the walk does not read"
        );
    }
    for left_out in [
        "crates/x/src/lib.rs",
        "crates/x/Cargo.toml",
        "Cargo.lock",
        "fuzz/Cargo.lock",
        "ui/pnpm-lock.yaml",
        "docs/specs/chatgpt-sign-in.md",
        "docs/specs/tools/run.md",
    ] {
        assert!(
            !holds_a_command(&at(left_out)),
            "{left_out} is a file the walk reads"
        );
    }
    // And what the walk found is most of the repository's text: the files of every kind a command
    // could be kept in, which the lists it replaced did not name.
    let paths: Vec<String> = command_files().into_iter().map(|(path, _)| path).collect();
    assert!(
        paths.len() > 600,
        "{} files were read, so this walk would pass by reading almost nothing",
        paths.len()
    );
    for extension in ["css", "svg", "html", "ftl", "txt", "properties", "xml"] {
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with(&format!(".{extension}"))),
            "no .{extension} file was read"
        );
    }
    assert!(
        !paths.iter().any(|path| path.ends_with(".rs")),
        "a Rust source was read"
    );
}

/// Where `needle` appears in the source a release build compiles, by file relative to the
/// workspace and how many times, for every file under a crate's `src`.
fn release_callers(needle: &str) -> Vec<(String, usize)> {
    let mut found: Vec<(String, usize)> = release_sources()
        .into_iter()
        .filter_map(|(file, code)| {
            let count = code.matches(needle).count();
            (count > 0).then(|| (relative(&file), count))
        })
        .collect();
    found.sort();
    found
}

/// The text of each argument of each call that starts at `needle` in `code`, split at the commas
/// that are not inside a bracket.
fn call_arguments(code: &str, needle: &str) -> Vec<Vec<String>> {
    let mut calls = Vec::new();
    for (at, _) in code.match_indices(needle) {
        let mut depth = 1;
        let mut arguments = vec![String::new()];
        for character in code[at + needle.len()..].chars() {
            match character {
                '(' | '[' | '{' | '<' => depth += 1,
                ')' | ']' | '}' | '>' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                break;
            }
            match (character, depth) {
                (',', 1) => arguments.push(String::new()),
                (other, _) => arguments.last_mut().expect("an argument").push(other),
            }
        }
        calls.push(
            arguments
                .into_iter()
                .map(|argument| argument.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|argument| !argument.is_empty())
                .collect(),
        );
    }
    calls
}

/// [`call_arguments`] over the source a release build compiles.
fn release_call_arguments(needle: &str) -> Vec<Vec<String>> {
    release_sources()
        .into_iter()
        .flat_map(|(_, code)| call_arguments(&code, needle))
        .collect()
}

/// The functions in `code` that take `word` as the type of a parameter, by name: the name after
/// `fn` whose parenthesised parameter list holds `word` as a whole identifier.
fn functions_taking(code: &str, word: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, _) in code.match_indices("fn ") {
        let before = code[..at].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = &code[at + "fn ".len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let Some(open) = rest.find('(') else {
            continue;
        };
        let mut depth = 0;
        let mut parameters = String::new();
        for character in rest[open..].chars() {
            match character {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            parameters.push(character);
            if depth == 0 {
                break;
            }
        }
        let holds = parameters
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|identifier| identifier == word);
        if holds && !name.is_empty() {
            found.push(name);
        }
    }
    found
}

/// CHATGPT-2 says a release build sends the account's tokens only to the hosts compiled in. What
/// makes that so is who may name a host, and that is pinned here as the callers of what can. `Hosts`
/// is the sign-in's destinations: the compiled-in ones are made by one constructor, a stand-in's by
/// `Hosts::at`, which exists only with the seam, and the functions that take a `Hosts` are those of
/// the sign-in's own module. The account's serving host is the base address of a `Provider`, which
/// `Provider::chatgpt_account` builds from a constant, and `ResponsesClient::for_account` and
/// `list_from_account` send the access token to whatever provider they are given, so each has one
/// caller, and the argument at that call is the account's provider, which one function of the agent
/// holds. A second caller of any of these, or a function that takes a `Hosts` that is not named
/// here, is a second place that can say where a token goes, and this fails until the clause says why.
#[test]
fn only_the_agent_names_the_host_the_account_is_sent_to_and_no_release_calls_the_seam() {
    let chatgpt_agent = "crates/agent/src/chatgpt.rs".to_string();
    let backend = "crates/agent/src/backend.rs".to_string();
    let chatgpt_command = "crates/cli/src/chatgpt.rs".to_string();
    // What is called, and the files and counts allowed to call it.
    let allowed: [(&str, Vec<(String, usize)>); 8] = [
        // The constructor that is not a constant exists with the seam, and nothing a release
        // compiles calls it.
        ("Hosts::at(", vec![]),
        // The compiled-in hosts are asked for by the renewer's source and by the command.
        (
            "Hosts::compiled_in",
            vec![(chatgpt_agent.clone(), 1), (chatgpt_command.clone(), 1)],
        ),
        // The one renewer of the process.
        ("Renewer::new(", vec![(chatgpt_agent.clone(), 1)]),
        // The account's own client and its listing, the two calls that send the access token.
        ("ResponsesClient::for_account(", vec![(backend.clone(), 1)]),
        ("list_from_account(", vec![(chatgpt_agent.clone(), 1)]),
        ("account_models_from(", vec![(chatgpt_agent.clone(), 1)]),
        // The account's provider is held by one function, which the agent calls where it needs the
        // account's address: the definition, the roster, the client and the address of the
        // listing's route.
        (
            "account_provider()",
            vec![(backend.clone(), 2), (chatgpt_agent.clone(), 2)],
        ),
        // Where the provider is built: the agent's one holder, and no front end.
        (
            "Provider::chatgpt_account",
            vec![(chatgpt_agent.clone(), 1)],
        ),
    ];
    for (needle, expected) in allowed {
        assert_eq!(
            release_callers(needle),
            expected,
            "`{needle}` has a caller that CHATGPT-2 does not name. A release build sends the \
             account's tokens to the hosts it names, so a new place that names or reads one is a \
             change to that clause"
        );
    }

    // The provider each of the two calls that send the token is given is the account's.
    let clients = release_call_arguments("ResponsesClient::for_account(");
    assert_eq!(clients.len(), 1);
    assert_eq!(
        clients[0].get(1).map(String::as_str),
        Some("crate::chatgpt::account_provider()"),
        "{clients:?}"
    );
    let listings = release_call_arguments("list_from_account(");
    assert_eq!(listings.len(), 1);
    assert_eq!(listings[0].get(1).map(String::as_str), Some("provider"));
    let rosters = release_call_arguments("account_models_from(");
    assert_eq!(rosters.len(), 1);
    assert_eq!(
        rosters[0].get(2).map(String::as_str),
        Some("account_provider()"),
        "the roster of the account is asked of a provider that is not the account's: {rosters:?}"
    );

    // The functions that take the sign-in's destinations are the sign-in's own.
    let mut taking: Vec<(String, String)> = release_sources()
        .into_iter()
        .filter(|(file, code)| {
            relative(file).starts_with("crates/skus/src/chatgpt/")
                || code.contains("bravebot_skus::chatgpt")
        })
        .flat_map(|(file, code)| {
            let file = relative(&file);
            functions_taking(&code, "Hosts")
                .into_iter()
                .map(move |name| (file.clone(), name))
        })
        .collect();
    taking.sort();
    let skus =
        |file: &str, name: &str| (format!("crates/skus/src/chatgpt/{file}"), name.to_string());
    assert_eq!(
        taking,
        [
            skus("authorize.rs", "url"),
            skus("exchange.rs", "exchange"),
            skus("exchange.rs", "refresh"),
            skus("id_token.rs", "key_set"),
            skus("id_token.rs", "verify"),
            skus("mod.rs", "run"),
            skus("mod.rs", "sign_in"),
            skus("renew.rs", "at"),
            skus("renew.rs", "new"),
        ],
        "a function takes the sign-in's destinations that CHATGPT-2 does not name"
    );
}

/// The scan that pins the callers has to see a call in code and has to pass over one in a test
/// module or a comment, or the test above passes on a source it cannot read.
#[test]
fn the_caller_scan_reads_code_and_leaves_out_tests_and_comments() {
    let source = "\
// Hosts::at(x) is only said here
fn live() { Hosts::at(base); }
#[cfg(test)]
mod tests {
    fn in_a_test() { Hosts::at(base); Hosts::at(base); }
}
fn after() { for_account(h); }
";
    let code = production_code(source);
    assert_eq!(code.matches("Hosts::at(").count(), 1, "{code}");
    assert_eq!(code.matches("for_account(").count(), 1, "{code}");
    assert!(!code.contains("only said here"), "{code}");

    // A module declared without a body is the declaration alone. The code after it is read, which
    // the scan did not do when it took the declaration for the start of a body.
    for declaration in [
        "#[cfg(test)]\nmod tests;",
        "#[cfg(test)]\npub(crate) mod tests;",
        "#[cfg(test)]\n#[path = \"tests_of_it.rs\"]\nmod tests;",
        "#[cfg(test)]\nmod tests {}",
        "#[cfg(all(test, unix))]\n#[path = \"tests_of_it.rs\"]\nmod tests;",
    ] {
        let code = production_code(&format!(
            "{declaration}\nfn live() {{ Hosts::at(base); }}\n"
        ));
        assert_eq!(
            code.matches("Hosts::at(").count(),
            1,
            "{declaration}: {code}"
        );
        assert!(!code.contains("mod tests"), "{declaration}: {code}");
    }
    // A gate on something that is not a module leaves the item and its attributes in.
    let code = production_code(
        "#[cfg(test)]\n#[allow(dead_code)]\nfn only_a_test() { Hosts::at(base); }\n",
    );
    assert_eq!(code.matches("Hosts::at(").count(), 1, "{code}");
}

/// The reader of a call's arguments and of the functions that take a type splits where Rust does,
/// so a nested call or a generic is not taken for two arguments or a parameter.
#[test]
fn the_call_and_signature_readers_split_where_rust_splits() {
    let code = "\
fn make() {
    ResponsesClient::for_account(egress, crate::chatgpt::account_provider(), model.into(), sign_in, Map::<A, B>::new());
}
";
    let calls = call_arguments(code, "ResponsesClient::for_account(");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].len(), 5, "{calls:?}");
    assert_eq!(calls[0][1], "crate::chatgpt::account_provider()");

    let signatures = "\
pub fn sign_in(hosts: &Hosts, transport: T) {}
fn run<S>(a: S, hosts: Hosts) -> u8 {}
fn hosts_of(a: &HostsOfSomethingElse) {}
pub(super) fn quiet(a: u8) {}
fn nested(callback: impl Fn(&Hosts)) {}
";
    assert_eq!(
        functions_taking(signatures, "Hosts"),
        ["sign_in", "run", "nested"]
    );
}

/// A file only a test module declares is a test, and the scan of what a release compiles has to
/// leave it out or a test that names the seam is taken for a caller of it.
#[test]
fn a_file_only_a_test_module_declares_is_not_a_release_source() {
    let source = "\
mod live;
#[cfg(test)]
mod plain;
#[cfg(test)]
#[path = \"elsewhere_tests.rs\"]
pub(crate) mod renamed;
    #[cfg(test)]
    mod nested;
#[cfg(test)]
mod inline {
}
#[cfg(all(test, unix))]
mod unix_only;
#[cfg(not(test))]
mod other;
";
    assert_eq!(
        declared_test_modules(source),
        [
            (None, "plain".to_string()),
            (
                Some("elsewhere_tests.rs".to_string()),
                "renamed".to_string()
            ),
            (None, "nested".to_string()),
            (None, "unix_only".to_string()),
        ]
    );
    let at = std::path::Path::new("crates/x/src");
    assert_eq!(
        module_files(&at.join("lib.rs"), None, "plain"),
        [at.join("plain.rs"), at.join("plain/mod.rs")]
    );
    assert_eq!(
        module_files(&at.join("app.rs"), None, "plain"),
        [at.join("app/plain.rs"), at.join("app/plain/mod.rs")]
    );
    assert_eq!(
        module_files(&at.join("app.rs"), Some("elsewhere_tests.rs"), "renamed"),
        [at.join("elsewhere_tests.rs")]
    );

    let files: Vec<String> = release_sources()
        .iter()
        .map(|(file, _)| relative(file))
        .collect();
    // A file a test module declares, which this test reads from the tree: any that exists is left
    // out, and one that is declared by a test module and found among the sources is a fault.
    for (file, text) in std::fs::read_dir(workspace().join("crates"))
        .expect("the crates directory")
        .flatten()
        .map(|krate| krate.path().join("src"))
        .filter(|source| source.is_dir())
        .flat_map(|source| rust_files(&source))
        .map(|file| {
            let text = std::fs::read_to_string(&file).expect("a source file");
            (file, text)
        })
    {
        for (path, name) in declared_test_modules(&text) {
            for candidate in module_files(&file, path.as_deref(), &name) {
                if candidate.is_file() {
                    assert!(
                        !files.contains(&relative(&candidate)),
                        "{} is declared by a test module and was taken for a source a release \
                         compiles",
                        relative(&candidate)
                    );
                }
            }
        }
    }
    for release in ["crates/cli/src/chatgpt.rs", "crates/agent/src/chatgpt.rs"] {
        assert!(
            files.iter().any(|file| file == release),
            "{release} was left out, so the scan reads less than a release compiles"
        );
    }
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
