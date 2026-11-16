//! The issue or pull request link a planner asked the session to record (SETLINK-1), and the
//! references to issues and pull requests a session met in text it may read (PANEL-16).
//!
//! The session record lives in a crate this one cannot depend on, so a link leaves a turn as the
//! kind and the text of the address, and whoever owns the record reads it as a link again before
//! keeping it.

/// Which of the session's two links a call is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Issue,
    PullRequest,
}

/// A link the planner asked to have recorded, once it was checked against what the person typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSet {
    pub kind: LinkKind,
    pub url: String,
}

/// The characters a sentence puts around an address: brackets, quotes and punctuation.
fn wraps(c: char) -> bool {
    matches!(
        c,
        '<' | '>' | '(' | ')' | '[' | ']' | '"' | '\'' | '`' | ',' | ';' | '.'
    )
}

/// Whether `url` is, as a whole, one of the words of a line the person typed.
///
/// A word is a run of non-space characters with the quotes, brackets and trailing punctuation a
/// sentence puts around an address taken off. A substring is not enough: `https://a.example/x`
/// is inside `https://a.example/xyz`, and a link the person did not write would pass.
pub fn typed_in(typed: &[String], url: &str) -> bool {
    typed.iter().any(|line| {
        line.split_whitespace()
            .any(|word| word.trim_matches(wraps) == url)
    })
}

/// The most digits an issue or pull request number may have, which is more than a repository's
/// issues have reached.
pub const MAX_DIGITS: usize = 9;

/// The longest an owner and a repository name may be on GitHub.
const MAX_OWNER: usize = 39;
const MAX_REPOSITORY: usize = 100;

/// The most references one string adds. A fixed count, taken in the order the string holds them.
pub const PER_STRING: usize = 64;

/// How much of one string is read, counted from its start. A command or a pasted log can be large.
pub const SCANNED_BYTES: usize = 64 * 1024;

/// The most references one turn collects. The first met are kept and later ones are dropped.
pub const PER_TURN: usize = 200;

/// Whether `text` is an issue or pull request number: one to [`MAX_DIGITS`] digits, the first not
/// a zero, since neither `#0` nor `#007` is the number of anything.
pub fn is_number(text: &str) -> bool {
    (1..=MAX_DIGITS).contains(&text.len())
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && !text.starts_with('0')
}

fn number_of(text: &str) -> Option<u64> {
    if is_number(text) {
        text.parse().ok()
    } else {
        None
    }
}

/// An issue or a pull request on GitHub, by the owner and repository it is in and its number.
///
/// Built only by [`Reference::new`], so the owner and the repository hold only the characters
/// GitHub allows in them and the address and shorthand it writes can carry nothing that ends a
/// terminal escape sequence (PANEL-2). Two references name the same item when
/// [`Reference::same_item`] says so, whatever kind each was read as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    owner: String,
    repo: String,
    number: u64,
    kind: LinkKind,
    /// Whether the text the reference was read from said which kind it is: an address with
    /// `issues` or `pull` in it, `gh pr` or `gh issue`, or a closing keyword. Every reader here
    /// names it, and an item whose kind was not named is one an older build wrote.
    kind_named: bool,
}

impl Reference {
    /// The reference to item `number` of `owner/repo`, of the kind the source named, or `None`
    /// where a part holds a character GitHub does not allow, or is too long, or the number is not
    /// one to [`MAX_DIGITS`] digits.
    pub fn new(owner: &str, repo: &str, number: u64, kind: LinkKind) -> Option<Self> {
        let owner_spelled = (1..=MAX_OWNER).contains(&owner.len())
            && owner
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
        let repo_spelled = (1..=MAX_REPOSITORY).contains(&repo.len())
            && !matches!(repo, "." | "..")
            && repo
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
        (owner_spelled && repo_spelled && number_of(&number.to_string()).is_some()).then(|| Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
            kind,
            kind_named: true,
        })
    }

    /// The same reference with its kind marked as a reading and not as something the text said.
    #[must_use]
    pub fn assumed(mut self) -> Self {
        self.kind_named = false;
        self
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }

    pub fn number(&self) -> u64 {
        self.number
    }

    pub fn kind(&self) -> LinkKind {
        self.kind
    }

    /// Whether the text this was read from named the kind (see [`Reference::assumed`]).
    pub fn kind_named(&self) -> bool {
        self.kind_named
    }

    /// `owner/repo#n`.
    pub fn shorthand(&self) -> String {
        format!("{}/{}#{}", self.owner, self.repo, self.number)
    }

    /// The address of the item. GitHub sends `/issues/{n}` to `/pull/{n}` where `{n}` is a pull
    /// request, so a reference whose kind was assumed still opens.
    pub fn address(&self) -> String {
        let path = match self.kind {
            LinkKind::Issue => "issues",
            LinkKind::PullRequest => "pull",
        };
        format!(
            "https://github.com/{}/{}/{path}/{}",
            self.owner, self.repo, self.number
        )
    }

    /// Whether `other` is the same item: the same owner, repository and number. GitHub reads an
    /// owner and a repository without regard to case, so `Brave/Bravebot#1` and `brave/bravebot#1`
    /// are one item. The kind is not compared.
    pub fn same_item(&self, other: &Self) -> bool {
        self.number == other.number
            && self.owner.eq_ignore_ascii_case(&other.owner)
            && self.repo.eq_ignore_ascii_case(&other.repo)
    }
}

