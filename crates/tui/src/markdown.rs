//! Inline markdown for the transcript.
//!
//! Models write markdown whether or not anything asks them to, so a transcript that shows the
//! source characters reads worse than one that shows what they meant. This turns the common
//! inline forms into styles and drops the markers.
//!
//! The matcher is hand written and single pass: it never backtracks, and an unclosed marker
//! stays on screen as the literal character it is. That matters more here than completeness,
//! since the text being scanned arrived through a turn and may be adversarial. Nothing here
//! decides anything: the input is already released for the screen, and the only output is
//! styling.
//!
//! An issue or pull request number, `#123`, is underlined where the session can link it
//! ([`crate::issue_link`]). The underline is what carries the match to the screen, where the
//! widget that wraps the line has put it, and it is still only styling. A web address is marked
//! the same way where the cells are scanned afterwards ([`crate::text_links`]).
//!
//! Block structure lives elsewhere. A table is a layout decision, so it is made in
//! [`crate::table`], which calls back here for the contents of each cell and never the other way
//! round. A fenced block is found here ([`fences`]) and drawn where the reply is laid out, since
//! what is inside one is not read as markdown at all. Still deliberately absent: lists and links,
//! each of which needs a layout decision nobody has made yet.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::issue_link;
use crate::text_links::{self, ADDRESS_MARK, Marks, PATH_MARK};
use crate::theme;

/// Inline code, coloured rather than emboldened so it reads as a quotation of the workspace.
pub(crate) fn code() -> Color {
    theme::brand_primary()
}

/// A fenced block in a reply: lines between one that opens it and one that closes it.
///
/// A fence is a line of three or more backticks or tildes, indented by any number of spaces, with
/// what the block is written in after it. It closes at the next line of the same character, at
/// least as long, with nothing after it, and at the end of the reply where no line does. Lines
/// inside one are what they were written as, whatever they look like: a fence of the other
/// character, a table row and a heading are text there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fence {
    /// The line of the reply that opens the block.
    pub open: usize,
    /// The line that closes it, which is none where the reply ended first.
    pub close: Option<usize>,
    /// What follows the fence on its line, with the blanks around it taken off.
    pub info: String,
    /// How many spaces the fence was indented by, which its lines are indented by as well.
    indent: usize,
}

impl Fence {
    /// The first line of the reply after the block, which is the line after the closing fence or
    /// the end of a reply that never closed it.
    pub fn end(&self, lines: usize) -> usize {
        self.close.map_or(lines, |close| close + 1)
    }

    /// Whether a line inside the block holds something other than blanks, which is what a block
    /// needs to be worth copying: a block of blank lines has nothing to put on the clipboard.
    pub fn holds_words(&self, source: &[&str]) -> bool {
        source
            .get(self.open + 1..self.close.unwrap_or(source.len()))
            .is_some_and(|lines| lines.iter().any(|line| !line.trim().is_empty()))
    }

    /// The lines inside the block, as the reply wrote them but without the indent of the fence.
    pub fn body<'a>(&self, source: &[&'a str]) -> Vec<&'a str> {
        let last = self.close.unwrap_or(source.len());
        source
            .get(self.open + 1..last)
            .unwrap_or_default()
            .iter()
            .map(|line| {
                let blanks = line.len() - line.trim_start_matches(' ').len();
                line.get(blanks.min(self.indent)..).unwrap_or_default()
            })
            .collect()
    }

    /// The words of the block, one line to a line and no fence, ready to be pasted.
    pub fn text(&self, source: &[&str]) -> String {
        self.body(source).join("\n")
    }
}

/// The fenced blocks of a reply, in the order they come.
///
/// Made in one pass over the lines. What the blocks are made of is never read past the one line
/// that could close them, so a reply written to look like many fences costs no more than its length.
pub fn fences(source: &[&str]) -> Vec<Fence> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < source.len() {
        let Some((indent, mark, length, info)) = opening(source[at]) else {
            at += 1;
            continue;
        };
        let close = (at + 1..source.len()).find(|&line| closes(source[line], mark, length));
        found.push(Fence {
            open: at,
            close,
            info: info.to_string(),
            indent,
        });
        at = close.map_or(source.len(), |close| close + 1);
    }
    found
}

/// The words of the `number`th fenced block of a reply, counting every block from none.
pub fn block_text(reply: &str, number: usize) -> Option<String> {
    let source: Vec<&str> = reply.lines().collect();
    fences(&source).get(number).map(|block| block.text(&source))
}

