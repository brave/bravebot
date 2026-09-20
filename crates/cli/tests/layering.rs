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

/// The first double-quoted string in a fragment of TOML.
fn quoted(value: &str) -> Option<&str> {
    let (_, rest) = value.split_once('"')?;
    let (inner, _) = rest.split_once('"')?;
    Some(inner)
}

/// The crate an inline dependency table renames itself from, if it renames itself.
fn renamed_from(value: &str) -> Option<&str> {
    let (_, rest) = value.split_once("package")?;
    let (_, rest) = rest.split_once('=')?;
    quoted(rest)
}

/// Keep a name if it is one of this workspace's crates.
fn note(found: &mut BTreeSet<String>, name: &str) {
    if name.starts_with(PREFIX) {
        found.insert(name.to_string());
    }
}

/// The crates in this workspace a member depends on, whatever section of its manifest asks for
/// them and however that section spells the request.
///
/// A dependency is an edge whether it was declared for the build, the tests, the build script or
/// one platform: what LAYER-1 grants is the reach of the crate, and a test that links the
/// terminal interface reaches it. The spellings matter for the same reason. Cargo takes a
/// dependency as one line in a table, as a table of its own, and under a name of the caller's
/// choosing with the real crate in a `package` key, so a reader that saw only the first of those
/// would be satisfied by a manifest that had merely been reformatted.
fn workspace_dependencies(manifest: &str) -> BTreeSet<String> {
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
                note(&mut found, &name);
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
        note(&mut found, name);
    }
    if let Some(name) = entry.take() {
        note(&mut found, &name);
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
