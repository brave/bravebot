//! `bravebot sessions import <claude-code|opencode>`: copy sessions another coding agent kept
//! (SESSION-32, IMPORT-11).
//!
//! Run by a person and never by a session. It lists what the other program kept for a workspace,
//! and copies what is named; nothing is copied for a bare listing.

use crate::exit::{Ending, fail};
use crate::progress::{printable, printable_path};
use bravebot_i18n::t;
use bravebot_session::import::{self, Copied, Failed, Found, Pick};
use std::path::PathBuf;
use std::process::ExitCode;

const OPENCODE: &str = "opencode";

/// What was asked for, once the words are read.
#[derive(Debug, PartialEq, Eq)]
struct Request {
    project: Option<PathBuf>,
    selection: Selection,
}

#[derive(Debug, PartialEq, Eq)]
enum Selection {
    /// Nothing named: list what is there.
    Listing,
    Every,
    Named(Vec<String>),
}

/// `bravebot sessions import <tool> [--project <dir>] [--all | <id>...]`, after `import`.
pub(crate) fn command(args: &[String]) -> ExitCode {
    let Some((tool, rest)) = args.split_first() else {
        return fail(Ending::Argument, t!(sessions_import_usage));
    };
    let Some(request) = read(rest) else {
        return fail(Ending::Argument, t!(sessions_import_usage));
    };
    match tool.as_str() {
        import::CLAUDE_CODE => claude_code(request),
        // Its sessions are rows in a database file, and nothing here reads one.
        OPENCODE => fail(Ending::Failed, t!(sessions_import_opencode)),
        _ => fail(Ending::Argument, t!(sessions_import_usage)),
    }
}

fn read(words: &[String]) -> Option<Request> {
    let mut project = None;
    let mut every = false;
    let mut named = Vec::new();
    let mut words = words.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--project" if project.is_none() => project = Some(PathBuf::from(words.next()?)),
            "--all" => every = true,
            flag if flag.starts_with('-') => return None,
            id => named.push(id.to_string()),
        }
    }
    let selection = match (every, named.is_empty()) {
        (true, true) => Selection::Every,
        (false, true) => Selection::Listing,
        (false, false) => Selection::Named(named),
        (true, false) => return None,
    };
    Some(Request { project, selection })
}

fn claude_code(request: Request) -> ExitCode {
    let project = match crate::background::project_directory(request.project) {
        Ok(path) => path,
        Err(code) => return code,
    };
    let Some(claude_dir) = bravebot_config::import::Places::from_env()
        .claude_code
        .and_then(|settings| settings.parent().map(std::path::Path::to_path_buf))
    else {
        return fail(Ending::Failed, t!(sessions_import_no_source));
    };

    let found = import::claude_code(&claude_dir, &project);
    let chosen: Vec<&Found> = match &request.selection {
        Selection::Listing => {
            list(&found, &project);
            return ExitCode::SUCCESS;
        }
        Selection::Every => found.iter().collect(),
        Selection::Named(typed) => {
            let mut picked = Vec::new();
            for name in typed {
                match import::pick(&found, name) {
                    Ok(one) => picked.push(one),
                    Err(Pick::Missing) => {
                        return fail(Ending::Argument, missing(name));
                    }
                    Err(Pick::Ambiguous) => {
                        return fail(Ending::Argument, ambiguous(name));
                    }
                }
            }
            picked
        }
    };

    let mut failed = false;
    for one in chosen {
        match import::copy(&project, one) {
            Ok(Copied::Written) => println!("{}", copied(&one.title, &one.id)),
            Ok(Copied::Already) => println!("{}", there(&one.title)),
            Err(Failed::NoStateDirectory) => {
                return fail(Ending::Failed, t!(sessions_no_home));
            }
            Err(Failed::Write(err)) => {
                failed = true;
                eprintln!(
                    "{}",
                    t!(
                        sessions_import_failed,
                        title = printable(&one.title),
                        problem = err
                    )
                );
            }
        }
    }
    if failed {
        ExitCode::from(Ending::Failed.status())
    } else {
        ExitCode::SUCCESS
    }
}

