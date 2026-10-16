//! The steps of a turn's round loop that decide from the turn's own state: a status, a count, a
//! flag the driver set, the category of an error.
//!
//! None reads a file's content, labels anything or declassifies anything, which is why they can
//! live apart from the blocks of `one_turn` that present a tool's result and record the planner's
//! own turn. Each moves a block of the loop unchanged and takes what the block used as parameters.
//!
//! This file holds a `t!` lookup for the person (`limit_the_tools`) beside text for the planner,
//! so it stays off `PLANNER_FACING` in `tests/audience.rs`.

use bravebot_aichat::protocol::{ChatRequest, Effort, Message, Tool};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::policy::{AfterCeilingStop, Policy};
use bravebot_i18n::t;
use bravebot_net::Egress;
use std::time::Instant;

use super::{
    ROUNDS_AFTER_WRITING_BEFORE_RUNNING, ROUNDS_BEFORE_WRITING, ReadOutputIs, TOOL_BUDGET_SPENT,
    Task, TurnError, Worked, Working, after_a_ceiling_stop, ceiling_stop_narration,
    record_ceiling_stop,
};
use crate::backend::BackendError;
use crate::confirm::Confirmer;
use crate::conversation::{Composed, Conversation};
use crate::report::{DelegateId, Phase, Reporter};
use crate::request_view::{Prompt, Provenance, RequestView};
use crate::timing::Elapsed;
use crate::tools;
use crate::workspace::Workspace;
use bravebot_aichat::{Completion, Subscription};

/// Whether the endpoint answered with a different model from the one this turn asked for.
///
/// The endpoint substitutes rather than refuses a name it will not serve, so the answer's model
/// is compared with the name as the backend sends it. A backend that does not report the model it
/// was asked for, and the default model, are never counted as substituted.
pub(super) fn model_was_substituted(config: &Config, resolved: &str, answered: &str) -> bool {
    let asked = crate::backend::Backend::name_as_asked(config, resolved);
    crate::backend::Backend::reports_the_model_it_was_asked_for(config, resolved)
        && asked != bravebot_config::DEFAULT_MODEL
        && asked != answered
}

/// Which of the two reminders a turn has already put to the planner. Each is said once.
#[derive(Default)]
pub(super) struct Reminded {
    /// Whether the driver has already said this turn that no write has been asked for. Whether one
    /// has is the conversation's, since a turn continuing a stopped one inherits its writes. A
    /// requested write counts rather than a completed one: a write the user refused is a planner
    /// that tried to deliver, and telling it to start delivering would be answering something
    /// nobody asked.
    pub(super) nothing_written: bool,
    /// Whether it has said that files changed and nothing was run.
    pub(super) nothing_run: bool,
}

