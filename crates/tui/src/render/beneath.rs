//! The rows between the box and the hint line: what is attached, stashed and queued, and the
//! commands, skills, files and shortcuts on offer.

use bravebot_i18n::t;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::keybindings::Keybindings;
use crate::state::{Bound, Session, Status};
use crate::theme;
use crate::wrap;

use super::{dim, head_of, printable, tail_of};

/// How a command is written in the list: the word, and what follows it.
fn command_word(command: &crate::command_table::Command) -> String {
    if command.argument.is_empty() {
        command.name.to_string()
    } else {
        format!("{} {}", command.name, command.argument)
    }
}

/// What is attached to the line being typed, one per row.
///
/// The marker as it appears in the line, then the path it stands for. The path is shown because
/// the marker is deliberately short and a user about to send a file should be able to see which
/// file it is: `[Image #1]` alone is not enough to tell a screenshot from a private key.
///
/// A filename is content, and this is content reaching a screen rather than a model. It is the
/// user's own drop being read back to them, so nothing here is a decision and nothing is labelled.
fn attached_lines(session: &Session, width: u16) -> Vec<Line<'static>> {
    session
        .attached_to_the_line()
        .map(|attached| {
            let lead = format!("  {} ", attached.marker);
            let room = (width as usize).saturating_sub(lead.chars().count());
            Line::from(vec![
                Span::styled(lead, Style::default().fg(theme::brand_primary())),
                Span::styled(printable(&tail_of(&attached.shown, room)), dim()),
            ])
        })
        .collect()
}

/// The line put away, on one row, with the key that brings it back.
///
/// Drawn because the key took a line off the screen. Without a row saying where it went, a press
/// that emptied the box is indistinguishable from one that threw the words away, and the only way
/// to find out which it was is to press the key again and hope.
///
/// One row however long the line was: this is a reminder that something is waiting, not the line
/// itself, and a paragraph put away would push the transcript off the screen to say so.
///
/// The words come before the key where the two cannot both fit. Which line is waiting is the part
/// only this row can say; the key is also in the list `?` puts up, and cut to `ctrl-s to bring i` it
/// is worse than absent. Below the width that holds a legible few words of the line, the reminder
/// goes and the line keeps the row.
///
/// No row may run past the edge, for the reason the shortcut list may not: a row that wraps takes a
/// row the layout did not reserve and pushes the hint line off the screen.
///
/// The text is the user's own words on their way back to them, so nothing here is a decision and
/// nothing is labelled. `printable` all the same, since a line can be pasted and a paste can carry
/// anything.
pub(super) fn stashed_lines(session: &Session, width: u16) -> Vec<Line<'static>> {
    /// Enough of the line to recognise it by. Below this the reminder is dropped to make room.
    const LEGIBLE: usize = 12;

    let Some(stashed) = session.stashed() else {
        return Vec::new();
    };

    let lead = "  stashed ";
    let trail_string = format!("  {} to bring it back", session.bindings().stash_name());
    let after_lead = (width as usize).saturating_sub(lead.chars().count());
    // Nothing of the line itself would fit, so the row would say only that something is stashed
    // without saying what. A terminal this narrow has no room to spare for that.
    if after_lead == 0 {
        return Vec::new();
    }

    let with_trail = after_lead.saturating_sub(trail_string.chars().count());
    let (room, trail) = if with_trail >= LEGIBLE {
        (with_trail, trail_string)
    } else {
        (after_lead, String::new())
    };

    vec![Line::from(vec![
        Span::styled(lead, Style::default().fg(theme::brand_primary())),
        Span::styled(printable(&head_of(stashed, room)), dim()),
        Span::styled(trail, dim()),
    ])]
}

