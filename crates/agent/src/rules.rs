//! Rule files: standing instructions that load because of a path (INSTR-16).
//!
//! A rule is one markdown file in `<workspace>/.bravebot/rules` or `~/.bravebot/rules`. One with no
//! `paths` in its frontmatter is standing text like the project's `AGENTS.md`. One with a `paths`
//! list is held back until the session has touched a file one of its globs matches, which keeps a
//! convention for one kind of file out of every request that never goes near one.
//!
//! # What decides, and what does not
//!
//! The only thing compared with a glob is a name the planner typed to a file tool, or one the
//! person typed with `@`, placed under the root by its spelling
//! ([`crate::workspace::Workspace::record_touch`]). No file content and no tool output is compared,
//! so nothing untrusted decides which trusted rule loads. A rule's text arrives through the same
//! read, trust map and deny rules as the project's `AGENTS.md`. The project's directory is listed
//! to count what is in it, and an untrusted one is counted and never named.

use crate::agents::markdown_files;
use crate::confirm::shown_name;
use crate::skills::{
    Block, Notice, body_after_frontmatter, frontmatter_lines, indent_of, same_directory, unquoted,
};
use crate::workspace::Workspace;
use bravebot_core::capability::Capability;
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use std::path::Path;

/// Where a project keeps its rule files.
pub(crate) const WORKSPACE_RULES: &str = ".bravebot/rules";

/// Where the person keeps their own, beneath `~/.bravebot`.
const HOME_RULES: &str = "rules";

/// One `paths` entry, ready to be compared with a name.
#[derive(Debug, Clone)]
struct Glob {
    /// The glob after [`crate::glob::expand`], so one glob applied to many names expands once.
    expanded: Vec<Alternative>,
}

/// One of the plain patterns a glob's `{a,b}` groups expand to.
#[derive(Debug, Clone)]
struct Alternative {
    pattern: String,
    /// Written with a leading `/` or `./` and no other `/`, which names the project root's own
    /// files. Without the mark the pattern would be read as a bare name and match at any depth.
    /// Decided for each alternative, since `/{src/a,b}.rs` is two patterns and only one has a `/`.
    root_only: bool,
}

impl Glob {
    /// `None` where nothing is left once the leading `./` or `/` is taken off, since globs are
    /// compared with names relative to the project root. A glob ending in `/` names everything
    /// below that directory.
    fn new(written: &str) -> Option<Self> {
        let anchored = written.starts_with('/') || written.starts_with("./");
        let mut glob = written
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_string();
        if glob.is_empty() {
            return None;
        }
        if glob.ends_with('/') {
            glob.push_str("**");
        }
        Some(Self {
            expanded: crate::glob::expand(&glob)
                .into_iter()
                .map(|pattern| Alternative {
                    root_only: anchored && !pattern.contains('/'),
                    pattern,
                })
                .collect(),
        })
    }

    fn matches(&self, name: &str) -> bool {
        self.expanded.iter().any(|alternative| {
            crate::glob::matches_any(std::slice::from_ref(&alternative.pattern), name)
                && (!alternative.root_only || !name.contains('/') || alternative.pattern == "**")
        })
    }
}

/// Which files a rule is about.
#[derive(Debug, Clone)]
enum Scope {
    /// No `paths` key: every turn, like the project's `AGENTS.md`.
    Always,
    /// A `paths` list. An empty one names no file, and the rule never loads.
    Paths(Vec<Glob>),
}

/// One rule file that passed the trust gate.
#[derive(Debug, Clone)]
pub(crate) struct Rule {
    /// Where it was read from: `.bravebot/rules/<name>.md` or `~/.bravebot/rules/<name>.md`.
    pub(crate) origin: String,
    /// What the planner is given, with the frontmatter taken off and the whitespace at the ends.
    pub(crate) text: String,
    scope: Scope,
}

/// The rules a turn starts with: the ones in force on every turn, and the ones held for a path.
#[derive(Debug, Default)]
pub(crate) struct Found {
    /// `~/.bravebot/rules` with no `paths`, in name order.
    pub(crate) home: Vec<Rule>,
    /// `<workspace>/.bravebot/rules` with no `paths`, in name order.
    pub(crate) project: Vec<Rule>,
    /// Every rule with a `paths` list, the person's own before the project's.
    pub(crate) held: Held,
}

