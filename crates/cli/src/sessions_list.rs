//! `bravebot sessions list [--json] [--limit <n>]`: the sessions recorded for this directory
//! (CLI-32).
//!
//! Reads the records under this directory's store and prints a line each: the id, when it was
//! last written, the title, the branch, the front end that wrote it (SESSION-29) and whether it
//! can be carried on. Nothing is written, and no record is opened for its conversation.
//!
//! Every field printed is one the program itself wrote down or one the person typed (SESSION-2),
//! and the title is cleaned of anything a terminal would act on before it reaches a row.

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use bravebot_session::search;
use bravebot_session::sessions::{self, Summary};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

/// How many rows are printed when nobody said.
const ROWS: usize = 20;

/// The longest title printed, in characters.
const LONGEST_TITLE: usize = 80;

/// What was asked for, once the words are read.
#[derive(Debug, PartialEq, Eq)]
struct Request {
    as_json: bool,
    limit: usize,
}

/// `bravebot sessions list ...`, after `list`.
pub(crate) fn command(args: &[String]) -> ExitCode {
    let Some(request) = read(args) else {
        return fail(Ending::Argument, t!(sessions_list_usage));
    };
    let project = match crate::background::project_directory(None) {
        Ok(path) => path,
        Err(code) => return code,
    };

    let listed = sessions::list(&project);
    let here = here(&listed, &project);
    let shown: Vec<&Summary> = here.iter().copied().take(request.limit).collect();

    let mut out = std::io::stdout().lock();
    if request.as_json {
        for session in &shown {
            let _ = writeln!(out, "{}", as_json(session));
        }
    } else if shown.is_empty() {
        let _ = writeln!(out, "{}", t!(sessions_list_none));
    } else {
        for session in &shown {
            let _ = writeln!(out, "{}", row(session));
        }
    }

    // The hint goes to stderr, so stdout carries the listing and nothing else (CLI-5).
    if here.len() > shown.len() {
        eprintln!(
            "{}",
            t!(sessions_list_more, held = here.len() - shown.len())
        );
    }
    ExitCode::SUCCESS
}

/// `--json` and `--limit <n>`, in either order. A limit of zero, a limit that is not a number, a
/// repeated flag and any other word are refused rather than guessed at.
fn read(words: &[String]) -> Option<Request> {
    let mut request = Request {
        as_json: false,
        limit: ROWS,
    };
    let mut limit_given = false;
    let mut rest = words.iter();
    while let Some(word) = rest.next() {
        match word.as_str() {
            "--json" if !request.as_json => request.as_json = true,
            "--limit" if !limit_given => {
                let count: usize = rest.next()?.parse().ok()?;
                request.limit = (count > 0).then_some(count)?;
                limit_given = true;
            }
            _ => return None,
        }
    }
    Some(request)
}

/// The sessions whose record says it ran in `project`, newest first.
///
/// The store is found under one path segment standing for the working directory
/// ([`sessions::key_for`]), and that segment is not reversible: two directories can share a
/// store. The path the record kept is what says which of them a session belongs to
/// (SESSION-28), so the listing filters on that and never on the name of the directory holding
/// the records.
fn here<'a>(listed: &'a [Summary], project: &Path) -> Vec<&'a Summary> {
    listed
        .iter()
        .filter(|session| Path::new(&session.directory) == project)
        .collect()
}

/// One row: the id, how long ago it was written, the title, the branch, the front end and whether
/// it can be continued.
fn row(session: &Summary) -> String {
    t!(
        sessions_list_row,
        id = session.id.as_str(),
        when = sessions::how_long_ago(session.updated),
        title = search::clean(&session.title, LONGEST_TITLE),
        branch = named(session.branch.as_deref()),
        front = named(session.front.as_deref()),
        state = if session.manifest {
            t!(sessions_list_not_continuable)
        } else {
            t!(sessions_list_continuable)
        }
    )
    .to_string()
}

