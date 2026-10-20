//! The hint line under the box, and the readings it carries.

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::Session;
use crate::theme;

use super::dim;

/// How the hint line says where the rest of the bindings went.
pub(super) const SHORTCUTS_HINT: &str = "? for shortcuts";

/// What the context reading says where the session has none to give.
///
/// Two states reach it: nothing measured yet, and a count that arrived with no budget to divide it
/// by. Said rather than left blank, because a blank is also the other state with no reading, a
/// line with no room for the figure, and a reading that has stopped working, and nothing on the
/// line tells those apart.
pub(super) const UNMEASURED_CONTEXT: &str = "context not yet measured";

/// The word for a conversation shortened underneath its reading. On its own where the compaction
/// has no figure to give, and in front of the figure where it has one.
pub(super) const COMPACTED_CONTEXT: &str = "context compacted";

/// Which parts of the hint line fit, giving up the expendable ones in the order named.
///
/// Returns indices into `parts`, in their original order, so a caller draws what is left where it
/// already was. An empty part is skipped: the mode is absent most of the time, and a line that
/// reserved room for it would separate two things with nothing between them.
///
/// Dropping whole parts rather than letting the terminal cut the last one, which leaves "? fo" on a
/// terminal eighty wide. Half a word reads as a rendering bug; a part that is simply not there reads
/// as a line that had no room, which is the truth.
///
/// Widths are counted in characters. Not the width a terminal draws for every script, but much closer
/// than a count of bytes and it needs nothing to work it out.
fn fitted(parts: &[String], expendable: &[usize], width: u16) -> Vec<usize> {
    /// What the parts are joined with, and the indent before the first.
    const SEPARATOR: usize = 5;
    const INDENT: usize = 2;

    let mut kept: Vec<usize> = (0..parts.len())
        .filter(|index| !parts[*index].is_empty())
        .collect();
    let measure = |kept: &[usize]| -> usize {
        let text: usize = kept.iter().map(|index| parts[*index].chars().count()).sum();
        INDENT + text + SEPARATOR * kept.len().saturating_sub(1)
    };

    for giving_up in expendable {
        if measure(&kept) <= width as usize {
            break;
        }
        kept.retain(|index| index != giving_up);
    }

    // A terminal too narrow for even what is left keeps nothing. Everything expendable is already
    // gone by here, so what remains would be cut, and half a word is the thing this exists to avoid:
    // an empty line reads as a terminal with no room, which it is.
    if measure(&kept) > width as usize {
        kept.clear();
    }
    kept
}

/// What the row under the box says about a live loop, from how long is left until the next tick.
///
/// A moment where one is known and still ahead, and the bare word otherwise. No moment at all is a
/// tick in flight or a self-paced loop that has not been told when to wake. A moment already gone is
/// a tick the session is not free to take yet, because a turn is running or the queue is holding
/// something, and a row counting down to `0s` and sitting there reads as a loop that has stalled
/// rather than one waiting its turn.
pub(super) fn loop_part(until: Option<std::time::Duration>) -> String {
    match until {
        Some(until) if until >= std::time::Duration::from_secs(1) => {
            t!(loop_hint_next, next = crate::loops::spell(until))
        }
        _ => t!(loop_hint).to_string(),
    }
}

pub(crate) fn cache_hit_rate(session: &Session) -> Option<String> {
    let cached = session.cached().filter(|cached| cached.read_tokens > 0)?;
    let prompt_tokens = session
        .cached_prompt_tokens()
        .filter(|tokens| *tokens > 0)?;
    let tenths = (u128::from(cached.read_tokens) * 1_000 + u128::from(prompt_tokens) / 2)
        / u128::from(prompt_tokens);
    let rate = format!(
        "{}{}{}",
        tenths / 10,
        t!(number_decimal_separator),
        tenths % 10
    );
    Some(t!(hint_cache_hit_rate, rate = rate).to_string())
}

