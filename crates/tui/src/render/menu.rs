//! The small menu a right click opens (MOUSE-52).

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::state::{Choice, Menu, Panel, Popup};
use crate::theme;

/// What an item says, in the catalog's words.
fn label(choice: Choice) -> String {
    match choice {
        Choice::Copy(_) | Choice::CopySelection => t!(menu_copy),
        Choice::CopyAsText(_) => t!(menu_copy_text),
        Choice::Branch => t!(menu_branch),
        Choice::OpenDelegate(_) => t!(menu_open_delegate),
    }
    .to_string()
}

/// Where the menu is drawn in a frame of `screen`: its corner at the pointer where it has room, and
/// moved up and left to keep every cell of it on the screen where it has not. Never larger than the
/// screen, and never wrapped: a label longer than the room is cut by the edge of the panel.
fn placed(screen: Rect, menu: &Menu, labels: &[String]) -> Rect {
    // A border on each side, the cursor's mark and its blank, and a blank after the longest label.
    let widest = labels
        .iter()
        .map(|label| crate::wrap::display_width(label))
        .max()
        .unwrap_or(0);
    let width = u16::try_from(widest + 5)
        .unwrap_or(u16::MAX)
        .min(screen.width);
    let height = u16::try_from(labels.len() + 2)
        .unwrap_or(u16::MAX)
        .min(screen.height);
    let (row, column) = menu.at();
    Rect::new(
        column
            .min(screen.right().saturating_sub(width))
            .max(screen.x),
        row.min(screen.bottom().saturating_sub(height))
            .max(screen.y),
        width,
        height,
    )
}

/// Draw the menu over the frame, and give back the cells it covers and those of its items, which are
/// recorded so that a press on one is the menu's and reaches nothing beneath (MOUSE-45).
pub(super) fn draw_menu(frame: &mut Frame, menu: &Menu) -> Popup {
    let labels: Vec<String> = menu.choices().iter().map(|choice| label(*choice)).collect();
    let area = placed(frame.area(), menu, &labels);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::brand_primary()))
        .style(Style::default().bg(theme::background()).fg(theme::text()));
    let inside = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = labels
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let chosen = index == menu.cursor();
            let (marker, name) = match chosen {
                true => (
                    "\u{276f} ",
                    Style::default()
                        .fg(theme::brand_primary())
                        .add_modifier(Modifier::BOLD),
                ),
                false => ("  ", Style::default().fg(theme::text())),
            };
            Line::from(vec![
                Span::styled(marker, Style::default().fg(theme::brand_primary())),
                Span::styled(label.clone(), name),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inside);

    Popup {
        panel: Panel::Menu,
        area,
        list: Some(Rect::new(
            inside.x,
            inside.y,
            inside.width,
            u16::try_from(labels.len())
                .unwrap_or(u16::MAX)
                .min(inside.height),
        )),
    }
}
