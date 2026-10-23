//! The documentation published with this program, carried inside it.
//!
//! An installed session cannot read the repository's `docs/website/docs`, so the pages are compiled
//! in and a question about using bravebot is answered from the version that is running. The text is
//! this program's own, written into the binary like the loop skill's, which is why it is trusted by
//! provenance and goes through no gate (SKILL-12, SKILL-17).
//!
//! The clause headings of `docs/specs/` are carried too, one page per spec, found through a page that
//! lists them. `build.rs` reads them out of the specs when the program is built.
//!
//! The planner selects a page from a fixed table by an exact match of its name. The name is never a
//! path and nothing here reads a file at run time (SKILL-8).

use std::sync::OnceLock;

/// The skill whose body is the index and whose pages are loaded one at a time.
pub const SKILL: &str = "bravebot-help";

/// One published page: its place under the documentation, and its text with the frontmatter.
struct Page {
    id: &'static str,
    text: &'static str,
}

macro_rules! page {
    ($id:literal) => {
        Page {
            id: $id,
            text: include_str!(concat!("../../../docs/website/docs/", $id, ".md")),
        }
    };
}

/// Every page, in path order. A test holds this to the files under `docs/website/docs`.
const PAGES: [Page; 46] = [
    page!("customize/agents"),
    page!("customize/configuration"),
    page!("customize/effort"),
    page!("customize/hooks"),
    page!("customize/instructions"),
    page!("customize/language"),
    page!("customize/managed-settings"),
    page!("customize/mcp-servers"),
    page!("customize/mcp/calendar"),
    page!("customize/mcp/github"),
    page!("customize/mcp/gmail"),
    page!("customize/model"),
    page!("customize/premium"),
    page!("customize/providers/anthropic"),
    page!("customize/providers/bedrock"),
    page!("customize/providers/openai-compatible"),
    page!("customize/signing-in"),
    page!("customize/skills"),
    page!("customize/theme"),
    page!("development"),
    page!("how-it-works"),
    page!("intro"),
    page!("quickstart"),
    page!("reference/cli"),
    page!("reference/commands"),
    page!("reference/run-tool"),
    page!("reference/tools"),
    page!("security/audit-trail"),
    page!("security/permissions"),
    page!("security/sandbox"),
    page!("security/security"),
    page!("security/trust"),
    page!("security/vetting"),
    page!("troubleshooting"),
    page!("using/context"),
    page!("using/editors"),
    page!("using/headless"),
    page!("using/incognito"),
    page!("using/interactive-mode"),
    page!("using/manifest-mode"),
    page!("using/rewind"),
    page!("using/sessions"),
    page!("using/shell-mode"),
    page!("using/transcript"),
    page!("using/vi-mode"),
    page!("using/watches"),
];

/// The pages `build.rs` wrote from `docs/specs`: the list of specs, and one page of clause headings
/// for each.
mod specs {
    include!(concat!(env!("OUT_DIR"), "/spec_headings.rs"));
}

/// The id of the page that lists the specs. No published page is named this.
const SPEC_LIST: &str = "specs";

/// The name the planner loads a page by. `page()` strips exactly this prefix and matches what is
/// left against a page's id, so the two must agree.
fn name_of(id: &str) -> String {
    format!("{SKILL}/{id}")
}

/// The text of the page of this name, without its frontmatter, or `None` for any other name.
///
/// An exact match against the table. A name that is a traversal, a path, or one character out
/// matches nothing.
pub fn page(name: &str) -> Option<&'static str> {
    let id = name.strip_prefix(SKILL)?.strip_prefix('/')?;
    if id == SPEC_LIST {
        return Some(specs::LIST);
    }
    if let Some(spec) = specs::SPECS.iter().find(|(spec_id, _)| *spec_id == id) {
        return Some(spec.1);
    }
    PAGES
        .iter()
        .find(|page| page.id == id)
        .map(|page| crate::skills::body_after_frontmatter(page.text))
}

