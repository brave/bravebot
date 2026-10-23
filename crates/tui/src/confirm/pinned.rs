//! The one layout a question here is drawn through, and the one place a yes is decided.
//!
//! A file of its own so that nothing outside it can make a [`Drawn`] that takes a yes. Every key
//! that approves is passed through [`Drawn::take`], and only [`draw`] says whether the rows that
//! decide the question have been on the screen. [`Pinned`] and [`rows_in`] sit beside [`Drawn`] and
//! neither builds or reads one.

use std::ops::Range;

use bravebot_i18n::t;
use ratatui::layout::{Rect, Size};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use crate::theme;

use super::scroll_hint;

/// A question to lay out.
pub(super) struct Question<'k> {
    /// Pinned above the part that scrolls.
    pub(super) above: Vec<Line<'static>>,
    /// The part that scrolls.
    pub(super) body: Vec<Line<'static>>,
    /// How many lines at the head of `body` decide the question. Every row they wrap to has to
    /// have been drawn at this width, in some draw, before a yes is taken.
    pub(super) deciding: usize,
    /// Rows after those that decide it as well: the start of what the question is about.
    pub(super) opening: u16,
    /// Rows of the body one draw has to show for a yes, or all of it where it is shorter.
    pub(super) needed: u16,
    /// Pinned below the part that scrolls.
    pub(super) below: Vec<Line<'static>>,
    /// The keys, given whether this draw takes a yes so the keys that approve can say so.
    pub(super) keys: &'k dyn Fn(bool) -> Vec<Line<'static>>,
    /// A picture over the body's blank rows from this row, painted whole or not at all, so its
    /// rows count as drawn only in a draw with room to paint it.
    pub(super) picture: Option<(u16, Size)>,
}

impl<'k> Question<'k> {
    /// A question that is all body over its keys.
    pub(super) fn scrolled(
        body: Vec<Line<'static>>,
        deciding: usize,
        opening: u16,
        keys: &'k dyn Fn(bool) -> Vec<Line<'static>>,
    ) -> Self {
        Question {
            above: Vec::new(),
            body,
            deciding,
            opening,
            needed: 1,
            below: Vec::new(),
            keys,
            picture: None,
        }
    }
}

/// Which of a question's deciding rows have been drawn so far.
///
/// Kept across draws by the prompt that asks, because the rows are seen one screen at a time.
/// A draw at another width, or with another number of deciding rows, starts it again: the rows
/// it counted are not the rows on the screen any more.
#[derive(Debug, Default)]
pub(super) struct Seen {
    width: u16,
    rows: Vec<bool>,
}

impl Seen {
    /// Count `shown` as drawn, and return how many deciding rows still have not been.
    fn mark(&mut self, width: u16, deciding: u16, shown: impl Iterator<Item = u16>) -> u16 {
        if self.width != width || self.rows.len() != usize::from(deciding) {
            *self = Seen {
                width,
                rows: vec![false; usize::from(deciding)],
            };
        }
        for row in shown {
            if let Some(seen) = self.rows.get_mut(usize::from(row)) {
                *seen = true;
            }
        }
        u16::try_from(self.rows.iter().filter(|seen| !**seen).count()).unwrap_or(u16::MAX)
    }
}

/// What one draw of a question decided for the keys that answer it.
///
/// The fields are private, and [`Default`] takes no yes, so a value that does take one comes
/// out of [`draw`] or out of nowhere.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Drawn {
    furthest: u16,
    rows: u16,
    answerable: bool,
    picture: Option<Rect>,
}

/// A key's response, as far as whether it approves anything.
pub(super) trait Approving {
    fn approves(&self) -> bool;
}

impl Drawn {
    /// How far the part that scrolls can be scrolled.
    #[cfg(test)]
    pub(super) fn furthest(&self) -> u16 {
        self.furthest
    }

    /// Whether a yes is taken from this draw.
    #[cfg(test)]
    pub(super) fn answerable(&self) -> bool {
        self.answerable
    }

    /// How many rows a page moves: one fewer than the body shows, so no row is paged past
    /// without having been drawn.
    pub(super) fn page(&self) -> i16 {
        i16::try_from(self.rows.saturating_sub(1).max(1)).unwrap_or(i16::MAX)
    }

    /// How many rows of the body this draw showed.
    #[cfg(test)]
    pub(super) fn rows(&self) -> u16 {
        self.rows
    }

    /// Where to paint the question's picture, where this draw has room for all of it.
    pub(super) fn picture(&self) -> Option<Rect> {
        self.picture
    }

    /// Where the part that scrolls starts once moved `by` rows from `scroll`.
    ///
    /// Moved from where this draw put it rather than from `scroll`, which a terminal made taller
    /// since can leave past the bottom, where a press of Up would move nothing.
    pub(super) fn moved(&self, scroll: u16, by: i16) -> u16 {
        scroll
            .min(self.furthest)
            .saturating_add_signed(by)
            .min(self.furthest)
    }

    /// A key's response, unless it approves and this draw takes no yes.
    pub(super) fn take<R: Approving>(&self, response: Option<R>) -> Option<R> {
        response.filter(|response| self.answerable || !response.approves())
    }

    /// A draw that takes a yes, for a test of what a key means once it is taken.
    #[cfg(test)]
    pub(super) fn taking_yes() -> Self {
        Drawn {
            furthest: u16::MAX,
            rows: 11,
            answerable: true,
            ..Drawn::default()
        }
    }
}

