//! The event loop.
//!
//! Runs turns one at a time. Each turn builds its own policy inside `turn::resume`, so
//! nothing about the interface can extend a policy's life beyond the turn that created
//! it. What does outlive a turn is the conversation, which is how a follow-up like "try that
//! again" has anything to refer to.
//!
//! Turns run synchronously: the interface shows "working" and stops accepting input until
//! the reply arrives. That is honest about what is happening, and it keeps two turns from
//! ever being in flight together.

// The modules under this one are slices of this file, so each names what the rest of the file
// names with `use super::*`, and this file names theirs with a glob in turn.
#![cfg_attr(
    not(test),
    expect(
        clippy::wildcard_imports,
        reason = "the modules under app are slices of one file and share its names without a list"
    )
)]

use bravebot_agent::conversation::Conversation;
use bravebot_agent::lsp::LanguageServers;
use bravebot_agent::turn::{self, PastedImage, Task};
use bravebot_agent::{SessionScratch, Workspace};
use bravebot_config::Attribution;
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::permissions::Permissions;
use bravebot_core::programs::{AskedAbout, TrustedPrograms};
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use bravebot_net::Egress;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event as TermEvent, KeyEvent, KeyEventKind,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::input;
use crate::render;
use crate::select;
use crate::state::{Session, Status};
use bravebot_session::audit::{Stamped, Trail};

// Whole, so the CLI keeps reaching the table as `bravebot_tui::app::commands`, and this file and
// its tests name the commands without a list of them.
pub use crate::command_table::*;

mod actions;
mod sessions_screen;
use actions::Held;
mod copying;
// Reached by `crate::app::open_link_with`, which a `text_links` test calls.
#[cfg(test)]
pub(crate) use copying::open_link_with;
use copying::*;
mod directories;
use directories::*;
mod keys;
// The glob for what this file and its tests call. The four `pub` ones stay public as
// `bravebot_tui::app::..`, the path `tests/references.rs` names them by.
use keys::*;
pub use keys::{Action, handle_key, handle_key_while_working, handle_mouse};

mod models;
use models::*;
// Public for the CLI, which names them as `bravebot_tui::app::..`.
pub use models::{
    account_models_to_start_on, adopt_listing_for_model, configured_models_to_start_on,
    reads_structured_output, xai_models_to_start_on,
};

mod opening;
use opening::*;

mod records;
pub(crate) use records::coverage_causes;
use records::*;

mod session_commands;
pub use session_commands::set_language;
use session_commands::*;

mod side_requests;
use side_requests::*;

mod terminal;
pub(crate) use terminal::TRACK_MOTION_ONLY_WHILE_DRAGGING;
use terminal::*;

mod turns;
pub use turns::with_submitted_attachments;
// Reached by `crate::from_worker`, which takes the messages that ask nothing into the session.
pub(crate) use turns::progress_of_a_turn;
use turns::*;

/// How long to wait for a key before redrawing. Short enough that a status change appears
/// promptly, long enough not to spin.
const POLL: Duration = Duration::from_millis(100);

/// How often a session with nothing happening in it still owes the screen a frame.
///
/// One second, which is the finest the countdown under the box is spelled to. Frames are drawn from
/// what changed, and a loop waiting for its next tick changes nothing: the row would hold the
/// moment it last drew for the whole of an interval, so a five-minute loop would say `next in 4m
/// 58s` for five minutes and correct itself the moment somebody pressed a key for an unrelated
/// reason. That is the one state the row exists to report, so it is the one state it cannot be
/// stale in.
const COUNTDOWN: Duration = Duration::from_secs(1);

/// Whether a pass with no event in it owes the rows around the box a frame anyway: the countdown
/// under it while a loop is live, and the light above it while anything is still active
/// ([WATCH-25](../../../docs/specs/watching.md#WATCH-25)). A session with a watch or a job and no
/// loop is owed one too, since the light changes only if a frame is drawn.
fn a_session_is_owed_a_frame(session: &Session, since_drawn: Duration) -> bool {
    session.is_standing() && since_drawn >= COUNTDOWN
}

/// How often to look at the face on the opening screen. A blink keeps the lids shut for about a
/// tenth of a second, so the wait for a key has to be shorter than that for it to be drawn.
const FACE_POLL: Duration = Duration::from_millis(40);

/// Whether the opening screen owes its face a frame: the face is on screen and is held differently
/// from the last time it was looked at. Every other screen has no face to repaint.
fn a_face_is_owed_a_frame(
    opening: bool,
    held: crate::avatar::Look,
    shown: crate::avatar::Look,
) -> bool {
    opening && held != shown
}

/// How long to wait for a key. Short only while a face on the opening screen can move, so a
/// session past its first message, or one that asked for no motion, does not wake any faster.
fn wait_for_a_key(opening: bool, stilled: bool) -> Duration {
    if opening && !stilled { FACE_POLL } else { POLL }
}

/// How often to redraw while a turn runs. Matches the spinner's own frame time so the animation
/// advances by one glyph per redraw rather than skipping.
const FRAME: Duration = Duration::from_millis(120);

/// What `/status` says: the session's standing, read now.
///
/// `rules` is the trust map and the vouched programs where the caller holds them, and `None` while
/// a turn does, since it answers into both as it runs (CMD-8).
fn status_report(
    session: &Session,
    stored: &bravebot_session::sessions::Handle,
    workspace: &Workspace,
    scratch: Option<&std::path::Path>,
    config: &Config,
    rules: Option<(&TrustStore, &TrustedPrograms)>,
) -> crate::status::Report {
    let empty_trust = TrustStore::new(workspace.root());
    let empty_programs = TrustedPrograms::default();
    let theme = crate::theme::name();
    // Read here rather than held, for the reason the run prompt reads it where it would
    // draw: the file belongs to every session begun in this directory, so a person
    // asking what they are carrying should be told what the file says now.
    let record = remembered_record(workspace);
    // Read now for the same reason: another session here may have kept or withdrawn it.
    let kept = honoured(workspace.root()).map(|honoured| {
        let root = honoured.kept.root.clone();
        (
            bravebot_session::sessions::how_long_ago(honoured.kept.at),
            honoured.store.path().to_path_buf(),
            Some(root).filter(|root| root != workspace.root()),
        )
    });
    let checkouts = workspace.session_checkouts();
    // Read now for the same reason: `/reach` in another session here may have added or removed one.
    let reach = bravebot_agent::home::directory()
        .map(|home| bravebot_agent::reach::listed(&home, stored.id(), workspace.root()))
        .unwrap_or_default();
    let requested_paths = bravebot_agent::reach::requested_paths_listed(&workspace.path_reach());
    let reached_directories =
        bravebot_agent::reach::reached_directories_listed(&workspace.reached_directories());
    crate::status::report(&crate::status::Facts {
        session_name: stored.title(),
        session_id: stored.id(),
        directory: workspace.root(),
        added_directories: &workspace.added_directories(),
        scratch,
        checkouts: &checkouts,
        model: session.model(),
        agent: session.standing_definition(),
        agent_by_setting: session.standing_is_by_setting(),
        effort: session.effort(),
        effort_for_this_session: session.effort_is_for_this_session(),
        model_reads_effort: session.model_reads_effort(),
        account_effort: session.account_level(),
        served_model: session.served_model(),
        substituted_model: session.substituted_model(),
        premium: session.premium(),
        theme: &theme,
        config,
        confinement: &session.confinement,
        sockets_with_a_path_reachable: session.sockets_with_a_path_reachable,
        sandbox_mode: session.sandbox_mode(),
        sandbox_chosen: crate::sandbox_command::detail(session.sandbox()),
        servers: &session.servers,
        permission_mode: session.permission_mode(),
        began_in_bypass: session.began_in_bypass(),
        programs_write_unasked: crate::status::programs_write_unasked(
            session.sandbox_mode(),
            session.permission_mode(),
            bravebot_agent::home::profile().as_deref(),
        ),
        auto_vetting: session.auto_vetting(),
        turns: session.turns,
        tokens: session.tokens,
        tokens_in_flight: session.running_tokens(),
        timing: session.timing_total(),
        cached: session.cached(),
        breakpoints_given_up: session.breakpoints_given_up(),
        trust: rules.map_or(&empty_trust, |(trust, _)| trust),
        programs: rules.map_or(&empty_programs, |(_, programs)| programs),
        turn_holds_rules: rules.is_none(),
        looping: session.loops().as_slice(),
        watches: session.watches(),
        jobs: session.jobs().collect(),
        goal: session.goal(),
        remembered: record
            .as_ref()
            .map(|(store, lines)| crate::status::Remembered {
                lines,
                path: store.path(),
            }),
        reach: &reach,
        requested_paths: &requested_paths,
        reached_directories: &reached_directories,
        kept_trust: kept
            .as_ref()
            .map(|(when, path, root)| crate::status::KeptTrust {
                when,
                path,
                root: root.as_deref(),
            }),
    })
}

/// What a command carried out mid-turn reaches besides the session, none of which the turn holds.
struct Beside<'a> {
    /// The record the session is kept under. The turn is given only its id, and the record is
    /// written after the turn with whatever name the session has by then.
    stored: &'a mut bravebot_session::sessions::Handle,
    /// The working directory, by name only: what `/forget-trust` withdraws is kept about it, and
    /// `/export` writes under it.
    root: &'a std::path::Path,
    /// Where that answer is kept, or `None` where the platform names nowhere.
    home: Option<&'a std::path::Path>,
    /// The workspace the turn works in, which `/status` reads and the turn only borrows.
    workspace: &'a Workspace,
    /// The session's scratch directory, which `/status` names.
    scratch: Option<&'a std::path::Path>,
    /// The configuration the turn was started with. The turn holds a copy of its own, so `/model`
    /// may change this one for the turn after.
    config: &'a mut Config,
    /// The managed settings, read once before the turn, which `/sandbox` holds a mode to (CMD-8).
    managed: &'a bravebot_config::Managed,
    /// The settings the session was started with, which say the servers `/mcp` lists as requested.
    settings: &'a bravebot_config::Settings,
}

/// A press during a turn that is not one of the keys that only stop it.
///
/// Ctrl-Enter is stopped here like any other stop, and what is waiting goes from the queue once the
/// turn has ended, as it would have after any turn.
///
/// A command the key handler carried out as it was typed, but which needs more than the session to
/// finish, is finished here, and what it says goes into the transcript as the key handler's own
/// does (CMD-8).
fn turn_key(
    session: &mut Session,
    key: KeyEvent,
    cancel: &Cancel,
    beside: &mut Beside<'_>,
) -> Action {
    let action = handle_key_while_working(session, key);
    carry_out_held_pick(session, beside.config);
    carry_out_while_working(session, action, cancel, beside)
}

/// Carry out what a key or a click answered with while a turn runs.
///
/// Both come here so a command a click stands for finishes as the same command typed does: a click
/// on the panel's model row asks for the picker the way `/model` does, and the loop that holds the
/// configuration opens it (MOUSE-29).
fn carry_out_while_working(
    session: &mut Session,
    action: Action,
    cancel: &Cancel,
    beside: &mut Beside<'_>,
) -> Action {
    match action {
        Action::SendNow => stop_what_is_running(session, cancel),
        Action::Rename(name) => {
            session.answer_while_working(|session| rename_session(session, beside.stored, &name));
        }
        Action::Note(text) => {
            session.answer_while_working(|session| note_session(session, beside.stored, &text));
        }
        Action::Link(kind, argument) => session.answer_while_working(|session| {
            link_session(session, beside.stored, kind, &argument);
        }),
        Action::ForgetTrust => session.answer_while_working(|session| {
            session.note(forget_trust(beside.home, beside.root));
        }),
        Action::Status => session.answer_while_working(|session| {
            let report = status_report(
                session,
                beside.stored,
                beside.workspace,
                beside.scratch,
                beside.config,
                None,
            );
            session.report(report);
        }),
        // Only the forms that list reach here (CMD-8): the servers are read as the session holds
        // them, which the turn does not change, and none is started, stopped or asked about.
        Action::Mcp(argument) if crate::mcp_command::only_lists(&argument) => {
            session.answer_while_working(|session| {
                crate::mcp_command::list(session, beside.settings, beside.home);
            });
        }
        Action::SetTheme(name) => {
            session.answer_while_working(|session| set_theme(session, &name));
        }
        Action::SetEffort(level, scope) => {
            session.answer_while_working(|session| set_effort(session, &level, scope));
        }
        Action::Sandbox(word) => session.answer_while_working(|session| {
            let kept = session.sandbox_mode();
            crate::sandbox_command::run(session, &word, beside.managed);
            if session.sandbox_mode() != kept {
                session.note(t!(session_sandbox_turn_follows_it).to_string());
            }
        }),
        Action::Variant(word) => {
            session.answer_while_working(|session| set_variant(session, beside.config, &word));
        }
        Action::Advisor(word) => {
            session.answer_while_working(|session| set_advisor(session, beside.config, &word));
        }
        Action::Style(word) => {
            session.answer_while_working(|session| set_style(session, beside.workspace, &word));
        }
        Action::Language(word) => {
            session.answer_while_working(|session| set_language(session, &word));
        }
        Action::CrashReports(word) => session.answer_while_working(|session| {
            crate::crash_reports_command::run(session, &word, beside.managed);
        }),
        Action::CopyReply(text) => session.answer_while_working(|session| {
            copy_reply(session, &text, crate::clipboard::copy);
        }),
        // A link `/open` named, opened as a click on it opens it: starting the browser changes
        // nothing the turn holds (MOUSE-40, CMD-30).
        Action::OpenUrl(address) => open_link(session, &address, true),
        Action::Export(path) => session.answer_while_working(|session| {
            export_session(session, beside.stored, beside.root, path.as_deref());
        }),
        Action::ChooseModel(scope) => session.answer_while_working(|session| {
            open_model_picker(session, beside.config, scope);
        }),
        // The sessions beside this one can be looked at, stopped, named, archived and deleted while
        // a turn runs. Opening one ends this session, which a turn in flight does not allow
        // without being stopped, and that is asked once (SCREEN-15).
        Action::OpenCenter => sessions_screen::open_sessions_screen(
            session,
            beside.stored.id(),
            beside.root,
            None,
            false,
        ),
        Action::SessionsScreen(crate::sessions_screen::Effect::Rename { task, name })
            if task.current =>
        {
            session.answer_while_working(|session| {
                rename_session(session, beside.stored, &name);
            });
            sessions_screen::read_sessions_screen(session, Instant::now());
        }
        // The question answered, so the turn is stopped as its stop key stops it, and what was
        // asked waits for the turn to end (SCREEN-15).
        Action::SessionsScreen(crate::sessions_screen::Effect::StopTurnThen(then)) => {
            session.hold_for_the_end_of_the_turn(*then);
            keys::stop_what_is_running(session, cancel);
        }
        // Opening, copying and beginning a session end this one, so the question is put before the
        // turn is touched (SCREEN-15).
        Action::SessionsScreen(effect) => {
            sessions_screen::ask_to_stop_the_turn(session, beside.root, effect)
        }
        Action::ChooseTheme => open_theme_picker(session),
        Action::Aside(question, pasted, attached) => session.answer_while_working(|session| {
            ask_aside_beside_a_turn(session, beside.config, &question, &pasted, &attached);
        }),
        Action::ChooseEffort(scope) => open_effort_picker(session, scope),
        Action::ChooseEditing => open_editing_picker(session),
        action => return act_while_working(session, action, crate::clipboard::paste),
    }
    Action::None
}

/// Carry out the row a picker was left on, if it was left on one.
///
/// Said in the transcript as any command carried out mid-turn is. Asked after every key, and every
/// frame as well: a row taken while another working loop had the keys is held until a loop that
/// holds the configuration can carry it out, and the turn that follows is the first.
fn carry_out_held_pick(session: &mut Session, config: &mut Config) {
    if let Some(picked) = session.take_pick() {
        let sandbox_pick = matches!(picked, crate::picking::Picked::Sandbox(_));
        session.answer_while_working(|session| {
            let kept = session.sandbox_mode();
            carry_out_pick(session, config, picked);
            // The turn running follows a new mode, and is told so as `/sandbox <word>` tells it.
            if sandbox_pick && session.sandbox_mode() != kept {
                session.note(t!(session_sandbox_turn_follows_it).to_string());
            }
        });
    }
}

/// Carry out what the picker closed on, which a key handler left for whoever can.
///
/// Said as a command carried out mid-turn is, so it goes into the transcript as it is said
/// (CMD-8).
fn carry_out_pick(session: &mut Session, config: &mut Config, picked: crate::picking::Picked) {
    use crate::picking::Picked;
    match picked {
        Picked::Model(model, scope) => model_picked(session, config, model, scope),
        Picked::Theme(theme) => theme_picked(session, &theme.name),
        Picked::Effort(row, scope) => effort_picked(session, row.0, scope),
        Picked::Editing(row) => config_picked(session, row),
        // The word a person would have typed, through the function they would have typed it to, so
        // the managed floor, the already-held sentence and the jobs note are the same ones.
        Picked::Sandbox(row) => {
            crate::sandbox_command::run(session, row.0.name(), &bravebot_config::Managed::load())
        }
    }
}

/// What a command's line can take with it, which is decided by what answers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Carries {
    /// Everything a prompt carries, because what answers the line is a turn.
    ///
    /// `/loop`, `/agent` and `/plan`. A tick is an ordinary turn, so its first one carries the pictures and
    /// the files the person named exactly as the prompt they typed them into would have, and an
    /// addressed run is a turn of the session's own.
    Everything,
    /// Bytes and nothing else, because what answers the line is one request and no turn.
    ///
    /// `/btw` and `/manifest`. A picture and a PDF are bytes the request can hold. A text file's
    /// contents would be a context message, and both of those planners are precommitted to a context
    /// holding the task and the driver's own words and nothing read out of the tree.
    Bytes,
    /// Nothing, because the driver carries the command out itself and there is no request at all.
    ///
    /// Every other command, `/goal` included: a condition is judged against a turn that has not
    /// happened yet, so a picture pasted into one has no moment to be shown at.
    Nothing,
}

/// What the line is, read off the line: both callers have one and neither knows any more than that.
fn carries(line: &str) -> Carries {
    if [LOOP_COMMAND, AGENT_COMMAND, PLAN_COMMAND]
        .iter()
        .any(|command| argument_to(line, command).is_some())
    {
        return Carries::Everything;
    }
    match [BTW_COMMAND, MANIFEST_COMMAND]
        .iter()
        .any(|command| argument_to(line, command).is_some())
    {
        true => Carries::Bytes,
        false => Carries::Nothing,
    }
}

