//! Running a turn or a manifest on a worker and folding what it returns into the session: the task a
//! turn is built with, what it left, and how a stop or a failure is recorded.

use super::{
    AGENT_COMMAND, Action, AskedAbout, Beside, CLEAR_COMMAND, Cancel, Config, Conversation,
    CrosstermBackend, Duration, EXIT_COMMAND, Egress, GOAL_COMMAND, Instant, KeyEventKind,
    LanguageServers, PastedImage, Path, Permissions, RunSettings, Session, Stamped, Task, Terminal,
    Trail, TrustStore, TrustedPrograms, Workspace, act_while_working, argument_to,
    begin_manifest_step, carry_out_held_pick, drain_worker, drain_worker_until_stopped, edit_body,
    end_manifest_step, handle_key_while_working, io, mark_title, mpsc, note_a_refused_level,
    pump_terminal, redraw, refusal, settle_the_turn, sign_in_if_needed, stop_what_is_running,
    stops_the_turn, t, thread, title_state_during, turn, turn_key,
};

/// Plan one task in full, put the frozen plan to the person, and walk it, redrawing throughout.
///
/// The other kind of run, started from a session rather than from the command line. Between
/// [`run_turn_animated`] and [`aside_animated`] in what it needs: it asks about a plan and about
/// every write a step reaches, so it has the turn's answer channel, and it gives the conversation
/// nothing, so it takes none.
///
/// **The session is blocked for the duration.** A manifest run is one run with a frozen plan, and
/// there is no planner in it to hand a line to: the person cannot interleave a turn with it, and
/// the loop below refuses to send. What they can still do is everything the turn loop lets them do
/// while a turn runs, which is read the transcript, edit the box, and stop what is in flight.
///
/// **The conversation is neither read nor written.** It is not lent here at all, which is stronger
/// than an aside's promise to hand it back unchanged: the planner that reads the task is a fresh
/// one whose context holds the task string, the pictures pasted into it, and the driver's own words
/// (MANIFEST-1), so sending the conversation would break the gate rather than merely widen it, and
/// a step's result is quarantined (MANIFEST-8) so there is nothing it could give back. What the
/// transcript shows is the goal as the planner understood it, the frozen plan, each step as it runs,
/// and the reply, all of it released for a screen and none of it in the exchange a later turn
/// resumes.
///
/// **The run is written down as its own record**, by the same function the command line uses, and
/// the session notes the id. That keeps the presence of `manifest` in a record the thing that makes
/// it a manifest run: this session's own record stays a conversation, so it still resumes, and the
/// run's record still has no conversation, so the picker still refuses it for the right reason.
///
/// The trust map is lent, and the session keeps the file decisions the run ends with: the
/// attachment grants and any downgrade a completed write recorded, whether the run finished, failed
/// or was stopped ([`crate::manifest_run::join`]). A grant a step was given for a program belongs to
/// the run and travels into the run's own record; folding it into the session would leave the
/// conversation holding a yes that was given about a plan it never saw.
#[expect(
    clippy::too_many_arguments,
    reason = "the terminal, session and trust are exclusive borrows of the event loop's own state, which a bundle would have to hold together"
)]
pub(super) fn manifest_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &Config,
    workspace: &Workspace,
    task: &str,
    pasted: &[crate::state::AttachedImage],
    attached: &[crate::state::Attached],
    trust: &mut TrustStore,
    permissions: &Permissions,
    run: &RunSettings,
) -> io::Result<Vec<Stamped>> {
    // For the reason a turn does it: a sign-in needs the terminal, and this is the thread that has
    // it. Left to the worker, the URL and code the AWS CLI prints would land nowhere anyone reads.
    sign_in_if_needed(terminal, session, config)?;

    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();
    let (answer_tx, answer_rx) = mpsc::channel::<crate::remote_confirm::Reply>();

    // A fresh token, as a turn takes: reusing one could stop a run before it started.
    let cancel = Cancel::new();
    let worker_cancel = cancel.clone();

    let worker_config = config.clone();
    let worker_workspace = workspace.clone();
    let worker_trust = trust.clone();
    // The session's own handle, so a key pressed while the run walks applies to the rest of it
    // (MODE-8).
    let permission_mode = session.live_permission_mode();
    // No files: a file named with `@` is context, and this mode fixes its plan before it observes
    // anything, which is the same reason a pipe is refused (MANIFEST-9).
    //
    // A pasted picture goes in. It is not observed context: the person pressed Ctrl-V on their own
    // clipboard, so it arrives on the footing of the task they typed it into (PASTE-2) and says
    // nothing about what anybody has read (PASTE-5). MANIFEST-1 names it for that reason, and a plan
    // asked for from a screenshot is still a plan fixed before anything was looked at.
    //
    // A dropped picture goes in beside it, on the same footing and for the same reason: the person
    // dragged that file onto the window and let the line go, so its path was fixed by their gesture
    // before any request went out (DROP-2). It is read before the planner's policy exists, which is
    // what keeps a planner that cannot read from having read something.
    let mut worker_task = run.task(task, session, permissions);
    // In the order the markers in the task number them, for the reason a turn's are: a planner
    // reading "[Image #2]" has to be able to count to the picture that answers it.
    for image in pasted {
        worker_task = worker_task.with_image(PastedImage {
            media_type: image.media_type,
            bytes: image.bytes.clone(),
        });
    }
    // Only what is carried as bytes, since `crate::state::without_text_files` settled the rest to their
    // names before the action was built: what is left here is a picture or a PDF, and the run reads
    // it before it plans.
    for file in attached {
        if let bravebot_filetype::by_name::Kind::Attachment(media) = file.kind {
            worker_task = worker_task.with_attachment(file.name.clone(), media);
        }
    }
    // Nothing is said about the standing form of a write answer, so nothing offers it. A plan has
    // no standing answer at all (MANIFEST-10), and a run whose steps were fixed before anything was
    // read is the worst place to record one: the key would be pressed about a step in a plan that
    // is written afresh for every run.
    let asked = task.to_string();

    session.begin_aside();
    session.note(t!(manifest_began));

    let worker = thread::spawn(move || {
        let mut worker_trust = worker_trust;
        let mut sink = Trail::new();
        let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main.clone());
        // A queue of its own, and empty. A line typed while a run walks is not something the run
        // can take: there is no planner in it to hand one to, so the queue the session holds stays
        // the session's and the next turn sends it.
        let mut asking = crate::remote_confirm::RemoteConfirmer::new(
            to_main,
            answer_rx,
            crate::remote_confirm::Interjections::new(),
        );
        // Screening off the task, which asks for none: a plan is fixed before anything is read, so
        // nothing fills in a verdict here and a confirmer told to screen would refuse on a word
        // nobody made.
        let mut confirmer =
            bravebot_agent::Confining::new(&mut asking, permission_mode, worker_task.auto_vetting);
        let egress = Egress::new();
        let outcome = bravebot_agent::manifest::run_recording(
            &worker_config,
            &egress,
            &worker_workspace,
            &worker_task,
            &mut confirmer,
            &mut reporter,
            &mut sink,
            &mut worker_trust,
            &worker_cancel,
        );
        (outcome, sink, worker_trust)
    });

    let mut run = crate::manifest_run::Run::default();
    loop {
        // Each frame, so an inhibitor that exits by itself while this runs is said and `/caffeinate`
        // turned off then, not at the next pass of the outer loop (CMD-12).
        session.keep_awake();
        redraw(terminal, session)?;

        // Read here rather than in the outer loop, which is blocked for the duration, and for the
        // reason a turn reads it here: a run that walks for ten minutes must not leave the
        // interface deaf, and the frame's waiting is done here so a key press wakes the loop.
        pump_terminal(terminal, session, |session, key| {
            if key.kind == KeyEventKind::Release {
                return Action::None;
            }
            // Both keys stop the run and neither leaves, exactly as in a turn. A person
            // watching a plan go wrong is asking for the plan to stop; the next press, at
            // the box, is the one that leaves.
            if stops_the_turn(session, key) {
                stop_what_is_running(session, &cancel);
                Action::None
            } else {
                let action = handle_key_while_working(session, key);
                act_while_working(session, action, crate::clipboard::paste)
            }
        })?;

        let carrying_on = drain_worker(&from_worker, Duration::ZERO, |message| {
            // Progress, with no reply to give. The goal as the planner understood it and the frozen
            // plan both arrive as narration, and each step as an activity, so the transcript of a
            // run reads the way the transcript of a turn does.
            let Some(message) = run.progress(session, message) else {
                return;
            };
            // The approvals a step reaches are drawn and block the worker on the person, so the title
            // says so for as long as they are open, as it does in a turn. The plan question marks
            // itself, and the questions this mode cannot ask are refused undrawn.
            let asks = begin_manifest_step(&message, |state| mark_title(terminal, state));
            match message {
                // The one question this mode asks that a turn does not, and the reason the session
                // prompt is worth reaching: it is drawn and scrolled rather than printed and read off
                // a line, so a plan longer than the window can be walked back through before it is
                // answered.
                crate::remote_confirm::ToMain::Manifest(request) => {
                    mark_title(terminal, crate::title::State::Waiting);
                    let answer = crate::confirm::ask_manifest(terminal, &request);
                    mark_title(terminal, crate::title::State::Working);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    // Nothing is noted on the transcript, for the reason a turn notes nothing: the
                    // answer covers this plan and no other, so there is no standing decision to
                    // record, and the plan is about to be walked in the open where the transcript
                    // shows every step of it.
                    let _ =
                        answer_tx.send(crate::remote_confirm::Reply::Manifest(answer.decision()));
                }
                // Approving the plan was not approving its writes, so each one is still put to the
                // person as its step reaches it.
                crate::remote_confirm::ToMain::Write(request) => {
                    let answer = crate::confirm::ask(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Write(answer.decision()));
                }
                crate::remote_confirm::ToMain::Fetch(request) => {
                    let answer = crate::confirm::ask_fetch(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Fetch(answer.decision()));
                }
                crate::remote_confirm::ToMain::Vouch(request) => {
                    let answer = crate::confirm::ask_vouch(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Vouch(answer.decision()));
                }
                crate::remote_confirm::ToMain::Exposure(request) => {
                    let answer = crate::confirm::ask_exposure(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    let _ =
                        answer_tx.send(crate::remote_confirm::Reply::Exposure(answer.decision()));
                }
                // The questions a turn asks that this mode cannot. There is no shell and no `run` in
                // the schema (MANIFEST-5), so no pipeline is proposed and no output is read back;
                // there is no step that asks to be shown a slot; and there is no planner left to pose
                // a question. None of the four can arrive, and each
                // of them is a question the worker is *blocked* on, so silence here would be a hang
                // nothing can break: the loop would go round forever with the worker waiting on a
                // reply and the cancel token never looked at. Answered the way every other failure to
                // carry a question is answered, with the negative one.
                // What is left announces rather than asks, so nothing waits on it. The manifest is the
                // task list, so no list changes; there is no planner to delegate or to be interjected
                // at; and a run's steps report through `Started` and `Finished` above.
                other => {
                    if let Some(reply) = refusal(&other) {
                        let _ = answer_tx.send(reply);
                    }
                }
            }
            end_manifest_step(asks, |state| mark_title(terminal, state));
        });

        if !carrying_on {
            break;
        }
    }

    // What the run decided about the tree is the session's, the way a turn's is. A file dropped on
    // the line was vouched for by the gesture that put it there, and `dropping.md` DROP-2 has that
    // rule hold for the rest of the session rather than for the run; the rules the run's own writes
    // recorded belong to the same tree the next turn reads.
    let (outcome, sink) = crate::manifest_run::join(worker, trust);

    let stopped_by_the_person = was_stopped(&outcome, &cancel);

    // The other place a level goes out, so the other place one can be refused. A run carries the
    // session's chosen level the way a turn does, and the session it comes back to is the one that
    // reports whether a level is in force.
    let refused = bravebot_agent::backend::refused_a_level(
        config,
        session.model().unwrap_or(&config.default_model),
    );
    note_a_refused_level(session, refused);

    let recorded = run.complete(session, workspace.root(), &asked, &outcome, &cancel);

    say_how_a_manifest_run_ended(session, &outcome, stopped_by_the_person);
    // Last, so it is the line under the reply or under what went wrong. The run's own record is
    // where the plan, the proposal and the steps are kept in full, and the id is the only way in:
    // the picker marks a run's row but refuses Enter on it, so naming the id on the command line is
    // what reads one back, and a person who never saw this line has no way to ask for it.
    if let Some(id) = recorded {
        session.note(t!(manifest_recorded, id = &id));
    }

    Ok(sink.events().to_vec())
}

