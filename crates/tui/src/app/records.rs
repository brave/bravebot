//! The session's record and the points it can be rewound to: the figures a record is built from,
//! the writes of it, `/undo`, `/rewind` and `/hunks`.

use super::*;

/// The session as it stood before the turn now in flight, for `/undo` to rewind to.
///
/// The turn's own counts come from [`Session::turn_start`] and not from the session in hand: a
/// prompt arrives already pushed onto the transcript and already counted as a turn, so the live
/// figures describe the turn this exists to undo rather than what it replaced.
pub(super) fn rewind_point(
    session: &Session,
    conversation: &Conversation,
    trust: &TrustStore,
    programs: &TrustedPrograms,
    stored: &bravebot_session::sessions::Handle,
) -> bravebot_session::sessions::TurnSnapshot {
    let began = session.turn_start();
    bravebot_session::sessions::TurnSnapshot {
        conversation: conversation.snapshot(),
        turns: began.turns,
        tokens: session.tokens,
        split: session.split(),
        spend: session.spend_by_turn().clone(),
        timing: session.timing_by_turn().clone(),
        cached: session.cached(),
        cached_prompt_tokens: session.cached_prompt_tokens(),
        trust: trust.clone(),
        programs: programs.clone(),
        transcript_len: began.transcript_len,
        title: stored.title().to_string(),
        was_wrote: stored.resumable().is_some(),
    }
}