/// Rules with a `paths` list that no file touched so far matches.
#[derive(Debug, Clone, Default)]
pub struct Held {
    rules: Vec<Rule>,
    /// Whether the heading over the rules a session has had added has been written, so one is
    /// written once however many rules arrive.
    introduced: bool,
    /// How many of the files the session has touched were compared with the globs, so a round
    /// compares the ones touched since and no others.
    compared: usize,
}

/// A held rule a touched file matched.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub(crate) origin: String,
    pub(crate) text: String,
    /// The first of the touched names, as [`Held::take`] was given them, one of the rule's globs
    /// matched.
    pub(crate) matched: String,
    /// Whether this is the first rule added to the session's prompt, which carries the heading.
    pub(crate) introduces: bool,
}

impl Held {
    /// Whether nothing is held, so nothing needs asking of the touched names.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The held rules a file `workspace` has had touched since the last time this was asked
    /// matches, removed from the set (INSTR-16).
    pub(crate) fn take_for(&mut self, workspace: &Workspace) -> Vec<Loaded> {
        if self.rules.is_empty() {
            return Vec::new();
        }
        let new = workspace.touched_files_from(self.compared);
        if new.is_empty() {
            return Vec::new();
        }
        self.compared += new.len();
        self.take(&new, &workspace.rules_added())
    }

    /// Move every held rule one of `files` matches out of the set.
    ///
    /// The ones in `earlier`, which is where the session added rules before, come first and in that
    /// order, so a turn that starts later puts them where the turn that added them did. The rest
    /// follow in the order they were held. Only names are compared, with the globs the rule's
    /// frontmatter wrote.
    fn take(&mut self, files: &[String], earlier: &[String]) -> Vec<Loaded> {
        let mut loaded = Vec::new();
        let mut kept = Vec::new();
        for rule in std::mem::take(&mut self.rules) {
            let matched = match &rule.scope {
                Scope::Paths(globs) => files
                    .iter()
                    .find(|file| globs.iter().any(|glob| glob.matches(file))),
                Scope::Always => None,
            };
            match matched {
                Some(file) => loaded.push(Loaded {
                    origin: rule.origin,
                    text: rule.text,
                    matched: file.clone(),
                    introduces: false,
                }),
                None => kept.push(rule),
            }
        }
        self.rules = kept;
        loaded.sort_by_key(|rule| {
            earlier
                .iter()
                .position(|origin| *origin == rule.origin)
                .unwrap_or(usize::MAX)
        });
        for rule in &mut loaded {
            rule.introduces = !std::mem::replace(&mut self.introduced, true);
        }
        loaded
    }
}

/// Read the rule files a turn can see: the person's own, then the project's.
///
/// The person's directory is trusted for sitting in `~/.bravebot`, unless the way to it goes
/// through a directory a turn can write to (SKILL-3). The project's goes through the trust map and
/// the `deny` rules. Its file names are counted and never shown unless the map vouches for the
/// directory, since a file name in a directory nobody vouched for is content too.
pub(crate) fn read<S: Sink>(
    policy: &mut Policy<'_, S>,
    sources: &Workspace,
    workspace: &Workspace,
    home: Option<&Path>,
    notices: &mut Vec<Notice>,
) -> Found {
    let mut found = Found::default();
    let mut held = Vec::new();
    let from_home = match home {
        Some(home) => read_home(policy, sources, workspace, home, notices),
        None => Vec::new(),
    };
    for rule in from_home {
        match rule.scope {
            Scope::Always => found.home.push(rule),
            Scope::Paths(_) => held.push(rule),
        }
    }
    for rule in read_workspace(policy, sources, notices) {
        match rule.scope {
            Scope::Always => found.project.push(rule),
            Scope::Paths(_) => held.push(rule),
        }
    }
    found.held = Held {
        rules: held,
        ..Held::default()
    };
    found
}