/// How the context currently stands: unmeasured, compacted, or measured as a percentage.
///
/// Each of them says which it is, and none of them is drawn as nothing: a reading that comes and
/// goes is one people stop reading, and this is the only account of the size of a conversation
/// there is.
pub(crate) fn context_reading(session: &Session) -> String {
    match session.occupancy() {
        crate::state::Occupancy::Unmeasured => UNMEASURED_CONTEXT.to_string(),
        // How much of the budget the compaction won back, which is what somebody who has just
        // shortened a conversation is asking. Approximate, and marked so: the summariser counted
        // its own instructions along with the exchange it read, and there is no tokeniser here to
        // take them off again, so the figure is a little larger than the room really is.
        crate::state::Occupancy::Compacted { .. } => match session.won_back() {
            Some(percent) => format!("{COMPACTED_CONTEXT}, ~{percent}% won back"),
            None => COMPACTED_CONTEXT.to_string(),
        },
        crate::state::Occupancy::Measured { guessed, .. } => match session.fullness() {
            Some(percent) if guessed => format!("context ~{percent}%"),
            Some(percent) => format!("context {percent}%"),
            // A count with no budget to divide it by is no reading of how full the context is, so
            // the session knows no more here than one that has measured nothing and says the same.
            None => UNMEASURED_CONTEXT.to_string(),
        },
    }
}

/// The six parts both lines draw, read before either branch is taken.
struct HintFacts {
    looping: String,
    trail: String,
    jobs: String,
    held: String,
    movable: String,
    watchable: String,
}

/// Where each part of the shell line sits in the array `shell_parts` builds.
mod shell {
    pub const NAME: usize = 0;
    pub const CANCEL: usize = 1;
    pub const OUTPUT: usize = 2;
    pub const MOVABLE: usize = 3;
    pub const LOOP: usize = 4;
    pub const JOBS: usize = 5;
    pub const WATCHABLE: usize = 6;
    pub const HELD: usize = 7;
}

/// Where each part of the ordinary line sits in the array `ordinary_parts` builds.
mod ordinary {
    // 0 is the permission mode and 1 the vi mode, which `MODES` covers.
    pub const TRAIL: usize = 2;
    pub const CONTEXT: usize = 3;
    pub const CACHE: usize = 4;
    pub const LOOP: usize = 5;
    pub const JOBS: usize = 6;
    pub const WATCHABLE: usize = 7;
    pub const MOVABLE: usize = 8;
    pub const SHORTCUTS: usize = 9;
    pub const INFO: usize = 10;
    pub const HELD: usize = 11;
    /// How many parts lead the line in the accent colour: the permission mode and the vi mode.
    pub const MODES: usize = 2;
}

