//! `/loop` and `/after` typed in the window, and the list of a session's loops the window draws.
//!
//! The argument is read by [`bravebot_agent::repeat::parse`], the function the terminal's `/loop`
//! reads it with, and the session's loops are the same [`Loops`] the terminal holds (LOOP-15,
//! LOOP-16). Only a line a person typed into the composer reaches [`command`] (CMD-1): the window
//! sends it as `loop.command`, which the host's allowlist does not offer.
//!
//! The lines an answer carries are drawn in the transcript and never sent to the model. A loop's
//! line is whatever somebody typed, so each is pictured with
//! [`bravebot_agent::confirm::printable`] before it is answered.

use crate::protocol::{ErrorCode, Failure};
use bravebot_agent::repeat::{self, Asked, Delayed, Held, Loops, Pacing, Running};
use bravebot_agent::turn::Wakeup;
use bravebot_i18n::t;
use serde_json::{Value, json};
use std::time::Instant;

/// Carry out `/loop <argument>` against a session's loops.
///
/// `turn_running` is whether the session has a turn in flight. A start is refused then with
/// `TurnInFlight`, so the window holds the line until the turn ends (LOOP-6, CMD-8): the first
/// tick of a loop is a turn of its own. `ready` is whether the session could start a turn at all,
/// and a start is refused with its failure where it could not, so no loop is recorded whose first
/// tick has nowhere to go. The bare form and `stop` are answered at once, and `stop` leaves the
/// turn in flight running (LOOP-11).
///
/// Answers with the number of the loop it started, where it started one, so the caller sends that
/// loop's first tick at once (LOOP-4).
pub fn command(
    argument: &str,
    loops: &mut Loops,
    turn_running: bool,
    ready: Result<(), Failure>,
    now: Instant,
) -> Result<(Value, Option<usize>), Failure> {
    match repeat::parse(argument) {
        Asked::Report => Ok((report(loops, now), None)),
        Asked::Stop(which) => Ok((stop(loops, which), None)),
        Asked::Unreadable => Ok((said(&[t!(loop_needs_a_prompt).to_string()], true), None)),
        Asked::Start(request) => {
            if turn_running {
                return Err(Failure::new(
                    ErrorCode::TurnInFlight,
                    t!(loop_busy).to_string(),
                ));
            }
            ready?;
            // Refused rather than replacing one: the person typed every loop that is live
            // (LOOP-16).
            if loops.is_full() {
                return Ok((said(&[t!(loop_full, count = repeat::MAX_LIVE)], true), None));
            }
            let (answer, number) = start(loops, request);
            Ok((answer, Some(number)))
        }
    }
}

/// Carry out `/after <argument>` against a session's loops (LOOP-18).
///
/// The line takes a place among the loops, so it has a number and counts towards the cap. Nothing
/// is sent now: the one tick is due when the delay is up and the session is idle, and the poll
/// clock sends it. So unlike `/loop` it is taken while a turn runs, and the delay counts from
/// `now`, when the person typed it. The clock sends the tick only to a session that can start a
/// turn, so nothing here asks whether this one can yet.
pub fn after(argument: &str, loops: &mut Loops, now: Instant) -> Value {
    let request = match repeat::parse_after(argument) {
        Delayed::Start(request) => request,
        Delayed::Unreadable => return said(&[t!(after_needs_a_delay).to_string()], true),
    };
    if loops.is_full() {
        return said(&[t!(loop_full, count = repeat::MAX_LIVE)], true);
    }
    let mut lines = Vec::new();
    let held = match request.adjusted {
        Some(Held::Raised(delay)) => {
            lines.push(t!(after_delay_raised, delay = repeat::spell(delay)));
            json!({"held": "raised", "every": repeat::spell(delay)})
        }
        Some(Held::Capped(delay)) => {
            lines.push(t!(after_delay_capped, delay = repeat::spell(delay)));
            json!({"held": "capped", "every": repeat::spell(delay)})
        }
        None => Value::Null,
    };
    let delay = match request.pacing {
        Pacing::Once(delay) => delay,
        _ => Default::default(),
    };
    let number = loops.start(Running::after(request, now));
    lines.push(t!(
        after_started,
        number = number,
        delay = repeat::spell(delay)
    ));
    let pace = loops
        .iter()
        .find(|running| running.number() == number)
        .map(Running::pace);
    let mut answer = said(&lines, false);
    answer["number"] = json!(number);
    answer["pace"] = json!(pace);
    answer["adjusted"] = held;
    answer
}

/// How a turn ended, as far as the session's loops need to know.
pub struct TurnEnd {
    /// What the turn asked for with `schedule_next`, where it finished and asked.
    pub wakeup: Option<Wakeup>,
    /// Whether the turn was stopped, which ends every loop (LOOP-11).
    pub cancelled: bool,
    /// The line the turn ran, where the person typed it and the turn was offered a later look at
    /// it (LOOP-14). `None` for a tick, and for a line this program composed.
    pub looking_again: Option<String>,
    /// Whether a standing watch is live once the turn's own watches are armed. A session does one
    /// of a watch and a loop at a time, and a turn that asked for both meant the watch.
    pub watching: bool,
}

