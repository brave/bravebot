//! The prompts that grant something to the agent: a fetch, a path, a host, a checkout, an exposure and a manifest.

use bravebot_agent::confirm::{
    ExposureRequest, FetchRequest, HostRequest, ManifestRequest, PathRequest,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confirm::tests::*;
    use crate::confirm::*;
    use bravebot_i18n::t;
    use ratatui::backend::TestBackend;

    fn rendered_fetch(request: &FetchRequest) -> String {
        fetch_screen(request, (160, 24), 0).0.concat()
    }

    fn fetch_screen(request: &FetchRequest, size: (u16, u16), scroll: u16) -> (Vec<String>, Drawn) {
        pinned_screen(size, |frame| {
            draw_fetch(frame, request, scroll, &mut Seen::default())
        })
    }

    fn path_screen(request: &PathRequest, size: (u16, u16), scroll: u16) -> (Vec<String>, Drawn) {
        pinned_screen(size, |frame| {
            draw_path(frame, request, scroll, &mut Seen::default())
        })
    }

    /// SANDBOX-28: the person is shown the path and whether it is read or written, the planner's
    /// reason, how long a yes lasts and that it marks nothing trusted.
    #[test]
    fn a_path_prompt_shows_the_path_the_access_the_reason_and_what_a_yes_does_not_do() {
        let reading = path_screen(&a_path_request(false), (160, 24), 0).0.concat();
        for shown in [
            "programs this session starts may read /opt/toolchain/include",
            "the planner says: the build reads headers there",
            "Every command the planner runs from now until this session ends",
            "The directory is not marked trusted",
            "y Yes, for this session",
        ] {
            assert!(reading.contains(shown), "{shown} is not drawn in {reading}");
        }
        assert!(!reading.contains("read and write"), "{reading}");

        let writing = path_screen(&a_path_request(true), (160, 24), 0).0.concat();
        assert!(
            writing.contains("may read and write /opt/toolchain/include"),
            "{writing}"
        );
    }

    /// SANDBOX-28: a path too long for the box is the thing a yes grants, so a yes is not taken
    /// until every row of it has been drawn.
    #[test]
    fn a_path_longer_than_the_box_takes_no_yes_until_the_end_of_it_has_been_drawn() {
        let request = PathRequest {
            path: std::path::PathBuf::from(format!("/opt/{}/tail", numbered('p', 200).join(" "))),
            write: false,
            why: "headers".to_string(),
        };
        let (rows, drawn) = path_screen(&request, (80, 24), 0);
        assert!(!drawn.answerable(), "{}", rows.join("\n"));
        assert!(
            box_rows(&rows).iter().any(|row| row.contains("p0000")),
            "{}",
            rows.join("\n")
        );
    }

    fn host_screen(hosts: &[&str], size: (u16, u16), scroll: u16) -> (Vec<String>, Drawn) {
        let request = HostRequest {
            hosts: hosts.iter().map(|host| host.to_string()).collect(),
        };
        pinned_screen(size, |frame| {
            draw_host(frame, &request, scroll, &mut Seen::default())
        })
    }

    /// SANDBOX-24: the person is shown how many hosts were asked for, each on a row of its own, how
    /// long a yes lasts and that nothing is written, and the keys. One host is worded as one.
    #[test]
    fn a_host_prompt_shows_each_host_and_what_a_yes_does() {
        let shown = host_screen(&["a.example", "b.example"], (160, 24), 0).0;
        let rows = box_rows(&shown);
        for row in ["a.example", "b.example"] {
            assert!(
                rows.iter().filter(|line| line.contains(row)).count() == 1,
                "{row} is not on a row of its own in {rows:#?}"
            );
        }
        let screen = rows.concat();
        for text in [
            "programs this session started asked for 2 hosts",
            "Every command the planner runs from now until this session ends",
            "Nothing is written to disk",
            "y Yes, for this session",
        ] {
            assert!(screen.contains(text), "{text} is not drawn in {screen}");
        }
        let one = host_screen(&["a.example"], (160, 24), 0).0.concat();
        assert!(
            one.contains("a program this session started asked for a host"),
            "{one}"
        );
    }

    /// SANDBOX-24: the hosts are what a yes grants, so a yes is not taken until every row of them
    /// has been drawn.
    #[test]
    fn a_host_list_longer_than_the_box_takes_no_yes_until_the_end_of_it_has_been_drawn() {
        let names: Vec<String> = numbered('h', 8)
            .into_iter()
            .map(|label| format!("{label}.example.org"))
            .collect();
        let hosts: Vec<&str> = names.iter().map(String::as_str).collect();
        let (rows, drawn) = host_screen(&hosts, (80, 12), 0);
        assert!(!drawn.answerable(), "{}", rows.join("\n"));
    }

    /// The metadata service asks nothing of whoever reaches it and answers with the credentials
    /// of the role the machine runs as, so approving a request to it is granting those. A prompt
    /// showing the address alone is asking a person about a link-local number.
    #[test]
    fn a_fetch_prompt_says_what_the_metadata_service_is() {
        let drawn = rendered_fetch(&FetchRequest {
            url: "http://169.254.169.254/latest/meta-data/iam/security-credentials/".to_string(),
            host: "169.254.169.254".to_string(),
            redirected_from: None,
            classified: Default::default(),
        });
        assert!(drawn.contains("metadata service"), "{drawn}");
        assert!(drawn.contains("credentials of the role"), "{drawn}");
    }

    /// FETCH-4: a host a redirect led to is drawn as one, with the host that was approved beside it,
    /// and a fetch that was not redirected says nothing of the sort.
    #[test]
    fn a_fetch_prompt_for_a_redirect_says_where_it_was_approved_for() {
        let drawn = rendered_fetch(&FetchRequest {
            url: "https://docs.example.com/start".to_string(),
            host: "cdn.example.net".to_string(),
            redirected_from: Some("docs.example.com".to_string()),
            classified: Default::default(),
        });
        assert!(drawn.contains("talking to cdn.example.net"), "{drawn}");
        assert!(
            drawn.contains("redirected from docs.example.com"),
            "{drawn}"
        );

        let plain = rendered_fetch(&FetchRequest {
            url: "https://docs.example.com/start".to_string(),
            host: "docs.example.com".to_string(),
            redirected_from: None,
            classified: Default::default(),
        });
        assert!(!plain.contains("redirected"), "{plain}");
    }

    /// An ordinary host is an ordinary question, and nothing of the kind is said about it.
    #[test]
    fn a_fetch_prompt_says_nothing_of_the_sort_about_an_ordinary_host() {
        let drawn = rendered_fetch(&FetchRequest {
            url: "https://example.test/page".to_string(),
            host: "example.test".to_string(),
            redirected_from: None,
            classified: Default::default(),
        });
        assert!(!drawn.contains("metadata service"), "{drawn}");
    }

    /// FETCH-2: the host is what a person is answering about, so a URL that wraps past the bottom
    /// of the box scrolls under it rather than pushing it off, and the box says how much is below.
    /// Asked at widths where the keys leave too little of their row for that to fit beside them.
    #[test]
    fn a_url_longer_than_the_fetch_box_keeps_the_host_above_it_and_says_how_much_is_below() {
        let request = FetchRequest {
            url: format!("https://docs.example.invalid/{}", "section/".repeat(250)),
            host: "docs.example.invalid".to_string(),
            redirected_from: None,
            classified: Default::default(),
        };
        for width in [56, 64, 80] {
            let (rows, drawn) = fetch_screen(&request, (width, 24), 0);
            let screen = rows.join("\n");

            let inside = box_rows(&rows);
            let host = t!(fetch_host, host = "docs.example.invalid").to_string();
            let host_row = inside.iter().position(|row| row.contains(&host));
            let url_row = inside
                .iter()
                .position(|row| row.trim_start().starts_with(t!(fetch_verb)));
            assert!(
                matches!((host_row, url_row), (Some(host), Some(url)) if host < url),
                "{width} columns: the host is not drawn above the URL: {screen}"
            );
            assert!(drawn.furthest() > 0, "a URL of 2,000 characters fitted");
            assert!(
                box_words(&rows).contains(&keys_words(t!(fetch_yes), t!(fetch_no))),
                "{width} columns: the keys were pushed off: {screen}"
            );
            assert!(
                inside
                    .iter()
                    .any(|row| row.contains(scroll_hint(drawn.furthest()).trim())),
                "{width} columns: the box does not say how much of the URL is below: {screen}"
            );
            assert!(drawn.answerable(), "{width} columns: {screen}");
        }
    }

    /// FETCH-2: a userinfo segment is the part of a URL that names a site the request does not
    /// reach. One long enough to fill the box leaves the address it does reach on the screen, and
    /// the sentence saying that address is the metadata service, and the keys under them.
    #[test]
    fn a_userinfo_that_fills_the_fetch_box_leaves_the_host_and_what_it_is_on_screen() {
        let (rows, drawn) = fetch_screen(&a_metadata_fetch_behind_a_long_userinfo(), (80, 24), 0);
        let screen = rows.join("\n");
        let shown = box_words(&rows);

        assert!(
            shown.contains(&words(
                &t!(fetch_host, host = "169.254.169.254").to_string()
            )),
            "the host the request reaches is not on the screen: {screen}"
        );
        assert!(
            shown.contains(&words(t!(fetch_authority_metadata))),
            "the metadata warning is not on the screen: {screen}"
        );
        assert!(
            shown.contains(&keys_words(t!(fetch_yes), t!(fetch_no))),
            "{screen}"
        );
        assert!(drawn.answerable(), "{screen}");
    }

    /// FETCH-2: the rest of the URL is reachable by the arrows, and the host stays where it was
    /// while it is read.
    #[test]
    fn the_end_of_a_long_url_can_be_scrolled_to_with_the_host_still_shown() {
        let request = a_metadata_fetch_behind_a_long_userinfo();
        let end = "@169.254.169.254/latest/meta-data/";
        let host = words(&t!(fetch_host, host = "169.254.169.254").to_string());

        let (top, drawn) = fetch_screen(&request, (80, 24), 0);
        assert!(
            !box_rows(&top).concat().contains(end),
            "the whole URL fitted, so nothing here scrolls"
        );

        let (rows, _) = fetch_screen(&request, (80, 24), drawn.furthest());
        let screen = rows.join("\n");
        assert!(
            box_rows(&rows).concat().contains(end),
            "scrolling to the end did not reach the end of the URL: {screen}"
        );
        assert!(box_words(&rows).contains(&host), "{screen}");
    }

    /// FETCH-2: `y` is not taken from a draw that cut off the host, what it is, the keys, or the
    /// URL, whatever the size of the terminal; `n` is taken from any draw. Keys wider than the box
    /// wrap rather than costing the question its yes.
    #[test]
    fn a_fetch_question_takes_a_yes_only_from_a_draw_showing_the_host_and_the_keys() {
        let request = a_metadata_fetch_behind_a_long_userinfo();
        let host = words(&t!(fetch_host, host = "169.254.169.254").to_string());
        let warning = words(t!(fetch_authority_metadata));

        for width in [1, 2, 24, 40, 50, 56, 60, 64, 80, 160] {
            for height in 1..=30 {
                let (rows, drawn) = fetch_screen(&request, (width, height), 0);
                if !drawn.answerable() {
                    continue;
                }
                let screen = rows.join("\n");
                let shown = box_words(&rows);
                assert!(shown.contains(&host), "{width}x{height}: {screen}");
                assert!(shown.contains(&warning), "{width}x{height}: {screen}");
                assert!(
                    shown.contains(&keys_words(t!(fetch_yes), t!(fetch_no))),
                    "{width}x{height}: {screen}"
                );
                assert!(
                    box_rows(&rows)
                        .iter()
                        .any(|row| row.trim_start().starts_with(t!(fetch_verb))),
                    "{width}x{height}: {screen}"
                );
            }
        }
        for size in [(80, 3), (80, 4), (80, 10), (24, 6), (2, 40), (1, 1)] {
            assert!(
                !fetch_screen(&request, size, 0).1.answerable(),
                "{size:?} took a yes"
            );
        }
        for size in [(80, 24), (50, 24)] {
            assert!(
                fetch_screen(&request, size, 0).1.answerable(),
                "{size:?} refused a yes"
            );
        }

        let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let (_, cut_off) = fetch_screen(&request, (80, 4), 0);
        let (_, whole) = fetch_screen(&request, (80, 24), 0);
        assert_eq!(answer_for(y, &cut_off), None);
        assert_eq!(
            answer_for(n, &cut_off),
            Some(Response::Answer(Answer::Reject))
        );
        assert_eq!(
            answer_for(y, &whole),
            Some(Response::Answer(Answer::Approve))
        );
    }

    /// FETCH-2: a scroll moves the URL from where the last draw put it. A terminal made taller
    /// since the last key leaves the stored position past the bottom, and Up has to move it.
    #[test]
    fn a_fetch_scroll_moves_from_where_the_url_was_drawn() {
        let request = a_metadata_fetch_behind_a_long_userinfo();
        let (_, short) = fetch_screen(&request, (80, 16), 0);
        let (_, tall) = fetch_screen(&request, (80, 30), 0);
        assert!(short.furthest() > tall.furthest() && tall.furthest() > 0);

        let scrolled_to_the_end = short.furthest();
        assert_eq!(
            tall.moved(scrolled_to_the_end, -1),
            tall.furthest() - 1,
            "Up moved nothing after the terminal grew"
        );
        assert_eq!(tall.moved(0, -1), 0);
        assert_eq!(tall.moved(tall.furthest(), i16::MAX), tall.furthest());
    }

    fn remove_checkout_screen(
        checkout: &bravebot_agent::workspace::SessionCheckout,
        size: (u16, u16),
        scroll: u16,
    ) -> (Vec<String>, Drawn) {
        pinned_screen(size, |frame| {
            draw_remove_checkout(frame, checkout, scroll, &mut Seen::default())
        })
    }

    /// What a person answering whether to remove a checkout has to see, squeezed.
    fn remove_checkout_pinned() -> Vec<String> {
        [
            t!(remove_checkout_which, id = "c2", delegate = "d1").to_string(),
            "/state/checkouts/work/c2".to_string(),
            t!(remove_checkout_explained).to_string(),
            format!("y {} n {}", t!(remove_checkout_yes), t!(remove_checkout_no)),
        ]
        .iter()
        .map(|text| squeezed(text))
        .collect()
    }

    /// PROMPT-1: the names the removal would delete are drawn as what they are, so a file called
    /// `note.md ` is not listed as the file called `note.md`.
    #[test]
    fn the_remove_checkout_question_marks_a_name_that_would_draw_as_another() {
        let screen = |name: &str| {
            let mut checkout = a_checkout_forty_files_were_written_in();
            checkout.candidates.named = std::iter::once(name.to_string()).collect();
            remove_checkout_screen(&checkout, (100, 30), 0).0.join("\n")
        };
        let plain = screen("note.md");
        assert!(plain.contains("note.md"), "{plain}");
        for (name, spelled) in UNSEEN_NAMES {
            let drawn = screen(name);
            assert_ne!(drawn, plain, "{name:?}");
            assert!(drawn.contains(spelled), "{name:?}\n{drawn}");
        }
        // The checkout's own directory is drawn in the same question, and named from the project's.
        let mut checkout = a_checkout_forty_files_were_written_in();
        checkout.path = "/state/checkouts/a\u{202e}b/c2".into();
        let drawn = remove_checkout_screen(&checkout, (100, 30), 0).0.join("\n");
        assert!(
            drawn.contains("\"/state/checkouts/a\\u{202e}b/c2\""),
            "{drawn}"
        );
    }

    /// CHECKOUT-15. The question names the checkout, where it is and what removing it deletes, and
    /// the names written in it scroll beneath. No turn runs at rest, so the keys offer none to stop.
    #[test]
    fn the_remove_checkout_question_names_the_checkout_and_what_removing_it_deletes() {
        let checkout = a_checkout_forty_files_were_written_in();
        let (rows, drawn) = remove_checkout_screen(&checkout, (100, 30), 0);
        let screen = rows.join("\n");
        let shown = box_text(&rows);
        for pinned in remove_checkout_pinned() {
            assert!(shown.contains(&pinned), "{pinned}: {screen}");
        }
        assert!(shown.contains("src/00.rs"), "{screen}");
        assert!(!shown.contains("src/39.rs"), "forty names fitted: {screen}");
        assert!(
            !shown.contains(&squeezed(t!(stop_the_turn))),
            "the keys offer to stop a turn: {screen}"
        );
        assert!(!shown.contains("ctrl-c"), "{screen}");
        assert!(drawn.answerable(), "{screen}");

        let (end, _) = remove_checkout_screen(&checkout, (100, 30), drawn.furthest());
        let screen = end.join("\n");
        let shown = box_text(&end);
        for at_the_end in [
            "src/39.rs".to_string(),
            squeezed(&t!(checkouts_referenced, id = "c2", count = 2)),
            squeezed(&t!(checkouts_unread, id = "c2")),
        ] {
            assert!(shown.contains(&at_the_end), "{at_the_end}: {screen}");
        }
        for pinned in remove_checkout_pinned() {
            assert!(shown.contains(&pinned), "{pinned}: {screen}");
        }
    }

    /// CHECKOUT-15. `y` is not taken from a draw that cut off which checkout it is, what removing
    /// it deletes or the keys, whatever the size of the terminal.
    #[test]
    fn a_remove_checkout_question_takes_a_yes_only_from_a_draw_showing_what_it_removes() {
        let checkout = a_checkout_forty_files_were_written_in();
        let pinned = remove_checkout_pinned();
        let mut answered = 0;
        for width in [1, 2, 24, 40, 56, 80, 100] {
            for height in 1..=30 {
                let (rows, drawn) = remove_checkout_screen(&checkout, (width, height), 0);
                if !drawn.answerable() {
                    continue;
                }
                answered += 1;
                let screen = rows.join("\n");
                let shown = box_text(&rows);
                for expected in &pinned {
                    assert!(
                        shown.contains(expected),
                        "{width}x{height}: {expected}: {screen}"
                    );
                }
            }
        }
        assert!(answered > 0, "no draw took a yes");
    }

    /// PROMPT-1: the credential-read prompt draws the file it names the same way.
    #[test]
    fn the_exposure_prompt_marks_an_unreadable_name() {
        let screen = |path: &str| {
            let request = ExposureRequest {
                path: path.into(),
                credentials: vec!["an API key at line 1".into()],
            };
            rows_of(100, 30, |frame| {
                draw_exposure(frame, &request, 0, &mut Seen::default());
            })
            .join("\n")
        };
        let plain = screen("note.md");
        assert!(plain.contains("note.md"), "{plain}");
        for (name, spelled) in UNSEEN_NAMES {
            let drawn = screen(name);
            assert_ne!(drawn, plain, "{name:?}");
            assert!(drawn.contains(spelled), "{name:?}\n{drawn}");
        }
    }

    /// PROMPT-1, SANDBOX-28: the prompt about a directory programs may reach draws the path the
    /// grant is made for, so a path ending in whitespace is not drawn as the one without it.
    #[test]
    fn the_path_prompt_marks_an_unreadable_name() {
        let screen = |path: &str| {
            let mut request = a_path_request(true);
            request.path = std::path::PathBuf::from(path);
            path_screen(&request, (160, 24), 0).0.join("\n")
        };
        let plain = screen("/opt/include");
        assert!(plain.contains("/opt/include"), "{plain}");
        for (name, spelled) in [
            ("/opt/include ", "\"/opt/include\u{2423}\""),
            ("/opt/in\u{202e}clude", "\"/opt/in\\u{202e}clude\""),
        ] {
            let drawn = screen(name);
            assert_ne!(drawn, plain, "{name:?}");
            assert!(drawn.contains(spelled), "{name:?}\n{drawn}");
        }
    }

    /// The question a read raises says what was found and where, and repeats no part of the value.
    ///
    /// A person shown "may the model read .env?" is being asked the question they already answered
    /// at startup, so the finding is the whole of what makes this one different and has to be on
    /// the screen. The value is the one thing that must not be: a prompt that quoted the key would
    /// put it in front of whoever is watching in order to warn that it was about to be in front of
    /// a model.
    ///
    /// No margin bar either, unlike every other prompt here that shows something. There is no
    /// quarantined content on this screen: the file is one the trust map already covers, and the
    /// rows under the heading are the driver's own description of a finding rather than bytes.
    #[test]
    fn the_exposure_prompt_says_what_was_found_and_never_the_value() {
        let request = ExposureRequest {
            path: ".env".into(),
            credentials: vec![
                "an AWS access key id at .env:1 (20 characters, upper and digits, 3f2a1c0d)".into(),
            ],
        };
        let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_exposure(frame, &request, 0, &mut Seen::default());
            })
            .expect("draw");
        let drawn: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(
            drawn.contains(".env"),
            "the prompt did not name the file: {drawn}"
        );
        assert!(
            drawn.contains("an AWS access key id at .env:1"),
            "the prompt did not say what was found or where: {drawn}"
        );
        // The sentence that makes this a different question from the one asked at startup.
        assert!(
            drawn.contains("whoever performs inference"),
            "the prompt did not say what a yes discloses: {drawn}"
        );
        // Both answers are offered: the finding informs and decides nothing.
        assert!(
            drawn.contains("send it anyway") && drawn.contains("keep it back"),
            "{drawn}"
        );
        assert_eq!(
            drawn.matches(BAR).count(),
            0,
            "a finding was drawn behind the margin that marks content nobody vouched for: {drawn}"
        );
    }

    fn rendered_manifest(request: &ManifestRequest) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_manifest(frame, request, 0, &mut Seen::default());
            })
            .expect("draw");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// One answer covers the whole run, so the whole run is what the prompt shows: every step, and
    /// the task they are meant to serve. A count of steps would be a summary of what is at stake
    /// rather than the thing itself.
    #[test]
    fn the_plan_prompt_shows_the_task_and_every_step() {
        let drawn = rendered_manifest(&a_plan(&[
            "1. [fetch] read notes.md into notes",
            "2. [transform] summarise notes into summary",
            "3. [act] write summary to summary.md",
        ]));

        assert!(drawn.contains("tidy the notes"), "{drawn}");
        assert!(drawn.contains("3 steps"), "{drawn}");
        for step in [
            "read notes.md",
            "summarise notes",
            "write summary to summary.md",
        ] {
            assert!(
                drawn.contains(step),
                "the plan did not show {step}: {drawn}"
            );
        }
    }

    /// Both halves a reader would otherwise guess at, and they pull in opposite directions: a yes
    /// here does not carry the writes inside the plan, and a no costs nothing because the run has
    /// touched nothing yet.
    #[test]
    fn the_plan_prompt_says_what_approving_it_does_and_does_not_do() {
        let drawn = rendered_manifest(&a_plan(&["1. [act] write summary to summary.md"]));

        assert!(drawn.contains("not approving its writes"), "{drawn}");
        assert!(
            drawn.contains("nothing has been read or written yet"),
            "{drawn}"
        );
    }

    /// Enter is the key most likely to be pressed out of habit, and at this prompt it would start a
    /// whole program rather than one effect.
    ///
    /// The mapping is the one the write and output prompts read, which is what `ask_manifest` asks.
    /// The second half is about this prompt in particular: the keys it offers are the two answers,
    /// and it offers no third one for a reader to reach for without deciding.
    #[test]
    fn enter_does_not_approve_a_plan() {
        assert_eq!(
            answer_for(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                &Drawn::taking_yes()
            ),
            None
        );

        let drawn = rendered_manifest(&a_plan(&["1. [act] write summary to summary.md"]));
        assert!(drawn.contains("run it"), "{drawn}");
        assert!(drawn.contains("don't"), "{drawn}");
        assert!(
            !drawn.to_lowercase().contains("enter"),
            "the plan prompt offers Enter as an answer: {drawn}"
        );
    }

    /// MANIFEST-11. Escape at the plan prompt stops the run, as it does a step later, so the run is
    /// not written as a record. Read as a plain decline it reaches the session as a `Reject`, which
    /// does not set the cancel token. Saying no with `n` is still a decline that leaves the session
    /// and its run going to the record, and Ctrl-C stops the run as before.
    #[test]
    fn escape_at_the_plan_prompt_stops_the_run_and_n_declines_it() {
        let escape = manifest_answer_for(press(KeyCode::Esc), &Drawn::taking_yes());
        assert_eq!(escape, Some(Response::Answer(Answer::Interrupt)));
        assert!(
            Answer::Interrupt.stops_the_turn(),
            "the interrupt would not set the cancel token"
        );

        let no = manifest_answer_for(press(KeyCode::Char('n')), &Drawn::taking_yes());
        assert_eq!(no, Some(Response::Answer(Answer::Reject)));
        assert!(!Answer::Reject.stops_the_turn());

        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(
            manifest_answer_for(ctrl_c, &Drawn::taking_yes()),
            Some(Response::Answer(Answer::Interrupt))
        );
        assert_eq!(
            manifest_answer_for(press(KeyCode::Char('y')), &Drawn::taking_yes()),
            Some(Response::Answer(Answer::Approve))
        );
        assert_eq!(
            Answer::Interrupt.decision(),
            Decision::Reject,
            "stopping the run approved the plan"
        );
    }

    /// A step below the fold is as binding as the first one, so a plan longer than the box is
    /// scrolled to rather than cut short, and the question stays on screen while it is.
    #[test]
    fn a_long_plan_keeps_the_question_on_screen_and_offers_the_rest() {
        let request = ManifestRequest {
            task: "read everything".into(),
            steps: (0..60)
                .map(|n| format!("{}. [fetch] read file{n}.md into slot{n}", n + 1))
                .collect(),
        };

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let mut furthest = 0;
        terminal
            .draw(|frame| {
                furthest = draw_manifest(frame, &request, 0, &mut Seen::default()).furthest()
            })
            .expect("draw");
        assert!(furthest > 0, "a sixty step plan reported nothing to scroll");

        terminal
            .draw(|frame| {
                draw_manifest(frame, &request, furthest, &mut Seen::default());
            })
            .expect("draw");
        let drawn: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(
            drawn.contains("read file59.md"),
            "the last step could not be reached: {drawn}"
        );
        assert!(drawn.contains("run it"), "the question scrolled away");
    }
}
