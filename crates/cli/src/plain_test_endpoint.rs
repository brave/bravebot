//! Canned loopback responses, following the CLI integration tests' std-only fixtures.
use bravebot_config::Config;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub fn tool(name: &str, arguments: &str) -> String {
    // These fixtures contain only ASCII JSON objects, with no control characters.
    assert!(
        arguments
            .bytes()
            .all(|b| b.is_ascii() && !b.is_ascii_control())
    );
    let arguments = arguments.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"data: {{"choices":[{{"delta":{{"tool_calls":[{{"index":0,"id":"call","type":"function","function":{{"name":"{name}","arguments":"{arguments}"}}}}]}},"finish_reason":"tool_calls"}}]}}"#
    ) + "\n\ndata: [DONE]\n\n"
}

pub fn answer() -> String {
    "data: {\"choices\":[{\"delta\":{\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".into()
}

pub fn endpoint(
    replies: Vec<String>,
) -> (Config, mpsc::Receiver<String>, std::thread::JoinHandle<()>) {
    const LIMIT: Duration = Duration::from_secs(10);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut next = 0;
        while next < replies.len() {
            let until = Instant::now() + LIMIT;
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < until, "request {next} never arrived");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("accept: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(LIMIT)).unwrap();
            stream.set_write_timeout(Some(LIMIT)).unwrap();
            let mut reader = BufReader::new(stream);
            let mut length = 0;
            loop {
                let mut header = String::new();
                assert!(reader.read_line(&mut header).unwrap() > 0);
                if header == "\r\n" {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let body = String::from_utf8(body).unwrap();
            // A classifier side request is answered without advancing the planner script.
            let checking = body.contains("You are a prompt-injection classifier.");
            let reply = if checking {
                answer()
            } else {
                tx.send(body).unwrap();
                let reply = replies[next].clone();
                next += 1;
                reply
            };
            let response = if reply == "fail" {
                "HTTP/1.1 418 Teapot\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
            };
            let _ = reader.into_inner().write_all(response.as_bytes());
        }
    });
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(address.clone()),
        _ => None,
    })
    .unwrap();
    (config, rx, server)
}
