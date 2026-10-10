//! A question about an effect as the lines a person reads before approving it.
//!
//! One implementation for every surface that shows a question as text. The command-line session
//! draws these lines in a terminal and the editor bridge sends them to an editor, so the two
//! cannot disagree about what an approval shows, which markers a change carries, how much of it is
//! left out, or how a line of quarantined content is made safe to display. Each question has a
//! plain description of itself that the caller fills in from whatever it holds, and one function
//! that words it.
//!
//! The functions here read the content they are given, to replace its control characters and to
//! count its lines. They are called by presentation callers only and run nowhere near the driver.

#![forbid(unsafe_code)]

use bravebot_agent::diff::Change;
use bravebot_i18n::t;

/// Drawn down the margin of everything the planner was not allowed to read.
///
/// The same glyph the interactive transcript uses, on every row of the block, so the mark cannot
/// be ended by anything written inside it. A caption could be imitated; a margin cannot.
pub const QUARANTINE_BAR: &str = "\u{2503}";

/// How many lines of what a question is about are shown before the rest is counted instead.
///
/// A write is approved from the change it would make and a read from the bytes it would release,
/// so the content is what the question is; and the whole of a generated file is not readable as
/// one. A person scrolled past a thousand lines is answering whatever was in front of them at the
/// end of it, which is not the question that was asked.
pub const MOST_CONTENT_LINES: usize = 40;

/// Replace control characters, so shown text cannot move the cursor or recolour the screen.
///
/// The margin in front of every row is written by the caller. An escape sequence in the content
/// would let the content write one instead, and a forged margin is worse than no margin, since
/// drawing one is the whole claim being made about the block.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if !c.is_control() || c == '\t' {
                c
            } else {
                // The Unicode pictures for C0, so an escape reads as ␛ rather than vanishing: a
                // character silently dropped is one nobody can tell was ever in the file.
                char::from_u32(0x2400 + c as u32).unwrap_or('\u{fffd}')
            }
        })
        .collect()
}

/// Quarantined content as rows, each behind a margin, capped.
///
/// The margin is on every row, for the reason [`QUARANTINE_BAR`] is on every row of a block: a
/// caption above the block could be imitated by the block's own first line, and a margin cannot.
/// The cap is because a question has to be readable as one: a person scrolled past a thousand
/// lines of output is answering whatever is in front of them at the end of it.
pub fn quarantined(content: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut counted = content.lines();
    for line in counted.by_ref().take(MOST_CONTENT_LINES) {
        rows.push(format!("{QUARANTINE_BAR} {}", printable(line)));
    }
    let left_out = counted.count();
    if left_out > 0 {
        rows.push(t!(transcript_more_lines, count = left_out).to_string());
    }
    rows
}

/// What a write's approval holds besides the sentence naming it.
pub struct Write<'a> {
    /// The body came from somewhere nobody vouched for.
    pub untrusted: bool,
    /// What the isolated processor that produced the body said about it, already trimmed.
    pub remark: Option<&'a [String]>,
    /// What the scan inferred about the body, each already a kind, a location and a masked preview.
    pub credentials: &'a [String],
    /// The working directory's file was written after the checkout it is being brought back from.
    pub written_since_checkout: bool,
    /// What the write does to the file's line terminators, which `changes` cannot show.
    pub line_endings: Option<&'a str>,
    /// False when the diff had to give up on an exact answer.
    pub exact: bool,
    /// Lines the change adds, for saying what an inexact diff is too large to show.
    pub added: usize,
    /// Lines the change removes.
    pub removed: usize,
    /// The change, already condensed to the context the caller wants around each run.
    pub changes: &'a [Change],
}

