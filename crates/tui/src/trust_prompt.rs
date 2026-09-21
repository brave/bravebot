//! Asking, at startup, whether to trust the working directory.
//!
//! The answer decides how the session behaves. Trusting the directory means work inside it
//! proceeds without a prompt for every write, because reads from it return trusted data.
//! Declining means everything is untrusted, so every write is shown, which is the correct
//! behaviour for a directory whose contents came from somewhere else.
//!
//! Nothing is trusted by default. An unreadable terminal, an unexpected key, or a lost event
//! stream all resolve to declining, because the failure mode of guessing wrong here is that a
//! session silently writes to files nobody vouched for.
//!
//! The one session that is not asked is the one bypassing every permission, which answers this
//! question along with the rest. [`answered_by`] is where that is decided.
//!
//! What the tree already holds is put in the same box, above the question, because vouching is
//! the moment the contents of the directory become readable by a turn and disclosed to whoever
//! performs inference. A scan reported after the answer would describe a disclosure that had
//! already happened. It answers nothing on the person's behalf: the findings are there to be read
//! while the question is still open.
//!
//! A settings file may name directories to open beside the working directory, and a name in one is
//! a request rather than a grant: each is put as its own question and only an accepted one is
//! opened. A file arriving with a checkout is the easiest thing on the machine to write to, so a
//! name in one deciding what is reachable and trusted would be reach granted by whatever last
//! edited it.

use bravebot_agent::PermissionMode;
use bravebot_agent::credential_scan::TreeScan;
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use std::path::Path;

use crate::theme;

/// What the user decided about the working directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Trust,
    Decline,
    /// Leave without starting a session at all.
    Leave,
}

/// Ask about `directory`, returning the trust map the session should start with.
///
/// Trusting records the workspace root, which covers everything beneath it. Declining records
/// nothing, leaving an empty map in which no path is trusted.
///
/// Asked afresh every time a session begins. The answer is standing permission for as long as
/// that session lasts and no longer: it is not written down anywhere a later launch will read it,
/// so nothing this grants can be inherited by a session whose user was never asked. A resumed
/// session is the one exception, and it inherits the answer its own user gave rather than
/// skipping the question, which is why this is not called at all in that case.
///
/// `None` is the third answer: the user pressed Ctrl-C, which is neither trusting nor declining
/// but a request to leave, so no session begins at all.
pub fn ask<B: Backend>(
    terminal: &mut Terminal<B>,
    directory: &Path,
    scan: &TreeScan,
) -> Option<TrustStore> {
    let answer = match terminal.draw(|frame| draw(frame, directory, scan)) {
        Ok(_) => read_answer(),
        // A terminal that cannot be drawn to cannot carry the question.
        Err(_) => Answer::Decline,
    };

    trust_for(answer, directory)
}

/// Ask about each directory a settings file named, returning the ones to open.
///
/// One question per directory, because each grants reach and trust over a tree of its own and one
/// key standing in for all of them would be a grant nobody read. What an answer of yes grants is
/// what `/add-dir` grants, so a directory a file named and a directory a person typed are open on
/// identical terms.
///
/// `None` is the request to leave, for the reason it is at the working directory's own question: a
/// session that began behind it is one nobody agreed to have.
pub fn ask_named<B: Backend>(
    terminal: &mut Terminal<B>,
    directories: &[String],
) -> Option<Vec<String>> {
    accepted(directories, |directory| {
        match terminal.draw(|frame| draw_named(frame, directory)) {
            Ok(_) => read_answer(),
            // A terminal that cannot be drawn to cannot carry the question.
            Err(_) => Answer::Decline,
        }
    })
}

/// Which of the directories to open, given how the question about each was answered.
///
/// Separated from the terminal so the decision can be tested without one, the way [`trust_for`]
/// is. Leaving opens none of them, including any accepted before it: a person on their way out is
/// not somebody who granted something.
fn accepted(directories: &[String], mut answer: impl FnMut(&str) -> Answer) -> Option<Vec<String>> {
    let mut opening = Vec::new();
    for directory in directories {
        match answer(directory) {
            Answer::Trust => opening.push(directory.clone()),
            Answer::Decline => continue,
            Answer::Leave => return None,
        }
    }
    Some(opening)
}

