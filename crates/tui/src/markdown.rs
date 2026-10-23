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
//! widget that wraps the line has put it, and it is still only styling.
//!
//! Block structure lives elsewhere. A table is a layout decision, so it is made in
//! [`crate::table`], which calls back here for the contents of each cell and never the other way
//! round. Still deliberately absent: fenced blocks, lists, and links, each of which needs a
//! layout decision nobody has made yet.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::issue_link;
use crate::theme;

/// Inline code, coloured rather than emboldened so it reads as a quotation of the workspace.
fn code() -> Color {
    theme::brand_primary()
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
    let mut out = Vec::new();
    match heading(text) {
        Some(rest) => inline(
            rest,
            base.add_modifier(Modifier::BOLD),
            0,
            references,
            &mut out,
        ),
        None => inline(text, base, 0, references, &mut out),
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

fn inline(text: &str, base: Style, depth: usize, references: bool, out: &mut Vec<Span<'static>>) {
    if depth == MAX_DEPTH {
        out.push(Span::styled(text.to_string(), base));
        return;
    }

    let bytes = text.as_bytes();
    let mut plain = String::new();
    let mut at = 0;

    while at < bytes.len() {
        match bytes[at] {
            b'`' => {
                if let Some(end) = closing(text, at + 1, "`") {
                    flush(&mut plain, base, out);
                    // Code is verbatim: markers inside it are the text, not formatting.
                    out.push(Span::styled(text[at + 1..end].to_string(), base.fg(code())));
                    at = end + 1;
                    continue;
                }
            }
            b'#' if references => {
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
                    inline(&text[at + 2..end], bold, depth + 1, references, out);
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
                    inline(&text[at + 1..end], italic, depth + 1, references, out);
                    at = end + 1;
                    continue;
                }
            }
            _ => {}
        }

        let next = text[at..].chars().next().expect("at sits on a boundary");
        plain.push(next);
        at += next.len_utf8();
    }

    flush(&mut plain, base, out);
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
}
