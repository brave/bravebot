//! Loops, goals, schedules, file watches and background sessions.

use super::*;

/// Neither half of an aside is in the conversation, so an answer that arrives adds nothing to
/// the turn's own lines: a line about it there is one a reader takes the planner to have had,
/// and the planner has read neither the question nor the answer. The row the answer becomes,
/// and the hint line that counts it, are where an aside is said to have happened.
#[test]
fn answering_a_question_beside_the_work_leaves_the_transcript_alone() {
    let mut session = Session::new("none");
    session.begin_aside();
    let before = session.transcript.len();

    aside_answered(
        &mut session,
        "why is the parser recursive?".to_string(),
        bravebot_agent::aside::Answered {
            shown: "because the grammar nests".to_string(),
            kept: Some("because the grammar nests".to_string()),
            usage: Default::default(),
        },
    );

    assert_eq!(
        session.watched_aside().and_then(|a| a.answer.as_deref()),
        Some("because the grammar nests"),
        "the answer did not reach the row that is the only place it is read"
    );
    let added: Vec<&str> = session.transcript[before..]
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    assert!(
        added.is_empty(),
        "an answered aside put a line among the turn's own: {added:?}"
    );
}

/// The tail is where the planner's own half-written reply is drawn, so an answer arriving
/// there reads as the planner writing something it has not read. There is one model writing
/// at a time in that place, and an aside is not the one.
#[test]
fn an_answer_being_written_beside_the_work_is_not_drawn_over_the_turn() {
    let mut session = Session::new("none");
    session.streaming("the turn was saying this");
    session.begin_aside();
    let before = session.transcript.len();

    aside_reported(
        &mut session,
        crate::remote_confirm::ToMain::Streaming("because the grammar".to_string()),
    );

    assert_eq!(
        session.reply_so_far(),
        "the turn was saying this",
        "the answer reached the tail the turn's own reply is drawn in"
    );
    let added: Vec<&str> = session.transcript[before..]
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    assert!(
        added.is_empty(),
        "the answer was drawn among the turn's own lines: {added:?}"
    );
}

/// The command sends nothing when Enter is pressed: the line is held for later. A wrong
/// implementation submits it at once, or reads a line with no delay as a request.
#[test]
fn the_after_command_holds_the_line_and_sends_nothing_now() {
    let mut session = Session::new("none");
    for c in "/after 5m check the deploy".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(
        session.looping().map(|running| running.prompt()),
        Some("check the deploy")
    );

    let mut bare = Session::new("none");
    for c in "/after check the deploy".chars() {
        handle_key(&mut bare, key(KeyCode::Char(c)));
    }
    assert_eq!(handle_key(&mut bare, key(KeyCode::Enter)), Action::Redraw);
    assert!(bare.looping().is_none(), "a line with no delay was held");
}

/// An interval with nothing after it is a loop somebody asked for and cannot have, so the
/// interface says what the command needs rather than quietly doing nothing. Not read as the
/// bare word either: a person halfway through typing a new interval is not asking about the
/// loop they are replacing.
#[test]
fn an_interval_with_nothing_after_it_says_what_the_command_needs() {
    for line in ["/loop 5m", "/loop every 5m"] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw,
            "{line}"
        );
        assert!(session.looping().is_none(), "{line} started a loop");
        assert_eq!(session.transcript.len(), 1, "{line} said nothing");
        assert_eq!(
            session.transcript[0].text,
            t!(loop_needs_a_prompt),
            "{line}"
        );
    }
}

/// The frame a session at rest is owed, and the ones it is not. A session with nothing active
/// is left alone, because nothing on its screen goes stale by standing still. One with only a
/// watch, only a job or only a loop is owed a frame a second, since the countdown and the
/// light change only if one is drawn. A session drawn inside the last second is left alone
/// too: the rows are spelled no finer than a second and a frame per pass is a spin.
#[test]
fn a_session_is_owed_a_frame_once_a_second_and_only_while_something_is_active() {
    let idle = Session::new("none");
    let mut watching = Session::new("none");
    watching.arm_watch(
        "notes.md",
        "/work",
        bravebot_agent::watch::Looked::Saw("first".to_string()),
    );
    let mut running = Session::new("none");
    running.job(bravebot_agent::report::JobEvent::Started {
        name: "job:1".to_string(),
        line: "sleep 600".to_string(),
        moved_after: None,
        stop: bravebot_core::cancel::JobStop::new(),
    });
    let mut looping = Session::new("none");
    looping.start_loop(
        crate::loops::request("5m check the deploy"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );

    assert!(!a_session_is_owed_a_frame(&idle, Duration::from_secs(9)));
    for session in [&watching, &running, &looping] {
        assert!(session.is_standing());
        assert!(!a_session_is_owed_a_frame(
            session,
            COUNTDOWN - Duration::from_millis(1)
        ));
        assert!(a_session_is_owed_a_frame(session, COUNTDOWN));
        assert!(a_session_is_owed_a_frame(session, Duration::from_secs(9)));
    }
}

/// The frame a face is owed, and the ones it is not. A face held as it was is left alone, and a
/// screen that is not the opening one has no face to repaint, whatever the clock says.
#[test]
fn a_face_is_owed_a_frame_only_on_the_opening_screen_and_only_when_its_look_changed() {
    use crate::avatar::Look;
    let open = Look::default();
    let shut = Look {
        away: false,
        blink: true,
    };
    assert!(a_face_is_owed_a_frame(true, shut, open));
    assert!(a_face_is_owed_a_frame(true, open, shut));
    assert!(!a_face_is_owed_a_frame(true, open, open));
    assert!(!a_face_is_owed_a_frame(false, shut, open));
}

/// The short wait belongs to the opening screen with motion allowed, and to nothing else.
#[test]
fn the_wait_for_a_key_is_short_only_while_a_face_can_move() {
    assert_eq!(wait_for_a_key(true, false), FACE_POLL);
    assert_eq!(wait_for_a_key(true, true), POLL);
    assert_eq!(wait_for_a_key(false, false), POLL);
    assert_eq!(wait_for_a_key(false, true), POLL);
    assert!(FACE_POLL < POLL);
}

/// A held `/after` line is read as sending later, not as repeating (LOOP-18). A wrong
/// implementation answers "repeating: ...".
#[test]
fn the_bare_loop_command_says_an_after_line_is_sending_later() {
    let mut session = Session::new("none");
    for c in "/after 5m tail the log".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    session.transcript.clear();
    for c in LOOP_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));

    assert_eq!(session.transcript.len(), 2, "{:?}", session.transcript);
    assert!(
        session.transcript[0].text.contains("tail the log"),
        "{}",
        session.transcript[0].text
    );
    assert!(
        session.transcript[0].text.starts_with(
            t!(after_active, number = 1, prompt = "", when = "")
                .split(':')
                .next()
                .unwrap()
        ),
        "{}",
        session.transcript[0].text
    );
    assert_eq!(session.transcript[1].text, t!(loop_ends_with));
    assert_eq!(session.loops().len(), 1, "the report ended it");
}

/// The ending a person can reach without also stopping the turn that is in flight, which is
/// what Ctrl-C reaches first: a loop over a long turn is otherwise ended only by cancelling the
/// work it has just started. Asked for at rest it takes effect on the press.
#[test]
fn the_loop_command_ends_the_loop_when_asked_to_stop() {
    let mut session = Session::new("none");
    session.start_loop(
        crate::loops::request("5m check the deploy"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    session.complete("done", Vec::new(), 0);
    for c in "/loop stop".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert!(session.looping().is_none(), "the loop is still running");
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(loop_stopped)),
        "the ending was not announced"
    );
}

/// LOOP-15: `/loop stop 2` typed in the box ends loop 2 and leaves loop 1, and a bare `stop`
/// with both live ends neither.
#[test]
fn the_loop_command_ends_the_loop_it_names() {
    let mut session = Session::new("none");
    for line in ["5m check the deploy", "1h check the pull request"] {
        session.start_loop(
            crate::loops::request(line),
            Vec::new(),
            Vec::new(),
            crate::state::NamesFrom::nothing(),
        );
        session.complete("done", Vec::new(), 0);
        session.loop_turn_ended(None);
    }
    let type_line = |session: &mut Session, line: &str| {
        for c in line.chars() {
            handle_key(session, key(KeyCode::Char(c)));
        }
        assert_eq!(handle_key(session, key(KeyCode::Enter)), Action::Redraw);
    };

    type_line(&mut session, "/loop stop");
    assert_eq!(session.loops().len(), 2, "a bare stop picked a loop");

    type_line(&mut session, "/loop stop 2");
    let left: Vec<usize> = session.loops().iter().map(|r| r.number()).collect();
    assert_eq!(left, [1]);
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(loop_stopped_numbered, number = 2)),
        "the ending was not announced"
    );
}

/// Nothing to stop is worth saying, because the person believed there was something. A command
/// that answered it with silence would leave them unable to tell which of the two it meant.
#[test]
fn asking_to_stop_a_loop_that_is_not_running_says_so() {
    let mut session = Session::new("none");
    for c in "/loop stop".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.transcript.len(), 1);
    assert_eq!(session.transcript[0].text, t!(loop_none));
}

/// The one place in the command where a reserved word could eat a line somebody meant to have
/// repeated. The word ends a loop as the whole argument, and starts one as the first word of a
/// sentence.
#[test]
fn a_loop_over_a_line_beginning_with_stop_is_still_a_loop() {
    let mut session = Session::new("none");
    for c in "/loop stop the deploy if it is still going".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("stop the deploy if it is still going".to_string())
    );
    assert_eq!(
        session.looping().map(|running| running.prompt()),
        Some("stop the deploy if it is still going")
    );
}

