//! The Sessions screen's screen: a header with the grouping and the tabs, the grouped list
//! of tasks with a column for each of the status and the age, the details of the task under the
//! cursor where the screen is wide, and a footer of keys (SCREEN-1).
//!
//! Everything drawn is a name the person typed, a fact about a process or a file, or a word from
//! the catalog. The strings a row holds were pictured when the row was made, so none of them is
//! drawn raw here (SCREEN-2).

use bravebot_i18n::t;
use bravebot_session::jobs::{Held, Mode};
use bravebot_session::listing::{Grouping, Status};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::sessions_keys::{Action, SessionsKeys};
use crate::sessions_screen::{
    Click, Confirm, Destructive, Effect, Entry, Row, SessionsScreen, TABS,
};
use crate::state::{Hit, Laid, Session, Target};
use crate::theme;

use super::{dim, head_of};

/// The columns before a row's name: the cursor, a space, the dot and a space.
const GUTTER: u16 = 4;

/// The widest status word, so the column after it starts in one place on every row.
const STATUS_WIDTH: u16 = 14;

/// The widest age.
const AGE_WIDTH: u16 = 9;

/// The width of the list at which the status and the age are drawn.
const METADATA_FROM: u16 = 62;

/// The width of the project column of the list that is not grouped, and of the list at which it is
/// drawn: wide enough that the names keep room beside it.
const PROJECT_WIDTH: u16 = 18;
const PROJECT_FROM: u16 = 74;

/// The width of the branch and size columns, and of the list at which they are drawn. A narrower
/// list gives them to the details beside it, or to the line under it (SCREEN-4).
const BRANCH_WIDTH: u16 = 16;
const SIZE_WIDTH: u16 = 7;
const WIDE_FROM: u16 = 100;

/// The width of the body at which the details of the task under the cursor stand beside the list.
const DETAILS_FROM: u16 = 90;

/// The height of the list at which the search box above it is drawn.
const SEARCH_FROM_HEIGHT: u16 = 4;

/// The height of the list at which the line under it is drawn.
const STRIP_FROM_HEIGHT: u16 = 8;

const LIST_LEAST: u16 = 46;
const DETAILS_WIDTH: u16 = 38;

/// The width of the screen at which the tabs are joined by the key that moves between them.
const FILTER_HINT_FROM: u16 = 80;

/// The parts of the screen.
struct Zones {
    header: Rect,
    search: Rect,
    list: Rect,
    /// The line under the list that says the branch, size and id of the session under the cursor
    /// where the details are not beside it.
    strip: Rect,
    gap: Rect,
    details: Rect,
    footer: Rect,
}

fn zones(area: Rect) -> Zones {
    let footer_height = u16::from(area.height >= 2);
    let header_height = if area.height >= 6 { 3 } else { 0 };
    let footer = Rect::new(
        area.x,
        area.bottom().saturating_sub(footer_height),
        area.width,
        footer_height,
    );
    let header = Rect::new(area.x, area.y, area.width, header_height);
    let body = Rect::new(
        area.x,
        header.bottom(),
        area.width,
        footer.y.saturating_sub(header.bottom()),
    )
    .inner(Margin::new(2, 0));
    let (list, gap, details) = if body.width >= DETAILS_FROM {
        let [list, gap, details] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(LIST_LEAST),
                Constraint::Length(3),
                Constraint::Length(DETAILS_WIDTH),
            ])
            .areas(body);
        (list, gap, details)
    } else {
        (body, Rect::default(), Rect::default())
    };
    let search = Rect::new(
        list.x,
        list.y,
        list.width,
        u16::from(list.height >= SEARCH_FROM_HEIGHT),
    );
    let list = Rect::new(
        list.x,
        list.y + search.height,
        list.width,
        list.height - search.height,
    );
    let (list, strip) = if details.is_empty() && list.height >= STRIP_FROM_HEIGHT {
        (
            Rect::new(list.x, list.y, list.width, list.height - 1),
            Rect::new(list.x, list.bottom() - 1, list.width, 1),
        )
    } else {
        (list, Rect::default())
    };
    Zones {
        header,
        search,
        list,
        strip,
        gap,
        details,
        footer,
    }
}

/// Draw the Sessions screen, in place of the transcript and the box.
pub(super) fn draw_sessions_screen(frame: &mut Frame, session: &Session) -> Laid {
    let mut laid = Laid::default();
    let Some(sessions_screen) = session.sessions_screen() else {
        return laid;
    };
    let area = frame.area();
    let zones = zones(area);

    draw_header(frame, &zones, sessions_screen, &mut laid);
    draw_editor(frame, zones.search, sessions_screen);

    if sessions_screen.helping() {
        draw_help(
            frame,
            sessions_screen.keys(),
            Rect::new(
                area.x,
                zones.header.bottom(),
                area.width,
                zones.footer.y.saturating_sub(zones.header.bottom()),
            )
            .inner(Margin::new(2, 0)),
        );
    } else {
        for y in zones.gap.y..zones.gap.bottom() {
            if zones.gap.width > 1 {
                frame.render_widget(
                    Paragraph::new(Span::styled("│", dim())),
                    Rect::new(zones.gap.x + 1, y, 1, 1),
                );
            }
        }
        draw_list(frame, zones.list, sessions_screen, &mut laid);
        if !zones.strip.is_empty() {
            draw_strip(frame, zones.strip, sessions_screen);
        }
        if !zones.details.is_empty() {
            draw_details(frame, zones.details, sessions_screen);
        }
    }
    draw_footer(frame, zones.footer, sessions_screen);
    if let Some(confirm) = sessions_screen.confirming() {
        draw_confirm(frame, area, confirm);
    }
    laid
}

pub(crate) fn status_word(status: Status) -> &'static str {
    match status {
        Status::NeedsInput => t!(sessions_screen_status_needs_input),
        Status::Interrupted => t!(sessions_screen_status_interrupted),
        Status::Working => t!(sessions_screen_status_working),
        Status::Ready => t!(sessions_screen_status_ready),
        Status::OpenElsewhere => t!(sessions_screen_status_open_elsewhere),
        Status::Stopped => t!(sessions_screen_status_stopped),
        Status::Saved => t!(sessions_screen_status_saved),
    }
}

/// A status as a row says it: its word, and for a session holding a prompt the one word for the kind
/// of prompt held, in brackets (BG-5, SCREEN-16).
pub(crate) fn status_in_words(status: Status, held: Option<Held>) -> String {
    match (status, held) {
        (Status::NeedsInput, Some(held)) => {
            format!("{} ({})", status_word(status), held_word(held))
        }
        _ => status_word(status).to_string(),
    }
}

/// The dot before a row and the colour it is drawn in.
fn status_dot(status: Status) -> (&'static str, Color) {
    match status {
        Status::NeedsInput => ("●", theme::fail()),
        Status::Interrupted => ("!", theme::fail()),
        Status::Working => ("●", theme::ok()),
        Status::Ready | Status::OpenElsewhere => ("○", theme::accent()),
        Status::Stopped | Status::Saved => ("○", theme::muted()),
    }
}

fn tab_word(tab: usize) -> &'static str {
    match tab {
        0 => t!(sessions_screen_tab_all),
        1 => t!(sessions_screen_tab_needs_you),
        2 => t!(sessions_screen_tab_working),
        3 => t!(sessions_screen_tab_ready),
        _ => t!(sessions_screen_tab_inactive),
    }
}

fn held_word(held: Held) -> &'static str {
    match held {
        Held::Write => t!(sessions_held_write),
        Held::Run => t!(sessions_held_run),
        Held::Read => t!(sessions_held_read),
        Held::Fetch => t!(sessions_held_fetch),
        Held::Server => t!(sessions_held_server),
        Held::Vouch => t!(sessions_held_vouch),
        Held::Tools => t!(sessions_held_tools),
        Held::Move => t!(sessions_held_move),
        Held::Manifest => t!(sessions_held_manifest),
        Held::Question => t!(sessions_held_question),
        Held::Message => t!(sessions_held_message),
    }
}

fn mode_word(mode: Mode) -> &'static str {
    match mode {
        Mode::Ask => t!(mode_ask),
        Mode::AcceptEdits => t!(mode_accept_edits),
        Mode::Plan => t!(mode_plan),
    }
}

/// The key that moves between the tabs, as the hint beside them names it.
fn filter_keys() -> &'static str {
    t!(sessions_screen_filter_keys)
}

