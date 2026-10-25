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
    Inputs, LimitReached, Plan, ROUNDS_AFTER_WRITING_BEFORE_RUNNING, ROUNDS_BEFORE_WRITING,
    ReadOutputIs, State, TOOL_BUDGET_SPENT, Task, TurnError, Worked, Working, collect_delegates,
    collect_jobs, narrate_between_calls, note_rules_added, record_answer, rule_notices,
    show_what_running_jobs_have_printed, stopped_at, take_interjections, take_session_messages,
    tell_of_kept_jobs, token_limit_reached,
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

/// What the person is told when a reply reached the output ceiling and the planner is asked again
/// (TURN-7). A turn with no tools left is asked for a shorter answer rather than for the work.
pub(super) fn ceiling_stop_narration(
    cut_off: &bravebot_aichat::CutOff,
    may_call_tools: bool,
) -> String {
    use bravebot_aichat::OpenCall;
    let tokens = cut_off.ceiling;
    if !may_call_tools {
        return t!(ceiling_stop_answer_now, tokens = tokens);
    }
    match (&cut_off.call, cut_off.thought) {
        (
            Some(OpenCall {
                tool: Some(tool), ..
            }),
            _,
        ) => {
            t!(ceiling_stop_in_call, tokens = tokens, tool = tool.as_str())
        }
        (Some(OpenCall { tool: None, .. }), _) => t!(ceiling_stop_in_a_call, tokens = tokens),
        (None, true) => t!(ceiling_stop_thinking, tokens = tokens),
        (None, false) => t!(ceiling_stop_silent, tokens = tokens),
    }
}

/// What the person is told when a reply the output ceiling stopped is the turn's answer. A call it
/// was writing is named, since the text reads as though the work it announces was done.
pub(super) fn answer_stopped_narration(cut_off: &bravebot_aichat::CutOff) -> String {
    use bravebot_aichat::OpenCall;
    let tokens = cut_off.ceiling;
    match &cut_off.call {
        Some(OpenCall {
            tool: Some(tool), ..
        }) => t!(
            ceiling_stop_ends_in_call,
            tokens = tokens,
            tool = tool.as_str()
        ),
        Some(OpenCall { tool: None, .. }) => t!(ceiling_stop_ends_in_a_call, tokens = tokens),
        None => "the model reached its output limit, so this answer stops where it did. \
                 What it wrote is kept; raise BRAVEBOT_OUTPUT_BUDGET or ask for less in one turn"
            .to_string(),
    }
}

/// Put a reply the output ceiling stopped in the trail, with what the turn did about it (TURN-7).
pub(super) fn record_ceiling_stop<S: Sink>(
    policy: &mut Policy<'_, S>,
    round: usize,
    cut_off: &bravebot_aichat::CutOff,
    then: AfterCeilingStop,
) {
    policy.record_ceiling_stop(
        round,
        cut_off.ceiling,
        cut_off
            .call
            .as_ref()
            .map(|call| (call.tool.as_deref(), call.arguments)),
        cut_off.thought,
        then,
    );
}

/// What the planner is told after a reply of its own reached the output ceiling (TURN-7).
///
/// Made from the stop's structure and the request's own spelling of a tool it offered, so nothing
/// the reply wrote reaches the line. The remedy follows from what the reply was writing: a file
/// too big for one reply is written in parts, with the write tools `request` offered, and a reply
/// that spent the ceiling thinking is asked to act sooner.
pub(super) fn after_a_ceiling_stop(
    cut_off: &bravebot_aichat::CutOff,
    request: &ChatRequest,
    may_call_tools: bool,
) -> String {
    use bravebot_aichat::OpenCall;
    let ceiling = cut_off.ceiling;
    let stopped = match (&cut_off.call, cut_off.thought) {
        (
            Some(OpenCall {
                tool: Some(tool), ..
            }),
            _,
        ) => {
            format!("while writing a call to {tool}, so the call was not made")
        }
        (Some(OpenCall { tool: None, .. }), _) => {
            "while writing a tool call, so the call was not made".to_string()
        }
        (None, true) => "while thinking, before it said or did anything".to_string(),
        (None, false) => "before it said or did anything".to_string(),
    };
    let next = match &cut_off.call {
        _ if !may_call_tools => "Answer now, in fewer words.",
        Some(OpenCall {
            tool: Some(tool), ..
        }) if tools::writes_a_file(tool) => match (
            request.offered("write_file").is_some(),
            request.offered("edit_file").is_some(),
        ) {
            (true, true) => {
                "Nothing was written. Make the same change in smaller calls, each well under the \
                 limit: split it across several files, or write the first part and add the rest \
                 with edit_file."
            }
            (true, false) => {
                "Nothing was written. Make the same change in smaller calls, each well under the \
                 limit: split it across several files."
            }
            _ => {
                "Nothing was written. Make the same change in smaller calls, each well under the limit."
            }
        },
        Some(_) => "Make it again with less in it, in smaller calls each well under the limit.",
        None if cut_off.thought => "Think less before you act: make the next call now.",
        None => "Make the next call now, or give a shorter answer.",
    };
    format!(
        "{TOOL_BUDGET_SPENT} Your last reply reached the output limit of {ceiling} tokens \
         {stopped}. {next}"
    )
}

