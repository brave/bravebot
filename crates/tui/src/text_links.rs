//! Web addresses and file paths in the assistant's words, found on the screen so a click can open
//! them (MOUSE-40, MOUSE-42).
//!
//! What the matcher marks and what a click opens are the same cells. The matcher
//! ([`crate::markdown`]) marks the characters of an address with [`ADDRESS_MARK`] and of a path
//! with [`PATH_MARK`] as it styles a line, and [`scan`] reads the marks off the frame once the
//! widget has wrapped the lines, since only then is it known which cells they landed in. What a
//! click opens is the characters those cells show, joined across the rows of one wrapped line and
//! nothing else: it is never a target recorded beside the text, and an escape sequence that links a
//! cell elsewhere ([`crate::hyperlink`]) is not read at all.
//!
//! Finding either in released text is a parse for display. The assistant's words are drawn for a
//! person and decide nothing here: the mark is styling, the scan records where a click would land,
//! and what opens an address or a file is the person's click on cells they can see. The driver
//! reads none of it. A path is not looked for on the file system here: whether it names a file is
//! asked when it is opened, by the code that opens it.

use crate::opener;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

/// How the matcher marks the characters of a web address, for [`scan`] to find on the screen.
///
/// A blink the terminal is never sent: [`scan`] takes it off every cell it finds and underlines the
/// cells of an address it keeps. It is a bit nothing else in the interface sets, and it is not
/// [`crate::issue_link::MARK`], which would make the digits after a `#` in an address read as an
/// issue number.
pub const ADDRESS_MARK: Modifier = Modifier::SLOW_BLINK;

/// How the matcher marks the characters of a file path, taken off the cells by [`scan`] as
/// [`ADDRESS_MARK`] is. Not drawn: whether a path names a file is not known until it is opened, and
/// an underline under a name that opens nothing would read as a link that is broken.
pub const PATH_MARK: Modifier = Modifier::RAPID_BLINK;

/// What the matcher is asked to mark in the words it styles.
///
/// A flag for each thing a click may open, so a place that draws assistant words with nothing to
/// scan them afterwards marks nothing: a mark nobody takes off would reach the terminal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    /// Underline each `#<number>` the session can link ([`crate::issue_link`]).
    pub references: bool,
    /// Mark each web address a click opens.
    pub addresses: bool,
    /// Mark each file path a click opens.
    pub paths: bool,
}

impl Marks {
    /// Only the issue and pull request numbers, for the places whose cells are not scanned.
    pub const fn references(references: bool) -> Self {
        Self {
            references,
            addresses: false,
            paths: false,
        }
    }
}

/// What a click on a link in the transcript opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A web address, as the cells show it.
    Address(String),
    /// A file path, as the cells show it, with the place in the file it was written with.
    Path(PathRef),
}

/// A path as the cells show it, taken apart: the name, and the line and column that follow it as
/// `name:line` or `name:line:column`.
///
/// Only text. Whether the name is a file, and which, is for whatever opens it to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRef {
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// A link as drawn: what it opens, and the cells it is drawn in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub target: Target,
    /// The characters of the cells, joined across the rows of one wrapped line: what a person
    /// reads, which for a path is the place after the name as it was written (`:9-12`) and not only
    /// the line it starts at.
    pub drawn: String,
    /// One entry for each row the link runs over: the row, the first column and one past the last.
    pub cells: Vec<(u16, u16, u16)>,
}

impl Found {
    /// Whether the link is drawn in the cell at `row` and `column`.
    pub fn covers(&self, row: u16, column: u16) -> bool {
        self.cells
            .iter()
            .any(|&(at, from, to)| at == row && (from..to).contains(&column))
    }
}

/// `text` from byte `from` to byte `to`, or nothing where either is out of range or inside a
/// character. Every caller here has the ends from a search of `text`, so none is.
pub(crate) fn between(text: &str, from: usize, to: usize) -> &str {
    text.get(from..to).unwrap_or("")
}

/// `text` from byte `at` on, under the same terms as [`between`].
fn from(text: &str, at: usize) -> &str {
    text.get(at..).unwrap_or("")
}

/// `text` up to byte `at`, under the same terms as [`between`].
fn before(text: &str, at: usize) -> &str {
    text.get(..at).unwrap_or("")
}

/// The schemes a click opens, which are the two [`opener::is_openable`] accepts.
const SCHEMES: [&str; 2] = ["http://", "https://"];

/// Whether an address starts at byte `at` of `text`: a scheme, at the start of the line or after a
/// blank or one of the characters that open a quotation or a bracket.
///
/// `at` is on a character boundary, and `text[at..]` is read through its first eight bytes.
pub fn starts_an_address(text: &str, at: usize) -> bool {
    let rest = &text.as_bytes()[at..];
    let scheme = SCHEMES.iter().any(|scheme| {
        rest.get(..scheme.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(scheme.as_bytes()))
    });
    scheme
        && before(text, at)
            .chars()
            .next_back()
            .is_none_or(|before| before.is_whitespace() || "*_~([<\"'".contains(before))
}

/// Where the address that starts at `at` ends, or, where it is not one, where the run of
/// characters it would have been part of ends.
///
/// The run is everything up to a blank, `<`, `>`, `"`, a backtick or the `](` of a markdown link. An
/// address is a run of printable ASCII only: one holding any other character is not an address at all, since the
/// characters after it could be a lookalike of the ones before or hide them, and the run is
/// skipped whole so the next scheme inside it is not tried again. Trailing punctuation is not part
/// of the address, and neither is a closing bracket with no opening one before it, so
/// `(https://example.test/a)` is the address inside the brackets and
/// `https://example.test/a_(b)` is the whole of it.
///
/// A run that names no host, or a user, is not an address either ([`opener::is_openable`]).
pub fn address_end(text: &str, at: usize) -> Result<usize, usize> {
    let run_end = run_end(text, at);
    let trimmed = at + trimmed_len(between(text, at, run_end));
    if opener::is_openable(between(text, at, trimmed)) {
        Ok(trimmed)
    } else {
        Err(run_end)
    }
}