/// The map for a session whose mode has already answered, or `None` where a person must answer.
///
/// Bypassing answers this question along with every other one, and answers it yes. That mode
/// already approves vouching for each quarantined file the planner asks to read, so putting the
/// question would be asking for a grant the session is about to make anyway, one file at a time,
/// and a modal box before the first prompt is the most conspicuous thing there is to put to a
/// person who asked to be asked about nothing.
///
/// Separated from the loop so it can be tested without a terminal.
pub fn answered_by(mode: PermissionMode, directory: &Path) -> Option<TrustStore> {
    match mode {
        PermissionMode::Bypass => Some(trusting_the_workspace(directory)),
        PermissionMode::Ask | PermissionMode::AcceptEdits | PermissionMode::Plan => None,
    }
}

/// The map an answer starts the session with, or `None` for leaving.
///
/// Every map is made against the working directory, declining included: a map is asked about
/// relative names whatever it holds, and one made against somewhere else would answer about
/// another project's files.
///
/// Public because the question is put on two surfaces and answered in one place. A session in
/// lines (CLI-14) asks it as a line rather than as a panel, and what a yes grants there has to be
/// what a yes grants here: two functions writing the map would be two readings of TRUST-7, and the
/// one the tests pin is this one.
pub fn trust_for(answer: Answer, directory: &Path) -> Option<TrustStore> {
    match answer {
        Answer::Leave => None,
        Answer::Trust => Some(trusting_the_workspace(directory)),
        Answer::Decline => Some(TrustStore::new(directory)),
    }
}

/// The rule trusting the workspace records: the root, which covers everything beneath it.
///
/// One place, so the map reached without the question is the map a yes would have written.
fn trusting_the_workspace(directory: &Path) -> TrustStore {
    let mut trust = TrustStore::new(directory);
    trust.trust(".");
    trust
}

/// Block until the user answers.
fn read_answer() -> Answer {
    loop {
        match event::read() {
            // Presses only: the interface asks for disambiguated keys, so a release arrives too,
            // and answering a question twice grants standing permission on one keystroke.
            Ok(TermEvent::Key(key)) if key.kind != event::KeyEventKind::Press => continue,
            Ok(TermEvent::Key(key)) => match answer_for(key) {
                Some(answer) => return answer,
                None => continue,
            },
            Ok(_) => continue,
            Err(_) => return Answer::Decline,
        }
    }
}

/// Interpret one key press, or `None` for a key that answers nothing.
///
/// Separated from the loop so it can be tested without a terminal.
fn answer_for(key: KeyEvent) -> Option<Answer> {
    // Raw mode delivers Ctrl-C as a key rather than as a signal, so a prompt that ignored it
    // would be a screen with no way out: the interrupt everyone reaches for would do nothing.
    // It is not an answer to the question, so it starts nothing rather than declining.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(Answer::Leave),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('y' | 'Y') => Some(Answer::Trust),
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(Answer::Decline),
        // Enter is deliberately not a yes: it is the key most likely to be pressed
        // out of habit, and this question grants standing permission.
        _ => None,
    }
}

/// Draw the question about the working directory.
fn draw(frame: &mut ratatui::Frame, directory: &Path, scan: &TreeScan) {
    let mut lines = vec![
        asking(
            t!(trust_directory_question),
            &directory.display().to_string(),
        ),
        Line::raw(""),
    ];
    lines.extend(found(scan));
    lines.extend([
        // One line each, wrapped by the paragraph rather than broken here: a translation does
        // not break where the English did, and a sentence split into two spans cannot be rewrapped.
        Line::from(Span::raw(t!(trust_directory_explained))),
        Line::raw(""),
        Line::from(Span::styled(
            t!(trust_directory_regardless),
            Style::default().fg(theme::muted()),
        )),
    ]);

    panel(
        frame,
        t!(trust_directory_title),
        lines,
        keys(t!(trust_directory_yes), t!(trust_directory_no)),
    );
}

/// How many findings are put on the screen before the rest become a count.
///
/// A real repository answers a first scan with more findings than anybody reads standing at a
/// modal box, and a list long enough to push the question off the screen is how a scan gets
/// ignored. The ranking in [`TreeScan::findings`] is what makes a short list the right short
/// list.
const SHOWN: usize = 3;

