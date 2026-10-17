//! The MCP prompts: a server, a move, a tool list and a call.

use bravebot_agent::confirm::{
    CallDecision, McpCallRequest, MoveRequest, ServerRequest, ToolListRequest,
};
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::render::marked_rows;
use crate::theme;

use super::chrome::{
    answer_keys, approving, centred, indented, panel, refusing, stopping, verdict_rows,
};
use super::pinned::{self, Drawn, Question, Seen};
use super::{Answer, Next, Response, Step, answer_for, ask_scrolled, next_press};

/// Put the language-server question to the user.
///
/// Its own prompt rather than a run's, because what a yes grants has a different shape: a process
/// that lives for the session rather than one argv that exits. LSP-5 is where that is settled.
pub fn ask_server<B: Backend>(terminal: &mut Terminal<B>, request: &ServerRequest) -> Answer {
    ask_scrolled(
        terminal,
        Answer::Reject,
        |frame, scroll, seen| draw_server(frame, request, scroll, seen),
        // Scrolls as well as answers: the question is a binary, a directory and two sentences, but
        // the binary and the directory can each be longer than the box.
        |key, drawn| answer_for(key, drawn).map(Step::from),
    )
}

/// Draw the language-server question.
///
/// What a person is answering about is what the process will be allowed to do, so the build-tooling
/// sentence is drawn where it cannot be missed rather than left inside "with your own access". A
/// server that only reads says that instead, because the two are genuinely different propositions
/// and a prompt that warned about both would teach the reader to skim.
///
/// All of it decides the question, so no yes is taken until all of it has been drawn.
pub(super) fn draw_server(
    frame: &mut ratatui::Frame,
    request: &ServerRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::ok(), t!(server_title));

    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{} ", t!(server_verb)),
            Style::default()
                .fg(theme::ok())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            request.program.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ])];
    if !request.args.is_empty() {
        lines.push(Line::styled(
            format!(
                "  {}",
                t!(server_arguments, arguments = request.arguments_line())
            ),
            Style::default().fg(theme::muted()),
        ));
    }
    lines.push(Line::styled(
        format!(
            "  {}",
            t!(server_workspace, workspace = request.workspace.as_str())
        ),
        Style::default().fg(theme::muted()),
    ));
    lines.push(Line::raw(""));

    // The consequential half. Drawn in the warning colour where dependency code runs, since that is
    // the part of this question a person could not have inferred from the word "start".
    let (sentence, style) = if request.declared {
        // What a program a person named runs is not known here, so the prompt says that rather than
        // that it runs nothing.
        (
            t!(server_declared_unknown),
            Style::default().fg(theme::note()),
        )
    } else if request.runs_build_tooling {
        (t!(server_build_tooling), Style::default().fg(theme::note()))
    } else {
        (t!(server_reads_only), Style::default().fg(theme::muted()))
    };
    lines.extend(indented(sentence, style, inside.width as usize));
    lines.push(Line::raw(""));
    lines.extend(indented(
        t!(server_explained),
        Style::default().fg(theme::muted()),
        inside.width as usize,
    ));

    let keys = |answerable| vec![answer_keys(t!(server_yes), t!(server_no), answerable)];
    pinned::draw(
        frame,
        inside,
        Question::scrolled(lines, usize::MAX, 0, &keys),
        scroll,
        seen,
    )
}

/// Ask whether a remote MCP server is now where its reply pointed, blocking until answered.
pub fn ask_move<B: Backend>(terminal: &mut Terminal<B>, request: &MoveRequest) -> Answer {
    ask_scrolled(
        terminal,
        Answer::Reject,
        |frame, scroll, seen| draw_move(frame, request, scroll, seen),
        |key, drawn| answer_for(key, drawn).map(Step::from),
    )
}