/// The lines a proposed write is read before approving: the note on where the body came from,
/// then the change itself.
pub fn write_lines(write: &Write<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    if write.untrusted {
        lines.push(t!(write_untrusted).to_string());
    }
    // What the isolated processor that produced the body said about it, beside the diff rather than
    // somewhere up the scrollback: a remark saying a typo was fixed is only a claim worth anything
    // while the lines it describes are in front of the person reading it. It decides nothing, and
    // it is free text a processor authored, so it goes behind the margin with the content.
    if let Some(remark) = write.remark {
        lines.push(t!(write_remark).to_string());
        lines.extend(quarantined(&remark.join("\n")));
    }
    // What the scan inferred, beside the lines it read it from. These are the driver's own words
    // about its own findings, each already a kind, a location and a masked preview, so no part of
    // the value is repeated here and none of it needs the margin content sits behind.
    if !write.credentials.is_empty() {
        lines.push(t!(write_credentials).to_string());
        lines.extend(write.credentials.iter().map(|found| printable(found)));
    }

    if write.written_since_checkout {
        lines.push(t!(write_since_checkout).to_string());
    }
    lines.extend(write.line_endings.map(str::to_string));

    // A change too large to diff says so rather than showing a guess at it. The summary the caller
    // puts above still counts the lines.
    if !write.exact {
        lines.push(t!(
            write_too_large_to_show,
            added = write.added,
            removed = write.removed
        ));
        return lines;
    }

    let mut changed = 0usize;
    let mut left_out = 0usize;
    for held in write.changes {
        if changed == MOST_CONTENT_LINES {
            left_out += 1;
            continue;
        }
        changed += 1;
        lines.push(match held {
            Change::Added(line) => format!("+ {}", printable(line)),
            Change::Removed(line) => format!("- {}", printable(line)),
            Change::Kept(line) => format!("  {}", printable(line)),
            Change::Elided(count) => t!(write_unchanged, count = *count).to_string(),
        });
    }
    if left_out > 0 {
        lines.push(t!(transcript_more_lines, count = left_out).to_string());
    }
    lines
}

/// What an automated check made of some content, as far as a person reading the question is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// The check completed and found nothing.
    Safe,
    /// The check found something.
    Unsafe,
    /// The check did not complete.
    Inconclusive,
}

impl Check {
    /// The check a verdict's word names (`Verdict::word` in `bravebot-core`). A word that is none
    /// of the three is the one that reassures least.
    pub fn from_word(word: &str) -> Self {
        match word {
            "safe" => Self::Safe,
            "unsafe" => Self::Unsafe,
            _ => Self::Inconclusive,
        }
    }
}

/// What a check made of the same bytes, in one line.
///
/// Advice beside the content and never in place of it: it decides nothing here, exactly as it
/// decides nothing in the panel. The check's own sentence is not carried: it is free text written
/// about content an attacker may own, and a line-oriented question has no margin to put it behind.
pub fn checked(check: Check) -> String {
    match check {
        Check::Safe => t!(check_safe),
        Check::Unsafe => t!(check_unsafe),
        Check::Inconclusive => t!(check_inconclusive),
    }
    .to_string()
}

/// One step of a run as the person reads it.
pub struct RunStep {
    /// The step as the line wrote it, each argument quoted so a space inside one cannot read as
    /// the boundary between two.
    pub written: String,
    /// The file this step runs, because a name is not a program: `$PATH` decides what `grep` means.
    pub binary: String,
}

/// What the stages of a run are confined to, in the sentences the agent worded for it.
pub struct Confinement {
    /// The sentence that introduces the directories.
    pub heading: String,
    /// The directories every stage reads and writes.
    pub directories: Vec<String>,
    /// What else the turn confines, in the order the agent gave it.
    pub sentences: Vec<String>,
}

/// A run, as the lines a person reads before approving it.
pub struct Run<'a> {
    /// The agent's one-line description of the run.
    pub summary: &'a str,
    /// Every step, in order.
    pub steps: &'a [RunStep],
    /// The paths the plan says it writes.
    pub writes: &'a [String],
    /// What the stages are confined to, or `None` where the turn does not confine them.
    pub confinement: Option<&'a Confinement>,
    /// One sentence for each access the line reaches that nothing here holds, as the agent worded it.
    pub ambient: &'a [String],
    /// Bytes going into a program are released somewhere this policy stops governing.
    pub releases_private: bool,
}