/// A goal is a condition, not a prompt. Setting one that started a turn would send a line
/// nobody typed, and there is no line: the argument is what stops the work, not what starts
/// it.
#[test]
fn the_goal_command_sets_a_condition_without_sending_anything() {
    let mut session = Session::new("none");
    for c in "/goal cargo test exits 0".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(
        session.goal().map(crate::goals::Running::condition),
        Some("cargo test exits 0")
    );
}

/// Pause and resume are typed by a person and touch only what the session keeps, so they run
/// where `/goal clear` does, mid-turn, while setting a goal waits.
#[test]
fn the_goal_command_pauses_and_resumes_the_goal_even_mid_turn() {
    let mut session = Session::new("none");
    session.start_goal("cargo test exits 0".to_string());
    for (line, paused) in [("/goal pause", true), ("/goal resume", false)] {
        assert!(runs_while_working(line), "{line} waited for the turn");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Redraw
        );
        assert_eq!(
            session.goal().map(crate::goals::Running::is_paused),
            Some(paused),
            "{line}"
        );
        assert_eq!(
            session.goal().map(crate::goals::Running::condition),
            Some("cargo test exits 0"),
            "{line} changed the condition"
        );
    }
    assert!(!runs_while_working("/goal pause the deploy first"));
}

/// With no condition there is no goal to set, and the interface says what it needs rather
/// than quietly doing nothing.
#[test]
fn the_bare_goal_command_is_still_the_command() {
    let mut session = Session::new("none");
    for c in GOAL_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert!(session.goal().is_none(), "the bare word set a goal");
    assert_eq!(session.transcript.len(), 1, "the bare word said nothing");
}

/// A `@path` is a person vouching for a file with their keystroke. The sentence a goal sends
/// the work back with is this program's, quoting a judge, so a word beginning with `@` in one
/// is prose: reading it as a path would open a file on the say-so of a model, wearing an
/// endorsement nobody gave.
///
/// Through what picks the next line after a turn, since that is where the sentence is marked
/// as the driver's: a test of the reader alone passes with the mark on the wrong side.
#[test]
fn a_path_named_in_a_sentence_the_driver_wrote_vouches_for_nothing() {
    let mut session = Session::new("none");
    session.start_goal("cargo test exits 0".to_string());

    let (carrying_on, wrote) = next_after_a_turn(&mut session, |session| {
        Ok(
            session.goal_judged(Ok(bravebot_agent::goal::Verdict::NotMet {
                reason: "the failure is in @crates/core/src/policy.rs".to_string(),
            })),
        )
    })
    .expect("judging cannot fail here")
    .expect("a goal not met yet sends the work back");

    assert!(
        carrying_on.contains("@crates/core/src/policy.rs"),
        "the reason was not quoted, so nothing here could name a file: {carrying_on}"
    );
    assert!(
        named_by_the_person(&session, wrote, std::path::Path::new("."), &[])
            .files
            .is_empty(),
        "a sentence this program wrote opened a file"
    );
    assert!(
        session.names_of_the_turn().is_nothing(),
        "the turn the driver began kept a line to read names from"
    );
}

/// The other half: a line the person queued goes out before the goal is judged, as theirs, so
/// a `@path` in it still names a file.
#[test]
fn a_path_named_in_a_line_the_person_queued_still_names_a_file() {
    let mut session = Session::new("none");
    session.start_goal("cargo test exits 0".to_string());
    for c in "fix the build".chars() {
        session.type_char(c);
    }
    session.submit();
    for c in "look at @crates/core/src/policy.rs".chars() {
        session.type_char(c);
    }
    assert!(session.queue(), "the line was not queued");
    session.complete("done", Vec::new(), 0);

    let mut judged = false;
    let (_queued, wrote) = next_after_a_turn(&mut session, |_| {
        judged = true;
        Ok(None)
    })
    .expect("judging cannot fail here")
    .expect("the queued line was not sent");

    assert!(!judged, "the goal was judged before the person's line went");
    assert_eq!(
        named_by_the_person(&session, wrote, std::path::Path::new("."), &[]).files,
        vec!["crates/core/src/policy.rs".to_string()],
        "a line the person typed stopped naming its files"
    );
}

/// FSWATCH-4: the sentence a watch's fire begins a turn with is this program's, so the path in
/// it vouches for nothing even where it is spelled `@path`. A fire marked as the person's line
/// would open that file on the say-so of a sentence nobody typed.
///
/// The fire's own session is asked whose line it is, since that choice is made where the
/// prompt is submitted and a test of the reader alone passes with the fire marked as
/// the person's. The `contains` and `referenced` assertions keep the test from passing
/// because the sentence never held a `@path` to read.
#[test]
fn a_path_named_in_a_fires_prompt_vouches_for_nothing() {
    let mut session = Session::new("none");
    session.arm_watch(
        "@crates/core/src/policy.rs",
        ".",
        bravebot_agent::watch::Looked::Saw("first".to_string()),
    );
    let prompt = session
        .watch_fired(Instant::now() + Duration::from_secs(6), |_, _| {
            bravebot_agent::watch::Looked::Saw("second".to_string())
        })
        .expect("a changed look did not fire");
    assert!(session.watch_is_firing(), "the fire was not in flight");

    assert!(
        prompt.contains("@crates/core/src/policy.rs"),
        "the fire did not name the path, so nothing here could name a file: {prompt}"
    );
    assert_eq!(
        bravebot_mentions::referenced(&prompt),
        vec!["crates/core/src/policy.rs".to_string()],
        "the sentence names no file to read, so the test would pass whatever marked it"
    );
    assert_eq!(
        whose_line(&session),
        Wrote::TheDriver,
        "a watch's fire was treated as a line the person typed"
    );
    assert!(
        named_by_the_person(
            &session,
            whose_line(&session),
            std::path::Path::new("."),
            &[]
        )
        .files
        .is_empty(),
        "a path in a fire's sentence opened a file"
    );
    assert!(
        session.names_of_the_turn().is_nothing(),
        "the fire kept a line to read names from"
    );
}

/// The other half: with no fire in flight the line is the person's, and a `@path` in it names
/// a file. A `whose_line` that always answered the driver would pass
/// `a_path_named_in_a_fires_prompt_vouches_for_nothing`.
#[test]
fn a_line_typed_with_no_fire_in_flight_is_the_persons_and_names_its_files() {
    let mut session = Session::new("none");
    let Action::Submit(_) = enter_line(&mut session, "look at @crates/core/src/policy.rs") else {
        panic!("the line was not sent");
    };

    assert_eq!(
        whose_line(&session),
        Wrote::ThePerson,
        "a line typed with no watch firing was treated as this program's"
    );
    assert_eq!(
        named_by_the_person(
            &session,
            whose_line(&session),
            std::path::Path::new("."),
            &[]
        )
        .files,
        vec!["crates/core/src/policy.rs".to_string()],
        "a line the person typed stopped naming its files"
    );
}

/// Leaving ends a goal, so a person who asked to leave during a turn does not wait on a check
/// of it, or have the work carried on, before the session closes.
#[test]
fn a_goal_is_not_judged_after_the_person_asked_to_leave() {
    let mut session = Session::new("none");
    session.start_goal("cargo test exits 0".to_string());
    session.quit();

    let mut judged = false;
    let sending = next_after_a_turn(&mut session, |_| {
        judged = true;
        Ok(Some("carry on".to_string()))
    })
    .expect("judging cannot fail here");

    assert!(
        !judged,
        "the goal was judged after the person asked to leave"
    );
    assert_eq!(
        sending, None,
        "a line went out after the person asked to leave"
    );
}

/// GOAL-10: `/exit` and `/clear` typed while a turn runs end the goal, so the goal does not
/// send the work back before they are carried out.
#[test]
fn a_goal_is_not_judged_while_a_command_waits_in_the_queue() {
    for command in ["/exit", "/clear", "/goal cargo test exits 1", "/goal clear"] {
        let mut session = Session::new("none");
        session.start_goal("cargo test exits 0".to_string());
        for c in "fix the build".chars() {
            session.type_char(c);
        }
        session.submit();
        for c in command.chars() {
            session.type_char(c);
        }
        assert!(session.queue_command(), "{command} was not queued");
        session.complete("done", Vec::new(), 0);

        let mut judged = false;
        let sending = next_after_a_turn(&mut session, |_| {
            judged = true;
            Ok(Some("carry on".to_string()))
        })
        .expect("judging cannot fail here");

        assert!(!judged, "the goal was judged with {command} waiting");
        assert_eq!(sending, None, "work went out with {command} waiting");
        assert!(
            session.take_queued_command().is_some(),
            "{command} was not left for the loop to carry out"
        );
    }
}

/// MANIFEST-9's other half: a session can reach the planning mode, so the word has to start a
/// run and carry the whole task. The task is a sentence, like a goal's condition, so nothing
/// may be cut off it: what the planner is shown is what it plans, and the planner is shown
/// this and nothing else.
#[test]
fn a_session_can_ask_for_a_manifest_run() {
    let mut session = Session::new("none");
    for c in "/manifest summarise every doc under docs/specs".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Manifest(
            "summarise every doc under docs/specs".to_string(),
            Vec::new(),
            Vec::new()
        )
    );
    assert!(
        session.input().is_empty(),
        "the command is still in the box"
    );
}

