use super::arguments::{Scratch, with_tools};
use super::*;
use crate::confirm::{
    Confirmer, MessageDecision, MessageRequest, RunDecision, RunRequest, Unattended, WriteDecision,
    WriteRequest,
};
use crate::messages::{Courier, Delivery, Found, Ledger, Messaging, Target};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::{Event, RecordingSink};
use bravebot_core::label::Integrity;
use bravebot_core::policy::{ReleasePlan, Routing};
use std::collections::VecDeque;
use std::sync::Mutex;

/// What a roster answers to each name, and everything offered to a session.
#[derive(Debug)]
struct Sessions {
    found: Vec<(&'static str, Found)>,
    delivery: Delivery,
    looked_up: Mutex<Vec<String>>,
    delivered: Mutex<Vec<(String, String, String)>>,
}

impl Sessions {
    fn answering(found: Vec<(&'static str, Found)>) -> &'static Self {
        Self::delivering(found, Delivery::Queued)
    }

    fn delivering(found: Vec<(&'static str, Found)>, delivery: Delivery) -> &'static Self {
        Box::leak(Box::new(Self {
            found,
            delivery,
            looked_up: Mutex::default(),
            delivered: Mutex::default(),
        }))
    }

    fn looked_up(&self) -> Vec<String> {
        self.looked_up.lock().unwrap().clone()
    }

    fn delivered(&self) -> Vec<(String, String, String)> {
        self.delivered.lock().unwrap().clone()
    }
}

impl Courier for Sessions {
    fn own_id(&self) -> &str {
        "sender-id"
    }

    fn find(&self, name: &str) -> Found {
        self.looked_up.lock().unwrap().push(name.to_string());
        self.found
            .iter()
            .find(|(known, _)| *known == name)
            .map_or(Found::Missing, |(_, found)| found.clone())
    }

    fn deliver(&self, to: &Target, label: &str, text: &str) -> Delivery {
        self.delivered
            .lock()
            .unwrap()
            .push((to.id.clone(), label.to_string(), text.to_string()));
        self.delivery
    }
}

fn session(id: &str, name: &str) -> Found {
    Found::Session(Target {
        id: id.to_string(),
        name: name.to_string(),
        state: "needs-input".to_string(),
        directory: "/work/build".to_string(),
    })
}

/// A person who answers each message question in turn and declines once the answers run
/// out, and the questions they were asked.
struct Person {
    answers: VecDeque<MessageDecision>,
    asked: Vec<MessageRequest>,
}

impl Person {
    fn saying(answers: &[MessageDecision]) -> Self {
        Self {
            answers: answers.iter().copied().collect(),
            asked: Vec::new(),
        }
    }
}

impl Confirmer for Person {
    fn confirm_message(&mut self, request: &MessageRequest) -> MessageDecision {
        self.asked.push(request.clone());
        self.answers.pop_front().unwrap_or(MessageDecision::Decline)
    }
    fn confirm_write(&mut self, request: &WriteRequest) -> WriteDecision {
        Unattended.confirm_write(request)
    }
    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        Unattended.confirm_run(request)
    }
    fn confirm_read_output(&mut self, request: &crate::confirm::OutputRequest) -> Decision {
        Unattended.confirm_read_output(request)
    }
    fn confirm_vetted_read(&mut self, request: &crate::confirm::VetRequest) -> Decision {
        Unattended.confirm_vetted_read(request)
    }
    fn confirm_fetch(&mut self, request: &crate::confirm::FetchRequest) -> Decision {
        Unattended.confirm_fetch(request)
    }
    fn confirm_server(&mut self, request: &crate::confirm::ServerRequest) -> Decision {
        Unattended.confirm_server(request)
    }
    fn confirm_manifest(&mut self, request: &crate::confirm::ManifestRequest) -> Decision {
        Unattended.confirm_manifest(request)
    }
    fn confirm_vouch(&mut self, request: &crate::confirm::VouchRequest) -> Decision {
        Unattended.confirm_vouch(request)
    }
    fn confirm_exposing_read(&mut self, request: &crate::confirm::ExposureRequest) -> Decision {
        Unattended.confirm_exposing_read(request)
    }
    fn confirm_tool_list(&mut self, request: &crate::confirm::ToolListRequest) -> Decision {
        Unattended.confirm_tool_list(request)
    }
    fn confirm_mcp_call(
        &mut self,
        request: &crate::confirm::McpCallRequest,
    ) -> crate::confirm::CallDecision {
        Unattended.confirm_mcp_call(request)
    }
    fn confirm_path(&mut self, request: &crate::confirm::PathRequest) -> Decision {
        Unattended.confirm_path(request)
    }
    fn confirm_host(&mut self, request: &crate::confirm::HostRequest) -> Decision {
        Unattended.confirm_host(request)
    }
    fn confirm_move(&mut self, request: &crate::confirm::MoveRequest) -> Decision {
        Unattended.confirm_move(request)
    }
    fn ask_user(&mut self, asking: &bravebot_core::ask::Asking) -> Vec<bravebot_core::ask::Answer> {
        Unattended.ask_user(asking)
    }
    fn interjection(&mut self) -> Option<String> {
        Unattended.interjection()
    }
}

fn policy(sink: &mut RecordingSink) -> Policy<'_, RecordingSink> {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "tell the other session");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        sink,
    )
    .expect("policy")
}

fn call(target: &str, text: &str) -> ToolCall {
    serde_json::from_value(json!({
        "id": "1",
        "function": {
            "name": "message_session",
            "arguments": json!({"target": target, "text": text, "why": "it needs to know"})
                .to_string(),
        }
    }))
    .expect("a call")
}

/// What the screen was told of each call, in order: the note and whether it failed.
type Told = Vec<(String, bool)>;

/// Make `calls` in one turn against `sessions`, offered a way to send unless `offered` is
/// false, as a delegate's call when `delegated`.
fn put(
    policy: &mut Policy<'_, RecordingSink>,
    sessions: &'static Sessions,
    person: &mut Person,
    reporter: &mut crate::report::RecordingReporter,
    offered: bool,
    delegated: bool,
    calls: &[ToolCall],
) -> Told {
    // A directory of its own for each call: the tests run side by side in one process, and
    // a name they shared would be removed by whichever finished first.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let scratch = Scratch::new(&format!(
        "session-messages-{}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    // Leaked for the reason the advisor test leaks its context: the harness lends `Tools`
    // to the closure for a lifetime of its own.
    let ledger: &'static mut Ledger = Box::leak(Box::default());
    with_tools(&workspace, |tools| {
        tools.delegated = delegated;
        tools.messaging = offered.then_some(Messaging {
            courier: sessions,
            ledger,
        });
        for call in calls {
            dispatch(policy, tools, person, reporter, call);
        }
    });
    reporter
        .finished
        .iter()
        .map(|done| (done.note.clone().unwrap_or_default(), done.failed))
        .collect()
}

fn once(
    sessions: &'static Sessions,
    person: &mut Person,
    calls: &[ToolCall],
) -> (Told, crate::report::RecordingReporter) {
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    let mut reporter = crate::report::RecordingReporter::default();
    let told = put(
        &mut policy,
        sessions,
        person,
        &mut reporter,
        true,
        false,
        calls,
    );
    (told, reporter)
}

/// MSG-12: a session that was offered no way to send and a delegate that was handed one
/// anyway are each answered as any unknown name is, and neither looks at the roster or asks
/// anyone. The last case is the control: with both set right the same call is delivered, so
/// a dispatch that refused every call would not pass.
#[test]
fn a_call_to_message_session_nobody_was_offered_is_answered_as_an_unknown_name() {
    for (offered, delegated, sends) in [
        (false, false, false),
        (true, true, false),
        (true, false, true),
    ] {
        let sessions = Sessions::answering(vec![("build", session("build-id", "build"))]);
        let mut person = Person::saying(&[MessageDecision::Once]);
        let mut sink = RecordingSink::new();
        let mut policy = policy(&mut sink);
        let mut reporter = crate::report::RecordingReporter::default();
        let told = put(
            &mut policy,
            sessions,
            &mut person,
            &mut reporter,
            offered,
            delegated,
            &[call("build", "rotate the key")],
        );
        assert_eq!(
            sessions.delivered().len(),
            usize::from(sends),
            "offered {offered}, delegated {delegated}: {told:?}"
        );
        if !sends {
            assert_eq!(
                told,
                vec![("error: no such tool 'message_session'".to_string(), true)]
            );
            assert!(sessions.looked_up().is_empty(), "the roster was read");
            assert!(person.asked.is_empty(), "a person was asked");
        }
    }
}

/// MSG-1: a name that names no one session, the sender itself or a session nobody runs is
/// refused naming the name asked for, before anyone is asked, and each says why in words of
/// its own. A name left blank or no text is refused without reading the roster.
#[test]
fn a_target_that_is_not_one_live_other_session_is_refused_before_anyone_is_asked() {
    let sessions = Sessions::answering(vec![
        ("twice", Found::Ambiguous),
        ("me", Found::Itself),
        ("asleep", Found::NotLive),
    ]);
    let mut person = Person::saying(&[MessageDecision::Once; 8]);
    let (told, _) = once(
        sessions,
        &mut person,
        &[
            call("nobody", "hello"),
            call("twice", "hello"),
            call("me", "hello"),
            call("asleep", "hello"),
            call("  ", "hello"),
            call("nobody", ""),
        ],
    );
    let said: Vec<&str> = told.iter().map(|(note, _)| note.as_str()).collect();
    assert!(said[0].contains("no session is named 'nobody'"), "{said:?}");
    assert!(
        said[1].contains("more than one session is named 'twice'"),
        "{said:?}"
    );
    assert!(said[2].contains("'me' is this session"), "{said:?}");
    assert!(
        said[3].contains("no process is running the session named 'asleep'"),
        "{said:?}"
    );
    assert!(
        said[4].starts_with("error: 'target' and 'text' are required"),
        "{said:?}"
    );
    assert!(
        said[5].starts_with("error: 'target' and 'text' are required"),
        "{said:?}"
    );
    assert!(told.iter().all(|(_, failed)| *failed));
    assert_eq!(sessions.looked_up(), ["nobody", "twice", "me", "asleep"]);
    assert!(
        person.asked.is_empty(),
        "a person was asked about a refused call"
    );
    assert!(sessions.delivered().is_empty());
}

/// MSG-2: a context that has met something untrusted sends nothing, reads no roster and asks
/// nobody, so it learns nothing about which sessions exist. The control is the same call from
/// a clean context, which reaches the roster and the person.
#[test]
fn a_context_that_has_met_something_untrusted_sends_nothing_and_reads_no_roster() {
    for (fallen, reads) in [(true, false), (false, true)] {
        let sessions = Sessions::answering(vec![("build", session("build-id", "build"))]);
        let mut person = Person::saying(&[MessageDecision::Once]);
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "tell the other session");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::FileRead]),
            &mut sink,
        )
        .expect("policy");
        if fallen {
            policy = policy.resuming(Integrity::Untrusted);
        }
        let mut reporter = crate::report::RecordingReporter::default();
        let told = put(
            &mut policy,
            sessions,
            &mut person,
            &mut reporter,
            true,
            false,
            &[call("build", "rotate the key")],
        );
        assert_eq!(!sessions.looked_up().is_empty(), reads, "{told:?}");
        assert_eq!(!person.asked.is_empty(), reads, "{told:?}");
        assert_eq!(!sessions.delivered().is_empty(), reads, "{told:?}");
        if fallen {
            assert!(
                told[0]
                    .0
                    .starts_with("refused: this run's context has met something untrusted"),
                "{told:?}"
            );
        }
    }
}

