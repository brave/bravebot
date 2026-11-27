//! The plugin update prompt (PLUGIN-9): what a fetch of the recorded source changed, as each part
//! added, removed or changed in full, and one yes or no.
//!
//! The listing is built in `bravebot_agent::plugin_listing`, which the desktop bridge lists from
//! too, from two readings of data: the installed copy as `Before` holds it and the staged tree. Each
//! part is drawn as the install prompt draws it (the same row builders), so a
//! hook, a server, a skill or a definition reads the same wherever it is listed. Nothing here decides
//! what runs or what is replaced: the comparison produces rows and nothing else, and a part that
//! cannot be read is said in words, never listed as absent.
//!
//! Like the install prompt, nothing here asks the permission mode, an allow rule or
//! `skip_permissions`, a yes is taken only from a typed y, and none is taken while any text of the
//! plugin is cut or before every row has been drawn (`ask_drawing`).

use bravebot_agent::plugins::{Change, FileChange};
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::cell::Cell;

use crate::theme;

use super::super::chrome::{answer_keys, centred, indented, panel};
use super::super::pinned::{self, Drawn, Question, Seen};
use super::super::{Next, next_press};
use super::parts::{self, Pen};
use super::{ask_drawing, definition_body, fact_list, skill_body, untrusted};

pub use bravebot_agent::plugin_listing::{Delta, PartDeltas, UpdateRequest};

/// Put the update question to the person, blocking until answered. Only a yes returns true.
pub fn ask_update<B: Backend>(terminal: &mut Terminal<B>, request: &UpdateRequest) -> bool {
    ask_update_reading(terminal, request, next_press)
}

/// [`ask_update`], taking each event from `next`, so a test can stand in for the terminal.
pub(crate) fn ask_update_reading<B: Backend>(
    terminal: &mut Terminal<B>,
    request: &UpdateRequest,
    next: impl FnMut() -> Next,
) -> bool {
    ask_drawing(
        terminal,
        |frame, expanded, scroll, seen, cut| {
            draw_update(frame, request, expanded, scroll, seen, cut)
        },
        next,
    )
}

/// The label of a part's change, in words and in a colour.
fn mark(
    text: impl Into<String>,
    colour: ratatui::style::Color,
    width: usize,
) -> Vec<Line<'static>> {
    indented(
        text,
        Style::default().fg(colour).add_modifier(Modifier::BOLD),
        width,
    )
}