/// The words of each fenced block of a reply that holds a line, in the order they come.
///
/// These are the blocks that draw the word that copies them, which is what a block is counted by
/// where it is asked for by number and not clicked.
pub fn copyable_blocks(reply: &str) -> Vec<String> {
    let source: Vec<&str> = reply.lines().collect();
    fences(&source)
        .iter()
        .filter(|block| block.holds_words(&source))
        .map(|block| block.text(&source))
        .collect()
}

/// How a fence is indented, what it is made of and how many of them there are, and what follows
/// them, where the line opens a block.
///
/// A backtick fence cannot hold a backtick in what follows it, which is what tells it from a code
/// span that happens to open a line.
fn opening(line: &str) -> Option<(usize, char, usize, &str)> {
    let rest = line.trim_start_matches(' ');
    let mark = rest.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let length = rest.chars().take_while(|c| *c == mark).count();
    let info = rest.get(length..).unwrap_or_default().trim();
    (length >= 3 && !(mark == '`' && info.contains('`'))).then_some((
        line.len() - rest.len(),
        mark,
        length,
        info,
    ))
}

/// Whether the line closes a block opened by `length` of `mark`.
fn closes(line: &str, mark: char, length: usize) -> bool {
    let rest = line.trim_start_matches(' ');
    let run = rest.chars().take_while(|c| *c == mark).count();
    run >= length && rest.get(run..).unwrap_or_default().trim().is_empty()
}

/// How deep styling may nest before markers are shown as themselves.
///
/// Nesting recurses, and the text being styled arrives through a turn, so the depth cannot be
/// left to depend on an argument about how the matcher happens to pair markers up. Closing at
/// the first candidate keeps real replies within a level or two of this anyway: the cap exists
/// so that a line built to nest cannot recurse the renderer off its stack.
const MAX_DEPTH: usize = 8;

/// Style one line of markdown into spans, all inheriting `base`.
///
/// `references` underlines each `#<number>` ([`reference`]), and is true only where the session
/// goes on to link what it underlined. An underline nothing opens would read as a link that is
/// broken.
pub fn spans(text: &str, base: Style, references: bool) -> Vec<Span<'static>> {
    spans_with(text, base, Marks::references(references))
}

/// [`spans`], marking everything `marks` asks for.
///
/// A mark is for a caller that scans the cells it ends up in. One that does not asks for no more
/// than it takes off.
pub fn spans_with(text: &str, base: Style, marks: Marks) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    match heading(text) {
        Some(rest) => inline(rest, base.add_modifier(Modifier::BOLD), 0, marks, &mut out),
        None => inline(text, base, 0, marks, &mut out),
    }
    out
}

/// The text of an ATX heading, if the line is one.
///
/// Up to six hashes and then a space, matching the usual rule: `#foo` is a word beginning with a
/// hash, not a heading.
fn heading(text: &str) -> Option<&str> {
    let hashes = text.len() - text.trim_start_matches('#').len();
    if (1..=6).contains(&hashes) {
        text[hashes..].strip_prefix(' ')
    } else {
        None
    }
}

/// The text between two offsets that fall at markers, which are ASCII, or none where they do not.
fn between(text: &str, from: usize, to: usize) -> &str {
    text.get(from..to).unwrap_or_default()
}

