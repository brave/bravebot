//! Scripted planner endpoint shared by frontend interruption tests.
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub const LIMIT: Duration = Duration::from_secs(10);

pub fn tool(name: &str, arguments: serde_json::Value) -> String {
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call","type":"function","function":{"name":name,"arguments":arguments.to_string()}}]},"finish_reason":"tool_calls"}]})
    )
}

pub fn answer() -> String {
    "data: {\"choices\":[{\"delta\":{\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".into()
}

// A request after write_file proves the effect and its reconciliation have returned.
pub fn endpoint(
    replies: Vec<String>,
    cancel_at: Option<(usize, Cancel)>,
) -> (Config, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let mut held = Vec::new();
        let mut index = 0;
        while index < replies.len() {
            let until = std::time::Instant::now() + LIMIT;
            let (stream, _) = loop {
                match listener.accept() {
                    Ok(stream) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < until,
                            "request {index} never arrived"
                        );
                        thread::sleep(Duration::from_millis(5));
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
                let mut line = String::new();
                assert!(
                    reader.read_line(&mut line).unwrap() > 0,
                    "incomplete HTTP headers"
                );
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let mut stream = reader.into_inner();
            let body = String::from_utf8(body).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
            // Final planner rounds also omit tools. Only the classifier's system role
            // identifies the side request; user or file content must not select it.
            let checking = parsed["messages"].as_array().is_some_and(|messages| {
                messages.iter().any(|message| {
                    message["role"] == "system"
                        && message["content"]
                            .as_str()
                            .or_else(|| message["content"][0]["text"].as_str())
                            .is_some_and(|content| {
                                content.starts_with("You are a prompt-injection classifier.")
                            })
                })
            });
            let reply = if checking {
                answer()
            } else {
                replies[index].clone()
            };
            if !checking {
                tx.send(body).unwrap();
            }
            if let Some((at, cancel)) = &cancel_at
                && !checking
                && index == *at
            {
                cancel.cancel();
                held.push(stream);
                index += 1;
                continue;
            }
            let response = if reply == "fail" {
                "HTTP/1.1 418 Teapot\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
            };
            let _ = stream.write_all(response.as_bytes());
            if !checking {
                index += 1;
            }
        }
    });
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(address.clone()),
        _ => None,
    })
    .unwrap();
    (config, rx, worker)
}
