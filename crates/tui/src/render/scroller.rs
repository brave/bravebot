//! The scroller: the transcript paged through by itself, with its help and its hint row.

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthChar;

use crate::keybindings::Keybindings;
use crate::state::{Laid, Session};
use crate::theme;

use super::dim;
use super::transcript::draw_transcript;

/// Draw the scroller: the transcript, and one row saying what the keys are.
///
/// The box goes, and so does the indicator above it and anything being offered beneath it. Every
/// one of them is a thing the person is being invited to type at, and while the scroller is open
/// no key reaches any of them: a box drawn under a mode that cannot reach it is three rows of the
/// screen spent inviting a keystroke that would do nothing. What they cost is given to the
/// transcript, which is the whole of what somebody opened a pager to look at.
///
/// The one row kept is the footer, because a mode where the letters do nothing with nothing on
/// the screen to say why is indistinguishable from an interface that has stopped responding.
pub(super) fn draw_scroller(frame: &mut Frame, session: &Session) -> Laid {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // transcript
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    let laid = draw_transcript(frame, areas[0], session);
    draw_scroller_hint(frame, areas[1], session, laid.matches.len());

    // Over the transcript rather than beside it, because a key list is read instead of the
    // transcript and never at the same time.
    if session.scroller().is_some_and(|scroller| scroller.help) {
        draw_scroller_help(frame, frame.area(), session);
    }

    if let Some(selection) = &session.selection {
        crate::select::highlight(frame.buffer_mut(), selection);
    }

    laid
}

/// The keys the scroller answers, in the order somebody reaches for them.
///
/// The way out is last and is never the row that did not fit: a list that scrolled its own exit
/// off the screen would be a mode nobody could leave.
fn scroller_keys() -> [(&'static str, &'static str); 14] {
    [
        // vi's two pairs of line chords take rows of their own: the key column of the first row
        // is full, and a parenthesis on its meaning would not hold them either.
        ("up/down, j/k, y/e", t!(scroller_key_line)),
        ("ctrl-y / ctrl-e", t!(scroller_key_line)),
        ("ctrl-p / ctrl-n", t!(scroller_key_line)),
        ("ctrl-u / ctrl-d", t!(scroller_key_half_page)),
        ("space, f / b", t!(scroller_key_full_page)),
        ("g / G, < / >", t!(scroller_key_ends)),
        ("{ / }", t!(scroller_key_prompts)),
        ("/ then n/N", t!(scroller_key_search)),
        ("5j, 3}, 2n", t!(scroller_key_count)),
        // The keys the search itself answers get a row of their own rather than a parenthesis on
        // the row above: the second spellings carried that way are the same key by another name,
        // and these two are neither, so a list that folded them in would be naming keys it had
        // not told anybody about.
        ("enter / backspace", t!(scroller_key_search_run)),
        ("x", t!(scroller_key_expand)),
        ("v", t!(scroller_key_editor)),
        ("?", t!(scroller_key_this_list)),
        ("any key", t!(scroller_key_close_list)),
    ]
}

/// What closes the scroller, which is the one row of the key list that is never dropped.
///
/// Ctrl-C is named by the meaning rather than here. It closes the scroller wherever the chord that
/// opened it has been moved to, and naming it in both columns had the row listing it twice.
fn scroller_exit(bindings: &Keybindings) -> (String, &'static str) {
    (
        format!("q / esc / {}", bindings.scroller_name()),
        t!(scroller_key_close),
    )
}

