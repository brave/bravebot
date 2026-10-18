//! Terminal hyperlinks (OSC 8) over cells already drawn.
//!
//! A cell holds the text the terminal is sent for it, so each cell of a link carries the whole
//! sequence around its own character. A cell redrawn alone is still linked, and [`shown`] gives
//! back the character for anything that reads the screen as text.

use ratatui::buffer::{Buffer, CellDiffOption};
use std::num::NonZeroU16;

const OPEN: &str = "\x1b]8;;";
const END: &str = "\x1b\\";
const CLOSE: &str = "\x1b]8;;\x1b\\";

/// Make the `width` cells from `(x, y)` a link to `url`, each cell showing one column.
///
/// Nothing is written for a `url` with anything but printable ASCII in it, since it goes to the
/// terminal inside an escape sequence and an escape or a bell in it would end that sequence.
pub fn link(buffer: &mut Buffer, x: u16, y: u16, width: u16, url: &str) {
    if !url.bytes().all(|b| b.is_ascii_graphic()) {
        return;
    }
    for column in x..x.saturating_add(width) {
        if let Some(cell) = buffer.cell_mut((column, y)) {
            let linked = format!("{OPEN}{url}{END}{}{CLOSE}", cell.symbol());
            // The sequence takes no column on the screen, but ratatui would count its characters.
            cell.set_symbol(&linked)
                .set_diff_option(CellDiffOption::ForcedWidth(NonZeroU16::MIN));
        }
    }
}

/// What a cell shows, without the link [`link`] put around it.
pub fn shown(symbol: &str) -> &str {
    symbol
        .strip_prefix(OPEN)
        .and_then(|rest| rest.strip_suffix(CLOSE))
        .and_then(|rest| rest.split_once(END))
        .map_or(symbol, |(_, shown)| shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn row(text: &str) -> Buffer {
        Buffer::with_lines([text])
    }

    #[test]
    fn a_linked_cell_carries_the_address_and_reads_back_as_its_character() {
        let mut buffer = row("abc");
        link(&mut buffer, 1, 0, 1, "https://x.test/1");
        assert_eq!(
            buffer[(1, 0)].symbol(),
            "\x1b]8;;https://x.test/1\x1b\\b\x1b]8;;\x1b\\"
        );
        assert_eq!(shown(buffer[(1, 0)].symbol()), "b");
        assert_eq!(
            buffer[(0, 0)].symbol(),
            "a",
            "a cell before the link was linked"
        );
        assert_eq!(
            buffer[(2, 0)].symbol(),
            "c",
            "a cell after the link was linked"
        );
    }

    /// The address travels inside an escape sequence, so one that holds an escape or a bell could
    /// end it and write whatever follows to the terminal as a command.
    #[test]
    fn an_address_that_could_end_the_sequence_is_not_written() {
        for url in [
            "https://x.test/\x1b]0;owned\x07",
            "https://x.test/ a",
            "https://x.test/\u{e9}",
        ] {
            let mut buffer = row("ab");
            link(&mut buffer, 0, 0, 2, url);
            assert_eq!(buffer, row("ab"), "{url:?} was written");
        }
    }

    #[test]
    fn a_link_past_the_buffer_is_cut_to_it() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
        link(&mut buffer, 1, 0, 5, "https://x.test/");
        assert_eq!(shown(buffer[(1, 0)].symbol()), " ");
        assert_ne!(
            buffer[(1, 0)].symbol(),
            " ",
            "the last cell in the buffer was not linked"
        );
    }
}