/// A field a record written before it was kept has nothing for, and a detached head has no branch
/// name. Cleaned like the title: a record somebody edited by hand must not put an escape on a row.
fn named(value: Option<&str>) -> String {
    match value {
        Some(text) => search::clean(text, LONGEST_TITLE),
        None => t!(sessions_list_unknown).to_string(),
    }
}

/// One session as one line of JSON.
///
/// Every field is written on every line, including the ones a record from before they were kept
/// has nothing for, so a caller needs no presence check to read one.
fn as_json(session: &Summary) -> String {
    crate::json::object(&[
        ("id", crate::json::quoted(&session.id)),
        ("title", crate::json::quoted(&session.title)),
        ("directory", crate::json::quoted(&session.directory)),
        ("updated", session.updated.to_string()),
        ("branch", crate::json::maybe(session.branch.as_deref())),
        ("front", crate::json::maybe(session.front.as_deref())),
        ("continuable", (!session.manifest).to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    fn session(id: &str, directory: &str) -> Summary {
        Summary {
            id: id.to_string(),
            title: format!("work in {directory}"),
            directory: directory.to_string(),
            front: Some("terminal".to_string()),
            branch: Some("main".to_string()),
            issue: None,
            pull_request: None,
            handed_off_from: None,
            updated: 1_700_000_000,
            bytes: 0,
            manifest: false,
        }
    }

    #[test]
    fn the_words_say_how_many_rows_and_whether_they_are_json() {
        assert_eq!(
            read(&words("--json --limit 5")),
            Some(Request {
                as_json: true,
                limit: 5
            })
        );
        assert_eq!(
            read(&[]),
            Some(Request {
                as_json: false,
                limit: ROWS
            })
        );
    }

    #[test]
    fn a_muddled_request_is_refused_rather_than_guessed_at() {
        for muddled in [
            "--limit",
            "--limit 0",
            "--limit two",
            "--limit -1",
            "--limit 5 --limit 6",
            "--json --json",
            "--all",
            "list",
        ] {
            assert_eq!(read(&words(muddled)), None, "{muddled}");
        }
    }

    /// The defect this exists for: the directory the records sit under is one path segment that
    /// cannot be turned back into a path, so two working directories can share it. A listing that
    /// trusted the segment would show somebody another directory's sessions.
    #[test]
    fn a_session_recorded_in_another_directory_sharing_the_store_is_not_listed() {
        let project = PathBuf::from("/w/one");
        let listed = [
            session("here", "/w/one"),
            session("elsewhere", "/w/two"),
            session("also-here", "/w/one"),
        ];

        assert_eq!(
            here(&listed, &project)
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["here", "also-here"]
        );
    }

    /// `directory` is a `String`, so a directory whose name is not UTF-8 can only be recorded as
    /// the lossy rendering `Path::display` gives it, the same text an ordinary, validly-named
    /// directory could be called. Comparing on that rendering is still the strongest comparison
    /// available on top of it, and this is its edge: the exact bytes of a directory that is not
    /// UTF-8 match nothing, since no `String` holds them, and the valid name its rendering spells
    /// matches whatever was recorded under either one. `/w/one` and `/w/two` above pass under a
    /// byte comparison and a lossy one alike, so they could not have caught `here` doing the lossy
    /// one; a name that is not UTF-8 beside the name its own rendering spells can.
    #[test]
    #[cfg(unix)]
    fn a_directory_that_is_not_utf8_is_told_from_an_ordinary_one_only_up_to_its_rendering() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let not_utf8 = PathBuf::from(OsStr::from_bytes(b"/w/\xff"));
        let rendering = not_utf8.to_string_lossy().into_owned();
        assert_ne!(
            rendering.as_bytes(),
            not_utf8.as_os_str().as_bytes(),
            "the fixture is not actually lossy"
        );
        let spelled = PathBuf::from(&rendering);
        let listed = [session("recorded-as-the-rendering", &rendering)];

        assert!(
            here(&listed, &not_utf8).is_empty(),
            "a directory that is not UTF-8 was matched by its own exact bytes, which nothing \
             recorded under it can hold"
        );
        assert_eq!(
            here(&listed, &spelled)
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["recorded-as-the-rendering"],
            "the ordinary, validly-named directory the rendering spells was not matched by its \
             own exact name"
        );
    }

    /// A manifest run is a session in every other respect, so it is listed. Saying it cannot be
    /// continued on the row is what stops somebody choosing it and finding out afterwards.
    #[test]
    fn a_manifest_run_is_listed_and_says_it_cannot_be_continued() {
        let run = Summary {
            manifest: true,
            ..session("abc", "/w/one")
        };
        let line = row(&run);

        assert!(line.contains("abc"), "{line}");
        assert!(line.contains(t!(sessions_list_not_continuable)), "{line}");
        assert!(!line.contains(t!(sessions_list_continuable)), "{line}");
        assert!(
            row(&session("abc", "/w/one")).contains(t!(sessions_list_continuable)),
            "a turn session says it can be"
        );
    }

    #[test]
    fn a_row_says_the_front_end_that_wrote_it_and_the_branch_it_was_on() {
        let line = row(&session("abc", "/w/one"));
        assert!(line.contains("terminal"), "{line}");
        assert!(line.contains("main"), "{line}");

        let older = Summary {
            front: None,
            branch: None,
            ..session("abc", "/w/one")
        };
        assert_eq!(
            row(&older)
                .matches(&t!(sessions_list_unknown).to_string())
                .count(),
            2,
            "a record from before either was kept says so rather than inventing one"
        );
    }

    #[test]
    fn a_title_reaches_a_row_with_nothing_a_terminal_would_act_on() {
        let awkward = Summary {
            title: "clear\u{1b}[2J\nthe screen".to_string(),
            ..session("abc", "/w/one")
        };
        assert!(row(&awkward).contains("clear [2J the screen"));
    }

    /// `--json` is read by a program, so stdout holds one object per line and nothing else: no
    /// header, no count, no hint. The hint about further sessions is on stderr for that reason.
    #[test]
    fn json_is_one_object_per_line_and_nothing_else() {
        let listed = [session("one", "/w/one"), session("two", "/w/one")];
        let lines: Vec<String> = listed.iter().map(as_json).collect();

        assert_eq!(lines.len(), 2);
        for line in &lines {
            assert!(line.starts_with('{') && line.ends_with('}'), "{line}");
            assert!(!line.contains('\n'), "{line}");
            let read: serde_json::Value = serde_json::from_str(line).expect("one object");
            assert!(read.get("id").is_some(), "{line}");
            assert_eq!(read["continuable"], serde_json::json!(true));
            assert_eq!(read["directory"], serde_json::json!("/w/one"));
            assert_eq!(read["front"], serde_json::json!("terminal"));
            assert_eq!(read["updated"], serde_json::json!(1_700_000_000u64));
        }
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&as_json(&Summary {
                manifest: true,
                front: None,
                branch: None,
                ..session("run", "/w/one")
            }))
            .expect("one object")["continuable"],
            serde_json::json!(false)
        );
    }

    /// A title is the prompt a person typed, and a quote in it would end the string it is written
    /// in and leave a caller reading fields this command did not mean.
    #[test]
    fn a_typed_title_cannot_break_out_of_the_object_it_is_written_in() {
        let awkward = Summary {
            title: r#""},{"continuable":true,"x":""#.to_string(),
            ..session("abc", "/w/one")
        };
        let line = as_json(&awkward);

        assert_eq!(line.matches(r#""continuable":"#).count(), 1, "{line}");
        let read: serde_json::Value = serde_json::from_str(&line).expect("one object");
        assert_eq!(read["title"], serde_json::json!(awkward.title));
    }
}