/// What a round does before its request goes out: stop if the turn was stopped, say a change of
/// mode, bring in a rule a file matched, take back what finished while the last round ran, hold
/// to the budgets and the limits, and compact when the conversation is due for it.
///
/// Each step decides from the turn's own state or hands what it carries to an effect. The
/// system prompt a rule grows stays in `grown`, which the caller reads to build the request.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the turn's inputs and plan, its state, and each collaborator the steps before a request lend to an effect"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the steps before a request are taken in order from one state, and were one block of run_rounds"
)]
pub(super) fn before_a_request<S: Sink, C: Confirmer, R: Reporter>(
    inputs: Inputs<'_>,
    plan: &Plan<'_>,
    state: &mut State<'_>,
    policy: &mut Policy<'_, S>,
    conversation: &mut Conversation,
    reporter: &mut R,
    confirmer: &mut C,
    delegates: &mut Vec<Working<'_>>,
    waits: &mut crate::timing::DelegateWait,
    grown: &mut Option<(Prompt, String)>,
) -> Result<(), TurnError> {
    let Inputs {
        config,
        egress,
        workspace,
        task,
        cancel,
        hooks,
        ..
    } = inputs;
    let read_output_is = || super::setup::read_output_is(task, plan.offers_read_output);
    // Checked before each request rather than mid-flight: a request already on the wire has
    // to finish, but nothing new needs to start.
    if cancel.is_cancelled() {
        return Err(TurnError::Cancelled { attempts: Some(0) });
    }

    // The mode the person last chose applies to this turn as it runs (MODE-8). The
    // prompt was written in the one the turn started in, so a change that matters to
    // the planner is said to it before the next request.
    let chosen = task.permission_mode.get();
    if let Some(notice) = state.told_mode.change_notice(chosen) {
        conversation.push_composed_from(
            Message::user(&notice),
            Composed::Notice,
            Provenance::Driver,
        );
    }
    state.told_mode = chosen;

    // A file the last round read or wrote may match the paths of a rule held back. The rule is
    // appended to the system prompt before this request goes out, after every byte the last
    // one carried, and the person is told which file brought it (INSTR-16).
    let loaded = state.held_rules.take_for(workspace);
    if !loaded.is_empty() {
        let (system, system_text) =
            grown.get_or_insert_with(|| (plan.system.clone(), plan.system_text.clone()));
        for loaded in &loaded {
            for (provenance, text) in crate::preamble::rule_pieces(loaded) {
                system.push(provenance, text);
            }
        }
        *system_text = system.text();
        for notice in rule_notices(workspace, task.delegate.is_none(), &loaded) {
            reporter.notice(notice.message.clone());
            state.notices.push(notice);
        }
        note_rules_added(workspace, &loaded);
    }

    // Whatever finished while the last round was running, before the planner is asked what to
    // do next. Waiting for none of them: one that is still working is left working, which is
    // the whole of what starting them separately buys.
    collect_delegates(
        delegates,
        policy,
        conversation,
        reporter,
        &mut state.account,
        false,
        waits,
        &mut state.spent,
        state.hook_notices,
        &mut state.worked,
        state.steps,
    )?;

    // After the collection above, which adds what a finished delegate spent, so a delegate
    // whose child took it past its budget goes to the planner's next request without tools.
    stop_at_token_budget(
        &mut state.may_call_tools,
        task,
        state.account.tokens,
        reporter,
        conversation,
    );

    reporter.spent(crate::outcome::Spent {
        timing: state.spent.finish(),

        ..state.account
    });

    // And whatever exited while it was running, for the same reason and in the same place:
    // the finish of a background job is news the turn is told rather than something the
    // planner has to remember to ask about (CMDLINE-14).
    collect_jobs(
        state.jobs,
        task.output_cap.unwrap_or(tools::OUTPUT_CAP),
        read_output_is(),
        policy,
        conversation,
        reporter,
    )?;

    show_what_running_jobs_have_printed(state.jobs, policy, reporter);

    // Once, after the finishes above, so what is left is what is still going: a job
    // the session kept from an earlier turn is something the planner has no other way
    // to know is there (RUN-15).
    if !std::mem::replace(&mut state.told_of_kept_jobs, true) {
        tell_of_kept_jobs(state.jobs, conversation);
    }

    // And whatever another session sent, in the same place and for the same reason: it
    // reaches the planner at the start of a request, so a session with no turn running
    // keeps it until a person starts one (MSG-6).
    take_session_messages(task, policy, conversation, reporter)?;

    // Before anything new is sent, compaction included, since a summary is a request that
    // spends tokens (TURN-8). A delegate's own task carries no limit: what it spends
    // reaches this total when it is collected, and the person is asked once, here.
    {
        let mut asking = crate::confirm::Timed::new(confirmer);
        let held = crate::spend_limit::hold(
            &task.spend_limit,
            task.spent_before + state.account.tokens,
            state.steps,
            policy,
            &mut asking,
            reporter,
        );
        state.spent.stalled += asking.waited();
        if held == crate::spend_limit::Held::Stop {
            return Err(TurnError::Cancelled { attempts: Some(0) });
        }
    }

    // The person's own limit on the run, which ends it where the session limit above asks
    // (TURN-14). Nobody is asked: a run that has one has nobody to ask, and raising a limit
    // the person set is theirs to do by running it again.
    if let Some(reached) = token_limit_reached(task, state) {
        return Err(stopped_at(policy, state.steps, reached));
    }

    compact_if_due(
        config,
        egress,
        task.account_session(),
        cancel,
        policy,
        conversation,
        reporter,
        state.subscription.as_mut(),
        state.turn_model.as_deref(),
        state.steps,
        &mut state.account,
        &mut state.spent,
        &mut state.may_compact,
        &mut state.retries,
        // A delegate summarises a conversation of its own, which is not the session's (HOOK-2).
        match task.delegate {
            None => Some(AfterCompacting {
                hooks,
                workspace,
                said: &mut *state.hook_notices,
            }),
            Some(_) => None,
        },
    )?;

    // Again, because the summary a compaction asks for spends tokens of its own, and the
    // request that follows it is the one this limit is meant to hold back (TURN-14).
    if let Some(reached) = token_limit_reached(task, state) {
        return Err(stopped_at(policy, state.steps, reached));
    }
    Ok(())
}

