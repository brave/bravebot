//! The listener that takes the browser's redirect
//! ([CHATGPT-4](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-4)).
//!
//! A port on the loopback address is reachable by every program on the machine, and by a page in
//! the person's browser that makes a request to it. So a request is the callback only if it carries
//! the `state` this sign-in generated, and every other request is answered with a fixed page and
//! ignored. From the callback it reads three query parameters and nothing else, and it reads no
//! header.

use crate::Secret;
use crate::oauth::encoding::{percent_decode, same};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// The most a connection may send before its request line is complete.
const LINE_LIMIT: usize = 8 * 1024;

/// How long a connection has to send a complete request line.
const LINE_TIME: Duration = Duration::from_secs(5);

/// How long the accept loop waits before it looks again for a connection or an answer.
const LOOK_AGAIN_IN: Duration = Duration::from_millis(10);

/// The longest a `client_id` the server issued may be.
const CLIENT_LIMIT: usize = 128;

/// The words on the two pages the listener answers with, which the caller states because it holds
/// the message catalog and this crate does not.
///
/// Each is text, not markup: it is escaped into a page that loads nothing and repeats nothing from
/// the request.
pub struct Pages<'a> {
    /// What the callback is answered with.
    pub done: &'a str,
    /// What every other request is answered with.
    pub not_found: &'a str,
}

/// A page of one paragraph, holding `text` escaped.
fn page(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    format!("<!doctype html><meta charset=\"utf-8\"><title>bravebot</title><p>{escaped}</p>")
}

/// The two pages as they are sent.
struct Rendered {
    done: String,
    not_found: String,
}

/// What the redirect carried.
///
/// The three values the listener reads and nothing else. The code is a bearer value for the few
/// seconds before the exchange, so it is held as a [`Secret`].
pub(super) struct Callback {
    pub(super) code: Option<Secret>,
    /// The client OpenAI issued, where it is a string of the shape a client identifier has.
    pub(super) client_id: Option<String>,
}

/// Why no callback came.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Waited {
    /// The time bound passed.
    TimedOut,
}

/// The listener, bound to `127.0.0.1` and nothing else.
pub(super) struct Listener {
    inner: TcpListener,
    /// How long a connection has to send a complete request line.
    line_time: Duration,
}

