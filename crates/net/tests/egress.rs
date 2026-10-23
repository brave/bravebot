//! Integration tests against a real loopback HTTP server.
//!
//! The unit tests cover URL and cap logic in isolation; these check the behaviour that
//! only appears when bytes actually move: that the policy gate runs before the request,
//! that every redirect hop is revalidated, and that a response body arrives labelled.

use bravebot_core::cancel::Cancel;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::{Event, RecordingSink};
use bravebot_core::label::Label;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::value::Labelled;
use bravebot_net::{Egress, EgressError, Request, Timeouts};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

/// A single-shot server that replies with a canned sequence, one response per
/// connection, then stops.
fn serve(responses: Vec<String>) -> String {
    serve_as("127.0.0.1", responses)
}

/// The same, reachable under `host`, so a test can tell two loopback servers apart by name.
///
/// Bound through the name rather than through an address resolved here, so the client and the
/// listener agree about which loopback address it means.
fn serve_as(host: &str, responses: Vec<String>) -> String {
    let listener = TcpListener::bind((host, 0)).expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();

    thread::spawn(move || {
        for response in responses {
            match listener.accept() {
                Ok((stream, _)) => handle(stream, &response),
                Err(_) => break,
            }
        }
    });

    format!("http://{host}:{port}")
}

fn handle(mut stream: TcpStream, response: &str) {
    // Read the request head so the client is not writing into a closed socket.
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        if line == "\r\n" || line == "\n" {
            break;
        }
        line.clear();
    }
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn ok_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn redirect_to(location: &str) -> String {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

/// A redirect with nowhere to go, which ends the chain in a failure rather than another hop.
fn redirect_without_location() -> String {
    "HTTP/1.1 302 Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
}

fn not_found() -> String {
    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
}

fn routing() -> Routing {
    let mut r = Routing::new();
    r.insert_trusted("task", "fetch a page");
    r
}

#[test]
fn a_successful_fetch_returns_a_labelled_body() {
    let base = serve(vec![ok_response("hello from the server")]);
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::new();
    let response = egress
        .fetch(&mut policy, Request::get(&base), Label::untrusted_public())
        .expect("fetch succeeds");

    assert_eq!(response.status, 200);
    assert_eq!(response.body.label(), Label::untrusted_public());
    assert!(!response.truncated);
}

/// Without the fetch capability nothing should leave the process, and the failure must
/// come from the gate rather than from a connection error.
#[test]
fn a_fetch_without_the_capability_is_refused() {
    let base = serve(vec![ok_response("should never be read")]);
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::none(),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::new();
    let error = egress
        .fetch(&mut policy, Request::get(&base), Label::untrusted_public())
        .expect_err("must be refused");

    assert!(error.to_string().contains("web_fetch"));
    assert!(!policy.finish(), "the refusal must be recorded");
}

/// The property the manual redirect loop exists for: each hop is checked, so the gate
/// sees the redirect target and not only the original URL.
#[test]
fn every_redirect_hop_is_revalidated() {
    // Two servers, and the hop between them named absolutely, so the gate's record of the second
    // check names a host the first request did not go to. A path-absolute Location keeps the
    // authority it was served from, which is a redirect the record cannot tell from no redirect at
    // all now that only the host is kept.
    let second = serve_as("localhost", vec![ok_response("final destination")]);
    let first = serve(vec![redirect_to(&format!("{second}/second"))]);

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::new();
    let response = egress
        .fetch(
            &mut policy,
            Request::get(format!("{first}/first")),
            Label::untrusted_public(),
        )
        .expect("redirect is followed");
    assert_eq!(response.status, 200);

    // Two network gate events: the original URL and the redirect target.
    let checked: Vec<&String> = sink
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::GatePassed {
                gate: "network",
                detail,
            } => Some(detail),
            _ => None,
        })
        .collect();

    assert_eq!(checked.len(), 2, "expected one check per hop: {checked:?}");
    assert_eq!(checked[0].as_str(), "egress to 127.0.0.1");
    assert_eq!(
        checked[1].as_str(),
        "egress to localhost",
        "the redirect target was not the URL the second check saw: {checked:?}"
    );
}

/// A redirect chain that never terminates must stop rather than loop forever.
#[test]
fn a_redirect_loop_is_bounded() {
    // More redirects than the cap allows, all pointing at the same server.
    let responses: Vec<String> = (0..10).map(|_| redirect_to("/again")).collect();
    let base = serve(responses);

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::new();
    let error = egress
        .fetch(
            &mut policy,
            Request::get(format!("{base}/start")),
            Label::untrusted_public(),
        )
        .expect_err("must give up");

    assert!(
        error.to_string().contains("too many redirects"),
        "unexpected error: {error}"
    );
}

/// A failure's text is the part of it a caller formats into whatever it is building, including a
/// message the planner reads. Past the first hop the URL a request is on is a string a server
/// wrote into a `Location` header, so a failure there names the URL the caller asked for and
/// carries nothing of where the redirect went.
#[test]
fn a_redirect_that_leads_nowhere_names_the_url_that_was_asked_for() {
    let base = serve(vec![
        redirect_to("/SENTINEL-REDIRECT-BYTES"),
        redirect_without_location(),
    ]);

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::new();
    let error = egress
        .fetch(
            &mut policy,
            Request::get(format!("{base}/start")),
            Label::untrusted_public(),
        )
        .expect_err("the second hop carried no location");

    let reported = error.to_string();
    assert!(
        !reported.contains("SENTINEL-REDIRECT-BYTES"),
        "the redirect the server chose reached the failure's text: {reported}"
    );
    assert!(
        reported.contains("/start"),
        "the failure did not name the request it was about: {reported}"
    );
}

/// A failure on the first hop, where no redirect has been followed, is worded by this crate and
/// not by the transport. The transport's text for a TLS handshake that fails on the name is the
/// names the server's certificate presented, and a failure's text reaches the planner as the
/// driver's own words. A TLS server cannot be stood up here, so this pins the wiring with the
/// failure that can be: the transport words a refused connection with the operating system's
/// message, and the failure must carry this crate's sentence instead.
#[test]
fn a_first_hop_failure_is_worded_by_this_crate_and_not_the_transport() {
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("bind loopback")
        .local_addr()
        .expect("addr")
        .port();

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::new();
    let error = egress
        .fetch(
            &mut policy,
            Request::get(format!("http://127.0.0.1:{port}/start")),
            Label::untrusted_public(),
        )
        .expect_err("nothing is listening");

    let reported = error.to_string();
    assert!(
        reported.ends_with("failed: the connection was refused"),
        "the failure was not worded by this crate: {reported}"
    );
    assert!(
        reported.contains("/start"),
        "the failure did not name the request it was about: {reported}"
    );
}

/// The same property for the arm an attacker reaches with an ordinary reply rather than a
/// malformed one: a status the caller can act on, about the URL it asked for.
#[test]
fn a_status_after_a_redirect_names_the_url_that_was_asked_for() {
    let base = serve(vec![redirect_to("/SENTINEL-REDIRECT-BYTES"), not_found()]);

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::new();
    let error = egress
        .fetch(
            &mut policy,
            Request::get(format!("{base}/start")),
            Label::untrusted_public(),
        )
        .expect_err("the second hop was a 404");

    let reported = error.to_string();
    assert!(
        !reported.contains("SENTINEL-REDIRECT-BYTES"),
        "the redirect the server chose reached the failure's text: {reported}"
    );
    assert!(
        reported.contains("/start") && reported.contains("404"),
        "the failure did not report what happened to the request: {reported}"
    );
}

#[test]
fn a_non_success_status_is_an_error() {
    let base = serve(vec![
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
    ]);

    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::new();
    let error = egress
        .fetch(&mut policy, Request::get(&base), Label::untrusted_public())
        .expect_err("404 is an error");
    assert!(error.to_string().contains("404"), "unexpected: {error}");
}

/// A policy that lets a request fetch, for a test that asks nothing else of it.
fn fetching(sink: &mut RecordingSink) -> Policy<'_, RecordingSink> {
    Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        sink,
    )
    .expect("policy begins")
}