/// Carry out the command a line is, whether it was typed just now or queued while a turn ran.
///
/// One place both callers reach, because the two differ only in when the line arrives. Enter at rest
/// dispatches the line the box holds; the loop dispatches the line somebody queued once the turn it
/// was waiting for has ended. Neither reads anything but the line, so a queued command cannot be
/// confused by whatever is in the box by the time it runs.
///
/// The line, not the box, for that reason: a person who queued `/clear` and then began typing
/// something else has a box that is theirs, and this must not read it or write it.
///
/// Every word [`commands`] names is answered here. The last line is unreachable for any of them and
/// is a table entry that nobody wired up: `every_command_in_the_table_dispatches` is what says so,
/// since a word recognised and then quietly dropped would be worse than one never recognised.
fn dispatch_command(session: &mut Session, commanded: crate::state::Commanded) -> Action {
    // What the line named survives only as far as the command can take it. For a word the driver
    // carries out itself the marker becomes a sentence saying a picture was pasted, or the dropped
    // file's own name, since a path or a name that came out of a paste was never going to be one.
    let crate::state::Commanded {
        line,
        pasted,
        attached,
        typed_at,
        names,
    } = match carries(&commanded.line) {
        Carries::Everything => commanded,
        Carries::Bytes => crate::state::without_text_files(commanded),
        Carries::Nothing => session.without_what_it_cannot_carry(commanded),
    };
    let line = line.as_str();

    if line.trim() == EXIT_COMMAND {
        session.quit();
        return Action::Quit;
    }
    if line.trim() == MODEL_COMMAND {
        return ask_which_model(session, crate::model_prompt::Scope::EverySession);
    }
    if argument_to(line, MODEL_COMMAND) == Some(crate::command_table::MODEL_SESSION_WORD) {
        return ask_which_model(session, crate::model_prompt::Scope::ThisSession);
    }
    if let Some(name) = argument_to(line, THEME_COMMAND) {
        return if name.is_empty() {
            Action::ChooseTheme
        } else {
            Action::SetTheme(name.to_string())
        };
    }
    if let Some(argument) = argument_to(line, EFFORT_COMMAND) {
        return match crate::command_table::effort_request(argument) {
            crate::command_table::EffortAsk::Picker(scope) => Action::ChooseEffort(scope),
            crate::command_table::EffortAsk::Level(level, scope) => {
                Action::SetEffort(level.to_string(), scope)
            }
        };
    }
    if let Some(word) = argument_to(line, VARIANT_COMMAND) {
        return Action::Variant(word.to_string());
    }
    if let Some(word) = argument_to(line, ADVISOR_COMMAND) {
        return Action::Advisor(word.to_string());
    }
    if let Some(word) = argument_to(line, STYLE_COMMAND) {
        return Action::Style(word.to_string());
    }
    if let Some(word) = argument_to(line, MODE_COMMAND) {
        return Action::Mode(word.to_string());
    }
    if let Some(word) = argument_to(line, LANGUAGE_COMMAND) {
        return Action::Language(word.to_string());
    }
    if let Some(word) = argument_to(line, CRASH_REPORTS_COMMAND) {
        return Action::CrashReports(word.to_string());
    }
    if line.trim() == CONFIG_COMMAND {
        return Action::ChooseEditing;
    }
    if line.trim() == STATUS_COMMAND {
        return Action::Status;
    }
    if line.trim() == FORGET_TRUST_COMMAND {
        return Action::ForgetTrust;
    }
    if let Some(argument) = argument_to(line, REACH_COMMAND) {
        return Action::Reach(argument.to_string());
    }
    if let Some(argument) = argument_to(line, MCP_COMMAND) {
        return Action::Mcp(argument.to_string());
    }
    if let Some(argument) = argument_to(line, PLUGIN_COMMAND) {
        return Action::Plugin(argument.to_string());
    }
    // A bare word opens the list of modes, at rest and while a turn runs, and says nothing: the
    // picker is a mode of the session, and the choice goes through `sandbox_command::run` as the
    // word does. A word after it is carried whole to the loop, unchanged (SANDBOX-22).
    if let Some(word) = argument_to(line, SANDBOX_COMMAND) {
        if word.is_empty() {
            session.open_picking(crate::picking::Picking::Sandbox(
                crate::sandbox_prompt::Picker::new(session.sandbox_mode()),
            ));
            return Action::Redraw;
        }
        return Action::Sandbox(word.to_string());
    }
    if line.trim() == COST_COMMAND {
        session.report_spend();
        return Action::Redraw;
    }
    if line.trim() == CONTEXT_COMMAND {
        session.report_context();
        return Action::Redraw;
    }
    if line.trim() == LSP_COMMAND {
        session.report_started_language_servers();
        return Action::Redraw;
    }
    // From the list the session holds: nothing is read, fetched or sent, and no turn starts
    // (CMD-31).
    if line.trim() == ISSUES_COMMAND {
        session.report_references(bravebot_agent::links::LinkKind::Issue);
        return Action::Redraw;
    }
    if line.trim() == PRS_COMMAND {
        session.report_references(bravebot_agent::links::LinkKind::PullRequest);
        return Action::Redraw;
    }
    if let Some(argument) = argument_to(line, OPEN_COMMAND) {
        return match crate::open_command::parse(argument) {
            crate::open_command::Asked::List => {
                session.list_links();
                Action::Redraw
            }
            crate::open_command::Asked::Number(number) => session.open_listed_link(number),
            crate::open_command::Asked::Edit(number) => session.edit_listed_link(number),
            crate::open_command::Asked::Typed(typed) => session.open_typed(&typed, false),
            crate::open_command::Asked::EditTyped(typed) => session.open_typed(&typed, true),
            crate::open_command::Asked::Unreadable => {
                session.note(t!(open_takes));
                Action::Redraw
            }
        };
    }
    if let Some(figure) = argument_to(line, LIMIT_COMMAND) {
        set_limit(session, figure);
        return Action::Redraw;
    }
    if line.trim() == REQUEST_COMMAND {
        session.show_request();
        return Action::Redraw;
    }
    // What the summary must keep is typed by the person and goes to the summariser as it was
    // typed, so it is never a prompt: nothing about it joins the exchange.
    if let Some(focus) = argument_to(line, COMPACT_COMMAND) {
        return Action::Compact(focus.to_string());
    }
    // The question is taken verbatim and never sent as a prompt: it goes out over a copy of the
    // conversation and the copy is thrown away, so nothing about it joins the exchange.
    if let Some(question) = argument_to(line, BTW_COMMAND) {
        return Action::Aside(question.to_string(), pasted, attached);
    }
    // Nothing the person types reaches the question: it is the driver's own sentence, so the line
    // is the command and the argument, if any, is not read.
    if line.trim() == RECAP_COMMAND {
        return Action::Recap;
    }
    if line.trim() == CLEAR_COMMAND {
        return Action::Clear;
    }
    if let Some(path) = argument_to(line, EXPORT_COMMAND) {
        return if path.is_empty() {
            Action::Export(None)
        } else {
            Action::Export(Some(path.to_string()))
        };
    }
    if let Some(back) = argument_to(line, COPY_COMMAND) {
        return match text_to_copy(session, back) {
            Ok(text) => Action::CopyReply(text),
            Err(refused) => {
                session.note(refused);
                Action::Redraw
            }
        };
    }
    if line.trim() == UNDO_COMMAND {
        return Action::Undo;
    }
    if let Some(turns) = argument_to(line, REWIND_COMMAND) {
        return Action::Rewind(turns.to_string());
    }
    if let Some(number) = argument_to(line, HUNKS_COMMAND) {
        return Action::Hunks(number.to_string());
    }
    if let Some(directory) = argument_to(line, ADD_DIR_COMMAND) {
        // `close` and `close <path>` are the command and not a directory called `close`. Such a
        // directory is named with a separator, as `../close` does.
        return match directory.split_once(char::is_whitespace) {
            Some((ADD_DIR_CLOSE, named)) => Action::CloseDirectory(named.trim().to_string()),
            None if directory == ADD_DIR_CLOSE => Action::CloseDirectory(String::new()),
            _ => Action::AddDirectory(directory.to_string()),
        };
    }
    if let Some(directory) = argument_to(line, CD_COMMAND) {
        return Action::ChangeDirectory(directory.to_string());
    }
    if let Some(name) = argument_to(line, RENAME_COMMAND) {
        return Action::Rename(name.to_string());
    }
    if let Some(text) = argument_to(line, NOTE_COMMAND) {
        return Action::Note(text.to_string());
    }
    if let Some(name) = argument_to(line, BRANCH_COMMAND) {
        return Action::Branch(name.to_string());
    }
    // The goal is typed by the person and is the one thing that goes into the request beside the
    // conversation, so it is never a prompt: nothing about it joins the exchange.
    if let Some(goal) = argument_to(line, HANDOFF_COMMAND) {
        return Action::Handoff(goal.to_string());
    }
    if line.trim() == BACKGROUND_COMMAND {
        return Action::Background;
    }
    // Only names which record to pick up: nothing typed here is sent, and the record is read and
    // restored by the path `--resume` takes (CMD-14).
    if let Some(id) = argument_to(line, RESUME_COMMAND) {
        return Action::Resume(id.to_string());
    }
    if let Some(argument) = argument_to(line, ISSUE_COMMAND) {
        return Action::Link(
            bravebot_session::sessions::Link::Issue,
            argument.to_string(),
        );
    }
    if let Some(argument) = argument_to(line, PR_COMMAND) {
        return Action::Link(
            bravebot_session::sessions::Link::PullRequest,
            argument.to_string(),
        );
    }
    // The command that starts the other kind of run. The task is taken verbatim and is never sent
    // as a prompt: the planner that reads it is a fresh one with nothing but the task and the
    // driver's own words in its context, so the conversation neither goes into the run nor hears
    // anything back from it.
    if let Some(task) = argument_to(line, MANIFEST_COMMAND) {
        return Action::Manifest(task.to_string(), pasted, attached);
    }
    // The task is the prompt, as it is for `/agent`; the remainder of the line is sent and the typed
    // line is not (CMD-4). The mode is set before the turn begins, so the turn reads plan mode
    // (MODE-8).
    if let Some(task) = argument_to(line, PLAN_COMMAND) {
        session.enter_plan_mode();
        if task.is_empty() {
            return Action::Redraw;
        }
        return Action::Submit(session.start_planned(task, pasted, attached, names));
    }
    // A turn the driver writes the prompt of. Nothing the person typed after the word reaches it:
    // the word takes no argument, so a line with one was never this command.
    if line.trim() == INIT_COMMAND {
        return match session.start_init() {
            Some(prompt) => Action::Submit(prompt),
            None => Action::Redraw,
        };
    }
    // The same kind of turn, with the target named after the word. A target that names nothing usable
    // is a usage note and no turn.
    if let Some(argument) = argument_to(line, REVIEW_COMMAND) {
        return match crate::review_command::parse(argument) {
            Some(asked) => Action::Submit(session.start_review(&asked, names)),
            None => {
                session.note(t!(review_usage));
                Action::Redraw
            }
        };
    }
    // Sends nothing: the draft goes in the box and the person's own Enter sends it as a prompt
    // (CMD-23). A source that names nothing usable is a usage note and no request.
    if let Some(argument) = argument_to(line, COMMIT_MESSAGE_COMMAND) {
        return match crate::commit_message_command::parse(argument) {
            Some(asked) => Action::CommitMessage(asked),
            None => {
                session.note(t!(commit_message_usage));
                Action::Redraw
            }
        };
    }
    // The question is the person's and goes into a sentence the driver wrote, so the turn is the
    // driver's prompt like `/init`'s. Asked nothing, the word says what it takes and no turn starts.
    if let Some(question) = argument_to(line, HELP_COMMAND) {
        if question.is_empty() {
            session.note(t!(help_usage));
            return Action::Redraw;
        }
        return Action::Submit(session.start_help(question, names));
    }
    // From text this program carries: nothing is read, fetched or sent, and no turn starts.
    if line.trim() == CHANGELOG_COMMAND {
        let running = env!("CARGO_PKG_VERSION");
        session.report_changelog(running, crate::help_command::release_notes(running));
        return Action::Redraw;
    }
    // Carried unparsed, because which names exist is read from the workspace and the trust map,
    // both of which the loop owns.
    if let Some(argument) = argument_to(line, AGENT_COMMAND) {
        return Action::Address(argument.to_string(), pasted, attached, names);
    }
    // The command that sends a prompt rather than the line it was typed on. `/loop 5m check the
    // deploy` arms the loop and hands back "check the deploy", which is what every tick sends from
    // here on. The other two forms send nothing: they read what is repeating and end it, which are
    // the halves of a standing loop a transcript cannot show.
    if let Some(argument) = argument_to(line, LOOP_COMMAND) {
        return match crate::loops::parse(argument) {
            crate::loops::Asked::Report => {
                session.report_loop();
                Action::Redraw
            }
            crate::loops::Asked::Stop(which) => {
                session.stop_loop(which);
                Action::Redraw
            }
            crate::loops::Asked::Start(request) => {
                match session.start_loop(request, pasted, attached, names) {
                    Some(prompt) => Action::Submit(prompt),
                    None => Action::Redraw,
                }
            }
            crate::loops::Asked::Unreadable => {
                session.note(t!(loop_needs_a_prompt));
                Action::Redraw
            }
        };
    }
    // Arms a prompt for later and sends nothing now: the line goes out once, from the loop's own
    // clock, as a prompt and never as a command.
    if let Some(argument) = argument_to(line, AFTER_COMMAND) {
        match crate::loops::parse_after(argument) {
            crate::loops::Delayed::Start(request) => session.start_after(request, typed_at, names),
            crate::loops::Delayed::Unreadable => session.note(t!(after_needs_a_delay)),
        }
        return Action::Redraw;
    }
    // The command that sends nothing. A goal is a condition and not a prompt, so setting one arms it
    // and waits: what it keeps going is whatever the person asks for next.
    if let Some(argument) = argument_to(line, GOAL_COMMAND) {
        match crate::goals::parse(argument) {
            crate::goals::Asked::Report => session.report_goal(),
            crate::goals::Asked::Clear => {
                if !session.clear_goal() {
                    session.note(t!(goal_none));
                }
            }
            crate::goals::Asked::Pause => session.pause_goal(),
            crate::goals::Asked::Resume => session.resume_goal(),
            crate::goals::Asked::Set(condition) => session.start_goal(condition),
        }
        return Action::Redraw;
    }
    // Sends nothing to the turn. The arm is reached only from a line a person typed into the box
    // (CMD-1), and what answers it is the shared crate's subcommands, so a line the planner wrote
    // is text that never gets here.
    if let Some(argument) = argument_to(line, SCHEDULE_COMMAND) {
        session.schedule_asked(argument);
        return Action::Redraw;
    }
    // The command that sends nothing either. Listing and stopping are answered here at any time.
    // Arming goes to the loop, which owns the workspace, the live mode and the trust map the first
    // look needs. A line that arms waits in the queue while a turn runs (CMD-8) and reaches here
    // once the session is idle, so the look never runs while a turn holds the trust map
    // (FSWATCH-7).
    if let Some(argument) = argument_to(line, WATCH_COMMAND) {
        match crate::watch_command::parse(argument) {
            crate::watch_command::Asked::List => session.report_watches(),
            crate::watch_command::Asked::Stop(number) => {
                session.stop_watch(number);
            }
            crate::watch_command::Asked::Arm(path) => return Action::ArmWatch(path),
            crate::watch_command::Asked::Unreadable => session.note(t!(watch_command_takes)),
        }
        return Action::Redraw;
    }
    // Sends nothing to the turn: a stop sets the job's token, which the turn reads at its next
    // step (RUN-27).
    if let Some(argument) = argument_to(line, JOBS_COMMAND) {
        match crate::jobs_command::parse(argument) {
            crate::jobs_command::Asked::List => session.report_jobs(),
            crate::jobs_command::Asked::Stop { name, delegate } => {
                session.stop_job(&name, delegate.as_deref());
            }
            crate::jobs_command::Asked::Unreadable => session.note(t!(jobs_command_takes)),
        }
        return Action::Redraw;
    }
    // Nothing the person typed after the word reaches it, and nothing is sent: what answers it is
    // the map and the filesystem, which the loop owns.
    if line.trim() == MEMORY_COMMAND {
        return Action::ListMemories;
    }
    // Opens the view over what commands printed. Said when there is none, since a typed command
    // that does nothing at all reads as one that was not understood.
    if line.trim() == OUTPUTS_COMMAND {
        return open_outputs(session);
    }
    // Opens the Sessions screen, as Left over an empty box does (SCREEN-1). Nothing the person
    // typed after the word reaches it, and nothing is sent.
    if line.trim() == SESSIONS_COMMAND {
        return Action::OpenCenter;
    }
    if line.trim() == PANEL_COMMAND {
        session.toggle_panel();
        return Action::Redraw;
    }
    // Opens the checklist of the panel's sections. It edits the session and nothing else, so it is
    // the same at rest and while a turn runs (PANEL-17).
    if line.trim() == SIDEBAR_COMMAND {
        session.open_picking(crate::picking::Picking::Sidebar(
            crate::sidebar_prompt::Picker::new(session.panel_sections()),
        ));
        return Action::Redraw;
    }
    if line.trim() == DIFF_COMMAND {
        session.toggle_files();
        return Action::Redraw;
    }
    // From the plan the session holds: nothing is read, fetched or sent, and no turn starts
    // (CMD-28).
    if line.trim() == TODOS_COMMAND {
        session.report_plan();
        return Action::Redraw;
    }
    // From what the session keeps and the files on disk, which the loop owns (OPEN-5).
    if line.trim() == UNFINISHED_COMMAND {
        return Action::Unfinished;
    }
    if line.trim() == THINKING_COMMAND {
        session.toggle_thinking();
        return Action::Redraw;
    }
    if line.trim() == CAFFEINATE_COMMAND {
        session.toggle_caffeinate();
        return Action::Redraw;
    }
    // For a terminal that was reset or cleared from outside, which no event reports.
    if line.trim() == REDRAW_COMMAND {
        session.retake_terminal = true;
        return Action::Redraw;
    }
    if let Some(argument) = argument_to(line, CHECKOUTS_COMMAND) {
        return match crate::checkouts_command::parse(argument) {
            crate::checkouts_command::Asked::List => Action::ListCheckouts,
            crate::checkouts_command::Asked::Remove(id) => Action::RemoveCheckout(id),
            crate::checkouts_command::Asked::Apply(id, paths) => Action::ApplyCheckout(id, paths),
            crate::checkouts_command::Asked::Unreadable => {
                session.note(t!(checkouts_command_takes));
                Action::Redraw
            }
        };
    }
    Action::None
}

/// Open the view over what commands printed, and say so where no command has run.
///
/// The one path `/outputs` and the outputs chord both take (WATCH-7), so the two cannot come to
/// disagree about what an empty list does.
fn open_outputs(session: &mut Session) -> Action {
    if !session.watch_outputs() {
        session.note(t!(outputs_none));
    }
    Action::Redraw
}

/// The next thing the queue is holding for the loop, if the session is free to take it.
///
/// A command somebody queued while a turn ran is carried out here rather than anywhere in the key
/// handling, because nobody is going to press anything to make it happen: the press already happened,
/// and this is the loop keeping the promise the queue made. It becomes the action Enter on that line
/// would have become at rest, which is the whole of what was deferred.
///
/// A prompt waiting behind a command goes from here too. The turn loop sends what is queued as each
/// turn ends and stops at a command, so without this the line behind one would wait for a key press
/// that nobody is going to make.
fn queued_next(session: &mut Session) -> Option<Action> {
    // The first prompt of a session a handoff or `-i` has just begun, which nobody will press
    // anything to send: Enter on the brief, or the command line, already did.
    if let Some(prompt) = session.start_handed_over() {
        return Some(Action::Submit(prompt));
    }
    if let Some(commanded) = session.take_queued_command() {
        return Some(dispatch_command(session, commanded));
    }
    // And a command line somebody queued in shell mode, which is here for the same reason and
    // becomes what Enter on it at rest becomes. Each of the three asks about the head of the queue
    // and answers for nothing else, so the order they are asked in decides nothing: what somebody
    // typed first happens first.
    if let Some(line) = session.take_queued_shell() {
        return Some(Action::Run(line));
    }
    if let Some(line) = session.take_queued_private_shell() {
        return Some(Action::RunPrivately(line));
    }
    session.send_queued().map(Action::Submit)
}

