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
//! The release notes are carried here too, as `CHANGELOG.md` holds them, for the terminal's
//! `/changelog` and the desktop's Release Notes (CMD-25).
//!
//! The planner selects a page from a fixed table by an exact match of its name. The name is never a
//! path and nothing here reads a file at run time (SKILL-8). A long page is loaded one `##` section
//! at a time, the section named the same way after a `#` (SKILL-17).

use std::borrow::Cow;
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
const PAGES: [Page; 62] = [
    page!("customize/agents"),
    page!("customize/configuration"),
    page!("customize/effort"),
    page!("customize/hooks"),
    page!("customize/importing"),
    page!("customize/instructions"),
    page!("customize/language"),
    page!("customize/managed-settings"),
    page!("customize/mcp-servers"),
    page!("customize/mcp/brave"),
    page!("customize/mcp/calendar"),
    page!("customize/mcp/github"),
    page!("customize/mcp/gmail"),
    page!("customize/memory"),
    page!("customize/model"),
    page!("customize/premium"),
    page!("customize/providers/anthropic"),
    page!("customize/providers/bedrock"),
    page!("customize/providers/chatgpt"),
    page!("customize/providers/google-vertex"),
    page!("customize/providers/llama-server"),
    page!("customize/providers/lm-studio"),
    page!("customize/providers/ollama"),
    page!("customize/providers/openai"),
    page!("customize/providers/openai-compatible"),
    page!("customize/providers/openrouter"),
    page!("customize/providers/xai"),
    page!("customize/signing-in"),
    page!("customize/skills"),
    page!("customize/theme"),
    page!("development"),
    page!("how-it-works"),
    page!("install"),
    page!("intro"),
    page!("quickstart"),
    page!("reference/cli"),
    page!("reference/commands"),
    page!("reference/lsp-tool"),
    page!("reference/run-tool"),
    page!("reference/tools"),
    page!("security/audit-trail"),
    page!("security/permission-modes"),
    page!("security/permission-rules"),
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
    page!("using/sessions-screen"),
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

/// A page of at most this many characters loads whole. A longer one loads as its opening text and
/// the list of its sections, so a question costs the section that answers it and not the page.
const WHOLE_PAGE_LIMIT: usize = 12_000;

/// One section of a page: its heading, the name it loads by, and its text from the heading to the
/// next section.
struct Section<'a> {
    heading: &'a str,
    slug: String,
    text: &'a str,
}

/// The heading lowercased, with each run of characters other than letters and digits written as one
/// hyphen and none at either end.
fn slug_of(heading: &str) -> String {
    let mut out = String::new();
    for c in heading.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The character and count of the fence marks a line starts with, ignoring indentation.
fn fence_marks(line: &str) -> Option<(char, usize)> {
    let line = line.trim_start();
    let kind = line.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let count = line.chars().take_while(|c| *c == kind).count();
    (count >= 3).then_some((kind, count))
}

/// The text before the first section of a page, and its sections in order.
///
/// A section starts at a `## ` line outside a code fence, or at a `### ` line where the page has
/// no `## ` line. A repeated heading gets `-2`, `-3` added to its name so that every section has a
/// name of its own.
fn sections(text: &str) -> (&str, Vec<Section<'_>>) {
    let mut marked: Vec<(usize, usize, &str)> = Vec::new();
    let mut fence = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        match (fence, fence_marks(line)) {
            (None, Some(open)) => fence = Some(open),
            (Some((kind, length)), Some((found, count)))
                if found == kind && count >= length && line.trim().len() == count =>
            {
                fence = None;
            }
            (None, None) => {
                if let Some(heading) = line.strip_prefix("## ") {
                    marked.push((at, 2, heading.trim()));
                } else if let Some(heading) = line.strip_prefix("### ") {
                    marked.push((at, 3, heading.trim()));
                }
            }
            _ => {}
        }
        at += line.len();
    }
    let level = marked.iter().map(|head| head.1).min().unwrap_or(2);
    let heads: Vec<(usize, &str)> = marked
        .iter()
        .filter(|head| head.1 == level)
        .map(|head| (head.0, head.2))
        .collect();
    let intro = text
        .get(..heads.first().map_or(text.len(), |head| head.0))
        .unwrap_or_default();
    let mut taken: Vec<String> = Vec::new();
    let found = heads
        .iter()
        .enumerate()
        .map(|(n, &(from, heading))| {
            let to = heads.get(n + 1).map_or(text.len(), |next| next.0);
            let base = match slug_of(heading) {
                empty if empty.is_empty() => "section".to_string(),
                slug => slug,
            };
            let mut slug = base.clone();
            let mut count = 1;
            while taken.contains(&slug) {
                count += 1;
                slug = format!("{base}-{count}");
            }
            taken.push(slug.clone());
            Section {
                heading,
                slug,
                text: text.get(from..to).unwrap_or_default().trim_end(),
            }
        })
        .collect();
    (intro, found)
}

