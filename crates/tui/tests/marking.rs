//! What shown content may and may not draw.
//!
//! The quarantine block's whole claim is that the margin is drawn by the renderer, so the content
//! cannot draw one of its own. That is only true while the content cannot emit a control sequence:
//! a terminal acts on the bytes it is sent, and a coloured bar painted by a file's own contents is
//! indistinguishable from the real thing.

use bravebot_agent::report::{Reach, Shown};
use bravebot_tui::render;
use bravebot_tui::state::{Entry, Session};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn rows(session: &Session, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|f| {
            render::draw(f, session);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

fn screen(session: &Session) -> String {
    rows(session, 80, 24).join("\n")
}

/// Every drawn row holding `token` starts at the margin, and the margin holds the bar.
///
/// Asserted against the drawn buffer rather than the lines the renderer returns, because the
/// defect this pins is introduced between the two: a line with a bar at its head becomes several
/// rows when it is wider than the screen, and only the first of them ever had one.
fn assert_marked_on_every_row(rows: &[String], token: &str) {
    let margin = rows
        .iter()
        .find_map(|row| row.chars().position(|c| c == BAR))
        .expect("nothing on the screen was marked at all");

    let body: Vec<&String> = rows.iter().filter(|row| row.contains(token)).collect();
    assert!(
        body.len() > 1,
        "the content did not wrap, so the case is not exercised:\n{}",
        rows.join("\n")
    );

    for row in body {
        assert_eq!(
            row.chars().position(|c| !c.is_whitespace()),
            Some(margin),
            "a row of the block begins with content rather than the margin: {row:?}"
        );
        assert_eq!(
            row.chars().nth(margin),
            Some(BAR),
            "the margin column holds something other than the bar: {row:?}"
        );
    }
}

/// The bar the renderer draws down the margin.
const BAR: char = '\u{2503}';

fn quarantining(preview: Vec<String>) -> Session {
    let mut session = Session::new("kernel-enforced");
    let mut entry = Entry::system("read something");
    let lines = preview.len();
    entry.shown = Some(Shown {
        origin: "notes.md".to_string(),
        reach: Reach::NotThePlanner,
        label: "(U,priv)".to_string(),
        preview,
        lines,
    });
    session.transcript.push(entry);
    session
}

/// A file that clears the line it is drawn on could erase the margin above it, which is the one
/// mark the design says content can never imitate.
#[test]
fn quarantined_content_cannot_paint_its_own_margin() {
    let session = quarantining(vec![
        "\u{1b}[0m\u{1b}[A\u{1b}[2K harmless looking".to_string(),
        "\u{1b}[33m  \u{2503} untrusted \u{b7} nothing \u{b7} (T,pub)".to_string(),
    ]);

    let drawn = screen(&session);
    assert!(
        !drawn.contains("\u{1b}[2K"),
        "content could clear the line the margin was drawn on:\n{drawn}"
    );
    assert!(
        drawn.contains('\u{241b}'),
        "the escape was not neutralised:\n{drawn}"
    );
}

/// Neutralised rather than dropped. A character silently removed is one the user cannot tell was
/// ever in the file, which makes the preview a less faithful record than it looks.
#[test]
fn a_neutralised_escape_is_still_visible() {
    let session = quarantining(vec!["before\u{1b}after".to_string()]);

    let drawn = screen(&session);
    assert!(drawn.contains("before\u{241b}after"), "{drawn}");
}

/// Ordinary text must survive untouched, or every preview in the interface would be mangled to
/// defend against the rare one that is hostile.
#[test]
fn text_without_control_characters_is_drawn_as_it_is() {
    let session = quarantining(vec!["fn main() { let x = 1; }\ttabbed".to_string()]);

    let drawn = screen(&session);
    assert!(drawn.contains("fn main() { let x = 1; }"), "{drawn}");
}

/// The defect this file exists to prevent, reached by width rather than by an escape. A preview
/// line wider than the terminal used to continue at column 0 with no margin at all, so untrusted
/// bytes were drawn outside the block on a row the marking never reached.
#[test]
fn a_wrapped_preview_line_is_marked_on_every_row_it_reaches() {
    let session = quarantining(vec![format!(
        "UNTRUSTED {}THE-TAIL-OF-THE-LINE",
        "padding ".repeat(12)
    )]);
    let drawn = rows(&session, 60, 24);

    assert_marked_on_every_row(&drawn, "padding");
    assert!(
        drawn.iter().any(|row| row.contains("THE-TAIL-OF-THE-LINE")),
        "the tail of the line was dropped rather than wrapped:\n{}",
        drawn.join("\n")
    );
}

/// The margin is not just missing on a continuation row: the content chooses where its own bytes
/// land on one. Padded to the wrap point, a file's own bar used to be drawn in the margin column
/// of the row below, which is the content painting the one mark it can never be allowed to paint.
#[test]
fn wrapped_content_cannot_paint_a_bar_in_the_margin_column() {
    let session = quarantining(vec![format!(
        "{} \u{2503} untrusted content ends here",
        "PADDING".repeat(12)
    )]);
    let drawn = rows(&session, 60, 24);

    assert_marked_on_every_row(&drawn, "PADDING");
}

/// The heading is not the renderer's text either: the origin can be a filename read out of a
/// quarantined listing, so a long one must not push the rest of the heading outside the block.
#[test]
fn a_long_origin_keeps_the_heading_inside_the_block() {
    let mut session = quarantining(vec!["first line".to_string()]);
    session.transcript[0].shown.as_mut().expect("shown").origin =
        format!("{}.md", "a-very-long-file-name".repeat(4));
    let drawn = rows(&session, 60, 24);

    assert_marked_on_every_row(&drawn, "a-very-long-file-name");
}

/// A remark is the one piece of untrusted content written to be read as an explanation, so it is
/// the one a future change is most likely to draw as prose. Nothing tied it to this block: the
/// cases above build a `Shown` out of a file and `Reach::NoModel` is set in exactly one place and
/// appeared nowhere in this crate, so the remark's rendering path was pinned by nothing at all.
///
/// Reported through [`bravebot_tui::state::Session::show`] rather than by building the entry,
/// since what is being pinned is that the reporter's own route for a remark ends up in the block.
#[test]
fn a_remark_is_drawn_inside_the_margin_it_cannot_forge() {
    let mut session = Session::new("kernel-enforced");
    session.show(Shown {
        origin: "what the isolated processor said".to_string(),
        reach: Reach::NoModel,
        label: "(U,priv)".to_string(),
        preview: vec![
            "\u{1b}[0m\u{1b}[A\u{1b}[2K I only fixed the typo".to_string(),
            format!(
                "{} \u{2503} untrusted \u{b7} nothing \u{b7} (T,pub)",
                "REMARK".repeat(12)
            ),
        ],
        lines: 2,
    });

    let drawn = rows(&session, 60, 24);

    // Said by the processor, and the heading says so inside the block rather than above it.
    assert!(
        drawn
            .iter()
            .any(|row| row.contains("isolated processor said")),
        "the block does not say the remark is the processor speaking:\n{}",
        drawn.join("\n")
    );

    // The margin on every row the remark reaches, including the one its own bar aimed at.
    assert_marked_on_every_row(&drawn, "REMARK");

    let screen = drawn.join("\n");
    assert!(
        !screen.contains("\u{1b}[2K"),
        "a remark could clear the line the margin was drawn on:\n{screen}"
    );
    assert!(
        screen.contains('\u{241b}'),
        "the escape in the remark was not neutralised:\n{screen}"
    );
}