/// What an idle session does next of its own accord: the queue first, then the recap owed to a
/// person who has been away (CMD-20). The recap comes after the queue so a line somebody typed is
/// never held back by it, and it is owed once: the arm that runs it calls [`Away::recapped`].
fn queued_or_recap(session: &mut Session, now: Instant) -> Option<Action> {
    queued_next(session).or_else(|| {
        session
            .away
            .due(now, session.turns)
            .then_some(Action::Recap)
    })
}

/// Settle a `/agent` line against the definitions this session resolved, and start the turn it
/// names where it names one.
///
/// Every refusal is a note rather than a failed turn, because a failed turn shows a person a
/// category and each of these is a sentence about the line they typed. The turn asks the kernel
/// the same question again, so a definition that changed on disk in between is refused there
/// rather than run.
fn address(
    session: &mut Session,
    config: &Config,
    definitions: &bravebot_core::delegate::Definitions,
    argument: &str,
    pasted: Vec<crate::state::AttachedImage>,
    attached: Vec<crate::state::Attached>,
    names_of_the_line: crate::state::NamesFrom,
) -> Action {
    let names = definitions.names().join(", ");
    let (name, task) = argument
        .split_once(char::is_whitespace)
        .map_or((argument, ""), |(name, task)| (name, task.trim()));
    if name.is_empty() {
        session.note(t!(agent_resolved, names = names));
        return Action::Redraw;
    }
    let addressed = match definition_named(config, definitions, name) {
        Ok(addressed) => addressed,
        Err(refused) => {
            session.note(refused.into_message());
            return Action::Redraw;
        }
    };
    if task.is_empty() {
        session.note(t!(agent_needs_a_task, name = name));
        return Action::Redraw;
    }
    Action::Submit(session.address(addressed, task, pasted, attached, names_of_the_line))
}

/// Why a definition a name selects cannot be worked under.
///
/// Kept apart because a session resumed under a name its record carries goes on without a
/// definition that no longer exists and refuses one it cannot reach: continuing without a model
/// that needs a sign-in would substitute the planner's (ADDRESS-11, CLI-9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unusable {
    /// Nothing in the set a turn starting now would resolve has the name.
    Missing(String),
    /// The definition exists, and the model it asks for is refused or needs a sign-in.
    Refused(String),
}

impl Unusable {
    /// The sentence saying why, for whoever is to read it.
    pub fn into_message(self) -> String {
        match self {
            Unusable::Missing(message) | Unusable::Refused(message) => message,
        }
    }
}

/// What a session or a run is to work under, once the name typed on the command line, the name
/// its record carries and the name the person's settings carry have been weighed.
#[derive(Debug, PartialEq, Eq)]
pub enum Settled<T> {
    /// No source named a definition.
    Nothing,
    /// The session works under `name`, which `work` accepted as `found`, and its record says so.
    /// `by_setting` is true where the `agent` setting is what named it.
    Under {
        name: String,
        found: T,
        by_setting: bool,
    },
    /// The record's definition no longer exists. The session goes on without it, says so, and
    /// stops recording the name.
    Gone { name: String, why: String },
    /// The `agent` setting names a definition that does not exist. The session goes on without
    /// one, says so, and records nothing.
    SettingGone { name: String, why: String },
}

/// Decide which definition a session or run works under, the one policy every front end shares
/// (ADDRESS-3, ADDRESS-13).
///
/// A name typed on this command line wins and ends the session when `work` finds nothing for it.
/// A recorded name is next. It is the person's own earlier `--agent`, so one that matches nothing
/// now is reported as [`Settled::Gone`] for the caller to say in its own way, and one whose model
/// is refused or needs a sign-in ends the session too, because going on would substitute the
/// planner's model for the one the definition asked for (ADDRESS-11, CLI-9). The `agent` setting
/// is last and is a default, so one that matches nothing is [`Settled::SettingGone`] and does not
/// end a session that never asked for it, while one whose model is refused ends it for the reason
/// a recorded name does. `work` is given the name to try and is called at most once.
pub fn settle_definition<T>(
    typed: Option<&str>,
    recorded: Option<&str>,
    configured: Option<&str>,
    work: impl FnOnce(&str) -> Result<T, Unusable>,
) -> Result<Settled<T>, String> {
    match (typed, recorded, configured) {
        (Some(name), _, _) => work(name)
            .map(|found| Settled::Under {
                name: name.to_string(),
                found,
                by_setting: false,
            })
            .map_err(Unusable::into_message),
        (None, Some(name), _) => match work(name) {
            Ok(found) => Ok(Settled::Under {
                name: name.to_string(),
                found,
                by_setting: false,
            }),
            Err(Unusable::Missing(why)) => Ok(Settled::Gone {
                name: name.to_string(),
                why,
            }),
            Err(refused) => Err(refused.into_message()),
        },
        (None, None, Some(name)) => match work(name) {
            Ok(found) => Ok(Settled::Under {
                name: name.to_string(),
                found,
                by_setting: true,
            }),
            Err(Unusable::Missing(why)) => Ok(Settled::SettingGone {
                name: name.to_string(),
                why,
            }),
            Err(refused) => Err(refused.into_message()),
        },
        (None, None, None) => Ok(Settled::Nothing),
    }
}

/// Work every turn of `session` under the definition `name` selects, or say why it selects none.
///
/// Matched against the set a turn starting now would resolve under `trust`, as a `/agent` line is
/// (ADDRESS-5). The note is shown once at the start, and `/status` shows the definition for the
/// rest of the session, because the note scrolls away.
///
/// A definition naming a model makes that the session's model, so its window is asked for here, as
/// the session's own model's was when the session opened.
fn work_under(
    session: &mut Session,
    config: &mut Config,
    workspace: &Workspace,
    home: Option<&std::path::Path>,
    trust: &TrustStore,
    permissions: &bravebot_core::permissions::Permissions,
    name: &str,
) -> Result<(), Unusable> {
    let definitions = bravebot_agent::agents::resolved(
        workspace,
        home,
        trust.clone(),
        permissions.clone(),
        &mut Trail::new(),
    );
    let definition = definition_named(config, &definitions, name)?;
    let names_a_model = definition.model.is_some();
    session.note(t!(session_working_under, definition = name));
    session.work_under(definition);
    if names_a_model {
        ask_for_the_budget_of_the_model_in_force(session, config);
    }
    Ok(())
}

/// The definition `name` selects from `definitions`, or the sentence saying why it selects none.
///
/// Applies the two refusals a `/agent` line with a task can meet, in the same words, so a name
/// given on the command line is refused as the same name typed into the box would be (ADDRESS-5,
/// ADDRESS-11).
pub fn definition_named(
    config: &Config,
    definitions: &bravebot_core::delegate::Definitions,
    name: &str,
) -> Result<crate::state::Addressed, Unusable> {
    let Some(definition) = definitions.get(name) else {
        return Err(Unusable::Missing(t!(
            agent_no_such_definition,
            name = name,
            names = definitions.names().join(", ")
        )));
    };
    // The machine-level layer first: a model this machine does not request is refused whatever the
    // credentials for it are, and asking about a sign-in would send somebody to fix the wrong thing
    // (BACKEND-48).
    if let Some(written) = definition.model()
        && let Some((file, why)) = config.model_refused(written)
    {
        return Err(Unusable::Refused(t!(
            delegate_model_refused,
            definition = name,
            model = written,
            reason = bravebot_agent::backend::refusal_reason(file, why)
        )));
    }
    if let Some(written) = definition.model()
        && let Some(id) = config.unconfigured_service(&config.model_named(written))
    {
        return Err(Unusable::Refused(t!(
            delegate_model_unconfigured,
            definition = name,
            model = written,
            id = id
        )));
    }
    if let Some(written) = definition.model()
        && bravebot_agent::backend::Backend::needs_sign_in(config, &config.model_named(written))
    {
        return Err(Unusable::Refused(t!(
            delegate_model_needs_sign_in,
            definition = name,
            model = written
        )));
    }
    Ok(crate::state::Addressed {
        name: name.to_string(),
        model: definition
            .model()
            .map(|written| config.model_named(written)),
    })
}

/// Carry out what a key or a paste answered with while a turn is running.
///
/// [`Action::Paste`] is the one answer either handler a working loop calls gives that the session
/// cannot carry out itself, since reading the clipboard runs the platform's own tools. Everything
/// else they return is a redraw the loops do every frame regardless, bar [`Action::Suspend`], which
/// needs the terminal that only [`pump_terminal`] holds. That one is handed back, and the closure
/// that read the key returns it to the pump (INPUT-47).
///
/// Every working loop goes through here, by way of [`pump_terminal`], rather than answering the
/// event and dropping what came back. Four of them did exactly that, so Ctrl-V mid-turn reached its arm, produced the right
/// answer, and had it thrown away: the clipboard was never read and the picture never staged, on a
/// key the idle path has always answered (INPUT-9, PASTE-7).
///
/// The read arrives as an argument for the reason [`take_from_clipboard`] takes one: a test can
/// then say what the clipboard held and watch the picture land in the box, rather than assert only
/// that the answer came back, which is what left the discard invisible.
#[must_use = "a suspension has to reach the pump, which holds the terminal"]
fn act_while_working(
    session: &mut Session,
    action: Action,
    read: impl FnOnce() -> crate::clipboard::Pasted,
) -> Action {
    if action == Action::Paste {
        take_from_clipboard(session, read());
    }
    #[cfg(unix)]
    if action == Action::Suspend {
        return Action::Suspend;
    }
    // A copy that a key asked for, which the item of the menu a right click opened does, needs the
    // terminal that only [`pump_terminal`] holds (MOUSE-52).
    if matches!(action, Action::Copy | Action::CopyReply(_)) {
        return action;
    }
    Action::None
}

/// What a working loop is handed to answer: a key, or what a click asked for.
///
/// A click is answered by the session where it can be, and what it leaves for a loop to carry out
/// is handed to the loop with the rest of what the loop holds (MOUSE-29). A copy is not handed on:
/// a selection needs the terminal that only [`pump_terminal`] holds, and a reply copied by a click is
/// carried out for every loop alike, both by [`answer_mouse`].
enum Heard {
    Key(KeyEvent),
    Click(Action),
}

/// A working loop that has no use for what a click asks for, which is every loop but a turn's:
/// each queues a command a click stands for, as it does one typed, so none is left to carry out.
fn only_keys(
    mut on_key: impl FnMut(&mut Session, KeyEvent) -> Action,
) -> impl FnMut(&mut Session, Heard) -> Action {
    move |session, heard| match heard {
        Heard::Key(key) => on_key(session, key),
        Heard::Click(_) => Action::None,
    }
}

/// Wait for the terminal for a frame, and handle everything that has arrived.
///
/// Every loop that waits on a worker does this the same way, and the waiting is done here so that
/// a key press wakes the loop rather than queueing behind the worker. Everything waiting is read,
/// not one event per pass: a drag read one event at a time takes seconds to catch up with the
/// pointer.
///
/// A key goes to `on_input`, which is the one part that differs between the loops. A paste and the
/// mouse are the same in all of them, so they are answered here, and a paste reaches
/// [`act_while_working`] so the clipboard is read for it. What `on_input` answers a key with is carried
/// out here when it needs the terminal, which no loop's own closure holds: [`Action::Suspend`] gives
/// the terminal back, stops, and takes it again, and the loop carries on afterwards (INPUT-47). What a
/// click asks for other than a copy goes to `on_input` as well.
fn pump_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    mut on_input: impl FnMut(&mut Session, Heard) -> Action,
) -> io::Result<()> {
    // Sooner while a drag held outside the transcript is scrolling it, since the pointer held still
    // sends nothing to wake the loop for the next step (MOUSE-37).
    let wait = if session.pulling_at_an_edge() {
        select::EDGE_TICK
    } else {
        FRAME
    };
    if input::poll(wait)? {
        while input::poll(Duration::ZERO)? {
            let taken = input::read()?;
            took_input(session, &taken);
            match taken {
                TermEvent::Key(key) => {
                    let answered = on_input(session, Heard::Key(key));
                    // The turn stops with the process and goes on after it.
                    #[cfg(unix)]
                    if answered == Action::Suspend {
                        suspend_session(terminal, session)?;
                    }
                    // What the menu a right click opened asked for with a key is carried out as a
                    // click's is (MOUSE-52).
                    match answered {
                        Action::Copy => {
                            copy_selection(terminal, session, crate::clipboard::copy)?;
                        }
                        reply @ Action::CopyReply(_) => {
                            copy_a_reply_clicked(session, reply, crate::clipboard::copy);
                        }
                        _ => {}
                    }
                }
                TermEvent::Paste(text) => {
                    let action = handle_paste(session, &text);
                    // A paste never asks for a stop, so there is nothing to hand on.
                    let _ = act_while_working(session, action, crate::clipboard::paste);
                }
                TermEvent::Mouse(mouse) => {
                    // What a report leaves that is not a copy is for the loop to carry out
                    // (MOUSE-29). Nothing a click leaves can suspend or stop anything, so what the
                    // loop answers is not handed on.
                    let left = answer_mouse(terminal, session, mouse, crate::clipboard::copy)?;
                    // A click on the hint that names the paste key asks for the clipboard, which
                    // only a loop that reads the terminal can take (MOUSE-48).
                    if left == Action::Paste {
                        take_from_clipboard(session, crate::clipboard::paste());
                    } else {
                        let _ = on_input(session, Heard::Click(left));
                    }
                }
                _ => {}
            }
        }
    }
    // Whether or not anything arrived: the step is owed to a pointer that stayed where it was.
    session.scroll_toward_the_pointer(Instant::now());
    // And the list of sessions, if the Sessions screen is open over the work: nobody presses a key for a
    // session elsewhere to change (SCREEN-1).
    sessions_screen::read_sessions_screen_if_due(session, Instant::now());
    Ok(())
}

/// Answer one mouse report where a turn or a command is running: a selection the report ended, or a
/// reply it asked to copy, goes to the clipboard through `copy`, and a link it opened goes to the
/// browser (MOUSE-40). Opening a link changes nothing the work in flight holds, so it is not held
/// back as a command that acts on the conversation would be.
///
/// The event is handled before the action is looked at, because handling it scrolls and moves the
/// selection whatever it returns. A match guard would hide that. What the report asks for that is
/// not carried out here is given back, for a loop that holds what carrying it out takes (MOUSE-29).
fn answer_mouse<B>(
    terminal: &mut Terminal<B>,
    session: &mut Session,
    mouse: ratatui::crossterm::event::MouseEvent,
    copy: impl FnOnce(&str) -> bool,
) -> io::Result<Action>
where
    B: ratatui::backend::Backend,
    B::Error: Send + Sync + 'static,
{
    match handle_mouse(session, mouse) {
        Action::Copy => {
            copy_selection(terminal, session, copy)?;
            Ok(Action::None)
        }
        reply @ Action::CopyReply(_) => {
            copy_a_reply_clicked(session, reply, copy);
            Ok(Action::None)
        }
        Action::OpenUrl(address) => {
            open_link(session, &address, true);
            Ok(Action::None)
        }
        left => Ok(left),
    }
}

/// Interpret a paste, at rest and while a turn is running.
///
/// Answer a paste the terminal delivered, which is what Command-V and the middle mouse button
/// become by the time they reach this process. A paste is one act rather than a run of keys, which
/// is the whole point of asking the terminal for it separately: the text lands in the box and the
/// user decides when to send it. It never submits, whatever it ends with.
///
/// A paste that arrives carrying nothing is the interesting one. It means the terminal wrote the
/// bracketed-paste markers with nothing between them, which is what happens when the clipboard
/// holds something with no text in it at all: a picture. The terminal has no way to send that down
/// a pty and no way to say so either, so the empty paste is the whole of the signal, and the answer
/// is to go and read the clipboard directly.
///
/// Said once per session and then not again, because a user who has been told which key carries a
/// picture does not need telling every time they use the other one.
///
/// A paste mid-turn is folded as one at rest is, for the same reason: a stack trace pasted mid-turn
/// would otherwise push the reply being read off the screen, and mid-turn is exactly when there is
/// a reply worth reading. Every working loop reaches this through [`pump_terminal`] rather than
/// binding a paste arm inline, because writing the same arm in each is how they came to disagree.
/// Mid-turn is when the delegate view is open, since a delegate only runs inside a turn, so a paste
/// while watching reaches this handler. Mid-turn, what comes back is what [`act_while_working`]
/// carries out, which is the clipboard read an empty paste asks for and nothing else.
///
/// A drop is recognised mid-turn too, since a drop reaches the terminal as a paste of the path and
/// a running turn is when people drop a file: they are watching a reply and want to hand over the
/// screenshot it is about. Without this the path was written out as prose, so the line said
/// nothing about a file and the attachment was never staged.
///
/// A wiped handoff and a wiped enhancement are dropped first, at rest and mid-turn alike: a turn
/// can begin with the brief still in the box.
pub fn handle_paste(session: &mut Session, text: &str) -> Action {
    session.drop_a_wiped_handoff();
    session.drop_a_wiped_enhancement();
    // The line is the scroller's to leave alone, and the delegate view's, the picker's, the prompt
    // search's and the menu's: a paste is not a key, so it arrives from the terminal whatever mode is open
    // and the guard that holds the box still for a keystroke never sees it. Nothing is said about
    // it either way: the scroller and the view have the whole screen but their footer, and a
    // sentence drawn nowhere is no answer. Before the empty case as much as the rest, because that
    // one spends a hint said once a session and answers with the clipboard read that would put a
    // picture in a box nobody can see.
    if session.scrolling()
        || session.watching().is_some()
        || session.modal()
        || session.files_open()
        || session.peeking()
    {
        return Action::None;
    }
    // A picture copied to the clipboard reaches a terminal as a paste of nothing at all, since the
    // terminal hands over text and there is none. Said before anything else looks at the text,
    // because an empty paste is no more a drop than it is a prompt, and left to `paste_text` it
    // would write nothing and say nothing, so the key that carries a picture would never be named.
    if text.is_empty() {
        let chord = session.bindings().paste_name();
        session.note_once(t!(paste_arrived_empty, chord = chord));
        return Action::Paste;
    }
    // A drop reaches the terminal as a paste of the path, so this is where one is recognised.
    // Anything that is not a drop is text the user pasted, which lands in the box whole or behind
    // a marker depending on how much of the screen it was about to take.
    if !session.drop_files(text) {
        session.paste_text(text);
    }
    Action::Redraw
}

