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
use bravebot_core::delegate::DelegateId;
use bravebot_core::event::{Event, RecordingSink, Sink};
use std::io::Write;

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
#[derive(Default, PartialEq)]
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
    /// The definition the run's turn was addressed to, by the name its kernel matched, or `None`
    /// where the turn was the planner's or ended with no outcome to say (CLI-17).
    pub agent: Option<&'a str>,
    /// The id of the session record this run wrote, which `--resume` takes (CLI-25), or `None`
    /// where the run wrote none: it was incognito, it failed, or the record could not be written.
    pub session: Option<&'a str>,
    pub steps: usize,
    pub tokens: Tokens,
    pub calls: &'a [Call],
    pub refusals: &'a [Refusal],
    /// The driver's own words about what loaded and what did not.
    pub notices: &'a [String],
    /// The reply as one line of JSON, where `--output-schema` was given and the reply matched it
    /// (CLI-28). Already JSON, written as it is.
    pub structured: Option<&'a str>,
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
    let calls = object_list(report.calls.iter().map(|call| object(&call_pairs(call))));
    let refusals = object_list(
        report
            .refusals
            .iter()
            .map(|refusal| object(&refusal_pairs(refusal))),
    );

    object(&[
        ("schema", SCHEMA.to_string()),
        ("ok", report.ending.ok().to_string()),
        ("status", report.ending.status().to_string()),
        ("reason", quoted(report.ending.name())),
        ("identifier", maybe(report.ending.identifier().as_deref())),
        ("message", maybe(report.message)),
        ("reply", quoted(report.reply)),
        ("model", quoted(report.model)),
        ("agent", maybe(report.agent)),
        ("session", maybe(report.session)),
        ("steps", report.steps.to_string()),
        ("tokens", token_fields(&report.tokens)),
        ("calls", calls),
        ("refusals", refusals),
        ("notices", strings(report.notices)),
        (
            "structured",
            report.structured.unwrap_or("null").to_string(),
        ),
    ])
}

/// One call's fields, as the result object lists it and as the stream's `call` event carries it.
fn call_pairs(call: &Call) -> Vec<(&'static str, String)> {
    vec![
        ("tool", quoted(&call.tool)),
        ("target", quoted(&call.target)),
        ("refused", call.refused.to_string()),
    ]
}

/// One refusal's fields, shared the same way with the stream's `refusal` event.
fn refusal_pairs(refusal: &Refusal) -> Vec<(&'static str, String)> {
    vec![
        ("gate", quoted(refusal.gate)),
        ("principle", quoted(refusal.principle)),
        ("reason", quoted(&refusal.reason)),
    ]
}

/// The token counts, as the result object holds them and as the stream's `usage` event does.
fn token_fields(tokens: &Tokens) -> String {
    object(&[
        ("total", tokens.total.to_string()),
        ("output", tokens.output.to_string()),
        ("context", tokens.context.to_string()),
        ("cache_read", tokens.cache_read.to_string()),
        ("cache_written", tokens.cache_written.to_string()),
    ])
}

/// One event of the stream a `--json-stream` run writes before its result object.
///
/// The event's own fields are the ones the result object already carries for the same thing, so a
/// caller that reads the stream learns nothing a caller of `--json` would not learn at the end.
/// Carries the schema number of [`SCHEMA`] and follows its add-only rule.
fn event(kind: &str, mut fields: Vec<(&str, String)>) -> String {
    let mut all = vec![("schema", SCHEMA.to_string()), ("event", quoted(kind))];
    all.append(&mut fields);
    object(&all)
}

/// A tool call finished, refused or not.
pub fn call_event(call: &Call) -> String {
    event("call", call_pairs(call))
}

/// A gate refused something.
pub fn refusal_event(refusal: &Refusal) -> String {
    event("refusal", refusal_pairs(refusal))
}

/// The run's cumulative token usage changed.
pub fn usage_event(tokens: &Tokens) -> String {
    event("usage", vec![("tokens", token_fields(tokens))])
}

/// Somewhere a stream's lines go. A failed write is dropped, as it is for the reply: a closed
/// stdout is a caller that stopped reading, and the run should not stop for it.
pub struct Stream(Box<dyn std::io::Write + Send>);

impl Stream {
    pub fn new(out: impl std::io::Write + Send + 'static) -> Self {
        Self(Box::new(out))
    }

    /// One line, flushed, so a caller following the run sees it as it happens.
    pub fn line(&mut self, line: &str) {
        let _ = writeln!(self.0, "{line}");
        let _ = self.0.flush();
    }
}

/// A trail that also writes each refusal to a stream as the gate takes it.
///
/// Holds the same events a plain [`RecordingSink`] would, so the result object and `--trace` read
/// from it exactly as they do without the stream.
#[derive(Default)]
pub struct Streaming {
    recorded: RecordingSink,
    stream: Option<Stream>,
}

impl Streaming {
    pub fn new(stream: Option<Stream>) -> Self {
        Self {
            recorded: RecordingSink::new(),
            stream,
        }
    }

    pub fn recorded(&self) -> &RecordingSink {
        &self.recorded
    }
}

impl Sink for Streaming {
    fn emit(&mut self, event: Event) {
        if let (
            Some(stream),
            Event::GateBlocked {
                gate,
                reason,
                principle,
                ..
            },
        ) = (self.stream.as_mut(), &event)
        {
            stream.line(&refusal_event(&Refusal {
                gate,
                principle: principle.name(),
                reason: reason.clone(),
            }));
        }
        self.recorded.emit(event);
    }