/// The bare word plans nothing. There is no default task and there could not be one: the task
/// string is the whole of a manifest run's input, so a run started without one would plan
/// against nothing at all.
#[test]
fn a_manifest_run_needs_a_task_to_plan() {
    let mut session = Session::new("none");
    for c in MANIFEST_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Manifest(String::new(), Vec::new(), Vec::new()),
        "the bare word has to reach the loop, which says what it needs"
    );
    assert!(
        t!(manifest_needs_a_task).contains(MANIFEST_COMMAND),
        "the answer does not name the command it is about"
    );
}

/// MANIFEST-11. Escape at the plan prompt is answered as a decline, so a run stopped there
/// comes back saying the plan was not approved rather than saying it was cancelled. Read off
/// the error, the key would mean "leave no record" one moment and "write one" the moment
/// before, for a person who pressed the same key asking for the same thing.
#[test]
fn a_run_the_person_stopped_is_read_off_the_key_and_not_off_the_error() {
    let declined = || {
        Err(bravebot_agent::TurnError::Precommit(
            "the plan was not approved, so nothing ran".to_string(),
        ))
    };
    let cancelled = || Err(bravebot_agent::TurnError::Cancelled { attempts: None });

    let pressed = Cancel::new();
    pressed.cancel();
    assert!(
        was_stopped(&declined(), &pressed),
        "a decline the person's own key produced was not read as a stop"
    );
    assert!(was_stopped(&cancelled(), &pressed));

    let untouched = Cancel::new();
    assert!(
        !was_stopped(&declined(), &untouched),
        "a plan declined at the prompt is a run worth recording"
    );
}

/// A sentence that merely mentions the word is a prompt. Starting a run from one would plan a
/// whole run's worth of effects off a line somebody meant to say.
#[test]
fn a_manifest_run_is_not_a_prompt() {
    for line in [
        "/manifests are the other mode",
        "what does /manifest do",
        "explain the /manifest command",
    ] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }

        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.to_string()),
            "{line} started a run"
        );
    }
}

/// The key that stops things has to stop this one too, or a person watching a goal go wrong
/// has to leave the session to get out of it.
#[test]
fn interrupting_takes_the_goal_off_before_it_leaves() {
    let mut session = Session::new("none");
    session.start_goal("cargo test exits 0".to_string());

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert!(session.goal().is_none());
    assert!(
        !session.is_quitting(),
        "the press that stopped it also left"
    );

    // The rung that leaves offers first, so the press that takes it is a second one.
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
}

/// A turn that failed still ended. A watch left marked as firing would never fire again, and
/// every prompt after it would be read as this program's rather than the person's, which
/// quietly stops `@path` from vouching for anything.
#[test]
fn a_fire_whose_turn_failed_stops_being_the_turn_in_flight() {
    let mut session = Session::new("none");
    session.arm_watch(
        "notes.md",
        "/work",
        bravebot_agent::watch::Looked::Saw("first".to_string()),
    );
    session
        .watch_fired(Instant::now() + Duration::from_secs(6), |_, _| {
            bravebot_agent::watch::Looked::Saw("second".to_string())
        })
        .expect("a fire");
    assert!(session.watch_is_firing());

    fold_outcome(
        &mut session,
        Err(turn::TurnError::Precommit(
            "the backend said no".to_string(),
        )),
        Trail::new(),
        Carried {
            trust: TrustStore::new("/work"),
            programs: TrustedPrograms::new(),
            asked: AskedAbout::new(),
            exposed: bravebot_core::credentials::Exposed::new(),
        },
        Occupied {
            budget: 100_000,
            guessed: false,
            last_request_tokens: 0,
        },
        Asked {
            name: "test-model".to_string(),
            comparable: true,
            configured: "test-model".to_string(),
        },
        Line {
            text: "",
            addressed: None,
            offered_a_later_look: false,
        },
        &workspace_for_test(),
    );

    assert!(!session.watch_is_firing(), "the watch is still firing");
    assert_eq!(
        session.watches().len(),
        1,
        "a failed request ended the watch"
    );
}

/// Watches are the last rung before leaving, for the reason the loop and the goal are rungs:
/// somebody pressing the key that stops things wants the things stopped, and picking which of
/// eight survived is not a decision to make from a keystroke.
#[test]
fn interrupting_ends_every_watch_before_it_leaves() {
    let mut session = Session::new("none");
    session.arm_watch(
        "a.md",
        "/work",
        bravebot_agent::watch::Looked::Saw("first".to_string()),
    );
    session.arm_watch(
        "b.md",
        "/work",
        bravebot_agent::watch::Looked::Saw("first".to_string()),
    );

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert!(session.watches().is_empty());
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(watches_stopped, count = 2)),
        "the watches ended in silence"
    );
    assert!(
        !session.is_quitting(),
        "the press that stopped them also left"
    );

    // The rung that leaves offers first, so the press that takes it is a second one.
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
}

/// Listing and stopping are answered by the session itself. A path goes to the loop, which
/// owns what the first look needs, and nothing is armed before it gets there.
#[test]
fn the_watch_command_lists_and_stops_and_hands_a_path_to_the_loop() {
    let mut session = Session::new("none");
    session.arm_watch(
        "a.md",
        "/work",
        bravebot_agent::watch::Looked::Saw("first".to_string()),
    );

    for c in "/watch".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.watches().len(), 1, "a report ended a watch");

    for c in "/watch stop 1".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert!(session.watches().is_empty());

    for c in "/watch src/main.rs".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::ArmWatch("src/main.rs".to_string())
    );
    assert!(
        session.watches().is_empty(),
        "the key handling armed a watch without the loop's look"
    );
}

/// The key that means "stop what is happening" has to reach the thing that keeps happening
/// before it reaches the thing that would take the session with it.
#[test]
fn interrupting_stops_the_loop_before_it_leaves() {
    let mut session = Session::new("none");
    session.start_loop(
        crate::loops::request("5m watch"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    session.complete("done", Vec::new(), 0);

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert!(
        session.looping().is_none(),
        "the loop survived the interrupt"
    );
    assert!(!session.is_quitting(), "the session left as well");

    // The rung that leaves offers first, so the press that takes it is a second one.
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
}

/// LOOP-11: Ctrl-C ends every loop, not the lowest-numbered one, because a key that left one
/// running would send the next prompt as though nothing had been said.
#[test]
fn interrupting_ends_every_loop_when_several_are_live() {
    let mut session = Session::new("none");
    for line in ["5m check the deploy", "1h check the pull request"] {
        session.start_loop(
            crate::loops::request(line),
            Vec::new(),
            Vec::new(),
            crate::state::NamesFrom::nothing(),
        );
        session.complete("done", Vec::new(), 0);
        session.loop_turn_ended(None);
    }
    assert_eq!(session.loops().len(), 2);

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert!(session.loops().is_empty(), "a loop survived the interrupt");
}

/// A half-written line is nearer than the loop, so stopping the loop does not cost somebody
/// the sentence they were in the middle of.
#[test]
fn interrupting_clears_the_line_before_it_stops_the_loop() {
    let mut session = Session::new("none");
    session.start_loop(
        crate::loops::request("5m watch"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    session.complete("done", Vec::new(), 0);
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    handle_key(&mut session, ctrl('c'));
    assert!(session.input().is_empty());
    assert!(session.looping().is_some(), "the loop went with the line");
}

/// An answer that arrives beside a turn becomes a row of the agents list, the view is not opened
/// over the turn, and nothing reaches the transcript. Rejects `asked_aside`, which opens the view.
#[test]
fn a_btw_answered_beside_a_turn_is_a_row_and_does_not_open_the_view() {
    let mut session = a_turn_running_on("first");
    let transcript = session.transcript.len();
    let (sender, from_worker) = std::sync::mpsc::channel();
    drop(sender);
    session.begin_aside_beside_a_turn(crate::state::AsideFlight {
        asked: "what is in parser.py".to_string(),
        cancel: Cancel::new(),
        from_worker,
        worker: std::thread::spawn(|| {
            Ok(bravebot_agent::aside::Answered {
                shown: "parse() is a stub".to_string(),
                kept: None,
                usage: Default::default(),
            })
        }),
    });

    settle_aside_beside_a_turn(&mut session, &Cancel::new());

    assert!(!session.aside_in_flight());
    assert_eq!(session.asides().len(), 1, "no row was kept");
    assert_eq!(session.asides()[0].question, "what is in parser.py");
    assert!(session.watching().is_none(), "the view took the screen");
    // The one line that says the row is there and which key opens it, and none of its words.
    let answered = t!(
        btw_answered_beside_a_turn,
        chord = session.bindings().watch_name()
    );
    assert_eq!(session.transcript.len(), transcript + 1);
    assert_eq!(said_under_the_turn(&session), vec![answered.as_str()]);
    assert!(
        !said_in_the_transcript(&session)
            .iter()
            .any(|said| said.contains("parser.py") || said.contains("a stub")),
        "a half of the aside reached the transcript"
    );
}

/// `/loop stop` typed behind a waiting `/after` waits behind it, since both use the one slot. The
/// regression it rejects is a stop carried out first, finding nothing, and the line sent anyway.
#[test]
fn a_loop_stop_typed_behind_a_waiting_after_waits_behind_it() {
    let mut session = a_turn_running_on("first");
    type_while_working(&mut session, "/after 5m check the deploy");

    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, "/loop stop", key(KeyCode::Enter), beside)
    });

    assert_eq!(
        waiting_prompts(&session),
        vec!["/after 5m check the deploy", "/loop stop"]
    );
}

/// LOOP-18: the delay of an `/after` line typed during a turn counts from when it was typed. Six
/// minutes into the turn, an `/after 5m` line is due the moment the turn ends. The regression it
/// rejects is arming the line from the moment the queue reaches it, which makes it wait five
/// more minutes.
#[test]
fn an_after_line_typed_during_a_turn_is_due_at_once_when_its_delay_passed_before_the_turn_ended() {
    let mut session = a_turn_running_on("first");
    type_while_working(&mut session, "/after 5m check the deploy");
    assert!(
        session.looping().is_none(),
        "the line was armed while queued"
    );

    session.backdate_queued(Duration::from_secs(6 * 60));
    session.complete("done", Vec::new(), 0);
    assert_eq!(queued_next(&mut session), Some(Action::Redraw));

    assert_eq!(
        session.loop_tick().as_deref(),
        Some("check the deploy"),
        "the delay was counted from the end of the turn"
    );
}

/// LOOP-18, LOOP-6: a delay that has not passed when the turn ends is what is left of it, and
/// the row that counts down shows that remainder. The regressions it rejects are counting the
/// whole delay again, and sending the line before its delay is up.
#[test]
fn an_after_line_typed_during_a_turn_waits_for_what_is_left_of_its_delay() {
    let mut session = a_turn_running_on("first");
    type_while_working(&mut session, "/after 5m check the deploy");

    session.backdate_queued(Duration::from_secs(2 * 60));
    session.complete("done", Vec::new(), 0);
    assert_eq!(queued_next(&mut session), Some(Action::Redraw));

    assert_eq!(session.loop_tick(), None, "sent before the delay was up");
    let left = session
        .looping()
        .and_then(|running| running.until(Instant::now()))
        .expect("the line is armed");
    assert!(
        left > Duration::from_secs(2 * 60) && left <= Duration::from_secs(3 * 60),
        "{left:?} is not what is left of five minutes after two"
    );
}

/// LOOP-18: a line typed at rest counts its delay from now. The regression it rejects is a
/// typing time taken from somewhere older than the line.
#[test]
fn an_after_line_typed_at_rest_counts_its_delay_from_now() {
    let mut session = Session::new("none");
    type_line(&mut session, "/after 5m check the deploy");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );

    let left = session
        .looping()
        .and_then(|running| running.until(Instant::now()))
        .expect("the line is armed");
    assert!(left > Duration::from_secs(5 * 60 - 5), "{left:?}");
    assert_eq!(session.loop_tick(), None, "sent before the delay was up");
}