/// Lines sent while something was running, each waiting for its place in the queue.
///
/// The line as it was typed, and under it the mark saying what has happened to it. Marked rather
/// than merely indented, because a person who pressed Enter has to be able to tell at a glance
/// that the words went somewhere: a line sitting quietly under the box is what this replaced, and
/// it read as a key press that had been ignored.
///
/// The prompt is the user's own text on its way back to them, so nothing here is a decision and
/// nothing is labelled. Control characters go through `printable` all the same, since a prompt can
/// be pasted and a paste can carry anything.
///
/// A command line waiting there carries the `!` the scrollback echoes one behind, because the two
/// go to different places and the row is the only thing that says which: `echo pwned` under the
/// box with nothing in front of it reads as words on their way to the model.
///
/// Beside each mark, where the line is going: into the turn in flight, into a turn of its own or
/// the one after, or carried out rather than sent. A person deciding whether to stop a turn over a
/// correction needs to know whether the correction is about to be read, and the mark alone says
/// only that it waits. Dropped whole where the width will not hold it, since half of it could say
/// the wrong thing.
///
/// The last one's mark says how to have them all go now, where the key that does it reaches this
/// program. Beside the mark rather than on a row of its own, so the rows under the box do not grow by
/// one the moment anything is queued, and dropped whole where the width will not hold it.
fn queued_lines(session: &Session, width: u16) -> Vec<Line<'static>> {
    const BADGE: &str = " QUEUED ";
    const SEND_NOW: &str = "  ctrl-enter to stop the turn and send now";

    let mut lines = Vec::new();
    for (at, waiting) in session.queued.iter().enumerate() {
        let room = (width as usize).saturating_sub(4);
        let goes = session.where_it_goes(at);
        let lead = match goes {
            Bound::Run => "! ",
            Bound::IntoThisTurn
            | Bound::ItsOwnTurn
            | Bound::IntoTheNextTurn
            | Bound::CarriedOut => "  ",
        };
        lines.push(Line::from(vec![
            Span::styled(lead, Style::default().fg(theme::brand_primary())),
            Span::styled(
                printable(&head_of(&waiting.prompt, room)),
                Style::default().fg(theme::brand_primary()),
            ),
        ]));
        let mut mark = vec![
            Span::raw("  "),
            Span::styled(
                BADGE,
                Style::default()
                    .fg(theme::on_primary())
                    .bg(theme::brand_primary())
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        // The offer before the words, where both will not fit, since it is the one way to hurry
        // what is waiting and the rows above still say where theirs are going.
        let used = 2 + wrap::display_width(BADGE);
        let offer = at + 1 == session.queued.len()
            && session.offers_to_send_now()
            && used + wrap::display_width(SEND_NOW) <= width as usize;
        let offered = if offer {
            wrap::display_width(SEND_NOW)
        } else {
            0
        };
        let left = (width as usize).saturating_sub(used + offered);
        let bound = format!(
            "  {}",
            match goes {
                Bound::IntoThisTurn => t!(queued_into_this_turn),
                Bound::ItsOwnTurn => t!(queued_its_own_turn),
                Bound::IntoTheNextTurn => t!(queued_into_the_next_turn),
                Bound::CarriedOut => t!(queued_carried_out),
                Bound::Run => t!(queued_run),
            }
        );
        if wrap::display_width(&bound) <= left {
            mark.push(Span::styled(bound, dim()));
        }
        if offer {
            mark.push(Span::styled(SEND_NOW, dim()));
        }
        lines.push(Line::from(mark));
    }
    lines
}

/// Everything drawn between the box and the hint line, one per row.
///
/// Returned rather than rendered, because the height the layout reserves has to be the height this
/// comes to: the shortcut list folds into as many columns as the width holds, so counting the
/// entries would not answer it.
///
/// What is offered is commands and skills or files, never a mixture, because the last word opens
/// with a slash or an `@`. Nothing labelled is involved either way: the commands are this program's
/// own words, the skills are the names and descriptions a turn would advertise and passed the same
/// trust gate, and the filenames are read out of the directory to show a person which files are in
/// it, never to decide anything and never reaching a model from here.
pub(super) fn lines_beneath_the_box(
    session: &Session,
    width: u16,
    offered: &crate::state::Offered,
) -> Vec<Line<'static>> {
    // Attachments first, nearest the box, because they belong to the line still in it: they are
    // what the next Enter will carry, and a file staged mid-turn appeared below the queue, which
    // reads as belonging to a prompt already gone.
    let mut lines = attached_lines(session, width);
    // Then the line put away, which is about the box and nothing else: it is what the next press of
    // one key puts back into it. Above the queue because those prompts have gone and this one has
    // not, and below the attachments because they belong to the line in the box now.
    lines.extend(stashed_lines(session, width));
    // Then waiting prompts, which describe something already done. Below the line they are no
    // part of, and still above what is offered, since a line the person believes they have sent
    // has to be visible without hunting for it.
    lines.extend(queued_lines(session, width));
    lines.extend(match offered {
        crate::state::Offered::Nothing => Vec::new(),
        crate::state::Offered::Slash { commands, skills } => {
            slash_lines(session, commands, skills, width)
        }
        crate::state::Offered::Files(entries) => entry_lines(session, entries),
        crate::state::Offered::Shortcuts => {
            shortcut_lines(session.editing(), session.bindings(), width)
        }
    });
    lines
}