/// Bring what is on the clipboard into the prompt.
///
/// Separated from the loop so it can be tested without a terminal, and taking the read as an
/// argument so a test can say what the clipboard held.
fn take_from_clipboard(session: &mut Session, pasted: crate::clipboard::Pasted) {
    use crate::clipboard::Pasted;
    use bravebot_agent::turn::MAX_PASTED_IMAGE_BYTES;

    // Whatever the answer was, it is the current one, so the hint has served its purpose. It comes
    // back at the next focus change if the picture is still there and still wanted.
    session.image_on_clipboard = false;

    match pasted {
        // A command line is not a sentence, so a marker in one names nothing and would be passed to
        // the shell as literal text. Saying so beats writing it and letting the shell complain.
        Pasted::Image(_) if session.shell => session.note(t!(paste_not_a_command)),
        Pasted::Image(image) => session.attach(image),
        // The paths are put to the drop handling as the line a paste of them would have been, so a
        // copied file is carried, read as context or written out as its path by the rules a dragged
        // one is, and in shell mode is written as words (PASTE-12). Nothing here reads the file.
        Pasted::Files(paths) => {
            if !session.drop_files(&crate::dropped::line_of(&paths)) {
                session.note(t!(paste_nothing_on_clipboard));
            }
        }
        Pasted::FilesRefused => session.note(t!(paste_files_refused)),
        Pasted::Text(text) => session.paste_text(&text),
        Pasted::TooLarge(bytes) => session.note(bravebot_i18n::sizes::paste_too_large(
            bytes as u64,
            MAX_PASTED_IMAGE_BYTES as u64,
        )),
        Pasted::Nothing => session.note(t!(paste_nothing_on_clipboard)),
    }
}

/// Draw a frame, and take back what it laid the transcript out to.
///
/// Every draw a person looks at goes through here, so nothing can redraw without the session
/// learning the shape of what was drawn. The scroller's keys are answered against those numbers,
/// and a stale set is a jump to the wrong row.
fn redraw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) -> io::Result<()> {
    // `/redraw`: the terminal may have been reset or cleared from outside, so its modes are given
    // back and asked for again, and the screen is cleared so the draw below sends every cell
    // instead of the difference from a screen that is no longer there (CMD-33).
    if std::mem::take(&mut session.retake_terminal) {
        // Only writes: nothing is read back, so this is safe while the terminal's reader runs.
        // Given back first so the keyboard flags are popped before they are pushed again.
        give_back_modes(terminal.backend_mut(), enhanced_keys())?;
        ask_for_modes(
            terminal.backend_mut(),
            enhanced_keys(),
            session.mouse_captured(),
        )?;
        terminal.clear()?;
    }
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, session))
        .map_err(io::Error::other)?;
    session.note_layout(laid);
    // Every frame, so a turn started or ended by any path reaches the title; an unchanged state
    // writes nothing.
    crate::title::mark(terminal.backend_mut(), run_state(session.status))?;
    // And the pointer's, because the border can move under a pointer at rest without a report of it.
    crate::pointer::show(terminal.backend_mut(), session.pointer_shape())?;
    // The notice has its own settings: `terminalTitle` and an incognito session do not reach it,
    // since its text is fixed and holds nothing of the session.
    let _ = crate::notify::state(terminal.backend_mut(), run_state(session.status));
    Ok(())
}

/// What the title says the session is doing, from the driver's own status.
const fn run_state(status: Status) -> crate::title::State {
    match status {
        Status::Working | Status::Running => crate::title::State::Working,
        Status::Idle | Status::Quitting => crate::title::State::Ready,
    }
}

/// What the title says while `message` is handled: a question holds the run for the person, and
/// anything else leaves the turn working.
fn title_state_during(message: &crate::remote_confirm::ToMain) -> crate::title::State {
    if refusal(message).is_some() {
        crate::title::State::Waiting
    } else {
        crate::title::State::Working
    }
}

/// Whether `message` is an approval a manifest run's step puts to the person: drawn, and holding
/// the worker until it is answered.
fn manifest_step_asks(message: &crate::remote_confirm::ToMain) -> bool {
    use crate::remote_confirm::ToMain;
    matches!(
        message,
        ToMain::Write(_)
            | ToMain::Fetch(_)
            | ToMain::Vouch(_)
            | ToMain::Exposure(_)
            | ToMain::ToolList(_)
            | ToMain::McpCall(_)
            | ToMain::Apply(_)
    )
}

/// Turn the title to waiting before a manifest run's step puts `message` to the person, when it
/// does. Returns whether it asked, to be given to [`end_manifest_step`].
fn begin_manifest_step(
    message: &crate::remote_confirm::ToMain,
    mark: impl FnOnce(crate::title::State),
) -> bool {
    let asks = manifest_step_asks(message);
    if asks {
        mark(crate::title::State::Waiting);
    }
    asks
}

/// Turn the title back to working once the approval [`begin_manifest_step`] reported has been
/// answered.
fn end_manifest_step(asks: bool, mark: impl FnOnce(crate::title::State)) {
    if asks {
        mark(crate::title::State::Working);
    }
}

/// Say in the title what the session is doing. A title that cannot be written is not a reason to
/// stop a turn or leave a question unasked.
fn mark_title(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, state: crate::title::State) {
    let _ = crate::title::mark(terminal.backend_mut(), state);
    let _ = crate::notify::state(terminal.backend_mut(), state);
}

/// The model and level the command line named for the session it opens, which outrank every file
/// and what `/model` and `/effort` recorded, and are recorded nowhere (CLI-31).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AskedFor {
    /// The model as typed, resolved against the configuration when the session opens.
    pub model: Option<String>,
    /// The level, already checked to be one.
    pub effort: Option<bravebot_aichat::protocol::Effort>,
}

/// The links the command line gave the session it opens, with `--issue` and `--pr`, each already
/// read as a link (PANEL-12). Set on the first session only, after any link its record held.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Links {
    pub issue: Option<bravebot_session::sessions::Url>,
    pub pull_request: Option<bravebot_session::sessions::Url>,
}

impl Links {
    /// Set each link the command line gave on the session's record, leaving the other as it is.
    fn give_to(self, stored: &mut bravebot_session::sessions::Handle) {
        use bravebot_session::sessions::Link;
        for (kind, url) in [
            (Link::Issue, self.issue),
            (Link::PullRequest, self.pull_request),
        ] {
            if url.is_some() {
                stored.set_link(kind, url);
            }
        }
    }
}

/// What a session begins with.
#[derive(Debug, Default)]
pub enum Start {
    /// A new session, with nothing behind it.
    #[default]
    Fresh,
    /// A new session that sends this prompt as its first turn once the questions it opens with are
    /// answered, as the person's own line, under the model and level the command line named
    /// (CLI-31).
    Sending(String, AskedFor),
    /// A new session with this text put in the box and not sent, and the sentence that says where
    /// it started where that is not where a link asked (LINK-4).
    Prefilling(String, Option<String>),
    /// A new session with the Sessions screen open over it, to pick one up (`--resume`).
    Choose,
    /// The same, opened for the sessions linked to one pull request, given by number or address.
    ChooseFromPr(String),
    /// A session read back off disk, continuing where it left off.
    Resuming(Box<bravebot_session::sessions::Record>),
}

/// How a session ended.
#[derive(Debug)]
pub enum Ended {
    /// The person left, with the session left behind where there is one to pick up again.
    Left(Option<bravebot_session::sessions::Resumable>),
    /// The person handed the session to a background process (BG-2). Its record is written, and
    /// the caller starts the process in `mode` once the terminal is handed back.
    Background {
        left: bravebot_session::sessions::Resumable,
        mode: bravebot_session::jobs::Mode,
    },
    /// The session never began, because the definition it was started under was refused, or the
    /// record to resume is held by another process, for this reason. The caller prints it after the
    /// terminal is handed back, so it stays on the screen.
    Refused(String),
    /// The person opened a task in the Sessions screen that this process cannot carry on in:
    /// a background process to join, or a record of another directory to resume, copy or begin a
    /// session in (SCREEN-8). The caller does it once the terminal is handed back.
    Open(Elsewhere),
}

/// Where the Sessions screen sent the person, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elsewhere {
    /// The directory the session works in, which the new process starts in.
    pub directory: String,
    pub how: How,
}

/// What is done in the directory of an [`Elsewhere`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum How {
    /// Join the background process running the session with this id (BG-9).
    Attach(String),
    /// Resume the record with this id, as `--resume` does.
    Resume(String),
    /// Copy the record with this id and carry on in the copy, as `--fork` does.
    Fork(String),
    /// Begin a session there.
    Fresh,
}

/// What `/resume` leaves for the loop's caller to start the next session with.
struct Switch {
    /// The record the person chose.
    record: Box<bravebot_session::sessions::Record>,
    /// The workspace as the session left it, so a session that moved with `/cd` resumes from there.
    workspace: Workspace,
    /// The sandbox mode as the session left it, so `/sandbox` lasts until the process ends and a
    /// `/resume` does not put back the mode start-up chose.
    sandbox: crate::sandbox_command::Sandboxed,
}

/// What `run` settled for one session before `event_loop` begins it, carried from `run` through
/// `event_loop` to `begin_session`. A `/resume` begins another session with a new one of these.
struct Startup {
    confinement: String,
    sockets_with_a_path_reachable: bool,
    sandbox: crate::sandbox_command::Sandboxed,
    start: Start,
    named: Option<String>,
    links: Links,
    skip_permissions: bool,
    /// Whether the command line asked for auto with `--auto` (AUTO-1).
    asked_for_auto: bool,
    prompts: bravebot_agent::turn::SystemPrompts,
}

/// Run the interface until the user leaves.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are what the cli settled before the interface starts, passed once by its single caller"
)]
pub fn run(
    config: &mut Config,
    workspace: &Workspace,
    confinement: String,
    sockets_with_a_path_reachable: bool,
    mut servers: crate::state::Servers,
    start: Start,
    agent: Option<String>,
    links: Links,
    skip_permissions: bool,
    asked_for_auto: bool,
    prompts: bravebot_agent::turn::SystemPrompts,
    moments: &mut bravebot_agent::hooks::SessionMoments,
) -> io::Result<Ended> {
    // First, because the questions the terminal is asked on the way in read from it, and a terminal
    // that has gone is where they would spin.
    crate::hangup::end_with_the_terminal();
    // Before the terminal is taken, because the request for no colour decides whether it is asked
    // about its background on the way in, and that question happens inside the takeover.
    crate::theme::sense_no_color();
    crate::indicator::sense_no_motion();
    crate::blocks::sense();

    // Read before the takeover, which is where capture is asked for, and again with the rest of the
    // settings once the session exists: the file is small and the session is built after the
    // terminal is taken.
    let early = bravebot_config::Settings::load();
    let mouse = early.tui_mouse() != Some(false);
    let mut stdout = io::stdout();
    take_over_terminal(&mut stdout, mouse)
        .inspect_err(|_| crate::hangup::end_if_the_terminal_hung_up())?;
    // After the takeover, which has finished asking about the background, and before the first
    // event is read, so the reply to this question cannot be taken for a keystroke.
    crate::preview::sense(terminal_answers_queries());

    // Handed back on the way out, because the terminal is already taken by the line above and a
    // failure here would otherwise return from a session that never started, leaving the person in
    // raw mode on a screen they cannot get off.
    let mut terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => terminal,
        Err(failure) => {
            hand_back_terminal(&mut io::stdout())
                .inspect_err(|_| crate::hangup::end_if_the_terminal_hung_up())?;
            return Err(failure);
        }
    };

    let result = match start {
        // The Sessions screen refuses Enter on one of these, and `--resume` refuses it too. If a
        // record still arrives here, loading its empty conversation as a turn would continue a run
        // that cannot be continued.
        Start::Resuming(record) if record.manifest.is_some() => Ok(Ended::Left(None)),
        // `--resume` and `--from-pr` open the Sessions screen over an ordinary new session, once it
        // has begun, so leaving the screen is an ordinary session rather than nothing.
        mut start => {
            // A `/resume` ends the loop with the record it chose, and the next one begins from it
            // exactly as a session started with `--resume` does, so what comes back is what that
            // flag restores and nothing a second path decided. The servers outlive the switch:
            // they were started for this process, not for one conversation.
            let mut workspace = workspace.clone();
            // Only the first session is the one the command line named a definition for. A
            // session `/resume` switches to works under the definition its own record names.
            let mut named = agent;
            // Likewise: a link the command line gave belongs to the session it opened.
            let mut links = links;
            // What start-up chose, until `/sandbox` moves it. Carried across a `/resume` so the
            // command lasts until the process ends.
            let mut sandbox = crate::sandbox_command::Sandboxed::at_start();
            loop {
                let mut switch = None;
                let ended = event_loop(
                    &mut terminal,
                    config,
                    &workspace,
                    &mut servers,
                    Startup {
                        confinement: confinement.clone(),
                        sockets_with_a_path_reachable,
                        sandbox: sandbox.clone(),
                        start,
                        named: named.take(),
                        links: std::mem::take(&mut links),
                        skip_permissions,
                        asked_for_auto,
                        prompts: prompts.clone(),
                    },
                    moments,
                    &mut switch,
                );
                match (ended, switch) {
                    (Ok(_), Some(next)) => {
                        workspace = next.workspace;
                        sandbox = next.sandbox;
                        start = Start::Resuming(next.record);
                    }
                    (ended, _) => break ended,
                }
            }
        }
    };

    // Restore the terminal even if the loop failed: leaving a user in raw mode on an
    // alternate screen is worse than the original error.
    finish_run(
        &mut terminal,
        result,
        hand_back_terminal,
        crate::hangup::end_if_the_terminal_hung_up,
    )
}

/// End [`run`] with `result`, the way the loop ended, after the terminal has been handed back whatever
/// that was (INPUT-33).
///
/// A session that ends by failing hands the terminal back as one that is left does, since a terminal
/// left in raw mode on a screen that is not its own is worse for the person than the error that put
/// it there. Where the terminal cannot be handed back either, that failure is the one returned and the
/// session's own is dropped: it is the one that says the terminal is still held, and `hung_up` has
/// been asked about it.
///
/// Split from `run`, which needs a real terminal, so that a test can end a run by failing.
fn finish_run<W: Write, T>(
    terminal: &mut Terminal<CrosstermBackend<W>>,
    result: io::Result<T>,
    hand_back: impl FnOnce(&mut CrosstermBackend<W>) -> io::Result<()>,
    hung_up: impl FnOnce(),
) -> io::Result<T> {
    end_the_run(terminal, hand_back, hung_up)?;
    result
}

/// Give the terminal back for good at the end of [`run`], cursor and all. However the run ended, it
/// has left its herdr pane: a `/resume` stays inside the loop, and a process started on the way out
/// reports for itself.
///
/// Split from `run` so a test can reach it, since that function needs a real terminal, and told how
/// to hand the terminal back so that a test does not ask the terminal the test binary runs in about
/// its keyboard modes or take it out of raw mode. A process outside the terminal's foreground group
/// is stopped by the terminal at either, and a test that is stopped never finishes.
///
/// `hung_up` is called where the terminal cannot be handed back, while `terminal` is still alive.
/// A terminal whose other end has closed fails here, and the session then ends with the status of a
/// hangup (INPUT-51) instead of dropping `terminal`. The drop shows the cursor again, and on a
/// closed terminal that fails and is reported with `eprintln!`, which panics.
fn end_the_run<W: Write>(
    terminal: &mut Terminal<CrosstermBackend<W>>,
    hand_back: impl FnOnce(&mut CrosstermBackend<W>) -> io::Result<()>,
    hung_up: impl FnOnce(),
) -> io::Result<()> {
    leave(terminal.backend_mut(), hand_back)
        .and_then(|()| terminal.show_cursor())
        .inspect_err(|_| hung_up())
}