/// What the scan of the working tree says, as lines a person reads.
///
/// Public because the question is put on two surfaces and a scan is reported on both, and because
/// a session that is never asked is told the same thing in its transcript: three readings of what
/// a finding says would be three chances for one of them to say the value.
///
/// Every line is the driver's own words about a path, with the path put through the same
/// replacement the rest of the interface uses. A file in the tree is named by whoever wrote the
/// tree, so a name carrying an escape sequence would otherwise draw over a panel that exists to
/// be read before a grant.
pub fn scan_report(scan: &TreeScan) -> Vec<String> {
    said(scan, scan.findings().len())
}

/// The same report with the findings cut to the few that are put in front of somebody at once.
///
/// What a question carries, on either surface. The whole of it is what the transcript keeps:
/// a finding nothing names anywhere is a finding that was counted and then lost, and there is
/// no store yet for it to be looked up in.
pub fn scan_summary(scan: &TreeScan) -> Vec<String> {
    said(scan, SHOWN)
}

/// The report, naming at most `most` of the findings.
fn said(scan: &TreeScan, most: usize) -> Vec<String> {
    let findings = scan.findings();
    let mut lines = Vec::new();

    if findings.is_empty() {
        lines.push(t!(trust_directory_scan_none, files = scan.read()));
    } else {
        lines.push(t!(trust_directory_scan_found, count = findings.len()));
        lines.extend(
            findings
                .iter()
                .take(most)
                .map(|finding| crate::render::printable(&finding.describe())),
        );
        if findings.len() > most {
            lines.push(t!(trust_directory_scan_more, count = findings.len() - most));
        }
    }

    if !scan.everything_was_read() {
        lines.push(t!(trust_directory_scan_partial).to_string());
    }
    lines
}

/// The report, styled for the panel: what was found reads as a warning and the rest as an aside.
///
/// The first line is the one that changes the answer, so it is the one drawn in the colour that
/// says so. A directory where nothing matched says so quietly, because a scan that shouts about
/// finding nothing is one people learn to skip past.
fn found(scan: &TreeScan) -> Vec<Line<'static>> {
    // A directory where the whole walk ran and matched nothing has nothing to put in the box.
    // The scan is recorded in the transcript either way, which is where a person can go and look
    // for it; a modal box that says "nothing found" on every launch is a box people stop reading,
    // and this one grants standing permission over a whole tree.
    if scan.findings().is_empty() && scan.everything_was_read() {
        return Vec::new();
    }
    let findings = scan.findings().len();
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (index, said) in scan_summary(scan).into_iter().enumerate() {
        let style = match index == 0 && findings > 0 {
            true => Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
            false => Style::default().fg(theme::muted()),
        };
        lines.push(Line::from(Span::styled(said, style)));
    }
    lines.push(Line::raw(""));
    lines
}

/// Draw the question about one directory a settings file named.
fn draw_named(frame: &mut ratatui::Frame, directory: &str) {
    let lines = vec![
        asking(t!(named_directory_question), directory),
        Line::raw(""),
        Line::from(Span::raw(t!(named_directory_explained))),
        Line::raw(""),
        Line::from(Span::styled(
            t!(named_directory_regardless),
            Style::default().fg(theme::muted()),
        )),
    ];

    panel(
        frame,
        t!(named_directory_title),
        lines,
        keys(t!(named_directory_yes), t!(named_directory_no)),
    );
}

/// What is being asked, and the path it is being asked about.
///
/// The path is the whole of what an answer is about, so it is drawn rather than summarised.
fn asking(question: &str, directory: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{question} "),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            directory.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("?"),
    ])
}

/// The answers on offer: the same two keys and the same way out at either question.
fn keys(yes: &str, no: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "  y",
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {yes}    ")),
        Span::styled(
            "n",
            Style::default()
                .fg(theme::fail())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {no}    ")),
        Span::styled(
            "ctrl-c",
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", t!(quit)),
            Style::default().fg(theme::muted()),
        ),
    ])
}

