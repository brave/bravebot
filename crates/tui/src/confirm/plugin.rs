//! The plugin install prompt (PLUGIN-4): the name, version and source of a plugin staged from a
//! local path, every skill and definition it offers, and one yes or no.
//!
//! Nothing here asks the permission mode, an allow rule or `skip_permissions`: the prompt has no
//! request type of the confirmer, so nothing in `bravebot_agent::permission_mode` can name it. The
//! one function that puts it is [`ask_install`], and it answers no wherever it cannot be drawn or
//! the terminal cannot be read.
//!
//! Everything the plugin wrote is drawn behind the margin that marks untrusted content, through
//! [`marked_rows`], which writes control and invisible characters out. A long description is cut to
//! a few rows until `e` shows the rest, and no yes is taken while any text is cut, so nothing the
//! plugin said is left unread by a yes. The body scrolls, and no yes is taken before every row of
//! it has been drawn.

mod parts;

use bravebot_agent::plugin_source::Source;
use bravebot_agent::plugins::{self, Offered, Staged};
use bravebot_agent::skills::Catalogue;
use bravebot_core::delegate::Definitions;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::cell::Cell;

use crate::render::marked_rows;
use crate::theme;

use super::chrome::{answer_keys, centred, indented, panel};
use super::pinned::{self, Approving, Drawn, Question, Seen};
use super::{Next, next_press};

pub use parts::{HookRow, Listed, ServerRow};

/// How many rows of one description are drawn before `e` is pressed.
const DESCRIPTION_ROWS: usize = 3;

/// A skill the plugin offers, as the prompt lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRow {
    pub name: String,
    pub description: String,
    /// What takes the name instead, where a source above a plugin offers it.
    pub shadowed_by: Option<String>,
}

/// A definition the plugin offers, as the prompt lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionRow {
    pub name: String,
    pub kind: String,
    pub shadowed_by: Option<String>,
}

/// The rows only a git source has (PLUGIN-4, PLUGIN-14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRows {
    /// The ref the person typed after `#`, or `None` where they named none.
    pub requested_ref: Option<String>,
    /// The full 40-digit commit the clone resolved to.
    pub commit: String,
}

/// What the install prompt shows. Built from the parts of the staged tree, not from a field the
/// manifest wrote about itself (PLUGIN-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRequest {
    pub name: String,
    pub version: String,
    /// The source directory. A local path has no commit, so none is shown.
    pub source: String,
    /// What a git source adds to the listing. `None` for a local path, which has no commit, so
    /// neither row is drawn for it.
    pub git: Option<GitRows>,
    pub skills: Vec<SkillRow>,
    pub definitions: Vec<DefinitionRow>,
    /// The hooks the plugin's hooks file declares, `${PLUGIN_ROOT}` already replaced. `None` where
    /// the manifest names no hooks file.
    pub hooks: Option<Listed<HookRow>>,
    /// The servers the plugin's servers file declares, `${PLUGIN_ROOT}` already replaced. `None`
    /// where the manifest names no servers file.
    pub servers: Option<Listed<ServerRow>>,
}

impl InstallRequest {
    /// The listing of `staged`, with each name a higher-ranked source offers marked as shadowed.
    /// `catalogue` and `definitions` are what discovery found for this session.
    pub fn from_staged(staged: &Staged, catalogue: &Catalogue, definitions: &Definitions) -> Self {
        let offers = plugins::staged_offers(staged);
        let mut skills = Vec::new();
        let mut kinds = Vec::new();
        for mark in plugins::shadow_marks(&offers, catalogue, definitions) {
            match mark.offer.what {
                Offered::Skill { description } => skills.push(SkillRow {
                    name: mark.offer.name,
                    description,
                    shadowed_by: mark.by,
                }),
                Offered::Definition { kind } => kinds.push(DefinitionRow {
                    name: mark.offer.name,
                    kind: kind.to_string(),
                    shadowed_by: mark.by,
                }),
            }
        }
        Self {
            name: staged.manifest.name.clone(),
            version: staged.manifest.version.clone(),
            source: staged.source.to_string(),
            git: match &staged.source {
                Source::Local(_) => None,
                Source::Git {
                    requested_ref,
                    commit,
                    ..
                } => Some(GitRows {
                    requested_ref: requested_ref.clone(),
                    commit: commit.to_string(),
                }),
            },
            skills,
            definitions: kinds,
            hooks: parts::hooks_of(staged),
            servers: parts::servers_of(staged),
        }
    }
}