/// The bytes of a refusal's body, read the way a caller that has decided to compare them would.
fn start_of(policy: &mut Policy<'_, RecordingSink>, body: Labelled<Vec<u8>>) -> Vec<u8> {
    policy
        .decode_transport("test", Label::untrusted_public())
        .decode(body)
        .0
}

/// A refusal as a service sends it: a status line, any further headers, and a JSON body.
fn refusal(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// The failure of a request that was refused, which these tests expect and read.
fn refused(
    egress: &Egress,
    policy: &mut Policy<'_, RecordingSink>,
    request: Request,
) -> EgressError {
    egress
        .fetch(policy, request, Label::untrusted_public())
        .expect_err("a non-success status is a failure")
}

/// NET-12: a request that asks for a refusal's body is handed all of it, labelled as its call says,
/// with the status and the wait the server asked for, and a request that does not ask is told the
/// status alone. A client that ignored the request would hand nobody a body, and one that always
/// kept it would hand every caller a body it never asked for.
#[test]
fn a_request_that_asks_is_handed_a_refusals_body_and_one_that_does_not_is_told_the_status_alone() {
    let body = r#"{"error":{"code":"a_code"}}"#;
    let reply = refusal("429 Too Many Requests", "Retry-After: 7\r\n", body);
    let base = serve(vec![reply.clone(), reply]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = fetching(&mut sink);

    let asked = refused(
        &egress,
        &mut policy,
        Request::get(&base).reading_a_refusal(),
    );
    let EgressError::Status {
        status,
        retry_after,
        body: Some(handed),
        ..
    } = asked
    else {
        panic!("a request that asked was not handed the body: {asked:?}");
    };
    assert_eq!(status, 429);
    assert_eq!(retry_after, Some(Duration::from_secs(7)));
    assert_eq!(handed.label(), Label::untrusted_public());
    assert_eq!(start_of(&mut policy, handed), body.as_bytes());

    let unasked = refused(&egress, &mut policy, Request::get(&base));
    assert!(
        matches!(
            unasked,
            EgressError::Status {
                status: 429,
                retry_after: Some(_),
                body: None,
                ..
            }
        ),
        "a request that did not ask was handed a body: {unasked:?}"
    );
}

/// NET-12: a streamed request is handed the body as a buffered one is, since the failure is
/// decided before either reads a success's body.
#[test]
fn a_streamed_request_that_asks_is_handed_a_refusals_body() {
    let base = serve(vec![refusal("401 Unauthorized", "", r#"{"error":{}}"#)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = fetching(&mut sink);

    let error = egress
        .fetch_streaming(
            &mut policy,
            Request::get(&base).reading_a_refusal(),
            Label::untrusted_public(),
            None,
        )
        .map(|_| ())
        .expect_err("a 401 is a failure");

    assert!(
        matches!(
            error,
            EgressError::Status {
                status: 401,
                body: Some(_),
                ..
            }
        ),
        "{error:?}"
    );
}

/// NET-12: a body of exactly the cap is handed over whole, and one byte more is not handed over at
/// all. A body cut at the cap would be read as if it were whole, and a cap that refused its own
/// length would hide a body that fits.
#[test]
fn a_refusals_body_is_handed_over_whole_up_to_the_cap_and_not_at_all_past_it() {
    let at_the_cap = "a".repeat(bravebot_net::REFUSAL_BODY_BYTES);
    let past_it = "a".repeat(bravebot_net::REFUSAL_BODY_BYTES + 1);
    let base = serve(vec![
        refusal("401 Unauthorized", "", &at_the_cap),
        refusal("401 Unauthorized", "", &past_it),
    ]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = fetching(&mut sink);

    let whole = refused(
        &egress,
        &mut policy,
        Request::get(&base).reading_a_refusal(),
    );
    let EgressError::Status {
        body: Some(handed), ..
    } = whole
    else {
        panic!("a body of exactly the cap was not handed over: {whole:?}");
    };
    assert_eq!(start_of(&mut policy, handed), at_the_cap.as_bytes());

    let long = refused(
        &egress,
        &mut policy,
        Request::get(&base).reading_a_refusal(),
    );
    assert!(
        matches!(
            long,
            EgressError::Status {
                status: 401,
                body: None,
                ..
            }
        ),
        "a body past the cap was handed over: {long:?}"
    );
}

/// NET-12: asking for refusals changes nothing for a success, whose body is read as it always is.
#[test]
fn a_success_is_not_changed_by_asking_for_a_refusals_body() {
    let base = serve(vec![ok_response("hello from the server")]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = fetching(&mut sink);

    let response = egress
        .fetch(
            &mut policy,
            Request::get(&base).reading_a_refusal(),
            Label::untrusted_public(),
        )
        .expect("a 200 is a reply");

    assert_eq!(response.status, 200);
    assert!(!response.truncated);
    assert_eq!(
        start_of(&mut policy, response.body),
        b"hello from the server"
    );
}

/// NET-12: a refusal whose body stops partway is the status alone. A client that took the part that
/// arrived would hand its caller a body cut wherever the connection broke, to be read as if it were
/// whole.
#[test]
fn a_refusal_whose_body_stops_partway_is_the_status_alone() {
    let base = serve(vec![
        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n{\"error\":"
            .to_string(),
    ]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = fetching(&mut sink);

    let error = refused(
        &egress,
        &mut policy,
        Request::get(&base).reading_a_refusal(),
    );

    assert!(
        matches!(
            error,
            EgressError::Status {
                status: 401,
                body: None,
                ..
            }
        ),
        "{error:?}"
    );
}

/// NET-12 and NET-5: a refusal whose body goes quiet is given up on at the gap bound, as any body
/// is, and is the status alone. A read of it that had no bound would hold the request for as long
/// as the server cared to say nothing.
#[test]
fn a_refusal_whose_body_goes_quiet_is_the_status_alone_after_the_gap_bound() {
    let base = serve_stalled_reply("401 Unauthorized", Duration::ZERO, || {});
    let egress = Egress::with_timeouts(Timeouts {
        idle: Duration::from_millis(300),
        reply: Duration::from_secs(30),
        ..Timeouts::default()
    });
    let mut sink = RecordingSink::new();
    let mut policy = fetching(&mut sink);
    let started = std::time::Instant::now();

    let error = refused(
        &egress,
        &mut policy,
        Request::get(&base).reading_a_refusal(),
    );

    assert!(
        matches!(
            error,
            EgressError::Status {
                status: 401,
                body: None,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the wait for a quiet refusal ran {:?}",
        started.elapsed()
    );
}

/// NET-12: a caller that stops the request while a refusal's body is still awaited is answered at
/// once, though the body's own bound is minutes away. A read that ran after the stop was last
/// looked for would hold the turn, and the renewal that follows a refused token, until the server
/// finished or the gap bound passed.
#[test]
fn a_stop_while_a_refusals_body_is_awaited_is_acted_on() {
    let base = serve_stalled_reply("401 Unauthorized", Duration::ZERO, || {});
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = fetching(&mut sink);
    let stop = Cancel::new();
    let presses = stop.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        presses.cancel();
    });
    let started = std::time::Instant::now();

    let outcome = egress.fetch_streaming(
        &mut policy,
        Request::get(&base).reading_a_refusal(),
        Label::untrusted_public(),
        Some(&stop),
    );

    assert!(
        matches!(outcome, Err(EgressError::Stopped { .. })),
        "{:?}",
        outcome.map(|_| ())
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the stop was acted on after {:?}",
        started.elapsed()
    );
}

/// NET-12 and DIAG-7: a refusal whose body fails while it is read is written to the diagnostic log
/// as the body failing, beside the status that is the failure, and a body that failed after the
/// request was stopped is not written, since a stop is not a failure. A read that said nothing
/// would leave the log showing a status for a request whose listed code was never seen.
#[test]
fn a_refusal_whose_body_fails_while_it_is_read_is_written_to_the_diagnostic_log() {
    let timeouts = Timeouts {
        idle: Duration::from_millis(300),
        reply: Duration::from_secs(30),
        ..Timeouts::default()
    };

    let base = serve_stalled_reply("401 Unauthorized", Duration::ZERO, || {});
    let egress = Egress::with_timeouts(timeouts);
    let log = logged(|| {
        let mut sink = RecordingSink::new();
        let mut policy = fetching(&mut sink);
        let error = refused(
            &egress,
            &mut policy,
            Request::get(&base).reading_a_refusal(),
        );
        assert!(matches!(error, EgressError::Status { .. }), "{error:?}");
    });
    let lines = about(&log, &base);
    assert_eq!(lines.len(), 2, "{log:?}");
    let failed_read = lines
        .iter()
        .find(|line| line.contains("kind=transport"))
        .unwrap_or_else(|| panic!("the body failing was not written: {lines:?}"));
    assert!(failed_read.contains("status=401"), "{failed_read}");
    assert!(
        number_after(failed_read, "elapsed_ms") >= 250,
        "the request had waited for the quiet that ended it: {failed_read}"
    );
    assert!(
        lines.iter().any(|line| line.contains("kind=status")),
        "the status that is the failure was not written: {lines:?}"
    );
    for line in &lines {
        assert!(
            !line.contains("beginning of an answer"),
            "what the server sent reached the log: {line}"
        );
    }

    let base = serve_stalled_reply("401 Unauthorized", Duration::ZERO, || {});
    let egress = Egress::with_timeouts(timeouts);
    let log = logged(|| {
        let mut sink = RecordingSink::new();
        let mut policy = fetching(&mut sink);
        let stop = Cancel::new();
        let presses = stop.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            presses.cancel();
        });
        let outcome = egress.fetch_streaming(
            &mut policy,
            Request::get(&base).reading_a_refusal(),
            Label::untrusted_public(),
            Some(&stop),
        );
        assert!(matches!(outcome, Err(EgressError::Stopped { .. })));
        // Past the gap, which is when the abandoned read ends in the failure that must not be written.
        thread::sleep(Duration::from_millis(900));
    });
    assert!(
        about(&log, &base).is_empty(),
        "a stopped request was written: {log:?}"
    );
}

/// NET-12 and NET-2: a refusal after a redirect is reported against the URL that was asked for, as a
/// status is, and carries no body: the body is the answer of a host a server named, which the caller
/// did not ask. Nothing is read from that host's body either, so a body that stalls there holds the
/// failure no longer than the status does and writes nothing about the host it led to. A client that
/// handed the body over would let a server's redirect speak for the host the caller asked.
#[test]
fn a_refusal_after_a_redirect_carries_no_body_and_names_the_url_that_was_asked_for() {
    let stalled = serve_stalled_reply("401 Unauthorized", Duration::ZERO, || {});
    let base = serve(vec![
        redirect_to(&format!("{stalled}/SENTINEL-REDIRECT-BYTES")),
        refusal("404 Not Found", "", r#"{"error":{}}"#),
    ]);
    let egress = Egress::with_timeouts(Timeouts {
        idle: Duration::from_secs(5),
        reply: Duration::from_secs(30),
        ..Timeouts::default()
    });
    let asked = format!("{base}/start");
    let started = std::time::Instant::now();

    let log = logged(|| {
        let mut sink = RecordingSink::new();
        let mut policy = fetching(&mut sink);
        let error = refused(
            &egress,
            &mut policy,
            Request::get(&asked).reading_a_refusal(),
        );

        let EgressError::Status {
            url, status, body, ..
        } = &error
        else {
            panic!("{error:?}");
        };
        assert_eq!((url.as_str(), *status), (asked.as_str(), 401));
        assert!(body.is_none(), "a body the redirect led to was handed over");
        assert!(
            !error.to_string().contains("SENTINEL-REDIRECT-BYTES"),
            "the redirect the server chose reached the failure's text: {error}"
        );
    });

    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the body of the redirect's host was waited on for {:?}",
        started.elapsed()
    );
    assert!(
        about(&log, &stalled).is_empty(),
        "the redirect's host reached the log: {log:?}"
    );
}

/// NET-12: a request that waits on its reply without bound is handed no refusal body, since a stop
/// would leave the read blocked with no bound to end it. The status is the failure.
#[test]
fn a_request_that_waits_without_bound_is_not_handed_a_refusals_body() {
    let base = serve(vec![refusal("401 Unauthorized", "", r#"{"error":{}}"#)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = fetching(&mut sink);
    let stop = Cancel::new();

    let outcome = egress.fetch_streaming(
        &mut policy,
        Request::get(&base)
            .patient_on_this_machine()
            .reading_a_refusal(),
        Label::untrusted_public(),
        Some(&stop),
    );

    assert!(
        matches!(
            outcome,
            Err(EgressError::Status {
                status: 401,
                body: None,
                ..
            })
        ),
        "{:?}",
        outcome.map(|_| ())
    );
}

/// A non-http scheme must be rejected before any connection is attempted, so the
/// network path cannot be used to read local files.
#[test]
fn non_http_schemes_never_reach_the_network() {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::new();
    let error = egress
        .fetch(
            &mut policy,
            Request::get("file:///etc/passwd"),
            Label::untrusted_public(),
        )
        .expect_err("must be refused");

    assert!(error.to_string().contains("only http and https"));
    // Refused before the gate, so no network check was recorded at all.
    assert!(
        !sink.events().iter().any(|e| matches!(
            e,
            Event::GatePassed {
                gate: "network",
                ..
            }
        )),
        "a non-http url should not reach the network gate"
    );
}

/// A server that sends its headers at once and then writes the body a piece at a time.
///
/// This is what a model streaming a long answer looks like on the wire, and what a single
/// end-to-end timeout could not tell apart from a stalled connection.
fn serve_trickled(pieces: Vec<&'static str>, gap: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if line == "\r\n" || line == "\n" {
                break;
            }
            line.clear();
        }

        let length: usize = pieces.iter().map(|p| p.len()).sum();
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.flush();

        for piece in pieces {
            thread::sleep(gap);
            let _ = stream.write_all(piece.as_bytes());
            let _ = stream.flush();
        }
    });

    format!("http://127.0.0.1:{port}")
}

/// A server that accepts the connection and then says nothing at all.
fn serve_silence() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();

    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        // Held open, unanswered, past anything the test waits for: dropping it would itself be
        // an answer of a kind, and the point is a connection that says nothing at all.
        thread::sleep(Duration::from_secs(60));
        drop(stream);
    });

    format!("http://127.0.0.1:{port}")
}

/// The bug a closed laptop lid exposed. A reply that is still being written takes longer than
/// any one phase of the request allows, and cutting it off for that is cutting off a working
/// request for working slowly.
#[test]
fn a_reply_still_arriving_is_not_cut_off_for_taking_longer_than_it_took_to_start() {
    let base = serve_trickled(
        vec!["one ", "two ", "three ", "four ", "five"],
        Duration::from_millis(120),
    );
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    // The reply takes several times longer than the longest gap allowed within it, which is
    // precisely what a single end-to-end bound cannot express.
    let egress = Egress::with_timeouts(Timeouts {
        idle: Duration::from_millis(400),
        reply: Duration::from_secs(10),
        ..Timeouts::default()
    });

    let response = egress
        .fetch(&mut policy, Request::get(&base), Label::untrusted_public())
        .expect("a slowly written body still arrives");

    assert_eq!(response.status, 200);
    assert!(!response.truncated);
    let label = response.body.label();
    let (body, _) = policy.decode_transport("test", label).decode(response.body);
    assert_eq!(String::from_utf8_lossy(&body), "one two three four five");
}

/// A caller that knows how long its reply can run, a model writing to its ceiling, needs that
/// long, and one bound for every reply is too short for the longest of them.
#[test]
fn a_request_stating_how_long_its_reply_may_take_is_given_that_long() {
    for streamed in [false, true] {
        let base = serve_trickled(
            vec!["one ", "two ", "three ", "four ", "five"],
            Duration::from_millis(120),
        );
        let mut sink = RecordingSink::new();
        let mut policy = Policy::begin(
            routing(),
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .expect("policy begins");

        // The whole reply takes twice the bound on a reply of unstated length.
        let egress = Egress::with_timeouts(Timeouts {
            idle: Duration::from_secs(5),
            reply: Duration::from_millis(300),
            ..Timeouts::default()
        });

        let request = Request::get(&base);
        let request = if streamed {
            request.stream_within(Duration::from_secs(10))
        } else {
            request.reply_within(Duration::from_secs(10))
        };
        let response = egress
            .fetch(&mut policy, request, Label::untrusted_public())
            .unwrap_or_else(|e| {
                panic!("a reply inside its stated bound failed, streamed: {streamed}: {e:?}")
            });

        // A body cut short is an error rather than a short body, so this is all of it.
        assert_eq!(response.status, 200);
        assert!(!response.truncated);
    }
}

/// The bound a request states is a bound, not a floor under the default: a caller stating a
/// shorter one than the default is held to it. A reply cut there was still being written, so it
/// is reported as out of time rather than as a connection that gave out, and not as worth another
/// attempt, which would write it as long again and be billed for it again.
#[test]
fn a_reply_outlasting_the_time_its_request_stated_is_given_up_on() {
    for streamed in [false, true] {
        // More than a second between pieces, since ureq gives a read begun after its deadline one
        // more second: a reply whose pieces came closer than that could run on past it.
        let base = serve_trickled(vec!["one ", "two"], Duration::from_millis(1_200));
        let mut sink = RecordingSink::new();
        let mut policy = Policy::begin(
            routing(),
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .expect("policy begins");

        let egress = Egress::with_timeouts(Timeouts {
            idle: Duration::from_secs(5),
            reply: Duration::from_secs(30),
            ..Timeouts::default()
        });

        let bound = Duration::from_millis(600);
        let error = if streamed {
            let mut stream = egress
                .fetch_streaming(
                    &mut policy,
                    Request::get(&base).stream_within(bound),
                    Label::untrusted_public(),
                    None,
                )
                .expect("the reply starts arriving");
            loop {
                match stream.next_chunk() {
                    Ok(Some(_)) => continue,
                    Ok(None) => panic!("a reply outlasting its bound ended cleanly"),
                    Err(error) => break error,
                }
            }
        } else {
            egress
                .fetch(
                    &mut policy,
                    Request::get(&base).reply_within(bound),
                    Label::untrusted_public(),
                )
                .expect_err("a reply outlasting the bound its request stated is not waited on")
        };

        assert!(
            matches!(&error, EgressError::OutOfTime { url } if *url == base),
            "streamed: {streamed}, got {error:?}"
        );
        assert!(!error.is_transient(), "streamed: {streamed}");
    }
}

/// A server that takes the request, says nothing at all for a while, and only then answers.
///
/// What an endpoint that is thinking looks like on the wire: the request is long gone, the
/// socket is healthy, and the first byte of the answer is simply not ready yet.
///
/// The request body is read as well as the head. Closing a connection with bytes still unread
/// on it is answered with a reset rather than a clean end, and a reset that overtakes the reply
/// takes the reply with it.
fn serve_after_thinking(delay: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        let mut length = 0;
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if line == "\r\n" || line == "\n" {
                break;
            }
            let header = line.to_ascii_lowercase();
            if let Some(value) = header.strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
            line.clear();
        }
        reader
            .read_exact(&mut vec![0; length])
            .expect("the request body");

        thread::sleep(delay);

        let body = "an answer worth waiting for";
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body.as_bytes());
        let _ = stream.flush();
    });

    format!("http://127.0.0.1:{port}")
}

/// The wait before a reply starts belongs to `reply`, and the request was sent long before it
/// began. Bounding it by `send` instead reports a thinking endpoint as a dead connection, and
/// the caller retries a request that was going to be answered.
///
/// Sent as a POST, which is the shape the completion endpoint is asked in and the one where
/// every send phase has run before the wait begins.
#[test]
fn a_reply_that_takes_longer_than_the_send_bound_to_start_is_not_a_failed_send() {
    let base = serve_after_thinking(Duration::from_millis(900));
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    // Nothing about sending is slow here, so the send bound is deliberately far under the wait
    // the answer needs: it is the bound that must not be the one that decides.
    let egress = Egress::with_timeouts(Timeouts {
        send: Duration::from_millis(200),
        reply: Duration::from_secs(10),
        ..Timeouts::default()
    });

    let response = egress
        .fetch(
            &mut policy,
            Request::post(&base, b"{}".to_vec()),
            Label::untrusted_public(),
        )
        .expect("a reply that takes a while to start still arrives");

    assert_eq!(response.status, 200);
    assert!(!response.truncated);
    let label = response.body.label();
    let (body, _) = policy.decode_transport("test", label).decode(response.body);
    assert_eq!(
        String::from_utf8_lossy(&body),
        "an answer worth waiting for"
    );
}

/// A reply written in full before any of it is sent spends its time before the first byte, so
/// that wait is what a bound its request states has to lengthen.
#[test]
fn a_reply_written_before_any_of_it_is_sent_is_waited_on_as_long_as_its_request_stated() {
    // Past the send and reply bounds together, by more than the second ureq gives a read begun
    // after its deadline.
    let base = serve_after_thinking(Duration::from_millis(2_000));
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::with_timeouts(Timeouts {
        send: Duration::from_millis(100),
        reply: Duration::from_millis(300),
        ..Timeouts::default()
    });

    let response = egress
        .fetch(
            &mut policy,
            Request::post(&base, b"{}".to_vec()).reply_within(Duration::from_secs(10)),
            Label::untrusted_public(),
        )
        .expect("a reply that began inside the bound its request stated arrives");

    assert_eq!(response.status, 200);
    assert!(!response.truncated);
}

/// The same reply outlasting that bound is cut before any of it arrives, while it is being written,
/// so it is out of time as surely as one cut part way, and never a request that did not get through.
#[test]
fn a_reply_written_before_any_of_it_is_sent_is_out_of_time_when_it_outlasts_its_stated_bound() {
    let base = serve_after_thinking(Duration::from_millis(2_000));
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::with_timeouts(Timeouts {
        reply: Duration::from_secs(30),
        ..Timeouts::default()
    });

    let error = egress
        .fetch(
            &mut policy,
            Request::post(&base, b"{}".to_vec()).reply_within(Duration::from_millis(600)),
            Label::untrusted_public(),
        )
        .expect_err("a reply outlasting the bound its request stated is not waited on");

    assert!(
        matches!(&error, EgressError::OutOfTime { url } if *url == base),
        "got {error:?}"
    );
    assert!(!error.is_transient());
}

/// A stream sends its first bytes at once, so how long a request says its stream may run says
/// nothing about how long to wait for them. A server that never begins is given up on as soon as
/// it would be for any other request.
#[test]
fn a_stream_is_waited_on_to_begin_no_longer_whatever_its_request_said() {
    let base = serve_after_thinking(Duration::from_millis(2_000));
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::with_timeouts(Timeouts {
        reply: Duration::from_millis(300),
        ..Timeouts::default()
    });

    let error = egress
        .fetch(
            &mut policy,
            Request::post(&base, b"{}".to_vec()).stream_within(Duration::from_secs(10)),
            Label::untrusted_public(),
        )
        .expect_err("a stream that has not begun is not waited on as long as it may run");

    assert!(
        matches!(error, EgressError::Transport { .. }),
        "expected a transport failure, got {error:?}"
    );
}

/// The other half of the same property: a request that is getting nowhere still ends.
#[test]
fn a_reply_that_never_comes_gives_up() {
    let base = serve_silence();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::with_timeouts(Timeouts {
        reply: Duration::from_millis(300),
        ..Timeouts::default()
    });

    let error = egress
        .fetch(&mut policy, Request::get(&base), Label::untrusted_public())
        .expect_err("a server that never answers is not waited on forever");

    assert!(
        matches!(error, EgressError::Transport { .. }),
        "expected a transport failure, got {error:?}"
    );
}

/// A server that starts a reply and then stops, without closing the connection.
///
/// The shape of a machine that went to sleep mid-request: the bytes stop, and nothing at either
/// end says the connection is over.
fn serve_stalled_body() -> String {
    serve_stalled_body_then(Duration::ZERO, || {})
}

/// The same, answering only after `delay`, and calling `when_quiet` once the part of the body it
/// writes is on the wire and it has gone quiet.
///
/// The delay makes the time a request has taken by the moment its body fails longer than the
/// body's own wait. The hook is how a test acts at a moment the client is known to be reading,
/// which nothing on the client's side says.
fn serve_stalled_body_then(delay: Duration, when_quiet: impl FnOnce() + Send + 'static) -> String {
    serve_stalled_reply("200 OK", delay, when_quiet)
}

/// The same under a status line of the caller's, so a refusal can stall as a success does.
fn serve_stalled_reply(
    status: &str,
    delay: Duration,
    when_quiet: impl FnOnce() + Send + 'static,
) -> String {
    let status = status.to_string();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if line == "\r\n" || line == "\n" {
                break;
            }
            line.clear();
        }

        thread::sleep(delay);
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: 100\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(b"the beginning of an answer");
        let _ = stream.flush();
        when_quiet();
        thread::sleep(Duration::from_secs(60));
    });

    format!("http://127.0.0.1:{port}")
}

