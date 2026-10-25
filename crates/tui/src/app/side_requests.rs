//! The requests a session makes outside a turn, each on a worker with one reply to wait for:
//! `/compact`, an aside, `/commit`, a handoff, `/enhance` and the goal check.

use super::{
    Action, Cancel, Config, Conversation, CrosstermBackend, Duration, Egress, KeyEvent,
    KeyEventKind, PastedImage, Path, RECAP_LABEL, Session, Stamped, Terminal, Trail, TrustStore,
    Workspace, act_while_working, drain_worker, handle_key_while_working, io, mpsc, pump_terminal,
    redraw, sign_in_if_needed, stop_what_is_running, stops_the_turn, t, thread, turn,
};

/// Interpret a key press while a summary or an aside is out.
///
/// Both stop keys stop the request (INPUT-4), the way they stop a turn: it is walked away from, the
/// conversation stays as it was, and the session stays open. Leaving is the press made with nothing
/// running, so no press here leaves.
///
/// Both keys are read against [`stops_the_turn`], as the turn's own loop reads them, so a scroller,
/// a view or a search opened while the request was out answers the press and the press that reaches
/// the request behind it is the next one. Without that, the key that closes those modes everywhere
/// else stopped the request from inside them.
pub(super) fn one_request_key(session: &mut Session, key: KeyEvent, cancel: &Cancel) -> Action {
    // Presses only, for the reason the outer loop ignores releases, and for one this function
    // brings with it: the press that closes a mode leaves nothing open, so its own release would
    // read as a second press and stop a request the first never reached.
    if key.kind == KeyEventKind::Release {
        return Action::None;
    }

    if !stops_the_turn(session, key) {
        let action = handle_key_while_working(session, key);
        return act_while_working(session, action, crate::clipboard::paste);
    }

    stop_what_is_running(session, cancel);
    Action::None
}

/// What a compaction won back, in tokens.
///
/// The summariser read the part of the exchange that stopped being sent and wrote what replaced
/// it, so the difference between the two halves of its own usage is the room the conversation
/// gave back. Nothing else here can supply the figure: the server is the only thing that counts
/// tokens, and it does not count the shortened conversation until the next request.
///
/// The figure only ever over-states. What the summariser read holds the instructions it was
/// given as well as the exchange, and there is no tokeniser here to take them off again; a server
/// that reported no count of what it wrote leaves the interface's own estimate of the reply
/// standing in its place, which counts chunks rather than tokens and so counts no more of them.
/// Saturating, because those are two counts of different things and nothing guarantees their
/// order: subtracting the other way round would report a compaction that shortened nothing as
/// having freed sixteen million million million tokens.
pub(super) fn room_won_back(usage: &bravebot_aichat::protocol::Usage) -> u64 {
    usage.prompt_tokens.saturating_sub(usage.completion_tokens)
}