/// Sign in to the backend the next request will use, where it needs one and has none.
///
/// The URL and the code go into the transcript, where the person is already looking. The interface
/// keeps the screen throughout: handing it over instead put the one thing somebody has to read and
/// type underneath a display that was about to be redrawn, and left them in a terminal that no longer
/// looked like the program they were using.
///
/// Off-thread for the reason a turn is: the sign-in waits for a browser to be visited, which is as
/// long as the person takes, and run here it would freeze the interface for the whole of it.
///
/// Nothing happens in the common case. A good session is not a sign-in, a build with no AWS
/// configuration cannot want one, and a model served by Brave never needs one whatever else is
/// configured, so the question is asked of the model about to answer rather than of what exists.
fn sign_in_if_needed(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
) -> io::Result<()> {
    let model = model_in_force(session.model(), config).to_string();
    if !bravebot_agent::backend::Backend::needs_sign_in(config, &model) {
        return Ok(());
    }

    session.note(t!(session_signing_in));

    let (lines, arriving) = mpsc::channel::<String>();
    let worker_config = config.clone();
    let worker = thread::spawn(move || {
        bravebot_agent::backend::Backend::sign_in_if_needed(&worker_config, &model, |line| {
            // A closed channel is an interface that has stopped listening, and there is nothing to
            // be done about it from here: the sign-in is already running.
            let _ = lines.send(line);
        })
        .map_err(|failure| failure.to_string())
    });

    // Drawn as they arrive rather than collected, because a code is only useful while the command
    // that printed it is still waiting.
    loop {
        redraw(terminal, session)?;

        match arriving.recv_timeout(FRAME) {
            Ok(line) => session.note(line),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    // Said rather than swallowed, and not fatal: the turn goes ahead and fails with the backend's
    // own account of what is wrong, which is more use than this function's guess at it.
    if let Ok(Err(failure)) = worker.join() {
        session.note(failure);
    }
    redraw(terminal, session)?;
    Ok(())
}

/// The session left behind, for telling somebody how to pick it up again.
///
/// Nothing for a session that was opened and left without sending anything: there is no record to
/// resume, and naming one would be offering a command that answers "no session by that name".
///
/// Where it is, and not only what it is called: `/cd` moves the record, and the shell this is
/// eventually printed into did not move with it.
fn left_behind(stored: &bravebot_session::sessions::Handle) -> Ended {
    Ended::Left(stored.to_resume())
}

/// The first option this session was started with that a background process does not read, set
/// once by the command line before the interface opens.
static BACKGROUND_LOSES: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Record an option the session was started with that `/bg` could not pass on. Only the first one
/// is kept, since one is enough to refuse.
pub fn background_cannot_carry(flag: &str) {
    let _ = BACKGROUND_LOSES.set(flag.to_string());
}

/// How `/bg` ends the session, or the reason it cannot (BG-2, BG-8).
///
/// Everything refused here is something the background process would not carry: a mode that answers
/// every question, an option given on the command line, which that process never receives, a
/// sandbox mode `/sandbox` moved away from `settled`, the one start-up chose and the background
/// process chooses again, a loop, a goal or a watch, which the process has no way to run yet, and a
/// session with no record, which is all it resumes from.
fn handed_to_the_background(
    session: &Session,
    resumable: Option<bravebot_session::sessions::Resumable>,
    lost: Option<&str>,
    settled: bravebot_sandbox::SandboxMode,
    incognito: bool,
) -> Result<Ended, String> {
    use bravebot_session::jobs::Refusal;
    let decided = bravebot_session::jobs::refusal(&bravebot_session::jobs::Handing {
        permission_mode: session.permission_mode(),
        sandbox: session.sandbox_mode(),
        settled,
        // `/bg` waits for the turn before it is decided, so nothing is running here.
        busy: false,
        keeps_running: session.looping().is_some()
            || session.goal().is_some()
            || !session.watches().is_empty(),
        recorded: resumable.is_some(),
        incognito,
    });
    // The terminal's own check, which the window has no counterpart of, said where `/bg` has
    // always said it: after the mode and before the rest.
    if let Some(flag) = lost
        && !decided.is_err_and(Refusal::precedes_the_options)
    {
        return Err(t!(session_bg_option_lost, flag = flag));
    }
    let mode = decided.map_err(|refusal| match refusal {
        Refusal::Unsupported => t!(bg_unsupported).to_string(),
        Refusal::Auto => t!(session_bg_auto).to_string(),
        Refusal::Bypass => t!(session_bg_bypass).to_string(),
        Refusal::SandboxMoved { mode, settled } => t!(
            session_bg_sandbox_moved,
            mode = mode.name(),
            settled = settled.name()
        ),
        Refusal::Busy | Refusal::KeepsRunning => t!(session_bg_keeps_nothing_running).to_string(),
        Refusal::Incognito => t!(session_bg_incognito).to_string(),
        Refusal::NothingRecorded => t!(session_bg_nothing_recorded).to_string(),
    })?;
    let Some(left) = resumable else {
        return Err(t!(session_bg_nothing_recorded).to_string());
    };
    Ok(Ended::Background { left, mode })
}

/// The directory this session writes what is not part of the project into, made and reachable.
///
/// Both at once, because a directory nothing may write in is not worth creating, and reach left
/// pointing at the directory a previous session was given names one that has been removed.
///
/// A session that cannot be given one carries on without it. A temporary directory that is full,
/// read-only or missing is a reason to have nowhere to put an intermediate file, and not a reason
/// to refuse somebody the session they asked for.
///
/// Said out loud when that happens, because the alternative is a turn told it has nowhere to write
/// with nothing on the screen to say why.
fn opened_scratch(session: &mut Session, workspace: &mut Workspace) -> Option<SessionScratch> {
    let scratch = match SessionScratch::create() {
        Ok(scratch) => Some(scratch),
        Err(problem) => {
            // `problem` is the operating system's error and holds no path or other text a checkout
            // chose (`SessionScratch::create`), so it is not drawn; the one-shot run's copy of this
            // sentence follows the same reasoning (LAYER-3).
            session.note(t!(
                session_scratch_unavailable,
                problem = problem.to_string()
            ));
            None
        }
    };
    workspace.open_scratch(scratch.as_ref().and_then(SessionScratch::directory));
    bravebot_session::sessions::sweep_unlisted_checkouts(workspace);
    scratch
}

/// What asking for the model picker does: the picker, or the note that the definition in force
/// has already named the model. Shared by `/model`, `/model session` and a click on the panel's
/// model row, which differ only in the scope the picker opens in.
pub(crate) fn ask_which_model(session: &mut Session, scope: crate::model_prompt::Scope) -> Action {
    // Every turn asks for the definition's model, so a pick would change what `/status` says
    // and not what any turn asks for (CLI-17).
    if let Some(refused) = session.standing_definition().and_then(|standing| {
        let model = standing.model.as_ref()?;
        Some(t!(
            session_model_is_the_definitions,
            definition = &standing.name,
            model = model
        ))
    }) {
        session.note(refused);
        // A model a prompt file named has nowhere to open, and is not kept for a later picker.
        session.take_model_to_offer();
        return Action::Redraw;
    }
    Action::ChooseModel(scope)
}

/// What the idle loop does with one event it has read.
fn idle_action(session: &mut Session, taken: TermEvent) -> Action {
    match taken {
        // Presses only. Asking for disambiguated keys asks for releases as well, and a
        // release handled as a press types every character twice.
        TermEvent::Key(key) if key.kind == KeyEventKind::Release => Action::None,
        TermEvent::Key(key) => handle_key(session, key),
        TermEvent::Mouse(mouse) => handle_mouse(session, mouse),
        TermEvent::Paste(text) => handle_paste(session, &text),
        // Coming back from copying something is the moment a picture appears on the
        // clipboard, and the cheapest moment to notice: once per switch away and back,
        // rather than a clipboard tool spawned on a timer for the whole life of the
        // session.
        TermEvent::FocusGained => {
            session.away.focus(true);
            session.image_on_clipboard = crate::clipboard::holds_an_image();
            Action::Redraw
        }
        TermEvent::FocusLost => {
            session.away.focus(false);
            Action::None
        }
        // Nothing else draws while the box is idle, so a frame laid out for the old size
        // would stay until the next key (INPUT-41).
        TermEvent::Resize(..) => Action::Redraw,
    }
}

/// What the settings say a run of anything may spend or do, read once where the settings are and
/// kept beside them. A turn and a manifest run build the same base [`Task`] out of it, so the two
/// cannot drift apart on which layer named a cap.
struct RunSettings {
    attribution: Attribution,
    /// What a command's output may spend of the conversation, where the settings said anything.
    output_cap: Option<usize>,
    /// What one download may write to a file.
    download_cap: Option<usize>,
    /// How long a command may run, resolved where the settings were read (RUN-23).
    deadlines: bravebot_agent::exec::Deadlines,
}

impl RunSettings {
    fn from_settings(settings: &bravebot_config::Settings) -> Self {
        Self {
            attribution: settings.attribution().clone(),
            output_cap: settings.run_output_cap(),
            download_cap: settings.download_max_bytes(),
            deadlines: bravebot_agent::exec::Deadlines::from_settings(settings),
        }
    }

    /// The task a turn or a manifest run starts from.
    fn task(&self, prompt: &str, session: &Session, permissions: &Permissions) -> Task {
        Task::new(prompt)
            .with_home(bravebot_agent::home::directory())
            .with_profile(bravebot_agent::home::profile())
            .with_cache(bravebot_agent::home::cache())
            .with_model(session.model().map(str::to_string))
            .with_effort(session.effort_in_force())
            .with_variant(session.variant().map(str::to_string))
            .in_account_session(session.account_session().clone())
            .with_permissions(permissions.clone())
            .with_permission_mode(session.live_permission_mode())
            .with_attribution(self.attribution.clone())
            .with_output_cap(self.output_cap)
            .with_download_cap(self.download_cap)
            .with_deadlines(self.deadlines)
            // The planner is told it is talking to a person at this interface (INSTR-9).
            .in_front_end(bravebot_agent::FrontEnd::Terminal)
            .with_confined_runs(true)
            .asking_about_unconfined(true)
            .with_sandbox_mode(session.live_sandbox_mode())
    }
}

/// The locals `event_loop` keeps from its startup. `event_loop` binds them in the order of these
/// fields, which is the order the startup creates them, so they drop in the reverse of it.
struct Begun {
    workspace: Workspace,
    settings: bravebot_config::Settings,
    run_settings: RunSettings,
    session: Session,
    conversation: Conversation,
    stored: bravebot_session::sessions::Handle,
    scratch: Option<SessionScratch>,
    answers: Answers,
}

/// Bring a session up to what a resumed record left: the transcript and the notes that caveat it,
/// the accounts, the vouched programs, the directories opened with `/add-dir`, the checkouts and
/// the rewind points. Returns the conversation, the record's handle and the programs vouched for.
///
/// Nothing here needs a terminal. The trust question follows it in `begin_session`, because the
/// map it asks about comes from the record and the transcript is shown before anything is asked.
///
/// `Err` is a record another process holds, refused before the session shows or restores anything
/// (SESSION-38).
fn restore_resumed(
    session: &mut Session,
    config: &Config,
    workspace: &mut Workspace,
    record: &bravebot_session::sessions::Record,
) -> Result<
    (
        Conversation,
        bravebot_session::sessions::Handle,
        TrustedPrograms,
    ),
    bravebot_session::sessions::Held,
> {
    let mut handle = bravebot_session::sessions::Handle::resuming(
        workspace.root(),
        record,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    )?;
    let conversation = Conversation::restored(record.conversation.clone());
    // Shown before anything else, because a session that silently continues something
    // the user cannot see is one they will contradict without meaning to. The trail comes
    // out of the audit beside the record, so Ctrl-T answers for the whole session rather
    // than only for the turns this process ran.
    let recalled = bravebot_session::sessions::recall(workspace.root(), record);
    session.replay(&conversation, &record.title, &recalled);
    session.restore_accounts(record);
    session.restore_references(handle.references());
    if let Some(name) = record.skill_mode.as_deref() {
        session.resume_skill_mode(name);
    }
    if conversation.last_request_tokens() > 0 {
        session.measured(
            conversation.last_request_tokens(),
            config.context_budget,
            config.budget_is_guessed(),
        );
    }
    // Said after the transcript, so it reads as a caveat on what was just shown: the work
    // it describes may not be in the tree the user is now looking at.
    if let Some(note) = bravebot_session::sessions::branch_note(
        record.branch.as_deref(),
        bravebot_session::sessions::branch_of(workspace.root()).as_deref(),
    ) {
        session.note(note);
    }
    // The same caveat about the other half of what produced that transcript: not the
    // tree it ran against, but the code that ran.
    if let Some(note) =
        bravebot_session::sessions::build_note(record.build.as_deref(), bravebot_stamp::BUILD)
    {
        session.note(note);
    }
    // And the third of them: which surface drew the transcript above. The other one
    // renders the same session differently, so a detail being read as a symptom of the
    // work may be a symptom of the program that showed it.
    if let Some(note) = bravebot_session::sessions::front_note(
        record.front.as_deref(),
        bravebot_session::sessions::Front::Terminal,
    ) {
        session.note(note);
    }
    // The programs go the way the map does and for the same reason: the person resuming
    // is the person who vouched for them. Unlike the map there is nothing to ask about an
    // absent list, since an empty one simply means every run asks.
    let vouched = record.trusted_programs(workspace.root());
    // The other half of `/add-dir`, which the map cannot carry: a directory has to be
    // open for an absolute path in it to resolve at all. Restored here so the rule and
    // the reach come back together, rather than the rule alone.
    let already_open = workspace.added_directories();
    for note in record.reopen_added_directories(workspace) {
        session.note(note);
    }
    // Counted for the settings layered from here on, as `/add-dir` counts one, so the file
    // `--settings` named is judged against the directories this resume made writable (#2599).
    // Only the directories this call opened are counted: the others were counted when opened.
    for reopened in workspace.added_directories() {
        if !already_open.contains(&reopened) {
            bravebot_config::add_writable_directory(reopened);
        }
    }
    // Beside them, the checkouts the session's delegates were given: their rules came
    // back with the map, and the list and the candidate paths come back here (CHECKOUT-16).
    if let Some(note) = record.restore_checkouts(workspace, &mut handle) {
        session.note(note);
    }
    // The names the file tools had reached, so the nested instructions and path-scoped rules
    // they loaded are in force again from the first turn (INSTR-14, INSTR-16).
    workspace.restore_touched(&record.touched);
    // After the transcript rather than before it, because each point's place in that
    // transcript is worked out from it: a point is a turn number in the record and an
    // index in the session, and the list it indexes has to exist first.
    session.restore_rewind(record, workspace, &conversation);
    // Last, once the checkouts, the plan and the record's work are back: what the session left
    // open, worked out again from the record and the files on disk (OPEN-6).
    let written = record
        .written
        .iter()
        .filter_map(bravebot_session::sessions::StoredPath::to_path)
        .collect();
    session.resume_open_loops(record.work, written, workspace);
    Ok((conversation, handle, vouched))
}

/// Put into force the settings a session reads once, before any turn. `asked_effort` is the level
/// the command line named, which outranks the one the settings and the record settle.
fn adopt_settings(
    session: &mut Session,
    settings: &bravebot_config::Settings,
    asked_effort: Option<bravebot_aichat::protocol::Effort>,
) {
    // Once the settings are read and before the first question the session can ask, so herdr
    // knows bravebot holds the pane from the start rather than from the first frame (HERDR-8).
    crate::herdr::state(crate::title::State::Ready);
    // Settled before a key can be pressed, since this is what decides whether a letter is a letter.
    session.adopt_editing(settings.editor_mode());
    // And the level a turn asks for, which the record or this file answers, whichever BACKEND-43
    // ranks higher. Read once beside the rest: a file edited mid-session describes the next one,
    // and `/effort` is how this one is changed.
    session.adopt_effort(settings);
    if let Some(level) = asked_effort {
        session.name_effort(level);
        say_if_unread(session);
    }
    // After the level is settled, which is the level the ChatGPT account is sent for the session.
    session.begin_account_session();
    session.adopt_limit(settings);
    // Settled here too, and once, for the reason the mode is read once per turn: what decides
    // whether somebody is asked must not change under a prompt already on the screen. The command
    // line's switch is read here and nowhere else in the interface.
    session.adopt_vetting(bravebot_core::vetting::asked_for(), settings.auto_vetting());
    session.adopt_keybindings(settings.keybindings());
    say_undeliverable_chords(session);
    session.adopt_sessions_keys(settings.sessions_keys());
    // And how far the wheel moves the view, for the reason the chords are read here: a figure read
    // once at startup describes the session, and a terminal's own idea of a notch does not change
    // under it.
    session.adopt_wheel_rows(settings.wheel_rows());
    session.adopt_mouse(settings.tui_mouse());
    session.adopt_context_menu(settings.tui_context_menu());
    session.adopt_panel();
    session.adopt_caffeinate();
    session.away.adopt(settings.away_summary_enabled());
    session
        .away
        .adopt_idle(settings.away_summary_idle_seconds());
    session.adopt_ask_idle(settings.ask_idle_seconds());
    // A background job belongs to the session here and ends with it, except where nothing may
    // outlive the process that wrote it (RUN-15).
    if !bravebot_core::incognito::engaged() {
        session.keep_jobs_between_turns();
    }
    session.adopt_prompt_files(crate::prompt_files::directory());
    session.adopt_suggestions(settings.suggestions() == Some(true));
}

/// Say which of the chords a settings file asked for this terminal cannot send (INPUT-32).
///
/// The action stayed on its default, so the key the person wrote does nothing and `?` names the
/// default rather than it. Nothing else on the screen would show that, and the press they expect to
/// work is Escape on a terminal that reports it with no modifier at all, which clears the line. One
/// sentence with a count behind it where more than one was refused, as the Sessions screen says its
/// own refused keys ([SCREEN-14](../../docs/specs/sessions-screen.md)).
fn say_undeliverable_chords(session: &mut Session) {
    let chords = session.undeliverable_chords();
    let Some(first) = chords.first() else {
        return;
    };
    let mut said = first.sentence();
    if chords.len() > 1 {
        let more = chords.len() - 1;
        said = format!(
            "{said}; {}",
            t!(keybindings_undeliverable_more, count = more)
        );
    }
    session.note(said);
}

/// Name the session's id where it is handed on, on every pass of the loop: to the panel, and to
/// herdr, which reopens the session by it. Any path that changes the session reaches both on the
/// next pass, once the record `--resume` reads is on disk (HERDR-9). Returns whether the panel
/// changed.
fn name_the_id(session: &mut Session, stored: &bravebot_session::sessions::Handle) -> bool {
    let changed = session.name_id(stored.resumable());
    crate::herdr::resume(stored.resumable());
    changed
}

/// Put what the command line asked for on a session whose record is open and every startup
/// question is answered: the links `--issue` and `--pr` gave, the prompt `-p` handed over, the text
/// a link put in the box, and the Sessions screen `--resume` opens.
///
/// Split from `begin_session` so a test can reach it, since that function needs a real terminal for
/// the questions that come before this.
fn apply_the_command_line(
    session: &mut Session,
    stored: &mut bravebot_session::sessions::Handle,
    root: &std::path::Path,
    links: Links,
    first_turn: Option<String>,
    prefill: Option<(String, Option<String>)>,
    choosing: Option<Option<String>>,
) {
    // After the record is open, so a link the command line gave replaces one a resumed record held,
    // as `/issue` and `/pr` would (PANEL-12).
    // What `--issue` and `--pr` gave is the person's own text, read as a typed line is (PANEL-16).
    for given in [&links.issue, &links.pull_request].into_iter().flatten() {
        session.note_typed(given.as_str());
    }
    links.give_to(stored);

    // Held rather than sent: the loop's first pass sends it, and the loop runs only once the
    // startup questions are answered.
    if let Some(prompt) = first_turn {
        session.hand_over_from_the_command_line(&prompt);
    }

    // Put in the box after every question is answered, and never handed over: Enter is the person's
    // to press (LINK-4).
    if let Some((text, note)) = prefill {
        if let Some(note) = note {
            session.note(note);
        }
        session.prefill_from_link(&text);
    }

    if let Some(from_pr) = choosing {
        sessions_screen::open_sessions_screen(session, stored.id(), root, from_pr.as_deref(), true);
    }
}

/// Say, before the first prompt can be typed, that `lost` bytes typed while the terminal was asked
/// about itself were not kept, where any were: a person who lost some of what they typed has to be
/// told before they send what is left (INPUT-52).
fn note_startup_input_lost(session: &mut Session, lost: usize) {
    if lost > 0 {
        session.note(t!(session_startup_input_lost));
    }
}

/// What `run` settled for the session itself, apart from what it starts with: how it is confined,
/// the sandbox, whether the permission rungs start in bypass, and the words of its turns.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts the session was started with, each read on its own"
)]
struct SessionBasis {
    confinement: String,
    sockets_with_a_path_reachable: bool,
    sandbox: crate::sandbox_command::Sandboxed,
    skip_permissions: bool,
    asked_for_auto: bool,
    /// Whether a `bravebot://open` link started the session, which then never reads
    /// `permissions.startInAuto` (LINK-2).
    from_link: bool,
    prompts: bravebot_agent::turn::SystemPrompts,
}

/// Settle the mode a new session opens in and where the key can reach from there (MODE-1, MODE-5,
/// AUTO-1).
///
/// A mode the line asked for decides, and the setting is read only where the line asked for none:
/// `--dangerously-skip-permissions` opens in bypass, which is put back to asking where a settings
/// layer took bypass off the ladder, and `--auto` opens in auto. Without either, a session opens in
/// auto where the settings in force say `permissions.startInAuto`, which only the person's own file
/// and the file `--settings` names can say (see `bravebot_config::Settings::start_in_auto`), and
/// asking everywhere else. Every way into the terminal client opens here, a resumed, continued or
/// forked session included, since a record keeps no mode (MODE-10). A session a
/// `bravebot://open` link started does not read the setting, since the link comes from outside the
/// person's own typing and keeps asking (LINK-2). No environment variable that selects a mode is
/// read.
fn open_in_the_mode_asked_for(
    session: Session,
    skip_permissions: bool,
    asked_for_auto: bool,
    from_link: bool,
    settings: &bravebot_config::Settings,
) -> Session {
    let setting = !from_link && settings.start_in_auto();
    let mut session = match (skip_permissions, asked_for_auto || setting) {
        // The flag opens the session in bypass. The rung is on the ladder the key walks either
        // way, unless a settings layer took it off (MODE-5).
        (true, _) => session.starting_in_bypass(),
        (false, true) => session.starting_in_auto(match asked_for_auto {
            true => bravebot_agent::ModeCause::Flag,
            false => bravebot_agent::ModeCause::Setting,
        }),
        (false, false) => session,
    };
    if bypass_is_unreachable(settings) {
        session.make_bypass_unreachable();
    }
    session
}