/// What a long page loads as: its opening text, then one line per section with the name to load it
/// by and its size.
fn outline(id: &str, text: &str, sections: &[Section<'_>], intro: &str) -> String {
    let mut out = String::from(intro.trim_end());
    out.push_str(&format!(
        "\n\nThis page is {} characters, so it loads by section. Load the section whose heading fits \
         the question with load_skill, using the name at the start of its line:\n\n",
        text.chars().count()
    ));
    for section in sections {
        out.push_str(&format!(
            "- {}#{}: {} ({} characters)\n",
            name_of(id),
            section.slug,
            section.heading,
            section.text.chars().count()
        ));
    }
    out
}

/// How the index states a page's size: its characters, and for a page that loads by section, how
/// many sections it has.
fn size_note(text: &str) -> String {
    let characters = text.chars().count();
    let count = sections(text).1.len();
    if characters > WHOLE_PAGE_LIMIT && count > 0 {
        format!("({characters} characters in {count} sections)")
    } else {
        format!("({characters} characters)")
    }
}

/// The text of the page of this name, without its frontmatter, or `None` for any other name.
///
/// A page of more than `WHOLE_PAGE_LIMIT` characters is answered with its opening text and the
/// names of its sections. `<page name>#<section>` is the text of one section.
///
/// An exact match against the table. A name that is a traversal, a path, or one character out
/// matches nothing, and neither does a section the page does not have.
pub fn page(name: &str) -> Option<Cow<'static, str>> {
    let id = name.strip_prefix(SKILL)?.strip_prefix('/')?;
    let (id, wanted) = match id.split_once('#') {
        Some((id, section)) => (id, Some(section)),
        None => (id, None),
    };
    if wanted.is_none() {
        if id == SPEC_LIST {
            return Some(Cow::Borrowed(specs::LIST));
        }
        if let Some(spec) = specs::SPECS.iter().find(|(spec_id, _)| *spec_id == id) {
            return Some(Cow::Borrowed(spec.1));
        }
    }
    let page = PAGES.iter().find(|page| page.id == id)?;
    let text = crate::skills::body_after_frontmatter(page.text);
    let (intro, found) = sections(text);
    match wanted {
        Some(slug) => found
            .into_iter()
            .find(|section| section.slug == slug)
            .map(|section| Cow::Borrowed(section.text)),
        None if text.chars().count() > WHOLE_PAGE_LIMIT && !found.is_empty() => {
            Some(Cow::Owned(outline(id, text, &found, intro)))
        }
        None => Some(Cow::Borrowed(text)),
    }
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
                "- {}: {}. {} {}\n",
                name_of(page.id),
                said("title"),
                said("description"),
                size_note(crate::skills::body_after_frontmatter(page.text))
            ));
        }
        // The page of specs always loads whole, so its size is the plain character count.
        out.push_str(&format!(
            "- {}: {SPEC_LIST_SUMMARY} ({} characters)\n",
            name_of(SPEC_LIST),
            specs::LIST.chars().count()
        ));
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
load_skill, using the name at the start of its line. Each line ends with the size of the page. A \
page of more than 12000 characters loads as a list of its sections with their sizes: load the one \
section whose heading fits the question, using the name on its line. Do not load every page or \
every section: a question costs what it loads. If what you loaded does not answer it, say so \
rather than guessing.

Pages:
";

/// The release notes, newest first, as `CHANGELOG.md` holds them.
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// One release's notes: the version its heading names and the lines under the heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Release<'a> {
    pub version: &'a str,
    pub notes: &'a str,
}