impl Listener {
    /// Bind `127.0.0.1` at a port the operating system chooses.
    pub(super) fn bind() -> std::io::Result<Self> {
        let inner = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))?;
        inner.set_nonblocking(true)?;
        Ok(Self {
            inner,
            line_time: LINE_TIME,
        })
    }

    /// A listener whose connections have `line_time` to send a request line, so a test can hold a
    /// connection silent for as long as it needs without the wait depending on the clock.
    #[cfg(test)]
    fn bind_waiting(line_time: Duration) -> std::io::Result<Self> {
        Ok(Self {
            line_time,
            ..Self::bind()?
        })
    }

    pub(super) fn port(&self) -> std::io::Result<u16> {
        Ok(self.inner.local_addr()?.port())
    }

    /// Wait for the callback that carries `state`, for at most `within`.
    ///
    /// The first callback is the only one served, and the listener closes after it. Connections are
    /// served at the same time, so one that sends nothing delays no other.
    pub(super) fn wait(
        self,
        state: &Secret,
        within: Duration,
        pages: &Pages<'_>,
    ) -> Result<Callback, Waited> {
        let until = Instant::now() + within;
        let pages = Arc::new(Rendered {
            done: page(pages.done),
            not_found: page(pages.not_found),
        });
        let served = Arc::new(AtomicBool::new(false));
        let expected: Arc<[u8]> = Arc::from(state.expose().as_bytes());
        let (found, answer) = mpsc::channel();
        let line_time = self.line_time;

        loop {
            if let Ok(callback) = answer.try_recv() {
                return Ok(callback);
            }
            if Instant::now() >= until {
                return Err(Waited::TimedOut);
            }
            match self.inner.accept() {
                Ok((stream, _)) => {
                    // A connection accepted from a non-blocking listener is non-blocking too on
                    // some platforms, and a read that waits for the request would fail at once.
                    if stream.set_nonblocking(false).is_err() {
                        continue;
                    }
                    let (served, expected, found, pages) = (
                        Arc::clone(&served),
                        Arc::clone(&expected),
                        found.clone(),
                        Arc::clone(&pages),
                    );
                    std::thread::spawn(move || {
                        serve(stream, &expected, &served, &found, line_time, &pages)
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(LOOK_AGAIN_IN);
                }
                // A connection that failed before it was accepted is not the callback.
                Err(_) => std::thread::sleep(LOOK_AGAIN_IN),
            }
        }
    }
}

/// Answer one connection, and pass on the callback where it is the first.
fn serve(
    mut stream: TcpStream,
    state: &[u8],
    served: &AtomicBool,
    found: &mpsc::Sender<Callback>,
    line_time: Duration,
    pages: &Rendered,
) {
    let Some(line) = request_line(&mut stream, line_time) else {
        return;
    };
    let callback = callback_of(&line, state);
    // Only one callback is served: a second that arrives at the same moment is answered as any
    // other request is.
    let first = callback.is_some()
        && served
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok();
    let _ = respond(
        &mut stream,
        if first { &pages.done } else { &pages.not_found },
        first,
    );
    if first && let Some(callback) = callback {
        let _ = found.send(callback);
    }
    drain(&mut stream);
}

/// The most that is read and thrown away after the reply, which is more than a browser's headers.
const DRAIN_LIMIT: usize = 64 * 1024;

/// How long a connection is read for after the reply.
const DRAIN_TIME: Duration = Duration::from_millis(500);

/// Say there is nothing more to send, then read what the peer sent after its request line and
/// throw it away, so the connection is closed with nothing unread.
///
/// Closing a socket that holds unread bytes sends a reset on most systems, and a reset can take the
/// reply with it before the browser has read it: the person would see the connection fail after the
/// callback was accepted. Nothing read here is looked at, so the listener still reads no header.
fn drain(stream: &mut TcpStream) {
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let began = Instant::now();
    let mut thrown = 0;
    let mut chunk = [0u8; 4096];
    while thrown < DRAIN_LIMIT {
        let Some(left) = DRAIN_TIME.checked_sub(began.elapsed()) else {
            return;
        };
        if stream.set_read_timeout(Some(left)).is_err() {
            return;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => thrown += read,
        }
    }
}

/// The first line of the request, or `None` where the connection did not send a complete one in
/// time and within the limit.
///
/// Nothing after the line is read, so no header is.
fn request_line(stream: &mut TcpStream, line_time: Duration) -> Option<Vec<u8>> {
    let began = Instant::now();
    let mut line = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let left = line_time.checked_sub(began.elapsed())?;
        stream
            .set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .ok()?;
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        line.extend_from_slice(&chunk[..read]);
        if let Some(end) = line.iter().position(|&byte| byte == b'\n') {
            line.truncate(end);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Some(line);
        }
        if line.len() >= LINE_LIMIT {
            return None;
        }
    }
}

/// What the request line says, where it is a `GET` for `/auth/callback` whose `state` is the one
/// generated here.
fn callback_of(line: &[u8], state: &[u8]) -> Option<Callback> {
    let line = std::str::from_utf8(line).ok()?;
    let mut words = line.split(' ');
    let (method, target) = (words.next()?, words.next()?);
    if method != "GET" {
        return None;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/auth/callback" {
        return None;
    }

    let (mut sent_state, mut code, mut client_id) = (None, None, None);
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let slot = match name {
            "state" => &mut sent_state,
            "code" => &mut code,
            "client_id" => &mut client_id,
            _ => continue,
        };
        // The first of a name that is sent twice is the one read.
        if slot.is_none() {
            *slot = Some(percent_decode(value));
        }
    }

    let sent_state = sent_state??;
    if !same(&sent_state, state) {
        return None;
    }
    Some(Callback {
        code: code
            .flatten()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .filter(|code| !code.is_empty())
            .map(Secret::new),
        client_id: client_id
            .flatten()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .filter(|id| is_client_id(id)),
    })
}

