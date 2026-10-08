//! The proxy a session runs so a confined program with egress reaches only the hosts a list
//! names.
//!
//! The confinement backends filter by address and port, so a stage is told the one loopback port
//! to reach and this proxy decides on the host name. It decides from the destination named in a
//! `CONNECT` request line and from nothing else: not a header, not a reply, not a byte of what
//! the tunnel carries. TLS is not terminated, so an allowed tunnel is two byte streams copied
//! into each other without being read, and a request that is not a `CONNECT` is refused.
//! `docs/specs/sandboxing.md` ([SANDBOX-24]) decides the rest.
//!
//! [SANDBOX-24]: ../../../docs/specs/sandboxing.md

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::hosts::{HostList, Refusal, Verdict};

/// The ports a tunnel may be opened to unless a caller says otherwise.
pub const DEFAULT_PORTS: &[u16] = &[443, 80];

/// The longest request head read. A client that has not finished its head by then is dropped.
const MAX_HEAD: usize = 8 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// What a refused tunnel is answered with. It is the same for every host and every reason, so a
/// program learns nothing from it and the planner is handed nothing a program chose.
const REFUSAL: &[u8] = b"HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\n\
Content-Length: 41\r\nConnection: close\r\n\r\n\
refused by the session's allowed-hosts list";
const NOT_A_TUNNEL: &[u8] = b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\
Connection: close\r\n\r\n";
const UNREACHABLE: &[u8] =
    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// One decision the proxy took: the host a request named and what the list said of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub host: String,
    pub verdict: Verdict,
}

/// What a proxy is started with.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub list: HostList,
    /// The ports a tunnel may be opened to.
    pub ports: Vec<u16>,
}

impl ProxyConfig {
    pub fn new(list: HostList) -> Self {
        Self {
            list,
            ports: DEFAULT_PORTS.to_vec(),
        }
    }
}

/// A running proxy. It listens on a loopback port until it is dropped.
pub struct Proxy {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    decisions: Arc<Mutex<Vec<Decision>>>,
    accept: Option<JoinHandle<()>>,
}

impl Proxy {
    /// Starts listening on a loopback port the system chooses.
    pub fn start(config: ProxyConfig) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let decisions = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::new(config);
        let accept = {
            let (stop, decisions) = (stop.clone(), decisions.clone());
            thread::Builder::new()
                .name("allowed-hosts-proxy".into())
                .spawn(move || {
                    for stream in listener.incoming() {
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let Ok(stream) = stream else { continue };
                        let (config, decisions) = (shared.clone(), decisions.clone());
                        let _ = thread::Builder::new()
                            .name("allowed-hosts-tunnel".into())
                            .spawn(move || {
                                let _ = serve(stream, &config, &decisions);
                            });
                    }
                })?
        };
        Ok(Self {
            addr,
            stop,
            decisions,
            accept: Some(accept),
        })
    }

    /// The loopback address a stage is allowed to reach.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The variables that point a program at this proxy, upper and lower case. The caller sets
    /// them after the person's own environment is applied, so an assignment a model wrote cannot
    /// point a stage elsewhere.
    pub fn environment(&self) -> Vec<(String, String)> {
        let url = format!("http://{}", self.addr);
        ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"]
            .into_iter()
            .flat_map(|name| [name.to_string(), name.to_ascii_lowercase()])
            .map(|name| (name, url.clone()))
            .collect()
    }

    /// Every decision taken so far, oldest first.
    pub fn decisions(&self) -> Vec<Decision> {
        self.decisions
            .lock()
            .map(|decisions| decisions.clone())
            .unwrap_or_default()
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // A blocked `accept` only returns for a connection, so make one.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
    }
}

fn serve(
    mut client: TcpStream,
    config: &ProxyConfig,
    decisions: &Mutex<Vec<Decision>>,
) -> io::Result<()> {
    client.set_read_timeout(Some(HEAD_TIMEOUT))?;
    let (head, rest) = read_head(&mut client)?;
    let Some((host, port)) = connect_target(&head) else {
        return client.write_all(NOT_A_TUNNEL);
    };
    let mut verdict = config.list.decide(&host);
    if verdict.is_allowed() && !config.ports.contains(&port) {
        verdict = Verdict::Refused(Refusal::Port);
    }
    if let Ok(mut log) = decisions.lock() {
        log.push(Decision {
            host: host.clone(),
            verdict: verdict.clone(),
        });
    }
    if !verdict.is_allowed() {
        return client.write_all(REFUSAL);
    }
    let Some(upstream) = open(&host, port) else {
        return client.write_all(UNREACHABLE);
    };
    client.set_read_timeout(None)?;
    client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
    tunnel(client, upstream, &rest)
}