/// The gap bound is what makes a dead connection detectable at all, and it has to be measured
/// between pieces rather than from the start, or it would be the end-to-end bound again.
#[test]
fn a_reply_that_stops_arriving_is_given_up_on() {
    let base = serve_stalled_body();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::with_timeouts(Timeouts {
        idle: Duration::from_millis(300),
        reply: Duration::from_secs(30),
        ..Timeouts::default()
    });

    let mut stream = egress
        .fetch_streaming(
            &mut policy,
            Request::get(&base),
            Label::untrusted_public(),
            None,
        )
        .expect("the reply starts arriving");

    let error = loop {
        match stream.next_chunk() {
            Ok(Some(_)) => continue,
            Ok(None) => panic!("the body should not have ended cleanly"),
            Err(error) => break error,
        }
    };

    assert!(
        matches!(error, EgressError::Transport { .. }),
        "expected a transport failure, got {error:?}"
    );
}

/// A request stating that its reply may take a long time has not said the connection may go
/// quiet for that long: a dead connection is the same whoever asked.
#[test]
fn a_reply_that_stops_arriving_is_given_up_on_however_long_its_request_said_it_may_take() {
    for streamed in [false, true] {
        let base = serve_stalled_body();
        let mut sink = RecordingSink::new();
        let mut policy = Policy::begin(
            routing(),
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .expect("policy begins");

        let egress = Egress::with_timeouts(Timeouts {
            idle: Duration::from_millis(300),
            reply: Duration::from_secs(30),
            ..Timeouts::default()
        });

        let request = Request::get(&base);
        let request = if streamed {
            request.stream_within(Duration::from_secs(20))
        } else {
            request.reply_within(Duration::from_secs(20))
        };
        let started = std::time::Instant::now();
        let mut stream = egress
            .fetch_streaming(&mut policy, request, Label::untrusted_public(), None)
            .expect("the reply starts arriving");

        let error = loop {
            match stream.next_chunk() {
                Ok(Some(_)) => continue,
                Ok(None) => panic!("the body should not have ended cleanly"),
                Err(error) => break error,
            }
        };

        assert!(
            matches!(error, EgressError::Transport { .. }),
            "expected a transport failure, streamed: {streamed}, got {error:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "a silent connection was waited on for {:?}, streamed: {streamed}",
            started.elapsed()
        );
    }
}

/// Read the whole body at `base` through an egress whose gap bound is `default_gap`, with the request
/// as `shape` makes it, and say how many bytes arrived.
fn bytes_read(
    base: &str,
    default_gap: Duration,
    shape: impl FnOnce(Request) -> Request,
) -> Result<usize, EgressError> {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");
    let egress = Egress::with_timeouts(Timeouts {
        idle: default_gap,
        reply: Duration::from_secs(30),
        ..Timeouts::default()
    });
    let mut stream = egress.fetch_streaming(
        &mut policy,
        shape(Request::get(base)),
        Label::untrusted_public(),
        None,
    )?;
    while stream.next_chunk()?.is_some() {}
    Ok(stream.handed_back())
}

/// A person who stated a longer gap than the default is waited for through it, whatever else the
/// request says about its reply. The pieces come further apart than the gap the egress layer
/// allows, so only a gap the request itself carried gets the body to the end.
#[test]
fn a_request_stating_a_longer_gap_between_pieces_is_waited_on_that_long() {
    let pieces = || vec!["one ", "two ", "three"];
    let default_gap = Duration::from_millis(300);
    let apart = Duration::from_millis(700);

    let base = serve_trickled(pieces(), apart);
    let unstated = bytes_read(&base, default_gap, |request| request);
    assert!(
        matches!(unstated, Err(EgressError::Transport { .. })),
        "pieces further apart than the default gap were waited for: {unstated:?}"
    );

    for (name, shape) in [
        (
            "alone",
            Box::new(|request: Request| request.idle_within(Duration::from_secs(3)))
                as Box<dyn FnOnce(Request) -> Request>,
        ),
        (
            "with a whole reply bound",
            Box::new(|request: Request| {
                request
                    .idle_within(Duration::from_secs(3))
                    .reply_within(Duration::from_secs(20))
            }),
        ),
        (
            "with a stream's reply bound",
            Box::new(|request: Request| {
                request
                    .stream_within(Duration::from_secs(20))
                    .idle_within(Duration::from_secs(3))
            }),
        ),
    ] {
        let base = serve_trickled(pieces(), apart);
        let stated = bytes_read(&base, default_gap, shape);
        assert_eq!(
            stated.as_ref().ok(),
            Some(&"one two three".len()),
            "a gap of three seconds {name} did not carry the body: {stated:?}"
        );
    }
}

/// The stated gap is the one in force in either direction. A person on a link that dies quietly may
/// want a connection that has gone silent given up on sooner than the default allows, and an
/// implementation that only ever lengthened the gap would make them wait the default out.
#[test]
fn a_request_stating_a_shorter_gap_gives_up_on_a_quiet_reply_sooner() {
    let base = serve_stalled_body();
    let started = std::time::Instant::now();
    let stated = bytes_read(&base, Duration::from_secs(8), |request| {
        request.idle_within(Duration::from_millis(300))
    });

    assert!(
        matches!(stated, Err(EgressError::Transport { .. })),
        "a body that stopped was read as {stated:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "a gap of 300 ms was waited on for {:?}, which is the default gap's order",
        started.elapsed()
    );
}

/// A local model writing a tool call, as Ollama sends one: nothing at all while the model thinks,
/// not even the headers, then a first piece, then nothing again while the call is written, then
/// the rest.
fn serve_like_a_local_model(host: &str, silence: Duration) -> String {
    let listener = TcpListener::bind((host, 0)).expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        let mut length = 0;
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if line == "\r\n" || line == "\n" {
                break;
            }
            let header = line.to_ascii_lowercase();
            if let Some(value) = header.strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
            line.clear();
        }
        // Read whole, so closing afterwards is a clean end rather than a reset.
        let _ = reader.read_exact(&mut vec![0; length]);

        thread::sleep(silence);
        let (first, rest) = ("a preamble, ", "and then a tool call");
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            first.len() + rest.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(first.as_bytes());
        let _ = stream.flush();
        thread::sleep(silence);
        let _ = stream.write_all(rest.as_bytes());
        let _ = stream.flush();
    });

    format!("http://{host}:{port}")
}