/// Draw a question whose middle can be longer than the box.
///
/// What is pinned claims its rows first and the keys next, and the body takes the rows left
/// between them. A body longer than that scrolls, with a row under it saying how much of it is
/// below, or how many of the rows that decide the question have not been drawn yet.
///
/// A yes is taken where everything pinned and the keys are whole, the body has a row, and every
/// deciding row has been drawn whole in this draw or an earlier one at the same width.
pub(super) fn draw(
    frame: &mut ratatui::Frame,
    inside: Rect,
    question: Question,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let wrapped = |lines: Vec<Line<'static>>| Paragraph::new(lines).wrap(Wrap { trim: false });
    let deciding = question.deciding.min(question.body.len());
    let decided_by = rows_in(&wrapped(question.body[..deciding].to_vec()), inside.width)
        .saturating_add(question.opening);
    let header = wrapped(question.above);
    let body = wrapped(question.body);
    let footer = wrapped(question.below);
    let rest = rows_in(&body, inside.width);
    let decided_by = decided_by.min(rest);
    let laid = Pinned::new(
        inside,
        (
            rows_in(&header, inside.width),
            rest,
            rows_in(&footer, inside.width),
            rows_in(&wrapped((question.keys)(true)), inside.width),
        ),
        scroll,
    );
    let (offset, body_rows) = (laid.offset, laid.body.height);
    // Inside the margin bar and the space after it.
    let painted = question.picture.and_then(|(at, size)| {
        (at >= offset && at - offset + size.height <= body_rows && size.width + 2 <= inside.width)
            .then(|| {
                Rect::new(
                    inside.x + 2,
                    laid.body.y + at - offset,
                    size.width,
                    size.height,
                )
            })
    });
    let unpainted = question
        .picture
        .filter(|_| painted.is_none())
        .map_or(0..0, |(at, size)| at..at.saturating_add(size.height));
    let shown: Range<u16> = if laid.whole {
        offset..offset + body_rows
    } else {
        0..0
    };
    let unseen = seen.mark(
        inside.width,
        decided_by,
        shown.filter(|row| !unpainted.contains(row)),
    );
    let answerable =
        laid.whole && body_rows > 0 && body_rows >= question.needed.min(rest) && unseen == 0;

    let hint = match unseen {
        0 => Line::styled(
            scroll_hint(laid.furthest - offset),
            Style::default().fg(theme::brand_primary()),
        ),
        _ => Line::styled(
            format!("   {}", t!(prompt_unseen, count = usize::from(unseen))),
            Style::default().fg(theme::running()),
        ),
    };
    laid.render(
        frame,
        header,
        body,
        hint,
        footer,
        wrapped((question.keys)(answerable)),
    );

    Drawn {
        furthest: laid.furthest,
        rows: body_rows,
        answerable,
        picture: painted,
    }
}

/// The rows a paragraph takes when wrapped to `width`.
pub(super) fn rows_in(paragraph: &Paragraph, width: u16) -> u16 {
    u16::try_from(paragraph.line_count(width)).unwrap_or(u16::MAX)
}

/// Where a question whose middle can be longer than the box puts each of its parts.
///
/// The rows above and below the middle claim their rows first and the keys next, so keys drawn
/// whole have everything pinned drawn whole with them, and the middle takes the rows left between.
pub(super) struct Pinned {
    header: Rect,
    pub(super) body: Rect,
    /// The row saying how much of the middle is below, where it does not all fit.
    hint: Rect,
    footer: Rect,
    keys: Rect,
    /// The first row of the middle that is on the screen.
    pub(super) offset: u16,
    /// How far the middle can be scrolled.
    pub(super) furthest: u16,
    /// Whether the keys, and with them everything pinned, are whole in a box with a column to draw
    /// in. A box with no column takes no rows for any of it, so the keys count as whole there.
    pub(super) whole: bool,
}

impl Pinned {
    /// Laid out for parts this many rows tall: above the middle, the middle, below it, and the keys.
    pub(super) fn new(
        inside: Rect,
        (heading, rest, footing, answering): (u16, u16, u16, u16),
        scroll: u16,
    ) -> Self {
        let header_rows = heading.min(inside.height);
        let footer_rows = footing.min(inside.height - header_rows);
        let keys_rows = answering.min(inside.height - header_rows - footer_rows);
        let between = inside.height - header_rows - footer_rows - keys_rows;
        // A row saying how much of the middle is below, where that leaves the middle a row of its
        // own. The keys wrap and this does not share their row, so neither cuts off the other.
        let hint_rows = u16::from(rest > between && between > 1);
        // Only the rows the middle fills, so what is pinned below it is drawn right under it.
        let body_rows = rest.min(between - hint_rows);
        let furthest = rest - body_rows;

        let row = |y: u16, height: u16| Rect {
            y,
            height,
            ..inside
        };
        let header = row(inside.y, header_rows);
        let body = row(header.bottom(), body_rows);
        let hint = row(body.bottom(), hint_rows);
        let footer = row(hint.bottom(), footer_rows);
        let keys = row(inside.bottom() - keys_rows, keys_rows);
        Pinned {
            header,
            body,
            hint,
            footer,
            keys,
            offset: scroll.min(furthest),
            furthest,
            whole: inside.width > 0 && keys_rows == answering,
        }
    }

    pub(super) fn render(
        &self,
        frame: &mut ratatui::Frame,
        header: Paragraph,
        body: Paragraph,
        hint: Line,
        footer: Paragraph,
        keys: Paragraph,
    ) {
        frame.render_widget(header, self.header);
        frame.render_widget(body.scroll((self.offset, 0)), self.body);
        frame.render_widget(Paragraph::new(hint), self.hint);
        frame.render_widget(footer, self.footer);
        frame.render_widget(keys, self.keys);
    }
}