/// The two reminders put to the planner after a round's results and any interjection.
#[allow(clippy::too_many_arguments)]
pub(super) fn nudges(
    conversation: &mut Conversation,
    task: &Task,
    may_write: bool,
    may_run: bool,
    steps: usize,
    worked: &Worked,
    delegates: &[Working<'_>],
    reminded: &mut Reminded,
) {
    // Said after the round's results and any interjection, which is where a message from
    // the driver belongs: the planner reads what its calls returned, then what it is being
    // told about them. Between the calls and their results it would break the pairing.
    //
    // Once per turn. A planner that has been told and carried on reading has either
    // decided it has nothing to write yet, which is allowed, or is not going to be talked
    // out of it, and repeating the line every round would spend a request each time to say
    // something already in the conversation.
    //
    // Conditional in its wording rather than in its firing. The driver cannot tell a task
    // that asks for a change from one that asks a question, and it must not try: what it
    // knows is that rounds have gone by and nothing was written, and the planner is the
    // one that knows whether that is wrong.
    //
    // Plan mode offers the write tools and refuses every call to them, so no write
    // is possible while it is in force. Read as the round ends rather than once per
    // turn, since the person can change the mode while the turn runs (MODE-8).
    // The sandbox mode `read-only` does the same for the whole turn (SANDBOX-22).
    if may_write
        && !task.permission_mode.get().refuses_writes()
        && !tools::sandbox_refuses_writes(task.confine_runs, task.sandbox)
        && !conversation.asked_to_write()
        && !reminded.nothing_written
        && steps >= ROUNDS_BEFORE_WRITING
    {
        reminded.nothing_written = true;
        conversation.push_composed_from(
            Message::user(format!(
                "{TOOL_BUDGET_SPENT} That is {steps} rounds of tools and nothing written yet. \
     If the task asks for a change and any part of it is settled, write that part \
     now and keep looking only for the parts that are not. If it asks for no \
     change, carry on."
            )),
            Composed::Notice,
            Provenance::Driver,
        );
    }

    // The other half of the same problem. A change nobody built is a guess about whether
    // it builds, and the turn that edited eighteen files without compiling one of them
    // did not decide against building: it never got there, and the person watching could
    // not tell that from the summary.
    //
    // Counted from the write, since before that there is nothing to run, and said once
    // for the reason the line above is. A delegate is named because this is the case it
    // exists for: a build log is long, what is wanted from it is one sentence, and a
    // checker reads the one and reports the other.
    //
    // Not while a delegate working in this workspace is still out. It may be the
    // checker this line points at, part way through the build, and what it ran is
    // counted only once it is taken back.
    if may_run
        && !worked.ran_a_program
        && !reminded.nothing_run
        && worked
            .changed_at
            .is_some_and(|at| steps >= at + ROUNDS_AFTER_WRITING_BEFORE_RUNNING)
        && delegates.iter().all(|working| working.checkout.is_some())
    {
        reminded.nothing_run = true;
        conversation.push_composed_from(
            Message::user(format!(
                "{TOOL_BUDGET_SPENT} Files have changed this turn and nothing has been run. \
     A change that has not been built is a guess about whether it builds, so find \
     how this project builds and tests, and run that. Where the log is long and \
     what you want from it is which test failed, hand it to a checker with \
     spawn_agent instead. If there is nothing here to build, carry on."
            )),
            Composed::Notice,
            Provenance::Driver,
        );
    }
}

/// Take the tools away from the next request once the budget is spent or a person has stopped this
/// delegate, and say so to the person and to the planner.
pub(super) fn limit_the_tools<R: Reporter>(
    may_call_tools: &mut bool,
    task: &Task,
    steps: usize,
    reporter: &mut R,
    conversation: &mut Conversation,
) {
    // An unwatched turn with no bound on it does not stop being a turn, it stops being
    // anything: an agent that cannot make progress asks for one more tool call for as long as
    // anyone lets it. Where a person is watching there is a better bound than any number, and
    // `rounds` is `None`. See [`Task::rounds`] and [`MAX_TOOL_ROUNDS`].
    //
    // The budget is spent on tools, so the last word is taken away rather than the turn:
    // the next request carries no tools at all, and the planner answers with what it has.
    // Ending here instead would throw away the work and tell the user only that something
    // went round in circles.
    //
    // A person stopping this one delegate is the same ending reached early: the round
    // the planner just asked for runs, and the request after it carries no tools
    // (DELEGATE-25).
    if *may_call_tools && let Some(stop) = task.stop.as_ref().filter(|stop| stop.is_requested()) {
        *may_call_tools = false;
        stop.honour();
        reporter.narration(t!(delegate_stopped_narration).to_string());
        conversation.push_composed_from(
            Message::user(format!(
                "{TOOL_BUDGET_SPENT} The person stopped you, and you have no more tool \
             calls. Answer now with what you know. If the work is not finished, say \
             what you found and what is left to do."
            )),
            Composed::Notice,
            Provenance::Driver,
        );
    } else if let Some(limit) = task
        .rounds
        .filter(|limit| steps >= *limit && *may_call_tools)
    {
        *may_call_tools = false;
        reporter.narration(format!(
            "that is {limit} tool calls without an answer, so this turn has to finish with \
 what it has"
        ));
        conversation.push_composed_from(
            Message::user(format!(
                "{TOOL_BUDGET_SPENT} You have made {limit} tool calls this turn and have no more. \
 Answer now with what you know. If the work is not finished, say what you found, \
 what stopped you, and what would let you finish, such as a file named or a \
 directory trusted."
            )),
            Composed::Notice,
            Provenance::Driver,
        );
    }
}

/// Start the delegates a call asked for, each on a thread of the turn's scope.
#[allow(clippy::too_many_arguments)]
pub(super) fn start_delegates<'scope, 'env, S, C, R>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    started: Vec<(DelegateId, crate::delegate::Seeded)>,
    delegates: &mut Vec<Working<'scope>>,
    servers: Option<&crate::lsp::LanguageServers>,
    confirming: &'scope crate::shared::Lent<'_, C>,
    reporting: &'scope crate::shared::Lent<'_, R>,
    recording: &'scope crate::shared::Lent<'_, S>,
    config: &'scope Config,
    egress: &'scope Egress,
    workspace: &'scope Workspace,
    task: &'scope Task,
    cancel: &'scope Cancel,
    wallet: Option<&'scope dyn crate::shared::Spends>,
    turn_model: &Option<String>,
    effort: Option<Effort>,
) where
    S: Sink + ?Sized + Send,
    C: Confirmer + ?Sized + Send,
    R: Reporter + ?Sized + Send,
{
    // Started here rather than inside the call. A delegate outlives the call that asked
    // for one: that call has already answered, and what is still here when the work
    // finishes is the turn.
    // The turn's, not the session's: an addressed turn runs on its definition's.
    //
    // Copied rather than borrowed. A delegate outlives the round that started it,
    // so a `&str` into the turn's model would hold that variable borrowed for as
    // long as the delegate lives, and a skill loaded later in the turn could then
    // never change it. What each delegate carries is the model in force when it
    // started, which is the one its parent was asking at the time.
    let spawning_model = turn_model.clone();
    let spawning_effort = effort;
    for (id, seeded) in started {
        let vouched = seeded.vouched.clone();
        let checkout = seeded
            .workspace
            .as_ref()
            .and_then(crate::workspace::Workspace::checkout_handle);
        let shared = match checkout {
            Some(_) => None,
            None => servers.map(crate::lsp::LanguageServers::share),
        };
        let spawning_model = spawning_model.clone();
        let rounds = seeded.spec.rounds();
        let stop = seeded.stop.clone();
        let stopped_by = stop.clone();
        let handle = scope.spawn(move || {
            let mut confirmer = confirming.delegate_stoppable(id, stopped_by);
            let mut reporter = reporting.delegate(id);
            let mut sink = recording.delegate(id);
            let ended = crate::delegate::run(
                &seeded,
                config,
                egress,
                seeded.workspace.as_ref().unwrap_or(workspace),
                task.home.as_deref(),
                task.profile.as_deref(),
                spawning_model.as_deref(),
                spawning_effort,
                task.permission_mode.clone(),
                task.auto_vetting,
                &task.attribution,
                task.system_prompts.appending.as_deref(),
                task.output_cap,
                task.deadlines,
                task.confine_runs,
                task.sandbox,
                task.mcp.as_ref(),
                cancel,
                &mut confirmer,
                &mut reporter,
                &mut sink,
                wallet,
                shared,
            );
            (ended, reporter.last_spent(), reporter.take_inference())
        });
        delegates.push(Working {
            #[cfg(test)]
            join_started: None,
            id,
            seeded: vouched,
            checkout,
            rounds,
            stop,
            handle,
        });
    }
}

