//! The declarations a source file makes, and a ranked map of them (MAP-1..MAP-6).
//!
//! Everything here is a function of text the caller has already decided it may read. Nothing in
//! this module consults the trust map or opens a file, so it cannot be handed a file by mistake and
//! cannot decide what is vouched for.
//!
//! The scanner is line based and hand written, like the glob and regex matchers: no parser, no
//! backtracking, work proportional to the text. It finds the declaration a line opens and nothing
//! about what follows it, so a signature split over lines is shown by its first line.

use std::collections::{HashMap, HashSet};

/// Tokens asked for when a call names none.
pub const DEFAULT_BUDGET: usize = 1_000;
/// The least a call may ask for, so a map is never a handful of names.
pub const MIN_BUDGET: usize = 100;
/// The most a call may ask for, since the result is paid for in every later round.
pub const MAX_BUDGET: usize = 8_000;
/// Characters standing for one token, the rough measure used everywhere a budget is kept.
const CHARS_PER_TOKEN: usize = 4;
/// The longest declaration shown, in bytes.
const MAX_HEAD: usize = 120;
/// The longest name that counts as a declaration. Longer is generated code.
const MAX_NAME: usize = 80;

/// The language a file is read as, from its extension alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    Script,
    Go,
    Jvm,
    CFamily,
}

/// The extensions a map reads, spelled as the globs a walk selects by.
pub const EXTENSIONS: [&str; 22] = [
    "rs", "py", "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "go", "java", "kt", "kts",
    "c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx",
];

impl Language {
    /// The language of `path`, or `None` for a file a map does not read.
    pub fn of(path: &str) -> Option<Self> {
        let extension = path.rsplit_once('.')?.1;
        Some(match extension {
            "rs" => Self::Rust,
            "py" => Self::Python,
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => Self::Script,
            "go" => Self::Go,
            "java" | "kt" | "kts" => Self::Jvm,
            "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Self::CFamily,
            _ => return None,
        })
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &[
                "fn", "struct", "enum", "trait", "type", "mod", "const", "static",
            ],
            Self::Python => &["def", "class"],
            Self::Script => &[
                "function",
                "class",
                "interface",
                "enum",
                "type",
                "namespace",
                "const",
                "let",
                "var",
            ],
            Self::Go => &[],
            Self::Jvm => &[
                "class",
                "interface",
                "enum",
                "record",
                "object",
                "fun",
                "trait",
                "def",
            ],
            Self::CFamily => &["class", "struct", "enum", "union", "namespace"],
        }
    }

    fn modifiers(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["pub", "unsafe", "async", "extern", "default"],
            Self::Python => &["async"],
            Self::Script => &[
                "export",
                "default",
                "async",
                "declare",
                "abstract",
                "public",
                "private",
                "protected",
                "static",
            ],
            Self::Go => &[],
            Self::Jvm => &[
                "public",
                "private",
                "protected",
                "static",
                "final",
                "abstract",
                "open",
                "sealed",
                "data",
                "internal",
                "override",
                "suspend",
                "inline",
                "annotation",
                "value",
                "case",
            ],
            Self::CFamily => &["typedef"],
        }
    }
}

/// One declaration: the name it introduces, where, and the line that opens it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    /// 1-based.
    pub line: usize,
    /// The declaration's opening line, trimmed and cut where its body or value begins.
    pub head: String,
}

/// The declarations in `text`, in the order they appear.
pub fn symbols(language: Language, text: &str) -> Vec<Symbol> {
    let mut found = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let Some(name) = declared(language, line) else {
            continue;
        };
        if name.len() > MAX_NAME {
            continue;
        }
        found.push(Symbol {
            name: name.to_string(),
            line: index + 1,
            head: head_of(line),
        });
    }
    found
}

/// The name a line declares, if it opens a declaration.
fn declared(language: Language, line: &str) -> Option<&str> {
    match language {
        Language::Go => return go_name(line),
        Language::CFamily => {
            if let Some(rest) = line.strip_prefix("#define ") {
                return non_empty(leading_identifier(rest.trim_start()));
            }
            // A forward declaration names a type and declares nothing about it.
            if line.ends_with(';') && !line.contains('{') {
                return None;
            }
        }
        Language::Rust => {
            if let Some(rest) = line.strip_prefix("macro_rules!") {
                return non_empty(leading_identifier(rest.trim_start()));
            }
        }
        _ => {}
    }

    let keywords = language.keywords();
    let modifiers = language.modifiers();
    let mut exported = false;
    let mut in_visibility = false;
    let mut tokens = line.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        if in_visibility {
            in_visibility = !token.contains(')');
            continue;
        }
        let word = token_word(token);
        if token.starts_with('"') || token.starts_with("pub(") || modifiers.contains(&word) {
            exported |= word == "export" || word == "pub";
            // `pub(in some::path)` is a visibility spread over two tokens.
            in_visibility = token.starts_with("pub(") && !token.contains(')');
            continue;
        }
        if !keywords.contains(&word) {
            return None;
        }
        let mut next = tokens.peek().copied()?;
        if language == Language::Rust && word == "static" && next == "mut" {
            tokens.next();
            next = tokens.peek().copied()?;
        }
        let next_word = token_word(next);
        // `const fn`, `enum class`, `type alias` style runs: the first word was a modifier.
        if keywords.contains(&next_word) || modifiers.contains(&next_word) {
            continue;
        }
        // A binding in a script is a declaration worth a line only when it is exported; inside a
        // function it is a local.
        if language == Language::Script && matches!(word, "const" | "let" | "var") && !exported {
            return None;
        }
        return non_empty(leading_identifier(next));
    }
    None
}

