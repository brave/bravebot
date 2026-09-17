//! The result object a `--json` run puts on stdout.
//!
//! The prose reply is written for a person, so everything a program would want from a run is
//! either absent from it or has to be recovered by reading English. This is the same run, said
//! once, in fields a caller can index: how it ended, what it cost, which tools it ran and what
//! each of them acted on, and what a gate refused.
//!
//! Written by hand rather than derived. The shape is a published interface, so it is worth
//! reading in the source exactly as a consumer will read it, and the crate carries no
//! serialisation dependency to spend on one flat object.

use crate::exit::Ending;

/// The number in the `schema` field.
///
/// Fields are added within a schema, never removed, renamed, or given a different meaning, so a
/// caller that reads the fields it knows keeps working. Anything that would break such a caller
/// takes the next number, and both are served for a release before the older one goes.
const SCHEMA: u32 = 1;

/// One tool call a turn made.
pub struct Call {
    /// The tool's own name, never the word a person is shown: a caller matching on the localised
    /// verb would be matching on the reader's language.
    pub tool: String,
    /// What it acted on, as the model named it.
    ///
    /// A name rather than a resolved path, and the difference shows: a call naming a reference to
    /// a file says the reference, and a manifest step says what the step was. There is nothing
    /// further back to ask, since the driver carries the argument without reading it, so a field
    /// promising paths would be right most of the time and wrong without saying which.
    pub target: String,
    /// Whether it was refused or failed.
    pub refused: bool,
}

/// One thing a gate refused.
pub struct Refusal {
    /// Which gate refused.
    pub gate: &'static str,
    /// Which principle the refusal upholds.
    pub principle: &'static str,
    /// Why, in the words the gate refused with.
    pub reason: String,
}

/// What the turn cost.
#[derive(Default)]
pub struct Tokens {
    pub total: u64,
    pub output: u64,
    pub context: u64,
    pub cache_read: u64,
    pub cache_written: u64,
}

/// Everything a `--json` run has to say.
///
/// Every field is written on every run, including the ones a failure before the turn has nothing
/// to put in. A caller then needs no presence check to read one, and the absence of a reply is
/// said by the status rather than by a missing key.
pub struct Report<'a> {
    pub ending: Ending,
    /// What went wrong, in the reader's own language. `None` where nothing did.
    pub message: Option<&'a str>,
    pub reply: &'a str,
    pub model: &'a str,
    pub steps: usize,
    pub tokens: Tokens,
    pub calls: &'a [Call],
    pub refusals: &'a [Refusal],
    /// The driver's own words about what loaded and what did not.
    pub notices: &'a [String],
}

/// One JSON string, quoted and escaped.
///
/// Every control character is escaped, JSON's own set and the rest alike, so a reply carrying a
/// terminal escape cannot repaint the screen of whoever pipes this into `jq` and reads the result.
/// Everything else goes through as itself: the output is UTF-8, and escaping it to ASCII would
/// only make a path harder to read.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A JSON array of strings.
fn strings<S: AsRef<str>>(values: impl IntoIterator<Item = S>) -> String {
    let items: Vec<String> = values
        .into_iter()
        .map(|value| quoted(value.as_ref()))
        .collect();
    format!("[{}]", items.join(","))
}

/// A JSON object from fields already rendered, in the order given.
fn object(fields: &[(&str, String)]) -> String {
    let pairs: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("{}:{value}", quoted(name)))
        .collect();
    format!("{{{}}}", pairs.join(","))
}

/// `null`, or the value quoted.
fn maybe(value: Option<&str>) -> String {
    match value {
        Some(text) => quoted(text),
        None => "null".to_string(),
    }
}

/// The whole result, on one line.
///
/// One line so that a run can be appended to a log and read back a record at a time, which is how
/// a caller collecting several of them will want it.
pub fn render(report: &Report<'_>) -> String {
    let calls = object_list(report.calls.iter().map(|call| {
        object(&[
            ("tool", quoted(&call.tool)),
            ("target", quoted(&call.target)),
            ("refused", call.refused.to_string()),
        ])
    }));
    let refusals = object_list(report.refusals.iter().map(|refusal| {
        object(&[
            ("gate", quoted(refusal.gate)),
            ("principle", quoted(refusal.principle)),
            ("reason", quoted(&refusal.reason)),
        ])
    }));

    object(&[
        ("schema", SCHEMA.to_string()),
        ("ok", report.ending.ok().to_string()),
        ("status", report.ending.status().to_string()),
        ("reason", quoted(report.ending.name())),
        ("identifier", maybe(report.ending.identifier().as_deref())),
        ("message", maybe(report.message)),
        ("reply", quoted(report.reply)),
        ("model", quoted(report.model)),
        ("steps", report.steps.to_string()),
        (
            "tokens",
            object(&[
                ("total", report.tokens.total.to_string()),
                ("output", report.tokens.output.to_string()),
                ("context", report.tokens.context.to_string()),
                ("cache_read", report.tokens.cache_read.to_string()),
                ("cache_written", report.tokens.cache_written.to_string()),
            ]),
        ),
        ("calls", calls),
        ("refusals", refusals),
        ("notices", strings(report.notices)),
    ])
}