/// What a key at the prompt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InstallResponse {
    Yes,
    No,
    Scroll(i16),
    /// Show the rest of every cut part, or cut them again.
    Expand,
}

impl Approving for InstallResponse {
    fn approves(&self) -> bool {
        match self {
            InstallResponse::Yes => true,
            InstallResponse::No | InstallResponse::Scroll(_) | InstallResponse::Expand => false,
        }
    }
}

/// One key at the prompt, or `None` for a key that answers nothing.
///
/// `cut` says text of the plugin is drawn cut, and no yes is taken then. Enter is not an answer.
pub(super) fn install_answer_for(
    key: KeyEvent,
    drawn: &Drawn,
    cut: bool,
) -> Option<InstallResponse> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(InstallResponse::No),
            _ => None,
        };
    }
    let page = drawn.page();
    let response = drawn.take(match key.code {
        KeyCode::Char('y' | 'Y') => Some(InstallResponse::Yes),
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(InstallResponse::No),
        KeyCode::Char('e' | 'E') => Some(InstallResponse::Expand),
        KeyCode::Up | KeyCode::Char('k') => Some(InstallResponse::Scroll(-1)),
        KeyCode::Down | KeyCode::Char('j') => Some(InstallResponse::Scroll(1)),
        KeyCode::PageUp => Some(InstallResponse::Scroll(-page)),
        KeyCode::PageDown => Some(InstallResponse::Scroll(page)),
        KeyCode::Home => Some(InstallResponse::Scroll(i16::MIN)),
        KeyCode::End => Some(InstallResponse::Scroll(i16::MAX)),
        _ => None,
    });
    response.filter(|response| !(cut && response.approves()))
}

/// Put the install question to the person, blocking until answered. Only a yes returns true.
pub fn ask_install<B: Backend>(terminal: &mut Terminal<B>, request: &InstallRequest) -> bool {
    ask_install_reading(terminal, request, next_press)
}

/// [`ask_install`], taking each event from `next`, so a test can stand in for the terminal.
pub(crate) fn ask_install_reading<B: Backend>(
    terminal: &mut Terminal<B>,
    request: &InstallRequest,
    mut next: impl FnMut() -> Next,
) -> bool {
    let mut scroll = 0u16;
    let mut seen = Seen::default();
    let mut expanded = false;
    let mut drawn = Drawn::default();
    let cut = Cell::new(false);
    let mut ignored = false;
    loop {
        if !ignored
            && terminal
                .draw(|frame| {
                    drawn = draw_install(frame, request, expanded, scroll, &mut seen, &cut)
                })
                .is_err()
        {
            return false;
        }
        ignored = false;
        match next() {
            Next::Ignored => ignored = true,
            Next::Skip => continue,
            Next::Lost => return false,
            Next::Wheel(by) => scroll = drawn.moved(scroll, by),
            Next::Press(key) => match install_answer_for(key, &drawn, cut.get()) {
                Some(InstallResponse::Yes) => return true,
                Some(InstallResponse::No) => return false,
                Some(InstallResponse::Scroll(by)) => scroll = drawn.moved(scroll, by),
                Some(InstallResponse::Expand) => expanded = !expanded,
                None => continue,
            },
        }
    }
}

/// `text` behind the margin, one row per line, cut to `limit` rows unless `expanded`. Returns the
/// rows and whether any was cut.
pub(super) fn untrusted(
    text: &str,
    style: Style,
    margin: &Span<'static>,
    width: usize,
    limit: Option<usize>,
    expanded: bool,
) -> (Vec<Line<'static>>, bool) {
    let mut rows = Vec::new();
    for line in text.lines() {
        rows.extend(marked_rows(
            margin,
            &[Span::styled(line.to_string(), style)],
            width,
        ));
    }
    match limit {
        Some(limit) if rows.len() > limit && !expanded => {
            rows.truncate(limit);
            (rows, true)
        }
        _ => (rows, false),
    }
}