/// The session `begin_session` opens on, built and configured from the settings and what the
/// command line named, before any question is put to the person.
fn new_session(
    config: &mut Config,
    settings: &bravebot_config::Settings,
    workspace: &Workspace,
    asked: &AskedFor,
    servers: crate::state::Servers,
    basis: SessionBasis,
) -> Session {
    let SessionBasis {
        confinement,
        sockets_with_a_path_reachable,
        sandbox,
        skip_permissions,
        asked_for_auto,
        from_link,
        prompts,
    } = basis;
    // The one place persistence is turned on: history in ~/.bravebot outlives the session.
    let mut session = Session::new(confinement)
        .with_sockets_with_a_path_reachable(sockets_with_a_path_reachable)
        .with_sandbox(sandbox)
        .with_stored_history()
        .in_workspace(workspace.root())
        .on_tier(config)
        .with_model_labels(config);
    // The saved pick where a checkout's settings do not outrank it (BACKEND-11), before the window
    // below is asked for, which is the window of whichever model this settles on. Not read at all
    // where the command line named a model, so a pick set aside is not reported for a model that
    // was never going to be asked for.
    match &asked.model {
        Some(name) => session.name_model(config.model_named(name)),
        None => session.adopt_model(settings, config),
    }
    session.adopt_workspace(workspace);
    session.adopt_repository(crate::issue_link::Repository::of(workspace.root()));
    session.servers = servers;
    // Windows reports modifiers on every key without being asked, and crossterm says it cannot be
    // asked there, so the question only settles it elsewhere.
    session.ctrl_enter_arrives = cfg!(windows) || enhanced_keys();
    session = open_in_the_mode_asked_for(
        session,
        skip_permissions,
        asked_for_auto,
        from_link,
        settings,
    );
    // Before the first turn, and kept until the session ends: a resumed record stores none, so
    // these are the words of the turns this process sends (CLI-19).
    session = session.with_system_prompts(prompts);

    // The model outlived the session that chose it, so the window that came with it has to be asked
    // for again: it is reported by the listing and nowhere else, and nothing on disk remembers it.
    // Asked on a thread of its own, so what the session opens on does not wait for a service: the
    // loop adopts the answer on the pass it arrives, and a turn waits for it (BACKEND-64).
    ask_for_the_budget_of_the_model_in_force(&mut session, config);
    session
}

/// What `begin_session` reads off `start` before the match that consumes it.
struct StartFacts {
    /// The name the record carries, which is the person's own earlier `--agent`.
    recorded: Option<String>,
    /// The prompt the session sends as its first turn.
    first_turn: Option<String>,
    /// The text put in the box and not sent, with the sentence that says where it started.
    prefill: Option<(String, Option<String>)>,
    /// Whether the Sessions screen opens over the new session, and for which pull request.
    choosing: Option<Option<String>>,
}

impl StartFacts {
    fn of(start: &Start) -> Self {
        StartFacts {
            recorded: match start {
                Start::Resuming(record) => record.agent.clone(),
                _ => None,
            },
            first_turn: match start {
                Start::Sending(prompt, _) => Some(prompt.clone()),
                _ => None,
            },
            prefill: match start {
                Start::Prefilling(text, note) => Some((text.clone(), note.clone())),
                _ => None,
            },
            choosing: match start {
                Start::Choose => Some(None),
                Start::ChooseFromPr(wanted) => Some(Some(wanted.clone())),
                _ => None,
            },
        }
    }
}

/// The keys and managed values the readers of `settings` and of the machine's managed layer dropped
/// or combined, each said once as a note in the words `doctor` uses (BACKEND-36, RETRIEVE-23). Each
/// reads to whoever wrote it as a restriction in force, and the file is otherwise applied with
/// nothing said. The managed layer is the one `bravebot_agent::dropped_keys` reads itself.
fn say_dropped_keys(session: &mut Session, settings: &bravebot_config::Settings) {
    for said in bravebot_agent::dropped_keys::at_session_start_of_this_machine(
        settings,
        bravebot_agent::dropped_keys::Spelling::Drawn,
    ) {
        session.note(said);
    }
}

/// The notes said once the trust answer is in: a newer release, the crash report notice, why a
/// server the checkout asked for is absent, and the keys of the settings that nothing read.
fn say_startup_notes(
    session: &mut Session,
    config: &Config,
    settings: &bravebot_config::Settings,
    absent: Vec<String>,
) {
    // After the trust answer, because that question is the first thing on the screen and an aside
    // about a newer release does not come before it. Nothing is fetched here: the line is read off
    // what an earlier launch wrote down, and the ask that answers the next launch runs behind the
    // session rather than in front of it.
    if let Some(newer) = bravebot_update::at_startup(settings.update_check()) {
        session.note(newer);
    }
    // After the line about a release for the same reason, and said once: a build that sends crash
    // reports tells the person on the first launch that does, with the two ways of stopping it.
    if let Some(notice) = crate::crash_notice::at_startup() {
        session.note(notice);
    }
    // Said once, and before the first usage ping goes, which waits on the record this leaves.
    if let Some(notice) = bravebot_session::store::take_usage_ping_notice() {
        session.note(notice);
    }
    bravebot_usage_ping::at_startup(
        bravebot_usage_ping::Surface::Tui,
        bravebot_usage_ping::backend(config, session.model()),
        bravebot_usage_ping::premium(config),
    );
    // Why a server this checkout asked for is not in the session, said where the person starts
    // reading rather than on the plain terminal the screen is about to cover.
    for note in absent {
        session.note(note);
    }
    say_dropped_keys(session, settings);
}

/// Put the directories the settings name to the person or open them, by how the trust map was
/// settled, and open and vouch for the ones accepted.
///
/// `Err` is the person leaving at the question.
fn open_requested_directories(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    workspace: &mut Workspace,
    trust: &mut TrustStore,
    whence: Whence,
    settings: &bravebot_config::Settings,
    stored: &bravebot_session::sessions::Handle,
) -> Result<(), Ended> {
    // After the startup question, and put rather than applied: naming a directory in a settings
    // file asks for it instead of granting it, so one the person accepts is opened on the same
    // terms as one typed at `/add-dir`, and one they decline is not opened at all.
    let requested = named_directories(
        whence,
        bravebot_agent::permissions::additional_directories(settings),
    );
    let opening = match requested {
        Named::Opening(names) => named_to_open(session, workspace, &names),
        Named::Asking(names) => {
            let paths = named_to_open(session, workspace, &names);
            // Carried for the reason the working directory's own question carries it.
            let mut carried = String::new();
            let accepted = match crate::trust_prompt::ask_named(terminal, &paths, &mut carried) {
                Some(accepted) => accepted,
                None => return Err(left_behind(stored)),
            };
            if !carried.is_empty() {
                session.paste_text(&carried);
            }
            accepted
        }
    };
    open_named(session, workspace, trust, &opening);
    Ok(())
}

/// Everything `event_loop` settles before its first pass: the session, a resumed record, the
/// trust question, the scratch directory, the rules, the named directories and an addressed
/// definition, in that order.
///
/// `Err` is a session that ended before it began: the person left at one of the questions, the
/// definition it was started under was refused, or the record to resume is held by another process.
fn begin_session(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: &mut Config,
    workspace: &Workspace,
    servers: &mut crate::state::Servers,
    startup: Startup,
) -> Result<Begun, Ended> {
    let Startup {
        confinement,
        sockets_with_a_path_reachable,
        sandbox,
        start,
        named,
        links,
        skip_permissions,
        asked_for_auto,
        prompts,
    } = startup;
    let basis = SessionBasis {
        confinement,
        sockets_with_a_path_reachable,
        sandbox,
        skip_permissions,
        asked_for_auto,
        from_link: matches!(start, Start::Prefilling(..)),
        prompts,
    };
    // Owned rather than borrowed, because `/add-dir` opens another directory partway through and
    // the turns after it must see one. The primary root never changes, so nothing keyed on it
    // (the session record, where AGENTS.md is looked for) moves underneath.
    let mut workspace = workspace.clone();

    // The rules the user wrote in advance, read once for the workspace: a person editing the file
    // mid-session is describing the next one, and rules that changed halfway through a turn would
    // be the harder thing to explain. Every turn below is given these, until `/cd` or `/clear`
    // reads them again. Read before the session is built, since they decide the model it opens on.
    // `/cd` reads only the rules again, so the model and level settled from them stay as they are.
    let settings = bravebot_config::Settings::load();

    // What the command line named for this session, which outranks the saved pick and is not
    // recorded (CLI-31). Read off `start` before the match below consumes it.
    let asked = match &start {
        Start::Sending(_, asked) => asked.clone(),
        _ => AskedFor::default(),
    };
    let mut mcp_servers = std::mem::take(servers);
    let absent = std::mem::take(&mut mcp_servers.notes);
    let mut session = new_session(config, &settings, &workspace, &asked, mcp_servers, basis);
    // The same answer decides whether a chord on Escape can arrive at all, and the session is given
    // it here because the chords are read further down, out of the settings (INPUT-32).
    session.adopt_keys_disambiguated(session.ctrl_enter_arrives);

    // What the session begins holding, which is what decides whether the startup question is put
    // to its user at all. Read off `start` before the match below consumes it.
    let beginning = beginning_of(&start, workspace.root());
    // Read off `start` for the same reason, and matched only after the question below is answered,
    // because the set includes the checkout's definitions only when the person vouched for them.
    // The name the record carries is the person's own earlier `--agent`; `named` is the one typed
    // on this command line (ADDRESS-3).
    let StartFacts {
        recorded,
        first_turn,
        prefill,
        choosing,
    } = StartFacts::of(&start);

    // Outlives every turn, which is the point: a turn begins with the exchange so far rather
    // than with nothing, so the user can say "try that again" and be understood. A resumed
    // session begins with an exchange that outlived the process it happened in.
    let (conversation, mut stored, programs) = match start {
        // These five begin a new record. `Choose` and `ChooseFromPr` open the Sessions screen over
        // it, in `apply_the_command_line`.
        Start::Fresh
        | Start::Sending(..)
        | Start::Prefilling(..)
        | Start::Choose
        | Start::ChooseFromPr(_) => (
            Conversation::new(),
            bravebot_session::sessions::Handle::begin(
                workspace.root(),
                bravebot_session::sessions::Front::Terminal,
                bravebot_stamp::BUILD,
            ),
            // A session that was never asked vouches for nothing, exactly as with the map.
            TrustedPrograms::new(),
        ),
        Start::Resuming(record) => {
            match restore_resumed(&mut session, config, &mut workspace, &record) {
                Ok(restored) => restored,
                Err(held) => return Err(Ended::Refused(held.refusal())),
            }
        }
    };

    // Settled once, before any turn. Nothing means the user left at the question, and a session
    // they never agreed to have must not begin behind it.
    let Some((mut trust, whence)) = opening_trust(
        terminal,
        &mut session,
        workspace.root(),
        beginning,
        stored.id(),
    ) else {
        return Err(left_behind(&stored));
    };

    // Somewhere of its own to write what is not part of the project, held for as long as the
    // session: dropping it takes the directory and everything in it. After the question, so a
    // person who left at it has nothing created for a session they declined to have.
    let scratch = opened_scratch(&mut session, &mut workspace);

    // The session this is, resumed or new, and the index of its workspace are what the sweep of
    // what has outlived the retention period must leave, whatever their age (STATE-4). Once the
    // session exists and not before, so a record a person picked to resume is not removed from
    // under the pick.
    bravebot_session::retention::at_startup(
        settings.retention(),
        bravebot_session::retention::InUse {
            session: Some(stored.id()),
            workspace: Some(workspace.root()),
        },
    );

    say_startup_notes(&mut session, config, &settings, absent);

    adopt_settings(&mut session, &settings, asked.effort);
    // Beside the settings, with the workspace in hand: a style file reached through it is not read.
    adopt_style(&mut session, &workspace, settings.style());
    crate::title::adopt(
        settings.terminal_title(),
        bravebot_core::incognito::engaged(),
    );
    crate::notify::adopt(
        settings.notification_method(),
        settings.notification_condition(),
        std::env::var("TERM_PROGRAM").ok().as_deref(),
    );
    let sources = RuleSources::ambient();
    let Some(permissions) = rules_from(
        &mut session,
        &settings,
        workspace.root(),
        sources.grants(workspace.root()).as_ref(),
        whence,
        stored.id(),
        |asking, carried| crate::trust_prompt::ask_granted(terminal, asking, carried),
    ) else {
        return Err(left_behind(&stored));
    };
    // Said out loud, as a rule that parses as nothing is, and it matters more: this one is not a
    // rule that quietly does nothing but every rule at once. Said before the first prompt can
    // be typed, so a person who did not mean to pass the flag finds out before a write happens
    // rather than after one. `/status` says it too, for the rest of the session, since a note
    // scrolls away.
    if skip_permissions {
        session.note(t!(session_permissions_skipped));
    }
    note_startup_input_lost(&mut session, crate::input::take_lost_at_startup());
    // And for the same reason again: a mode that stops a prompt appearing has to be said before
    // the first slot reaches it, since the one thing a person cannot read off the transcript is a
    // question that was never put. `/status` says it too, for the rest of the session.
    session.say_vetting_in_force();
    open_requested_directories(
        terminal,
        &mut session,
        &mut workspace,
        &mut trust,
        whence,
        &settings,
        &stored,
    )?;
    let answers = Answers::opening(
        trust,
        programs,
        Rules {
            sources,
            permissions,
        },
    );

    // Matched after every startup question, because the set holds the checkout's definitions only
    // when the answer vouched for them. A name matching nothing starts no session, because a
    // planner's session in its place would look the same on screen and have more tools (CLI-17).
    if let Err(refused) = settle_under(
        &mut session,
        config,
        &workspace,
        bravebot_agent::home::directory().as_deref(),
        &answers.trust,
        &answers.rules.permissions,
        &mut stored,
        named.as_deref(),
        recorded.as_deref(),
        settings.agent(),
    ) {
        return Err(Ended::Refused(refused));
    }

    // After the record is open, so a link the command line gave replaces one a resumed record held,
    // as `/issue` and `/pr` would (PANEL-12).
    apply_the_command_line(
        &mut session,
        &mut stored,
        workspace.root(),
        links,
        first_turn,
        prefill,
        choosing,
    );

    let run_settings = RunSettings::from_settings(&settings);
    Ok(Begun {
        workspace,
        settings,
        run_settings,
        session,
        conversation,
        stored,
        scratch,
        answers,
    })
}

/// Fire `session-started` for the session that has just begun, before its first turn is sent
/// (HOOK-10), and note in the transcript what a hook that went wrong said.
///
/// `draw` puts the opening screen up first where a hook will run, so the person is not left on a
/// blank one while it holds the first prompt back, and is not called where none will. It is also
/// called where a plugin's hook may be asked about, so the question has a screen under it. Said once
/// however many conversations `/resume` opens in this interface, because `moments` has fired it
/// already.
///
/// `started` fires the moment, and is handed the same `state` as `draw`, so one terminal serves the
/// screen and the question. The question for a plugin's hook comes through
/// [`bravebot_agent::hooks::SessionMoments::started_asking`], the decision every other moment of a
/// plugin's hook goes through.
fn start_the_session<T>(
    session: &mut Session,
    moments: &mut bravebot_agent::hooks::SessionMoments,
    state: &mut T,
    draw: impl FnOnce(&mut T, &mut Session) -> io::Result<()>,
    started: impl FnOnce(&mut T, &mut bravebot_agent::hooks::SessionMoments) -> Vec<String>,
) -> io::Result<()> {
    if moments.will_start_a_hook() || moments.may_ask_at_start() {
        draw(state, session)?;
    }
    for said in started(state, moments) {
        session.note(said);
    }
    Ok(())
}

/// Whether a question about a plugin's `file-changed` entry may be put now: the box holds no draft,
/// so a key typed into it is never taken as the answer, and the Sessions screen is not what is on
/// the screen. The event loop needs a real terminal, so this is the condition it tests, named so a
/// test can reach it.
fn may_put_a_plugin_question(session: &Session) -> bool {
    session.input().is_empty() && !session.showing_sessions()
}

