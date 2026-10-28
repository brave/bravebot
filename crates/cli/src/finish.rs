//! How a finished turn, or a run stopped before one, is written out.

use crate::exit::{Ending, fail};
use crate::goal_run::Unmet;
use crate::json;
use crate::usage::print_help;
use bravebot_core::delegate::DelegateId;
use bravebot_core::event::{Event, Principle, RecordingSink, Role};
use bravebot_i18n::t;
use std::io::Write;
use std::process::ExitCode;

/// What a finished turn has to say, before anything decides where it goes.
pub(crate) struct Finished<'a> {
    /// What goes on stdout: the released reply, or the result object `--json` asked for in its
    /// place.
    pub(crate) reply: &'a str,
    /// The driver's own words about what loaded and what did not, never anything read out of a
    /// file.
    pub(crate) notices: &'a [String],
    /// What a traced manifest run planned, when there was one. On stderr with the trail,
    /// never on stdout: a pipe of the reply must not pick up the plan.
    pub(crate) attempt: Option<&'a str>,
    /// The model behind the run, named where `--trace` asked for the trail. The trail itself was
    /// written line by line as the gates ran (TRACE-5), so it is not part of what is reported here.
    pub(crate) model: Option<&'a str>,
    /// Whether no gate refused anything during the turn.
    pub(crate) clean: bool,
    /// How the turn ended, which is what the substitution complaint's identifier is read from: it
    /// is a failure only where the command line named the model.
    pub(crate) ending: Ending,
    /// What to say where one model was asked for and another one answered.
    pub(crate) not_served: Option<&'a str>,
}

/// Write a finished turn: the reply to `reply`, every other word to `beside`.
///
/// Which stream each part lands on is the whole of what this decides, so it takes both rather
/// than reaching for stdout and stderr itself: a run's output is then something a test can read
/// back. A notice or an audit trail sharing the reply's stream would corrupt whatever the reply
/// was piped into.
pub(crate) fn report(reply: &mut impl Write, beside: &mut impl Write, run: &Finished<'_>) {
    say_notices(beside, run.notices);
    if let Some(complaint) = run.not_served {
        // Said with the identifier of the status it is the reason for (CLI-6), and plainly where
        // the run did not fail over it.
        // It names the model the service reported having served, which the service chose.
        let complaint = crate::progress::printable_lines(complaint);
        let said = if run.ending == Ending::Failed {
            Ending::Failed.told(complaint)
        } else {
            complaint
        };
        let _ = writeln!(beside, "{said}");
    }
    let _ = writeln!(reply, "{}", run.reply);
    if let Some(attempt) = run.attempt {
        let _ = writeln!(beside);
        let _ = write!(beside, "{}", crate::progress::printable_lines(attempt));
    }
    if let Some(model) = run.model {
        let _ = writeln!(beside);
        let model = crate::progress::printable(model);
        let _ = writeln!(beside, "{}", t!(cli_model_used, model = model));
    }
    if !run.clean {
        let _ = writeln!(beside);
        let _ = writeln!(
            beside,
            "{}",
            Ending::Refused.told(t!(cli_something_was_refused))
        );
    }
}

/// Say what the turn said about itself as it ran: which standing instructions and skills loaded,
/// and what a hook did that went wrong.
///
/// One function for all three surfaces that print these, so a turn that ended in a failure and one
/// that answered cannot come to word the same sentence differently. Never on the reply's stream:
/// these are the driver's own words and a pipe reading the reply must not pick them up.
///
/// A notice can name a file the checkout holds, so each is drawn through
/// [`crate::progress::printable_lines`] (LAYER-3).
pub(crate) fn say_notices(beside: &mut impl Write, notices: &[String]) {
    for notice in notices {
        let notice = crate::progress::printable_lines(notice);
        let _ = writeln!(beside, "{}", t!(cli_notice, notice = notice));
    }
}

/// What a manifest run that stopped leaves for a person to read (MANIFEST-3): the plan or the
/// document that would not become one, and the steps that ran. It is printed whether or not
/// `--trace` was asked for, since a failed plan with no plan to look at is a one-line complaint
/// about a document nobody can see.
pub(crate) fn say_what_a_stopped_manifest_produced(
    output: &mut impl Write,
    attempt: &bravebot_agent::manifest::Attempt,
) {
    let report = crate::progress::printable_lines(&attempt.describe());
    if !report.is_empty() {
        let _ = writeln!(output);
        let _ = write!(output, "{report}");
    }
}