/// Where the run of characters that starts at `at` ends: at a blank, `<`, `>`, `"` or a backtick, or
/// at the `]` of a `](` that closes the text of a markdown link and opens its target, so the text and
/// the target are two runs.
fn run_end(text: &str, at: usize) -> usize {
    let rest = from(text, at);
    let mut chars = rest.char_indices().peekable();
    while let Some((offset, c)) = chars.next() {
        let ends_the_text_of_a_link =
            c == ']' && chars.peek().is_some_and(|&(_, next)| next == '(');
        if c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`') || ends_the_text_of_a_link {
            return at + offset;
        }
    }
    text.len()
}

/// The most bytes a path may have, which is more than a file system takes in one name.
const MOST_PATH_BYTES: usize = 1024;

/// Whether a path may start at byte `at` of `text`: at the start of the line, or after a blank or a
/// bracket or quotation mark that opens, on a character a path may begin with.
pub fn starts_a_path(text: &str, at: usize) -> bool {
    let Some(first) = from(text, at).chars().next() else {
        return false;
    };
    let begins = first.is_ascii_alphanumeric()
        || matches!(first, '.' | '/' | '_' | '@')
        || (!first.is_ascii() && first.is_alphanumeric());
    begins
        && before(text, at)
            .chars()
            .next_back()
            .is_none_or(|before| before.is_whitespace() || "([{<\"'".contains(before))
}

/// Where the path that starts at `at` ends, or, where it is not one, where the run of characters it
/// would have been part of ends. The run is measured as an address's is, and the punctuation of the
/// sentence is taken off its end the same way, so `(src/main.rs:12).` is the path inside the
/// brackets and the line after the colon.
pub fn path_end(text: &str, at: usize) -> Result<usize, usize> {
    let run_end = run_end(text, at);
    let trimmed = at + trimmed_len(between(text, at, run_end));
    if path_of(between(text, at, trimmed)).is_some() {
        Ok(trimmed)
    } else {
        Err(run_end)
    }
}

/// A line or a column: one to six digits, and not zero.
fn number(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > 6 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|&number| number > 0)
}

/// The text before the last `:number` or `:number-number` of `text`, and the number the first of
/// them is, with whether it was the range of two.
fn peel(text: &str) -> Option<(&str, u32, bool)> {
    let (rest, tail) = text.rsplit_once(':')?;
    match tail.split_once('-') {
        Some((first, last)) => number(last).and(number(first)).map(|n| (rest, n, true)),
        None => number(tail).map(|n| (rest, n, false)),
    }
}

/// `text` as a path, with the line and the column it was written with, where it is one.
///
/// A path is made of letters and digits, `_`, `-`, `.`, `/`, `+` and `@`, in at most
/// [`MOST_PATH_BYTES`] bytes: a character outside printable ASCII is allowed where it is a letter or
/// a digit, so a file named in another script is one. It has no `..` anywhere in it (`../a.rs` and
/// `wait...what` are not names), no empty part (`//`), no leading `-` and no `@` without a `/` (an
/// address for mail is not a path). Its last part has an
/// extension: a `.` and then one to ten letters and digits with a letter among them, so `1.5.0` is
/// a version and `README.md` is a file. A one-letter extension is taken only with a `/` before it or
/// for `.c` and `.h`, so `e.g` and `i.e` are words. A place after the name is `:line`, `:line:column`
/// or `:first-last`, which is the line it begins at.
///
/// This is how the text reads and says nothing of the file system: `example.com` is a path in this
/// sense and is not a file in any directory.
pub fn path_of(text: &str) -> Option<PathRef> {
    let (base, line, column) = match peel(text) {
        None => (text, None, None),
        Some((rest, last, range)) => match peel(rest) {
            Some((before, first, false)) if !range => (before, Some(first), Some(last)),
            _ => (rest, Some(last), None),
        },
    };
    is_a_path(base).then(|| PathRef {
        path: base.to_string(),
        line,
        column,
    })
}

fn is_a_path(base: &str) -> bool {
    let made_of_path = base.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '_' | '-' | '.' | '/' | '+' | '@')
            || (!c.is_ascii() && c.is_alphanumeric())
    });
    if base.is_empty()
        || base.len() > MOST_PATH_BYTES
        || !made_of_path
        || base.starts_with('-')
        || base.contains("//")
        || base.contains("..")
        || (base.contains('@') && !base.contains('/'))
    {
        return false;
    }
    let name = base.rsplit('/').next().unwrap_or("");
    let Some(dot) = name.rfind('.') else {
        return false;
    };
    let extension = from(name, dot + 1);
    let spelled = !extension.is_empty()
        && extension.len() <= 10
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && extension.bytes().any(|byte| byte.is_ascii_alphabetic());
    spelled && (extension.len() > 1 || base.contains('/') || matches!(extension, "c" | "h"))
}

