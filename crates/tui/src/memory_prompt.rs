//! Edit live core memory rows stored under `~/.bravebot/agent_memory`.
//!
//! Opened by `/memory`. The person edits the list directly; nothing here goes through the planner.

use bravebot_agent::memory::{CoreRow, MAX_LIVE_CORE_ROWS, replace_live_core};
use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::theme;

/// Edit core memory in a centred panel. Returns whether anything was saved.
///
/// `edit_line` hands the text to the user's editor. `Ok(Some)` is what they saved, `Ok(None)` is
/// a quiet cancel, and `Err` is shown in the panel.
pub fn edit<B: Backend, E>(
    terminal: &mut Terminal<B>,
    auto_memory_enabled: bool,
    mut behind: impl FnMut(&mut Frame),
    mut edit_line: E,
) -> bool
where
    E: FnMut(&mut Terminal<B>, &str) -> Result<Option<String>, String>,
{
    if !auto_memory_enabled {
        show_note(terminal, &mut behind, t!(memory_disabled));
        return false;
    }
    let settings = bravebot_config::Settings::load();
    let store = bravebot_agent::memory::store_from_settings(&settings);
    if store.is_none() {
        show_note(terminal, &mut behind, t!(memory_no_home));
        return false;
    }
    let store = store.expect("checked");
    let read_only = bravebot_agent::home::writable().is_none();
    let mut rows = bravebot_agent::memory::load_live_core(&store);
    let mut selected = 0usize;
    let mut dirty = false;

    loop {
        if terminal
            .draw(|frame| {
                behind(frame);
                draw(frame, &rows, selected, dirty, read_only);
            })
            .is_err()
        {
            return false;
        }

        let Ok(TermEvent::Key(key)) = event::read() else {
            continue;
        };
        if key.kind != event::KeyEventKind::Press {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return false;
        }

        match key.code {
            KeyCode::Esc => {
                if read_only {
                    return false;
                }
                rows.retain(|row| !row.text.trim().is_empty());
                return dirty && save(&store, &rows);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !rows.is_empty() {
                    selected = (selected + 1).min(rows.len() - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                selected = selected.saturating_sub(1);
            }
            KeyCode::Char('d') if !read_only => {
                if rows.is_empty() {
                    continue;
                }
                rows.remove(selected);
                if selected >= rows.len() && !rows.is_empty() {
                    selected = rows.len() - 1;
                }
                dirty = true;
            }
            KeyCode::Char('a') if !read_only => {
                if rows.len() >= MAX_LIVE_CORE_ROWS {
                    show_note(terminal, &mut behind, t!(memory_at_cap));
                    continue;
                }
                rows.push(CoreRow::new(bravebot_agent::memory::CORE_MTYPE_FACT, String::new()));
                selected = rows.len() - 1;
                match edit_line(terminal, "") {
                    Ok(Some(text)) if text.trim().is_empty() => {
                        rows.pop();
                    }
                    Ok(Some(text)) => {
                        rows[selected].text = text;
                        dirty = true;
                    }
                    Ok(None) => {
                        rows.pop();
                    }
                    Err(message) => {
                        rows.pop();
                        show_note(terminal, &mut behind, &message);
                    }
                }
                if selected >= rows.len() && !rows.is_empty() {
                    selected = rows.len() - 1;
                }
            }
            KeyCode::Char('e') if !read_only => {
                if rows.is_empty() {
                    continue;
                }
                let current = rows[selected].text.clone();
                match edit_line(terminal, &current) {
                    Ok(Some(text)) => {
                        rows[selected].text = text;
                        dirty = true;
                    }
                    Ok(None) => {}
                    Err(message) => show_note(terminal, &mut behind, &message),
                }
            }
            KeyCode::Char('m') if !read_only => {
                if rows.is_empty() {
                    continue;
                }
                if let Some(mtype) = choose_mtype(terminal, &mut behind, &rows[selected].mtype) {
                    rows[selected].mtype = mtype;
                    dirty = true;
                }
            }
            KeyCode::Enter if !read_only => {
                rows.retain(|row| !row.text.trim().is_empty());
                if dirty {
                    if save(&store, &rows) {
                        return true;
                    }
                    show_note(terminal, &mut behind, t!(memory_not_writable));
                }
            }
            _ => {}
        }
    }
}

/// Pick fact or preference over the session.
fn choose_mtype<B: Backend>(
    terminal: &mut Terminal<B>,
    behind: &mut impl FnMut(&mut Frame),
    current: &str,
) -> Option<String> {
    let types = bravebot_agent::memory::CORE_MTYPES;
    let mut selected = types
        .iter()
        .position(|name| *name == current)
        .unwrap_or(0);

    loop {
        if terminal
            .draw(|frame| {
                behind(frame);
                draw_mtype_picker(frame, types, selected);
            })
            .is_err()
        {
            return None;
        }

        let Ok(TermEvent::Key(key)) = event::read() else {
            continue;
        };
        if key.kind != event::KeyEventKind::Press {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return None;
        }

        match key.code {
            KeyCode::Esc => return None,
            KeyCode::Enter => return Some(types[selected].to_string()),
            KeyCode::Char('j') | KeyCode::Down => {
                selected = (selected + 1).min(types.len() - 1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                selected = selected.saturating_sub(1);
            }
            _ => {}
        }
    }
}

fn draw_mtype_picker(frame: &mut Frame, types: &[&str], selected: usize) {
    let area = centred(frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::brand_primary()))
        .title(format!(" {} ", t!(memory_mtype_title)))
        .style(Style::default().bg(theme::background()).fg(theme::text()));
    let inside = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();
    for (index, name) in types.iter().enumerate() {
        let marker = if index == selected { "›" } else { " " };
        let style = if index == selected {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let hint = match *name {
            bravebot_agent::memory::CORE_MTYPE_FACT => t!(memory_mtype_hint_fact),
            bravebot_agent::memory::CORE_MTYPE_PREFERENCE => t!(memory_mtype_hint_preference),
            _ => "",
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {name}"), style),
            Span::styled(format!("  {hint}"), Style::default().fg(theme::muted())),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        t!(memory_mtype_keys),
        Style::default().fg(theme::muted()),
    ));

    frame.render_widget(Paragraph::new(lines), inside);
}

fn save(store: &std::path::Path, rows: &[CoreRow]) -> bool {
    if bravebot_agent::home::writable().is_none() {
        return false;
    }
    replace_live_core(store, rows).is_ok()
}

fn show_note<B: Backend>(
    terminal: &mut Terminal<B>,
    behind: &mut impl FnMut(&mut Frame),
    message: &str,
) {
    loop {
        if terminal
            .draw(|frame| {
                behind(frame);
                let area = centred(frame.area());
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme::brand_primary()))
                    .title(format!(" {} ", t!(memory_title)))
                    .style(Style::default().bg(theme::background()).fg(theme::text()));
                frame.render_widget(Clear, area);
                frame.render_widget(
                    Paragraph::new(message).wrap(Wrap { trim: false }).block(block),
                    area,
                );
            })
            .is_err()
        {
            return;
        }
        let Ok(TermEvent::Key(key)) = event::read() else {
            continue;
        };
        if key.kind == event::KeyEventKind::Press {
            return;
        }
    }
}

fn draw(frame: &mut Frame, rows: &[CoreRow], selected: usize, dirty: bool, read_only: bool) {
    let area = centred(frame.area());
    frame.render_widget(Clear, area);
    let title = if read_only {
        format!(" {} ({}) ", t!(memory_title), t!(memory_read_only_tag))
    } else if dirty {
        format!(" {} * ", t!(memory_title))
    } else {
        format!(" {} ", t!(memory_title))
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::brand_primary()))
        .title(title)
        .style(Style::default().bg(theme::background()).fg(theme::text()));
    let inside = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();
    if rows.is_empty() {
        lines.push(Line::styled(
            if read_only {
                t!(memory_empty_read_only)
            } else {
                t!(memory_empty)
            },
            Style::default().fg(theme::muted()),
        ));
    } else {
        for (index, row) in rows.iter().enumerate() {
            let marker = if index == selected { "›" } else { " " };
            let style = if index == selected {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let mtype = display_mtype(&row.mtype);
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::styled(format!("[{}] ", mtype), Style::default().fg(theme::muted())),
                Span::styled(row.text.clone(), style),
            ]));
        }
    }

    let keys = if read_only {
        t!(memory_keys_read_only)
    } else {
        t!(memory_keys)
    };

    if read_only {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(2)])
            .split(inside);
        frame.render_widget(
            Paragraph::new(t!(memory_read_only)).style(Style::default().fg(theme::muted())),
            layout[0],
        );
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), layout[1]);
        frame.render_widget(
            Paragraph::new(keys).style(Style::default().fg(theme::muted())),
            layout[2],
        );
    } else {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(inside);
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), layout[0]);
        frame.render_widget(
            Paragraph::new(keys).style(Style::default().fg(theme::muted())),
            layout[1],
        );
    }
}

/// Map a legacy or model-chosen label to the nearest offered type for display.
fn display_mtype(mtype: &str) -> String {
    if bravebot_agent::memory::CORE_MTYPES.contains(&mtype) {
        mtype.to_string()
    } else {
        format!("{mtype}*")
    }
}

fn centred(area: Rect) -> Rect {
    let width = area.width.min(72);
    let height = area.height.min(20);
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    Rect::new(x, y, width, height)
}