fn list(found: &[Found], project: &std::path::Path) {
    if found.is_empty() {
        println!(
            "{}",
            t!(sessions_import_none, directory = printable_path(project))
        );
        return;
    }
    for one in found {
        let id: String = one.source_id.chars().take(8).collect();
        let when = bravebot_session::sessions::how_long_ago(one.updated);
        println!("{}", row(one.imported, &id, &when, &one.title));
    }
    println!("{}", t!(sessions_import_how));
}

/// The line for a session no copy of which is here yet, or that is already copied. The id and the
/// title are text another program wrote.
fn row(imported: bool, id: &str, when: &str, title: &str) -> String {
    if imported {
        t!(
            sessions_import_row_there,
            id = printable(id),
            when = when,
            title = printable(title)
        )
    } else {
        t!(
            sessions_import_row,
            id = printable(id),
            when = when,
            title = printable(title)
        )
    }
}

/// The line after a session was copied.
fn copied(title: &str, id: &str) -> String {
    t!(
        sessions_import_copied,
        title = printable(title),
        id = printable(id)
    )
}

/// The line for a session that was already here.
fn there(title: &str) -> String {
    t!(sessions_import_there, title = printable(title))
}

/// The refusal for a typed id that begins no session.
fn missing(typed: &str) -> String {
    t!(sessions_import_missing, id = printable(typed))
}

/// The refusal for a typed id that begins more than one session.
fn ambiguous(typed: &str) -> String {
    t!(sessions_import_ambiguous, id = printable(typed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn the_words_after_the_tool_say_what_to_copy() {
        assert_eq!(
            read(&words("")),
            Some(Request {
                project: None,
                selection: Selection::Listing
            })
        );
        assert_eq!(
            read(&words("--all --project /w")),
            Some(Request {
                project: Some(PathBuf::from("/w")),
                selection: Selection::Every
            })
        );
        assert_eq!(
            read(&words("ab12 --project /w cd34")),
            Some(Request {
                project: Some(PathBuf::from("/w")),
                selection: Selection::Named(words("ab12 cd34"))
            })
        );
    }

    /// LAYER-3: a session's title and id are text another program wrote, so every line that
    /// prints one draws control characters and bidirectional overrides as pictures.
    #[test]
    fn the_title_and_id_of_a_session_another_program_wrote_are_drawn_as_pictures() {
        let title = "fix\u{1b}[2J\u{202e}it";
        let id = "ab\u{1b}[31m\u{202e}cd";
        let title_pictured = "fix\u{241b}[2J\u{2039}U+202E\u{203A}it";
        let id_pictured = "ab\u{241b}[31m\u{2039}U+202E\u{203A}cd";

        let lines = [
            ("a row", row(false, id, "3m ago", title), true, true),
            (
                "a row already copied",
                row(true, id, "3m ago", title),
                true,
                true,
            ),
            ("a copy", copied(title, id), true, true),
            ("a session already here", there(title), true, false),
            ("a missing id", missing(id), false, true),
            ("an ambiguous id", ambiguous(id), false, true),
        ];
        for (what, line, names_title, names_id) in lines {
            assert!(
                !line.contains(['\u{1b}', '\u{202e}']),
                "{what} printed a raw character: {line:?}"
            );
            assert_eq!(
                line.contains(title_pictured),
                names_title,
                "{what} did not draw the title: {line:?}"
            );
            assert_eq!(
                line.contains(id_pictured),
                names_id,
                "{what} did not draw the id: {line:?}"
            );
        }
    }

    #[test]
    fn a_muddled_request_is_refused_rather_than_guessed_at() {
        for muddled in [
            "--all ab12",
            "--project",
            "--project /a --project /b",
            "--everything",
            "-x",
        ] {
            assert_eq!(read(&words(muddled)), None, "{muddled}");
        }
    }
}
