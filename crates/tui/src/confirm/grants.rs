//! The prompts that grant something to the agent: a fetch, a path, a host, a checkout, an exposure and a manifest.

use bravebot_agent::confirm::{
    ConsultRequest, ExposureRequest, FetchRequest, HostRequest, ManifestRequest, PathRequest,
};
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme;

use super::chrome::{answer_keys, centred, indented, panel, yes_and_no};
use super::pinned::{self, Drawn, Question, Seen};
use super::{Answer, Response, answer_for, ask_plain};

/// Interpret one key press at the plan prompt, or `None` for a key that answers nothing there.
///
/// Escape stops the run here, as Ctrl-C does, instead of declining the plan. A run has no other
/// moment at which Escape means "leave this one effect": a step later the same key cancels the run,
/// and a person pressing it at the plan asked for the same thing (MANIFEST-11). Every other key is
/// read as at any other prompt.
pub(super) fn manifest_answer_for(key: KeyEvent, drawn: &Drawn) -> Option<Response<Answer>> {
    match key.code {
        KeyCode::Esc if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            drawn.take(Some(Response::Answer(Answer::Interrupt)))
        }
        _ => answer_for(key, drawn),
    }
}

/// Ask whether to fetch a URL, blocking until answered.
pub fn ask_fetch<B: Backend>(terminal: &mut Terminal<B>, request: &FetchRequest) -> Answer {
    ask_plain(
        terminal,
        |frame, scroll, seen| draw_fetch(frame, request, scroll, seen),
        answer_for,
    )
}

/// Draw the fetch question.
///
/// The host is drawn on its own line rather than left inside the URL. A person skimming
/// `https://example.com@evil.test/` reads the first name and the request goes to the second, so
/// what they are actually answering about is put where it cannot be misread.
///
/// It is drawn above the URL, in rows the URL cannot take. A userinfo segment can be longer than
/// the box, and a host drawn below it would be pushed off the screen while the keys stayed on it.
pub(super) fn draw_fetch(
    frame: &mut ratatui::Frame,
    request: &FetchRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::ok(), t!(fetch_title));
    let width = inside.width as usize;

    let mut header = indented(
        t!(fetch_host, host = request.host.as_str()),
        Style::default().fg(theme::muted()),
        width,
    );
    // Under the host, so a hop reads as one: the host above is where a server sent the fetch, and
    // this is the one the person approved.
    if let Some(from) = request.redirected_from.as_deref() {
        header.extend(indented(
            t!(fetch_redirected, from = from),
            Style::default().fg(theme::fail()),
            width,
        ));
    }
    // What the host is, where it is the metadata service of the machine this runs on. The
    // address above is what the request reaches and is not what it means: that service asks
    // nothing of whoever opens the socket and answers with the credentials of the role, so a
    // person shown the number alone is being asked about an address.
    if request.ambient_authority().is_some() {
        header.extend(indented(
            t!(fetch_authority_metadata),
            Style::default().fg(theme::fail()),
            width,
        ));
    }
    header.push(Line::raw(""));

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", t!(fetch_verb)),
                Style::default()
                    .fg(theme::ok())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                request.url.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(""),
    ];
    lines.extend(indented(
        t!(fetch_explained),
        Style::default().fg(theme::muted()),
        width,
    ));

    let keys = |answerable| vec![answer_keys(t!(fetch_yes), t!(fetch_no), answerable)];
    let question = Question {
        above: header,
        body: lines,
        deciding: 0,
        opening: 0,
        needed: 1,
        below: Vec::new(),
        keys: &keys,
        picture: None,
    };
    pinned::draw(frame, inside, question, scroll, seen)
}

/// Ask whether programs may reach one more path for the session, blocking until answered.
pub fn ask_path<B: Backend>(terminal: &mut Terminal<B>, request: &PathRequest) -> Answer {
    ask_plain(
        terminal,
        |frame, scroll, seen| draw_path(frame, request, scroll, seen),
        answer_for,
    )
}