fn draw_header(
    frame: &mut Frame,
    zones: &Zones,
    sessions_screen: &SessionsScreen,
    laid: &mut Laid,
) {
    let header = zones.header;
    if header.is_empty() {
        return;
    }
    let inset =
        |row: u16| Rect::new(header.x, header.y + row, header.width, 1).inner(Margin::new(2, 0));
    let title = inset(0);
    let waiting = sessions_screen.count(1);
    let working = sessions_screen.count(2);
    let mut left = vec![Span::styled(
        t!(sessions_screen_title),
        Style::default()
            .fg(theme::brand_primary())
            .add_modifier(Modifier::BOLD),
    )];
    left.push(Span::styled("  ", dim()));
    if waiting + working == 0 {
        left.push(Span::styled(t!(sessions_screen_summary_calm), dim()));
    } else {
        if waiting > 0 {
            left.push(Span::styled(
                t!(sessions_screen_summary_waiting, count = waiting),
                Style::default().fg(theme::fail()),
            ));
        }
        if waiting > 0 && working > 0 {
            left.push(Span::styled("  ", dim()));
        }
        if working > 0 {
            left.push(Span::styled(
                t!(sessions_screen_summary_working, count = working),
                Style::default().fg(theme::ok()),
            ));
        }
    }
    // The pull request the list is narrowed to, as typed on the command line.
    if let Some(wanted) = sessions_screen.pull_request() {
        left.push(Span::styled(
            format!(
                "  ·  {}",
                t!(
                    sessions_screen_for_pull_request,
                    pull_request = super::printable(wanted)
                )
            ),
            dim(),
        ));
    }
    // What the key will do, and not what the list is in now: the second is on the screen already.
    let group_key = sessions_screen.keys().name(Action::Group);
    let layout = match sessions_screen.grouping() {
        Grouping::Together => t!(sessions_screen_group_by_project, key = group_key),
        Grouping::Project => t!(sessions_screen_group_by_agent, key = group_key),
        Grouping::Agent => t!(sessions_screen_group_by_model, key = group_key),
        Grouping::Model => t!(sessions_screen_group_together, key = group_key),
    };
    let width = u16::try_from(layout.width()).unwrap_or(u16::MAX);
    if title.width > width + 40 && !sessions_screen.editing() {
        frame.render_widget(
            Paragraph::new(Span::styled(layout, dim())),
            Rect::new(title.right() - width, title.y, width, 1),
        );
    }
    frame.render_widget(Paragraph::new(Line::from(left)), title);

    let mut tabs = inset(1);
    if tabs.width >= FILTER_HINT_FROM && !sessions_screen.editing() {
        let hint = format!("{} {}", filter_keys(), t!(sessions_screen_filter_word));
        let width = u16::try_from(hint.width()).unwrap_or(u16::MAX);
        frame.render_widget(
            Paragraph::new(Span::styled(hint, dim())),
            Rect::new(tabs.right() - width, tabs.y, width, 1),
        );
        tabs.width = tabs.width.saturating_sub(width + 2);
    }
    let mut x = tabs.x;
    for tab in 0..TABS.len() {
        let label = format!(" {} {} ", tab_word(tab), sessions_screen.count(tab));
        let width = u16::try_from(label.width()).unwrap_or(u16::MAX);
        if x + width > tabs.right() {
            break;
        }
        let chosen = tab == sessions_screen.tab();
        let style = match chosen {
            true => theme::picked_out().fg(theme::on_primary()),
            // What needs a person is the one count drawn in a colour of its own.
            false if tab == 1 && sessions_screen.count(tab) > 0 => {
                Style::default().fg(theme::fail())
            }
            false => dim(),
        };
        let at = Rect::new(x, tabs.y, width, 1);
        frame.render_widget(Paragraph::new(Span::styled(label, style)), at);
        laid.targets
            .push(Target::on_screen(at, Hit::SessionsScreen(Click::Tab(tab))));
        x += width + 1;
    }

    frame.render_widget(
        Paragraph::new(Span::styled(
            "─".repeat(usize::from(header.width.saturating_sub(4))),
            dim(),
        )),
        inset(2),
    );
}

/// The line being typed above the list, with the caret where the next character lands.
fn draw_editor(frame: &mut Frame, area: Rect, sessions_screen: &SessionsScreen) {
    if area.is_empty() {
        return;
    }
    // The box is there whether or not it has the focus. Unfocused it is dim and holds a hint of what
    // it searches when empty; focused it is filled and the caret stands in it.
    let focused = sessions_screen.editing();
    if focused {
        frame.render_widget(
            Block::default().style(theme::picked_out().fg(theme::on_primary())),
            area,
        );
    } else {
        let label = format!("{} ", t!(sessions_screen_search_label));
        let text = match sessions_screen.search() {
            "" => t!(sessions_screen_search_placeholder).to_string(),
            typed => typed.to_string(),
        };
        let room = usize::from(area.width).saturating_sub(label.width() + 3);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(label, dim()),
                Span::styled(
                    head_of(&text, room),
                    match sessions_screen.search().is_empty() {
                        true => dim(),
                        false => Style::default().fg(theme::text()),
                    },
                ),
            ])),
            area.inner(Margin::new(2, 0)),
        );
        return;
    }
    let (label, text) = match sessions_screen.renaming() {
        Some(renaming) => (
            format!("{} ", t!(sessions_screen_rename_label)),
            renaming.text.as_str(),
        ),
        None => (
            format!("{} ", t!(sessions_screen_search_label)),
            sessions_screen.search(),
        ),
    };
    let room = usize::from(area.width).saturating_sub(label.width() + 1);
    // The end of the line, which is where the next character lands: what is kept is the tail.
    let mut kept = String::new();
    let mut used = 0;
    for letter in text.chars().rev() {
        let width = UnicodeWidthStr::width(letter.to_string().as_str());
        if used + width > room {
            break;
        }
        kept.insert(0, letter);
        used += width;
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                label.clone(),
                Style::default()
                    .fg(theme::on_primary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(kept),
        ]))
        .style(theme::picked_out().fg(theme::on_primary())),
        area,
    );
    frame.set_cursor_position(Position::new(
        area.x
            + u16::try_from((label.width() + used).min(usize::from(area.width.saturating_sub(1))))
                .unwrap_or(0),
        area.y,
    ));
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass over the entries, whose arms each draw one kind of line"
)]
fn draw_list(frame: &mut Frame, area: Rect, sessions_screen: &SessionsScreen, laid: &mut Laid) {
    // The last column is left for the row's fill to end beyond the scroll marks.
    let width = area.width.saturating_sub(1);
    if width == 0 || area.is_empty() {
        return;
    }
    let area = Rect { width, ..area };
    let entries = sessions_screen.entries();
    if entries.is_empty() {
        let words = if !sessions_screen.has_read() {
            t!(sessions_screen_empty_loading)
        } else if sessions_screen.rows().is_empty() && sessions_screen.pull_request().is_none() {
            t!(sessions_screen_empty_none)
        } else {
            t!(sessions_screen_empty_no_match)
        };
        frame.render_widget(Paragraph::new(Span::styled(words, dim())), area);
        return;
    }

    let selected_entry = entries
        .iter()
        .position(|entry| match entry {
            Entry::Task(index) => Some(*index) == sessions_screen.selected_index(),
            Entry::ShowMore => sessions_screen.on_show_more(),
            _ => false,
        })
        .unwrap_or_default();
    let padded = area.height >= 3;
    let viewport = Rect::new(
        area.x,
        area.y + u16::from(padded),
        area.width,
        area.height - 2 * u16::from(padded),
    );
    let metadata = area.width >= METADATA_FROM;
    let columns = Columns {
        metadata,
        project: metadata
            && area.width >= PROJECT_FROM
            && sessions_screen.grouping() != Grouping::Project,
        wide: metadata && area.width >= WIDE_FROM,
    };
    if padded {
        draw_columns(frame, Rect::new(area.x, area.y, area.width, 1), columns);
    }
    let height = usize::from(viewport.height);
    sessions_screen.page_height().set(height);

    let mut start = sessions_screen
        .scroll()
        .get()
        .min(entries.len().saturating_sub(height));
    if selected_entry < start {
        // The heading above the first row of a group comes into view with it.
        start = if height > 1
            && selected_entry > 0
            && matches!(entries[selected_entry - 1], Entry::Group(_))
        {
            selected_entry - 1
        } else {
            selected_entry
        };
    }
    if selected_entry >= start + height {
        start = (selected_entry + 1).saturating_sub(height);
    }
    sessions_screen.scroll().set(start);

    for (offset, entry) in entries.iter().skip(start).take(height).enumerate() {
        let rect = Rect::new(
            viewport.x,
            viewport.y + u16::try_from(offset).unwrap_or(u16::MAX),
            viewport.width,
            1,
        );
        match entry {
            Entry::Gap => {}
            Entry::Group(index) => draw_heading(frame, rect, sessions_screen, *index),
            Entry::Said(index) => {
                let said = sessions_screen
                    .said_in(&sessions_screen.rows()[*index])
                    .unwrap_or("");
                let room = rect.width.saturating_sub(GUTTER);
                frame.render_widget(
                    Paragraph::new(Span::styled(head_of(said, usize::from(room)), dim())),
                    Rect::new(rect.x + GUTTER.min(rect.width), rect.y, room, 1),
                );
            }
            Entry::Task(index) => {
                // The cursor is in the search box while it has the focus, and no row is the cursor's.
                let chosen = Some(*index) == sessions_screen.selected_index()
                    && !sessions_screen.searching();
                let row = &sessions_screen.rows()[*index];
                let starred = sessions_screen.is_pinned(*index)
                    && sessions_screen.grouping() == Grouping::Together;
                draw_task(frame, rect, row, chosen, columns, starred);
                laid.targets.push(Target::on_screen(
                    rect,
                    Hit::SessionsScreen(Click::Row(*index)),
                ));
            }
            Entry::ShowMore => {
                let chosen = sessions_screen.on_show_more();
                let style = match chosen {
                    true => theme::picked_out().fg(theme::on_primary()),
                    false => Style::default().fg(theme::text()),
                };
                frame.render_widget(Block::default().style(style), rect);
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        format!(
                            "{}   {}",
                            if chosen { "›" } else { " " },
                            t!(sessions_screen_show_more)
                        ),
                        style,
                    )),
                    rect,
                );
                laid.targets.push(Target::on_screen(
                    rect,
                    Hit::SessionsScreen(Click::ShowMore),
                ));
            }
        }
    }
    if padded && start > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled("↑", dim())),
            Rect::new(area.x, area.y, 1, 1),
        );
    }
    if padded && start + height < entries.len() {
        frame.render_widget(
            Paragraph::new(Span::styled("↓", dim())),
            Rect::new(area.x, area.bottom() - 1, 1, 1),
        );
    }
}