/// Concrete in the backend rather than generic: the loop is only ever driven by a real
/// terminal, and a generic backend's error type carries no bounds to convert from.
///
/// Returns the session left behind, where there is one to pick up again.
fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: &mut Config,
    workspace: &Workspace,
    servers: &mut crate::state::Servers,
    startup: Startup,
    moments: &mut bravebot_agent::hooks::SessionMoments,
    switch: &mut Option<Switch>,
) -> io::Result<Ended> {
    let skip_permissions = startup.skip_permissions;
    let Begun {
        mut workspace,
        settings,
        run_settings,
        mut session,
        mut conversation,
        mut stored,
        mut scratch,
        mut answers,
    } = match begin_session(terminal, config, workspace, servers, startup) {
        Ok(begun) => begun,
        Err(ended) => return Ok(ended),
    };

    // The session has begun and the first turn is not yet sent, which is where a hook on the start
    // of a session runs (HOOK-10). A plugin's hook there is asked about first, in this terminal, by
    // the decision every moment of a plugin's hook goes through (PLUGIN-7).
    moments.keep_answers(session.account_session().clone());
    start_the_session(
        &mut session,
        moments,
        terminal,
        redraw,
        |terminal, moments| {
            let mut asking = crate::confirm::TerminalConfirmer::new(terminal);
            moments.started_asking(&mut asking)
        },
    )?;
    // The files a person asked to have a command run for, looked at from here until the interface is
    // left (HOOK-2). Read from the directory the session opened in, as the session moments are. A
    // plugin's entries are watched too, and each is asked about at the first change that would run
    // it.
    let watching = bravebot_agent::hooks::Watching::begin(
        bravebot_agent::hooks::FileChanged::read(
            bravebot_agent::home::directory().as_deref(),
            workspace.root(),
            Instant::now(),
        )
        .with_plugins(
            bravebot_agent::home::directory().as_deref(),
            session.account_session().clone(),
            Instant::now(),
        ),
    );

    // Drawn when something has changed rather than on every pass. A drag arrives as a stream of
    // positions, and a frame for each costs more than the whole gesture is worth: with a long
    // transcript the queue outruns the drawing and the highlight trails seconds behind the
    // pointer. Coalescing a burst into one frame is what makes it keep up.
    let mut needs_draw = true;
    let mut drawn_at = Instant::now();
    let mut face_held = crate::avatar::look_now();
    // The request for a suggestion that has gone out and not yet answered, if one has (INPUT-43).
    let mut suggesting: Option<Suggesting> = None;

    loop {
        settle_between_frames(
            &mut session,
            config,
            &workspace,
            &mut stored,
            &conversation,
            &answers,
            &mut suggesting,
            watching.as_ref(),
            &mut needs_draw,
        );
        // A question about a plugin's `file-changed` entry that its thread is waiting on. Put only
        // where the box holds nothing, so a key typed into a draft is never taken as the answer, and
        // not over the Sessions screen. It waits, with the thread, until then. The decision is the
        // one every moment's plugin hook goes through (PLUGIN-7).
        if let Some(watching) = watching.as_ref()
            && may_put_a_plugin_question(&session)
        {
            let mut asking = crate::confirm::TerminalConfirmer::new(terminal);
            needs_draw |= watching.serve(&mut asking);
        }

        // Waiting for the burst to end, but not indefinitely: a drag that never pauses would
        // otherwise show nothing until it stopped. Only a drag or a resize is waited out. A typed character
        // is drawn as soon as it has been handled, whatever is queued behind it.
        let waited_long_enough = drawn_at.elapsed() >= FRAME;
        if needs_draw && (waited_long_enough || !input::next_is_a_stream()?) {
            redraw(terminal, &mut session)?;
            needs_draw = false;
            drawn_at = Instant::now();
        }
        // Every pass rather than at each place the name changes, so a name that arrives by a path
        // nobody listed here (a first save, `/rename`, a resume, a rewind, `/clear`) still reaches
        // the title. An unchanged name writes nothing.
        crate::title::show(terminal.backend_mut(), stored.title())?;
        // The pointer's shape follows a report of the pointer, which asks for no frame, so it is
        // looked at here as well as when one is drawn.
        crate::pointer::show(terminal.backend_mut(), session.pointer_shape())?;

        if session.is_quitting() {
            return Ok(left_behind(&stored));
        }

        // While the Sessions screen is open the session starts nothing of its own. Its list is read
        // again on a timer, because nobody presses a key for a session elsewhere to change.
        let showing_sessions = session.showing_sessions();
        if showing_sessions
            && sessions_screen::read_sessions_screen_if_due(&mut session, Instant::now())
        {
            needs_draw = true;
        }
        let Some(action) = next_idle_action(
            &mut session,
            &workspace,
            &answers.trust,
            showing_sessions,
            suggesting.is_some(),
            drawn_at,
            &mut face_held,
            &mut needs_draw,
        )?
        else {
            continue;
        };

        needs_draw |= !matches!(action, Action::None);

        // Settled ahead of the match, so a name that matched becomes the turn the `Submit` arm
        // runs and there is no second copy of that arm to keep in step with it. Resolved here and
        // now rather than once for the session, because a turn resolves the set afresh too, and a
        // name printed from a stale set would be a name the turn then refuses.
        let action = match action {
            Action::Address(argument, pasted, attached, names) => {
                let definitions = bravebot_agent::agents::resolved(
                    &workspace,
                    bravebot_agent::home::directory().as_deref(),
                    answers.trust.clone(),
                    answers.rules.permissions.clone(),
                    &mut Trail::new(),
                );
                address(
                    &mut session,
                    config,
                    &definitions,
                    &argument,
                    pasted,
                    attached,
                    names,
                )
            }
            other => other,
        };

        let private = matches!(action, Action::RunPrivately(_));
        let mut held = Held {
            session: &mut session,
            conversation: &mut conversation,
            stored: &mut stored,
            answers: &mut answers,
            workspace: &mut workspace,
            scratch: &mut scratch,
            settings: &settings,
            run_settings: &run_settings,
        };
        match action {
            Action::Quit => return Ok(left_behind(held.stored)),
            Action::Copy => copy_selection(terminal, held.session, crate::clipboard::copy)?,
            Action::CopyReply(text) => copy_reply(held.session, &text, crate::clipboard::copy),
            Action::OpenUrl(address) => open_link(held.session, &address, false),
            Action::Preview(path) => held.preview_file(&path),
            Action::EditFile(path) => held.edit_file(terminal, &path)?,
            Action::Paste => take_from_clipboard(held.session, crate::clipboard::paste()),
            Action::Edit => edit_prompt(terminal, held.session)?,
            #[cfg(unix)]
            Action::Suspend => suspend_session(terminal, held.session)?,
            Action::Export(path) => {
                export_session(
                    held.session,
                    held.stored,
                    held.workspace.root(),
                    path.as_deref(),
                );
            }
            Action::Undo => {
                held.asking_about_a_restore(terminal, |held, ask| held.rewind_steps(ask, 1))
            }
            Action::Rewind(turns) => {
                held.asking_about_a_restore(terminal, |held, ask| held.rewind_command(ask, &turns))
            }
            Action::Hunks(argument) => held
                .asking_about_a_restore(terminal, |held, ask| held.hunks_command(ask, &argument)),
            Action::Show => show_transcript(terminal, held.session)?,
            Action::ChooseModel(scope) => open_model_picker(held.session, config, scope),
            Action::ChooseTheme => choose_theme(terminal, held.session),
            Action::SetTheme(name) => set_theme(held.session, &name),
            Action::ChooseEffort(scope) => choose_effort(terminal, held.session, scope),
            Action::SetEffort(level, scope) => set_effort(held.session, &level, scope),
            Action::Variant(word) => set_variant(held.session, config, &word),
            Action::Advisor(word) => set_advisor(held.session, config, &word),
            Action::Style(word) => set_style(held.session, held.workspace, &word),
            Action::Mode(word) => set_mode(
                held.session,
                held.stored,
                held.workspace,
                held.answers.trust.clone(),
                held.answers.rules.permissions.clone(),
                &word,
            ),
            Action::Language(word) => set_language(held.session, &word),
            Action::CrashReports(word) => crate::crash_reports_command::run(
                held.session,
                &word,
                &bravebot_config::Managed::load(),
            ),
            Action::ChooseEditing => choose_editing(terminal, held.session),
            Action::AddDirectory(directory) => held.add_directory_command(&directory),
            Action::CloseDirectory(directory) => held.close_directory_command(&directory),
            Action::Note(text) => note_session(held.session, held.stored, &text),
            Action::ChangeDirectory(directory) => {
                if let Some(ended) = held.change_directory_command(terminal, &directory) {
                    return Ok(ended);
                }
            }
            Action::Rename(name) => rename_session(held.session, held.stored, &name),
            Action::Background => {
                if let Some(ended) = held.background() {
                    return Ok(ended);
                }
            }
            Action::Branch(name) => held.branch(&name),
            Action::Resume(typed) => {
                if let Some(ended) = held.resume(servers, switch, &typed) {
                    return Ok(ended);
                }
            }
            Action::OpenCenter => held.open_sessions_screen(),
            Action::SessionsScreen(effect) => {
                if let Some(ended) = held.sessions_screen_effect(terminal, servers, switch, effect)
                {
                    return Ok(ended);
                }
            }
            Action::Link(kind, argument) => {
                link_session(held.session, held.stored, kind, &argument)
            }
            Action::Status => held.status(config),
            Action::ForgetTrust => held.forget_trust_command(),
            Action::ArmWatch(path) => held.arm_watch_command(&path),
            Action::Sandbox(word) => {
                crate::sandbox_command::run(held.session, &word, &bravebot_config::Managed::load())
            }
            Action::Reach(argument) => held.reach(&argument),
            Action::Mcp(argument) => held.mcp(terminal, &argument, skip_permissions),
            Action::Plugin(argument) => held.plugin(terminal, &argument),
            Action::ListCheckouts => held.list_checkouts(),
            Action::Unfinished => held.session.report_unfinished(held.workspace),
            Action::ListMemories => held.list_memories(),
            Action::RemoveCheckout(id) => held.remove_checkout_command(terminal, &id),
            Action::ApplyCheckout(id, paths) => held.apply_checkout(terminal, config, &id, &paths),
            Action::Compact(focus) => held.compact(terminal, config, &focus)?,
            Action::Aside(question, pasted, attached) => {
                held.aside(terminal, config, &question, &pasted, &attached)?
            }
            Action::Recap => held.recap(terminal, config)?,
            Action::CommitMessage(asked) => held.commit_message(terminal, config, &asked)?,
            Action::Manifest(task, pasted, attached) => {
                // Sized against the window of the model that answers it, as a turn is.
                adopt_the_awaited_budget(held.session, config, Some(BUDGET_WAIT));
                held.manifest(terminal, config, &task, &pasted, &attached)?
            }
            Action::Clear => {
                if let Some(ended) = held.clear(terminal, None) {
                    return Ok(ended);
                }
            }
            Action::HandOver(brief) => {
                if let Some(ended) = held.clear(terminal, Some(brief)) {
                    return Ok(ended);
                }
            }
            Action::Handoff(goal) => held.handoff(terminal, config, &goal)?,
            Action::Enhance(line, draft) => held.enhance(terminal, config, &line, &draft)?,
            Action::Submit(prompt) => suggesting = held.submit(terminal, config, prompt)?,
            Action::Run(line) | Action::RunPrivately(line) => {
                held.run_line(terminal, &line, private)?
            }
            // Cancel and SendNow are only reachable while a turn runs, which `run_turn_animated`
            // handles.
            Action::Cancel | Action::SendNow | Action::None | Action::Redraw => {}
            // Settled into a `Submit` or a note ahead of this match.
            Action::Address(..) => {}
        }
        // What the person chose over a running turn, and answered `y` to stopping it for, is
        // carried out now that the turn has ended and its record is written (SCREEN-15).
        if let Some(effect) = held.session.take_held_for_the_end_of_the_turn() {
            needs_draw = true;
            if let Some(ended) = held.sessions_screen_effect(terminal, servers, switch, effect) {
                return Ok(ended);
            }
        }
    }
}

/// What the loop settles on each pass before the frame is drawn: the lists a slash or an `@` offers,
/// what arrived from a request or a thread since the last pass, the names a panel shows, and the
/// notes of a command that went wrong. Each sets `needs_draw` where it changed what is on the screen.
#[expect(
    clippy::too_many_arguments,
    reason = "the loop's own state, each part of which one of the steps reads or sets"
)]
fn settle_between_frames(
    session: &mut Session,
    config: &mut Config,
    workspace: &Workspace,
    stored: &mut bravebot_session::sessions::Handle,
    conversation: &Conversation,
    answers: &Answers,
    suggesting: &mut Option<Suggesting>,
    watching: Option<&bravebot_agent::hooks::Watching>,
    needs_draw: &mut bool,
) {
    // Every pass, so a hold goes as soon as the last turn ends and the loop stops, and one is
    // taken for a loop's next tick before the wait for it begins.
    session.keep_awake();
    // Before the frame and before the next key, so what a slash offers is on the screen as the
    // slash is, and Tab never reaches a list the frame did not show.
    let reached = session.servers.session.clone();
    session.settle_skills(|| {
        let mut offered = crate::skills::resolved(
            workspace,
            answers.trust.clone(),
            answers.rules.permissions.clone(),
        );
        // Beside the skills and sorted among them by name. A prompt's word starts `mcp__` and
        // an alias, so it never claims a word a command holds.
        offered.extend(crate::skills::from_servers(reached.as_ref()));
        offered
    });
    // The project's styles a `/style` could name, read where the trust map is in hand.
    session.settle_styles(|| {
        bravebot_agent::styles::offered_from_project(
            workspace,
            bravebot_agent::home::directory().as_deref(),
            answers.trust.clone(),
            answers.rules.permissions.clone(),
            &mut Trail::new(),
        )
    });
    // The same for the sessions an `@` offers after the files.
    session.settle_sessions(|| past_sessions_offered(workspace.root(), stored.id()));
    session.settle_definitions(|| {
        definitions_offered(
            workspace,
            answers.trust.clone(),
            answers.rules.permissions.clone(),
        )
    });
    // A picture decoded since the last pass is drawn on this one rather than at the next key.
    *needs_draw |= session.settle_previews();
    // The rows of a model picker still being listed are drawn on the pass they arrive.
    *needs_draw |= session.settle_picking();
    // The answer of a `/schedule run-now` is drawn on the pass it arrives.
    *needs_draw |= session.settle_schedule_runs();
    // The window of the model the session opened on, asked for while the first frame was drawn
    // (BACKEND-64).
    *needs_draw |= adopt_the_awaited_budget(session, config, None);
    // A target that was marked and is not now, as when a key or the terminal's being left ended
    // it with no other answer that draws (MOUSE-51).
    *needs_draw |= session.take_the_hover_ended();
    // A suggestion the box has stopped being empty for is dropped, and one whose request has
    // answered is taken, before the frame so it is drawn on this pass (INPUT-43).
    session.settle_suggestion();
    *needs_draw |= take_suggestion(session, stored, suggesting);
    // A question asked beside a turn that has ended since, answered on the pass it arrives. Its
    // row is in the record, so a session left after it resumes with the answer (CMD-8).
    if settle_aside_between_turns(session) {
        let title = stored.title().to_string();
        with_standing(
            session,
            conversation,
            &answers.trust,
            &answers.programs,
            workspace,
            |standing| stored.save(&title, standing),
        );
        *needs_draw = true;
    }
    // Before the frame rather than after it, so the first frame of an open panel names the
    // session, and a name that changed by any path reaches the panel without waiting for a key.
    *needs_draw |= session.identify(
        stored.title(),
        crate::status::abbreviate(workspace.root()),
        stored.branch(),
    );
    *needs_draw |= name_the_id(session, stored);
    // A drag held outside the transcript scrolls it a step on each pass, with the pointer still
    // and no report to wake the loop (MOUSE-37).
    *needs_draw |= session.scroll_toward_the_pointer(Instant::now());
    *needs_draw |= show_the_links(session, stored);

    // A pick made while a turn ran that no loop of the turn was left to carry out, because the
    // turn ended or another working loop began first.
    if let Some(picked) = session.take_pick() {
        carry_out_pick(session, config, picked);
        *needs_draw = true;
    }

    // What a `file-changed` command said when it went wrong, which no turn is open to carry.
    if let Some(watching) = watching {
        for said in watching.said() {
            session.note(said);
            *needs_draw = true;
        }
    }
}

/// The action the loop at rest takes next, and `Ok(None)` where nothing arrived and it is to go round
/// again: a loop's tick, a watch that fired, what the queue holds or a click that waited, and
/// otherwise what the person typed while the loop waited for it.
///
/// While the Sessions screen is open the session starts nothing of its own: a loop's tick, a watch
/// and the queue wait until it is left (SCREEN-1), and the wait for a key still happens.
#[expect(
    clippy::too_many_arguments,
    reason = "the loop's own state, each part of which one of the steps reads or sets"
)]
fn next_idle_action(
    session: &mut Session,
    workspace: &Workspace,
    trust: &TrustStore,
    showing_sessions: bool,
    suggesting: bool,
    drawn_at: Instant,
    face_held: &mut crate::avatar::Look,
    needs_draw: &mut bool,
) -> io::Result<Option<Action>> {
    if !showing_sessions {
        // A loop's next tick, looked at before the interface settles down to wait for a key.
        // Nobody is going to press anything to make it happen, so a tick taken only after input
        // arrives would sit there until somebody typed something unrelated. It becomes an
        // ordinary submission, because that is what it is: the line the person typed at `/loop`,
        // sent again.
        if let Some(prompt) = session.loop_tick() {
            return Ok(Some(Action::Submit(prompt)));
        }
        // Then the watches, looked at here for the same reason and in the same pass: a
        // filesystem change is the one event in this program that nobody presses a key for.
        // Only one of the two can produce a turn, and a session holds only one kind at a
        // time, so the order between them decides nothing.
        //
        // The reach is derived again at each pass from the live mode, so a watch on a path
        // outside the workspace ends once the key leaves bypass (FSWATCH-7).
        let session_mode = session.live_permission_mode();
        if let Some(prompt) = session.watch_fired(Instant::now(), |path, under| {
            bravebot_agent::tools::look_at_a_watch(workspace, &session_mode, trust, path, under)
        }) {
            return Ok(Some(Action::Submit(prompt)));
        }
        // Then what the queue is holding, before the interface settles down to wait, for
        // the reason a tick is looked at here.
        // Then a click on a path that has waited to see whether it was the first of a
        // double click, which nobody is going to press anything to carry out (MOUSE-42).
        if let Some(action) = queued_or_recap(session, Instant::now()).or_else(|| {
            session
                .take_the_click_due(Instant::now())
                .map(Action::Preview)
        }) {
            return Ok(Some(action));
        }
    }
    if a_session_is_owed_a_frame(session, drawn_at.elapsed()) {
        *needs_draw = true;
    }
    // The face on the opening screen blinks and glances without a key, and is
    // drawn again only when how it is held has changed.
    let opening = render::opening_screen(session);
    let held = crate::avatar::look_now();
    if a_face_is_owed_a_frame(opening, held, *face_held) {
        *face_held = held;
        *needs_draw = true;
    }
    // Through the reader rather than the terminal, so a run is seen whole.
    if !input::poll(wait_for_input(session, opening, suggesting))? {
        return Ok(None);
    }
    let taken = input::read()?;
    took_input(session, &taken);
    Ok(Some(idle_action(session, taken)))
}

/// How long the loop at rest waits for a key before it goes round again.
fn wait_for_input(session: &Session, opening: bool, suggesting: bool) -> Duration {
    let mut wait = wait_for_a_key(opening, crate::indicator::stilled());
    // And sooner while that is going on, for the next step of it.
    if session.pulling_at_an_edge() {
        wait = wait.min(select::EDGE_TICK);
    }
    // Short while a suggestion is on its way, so it is drawn as it arrives
    // rather than at the next key.
    if suggesting || session.picking_is_loading() || session.aside_in_flight() {
        wait = wait.min(FRAME);
    }
    // And until a click that waits is due.
    if let Some(due) = session.the_click_due_in(Instant::now()) {
        wait = wait.min(due);
    }
    // And until the Sessions screen's list is to be read again (SCREEN-2).
    if let Some(sessions_screen) = session.sessions_screen() {
        wait = wait.min(sessions_screen.due_in(Instant::now()));
    }
    wait
}

/// A request for a follow-up suggestion that has gone out on a thread of its own (INPUT-43).
struct Suggesting {
    answer: mpsc::Receiver<Suggested>,
}

/// What the request for a suggestion came back with.
struct Suggested {
    /// The suggestion, where the answer was inside the counts the instruction asked for.
    text: Option<String>,
    usage: bravebot_aichat::protocol::Usage,
    events: Vec<Stamped>,
}

/// Whether a turn that just ended is followed by a request for a suggestion (INPUT-43).
///
/// Every input is something the driver already knows: the setting, the mode, how the turn ended and
/// what else is waiting. Nothing the model wrote is read to decide it.
fn suggestion_is_due(
    on: bool,
    incognito: bool,
    mode: bravebot_agent::PermissionMode,
    ending: Option<bravebot_agent::Ending>,
    carrying_on: bool,
) -> bool {
    on && !incognito
        && mode != bravebot_agent::PermissionMode::Plan
        && matches!(ending, Some(bravebot_agent::Ending::Done))
        && !carrying_on
}

/// Ask for the next prompt when the turn that ended is one to follow, without waiting for it.
///
/// The exchange is forked here, on the thread that holds it, and only the request crosses to the
/// worker, as an aside's does: nothing the worker does can reach the conversation.
fn suggest_next(
    session: &Session,
    config: &Config,
    conversation: &Conversation,
    trust: &TrustStore,
) -> Option<Suggesting> {
    let carrying_on = session.looping().is_some() || session.watch_is_firing();
    if !suggestion_is_due(
        session.suggests(),
        bravebot_core::incognito::engaged(),
        session.permission_mode(),
        session.finished.as_ref().map(|finished| finished.ending),
        carrying_on,
    ) {
        return None;
    }
    let asking = bravebot_agent::aside::Question::suggest(conversation);
    let config = config.clone();
    let trust = trust.clone();
    let session_model = session.model().map(str::to_string);
    let account = session.account_session().clone();
    let (send, answer) = mpsc::channel();
    thread::spawn(move || {
        let egress = Egress::new();
        let mut sink = Trail::new();
        // The model `summaryModel` names where it names one, which is the cost boundary the
        // setting is for. One that cannot be used is no suggestion, and not a note: nobody asked.
        let done =
            bravebot_agent::compact::side_request_model(&config, &egress, session_model.as_deref())
                .ok()
                .and_then(|model| {
                    turn::aside(
                        &config,
                        &egress,
                        &account,
                        asking,
                        Some(model.as_ref()),
                        &mut sink,
                        trust,
                        |_written| {},
                        &Cancel::new(),
                    )
                    .ok()
                });
        let (text, usage) = suggested_text(done);
        let _ = send.send(Suggested {
            text,
            usage,
            events: sink.events().to_vec(),
        });
    });
    Some(Suggesting { answer })
}