/// The rows for the plugin's name, version, source and, for a git source, its ref and commit.
fn fact_rows(
    lines: &mut Vec<Line<'static>>,
    request: &InstallRequest,
    margin: &Span<'static>,
    bold: Style,
    muted: Style,
    width: usize,
    expanded: bool,
) {
    let source_label = match request.git {
        Some(_) => t!(plugin_install_source_git),
        None => t!(plugin_install_source),
    };
    let mut facts = vec![
        (t!(plugin_install_name), request.name.clone()),
        (t!(plugin_install_version), request.version.clone()),
        (source_label, request.source.clone()),
    ];
    if let Some(git) = &request.git {
        facts.push((
            t!(plugin_install_requested_ref),
            git.requested_ref
                .clone()
                .unwrap_or_else(|| t!(plugin_install_no_ref).to_string()),
        ));
        facts.push((t!(plugin_install_commit), git.commit.clone()));
    }
    for (label, value) in facts {
        lines.extend(indented(label, muted, width));
        let (rows, _) = untrusted(&value, bold, margin, width, None, expanded);
        lines.extend(rows);
    }
}

/// Draw the install prompt, returning what the draw decided for its keys.
///
/// `cut` is set to whether any text of the plugin is drawn cut.
pub(super) fn draw_install(
    frame: &mut ratatui::Frame,
    request: &InstallRequest,
    expanded: bool,
    scroll: u16,
    seen: &mut Seen,
    cut: &Cell<bool>,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::note(), t!(plugin_install_title));
    let width = inside.width as usize;
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme::muted());
    let margin = Span::styled("┃ ", Style::default().fg(theme::running()));
    let mut any_cut = false;

    let mut lines: Vec<Line<'static>> = Vec::new();
    fact_rows(&mut lines, request, &margin, bold, muted, width, expanded);

    lines.push(Line::raw(""));
    lines.extend(match request.skills.is_empty() {
        true => indented(t!(plugin_install_no_skills), muted, width),
        false => indented(
            t!(plugin_install_skills, count = request.skills.len()),
            bold,
            width,
        ),
    });
    for skill in &request.skills {
        lines.push(Line::raw(""));
        let (rows, _) = untrusted(&skill.name, bold, &margin, width, None, expanded);
        lines.extend(rows);
        let (rows, was_cut) = untrusted(
            &skill.description,
            Style::default(),
            &margin,
            width,
            Some(DESCRIPTION_ROWS),
            expanded,
        );
        lines.extend(rows);
        any_cut |= was_cut;
        shadow_rows(&mut lines, skill.shadowed_by.as_deref(), width);
    }

    lines.push(Line::raw(""));
    lines.extend(match request.definitions.is_empty() {
        true => indented(t!(plugin_install_no_definitions), muted, width),
        false => indented(
            t!(
                plugin_install_definitions,
                count = request.definitions.len()
            ),
            bold,
            width,
        ),
    });
    definition_rows(&mut lines, request, &margin, bold, width, expanded);

    let pen = parts::Pen {
        margin: &margin,
        bold,
        muted,
        width,
    };
    parts::draw_hooks(&pen, &mut lines, request.hooks.as_ref());
    parts::draw_servers(&pen, &mut lines, request.servers.as_ref());

    if any_cut {
        lines.push(Line::raw(""));
        lines.extend(indented(t!(plugin_install_expand), muted, width));
    } else if expanded {
        lines.push(Line::raw(""));
        lines.extend(indented(t!(plugin_install_collapse), muted, width));
    }
    cut.set(any_cut);

    let mut below = indented(t!(plugin_install_not_done), muted, width);
    below.push(Line::raw(""));
    let deciding = lines.len();
    let keys = |answerable: bool| {
        let mut rows = indented(t!(plugin_install_question), bold, width);
        rows.push(answer_keys(
            t!(plugin_install_yes),
            t!(plugin_install_no),
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

/// The rows for each definition: its name, its kind and the mark of a source that shadows it.
fn definition_rows(
    lines: &mut Vec<Line<'static>>,
    request: &InstallRequest,
    margin: &Span<'static>,
    bold: Style,
    width: usize,
    expanded: bool,
) {
    for definition in &request.definitions {
        lines.push(Line::raw(""));
        let (rows, _) = untrusted(&definition.name, bold, margin, width, None, expanded);
        lines.extend(rows);
        let (rows, _) = untrusted(
            &t!(plugin_install_kind, kind = definition.kind.as_str()),
            Style::default(),
            margin,
            width,
            None,
            expanded,
        );
        lines.extend(rows);
        shadow_rows(lines, definition.shadowed_by.as_deref(), width);
    }
}

/// The mark under a name that a higher-ranked source takes. Its origin is a path, so it is drawn
/// behind the margin as well.
fn shadow_rows(lines: &mut Vec<Line<'static>>, by: Option<&str>, width: usize) {
    if let Some(by) = by {
        lines.extend(marked_rows(
            &Span::styled("  ", Style::default()),
            &[Span::styled(
                t!(plugin_install_shadowed, by = by).to_string(),
                Style::default().fg(theme::note()),
            )],
            width,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyEvent;
    use std::collections::VecDeque;

    fn request() -> InstallRequest {
        InstallRequest {
            name: "demo".into(),
            version: "2.3.4".into(),
            source: "/work/src-demo".into(),
            git: None,
            hooks: None,
            servers: None,
            skills: vec![SkillRow {
                name: "hello".into(),
                description: "Says hello.".into(),
                shadowed_by: None,
            }],
            definitions: vec![DefinitionRow {
                name: "helper".into(),
                kind: "worker".into(),
                shadowed_by: None,
            }],
        }
    }

    /// The keys a person presses at the prompt, the answer, how many keys it read, and the last
    /// screen it drew. A script that runs out is a lost terminal.
    fn answered(request: &InstallRequest, keys: &[KeyCode]) -> (bool, usize, String) {
        answered_in(request, keys, 60)
    }

    /// [`answered`] on a terminal `rows` high, for a listing longer than a screen.
    fn answered_in(request: &InstallRequest, keys: &[KeyCode], rows: u16) -> (bool, usize, String) {
        let mut terminal = Terminal::new(TestBackend::new(100, rows)).unwrap();
        let mut queue: VecDeque<KeyCode> = keys.iter().copied().collect();
        let mut reads = 0;
        let answer = ask_install_reading(&mut terminal, request, || {
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

    /// R23, R24, R25, R30: the prompt names the plugin, version and source, every skill with its
    /// description, every definition with its kind, says what a yes does not do, and puts one
    /// question. A local path has no commit, so none is drawn.
    ///
    /// Rejects: a part left out, and a prompt that does not say a hook or a server stays off.
    #[test]
    fn the_prompt_lists_every_part_and_says_what_a_yes_does_not_do() {
        let (_, _, screen) = answered(&request(), &[KeyCode::Char('n')]);
        for shown in [
            "demo",
            "2.3.4",
            "/work/src-demo",
            "hello",
            "Says hello.",
            "helper",
            "kind: worker",
            "does not make a hook run",
            "make a server reachable",
            "Install this plugin?",
            "Yes, install it",
            "No, install nothing",
        ] {
            assert!(screen.contains(shown), "{shown}\n{screen}");
        }
        assert!(!screen.to_lowercase().contains("commit"), "{screen}");
    }

    /// PLUGIN-4, PLUGIN-14: a git source's prompt shows the requested ref and the full commit,
    /// each behind the margin, and says plainly when no ref was named.
    ///
    /// Rejects: a commit cut to a short id, a requested ref that is not drawn, a commit drawn
    /// unmarked, a ref shown where none was typed, and rows drawn for a local path.
    #[test]
    fn a_git_source_shows_the_requested_ref_and_the_full_commit() {
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let mut shown = request();
        shown.source = "https://example.com/demo.git".into();
        shown.git = Some(GitRows {
            requested_ref: Some("release-2".into()),
            commit: commit.into(),
        });
        let (_, _, screen) = answered(&shown, &[KeyCode::Char('n')]);
        for expected in [
            "https://example.com/demo.git",
            "Requested ref",
            "release-2",
            "Commit it resolved to",
            commit,
        ] {
            assert!(screen.contains(expected), "{expected}\n{screen}");
        }
        let row = screen
            .lines()
            .find(|row| row.contains(commit))
            .expect("the commit is drawn");
        assert!(row.contains('\u{2503}'), "{row}");
        assert!(!screen.contains("none given"), "{screen}");

        shown.git = Some(GitRows {
            requested_ref: None,
            commit: commit.into(),
        });
        let (_, _, unnamed) = answered(&shown, &[KeyCode::Char('n')]);
        assert!(unnamed.contains("none given"), "{unnamed}");
        assert!(unnamed.contains(commit), "{unnamed}");

        let (_, _, local) = answered(&request(), &[KeyCode::Char('n')]);
        assert!(!local.contains("Requested ref"), "{local}");
        assert!(!local.contains("Commit it resolved to"), "{local}");
    }

    /// R26: what the plugin wrote is drawn behind the margin, with control and invisible characters
    /// written out, so a name cannot move the cursor or hide text.
    ///
    /// Rejects: a name or description drawn raw.
    #[test]
    fn text_from_the_plugin_is_drawn_as_untrusted_content() {
        let mut shown = request();
        shown.skills[0].name = "he\u{7}llo\u{1b}[2J".into();
        shown.skills[0].description = "ok\u{202e}evil".into();
        shown.definitions[0].name = "he\u{200b}lper".into();
        let (_, _, screen) = answered(&shown, &[KeyCode::Char('n')]);
        for raw in ['\u{7}', '\u{1b}', '\u{202e}', '\u{200b}'] {
            assert!(!screen.contains(raw), "{raw:?} drawn raw\n{screen}");
        }
        assert!(screen.contains('\u{2503}'), "no margin\n{screen}");
        let rows: Vec<&str> = screen.lines().collect();
        let row = rows
            .iter()
            .find(|row| row.contains("evil"))
            .expect("the description is drawn");
        assert!(row.contains('\u{2503}'), "{row}");
    }

    /// R27: a long description is cut, the rest is shown on `e`, and a yes is not taken while any
    /// of it is cut. Nothing is dropped.
    ///
    /// Rejects: a description cut with no way to read the rest, and a yes taken over cut text.
    #[test]
    fn a_long_description_is_cut_and_shown_on_request_and_a_yes_waits_for_it() {
        let mut long = request();
        long.skills[0].description = (1..=12)
            .map(|n| format!("sentence number {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (_, _, cut) = answered(&long, &[KeyCode::Char('n')]);
        assert!(cut.contains("sentence number 1"), "{cut}");
        assert!(!cut.contains("sentence number 12"), "{cut}");
        assert!(cut.contains("e to show the rest"), "{cut}");

        // A yes over cut text answers nothing, so the prompt reads on.
        let (answer, reads, _) = answered(&long, &[KeyCode::Char('y'), KeyCode::Char('n')]);
        assert!((answer, reads) == (false, 2), "{answer} {reads}");
        // After `e` the whole text is drawn and the yes is taken.
        let (answer, reads, shown) = answered(&long, &[KeyCode::Char('e'), KeyCode::Char('y')]);
        assert!((answer, reads) == (true, 2), "{answer} {reads}");
        assert!(shown.contains("sentence number 12"), "{shown}");
    }

    /// R28: a skill or definition that a higher-ranked source takes is marked as shadowed.
    ///
    /// Rejects: a mark dropped from the drawing.
    #[test]
    fn a_shadowed_skill_and_definition_are_marked() {
        let mut shown = request();
        shown.skills[0].shadowed_by = Some("built-in".into());
        shown.definitions[0].shadowed_by = Some("the project's agents".into());
        let (_, _, screen) = answered(&shown, &[KeyCode::Char('n')]);
        assert!(screen.contains("Shadowed by built-in"), "{screen}");
        assert!(
            screen.contains("Shadowed by the project's agents"),
            "{screen}"
        );
        let (_, _, plain) = answered(&request(), &[KeyCode::Char('n')]);
        assert!(!plain.contains("Shadowed"), "{plain}");
    }

    /// PLUGIN-4, PLUGIN-13: a plugin with a hooks file and a servers file is listed in full, from
    /// the staged tree, with `${PLUGIN_ROOT}` already the installed path, each word of each argv on
    /// its own row behind the margin, and the host, the command and the files on rows of their own.
    ///
    /// Rejects: a hook or server left out of the prompt, an argv drawn with the unexpanded
    /// variable or the staging path, an argv cut or joined into one row, a host or a file that has
    /// no row, text drawn without the margin, and an environment value that is expanded.
    #[test]
    fn hooks_and_servers_are_listed_in_full_with_the_installed_path_and_their_reach() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = scratch.path().join("src");
        std::fs::create_dir_all(source.join("skills")).unwrap();
        std::fs::write(
            source.join("plugin.toml"),
            "name = \"demo\"\nversion = \"1\"\ndescription = \"d\"\nskills = \"skills\"\nhooks = \"h.json\"\nservers = \"s.json\"\n",
        )
        .unwrap();
        std::fs::write(
            source.join("h.json"),
            r#"{"hooks":[{"on":"turn-started","run":["${PLUGIN_ROOT}/hook.sh","--flag","two words"]},{"on":"nope","run":["x"]}]}"#,
        )
        .unwrap();
        std::fs::write(
            source.join("s.json"),
            r#"{"servers":{"local":{"transport":"stdio","argv":["${PLUGIN_ROOT}/server.py","-v"],"variables":["API_KEY"],"env":{"HOME_X":"${PLUGIN_ROOT}/x"},"reads":["/etc/hosts"]},"web":{"transport":"http","url":"https://mcp.example.org/v1"},"broken":{"transport":"stdio"}}}"#,
        )
        .unwrap();
        let staged = bravebot_agent::plugins::stage(&source, &home).unwrap();
        let request = InstallRequest::from_staged(
            &staged,
            &bravebot_agent::skills::Catalogue::default(),
            &Definitions::default(),
        );
        let installed = bravebot_agent::plugins::installed_dir(&home, "demo");
        let installed = installed.display().to_string();
        let (_, _, screen) = answered_in(&request, &[KeyCode::Char('n')], 200);
        let flat: String = screen
            .lines()
            .map(|row| {
                row.trim()
                    .trim_matches('\u{2502}')
                    .trim_start_matches('\u{2503}')
                    .trim()
            })
            .collect();
        for expected in [
            "Hooks (1)",
            "Runs at turn-started",
            "2. --flag",
            "3. two words",
            "The hooks file holds more than is listed here: 1 entries",
            "Servers (3)",
            "local",
            "stdio",
            "API_KEY",
            "HOME_X=${PLUGIN_ROOT}/x",
            "Would start the program",
            "May read the file",
            "/etc/hosts",
            "https://mcp.example.org/v1",
            "Would reach the host",
            "mcp.example.org",
            "never started",
        ] {
            assert!(screen.contains(expected), "{expected}\n{screen}");
        }
        // Rows wrap at the screen width, so the paths are compared with the rows joined.
        assert!(flat.contains(&format!("{installed}/hook.sh")), "{flat}");
        assert!(flat.contains(&format!("{installed}/server.py")), "{flat}");
        assert!(installed.contains("plugins"), "{installed}");
        assert!(!screen.contains(".plugin-staging"), "{screen}");
        assert!(!screen.contains("hook.sh${"), "{screen}");
        let row = screen
            .lines()
            .find(|row| row.contains("two words"))
            .expect("the third word is drawn");
        assert!(row.contains('\u{2503}'), "{row}");
        let row = screen
            .lines()
            .find(|row| row.contains("mcp.example.org") && !row.contains("https"))
            .expect("the host has a row");
        assert!(row.contains('\u{2503}'), "{row}");
    }

    /// PLUGIN-4: a hooks file that cannot be read is said in the listing, so the prompt is not
    /// shorter than the plugin without telling the person.
    ///
    /// Rejects: an unusable file listed as a plugin with no hooks.
    #[test]
    fn an_unreadable_hooks_file_is_said_and_not_listed_as_no_hooks() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = scratch.path().join("src");
        std::fs::create_dir_all(source.join("skills")).unwrap();
        std::fs::write(
            source.join("plugin.toml"),
            "name = \"demo\"\nversion = \"1\"\ndescription = \"d\"\nskills = \"skills\"\nhooks = \"h.json\"\n",
        )
        .unwrap();
        std::fs::write(source.join("h.json"), "{ not json").unwrap();
        let staged = bravebot_agent::plugins::stage(&source, &home).unwrap();
        let request = InstallRequest::from_staged(
            &staged,
            &bravebot_agent::skills::Catalogue::default(),
            &Definitions::default(),
        );
        let (_, _, screen) = answered(&request, &[KeyCode::Char('n')]);
        assert!(screen.contains("Hooks (0)"), "{screen}");
        assert!(screen.contains("is not a hooks file"), "{screen}");
        assert!(!screen.contains("offers no hook"), "{screen}");
        let (_, _, plain) = answered(&request_without_parts(), &[KeyCode::Char('n')]);
        assert!(plain.contains("offers no hook"), "{plain}");
        assert!(plain.contains("offers no server"), "{plain}");
    }

    fn request_without_parts() -> InstallRequest {
        request()
    }

    /// PLUGIN-4: a server whose alias is fine alone but whose `<plugin>_<alias>` is longer than a
    /// server's name may be is listed in full and marked never started, as an entry that states no
    /// usable declaration is. The same server under a shorter alias carries no such mark.
    ///
    /// Rejects: a listing that marks usable a server a session can never reach, which a person
    /// would answer yes to as though it would start.
    #[test]
    fn a_server_whose_joined_alias_is_too_long_is_listed_as_never_started() {
        let listing = |alias: &str| {
            let scratch = tempfile::tempdir().unwrap();
            let home = scratch.path().join("home");
            let source = scratch.path().join("src");
            std::fs::create_dir_all(&source).unwrap();
            std::fs::write(
                source.join("plugin.toml"),
                "name = \"demo\"\nversion = \"1\"\ndescription = \"d\"\nservers = \"s.json\"\n",
            )
            .unwrap();
            std::fs::write(
                source.join("s.json"),
                format!(
                    r#"{{"servers":{{"{alias}":{{"transport":"http","url":"https://mcp.example.org/v1"}}}}}}"#
                ),
            )
            .unwrap();
            let staged = bravebot_agent::plugins::stage(&source, &home).unwrap();
            let request = InstallRequest::from_staged(
                &staged,
                &bravebot_agent::skills::Catalogue::default(),
                &Definitions::default(),
            );
            let (_, _, screen) = answered_in(&request, &[KeyCode::Char('n')], 200);
            screen
        };
        let too_long = "x".repeat(60);
        let long = listing(&too_long);
        assert!(
            long.contains("https://mcp.example.org/v1"),
            "listed in full\n{long}"
        );
        assert!(
            long.contains("too long together, so it is never started"),
            "{long}"
        );
        let short = listing(&"x".repeat(59));
        assert!(short.contains("https://mcp.example.org/v1"), "{short}");
        assert!(!short.contains("never started"), "{short}");
    }

    /// Only a typed y or Y says yes. Enter, Esc, n, a lost terminal and an unknown key do not.
    ///
    /// Rejects: a default of yes, and a lost terminal that approves.
    #[test]
    fn only_a_y_installs() {
        let asked = |keys: &[KeyCode]| answered(&request(), keys).0;
        assert!(asked(&[KeyCode::Char('y')]));
        assert!(asked(&[KeyCode::Char('Y')]));
        assert!(!asked(&[KeyCode::Char('n')]));
        assert!(!asked(&[KeyCode::Esc]));
        assert!(!asked(&[]));
        // Enter and an unknown key answer nothing, so the next key decides.
        assert!(!asked(&[KeyCode::Enter, KeyCode::Char('x'), KeyCode::Esc]));
        assert!(asked(&[KeyCode::Enter, KeyCode::Char('y')]));
    }
}
