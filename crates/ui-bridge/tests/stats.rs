//! What `session.stats` totals from the recorded sessions, and what it refuses (CLI-37).
//!
//! Driven through the binary, because the totals read the sessions directory under the process's
//! home, and only a home of the test's own holds exactly the records the test wrote.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use bravebot_session::sessions::key_for;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(path.join("home/.bravebot")).expect("a home directory");
    path.canonicalize().expect("a real scratch directory")
}

/// Ask a fresh binary one question, in a home of the test's own, and return its answer.
fn ask(home: &Path, method: &str, params: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .env_clear()
        .env("HOME", home)
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:9")
        .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", "http://127.0.0.1:9")
        .env("BRAVEBOT_DEFAULT_MODEL", "stub-model")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the built binary runs");
    let line = json!({"id": 1, "method": method, "params": params}).to_string();
    let stdin = child.stdin.as_mut().expect("the binary reads requests");
    writeln!(stdin, "{line}").expect("the request is written");
    stdin.flush().expect("the request is sent");
    let stdout = child.stdout.take().expect("the binary answers");
    let answer = BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .find(|message| message["id"] == 1)
        .expect("an answer to the request");
    let _ = child.kill();
    let _ = child.wait();
    answer
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a clock after the epoch")
        .as_secs()
}