/// LOOP-18: a command line queued without being typed, which is what a click on a hint does
/// through `queue_command_line`, has no typing time of its own and counts its delay from when it
/// was queued. The regression it rejects is a typing time older than the line. A click only ever
/// queues the fixed command lines of `click_command`, so this drives the queue entry point
/// directly and not a click.
#[test]
fn an_after_line_queued_by_the_command_path_counts_its_delay_from_now() {
    let mut session = a_turn_running_on("first");
    assert!(session.queue_command_line("/after 5m check the deploy"));

    session.complete("done", Vec::new(), 0);
    assert_eq!(queued_next(&mut session), Some(Action::Redraw));

    let left = session
        .looping()
        .and_then(|running| running.until(Instant::now()))
        .expect("the line is armed");
    assert!(left > Duration::from_secs(5 * 60 - 5), "{left:?}");
}

/// A refusal is drawn as one, with the shared function's sentence unchanged. Rejects a
/// command that words its own refusals or draws a refusal as success.
#[test]
fn a_refusal_from_the_shared_function_is_drawn_as_a_refusal_in_its_own_words() {
    let mut session = a_session_scheduling_through_the_fake();
    type_line(&mut session, "/schedule remove 9");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(last_note(&session), "no entry 9");
    assert_eq!(
        session.transcript.last().map(|entry| entry.tone),
        Some(crate::state::Tone::Refused)
    );
}

/// For lines the shared function refuses before it reads the state directory, the note a
/// session draws is the sentence the command line prints for the same words, from the same
/// function. Rejects a session with sentences of its own.
#[test]
fn a_session_draws_the_sentence_the_command_line_prints_for_the_same_words() {
    for (line, words) in [
        ("/schedule bogus", vec!["bogus"]),
        ("/schedule remove", vec!["remove"]),
        ("/schedule pause 1 2", vec!["pause", "1", "2"]),
        ("/schedule add --cron", vec!["add", "--cron"]),
    ] {
        let args: Vec<String> = words.iter().map(|w| (*w).to_string()).collect();
        let on_the_command_line = bravebot_schedule::command(
            &args,
            &bravebot_schedule::Context {
                typed: Some(bravebot_schedule::TypedLine::of_a_person()),
                mode: Some(bravebot_schedule::Mode::Read),
                flags_typed: false,
                project: Some(std::path::PathBuf::from("/work")),
            },
        );
        let sentence = on_the_command_line.err.expect("a refusal");
        let mut session = Session::new("none");
        type_line(&mut session, line);
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(
            last_note(&session),
            crate::render::printable(&sentence),
            "{line}"
        );
        assert_eq!(
            session.transcript.last().map(|entry| entry.tone),
            Some(crate::state::Tone::Refused),
            "{line}"
        );
    }
}

/// Whatever mode the session is in, a mode word typed into `/schedule add` is
/// refused with the sentence that points at the command line, and the session's mode is not
/// changed by it. Rejects a session that stores its own mode, or one that lets the word
/// choose one.
#[test]
fn a_session_in_accept_edits_or_bypass_stores_read_and_refuses_a_mode_word() {
    use bravebot_agent::PermissionMode;
    for (presses, mode) in [
        (1, PermissionMode::AcceptEdits),
        (4, PermissionMode::Bypass),
    ] {
        for word in ["--accept-edits", "--dangerously-skip-permissions"] {
            let mut session = Session::new("none");
            for _ in 0..presses {
                handle_key(&mut session, shift(KeyCode::Tab));
            }
            assert_eq!(session.permission_mode(), mode);
            type_line(
                &mut session,
                &format!("/schedule add --cron '0 9 * * *' --prompt hi {word}"),
            );
            handle_key(&mut session, key(KeyCode::Enter));
            assert_eq!(
                last_note(&session),
                crate::render::printable(t!(schedule_add_mode_on_the_command_line)),
                "{mode:?} {word}"
            );
            assert_eq!(session.permission_mode(), mode);
        }
    }
    // Without a word the seam is given nothing about the mode: its signature is the words, the
    // proof that a person typed them and the directory, and the same words reach it in each.
    for presses in [0, 1, 4] {
        let mut session = a_session_scheduling_through_the_fake();
        for _ in 0..presses {
            handle_key(&mut session, key_shift_tab());
        }
        type_line(&mut session, "/schedule add --cron '0 9 * * *' --prompt hi");
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(schedule_calls().len(), 1, "{presses}");
    }
}

fn key_shift_tab() -> KeyEvent {
    shift(KeyCode::Tab)
}

/// A control character in a line is drawn as a picture, so it cannot move the cursor or
/// rewrite the screen. Rejects a note that holds the line as it came.
#[test]
fn a_control_character_in_a_reply_is_pictured_in_the_note() {
    let mut session = a_session_scheduling_through_the_fake();
    type_line(&mut session, "/schedule hostile");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(
        last_note(&session),
        "a\u{241b}[2Jb\u{240d}c\u{2407}d\nsecond"
    );
}

/// A reply is drawn as its lines, then the sentence of a refusal, then the notice, in one note
/// marked as a refusal by the reply's outcome. Rejects a note that reorders them or drops the
/// notice.
#[test]
fn a_reply_is_drawn_as_its_lines_then_its_sentence_then_its_notice() {
    let mut session = a_session_scheduling_through_the_fake();
    type_line(&mut session, "/schedule noticing");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(
        last_note(&session),
        "first\nsecond\nthe sentence\nthe notice"
    );
    assert_eq!(
        session.transcript.last().map(|entry| entry.tone),
        Some(crate::state::Tone::Refused)
    );
}

/// A line the planner wrote that reads `/schedule add ...` is text: the only caller of the
/// seam is the command the box dispatches, so a reply that holds the line reaches nothing.
/// Rejects an implementation that looks for commands in what a reply says.
#[test]
fn a_reply_that_reads_schedule_add_is_text_and_reaches_nothing() {
    let mut session = a_session_scheduling_through_the_fake();
    session.transcript.push(crate::Entry::assistant(
        "/schedule add --cron '* * * * *' --prompt 'rm -rf'",
        Vec::new(),
    ));
    assert!(!session.settle_schedule_runs());
    assert!(schedule_calls().is_empty());
    assert!(!session.a_turn_is_running());
}

