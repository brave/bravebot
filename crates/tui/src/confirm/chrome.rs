//! The frame every prompt here is drawn in: its panel, its key styles and the shared rows.

use bravebot_core::vetting::Verdict;
use bravebot_i18n::t;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear};

use crate::render::marked_rows;
use crate::theme;

/// Prose the panels indent by two columns, wrapped so every row keeps the indent.
///
/// The paragraph would wrap it too, but back to column zero, and these sentences sit beside
/// lines that are indented on purpose. Wrapped here rather than broken in the catalog, since a
/// translation does not break where the English did.
pub(super) fn indented(text: impl Into<String>, style: Style, width: usize) -> Vec<Line<'static>> {
    marked_rows(&Span::raw("  "), &[Span::styled(text.into(), style)], width)
}

/// What a check said, for the head of a prompt whose answer would promote content.
///
/// Shared by all three of them, so the word reads the same wherever it is drawn and a prompt cannot
/// be given one without the other. Which of the three banners is drawn is the one thing decided from
/// what the check said, and it decides nothing further: the bytes are below it either way, and the
/// keys are the same three.
///
/// The banner is the driver's own words and is drawn outside the margin. The sentence under it came
/// out of content nobody vouched for and is drawn inside it, on every row it reaches, which is the
/// distinction the bar exists to make: a reader can tell which line the program wrote and which line
/// came out of the page. It is free text about content an attacker may own and it can lie; what
/// stops that mattering is that the bytes it describes are on the same screen.
///
/// The two failures are told apart rather than collapsed. "This looks like an attempt to give
/// instructions" and "nothing looked at this" are different facts about different risks, and one
/// sentence covering both would be wrong about one of them.
///
/// What went wrong is not said. The driver's word for it is English and goes in the audit trail;
/// putting it in this sentence would splice an untranslated fragment into a translated one, and the
/// difference between a backend that was down and a reply nobody could read a verdict out of is the
/// same fact to the person answering.
pub(super) fn verdict_rows(
    verdict: Verdict,
    reason: Option<&String>,
    margin: &Span<'static>,
    width: usize,
) -> Vec<Line<'static>> {
    let (banner, colour) = match verdict {
        Verdict::Safe => (t!(check_safe), theme::ok()),
        Verdict::Unsafe => (t!(check_unsafe), theme::fail()),
        Verdict::Inconclusive(_) => (t!(check_inconclusive), theme::running()),
    };
    let mut rows = indented(
        banner,
        Style::default().fg(colour).add_modifier(Modifier::BOLD),
        width,
    );
    if let Some(reason) = reason {
        rows.extend(marked_rows(
            margin,
            &[Span::styled(
                reason.clone(),
                Style::default().fg(theme::muted()),
            )],
            width,
        ));
    }
    rows
}

/// How much further the body goes, or that there is nothing below.
pub(crate) fn scroll_hint(below: u16) -> String {
    if below > 0 {
        format!("   {}", t!(scroll_more, count = below))
    } else {
        format!("   {}", t!(scroll_back))
    }
}

/// The style of a key that approves, muted where this draw takes no yes.
pub(super) fn approving(colour: Color, answerable: bool) -> Style {
    Style::default()
        .fg(if answerable { colour } else { theme::muted() })
        .add_modifier(Modifier::BOLD)
}

/// The style of a key that refuses, which every draw takes.
pub(super) fn refusing() -> Style {
    Style::default()
        .fg(theme::fail())
        .add_modifier(Modifier::BOLD)
}

/// The key that stops the turn, which every draw takes.
pub(super) fn stopping() -> [Span<'static>; 2] {
    [
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(stop_the_turn)),
            Style::default().fg(theme::muted()),
        ),
    ]
}

/// A question's keys, with `y` muted where a yes is not taken from this draw.
pub(super) fn answer_keys(yes: &str, no: &str, answerable: bool) -> Line<'static> {
    let mut spans = yes_and_no(yes, no, answerable);
    spans.push(Span::raw("    "));
    spans.extend(stopping());
    Line::from(spans)
}

/// The `y` and `n` of a question's keys, `y` muted where a yes is not taken from this draw.
pub(super) fn yes_and_no(yes: &str, no: &str, answerable: bool) -> Vec<Span<'static>> {
    vec![
        Span::styled("  y", approving(theme::ok(), answerable)),
        Span::raw(format!(" {yes}    ")),
        Span::styled("n", refusing()),
        Span::raw(format!(" {no}")),
    ]
}

/// Draw the outer box of a prompt, and return the area inside its border.
///
/// The theme's own background as well as its border, because `Clear` empties cells without
/// colouring them: a panel that painted only its border would be a hole in the palette, with the
/// themed transcript still drawn around it, and the boundary between what the system is asking and
/// what somebody else's bytes say is exactly what a person is reading when they answer. The text
/// colour comes with it, so a span that sets none of its own is the theme's rather than the
/// terminal's.
pub(super) fn panel(frame: &mut ratatui::Frame, area: Rect, border: Color, title: &str) -> Rect {
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .title(format!(" {title} "))
        .style(Style::default().bg(theme::background()).fg(theme::text()));
    // Measured before the block is handed over, because the bodies are laid out against the width
    // they will be drawn at: a margin decided without knowing the width is a margin the first
    // wrapped row escapes.
    let inside = block.inner(area);
    frame.render_widget(block, area);
    inside
}

/// A centred box, sized to the terminal but never larger than it.
pub(super) fn centred(area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(5),
            Constraint::Percentage(90),
            Constraint::Percentage(5),
        ])
        .split(vertical[1])[1]
}