/// The parts of the hint line that do not depend on which mode the box is in.
fn hint_facts(session: &Session) -> HintFacts {
    // Read before the shell line as well as the ordinary one. A loop spends a turn whichever mode
    // the box is in, and shell mode is where somebody is least likely to be thinking about one.
    let looping = session.looping().map_or_else(String::new, |running| {
        loop_part(running.until(std::time::Instant::now()))
    });

    // Named only once a turn has left a trail to look at. Offering the key before that is a line
    // under the box inviting a press that changes nothing on screen, and what a person learns from
    // that press is that the key does not work.
    let trail = match (session.has_trail(), session.show_trail) {
        (false, _) => String::new(),
        (true, true) => format!("{} hide trail", session.bindings().trail_name()),
        (true, false) => format!("{} show trail", session.bindings().trail_name()),
    };

    // Beside the loop and for the same reason: a job runs while nobody watches it, and once the
    // block that started it has scrolled away nothing else on the screen says it runs (RUN-26).
    // Counted from the driver's events and never from what a job printed.
    let jobs = match session.jobs_running() {
        0 => String::new(),
        count => t!(jobs_hint, count = count),
    };

    // The view is held where it was scrolled to, so what arrives goes below it and nothing else on
    // the screen says so. A count of rows from the layout and the key that returns, never any of
    // what those rows hold (VIEW-27).
    let held = match session.rows_below() {
        0 => String::new(),
        count => t!(held_hint, chord = "ctrl-end", count = count),
    };

    // Only while the command the turn is waiting on can be moved, for the reason the trail is only
    // named once there is one: offered at any other moment, the press does nothing.
    let movable = if session.can_move_to_background() {
        t!(
            background_hint,
            chord = session.bindings().background_name()
        )
        .to_string()
    } else {
        String::new()
    };

    // The way into the view, for as long as it holds anything. The row that reports what the turn
    // is doing names the key too, but that row goes when the turn ends, and what the view holds is
    // most worth opening afterwards: the transcript keeps one sentence about a delegate and a
    // preview of what a command printed. The count is there because a key with nothing behind it
    // does nothing at all, and this line is read at a glance.
    //
    // Everything the view opens, not the delegates alone. A session that ran commands and spawned
    // no delegate has a key that works and, counted the other way, no line saying so.
    //
    // Read before the shell line as well as the ordinary one (WATCH-11): the view holds the same
    // things whichever mode the box is in, and this is the line that is always drawn.
    let watchable = match session.watchable().len() {
        0 => String::new(),
        count => t!(
            watching_hint,
            chord = session.bindings().watch_name(),
            count = count
        ),
    };
    HintFacts {
        looping,
        trail,
        jobs,
        held,
        movable,
        watchable,
    }
}

/// The parts of the shell line, indexed by `shell`.
fn shell_parts(session: &Session, facts: HintFacts) -> [String; 8] {
    // Fitted like the ordinary line, so a narrow terminal drops a part whole rather than
    // cutting one at the last column. The shell is the mode and goes last; what it says about
    // the line goes before the things that are spending something unwatched. The way into the
    // view is a thing learned once, so it is given up right after the sentences about the line.
    [
        format!(
            "{} {}",
            if session.shell_private() { "!!" } else { "!" },
            bravebot_agent::shell::shell()
        ),
        "esc to cancel".to_string(),
        match session.shell_private() {
            true => "output stays here, not sent to the model",
            false => "output goes to the model",
        }
        .to_string(),
        facts.movable,
        facts.looping,
        facts.jobs,
        facts.watchable,
        facts.held,
    ]
}

/// The parts of the ordinary line, indexed by `ordinary`.
fn ordinary_parts(
    session: &Session,
    area: Rect,
    screen_height: u16,
    facts: HintFacts,
) -> [String; 12] {
    // The panel shows the context and the cache while it has the rows for them, so the line drops
    // both rather than saying them twice (PANEL-8). Offered while it is closed and there is room to
    // draw it.
    let panel_shows_context = crate::panel::shows_context(session, area.width, screen_height);
    let context = if panel_shows_context {
        String::new()
    } else {
        context_reading(session)
    };
    let info = if !session.panel_open() && crate::panel::fits(area.width) {
        t!(panel_hint, chord = session.bindings().panel_name()).to_string()
    } else {
        String::new()
    };

    let cache = if panel_shows_context {
        String::new()
    } else {
        cache_hit_rate(session).unwrap_or_default()
    };

    // Not a list of bindings any more. Every one of them, with what it does, is a `?` away, which
    // is both more than this line could hold and the moment a person wants to know; what stays here
    // is what the session is doing, which is the part they cannot ask for.
    //
    // The state comes first because the line is cut by a terminal narrower than it, and the end is
    // what goes. A binding cut off is one somebody learns once; a figure cut off is the only thing
    // here they have no other way to see.
    //
    // The mode leads all of it, and is the one thing on this line drawn in a colour. It is what
    // decides whether the next write stops to ask, so of everything here it is the fact somebody
    // most needs to catch without looking for it.
    //
    // Assembled and then trimmed to the width rather than left to the terminal, which cuts wherever
    // the last column falls and leaves "? fo". Dropping a whole part at a separator is legible; half
    // a word reads as a rendering bug. See `fitted` for the order they are given up in.
    //
    // An empty part is skipped rather than drawn, so a session with no trail, nothing measured and
    // nothing to open does not open its line on a separator with nothing in front of it.
    let mode = crate::status::named_mode(session.permission_mode(), session.began_in_bypass());
    // Beside the permission mode, on the same footing and for the same reason: which vi mode the box
    // is in decides whether the next letter is a letter or an instruction, so of everything here the
    // two of them are what somebody has to catch without going looking. Nothing at all for the box
    // everybody else has, which is in no mode to report.
    //
    // With the keys of an instruction still waiting for more after it, as vi's `showcmd` draws them.
    // A `d` waiting decides what the next letter does as much as the mode does, and drawn nowhere it
    // is the next `w` that says so, by deleting a word.
    let editing = session
        .vi_mode()
        .map(|mode| match session.half_typed() {
            Some(keys) => format!("{} {keys}", mode.as_str()),
            None => mode.as_str().to_string(),
        })
        .unwrap_or_default();
    // What is going to happen here without anybody touching the keyboard. Between ticks the screen
    // is a transcript of things that have already happened, so a loop that is quietly spending a
    // turn every five minutes is the one fact about the session nothing on it reports: the note
    // that said it started has scrolled away, and the next tick is the first sign of life since.
    [
        mode.unwrap_or_default().to_string(),
        editing,
        facts.trail,
        context,
        cache,
        facts.looping,
        facts.jobs,
        facts.watchable,
        facts.movable,
        SHORTCUTS_HINT.to_string(),
        info,
        facts.held,
    ]
}