/// The names of the columns above the list.
/// Which of the columns after the name a row has room for.
#[derive(Debug, Clone, Copy)]
struct Columns {
    /// The status and the age.
    metadata: bool,
    /// The project, in a list whose headings are not projects.
    project: bool,
    /// The branch and the size.
    wide: bool,
}

/// Where each column stands in a row.
struct Placed {
    project: Option<Rect>,
    branch: Option<Rect>,
    size: Option<Rect>,
    status: Rect,
    age: Rect,
}

impl Columns {
    /// The columns the name gives up its room to.
    fn taken(self) -> u16 {
        let metadata = if self.metadata {
            STATUS_WIDTH + 2 + AGE_WIDTH + 1
        } else {
            0
        };
        let project = if self.project { PROJECT_WIDTH + 2 } else { 0 };
        let wide = if self.wide {
            BRANCH_WIDTH + 2 + SIZE_WIDTH + 2
        } else {
            0
        };
        metadata + project + wide
    }

    /// Where the columns stand in a row drawn in `rect`, from the right: the age, the status, then
    /// the size, the branch and the project where there is room for them.
    fn place(self, rect: Rect) -> Placed {
        let age = Rect::new(rect.right().saturating_sub(AGE_WIDTH), rect.y, AGE_WIDTH, 1);
        let status = Rect::new(
            age.x.saturating_sub(2 + STATUS_WIDTH),
            rect.y,
            STATUS_WIDTH,
            1,
        );
        let mut left = status.x;
        let mut take = |width: u16, wanted: bool| {
            wanted.then(|| {
                let at = Rect::new(left.saturating_sub(2 + width), rect.y, width, 1);
                left = at.x;
                at
            })
        };
        let size = take(SIZE_WIDTH, self.wide);
        let branch = take(BRANCH_WIDTH, self.wide);
        let project = take(PROJECT_WIDTH, self.project);
        Placed {
            project,
            branch,
            size,
            status,
            age,
        }
    }
}

/// The names of the columns above the list.
fn draw_columns(frame: &mut Frame, area: Rect, columns: Columns) {
    frame.render_widget(
        Paragraph::new(Span::styled(t!(sessions_screen_column_tasks), dim())),
        Rect::new(
            area.x + GUTTER.min(area.width),
            area.y,
            area.width.saturating_sub(GUTTER),
            1,
        ),
    );
    if !columns.metadata {
        return;
    }
    let placed = columns.place(area);
    let mut name = |at: Option<Rect>, text: &'static str, right: bool| {
        if let Some(at) = at {
            let label = Paragraph::new(Span::styled(text, dim()));
            frame.render_widget(
                match right {
                    true => label.alignment(ratatui::layout::Alignment::Right),
                    false => label,
                },
                at,
            );
        }
    };
    name(placed.project, t!(sessions_screen_column_project), false);
    name(placed.branch, t!(sessions_screen_column_branch), false);
    name(placed.size, t!(sessions_screen_column_size), true);
    name(
        Some(placed.status),
        t!(sessions_screen_column_status),
        false,
    );
    name(Some(placed.age), t!(sessions_screen_column_updated), true);
}

/// The name a project goes by in the list: the last two parts of its directory, which tell two
/// checkouts of different projects apart where the last part alone often does not.
fn project_name(directory: &str) -> String {
    let path = std::path::Path::new(directory);
    let parts: Vec<String> = path
        .components()
        .filter_map(|part| match part {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    match parts.len() {
        0 => directory.to_string(),
        1 => parts[0].clone(),
        count => parts[count - 2..].join("/"),
    }
}

fn draw_heading(frame: &mut Frame, rect: Rect, sessions_screen: &SessionsScreen, index: usize) {
    let row = &sessions_screen.rows()[index];
    let label = if sessions_screen.is_pinned(index) {
        t!(sessions_screen_group_pinned).to_string()
    } else {
        match sessions_screen.grouping() {
            Grouping::Agent => row
                .agent
                .clone()
                .unwrap_or_else(|| t!(sessions_screen_agent_none).to_string()),
            Grouping::Model => row
                .model
                .clone()
                .unwrap_or_else(|| t!(sessions_screen_model_unknown).to_string()),
            _ => {
                let directory = crate::status::abbreviate(std::path::Path::new(&row.directory));
                match sessions_screen.is_here(&row.task.directory) {
                    true => format!("{}  {directory}", t!(sessions_screen_here_heading)),
                    false => directory,
                }
            }
        }
    };
    let (shown, all) = sessions_screen.group_counts(index);
    let count = match shown == all {
        true => shown.to_string(),
        false => t!(sessions_screen_group_count_of, shown = shown, all = all),
    };
    let room = usize::from(rect.width).saturating_sub(count.width() + 2);
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{}  {count}", head_of(&label, room.min(64))),
            dim(),
        )),
        rect,
    );
}

#[expect(
    clippy::too_many_lines,
    reason = "the cells of one row, each placed by the columns the row has room for"
)]
fn draw_task(
    frame: &mut Frame,
    rect: Rect,
    row: &Row,
    chosen: bool,
    columns: Columns,
    starred: bool,
) {
    let fill = theme::picked_out().fg(theme::on_primary());
    if chosen {
        frame.render_widget(Block::default().style(fill), rect);
    }
    let base = match chosen {
        true => fill,
        false => Style::default().fg(theme::text()),
    };
    let (dot, colour) = status_dot(row.task.status);
    let dot_style = match chosen {
        true => fill,
        false => Style::default().fg(colour),
    };
    let marker = Rect::new(rect.x, rect.y, rect.width.min(1), 1);
    frame.render_widget(
        Paragraph::new(Span::styled(if chosen { "›" } else { " " }, base)),
        marker,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(dot, dot_style)),
        Rect::new(
            rect.x + rect.width.min(2),
            rect.y,
            rect.width.saturating_sub(2).min(1),
            1,
        ),
    );

    let title_x = rect.x + GUTTER.min(rect.width);
    let room = rect
        .width
        .saturating_sub(GUTTER)
        .saturating_sub(columns.taken());
    // Said on the row rather than on selection, so nobody picks a manifest run and then finds out.
    let mut here = String::new();
    if row.task.current {
        here.push_str(&format!("  {}", t!(sessions_screen_here)));
    }
    if row.task.manifest {
        here.push_str(&format!("  {}", t!(sessions_screen_manifest)));
    }
    // A pin is a star before the name where there is no heading to say so.
    let star = if starred { "★ " } else { "" };
    let title = head_of(
        &row.title,
        usize::from(room)
            .saturating_sub(here.width())
            .saturating_sub(star.width())
            .max(1),
    );
    let mut spans = Vec::new();
    if starred {
        spans.push(Span::styled(
            star,
            match chosen {
                true => base,
                false => Style::default().fg(theme::accent()),
            },
        ));
    }
    spans.push(Span::styled(
        title,
        match chosen {
            true => base,
            false => Style::default().fg(theme::text()),
        },
    ));
    if !here.is_empty() {
        spans.push(Span::styled(here, if chosen { base } else { dim() }));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(title_x, rect.y, room, 1),
    );
    let cell_style = match chosen {
        true => base,
        false => dim(),
    };
    let placed = columns.place(rect);
    if let Some(at) = placed.project {
        frame.render_widget(
            Paragraph::new(Span::styled(
                head_of(&project_name(&row.directory), usize::from(PROJECT_WIDTH)),
                cell_style,
            )),
            at,
        );
    }
    if let Some(at) = placed.branch {
        frame.render_widget(
            Paragraph::new(Span::styled(
                head_of(
                    row.branch.as_deref().unwrap_or(""),
                    usize::from(BRANCH_WIDTH),
                ),
                cell_style,
            )),
            at,
        );
    }
    if let Some(at) = placed.size {
        frame.render_widget(
            Paragraph::new(Span::styled(row.size.clone(), cell_style))
                .alignment(ratatui::layout::Alignment::Right),
            at,
        );
    }
    if columns.metadata {
        let (age, status) = (placed.age, placed.status);
        frame.render_widget(
            Paragraph::new(Span::styled(status_word(row.task.status), base)),
            status,
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                crate::history_search::age_since(row.task.updated),
                match chosen {
                    true => base,
                    false => dim(),
                },
            ))
            .alignment(ratatui::layout::Alignment::Right),
            age,
        );
    }
}