/// Draw the question of whether programs may reach a path.
///
/// The path is what a yes grants, so every row it takes has to have been drawn before a yes is
/// taken: a path long enough to scroll could otherwise hide the end of it. The reason is the
/// planner's own text and sits above, pinned, where it cannot push the keys off the screen.
pub(super) fn draw_path(
    frame: &mut ratatui::Frame,
    request: &PathRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::note(), t!(path_title));
    let width = inside.width as usize;
    let muted = Style::default().fg(theme::muted());
    let shown = bravebot_agent::confirm::shown_path(&request.path);

    let access = match request.write {
        true => t!(path_writes, path = shown.as_str()),
        false => t!(path_reads, path = shown.as_str()),
    };
    let mut body = indented(access, Style::default().add_modifier(Modifier::BOLD), width);
    let deciding = body.len();
    body.extend(indented(
        t!(path_why, why = request.why.as_str()),
        muted,
        width,
    ));
    body.push(Line::raw(""));
    body.extend(indented(t!(path_explained), muted, width));
    body.push(Line::raw(""));
    body.extend(indented(t!(path_not_trusted), muted, width));

    let keys = |answerable| vec![answer_keys(t!(path_yes), t!(path_no), answerable)];
    let question = Question::scrolled(body, deciding, 0, &keys);
    pinned::draw(frame, inside, question, scroll, seen)
}

/// Ask whether programs may reach hosts the allowed-hosts list does not cover, for the session,
/// blocking until answered.
pub fn ask_host<B: Backend>(terminal: &mut Terminal<B>, request: &HostRequest) -> Answer {
    ask_plain(
        terminal,
        |frame, scroll, seen| draw_host(frame, request, scroll, seen),
        answer_for,
    )
}

/// Draw the question of whether programs may reach the hosts they asked for.
///
/// The hosts are what a yes grants, so each is drawn on a row of its own, bold, before the keys
/// can take an answer. They are names a program chose, validated as host names before they get
/// here, and appear nowhere the planner reads.
pub(super) fn draw_host(
    frame: &mut ratatui::Frame,
    request: &HostRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::note(), t!(host_title));
    let width = inside.width as usize;
    let muted = Style::default().fg(theme::muted());

    let mut body = indented(
        t!(host_asked, count = request.hosts.len()),
        Style::default().add_modifier(Modifier::BOLD),
        width,
    );
    for host in &request.hosts {
        body.extend(indented(
            t!(host_row, host = host.as_str()),
            Style::default().add_modifier(Modifier::BOLD),
            width,
        ));
    }
    let deciding = body.len();
    body.push(Line::raw(""));
    body.extend(indented(t!(host_explained), muted, width));

    let keys = |answerable| vec![answer_keys(t!(host_yes), t!(host_no), answerable)];
    let question = Question::scrolled(body, deciding, 0, &keys);
    pinned::draw(frame, inside, question, scroll, seen)
}

/// Ask whether to send the consult the planner proposed, blocking until answered (CONSULT-12).
///
/// A card carrying a refusal takes no yes: the bot's model cannot run here, so the only answers
/// are to close it or stop the turn (CONSULT-10).
pub fn ask_consult<B: Backend>(terminal: &mut Terminal<B>, request: &ConsultRequest) -> Answer {
    ask_plain(
        terminal,
        |frame, scroll, seen| draw_consult(frame, request, scroll, seen),
        |key, drawn| consult_answer(request, key, drawn),
    )
}

/// What a key means at a consult's card, where a refused card turns a yes into nothing.
pub(super) fn consult_answer(
    request: &ConsultRequest,
    key: KeyEvent,
    drawn: &Drawn,
) -> Option<Response<Answer>> {
    match answer_for(key, drawn) {
        Some(Response::Answer(Answer::Approve)) if request.refused.is_some() => None,
        other => other,
    }
}

