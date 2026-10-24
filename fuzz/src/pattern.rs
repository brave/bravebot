//! The pattern side of the `regex` target.
//!
//! A generated pattern is a syntax tree of the subset `bravebot_agent::regex` supports. It is
//! rendered to the text the engine compiles and read directly by an evaluator that shares no code
//! with the engine's parser, program or literal shortcuts, so a disagreement is a defect in one of
//! them. `deny.toml` bans the `regex` crate, so the evaluator is written here.
//!
//! The evaluator tracks the set of positions a node can end at instead of backtracking. `(a*)*` and
//! `(a|)*` have empty iterations that a backtracking search has to guard against, and the set
//! reaches a fixed point without a guard. Its cost is polynomial in the text, so the limits below
//! protect the harness from long texts only.

#![forbid(unsafe_code)]

use arbitrary::{Arbitrary, Result, Unstructured};
use std::collections::BTreeSet;

/// The longest pattern the engine compiles. Private in the engine, so restated here: the target
/// fails when the two disagree about where a pattern becomes too long.
pub const MAX_PATTERN: usize = 1_000;

/// How deeply the engine lets groups nest, restated for the same reason.
pub const MAX_DEPTH: usize = 50;

/// Nodes in a generated tree that have children. Each has at most three, so a tree is at most
/// `3 * MAX_NODES + 1` nodes.
pub const MAX_NODES: usize = 12;

/// Characters in a generated text.
pub const MAX_TEXT_CHARS: usize = 16;

/// What generated patterns and texts are made of. Most of it is the engine's own metacharacters,
/// so a pattern spells them with an escape and a text holds them as plain characters. The last
/// four are the ones its case folding treats specially: a letter with a one-character lowercase,
/// the Kelvin sign, whose lowercase is ASCII `k`, and `ß`, whose uppercase is two characters.
const ALPHABET: &[char] = &[
    'a', 'b', 'c', 'k', 'A', 'B', 'K', 'Z', '0', '1', '9', '_', ' ', '\n', '\t', '-', '.', '*',
    '+', '?', '(', ')', '[', ']', '|', '^', '$', '\\', '{', '\u{e9}', '\u{c9}', '\u{212A}',
    '\u{df}',
];

/// A character of the alphabet. The first eight are drawn more often than the rest, so a pattern
/// and a text share characters often enough to match.
fn pick(u: &mut Unstructured<'_>) -> Result<char> {
    let at = u.int_in_range(0..=ALPHABET.len() + 7)?;
    Ok(ALPHABET[if at < ALPHABET.len() {
        at
    } else {
        at - ALPHABET.len()
    }])
}

/// A text for a pattern to be matched against.
#[derive(Debug, Clone)]
pub struct Text(pub String);

impl<'a> Arbitrary<'a> for Text {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        let length = u.int_in_range(0..=MAX_TEXT_CHARS)?;
        let mut text = String::new();
        for _ in 0..length {
            text.push(pick(u)?);
        }
        Ok(Self(text))
    }
}

/// `\d`, `\w` and `\s`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Short {
    Digit,
    Word,
    Space,
}

impl Short {
    fn holds(self, c: char) -> bool {
        match self {
            Self::Digit => c.is_ascii_digit(),
            Self::Word => c.is_alphanumeric() || c == '_',
            Self::Space => c.is_whitespace(),
        }
    }

    fn letter(self, negated: bool) -> char {
        let lower = match self {
            Self::Digit => 'd',
            Self::Word => 'w',
            Self::Space => 's',
        };
        if negated {
            lower.to_ascii_uppercase()
        } else {
            lower
        }
    }

    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(match u.int_in_range(0..=2)? {
            0 => Self::Digit,
            1 => Self::Word,
            _ => Self::Space,
        })
    }
}

/// One member of a bracketed class.
#[derive(Debug, Clone)]
pub enum Item {
    Single(char),
    /// Inclusive, with the first end not above the second.
    Range(char, char),
    Named(Short, bool),
}

impl Item {
    fn holds(&self, c: char) -> bool {
        match self {
            Self::Single(only) => c == *only,
            Self::Range(start, end) => c >= *start && c <= *end,
            Self::Named(named, negated) => named.holds(c) != *negated,
        }
    }
}