/// Draw the key list over the transcript.
///
/// Short terminals lose rows from the middle of the list rather than the end of it. Every row here
/// is a convenience except the last, and the last is the only one somebody is stuck without.
fn draw_scroller_help(frame: &mut Frame, area: Rect, session: &Session) {
    let row = |(key, what): (&str, &str)| {
        Line::from(vec![
            Span::styled(
                format!(" {key:<18}"),
                Style::default().fg(theme::brand_primary()),
            ),
            Span::styled(what.to_string(), dim()),
        ])
    };

    // A border costs two rows, and on a terminal with three there is no version of this worth
    // having a frame around: the way out is what the list is for, and it goes on the screen with
    // or without one.
    let bordered = area.height >= 4;
    let room = if bordered {
        (area.height as usize).saturating_sub(2)
    } else {
        area.height as usize
    }
    .max(1);

    let mut rows: Vec<Line> = scroller_keys()
        .into_iter()
        .take(room.saturating_sub(1))
        .map(row)
        .collect();
    let (exit_chord, exit_desc) = scroller_exit(session.bindings());
    rows.push(row((&exit_chord, exit_desc)));

    let width = area.width.min(58);
    let height = (rows.len() as u16 + if bordered { 2 } else { 0 }).min(area.height);
    let box_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    frame.render_widget(ratatui::widgets::Clear, box_area);
    let list = Paragraph::new(rows);
    let list = if bordered {
        list.block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(" {} ", t!(scroller_title))),
        )
    } else {
        list
    };
    frame.render_widget(list, box_area);
}

/// The tail of `needle` that fits in `room` columns, with an ellipsis where the head went.
///
/// The tail because the end is where somebody typing is looking. Whatever the row cannot hold of a
/// needle is cut here, before it is drawn, because a span too wide for the row is cut by the row at
/// its right edge, and what is there is the way out.
pub(super) fn needle_that_fits(needle: &str, room: usize) -> String {
    let width = |text: &str| text.chars().filter_map(|c| c.width()).sum::<usize>();
    if width(needle) <= room {
        return needle.to_string();
    }
    let mut kept: Vec<char> = Vec::new();
    let mut used = 1;
    for c in needle.chars().rev() {
        let next = used + c.width().unwrap_or(0);
        if next > room {
            break;
        }
        used = next;
        kept.push(c);
    }
    if room == 0 {
        return String::new();
    }
    kept.push('…');
    kept.reverse();
    kept.into_iter().collect()
}