/// How much of `run` is left once the punctuation a sentence puts after an address is taken off
/// its end.
fn trimmed_len(run: &str) -> usize {
    let count = |c: char| run.matches(c).count();
    let (mut round, mut square, mut curly) = (
        count(')') as isize - count('(') as isize,
        count(']') as isize - count('[') as isize,
        count('}') as isize - count('{') as isize,
    );
    let mut end = run.len();
    // A closing bracket with more of them left than there are openers before it belongs to the
    // sentence around the address.
    let unmatched = |extra: &mut isize| {
        let extra_closer = *extra > 0;
        if extra_closer {
            *extra -= 1;
        }
        extra_closer
    };
    while let Some(last) = before(run, end).chars().next_back() {
        let drop = match last {
            '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '*' | '_' | '~' => true,
            ')' => unmatched(&mut round),
            ']' => unmatched(&mut square),
            '}' => unmatched(&mut curly),
            _ => false,
        };
        if !drop {
            break;
        }
        end -= last.len_utf8();
    }
    end
}

/// What a mark on a cell says the run it is in is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Address,
    Path,
}

/// One run of cells carrying a mark, on one row.
struct Run {
    kind: Kind,
    row: u16,
    from: u16,
    to: u16,
    text: String,
}

/// A link gathered from the runs over the rows a wrapped line took.
struct Chain {
    kind: Kind,
    text: String,
    cells: Vec<(u16, u16, u16)>,
    /// Whether the line it belongs to began above `area`, so what is drawn is the end of it.
    cut_above: bool,
}

/// Find the links in `area` of a frame the transcript has been drawn into, and make them what the
/// screen shows them as.
///
/// Every cell carrying [`ADDRESS_MARK`] or [`PATH_MARK`] is a cell of a link the matcher found. Its
/// mark is taken off, so it never reaches the terminal, and the cells of an address that is kept are
/// underlined.
///
/// A run reaching the last column of a row continues on the next when that row is one the wrap made
/// inside a word, which `continued` says (the rows of the transcript that continue the line above,
/// each with whether the wrap broke at a blank). A blank is not part of a link, so a row the wrap
/// broke at one begins a new run. A link is dropped when only part of it is in `area`: the row it
/// begins on is above it, or it reaches the last column of the last row and may go on below. Opening
/// what is drawn of one would open something nobody can see the whole of.
///
/// What is kept is read back from the cells and tested again as what opens it tests it, so the link
/// is the characters on the screen even where the matcher and the widget disagree.
pub fn scan(buffer: &mut Buffer, area: Rect, continued: &[(u16, bool)]) -> Vec<Found> {
    let area = area.intersection(buffer.area);
    let mut runs: Vec<Run> = Vec::new();
    for row in area.top()..area.bottom() {
        let mut column = area.left();
        while column < area.right() {
            let Some(kind) = kind_of(&buffer[(column, row)]) else {
                column += 1;
                continue;
            };
            let from = column;
            let mut text = String::new();
            while column < area.right() && kind_of(&buffer[(column, row)]) == Some(kind) {
                let cell = &mut buffer[(column, row)];
                text.push_str(cell.symbol());
                // A character drawn wider than a column leaves cells after it that carry no style
                // of their own and are part of it.
                column += u16::try_from(cell.symbol().width().max(1)).unwrap_or(1);
                taken(cell);
            }
            runs.push(Run {
                kind,
                row,
                from,
                to: column.min(area.right()),
                text,
            });
        }
    }

    // Looked up once for each run, so a line of many rows is not searched in full for each of them.
    let inside_a_word: std::collections::HashSet<u16> = continued
        .iter()
        .filter(|&&(_, at_blank)| !at_blank)
        .map(|&(row, _)| row)
        .collect();
    let continues_a_word = |row: u16| inside_a_word.contains(&row);
    // A wide character that does not fit in the last column goes to the next row and leaves the row
    // one cell short, so a row that ends there and is followed by a row whose first character is
    // wide is as full as one that ends in the last column.
    let first_is_wide = |run: &Run| buffer[(run.from, run.row)].symbol().width() > 1;
    let fills_the_row = |to: u16, next: &Run| {
        to == area.right() || (to.saturating_add(1) == area.right() && first_is_wide(next))
    };
    let mut chains: Vec<Chain> = Vec::new();
    for run in runs {
        let joins = run.from == area.left()
            && continues_a_word(run.row)
            && chains.last().is_some_and(|chain| {
                chain.kind == run.kind
                    && chain
                        .cells
                        .last()
                        .is_some_and(|&(row, _, to)| row + 1 == run.row && fills_the_row(to, &run))
            });
        match chains.last_mut().filter(|_| joins) {
            Some(chain) => {
                chain.text.push_str(&run.text);
                chain.cells.push((run.row, run.from, run.to));
            }
            None => chains.push(Chain {
                kind: run.kind,
                text: run.text,
                cells: vec![(run.row, run.from, run.to)],
                cut_above: run.from == area.left() && continues_a_word(run.row),
            }),
        }
    }

    let last_row = area.bottom().saturating_sub(1);
    let mut found = Vec::new();
    for chain in chains {
        let may_go_on_below = chain
            .cells
            .last()
            .is_some_and(|&(row, _, to)| row == last_row && to == area.right());
        if chain.cut_above || may_go_on_below {
            continue;
        }
        let target = match chain.kind {
            Kind::Address => opener::is_openable(&chain.text).then(|| {
                for &(row, from, to) in &chain.cells {
                    for column in from..to {
                        buffer[(column, row)]
                            .set_style(Style::default().add_modifier(Modifier::UNDERLINED));
                    }
                }
                Target::Address(chain.text.clone())
            }),
            Kind::Path => path_of(&chain.text).map(Target::Path),
        };
        if let Some(target) = target {
            found.push(Found {
                target,
                drawn: chain.text,
                cells: chain.cells,
            });
        }
    }
    found
}