/// `func Name(` and `func (r T) Name(`, and `type Name`, at the start of a line.
fn go_name(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix("func ") {
        let rest = rest.trim_start();
        let rest = match rest.strip_prefix('(') {
            Some(receiver) => receiver[receiver.find(')')? + 1..].trim_start(),
            None => rest,
        };
        return non_empty(leading_identifier(rest));
    }
    non_empty(leading_identifier(line.strip_prefix("type ")?.trim_start()))
}

fn non_empty(name: &str) -> Option<&str> {
    (!name.is_empty()).then_some(name)
}

/// The identifier a token begins with, which for `pub(crate)` is `pub` and for `function*` is
/// `function`.
fn token_word(token: &str) -> &str {
    let word = leading_identifier(token);
    if word.len() == token.len() {
        return word;
    }
    match &token[word.len()..] {
        "*" | "(" | "<" => word,
        rest if word == "pub" && rest.starts_with('(') => word,
        _ => "",
    }
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

fn leading_identifier(text: &str) -> &str {
    let end = text
        .bytes()
        .position(|byte| !is_identifier_byte(byte))
        .unwrap_or(text.len());
    let word = &text[..end];
    if word.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        ""
    } else {
        word
    }
}

fn head_of(line: &str) -> String {
    let mut head = line;
    if let Some(at) = head.find('{') {
        head = &head[..at];
    }
    if let Some(at) = head.find(" = ")
        && !head[..at].contains('(')
    {
        head = &head[..at];
    }
    let mut head = head.trim_end().to_string();
    crate::workspace::truncate_on_char_boundary(&mut head, MAX_HEAD);
    head
}

/// Every identifier in `text`, each as often as it occurs.
pub fn identifiers(text: &str) -> impl Iterator<Item = &str> {
    let bytes = text.as_bytes();
    let mut at = 0;
    std::iter::from_fn(move || {
        while at < bytes.len() {
            if !is_identifier_byte(bytes[at]) {
                at += 1;
                continue;
            }
            let start = at;
            while at < bytes.len() && is_identifier_byte(bytes[at]) {
                at += 1;
            }
            if !bytes[start].is_ascii_digit() {
                return Some(&text[start..at]);
            }
        }
        None
    })
}

/// A file a map is built from.
pub struct Source<'a> {
    pub path: &'a str,
    pub text: &'a str,
}

/// A map and what it left out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Built {
    /// Files with the declarations chosen for them, most referenced first.
    pub body: String,
    /// Declarations found in all.
    pub found: usize,
    /// Declarations the body shows.
    pub shown: usize,
}