/// A home with two projects, `a` and `b`, and the records each test reads:
///
/// - `a` holds a session from now that `model-x` answered, with a split, and one from 1970 that
///   names no model and has no split.
/// - `b` holds a session from now whose model name carries an escape, and a `.json` file that is
///   not a record.
struct Fixture {
    root: PathBuf,
    home: PathBuf,
    a: PathBuf,
    b: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = scratch(name);
        let home = root.join("home");
        let a = root.join("a");
        let b = root.join("b");
        std::fs::create_dir_all(&a).expect("project a");
        std::fs::create_dir_all(&b).expect("project b");
        let fixture = Self { root, home, a, b };
        let recent = now();
        fixture.record(
            &fixture.a,
            "20260101T000000-aaaa",
            json!({"updated": recent, "turns": 2, "tokens": 100,
                   "split": {"read": 60, "written": 30}, "model": "model-x"}),
        );
        fixture.record(
            &fixture.a,
            "20260101T000000-bbbb",
            json!({"updated": 1, "turns": 1, "tokens": 50}),
        );
        fixture.record(
            &fixture.b,
            "20260101T000000-cccc",
            json!({"updated": recent, "turns": 3, "tokens": 400,
                   "split": {"read": 300, "written": 100}, "model": "\u{1b}]0;owned\u{7}model-y"}),
        );
        std::fs::write(fixture.store(&fixture.b).join("junk.json"), "not a record")
            .expect("a junk file");
        fixture
    }

    fn store(&self, project: &Path) -> PathBuf {
        let store = self.home.join(".bravebot/sessions").join(key_for(project));
        std::fs::create_dir_all(&store).expect("a store");
        store
    }

    fn record(&self, project: &Path, id: &str, figures: Value) {
        let mut record = json!({
            "id": id, "directory": project.display().to_string(), "title": id, "started": 1,
            "conversation": {"messages": [], "context": "trusted"},
        });
        record
            .as_object_mut()
            .expect("an object")
            .extend(figures.as_object().expect("an object").clone());
        std::fs::write(
            self.store(project).join(format!("{id}.json")),
            record.to_string(),
        )
        .expect("a record");
    }

    fn stats(&self, params: Value) -> Value {
        ask(&self.home, "session.stats", params)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The figures of sessions whose records carry no cache figures, as every record [`Fixture::new`]
/// writes.
fn figures(sessions: u64, turns: u64, tokens: u64, read: u64, written: u64) -> Value {
    json!({
        "sessions": sessions, "turns": turns, "tokens": tokens,
        "read": read, "written": written, "unsplit": tokens - read - written,
        "cacheRead": 0, "cacheWritten": 0, "hitRatePermille": null, "hitRate": "-",
    })
}

/// A row of sessions whose records name no service, as every record [`Fixture::new`] writes.
fn row(model: Value, spent: Value) -> Value {
    let mut row = spent;
    row["model"] = model;
    row["service"] = Value::Null;
    row["serviceWord"] = json!("unknown");
    row
}

/// With no parameters every project's records are totalled, a row for each model comes ordered by
/// tokens with `null` for the record that names none, and the junk file is counted and adds to no
/// figure.
#[test]
fn no_parameters_total_every_project_with_a_row_for_each_model() {
    let fixture = Fixture::new("bravebot-ui-bridge-stats-everything");
    let answer = fixture.stats(json!({}));
    let ok = &answer["ok"];
    assert_eq!(ok["directory"], Value::Null, "{answer}");
    assert_eq!(ok["days"], Value::Null, "{answer}");
    assert_eq!(ok["total"], figures(3, 6, 550, 360, 130), "{answer}");
    assert_eq!(ok["unreadable"], 1, "{answer}");
    let models = ok["models"].as_array().expect("model rows");
    assert_eq!(models.len(), 3, "{answer}");
    assert_eq!(models[0]["tokens"], 400, "the most tokens first: {answer}");
    assert_eq!(
        models[1],
        row(json!("model-x"), figures(1, 2, 100, 60, 30)),
        "{answer}"
    );
    assert_eq!(
        models[2],
        row(Value::Null, figures(1, 1, 50, 0, 0)),
        "a record that names no model is the null row: {answer}"
    );
}

/// One model answered by Bedrock and by a gateway is two rows, each naming its service by the
/// record's fixed word and by the word the terminal shows for it, and a word that is no backend is
/// the row for no service. The cache figures and the hit rate count only the records that carry a
/// cache block, a stated miss included, and the sessions without one are counted apart as the
/// command counts them. The hit rate and the note on those sessions arrive in the terminal's words.
///
/// Rejects an answer grouped by model alone, which would draw one row for two services, a row
/// named by whatever word the record holds, a hit rate taken over every session's reads, and a
/// shown word, rate or note that differs from what `bravebot stats` prints.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_model_answered_by_two_services_is_a_row_for_each_with_its_cache_figures() {
    let (fixture, c) = fixture_with_one_model_on_three_services();
    let answer = fixture.stats(json!({"directory": c.display().to_string()}));
    let ok = &answer["ok"];
    let rows: Vec<(&Value, &Value, &Value, &Value, &Value, &Value)> = ok["models"]
        .as_array()
        .expect("model rows")
        .iter()
        .map(|row| {
            (
                &row["model"],
                &row["service"],
                &row["tokens"],
                &row["cacheRead"],
                &row["cacheWritten"],
                &row["hitRatePermille"],
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                &json!("shared"),
                &json!("bedrock"),
                &json!(1100),
                &json!(600),
                &json!(200),
                &json!(600)
            ),
            (
                &json!("shared"),
                &json!("gateway"),
                &json!(900),
                &json!(0),
                &json!(0),
                &json!(0)
            ),
            (
                &json!("shared"),
                &Value::Null,
                &json!(30),
                &json!(5),
                &json!(0),
                &json!(500)
            ),
        ],
        "{answer}"
    );
    // 605 served of the 1510 read by the three records that carry a cache block.
    assert_eq!(ok["total"]["cacheRead"], 605, "{answer}");
    assert_eq!(ok["total"]["cacheWritten"], 200, "{answer}");
    assert_eq!(ok["total"]["hitRatePermille"], 400, "{answer}");
    assert_eq!(ok["total"]["hitRate"], "40.0%", "{answer}");
    let shown: Vec<(&Value, &Value)> = ok["models"]
        .as_array()
        .expect("model rows")
        .iter()
        .map(|row| (&row["serviceWord"], &row["hitRate"]))
        .collect();
    assert_eq!(
        shown,
        [
            (&json!("bedrock"), &json!("60.0%")),
            (&json!("gateway"), &json!("0.0%")),
            (&json!("unknown"), &json!("50.0%")),
        ],
        "{answer}"
    );
    assert_eq!(
        ok["withoutCache"],
        json!({"sessions": 2, "silent": 1, "older": 1,
               "note": "2 sessions carry no cache figures and are left out of the hit rate. \
                        On a service that states none: 1. Naming no service, so possibly \
                        older than the cache figures: 1."}),
        "{answer}"
    );
    assert!(!answer.to_string().contains('\u{1b}'), "{answer}");
}