/// MSG-3: the person is shown the session as the roster holds it and the whole text, with
/// the label it is sent under. A decline sends nothing and tells the planner only that the
/// message was not sent. The label is the context's own, so a context holding private
/// content shows and sends `(T,priv)`.
#[test]
fn the_person_is_shown_the_roster_entry_the_whole_text_and_the_label_and_a_decline_sends_nothing() {
    let long = format!("first line\nsecond line {}", "x".repeat(3_000));
    for (private, label) in [(false, "(T,pub)"), (true, "(T,priv)")] {
        let sessions = Sessions::answering(vec![("build", session("build-id", "build"))]);
        let mut person = Person::saying(&[MessageDecision::Decline, MessageDecision::Once]);
        let mut sink = RecordingSink::new();
        let mut policy = policy(&mut sink);
        if private {
            policy.heard_from_delegate(Label::trusted_private());
        }
        let mut reporter = crate::report::RecordingReporter::default();
        let told = put(
            &mut policy,
            sessions,
            &mut person,
            &mut reporter,
            true,
            false,
            &[call("build", &long), call("build", &long)],
        );
        assert_eq!(
            person.asked,
            vec![
                MessageRequest {
                    name: "build".to_string(),
                    state: "needs-input".to_string(),
                    directory: "/work/build".to_string(),
                    text: long.clone(),
                    label: if private {
                        Label::trusted_private()
                    } else {
                        Label::trusted_public()
                    },
                };
                2
            ]
        );
        assert_eq!(told[0], ("the message was not sent".to_string(), true));
        assert_eq!(
            sessions.delivered(),
            vec![("build-id".to_string(), label.to_string(), long.clone())],
            "only the message the person agreed to went, and under the context's label"
        );
    }
}

