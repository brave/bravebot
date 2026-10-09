//! The mark an empty session opens on.
//!
//! Drawn in the transcript's own area rather than as a splash, so the first reply lands where
//! the mark was and nothing has to be dismissed. It is left aligned on a margin of its own, and
//! floated down from the top edge, because a block pinned into the corner reads as an error
//! message rather than as a title.

use bravebot_i18n::t;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::avatar;
use crate::theme;
use crate::theme::Rgb;

/// The wordmark, one string per row, padded to a rectangle.
///
/// Padded because the rows are otherwise three different lengths, and a row that ends early is a
/// row nothing can be checked against: the test that the mark is a rectangle is what would catch
/// a stroke lost while editing it.
const LOGO: &[&str] = &[
    "██                                 ██             ██   ",
    "██▀▀█▄ ██▀█ ▄█▀▀██ ██  ██ ▄█▀▀██   ██▀▀█▄ ▄█▀▀█▄ ▀██▀▀ ",
    "██░░██░██░░░██░░██░▀█▄▄█▀░██▀▀▀▀░  ██░░██░██░░██░ ██░░░",
    "██▄▄█▀░██░  ▀█▄▄██░ ▀██▀░░▀█▄▄▄▀░  ██▄▄█▀░▀█▄▄█▀░  ▀█▄▄",
    " ░░░░░░ ░░   ░░░░░░  ░░░░  ░░░░░░   ░░░░░░ ░░░░░░   ░░░",
];

const _: () = assert!(LOGO.len() == avatar::ROWS);

/// The columns between the face and the mark.
const FACE_GAP: &str = "  ";

/// The character the mark's drop shadow is drawn with.
///
/// Coloured apart from the letterform, since a shadow in the same ink as the letters is not a
/// shadow: it reads as a second, blurrier copy of the word.
const SHADOW: char = '░';

/// The column the name divides at.
///
/// Only "brave" carries the brand, and "bot" is drawn in whatever ink the terminal is set to, so
/// the mark reads as the name of a thing built on Brave rather than as a five letter logo with
/// three more letters stuck to it. Columns 33 and 34 are blank down every row, which is the gap
/// between the halves, and the seam falls after it so "bot" begins on its own first stroke.
const SEAM: usize = 35;

/// The rows that carry the letterform. The rows below them are shadow alone, so the gradient is
/// spread over these and reaches its last colour on the bottom of the letters.
const LETTER_ROWS: usize = 4;

/// The left margin, wider than the transcript's lead so the mark sits clear of the edge.
const INDENT: &str = "   ";

/// Rows kept above the mark at most, however tall the terminal is.
///
/// A tall terminal would otherwise push the mark into the middle of the screen, a long way from
/// the box the user is about to type in.
const MAX_TOP: u16 = 5;

/// Columns the mark itself needs.
fn mark_width() -> usize {
    LOGO.first().map_or(0, |row| row.chars().count())
}

/// Whether the mark fits beside its margin.
///
/// A terminal narrower than this wraps the mark rather than clipping it, which folds the second
/// half of the word under the first and reads as a rendering fault. Dropping it is the better
/// failure: the name is written out below in any case.
fn fits(width: u16) -> bool {
    width as usize >= INDENT.len() + mark_width()
}

/// Whether the face fits to the left of the mark.
///
/// Dropped on its own before the mark is, since the mark carries the name.
fn face_fits(width: u16) -> bool {
    width as usize >= INDENT.len() + avatar::WIDTH + FACE_GAP.len() + mark_width()
}

/// The face drawn to the left of the mark in the mark's colours, one list of spans per row, or
/// nothing where there is no room for it or no colour was asked for.
fn face_rows(
    seed: &str,
    look: avatar::Look,
    stops: &[Rgb; 3],
    width: u16,
    plain: bool,
) -> Vec<Vec<Span<'static>>> {
    if plain || !face_fits(width) {
        return Vec::new();
    }
    avatar::rows(seed, look, stops)
}