/// Draw the card for a consult the planner proposed.
///
/// Which bot, the model it answers on and its bound are pinned above, where a long brief cannot
/// push them off the screen (CONSULT-10). The brief is what a yes sends, so every row of it has to
/// have been drawn before a yes is taken.
pub(super) fn draw_consult(
    frame: &mut ratatui::Frame,
    request: &ConsultRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(
        frame,
        area,
        theme::note(),
        &t!(consult_title, bot = request.bot.as_str()),
    );
    let width = inside.width as usize;
    let muted = Style::default().fg(theme::muted());
    let bold = Style::default().add_modifier(Modifier::BOLD);

    let mut header = indented(
        t!(
            consult_proposed,
            bot = request.bot.as_str(),
            definition = request.definition.as_str()
        ),
        bold,
        width,
    );
    let rounds = request.rounds.to_string();
    header.extend(indented(
        match request.budget {
            Some(tokens) => t!(
                consult_bound_with_budget,
                model = request.model.as_str(),
                rounds = rounds,
                tokens = tokens.to_string()
            ),
            None => t!(
                consult_bound,
                model = request.model.as_str(),
                rounds = rounds
            ),
        },
        muted,
        width,
    ));
    if let Some(refused) = &request.refused {
        let failing = Style::default()
            .fg(theme::fail())
            .add_modifier(Modifier::BOLD);
        header.extend(indented(refused.clone(), failing, width));
    }
    header.push(Line::raw(""));

    let mut body = indented(
        t!(consult_brief_labelled, label = request.label.to_string()),
        muted,
        width,
    );
    for line in request.brief.lines() {
        body.extend(indented(line.to_string(), bold, width));
    }
    let deciding = body.len();
    body.push(Line::raw(""));
    let explained = match request.refused {
        Some(_) => t!(consult_refused_explained, bot = request.bot.as_str()),
        None => t!(consult_explained).to_string(),
    };
    body.extend(indented(explained, muted, width));

    let refused = request.refused.is_some();
    let keys = |answerable| {
        if refused {
            let mut spans = vec![
                Span::styled("  n", super::chrome::refusing()),
                Span::raw(format!(" {}    ", t!(consult_close))),
            ];
            spans.extend(super::chrome::stopping());
            vec![Line::from(spans)]
        } else {
            vec![answer_keys(t!(consult_yes), t!(consult_no), answerable)]
        }
    };
    let question = Question {
        above: header,
        ..Question::scrolled(body, deciding, 0, &keys)
    };
    pinned::draw(frame, inside, question, scroll, seen)
}

/// Ask whether to remove a checkout something was done in, blocking until answered (CHECKOUT-15).
///
/// Asked with no turn running, so ctrl-c keeps the checkout like a no, and nothing is stopped.
pub fn ask_remove_checkout<B: Backend>(
    terminal: &mut Terminal<B>,
    checkout: &bravebot_agent::workspace::SessionCheckout,
) -> Answer {
    ask_plain(
        terminal,
        |frame, scroll, seen| draw_remove_checkout(frame, checkout, scroll, seen),
        answer_for,
    )
}

/// Draw the question of whether to remove a checkout.
///
/// Which checkout and what removing it deletes are pinned. The names written in it scroll, since a
/// delegate can write any number of files.
pub(super) fn draw_remove_checkout(
    frame: &mut ratatui::Frame,
    checkout: &bravebot_agent::workspace::SessionCheckout,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::note(), t!(remove_checkout_title));
    let width = inside.width as usize;
    let muted = Style::default().fg(theme::muted());

    let id = checkout.id.as_str();
    let mut above = indented(
        t!(
            remove_checkout_which,
            id = id,
            delegate = checkout.delegate.to_string()
        ),
        muted,
        width,
    );
    above.extend(indented(
        bravebot_agent::confirm::shown_path(&checkout.path),
        Style::default().add_modifier(Modifier::BOLD),
        width,
    ));
    above.push(Line::raw(""));

    let mut written = Vec::new();
    for name in &checkout.candidates.named {
        written.extend(indented(
            bravebot_agent::confirm::shown_name(name),
            Style::default(),
            width,
        ));
    }
    let referenced = checkout.candidates.referenced;
    if referenced > 0 {
        written.extend(indented(
            t!(checkouts_referenced, id = id, count = referenced),
            muted,
            width,
        ));
    }
    written.extend(indented(t!(checkouts_unread, id = id), muted, width));

    let mut below = vec![Line::raw("")];
    below.extend(indented(
        t!(remove_checkout_explained),
        Style::default().fg(theme::fail()),
        width,
    ));

    let (yes, no) = (t!(remove_checkout_yes), t!(remove_checkout_no));
    let keys = |answerable| vec![Line::from(yes_and_no(yes, no, answerable))];
    let question = Question {
        above,
        body: written,
        deciding: 0,
        opening: 0,
        needed: 1,
        below,
        keys: &keys,
        picture: None,
    };
    pinned::draw(frame, inside, question, scroll, seen)
}

/// Put a file the scan found a credential in to the person, blocking until answered.
///
/// The one prompt here that is not about an effect. Nothing is written, nothing runs and nothing
/// leaves the machine; what a yes agrees to is that the file's text goes into a model's context,
/// and so to whoever performs inference. CRED-15 is where that is settled.
pub fn ask_exposure<B: Backend>(terminal: &mut Terminal<B>, request: &ExposureRequest) -> Answer {
    // `ask_plain` refuses a terminal that cannot be drawn to. That one cannot show what was found,
    // and sending a credential to a model nobody warned anybody about is the one thing this
    // question cannot mean.
    ask_plain(
        terminal,
        |frame, scroll, seen| draw_exposure(frame, request, scroll, seen),
        answer_for,
    )
}