/// MSG-4: the second answer covers the later messages to that session this turn and no
/// other session, and the first answer covers nothing past itself. A covered message is not
/// asked about and is still reported to the screen as it goes.
#[test]
fn a_standing_answer_covers_the_later_messages_to_that_session_and_no_other() {
    let sessions = Sessions::answering(vec![
        ("build", session("build-id", "build")),
        ("docs", session("docs-id", "docs")),
    ]);
    let mut person = Person::saying(&[
        MessageDecision::ForTheTurn,
        MessageDecision::Once,
        MessageDecision::Once,
        MessageDecision::Once,
    ]);
    let (_, reporter) = once(
        sessions,
        &mut person,
        &[
            call("build", "one"),
            call("build", "two"),
            call("docs", "three"),
            call("docs", "four"),
        ],
    );
    let asked: Vec<&str> = person
        .asked
        .iter()
        .map(|asked| asked.text.as_str())
        .collect();
    assert_eq!(
        asked,
        ["one", "three", "four"],
        "build was asked again, or docs was covered"
    );
    let delivered: Vec<String> = sessions
        .delivered()
        .into_iter()
        .map(|(_, _, text)| text)
        .collect();
    assert_eq!(delivered, ["one", "two", "three", "four"]);
    let drawn: Vec<&str> = reporter
        .sent
        .iter()
        .map(|sent| sent.text.as_str())
        .collect();
    assert_eq!(
        drawn,
        ["one", "two", "three", "four"],
        "a covered message was not drawn"
    );
}