/// Whether a manifest run ended because the person stopped it.
///
/// Read off the token they set rather than off the error that came back, and that is the whole of
/// why this is a function. The two keys that stop a run reach it as two different things: pressed at
/// the plan prompt they are answered as a decline, so the run reports a plan nobody approved, and
/// pressed a step later they are a cancellation. Matching on `Cancelled` would therefore make the
/// same key mean two things depending on the moment it was pressed, and the person who pressed it
/// asked for the same thing at both.
///
/// A run that finished is never this, however late the key arrived: the work is done, and throwing
/// away the record of a run that completed would lose what it did.
pub(super) fn was_stopped(
    outcome: &Result<bravebot_agent::Outcome, bravebot_agent::TurnError>,
    cancel: &Cancel,
) -> bool {
    outcome.is_err() && cancel.is_cancelled()
}

/// Whether a queued command line ends the goal, or sets, pauses or clears one: `/exit`, `/clear`
/// and `/goal` in any form.
pub(super) fn ends_or_changes_a_goal(line: &str) -> bool {
    let line = line.trim();
    line == EXIT_COMMAND || line == CLEAR_COMMAND || argument_to(line, GOAL_COMMAND).is_some()
}

/// What goes out once a turn has ended, and whose line it is, with `judge` being what puts the goal
/// to a check.
///
/// Nothing waiting goes out after somebody has asked to leave. A prompt the person queued goes
/// before the goal is judged: their own prompts are the session, and a condition judged before they
/// have been sent would be judged against an exchange that is missing them. A command the person
/// queued that ends or changes the goal is carried out before it is judged too (GOAL-10). The
/// sentence a goal carries the work on with is this program's, so a `@path` inside it names no file.
pub(super) fn next_after_a_turn(
    session: &mut Session,
    judge: impl FnOnce(&mut Session) -> io::Result<Option<String>>,
) -> io::Result<Option<(String, Wrote)>> {
    if session.is_quitting() {
        return Ok(None);
    }
    if let Some(queued) = session.send_queued() {
        return Ok(Some((queued, Wrote::ThePerson)));
    }
    // The outer loop carries out what is left of the queue, and a command among it that ends or
    // changes the goal has to run before the goal sends the work back, or it waits on up to ten
    // more rounds. Any other command leaves the goal as it was, so it is judged as usual.
    if session.commands_waiting().any(ends_or_changes_a_goal) {
        return Ok(None);
    }
    Ok(judge(session)?.map(|prompt| (prompt, Wrote::TheDriver)))
}

/// What the transcript says of how a manifest run ended: its reply, nothing for a run the person
/// stopped, and for a failure the sentence a person is told, which is the ChatGPT account's own
/// sentence where it was that account's planner that failed and the category's name otherwise.
///
/// Apart from the loop that walks the run, which needs a terminal, so a test can say what a run
/// ended with.
pub(super) fn say_how_a_manifest_run_ended(
    session: &mut Session,
    outcome: &Result<bravebot_agent::Outcome, bravebot_agent::TurnError>,
    stopped_by_the_person: bool,
) {
    match outcome {
        Ok(finished) => {
            // Released for a screen, exactly as a turn's reply is, and into the transcript rather
            // than into the conversation: what this run said is not something a later turn holds.
            // Nothing is said about that here. A reply that arrived is the run having worked, and a
            // line under it announcing what did not happen reads as a complaint about the run.
            session.narrate(finished.reply_for_display());
        }
        // Stopped by the person, so there is nothing to report. The transcript already holds the
        // steps that ran, and a complaint about a run they turned off themselves is noise.
        Err(_) if stopped_by_the_person => {}
        Err(failure) => {
            session.note(t!(manifest_failed, problem = failure.to_string()));
        }
    }
}