/// The details of the task under the cursor: what it is and what it has used, and for a task that
/// needs the person, which kind of thing it is waiting for (SCREEN-3).
#[expect(
    clippy::too_many_lines,
    reason = "the lines of the details, one block for each fact a session has"
)]
fn details_lines(row: &Row, width: usize) -> Vec<Line<'static>> {
    let task = &row.task;
    let (dot, colour) = status_dot(task.status);
    let label = |text: &'static str| Line::from(Span::styled(text, dim()));
    let mut lines = vec![
        Line::from(Span::styled(
            t!(sessions_screen_details_title),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::default(),
        Line::from(Span::styled(
            head_of(&row.title, width),
            Style::default()
                .fg(theme::text())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(dot, Style::default().fg(colour)),
            Span::raw(" "),
            Span::raw(status_word(task.status)),
        ]),
    ];
    match task.status {
        Status::NeedsInput => {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                t!(sessions_screen_details_attention),
                Style::default().fg(theme::fail()),
            )));
            lines.push(Line::from(Span::raw(match task.held {
                Some(held) => t!(sessions_screen_details_waiting_for, kind = held_word(held)),
                None => t!(sessions_screen_details_waiting).to_string(),
            })));
            lines.push(Line::from(Span::styled(
                t!(sessions_screen_details_open_to_answer),
                dim(),
            )));
        }
        Status::Interrupted => {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                t!(sessions_screen_details_attention),
                Style::default().fg(theme::fail()),
            )));
            lines.push(Line::from(Span::raw(t!(
                sessions_screen_details_interrupted
            ))));
        }
        Status::OpenElsewhere => lines.push(Line::from(Span::styled(
            t!(sessions_screen_details_elsewhere),
            dim(),
        ))),
        _ => {}
    }
    if task.current {
        lines.push(Line::from(Span::styled(
            t!(sessions_screen_details_here),
            dim(),
        )));
    }
    if task.manifest {
        lines.push(Line::from(Span::styled(
            t!(sessions_screen_details_manifest),
            dim(),
        )));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::raw(t!(
        sessions_screen_details_model,
        model = match &row.model {
            Some(model) => model.clone(),
            None => t!(sessions_screen_model_unknown).to_string(),
        }
    ))));
    if let Some(mode) = task.mode {
        lines.push(Line::from(Span::raw(t!(
            sessions_screen_details_mode,
            mode = mode_word(mode)
        ))));
    }
    lines.push(Line::from(Span::raw(t!(
        sessions_screen_details_turns,
        turns = task.turns
    ))));
    if task.tokens > 0 {
        lines.push(Line::from(Span::raw(if task.read + task.written > 0 {
            t!(
                sessions_screen_details_tokens_split,
                read = crate::indicator::format_tokens(task.read),
                written = crate::indicator::format_tokens(task.written)
            )
        } else {
            t!(
                sessions_screen_details_tokens_total,
                total = crate::indicator::format_tokens(task.tokens)
            )
        })));
    }
    lines.push(Line::default());
    lines.push(label(t!(sessions_screen_details_project)));
    lines.push(Line::from(Span::raw(row.directory.clone())));
    if let Some(branch) = &row.branch {
        lines.push(Line::default());
        lines.push(label(t!(sessions_screen_details_branch)));
        lines.push(Line::from(Span::raw(branch.clone())));
    }
    let mut detail = |name: &'static str, value: String| {
        lines.push(Line::default());
        lines.push(label(name));
        lines.push(Line::from(Span::raw(value)));
    };
    if let Some(agent) = &row.agent {
        detail(t!(sessions_screen_details_agent), agent.clone());
    }
    if let Some(issue) = &row.issue {
        detail(t!(sessions_screen_details_issue), issue.clone());
    }
    if let Some(pull_request) = &row.pull_request {
        detail(
            t!(sessions_screen_details_pull_request),
            pull_request.clone(),
        );
    }
    if let Some(source) = &row.handed_off_from {
        detail(t!(sessions_screen_details_handed_off), source.clone());
    }
    if !row.size.is_empty() {
        detail(t!(sessions_screen_details_size), row.size.clone());
    }
    detail(t!(sessions_screen_details_id), task.id.clone());
    if let Some(prompt) = &row.prompt {
        lines.push(Line::default());
        lines.push(label(t!(sessions_screen_details_prompt)));
        // Two lines of it at most, the second ending in an ellipsis where it was cut.
        lines.push(Line::from(Span::raw(head_of(
            prompt,
            width.saturating_mul(2).saturating_sub(1),
        ))));
    }
    lines
}

/// What the list has no column for, said for the session under the cursor: the branch, the size
/// and the start of the id.
fn draw_strip(frame: &mut Frame, area: Rect, sessions_screen: &SessionsScreen) {
    let Some(row) = sessions_screen.selected_row() else {
        return;
    };
    let parts: Vec<&str> = [
        row.branch.as_deref(),
        Some(row.size.as_str()).filter(|size| !size.is_empty()),
        Some(row.short_id.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect();
    frame.render_widget(
        Paragraph::new(Span::styled(
            head_of(&parts.join("  ·  "), usize::from(area.width)),
            dim(),
        )),
        area,
    );
}

fn draw_details(frame: &mut Frame, area: Rect, sessions_screen: &SessionsScreen) {
    let Some(row) = sessions_screen.selected_row() else {
        return;
    };
    frame.render_widget(
        Paragraph::new(details_lines(row, usize::from(area.width))).wrap(Wrap { trim: false }),
        area,
    );
}

/// The keys under the list, and what each does.
fn footer_hints(sessions_screen: &SessionsScreen) -> Vec<(String, &'static str)> {
    let mut hints = Vec::new();
    if sessions_screen.confirming().is_some() {
        return vec![
            ("y".into(), t!(sessions_screen_hint_confirm)),
            ("n".into(), t!(sessions_screen_hint_cancel)),
        ];
    }
    if !sessions_screen.helping() && !sessions_screen.editing() {
        hints.push((
            sessions_screen.keys().name(Action::Help),
            t!(sessions_screen_hint_help),
        ));
    }
    hints.push(("esc".into(), t!(sessions_screen_hint_back)));
    if sessions_screen.helping() {
        return hints;
    }
    hints.push(("↑/↓".into(), t!(sessions_screen_hint_move)));
    hints.push((
        "enter".into(),
        match (sessions_screen.renaming(), sessions_screen.searching()) {
            (Some(_), _) => t!(sessions_screen_hint_rename),
            (None, true) => t!(sessions_screen_hint_open_first),
            (None, false) => t!(sessions_screen_hint_open),
        },
    ));
    if !sessions_screen.editing() {
        hints.push((
            sessions_screen.keys().name(Action::New),
            t!(sessions_screen_hint_new),
        ));
    }
    hints
}

fn draw_footer(frame: &mut Frame, area: Rect, sessions_screen: &SessionsScreen) {
    if area.is_empty() {
        return;
    }
    let area = area.inner(Margin::new(2, 0));
    if let Some(notice) = sessions_screen.notice() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                head_of(notice, usize::from(area.width)),
                dim(),
            )),
            area,
        );
        return;
    }
    let mut spans: Vec<Span> = Vec::new();
    for (key, word) in footer_hints(sessions_screen) {
        if !spans.is_empty() {
            spans.push(Span::styled("  ", dim()));
        }
        spans.push(Span::styled(
            key,
            Style::default()
                .fg(theme::text())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {word}"), dim()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The shortcuts, in the groups the footer's hints are the first of.
#[expect(clippy::too_many_lines, reason = "the list of keys, one row for each")]
fn draw_help(frame: &mut Frame, keys: &SessionsKeys, area: Rect) {
    if area.is_empty() {
        return;
    }
    let group = |title: &'static str, entries: Vec<(String, &'static str)>| {
        // The keys are one column as wide as the widest of them, and two more.
        let keys = entries
            .iter()
            .map(|(key, _)| key.width())
            .max()
            .unwrap_or(0)
            + 2;
        let mut lines = vec![Line::from(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD),
        ))];
        for (key, word) in entries {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{key}{}", " ".repeat(keys - key.width())),
                    Style::default().fg(theme::text()),
                ),
                Span::styled(word, dim()),
            ]));
        }
        lines
    };
    let navigate = group(
        t!(sessions_screen_help_navigate),
        vec![
            (
                format!("↑ {}", keys.name(Action::Up)),
                t!(sessions_screen_help_up),
            ),
            (
                format!("↓ {}", keys.name(Action::Down)),
                t!(sessions_screen_help_down),
            ),
            ("enter →".into(), t!(sessions_screen_help_open)),
            ("pgup".into(), t!(sessions_screen_help_page_up)),
            ("pgdn".into(), t!(sessions_screen_help_page_down)),
            ("ctrl-c".into(), t!(sessions_screen_help_quit)),
        ],
    );
    let tasks = group(
        t!(sessions_screen_help_tasks),
        vec![
            (keys.name(Action::New), t!(sessions_screen_help_new)),
            (keys.name(Action::Fork), t!(sessions_screen_help_fork)),
            (keys.name(Action::Rename), t!(sessions_screen_help_rename)),
            (keys.name(Action::Stop), t!(sessions_screen_help_stop)),
            (keys.name(Action::Archive), t!(sessions_screen_help_archive)),
            (keys.name(Action::Hide), t!(sessions_screen_help_hide)),
            (keys.name(Action::Delete), t!(sessions_screen_help_delete)),
            (keys.name(Action::Pin), t!(sessions_screen_help_pin)),
        ],
    );
    let view = group(
        t!(sessions_screen_help_view),
        vec![
            (filter_keys().into(), t!(sessions_screen_help_filter)),
            ("1-5".into(), t!(sessions_screen_help_tabs)),
            (keys.name(Action::Search), t!(sessions_screen_help_search)),
            (keys.name(Action::Group), t!(sessions_screen_help_group)),
            (keys.name(Action::Help), t!(sessions_screen_help_help)),
        ],
    );
    let title = Line::from(Span::styled(
        t!(sessions_screen_help_title),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    let below = Rect::new(
        area.x,
        area.y + 2.min(area.height),
        area.width,
        area.height.saturating_sub(2),
    );
    frame.render_widget(
        Paragraph::new(title),
        Rect::new(area.x, area.y, area.width, 1),
    );
    if below.width >= 90 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Ratio(1, 3); 3])
            .split(below);
        for (lines, column) in [navigate, tasks, view].into_iter().zip(columns.iter()) {
            frame.render_widget(Paragraph::new(lines), *column);
        }
        return;
    }
    let mut stacked = navigate;
    for part in [tasks, view] {
        stacked.push(Line::default());
        stacked.extend(part);
    }
    let room = usize::from(below.height);
    if stacked.len() > room && room > 0 {
        stacked.truncate(room - 1);
        stacked.push(Line::from(Span::styled(
            t!(sessions_screen_help_resize),
            dim(),
        )));
    }
    frame.render_widget(Paragraph::new(stacked), below);
}