/// One row per command and then one per skill, each with what it is for.
///
/// The description column is measured from every command and every skill held rather than from the
/// rows on screen, so it sits in the same place however far the list has narrowed. Measuring the
/// visible rows instead would slide the descriptions sideways with each letter typed.
fn slash_lines(
    session: &Session,
    commands: &[crate::command_table::Command],
    skills: &[crate::skills::Skill],
    width: u16,
) -> Vec<Line<'static>> {
    let column = crate::command_table::commands()
        .iter()
        .map(|command| command_word(command).chars().count())
        .chain(
            crate::skills::matching(session.held_skills(), "")
                .iter()
                .map(|skill| skill_word(skill).chars().count()),
        )
        .max()
        .unwrap_or(0);

    // While work runs a column between the word and its description says when Enter carries the
    // command out (CMD-8). Sized for the longest of the three so the descriptions stay in line.
    let working = session.status == Status::Working;
    let when = working.then(|| {
        [
            t!(command_when_now),
            t!(command_when_queued),
            t!(command_when_either),
        ]
        .iter()
        .map(|label| label.chars().count())
        .max()
        .unwrap_or(0)
    });

    let highlighted = session.highlighted_completion();
    let mut lines: Vec<Line<'static>> = commands
        .iter()
        .map(|command| {
            let chosen = Some(*command) == highlighted;
            let word = command_word(command);
            let padding = column.saturating_sub(word.chars().count()) + 2;
            let mut row = slash_row(chosen, word);
            row.push(Span::raw(" ".repeat(padding)));
            if let Some(room) = when {
                let label = command_when(session, command);
                row.push(Span::styled(label, dim()));
                row.push(Span::raw(
                    " ".repeat(room.saturating_sub(label.chars().count()) + 2),
                ));
            }
            row.push(Span::styled(command.description, dim()));
            Line::from(row)
        })
        .collect();

    let highlighted = session.highlighted_skill();
    lines.extend(skills.iter().map(|skill| {
        let chosen = highlighted.as_ref() == Some(skill);
        let word = skill_word(skill);
        let padding = column.saturating_sub(word.chars().count()) + 2;
        let from = match skill.source {
            bravebot_agent::skills::Source::Workspace => t!(skill_from_project),
            bravebot_agent::skills::Source::Home => t!(skill_from_user),
            bravebot_agent::skills::Source::BuiltIn => t!(skill_from_built_in),
        };
        // Cut rather than wrapped, since nothing below the box wraps, and cut short of the width so
        // where the skill came from is still on the row.
        let used = SLASH_MARGIN + word.chars().count() + padding + 1 + from.chars().count();
        let room = usize::from(width).saturating_sub(used);
        let description = skill
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let mut row = slash_row(chosen, word);
        row.push(Span::raw(" ".repeat(padding)));
        row.push(Span::styled(head_of(&printable(&description), room), dim()));
        row.push(Span::styled(format!(" {from}"), dim()));
        Line::from(row)
    }));
    lines
}