/// MSG-4 (#4128): a standing answer covers the target under the label the person was shown it
/// under, and no other. A message put to the same target after the sender's own context has
/// turned private carries a label the standing answer was never shown, so it is asked about
/// again, this time with the new label. The control is two calls with no label change between
/// them, which the standing answer still covers.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_standing_answer_does_not_cover_the_same_target_under_a_label_it_was_not_shown() {
    for (label_rises, expect_asked) in [(true, 2), (false, 1)] {
        let sessions = Sessions::answering(vec![("build", session("build-id", "build"))]);
        let mut person = Person::saying(&[MessageDecision::ForTheTurn, MessageDecision::Once]);
        let mut sink = RecordingSink::new();
        let mut policy = policy(&mut sink);
        let mut reporter = crate::report::RecordingReporter::default();
        let scratch = Scratch::new(&format!("session-messages-label-key-{label_rises}"));
        let workspace = Workspace::new(&scratch.path).expect("workspace");
        let ledger: &'static mut Ledger = Box::leak(Box::default());
        with_tools(&workspace, |tools| {
            tools.messaging = Some(Messaging {
                courier: sessions,
                ledger,
            });
            dispatch(
                &mut policy,
                tools,
                &mut person,
                &mut reporter,
                &call("build", "one"),
            );
            if label_rises {
                policy.heard_from_delegate(Label::trusted_private());
            }
            dispatch(
                &mut policy,
                tools,
                &mut person,
                &mut reporter,
                &call("build", "two"),
            );
        });
        assert_eq!(
            person.asked.len(),
            expect_asked,
            "label_rises {label_rises}: {:?}",
            person.asked
        );
        if label_rises {
            assert_eq!(person.asked[0].label, Label::trusted_public());
            assert_eq!(person.asked[1].label, Label::trusted_private());
        }
    }
}

