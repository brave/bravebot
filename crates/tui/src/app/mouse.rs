//! What a mouse event does: the wheel, a drag and a release, and the click each kind of target
//! stands for.

use super::dispatch_command;
use super::keys::{Action, carry_out_choice};
use crate::command_table::{
    CONTEXT_COMMAND, DIFF_COMMAND, EFFORT_COMMAND, LSP_COMMAND, MCP_COMMAND, MODEL_COMMAND,
    is_carried_out_at_once, why_it_waits,
};
use crate::render;
use crate::state::{Hit, Session, Status, Target};
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// What a report of the mouse ends before it is read, whatever it then asks for.
fn end_what_a_report_ends(session: &mut Session, mouse: MouseEvent) {
    // A drag goes on through the wheel and its own reports. A press, a release, a move with no button
    // and the other buttons each end it: a pointer held outside the transcript scrolls it only for
    // as long as the left button is down and has moved (MOUSE-37).
    if !matches!(
        mouse.kind,
        MouseEventKind::Drag(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight
    ) {
        session.stop_sweeping();
    }
    // A press is either the second of a double click or somewhere else, and in both a click on a
    // path that was waiting is not the one the person meant (MOUSE-42). It is not the end of a drag
    // that was held either, which a press on a thumb or a border begins again below.
    if matches!(mouse.kind, MouseEventKind::Down(_)) {
        session.drop_the_click_put_off();
        session.end_the_drags();
    }
    // A button going down, or held while the pointer moves, is no longer a pointer crossing the
    // screen. The wheel is not: it moves what is under the pointer, which is read again (MOUSE-51).
    if matches!(
        mouse.kind,
        MouseEventKind::Down(_) | MouseEventKind::Drag(_)
    ) {
        session.end_hover();
    }
}

/// Interpret a mouse event.
///
/// The wheel is what most people reach for first, so it scrolls without any modifier.
///
/// Dragging selects. Capturing the mouse for the wheel is what took the terminal's own selection
/// away, so the drag that would have highlighted a line arrives here instead, and answering it
/// is the only way a user gets to copy anything.
pub fn handle_mouse(session: &mut Session, mouse: MouseEvent) -> Action {
    session.note_pointer(mouse.row, mouse.column);
    session.settle_the_view();
    end_what_a_report_ends(session, mouse);
    // Before any arm, so that nothing drawn beneath a panel over the frame answers a press that
    // lands on it (MOUSE-45). The two above only forget what a press or a release ends, and a press
    // on a panel ends a drag and a waiting click as a press anywhere does. A target added to the
    // arms below is covered without being named here.
    if session.popup_claims(mouse) {
        return match mouse.kind {
            // A press on an item of the menu is the item's: what it asks for is carried out as the
            // key that takes it carries it out (MOUSE-52).
            MouseEventKind::Down(_) => match session.take_the_menu_choice() {
                Some(choice) => carry_out_choice(session, choice),
                None => Action::Redraw,
            },
            // A turn of the wheel walks the cursor of the panel, and the frame that shows it is
            // asked for as for any other report that changed what is drawn (MOUSE-46).
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => Action::Redraw,
            _ => Action::None,
        };
    }
    match mouse.kind {
        MouseEventKind::ScrollUp => wheel(session, mouse, false),
        MouseEventKind::ScrollDown => wheel(session, mouse, true),
        // The border between the transcript and the panel is the panel's handle, and a press on
        // it starts neither a selection nor a copy (PANEL-6).
        MouseEventKind::Down(MouseButton::Left)
            if session.begin_panel_drag(mouse.row, mouse.column) =>
        {
            Action::Redraw
        }
        MouseEventKind::Drag(MouseButton::Left) if session.dragging_panel() => {
            session.drag_panel_to(mouse.column);
            Action::Redraw
        }
        MouseEventKind::Up(MouseButton::Left) if session.dragging_panel() => {
            session.end_panel_drag();
            Action::Redraw
        }
        // The scrollbar beside the transcript: a press puts the view there or takes hold of the
        // thumb, a drag carries it, and none of them starts a selection or a copy (MOUSE-49).
        MouseEventKind::Down(MouseButton::Left)
            if session.begin_bar_drag(mouse.row, mouse.column) =>
        {
            Action::Redraw
        }
        MouseEventKind::Drag(MouseButton::Left) if session.dragging_bar() => {
            session.drag_bar_to(mouse.row);
            Action::Redraw
        }
        MouseEventKind::Up(MouseButton::Left) if session.dragging_bar() => {
            session.end_bar_drag();
            Action::Redraw
        }
        // Shift-click moves the loose end of the selection held, when the press is in the pane
        // it belongs to. Where it is not, or nothing is held, it is a press like any other.
        MouseEventKind::Down(MouseButton::Left)
            if mouse.modifiers.contains(KeyModifiers::SHIFT)
                && session.selection_holds(mouse.row, mouse.column) =>
        {
            session.last_click = None;
            session.extend_selection(mouse.row, mouse.column);
            Action::Redraw
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let (click, count) = crate::select::count_click(
                session.last_click,
                std::time::Instant::now(),
                mouse.row,
                mouse.column,
            );
            session.last_click = Some(click);
            session.note_the_press(mouse.row, mouse.column);
            // The second press of a double click whose first click acted on a target begins nothing:
            // a terminal reports the first release as a click, and what the second press would
            // select is the words of whatever the click left under the pointer (MOUSE-50).
            if count > 1 && session.click_acted {
                return Action::None;
            }
            session.click_acted = false;
            session.begin_selection_by(
                mouse.row,
                mouse.column,
                crate::select::Unit::of_click(count),
            );
            Action::Redraw
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            session.sweep_to(mouse.row, mouse.column);
            Action::Redraw
        }
        MouseEventKind::Up(MouseButton::Left) => release(session, mouse),
        // A right press on a reply, a delegate's block or a selection held opens the small menu
        // there (MOUSE-52). Anywhere else it is no press anything acts on.
        MouseEventKind::Down(MouseButton::Right) => {
            match session.open_the_menu(mouse.row, mouse.column) {
                true => Action::Redraw,
                false => Action::None,
            }
        }
        // The pointer moved with no button held. A frame is wanted only when the target under it is
        // not the one that was (MOUSE-51).
        MouseEventKind::Moved => match session.hover_at(mouse.row, mouse.column) {
            true => Action::Redraw,
            false => Action::None,
        },
        _ => Action::None,
    }
}

/// What a turn of the wheel does, up or down (MOUSE-46).
///
/// While the scroller is open it is one of the scroller's keys, so it stops at the first row and the
/// last the way every other movement in that mode does. It takes no count, so it drops one, as any
/// other key would. In the Sessions screen it scrolls the details beside the list while the pointer
/// is over them and walks the cursor elsewhere (SCREEN-12). Over the rows of a list of what could be
/// typed it walks the list and not the transcript beneath it. Over the panel in the files state it
/// scrolls the pane under the pointer (MOUSE-19). Elsewhere it scrolls the transcript. How far it goes
/// is what `tui.wheelRows` named, which is the same distance in each.
fn wheel(session: &mut Session, mouse: MouseEvent, down: bool) -> Action {
    let rows = session.wheel_rows();
    // Over the details of the Sessions screen the wheel scrolls them, and elsewhere on it the wheel
    // walks the cursor (SCREEN-12).
    let over_the_details = session.sessions_screen().is_some()
        && session
            .laid
            .find_of_kind(session, mouse.row, mouse.column, |hit| {
                hit == Hit::SessionsScreen(crate::sessions_screen::Click::Details)
            })
            .is_some();
    if let Some(sessions_screen) = session.sessions_screen_mut() {
        match over_the_details {
            true => {
                sessions_screen.scroll_details(usize::from(rows), down);
            }
            false => sessions_screen.wheel(usize::from(rows), down),
        }
    } else if session.scrolling() {
        session.take_the_scroller_count();
        match down {
            true => session.scroller_on(rows),
            false => session.scroller_back(rows),
        }
    } else if session.files_wheel(mouse.row, mouse.column, !down) {
        // The pane of the files state under the pointer was scrolled.
    } else if session.peek_wheel(mouse.row, mouse.column, !down) {
        // The text of the paste under the pointer was scrolled.
    } else if session.completion_at(mouse.row, mouse.column).is_some() {
        session.walk_completions(rows, down);
    } else {
        match down {
            true => session.scroll_down(rows),
            false => session.scroll_up(rows),
        }
    }
    Action::Redraw
}

/// What the release of the left button does: it ends the click the press began on the target under it,
/// or it ends a selection.
///
/// The target is the one the last frame drew there that stands in the state the session is in now
/// ([`Session::target_at`]), and what a click on each kind is is what its clause says. A release that
/// ends no click, because the pointer left the cell it pressed or swept something, or that finds no
/// target, or one that does nothing where it is, ends the selection and asks for the copy of it.
///
/// The rest of a double or triple click whose first click acted does nothing at all, and what each
/// click that acts leaves is remembered for it ([`Session::click_acted`], MOUSE-50).
fn release(session: &mut Session, mouse: MouseEvent) -> Action {
    let Some(presses) = click_ended_by(session, mouse) else {
        return Action::Copy;
    };
    if presses > 1 && session.click_acted {
        return Action::None;
    }
    let Some(target) = session.target_at(mouse.row, mouse.column).copied() else {
        return Action::Copy;
    };
    // What a frame drawn since the press put under the cell is not what was pressed on.
    if !session.press_stood_on(target.hit) {
        return Action::Copy;
    }
    let action = click_on(session, target, presses, mouse);
    session.click_acted = presses == 1 && click_acted_on(session, target.hit, &action);
    action
}

/// Whether a click that asked for `action` of the target `hit` acted, so that the presses that follow
/// it on the cell begin nothing (MOUSE-50). Three kinds do not. The text of the box has a caret to
/// put and a word to select when it is double clicked, as any text has. A row of the list under the
/// box takes a second click as its own (MOUSE-15). A path waits for the second press to say whether
/// the first was a click on it or the start of a double click (MOUSE-42).
fn click_acted_on(session: &Session, hit: Hit, action: &Action) -> bool {
    *action != Action::Copy
        && !matches!(
            hit,
            Hit::Text { .. } | Hit::Item(_) | Hit::SessionsScreen(_)
        )
        && session.pending_click.is_none()
}

/// What a release that ends a click on `target` does, the click being the `presses`th press of its
/// run.
fn click_on(session: &mut Session, target: Target, presses: u8, mouse: MouseEvent) -> Action {
    let (row, column) = (mouse.row, mouse.column);
    // The first click of a double click puts the cursor on a row of the list under the box and the
    // second takes it, so the list answers two presses and every other kind one (MOUSE-15). A double
    // or triple click anywhere else is the selection it made.
    if let Hit::Item(index) = target.hit {
        return match presses {
            1 | 2 => click_an_item(session, index, presses == 2),
            _ => Action::Copy,
        };
    }
    // The details of the Sessions screen are drawn for the wheel and take no click, which is the
    // selection it made, as on any cell that is no row (SCREEN-12).
    if target.hit == Hit::SessionsScreen(crate::sessions_screen::Click::Details) {
        return Action::Copy;
    }
    // The same for a row of the Sessions screen: the first click puts the cursor on it and the
    // second opens it, as Enter does (SCREEN-12).
    if let Hit::SessionsScreen(click) = target.hit {
        return match presses {
            1 | 2 => click_in_the_sessions_screen(session, click, presses == 2),
            _ => Action::None,
        };
    }
    if presses != 1 {
        return Action::Copy;
    }
    let acted = match target.hit {
        // The way back out of a delegate's view leaves it as the key that opened it does (MOUSE-24).
        Hit::Back => session.stop_watching(),
        // In the scroller a call opens or closes as `x` does (MOUSE-9), and at rest a call with more
        // to show opens the scroller on it expanded (MOUSE-25).
        Hit::Call(index) if session.scrolling() => {
            session.flip_call(index, target.area.y, render::as_last_drawn);
            true
        }
        Hit::Call(index) => {
            session.open_the_scroller_on_the_call(index, target.area.y, render::as_drawn_at)
        }
        // A delegate's block opens that delegate's view as the watch key does (MOUSE-10, MOUSE-23).
        Hit::Delegate(index) => session.watch_delegate(index),
        // The prompt drawn above the view goes to the top of it, as `{` puts one there (MOUSE-54).
        Hit::Pinned(row) => {
            session.go_to_the_pinned_prompt(row);
            true
        }
        // The button above the box returns the view to the tail as `ctrl-end` does (MOUSE-55).
        Hit::ToTail => {
            session.return_to_the_tail();
            true
        }
        // The face on the opening screen blinks or looks to the other side (MOUSE-56). It is drawn
        // again at once, since the loop only looks at the face between keys.
        Hit::Face => {
            crate::avatar::poke();
            true
        }
        // A prompt waiting behind the turn comes back into the box, with the ones typed after it, as
        // Up takes them all. A command a click queued has nothing to put there, so the click removes
        // it, and so do the words on the row (MOUSE-28).
        Hit::Queued(place) if session.queued_comes_back(place) => session.unqueue_from(place),
        Hit::Queued(place) | Hit::Discard(crate::state::Discard::One(place)) => {
            session.discard_queued(place)
        }
        Hit::Discard(crate::state::Discard::All) => session.discard_queued_all(),
        // The marker that leads a finished reply copies that reply, as `/copy` does (MOUSE-26), and
        // the word at the right of a fenced block's header copies the block alone (MOUSE-27). What
        // is copied is the entry's own words, so a reply the screen cut across rows copies as the
        // one it was.
        Hit::Reply(_) | Hit::Block(..) => match session.words_to_copy(target.hit) {
            Some(words) => {
                session.clear_selection();
                return Action::CopyReply(super::copyable(&words));
            }
            None => false,
        },
        Hit::Link(index) => return click_a_link(session, index, mouse.modifiers),
        Hit::Panel(_) | Hit::Footer(_) => return click_shortcut(session, target.hit),
        // A click in the prompt box puts the caret where it landed (MOUSE-16).
        Hit::Text { .. } => {
            let on_a_marker = session.click_on_a_marker(row, column);
            let put = session.click_in_the_box(row, column);
            // A press on the cells of a marker shows the paste whole, over the caret it just put at
            // the marker's start.
            if on_a_marker {
                session.peek_pasted_text();
            }
            put
        }
        // A press on the scrollbar acts on the press, and a panel that holds the input has taken the
        // press before this is asked (MOUSE-45, MOUSE-49).
        // A pane the wheel scrolls is no click: what a click acts on is drawn over it.
        Hit::Scrollbar
        | Hit::Popup(_)
        | Hit::PopupRows(_)
        | Hit::Match(_)
        | Hit::Item(_)
        | Hit::Words(_)
        | Hit::Wheel(_)
        | Hit::SessionsScreen(_) => false,
    };
    if !acted {
        return Action::Copy;
    }
    session.clear_selection();
    Action::Redraw
}

/// What a release does that ends a click on a part of the Sessions screen: a tab is chosen, and
/// a row or "Show more" takes the cursor, and is opened by the second click of a double click
/// (SCREEN-12).
fn click_in_the_sessions_screen(
    session: &mut Session,
    click: crate::sessions_screen::Click,
    second: bool,
) -> Action {
    use crate::sessions_screen::{Click, Outcome};
    session.clear_selection();
    let Some(sessions_screen) = session.sessions_screen_mut() else {
        return Action::None;
    };
    // A second press is one only where it is on what the first pointed at, and not on whatever a
    // turn of the wheel or a read of the list has since put at the cell (MOUSE-15).
    let second = sessions_screen.press(click) && second;
    let outcome = match (click, second) {
        (Click::Tab(_), _) | (_, false) => return Action::Redraw,
        (_, true) => sessions_screen.open(),
    };
    match outcome {
        // What changes nothing the session is running is done here, as the key path does it, so
        // that "Show more" loads more whichever loop has the mouse. What is left is for the loop that
        // holds the record and the terminal.
        Outcome::Effect(effect) => match super::sessions_screen::carry_out(session, effect) {
            Some(left) => Action::SessionsScreen(left),
            None => Action::Redraw,
        },
        _ => Action::Redraw,
    }
}

/// What a release does that ends the first or the second click of a double click on a row of a list
/// of what could be typed: the first puts the cursor on the row and the second takes it as Tab does.
fn click_an_item(session: &mut Session, index: usize, second: bool) -> Action {
    session.clear_selection();
    session.select_completion(index);
    if second {
        session.accept_completion();
    }
    Action::Redraw
}

/// What a click with the keys `held` on the link at `index` asks for. With ctrl held an address asks
/// for the browser (MOUSE-40) and a path for the editor, at the place the path gave (MOUSE-43). With
/// nothing held a path waits to see whether a second press follows, and then shows the file
/// (MOUSE-42), which the scroller is not the place for since the file is added at the end of the
/// transcript. Any other click on a link is the selection start it always was.
fn click_a_link(session: &mut Session, index: usize, held: KeyModifiers) -> Action {
    use crate::text_links::Target;
    let ctrl = held.contains(KeyModifiers::CONTROL);
    let asks = match session.laid.links.get(index).cloned() {
        Some(Target::Address(address)) if ctrl => Action::OpenUrl(address),
        Some(Target::Path(path)) if ctrl => Action::EditFile(path),
        Some(Target::Path(path))
            if held == KeyModifiers::NONE && session.answers_a_plain_click_on_a_path() =>
        {
            session.put_off_a_click(path, std::time::Instant::now());
            Action::Redraw
        }
        _ => return Action::Copy,
    };
    session.clear_selection();
    asks
}

/// How many presses the click is that a release at `mouse` ends, where it ends one: the press and the
/// release are on one cell and nothing was swept between them, so a drag and a word or row taken by a
/// double or triple click stay selections. `None` where the release ends no click.
fn click_ended_by(session: &Session, mouse: MouseEvent) -> Option<u8> {
    if !session
        .selection
        .is_none_or(|selection| selection.held_still())
    {
        return None;
    }
    let click = session.last_click?;
    (click.cell() == (mouse.row, mouse.column)).then(|| click.count())
}

/// What a click on a row of the info panel, a control of its files state or a hint under the box
/// does.
fn click_shortcut(session: &mut Session, hit: Hit) -> Action {
    session.clear_selection();
    match hit {
        Hit::Footer(footer) => click_footer(session, footer),
        Hit::Panel(crate::panel::Target::Model) => click_command(session, MODEL_COMMAND),
        Hit::Panel(crate::panel::Target::Effort) => click_command(session, EFFORT_COMMAND),
        Hit::Panel(crate::panel::Target::Context) => click_command(session, CONTEXT_COMMAND),
        Hit::Panel(crate::panel::Target::Servers) => click_command(session, MCP_COMMAND),
        Hit::Panel(crate::panel::Target::Languages) => click_command(session, LSP_COMMAND),
        Hit::Panel(crate::panel::Target::File(index)) => {
            session.files_select(index);
            Action::Redraw
        }
        Hit::Panel(crate::panel::Target::CloseFiles) => {
            session.close_files();
            Action::Redraw
        }
        Hit::Panel(crate::panel::Target::ClosePeek) => {
            session.close_peek();
            Action::Redraw
        }
        Hit::Panel(crate::panel::Target::PreviousEdit) => {
            session.files_to_edit(false);
            Action::Redraw
        }
        Hit::Panel(crate::panel::Target::NextEdit) => {
            session.files_to_edit(true);
            Action::Redraw
        }
        Hit::Panel(crate::panel::Target::Files) => click_command(session, DIFF_COMMAND),
        // The key that hides the panel does this, and the choice is kept the way the key keeps it
        // (MOUSE-21).
        Hit::Panel(crate::panel::Target::Hide) => {
            session.toggle_panel();
            Action::Redraw
        }
        // The name goes into the box behind the command that renames, for the person to edit and
        // send, and nothing is renamed by the click (MOUSE-30).
        Hit::Panel(crate::panel::Target::Name) => {
            session.offer_to_rename();
            Action::Redraw
        }
        // The mark of a finished task opens the whole of its words, or closes them (MOUSE-32).
        Hit::Panel(crate::panel::Target::Detail(task)) => {
            session.toggle_plan_detail(task);
            Action::Redraw
        }
        // The view goes to the turn that wrote the task (MOUSE-31).
        Hit::Panel(crate::panel::Target::Task(task)) => {
            session.go_to_task(task, render::plan_item_row);
            Action::Redraw
        }
        // The address of a row of the Pull requests or Issues section: the one `/pr` or `/issue` set,
        // as it was set, or the one the reference builds (MOUSE-41).
        Hit::Panel(
            link @ (crate::panel::Target::PullRequest(_) | crate::panel::Target::Issue(_)),
        ) => match session.panel_link(link) {
            Some(address) => Action::OpenUrl(address),
            None => Action::Redraw,
        },
        // Not rows of the panel or hints, which are all this is asked about. Named, so that a kind of
        // target added to the record has to be given its place here and is not taken for nothing.
        Hit::Back
        | Hit::Reply(_)
        | Hit::Block(..)
        | Hit::Queued(_)
        | Hit::Discard(_)
        | Hit::Popup(_)
        | Hit::PopupRows(_)
        | Hit::Match(_)
        | Hit::Words(_)
        | Hit::Wheel(_)
        | Hit::Scrollbar
        | Hit::Pinned(_)
        | Hit::ToTail
        | Hit::Face
        | Hit::Item(_)
        | Hit::Text { .. }
        | Hit::Link(_)
        | Hit::Call(_)
        | Hit::Delegate(_)
        | Hit::SessionsScreen(_) => Action::None,
    }
}

/// Carry out the command `line` as Enter on it would where the session is now, for a click that
/// stands for it (MOUSE-29).
///
/// The line is never in the box, which a click leaves as it is, and is handed to the same places a
/// typed one goes. At the plain prompt that is [`dispatch_command`], and what it asks for comes back
/// as an action the loop carries out. While a turn runs it is carried out as typed where the table
/// says the command runs mid-turn and nothing waiting ahead of it could change what it acts on, with
/// what it says written as a command's answer is (CMD-8); anywhere else it waits in the queue, with
/// the note a typed line gets. The loops that are not a turn queue every command, as they do a typed
/// one, and so does the loop that runs a command the person typed, where no line can be typed to
/// wait. So a click is never dropped: it does what the typed line does, or says why it waits.
pub(super) fn click_command(session: &mut Session, line: &str) -> Action {
    let commanded = crate::state::Commanded {
        line: line.to_string(),
        pasted: Vec::new(),
        attached: Vec::new(),
        typed_at: std::time::Instant::now(),
        // A click stands for a command nobody typed in the box, so no line says which words were
        // typed.
        names: session.names_from_words_nobody_pasted(line),
    };
    if !matches!(session.status, Status::Working | Status::Running) {
        return dispatch_command(session, commanded);
    }
    if is_carried_out_at_once(session, line) {
        return session.answer_while_working(|session| dispatch_command(session, commanded));
    }
    let why = why_it_waits(session, line);
    if session.queue_command_line(line)
        && let Some(why) = why
    {
        session.answer_while_working(|session| session.note(why));
    }
    Action::Redraw
}

/// What a click on a hint of the row under the box does: what the key it names does (MOUSE-14,
/// MOUSE-48). [`Session::target_at`] has already said the key does something now.
fn click_footer(session: &mut Session, target: crate::state::Footer) -> Action {
    use crate::state::Footer;
    match target {
        // The ladder short of bypassing: a click never grants what stops every later question
        // being asked (MOUSE-14).
        Footer::Mode => session.cycle_permission_mode_by_click(),
        Footer::Trail => session.toggle_trail(),
        Footer::Shortcuts => session.toggle_shortcuts(),
        Footer::Info => session.toggle_panel(),
        Footer::Watch => {
            session.watch();
        }
        Footer::Outputs => {
            session.watch_outputs();
        }
        Footer::Background => {
            session.move_to_background();
        }
        Footer::Held => session.return_to_the_tail(),
        Footer::CancelShell => session.clear_input(),
        Footer::Paste => return Action::Paste,
    }
    Action::Redraw
}