/// What `run_rounds` does next once a round's reply has been received and recovered.
pub(super) enum AfterReply {
    /// Ask again: the reply was dealt with and the round starts over.
    Continue,
    /// The reply is the turn's answer.
    Answer,
    /// The reply asks for a round of tool calls, which is within every limit.
    RunTheCalls,
}

/// What a round does with a reply that came back: account for what it spent, deal with a reply the
/// output ceiling stopped, finish the turn when the budget is spent or the planner answered, and
/// refuse a round past the most the person allowed (TURN-14).
///
/// Decided from the turn's own counters and from whether the reply carries calls and was cut off.
/// What the reply says is only recorded and handed to the effects that show it.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the turn's inputs and state, each collaborator the steps lend to an effect, and the request and reply of the round"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the steps after a reply are taken in order from one state, and were one block of run_rounds"
)]
pub(super) fn after_a_reply<S: Sink, C: Confirmer, R: Reporter>(
    inputs: Inputs<'_>,
    state: &mut State<'_>,
    policy: &mut Policy<'_, S>,
    conversation: &mut Conversation,
    reporter: &mut R,
    confirmer: &mut C,
    delegates: &mut Vec<Working<'_>>,
    waits: &mut crate::timing::DelegateWait,
    request: &ChatRequest,
    completion: &mut Completion,
) -> Result<AfterReply, TurnError> {
    let Inputs { task, cancel, .. } = inputs;
    state.retries.asked_after_an_empty_reply = false;
    // The backend carries no call out of a reply the ceiling stopped (BACKEND-42),
    // and one that did would be a call nobody finished writing.
    if completion.cut_off.is_some() {
        completion.calls.clear();
    }
    state.account.add_usage(completion.usage);
    state.account.context_tokens = completion.context_tokens;
    conversation.measured(state.account.context_tokens);
    reporter.spent(crate::outcome::Spent {
        timing: state.spent.finish(),
        ..state.account
    });

    // A reply the ceiling stopped part way through a call, after text. The text is
    // the planner's and goes into the conversation as any answer does; the call was
    // never made, and the planner is told so and asked for the work in parts.
    if let Some(cut_off) = completion
        .cut_off
        .as_ref()
        .filter(|cut_off| cut_off.call.is_some())
        && state.may_call_tools
        && !state.retries.asked_after_a_ceiling_stop
    {
        state.retries.asked_after_a_ceiling_stop = true;
        record_ceiling_stop(policy, state.steps, cut_off, AfterCeilingStop::AskedAgain);
        record_answer(policy, conversation, &completion.content)?;
        narrate_between_calls(policy, reporter, &completion.content);
        reporter.narration(ceiling_stop_narration(cut_off, state.may_call_tools));
        conversation.push_composed_from(
            Message::user(after_a_ceiling_stop(cut_off, request, state.may_call_tools)),
            Composed::Notice,
            Provenance::Driver,
        );
        return Ok(AfterReply::Continue);
    }
    if completion.cut_off.is_none() {
        state.retries.asked_after_a_ceiling_stop = false;
    }
    // A reply the ceiling stopped is kept for what it says, so the person has to be
    // told that it stops short: the text arrives looking like an answer, and an answer
    // that ends mid-sentence is worth nothing if it is read as a whole one. It carries
    // no calls, so this round is the last.
    if let Some(cut_off) = &completion.cut_off {
        record_ceiling_stop(policy, state.steps, cut_off, AfterCeilingStop::KeptTheText);
        reporter.narration(answer_stopped_narration(cut_off));
    }

    // The budget is spent, so this round is the answer whatever it holds. A planner that
    // asked for a tool anyway does not get one: a request that offered none is not one a
    // call can be answering, and running them would put the turn back in the loop the
    // budget exists to end.
    if !state.may_call_tools {
        if !completion.calls.is_empty() {
            reporter
                .narration("the tool budget was spent, so the last calls were not run".to_string());
        }
        // Waited for even here, where the planner will not read what they say. They are
        // writing to a person's workspace, and a turn that reported itself finished while
        // that was still going would be reporting something untrue.
        collect_delegates(
            delegates,
            policy,
            conversation,
            reporter,
            &mut state.account,
            true,
            waits,
            &mut state.spent,
            state.hook_notices,
            &mut state.worked,
            state.steps,
        )?;
        return Ok(AfterReply::Answer);
    }

    if completion.calls.is_empty() {
        // The planner has answered while something it started is still working. The turn
        // asked for that work, so what the delegate says is part of what the turn was for:
        // the answer so far goes into the conversation, the reports follow it, and the
        // planner answers once more knowing what came back.
        if !delegates.is_empty() {
            // Between-calls text rather than the reply: the round is not the
            // turn's last, and left as the reply in progress it would be cleared
            // when the next round starts and never reach the transcript.
            narrate_between_calls(policy, reporter, &completion.content);
            record_answer(policy, conversation, &completion.content)?;
            collect_delegates(
                delegates,
                policy,
                conversation,
                reporter,
                &mut state.account,
                true,
                waits,
                &mut state.spent,
                state.hook_notices,
                &mut state.worked,
                state.steps,
            )?;
            // A round the planner spent waiting is still a round, and the wait is when a
            // person watching a turn go somewhere they did not ask for is most likely to
            // say so. This is the boundary their line was aimed at. Reaching the next
            // request without asking would put the delegate's report to the planner and
            // none of what the person made of it, and the turn can end on that request.
            //
            // Guarded on the stop for the reason the boundary below is: Escape during the
            // wait ends this turn, and a line typed after it belongs to the next one.
            if !cancel.is_cancelled() {
                take_interjections(task, confirmer, policy, conversation, reporter);
            }
            return Ok(AfterReply::Continue);
        }
        // The planner has answered with tasks still open in the list the panel keeps. The
        // answer goes into the conversation as it does above, and the planner is asked once
        // for the list to be brought up to date, so the answer and the panel agree when the
        // turn ends. A turn the person stopped as the answer arrived is not asked anything
        // further, since the next request would end it as stopped and lose the answer. Nor is
        // a reply the ceiling stopped, which is the turn's answer as it stands.
        if completion.cut_off.is_none()
            && !cancel.is_cancelled()
            && let Some(open) = open_tasks_to_ask_about(policy, conversation, &state.offered)
        {
            narrate_between_calls(policy, reporter, &completion.content);
            record_answer(policy, conversation, &completion.content)?;
            ask_about_open_tasks(conversation, &mut state.reminded, open);
            // The line a person typed as the answer arrived is meant for the turn they are
            // watching, and the request this goes on to make is its next round.
            take_interjections(task, confirmer, policy, conversation, reporter);
            return Ok(AfterReply::Continue);
        }
        return Ok(AfterReply::Answer);
    }

    // The planner asked for a round past the most the person allowed. Stopped before that
    // round runs, so nothing it would have done has happened (TURN-14). A run that answered
    // after its last allowed round never gets here, since it asked for no further round.
    if let Some(most) = task.limits.rounds
        && state.steps >= most
    {
        return Err(stopped_at(
            policy,
            state.steps,
            LimitReached::Rounds { most },
        ));
    }
    Ok(AfterReply::RunTheCalls)
}

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