/// MSG-10: a turn puts at most ten messages to one session. The eleventh is refused without
/// asking, the count includes the ones a person declined, and another session is still open
/// to it. A bound counted per turn would refuse the other session as well, one that counted
/// only what was sent would let a declined question be asked for ever, and one that allowed
/// eleven would show at the boundary.
#[test]
fn a_turn_puts_at_most_ten_messages_to_one_session_whether_or_not_they_were_sent() {
    let sessions = Sessions::answering(vec![
        ("build", session("build-id", "build")),
        ("docs", session("docs-id", "docs")),
    ]);
    let mut answers = vec![MessageDecision::Decline; 4];
    answers.extend([MessageDecision::Once; 7]);
    let mut person = Person::saying(&answers);
    let mut calls: Vec<ToolCall> = (0..11).map(|n| call("build", &format!("m{n}"))).collect();
    calls.push(call("docs", "other"));
    let (told, _) = once(sessions, &mut person, &calls);
    assert_eq!(person.asked.len(), 11, "ten for build and one for docs");
    assert!(
        told[10].0.starts_with(
            "refused: this turn has already put 10 messages to the session named 'build'"
        ),
        "{:?}",
        told[10]
    );
    assert!(told[10].0.contains("nobody was asked"));
    assert_eq!(told[11], ("message sent to docs".to_string(), false));
    assert_eq!(
        sessions.delivered().len(),
        6 + 1,
        "six of build's ten were sent, and the one to docs"
    );
}

/// MSG-5: what the receiver answered is told to the planner in a sentence of its own, and a
/// queued message is not said to be read. The trail keeps the sender, the receiver, the
/// label, the length, what the person answered and the word for what became of it, and none
/// of the text.
#[test]
fn what_became_of_a_message_is_told_and_recorded_without_the_text() {
    for (delivery, told_has, word) in [
        (Delivery::Queued, "message sent to build", "queued"),
        (
            Delivery::Full,
            "already holds as many messages as it will wait for",
            "full",
        ),
        (Delivery::TooLong, "refuses a message that long", "too-long"),
        (Delivery::Stopping, "is ending", "stopping"),
        (Delivery::Unknown, "does not know this one", "unknown"),
        (Delivery::Unreached, "did not answer", "not-reached"),
    ] {
        let sessions =
            Sessions::delivering(vec![("build", session("build-id", "build"))], delivery);
        let mut person = Person::saying(&[MessageDecision::Once]);
        let mut sink = RecordingSink::new();
        {
            let mut policy = policy(&mut sink);
            let mut reporter = crate::report::RecordingReporter::default();
            let told = put(
                &mut policy,
                sessions,
                &mut person,
                &mut reporter,
                true,
                false,
                &[call("build", "SECRET-WORDS")],
            );
            assert!(told[0].0.contains(told_has), "{delivery:?}: {told:?}");
            assert_eq!(told[0].1, delivery != Delivery::Queued, "{delivery:?}");
            assert_eq!(
                reporter.sent.len(),
                usize::from(delivery == Delivery::Queued),
                "{delivery:?}"
            );
        }
        assert!(
            sink.events().iter().any(|e| matches!(
                e,
                Event::GatePassed { gate: "message", detail }
                    if detail.contains("sender-id")
                        && detail.contains("build-id")
                        && detail.contains("12 character(s)")
                        && detail.contains("(T,pub)")
                        && detail.contains(word)
                        && !detail.contains("SECRET-WORDS")
            )),
            "{delivery:?} left no trace: {:?}",
            sink.events()
        );
    }
}