/// The record of lines remembered past a session for this workspace, and what it holds now.
///
/// `None` on a machine that names no state directory, which is a session with nothing to report.
///
/// The reading directory rather than the writable one: a session that adds nothing to
/// `~/.bravebot` still honours what an earlier one recorded, so it is still carrying those answers
/// and still has to be able to read them back.
///
/// Read rather than held, and keyed by the workspace root, which is the tree a person answered
/// about: what `make check` does depends on the tree it runs in.
pub(super) fn remembered_record(
    workspace: &Workspace,
) -> Option<(
    bravebot_agent::remembered::Store,
    bravebot_core::remembered::Remembered,
)> {
    let store = bravebot_agent::remembered::Store::new(
        &bravebot_agent::home::directory()?,
        workspace.root(),
    );
    let lines = store.read();
    Some((store, lines))
}

/// What the session gets back from a turn and carries into the next one.
///
/// A struct rather than a tuple because every field is something the session owns and lends for
/// the length of one turn, and a caller taking six positional values back has no way of saying
/// which is which.
pub(super) struct Continued {
    pub(super) conversation: Conversation,
    pub(super) trust: TrustStore,
    pub(super) programs: TrustedPrograms,
    pub(super) servers: Option<LanguageServers>,
    pub(super) asked_about: AskedAbout,
    pub(super) exposed: bravebot_core::credentials::Exposed,
    pub(super) events: Vec<Stamped>,
}

/// What the joined worker returns, including an ordinary error or cancellation.
pub(super) struct FinishedTurn {
    pub(super) decisions: Option<turn::Decisions>,
    pub(super) outcome: Result<turn::Outcome, turn::TurnError>,
    pub(super) conversation: Conversation,
    pub(super) sink: Trail,
    pub(super) servers: Option<LanguageServers>,
}

/// What the session keeps of a turn's inputs while the worker holds them: the decisions, for a
/// worker that cannot return an outcome, and where the conversation stood, for a stop that takes
/// the prompt back.
pub(super) struct RetainedTurn {
    pub(super) files: bravebot_core::file_authority::FileAuthority,
    pub(super) programs: TrustedPrograms,
    pub(super) asked: AskedAbout,
    pub(super) exposed: bravebot_core::credentials::Exposed,
    pub(super) began: bravebot_agent::conversation::Mark,
}