/// Rank the declarations in `sources` by how many other files mention their name, and show as
/// many from the top as `budget` tokens hold.
///
/// A name is credited once per file that mentions it and does not declare it, and a name declared
/// in several places shares that credit between them, so `new` and `main` do not outrank what a
/// reader came to find. Ties fall to path then line, which makes the map the same every time.
pub fn build(sources: &[Source<'_>], budget: usize) -> Built {
    let budget = budget.clamp(MIN_BUDGET, MAX_BUDGET) * CHARS_PER_TOKEN;

    let scanned: Vec<(&Source<'_>, Vec<Symbol>)> = sources
        .iter()
        .filter_map(|source| {
            let language = Language::of(source.path)?;
            Some((source, symbols(language, source.text)))
        })
        .collect();

    let mut declared_in: HashMap<&str, usize> = HashMap::new();
    for (_, found) in &scanned {
        for symbol in found {
            *declared_in.entry(symbol.name.as_str()).or_default() += 1;
        }
    }

    let mut mentions: HashMap<&str, u64> = HashMap::new();
    for (source, own) in &scanned {
        let own: HashSet<&str> = own.iter().map(|symbol| symbol.name.as_str()).collect();
        let mut seen: HashSet<&str> = HashSet::new();
        for word in identifiers(source.text) {
            if declared_in.contains_key(word) && !own.contains(word) && seen.insert(word) {
                *mentions.entry(word).or_default() += 1;
            }
        }
    }

    struct Candidate<'a> {
        score: u64,
        path: &'a str,
        symbol: &'a Symbol,
    }
    let mut candidates: Vec<Candidate<'_>> = scanned
        .iter()
        .flat_map(|(source, found)| {
            found.iter().map(|symbol| Candidate {
                score: mentions.get(symbol.name.as_str()).copied().unwrap_or(0) * 1024
                    / declared_in[symbol.name.as_str()] as u64,
                path: source.path,
                symbol,
            })
        })
        .collect();
    let found = candidates.len();
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.cmp(b.path))
            .then_with(|| a.symbol.line.cmp(&b.symbol.line))
    });

    let mut spent = 0usize;
    let mut chosen: Vec<&Candidate<'_>> = Vec::new();
    let mut opened: HashSet<&str> = HashSet::new();
    for candidate in &candidates {
        let header = if opened.contains(candidate.path) {
            0
        } else {
            candidate.path.len() + 2
        };
        let cost = header + line_for(candidate.symbol).len() + 1;
        if spent + cost > budget {
            continue;
        }
        spent += cost;
        opened.insert(candidate.path);
        chosen.push(candidate);
    }

    let mut by_file: HashMap<&str, (u64, Vec<&Symbol>)> = HashMap::new();
    for candidate in &chosen {
        let entry = by_file.entry(candidate.path).or_default();
        entry.0 += candidate.score;
        entry.1.push(candidate.symbol);
    }
    let mut files: Vec<(&str, u64, Vec<&Symbol>)> = by_file
        .into_iter()
        .map(|(path, (score, mut symbols))| {
            symbols.sort_by_key(|symbol| symbol.line);
            (path, score, symbols)
        })
        .collect();
    files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    let mut body = String::new();
    for (path, _, symbols) in &files {
        body.push_str(path);
        body.push_str(":\n");
        for symbol in symbols {
            body.push_str(&line_for(symbol));
            body.push('\n');
        }
    }
    if body.ends_with('\n') {
        body.pop();
    }

    Built {
        body,
        found,
        shown: chosen.len(),
    }
}