/// Say what each point a rewind could reach wrote over, most recent first.
///
/// Most recent first because that is the order the numbers run in: `/rewind 1` is the turn that
/// just ended, so a list starting at the far end would have somebody counting rows backwards to
/// find the one they mean. It is also what lets each row carry one turn's paths rather than that
/// turn's and every later turn's: going back to a row puts back every row above it as well,
/// which the heading says, and repeating the same paths down the list would bury the new ones.
///
/// Every path is named rather than counted, for the reason a refused path is: deciding whether
/// to go back is deciding about those files, and a count of them decides nothing.
pub(super) fn list_rewind_points(session: &mut Session) {
    let lines: Vec<String> = session
        .rewind
        .points()
        .iter()
        .rev()
        .enumerate()
        .map(|(back, point)| {
            let turns = back + 1;
            let turn = point.snapshot.turns + 1;
            let asked = bravebot_session::sessions::title_from(&point.prompt);
            if point.backups.is_empty() {
                t!(
                    session_rewind_point_wrote_nothing,
                    turns = turns,
                    turn = turn,
                    asked = asked
                )
            } else {
                let paths = point
                    .backups
                    .iter()
                    .map(|backup| backup.path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                t!(
                    session_rewind_point,
                    turns = turns,
                    turn = turn,
                    asked = asked,
                    paths = paths
                )
            }
        })
        .collect();
    if lines.is_empty() {
        session.note(t!(session_nothing_to_undo));
        return;
    }
    session.note(t!(session_rewind_points));
    for line in lines {
        session.note(line);
    }
}

/// Build what the session amounts to, as a record is written, and hand it to `then`.
///
/// Every write of the session's record takes the same fifteen fields from the same places, so
/// they are named once. `Standing` is `Copy` and borrows temporaries, so it cannot be returned;
/// a closure is how a caller gets to use it.
pub(super) fn with_standing<R>(
    session: &Session,
    conversation: &Conversation,
    trust: &TrustStore,
    programs: &TrustedPrograms,
    workspace: &Workspace,
    then: impl FnOnce(bravebot_session::sessions::Standing<'_>) -> R,
) -> R {
    then(bravebot_session::sessions::Standing {
        history: Some(session.turn_history()),
        conversation: &conversation.snapshot(),
        turns: session.turns,
        tokens: session.tokens,
        split: session.split(),
        spend: session.spend_by_turn(),
        timing: session.timing_by_turn(),
        model: session.served_model(),
        todos: &session.todos_by_turn(),
        asides: session.asides(),
        trust,
        programs,
        directories: &workspace.added_directories(),
        // None, and it stays none however many runs this session starts. Its presence is what
        // makes a record a manifest run, and this record is a conversation that can be resumed;
        // the run has a record of its own where that field is filled.
        manifest: None,
        rewind: session.rewind.points(),
        checkouts: &workspace.session_checkouts(),
        touched: Some(&workspace.touched_record()),
    })
}

/// Write a compaction down now rather than at the end of the next turn: the shortening is the
/// change, and a session that compacted and then slept should resume compacted.
///
/// Nothing where no turn has been had and nothing is on disk, as with closing a directory: the
/// conversation is empty, so nothing was shortened, and a session opened and abandoned should leave
/// no record.
pub(super) fn save_compacted(
    session: &Session,
    stored: &mut bravebot_session::sessions::Handle,
    conversation: &Conversation,
    trust: &TrustStore,
    programs: &TrustedPrograms,
    workspace: &Workspace,
    events: &[Stamped],
) {
    if save_record(session, conversation, stored, trust, programs, workspace) {
        stored.append_audit(session.turns, events);
    }
}

/// Put the session back to where it stood `steps` turns ago, on disk and in the conversation.
///
/// Says what happened either way. A rewind that reported nothing would leave somebody who asked
/// for three turns back and had two believing the tree in front of them is three turns older
/// than it is.
#[expect(
    clippy::too_many_arguments,
    reason = "each is a piece of session state the rewind restores, and the caller holds them as separate locals, so there is no one struct to pass"
)]
pub(super) fn rewind(
    session: &mut Session,
    conversation: &mut Conversation,
    trust: &mut TrustStore,
    programs: &mut TrustedPrograms,
    stored: &mut bravebot_session::sessions::Handle,
    workspace: &Workspace,
    servers: &mut Option<LanguageServers>,
    steps: usize,
) {
    session.rewind.bind_coverage(workspace);
    let Some(point) = session.rewind.take(steps) else {
        // Saying how far back it does go rather than refusing in the abstract, since the next
        // thing the person types is that number.
        match session.rewind.points().len() {
            0 => session.note(t!(session_nothing_to_undo)),
            kept => session.note(t!(session_rewind_goes_no_further, kept = kept)),
        }
        return;
    };
    // The turns a list described are gone, and a turn run again in their place numbers the same. The
    // replies a list of links was read from are gone with them.
    session.hunks_listed = None;
    session.links_listed = None;
    stored.retain_rewind_coverage(&point.coverage);
    let gaps = point.coverage.gaps();
    // Read off the point before anything is removed. The display prompts of the rewound turns go
    // with them (SESSION-19), so after the removal there is nothing left to put back.
    let undone = point.prompt;
    let snapshot = point.snapshot;
    let refused = bravebot_agent::rewind::restore(
        workspace,
        point.backups,
        trust,
        &snapshot.trust,
        servers,
        bravebot_agent::home::directory().as_deref(),
    );
    if !refused.is_empty() {
        session
            .rewind
            .record_gap(bravebot_agent::rewind::CoverageGap::BackupUnavailable);
    }

    *conversation = bravebot_agent::Conversation::restored(snapshot.conversation);
    session.turns = snapshot.turns;
    session.tokens = snapshot.tokens;
    session.restore_spend(snapshot.tokens, snapshot.spend);
    session.restore_split(snapshot.split);
    session.restore_timing(snapshot.timing);
    // With the spend, for the same reason clearing takes it: the figure describes a
    // prompt that is no longer part of what this session sent.
    session.restore_cache(snapshot.cached, snapshot.cached_prompt_tokens);
    session.written = 0;
    session.finished = None;
    *programs = snapshot.programs;

    session.transcript.truncate(snapshot.transcript_len);
    session.drop_agents_beyond_transcript();
    session.end_files_if_nothing_written();
    session.rewind_history();
    stored.truncate_audit(session.turns + 1);
    // The prompt that was undone goes back into the box, so the next thing the person does is
    // edit it and send it again rather than retype it. After the history removal, which is what
    // takes the prompt off the display, and from the point rather than from the display.
    session.return_prompt(&undone);

    if snapshot.turns == 0
        && !snapshot.was_wrote
        && refused.is_empty()
        && gaps.is_empty()
        && *trust == snapshot.trust
    {
        stored.discard_unwritten(&snapshot.title);
    } else {
        with_standing(
            session,
            conversation,
            trust,
            programs,
            workspace,
            |standing| stored.save(&snapshot.title, standing),
        );
    }
    // Where it landed rather than how far it came, because that is the fact a person checks the
    // tree against, and a count of turns is one they would have to do the arithmetic on.
    let turn = snapshot.turns + 1;
    if refused.is_empty() {
        session.note(t!(session_rewound, turn = turn));
    } else {
        // Named rather than counted. A person who has to go and put a file back by
        // hand needs to know which one, and a count sends them looking.
        let paths = refused
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        session.note(t!(session_rewound_partly, turn = turn, paths = paths));
    }
    if !gaps.is_empty() {
        session.note(t!(
            session_rewind_uncovered,
            causes = coverage_causes(&gaps)
        ));
    }
}