/// Every bound on a reply is there to tell a slow answer from a connection that died without
/// saying so, and a connection to this machine cannot: when the server goes, the socket says it
/// has. A local model can be silent for minutes while it writes a tool call, and a gap bound cut
/// that silence as though the connection had gone.
#[test]
fn a_stream_from_this_machine_that_asks_is_waited_on_through_any_silence() {
    let stop = Cancel::new();
    for host in ["127.0.0.1", "localhost"] {
        for patient in [true, false] {
            let base = serve_like_a_local_model(host, Duration::from_millis(800));
            let mut sink = RecordingSink::new();
            let mut policy = Policy::begin(
                routing(),
                ReleasePlan::new(),
                CapabilitySet::from_iter([Capability::WebFetch]),
                &mut sink,
            )
            .expect("policy begins");

            // Each silence outlasts both the wait for the reply and the gap within it.
            let egress = Egress::with_timeouts(Timeouts {
                reply: Duration::from_millis(300),
                idle: Duration::from_millis(300),
                ..Timeouts::default()
            });
            let request = Request::post(&base, b"{}".to_vec());
            let request = if patient {
                request.patient_on_this_machine()
            } else {
                request
            };

            // To its end: the server states the body's length, so a clean end is all of it.
            let read = egress
                .fetch_streaming(&mut policy, request, Label::untrusted_public(), Some(&stop))
                .and_then(|mut stream| {
                    let mut pieces = 0;
                    while stream.next_chunk()?.is_some() {
                        pieces += 1;
                    }
                    Ok(pieces)
                });

            if patient {
                let pieces = read.unwrap_or_else(|error| panic!("{host}: {error:?}"));
                assert!(pieces >= 2, "{host}: both halves arrive, got {pieces}");
            } else {
                assert!(
                    matches!(read, Err(EgressError::Transport { .. })),
                    "{host}: without the ask the silence is still cut: {read:?}"
                );
            }
        }
    }
}