/// Summarise the conversation, showing the spinner while it happens.
///
/// A smaller relative of [`run_turn_animated`], and smaller because there is less to do: a
/// summariser has no tools, so nothing asks about a write, nothing asks the user a question, and
/// nothing lands in the workspace. What is left is a model call that takes as long as a round, and
/// a loop that keeps the screen alive while it does.
///
/// Either stop key walks away from the request and leaves the conversation as it was, as a failed
/// summary does (INPUT-4). A summary that has already come back is kept: the stop landed too late
/// to be one.
///
/// A summary that comes back fires the person's `compacted` hooks in `workspace`'s directory
/// (HOOK-2), and the loop below says which one is running while it holds the screen (HOOK-9). A stop
/// key does not reach a hook: it runs to its bound, as one after a cancelled turn does.
pub(super) fn compact_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    conversation: Conversation,
    trust: &TrustStore,
    focus: &str,
    workspace: &Workspace,
) -> io::Result<(Conversation, Vec<Stamped>)> {
    // For the reason a turn does it: the summary is one request to the same backend, and a sign-in
    // is not something a worker thread can ask for.
    sign_in_if_needed(terminal, session, config)?;

    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();

    let worker_config = config.clone();
    let worker_trust = trust.clone();
    let model = session.model().map(str::to_string);
    let account = session.account_session().clone();
    let focus = (!focus.is_empty()).then(|| focus.to_string());
    let cancel = Cancel::new();
    let worker_cancel = cancel.clone();
    let worker_workspace = workspace.clone();
    let home = bravebot_agent::home::directory();

    session.begin_aside();

    let worker = thread::spawn(move || {
        summarise_on_a_worker(
            &worker_config,
            &account,
            conversation,
            model.as_deref(),
            worker_trust,
            focus.as_deref(),
            &worker_cancel,
            &worker_workspace,
            home.as_deref(),
            to_main,
        )
    });

    loop {
        // Each frame, so an inhibitor that exits by itself while this runs is said and `/caffeinate`
        // turned off then, not at the next pass of the outer loop (CMD-12).
        session.keep_awake();
        redraw(terminal, session)?;

        // Input is still read, so a long summary does not leave the interface deaf, and the
        // frame's waiting is done here for the reason the turn loop does it here: a key press
        // has to wake the loop rather than queue behind it.
        pump_terminal(terminal, session, |session, key| {
            // Which of a mode standing over the session and the request a stop key is asking
            // about is that function's to say.
            one_request_key(session, key, &cancel)
        })?;

        // Drained for the same reason the turn's own loop drains, and waiting on the terminal
        // for the same reason it does: a person may be typing their next prompt while this runs.
        let carrying_on = drain_worker(&from_worker, Duration::ZERO, |message| {
            compact_reported(session, message)
        });

        if !carrying_on {
            break;
        }
    }

    let (done, conversation, sink) = worker.join().unwrap_or_else(|_| {
        (
            Err(t!(compact_ended_unexpectedly).to_string()),
            Conversation::new(),
            Trail::new(),
        )
    });

    compact_ended(session, config.context_budget, &done, cancel.is_cancelled());

    Ok((conversation, sink.events().to_vec()))
}

/// What the thread `compact_animated` starts runs: the summary, and the hooks a person declared for
/// `compacted` once it is adopted, in the directory the session works in (HOOK-2).
///
/// Apart from [`compact_animated`] so that what the terminal hands the agent, the state directory
/// the hooks are read from and the workspace they run in, is somewhere a test can call without a
/// terminal.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are what the worker thread owns or borrows, so a test can call it without a terminal"
)]
pub(super) fn summarise_on_a_worker(
    config: &Config,
    account: &bravebot_agent::chatgpt::AccountSession,
    conversation: Conversation,
    model: Option<&str>,
    trust: TrustStore,
    focus: Option<&str>,
    cancel: &Cancel,
    workspace: &Workspace,
    home: Option<&Path>,
    to_main: mpsc::Sender<crate::remote_confirm::ToMain>,
) -> (
    Result<Option<bravebot_agent::compact::Compacted>, String>,
    Conversation,
    Trail,
) {
    let mut sink = Trail::new();
    let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main);
    let egress = Egress::new();
    let mut conversation = conversation;
    // Reduced to what a person can be told before it crosses back, since the error types are
    // the kernel's and this thread is the only place they mean anything.
    let done = turn::compact(
        config,
        &egress,
        account,
        &mut conversation,
        model,
        &mut reporter,
        &mut sink,
        trust,
        focus,
        cancel,
        Some(turn::Hooked { home, workspace }),
    )
    .map_err(|error| bravebot_agent::report::why_a_summary_failed(&error));
    (done, conversation, sink)
}

/// Take what the thread summarising the conversation says while it works.
///
/// Apart from [`compact_animated`] so that what a hook running after the summary shows, and says
/// when it goes wrong, is somewhere a test can call.
pub(super) fn compact_reported(session: &mut Session, message: crate::remote_confirm::ToMain) {
    match message {
        crate::remote_confirm::ToMain::Phase(phase) => session.set_phase(phase),
        crate::remote_confirm::ToMain::Narration(text) => session.narrate(text),
        crate::remote_confirm::ToMain::Notice(text) => session.note_once(text),
        crate::remote_confirm::ToMain::Streaming(text) => session.streaming(&text),
        crate::remote_confirm::ToMain::Thinking(text) => session.thinking(&text),
        crate::remote_confirm::ToMain::Composing(call) => session.composing(call),
        crate::remote_confirm::ToMain::HookStarted(moment, program) => {
            session.hook_running(moment, program)
        }
        crate::remote_confirm::ToMain::HookFinished => session.hook_over(),
        _ => {}
    }
}