/// Which of the reminders a turn has already put to the planner. Each is said once.
#[derive(Default)]
pub(super) struct Reminded {
    /// Whether the driver has already said this turn that no write has been asked for. Whether one
    /// has is the conversation's, since a turn continuing a stopped one inherits its writes. A
    /// requested write counts rather than a completed one: a write the user refused is a planner
    /// that tried to deliver, and telling it to start delivering would be answering something
    /// nobody asked.
    pub(super) nothing_written: bool,
    /// Set once the reminder that files changed and nothing was run has been put, so it is said once
    /// a turn and a planner that keeps not running anything is not nagged every round.
    pub(super) nothing_run: bool,
    /// Set once the planner has been asked to bring its task list up to date. Set where the answer
    /// is handled rather than by [`nudges`], and read where a task list's result is recorded: a list
    /// written after the question is its answer to it, and the conversation does not count it.
    pub(super) open_tasks: bool,
}

/// The two reminders put to the planner after a round's results and any interjection.
#[expect(
    clippy::too_many_arguments,
    reason = "the conversation and `reminded` are exclusive borrows and the rest is read-only state of the round, so a bundle would hold both kinds at once"
)]
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
        // A delegate that wrote in this workspace has delivered: its changes are on disk, and
        // the planner's own write calls are not the only way a turn writes (TURN-3).
        && worked.changed_at.is_none()
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