/// `run-now` returns at once with a note saying it started, and the answer is drawn when it
/// arrives, not before. Rejects a command that blocks on the run, that draws nothing when the
/// run ends, or that draws the answer twice.
#[test]
fn run_now_answers_when_the_run_ends_and_not_before() {
    let mut session = a_session_scheduling_through_the_fake();
    type_line(&mut session, "/schedule run-now 2");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(last_note(&session), t!(schedule_command_running));
    assert_eq!(session.schedule_runs_waiting(), 1);
    let notes = session.transcript.len();
    assert!(!session.settle_schedule_runs(), "drew before the run ended");
    assert_eq!(session.transcript.len(), notes);

    SCHEDULE_RUN.with(|run| {
        run.borrow()
            .as_ref()
            .expect("started")
            .send(schedule_reply(
                bravebot_schedule::Outcome::Done,
                &["the run said this"],
                None,
            ))
            .expect("sent");
    });
    assert!(session.settle_schedule_runs());
    assert_eq!(last_note(&session), "the run said this");
    assert_eq!(session.schedule_runs_waiting(), 0);
    assert!(!session.settle_schedule_runs(), "drew it twice");
    assert_eq!(session.transcript.len(), notes + 1);
}

/// A run whose thread ended without answering is forgotten, so the session does not wait
/// for it for good.
#[test]
fn a_run_now_that_never_answers_is_forgotten() {
    let mut session = a_session_scheduling_through_the_fake();
    type_line(&mut session, "/schedule run-now 2");
    handle_key(&mut session, key(KeyCode::Enter));
    SCHEDULE_RUN.with(|run| *run.borrow_mut() = None);
    session.settle_schedule_runs();
    assert_eq!(session.schedule_runs_waiting(), 0);
}

/// An unclosed quote is refused before the seam is reached, since what the line
/// would do is not known. Rejects guessing where the quote ends.
#[test]
fn an_unclosed_quote_is_refused_before_anything_is_run() {
    let mut session = a_session_scheduling_through_the_fake();
    type_line(&mut session, "/schedule add --prompt 'never closed");
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(schedule_calls().is_empty());
    assert_eq!(last_note(&session), t!(schedule_command_unclosed_quote));
    assert_eq!(
        session.transcript.last().map(|entry| entry.tone),
        Some(crate::state::Tone::Refused)
    );
}

/// The goal check is one request too, and the goal behind it is what its stop keys are for.
/// Neither key is about the goal or the check while the search is open.
#[test]
fn the_search_answers_the_stop_keys_before_the_goal_check_does() {
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        let mut session = having_sent(&["first question"]);
        session.begin_aside();
        session.start_goal("cargo test exits 0".to_string());
        let cancel = Cancel::new();

        goal_check_key(&mut session, ctrl('r'), &cancel);
        assert!(session.searching_history(), "the search did not open");
        let said = session.transcript.len();

        goal_check_key(&mut session, stopping, &cancel);

        assert!(
            !session.searching_history(),
            "{stopping:?} left the search open"
        );
        assert!(
            session.goal().is_some(),
            "{stopping:?} cleared the goal from inside the search"
        );
        assert!(
            !cancel.is_cancelled(),
            "{stopping:?} stopped the check from inside the search"
        );
        assert!(!session.is_quitting(), "{stopping:?} ended the session");
        assert_eq!(
            session.transcript.len(),
            said,
            "{stopping:?} reported the goal from inside the search"
        );
    }
}

/// A loopback model answering every request with `words`, streamed the way the real one is.
#[cfg(unix)]
fn a_model_answering_every_request(words: &'static str) -> String {
    use std::io::{BufRead, BufReader, Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = format!("http://{}", listener.local_addr().expect("an address"));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            let mut length = 0usize;
            while matches!(reader.read_line(&mut line), Ok(read) if read > 2) {
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap_or(0);
                }
                line.clear();
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            let frames = format!(
                "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{words}\"}}}}]}}\n\n\
                     data: {{\"choices\":[{{\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":400,\"completion_tokens\":20}}}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{frames}",
                frames.len()
            );
            let _ = reader.into_inner().write_all(response.as_bytes());
        }
    });
    address
}

/// HOOK-2, HOOK-9: the thread `/compact` starts runs the person's `compacted` hook once the
/// summary is adopted, in the session's directory, and tells the screen which hook is running.
/// A terminal that handed the agent no hooks would shorten the conversation and fire nothing.
#[cfg(unix)]
#[test]
fn the_thread_that_summarises_fires_the_hooks_in_the_sessions_directory() {
    use std::os::unix::fs::PermissionsExt;

    let directory = crate::testutil::scratch_dir("bravebot-app-compact-hook");
    let _ = std::fs::remove_dir_all(&directory);
    let home = directory.join("state");
    std::fs::create_dir_all(&home).expect("a state directory");
    let note = directory.join("note");
    std::fs::write(&note, "#!/bin/sh\ncat >> fired.txt\n").expect("a script");
    std::fs::set_permissions(&note, std::fs::Permissions::from_mode(0o755)).expect("exec");
    std::fs::write(
        home.join("hooks.json"),
        format!(
            r#"{{"hooks": [{{"on": "compacted", "run": [{:?}]}}]}}"#,
            note.canonicalize().expect("canonical")
        ),
    )
    .expect("a hooks file");
    let workspace = Workspace::new(&directory).expect("workspace");
    let endpoint = a_model_answering_every_request("they were porting the parser");
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.clone()),
        _ => None,
    })
    .expect("config");
    let mut conversation = Conversation::new();
    for (prompt, answer) in [
        ("port the parser to the new lexer", "started on it"),
        ("what about the error type", "widened it"),
        ("now the tests", "updated them"),
        ("carry on", "carrying on"),
    ] {
        conversation.push(bravebot_aichat::protocol::Message::user(prompt));
        conversation.push(bravebot_aichat::protocol::Message::assistant(answer));
    }
    let (to_main, from_worker) = mpsc::channel();

    let (done, shortened, _trail) = summarise_on_a_worker(
        &config,
        &bravebot_agent::chatgpt::AccountSession::begin(),
        conversation,
        None,
        TrustStore::new("/work"),
        None,
        &Cancel::new(),
        &workspace,
        Some(&home),
        to_main,
    );

    let fired = std::fs::read_to_string(directory.join("fired.txt"));
    let said: Vec<String> = from_worker
        .try_iter()
        .filter_map(|message| match message {
            crate::remote_confirm::ToMain::HookStarted(moment, _) => {
                Some(format!("started {moment}"))
            }
            crate::remote_confirm::ToMain::HookFinished => Some("finished".to_string()),
            _ => None,
        })
        .collect();
    std::fs::remove_dir_all(&directory).ok();

    assert!(matches!(done, Ok(Some(_))), "no summary was adopted");
    assert!(shortened.len() < 8, "the conversation was not shortened");
    assert_eq!(
        fired.expect("the hook did not run in the session's directory"),
        "{\"event\":\"compacted\"}\n"
    );
    assert_eq!(said, ["started compacted", "finished"]);
}

/// HOOK-9, HOOK-7: a `compacted` hook runs on the thread that summarised, so the screen is told
/// which one holds it while it runs and what went wrong when it ended badly. A loop that
/// dropped those messages would show a frozen `/compact` for as long as the hook took, and say
/// nothing of a hook that failed.
#[test]
fn a_hook_running_after_a_summary_is_named_and_its_trouble_said() {
    let mut session = having_sent(&["first question"]);
    session.begin_aside();

    compact_reported(
        &mut session,
        crate::remote_confirm::ToMain::HookStarted("compacted", "notify".to_string()),
    );
    assert_eq!(
        session.indicator().expect("working").verb,
        t!(indicator_hook, moment = "compacted", program = "notify")
    );

    compact_reported(&mut session, crate::remote_confirm::ToMain::HookFinished);
    assert_ne!(
        session.indicator().expect("working").verb,
        t!(indicator_hook, moment = "compacted", program = "notify"),
        "the hook was still named after it was over"
    );

    let said = t!(
        hook_failed,
        moment = "compacted",
        program = "notify",
        status = "exit status: 3"
    );
    compact_reported(
        &mut session,
        crate::remote_confirm::ToMain::Notice(said.to_string()),
    );
    assert_eq!(last_note(&session), said);
}

/// The goal check is one request as well, and there is something behind it worth stopping: the
/// goal that keeps putting turns out. Both keys take it off, say so, and stop the check with
/// it, since a person pressing either at the ninth round is asking for the tenth not to go. No
/// press leaves, however many are made.
#[test]
fn a_stop_key_takes_the_goal_off_and_stops_the_check() {
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        let mut session = having_sent(&["first question"]);
        session.begin_aside();
        session.start_goal("cargo test exits 0".to_string());
        let cancel = Cancel::new();
        let said = session.transcript.len();

        goal_check_key(&mut session, stopping, &cancel);

        assert!(session.goal().is_none(), "{stopping:?} left the goal armed");
        assert!(cancel.is_cancelled(), "{stopping:?} did not stop the check");
        let after = session.transcript[said..]
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            after.contains(t!(goal_cleared)),
            "{stopping:?} took the goal off without saying so: {after}"
        );
        assert!(!session.is_quitting(), "{stopping:?} ended the session");

        goal_check_key(&mut session, stopping, &cancel);
        goal_check_key(&mut session, stopping, &cancel);
        assert!(
            !session.is_quitting(),
            "{stopping:?} pressed again ended the session"
        );
    }

    let mut session = having_sent(&["first question"]);
    session.begin_aside();
    let cancel = Cancel::new();
    let said = session.transcript.len();

    goal_check_key(&mut session, key(KeyCode::Esc), &cancel);

    assert!(
        cancel.is_cancelled(),
        "escape did not stop a check with no goal"
    );
    assert_eq!(
        session.transcript.len(),
        said,
        "escape reported something with no goal to take off"
    );

    for _ in 0..3 {
        goal_check_key(&mut session, ctrl('c'), &cancel);
    }
    assert!(
        !session.is_quitting(),
        "ctrl-c left with the goal already off"
    );
}