/// The lines a run is read before approving: every step as the line wrote it, the binary each name
/// resolved to, what it would write, and what it is not confined to.
///
/// The same things the panel shows, in the same order.
pub fn run_lines(run: &Run<'_>) -> Vec<String> {
    let mut lines = vec![printable(run.summary)];
    for step in run.steps {
        lines.push(printable(&step.written));
        lines.push(format!("  {}", printable(&step.binary)));
    }
    if !run.writes.is_empty() {
        lines.push(t!(run_writes).to_string());
        for path in run.writes {
            lines.push(format!("  {}", printable(path)));
        }
    }
    // Said every time, because it is the thing a person is likeliest to assume otherwise: what the
    // programs are confined to where the turn confines them, and that they are not where it does not.
    match run.confinement {
        Some(confined) => {
            lines.push(confined.heading.clone());
            for directory in &confined.directories {
                lines.push(format!("  {}", printable(directory)));
            }
            lines.extend(confined.sentences.iter().cloned());
        }
        None => lines.push(t!(run_not_sandboxed).to_string()),
    }
    // Which access in particular a yes hands over, where the line reaches one nothing here holds.
    // The line above says what confinement there is and is said every time; this says what is
    // being granted, and is said only where there is something to name.
    if !run.ambient.is_empty() {
        lines.push(t!(run_spends_authority).to_string());
        for sentence in run.ambient {
            lines.push(format!("  {sentence}"));
        }
    }
    if run.releases_private {
        lines.push(t!(run_releases_private).to_string());
    }
    lines
}

/// The lines a fetch is read before approving: the host on its own row, then the URL.
///
/// The host is its own row because that is what the answer is about: a URL is easy to misread, and
/// `https://example.com@evil.test/` names one site and reaches another.
pub fn fetch_lines(host: &str, url: &str, metadata_service: bool) -> Vec<String> {
    let mut lines = vec![t!(fetch_host, host = printable(host)), printable(url)];
    // What the host is, where it is this machine's own metadata service. That service asks
    // nothing of whoever opens the socket and answers with the credentials of the role, so
    // the address alone does not say what the request reaches.
    if metadata_service {
        lines.push(t!(fetch_authority_metadata).to_string());
    }
    lines.push(t!(fetch_explained).to_string());
    lines
}

/// The lines a command's output is read before the planner may: what it is, what was made of it,
/// then the output behind the margin.
pub fn output_lines(summary: &str, check: Check, output: &str) -> Vec<String> {
    let mut lines = vec![
        printable(summary),
        t!(output_unseen).to_string(),
        checked(check),
    ];
    lines.extend(quarantined(output));
    lines
}

/// A picture or a PDF in place of the text of a vetted read.
pub struct Picture<'a> {
    /// The copy to open, which is the driver's own path.
    pub path: &'a str,
    /// Whether it is a PDF, which can hold text no page draws.
    pub pdf: bool,
}

/// A vetted read, as the lines a person reads before approving it.
pub struct Vet<'a> {
    /// The agent's one-line description of the read.
    pub summary: &'a str,
    /// The planner's own words about what it expects.
    pub expects: &'a str,
    /// What the check made of the content.
    pub check: Check,
    /// The text, shown where there is no picture.
    pub content: &'a str,
    /// The picture to open in place of the text.
    pub picture: Option<Picture<'a>>,
}