/// How many tasks the planner's latest list leaves open, where the planner is to be asked about
/// them before the turn ends with its answer (TURN-12). `None` where it is not: no list, none
/// open, the planner already asked about this one, or no way to send a list this turn.
///
/// The count is taken from the conversation whether or not it leads to a question, so a list is
/// read once: a list with nothing open is not read again at the end of every turn after it. A turn
/// that cannot send a list leaves it for one that can.
///
/// Whether to ask is a decision, so the count is read through `Policy::read_trusted_content`, which
/// refuses a list that is not trusted. None is kept for one: the conversation records a count only
/// where the planner was shown its list, and a list at any other label is held back from it.
pub(super) fn open_tasks_to_ask_about<S: Sink>(
    policy: &mut Policy<'_, S>,
    conversation: &mut Conversation,
    offered: &[Tool],
) -> Option<usize> {
    if !offered
        .iter()
        .any(|tool| tool.function.name == "todo_write")
    {
        return None;
    }
    let open = conversation.take_open_tasks()?;
    policy
        .read_trusted_content("todo_write", &open)
        .ok()
        .filter(|count| *count > 0)
}

/// Ask the planner to bring its task list up to date, saying how many tasks it leaves open and
/// nothing about them.
///
/// Conditional in its wording rather than in its firing, as the reminders above are. The driver
/// cannot tell a list the planner finished from one it left open for a reason, such as a question
/// it put to the person, and must not try: it knows only how many tasks are open, and the planner
/// knows whether that is wrong.
pub(super) fn ask_about_open_tasks(
    conversation: &mut Conversation,
    reminded: &mut Reminded,
    open: usize,
) {
    reminded.open_tasks = true;
    let tasks = if open == 1 { "task" } else { "tasks" };
    conversation.push_composed_from(
        Message::user(format!(
            "{TOOL_BUDGET_SPENT} Your task list still has {open} {tasks} not marked completed \
             or cancelled. Send the whole list again with todo_write, marking a task completed \
             where it is done and cancelled where it was dropped. If work remains, leave those \
             tasks open and say in your answer what is left. Then give your answer again in \
             full, since the person is shown the last one as your reply."
        )),
        Composed::Notice,
        Provenance::Driver,
    );
}