/// A pattern as a tree.
#[derive(Debug, Clone)]
pub enum Re {
    Lit(char),
    Any,
    Class {
        negated: bool,
        items: Vec<Item>,
    },
    Short(Short, bool),
    Cat(Vec<Re>),
    Alt(Vec<Re>),
    Star(Box<Re>),
    Plus(Box<Re>),
    Opt(Box<Re>),
    Group(Box<Re>),
    /// `(?i:...)` or `(?-i:...)`.
    Fold(bool, Box<Re>),
    /// `(?i)` or `(?-i)`, which changes case folding for the rest of the group around it.
    Flag(bool),
    Start,
    End,
    Boundary(bool),
}

impl<'a> Arbitrary<'a> for Re {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        let mut budget = MAX_NODES;
        Self::grow(u, &mut budget)
    }
}

impl Re {
    /// A tree of at most `budget` more nodes. The first alternative is a leaf, which is also what
    /// an exhausted input chooses, so a short input stops growing.
    fn grow(u: &mut Unstructured<'_>, budget: &mut usize) -> Result<Self> {
        if *budget == 0 {
            return Ok(Self::Lit(pick(u)?));
        }
        *budget -= 1;
        Ok(match u.int_in_range(0..=17)? {
            0..=3 => Self::Lit(pick(u)?),
            4 => Self::Any,
            5 => Self::class(u)?,
            6 => Self::Short(Short::arbitrary(u)?, u.arbitrary()?),
            7 | 8 => Self::Cat(Self::children(u, budget)?),
            9 => Self::Alt(Self::children(u, budget)?),
            10 => Self::Star(Box::new(Self::grow(u, budget)?)),
            11 => Self::Plus(Box::new(Self::grow(u, budget)?)),
            12 => Self::Opt(Box::new(Self::grow(u, budget)?)),
            13 => Self::Group(Box::new(Self::inside(u, budget)?)),
            14 => Self::Fold(u.arbitrary()?, Box::new(Self::inside(u, budget)?)),
            15 => Self::Flag(u.arbitrary()?),
            16 => {
                if u.arbitrary()? {
                    Self::Start
                } else {
                    Self::End
                }
            }
            _ => Self::Boundary(u.arbitrary()?),
        })
    }

    /// What a group holds. A third of the time it begins with a flag, since where a flag stops
    /// is the thing a group decides, and a flag followed by a sibling after the group is a
    /// combination that random trees almost never build.
    fn inside(u: &mut Unstructured<'_>, budget: &mut usize) -> Result<Self> {
        let inner = Self::grow(u, budget)?;
        if *budget > 0 && u.ratio(1, 3)? {
            *budget -= 1;
            return Ok(Self::Cat(vec![Self::Flag(u.arbitrary()?), inner]));
        }
        Ok(inner)
    }

    fn children(u: &mut Unstructured<'_>, budget: &mut usize) -> Result<Vec<Self>> {
        let count = u.int_in_range(2..=3)?;
        (0..count).map(|_| Self::grow(u, budget)).collect()
    }

    fn class(u: &mut Unstructured<'_>) -> Result<Self> {
        let count = u.int_in_range(1..=4)?;
        let mut items = Vec::new();
        for _ in 0..count {
            items.push(match u.int_in_range(0..=3)? {
                0 | 1 => Item::Single(pick(u)?),
                2 => {
                    let (a, b) = (pick(u)?, pick(u)?);
                    Item::Range(a.min(b), a.max(b))
                }
                _ => Item::Named(Short::arbitrary(u)?, u.arbitrary()?),
            });
        }
        Ok(Self::Class {
            negated: u.arbitrary()?,
            items,
        })
    }
}

/// What the evaluator reads: the tree with case folding settled for each character and class, and
/// the groups that only scope a flag removed.
#[derive(Debug, Clone)]
pub enum Node {
    Empty,
    Char(char, bool),
    Any,
    Class {
        negated: bool,
        folded: bool,
        items: Vec<Item>,
    },
    Cat(Vec<Node>),
    Alt(Vec<Node>),
    Star(Box<Node>),
    Plus(Box<Node>),
    Opt(Box<Node>),
    Start,
    End,
    Boundary(bool),
}

/// A pattern as the engine reads it, and what the engine should do with it.
#[derive(Debug)]
pub struct Built {
    pub text: String,
    pub expected: Expected,
}

#[derive(Debug)]
pub enum Expected {
    /// More than [`MAX_PATTERN`] characters, which is checked before anything is parsed.
    TooLong { length: usize },
    /// A group opened past [`MAX_DEPTH`] levels deep.
    TooDeep,
    /// A pattern that compiles, with the tree to match by.
    Compiles(Node),
}