/// The question before an archive or a delete, over the list.
fn draw_confirm(frame: &mut Frame, area: Rect, confirm: &Confirm) {
    let name = crate::render::printable(confirm.task.title.lines().next().unwrap_or("").trim());
    let name = head_of(&name, 48);
    let (title, body) = match confirm.what {
        Destructive::Archive => (
            t!(sessions_screen_archive_title, name = name),
            t!(sessions_screen_archive_body),
        ),
        Destructive::Delete => (
            t!(sessions_screen_delete_title, name = name),
            t!(sessions_screen_delete_body),
        ),
        Destructive::Interrupt => (
            match confirm.then.as_deref() {
                Some(Effect::Fork(_)) => t!(sessions_screen_interrupt_copy_title, name = name),
                Some(Effect::New(_)) => t!(sessions_screen_interrupt_new_title, name = name),
                _ => t!(sessions_screen_interrupt_open_title, name = name),
            },
            t!(sessions_screen_interrupt_body),
        ),
    };
    let width = area.width.min(64).max(20.min(area.width));
    let inner_width = usize::from(width.saturating_sub(4)).max(1);
    let body_lines = textwrap_lines(body, inner_width);
    // The question, a blank line, the explanation, a blank line and the keys, inside a border.
    let height = (u16::try_from(body_lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(6))
    .min(area.height);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let mut lines = vec![
        Line::from(Span::styled(
            head_of(&title, inner_width),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::default(),
    ];
    lines.extend(
        body_lines
            .into_iter()
            .map(|line| Line::from(Span::styled(line, dim()))),
    );
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {}   ", t!(sessions_screen_hint_confirm)), dim()),
        Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {}", t!(sessions_screen_hint_cancel)), dim()),
    ]));
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::fail()))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        ),
        popup,
    );
}

