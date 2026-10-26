//! Marking the target the pointer is over, and saying what a click on it does (MOUSE-51).

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

use crate::state::{Footer, HintRow, Hit, Hovered, Laid, Session};

use super::{dim, head_of, tail_of};

/// What a hovered target is drawn with over what it already is: underlined, which is how an address
/// and the word that copies a block are already drawn, and bold, which tells those from the others
/// and which a cell that is already underlined has to gain to show the pointer is on it. Neither is a
/// colour, so it shows under `NO_COLOR` and in every theme.
fn marking() -> Style {
    Style::default().add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
}

/// The target the pointer is over in the frame just drawn into `laid`, where the pointer is hovering.
///
/// Read from this frame's record and not the session's, which holds the one before it, so what is
/// marked is what the frame drew under the pointer.
pub(super) fn hovered_in(session: &Session, laid: &Laid) -> Option<Hovered> {
    if !session.is_hovering() {
        return None;
    }
    let (row, column) = session.pointer_at()?;
    let top = laid.window.map_or(0, |window| window.top);
    laid.hovered(session, top, row, column)
}

/// Mark the target the pointer is over. Nothing else about the frame changes: no cell's text does.
pub(super) fn mark_the_hovered(frame: &mut Frame, session: &Session, laid: &Laid) {
    let Some(hovered) = hovered_in(session, laid) else {
        return;
    };
    let top = laid.window.map_or(0, |window| window.top);
    let areas: Vec<Rect> = match hovered.hit {
        // The track of the scrollbar is a column of blanks round the thumb, which is no text to
        // underline.
        Hit::Scrollbar => Vec::new(),
        // An address or a path that was wrapped is one link over several rows.
        Hit::Link(index) => laid
            .targets
            .iter()
            .filter(|target| target.hit == Hit::Link(index))
            .map(|target| laid.on_the_screen(target, top))
            .collect(),
        _ => vec![hovered.area],
    };
    for area in areas {
        mark(frame.buffer_mut(), area);
    }
}

/// Mark the words on the first row of `area`, leaving the blanks before and after them as they were,
/// since a target is as wide as its row and an underline across the blank of a row is no mark of what
/// is there. A target of several rows, a call or a block of a delegate, is marked on its first.
fn mark(buffer: &mut Buffer, area: Rect) {
    let row = area.y;
    let drawn = |column: u16| {
        buffer
            .cell((column, row))
            .is_some_and(|cell| !cell.symbol().trim().is_empty())
    };
    let columns = area.x..area.right();
    let (Some(first), Some(last)) = (
        columns.clone().find(|column| drawn(*column)),
        columns.rev().find(|column| drawn(*column)),
    ) else {
        return;
    };
    for column in first..=last {
        if let Some(cell) = buffer.cell_mut((column, row)) {
            cell.set_style(marking());
        }
    }
}

/// What the pointer over a target says: what a click on it does, and for a link where it goes.
struct Says {
    /// What a click does, the interface's own sentence.
    what: String,
    /// The address or the path a click on a link acts on, as the cells show it, and whether it is cut
    /// from the left, keeping the end that tells one path from another, or from the right.
    detail: Option<(String, bool)>,
}

