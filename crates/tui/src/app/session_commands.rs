//! The commands that change one setting of a session, or its name and place in the saved records:
//! the pickers, `/effort`, `/advisor`, `/limit`, `/style`, `/theme`, `/rename`, `/branch`, `/link`.

use super::*;

/// Open the theme picker and persist what the person chose.
pub(super) fn choose_theme(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) {
    let current = crate::theme::name();
    let themes = crate::theme::listed(&current);
    if let Some(chosen) = crate::theme_prompt::choose(terminal, themes, &current, |frame| {
        render::draw(frame, session);
    }) {
        theme_picked(session, &chosen.name);
    }
}

/// Persist the theme the person chose, which the picker has already put in force.
pub(super) fn theme_picked(session: &mut Session, name: &str) {
    bravebot_session::store::save_theme(name);
    session.note(t!(session_theme_set, theme = name));
}

/// Open the theme picker as a mode of the session, for a turn that is running.
pub(super) fn open_theme_picker(session: &mut Session) {
    let current = crate::theme::name();
    let themes = crate::theme::listed(&current);
    session.open_picking(crate::picking::Picking::Theme(crate::theme_prompt::opened(
        themes, &current,
    )));
}

/// Open the effort picker and keep what the person chose.
pub(super) fn choose_effort(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) {
    if let Some(row) = crate::effort_prompt::choose(terminal, session.effort(), |frame| {
        render::draw(frame, session);
    }) {
        effort_picked(session, row.0);
    }
}

/// Keep the level the person chose, and say so.
pub(super) fn effort_picked(
    session: &mut Session,
    level: Option<bravebot_aichat::protocol::Effort>,
) {
    session.choose_effort(level);
    session.note(said_of(level));
    say_if_unread(session);
}

/// Open the effort picker as a mode of the session, for a turn that is running.
pub(super) fn open_effort_picker(session: &mut Session) {
    session.open_picking(crate::picking::Picking::Effort(
        crate::effort_prompt::Picker::new(session.effort()),
    ));
}

/// Open the panel of preferences and keep what the person chose.
pub(super) fn choose_editing(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
) {
    if let Some(row) = crate::config_prompt::choose(terminal, session.editing(), |frame| {
        render::draw(frame, session);
    }) {
        editing_picked(session, row.0);
    }
}

/// Keep the style of editing the person chose, and say so.
pub(super) fn editing_picked(session: &mut Session, editing: crate::vim::Editing) {
    session.choose_editing(editing);
    // Said out loud, because the choice changes what the next keystroke does and the box gives no
    // other sign of it until a letter has already gone somewhere unexpected. The mode beneath the
    // box says which vi mode is in force, and this says the style was chosen at all.
    session.note(match editing {
        crate::vim::Editing::Vi => t!(session_editing_vi),
        crate::vim::Editing::Ordinary => t!(session_editing_ordinary),
    });
}

/// Open the panel of preferences as a mode of the session, for a turn that is running.
pub(super) fn open_editing_picker(session: &mut Session) {
    session.open_picking(crate::picking::Picking::Editing(
        crate::config_prompt::Picker::new(session.editing()),
    ));
}

/// Say that the model in force reads no level, where one has just been asked for.
///
/// The choice is kept either way: a person may be about to change model, and throwing away what
/// they just picked would make the two commands depend on the order they were typed in.
pub(super) fn say_if_unread(session: &mut Session) {
    if session.effort().is_some() && !session.model_reads_effort() {
        session.note(t!(session_effort_not_read));
    }
}

/// Take a level by name without opening the picker.
///
/// A word this program does not know changes nothing and says so. It must not reach a request
/// field, and silently ignoring it would leave somebody believing they had asked for something.
pub(super) fn set_effort(session: &mut Session, word: &str) {
    match bravebot_aichat::protocol::Effort::named(word) {
        Some(level) => effort_picked(session, Some(level)),
        None => session.note(t!(session_no_such_effort, effort = word)),
    }
}