/// Put what a turn underneath is saying on the row, keeping `reserved` columns for what follows.
///
/// What follows is the way out, and reserving it is how the two rules meeting on this row resolve
/// when it cannot hold both: a turn in flight is said nowhere else on the screen, and a mode whose
/// keys do nothing with nothing on the screen to say how to leave it is what an interface that has
/// stopped responding looks like. Gives back the columns still free, which is what decides whether
/// a longer list of keys can have them.
fn say_the_turn_if_it_fits(
    spans: &mut Vec<Span<'static>>,
    width: u16,
    reserved: usize,
    saying: [Option<Span<'static>>; 2],
) -> usize {
    let said: usize = spans.iter().map(|span| span.width()).sum();
    let mut left = (width as usize).saturating_sub(said + reserved);
    for span in saying.into_iter().flatten() {
        if span.width() <= left {
            left -= span.width();
            spans.push(span);
        }
    }
    left
}

/// The one row under the transcript while the scroller is open.
///
/// What it says depends on what the scroller is doing, and a turn going on underneath is said in
/// all three of its states: a search standing on the row does not stop the turn running below it,
/// and this row is the only place either can be said from. A needle long enough to crowd the row
/// is paid for by what the turn is saying, never by the way out.
fn draw_scroller_hint(frame: &mut Frame, area: Rect, session: &Session, found: usize) {
    let scroller = match session.scroller() {
        Some(scroller) => scroller,
        None => return,
    };

    // The indicator's row went with the box, and a turn that has not written anything yet leaves
    // nothing else on the screen moving. Somebody reading back through a turn in flight has to be
    // able to tell it is still in flight, so the footer says so in the indicator's own word.
    let running = session.indicator().map(|indicator| {
        Span::styled(
            format!("  ·  {}…", indicator.verb),
            Style::default().fg(theme::running()),
        )
    });

    // The view is held, so what arrives goes below it, and how much is a thing the person has no
    // other way to find out without giving up the place they are reading.
    let below = session.rows_below();
    let arrived = (below > 0).then(|| {
        Span::styled(
            format!("  ·  {}", t!(scroller_rows_below, count = below)),
            dim(),
        )
    });

    // A search being typed owns the line: what somebody is typing is the thing they are looking
    // at, and a caret says the keys are going here rather than into the box.
    if let Some(typing) = &scroller.typing {
        // Escape is the way out of a search, and the hint naming it is what the row holds on to
        // while the needle grows, so the needle is what is cut to leave it room.
        let searching = Span::styled(format!("  ·  {}", t!(scroller_searching)), dim());
        let room = (area.width as usize).saturating_sub(searching.width() + "  /".len() + 1);
        let mut spans = vec![
            Span::styled(
                format!("  /{}", needle_that_fits(typing, room)),
                Style::default().fg(theme::brand_primary()),
            ),
            Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)),
        ];
        say_the_turn_if_it_fits(
            &mut spans,
            area.width,
            searching.width(),
            [running, arrived],
        );
        spans.push(searching);

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }

    // A count decides how far the next key goes, so it is drawn until that key takes it, and first
    // on the row: after a needle it would read as part of what is being looked for. Drawn first,
    // a long one would push the way out off a narrow row, so it is drawn only where the `kept`
    // columns after it still fit.
    let counted = |kept: usize| {
        scroller
            .count
            .map(|count| {
                Span::styled(
                    format!("  {count}"),
                    Style::default()
                        .fg(theme::brand_primary())
                        .add_modifier(Modifier::BOLD),
                )
            })
            .filter(|counted| counted.width() + kept <= area.width as usize)
    };

    if !scroller.needle.is_empty() {
        // Never the matched text. The footer is the one row of the screen the interface speaks in
        // its own voice, and a quotation there is untrusted content drawn outside a marked block.
        let standing = if found == 0 {
            t!(scroller_no_matches).to_string()
        } else {
            t!(
                scroller_match_of,
                at = scroller.at.min(found - 1) + 1,
                total = found
            )
        };
        let standing = Span::styled(format!("  ·  {standing}"), dim());

        // The keys are what gives up the room, because every one of them is behind `?` as well
        // while a turn underneath is said nowhere but here. What the row keeps before anything
        // else is the pair naming the key that closes the mode, so a needle wide enough to crowd
        // the row costs the longer list first and then what the turn is saying.
        let walking = Span::styled(format!("  ·  {}", t!(scroller_search_keys)), dim());
        let closing = Span::styled(format!("  ·  {}", t!(scroller_footer_keys)), dim());

        // The needle is whatever somebody typed, so it is cut to the columns the way out leaves,
        // and the match count goes before any of the needle does.
        let room = (area.width as usize).saturating_sub(closing.width() + "  /".len());
        let shown = needle_that_fits(&scroller.needle, room);
        let shown_width: usize = shown.chars().filter_map(|c| c.width()).sum();
        let looking: Vec<Span> = std::iter::once(Span::styled(
            format!("  /{shown}"),
            Style::default().fg(theme::brand_primary()),
        ))
        .chain((shown_width + standing.width() <= room).then_some(standing))
        .collect();
        let kept = looking.iter().map(Span::width).sum::<usize>() + closing.width();
        let mut spans: Vec<Span> = counted(kept).into_iter().collect();
        spans.extend(looking);
        let left =
            say_the_turn_if_it_fits(&mut spans, area.width, closing.width(), [running, arrived]);
        spans.push(if walking.width() <= closing.width() + left {
            walking
        } else {
            closing
        });

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }

    // Four things and no more, because every key is behind `?` and a row long enough to be cut in
    // half advertises whatever happened to be at the near end of it. `/` is here because it is
    // the one key nobody guesses, and everything else on the row is a way to find out the rest.
    let way_out = [
        Span::styled(
            format!("  {}", t!(scroller_footer)),
            Style::default().fg(theme::brand_primary()),
        ),
        Span::styled(format!("  ·  {}", t!(scroller_footer_keys)), dim()),
    ];
    let mut spans: Vec<Span> = counted(way_out.iter().map(Span::width).sum())
        .into_iter()
        .collect();
    spans.extend(way_out);
    spans.extend(running);
    spans.extend(arrived);
    spans.push(Span::styled(
        format!("  ·  {}", t!(scroller_footer_search)),
        dim(),
    ));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
