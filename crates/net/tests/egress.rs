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

        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 100\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(b"the beginning of an answer");
        let _ = stream.flush();
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
/// its end where no token is looked at, so neither is waited on that way however it asks.
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
