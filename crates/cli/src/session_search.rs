//! `bravebot sessions search [workspace:<dir>] [since:<n>h|d|w] <text>`: find past sessions by
//! what was said in them (SESSION-33).
//!
//! Prints a session's id and title on a line each, newest first, and nothing else. Exits 1 when
//! nothing matches, as `grep` does, so a script can branch on it.

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use bravebot_session::search::{self, Corpus, Query};
use bravebot_session::sessions::{self, Summary};
use std::path::PathBuf;
use std::process::ExitCode;

/// The longest title printed, in characters.
const LONGEST_TITLE: usize = 120;

/// What was asked for, once the words are read.
#[derive(Debug, PartialEq, Eq)]
struct Request {
    workspace: Option<PathBuf>,
    query: Query,
}

/// `bravebot sessions search ...`, after `search`.
pub(crate) fn command(args: &[String]) -> ExitCode {
    let Some(request) = read(args) else {
        return fail(Ending::Argument, t!(sessions_search_usage));
    };
    let directory = match request.workspace {
        Some(path) => path,
        None => match std::env::current_dir() {
            Ok(here) => here,
            Err(_) => return fail(Ending::Failed, t!(cli_directory_unknown)),
        },
    };
    let Ok(project) = directory.canonicalize().and_then(|resolved| {
        resolved
            .is_dir()
            .then_some(resolved)
            .ok_or(std::io::ErrorKind::NotADirectory.into())
    }) else {
        return fail(
            Ending::Argument,
            t!(sessions_import_no_project, path = directory.display()),
        );
    };

    let listed = sessions::list(&project);
    let corpus = if request.query.has_phrase() {
        // A session outside the window, or one whose title already holds the phrase, is not read.
        let now = sessions_now();
        let wanted: std::collections::HashSet<&str> = listed
            .iter()
            .filter(|session| request.query.admits(session.updated, now))
            .filter(|session| {
                !session
                    .title
                    .to_lowercase()
                    .contains(request.query.phrase())
            })
            .map(|session| session.id.as_str())
            .collect();
        Corpus::read(&project, |id| wanted.contains(id))
    } else {
        Corpus::default()
    };
    let hits = found(&listed, &corpus, &request.query, sessions_now());
    if hits.is_empty() {
        return fail(Ending::Failed, t!(sessions_search_none));
    }
    for session in hits {
        println!("{}", row(session));
    }
    ExitCode::SUCCESS
}

/// Every argument that is not a `workspace:` word is text for the query, so a phrase may be quoted
/// whole or left as separate words. After `--` every argument is text, so a word may begin with `-`.
fn read(words: &[String]) -> Option<Request> {
    let mut workspace = None;
    let mut rest = Vec::new();
    let mut literal = false;
    for word in words {
        if literal {
            rest.push(word.as_str());
            continue;
        }
        match word.strip_prefix("workspace:") {
            Some("") => return None,
            Some(_) if workspace.is_some() => return None,
            Some(path) => workspace = Some(PathBuf::from(path)),
            None if word == "--" => literal = true,
            None if word.starts_with('-') => return None,
            None => rest.push(word.as_str()),
        }
    }
    let query = Query::parse(&rest.join(" ")).ok()?;
    if !query.has_phrase() && !query.has_window() {
        return None;
    }
    Some(Request { workspace, query })
}

/// The sessions, newest first, whose title or words hold the phrase and that are recent enough.
fn found<'a>(listed: &'a [Summary], corpus: &Corpus, query: &Query, now: u64) -> Vec<&'a Summary> {
    listed
        .iter()
        .filter(|session| query.admits(session.updated, now))
        .filter(|session| {
            !query.has_phrase()
                || session.title.to_lowercase().contains(query.phrase())
                || corpus.found(&session.id, query).is_some()
        })
        .collect()
}

fn row(session: &Summary) -> String {
    format!(
        "{}  {}",
        session.id,
        search::clean(&session.title, LONGEST_TITLE)
    )
}

fn sessions_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    fn session(id: &str, title: &str, updated: u64) -> Summary {
        Summary {
            id: id.to_string(),
            title: title.to_string(),
            branch: None,
            issue: None,
            pull_request: None,
            updated,
            bytes: 0,
            manifest: false,
        }
    }

    #[test]
    fn the_words_say_where_to_look_how_far_back_and_for_what() {
        let request = read(&words("workspace:/w since:3d fix the build")).expect("a request");
        assert_eq!(request.workspace, Some(PathBuf::from("/w")));
        assert_eq!(request.query.phrase(), "fix the build");
        assert!(request.query.admits(100, 100 + 3 * 86_400));
        assert!(!request.query.admits(100, 101 + 3 * 86_400));
    }

    #[test]
    fn a_quoted_phrase_may_carry_the_since_word() {
        let request = read(&["fix the build since:1w".to_string()]).expect("a request");
        assert_eq!(request.query.phrase(), "fix the build");
    }

    #[test]
    fn a_muddled_request_is_refused_rather_than_guessed_at() {
        for muddled in [
            "",
            "workspace:/w",
            "workspace: text",
            "workspace:/a workspace:/b text",
            "since:3m text",
            "--all text",
        ] {
            assert_eq!(read(&words(muddled)), None, "{muddled}");
        }
    }

    #[test]
    fn words_after_a_double_dash_are_text_even_if_they_begin_with_a_dash() {
        let request = read(&words("workspace:/w -- -v --all")).expect("a request");
        assert_eq!(request.query.phrase(), "-v --all");
        assert_eq!(request.workspace, Some(PathBuf::from("/w")));
        assert_eq!(read(&words("--")), None);
    }

    #[test]
    fn a_window_alone_lists_the_recent_sessions() {
        let request = read(&words("since:1d")).expect("a request");
        let now = 1_000_000;
        let listed = [
            session("a", "recent", now),
            session("b", "old", now - 86_401),
        ];
        let hits = found(&listed, &Corpus::default(), &request.query, now);
        assert_eq!(
            hits.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["a"]
        );
    }

    #[test]
    fn a_title_or_a_line_said_matches_and_a_session_with_neither_does_not() {
        let listed = [
            session("title", "Rotate the Keys", 10),
            session("said", "unrelated", 9),
            session("neither", "unrelated", 8),
        ];
        let corpus = Corpus::of([
            ("said".to_string(), vec!["we should rotate it".to_string()]),
            ("neither".to_string(), vec!["nothing here".to_string()]),
        ]);
        let query = Query::parse("rotate").expect("a query");
        let hits = found(&listed, &corpus, &query, 10);
        assert_eq!(
            hits.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["title", "said"]
        );
    }

    #[test]
    fn a_row_is_the_id_and_a_title_with_nothing_a_terminal_would_act_on() {
        let line = row(&session("abc", "clear\u{1b}[2J\nthe screen", 0));
        assert_eq!(line, "abc  clear [2J the screen");
    }
}
