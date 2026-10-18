//! Regular expressions for `search`, matched without backtracking.
//!
//! A pattern arrives through a turn, so the engine that runs it is attack surface. The usual
//! objection to regular expressions here is catastrophic backtracking, where `(a+)+$` against a
//! run of `a`s costs exponential time. That is a property of backtracking implementations, not of
//! regular expressions: this one simulates a Thompson NFA, advancing a set of states one
//! character at a time, so a pattern is matched in time proportional to the length of the line
//! times the size of the pattern and there is no input that makes it blow up.
//!
//! The NFA is the slow part, and most lines in a search are nowhere near a match, so a line is
//! looked through for literal text first. A pattern that is only an alternation of literals is
//! answered by finding one of them. Any other pattern that requires some text in every match
//! rejects a line lacking it, and only a line that holds it is simulated. Both give the answer the
//! NFA gives on every line, and a test holds them to it.
//!
//! Hand-written rather than a dependency, and deliberately small. Supported, and nothing else:
//!
//! - literal characters
//! - `.`, any character except a newline
//! - `*`, `+`, `?`
//! - `|`, and `(...)` to group, or `(?:...)`, which is the same group since nothing is captured
//! - `(?i)` and `(?-i)`, folding case or not for the rest of the group they are in, and
//!   `(?i:...)` and `(?-i:...)` for only what they enclose
//! - `[abc]`, `[a-z]`, `[^abc]`
//! - `\d`, `\w`, `\s` and their negations `\D`, `\W`, `\S`
//! - `^` and `$`, anchoring to the ends of a line
//! - `\b` and `\B`, word boundaries
//! - a backslash before punctuation, matching it literally
//!
//! Counted repetition (`a{3}`, `a{2,9}`) is absent on purpose, and `{` is an ordinary character.
//! Nesting two of them multiplies the states one expands to, which is the one construct that
//! turns a short pattern into an enormous program, and no bound on the pattern's length would
//! bound the work. Backreferences are absent because they are not regular: no automaton
//! recognises them, and matching one needs the backtracking this engine exists to avoid.
//! Lookaround, named groups and every flag but `i` are absent too, and a `(?` introducing one is
//! refused as that rather than read as a group whose first thing is a `?` with nothing to repeat.
//!
//! Captures are not extracted. `search` reports the line a match was found on, so whether a
//! pattern matched is the whole question, and a boolean is all any of this has to answer.

use std::cell::RefCell;
use std::fmt;

/// The longest pattern that will be compiled.
///
/// Every construct here expands to a number of states linear in the pattern's length, so this is
/// also the bound on the program, and with it the bound on the work any one line costs.
const MAX_PATTERN: usize = 1_000;

/// How deeply groups may nest.
///
/// The parser is recursive descent, so this is what keeps `((((...))))` from exhausting the
/// stack. The matcher itself does not recurse.
const MAX_DEPTH: usize = 50;

#[derive(Debug, PartialEq, Eq)]
pub enum PatternError {
    TooLong {
        length: usize,
    },
    TooDeep,
    UnclosedGroup,
    UnmatchedParen,
    UnclosedClass,
    EmptyClass,
    /// A `*`, `+` or `?` with nothing before it to repeat.
    NothingToRepeat {
        at: usize,
    },
    /// A backslash at the end of the pattern, promising an escape that never arrived.
    DanglingEscape,
    /// A `(?` opening something other than `(?:` or the case flag, such as lookaround.
    UnsupportedGroup {
        at: usize,
    },
    /// A range whose end sorts before its start, like `[z-a]`.
    ReversedRange {
        start: char,
        end: char,
    },
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { length } => write!(
                f,
                "the pattern is {length} characters, and {MAX_PATTERN} is the most that is matched"
            ),
            Self::TooDeep => write!(f, "the pattern nests groups more than {MAX_DEPTH} deep"),
            Self::UnclosedGroup => write!(f, "a '(' was never closed"),
            Self::UnmatchedParen => write!(f, "a ')' closes a group that was never opened"),
            Self::UnclosedClass => write!(f, "a '[' was never closed"),
            Self::EmptyClass => write!(f, "'[]' matches nothing at all"),
            Self::NothingToRepeat { at } => write!(
                f,
                "the repeat at character {at} has nothing before it to repeat"
            ),
            Self::DanglingEscape => write!(f, "the pattern ends with a '\\' and nothing to escape"),
            Self::UnsupportedGroup { at } => write!(
                f,
                "the '(?' at character {at} is not supported: '(?:...)' and the case flags \
                 '(?i)' and '(?-i)' are, but lookaround, named groups and other flags are not"
            ),
            Self::ReversedRange { start, end } => {
                write!(f, "the range '{start}-{end}' runs backwards")
            }
        }
    }
}

impl std::error::Error for PatternError {}

/// A named set of characters, as `\d` and friends abbreviate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shorthand {
    Digit,
    Word,
    Space,
}

