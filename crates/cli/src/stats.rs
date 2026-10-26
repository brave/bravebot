//! `bravebot stats [--days <n>] [--models] [--project [<dir>]]`: what the recorded sessions spent
//! (CLI-37).
//!
//! Totals the turns and tokens of the session records under the state directory, for every project
//! or for one, and for the last `n` days or all of them. Nothing is written and no record is opened
//! for its conversation.
//!
//! Every figure is a number the program wrote down. The one word that is not is the model's name,
//! which the server reported, so it is cleaned of anything a terminal would act on before it reaches
//! a row (VIEW-4).

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use bravebot_session::search;
use bravebot_session::sessions;
use bravebot_session::stats::{Scope, Spent, Totals};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The longest model name printed, in characters.
const LONGEST_MODEL: usize = 60;

const SECONDS_IN_A_DAY: u64 = 86_400;

/// The most days a count may name. The sentence says the number back through a message that counts
/// in `i64`, and a larger one would be printed as another number than the one the cut used.
const MOST_DAYS: u64 = i64::MAX as u64;

/// What was asked for, once the words are read.
#[derive(Debug, Default, PartialEq, Eq)]
struct Request {
    days: Option<u64>,
    models: bool,
    /// `None` is every project, `Some(None)` is this directory, `Some(Some(dir))` is `dir`.
    project: Option<Option<PathBuf>>,
}

/// `bravebot stats ...`, after `stats`.
pub(crate) fn command(args: &[String]) -> ExitCode {
    let Some(request) = read(args) else {
        return fail(Ending::Argument, t!(stats_usage));
    };
    let project = match &request.project {
        Some(named) => match crate::background::project_directory(named.clone()) {
            Ok(path) => Some(path),
            Err(code) => return code,
        },
        None => None,
    };
    let since = request
        .days
        .map(|days| sessions::now().saturating_sub(days.saturating_mul(SECONDS_IN_A_DAY)));

    let totals = bravebot_session::stats::totals(Scope {
        project: project.as_deref(),
        since,
    });

    let _ = write!(
        std::io::stdout().lock(),
        "{}",
        report(&request, project.as_deref(), &totals)
    );

    // Beside the figures rather than among them, so stdout carries the table and nothing else
    // (CLI-5).
    eprintln!("{}", t!(stats_incognito));
    if totals.unreadable > 0 {
        eprintln!("{}", t!(stats_unreadable, count = totals.unreadable));
    }
    ExitCode::SUCCESS
}

/// `--days <n>`, `--models` and `--project`, with or without a directory after it, in any order. A
/// count of zero, a count that is not a number or is past what the sentence can print back, an
/// empty directory, a repeated flag and any other word are refused rather than guessed at.
///
/// A word after `--project` is its directory unless it opens with `-`, which is the next flag.
fn read(words: &[String]) -> Option<Request> {
    let mut request = Request::default();
    let mut rest = words.iter().peekable();
    while let Some(word) = rest.next() {
        match word.as_str() {
            "--models" if !request.models => request.models = true,
            "--days" if request.days.is_none() => {
                let count: u64 = rest.next()?.parse().ok()?;
                request.days = Some((1..=MOST_DAYS).contains(&count).then_some(count)?);
            }
            "--project" if request.project.is_none() => {
                let named = rest.next_if(|next| !next.starts_with('-'));
                // A script whose variable expanded to nothing, quoted, asked about a directory it
                // did not name, and would be shown this one's sessions.
                if named.is_some_and(|directory| directory.is_empty()) {
                    return None;
                }
                request.project = Some(named.map(PathBuf::from));
            }
            _ => return None,
        }
    }
    Some(request)
}