/// Take what a summary came back with, and say what the person needs to know of it.
///
/// `stopped` is whether a stop key was pressed, and it only matters to a summary that did not
/// come back: one that did is kept, the stop having landed too late to be one. Apart from
/// [`compact_animated`], which needs a terminal and a backend to reach, so that what the end does
/// to the session is somewhere a test can call.
pub(super) fn compact_ended(
    session: &mut Session,
    budget: u64,
    done: &Result<Option<bravebot_agent::compact::Compacted>, String>,
    stopped: bool,
) {
    match done {
        Ok(Some(summary)) => {
            session.end_aside_of(summary.usage);
            session.compacted(room_won_back(&summary.usage), budget);
            session.note(t!(
                compact_done,
                summarised = summary.summarised,
                kept = summary.kept
            ));
        }
        Ok(None) => {
            session.end_aside(0);
            session.note(t!(compact_nothing_to_do));
        }
        Err(_) if stopped => {
            session.end_aside(0);
            session.note(t!(compact_stopped));
        }
        Err(message) => {
            session.end_aside(0);
            session.note(t!(compact_failed, problem = message));
        }
    }
}

/// Ask one question beside the work, redrawing while it is answered.
///
/// The same shape as `compact_animated`, and for the same reasons: one request, off the thread
/// that owns the terminal, with the loop below drawing so a slow answer does not read as a hang.
///
/// The conversation is lent by reference and comes back unchanged, which is the whole of what
/// makes this an aside. Nothing here can push a message, so there is no path by which the question
/// or its answer reaches the exchange a later turn resumes.
///
/// A question that could not be answered leaves a line in the transcript rather than a row with
/// nothing in it. The gates it did pass are still returned, since they decided about a request
/// that really went out.
#[expect(
    clippy::too_many_arguments,
    reason = "the terminal, session and trust are exclusive borrows of the event loop's own state, which a bundle would have to hold together"
)]
pub(super) fn aside_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    conversation: &Conversation,
    trust: &mut TrustStore,
    question: &str,
    recap: bool,
    pasted: &[crate::state::AttachedImage],
    attached: &[crate::state::Attached],
    workspace: &Workspace,
) -> io::Result<Vec<Stamped>> {
    let asked = if recap {
        RECAP_LABEL.to_string()
    } else {
        question.to_string()
    };
    let (done, events) = ask_animated(
        terminal,
        session,
        config,
        conversation,
        trust,
        question,
        recap,
        None,
        pasted,
        attached,
        workspace,
    )?;
    match done {
        Ok(answered) => aside_answered(session, asked, answered),
        Err(unanswered) => aside_unanswered(session, unanswered),
    }
    Ok(events)
}

/// Why a request asked beside the work brought nothing back.
pub(super) enum Unanswered {
    /// The person stopped it (INPUT-4).
    Stopped,
    /// It failed, in a sentence a person can be told.
    Failed(String),
}

