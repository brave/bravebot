//! How a finished turn, or a run stopped before one, is written out.

use crate::exit::{Ending, fail};
use crate::json;
use crate::usage::print_help;
use bravebot_core::event::{Event, RecordingSink, Role};
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
    /// The trail and the model behind it, when `--trace` asked for them.
    pub(crate) trail: Option<(&'a RecordingSink, &'a str)>,
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
        let said = if run.ending == Ending::Failed {
            Ending::Failed.told(complaint)
        } else {
            complaint.to_string()
        };
        let _ = writeln!(beside, "{said}");
    }
    let _ = writeln!(reply, "{}", run.reply);
    if let Some(attempt) = run.attempt {
        let _ = writeln!(beside);
        let _ = write!(beside, "{attempt}");
    }
    if let Some((sink, model)) = run.trail {
        let _ = writeln!(beside);
        print_trace(beside, sink);
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
pub(crate) fn say_notices(beside: &mut impl Write, notices: &[String]) {
    for notice in notices {
        let _ = writeln!(beside, "{}", t!(cli_notice, notice = notice));
    }
}

/// What a manifest run that stopped leaves for a person to read (MANIFEST-3): the plan or the
/// document that would not become one, and the steps that ran. Only the audit trail waits on
/// `--trace`; the attempt is printed either way, since a failed plan with no plan to look at is a
/// one-line complaint about a document nobody can see.
pub(crate) fn say_what_a_stopped_manifest_produced(
    output: &mut impl Write,
    attempt: &bravebot_agent::manifest::Attempt,
    trail: Option<&RecordingSink>,
) {
    let report = attempt.describe();
    if !report.is_empty() {
        let _ = writeln!(output);
        let _ = write!(output, "{report}");
    }
    if let Some(trail) = trail {
        let _ = writeln!(output);
        print_trace(output, trail);
    }
}

/// Print the audit trail: what was checked, allowed, and refused.
///
/// A failed write is dropped, as it is for progress. The trail describes a turn that has already
/// happened, so a closed stderr means nobody is reading it, not that the run should die holding a
/// reply it has already produced.
pub(crate) fn print_trace(output: &mut impl Write, sink: &RecordingSink) {
    macro_rules! trace {
        ($($arg:tt)*) => {
            { let _ = writeln!(output, $($arg)*); }
        };
    }

    trace!("audit trail");
    for (delegate, event) in sink.recorded() {
        // Which run took the decision, in front of what it decided. A turn and the delegates it
        // spawned record into this one trail, and two delegates of the same kind decide alike.
        let run = match delegate {
            Some(delegate) => format!("{delegate} "),
            None => String::new(),
        };
        match event {
            Event::GatePassed { gate, detail } => trace!("  ok      {run}{gate}: {detail}"),
            Event::GateBlocked { gate, reason, .. } => trace!("  BLOCK   {run}{gate}: {reason}"),
            Event::Observed { capability, label } => {
                trace!("  observe {run}{capability} produced {label}")
            }
            Event::SlotWritten { slot, label } => trace!("  slot    {run}{slot} at {label}"),
            Event::SlotDeferred {
                slot,
                label,
                origin,
            } => trace!("  defer   {run}{slot} holds {origin}, unread, at {label}"),
            Event::Declassified { slot, from, to, .. } => {
                trace!("  release {run}{slot} {from} -> {to}")
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
                trace!("  {mark} {run}{tool}.{field} [{role}] {label}");
            }
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

/// The result object for a run with no reply to report.
///
/// Written even where nothing ran at all, because the alternative is a caller having to tell an
/// empty stdout from a result, which is the prose surface again with an extra step. A run that
/// failed part way through still says what it had done by then: the calls are what a caller needs
/// to know which of them to undo, and the notices are what it was told on the way, there being no
/// outcome left to carry them (HOOK-7).
pub(crate) fn what_ran(
    ending: Ending,
    message: &str,
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
        steps: 0,
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