/// Run a turn on a worker thread, redrawing while it works.
///
/// The turn itself blocks on network requests, so running it here would freeze the indicator on
/// its first frame and make a slow model look like a hang. Off-thread, the loop below keeps
/// drawing, and the elapsed time and spinner advance on their own.
///
/// Write approvals come back over a channel because only this thread owns the terminal. The
/// worker blocks until an answer arrives, which is what a write must wait for anyway.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the terminal and each piece of session state a turn takes and hands back"
)]
pub(super) fn run_turn_animated(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &mut Config,
    workspace: &Workspace,
    scratch: Option<&std::path::Path>,
    prompt: &str,
    wrote: Wrote,
    conversation: Conversation,
    trust: TrustStore,
    programs: TrustedPrograms,
    // The servers this session has started, LSP-8, or `None` before the first turn has had the
    // chance to start one. Owned here and handed back like the conversation, because it is the
    // session that keeps it: built inside a turn it would be shut down at the end of that turn,
    // and the next message would ask the same person about the same language.
    servers: Option<LanguageServers>,
    asked_about: AskedAbout,
    exposed: bravebot_core::credentials::Exposed,
    permissions: &Permissions,
    run: &RunSettings,
    // The session's record. Its id travels with the task because a run prompt may be answered with
    // the key whose grant outlives the session, and the record of that says which session pressed
    // it so that `/status` can tell a person which answers they are still carrying. The record
    // itself stays here, for a `/rename` typed while the turn runs.
    stored: &mut bravebot_session::sessions::Handle,
) -> io::Result<Continued> {
    // The prompt is in the transcript by now, and drawn before anything that might take a moment:
    // a check that has to run the AWS CLI holds the frame for as long as the process takes, and
    // until this the line somebody typed is nowhere on their screen.
    redraw(terminal, session)?;

    // Taken, so only the turn a person's `/agent` line started carries that line's name. Every
    // other turn carries the definition the session was started under, if any (CLI-17). Kept past
    // the turn as well, because a stop puts the line back as it was typed, name and all.
    let addressed = session.take_addressing();

    // Before the worker starts, because a sign-in needs the terminal and this is the thread that
    // has it. Left to the worker, the URL and code the AWS CLI prints would land in a frame this
    // loop redraws over.
    //
    // Not for a turn whose definition named its own model: nothing in it asks the session's, and
    // `/agent` and `--agent` have already refused a definition whose model needs a sign-in.
    if addressed
        .as_ref()
        .is_none_or(|addressed| addressed.model.is_none())
    {
        sign_in_if_needed(terminal, session, config)?;
    }

    // One channel for everything the worker sends, because the main thread waits on exactly one
    // thing and `mpsc` cannot select across two. Only a write expects a reply.
    let (to_main, from_worker) = mpsc::channel::<crate::remote_confirm::ToMain>();
    let (answer_tx, answer_rx) = mpsc::channel::<crate::remote_confirm::Reply>();
    // Prompts typed while this turn runs, going the other way. Shared rather than sent, so that
    // taking one back off the queue takes it out of the turn's reach too.
    let typed = session.interjections();

    // A fresh token per turn: reusing one could cancel a turn before it started.
    let cancel = Cancel::new();
    let worker_cancel = cancel.clone();

    // Cloned rather than borrowed so the worker owns everything it needs. Config and Workspace
    // are cheap handles; Egress builds its own connection pool.
    let worker_config = config.clone();
    // The model a `/agent` line's definition names answers this turn, so its window is the budget
    // the turn compacts against (BACKEND-7). Taken in the worker's copy, so the session's own
    // budget is what it was once the turn is over, and in the worker, so the listing it may ask
    // for does not hold the screen.
    let addressed_model = addressed
        .as_ref()
        .and_then(|addressed| addressed.model.clone());
    let worker_workspace = workspace.clone();
    // No round limit: the person reading this screen is the bound, and a stop reaches a turn
    // mid-round. A number here would only interrupt work that was going fine.
    // Which tick of a loop this is, where it is one at all. A prompt the person typed in the
    // middle of a loop is not a tick of it and carries nothing.
    let tick = session
        .loops()
        .in_flight()
        .and_then(|running| running.tick());
    // And what the session is working towards, where a person set a condition. Every turn under a
    // goal carries it, the first one included: the round that sets the direction is the one that
    // most needs to know what it is aiming at.
    // A goal the person has held still is not aimed at: the turn is told nothing about it.
    let working_towards = session
        .goal_in_force()
        .map(|goal| goal.condition().to_string());
    // And whether this turn may arm a standing watch. Read here for the reason the mode is read
    // here: only the session can count what is live, and a tool answering out of a stale count
    // would tell the planner about a watch the session then refused.
    let arming = session.arming();
    // And whether a wait this turn asks for would become a later look at this line, which decides
    // whether it is offered a way to ask for one. Read here for the reason `arming` is: only the
    // session knows what it is already doing, and a tool answering out of a stale reading would
    // tell the planner a watch exists that the session then declines to keep.
    let looking_again = will_look_again(session, wrote);
    // The session's own handle rather than a copy of its value: the person may press the key while
    // this turn runs, and the planner, the tools and the confirmer all read what they pressed last
    // (MODE-8).
    let permission_mode = session.live_permission_mode();
    // The worker starts the language servers, and the info panel on this thread names them.
    let language_servers = session.language_servers().clone();
    let kept_jobs = session.kept_jobs().cloned();
    let mut task = run
        .task(prompt, session, permissions)
        .with_rounds(None)
        // There is somebody in front of this, so a run prompt here may offer the key whose answer
        // outlives the session. A one-shot run says nothing here and reads no record.
        .remembering(Some(stored.id().to_string()))
        .addressing(addressed.as_ref().map(|addressed| addressed.name.clone()))
        // The words `--system-prompt` and `--append-system-prompt` named, on every turn the session
        // sends, ticks and goal rounds included (CLI-19).
        .with_system_prompts(session.system_prompts().clone())
        // Whether a check that finds nothing answers in the person's place. Read off the session
        // for the reason the mode is: the `a` key can change it, and a turn keeps the answer it
        // began with.
        .with_auto_vetting(session.auto_vetting())
        .ticking(tick)
        .arming(arming)
        .looking_again(looking_again)
        // And the files this session has already agreed the planner may be given, so a planner
        // reading the same `.env` on turn after turn is asked about it once.
        .already_exposed(exposed.clone())
        .working_towards(working_towards)
        // The servers the session started, with a grant naming each. Their tools are offered once
        // a person has read the list, which the turn puts to them before it plans (SERVERS-8).
        .with_mcp(session.servers.session.clone());
    // Every file named with `@` becomes context, which a turn treats as trusted: the user typed the
    // path and their keystroke is what vouches for it, exactly as `--file` does on the command
    // line. Read back out of the prompt rather than tracked while it is typed, so the line that was
    // sent and the files that came with it cannot disagree.
    for file in files_named_in(
        prompt,
        wrote,
        workspace.root(),
        &workspace.reference_sources(),
    ) {
        task = task.with_file(file);
    }
    if session.take_driver_prompt() || wrote == Wrote::TheDriver {
        task = task.written_by_the_driver();
    }
    task = with_firing_watch(task, session);
    // The jobs the session keeps, so one started here is still running when the next turn begins
    // (RUN-15).
    if let Some(kept) = kept_jobs {
        task = task.keeping_jobs(kept);
    }
    task = with_named_sessions(task, session, workspace.root(), prompt, wrote);
    task = with_mcp_prompt(task, session, prompt, wrote);
    task = with_submitted_attachments(task, session);
    task = with_session_advisor(task, session);
    task = with_session_style(task, session);
    task = with_session_skill_mode(task, session);
    task = with_session_limit(task, session);
    // The worker shares file decisions so errors cannot return the pre-write map.
    let file_authority = bravebot_core::file_authority::FileAuthority::new(trust.clone());
    let task = task.with_file_authority(file_authority.clone());
    let retained = RetainedTurn {
        files: file_authority,
        programs: programs.clone(),
        asked: asked_about.clone(),
        exposed: exposed.clone(),
        began: conversation.mark(),
    };

    let worker = thread::spawn(move || {
        let mut sink = Trail::new();
        // Two handles over one channel back to the thread that owns the terminal: one asks about
        // writes and waits, the other reports progress and moves on.
        let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main.clone());
        let mut asking = crate::remote_confirm::RemoteConfirmer::new(to_main, answer_rx, typed);
        // The mode the turn begins in is read here, once, and the window it is sized against and the
        // turn itself both follow that reading (BACKEND-56). Before the confirmer below, which takes
        // the session's handle on the mode.
        let egress = Egress::new();
        let (task, worker_config) = settle_the_turn(
            &worker_config,
            &egress,
            task,
            &permission_mode,
            addressed_model.as_deref(),
        );
        // Wrapped rather than replaced, because two of the confirmer's questions still cross back to
        // the terminal: a question the planner posed asks for information rather than consent, and an
        // interjection is the person typing unprompted.
        //
        // The mode as it was when the prompt was sent. A turn keeps the one it began with: a mode
        // changed while it runs describes the next turn, and a write already being reviewed must not
        // have the question withdrawn from under the person answering it.
        //
        // Screening is read off the task rather than the session for the same reason at one remove:
        // the tools that fill in a verdict read the task's answer, and this is what decides whether a
        // verdict that objects can refuse, so the two reading different answers would be a run that
        // screens nothing while believing it does.
        let mut confirmer =
            bravebot_agent::Confining::new(&mut asking, permission_mode, task.auto_vetting);
        // Owned by the worker for the duration and handed back afterwards, whether the turn
        // succeeded or not. A failed turn is still part of the conversation, and the next one
        // is usually about it.
        let mut conversation = conversation;
        // Built on the first turn of the session and handed back with the conversation. Nothing is
        // started by building it: LSP-8 starts a server on the first question that needs one, so a
        // session that asks about no symbol still starts no process and prompts about none.
        let mut servers = servers.unwrap_or_else(|| {
            // The task's home rather than a second look at the state directory, so an index is
            // cached where the rest of this session's state goes.
            LanguageServers::new(worker_workspace.root().to_path_buf(), task.home.clone())
                .reporting_to(language_servers)
        });
        let completed = turn::resume(
            &worker_config,
            &egress,
            &worker_workspace,
            &task,
            &mut conversation,
            &mut confirmer,
            &mut reporter,
            &mut sink,
            trust,
            programs,
            Some(&mut servers),
            &worker_cancel,
        );
        FinishedTurn {
            decisions: Some(completed.decisions),
            outcome: completed.outcome,
            conversation,
            sink,
            servers: Some(servers),
        }
    });

    let home = bravebot_agent::home::directory();
    let managed = bravebot_config::Managed::load();
    let mut beside = Beside {
        managed: &managed,
        stored,
        root: workspace.root(),
        home: home.as_deref(),
        workspace,
        scratch,
        config: &mut *config,
    };

    // Redraw until the turn finishes, answering approvals and watching for a cancel on the way.
    loop {
        // Each frame, so an inhibitor that exits by itself while this runs is said and `/caffeinate`
        // turned off then, not at the next pass of the outer loop (CMD-12).
        session.keep_awake();
        redraw(terminal, session)?;

        // Input is polled here rather than in the outer loop, which is blocked for the duration
        // of the turn. Without this the interface would take none at all while working, and a
        // long turn is exactly when someone wants to copy what has appeared so far.
        //
        // This is also where the frame's waiting is done, and that is the point of it. Blocking on
        // the worker instead left a key press sitting for up to a frame before anything looked at
        // it, so typing during a turn lagged while typing between turns did not, for no reason a
        // person could see. A keystroke wakes this the instant it arrives; the worker's messages
        // are picked up on the way round and drawn at the frame rate, which is all they need.
        pump_terminal(terminal, session, |session, key| {
            // Presses only, for the reason the outer loop ignores releases: a release taken
            // for a press would type every character twice, and cancel the turn on the way up
            // from the Escape that already cancelled it.
            if key.kind == KeyEventKind::Release {
                return Action::None;
            }
            // Both keys stop the turn and neither leaves. Ctrl-C is the way out of the
            // program, but there is a turn to stop first, and a person watching an answer
            // go wrong is asking for the answer to stop rather than for the session to
            // end. The next press, at the box, is the one that leaves.
            //
            // The status line says the turn is stopping from this press until it ends.
            // The prompt coming back to the box is the answer, but it comes only once the
            // worker and every delegate have returned, which can be seconds, and a screen
            // that stands still that long reads as a press nobody heard.
            if stops_the_turn(session, key) {
                stop_what_is_running(session, &cancel);
                Action::None
            } else {
                turn_key(session, key, &cancel, &mut beside)
            }
        })?;
        carry_out_held_pick(session, beside.config);

        // A question that arrives once the work has been stopped is refused undrawn, by the
        // wrapper below; see [`drain_worker_until_stopped`].
        let mut handle = |message: crate::remote_confirm::ToMain| {
            // What asks nothing is taken into the session, and what is left is a question.
            let Some(message) = crate::from_worker::show_in_a_turn(
                session,
                message,
                beside.config.context_budget,
                beside.config.budget_is_guessed(),
            ) else {
                return;
            };
            // Which prompt widget is open is the driver's own state, so a message that blocks the
            // worker on the person is what turns the title to waiting, and its answer turns it back.
            let asks = title_state_during(&message) == crate::title::State::Waiting;
            if asks {
                mark_title(terminal, crate::title::State::Waiting);
            }
            match message {
                crate::remote_confirm::ToMain::Write(request) => {
                    // The editor takes the terminal and gives it back, so whether the mouse was
                    // captured is read now, before the prompt holds everything it would be read
                    // from.
                    let mouse = session.mouse_captured();
                    let answer =
                        crate::confirm::ask_editing(terminal, &request, |terminal, body, path| {
                            edit_body(terminal, mouse, body, path)
                        });
                    // Ctrl-C at the prompt is the same request it is anywhere else in a turn: stop.
                    // Set before the answer goes back, so the worker sees it as soon as it wakes.
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    // A closed channel means the worker is already gone, so there is nothing to
                    // answer and the loop below will collect its result.
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Write(answer.decision()));
                }
                crate::remote_confirm::ToMain::Run(request) => {
                    let answer = crate::confirm::ask_run(terminal, &request);
                    // Ctrl-C at the prompt is the same request it is anywhere else in a turn: stop.
                    // Set before the answer goes back, so the worker sees it as soon as it wakes.
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    // What was vouched for travels back with the turn's outcome, exactly as the
                    // trust map does: the tool records it on the policy, and the policy carries it
                    // out. Recording it here as well would give the session a second copy to
                    // disagree with.
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Run(answer.decision()));
                }
                crate::remote_confirm::ToMain::ReadOutput(request) => {
                    let answer = crate::confirm::ask_output(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    // The standing key, handled as at the other vetting prompt and for the same
                    // reasons: the approval itself leaves no rule behind, and turning the mode on is
                    // the interface's own standing decision rather than the turn's, so the answer the
                    // worker is waiting for is the same either way and this takes effect next turn.
                    if answer.turns_vetting_on() {
                        session.choose_vetting(true);
                        // Said on the transcript because the person will not otherwise see it
                        // recorded anywhere, and what it changes is that later prompts do not appear.
                        session.note(t!(session_vetting_on));
                    }
                    let _ =
                        answer_tx.send(crate::remote_confirm::Reply::ReadOutput(answer.decision()));
                }
                crate::remote_confirm::ToMain::Vet(request) => {
                    let answer = crate::confirm::ask_vet(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    // The approval itself is noted nowhere: it covers the bytes that were on the
                    // screen and leaves no rule behind, so there is no standing decision to record.
                    // Turning the mode on is a standing decision, and it is the interface's own rather
                    // than the turn's: the answer the worker is waiting for is the same either way,
                    // and what this adds takes effect from the next turn.
                    if answer.turns_vetting_on() {
                        session.choose_vetting(true);
                        // Said on the transcript because the person will not otherwise see it
                        // recorded anywhere, and what it changes is that later prompts do not appear.
                        session.note(t!(session_vetting_on));
                    }
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Vet(answer.decision()));
                }
                crate::remote_confirm::ToMain::Fetch(request) => {
                    let answer = crate::confirm::ask_fetch(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    // Nothing is noted on the transcript: an approval covers this one URL and leaves
                    // no standing permission behind, so there is no decision to record.
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Fetch(answer.decision()));
                }
                crate::remote_confirm::ToMain::Vouch(request) => {
                    let answer = crate::confirm::ask_vouch(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    if answer == crate::confirm::Answer::Approve {
                        // Said on the transcript because it is a standing decision the user will not
                        // otherwise see recorded anywhere until they ask for /status.
                        session.note(t!(session_vouched_for, path = &request.path));
                    }
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Vouch(answer.decision()));
                }
                crate::remote_confirm::ToMain::Exposure(request) => {
                    let answer = crate::confirm::ask_exposure(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    if answer == crate::confirm::Answer::Approve {
                        // Said on the transcript because it lasts the session and the box it was
                        // answered in is gone: what is left otherwise is a read that looks like any
                        // other, with nothing to say a credential went with it.
                        session.note(t!(session_exposed, path = &request.path));
                    }
                    let _ =
                        answer_tx.send(crate::remote_confirm::Reply::Exposure(answer.decision()));
                }
                crate::remote_confirm::ToMain::Server(request) => {
                    let answer = crate::confirm::ask_server(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    if answer == crate::confirm::Answer::Approve {
                        // Said on the transcript for the reason vouching for a file is: it lasts the
                        // session, and a process running with the user's access is worth being able to
                        // see they agreed to.
                        session.note(t!(
                            session_started_server,
                            language = request.language,
                            program = request.program.as_str()
                        ));
                    }
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Server(answer.decision()));
                }
                crate::remote_confirm::ToMain::Manifest(request) => {
                    let answer = crate::confirm::ask_manifest(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    // Nothing is noted on the transcript. The answer covers this plan and no other, so
                    // there is no standing decision to record, and the plan itself is about to be
                    // walked in the open where the transcript will show every step of it.
                    let _ =
                        answer_tx.send(crate::remote_confirm::Reply::Manifest(answer.decision()));
                }
                crate::remote_confirm::ToMain::ToolList(request) => {
                    let answer = crate::confirm::ask_tool_list(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    if answer == crate::confirm::Answer::Approve {
                        // Said on the transcript because the planner reads these tools' descriptions
                        // from here on, in this session and the ones after it.
                        session.note(t!(
                            session_offered_tools,
                            alias = &request.alias,
                            count = request.tools.len()
                        ));
                    }
                    let _ =
                        answer_tx.send(crate::remote_confirm::Reply::ToolList(answer.decision()));
                }
                crate::remote_confirm::ToMain::McpCall(request) => {
                    let answer = crate::confirm::ask_mcp_call(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    if answer.answer == crate::confirm::CallAnswer::ApproveAndStand {
                        // The one answer at this prompt that outlasts it, so the one worth being able
                        // to find afterwards.
                        session.note(t!(session_stands_for_tool, tool = request.name()));
                    }
                    let _ =
                        answer_tx.send(crate::remote_confirm::Reply::McpCall(answer.decision()));
                }
                crate::remote_confirm::ToMain::Move(request) => {
                    let answer = crate::confirm::ask_move(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Move(answer.decision()));
                }
                crate::remote_confirm::ToMain::Path(request) => {
                    let answer = crate::confirm::ask_path(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Path(answer.decision()));
                }
                crate::remote_confirm::ToMain::Host(request) => {
                    let answer = crate::confirm::ask_host(terminal, &request);
                    if answer.stops_the_turn() {
                        stop_what_is_running(session, &cancel);
                    }
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Host(answer.decision()));
                }
                crate::remote_confirm::ToMain::Ask(asking) => {
                    let idle = session.ask_idle_for(&asking);
                    let answers = crate::ask::put(session, &asking, |outstanding| {
                        crate::ask::ask_within(terminal, outstanding, idle)
                    });
                    let _ = answer_tx.send(crate::remote_confirm::Reply::Ask(answers));
                }
                // What `show_in_a_turn` took above is not here. A question added to `ToMain` that no
                // arm draws is refused, as in a manifest run, since the worker is blocked on it.
                other => {
                    if let Some(reply) = refusal(&other) {
                        #[expect(
                            clippy::let_underscore_must_use,
                            reason = "the worker may have ended, and a refusal to nobody has no one to be told"
                        )]
                        let _ = answer_tx.send(reply);
                    }
                }
            }
            if asks {
                mark_title(terminal, crate::title::State::Working);
            }
        };
        let carrying_on = drain_worker_until_stopped(
            &from_worker,
            Duration::ZERO,
            &cancel,
            &answer_tx,
            &mut handle,
        );

        // The worker dropped its senders, so the turn is over.
        if !carrying_on {
            break;
        }
        session.give_way_to_the_queue(Instant::now());
    }

    let finished = worker.join().unwrap_or_else(|_| {
        // A panicked turn is reported rather than propagated: the session survives. The
        // conversation does not, since the thread that held it is gone, and neither do the
        // servers: they went down with the thread that owned them, so the next turn starts and
        // is asked about a fresh one.
        FinishedTurn {
            decisions: None,
            outcome: Err(turn::TurnError::Precommit(
                t!(turn_ended_unexpectedly).to_string(),
            )),
            conversation: Conversation::new(),
            sink: Trail::new(),
            servers: None,
        }
    });

    Ok(finish_turn(
        session,
        config,
        workspace,
        Line {
            text: prompt,
            addressed: addressed.as_ref(),
            offered_a_later_look: looking_again,
        },
        finished,
        retained,
    ))
}