/// Render `re` wrapped in `nest` groups and followed by `pad` literal `a`s, and work out what the
/// engine should make of it. `folded` is whether the pattern is compiled with `compile_folded`.
pub fn build(re: &Re, nest: usize, pad: usize, folded: bool) -> Built {
    let mut root = re.clone();
    if pad > 0 {
        // Grouped so that padding after a top-level alternation does not join its last branch.
        let mut parts = vec![Re::Group(Box::new(root))];
        parts.extend((0..pad).map(|_| Re::Lit('a')));
        root = Re::Cat(parts);
    }
    for _ in 0..nest {
        root = Re::Group(Box::new(root));
    }

    let mut builder = Builder {
        out: String::new(),
        fold: folded,
        depth: 0,
        deepest: 0,
    };
    let node = builder.alternation(&root);

    let length = builder.out.chars().count();
    let expected = if length > MAX_PATTERN {
        Expected::TooLong { length }
    } else if builder.deepest > MAX_DEPTH {
        Expected::TooDeep
    } else {
        Expected::Compiles(node)
    };
    Built {
        text: builder.out,
        expected,
    }
}

/// Writes the pattern and builds the tree the engine should read from it in one walk, so the two
/// cannot disagree about where a group or a flag begins and ends.
struct Builder {
    out: String,
    /// Whether the next character or class ignores case.
    fold: bool,
    depth: usize,
    deepest: usize,
}

impl Builder {
    /// `re` where an alternation needs no parentheses: the whole pattern, or the inside of a group.
    fn alternation(&mut self, re: &Re) -> Node {
        match re {
            Re::Alt(branches) if branches.len() >= 2 => {
                let mut nodes = Vec::new();
                for (at, branch) in branches.iter().enumerate() {
                    if at > 0 {
                        self.out.push('|');
                    }
                    nodes.push(self.element(branch));
                }
                Node::Alt(nodes)
            }
            Re::Alt(branches) => branches
                .first()
                .map_or(Node::Empty, |only| self.element(only)),
            other => self.element(other),
        }
    }

    /// `re` as one item of a sequence.
    fn element(&mut self, re: &Re) -> Node {
        match re {
            Re::Lit(c) => {
                self.character(*c, "\\.*+?()[]|^$");
                Node::Char(*c, self.fold)
            }
            Re::Any => {
                self.out.push('.');
                Node::Any
            }
            Re::Class { negated, items } => {
                self.out.push('[');
                if *negated {
                    self.out.push('^');
                }
                for item in items {
                    match item {
                        Item::Single(c) => self.member(*c),
                        Item::Range(start, end) => {
                            self.member(*start);
                            self.out.push('-');
                            self.member(*end);
                        }
                        Item::Named(named, negated) => {
                            self.out.push('\\');
                            self.out.push(named.letter(*negated));
                        }
                    }
                }
                self.out.push(']');
                Node::Class {
                    negated: *negated,
                    folded: self.fold,
                    items: items.clone(),
                }
            }
            Re::Short(named, negated) => {
                self.out.push('\\');
                self.out.push(named.letter(*negated));
                Node::Class {
                    negated: false,
                    folded: self.fold,
                    items: vec![Item::Named(*named, *negated)],
                }
            }
            Re::Cat(items) => {
                let nodes: Vec<Node> = items.iter().map(|item| self.element(item)).collect();
                match nodes.len() {
                    0 => Node::Empty,
                    _ => Node::Cat(nodes),
                }
            }
            Re::Alt(_) => self.group("(?:", re, None),
            Re::Star(inner) => {
                let node = self.operand(inner);
                self.out.push('*');
                Node::Star(Box::new(node))
            }
            Re::Plus(inner) => {
                let node = self.operand(inner);
                self.out.push('+');
                Node::Plus(Box::new(node))
            }
            Re::Opt(inner) => {
                let node = self.operand(inner);
                self.out.push('?');
                Node::Opt(Box::new(node))
            }
            Re::Group(inner) => self.group("(", inner, None),
            Re::Fold(on, inner) => {
                let open = if *on { "(?i:" } else { "(?-i:" };
                self.group(open, inner, Some(*on))
            }
            Re::Flag(on) => {
                self.out.push_str(if *on { "(?i)" } else { "(?-i)" });
                self.fold = *on;
                Node::Empty
            }
            Re::Start => {
                self.out.push('^');
                Node::Start
            }
            Re::End => {
                self.out.push('$');
                Node::End
            }
            Re::Boundary(negated) => {
                self.out.push_str(if *negated { "\\B" } else { "\\b" });
                Node::Boundary(*negated)
            }
        }
    }