/// Draw one question, in a box whose every cell the theme paints, with its answers on the last
/// row.
///
/// One implementation for both questions here, because the chrome is what says the question is the
/// system's own: `Clear` empties the cells under the panel without colouring them, so the
/// background and text colour are set for the block rather than for the border alone.
///
/// The answers are drawn into a row of their own at the foot of the box rather than as the last
/// line of the prose. A paragraph longer than the box is clipped at the bottom, and the line that
/// goes first is then the one saying which keys answer the question: a panel that grants standing
/// permission over a whole tree, asked on a small terminal or in a directory with something to
/// report, would be a box with no visible way to answer it. Clipping the middle of the
/// explanation costs a reader something; clipping the keys costs them the question.
fn panel(
    frame: &mut ratatui::Frame,
    title: &str,
    lines: Vec<Line<'static>>,
    answers: Line<'static>,
) {
    let area = centred(frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::brand_primary()))
        .title(format!(" {title} "))
        .style(Style::default().bg(theme::background()).fg(theme::text()));
    let inside = block.inner(area);
    frame.render_widget(block, area);

    if inside.height == 0 {
        return;
    }
    // As many rows as the answers need at this width, never more than the box has. A narrow
    // terminal wraps them rather than cutting them off at the edge, which is the same reason
    // they are drawn at the foot rather than at the end of the prose.
    let answers = Paragraph::new(answers).wrap(Wrap { trim: false });
    let needed = (answers.line_count(inside.width) as u16).clamp(1, inside.height);
    let prose = Rect {
        height: inside.height - needed,
        ..inside
    };
    let foot = Rect {
        y: inside.y + inside.height - needed,
        height: needed,
        ..inside
    };

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), prose);
    frame.render_widget(answers, foot);
}