/// Adopt decisions before classifying the joined result or handing state back for saving.
pub(super) fn finish_turn(
    session: &mut Session,
    config: &Config,
    workspace: &Workspace,
    line: Line<'_>,
    finished: FinishedTurn,
    retained: RetainedTurn,
) -> Continued {
    let FinishedTurn {
        decisions,
        outcome,
        mut conversation,
        sink,
        servers,
    } = finished;
    let carried = match decisions {
        Some(turn::Decisions {
            trust,
            programs,
            asked_about,
            exposed,
        }) => Carried {
            trust,
            programs,
            asked: asked_about,
            exposed,
        },
        // Preserve the existing unwind fallback; ordinary endings always carry current decisions.
        None => Carried {
            trust: retained.files.snapshot(),
            programs: retained.programs,
            asked: retained.asked,
            exposed: retained.exposed,
        },
    };
    // Record cancellation separately from failure, then restore the prompt when possible.
    let events = sink.events().to_vec();

    // A stopped turn may already have learned that this model refuses the chosen level. Asked of the
    // model in force now, as is the budget below, even where a model was picked while the turn ran:
    // the pick already recorded whether its model reads a level and moved the gauge to its budget,
    // and both describe the model the next turn is sent to (`Session::update_budget`).
    let refused = bravebot_agent::backend::refused_a_level(
        config,
        session.model().unwrap_or(&config.default_model),
    );
    note_a_refused_level(session, refused);

    let carried = if let Err(turn::TurnError::Cancelled { attempts }) = &outcome {
        let typed = line.as_typed(session.standing_definition());
        // A prompt that is back in the box is sent again from there, so its copy in the
        // conversation goes too: left in, every later request carries it once for each send and
        // compaction counts each as an exchange of its own. Only the turn's opening goes, and only
        // while the planner has done nothing.
        if finish_cancelled_turn(session, &typed, *attempts)
            && conversation.forget_since(retained.began)
        {
            session.prompt_withdrawn();
        }
        carried
    } else {
        // The backend decides how to compare the requested and reported model names.
        let chosen = line.model(session, config);
        let asked = Asked {
            name: bravebot_agent::backend::Backend::name_as_asked(config, &chosen),
            comparable: bravebot_agent::backend::Backend::reports_the_model_it_was_asked_for(
                config, &chosen,
            ),
        };
        fold_outcome(
            session,
            outcome,
            sink,
            carried,
            Occupied {
                budget: config.context_budget,
                guessed: config.budget_is_guessed(),
                last_request_tokens: conversation.last_request_tokens(),
            },
            asked,
            line,
            workspace,
        )
    };
    // After the turn is folded in, so the prompt a stop gives back and the trail a failure keeps
    // are decided with what commands said during the turn still set apart from it.
    session.settle_what_was_said();
    let Carried {
        trust,
        programs,
        asked,
        exposed,
    } = carried;
    Continued {
        conversation,
        trust,
        programs,
        servers,
        asked_about: asked,
        exposed,
        events,
    }
}