    /// `re` as what a `*`, `+` or `?` repeats. A bare anchor or flag cannot be repeated, and a
    /// sequence or alternation repeats only as a group, so those are put in one.
    fn operand(&mut self, re: &Re) -> Node {
        match re {
            Re::Lit(_)
            | Re::Any
            | Re::Class { .. }
            | Re::Short(..)
            | Re::Group(_)
            | Re::Fold(..)
            | Re::Star(_)
            | Re::Plus(_)
            | Re::Opt(_) => self.element(re),
            _ => self.group("(?:", re, None),
        }
    }

    /// `inner` between `open` and `)`. A flag set inside ends with the group, and `fold` is what
    /// the group's own flag, as in `(?i:`, sets for what it encloses.
    fn group(&mut self, open: &str, inner: &Re, fold: Option<bool>) -> Node {
        let outer = self.fold;
        if let Some(on) = fold {
            self.fold = on;
        }
        self.out.push_str(open);
        self.depth += 1;
        self.deepest = self.deepest.max(self.depth);
        let node = self.alternation(inner);
        self.out.push(')');
        self.depth -= 1;
        self.fold = outer;
        node
    }

    /// A character inside brackets, escaped where it would end or reshape the class.
    fn member(&mut self, c: char) {
        self.character(c, "\\]^-");
    }

    /// `c`, escaped if it is one of `special`. A newline and a tab are written as the escapes the
    /// engine reads for them, so both spellings of each character are reached.
    fn character(&mut self, c: char, special: &str) {
        match c {
            '\n' => self.out.push_str("\\n"),
            '\t' => self.out.push_str("\\t"),
            _ => {
                if special.contains(c) {
                    self.out.push('\\');
                }
                self.out.push(c);
            }
        }
    }
}

/// The single-character lowercase of `c`, or `c` where lowercasing gives more than one character.
fn lower(c: char) -> char {
    let mut mapped = c.to_lowercase();
    match (mapped.next(), mapped.next()) {
        (Some(only), None) => only,
        _ => c,
    }
}

/// The single-character uppercase of `c`, or `c` where uppercasing gives more than one character.
fn upper(c: char) -> char {
    let mut mapped = c.to_uppercase();
    match (mapped.next(), mapped.next()) {
        (Some(only), None) => only,
        _ => c,
    }
}

fn is_word(c: char) -> bool {
    Short::Word.holds(c)
}

/// Whether `node` matches somewhere in `text`.
pub fn matches(node: &Node, text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let everywhere: BTreeSet<usize> = (0..=chars.len()).collect();
    !ends(node, &chars, &everywhere).is_empty()
}

/// The positions at which `node` can end, having begun at any of `starts`.
fn ends(node: &Node, text: &[char], starts: &BTreeSet<usize>) -> BTreeSet<usize> {
    let step = |holds: &dyn Fn(char) -> bool| -> BTreeSet<usize> {
        starts
            .iter()
            .filter(|&&at| text.get(at).is_some_and(|&c| holds(c)))
            .map(|&at| at + 1)
            .collect()
    };
    match node {
        Node::Empty => starts.clone(),
        Node::Char(want, folded) => step(&|c| c == *want || (*folded && lower(c) == lower(*want))),
        Node::Any => step(&|c| c != '\n'),
        Node::Class {
            negated,
            folded,
            items,
        } => step(&|c| {
            let hit = |c: char| items.iter().any(|item| item.holds(c));
            let member = hit(c) || (*folded && (hit(lower(c)) || hit(upper(c))));
            member != *negated
        }),
        Node::Cat(nodes) => nodes
            .iter()
            .fold(starts.clone(), |at, node| ends(node, text, &at)),
        Node::Alt(branches) => branches
            .iter()
            .flat_map(|branch| ends(branch, text, starts))
            .collect(),
        Node::Star(inner) => closure(inner, text, starts.clone()),
        Node::Plus(inner) => closure(inner, text, ends(inner, text, starts)),
        Node::Opt(inner) => {
            let mut reached = starts.clone();
            reached.extend(ends(inner, text, starts));
            reached
        }
        Node::Start => starts.iter().copied().filter(|&at| at == 0).collect(),
        Node::End => starts
            .iter()
            .copied()
            .filter(|&at| at == text.len())
            .collect(),
        Node::Boundary(negated) => starts
            .iter()
            .copied()
            .filter(|&at| {
                let before = at > 0 && is_word(text[at - 1]);
                let after = text.get(at).is_some_and(|&c| is_word(c));
                (before != after) != *negated
            })
            .collect(),
    }
}