impl Shorthand {
    fn holds(self, c: char) -> bool {
        match self {
            Self::Digit => c.is_ascii_digit(),
            // `_` counts as a word character, as it does everywhere else, so `\w+` matches an
            // identifier rather than stopping in the middle of one.
            Self::Word => c.is_alphanumeric() || c == '_',
            Self::Space => c.is_whitespace(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Member {
    /// Inclusive, and a single character is a range of one.
    Range(char, char),
    Named(Shorthand, bool),
}

impl Member {
    fn holds(&self, c: char) -> bool {
        match self {
            Self::Range(start, end) => c >= *start && c <= *end,
            Self::Named(named, negated) => named.holds(c) != *negated,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Class {
    negated: bool,
    /// Whether case is ignored, decided where the class was written.
    folded: bool,
    members: Vec<Member>,
}

impl Class {
    /// Whether the class matches `c`.
    ///
    /// Where case is ignored, the fold applies to the membership test and the negation is applied
    /// to the result, which is the whole subtlety: trying both cases against the finished decision
    /// would make `[^a-z]` match `A`, because the lowercase of `A` is outside the negated set.
    /// Widening a set has to widen what it excludes too.
    fn holds(&self, c: char) -> bool {
        let member = self
            .members
            .iter()
            .any(|m| m.holds(c) || (self.folded && (m.holds(lower(c)) || m.holds(upper(c)))));
        member != self.negated
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    /// Matches without consuming anything, which is what an empty alternative branch is.
    Empty,
    Char {
        want: char,
        folded: bool,
    },
    Any,
    Class(Class),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Star(Box<Node>),
    Plus(Box<Node>),
    Opt(Box<Node>),
    LineStart,
    LineEnd,
    /// `\b`, or `\B` when negated.
    Boundary(bool),
}

/// One step of the compiled program.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Inst {
    Char {
        want: char,
        folded: bool,
    },
    Any,
    Class(Class),
    /// Continue as both threads. The whole reason the running time is bounded: a split is two
    /// states in one set, not two attempts one after the other.
    Split(usize, usize),
    Jump(usize),
    LineStart,
    LineEnd,
    Boundary(bool),
    Match,
}

/// A compiled pattern.
#[derive(Debug, Clone)]
pub struct Regex {
    program: Vec<Inst>,
    /// Text a line must hold for the pattern to match it, when the pattern requires some.
    required: Option<Required>,
}

/// Literal text of which every match holds at least one, found in a line as a substring.
///
/// A line holding none of them cannot match and is rejected without running the NFA. Where the
/// pattern is nothing but an alternation of literals, a line holding one of them is a match, and
/// the NFA is not needed for it either, so a search for several spellings of a name does not run
/// the NFA on every line.
#[derive(Debug, Clone)]
struct Required {
    literals: Vec<Literal>,
    /// Whether holding one of `literals` is itself a match.
    whole: bool,
    /// Whether any of `literals` ignores case, which is when a line is lowercased to be searched.
    folded: bool,
}

/// Characters that a match holds in order and in a row.
#[derive(Debug, Clone)]
struct Literal {
    /// The characters, lowercased where `folded`, so a comparison is between two lowercase forms.
    text: String,
    folded: bool,
}

/// Whether a line holds a literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presence {
    Absent,
    Present,
    /// Only the NFA can say. A non-ASCII character in the line can fold to an ASCII one (U+212A,
    /// the Kelvin sign, folds to `k`), which a comparison of bytes does not see.
    Unknown,
}

impl Literal {
    /// The literal spelled by `run`, every character of which ignores case or none does.
    fn new(run: &[(char, bool)]) -> Self {
        let folded = run.first().is_some_and(|&(_, folded)| folded);
        Self {
            text: run
                .iter()
                .map(|&(want, _)| if folded { lower(want) } else { want })
                .collect(),
            folded,
        }
    }

    /// How many characters a line has to hold in a row to pass this literal.
    fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// Whether `line` holds this literal. `lowered` is `line` lowercased where it is ASCII
    /// throughout, and is only read for a literal that ignores case.
    fn presence_in(&self, line: &str, lowered: Option<&str>) -> Presence {
        if !self.folded {
            // Equal characters have equal UTF-8, and valid UTF-8 found in valid UTF-8 starts on a
            // character boundary, so this finds what the NFA's comparison of characters finds.
            return if line.contains(self.text.as_str()) {
                Presence::Present
            } else {
                Presence::Absent
            };
        }
        let Some(lowered) = lowered else {
            return Presence::Unknown;
        };
        // The NFA compares `lower` of both sides, so on an ASCII line a character that does not
        // lowercase to ASCII has nothing to match.
        if lowered.contains(self.text.as_str()) {
            Presence::Present
        } else {
            Presence::Absent
        }
    }
}

impl Required {
    fn of(node: &Node) -> Option<Self> {
        let (literals, whole) = match Self::exactly(node) {
            Some(literals) => (literals, true),
            None => (Self::within(node)?, false),
        };
        let folded = literals.iter().any(|literal| literal.folded);
        Some(Self {
            literals,
            whole,
            folded,
        })
    }

    /// The literals `node` is an alternation of, or `None` where it is anything else.
    fn exactly(node: &Node) -> Option<Vec<Literal>> {
        match node {
            Node::Alt(branches) => {
                let mut all = Vec::new();
                for branch in branches {
                    all.extend(Self::exactly(branch)?);
                }
                Some(all)
            }
            Node::Char { .. } => Self::spelled(std::slice::from_ref(node)).map(|only| vec![only]),
            Node::Concat(nodes) => Self::spelled(nodes).map(|only| vec![only]),
            _ => None,
        }
    }

    /// The one literal `nodes` consist of, or `None` where they hold anything else or spell more
    /// than one, as `a(?i)b` does.
    fn spelled(nodes: &[Node]) -> Option<Literal> {
        let (mut runs, rest) = Self::runs(nodes);
        match (runs.pop(), runs.is_empty(), rest.is_empty()) {
            (Some(only), true, true) => Some(only),
            _ => None,
        }
    }

    /// A set of literals one of which every match of `node` holds, or `None` where none is
    /// known. Of the sets a sequence offers, the one whose shortest literal is longest wins,
    /// since a longer literal is found in fewer lines, and then the one with the fewest, and then
    /// the one found first, a run of characters before the nodes between runs.
    fn within(node: &Node) -> Option<Vec<Literal>> {
        match node {
            Node::Char { .. } => Self::spelled(std::slice::from_ref(node)).map(|only| vec![only]),
            Node::Plus(inner) => Self::within(inner),
            Node::Alt(branches) => {
                let mut all = Vec::new();
                for branch in branches {
                    all.extend(Self::within(branch)?);
                }
                Some(all)
            }
            Node::Concat(nodes) => {
                let (runs, rest) = Self::runs(nodes);
                let mut candidates: Vec<Vec<Literal>> =
                    runs.into_iter().map(|run| vec![run]).collect();
                candidates.extend(rest.into_iter().filter_map(Self::within));
                // `max_by_key` keeps the last of equal keys, so the order is reversed to keep the
                // first. The first is the character a pattern begins with where it begins with one.
                candidates.into_iter().rev().max_by_key(|set| {
                    (
                        set.iter().map(Literal::len).min(),
                        std::cmp::Reverse(set.len()),
                    )
                })
            }
            // A repeat that may run zero times, a class, `.` and the anchors require no text.
            _ => None,
        }
    }

    /// Split a sequence into the literals it spells and the nodes between them. Characters that
    /// ignore case and ones that do not make separate literals, and `Empty`, which a `(?i)`
    /// compiles to and which consumes nothing, does not separate two characters.
    fn runs(nodes: &[Node]) -> (Vec<Literal>, Vec<&Node>) {
        let mut runs = Vec::new();
        let mut rest = Vec::new();
        let mut current: Vec<(char, bool)> = Vec::new();
        for node in nodes {
            match node {
                Node::Char { want, folded } => {
                    if current.first().is_some_and(|first| first.1 != *folded) {
                        runs.push(Literal::new(&current));
                        current.clear();
                    }
                    current.push((*want, *folded));
                }
                Node::Empty => {}
                other => {
                    if !current.is_empty() {
                        runs.push(Literal::new(&current));
                        current.clear();
                    }
                    rest.push(other);
                }
            }
        }
        if !current.is_empty() {
            runs.push(Literal::new(&current));
        }
        (runs, rest)
    }

    fn presence_in(&self, line: &str) -> Presence {
        if self.folded && line.is_ascii() {
            // Lowercased once for every literal rather than compared a byte at a time.
            return SCRATCH.with_borrow_mut(|scratch| {
                let found = self.scan(line, Some(scratch.lowercase(line)));
                scratch.release();
                found
            });
        }
        self.scan(line, None)
    }

    fn scan(&self, line: &str, lowered: Option<&str>) -> Presence {
        let mut unknown = false;
        for literal in &self.literals {
            match literal.presence_in(line, lowered) {
                Presence::Present => return Presence::Present,
                Presence::Unknown => unknown = true,
                Presence::Absent => {}
            }
        }
        if unknown {
            Presence::Unknown
        } else {
            Presence::Absent
        }
    }
}

impl Regex {
    /// Compile `pattern`, or say why it cannot be.
    pub fn compile(pattern: &str) -> Result<Self, PatternError> {
        Self::build(pattern, false)
    }

    /// Compile `pattern` to match without regard to case.
    pub fn compile_folded(pattern: &str) -> Result<Self, PatternError> {
        Self::build(pattern, true)
    }

    fn build(pattern: &str, folded: bool) -> Result<Self, PatternError> {
        let chars: Vec<char> = pattern.chars().collect();
        if chars.len() > MAX_PATTERN {
            return Err(PatternError::TooLong {
                length: chars.len(),
            });
        }

        let mut parser = Parser {
            chars: &chars,
            at: 0,
            depth: 0,
            fold: folded,
        };
        let node = parser.alternation()?;
        if parser.at < chars.len() {
            // The only way to stop early is a ')' with nothing open.
            return Err(PatternError::UnmatchedParen);
        }

        let mut program = Vec::new();
        emit(&node, &mut program);
        program.push(Inst::Match);
        Ok(Self {
            program,
            required: Required::of(&node),
        })
    }

    /// Whether `text` holds a match anywhere in it.
    ///
    /// A line is first looked through for the literal text the pattern requires, which rejects
    /// most lines for a pattern that has some and accepts one outright for a pattern that is
    /// nothing else. What is left goes to the NFA.
    pub fn matches(&self, text: &str) -> bool {
        if let Some(required) = &self.required {
            match required.presence_in(text) {
                Presence::Absent => return false,
                Presence::Present if required.whole => return true,
                Presence::Present | Presence::Unknown => {}
            }
        }
        self.simulate(text)
    }

    /// Run the NFA over `text`, with no regard to the literals.
    ///
    /// Unanchored, so a thread starts at every position unless `^` says otherwise. The state set
    /// is deduplicated at each position, so the work is bounded by the line's length times the
    /// program's size however many threads the pattern would like to spawn.
    fn simulate(&self, text: &str) -> bool {
        #[cfg(test)]
        SIMULATIONS.with(|runs| runs.set(runs.get() + 1));

        SCRATCH.with_borrow_mut(|scratch| {
            scratch.load(self.program.len(), text);
            let found = self.run(scratch);
            scratch.release();
            found
        })
    }

    fn run(&self, scratch: &mut Scratch) -> bool {
        let Scratch {
            chars,
            current,
            next,
            on_current,
            on_next,
            pending,
            ..
        } = scratch;

        for position in 0..=chars.len() {
            // A fresh attempt beginning here, which is what makes the search unanchored. Where
            // the pattern starts with `^` the assertion kills this thread at once for every
            // position but the first.
            self.follow(0, position, chars, current, on_current, pending);

            if current.iter().any(|&pc| self.program[pc] == Inst::Match) {
                return true;
            }
            if position == chars.len() {
                break;
            }

            let c = chars[position];
            next.clear();
            on_next.iter_mut().for_each(|seen| *seen = false);
            for &pc in current.iter() {
                let consumes = match &self.program[pc] {
                    Inst::Char { want, folded } => {
                        *want == c || (*folded && lower(*want) == lower(c))
                    }
                    // A line never holds its own terminator, but a search may be handed text
                    // that does, and `.` is not supposed to cross one.
                    Inst::Any => c != '\n',
                    Inst::Class(class) => class.holds(c),
                    _ => continue,
                };
                if consumes {
                    self.follow(pc + 1, position + 1, chars, next, on_next, pending);
                }
            }

            std::mem::swap(current, next);
            std::mem::swap(on_current, on_next);
        }

        false
    }

    /// Add `pc` and everything reachable from it without consuming a character.
    ///
    /// Iterative rather than recursive: the closure of a pattern like `(a*)*` reaches deep, and a
    /// pattern arriving through a turn must not be able to choose the depth of this stack.
    /// `pending` is the stack, emptied on return and kept by the caller so that it is allocated
    /// once for the line rather than once per character.
    fn follow(
        &self,
        pc: usize,
        position: usize,
        chars: &[char],
        list: &mut Vec<usize>,
        seen: &mut [bool],
        pending: &mut Vec<usize>,
    ) {
        pending.push(pc);
        while let Some(pc) = pending.pop() {
            if seen[pc] {
                continue;
            }
            seen[pc] = true;
            match &self.program[pc] {
                Inst::Jump(to) => pending.push(*to),
                Inst::Split(a, b) => {
                    pending.push(*a);
                    pending.push(*b);
                }
                Inst::LineStart => {
                    if position == 0 {
                        pending.push(pc + 1);
                    }
                }
                Inst::LineEnd => {
                    if position == chars.len() {
                        pending.push(pc + 1);
                    }
                }
                Inst::Boundary(negated) => {
                    let word = |c: char| Shorthand::Word.holds(c);
                    let before = position > 0 && word(chars[position - 1]);
                    let after = position < chars.len() && word(chars[position]);
                    if ((before != after) != *negated) || (before && after && *negated) {
                        pending.push(pc + 1);
                    }
                }
                // Consumes a character, so it stays in the set for the step to deal with.
                Inst::Char { .. } | Inst::Any | Inst::Class(_) | Inst::Match => list.push(pc),
            }
        }
    }
}

/// The most characters of a line that the buffers keep room for between calls.
const RETAINED_CHARS: usize = 64 * 1024;

thread_local! {
    static SCRATCH: RefCell<Scratch> = const { RefCell::new(Scratch::empty()) };
}

#[cfg(test)]
thread_local! {
    /// How many times the NFA has run on this thread.
    static SIMULATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The buffers a line is matched in: the line lowercased for the literals, and what the NFA works
/// in.
///
/// Kept for the thread rather than built per line: a search runs the NFA on lines that are a few
/// dozen characters long, and building these costs more than running over them.
struct Scratch {
    lowered: String,
    chars: Vec<char>,
    current: Vec<usize>,
    next: Vec<usize>,
    on_current: Vec<bool>,
    on_next: Vec<bool>,
    pending: Vec<usize>,
}

impl Scratch {
    const fn empty() -> Self {
        Self {
            lowered: String::new(),
            chars: Vec::new(),
            current: Vec::new(),
            next: Vec::new(),
            on_current: Vec::new(),
            on_next: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// `line` lowercased where it is ASCII, which is the only place a byte comparison can tell
    /// that two cases are one character.
    fn lowercase(&mut self, line: &str) -> &str {
        self.lowered.clear();
        self.lowered.push_str(line);
        self.lowered.make_ascii_lowercase();
        &self.lowered
    }

    /// Ready the buffers for `text` and a program of `states` instructions.
    fn load(&mut self, states: usize, text: &str) {
        self.chars.clear();
        self.chars.extend(text.chars());
        self.current.clear();
        self.next.clear();
        self.pending.clear();
        // Both start with no state in them, which `matches` relies on at the first position.
        self.on_current.clear();
        self.on_current.resize(states, false);
        self.on_next.clear();
        self.on_next.resize(states, false);
    }

    /// Give back the room a very long line took, so one minified file does not leave the buffers
    /// for it, four bytes per character in the largest, held by the thread for the rest of the
    /// search.
    fn release(&mut self) {
        if self.lowered.capacity() > RETAINED_CHARS {
            self.lowered = String::new();
        }
        if self.chars.capacity() > RETAINED_CHARS {
            self.chars = Vec::new();
        }
    }
}

/// Compile `node` onto the end of `program`.
fn emit(node: &Node, program: &mut Vec<Inst>) {
    match node {
        Node::Empty => {}
        Node::Char { want, folded } => program.push(Inst::Char {
            want: *want,
            folded: *folded,
        }),
        Node::Any => program.push(Inst::Any),
        Node::Class(class) => program.push(Inst::Class(class.clone())),
        Node::LineStart => program.push(Inst::LineStart),
        Node::LineEnd => program.push(Inst::LineEnd),
        Node::Boundary(negated) => program.push(Inst::Boundary(*negated)),
        Node::Concat(nodes) => {
            for node in nodes {
                emit(node, program);
            }
        }
        Node::Alt(branches) => {
            // Each branch gets a split choosing between it and everything after it, and ends with
            // a jump to the common exit, which is patched once every branch is placed.
            let mut exits = Vec::new();
            for (index, branch) in branches.iter().enumerate() {
                if index + 1 == branches.len() {
                    emit(branch, program);
                    break;
                }
                let split = program.len();
                program.push(Inst::Jump(0));
                emit(branch, program);
                let exit = program.len();
                program.push(Inst::Jump(0));
                exits.push(exit);
                let rest = program.len();
                program[split] = Inst::Split(split + 1, rest);
            }
            let end = program.len();
            for exit in exits {
                program[exit] = Inst::Jump(end);
            }
        }
        Node::Star(inner) => {
            let split = program.len();
            program.push(Inst::Jump(0));
            emit(inner, program);
            program.push(Inst::Jump(split));
            let after = program.len();
            program[split] = Inst::Split(split + 1, after);
        }
        Node::Plus(inner) => {
            let start = program.len();
            emit(inner, program);
            let split = program.len();
            program.push(Inst::Jump(0));
            let after = program.len();
            program[split] = Inst::Split(start, after);
        }
        Node::Opt(inner) => {
            let split = program.len();
            program.push(Inst::Jump(0));
            emit(inner, program);
            let after = program.len();
            program[split] = Inst::Split(split + 1, after);
        }
    }
}

struct Parser<'p> {
    chars: &'p [char],
    at: usize,
    depth: usize,
    /// Whether what is parsed from here on ignores case.
    ///
    /// Recorded on each character and class rather than applied by lowercasing the pattern string,
    /// which would rewrite `\D`, `\W` and `\S` into the classes they negate and invert what the
    /// search asked for. A folded compile starts it true, and `(?i)` and `(?-i)` switch it until
    /// the group they are in closes.
    fold: bool,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn char(&self, want: char) -> Node {
        Node::Char {
            want,
            folded: self.fold,
        }
    }

    fn named(&self, shorthand: Shorthand, negated: bool) -> Node {
        Node::Class(Class {
            negated: false,
            folded: self.fold,
            members: vec![Member::Named(shorthand, negated)],
        })
    }

    /// `a|b|c`, the loosest-binding construct and so the outermost.
    fn alternation(&mut self) -> Result<Node, PatternError> {
        let mut branches = vec![self.concatenation()?];
        while self.peek() == Some('|') {
            self.at += 1;
            branches.push(self.concatenation()?);
        }
        if branches.len() == 1 {
            // The length was checked on the line above.
            // nosemgrep: trailofbits.rs.panic-in-function-returning-result.panic-in-function-returning-result
            return Ok(branches.pop().expect("one branch"));
        }
        Ok(Node::Alt(branches))
    }

    /// A run of repeated atoms, stopping at `|` or the `)` that closes the group above.
    fn concatenation(&mut self) -> Result<Node, PatternError> {
        let mut nodes = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            nodes.push(self.repeated()?);
        }
        match nodes.len() {
            0 => Ok(Node::Empty),
            // The arm matched on the length, so the pop is checked by construction.
            // nosemgrep: trailofbits.rs.panic-in-function-returning-result.panic-in-function-returning-result
            1 => Ok(nodes.pop().expect("one node")),
            _ => Ok(Node::Concat(nodes)),
        }
    }

    /// An atom and whatever repeats it.
    fn repeated(&mut self) -> Result<Node, PatternError> {
        let at = self.at;
        let (atom, repeatable) = self.atom()?;

        let mut node = atom;
        while let Some(c) = self.peek() {
            let wrap = match c {
                '*' => Node::Star(Box::new(node)),
                '+' => Node::Plus(Box::new(node)),
                '?' => Node::Opt(Box::new(node)),
                _ => break,
            };
            if !repeatable {
                return Err(PatternError::NothingToRepeat { at });
            }
            self.at += 1;
            node = wrap;
        }
        Ok(node)
    }

    /// One atom, and whether a `*`, `+` or `?` may follow it.
    ///
    /// A bare anchor may not be repeated: `^*` is a mistake rather than a pattern, and saying so
    /// beats compiling something nobody intended. Wrapped in a group it may, because `(\B)?` is
    /// then an ordinary optional subexpression and every engine accepts it.
    fn atom(&mut self) -> Result<(Node, bool), PatternError> {
        let Some(c) = self.peek() else {
            return Ok((Node::Empty, true));
        };
        match c {
            '(' => {
                let open = self.at;
                self.at += 1;
                let outer = self.fold;
                if self.peek() == Some('?') {
                    self.at += 1;
                    if !self.group_flags(open)? {
                        // `(?i)` encloses nothing, so there is nothing for a repeat to repeat.
                        return Ok((Node::Empty, false));
                    }
                }
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return Err(PatternError::TooDeep);
                }
                let inner = self.alternation()?;
                if self.peek() != Some(')') {
                    return Err(PatternError::UnclosedGroup);
                }
                self.at += 1;
                self.depth -= 1;
                // A flag inside the group ends with it.
                self.fold = outer;
                Ok((inner, true))
            }
            '[' => Ok((self.class()?, true)),
            '.' => {
                self.at += 1;
                Ok((Node::Any, true))
            }
            '^' => {
                self.at += 1;
                Ok((Node::LineStart, false))
            }
            '$' => {
                self.at += 1;
                Ok((Node::LineEnd, false))
            }
            '*' | '+' | '?' => Err(PatternError::NothingToRepeat { at: self.at }),
            '\\' => {
                self.at += 1;
                let node = self.escape()?;
                let repeatable = !matches!(node, Node::Boundary(_));
                Ok((node, repeatable))
            }
            _ => {
                self.at += 1;
                Ok((self.char(c), true))
            }
        }
    }

    /// What follows a `(?`, through the `:` or `)` that ends it, the `(` being at `open`.
    ///
    /// Returns whether a group follows: `(?:` and `(?i:` open one, and `(?i)` does not. A flag
    /// takes effect where it is read, so `(?i:` folds the group it opens and `(?i)` folds the rest
    /// of the group around it, including the branches of it after a `|`, as every other engine
    /// does.
    fn group_flags(&mut self, open: usize) -> Result<bool, PatternError> {
        let mut negated = false;
        let mut flagged = false;
        loop {
            match self.peek() {
                None => return Err(PatternError::UnclosedGroup),
                Some(':') => {
                    self.at += 1;
                    return Ok(true);
                }
                Some(')') if flagged => {
                    self.at += 1;
                    return Ok(false);
                }
                Some('-') if !negated && self.chars.get(self.at + 1) == Some(&'i') => {
                    negated = true;
                }
                Some('i') => {
                    self.fold = !negated;
                    flagged = true;
                }
                Some(_) => return Err(PatternError::UnsupportedGroup { at: open }),
            }
            self.at += 1;
        }
    }

    /// What a `\` introduces, the backslash already consumed.
    fn escape(&mut self) -> Result<Node, PatternError> {
        let Some(c) = self.peek() else {
            return Err(PatternError::DanglingEscape);
        };
        self.at += 1;
        Ok(match c {
            'd' => self.named(Shorthand::Digit, false),
            'D' => self.named(Shorthand::Digit, true),
            'w' => self.named(Shorthand::Word, false),
            'W' => self.named(Shorthand::Word, true),
            's' => self.named(Shorthand::Space, false),
            'S' => self.named(Shorthand::Space, true),
            'b' => Node::Boundary(false),
            'B' => Node::Boundary(true),
            'n' => self.char('\n'),
            't' => self.char('\t'),
            'r' => self.char('\r'),
            // Anything else stands for itself, which is what `\.` and `\\` are for.
            other => self.char(other),
        })
    }

    /// `[...]`, the `[` not yet consumed.
    fn class(&mut self) -> Result<Node, PatternError> {
        self.at += 1;
        let negated = self.peek() == Some('^');
        if negated {
            self.at += 1;
        }

        let mut members = Vec::new();
        // A `]` first thing is the character, not the end of an empty class, which is what every
        // other engine does with it.
        if self.peek() == Some(']') {
            members.push(Member::Range(']', ']'));
            self.at += 1;
        }

        loop {
            let Some(c) = self.peek() else {
                return Err(PatternError::UnclosedClass);
            };
            if c == ']' {
                self.at += 1;
                break;
            }
            self.at += 1;

            let start = if c == '\\' {
                match self.escaped_member()? {
                    Ok(named) => {
                        members.push(named);
                        continue;
                    }
                    Err(literal) => literal,
                }
            } else {
                c
            };

            // `a-z`, unless the `-` is the last thing before the `]`, where it is a character.
            if self.peek() == Some('-') && self.chars.get(self.at + 1).copied() != Some(']') {
                self.at += 1;
                let Some(end) = self.peek() else {
                    return Err(PatternError::UnclosedClass);
                };
                self.at += 1;
                let end = if end == '\\' {
                    match self.escaped_member()? {
                        // `[a-\d]` is not a range anybody meant, so the shorthand joins the class
                        // on its own and the `-` is left as a character.
                        Ok(named) => {
                            members.push(Member::Range(start, start));
                            members.push(Member::Range('-', '-'));
                            members.push(named);
                            continue;
                        }
                        Err(literal) => literal,
                    }
                } else {
                    end
                };
                if end < start {
                    return Err(PatternError::ReversedRange { start, end });
                }
                members.push(Member::Range(start, end));
            } else {
                members.push(Member::Range(start, start));
            }
        }

        if members.is_empty() {
            return Err(PatternError::EmptyClass);
        }
        Ok(Node::Class(Class {
            negated,
            folded: self.fold,
            members,
        }))
    }

    /// An escape inside a class: either a named set, or a character standing for itself.
    ///
    /// The backslash is consumed by the caller. `Ok` is a set that joins the class whole, `Err`
    /// is a plain character that may still turn out to be one end of a range.
    fn escaped_member(&mut self) -> Result<Result<Member, char>, PatternError> {
        let Some(c) = self.peek() else {
            return Err(PatternError::DanglingEscape);
        };
        self.at += 1;
        Ok(match c {
            'd' => Ok(Member::Named(Shorthand::Digit, false)),
            'D' => Ok(Member::Named(Shorthand::Digit, true)),
            'w' => Ok(Member::Named(Shorthand::Word, false)),
            'W' => Ok(Member::Named(Shorthand::Word, true)),
            's' => Ok(Member::Named(Shorthand::Space, false)),
            'S' => Ok(Member::Named(Shorthand::Space, true)),
            'n' => Err('\n'),
            't' => Err('\t'),
            'r' => Err('\r'),
            other => Err(other),
        })
    }
}

/// The single-character lowercase of `c`, or `c` where folding it is not one character.
///
/// Anything whose mapping is longer stays as it is, which keeps a fold a comparison between two
/// characters rather than a rewrite of the subject.
fn lower(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    let mut mapped = c.to_lowercase();
    match (mapped.next(), mapped.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

fn upper(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_uppercase();
    }
    let mut mapped = c.to_uppercase();
    match (mapped.next(), mapped.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, text: &str) -> bool {
        Regex::compile(pattern)
            .expect("the pattern compiles")
            .matches(text)
    }

    fn simulations() -> usize {
        SIMULATIONS.with(std::cell::Cell::get)
    }

    /// The NFA over `text` in buffers built for the call, which no shortcut and no earlier line
    /// has touched.
    fn by_automaton_alone(regex: &Regex, text: &str) -> bool {
        let mut scratch = Scratch::empty();
        scratch.load(regex.program.len(), text);
        regex.run(&mut scratch)
    }

    #[test]
    fn a_literal_pattern_matches_the_substring_it_spells() {
        assert!(matches("needle", "a needle in a haystack"));
        assert!(!matches("needle", "a haystack"));
    }

    #[test]
    fn a_dot_matches_any_character_but_a_newline() {
        assert!(matches("a.c", "abc"));
        assert!(matches("a.c", "a c"));
        assert!(!matches("a.c", "ac"));
        assert!(!matches("a.c", "a\nc"));
    }

    #[test]
    fn a_star_matches_a_run_of_any_length_including_none() {
        assert!(matches("ab*c", "ac"));
        assert!(matches("ab*c", "abc"));
        assert!(matches("ab*c", "abbbbc"));
        assert!(!matches("ab*c", "adc"));
    }

    #[test]
    fn a_plus_needs_at_least_one() {
        assert!(!matches("ab+c", "ac"));
        assert!(matches("ab+c", "abc"));
    }

    #[test]
    fn a_question_mark_makes_what_precedes_it_optional() {
        assert!(matches("colou?r", "color"));
        assert!(matches("colou?r", "colour"));
    }

    #[test]
    fn alternation_matches_any_of_its_branches() {
        assert!(matches("cat|dog", "a dog"));
        assert!(matches("cat|dog", "a cat"));
        assert!(!matches("cat|dog", "a bird"));
    }

    #[test]
    fn a_group_can_be_repeated_as_a_unit() {
        assert!(matches("(ab)+c", "ababc"));
        assert!(!matches("(ab)+c", "abac"));
    }

    #[test]
    fn alternation_inside_a_group_binds_to_the_group() {
        assert!(matches("gr(a|e)y", "grey"));
        assert!(matches("gr(a|e)y", "gray"));
        assert!(!matches("gr(a|e)y", "groy"));
    }

    #[test]
    fn a_character_class_matches_any_member() {
        assert!(matches("[abc]x", "bx"));
        assert!(!matches("[abc]x", "dx"));
    }

    #[test]
    fn a_class_range_covers_its_endpoints() {
        assert!(matches("[a-f]", "f"));
        assert!(matches("[a-f]", "a"));
        assert!(!matches("[a-f]", "g"));
    }

    #[test]
    fn a_negated_class_matches_what_it_excludes() {
        assert!(matches("[^0-9]", "a"));
        assert!(!matches("[^0-9]", "7"));
    }

    #[test]
    fn a_shorthand_class_matches_its_set() {
        assert!(matches(r"\d\d", "x42"));
        assert!(!matches(r"\d\d", "x4y"));
        assert!(matches(r"\w+", "name_1"));
        assert!(matches(r"a\sb", "a b"));
        assert!(!matches(r"a\sb", "ab"));
    }

    #[test]
    fn a_negated_shorthand_matches_the_complement() {
        assert!(matches(r"\D", "a"));
        assert!(!matches(r"\D", "5"));
        assert!(matches(r"\S", "a"));
        assert!(!matches(r"\S", " "));
    }

    #[test]
    fn a_shorthand_can_be_a_member_of_a_class() {
        assert!(matches(r"[\d_]+", "4_2"));
        assert!(!matches(r"[\d_]", "a"));
        assert!(matches(r"[^\d]", "a"));
        assert!(!matches(r"[^\d]", "4"));
    }

    #[test]
    fn anchors_tie_a_match_to_the_ends_of_the_line() {
        assert!(matches("^fn ", "fn main"));
        assert!(!matches("^fn ", "  fn main"));
        assert!(matches(";$", "let x = 1;"));
        assert!(!matches(";$", "let x = 1; // trailing"));
        assert!(matches("^$", ""));
        assert!(!matches("^$", "x"));
    }

    #[test]
    fn a_word_boundary_matches_between_a_word_and_what_is_not_one() {
        assert!(matches(r"\bcat\b", "the cat sat"));
        assert!(!matches(r"\bcat\b", "concatenate"));
        assert!(matches(r"\Bcat\B", "concatenate"));
        assert!(!matches(r"\Bcat\B", "the cat sat"));
    }

    #[test]
    fn an_escaped_metacharacter_matches_itself() {
        assert!(matches(r"a\.c", "a.c"));
        assert!(!matches(r"a\.c", "abc"));
        assert!(matches(r"\(\)", "()"));
        assert!(matches(r"2\+2", "2+2"));
        assert!(matches(r"a\\b", r"a\b"));
    }

    #[test]
    fn a_brace_is_an_ordinary_character() {
        // Counted repetition is not supported, so this is a literal and says so by matching one.
        assert!(matches("a{2}", "a{2}"));
        assert!(!matches("a{2}", "aa"));
    }

    /// Folding by lowercasing the pattern string would turn `\D` into `\d` and match the opposite
    /// of what was asked for, which is why the engine folds the comparison instead.
    #[test]
    fn a_folded_pattern_keeps_a_negated_shorthand_negated() {
        let folded = Regex::compile_folded(r"\D").expect("compiles");
        assert!(folded.matches("a"), "a letter is not a digit");
        assert!(!folded.matches("5"), "a digit matched the negation of one");

        let folded = Regex::compile_folded(r"\W+").expect("compiles");
        assert!(folded.matches("!!"));
        assert!(!folded.matches("abc"));
    }

    /// Folding widens what a class matches, so it has to widen what a negated class excludes by
    /// the same set. Testing both cases of the subject against the finished membership decision
    /// instead makes `[^a-z]` match `A`, because `a` is outside the set the negation describes.
    #[test]
    fn folding_a_negated_class_widens_what_it_excludes() {
        let folded = Regex::compile_folded("[^a-z]").expect("compiles");
        assert!(
            !folded.matches("A"),
            "an uppercase letter escaped a negated lowercase range"
        );
        assert!(!folded.matches("z"));
        assert!(
            folded.matches("9"),
            "a digit is outside the range either way"
        );

        let folded = Regex::compile_folded("[^A-Z]+").expect("compiles");
        assert!(!folded.matches("abc"));
        assert!(folded.matches("123"));
    }

    #[test]
    fn a_folded_pattern_matches_either_case() {
        let folded = Regex::compile_folded("fn Main").expect("compiles");
        assert!(folded.matches("FN MAIN"));
        assert!(folded.matches("fn main"));
        assert!(!folded.matches("fn other"));

        // A class folds too, so `[a-z]` covers an uppercase letter without being rewritten.
        let class = Regex::compile_folded("[a-z]+").expect("compiles");
        assert!(class.matches("ABC"));

        // Case-sensitive by default, so the fold is something a caller asks for.
        assert!(
            !Regex::compile("fn Main")
                .expect("compiles")
                .matches("FN MAIN")
        );
    }

    /// The pattern a real turn sent, which was refused as a repeat with nothing before it.
    #[test]
    fn an_inline_flag_ignores_case_for_the_rest_of_the_pattern() {
        let pattern = r"(?i)(personal access token|\bPAT\b|policy|policies)";
        assert!(matches(pattern, "Create a Personal Access Token"));
        assert!(matches(pattern, "a pat for CI"));
        assert!(matches(pattern, "See the POLICY page"));
        assert!(!matches(pattern, "the path to it"));
        assert!(matches("(?i)[a-z]+[0-9]", "ABC1"));
        assert!(matches(r"(?i)\bmain\b", "fn MAIN()"));
    }

    #[test]
    fn a_flag_ends_with_the_group_it_is_in() {
        assert!(matches("a((?i)b)c", "aBc"));
        assert!(!matches("a((?i)b)c", "aBC"));
        assert!(!matches("a((?i)b)c", "ABc"));
        assert!(matches("a(?i:b)c", "aBc"));
        assert!(!matches("a(?i:b)c", "aBC"));
        // Nothing before the flag is folded by it.
        assert!(!matches("a(?i)b", "Ab"));
    }

    #[test]
    fn a_flag_carries_into_the_later_branches_of_its_group() {
        assert!(matches("x(?i)a|b", "B"));
        assert!(!matches("(x(?i)a|b)|c", "C"));
    }

    #[test]
    fn a_flag_can_turn_folding_off() {
        let folded = Regex::compile_folded("a(?-i)B").expect("compiles");
        assert!(folded.matches("AB"));
        assert!(!folded.matches("Ab"));

        let scoped = Regex::compile_folded("(?-i:M)ain").expect("compiles");
        assert!(scoped.matches("MAIN"));
        assert!(!scoped.matches("main"));

        assert!(matches("(?i)a(?-i)B", "AB"));
        assert!(!matches("(?i)a(?-i)B", "Ab"));
    }

    /// The flag is recorded on each class, so it has to keep the rule the folded compile keeps:
    /// widening what a negated class matches means widening what it excludes.
    #[test]
    fn an_inline_flag_on_a_negated_class_widens_what_it_excludes() {
        assert!(!matches("(?i)[^a-z]", "A"));
        assert!(matches("(?i)[^a-z]", "9"));
        assert!(matches(r"(?i)\D", "a"));
        assert!(!matches(r"(?i)\D", "5"));
    }

    #[test]
    fn a_non_capturing_group_is_an_ordinary_group() {
        assert!(matches("(?:ab)+c", "ababc"));
        assert!(!matches("(?:ab)+c", "abac"));
        assert!(matches("gr(?:a|e)y", "grey"));
        assert!(!matches("gr(?:a|e)y", "Grey"));
    }

    #[test]
    fn a_flag_alone_cannot_be_repeated() {
        assert_eq!(
            Regex::compile("(?i)*a").unwrap_err(),
            PatternError::NothingToRepeat { at: 0 }
        );
    }

    /// Before the flag was read, `(?` fell through to a group whose first thing was a `?`, and the
    /// report blamed a repeat. Lookaround and the rest are still absent, so they are named instead.
    #[test]
    fn a_group_form_the_engine_lacks_is_named_rather_than_blamed_on_a_repeat() {
        for pattern in [
            "(?=a)", "(?!a)", "(?<=a)", "(?<!a)", "(?P<n>a)", "(?<n>a)", "(?m)^a", "(?s).",
            "(?x)a", "(?)", "(?-)", "(?-:a)", "(?i-)", "(?im)a",
        ] {
            let error = Regex::compile(pattern).unwrap_err();
            assert_eq!(error, PatternError::UnsupportedGroup { at: 0 }, "{pattern}");
            assert!(!error.to_string().contains("repeat"), "{pattern}: {error}");
        }
        assert_eq!(
            Regex::compile("ab(?=c)").unwrap_err(),
            PatternError::UnsupportedGroup { at: 2 }
        );
        assert_eq!(
            Regex::compile("(?i").unwrap_err(),
            PatternError::UnclosedGroup
        );
        assert_eq!(
            Regex::compile("(?:a").unwrap_err(),
            PatternError::UnclosedGroup
        );
    }

    #[test]
    fn a_pattern_matches_across_a_multibyte_character() {
        assert!(matches("é.", "éx"));
        assert!(matches("[é]", "é"));
        assert!(matches(".", "日"));
    }

    #[test]
    fn an_empty_alternative_branch_matches_nothing_at_all() {
        assert!(matches("a(b|)c", "ac"));
        assert!(matches("a(b|)c", "abc"));
    }

    /// The property the whole engine exists for. A backtracking implementation takes
    /// exponential time on this, and the test would not finish rather than fail.
    ///
    /// Each subject holds the literal its pattern requires and fails at the anchor after it, so
    /// the line reaches the NFA. A subject without the literal is rejected before the NFA runs and
    /// shows nothing about it.
    #[test]
    fn a_pattern_built_to_backtrack_catastrophically_still_returns_promptly() {
        let started = std::time::Instant::now();
        let before = simulations();
        let subject = format!("{}b", "a".repeat(60));
        assert!(!matches("(a+)+$b", &subject));
        assert!(!matches("(a|aa)*c$", &format!("{}cb", "a".repeat(60))));
        assert!(!matches("(x+x+)+y$", &format!("{}yz", "x".repeat(60))));
        assert_eq!(
            simulations() - before,
            3,
            "each subject has to reach the NFA for this to bound it"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "matching took {:?}, which is the backtracking blowup this engine rules out",
            started.elapsed()
        );
    }

    #[test]
    fn a_nested_star_terminates_rather_than_looping_on_the_empty_match() {
        assert!(matches("(a*)*b", "aaab"));
        assert!(matches("(a*)*", ""));
        assert!(!matches("(a*)*b", "ccc"));
        let before = simulations();
        assert!(!matches("(a*)*b$", "aaabc"));
        assert_eq!(simulations() - before, 1);
    }

    #[test]
    fn an_unclosed_group_is_reported_rather_than_guessed_at() {
        assert_eq!(
            Regex::compile("(ab").unwrap_err(),
            PatternError::UnclosedGroup
        );
        assert_eq!(
            Regex::compile("ab)").unwrap_err(),
            PatternError::UnmatchedParen
        );
    }

    #[test]
    fn an_unclosed_class_is_reported() {
        assert_eq!(
            Regex::compile("[abc").unwrap_err(),
            PatternError::UnclosedClass
        );
        assert_eq!(
            Regex::compile("[]").unwrap_err(),
            PatternError::UnclosedClass
        );
    }

    #[test]
    fn a_repeat_with_nothing_before_it_is_reported() {
        assert_eq!(
            Regex::compile("*ab").unwrap_err(),
            PatternError::NothingToRepeat { at: 0 }
        );
        assert_eq!(
            Regex::compile("^*").unwrap_err(),
            PatternError::NothingToRepeat { at: 0 }
        );
    }

    /// A bare anchor cannot be repeated, but a group holding one is an ordinary subexpression and
    /// every other engine accepts a `?` after it. Refusing the second on the strength of the
    /// first rejects patterns that are not mistakes.
    #[test]
    fn a_group_holding_an_anchor_may_still_be_repeated() {
        assert!(matches(r"(\B)?a", "a"));
        assert!(matches("(^)?a", "a"));
        assert!(matches("(a$)?b", "b"));
        assert_eq!(
            Regex::compile(r"\B*").unwrap_err(),
            PatternError::NothingToRepeat { at: 0 }
        );
    }

    #[test]
    fn a_dangling_escape_is_reported() {
        assert_eq!(
            Regex::compile(r"ab\").unwrap_err(),
            PatternError::DanglingEscape
        );
    }

    #[test]
    fn a_backwards_range_is_reported() {
        assert_eq!(
            Regex::compile("[z-a]").unwrap_err(),
            PatternError::ReversedRange {
                start: 'z',
                end: 'a'
            }
        );
    }

    #[test]
    fn a_pattern_past_the_length_cap_is_refused() {
        let long = "a".repeat(MAX_PATTERN + 1);
        assert_eq!(
            Regex::compile(&long).unwrap_err(),
            PatternError::TooLong {
                length: MAX_PATTERN + 1
            }
        );
    }

    /// Recursive descent parses the groups, so an absurdly nested pattern has to be refused
    /// rather than allowed to exhaust the stack.
    #[test]
    fn a_pattern_nested_past_the_depth_cap_is_refused() {
        let deep = format!(
            "{}a{}",
            "(".repeat(MAX_DEPTH + 1),
            ")".repeat(MAX_DEPTH + 1)
        );
        assert_eq!(Regex::compile(&deep).unwrap_err(), PatternError::TooDeep);
    }

    #[test]
    fn a_hyphen_at_either_end_of_a_class_is_a_character() {
        assert!(matches("[a-]", "-"));
        assert!(matches("[-a]", "-"));
        assert!(matches("[a-]", "a"));
    }

    /// A line holding the literal text a pattern requires is not thereby a match, so a line
    /// holding that text and not the rest still has to be tried and refused.
    #[test]
    fn a_pattern_beginning_with_a_literal_matches_only_where_the_whole_pattern_does() {
        assert!(matches("needle|haystack", "xx haystack"));
        assert!(!matches("needle|haystack", "needl hayst"));
        assert!(matches("ab+c", "zzabbbc"));
        assert!(!matches("ab+c", "zzab"));
        assert!(!matches("needle", ""));
    }

    #[test]
    fn a_folded_pattern_beginning_with_a_literal_is_found_in_either_case() {
        let folded = Regex::compile_folded("dead code|unused").expect("compiles");
        assert!(folded.matches("An UNUSED import"));
        assert!(folded.matches("Dead Code"));
        let flagged = Regex::compile("(?i)dead code|unused").expect("compiles");
        assert!(flagged.matches("An UNUSED import"));
        assert!(!matches("dead code|unused", "An UNUSED import"));
    }

    /// A literal stands for a branch only where every match of the branch holds it. Each pattern
    /// here has a branch that can match without the literal a neighbouring branch spells.
    #[test]
    fn a_branch_that_can_begin_with_something_else_is_still_matched() {
        assert!(matches("abc|x*y", "y"));
        // The repeated text is longer than the text after it, so a repeat taken to require it
        // would make it the literal looked for.
        assert!(matches("abc|(xyz)*y", "y"));
        assert!(matches("abc|.d", "zd"));
        assert!(matches(r"abc|\bd", "d"));
        assert!(matches("a?b", "b"));
        assert!(matches("(a|b*)c", "c"));
        assert!(matches("abc|", "zzz"));
        assert!(matches("^abc|def", "abc"));
        assert!(matches("^abc|def", "xdef"));
        assert!(!matches("^abc|def", "xabc"));
        assert!(matches("abc|[xy]z", "yz"));
    }

    /// The automaton is told apart from the substring search by counting its runs, which a result
    /// cannot show: both give the same answer. Each row is a pattern, a line, whether the pattern
    /// matches it, and whether the automaton had to run to say so.
    #[test]
    fn the_automaton_runs_only_for_lines_that_could_match() {
        let rows: [(&str, &str, bool, bool); 27] = [
            // Nothing but literals: finding one is the answer.
            (
                r"Command::new|\.spawn\(",
                "let c = Command::new(x);",
                true,
                false,
            ),
            (r"Command::new|\.spawn\(", "child.spawn()?;", true, false),
            (
                r"Command::new|\.spawn\(",
                "Command::ne and spawn(",
                false,
                false,
            ),
            // An escaped dot is a dot, so a line with another character there is not a match.
            (r"Command::new|\.spawn\(", "xspawn(", false, false),
            (r"Command::new|\.spawn\(", "command::new", false, false),
            ("(?i)foo|bar", "a FOO b", true, false),
            ("(?i)foo|bar", "a fo b", false, false),
            // A literal alongside other constructs rejects a line lacking it and decides nothing
            // for a line holding it.
            (r"Command::new\([^)]*\)", "Cargo::new(x)", false, false),
            (r"Command::new\([^)]*\)", "Command::new(x)", true, true),
            (r"Command::new\([^)]*\)", "Command::new(x", false, true),
            // The longer literal is the one looked for, not the one the pattern begins with.
            (r"a[0-9]*Command::new", "a1 Commands", false, false),
            (r"fn +[a-z_]+ *=", "let value = 1;", false, false),
            // Of two literals as long as each other, the one a pattern begins with is looked for,
            // not the space after it that nearly every line holds.
            (r"Z +[a-z]", "a b c d", false, false),
            (r"Z +[a-z]", "xx Z q", true, true),
            (r"fn +[a-z_]+ *=", "fn value = 1;", true, true),
            // A repeat that may run no times requires nothing of its own.
            ("x*y", "aaa", false, false),
            ("x*y", "y", true, true),
            ("(a|b)c", "bd", false, false),
            ("(a|b)c", "bc", true, true),
            // Zero-width assertions are checked by the automaton once the literal is there.
            (r"\bfoo\b", "bar", false, false),
            (r"\bfoo\b", "afoob", false, true),
            (r"\bfoo\b", "a foo b", true, true),
            ("^abc|def", "xdef", true, true),
            ("^abc|def", "xabc", false, true),
            // A branch that can match without a literal leaves no literal to require.
            ("abc|", "zzz", true, true),
            ("a|b*", "zzz", true, true),
            ("[ab]", "zzz", false, true),
        ];
        for (pattern, line, wanted, ran) in rows {
            let regex = Regex::compile(pattern).expect("compiles");
            let before = simulations();
            assert_eq!(regex.matches(line), wanted, "{pattern} on {line:?}");
            assert_eq!(
                simulations() > before,
                ran,
                "whether the automaton ran for {pattern} on {line:?}"
            );
        }
    }

    /// Every shortcut is checked against the automaton run alone, on a fresh set of buffers, over
    /// every short line of characters that fold into one another or sit on either side of the
    /// ASCII boundary. The patterns are chosen so that each way of finding the required text is
    /// used: a whole branch, a run in a sequence, a repeat, an alternation inside a sequence, a
    /// flag partway through, and an empty branch.
    #[test]
    fn the_literal_shortcuts_give_the_answer_the_automaton_gives() {
        let patterns = [
            "ab",
            "ab|ba",
            "abk|\u{e9}",
            "a+b",
            "ab+",
            "(ab)+",
            "(ab)+k",
            "a(b|k)",
            "(a|b)k",
            "a|b*",
            "a*b",
            "a?b",
            "ab?",
            "^ab|ba",
            "ab$",
            r"\bab",
            "[ab]k",
            "k.a",
            "a.|bk",
            "(a|b)(k|\u{e9})",
            "ab|",
            "(?i)ab",
            "(?i)ab|k",
            "a(?i)b",
            "(?i)a(?-i)b",
            "k(?i)b|a",
            "(?i)\u{e9}",
            "(?i)k",
            "\u{212A}a|ab",
            "a(b)",
            "(?:ab|k)+a",
        ];
        let alphabet = ['a', 'b', 'A', 'B', 'k', '\u{212A}', '\u{e9}', '\u{c9}'];
        let mut lines = vec![String::new()];
        let mut shortest = 0;
        for _ in 0..4 {
            let end = lines.len();
            for at in shortest..end {
                for letter in alphabet {
                    lines.push(format!("{}{letter}", lines[at]));
                }
            }
            shortest = end;
        }

        for folded in [false, true] {
            for pattern in patterns {
                let regex = if folded {
                    Regex::compile_folded(pattern)
                } else {
                    Regex::compile(pattern)
                }
                .expect("compiles");
                for line in &lines {
                    assert_eq!(
                        regex.matches(line),
                        by_automaton_alone(&regex, line),
                        "{pattern} (folded: {folded}) on {line:?}"
                    );
                }
            }
        }
    }

    /// A pattern that ignores case finds its text in either case without the automaton while the
    /// line is ASCII, and leaves a line with any other character to it, because such a character
    /// can fold to an ASCII one.
    #[test]
    fn a_literal_that_ignores_case_is_found_in_either_case_and_a_wider_line_goes_to_the_automaton()
    {
        let regex = Regex::compile_folded("kelvin|celsius").expect("compiles");
        let before = simulations();
        assert!(regex.matches("the KELVIN scale"));
        assert!(regex.matches("Celsius"));
        assert!(!regex.matches("the kelvi scale"));
        assert_eq!(simulations(), before);

        assert!(regex.matches("the \u{212A}elvin scale"));
        assert!(!regex.matches("the \u{e9}lvin scale"));
        assert_eq!(simulations() - before, 2);
    }

    #[test]
    fn a_pattern_of_literals_over_megabytes_of_text_finishes_promptly() {
        let source = [
            "use std::collections::HashMap;",
            "pub fn load(path: &Path) -> Result<Config, Error> {",
            "    let text = std::fs::read_to_string(path)?;",
            "    let mut child = Command::new(\"git\");",
            "    child.args([\"status\"]).spawn()?;",
            "    // lines like these begin with the common letters l, p, u, c and s",
            "    for (index, line) in text.lines().enumerate() {",
            "        if line.starts_with('#') { continue; }",
            "    }",
        ];
        let mut text = String::new();
        while text.len() < 4 * 1024 * 1024 {
            for line in source {
                text.push_str(line);
                text.push('\n');
            }
        }
        let regex = Regex::compile(
            r"Command::new|\.spawn\(|std::process::Command|webbrowser::open|open::that",
        )
        .expect("compiles");

        let started = std::time::Instant::now();
        let before = simulations();
        let hits = text.lines().filter(|line| regex.matches(line)).count();
        let elapsed = started.elapsed();
        assert_eq!(simulations(), before, "no line of this text needs the NFA");

        assert_eq!(hits, 2 * text.lines().count() / source.len());
        // The NFA takes more than ten seconds over this text in a debug build and over a second
        // in a release one, and finding the literals takes a fraction of a second in either.
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "{hits} lines of {} bytes took {elapsed:?}",
            text.len()
        );
    }

    /// The buffers are kept between lines so they are not built for each, and a long line must
    /// not leave its share of them behind for the rest of the search.
    #[test]
    fn a_very_long_line_does_not_leave_its_buffers_held() {
        let long = "a".repeat(4 * RETAINED_CHARS);
        // Holds the literal the pattern requires, so the line reaches the automaton.
        let automaton = Regex::compile("(a|b)+c$").expect("compiles");
        let before = simulations();
        assert!(!automaton.matches(&format!("{long}cd")));
        assert_eq!(simulations() - before, 1);
        assert!(SCRATCH.with_borrow(|scratch| scratch.chars.capacity()) <= RETAINED_CHARS);

        let folded = Regex::compile_folded("needle").expect("compiles");
        assert!(!folded.matches(&long));
        assert!(SCRATCH.with_borrow(|scratch| scratch.lowered.capacity()) <= RETAINED_CHARS);
    }

    /// Literals that ignore case are compared in lowercase, and the Kelvin sign (U+212A) lowercases
    /// to `k`, so it matches `k` whether the pattern or the line holds it.
    #[test]
    fn a_character_that_folds_across_the_ascii_boundary_still_matches() {
        let ascii = Regex::compile_folded("kelvin").expect("compiles");
        assert!(ascii.matches("\u{212A}elvin"));
        let kelvin = Regex::compile_folded("\u{212A}elvin").expect("compiles");
        assert!(kelvin.matches("kelvin"));
        assert!(kelvin.matches("KELVIN"));
        assert!(
            !Regex::compile("kelvin")
                .expect("compiles")
                .matches("\u{212A}elvin")
        );
    }

    #[test]
    fn a_non_ascii_letter_folds_to_its_other_case() {
        let regex = Regex::compile_folded("école").expect("compiles");
        assert!(regex.matches("L'ÉCOLE"));
        assert!(regex.matches("l'école"));
        assert!(
            !Regex::compile("école")
                .expect("compiles")
                .matches("L'ÉCOLE")
        );
        let class = Regex::compile_folded("[é]").expect("compiles");
        assert!(class.matches("É"));
    }

    #[test]
    fn a_closing_bracket_first_in_a_class_is_a_character() {
        assert!(matches("[]]", "]"));
        assert!(!matches("[]]", "a"));
    }
}