/// `text` broken into lines of at most `width` columns at the blanks.
fn textwrap_lines(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.width() + 1 + word.width() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions_screen::fixtures::{mixed, sessions_screen_of, task};
    use crate::sessions_screen::{Click, Outcome};
    use bravebot_session::listing::{Page, Status};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn press(session: &mut Session, code: KeyCode) {
        let sessions_screen = session
            .sessions_screen_mut()
            .expect("the Sessions screen is open");
        sessions_screen.key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn typed(session: &mut Session, text: &str) {
        for letter in text.chars() {
            press(session, KeyCode::Char(letter));
        }
    }

    fn opened(tasks: Vec<bravebot_session::listing::Task>) -> Session {
        let mut session = Session::new("kernel-enforced");
        session.open_sessions_screen(sessions_screen_of(tasks));
        session
    }

    /// The screen as the frame drew it, a string for each row with the blanks at its end taken off.
    fn screen(
        session: &Session,
        width: u16,
        height: u16,
    ) -> (Vec<String>, Laid, Terminal<TestBackend>) {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a terminal");
        let mut laid = Laid::default();
        terminal
            .draw(|frame| laid = super::super::draw(frame, session))
            .expect("the frame draws");
        let buffer = terminal.backend().buffer().clone();
        let rows = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect();
        (rows, laid, terminal)
    }

    fn row_with<'a>(rows: &'a [String], needle: &str) -> Option<(usize, &'a String)> {
        rows.iter()
            .enumerate()
            .find(|(_, row)| row.contains(needle))
    }

    /// SCREEN-4. The header says how many sessions are waiting on the person and how many are
    /// working, names what the grouping key will do, and counts each tab.
    #[test]
    fn the_header_summarises_what_needs_attention_and_counts_each_tab() {
        let mut session = opened(mixed());

        let (rows, _, _) = screen(&session, 120, 30);

        assert!(rows[0].contains("Sessions"), "{:?}", rows[0]);
        assert!(rows[0].contains("1 waiting on you"), "{:?}", rows[0]);
        assert!(rows[0].contains("2 working"), "{:?}", rows[0]);
        assert!(rows[0].contains("g  group by project"), "{:?}", rows[0]);
        for counted in ["All 5", "Needs you 1", "Working 2", "Ready 1", "Inactive 1"] {
            assert!(
                rows[1].contains(counted),
                "{counted:?} is not in {:?}",
                rows[1]
            );
        }
        assert!(rows[1].contains("Tab/Shift-Tab filter"), "{:?}", rows[1]);

        // The key names what it will do, so each press names the next layout and the last one names
        // the way back.
        for next in ["g  group by agent", "g  group by model", "g  list together"] {
            press(&mut session, KeyCode::Char('g'));
            let (rows, _, _) = screen(&session, 120, 30);
            assert!(rows[0].contains(next), "{next:?} is not in {:?}", rows[0]);
        }
        press(&mut session, KeyCode::Char('g'));
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(rows[0].contains("g  group by project"), "{:?}", rows[0]);
    }

    /// SCREEN-4. With nothing waiting or working the header says so, and the count of what waits is
    /// the one tab drawn in a colour of its own.
    #[test]
    fn a_calm_header_says_so_and_only_the_waiting_count_has_a_colour() {
        let calm = opened(vec![task("a", "/work/one", Status::Saved, 5)]);
        let (rows, _, _) = screen(&calm, 120, 30);
        assert!(
            rows[0].contains("Nothing is waiting on you"),
            "{:?}",
            rows[0]
        );

        let busy = opened(mixed());
        let (rows, _, terminal) = screen(&busy, 120, 30);
        let buffer = terminal.backend().buffer();
        let at = |needle: &str| {
            let column = rows[1].find(needle).expect("a tab") as u16;
            buffer[(column, 1)].fg
        };
        assert_eq!(at("Needs you 1"), theme::fail());
        assert_ne!(at("Ready 1"), theme::fail());
    }

    /// SCREEN-4. Every tab is recorded where it was drawn, so a click on it is found.
    #[test]
    fn each_tab_and_row_is_recorded_where_it_is_drawn() {
        let session = opened(mixed());

        let (rows, laid, _) = screen(&session, 120, 30);

        let tabs: Vec<usize> = laid
            .targets
            .iter()
            .filter_map(|target| match target.hit {
                Hit::SessionsScreen(Click::Tab(tab)) => Some(tab),
                _ => None,
            })
            .collect();
        assert_eq!(tabs, [0, 1, 2, 3, 4]);
        let tab = laid
            .targets
            .iter()
            .find(|target| target.hit == Hit::SessionsScreen(Click::Tab(1)))
            .expect("the second tab");
        let drawn = &rows[usize::from(tab.area.y)];
        let text: String = drawn
            .chars()
            .skip(usize::from(tab.area.x))
            .take(usize::from(tab.area.width))
            .collect();
        assert!(
            text.contains("Needs you"),
            "the target is not over its tab: {text:?}"
        );

        let at = session
            .sessions_screen()
            .expect("open")
            .rows()
            .iter()
            .position(|row| row.task.id == "d")
            .expect("d");
        let target = laid
            .targets
            .iter()
            .find(|target| target.hit == Hit::SessionsScreen(Click::Row(at)))
            .expect("the row of d");
        assert!(
            rows[usize::from(target.area.y)].contains("title d"),
            "the target is not over its row"
        );
    }

    /// SCREEN-3. The row under the cursor is marked and filled, and no other is.
    #[test]
    fn the_row_under_the_cursor_is_marked_and_filled_edge_to_edge() {
        let session = opened(mixed());

        let (rows, _, terminal) = screen(&session, 120, 30);

        let under = session
            .sessions_screen()
            .and_then(|sessions_screen| sessions_screen.selected_row())
            .expect("a row");
        let (y, row) = row_with(&rows, &under.title).expect("the row is drawn");
        assert!(row.trim_start().starts_with('›'), "{row:?}");
        let fill = theme::picked_out().bg;
        let buffer = terminal.backend().buffer();
        let marked = (0..80)
            .filter(|x| buffer[(*x, y as u16)].bg == fill.unwrap_or_default())
            .count();
        assert!(
            marked >= 70,
            "the row is not filled across the list: {marked}"
        );
        let other = rows
            .iter()
            .position(|row| row.contains("title") && !row.contains(&under.title))
            .expect("another row");
        assert_ne!(
            buffer[(10, other as u16)].bg,
            fill.unwrap_or_default(),
            "another row is filled"
        );
        assert_eq!(
            rows.iter()
                .filter(|row| row.trim_start().starts_with('›'))
                .count(),
            1,
            "more than one row is marked"
        );
    }

    /// SCREEN-3. The status and the age stand in columns where the list is wide enough for them and
    /// are left out where it is not, so the name keeps the room. The details take 41 columns of a
    /// wide screen, so a screen of 100 columns has a list too narrow for them.
    #[test]
    fn the_status_and_age_columns_are_dropped_where_the_list_is_too_narrow() {
        let session = opened(mixed());

        for (width, shown) in [(120, true), (80, true), (100, false), (50, false)] {
            let (rows, _, _) = screen(&session, width, 30);
            let (_, row) = row_with(&rows, "title c").expect("the row is drawn");
            assert_eq!(
                row.contains("Needs input"),
                shown,
                "at {width} columns: {row:?}"
            );
            assert_eq!(
                row.contains("ago") || row.contains("now"),
                shown,
                "at {width} columns: {row:?}"
            );
        }
    }

    /// SCREEN-3. The details of the task under the cursor stand beside the list on a wide screen and
    /// are not drawn on a narrow one.
    #[test]
    fn the_details_stand_beside_the_list_only_where_the_screen_is_wide() {
        let session = opened(mixed());

        let (wide, _, _) = screen(&session, 120, 30);
        let (narrow, _, _) = screen(&session, 80, 30);

        assert!(wide.iter().any(|row| row.contains("Session details")));
        assert!(wide.iter().any(|row| row.contains("Model: model-a")));
        assert!(
            !narrow.iter().any(|row| row.contains("Session details")),
            "{narrow:?}"
        );
    }

    /// SCREEN-3. A task that needs the person says which kind of question it holds, in a word from
    /// the fixed set, and one whose process ended in a turn says that.
    #[test]
    fn a_task_that_needs_the_person_says_what_kind_of_thing_it_waits_for() {
        let mut asking = task("q", "/work/one", Status::NeedsInput, 5);
        asking.held = Some(bravebot_session::jobs::Held::Write);
        asking.title = "ask me".to_string();
        let mut died = task("d", "/work/two", Status::Interrupted, 4);
        died.title = "died".to_string();
        let mut session = opened(vec![asking, died]);

        let (rows, _, _) = screen(&session, 120, 30);
        assert!(rows.iter().any(|row| row.contains("Needs attention")));
        assert!(
            rows.iter().any(|row| row.contains("answer to a write")),
            "{rows:?}"
        );
        press(&mut session, KeyCode::Down);
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows.iter()
                .any(|row| row.contains("ended in the middle of a")),
            "{rows:?}"
        );
    }

    /// SCREEN-5. A heading counts what is shown of the project when a search narrows it.
    #[test]
    fn a_heading_counts_what_is_shown_of_what_there_is_when_a_search_narrows_it() {
        let mut session = opened(mixed());
        press(&mut session, KeyCode::Char('g'));
        assert_eq!(
            session.sessions_screen().expect("open").grouping(),
            Grouping::Project
        );

        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows.iter().any(|row| row.contains("/work/one  2")),
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("/work/two  3")),
            "{rows:?}"
        );

        press(&mut session, KeyCode::Char('/'));
        typed(&mut session, "title d");
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows.iter().any(|row| row.contains("/work/two  1 of 3")),
            "a project with some of its sessions left out says how many: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains("/work/one")),
            "{rows:?}"
        );
    }

    /// SCREEN-5. The list that is not grouped has no headings, names the project of each session in
    /// a column where there is room, and the grouped list does not repeat it on every row.
    #[test]
    fn the_project_is_a_column_of_the_list_that_is_not_grouped_and_a_heading_of_the_one_that_is() {
        let mut session = opened(mixed());

        let (together, _, _) = screen(&session, 120, 30);
        let row = row_with(&together, "title a").expect("a row").1;
        assert!(row.contains("work/one"), "no project column: {row:?}");
        let titles = |rows: &[String]| rows[4].chars().take(76).collect::<String>();
        assert!(titles(&together).contains("Project"), "no column title");
        assert!(
            !together.iter().any(|row| row.contains("/work/one  2")),
            "the list that is not grouped has a heading"
        );

        press(&mut session, KeyCode::Char('g'));
        let (grouped, _, _) = screen(&session, 120, 30);
        let row = row_with(&grouped, "title a").expect("a row").1;
        assert!(
            !row.contains("work/one"),
            "the project is repeated on a row: {row:?}"
        );
        assert!(!titles(&grouped).contains("Project"), "a column title");

        let (narrow, _, _) = screen(&opened(mixed()), 70, 30);
        let row = row_with(&narrow, "title a").expect("a row").1;
        assert!(
            !row.contains("work/one"),
            "the column was kept where it leaves no room: {row:?}"
        );
    }

    /// SCREEN-5. The project layout puts this directory first under a heading that says so.
    #[test]
    fn the_project_layout_puts_this_directory_first_and_says_so() {
        let mut session = Session::new("kernel-enforced");
        let mut sessions_screen = SessionsScreen::new(
            Grouping::Project,
            Vec::new(),
            std::collections::HashMap::new(),
            None,
        )
        .in_directory(std::path::PathBuf::from("/work/two"));
        sessions_screen.replace(bravebot_session::listing::Page {
            tasks: mixed(),
            more: false,
        });
        session.open_sessions_screen(sessions_screen);

        let (rows, _, _) = screen(&session, 120, 30);

        let (here, heading) = row_with(&rows, "Here").expect("a heading for this directory");
        assert!(heading.contains("Here  /work/two"), "{heading:?}");
        let (other, _) = row_with(&rows, "/work/one").expect("the other project");
        assert!(here < other, "this directory is not first");
    }

    /// SCREEN-5. A pinned session is first and marked with a star in the list that is not grouped, and
    /// stands under a heading of its own in the one that is.
    #[test]
    fn a_pinned_session_is_a_star_together_and_a_heading_by_project() {
        let mut session = opened(mixed());
        press(&mut session, KeyCode::Char('p'));
        let name = session
            .sessions_screen()
            .and_then(|sessions_screen| sessions_screen.selected_row())
            .map(|row| row.title.clone())
            .expect("a row");

        let (rows, _, _) = screen(&session, 120, 30);
        let (first, row) = row_with(&rows, "★").expect("a starred row");
        assert!(row.contains(&format!("★ {name}")), "{row:?}");
        assert_eq!(rows.iter().filter(|row| row.contains('★')).count(), 1);
        assert!(
            !rows.iter().any(|row| row.contains("Pinned")),
            "a heading in the list that is not grouped"
        );
        assert!(
            rows[..first].iter().all(|row| !row.contains("title")),
            "a row above the pinned one"
        );

        press(&mut session, KeyCode::Char('g'));
        let (rows, _, _) = screen(&session, 120, 30);
        let (heading, _) = row_with(&rows, "Pinned  1").expect("a Pinned heading");
        let selected = rows
            .iter()
            .position(|row| row.contains('›') && !row.contains("Search"))
            .expect("a cursor row");
        assert_eq!(
            selected,
            heading + 1,
            "the pinned row is not under its heading"
        );
        assert!(
            !rows.iter().any(|row| row.contains('★')),
            "a star under a heading"
        );
    }

    /// SCREEN-7. A list longer than the screen scrolls to keep the cursor row on it, and says there
    /// is more above and below.
    #[test]
    fn a_list_taller_than_the_screen_scrolls_to_keep_the_cursor_on_it() {
        let tasks = (0..40)
            .map(|at| task(&format!("t{at:02}"), "/work/one", Status::Saved, 1000 - at))
            .collect();
        let mut session = opened(tasks);
        let (top, _, _) = screen(&session, 120, 14);
        assert!(top.iter().any(|row| row.contains("title t00")));
        assert!(
            top.iter()
                .any(|row| row.trim() == "↓" || row.starts_with('↓') || row.contains("  ↓"))
        );

        press(&mut session, KeyCode::End);
        let (end, _, _) = screen(&session, 120, 14);

        assert!(
            end.iter()
                .any(|row| row.contains("title t39") && row.contains('›')),
            "{end:?}"
        );
        assert!(
            !end.iter().any(|row| row.contains("title t00")),
            "the top is still drawn"
        );
        assert!(
            end.iter()
                .any(|row| row.starts_with('↑') || row.contains("  ↑") || row.trim() == "↑")
        );
    }

    /// SCREEN-7. "Show more" is the last entry when older sessions were left out, and is the row the
    /// cursor goes to after the last task.
    #[test]
    fn show_more_is_drawn_after_the_last_task_when_older_sessions_were_left_out() {
        let mut session = Session::new("kernel-enforced");
        let mut sessions_screen = sessions_screen_of(Vec::new());
        sessions_screen.replace(Page {
            tasks: mixed(),
            more: true,
        });
        session.open_sessions_screen(sessions_screen);
        press(&mut session, KeyCode::End);
        press(&mut session, KeyCode::Down);

        let (rows, laid, _) = screen(&session, 120, 30);

        let (at, row) = row_with(&rows, "Show more").expect("a Show more row");
        assert!(row.contains('›'), "the cursor is not on it: {row:?}");
        assert!(
            at > rows
                .iter()
                .rposition(|row| row.contains("title"))
                .expect("a task")
        );
        assert!(
            laid.targets
                .iter()
                .any(|target| target.hit == Hit::SessionsScreen(Click::ShowMore))
        );
    }

    /// SCREEN-7. An empty list says why it is empty: nothing yet, nothing that matches, or not read.
    #[test]
    fn an_empty_list_says_whether_nothing_exists_or_nothing_matches() {
        let session = opened(Vec::new());
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows.iter().any(|row| row.contains("No sessions yet")),
            "{rows:?}"
        );

        let mut session = opened(mixed());
        press(&mut session, KeyCode::Char('/'));
        typed(&mut session, "zzzz");
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows.iter().any(|row| row.contains("No matching sessions")),
            "{rows:?}"
        );
    }

    /// SCREEN-7. The line being typed stands above the list with its label, and the caret is put
    /// after what was typed.
    #[test]
    fn the_line_being_typed_has_its_label_and_the_caret_after_it() {
        let mut session = opened(mixed());
        press(&mut session, KeyCode::Char('/'));
        typed(&mut session, "ab");

        let (rows, _, mut terminal) = screen(&session, 120, 30);

        let (y, row) = row_with(&rows, "Search ›").expect("a Search line");
        assert!(row.contains("Search › ab"), "{row:?}");
        let caret = terminal.get_cursor_position().expect("a cursor position");
        assert_eq!(caret.y as usize, y);
        assert_eq!(caret.x as usize, 2 + "Search › ab".chars().count());

        let mut session = opened(mixed());
        press(&mut session, KeyCode::Char('r'));
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(rows.iter().any(|row| row.contains("Rename › ")), "{rows:?}");
    }

    /// SCREEN-9. Archiving and deleting put the question over the list, naming the task, and say
    /// which keys answer it.
    #[test]
    fn archive_and_delete_put_a_question_over_the_list_naming_the_task() {
        let mut session = opened(mixed());
        let name = session
            .sessions_screen()
            .and_then(|sessions_screen| sessions_screen.selected_row())
            .map(|row| row.title.clone())
            .expect("a row");

        press(&mut session, KeyCode::Char('a'));
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows.iter()
                .any(|row| row.contains(&format!("Archive “{name}”?"))),
            "{rows:?}"
        );
        assert!(
            rows.iter()
                .any(|row| row.contains("y confirm") && row.contains("n cancel")),
            "{rows:?}"
        );

        press(&mut session, KeyCode::Char('n'));
        press(&mut session, KeyCode::Backspace);
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows.iter()
                .any(|row| row.contains(&format!("Permanently delete “{name}”?"))),
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("cannot be undone")),
            "{rows:?}"
        );
    }

    /// SCREEN-7. The footer names the keys that answer where the person is: the list's, a line being
    /// typed, a question and the help.
    #[test]
    fn the_footer_names_the_keys_that_answer_where_the_person_is() {
        let footer = |session: &Session| {
            let (rows, _, _) = screen(session, 120, 30);
            rows.last().cloned().unwrap_or_default()
        };

        let mut session = opened(mixed());
        let list = footer(&session);
        for hint in ["? help", "esc back", "↑/↓ move", "enter open", "n new"] {
            assert!(list.contains(hint), "{hint:?} is not in {list:?}");
        }

        press(&mut session, KeyCode::Char('/'));
        let searching = footer(&session);
        assert!(
            searching.contains("enter open") && searching.contains("esc back"),
            "{searching:?}"
        );
        assert!(
            !searching.contains("? help") && !searching.contains("n new"),
            "{searching:?}"
        );
        press(&mut session, KeyCode::Down);

        press(&mut session, KeyCode::Char('r'));
        assert!(
            footer(&session).contains("enter rename"),
            "{:?}",
            footer(&session)
        );
        press(&mut session, KeyCode::Esc);

        press(&mut session, KeyCode::Char('a'));
        let asking = footer(&session);
        assert!(
            asking.contains("y confirm") && asking.contains("n cancel"),
            "{asking:?}"
        );
        assert!(!asking.contains("esc back"), "{asking:?}");
        press(&mut session, KeyCode::Esc);

        press(&mut session, KeyCode::Char('?'));
        let helping = footer(&session);
        assert!(
            helping.contains("esc back") && !helping.contains("? help"),
            "{helping:?}"
        );
        assert!(!helping.contains("↑/↓ move"), "{helping:?}");
    }

    /// SCREEN-7. The help lists the keys in three groups, and none of a group's keys run into its
    /// words.
    #[test]
    fn the_help_lists_the_keys_in_three_groups() {
        let mut session = opened(mixed());
        press(&mut session, KeyCode::Char('?'));

        for width in [120_u16, 70] {
            let (rows, _, _) = screen(&session, width, 40);
            for group in ["Navigate", "Sessions", "View"] {
                assert!(
                    rows.iter().any(|row| row.contains(group)),
                    "{group} at {width}: {rows:?}"
                );
            }
            assert!(rows.iter().any(|row| row.contains("Archive")), "{rows:?}");
            assert!(
                rows.iter().all(|row| !row.contains("Tab/Shift-Tabfilter")),
                "a key ran into its word: {rows:?}"
            );
        }
    }

    /// SCREEN-2. A name, a directory or a model that holds a control character is drawn as the
    /// picture of it, on the list, in the details and in a question.
    #[test]
    fn a_control_character_in_what_a_row_holds_is_never_drawn() {
        let mut hostile = task("h", "/work/\u{1b}[2J", Status::Saved, 9);
        hostile.title = "name\u{1b}[31m red\u{7}".to_string();
        hostile.model = Some("m\u{1b}]0;x\u{7}".to_string());
        hostile.branch = Some("b\u{8}ranch".to_string());
        hostile.prompt = Some("p\u{1b}[1;1H".to_string());
        let mut session = opened(vec![hostile]);

        for step in 0..2 {
            if step == 1 {
                press(&mut session, KeyCode::Backspace);
            }
            let (_, _, terminal) = screen(&session, 120, 30);
            for cell in terminal.backend().buffer().content() {
                assert!(
                    !cell.symbol().chars().any(char::is_control),
                    "a control character was drawn: {:?}",
                    cell.symbol()
                );
            }
        }
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows.iter().any(|row| row.contains('\u{241b}')),
            "the escape was not pictured"
        );
    }

    /// SCREEN-7. Whatever the size of the terminal, the frame draws without a panic and keeps
    /// inside it, down to a terminal with no room at all.
    #[test]
    fn a_tiny_terminal_draws_without_a_panic() {
        let mut session = opened(mixed());
        press(&mut session, KeyCode::Char('a'));
        for (width, height) in [
            (1, 1),
            (10, 2),
            (20, 3),
            (30, 5),
            (45, 6),
            (60, 8),
            (89, 12),
        ] {
            screen(&session, width, height);
        }
        press(&mut session, KeyCode::Char('n'));
        press(&mut session, KeyCode::Char('?'));
        for (width, height) in [(1, 1), (10, 2), (30, 5), (60, 8)] {
            screen(&session, width, height);
        }
    }

    /// SCREEN-6. Nothing of the plain frame is drawn under the Sessions screen: not the box, the hints under
    /// it or the transcript.
    #[test]
    fn the_box_and_the_transcript_are_not_drawn_while_the_sessions_screen_is_open() {
        let mut session = opened(mixed());
        session.note("a line of the transcript".to_string());

        let (rows, _, _) = screen(&session, 120, 30);

        assert!(
            !rows
                .iter()
                .any(|row| row.contains("a line of the transcript"))
        );
        assert!(!rows.iter().any(|row| row.contains("Ask Brave Bot")));
        assert!(!rows.iter().any(|row| row.contains("? for shortcuts")));
        let outcome = session
            .sessions_screen_mut()
            .expect("open")
            .key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(outcome, Outcome::Close);
    }

    fn rich(id: &str) -> bravebot_session::listing::Task {
        let mut task = mixed().remove(0);
        task.id = id.to_string();
        task.title = "fix the flaky test".to_string();
        task.branch = Some("fix-flaky".to_string());
        task.bytes = 12 * 1024;
        task.issue = Some("https://example.com/org/repo/issues/12".to_string());
        task.pull_request = Some("https://example.com/org/repo/pull/34".to_string());
        task.handed_off_from = Some("0123456789abcdef".to_string());
        task.agent = Some("reviewer".to_string());
        task
    }

    /// SCREEN-4. A wide list has the branch and the size of every session in columns of their own.
    #[test]
    fn a_wide_list_has_the_branch_and_the_size_of_each_session_in_columns() {
        let session = opened(vec![rich("0123abcd-0000-4000-8000-000000000000")]);
        let (rows, _, _) = screen(&session, 160, 30);
        assert!(rows.iter().any(|row| row.contains("Branch")), "{rows:?}");
        assert!(rows.iter().any(|row| row.contains("Size")), "{rows:?}");
        let (_, line) = row_with(&rows, "fix the flaky test").expect("the row");
        assert!(
            line.contains("fix-flaky") && line.contains("12.0KB"),
            "{line:?}"
        );
    }

    /// SCREEN-4. Where the list is too narrow for the columns and the details, a line under it says
    /// the branch, the size and the start of the id of the session under the cursor.
    #[test]
    fn a_narrow_list_says_the_branch_size_and_id_of_the_cursor_row_under_it() {
        let session = opened(vec![rich("0123abcd-0000-4000-8000-000000000000")]);
        let (rows, _, _) = screen(&session, 80, 30);
        let (_, line) = row_with(&rows, "fix-flaky").expect("the line under the list");
        assert!(
            line.contains("12.0KB") && line.contains("0123abcd"),
            "{line:?}"
        );
    }

    /// SCREEN-3. The details give the whole id, the size, the agent, the links and the session it was
    /// handed off from.
    #[test]
    fn the_details_give_the_id_size_agent_links_and_where_it_was_handed_off_from() {
        let id = "0123abcd-0000-4000-8000-000000000000";
        let session = opened(vec![rich(id)]);
        let (rows, _, _) = screen(&session, 120, 40);
        let all = rows.join("\n");
        for wanted in [id, "12.0KB", "reviewer", "issues/12", "pull/34", "01234567"] {
            assert!(all.contains(wanted), "{wanted:?} is not in {all}");
        }
    }

    /// SCREEN-4. A session found by what was said shows the line that holds the text under its row.
    #[test]
    fn a_found_session_shows_the_line_that_matched_under_its_row() {
        let mut session = opened(vec![rich("0123abcd-0000-4000-8000-000000000000")]);
        session
            .sessions_screen_mut()
            .expect("open")
            .set_corpus_for_test(bravebot_session::search::Corpus::of([(
                "0123abcd-0000-4000-8000-000000000000".to_string(),
                vec!["the migration failed on the staging table".to_string()],
            )]));
        press(&mut session, KeyCode::Char('/'));
        for c in "staging".chars() {
            press(&mut session, KeyCode::Char(c));
        }
        let (rows, _, _) = screen(&session, 120, 30);
        let (at, _) = row_with(&rows, "fix the flaky test").expect("the row");
        assert!(
            rows[at + 1].contains("the migration failed on the staging table"),
            "{rows:?}"
        );
    }

    /// SCREEN-5. Grouped by agent, a heading names the agent, and the sessions with none are under a
    /// heading that says so.
    #[test]
    fn an_agent_heading_names_the_agent_and_the_sessions_with_none() {
        let mut session = opened(vec![rich("0123abcd-0000-4000-8000-000000000000")]);
        let mut plain = mixed().remove(1);
        plain.id = "plain".to_string();
        session
            .sessions_screen_mut()
            .expect("open")
            .replace(bravebot_session::listing::Page {
                tasks: vec![rich("0123abcd-0000-4000-8000-000000000000"), plain],
                more: false,
            });
        press(&mut session, KeyCode::Char('g'));
        press(&mut session, KeyCode::Char('g'));
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(row_with(&rows, "reviewer").is_some(), "{rows:?}");
        assert!(row_with(&rows, "No agent").is_some(), "{rows:?}");
    }

    /// SCREEN-3. A manifest run, which can be read and not continued, says so on its row.
    #[test]
    fn a_manifest_run_is_marked_on_its_row() {
        let mut manifest = mixed().remove(0);
        manifest.title = "a planned run".to_string();
        manifest.manifest = true;
        let session = opened(vec![manifest]);
        let (rows, _, _) = screen(&session, 120, 30);
        let (_, line) = row_with(&rows, "a planned run").expect("the row");
        assert!(line.contains("manifest"), "{line:?}");
    }

    /// SCREEN-4. The search box is always drawn, with a hint of what it searches while it is empty,
    /// and no row is the cursor while the box has the focus.
    #[test]
    fn the_search_box_is_always_drawn_and_the_cursor_is_in_it_or_in_a_row() {
        let mut session = opened(mixed());
        let (rows, _, _) = screen(&session, 120, 30);
        let (at, line) = row_with(&rows, "Search ›").expect("the box");
        assert!(line.contains("what was said in them"), "{line:?}");
        assert!(
            rows.iter()
                .any(|row| row.contains('›') && !row.contains("Search")),
            "no row is the cursor while the cursor is in the list"
        );

        press(&mut session, KeyCode::Up);
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            !rows
                .iter()
                .any(|row| row.contains('›') && !row.contains("Search")),
            "a row is the cursor while the box has the focus: {rows:?}"
        );

        for c in "zzz".chars() {
            press(&mut session, KeyCode::Char(c));
        }
        press(&mut session, KeyCode::Down);
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(rows[at].contains("Search › zzz"), "{:?}", rows[at]);
    }

    /// SCREEN-15. The question over a running turn says which session is opened, copied or begun
    /// from, and that the turn is stopped.
    #[test]
    fn the_question_about_a_running_turn_names_the_session_and_what_is_done() {
        for (wanted, then) in [
            (
                "Stop this turn and open",
                Effect::Open as fn(Box<_>) -> Effect,
            ),
            (
                "Stop this turn and copy",
                Effect::Fork as fn(Box<_>) -> Effect,
            ),
        ] {
            let mut session = opened(mixed());
            let task = session
                .sessions_screen()
                .and_then(|sessions_screen| sessions_screen.selected_row())
                .expect("a row")
                .task
                .clone();
            let name = task.title.clone();
            session
                .sessions_screen_mut()
                .expect("open")
                .ask_to_stop_the_turn(then(Box::new(task)));

            let (rows, _, _) = screen(&session, 120, 30);

            let all = rows.join("\n");
            assert!(all.contains(wanted), "{wanted:?} is not in {all}");
            assert!(all.contains(&name), "{name:?} is not in {all}");
            assert!(all.contains("stopped, as Escape stops it"), "{all}");
        }
        let mut session = opened(mixed());
        session
            .sessions_screen_mut()
            .expect("open")
            .ask_to_stop_the_turn(Effect::New(None));
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows.join("\n").contains("begin a session where"),
            "{rows:?}"
        );
    }

    fn opened_with_keys(pairs: &[(&str, &str)]) -> Session {
        let mut session = Session::new("kernel-enforced");
        let configured = pairs
            .iter()
            .map(|(name, key)| ((*name).to_string(), (*key).to_string()))
            .collect();
        session.adopt_sessions_keys(&configured);
        session.open_sessions_screen(sessions_screen_of(mixed()));
        session
    }

    /// SCREEN-14. The footer, the hint beside the title and the list `?` open name the keys in force
    /// and not the ones the screen began with.
    #[test]
    fn the_footer_the_hint_and_the_help_name_the_keys_in_force() {
        let mut session = opened_with_keys(&[
            ("new", "alt-n"),
            ("group", "ctrl-g"),
            ("stop", "z"),
            ("delete", "d"),
            ("help", "f2"),
            ("up", "w"),
        ]);
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows[0].contains("ctrl-g  group by project"),
            "{:?}",
            rows[0]
        );
        let footer = rows.last().expect("a footer");
        assert!(footer.contains("alt-n new"), "{footer:?}");
        assert!(footer.contains("f2 help"), "{footer:?}");
        assert!(!footer.contains("? help"), "{footer:?}");

        press(&mut session, KeyCode::F(2));
        let (rows, _, _) = screen(&session, 120, 40);
        let all = rows.join("\n");
        for named in ["↑ w", "alt-n", "ctrl-g", "z ", "d "] {
            assert!(all.contains(named), "{named:?} is not in {all}");
        }
        for gone in ["↑ k", "backspace", "\n  x "] {
            assert!(!all.contains(gone), "{gone:?} is still in {all}");
        }
    }

    /// SCREEN-14. What a settings file asked for that the screen could not have is said when the
    /// screen opens, with the key the action stays on.
    #[test]
    fn a_key_that_cannot_be_had_is_said_when_the_screen_opens() {
        let mut session = Session::new("kernel-enforced");
        let configured = [
            ("stop", "enter"),
            ("archive", "banana"),
            ("pin", "z"),
            ("hide", "z"),
        ]
        .iter()
        .map(|(name, key)| ((*name).to_string(), (*key).to_string()))
        .collect();
        session.adopt_sessions_keys(&configured);
        session.open_sessions_screen(sessions_screen_of(mixed()));
        let notice = session
            .sessions_screen()
            .and_then(|screen| screen.notice())
            .expect("a notice")
            .to_string();
        assert!(notice.contains("sessionsKeys."), "{notice:?}");
        assert!(notice.contains("more keys refused"), "{notice:?}");
        let (rows, _, _) = screen(&session, 120, 30);
        assert!(
            rows.last().expect("a footer").contains("sessionsKeys."),
            "{rows:?}"
        );
    }

    /// SCREEN-4. A list narrowed to a pull request names it, and where no session is linked to it the
    /// screen says nothing matches and not that there are no sessions.
    #[test]
    fn a_pull_request_nothing_is_linked_to_is_named_and_says_nothing_matches() {
        let mut session = Session::new("kernel-enforced");
        let mut sessions_screen = SessionsScreen::new(
            Grouping::Together,
            Vec::new(),
            std::collections::HashMap::new(),
            None,
        )
        .from_pull_request("99");
        sessions_screen.replace(bravebot_session::listing::Page {
            tasks: mixed(),
            more: false,
        });
        session.open_sessions_screen(sessions_screen);

        let (rows, _, _) = screen(&session, 120, 30);

        assert!(rows[0].contains("for pull request 99"), "{:?}", rows[0]);
        let all = rows.join("\n");
        assert!(all.contains("No matching sessions"), "{all}");
        assert!(!all.contains("No sessions yet"), "{all}");
    }
}