/// What the skill tells the planner, then one line per page (its name, title and description), then
/// the line for the page that lists the specs.
pub fn body() -> &'static str {
    static BODY: OnceLock<String> = OnceLock::new();
    BODY.get_or_init(|| {
        let mut out = String::from(INSTRUCTIONS);
        for page in &PAGES {
            let declared = crate::skills::declarations(page.text).unwrap_or_default();
            let said = |key: &str| declared.get(key).map(String::as_str).unwrap_or("");
            out.push_str(&format!(
                "- {}: {}. {}\n",
                name_of(page.id),
                said("title"),
                said("description")
            ));
        }
        out.push_str(&format!("- {}: {SPEC_LIST_SUMMARY}\n", name_of(SPEC_LIST)));
        out
    })
}

/// The index line of the page that lists the specs, which has no frontmatter to take one from.
const SPEC_LIST_SUMMARY: &str = "The specifications. Lists every spec of this version, each with a \
page of its clause headings. Load it when the pages above do not settle exactly what bravebot \
does, then load the page of the spec that fits.";

/// The part of the body that is not the index.
const INSTRUCTIONS: &str = "\
These are the pages of the documentation of the version of bravebot that is running. Answer from \
them and not from memory: a command, key or setting that no page names may not exist in this \
version.

Pick the one or two pages whose title and description fit the question and load each with \
load_skill, using the name at the start of its line. Do not load every page: a question costs the \
pages it needs. If the pages you loaded do not answer it, say so rather than guessing.

Pages:
";

#[cfg(test)]
mod tests {
    use super::*;