/// The rules in `~/.bravebot/rules`, labelled from where they sit.
fn read_home<S: Sink>(
    policy: &mut Policy<'_, S>,
    sources: &Workspace,
    workspace: &Workspace,
    home: &Path,
    notices: &mut Vec<Notice>,
) -> Vec<Rule> {
    if policy.before_capability(Capability::FileRead).is_err() {
        return Vec::new();
    }
    let root = home.join(HOME_RULES);
    let (files, not_text) = markdown_files(&root);

    let mut rules = Vec::new();
    let mut passed_over = 0;
    for file in files {
        let path = root.join(&file);
        if [sources, workspace]
            .into_iter()
            .any(|workspace| crate::skills::goes_through_the_workspace(workspace, &path))
        {
            passed_over += 1;
            continue;
        }
        let origin = format!("~/.bravebot/{HOME_RULES}/{file}");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let labelled = policy.label_user_configuration(&origin, text);
        let Ok(text) = policy.read_trusted_content("preamble", &labelled) else {
            continue;
        };
        rules.extend(admit(&text, origin, notices));
    }
    // The project's own pass has said what it did with each file where it reads this directory.
    if (passed_over > 0 || not_text > 0)
        && !same_directory(&root, &sources.root().join(WORKSPACE_RULES))
    {
        if not_text > 0 {
            notices.push(Notice::rule_names_not_text(
                &format!("~/.bravebot/{HOME_RULES}"),
                not_text,
            ));
        }
        if passed_over > 0 {
            notices.push(Notice::home_rules_reached_through_the_workspace(
                passed_over,
            ));
        }
    }
    rules
}

/// The rules in `<workspace>/.bravebot/rules`, labelled by the trust map.
fn read_workspace<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    notices: &mut Vec<Notice>,
) -> Vec<Rule> {
    let (files, not_text) = markdown_files(&workspace.root().join(WORKSPACE_RULES));
    if !policy.trust().is_trusted(WORKSPACE_RULES) {
        if files.len() + not_text > 0 {
            notices.push(Notice::rules_directory_not_trusted(files.len() + not_text));
        }
        return Vec::new();
    }
    if not_text > 0 {
        notices.push(Notice::rule_names_not_text(WORKSPACE_RULES, not_text));
    }

    let mut rules = Vec::new();
    for file in files {
        let relative = format!("{WORKSPACE_RULES}/{file}");
        let shown = shown_name(&relative);
        if workspace.rule_denies_reading(policy, &relative) {
            notices.push(Notice::denied_by_rule(&shown));
            continue;
        }
        match crate::preamble::read_instructions_as(policy, workspace, &relative, &shown) {
            Ok(Some(text)) => rules.extend(admit(&text, relative, notices)),
            Ok(None) => {}
            Err(notice) => notices.push(notice),
        }
    }
    rules
}

/// One file that passed the gate, as a rule: where it applies, and what it says.
///
/// Nothing where it says nothing, which is not worth a line. A file whose frontmatter, or whose
/// `paths` list, is opened and never closed is left out rather than read as text for every turn,
/// since it was written to apply to some paths and which ones cannot be read. A byte order mark
/// and blank lines before the first marker are not part of the file's text: an editor puts them
/// there, and a rule would otherwise be read with its frontmatter in it.
fn admit(text: &str, origin: String, notices: &mut Vec<Notice>) -> Option<Rule> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let scope = match frontmatter_lines(text) {
        Block::Absent => Scope::Always,
        Block::Unclosed => {
            notices.push(Notice::rule_frontmatter_unreadable(&shown_name(&origin)));
            return None;
        }
        Block::Lines(block) => match paths_declared(&block) {
            Paths::Absent => Scope::Always,
            Paths::Unreadable => {
                notices.push(Notice::rule_frontmatter_unreadable(&shown_name(&origin)));
                return None;
            }
            Paths::Listed(written) => {
                Scope::Paths(written.iter().filter_map(|glob| Glob::new(glob)).collect())
            }
        },
    };
    let body = body_after_frontmatter(text).trim();
    (!body.is_empty()).then(|| Rule {
        origin,
        text: body.to_string(),
        scope,
    })
}

/// What a frontmatter block says about the files a rule is for.
#[derive(Debug, PartialEq, Eq)]
enum Paths {
    /// No `paths` key.
    Absent,
    /// The globs the key lists, as they are written. None at all for `paths: []`.
    Listed(Vec<String>),
    /// A `[` list that is never closed, or a value that is not a list of globs.
    Unreadable,
}