/// Add `found` to `list` in the order met, and return what changed in it: each reference that was
/// new, and each that gained a kind its text named.
///
/// A reference already in the list keeps its place and its spelling. Its kind changes only from an
/// assumed one to a named one, and a named kind is never replaced by another. Where the list holds `cap` references a new one
/// is dropped, so the first met are the ones kept.
pub fn merge(list: &mut Vec<Reference>, found: &[Reference], cap: usize) -> Vec<Reference> {
    let mut changed = Vec::new();
    for reference in found {
        match list.iter().position(|held| held.same_item(reference)) {
            Some(at) if !list[at].kind_named && reference.kind_named => {
                list[at].kind = reference.kind;
                list[at].kind_named = true;
                changed.push(list[at].clone());
            }
            Some(_) => {}
            None if list.len() < cap => {
                list.push(reference.clone());
                changed.push(reference.clone());
            }
            None => {}
        }
    }
    changed
}

/// Whether `c` ends a word in the text [`references_in_text`] and [`references_in_command`] read:
/// white space, a bracket or a quote, or a character that ends a segment of a command.
fn ends_word(c: char) -> bool {
    c.is_whitespace() || (wraps(c) && c != '.') || matches!(c, '|' | '&')
}

/// `text` cut to [`SCANNED_BYTES`] at the end of a word, so a number the cut falls inside is
/// dropped and never shortened to a different one.
///
/// The cut is the bound where the text goes on with a character that ends a word, and otherwise the
/// last such character inside the bound. A text with none inside the bound gives nothing.
fn bounded(text: &str) -> &str {
    if text.len() <= SCANNED_BYTES {
        return text;
    }
    let mut end = SCANNED_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let (kept, rest) = text.split_at(end);
    if rest.chars().next().is_some_and(ends_word) {
        return kept;
    }
    match kept.rfind(ends_word) {
        Some(at) => kept.get(..at).unwrap_or_default(),
        None => "",
    }
}

/// The reference one word is, where it is one: a full `https://github.com/{owner}/{repo}/issues/{n}`
/// or `/pull/{n}` address. Whatever follows the number is dropped, so the reference is the item
/// and never a page under it.
fn reference_in(word: &str) -> Option<Reference> {
    let path = word.strip_prefix("https://github.com/")?;
    let mut parts = path.split('/');
    let (owner, repo, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let kind = match kind {
        "issues" => LinkKind::Issue,
        "pull" => LinkKind::PullRequest,
        _ => return None,
    };
    // Whatever follows the number in the address, a fragment or a query, is not the number.
    let number = number.split(['#', '?']).next()?;
    Reference::new(owner, repo, number_of(number)?, kind)
}

/// The words of `text`: split at white space and at the brackets and quotes a sentence puts around
/// an address, with the punctuation that ends a sentence taken off each end.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_whitespace() || (wraps(c) && c != '.'))
        .map(|word| word.trim_matches(['.', ':', '!', '?']))
        .filter(|word| !word.is_empty())
}

