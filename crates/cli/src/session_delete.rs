//! `bravebot sessions delete <id>` and `bravebot sessions purge [--dry-run] [-y]`: remove what a
//! session kept (CLI-33).
//!
//! Both are keyed on ids, which the program wrote, and on the directory the person is in. Nothing
//! read out of a record decides what is deleted, and nothing is printed from a record but its id,
//! its age and its title, cleaned of what a terminal would act on.

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use bravebot_session::sessions::{self, Deletion, Purge};
use bravebot_session::{jobs, search};
use std::io::{BufRead, IsTerminal, Write};
use std::process::ExitCode;

/// The longest title a dry run prints, in characters.
const LONGEST_TITLE: usize = 80;

/// What was asked for, once the words are read.
#[derive(Debug, PartialEq, Eq)]
enum Request {
    Delete(String),
    Purge { dry_run: bool, yes: bool },
}

/// `bravebot sessions delete ...`, after `delete`.
pub(crate) fn delete(args: &[String]) -> ExitCode {
    let Some(Request::Delete(id)) = read("delete", args) else {
        return fail(Ending::Argument, t!(sessions_delete_usage));
    };
    let project = match crate::background::project_directory(None) {
        Ok(path) => path,
        Err(code) => return code,
    };
    if !sessions::is_a_session_name(&id) {
        return fail(Ending::Argument, t!(sessions_delete_invalid, id = id));
    }
    if jobs::is_running(&id) {
        return fail(Ending::Failed, t!(sessions_delete_held, id = id));
    }
    match sessions::delete(&project, &id) {
        Ok(()) => {
            println!("{}", t!(sessions_delete_done, id = id));
            ExitCode::SUCCESS
        }
        Err(Deletion::Invalid) => fail(Ending::Argument, t!(sessions_delete_invalid, id = id)),
        Err(Deletion::NotFound) => fail(Ending::Argument, t!(sessions_delete_missing, id = id)),
        Err(Deletion::Failed(_)) => fail(Ending::Failed, t!(sessions_delete_failed, id = id)),
    }
}

/// `bravebot sessions purge ...`, after `purge`.
pub(crate) fn purge(args: &[String]) -> ExitCode {
    let Some(Request::Purge { dry_run, yes }) = read("purge", args) else {
        return fail(Ending::Argument, t!(sessions_delete_usage));
    };
    let project = match crate::background::project_directory(None) {
        Ok(path) => path,
        Err(code) => return code,
    };
    let plan = sessions::purge_plan(&project, jobs::is_running);
    if plan.delete.is_empty() && plan.held.is_empty() {
        println!("{}", t!(sessions_purge_none));
        return ExitCode::SUCCESS;
    }
    let mut out = std::io::stdout().lock();
    if dry_run {
        let listed = sessions::list(&project);
        for id in &plan.delete {
            if let Some(session) = listed.iter().find(|session| &session.id == id) {
                let _ = writeln!(
                    out,
                    "{}",
                    t!(
                        sessions_purge_row,
                        id = session.id.as_str(),
                        when = sessions::how_long_ago(session.updated),
                        title = search::clean(&session.title, LONGEST_TITLE)
                    )
                );
            }
        }
        say_held(&plan, &mut out);
        return ExitCode::SUCCESS;
    }
    if !plan.delete.is_empty() && !yes {
        if !std::io::stdin().is_terminal() {
            return fail(Ending::Argument, t!(sessions_purge_needs_yes));
        }
        // On standard error, so a redirected listing never swallows the question.
        let mut asking = std::io::stderr().lock();
        let _ = write!(
            asking,
            "{} {} ",
            t!(
                sessions_purge_question,
                directory = project.display(),
                count = plan.delete.len()
            ),
            t!(line_answer)
        );
        let _ = asking.flush();
        drop(asking);
        if !agreed(&mut std::io::stdin().lock()) {
            let _ = writeln!(out, "{}", t!(sessions_purge_declined));
            return ExitCode::SUCCESS;
        }
    }
    let failed = sessions::purge(&project, &plan);
    let _ = writeln!(
        out,
        "{}",
        t!(
            sessions_purge_done,
            count = plan.delete.len() - failed.len()
        )
    );
    say_held(&plan, &mut out);
    for id in &failed {
        eprintln!("{}", t!(sessions_delete_failed, id = id.as_str()));
    }
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(Ending::Failed.status())
    }
}

/// One line for each record a purge left because a background session holds it.
fn say_held(plan: &Purge, out: &mut impl Write) {
    for id in &plan.held {
        let _ = writeln!(out, "{}", t!(sessions_purge_held, id = id.as_str()));
    }
}

/// Whether the person's one line is the word for yes. End of input and anything else is no.
fn agreed(from: &mut impl BufRead) -> bool {
    let mut typed = String::new();
    match from.read_line(&mut typed) {
        Ok(read) if read > 0 => typed.trim().to_lowercase() == t!(line_answer_yes),
        _ => false,
    }
}

/// The words after `delete` or `purge`. A missing id, a second word, a repeated flag and any word
/// that is not one of the two flags are refused rather than guessed at.
fn read(verb: &str, words: &[String]) -> Option<Request> {
    match verb {
        "delete" => match words {
            [id] if !id.starts_with('-') => Some(Request::Delete(id.clone())),
            _ => None,
        },
        _ => {
            let (mut dry_run, mut yes) = (false, false);
            for word in words {
                match word.as_str() {
                    "--dry-run" if !dry_run => dry_run = true,
                    "-y" if !yes => yes = true,
                    _ => return None,
                }
            }
            Some(Request::Purge { dry_run, yes })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn delete_takes_one_id_and_purge_takes_its_two_flags() {
        assert_eq!(
            read("delete", &words("abc-1")),
            Some(Request::Delete("abc-1".to_string()))
        );
        assert_eq!(
            read("purge", &[]),
            Some(Request::Purge {
                dry_run: false,
                yes: false
            })
        );
        assert_eq!(
            read("purge", &words("-y --dry-run")),
            Some(Request::Purge {
                dry_run: true,
                yes: true
            })
        );
    }

    #[test]
    fn a_muddled_request_is_refused_rather_than_guessed_at() {
        for muddled in ["", "a b", "--all", "-y"] {
            assert_eq!(read("delete", &words(muddled)), None, "{muddled:?}");
        }
        for muddled in ["-y -y", "--dry-run --dry-run", "--all", "abc", "-y abc"] {
            assert_eq!(read("purge", &words(muddled)), None, "{muddled}");
        }
    }

    /// Only the word for yes deletes. A bare return, end of input and a near miss all leave the
    /// sessions where they are, since the question is about something that cannot be undone.
    #[test]
    fn only_the_word_for_yes_agrees_to_a_purge() {
        let yes = t!(line_answer_yes).to_string();
        assert!(agreed(&mut format!("{yes}\n").as_bytes()));
        assert!(agreed(
            &mut format!(" {} \n", yes.to_uppercase()).as_bytes()
        ));
        assert!(!agreed(&mut "\n".as_bytes()));
        assert!(!agreed(&mut "".as_bytes()));
        assert!(!agreed(&mut format!("{yes}es please\n").as_bytes()));
        assert!(!agreed(&mut "n\n".as_bytes()));
    }
}
