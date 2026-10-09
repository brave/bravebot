//! The info panel beside the transcript: what the session is, how full its context is, and its
//! plan ([PANEL-5](../../../docs/specs/info-panel.md#PANEL-5) onward).
//!
//! Everything here is drawn from what the person typed, the driver's own counters, the planner's
//! task list and the directory the session runs in, and the links the person gave it. Nothing a
//! tool returned reaches it.

use crate::state::Session;
use crate::theme;
use bravebot_i18n::t;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Columns the panel takes, its border included.
pub const WIDTH: u16 = 36;

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
fn ending(text: &str, width: usize) -> String {
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
fn link_row(label: &str, url: &str, width: usize) -> Line<'static> {
    let room = width.saturating_sub(crate::wrap::display_width(label) + 1);
    Line::from(vec![
        Span::styled(format!(" {label} "), dim()),
        Span::raw(ending(&crate::render::printable(url), room)),
    ])
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
    let height = usize::from(height);
    if height == 0 {
        return Vec::new();
    }
    let text = text_width(width);
    let mut body = head(session, text);

    let languages = session.language_servers().programs();
    if !languages.is_empty() {
        let rows = languages
            .iter()
            .map(|program| row((*program).to_string(), Style::default()))
            .collect();
        section(&mut body, t!(panel_language_servers).to_string(), rows);
    }
    if !session.servers.started.is_empty() {
        let rows = session
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
        section(&mut body, t!(panel_mcp_servers).to_string(), rows);
    }

    let plan = session.plan();
    // The blank row and the heading come out of the room before the rows do.
    let room = (height - 1).saturating_sub(body.len() + 2);
    if !plan.is_empty() && room > 0 {
        section(
            &mut body,
            t!(panel_plan).to_string(),
            plan_rows(plan, text, room),
        );
    }

    body.truncate(height - 1);
    body.resize(height - 1, Line::raw(""));
    body.push(row(
        t!(panel_hide, chord = session.bindings().panel_name()).to_string(),
        dim(),
    ));
    body
}

/// Whether a frame `columns` by `rows` draws the panel with its Context section whole.
///
/// The hint line hands the context reading and the cache rate to the panel only then. On a terminal
/// too short for the sections above it, the panel cuts the Context section, and the line keeps
/// both rather than leave the reading on neither.
pub fn shows_context(session: &Session, columns: u16, rows: u16) -> bool {
    // The hint line takes one row and the key that hides the panel another.
    drawn(session, columns) && head(session, text_width(WIDTH)).len() + 2 <= usize::from(rows)
}

/// One column for the border and one for the space after it.
fn text_width(width: u16) -> usize {
    usize::from(width).saturating_sub(2).max(1)
}

/// The Session, Goal, Links and Context sections, each left out where it has nothing to say but for
/// the context, which always has a reading.
fn head(session: &Session, text: usize) -> Vec<Line<'static>> {
    let mut body: Vec<Line<'static>> = Vec::new();

    let identity = session.identity();
    let mut about = Vec::new();
    for name_row in wrapped(&identity.name, text) {
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
    if !about.is_empty() {
        section(&mut body, t!(panel_session).to_string(), about);
    }

    let mut model = Vec::new();
    if let Some(chosen) = session.model() {
        model.push(row(
            ending(&crate::render::printable(chosen), text),
            Style::default(),
        ));
    }
    if let Some(level) = session.effort_in_force() {
        model.push(row(
            t!(panel_effort, level = level.as_str()).to_string(),
            dim(),
        ));
    }
    if !model.is_empty() {
        section(&mut body, t!(panel_model).to_string(), model);
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

    let links: Vec<Line<'static>> = [
        (t!(panel_pull_request), &identity.pull_request),
        (t!(panel_issue), &identity.issue),
    ]
    .into_iter()
    .filter_map(|(label, url)| Some(link_row(label, url.as_deref()?, text)))
    .collect();
    if !links.is_empty() {
        section(&mut body, t!(panel_links).to_string(), links);
    }

    let mut context = vec![row(
        crate::render::context_reading(session),
        Style::default(),
    )];
    let spent = session.spent_tokens();
    if spent > 0 {
        context.push(row(
            t!(panel_spent, tokens = crate::status::tokens(spent)).to_string(),
            dim(),
        ));
    }
    if let Some(rate) = crate::render::cache_hit_rate(session) {
        context.push(row(rate, dim()));
    }
    if let Some(cached) = session.cached() {
        context.push(row(
            t!(
                panel_cache_read,
                tokens = crate::status::tokens(cached.read_tokens)
            )
            .to_string(),
            dim(),
        ));
        context.push(row(
            t!(
                panel_cache_written,
                tokens = crate::status::tokens(cached.written_tokens)
            )
            .to_string(),
            dim(),
        ));
    }
    section(&mut body, t!(panel_context).to_string(), context);
    body
}