/// A watch's fire recorded as the watch's, from the terminal's own record of which one it was, so a
/// surface drawing the session back draws the row it draws for a fire the bridge started rather
/// than a prompt somebody typed (LAYER-6). A turn nobody's watch fired is left as typed.
pub(super) fn with_firing_watch(task: Task, session: &Session) -> Task {
    match session.firing_watch() {
        Some((number, path)) => {
            task.composed_rather_than_typed(bravebot_agent::conversation::Composed::Watch {
                number,
                path,
            })
        }
        None => task,
    }
}

/// The output style `/style` chose, carried on the turn so it opens the system prompt (CLI-19).
pub(super) fn with_session_style(task: Task, session: &Session) -> Task {
    task.with_style(session.style().cloned())
}

/// The skill `/mode` put in force, carried on the turn by name so it reads the skill again and puts
/// it in its system prompt (SKILL-21).
pub(super) fn with_session_skill_mode(task: Task, session: &Session) -> Task {
    task.with_skill_mode(session.skill_mode().map(str::to_string))
}

/// The session's spend limit and what the session had spent before this turn, carried on the turn
/// so it asks before a request that would go past the limit (TURN-8).
///
/// The limit is the session's own handle, so a `/limit` typed while the turn runs reaches it.
pub(super) fn with_session_limit(task: Task, session: &Session) -> Task {
    task.with_spend_limit(session.spend_limit().clone(), session.spent_tokens())
}

/// The model `/advisor` named, carried on the turn so the planner is offered it (ADVISOR-9).
///
/// `None` where the session named none, which leaves the turn to the `advisorModel` setting.
pub(super) fn with_session_advisor(task: Task, session: &Session) -> Task {
    task.with_advisor(session.advisor().map(str::to_string))
}

/// Carry the attachments owned by the submitted line into its turn request.
///
/// Drops precede pastes, as they do in the model request. The session has already
/// removed attachments whose markers the person deleted before submitting.
pub fn with_submitted_attachments(mut task: Task, session: &Session) -> Task {
    for attached in session.sent_attachments() {
        task = match attached.kind {
            bravebot_filetype::by_name::Kind::Attachment(media) => {
                task.with_attachment(attached.name.clone(), media)
            }
            // A text file is context, which is what `@` and `--file` already do with one. It
            // goes in as a drop rather than as a named file because a drop comes from wherever
            // the user dragged it from, and that is rarely inside the workspace.
            bravebot_filetype::by_name::Kind::Text => task.with_dropped_text(attached.name.clone()),
        };
    }
    for image in session.sent_pasted() {
        task = task.with_image(PastedImage {
            media_type: image.media_type,
            bytes: image.bytes.clone(),
        });
    }
    task
}

/// Whose line a turn is running.
///
/// A `@path` in a prompt is a person vouching for a file with their keystroke, so it is read out
/// of a line they typed and out of no other. Every prompt was one until a goal could write the
/// sentence that carries the work on: that sentence is this program's, quoting a judge, and an
/// `@` inside it names nothing and vouches for nobody.
///
/// A loop tick is the person's, because the line a tick sends is the one they typed into `/loop`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Wrote {
    /// The person, at the box, or a loop repeating the line they typed there.
    ThePerson,
    /// This program, carrying the work on towards a condition the person set.
    TheDriver,
}

/// The wait that starts a loop nobody typed `/loop` for, where there is one.
///
/// `None` where the turn asked for nothing, and where it was not offered a later look before it
/// ran ([`will_look_again`]). That refuses a line that is not the person's: a loop repeats a line
/// somebody endorsed, and the sentence this program writes to carry a goal on is not one, so a
/// turn under a goal asking for a later look would otherwise leave behind a loop sending the
/// driver's own words back every quarter of an hour.
///
/// It refuses a tick as well. A self-paced tick asks for its loop's next wait, and a `/loop stop`
/// typed while it ran leaves no loop for that wait to pace: kept, it would start a new loop on the
/// line the person had just stopped.
///
/// Split out from `fold_outcome` so it can be tested. That function's answer to a finished turn
/// needs a whole [`turn::Outcome`], and the field holding the released reply is its own crate's.
pub(super) fn watch_to_start(wakeup: Option<turn::Wakeup>, offered: bool) -> Option<turn::Wakeup> {
    wakeup.filter(|_| offered)
}

/// Whether a wait this turn asks for would become a later look at its own line.
///
/// Asked before the turn rather than after it, because it decides whether the turn is offered a
/// way to ask for one at all (SCHED-6). Where the answer is no the wait is discarded, and a tool
/// whose confirmation says a later look is arranged and needs nothing from the person would be
/// describing a watch nothing is keeping.
///
/// The same refusals [`crate::state::Session::watch_again`] makes once the turn has ended, in one
/// place ahead of it, and the answer [`watch_to_start`] reads then: the line has to be the
/// person's, which `/init`, `/review` and `/help` are not, since the person's keystroke sends a prompt this
/// program wrote; and the session has to have nothing else of its own already running, since it
/// does one of a watch, a loop and a goal at a time. A turn that is a tick of a loop is not this
/// question at all, the loop being already the thing that asks again, and the agent reads this
/// only where there is no tick.
pub(super) fn will_look_again(session: &Session, wrote: Wrote) -> bool {
    wrote == Wrote::ThePerson
        && !session.driver_prompt_is_pending()
        && session.looping().is_none()
        && session.goal().is_none()
        && session.watches().is_empty()
}

/// The files a prompt vouches for by naming them with `@`.
///
/// Out of a line the person wrote, and out of no other. The keystroke is the whole of what vouches
/// for the path, so a sentence this program wrote has nothing to vouch with: an `@` a judge
/// happened to put in a reason would otherwise open a file on its own say-so, wearing an
/// endorsement nobody gave.
pub(super) fn files_named_in(
    prompt: &str,
    wrote: Wrote,
    root: &std::path::Path,
    sources: &bravebot_mentions::Sources,
) -> Vec<String> {
    match wrote {
        // A name under a reference's alias is the file in that directory (REFER-6).
        Wrote::ThePerson => bravebot_mentions::referenced(prompt)
            .into_iter()
            .map(|name| {
                // A reference directory with no text spelling leaves the name as typed. Nothing
                // in the workspace has that name, so the turn's read refuses it by name, and no
                // other directory is read in its place.
                bravebot_mentions::resolved(root, &name, sources).unwrap_or(name)
            })
            .collect(),
        Wrote::TheDriver => Vec::new(),
    }
}

