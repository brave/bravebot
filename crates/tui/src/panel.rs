//! The info panel beside the transcript: what the session is, how full its context is, and its
//! plan ([PANEL-5](../../../docs/specs/info-panel.md#PANEL-5) onward).
//!
//! Everything here is drawn from what the person typed, the driver's own counters, the planner's
//! task list and the directory the session runs in, and the links the person gave it. Nothing a
//! tool returned reaches it.

use crate::state::Session;
use crate::theme;
use bravebot_aichat::protocol::Cached;
use bravebot_i18n::t;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

/// Columns the panel takes, its border included.
pub const WIDTH: u16 = 36;

/// The fewest columns the panel can be dragged down to, border included.
pub const NARROWEST_PANEL: u16 = 24;

/// Columns the transcript keeps whatever the panel is dragged to.
const TRANSCRIPT_MINIMUM: u16 = 64;

/// A requested width held between the panel's minimum and what leaves the transcript its own on a
/// terminal `columns` wide.
pub fn clamped(width: u16, columns: u16) -> u16 {
    let most = columns
        .saturating_sub(TRANSCRIPT_MINIMUM)
        .max(NARROWEST_PANEL);
    width.clamp(NARROWEST_PANEL, most)
}

/// Columns at the left of each panel row that hold no text: the border and the space after it.
pub const INSET: u16 = 2;

/// The narrowest terminal the panel is drawn on, which leaves the transcript 64 columns.
pub const NARROWEST: u16 = 100;

/// Rows the session's name and a goal's condition may each take before they are cut.
const MOST_ROWS: usize = 3;

/// Whether a terminal this wide has room for the panel.
pub fn fits(columns: u16) -> bool {
    columns >= NARROWEST
}

/// Whether a frame this wide draws the panel: the person has it open and it fits.
pub fn drawn(session: &Session, columns: u16) -> bool {
    session.panel_open() && fits(columns)
}

fn dim() -> Style {
    Style::default().fg(theme::muted())
}

fn heading(text: String) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {text}"),
        Style::default()
            .fg(theme::brand_primary())
            .add_modifier(Modifier::BOLD),
    ))
}

fn row(text: String, style: Style) -> Line<'static> {
    Line::from(Span::styled(format!(" {text}"), style))
}

/// `text` wrapped to `width` columns over at most [`MOST_ROWS`] rows, the last cut with an ellipsis
/// where more was left.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let shown = crate::render::printable(&crate::status::cut(text, usize::MAX));
    if shown.is_empty() {
        return Vec::new();
    }
    let mut rows = crate::wrap::wrap(&shown, width, 0).rows;
    if rows.len() > MOST_ROWS {
        let rest = rows.split_off(MOST_ROWS - 1).join(" ");
        rows.push(crate::status::cut(&rest, width));
    }
    rows
}

/// `text` in one row of at most `width` columns, cut from the left with an ellipsis where it is
/// longer: the end of a directory and of a branch is the part that tells two of them apart.
pub(crate) fn ending(text: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    if crate::wrap::display_width(text) <= width {
        return text.to_string();
    }
    let mut kept = Vec::new();
    let mut used = 0;
    for c in text.chars().rev() {
        let wide = c.width().unwrap_or(0);
        if used + wide + 1 > width {
            break;
        }
        used += wide;
        kept.push(c);
    }
    std::iter::once('…').chain(kept.into_iter().rev()).collect()
}

/// One row of the Links section: what the link is, dim, and the link cut from the left to the rest
/// of the row, since the number at its end is what tells two apart.
///
/// A GitHub pull request or issue address is written as its shorthand, and the row comes with the
/// [`Link`] that makes the cells of the shorthand a link to the address, at row 0.
fn link_row(label: &str, url: &str, width: usize) -> (Line<'static>, Option<Link>) {
    let label_width = crate::wrap::display_width(label);
    let room = width.saturating_sub(label_width + 1);
    let (text, address) = match shorthand(url) {
        Some(short) => (short, Some(url)),
        None => (crate::render::printable(url), None),
    };
    let shown = ending(&text, room);
    let link = address.map(|url| Link {
        row: 0,
        // The space before the label and the one after it.
        column: label_width + 2,
        width: crate::wrap::display_width(&shown),
        url: url.to_string(),
    });
    let line = Line::from(vec![
        Span::styled(format!(" {label} "), dim()),
        Span::raw(shown),
    ]);
    (line, link)
}

/// `https://github.com/{owner}/{repo}/pull/{n}` or `.../issues/{n}` as `{owner}/{repo}#{n}`.
///
/// Each part may hold only the characters GitHub allows in it, so the address can go to the
/// terminal as a link with nothing in it able to end the escape sequence (PANEL-2).
fn shorthand(word: &str) -> Option<String> {
    let mut parts = word.strip_prefix("https://github.com/")?.split('/');
    let (owner, repo, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let made_of =
        |part: &str, allowed: fn(char) -> bool| !part.is_empty() && part.chars().all(allowed);
    let fits = parts.next().is_none()
        && matches!(kind, "pull" | "issues")
        && made_of(owner, |c| c.is_ascii_alphanumeric() || c == '-')
        && made_of(repo, |c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
        })
        && made_of(number, |c| c.is_ascii_digit());
    fits.then(|| format!("{owner}/{repo}#{number}"))
}

/// `name` with each GitHub pull request or issue address in it written as its shorthand, and each
/// shorthand beside the address it stands for.
fn shortened(name: &str) -> (String, Vec<(String, String)>) {
    let mut addresses = Vec::new();
    let words: Vec<String> = name
        .split(' ')
        .map(|word| match shorthand(word) {
            Some(short) => {
                addresses.push((short.clone(), word.to_string()));
                short
            }
            None => word.to_string(),
        })
        .collect();
    (words.join(" "), addresses)
}

/// The column of each place in `row` where `word` stands as a whole word.
fn columns_of<'a>(row: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    row.match_indices(word)
        .filter(move |(at, _)| {
            let (before, after) = (&row[..*at], &row[at + word.len()..]);
            (before.is_empty() || before.ends_with(' '))
                && (after.is_empty() || after.starts_with(' '))
        })
        .map(move |(at, _)| crate::wrap::display_width(&row[..at]))
}

/// Part of a panel row that links to `url`: the row, the column it starts at inside the border,
/// and how many columns it takes.
struct Link {
    row: usize,
    column: usize,
    width: usize,
    url: String,
}

fn section(body: &mut Vec<Line<'static>>, title: String, rows: Vec<Line<'static>>) {
    if !body.is_empty() {
        body.push(Line::raw(""));
    }
    body.push(heading(title));
    body.extend(rows);
}

/// The panel's rows, `height` of them, the last naming the key that hides it.
///
/// Each section is left out where it has nothing to say, but for the context, which always has a
/// reading. The plan is last because it is the one section that can be longer than the room, so it
/// is the one cut to what is left.
pub fn lines(session: &Session, width: u16, height: u16) -> Vec<Line<'static>> {
    laid_out(session, width, height).0
}

/// The screen rows of a panel drawn over `area` that continue on the row below, as the two halves
/// of the session id do. A copy joins them without a line break.
pub fn continued_rows(session: &Session, area: Rect) -> Vec<u16> {
    laid_out(session, area.width, area.height)
        .2
        .into_iter()
        .map(|row| area.y + row as u16)
        .collect()
}

/// Draw the panel over `area`, its border on the left, each shortened address a link to the
/// address it stands for.
pub fn draw(buffer: &mut Buffer, area: Rect, session: &Session) {
    let (rows, links, ..) = laid_out(session, area.width, area.height);
    Clear.render(area, buffer);
    Paragraph::new(rows)
        .block(Block::default().borders(Borders::LEFT).border_style(dim()))
        .render(area, buffer);
    for link in links {
        // One column for the border.
        crate::hyperlink::link(
            buffer,
            area.x + 1 + link.column as u16,
            area.y + link.row as u16,
            link.width as u16,
            &link.url,
        );
    }
}

/// The rows of the panel, where the links are, which rows continue on the row below, and which rows
/// a click acts on.
type Body = (Vec<Line<'static>>, Vec<Link>, Vec<usize>, Vec<Place>);

/// What a click on a row of the panel acts on (MOUSE-11, MOUSE-18, MOUSE-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The model row: the same question `/model` asks.
    Model,
    /// The effort row: the same question `/effort` asks.
    Effort,
    /// The context reading: the same question `/context` asks.
    Context,
    /// A started MCP server's row: the same question a bare `/mcp` asks.
    Servers,
    /// A file's row in the list of the files state: the highlight the keys move (MOUSE-18).
    File(usize),
    /// The `x` in the heading row of the files state: the end of the state, as `q` is (MOUSE-18).
    CloseFiles,
    /// The control for the previous edit in the bar of the files state: the key that goes to it.
    PreviousEdit,
    /// The control for the next edit in the bar of the files state.
    NextEdit,
    /// The heading of the Files section and the row under it: the same question `/diff` asks
    /// (MOUSE-20).
    Files,
}

/// Where a click acts on the panel: a row, and the cells of it that count, or all of them where
/// `columns` is `None`. Columns are counted from the first one inside the border.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub row: usize,
    pub columns: Option<std::ops::Range<usize>>,
    pub target: Target,
}

impl Place {
    /// A target that every cell of the row belongs to.
    pub fn on_row(row: usize, target: Target) -> Self {
        Place {
            row,
            columns: None,
            target,
        }
    }

    /// A target that the cells `columns` of the row belong to.
    pub fn on_cells(row: usize, columns: std::ops::Range<usize>, target: Target) -> Self {
        Place {
            row,
            columns: Some(columns),
            target,
        }
    }
}

/// The target a press on screen cell (`row`, `column`) of a panel drawn over `area` lands on, if any.
///
/// Read from the rows [`draw`] draws, so a row that is not drawn, or is cut for want of room, is
/// not a target. The border column is the panel's handle and is never one (PANEL-6).
pub fn target_at(session: &Session, area: Rect, row: u16, column: u16) -> Option<Target> {
    if column <= area.x || column >= area.right() || row < area.y {
        return None;
    }
    let at = usize::from(row - area.y);
    // The border is the first column of the area.
    let cell = usize::from(column - area.x - 1);
    laid_out(session, area.width, area.height)
        .3
        .into_iter()
        .find_map(|place| {
            let on_it = place.row == at
                && place
                    .columns
                    .as_ref()
                    .is_none_or(|columns| columns.contains(&cell));
            on_it.then_some(place.target)
        })
}

/// The rows [`lines`] gives, where among them the links are, which of them continue on the row
/// below, and which rows a click acts on.
fn laid_out(session: &Session, width: u16, height: u16) -> Body {
    let height = usize::from(height);
    if height == 0 {
        return (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    }
    // None of the ordinary sections is drawn beside the files state (FILES-4).
    if let Some(state) = session.files_state() {
        let files = session.written_files();
        let (rows, targets) = crate::files_mode::rows(
            &files,
            state,
            session.has_earlier_calls(),
            width,
            height as u16,
        );
        return (rows, Vec::new(), Vec::new(), targets);
    }
    let text = text_width(width);
    let (mut body, mut links, mut continued, mut targets) = head(session, text);

    let languages = session.language_servers().programs();
    if !languages.is_empty() {
        let rows = languages
            .iter()
            .map(|program| row((*program).to_string(), Style::default()))
            .collect();
        section(&mut body, t!(panel_language_servers).to_string(), rows);
    }
    if !session.servers.started.is_empty() {
        let rows: Vec<_> = session
            .servers
            .started
            .iter()
            .map(|alias| {
                row(
                    ending(&crate::render::printable(alias), text),
                    Style::default(),
                )
            })
            .collect();
        let count = rows.len();
        section(&mut body, t!(panel_mcp_servers).to_string(), rows);
        let first = body.len() - count;
        targets.extend((first..first + count).map(|row| Place::on_row(row, Target::Servers)));
    }

    // Left out until a file has been written, and ahead of the plan so that the plan takes the rows
    // it leaves (FILES-1).
    let (count, added, removed) = session.written_totals();
    if count > 0 {
        let mut total = vec![Span::raw(format!(
            " {}",
            t!(panel_files_total, count = count)
        ))];
        for (sign, count, colour) in [("+", added, theme::ok()), ("-", removed, theme::fail())] {
            if count > 0 {
                total.push(Span::styled(
                    format!(" {sign}{count}"),
                    Style::default().fg(colour),
                ));
            }
        }
        section(
            &mut body,
            t!(panel_files).to_string(),
            vec![Line::from(total)],
        );
        // The heading and the row under it, which are the last two rows so far.
        let totals = body.len() - 1;
        targets.extend([totals - 1, totals].map(|row| Place::on_row(row, Target::Files)));
    }

    let plan = session.plan();
    // The blank row and the heading come out of the room before the rows do.
    let room = (height - 1).saturating_sub(body.len() + 2);
    if !plan.is_empty() && room > 0 {
        let cut = cut_for(plan, room);
        section(
            &mut body,
            t!(panel_plan).to_string(),
            plan_rows(plan, &cut, !session.a_turn_is_running(), text),
        );
    }

    body.truncate(height - 1);
    body.resize(height - 1, Line::raw(""));
    body.push(row(
        t!(panel_hide, chord = session.bindings().panel_name()),
        dim(),
    ));
    links.retain(|link| link.row < height - 1);
    continued.retain(|row| *row < height - 1);
    targets.retain(|place| place.row < height - 1);
    (body, links, continued, targets)
}

/// Whether a frame `columns` by `rows` draws the panel with its Context section whole.
///
/// The hint line hands the context reading and the cache rate to the panel only then. On a terminal
/// too short for the sections above it, the panel cuts the Context section, and the line keeps
/// both rather than leave the reading on neither.
pub fn shows_context(session: &Session, columns: u16, rows: u16) -> bool {
    // The hint line takes one row and the key that hides the panel another. The files state has no
    // Context section, so the line keeps both figures there (FILES-4).
    drawn(session, columns)
        && !session.files_open()
        && head(session, text_width(session.panel_columns(columns)))
            .0
            .len()
            + 2
            <= usize::from(rows)
}

/// One column for the border and one for the space after it.
fn text_width(width: u16) -> usize {
    usize::from(width).saturating_sub(usize::from(INSET)).max(1)
}

/// The Session, Goal, Links and Context sections, each left out where it has nothing to say but for
/// the context, which always has a reading, where in them the links are, and which rows continue on
/// the row below.
fn head(session: &Session, text: usize) -> Body {
    let mut targets = Vec::new();
    let mut body: Vec<Line<'static>> = Vec::new();
    let mut links = Vec::new();
    let mut continued = Vec::new();
    let mut id_row = None;

    let identity = session.identity();
    let (name, addresses) = shortened(&crate::render::printable(&crate::status::cut(
        &identity.name,
        usize::MAX,
    )));
    let mut about = Vec::new();
    let mut name_links = Vec::new();
    for name_row in wrapped(&name, text) {
        for (short, url) in &addresses {
            for column in columns_of(&name_row, short) {
                name_links.push(Link {
                    row: about.len(),
                    // The space `row` puts before the text.
                    column: column + 1,
                    width: crate::wrap::display_width(short),
                    url: url.clone(),
                });
            }
        }
        about.push(row(name_row, Style::default()));
    }
    if !identity.directory.is_empty() {
        about.push(row(
            ending(&crate::render::printable(&identity.directory), text),
            dim(),
        ));
    }
    if let Some(branch) = &identity.branch {
        about.push(row(ending(&crate::render::printable(branch), text), dim()));
    }
    if let Some(id) = &identity.id {
        // Whole, since a lookup reads `{id}.json` and a fragment finds nothing, so split after the
        // last hyphen over two rows rather than cut.
        let id = crate::render::printable(id);
        let (first, second) = id.split_at(id.rfind('-').map_or(0, |at| at + 1));
        if !first.is_empty() && !second.is_empty() {
            id_row = Some(about.len());
        }
        for half in [first, second] {
            if !half.is_empty() {
                about.push(row(half.to_string(), dim()));
            }
        }
    }
    if !about.is_empty() {
        let count = about.len();
        section(&mut body, t!(panel_session).to_string(), about);
        let first = body.len() - count;
        continued.extend(id_row.map(|row| first + row));
        links.extend(name_links.into_iter().map(|link| Link {
            row: first + link.row,
            ..link
        }));
    }

    let mut model = Vec::new();
    // Where each row will land once the section is placed, which `section` decides.
    let mut model_targets = Vec::new();
    if let Some(chosen) = session.model_label() {
        model_targets.push((model.len(), Target::Model));
        model.push(row(
            ending(&crate::render::printable(chosen), text),
            Style::default(),
        ));
    }
    if let Some(level) = session.effort_in_force() {
        model_targets.push((model.len(), Target::Effort));
        model.push(row(t!(panel_effort, level = level.as_str()), dim()));
    }
    if !model.is_empty() {
        let count = model.len();
        section(&mut body, t!(panel_model).to_string(), model);
        let first = body.len() - count;
        targets.extend(
            model_targets
                .into_iter()
                .map(|(row, target)| Place::on_row(first + row, target)),
        );
    }

    if let Some(goal) = session.goal() {
        let rows = wrapped(goal.condition(), text)
            .into_iter()
            .map(|condition| row(condition, Style::default()))
            .chain(
                goal.is_paused()
                    .then(|| row(t!(panel_goal_paused).to_string(), dim())),
            )
            .collect();
        section(&mut body, t!(panel_goal).to_string(), rows);
    }

    let mut link_rows = Vec::new();
    let mut address_links = Vec::new();
    for (label, url) in [
        (t!(panel_pull_request), &identity.pull_request),
        (t!(panel_issue), &identity.issue),
    ] {
        let Some(url) = url.as_deref() else {
            continue;
        };
        let (line, link) = link_row(label, url, text);
        address_links.extend(link.map(|link| Link {
            row: link_rows.len(),
            ..link
        }));
        link_rows.push(line);
    }
    if !link_rows.is_empty() {
        let count = link_rows.len();
        section(&mut body, t!(panel_links).to_string(), link_rows);
        let first = body.len() - count;
        links.extend(address_links.into_iter().map(|link| Link {
            row: first + link.row,
            ..link
        }));
    }

    let mut context = vec![row(
        crate::render::context_reading(session),
        Style::default(),
    )];
    let spent = session.spent_tokens();
    if spent > 0 {
        context.push(row(
            t!(panel_spent, tokens = crate::status::tokens(spent)),
            dim(),
        ));
    }
    if let Some(rate) = crate::render::cache_hit_rate(session) {
        context.push(row(rate, dim()));
    }
    // A zero is a service that reported nothing as much as a cache that missed, so it draws nothing.
    if let Some(cached) = session.cached().filter(Cached::any) {
        if cached.read_tokens > 0 {
            context.push(row(
                t!(
                    panel_cache_read,
                    tokens = crate::status::tokens(cached.read_tokens)
                ),
                dim(),
            ));
        }
        if cached.written_tokens > 0 {
            context.push(row(
                t!(
                    panel_cache_written,
                    tokens = crate::status::tokens(cached.written_tokens)
                ),
                dim(),
            ));
        }
    }
    let count = context.len();
    section(&mut body, t!(panel_context).to_string(), context);
    targets.push(Place::on_row(body.len() - count, Target::Context));
    (body, links, continued, targets)
}

/// The mark of a row that no turn is working on and that was neither finished nor dropped.
///
/// Hollow where the mark of a row being worked on is filled, so the two differ under `NO_COLOR`
/// too, where the ink of a row is the terminal's own.
pub(crate) const LEFT_OPEN_MARKER: &str = "□";

/// Which rows of a plan that does not fit are drawn, and how many of each kind are not.
#[derive(Default)]
struct Cut {
    /// The positions in the plan of the rows drawn, in the plan's order.
    shown: Vec<usize>,
    /// Rows still to do or in progress that were left out.
    to_do: usize,
    done: usize,
    cancelled: usize,
}

/// The `capacity` rows of `plan` to draw, chosen from each row's status and from nothing else.
///
/// The rows still outstanding come first, the one in progress before the pending ones, each in the
/// plan's order. Where capacity is left over it goes to the finished rows, those nearest the first
/// outstanding row first and the earlier of two as near, or nearest the end of a plan with none, so
/// that what was last done is shown beside what comes next. The rows chosen are drawn in the plan's
/// order.
///
/// The text of a task is the planner's and decides nothing here. A status the driver does not
/// recognise reads as outstanding ([TODO-2](../../../docs/specs/tools/todo-write.md#TODO-2)), so
/// its row is drawn first and counted among those still to do.
fn cut_to(plan: &[bravebot_core::todo::Row], capacity: usize) -> Cut {
    use bravebot_core::todo::Status;
    let mut outstanding: Vec<usize> = (0..plan.len()).filter(|&at| !plan[at].struck()).collect();
    let anchor = outstanding.first().copied().unwrap_or(plan.len());
    // Stable, so each group keeps the plan's order.
    outstanding.sort_by_key(|&at| plan[at].status != Status::Active);
    outstanding.truncate(capacity);
    let spare = capacity - outstanding.len();
    let mut finished: Vec<usize> = (0..plan.len()).filter(|&at| plan[at].struck()).collect();
    finished.sort_by_key(|&at| (at.abs_diff(anchor), at));
    let mut shown = outstanding;
    shown.extend(finished.into_iter().take(spare));
    shown.sort_unstable();

    let mut cut = Cut {
        shown,
        ..Cut::default()
    };
    for (at, row) in plan.iter().enumerate() {
        if cut.shown.binary_search(&at).is_ok() {
            continue;
        }
        match row.status {
            Status::Pending | Status::Active => cut.to_do += 1,
            Status::Done => cut.done += 1,
            Status::Cancelled => cut.cancelled += 1,
        }
    }
    cut
}

/// The rows of `plan` drawn in `room` rows: all of them where they fit, and otherwise as many as the
/// room holds beside the row that counts the rest.
fn cut_for(plan: &[bravebot_core::todo::Row], room: usize) -> Cut {
    if plan.len() <= room {
        cut_to(plan, plan.len())
    } else {
        cut_to(plan, room - 1)
    }
}

/// The row that says what a plan left out, or `None` where it left nothing out.
///
/// Each kind is counted in words of its own, the rows still to do first, so a count of finished
/// work is not read as work to come. A row cut to the width of a narrow panel loses the finished
/// counts before it loses the unfinished one.
fn left_out_row(cut: &Cut, width: usize) -> Option<Line<'static>> {
    let mut counts = Vec::new();
    if cut.to_do > 0 {
        counts.push(t!(panel_hidden_to_do, count = cut.to_do));
    }
    if cut.done > 0 {
        counts.push(t!(panel_hidden_done, count = cut.done));
    }
    if cut.cancelled > 0 {
        counts.push(t!(panel_hidden_cancelled, count = cut.cancelled));
    }
    (!counts.is_empty()).then(|| row(crate::status::cut(&counts.join(", "), width), dim()))
}