/// What a request asked beside the work is writing, where it is neither a question nor a recap.
#[derive(Clone, Copy)]
pub(super) enum Writing<'a> {
    /// The brief `/handoff` writes, for this goal.
    Brief(&'a str),
    /// The rewrite of this draft that the enhance key asks for (INPUT-48).
    Enhancement(&'a str),
}

/// The request an aside, a handoff's brief and an enhancement share, drawn while it is answered.
///
/// Hands back what came of it and leaves what that means to the caller: a question's answer is a
/// row beside the transcript, and a brief or an enhancement goes into the box. A stop key walks
/// away from the request and comes back as [`Unanswered::Stopped`] (INPUT-4). `writing` says what
/// the request is for where it is not a question or a recap.
#[expect(
    clippy::too_many_arguments,
    reason = "the terminal, session and trust are exclusive borrows of the event loop's own state, which a bundle would have to hold together"
)]
pub(super) fn ask_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    conversation: &Conversation,
    trust: &mut TrustStore,
    question: &str,
    recap: bool,
    writing: Option<Writing<'_>>,
    pasted: &[crate::state::AttachedImage],
    attached: &[crate::state::Attached],
    workspace: &Workspace,
) -> io::Result<(
    Result<bravebot_agent::aside::Answered, Unanswered>,
    Vec<Stamped>,
)> {
    // For the reason a turn and a summary both do it: this is one request to the same backend,
    // and a sign-in is not something a worker thread can ask for.
    sign_in_if_needed(terminal, session, config)?;

    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();

    let worker_config = config.clone();
    let mut worker_trust = trust.clone();
    let worker_workspace = workspace.clone();
    let model = session.model().map(str::to_string);
    let account = session.account_session().clone();
    let cancel = Cancel::new();
    let worker_cancel = cancel.clone();
    // Taken here, before the worker starts, because that is what crosses to it: the request, not
    // the conversation. The conversation stays on this thread and is not touched again, so there
    // is no path by which an aside could add anything to it.
    //
    // The pictures go with the question, in the order the markers in it number them, the way they go
    // with a prompt: somebody asking about a screenshot beside the work is asking about the thing
    // they pasted, and pasting.md PASTE-2 is the whole of why it may be looked at.
    let asking = match writing {
        Some(Writing::Brief(goal)) => bravebot_agent::aside::Question::handoff(conversation, goal),
        Some(Writing::Enhancement(draft)) => {
            bravebot_agent::aside::Question::enhance(conversation, draft)
        }
        None if recap => bravebot_agent::aside::Question::recap(conversation),
        None => bravebot_agent::aside::Question::about(
            conversation,
            question,
            pasted
                .iter()
                .map(|image| PastedImage {
                    media_type: image.media_type,
                    bytes: image.bytes.clone(),
                })
                .collect(),
        ),
    };
    // The files dropped onto the line, as paths to read rather than as bytes: the read itself is the
    // worker's, so that it lands in the same trail as the request it travels in. Only what is carried
    // as bytes reaches here, since `crate::state::without_text_files` settled the rest to their names
    // before the action was built.
    let dropped: Vec<_> = attached
        .iter()
        .filter_map(|file| match file.kind {
            bravebot_filetype::by_name::Kind::Attachment(media) => {
                Some(bravebot_agent::turn::Attachment {
                    path: file.name.clone(),
                    media: media.to_string(),
                })
            }
            bravebot_filetype::by_name::Kind::Text => None,
        })
        .collect();
    session.begin_aside();

    let worker = thread::spawn(move || {
        let mut sink = Trail::new();
        let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main);
        let egress = Egress::new();
        // Before the question's own policy exists, which is the whole point: that one holds
        // `WebFetch` alone and reaches no workspace, so what reads a dropped file is a policy of its
        // own holding `FileRead` and nothing else. See `bravebot_agent::attached`.
        // The map is lent to the read rather than copied for it, so the rule each drop records is
        // in it afterwards: the question's own policy is built from it, and the main thread takes it
        // as the session's below. A read that failed part way still recorded rules for the files it
        // got to, so it travels back from here too.
        let asking = match bravebot_agent::attached::read(
            &worker_workspace,
            &dropped,
            &mut worker_trust,
            &mut sink,
        ) {
            Ok(carried) => asking.carrying(carried),
            // Reduced to a sentence here for the reason the answer is below: the error types are the
            // kernel's, and this thread is the only place they mean anything.
            Err(error) => return (Err(error.to_string()), sink, Some(worker_trust)),
        };
        // Reduced to what a person can be told before it crosses back, since the error types are
        // the kernel's and this thread is the only place they mean anything.
        let done = turn::aside(
            &worker_config,
            &egress,
            &account,
            asking,
            model.as_deref(),
            &mut reporter,
            &mut sink,
            worker_trust.clone(),
            // Taken and dropped. An answer has to be released to be looked at, and the one place
            // this side could draw it as it arrives is the tail the turn's own half-written reply
            // fills, which is among the turn's own lines.
            |_written| {},
            &worker_cancel,
        )
        .map_err(|e| e.to_string());
        (done, sink, Some(worker_trust))
    });

    loop {
        // Each frame, so an inhibitor that exits by itself while this runs is said and `/caffeinate`
        // turned off then, not at the next pass of the outer loop (CMD-12).
        session.keep_awake();
        redraw(terminal, session)?;

        // Input is still read for the reason a summary reads it: a long answer must not leave the
        // interface deaf, and the frame's waiting is done here so a key press wakes the loop
        // rather than queueing behind the worker.
        pump_terminal(terminal, session, |session, key| {
            // The same shape as a summary's keys, and the same function reads them.
            one_request_key(session, key, &cancel)
        })?;

        let carrying_on = drain_worker(&from_worker, Duration::ZERO, |message| {
            aside_reported(session, message);
        });

        if !carrying_on {
            break;
        }
    }

    let (done, sink, vouched) = worker.join().unwrap_or_else(|_| {
        (
            Err(t!(btw_ended_unexpectedly).to_string()),
            Trail::new(),
            None,
        )
    });

    // What the question read is not the question's to keep. Dropping a file is the grant and the
    // grant lasts the rest of the session (`dropping.md` DROP-2), so the map the read recorded its
    // rules in becomes the session's, exactly as a turn's does. Without this the read would happen
    // against a clone nothing ever looks at again, and the next prompt naming the same picture would
    // be handed a sentence about one.
    if let Some(vouched) = vouched {
        *trust = vouched;
    }

    let done = done.map_err(|message| {
        if cancel.is_cancelled() {
            Unanswered::Stopped
        } else {
            Unanswered::Failed(message)
        }
    });

    Ok((done, sink.events().to_vec()))
}