/// The lines a vetted read is read before approving.
pub fn vet_lines(vet: &Vet<'_>) -> Vec<String> {
    let mut lines = vec![
        printable(vet.summary),
        // The planner's own words about what it expects, which is untrusted for the reason
        // everything the planner wrote is.
        t!(vet_expected, expects = printable(vet.expects)),
        t!(vet_covers_this_only).to_string(),
        t!(vet_unseen).to_string(),
        checked(vet.check),
    ];
    match &vet.picture {
        // A picture is not a thing a line of text can carry, so the person is given a copy to
        // open. The path is the driver's own, which is why it goes out as any line of this
        // program's does rather than inside the margin.
        Some(picture) => {
            lines.push(t!(vet_picture_open).to_string());
            lines.push(printable(picture.path));
            lines.push(t!(vet_picture_words).to_string());
            if picture.pdf {
                lines.push(t!(vet_pdf_hidden_text).to_string());
            }
        }
        None => lines.extend(quarantined(vet.content)),
    }
    lines
}

/// The lines a quarantined file is read before it is vouched for.
pub fn vouch_lines(path: &str, check: Check, preview: &str) -> Vec<String> {
    let mut lines = vec![
        printable(path),
        t!(vouch_explained).to_string(),
        checked(check),
    ];
    match preview.is_empty() {
        true => lines.push(t!(vouch_nothing).to_string()),
        false => lines.extend(quarantined(preview)),
    }
    lines
}

/// The findings in a file the planner asked to read, and nothing of the file.
///
/// Each line is already a kind, a place and a mask, so there is no content here to picture or to
/// put behind a margin. `printable` is still applied, since a control character reaching a terminal
/// from any direction is a cursor somewhere else, and the path in a finding is a path the planner
/// may have spelled.
pub fn exposure_lines(path: &str, findings: &[String]) -> Vec<String> {
    let mut lines = vec![
        printable(path),
        t!(expose_explained).to_string(),
        t!(expose_found).to_string(),
    ];
    lines.extend(
        findings
            .iter()
            .map(|finding| format!("  {}", printable(finding))),
    );
    lines
}

/// A language server the planner would like started.
pub struct Server<'a> {
    /// The agent's one-line description of the server.
    pub summary: &'a str,
    /// The absolute path the server's name resolved to.
    pub program: &'a str,
    /// The tree it would index.
    pub workspace: &'a str,
    /// Starting it runs code from the dependency tree with the person's own access.
    pub runs_build_tooling: bool,
}

/// The lines a language server is read before it is started.
pub fn server_lines(server: &Server<'_>) -> Vec<String> {
    vec![
        printable(server.summary),
        printable(server.program),
        t!(server_workspace, workspace = printable(server.workspace)),
        match server.runs_build_tooling {
            true => t!(server_build_tooling).to_string(),
            false => t!(server_reads_only).to_string(),
        },
        t!(server_explained).to_string(),
    ]
}

/// One tool an MCP server lists.
pub struct Tool<'a> {
    /// The name under the alias.
    pub name: &'a str,
    /// Each argument as one line.
    pub arguments: &'a [String],
    /// The server's own sentence about it.
    pub description: Option<&'a str>,
}

/// The tools an MCP server lists, as the lines a person reads before they are offered to a model.
pub struct Tools<'a> {
    /// The server's alias.
    pub alias: &'a str,
    /// Every tool the client would draw.
    pub tools: &'a [Tool<'a>],
    /// How many the client would not draw.
    pub refused: usize,
    /// A list was vouched for under this declaration before and this is not it.
    pub changed: bool,
    /// What the check made of the list.
    pub check: Check,
}

/// The whole list, each description behind the margin and none of it cut.
///
/// A yes puts exactly this text in front of the planner for every session the list stays the
/// same, so what is read here has to be all of it. The description is behind the margin because it
/// is the server's words, and the names and arguments are not because the client drew them from a
/// fixed alphabet.
pub fn tools_lines(list: &Tools<'_>) -> Vec<String> {
    let mut lines = vec![match list.tools.is_empty() {
        true => t!(mcp_tools_none, alias = printable(list.alias)),
        false => t!(
            mcp_tools_offered,
            alias = printable(list.alias),
            count = list.tools.len()
        ),
    }];
    if list.changed {
        lines.push(t!(mcp_tools_changed).to_string());
    }
    lines.push(checked(list.check));
    for tool in list.tools {
        lines.push(format!("  {}", printable(tool.name)));
        if !tool.arguments.is_empty() {
            lines.push(format!("    {}", printable(&tool.arguments.join(", "))));
        }
        if let Some(description) = tool.description {
            lines.extend(
                description
                    .lines()
                    .map(|line| format!("{QUARANTINE_BAR} {}", printable(line))),
            );
        }
    }
    if list.refused > 0 {
        lines.push(t!(mcp_tools_not_listed, count = list.refused));
    }
    lines.push(t!(mcp_tools_explained).to_string());
    lines
}