/// The releases `text` holds, in the order it holds them.
///
/// A release starts at a line `## [<version>](<address>)` and runs to the next such line. Anything
/// before the first one belongs to no release, and a line that starts `## [` without an address is
/// part of the notes above it.
pub(crate) fn releases_in(text: &str) -> Vec<Release<'_>> {
    let mut heads = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("## [")
            && let Some((version, after)) = rest.split_once(']')
            && after.starts_with('(')
        {
            heads.push((at, at + line.len(), version));
        }
        at += line.len();
    }
    heads
        .iter()
        .enumerate()
        .map(|(n, &(_, notes_from, version))| {
            let end = heads.get(n + 1).map_or(text.len(), |next| next.0);
            Release {
                version,
                notes: text
                    .get(notes_from..end)
                    .unwrap_or_default()
                    .trim_matches(['\n', '\r']),
            }
        })
        .collect()
}

/// The release among `releases` named `version`, or the first of them where none is named that.
///
/// The name has to be the whole version: `0.1` is not the release `0.17.0`.
pub(crate) fn release_for<'a>(releases: &[Release<'a>], version: &str) -> Option<Release<'a>> {
    releases
        .iter()
        .find(|release| release.version == version)
        .or(releases.first())
        .copied()
}

/// The notes of the release this build carries for `version`, or the newest release's where it
/// carries none for it.
///
/// `None` only where the changelog holds no release at all. A version with no heading, such as a
/// development build whose version was set by hand, is answered with the newest release so that
/// `/changelog` and the desktop's Release Notes show something; the caller says which version it
/// found.
pub fn release_notes(version: &str) -> Option<Release<'static>> {
    release_for(&releases_in(CHANGELOG), version)
}

