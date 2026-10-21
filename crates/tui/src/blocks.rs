//! Block art in a terminal that leaves gaps between its rows.
//!
//! The wordmark and the face beside it are drawn in `█`, `▀` and `▄`. Most terminals paint those to
//! the edge of the cell. Apple's Terminal draws them from the font, in a band that starts a few
//! pixels below the top of the cell, so every row of the art is cut by a stripe, and a half block
//! over a background colour shows that colour in the stripe.
//!
//! A cell's background fills the whole cell in every terminal, and a lower half block is flush with
//! the bottom of it. [`painted`] draws a cell from those two alone.

use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::style::{Color, Modifier, Style};

/// Held in a static rather than read per cell, because the opening screen is drawn every frame.
static PAINTED: AtomicBool = AtomicBool::new(false);

/// Read which terminal this is, once, before anything is drawn.
pub fn sense() {
    PAINTED.store(
        crate::apple_terminal(std::env::var_os("TERM_PROGRAM").as_deref()),
        Ordering::Relaxed,
    );
}

/// Whether block art is drawn with [`painted`] in this terminal.
pub fn needs_painting() -> bool {
    PAINTED.load(Ordering::Relaxed)
}

/// Whether `ink` is the terminal's own foreground rather than a colour set here.
fn own(ink: Option<Color>) -> bool {
    matches!(ink, None | Some(Color::Reset))
}

/// A block cell redrawn from its background, given the character and style that draw it where
/// blocks reach the edge of the cell.
///
/// - `█` is a space on the colour. The terminal's own ink has no colour to name as a background, so
///   it is a space in reverse video.
/// - `▀` is `▄` in reverse video: the cell is filled with the upper colour and the lower half is cut
///   out in the terminal's own background. Where the cell has a lower colour as well, that colour is
///   the glyph and the upper colour the background.
/// - Everything else, `▄` included, is drawn as it was.
pub fn painted(character: char, style: Style) -> (char, Style) {
    match character {
        '█' if own(style.fg) => (
            ' ',
            Style { fg: None, ..style }.add_modifier(Modifier::REVERSED),
        ),
        '█' => (
            ' ',
            Style {
                fg: None,
                bg: style.fg,
                ..style
            },
        ),
        '▀' => match style.bg {
            None => ('▄', style.add_modifier(Modifier::REVERSED)),
            Some(lower) if own(style.fg) => (
                '▄',
                Style {
                    bg: Some(lower),
                    ..style
                }
                .add_modifier(Modifier::REVERSED),
            ),
            Some(lower) => (
                '▄',
                Style {
                    fg: Some(lower),
                    bg: style.fg,
                    ..style
                },
            ),
        },
        _ => (character, style),
    }
}

/// What a cell shows in a terminal that paints blocks to the edge of the cell, for the tests of
/// every module that draws block art.
#[cfg(test)]
pub(crate) mod shown {
    use ratatui::style::{Color, Modifier, Style};

    /// The terminal's own ink and background, which a style that names neither shows.
    const INK: Color = Color::Rgb(0xe0, 0xe0, 0xe0);
    const GROUND: Color = Color::Rgb(0x20, 0x22, 0x2a);

    /// Whether `character` is one of the characters block art is drawn in, and so has two halves.
    pub(crate) fn is_block(character: char) -> bool {
        matches!(character, '█' | '▀' | '▄' | ' ')
    }

    /// The colour of the upper and the lower half of a cell drawn with `character` in `style`.
    pub(crate) fn halves(character: char, style: Style) -> (Color, Color) {
        let resolve = |color: Option<Color>, own: Color| match color {
            None | Some(Color::Reset) => own,
            Some(color) => color,
        };
        let (mut ink, mut ground) = (resolve(style.fg, INK), resolve(style.bg, GROUND));
        if style.add_modifier.contains(Modifier::REVERSED) {
            std::mem::swap(&mut ink, &mut ground);
        }
        match character {
            '█' => (ink, ink),
            '▀' => (ink, ground),
            '▄' => (ground, ink),
            ' ' => (ground, ground),
            other => panic!("{other:?} is not a block"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::shown::halves;
    use super::*;

    const ORANGE: Color = Color::Rgb(0xf0, 0x50, 0x1e);
    const BLUE: Color = Color::Rgb(0x28, 0x5a, 0xdc);

    /// Every way the wordmark and the face draw a block cell.
    fn drawn_cells() -> Vec<(char, Style)> {
        let fg = |color| Style::default().fg(color);
        vec![
            ('█', fg(ORANGE)),
            ('█', fg(Color::Reset)),
            ('█', Style::default()),
            ('▀', fg(ORANGE)),
            ('▀', Style::default()),
            ('▀', fg(ORANGE).bg(BLUE)),
            ('▀', Style::default().bg(BLUE)),
            ('▄', fg(ORANGE)),
            ('▄', Style::default()),
            (' ', Style::default()),
        ]
    }

    /// The point of painting: whatever a cell showed where blocks reach the edge, it shows the
    /// same two halves, in every colour combination the art uses, the terminal's own ink included.
    #[test]
    fn a_painted_cell_shows_the_same_two_halves_as_the_block_it_replaces() {
        for (character, style) in drawn_cells() {
            let (painted_character, painted_style) = painted(character, style);
            assert_eq!(
                halves(painted_character, painted_style),
                halves(character, style),
                "{character:?} {style:?} became {painted_character:?} {painted_style:?}"
            );
        }
    }

    /// The characters that leave a stripe along the top of the cell in the terminal this is for
    /// are never drawn, which is what a cell that only looks right in the model above would miss.
    #[test]
    fn a_painted_cell_is_a_space_or_a_lower_half_block() {
        for (character, style) in drawn_cells() {
            let (painted_character, _) = painted(character, style);
            assert!(
                matches!(painted_character, ' ' | '▄'),
                "{character:?} {style:?} stayed {painted_character:?}"
            );
        }
    }

    /// A full block takes the colour as its background and no foreground, so nothing of it rests
    /// on a glyph.
    #[test]
    fn a_full_block_is_a_space_on_its_colour() {
        assert_eq!(
            painted('█', Style::default().fg(ORANGE)),
            (' ', Style::default().bg(ORANGE))
        );
    }

    /// The terminal's own ink cannot be named as a background, so it is asked for by reversing
    /// the cell, which adds no colour of this program's.
    #[test]
    fn the_terminals_own_ink_is_painted_by_reversing_the_cell() {
        for ink in [Style::default(), Style::default().fg(Color::Reset)] {
            let (character, style) = painted('█', ink);
            assert_eq!(character, ' ');
            assert!(style.add_modifier.contains(Modifier::REVERSED), "{style:?}");
            assert_eq!((style.fg, style.bg), (None, None), "{style:?}");
        }
    }

    /// What is not a block that stripes is left as it was, so the shadow keeps its texture and a
    /// lower half block keeps its place against the bottom of the cell.
    #[test]
    fn a_shade_a_lower_half_and_a_space_are_left_as_they_were() {
        let style = Style::default().fg(ORANGE);
        for character in ['░', '▄', ' ', 'b'] {
            assert_eq!(painted(character, style), (character, style));
        }
    }
}