/// The scope in a sentence, then a table: a row for each model when asked for, then the total.
fn report(request: &Request, project: Option<&Path>, totals: &Totals) -> String {
    let mut rows: Vec<(String, Spent)> = Vec::new();
    if request.models {
        rows.extend(by_model(totals));
    }
    rows.push((t!(stats_row_total).to_string(), totals.all));

    let heading = [
        if request.models {
            t!(stats_column_model).to_string()
        } else {
            String::new()
        },
        t!(stats_column_sessions).to_string(),
        t!(stats_column_turns).to_string(),
        t!(stats_column_tokens).to_string(),
        t!(stats_column_read).to_string(),
        t!(stats_column_written).to_string(),
        t!(stats_column_unsplit).to_string(),
    ];
    let cells: Vec<[String; 7]> = rows
        .iter()
        .map(|(name, spent)| {
            [
                name.clone(),
                spent.sessions.to_string(),
                spent.turns.to_string(),
                spent.tokens.to_string(),
                spent.split.read.to_string(),
                spent.split.written.to_string(),
                spent.unsplit().to_string(),
            ]
        })
        .collect();
    let widths: Vec<usize> = (0..7)
        .map(|column| {
            std::iter::once(&heading)
                .chain(&cells)
                .map(|row| width(&row[column]))
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut out = String::new();
    out.push_str(&scope(request.days, project));
    out.push_str("\n\n");
    for row in std::iter::once(&heading).chain(&cells) {
        // The name is left-aligned and the figures right-aligned, so a column of them lines up by
        // place value. Padded by hand because `format!` pads by characters, and a name of wide
        // characters takes two columns each.
        let line = row
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let padding = " ".repeat(widths[column] - width(cell));
                match column {
                    0 => format!("{cell}{padding}"),
                    _ => format!("{padding}{cell}"),
                }
            })
            .collect::<Vec<_>>()
            .join("  ");
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// How many columns of the terminal a cell takes, which is what the interface measures text by.
fn width(cell: &str) -> usize {
    bravebot_tui::wrap::display_width(cell)
}

/// A row for each model, the one that spent most first. Grouped on the name as it is shown, so two
/// names that differ only in what cleaning removes are one row rather than two that read alike. A
/// name with nothing left after cleaning shows as nothing, so it is the row for no name at all.
fn by_model(totals: &Totals) -> Vec<(String, Spent)> {
    let mut shown: BTreeMap<String, Spent> = BTreeMap::new();
    for (model, spent) in &totals.by_model {
        let name = model
            .as_deref()
            .map(|name| search::clean(name, LONGEST_MODEL))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| t!(stats_model_unknown).to_string());
        shown.entry(name).or_default().merge(*spent);
    }
    let mut rows: Vec<(String, Spent)> = shown.into_iter().collect();
    rows.sort_by(|(left_name, left), (right_name, right)| {
        right
            .tokens
            .cmp(&left.tokens)
            .then(right.sessions.cmp(&left.sessions))
            .then_with(|| left_name.cmp(right_name))
    });
    rows
}

/// What the figures cover. The directory is the person's own, typed or standing in, and is cleaned
/// like every other word printed in case its name holds an escape.
fn scope(days: Option<u64>, project: Option<&Path>) -> String {
    match (days, project) {
        (None, None) => t!(stats_scope_everything).to_string(),
        (Some(days), None) => t!(stats_scope_days, days = days),
        (None, Some(project)) => t!(stats_scope_project, project = shown(project)),
        (Some(days), Some(project)) => t!(
            stats_scope_days_project,
            days = days,
            project = shown(project)
        ),
    }
}

fn shown(project: &Path) -> String {
    search::clean(&project.display().to_string(), usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    fn spent(sessions: u64, turns: u64, tokens: u64) -> Spent {
        Spent {
            sessions,
            turns,
            tokens,
            split: bravebot_session::sessions::Split::default(),
        }
    }

    /// The same, for sessions that recorded how the tokens divided.
    fn spent_split(sessions: u64, turns: u64, read: u64, written: u64) -> Spent {
        Spent {
            split: bravebot_session::sessions::Split { read, written },
            ..spent(sessions, turns, read + written)
        }
    }

    /// The words on each row after the sentence and the heading.
    fn stats_rows_of(report: &str) -> Vec<Vec<&str>> {
        report
            .lines()
            .skip(3)
            .map(|line| line.split_whitespace().collect())
            .collect()
    }

    fn totals(models: &[(Option<&str>, Spent)]) -> Totals {
        let mut totals = Totals::default();
        for (model, spent) in models {
            totals.all.merge(*spent);
            totals.by_model.insert(model.map(str::to_string), *spent);
        }
        totals
    }

    /// Each word changes one thing the run does, so each must reach the request the run is made
    /// from, in any order, and a bare `--project` is this directory rather than every one.
    #[test]
    fn the_words_say_how_far_back_whether_to_split_by_model_and_where() {
        assert_eq!(
            read(&words("--days 7 --models --project /w/one")),
            Some(Request {
                days: Some(7),
                models: true,
                project: Some(Some(PathBuf::from("/w/one"))),
            })
        );
        assert_eq!(read(&[]), Some(Request::default()));
        assert_eq!(
            read(&words("--project")),
            Some(Request {
                project: Some(None),
                ..Request::default()
            })
        );
    }

    /// A directory after `--project` is optional, so the next flag has to end it rather than be
    /// taken for the directory's name.
    #[test]
    fn a_flag_after_project_is_a_flag_and_leaves_the_project_as_this_directory() {
        assert_eq!(
            read(&words("--project --models")),
            Some(Request {
                models: true,
                project: Some(None),
                ..Request::default()
            })
        );
        assert_eq!(
            read(&words("--project --days 2")),
            Some(Request {
                days: Some(2),
                project: Some(None),
                ..Request::default()
            })
        );
    }

    /// A script whose variable expanded to nothing, or a flag typed twice, asked for something other
    /// than the run that guessing would make. A count too large to be printed back is refused with
    /// them, so the sentence never names a different number of days than the cut used.
    #[test]
    fn a_muddled_request_is_refused_rather_than_guessed_at() {
        for muddled in [
            "--days",
            "--days 0",
            "--days two",
            "--days -1",
            "--days 1 --days 2",
            "--days 9223372036854775808",
            "--days 18446744073709551616",
            "--models --models",
            "--project /a --project /b",
            "--project --project",
            "--json",
            "all",
            "/w/one",
            "--models extra",
        ] {
            assert_eq!(read(&words(muddled)), None, "{muddled}");
        }
    }

    /// Without `--models` the answer is the one total, so a row per model would be a table the
    /// person did not ask for.
    #[test]
    fn a_table_without_models_is_the_total_alone() {
        let report = report(
            &Request::default(),
            None,
            &totals(&[(Some("a"), spent(2, 8, 1_250)), (None, spent(1, 1, 5))]),
        );

        let lines: Vec<&str> = report.lines().collect();
        assert_eq!(lines.len(), 4, "{report}");
        assert_eq!(lines[0], t!(stats_scope_everything));
        assert_eq!(lines[1], "");
        assert!(lines[2].contains(t!(stats_column_tokens)), "{report}");
        assert!(lines[3].starts_with(t!(stats_row_total)), "{report}");
        assert_eq!(
            lines[3].split_whitespace().collect::<Vec<_>>(),
            [t!(stats_row_total), "3", "9", "1255", "0", "0", "1255"],
            "{report}"
        );
    }

    /// The read and written columns are the figures of the sessions that recorded the split, and a
    /// row whose sessions did not has its tokens in the unsplit column. A column swapped for its
    /// neighbour would put the written figure under Read, and a row that counted unsplit tokens as
    /// read would claim a division no record made.
    #[test]
    fn the_split_is_drawn_under_its_own_headings_beside_the_total() {
        let report = report(
            &Request {
                models: true,
                ..Request::default()
            },
            None,
            &totals(&[
                (Some("split"), spent_split(2, 5, 900, 100)),
                (Some("old"), spent(1, 3, 40)),
            ]),
        );

        let heading: Vec<&str> = report.lines().nth(2).unwrap().split_whitespace().collect();
        assert_eq!(
            heading,
            [
                t!(stats_column_model),
                t!(stats_column_sessions),
                t!(stats_column_turns),
                t!(stats_column_tokens),
                t!(stats_column_read),
                t!(stats_column_written),
                t!(stats_column_unsplit),
            ],
            "{report}"
        );
        assert_eq!(
            stats_rows_of(&report),
            [
                vec!["split", "2", "5", "1000", "900", "100", "0"],
                vec!["old", "1", "3", "40", "0", "0", "40"],
                vec![t!(stats_row_total), "3", "8", "1040", "900", "100", "40"],
            ],
            "{report}"
        );
    }

    /// The model that spent the most is first, and the total is last whatever it is called.
    #[test]
    fn a_table_with_models_lists_the_one_that_spent_most_first_and_the_total_last() {
        let report = report(
            &Request {
                models: true,
                ..Request::default()
            },
            None,
            &totals(&[
                (Some("small"), spent(1, 2, 100)),
                (Some("big"), spent(1, 4, 9_000)),
                (None, spent(1, 0, 0)),
            ]),
        );

        let rows = stats_rows_of(&report);
        assert_eq!(
            rows,
            [
                vec!["big", "1", "4", "9000", "0", "0", "9000"],
                vec!["small", "1", "2", "100", "0", "0", "100"],
                vec![t!(stats_model_unknown), "1", "0", "0", "0", "0", "0"],
                vec![t!(stats_row_total), "3", "6", "9100", "0", "0", "9100"],
            ],
            "{report}"
        );
    }

    /// A model's name is what the server said it was. Two names that differ only in what a
    /// terminal would act on are one row, and nothing in the name reaches the screen as itself.
    #[test]
    fn a_model_name_reaches_a_row_with_nothing_a_terminal_would_act_on() {
        let report = report(
            &Request {
                models: true,
                ..Request::default()
            },
            None,
            &totals(&[
                (Some("evil\u{1b}[2Jmodel"), spent(1, 1, 10)),
                (Some("evil [2Jmodel"), spent(1, 1, 20)),
            ]),
        );

        assert!(!report.contains('\u{1b}'), "{report:?}");
        let rows: Vec<&str> = report.lines().skip(3).collect();
        assert_eq!(rows.len(), 2, "{report}");
        assert!(rows[0].starts_with("evil [2Jmodel"), "{report}");
        assert_eq!(
            rows[0].split_whitespace().rev().nth(3),
            Some("30"),
            "the two names were not one row: {report}"
        );
    }

    /// The sentence is what tells a person which sessions the figures are about, so each of the
    /// two things that narrow them has to be in it, and one day is not "1 days".
    #[test]
    fn the_sentence_says_how_far_back_and_where() {
        let here = Path::new("/w/one");
        assert_eq!(scope(None, None), t!(stats_scope_everything));
        assert_eq!(scope(Some(7), None), t!(stats_scope_days, days = 7));
        assert_eq!(scope(Some(1), None), t!(stats_scope_days, days = 1));
        assert_ne!(scope(Some(1), None), scope(Some(2), None));
        assert!(scope(None, Some(here)).contains("/w/one"));
        let both = scope(Some(7), Some(here));
        assert!(both.contains("/w/one") && both.contains('7'), "{both}");
    }

    /// The directory is the person's own, but a directory in a checkout can be named anything.
    #[test]
    fn a_directory_with_an_escape_in_its_name_is_not_drawn_raw() {
        let named = Path::new("/w/one\u{1b}[2J");
        assert!(!scope(None, Some(named)).contains('\u{1b}'));
    }

    /// A quoted variable that expanded to nothing names no directory. Left bare it is this
    /// directory, and the other way round it must not be.
    #[test]
    fn an_empty_directory_after_project_is_refused_and_not_read_as_this_one() {
        assert_eq!(read(&["--project".to_string(), String::new()]), None);
        assert!(read(&["--project".to_string()]).is_some());
    }

    /// The figures line up by place value, which a name of wide characters breaks if the padding
    /// counts characters, since each takes two columns of the terminal.
    #[test]
    fn a_model_name_of_wide_characters_keeps_the_figures_in_their_columns() {
        let report = report(
            &Request {
                models: true,
                ..Request::default()
            },
            None,
            &totals(&[
                (Some("\u{6a21}\u{578b}"), spent(1, 1, 10)),
                (Some("ab"), spent(1, 1, 5)),
            ]),
        );

        let widths: Vec<usize> = report
            .lines()
            .skip(2)
            .map(bravebot_tui::wrap::display_width)
            .collect();
        assert!(
            widths.iter().all(|columns| *columns == widths[0]),
            "the rows are not as wide as each other in the terminal: {widths:?}\n{report}"
        );
    }

    /// The largest count the cut and the sentence agree on is accepted, and the next is not.
    #[test]
    fn the_most_days_that_can_be_printed_back_are_accepted_and_one_more_is_not() {
        assert_eq!(
            read(&words("--days 9223372036854775807")),
            Some(Request {
                days: Some(i64::MAX as u64),
                ..Request::default()
            })
        );
        assert_eq!(read(&words("--days 9223372036854775808")), None);
        assert!(scope(Some(i64::MAX as u64), None).contains("9223372036854775807"));
    }

    /// A name with nothing a person could read in it, or with nothing left once the characters a
    /// terminal would act on are removed, would head a row with a blank label beside the `unknown`
    /// row. It is that row.
    #[test]
    fn a_model_name_with_nothing_left_after_cleaning_is_the_row_for_no_name() {
        let report = report(
            &Request {
                models: true,
                ..Request::default()
            },
            None,
            &totals(&[
                (Some(""), spent(1, 1, 10)),
                (Some("\u{200b}\u{1b}"), spent(1, 2, 20)),
                (None, spent(1, 4, 40)),
            ]),
        );

        let rows = stats_rows_of(&report);
        assert_eq!(
            rows,
            [
                vec![t!(stats_model_unknown), "3", "7", "70", "0", "0", "70"],
                vec![t!(stats_row_total), "3", "7", "70", "0", "0", "70"],
            ],
            "{report}"
        );
    }
}