/// A centred box, sized to the terminal but never larger than it.
fn centred(area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(5),
            Constraint::Percentage(90),
            Constraint::Percentage(5),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::credential_scan::{scan_tree, scan_tree_within};
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use std::time::Duration;

    /// The working directory these answers are about.
    fn here() -> &'static Path {
        Path::new("/work")
    }

    /// A tree built for one test, removed first so a previous run leaves nothing behind.
    fn tree(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = crate::testutil::scratch_dir(&format!("bravebot-trust-prompt-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("make the directory");
        for (path, contents) in files {
            std::fs::write(root.join(path), contents).expect("write the file");
        }
        root
    }

    /// A finished scan of a directory holding nothing, which is what most of these tests are
    /// about the absence of.
    ///
    /// Named by its caller, because these tests run beside each other and two of them making and
    /// removing one directory is a race that fails whichever got there second.
    fn nothing_found(name: &str) -> TreeScan {
        scan_tree(&tree(name, &[]))
    }

    /// An AWS key id, which is a shape rather than a guess.
    const A_DECLARED_KEY: &str = "AKIAIOSFODNN7EXAMPLE";

    /// The characters one question puts on a terminal, in reading order.
    ///
    /// Takes the drawing rather than a path, so both questions are measured by one helper and an
    /// assertion about what a prompt says cannot be made of one and forgotten on the other.
    fn rendered(draw_it: impl FnOnce(&mut ratatui::Frame)) -> String {
        let mut terminal = Terminal::new(TestBackend::new(72, 20)).expect("terminal");
        terminal.draw(draw_it).expect("draw");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// Every cell the border encloses carries the theme's own colours, for one question.
    ///
    /// `Clear` empties the cells under a panel without colouring them, so a prompt that styled
    /// only its border is a hole in the palette: themed border, terminal-default everything else.
    fn paints_the_themes_chrome(draw_it: impl FnOnce(&mut ratatui::Frame)) {
        let _held = theme::exclusive();
        let theme = theme::find("nord").expect("nord is built in");
        theme::apply(&theme);
        let painted = theme::background();

        let mut terminal = Terminal::new(TestBackend::new(72, 20)).expect("terminal");
        terminal.draw(draw_it).expect("draw");
        let buffer = terminal.backend().buffer().clone();
        theme::apply_brave();

        let inside = centred(*buffer.area());
        // Every cell the border encloses, including the rows the prose did not reach: an unpainted
        // row below the keys is the same hole as an unpainted one beside them.
        for y in 1..inside.height - 1 {
            for x in 1..inside.width - 1 {
                let cell = &buffer[(inside.x + x, inside.y + y)];
                assert_eq!(
                    cell.bg, painted,
                    "the cell at {x},{y} kept the terminal's own background"
                );
                assert_ne!(
                    cell.fg,
                    Color::Reset,
                    "the cell at {x},{y} kept the terminal's own text colour"
                );
            }
        }
    }

    fn names(directories: &[&str]) -> Vec<String> {
        directories.iter().map(|d| d.to_string()).collect()
    }

    /// The flag says nothing is put to the person, and a modal box before the first prompt is the
    /// most conspicuous thing there is to put. The map is the one a yes would have written, which
    /// is what the mode is about to grant anyway: it approves vouching for every quarantined file
    /// the planner reads, so the workspace ends up trusted a file at a time regardless.
    #[test]
    fn bypassing_trusts_the_workspace_instead_of_asking() {
        let trust =
            answered_by(PermissionMode::Bypass, here()).expect("bypassing answers the question");

        assert!(trust.is_trusted("."));
        assert!(trust.is_trusted("src/main.rs"), "the rule covers the tree");
    }

    /// Every other mode answers fewer questions than this one, so none of them may answer the one
    /// that grants standing permission over a whole tree.
    #[test]
    fn every_other_mode_leaves_the_question_to_the_person() {
        for mode in [
            PermissionMode::Ask,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
        ] {
            assert!(answered_by(mode, here()).is_none(), "{mode:?} answered it");
        }
    }

    #[test]
    fn the_prompt_names_the_directory_and_both_answers() {
        let output = rendered(|frame| {
            draw(
                frame,
                Path::new("/home/me/project"),
                &nothing_found("named"),
            )
        });
        assert!(output.contains("/home/me/project"));
        assert!(output.contains("trust it"));
        assert!(output.contains("every write"));
    }

    /// The question must say what saying yes actually does, since it grants standing
    /// permission rather than approving one action.
    #[test]
    fn the_prompt_explains_the_consequence() {
        let output =
            rendered(|frame| draw(frame, Path::new("/tmp/x"), &nothing_found("explained")));
        assert!(output.contains("trusted"), "no mention of trust: {output}");
        // Wrapping can split a phrase across lines, so assert on a short fragment.
        assert!(
            output.contains("Say no if you"),
            "no guidance on when to decline: {output}"
        );
    }

    /// This is the first screen of a session and it grants standing permission over a whole tree,
    /// so its chrome is the first thing that says the question is the system's own.
    #[test]
    fn the_prompt_paints_the_themes_background_inside_its_border() {
        paints_the_themes_chrome(|frame| {
            draw(
                frame,
                Path::new("/home/me/project"),
                &nothing_found("painted"),
            )
        });
    }

    /// The whole of the clause: what is in the directory is on the screen while the question
    /// about vouching for it is still unanswered. Trusting is the moment the tree becomes
    /// readable by a turn and disclosed to whoever performs inference, so a report drawn after
    /// the answer would describe a disclosure that had already happened.
    #[test]
    fn what_the_scan_found_is_on_the_panel_the_question_is_asked_in() {
        let root = tree("found", &[(".env", &format!("KEY={A_DECLARED_KEY}\n"))]);
        let scan = scan_tree(&root);

        let output = rendered(|frame| draw(frame, &root, &scan));

        assert!(
            output.contains("1 credential is already in this directory"),
            "the finding was not drawn beside the question: {output}"
        );
        assert!(
            output.contains("an AWS access key id at .env:1"),
            "the finding did not say what it was or where: {output}"
        );
        assert!(
            output.contains("trust it"),
            "the answer was pushed off the panel by the report: {output}"
        );
    }

    /// The answers have to fit the terminal they are drawn on, not just the wide one. Drawn in a
    /// row of their own they are clipped at the edge rather than wrapped unless the row is given
    /// the height the width needs, and a narrow window is where the box is already tightest.
    #[test]
    fn the_answers_wrap_rather_than_run_off_a_narrow_terminal() {
        let mut terminal = Terminal::new(TestBackend::new(48, 20)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, Path::new("/tmp/x"), &nothing_found("narrow")))
            .expect("draw");
        let output: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(output.contains("trust it"), "{output}");
        assert!(output.contains("ask me about every write"), "{output}");
        assert!(output.contains("ctrl-c"), "{output}");
    }

    /// The keys are the question. This box grants standing permission over a whole tree and is
    /// answered by one press, so a report long enough to fill the panel must not be what takes
    /// the answers off it: a person looking at findings with no visible way to say no is a
    /// person who presses `y`.
    #[test]
    fn the_answers_stay_on_the_panel_however_much_there_is_to_report() {
        let root = tree(
            "crowded",
            &[(
                ".env",
                &format!(
                    "A={A_DECLARED_KEY}\nB={A_DECLARED_KEY}1\nC={A_DECLARED_KEY}2\n\
                     D={A_DECLARED_KEY}3\nE={A_DECLARED_KEY}4\n"
                ),
            )],
        );
        let scan = scan_tree(&root);

        let output = rendered(|frame| draw(frame, &root, &scan));

        assert!(output.contains("trust it"), "{output}");
        assert!(output.contains("ask me about every write"), "{output}");
        assert!(output.contains("ctrl-c"), "{output}");
    }

    /// A report that quoted what it found would be a second copy of every credential in the
    /// directory, drawn by the thing that was checking for copies, on a screen anybody walking
    /// past can read.
    #[test]
    fn the_panel_never_draws_the_value_that_was_found() {
        let root = tree(
            "quiet",
            &[("config.env", &format!("KEY={A_DECLARED_KEY}\n"))],
        );
        let scan = scan_tree(&root);

        let output = rendered(|frame| draw(frame, &root, &scan));

        assert!(!output.contains(A_DECLARED_KEY), "{output}");
        assert!(
            !output.contains(&A_DECLARED_KEY[..8]),
            "a prefix of the value is most of what identifies it: {output}"
        );
    }

    /// A real repository answers a first scan with more findings than anybody reads standing at
    /// a modal box. A list long enough to push the question and its answers off the screen is
    /// how a person ends up pressing `y` at a panel they could not read.
    #[test]
    fn a_long_list_of_findings_is_cut_short_and_the_rest_counted() {
        let root = tree(
            "many",
            &[(
                ".env",
                &format!(
                    "A={A_DECLARED_KEY}\nB={A_DECLARED_KEY}1\nC={A_DECLARED_KEY}2\nD={A_DECLARED_KEY}3\nE={A_DECLARED_KEY}4\n"
                ),
            )],
        );
        let scan = scan_tree(&root);
        assert_eq!(
            scan.findings().len(),
            5,
            "the fixture must overflow the cut"
        );

        let shown = scan_summary(&scan);

        assert_eq!(
            shown.iter().filter(|line| line.contains(".env:")).count(),
            SHOWN,
            "{shown:?}"
        );
        assert!(
            shown.iter().any(|line| line.contains("and 2 more")),
            "the findings past the cut were dropped rather than counted: {shown:?}"
        );

        // And the record keeps every one of them. Nothing stores a finding yet, so one that is
        // counted and named nowhere is one nobody can ever look at.
        let whole = scan_report(&scan);
        assert_eq!(
            whole.iter().filter(|line| line.contains(".env:")).count(),
            5,
            "{whole:?}"
        );
        assert!(
            !whole.iter().any(|line| line.contains("more")),
            "the whole report still counted something it had named: {whole:?}"
        );
    }

    /// A walk that stopped has to say so wherever it is reported. Silence read as a clean
    /// directory is the one thing a partial scan must never produce, and the person is about to
    /// answer a question on the strength of it.
    #[test]
    fn a_scan_that_ran_out_of_time_says_so_rather_than_reading_as_clean() {
        let root = tree("cut", &[(".env", &format!("KEY={A_DECLARED_KEY}\n"))]);

        let stopped = scan_tree_within(&root, Duration::ZERO);
        let report = scan_report(&stopped);
        assert!(
            report
                .iter()
                .any(|line| line.contains("Part of this directory was not read")),
            "{report:?}"
        );

        let output = rendered(|frame| draw(frame, &root, &stopped));
        assert!(
            output.contains("Part of this directory was not read"),
            "{output}"
        );
    }

    /// The scan is recorded whatever it found, because a person who cannot tell whether it ran
    /// cannot read its silence. It is kept out of the panel in that case and not out of the
    /// record: the box grants standing permission over a tree and is the wrong place for a line
    /// that says nothing happened.
    #[test]
    fn a_directory_where_nothing_matched_is_reported_without_crowding_the_question() {
        let scan = nothing_found("quiet-directory");

        let report = scan_report(&scan);
        assert!(
            report.iter().any(|line| line.contains("Matched nothing")),
            "{report:?}"
        );

        let output = rendered(|frame| draw(frame, Path::new("/home/me/project"), &scan));
        assert!(!output.contains("Matched nothing"), "{output}");
    }

    /// A file in the tree is named by whoever wrote the tree. A name carrying an escape sequence
    /// would otherwise move the cursor and recolour the very panel that exists to be read before
    /// a grant, which is the one screen where a forged line costs the most.
    #[test]
    #[cfg(unix)]
    fn a_file_name_carrying_an_escape_sequence_cannot_draw_on_the_panel() {
        let root = tree("escaping", &[]);
        std::fs::write(
            root.join("\u{1b}[31mgotcha.env"),
            format!("KEY={A_DECLARED_KEY}\n"),
        )
        .expect("write the file");
        let scan = scan_tree(&root);

        let report = scan_report(&scan);

        assert!(
            report.iter().any(|line| line.contains('\u{241b}')),
            "the escape was not turned into a glyph: {report:?}"
        );
        assert!(
            !report.iter().any(|line| line.contains('\u{1b}')),
            "an escape sequence reached the panel: {report:?}"
        );
    }

    /// A settings file arrives with whatever produced the checkout, so a directory named in one is
    /// a request. Only the answer opens it, and a name nobody accepted leaves the path as
    /// unreachable and unvouched for as any other outside the workspace.
    #[test]
    fn a_directory_a_file_named_is_opened_only_where_the_person_accepts_it() {
        let named = names(&["/home/me/notes", "/home/me/.ssh", "/srv/shared"]);

        let accepted_one = accepted(&named, |directory| match directory {
            "/home/me/notes" => Answer::Trust,
            _ => Answer::Decline,
        })
        .expect("answering still starts a session");
        assert_eq!(accepted_one, names(&["/home/me/notes"]));

        let accepted_none =
            accepted(&named, |_| Answer::Decline).expect("declining still starts a session");
        assert!(
            accepted_none.is_empty(),
            "a declined name was opened anyway: {accepted_none:?}"
        );
    }

    /// Leaving is the answer to no question, so a name accepted before it is not a grant either:
    /// a session that opened a directory on the way out would be acting on an answer nobody
    /// finished giving.
    #[test]
    fn leaving_at_one_of_the_questions_opens_nothing() {
        let named = names(&["/home/me/notes", "/home/me/.ssh"]);

        let answered = accepted(&named, |directory| match directory {
            "/home/me/notes" => Answer::Trust,
            _ => Answer::Leave,
        });

        assert!(answered.is_none(), "leaving started a session anyway");
    }

    /// The path is the whole of what the answer is about, and it is the one thing a settings file
    /// chose rather than the person reading the box.
    #[test]
    fn the_named_prompt_shows_the_directory_it_would_open() {
        let output = rendered(|frame| draw_named(frame, "/home/me/.ssh"));
        assert!(output.contains("/home/me/.ssh"), "no path: {output}");
        assert!(output.contains("open it"));
        assert!(output.contains("leave it closed"));
    }

    /// Opening a directory grants two things at once, reach and trust, and neither is on the
    /// screen unless the question says so. The question also has to say where it came from: a box
    /// naming a directory the person has never typed is otherwise unexplained.
    #[test]
    fn the_named_prompt_explains_what_opening_does() {
        let output = rendered(|frame| draw_named(frame, "/srv/shared"));
        // Wrapping can split a phrase across lines, so assert on short fragments.
        assert!(
            output.contains("settings file"),
            "no mention of where the name came from: {output}"
        );
        assert!(output.contains("trusted"), "no mention of trust: {output}");
    }

    /// This question needs the theme's own chrome for the reason the working directory's does: the
    /// frame is what says the question is the system's and not something a file being read is
    /// asking.
    #[test]
    fn the_named_prompt_paints_the_themes_background_inside_its_border() {
        paints_the_themes_chrome(|frame| draw_named(frame, "/home/me/.ssh"));
    }

    /// Both questions, since either can be the first thing a session draws on a small terminal.
    ///
    /// Surviving the draw is half of it. These two are asked before a session exists, and nothing
    /// else is on the screen to say what the keys mean, so a small terminal that drew the border
    /// and lost the question would leave somebody pressing `y` at a panel that never said what it
    /// was about.
    #[test]
    fn a_tiny_terminal_still_renders() {
        let mut terminal = Terminal::new(TestBackend::new(24, 8)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, Path::new("/tmp/x"), &nothing_found("tiny")))
            .expect("must not panic on a small area");
        assert!(
            drawn_on(&terminal).contains("Trust /tmp/x?"),
            "the question was drawn out of view: {}",
            drawn_on(&terminal)
        );

        terminal
            .draw(|frame| draw_named(frame, "/tmp/x"))
            .expect("must not panic on a small area");
        assert!(
            drawn_on(&terminal).contains("Open /tmp/x?"),
            "the question was drawn out of view: {}",
            drawn_on(&terminal)
        );
    }

    /// The characters on a terminal that has already been drawn to.
    fn drawn_on(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// Trusting records the root, which covers the whole tree. Asked of the answer rather than of
    /// a store built here, for the reason [`declining_trusts_nothing`] is: a `trust_for` that
    /// returned an empty map on a yes would satisfy a test that trusted `.` itself, and every
    /// write would be shown to somebody who said yes.
    #[test]
    fn trusting_covers_the_whole_workspace() {
        let trust = trust_for(Answer::Trust, here()).expect("trusting starts a session");
        assert!(trust.is_trusted("."));
        assert!(trust.is_trusted("src/main.rs"));
        assert!(trust.is_trusted("deep/nested/file.txt"));
    }

    /// The key that grants standing permission over a whole tree is the deliberate one and no
    /// other. Enter is the key most likely to be pressed out of habit, so it answers nothing: a
    /// keystroke made without reading must not be the answer that costs the most to get wrong.
    #[test]
    fn only_y_trusts_and_enter_answers_nothing() {
        let pressed = |code| KeyEvent::new(code, KeyModifiers::NONE);

        assert_eq!(answer_for(pressed(KeyCode::Char('y'))), Some(Answer::Trust));
        assert_eq!(answer_for(pressed(KeyCode::Char('Y'))), Some(Answer::Trust));
        assert_eq!(
            answer_for(pressed(KeyCode::Char('n'))),
            Some(Answer::Decline)
        );
        assert_eq!(
            answer_for(pressed(KeyCode::Char('N'))),
            Some(Answer::Decline)
        );
        assert_eq!(answer_for(pressed(KeyCode::Esc)), Some(Answer::Decline));
        assert_eq!(answer_for(pressed(KeyCode::Enter)), None);
    }

    /// Ctrl-C is the interrupt everyone reaches for, and raw mode turns it into an ordinary key
    /// press. A prompt that ignored it would be a screen with no way out.
    #[test]
    fn ctrl_c_leaves_rather_than_answering_the_question() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(answer_for(key), Some(Answer::Leave));
    }

    /// Leaving is not a quiet decline: a session that started anyway would be one the user
    /// never agreed to have.
    #[test]
    fn leaving_starts_no_session() {
        assert!(trust_for(Answer::Leave, here()).is_none());
        assert!(trust_for(Answer::Decline, here()).is_some());
    }

    /// A plain `c` is not an interrupt, and neither is any other control chord.
    #[test]
    fn only_ctrl_c_leaves() {
        assert_eq!(
            answer_for(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            answer_for(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)),
            None
        );
    }

    /// Declining leaves nothing trusted, so every write is shown. Asked of the answer rather than
    /// of a store built here, which is what the clause is about: a `trust_for` that trusted `.`
    /// on a decline would have satisfied a test that only built its own empty store.
    #[test]
    fn declining_trusts_nothing() {
        let trust = trust_for(Answer::Decline, here()).expect("declining still starts a session");
        assert!(trust.is_empty());
        assert!(!trust.is_trusted("src/main.rs"));
        assert!(!trust.is_trusted("."));
    }
}