/// The plan in at most `room` rows, the task being worked on among them.
///
/// Where the rows do not all fit, the visible part starts at the task in progress if it would
/// otherwise fall below the cut, since what is being done now is what somebody glancing at the
/// panel is looking for. The last row counts the rows left out above the visible part apart from
/// those below it, so a count is never read as work still to come when it is work already past.
fn plan_rows(plan: &[bravebot_core::todo::Row], width: usize, room: usize) -> Vec<Line<'static>> {
    let (start, shown) = if plan.len() <= room {
        (0, plan.len())
    } else {
        let shown = room - 1;
        let active = plan
            .iter()
            .position(|row| row.status == bravebot_core::todo::Status::Active)
            .unwrap_or(0);
        let start = if active < shown {
            0
        } else {
            active.min(plan.len() - shown)
        };
        (start, shown)
    };
    let mut lines: Vec<Line<'static>> = plan[start..start + shown]
        .iter()
        .map(|task| {
            let (marker, text) = if task.status == bravebot_core::todo::Status::Cancelled {
                (dim(), dim().add_modifier(Modifier::CROSSED_OUT))
            } else if task.struck() {
                (
                    Style::default().fg(theme::ok()),
                    dim().add_modifier(Modifier::CROSSED_OUT),
                )
            } else {
                (Style::default().fg(theme::running()), Style::default())
            };
            let content = crate::status::cut(
                &crate::render::printable(&task.content),
                width.saturating_sub(2),
            );
            Line::from(vec![
                Span::styled(format!(" {} ", task.marker), marker),
                Span::styled(content, text),
            ])
        })
        .collect();
    let later = plan.len() - start - shown;
    let count = match (start, later) {
        (0, 0) => None,
        (0, later) => Some(t!(panel_more, count = later)),
        (earlier, 0) => Some(t!(panel_earlier, count = earlier)),
        (earlier, later) => Some(t!(panel_earlier_and_more, earlier = earlier, later = later)),
    };
    if let Some(count) = count {
        lines.push(row(count.to_string(), dim()));
    }
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
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let mut laid = None;
        terminal
            .draw(|frame| laid = Some(crate::render::draw(frame, session)))
            .expect("draw succeeds");
        session.note_layout(laid.expect("the frame was drawn"));
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
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
    /// the name would draw them the same.
    #[test]
    fn a_long_link_keeps_its_end() {
        let mut session = Session::new("none");
        session.link(
            None,
            Some("https://github.com/an-organisation/a-long-repository-name/pull/12345"),
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

    /// The task being worked on is what somebody glancing at the panel looks for, so a plan cut
    /// to the room keeps it in view. The rows left out above it are counted apart from those below
    /// it, so a count of finished work is not read as work still to come.
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

    #[test]
    fn a_plan_longer_than_the_room_keeps_the_task_in_progress_and_counts_each_side_of_it() {
        let drawn_with = |tasks: usize| {
            let rows: Vec<(String, Status)> = (1..=tasks)
                .map(|n| {
                    let status = match n {
                        1..=15 => Status::Done,
                        16 => Status::Active,
                        _ => Status::Pending,
                    };
                    (format!("task {n}"), status)
                })
                .collect();
            let borrowed: Vec<(&str, Status)> = rows
                .iter()
                .map(|(text, status)| (text.as_str(), *status))
                .collect();
            texts(&lines(&working_on(plan(&borrowed)), WIDTH, 14))
        };
        let shown = |drawn: &[String]| -> Vec<String> {
            drawn
                .iter()
                .filter_map(|row| row.find("task ").map(|at| row[at..].to_string()))
                .collect()
        };

        // Fourteen rows: the context section takes two, the blank and the heading two more, the
        // key that hides the panel one, and the count one, which leaves eight tasks.
        let both = drawn_with(30);
        assert_eq!(both.len(), 14, "the panel is not the height it was given");
        assert_eq!(
            shown(&both),
            (16..=23).map(|n| format!("task {n}")).collect::<Vec<_>>(),
            "the rows shown do not start at the task in progress: {both:#?}"
        );
        assert!(
            both.contains(&t!(panel_earlier_and_more, earlier = 15, later = 7).to_string()),
            "the rows above and below are not counted apart: {both:#?}"
        );

        let above = drawn_with(20);
        assert_eq!(
            shown(&above),
            (13..=20).map(|n| format!("task {n}")).collect::<Vec<_>>(),
            "the last rows of the plan are not the ones shown: {above:#?}"
        );
        assert!(
            above.contains(&t!(panel_earlier, count = 12).to_string()),
            "rows left out above are counted as more to come: {above:#?}"
        );
    }
}