/// A row of the `@` list for each earlier session of this directory but the open one, newest first.
///
/// A manifest run and a session with no conversation yet are left out: neither has words to quote
/// (NAME-10). What the row says is what the person chooses by, and the bound is what naming it
/// would add at most.
pub(super) fn past_sessions_offered(root: &Path, current: &str) -> Vec<bravebot_mentions::Entry> {
    bravebot_session::sessions::list(root)
        .into_iter()
        .filter(|listed| listed.id != current && !listed.manifest)
        .map(|listed| {
            bravebot_mentions::session_entry(
                &listed.id,
                &listed.title,
                t!(
                    session_mention_row,
                    title = listed.title.as_str(),
                    when = bravebot_session::sessions::how_long_ago(listed.updated),
                    chars = bravebot_session::excerpt::BOUND
                ),
            )
        })
        .collect()
}

/// Carry the prompt of an MCP server a line runs, which the turn fetches (CMD-26).
///
/// From a line the person wrote and from no other, for the reason [`files_named_in`] is: the
/// keystroke is what runs it, and a line this program composed to carry a goal on is no keystroke.
/// A line typed at the box whose arguments do not parse was refused where Enter was pressed. One
/// that reaches here without parsing, a loop's prompt, goes on as the sentence it is and the person
/// is told so.
pub(super) fn with_mcp_prompt(
    task: Task,
    session: &mut Session,
    prompt: &str,
    wrote: Wrote,
) -> Task {
    if wrote == Wrote::TheDriver {
        return task;
    }
    let Some(asked) = session
        .servers
        .session
        .as_ref()
        .and_then(|servers| servers.prompt_request(prompt))
    else {
        return task;
    };
    match asked {
        Ok(request) => task.with_mcp_prompt(Some(request)),
        Err(_) => {
            session.note(t!(mcp_prompt_sent_as_text));
            task
        }
    }
}

/// Add what each past session a prompt names with `@session:<id>` said, and tell the person what
/// was and was not added.
///
/// From a line the person wrote and from no other, for the reason [`files_named_in`] is: the
/// keystroke is what vouches for it. A session that cannot be quoted is said so in the transcript
/// and the turn goes on without it, since the line is still a question worth answering.
pub(super) fn with_named_sessions(
    mut task: Task,
    session: &mut Session,
    root: &Path,
    prompt: &str,
    wrote: Wrote,
) -> Task {
    if wrote == Wrote::TheDriver {
        return task;
    }
    for id in bravebot_mentions::referenced_sessions(prompt) {
        match bravebot_session::excerpt::of(root, &id) {
            Ok(excerpt) => {
                let chars = excerpt.chars;
                session.note(if excerpt.cut {
                    t!(session_mention_added_cut, id = id.as_str(), chars = chars)
                } else {
                    t!(session_mention_added, id = id.as_str(), chars = chars)
                });
                task = task.with_session_excerpt(id, excerpt.text);
            }
            Err(refused) => session.note(refused.said(&id)),
        }
    }
    task
}

/// What the last turn asked its backend for, for comparing against what answered.
///
/// The two travel together because either alone is misleading: a name means nothing without knowing
/// whether it is comparable to a reply at all.
pub(super) struct Asked {
    /// The name the service was actually sent, which for a gateway is not the name a session holds.
    pub(super) name: String,
    /// Whether that name and the one a reply reports are drawn from one roster.
    pub(super) comparable: bool,
}

/// Record what the turn asked for against what answered, and say so once where they differ.
///
/// The endpoint substitutes rather than refusing: a premium model requested without a credential
/// comes back answered by a weaker model, with a 200 and a perfectly ordinary reply. So the only
/// trace is this field, and a session that never compares them cannot tell a model it chose from
/// one chosen for it.
///
/// The name is recorded whatever put it in force. A model named by the settings file is one
/// somebody expects to be answered by exactly as much as one picked from `/model` is, and passing
/// the asked-for half only for the second left a configured default substituted in silence for a
/// whole session while `/status` reported the substitution for the same turn. What keeps the
/// automatic entry from reading as a substitution is the roster comparison in
/// [`Session::substituted_model`], not the absence of a choice.
///
/// Said only when they differ, and only when the difference is new, since it would otherwise be a
/// line on every turn for the rest of the session.
pub(super) fn record_the_model_that_answered(
    session: &mut Session,
    asked: Asked,
    served: &str,
    premium: bool,
) {
    let already = session.substituted_model().is_some();
    session.served(asked.name, served, premium, asked.comparable);
    if !already && let Some(requested) = session.substituted_model() {
        session.note(t!(
            session_model_substituted,
            asked = requested,
            served = served
        ));
    }
}

/// Record the tier and the model of a finished turn, whichever model it ran on (PREM-9).
///
/// `/status` reports what the last turn did, so every turn is recorded. Only the comparison with
/// what the session asked for is left out where the turn ran on a model a definition or a skill
/// named: the turn has already compared that one with the model that answered (ADDRESS-11,
/// SKILL-15), and a comparison with the session's would report a substitution that did not
/// happen. That turn is recorded as asking for what answered, with nothing to compare.
pub(super) fn record_what_the_turn_ran_on(
    session: &mut Session,
    asked: Asked,
    served: &str,
    premium: bool,
    ran_on_the_sessions_model: bool,
) {
    if ran_on_the_sessions_model {
        record_the_model_that_answered(session, asked, served, premium);
    } else {
        session.served(served, served, premium, false);
    }
}

/// What a finished turn is measured against, and what to fall back on where it reported nothing.
///
/// One value rather than three arguments because none of them says anything alone: a figure without
/// the budget it is against is not a percentage, and a budget without knowing whether it was
/// advertised or assumed is not one the hint line may state flatly.
pub(super) struct Occupied {
    /// The budget the conversation is compacted at.
    pub(super) budget: u64,
    /// Whether that budget is a guess rather than one somebody typed or the endpoint advertised.
    pub(super) guessed: bool,
    /// What the last request the turn managed to send came to, or zero where it sent none.
    pub(super) last_request_tokens: u64,
}

/// The line a turn ran, and whether it may be repeated.
///
/// One value rather than two because neither says anything alone here: a loop repeats a line, and
/// whether this one may be repeated is a question about who wrote it and what the session was
/// doing when it went, rather than about the words.
#[derive(Clone, Copy)]
pub(super) struct Line<'a> {
    pub(super) text: &'a str,
    // The definition a person's `/agent` line addressed, which `text` does not carry.
    pub(super) addressed: Option<&'a crate::state::Addressed>,
    // Whether the turn was offered a later look at this line before it ran ([`will_look_again`]).
    pub(super) offered_a_later_look: bool,
}

impl Line<'_> {
    /// The line as the person typed it. Only the task went to the planner, but a stopped turn
    /// returning only the task would have Enter send it to the session's planner instead.
    ///
    /// Only the task where the name is the session's standing definition, because Enter on the task
    /// already addresses it, and a line naming it again would look like a second definition.
    pub(super) fn as_typed(&self, standing: Option<&crate::state::Addressed>) -> String {
        match self.addressed {
            Some(addressed) if standing.is_none_or(|standing| standing.name != addressed.name) => {
                format!("{AGENT_COMMAND} {} {}", addressed.name, self.text)
            }
            _ => self.text.to_string(),
        }
    }

    /// The model the turn asked for: an addressed definition's where it named one (ADDRESS-11).
    pub(super) fn model(&self, session: &Session, config: &Config) -> String {
        self.addressed
            .and_then(|addressed| addressed.model.as_deref())
            .or(session.model_of_the_turn())
            .unwrap_or(&config.default_model)
            .to_string()
    }
}

/// What a turn hands to the next one: paths the person vouched for, programs they allowed, and the
/// run prompts they have read.
///
/// One value rather than three because they travel together in both directions, and a turn that
/// reported none of them hands on exactly what it was given. The third is not a grant and sits here
/// only because it has the same lifetime as the other two.
pub(super) struct Carried {
    pub(super) trust: TrustStore,
    pub(super) programs: TrustedPrograms,
    pub(super) asked: AskedAbout,
    pub(super) exposed: bravebot_core::credentials::Exposed,
}