/// The spans for the parts `fitted` kept: those below `accented_count` joined into one accented
/// span, the rest into one dim span, so what is marked is exactly what changes the meaning of a
/// keystroke. Drawn from `kept` like everything else, so a terminal with no room for a part drops
/// it whole rather than showing the front half of it.
pub(super) fn hint_spans(
    parts: &[String],
    kept: &[usize],
    accented_count: usize,
) -> Vec<Span<'static>> {
    let joined = |indices: &[usize]| -> Vec<&str> {
        indices.iter().map(|index| parts[*index].as_str()).collect()
    };
    let (accented, rest): (Vec<usize>, Vec<usize>) =
        kept.iter().partition(|index| **index < accented_count);

    let mut spans = Vec::new();
    if !accented.is_empty() {
        spans.push(Span::styled(
            format!("  {}", joined(&accented).join("  ·  ")),
            Style::default().fg(theme::accent()),
        ));
    }
    if !rest.is_empty() {
        let separator = if accented.is_empty() { "  " } else { "  ·  " };
        spans.push(Span::styled(
            format!("{separator}{}", joined(&rest).join("  ·  ")),
            dim(),
        ));
    }
    spans
}

/// The shortcut line. Keeps the bindings discoverable without a help command.
pub(super) fn draw_hint(frame: &mut Frame, area: Rect, session: &Session) {
    let facts = hint_facts(session);

    // In shell mode the usual bindings are beside the point: the line goes to a shell, so what a
    // user needs to know is which shell and how to get back out again.
    if session.shell {
        let parts = shell_parts(session, facts);
        let kept = fitted(
            &parts,
            &[
                shell::OUTPUT,
                shell::CANCEL,
                shell::WATCHABLE,
                shell::MOVABLE,
                shell::HELD,
                shell::JOBS,
                shell::LOOP,
            ],
            area.width,
        );
        let spans = hint_spans(&parts, &kept, shell::NAME + 1);
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }

    let parts = ordinary_parts(session, area, frame.area().height, facts);
    let context_is_unmeasured = parts[ordinary::CONTEXT] == UNMEASURED_CONTEXT;
    // Indices into `parts`, in the order they are given up: the way to the panel before anything,
    // since the panel is a press away whether or not the line names it (PANEL-7), then the way to
    // the bindings, then the trail toggle, both being things somebody learns once. Then the
    // figures. Neither mode is ever listed, because of everything here they are what changes what
    // the next keystroke does.
    //
    // A reading with no figure in it goes before any of them. The readings are kept late because a
    // figure is the one thing on this line nothing else can tell somebody, and a sentence saying
    // there is no figure yet is not one: it would be holding the room against two working bindings.
    //
    // The loop is last of all, and is given up only by a terminal with no room for anything else,
    // because it is the only part of this line that is spending something while nobody watches. It
    // is still in the list: a part nothing may give up makes `fitted` clear the whole line on a
    // narrow terminal, which would take the mode with it, and a mode nobody can read is worse than
    // a loop they can still find with `/loop` or `/status`.
    //
    // The way to move a command goes just before the jobs, and the jobs just before the loop. The
    // offer is up only while somebody is waiting on that command, which is the moment they are
    // reading this line for a way out of the wait; a job is spending something unwatched, as the
    // loop is.
    use ordinary::{CACHE, CONTEXT, HELD, INFO, JOBS, LOOP, MOVABLE, SHORTCUTS, TRAIL, WATCHABLE};
    let expendable: &[usize] = if context_is_unmeasured {
        &[
            INFO, CONTEXT, SHORTCUTS, TRAIL, CACHE, WATCHABLE, MOVABLE, HELD, JOBS, LOOP,
        ]
    } else {
        &[
            INFO, SHORTCUTS, TRAIL, CONTEXT, CACHE, WATCHABLE, MOVABLE, HELD, JOBS, LOOP,
        ]
    };
    // A note is drawn over the right of this same row, so what the parts may occupy is the width
    // less that note. Fitted against the whole width instead, the last part that fits is one the
    // note then writes over the middle of, which is the half a word that dropping a part whole
    // exists to avoid.
    let note = note_at_the_right(session);
    let reserved = note.as_deref().map_or(0, |note| {
        u16::try_from(note.chars().count()).unwrap_or(u16::MAX)
    });
    let kept = fitted(&parts, expendable, area.width.saturating_sub(reserved));

    let spans = hint_spans(&parts, &kept, ordinary::MODES);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);

    if let Some(note) = note {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                note,
                Style::default().fg(theme::brand_primary()),
            )))
            .alignment(Alignment::Right),
            area,
        );
    }
}