/// The rows of `plan` that `cut` chose, and the row that says what it left out.
///
/// `left_open` is whether no turn is running: a row still outstanding is then drawn hollow and
/// dim, so that after a turn ends it does not read as work being done.
fn plan_rows(
    plan: &[bravebot_core::todo::Row],
    cut: &Cut,
    left_open: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = cut
        .shown
        .iter()
        .map(|&at| &plan[at])
        .map(|task| {
            let (glyph, marker, text) = if task.status == bravebot_core::todo::Status::Cancelled {
                (
                    task.marker,
                    dim(),
                    dim().add_modifier(Modifier::CROSSED_OUT),
                )
            } else if task.struck() {
                (
                    task.marker,
                    Style::default().fg(theme::ok()),
                    dim().add_modifier(Modifier::CROSSED_OUT),
                )
            } else if left_open {
                (LEFT_OPEN_MARKER, dim(), dim())
            } else {
                (
                    task.marker,
                    Style::default().fg(theme::running()),
                    Style::default(),
                )
            };
            let content = crate::status::cut(
                &crate::render::printable(&task.content),
                width.saturating_sub(2),
            );
            Line::from(vec![
                Span::styled(format!(" {glyph} "), marker),
                Span::styled(content, text),
            ])
        })
        .collect();
    lines.extend(left_out_row(cut, width));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::lsp::{Language, Roster};
    use bravebot_core::todo::{Item, List, Status};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// The frame drawn `width` by `height`, one string a row, with its layout recorded on the
    /// session the way the event loop records it after every draw.
    fn screen(session: &mut Session, width: u16, height: u16) -> Vec<String> {
        let buffer = frame(session, width, height);
        (0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    /// The cells the terminal was sent for a frame `width` by `height`.
    fn frame(session: &mut Session, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let mut laid = None;
        terminal
            .draw(|frame| laid = Some(crate::render::draw(frame, session)))
            .expect("draw succeeds");
        session.note_layout(laid.expect("the frame was drawn"));
        terminal.backend().buffer().clone()
    }

    /// What the panel's columns hold on each row but the hint line, the border left off.
    fn panel_column(rows: &[String], width: u16) -> Vec<String> {
        let start = usize::from(width - WIDTH) + 1;
        rows[..rows.len() - 1]
            .iter()
            .map(|row| {
                row.chars()
                    .skip(start)
                    .collect::<String>()
                    .trim()
                    .to_string()
            })
            .collect()
    }

    fn texts(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.to_string().trim().to_string())
            .collect()
    }

    fn opened_at(width: u16) -> Session {
        let mut session = Session::new("none");
        screen(&mut session, width, 30);
        session.toggle_panel();
        assert!(
            session.panel_open(),
            "the panel did not open at {width} columns"
        );
        session
    }

    /// MOUSE-11. Rejects: a target on the heading above the rows, one on the border column, one on
    /// a row cut for want of room, and an effort row for a model that reads none.
    #[test]
    fn only_the_drawn_model_and_effort_rows_are_click_targets() {
        let mut session = Session::new("none");
        session.choose_model("claude-sonnet".to_string(), &model_config());
        session.note_model_reads_effort(true);
        session.choose_effort(Some(bravebot_aichat::protocol::Effort::Xhigh));
        let area = Rect::new(84, 0, WIDTH, 40);
        let rows = lines(&session, area.width, area.height);
        let at = |needle: &str| {
            rows.iter()
                .position(|line| line.to_string().contains(needle))
                .expect(needle) as u16
        };
        let model = at("claude-sonnet");
        let effort = at(&t!(panel_effort, level = "xhigh").to_string());
        let inside = area.x + 3;

        assert_eq!(
            target_at(&session, area, area.y + model, inside),
            Some(Target::Model)
        );
        assert_eq!(
            target_at(&session, area, area.y + effort, inside),
            Some(Target::Effort)
        );
        assert_eq!(target_at(&session, area, area.y + model - 1, inside), None);
        assert_eq!(target_at(&session, area, area.y + model, area.x), None);
        // Cut to fewer rows than the model's, it is not drawn and so is not a target.
        let short = Rect::new(area.x, 0, WIDTH, model);
        assert_eq!(target_at(&session, short, short.y + model, inside), None);

        session.note_model_reads_effort(false);
        assert_eq!(target_at(&session, area, area.y + effort, inside), None);
    }

    /// MOUSE-12. Rejects: a target on the Context heading or on the rows below the reading, and
    /// one on a reading cut for want of room.
    #[test]
    fn only_the_context_reading_is_a_context_click_target() {
        let session = Session::new("none");
        let area = Rect::new(84, 0, WIDTH, 40);
        let rows = lines(&session, area.width, area.height);
        let reading = rows
            .iter()
            .position(|line| {
                line.to_string()
                    .contains(&crate::render::context_reading(&session))
            })
            .expect("reading") as u16;
        let inside = area.x + 3;

        assert_eq!(
            target_at(&session, area, area.y + reading, inside),
            Some(Target::Context)
        );
        assert_eq!(
            target_at(&session, area, area.y + reading - 1, inside),
            None
        );
        assert_eq!(
            target_at(&session, area, area.y + reading + 1, inside),
            None
        );
        let short = Rect::new(area.x, 0, WIDTH, reading);
        assert_eq!(target_at(&session, short, short.y + reading, inside), None);
    }

    /// MOUSE-13. Rejects: a target on the MCP heading or the rows around the servers, and one on
    /// a language server's row, which `/mcp` does not list.
    #[test]
    fn only_a_started_mcp_servers_rows_are_click_targets() {
        let mut session = Session::new("none");
        session.servers.started = vec!["notes".to_string(), "docs".to_string()];
        let area = Rect::new(84, 0, WIDTH, 40);
        let rows = lines(&session, area.width, area.height);
        let at = |needle: &str| {
            rows.iter()
                .position(|line| line.to_string().contains(needle))
                .expect(needle) as u16
        };
        let inside = area.x + 3;
        let target = |row: u16| target_at(&session, area, area.y + row, inside);

        let heading = at(t!(panel_mcp_servers));
        assert_eq!(target(heading), None);
        assert_eq!(target(at("notes")), Some(Target::Servers));
        assert_eq!(target(at("docs")), Some(Target::Servers));
        assert_eq!(target(at("notes").max(at("docs")) + 1), None);
        let short = Rect::new(area.x, 0, WIDTH, at("notes"));
        assert_eq!(
            target_at(&session, short, short.y + at("notes"), inside),
            None
        );
    }

    fn model_config() -> bravebot_config::Config {
        bravebot_config::Config::from_lookup(|key| match key {
            "SERVICES_KEY_AICHAT" => Some("a-signing-key".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("a-key-id".into()),
            "BRAVE_AI_CHAT_ENDPOINT" => Some("https://ai-chat.bsg.brave.com".into()),
            _ => None,
        })
        .expect("config")
    }

    fn plan(rows: &[(&str, Status)]) -> Vec<bravebot_core::todo::Row> {
        bravebot_core::todo::rows(&List::new(
            rows.iter()
                .map(|(content, status)| Item::new(*content, *status))
                .collect(),
        ))
    }

    fn working_on(rows: Vec<bravebot_core::todo::Row>) -> Session {
        let mut session = Session::new("none");
        session.type_char('a');
        session.submit();
        session.set_todos(rows);
        session
    }

    /// A hundred columns is where the transcript still has 64 beside the panel, and the panel is
    /// what gives way below that. Narrowing the terminal hides the panel without closing it, so
    /// widening it again brings the panel back without a second press.
    #[test]
    fn the_panel_is_drawn_from_a_hundred_columns_and_not_below() {
        let mut session = opened_at(100);
        let rows = screen(&mut session, 100, 30);
        let border = usize::from(100 - WIDTH);
        assert!(
            rows[..29]
                .iter()
                .all(|row| row.chars().nth(border) == Some('│')),
            "the panel's border is not on every row above the hint line: {rows:#?}"
        );
        assert!(
            panel_column(&rows, 100).contains(&t!(panel_context).to_string()),
            "the panel has no context section at 100 columns: {rows:#?}"
        );

        let narrow = screen(&mut session, 99, 30);
        assert!(
            !narrow
                .iter()
                .any(|row| row.contains('│') && row.contains(&t!(panel_context).to_string())),
            "the panel was drawn at 99 columns: {narrow:#?}"
        );
        assert!(
            session.panel_open(),
            "narrowing the terminal closed the panel"
        );

        let wide = screen(&mut session, 120, 30);
        assert!(
            panel_column(&wide, 120).contains(&t!(panel_context).to_string()),
            "widening the terminal did not bring the panel back: {wide:#?}"
        );
    }

    /// A press that opened a panel the screen cannot show would look as if it did nothing, and the
    /// next widening would bring back a panel nobody remembers opening. A press to close one is
    /// the opposite case: kept open, the panel the person pressed to be rid of comes back with the
    /// next widening.
    #[test]
    fn a_press_on_a_narrow_terminal_does_not_open_the_panel_and_does_close_it() {
        let mut session = Session::new("none");
        screen(&mut session, 99, 30);
        session.toggle_panel();
        assert!(
            !session.panel_open(),
            "the panel opened on a 99-column terminal"
        );
        let note = t!(panel_too_narrow, columns = NARROWEST).to_string();
        let rows = screen(&mut session, 99, 30);
        assert!(
            rows.iter().any(|row| row.contains(&note[..40])),
            "no note said why the panel did not open: {rows:#?}"
        );

        let mut open = opened_at(120);
        screen(&mut open, 99, 30);
        open.toggle_panel();
        assert!(
            !open.panel_open(),
            "a press on a narrowed terminal left the panel open"
        );
        let widened = screen(&mut open, 120, 30);
        assert!(
            !panel_column(&widened, 120)
                .iter()
                .any(|row| row.contains(&t!(panel_context).to_string())),
            "the panel came back on widening after a press closed it: {widened:#?}"
        );
    }

    /// The name comes from the first turn or a `/rename`, neither of which is a key the event loop
    /// would otherwise redraw for, so the loop draws again on what this returns.
    #[test]
    fn a_new_name_redraws_an_open_panel_and_nothing_else_does() {
        let mut closed = Session::new("none");
        assert!(
            !closed.identify("first", "~/b".to_string(), None),
            "a new name redrew a closed panel"
        );

        let mut open = opened_at(120);
        assert!(
            open.identify("first", "~/b".to_string(), None),
            "a new name did not redraw the open panel"
        );
        assert!(
            !open.identify("first", "~/b".to_string(), None),
            "an unchanged name redrew the panel"
        );
        assert!(
            open.identify("first", "~/b".to_string(), Some("main")),
            "a new branch did not redraw the open panel"
        );
        assert!(
            open.link(None, Some("https://x.test/p/2")),
            "a new link did not redraw the open panel"
        );
        assert!(
            !open.link(None, Some("https://x.test/p/2")),
            "an unchanged link redrew the panel"
        );
        assert!(
            !open.identify("first", "~/b".to_string(), Some("main")),
            "naming the session again redrew it, or dropped its link"
        );
    }

    /// The transcript and the input box wrap to the columns left beside the panel rather than to
    /// the terminal's. Laid out at the full width, a reply runs on under the panel, and a typed
    /// line is given one row where it needs two, so its start scrolls out of the box. Every word
    /// is distinct, so a word missing from the left columns names which of the two went wrong.
    #[test]
    fn the_transcript_and_the_input_wrap_to_the_columns_the_panel_leaves() {
        let mut session = opened_at(100);
        session.type_char('a');
        session.submit();
        let reply: Vec<String> = (1..=40).map(|n| format!("r{n}")).collect();
        session.complete(reply.join(" "), Vec::new(), 0);
        let typed: Vec<String> = (1..=24).map(|n| format!("t{n}")).collect();
        for c in typed.join(" ").chars() {
            session.type_char(c);
        }
        let rows = screen(&mut session, 100, 30);
        let left: Vec<String> = rows[..rows.len() - 1]
            .iter()
            .map(|row| row.chars().take(usize::from(100 - WIDTH)).collect())
            .collect();
        let words: Vec<&str> = left
            .iter()
            .flat_map(|row| row.split(|c: char| !c.is_alphanumeric()))
            .collect();
        for word in reply.iter().chain(&typed) {
            assert!(
                words.contains(&word.as_str()),
                "{word} is not whole left of the panel: {rows:#?}"
            );
        }
    }

    /// The row that hides the panel names whatever chord a settings file moved it to, or it
    /// would tell the person to press a key that does nothing.
    #[test]
    fn the_last_row_names_the_chord_in_force() {
        let mut session = Session::new("none");
        let mut moved = std::collections::BTreeMap::new();
        moved.insert("panel".to_string(), "alt-i".to_string());
        session.adopt_keybindings(&moved);
        screen(&mut session, 120, 30);
        session.toggle_panel();
        let column = panel_column(&screen(&mut session, 120, 30), 120);
        assert_eq!(
            column.last().map(String::as_str),
            Some(t!(panel_hide, chord = "alt-i").as_ref()),
            "the last row does not name the moved chord: {column:#?}"
        );
    }

    /// The hint names the panel only where pressing it would show one: open, it is already on
    /// screen, and on a narrow terminal the press only leaves a note.
    #[test]
    fn the_hint_line_offers_the_panel_only_while_it_is_closed_and_would_fit() {
        let info = t!(panel_hint, chord = "ctrl-x").to_string();
        let mut closed = Session::new("none");
        let wide = screen(&mut closed, 120, 30);
        assert!(
            wide[29].contains(&info),
            "a closed panel that would fit is not offered: {}",
            wide[29]
        );
        let narrow = screen(&mut closed, 99, 30);
        assert!(
            !narrow[29].contains(&info),
            "the panel is offered where it does not fit: {}",
            narrow[29]
        );
        let mut open = opened_at(120);
        let rows = screen(&mut open, 120, 30);
        assert!(
            !rows[29].contains(&info),
            "an open panel is still offered: {}",
            rows[29]
        );
    }

    /// The offer is the one part the hint line can lose without hiding anything: the loop and the
    /// jobs say something is spending, and the panel the offer names is one press away anyway.
    #[test]
    fn the_info_part_is_the_first_the_hint_line_gives_up() {
        let mut session = Session::new("kernel").starting_in_bypass();
        session.start_loop(
            crate::loops::request("2d check the deploy"),
            Vec::new(),
            Vec::new(),
        );
        session.complete("done", Vec::new(), 0);
        session.loop_turn_ended(None);
        let info = t!(panel_hint, chord = "ctrl-x").to_string();
        let full = screen(&mut session, 200, 30)[29].clone();
        let others: Vec<String> = full
            .trim()
            .split("  ·  ")
            .map(|part| part.trim().to_string())
            .filter(|part| *part != info)
            .collect();
        assert!(
            full.contains(&info),
            "the offer is missing at 200 columns: {full}"
        );

        let mut dropped_alone = false;
        for width in 100..200 {
            let hint = screen(&mut session, width, 30)[29].clone();
            let missing = others.iter().filter(|part| !hint.contains(*part)).count();
            if missing > 0 {
                assert!(
                    !hint.contains(&info),
                    "at {width} columns a part went before the offer did: {hint}"
                );
            }
            dropped_alone |= missing == 0 && !hint.contains(&info);
        }
        assert!(
            dropped_alone,
            "no width dropped the offer while keeping the rest, so the order was never tested"
        );
    }

    /// Each state of the reading moves, the unmeasured one included: a reading left out of both
    /// places would read as a session with nothing in its context.
    #[test]
    fn the_context_reading_moves_into_the_panel_in_each_state() {
        let mut measured = Session::new("none");
        measured.measured(50_000, 200_000, false);
        let mut compacted = Session::new("none");
        compacted.compacted(40_000, 200_000);
        for mut session in [Session::new("none"), measured, compacted] {
            let reading = crate::render::context_reading(&session);
            assert!(
                !reading.is_empty(),
                "a state of the reading is drawn as a blank"
            );
            let closed = screen(&mut session, 120, 30);
            assert!(
                closed[29].contains(&reading),
                "the hint line lost {reading:?} with the panel closed: {}",
                closed[29]
            );
            session.toggle_panel();
            let open = screen(&mut session, 120, 30);
            assert!(
                !open[29].contains(&reading),
                "the hint line kept {reading:?} with the panel drawn: {}",
                open[29]
            );
            assert!(
                panel_column(&open, 120).contains(&reading),
                "the panel does not show {reading:?}: {open:#?}"
            );
        }
    }

    /// A terminal too short for the sections above the Context section cuts it from the panel, and
    /// the reading moved out of the hint line would then be on neither.
    #[test]
    fn the_context_reading_stays_on_the_hint_line_where_the_panel_is_too_short_for_it() {
        let mut session = opened_at(100);
        session.identify(
            &"a very long name ".repeat(12),
            "~/b".to_string(),
            Some("main"),
        );
        let reading = crate::render::context_reading(&session);

        // Six rows of Session, a blank, and two of Context: the hint line and the key that hides
        // the panel need two rows more than that.
        let short = screen(&mut session, 100, 10);
        assert!(
            short[9].contains(&reading),
            "the hint line dropped {reading:?} for a panel with no room for it: {short:#?}"
        );
        let tall = screen(&mut session, 100, 11);
        assert!(
            !tall[10].contains(&reading) && panel_column(&tall, 100).contains(&reading),
            "the reading did not move into a panel with room for it: {tall:#?}"
        );
    }

    /// What was read out of the cache and what was written into it cost different amounts, so
    /// one figure for both would say nothing about either.
    #[test]
    fn the_cache_read_and_written_are_two_figures() {
        let mut session = Session::new("none");
        session.served_from_cache(
            bravebot_aichat::protocol::Cached {
                read_tokens: 12_000,
                written_tokens: 3_000,
            },
            20_000,
        );
        let rate = crate::render::cache_hit_rate(&session).expect("a hit rate");
        let closed = screen(&mut session, 120, 30);
        assert!(
            closed[29].contains(&rate),
            "no hit rate on the hint line: {}",
            closed[29]
        );

        session.toggle_panel();
        let rows = screen(&mut session, 120, 30);
        assert!(
            !rows[29].contains(&rate),
            "the hint line kept the hit rate: {}",
            rows[29]
        );
        let column = panel_column(&rows, 120);
        for expected in [
            rate,
            t!(panel_cache_read, tokens = crate::status::tokens(12_000)).to_string(),
            t!(panel_cache_written, tokens = crate::status::tokens(3_000)).to_string(),
        ] {
            assert!(
                column.contains(&expected),
                "{expected:?} is missing: {column:#?}"
            );
        }

        let none = texts(&lines(&Session::new("none"), WIDTH, 30));
        assert!(
            !none
                .iter()
                .any(|row| row.contains(&t!(panel_cache_read, tokens = "").to_string())),
            "a cache figure was drawn before any turn reported one: {none:#?}"
        );
    }

    fn cache_row_count(session: &Session) -> usize {
        texts(&lines(session, WIDTH, 30))
            .iter()
            .filter(|row| {
                row.contains(&t!(panel_cache_read, tokens = "").to_string())
                    || row.contains(&t!(panel_cache_written, tokens = "").to_string())
            })
            .count()
    }

    /// A zero says a service that reported nothing exactly as much as it says a missed cache, so
    /// it draws nothing, and a write-only turn draws the write alone.
    #[test]
    fn a_zero_cache_figure_is_not_drawn() {
        let mut session = Session::new("none");
        session.served_from_cache(Cached::default(), 20_000);
        assert_eq!(cache_row_count(&session), 0);

        session.served_from_cache(
            Cached {
                read_tokens: 0,
                written_tokens: 3_000,
            },
            20_000,
        );
        let rows = texts(&lines(&session, WIDTH, 30));
        let written = t!(panel_cache_written, tokens = crate::status::tokens(3_000)).to_string();
        assert!(rows.iter().any(|row| row.contains(&written)), "{rows:#?}");
        assert_eq!(cache_row_count(&session), 1, "{rows:#?}");
    }

    /// A turn stopped or failed before any request completed has reported nothing, so it neither
    /// draws figures in a session that has none nor leaves the ones an earlier turn had (BACKEND-31).
    #[test]
    fn a_turn_ended_before_any_request_reports_no_cache_figures() {
        let figures = Cached {
            read_tokens: 12_000,
            written_tokens: 3_000,
        };
        let ending = || bravebot_agent::Ending::Stopped { attempts: None };
        for fail in [false, true] {
            let end = |session: &mut Session| {
                if fail {
                    session.fail("boom", ending());
                } else {
                    session.stopped(None);
                }
            };
            let mut fresh = Session::new("none");
            end(&mut fresh);
            assert_eq!(fresh.cached(), None, "fail: {fail}");
            assert_eq!(fresh.cached_prompt_tokens(), None, "fail: {fail}");

            let mut earlier = Session::new("none");
            earlier.served_from_cache(figures, 20_000);
            end(&mut earlier);
            assert_eq!(earlier.cached(), None, "fail: {fail}");
            assert_eq!(earlier.cached_prompt_tokens(), None, "fail: {fail}");
            assert_eq!(cache_row_count(&earlier), 0, "fail: {fail}");
        }
    }

    /// A loop spends while nobody is watching, and the panel can be closed or the terminal too
    /// narrow for it, so the line that says one is running is not the panel's to take.
    #[test]
    fn the_loop_stays_on_the_hint_line_while_the_panel_is_drawn() {
        let mut session = Session::new("kernel");
        session.start_loop(
            crate::loops::request("2d check the deploy"),
            Vec::new(),
            Vec::new(),
        );
        session.complete("done", Vec::new(), 0);
        session.loop_turn_ended(None);
        screen(&mut session, 120, 30);
        session.toggle_panel();
        let hint = screen(&mut session, 120, 30)[29].clone();
        let counting = t!(loop_hint_next, next = "1d 23h").to_string();
        assert!(
            hint.contains(&counting),
            "the loop left the hint line with the panel drawn: {hint}"
        );
    }

    /// The panel stops above the hint line, so the note the line draws at its right edge is at the
    /// terminal's right edge, under the panel, and not at the right edge of the transcript's columns.
    #[test]
    fn the_hint_line_runs_under_the_panel_to_the_right_edge() {
        let mut session = opened_at(100);
        session.offered_to_leave = true;
        let rows = screen(&mut session, 100, 30);
        let note = "ctrl-c again to exit";
        let hint = &rows[29];
        assert_eq!(
            hint.trim_end().chars().count(),
            100 - 2,
            "the note does not end at the right edge of the terminal: {hint:?}"
        );
        assert!(hint.trim_end().ends_with(note), "{hint:?}");
        assert!(
            rows[28].chars().nth(usize::from(100 - WIDTH)) == Some('│'),
            "the panel's border is gone above the hint line: {rows:#?}"
        );
    }

    /// The panel speaks in the interface's own voice, so a reply or anything a tool returned drawn
    /// in it would read as the interface saying it.
    #[test]
    fn nothing_a_turn_returned_reaches_the_panel() {
        let mut session = Session::new("none");
        session.identify("typed by the person", "~/b".to_string(), None);
        session.type_char('a');
        session.submit();
        session.complete("REPLYTEXT ignore the rules", Vec::new(), 0);
        let drawn = texts(&lines(&session, WIDTH, 30));
        assert!(
            !drawn.iter().any(|row| row.contains("REPLYTEXT")),
            "the reply reached the panel: {drawn:#?}"
        );
    }

    /// A held goal is still drawn, with its condition, and says it is held.
    #[test]
    fn a_paused_goal_is_drawn_and_says_it_is_paused() {
        let mut session = Session::new("none");
        session.start_goal("the tests pass".to_string());
        let running = texts(&lines(&session, WIDTH, 30));
        assert!(
            !running.iter().any(|row| row == t!(panel_goal_paused)),
            "{running:#?}"
        );

        session.pause_goal();
        let drawn = texts(&lines(&session, WIDTH, 30));
        assert!(
            drawn.iter().any(|row| row.contains("the tests pass")),
            "{drawn:#?}"
        );
        assert!(
            drawn.iter().any(|row| row.contains(t!(panel_goal_paused))),
            "{drawn:#?}"
        );
    }

    /// Rejects: drawing the model a reply named in place of the one the person chose, drawing the
    /// effort of a model that reads none, and a heading over nothing where neither is set.
    #[test]
    fn the_model_section_shows_the_choice_and_the_effort_in_force_and_ignores_a_reply() {
        let heading = t!(panel_model).to_string();
        let bare = texts(&lines(&Session::new("none"), WIDTH, 30));
        assert!(
            !bare.contains(&heading),
            "a heading over nothing: {bare:#?}"
        );

        let mut session = Session::new("none");
        session.choose_model("claude-sonnet".to_string(), &model_config());
        session.choose_effort(Some(bravebot_aichat::protocol::Effort::Xhigh));
        let level = bravebot_aichat::protocol::Effort::Xhigh.as_str();
        let expected = vec![
            heading.clone(),
            "claude-sonnet".to_string(),
            t!(panel_effort, level = level).to_string(),
        ];
        let at = |drawn: &[String]| {
            let start = drawn.iter().position(|row| *row == heading);
            start.map(|start| drawn[start..start + 3].to_vec())
        };
        assert_eq!(
            at(&texts(&lines(&session, WIDTH, 30))),
            Some(expected.clone())
        );

        session.served("claude-sonnet", "qwen-14b-instruct", false, true);
        let after_a_reply = texts(&lines(&session, WIDTH, 30));
        assert_eq!(at(&after_a_reply), Some(expected));
        assert!(
            !after_a_reply.iter().any(|row| row.contains("qwen")),
            "the model a reply named was drawn: {after_a_reply:#?}"
        );

        session.note_model_reads_effort(false);
        let unread = texts(&lines(&session, WIDTH, 30));
        assert!(
            !unread.iter().any(|row| row.contains(level)),
            "an effort the model does not read was drawn: {unread:#?}"
        );
        assert!(unread.contains(&"claude-sonnet".to_string()), "{unread:#?}");
    }

    /// CHATGPT-10: on a model of the ChatGPT account the effort row is the level the account is
    /// sent, which is the one the session began with, and the level chosen since is drawn on no row.
    ///
    /// Rejects: the chosen level drawn as the one in force, which would tell the person a request
    /// carries a level it does not, and the row drawn from the session's choice by a panel that
    /// asks the choice itself and not the level in force.
    #[test]
    fn the_effort_row_of_a_chatgpt_session_is_the_level_it_was_begun_at() {
        use bravebot_aichat::protocol::Effort;
        let mut session = Session::new("none");
        session.choose_effort(Some(Effort::Low));
        session.begin_account_session();
        session.choose_effort(Some(Effort::High));
        session.note_model_is_account(true);

        let drawn = texts(&lines(&session, WIDTH, 30));

        assert!(
            drawn.contains(&t!(panel_effort, level = "low").to_string()),
            "the level the session began at was not drawn: {drawn:#?}"
        );
        assert!(
            !drawn.contains(&t!(panel_effort, level = "high").to_string()),
            "the level chosen since was drawn as the one in force: {drawn:#?}"
        );

        session.note_model_is_account(false);
        let elsewhere = texts(&lines(&session, WIDTH, 30));
        assert!(
            elsewhere.contains(&t!(panel_effort, level = "high").to_string()),
            "a model that is not the account's was not drawn at the level chosen: {elsewhere:#?}"
        );
    }

    /// PANEL-9. A tier set to an inference-profile ARN is drawn by the name the `/model` picker
    /// gave it.
    ///
    /// Rejects: the tail of the ARN (what cutting the id from the left leaves), one tier drawn by
    /// another's name, and a model no account offers drawn by anything but its request name.
    #[test]
    fn the_model_section_names_a_tier_rather_than_the_tail_of_its_arn() {
        let sonnet = "arn:aws:bedrock:us-west-2:1:application-inference-profile/pzko74tz7aq5";
        let haiku = "arn:aws:bedrock:us-west-2:1:application-inference-profile/h4ik0";
        let config = bravebot_config::Config::from_lookup(|key| match key {
            "BRAVEBOT_USE_BEDROCK" => Some("1".into()),
            "AWS_REGION" => Some("us-west-2".into()),
            "ANTHROPIC_DEFAULT_SONNET_MODEL" => Some(sonnet.into()),
            "ANTHROPIC_DEFAULT_HAIKU_MODEL" => Some(haiku.into()),
            _ => None,
        })
        .expect("config");
        let heading = t!(panel_model).to_string();
        let chosen = |session: &Session| {
            let drawn = texts(&lines(session, WIDTH, 30));
            let at = drawn.iter().position(|row| *row == heading);
            at.map(|at| drawn[at + 1].clone())
        };

        let mut session = Session::new("none").with_model_labels(&config);
        session.choose_model(sonnet.to_string(), &config);
        assert_eq!(chosen(&session).as_deref(), Some("Sonnet (Bedrock)"));
        session.choose_model(haiku.to_string(), &config);
        assert_eq!(chosen(&session).as_deref(), Some("Haiku (Bedrock)"));
        session.choose_model("claude-3-sonnet".to_string(), &config);
        assert_eq!(chosen(&session).as_deref(), Some("claude-3-sonnet"));

        // Without the labels a session draws the request name, as before.
        let mut bare = Session::new("none");
        bare.choose_model("claude-3-sonnet".to_string(), &config);
        assert_eq!(chosen(&bare).as_deref(), Some("claude-3-sonnet"));
    }

    /// Rejects: a total that leaves out the turn in flight (which `/cost` counts), one drawn
    /// before anything was spent, and one that is not the session's sum.
    #[test]
    fn the_token_total_is_a_context_row_that_sums_the_session() {
        let spent = |session: &Session| {
            texts(&lines(session, WIDTH, 30))
                .into_iter()
                .find(|row| row.ends_with(t!(panel_spent, tokens = "").trim()))
        };
        let mut session = Session::new("none");
        assert_eq!(spent(&session), None);

        session.end_run(1_500, None);
        session.end_run(2_000, None);
        assert_eq!(session.spent_tokens(), 3_500);
        assert_eq!(
            spent(&session),
            Some(t!(panel_spent, tokens = crate::status::tokens(3_500)).to_string())
        );

        let mut running = Session::new("none");
        running.end_run(1_000, None);
        running.type_char('a');
        running.submit();
        running.progressed(bravebot_agent::Spent {
            tokens: 4_200,
            ..Default::default()
        });
        assert!(running.a_turn_is_running(), "no turn was running");
        assert_eq!(
            spent(&running),
            Some(t!(panel_spent, tokens = crate::status::tokens(5_200)).to_string())
        );

        let drawn = texts(&lines(&session, WIDTH, 30));
        let context = drawn
            .iter()
            .position(|row| *row == t!(panel_context))
            .expect("the Context heading");
        assert!(
            drawn[context..]
                .iter()
                .any(|row| row.contains(&crate::status::tokens(3_500))),
            "the total is not under Context: {drawn:#?}"
        );
    }

    /// A heading over nothing tells somebody scanning the panel there is something to read.
    #[test]
    fn the_sections_come_in_order_and_an_empty_one_leaves_no_heading() {
        let headings = [
            t!(panel_session).to_string(),
            t!(panel_model).to_string(),
            t!(panel_goal).to_string(),
            t!(panel_links).to_string(),
            t!(panel_context).to_string(),
            t!(panel_language_servers).to_string(),
            t!(panel_mcp_servers).to_string(),
            t!(panel_plan).to_string(),
        ];
        let bare = texts(&lines(&Session::new("none"), WIDTH, 30));
        let shown: Vec<&String> = headings.iter().filter(|h| bare.contains(h)).collect();
        assert_eq!(
            shown,
            vec![&headings[4]],
            "headings over nothing: {bare:#?}"
        );

        let mut full = working_on(plan(&[("Write the panel", Status::Active)]));
        full.identify(
            "the info panel",
            "~/bravebot".to_string(),
            Some("fix-info-panel"),
        );
        full.choose_model("claude-sonnet".to_string(), &model_config());
        full.start_goal("the tests pass".to_string());
        full.link(Some("https://x.test/i/1"), None);
        full.report_language_servers(Roster::of([Language::Rust]));
        full.servers.started = vec!["notes".to_string()];
        let drawn = texts(&lines(&full, WIDTH, 30));
        let at: Vec<usize> = headings
            .iter()
            .map(|h| {
                drawn
                    .iter()
                    .position(|row| row == h)
                    .unwrap_or_else(|| panic!("no {h:?} heading: {drawn:#?}"))
            })
            .collect();
        assert!(
            at.windows(2).all(|pair| pair[0] < pair[1]),
            "out of order: {drawn:#?}"
        );
        for expected in [
            "the info panel",
            "~/bravebot",
            "fix-info-panel",
            "claude-sonnet",
            "the tests pass",
            "Issue https://x.test/i/1",
            "rust-analyzer",
            "notes",
        ] {
            assert!(
                drawn.iter().any(|row| row == expected),
                "{expected:?} missing: {drawn:#?}"
            );
        }
    }

    /// Each link has its row only once the person gave it, the pull request's first, and a session
    /// given neither has no heading for them.
    #[test]
    fn the_links_section_has_a_row_for_each_link_that_is_set() {
        let mut session = Session::new("none");
        let heading = t!(panel_links).to_string();
        assert!(
            !texts(&lines(&session, WIDTH, 30)).contains(&heading),
            "a Links heading over no links"
        );

        session.link(Some("https://x.test/i/1"), None);
        let drawn = texts(&lines(&session, WIDTH, 30));
        let at = drawn
            .iter()
            .position(|row| *row == heading)
            .unwrap_or_else(|| panic!("no Links heading: {drawn:#?}"));
        assert_eq!(drawn[at + 1], "Issue https://x.test/i/1");
        assert_eq!(drawn[at + 2], "", "a row for a link that is not set");

        session.link(Some("https://x.test/i/1"), Some("https://x.test/p/2"));
        let drawn = texts(&lines(&session, WIDTH, 30));
        assert_eq!(
            drawn[at + 1..at + 3],
            [
                "Pull request https://x.test/p/2",
                "Issue https://x.test/i/1"
            ],
            "{drawn:#?}"
        );
    }

    /// Two pull requests in one repository differ only in the number at the end, so a link cut like
    /// the name would draw them the same. The host is not GitHub's, so the address is not
    /// written as a shorthand.
    #[test]
    fn a_long_link_keeps_its_end() {
        let mut session = Session::new("none");
        session.link(
            None,
            Some("https://git.example.org/an-organisation/a-long-repository-name/pull/12345"),
        );
        let drawn = texts(&lines(&session, WIDTH, 30));
        let row = drawn
            .iter()
            .find(|row| row.starts_with("Pull request"))
            .unwrap_or_else(|| panic!("no pull request row: {drawn:#?}"));
        assert!(
            row.ends_with("/pull/12345") && row.contains('…'),
            "{row:?} is not cut from the left"
        );
        assert_eq!(
            row.chars().count(),
            usize::from(WIDTH) - 2,
            "{row:?} is not cut to the panel's width"
        );
    }

    /// The name is the one section that grows with what the person typed, so it is the one held
    /// to a size, and the ellipsis says the rest exists.
    #[test]
    fn a_long_name_takes_three_rows_and_ends_in_an_ellipsis() {
        let mut session = Session::new("none");
        session.identify(&"a very long name ".repeat(12), "~/b".to_string(), None);
        let drawn = texts(&lines(&session, WIDTH, 30));
        let heading = t!(panel_session).to_string();
        let first = drawn
            .iter()
            .position(|row| *row == heading)
            .expect("a session heading");
        let directory = drawn
            .iter()
            .position(|row| row == "~/b")
            .expect("the directory");
        assert_eq!(
            directory - first - 1,
            MOST_ROWS,
            "the name took other than three rows: {drawn:#?}"
        );
        assert!(
            drawn[directory - 1].ends_with('…'),
            "the cut row has no ellipsis: {drawn:#?}"
        );
    }

    /// The name rows under the Session heading, for a name that leaves the directory `~/b`.
    fn name_rows(session: &Session) -> Vec<String> {
        let drawn = texts(&lines(session, WIDTH, 30));
        let heading = t!(panel_session).to_string();
        let first = drawn
            .iter()
            .position(|row| *row == heading)
            .expect("a session heading");
        let directory = drawn
            .iter()
            .position(|row| row == "~/b")
            .expect("the directory");
        drawn[first + 1..directory].to_vec()
    }

    /// A session started on a pull request is usually named by a prompt holding its address,
    /// which wraps over two of the panel's rows and pushes the rest of the section down.
    #[test]
    fn a_github_address_in_the_name_is_drawn_as_its_shorthand() {
        let mut session = Session::new("none");
        for (name, shown) in [
            (
                "/pr-fix https://github.com/brave/bravebot/pull/1975",
                "/pr-fix brave/bravebot#1975",
            ),
            (
                "see https://github.com/a-b/c.d_e/issues/12 now",
                "see a-b/c.d_e#12 now",
            ),
        ] {
            session.identify(name, "~/b".to_string(), None);
            assert_eq!(name_rows(&session), [shown], "{name:?}");
        }
    }

    /// An address with more after the number, on another host, over plain http, or with a
    /// character GitHub does not allow is not a pull request or issue the shorthand can name.
    #[test]
    fn an_address_of_any_other_shape_is_drawn_as_written() {
        let mut session = Session::new("none");
        for name in [
            "https://github.com/a/b/pull/1/f",
            "https://x.test/a/b/pull/1",
            "http://github.com/a/b/pull/1",
            "https://github.com/a_b/c/pull/1",
            "https://github.com/a/b/commit/1",
            "https://github.com/a/b/pull/1a",
        ] {
            session.identify(name, "~/b".to_string(), None);
            assert_eq!(name_rows(&session), [name], "{name:?}");
        }
    }

    /// The shorthand stands for the address, so it has to open it, and only its own cells may:
    /// a link one column off, or over the words beside it, opens the pull request from text that
    /// does not name it.
    #[test]
    fn the_shorthand_links_to_the_address_and_nothing_else_does() {
        const ADDRESS: &str = "https://github.com/brave/bravebot/pull/1975";
        let mut session = opened_at(100);
        session.identify(
            &format!("/pr-fix {ADDRESS} and https://x.test/a/b/pull/1"),
            "~/b".to_string(),
            None,
        );
        let buffer = frame(&mut session, 100, 30);

        let mut linked = Vec::new();
        for y in 0..30 {
            for x in 0..100 {
                let symbol = buffer[(x, y)].symbol();
                let shown = crate::hyperlink::shown(symbol);
                if shown != symbol {
                    assert!(
                        symbol.starts_with(&format!("\x1b]8;;{ADDRESS}\x1b\\")),
                        "({x}, {y}) links elsewhere: {symbol:?}"
                    );
                    linked.push((x, y, shown.to_string()));
                }
            }
        }
        let y = linked
            .first()
            .map(|(_, y, _)| *y)
            .expect("nothing was linked");
        let row: Vec<&str> = (100 - WIDTH..100)
            .map(|x| crate::hyperlink::shown(buffer[(x, y)].symbol()))
            .collect();
        let at = (0..row.len())
            .find(|&i| row[i..].concat().starts_with("brave/"))
            .expect("the shorthand is on the row");
        let start = 100 - WIDTH + at as u16;
        assert_eq!(
            linked,
            (start..)
                .zip("brave/bravebot#1975".chars())
                .map(|(x, c)| (x, y, c.to_string()))
                .collect::<Vec<_>>(),
            "the link is not exactly the shorthand's cells: {row:?}"
        );
    }

    /// The rows under the Links heading.
    fn link_rows(session: &Session) -> Vec<String> {
        let drawn = texts(&lines(session, WIDTH, 30));
        let heading = t!(panel_links).to_string();
        let first = drawn
            .iter()
            .position(|row| *row == heading)
            .unwrap_or_else(|| panic!("no Links heading: {drawn:#?}"));
        drawn[first + 1..]
            .iter()
            .take_while(|row| !row.is_empty())
            .cloned()
            .collect()
    }

    /// Where `text` starts among the cells the terminal was sent for the panel drawn `columns`
    /// wide, as the column and the row.
    fn found_in_panel(buffer: &ratatui::buffer::Buffer, columns: u16, text: &str) -> (u16, u16) {
        (0..buffer.area.height)
            .find_map(|y| {
                let row: String = (columns - WIDTH..columns)
                    .map(|x| crate::hyperlink::shown(buffer[(x, y)].symbol()))
                    .collect();
                let at = row.find(text)?;
                Some((columns - WIDTH + row[..at].chars().count() as u16, y))
            })
            .unwrap_or_else(|| panic!("{text:?} is not in the panel"))
    }

    /// Each linked cell of `buffer`, top to bottom and left to right: where it is, the character it
    /// shows and the address it opens.
    fn linked_cells(buffer: &ratatui::buffer::Buffer) -> Vec<(u16, u16, String, String)> {
        let mut linked = Vec::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let symbol = buffer[(x, y)].symbol();
                let shown = crate::hyperlink::shown(symbol);
                if shown != symbol {
                    let (address, _) = symbol
                        .strip_prefix("\x1b]8;;")
                        .and_then(|rest| rest.split_once("\x1b\\"))
                        .expect("a linked cell opens with the address");
                    linked.push((x, y, shown.to_string(), address.to_string()));
                }
            }
        }
        linked
    }

    /// A pull request and an issue are given as the addresses GitHub shows, which cut from the left
    /// to the room beside the word leave the repository's name off.
    #[test]
    fn a_github_address_in_a_link_row_is_drawn_as_its_shorthand() {
        let mut session = Session::new("none");
        session.link(
            Some("https://github.com/brave/bravebot/issues/1267"),
            Some("https://github.com/brave/bravebot/pull/1975"),
        );
        assert_eq!(
            link_rows(&session),
            [
                "Pull request brave/bravebot#1975",
                "Issue brave/bravebot#1267"
            ]
        );
    }

    /// An address the shorthand cannot name is drawn as the person gave it, and is not a link, as
    /// in the name.
    #[test]
    fn a_link_row_of_any_other_shape_is_drawn_as_written_and_not_linked() {
        for address in [
            "http://github.com/a/b/pull/1",
            "https://x.test/a/b/pull/1",
            "https://github.com/a/b/pull",
        ] {
            let mut session = opened_at(100);
            session.link(Some(address), None);
            assert_eq!(link_rows(&session), [format!("Issue {address}")]);
            let buffer = frame(&mut session, 100, 30);
            assert_eq!(linked_cells(&buffer), [], "{address:?} was linked");
        }
    }

    /// The shorthand opens the address it stands for, so each row's cells have to open their own
    /// row's address: the pull request's number is not the issue's, and neither the word beside the
    /// shorthand nor the rest of the row may open anything.
    #[test]
    fn a_link_row_shorthand_links_to_its_own_address_and_nothing_else_does() {
        const PULL: &str = "https://github.com/brave/bravebot/pull/1975";
        const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
        let mut session = opened_at(100);
        session.link(Some(ISSUE), Some(PULL));
        let buffer = frame(&mut session, 100, 30);

        let mut expected = Vec::new();
        for (word, shorthand, address) in [
            ("Pull request", "brave/bravebot#1975", PULL),
            ("Issue", "brave/bravebot#1267", ISSUE),
        ] {
            let (x, y) = found_in_panel(&buffer, 100, &format!("{word} {shorthand}"));
            let start = x + word.len() as u16 + 1;
            expected.extend(
                shorthand
                    .chars()
                    .zip(start..)
                    .map(|(c, x)| (x, y, c.to_string(), address.to_string())),
            );
        }
        assert_eq!(linked_cells(&buffer), expected);
    }

    /// A shorthand longer than the room is cut from the left, as an address is, and the link
    /// covers the cells drawn, the ellipsis included, so the number at the end still opens it.
    #[test]
    fn a_long_shorthand_keeps_its_end_and_is_linked_as_drawn() {
        const PULL: &str = "https://github.com/an-organisation/a-long-repository-name/pull/12345";
        let mut session = opened_at(100);
        session.link(None, Some(PULL));
        let buffer = frame(&mut session, 100, 30);

        let drawn = "…epository-name#12345";
        let (x, y) = found_in_panel(&buffer, 100, &format!("Pull request {drawn}"));
        let start = x + "Pull request ".len() as u16;
        assert_eq!(
            linked_cells(&buffer),
            drawn
                .chars()
                .zip(start..)
                .map(|(c, x)| (x, y, c.to_string(), PULL.to_string()))
                .collect::<Vec<_>>()
        );
    }

    /// A row the panel is too short to draw is not drawn, and a link to its place would open the
    /// address from whatever the panel draws there instead.
    #[test]
    fn a_link_row_below_the_panels_height_is_not_linked() {
        let mut session = opened_at(100);
        session.link(None, Some("https://github.com/brave/bravebot/pull/1975"));
        let buffer = frame(&mut session, 100, 2);
        assert_eq!(linked_cells(&buffer), []);
    }

    const ID: &str = "0b3f6c1e-9a2d-4e57-8c41-5d7a2f90b3e8";

    /// A lookup reads `{id}.json`, so a fragment finds nothing: the id is drawn whole, over two
    /// rows split at a hyphen, and the two rows read back as the id.
    #[test]
    fn the_session_id_is_drawn_whole_over_two_rows() {
        let mut session = Session::new("none");
        session.identify("name", "~/b".to_string(), Some("main"));
        session.name_id(Some(ID));
        let drawn = texts(&lines(&session, WIDTH, 30));
        let at = drawn
            .iter()
            .position(|row| row == "0b3f6c1e-9a2d-4e57-8c41-")
            .unwrap_or_else(|| panic!("no first half of the id: {drawn:#?}"));
        assert_eq!(drawn[at + 1], "5d7a2f90b3e8", "{drawn:#?}");
        assert_eq!(
            drawn[at - 1],
            "main",
            "the id is not the section's last row"
        );
        assert_eq!(format!("{}{}", drawn[at], drawn[at + 1]), ID);
    }

    /// A session with no record has no id to resume by, so the row is absent, and it follows the
    /// handle: it appears with the first save and goes when the handle has none.
    #[test]
    fn the_id_row_follows_whether_the_session_has_a_record() {
        let mut open = opened_at(120);
        let shown = |session: &Session| {
            texts(&lines(session, WIDTH, 30))
                .iter()
                .any(|row| row.starts_with("0b3f6c1e"))
        };
        open.identify("name", "~/b".to_string(), None);
        assert!(!shown(&open), "an id row before any record");
        assert!(open.name_id(Some(ID)), "the first save did not redraw");
        assert!(shown(&open));
        assert!(!open.name_id(Some(ID)), "an unchanged id redrew the panel");
        assert!(open.name_id(Some("11111111-2222-4333-8444-555555555555")));
        assert!(!shown(&open), "the old id outlived a change of session");
        assert!(open.name_id(None));
        assert!(
            !texts(&lines(&open, WIDTH, 30))
                .iter()
                .any(|r| r.starts_with("11111111"))
        );
    }

    /// The two id rows lengthen the Session section, and the hint line keeps the context reading
    /// until the panel can show its Context section whole.
    #[test]
    fn the_id_rows_count_toward_where_the_context_section_fits() {
        let mut session = opened_at(120);
        session.identify("name", "~/b".to_string(), None);
        let bare = (1..40)
            .find(|rows| shows_context(&session, 120, *rows))
            .expect("fits somewhere");
        session.name_id(Some(ID));
        let with_id = (1..40)
            .find(|rows| shows_context(&session, 120, *rows))
            .expect("fits somewhere");
        assert_eq!(with_id, bare + 2);
        for rows in [with_id - 1, with_id] {
            let drawn = panel_column(&screen(&mut session, 120, rows), 120);
            assert_eq!(
                drawn.contains(&crate::render::context_reading(&session)),
                shows_context(&session, 120, rows),
                "shows_context disagrees with the rows drawn at {rows}: {drawn:#?}"
            );
        }
    }

    /// A branch is whatever a checkout was given, and an escape in it drawn raw would act on the
    /// terminal the panel is drawn in.
    #[test]
    fn control_characters_in_the_session_section_are_drawn_as_pictures() {
        let mut session = Session::new("none");
        session.identify(
            "name\u{1b}[2J",
            "~/dir\u{7}".to_string(),
            Some("main\u{1b}]0;owned\u{7}"),
        );
        let drawn = texts(&lines(&session, WIDTH, 30));
        assert!(
            !drawn.iter().any(|row| row.chars().any(char::is_control)),
            "a control character reached the panel: {drawn:#?}"
        );
        assert!(
            drawn.iter().any(|row| row.starts_with("main")),
            "the branch was dropped rather than drawn: {drawn:#?}"
        );
    }

    /// Two checkouts of one project, or two branches off one prefix, differ at the end, so a row
    /// cut like the name would draw them the same.
    #[test]
    fn a_long_directory_and_branch_keep_their_ends() {
        let mut session = Session::new("none");
        session.identify(
            "name",
            format!("~/{}checkout-two", "deep/".repeat(8)),
            Some(&format!("{}fix-two", "feature/".repeat(5))),
        );
        let drawn = texts(&lines(&session, WIDTH, 30));
        let text = usize::from(WIDTH) - 2;
        for end in ["checkout-two", "fix-two"] {
            let row = drawn
                .iter()
                .find(|row| row.ends_with(end))
                .unwrap_or_else(|| panic!("no row keeps {end:?}: {drawn:#?}"));
            assert!(
                row.starts_with('…') && row.chars().count() == text,
                "{row:?} is not cut from the left to the panel's {text} columns"
            );
        }
    }

    /// A session that has asked nothing of a language has no server, and a heading over nothing
    /// would say one is expected. The programs are the table's names, one row each, in name order.
    #[test]
    fn each_started_language_server_has_a_row_and_none_leaves_no_heading() {
        let heading = t!(panel_language_servers).to_string();
        let mut session = working_on(plan(&[("Write the panel", Status::Active)]));
        assert!(
            !texts(&lines(&session, WIDTH, 30)).contains(&heading),
            "a language servers heading over nothing"
        );

        session.report_language_servers(Roster::of([Language::Python, Language::Rust]));
        let drawn = texts(&lines(&session, WIDTH, 30));
        let at = drawn
            .iter()
            .position(|row| *row == heading)
            .unwrap_or_else(|| panic!("no language servers heading: {drawn:#?}"));
        assert_eq!(
            drawn[at + 1..at + 3],
            ["pyright-langserver", "rust-analyzer"],
            "{drawn:#?}"
        );
        let context = drawn
            .iter()
            .position(|row| *row == *t!(panel_context))
            .expect("a context heading");
        let planned = drawn
            .iter()
            .position(|row| *row == *t!(panel_plan))
            .expect("a plan heading");
        assert!(context < at && at < planned, "out of order: {drawn:#?}");
    }

    /// The servers are the plan's neighbours in a column the plan fills last, so a panel too short
    /// for both gives the plan the rows the servers left rather than the other way about.
    #[test]
    fn the_plan_gets_the_rows_the_server_sections_leave() {
        let tasks: Vec<(String, Status)> = (0..20)
            .map(|n| (format!("task {n}"), Status::Pending))
            .collect();
        let rows: Vec<(&str, Status)> = tasks.iter().map(|(t, s)| (t.as_str(), *s)).collect();
        let mut session = working_on(plan(&rows));
        let without = texts(&lines(&session, WIDTH, 20));
        session.report_language_servers(Roster::of([Language::Rust]));
        session.servers.started = vec!["notes".to_string()];
        let with = texts(&lines(&session, WIDTH, 20));
        let shown = |drawn: &[String]| drawn.iter().filter(|row| row.contains("task ")).count();
        assert_eq!(with.len(), 20, "the panel is not the height it was given");
        assert_eq!(
            shown(&without) - shown(&with),
            6,
            "two headings, two rows and two blank rows came out of the plan's room: {with:#?}"
        );
        assert_eq!(
            with.last().map(String::as_str),
            Some(
                t!(panel_hide, chord = session.bindings().panel_name())
                    .to_string()
                    .as_str()
            ),
            "the last row is not the key that hides the panel"
        );
    }

    /// A session with the plan `rows` and two files written, one of which lost a line.
    fn with_two_files_written(rows: &[(&str, Status)]) -> Session {
        let mut session = working_on(plan(rows));
        for (path, before, after) in [("a.rs", "x\n", "x\ny\nz\n"), ("b.rs", "p\nq\n", "p\n")] {
            let diff = bravebot_agent::diff::Diff::compute(before, after);
            session.transcript.push(crate::state::Entry::tool(
                bravebot_agent::report::Activity::running("Update", path)
                    .of_tool("edit_file")
                    .with_changes(diff.condensed(3))
                    .with_written(Some(bravebot_agent::report::Written {
                        landed: path.into(),
                        added: diff.added(),
                        removed: diff.removed(),
                        numbers: diff.condensed_numbers(3),
                    }))
                    .done("done"),
            ));
        }
        session
    }

    /// FILES-1, PANEL-9: the Files section is left out until a file has been written, and then has
    /// the totals and no row of its own beyond them. It comes out of the plan's room, as a server
    /// section does.
    #[test]
    fn the_files_section_has_the_totals_and_leaves_the_plan_its_rows() {
        let tasks: Vec<(String, Status)> = (0..20)
            .map(|n| (format!("task {n}"), Status::Pending))
            .collect();
        let rows: Vec<(&str, Status)> = tasks.iter().map(|(t, s)| (t.as_str(), *s)).collect();
        let without_files = working_on(plan(&rows));
        let without = texts(&lines(&without_files, WIDTH, 20));
        assert!(
            !without.iter().any(|row| *row == *t!(panel_files)),
            "a Files section was drawn before anything was written: {without:#?}"
        );

        let session = with_two_files_written(&rows);
        let with = texts(&lines(&session, WIDTH, 20));
        let heading = with
            .iter()
            .position(|row| *row == *t!(panel_files))
            .unwrap_or_else(|| panic!("no Files heading: {with:#?}"));
        assert_eq!(with[heading + 1].trim(), "2 files +2 -1", "{with:#?}");
        assert_eq!(
            with[heading + 2].trim(),
            "",
            "a row follows the totals: {with:#?}"
        );
        assert!(
            !with.iter().any(|row| row.contains("/diff")),
            "the section names the command: {with:#?}"
        );
        let plan_heading = with
            .iter()
            .position(|row| *row == *t!(panel_plan))
            .unwrap_or_else(|| panic!("the plan was dropped: {with:#?}"));
        assert_eq!(
            plan_heading,
            heading + 3,
            "the plan does not follow the section"
        );
        let shown = |drawn: &[String]| drawn.iter().filter(|row| row.contains("task ")).count();
        assert_eq!(with.len(), 20, "the panel is not the height it was given");
        assert_eq!(
            shown(&without) - shown(&with),
            3,
            "a blank row, a heading and a row came out of the plan's room: {with:#?}"
        );
    }

    /// MOUSE-20. Rejects: a target on the rows under the section, on the plan, or on the border
    /// column, and a section whose heading or totals row is not one.
    #[test]
    fn only_the_files_heading_and_its_totals_row_are_click_targets() {
        let tasks: Vec<(String, Status)> = (0..6)
            .map(|n| (format!("task {n}"), Status::Pending))
            .collect();
        let rows: Vec<(&str, Status)> = tasks.iter().map(|(t, s)| (t.as_str(), *s)).collect();
        let session = with_two_files_written(&rows);
        let area = Rect::new(84, 0, WIDTH, 30);
        let drawn = texts(&lines(&session, area.width, area.height));
        let heading = drawn
            .iter()
            .position(|row| *row == *t!(panel_files))
            .expect("the Files heading") as u16;
        let inside = area.x + 3;
        let at = |row: u16, column: u16| target_at(&session, area, area.y + row, column);

        assert_eq!(at(heading, inside), Some(Target::Files));
        assert_eq!(at(heading + 1, inside), Some(Target::Files));
        assert_eq!(at(heading - 1, inside), None, "{drawn:#?}");
        assert_eq!(at(heading + 2, inside), None, "{drawn:#?}");
        assert_eq!(at(heading + 3, inside), None, "the plan's heading");
        assert_eq!(at(heading + 4, inside), None, "a row of the plan");
        assert_eq!(
            at(heading, area.x),
            None,
            "the border column is the resize handle"
        );

        // A heading or a totals row that the height of the panel cuts is not a target. The last row
        // of the panel names the key that hides it, so a panel `n` rows tall draws `n - 1`.
        let target = |height: u16, row: u16| {
            let area = Rect::new(84, 0, WIDTH, height);
            target_at(&session, area, row, inside)
        };
        assert_eq!(target(heading + 2, heading), Some(Target::Files));
        assert_eq!(
            target(heading + 2, heading + 1),
            None,
            "the totals row is cut"
        );
        assert_eq!(target(heading + 1, heading), None, "the heading is cut");

        // A session that has written nothing has no section to click.
        let bare = working_on(plan(&rows));
        for row in 0..area.height {
            assert_ne!(
                target_at(&bare, area, area.y + row, inside),
                Some(Target::Files),
                "row {row}"
            );
        }
    }

    /// An MCP server's alias is whatever the settings file called it, so it is drawn as every other
    /// typed string is, with its control characters pictured and a long one cut from the left.
    #[test]
    fn an_mcp_alias_is_drawn_pictured_and_cut_from_the_left() {
        let mut session = Session::new("none");
        session.servers.started =
            vec!["a\u{1b}[2Jb".to_string(), format!("{}tail", "x".repeat(60))];
        let drawn = texts(&lines(&session, WIDTH, 30));
        let heading = drawn
            .iter()
            .position(|row| *row == *t!(panel_mcp_servers))
            .unwrap_or_else(|| panic!("no MCP servers heading: {drawn:#?}"));
        assert_eq!(drawn[heading + 1], "a\u{241b}[2Jb", "{drawn:#?}");
        assert!(
            drawn[heading + 2].starts_with('…') && drawn[heading + 2].ends_with("tail"),
            "{drawn:#?}"
        );
        assert!(
            drawn.iter().all(|row| !row.contains('\u{1b}')),
            "an escape reached the panel: {drawn:#?}"
        );
    }

    /// The plan is most use between turns, when it says where the work was left, and `/clear`
    /// starts a conversation that has no plan yet.
    #[test]
    fn the_plan_stays_after_the_turn_and_clear_empties_it() {
        let mut session = working_on(plan(&[
            ("Write the panel", Status::Done),
            ("Test the panel", Status::Active),
        ]));
        session.complete("done", Vec::new(), 0);
        let after = texts(&lines(&session, WIDTH, 30));
        assert!(
            after.iter().any(|row| row.ends_with("Test the panel")),
            "the plan went with the turn: {after:#?}"
        );

        session.type_char('b');
        session.submit();
        session.set_todos(plan(&[("Ship the panel", Status::Active)]));
        let next = texts(&lines(&session, WIDTH, 30));
        assert!(
            next.iter().any(|row| row.ends_with("Ship the panel"))
                && !next.iter().any(|row| row.ends_with("Test the panel")),
            "the next report did not replace the plan: {next:#?}"
        );

        session.complete("done", Vec::new(), 0);
        session.clear();
        let cleared = texts(&lines(&session, WIDTH, 30));
        assert!(
            !cleared.contains(&t!(panel_plan).to_string()),
            "/clear left a plan: {cleared:#?}"
        );
    }

    /// The session record cannot tell a turn that emptied its list from one that kept none, so a
    /// report with no rows must leave the plan as the report before it in the turn set it, not
    /// fall back to an earlier turn's plan or to nothing.
    #[test]
    fn an_empty_report_after_one_with_rows_keeps_that_plan() {
        let shown = |session: &Session| texts(&lines(session, WIDTH, 30));
        let mut session = working_on(plan(&[("Earlier turn", Status::Done)]));
        session.complete("done", Vec::new(), 0);

        session.type_char('b');
        session.submit();
        session.set_todos(plan(&[("Later turn", Status::Active)]));
        session.set_todos(Vec::new());
        let during = shown(&session);
        assert!(
            during.iter().any(|row| row.ends_with("Later turn"))
                && !during.iter().any(|row| row.ends_with("Earlier turn")),
            "the empty report changed the plan mid-turn: {during:#?}"
        );

        session.complete("done", Vec::new(), 0);
        let after = shown(&session);
        assert!(
            after.iter().any(|row| row.ends_with("Later turn"))
                && !after.iter().any(|row| row.ends_with("Earlier turn")),
            "the plan fell back once the turn ended: {after:#?}"
        );

        let mut first = working_on(plan(&[("Only plan", Status::Active)]));
        first.set_todos(Vec::new());
        first.complete("done", Vec::new(), 0);
        assert!(
            shown(&first).iter().any(|row| row.ends_with("Only plan")),
            "the plan went when its turn was the first"
        );
    }

    /// TODO-3 in the panel, which draws the plan a second time and from its own branch. The
    /// transcript test beside this one draws neither, so without this the panel could show a
    /// cancelled task with the finished tick and nothing would fail.
    ///
    /// Asserted against a done row in the same plan rather than against a colour named here: what
    /// matters is that the two are told apart, and pinning the literal colour would fail on a
    /// theme change that kept them distinct.
    #[test]
    fn a_cancelled_task_in_the_panel_is_struck_and_marked_apart_from_a_done_one() {
        let drawn = lines(
            &working_on(plan(&[
                ("finished it", Status::Done),
                ("dropped it", Status::Cancelled),
                ("still to do", Status::Pending),
            ])),
            WIDTH,
            14,
        );
        let row_for = |needle: &str| -> Line<'static> {
            drawn
                .iter()
                .find(|line| line.to_string().contains(needle))
                .unwrap_or_else(|| panic!("no panel row holds {needle}: {:#?}", texts(&drawn)))
                .clone()
        };

        let done = row_for("finished it");
        let cancelled = row_for("dropped it");
        let pending = row_for("still to do");

        // Struck through, as a done row is: the task is over either way.
        for (name, line) in [("done", &done), ("cancelled", &cancelled)] {
            assert!(
                line.spans[1]
                    .style
                    .add_modifier
                    .contains(Modifier::CROSSED_OUT),
                "the {name} row's text is not struck through: {:?}",
                line.spans[1].style
            );
        }
        assert!(
            !pending.spans[1]
                .style
                .add_modifier
                .contains(Modifier::CROSSED_OUT),
            "a task still to do was struck through: {:?}",
            pending.spans[1].style
        );

        // And told apart at the marker, which is the whole point: a cancelled task is not a win.
        assert_ne!(
            cancelled.spans[0].style.fg, done.spans[0].style.fg,
            "a cancelled task's marker is the same colour as a finished one's, so a dropped step \
             reads as a completed one"
        );
        assert_ne!(
            cancelled.spans[0].content, done.spans[0].content,
            "a cancelled task carries the finished marker"
        );
    }

    /// The rows under the plan's heading, which is the last section, and nothing of the other
    /// sections, whose figures move from one turn to the next.
    fn plan_part(session: &Session, rows: usize) -> Vec<Line<'static>> {
        let all = lines(session, WIDTH, 24);
        let heading = texts(&all)
            .iter()
            .position(|row| *row == t!(panel_plan))
            .expect("a plan heading");
        let part: Vec<Line<'static>> = all[heading + 1..]
            .iter()
            .take_while(|line| !line.to_string().trim().is_empty())
            .cloned()
            .collect();
        assert_eq!(
            part.len(),
            rows,
            "the plan is not drawn whole: {:#?}",
            texts(&all)
        );
        part
    }

    fn plan_row(part: &[Line<'static>], needle: &str) -> Line<'static> {
        part.iter()
            .find(|line| line.to_string().contains(needle))
            .unwrap_or_else(|| panic!("no plan row holds {needle}: {:#?}", texts(part)))
            .clone()
    }

    const MIXED: [(&str, Status); 4] = [
        ("finished it", Status::Done),
        ("dropped it", Status::Cancelled),
        ("working on it", Status::Active),
        ("still to do", Status::Pending),
    ];

    /// A row still open after the turn that wrote the list has ended reads as work nobody is
    /// doing, which is what the answer beside it says. The mark has to differ as well as the ink,
    /// since `NO_COLOR` leaves the ink the terminal's own.
    ///
    /// Asserted against the same rows drawn while their turn runs, rather than against a colour
    /// named here, so a theme change that keeps the two apart does not fail it. Finished and
    /// dropped rows are drawn as they were, and a turn that starts is working on the plan again
    /// before it has reported a list.
    #[test]
    fn a_row_a_turn_left_open_is_drawn_apart_from_work_being_done() {
        let _held = crate::theme::exclusive();
        let mut session = working_on(plan(&MIXED));
        let live = plan_part(&session, 4);
        session.complete("done", Vec::new(), 0);
        let left = plan_part(&session, 4);

        for needle in ["working on it", "still to do"] {
            let (before, after) = (plan_row(&live, needle), plan_row(&left, needle));
            assert_ne!(
                after.spans[0].content, before.spans[0].content,
                "{needle} keeps the mark of work being done after its turn ended"
            );
            assert_ne!(
                after.spans[0].style.fg, before.spans[0].style.fg,
                "{needle} keeps the ink of work being done after its turn ended"
            );
            assert_ne!(
                after.spans[1].style, before.spans[1].style,
                "{needle} keeps the text of work being done after its turn ended"
            );
            assert!(
                !after.spans[1]
                    .style
                    .add_modifier
                    .contains(Modifier::CROSSED_OUT),
                "{needle} is struck through, so it reads as no longer outstanding"
            );
        }
        for needle in ["finished it", "dropped it"] {
            assert_eq!(
                plan_row(&left, needle),
                plan_row(&live, needle),
                "{needle} was drawn differently once the turn ended"
            );
        }

        // The next turn has reported nothing yet, and it is the one that may be working on them.
        session.type_char('b');
        session.submit();
        assert!(session.a_turn_is_running(), "no turn was running");
        assert_eq!(
            plan_part(&session, 4),
            live,
            "the rows are not drawn as work being done while a turn runs"
        );
    }

    /// A compaction or an aside shares the working status with a turn and does no work on the
    /// plan, so the rows it leaves outstanding stay left open while it runs.
    #[test]
    fn a_working_session_that_is_not_in_a_turn_draws_open_rows_as_left_open() {
        let _held = crate::theme::exclusive();
        let mut session = working_on(plan(&MIXED));
        let live = plan_part(&session, 4);
        session.complete("done", Vec::new(), 0);
        let left = plan_part(&session, 4);

        session.begin_aside();
        assert!(!session.a_turn_is_running(), "the aside is a turn");
        assert_eq!(
            plan_part(&session, 4),
            left,
            "an aside drew the open rows as work being done"
        );
        assert_ne!(
            left, live,
            "the fixture draws a turn's rows and its absence alike"
        );
    }

    /// A plan of `Status` runs, each row's text its place in the plan: `plan_of(&[(Done, 2),
    /// (Pending, 1)])` is `task 1` and `task 2` done and `task 3` pending.
    fn plan_of(runs: &[(Status, usize)]) -> Vec<(String, Status)> {
        runs.iter()
            .flat_map(|(status, count)| std::iter::repeat_n(*status, *count))
            .enumerate()
            .map(|(at, status)| (format!("task {}", at + 1), status))
            .collect()
    }

    /// The panel drawn `height` rows tall for a session whose plan is `rows`, which a turn is
    /// working on.
    fn panel_for(rows: &[(String, Status)], height: u16) -> Vec<String> {
        let borrowed: Vec<(&str, Status)> = rows
            .iter()
            .map(|(text, status)| (text.as_str(), *status))
            .collect();
        let drawn = texts(&lines(&working_on(plan(&borrowed)), WIDTH, height));
        assert_eq!(
            drawn.len(),
            usize::from(height),
            "the panel is not the height it was given: {drawn:#?}"
        );
        drawn
    }

    /// The numbers of the tasks a drawn panel holds, in the order it draws them.
    fn tasks_in(drawn: &[String]) -> Vec<usize> {
        drawn
            .iter()
            .filter_map(|row| {
                row.find("task ")
                    .map(|at| row[at + 5..].parse().expect("a number"))
            })
            .collect()
    }

    /// The row that says what the plan left out, which is the only row of the panel that begins
    /// with a plus.
    fn left_out_in(drawn: &[String]) -> Option<&str> {
        drawn
            .iter()
            .map(String::as_str)
            .find(|row| row.starts_with('+'))
    }

    /// A panel of fourteen rows leaves the plan nine: the context section takes two, the blank and
    /// the heading two more, and the key that hides the panel one. Eight of the nine are tasks when
    /// the plan does not fit, since the ninth says what was left out.
    const HEIGHT_FOR_EIGHT_TASKS: u16 = 14;

    /// A plan whose unfinished rows are at its end must show them. In a plan of eight rows done and
    /// two pending, a panel with room for fewer than ten rows has to draw both pending rows and say
    /// how many finished ones it left out.
    ///
    /// Rejects: a window that starts at the first row, or at the task in progress with the first
    /// row as the fallback, which draws only finished rows.
    #[test]
    fn a_long_plan_shows_the_rows_still_to_do_before_the_finished_ones() {
        let rows = plan_of(&[(Status::Done, 8), (Status::Pending, 2)]);

        // Five rows of room: the two pending rows, two finished rows, and the row that counts.
        let drawn = panel_for(&rows, 10);
        assert_eq!(tasks_in(&drawn), [7, 8, 9, 10], "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), Some("+6 done"), "{drawn:#?}");

        // Three rows of room, which is fewer than the two pending rows and the count are.
        let drawn = panel_for(&rows, 8);
        assert_eq!(tasks_in(&drawn), [9, 10], "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), Some("+8 done"), "{drawn:#?}");

        // Room for the whole plan draws it, and counts nothing.
        let drawn = panel_for(&rows, 30);
        assert_eq!(tasks_in(&drawn), (1..=10).collect::<Vec<_>>(), "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), None, "{drawn:#?}");
    }

    /// The rows shown stay in the order of the plan, not the order they were chosen in. The plan
    /// has a pending row ahead of the task in progress, and the choice takes the task in progress
    /// first.
    ///
    /// Rejects: drawing the rows in the order they are chosen, which puts the task in progress
    /// above the pending row that comes first in the plan.
    #[test]
    fn the_rows_shown_keep_the_order_of_the_plan() {
        let mut rows = plan_of(&[(Status::Done, 12)]);
        for (at, status) in [
            (0, Status::Pending),
            (2, Status::Active),
            (3, Status::Pending),
            (7, Status::Pending),
        ] {
            rows[at].1 = status;
        }

        let drawn = panel_for(&rows, HEIGHT_FOR_EIGHT_TASKS - 4);
        assert_eq!(tasks_in(&drawn), [1, 3, 4, 8], "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), Some("+8 done"), "{drawn:#?}");
    }

    /// Where there are more rows still to do than room, the task in progress is kept and the
    /// pending rows after it are the ones left out, and the row says they are still to do.
    ///
    /// Rejects: taking the first rows that are not finished, which drops a task in progress that
    /// comes after enough pending rows, and counting what it left out as `done`.
    #[test]
    fn the_task_in_progress_is_kept_when_there_are_more_rows_still_to_do_than_room() {
        let rows = plan_of(&[
            (Status::Done, 3),
            (Status::Pending, 5),
            (Status::Active, 1),
            (Status::Pending, 5),
        ]);

        let drawn = panel_for(&rows, HEIGHT_FOR_EIGHT_TASKS - 5);
        // Three rows: the task in progress and the first two pending rows.
        assert_eq!(tasks_in(&drawn), [4, 5, 9], "{drawn:#?}");
        assert_eq!(
            left_out_in(&drawn),
            Some(
                format!(
                    "{}, {}",
                    t!(panel_hidden_to_do, count = 8),
                    t!(panel_hidden_done, count = 3)
                )
                .as_str()
            ),
            "{drawn:#?}"
        );
    }

    /// Each kind of row the panel leaves out is counted in words of its own, the rows still to do
    /// first, so that a figure for finished work is not read as work to come and a dropped row is
    /// not counted as finished.
    ///
    /// Rejects: one count of every row left out, which says nothing of what they were, and a count
    /// that adds the cancelled rows to the done ones.
    #[test]
    fn the_row_that_counts_what_was_left_out_names_each_kind_in_words_of_its_own() {
        let said = |to_do: usize, done: usize, cancelled: usize| {
            let mut parts = Vec::new();
            if to_do > 0 {
                parts.push(t!(panel_hidden_to_do, count = to_do));
            }
            if done > 0 {
                parts.push(t!(panel_hidden_done, count = done));
            }
            if cancelled > 0 {
                parts.push(t!(panel_hidden_cancelled, count = cancelled));
            }
            parts.join(", ")
        };

        // Only finished rows are left out.
        let rows = plan_of(&[(Status::Done, 9), (Status::Active, 1), (Status::Pending, 2)]);
        let drawn = panel_for(&rows, HEIGHT_FOR_EIGHT_TASKS);
        assert_eq!(
            left_out_in(&drawn),
            Some(said(0, 4, 0).as_str()),
            "{drawn:#?}"
        );

        // Cancelled rows are told apart from done ones.
        let rows = plan_of(&[
            (Status::Cancelled, 4),
            (Status::Done, 1),
            (Status::Active, 1),
            (Status::Pending, 7),
        ]);
        let drawn = panel_for(&rows, HEIGHT_FOR_EIGHT_TASKS);
        assert_eq!(tasks_in(&drawn), (6..=13).collect::<Vec<_>>(), "{drawn:#?}");
        assert_eq!(
            left_out_in(&drawn),
            Some(said(0, 1, 4).as_str()),
            "{drawn:#?}"
        );

        // All three kinds, with the rows still to do first.
        let rows = plan_of(&[
            (Status::Done, 3),
            (Status::Cancelled, 2),
            (Status::Active, 1),
            (Status::Pending, 6),
        ]);
        let drawn = panel_for(&rows, HEIGHT_FOR_EIGHT_TASKS - 4);
        assert_eq!(tasks_in(&drawn), [6, 7, 8, 9], "{drawn:#?}");
        assert_eq!(
            left_out_in(&drawn),
            Some(said(3, 3, 2).as_str()),
            "{drawn:#?}"
        );
        assert!(
            said(3, 3, 2).find("to do") < said(3, 3, 2).find("done"),
            "the rows still to do are not counted first"
        );

        // A panel too narrow for all three cuts the finished counts and keeps the first.
        let narrow = lines(
            &working_on(plan(
                &rows
                    .iter()
                    .map(|(text, status)| (text.as_str(), *status))
                    .collect::<Vec<_>>(),
            )),
            NARROWEST_PANEL,
            HEIGHT_FOR_EIGHT_TASKS - 4,
        );
        let narrow = texts(&narrow);
        let counted = left_out_in(&narrow).expect("a row that counts");
        assert!(
            counted.starts_with(&t!(panel_hidden_to_do, count = 3).to_string())
                && crate::wrap::display_width(counted) <= usize::from(NARROWEST_PANEL) - 2,
            "{counted:?}"
        );
    }

    /// Rows left over after the rows still to do go to the finished rows nearest the first of
    /// them, drawn in the plan's order, so what was last done is beside what comes next. Of two
    /// rows as near, the earlier is taken first. A plan with nothing left to do shows its last
    /// rows.
    ///
    /// Rejects: filling from the top of the plan, or from the end, a tie that goes to the later
    /// row, and a plan with nothing left to do drawn from its first row.
    #[test]
    fn room_left_over_goes_to_the_finished_rows_nearest_the_first_one_still_to_do() {
        let mut rows = plan_of(&[(Status::Done, 12)]);
        rows[6].1 = Status::Active;
        rows[7].1 = Status::Pending;

        // Five rows of room hold four tasks beside the row that counts: the two rows to do and
        // two finished ones. The row before them is nearest. Rows 5 and 9 are two places away and
        // one is left, so the tie shows: the earlier.
        let drawn = panel_for(&rows, 10);
        assert_eq!(tasks_in(&drawn), [5, 6, 7, 8], "{drawn:#?}");

        // One row more takes the later of the two as well.
        let drawn = panel_for(&rows, 11);
        assert_eq!(tasks_in(&drawn), [5, 6, 7, 8, 9], "{drawn:#?}");

        let finished = plan_of(&[(Status::Done, 20)]);
        let drawn = panel_for(&finished, HEIGHT_FOR_EIGHT_TASKS);
        assert_eq!(
            tasks_in(&drawn),
            (13..=20).collect::<Vec<_>>(),
            "{drawn:#?}"
        );
        assert_eq!(left_out_in(&drawn), Some("+12 done"), "{drawn:#?}");
    }

    /// The panel draws a plan of any size: none at all, one row, twelve rows, fifty rows, and the
    /// fewest rows that leave the plan any room.
    ///
    /// Rejects: a heading over no rows, a count row on a plan that fits, a plan longer than the
    /// room that overruns the panel or pushes off the key that hides it, and a panel with no room
    /// for the plan that draws the heading anyway.
    #[test]
    fn the_panel_draws_a_plan_of_none_one_twelve_and_fifty_rows_and_at_the_least_height() {
        let heading = t!(panel_plan).to_string();
        let hide =
            |session: &Session| t!(panel_hide, chord = session.bindings().panel_name()).to_string();
        let session = Session::new("none");

        // No plan: no heading.
        let drawn = texts(&lines(&session, WIDTH, 30));
        assert!(!drawn.contains(&heading), "{drawn:#?}");

        // One row: the heading, the row and nothing counted.
        let drawn = panel_for(&plan_of(&[(Status::Active, 1)]), 14);
        assert!(drawn.contains(&heading), "{drawn:#?}");
        assert_eq!(tasks_in(&drawn), [1], "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), None, "{drawn:#?}");

        // Twelve rows that do not fit.
        let twelve = plan_of(&[(Status::Done, 6), (Status::Active, 1), (Status::Pending, 5)]);
        let drawn = panel_for(&twelve, HEIGHT_FOR_EIGHT_TASKS);
        assert_eq!(tasks_in(&drawn), (5..=12).collect::<Vec<_>>(), "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), Some("+4 done"), "{drawn:#?}");
        // And twelve that do.
        let drawn = panel_for(&twelve, 30);
        assert_eq!(tasks_in(&drawn), (1..=12).collect::<Vec<_>>(), "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), None, "{drawn:#?}");
        // A plan that fills the room to the last row is drawn whole, with no row to count.
        let nine = plan_of(&[(Status::Done, 7), (Status::Active, 1), (Status::Pending, 1)]);
        let drawn = panel_for(&nine, HEIGHT_FOR_EIGHT_TASKS);
        assert_eq!(tasks_in(&drawn), (1..=9).collect::<Vec<_>>(), "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), None, "{drawn:#?}");

        // Fifty rows, nine of them to do after the task in progress, on a panel that holds eight.
        let fifty = plan_of(&[
            (Status::Done, 40),
            (Status::Active, 1),
            (Status::Pending, 9),
        ]);
        let drawn = panel_for(&fifty, HEIGHT_FOR_EIGHT_TASKS);
        assert_eq!(
            tasks_in(&drawn),
            [41, 42, 43, 44, 45, 46, 47, 48],
            "{drawn:#?}"
        );
        assert_eq!(
            left_out_in(&drawn),
            Some(
                format!(
                    "{}, {}",
                    t!(panel_hidden_to_do, count = 2),
                    t!(panel_hidden_done, count = 40)
                )
                .as_str()
            ),
            "{drawn:#?}"
        );
        assert_eq!(drawn.last(), Some(&hide(&session)), "{drawn:#?}");

        // The least height with room for the plan: its heading and one row, which says what the
        // plan holds since there is no room for any of its rows.
        let drawn = panel_for(&fifty, 6);
        assert_eq!(tasks_in(&drawn), Vec::<usize>::new(), "{drawn:#?}");
        let at = drawn
            .iter()
            .position(|row| *row == heading)
            .expect("a heading");
        assert_eq!(
            drawn[at + 1],
            format!(
                "{}, {}",
                t!(panel_hidden_to_do, count = 10),
                t!(panel_hidden_done, count = 40)
            ),
            "{drawn:#?}"
        );
        // One row fewer leaves the plan none, and it is not drawn at all.
        let drawn = panel_for(&fifty, 5);
        assert!(!drawn.contains(&heading), "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), None, "{drawn:#?}");
        for height in [1, 2] {
            let drawn = panel_for(&fifty, height);
            assert!(!drawn.contains(&heading), "{drawn:#?}");
        }
    }

    /// Which rows are drawn and what is counted depend on each row's status and never on its text.
    /// The text of every row here says the opposite of the status it carries.
    ///
    /// Rejects: a choice or a count made from the words or the mark in a task's text, which the
    /// planner writes and a page it read may have dictated.
    #[test]
    fn the_rows_drawn_and_counted_follow_the_status_and_never_the_text() {
        let claiming = [
            ("\u{2713} completed", Status::Pending),
            ("done:", Status::Pending),
            ("pending", Status::Done),
            ("TODO: not started", Status::Done),
            ("in_progress now", Status::Cancelled),
            ("\u{25a0}", Status::Done),
            ("cancelled", Status::Active),
            ("completed", Status::Pending),
            ("[x] done", Status::Pending),
            ("pending", Status::Done),
            ("in progress", Status::Done),
            ("(pending)", Status::Cancelled),
        ];
        let rows: Vec<(String, Status)> = claiming
            .iter()
            .enumerate()
            .map(|(at, (claim, status))| (format!("{claim} task {}", at + 1), *status))
            .collect();

        // Rows 1, 2, 7, 8 and 9 are still to do, and the three finished rows nearest the first of
        // them are rows 3, 4 and 5. The rows left out are 6, 10 and 11, done, and 12, cancelled.
        let drawn = panel_for(&rows, HEIGHT_FOR_EIGHT_TASKS);
        assert_eq!(tasks_in(&drawn), [1, 2, 3, 4, 5, 7, 8, 9], "{drawn:#?}");
        assert_eq!(
            left_out_in(&drawn),
            Some(
                format!(
                    "{}, {}",
                    t!(panel_hidden_done, count = 3),
                    t!(panel_hidden_cancelled, count = 1)
                )
                .as_str()
            ),
            "{drawn:#?}"
        );
    }

    /// The rows `/todos` leaves under its heading in the transcript laid out `width` columns wide,
    /// as the transcript lays them out and not as the rows were built.
    fn printed(session: &mut Session, width: u16) -> Vec<String> {
        let plan = session.plan().to_vec();
        let counted = |status| plan.iter().filter(|task| task.status == status).count();
        let (done, cancelled) = (counted(Status::Done), counted(Status::Cancelled));
        // The cancelled tasks are counted apart, as the planner's own result counts them.
        let heading = if cancelled == 0 {
            t!(todos_heading, done = done, total = plan.len())
        } else {
            t!(
                todos_heading_cancelled,
                done = done,
                total = plan.len(),
                cancelled = cancelled
            )
        };
        session.report_plan();
        session.note_layout(crate::state::Laid {
            width,
            height: 40,
            ..crate::state::Laid::default()
        });
        let drawn: Vec<String> = crate::render::as_text(session)
            .lines()
            .map(str::to_string)
            .collect();
        let at = drawn
            .iter()
            .rposition(|row| row.trim() == heading)
            .unwrap_or_else(|| panic!("no heading {heading:?}: {drawn:#?}"));
        drawn[at + 1..].to_vec()
    }

    /// One task of a printed plan as its rows say it.
    #[derive(Debug, PartialEq)]
    struct PrintedTask {
        /// The mark in the margin of its first row.
        mark: char,
        /// What the margin says beside the mark, which may be cut, or nothing.
        status: String,
        /// The text of its rows with the margin taken off and the spaces and breaks taken out, so
        /// a break anywhere in the text, between words or inside one, leaves the same string.
        text: String,
    }

    /// The tasks a plan printed `width` columns wide says. A row whose margin is blank belongs to
    /// the task above it, and one whose margin holds anything begins a task.
    fn printed_tasks(rows: &[String], width: u16) -> Vec<PrintedTask> {
        let margin = crate::render::printed_margin(usize::from(width));
        let mut tasks: Vec<PrintedTask> = Vec::new();
        for row in rows.iter().filter(|row| !row.trim().is_empty()) {
            let before: String = row.chars().take(margin).collect();
            let after: String = row.chars().skip(margin).collect();
            let text: String = after.split_whitespace().collect();
            if before.trim().is_empty() {
                tasks.last_mut().expect("a first row").text.push_str(&text);
                continue;
            }
            let mut beside = before.trim_start().chars();
            tasks.push(PrintedTask {
                mark: beside.next().expect("a mark"),
                status: beside.collect::<String>().trim().to_string(),
                text,
            });
        }
        tasks
    }

    /// The status word and the text of each task of a plan printed with its margin whole.
    fn tasks_printed(rows: &[String]) -> Vec<(String, String)> {
        let whole = crate::render::printed_margin(usize::MAX) + 2;
        printed_tasks(rows, u16::try_from(whole).expect("a width"))
            .into_iter()
            .map(|task| (task.status, task.text))
            .collect()
    }

    /// The status word a printed plan says for `status`.
    fn status_in_words(status: Status) -> &'static str {
        match status {
            Status::Done => t!(todos_status_done),
            Status::Active => t!(todos_status_active),
            Status::Pending => t!(todos_status_pending),
            Status::Cancelled => t!(todos_status_cancelled),
        }
    }

    /// A printed plan wide enough for its whole margin draws every task: in the plan's order, with
    /// its mark, its status in words and its text wrapped to the width and not cut. The long word
    /// in task `n` is `12 + 3n` columns, so at every width tried some task has a word wider than
    /// the room for text, and the test asserts that, so it breaks inside the word.
    ///
    /// Rejects: a printed plan that keeps only the rows the panel has room for, that cuts a long
    /// task to the width as the panel does, that leaves a status out or names another, and that
    /// loses text where a word is wider than the row.
    #[test]
    fn the_whole_plan_is_printed_with_every_task_its_status_and_none_of_its_text_cut() {
        let tasks: Vec<(String, Status)> = (1..=50)
            .map(|n| {
                (
                    format!(
                        "task {n} is written to be longer than any row it is drawn in, \
                         and stays whole: unbroken{}word",
                        "x".repeat(3 * n)
                    ),
                    [
                        Status::Done,
                        Status::Active,
                        Status::Pending,
                        Status::Cancelled,
                    ][n % 4],
                )
            })
            .collect();
        let borrowed: Vec<(&str, Status)> = tasks
            .iter()
            .map(|(text, status)| (text.as_str(), *status))
            .collect();
        let longest_word = |text: &str| {
            text.split_whitespace()
                .map(crate::wrap::display_width)
                .max()
                .unwrap_or(0)
        };
        // The margin is whole from the width that leaves the text its two columns.
        let whole = crate::render::printed_margin(usize::MAX) + 2;
        for width in [whole, whole + 1, 40, 80, 120] {
            let room = width - crate::render::printed_margin(width);
            assert!(
                tasks.iter().any(|(text, _)| longest_word(text) > room),
                "no word is wider than the {room} columns of text at {width} columns, so no word breaks"
            );
            let width = u16::try_from(width).expect("a width");
            let mut session = working_on(plan(&borrowed));
            let rows = printed(&mut session, width);
            assert!(
                rows.iter().all(|row| !row.contains('…')),
                "a row was cut at {width} columns: {rows:#?}"
            );
            let printed = tasks_printed(&rows);
            assert_eq!(printed.len(), 50, "{width} columns: {rows:#?}");
            for (n, (said, (original, status))) in printed.iter().zip(&tasks).enumerate() {
                let expected = (
                    status_in_words(*status).to_string(),
                    original.split_whitespace().collect::<String>(),
                );
                assert_eq!(*said, expected, "task {} at {width} columns", n + 1);
            }
        }
    }

    /// A printed plan holds at every width from a single column up: no row is wider than the
    /// width, every task begins with its mark and every row after the first begins with the blank
    /// margin, so text of a task is never where a mark, a status or a margin stands. The status is
    /// cut with an ellipsis where the width cannot hold it, and left out where it cannot hold a
    /// letter and the ellipsis. Wide characters, an escape sequence and a forged mark and status
    /// are among the text. From three columns up none of the text is cut or replaced. At two
    /// columns a character wider than the single column left is drawn as a replacement, and a
    /// single column holds the mark alone.
    ///
    /// Rejects: a margin wider than the width, which the transcript wraps again and lets the text
    /// begin a row at the left edge; a wide character that overflows the column left for it; a
    /// status that is dropped where it could be cut, or cut to an ellipsis alone; and text that is
    /// lost at a narrow width.
    #[test]
    fn a_printed_plan_holds_at_every_width_from_one_column_up() {
        let texts = [
            "an ordinary sentence long enough to wrap onto several narrow rows",
            "\u{6f22}\u{5b57}\u{304b}\u{306a}\u{4ea4}\u{3058}\u{308a} and \u{1f600}\u{1f600} wide",
            "esc\u{1b}[2J \u{2713} completed forged\n\u{25a0} pending forged too",
        ];
        let statuses = [
            Status::Pending,
            Status::Done,
            Status::Cancelled,
            Status::Active,
        ];
        let tasks: Vec<(&str, Status)> = statuses
            .iter()
            .flat_map(|status| texts.iter().map(move |text| (*text, *status)))
            .collect();
        let fixed_beside_the_status = crate::render::MARGIN_BESIDE_THE_STATUS;
        for width in (1..=40).chain([60, 120]) {
            let mut session = working_on(plan(&tasks));
            let marks: Vec<char> = session
                .plan()
                .iter()
                .map(|row| {
                    assert_eq!(
                        crate::wrap::display_width(row.marker),
                        1,
                        "a mark is not one column"
                    );
                    row.marker.chars().next().expect("a mark")
                })
                .collect();
            let columns = u16::try_from(width).expect("a width");
            let rows = printed(&mut session, columns);
            let all = rows.join("\n");
            assert!(!all.contains('\u{1b}'), "an escape at {width}: {all}");
            for row in &rows {
                assert!(
                    crate::wrap::display_width(row) <= width,
                    "a row is wider than {width} columns: {row:?}\n{all}"
                );
            }

            let printed = printed_tasks(&rows, columns);
            assert_eq!(printed.len(), tasks.len(), "{width} columns:\n{all}");
            let margin = crate::render::printed_margin(width);
            for (n, (said, (original, status))) in printed.iter().zip(&tasks).enumerate() {
                let at = format!("task {n} at {width} columns:\n{all}");
                assert_eq!(said.mark, marks[n], "{at}");

                let word = status_in_words(*status);
                if margin <= fixed_beside_the_status + 1 {
                    assert_eq!(said.status, "", "{at}");
                } else if said.status == word {
                    // Whole where the margin holds it.
                } else {
                    let cut = said.status.strip_suffix('…').unwrap_or(&said.status);
                    assert!(
                        !cut.is_empty() && word.starts_with(cut) && said.status.ends_with('…'),
                        "the status is neither whole nor cut: {at}"
                    );
                }

                let shown: String = crate::render::printable(original)
                    .split_whitespace()
                    .collect();
                let expected: String = match width {
                    1 => String::new(),
                    2 => shown
                        .chars()
                        .map(|c| {
                            if crate::wrap::display_width(&c.to_string()) > 1 {
                                '\u{fffd}'
                            } else {
                                c
                            }
                        })
                        .collect(),
                    _ => shown,
                };
                assert_eq!(said.text, expected, "{at}");
            }
        }
    }

    /// A colour and the modifiers a cell or a span is drawn in.
    type Ink = (ratatui::style::Color, Modifier);

    /// The colour and modifiers a span of the panel is drawn in.
    fn ink_of(span: &Span<'static>) -> Ink {
        (
            span.style.fg.unwrap_or(ratatui::style::Color::Reset),
            span.style.add_modifier,
        )
    }

    /// The row of a printed plan on the screen that holds both `text` and its status `word`, which
    /// no other drawing of the plan holds: the mark and the ink of the mark and of the text.
    fn printed_row(buffer: &Buffer, text: &str, word: &str) -> (String, Ink, Ink) {
        let width = buffer.area.width;
        for y in (0..buffer.area.height).rev() {
            let row: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            let Some(at) = row.find(text).filter(|_| row.contains(word)) else {
                continue;
            };
            let column = |chars: usize| u16::try_from(chars).expect("a column");
            let text_at = column(row[..at].chars().count());
            let mark_at = column(row.chars().take_while(|c| *c == ' ').count());
            let ink = |x: u16| (buffer[(x, y)].fg, buffer[(x, y)].modifier);
            return (
                buffer[(mark_at, y)].symbol().to_string(),
                ink(mark_at),
                ink(text_at),
            );
        }
        panic!("no row of the screen holds {text} and {word}");
    }

    /// A printed plan draws a row that is neither completed nor cancelled as the panel does: as
    /// work being done when a turn is running as it is printed, and left open, hollow and dim, when
    /// none is. The status word is the planner's either way, and the printed plan keeps the
    /// drawing it had when it was printed after the turn ends.
    ///
    /// Asserted against the panel's own rows in the same state, so a change to how the panel draws
    /// them is followed here: the mark and its ink while a turn runs, and the mark and the text as
    /// well once it has ended. The text of a row being worked on is bold, as the plan under a turn
    /// draws it. Rows that are completed or cancelled are drawn alike in both.
    ///
    /// Rejects: a printed plan that draws every open row as work being done whether or not a turn
    /// is running, one that reads whether a turn is running as it draws, which rewrites the
    /// printout when the turn ends, an open row drawn struck through, and a status word taken from
    /// how the row is drawn.
    #[test]
    fn a_printed_plan_draws_open_rows_as_the_panel_does_and_keeps_that_after_the_turn() {
        let _held = crate::theme::exclusive();
        assert_eq!(
            crate::wrap::display_width(LEFT_OPEN_MARKER),
            1,
            "the hollow mark is not one column, which the margin reserves"
        );
        let open = [("working on it", "in progress"), ("still to do", "pending")];
        let closed = [("finished it", "completed"), ("dropped it", "cancelled")];

        // Printed while a turn runs, and again once it has ended.
        let mut running = working_on(plan(&MIXED));
        let panel_running = plan_part(&running, 4);
        running.report_plan();
        let while_running = frame(&mut running, 80, 40);

        let mut ended = working_on(plan(&MIXED));
        ended.complete("done", Vec::new(), 0);
        let panel_ended = plan_part(&ended, 4);
        ended.report_plan();
        let after = frame(&mut ended, 80, 40);

        for (text, word) in open {
            let (mark, mark_ink, text_ink) = printed_row(&while_running, text, word);
            let panel_row = plan_row(&panel_running, text);
            let at = format!("{text} printed while a turn runs");
            assert_eq!(mark, panel_row.spans[0].content.trim(), "{at}");
            assert_eq!(mark_ink, ink_of(&panel_row.spans[0]), "{at}");
            // The text is bold as the plan under a turn draws it, and the panel's is not.
            assert!(text_ink.1.contains(Modifier::BOLD), "{at}: {text_ink:?}");
            assert!(!text_ink.1.contains(Modifier::CROSSED_OUT), "{at}");

            let (left_mark, left_mark_ink, left_text_ink) = printed_row(&after, text, word);
            let panel_row = plan_row(&panel_ended, text);
            let at = format!("{text} printed after the turn");
            assert_eq!(left_mark, LEFT_OPEN_MARKER, "{at}");
            assert_eq!(left_mark, panel_row.spans[0].content.trim(), "{at}");
            assert_eq!(left_mark_ink, ink_of(&panel_row.spans[0]), "{at}");
            assert_eq!(left_text_ink, ink_of(&panel_row.spans[1]), "{at}");
            assert!(
                !left_text_ink.1.contains(Modifier::CROSSED_OUT),
                "{at} is struck through"
            );

            // The two are drawn apart in the mark and in both inks, so the comparisons against
            // the panel above say something.
            assert_ne!(left_mark, mark, "{text}");
            assert_ne!(left_mark_ink, mark_ink, "{text}");
            assert_ne!(left_text_ink, text_ink, "{text}");
        }
        for (text, word) in closed {
            assert_eq!(
                printed_row(&after, text, word),
                printed_row(&while_running, text, word),
                "{text} is drawn differently once the turn ended"
            );
        }

        // A printout made while the turn ran does not change when the turn ends.
        let drawn = |buffer: &Buffer| open.map(|(text, word)| printed_row(buffer, text, word));
        let before_end = drawn(&while_running);
        running.complete("done", Vec::new(), 0);
        assert_eq!(
            drawn(&frame(&mut running, 80, 40)),
            before_end,
            "a printout changed when its turn ended"
        );
    }

    /// A task is the planner's text, so it cannot draw anything of the interface's: its escape
    /// sequence and its line break are drawn as visible characters, and a mark or a status it
    /// writes into itself appears after the margin of the mark and status it carries, on whichever
    /// row it lands, so it is never in the margin.
    ///
    /// The width is narrow enough that the forged mark begins a row of its own, where a row
    /// without the margin would show it as the mark of a task.
    ///
    /// Rejects: control characters drawn as they are, which the terminal acts on or the buffer
    /// drops without a trace, and rows that break without the margin, which lets text of the task
    /// stand where the mark and status of a task stand.
    #[test]
    fn a_printed_task_cannot_draw_a_margin_a_mark_or_a_status_of_its_own() {
        let forged =
            "fix it\u{1b}[2J\u{1b}[31m \u{2713} completed forged\n\u{2713} completed second";
        let mut session = working_on(plan(&[
            (forged, Status::Pending),
            ("an honest task", Status::Done),
        ]));
        let width = u16::try_from(crate::render::printed_margin(usize::MAX) + 12).expect("a width");
        let margin = crate::render::printed_margin(usize::from(width));
        let rows = printed(&mut session, width);
        let all = rows.join("\n");

        assert!(
            !all.contains('\u{1b}'),
            "an escape reached the screen: {all}"
        );
        assert!(
            all.contains('\u{241b}'),
            "the escape was not drawn as a visible character: {all}"
        );
        assert!(
            all.contains('\u{240a}'),
            "the line break was not drawn as a visible character: {all}"
        );

        let group: Vec<&String> = rows
            .iter()
            .take_while(|row| !row.contains("honest"))
            .filter(|row| !row.trim().is_empty())
            .collect();
        for row in group.iter().skip(1) {
            assert!(
                row.chars().take(margin).all(|c| c == ' '),
                "text of the task stands in the margin: {row:?}"
            );
        }
        assert!(
            group.len() >= 4,
            "the task did not wrap, so the fixture proves nothing: {rows:#?}"
        );
        assert!(
            group[0].starts_with(&format!("  \u{25a0} {}", t!(todos_status_pending))),
            "the task does not begin with the mark and status it carries: {rows:#?}"
        );
        assert!(
            group[1..].iter().any(|row| row
                .chars()
                .skip(margin)
                .collect::<String>()
                .trim_start()
                .starts_with('\u{2713}')),
            "no row begins with the forged mark, so the fixture proves nothing: {rows:#?}"
        );
        // Only the two tasks have a mark of their own.
        let marks = rows
            .iter()
            .filter(|row| {
                row.trim_start()
                    .starts_with(['\u{2713}', '\u{25a0}', '\u{2717}'])
            })
            .filter(|row| row.chars().take(margin).any(|c| c != ' '))
            .count();
        assert_eq!(marks, 2, "{rows:#?}");
    }

    /// The rows of a printed plan are the rows the screen draws: none is wider than the
    /// transcript, so the transcript does not wrap it again, at the widths where it would have
    /// wrapped a margin of the whole width.
    ///
    /// Rejects: a margin wider than the transcript, which the paragraph wraps into a row that
    /// starts at the left edge with the text of the task.
    #[test]
    fn a_printed_plan_is_drawn_on_the_screen_as_it_is_laid_out() {
        let task = "a long task in plain words that has to wrap onto more rows than one";
        for width in [12u16, 18, 24, 30] {
            let mut session = working_on(plan(&[(task, Status::Active), ("short", Status::Done)]));
            let laid = printed(&mut session, width);
            let screen = screen(&mut session, width, 60);
            for row in laid.iter().filter(|row| !row.trim().is_empty()) {
                assert!(
                    screen
                        .iter()
                        .any(|drawn| drawn.trim_end() == row.trim_end()),
                    "{row:?} is not a row of the screen at {width} columns: {screen:#?}"
                );
            }
        }
    }

    /// A printed plan is the plan as it stood when it was printed. The planner reporting another
    /// does not rewrite it, and the next `/todos` prints the new one under it.
    ///
    /// Rejects: an entry that reads the session's plan as it draws, which would print the later
    /// plan in both places.
    #[test]
    fn a_printed_plan_stays_as_it_was_when_the_plan_changes() {
        let mut session = working_on(plan(&[("first draft", Status::Active)]));
        session.report_plan();
        session.set_todos(plan(&[("second draft", Status::Active)]));
        screen(&mut session, 80, 40);
        let drawn = crate::render::as_text(&session);
        assert!(drawn.contains("first draft"), "{drawn}");
        assert!(!drawn.contains("second draft"), "{drawn}");

        session.report_plan();
        screen(&mut session, 80, 40);
        let drawn = crate::render::as_text(&session);
        let (first, second) = (drawn.find("first draft"), drawn.find("second draft"));
        assert!(first.is_some(), "the first plan is gone: {drawn}");
        assert!(second.is_some(), "the second plan was not printed: {drawn}");
        assert!(first < second, "{drawn}");
    }

    /// The key prints the same whole plan the command does, with every task, its status and none
    /// of its text cut, at a width the panel could not draw it in.
    ///
    /// Rejects: a key that prints some other form of the plan, such as the rows the panel has room
    /// for, and one that cuts a task to the width.
    #[test]
    fn the_key_and_the_command_print_the_same_whole_plan_and_cut_none_of_it() {
        let rows: Vec<(String, Status)> = (1..=12)
            .map(|n| {
                (
                    format!(
                        "task {n} is written to be longer than the panel is wide: {}",
                        "word ".repeat(n)
                    ),
                    [
                        Status::Done,
                        Status::Active,
                        Status::Pending,
                        Status::Cancelled,
                    ][n % 4],
                )
            })
            .collect();
        let borrowed: Vec<(&str, Status)> = rows
            .iter()
            .map(|(text, status)| (text.as_str(), *status))
            .collect();
        let finished = |plan| {
            let mut session = working_on(plan);
            session.complete("done", Vec::new(), 0);
            session
        };
        let alt_a = ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('a'),
            ratatui::crossterm::event::KeyModifiers::ALT,
        );

        let mut keyed = finished(plan(&borrowed));
        crate::app::handle_key(&mut keyed, alt_a);
        let mut typed = finished(plan(&borrowed));
        for c in "/todos".chars() {
            crate::app::handle_key(
                &mut typed,
                ratatui::crossterm::event::KeyEvent::new(
                    ratatui::crossterm::event::KeyCode::Char(c),
                    ratatui::crossterm::event::KeyModifiers::NONE,
                ),
            );
        }
        crate::app::handle_key(
            &mut typed,
            ratatui::crossterm::event::KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Enter,
                ratatui::crossterm::event::KeyModifiers::NONE,
            ),
        );

        for width in [40, 100] {
            screen(&mut keyed, width, 60);
            screen(&mut typed, width, 60);
            let (by_key, by_command) = (
                crate::render::as_text(&keyed),
                crate::render::as_text(&typed),
            );
            // The typed command is a line of its own above its plan, which the key has no line
            // for, so each is compared from its heading down.
            let from_heading = |text: &str| -> String {
                assert_eq!(text.matches("plan:").count(), 1, "{text}");
                text[text.find("plan:").expect("a heading")..].to_string()
            };
            assert_eq!(
                from_heading(&by_key),
                from_heading(&by_command),
                "the key and the command printed different plans at {width} columns"
            );
            let all: Vec<String> = by_key.lines().map(str::to_string).collect();
            let at = all
                .iter()
                .rposition(|row| row.trim().starts_with("plan:"))
                .expect("a heading");
            assert!(all.iter().all(|row| !row.contains('…')), "{by_key}");
            let printed = tasks_printed(&all[at + 1..]);
            assert_eq!(printed.len(), 12, "{by_key}");
            for ((said, text), (original, status)) in printed.iter().zip(&rows) {
                let word = match status {
                    Status::Done => t!(todos_status_done),
                    Status::Active => t!(todos_status_active),
                    Status::Pending => t!(todos_status_pending),
                    Status::Cancelled => t!(todos_status_cancelled),
                };
                assert_eq!(said, word, "{original}");
                assert_eq!(
                    *text,
                    original.split_whitespace().collect::<String>(),
                    "{original} lost text at {width} columns"
                );
            }
        }
    }

    fn mouse(
        kind: ratatui::crossterm::event::MouseEventKind,
        row: u16,
        column: u16,
    ) -> ratatui::crossterm::event::MouseEvent {
        ratatui::crossterm::event::MouseEvent {
            kind,
            column,
            row,
            modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
        }
    }

    /// Hold the left button down on `from`, move to `to` and let go, as the event loop would.
    fn dragged(session: &mut Session, from: (u16, u16), to: (u16, u16)) -> Vec<crate::app::Action> {
        use ratatui::crossterm::event::{MouseButton::Left, MouseEventKind::*};
        [Down(Left), Drag(Left), Up(Left)]
            .into_iter()
            .zip([from, to, to])
            .map(|(kind, (row, column))| {
                crate::app::handle_mouse(session, mouse(kind, row, column))
            })
            .collect()
    }

    /// Dragging the border moves it, the panel takes what is to its right, and the transcript and
    /// the input wrap to the columns left of it. A panel that kept 36 columns, or a transcript laid
    /// out at the old width, fails here.
    #[test]
    fn a_drag_on_the_border_moves_it_and_the_transcript_rewraps() {
        let mut session = opened_at(120);
        screen(&mut session, 120, 30);
        assert_eq!(session.laid.width, 84, "the default is 36 columns");
        dragged(&mut session, (5, 84), (5, 70));
        let rows = screen(&mut session, 120, 30);
        assert_eq!(session.laid.width, 70, "the transcript kept its old width");
        assert_eq!(session.panel_columns(120), 50);
        let cell = |x: usize| rows[5].chars().nth(x).unwrap_or(' ');
        assert_eq!(cell(70), '│', "the border is not where it was dragged to");
        assert_ne!(cell(84), '│', "the border is also where it was");
    }

    /// A drag past either end stops at the minimum panel and at the transcript's 64 columns, so
    /// neither pane can be dragged out of existence.
    #[test]
    fn the_width_is_clamped_at_both_ends() {
        let mut session = opened_at(120);
        screen(&mut session, 120, 30);
        dragged(&mut session, (5, 84), (5, 0));
        assert_eq!(
            session.panel_columns(120),
            56,
            "the transcript lost columns"
        );
        screen(&mut session, 120, 30);
        assert_eq!(session.laid.width, 64);
        let border = session.laid.width;
        dragged(&mut session, (5, border), (5, 119));
        assert_eq!(session.panel_columns(120), NARROWEST_PANEL);
    }

    /// A terminal narrowed while the panel is open draws it at what fits, and widened again it is
    /// back at the width that was dragged to: the choice is the person's, the clamp is the screen's.
    #[test]
    fn a_narrowed_terminal_reclamps_without_forgetting_the_choice() {
        let mut session = opened_at(160);
        screen(&mut session, 160, 30);
        dragged(&mut session, (5, 124), (5, 90));
        assert_eq!(session.panel_columns(160), 70);
        assert_eq!(session.panel_columns(120), 56);
        assert_eq!(session.panel_columns(100), 36);
        assert_eq!(session.panel_columns(160), 70, "the choice was overwritten");
    }

    /// A press on the border is the handle and never the start of a selection, and the release of
    /// that drag copies nothing. A press one column to the right is the panel's text and still
    /// selects, so the border is exactly one column wide.
    #[test]
    fn a_drag_from_the_border_never_selects_or_copies() {
        use crate::app::Action;
        let mut session = opened_at(120);
        screen(&mut session, 120, 30);
        let actions = dragged(&mut session, (5, 84), (8, 100));
        assert!(session.selection.is_none(), "the drag selected text");
        assert!(!actions.contains(&Action::Copy), "the release copied");
        assert!(!session.dragging_panel(), "the drag was never let go");

        let mut session = opened_at(120);
        screen(&mut session, 120, 30);
        let actions = dragged(&mut session, (5, 90), (6, 95));
        assert!(
            session.selection.is_some(),
            "text in the panel did not select"
        );
        assert_eq!(actions.last(), Some(&Action::Copy));
    }

    /// Move the pointer to `row` and `column` with no button held, as a terminal reports it, and
    /// say what shape the session then asks for.
    fn shape_at(session: &mut Session, row: u16, column: u16) -> crate::pointer::Shape {
        let moved = mouse(
            ratatui::crossterm::event::MouseEventKind::Moved,
            row,
            column,
        );
        crate::app::handle_mouse(session, moved);
        session.pointer_shape()
    }

    /// The border is the panel's left column from its first row to its last, so a pointer one
    /// column to either side, or on the hint line under it, is not told it can drag.
    #[test]
    fn the_resize_pointer_is_over_the_border_column_and_nowhere_else() {
        use crate::pointer::Shape::{Default, ResizeColumns};
        let mut session = opened_at(120);
        screen(&mut session, 120, 30);
        let panel = session.laid.panes.last().expect("panel").1;
        let (first, last, below) = (panel.top(), panel.bottom() - 1, panel.bottom());
        let border = panel.left();
        assert_eq!(shape_at(&mut session, first, border), ResizeColumns);
        assert_eq!(shape_at(&mut session, last, border), ResizeColumns);
        assert_eq!(shape_at(&mut session, below, border), Default, "below it");
        assert_eq!(shape_at(&mut session, 5, border - 1), Default, "left of it");
        assert_eq!(
            shape_at(&mut session, 5, border + 1),
            Default,
            "right of it"
        );
    }

    /// A drag keeps the resize pointer while the pointer is nowhere near where the border was when
    /// the frame was drawn, and when it ends the pointer is the resize one only if it rests on the
    /// border where the next frame put it. A shape taken from hovering alone drops in the first, and
    /// one taken from the drag alone is wrong in the second.
    #[test]
    fn a_drag_holds_the_resize_pointer_and_its_end_follows_the_border() {
        use crate::pointer::Shape::{Default, ResizeColumns};
        use ratatui::crossterm::event::{MouseButton::Left, MouseEventKind::*};
        let mut session = opened_at(120);
        screen(&mut session, 120, 30);
        let step = |session: &mut Session, kind, column| {
            crate::app::handle_mouse(session, mouse(kind, 5, column));
            session.pointer_shape()
        };
        assert_eq!(step(&mut session, Down(Left), 84), ResizeColumns);
        assert_eq!(
            step(&mut session, Drag(Left), 70),
            ResizeColumns,
            "the drag ended the shape before the border had been drawn where the pointer is"
        );
        screen(&mut session, 120, 30);
        assert_eq!(
            step(&mut session, Up(Left), 70),
            ResizeColumns,
            "at rest on the border"
        );
        assert_eq!(step(&mut session, Moved, 40), Default, "moved off it");
    }

    /// A panel closed while the pointer rests on its border leaves the pointer over nothing, with
    /// no report of the pointer to say so.
    #[test]
    fn a_panel_closed_under_the_pointer_gives_the_resize_pointer_back() {
        use crate::pointer::Shape::{Default, ResizeColumns};
        let mut session = opened_at(120);
        screen(&mut session, 120, 30);
        let border = session.laid.panes.last().expect("panel").1.left();
        assert_eq!(shape_at(&mut session, 5, border), ResizeColumns);
        session.toggle_panel();
        screen(&mut session, 120, 30);
        assert_eq!(session.pointer_shape(), Default);
    }
}
