//! `/loop` typed in the window, and the list of a session's loops the window draws.
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
use bravebot_agent::repeat::{self, Asked, Held, Loops, Pacing, Running};
use bravebot_i18n::t;
use serde_json::{Value, json};
use std::time::Instant;

/// Carry out `/loop <argument>` against a session's loops.
///
/// `turn_running` is whether the session has a turn in flight. A start is refused then with
/// `TurnInFlight`, so the window holds the line until the turn ends (LOOP-6, CMD-8): the first
/// tick of a loop is a turn of its own. The bare form and `stop` are answered at once, and `stop`
/// leaves the turn in flight running (LOOP-11).
pub fn command(
    argument: &str,
    loops: &mut Loops,
    turn_running: bool,
    now: Instant,
) -> Result<Value, Failure> {
    match repeat::parse(argument) {
        Asked::Report => Ok(report(loops, now)),
        Asked::Stop(which) => Ok(stop(loops, which)),
        Asked::Unreadable => Ok(said(&[t!(loop_needs_a_prompt).to_string()], true)),
        Asked::Start(request) => {
            if turn_running {
                return Err(Failure::new(
                    ErrorCode::TurnInFlight,
                    t!(loop_busy).to_string(),
                ));
            }
            // Refused rather than replacing one: the person typed every loop that is live
            // (LOOP-16).
            if loops.is_full() {
                return Ok(said(&[t!(loop_full, count = repeat::MAX_LIVE)], true));
            }
            Ok(start(loops, request))
        }
    }
}

/// Record a loop and say what it is. Numbered before anything is said, so the answer names the
/// number `/loop stop` takes.
fn start(loops: &mut Loops, request: repeat::Request) -> Value {
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
    answer
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
/// status view, with the seconds until the soonest tick where one is armed.
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