/// A project `c`, in a fixture of its own, whose records are one model answered by Bedrock, by a
/// gateway that stated a miss, by a gateway that stated no cache figure, by a word that names no
/// backend and by a record that names no service.
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn fixture_with_one_model_on_three_services() -> (Fixture, PathBuf) {
    let fixture = Fixture::new("bravebot-ui-bridge-stats-services");
    let c = fixture.root.join("c");
    std::fs::create_dir_all(&c).expect("project c");
    let recent = now();
    // Bedrock: 1000 read, 600 of them out of the cache.
    fixture.record(
        &c,
        "20260101T000000-dddd",
        json!({"updated": recent, "turns": 1, "tokens": 1100, "model": "shared",
               "service": "bedrock",
               "split": {"read": 1000, "written": 100,
                         "cache": {"read_tokens": 600, "written_tokens": 200}}}),
    );
    // The gateway stated a miss on every request: in the hit rate, at zero.
    fixture.record(
        &c,
        "20260101T000000-eeee",
        json!({"updated": recent, "turns": 1, "tokens": 600, "model": "shared",
               "service": "gateway",
               "split": {"read": 500, "written": 100,
                         "cache": {"read_tokens": 0, "written_tokens": 0}}}),
    );
    // The gateway stated no cache figure: left out of the hit rate and counted as silent.
    fixture.record(
        &c,
        "20260101T000000-ffff",
        json!({"updated": recent, "turns": 1, "tokens": 300, "model": "shared",
               "service": "gateway", "split": {"read": 300, "written": 0}}),
    );
    // A word that names no backend, with a cache block: 5 of 10 out of the cache.
    fixture.record(
        &c,
        "20260101T000000-gggg",
        json!({"updated": recent, "turns": 1, "tokens": 10, "model": "shared",
               "service": "\u{1b}]0;owned\u{7}",
               "split": {"read": 10, "written": 0,
                         "cache": {"read_tokens": 5, "written_tokens": 0}}}),
    );
    // No service and no cache block: possibly older than the cache figures.
    fixture.record(
        &c,
        "20260101T000000-hhhh",
        json!({"updated": recent, "turns": 1, "tokens": 20, "model": "shared",
               "split": {"read": 20, "written": 0}}),
    );
    (fixture, c)
}

/// The escape the server's model name carried never reaches the answer, and what is left of the
/// name does.
#[test]
fn a_model_name_reaches_the_answer_cleaned_of_its_escape() {
    let fixture = Fixture::new("bravebot-ui-bridge-stats-escape");
    let answer = fixture.stats(json!({}));
    let text = answer.to_string();
    assert!(!text.contains('\u{1b}'), "{text}");
    assert!(!text.contains('\u{7}'), "{text}");
    let name = answer["ok"]["models"][0]["model"]
        .as_str()
        .expect("a named row");
    assert!(name.ends_with("model-y"), "{answer}");
    assert!(
        !name.chars().any(char::is_control),
        "no control character is left: {answer}"
    );
}

/// A directory keeps the records whose stored path is that directory, and is said back.
#[test]
fn a_directory_keeps_only_the_records_that_ran_in_it() {
    let fixture = Fixture::new("bravebot-ui-bridge-stats-directory");
    let answer = fixture.stats(json!({"directory": fixture.a.display().to_string()}));
    let ok = &answer["ok"];
    assert_eq!(ok["directory"], fixture.a.display().to_string(), "{answer}");
    assert_eq!(ok["total"], figures(2, 3, 150, 60, 30), "{answer}");
    assert_eq!(
        ok["unreadable"], 0,
        "the other project's junk is not read: {answer}"
    );
    let models: Vec<&Value> = ok["models"]
        .as_array()
        .expect("model rows")
        .iter()
        .map(|row| &row["model"])
        .collect();
    assert_eq!(models, [&json!("model-x"), &Value::Null], "{answer}");
}

/// `days` leaves out a record last written before the cut.
#[test]
fn days_leave_out_a_record_last_written_before_the_cut() {
    let fixture = Fixture::new("bravebot-ui-bridge-stats-days");
    let answer = fixture.stats(json!({"days": 1}));
    let ok = &answer["ok"];
    assert_eq!(ok["days"], 1, "{answer}");
    assert_eq!(ok["total"], figures(2, 5, 500, 360, 130), "{answer}");
    assert!(
        ok["models"]
            .as_array()
            .expect("model rows")
            .iter()
            .all(|row| !row["model"].is_null()),
        "the 1970 record's row is gone: {answer}"
    );

    let both = fixture.stats(json!({"days": 1, "directory": fixture.a.display().to_string()}));
    assert_eq!(both["ok"]["total"], figures(1, 2, 100, 60, 30), "{both}");
}

/// A count of zero, a negative count and one that is not a whole number are refused, as `--days`
/// refuses them, and so is a directory that is not one.
#[test]
fn a_count_that_is_no_number_of_days_and_a_path_that_is_no_directory_are_refused() {
    let fixture = Fixture::new("bravebot-ui-bridge-stats-refusals");
    for days in [json!(0), json!(-1), json!("x"), json!(1.5), json!(u64::MAX)] {
        let answer = fixture.stats(json!({"days": days}));
        assert_eq!(answer["error"]["code"], "bad_request", "{days}: {answer}");
        assert!(answer.get("ok").is_none(), "{answer}");
    }

    let file = fixture.root.join("a-file");
    std::fs::write(&file, "").expect("a file");
    for directory in [
        file.display().to_string(),
        fixture.root.join("missing").display().to_string(),
    ] {
        let answer = fixture.stats(json!({"directory": directory}));
        assert_eq!(
            answer["error"]["code"], "not_a_directory",
            "{directory}: {answer}"
        );
    }
}