/// One call to an MCP server's tool.
pub struct Call<'a> {
    /// `alias:tool`, the name a person reads and a rule matches.
    pub name: &'a str,
    /// Each argument's name and its value, as the planner wrote it.
    pub arguments: &'a [(String, String)],
    /// What the server says the tool does.
    pub description: Option<&'a str>,
}

/// The lines a call to an MCP server's tool is read before approving.
pub fn call_lines(call: &Call<'_>) -> Vec<String> {
    let mut lines = vec![format!("{} {}", printable(call.name), t!(mcp_call_kind))];
    match call.arguments.is_empty() {
        true => lines.push(format!("  {}", t!(mcp_call_no_arguments))),
        false => lines.extend(
            call.arguments
                .iter()
                .map(|(name, value)| format!("  {}: {}", printable(name), printable(value))),
        ),
    }
    if let Some(description) = call.description {
        lines.extend(quarantined(description));
    }
    lines
}

/// A remote MCP server whose reply pointed somewhere it is not declared.
pub struct Move<'a> {
    /// The server's alias.
    pub alias: &'a str,
    /// The url the declaration names now.
    pub declared: &'a str,
    /// Where the reply pointed.
    pub destination: &'a str,
    /// The host and port the destination reaches.
    pub authority: &'a str,
    /// A yes is written into the declarations, where it is not it lasts for this session only.
    pub may_record: bool,
}

/// The url a yes declares, pictured like every other word a server wrote, under the one the
/// declaration names now and above the host and port it reaches.
pub fn move_lines(moved: &Move<'_>) -> Vec<String> {
    let mut lines = vec![
        t!(
            mcp_move_declared,
            alias = printable(moved.alias),
            url = printable(moved.declared)
        ),
        t!(mcp_move_destination, url = printable(moved.destination)),
        t!(mcp_move_reaching, authority = printable(moved.authority)),
        t!(mcp_move_explained).to_string(),
    ];
    if !moved.may_record {
        lines.push(t!(mcp_move_this_session_only).to_string());
    }
    lines
}