/// A check the person stopped is the request being walked away from, and the key already took
/// the goal off, so reporting it as a check that failed would tell them the goal could not be
/// checked a moment after they cleared it. One that failed with no stop pressed is the goal's
/// own failure, and a verdict that crosses a stop on its way back is dropped.
#[test]
fn a_stopped_goal_check_is_not_reported_as_a_failure() {
    let mut session = having_sent(&["first question"]);
    session.begin_aside();
    session.start_goal("cargo test exits 0".to_string());
    session.clear_goal();
    let said = session.transcript.len();

    let carried_on = goal_checked(&mut session, Err("offline".to_string()), true);

    assert!(carried_on.is_none());
    assert_eq!(session.status, Status::Idle);
    assert_eq!(
        session.transcript.len(),
        said,
        "the stopped check was reported: {}",
        last_note(&session)
    );

    let mut session = having_sent(&["first question"]);
    session.begin_aside();
    session.start_goal("cargo test exits 0".to_string());

    let carried_on = goal_checked(&mut session, Err("offline".to_string()), false);

    assert!(carried_on.is_none());
    assert_eq!(
        last_note(&session),
        t!(goal_failed, problem = "offline"),
        "a check that failed with no stop pressed was not reported"
    );

    let mut session = having_sent(&["first question"]);
    session.begin_aside();
    session.start_goal("cargo test exits 0".to_string());
    session.clear_goal();
    let turns = session.turns;
    let said = session.transcript.len();

    let carried_on = goal_checked(
        &mut session,
        Ok(bravebot_agent::goal::Assessed {
            verdict: bravebot_agent::goal::Verdict::NotMet {
                reason: "nothing above runs the tests".to_string(),
            },
            usage: Default::default(),
        }),
        true,
    );

    assert!(
        carried_on.is_none(),
        "a verdict after the stop sent work back"
    );
    assert_eq!(session.turns, turns);
    assert_eq!(session.transcript.len(), said);
}

/// BG-2: `/bg` is a command of its own, a longer word or a sentence mentioning it stays a prompt
/// (CMD-2), and typed during a turn it waits for the turn (CMD-8).
#[test]
fn the_background_command_is_never_a_prompt_and_waits_for_the_turn() {
    let mut session = Session::new("none");
    for c in "/bg".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Background
    );
    for line in ["what does /bg do", "/bgx", "/bg now"] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert!(
            matches!(
                handle_key(&mut session, key(KeyCode::Enter)),
                Action::Submit(_)
            ),
            "{line:?} was taken for the command"
        );
    }
    let mut session = a_turn_running_on("first");
    for c in "/bg".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(waiting_prompts(&session), vec!["/bg"]);
}

fn a_record() -> Option<bravebot_session::sessions::Resumable> {
    Some(bravebot_session::sessions::Resumable {
        id: "3f2a9c1e-0000-4000-8000-000000000000".to_string(),
        directory: std::path::PathBuf::from("/work"),
    })
}

/// BG-2 and BG-8: `/bg` ends the session handing over its record and the mode it was in, for
/// every mode a background session can run in.
#[test]
fn the_background_command_carries_the_record_and_the_mode() {
    use bravebot_session::jobs::Mode;
    if !cfg!(unix) {
        return;
    }
    let mut editing = Session::new("none");
    editing.cycle_permission_mode();
    let mut planning = Session::new("none");
    planning.enter_plan_mode();
    for (session, expected) in [
        (Session::new("none"), Mode::Ask),
        (editing, Mode::AcceptEdits),
        (planning, Mode::Plan),
    ] {
        match handed_to_the_background(
            &session,
            a_record(),
            None,
            bravebot_sandbox::SandboxMode::WorkspaceWrite,
            false,
        ) {
            Ok(Ended::Background { left, mode }) => {
                assert_eq!(mode, expected);
                assert_eq!(left.directory, std::path::PathBuf::from("/work"));
            }
            other => panic!("{other:?}"),
        }
    }
}

/// BG-8: a session in bypass is not handed over, and says why.
#[test]
fn the_background_command_is_refused_in_bypass() {
    if !cfg!(unix) {
        return;
    }
    let session = Session::new("none").starting_in_bypass();
    assert_eq!(
        handed_to_the_background(
            &session,
            a_record(),
            None,
            bravebot_sandbox::SandboxMode::WorkspaceWrite,
            false
        )
        .unwrap_err(),
        t!(session_bg_bypass).to_string()
    );
}

/// BG-8: a session in auto is not handed over either, and says why, so a session the setting
/// opened in auto is refused until the key leaves it. The fault it rejects is a session in auto
/// started in the background with nobody to answer a question the classifier does not allow.
#[test]
fn the_background_command_is_refused_in_auto() {
    if !cfg!(unix) {
        return;
    }
    let session = Session::new("none").starting_in_auto(bravebot_agent::ModeCause::Flag);
    assert_eq!(
        handed_to_the_background(
            &session,
            a_record(),
            None,
            bravebot_sandbox::SandboxMode::WorkspaceWrite,
            false
        )
        .unwrap_err(),
        t!(session_bg_auto).to_string()
    );
}

/// BG-2: an option given on the command line never reaches the background process, so a session
/// started with one is not handed over.
#[test]
fn the_background_command_is_refused_after_an_option_it_cannot_pass_on() {
    if !cfg!(unix) {
        return;
    }
    let session = Session::new("none");
    let refused = handed_to_the_background(
        &session,
        a_record(),
        Some("--no-shell"),
        bravebot_sandbox::SandboxMode::WorkspaceWrite,
        false,
    )
    .unwrap_err();
    assert_eq!(
        refused,
        t!(session_bg_option_lost, flag = "--no-shell").to_string()
    );
    assert!(refused.contains("--no-shell"), "{refused}");
}

/// BG-2, SANDBOX-22: the background process chooses its sandbox mode again from what start-up
/// read, so a session `/sandbox` moved to another mode is not handed over, whether to a
/// stricter mode or to `off`, and says which mode it would start in. One moved back to that
/// mode is handed over, and so is one whose start-up mode is the one it holds.
#[test]
fn the_background_command_is_refused_after_sandbox_moved_the_mode() {
    if !cfg!(unix) {
        return;
    }
    for moved in [
        bravebot_sandbox::SandboxMode::Strict,
        bravebot_sandbox::SandboxMode::Off,
    ] {
        let mut session = Session::new("none");
        session.set_sandbox_mode(moved);
        let refused = handed_to_the_background(
            &session,
            a_record(),
            None,
            bravebot_sandbox::SandboxMode::WorkspaceWrite,
            false,
        )
        .unwrap_err();
        assert_eq!(
            refused,
            t!(
                session_bg_sandbox_moved,
                mode = moved.name(),
                settled = "workspace-write"
            )
            .to_string()
        );
    }
    let mut session = Session::new("none");
    session.set_sandbox_mode(bravebot_sandbox::SandboxMode::Strict);
    session.set_sandbox_mode(bravebot_sandbox::SandboxMode::WorkspaceWrite);
    assert!(matches!(
        handed_to_the_background(
            &session,
            a_record(),
            None,
            bravebot_sandbox::SandboxMode::WorkspaceWrite,
            false
        ),
        Ok(Ended::Background { .. })
    ));
    let mut session = Session::new("none");
    session.set_sandbox_mode(bravebot_sandbox::SandboxMode::Strict);
    assert!(matches!(
        handed_to_the_background(
            &session,
            a_record(),
            None,
            bravebot_sandbox::SandboxMode::Strict,
            false
        ),
        Ok(Ended::Background { .. })
    ));
}

/// BG-2: a session with nothing recorded has nothing to hand over, and a loop, a goal or a
/// watch would be dropped without a word, so each is refused.
#[test]
fn the_background_command_is_refused_without_a_record_or_with_work_it_would_drop() {
    if !cfg!(unix) {
        return;
    }
    let session = Session::new("none");
    assert_eq!(
        handed_to_the_background(
            &session,
            None,
            None,
            bravebot_sandbox::SandboxMode::WorkspaceWrite,
            false
        )
        .unwrap_err(),
        t!(session_bg_nothing_recorded).to_string()
    );
    assert_eq!(
        handed_to_the_background(
            &session,
            None,
            None,
            bravebot_sandbox::SandboxMode::WorkspaceWrite,
            true
        )
        .unwrap_err(),
        t!(session_bg_incognito).to_string()
    );

    let mut session = Session::new("none");
    session.start_goal("the tests pass".to_string());
    assert_eq!(
        handed_to_the_background(
            &session,
            a_record(),
            None,
            bravebot_sandbox::SandboxMode::WorkspaceWrite,
            false
        )
        .unwrap_err(),
        t!(session_bg_keeps_nothing_running).to_string()
    );
}