/// Draw the question of whether a server moved.
///
/// The destination came from the server's reply, so every line goes through the margin that
/// replaces control characters, and the host it reaches is drawn on its own line for the reason
/// the fetch question draws one.
///
/// That host and what a yes does are drawn under the destination in rows it cannot take. The
/// server chose the destination's length, and one longer than the box would otherwise push them
/// off the screen while the keys stayed on it.
pub(super) fn draw_move(
    frame: &mut ratatui::Frame,
    request: &MoveRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::note(), t!(mcp_move_title));
    let width = inside.width as usize;
    let muted = Style::default().fg(theme::muted());

    let declared = indented(
        t!(
            mcp_move_declared,
            alias = request.alias.as_str(),
            url = request.declared.as_str()
        ),
        muted,
        width,
    );
    let destination = indented(
        t!(mcp_move_destination, url = request.destination.as_str()),
        Style::default().add_modifier(Modifier::BOLD),
        width,
    );
    // The url starts no later than the row after those the sentence takes without it, so a draw
    // showing fewer rows can show the sentence and none of the url.
    let sentence = indented(t!(mcp_move_destination, url = ""), Style::default(), width);
    let needed = u16::try_from(sentence.len())
        .unwrap_or(u16::MAX)
        .saturating_add(1);
    let mut below = indented(
        t!(mcp_move_reaching, authority = request.authority.as_str()),
        Style::default().fg(theme::note()),
        width,
    );
    below.push(Line::raw(""));
    below.extend(indented(t!(mcp_move_explained), muted, width));
    if !request.may_record {
        below.push(Line::raw(""));
        below.extend(indented(t!(mcp_move_this_session_only), muted, width));
    }

    let keys = |answerable| vec![answer_keys(t!(mcp_move_yes), t!(mcp_move_no), answerable)];
    let question = Question {
        above: declared,
        body: destination,
        deciding: 0,
        opening: 0,
        needed,
        below,
        keys: &keys,
        picture: None,
    };
    pinned::draw(frame, inside, question, scroll, seen)
}

/// Put the tools an MCP server offers to the person, blocking until answered (SERVERS-8).
///
/// Answered with `1` or `2`, which are the rows it draws, and with `y` and `n` as at every other
/// prompt here. A standing decision like the vouch offer, so the keys are the vouch offer's too.
pub fn ask_tool_list<B: Backend>(terminal: &mut Terminal<B>, request: &ToolListRequest) -> Answer {
    ask_scrolled(
        terminal,
        Answer::Reject,
        |frame, scroll, seen| draw_tool_list(frame, request, scroll, seen),
        |key, drawn| tool_list_answer_for(key, drawn).map(Step::from),
    )
}

/// One key at the tool list, or `None` for a key that answers nothing.
pub(super) fn tool_list_answer_for(key: KeyEvent, drawn: &Drawn) -> Option<Response> {
    if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
        match key.code {
            KeyCode::Char('1') => return drawn.take(Some(Response::Answer(Answer::Approve))),
            KeyCode::Char('2') => return Some(Response::Answer(Answer::Reject)),
            _ => {}
        }
    }
    answer_for(key, drawn)
}

/// Draw the tool list, returning what the draw decided for its keys.
///
/// Every tool is drawn, and every row of every description: a yes promotes exactly this text, so a
/// description cut short here would be words the planner reads that nobody did. The descriptions
/// carry the margin because they are the server's, and the names and arguments do not because the
/// client drew them from an alphabet with nothing in it that can pass for this program's words.
/// For the same reason all of it decides the question, and no yes is taken until all of it has
/// been drawn.
pub(super) fn draw_tool_list(
    frame: &mut ratatui::Frame,
    request: &ToolListRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::ok(), t!(mcp_tools_title));
    let width = inside.width as usize;

    let marked = Style::default().fg(theme::running());
    let margin = Span::styled("┃ ", marked);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme::muted());

    let heading = match request.tools.is_empty() {
        true => t!(mcp_tools_none, alias = &request.alias),
        false => t!(
            mcp_tools_offered,
            alias = &request.alias,
            count = request.tools.len()
        ),
    };
    let mut lines = indented(heading, bold, width);
    lines.push(Line::raw(""));
    lines.extend(verdict_rows(
        request.verdict,
        request.reason.as_ref(),
        &margin,
        width,
    ));
    lines.push(Line::raw(""));
    lines.extend(indented(t!(mcp_tools_explained), muted, width));

    for tool in &request.tools {
        lines.push(Line::raw(""));
        lines.extend(indented(tool.name.clone(), bold, width));
        if !tool.arguments.is_empty() {
            lines.extend(marked_rows(
                &Span::raw("    "),
                &[Span::raw(tool.arguments.join(", "))],
                width,
            ));
        }
        if let Some(description) = &tool.description {
            for line in description.lines() {
                lines.extend(marked_rows(&margin, &[Span::raw(line.to_string())], width));
            }
        }
    }
    if request.refused > 0 {
        lines.push(Line::raw(""));
        lines.extend(indented(
            t!(mcp_tools_not_listed, count = request.refused),
            muted,
            width,
        ));
    }
    if request.withheld > 0 {
        lines.push(Line::raw(""));
        lines.extend(indented(
            t!(mcp_tools_withheld, count = request.withheld),
            muted,
            width,
        ));
    }

    let keys = |answerable: bool| {
        let mut spans = vec![
            Span::styled("  1", approving(theme::ok(), answerable)),
            Span::raw(format!(" {}    ", t!(mcp_tools_yes))),
            Span::styled("2", refusing()),
            Span::raw(format!(" {}    ", t!(mcp_tools_no))),
        ];
        spans.extend(stopping());
        vec![Line::from(spans)]
    };
    pinned::draw(
        frame,
        inside,
        Question::scrolled(lines, usize::MAX, 0, &keys),
        scroll,
        seen,
    )
}

