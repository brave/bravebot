//! Servers on this machine that answer as a model server does, for the tests of what the import
//! asks them (`docs/specs/import.md`, IMPORT-10 and IMPORT-12).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

/// A server on this machine answering one connection with `response`, and its base URL.
pub(crate) fn serve(response: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            answer(stream, &response);
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn answer(mut stream: TcpStream, response: &[u8]) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        if line == "\r\n" || line == "\n" {
            break;
        }
        line.clear();
    }
    let _ = stream.write_all(response);
    let _ = stream.flush();
}

pub(crate) fn http(status: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

/// A server on this machine answering each request by its path from `routes`, and a 404 for any
/// other, until the test ends. Returns its base URL.
pub(crate) fn serve_paths(routes: Vec<(&'static str, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request = String::new();
            let _ = reader.read_line(&mut request);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if line == "\r\n" || line == "\n" {
                    break;
                }
                line.clear();
            }
            let path = request.split_whitespace().nth(1).unwrap_or("");
            let response = routes
                .iter()
                .find(|(route, _)| *route == path)
                .map(|(_, response)| response.clone())
                .unwrap_or_else(|| http("404 Not Found", b"{}"));
            let _ = stream.write_all(&response);
            let _ = stream.flush();
        }
    });
    format!("http://127.0.0.1:{port}")
}