/// The release notes as the transcript shows them: each markdown link written as its text, so a
/// bullet ending `([#1551](https://github.com/brave/bravebot/issues/1551))` ends `(#1551)`.
///
/// Anything that is not a whole link is left as it was written: a bracket with no address after
/// it, an image, and a link inside a code span.
pub fn plain(notes: &str) -> String {
    notes
        .split('\n')
        .map(plain_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// One line of the notes. A code span and a link end with their line, so nothing is carried over
/// from the line before, and a backtick left open on one bullet does not hide the links on the rest.
fn plain_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    let mut in_code = false;
    while let Some(at) = rest.find(['[', '`']) {
        let (before, here) = rest.split_at(at);
        out.push_str(before);
        if let Some(after) = here.strip_prefix('`') {
            in_code = !in_code;
            out.push('`');
            rest = after;
            continue;
        }
        let image = before.ends_with('!');
        match link_at(here) {
            Some((label, length)) if !in_code && !image => {
                out.push_str(label);
                rest = here.split_at(length).1;
            }
            _ => {
                out.push('[');
                rest = here.split_at('['.len_utf8()).1;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The label of the link `text` starts with and how many bytes the link takes up.
///
/// The label holds no bracket. The address ends at the parenthesis that closes the one opening it,
/// so a `(bar)` inside an address belongs to it.
fn link_at(text: &str) -> Option<(&str, usize)> {
    let (label, after) = text.strip_prefix('[')?.split_once("](")?;
    if label.is_empty() || label.contains(['[', ']']) {
        return None;
    }
    let mut depth = 1;
    for (at, c) in after.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((label, "[".len() + label.len() + "](".len() + at + ")".len()));
                }
            }
            _ => {}
        }
    }
    None
}

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

    /// The text of the published page of this id, as the planner is given it.
    fn page_text(id: &str) -> &'static str {
        let page = PAGES
            .iter()
            .find(|page| page.id == id)
            .expect("a published page");
        crate::skills::body_after_frontmatter(page.text)
    }

    /// The regression it rejects: a section running past the next heading, a heading inside a code
    /// fence starting a section, or two sections sharing a name so that one cannot be loaded.
    #[test]
    fn a_section_runs_to_the_next_heading_and_a_fenced_heading_starts_none() {
        let text = "# Title\n\nintro\n\n## First one\n\nbody\n\n```sh\n## not a heading\n```\n\n### Sub\n\nmore\n\n## Second, with `code`!\n\nend\n\n## First one\n\nagain\n";
        let (intro, found) = sections(text);
        assert_eq!(intro, "# Title\n\nintro\n\n");
        let slugs: Vec<&str> = found.iter().map(|section| section.slug.as_str()).collect();
        assert_eq!(slugs, ["first-one", "second-with-code", "first-one-2"]);
        assert_eq!(
            found[0].text,
            "## First one\n\nbody\n\n```sh\n## not a heading\n```\n\n### Sub\n\nmore"
        );
        assert_eq!(found[1].text, "## Second, with `code`!\n\nend");
        assert_eq!(found[2].text, "## First one\n\nagain");
    }

    /// The regression it rejects: a long page loading whole, which puts tens of thousands of
    /// characters in every later round for a question one section answers, or an outline naming a
    /// section that does not load.
    #[test]
    fn a_long_page_loads_as_its_sections_and_a_section_loads_alone() {
        let whole = page_text("reference/commands");
        assert!(whole.chars().count() > WHOLE_PAGE_LIMIT);
        let outline = page("bravebot-help/reference/commands").expect("the commands page");
        assert!(outline.len() < whole.len() / 5, "{} bytes", outline.len());

        let named: Vec<&str> = outline
            .lines()
            .filter_map(|line| line.strip_prefix("- "))
            .map(|line| line.split(": ").next().unwrap())
            .collect();
        let (_, found) = sections(whole);
        assert!(found.len() > 10, "{} sections", found.len());
        assert_eq!(named.len(), found.len());
        for (name, section) in named.iter().zip(&found) {
            assert!(
                name.starts_with("bravebot-help/reference/commands#"),
                "{name}"
            );
            let loaded = page(name).unwrap_or_else(|| panic!("{name} loads nothing"));
            assert!(
                loaded.ends_with(section.text.lines().last().unwrap())
                    && loaded.contains(section.heading),
                "{name}: {loaded}"
            );
            assert!(loaded.len() < whole.len() / 2, "{name} is most of the page");
        }
    }

    /// The regression it rejects: a page small enough to answer a question being split into
    /// sections, which costs a second call for no saving.
    #[test]
    fn a_short_page_loads_whole() {
        let whole = page_text("using/watches");
        assert!(whole.chars().count() <= WHOLE_PAGE_LIMIT);
        assert_eq!(
            &*page("bravebot-help/using/watches").expect("the watches page"),
            whole
        );
    }

    /// The regression it rejects: splitting only at `##`, which leaves a page whose sections are all
    /// `###` as one block that loads whole.
    #[test]
    fn a_page_with_only_third_level_headings_splits_at_them() {
        let (intro, found) = sections(
            "# T

intro

### A

a

### B

b
",
        );
        assert_eq!(
            intro,
            "# T

intro

"
        );
        let slugs: Vec<&str> = found.iter().map(|section| section.slug.as_str()).collect();
        assert_eq!(slugs, ["a", "b"]);
        assert_eq!(found[1].text, "### B\n\nb");
    }

    /// The regression it rejects: a long page with no section to load it by, which the outline would
    /// list as empty and so leave unreadable.
    #[test]
    fn every_long_page_has_sections_to_load_it_by() {
        for page in &PAGES {
            let text = crate::skills::body_after_frontmatter(page.text);
            if text.chars().count() > WHOLE_PAGE_LIMIT {
                assert!(!sections(text).1.is_empty(), "{} has no sections", page.id);
            }
        }
    }

    /// The same table-only rule as for a page: a near-miss, another page's section, a spec page's
    /// or a traversal must not reach text the table does not name.
    #[test]
    fn a_section_is_found_by_its_exact_name_and_nothing_else() {
        let (_, found) = sections(page_text("reference/commands"));
        let slug = &found[0].slug;
        assert!(page(&format!("bravebot-help/reference/commands#{slug}")).is_some());
        for wrong in [
            "bravebot-help/reference/commands#".to_string(),
            "bravebot-help/reference/commands#no-section-has-this-heading".to_string(),
            format!("bravebot-help/reference/commands#{slug}x"),
            format!("bravebot-help/reference/commands#{slug}-"),
            format!("bravebot-help/reference/commands#{slug}#{slug}"),
            format!("bravebot-help/reference/commands#{slug}/"),
            format!("bravebot-help/reference/commandz#{slug}"),
            format!("bravebot-help/using/watches#{slug}"),
            format!("bravebot-help#{slug}"),
            format!("bravebot-help/specs#{slug}"),
            format!("bravebot-help/specs/commands#{slug}"),
            format!("reference/commands#{slug}"),
            format!("#{slug}"),
        ] {
            assert!(page(&wrong).is_none(), "{wrong} named a page");
        }
    }

    /// The regression it rejects: an index that gives no size, so the planner picks a page
    /// without knowing it costs 50,000 characters, or one that says a short page loads by section.
    #[test]
    fn the_index_gives_the_size_of_every_page_and_says_which_load_by_section() {
        let index = body();
        assert!(
            index.contains(&format!("{WHOLE_PAGE_LIMIT} characters")),
            "{index}"
        );
        for page in &PAGES {
            let text = crate::skills::body_after_frontmatter(page.text);
            let prefix = format!("- {}: ", name_of(page.id));
            let line = index
                .lines()
                .find(|line| line.starts_with(&prefix))
                .unwrap_or_else(|| panic!("{} is not in the index", page.id));
            let characters = text.chars().count();
            if characters > WHOLE_PAGE_LIMIT {
                let count = sections(text).1.len();
                assert!(
                    line.ends_with(&format!("({characters} characters in {count} sections)")),
                    "{line}"
                );
            } else {
                assert!(
                    line.ends_with(&format!("({characters} characters)")),
                    "{line}"
                );
            }
        }
        let prefix = format!("- {}: ", name_of(SPEC_LIST));
        let line = index
            .lines()
            .find(|line| line.starts_with(&prefix))
            .expect("the specs page is in the index");
        assert!(
            line.ends_with(&format!("({} characters)", specs::LIST.chars().count())),
            "{line}"
        );
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

    /// The release notes link every issue number, and a line drawn with the link's address in it
    /// is a line most of which is a URL.
    #[test]
    fn a_link_is_drawn_as_its_text_and_the_rest_of_the_line_is_untouched() {
        assert_eq!(
            plain(
                " - Fixed a hang. ([#2122](https://github.com/brave/bravebot/issues/2122), \
[#2123](https://github.com/brave/bravebot/issues/2123))"
            ),
            " - Fixed a hang. (#2122, #2123)"
        );
        assert_eq!(plain("no link here"), "no link here");
    }

    /// Brackets that are not a link are the release notes' own words, and dropping them would
    /// change what a line says.
    #[test]
    fn brackets_that_are_not_a_link_are_kept() {
        for text in [
            "Added `[a | b]` to the hint",
            "[unclosed](https://example.org",
            "[](https://example.org/empty)",
            "a [b] c (d)",
        ] {
            assert_eq!(plain(text), text);
        }
        assert_eq!(plain("[a [b](https://x.test/y)"), "[a b");
    }

    /// An address that is not closed on its line is not an address, so the lines after it, and the
    /// parenthesis that closes some later sentence, are not swallowed into one.
    #[test]
    fn an_address_that_is_not_closed_takes_nothing_after_it() {
        for text in [
            "a [b](c\n - Added foo (#1)",
            "a [b](c\n - Added foo (a)) more",
            "Added `[a](b` and later (see c)",
            "[b](prose that opens (like this)",
        ] {
            assert_eq!(plain(text), text);
        }
    }

    /// A backtick left open on one bullet ends with that bullet. Carried on, it would hide the links
    /// of every bullet after it and leave their addresses in the lines.
    #[test]
    fn a_backtick_left_open_does_not_hide_the_links_on_the_next_line() {
        assert_eq!(
            plain("Added the ` key and [x](y)\n - Later [z](w)"),
            "Added the ` key and [x](y)\n - Later z"
        );
    }

    /// A parenthesis inside an address closes only its own pair, or the rest of the address would
    /// be printed after the label.
    #[test]
    fn an_address_with_parentheses_of_its_own_is_one_address() {
        assert_eq!(
            plain("[wiki](https://en.wikipedia.org/wiki/Foo_(bar)) and [b](c)"),
            "wiki and b"
        );
    }

    /// An image and a link in a code span are not links the notes ask to have followed, so they
    /// are shown as written.
    #[test]
    fn an_image_and_a_link_in_a_code_span_are_kept_and_a_link_after_them_is_not() {
        assert_eq!(plain("![alt](u) [z](w)"), "![alt](u) z");
        assert_eq!(plain("`[x](y)` then [z](w)"), "`[x](y)` then z");
        assert_eq!(plain("[a!](u)[b](v)"), "a!b");
    }

    const FIXTURE: &str = "\
Release notes.

## [0.18.0](https://example.test/v0.18.0)

 - Added a thing. ([#1](https://example.test/1))
### [not a release](https://example.test/x)
 - Mentions a `## [0.0.0]` heading in the middle of a line.
## [Unreleased]
 - Under a heading with no address.
## [Docs] see [the docs](https://example.test/docs)
 - Under a heading with prose after the bracket.

## [0.17.0](https://example.test/v0.17.0)

 - Fixed a thing.

## [0.1.0](https://example.test/v0.1.0)
 - First.
";

    /// A release is the lines under its own heading and no others, so the notes of one version are
    /// never shown under another's name.
    #[test]
    fn a_release_is_the_lines_under_its_heading_up_to_the_next_heading() {
        let releases = releases_in(FIXTURE);
        let versions: Vec<&str> = releases.iter().map(|release| release.version).collect();
        assert_eq!(versions, ["0.18.0", "0.17.0", "0.1.0"]);
        assert_eq!(releases[1].notes, " - Fixed a thing.");
        assert_eq!(releases[2].notes, " - First.");
    }

    /// A subheading, a heading with no address and a heading written in the middle of a line are
    /// part of a release's notes. Splitting on them would cut a release short and invent a version.
    #[test]
    fn only_a_line_that_begins_with_the_heading_mark_starts_a_release() {
        let notes = releases_in(FIXTURE)[0].notes;
        assert!(notes.contains("### [not a release]"), "{notes}");
        assert!(notes.contains("a `## [0.0.0]` heading"), "{notes}");
        assert!(notes.contains("## [Unreleased]"), "{notes}");
        assert!(notes.contains("## [Docs] see [the docs]"), "{notes}");
        assert!(notes.starts_with(" - Added a thing."), "{notes}");
    }

    /// Text before the first heading belongs to no release, and a text with no heading holds none.
    #[test]
    fn text_before_the_first_heading_is_no_release() {
        assert!(releases_in("Release notes.\n\nNo headings.\n").is_empty());
        assert!(releases_in("").is_empty());
        assert!(
            !releases_in(FIXTURE)
                .iter()
                .any(|r| r.notes.contains("Release notes."))
        );
    }

    /// The version has to be the whole of a heading's version. A prefix match would answer a build
    /// running `0.1` with the notes of `0.17.0`.
    #[test]
    fn a_release_is_found_by_its_whole_version_and_else_the_newest_is_given() {
        let releases = releases_in(FIXTURE);
        assert_eq!(
            release_for(&releases, "0.1.0").map(|r| r.notes),
            Some(" - First.")
        );
        for unnamed in ["0.1", "0.17", "0.17.0-dev", "9.9.9", ""] {
            assert_eq!(
                release_for(&releases, unnamed).map(|r| r.version),
                Some("0.18.0"),
                "{unnamed:?}"
            );
        }
        assert_eq!(release_for(&[], "0.17.0"), None);
    }

    /// `/changelog` reads the carried file with this parser, so a heading it stops being written
    /// with would leave the command with nothing to show and the tests above passing. The first
    /// release is the one shown where a version has no heading, so it is the newest.
    #[test]
    fn the_carried_changelog_lists_releases_newest_first_and_the_newest_has_notes() {
        fn number(version: &str) -> Vec<u32> {
            version
                .split(['.', '-'])
                .filter_map(|part| part.parse().ok())
                .take(3)
                .collect()
        }
        let releases = releases_in(CHANGELOG);
        assert!(releases.len() > 1);
        assert!(
            !releases[0].notes.is_empty(),
            "{} has no notes",
            releases[0].version
        );
        for pair in releases.windows(2) {
            assert!(
                number(pair[0].version) >= number(pair[1].version),
                "{} is listed before {}",
                pair[0].version,
                pair[1].version
            );
        }
        assert_eq!(release_notes(releases[1].version), Some(releases[1]));
    }
}