/// Whether `id` has the shape of a client identifier: not empty, not long, and made of letters,
/// digits, hyphens, underscores and periods.
fn is_client_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= CLIENT_LIMIT
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Write the fixed reply. The callback's is `200` and every other request's is `404`.
fn respond(stream: &mut TcpStream, page: &str, callback: bool) -> std::io::Result<()> {
    let status = if callback { "200 OK" } else { "404 Not Found" };
    stream.set_write_timeout(Some(LINE_TIME))?;
    write!(
        stream,
        "HTTP/1.1 {status}\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\n\
         cache-control: no-store\r\nreferrer-policy: no-referrer\r\nconnection: close\r\n\r\n{page}",
        page.len()
    )?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = "a-state-this-sign-in-made";

    const PAGES: Pages<'static> = Pages {
        done: "Return to the program.",
        not_found: "Nothing here.",
    };

    /// Send `request` to the listener's port and return what comes back, which is empty where the
    /// connection was closed without an answer.
    fn send(port: u16, request: &[u8]) -> String {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("a timeout");
        stream.write_all(request).expect("a request");
        let mut reply = String::new();
        let _ = stream.read_to_string(&mut reply);
        reply
    }

    fn get(port: u16, target: &str) -> String {
        send(
            port,
            format!("GET {target} HTTP/1.1\r\nhost: x\r\n\r\n").as_bytes(),
        )
    }

    /// A listener on its own thread, so the test can be the browser.
    fn waiting(within: Duration) -> (u16, std::thread::JoinHandle<Result<Callback, Waited>>) {
        waiting_for(LINE_TIME, within)
    }

    /// The same, where a connection has `line_time` to send its request line.
    fn waiting_for(
        line_time: Duration,
        within: Duration,
    ) -> (u16, std::thread::JoinHandle<Result<Callback, Waited>>) {
        let listener = Listener::bind_waiting(line_time).expect("a listener");
        let port = listener.port().expect("a port");
        let state = Secret::new(STATE);
        (
            port,
            std::thread::spawn(move || listener.wait(&state, within, &PAGES)),
        )
    }

    /// The listener takes only `127.0.0.1`, on a port it did not choose. A listener on all
    /// interfaces would take a callback from the network.
    #[test]
    fn the_listener_is_bound_to_the_loopback_address_only() {
        let listener = Listener::bind().expect("a listener");
        let address = listener.inner.local_addr().expect("an address");
        assert_eq!(address.ip(), std::net::IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_ne!(address.port(), 0);
    }

    /// The callback that carries the state is the one that is served, and the page it is answered
    /// with repeats nothing from the request.
    #[test]
    fn the_callback_with_the_state_is_served_and_answered_with_a_page_that_repeats_nothing() {
        let (port, waiter) = waiting(Duration::from_secs(20));

        let reply = get(
            port,
            &format!("/auth/callback?code=the-code&state={STATE}&client_id=issued-1"),
        );

        let callback = waiter.join().expect("the waiter").expect("a callback");
        assert_eq!(callback.code.expect("a code").expose(), "the-code");
        assert_eq!(callback.client_id.as_deref(), Some("issued-1"));
        assert!(reply.starts_with("HTTP/1.1 200 OK"), "{reply}");
        assert!(reply.contains("Return to the program."), "{reply}");
        assert!(!reply.contains("the-code"), "{reply}");
        assert!(!reply.contains(STATE), "{reply}");
    }

    /// The words on each page are the caller's, and are escaped into the page: a word with markup
    /// in it is text on the page and not markup, so a catalog entry cannot put a script on a page
    /// the person's browser loads from a port other programs can reach. The two pages are not the
    /// same page.
    #[test]
    fn each_page_carries_the_callers_words_escaped_and_the_two_differ() {
        let rendered = page("a <b>\"c\" & 'd'</b>");

        assert_eq!(
            rendered,
            "<!doctype html><meta charset=\"utf-8\"><title>bravebot</title>\
             <p>a &lt;b&gt;&quot;c&quot; &amp; &#39;d&#39;&lt;/b&gt;</p>"
        );
        assert!(!rendered.contains("<b>"));
        assert_ne!(page(PAGES.done), page(PAGES.not_found));
    }

    /// A request that is not the callback does not end the sign-in, whatever it is: another
    /// method, another path, no state or the wrong one. Each is answered with the fixed page and
    /// the listener goes on waiting, so one request from a page or a program cannot end every
    /// sign-in.
    #[test]
    fn a_request_that_is_not_the_callback_is_answered_and_ignored_and_the_wait_goes_on() {
        let (port, waiter) = waiting(Duration::from_secs(20));

        for request in [
            "POST /auth/callback?code=c&state=a-state-this-sign-in-made HTTP/1.1\r\n\r\n",
            "GET /other?code=c&state=a-state-this-sign-in-made HTTP/1.1\r\n\r\n",
            "GET /auth/callback?code=c HTTP/1.1\r\n\r\n",
            "GET /auth/callback?code=c&state=another HTTP/1.1\r\n\r\n",
            "GET /auth/callback?code=c&state=a-state-this-sign-in-mad HTTP/1.1\r\n\r\n",
            "GET /auth/callback?error=access_denied&state=wrong HTTP/1.1\r\n\r\n",
            "GET / HTTP/1.1\r\n\r\n",
        ] {
            let reply = send(port, request.as_bytes());
            assert!(reply.starts_with("HTTP/1.1 404"), "{request}: {reply}");
            assert!(!reply.contains("c&state"), "{reply}");
        }
        assert!(
            !waiter.is_finished(),
            "a request that was not the callback ended the wait"
        );

        let reply = get(port, &format!("/auth/callback?code=real&state={STATE}"));
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        let callback = waiter.join().expect("the waiter").expect("a callback");
        assert_eq!(callback.code.expect("a code").expose(), "real");
    }

    /// A callback with the state and no code is the callback and ends the sign-in, whatever else it
    /// carries: it reaches the caller with no code, and an `error` the server put there is not read.
    #[test]
    fn a_callback_with_no_code_is_returned_as_one_with_none() {
        let (port, waiter) = waiting(Duration::from_secs(20));

        get(
            port,
            &format!("/auth/callback?error=access_denied&state={STATE}"),
        );

        let callback = waiter.join().expect("the waiter").expect("a callback");
        assert!(callback.code.is_none());
    }

    /// Run `serve` over one connection, as the browser would make it, and return what came back
    /// and whether the callback was handed on.
    fn through_serve(request: &str, served: &AtomicBool, found: &mpsc::Sender<Callback>) -> String {
        let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a socket");
        let port = socket.local_addr().expect("an address").port();
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        let (stream, _) = socket.accept().expect("the connection");
        client.write_all(request.as_bytes()).expect("a request");
        let pages = Rendered {
            done: page(PAGES.done),
            not_found: page(PAGES.not_found),
        };
        serve(stream, STATE.as_bytes(), served, found, LINE_TIME, &pages);
        let mut reply = String::new();
        let _ = client.read_to_string(&mut reply);
        reply
    }

    /// The first callback is the only one served. A second that carries the right state, arriving
    /// after the first was taken, is answered as any other request is and hands nothing on, so the
    /// code the caller gets is the first one's.
    #[test]
    fn only_the_first_callback_is_served() {
        let served = AtomicBool::new(false);
        let (found, taken) = mpsc::channel();

        let first = through_serve(
            &format!("GET /auth/callback?code=first&state={STATE} HTTP/1.1\r\n"),
            &served,
            &found,
        );
        let second = through_serve(
            &format!("GET /auth/callback?code=second&state={STATE} HTTP/1.1\r\n"),
            &served,
            &found,
        );

        assert!(first.starts_with("HTTP/1.1 200"), "{first}");
        assert!(second.starts_with("HTTP/1.1 404"), "{second}");
        assert_eq!(
            taken
                .try_recv()
                .expect("the first callback")
                .code
                .expect("a code")
                .expose(),
            "first"
        );
        assert!(taken.try_recv().is_err(), "a second callback was handed on");
    }

    /// A value is read from the query percent-decoded, the first of a name sent twice is the one
    /// read, and the state is compared decoded.
    #[test]
    fn the_query_is_decoded_and_the_first_of_a_repeated_name_is_read() {
        let state = STATE.as_bytes();
        let read = |query: &str| {
            callback_of(
                format!("GET /auth/callback?{query} HTTP/1.1").as_bytes(),
                state,
            )
        };

        let callback =
            read(&format!("code=a%2Bb%3Dc&code=other&state={STATE}")).expect("a callback");
        assert_eq!(callback.code.expect("a code").expose(), "a+b=c");

        // A first state that is wrong is not rescued by a second that is right.
        assert!(read(&format!("state=wrong&state={STATE}")).is_none());
        // The same state written with an escape is the same state.
        assert!(read("state=a-state-this-sign-in-m%61de").is_some());
    }

    /// A `client_id` of the wrong shape is not used: empty, too long, or with a character outside
    /// letters, digits, hyphen, underscore and period. The callback is still the callback.
    #[test]
    fn a_client_identifier_of_the_wrong_shape_is_dropped() {
        let longest = "x".repeat(128);
        let too_long = "x".repeat(129);
        for (given, kept) in [
            ("issued-1.a_b", Some("issued-1.a_b")),
            (longest.as_str(), Some(longest.as_str())),
            ("", None),
            ("has%20space", None),
            ("has%2Fslash", None),
            ("semi;colon", None),
            (too_long.as_str(), None),
        ] {
            let line =
                format!("GET /auth/callback?state={STATE}&code=c&client_id={given} HTTP/1.1");
            let callback = callback_of(line.as_bytes(), STATE.as_bytes()).expect("a callback");
            assert_eq!(callback.client_id.as_deref(), kept, "{given}");
        }
    }

    /// A connection that sends nothing delays no other: the callback is served while it is still
    /// open. The silent connection is given two minutes, far longer than the test takes, so the
    /// callback being served at all is the evidence that it was not waited behind.
    #[test]
    fn a_connection_that_sends_nothing_delays_no_other() {
        let (port, waiter) = waiting_for(Duration::from_secs(120), Duration::from_secs(60));
        let mut silent = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");

        get(port, &format!("/auth/callback?code=c&state={STATE}"));

        let callback = waiter.join().expect("the waiter").expect("a callback");
        assert_eq!(callback.code.expect("a code").expose(), "c");
        silent
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("a timeout");
        let mut byte = [0u8; 1];
        assert!(
            matches!(
                silent.read(&mut byte),
                Err(ref e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
            ),
            "the silent connection was closed or answered before the callback was"
        );
    }

    /// A connection that has not sent a complete request line when its time is up is closed with
    /// no answer, so a program that opens a connection and says nothing holds it for no longer.
    #[test]
    fn a_connection_that_sends_nothing_is_closed_when_its_time_is_up() {
        let (port, waiter) = waiting_for(Duration::from_millis(200), Duration::from_secs(60));
        let mut silent = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        silent
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("a timeout");

        let mut reply = Vec::new();
        let read = silent.read_to_end(&mut reply);

        assert!(read.is_ok(), "the connection was left open: {read:?}");
        assert!(reply.is_empty(), "{reply:?}");
        assert!(!waiter.is_finished());
        get(port, &format!("/auth/callback?code=c&state={STATE}"));
        waiter.join().expect("the waiter").expect("a callback");
    }

    /// A request line that is never completed is not read past the limit: 8 KiB with no line end
    /// is closed with no answer at once, where a read without a limit would hold the connection
    /// and the memory for as long as the sender liked. The connection is given two minutes, so a
    /// close that comes only when its time is up is not seen inside the ten seconds waited here.
    #[test]
    fn a_request_line_longer_than_the_limit_is_closed_without_an_answer() {
        let (port, waiter) = waiting_for(Duration::from_secs(120), Duration::from_secs(60));
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("a timeout");
        stream
            .write_all(&vec![b'a'; LINE_LIMIT + 1024])
            .expect("a request");

        let mut reply = Vec::new();
        let read = stream.read_to_end(&mut reply);

        // Closed with bytes unread, which a peer sees as a reset as often as an end of stream.
        assert!(
            !matches!(
                read,
                Err(ref e) if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                )
            ),
            "the connection was left open: {read:?}"
        );
        assert!(reply.is_empty(), "{reply:?}");
        assert!(!waiter.is_finished());
        get(port, &format!("/auth/callback?code=c&state={STATE}"));
        waiter.join().expect("the waiter").expect("a callback");
    }

    /// No callback inside the bound is reported as the time passing, and the listener is gone.
    #[test]
    fn a_wait_that_sees_no_callback_ends_at_the_bound() {
        let (port, waiter) = waiting(Duration::from_millis(200));

        assert_eq!(
            waiter.join().expect("the waiter").err(),
            Some(Waited::TimedOut)
        );
        assert!(
            TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err(),
            "the port is still open after the wait ended"
        );
    }

    /// A browser sends a good deal after the request line, and a reply followed by a close with those
    /// bytes unread can be answered with a reset that takes the page with it. The whole reply is
    /// read by the client here, headers of several kilobytes after the line included.
    #[test]
    fn a_reply_reaches_a_client_that_sent_more_than_its_request_line() {
        let (port, waiter) = waiting(Duration::from_secs(20));
        let headers = format!("user-agent: {}\r\n", "a".repeat(6000));

        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("a timeout");
        stream
            .write_all(
                format!("GET /auth/callback?code=c&state={STATE} HTTP/1.1\r\n{headers}\r\n")
                    .as_bytes(),
            )
            .expect("a request");
        let mut reply = String::new();
        let read = stream.read_to_string(&mut reply);

        assert!(
            read.is_ok(),
            "the reply was lost to a reset: {read:?} {reply:?}"
        );
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        waiter.join().expect("the waiter").expect("a callback");
    }

    /// The request line is read up to its end and no further, so a header is never read: a request
    /// that sends the line and nothing else is answered at once. The connection is given two
    /// minutes to finish, so a listener that waited for the headers would send no reply inside the
    /// ten seconds the browser's side waits for one.
    #[test]
    fn the_request_line_is_all_that_is_read() {
        let (port, waiter) = waiting_for(Duration::from_secs(120), Duration::from_secs(60));

        let reply = send(
            port,
            format!("GET /auth/callback?code=c&state={STATE} HTTP/1.1\r\n").as_bytes(),
        );

        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        waiter.join().expect("the waiter").expect("a callback");
    }
}