/// Name the model the planner may consult, say which it may, or drop the choice.
///
/// The name is held to what `--advisor` holds it to: resolved as every route to a model resolves,
/// then refused where the machine-level layer refuses it or nothing is configured to answer it
/// (ADVISOR-9). A name that changes nothing says why, since the next turn would otherwise offer a
/// tool whose every call fails.
pub(super) fn set_advisor(session: &mut Session, config: &Config, word: &str) {
    let word = word.trim();
    if word.is_empty() {
        let note = match session
            .advisor()
            .map(str::to_string)
            .or_else(|| config.advisor())
        {
            Some(model) => t!(session_advisor_in_force, model = model),
            None => t!(session_advisor_none).to_string(),
        };
        session.note(note);
        return;
    }
    if word == ADVISOR_OFF {
        session.choose_advisor(None);
        session.note(match config.advisor() {
            Some(model) => t!(session_advisor_dropped_setting_remains, model = model),
            None => t!(session_advisor_dropped).to_string(),
        });
        return;
    }
    let model = config.model_named(word);
    match bravebot_agent::backend::serving(config, &Egress::new(), &model) {
        bravebot_agent::backend::Serving::Refused { file, why } => {
            session.note(t!(
                managed_model_refused,
                model = model,
                reason = bravebot_agent::backend::refusal_reason(&file, why)
            ));
        }
        bravebot_agent::backend::Serving::NothingConfigured { .. } => {
            session.note(t!(session_advisor_nothing_serves, model = model));
        }
        bravebot_agent::backend::Serving::Configured => {
            if bravebot_agent::backend::Backend::needs_sign_in(config, &model) {
                session.note(t!(session_advisor_needs_sign_in, model = model));
            } else {
                session.note(t!(session_advisor_set, model = model.as_str()));
                session.choose_advisor(Some(model));
            }
        }
    }
}

/// What the session has spent, counted in `unit`.
pub(super) fn spent_in(session: &Session, unit: bravebot_config::limit::Unit) -> u64 {
    bravebot_agent::spend_limit::spent_in(unit, session.spend_limit(), session.spent_tokens())
}

/// Say what the session's spend limit is, set it from a figure, or remove it.
///
/// The figure is read as a count, of tokens unless it says credits, and never sent anywhere. A
/// limit already below what the session has spent is accepted, and the note says the next request
/// will ask about it.
pub(super) fn set_limit(session: &mut Session, figure: &str) {
    let figure = figure.trim();
    if figure.is_empty() {
        let note = match session.spend_limit().limit() {
            Some(limit) => t!(
                session_limit_in_force,
                limit = limit.figure,
                unit = bravebot_agent::spend_limit::noun(limit.unit),
                spent = spent_in(session, limit.unit)
            ),
            None => t!(session_limit_none).to_string(),
        };
        session.note(note);
    } else if figure == LIMIT_OFF {
        session.spend_limit().set(None);
        session.note(t!(session_limit_cleared));
    } else if let Some(limit) =
        bravebot_config::limit::parse(figure, bravebot_config::limit::Unit::Tokens)
    {
        session.spend_limit().set(Some(limit));
        let spent = spent_in(session, limit.unit);
        let unit = bravebot_agent::spend_limit::noun(limit.unit);
        session.note(if limit.figure > spent {
            t!(
                session_limit_set,
                limit = limit.figure,
                unit = unit,
                spent = spent
            )
        } else {
            t!(
                session_limit_set_below_spent,
                limit = limit.figure,
                unit = unit,
                spent = spent
            )
        });
    } else {
        session.note(t!(session_limit_unknown, figure = figure));
    }
}

/// Start the session in the style the settings name, if they name one (INSTR-12).
///
/// Looked up as `/style` looks a name up, so a name that is not a style is said and leaves none in
/// force. Nothing is said for one that is, and a `--system-prompt` run holds it as `/style` does.
pub(super) fn adopt_style(session: &mut Session, named: Option<&str>) {
    adopt_style_in(session, named, bravebot_agent::home::directory().as_deref());
}

/// [`adopt_style`] with the `~/.bravebot` directory named, so a test can give it a scratch one.
pub(super) fn adopt_style_in(
    session: &mut Session,
    named: Option<&str>,
    home: Option<&std::path::Path>,
) {
    let Some(word) = named else {
        return;
    };
    match bravebot_agent::styles::find(home, word) {
        Some(style) => session.choose_style(Some(style)),
        None => {
            let styles = bravebot_agent::styles::available(home).join(", ");
            session.note(t!(session_style_unknown, style = word, styles = styles));
        }
    }
}