/// What a manifest run that was written down produced, as `--resume` and `--fork` name it: the
/// same report as [`say_what_a_stopped_manifest_produced`], read back from the record, so the
/// plan, the steps and the reason it stopped are text a checkout or a model chose and are drawn
/// through [`crate::progress::printable_lines`] (LAYER-3).
pub(crate) fn say_what_a_stored_manifest_produced(
    output: &mut impl Write,
    stored: &bravebot_session::sessions::StoredManifest,
) {
    let report = crate::progress::printable_lines(&stored.describe());
    if !report.is_empty() {
        let _ = writeln!(output);
        let _ = write!(output, "{report}");
    }
}

/// The heading `--trace` writes before the first decision.
pub(crate) const TRAIL_HEADING: &str = "audit trail";

/// One line of the audit trail: what was checked, allowed, or refused, and by which run.
///
/// The whole line is drawn through [`crate::progress::printable`] (LAYER-3). A path, a listing
/// entry's name or a gate's reason is text a checkout can choose, and the line goes to a
/// terminal, so a control character in any of them is replaced by its picture. The trail is one
/// line per event, so a line break in a field is pictured too.
pub(crate) fn trace_line(delegate: Option<DelegateId>, event: &Event) -> String {
    crate::progress::printable(&unpictured_trace_line(delegate, event))
}

fn unpictured_trace_line(delegate: Option<DelegateId>, event: &Event) -> String {
    // Which run took the decision, in front of what it decided. A turn and the delegates it
    // spawned record into one trail, and two delegates of the same kind decide alike.
    let run = match delegate {
        Some(delegate) => format!("{delegate} "),
        None => String::new(),
    };
    match event {
        Event::GatePassed { gate, detail } => format!("  ok      {run}{gate}: {detail}"),
        Event::GateBlocked { gate, reason, .. } => format!("  BLOCK   {run}{gate}: {reason}"),
        Event::Observed { capability, label } => {
            format!("  observe {run}{capability} produced {label}")
        }
        Event::SlotWritten { slot, label } => format!("  slot    {run}{slot} at {label}"),
        Event::SlotDeferred {
            slot,
            label,
            origin,
        } => format!("  defer   {run}{slot} holds {origin}, unread, at {label}"),
        Event::Declassified { slot, from, to, .. } => {
            format!("  release {run}{slot} {from} -> {to}")
        }
        Event::ActionField {
            tool,
            field,
            role,
            label,
            allowed,
        } => {
            let mark = if *allowed { "ok     " } else { "BLOCK  " };
            let role = match role {
                Role::Routing => "routing",
                Role::Content => "content",
            };
            format!("  {mark} {run}{tool}.{field} [{role}] {label}")
        }
    }
}

/// How a turn that ran ended.
///
/// A turn something was refused in did not do what it was asked, and neither did one answered by
/// a model other than the one the command line named. Both are invisible to a script reading the
/// reply, which is what makes the status the only thing that can carry them.
pub(crate) fn ending_of_a_turn(
    clean: bool,
    named_on_the_command_line: bool,
    not_served: bool,
) -> Ending {
    if !clean {
        return Ending::Refused;
    }
    if named_on_the_command_line && not_served {
        return Ending::Failed;
    }
    Ending::Done
}

/// The sentence a result object explains a finished turn's failure with, if it has one.
///
/// The same words the run said on stderr, so the two surfaces do not disagree about why a turn
/// that produced a reply is still a failure.
pub(crate) fn failure_of_a_turn(ending: Ending, not_served: Option<&str>) -> Option<&str> {
    match ending {
        Ending::Done => None,
        Ending::Refused => Some(t!(cli_something_was_refused)),
        _ => not_served,
    }
}

/// How a run that worked towards a condition ends, given how its turns did (CLI-35).
///
/// An unmet condition is what a status 0 cannot say, so it decides only where the turns would have
/// ended well. A refusal or a substituted model outranks it, being what a person has to deal with
/// first, and [`say_the_goal_unmet`] still says the condition was not met.
pub(crate) fn ending_with_the_goal(turns: Ending, unmet: Option<&Unmet>) -> Ending {
    match (turns, unmet) {
        (Ending::Done, Some(why)) => why.ending(),
        _ => turns,
    }
}

/// The sentence a result object explains a run's failure with: why its turns failed, if they did,
/// and then why its condition went unmet, if it did.
pub(crate) fn failure_with_the_goal(
    failure: Option<&str>,
    unmet: Option<&Unmet>,
) -> Option<String> {
    match (failure, unmet) {
        (Some(failure), Some(why)) => Some(format!("{failure}\n{}", why.said())),
        (Some(failure), None) => Some(failure.to_string()),
        (None, Some(why)) => Some(why.said()),
        (None, None) => None,
    }
}