/// Rows the block occupies, so the padding can be measured against what is left.
fn height(with_mark: bool) -> u16 {
    // The mark when there is room for it, a blank, the name, and room for the invitation and the
    // line or two the session reports about starting up.
    let mark = if with_mark { LOGO.len() as u16 } else { 0 };
    mark + 5
}

/// Blank rows above the mark, given the height the transcript has.
///
/// A third of what is spare rather than half: the mark belongs above the middle of its area,
/// since the input box sits below it and the eye should travel downwards to reach it.
fn top_padding(width: u16, available: u16) -> u16 {
    (available.saturating_sub(height(fits(width))) / 3).min(MAX_TOP)
}

/// The colour at one cell of the branded half, on a gradient through `stops`, which are
/// [`theme::mark`]'s.
///
/// The position is measured along the diagonal from the top-left corner to the bottom-right, with
/// rows and columns each counting for half of it. The block is far wider than it is tall, so a row
/// moves the colour as far as eleven columns do and the bands run nearly level, sloping down to
/// the left. Both other corners land on the middle stop.
///
/// Mixed in whole channel steps rather than by ratio, since the gradient spans at most a few dozen
/// points of a channel between stops: floating point buys no shade the terminal could show.
///
/// The mark is the largest thing on the screen and the most obviously painted, and its gradient is
/// mixed here rather than taken from the palette, so it is where a request for no colour would go
/// unheeded first. Whether any was wanted is taken as an argument rather than read here, so the
/// rule can be checked without putting a process-wide switch in force under every other test in
/// this binary.
fn brand_at(stops: &[Rgb; 3], row: usize, column: usize, plain: bool) -> Color {
    if plain {
        return Color::Reset;
    }
    let across = (SEAM - 1) as i32;
    let down = (LETTER_ROWS - 1) as i32;
    let half = across * down;
    let at = row.min(LETTER_ROWS - 1) as i32 * across + column.min(SEAM - 1) as i32 * down;
    let (start, end, at) = if at <= half {
        (stops[0], stops[1], at)
    } else {
        (stops[1], stops[2], at - half)
    };
    let mix = |from: u8, to: u8| (from as i32 + (to as i32 - from as i32) * at / half) as u8;

    Color::Rgb(
        mix(start.0, end.0),
        mix(start.1, end.1),
        mix(start.2, end.2),
    )
}

/// The ink a character takes, given the row and column it sits in.
///
/// Past the seam the letterform is left unstyled rather than given a colour of its own, so it
/// takes whatever the terminal is set to and stays legible on a light theme and a dark one alike.
/// The shadow keeps out of the gradient: it is depth rather than part of the mark's colour.
fn ink(stops: &[Rgb; 3], row: usize, column: usize, character: char) -> Style {
    if character == SHADOW {
        Style::default().fg(theme::muted())
    } else if column < SEAM {
        Style::default().fg(brand_at(stops, row, column, theme::no_color()))
    } else {
        Style::default()
    }
}