/// Carry out `/hunks`: list the hunks of the last turn, or put one back, and keep the audit and
/// the record.
///
/// A write that failed leaves its path distrusted, and a record that kept the map as the last turn
/// ended would trust a half-written file again on a resume, so the record is written again when
/// the map changed. Nothing is appended to the audit when no gate was passed, and nothing is saved
/// where there is no record to keep, as with closing a directory.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the session state `/hunks` reads and writes, and the argument the person typed"
)]
pub(super) fn carry_out_hunks(
    session: &mut Session,
    conversation: &Conversation,
    stored: &mut bravebot_session::sessions::Handle,
    workspace: &Workspace,
    trust: &mut TrustStore,
    programs: &TrustedPrograms,
    servers: &mut Option<LanguageServers>,
    argument: &str,
) {
    let trust_before = trust.clone();
    let mut trail = Trail::new();
    match crate::hunks_command::parse(argument) {
        crate::hunks_command::Asked::List => {
            crate::hunks_command::list(session, workspace, trust, &mut trail)
        }
        crate::hunks_command::Asked::PutBack(number) => crate::hunks_command::put_back(
            session,
            number,
            workspace,
            trust,
            servers,
            bravebot_agent::home::directory().as_deref(),
            &mut trail,
        ),
        crate::hunks_command::Asked::Unreadable => session.note(t!(session_hunks_needs_a_number)),
    }
    if !trail.events().is_empty() {
        stored.append_audit(session.turns, trail.events());
    }
    if *trust != trust_before {
        save_record(session, conversation, stored, trust, programs, workspace);
    }
}

/// Write the session's record now, if there is one to write: nothing where no turn has been had and
/// nothing is on disk, so a session opened and abandoned leaves none. Whether it did is the answer.
fn save_record(
    session: &Session,
    conversation: &Conversation,
    stored: &mut bravebot_session::sessions::Handle,
    trust: &TrustStore,
    programs: &TrustedPrograms,
    workspace: &Workspace,
) -> bool {
    if session.turns == 0 && stored.resumable().is_none() {
        return false;
    }
    let title = stored.title().to_string();
    with_standing(
        session,
        conversation,
        trust,
        programs,
        workspace,
        |standing| stored.save(&title, standing),
    );
    true
}

/// What a rewind point does not account for, worded and joined.
pub(crate) fn coverage_causes(
    gaps: &std::collections::BTreeSet<bravebot_agent::rewind::CoverageGap>,
) -> String {
    use bravebot_agent::rewind::CoverageGap;
    gaps.iter()
        .map(|gap| match gap {
            CoverageGap::Command => t!(session_rewind_cause_command),
            CoverageGap::Hook => t!(session_rewind_cause_hook),
            CoverageGap::Scratch => t!(session_rewind_cause_scratch),
            CoverageGap::LanguageServer => t!(session_rewind_cause_server),
            CoverageGap::Desktop => t!(session_rewind_cause_desktop),
            CoverageGap::Checkout => t!(session_rewind_cause_checkout),
            CoverageGap::BackupUnavailable => t!(session_rewind_cause_backup),
            CoverageGap::Unknown => t!(session_rewind_cause_unknown),
        })
        .collect::<Vec<_>>()
        .join(", ")
}