/// The globs a frontmatter block's `paths` key lists.
///
/// Three spellings, since a file written for another agent uses whichever its author's editor
/// produced: a list with one `- glob` item a line, a `[a, b]` list that may run over several lines,
/// and one glob or a comma-separated run of them after the colon. A comma inside `{}` or quotes
/// belongs to the glob, a `]` closes a `[` list only where no `[` inside it is open, and a `#`
/// after whitespace begins a comment. The last `paths` key wins, as the last of any key does in
/// [`crate::skills::declarations`].
fn paths_declared(block: &[&str]) -> Paths {
    let mut declared = Paths::Absent;
    for (at, line) in block.iter().enumerate() {
        if indent_of(line) != 0 {
            continue;
        }
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        if key.trim() != "paths" {
            continue;
        }
        let rest = without_comment(rest).trim();
        let globs = match rest {
            "" => Some(list_items(&block[at + 1..])),
            "~" | "null" | "Null" | "NULL" => Some(Vec::new()),
            _ if rest.starts_with(['|', '>']) => None,
            _ => match rest.strip_prefix('[') {
                Some(inner) => flow_list(inner, &block[at + 1..]).map(|list| split_globs(&list)),
                None => Some(split_globs(rest)),
            },
        };
        declared = match globs {
            Some(globs) => Paths::Listed(
                globs
                    .into_iter()
                    .map(|glob| glob.trim().to_string())
                    .filter(|glob| !glob.is_empty())
                    .collect(),
            ),
            None => Paths::Unreadable,
        };
    }
    declared
}

/// What a `[` list holds, up to the `]` that closes it, which may be on a later line. `None` where
/// nothing does before the next key at the left margin.
fn flow_list(first: &str, following: &[&str]) -> Option<String> {
    let mut list = String::new();
    let mut depth = 1usize;
    let mut quote: Option<char> = None;
    let mut before = '[';
    for (n, line) in std::iter::once(first)
        .chain(following.iter().copied())
        .enumerate()
    {
        if n > 0 && indent_of(line) == 0 && looks_like_a_key(line) {
            return None;
        }
        for character in without_comment(line).chars() {
            match (quote, character) {
                (Some(open), c) if c == open => quote = None,
                (None, '"' | '\'') if opens_a_quote(before) => quote = Some(character),
                (None, '[') => depth += 1,
                (None, ']') => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(list);
                    }
                }
                _ => {}
            }
            list.push(character);
            before = character;
        }
        list.push(' ');
        before = ' ';
    }
    None
}

/// Whether a quote after `before` opens a quoted value, as it does where a value starts and not
/// inside a word: the apostrophe of `don't/**` is part of the glob.
fn opens_a_quote(before: char) -> bool {
    before.is_whitespace() || before == '[' || before == ','
}