/// Draw the exposure question, returning what the draw decided for its keys.
///
/// No preview and no margin, which is what makes this different from every other prompt that
/// shows content. There is nothing quarantined here: the file is one the trust map already covers,
/// and the rows under the heading are findings rather than bytes. A finding is a kind, a place and
/// a mask of the value, so drawing one repeats no part of what it describes, and showing the line
/// the key is on would put the key on a screen in order to warn that it was about to be on one.
///
/// Every finding decides the question, so no yes is taken until all of them have been drawn.
pub(super) fn draw_exposure(
    frame: &mut ratatui::Frame,
    request: &ExposureRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::fail(), t!(expose_title));

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", t!(expose_verb)),
                Style::default()
                    .fg(theme::fail())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                bravebot_agent::confirm::shown_name(&request.path),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(""),
    ];
    lines.extend(indented(
        t!(expose_explained),
        Style::default().fg(theme::muted()),
        inside.width as usize,
    ));
    lines.push(Line::raw(""));
    lines.extend(indented(
        t!(expose_found),
        Style::default().fg(theme::muted()),
        inside.width as usize,
    ));
    for finding in &request.credentials {
        lines.extend(indented(
            finding.clone(),
            Style::default().fg(theme::fail()),
            inside.width as usize,
        ));
    }

    let keys = |answerable| vec![answer_keys(t!(expose_yes), t!(expose_no), answerable)];
    pinned::draw(
        frame,
        inside,
        Question::scrolled(lines, usize::MAX, 0, &keys),
        scroll,
        seen,
    )
}

/// Put a whole frozen plan to the person, blocking until answered.
///
/// The only prompt here about a run rather than about one effect, and the only one raised before
/// anything has happened at all. A manifest run fixes every destination while the task string is
/// the only input in existence, so there is no later moment at which any of this could be asked
/// again and nothing a step reads can add to it: what is on the screen is the whole of what will
/// happen. MANIFEST-10 is where that is settled.
pub fn ask_manifest<B: Backend>(terminal: &mut Terminal<B>, request: &ManifestRequest) -> Answer {
    // `ask_plain` refuses a terminal that cannot be drawn to. That one cannot show the plan, and
    // running a program nobody was shown is the one thing this question cannot mean.
    ask_plain(
        terminal,
        |frame, scroll, seen| draw_manifest(frame, request, scroll, seen),
        // Scrolls as well as answers: a plan longer than the box is the one most worth reading
        // before answering, since approving it approves the steps below the fold as well.
        manifest_answer_for,
    )
}

/// Draw the plan, returning what the draw decided for its keys.
///
/// No margin bar down the steps, unlike every other body in this file. The others are somebody
/// else's bytes; this is the driver's own rendering of a program that came from a context holding
/// the task string and the driver's words. A bar here would mark the steps as content nobody may
/// trust, which is the opposite of why they can be shown at all.
///
/// Every step and what a yes settles decide the question, so no yes is taken until all of them
/// have been drawn.
pub(super) fn draw_manifest(
    frame: &mut ratatui::Frame,
    request: &ManifestRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::brand_primary(), t!(plan_title));

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", t!(plan_verb)),
                Style::default()
                    .fg(theme::brand_primary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                t!(plan_steps, count = request.steps.len()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", t!(plan_goal, task = &request.task)),
                Style::default().fg(theme::muted()),
            ),
        ]),
        Line::raw(""),
    ];

    // Every step, never a count of them and never the first few. A person cannot endorse a step
    // they were not shown, and the one that walks off the bottom of the box is as binding as the
    // first: what does not fit is scrolled to.
    for step in &request.steps {
        lines.extend(indented(
            step.clone(),
            Style::default(),
            inside.width as usize,
        ));
    }
    lines.push(Line::raw(""));

    // What a yes settles, then the two things it does not. Each write in the plan is still put to
    // the person as it comes up, and nothing has happened yet, so declining costs nothing.
    for sentence in [
        t!(plan_explained),
        t!(plan_not_its_writes),
        t!(plan_nothing_yet),
    ] {
        lines.extend(indented(
            sentence,
            Style::default().fg(theme::muted()),
            inside.width as usize,
        ));
    }

    let keys = |answerable| vec![answer_keys(t!(plan_yes), t!(plan_no), answerable)];
    pinned::draw(
        frame,
        inside,
        Question::scrolled(lines, usize::MAX, 0, &keys),
        scroll,
        seen,
    )
}