fn inline(text: &str, base: Style, depth: usize, marks: Marks, out: &mut Vec<Span<'static>>) {
    if depth == MAX_DEPTH {
        out.push(Span::styled(text.to_string(), base));
        return;
    }

    let bytes = text.as_bytes();
    let mut plain = String::new();
    let mut at = 0;
    // Where the run an address was not found in ends, so each scheme inside one is not measured
    // again: the run is measured once, however many schemes it holds.
    let mut no_address_before = 0;
    // The same for the run a path was not found in.
    let mut no_path_before = 0;

    while at < bytes.len() {
        if marks.addresses
            && at >= no_address_before
            && matches!(bytes[at], b'h' | b'H')
            && text_links::starts_an_address(text, at)
        {
            match text_links::address_end(text, at) {
                Ok(end) => {
                    flush(&mut plain, base, out);
                    // Verbatim, like a code span: what is in an address is not markdown.
                    let marked = base.add_modifier(ADDRESS_MARK);
                    out.push(Span::styled(
                        text_links::between(text, at, end).to_string(),
                        marked,
                    ));
                    at = end;
                    continue;
                }
                Err(run_end) => no_address_before = run_end,
            }
        }
        match bytes[at] {
            b'`' => {
                if let Some(end) = closing(text, at + 1, "`") {
                    flush(&mut plain, base, out);
                    // Code is verbatim: markers inside it are the text, not formatting. A span
                    // that is nothing but an address is one a click opens.
                    let quoted = between(text, at + 1, end);
                    let mut style = base.fg(code());
                    if marks.addresses && is_one_address(quoted) {
                        style = style.add_modifier(ADDRESS_MARK);
                    } else if marks.paths && is_one_path(quoted) {
                        style = style.add_modifier(PATH_MARK);
                    }
                    out.push(Span::styled(quoted.to_string(), style));
                    at = end + 1;
                    continue;
                }
            }
            b'#' if marks.references => {
                if let Some(end) = reference(text, at) {
                    flush(&mut plain, base, out);
                    let marked = base.add_modifier(issue_link::MARK);
                    out.push(Span::styled(text[at..end].to_string(), marked));
                    at = end;
                    continue;
                }
            }
            b'*' if text[at..].starts_with("**") => {
                if let Some(end) = closing(text, at + 2, "**") {
                    flush(&mut plain, base, out);
                    let bold = base.add_modifier(Modifier::BOLD);
                    inline(between(text, at + 2, end), bold, depth + 1, marks, out);
                    at = end + 2;
                    continue;
                }
            }
            b'*' | b'_' => {
                let marker = &text[at..at + 1];
                if opens_word(text, at)
                    && let Some(end) = emphasis_close(text, at + 1, marker)
                {
                    flush(&mut plain, base, out);
                    let italic = base.add_modifier(Modifier::ITALIC);
                    inline(between(text, at + 1, end), italic, depth + 1, marks, out);
                    at = end + 1;
                    continue;
                }
            }
            _ => {}
        }

        // After the markers, so `_name.rs_` is still emphasis, and a name that begins with an
        // underscore and has no closing one is a name.
        if marks.paths && at >= no_path_before && text_links::starts_a_path(text, at) {
            match text_links::path_end(text, at) {
                Ok(end) => {
                    flush(&mut plain, base, out);
                    // Verbatim, as an address is: what is inside a name is not markdown.
                    let marked = base.add_modifier(PATH_MARK);
                    out.push(Span::styled(
                        text_links::between(text, at, end).to_string(),
                        marked,
                    ));
                    at = end;
                    continue;
                }
                Err(run_end) => no_path_before = run_end,
            }
        }

        let next = text[at..].chars().next().expect("at sits on a boundary");
        plain.push(next);
        at += next.len_utf8();
    }

    flush(&mut plain, base, out);
}

/// Whether `code` is a file path and nothing else.
fn is_one_path(code: &str) -> bool {
    text_links::starts_a_path(code, 0) && text_links::path_end(code, 0) == Ok(code.len())
}

/// Whether `code` is a web address and nothing else.
fn is_one_address(code: &str) -> bool {
    text_links::starts_an_address(code, 0) && text_links::address_end(code, 0) == Ok(code.len())
}

/// Where the issue or pull request number the `#` at `at` opens ends, if it opens one.
///
/// A `#` that follows a word character, an `&` or a `/` is part of something else (`C#1`,
/// `page#12`, the entity `&#39;`, the fragment of `https://example.com/docs/#12`), and so is one
/// whose digits are followed by a word character (`#12abc`). The
/// digits have to be a number ([`issue_link::is_number`]). A code span is taken whole before this
/// is reached, so a number inside backticks stays as it was written.
fn reference(text: &str, at: usize) -> Option<usize> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    if text[..at]
        .chars()
        .next_back()
        .is_some_and(|c| word(c) || matches!(c, '&' | '/'))
    {
        return None;
    }
    let rest = &text[at + 1..];
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    if !issue_link::is_number(&rest[..digits]) || rest[digits..].chars().next().is_some_and(word) {
        return None;
    }
    Some(at + 1 + digits)
}

fn flush(plain: &mut String, style: Style, out: &mut Vec<Span<'static>>) {
    if !plain.is_empty() {
        out.push(Span::styled(std::mem::take(plain), style));
    }
}

/// Where the run opened at `from` closes, if it does.
///
/// The span must be non-empty and must not begin or end with a space, so a lone marker in prose
/// stays a lone marker: `2 * 3 * 4` is arithmetic.
fn closing(text: &str, from: usize, marker: &str) -> Option<usize> {
    if text[from..].starts_with(char::is_whitespace) {
        return None;
    }

    let mut at = from;
    while let Some(offset) = text[at..].find(marker) {
        let end = at + offset;
        if end > from && !text[from..end].ends_with(char::is_whitespace) {
            return Some(end);
        }
        at = end + marker.len();
    }
    None
}

/// A single marker only emphasises at a word boundary, so `snake_case_names` survive intact.
fn opens_word(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric())
}