/// What a press just did, drawn at the right of the hint row, where there is anything to say.
///
/// One note at a time, in this order, since they share the room: the parts of the line are fitted
/// against the width this leaves.
fn note_at_the_right(session: &Session) -> Option<String> {
    // The line the person was writing has just gone, so the press that took it is the one thing
    // worth explaining: without this, a key they pressed to stop something emptied the box and
    // said nothing, and the next press of it ends the session.
    if session.cleared_by_interrupt {
        return Some("ctrl-c again to exit  ".to_string());
    }

    // The same sentence for the same reason, on the rung where there was no line to take. Without
    // it the press that offered the way out would have done nothing and said nothing, which reads
    // as an interface that has stopped responding.
    if session.offered_to_leave {
        return Some("ctrl-c again to exit  ".to_string());
    }

    // A copy is silent otherwise, and a clipboard that may or may not have taken something is
    // worse than no clipboard: the user pastes to find out. Right-aligned, out of the way of the
    // hints, where the answer to "did that work" belongs.
    if let Some(characters) = session.copied {
        return Some(format!(
            "{} to clipboard  ",
            tally(characters, "char", "chars")
        ));
    }

    // Command-V cannot carry a picture and cannot say so: the chord never reaches this process, and
    // what the terminal does with it is write the clipboard's text into the pty, of which a picture
    // has none. So the only way anyone finds out which key does work is being told before they try
    // the one that does not.
    if session.image_on_clipboard {
        return Some(format!(
            "image on clipboard  ·  {} to paste  ",
            session.bindings().paste_name()
        ));
    }

    None
}

/// A count with the right noun, so a line does not read "1 chars".
fn tally(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}