/// Reads up to the blank line ending a request head. Returns the head and any bytes that arrived
/// after it, which belong to the tunnel.
fn read_head(client: &mut TcpStream) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            let rest = buffer.split_off(end + 4);
            return Ok((buffer, rest));
        }
        if buffer.len() > MAX_HEAD {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "head too long"));
        }
        let read = client.read(&mut chunk)?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

/// The host and port of a `CONNECT host:port HTTP/1.x` request line, or `None` for any other
/// request. Only the request line is read: a header cannot name a different destination.
fn connect_target(head: &[u8]) -> Option<(String, u16)> {
    let line = std::str::from_utf8(head).ok()?.lines().next()?;
    let mut parts = line.split(' ');
    let (method, target, version) = (parts.next()?, parts.next()?, parts.next()?);
    if method != "CONNECT" || !version.starts_with("HTTP/1.") || parts.next().is_some() {
        return None;
    }
    let (host, port) = target.rsplit_once(':')?;
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let port = port.parse().ok()?;
    (!host.is_empty()).then(|| (host.to_string(), port))
}

fn open(host: &str, port: u16) -> Option<TcpStream> {
    (host, port)
        .to_socket_addrs()
        .ok()?
        .find_map(|addr| TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok())
}

/// Copies each stream into the other until both have finished. Nothing is parsed.
fn tunnel(client: TcpStream, upstream: TcpStream, first: &[u8]) -> io::Result<()> {
    let mut upstream_writer = upstream.try_clone()?;
    upstream_writer.write_all(first)?;
    let mut client_reader = client.try_clone()?;
    let outbound = thread::spawn(move || {
        let _ = io::copy(&mut client_reader, &mut upstream_writer);
        let _ = upstream_writer.shutdown(Shutdown::Write);
    });
    let (mut upstream_reader, mut client_writer) = (upstream, client);
    let _ = io::copy(&mut upstream_reader, &mut client_writer);
    let _ = client_writer.shutdown(Shutdown::Write);
    let _ = outbound.join();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    /// A server that echoes what its first connection sends.
    fn echo_server() -> (SocketAddr, JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 256];
                while let Ok(read) = stream.read(&mut buffer) {
                    if read == 0 || stream.write_all(&buffer[..read]).is_err() {
                        break;
                    }
                }
            }
        });
        (addr, handle)
    }

    fn proxy_for(allowed: &[&str], denied: &[&str], ports: Vec<u16>) -> Proxy {
        let (list, invalid) = HostList::parse(allowed.iter().copied(), denied.iter().copied());
        assert!(invalid.is_empty());
        Proxy::start(ProxyConfig { list, ports }).unwrap()
    }

    /// Sends a `CONNECT` and returns the status line and the stream, read up to the end of the
    /// reply head.
    fn connect(proxy: &Proxy, target: &str) -> (String, io::BufReader<TcpStream>) {
        let mut stream = TcpStream::connect(proxy.addr()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(
            stream,
            "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n"
        )
        .unwrap();
        let mut reader = io::BufReader::new(stream);
        let mut status = String::new();
        reader.read_line(&mut status).unwrap();
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
            line.clear();
        }
        (status.trim().to_string(), reader)
    }

    #[test]
    fn a_listed_host_is_tunnelled_and_an_unlisted_one_is_refused() {
        let (server, _handle) = echo_server();
        let proxy = proxy_for(&["127.0.0.1"], &[], vec![server.port()]);

        const PAYLOAD: &[u8] = b"\x16\x03\x01 not parsed";
        let (status, mut stream) = connect(&proxy, &format!("127.0.0.1:{}", server.port()));
        assert!(status.starts_with("HTTP/1.1 200"), "{status}");
        stream.get_mut().write_all(PAYLOAD).unwrap();
        let mut echoed = vec![0u8; PAYLOAD.len()];
        stream.read_exact(&mut echoed).unwrap();
        assert_eq!(echoed, PAYLOAD);

        let (status, _) = connect(&proxy, "evil.example:443");
        assert!(status.starts_with("HTTP/1.1 403"), "{status}");
    }

    #[test]
    fn a_denied_host_is_refused_although_an_allowed_entry_covers_it() {
        let (server, _handle) = echo_server();
        let proxy = proxy_for(&["127.0.0.1"], &["127.0.0.1"], vec![server.port()]);
        let (status, _) = connect(&proxy, &format!("127.0.0.1:{}", server.port()));
        assert!(status.starts_with("HTTP/1.1 403"), "{status}");
    }

    #[test]
    fn a_listed_host_is_refused_on_a_port_the_proxy_does_not_carry() {
        let (server, _handle) = echo_server();
        let proxy = proxy_for(&["127.0.0.1"], &[], vec![443]);
        let (status, _) = connect(&proxy, &format!("127.0.0.1:{}", server.port()));
        assert!(status.starts_with("HTTP/1.1 403"), "{status}");
        assert_eq!(
            proxy.decisions().last().unwrap().verdict,
            Verdict::Refused(Refusal::Port)
        );
    }

    #[test]
    fn a_refusal_is_the_same_bytes_whatever_host_was_asked_for() {
        let proxy = proxy_for(&[], &["denied.example"], vec![443]);
        let mut bodies = Vec::new();
        for target in ["a.example:443", "denied.example:443", "b.example:8080"] {
            let mut stream = TcpStream::connect(proxy.addr()).unwrap();
            write!(stream, "CONNECT {target} HTTP/1.1\r\n\r\n").unwrap();
            let mut body = Vec::new();
            stream.read_to_end(&mut body).unwrap();
            bodies.push(body);
        }
        assert!(bodies.windows(2).all(|pair| pair[0] == pair[1]));
        let text = String::from_utf8(bodies.remove(0)).unwrap();
        assert!(text.starts_with("HTTP/1.1 403"));
        for target in ["example", "denied", "8080"] {
            assert!(!text.contains(target), "{text}");
        }
    }

    #[test]
    fn the_host_is_read_from_the_request_line_and_not_from_a_header() {
        let proxy = proxy_for(&["allowed.example"], &[], vec![443]);
        let mut stream = TcpStream::connect(proxy.addr()).unwrap();
        stream
            .write_all(b"CONNECT evil.example:443 HTTP/1.1\r\nHost: allowed.example:443\r\n\r\n")
            .unwrap();
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).unwrap();
        assert!(reply.starts_with(b"HTTP/1.1 403"));
        assert_eq!(proxy.decisions()[0].host, "evil.example");
    }

    #[test]
    fn a_request_that_is_not_a_connect_is_refused_without_a_decision() {
        let proxy = proxy_for(&["example.com"], &[], vec![80]);
        for request in [
            "GET http://example.com/ HTTP/1.1\r\n\r\n",
            "CONNECT example.com HTTP/1.1\r\n\r\n",
            "CONNECT example.com:443 HTTP/1.1 extra\r\n\r\n",
        ] {
            let mut stream = TcpStream::connect(proxy.addr()).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut reply = Vec::new();
            stream.read_to_end(&mut reply).unwrap();
            assert!(reply.starts_with(b"HTTP/1.1 405"), "{request:?}");
        }
        assert!(proxy.decisions().is_empty());
    }

    #[test]
    fn every_decision_is_recorded_with_the_host_and_the_rule_that_decided_it() {
        let (server, _handle) = echo_server();
        let proxy = proxy_for(
            &["*.example.com", "127.0.0.1"],
            &["bad.example.com"],
            vec![server.port()],
        );
        let _ = connect(&proxy, &format!("127.0.0.1:{}", server.port()));
        let _ = connect(&proxy, "bad.example.com:443");
        let _ = connect(&proxy, "other.example:443");
        let decisions = proxy.decisions();
        let hosts: Vec<_> = decisions.iter().map(|d| d.host.as_str()).collect();
        assert_eq!(hosts, ["127.0.0.1", "bad.example.com", "other.example"]);
        assert!(
            matches!(&decisions[0].verdict, Verdict::Allowed(rule) if rule.spelling() == "127.0.0.1")
        );
        assert!(
            matches!(&decisions[1].verdict, Verdict::Refused(Refusal::Denied(rule)) if rule.spelling() == "bad.example.com")
        );
        assert_eq!(decisions[2].verdict, Verdict::Refused(Refusal::NotListed));
    }

    #[test]
    fn the_environment_points_every_proxy_variable_at_the_loopback_port() {
        let proxy = proxy_for(&[], &[], vec![443]);
        let environment = proxy.environment();
        let url = format!("http://127.0.0.1:{}", proxy.addr().port());
        for name in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            assert!(
                environment.contains(&(name.to_string(), url.clone())),
                "{name}"
            );
        }
        assert!(proxy.addr().ip().is_loopback());
    }

    #[test]
    fn dropping_the_proxy_stops_it_listening() {
        let proxy = proxy_for(&[], &[], vec![443]);
        let addr = proxy.addr();
        drop(proxy);
        assert!(TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_err());
    }

    #[test]
    fn a_request_line_is_read_as_a_host_and_a_port() {
        for (head, expected) in [
            (
                "CONNECT github.com:443 HTTP/1.1\r\n\r\n",
                Some(("github.com", 443)),
            ),
            ("CONNECT [::1]:443 HTTP/1.0\r\n\r\n", Some(("::1", 443))),
            ("CONNECT github.com:99999 HTTP/1.1\r\n\r\n", None),
            ("CONNECT :443 HTTP/1.1\r\n\r\n", None),
            ("connect github.com:443 HTTP/1.1\r\n\r\n", None),
            ("CONNECT github.com:443 SPDY/3\r\n\r\n", None),
        ] {
            assert_eq!(
                connect_target(head.as_bytes()),
                expected.map(|(host, port)| (host.to_string(), port)),
                "{head:?}"
            );
        }
    }
}