/// The references a string holds: a full `https://github.com/{owner}/{repo}/issues/{n}` or
/// `/pull/{n}` address, in the order met.
///
/// The kind is the one the address says, and what follows the number is dropped. The word
/// `owner/repo#n` and a bare `#n` give nothing.
///
/// This reads the string and returns what it found. It calls no model, no network and no `gh`,
/// touches no file, and nothing in the driver may take a decision from its result: it is for the
/// list a person looks at (PANEL-16). It reads the first [`SCANNED_BYTES`] and returns at most
/// [`PER_STRING`] references.
pub fn references_in_text(text: &str) -> Vec<Reference> {
    let mut found = Vec::new();
    for word in words(bounded(text)) {
        if let Some(reference) = reference_in(word) {
            merge(&mut found, &[reference], PER_STRING);
        }
    }
    found
}

/// The `gh pr` and `gh issue` subcommands that take the number of the item they work on.
const NUMBERED: [&str; 17] = [
    "view", "comment", "checkout", "diff", "checks", "merge", "review", "close", "reopen", "edit",
    "ready", "develop", "lock", "unlock", "pin", "unpin", "transfer",
];

/// The words that make the item that follows them an issue the text closes.
const CLOSING: [&str; 9] = [
    "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved",
];

/// The repository a segment of a command names: `--repo o/r`, `--repo=o/r`, `-R o/r` or
/// `GH_REPO=o/r`. The outer `None` says it names none. The inner one says it names one that is not
/// on GitHub, so a number in the segment is not given the default's.
fn repository_named_in<'a>(words: &[&'a str]) -> Option<Option<(&'a str, &'a str)>> {
    for (at, word) in words.iter().enumerate() {
        let value = if let Some(value) = word
            .strip_prefix("--repo=")
            .or_else(|| word.strip_prefix("GH_REPO="))
        {
            value
        } else if matches!(*word, "--repo" | "-R") {
            let Some(value) = words.get(at + 1) else {
                continue;
            };
            value
        } else if let Some(value) = word.strip_prefix("-R") {
            value
        } else {
            continue;
        };
        let parts: Vec<&str> = value.split('/').collect();
        let named = match parts[..] {
            [owner, repo] => Some((owner, repo)),
            [host, owner, repo] if host.eq_ignore_ascii_case("github.com") => Some((owner, repo)),
            _ => None,
        };
        return Some(named);
    }
    None
}

/// The number a `gh pr` or `gh issue` subcommand works on, from the words after the subcommand.
///
/// It is the first word, or the first one after `-R x`, `--repo x`, `--repo=x` and `-Rx`. Any other
/// word before it, such as a flag and its value, means the words name no number: in
/// `gh pr checks --interval 30` the `30` is not an item.
fn number_after(words: &[&str]) -> Option<u64> {
    let mut rest = words.iter();
    while let Some(word) = rest.next() {
        if let Some(number) = number_of(word) {
            return Some(number);
        }
        match *word {
            "--repo" | "-R" => {
                rest.next();
            }
            _ if word.starts_with("--repo=") || word.starts_with("-R") => {}
            _ => return None,
        }
    }
    None
}