/// Arm the next tick from the turn that has just ended, and give back the events that say what
/// changed.
///
/// Only the loop whose tick that turn was is touched. A turn the person typed in the middle of a
/// loop is not a tick of it, and its ending does not reset the clock (LOOP-6). The next tick is
/// measured from `now`, the end of the turn, and not from when the tick went (LOOP-5).
pub fn turn_ended(loops: &mut Loops, end: TurnEnd, now: Instant) -> Vec<(&'static str, Value)> {
    if end.cancelled {
        let count = loops.stop_all();
        return stopped(count);
    }
    if let Some(running) = loops.in_flight_mut() {
        let (number, once) = (running.number(), running.once());
        if running.ended(end.wakeup, now) {
            return Vec::new();
        }
        loops.stop(number);
        let reason = match end.wakeup {
            _ if once => "sent",
            Some(wakeup) if wakeup.stop => "finished",
            _ => "silent",
        };
        return vec![("loop.ended", json!({"number": number, "reason": reason}))];
    }
    // A turn that asked to look again while nothing loops starts a loop over the person's line,
    // with its first tick the wait away (LOOP-14).
    match (end.looking_again, end.wakeup) {
        (Some(line), Some(wakeup)) if !end.watching && !loops.is_full() => {
            let number = loops.start(Running::armed(line, wakeup, now));
            vec![(
                "loop.armed",
                json!({"number": number, "after": repeat::spell(wakeup.after)}),
            )]
        }
        _ => Vec::new(),
    }
}

/// The event that says a stop ended every loop, with how many went (LOOP-11). Nothing where none
/// was live.
pub fn stopped(count: usize) -> Vec<(&'static str, Value)> {
    match count {
        0 => Vec::new(),
        count => vec![("loop.ended", json!({"reason": "stopped", "count": count}))],
    }
}

/// Record a loop and say what it is. Numbered before anything is said, so the answer names the
/// number `/loop stop` takes.
fn start(loops: &mut Loops, request: repeat::Request) -> (Value, usize) {
    let (adjusted, pacing) = (request.adjusted, request.pacing);
    let number = loops.start(Running::begin(request));
    let mut lines = Vec::new();
    // Said before the start, so a person who typed `1s` reads what the interval became (LOOP-7).
    let held = match adjusted {
        Some(Held::Raised(every)) => {
            lines.push(t!(loop_interval_raised, every = repeat::spell(every)));
            json!({"held": "raised", "every": repeat::spell(every)})
        }
        Some(Held::Capped(every)) => {
            lines.push(t!(loop_interval_capped, every = repeat::spell(every)));
            json!({"held": "capped", "every": repeat::spell(every)})
        }
        None => Value::Null,
    };
    match pacing {
        Pacing::Every(every) => lines.push(t!(
            loop_started_every,
            number = number,
            every = repeat::spell(every)
        )),
        Pacing::SelfPaced => lines.push(t!(loop_started_self_paced, number = number)),
        // `/loop` never reads a line sent once; that is `/after`.
        Pacing::Once(_) => {}
    }
    let pace = loops
        .iter()
        .find(|running| running.number() == number)
        .map(Running::pace);
    let mut answer = said(&lines, false);
    answer["number"] = json!(number);
    answer["pace"] = json!(pace);
    answer["adjusted"] = held;
    (answer, number)
}

/// The bare `/loop`: each loop under its number, then how to end one, or that nothing repeats
/// (LOOP-13).
fn report(loops: &Loops, now: Instant) -> Value {
    if loops.is_empty() {
        return said(&[t!(loop_none).to_string()], false);
    }
    let mut lines: Vec<String> = loops
        .iter()
        .map(|running| match running.once() {
            true => t!(
                after_active,
                number = running.number(),
                prompt = &first_line(running.prompt()),
                when = &running.when(now)
            ),
            false => t!(
                loop_active,
                number = running.number(),
                prompt = &first_line(running.prompt()),
                pace = &running.pace(),
                when = &running.when(now)
            ),
        })
        .collect();
    lines.push(t!(loop_ends_with).to_string());
    said(&lines, false)
}

/// `/loop stop` and `/loop stop <number>` (LOOP-15). A bare `stop` with several live ends none of
/// them and says it needs a number.
fn stop(loops: &mut Loops, which: Option<usize>) -> Value {
    if loops.is_empty() {
        return said(&[t!(loop_none).to_string()], true);
    }
    match which {
        None if loops.len() > 1 => said(&[t!(loop_stop_which, count = loops.len())], true),
        None => {
            loops.stop_all();
            said(&[t!(loop_stopped).to_string()], false)
        }
        Some(number) if loops.stop(number) => {
            said(&[t!(loop_stopped_numbered, number = number)], false)
        }
        Some(number) => said(&[t!(loop_no_such, number = number)], true),
    }
}

/// `loops.list`: every loop the session holds, for the window's row beside the composer and its
/// status view, with the seconds until the soonest tick where one is armed. Each row also carries
/// its own seconds as `until`, so the window can count loops and the lines `/after` holds apart, as
/// the terminal's row does.
pub fn list(loops: &Loops, now: Instant) -> Value {
    let rows: Vec<Value> = loops
        .iter()
        .map(|running| {
            json!({
                "number": running.number(),
                "prompt": first_line(running.prompt()),
                "pace": running.pace(),
                "when": running.when(now),
                "ticks": running.ticks(),
                "quiet": running.quiet(),
                "once": running.once(),
                "until": running.until(now).map(|left| left.as_secs()),
            })
        })
        .collect();
    json!({
        "loops": rows,
        "soonest": loops.soonest(now).map(|left| left.as_secs()),
    })
}

/// A loop's line as a row shows it: its first line, pictured. The rest of a pasted line would
/// arrive as a wall of transcript nobody asked for.
fn first_line(prompt: &str) -> String {
    bravebot_agent::confirm::printable(prompt.lines().next().unwrap_or_default())
}

fn said(lines: &[String], refused: bool) -> Value {
    let pictured: Vec<String> = lines
        .iter()
        .map(|line| bravebot_agent::confirm::printable(line))
        .collect();
    json!({ "lines": pictured, "refused": refused, "number": null, "pace": null, "adjusted": null })
}