/// A watch is armed on a workspace-relative path, so moving the working directory closes the
/// directory the answer that allowed it was about. The watch has to end with the move and say
/// so, for the reason the servers are dropped and the directories closed: nothing here is
/// still the thing anybody agreed to. With the move rather than at the next look, which is up
/// to five seconds away and long enough for a fire to go out in.
///
/// The directory moved to holds a `notes.md` of its own, of another size, which is what the
/// watch reported movement on while a look was resolved against whatever the working
/// directory happened to be. So a wrong answer here is a fire, on a file nothing in the
/// session wrote and nobody armed a watch on.
#[test]
fn changing_directory_ends_a_watch_armed_in_the_one_left_behind() {
    let root = crate::testutil::scratch_dir("bravebot-cd-watch-test");
    let project = root.join("project");
    let other = root.join("other");
    std::fs::create_dir_all(&project).expect("scratch");
    std::fs::create_dir_all(&other).expect("scratch");
    std::fs::write(project.join("notes.md"), "the watched one").expect("scratch");
    std::fs::write(other.join("notes.md"), "a different file, of another size").expect("scratch");

    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    let under = workspace.root();
    session.arm_watch("notes.md", under, workspace.look("notes.md", under));
    assert_eq!(session.watches().len(), 1, "the watch was not armed");

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            other.to_str().expect("utf-8 path"),
            never_asked,
        ),
        Changed::Moved
    );

    assert!(
        session.watches().is_empty(),
        "the watch outlived the directory it was armed in"
    );
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(watch_out_of_reach, number = 1)),
        "the watch ended without saying so"
    );

    // And nothing is left to fire on the next pass, which is where the fire about the other
    // directory's file went out from.
    let later = Instant::now() + Duration::from_secs(6);
    assert!(
        session
            .watch_fired(later, |path, under| workspace.look(path, under))
            .is_none(),
        "a fire went out about the file in the directory the session moved to"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// What `/watch <path>` does once the loop holds the workspace: [`Held::arm_watch_command`]
/// with everything else a session at rest has.
fn arm_typed(session: &mut Session, workspace: &mut Workspace, answers: &mut Answers, path: &str) {
    let mut stored = bravebot_session::sessions::Handle::begin(
        workspace.root(),
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let mut conversation = Conversation::new();
    let mut scratch = None;
    let settings = bravebot_config::Settings::default();
    let run_settings = RunSettings::from_settings(&settings);
    Held {
        session,
        conversation: &mut conversation,
        stored: &mut stored,
        answers,
        workspace,
        scratch: &mut scratch,
        settings: &settings,
        run_settings: &run_settings,
    }
    .arm_watch_command(path);
}

/// A session at rest in a scratch project, with the answers it opens with.
fn watch_project(name: &str) -> (std::path::PathBuf, Workspace, Session, Answers) {
    let root = crate::testutil::scratch_dir(name);
    let project = root.join("project");
    std::fs::create_dir_all(&project).expect("scratch");
    let workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    (root, workspace, session, answers)
}

/// FSWATCH-13: a watch the person typed is armed through the loop's first look and then fires,
/// coalesces and ends like a turn's. Without the fire there is nothing a typed watch is for,
/// and a separate path for it would be a second implementation of the same watch.
#[test]
fn typing_watch_and_a_path_arms_a_watch_that_fires_on_a_write() {
    let (root, mut workspace, mut session, mut answers) =
        watch_project("bravebot-typed-watch-test");
    std::fs::write(workspace.root().join("notes.md"), "first").expect("scratch");

    let Action::ArmWatch(path) = dispatch_command(&mut session, commanded("/watch notes.md"))
    else {
        panic!("/watch notes.md did not ask the loop to arm a watch");
    };
    arm_typed(&mut session, &mut workspace, &mut answers, &path);
    assert_eq!(session.watches().len(), 1, "the watch was not armed");
    assert!(
        session.watches()[0].armed_by_the_person(),
        "the watch was not marked as the person's"
    );
    assert!(
        noted(&session, &t!(watch_armed, number = 1, path = "notes.md")),
        "arming the watch said nothing"
    );

    std::fs::write(
        workspace.root().join("notes.md"),
        "a second, longer version",
    )
    .expect("scratch");
    let later = Instant::now() + Duration::from_secs(6);
    assert!(
        session
            .watch_fired(later, |path, under| workspace.look(path, under))
            .is_some(),
        "a write to the watched file did not fire"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// Typing a path already watched, under the same spelling or another one the workspace resolves
/// to the same file, leaves one watch and says which one it is.
#[test]
fn typing_watch_on_a_path_already_watched_arms_nothing_new() {
    let (root, mut workspace, mut session, mut answers) =
        watch_project("bravebot-typed-watch-again-test");
    std::fs::write(workspace.root().join("notes.md"), "first").expect("scratch");

    arm_typed(&mut session, &mut workspace, &mut answers, "notes.md");
    assert_eq!(session.watches().len(), 1, "the watch was not armed");
    for again in ["notes.md", "./notes.md"] {
        arm_typed(&mut session, &mut workspace, &mut answers, again);
        assert_eq!(session.watches().len(), 1, "{again} armed a second watch");
        assert!(
            noted(&session, &t!(watch_already, number = 1, path = again)),
            "{again} said nothing about the watch already on it"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

/// FSWATCH-2: a directory is refused, as a turn's request for one is.
#[test]
fn typing_watch_on_a_directory_is_refused() {
    let (root, mut workspace, mut session, mut answers) =
        watch_project("bravebot-typed-watch-directory-test");
    std::fs::create_dir_all(workspace.root().join("src")).expect("scratch");

    arm_typed(&mut session, &mut workspace, &mut answers, "src");

    assert!(session.watches().is_empty(), "a directory was watched");
    assert!(
        noted(&session, &t!(watch_refused_directory, path = "src")),
        "the refusal said nothing"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// FSWATCH-7: outside bypass, a path outside the project is not reached, so it is not armed.
#[test]
fn typing_watch_on_a_path_outside_the_project_is_refused_outside_bypass() {
    let (root, mut workspace, mut session, mut answers) =
        watch_project("bravebot-typed-watch-outside-test");
    let elsewhere = root.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("scratch");
    std::fs::write(elsewhere.join("notes.md"), "not in the project").expect("scratch");
    let outside = elsewhere
        .canonicalize()
        .expect("canonical")
        .join("notes.md");
    let outside = outside.to_str().expect("utf-8 path");

    arm_typed(&mut session, &mut workspace, &mut answers, outside);

    assert!(
        session.watches().is_empty(),
        "a path outside the project was watched"
    );
    assert!(
        noted(&session, &t!(watch_refused_unreadable, path = outside)),
        "the refusal said nothing"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// Nothing is armed while a turn runs, so the first look never runs while the turn holds the
/// trust map (FSWATCH-7). The line waits in the queue (CMD-8) and is carried out once the turn
/// ends, so the person does not type it again. Listing and stopping still answer at once.
#[test]
fn typing_watch_with_a_path_while_a_turn_runs_waits_for_the_turn() {
    assert!(runs_while_working("/watch"));
    assert!(runs_while_working("/watch stop 1"));
    assert!(!runs_while_working("/watch notes.md"));

    let mut session = a_turn_running_on("first");
    type_while_working(&mut session, "/watch notes.md");
    assert!(session.watches().is_empty());
    let said = said_under_the_turn(&session);
    assert!(
        said.len() == 1 && said[0].contains("waits for the turn to end"),
        "/watch notes.md was carried out mid-turn: {said:?}"
    );

    session.complete("answered", Vec::new(), 0);
    let taken = session
        .take_queued_command()
        .expect("/watch notes.md did not wait to be carried out");
    assert_eq!(taken.line, "/watch notes.md");
    assert_eq!(
        dispatch_command(&mut session, taken),
        Action::ArmWatch("notes.md".to_string())
    );
}

/// After `/cd` the same relative path names a different file, so a person's relative watch
/// ends on the move as a turn's does (FSWATCH-7). Left armed, it would report changes to a
/// file the person never named, here one of another size in the new directory.
#[test]
fn a_watch_the_person_armed_under_a_relative_path_ends_on_cd() {
    let (root, mut workspace, mut session, mut answers) =
        watch_project("bravebot-typed-watch-cd-test");
    let other = root.join("other");
    std::fs::create_dir_all(&other).expect("scratch");
    std::fs::write(workspace.root().join("notes.md"), "the watched one").expect("scratch");
    std::fs::write(other.join("notes.md"), "a different file, of another size").expect("scratch");

    arm_typed(&mut session, &mut workspace, &mut answers, "notes.md");
    assert_eq!(session.watches().len(), 1, "the watch was not armed");

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            other.to_str().expect("utf-8 path"),
            never_asked,
        ),
        Changed::Moved
    );

    assert!(
        session.watches().is_empty(),
        "the watch outlived the directory it was armed in"
    );
    assert!(
        noted(&session, &t!(watch_out_of_reach, number = 1)),
        "the watch ended without saying so"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// A loop repeats a line somebody endorsed. The sentence this program writes to carry a goal
/// on is not one, so a turn running that sentence and asking for a later look gets no loop:
/// otherwise the driver's own words would come back every quarter of an hour, and the person
/// would be reading a watch nobody set up over a line nobody typed.
#[test]
fn only_a_line_the_person_wrote_becomes_a_watch_the_turn_asked_for() {
    let wakeup = turn::Wakeup::asked(900, false);
    let idle = Session::new("none");
    assert_eq!(
        watch_to_start(Some(wakeup), will_look_again(&idle, Wrote::ThePerson)),
        Some(wakeup),
        "a turn on the person's own line could not arrange a later look"
    );
    assert_eq!(
        watch_to_start(Some(wakeup), will_look_again(&idle, Wrote::TheDriver)),
        None
    );
    assert_eq!(
        watch_to_start(None, will_look_again(&idle, Wrote::ThePerson)),
        None
    );
}

/// GOAL-14: a turn is told the condition of the goal in force, and a goal the person has held
/// still is not in force, so a turn under it is told nothing about a condition. A turn start
/// that read the goal that is set, rather than the goal in force, would tell the planner
/// about a paused goal. `goal_in_force` is tested alone in `state`; this is what the turn's
/// start reads.
///
/// The `Some` answers keep a version that always answers `None` from passing.
#[test]
fn a_turn_under_a_paused_goal_is_told_nothing_about_a_condition() {
    let mut session = Session::new("none");
    assert_eq!(
        goal_to_work_towards(&session),
        None,
        "a turn with no goal was told a condition"
    );

    session.start_goal("cargo test exits 0".to_string());
    assert_eq!(
        goal_to_work_towards(&session),
        Some("cargo test exits 0".to_string()),
        "a turn under a goal was not told its condition"
    );

    session.pause_goal();
    assert!(session.goal().is_some(), "pausing ended the goal");
    assert_eq!(
        goal_to_work_towards(&session),
        None,
        "a turn under a paused goal was told its condition"
    );

    session.resume_goal();
    assert_eq!(
        goal_to_work_towards(&session),
        Some("cargo test exits 0".to_string()),
        "a resumed goal was not told to the turn"
    );
}

/// GOAL-14: the task a turn is built with carries the condition of the goal in force, and
/// nothing for a paused goal. The test above reads `goal_to_work_towards` alone, so a task
/// builder that read the goal that is set, or passed no condition, would leave it passing.
/// Rejects: `the_turn_task` reading `session.goal()` where it should read the goal in force,
/// and one that leaves the condition off the task.
#[test]
fn the_task_a_turn_is_built_with_carries_the_goal_only_while_it_is_in_force() {
    let root = a_workspace_with_a_readme();
    let mut session = Session::new("none").in_workspace(root.path());
    let said_to = |session: &mut Session| {
        a_task_built_for(session, root.path(), "go")
            .working_towards
            .clone()
    };
    assert_eq!(
        said_to(&mut session),
        None,
        "a turn with no goal was built with a condition"
    );

    session.start_goal("cargo test exits 0".to_string());
    assert_eq!(
        said_to(&mut session),
        Some("cargo test exits 0".to_string()),
        "a turn under a goal was built without its condition"
    );

    session.pause_goal();
    assert!(session.goal().is_some(), "pausing ended the goal");
    assert_eq!(
        said_to(&mut session),
        None,
        "a turn under a paused goal was built with its condition"
    );

    session.resume_goal();
    assert_eq!(
        said_to(&mut session),
        Some("cargo test exits 0".to_string()),
        "a resumed goal was not carried on the task"
    );
}

/// And the turn is asked before it runs, not only answered afterwards. A turn whose wait is
/// going to be discarded and which is offered a way to ask for one is told back that a later
/// look is arranged and needs nothing from the person, and that sentence is what it writes
/// into the answer: the person reads that a watch exists, and nothing is watching.
///
/// The four refusals are the four the session makes once the turn has ended. The line has to
/// be the person's, since a loop repeats a line somebody endorsed; and the session has to
/// have nothing of its own already running, since it does one of a watch, a loop and a goal
/// at a time, and a wait arriving under any of the three is dropped.
#[test]
fn only_a_turn_whose_wait_would_be_kept_is_offered_a_later_look() {
    let idle = Session::new("none");
    assert!(
        will_look_again(&idle, Wrote::ThePerson),
        "a turn on the person's own line, with the session idle, was offered nothing"
    );
    assert!(
        !will_look_again(&idle, Wrote::TheDriver),
        "a sentence this program wrote was offered a loop over itself"
    );

    let mut init = Session::new("none");
    assert!(
        init.start_init().is_some(),
        "the fixture's workspace already had an AGENTS.md"
    );
    assert!(
        !will_look_again(&init, Wrote::ThePerson),
        "a /init turn was offered a loop over the prompt this program wrote"
    );
    let mut review = Session::new("none");
    review.start_review(
        &crate::review_command::Asked {
            target: crate::review_command::Target::Uncommitted,
            focus: None,
        },
        crate::state::NamesFrom::nothing(),
    );
    assert!(
        !will_look_again(&review, Wrote::ThePerson),
        "a /review turn was offered a loop over the prompt this program wrote"
    );

    let mut looping = Session::new("none");
    looping.start_loop(
        crate::loops::request("5m watch the build"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    assert!(
        !will_look_again(&looping, Wrote::ThePerson),
        "a line typed during a loop was offered a second one"
    );

    let mut goal = Session::new("none");
    goal.start_goal("cargo test exits 0".to_string());
    assert!(
        !will_look_again(&goal, Wrote::ThePerson),
        "a turn under a goal was offered a loop the goal will refuse"
    );

    let mut watching = Session::new("none");
    watching.arm_watch(
        "a.txt",
        ".",
        bravebot_agent::watch::Looked::Saw("a token".to_string()),
    );
    assert!(
        !will_look_again(&watching, Wrote::ThePerson),
        "a turn under a standing watch was offered a loop the watch will refuse"
    );
}

/// Any turn cancelled while a loop runs ends the loop, and says so (LOOP-11): here the tick
/// itself is the turn in flight. Left running, the loop would be neither due nor ended, and
/// the next line the person typed would go out as a tick of it.
#[test]
fn stopping_a_tick_ends_the_loop_and_says_so() {
    let mut session = Session::new("none");
    session.start_loop(
        crate::loops::request("5m watch the build"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    assert_eq!(
        session.status,
        Status::Working,
        "the first tick is not in flight"
    );

    stop_through_finish_turn(&mut session, "watch the build");

    assert!(
        session.looping().is_none(),
        "the stop left the loop running"
    );
    assert_eq!(
        session
            .transcript
            .iter()
            .filter(|entry| entry.text == t!(loop_stopped))
            .count(),
        1,
        "the ending was not announced exactly once"
    );
}

/// And so does the person's own prompt typed in the middle of a loop, which is not a tick.
#[test]
fn stopping_a_prompt_typed_during_a_loop_ends_the_loop_and_says_so() {
    let mut session = Session::new("none");
    session.start_loop(
        crate::loops::request("5m watch the build"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    session.complete("done", Vec::new(), 0);
    session.loop_turn_ended(None);
    assert!(session.looping().is_some(), "the loop ended with its tick");
    type_line(&mut session, "something else");
    session.submit().unwrap();

    stop_through_finish_turn(&mut session, "something else");

    assert!(
        session.looping().is_none(),
        "stopping the person's own turn left the loop running"
    );
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(loop_stopped)),
        "the ending was not announced"
    );
}

/// Stopping the turn a watch's fire started ends that watch, and says so. The state method is
/// tested alone; this is the wiring, which is what a person's key press reaches.
#[test]
fn stopping_a_fires_turn_through_the_driver_ends_the_watch_that_fired() {
    let mut session = Session::new("none");
    let saw = |token: &str| bravebot_agent::watch::Looked::Saw(token.to_string());
    session.arm_watch("notes.md", ".", saw("first"));
    session
        .watch_fired(Instant::now() + Duration::from_secs(6), |_, _| {
            saw("second")
        })
        .expect("a fire");
    assert!(session.watch_is_firing());
    stop_through_finish_turn(&mut session, "");

    assert!(
        session.watches().is_empty(),
        "stopping a fire's turn left the watch that fired running"
    );
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(watch_stopped_with_its_turn, number = 1)),
        "the watch ended in silence"
    );
}

/// A goal is a condition for a session and not for one turn, so stopping a turn going the
/// wrong way has to leave it: a person who has to retype the condition every time they
/// interrupt cannot steer the work at all. The stopped turn is recorded as stopped, which is
/// what keeps it from being judged and sent straight back: there is no answer to judge.
#[test]
fn stopping_a_turn_leaves_the_goal_set() {
    let mut session = Session::new("none");
    session.start_goal("cargo test exits 0".to_string());
    stop_through_finish_turn(&mut session, "");

    assert_eq!(
        session.goal().map(crate::goals::Running::condition),
        Some("cargo test exits 0"),
        "the stop took the goal off with the turn"
    );
    assert_eq!(
        session.finished.map(|turn| turn.ending),
        Some(bravebot_agent::Ending::Stopped { attempts: None }),
        "a stop was not recorded as one"
    );
    assert!(
        session
            .finished
            .is_some_and(|turn| turn.ending.unanswered()),
        "a stopped turn looked answered, so it would be judged"
    );
}

/// A request that never came back says nothing about whether the work is finished, so the
/// goal waits for a turn there is something to judge rather than being thrown away.
#[test]
fn a_turn_that_failed_leaves_the_goal_where_it_was() {
    let mut session = Session::new("none");
    session.start_goal("cargo test exits 0".to_string());
    let asked = Asked {
        name: "test-model".to_string(),
        comparable: true,
        configured: "test-model".to_string(),
    };

    fold_outcome(
        &mut session,
        Err(turn::TurnError::Precommit("failed".to_string())),
        Trail::new(),
        Carried {
            trust: TrustStore::new("/work"),
            programs: TrustedPrograms::new(),
            asked: AskedAbout::new(),
            exposed: bravebot_core::credentials::Exposed::new(),
        },
        Occupied {
            budget: 100_000,
            guessed: false,
            last_request_tokens: 0,
        },
        asked,
        Line {
            text: "",
            addressed: None,
            offered_a_later_look: false,
        },
        &workspace_for_test(),
    );

    assert!(
        session.goal().is_some(),
        "one failed request ended the goal"
    );
}