/// The plan before anything has run.
///
/// The steps are the driver's own rendering rather than somebody else's bytes, and the task is the
/// person's own words, so neither is behind a margin. They are still pictured: a control character
/// reaching a terminal from any direction is a cursor somewhere else.
pub fn manifest_lines(task: &str, steps: &[String]) -> Vec<String> {
    let mut lines = vec![printable(task)];
    for step in steps {
        lines.push(format!("  {}", printable(step)));
    }
    for sentence in [
        t!(plan_explained),
        t!(plan_not_its_writes),
        t!(plan_nothing_yet),
    ] {
        lines.push(sentence.to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_that_names_no_check_reassures_least() {
        assert_eq!(Check::from_word("safe"), Check::Safe);
        assert_eq!(Check::from_word("unsafe"), Check::Unsafe);
        assert_eq!(Check::from_word("inconclusive"), Check::Inconclusive);
        assert_eq!(Check::from_word("SAFE"), Check::Inconclusive);
        assert_eq!(Check::from_word(""), Check::Inconclusive);
    }

    #[test]
    fn a_run_shows_each_step_its_binary_what_it_confines_and_each_access_it_reaches() {
        let steps = [RunStep {
            written: "docker ps".to_string(),
            binary: "/usr/bin/docker".to_string(),
        }];
        let confinement = Confinement {
            heading: "confined to".to_string(),
            directories: vec!["/w".to_string()],
            sentences: vec!["it cannot reach the network".to_string()],
        };
        let text = run_lines(&Run {
            summary: "list containers",
            steps: &steps,
            writes: &["out.txt".to_string()],
            confinement: Some(&confinement),
            ambient: &["it reaches the container daemon".to_string()],
            releases_private: false,
        })
        .join("\n");
        for part in [
            "list containers",
            "docker ps",
            "  /usr/bin/docker",
            "out.txt",
            "confined to",
            "/w",
            "it cannot reach the network",
            "it reaches the container daemon",
        ] {
            assert!(text.contains(part), "`{part}` missing from {text}");
        }
    }

    fn write<'a>(changes: &'a [Change]) -> Write<'a> {
        Write {
            untrusted: false,
            remark: None,
            credentials: &[],
            written_since_checkout: false,
            line_endings: None,
            exact: true,
            added: 0,
            removed: 0,
            changes,
        }
    }

    #[test]
    fn each_kind_of_change_has_its_own_marker_and_an_escape_is_pictured() {
        let changes = [
            Change::Kept("same".to_string()),
            Change::Removed("old".to_string()),
            Change::Added("new\u{1b}[2J".to_string()),
            Change::Elided(3),
        ];
        assert_eq!(
            write_lines(&write(&changes)),
            [
                "  same".to_string(),
                "- old".to_string(),
                "+ new\u{241b}[2J".to_string(),
                t!(write_unchanged, count = 3).to_string(),
            ]
        );
    }

    #[test]
    fn a_body_nobody_vouched_for_says_so_before_the_change() {
        let changes = [Change::Added("body".to_string())];
        let trusted = write_lines(&write(&changes));
        let untrusted = write_lines(&Write {
            untrusted: true,
            ..write(&changes)
        });
        assert_eq!(trusted, ["+ body".to_string()]);
        assert_eq!(
            untrusted,
            [t!(write_untrusted).to_string(), "+ body".to_string()]
        );
    }

    #[test]
    fn a_change_longer_than_the_cap_counts_what_it_leaves_out() {
        let changes: Vec<Change> = (0..MOST_CONTENT_LINES + 7)
            .map(|n| Change::Added(format!("line {n}")))
            .collect();
        let lines = write_lines(&write(&changes));
        assert_eq!(lines.len(), MOST_CONTENT_LINES + 1);
        assert_eq!(lines[MOST_CONTENT_LINES - 1], "+ line 39");
        assert_eq!(
            lines[MOST_CONTENT_LINES],
            t!(transcript_more_lines, count = 7).to_string()
        );
    }

    #[test]
    fn a_change_too_large_to_diff_shows_no_guess_at_it() {
        let changes = [Change::Added("a guess".to_string())];
        let lines = write_lines(&Write {
            exact: false,
            added: 90,
            removed: 80,
            ..write(&changes)
        });
        assert_eq!(
            lines,
            [t!(write_too_large_to_show, added = 90usize, removed = 80usize).to_string()]
        );
    }

    #[test]
    fn what_a_processor_said_sits_behind_the_margin_and_its_escapes_are_pictured() {
        let remark = ["fixed a typo\u{1b}[31m".to_string()];
        let credentials = ["API_KEY at line 2 (sk-\u{2026})".to_string()];
        let lines = write_lines(&Write {
            remark: Some(&remark),
            credentials: &credentials,
            written_since_checkout: true,
            line_endings: Some("line endings: LF kept"),
            ..write(&[])
        });
        assert_eq!(
            lines,
            [
                t!(write_remark).to_string(),
                format!("{QUARANTINE_BAR} fixed a typo\u{241b}[31m"),
                t!(write_credentials).to_string(),
                credentials[0].clone(),
                t!(write_since_checkout).to_string(),
                "line endings: LF kept".to_string(),
            ]
        );
    }
}