/// What the loop does after a request that may have failed.
pub(super) enum Recovery {
    /// Ask again: a message was added, a compaction is waiting, or the turn moved to its fallback.
    Retry,
    /// The request was answered.
    Proceed(Completion),
}

/// What a turn has already done about a request that did not succeed. Each is done once, which is
/// what stops a model with nothing to say from costing a request per round.
#[derive(Default)]
pub(super) struct Retries {
    /// Whether the turn moved to the person's fallback model.
    pub(super) fell_back: bool,
    /// Whether a request the service refused has already been answered with a compaction this turn
    /// (COMPACT-14). Once, and not reset: a second refusal is the turn's failure.
    pub(super) compacted_after_a_refusal: bool,
    /// The refusal waiting on a compaction, which the next pass of the loop makes before it asks
    /// again, and which is the turn's failure if there turns out to be nothing to cut.
    pub(super) refused: Option<BackendError>,
    /// Whether the last request went out because the one before it came back empty (TURN-6).
    pub(super) asked_after_an_empty_reply: bool,
    /// Whether it went out because the one before it reached the output ceiling (TURN-7).
    pub(super) asked_after_a_ceiling_stop: bool,
}

/// Settle a request's result: retry it after a failure the turn may answer for, fail the turn, or
/// hand back the completion.
///
/// Decided from the error's category and status and from the turn's own flags. No reply text is
/// read.
#[allow(clippy::too_many_arguments)]
pub(super) fn recover_from_failed_request<S: Sink, R: Reporter>(
    completion: Result<Completion, BackendError>,
    request: &ChatRequest,
    config: &Config,
    policy: &mut Policy<'_, S>,
    conversation: &mut Conversation,
    reporter: &mut R,
    steps: usize,
    turn_model: &mut Option<String>,
    retries: &mut Retries,
    may_fall_back: bool,
    may_compact: bool,
    may_call_tools: bool,
) -> Result<Recovery, TurnError> {
    let model = turn_model.as_deref().unwrap_or(&config.default_model);
    // A failure the person's fallback model may answer for: the same request goes to it
    // and the rest of the turn runs on it (BACKEND-53). Decided from the category the
    // driver derived from the status, which no reply wrote.
    if let Err(error) = &completion
        && may_fall_back
        && let Some((to, category)) = crate::backend::fallback_after(config, model, error)
    {
        policy.record_model_fallback(steps, model, &to, category.name());
        reporter.narration(t!(
            fallback_model_in_use,
            from = model,
            to = to.as_str(),
            category = category.name()
        ));
        *turn_model = Some(to);
        retries.fell_back = true;
        return Ok(Recovery::Retry);
    }
    // An empty reply is the planner's own output saying nothing, so deciding on it
    // reads nothing untrusted. The same request sent again tends to stay empty, so the
    // turn adds a line and asks once. Two in a row is a model with nothing to say here,
    // and asking a third time would only spend another request finding that out.
    let completion = match completion {
        // The service would not take the request as written, and this turn has not yet
        // answered one that way. Decided from the status and from the turn's own state,
        // so nothing the refusal said reaches it. A refusal for some other reason costs
        // one summary, which COMPACT-5 bounds, and is then reported as it was.
        Err(error)
            if error.refused_the_body() && may_compact && !retries.compacted_after_a_refusal =>
        {
            retries.compacted_after_a_refusal = true;
            retries.refused = Some(error);
            return Ok(Recovery::Retry);
        }
        Err(error) if error.is_empty_reply() && !retries.asked_after_an_empty_reply => {
            retries.asked_after_an_empty_reply = true;
            conversation.push_composed_from(
                Message::user(format!(
                    "{TOOL_BUDGET_SPENT} Your last reply was empty. Carry on from where \
             you were: make the next call, or if the work is done, say what you \
             found."
                )),
                Composed::Notice,
                Provenance::Driver,
            );
            return Ok(Recovery::Retry);
        }
        // A reply that reached the ceiling having written nothing: part way through
        // a call, thinking, or neither. Decided from the stop's structure, which says
        // nothing the reply wrote, and asked once for the same reason TURN-6 asks
        // once.
        Err(error) if error.cut_off().is_some() && !retries.asked_after_a_ceiling_stop => {
            retries.asked_after_a_ceiling_stop = true;
            // A reply that ran out wrote something, so it was not empty.
            retries.asked_after_an_empty_reply = false;
            if let Some(cut_off) = error.cut_off() {
                record_ceiling_stop(policy, steps, cut_off, AfterCeilingStop::AskedAgain);
                reporter.narration(ceiling_stop_narration(cut_off, may_call_tools));
                conversation.push_composed_from(
                    Message::user(after_a_ceiling_stop(cut_off, request, may_call_tools)),
                    Composed::Notice,
                    Provenance::Driver,
                );
            }
            return Ok(Recovery::Retry);
        }
        // The second in a row, and it wrote nothing: the stop is the turn's failure.
        Err(error) if error.cut_off().is_some() => {
            if let Some(cut_off) = error.cut_off() {
                record_ceiling_stop(policy, steps, cut_off, AfterCeilingStop::Ended);
            }
            return Err(error.into());
        }
        other => other?,
    };
    Ok(Recovery::Proceed(completion))
}