/// A heading with the counts, then each delta with its label, in full. `body` draws one item
/// and returns whether any of it is cut.
fn section<T>(
    lines: &mut Vec<Line<'static>>,
    pen: &Pen<'_>,
    what: &str,
    deltas: &[Delta<T>],
    body: &dyn Fn(&mut Vec<Line<'static>>, &T) -> bool,
) -> bool {
    let count = |wanted: fn(&Delta<T>) -> bool| deltas.iter().filter(|delta| wanted(delta)).count();
    lines.push(Line::raw(""));
    lines.extend(indented(
        t!(
            plugin_update_section,
            what = what,
            added = count(|delta| matches!(delta, Delta::Added(_))),
            removed = count(|delta| matches!(delta, Delta::Removed(_))),
            changed = count(|delta| matches!(delta, Delta::Changed { .. }))
        ),
        pen.bold,
        pen.width,
    ));
    let mut cut = false;
    for delta in deltas {
        lines.push(Line::raw(""));
        match delta {
            Delta::Added(item) => {
                lines.extend(mark(t!(plugin_update_added), theme::ok(), pen.width));
                cut |= body(lines, item);
            }
            Delta::Removed(item) => {
                lines.extend(mark(t!(plugin_update_removed), theme::fail(), pen.width));
                cut |= body(lines, item);
            }
            Delta::Changed { was, now } => {
                lines.extend(mark(t!(plugin_update_changed), theme::running(), pen.width));
                lines.extend(indented(t!(plugin_update_was), pen.muted, pen.width));
                cut |= body(lines, was);
                lines.extend(indented(t!(plugin_update_now), pen.muted, pen.width));
                cut |= body(lines, now);
            }
        }
    }
    cut
}

/// What a hooks or servers file said that could not be listed, on either side.
fn part_notes<T>(pen: &Pen<'_>, lines: &mut Vec<Line<'static>>, part: &PartDeltas<T>) {
    if let Some(fault) = part.installed_fault {
        lines.extend(indented(
            t!(plugin_update_installed_fault),
            pen.muted,
            pen.width,
        ));
        lines.extend(indented(parts::fault_text(fault), pen.muted, pen.width));
    }
    parts::left_out(pen, lines, part.new_fault, part.new_unkept);
}

/// The rows for the facts of the update: name, versions, source, ref and the two commits.
fn fact_rows(
    lines: &mut Vec<Line<'static>>,
    request: &UpdateRequest,
    pen: &Pen<'_>,
    expanded: bool,
) {
    let source_label = match request.git {
        Some(_) => t!(plugin_install_source_git),
        None => t!(plugin_install_source),
    };
    let mut facts = vec![
        (t!(plugin_install_name), request.name.clone()),
        (
            t!(plugin_install_version),
            t!(
                plugin_update_version_change,
                old = request.installed_version.as_str(),
                new = request.new_version.as_str()
            ),
        ),
        (source_label, request.source.clone()),
    ];
    if let Some(git) = &request.git {
        facts.push((
            t!(plugin_install_requested_ref),
            git.requested_ref
                .clone()
                .unwrap_or_else(|| t!(plugin_install_no_ref).to_string()),
        ));
        facts.push((
            t!(plugin_update_commit_installed),
            git.installed_commit.clone(),
        ));
        facts.push((t!(plugin_update_commit_new), git.new_commit.clone()));
    }
    fact_list(
        lines, facts, pen.margin, pen.bold, pen.muted, pen.width, expanded,
    );
}

/// The rows of the files that differ, one per file, behind the margin.
fn file_rows(lines: &mut Vec<Line<'static>>, pen: &Pen<'_>, files: Option<&[FileChange]>) {
    lines.push(Line::raw(""));
    let Some(files) = files else {
        lines.extend(indented(
            t!(plugin_update_files_unread),
            pen.muted,
            pen.width,
        ));
        return;
    };
    lines.extend(indented(
        t!(plugin_update_files, count = files.len()),
        pen.bold,
        pen.width,
    ));
    for file in files {
        let name = String::from_utf8_lossy(&file.path);
        let path = name.as_ref();
        let text = match file.change {
            Change::Added => t!(plugin_update_file_added, path = path),
            Change::Removed => t!(plugin_update_file_removed, path = path),
            Change::Changed => t!(plugin_update_file_changed, path = path),
        };
        let (rows, _) = untrusted(&text, Style::default(), pen.margin, pen.width, None, false);
        lines.extend(rows);
    }
}

/// Draw the update prompt, returning what the draw decided for its keys.
///
/// `cut` is set to whether any text of the plugin is drawn cut.
pub(super) fn draw_update(
    frame: &mut ratatui::Frame,
    request: &UpdateRequest,
    expanded: bool,
    scroll: u16,
    seen: &mut Seen,
    cut: &Cell<bool>,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::note(), t!(plugin_update_title));
    let width = inside.width as usize;
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme::muted());
    let margin = Span::styled("┃ ", Style::default().fg(theme::running()));
    let pen = Pen {
        margin: &margin,
        bold,
        muted,
        width,
    };
    let mut any_cut = false;

    let mut lines: Vec<Line<'static>> = Vec::new();
    fact_rows(&mut lines, request, &pen, expanded);
    if !request.installed_readable {
        lines.push(Line::raw(""));
        lines.extend(indented(
            t!(plugin_update_installed_unreadable),
            muted,
            width,
        ));
    }

    any_cut |= section(
        &mut lines,
        &pen,
        t!(plugin_update_what_skills),
        &request.skills,
        &|lines, skill| skill_body(lines, skill, &margin, bold, width, expanded),
    );
    section(
        &mut lines,
        &pen,
        t!(plugin_update_what_definitions),
        &request.definitions,
        &|lines, definition| {
            definition_body(lines, definition, &margin, bold, width, expanded);
            false
        },
    );
    if let Some(hooks) = &request.hooks {
        section(
            &mut lines,
            &pen,
            t!(plugin_update_what_hooks),
            &hooks.deltas,
            &|lines, hook| {
                parts::hook_body(&pen, lines, hook);
                false
            },
        );
        part_notes(&pen, &mut lines, hooks);
    }
    if let Some(servers) = &request.servers {
        section(
            &mut lines,
            &pen,
            t!(plugin_update_what_servers),
            &servers.deltas,
            &|lines, server| {
                parts::server_body(&pen, lines, server);
                false
            },
        );
        part_notes(&pen, &mut lines, servers);
    }
    file_rows(&mut lines, &pen, request.files.as_deref());

    if any_cut {
        lines.push(Line::raw(""));
        lines.extend(indented(t!(plugin_install_expand), muted, width));
    } else if expanded {
        lines.push(Line::raw(""));
        lines.extend(indented(t!(plugin_install_collapse), muted, width));
    }
    cut.set(any_cut);

    let mut below = indented(t!(plugin_update_effect), muted, width);
    below.push(Line::raw(""));
    let deciding = lines.len();
    let keys = |answerable: bool| {
        let mut rows = indented(t!(plugin_update_question), bold, width);
        rows.push(answer_keys(
            t!(plugin_update_yes),
            t!(plugin_update_no),
            answerable && !any_cut,
        ));
        rows
    };
    let question = Question {
        above: Vec::new(),
        body: lines,
        deciding,
        opening: 0,
        needed: 1,
        below,
        keys: &keys,
        picture: None,
    };
    pinned::draw(frame, inside, question, scroll, seen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::plugin_listing::{HookRow, SkillRow, UpdateGit, deltas};
    use bravebot_config::hooks::Unkept;
    use bravebot_config::plugin::FileFault;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::VecDeque;

    fn skill(name: &str, description: &str) -> SkillRow {
        SkillRow {
            name: name.into(),
            description: description.into(),
            shadowed_by: None,
        }
    }

    fn hook(argv: &[&str]) -> HookRow {
        HookRow {
            moment: "turn-started".into(),
            tool: None,
            watches: None,
            argv: argv.iter().map(ToString::to_string).collect(),
            timeout_secs: None,
        }
    }

    fn request() -> UpdateRequest {
        UpdateRequest {
            name: "demo".into(),
            installed_version: "1.0.0".into(),
            new_version: "1.1.0".into(),
            source: "https://example.com/demo.git".into(),
            git: Some(UpdateGit {
                requested_ref: Some("main".into()),
                installed_commit: "0123456789abcdef0123456789abcdef01234567".into(),
                new_commit: "fedcba9876543210fedcba9876543210fedcba98".into(),
            }),
            installed_readable: true,
            skills: vec![
                Delta::Added(skill("new-skill", "Brand new.")),
                Delta::Removed(skill("old-skill", "Going away.")),
                Delta::Changed {
                    was: skill("kept", "Reads one way."),
                    now: skill("kept", "Reads another way."),
                },
            ],
            definitions: Vec::new(),
            hooks: Some(PartDeltas {
                deltas: vec![Delta::Changed {
                    was: hook(&["/p/hook.sh", "before-word"]),
                    now: hook(&["/p/hook.sh", "after-word"]),
                }],
                installed_fault: None,
                new_fault: None,
                new_unkept: Unkept::default(),
            }),
            servers: None,
            files: Some(vec![FileChange {
                path: b"hook.sh".to_vec(),
                change: Change::Changed,
            }]),
        }
    }

    fn answered_in(request: &UpdateRequest, keys: &[KeyCode], rows: u16) -> (bool, usize, String) {
        let mut terminal = Terminal::new(TestBackend::new(100, rows)).unwrap();
        let mut queue: VecDeque<KeyCode> = keys.iter().copied().collect();
        let mut reads = 0;
        let answer = ask_update_reading(&mut terminal, request, || {
            reads += 1;
            match queue.pop_front() {
                Some(code) => Next::Press(KeyEvent::new(code, KeyModifiers::NONE)),
                None => Next::Lost,
            }
        });
        let buffer = terminal.backend().buffer().clone();
        let area = buffer.area;
        let screen = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        (answer, reads, screen)
    }

    fn answered(request: &UpdateRequest, keys: &[KeyCode]) -> (bool, usize, String) {
        answered_in(request, keys, 120)
    }

    /// PLUGIN-9, PLUGIN-4, PLUGIN-14: the prompt names the plugin, both versions, the source, the
    /// ref and the installed and the new commit in full, and lists each part that was added,
    /// removed or changed, a changed one as what it was and what it is now, every word of a changed
    /// hook's argv on its own row, and the files that differ. The question names what a yes does.
    ///
    /// Rejects: a difference that shows only the new side, a removed part left out, a commit cut to
    /// a short id or a missing old one, a hook argv word that is not drawn, and counts that do not
    /// match the rows.
    #[test]
    fn the_prompt_shows_each_part_added_removed_and_changed_with_both_commits() {
        let (_, _, screen) = answered(&request(), &[KeyCode::Char('n')]);
        for shown in [
            "Update plugin",
            "1.0.0 to 1.1.0",
            "https://example.com/demo.git",
            "0123456789abcdef0123456789abcdef01234567",
            "fedcba9876543210fedcba9876543210fedcba98",
            "Skills: 1 added, 1 removed, 1 changed",
            "Added",
            "new-skill",
            "Brand new.",
            "Removed",
            "old-skill",
            "Going away.",
            "Changed",
            "Reads one way.",
            "Reads another way.",
            "Hooks: 0 added, 0 removed, 1 changed",
            "1. /p/hook.sh",
            "2. before-word",
            "2. after-word",
            "Files that differ (1)",
            "changed hook.sh",
            "Every answer you gave",
            "withdrawn",
            "Update this plugin?",
        ] {
            assert!(screen.contains(shown), "{shown}\n{screen}");
        }
    }

    /// PLUGIN-9, PLUGIN-4: what the plugin wrote is drawn behind the margin with control and
    /// invisible characters written out, in every row of the difference.
    ///
    /// Rejects: a name, description or argv word drawn raw, and a row without the margin.
    #[test]
    fn text_of_the_difference_is_drawn_behind_the_margin_and_written_out() {
        let mut shown = request();
        shown.skills = vec![Delta::Added(skill("he\u{7}llo\u{1b}[2J", "ok\u{202e}evil"))];
        shown.hooks = Some(PartDeltas {
            deltas: vec![Delta::Added(hook(&["/p/x", "a\u{200b}b"]))],
            installed_fault: None,
            new_fault: None,
            new_unkept: Unkept::default(),
        });
        let (_, _, screen) = answered(&shown, &[KeyCode::Char('n')]);
        for raw in ['\u{7}', '\u{1b}', '\u{202e}', '\u{200b}'] {
            assert!(!screen.contains(raw), "{raw:?} drawn raw\n{screen}");
        }
        for needle in ["evil", "2. a", "demo", "https://example.com/demo.git"] {
            let row = screen
                .lines()
                .find(|row| row.contains(needle))
                .unwrap_or_else(|| panic!("{needle} not drawn\n{screen}"));
            assert!(row.contains('\u{2503}'), "{needle} has no margin: {row}");
        }
    }

    /// PLUGIN-9, PLUGIN-4: a long description is cut, the rest is shown on `e`, and a yes is not
    /// taken while any of it is cut. Hook argv rows are never cut, however many.
    ///
    /// Rejects: a cut with no way to read on, a yes over cut text, and an argv truncated.
    #[test]
    fn a_long_description_is_cut_and_a_yes_waits_for_it_and_argv_rows_are_never_cut() {
        let mut long = request();
        long.skills = vec![Delta::Added(skill(
            "long",
            &(1..=12)
                .map(|n| format!("sentence number {n}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ))];
        let words: Vec<String> = (1..=30).map(|n| format!("word{n}")).collect();
        let argv: Vec<&str> = words.iter().map(String::as_str).collect();
        long.hooks = Some(PartDeltas {
            deltas: vec![Delta::Added(hook(&argv))],
            installed_fault: None,
            new_fault: None,
            new_unkept: Unkept::default(),
        });
        let (_, _, cut) = answered_in(&long, &[KeyCode::Char('n')], 300);
        assert!(cut.contains("sentence number 1"), "{cut}");
        assert!(!cut.contains("sentence number 12"), "{cut}");
        assert!(cut.contains("e to show the rest"), "{cut}");
        assert!(cut.contains("30. word30"), "an argv row was cut\n{cut}");

        let (answer, reads, _) = answered_in(&long, &[KeyCode::Char('y'), KeyCode::Char('n')], 300);
        assert!((answer, reads) == (false, 2), "{answer} {reads}");
        let (answer, reads, shown) =
            answered_in(&long, &[KeyCode::Char('e'), KeyCode::Char('y')], 300);
        assert!((answer, reads) == (true, 2), "{answer} {reads}");
        assert!(shown.contains("sentence number 12"), "{shown}");
    }

    /// PLUGIN-9: only a typed y or Y says yes. Enter, Esc, n, an unknown key and a lost terminal do
    /// not, and a yes is not taken from a screen too short to show the question whole.
    ///
    /// Rejects: a default of yes, a lost terminal that approves, and a yes before every row of the
    /// difference has been drawn.
    #[test]
    fn only_a_y_updates_and_not_before_every_row_has_been_drawn() {
        let asked = |keys: &[KeyCode]| answered(&request(), keys).0;
        assert!(asked(&[KeyCode::Char('y')]));
        assert!(asked(&[KeyCode::Char('Y')]));
        assert!(!asked(&[KeyCode::Char('n')]));
        assert!(!asked(&[KeyCode::Esc]));
        assert!(!asked(&[]));
        assert!(!asked(&[KeyCode::Enter, KeyCode::Char('x'), KeyCode::Esc]));
        assert!(asked(&[KeyCode::Enter, KeyCode::Char('y')]));
        // On a short screen the first y is not taken, because rows of the difference have not been
        // drawn yet; paging to the end takes it.
        let (answer, _, _) = answered_in(&request(), &[KeyCode::Char('y'), KeyCode::Char('n')], 14);
        assert!(!answer);
        let mut paging = vec![KeyCode::PageDown; 40];
        paging.push(KeyCode::Char('y'));
        let (answer, _, _) = answered_in(&request(), &paging, 14);
        assert!(answer);
    }

    /// PLUGIN-9: where the installed copy could not be read, or a hooks file could not be read on
    /// either side, the prompt says so and does not show the plugin as having had nothing.
    ///
    /// Rejects: an unreadable side drawn as an empty one.
    #[test]
    fn a_side_that_could_not_be_read_is_said_and_not_shown_as_empty() {
        let mut unread = request();
        unread.installed_readable = false;
        unread.files = None;
        unread.hooks = Some(PartDeltas {
            deltas: vec![Delta::Added(hook(&["/p/hook.sh"]))],
            installed_fault: Some(FileFault::NotHooks),
            new_fault: Some(FileFault::TooLarge),
            new_unkept: Unkept::default(),
        });
        let (_, _, screen) = answered(&unread, &[KeyCode::Char('n')]);
        for said in [
            "The installed copy of this plugin could not be read",
            "The installed copy's file for this part could not be read",
            "is not a hooks file",
            "too large to read",
            "could not be compared file by file",
        ] {
            assert!(screen.contains(said), "{said}\n{screen}");
        }
    }

    /// PLUGIN-9: the difference is computed from the two readings and nothing else. A part equal on
    /// both sides is not in it, a part with the same key but other content is a change, an item
    /// only in the new reading is an addition and one only in the old is a removal.
    ///
    /// Rejects: an unchanged part listed, a change shown as a removal plus an addition, and an
    /// addition or a removal lost.
    #[test]
    fn the_difference_leaves_out_what_is_equal_and_pairs_a_change() {
        let old = vec![skill("same", "x"), skill("edit", "old"), skill("gone", "g")];
        let new = vec![
            skill("same", "x"),
            skill("edit", "new"),
            skill("fresh", "f"),
        ];
        let found = deltas(
            &old,
            &new,
            |a, b| a.name == b.name && a.description == b.description,
            |skill| skill.name.clone(),
        );
        assert_eq!(
            found,
            vec![
                Delta::Changed {
                    was: skill("edit", "old"),
                    now: skill("edit", "new")
                },
                Delta::Added(skill("fresh", "f")),
                Delta::Removed(skill("gone", "g")),
            ]
        );
        assert!(deltas(&old, &old, |a, b| a == b, |s| s.name.clone()).is_empty());
    }
}