/// What the mark on a cell says, if it carries one.
fn kind_of(cell: &Cell) -> Option<Kind> {
    if cell.modifier.contains(ADDRESS_MARK) {
        Some(Kind::Address)
    } else if cell.modifier.contains(PATH_MARK) {
        Some(Kind::Path)
    } else {
        None
    }
}

/// Take the marks off a cell.
fn taken(cell: &mut Cell) {
    cell.set_style(Style::default().remove_modifier(ADDRESS_MARK | PATH_MARK));
}

/// How long a click on a path waits for a second press before it is carried out, which is how long
/// a second press takes to make it part of a double click.
const WAIT: std::time::Duration = crate::select::MULTI_CLICK;

/// A click is carried out within this long of being due, or not at all: one left over from a turn
/// that began since is not something the person is waiting on.
const STALE: std::time::Duration = std::time::Duration::from_millis(2_000);

impl crate::state::Session {
    /// Hold a click on `path` until it is known not to be the first of a double click.
    ///
    /// Selecting a file name by double click is how a person copies one, and a preview made at the
    /// first press would be made for that gesture as well.
    pub fn put_off_a_click(&mut self, path: PathRef, now: std::time::Instant) {
        self.pending_click = Some((now, path));
    }

    /// Drop the click that is waiting, as any press does: the press is a second one, or the person
    /// has gone to another place.
    pub fn drop_the_click_put_off(&mut self) {
        self.pending_click = None;
    }

    /// The click that has waited long enough, taken, or `None`. One that has waited so long that
    /// something else must have happened in between is dropped, and so is one that falls due where
    /// a click on a path is no longer answered: a key can have opened the scroller or a search, or
    /// a turn can have begun, while it waited.
    pub fn take_the_click_due(&mut self, now: std::time::Instant) -> Option<PathRef> {
        let (at, _) = self.pending_click.as_ref()?;
        let waited = now.saturating_duration_since(*at);
        if waited < WAIT {
            return None;
        }
        let (_, path) = self.pending_click.take()?;
        (waited < STALE && self.answers_a_plain_click_on_a_path()).then_some(path)
    }

    /// Whether a click with nothing held on a path is answered in the state the session is in: at the
    /// plain prompt, where the file is added to the transcript the person is looking at. Asked when
    /// the click is made and again when it falls due, so the one question decides both.
    pub(crate) fn answers_a_plain_click_on_a_path(&self) -> bool {
        !self.modal() && !self.scrolling() && self.shows_a_file_here()
    }