/// What a click on `hit` does, in the state the session is in, as the sentence the catalog holds.
/// `None` for the targets that say nothing of a click (see [`Hit::is_hoverable`]).
///
/// What decides is the kind of target and the state, never the words under it: the address or the
/// path of a link is the cells' own text, which is the only thing a click could open.
fn says(session: &Session, laid: &Laid, hit: Hit) -> Option<Says> {
    use crate::panel::Target;
    use crate::text_links::Target as Link;
    let said = |what: &str| {
        Some(Says {
            what: what.to_string(),
            detail: None,
        })
    };
    match hit {
        Hit::Back => said(t!(hover_back)),
        Hit::Reply(_) => said(t!(hover_reply)),
        Hit::Block(..) => said(t!(hover_block)),
        Hit::Queued(_) => said(t!(hover_queued)),
        Hit::Call(_) if session.scrolling() => said(t!(hover_call_toggle)),
        Hit::Call(_) => said(t!(hover_call_open)),
        Hit::Delegate(_) => said(t!(hover_delegate)),
        Hit::Link(index) => match laid.links.get(index)? {
            Link::Address(address) => Some(Says {
                what: t!(hover_address).to_string(),
                detail: Some((address.clone(), false)),
            }),
            Link::Path(path) => {
                let drawn = match (path.line, path.column) {
                    (Some(line), Some(column)) => format!("{}:{line}:{column}", path.path),
                    (Some(line), None) => format!("{}:{line}", path.path),
                    _ => path.path.clone(),
                };
                // The scroller is not where a file is shown, since it is added at the end of the
                // transcript, so a click there is the editor's alone.
                let what = if session.scrolling() {
                    t!(hover_path_edit)
                } else {
                    t!(hover_path)
                };
                Some(Says {
                    what: what.to_string(),
                    detail: Some((drawn, true)),
                })
            }
        },
        Hit::Panel(Target::Model) => said(t!(hover_panel_model)),
        Hit::Panel(Target::Effort) => said(t!(hover_panel_effort)),
        Hit::Panel(Target::Context) => said(t!(hover_panel_context)),
        Hit::Panel(Target::Servers) => said(t!(hover_panel_servers)),
        Hit::Panel(Target::Languages) => said(t!(hover_panel_languages)),
        Hit::Panel(Target::Hide) => said(t!(hover_panel_hide)),
        Hit::Panel(Target::Files) => said(t!(hover_panel_files)),
        Hit::Panel(Target::File(_)) => said(t!(hover_panel_file)),
        Hit::Panel(Target::CloseFiles) => said(t!(hover_panel_close_files)),
        Hit::Panel(Target::PreviousEdit) => said(t!(hover_panel_edit_previous)),
        Hit::Panel(Target::NextEdit) => said(t!(hover_panel_edit_next)),
        Hit::Panel(Target::Name) => said(t!(hover_panel_name)),
        Hit::Panel(Target::Task(_)) => said(t!(hover_panel_task)),
        Hit::Panel(Target::Detail(_)) => said(t!(hover_panel_detail)),
        // The address that opens, which is the one that was set and not the shorthand the row draws
        // for it.
        Hit::Panel(link @ (Target::PullRequest | Target::Issue)) => Some(Says {
            what: t!(hover_panel_link).to_string(),
            detail: session
                .panel_link(link)
                .map(|address| (address.to_string(), false)),
        }),
        Hit::Footer(Footer::Mode) => said(t!(hover_footer_mode)),
        Hit::Footer(Footer::Trail) => said(t!(hover_footer_trail)),
        Hit::Footer(Footer::Shortcuts) => said(t!(hover_footer_shortcuts)),
        Hit::Footer(Footer::Info) => said(t!(hover_footer_info)),
        Hit::Footer(Footer::Watch) => said(t!(hover_footer_watch)),
        Hit::Footer(Footer::Outputs) => said(t!(hover_footer_outputs)),
        Hit::Footer(Footer::Background) => said(t!(hover_footer_background)),
        Hit::Footer(Footer::Held) => said(t!(hover_footer_held)),
        Hit::Footer(Footer::CancelShell) => said(t!(hover_footer_cancel_shell)),
        Hit::Footer(Footer::Paste) => said(t!(hover_footer_paste)),
        Hit::Item(_) => said(t!(hover_item)),
        Hit::Scrollbar => said(t!(hover_scrollbar)),
        // The text of the box and of a reply, the panels that hold the input and the panes the
        // wheel scrolls have no click of their own to say.
        Hit::Text { .. }
        | Hit::Words(_)
        | Hit::Wheel(_)
        | Hit::Popup(_)
        | Hit::PopupRows(_)
        | Hit::Match(_) => None,
    }
}

/// The line that says what a click on the target under the pointer does, in `room` columns: the
/// sentence whole, then for a link the address or path cut to what is left. Where the room holds the
/// sentence and too little of the address beside it, the line is the address or path, cut, and not
/// the sentence: where a click goes is what the person cannot read anywhere else, since the cells
/// show a link only as far as its row of the transcript held it. `None` where the sentence does not
/// fit and there is no link to say, or where not even a few characters of one fit, since half a
/// sentence reads as a rendering fault.
fn fitted(says: &Says, room: usize) -> Option<String> {
    use crate::wrap::display_width;
    const SEPARATOR: &str = "  \u{b7}  ";
    // The fewest characters of an address or a path that say anything about it.
    const LEAST: usize = 4;
    let fits = display_width(&says.what) <= room;
    let Some((detail, keep_the_end)) = &says.detail else {
        return fits.then(|| says.what.clone());
    };
    let cut = |room: usize| {
        if *keep_the_end {
            tail_of(detail, room)
        } else {
            head_of(detail, room)
        }
    };
    let left = room.saturating_sub(display_width(&says.what) + display_width(SEPARATOR));
    if fits && left >= LEAST {
        return Some(format!("{}{SEPARATOR}{}", says.what, cut(left)));
    }
    (room >= LEAST).then(|| cut(room))
}

/// Say what a click on the target under the pointer does, on the row of hints, in the room after the
/// last cell the row has drawn. The hints are where they were and none is covered, since the row was
/// drawn whole before this, and a note drawn at the right of the row, or a message that fills it,
/// leaves no room, so the line gives way to it (MOUSE-51).
pub(super) fn say_what_a_click_does(frame: &mut Frame, session: &Session, laid: &Laid) {
    let Some(hovered) = hovered_in(session, laid) else {
        return;
    };
    let Some(HintRow { area, taken: false }) = laid.hint_row else {
        return;
    };
    let Some(says) = says(session, laid, hovered.hit) else {
        return;
    };
    let drawn_to = (area.x..area.right()).rev().find(|column| {
        frame
            .buffer_mut()
            .cell((*column, area.y))
            .is_some_and(|cell| !cell.symbol().trim().is_empty())
    });
    // Two blanks after what the row says, and the same indent as the row's own where it says nothing.
    let from = drawn_to
        .map_or(area.x, |column| column + 1)
        .saturating_add(2);
    let room = usize::from(area.right().saturating_sub(from));
    let Some(line) = fitted(&says, room) else {
        return;
    };
    let free = Rect::new(from, area.y, area.right().saturating_sub(from), 1);
    frame.render_widget(
        Paragraph::new(Span::styled(line, dim())).alignment(Alignment::Right),
        free,
    );
}