/// A quit keeps the session quitting; an ordinary stop may return its prompt to the editor.
///
/// Whether the prompt went back to the box, which is when the turn is also taken out of the
/// conversation (INPUT-4).
pub(super) fn finish_cancelled_turn(
    session: &mut Session,
    prompt: &str,
    attempts: Option<u32>,
) -> bool {
    // Stopping a turn stops every loop, whether or not it was a tick of one: the key means "stop
    // what is happening", and a schedule that survived it would send the next prompt as though
    // nothing had been said (LOOP-11).
    session.stop_loops();
    // And the watch whose fire this turn was, since stopping a fire's turn is the most exact way
    // anybody has to say which watch they are finished with: they are reading its prompt when they
    // press the key. A turn that was not a fire ends no watch, because that press is a person
    // steering their own work.
    session.stop_firing_watch();
    // Not the goal, which outlives one turn by design. A stopped turn is recorded as failed, so it
    // is not judged and the work is not sent straight back, and the person who stops a turn going
    // the wrong way keeps the condition they set: the key means "stop this", and a second press
    // with nothing running takes it off.
    session.stopped(attempts);
    // A fire is a whole turn, so the gap to the next fire is measured from here.
    session.watch_turn_ended();
    !session.is_quitting() && session.restore(prompt)
}

/// Take a round's report of what the turn has spent into the session, what its request came to
/// into the context reading, and the turn's running cache totals into the cache figures, so a turn
/// in flight reads how full the context is and how much the backend recognised rather than
/// waiting for the turn to end (INPUT-22, PANEL-8).
///
/// A report with no request behind it yet leaves both where they were.
pub(crate) fn progress_of_a_turn(
    session: &mut Session,
    spent: bravebot_agent::Spent,
    budget: u64,
    guessed: bool,
) {
    if spent.context_tokens > 0 {
        session.measured(spent.context_tokens, budget, guessed);
    }
    if spent.tokens > 0 {
        session.served_from_cache(
            spent.cached,
            spent.tokens.saturating_sub(spent.output_tokens),
        );
    }
    session.progressed(spent);
}

/// What a finished turn says about refusals, or nothing.
///
/// A refusal a later call to the same tool got past stays in the audit trail and is not drawn
/// here. One that stands is counted and the first is named. Where the turn was not clean and holds
/// no list of what was refused, the general sentence is all that can be said.
pub(super) fn refusal_note(
    clean: bool,
    standing: &[String],
    worked_around: usize,
) -> Option<String> {
    if clean {
        return None;
    }
    match standing.first() {
        Some(first) => Some(t!(
            session_refusals_left,
            count = standing.len(),
            first = first.as_str()
        )),
        None if worked_around > 0 => None,
        None => Some(t!(session_something_was_refused).to_string()),
    }
}

/// Record a finished turn in the session, charging what it cost along with how that divides into
/// what the model read and what it wrote, which only the worker's own count (`output_tokens`)
/// says.
///
/// Split out from `fold_outcome` so a test can call it: that function takes a whole
/// [`turn::Outcome`], which nothing outside the agent crate can build.
pub(super) fn complete_turn(
    session: &mut Session,
    reply: String,
    trail: Vec<bravebot_session::audit::TrailLine>,
    tokens: u64,
    output_tokens: u64,
    answered_as: Option<String>,
) {
    session.complete_as(reply, trail, tokens, answered_as);
    session.charged_split(tokens, output_tokens);
}

/// Fold a finished turn into the session.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the session, the outcome and the pieces of state the turn handed back, each its own type"
)]
pub(super) fn fold_outcome(
    session: &mut Session,
    outcome: Result<turn::Outcome, turn::TurnError>,
    sink: Trail,
    fallback: Carried,
    occupied: Occupied,
    asked: Asked,
    line: Line<'_>,
    // Lent for the one thing a finished turn needs it for: the first look at a path the turn
    // asked to have watched, which has to come from the same place every later look will.
    workspace: &Workspace,
) -> Carried {
    let carried = match outcome {
        Ok(outcome) => {
            let trail = sink.lines();
            complete_turn(
                session,
                outcome.reply_for_display().to_string(),
                trail,
                outcome.tokens,
                outcome.output_tokens,
                outcome
                    .addressed
                    .as_ref()
                    .map(|addressed| addressed.name().to_string()),
            );
            if let Some(note) = refusal_note(
                outcome.clean,
                &outcome.standing_refusals,
                outcome.worked_around,
            ) {
                session.note(note);
            }
            // Where the wall clock went, beside what the turn cost. The wall figure is the
            // session's own, taken from the moment Enter was pressed; this fills in the parts, which
            // only the worker saw.
            session.spent_time(outcome.timing);

            // How much of what the turn sent the backend recognised, which the two figures above
            // cannot say: a turn costs the same tokens whether they were read or recognised, and
            // about ten times the money.
            session.served_from_cache(
                outcome.cached,
                outcome.tokens.saturating_sub(outcome.output_tokens),
            );

            // What the turn's last request came to, against what it would be compacted at. Not
            // the same figure as the cost above: that adds every round together, this says how
            // full the context is now.
            session.measured(outcome.context_tokens, occupied.budget, occupied.guessed);

            record_what_the_turn_ran_on(
                session,
                asked,
                &outcome.model,
                outcome.premium,
                outcome.ran_on_the_sessions_model(),
            );
            // Where the turn was a tick, this is what arms the next one: an interval from the
            // driver's own clock, or the wait the turn asked for. Measured from here rather than
            // from when the tick went out, so the gap is between runs and a turn that outlasts
            // its own interval is not immediately due again.
            //
            // Where no loop is running, a wait the turn asked for starts one, which is how a turn
            // asked to watch something gets the later look it needs. The line it repeats is this
            // turn's own, and only where the person wrote it: a loop repeats a line somebody
            // endorsed, and a sentence this program wrote carrying a goal on is not one.
            // The watches this turn asked for, in the order it asked, and before the wait below:
            // a session does one of a watch, a loop and a goal at a time, and a turn that asked
            // for both meant the watch. Each has been through the gate a read of that path goes
            // through; what is left is the session's own question, which is whether it has room
            // and whether the first look sees anything.
            for armed in &outcome.watches {
                // The working directory the look was taken in travels with the watch, because a
                // relative path is the file it was armed on only while that is still the working
                // directory: after a `/cd` the same string names a file in the new one.
                // The first look is the one the tool call took, not a fresh one: the rest of the
                // turn may have rewritten the file, and that change is what the watch is for.
                session.arm_watch(&armed.path, workspace.root(), armed.looked.clone());
            }

            if session.looping().is_some() {
                session.loop_turn_ended(outcome.wakeup);
            } else if let Some(wakeup) = watch_to_start(outcome.wakeup, line.offered_a_later_look) {
                session.watch_again(line.text, wakeup);
            }

            // Carries forward any rule the turn recorded, so a path that received untrusted
            // data cannot be read back as trusted by the next turn, and any program the user
            // vouched for during it, so they are not asked about it again.
            Carried {
                trust: outcome.trust,
                programs: outcome.programs,
                asked: outcome.asked_about,
                exposed: outcome.exposed,
            }
        }
        Err(error) => {
            // A cancelled turn never arrives here: `finish_turn` hands it to
            // `finish_cancelled_turn`, which ends the loop (LOOP-11). Any other failure is a tick
            // that ended, and a self-paced loop that was told nothing falls back to the driver's
            // own wait. A loop must not end because one request failed, and must not run forever
            // on failures either.
            session.loop_turn_ended(None);

            // The trail is kept on failure too: a refusal is exactly when a user wants
            // to see what happened.
            let trail = sink.lines();
            // Format safe fields instead of an error that may contain endpoint credentials.
            let ending = error.ending();
            match ending {
                bravebot_agent::Ending::Failed(diagnosis) => {
                    session.fail(
                        bravebot_session::sessions::failure_reason(diagnosis, error.cut_off()),
                        ending,
                    );
                }
                bravebot_agent::Ending::Stopped { attempts } => session.stopped(attempts),
                bravebot_agent::Ending::Done => {}
            }
            session.keep_trail(trail);
            if occupied.last_request_tokens > 0 {
                session.measured(
                    occupied.last_request_tokens,
                    occupied.budget,
                    occupied.guessed,
                );
            }
            fallback
        }
    };

    // A fire is a whole turn, so the gap to the next fire of that watch is measured from here:
    // the turn's own length is what spaces fires out, exactly as it is for a tick. Outside the
    // two arms because a turn that failed still ended, and a watch left marked as firing would
    // never fire again and would make every prompt after it read as the driver's.
    session.watch_turn_ended();
    carried
}