/// Draft a commit message from changes, redrawing while it is answered.
///
/// The same shape as [`ask_animated`], and for the same reasons: one request, off the thread that
/// owns the terminal, with the loop below drawing so a slow answer does not read as a hang. What
/// differs is what crosses to the worker. No conversation does, since the draft is not about the
/// exchange and nothing it makes is for it, so there is no path by which the diff or the draft
/// reaches a later turn except the line the person sends (CMD-23).
///
/// Hands back what came of it and leaves what that means to the caller. A stop key walks away from
/// the request and comes back as [`Unanswered::Stopped`] (INPUT-4).
pub(super) fn commit_message_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    trust: &TrustStore,
    asked: &crate::commit_message_command::Asked,
    workspace: &Workspace,
) -> io::Result<(
    Result<bravebot_agent::commit_message::Drafted, Unanswered>,
    Vec<Stamped>,
)> {
    sign_in_if_needed(terminal, session, config)?;

    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();

    let worker_config = config.clone();
    let worker_trust = trust.clone();
    let worker_workspace = workspace.clone();
    let asked = asked.clone();
    let model = session.model().map(str::to_string);
    let account = session.account_session().clone();
    let cancel = Cancel::new();
    let worker_cancel = cancel.clone();
    session.begin_aside();

    let worker = thread::spawn(move || {
        let mut sink = Trail::new();
        let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main);
        let egress = Egress::new();
        // Reduced to what a person can be told before it crosses back, since the error types are
        // the kernel's and this thread is the only place they mean anything.
        let done = turn::commit_message(
            &worker_config,
            &egress,
            &account,
            &worker_workspace,
            &asked.source,
            asked.format.as_deref(),
            model.as_deref(),
            &mut reporter,
            &mut sink,
            worker_trust,
            &worker_cancel,
        )
        .map_err(|e| e.to_string());
        (done, sink)
    });

    loop {
        session.keep_awake();
        redraw(terminal, session)?;
        pump_terminal(terminal, session, |session, key| {
            one_request_key(session, key, &cancel)
        })?;
        let carrying_on = drain_worker(&from_worker, Duration::ZERO, |message| {
            aside_reported(session, message);
        });
        if !carrying_on {
            break;
        }
    }

    let (done, sink) = worker
        .join()
        .unwrap_or_else(|_| (Err(t!(btw_ended_unexpectedly).to_string()), Trail::new()));
    let done = done.map_err(|message| {
        if cancel.is_cancelled() {
            Unanswered::Stopped
        } else {
            Unanswered::Failed(message)
        }
    });
    Ok((done, sink.events().to_vec()))
}

/// Take what the request for a commit message came back with.
///
/// CMD-23: a draft is put in the box and nowhere else, for the person to read and edit, and a
/// refusal or a stop leaves the box as it was. Apart from [`commit_message_animated`] so a test can
/// call it without a backend.
pub(super) fn commit_message_answered(
    session: &mut Session,
    done: Result<bravebot_agent::commit_message::Drafted, Unanswered>,
) {
    match done {
        Ok(drafted) => {
            session.end_aside_of(drafted.usage);
            session.propose_commit_message(&drafted.message);
            if let Some(written) = &drafted.substituted {
                session.note(t!(processor_model_substituted, model = written));
            }
            if drafted.partial {
                session.note(t!(commit_message_partial));
            }
            session.note(t!(commit_message_ready));
        }
        Err(Unanswered::Stopped) => {
            session.end_aside(0);
            session.note(t!(commit_message_stopped));
        }
        Err(Unanswered::Failed(message)) => {
            session.end_aside(0);
            session.note(t!(commit_message_failed, problem = message));
        }
    }
}