    /// How long until a click that waits is due, for a loop that waits for the next event.
    pub fn the_click_due_in(&self, now: std::time::Instant) -> Option<std::time::Duration> {
        let (at, _) = self.pending_click.as_ref()?;
        Some(WAIT.saturating_sub(now.saturating_duration_since(*at)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The address the matcher finds in `text`, if one starts at its beginning.
    fn found_in(text: &str) -> Option<&str> {
        let end = starts_an_address(text, 0).then(|| address_end(text, 0).ok())??;
        Some(&text[..end])
    }

    /// Where an address ends is where a sentence stops using it: trailing punctuation and an
    /// unmatched closing bracket are the sentence's, and a bracket that is part of the address stays.
    #[test]
    fn an_address_stops_where_the_sentence_resumes() {
        for (text, address) in [
            ("https://example.test/a", "https://example.test/a"),
            ("https://example.test/a.", "https://example.test/a"),
            ("https://example.test/a,", "https://example.test/a"),
            ("https://example.test/a?", "https://example.test/a"),
            ("https://example.test/a).", "https://example.test/a"),
            ("https://example.test/a_(b)", "https://example.test/a_(b)"),
            ("https://example.test/a_(b)).", "https://example.test/a_(b)"),
            ("https://example.test/a]", "https://example.test/a"),
            ("https://example.test/a[1]", "https://example.test/a[1]"),
            ("https://example.test/a**", "https://example.test/a"),
            ("https://example.test/a_b_c", "https://example.test/a_b_c"),
            (
                "https://example.test/?q=1&r=2#frag",
                "https://example.test/?q=1&r=2#frag",
            ),
            ("HTTP://EXAMPLE.TEST/A", "HTTP://EXAMPLE.TEST/A"),
            ("https://example.test/a b", "https://example.test/a"),
            ("https://example.test/a>", "https://example.test/a"),
            ("https://example.test/a\"b", "https://example.test/a"),
            ("https://127.0.0.1:8080/x", "https://127.0.0.1:8080/x"),
        ] {
            assert_eq!(found_in(text), Some(address), "{text:?}");
        }
    }

    /// An address is where a scheme starts a word. A scheme inside another word, a different
    /// scheme and a run with no host are not.
    #[test]
    fn only_a_scheme_that_starts_a_word_starts_an_address() {
        for text in [
            "xhttps://example.test/",
            "a-https://example.test/",
            "ftp://example.test/",
            "https://",
            "https:// example.test",
            "https:///etc",
            "http:/example.test",
            "example.test/a",
        ] {
            assert_eq!(found_in(text), None, "{text:?}");
        }
        for before in ["", " ", "(", "[", "<", "\"", "'", "*", "_", "~"] {
            let text = format!("{before}https://example.test/");
            let at = before.len();
            assert!(starts_an_address(&text, at), "{before:?}");
        }
        assert!(!starts_an_address("a-https://example.test/", 2));
    }

    /// A character that is not printable ASCII anywhere in the run makes the whole run not an
    /// address, and says where the run ended so the next scheme inside it is not tried again.
    #[test]
    fn a_run_with_anything_but_printable_ascii_is_not_an_address() {
        for text in [
            "https://example.test/\u{e9}",
            "https://exampl\u{e9}.test/",
            "https://example.test/a\u{200b}b",
            "https://example.test/\u{202e}fdp.exe",
            "https://example.test/\u{1b}[31m",
            "https://example.test/\u{7f}",
            "https://example.test/\u{2026}",
        ] {
            assert_eq!(address_end(text, 0), Err(text.len()), "{text:?}");
        }
    }

    /// A user in front of the host is the part a person reads first and is not the host, so an
    /// address naming one is not an address.
    #[test]
    fn an_address_naming_a_user_is_not_an_address() {
        for text in [
            "https://github.com@example.test/",
            "https://user:password@example.test/",
            "http://a@b/",
        ] {
            assert!(address_end(text, 0).is_err(), "{text:?}");
        }
    }

    /// The text being scanned arrived through a turn, so a run built to be found again at every
    /// scheme inside it still ends: one failed run is skipped whole by the caller.
    #[test]
    fn a_run_of_schemes_is_measured_in_one_pass() {
        let text = format!("{}\u{e9}", "http://".repeat(50_000));
        assert_eq!(address_end(&text, 0), Err(text.len()));
        let punctuation = format!("https://a{}", ")".repeat(50_000));
        assert_eq!(address_end(&punctuation, 0), Ok("https://a".len()));
    }

    /// The address a click opened goes to the opener as the cells showed it, and where no browser
    /// could be started the person is told so without the address, which is text the assistant
    /// wrote and would read as the session's own sentence.
    #[test]
    fn a_browser_that_could_not_be_started_is_said_and_one_that_was_is_not() {
        use crate::app::open_link_with;
        use crate::state::Session;

        let address = "https://example.test/a?b=1";
        let mut session = Session::new("none");
        let mut asked = Vec::new();
        open_link_with(&mut session, address, false, |given| {
            asked.push(given.to_string());
            true
        });
        assert_eq!(asked, [address]);
        assert_eq!(
            session.transcript.len(),
            0,
            "a browser that opened was reported"
        );

        open_link_with(&mut session, address, false, |_| false);
        let said = session.transcript.last().expect("a note").text.clone();
        assert_eq!(said, bravebot_i18n::t!(session_link_not_opened));
        assert!(
            !said.contains("example.test"),
            "the address was repeated: {said}"
        );
        assert!(session.said_while_working().is_empty());

        open_link_with(&mut session, address, true, |_| false);
        assert_eq!(
            session.said_while_working().len(),
            1,
            "a note while working was not kept apart"
        );
    }

    fn row(text: &str) -> Buffer {
        Buffer::with_lines([text])
    }

    fn mark(buffer: &mut Buffer, row: u16, from: u16, to: u16) {
        for column in from..to {
            buffer[(column, row)].set_style(Style::default().add_modifier(ADDRESS_MARK));
        }
    }

    fn marks_left(buffer: &Buffer) -> bool {
        buffer.content.iter().any(|cell| kind_of(cell).is_some())
    }

    fn underlined(buffer: &Buffer, row: u16, column: u16) -> bool {
        buffer[(column, row)]
            .modifier
            .contains(Modifier::UNDERLINED)
    }

    /// The address a click opens is read from the cells, which is where the widget put it, and the
    /// cells are underlined with the mark taken off.
    #[test]
    fn the_cells_of_an_address_are_read_back_and_underlined() {
        let mut buffer = row("see https://example.test/a now");
        mark(&mut buffer, 0, 4, 26);
        let area = buffer.area;
        let found = scan(&mut buffer, area, &[]);

        assert_eq!(
            found,
            vec![Found {
                target: Target::Address("https://example.test/a".to_string()),
                drawn: "https://example.test/a".to_string(),
                cells: vec![(0, 4, 26)],
            }]
        );
        assert!(!marks_left(&buffer), "a mark was left for the terminal");
        assert!((4..26).all(|column| underlined(&buffer, 0, column)));
        assert!(!underlined(&buffer, 0, 3) && !underlined(&buffer, 0, 26));
        assert!(found[0].covers(0, 4) && found[0].covers(0, 25));
        assert!(!found[0].covers(0, 3) && !found[0].covers(0, 26) && !found[0].covers(1, 10));
    }

    /// An address the widget broke inside a word is one address over two rows, and one it broke at
    /// a blank is two.
    #[test]
    fn a_wrap_inside_the_address_joins_its_rows_and_a_wrap_at_a_blank_does_not() {
        let screen = || {
            let mut buffer = Buffer::with_lines(["go to https://exam", "ple.test/a and"]);
            mark(&mut buffer, 0, 6, 18);
            mark(&mut buffer, 1, 0, 10);
            buffer
        };

        let mut buffer = screen();
        let area = buffer.area;
        let found = scan(&mut buffer, area, &[(1, false)]);
        assert_eq!(
            found,
            vec![Found {
                target: Target::Address("https://example.test/a".to_string()),
                drawn: "https://example.test/a".to_string(),
                cells: vec![(0, 6, 18), (1, 0, 10)],
            }]
        );
        assert!(found[0].covers(1, 3) && found[0].covers(0, 17));

        let mut buffer = screen();
        let found = scan(&mut buffer, area, &[(1, true)]);
        assert!(
            found
                .iter()
                .all(|found| found.target != Target::Address("https://example.test/a".to_string())),
            "a blank was joined over: {found:?}"
        );
        assert!(!marks_left(&buffer));
    }

    /// Only part of an address is on the screen when the line it is on begins above it, or when it
    /// runs to the last column of the last row. What is drawn of one is not an address a click may
    /// open.
    #[test]
    fn an_address_only_partly_in_the_area_is_not_a_link() {
        let mut above = Buffer::with_lines(["ple.test/a and", "next line"]);
        mark(&mut above, 0, 0, 10);
        let area = above.area;
        assert_eq!(scan(&mut above, area, &[(0, false)]), vec![]);
        assert!(!marks_left(&above));

        let mut below = Buffer::with_lines(["a line", "go https://exam"]);
        mark(&mut below, 1, 3, 15);
        let area = below.area;
        assert_eq!(scan(&mut below, area, &[]), vec![]);
        assert!(!marks_left(&below));
        assert!(
            !(3..15).any(|column| underlined(&below, 1, column)),
            "a dropped address was underlined"
        );
    }

    /// A run that reads as an address but whose cells hold something else is not one: the cells
    /// are what a click opens, so whatever the matcher meant is not enough.
    #[test]
    fn what_the_cells_show_decides_what_is_a_link() {
        for text in [
            "https://exampl\u{e9}.test/",
            "https://user@example.test/",
            "file:///etc/passwd",
            "https://",
            "-a Calculator",
        ] {
            // Room after the text, so what is dropped is dropped for what the cells hold and not
            // for running to the last column.
            let mut buffer = row(&format!("{text}   "));
            let width = text.chars().count() as u16;
            mark(&mut buffer, 0, 0, width);
            let area = buffer.area;
            assert_eq!(scan(&mut buffer, area, &[]), vec![], "{text:?}");
            assert!(!marks_left(&buffer), "{text:?}");
        }
    }

    /// Cells outside the area are another widget's, so a mark there is not read.
    #[test]
    fn cells_outside_the_area_are_not_read() {
        let mut buffer = row("https://example.test/ https://other.test/");
        mark(&mut buffer, 0, 22, 41);
        let area = Rect::new(0, 0, 21, 1);
        assert_eq!(scan(&mut buffer, area, &[]), vec![]);
        assert!(marks_left(&buffer));
    }

    fn path(name: &str, line: Option<u32>, column: Option<u32>) -> Option<PathRef> {
        Some(PathRef {
            path: name.to_string(),
            line,
            column,
        })
    }

    /// What reads as a path: a name with an extension, with or without directories, and the place
    /// in it that follows. Each form a reply writes one in is one.
    #[test]
    fn what_reads_as_a_path_and_the_place_after_it() {
        for (text, expected) in [
            ("README.md", path("README.md", None, None)),
            ("src/main.rs", path("src/main.rs", None, None)),
            ("./src/main.rs", path("./src/main.rs", None, None)),
            ("/work/src/main.rs", path("/work/src/main.rs", None, None)),
            ("src/main.rs:12", path("src/main.rs", Some(12), None)),
            ("src/main.rs:12:5", path("src/main.rs", Some(12), Some(5))),
            ("src/main.rs:12-20", path("src/main.rs", Some(12), None)),
            (".gitignore", path(".gitignore", None, None)),
            ("a/.env.local", path("a/.env.local", None, None)),
            (
                "my-dir/some_file.test.ts",
                path("my-dir/some_file.test.ts", None, None),
            ),
            (
                "node_modules/@types/node/index.d.ts",
                path("node_modules/@types/node/index.d.ts", None, None),
            ),
            ("docs/caf\u{e9}.md", path("docs/caf\u{e9}.md", None, None)),
            (
                "\u{6587}\u{4ef6}/\u{540d}.txt",
                path("\u{6587}\u{4ef6}/\u{540d}.txt", None, None),
            ),
            ("a/b.c", path("a/b.c", None, None)),
            ("main.c", path("main.c", None, None)),
            ("lib.h:3", path("lib.h", Some(3), None)),
            ("a.tar.gz", path("a.tar.gz", None, None)),
            ("src/lib.rs:999999", path("src/lib.rs", Some(999_999), None)),
        ] {
            assert_eq!(path_of(text), expected, "{text:?}");
        }
    }

    /// What does not: prose that has a dot or a slash in it, a version, a name with no extension, a
    /// directory, an address for mail, something that climbs out, and a place that is not a place.
    #[test]
    fn what_does_not_read_as_a_path() {
        for text in [
            "",
            "e.g",
            "i.e",
            "a.m",
            "1.5.0",
            "3.14",
            "v1.2.3",
            "Makefile",
            "COPYING",
            "src/Makefile",
            "src/",
            "src",
            "and/or",
            "../secret.txt",
            "a/../b.txt",
            "a//b.txt",
            "Hmm...maybe",
            "wait...what",
            "a..b.rs",
            "-rf.txt",
            "name@example.com",
            "C:\\work\\a.txt",
            "a b.txt",
            "https://example.test/a.txt",
            "a.txt#L3",
            "a.txt:",
            "a.txt:0",
            "a.txt:-1",
            "a.txt:1234567",
            "a.txt:x",
            "a.b%20c",
            "a/b.\u{202e}txt",
            "a/b\u{200b}.txt",
            "a/b.t\u{e9}xt",
            "a.verylongextension",
            "~/a.txt",
            "a.txt:12:5:7",
        ] {
            assert_eq!(path_of(text), None, "{text:?}");
        }
        let long = format!("{}.rs", "a/".repeat(600));
        assert_eq!(
            path_of(&long),
            None,
            "a name longer than a file system takes"
        );
    }

    /// A path ends where the sentence resumes, and one inside brackets or code is the name inside.
    #[test]
    fn a_path_stops_where_the_sentence_resumes() {
        let found = |text: &str| {
            starts_a_path(text, 0)
                .then(|| path_end(text, 0).ok())
                .flatten()
                .map(|end| text[..end].to_string())
        };
        for (text, expected) in [
            ("src/main.rs", "src/main.rs"),
            ("src/main.rs.", "src/main.rs"),
            ("src/main.rs,", "src/main.rs"),
            ("src/main.rs:", "src/main.rs"),
            ("src/main.rs:12:", "src/main.rs:12"),
            ("src/main.rs:12.", "src/main.rs:12"),
            ("src/main.rs)", "src/main.rs"),
            ("src/main.rs**", "src/main.rs"),
            ("src/main.rs?", "src/main.rs"),
            ("src/main.rs and more", "src/main.rs"),
            ("_config.yml", "_config.yml"),
            ("__init__.py", "__init__.py"),
        ] {
            assert_eq!(found(text).as_deref(), Some(expected), "{text:?}");
        }
        for text in ["and/or", "Makefile", "-x.rs", "(src/main.rs)", "*a.rs"] {
            assert_eq!(found(text), None, "{text:?}");
        }
    }

    /// A path may start a line or follow a blank or an opening bracket or quotation mark, and is not
    /// found inside a word.
    #[test]
    fn a_path_starts_a_word() {
        assert!(starts_a_path("src/main.rs", 0));
        assert!(starts_a_path("see src/main.rs", 4));
        assert!(starts_a_path("(src/main.rs)", 1));
        assert!(starts_a_path("[src/main.rs]", 1));
        assert!(starts_a_path("\"src/main.rs\"", 1));
        assert!(starts_a_path("'src/main.rs'", 1));
        assert!(!starts_a_path("xsrc/main.rs", 1));
        assert!(!starts_a_path("a=src/main.rs", 2));
        assert!(!starts_a_path("see  ", 4));
        assert!(!starts_a_path("see -x", 4));
    }

    /// A path is at most 1024 bytes: one of exactly that is a path, and one byte more is not.
    #[test]
    fn a_path_of_1024_bytes_is_one_and_one_of_1025_is_not() {
        let named = |bytes: usize| format!("{}.rs", "a".repeat(bytes - 3));
        assert_eq!(named(1024).len(), 1024);
        assert!(path_of(&named(1024)).is_some());
        assert_eq!(path_of(&named(1025)), None);
        // Counted in bytes: 512 two-byte letters and ".rs" is 1027.
        assert_eq!(path_of(&format!("{}.rs", "\u{e9}".repeat(512))), None);
        assert!(path_of(&format!("{}.rs", "\u{e9}".repeat(510))).is_some());
    }

    /// A path may begin with a letter or a digit of another script, which is a name in that script,
    /// and not with a character that is neither, which is punctuation before a name.
    #[test]
    fn a_path_may_begin_with_a_letter_that_is_not_ascii_and_not_with_a_symbol() {
        assert!(starts_a_path(
            "\u{43d}\u{430}\u{447}\u{430}\u{43b}\u{43e}/a.rs",
            0
        ));
        assert!(starts_a_path("see \u{e9}t\u{e9}.txt", 4));
        assert!(starts_a_path("\u{6587}\u{4ef6}/a.txt", 0));
        for symbol in [
            "\u{ab}a.txt",
            "\u{2014}a.txt",
            "\u{2022}a.txt",
            "\u{1f600}a.txt",
        ] {
            assert!(!starts_a_path(symbol, 0), "{symbol:?}");
        }
    }

    /// A closing brace with no opening one before it belongs to the sentence, as a bracket does, and
    /// a pair that is part of the name stays.
    #[test]
    fn a_closing_brace_with_no_opener_is_not_part_of_the_path_or_the_address() {
        let braced = "{src/main.rs}";
        assert!(starts_a_path(braced, 1));
        assert_eq!(path_end(braced, 1), Ok(braced.len() - 1));
        assert_eq!(
            address_end("https://example.test/a}", 0),
            Ok("https://example.test/a".len())
        );
        let paired = "https://example.test/{a}";
        assert_eq!(address_end(paired, 0), Ok(paired.len()));
        let named = "src/{a}.rs";
        assert_eq!(
            path_end(named, 0),
            Err(named.len()),
            "a brace is no part of a name"
        );
    }

    /// The text being scanned arrived through a turn, so a long run of what could be a path is
    /// measured once.
    #[test]
    fn a_long_run_is_measured_in_one_pass() {
        let text = "(a/".repeat(100_000);
        assert!(path_end(&text, 1).is_err());
        let digits = format!("a.rs{}", ":1".repeat(100_000));
        assert_eq!(path_of(&digits), None);
    }

    /// A path is read back from the cells as an address is, and its cells are not underlined: whether
    /// it names a file is not known yet.
    #[test]
    fn the_cells_of_a_path_are_read_back_and_not_underlined() {
        let mut buffer = row("see src/main.rs:12:5 now");
        for column in 4..20 {
            buffer[(column, 0)].set_style(Style::default().add_modifier(PATH_MARK));
        }
        let area = buffer.area;
        let found = scan(&mut buffer, area, &[]);
        assert_eq!(
            found,
            vec![Found {
                target: Target::Path(PathRef {
                    path: "src/main.rs".to_string(),
                    line: Some(12),
                    column: Some(5),
                }),
                drawn: "src/main.rs:12:5".to_string(),
                cells: vec![(0, 4, 20)],
            }]
        );
        assert!(!marks_left(&buffer));
        assert!(
            (0..buffer.area.width).all(|column| !underlined(&buffer, 0, column)),
            "a path was underlined"
        );
    }

    /// A path the widget broke inside is one path, and one broken at a blank is not.
    #[test]
    fn a_wrap_inside_a_path_joins_its_rows() {
        let screen = || {
            let mut buffer = Buffer::with_lines(["open crates/tui/sr", "c/app.rs now"]);
            for column in 5..18 {
                buffer[(column, 0)].set_style(Style::default().add_modifier(PATH_MARK));
            }
            for column in 0..8 {
                buffer[(column, 1)].set_style(Style::default().add_modifier(PATH_MARK));
            }
            buffer
        };
        let mut buffer = screen();
        let area = buffer.area;
        let found = scan(&mut buffer, area, &[(1, false)]);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].target,
            Target::Path(PathRef {
                path: "crates/tui/src/app.rs".to_string(),
                line: None,
                column: None
            })
        );
        assert_eq!(found[0].cells, vec![(0, 5, 18), (1, 0, 8)]);

        let mut buffer = screen();
        let found = scan(&mut buffer, area, &[(1, true)]);
        assert!(
            found.iter().all(|found| found.target
                != Target::Path(PathRef {
                    path: "crates/tui/src/app.rs".to_string(),
                    line: None,
                    column: None
                })),
            "{found:?}"
        );
    }

    /// A wide character that did not fit in the last column goes to the next row and leaves the row
    /// one cell short, with the cell empty. The path goes on over the break: it is not the name that
    /// ends where the first row does, and the next row is not a name that begins above the screen.
    /// A row the wrap broke inside a word that ends one short with a narrow character after it
    /// stays two runs.
    #[test]
    fn a_wide_character_that_left_a_row_one_cell_short_does_not_end_the_path() {
        let screen = |first: &str, second: &str, marked: &[u16]| {
            let mut buffer = Buffer::with_lines([first, second]);
            for column in 0..9 {
                buffer[(column, 0)].set_style(Style::default().add_modifier(PATH_MARK));
            }
            for &column in marked {
                buffer[(column, 1)].set_style(Style::default().add_modifier(PATH_MARK));
            }
            buffer
        };
        let whole = |found: &[Found]| {
            found
                .iter()
                .map(|found| found.drawn.clone())
                .collect::<Vec<_>>()
        };

        let mut buffer = screen("aaaaaa.md ", "\u{6587}\u{6863}.txt  ", &[0, 2, 4, 5, 6, 7]);
        let area = buffer.area;
        let found = scan(&mut buffer, area, &[(1, false)]);
        assert_eq!(whole(&found), ["aaaaaa.md\u{6587}\u{6863}.txt"]);
        assert_eq!(found[0].cells, vec![(0, 0, 9), (1, 0, 8)]);

        let mut buffer = screen("aaaaaa.md ", "bbb.txt   ", &[0, 1, 2, 3, 4, 5, 6]);
        let found = scan(&mut buffer, area, &[(1, false)]);
        assert_eq!(whole(&found), ["aaaaaa.md"], "{found:?}");
        assert!(!marks_left(&buffer));
    }

    /// A path with a wide character in it has cells after that character that carry no mark of their
    /// own, and they are part of the run.
    #[test]
    fn a_path_with_a_wide_character_is_one_run() {
        let mut buffer = row("see \u{6587}\u{4ef6}/a.txt now");
        // Two wide characters, a slash and five more: ten cells, of which the second of each wide
        // character carries no mark.
        for column in [4, 6, 8, 9, 10, 11, 12, 13] {
            buffer[(column, 0)].set_style(Style::default().add_modifier(PATH_MARK));
        }
        let area = buffer.area;
        let found = scan(&mut buffer, area, &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            found[0].target,
            Target::Path(PathRef {
                path: "\u{6587}\u{4ef6}/a.txt".to_string(),
                line: None,
                column: None
            })
        );
        assert_eq!(found[0].cells, vec![(0, 4, 14)]);
        assert!(found[0].covers(0, 5) && found[0].covers(0, 7));
    }

    /// A mark that does not hold a path is taken off and finds nothing, whatever the matcher meant.
    #[test]
    fn what_the_cells_of_a_path_show_decides_what_is_a_link() {
        for text in ["../secret.txt", "Makefile", "a b.txt", "a.txt:0"] {
            let mut buffer = row(&format!("{text}   "));
            let width = text.chars().count() as u16;
            for column in 0..width {
                buffer[(column, 0)].set_style(Style::default().add_modifier(PATH_MARK));
            }
            let area = buffer.area;
            assert_eq!(scan(&mut buffer, area, &[]), vec![], "{text:?}");
            assert!(!marks_left(&buffer), "{text:?}");
        }
    }

    /// An address and a path are never one link, even where one ends at the last column and the next
    /// row, which the wrap made inside a word, begins with the other.
    #[test]
    fn a_run_of_one_kind_does_not_join_a_run_of_another() {
        let mut buffer = Buffer::with_lines(["go to https://exam", "ple.test/a.rs end"]);
        for column in 6..18 {
            buffer[(column, 0)].set_style(Style::default().add_modifier(ADDRESS_MARK));
        }
        for column in 0..13 {
            buffer[(column, 1)].set_style(Style::default().add_modifier(PATH_MARK));
        }
        let area = buffer.area;
        let found = scan(&mut buffer, area, &[(1, false)]);
        assert!(
            found.iter().all(|found| match &found.target {
                Target::Address(address) => address != "https://example.test/a.rs",
                Target::Path(path) => path.path != "https://example.test/a.rs",
            }),
            "{found:?}"
        );
        assert!(!marks_left(&buffer));
    }
}
