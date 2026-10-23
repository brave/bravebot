// The clause headings of `docs/specs/`, read out of each spec at build time.
//
// The build script `include!`s this file and writes the pages the help skill serves, so a spec that
// changes is carried the next time the program is built and no generated file is kept in the tree.
// The running program holds finished text and parses nothing (SKILL-17).

/// What a spec file says about itself and its clauses.
pub struct Spec {
    /// The clause prefix from the frontmatter, such as `CMD`.
    pub prefix: String,
    /// The frontmatter title, or the clause prefix when the frontmatter has none.
    pub title: String,
    /// `normative` or `proposed`.
    pub status: String,
    /// One line per clause, as the heading writes it: `CMD-24: ...`.
    pub headings: Vec<String>,
}

/// The value of a top-level `key: value` line of the frontmatter.
fn declared<'a>(frontmatter: &[&'a str], key: &str) -> Option<&'a str> {
    frontmatter.iter().find_map(|line| {
        line.strip_prefix(key)?
            .strip_prefix(':')
            .map(|value| value.trim().trim_matches(['"', '\'']))
    })
}

/// Whether `text` is a clause heading's words: an id, a hyphen, a number, a colon and a space.
fn starts_with_clause_id(text: &str) -> bool {
    let letters = text.bytes().take_while(u8::is_ascii_uppercase).count();
    let rest = &text[letters..];
    let Some(rest) = rest.strip_prefix('-').filter(|_| letters > 0) else {
        return false;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && rest[digits..].starts_with(": ")
}

/// The character and count of the fence marks a line starts with, ignoring indentation.
fn fence_marks(line: &str) -> Option<(char, usize)> {
    let line = line.trim_start();
    let kind = line.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let count = line.chars().take_while(|c| *c == kind).count();
    (count >= 3).then_some((kind, count))
}

/// The spec a file holds, or `None` for a file with no frontmatter `id`, such as the directory's
/// README.
///
/// A heading is a `### ` line outside a code fence whose text starts with a clause id. The other
/// `###` lines of a spec are sub-sections of a clause-less part and name no rule.
pub fn parse(text: &str) -> Option<Spec> {
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    let mut frontmatter = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        frontmatter.push(line);
    }
    if !closed {
        return None;
    }
    let prefix = declared(&frontmatter, "id").filter(|id| !id.is_empty())?;

    // The fence that is open: its character and how many it was written with. A line closes it only
    // if it is the same character, at least as many, and nothing else, so a shorter or different
    // fence quoted inside it does not end it.
    let mut fence: Option<(char, usize)> = None;
    let mut headings = Vec::new();
    for line in lines {
        let marks = fence_marks(line);
        match (fence, marks) {
            (None, Some(open)) => fence = Some(open),
            (Some((kind, length)), Some((found, count)))
                if found == kind && count >= length && line.trim().len() == count =>
            {
                fence = None;
            }
            (None, None) => {
                if let Some(heading) = line.strip_prefix("### ")
                    && starts_with_clause_id(heading)
                {
                    headings.push(heading.trim_end().to_string());
                }
            }
            _ => {}
        }
    }
    Some(Spec {
        prefix: prefix.to_string(),
        title: declared(&frontmatter, "title")
            .unwrap_or(prefix)
            .to_string(),
        status: declared(&frontmatter, "status")
            .unwrap_or("normative")
            .to_string(),
        headings,
    })
}

/// The name a spec's page is loaded by, below the help skill's own name.
pub fn page_id(path_below_specs: &str) -> String {
    format!("specs/{path_below_specs}")
}

/// The page for one spec: where it lives, what it is, and its clause headings.
pub fn page(path_below_specs: &str, spec: &Spec) -> String {
    let mut out = format!(
        "# {}\n\nSpec file: docs/specs/{path_below_specs}.md, clause ids `{}-<number>`.\n",
        spec.title, spec.prefix
    );
    if spec.status == "proposed" {
        out.push_str(
            "Status: proposed. This spec describes behaviour that may not be built in this version, \
             so say that when answering from it.\n",
        );
    }
    out.push_str(
        "\nEach line is the heading of one clause. It states the rule in a sentence and the clause \
         text is not carried here, so answer from the heading and give the clause id and the file \
         for the rest.\n\n",
    );
    for heading in &spec.headings {
        out.push_str(heading);
        out.push('\n');
    }
    out
}