/// The request for one round: the conversation under the system prompt, the turn's effort and
/// output schema, and the tools where the budget allows them.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_request<R: Reporter>(
    model: &str,
    conversation: &Conversation,
    system: &Prompt,
    system_text: &str,
    effort: Option<Effort>,
    task: &Task,
    may_call_tools: bool,
    offered: &[Tool],
    read_output_is: impl Fn() -> ReadOutputIs,
    reporter: &mut R,
) -> ChatRequest {
    let (messages, marks) = conversation.with_system_marked(system_text);
    let request = ChatRequest::new(model, messages).with_effort(effort);
    let request = match &task.output_schema {
        Some(schema) => request.with_response_format(Some(
            bravebot_aichat::protocol::ResponseFormat::json_schema(schema.value().clone()),
        )),
        None => request,
    };
    let request = if may_call_tools {
        // Who answers `read_output` is stated on this request's copy, since the
        // person can change the mode while the turn runs (RUN-14).
        let mut listed = offered.to_vec();
        tools::state_read_output_answer(&mut listed, read_output_is().stated());
        request.with_tools(listed)
    } else {
        request
    };
    if task.delegate.is_none() && reporter.wants_request_view() {
        reporter.request_built(RequestView::of(&request, system, &marks));
    }
    request
}

/// Shorten the conversation before a request when it is past the budget, or when the service
/// refused the last one and there may be something to cut.
///
/// Decided from token counts and the turn's own flags.
#[allow(clippy::too_many_arguments)]
pub(super) fn compact_if_due<S: Sink, R: Reporter, T: Subscription>(
    config: &Config,
    egress: &Egress,
    cancel: &Cancel,
    policy: &mut Policy<'_, S>,
    conversation: &mut Conversation,
    reporter: &mut R,
    subscription: Option<&mut T>,
    turn_model: Option<&str>,
    steps: usize,
    account: &mut crate::outcome::Spent,
    spent: &mut Elapsed,
    may_compact: &mut bool,
    retries: &mut Retries,
) -> Result<(), TurnError> {
    // Before the request rather than after the reply that overflowed. The figure being
    // compared is the last round's, so this is one round late by construction, which is why
    // the budget sits below any window rather than at it.
    let mut shortened = false;
    if retries.refused.is_some() || *may_compact && account.context_tokens >= config.context_budget
    {
        reporter.phase(Phase::Compacting);
        let mut chat = crate::processor::Chat::new(config, egress)
            .spending(subscription)
            .on(turn_model)
            .stoppable_by(cancel);
        // A summary is a model call, so it belongs in the inference figure for the same reason
        // its tokens belong in the total: the turn was waiting on the endpoint for it.
        let summarising = Instant::now();
        let summary = crate::compact::compact(policy, &mut chat, conversation, steps, None);
        let interval = crate::timing::Interval::since(summarising);
        spent.inference += interval.duration();
        reporter.inference_interval(interval);
        if let Err(error) = &summary
            && let Some(usage) = error.completed_usage()
        {
            account.add_usage(usage);
            reporter.spent(crate::outcome::Spent {
                timing: spent.finish(),
                ..*account
            });
        }
        match summary {
            Ok(Some(done)) => {
                shortened = true;
                account.add_usage(done.usage);
                // The figure measured the conversation the summary replaced.
                reporter.spent(crate::outcome::Spent {
                    context_tokens: 0,
                    timing: spent.finish(),
                    ..*account
                });
                reporter.narration(format!(
                    "the conversation was getting long, so {} earlier messages were \
         summarised and the last {} kept as they are",
                    done.summarised, done.kept
                ));
            }
            // Nothing to shorten yet, which is the ordinary answer and not worth a word.
            // Nothing was sent, so asking again next round is free, and a round or two later
            // there usually is something.
            //
            // Said nothing rather than saying so. Once a conversation is past the budget and
            // cannot get under it, this is the answer on nearly every round of every turn for
            // the rest of the session, and a line the user can do nothing about, repeated
            // forever, buries the ones they can. What it was there to prevent, a session
            // running out of room with no warning, is the context gauge's job, and the gauge
            // does it better: it is always on screen, and it says nothing twice.
            Ok(None) => {}
            // The conversation is untouched, so the turn carries on with the history it had.
            // Failing the turn over this would turn a request that might still have fit into
            // one that certainly does not happen.
            Err(crate::compact::CompactError::Chat(error)) if error.is_cancelled() => {
                return Err(error.into());
            }
            Err(error) => {
                *may_compact = false;
                let category = error.category();
                reporter.narration(format!(
                "the conversation could not be summarised ({}); continuing with the existing context",
                category.name()
            ));
            }
        }
    }

    // A request the service refused, and nothing to cut: the refusal is reported as it
    // always was, with no second request to repeat it (COMPACT-14).
    if let Some(error) = retries.refused.take()
        && !shortened
    {
        return Err(error.into());
    }
    Ok(())
}