/// Row `index` of the mark, with the shadow inked apart from the letterform and "brave" apart
/// from "bot", after the row of the face when there is one.
fn mark_row(stops: &[Rgb; 3], index: usize, row: &str, face: &[Span<'static>]) -> Line<'static> {
    let mut spans = vec![Span::raw(INDENT)];
    if !face.is_empty() {
        spans.extend(face.iter().cloned());
        spans.push(Span::raw(FACE_GAP));
    }
    let mut run = String::new();
    let mut current: Option<Style> = None;

    // Batched by the ink itself rather than by which half the column is in, so the columns the
    // gradient rounds to the same shade stay one span instead of one span each.
    for (column, character) in row.chars().enumerate() {
        let next = ink(stops, index, column, character);
        if let Some(had) = current.filter(|had| *had != next) {
            spans.push(Span::styled(std::mem::take(&mut run), had));
        }
        current = Some(next);
        run.push(character);
    }
    if let Some(had) = current {
        spans.push(Span::styled(run, had));
    }

    Line::from(spans)
}

/// The head of the opening screen: the mark, and the name beneath it.
///
/// `width` and `available` are the transcript area's. The first decides whether the mark is drawn
/// at all and the second how far down it floats. A terminal too short for the padding gets none.
///
/// `confinement` is what the platform can enforce over a process that runs code we did not write,
/// and is deliberately about the platform rather than about this session: nothing the session runs
/// is inside that boundary. The words say so, and `/status` has the room to say the rest.
///
/// `tier` says whether this build can reach the premium host, and is deliberately about the
/// configuration rather than about the credentials: a batch on disk may be expired, exhausted, or
/// issued for another environment, so finding one would not settle the tier either. What was
/// actually spent is settled by the first turn, which says so if it could not spend anything, and by
/// `/status` afterwards.
///
/// Stops at the name because whatever the session has to report about starting up goes next, and
/// [`invitation`] closes the screen underneath that.
pub fn lines(confinement: &str, tier: &str, width: u16, available: u16) -> Vec<Line<'static>> {
    lines_with_network(
        confinement,
        tier,
        network_shown(
            bravebot_config::run_network(),
            bravebot_config::sandbox::in_force().mode,
        ),
        bravebot_config::settled_sandbox_filesystem()
            .is_some_and(|settled| !settled.lists.is_empty()),
        width,
        available,
    )
}

/// The network the opening screen reports: open under `off`, which confines nothing for a closed
/// network to bind, so the screen does not claim a boundary the session does not have.
fn network_shown(
    network: bravebot_sandbox::network::Network,
    mode: bravebot_sandbox::SandboxMode,
) -> bravebot_sandbox::network::Network {
    match mode {
        bravebot_sandbox::SandboxMode::Off => bravebot_sandbox::network::Network::Open,
        _ => network,
    }
}

/// [`lines`] for a session whose programs have `network`, which is named under the tier where it is
/// not open, and which says where the person wrote filesystem rules. Said here and not only in
/// `/status` because each changes what a command can do, and a session that set one should not have
/// to be asked.
pub fn lines_with_network(
    confinement: &str,
    tier: &str,
    network: bravebot_sandbox::network::Network,
    filesystem_rules: bool,
    width: u16,
    available: u16,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    for _ in 0..top_padding(width, available) {
        lines.push(Line::raw(""));
    }

    if fits(width) {
        let stops = theme::mark();
        let face = face_rows(
            avatar::startup_seed(),
            avatar::look_now(),
            &stops,
            width,
            theme::no_color(),
        );
        lines.extend(LOGO.iter().enumerate().map(|(index, row)| {
            mark_row(
                &stops,
                index,
                row,
                face.get(index).map_or(&[][..], Vec::as_slice),
            )
        }));
        lines.push(Line::raw(""));
    }

    lines.push(Line::from(vec![
        Span::raw(INDENT),
        Span::styled("bravebot", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(
                "  ·  {}  ·  {tier}",
                t!(opening_confinement, level = confinement)
            ),
            Style::default().fg(theme::muted()),
        ),
    ]));
    if network.is_closed() {
        lines.push(Line::from(vec![
            Span::raw(INDENT),
            Span::styled(
                t!(opening_network_closed).to_string(),
                Style::default().fg(theme::muted()),
            ),
        ]));
    }
    if filesystem_rules {
        lines.push(Line::from(vec![
            Span::raw(INDENT),
            Span::styled(
                t!(opening_filesystem_rules).to_string(),
                Style::default().fg(theme::muted()),
            ),
        ]));
    }

    lines
}

/// The line that closes the opening screen, drawn under whatever the session reported.
///
/// Last rather than tucked under the name, so it sits nearest the box it is asking the user to
/// type in and reads as the next thing to do rather than as part of the title.
pub fn invitation() -> Line<'static> {
    Line::from(vec![
        Span::raw(INDENT),
        Span::styled(t!(opening_invitation), Style::default().fg(theme::muted())),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::BRAND_STOPS as STOPS;

    /// Wide enough for the mark, which most of these are not about.
    const WIDE: u16 = 90;

    fn rows(width: u16, available: u16) -> Vec<String> {
        lines("kernel-enforced", "premium available", width, available)
            .iter()
            .chain(std::iter::once(&invitation()))
            .map(|line| line.to_string())
            .collect()
    }

    /// The mark is the point of the screen, so a short terminal loses the padding and keeps it.
    #[test]
    fn a_short_terminal_drops_the_padding_rather_than_the_mark() {
        let rows = rows(WIDE, 3);
        assert_eq!(top_padding(WIDE, 3), 0);
        assert!(rows[0].contains('█'), "the mark was pushed off: {rows:?}");
    }

    /// Glued to the top is what this exists to avoid.
    #[test]
    fn a_tall_terminal_floats_the_mark_down_from_the_top() {
        let rows = rows(WIDE, 40);
        assert!(
            rows.first().is_some_and(|row| row.trim().is_empty()),
            "the mark is against the top edge: {rows:?}"
        );
    }

    /// And a very tall one does not float it into the middle, a long way from the input box.
    #[test]
    fn the_padding_is_bounded_however_tall_the_terminal_is() {
        assert_eq!(top_padding(WIDE, 200), MAX_TOP);
    }

    /// Left aligned on a margin of its own: against the side edge is the other half of what
    /// this exists to avoid.
    #[test]
    fn every_row_of_the_mark_is_indented() {
        for row in rows(WIDE, 24).iter().filter(|row| !row.trim().is_empty()) {
            assert!(row.starts_with(INDENT), "glued to the edge: {row}");
        }
    }

    /// The mark says nothing a screen reader or a narrow pane can use, so the name is written
    /// out too, next to the confinement this platform offers. Offers rather than applies: a person
    /// who reads a level here as a boundary around the session has been told the opposite of what
    /// is true of every read, write and program it runs.
    ///
    /// The tier is beside it because a person who is paying for premium wants to know at a glance
    /// that this session can reach it. Learning otherwise from a worse answer several turns in is
    /// the failure this line exists to prevent.
    #[test]
    fn the_mark_names_the_agent_its_confinement_and_its_tier() {
        let all = rows(WIDE, 24).join("\n");
        assert!(all.contains("bravebot"), "{all}");
        assert!(
            all.contains("confinement available: kernel-enforced"),
            "{all}"
        );
        assert!(all.contains("premium available"), "{all}");
        assert!(all.contains("Ask a question"), "{all}");
    }

    /// A session with filesystem rules of the person's own says so, and one with none says nothing.
    #[test]
    fn filesystem_rules_are_named_on_the_opening_screen_and_none_is_not() {
        use bravebot_sandbox::network::Network;
        let drawn = |rules| {
            lines_with_network(
                "kernel-enforced",
                "premium available",
                Network::Open,
                rules,
                WIDE,
                24,
            )
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
        };
        assert!(drawn(true).contains(t!(opening_filesystem_rules)));
        assert!(!drawn(false).contains("filesystem"), "{}", drawn(false));
    }

    /// A session that closed the network says so on the screen it opens on, and one that did not
    /// says nothing, as the screen never did.
    #[test]
    fn a_closed_network_is_named_on_the_opening_screen_and_an_open_one_is_not() {
        use bravebot_sandbox::network::Network;
        let drawn = |network| {
            lines_with_network(
                "kernel-enforced",
                "premium available",
                network,
                false,
                WIDE,
                24,
            )
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
        };
        assert!(drawn(Network::Closed).contains(t!(opening_network_closed)));
        assert!(
            !drawn(Network::Open).contains("network"),
            "{}",
            drawn(Network::Open)
        );
    }

    /// SANDBOX-22: under `off` nothing confines a program, so the opening screen does not claim a
    /// closed network. The regression it rejects is "network closed" over a session whose programs
    /// reach the network freely.
    #[test]
    fn a_closed_network_is_not_claimed_for_programs_nothing_confines() {
        use bravebot_sandbox::SandboxMode;
        use bravebot_sandbox::network::Network;
        for mode in [SandboxMode::Standard, SandboxMode::Strict] {
            assert_eq!(network_shown(Network::Closed, mode), Network::Closed);
        }
        assert_eq!(
            network_shown(Network::Closed, SandboxMode::Off),
            Network::Open
        );
    }

    /// A pane too narrow for the mark still has to say what the platform offers, since dropping
    /// the mark must not take the tier and the confinement with it.
    #[test]
    fn a_narrow_pane_still_reports_the_confinement_and_the_tier() {
        let narrow = (INDENT.len() + mark_width() - 1) as u16;
        let all = rows(narrow, 24).join("\n");
        assert!(
            all.contains("confinement available: kernel-enforced"),
            "{all}"
        );
        assert!(all.contains("premium available"), "{all}");
    }

    /// Wrapping the word under itself reads as a rendering fault, so a pane too narrow for the
    /// mark gets the name instead and the greeting still says what the session is.
    #[test]
    fn a_narrow_pane_drops_the_mark_rather_than_wrapping_it() {
        let narrow = (INDENT.len() + mark_width() - 1) as u16;
        let all = rows(narrow, 24).join("\n");
        assert!(!all.contains('█'), "the mark will wrap: {all}");
        assert!(all.contains("bravebot"), "nothing was left to read: {all}");
    }

    /// Every row the same width, or a stroke was lost while editing the mark.
    #[test]
    fn the_marks_rows_are_all_the_same_width() {
        let widths: Vec<usize> = LOGO.iter().map(|row| row.chars().count()).collect();
        assert!(
            widths.windows(2).all(|pair| pair[0] == pair[1]),
            "ragged: {widths:?}"
        );
    }

    /// The ink each column of a row is drawn in, the margin aside.
    fn inks(index: usize) -> Vec<Option<Color>> {
        mark_row(&STOPS, index, LOGO[index], &[])
            .spans
            .iter()
            .skip(1)
            .flat_map(|span| std::iter::repeat_n(span.style.fg, span.content.chars().count()))
            .collect()
    }

    /// The shadow is what gives the letterform its depth, so it is inked apart from it rather
    /// than drawn in the same orange.
    #[test]
    fn the_shadow_is_not_drawn_in_the_letterforms_ink() {
        let _held = theme::exclusive();
        let inks = inks(2);
        assert_eq!(
            inks[0],
            Some(brand_at(&STOPS, 2, 0, false)),
            "no letterform: {inks:?}"
        );
        assert!(inks.contains(&Some(theme::muted())), "no shadow: {inks:?}");
    }

    /// Only the half of the name that is a brand is drawn in the brand's colours.
    #[test]
    fn the_orange_stops_at_the_end_of_brave() {
        let _held = theme::exclusive();
        let inks = inks(1);
        assert_eq!(
            inks[0],
            Some(brand_at(&STOPS, 1, 0, false)),
            "brave lost its ink"
        );
        let muted = theme::muted();
        assert!(
            inks[SEAM..]
                .iter()
                .all(|ink| ink.is_none() || *ink == Some(muted)),
            "bot was branded too: {:?}",
            &inks[SEAM..]
        );
    }

    /// The mark is the largest painted thing a session opens with, and its gradient is mixed from
    /// literals rather than taken from the palette, so it is where a request for no colour goes
    /// unheeded first.
    #[test]
    fn the_wordmark_takes_no_colour_where_none_was_asked_for() {
        for row in 0..LOGO.len() {
            for column in [0, SEAM / 2, SEAM - 1] {
                assert_eq!(
                    brand_at(&STOPS, row, column, true),
                    Color::Reset,
                    "({row}, {column})"
                );
            }
        }
    }

    fn rgb((r, g, b): Rgb) -> Color {
        Color::Rgb(r, g, b)
    }

    /// The gradient runs corner to corner: the first orange at the top left, the last at the
    /// bottom right of the letters, and the middle one at the other two corners. A gradient running
    /// only across or only down puts the last orange on one of those two corners.
    #[test]
    fn the_gradient_runs_diagonally_through_the_three_oranges() {
        let bottom = LETTER_ROWS - 1;
        let right = SEAM - 1;
        assert_eq!(brand_at(&STOPS, 0, 0, false), rgb(STOPS[0]));
        assert_eq!(brand_at(&STOPS, 0, right, false), rgb(STOPS[1]));
        assert_eq!(brand_at(&STOPS, bottom, 0, false), rgb(STOPS[1]));
        assert_eq!(brand_at(&STOPS, bottom, right, false), rgb(STOPS[2]));

        let shades: Vec<Color> = (0..SEAM)
            .map(|column| brand_at(&STOPS, 0, column, false))
            .collect();
        let steps = shades.windows(2).filter(|pair| pair[0] != pair[1]).count();
        assert!(steps > 4, "the fade is too coarse to read as one: {steps}");
    }

    /// The bands slope rather than lie level or stand upright: one row down moves the colour
    /// further than several columns across, and the columns move it too.
    #[test]
    fn a_row_moves_the_gradient_further_than_several_columns() {
        let green = |row: usize, column: usize| match brand_at(&STOPS, row, column, false) {
            Color::Rgb(_, green, _) => green,
            other => panic!("not a mixed colour: {other:?}"),
        };
        let across = green(0, 0) - green(0, 5);
        let down = green(0, 0) - green(1, 0);
        assert!(across > 0, "the columns share one shade");
        assert!(down > across, "the gradient runs across rather than down");
    }

    /// A shade is never skipped backwards, along a row or down a column: a gradient that reversed
    /// anywhere would read as a banding fault rather than as a fade.
    #[test]
    fn the_gradient_only_ever_travels_one_way() {
        let green = |row: usize, column: usize| match brand_at(&STOPS, row, column, false) {
            Color::Rgb(_, green, _) => green,
            other => panic!("not a mixed colour: {other:?}"),
        };
        for row in 0..LOGO.len() {
            for column in 0..SEAM {
                if column > 0 {
                    assert!(
                        green(row, column) <= green(row, column - 1),
                        "({row}, {column})"
                    );
                }
                if row > 0 {
                    assert!(
                        green(row, column) <= green(row - 1, column),
                        "({row}, {column})"
                    );
                }
            }
        }
    }

    /// The gradient ends on the bottom of the letters, which is only true while the rows past
    /// [`LETTER_ROWS`] are shadow alone and the row above them is not.
    #[test]
    fn the_letterform_ends_where_the_gradient_does() {
        let letterform = |row: &str| row.chars().any(|c| c != ' ' && c != SHADOW);
        assert!(letterform(LOGO[LETTER_ROWS - 1]));
        assert!(!LOGO[LETTER_ROWS..].iter().any(|row| letterform(row)));
    }

    /// Past the seam the letterform takes the terminal's own ink, so the mark reads on a light
    /// theme as well as a dark one.
    #[test]
    fn bot_is_left_in_the_terminals_own_ink() {
        let inks = inks(1);
        assert!(
            inks[SEAM..].contains(&None),
            "bot was given a colour: {inks:?}"
        );
    }

    /// The seam is a column count into art that may be edited, so it is pinned to the gap it is
    /// meant to fall in: a stroke either side of it would be cut in half by the colour change.
    #[test]
    fn the_seam_falls_in_the_gap_between_the_two_halves() {
        for row in LOGO {
            let columns: Vec<char> = row.chars().collect();
            assert_eq!(columns[SEAM - 2], ' ', "the seam cuts a stroke: {row}");
            assert_eq!(columns[SEAM - 1], ' ', "the seam cuts a stroke: {row}");
        }
    }

    /// The face sits to the left of the mark on every row of it.
    #[test]
    fn the_face_is_drawn_to_the_left_of_the_mark() {
        let face = face_rows("v2:example-0", avatar::Look::default(), &STOPS, WIDE, false);
        assert_eq!(face.len(), LOGO.len());
        for (index, (row, face)) in LOGO.iter().zip(&face).enumerate() {
            let line = mark_row(&STOPS, index, row, face).to_string();
            let drawn: String = face.iter().map(|span| span.content.as_ref()).collect();
            assert_eq!(line, format!("{INDENT}{drawn}{FACE_GAP}{row}"));
        }
    }

    /// A pane with room for the mark and not the face keeps the mark, as it always did.
    #[test]
    fn a_pane_too_narrow_for_the_face_keeps_the_mark_alone() {
        let width = (INDENT.len() + mark_width()) as u16;
        assert!(fits(width) && !face_fits(width));
        assert!(
            face_rows(
                "v2:example-0",
                avatar::Look::default(),
                &STOPS,
                width,
                false
            )
            .is_empty()
        );
        assert!(face_fits(width + (avatar::WIDTH + FACE_GAP.len()) as u16));
    }

    fn opening_lines(width: u16) -> Vec<Line<'static>> {
        lines_with_network(
            "kernel-enforced",
            "premium available",
            bravebot_sandbox::network::Network::Open,
            false,
            width,
            24,
        )
    }

    /// The opening screen itself draws the face, painted, beside every row of the mark, and drops
    /// it for a width that fits the mark alone.
    #[test]
    fn the_opening_screen_draws_the_face_beside_the_mark_and_drops_it_when_narrow() {
        let mark_only = (INDENT.len() + mark_width()) as u16;
        for (width, with_face) in [(WIDE, true), (mark_only, false)] {
            let screen = opening_lines(width);
            let mut painted = false;
            for row in LOGO {
                let line = screen
                    .iter()
                    .find(|line| line.to_string().ends_with(row))
                    .unwrap_or_else(|| panic!("no line ends in the mark row {row:?}"));
                let text = line.to_string();
                let lead = text.chars().count() - row.chars().count();
                if with_face {
                    assert_eq!(
                        lead,
                        INDENT.len() + avatar::WIDTH + FACE_GAP.len(),
                        "{text:?}"
                    );
                    painted |= line.spans[1..=avatar::WIDTH]
                        .iter()
                        .any(|span| matches!(span.style.fg, Some(Color::Rgb(..))));
                } else {
                    assert_eq!(lead, INDENT.len(), "{text:?}");
                }
            }
            assert_eq!(painted, with_face);
        }
    }

    /// The opening screen draws the mark and the face in the theme in force: under a named theme
    /// "brave" opens in that theme's note ink and the face changes with it, and putting `brave`
    /// back brings the oranges back.
    #[test]
    fn the_opening_screen_draws_the_mark_in_the_theme_in_force() {
        let _held = theme::exclusive();
        let first_row = |screen: &[Line<'static>]| {
            screen
                .iter()
                .find(|line| line.to_string().ends_with(LOGO[0]))
                .expect("the mark was drawn")
                .clone()
        };
        let face_inks = |line: &Line<'static>| -> Vec<Option<Color>> {
            line.spans[1..=avatar::WIDTH]
                .iter()
                .flat_map(|span| [span.style.fg, span.style.bg])
                .collect()
        };
        let letter = avatar::WIDTH + 2;

        let nord = theme::find("nord").expect("nord is built in");
        theme::apply(&nord);
        let themed = first_row(&opening_lines(WIDE));
        theme::apply_brave();
        let branded = first_row(&opening_lines(WIDE));

        assert_eq!(themed.spans[letter].style.fg, Some(nord.palette.note));
        assert_eq!(branded.spans[letter].style.fg, Some(rgb(STOPS[0])));
        assert_ne!(face_inks(&themed), face_inks(&branded));
    }

    /// A request for no colour gets no painted face.
    #[test]
    fn the_face_takes_no_colour_where_none_was_asked_for() {
        assert!(face_rows("v2:example-0", avatar::Look::default(), &STOPS, WIDE, true).is_empty());
    }
}
