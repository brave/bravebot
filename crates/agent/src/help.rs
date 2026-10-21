//! The documentation published with this program, carried inside it.
//!
//! An installed session cannot read the repository's `docs/website/docs`, so the pages are compiled
//! in and a question about using bravebot is answered from the version that is running. The text is
//! this program's own, written into the binary like the loop skill's, which is why it is trusted by
//! provenance and goes through no gate (SKILL-12, SKILL-17).
//!
//! The planner selects a page from the fixed table below by an exact match of its name. The name is
//! never a path and nothing here reads a file at run time (SKILL-8).

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
const PAGES: [Page; 37] = [
    page!("customize/agents"),
    page!("customize/configuration"),
    page!("customize/hooks"),
    page!("customize/instructions"),
    page!("customize/mcp-servers"),
    page!("customize/mcp/calendar"),
    page!("customize/mcp/github"),
    page!("customize/mcp/gmail"),
    page!("customize/premium"),
    page!("customize/providers/anthropic"),
    page!("customize/providers/bedrock"),
    page!("customize/providers/openai-compatible"),
    page!("customize/signing-in"),
    page!("customize/skills"),
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
    page!("security/security"),
    page!("security/trust"),
    page!("security/vetting"),
    page!("troubleshooting"),
    page!("using/context"),
    page!("using/editors"),
    page!("using/headless"),
    page!("using/interactive-mode"),
    page!("using/sessions"),
    page!("using/shell-mode"),
    page!("using/transcript"),
    page!("using/vi-mode"),
    page!("using/watches"),
];

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
    PAGES
        .iter()
        .find(|page| page.id == id)
        .map(|page| crate::skills::body_after_frontmatter(page.text))
}

/// What the skill tells the planner, and then one line per page: its name, title and description.
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
        out
    })
}

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