/// Whether `line` opens with a `name:` as a frontmatter key does.
fn looks_like_a_key(line: &str) -> bool {
    line.split_once(':').is_some_and(|(name, _)| {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

/// The `- item` lines under a key, which YAML lets sit at the key's own indent or deeper. Ends at
/// the next line that is neither blank, a comment nor an item.
fn list_items(lines: &[&str]) -> Vec<String> {
    let mut items = Vec::new();
    for line in lines {
        let line = without_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        let Some(item) = line.strip_prefix('-') else {
            break;
        };
        items.push(unquoted(item.trim()));
    }
    items
}

/// `text` up to a `#` that begins a comment: one at the start or after whitespace, outside quotes.
fn without_comment(text: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut before = ' ';
    for (at, character) in text.char_indices() {
        match (quote, character) {
            (Some(open), c) if c == open => quote = None,
            (None, '"' | '\'') if opens_a_quote(before) => quote = Some(character),
            (None, '#') if before.is_whitespace() => return &text[..at],
            _ => {}
        }
        before = character;
    }
    text
}

/// `list` cut at the commas that are not inside braces or quotes, each piece without its quotes.
fn split_globs(list: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut before = ',';
    for character in list.chars() {
        match (quote, character) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') if opens_a_quote(before) => quote = Some(character),
            (None, '{') => depth += 1,
            (None, '}') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                pieces.push(unquoted(piece.trim()));
                piece.clear();
                before = ',';
                continue;
            }
            (None, _) => {}
        }
        piece.push(character);
        before = character;
    }
    pieces.push(unquoted(piece.trim()));
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(text: &str) -> Paths {
        match frontmatter_lines(text) {
            Block::Lines(block) => paths_declared(&block),
            _ => panic!("the fixture has frontmatter"),
        }
    }

    fn listed(globs: &[&str]) -> Paths {
        Paths::Listed(globs.iter().map(|glob| glob.to_string()).collect())
    }

    /// A list with one item a line is the spelling both Cline and Claude Code document.
    #[test]
    fn a_block_list_names_one_glob_a_line() {
        let text = "---\npaths:\n  - \"src/**/*.ts\"\n  - '*.md'\n  - docs/**\n---\nbody\n";
        assert_eq!(paths(text), listed(&["src/**/*.ts", "*.md", "docs/**"]));
    }

    /// YAML lets the dashes sit at the key's own indent, and a list ends at the next key.
    #[test]
    fn a_list_at_the_keys_own_indent_ends_at_the_next_key() {
        let text = "---\npaths:\n- a/*.rs\n- b/*.rs\ndescription: x\n- not/a/path\n---\nbody\n";
        assert_eq!(paths(text), listed(&["a/*.rs", "b/*.rs"]));
    }

    /// A comma inside a brace group is the group's, in a list on the key's line and in a bare one.
    #[test]
    fn a_comma_inside_braces_or_quotes_belongs_to_the_glob() {
        assert_eq!(
            paths("---\npaths: [\"src/**/*.{ts,tsx}\", 'a,b.md', docs/*.md]\n---\nbody\n"),
            listed(&["src/**/*.{ts,tsx}", "a,b.md", "docs/*.md"])
        );
        assert_eq!(
            paths("---\npaths: src/**/*.{ts,tsx}, tests/**\n---\nbody\n"),
            listed(&["src/**/*.{ts,tsx}", "tests/**"])
        );
    }

    /// An empty list names no file, which is not the same as naming every file: a rule with
    /// `paths: []` never loads, and one with no `paths` key at all always does.
    #[test]
    fn an_empty_list_names_no_file_and_no_key_names_every_turn() {
        assert_eq!(paths("---\npaths: []\n---\nbody\n"), listed(&[]));
        assert_eq!(paths("---\npaths:\n---\nbody\n"), listed(&[]));
        assert_eq!(paths("---\ndescription: x\n---\nbody\n"), Paths::Absent);
    }

    /// A comment after a glob is not part of it, and a `#` inside a glob or quotes is.
    #[test]
    fn a_comment_after_a_glob_is_not_part_of_it() {
        assert_eq!(
            paths("---\npaths: [\"a/**\", \"b/**\"]  # the api\n---\nbody\n"),
            listed(&["a/**", "b/**"])
        );
        assert_eq!(
            paths("---\npaths:\n  # first\n  - \"a/**\"  # one\n  - b/#c\n---\nbody\n"),
            listed(&["a/**", "b/#c"])
        );
        assert_eq!(
            paths("---\npaths: src/**  # x\n---\nbody\n"),
            listed(&["src/**"])
        );
    }

    /// A `[` list may run over several lines, which is how a long one is written.
    #[test]
    fn a_list_in_brackets_may_run_over_several_lines() {
        let text = "---\npaths: [\n  \"a/**\",  # one\n  \"b/**\",\n]\n---\nbody\n";
        assert_eq!(paths(text), listed(&["a/**", "b/**"]));
    }

    /// Only a key at the left margin is the key: a `paths:` inside an indented value is text.
    #[test]
    fn an_indented_paths_line_is_not_the_key() {
        let text = "---\ndescription: |\n  paths: [x]\n---\nbody\n";
        assert_eq!(paths(text), Paths::Absent);
    }

    /// A `]` inside a glob does not close the list, which is how a directory named for a route
    /// parameter is written. A list that is never closed is not read as the globs up to the next key.
    #[test]
    fn a_bracket_inside_a_glob_does_not_close_the_list() {
        assert_eq!(
            paths("---\npaths: [\"app/[slug]/**\", \"b/**\"]\n---\nbody\n"),
            listed(&["app/[slug]/**", "b/**"])
        );
        assert_eq!(
            paths("---\npaths: [app/[id]/page.tsx, c/**]\n---\nbody\n"),
            listed(&["app/[id]/page.tsx", "c/**"])
        );
        assert_eq!(
            paths("---\npaths: [a/**, b/**\ndescription: foo\n---\nbody\n"),
            Paths::Unreadable
        );
        assert_eq!(
            paths("---\npaths: [\n  a/**\n]\n---\nbody\n"),
            listed(&["a/**"])
        );
    }

    /// A rule is read from the first line that is not blank or a byte order mark, and a file whose
    /// list or frontmatter never closes is left out and not read as text for every turn.
    #[test]
    fn a_byte_order_mark_or_blank_lines_before_the_frontmatter_are_not_part_of_the_file() {
        for text in [
            "\u{feff}---\npaths: [\"a/**\"]\n---\nBODY\n",
            "\n\n---\npaths: [\"a/**\"]\n---\nBODY\n",
        ] {
            let mut notices = Vec::new();
            let rule =
                admit(text, ".bravebot/rules/r.md".to_string(), &mut notices).expect("a rule");
            assert_eq!(rule.text, "BODY");
            assert!(matches!(rule.scope, Scope::Paths(ref globs) if globs.len() == 1));
            assert!(notices.is_empty());
        }
        for text in [
            "---\npaths: [\"a/**\"\ndescription: x\n---\nBODY\n",
            "---\npaths: [\"a/**\"]\nBODY, never closed\n",
        ] {
            let mut notices = Vec::new();
            assert!(admit(text, ".bravebot/rules/r.md".to_string(), &mut notices).is_none());
            assert_eq!(notices.len(), 1, "{text}");
        }
    }

    fn held(text: &str) -> Held {
        let rule = admit(text, ".bravebot/rules/r.md".to_string(), &mut Vec::new())
            .expect("the fixture says something");
        Held {
            rules: vec![rule],
            ..Held::default()
        }
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    /// A glob with a separator is read against the whole name from the root, and one without is
    /// read against the file's own name at any depth, as `list_files` reads them.
    #[test]
    fn a_glob_with_a_separator_anchors_and_a_bare_one_matches_at_any_depth() {
        let mut anchored = held("---\npaths: [src/*.rs]\n---\nbody\n");
        assert!(
            anchored
                .take(&names(&["crates/agent/src/lib.rs", "src/lib/mod.rs"]), &[])
                .is_empty()
        );
        assert_eq!(anchored.take(&names(&["src/lib.rs"]), &[]).len(), 1);

        let mut bare = held("---\npaths: [\"*.rs\"]\n---\nbody\n");
        assert_eq!(
            bare.take(&names(&["crates/agent/src/lib.rs"]), &[])[0].matched,
            "crates/agent/src/lib.rs"
        );
    }

    /// A glob written with a leading `/` or `./` is about the project root's own files. Taking the
    /// prefix off and leaving a bare name would match the same name in every directory.
    #[test]
    fn a_glob_written_from_the_root_is_about_the_roots_own_files() {
        let loaded = |written: &str, name: &str| {
            held(&format!("---\npaths: [\"{written}\"]\n---\nbody\n"))
                .take(&names(&[name]), &[])
                .len()
        };
        for written in ["/Cargo.toml", "./Cargo.toml"] {
            assert_eq!(loaded(written, "Cargo.toml"), 1, "{written}");
            assert_eq!(loaded(written, "crates/foo/Cargo.toml"), 0, "{written}");
        }
        assert_eq!(loaded("./*.md", "README.md"), 1);
        assert_eq!(loaded("./*.md", "docs/README.md"), 0);
        assert_eq!(loaded("/{a,b}.rs", "b.rs"), 1);
        assert_eq!(loaded("/{a,b}.rs", "src/b.rs"), 0);
        assert_eq!(loaded("/**", "src/deep/lib.rs"), 1);
        assert_eq!(loaded("./src/*.rs", "src/lib.rs"), 1);
        assert_eq!(loaded("./src/*.rs", "crates/src/lib.rs"), 0);
        assert_eq!(loaded("/", "src/lib.rs"), 0);
    }

    /// A rule is added once, with the first name that matched it, and brings the heading only the
    /// first time anything is added.
    #[test]
    fn a_rule_is_taken_once_and_only_the_first_carries_the_heading() {
        let mut held = Held {
            rules: ["a", "b"]
                .into_iter()
                .map(|name| Rule {
                    origin: format!(".bravebot/rules/{name}.md"),
                    text: name.to_string(),
                    scope: Scope::Paths(vec![Glob::new(&format!("{name}/**")).unwrap()]),
                })
                .collect(),
            ..Held::default()
        };
        assert!(held.take(&names(&["c/x"]), &[]).is_empty());
        let first = held.take(&names(&["a/x", "a/y"]), &[]);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].matched, "a/x");
        assert!(first[0].introduces);
        assert!(held.take(&names(&["a/x"]), &[]).is_empty());
        let second = held.take(&names(&["b/z"]), &[]);
        assert_eq!(second.len(), 1);
        assert!(!second[0].introduces);
        assert!(held.is_empty());
    }

    /// Rules the session added before come first and in the order it added them, whatever order
    /// the files that matched them sort in, so a turn that starts later carries them as the turn
    /// that added them did. Only the first of them brings the heading.
    #[test]
    fn rules_added_before_keep_the_order_they_were_added_in() {
        let rule = |name: &str| Rule {
            origin: format!(".bravebot/rules/{name}.md"),
            text: name.to_string(),
            scope: Scope::Paths(vec![Glob::new(&format!("{name}/**")).unwrap()]),
        };
        let mut held = Held {
            rules: vec![rule("a"), rule("b"), rule("c")],
            ..Held::default()
        };

        let loaded = held.take(
            &names(&["a/x", "b/x", "c/x"]),
            &names(&[".bravebot/rules/c.md", ".bravebot/rules/a.md"]),
        );

        let order: Vec<&str> = loaded.iter().map(|rule| rule.text.as_str()).collect();
        assert_eq!(order, ["c", "a", "b"]);
        assert_eq!(
            loaded
                .iter()
                .map(|rule| rule.introduces)
                .collect::<Vec<_>>(),
            [true, false, false]
        );
    }

    /// `paths` is a list. A value that is a block scalar names globs nobody can read out of it, so
    /// the rule is left out, and `null` is the empty list a bare `paths:` is.
    #[test]
    fn a_value_that_is_not_a_list_is_unreadable_and_null_names_no_file() {
        for text in [
            "---\npaths: |\n  a/**\n---\nbody\n",
            "---\npaths: >-\n  a/**\n---\nbody\n",
        ] {
            assert_eq!(paths(text), Paths::Unreadable, "{text}");
        }
        for value in ["null", "~", "Null"] {
            assert_eq!(
                paths(&format!("---\npaths: {value}\n---\nbody\n")),
                listed(&[]),
                "{value}"
            );
        }
    }

    /// A glob ending in a slash is a directory, which is everything below it, and it is not a
    /// name no file has.
    #[test]
    fn a_glob_ending_in_a_slash_names_what_is_below_it() {
        let loaded = |written: &str, name: &str| {
            held(&format!("---\npaths: [\"{written}\"]\n---\nbody\n"))
                .take(&names(&[name]), &[])
                .len()
        };
        assert_eq!(loaded("docs/", "docs/a.md"), 1);
        assert_eq!(loaded("docs/", "docs/deep/b.md"), 1);
        assert_eq!(loaded("docs/", "src/docs/a.md"), 0);
        assert_eq!(loaded("/docs/", "docs/a.md"), 1);
    }

    /// A quote opens a quoted value where a value starts, and inside a word it is a character of the
    /// word: the glob of a directory named `don't` is not a quote with no end.
    #[test]
    fn a_quote_inside_a_word_is_part_of_the_glob() {
        assert_eq!(
            paths("---\npaths:\n  - don't/**  # x\n---\nbody\n"),
            listed(&["don't/**"])
        );
        assert_eq!(
            paths("---\npaths: [it's/**, b/**]\n---\nbody\n"),
            listed(&["it's/**", "b/**"])
        );
        assert_eq!(
            paths("---\npaths: it's/**, 'b, c/**'\n---\nbody\n"),
            listed(&["it's/**", "b, c/**"])
        );
    }

    /// An anchor belongs to each alternative of a brace group, not to the glob as written:
    /// `/{src/a,b}.rs` is `src/a.rs` and the root's own `b.rs`.
    #[test]
    fn an_anchor_is_decided_for_each_alternative_of_a_brace_group() {
        let loaded = |name: &str| {
            held("---\npaths: [\"/{src/a,b}.rs\"]\n---\nbody\n")
                .take(&names(&[name]), &[])
                .len()
        };
        assert_eq!(loaded("src/a.rs"), 1);
        assert_eq!(loaded("b.rs"), 1);
        assert_eq!(loaded("x/b.rs"), 0);
        assert_eq!(loaded("deep/dir/b.rs"), 0);
    }
}