/// `reached` and every position another run of `node` gets to from it.
fn closure(node: &Node, text: &[char], mut reached: BTreeSet<usize>) -> BTreeSet<usize> {
    let mut frontier = reached.clone();
    loop {
        let fresh: BTreeSet<usize> = ends(node, text, &frontier)
            .difference(&reached)
            .copied()
            .collect();
        if fresh.is_empty() {
            return reached;
        }
        reached.extend(fresh.iter().copied());
        frontier = fresh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(c: char) -> Re {
        Re::Lit(c)
    }

    fn compiled(re: &Re, folded: bool) -> (String, Node) {
        let built = build(re, 0, 0, folded);
        match built.expected {
            Expected::Compiles(node) => (built.text, node),
            other => panic!("{other:?}"),
        }
    }

    fn found(re: &Re, folded: bool, text: &str) -> bool {
        matches(&compiled(re, folded).1, text)
    }

    /// The engine reads the rendered text, so a renderer that wrote a different pattern from the
    /// tree would make every comparison with the evaluator a comparison of two different patterns.
    #[test]
    fn a_tree_renders_as_the_pattern_it_stands_for() {
        let tree = Re::Cat(vec![
            lit('a'),
            Re::Star(Box::new(Re::Alt(vec![lit('b'), lit('.')]))),
            Re::Opt(Box::new(Re::Start)),
        ]);
        assert_eq!(compiled(&tree, false).0, r"a(?:b|\.)*(?:^)?");
        let class = Re::Class {
            negated: true,
            items: vec![
                Item::Single(']'),
                Item::Range('-', '^'),
                Item::Named(Short::Digit, true),
            ],
        };
        assert_eq!(compiled(&class, false).0, r"[^\]\--\^\D]");
        assert_eq!(compiled(&Re::Flag(true), false).0, "(?i)");
        assert_eq!(
            compiled(&Re::Cat(vec![lit('\n'), lit('\t')]), false).0,
            r"\n\t"
        );
        let spaces = Re::Class {
            negated: false,
            items: vec![Item::Single('\n'), Item::Range('\t', '\u{b}')],
        };
        assert_eq!(compiled(&spaces, false).0, "[\\n\\t-\u{b}]");
        assert_eq!(
            compiled(&Re::Plus(Box::new(Re::Flag(false))), false).0,
            "(?:(?-i))+"
        );
    }

    /// The target expects the engine to refuse the patterns this predicts, so the prediction has to
    /// put each boundary where the engine documents it, with length checked before depth.
    #[test]
    fn a_group_that_is_too_deep_or_a_pattern_that_is_too_long_is_expected_to_be_refused() {
        assert!(matches!(
            build(&lit('a'), MAX_DEPTH + 1, 0, false).expected,
            Expected::TooDeep
        ));
        assert!(matches!(
            build(&lit('a'), MAX_DEPTH, 0, false).expected,
            Expected::Compiles(_)
        ));
        // `(` and `)` around 999 `a`s is 1,001 characters.
        assert!(matches!(
            build(&lit('a'), 0, MAX_PATTERN - 2, false).expected,
            Expected::TooLong { length: 1_001 }
        ));
        assert!(matches!(
            build(&lit('a'), 0, MAX_PATTERN - 3, false).expected,
            Expected::Compiles(_)
        ));
        // Length is checked before depth.
        assert!(matches!(
            build(&lit('a'), MAX_DEPTH + 1, MAX_PATTERN, false).expected,
            Expected::TooLong { .. }
        ));
    }

    /// The evaluator is what the engine is compared with, so a construct it reads wrongly is reported
    /// as a defect in the engine.
    #[test]
    fn the_evaluator_agrees_with_what_the_constructs_mean() {
        let ab = Re::Cat(vec![lit('a'), lit('b')]);
        assert!(found(&ab, false, "xxab"));
        assert!(!found(&ab, false, "ba"));
        assert!(!found(
            &Re::Cat(vec![lit('a'), Re::Any, lit('c')]),
            false,
            "a\nc"
        ));
        assert!(found(&Re::Star(Box::new(lit('z'))), false, ""));
        assert!(!found(&Re::Plus(Box::new(lit('z'))), false, "y"));
        let anchored = Re::Cat(vec![Re::Start, lit('a'), Re::End]);
        assert!(found(&anchored, false, "a"));
        assert!(!found(&anchored, false, "ba"));
        assert!(!found(&anchored, false, "ab"));
        // A loop that can match nothing ends.
        let nested = Re::Star(Box::new(Re::Group(Box::new(Re::Star(Box::new(lit('a')))))));
        assert!(found(&Re::Cat(vec![nested, lit('b')]), false, "aaab"));
        let word = Re::Cat(vec![Re::Boundary(false), lit('a')]);
        assert!(found(&word, false, "b a"));
        assert!(!found(&word, false, "ba"));
        assert!(found(
            &Re::Cat(vec![lit('b'), Re::Boundary(true), lit('a')]),
            false,
            "ba"
        ));
    }

    /// Negation applies after the fold. Trying both cases against the finished decision would let
    /// `[^a-z]` match `A`.
    #[test]
    fn a_folded_negated_class_excludes_the_other_case_too() {
        let class = Re::Class {
            negated: true,
            items: vec![Item::Range('a', 'z')],
        };
        assert!(!found(&class, true, "A"));
        assert!(found(&class, false, "A"));
        assert!(found(&class, true, "1"));
    }

    /// Where a flag stops is the engine's most intricate rule. The evaluator has to apply it as the
    /// engine documents it, or every pattern with a flag inside a group would be reported.
    #[test]
    fn a_flag_lasts_to_the_end_of_its_group_and_covers_later_branches() {
        let inside = Re::Cat(vec![
            Re::Group(Box::new(Re::Cat(vec![Re::Flag(true), lit('a')]))),
            lit('b'),
        ]);
        assert!(found(&inside, false, "Ab"));
        assert!(!found(&inside, false, "AB"));

        let later_branch = Re::Alt(vec![Re::Cat(vec![Re::Flag(true), lit('a')]), lit('b')]);
        assert!(found(&later_branch, false, "B"));

        let scoped = Re::Cat(vec![Re::Fold(true, Box::new(lit('a'))), lit('b')]);
        assert!(found(&scoped, false, "Ab"));
        assert!(!found(&scoped, false, "AB"));
        assert!(found(&Re::Fold(false, Box::new(lit('a'))), true, "a"));
        assert!(!found(&Re::Fold(false, Box::new(lit('a'))), true, "A"));
    }

    /// Case folds by the single-character lowercase of both sides, so the Kelvin sign is `k`.
    #[test]
    fn folding_compares_single_character_lowercases() {
        assert!(found(&lit('k'), true, "\u{212A}"));
        assert!(!found(&lit('k'), false, "\u{212A}"));
        assert!(found(&lit('\u{e9}'), true, "\u{c9}"));
        assert!(!found(&lit('s'), true, "\u{df}"));
    }

    /// The evaluator's cost grows with the tree, so a tree that ignored the budget would make an
    /// input slow rather than wrong.
    #[test]
    fn generated_trees_stay_inside_the_node_budget() {
        let bytes: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        for start in 0..64 {
            let mut u = Unstructured::new(&bytes[start..]);
            let re = Re::arbitrary(&mut u).expect("a tree");
            let (inner, all) = count(&re);
            assert!(inner <= MAX_NODES, "{inner} nodes with children in {re:?}");
            assert!(all <= 3 * MAX_NODES + 1, "{all} nodes in {re:?}");
        }
    }

    /// The nodes of `re` that have children, and all of them.
    fn count(re: &Re) -> (usize, usize) {
        let below = |children: &[&Re]| {
            children
                .iter()
                .map(|child| count(child))
                .fold((1, 1), |(inner, all), (i, a)| (inner + i, all + a))
        };
        match re {
            Re::Cat(items) | Re::Alt(items) => below(&items.iter().collect::<Vec<_>>()),
            Re::Star(inner)
            | Re::Plus(inner)
            | Re::Opt(inner)
            | Re::Group(inner)
            | Re::Fold(_, inner) => below(&[inner]),
            _ => (0, 1),
        }
    }
}