/// Take the tools away from the next request once this delegate has spent its token budget, and
/// say so to the person and to the planner. Whether it did.
///
/// The delegate's own total, from the usage its backend reported and the delegates it has
/// collected, against a figure its spec fixed: no reply or tool result is read (DELEGATE-29). Not
/// `spend_limit::hold`, which asks a person, and a delegate has none. Called after a round and
/// again once a delegate's spend has been collected, so a child's spend counts against its
/// parent before the parent's next request.
pub(super) fn stop_at_token_budget<R: Reporter>(
    may_call_tools: &mut bool,
    task: &Task,
    tokens: u64,
    reporter: &mut R,
    conversation: &mut Conversation,
) -> bool {
    let Some(budget) = task
        .delegate
        .as_ref()
        .and_then(bravebot_core::delegate::DelegateSpec::budget)
        .filter(|budget| tokens >= *budget && *may_call_tools)
    else {
        return false;
    };
    *may_call_tools = false;
    reporter.narration(t!(delegate_budget_narration, budget = budget));
    conversation.push_composed_from(
        Message::user(format!(
            "{TOOL_BUDGET_SPENT} You have spent your budget of {budget} tokens and have no more \
             tool calls. Answer now with what you know. If the work is not finished, say what \
             you found and what is left to do."
        )),
        Composed::Notice,
        Provenance::Driver,
    );
    true
}

/// Take the tools away from the next request once the budget is spent or a person has stopped this
/// delegate, and say so to the person and to the planner.
pub(super) fn limit_the_tools<R: Reporter>(
    may_call_tools: &mut bool,
    task: &Task,
    steps: usize,
    tokens: u64,
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
    } else if stop_at_token_budget(may_call_tools, task, tokens, reporter, conversation) {
        // Said by the call itself.
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
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the turn's scope, the delegates to start and each collaborator a delegate thread borrows"
)]
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
        let budget = seeded.spec.budget();
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
            budget,
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
    /// Set once the turn has moved to the fallback model, so the move happens once, and so that
    /// the model that answered is not compared with the session's, which is the one that failed.
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
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the failed completion and request, the turn's collaborators and counters, and three flags"
)]
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
        // The reason is handed on as the closed value it is, so that where it is the ChatGPT
        // account's the person is told that failure and not only its category.
        reporter.fell_back(model, &to, crate::report::Why::of(error.diagnosis()));
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
#[expect(
    clippy::too_many_arguments,
    reason = "the reporter is an exclusive borrow and the rest is read-only per-turn input held by the turn, which has no struct for them"
)]
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

/// What a compaction a turn made fires: the hooks, where they run, and where what they say goes.
///
/// Given for the turn a person asked for and not for a delegate, whose conversation is its own and
/// not the session's.
pub(super) struct AfterCompacting<'a> {
    pub hooks: &'a bravebot_config::hooks::Hooks,
    pub workspace: &'a Workspace,
    /// The sentences the hooks said went wrong, for the turn's account of itself (HOOK-7).
    pub said: &'a mut Vec<String>,
}