/// Why `/handoff` asks for nothing, as the line to tell the person, where it does not.
///
/// SESSION-34: with no goal there is nothing for the brief to be written for, and before a turn
/// has ended the session has no record for the new one to link back to. `recorded` is whether the
/// session has one. Nothing here reads the conversation, so a refusal is decided before any
/// request is made.
pub(super) fn handoff_refusal(goal: &str, turns: usize, recorded: bool) -> Option<&'static str> {
    if goal.is_empty() {
        Some(t!(handoff_needs_a_goal))
    } else if turns == 0 || !recorded {
        Some(t!(handoff_nothing_to_hand_off))
    } else {
        None
    }
}

/// Take what the request for a `/handoff` brief came back with.
///
/// SESSION-34: only a brief the planner could have held is offered. Where the exchange has met
/// something untrusted the answer is on no screen but a note, and a person reading it is not a
/// gate. Whether it could have been held is [`bravebot_agent::aside::Answered::kept`] being
/// present, which the aside's own policy decided from the labels; the brief's bytes are not
/// looked at here. Apart from [`ask_animated`] so a test can call it without a backend.
pub(super) fn handoff_answered(
    session: &mut Session,
    goal: &str,
    done: Result<bravebot_agent::aside::Answered, Unanswered>,
) {
    match done {
        Ok(answered) if answered.kept.is_some() => {
            session.end_aside_of(answered.usage);
            session.propose_handoff(&bravebot_agent::aside::handoff_prompt(
                &answered.shown,
                goal,
            ));
            session.note(t!(handoff_ready));
        }
        Ok(answered) => {
            session.end_aside_of(answered.usage);
            session.note(t!(handoff_untrusted));
        }
        Err(Unanswered::Stopped) => {
            session.end_aside(0);
            session.note(t!(handoff_stopped));
        }
        Err(Unanswered::Failed(message)) => {
            session.end_aside(0);
            session.note(t!(handoff_failed, problem = message));
        }
    }
}

/// Take what the request for an enhancement of `line` came back with (INPUT-48).
///
/// Only a rewrite the planner could have held is offered, as a brief is: where the exchange has met
/// something untrusted the answer is on no screen but a note, because a person reading it is not a
/// gate. Whether it could have been held is [`bravebot_agent::aside::Answered::kept`] being
/// present, which the aside's own policy decided from the labels, and what the rewrite says is not
/// looked at. Only its length is: one that is empty or too long is not offered rather than cut.
/// Apart from [`ask_animated`] so a test can call it without a backend.
pub(super) fn enhance_answered(
    session: &mut Session,
    line: &str,
    done: Result<bravebot_agent::aside::Answered, Unanswered>,
) {
    match done {
        Ok(answered) => {
            session.end_aside(answered.usage.total());
            if answered.kept.is_none() {
                session.note(t!(enhance_untrusted));
            } else if let Some(text) = session.clean_enhancement(&answered.shown) {
                if session.propose_enhancement(line, text) {
                    // Escape is vi's own in vi's style, so the line is put back with Ctrl-C there.
                    session.note(match session.editing() {
                        crate::vim::Editing::Ordinary => t!(enhance_ready),
                        crate::vim::Editing::Vi => t!(enhance_ready_vi),
                    });
                } else {
                    session.note(t!(enhance_line_changed));
                }
            } else {
                session.note(t!(
                    enhance_unusable,
                    limit = bravebot_agent::aside::ENHANCE_LIMIT
                ));
            }
        }
        Err(Unanswered::Stopped) => {
            session.end_aside(0);
            session.note(t!(enhance_stopped));
        }
        Err(Unanswered::Failed(message)) => {
            session.end_aside(0);
            session.note(t!(enhance_failed, problem = message));
        }
    }
}

