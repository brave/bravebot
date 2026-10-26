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

/// Rows a finished task opened to its whole words may take before they are cut.
const DETAIL_ROWS: usize = 6;

/// Columns at the left of a plan row that hold its mark, counted from the border: the space before
/// the glyph, the glyph, and the space after it. A click on them opens or closes a finished task's
/// words, and one on the rest of the row goes to the task (MOUSE-32).
const MARK_COLUMNS: u16 = 3;

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
    wrapped_over(text, width, MOST_ROWS)
}

// How many times a text has been wrapped on this thread, for the test that holds the panel's work
// to the length of the plan.
#[cfg(test)]
thread_local! {
    static WRAPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `text` wrapped to `width` columns over at most `most` rows, the last cut with an ellipsis where
/// more was left.
fn wrapped_over(text: &str, width: usize, most: usize) -> Vec<String> {
    #[cfg(test)]
    WRAPS.with(|wraps| wraps.set(wraps.get() + 1));
    let shown = crate::render::printable(&crate::status::cut(text, usize::MAX));
    if shown.is_empty() {
        return Vec::new();
    }
    let mut rows = crate::wrap::wrap(&shown, width, 0).rows;
    if rows.len() > most {
        let rest = rows.split_off(most - 1).join(" ");
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
/// address it stands for. What it drew that a click acts on is given back as the cells of each
/// (MOUSE-50).
pub fn draw(buffer: &mut Buffer, area: Rect, session: &Session) -> Vec<crate::state::Target> {
    let (rows, links, _, row_targets) = laid_out(session, area.width, area.height);
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
    // The panes the wheel scrolls are under every row and control drawn on them.
    let mut recorded = wheel_panes(session, area);
    recorded.extend(placed(session, area, &row_targets));
    recorded
}

/// The panes of the files state that a turn of the wheel scrolls in a panel drawn over `area`, each
/// as the cells it covers, the border column included since the wheel is the pointer's over the
/// panel (MOUSE-19). The heading, the list and the rule are above the rule's row, and the file's
/// name, the bar, the edits and the row of keys, which is the last of the panel, are below it.
fn wheel_panes(session: &Session, area: Rect) -> Vec<crate::state::Target> {
    use crate::state::{Hit, Scrolled, Target as Placed};
    if session.files_state().is_none() || area.is_empty() {
        return Vec::new();
    }
    let rule =
        crate::files_mode::Geometry::new(usize::from(area.height), session.written_totals().0)
            .rule();
    // The rows above the rule that are not the last row of the panel.
    let list = u16::try_from(rule + 1)
        .unwrap_or(u16::MAX)
        .min(area.height.saturating_sub(1));
    let edits = area.height - list;
    let mut panes = Vec::new();
    if list > 0 {
        panes.push(Placed::on_screen(
            Rect::new(area.x, area.y, area.width, list),
            Hit::Wheel(Scrolled::List),
        ));
    }
    if edits > 0 {
        panes.push(Placed::on_screen(
            Rect::new(area.x, area.y + list, area.width, edits),
            Hit::Wheel(Scrolled::Edits),
        ));
    }
    panes
}

/// The cells a click acts on in a panel drawn over `area`, given the places [`laid_out`] says are a
/// click's, each as the cells of its row and what a click on them is.
///
/// The border is the panel's resize handle and is no target (PANEL-6). A row is the cells after the
/// border, or the cells of it the layout names, so one that is not drawn, or is cut for want of
/// room, has none (MOUSE-11). The mark of a finished task opens its words and the rest of its row
/// goes to the task. Which it is reads the status, a closed set the strike-through is drawn from as
/// well, and never the words (MOUSE-32). The mark is recorded after the row so that it is the one
/// found.
fn placed(session: &Session, area: Rect, rows: &[Place]) -> Vec<crate::state::Target> {
    use crate::state::{Hit, Target as Placed};
    let mut placed = Vec::new();
    if area.is_empty() {
        return placed;
    }
    let text = area.width.saturating_sub(1);
    for place in rows {
        let y = area
            .y
            .saturating_add(u16::try_from(place.row).unwrap_or(u16::MAX));
        let (first, width) = match &place.columns {
            None => (0, text),
            Some(columns) => {
                let first = u16::try_from(columns.start).unwrap_or(u16::MAX).min(text);
                let end = u16::try_from(columns.end).unwrap_or(u16::MAX).min(text);
                (first, end.saturating_sub(first))
            }
        };
        if width == 0 {
            continue;
        }
        placed.push(Placed::on_screen(
            Rect::new(area.x + 1 + first, y, width, 1),
            Hit::Panel(place.target),
        ));
        if let Target::Task(task) = place.target
            && session
                .plan()
                .get(task)
                .is_some_and(|row| row.status == bravebot_core::todo::Status::Done)
        {
            placed.push(Placed::on_screen(
                Rect::new(area.x + 1, y, MARK_COLUMNS.min(text), 1),
                Hit::Panel(Target::Detail(task)),
            ));
        }
    }
    placed
}

/// The rows of the panel, where the links are, which rows continue on the row below, and which rows
/// a click acts on.
type Body = (Vec<Line<'static>>, Vec<Link>, Vec<usize>, Vec<Place>);

/// What a click on a row of the panel acts on (MOUSE-11, MOUSE-18, MOUSE-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A row of the session's name: the same line `/rename` is typed with, for the person to edit.
    Name,
    /// The model row: the same question `/model` asks.
    Model,
    /// The effort row: the same question `/effort` asks.
    Effort,
    /// The context reading: the same question `/context` asks.
    Context,
    /// A started MCP server's row: the same question a bare `/mcp` asks.
    Servers,
    /// A started language server's row: the same question `/lsp` asks.
    Languages,
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
    /// The row that names the key hiding the panel: the same thing that key does (MOUSE-21).
    Hide,
    /// The row of a task of the plan, by its place in the plan: the view goes to where the turn that
    /// wrote it drew it (MOUSE-31).
    Task(usize),
    /// The mark of a finished task, or a row below it once its words are open, by its place in the
    /// plan: it opens or closes the whole of them (MOUSE-32).
    Detail(usize),
    /// The pull request row of the Links section: opens the address `/pr` gave (MOUSE-41).
    PullRequest,
    /// The issue row of the Links section: opens the address `/issue` gave (MOUSE-41).
    Issue,
}

impl Target {
    /// Whether the files state draws this target and the ordinary state does not.
    pub fn is_in_the_files_state(self) -> bool {
        matches!(
            self,
            Target::File(_) | Target::CloseFiles | Target::PreviousEdit | Target::NextEdit
        )
    }
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

/// The target a press on a cell of a panel drawn over `area` lands on, if any, as the cells
/// [`draw`] records for it (MOUSE-50).
#[cfg(test)]
pub fn target_at(session: &Session, area: Rect, row: u16, column: u16) -> Option<Target> {
    let (.., rows) = laid_out(session, area.width, area.height);
    placed(session, area, &rows)
        .into_iter()
        .rev()
        .find(|target| target.area.contains((column, row).into()))
        .and_then(|target| match target.hit {
            crate::state::Hit::Panel(hit) => Some(hit),
            _ => None,
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
        let rows: Vec<_> = languages
            .iter()
            .map(|program| row((*program).to_string(), Style::default()))
            .collect();
        let count = rows.len();
        section(&mut body, t!(panel_language_servers).to_string(), rows);
        let first = body.len() - count;
        targets.extend((first..first + count).map(|row| Place::on_row(row, Target::Languages)));
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

    // The blank row and the heading come out of the room before the rows do.
    let room = (height - 1).saturating_sub(body.len() + 2);
    if !session.plan().is_empty() && room > 0 {
        plan_section(session, text, room, &mut body, &mut targets);
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
    targets.push(Place::on_row(height - 1, Target::Hide));
    (body, links, continued, targets)
}

/// The Plan section: its heading, the rows of the plan that `room` rows hold and the row that names
/// the key that prints it whole where one is drawn, added to `body`, with what a click on each row
/// of it acts on added to `targets`.
fn plan_section(
    session: &Session,
    text: usize,
    room: usize,
    body: &mut Vec<Line<'static>>,
    targets: &mut Vec<Place>,
) {
    let plan = session.plan();
    let opened = opened_words(plan, session.plan_detail(), text);
    let (cut, names_the_key) = choose_rows(plan, &opened, room);
    let (mut rows, tasks) = plan_rows(plan, opened, &cut, !session.a_turn_is_running(), text);
    if names_the_key {
        rows.push(row(
            crate::status::cut(
                &t!(panel_todos, chord = session.bindings().todos_name()),
                text,
            ),
            dim(),
        ));
    }
    let count = rows.len();
    section(body, t!(panel_plan).to_string(), rows);
    let first = body.len() - count;
    targets.extend(
        tasks
            .into_iter()
            .map(|(row, target)| Place::on_row(first + row, target)),
    );
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
    let name_rows = wrapped(&name, text);
    let name_row_count = name_rows.len();
    for name_row in name_rows {
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
        targets.extend((first..first + name_row_count).map(|row| Place::on_row(row, Target::Name)));
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
    let mut link_targets = Vec::new();
    for (label, url, target) in [
        (
            t!(panel_pull_request),
            &identity.pull_request,
            Target::PullRequest,
        ),
        (t!(panel_issue), &identity.issue, Target::Issue),
    ] {
        let Some(url) = url.as_deref() else {
            continue;
        };
        let (line, link) = link_row(label, url, text);
        address_links.extend(link.map(|link| Link {
            row: link_rows.len(),
            ..link
        }));
        // A row is a target only for an address the opener takes, so a click on one it refuses is
        // the selection it would be anywhere else.
        if crate::opener::is_openable(url) {
            link_targets.push((link_rows.len(), target));
        }
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
        targets.extend(
            link_targets
                .into_iter()
                .map(|(row, target)| Place::on_row(first + row, target)),
        );
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

/// The fewest rows the plan has for the row that names the key that prints it whole to be drawn.
/// With fewer, the rows of the plan and the row that counts what it left out take them.
const LEAST_ROOM_TO_NAME_THE_KEY: usize = 3;

/// The rows of `plan` drawn where `room` rows are the plan's, and whether the last of those rows
/// names the key that prints the plan whole.
///
/// Without the key row the plan has the room, less the row that counts what was left out where it
/// does not fit. The key row takes one more, and is drawn only where it takes the place of nothing a
/// person would look for. It is left out where the room has fewer than
/// [`LEAST_ROOM_TO_NAME_THE_KEY`] rows, where the plan fills the room exactly, and where it would
/// take the place of a row still to do: the room less the key row holds the whole plan, or holds
/// every row still to do and the row that counts the rest.
///
/// Decided from the statuses alone, by the same choice that draws the rows.
fn choose_rows(
    plan: &[bravebot_core::todo::Row],
    opened: &[Option<Vec<String>>],
    room: usize,
) -> (Cut, bool) {
    let total = rows_of_the_plan(opened);
    if room >= LEAST_ROOM_TO_NAME_THE_KEY && total != room {
        let beside_the_key = if total < room {
            cut_to(plan, opened, total)
        } else {
            cut_to(plan, opened, room - 2)
        };
        if beside_the_key.to_do == 0 {
            return (beside_the_key, true);
        }
    }
    (cut_for(plan, opened, room), false)
}

/// The mark of a row that no turn is working on and that was neither finished nor dropped.
///
/// Hollow where the mark of a row being worked on is filled, so the two differ under `NO_COLOR`
/// too, where the ink of a row is the terminal's own.
pub(crate) const LEFT_OPEN_MARKER: &str = "□";

/// The words of each finished task of `plan` that is in `open`, wrapped to `width` over at most
/// [`DETAIL_ROWS`] rows, and `None` for every other task (MOUSE-32).
///
/// Wrapped once, whichever of them the layout then looks at.
fn opened_words(
    plan: &[bravebot_core::todo::Row],
    open: &std::collections::BTreeSet<usize>,
    width: usize,
) -> Vec<Option<Vec<String>>> {
    plan.iter()
        .enumerate()
        .map(|(task, row)| {
            (row.status == bravebot_core::todo::Status::Done && open.contains(&task))
                .then(|| wrapped_over(&row.content, width.saturating_sub(2), DETAIL_ROWS))
        })
        .collect()
}

/// The rows task `at` takes: one, or as many as the words of a task that is open.
fn height_of(opened: &[Option<Vec<String>>], at: usize) -> usize {
    opened[at].as_ref().map_or(1, |rows| rows.len().max(1))
}

/// Which rows of a plan that does not fit are drawn, and how many of each kind are not.
#[derive(Default)]
struct Cut {
    /// The positions in the plan of the tasks drawn, in the plan's order, each with the rows it is
    /// given: its height, or the rows that were left of the room where its words do not all fit.
    shown: Vec<(usize, usize)>,
    /// Rows still to do or in progress that were left out.
    to_do: usize,
    done: usize,
    cancelled: usize,
}

/// The rows the whole plan takes: one for each task and the rows more that the words of an open one
/// take.
fn rows_of_the_plan(opened: &[Option<Vec<String>>]) -> usize {
    (0..opened.len()).map(|at| height_of(opened, at)).sum()
}

/// The tasks of `plan` to draw in `capacity` rows, chosen from each row's status and from nothing
/// else, where `opened` holds the words of the finished tasks a click opened and a task that is
/// open takes the rows they take (MOUSE-32).
///
/// The rows still outstanding come first, the one in progress before the pending ones, each in the
/// plan's order. Where capacity is left over it goes next to the finished tasks that are open, which
/// somebody asked to read, and then to the other finished rows, those nearest the first outstanding
/// row first and the earlier of two as near, or nearest the end of a plan with none, so that what was
/// last done is shown beside what comes next. The rows chosen are drawn in the plan's order. A task
/// that does not fit the capacity that is left is given what is left.
///
/// The text of a task is the planner's and decides nothing here. A status the driver does not
/// recognise reads as outstanding ([TODO-2](../../../docs/specs/tools/todo-write.md#TODO-2)), so
/// its row is drawn first and counted among those still to do.
fn cut_to(
    plan: &[bravebot_core::todo::Row],
    opened: &[Option<Vec<String>>],
    capacity: usize,
) -> Cut {
    use bravebot_core::todo::Status;
    let mut outstanding: Vec<usize> = (0..plan.len()).filter(|&at| !plan[at].struck()).collect();
    let anchor = outstanding.first().copied().unwrap_or(plan.len());
    // Stable, so each group keeps the plan's order.
    outstanding.sort_by_key(|&at| plan[at].status != Status::Active);
    let mut finished: Vec<usize> = (0..plan.len()).filter(|&at| plan[at].struck()).collect();
    finished.sort_by_key(|&at| (opened[at].is_none(), at.abs_diff(anchor), at));
    let mut spare = capacity;
    let mut shown = Vec::new();
    for at in outstanding.into_iter().chain(finished) {
        if spare == 0 {
            break;
        }
        let rows = height_of(opened, at).min(spare);
        spare -= rows;
        shown.push((at, rows));
    }
    shown.sort_unstable();

    let mut cut = Cut {
        shown,
        ..Cut::default()
    };
    for (at, row) in plan.iter().enumerate() {
        if cut
            .shown
            .binary_search_by_key(&at, |&(task, _)| task)
            .is_ok()
        {
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

/// The rows of `plan` drawn in `room` rows with no row for the key: all of them where they fit, and
/// otherwise as many as the room holds beside the row that counts the rest.
fn cut_for(plan: &[bravebot_core::todo::Row], opened: &[Option<Vec<String>>], room: usize) -> Cut {
    let total = rows_of_the_plan(opened);
    if total <= room {
        cut_to(plan, opened, total)
    } else {
        cut_to(plan, opened, room - 1)
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
///
/// A finished task whose words are in `opened` is drawn over as many rows as they take, where the
/// others are cut to one (MOUSE-32).
///
/// With the rows, which of them are a click's: the first row of a task goes to it, and the rows
/// under one opened go to its detail (MOUSE-31, MOUSE-32). The row that counts what was left out and
/// the row that names the key are none of them.
fn plan_rows(
    plan: &[bravebot_core::todo::Row],
    mut opened: Vec<Option<Vec<String>>>,
    cut: &Cut,
    left_open: bool,
    width: usize,
) -> (Vec<Line<'static>>, Vec<(usize, Target)>) {
    use bravebot_core::todo::Status;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut targets = Vec::new();
    for &(task, rows) in &cut.shown {
        let row = &plan[task];
        let (glyph, marker, text) = if row.status == Status::Cancelled {
            (row.marker, dim(), dim().add_modifier(Modifier::CROSSED_OUT))
        } else if row.struck() {
            (
                row.marker,
                Style::default().fg(theme::ok()),
                dim().add_modifier(Modifier::CROSSED_OUT),
            )
        } else if left_open {
            (LEFT_OPEN_MARKER, dim(), dim())
        } else {
            (
                row.marker,
                Style::default().fg(theme::running()),
                Style::default(),
            )
        };
        match opened[task].take() {
            // Not struck through: the words are opened to be read.
            Some(words) => {
                for (place, words) in words.into_iter().take(rows).enumerate() {
                    let lead = if place == 0 {
                        format!(" {glyph} ")
                    } else {
                        "   ".to_string()
                    };
                    targets.push((
                        lines.len(),
                        if place == 0 {
                            Target::Task(task)
                        } else {
                            Target::Detail(task)
                        },
                    ));
                    lines.push(Line::from(vec![
                        Span::styled(lead, marker),
                        Span::styled(words, dim()),
                    ]));
                }
            }
            None => {
                let content = crate::status::cut(
                    &crate::render::printable(&row.content),
                    width.saturating_sub(2),
                );
                targets.push((lines.len(), Target::Task(task)));
                lines.push(Line::from(vec![
                    Span::styled(format!(" {glyph} "), marker),
                    Span::styled(content, text),
                ]));
            }
        }
    }
    lines.extend(left_out_row(cut, width));
    (lines, targets)
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

    /// MOUSE-30. Rejects: a target on the heading above the name, on the directory, the branch or
    /// the id under it, on a name row that is not drawn, and a name that has no target on its second
    /// and third rows.
    #[test]
    fn only_the_rows_of_the_sessions_name_are_name_targets() {
        let mut session = Session::new("none");
        let area = Rect::new(84, 0, WIDTH, 40);
        let inside = area.x + 3;
        let target = |session: &Session, row: u16| target_at(session, area, area.y + row, inside);

        // Nothing is named yet, so there is no name to click, and the directory below is not one.
        session.identify("", "~/work".to_string(), Some("main"));
        let rows = lines(&session, area.width, area.height);
        let at = |rows: &[Line<'static>], needle: &str| {
            rows.iter()
                .position(|line| line.to_string().contains(needle))
                .expect(needle) as u16
        };
        assert_eq!(target(&session, at(&rows, "~/work")), None);

        let long = "one two three four five six seven eight nine ten eleven twelve thirteen";
        session.identify(long, "~/work".to_string(), Some("main"));
        session.name_id(Some("0e6f2c1a-4b7d-4a55-9d3e-123456789abc"));
        let rows = lines(&session, area.width, area.height);
        let heading = at(&rows, t!(panel_session));
        assert_eq!(target(&session, heading), None, "the heading");
        for row in 1..=3 {
            assert_eq!(
                target(&session, heading + row),
                Some(Target::Name),
                "name row {row}"
            );
        }
        for (row, what) in [
            (at(&rows, "~/work"), "the directory"),
            (at(&rows, "main"), "the branch"),
            (at(&rows, "0e6f2c1a"), "the id"),
        ] {
            assert_eq!(target(&session, row), None, "{what}");
        }
        // Cut to the heading and one name row and the line that hides the panel, the rest of the
        // name is not drawn.
        let short = Rect::new(area.x, 0, WIDTH, heading + 3);
        assert_eq!(
            target_at(&session, short, short.y + heading + 1, inside),
            Some(Target::Name)
        );
        assert_eq!(
            target_at(&session, short, short.y + heading + 2, inside),
            Some(Target::Hide),
            "the row where the second name row would be is the line that hides the panel"
        );
    }

    /// MOUSE-22. Rejects: a target on the Language servers heading or the rows around the servers,
    /// one on a row of another section, and one on a row cut for want of room.
    #[test]
    fn only_a_started_language_servers_rows_are_click_targets() {
        let mut session = Session::new("none");
        session.report_language_servers(Roster::of([Language::Python, Language::Rust]));
        session.servers.started = vec!["notes".to_string()];
        let area = Rect::new(84, 0, WIDTH, 40);
        let rows = lines(&session, area.width, area.height);
        let at = |needle: &str| {
            rows.iter()
                .position(|line| line.to_string().contains(needle))
                .expect(needle) as u16
        };
        let inside = area.x + 3;
        let target = |row: u16| target_at(&session, area, area.y + row, inside);

        let heading = at(t!(panel_language_servers));
        assert_eq!(target(heading), None, "the heading");
        assert_eq!(target(at("pyright-langserver")), Some(Target::Languages));
        assert_eq!(target(at("rust-analyzer")), Some(Target::Languages));
        assert_eq!(
            target(at("rust-analyzer") + 1),
            None,
            "the blank row below the section"
        );
        assert_eq!(target(at("notes")), Some(Target::Servers));
        let short = Rect::new(area.x, 0, WIDTH, at("pyright-langserver"));
        assert_eq!(
            target_at(&session, short, short.y + at("pyright-langserver"), inside),
            None
        );
    }

    /// MOUSE-13. Rejects: a target on the MCP heading or the rows around the servers, and an MCP
    /// target on a language server's row, which `/mcp` does not list.
    #[test]
    fn only_a_started_mcp_servers_rows_are_click_targets() {
        let mut session = Session::new("none");
        session.servers.started = vec!["notes".to_string(), "docs".to_string()];
        session.report_language_servers(Roster::of([Language::Rust]));
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
        assert_ne!(target(at("rust-analyzer")), Some(Target::Servers));
        let short = Rect::new(area.x, 0, WIDTH, at("notes"));
        assert_eq!(
            target_at(&session, short, short.y + at("notes"), inside),
            None
        );
    }

    /// MOUSE-21. Rejects: a target on any row but the last, which names the key, and one on the
    /// border column.
    #[test]
    fn only_the_hide_row_is_a_hide_target() {
        let session = Session::new("none");
        let area = Rect::new(84, 0, WIDTH, 40);
        let inside = area.x + 3;
        let target = |row: u16| target_at(&session, area, area.y + row, inside);
        assert_eq!(target(area.height - 1), Some(Target::Hide));
        assert_eq!(target(area.height - 2), None);
        assert_eq!(
            target_at(&session, area, area.y + area.height - 1, area.x),
            None
        );
    }

    /// A session whose first turn wrote `rows` as its plan and answered, followed by two turns that
    /// each answered at length, so the rows of the plan are far above the end of the transcript.
    fn a_plan_a_long_way_up(rows: Vec<bravebot_core::todo::Row>) -> Session {
        let mut session = working_on(rows);
        session.complete("the first answer", Vec::new(), 0);
        for _ in 0..2 {
            session.type_char('x');
            session.submit();
            let long: Vec<String> = (1..=30).map(|n| format!("answer line {n}")).collect();
            session.complete(long.join("\n\n"), Vec::new(), 0);
        }
        screen(&mut session, 120, 30);
        session.toggle_panel();
        assert!(session.panel_open(), "the panel did not open");
        session
    }

    /// The screen row of the panel's row that holds `needle`, in a frame `columns` wide.
    fn panel_row(rows: &[String], columns: u16, needle: &str) -> u16 {
        rows.iter()
            .position(|row| {
                row.chars()
                    .skip(usize::from(columns - WIDTH))
                    .collect::<String>()
                    .contains(needle)
            })
            .unwrap_or_else(|| panic!("no panel row holds {needle}: {rows:#?}")) as u16
    }

    /// Press and release the left button on one cell, as the event loop would.
    fn clicked(session: &mut Session, row: u16, column: u16) {
        use ratatui::crossterm::event::{MouseButton::Left, MouseEventKind::*};
        for kind in [Down(Left), Up(Left)] {
            crate::app::handle_mouse(session, mouse(kind, row, column));
        }
    }

    /// The part of a screen row that the transcript draws, in a frame `columns` wide.
    fn transcript_part(row: &str, columns: u16) -> String {
        row.chars()
            .take(usize::from(columns - WIDTH))
            .collect::<String>()
    }

    /// MOUSE-31. A click on a task of the plan puts the row its turn drew that task on at the top of
    /// the transcript, whichever task it is, including one after a task so long that the transcript
    /// wraps it. The regression it rejects is a view that goes to the turn's start or end, to the
    /// plan's first row for every task, or to a row counted with each task as one row.
    #[test]
    fn a_click_on_a_task_moves_the_view_to_the_row_its_turn_drew_it_on() {
        let long = "Read the tokenizer and list every place a token is consumed twice, then write \
                    down what each of them does with the quote that opens a string";
        for needle in [
            "Read the tokenizer",
            "Fix it",
            "Write the regression",
            "Update the changelog",
        ] {
            let mut session = a_plan_a_long_way_up(plan(&[
                (long, Status::Done),
                ("Fix it", Status::Done),
                ("Write the regression test", Status::Active),
                ("Update the changelog", Status::Pending),
            ]));
            let rows = screen(&mut session, 120, 30);
            assert!(
                !transcript_part(&rows[0], 120).contains(needle),
                "the task was on the first row before the click"
            );
            clicked(&mut session, panel_row(&rows, 120, needle), 120 - WIDTH + 4);

            let after = screen(&mut session, 120, 30);
            let top = transcript_part(&after[0], 120);
            assert!(top.contains(needle), "{needle}: the view is at {top:?}");
            assert!(
                top.contains('✓') || top.contains('■') || top.contains('□'),
                "{needle}: the top row is not the task's own line: {top:?}"
            );
        }
    }

    /// MOUSE-31. Where the plan is longer than the room the panel has, the rows drawn are a choice of
    /// them ([PANEL-10](../../../docs/specs/info-panel.md#PANEL-10)), and a click on one goes to the
    /// task it draws and not to the one at that place from the start. The regression it rejects is a
    /// row number taken for a task number.
    #[test]
    fn a_click_on_a_task_of_a_plan_longer_than_the_room_goes_to_that_task() {
        let names: Vec<(String, Status)> = (0..20)
            .map(|n| {
                let status = match n {
                    0..=14 => Status::Done,
                    15 => Status::Active,
                    _ => Status::Pending,
                };
                (format!("task {n}"), status)
            })
            .collect();
        let rows: Vec<(&str, Status)> = names.iter().map(|(t, s)| (t.as_str(), *s)).collect();
        let mut session = a_plan_a_long_way_up(plan(&rows));
        let drawn = screen(&mut session, 120, 14);
        let row = panel_row(&drawn, 120, "task 15");
        clicked(&mut session, row, 120 - WIDTH + 4);

        let after = screen(&mut session, 120, 14);
        assert!(
            transcript_part(&after[0], 120)
                .trim_end()
                .ends_with("task 15"),
            "{:?}",
            transcript_part(&after[0], 120)
        );
    }

    /// MOUSE-31. While the turn that wrote the plan is still running the plan is the list it
    /// reported, drawn above the box and not in the transcript, so the view goes to the end of the
    /// transcript, where that turn's words are.
    #[test]
    fn a_click_on_a_task_of_the_turn_in_flight_moves_the_view_to_the_end() {
        let mut session = a_plan_a_long_way_up(plan(&[("Earlier work", Status::Done)]));
        session.type_char('y');
        session.submit();
        session.set_todos(plan(&[
            ("Read the code", Status::Done),
            ("Change the code", Status::Active),
        ]));
        assert_eq!(session.plan_entry(), None, "the plan is the turn's own");
        let rows = screen(&mut session, 120, 30);
        session.scroll_up(25);
        screen(&mut session, 120, 30);
        assert!(session.rows_below() > 0, "the view did not move back");

        clicked(
            &mut session,
            panel_row(&rows, 120, "Read the code"),
            120 - WIDTH + 4,
        );

        assert_eq!(session.rows_below(), 0, "the view did not go to the end");
    }

    /// MOUSE-31. The heading, the row that counts what was left out, a drag over a task, a double
    /// click on one and a click over the scroller leave the view where it was. Each fixture is a
    /// gesture that would move it if its guard were missing.
    #[test]
    fn a_click_that_is_not_on_a_task_leaves_the_view_where_it_was() {
        let names: Vec<(String, Status)> = (0..20)
            .map(|n| (format!("task {n}"), Status::Pending))
            .collect();
        let rows: Vec<(&str, Status)> = names.iter().map(|(t, s)| (t.as_str(), *s)).collect();
        let mut session = a_plan_a_long_way_up(plan(&rows));
        let drawn = screen(&mut session, 120, 14);
        let inside = 120 - WIDTH + 4;
        let before = (session.top_row(), session.rows_below());

        clicked(&mut session, panel_row(&drawn, 120, t!(panel_plan)), inside);
        clicked(&mut session, panel_row(&drawn, 120, "+13 to do"), inside);
        let task = panel_row(&drawn, 120, "task 3");
        dragged(&mut session, (task, inside), (task, inside + 5));
        use ratatui::crossterm::event::{MouseButton::Left, MouseEventKind::*};
        for kind in [Down(Left), Down(Left), Up(Left)] {
            crate::app::handle_mouse(&mut session, mouse(kind, task, inside));
        }
        assert_eq!(
            (session.top_row(), session.rows_below()),
            before,
            "the view moved"
        );

        session.open_scroller();
        clicked(&mut session, task, inside);
        assert_eq!(
            (session.top_row(), session.rows_below()),
            before,
            "a click over the scroller moved the view"
        );
    }

    const LONG_TASK: &str = "Read the tokenizer and list every place a token is consumed twice, then write down what each does";

    /// A plan of a finished task whose words are longer than the panel's row, a finished one that
    /// fits it, one in progress, one waiting, and one dropped with long words of its own, ended by
    /// the turn that wrote it.
    fn a_plan_with_a_long_finished_task() -> Session {
        let mut session = working_on(plan(&[
            (LONG_TASK, Status::Done),
            ("Fix it", Status::Done),
            (
                "Write the regression test for the double read of an escaped quote",
                Status::Active,
            ),
            (
                "Update the changelog and the migration notes for the release after this one",
                Status::Pending,
            ),
            (
                "Try the old grammar, which has the same fault as the one before it did",
                Status::Cancelled,
            ),
        ]));
        session.complete("done", Vec::new(), 0);
        session
    }

    /// A session with the panel open on a terminal 120 columns wide and 40 rows high, drawn once so
    /// the panel's rows are recorded for a click.
    fn with_the_panel_open(mut session: Session) -> Session {
        screen(&mut session, 120, 40);
        session.toggle_panel();
        assert!(session.panel_open(), "the panel did not open");
        screen(&mut session, 120, 40);
        session
    }

    /// What the panel draws on each row of the frame the session is in, drawn again.
    fn drawn_plan(session: &mut Session) -> Vec<String> {
        let rows = screen(session, 120, 40);
        panel_column(&rows, 120)
    }

    /// MOUSE-32. A click on the mark of a finished task whose words the row cuts opens the whole of
    /// them under the mark, wrapped to the row, and a click on the mark or on a row of the words
    /// closes them. The rows after it move down and are the tasks they were. The regression it
    /// rejects is a toggle that changes nothing the person can see, one that cannot be undone, and
    /// rows after the open one that are taken for other tasks.
    #[test]
    fn the_mark_of_a_finished_task_opens_the_whole_of_its_words_and_closes_them_again() {
        let mut session = with_the_panel_open(a_plan_with_a_long_finished_task());
        let panel = session.laid.panes.last().expect("panel").1;
        let at = |rows: &[String], needle: &str| {
            rows.iter()
                .position(|row| row.contains(needle))
                .expect(needle) as u16
        };
        let closed = drawn_plan(&mut session);
        let first = at(&closed, "Read the tokenizer");
        assert!(closed[usize::from(first)].ends_with('…'), "{closed:#?}");
        let (mark, words) = (panel.x + 2, panel.x + 6);
        assert_eq!(
            session.panel_target_at(first, mark),
            Some(Target::Detail(0))
        );
        assert_eq!(session.panel_target_at(first, words), Some(Target::Task(0)));

        clicked(&mut session, first, mark);
        let open = drawn_plan(&mut session);
        let joined: String = open[usize::from(first)..usize::from(first) + 4]
            .iter()
            .map(|row| row.trim_start_matches(['✓', ' ']))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(joined.contains(LONG_TASK), "{open:#?}");
        let fix = at(&open, "Fix it");
        assert!(
            fix > first + 1,
            "the tasks after it did not move down: {open:#?}"
        );
        assert_eq!(session.panel_target_at(fix, words), Some(Target::Task(1)));
        assert_eq!(
            session.panel_target_at(first + 1, words),
            Some(Target::Detail(0)),
            "a row of the open words"
        );

        clicked(&mut session, first + 1, words);
        assert_eq!(
            drawn_plan(&mut session),
            closed,
            "a row of the words did not close them"
        );
        clicked(&mut session, first, mark);
        assert_ne!(
            drawn_plan(&mut session),
            closed,
            "the mark did not open them"
        );
        // A second click on the one cell within the interval is the rest of a double click and does
        // nothing, so the toggle is made by a click of its own.
        session.last_click = None;
        clicked(&mut session, first, mark);
        assert_eq!(
            drawn_plan(&mut session),
            closed,
            "the mark did not close them"
        );
    }

    /// MOUSE-32. The mark of a finished task, whether its words fit the row or not, opens them, and
    /// the mark of a task still to do, in progress or dropped goes to the task as a click on its
    /// words does. A short finished task that is open is read as it is, not struck through.
    /// The regression it rejects is a mark that opens a task whose work is not done, one that
    /// opens only the tasks whose words are cut, which reads the words to decide, and an open task
    /// that is still struck through.
    #[test]
    fn only_the_mark_of_a_finished_task_opens_its_words() {
        use ratatui::style::Modifier;
        let mut session = with_the_panel_open(a_plan_with_a_long_finished_task());
        let panel = session.laid.panes.last().expect("panel").1;
        let (mark, words) = (panel.x + 2, panel.x + 6);
        let rows = drawn_plan(&mut session);
        for (needle, opens) in [
            ("Read the tokenizer", true),
            ("Fix it", true),
            ("Write the regression", false),
            ("Update the changelog", false),
            ("Try the old grammar", false),
        ] {
            let row = rows
                .iter()
                .position(|row| row.contains(needle))
                .expect(needle) as u16;
            assert_eq!(
                matches!(session.panel_target_at(row, mark), Some(Target::Detail(_))),
                opens,
                "{needle}: its mark"
            );
            assert!(
                matches!(session.panel_target_at(row, words), Some(Target::Task(_))),
                "{needle}: its words"
            );
        }

        let struck = |session: &Session| {
            lines(session, WIDTH, 40)
                .iter()
                .find(|line| line.to_string().contains("Fix it"))
                .and_then(|line| line.spans.last())
                .is_some_and(|span| span.style.add_modifier.contains(Modifier::CROSSED_OUT))
        };
        assert!(struck(&session), "a finished task is struck through");
        let fix = rows
            .iter()
            .position(|row| row.contains("Fix it"))
            .expect("row") as u16;
        clicked(&mut session, fix, mark);
        assert!(session.plan_detail_open(1));
        assert!(!struck(&session), "an open task is still struck through");
    }

    /// MOUSE-32. A drag from the mark of a finished task and a click on it over the scroller leave
    /// the task closed. Each fixture is a gesture that would open it if its guard were missing.
    #[test]
    fn a_drag_on_a_mark_or_a_click_on_it_over_the_scroller_leaves_the_task_closed() {
        let mut session = with_the_panel_open(a_plan_with_a_long_finished_task());
        let panel = session.laid.panes.last().expect("panel").1;
        let first = drawn_plan(&mut session)
            .iter()
            .position(|row| row.contains("Read the tokenizer"))
            .expect("the task") as u16;
        let mark = panel.x + 2;

        dragged(&mut session, (first, mark), (first, mark + 8));
        assert!(!session.plan_detail_open(0), "a drag opened it");

        session.open_scroller();
        clicked(&mut session, first, mark);
        assert!(
            !session.plan_detail_open(0),
            "a click over the scroller opened it"
        );
    }

    /// MOUSE-32 and MOUSE-50. A double click on the mark of a finished task, as a terminal reports
    /// one, opens the words once, with its first click, and the second press begins no selection and
    /// does not close them again. The regression it rejects is the second click acting as a click of
    /// its own, which toggles the words open and shut in one gesture.
    #[test]
    fn a_double_click_on_a_mark_opens_the_words_once() {
        let mut session = with_the_panel_open(a_plan_with_a_long_finished_task());
        let panel = session.laid.panes.last().expect("panel").1;
        let first = drawn_plan(&mut session)
            .iter()
            .position(|row| row.contains("Read the tokenizer"))
            .expect("the task") as u16;
        let mark = panel.x + 2;

        session.last_click = None;
        clicked(&mut session, first, mark);
        assert!(
            session.plan_detail_open(0),
            "the first click did not open it"
        );
        clicked(&mut session, first, mark);
        assert!(session.plan_detail_open(0), "the second click closed it");
        assert!(
            session.selection.is_none(),
            "the second press selected a word"
        );
    }

    /// MOUSE-32. An open task takes its rows from the plan: where the plan no longer fits, the
    /// finished tasks farthest from the work still to do give their rows up and are counted as
    /// done, and the task in progress and the open task stay in view. The regression it rejects is
    /// a plan that grows past the panel, a count that does not follow the tasks left out, and an
    /// open task that is left out for the finished tasks beside the work, which would close what
    /// the click opened.
    #[test]
    fn an_open_task_takes_rows_from_the_plan_and_the_count_follows() {
        let mut tasks: Vec<(String, Status)> = vec![(LONG_TASK.to_string(), Status::Done)];
        tasks.extend((1..12).map(|n| {
            (
                format!("task {n}"),
                if n == 11 {
                    Status::Active
                } else {
                    Status::Done
                },
            )
        }));
        let rows: Vec<(&str, Status)> = tasks.iter().map(|(t, s)| (t.as_str(), *s)).collect();
        let mut session = working_on(plan(&rows));
        session.complete("done", Vec::new(), 0);
        let height = 17;
        let shown = |session: &Session| texts(&lines(session, WIDTH, height));

        let closed = shown(&session);
        assert_eq!(closed.len(), usize::from(height));
        assert!(
            closed.iter().any(|row| row.ends_with("task 11")),
            "{closed:#?}"
        );
        assert!(
            !closed.iter().any(|row| row.starts_with('+')),
            "a count was drawn where the whole plan fits: {closed:#?}"
        );

        session.toggle_plan_detail(0);
        let open = shown(&session);
        assert_eq!(open.len(), usize::from(height), "the panel grew");
        assert!(
            open.iter().any(|row| row.ends_with("task 11")),
            "the task in progress left the panel: {open:#?}"
        );
        assert!(
            open.contains(&t!(panel_hidden_done, count = 5).to_string()),
            "the finished tasks left out are not counted: {open:#?}"
        );
        assert!(
            open.iter().any(|row| row.contains("each does")),
            "the open task was left out for the finished tasks beside the work: {open:#?}"
        );
    }

    /// PANEL-10. The panel is laid out once for each frame, and the model writes the plan with no
    /// limit on its length, so the work of a layout has to grow with the length of the plan and not
    /// with its square. A plan of finished tasks that are all open, in progress at the last, is the
    /// one that makes a layout look for the place the rows that fit begin at from every task in turn.
    /// Rejects: a layout that wraps the words of each open task again for each place it tries, which is
    /// as many times as the plan is long for each of its tasks.
    #[test]
    fn the_work_of_laying_out_a_long_plan_grows_with_its_length() {
        const TASKS: usize = 300;
        let mut rows: Vec<(String, Status)> = (0..TASKS)
            .map(|n| (format!("{LONG_TASK} number {n}"), Status::Done))
            .collect();
        rows[TASKS - 1] = ("the last task".to_string(), Status::Active);
        let rows: Vec<(&str, Status)> = rows.iter().map(|(t, s)| (t.as_str(), *s)).collect();
        let mut session = working_on(plan(&rows));
        for task in 0..TASKS - 1 {
            session.toggle_plan_detail(task);
        }
        session.complete("done", Vec::new(), 0);

        let before = WRAPS.with(std::cell::Cell::get);
        let drawn = lines(&session, WIDTH, 20);
        let wraps = WRAPS.with(std::cell::Cell::get) - before;

        assert!(
            texts(&drawn)
                .iter()
                .any(|row| row.ends_with("the last task")),
            "the task in progress is not in view"
        );
        assert!(
            wraps <= 2 * TASKS,
            "{wraps} wraps for a plan of {TASKS} tasks of which {} are open",
            TASKS - 1
        );
    }

    /// MOUSE-32. The mark of a finished task is the glyph and the blank on each side of it, three
    /// cells, and the rest of the row is the task's own. The words a task opens to take at most six
    /// rows, the last cut where more was left, and a click on the first row of them beside the mark
    /// goes to the task and leaves them open, where a click on a row below it closes them. Rejects: a
    /// mark that is the glyph alone or one cell wider than the blanks around it, words opened over
    /// every row they take, and a click on the words of the first row that closes them.
    #[test]
    fn the_mark_is_the_glyph_and_its_two_blanks_and_the_words_open_to_six_rows() {
        let words = "Read the tokenizer and list every place a token is consumed twice, then write \
                     down what each of them does with the quote that opens a string and what comes \
                     after it until the next one closes it, and then read it all again in the order \
                     the file is read, and list what the order changes, and what it leaves, and why \
                     it matters to the tokenizer and to everything that reads from it afterwards";
        let mut session = with_the_panel_open(working_on(plan(&[
            (words, Status::Done),
            ("Fix it", Status::Done),
            ("Write the test", Status::Active),
        ])));
        session.complete("done", Vec::new(), 0);
        let panel = session.laid.panes.last().expect("panel").1;
        let rows = drawn_plan(&mut session);
        let first = rows
            .iter()
            .position(|row| row.contains("Read the tokenizer"))
            .expect("the task") as u16;
        let kinds: Vec<Option<Target>> = (panel.x..panel.x + 8)
            .map(|column| session.panel_target_at(first, column))
            .collect();
        let marks: Vec<usize> = kinds
            .iter()
            .enumerate()
            .filter(|(_, kind)| matches!(kind, Some(Target::Detail(0))))
            .map(|(at, _)| at)
            .collect();
        assert_eq!(marks.len(), 3, "the cells of the mark: {kinds:?}");
        assert!(
            marks.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "the mark is not one run: {marks:?}"
        );
        let after = marks[2] + 1;
        assert_eq!(kinds[after], Some(Target::Task(0)), "{kinds:?}");

        // Opened, the words take six rows and the last of them is cut.
        clicked(&mut session, first, panel.x + marks[1] as u16);
        let open = drawn_plan(&mut session);
        let fix = open
            .iter()
            .position(|row| row.contains("Fix it"))
            .expect("the next task") as u16;
        assert_eq!(
            fix - first,
            6,
            "the words took {} rows: {open:#?}",
            fix - first
        );
        assert!(open[usize::from(fix) - 1].ends_with('…'), "{open:#?}");

        // The rest of the first row goes to the task and leaves them open. A row below it closes them.
        clicked(&mut session, first, panel.x + after as u16);
        assert!(
            session.plan_detail_open(0),
            "the first row closed the words"
        );
        clicked(&mut session, first + 1, panel.x + after as u16);
        assert!(
            !session.plan_detail_open(0),
            "a row below the first did not close them"
        );
    }

    /// MOUSE-32 and MOUSE-50. A report that replaces the plan between the press on the mark of a task
    /// and its release leaves the same place under the pointer holding another task, so the release
    /// is not the end of that click. Rejects: a release that toggles the task now at the place, which
    /// opens the words of a task nobody pressed on.
    #[test]
    fn a_report_between_the_press_and_the_release_toggles_no_task() {
        use ratatui::crossterm::event::{MouseButton::Left, MouseEventKind::*};
        let mut session = with_the_panel_open(a_plan_with_a_long_finished_task());
        let panel = session.laid.panes.last().expect("panel").1;
        let first = drawn_plan(&mut session)
            .iter()
            .position(|row| row.contains("Read the tokenizer"))
            .expect("the task") as u16;
        let mark = panel.x + 2;

        session.last_click = None;
        crate::app::handle_mouse(&mut session, mouse(Down(Left), first, mark));
        session.set_todos(plan(&[
            ("Something else entirely", Status::Done),
            (LONG_TASK, Status::Done),
            ("Write the regression test", Status::Active),
        ]));
        drawn_plan(&mut session);
        crate::app::handle_mouse(&mut session, mouse(Up(Left), first, mark));

        assert!(
            !session.plan_detail_open(0) && !session.plan_detail_open(1),
            "the release opened a task the press was not on: {:?}",
            session.plan_detail()
        );
    }

    /// MOUSE-32. A report that replaces the plan, a new turn and a plan that is dropped close what
    /// was open, since the places are those of the plan before. The turn that wrote the plan ending
    /// leaves it as it was. The regression it rejects is a detail that stays open on whichever task
    /// is now at its place.
    #[test]
    fn what_was_opened_is_closed_when_the_plan_is_replaced_or_dropped() {
        let mut session = working_on(plan(&[(LONG_TASK, Status::Done), ("Next", Status::Active)]));
        session.toggle_plan_detail(0);
        assert!(session.plan_detail_open(0));
        session.set_todos(plan(&[
            ("Other work", Status::Done),
            (LONG_TASK, Status::Done),
        ]));
        assert!(
            !session.plan_detail_open(0),
            "a later report in the turn left it open"
        );

        session.toggle_plan_detail(1);
        session.toggle_plan_detail(7);
        assert!(session.plan_detail_open(1));
        assert!(
            !session.plan_detail_open(7),
            "a place beyond the plan opened"
        );
        session.complete("done", Vec::new(), 0);
        assert!(session.plan_detail_open(1), "the turn ending closed it");

        session.type_char('b');
        session.submit();
        assert!(!session.plan_detail_open(1), "a new turn left it open");

        session.set_todos(plan(&[(LONG_TASK, Status::Done)]));
        session.toggle_plan_detail(0);
        session.complete("done", Vec::new(), 0);
        session.clear();
        assert!(!session.plan_detail_open(0), "/clear left it open");
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

    /// MOUSE-20. Rejects: a Files target on the rows under the section or on the plan, whose rows
    /// belong to their tasks (MOUSE-32), on the border column, and a section whose heading or
    /// totals row is not one.
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
        assert_eq!(
            at(heading + 4, inside),
            Some(Target::Task(0)),
            "the first row of the plan is the first task's, not the section's"
        );
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
            Some(Target::Hide),
            "the totals row is cut"
        );
        assert_eq!(
            target(heading + 1, heading),
            Some(Target::Hide),
            "the heading is cut"
        );

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
        let mut part: Vec<Line<'static>> = all[heading + 1..]
            .iter()
            .take_while(|line| !line.to_string().trim().is_empty())
            .cloned()
            .collect();
        // The row that names the key is not one of the plan's.
        let key = t!(panel_todos, chord = session.bindings().todos_name()).to_string();
        if part
            .last()
            .is_some_and(|line| line.to_string().trim() == key)
        {
            part.pop();
        }
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

    /// A panel of fifteen rows leaves the plan ten: the context section takes two, the blank and
    /// the heading two more, and the key that hides the panel one. Eight of the ten are tasks when
    /// the plan does not fit, since one row names the key that prints the plan whole and one says
    /// what was left out.
    const HEIGHT_FOR_EIGHT_TASKS: u16 = 15;

    /// The height of a panel whose plan has `room` rows: the context section takes two, the blank
    /// and the heading two more, and the key that hides the panel one.
    fn height_for_room(room: u16) -> u16 {
        room + 5
    }

    /// A plan whose unfinished rows are at its end must show them. In a plan of eight rows done and
    /// two pending, a panel with room for fewer than ten rows draws both pending rows and the row
    /// that counts the finished ones it left out, at every room from three rows up. One or two
    /// rows of room cannot hold both pending rows and the count, so the count names the pending
    /// rows it left out. The key that prints the plan whole is named from four rows up, where it
    /// takes the place of neither pending row.
    ///
    /// Rejects: a window that starts at the first row, or at the task in progress with the first
    /// row as the fallback, which draws only finished rows; and a key row that takes the place of
    /// a pending row at three rows of room.
    #[test]
    fn a_long_plan_shows_the_rows_still_to_do_before_the_finished_ones() {
        let rows = plan_of(&[(Status::Done, 8), (Status::Pending, 2)]);
        let key = t!(panel_todos, chord = "alt-a").to_string();
        // The room, the tasks drawn, the row that counts what was left out, and whether the last
        // row names the key.
        let from = |first: usize| (first..=10).collect::<Vec<_>>();
        let table: [(u16, Vec<usize>, Option<&str>, bool); 11] = [
            (1, vec![], Some("+2 to do, +8 done"), false),
            (2, vec![9], Some("+1 to do, +8 done"), false),
            (3, from(9), Some("+8 done"), false),
            (4, from(9), Some("+8 done"), true),
            (5, from(8), Some("+7 done"), true),
            (6, from(7), Some("+6 done"), true),
            (7, from(6), Some("+5 done"), true),
            (8, from(5), Some("+4 done"), true),
            (9, from(4), Some("+3 done"), true),
            (10, from(1), None, false),
            (11, from(1), None, true),
        ];
        for (room, shown, left_out, names_the_key) in table {
            let drawn = panel_for(&rows, height_for_room(room));
            let at = format!("{room} rows of room: {drawn:#?}");
            assert_eq!(tasks_in(&drawn), shown, "{at}");
            assert_eq!(left_out_in(&drawn), left_out, "{at}");
            assert_eq!(drawn.contains(&key), names_the_key, "{at}");
        }

        // A panel with the room to spare draws the whole plan and counts nothing.
        let drawn = panel_for(&rows, 30);
        assert_eq!(tasks_in(&drawn), (1..=10).collect::<Vec<_>>(), "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), None, "{drawn:#?}");
    }

    /// The key row never takes the place of a row still to do: at every room from three rows up,
    /// in plans with every number of rows still to do, pending or in progress and in either order,
    /// the rows still to do drawn are as many as the room holds beside the row that counts the
    /// rest, whether or not the key is named.
    ///
    /// Rejects: a key row drawn wherever the room has three rows, which hides a row still to do that
    /// the room holds without it, and a choice of whether to draw it that counts only the pending
    /// rows, which draws it over a task in progress.
    #[test]
    fn the_key_row_never_takes_the_place_of_a_row_still_to_do() {
        use Status::{Active, Done, Pending};
        // Rows still to do after the finished ones: only pending rows, only rows in progress, and a
        // mix with the row in progress between pending rows.
        type Shape = fn(usize) -> Vec<(Status, usize)>;
        let shapes: [Shape; 3] = [
            |n| vec![(Pending, n)],
            |n| vec![(Active, n.min(2)), (Pending, n - n.min(2))],
            |n| vec![(Pending, n / 2), (Active, 1), (Pending, n - n / 2 - 1)],
        ];
        for shape in shapes {
            for to_do in 1..=12usize {
                for finished in [0usize, 3, 20] {
                    let mut runs = vec![(Done, finished)];
                    runs.extend(shape(to_do));
                    runs.retain(|(_, count)| *count > 0);
                    let rows = plan_of(&runs);
                    let outstanding: Vec<usize> = rows
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, status))| *status != Done)
                        .map(|(at, _)| at + 1)
                        .collect();
                    assert_eq!(outstanding.len(), to_do, "{runs:?}");
                    for room in 3..=14u16 {
                        let drawn = panel_for(&rows, height_for_room(room));
                        let room = usize::from(room);
                        let shown = tasks_in(&drawn)
                            .into_iter()
                            .filter(|task| outstanding.contains(task))
                            .count();
                        // Without the key row the plan has the room less the row that counts what
                        // it left out, where it does not fit whole.
                        let held = if rows.len() > room { room - 1 } else { room };
                        assert_eq!(
                            shown,
                            outstanding.len().min(held),
                            "{runs:?}, {room} rows of room: {drawn:#?}"
                        );
                    }
                }
            }
        }
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
    /// pending rows after it are the ones left out, and the row says they are still to do. The
    /// room has no row for the key that prints the plan whole, which would take a row still to do.
    ///
    /// Rejects: taking the first rows that are not finished, which drops a task in progress that
    /// comes after enough pending rows, counting what it left out as `done`, and a key row that
    /// takes the place of a row still to do.
    #[test]
    fn the_task_in_progress_is_kept_when_there_are_more_rows_still_to_do_than_room() {
        let rows = plan_of(&[
            (Status::Done, 3),
            (Status::Pending, 5),
            (Status::Active, 1),
            (Status::Pending, 5),
        ]);

        let drawn = panel_for(&rows, HEIGHT_FOR_EIGHT_TASKS - 5);
        // Four rows and the row that counts: the task in progress and the first three pending
        // rows.
        assert_eq!(tasks_in(&drawn), [4, 5, 6, 9], "{drawn:#?}");
        assert_eq!(
            left_out_in(&drawn),
            Some(
                format!(
                    "{}, {}",
                    t!(panel_hidden_to_do, count = 7),
                    t!(panel_hidden_done, count = 3)
                )
                .as_str()
            ),
            "{drawn:#?}"
        );
        assert!(
            !drawn.iter().any(|row| row.contains("whole plan")),
            "the key row took a row still to do: {drawn:#?}"
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
        assert_eq!(tasks_in(&drawn), [6, 7, 8, 9, 10], "{drawn:#?}");
        assert_eq!(
            left_out_in(&drawn),
            Some(said(2, 3, 2).as_str()),
            "{drawn:#?}"
        );
        assert!(
            said(2, 3, 2).find("to do") < said(2, 3, 2).find("done"),
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
            counted.starts_with(&t!(panel_hidden_to_do, count = 2).to_string())
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

        // Six rows of room hold four tasks beside the row that counts and the row that names the
        // key: the two rows to do and two finished ones. The row before them is nearest. Rows 5 and
        // 9 are two places away and one is left, so the tie shows: the earlier.
        let drawn = panel_for(&rows, height_for_room(6));
        assert_eq!(tasks_in(&drawn), [5, 6, 7, 8], "{drawn:#?}");

        // One row more takes the later of the two as well.
        let drawn = panel_for(&rows, height_for_room(7));
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
        let drawn = panel_for(&nine, HEIGHT_FOR_EIGHT_TASKS - 1);
        assert_eq!(tasks_in(&drawn), (1..=9).collect::<Vec<_>>(), "{drawn:#?}");
        assert_eq!(left_out_in(&drawn), None, "{drawn:#?}");

        // Fifty rows with five still to do, on a panel that holds eight tasks beside the key row:
        // the five and the three finished rows before them, and the key row after the count.
        let few_to_do = plan_of(&[
            (Status::Done, 45),
            (Status::Active, 1),
            (Status::Pending, 4),
        ]);
        let drawn = panel_for(&few_to_do, HEIGHT_FOR_EIGHT_TASKS);
        assert_eq!(
            tasks_in(&drawn),
            (43..=50).collect::<Vec<_>>(),
            "{drawn:#?}"
        );
        assert_eq!(left_out_in(&drawn), Some("+42 done"), "{drawn:#?}");
        assert_eq!(
            drawn[drawn.len() - 2],
            t!(panel_todos, chord = session.bindings().todos_name()),
            "{drawn:#?}"
        );

        // Fifty rows with ten still to do, which leave the panel no row for the key.
        let fifty = plan_of(&[
            (Status::Done, 40),
            (Status::Active, 1),
            (Status::Pending, 9),
        ]);
        let drawn = panel_for(&fifty, HEIGHT_FOR_EIGHT_TASKS);
        assert_eq!(
            tasks_in(&drawn),
            (41..=49).collect::<Vec<_>>(),
            "{drawn:#?}"
        );
        assert_eq!(
            left_out_in(&drawn),
            Some(
                format!(
                    "{}, {}",
                    t!(panel_hidden_to_do, count = 1),
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

    /// The plan's last row names the key that prints it whole, the chord in force, as the panel's
    /// last row names the key that hides it. It is drawn where the plan has three rows of room or
    /// more, does not fill them exactly and has every row still to do drawn beside it, and below
    /// the row that says what was left out where rows were.
    ///
    /// Rejects: no row at all, the default named after the key was moved, a row that takes the
    /// place of a task a plan that fits had, a row that crowds out the row that counts what was
    /// left out or the plan's own rows where there are only one or two to give, and one that is
    /// left out at three rows, where a task, the count and the key all fit.
    #[test]
    fn the_last_row_of_the_plan_names_the_key_that_prints_it_whole() {
        let key = |session: &Session| {
            t!(panel_todos, chord = session.bindings().todos_name()).to_string()
        };
        let heading = t!(panel_plan).to_string();
        let under_the_heading = |drawn: &[String]| -> Vec<String> {
            let at = drawn
                .iter()
                .position(|row| *row == heading)
                .expect("a heading");
            drawn[at + 1..]
                .iter()
                .take_while(|row| !row.is_empty() && !row.contains("hide panel"))
                .cloned()
                .collect()
        };

        // A plan with room to spare ends in the key.
        let session = working_on(plan(&[("one", Status::Active), ("two", Status::Pending)]));
        let drawn = texts(&lines(&session, WIDTH, 20));
        let part = under_the_heading(&drawn);
        assert_eq!(part.len(), 3, "{drawn:#?}");
        assert_eq!(part.last(), Some(&key(&session)), "{drawn:#?}");
        assert_eq!(key(&session), "alt-a whole plan");

        // A plan cut to the room ends in the key, below the row that says what was left out.
        let rows = plan_of(&[(Status::Done, 20), (Status::Active, 1)]);
        let session = working_on(plan(
            &rows
                .iter()
                .map(|(text, status)| (text.as_str(), *status))
                .collect::<Vec<_>>(),
        ));
        let drawn = texts(&lines(&session, WIDTH, HEIGHT_FOR_EIGHT_TASKS));
        let part = under_the_heading(&drawn);
        assert_eq!(part.len(), 10, "{drawn:#?}");
        assert_eq!(part.last(), Some(&key(&session)), "{drawn:#?}");
        assert!(part[8].starts_with('+'), "{drawn:#?}");

        // Three rows of room are the least that name the key: the one task still to do, the row
        // that counts the eight finished tasks it leaves out, and the key.
        let rows = plan_of(&[(Status::Done, 8), (Status::Pending, 1)]);
        let session = working_on(plan(
            &rows
                .iter()
                .map(|(text, status)| (text.as_str(), *status))
                .collect::<Vec<_>>(),
        ));
        let drawn = texts(&lines(&session, WIDTH, height_for_room(3)));
        let part = under_the_heading(&drawn);
        assert_eq!(tasks_in(&drawn), [9], "{drawn:#?}");
        assert_eq!(part.len(), 3, "{drawn:#?}");
        assert_eq!(part[1], "+8 done", "{drawn:#?}");
        assert_eq!(part[2], key(&session), "{drawn:#?}");

        // A plan that fills the room exactly keeps every task and names no key, and one with no
        // room for three rows names no key and keeps what it counts.
        let nine = plan_of(&[(Status::Done, 8), (Status::Active, 1)]);
        let drawn = panel_for(&nine, HEIGHT_FOR_EIGHT_TASKS - 1);
        assert_eq!(tasks_in(&drawn), (1..=9).collect::<Vec<_>>(), "{drawn:#?}");
        assert!(
            !drawn.contains(&key(&working_on(Vec::new()))),
            "the key took a task's row: {drawn:#?}"
        );
        for height in [6, 7] {
            let drawn = panel_for(&nine, height);
            assert!(
                !drawn.contains(&key(&working_on(Vec::new()))),
                "a plan with one or two rows to give named the key: {drawn:#?}"
            );
            assert!(left_out_in(&drawn).is_some(), "{drawn:#?}");
        }

        // A plan with every task finished, which the key row could take a task from without
        // taking a row still to do, names no key either where there is room for a task and the
        // count and no more.
        let finished = plan_of(&[(Status::Done, 12)]);
        for (height, tasks) in [(6, 0), (7, 1)] {
            let drawn = panel_for(&finished, height);
            assert_eq!(tasks_in(&drawn).len(), tasks, "{drawn:#?}");
            assert!(
                !drawn.contains(&key(&working_on(Vec::new()))),
                "a plan with {tasks} rows of room named the key: {drawn:#?}"
            );
        }

        // The chord in force is the one named.
        let mut session = working_on(plan(&[("one", Status::Active)]));
        let mut moved = std::collections::BTreeMap::new();
        moved.insert("todos".to_string(), "alt-y".to_string());
        session.adopt_keybindings(&moved);
        let drawn = texts(&lines(&session, WIDTH, 20));
        assert!(
            drawn.contains(&"alt-y whole plan".to_string()),
            "{drawn:#?}"
        );
        assert!(
            !drawn.iter().any(|row| row.contains("alt-a")),
            "the default is named after the key was moved: {drawn:#?}"
        );
    }

    /// The row that names the key is cut to the width of the panel with an ellipsis, as the row
    /// that counts is, so a chord longer than the panel is wide does not run past its edge.
    ///
    /// Rejects: a key row drawn at its whole length, which a panel 24 columns wide clips in the
    /// middle of a word.
    #[test]
    fn the_row_naming_the_key_is_cut_to_the_width_of_a_narrow_panel() {
        let mut session = working_on(plan(&[("one", Status::Active), ("two", Status::Pending)]));
        let mut moved = std::collections::BTreeMap::new();
        moved.insert("todos".to_string(), "ctrl-alt-pgdn".to_string());
        session.adopt_keybindings(&moved);
        let whole = t!(panel_todos, chord = "ctrl-alt-pgdn").to_string();
        assert_eq!(session.bindings().todos_name(), "ctrl-alt-pgdn");

        let narrowest = lines(&session, NARROWEST_PANEL, 20);
        let drawn = narrowest
            .iter()
            .find(|line| line.to_string().contains("ctrl-alt-pgdn"))
            .expect("a row naming the key");
        let room = usize::from(NARROWEST_PANEL) - usize::from(INSET);
        assert!(
            drawn.width() <= room + 1 && drawn.to_string().trim().ends_with('…'),
            "the row is not cut to the panel: {:?}",
            drawn.to_string()
        );
        assert!(
            whole.starts_with(drawn.to_string().trim().trim_end_matches('…')),
            "the cut row is not the start of the whole one: {:?}",
            drawn.to_string()
        );

        // A panel wide enough for the whole row draws it whole.
        let wide = texts(&lines(&session, WIDTH, 20));
        assert!(wide.contains(&whole), "{wide:#?}");
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