/// What the person did with a call to a server's tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallAnswer {
    Approve,
    /// Call it, and stop asking for this tool in this project.
    ApproveAndStand,
    Reject,
    /// Refuse the call and stop the turn that asked for it.
    Interrupt,
}

impl CallAnswer {
    /// What to tell the waiting turn. Interrupting refuses and records nothing.
    pub fn decision(self) -> CallDecision {
        match self {
            CallAnswer::Approve => CallDecision::approve(),
            CallAnswer::ApproveAndStand => CallDecision::approve_and_stand(),
            CallAnswer::Reject | CallAnswer::Interrupt => CallDecision::reject(),
        }
    }

    /// Whether the turn that asked stops as well as being refused.
    pub fn stops_the_turn(self) -> bool {
        match self {
            CallAnswer::Approve | CallAnswer::ApproveAndStand | CallAnswer::Reject => false,
            CallAnswer::Interrupt => true,
        }
    }
}

/// What a key press did at a call prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CallResponse {
    Answer(CallAnswer),
    Scroll(i16),
    /// Show the whole description, or only its first rows again.
    Expand,
}

/// One key at a call prompt, or `None` for a key that answers nothing.
///
/// `2` is bound only where the request says the answer can be recorded, for the reason `a` is
/// bound only where a run can be remembered: a key granting what the same screen says cannot be
/// granted is worse than an unbound one. The rows keep their numbers either way, so `3` is no
/// wherever it is pressed.
pub(super) fn call_answer_for(
    key: KeyEvent,
    request: &McpCallRequest,
    drawn: &Drawn,
) -> Option<CallResponse> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(CallResponse::Answer(CallAnswer::Interrupt)),
            _ => None,
        };
    }

    let page = drawn.page();
    drawn.take(match key.code {
        KeyCode::Char('1' | 'y' | 'Y') => Some(CallResponse::Answer(CallAnswer::Approve)),
        KeyCode::Char('2') if request.may_stand => {
            Some(CallResponse::Answer(CallAnswer::ApproveAndStand))
        }
        KeyCode::Char('3' | 'n' | 'N') | KeyCode::Esc => {
            Some(CallResponse::Answer(CallAnswer::Reject))
        }
        KeyCode::Char('e' | 'E') => Some(CallResponse::Expand),
        KeyCode::Up | KeyCode::Char('k') => Some(CallResponse::Scroll(-1)),
        KeyCode::Down | KeyCode::Char('j') => Some(CallResponse::Scroll(1)),
        KeyCode::PageUp => Some(CallResponse::Scroll(-page)),
        KeyCode::PageDown => Some(CallResponse::Scroll(page)),
        KeyCode::Home => Some(CallResponse::Scroll(i16::MIN)),
        KeyCode::End => Some(CallResponse::Scroll(i16::MAX)),
        // Enter is deliberately not an approval: it is the key most likely to be pressed out of
        // habit, and this prompt sends the arguments to a server.
        _ => None,
    })
}

impl pinned::Approving for CallResponse {
    fn approves(&self) -> bool {
        match self {
            CallResponse::Answer(answer) => match answer {
                CallAnswer::Approve | CallAnswer::ApproveAndStand => true,
                CallAnswer::Reject | CallAnswer::Interrupt => false,
            },
            CallResponse::Scroll(_) | CallResponse::Expand => false,
        }
    }
}

/// How many rows of a description a call prompt shows before it is expanded.
const DESCRIPTION_ROWS: usize = 2;