    fn recording_for(&mut self, delegate: Option<DelegateId>) {
        self.recorded.recording_for(delegate);
    }
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
            agent: None,
            session: None,
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
            structured: None,
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
            agent: None,
            session: None,
            steps: 0,
            tokens: Tokens::default(),
            calls: &[],
            refusals: &[],
            notices: &[],
            structured: None,
        });

        assert_eq!(
            written,
            concat!(
                r#"{"schema":1,"ok":false,"status":3,"reason":"configuration","#,
                r#""identifier":"BB1003","message":"l'adresse n'a pas de schema","#,
                r#""reply":"","model":"","agent":null,"session":null,"steps":0,"#,
                r#""tokens":{"total":0,"output":0,"context":0,"cache_read":0,"cache_written":0},"#,
                r#""calls":[],"refusals":[],"notices":[],"structured":null}"#,
            )
        );
    }

    /// The value is already JSON, so it is written as itself and not quoted into a string, which
    /// is the difference between a field a script indexes and one it has to parse again.
    #[test]
    fn a_structured_reply_is_a_value_and_not_a_string() {
        let mut report = finished(Ending::Done, &[], &[]);
        report.structured = Some(r#"{"verdict":"pass"}"#);
        let written = render(&report);

        assert!(
            written.ends_with(r#""notices":[],"structured":{"verdict":"pass"}}"#),
            "{written}"
        );
        assert!(
            render(&finished(Ending::Done, &[], &[])).ends_with(r#""structured":null}"#),
            "a run given no schema says so with a null"
        );
    }

    /// The id is what a script hands to `--resume`, so it is the one string in the object that is a
    /// reference to something else, and it has to arrive as itself rather than quoted away.
    #[test]
    fn a_recorded_run_names_its_session() {
        let mut report = finished(Ending::Done, &[], &[]);
        report.session = Some("1787860306-65099");
        let written = render(&report);

        assert!(
            written.contains(r#""agent":null,"session":"1787860306-65099","steps":2"#),
            "{written}"
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

    /// A writer a test can read back after the stream has been handed it.
    #[derive(Clone, Default)]
    struct Shared(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Shared {
        fn written(&self) -> String {
            String::from_utf8(self.0.lock().expect("lock").clone()).expect("utf-8")
        }
    }

    /// An event is the fields the result object holds for the same thing, so a caller following
    /// the stream and one reading the last line parse the same names. Each is checked against the
    /// object's own rendering rather than against a second copy of the field names.
    #[test]
    fn an_event_carries_the_fields_the_result_object_carries_for_the_same_thing() {
        let call = Call {
            tool: "write_file".to_string(),
            target: "notes.md".to_string(),
            refused: true,
        };
        let refusal = Refusal {
            gate: "action",
            principle: "integrity-gate",
            reason: "injection blocked".to_string(),
        };
        let tokens = Tokens {
            total: 9,
            output: 2,
            context: 7,
            cache_read: 1,
            cache_written: 0,
        };
        let object = render(&finished(
            Ending::Done,
            std::slice::from_ref(&call),
            std::slice::from_ref(&refusal),
        ));

        let call_line = call_event(&call);
        assert_eq!(
            call_line,
            r#"{"schema":1,"event":"call","tool":"write_file","target":"notes.md","refused":true}"#
        );
        assert!(
            object
                .contains(&call_line[r#"{"schema":1,"event":"call","#.len()..call_line.len() - 1])
        );
        let refusal_line = refusal_event(&refusal);
        assert_eq!(
            refusal_line,
            r#"{"schema":1,"event":"refusal","gate":"action","principle":"integrity-gate","reason":"injection blocked"}"#
        );
        assert!(object.contains(
            &refusal_line[r#"{"schema":1,"event":"refusal","#.len()..refusal_line.len() - 1]
        ));
        assert_eq!(
            usage_event(&tokens),
            r#"{"schema":1,"event":"usage","tokens":{"total":9,"output":2,"context":7,"cache_read":1,"cache_written":0}}"#
        );
        // The result object is the one line that has no `event`.
        assert!(!object.contains(r#""event""#), "{object}");
    }

    /// A refusal is written when the gate takes it, which is what lets a caller following the run
    /// see it before the run ends, and the trail the object and `--trace` read from still holds it.
    #[test]
    fn a_refusal_is_streamed_when_the_gate_takes_it() {
        let out = Shared::default();
        let mut sink = Streaming::new(Some(Stream::new(out.clone())));

        sink.emit(Event::GatePassed {
            gate: "action",
            detail: "read_file".to_string(),
        });
        assert_eq!(out.written(), "", "a passing gate is not a refusal");

        sink.emit(Event::GateBlocked {
            gate: "action",
            detail: "write_file".to_string(),
            reason: "injection blocked".to_string(),
            principle: bravebot_core::event::Principle::IntegrityGate,
        });

        assert_eq!(
            out.written(),
            concat!(
                r#"{"schema":1,"event":"refusal","gate":"action","principle":"integrity-gate","#,
                r#""reason":"injection blocked"}"#,
                "\n"
            )
        );
        assert_eq!(sink.recorded().blocked().count(), 1);
        assert_eq!(sink.recorded().events().len(), 2);
    }

    /// Without the flag nothing is written, and the trail is kept all the same.
    #[test]
    fn a_run_without_a_stream_writes_no_events() {
        let mut sink = Streaming::new(None);
        sink.emit(Event::GateBlocked {
            gate: "action",
            detail: "write_file".to_string(),
            reason: "injection blocked".to_string(),
            principle: bravebot_core::event::Principle::IntegrityGate,
        });
        assert_eq!(sink.recorded().blocked().count(), 1);
    }
}