fn line_for(symbol: &Symbol) -> String {
    format!("  {}: {}", symbol.line, symbol.head)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(language: Language, text: &str) -> Vec<String> {
        symbols(language, text)
            .into_iter()
            .map(|symbol| symbol.name)
            .collect()
    }

    #[test]
    fn rust_items_are_declarations_whatever_precedes_the_keyword() {
        let text = "\
pub fn plain() {}
pub(crate) async fn qualified() {}
pub const fn constant() {}
const LIMIT: usize = 4;
extern \"C\" fn foreign() {}
pub struct Thing {
macro_rules! shout {
    unsafe fn inner() {}
// fn commented() {}
let fn_like = 1;
static mut COUNTER: u8 = 0;
pub(in crate::a) fn scoped() {}
";
        assert_eq!(
            names(Language::Rust, text),
            [
                "plain",
                "qualified",
                "constant",
                "LIMIT",
                "foreign",
                "Thing",
                "shout",
                "inner",
                "COUNTER",
                "scoped"
            ]
        );
    }

    #[test]
    fn a_head_stops_where_the_body_or_value_begins() {
        let found = symbols(
            Language::Rust,
            "pub fn run(a: u8) -> u8 {\npub const KEY: &str = \"value\";\n",
        );
        assert_eq!(found[0].head, "pub fn run(a: u8) -> u8");
        assert_eq!(found[1].head, "pub const KEY: &str");
        assert_eq!(found[1].line, 2);
    }

    #[test]
    fn python_methods_and_classes_are_declarations() {
        let text = "class A:\n    async def go(self):\n        pass\n# def hidden():\nx = 1\n";
        assert_eq!(names(Language::Python, text), ["A", "go"]);
    }

    #[test]
    fn a_script_binding_counts_only_when_exported() {
        let text = "\
export function shown() {}
export default class Widget {}
function* generator() {}
export const exported = 1;
const local = 2;
export interface Props {
type Alias = string;
";
        assert_eq!(
            names(Language::Script, text),
            ["shown", "Widget", "generator", "exported", "Props", "Alias"]
        );
    }

    #[test]
    fn go_methods_are_named_by_the_method_not_the_receiver() {
        let text =
            "func (s *Server) Handle(w W) {\nfunc Plain() {\ntype Conn struct {\nvar x = 1\n";
        assert_eq!(names(Language::Go, text), ["Handle", "Plain", "Conn"]);
    }

    #[test]
    fn a_c_family_forward_declaration_declares_nothing() {
        let text = "\
struct Later;
struct Real {
typedef struct Aliased {
enum class Color {
#define LIMIT 4
class Widget : public Base {
";
        assert_eq!(
            names(Language::CFamily, text),
            ["Real", "Aliased", "Color", "LIMIT", "Widget"]
        );
    }

    #[test]
    fn jvm_enum_class_is_named_by_its_class_not_by_the_word_class() {
        let text = "enum class Mode {\npublic final class Box {\ninterface Shape {\nfun run() {\n";
        assert_eq!(names(Language::Jvm, text), ["Mode", "Box", "Shape", "run"]);
    }

    #[test]
    fn identifiers_skip_numbers_and_punctuation() {
        let words: Vec<&str> = identifiers("a1 + 22 - b_c(d9) 3x").collect();
        assert_eq!(words, ["a1", "b_c", "d9"]);
    }

    fn built(files: &[(&str, &str)], budget: usize) -> Built {
        let sources: Vec<Source<'_>> = files
            .iter()
            .map(|(path, text)| Source { path, text })
            .collect();
        build(&sources, budget)
    }

    #[test]
    fn the_declaration_other_files_mention_comes_first() {
        let map = built(
            &[
                ("a.rs", "pub fn lonely() {}\n"),
                ("d.rs", "pub fn popular() {}\n"),
                ("b.rs", "fn user() { popular(); }\n"),
                ("c.rs", "fn other() { popular(); }\n"),
            ],
            DEFAULT_BUDGET,
        );
        let popular = map.body.find("popular").expect("shown");
        let lonely = map.body.find("lonely").expect("shown");
        assert!(popular < lonely, "{}", map.body);
        assert!(map.body.starts_with("d.rs:"), "{}", map.body);
    }

    #[test]
    fn a_mention_in_the_declaring_file_earns_nothing() {
        let map = built(
            &[
                (
                    "a.rs",
                    "fn inward() {}\nfn call() { inward(); inward(); }\n",
                ),
                ("c.rs", "fn caller() { outward(); }\n"),
                ("z.rs", "fn outward() {}\n"),
            ],
            DEFAULT_BUDGET,
        );
        let outward = map.body.find("fn outward()").expect("shown");
        let inward = map.body.find("fn inward()").expect("shown");
        assert!(outward < inward, "{}", map.body);
    }

    #[test]
    fn a_name_declared_in_many_places_shares_its_credit() {
        let mut files = vec![(
            "user.rs".to_string(),
            "fn user() { new(); target(); }\n".to_string(),
        )];
        for n in 0..4 {
            files.push((format!("m{n}.rs"), "fn new() {}\n".to_string()));
        }
        files.push(("t.rs".to_string(), "fn target() {}\n".to_string()));
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let map = built(&borrowed, DEFAULT_BUDGET);
        let target = map.body.find("fn target()").expect("shown");
        let new = map.body.find("fn new()").expect("shown");
        assert!(target < new, "{}", map.body);
    }

    #[test]
    fn the_budget_bounds_the_body_and_the_count_says_what_it_cut() {
        let text: String = (0..400)
            .map(|n| format!("pub fn function_number_{n}() {{}}\n"))
            .collect();
        let map = built(&[("big.rs", &text)], MIN_BUDGET);
        assert!(
            map.body.len() <= MIN_BUDGET * CHARS_PER_TOKEN,
            "{}",
            map.body.len()
        );
        assert_eq!(map.found, 400);
        assert!(map.shown > 0 && map.shown < 400);
        assert_eq!(map.body.lines().count(), map.shown + 1);
    }

    #[test]
    fn a_tight_budget_keeps_the_declaration_that_ranks_first_not_the_first_in_the_file() {
        let mut text: String = (0..400)
            .map(|n| format!("pub fn function_number_{n}() {{}}\n"))
            .collect();
        text.push_str("pub fn wanted() {}\n");
        let map = built(
            &[("big.rs", &text), ("user.rs", "fn user() { wanted(); }\n")],
            MIN_BUDGET,
        );
        assert!(map.body.contains("fn wanted()"), "{}", map.body);
        assert!(map.shown < map.found);
    }

    #[test]
    fn a_path_too_long_for_the_budget_does_not_empty_the_map() {
        let long = "d/".repeat(300) + "long.rs";
        let map = built(
            &[
                (&long, "pub fn wanted() {}\n"),
                ("user.rs", "fn user() { wanted(); }\n"),
                ("a.rs", "pub fn small() {}\n"),
            ],
            MIN_BUDGET,
        );
        assert!(map.body.contains("fn small()"), "{}", map.body);
        assert!(map.shown < map.found);
    }

    #[test]
    fn the_same_sources_give_the_same_map() {
        let files = [
            ("z.rs", "pub fn zed() {}\n"),
            ("a.rs", "pub fn aye() {}\n"),
            ("m.rs", "pub fn em() { aye(); zed(); }\n"),
        ];
        assert_eq!(built(&files, 500), built(&files, 500));
    }
}