/// A stated gap is a bound like the default one, and a stream from this machine that asks to be
/// waited on through any silence is held to none of them: a person who stated 200 ms for a link
/// that dies quietly has not asked for a model on their own machine to be cut off while it writes.
#[test]
fn a_stated_gap_does_not_end_a_stream_from_this_machine_that_asks_to_be_waited_on() {
    let stop = Cancel::new();
    for patient in [true, false] {
        let base = serve_like_a_local_model("127.0.0.1", Duration::from_millis(800));
        let mut sink = RecordingSink::new();
        let mut policy = Policy::begin(
            routing(),
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .expect("policy begins");
        // Only the stated gap is shorter than the silence, so nothing else can be what cuts it.
        let egress = Egress::with_timeouts(Timeouts {
            reply: Duration::from_secs(30),
            idle: Duration::from_secs(30),
            ..Timeouts::default()
        });
        let request = Request::post(&base, b"{}".to_vec()).idle_within(Duration::from_millis(200));
        let request = if patient {
            request.patient_on_this_machine()
        } else {
            request
        };

        let read = egress
            .fetch_streaming(&mut policy, request, Label::untrusted_public(), Some(&stop))
            .and_then(|mut stream| {
                let mut pieces = 0;
                while stream.next_chunk()?.is_some() {
                    pieces += 1;
                }
                Ok(pieces)
            });

        if patient {
            let pieces = read.unwrap_or_else(|error| panic!("a stated gap cut it: {error:?}"));
            assert!(pieces >= 2, "both halves arrive, got {pieces}");
        } else {
            assert!(
                matches!(read, Err(EgressError::Transport { .. })),
                "without the ask a stated gap still cuts the silence: {read:?}"
            );
        }
    }
}

/// A server here may redirect anywhere, and how long the next hop is waited on is not something
/// a server gets to lengthen.
#[test]
fn a_redirect_from_this_machine_keeps_the_bounds() {
    let second = serve_like_a_local_model("127.0.0.1", Duration::from_millis(800));
    let first = serve(vec![redirect_to(&format!("{second}/v1"))]);
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::with_timeouts(Timeouts {
        reply: Duration::from_millis(300),
        idle: Duration::from_millis(300),
        ..Timeouts::default()
    });
    let error = egress
        .fetch_streaming(
            &mut policy,
            Request::get(format!("{first}/v1")).patient_on_this_machine(),
            Label::untrusted_public(),
            Some(&Cancel::new()),
        )
        .map(|_| ())
        .expect_err("the hop past a redirect is bounded");

    assert!(
        matches!(error, EgressError::Transport { .. }),
        "expected a transport failure, got {error:?}"
    );
}

/// With no bound, the only end to a wait on a server here that took the request and hung is
/// somebody stopping it. A stream fetched without a token cannot be, and a whole reply is read to
/// its end whatever is pressed meanwhile, so neither is waited on that way however it asks.
#[test]
fn a_wait_that_nobody_can_stop_keeps_the_bounds() {
    let stop = Cancel::new();
    for whole in [false, true] {
        let base = serve_like_a_local_model("127.0.0.1", Duration::from_millis(800));
        let mut sink = RecordingSink::new();
        let mut policy = Policy::begin(
            routing(),
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .expect("policy begins");

        let egress = Egress::with_timeouts(Timeouts {
            reply: Duration::from_millis(300),
            idle: Duration::from_millis(300),
            ..Timeouts::default()
        });
        let request = Request::post(&base, b"{}".to_vec()).patient_on_this_machine();
        let read = if whole {
            egress
                .fetch_watching(&mut policy, request, Label::untrusted_public(), Some(&stop))
                .map(|_| ())
        } else {
            egress
                .fetch_streaming(&mut policy, request, Label::untrusted_public(), None)
                .map(|_| ())
        };

        assert!(
            matches!(read, Err(EgressError::Transport { .. })),
            "whole: {whole}: the silence was waited through: {read:?}"
        );
    }
}

/// A buffered read has to tell the difference too. Silently handing back the part that arrived
/// turns a dead connection into whatever those bytes happen to parse as, which for a JSON reply
/// is a puzzling decoding error somewhere far from the cause.
#[test]
fn a_body_that_stops_partway_is_a_failure_rather_than_a_short_body() {
    let base = serve_stalled_body();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let egress = Egress::with_timeouts(Timeouts {
        idle: Duration::from_millis(300),
        reply: Duration::from_secs(30),
        ..Timeouts::default()
    });

    let error = egress
        .fetch(&mut policy, Request::get(&base), Label::untrusted_public())
        .expect_err("half a body is not a body");

    assert!(
        matches!(error, EgressError::Transport { .. }),
        "expected a transport failure, got {error:?}"
    );
}

/// Serialises the tests that read the process-wide diagnostic log.
static DIAG_LOG: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// What `run` wrote to the diagnostic log at the default level, one line per entry.
///
/// Other tests in this binary fail requests while one of these is running, so a test reads the
/// lines that name its own server's port and no others.
fn logged(run: impl FnOnce()) -> Vec<String> {
    /// Points the log nowhere again when the closure panics, so one failed assertion does not leave
    /// every later request in the binary writing to a directory that is gone.
    struct Unconfigure;
    impl Drop for Unconfigure {
        fn drop(&mut self) {
            bravebot_diag::configure(bravebot_diag::Level::Error, None);
        }
    }

    let _held = DIAG_LOG.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("a scratch directory");
    bravebot_diag::configure(bravebot_diag::Level::Error, Some(dir.path().to_path_buf()));
    {
        let _unconfigure = Unconfigure;
        run();
    }
    std::fs::read_dir(dir.path().join(bravebot_diag::DIRECTORY))
        .map(|entries| {
            entries
                .flatten()
                .flat_map(|e| {
                    std::fs::read_to_string(e.path())
                        .unwrap_or_default()
                        .lines()
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The lines `logged` returned that are about the server at `base`.
fn about(log: &[String], base: &str) -> Vec<String> {
    let host = format!("host={}", base.trim_start_matches("http://"));
    log.iter()
        .filter(|line| line.contains(&host))
        .cloned()
        .collect()
}

/// The number written after `key=` on `line`.
fn number_after(line: &str, key: &str) -> u128 {
    let rest = line
        .split_once(&format!("{key}="))
        .unwrap_or_else(|| panic!("{key} is not on {line:?}"))
        .1;
    rest.split_whitespace()
        .next()
        .and_then(|digits| digits.parse().ok())
        .unwrap_or_else(|| panic!("{key} is not a number on {line:?}"))
}

/// Reads `base` the buffered way, or the streamed way and to its end, and returns the failure.
fn read_to_failure(egress: &Egress, request: Request, streamed: bool) -> EgressError {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");
    if streamed {
        let mut stream = egress
            .fetch_streaming(&mut policy, request, Label::untrusted_public(), None)
            .expect("the reply starts arriving");
        loop {
            match stream.next_chunk() {
                Ok(Some(_)) => continue,
                Ok(None) => panic!("the body should not have ended cleanly"),
                Err(error) => break error,
            }
        }
    } else {
        egress
            .fetch(&mut policy, request, Label::untrusted_public())
            .expect_err("half a body is not a body")
    }
}

/// A reply that fails while its body is read is a failed request, and the log is where a person
/// attaching it to a bug report looks for one. The status was a 2xx, so nothing that looks at a
/// response's head sees it; writing it only on one of the two ways of reading a body leaves the
/// other's failures out of the file.
#[test]
fn a_request_whose_body_fails_while_it_is_read_is_written_to_the_diagnostic_log() {
    for streamed in [false, true] {
        let base = serve_stalled_body();
        let egress = Egress::with_timeouts(Timeouts {
            idle: Duration::from_millis(300),
            reply: Duration::from_secs(30),
            ..Timeouts::default()
        });

        let log = logged(|| {
            let error = read_to_failure(&egress, Request::get(&base), streamed);
            assert!(matches!(error, EgressError::Transport { .. }), "{error:?}");
        });

        let lines = about(&log, &base);
        assert_eq!(lines.len(), 1, "streamed: {streamed}: {log:?}");
        let line = &lines[0];
        assert!(
            line.contains("ERROR net.fetch"),
            "streamed: {streamed}: {line}"
        );
        assert!(
            line.contains("kind=transport"),
            "streamed: {streamed}: {line}"
        );
        assert!(
            line.contains("status=200"),
            "the reply began with a 200, streamed: {streamed}: {line}"
        );
        assert!(
            number_after(line, "elapsed_ms") >= 250,
            "the request had waited for the quiet that ended it, streamed: {streamed}: {line}"
        );
        assert!(
            !line.contains("beginning of an answer"),
            "what the server sent reached the log, streamed: {streamed}: {line}"
        );
    }
}

/// A deadline that passes mid-reply is a different kind of failure from a cut connection, and the
/// log names which, because "it timed out" and "it was reset" send a person looking in different
/// places. It is not worth retrying, which the line says too.
#[test]
fn a_reply_that_runs_past_its_deadline_is_written_as_out_of_time() {
    for streamed in [false, true] {
        let base = serve_trickled(vec!["one ", "two"], Duration::from_millis(1_200));
        let egress = Egress::with_timeouts(Timeouts {
            idle: Duration::from_secs(5),
            reply: Duration::from_secs(30),
            ..Timeouts::default()
        });
        let bound = Duration::from_millis(600);
        let request = if streamed {
            Request::get(&base).stream_within(bound)
        } else {
            Request::get(&base).reply_within(bound)
        };

        let log = logged(|| {
            let error = read_to_failure(&egress, request, streamed);
            assert!(matches!(error, EgressError::OutOfTime { .. }), "{error:?}");
        });

        let lines = about(&log, &base);
        assert_eq!(lines.len(), 1, "streamed: {streamed}: {log:?}");
        assert!(
            lines[0].contains("kind=out_of_time")
                && lines[0].contains("transient=0")
                && lines[0].contains("status=200"),
            "streamed: {streamed}: {}",
            lines[0]
        );
    }
}

/// How long a request took is the whole of it, from the moment it was sent, including the wait for
/// the reply to begin. A reply that took most of a minute to begin and then stopped is a different
/// report from one that stopped at once.
#[test]
fn a_body_failure_is_timed_from_when_the_request_began() {
    for streamed in [false, true] {
        let base = serve_stalled_body_then(Duration::from_millis(700), || {});
        let egress = Egress::with_timeouts(Timeouts {
            idle: Duration::from_millis(300),
            reply: Duration::from_secs(30),
            ..Timeouts::default()
        });

        let log = logged(|| {
            read_to_failure(&egress, Request::get(&base), streamed);
        });

        let lines = about(&log, &base);
        assert_eq!(lines.len(), 1, "streamed: {streamed}: {log:?}");
        let elapsed = number_after(&lines[0], "elapsed_ms");
        assert!(
            elapsed >= 900,
            "the wait for the reply to begin was left out, streamed: {streamed}: {}",
            lines[0]
        );
    }
}

/// A request a person stopped is not one that failed. A read blocked on a quiet server cannot be
/// interrupted, so what ends it after the stop is the connection giving out, and writing that
/// would leave a file for a run whose only fault was being stopped. The same read with nobody
/// having stopped it is written, which shows the fixture fails the request and that the stop is
/// what removes the line.
#[test]
fn a_body_that_fails_after_the_request_was_stopped_is_not_written() {
    for streamed in [false, true] {
        for stopped in [false, true] {
            let stop = Cancel::new();
            // The stop arrives while the read is blocked on a server that has gone quiet. A
            // buffered read hands nothing back until it ends, so the server presses it, a second
            // and a half after the reply began and two and a half seconds before the gap bound ends
            // the read. A press that came before the reply was seen would end the fetch as stopped
            // and fail the assertion below on the kind of error, so a slow machine fails this test
            // and cannot pass it. A streamed read is stopped between two of its own reads, where
            // the test can see that the first has returned.
            let presses = stop.clone();
            let base = serve_stalled_body_then(Duration::ZERO, move || {
                if stopped && !streamed {
                    thread::sleep(Duration::from_millis(1_500));
                    presses.cancel();
                }
            });
            let egress = Egress::with_timeouts(Timeouts {
                idle: if stopped && !streamed {
                    Duration::from_secs(4)
                } else {
                    Duration::from_millis(400)
                },
                reply: Duration::from_secs(30),
                ..Timeouts::default()
            });

            let log = logged(|| {
                let mut sink = RecordingSink::new();
                let mut policy = Policy::begin(
                    routing(),
                    ReleasePlan::new(),
                    CapabilitySet::from_iter([Capability::WebFetch]),
                    &mut sink,
                )
                .expect("policy begins");
                let error = if streamed {
                    let mut stream = egress
                        .fetch_streaming(
                            &mut policy,
                            Request::get(&base),
                            Label::untrusted_public(),
                            Some(&stop),
                        )
                        .expect("the reply starts arriving");
                    stream
                        .next_chunk()
                        .expect("the first of the body arrives")
                        .expect("and is not the end");
                    if stopped {
                        stop.cancel();
                    }
                    stream.next_chunk().expect_err("the quiet ends the read")
                } else {
                    egress
                        .fetch_watching(
                            &mut policy,
                            Request::get(&base),
                            Label::untrusted_public(),
                            Some(&stop),
                        )
                        .expect_err("half a body is not a body")
                };
                assert!(
                    matches!(error, EgressError::Transport { .. }),
                    "streamed: {streamed}, stopped: {stopped}: {error:?}"
                );
            });

            let lines = about(&log, &base);
            assert_eq!(
                lines.len(),
                usize::from(!stopped),
                "streamed: {streamed}, stopped: {stopped}: {log:?}"
            );
        }
    }
}

/// A fetch the way `fetch_url` runs one: approved for this URL, then in flight.
fn fetch_approved(egress: &Egress, url: &str) -> Result<bravebot_net::Response, EgressError> {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");
    policy.endorse_fetch(url);
    policy.before_fetch(url).expect("approved");
    let outcome = egress.fetch(&mut policy, Request::get(url), Label::untrusted_public());
    policy.fetch_finished();
    outcome
}

/// `localhost` is a name, so what the system resolver answers for it is classified: the approval
/// was for a host as written, and a name is the case it says nothing about. The listener answers,
/// so a refusal that came from nothing being there to connect to would still show a connection.
#[test]
fn a_fetched_name_that_resolves_to_this_machine_is_refused_without_a_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();
    listener.set_nonblocking(true).expect("nonblocking");

    let url = format!("http://localhost:{port}/page");
    let error = fetch_approved(&Egress::new(), &url).expect_err("must be refused");

    assert!(
        matches!(&error, EgressError::AddressRefused { url: named } if *named == url),
        "{error:?}"
    );
    thread::sleep(Duration::from_millis(100));
    assert!(
        listener.accept().is_err(),
        "a connection reached the listener"
    );
    assert!(!error.to_string().contains("127.0.0.1"), "{error}");
}

/// The refusal is the fetch's. The same name, reached by something that is not a fetch in flight,
/// is a local model endpoint and goes through.
#[test]
fn the_same_name_is_reached_when_no_fetch_is_in_flight() {
    let base = serve_as("localhost", vec![ok_response("a local model")]);
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy begins");

    let response = Egress::new()
        .fetch(&mut policy, Request::get(&base), Label::untrusted_public())
        .expect("not a fetch");
    assert_eq!(response.status, 200);
}

/// An address the person approved as written is not a lookup. The same server, asked for by name,
/// is the case above.
#[test]
fn an_approved_address_literal_on_this_machine_is_fetched() {
    let base = serve(vec![ok_response("local page")]);
    let response = fetch_approved(&Egress::new(), &format!("{base}/page")).expect("a literal");
    assert_eq!(response.status, 200);
}

fn serve_recording_headers(
    responses: Vec<String>,
) -> (String, std::sync::mpsc::Receiver<Vec<(String, String)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = std::sync::mpsc::channel();

    thread::spawn(move || {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let mut headers = Vec::new();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 {
                    break;
                }
                if header == "\r\n" || header == "\n" {
                    break;
                }
                if let Some((name, val)) = header.split_once(':') {
                    headers.push((name.trim().to_ascii_lowercase(), val.trim().to_string()));
                }
            }
            let _ = tx.send(headers);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://127.0.0.1:{port}"), rx)
}

#[test]
fn socket_scoped_headers_are_sent_to_declared_socket() {
    let (base, rx) = serve_recording_headers(vec![ok_response("ok")]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy");

    let request = Request::get(&base)
        .header("x-regular", "keep")
        .socket_header("authorization", "Bearer test-secret")
        .socket_header("x-scoped", "socket-secret");

    let response = egress
        .fetch(&mut policy, request, Label::untrusted_public())
        .expect("fetch succeeds");
    assert_eq!(response.status, 200);

    let headers = rx.recv().expect("received headers");
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer test-secret"),
        "authorization must be present: {headers:?}"
    );
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "x-scoped" && v == "socket-secret"),
        "x-scoped must be present: {headers:?}"
    );
    assert!(
        headers.iter().any(|(k, v)| k == "x-regular" && v == "keep"),
        "x-regular must be present: {headers:?}"
    );
}

#[test]
fn socket_scoped_headers_and_authorization_are_dropped_across_different_sockets_on_redirect() {
    let (second, second_rx) = serve_recording_headers(vec![ok_response("second")]);
    let (first, first_rx) = serve_recording_headers(vec![redirect_to(&format!("{second}/target"))]);

    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy");

    let request = Request::get(&first)
        .header("x-regular", "keep")
        .socket_header("authorization", "Bearer test-secret")
        .socket_header("x-scoped", "socket-secret");

    let response = egress
        .fetch(&mut policy, request, Label::untrusted_public())
        .expect("fetch succeeds through redirect");
    assert_eq!(response.status, 200);

    // First hop (declared socket) receives all headers.
    let first_headers = first_rx.recv().expect("first hop headers");
    assert!(
        first_headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer test-secret")
    );
    assert!(
        first_headers
            .iter()
            .any(|(k, v)| k == "x-scoped" && v == "socket-secret")
    );
    assert!(
        first_headers
            .iter()
            .any(|(k, v)| k == "x-regular" && v == "keep")
    );

    // Second hop (different socket) drops socket headers and authorization.
    let second_headers = second_rx.recv().expect("second hop headers");
    assert!(
        !second_headers.iter().any(|(k, _)| k == "authorization"),
        "authorization must be dropped on redirect to different socket: {second_headers:?}"
    );
    assert!(
        !second_headers.iter().any(|(k, _)| k == "x-scoped"),
        "socket-scoped header must be dropped on redirect to different socket: {second_headers:?}"
    );
    assert!(
        second_headers
            .iter()
            .any(|(k, v)| k == "x-regular" && v == "keep"),
        "regular header must be preserved: {second_headers:?}"
    );
}

#[test]
fn socket_scoped_headers_are_kept_on_same_socket_redirect() {
    let (server, rx) = serve_recording_headers(vec![
        redirect_to("/destination"),
        ok_response("destination reached"),
    ]);

    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy");

    let request = Request::get(format!("{server}/start"))
        .header("x-regular", "keep")
        .socket_header("authorization", "Bearer test-secret")
        .socket_header("x-scoped", "socket-secret");

    let response = egress
        .fetch(&mut policy, request, Label::untrusted_public())
        .expect("fetch succeeds through same socket redirect");
    assert_eq!(response.status, 200);

    let first_headers = rx.recv().expect("first hop headers");
    assert!(
        first_headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer test-secret")
    );

    let second_headers = rx.recv().expect("second hop headers");
    assert!(
        second_headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer test-secret"),
        "same socket redirect must retain socket headers: {second_headers:?}"
    );
    assert!(
        second_headers
            .iter()
            .any(|(k, v)| k == "x-scoped" && v == "socket-secret"),
        "same socket redirect must retain socket headers: {second_headers:?}"
    );
}

#[test]
fn plain_authorization_header_is_stripped_on_cross_socket_redirect() {
    let (second, second_rx) = serve_recording_headers(vec![ok_response("second")]);
    let (first, first_rx) = serve_recording_headers(vec![redirect_to(&format!("{second}/target"))]);

    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy");

    let request = Request::get(&first)
        .header("x-regular", "keep")
        .header("authorization", "Bearer plain-secret");

    let response = egress
        .fetch(&mut policy, request, Label::untrusted_public())
        .expect("fetch succeeds through redirect");
    assert_eq!(response.status, 200);

    let first_headers = first_rx.recv().expect("first hop headers");
    assert!(
        first_headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer plain-secret")
    );

    let second_headers = second_rx.recv().expect("second hop headers");
    assert!(
        !second_headers.iter().any(|(k, _)| k == "authorization"),
        "plain authorization header must be stripped on redirect to different socket: {second_headers:?}"
    );
    assert!(
        second_headers
            .iter()
            .any(|(k, v)| k == "x-regular" && v == "keep"),
        "regular header must be preserved: {second_headers:?}"
    );
}

/// A server whose first answer redirects to its own port over `https`, and which reports every
/// connection after that one.
///
/// A later connection is the second hop reaching the port. It is closed unanswered, so a client
/// that starts a TLS handshake there fails at once and the test does not wait on it.
fn serve_a_redirect_to_its_own_port_over_tls() -> (String, std::sync::mpsc::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();
    let (later, reached) = std::sync::mpsc::channel();

    thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        handle(
            stream,
            &redirect_to(&format!("https://127.0.0.1:{port}/moved")),
        );
        while let Ok((stream, _)) = listener.accept() {
            let _ = later.send(());
            drop(stream);
        }
    });

    (format!("http://127.0.0.1:{port}"), reached)
}

/// A server declared on a written port is on the scheme it was declared with. A reply that keeps
/// the address and the port and changes the scheme names a server that speaks something else
/// there, so the hop is a move and the credential is not sent to it.
#[test]
fn a_redirect_to_the_other_scheme_on_the_declared_port_is_not_sent_there() {
    let (base, reached) = serve_a_redirect_to_its_own_port_over_tls();
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy");

    let url = format!("{base}/start");
    policy.before_server_request(&url);
    let request = Request::get(&url).socket_header("authorization", "Bearer test-secret");
    let error = egress
        .fetch(&mut policy, request, Label::untrusted_public())
        .expect_err("a hop to the other scheme on the declared port was followed");

    assert!(matches!(error, EgressError::Denied(_)), "{error}");
    assert!(
        policy.take_server_hop().is_some(),
        "the refused hop is kept for the question about a move"
    );
    assert!(
        reached.try_recv().is_err(),
        "the second hop connected to the declared port"
    );
}

/// A url whose port cannot be read is connected to on the scheme's default, which is not the port
/// it spells. A request to a server declared that way is refused at the gate, before anything
/// connects, and is put to the person as a move rather than sent to port 80 or 443 with the
/// credential configured for the declared one.
#[test]
fn a_server_declared_with_a_port_that_cannot_be_read_is_sent_nothing() {
    for url in [
        "http://127.0.0.1:/mcp",
        "http://127.0.0.1:abc/mcp",
        "http://127.0.0.1:99999/mcp",
    ] {
        let egress = Egress::new();
        let mut sink = RecordingSink::new();
        let mut policy = Policy::begin(
            routing(),
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .expect("policy");

        policy.before_server_request(url);
        let request = Request::get(url).socket_header("authorization", "Bearer test-secret");
        let error = egress
            .fetch(&mut policy, request, Label::untrusted_public())
            .expect_err("a request to a socket nobody can read was sent");

        assert!(matches!(error, EgressError::Denied(_)), "{url}: {error}");
        assert!(policy.take_server_hop().is_some(), "{url}");
    }
}