/// Take what a question asked beside the work came back without.
///
/// Apart from [`aside_animated`] for the reason [`aside_answered`] is.
pub(super) fn aside_unanswered(session: &mut Session, why: Unanswered) {
    session.end_aside(0);
    match why {
        Unanswered::Stopped => session.note(t!(btw_stopped)),
        Unanswered::Failed(message) => session.note(t!(btw_failed, problem = message)),
    }
}

/// Take what a question asked beside the work came back with.
///
/// WATCH-18: the row and the view are where an aside is read, and neither half of it is anywhere
/// else. Apart from [`aside_animated`], which needs a terminal and a backend to reach, so that
/// what an answer does to the session is somewhere a test can call.
pub(super) fn aside_answered(
    session: &mut Session,
    asked: String,
    answered: bravebot_agent::aside::Answered,
) {
    session.end_aside_of(answered.usage);
    // The row and the view are the whole of it, and nothing joins the transcript. A line there
    // about an exchange the planner has read no part of is one a reader takes it to have had, and
    // what says afterwards that an aside happened is the hint line, which counts the rows and
    // names the key that opens them.
    session.asked_aside(bravebot_session::sessions::Aside {
        question: asked,
        answer: Some(answered.shown),
        kept: answered.kept.is_some(),
    });
}

/// Take what the thread answering a question beside the work says while it works.
///
/// Apart from [`aside_animated`] for the reason [`aside_answered`] is, and under the same clause:
/// an answer arriving as it is written is half of an aside, and the one place this side could draw
/// it is the tail where the planner's own half-written reply goes.
pub(super) fn aside_reported(session: &mut Session, message: crate::remote_confirm::ToMain) {
    match message {
        crate::remote_confirm::ToMain::Phase(phase) => session.set_phase(phase),
        crate::remote_confirm::ToMain::Notice(text) => session.note_once(text),
        _ => {}
    }
}

/// Interpret a key press while the goal check is out.
///
/// Read against [`stops_the_turn`] first, for the reason a summary reads it, and here the mode is
/// the likelier of the two to be open: nothing closes a view when a turn ends, so one opened while
/// the turn ran is still standing over the session when the goal goes to the judge.
///
/// Past that, both keys stop the check and the goal behind it with it (INPUT-4, GOAL-10). The goal
/// is the thing a person watching this actually wants off: without that, the key pressed at the
/// ninth round would stop only the request and the tenth would go out anyway. With the goal
/// already off there is only the request left to stop, and no press here leaves.
pub(super) fn goal_check_key(session: &mut Session, key: KeyEvent, cancel: &Cancel) -> Action {
    // Presses only, as in [`one_request_key`]: the release of the press that closed a mode arrives
    // with nothing open and would clear the goal the press was never about.
    if key.kind == KeyEventKind::Release {
        return Action::None;
    }

    if !stops_the_turn(session, key) {
        let action = handle_key_while_working(session, key);
        return act_while_working(session, action, crate::clipboard::paste);
    }

    if session.goal().is_some() {
        session.clear_goal();
    }
    stop_what_is_running(session, cancel);
    Action::None
}