/// Say why a goal run's condition went unmet, beside the reply.
///
/// With the identifier of the status where that is the reason the run ended on it, and plainly where
/// another failure outranked it, as the complaint about a substituted model is.
pub(crate) fn say_the_goal_unmet(beside: &mut impl Write, ending: Ending, why: &Unmet) {
    let said = why.said();
    let _ = match ending == why.ending() {
        true => writeln!(beside, "{}", ending.told(said)),
        false => writeln!(beside, "{said}"),
    };
}

/// Every refusal the trail holds, as a result object lists them.
pub(crate) fn refusals(sink: &RecordingSink) -> Vec<json::Refusal> {
    sink.blocked()
        .filter_map(|event| match event {
            Event::GateBlocked {
                gate,
                reason,
                principle,
                ..
            } => Some(json::Refusal {
                gate,
                principle: principle.name(),
                reason: reason.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// The refusals a trail holds, then the questions nobody could be asked.
///
/// A plan or a `run` is not told to the policy, so the trail has no entry for it, and a result
/// object that said `"ok":false,"status":4` with nothing to point at would be a refusal a script
/// cannot read. A question the policy did record is left out, since the trail's entry already
/// lists it.
pub(crate) fn with_declined(
    mut refused: Vec<json::Refusal>,
    declined: &[crate::one_shot::Declined],
) -> Vec<json::Refusal> {
    refused.extend(
        declined
            .iter()
            .filter(|d| !d.in_the_trail)
            .map(|d| json::Refusal {
                gate: d.gate,
                principle: Principle::Capability.name(),
                reason: d.reason.to_string(),
            }),
    );
    refused
}

/// The result object for a run with no reply to report.
///
/// Written even where nothing ran at all, because the alternative is a caller having to tell an
/// empty stdout from a result, which is the prose surface again with an extra step. A run that
/// failed part way through still says what it had done by then: the calls are what a caller needs
/// to know which of them to undo, and the notices are what it was told on the way, there being no
/// outcome left to carry them (HOOK-7).
///
/// `steps` is the rounds of tool calls the run made where it knows them, which is a run stopped at
/// a limit (CLI-38), and zero for every other run with no outcome: a count the run has not got is
/// not invented.
pub(crate) fn what_ran(
    ending: Ending,
    message: &str,
    steps: usize,
    spent: bravebot_agent::Spent,
    calls: &[json::Call],
    refusals: &[json::Refusal],
    notices: &[String],
) -> String {
    json::render(&json::Report {
        ending,
        message: Some(message),
        reply: "",
        model: "",
        agent: None,
        session: None,
        steps,
        tokens: json::Tokens {
            total: spent.tokens,
            output: spent.output_tokens,
            context: spent.context_tokens,
            cache_read: spent.cached.read_tokens,
            cache_written: spent.cached.written_tokens,
        },
        calls,
        refusals,
        notices,
        structured: None,
    })
}

/// Put a result object on stdout.
///
/// A failed write is dropped, as it is for the reply: a closed stdout is a caller that stopped
/// reading, and a run that has already finished should not die reporting what it did.
pub(crate) fn say_the_result(result: &str) {
    let _ = writeln!(std::io::stdout().lock(), "{result}");
}

/// Stop before the turn: the identifier and the message on stderr, and a result object on stdout
/// where one was asked for.
pub(crate) fn stopped_before_the_turn(
    as_json: bool,
    ending: Ending,
    message: impl std::fmt::Display,
) -> ExitCode {
    let message = message.to_string();
    let stopped = fail(ending, &message);
    if as_json {
        say_the_result(&what_ran(
            ending,
            &message,
            0,
            Default::default(),
            &[],
            &[],
            &[],
        ));
    }
    stopped
}

/// A command line refused before it named a command, where the usage is the rest of the answer.
///
/// The table is what a person mistyping a flag needs, and it is written with `println!`, so it
/// lands on the stream CLI-12 promises holds the result object and nothing else. A run that asked
/// for an object therefore gets the object in its place: the two cannot share stdout, and prose a
/// caller has to tell apart from a result is the surface the flag exists to replace. The complaint
/// itself is on stderr either way, so nothing is lost to the person reading.
pub(crate) fn refused_with_the_usage(as_json: bool, message: impl std::fmt::Display) -> ExitCode {
    let refused = stopped_before_the_turn(as_json, Ending::Argument, message);
    if !as_json {
        print_help();
    }
    refused
}