/// Say which output style is in force, take one by name, or clear the pick.
///
/// A name is looked up among the styles this build ships and the person's own files under
/// `~/.bravebot/styles`, and is never read from the line into the prompt. The pick lasts for the
/// session and is not recorded.
pub(super) fn set_style(session: &mut Session, word: &str) {
    set_style_in(session, word, bravebot_agent::home::directory().as_deref());
}

/// [`set_style`] with the `~/.bravebot` directory named, so a test can give it a scratch one.
pub(super) fn set_style_in(session: &mut Session, word: &str, home: Option<&std::path::Path>) {
    let word = word.trim();
    let styles = bravebot_agent::styles::available(home).join(", ");
    if word.is_empty() {
        let note = match session.style() {
            Some(style) => t!(
                session_style_in_force,
                style = style.name.as_ref(),
                styles = styles
            ),
            None => t!(session_style_none, styles = styles).to_string(),
        };
        session.note(note);
    } else if word == STYLE_OFF {
        session.choose_style(None);
        session.note(t!(session_style_cleared));
    } else if let Some(style) = bravebot_agent::styles::find(home, word) {
        let name = style.name.to_string();
        session.choose_style(Some(style));
        // `--system-prompt` was given for this run and has the opening, so the pick is held and
        // says it does not show.
        if session.system_prompts().replacing.is_some() {
            session.note(t!(session_style_set_but_replaced, style = name));
        } else {
            session.note(t!(session_style_set, style = name));
        }
    } else {
        session.note(t!(session_style_unknown, style = word, styles = styles));
    }
}

/// What the session says about the level now in force.
pub(super) fn said_of(effort: Option<bravebot_aichat::protocol::Effort>) -> String {
    match effort {
        Some(level) => t!(session_effort_set, effort = level.as_str()),
        None => t!(session_effort_unset).to_string(),
    }
}

/// Call the session what the person typed, and say so.
pub(super) fn rename_session(
    session: &mut Session,
    stored: &mut bravebot_session::sessions::Handle,
    name: &str,
) {
    if name.is_empty() {
        session.note(t!(session_rename_needs_a_name));
    } else if stored.rename(name) {
        // Every point holds the name the session had before it was renamed, the one a turn running
        // now opened among them (SESSION-19). A refused name changes nothing, so it gives up none.
        session.rewind.close();
        session.note(t!(session_renamed, title = stored.title()));
    } else {
        session.note(t!(session_rename_needs_something));
    }
}

/// The record `/resume` is to carry on from, or `None` after saying why there is none.
///
/// An id is read as `--resume` reads one, and a manifest run or a record a running background
/// session holds is refused as it refuses them. With no id the picker is drawn over the session,
/// without the session itself in it, and leaving it keeps the session as it was.
pub(super) fn record_to_resume(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    root: &Path,
    current: &str,
    typed: &str,
) -> Option<Box<bravebot_session::sessions::Record>> {
    let refusal = if typed.is_empty() {
        let others = bravebot_session::sessions::list(root)
            .iter()
            .any(|listed| listed.id != current);
        if !others {
            session.note(t!(session_resume_nothing_else));
            return None;
        }
        let chosen = crate::resume::choose_other(terminal, root, current);
        match chosen {
            crate::resume::Choice::Resume(record) => crate::resume::continuable(record),
            // Leaving the picker, by Escape or by Ctrl-C, stays where the person is.
            crate::resume::Choice::Fresh | crate::resume::Choice::Quit => return None,
        }
    } else {
        crate::resume::named(root, current, typed)
    };
    match refusal {
        Ok(record) => Some(record),
        Err(refused) => {
            session.note(refused.note());
            None
        }
    }
}