fn emphasis_close(text: &str, from: usize, marker: &str) -> Option<usize> {
    let mut at = from;
    while let Some(end) = closing(text, at, marker) {
        let after = text[end + marker.len()..].chars().next();
        if after.is_none_or(|c| !c.is_alphanumeric()) {
            return Some(end);
        }
        at = end + marker.len();
        if at >= text.len() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The visible text and its style, which is what the screen actually shows.
    fn styled(text: &str) -> Vec<(String, Style)> {
        spans(text, Style::default(), false)
            .into_iter()
            .map(|span| (span.content.into_owned(), span.style))
            .collect()
    }

    fn shown(text: &str) -> String {
        styled(text).into_iter().map(|(text, _)| text).collect()
    }

    fn styles_of(text: &str, wanted: &str) -> Style {
        styled(text)
            .into_iter()
            .find(|(content, _)| content == wanted)
            .unwrap_or_else(|| panic!("no span reading {wanted:?} in {text:?}"))
            .1
    }

    fn is_bold(style: Style) -> bool {
        style.add_modifier.contains(Modifier::BOLD)
    }

    fn is_italic(style: Style) -> bool {
        style.add_modifier.contains(Modifier::ITALIC)
    }

    /// The text of each span that carries the mark a number is given, with references on.
    fn numbers(text: &str) -> Vec<String> {
        spans(text, Style::default(), true)
            .into_iter()
            .filter(|span| span.style.add_modifier.contains(issue_link::MARK))
            .map(|span| span.content.into_owned())
            .collect()
    }

    fn found(text: &str) -> Vec<(usize, Option<usize>, String)> {
        let source: Vec<&str> = text.lines().collect();
        fences(&source)
            .into_iter()
            .map(|fence| (fence.open, fence.close, fence.info))
            .collect()
    }

    /// A block is the lines between a fence and the next one of the same character at least as long,
    /// so a longer fence holds a shorter one and a tilde fence holds backticks.
    #[test]
    fn a_fence_closes_on_the_same_character_at_least_as_long() {
        assert_eq!(
            found("a\n```rust\nx\n```\nb"),
            [(1, Some(3), "rust".to_string())]
        );
        assert_eq!(
            found("````\n```\ninner\n```\n````"),
            [(0, Some(4), String::new())]
        );
        assert_eq!(found("~~~\n```\nx\n~~~"), [(0, Some(3), String::new())]);
        assert_eq!(found("```\nx\n~~~\ny"), [(0, None, String::new())]);
        assert_eq!(found("``\nx\n``"), []);
    }

    /// A fence nobody closed holds the rest of the reply, which is what a reply that is still being
    /// written looks like, and a closing fence cannot carry words after it.
    #[test]
    fn a_block_that_is_never_closed_runs_to_the_end_and_a_close_has_nothing_after_it() {
        let source: Vec<&str> = "```\none\n``` not a close\ntwo".lines().collect();
        let blocks = fences(&source);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].close, None);
        assert_eq!(blocks[0].end(source.len()), 4);
        assert_eq!(blocks[0].text(&source), "one\n``` not a close\ntwo");
    }

    /// A backtick fence cannot hold a backtick in its info string, so a line that opens with a code
    /// span is a line of prose, and a tilde fence can.
    #[test]
    fn a_line_that_opens_with_a_code_span_is_not_a_fence() {
        assert_eq!(found("```inline``` and more\nnext"), []);
        assert_eq!(found("~~~ a`b\nx\n~~~"), [(0, Some(2), "a`b".to_string())]);
    }

    /// The words of a block are what it holds and nothing of the fence: the indent a fence was
    /// written at is taken off each line, no more than that is, and a blank line stays one.
    #[test]
    fn the_text_of_a_block_is_its_lines_without_the_fence_or_its_indent() {
        let source: Vec<&str> = "  ```sh\n  a\n    b\n\nc\n  ```".lines().collect();
        let blocks = fences(&source);
        assert_eq!(blocks[0].text(&source), "a\n  b\n\nc");
        let source: Vec<&str> = "```\n\tindented\n```".lines().collect();
        assert_eq!(fences(&source)[0].text(&source), "\tindented");
        let source: Vec<&str> = "```\n```".lines().collect();
        assert_eq!(fences(&source)[0].text(&source), "");
    }

    /// A block is worth copying when a line between its fences holds something other than blanks,
    /// closed or not, and the words a block is copied as are counted among the blocks that are, in the
    /// order they are written. A block of blank lines has nothing to put on the clipboard, so it is
    /// not one, and a block with a line of words after a blank one is.
    #[test]
    fn a_block_with_nothing_between_its_fences_is_not_one_to_copy() {
        let reply = "```\n```\n```a\nx\n```\n```\n\n```\n```\n  \n\t\n```\n~~~\nunclosed";
        assert_eq!(copyable_blocks(reply), ["x", "unclosed"]);
        assert_eq!(block_text(reply, 0).as_deref(), Some(""));
        assert_eq!(block_text(reply, 1).as_deref(), Some("x"));
        assert_eq!(block_text(reply, 2).as_deref(), Some(""));
        assert_eq!(block_text(reply, 3).as_deref(), Some("  \n\t"));
        assert_eq!(block_text(reply, 5), None);
        assert!(copyable_blocks("no fence here").is_empty());
        assert_eq!(
            copyable_blocks("```\n\n\nfirst\n\n```\n```\n   \nlast"),
            ["\n\nfirst\n", "   \nlast"]
        );
    }

    /// Many fences are many blocks, in order, and a block's lines are not searched for more.
    #[test]
    fn blocks_come_in_the_order_they_are_written() {
        assert_eq!(
            found("```a\n```\ntext\n~~~b\nx\n~~~"),
            [(0, Some(1), "a".to_string()), (3, Some(5), "b".to_string())]
        );
    }

    /// The text of each span that carries the mark an address is given, with addresses on.
    fn addresses(text: &str) -> Vec<String> {
        let marks = Marks {
            addresses: true,
            ..Marks::default()
        };
        spans_with(text, Style::default(), marks)
            .into_iter()
            .filter(|span| span.style.add_modifier.contains(ADDRESS_MARK))
            .map(|span| span.content.into_owned())
            .collect()
    }

    #[test]
    fn a_double_star_run_is_bold_and_its_markers_are_gone() {
        assert_eq!(shown("say **this** loudly"), "say this loudly");
        assert!(is_bold(styles_of("say **this** loudly", "this")));
        assert!(!is_bold(styles_of("say **this** loudly", "say ")));
    }

    #[test]
    fn a_single_marker_is_italic() {
        assert_eq!(shown("say *this* and _that_"), "say this and that");
        assert!(is_italic(styles_of("say *this* and _that_", "this")));
        assert!(is_italic(styles_of("say *this* and _that_", "that")));
    }

    #[test]
    fn inline_code_is_coloured() {
        let _held = theme::exclusive();
        assert_eq!(shown("run `make check` first"), "run make check first");
        assert_eq!(
            styles_of("run `make check` first", "make check").fg,
            Some(code())
        );
    }

    /// Nesting is the common case in a reply: a bold clause containing a path.
    #[test]
    fn code_inside_bold_keeps_both() {
        let _held = theme::exclusive();
        let style = styles_of("**edit `main.rs` now**", "main.rs");
        assert!(is_bold(style));
        assert_eq!(style.fg, Some(code()));
    }

    /// Markers inside code are text, since that is usually the point of quoting them.
    #[test]
    fn code_contents_are_verbatim() {
        assert_eq!(shown("`**not bold**`"), "**not bold**");
    }

    /// The regression this guards: an identifier is not an emphasis run.
    #[test]
    fn an_underscored_identifier_is_left_alone() {
        let text = "call read_trusted_content now";
        assert_eq!(shown(text), text);
        assert!(!is_italic(styles_of(text, text)));
    }

    /// Nor is a lone marker in prose or arithmetic.
    #[test]
    fn a_lone_marker_stays_literal() {
        assert_eq!(shown("2 * 3 = 6"), "2 * 3 = 6");
        assert_eq!(shown("an *unclosed run"), "an *unclosed run");
        assert_eq!(shown("**"), "**");
        assert_eq!(shown("a ** b"), "a ** b");
    }

    #[test]
    fn a_heading_is_bold_without_its_hashes() {
        assert_eq!(shown("## What changed"), "What changed");
        assert!(is_bold(styles_of("## What changed", "What changed")));
        // A hash without a space is part of a word, and seven is not a heading.
        assert_eq!(shown("#42 filed"), "#42 filed");
        assert_eq!(shown("####### deep"), "####### deep");
    }

    #[test]
    fn the_base_style_is_inherited() {
        let base = Style::default().fg(Color::Green);
        let spans = spans("plain **bold**", base, false);
        assert!(spans.iter().all(|span| span.style.fg == Some(Color::Green)));
    }

    /// A line of nothing but markers is the shape that would recurse furthest, so it is the one
    /// worth checking terminates and shows its markers rather than inventing text.
    #[test]
    fn a_line_of_markers_terminates() {
        let stars = shown(&"*".repeat(100_000));
        assert!(
            stars.chars().all(|c| c == '*'),
            "text appeared from nowhere"
        );
        assert!(!stars.is_empty());
    }

    /// Multibyte text must not be sliced through a character, and an empty line is a line.
    #[test]
    fn odd_input_does_not_panic() {
        assert_eq!(shown("héllo **wörld** ☃"), "héllo wörld ☃");
        assert_eq!(shown("日本 *語* です"), "日本 語 です");
        assert!(styled("").is_empty());
        assert_eq!(shown(&"**a**".repeat(200)), "a".repeat(200));
    }

    /// The mark is the whole of what the draw pass has to go on, so each place a number may stand
    /// in a reply carries it, and the text it marks is the number and nothing around it.
    #[test]
    fn a_number_is_marked_wherever_a_reply_may_put_one() {
        for (text, wanted) in [
            ("#2879", vec!["#2879"]),
            ("fixed in #2879", vec!["#2879"]),
            ("#2879 is fixed", vec!["#2879"]),
            ("(#2879)", vec!["#2879"]),
            ("see #12, #345 and #6.", vec!["#12", "#345", "#6"]),
            ("**see #12**", vec!["#12"]),
            ("_#12_", vec!["#12"]),
            ("## Fix #12", vec!["#12"]),
            ("日本 #12 です", vec!["#12"]),
            ("#123456789", vec!["#123456789"]),
        ] {
            assert_eq!(numbers(text), wanted, "{text:?}");
            assert_eq!(
                spans(text, Style::default(), true)
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>(),
                shown(text),
                "marking a number changed the text of {text:?}"
            );
        }
    }

    /// A hash that belongs to something else is not a reference, and a link to the wrong issue is
    /// worse than none.
    #[test]
    fn a_hash_that_is_part_of_something_else_is_not_marked() {
        for text in [
            "C#1",
            "page#12",
            "owner/repo#12",
            "snake_#1",
            "é#12",
            "&#39;",
            "https://example.com/docs/#123",
            "#12abc",
            "#12_",
            "#1\u{e9}",
            "#0",
            "#007",
            "#1234567890",
            "# 12",
            "#",
            "#a12",
            "a #",
        ] {
            assert_eq!(numbers(text), Vec::<String>::new(), "{text:?}");
        }
    }

    /// Code is verbatim, which is why a number someone quoted in backticks is the text they wrote
    /// and not a thing that opens a page.
    #[test]
    fn a_number_in_a_code_span_is_not_marked() {
        assert_eq!(numbers("`#12` and #34"), vec!["#34"]);
        assert_eq!(numbers("`see #12`"), Vec::<String>::new());
        assert_eq!(numbers("**`#12`**"), Vec::<String>::new());
    }

    /// A heading is a hash and a space, and a hash and digits is a number, so the two never
    /// compete for the same line.
    #[test]
    fn a_heading_is_not_a_number_and_a_number_at_the_head_of_a_line_is_not_a_heading() {
        assert_eq!(numbers("# 12"), Vec::<String>::new());
        assert_eq!(numbers("## What changed"), Vec::<String>::new());
        let style_of = |text: &str, wanted: &str| {
            spans(text, Style::default(), true)
                .into_iter()
                .find(|span| span.content == wanted)
                .unwrap_or_else(|| panic!("no span reading {wanted:?} in {text:?}"))
                .style
        };
        assert!(is_bold(style_of("## Fix #12", "Fix ")));
        let heading = style_of("## Fix #12", "#12");
        assert!(is_bold(heading) && heading.add_modifier.contains(issue_link::MARK));
        let line = style_of("#42 filed", "#42");
        assert!(!is_bold(line) && line.add_modifier.contains(issue_link::MARK));
    }

    /// Where nothing will link a number, underlining it would draw a link that does nothing.
    #[test]
    fn with_references_off_a_number_is_plain_text() {
        let drawn = spans("fixed in #2879, see **#12**", Style::default(), false);
        assert!(
            drawn
                .iter()
                .all(|span| !span.style.add_modifier.contains(issue_link::MARK))
        );
        assert_eq!(
            drawn
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            "fixed in #2879, see #12"
        );
    }

    /// The text being scanned arrived through a turn, so the longest runs of what the matcher
    /// looks at have to end.
    #[test]
    fn long_runs_of_hashes_and_digits_terminate() {
        for text in [
            "#".repeat(100_000),
            format!("#{}", "1".repeat(100_000)),
            "#1".repeat(50_000),
            " #1".repeat(50_000),
        ] {
            let drawn: String = spans(&text, Style::default(), true)
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            assert_eq!(drawn, text);
        }
    }

    /// The mark is the whole of what the scan of the cells has to go on, so each place an address
    /// may stand in a reply carries it, and the text it marks is the address and nothing around it.
    #[test]
    fn an_address_is_marked_wherever_a_reply_may_put_one() {
        for (text, wanted) in [
            ("https://example.test/a", vec!["https://example.test/a"]),
            (
                "see https://example.test/a now",
                vec!["https://example.test/a"],
            ),
            ("(https://example.test/a)", vec!["https://example.test/a"]),
            ("at https://example.test/a.", vec!["https://example.test/a"]),
            ("<https://example.test/a>", vec!["https://example.test/a"]),
            ("**https://example.test/a**", vec!["https://example.test/a"]),
            (
                "**see https://example.test/a**",
                vec!["https://example.test/a"],
            ),
            ("_https://example.test/a_", vec!["https://example.test/a"]),
            (
                "## At https://example.test/a",
                vec!["https://example.test/a"],
            ),
            ("`https://example.test/a`", vec!["https://example.test/a"]),
            (
                "https://a.test/x and http://b.test/y_z",
                vec!["https://a.test/x", "http://b.test/y_z"],
            ),
            (
                "https://example.test/a_b_c",
                vec!["https://example.test/a_b_c"],
            ),
            // The text of a markdown link ends where its target begins.
            (
                "[https://example.test/a](https://example.test/a)",
                vec!["https://example.test/a", "https://example.test/a"],
            ),
            (
                "[docs](https://example.test/docs)",
                vec!["https://example.test/docs"],
            ),
        ] {
            assert_eq!(addresses(text), wanted, "{text:?}");
            assert_eq!(
                spans_with(
                    text,
                    Style::default(),
                    Marks {
                        addresses: true,
                        ..Marks::default()
                    }
                )
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
                shown(text),
                "marking an address changed the text of {text:?}"
            );
        }
    }

    /// What is not an address is drawn as the text it is: a code span holding more than the address
    /// is a quotation of a command, and a scheme inside a word, a run with anything but printable
    /// ASCII in it and an address naming a user are not addresses.
    #[test]
    fn a_run_that_is_not_an_address_is_not_marked() {
        for text in [
            "`get https://example.test/a`",
            "`https://example.test/a.`",
            "xhttps://example.test/a",
            "a-https://example.test/a",
            "ftp://example.test/a",
            "https://",
            "https://exampl\u{e9}.test/a",
            "https://example.test/a\u{200b}",
            "https://github.com@example.test/",
            "file:///etc/passwd",
            "example.test/a",
        ] {
            assert_eq!(addresses(text), Vec::<String>::new(), "{text:?}");
        }
    }

    /// An address is text, not markdown: the markers inside one are in it, and the matcher that
    /// found it does not go on to style them.
    #[test]
    fn what_is_inside_an_address_is_not_styled() {
        let drawn = spans_with(
            "https://example.test/*a*/_b_c end",
            Style::default(),
            Marks {
                references: true,
                addresses: true,
                paths: false,
            },
        );
        assert_eq!(
            drawn
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            "https://example.test/*a*/_b_c end"
        );
        let address = &drawn[0];
        assert_eq!(address.content, "https://example.test/*a*/_b_c");
        assert!(!address.style.add_modifier.contains(Modifier::ITALIC));
        assert!(!address.style.add_modifier.contains(issue_link::MARK));
    }

    /// An address is marked only where the caller scans the cells afterwards. A mark nobody takes
    /// off would be sent to the terminal.
    #[test]
    fn with_addresses_off_an_address_is_plain_text() {
        for marks in [Marks::default(), Marks::references(true)] {
            let drawn = spans_with("see https://example.test/a now", Style::default(), marks);
            assert!(
                drawn
                    .iter()
                    .all(|span| !span.style.add_modifier.contains(ADDRESS_MARK)),
                "{marks:?}"
            );
        }
    }

    /// The text being scanned arrived through a turn, so a run of schemes with something after it
    /// that makes none of them an address is measured once and not at every scheme inside it.
    #[test]
    fn a_run_of_schemes_that_is_not_an_address_is_measured_once() {
        // Each scheme follows a bracket, which is where one may start, so each would be measured
        // again if the run it is part of were not remembered.
        let text = format!("{}\u{e9} https://example.test/a", "(http://".repeat(20_000));
        let started = std::time::Instant::now();
        let found = addresses(&text);
        assert_eq!(found, vec!["https://example.test/a"]);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "took {:?}",
            started.elapsed()
        );
    }

    /// The text of each span that carries the mark a path is given, with paths on.
    fn paths(text: &str) -> Vec<String> {
        let marks = Marks {
            paths: true,
            ..Marks::default()
        };
        spans_with(text, Style::default(), marks)
            .into_iter()
            .filter(|span| span.style.add_modifier.contains(PATH_MARK))
            .map(|span| span.content.into_owned())
            .collect()
    }

    /// Each place a reply may put a path carries the mark, with the text it marks the path and the
    /// place after it and nothing around them, and the text of the line is unchanged.
    #[test]
    fn a_path_is_marked_wherever_a_reply_may_put_one() {
        for (text, wanted) in [
            ("src/main.rs", vec!["src/main.rs"]),
            ("edit src/main.rs now", vec!["src/main.rs"]),
            ("edit src/main.rs.", vec!["src/main.rs"]),
            ("in src/main.rs:12:5, then", vec!["src/main.rs:12:5"]),
            ("(src/main.rs)", vec!["src/main.rs"]),
            ("**src/main.rs**", vec!["src/main.rs"]),
            ("**edit src/main.rs**", vec!["src/main.rs"]),
            ("*src/main.rs*", vec!["src/main.rs"]),
            ("## In src/main.rs", vec!["src/main.rs"]),
            ("`src/main.rs`", vec!["src/main.rs"]),
            ("`src/main.rs:3`", vec!["src/main.rs:3"]),
            ("a/b.rs and c/d.rs", vec!["a/b.rs", "c/d.rs"]),
            ("snake_case/foo_bar.rs", vec!["snake_case/foo_bar.rs"]),
            ("_config.yml", vec!["_config.yml"]),
            ("README.md", vec!["README.md"]),
            // The text of a markdown link ends where its target begins.
            (
                "[src/main.rs](src/main.rs)",
                vec!["src/main.rs", "src/main.rs"],
            ),
            (
                "[main.rs](src/main.rs:12)",
                vec!["main.rs", "src/main.rs:12"],
            ),
            (
                "[`src/main.rs`](src/main.rs)",
                vec!["src/main.rs", "src/main.rs"],
            ),
        ] {
            assert_eq!(paths(text), wanted, "{text:?}");
            assert_eq!(
                spans_with(
                    text,
                    Style::default(),
                    Marks {
                        paths: true,
                        ..Marks::default()
                    }
                )
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
                shown(text),
                "marking a path changed the text of {text:?}"
            );
        }
    }

    /// What is not a path is drawn as text: prose with a dot or a slash, a code span holding more
    /// than the name, an address, and a name whose emphasis markers are its own.
    #[test]
    fn a_run_that_is_not_a_path_is_not_marked() {
        for text in [
            "and/or",
            "e.g. this",
            "version 1.5.0",
            "`cat src/main.rs`",
            "`src/main.rs.`",
            "https://example.test/a/b.rs",
            "a=src/main.rs",
            "../escape.rs",
            "Makefile",
            "mail me@example.com",
            // A run of dots is not a name, however far in it sits.
            "Hmm...maybe",
            "wait...what",
            "a..b.rs",
        ] {
            assert_eq!(paths(text), Vec::<String>::new(), "{text:?}");
        }
    }

    /// A name written between underscores for emphasis is emphasis, and one that begins with an
    /// underscore and has no closing one is a name.
    #[test]
    fn emphasis_around_a_name_is_still_emphasis() {
        assert_eq!(shown("_src/main.rs_"), "src/main.rs");
        assert_eq!(paths("_src/main.rs_"), vec!["src/main.rs"]);
        assert!(is_italic(styles_of("_src/main.rs_", "src/main.rs")));
        assert_eq!(shown("_config.yml"), "_config.yml");
    }

    /// A path is marked only where the caller scans the cells afterwards.
    #[test]
    fn with_paths_off_a_path_is_plain_text() {
        for marks in [Marks::default(), Marks::references(true)] {
            let drawn = spans_with("edit src/main.rs now", Style::default(), marks);
            assert!(
                drawn
                    .iter()
                    .all(|span| !span.style.add_modifier.contains(PATH_MARK)),
                "{marks:?}"
            );
        }
    }

    /// A run of what could begin a path, each after a bracket, is measured once, however many
    /// places in it one could begin.
    #[test]
    fn a_run_of_possible_paths_that_is_not_one_is_measured_once() {
        let text = format!("{}\u{e9} src/main.rs", "(a/".repeat(20_000));
        let started = std::time::Instant::now();
        let found = paths(&text);
        assert_eq!(found, vec!["src/main.rs"]);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "took {:?}",
            started.elapsed()
        );
    }
}