/// Put the session's stopping condition to a judge, and give back the prompt that carries the work
/// on where it is not met yet.
///
/// `None` whenever nothing should happen: no goal is set, or the turn that just ended produced no
/// answer to judge. A turn that failed leaves the goal armed and is not judged, because a request
/// that never came back says nothing about whether the work is finished; the next turn is judged
/// instead.
///
/// The same shape as `aside_animated`, and for the same reasons: one request, off the thread that
/// owns the terminal, with the loop below drawing so a slow answer does not read as a hang. The
/// conversation is lent by reference and comes back unchanged. What goes back into it is the
/// driver's own sentence, sent as an ordinary prompt by the caller, so the exchange only ever
/// grows the way it grows for anything else.
pub(super) fn goal_check_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    conversation: &Conversation,
    trust: &TrustStore,
) -> io::Result<(Option<String>, Vec<Stamped>)> {
    if session.goal_in_force().is_none() {
        return Ok((None, Vec::new()));
    }
    // Read rather than assumed: `begin_aside` below sets the session working again, and after that
    // there is no telling a turn that answered from one that did not.
    if session.finished.is_none_or(|turn| turn.ending.unanswered()) {
        return Ok((None, Vec::new()));
    }
    // An invariant of the branch above, not a runtime condition.
    // nosemgrep: trailofbits.rs.panic-in-function-returning-result.panic-in-function-returning-result
    let condition = session
        .goal_in_force()
        .expect("the goal was there a moment ago")
        .condition()
        .to_string();

    // For the reason a turn and an aside both do it: this is one request to the same backend, and
    // a sign-in is not something a worker thread can ask for.
    sign_in_if_needed(terminal, session, config)?;

    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();

    let worker_config = config.clone();
    let worker_trust = trust.clone();
    let model = session.model().map(str::to_string);
    let account = session.account_session().clone();
    // Taken here, before the worker starts, because that is what crosses to it: the request, not
    // the conversation. The conversation stays on this thread and is not touched again.
    let check = bravebot_agent::goal::Check::of(conversation, &condition);
    let cancel = Cancel::new();
    let worker_cancel = cancel.clone();

    session.begin_aside();

    let worker = thread::spawn(move || {
        check_the_goal_on_a_worker(
            &worker_config,
            &account,
            check,
            model.as_deref(),
            worker_trust,
            &worker_cancel,
            to_main,
        )
    });

    loop {
        // Each frame, so an inhibitor that exits by itself while this runs is said and `/caffeinate`
        // turned off then, not at the next pass of the outer loop (CMD-12).
        session.keep_awake();
        redraw(terminal, session)?;

        // Input is still read for the reason an aside reads it: a slow answer must not leave the
        // interface deaf, and the frame's waiting is done here so a key press wakes the loop
        // rather than queueing behind the worker.
        pump_terminal(terminal, session, |session, key| {
            // Which of the goal, a mode over the session, and the request a stop key is asking
            // about is that function's to say.
            goal_check_key(session, key, &cancel)
        })?;

        let carrying_on = drain_worker(&from_worker, Duration::ZERO, |message| match message {
            crate::remote_confirm::ToMain::Phase(phase) => session.set_phase(phase),
            crate::remote_confirm::ToMain::Notice(text) => session.note_once(text),
            _ => {}
        });

        if !carrying_on {
            break;
        }
    }

    let (done, sink) = worker
        .join()
        .unwrap_or_else(|_| (Err(t!(goal_ended_unexpectedly).to_string()), Trail::new()));

    Ok((
        goal_checked(session, done, cancel.is_cancelled()),
        sink.events().to_vec(),
    ))
}

/// The goal check, as the thread that runs it does it.
///
/// Apart from [`goal_check_animated`] so a test can run what the thread runs without a terminal. The
/// failure is reduced to the sentence a person is told before it crosses back, since the error types
/// are the kernel's and this thread is the only place they mean anything: the sentence of the ChatGPT
/// account's failure where it was that account's, and the category's name otherwise.
pub(super) fn check_the_goal_on_a_worker(
    config: &Config,
    account: &bravebot_agent::chatgpt::AccountSession,
    check: bravebot_agent::goal::Check,
    model: Option<&str>,
    trust: TrustStore,
    cancel: &Cancel,
    to_main: mpsc::Sender<crate::remote_confirm::ToMain>,
) -> (Result<bravebot_agent::goal::Assessed, String>, Trail) {
    let mut sink = Trail::new();
    let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main);
    let egress = Egress::new();
    let done = turn::goal(
        config,
        &egress,
        account,
        check,
        model,
        &mut reporter,
        &mut sink,
        trust,
        cancel,
    )
    .map_err(|e| e.to_string());
    (done, sink)
}

/// Take what the goal check came back with, and give back the prompt that carries the work on
/// where there is one.
///
/// A check that failed after a stop key was pressed says nothing about the goal: the key took it
/// off, so the failure is the request being walked away from and is neither reported nor held
/// against anything. One that failed with no stop pressed is the goal's own failure, and a verdict
/// that arrives anyway is [`Session::goal_judged`]'s to drop. Apart from [`goal_check_animated`],
/// which needs a terminal and a backend to reach, so that what the end does to the session is
/// somewhere a test can call.
pub(super) fn goal_checked(
    session: &mut Session,
    done: Result<bravebot_agent::goal::Assessed, String>,
    stopped: bool,
) -> Option<String> {
    session.end_aside_of(
        done.as_ref()
            .map_or_else(|_| Default::default(), |assessed| assessed.usage),
    );
    match done {
        Err(_) if stopped => None,
        done => session.goal_judged(done.map(|assessed| assessed.verdict)),
    }
}