/// Put one call to a server's tool to the person, blocking until answered (SERVERS-7).
pub fn ask_mcp_call<B: Backend>(
    terminal: &mut Terminal<B>,
    request: &McpCallRequest,
) -> CallAnswer {
    let mut scroll = 0u16;
    let mut seen = Seen::default();
    let mut expanded = false;
    loop {
        let mut drawn = Drawn::default();
        if terminal
            .draw(|frame| drawn = draw_mcp_call(frame, request, expanded, scroll, &mut seen))
            .is_err()
        {
            return CallAnswer::Reject;
        }

        match next_press() {
            Next::Skip => continue,
            Next::Lost => return CallAnswer::Reject,
            Next::Press(key) => match call_answer_for(key, request, &drawn) {
                Some(CallResponse::Answer(answer)) => return answer,
                Some(CallResponse::Scroll(by)) => scroll = drawn.moved(scroll, by),
                Some(CallResponse::Expand) => expanded = !expanded,
                None => continue,
            },
        }
    }
}

/// Draw a call prompt, returning what the draw decided for its keys.
///
/// The arguments are the planner's own, which it wrote with nothing untrusted in its context, so
/// they are drawn as they are. The description is the server's, the one somebody read on its list,
/// and is behind the margin and cut to its first rows until asked for: the tool is named above it
/// and the arguments are what this call is about.
///
/// The question and its numbered rows are drawn with the keys, so they keep their rows however
/// long the arguments are, and no row that approves is taken until every argument has been drawn.
pub(super) fn draw_mcp_call(
    frame: &mut ratatui::Frame,
    request: &McpCallRequest,
    expanded: bool,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::ok(), t!(mcp_call_title));
    let width = inside.width as usize;

    let bold = Style::default().add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme::muted());
    let margin = Span::styled("┃ ", Style::default().fg(theme::running()));

    let mut lines = marked_rows(
        &Span::raw("  "),
        &[
            Span::styled(request.name(), bold),
            Span::styled(format!("    {}", t!(mcp_call_kind)), muted),
        ],
        width,
    );
    lines.push(Line::raw(""));
    match request.arguments.is_empty() {
        true => lines.extend(indented(t!(mcp_call_no_arguments), muted, width)),
        false => {
            let widest = request
                .arguments
                .iter()
                .map(|(name, _)| name.chars().count())
                .max()
                .unwrap_or(0);
            for (name, value) in &request.arguments {
                lines.extend(marked_rows(
                    &Span::raw("  "),
                    &[Span::raw(format!(
                        "{:<widest$} {value}",
                        format!("{name}:"),
                        widest = widest + 1
                    ))],
                    width,
                ));
            }
        }
    }

    let deciding = lines.len();
    if let Some(description) = &request.description {
        lines.push(Line::raw(""));
        let mut rows: Vec<Line<'static>> = Vec::new();
        for line in description.lines() {
            rows.extend(marked_rows(&margin, &[Span::raw(line.to_string())], width));
        }
        let longer = rows.len() > DESCRIPTION_ROWS;
        if longer && !expanded {
            rows.truncate(DESCRIPTION_ROWS);
        }
        lines.extend(rows);
        if longer {
            let hint = match expanded {
                true => t!(mcp_call_collapse),
                false => t!(mcp_call_expand),
            };
            lines.extend(indented(hint, muted, width));
        }
    }

    let keys = |answerable: bool| {
        let mut rows = indented(t!(mcp_call_question), bold, width);
        // A row that wraps carries on under its text rather than under its number.
        let hanging = Span::raw("     ");
        let option = |number: &'static str, style: Style, text: Span<'static>| {
            let mut rows = marked_rows(&hanging, &[text], width);
            if let Some(margin) = rows.first_mut().and_then(|row| row.spans.first_mut()) {
                *margin = Span::styled(format!("  {number}. "), style);
            }
            rows
        };
        rows.extend(option(
            "1",
            approving(theme::ok(), answerable),
            Span::raw(t!(mcp_call_yes)),
        ));
        let stand = t!(mcp_call_stand, tool = request.name());
        match request.may_stand {
            true => rows.extend(option(
                "2",
                approving(theme::ok(), answerable),
                Span::raw(stand),
            )),
            false => {
                rows.extend(option(
                    "2",
                    approving(theme::muted(), answerable),
                    Span::styled(stand, muted),
                ));
                rows.extend(marked_rows(
                    &hanging,
                    &[Span::styled(t!(mcp_call_cannot_stand), muted)],
                    width,
                ));
            }
        }
        rows.extend(option("3", refusing(), Span::raw(t!(mcp_call_no))));
        rows.push(Line::from(vec![
            Span::styled("  ctrl-c", muted.add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}", t!(stop_the_turn)), muted),
        ]));
        rows
    };
    pinned::draw(
        frame,
        inside,
        Question::scrolled(lines, deciding, 0, &keys),
        scroll,
        seen,
    )
}