/// Copy the session as `--fork` does and carry on in the copy, saying where the original is.
///
/// The copy is made from the original's record, which this leaves as the last turn wrote it, and is
/// then written again from what the session holds, so anything newer than that record is in the
/// copy and not lost to a copy that was only as current as the record. The rewind points go,
/// because a fork inherits none (SESSION-18) and the ones held describe the original's turns. A
/// loop, a goal and the watches are not written down, so none of them carries over (SESSION-18).
/// A session that keeps a checkout is refused before anything is copied, because the copy's record
/// lists none (CHECKOUT-16). What no record keeps is forgotten, so the copy asks again about it as
/// `--resume` of it would; whether a copy was made is what comes back.
pub(super) fn branch_session(
    session: &mut Session,
    stored: &mut bravebot_session::sessions::Handle,
    conversation: &Conversation,
    answers: &mut Answers,
    workspace: &Workspace,
    kept: &[bravebot_agent::workspace::SessionCheckout],
    name: &str,
) -> bool {
    use bravebot_session::sessions::Unbranched;
    if stored.to_resume().is_some() && !kept.is_empty() {
        let ids: Vec<&str> = kept.iter().map(|checkout| checkout.id.as_str()).collect();
        session.note(t!(
            session_branch_keeps_checkouts,
            count = ids.len(),
            ids = ids.join(", ")
        ));
        return false;
    }
    match stored.branch_off(name) {
        Ok(original) => {
            session.rewind.close();
            session.stop_loop();
            session.clear_goal();
            session.stop_watches();
            let title = stored.title().to_string();
            with_standing(
                session,
                conversation,
                &answers.trust,
                &answers.programs,
                workspace,
                |standing| stored.save(&title, standing),
            );
            session.note(t!(
                session_branched,
                title = stored.title(),
                id = original.id,
                directory = original.directory.display().to_string()
            ));
            answers.forget_what_no_record_keeps();
            workspace.forget_reach_and_hosts();
            session.answers.clear();
            true
        }
        Err(refused) => {
            session.note(match refused {
                Unbranched::NothingWritten => t!(session_branch_nothing_written),
                Unbranched::Unwritable => t!(session_branch_unwritable),
                Unbranched::Refused => t!(session_branch_refused),
            });
            false
        }
    }
}

/// Show, set or clear the link of this kind, as the argument to `/issue` or `/pr` asks, and say so.
///
/// A value that is not a link sets nothing and is not repeated back, since it may hold the escape
/// that made it one.
pub(super) fn link_session(
    session: &mut Session,
    stored: &mut bravebot_session::sessions::Handle,
    kind: bravebot_session::sessions::Link,
    argument: &str,
) {
    use bravebot_session::sessions::{Link, Url};
    let argument = argument.trim();
    let held = stored.link(kind).map(str::to_string);
    let said = match (argument, held) {
        ("", Some(url)) => match kind {
            Link::Issue => t!(session_issue_is, url = url),
            Link::PullRequest => t!(session_pull_request_is, url = url),
        },
        ("" | "clear", None) => match kind {
            Link::Issue => t!(session_issue_none).to_string(),
            Link::PullRequest => t!(session_pull_request_none).to_string(),
        },
        ("clear", Some(_)) => {
            stored.set_link(kind, None);
            match kind {
                Link::Issue => t!(session_issue_cleared).to_string(),
                Link::PullRequest => t!(session_pull_request_cleared).to_string(),
            }
        }
        (typed, _) => match Url::read(typed) {
            Some(url) => {
                stored.set_link(kind, Some(url));
                let url = stored.link(kind).unwrap_or_default();
                match kind {
                    Link::Issue => t!(session_issue_set, url = url),
                    Link::PullRequest => t!(session_pull_request_set, url = url),
                }
            }
            None => match kind {
                Link::Issue => t!(session_issue_refused).to_string(),
                Link::PullRequest => t!(session_pull_request_refused).to_string(),
            },
        },
    };
    session.link(stored.link(Link::Issue), stored.link(Link::PullRequest));
    session.note(said);
}

/// Apply a theme by name without opening the picker.
pub(super) fn set_theme(session: &mut Session, name: &str) {
    match crate::theme::find(name) {
        Some(theme) => {
            crate::theme::apply(&theme);
            bravebot_session::store::save_theme(&theme.name);
            session.note(t!(session_theme_set, theme = &theme.name));
        }
        None => session.note(t!(session_no_such_theme, theme = name)),
    }
}
