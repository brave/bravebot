//! A server a test runs in place of an authorization server, and the browser a test is.

use super::encoding::percent_decode;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What one request to the stand-in said.
#[derive(Debug, Clone)]
pub(crate) struct Received {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
}

impl Received {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The form body as decoded names and values, in the order sent.
    pub(crate) fn form(&self) -> Vec<(String, String)> {
        self.body
            .split('&')
            .map(|pair| {
                let (name, value) = pair.split_once('=').expect("a name and a value");
                let value = percent_decode(value).expect("an encoded value");
                (name.to_string(), String::from_utf8(value).expect("text"))
            })
            .collect()
    }
}

/// One reply the stand-in gives.
#[derive(Clone)]
pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
    /// What happens after the request is read and before the reply is written, which is where
    /// another process can act while this one waits for its answer.
    pub(crate) meanwhile: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    /// Read the request and close the connection without answering it, as a connection that is cut
    /// does.
    pub(crate) hangs_up: bool,
    /// Offer the connection for reuse, as a server that keeps connections alive does, and hang up on any
    /// request that arrives on it, as a server or an intermediary that has closed an idle connection
    /// does to a client that has not yet noticed.
    pub(crate) offers_reuse: bool,
}

impl Reply {
    pub(crate) fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), "application/json".into())],
            body: body.to_string(),
            meanwhile: None,
            hangs_up: false,
            offers_reuse: false,
        }
    }

    /// A reply that is no reply: the request is read and the connection is closed.
    pub(crate) fn hang_up() -> Self {
        Self {
            hangs_up: true,
            ..Self::json(200, "")
        }
    }

    /// This reply, which says the connection may be used again, and a connection that is then used
    /// again is hung up on without an answer.
    pub(crate) fn offering_reuse_then_hanging_up(mut self) -> Self {
        self.offers_reuse = true;
        self
    }

    /// This reply, written only after `act` has run, which a test uses to have another process do
    /// something while the one under test waits for its answer.
    pub(crate) fn after(mut self, act: impl Fn() + Send + Sync + 'static) -> Self {
        self.meanwhile = Some(std::sync::Arc::new(act));
        self
    }

    pub(crate) fn redirect(to: &str) -> Self {
        Self {
            status: 302,
            headers: vec![("location".into(), to.to_string())],
            body: String::new(),
            meanwhile: None,
            hangs_up: false,
            offers_reuse: false,
        }
    }
}

/// A server on a loopback port that answers each connection with the next reply and records what
/// was asked.
pub(crate) struct Server {
    pub(crate) base: String,
    served: Arc<AtomicUsize>,
    accepted: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    replies: Arc<Mutex<VecDeque<Reply>>>,
    thread: std::thread::JoinHandle<Vec<Received>>,
}

impl Server {
    /// Add replies after the ones it was started with, for a reply that holds a value the sign-in
    /// made after the server started, such as a token naming its nonce.
    pub(crate) fn then(&self, replies: Vec<Reply>) {
        self.replies.lock().expect("the replies").extend(replies);
    }

    /// How many requests have been taken so far, which a test reads at the moment something is shown
    /// to find out what had been asked by then.
    pub(crate) fn served(&self) -> usize {
        self.served.load(Ordering::SeqCst)
    }

    /// How many connections have been accepted so far, which a test reads to find out whether two
    /// requests shared one.
    pub(crate) fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }

    /// Stop serving and return every request received.
    pub(crate) fn finish(self) -> Vec<Received> {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.join().expect("the server thread")
    }
}

pub(crate) fn serve(replies: Vec<Reply>) -> Server {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a stand-in listener");
    listener
        .set_nonblocking(true)
        .expect("a non-blocking listener");
    let base = format!("http://{}", listener.local_addr().expect("an address"));
    let stop = Arc::new(AtomicBool::new(false));
    let served = Arc::new(AtomicUsize::new(0));
    let accepted = Arc::new(AtomicUsize::new(0));
    let replies = Arc::new(Mutex::new(VecDeque::from(replies)));
    let thread = std::thread::spawn({
        let stop = Arc::clone(&stop);
        let served = Arc::clone(&served);
        let accepted = Arc::clone(&accepted);
        let replies = Arc::clone(&replies);
        move || {
            let mut received = Vec::new();
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        accepted.fetch_add(1, Ordering::SeqCst);
                        let reply = replies
                            .lock()
                            .expect("the replies")
                            .pop_front()
                            .expect("a reply for this request");
                        // Counted before the reply is written, so a client that has read the reply
                        // finds the request counted.
                        served.fetch_add(1, Ordering::SeqCst);
                        received.push(answer(stream, &reply));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
            received
        }
    });
    Server {
        base,
        served,
        accepted,
        stop,
        replies,
        thread,
    }
}

fn answer(stream: TcpStream, reply: &Reply) -> Received {
    stream.set_nonblocking(false).expect("a blocking stream");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a timeout");
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    reader.read_line(&mut first).expect("a request line");
    let mut words = first.split_whitespace();
    let (method, path) = (
        words.next().expect("a method").to_string(),
        words.next().expect("a path").to_string(),
    );
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("a header");
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').expect("a header");
        headers.push((name.to_string(), value.trim().to_string()));
    }
    let length: usize = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map_or(0, |(_, value)| value.parse().expect("a length"));
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).expect("a body");

    if let Some(act) = &reply.meanwhile {
        act();
    }
    let mut stream = reader.into_inner();
    if reply.hangs_up {
        return Received {
            method,
            path,
            headers,
            body: String::from_utf8(body).expect("a text body"),
        };
    }
    let mut text = format!(
        "HTTP/1.1 {} Stand-in\r\ncontent-length: {}\r\nconnection: {}\r\n",
        reply.status,
        reply.body.len(),
        if reply.offers_reuse {
            "keep-alive"
        } else {
            "close"
        }
    );
    for (name, value) in &reply.headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str("\r\n");
    text.push_str(&reply.body);
    stream.write_all(text.as_bytes()).expect("a reply");
    stream.flush().expect("a flush");
    if reply.offers_reuse {
        // The connection stays open on a thread of its own, so the next connection is accepted, and a
        // request that comes on this one is read and left unanswered.
        std::thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut byte = [0_u8; 1];
            let _ = stream.read(&mut byte);
        });
    }

    Received {
        method,
        path,
        headers,
        body: String::from_utf8(body).expect("a text body"),
    }
}

/// The query of `url` as decoded names and values, in the order written.
pub(crate) fn parameters(url: &str) -> Vec<(String, String)> {
    let (_, query) = url.split_once('?').expect("a query");
    query
        .split('&')
        .map(|pair| {
            let (name, value) = pair.split_once('=').expect("a name and a value");
            let value = percent_decode(value).expect("an encoded value");
            (name.to_string(), String::from_utf8(value).expect("text"))
        })
        .collect()
}

/// The value of one named parameter of `found`.
pub(crate) fn named<'a>(found: &'a [(String, String)], name: &str) -> &'a str {
    found
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("no {name} in {found:?}"))
}

/// What the person's browser does after the sign-in page: a `GET` to the listener's port.
pub(crate) fn visit(port: u16, target: &str) -> String {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a timeout");
    stream
        .write_all(format!("GET {target} HTTP/1.1\r\nhost: x\r\n\r\n").as_bytes())
        .expect("a request");
    let mut reply = String::new();
    let _ = stream.read_to_string(&mut reply);
    reply
}