/// Shorten the conversation before a request when it is past the budget, or when the service
/// refused the last one and there may be something to cut.
///
/// Decided from token counts and the turn's own flags.
///
/// Once a summary has been adopted, the hooks declared for `compacted` run (HOOK-2). They read
/// nothing of the summary and decide nothing about the turn, which goes on as it would have.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the turn's collaborators, its spend and step counters, the compaction flag and the hooks"
)]
pub(super) fn compact_if_due<S: Sink, R: Reporter, T: Subscription>(
    config: &Config,
    egress: &Egress,
    session: &crate::chatgpt::AccountSession,
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
    afterwards: Option<AfterCompacting<'_>>,
) -> Result<(), TurnError> {
    // Before the request rather than after the reply that overflowed. The figure being
    // compared is the last round's, so this is one round late by construction, which is why
    // the budget sits below any window rather than at it.
    let mut shortened = false;
    // The cheaper step first (COMPACT-18): drop what old tool results hold, and summarise only
    // if that was not enough. A refused request has no figure to lower, so it goes straight on.
    if retries.refused.is_none()
        && *may_compact
        && account.context_tokens >= config.context_budget
        && conversation.mask_old_results().is_some()
    {
        account.context_tokens = conversation.last_request_tokens();
        reporter.spent(crate::outcome::Spent {
            timing: spent.finish(),
            ..*account
        });
    }
    if retries.refused.is_some() || *may_compact && account.context_tokens >= config.context_budget
    {
        reporter.phase(Phase::Compacting);
        let mut chat = crate::processor::Chat::new(config, egress, session)
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
                if let Some(after) = afterwards {
                    after.said.extend(super::fire_hooks(
                        after.hooks,
                        bravebot_config::hooks::Moment::Compacted,
                        None,
                        after.workspace,
                        reporter,
                    ));
                }
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
                reporter.summary_failed(crate::report::Why::of_summary(&error));
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

/// What the turn does once its rounds have returned, whatever way they ended: end the jobs it
/// does not keep, hand the counts to the caller, report what the turn spent, and say when files
/// changed and no command was run.
///
/// Decided from the turn's own counters and flags. No reply or tool result is read.
pub(super) fn close_the_rounds<S: Sink, R: Reporter>(
    policy: &mut Policy<'_, S>,
    reporter: &mut R,
    state: &mut State<'_>,
    began: Instant,
    may_run: bool,
    rounds_taken: &mut usize,
    worked_out: &mut Worked,
) {
    // Whatever way the rounds ended, a stop and a failed request included: each leaves the jobs
    // to die with the turn, and the person is told which ones before they do.
    //
    // Except where the session keeps them: those are left running for the next turn to find, and
    // are ended by the session ending (RUN-15).
    if !state.jobs.is_kept() {
        state.jobs.stop_all(policy, reporter);
    }
    *rounds_taken = state.steps;
    *worked_out = state.worked;
    // A ChatGPT sign-in renewed by a call that is not a round's request, such as the goal check or a
    // processor, and not said yet (CHATGPT-7).
    crate::chatgpt::say_an_unsaved_renewal(reporter);
    state.spent.wall = began.elapsed();
    reporter.spent(crate::outcome::Spent {
        timing: state.spent.finish(),
        ..state.account
    });

    // Said to the person, not to the planner, which gets its own question while the turn is still
    // going and can still act on it. They are the one about to act on a diff, and nothing the turn
    // leaves behind distinguishes a change that was compiled from one that was never tried. Not a
    // reproach: plenty of turns have nothing to build, and this says what happened rather than
    // what should have.
    //
    // Above the unwrap, so a stop and a failed request say it too: either leaves the same changed
    // files on disk as an answer does, and being stopped with none of it compiled is the state a
    // person is least able to spot for themselves. Both are the round loop's own record, settled
    // by whatever it and the delegates it took back did before it ended, and every delegate is
    // taken back before the rounds return.
    //
    // Only where a run was possible, the state the planner's question is asked in. A definition
    // or a delegate whose tools leave out `run` could not have built what it wrote, so the line
    // would report something it had no way to do.
    if may_run && state.worked.changed_at.is_some() && !state.worked.ran_a_program {
        reporter.narration(
            "files changed this turn and no command was run, so none of it has been \
         built or tested"
                .to_string(),
        );
    }
}

/// Say so, to the person and in the turn's notices, when the endpoint answered with a different
/// model from the one a definition named or a loaded skill moved the turn onto.
///
/// Compared by name from the turn's own record of what it asked for. The answering model's name is
/// only compared, as in [`model_was_substituted`]; the reply is not read.
pub(super) fn note_substituted_models<R: Reporter>(
    config: &Config,
    reporter: &mut R,
    state: &mut State<'_>,
    addressed: Option<&bravebot_core::delegate::Addressed>,
    definition_model: Option<&(String, String)>,
    answered: &str,
) {
    // Compared the way a delegate's model is, because the endpoint substitutes rather than refuses
    // a name it will not serve. The sentence names the definition and the model it named, and
    // never the one that answered (ADDRESS-11).
    if let (Some(addressed), Some((written, resolved))) = (addressed, definition_model)
        && model_was_substituted(config, resolved, answered)
    {
        let said = t!(
            delegate_model_substituted,
            definition = addressed.name(),
            model = written
        );
        reporter.notice(said.clone());
        state
            .notices
            .push(crate::skills::Notice::from_message(said));
    }
    // The same comparison for a model a loaded skill moved the turn onto, naming the skill and
    // the model as its file wrote it (SKILL-15). A delegate's turn is covered here too.
    if let Some((skill, written)) = &state.switched_by {
        let resolved = state.turn_model.as_deref().unwrap_or(&config.default_model);
        if model_was_substituted(config, resolved, answered) {
            let said = t!(skill_model_substituted, skill = skill, model = written);
            reporter.notice(said.clone());
            state
                .notices
                .push(crate::skills::Notice::from_message(said));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{RecordingReporter, Why};
    use bravebot_aichat::responses::protocol::Refusal;
    use bravebot_aichat::{AccountFailure, ChatError, RenewalFailure};
    use bravebot_core::capability::{Capability, CapabilitySet};
    use bravebot_core::event::RecordingSink;
    use bravebot_core::policy::{ReleasePlan, Routing};

    /// A reporter that keeps the closed value a move to the fallback model is handed, which is what a
    /// front end with words of its own for it is given.
    #[derive(Default)]
    struct Moves {
        why: Vec<Why>,
    }

    impl Reporter for Moves {
        fn todos(&mut self, _rows: Vec<bravebot_core::todo::Row>) {}

        fn fell_back(&mut self, _from: &str, _to: &str, why: Why) {
            self.why.push(why);
        }
    }

    /// A turn on `chatgpt/gpt-a` whose request failed with `error`, in a configuration whose fallback
    /// is `chatgpt/gpt-b`, and the model it is on afterwards and whether it asks again.
    fn after_a_failure<R: Reporter>(
        error: BackendError,
        reporter: &mut R,
    ) -> (Option<String>, bool) {
        let mut config = Config::from_lookup(|key| match key {
            "SERVICES_KEY_AICHAT" => Some("test-key".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
            "BRAVE_AI_CHAT_ENDPOINT" => Some("http://127.0.0.1:9".into()),
            _ => None,
        })
        .expect("config");
        config.fallback_model = Some("chatgpt/gpt-b".to_string());
        let mut routing = Routing::new();
        routing.insert_trusted("task", "ask a model");
        let mut sink = RecordingSink::new();
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .expect("a policy");
        let mut turn_model = Some("chatgpt/gpt-a".to_string());
        let recovered = recover_from_failed_request(
            Err(error),
            &ChatRequest::new("chatgpt/gpt-a", vec![Message::user("hello")]),
            &config,
            &mut policy,
            &mut Conversation::new(),
            reporter,
            0,
            &mut turn_model,
            &mut Retries::default(),
            true,
            false,
            true,
        );
        (turn_model, matches!(recovered, Ok(Recovery::Retry)))
    }

    /// CHATGPT-8 and BACKEND-53: a renewal of the account's sign-in that was rate-limited or met an
    /// unavailable service moves the turn to the fallback model, and the person is told that failure
    /// in the account's sentence and not only as the name of its category. The front end is handed the
    /// closed value, so the window can say it in its own words, and a front end with none of its own
    /// narrates it in the terminal's.
    ///
    /// The regressions it rejects are the category's name for a failure of the account, the failure
    /// handed on as a sentence, and a failure of any other service worded as the account's.
    #[test]
    fn a_move_to_the_fallback_after_the_accounts_failure_says_the_accounts_sentence() {
        for (failure, said) in [
            (
                RenewalFailure::RateLimited,
                "chatgpt/gpt-a failed (the ChatGPT sign-in could not be renewed: the request was rate-limited); this turn moves to chatgpt/gpt-b",
            ),
            (
                RenewalFailure::Unavailable,
                "chatgpt/gpt-a failed (the ChatGPT sign-in could not be renewed: the request was met with an unavailable service); this turn moves to chatgpt/gpt-b",
            ),
        ] {
            let account = AccountFailure::Renewal(failure);
            let error = || BackendError::from(ChatError::Account(account));

            let mut terminal = RecordingReporter::default();
            let (model, retried) = after_a_failure(error(), &mut terminal);
            assert!(retried, "{failure:?}");
            assert_eq!(model.as_deref(), Some("chatgpt/gpt-b"), "{failure:?}");
            assert_eq!(terminal.narration, [said], "{failure:?}");

            let mut window = Moves::default();
            after_a_failure(error(), &mut window);
            assert_eq!(window.why, [Why::Account(account)], "{failure:?}");
        }

        // A 503 from the service is not the account's own failure: the category says it, as before.
        let mut terminal = RecordingReporter::default();
        let (_, retried) = after_a_failure(
            BackendError::from(ChatError::Egress(bravebot_net::EgressError::Status {
                url: "https://api.example.invalid/v1/responses".to_string(),
                status: 503,
                retry_after: None,
                body: None,
            })),
            &mut terminal,
        );
        assert!(retried);
        assert_eq!(
            terminal.narration,
            ["chatgpt/gpt-a failed as unavailable; this turn moves to chatgpt/gpt-b"]
        );

        // A refusal that names the account is a category no fallback answers for, so no move is made
        // and nothing is said of one.
        let refusal = || {
            BackendError::from(ChatError::Account(AccountFailure::Named {
                refusal: Refusal::UsageLimit,
                status: Some(429),
                retry_after: None,
            }))
        };
        let mut terminal = RecordingReporter::default();
        let (model, retried) = after_a_failure(refusal(), &mut terminal);
        assert!(!retried);
        assert_eq!(model.as_deref(), Some("chatgpt/gpt-a"));
        assert!(terminal.narration.is_empty());
        let mut window = Moves::default();
        after_a_failure(refusal(), &mut window);
        assert!(window.why.is_empty());
    }
}