/// When Enter on a command carries it out, as the session stands (CMD-8).
///
/// Work that is not a turn, a compaction or an aside, holds every command back, whatever the table
/// says of one typed during a turn.
fn command_when(session: &Session, command: &crate::command_table::Command) -> &'static str {
    use crate::command_table::MidTurn;
    if !session.a_turn_is_running() {
        return t!(command_when_queued);
    }
    match command.mid_turn {
        MidTurn::Runs if crate::command_table::waits_behind_the_queue(session, command.name) => {
            t!(command_when_queued)
        }
        MidTurn::Runs => t!(command_when_now),
        MidTurn::Waits => t!(command_when_queued),
        MidTurn::Changes | MidTurn::RunsWhenNamed | MidTurn::RunsUnlessItStarts => {
            t!(command_when_either)
        }
    }
}

/// How wide the margin before a slash row's word is, the marker for the chosen row included.
const SLASH_MARGIN: usize = 4;

/// The margin and the word of one slash row, bold where it is the row the cursor is on.
fn slash_row(chosen: bool, word: String) -> Vec<Span<'static>> {
    let style = if chosen {
        Style::default()
            .fg(theme::brand_primary())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::brand_primary())
    };
    vec![
        Span::styled(if chosen { "  ❯ " } else { "    " }, style),
        Span::styled(word, style),
    ]
}

/// A skill as it is typed.
fn skill_word(skill: &crate::skills::Skill) -> String {
    format!("/{}", printable(&skill.name))
}