/// The page that lists every spec, one line each, with the name to load its page by.
pub fn list(skill: &str, specs: &[(String, Spec)]) -> String {
    let mut out = String::from(
        "# The specifications\n\nThe specifications of this version, one per topic. Load the page of \
         the one whose topic fits the question, using the name at the start of its line. A page \
         holds the headings of that spec's clauses.\n\n",
    );
    for (path, spec) in specs {
        let proposed = if spec.status == "proposed" {
            ", proposed"
        } else {
            ""
        };
        let count = spec.headings.len();
        let clauses = if count == 1 { "clause" } else { "clauses" };
        out.push_str(&format!(
            "- {skill}/{}: {} ({}, {count} {clauses}{proposed})\n",
            page_id(path),
            spec.title,
            spec.prefix
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "\
---
id: DEMO
title: Demonstrating the parser
status: proposed
governs:
  - crates/demo/src/lib.rs
---

## Scope

### Not a clause

<a id=\"DEMO-1\"></a>
### DEMO-1: the first rule

Text.

```markdown
### DEMO-9: a heading inside a fence is an example
```

````markdown
```
### DEMO-8: a shorter fence quoted inside a longer one does not end it
```
### DEMO-7: still inside the longer fence
````

~~~
### DEMO-6: a tilde fence is a fence
~~~

<a id=\"DEMO-2\"></a>
### DEMO-2: the second rule, with `code`

#### DEMO-3: too deep to be a clause heading
";

    /// A heading inside a fence is a quotation, and carrying it would tell the planner of a clause
    /// that does not exist. A sub-section with no id and a deeper heading are not clauses either.
    #[test]
    fn only_a_clause_heading_outside_a_code_fence_is_a_heading() {
        let spec = parse(SPEC).expect("a spec");
        assert_eq!(
            spec.headings,
            [
                "DEMO-1: the first rule",
                "DEMO-2: the second rule, with `code`"
            ]
        );
    }

    /// A longer fence quoting a shorter one stays open until its own closing line, so a heading
    /// quoted after the shorter fence is an example, and the real headings after the closing line
    /// are still read. A fence that toggled on any mark would carry the example and drop the rest.
    #[test]
    fn a_fence_is_closed_only_by_the_same_mark_at_least_as_long() {
        let text = "---\nid: F\n---\n````\n```\n### F-1: quoted\n```\n### F-2: still quoted\n````\n### F-3: real\n~~~\n```\n### F-4: quoted\n~~~\n### F-5: real\n";
        let spec = parse(text).expect("a spec");
        assert_eq!(spec.headings, ["F-3: real", "F-5: real"]);
    }

    /// A spec's title and status are what the list shows, so reading another key's value or the
    /// body's text in their place misdescribes the spec.
    #[test]
    fn the_frontmatter_names_the_prefix_title_and_status() {
        let spec = parse(SPEC).expect("a spec");
        assert_eq!(
            (
                spec.prefix.as_str(),
                spec.title.as_str(),
                spec.status.as_str()
            ),
            ("DEMO", "Demonstrating the parser", "proposed")
        );
    }

    /// The directory's README is not a spec, and a file whose frontmatter never closes holds no
    /// clauses the planner could be given.
    #[test]
    fn a_file_with_no_spec_id_is_not_a_spec() {
        assert!(parse("# Specs\n\n### CMD-1: a heading\n").is_none());
        assert!(parse("---\ntitle: No id\n---\n### CMD-1: a heading\n").is_none());
        assert!(parse("---\nid: OPEN\n### OPEN-1: never closed\n").is_none());
    }

    /// Only an upper-case id, a hyphen, a number and a colon make a clause heading.
    #[test]
    fn a_heading_needs_an_id_a_number_and_a_colon() {
        for text in ["CMD-1: x", "SHELLINT-12: x"] {
            assert!(starts_with_clause_id(text), "{text}");
        }
        for text in [
            "CMD: x",
            "cmd-1: x",
            "CMD-1 x",
            "-1: x",
            "CMD-: x",
            "Where it lives",
            "",
        ] {
            assert!(!starts_with_clause_id(text), "{text}");
        }
    }

    /// The page tells the planner the file to cite and that a proposed spec may not be built, since
    /// a clause of one read as behaviour answers a question with a feature that does not exist.
    #[test]
    fn a_page_names_its_file_and_says_when_the_spec_is_only_proposed() {
        let spec = parse(SPEC).expect("a spec");
        let text = page("tools/demo", &spec);
        assert!(text.starts_with("# Demonstrating the parser\n"), "{text}");
        assert!(text.contains("docs/specs/tools/demo.md"), "{text}");
        assert!(text.contains("Status: proposed"), "{text}");
        assert!(
            text.ends_with("DEMO-2: the second rule, with `code`\n"),
            "{text}"
        );

        let built = parse(&SPEC.replace("status: proposed", "status: normative")).expect("a spec");
        assert!(!page("demo", &built).contains("proposed"));
    }

    /// The list is how a page is found, so each line carries the exact name `page_id` gives it.
    #[test]
    fn the_list_gives_each_spec_the_name_its_page_is_loaded_by() {
        let specs = vec![("tools/demo".to_string(), parse(SPEC).expect("a spec"))];
        let text = list("help", &specs);
        assert!(
            text.contains(&format!(
                "- help/{}: Demonstrating the parser (DEMO, 2 clauses, proposed)\n",
                page_id("tools/demo")
            )),
            "{text}"
        );
        let one = parse("---\nid: ONE\ntitle: One rule\n---\n### ONE-1: the only rule\n").unwrap();
        let text = list("help", &[("one".to_string(), one)]);
        assert!(text.contains("(ONE, 1 clause)"), "{text}");
    }
}