/// The references a command line holds: those [`references_in_text`] finds, the item of
/// `gh pr <subcommand> <n>` and `gh issue <subcommand> <n>`, and the item a closing keyword
/// followed by `#n` or a full address names, as in a commit message.
///
/// The command is cut into segments at `&&`, `||`, `;`, `|` and a new line, and a number takes its
/// repository from the segment it is in: `--repo`, `--repo=`, `-R` or `GH_REPO=`, else
/// `default_repository`, else the number names nothing. A prose `#12` with no `gh` word or closing
/// keyword before it is not read. Quoting is not parsed beyond taking a quote off the ends of a
/// word, so a quoted `;` cuts a segment short.
///
/// This reads the string and returns what it found. It decides nothing about the command: whether
/// it runs, what it is asked about or what the planner is told is settled without it. It reads the
/// first [`SCANNED_BYTES`] and returns at most [`PER_STRING`] references.
pub fn references_in_command(
    line: &str,
    default_repository: Option<(&str, &str)>,
) -> Vec<Reference> {
    let mut found = Vec::new();
    for segment in bounded(line).split(['\n', ';', '|', '&']) {
        merge(&mut found, &references_in_text(segment), PER_STRING);
        let words: Vec<&str> = segment
            .split_whitespace()
            .map(|word| word.trim_matches(['"', '\'']))
            .collect();
        let repository = match repository_named_in(&words) {
            Some(named) => named,
            None => default_repository,
        };
        let mut segment_found = Vec::new();
        for (at, word) in words.iter().enumerate() {
            if word.rsplit('/').next() != Some("gh") {
                continue;
            }
            let kind = match words.get(at + 1) {
                Some(&"pr") => LinkKind::PullRequest,
                Some(&"issue") => LinkKind::Issue,
                _ => continue,
            };
            if !words
                .get(at + 2)
                .is_some_and(|subcommand| NUMBERED.contains(subcommand))
            {
                continue;
            }
            let number = number_after(&words[at + 3..]);
            if let (Some(number), Some((owner, repo))) = (number, repository) {
                segment_found.extend(Reference::new(owner, repo, number, kind));
            }
        }
        for pair in words.windows(2) {
            let keyword = pair[0].trim_end_matches(':');
            if !CLOSING
                .iter()
                .any(|closing| closing.eq_ignore_ascii_case(keyword))
            {
                continue;
            }
            let item = pair[1].trim_matches(|c: char| wraps(c) || matches!(c, ':' | '!' | '?'));
            if let Some(bare) = item.strip_prefix('#') {
                if let (Some(number), Some((owner, repo))) = (number_of(bare), repository) {
                    segment_found.extend(Reference::new(owner, repo, number, LinkKind::Issue));
                }
            } else if let Some(reference) = reference_in(item) {
                segment_found.push(reference);
            }
        }
        merge(&mut found, &segment_found, PER_STRING);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(all: &[&str]) -> Vec<String> {
        all.iter().map(|line| line.to_string()).collect()
    }

    #[test]
    fn a_whole_word_of_a_typed_line_is_found() {
        let typed = lines(&["this is for https://github.com/brave/bravebot/issues/1267."]);
        assert!(typed_in(
            &typed,
            "https://github.com/brave/bravebot/issues/1267"
        ));
    }

    #[test]
    fn an_address_wrapped_in_brackets_or_quotes_is_found() {
        let typed = lines(&["see (<https://a.example/x>), and \"https://b.example/y\""]);
        assert!(typed_in(&typed, "https://a.example/x"));
        assert!(typed_in(&typed, "https://b.example/y"));
    }

    #[test]
    fn the_start_of_a_longer_address_is_not_found() {
        let typed = lines(&["https://a.example/xyz"]);
        assert!(!typed_in(&typed, "https://a.example/x"));
    }

    #[test]
    fn an_address_nobody_typed_is_not_found() {
        assert!(!typed_in(&lines(&["fix the build"]), "https://a.example/x"));
        assert!(!typed_in(&[], "https://a.example/x"));
    }

    fn item(reference: &Reference) -> String {
        format!(
            "{} {:?} named={}",
            reference.shorthand(),
            reference.kind(),
            reference.kind_named()
        )
    }

    fn items(found: &[Reference]) -> Vec<String> {
        found.iter().map(item).collect()
    }

    /// The regression it rejects: a constructor that takes any text, so a name holding a slash, an
    /// escape character or a space reaches the screen and the address a click opens.
    #[test]
    fn a_reference_is_built_only_from_what_github_allows_in_a_name_and_a_number() {
        let issue = LinkKind::Issue;
        assert!(Reference::new("brave", "brave-core", 42, issue).is_some());
        assert!(Reference::new("o-1", "my_repo.v2", 999_999_999, issue).is_some());
        for (owner, repo, number) in [
            ("", "r", 1),
            ("o", "", 1),
            ("o/x", "r", 1),
            ("o", "r/x", 1),
            ("o\u{1b}", "r", 1),
            ("o", "r r", 1),
            ("o", "r\u{7}", 1),
            ("o", ".", 1),
            ("o", "..", 1),
            ("o.x", "r", 1),
            ("o_x", "r", 1),
            ("\u{e9}", "r", 1),
            ("o", "r", 0),
            ("o", "r", 1_000_000_000),
        ] {
            assert!(
                Reference::new(owner, repo, number, issue).is_none(),
                "{owner:?} {repo:?} {number} was accepted"
            );
        }
        assert!(Reference::new(&"o".repeat(39), "r", 1, issue).is_some());
        assert!(Reference::new(&"o".repeat(40), "r", 1, issue).is_none());
        assert!(Reference::new("o", &"r".repeat(100), 1, issue).is_some());
        assert!(Reference::new("o", &"r".repeat(101), 1, issue).is_none());
    }

    /// The regression it rejects: an address built from the kind alone, which sends a pull request
    /// to `/issues/` or an issue to `/pull/`, and a shorthand that drops the owner.
    #[test]
    fn a_reference_writes_its_shorthand_and_the_address_of_its_kind() {
        let issue = Reference::new("brave", "bravebot", 1267, LinkKind::Issue).unwrap();
        let pull = Reference::new("brave", "bravebot", 12, LinkKind::PullRequest).unwrap();
        assert_eq!(issue.shorthand(), "brave/bravebot#1267");
        assert_eq!(
            issue.address(),
            "https://github.com/brave/bravebot/issues/1267"
        );
        assert_eq!(pull.address(), "https://github.com/brave/bravebot/pull/12");
    }

    /// The regression it rejects: comparing the kind or the case, which lists one item twice.
    #[test]
    fn two_references_are_the_same_item_by_owner_repository_and_number() {
        let a = Reference::new("Brave", "BraveBot", 5, LinkKind::Issue).unwrap();
        let b = Reference::new("brave", "bravebot", 5, LinkKind::PullRequest).unwrap();
        let c = Reference::new("brave", "bravebot", 6, LinkKind::Issue).unwrap();
        let d = Reference::new("brave", "brave-core", 5, LinkKind::Issue).unwrap();
        assert!(a.same_item(&b));
        assert!(!a.same_item(&c));
        assert!(!a.same_item(&d));
    }

    /// The regression it rejects: a reader that takes the kind from the number's position, or that
    /// stops at the end of the number and so cannot read an address with a path after it.
    #[test]
    fn a_full_address_gives_the_kind_its_path_names_and_what_follows_the_number_is_ignored() {
        let found = references_in_text(
            "see https://github.com/brave/bravebot/issues/1267 and \
             https://github.com/brave/bravebot/pull/12/files, then \
             https://github.com/brave/bravebot/issues/9#issuecomment-1 and \
             https://github.com/brave/bravebot/pull/3?diff=split.",
        );
        assert_eq!(
            items(&found),
            [
                "brave/bravebot#1267 Issue named=true",
                "brave/bravebot#12 PullRequest named=true",
                "brave/bravebot#9 Issue named=true",
                "brave/bravebot#3 PullRequest named=true",
            ]
        );
    }

    /// The regression it rejects: a reader that takes any `#n`, or any word with a slash and a
    /// hash, so every number in prose and every path with a fragment becomes a reference.
    #[test]
    fn a_word_that_is_not_a_reference_gives_nothing() {
        for text in [
            "#12",
            "(#12)",
            "12",
            "Add the thing (#12)",
            "src/main.rs#L12",
            "docs/specs/a.md#12",
            "https://example.com/a/b#5",
            "https://example.com/o/r/issues/5",
            "http://github.com/o/r/issues/5",
            "https://github.com/o/r/commit/5",
            "https://github.com/o/r/issues/",
            "https://github.com/o/r/issues/5x",
            "https://github.com/o/r/issues/0",
            "https://github.com/o/r/issues/1234567890",
            "github.com/o/r#5",
            "o/r#0",
            "o/r#",
            "o//r#5",
            "owner/repo#12",
        ] {
            assert!(
                references_in_text(text).is_empty(),
                "{text:?} gave {:?}",
                references_in_text(text)
            );
        }
    }

    /// The regression it rejects: a reader of the loose word `owner/repo#n`, which lists the path
    /// `src/main.rs#12` as an issue of a repository named `main.rs`, and a reader of an address
    /// that keeps what follows the number, which lists a page under the item.
    #[test]
    fn only_a_full_address_is_read_and_it_gives_the_base_item() {
        for text in [
            "src/main.rs#12",
            "o/r#5",
            "fix lib/util.js#12",
            "vercel/next.js#5",
        ] {
            assert!(
                references_in_text(text).is_empty(),
                "{text:?} gave {:?}",
                references_in_text(text)
            );
        }
        for kind in ["issues", "pull"] {
            let base = format!("https://github.com/o/r/{kind}/5");
            for tail in ["", "/files", "?x=1", "#issuecomment-1", "/files?x=1#a"] {
                let found = references_in_text(&format!("see {base}{tail} now"));
                assert_eq!(found.len(), 1, "{base}{tail}");
                assert_eq!(found[0].address(), base, "{base}{tail}");
            }
        }
    }

    /// The regression it rejects: a reader that takes a word whole, so a reference in brackets, in
    /// a markdown link, in quotes or before a colon or a full stop is missed.
    #[test]
    fn what_a_sentence_puts_around_a_reference_is_taken_off() {
        let a = "https://github.com/o/r/issues";
        let found = references_in_text(&format!(
            "({a}/1) \"{a}/2\", [#3](https://github.com/o/r/pull/3). {a}/4: {a}/5! `{a}/6`,"
        ));
        assert_eq!(
            found.iter().map(Reference::number).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6]
        );
        assert_eq!(found[2].kind(), LinkKind::PullRequest);
    }

    /// The regression it rejects: no bound on how much of a string is read, or a cut that lands
    /// inside a character, which panics on a pasted log of non-ASCII text.
    #[test]
    fn only_the_first_part_of_a_long_string_is_read_and_the_cut_falls_on_a_character() {
        let tail = format!("{} {AN_ITEM}", "x ".repeat(SCANNED_BYTES));
        assert!(references_in_text(&tail).is_empty());
        let head = format!("{AN_ITEM} {}", "x ".repeat(SCANNED_BYTES));
        assert_eq!(references_in_text(&head).len(), 1);
        // `\u{e9}` is two bytes, and the bound falls between them.
        let wide = format!("{}\u{e9} {AN_ITEM}", "a".repeat(SCANNED_BYTES - 1));
        assert!(references_in_text(&wide).is_empty());
        assert!(references_in_command(&wide, None).is_empty());
    }

    const AN_ITEM: &str = "https://github.com/o/r/issues/7";
    const AN_ISSUES_PATH: &str = "https://github.com/o/r/issues/";

    /// The regression it rejects: a cut inside a number, which lists issue 1 for issue 123456, a
    /// different item.
    #[test]
    fn a_word_the_cut_falls_inside_is_dropped_and_not_shortened() {
        // The bound falls after the first digit of the number.
        let pad = SCANNED_BYTES - AN_ISSUES_PATH.len() - 1;
        let inside = format!("{} {AN_ISSUES_PATH}123456", "x".repeat(pad - 1));
        assert!(inside.len() > SCANNED_BYTES);
        assert!(references_in_text(&inside).is_empty());
        assert!(references_in_command(&inside, None).is_empty());
        // The same word ending exactly at the bound is whole, and is read.
        let whole_word = format!("{AN_ISSUES_PATH}12");
        let pad = SCANNED_BYTES - whole_word.len() - 1;
        let whole = format!("{} {whole_word} {AN_ISSUES_PATH}34", "x".repeat(pad));
        assert!(whole.len() > SCANNED_BYTES);
        let found = references_in_text(&whole);
        assert_eq!(found.len(), 1, "{whole:.0}");
        assert_eq!(found[0].number(), 12);
        // A line with no white space at all is cut at a separator and not inside a word.
        let packed = format!(
            "{}{AN_ITEM};{AN_ISSUES_PATH}123456",
            ";".repeat(SCANNED_BYTES - AN_ITEM.len() - 1 - AN_ISSUES_PATH.len() - 1)
        );
        assert!(packed.len() > SCANNED_BYTES);
        assert_eq!(
            references_in_command(&packed, None)
                .iter()
                .map(Reference::number)
                .collect::<Vec<_>>(),
            [7]
        );
    }

    /// The regression it rejects: no bound on how many a string adds, which lets one pasted list
    /// of numbers fill the session's list.
    #[test]
    fn one_string_adds_at_most_sixty_four_references_and_the_first_are_kept() {
        let many: String = (1..=100)
            .map(|n| format!("https://github.com/o/r/issues/{n} "))
            .collect();
        let found = references_in_text(&many);
        assert_eq!(found.len(), PER_STRING);
        assert_eq!(found[0].number(), 1);
        assert_eq!(found[PER_STRING - 1].number(), PER_STRING as u64);
    }

    /// The regression it rejects: a list that holds a sighting for every mention, or that lets a
    /// later reading override a kind the text named.
    #[test]
    fn a_list_holds_each_item_once_and_a_named_kind_settles_an_assumed_one_only() {
        let mut list = Vec::new();
        let assumed: Vec<Reference> = references_in_text("https://github.com/o/r/issues/5")
            .into_iter()
            .map(Reference::assumed)
            .collect();
        let pull = references_in_text("https://github.com/O/R/pull/5");
        let issue = references_in_text("https://github.com/o/r/issues/5");
        let other: Vec<Reference> = references_in_text("https://github.com/o/r/issues/6")
            .into_iter()
            .map(Reference::assumed)
            .collect();

        let changed = merge(&mut list, &assumed, 10);
        assert_eq!(items(&changed), ["o/r#5 Issue named=false"]);
        let changed = merge(&mut list, &pull, 10);
        assert_eq!(items(&changed), ["o/r#5 PullRequest named=true"]);
        assert!(merge(&mut list, &issue, 10).is_empty());
        assert!(merge(&mut list, &assumed, 10).is_empty());
        assert_eq!(merge(&mut list, &other, 10).len(), 1);
        assert_eq!(
            items(&list),
            ["o/r#5 PullRequest named=true", "o/r#6 Issue named=false"]
        );
    }

    /// The regression it rejects: dropping the oldest to make room, which lets a flood of later
    /// mentions push out what the person named first.
    #[test]
    fn a_full_list_keeps_what_it_holds_and_drops_the_rest() {
        let mut list = Vec::new();
        let a = "https://github.com/o/r/issues";
        let assumed: Vec<Reference> = references_in_text(&format!("{a}/1 {a}/2"))
            .into_iter()
            .map(Reference::assumed)
            .collect();
        merge(&mut list, &assumed, 2);
        let changed = merge(&mut list, &references_in_text(&format!("{a}/3")), 2);
        assert!(changed.is_empty());
        assert_eq!(
            list.iter().map(Reference::number).collect::<Vec<_>>(),
            [1, 2]
        );
        // An item already held still gains its kind at the cap.
        let changed = merge(
            &mut list,
            &references_in_text("https://github.com/o/r/pull/2"),
            2,
        );
        assert_eq!(changed.len(), 1);
    }

    fn command(line: &str, default: Option<(&str, &str)>) -> Vec<String> {
        items(&references_in_command(line, default))
    }

    /// The regression it rejects: a reader that does not know `gh`, or that takes the number of a
    /// subcommand that is not about one item, or the wrong kind for `gh issue`.
    #[test]
    fn the_item_a_gh_command_works_on_is_read_with_the_kind_of_its_noun() {
        assert_eq!(
            command("gh pr view 123 --repo brave/bravebot", None),
            ["brave/bravebot#123 PullRequest named=true"]
        );
        assert_eq!(
            command("gh issue comment 5 -R o/r --body ok", None),
            ["o/r#5 Issue named=true"]
        );
        assert_eq!(
            command("gh pr checkout --repo=o/r 7", None),
            ["o/r#7 PullRequest named=true"]
        );
        assert_eq!(
            command("GH_REPO=o/r gh issue close 8", None),
            ["o/r#8 Issue named=true"]
        );
        assert_eq!(
            command("/usr/local/bin/gh pr merge 9 --repo o/r", None),
            ["o/r#9 PullRequest named=true"]
        );
        assert_eq!(
            command("gh pr view \"10\" --repo \"o/r\"", None),
            ["o/r#10 PullRequest named=true"]
        );
        assert_eq!(
            command("gh pr view 11 --repo github.com/o/r", None),
            ["o/r#11 PullRequest named=true"]
        );
        for line in [
            "gh pr list --repo o/r --limit 30",
            "gh pr create --repo o/r --title x",
            "gh issue create --repo o/r --title x",
            "gh issue status",
            "gh pr view --repo o/r",
            "gh repo view 5 --repo o/r",
            "git commit -m 'fix the build'",
        ] {
            assert!(command(line, None).is_empty(), "{line}");
        }
    }

    /// The regression it rejects: the first all-digit word anywhere after the subcommand, which
    /// lists `brave/bravebot#30` for the `30` of `--interval 30`.
    #[test]
    fn a_number_is_read_only_first_or_after_a_repository_flag() {
        for line in [
            "gh pr checks --watch --interval 30 --repo brave/bravebot",
            "gh pr view --web 5 --repo o/r",
            "gh pr diff --color always 5 --repo o/r",
            "gh pr checks --interval 30 5 --repo o/r",
        ] {
            assert!(command(line, None).is_empty(), "{line}");
        }
        for (line, want) in [
            ("gh pr view --repo o/r 5", "o/r#5 PullRequest named=true"),
            ("gh pr view -R o/r 5", "o/r#5 PullRequest named=true"),
            ("gh pr view --repo=o/r 5", "o/r#5 PullRequest named=true"),
            ("gh pr view -Ro/r 5", "o/r#5 PullRequest named=true"),
            (
                "gh pr view 5 --watch --repo o/r",
                "o/r#5 PullRequest named=true",
            ),
        ] {
            assert_eq!(command(line, None), [want], "{line}");
        }
    }

    /// D1. The regression it rejects: a number with no repository listed under a guess, or the
    /// default taking the place of a repository the command named.
    #[test]
    fn a_bare_number_takes_the_commands_repository_then_the_default_and_otherwise_nothing() {
        let default = Some(("brave", "bravebot"));
        assert!(command("gh pr view 123", None).is_empty());
        assert_eq!(
            command("gh pr view 123", default),
            ["brave/bravebot#123 PullRequest named=true"]
        );
        assert_eq!(
            command("gh pr view 123 --repo o/r", default),
            ["o/r#123 PullRequest named=true"]
        );
        // The repository of another segment of the line is not this segment's.
        assert_eq!(
            command("gh pr view 5 --repo a/b && gh issue view 6", default),
            [
                "a/b#5 PullRequest named=true",
                "brave/bravebot#6 Issue named=true"
            ]
        );
        assert_eq!(
            command("gh pr view 5 && gh issue view 6 --repo c/d", None),
            ["c/d#6 Issue named=true"]
        );
    }

    /// The regression it rejects: a repository on another host, or one that is not spelled as a
    /// repository, falling back to the default and so naming an item of the wrong repository.
    #[test]
    fn a_repository_that_is_not_one_on_github_is_not_replaced_by_the_default() {
        let default = Some(("brave", "bravebot"));
        for line in [
            "gh pr view 5 --repo ghe.example.com/o/r",
            "gh pr view 5 --repo o",
            "gh pr view 5 --repo=o/r/extra",
            "gh pr view 5 -R o/r%",
        ] {
            assert!(command(line, default).is_empty(), "{line}");
        }
    }

    /// The regression it rejects: reading every `#n` of a commit message, which lists each number
    /// a person types, reading the loose word `owner/repo#n` after a keyword, and not reading the
    /// forms GitHub closes an issue by.
    #[test]
    fn a_closing_keyword_names_an_issue_and_a_number_with_no_keyword_names_nothing() {
        let default = Some(("brave", "bravebot"));
        assert_eq!(
            command("git commit -m \"Fix the build. Closes #12\"", default),
            ["brave/bravebot#12 Issue named=true"]
        );
        assert_eq!(
            command("git commit -m \"fixes: #13\"", default),
            ["brave/bravebot#13 Issue named=true"]
        );
        assert_eq!(
            command(
                "git commit -m \"Closes https://github.com/brave/brave-core/issues/77\"",
                None
            ),
            ["brave/brave-core#77 Issue named=true"]
        );
        assert!(command("git commit -m \"Closes o/r#5\"", None).is_empty());
        assert!(command("git commit -m \"Closes o/r#5\"", default).is_empty());
        assert!(command("git commit -m \"Closes #12\"", None).is_empty());
        assert!(command("git commit -m \"Add the thing (#12)\"", default).is_empty());
        assert!(command("git commit -m \"see #12 and #13\"", default).is_empty());
        assert!(command("echo 12 #13", default).is_empty());
    }

    /// The regression it rejects: reading a `gh` form only, so an address given to another program
    /// is lost.
    #[test]
    fn an_address_in_any_argument_of_a_command_is_read() {
        assert_eq!(
            command(
                "curl -s https://github.com/o/r/issues/5 | head && open https://github.com/o/r/pull/6",
                None
            ),
            ["o/r#5 Issue named=true", "o/r#6 PullRequest named=true"]
        );
    }

    /// The regression it rejects: a command that names an item twice, or in two forms, listing it
    /// twice or settling on the less certain kind.
    #[test]
    fn a_command_that_names_an_item_in_two_forms_lists_it_once_with_the_named_kind() {
        assert_eq!(
            command(
                "gh pr view https://github.com/o/r/pull/5; gh pr view 5 --repo o/r",
                None
            ),
            ["o/r#5 PullRequest named=true"]
        );
    }

    /// The regression it rejects: a command reader that works from the shell's own parser or its
    /// result, so a line the shell would refuse is not read. The reader takes a string and
    /// returns a list, and reads nothing else.
    #[test]
    fn a_line_that_is_not_valid_shell_is_still_read_as_words() {
        assert_eq!(
            command("gh pr view 5 --repo o/r \"unterminated ( ) $(", None),
            ["o/r#5 PullRequest named=true"]
        );
    }
}