/// Every key and marker, and what it does, in the order they are listed.
///
/// The one place they are written down, so a binding that changes cannot leave the list advertising
/// something that no longer works. Only what a person reaches for: the readline chords that move
/// the caret are there for the terminals that send nothing else, not because anybody looks them up.
///
/// The meanings are kept short deliberately. The longest of them sets the column, so a word saved
/// here is what lets two columns fit a terminal eighty wide, and that halves the rows the list takes
/// out of the transcript.
///
/// Taken against the style of editing, because Escape does something different in each and this is
/// the one place a binding's meaning is written down: a list that went on saying "clear the line" to
/// somebody whose Escape takes the letters as commands would advertise a binding that is not there.
/// Every other row means the same thing either way.
///
/// The chords a settings file can move are asked of the bindings rather than written here, so the
/// list names the key that answers rather than the key that used to. The eleven the file can move
/// are the only rows that vary: nothing can take `?` or Enter, and a marker is not a chord at all.
pub(super) fn shortcuts(
    editing: crate::vim::Editing,
    bindings: &Keybindings,
) -> [(String, &'static str); 26] {
    let escape = match editing {
        crate::vim::Editing::Ordinary => "clear the line",
        crate::vim::Editing::Vi => "letters as commands, then stop",
    };
    [
        ("!".to_string(), "run a shell command"),
        ("/".to_string(), "commands and skills"),
        ("@".to_string(), "name a file"),
        ("?".to_string(), "this list"),
        ("enter".to_string(), "send"),
        ("shift-enter".to_string(), "new line, or ctrl-j"),
        ("ctrl-enter".to_string(), "stop, send what is queued"),
        ("tab".to_string(), "take what is offered"),
        ("shift-tab".to_string(), "what to ask before acting"),
        ("esc".to_string(), escape),
        ("up / down".to_string(), "earlier prompts"),
        ("pgup / pgdn".to_string(), "scroll the transcript"),
        ("ctrl-c".to_string(), "stop, clear, then exit"),
        ("ctrl-d".to_string(), "exit"),
        (bindings.editor_name(), "write prompt in $EDITOR"),
        (bindings.watch_name(), "watch a delegate work"),
        (bindings.scroller_name(), "open the scroller"),
        (bindings.history_name(), "search earlier prompts"),
        (bindings.stash_name(), "stash, bring it back, or search"),
        (bindings.trail_name(), "show what a turn did"),
        (bindings.paste_name(), "paste, pictures too"),
        (bindings.background_name(), "background a running command"),
        (bindings.panel_name(), "show or hide the info panel"),
        (
            bindings.scope_name(),
            "earlier prompts: this session or all",
        ),
        (bindings.undo_name(), "undo the last edit"),
        ("drag".to_string(), "select, copy on release"),
    ]
}

/// The shortcuts in as many columns as the width will hold.
///
/// Filled down each column rather than across each row, so the markers stay together at the top of
/// the first one: read across and `!`, `/` and `@` would be split up by whatever the width happened
/// to be. One column when nothing else fits, and the meanings are cut to the width there rather
/// than drawn past the edge, where the terminal would wrap them under the keys and put the list one
/// row over the height the layout reserved for it.
pub(super) fn shortcut_lines(
    editing: crate::vim::Editing,
    bindings: &Keybindings,
    width: u16,
) -> Vec<Line<'static>> {
    /// Blank columns between one column of the list and the next.
    const GUTTER: usize = 3;
    /// Where the list starts, matching the other rows drawn beneath the box.
    const INDENT: usize = 4;

    let room = (width as usize).saturating_sub(INDENT);
    // Nothing legible fits, so nothing is drawn. Every column below is measured against this, and a
    // width of nothing would leave them all at zero and every entry cut down to its ellipsis.
    if room == 0 {
        return Vec::new();
    }

    let listed = shortcuts(editing, bindings);
    let key_column = listed
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0)
        .min(room);
    let widest = listed
        .iter()
        .map(|(_, meaning)| meaning.chars().count())
        .max()
        .unwrap_or(0);

    let columns = ((room + GUTTER) / (key_column + 2 + widest + GUTTER)).max(1);
    let rows = listed.len().div_ceil(columns);
    // What a meaning has to itself once the keys, the padding and the gutters are taken out. As wide
    // as the longest wherever there is room for it, which is every width but the narrowest; zero
    // where the keys alone fill the row, and then they are listed without their meanings rather than
    // with the meanings running past the edge.
    let meaning_column = widest.min(
        (room.saturating_sub(GUTTER * (columns - 1)) / columns).saturating_sub(key_column + 2),
    );

    (0..rows)
        .map(|row| {
            let mut spans = vec![Span::raw(" ".repeat(INDENT))];
            for column in 0..columns {
                let Some((key, meaning)) = listed.get(row + column * rows) else {
                    break;
                };
                // Only between columns, so no row carries trailing blanks a selection would pick up.
                if column > 0 {
                    spans.push(Span::raw(" ".repeat(GUTTER)));
                }
                let key = head_of(key, key_column);
                let gap = key_column - key.chars().count();
                spans.push(Span::styled(
                    key,
                    Style::default().fg(theme::brand_primary()),
                ));
                if meaning_column == 0 {
                    continue;
                }
                spans.push(Span::raw(" ".repeat(gap + 2)));
                let shown = head_of(meaning, meaning_column);
                let padding = meaning_column - shown.chars().count();
                spans.push(Span::styled(shown, dim()));
                // Padded only where another column follows, for the same reason as the gutter.
                if row + (column + 1) * rows < listed.len() {
                    spans.push(Span::raw(" ".repeat(padding)));
                }
            }
            Line::from(spans)
        })
        .collect()
}

/// One row per workspace entry a reference could name.
///
/// A directory is dimmer than a file and keeps its trailing slash, because the two are chosen for
/// different reasons: a file is what a reference ends at, and a directory is somewhere to keep
/// typing. Nothing here says what a file contains, only that it exists.
pub(super) fn entry_lines(
    session: &Session,
    offered: &[bravebot_mentions::Entry],
) -> Vec<Line<'static>> {
    let highlighted = session.highlighted_entry();
    offered
        .iter()
        .map(|entry| {
            let chosen = Some(entry) == highlighted.as_ref();
            let colour = if entry.is_directory {
                Style::default().fg(theme::accent())
            } else {
                Style::default().fg(theme::brand_primary())
            };
            let style = if chosen {
                colour.add_modifier(Modifier::BOLD)
            } else {
                colour
            };

            let mut spans = vec![
                Span::styled(if chosen { "  ❯ " } else { "    " }, style),
                Span::styled(entry.path.clone(), style),
            ];
            // A session says what it is, since its name is an id nobody chooses by.
            if let Some(note) = &entry.note {
                spans.push(Span::styled(format!("  {note}"), dim()));
            }
            Line::from(spans)
        })
        .collect()
}