    fn on_disk() -> Vec<String> {
        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).expect("the documentation directory") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else if path.extension().is_some_and(|ext| ext == "md") {
                    let relative = path.strip_prefix(root).expect("under the root");
                    out.push(
                        relative
                            .with_extension("")
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/website/docs");
        let mut found = Vec::new();
        walk(&root, &root, &mut found);
        found.sort();
        found
    }

    /// A page published on the website but missing from the binary is a page the planner cannot
    /// load, so a question about it is answered from memory.
    #[test]
    fn every_published_page_is_carried_in_the_binary() {
        let carried: Vec<&str> = PAGES.iter().map(|page| page.id).collect();
        assert_eq!(carried, on_disk());
    }

    /// A near-miss or traversal name that loaded a page would let the planner read a file the
    /// table does not name.
    #[test]
    fn a_page_is_found_by_its_exact_name_and_nothing_else() {
        let text = page("bravebot-help/using/watches").expect("the watches page");
        assert!(text.starts_with("# Watching a file"), "{text}");
        for wrong in [
            "using/watches",
            "bravebot-help",
            "bravebot-help/",
            "bravebot-help/using/watche",
            "bravebot-help/../Cargo",
            "bravebot-help//using/watches",
            "bravebot-help/using/watches.md",
        ] {
            assert!(page(wrong).is_none(), "{wrong} named a page");
        }
    }

    /// The spec files on disk that carry a clause prefix, with the ids of their clause anchors in
    /// order. An anchor is written above every clause heading, so it is counted apart from the
    /// headings the build script reads.
    fn specs_on_disk() -> Vec<(String, String, Vec<String>)> {
        fn walk(dir: &std::path::Path, below: &str, out: &mut Vec<(String, String, Vec<String>)>) {
            for entry in std::fs::read_dir(dir).expect("the specs directory") {
                let path = entry.expect("an entry").path();
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                if path.is_dir() {
                    walk(&path, &format!("{below}{stem}/"), out);
                } else if path.extension().is_some_and(|ext| ext == "md") {
                    let text = std::fs::read_to_string(&path).expect("a spec");
                    if path.file_name().is_some_and(|name| name == "README.md") {
                        continue;
                    }
                    let prefix = text
                        .lines()
                        .take(8)
                        .find_map(|line| line.strip_prefix("id: "))
                        .unwrap_or_else(|| panic!("{} has no id", path.display()));
                    let anchors = text
                        .lines()
                        .filter_map(|line| {
                            line.strip_prefix("<a id=\"")?
                                .strip_suffix("\"></a>")
                                .map(str::to_string)
                        })
                        .collect();
                    out.push((format!("{below}{stem}"), prefix.to_string(), anchors));
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs");
        let mut found = Vec::new();
        walk(&root, "", &mut found);
        found.sort();
        found
    }

    /// A spec missing from the binary is one whose clauses the planner cannot be given, and a
    /// clause missing from its page is a rule it answers as if there were none. Each page has to
    /// hold every clause of its file, in order.
    #[test]
    fn every_spec_is_carried_with_the_clause_headings_its_file_holds() {
        let on_disk = specs_on_disk();
        assert!(on_disk.len() > 60, "{} specs found", on_disk.len());
        let carried: Vec<&str> = specs::SPECS.iter().map(|(id, _)| *id).collect();
        let expected: Vec<String> = on_disk
            .iter()
            .map(|(path, _, _)| format!("specs/{path}"))
            .collect();
        assert_eq!(carried, expected);

        for ((path, prefix, anchors), (_, text)) in on_disk.iter().zip(specs::SPECS) {
            let headings: Vec<&str> = text
                .lines()
                .filter(|line| line.starts_with(&format!("{prefix}-")))
                .map(|line| line.split(':').next().unwrap())
                .collect();
            assert_eq!(&headings, anchors, "{path}");
        }
    }

    /// A heading carries the rule itself, so a page that listed ids alone would leave the planner
    /// to guess what each clause says.
    #[test]
    fn a_spec_page_holds_the_heading_of_a_clause_and_names_its_file() {
        let text = page("bravebot-help/specs/commands").expect("the commands spec");
        assert!(
            text.contains("\nCMD-24: `/help` asks the planner to answer a question from the"),
            "{text}"
        );
        assert!(text.contains("docs/specs/commands.md"), "{text}");
        assert!(
            !text.contains("\n### "),
            "a markdown heading mark was carried"
        );
        let nested = page("bravebot-help/specs/tools/read-file").expect("a spec in a directory");
        assert!(nested.contains("docs/specs/tools/read-file.md"), "{nested}");
    }

    /// The same table-only rule as for a published page: a near-miss, a directory, the README that
    /// is no spec, or a traversal name must not reach a spec page or a file.
    #[test]
    fn a_spec_page_is_found_by_its_exact_name_and_nothing_else() {
        for wrong in [
            "bravebot-help/specs/",
            "bravebot-help/specs/command",
            "bravebot-help/specs/commands.md",
            "bravebot-help/specs/tools",
            "bravebot-help/specs/README",
            "bravebot-help/specs/../commands",
            "bravebot-help/specs//commands",
            "bravebot-help/spec/commands",
            "specs/commands",
        ] {
            assert!(page(wrong).is_none(), "{wrong} named a page");
        }
    }

    /// A spec the list leaves out cannot be found, and a name on the list that loads nothing sends
    /// the planner away with no answer. The list also has to use the skill's own name, which the
    /// build script holds separately.
    #[test]
    fn the_list_of_specs_names_a_page_that_loads_for_each_spec() {
        let list = page("bravebot-help/specs").expect("the list of specs");
        let named: Vec<&str> = list
            .lines()
            .filter_map(|line| line.strip_prefix("- "))
            .map(|line| line.split(": ").next().unwrap())
            .collect();
        assert_eq!(named.len(), specs::SPECS.len());
        for name in named {
            assert!(
                name.starts_with(&format!("{SKILL}/specs/")),
                "{name} is not named under the skill"
            );
            assert!(page(name).is_some(), "{name} loads nothing");
        }
        assert!(
            list.contains("- bravebot-help/specs/commands: Slash commands (CMD, "),
            "{list}"
        );
    }

    /// The index is all the planner is told before it loads a page, so the list of specs has to be
    /// in it, and the headings themselves must not be, or the first load costs every spec.
    #[test]
    fn the_index_names_the_list_of_specs_and_carries_none_of_their_headings() {
        let index = body();
        assert!(
            index.contains("- bravebot-help/specs: The specifications."),
            "{index}"
        );
        assert!(
            !index.contains("CMD-24"),
            "a clause heading is in the index"
        );
    }

    /// A page left out of the index is one the planner is never told to load.
    #[test]
    fn the_index_names_every_page_with_its_title() {
        let index = body();
        for page in &PAGES {
            let declared = crate::skills::declarations(page.text).expect("frontmatter");
            let title = declared.get("title").expect("a title");
            assert!(
                index.contains(&format!("- {}: {title}.", name_of(page.id))),
                "{} is not in the index",
                page.id
            );
        }
    }
}