/// The text a suggestion request's answer offers, and what the request cost (INPUT-43).
///
/// Text is offered only where the answer is one the planner could have held (`kept`), as a brief
/// is: a conversation that has met something untrusted offers nothing, but the request is still
/// charged.
fn suggested_text(
    done: Option<bravebot_agent::aside::Answered>,
) -> (Option<String>, bravebot_aichat::protocol::Usage) {
    match done {
        Some(answered) => (
            answered
                .kept
                .as_deref()
                .and_then(bravebot_agent::aside::suggestion),
            answered.usage,
        ),
        None => (None, Default::default()),
    }
}

/// Take the answer to a request for a suggestion, if it has come, and say whether the screen
/// changed (INPUT-43).
///
/// What the request cost is charged and its trail kept whether or not the suggestion is offered:
/// the request went out either way.
fn take_suggestion(
    session: &mut Session,
    stored: &mut bravebot_session::sessions::Handle,
    suggesting: &mut Option<Suggesting>,
) -> bool {
    let Some(waiting) = suggesting else {
        return false;
    };
    match waiting.answer.try_recv() {
        Ok(done) => {
            *suggesting = None;
            session.charge_suggestion(done.usage);
            stored.append_audit(session.turns, &done.events);
            if let Some(text) = done.text {
                session.offer_suggestion(text);
            }
            true
        }
        Err(mpsc::TryRecvError::Empty) => false,
        Err(mpsc::TryRecvError::Disconnected) => {
            *suggesting = None;
            false
        }
    }
}

/// A key press while a command runs from shell mode.
///
/// A mode left open when the command began answers the press first, since a queued command line
/// begins one as the turn ends and nothing closes a mode then. They are read in the order every
/// other loop reads them ([`claimed_by_a_mode`]): a picker, then a selection held, which Escape puts
/// away and does nothing else (MOUSE-38), then the delegate view, which takes every key (WATCH-9),
/// and the scroller, which takes its own (SCROLL-1). So Ctrl-C and Escape are theirs while one is
/// open, and the first press after it closes is the one that reaches the command. Otherwise a
/// running command is something to stop, so Ctrl-C stops it and stays, for the reason it stops a
/// turn: the way out is the press after that, at the box. Every other key waits. A command is
/// brief, and taking a prompt here would leave it half-typed when the output lands on top of it.
fn command_key(session: &mut Session, key: KeyEvent, cancel: &Cancel) {
    if key.kind == KeyEventKind::Release {
        return;
    }
    // A mode's key never asks for a stop, so what it reads in return is not handed on.
    if let Some(action) = claimed_by_a_mode(session, key) {
        if !sessions_screen::say_why_it_waits(session, &action, sessions_screen::Busy::Command) {
            let _ = act_while_working(session, action, crate::clipboard::paste);
        }
    } else if stops_a_command(session, key) {
        stop_what_is_running(session, cancel);
    }
}

/// Run a command the user typed in shell mode, redrawing while it runs.
///
/// On a worker thread for the reason a turn is: a command can take as long as it likes, and running
/// it on the thread that owns the terminal would freeze the interface for the duration and make a
/// slow build indistinguishable from a hang. Escape cancels, which kills it.
///
/// What it prints is drawn as it prints it (SHELL-7), so a person can tell a slow build from a hang
/// and decide to stop it from what it has printed so far. The pieces reach the screen alone: what
/// the conversation is given is what [`settle_command`] releases when the command has ended.
///
/// No approval is asked for. The prompt a run normally goes through exists so a person endorses argv
/// the *planner* chose, and here the person typed it themselves: asking would be asking them to
/// confirm their own keystroke. What it printed goes into the conversation, labelled from that same
/// provenance by the kernel. See [`bravebot_agent::shell::record`]. A `private` command is the
/// exception: what it printed is drawn and goes nowhere else, through [`bravebot_agent::shell::show`].
fn run_command(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    workspace: &Workspace,
    line: &str,
    conversation: &mut Conversation,
    private: bool,
) -> io::Result<Vec<Stamped>> {
    let cancel = Cancel::new();
    let worker_cancel = cancel.clone();
    let worker_line = line.to_string();
    // Commands run in the primary root, which is the directory the prompt says it is in. An added
    // directory is reachable by path from here, so nothing is out of reach.
    let directory = workspace.root().to_path_buf();

    // Only the process goes to the worker. The conversation stays here, because handing it over
    // means taking it back, and a panicked thread hands back nothing: substituting a fresh one would
    // reset context integrity to trusted, which is an upgrade and never allowed.
    let (live, printing) = mpsc::channel();
    let worker = thread::spawn(move || {
        bravebot_agent::shell::run_live(&worker_line, &directory, &worker_cancel, &live)
    });

    // Drawn while it runs so the command appears in the transcript immediately and Escape has
    // somewhere to be pressed. The spinner is the turn indicator's, without the token counters,
    // which measure something no command spends.
    session.begin_command();
    while_the_command_runs(session, &worker, &printing, |session| {
        session.scroll_toward_the_pointer(Instant::now());
        redraw(terminal, session)?;

        while input::poll(Duration::ZERO)? {
            let taken = input::read()?;
            took_input(session, &taken);
            match taken {
                TermEvent::Key(key) => command_key(session, key, &cancel),
                TermEvent::Mouse(mouse) => {
                    // What a click leaves besides a copy is a command it stands for, which
                    // `handle_mouse` has queued, since no turn runs here to carry it out.
                    let _ = answer_mouse(terminal, session, mouse, crate::clipboard::copy)?;
                }
                // Everything else waits. A command is brief and the keys that matter during one are
                // the two above; taking a prompt here would leave it half-typed when the output
                // lands on top of it.
                _ => {}
            }
        }
        Ok(())
    })?;

    let ran = worker.join().unwrap_or_else(|_| {
        Err(bravebot_agent::shell::ShellError::Io(
            t!(command_thread_stopped).to_string(),
        ))
    });
    Ok(settle_command(session, line, ran, conversation, private))
}

/// Keep a frame drawn until the command ends, with what it has printed by then.
///
/// Each pass takes what the command has printed since the last one onto the session and then runs
/// `step`, which draws the frame and answers the keys. A command that prints nothing for minutes
/// still gets a frame every pass, for the spinner and for the key that stops it. All that is queued
/// is taken at once, in one call: a command printing faster than frames are drawn sends many pieces
/// between two, and the session looks only at the end of them.
///
/// Once more when the command has ended, so what it printed after the last pass is not lost with
/// the channel: a stopped command keeps what it had printed (SHELL-7).
fn while_the_command_runs<T>(
    session: &mut Session,
    worker: &thread::JoinHandle<T>,
    printing: &mpsc::Receiver<String>,
    mut step: impl FnMut(&mut Session) -> io::Result<()>,
) -> io::Result<()> {
    while !worker.is_finished() {
        session.command_printing(&printing.try_iter().collect::<String>());
        step(session)?;
        std::thread::sleep(FRAME.min(Duration::from_millis(50)));
    }
    session.command_printing(&printing.try_iter().collect::<String>());
    Ok(())
}

/// What a command that has ended leaves: the text the gate released drawn in place of what was
/// drawn while it ran, or a note saying why nothing was released, and the trail of what was decided.
///
/// What was drawn while it ran goes with the result. A command that was stopped or ran too long
/// released nothing, and what it had printed stays on the screen as an entry above the note, since
/// that is what the person stopped it from, with a second note saying the model was not sent it. It
/// is never put in the conversation (SHELL-3). Where the gate refused the output or it could not be
/// read, none of it stays.
///
/// The labelling happens here, on the thread that owns the conversation, and its trail is returned
/// so the decision reaches the audit file and Ctrl-T. Trusting a command's output is the most
/// consequential thing this feature does, so it must not be the one thing left unrecorded.
fn settle_command(
    session: &mut Session,
    line: &str,
    ran: Result<bravebot_agent::shell::Ran, bravebot_agent::shell::ShellError>,
    conversation: &mut Conversation,
    private: bool,
) -> Vec<Stamped> {
    let kept = match &ran {
        Err(
            bravebot_agent::shell::ShellError::Cancelled
            | bravebot_agent::shell::ShellError::TookTooLong { .. },
        ) => session.finish_stopped_command(),
        _ => {
            session.finish_command();
            false
        }
    };

    let mut sink = Trail::new();
    match ran {
        Ok(ran) => match shell_result(line, &ran, conversation, &mut sink, private) {
            Ok(recorded) => {
                match private {
                    true => session.printed_privately(&recorded.text),
                    false => session.printed(&recorded.text),
                }
                if !recorded.succeeded {
                    session.note(t!(command_reported_a_failure));
                }
            }
            Err(error) => session.note(format!("{error}")),
        },
        Err(error) => {
            session.note(format!("{error}"));
            if kept {
                session.note(t!(shell_stopped_not_sent));
            }
        }
    }

    sink.events().to_vec()
}

/// Release what a command printed, into the conversation unless the person typed it `!!`.
fn shell_result(
    line: &str,
    ran: &bravebot_agent::shell::Ran,
    conversation: &mut Conversation,
    sink: &mut Trail,
    private: bool,
) -> Result<bravebot_agent::shell::Recorded, bravebot_agent::shell::ShellError> {
    match private {
        true => bravebot_agent::shell::show(line, ran, sink),
        false => bravebot_agent::shell::record(line, ran, conversation, sink),
    }
}

/// Hand the worker's messages to `handle`: the one this frame waited for, and then everything
/// already queued behind it.
///
/// `wait` is how long to block for the first, and a caller that is already blocking on something
/// else passes nothing. Which one a loop waits on decides how quickly it answers a key press, so
/// it belongs to the caller rather than here.
///
/// `false` when the worker has dropped its senders and the turn is over.
///
/// Drained rather than taken one per pass, for the same reason terminal events are. A reply
/// arrives as hundreds of small messages, a draw rebuilds the whole transcript from its markdown,
/// and a frame for each spent longer laying out finished turns than the reply took to arrive: the
/// queue outran the drawing and what was on the screen fell behind what had been said.
fn drain_worker(
    from_worker: &mpsc::Receiver<crate::remote_confirm::ToMain>,
    wait: Duration,
    mut handle: impl FnMut(crate::remote_confirm::ToMain),
) -> bool {
    let mut received = from_worker.recv_timeout(wait);
    loop {
        match received {
            Ok(message) => handle(message),
            Err(mpsc::RecvTimeoutError::Timeout) => return true,
            Err(mpsc::RecvTimeoutError::Disconnected) => return false,
        }

        // Whatever else is waiting, without waiting for it. Empty means the burst is over and the
        // next frame shows all of it at once.
        received = from_worker.try_recv().map_err(|gone| match gone {
            mpsc::TryRecvError::Empty => mpsc::RecvTimeoutError::Timeout,
            mpsc::TryRecvError::Disconnected => mpsc::RecvTimeoutError::Disconnected,
        });
    }
}

/// Draw frames until the worker has dropped its senders.
///
/// Each pass keeps the session awake, redraws, reads the terminal and drains what the worker has
/// said. A key goes to `on_key` and a message to `on_message`, which are the two parts that differ
/// between the loops. `on_message` is given the terminal as well, for a loop whose worker asks the
/// person something and so has a prompt to draw. The loop reads no message: what a message does with
/// the text it carries is `on_message`'s.
fn drive_worker(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    from_worker: &mpsc::Receiver<crate::remote_confirm::ToMain>,
    mut on_key: impl FnMut(&mut Session, KeyEvent) -> Action,
    mut on_message: impl FnMut(
        &mut Terminal<CrosstermBackend<io::Stdout>>,
        &mut Session,
        crate::remote_confirm::ToMain,
    ),
) -> io::Result<()> {
    loop {
        // Each frame, so an inhibitor that exits by itself while this runs is said and `/caffeinate`
        // turned off then, not at the next pass of the outer loop (CMD-12).
        session.keep_awake();
        redraw(terminal, session)?;

        // Input is still read, so a long request does not leave the interface deaf, and the
        // frame's waiting is done here for the reason the turn loop does it here: a key press
        // has to wake the loop rather than queue behind the worker.
        pump_terminal(terminal, session, only_keys(&mut on_key))?;

        // Drained for the same reason the turn's own loop drains, and waiting on the terminal
        // for the same reason it does: a person may be typing their next prompt while this runs.
        let carrying_on = drain_worker(from_worker, Duration::ZERO, |message| {
            on_message(terminal, session, message)
        });

        if !carrying_on {
            return Ok(());
        }
    }
}

/// The two ends a loop that draws a worker's questions answers them through: the token that stops
/// the work and the channel the worker is blocked on.
struct Replying<'a> {
    cancel: &'a Cancel,
    answer_tx: &'a mpsc::Sender<crate::remote_confirm::Reply>,
}

impl Replying<'_> {
    /// Hand a question's answer back to the worker that is blocked on it.
    ///
    /// In this order: stop the work when the answer asks for it, so the worker sees the stop as soon
    /// as it wakes; run `note`, which says on the transcript or records on the session whatever the
    /// answer leaves behind; then send the decision. A closed channel means the worker is already
    /// gone, so there is nothing to answer and the loop collects its result.
    ///
    /// Nothing here reads what a request carried or what the person typed: the answer type decides
    /// whether it stops and what the worker is told ([`Answered`]), and `wrap` only chooses the
    /// shape the worker expects for that kind of question.
    fn reply<A: crate::confirm::Answered>(
        &self,
        session: &mut Session,
        answer: A,
        note: impl FnOnce(&mut Session, &A),
        wrap: impl FnOnce(A::Decision) -> crate::remote_confirm::Reply,
    ) {
        // Ctrl-C at the prompt is the same request it is anywhere else in a turn: stop.
        if answer.stops_the_turn() {
            stop_what_is_running(session, self.cancel);
        }
        note(session, &answer);
        let _ = self.answer_tx.send(wrap(answer.decision()));
    }

    /// [`Replying::reply`] for a question whose answer leaves nothing to say on the transcript.
    fn plainly<A: crate::confirm::Answered>(
        &self,
        session: &mut Session,
        answer: A,
        wrap: impl FnOnce(A::Decision) -> crate::remote_confirm::Reply,
    ) {
        self.reply(session, answer, |_, _| {}, wrap);
    }
}

/// The vetting prompts' standing key: turning auto-vetting on is the interface's own decision, so it
/// is made here and said on the transcript.
fn note_vetting_turned_on(session: &mut Session, answer: &crate::confirm::VetAnswer) {
    if answer.turns_vetting_on() {
        session.choose_vetting(true);
        // Said on the transcript because the person will not otherwise see it recorded anywhere,
        // and what it changes is that later prompts do not appear.
        session.note(t!(session_vetting_on));
    }
}

/// The refusal for a question the worker is blocked on, or `None` for a message that asks nothing.
///
/// The same answer an interrupt at that prompt gives: the question is declined and nothing is
/// granted.
fn refusal(message: &crate::remote_confirm::ToMain) -> Option<crate::remote_confirm::Reply> {
    use crate::remote_confirm::{Reply, ToMain};
    use bravebot_agent::confirm::{CallDecision, Decision, RunDecision, WriteDecision};
    Some(match message {
        ToMain::Write(_) => Reply::Write(WriteDecision::reject()),
        ToMain::Run(_) => Reply::Run(RunDecision::reject()),
        ToMain::ReadOutput(_) => Reply::ReadOutput(Decision::Reject),
        ToMain::Vet(_) => Reply::Vet(Decision::Reject),
        ToMain::Fetch(_) => Reply::Fetch(Decision::Reject),
        ToMain::Vouch(_) => Reply::Vouch(Decision::Reject),
        ToMain::Exposure(_) => Reply::Exposure(Decision::Reject),
        ToMain::Server(_) => Reply::Server(Decision::Reject),
        ToMain::Manifest(_) => Reply::Manifest(Decision::Reject),
        ToMain::ToolList(_) => Reply::ToolList(Decision::Reject),
        ToMain::McpCall(_) => Reply::McpCall(CallDecision::reject()),
        ToMain::Apply(_) => Reply::Apply(bravebot_agent::confirm::ApplyDecision::unasked()),
        ToMain::Move(_) => Reply::Move(Decision::Reject),
        ToMain::Path(_) => Reply::Path(Decision::Reject),
        ToMain::Host(_) => Reply::Host(Decision::Reject),
        ToMain::Hook(_) => Reply::Hook(Decision::Reject),
        ToMain::Ask(_) => Reply::Ask(Vec::new()),
        ToMain::PromptRecorded(_)
        | ToMain::RequestBuilt(_)
        | ToMain::Forked(_)
        | ToMain::Todos(_)
        | ToMain::References(_)
        | ToMain::Written(_)
        | ToMain::Spent(_)
        | ToMain::Phase(_)
        | ToMain::Narration(_)
        | ToMain::Notice(_)
        | ToMain::Streaming(_)
        | ToMain::Thinking(_)
        | ToMain::Composing(_)
        | ToMain::Started(_)
        | ToMain::Finished(_)
        | ToMain::Movable(_)
        | ToMain::Job(_)
        | ToMain::CheckStarted(_)
        | ToMain::CheckFinished
        | ToMain::HookStarted(..)
        | ToMain::HookFinished
        | ToMain::Quarantined(_)
        | ToMain::Printed(_)
        | ToMain::Returned(_)
        | ToMain::Landed(_)
        | ToMain::Interjected(_)
        | ToMain::DelegateWaiting(_)
        | ToMain::DelegateStarted(_)
        | ToMain::DelegateFinished { .. }
        | ToMain::ReportingFor(_) => return None,
    })
}

/// [`drain_worker`] for a loop that draws the worker's questions, which refuses one without
/// drawing it once the work has been stopped (INPUT-4).
///
/// Delegates share one confirmer and take turns at it, so when a person answers one delegate's
/// prompt with a stop, the next delegate's question is already on its way. Drawn, it would put a
/// prompt in front of someone who has just asked for the work to end, and stopping would take a
/// press for each delegate waiting. Everything that is not a question goes to `handle` as usual.
fn drain_worker_until_stopped(
    from_worker: &mpsc::Receiver<crate::remote_confirm::ToMain>,
    wait: Duration,
    replying: &Replying<'_>,
    mut handle: impl FnMut(crate::remote_confirm::ToMain),
) -> bool {
    drain_worker(from_worker, wait, |message| {
        if replying.cancel.is_cancelled()
            && let Some(reply) = refusal(&message)
        {
            // A closed channel means the worker is already gone, as at every other reply.
            let _ = replying.answer_tx.send(reply);
            return;
        }
        handle(message);
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "undo_tests.rs"]
mod undo_tests;

#[cfg(test)]
#[path = "sessions_screen_tests.rs"]
mod sessions_screen_tests;

#[cfg(all(test, unix))]
#[path = "chatgpt_failure_tests.rs"]
mod chatgpt_failure_tests;

#[cfg(test)]
#[path = "credential_mode_tests.rs"]
mod credential_mode_tests;

#[cfg(test)]
#[path = "mode_trail_tests.rs"]
mod mode_trail_tests;

#[cfg(all(test, unix))]
#[path = "kept_jobs_tests.rs"]
mod kept_jobs_tests;

#[cfg(test)]
#[path = "manifest_answer_tests.rs"]
mod manifest_answer_tests;

#[cfg(all(test, unix))]
#[path = "suspend_tests.rs"]
mod suspend_tests;

#[cfg(all(test, unix))]
#[path = "session_moments_tests.rs"]
mod session_moments_tests;