/// A JSON array of objects already rendered.
fn object_list(items: impl IntoIterator<Item = String>) -> String {
    let items: Vec<String> = items.into_iter().collect();
    format!("[{}]", items.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished<'a>(ending: Ending, calls: &'a [Call], refusals: &'a [Refusal]) -> Report<'a> {
        Report {
            ending,
            message: None,
            reply: "done",
            model: "qwen-3-235b",
            steps: 2,
            tokens: Tokens {
                total: 4096,
                output: 128,
                context: 3000,
                cache_read: 900,
                cache_written: 100,
            },
            calls,
            refusals,
            notices: &[],
        }
    }

    /// The defect this exists for: a caller could learn nothing from a run but the reply and a
    /// status of 1. Every assertion here is a question a script asked and had to parse English
    /// for.
    #[test]
    fn a_finished_run_says_what_it_did_in_fields_a_program_can_read() {
        let calls = [
            Call {
                tool: "read_file".to_string(),
                target: "src/main.rs".to_string(),
                refused: false,
            },
            Call {
                tool: "edit_file".to_string(),
                target: "src/main.rs".to_string(),
                refused: false,
            },
            Call {
                tool: "write_file".to_string(),
                target: "notes.md".to_string(),
                refused: true,
            },
        ];
        let written = render(&finished(Ending::Done, &calls, &[]));

        assert!(written.contains(r#""schema":1"#), "{written}");
        assert!(written.contains(r#""ok":true"#), "{written}");
        assert!(written.contains(r#""status":0"#), "{written}");
        assert!(written.contains(r#""reason":"done""#), "{written}");
        assert!(written.contains(r#""model":"qwen-3-235b""#), "{written}");
        assert!(written.contains(r#""steps":2"#), "{written}");
        assert!(
            written.contains(
                r#""tokens":{"total":4096,"output":128,"context":3000,"cache_read":900,"cache_written":100}"#
            ),
            "{written}"
        );
        assert!(
            written.contains(
                r#""calls":[{"tool":"read_file","target":"src/main.rs","refused":false}"#
            ),
            "{written}"
        );
        // A refused call is in the list and says so, which is what tells a caller the write it
        // asked for did not happen.
        assert!(
            written.contains(r#"{"tool":"write_file","target":"notes.md","refused":true}"#),
            "{written}"
        );
    }

    /// A failure before the turn is a result too. The alternative is a caller that has to tell an
    /// empty stdout from a result object, which is the prose surface again with more steps.
    ///
    /// The shape is pinned whole here because it is published: a field renamed or dropped is a
    /// consumer broken, and that is a line in a diff rather than something to notice later.
    #[test]
    fn a_failure_before_the_turn_is_still_a_result_object() {
        let written = render(&Report {
            ending: Ending::Configuration,
            message: Some("l'adresse n'a pas de schema"),
            reply: "",
            model: "",
            steps: 0,
            tokens: Tokens::default(),
            calls: &[],
            refusals: &[],
            notices: &[],
        });

        assert_eq!(
            written,
            concat!(
                r#"{"schema":1,"ok":false,"status":3,"reason":"configuration","#,
                r#""identifier":"BB1003","message":"l'adresse n'a pas de schema","#,
                r#""reply":"","model":"","steps":0,"#,
                r#""tokens":{"total":0,"output":0,"context":0,"cache_read":0,"cache_written":0},"#,
                r#""calls":[],"refusals":[],"notices":[]}"#,
            )
        );
    }

    /// Which principle a refusal upholds is what tells a caller whether to retry, ask somebody, or
    /// stop: it is the difference between an injection blocked and a capability never granted.
    #[test]
    fn a_refusal_names_the_principle_it_upholds() {
        let refusals = [Refusal {
            gate: "action",
            principle: "integrity-gate",
            reason: "injection blocked: routing field 'path' of 'write_file'".to_string(),
        }];
        let written = render(&finished(Ending::Refused, &[], &refusals));

        assert!(written.contains(r#""status":4"#), "{written}");
        assert!(written.contains(r#""identifier":"BB1004""#), "{written}");
        assert!(
            written.contains(
                r#""refusals":[{"gate":"action","principle":"integrity-gate","reason":"injection blocked"#
            ),
            "{written}"
        );
    }

    /// A reply is model output and a target is a name the model wrote, so both are content this
    /// run did not choose. A quote in either would end the string it is written in and leave the
    /// caller reading an object this program did not mean.
    #[test]
    fn content_cannot_break_out_of_the_object_it_is_written_in() {
        let calls = [Call {
            tool: "write_file".to_string(),
            target: r#""},"ok":true,"x":""#.to_string(),
            refused: false,
        }];
        let mut report = finished(Ending::Done, &calls, &[]);
        report.reply = "a \"quoted\" \\ reply\nwith a newline\u{1b}[2K";
        let written = render(&report);

        assert!(
            written.contains(r#""reply":"a \"quoted\" \\ reply\nwith a newline\u001b[2K""#),
            "{written}"
        );
        assert!(
            written.contains(r#""target":"\"},\"ok\":true,\"x\":\"""#),
            "{written}"
        );
        // One `"ok":` and one `"status":`, because a second of either is content that wrote a
        // field of its own and a caller reading the last wins.
        assert_eq!(written.matches(r#""ok":"#).count(), 1, "{written}");
    }
}
