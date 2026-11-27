//! Prompts and keys while a turn runs: queueing, hurrying, stopping and what is waiting.

use super::*;

/// A drop reaches the terminal as a paste, so a working loop that only pasted wrote the path
/// out as prose: the file was never staged, and the line said nothing about an attachment.
/// A turn in flight is when people drop a file, because they are reading a reply and want to
/// hand over the picture it is about.
#[test]
fn a_file_dropped_while_a_turn_is_running_is_attached() {
    let directory = crate::testutil::scratch_dir("bravebot-app-drop-while-working");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch");
    let file = directory.join("shot.png");
    std::fs::write(&file, [0x89u8, 0x50]).expect("write");

    let mut session = Session::new("none").in_workspace(&directory);
    session.type_char('a');
    session.submit();
    handle_paste(&mut session, &file.to_string_lossy());

    assert_eq!(session.input(), "[Image #1] ");
    assert_eq!(session.attached().len(), 1, "nothing was staged");
    assert_eq!(session.attached()[0].name, "shot.png");

    let _ = std::fs::remove_dir_all(&directory);
}

/// A counted row key moves rows inside the input and stops at the first or the last, where the
/// bare key walks the prompt history once the input runs out. Answering a count by pressing the
/// key it spells that many times would replace a two-row paragraph with a prompt from three
/// back, with nothing on the screen to say what took the line away.
#[test]
fn a_counted_row_key_never_reaches_the_prompt_history() {
    let mut session = editing_vis_way();
    type_line(&mut session, "an earlier prompt");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("an answer", Vec::new(), 0);
    // One row, so the row above is the history and nothing else: a counted key answered by the
    // one it spells would recall the prompt on the first of its steps.
    type_line(&mut session, "the line being written");
    handle_key(&mut session, key(KeyCode::Esc));

    handle_key(&mut session, key(KeyCode::Char('3')));
    handle_key(&mut session, key(KeyCode::Char('k')));

    assert_eq!(
        session.input(),
        "the line being written",
        "the count reached the history"
    );

    // The bare key is unchanged, so the ladder INPUT-27 describes is still there to walk.
    handle_key(&mut session, key(KeyCode::Char('k')));
    assert_eq!(
        session.input(),
        "an earlier prompt",
        "the bare key stopped reaching the history"
    );
}

/// The list is documentation, and a turn in flight refuses sending and nothing else. The key
/// used to set the flag with the list refused a place to be drawn, so the press did nothing on
/// screen and the list came up unasked when the turn ended, attached to no press at all.
#[test]
fn a_question_mark_lists_the_keys_while_a_turn_runs() {
    let mut session = Session::new("none");
    type_line(&mut session, "anything");
    session.submit().expect("the prompt is sent");

    handle_key_while_working(&mut session, key(KeyCode::Char('?')));
    assert!(session.shortcuts, "the list did not come up");
    assert!(session.input().is_empty(), "the marker was typed");
    assert_eq!(
        session.offered(),
        crate::state::Offered::Shortcuts,
        "the list had nowhere to be drawn"
    );

    handle_key_while_working(&mut session, key(KeyCode::Char('?')));
    assert!(!session.shortcuts, "the list did not go down again");
}

/// Rejects: a mid-turn Tab that is dropped, or one that leaves the line short of the name.
#[test]
fn tab_completes_a_half_typed_command_while_a_turn_runs() {
    let mut session = a_turn_running_on("anything");
    type_without_sending_while_working(&mut session, "/ef");

    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Tab)),
        Action::Redraw
    );

    assert_eq!(session.input(), "/effort ");
    assert_eq!(session.commands_waiting().count(), 0, "Tab queued the line");
}

/// Rejects: the arrows being dropped mid-turn, so the row Enter completes could not be chosen.
#[test]
fn the_arrows_choose_the_row_while_a_turn_runs() {
    let mut session = a_turn_running_on("anything");
    type_without_sending_while_working(&mut session, "/e");
    let first = session.highlighted_completion();

    handle_key_while_working(&mut session, key(KeyCode::Down));
    assert_ne!(session.highlighted_completion(), first, "Down did not move");

    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_ne!(session.input(), "/e", "Enter did not take the row");
    assert!(session.input().starts_with("/e"));
}

/// Rejects: a compaction or an aside, where `a_turn_is_running` is false, offering its
/// commands as a mid-turn arm would, when every one of them waits there.
#[test]
fn commands_are_offered_during_an_aside_as_well() {
    let mut session = Session::new("none");
    session.begin_aside();

    type_without_sending_while_working(&mut session, "/");

    assert!(matches!(
        session.offered(),
        crate::state::Offered::Slash { .. }
    ));
}

/// INPUT-9. Rejects: the working path leaving the flag set after the box was emptied, so the
/// next prompt, pressed on once the turn ended, started a new session instead of being sent.
#[test]
fn a_prompt_typed_mid_turn_after_wiping_the_brief_is_sent_as_a_prompt() {
    let mut session = a_turn_running_over_a_brief("the brief");
    handle_key_while_working(
        &mut session,
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
    );
    assert!(session.input().is_empty());
    for c in "something else".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    session.status = Status::Idle;

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("something else".to_string())
    );
}

/// INPUT-9. Rejects: a paste mid-turn not ending a wiped brief, as a paste at rest does.
#[test]
fn a_prompt_pasted_mid_turn_after_wiping_the_brief_is_sent_as_a_prompt() {
    let mut session = a_turn_running_over_a_brief("the brief");
    handle_key_while_working(
        &mut session,
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
    );
    assert!(session.input().is_empty());
    handle_paste(&mut session, "something else");
    session.status = Status::Idle;

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("something else".to_string())
    );
}

/// The trail is drawn per entry as a turn fills in, so it is what somebody watching a turn call
/// tools wants. Refused mid-turn, the key was swallowed by the catch-all for control chords
/// while the hint line went on advertising it on every frame of that turn.
#[test]
fn the_trail_can_be_asked_for_while_a_turn_runs() {
    let mut session = Session::new("none");
    type_line(&mut session, "anything");
    session.submit().expect("the prompt is sent");

    assert_eq!(
        handle_key_while_working(&mut session, ctrl('t')),
        Action::Redraw
    );
    assert!(session.show_trail, "the key was swallowed");
    assert_eq!(session.input(), "", "a stray character was typed");
}

/// With nothing sent yet, the arrows scroll: there is no history to walk.
#[test]
fn arrow_keys_scroll_when_there_is_no_history() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.scroll, 1);
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.scroll, 2);
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(session.scroll, 1);
}

/// Once something has been sent, Up recalls it, which is what a shell does and so what a
/// user expects at a prompt.
#[test]
fn up_recalls_a_previous_prompt() {
    let mut session = Session::new("none");
    for c in "first question".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("an answer", Vec::new(), 0);

    assert_eq!(handle_key(&mut session, key(KeyCode::Up)), Action::Redraw);
    assert_eq!(session.input(), "first question");
    assert_eq!(session.scroll, 0, "recall scrolled the transcript as well");
}

/// The keys that walk the history do it whether or not a turn is running. They reached no
/// arm at all mid-turn and fell through to nothing, so a person watching a turn go wrong
/// could see their last prompt in the transcript and had no way to get it back into the box.
#[test]
fn up_recalls_a_previous_prompt_while_a_turn_is_running() {
    let mut session = Session::new("none");
    for c in "first question".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    // No `complete`: the turn is still in flight.
    assert_eq!(session.status, Status::Working);

    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Up)),
        Action::Redraw
    );
    assert_eq!(session.input(), "first question");

    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Down)),
        Action::Redraw
    );
    assert!(session.input().is_empty(), "Down did not walk back out");
}

/// One press is its life whatever is running. A turn can begin without anybody pressing
/// anything, so an offer put up while the session was idle outlives the moment it answered,
/// and it says the next Ctrl-C leaves when mid-turn that key stops the turn.
#[test]
fn the_way_out_stops_being_offered_at_the_next_press_while_a_turn_runs() {
    let mut session = Session::new("none");
    type_line(&mut session, "half a thought");
    handle_key(&mut session, ctrl('c'));
    assert!(session.cleared_by_interrupt);

    // A tick submits from the main loop rather than from a key, which is how the offer
    // reaches a running turn at all.
    session.start_loop(
        crate::loops::request("30s check the deploy"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    assert_eq!(session.status, Status::Working);
    assert!(session.cleared_by_interrupt, "the tick took the offer down");

    handle_key_while_working(&mut session, key(KeyCode::Char('x')));
    assert!(
        !session.cleared_by_interrupt,
        "the hint outstayed its press"
    );
}

/// Enter mid-turn used to reach nothing, so the line sat in the box while the person waited
/// to notice the turn had ended. It goes now, and says that it is waiting.
#[test]
fn enter_queues_a_prompt_while_a_turn_is_running() {
    let mut session = Session::new("none");
    for c in "first".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in "second".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert!(session.input().is_empty(), "the line stayed in the box");
    assert_eq!(session.queued.len(), 1);
    assert_eq!(session.queued[0].prompt, "second");
}

/// The point of queueing, and what it used to fail at: a prompt typed mid-turn is put where the
/// running turn can reach it, not left for a turn that may be minutes away. A person who says
/// "no, not that file" while an agent works is trying to redirect the work in front of them,
/// and an instruction that waits for the answer arrives after the thing it was meant to change.
#[test]
fn a_prompt_queued_mid_turn_is_within_the_running_turns_reach() {
    let mut session = Session::new("none");
    for c in "first".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));

    // What the worker holds. Taken before the prompt is queued, exactly as a turn takes it.
    let reaching = session.interjections();
    assert!(reaching.take().is_none(), "something was waiting already");

    for c in "actually stop".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert_eq!(
        reaching.take().as_deref(),
        Some("actually stop"),
        "the turn could not reach a prompt typed while it ran"
    );
}

/// Until the turn takes it, a queued prompt has not been said: it is drawn above the box as
/// waiting, and the transcript is for what has happened. It joins the transcript at the moment
/// the planner is given it, which is what keeps the two reading in the same order.
#[test]
fn a_queued_prompt_joins_the_transcript_when_the_planner_is_given_it() {
    let mut session = Session::new("none");
    for c in "first".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    for c in "second".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    let said = session
        .transcript
        .iter()
        .filter(|entry| entry.speaker == crate::state::Speaker::User)
        .count();
    assert_eq!(said, 1, "a prompt that had not gone anywhere was recorded");

    session.interjected();
    let said: Vec<&str> = session
        .transcript
        .iter()
        .filter(|entry| entry.speaker == crate::state::Speaker::User)
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(said, vec!["first", "second"]);
    assert!(session.queued.is_empty(), "it is still drawn as waiting");
}

/// A prompt still waiting when the turn ends is a turn of its own, as every queued prompt used
/// to be. What must not happen is both: the copy left for the turn that has finished has to go,
/// or the same words reach the planner twice, the second time as an interjection into the very
/// turn they started.
#[test]
fn a_prompt_that_outlived_the_turn_is_sent_once() {
    let mut session = Session::new("none");
    for c in "first".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    for c in "second".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    let reaching = session.interjections();
    session.complete("done", Vec::new(), 0);
    assert_eq!(session.send_queued().as_deref(), Some("second"));
    assert!(
        reaching.take().is_none(),
        "the prompt was sent as a turn and left waiting to be interjected as well"
    );
}

/// Ctrl-Enter does to the line what Enter does and then asks for the turn to stop. Enter must
/// never ask that: somebody queueing a follow-up is not asking to lose the answer being written.
#[test]
fn ctrl_enter_queues_the_line_and_stops_the_turn_where_enter_only_queues() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));

    for c in "a".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Redraw,
        "Enter stopped the turn"
    );
    for c in "b".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, ctrl_enter()),
        Action::SendNow
    );
    assert_eq!(session.input(), "", "the line was left in the box");
    assert_eq!(
        session
            .queued
            .iter()
            .map(|line| line.prompt.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
}

/// The turn loop's half of it: the press that asks for the turn to stop stops it, and Enter
/// beside it leaves the turn running.
#[test]
fn ctrl_enter_stops_the_turn_in_flight_and_enter_does_not() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    let cancel = Cancel::new();

    for c in "a".chars() {
        pressed_during_a_turn(&mut session, key(KeyCode::Char(c)), &cancel);
    }
    pressed_during_a_turn(&mut session, key(KeyCode::Enter), &cancel);
    assert!(!cancel.is_cancelled(), "Enter stopped the turn");

    pressed_during_a_turn(&mut session, ctrl_enter(), &cancel);
    assert!(cancel.is_cancelled(), "Ctrl-Enter left the turn running");
}

/// A turn asked to stop takes nothing more, and the interface hears that it has stopped only
/// when it gets there. A prompt sent in between is drawn as going into the turn unless the stop
/// is recorded at the press that asked for it.
#[test]
fn a_stop_is_recorded_at_the_press_that_asks_for_it() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    let cancel = Cancel::new();
    for c in "second".chars() {
        pressed_during_a_turn(&mut session, key(KeyCode::Char(c)), &cancel);
    }
    pressed_during_a_turn(&mut session, key(KeyCode::Enter), &cancel);
    assert_eq!(session.where_it_goes(0), crate::state::Bound::IntoThisTurn);

    stop_what_is_running(&mut session, &cancel);

    assert_eq!(
        session.where_it_goes(0),
        crate::state::Bound::ItsOwnTurn,
        "still drawn as going into the turn being stopped"
    );
}

/// Over an empty box there is nothing to queue, and what is already waiting can go all the same.
/// A press that did nothing there would leave somebody who queued with Enter no way to hurry it.
#[test]
fn ctrl_enter_over_an_empty_box_sends_what_is_already_waiting() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    for c in "a".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert_eq!(
        handle_key_while_working(&mut session, ctrl_enter()),
        Action::SendNow
    );
}

/// With nothing waiting and nothing typed there is nothing to send, so stopping the turn would
/// be Escape under another name, and a person reaching for Enter who caught Ctrl too would lose
/// the answer for nothing.
#[test]
fn ctrl_enter_with_nothing_to_send_leaves_the_turn_running() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));

    assert_ne!(
        handle_key_while_working(&mut session, ctrl_enter()),
        Action::SendNow
    );
    assert_eq!(session.status, Status::Working);
}

/// A compaction, a goal check or a manifest is not a turn, and nothing waiting can go before one
/// has ended anyway. Ctrl-Enter there queues the line as Enter does and stops nothing: a
/// compaction stopped half way is work thrown away for no prompt sent any sooner.
#[test]
fn ctrl_enter_during_a_single_request_only_queues() {
    let mut session = having_sent(&["first question"]);
    session.begin_aside();
    for c in "a".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key_while_working(&mut session, ctrl_enter()),
        Action::Redraw
    );
    assert_eq!(session.queued.len(), 1, "the line was not queued");
    assert_eq!(session.status, Status::Working);
}

/// The whole of what the chord promises, through the stop arm the event loop reaches: the turn
/// ends, and the prompts that were waiting go as one turn rather than one after another. Nothing
/// may reach the planner twice on the way, neither as an interjection left behind in the buffer
/// nor as a second turn.
#[test]
fn what_ctrl_enter_hurried_goes_as_one_turn_once_the_turn_stops() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    let reaching = session.interjections();
    for c in "a".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    for c in "b".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, ctrl_enter()),
        Action::SendNow
    );

    finish_cancelled_turn(&mut session, "first", Some(0));

    assert_eq!(
        queued_next(&mut session),
        Some(Action::Submit("a\nb".to_string()))
    );
    assert!(
        session.queued.is_empty(),
        "a hurried prompt was left waiting"
    );
    assert!(
        reaching.take().is_none(),
        "a prompt sent in the turn was left to be interjected into it as well"
    );
    assert_eq!(
        session
            .transcript
            .iter()
            .filter(|entry| entry.speaker == crate::state::Speaker::User)
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>(),
        ["first", "a\nb"]
    );
}

/// Three prompts queued during one turn, and the rule that governs them afterwards: the running
/// turn takes what it can, one becomes the next turn, and the rest go on waiting under the same
/// rule. What must hold throughout is that the queue on the screen and the buffer the turn takes
/// from never disagree about how many prompts are waiting.
#[test]
fn what_is_still_waiting_stays_in_step_with_what_is_drawn() {
    let mut session = Session::new("none");
    session.type_char('a');
    session.submit();
    let reaching = session.interjections();

    for line in ["one", "two", "three"] {
        for c in line.chars() {
            session.type_char(c);
        }
        assert!(session.queue(), "nothing was queued");
    }

    // The turn takes the first, as it would at a round boundary.
    assert_eq!(reaching.take().as_deref(), Some("one"));
    session.interjected();
    assert_eq!(session.queued.len(), 2);

    // Then it ends with two still waiting. The oldest becomes a turn of its own.
    session.complete("an answer", Vec::new(), 0);
    assert_eq!(session.send_queued().as_deref(), Some("two"));
    assert_eq!(session.queued.len(), 1);

    // And the last is waiting for the turn that just began, not for a second copy of itself.
    assert_eq!(reaching.take().as_deref(), Some("three"));
    assert!(reaching.take().is_none(), "a prompt was waiting twice over");
}

/// Up reaches for the last thing the person said, and while prompts are waiting that is the
/// queue. It walked the history instead, which holds a copy of every queued line: the copy
/// came back, the person rewrote it, and the prompt they meant to take back went anyway.
///
/// One press for the whole queue, not one press each. A key that gave them back a line at a
/// time would leave the person pressing it until they guessed there were none left, with the
/// ones they had already taken back sitting in the box in front of them.
#[test]
fn up_takes_back_everything_waiting_rather_than_a_copy_of_it() {
    let mut session = Session::new("none");
    for c in "first".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    for line in ["second", "third"] {
        for c in line.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));
    }
    assert_eq!(session.queued.len(), 2);

    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Up)),
        Action::Redraw
    );
    assert_eq!(session.input(), "second\nthird");
    assert!(session.queued.is_empty(), "a prompt was left waiting");
}

/// Taking a prompt back has to take it out of the turn's reach too, or the key reads as having
/// done nothing: the line comes back to the box, the person rewrites it, and the copy the turn
/// was still holding arrives at the planner anyway. Which is the bug the old queue could not
/// have, because nothing could reach a queued prompt until the turn was over.
#[test]
fn taking_the_queue_back_takes_it_out_of_the_turns_reach() {
    let mut session = Session::new("none");
    for c in "first".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    let reaching = session.interjections();
    for c in "second".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    handle_key_while_working(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "second");
    assert!(
        reaching.take().is_none(),
        "a prompt taken back was still on its way to the planner"
    );
}

/// The other side of it: a prompt the turn has already been given cannot be taken back, because
/// the planner has it. Offering it to the box would leave the person editing a line that had
/// gone, and pressing Enter would say it twice.
#[test]
fn a_prompt_the_turn_has_taken_cannot_be_taken_back() {
    let mut session = Session::new("none");
    for c in "first".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    let reaching = session.interjections();
    for c in "second".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    // The turn takes it, as it would at its next round boundary.
    assert_eq!(reaching.take().as_deref(), Some("second"));

    handle_key_while_working(&mut session, key(KeyCode::Up));
    assert!(
        !session.input().contains("second"),
        "a prompt the planner already had came back to the box: {}",
        session.input()
    );
}

/// With nothing waiting the key means what it has always meant. Taking the queue back is the
/// exception, and it lasts exactly as long as there is a queue.
#[test]
fn up_walks_the_history_again_once_nothing_is_waiting() {
    let mut session = Session::new("none");
    for c in "first".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    for c in "second".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    handle_key_while_working(&mut session, key(KeyCode::Up));
    for _ in 0.."second".len() {
        handle_key_while_working(&mut session, key(KeyCode::Backspace));
    }
    assert_eq!(session.input(), "");

    handle_key_while_working(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "second", "the history was out of reach");
}

/// A paragraph in the box has rows of its own, and Up moves between them first. Taking the
/// queue back from the bottom row would pull the waiting prompts in whenever the person only
/// reached for the row above, and from the top row there is nowhere left to go but the queue.
#[test]
fn up_reaches_the_queue_only_from_the_top_row_of_a_paragraph() {
    let mut session = Session::new("none");
    for c in "first".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    for c in "waiting".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    for c in "top".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(
        &mut session,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
    );
    for c in "bottom".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(session.input(), "top\nbottom");
    assert_eq!(session.queued.len(), 1);

    handle_key_while_working(&mut session, key(KeyCode::Up));
    assert_eq!(
        session.queued.len(),
        1,
        "Up took the queue back from the bottom row of a paragraph"
    );
    assert_eq!(session.input(), "top\nbottom");

    handle_key_while_working(&mut session, key(KeyCode::Up));
    assert!(
        session.queued.is_empty(),
        "Up did not reach the queue from the top row"
    );
    assert_eq!(session.input(), "waiting\ntop\nbottom");
}

/// Shift-Enter writes a paragraph mid-turn as it does at rest, so it must not be caught by
/// the arm that queues. A queued half-sentence is worse than one that waits to be finished.
#[test]
fn starting_a_line_mid_turn_does_not_queue_it() {
    let mut session = Session::new("none");
    session.type_char('a');
    session.submit();

    for c in "half".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(
        &mut session,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
    );

    assert!(
        session.queued.is_empty(),
        "a paragraph was sent half-written"
    );
    assert!(session.input().contains('\n'), "no line was started");
}

/// Why a key is allowed to mean something different while a turn runs, or `None` where it is
/// not.
///
/// The whole of the difference between the two paths, in one place. Every one of them is about
/// sending or about leaving, which are the two things the box does not decide.
fn allowed_to_differ(key: KeyEvent) -> Option<&'static str> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        // Shift-Enter and Ctrl-J start a line and are not this key.
        KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => Some("sends"),
        KeyCode::Esc => Some("stops the turn in flight"),
        KeyCode::Char('c') if ctrl => Some("stops the turn in flight, and then leaves"),
        KeyCode::Char('d') if ctrl => Some("leaves, which is not something the box does"),
        KeyCode::Char('g') if ctrl => Some("hands the screen the turn is drawing on to an editor"),
        KeyCode::Char('p') if ctrl => {
            Some("rewrites the line over the exchange, which a turn in flight holds")
        }
        KeyCode::Char('!') => Some("arms a mode that changes what Enter does"),
        _ => None,
    }
}

/// Every key the two paths could see, so a binding added to one and not the other is caught
/// here rather than by somebody pressing it.
fn every_key() -> Vec<KeyEvent> {
    let codes = (0x20u8..=0x7e)
        .map(|byte| KeyCode::Char(char::from(byte)))
        .chain([
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Insert,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Esc,
            KeyCode::F(1),
        ]);
    let modifiers = [
        KeyModifiers::NONE,
        KeyModifiers::CONTROL,
        KeyModifiers::SHIFT,
        KeyModifiers::ALT,
    ];
    codes
        .flat_map(|code| modifiers.iter().map(move |held| KeyEvent::new(code, *held)))
        .collect()
}

/// The two paths answer the same set of keys, and the ones they are allowed to disagree about
/// are named rather than discovered.
///
/// Asserted as a set because a list was the bug. This walked six key codes, and three keys that
/// send nothing were answered by the idle path alone: none of the three was in the list, so the
/// test that was supposed to pin the agreement said nothing whatever about any of them.
#[test]
fn the_two_paths_answer_the_same_set_of_keys() {
    /// Everything a key press can reach that is not about sending, so a difference in any of it
    /// is a difference the clause forbids.
    fn answered(session: &Session, action: Action) -> String {
        format!(
            "{action:?} input={:?} caret={} scroll={} shell={} shortcuts={} trail={} \
                 stashed={:?} scrolling={} queued={} browsing={} searching={} vi={:?} \
                 interrupted={}",
            session.input(),
            session.caret(),
            session.scroll,
            session.shell,
            session.shortcuts,
            session.show_trail,
            session.stashed(),
            session.scrolling(),
            session.queued.len(),
            session.history.is_browsing(),
            session.searching_history(),
            session.vi_mode(),
            session.cleared_by_interrupt,
        )
    }

    let sent = |finished: bool, editing: crate::vim::Editing, typed: &str| {
        let mut session = Session::new("none");
        session.choose_editing(editing);
        type_line(&mut session, "first question");
        handle_key(&mut session, key(KeyCode::Enter));
        // The same transcript either way, so only the running turn differs.
        if finished {
            session.complete("an answer", Vec::new(), 0);
        } else {
            session.narrate("an answer");
        }
        // Several arms are read against whether there is a line, so both cases are swept.
        for c in typed.chars() {
            if finished {
                handle_key(&mut session, key(KeyCode::Char(c)));
            } else {
                handle_key_while_working(&mut session, key(KeyCode::Char(c)));
            }
        }
        // The offer to leave, standing over both sessions as it stands over a turn a tick
        // began: put up by a Ctrl-C that took a line while nothing was running, and carried
        // into the turn by a submission no key press made. It lives for one press, so the
        // press under test takes it down whichever path answers that press.
        session.cleared_by_interrupt = true;
        session
    };

    // Both styles of editing, and in the vi one every mode it has. A key wired into one path and
    // not the other is the bug this catches, and vi editing multiplies the places to wire one:
    // each mode gives every printable character a meaning of its own, so a letter answered by the
    // idle path alone would be a letter that silently did nothing during a turn.
    let modes = [
        None,
        Some(crate::vim::Mode::Normal),
        Some(crate::vim::Mode::Visual { lines: false }),
        Some(crate::vim::Mode::Visual { lines: true }),
        Some(crate::vim::Mode::Replace),
    ];
    for editing in crate::vim::Editing::ALL {
        for mode in modes {
            if mode.is_some() != (editing == crate::vim::Editing::Vi) {
                continue;
            }
            for line in ["", "half a thought"] {
                for pressed in every_key() {
                    if allowed_to_differ(pressed).is_some() {
                        continue;
                    }
                    let mut idle = sent(true, editing, line);
                    let mut working = sent(false, editing, line);
                    for session in [&mut idle, &mut working] {
                        match mode {
                            None => {}
                            Some(crate::vim::Mode::Visual { lines }) => {
                                session.enter_vi_normal();
                                session.type_char(if lines { 'V' } else { 'v' });
                            }
                            Some(crate::vim::Mode::Replace) => {
                                session.enter_vi_normal();
                                session.type_char('R');
                            }
                            Some(_) => {
                                session.enter_vi_normal();
                            }
                        }
                    }
                    assert_eq!(working.status, Status::Working);
                    assert_eq!(
                        answered(&idle, Action::None),
                        answered(&working, Action::None),
                        "the two sessions differed before {pressed:?} was pressed"
                    );

                    let at_rest = handle_key(&mut idle, pressed);
                    let mid_turn = handle_key_while_working(&mut working, pressed);

                    assert_eq!(
                        answered(&idle, at_rest),
                        answered(&working, mid_turn),
                        "{pressed:?} was answered differently while a turn was running, \
                             over a line of {line:?}, editing {editing:?} in {mode:?}"
                    );
                }
            }
        }
    }
}

/// Down walks back out of history, restoring the line that was being typed.
#[test]
fn down_returns_from_history_to_the_typed_line() {
    let mut session = Session::new("none");
    for c in "sent".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("ok", Vec::new(), 0);

    for c in "being typed".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "sent");

    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(
        session.input(),
        "being typed",
        "the half-typed line was not restored"
    );
}

/// Typing over a recalled prompt makes it the working line, so the position label goes away.
#[test]
fn typing_leaves_history_browsing() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('a')));
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("ok", Vec::new(), 0);

    handle_key(&mut session, key(KeyCode::Up));
    assert!(session.history.is_browsing());
    handle_key(&mut session, key(KeyCode::Char('b')));
    assert!(!session.history.is_browsing());
}

/// Escape leaves history along with clearing, so it cannot be left labelled with an empty box.
#[test]
fn escape_leaves_history_browsing() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('a')));
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("ok", Vec::new(), 0);

    handle_key(&mut session, key(KeyCode::Up));
    assert!(session.history.is_browsing());
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(!session.history.is_browsing());
    assert!(session.input().is_empty());
}

/// Escape clears in one press, so a slip loses the paragraph unless Up can bring it back. Nothing
/// has been sent, so the person has not been asked to send anything to get it.
#[test]
fn escape_keeps_the_cleared_line_for_up() {
    let mut session = Session::new("none");
    type_line(&mut session, "half a thought");
    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(session.input(), "");

    assert_eq!(handle_key(&mut session, key(KeyCode::Up)), Action::Redraw);
    assert_eq!(session.input(), "half a thought");
    assert_eq!(session.status, Status::Idle, "recalling the draft sent it");
    assert!(session.history.is_empty(), "the draft became a sent prompt");

    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(session.input(), "", "Down did not leave the draft");
}

/// The first rung of Ctrl-C takes the line, and takes it by the same route as Escape.
#[test]
fn ctrl_c_keeps_the_cleared_line_for_up() {
    let mut session = Session::new("none");
    type_line(&mut session, "half a thought");
    handle_key(&mut session, ctrl('c'));
    assert_eq!(session.input(), "");

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "half a thought");
}

/// One place to keep a cleared line, so the latest is what Up brings back first and the
/// earlier one is gone rather than stacked behind it.
#[test]
fn a_second_clear_replaces_the_kept_draft() {
    let mut session = having_sent(&["sent before"]);
    type_line(&mut session, "first draft");
    handle_key(&mut session, key(KeyCode::Esc));
    type_line(&mut session, "second draft");
    handle_key(&mut session, ctrl('c'));

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "second draft");
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(
        session.input(),
        "sent before",
        "the first draft was kept behind the second"
    );
}

/// A draft belongs to the line that was being written when it was cleared. Once a prompt goes,
/// Up walks the sent prompts alone.
#[test]
fn sending_a_prompt_drops_the_kept_draft() {
    let mut session = having_sent(&["sent before"]);
    type_line(&mut session, "abandoned");
    handle_key(&mut session, key(KeyCode::Esc));
    type_line(&mut session, "sent after");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("ok", Vec::new(), 0);

    let mut seen = Vec::new();
    for _ in 0..4 {
        handle_key(&mut session, key(KeyCode::Up));
        seen.push(session.input().to_string());
    }
    assert_eq!(
        seen,
        ["sent after", "sent before", "sent before", "sent before"],
        "the draft survived a send"
    );
}

/// Clearing a prompt that was only walked back to loses nothing, since the history has it, and
/// must not push out a draft that was lost.
#[test]
fn clearing_a_recalled_prompt_does_not_replace_the_draft() {
    let mut session = having_sent(&["sent before"]);
    type_line(&mut session, "abandoned");
    handle_key(&mut session, key(KeyCode::Esc));
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "sent before");

    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(session.input(), "");
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "abandoned");
}

/// Clearing a box with nothing to lose keeps nothing, so Up does not bring back blanks.
#[test]
fn clearing_an_empty_or_blank_line_keeps_no_draft() {
    let mut session = Session::new("none");
    type_line(&mut session, "a real draft");
    handle_key(&mut session, key(KeyCode::Esc));
    type_line(&mut session, "   ");
    handle_key(&mut session, key(KeyCode::Esc));
    handle_key(&mut session, key(KeyCode::Esc));

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "a real draft");
}

/// The draft is not a sent prompt, so the search over what was sent does not offer it.
#[test]
fn the_kept_draft_is_not_a_candidate_for_the_search() {
    let mut session = having_sent(&["sent before"]);
    type_line(&mut session, "abandoned");
    handle_key(&mut session, key(KeyCode::Esc));

    handle_key(&mut session, ctrl('r'));
    let offered: Vec<_> = session
        .history_matches()
        .iter()
        .map(|entry| entry.prompt.clone())
        .collect();
    assert_eq!(offered, ["sent before"]);
}

/// The draft in the box is the person's own typing, so Ctrl-S puts it away like any other line
/// and does not open a search the way it does on a stored prompt.
#[test]
fn ctrl_s_on_the_recalled_draft_puts_it_away() {
    let mut session = having_sent(&["sent before"]);
    type_line(&mut session, "abandoned");
    handle_key(&mut session, key(KeyCode::Esc));
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "abandoned");

    handle_key(&mut session, ctrl('s'));
    assert!(!session.searching_history());
    assert_eq!(session.input(), "");
    assert_eq!(session.stashed(), Some("abandoned"));
}

/// A session with two prompts stored by earlier sessions and one of its own.
fn with_earlier_sessions() -> Session {
    let mut session = Session::new("none");
    session.history = crate::history::History::from_entries(vec![
        bravebot_session::store::Entry::sent("old one", None),
        bravebot_session::store::Entry::sent("old two", None),
    ]);
    type_line(&mut session, "mine");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("ok", Vec::new(), 0);
    session
}

/// Up walks every stored prompt; the scope chord narrows to this session's from the prompt on
/// screen, and pressed again goes back to every stored one.
#[test]
fn the_scope_chord_narrows_the_walk_to_this_sessions_prompts_and_widens_it_again() {
    let mut session = with_earlier_sessions();
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "mine");
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "old two");
    assert_eq!(session.history.scope(), crate::history::Scope::All);

    // Not this session's, so the nearest of this session's before it, and failing that the
    // oldest.
    handle_key(&mut session, ctrl('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::Session);
    assert_eq!(session.input(), "mine");
    assert_eq!(session.history.position(), Some((1, 1)));
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(
        session.input(),
        "mine",
        "Up walked past this session's prompts"
    );

    handle_key(&mut session, ctrl('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::All);
    assert_eq!(
        session.input(),
        "mine",
        "widening moved off the prompt on screen"
    );
    assert_eq!(session.history.position(), Some((3, 3)));
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "old two");
}

/// Once a scope is chosen it holds: ending the walk, or sending, does not put it back.
#[test]
fn the_scope_chosen_holds_after_the_walk_ends_and_after_sending() {
    let mut session = with_earlier_sessions();
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, ctrl('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::Session);
    handle_key(&mut session, key(KeyCode::Down));
    assert!(!session.history.is_browsing());
    assert_eq!(session.history.scope(), crate::history::Scope::Session);
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "mine", "Up left this session's prompts");

    handle_key(&mut session, key(KeyCode::Down));
    type_line(&mut session, "mine again");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("ok", Vec::new(), 0);
    assert_eq!(session.history.scope(), crate::history::Scope::Session);
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "mine");
}

/// The terminal sees Ctrl-Right as a window-manager key on some systems (macOS gives it to
/// Mission Control), so the switch is a chord of its own and the arrows are only the word
/// motion, on a recalled prompt as everywhere else.
#[test]
fn ctrl_and_alt_arrows_on_a_recalled_prompt_move_the_caret_and_leave_the_scope() {
    for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
        let mut session = with_earlier_sessions();
        handle_key(&mut session, key(KeyCode::Up));
        assert_eq!(session.input(), "mine");
        let end = session.caret();
        handle_key(&mut session, KeyEvent::new(KeyCode::Left, modifiers));
        assert!(
            session.caret() < end,
            "{modifiers:?}-Left did not move the caret"
        );
        handle_key(&mut session, KeyEvent::new(KeyCode::Right, modifiers));
        assert_eq!(
            session.caret(),
            end,
            "{modifiers:?}-Right did not move it back"
        );
        assert_eq!(session.history.scope(), crate::history::Scope::All);
        assert_eq!(session.input(), "mine");
    }
}

/// A settings file can move the switch. The chord it left goes dead, and the one it moved to
/// works in the same two places the default does.
#[test]
fn a_moved_scope_chord_switches_and_the_old_one_does_not() {
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("scope".to_string(), "alt-n".to_string());

    let mut session = with_earlier_sessions();
    session.adopt_keybindings(&moved);
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, ctrl('n'));
    assert_eq!(
        session.history.scope(),
        crate::history::Scope::All,
        "the chord the switch was moved off still switched"
    );
    handle_key(&mut session, alt('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::Session);
    handle_key(&mut session, alt('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::All);
}

/// With nothing on screen from the history there is nothing to switch, so the chord does not
/// type, move or narrow anything in a line being written.
#[test]
fn the_scope_chord_does_nothing_in_a_line_being_typed() {
    let mut session = with_earlier_sessions();
    type_line(&mut session, "read src now");
    let caret = session.caret();
    handle_key(&mut session, ctrl('n'));
    assert_eq!(session.input(), "read src now");
    assert_eq!(session.caret(), caret);
    assert_eq!(session.history.scope(), crate::history::Scope::All);
}

/// A moved chord is dead in a typed line like the default is, and where it lands on a readline
/// key the key still edits there, since the switch only takes it on a recalled prompt.
#[test]
fn a_moved_scope_chord_is_not_typed_and_keeps_the_edit_it_landed_on() {
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("scope".to_string(), "alt-n".to_string());
    let mut session = with_earlier_sessions();
    session.adopt_keybindings(&moved);
    type_line(&mut session, "read src now");
    handle_key(&mut session, alt('n'));
    assert_eq!(session.input(), "read src now", "the chord was typed");

    let mut onto_edit = std::collections::BTreeMap::new();
    onto_edit.insert("scope".to_string(), "ctrl-a".to_string());
    let mut session = with_earlier_sessions();
    session.adopt_keybindings(&onto_edit);
    type_line(&mut session, "read src now");
    handle_key(&mut session, ctrl('a'));
    assert_eq!(
        session.caret(),
        0,
        "the edit key it was moved onto went dead"
    );
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, ctrl('a'));
    assert_eq!(session.history.scope(), crate::history::Scope::Session);
}

/// A prompt that is in both lists stays on screen across the switch, at its place in each.
#[test]
fn switching_scope_keeps_a_prompt_that_is_in_both() {
    let mut session = with_earlier_sessions();
    type_line(&mut session, "mine too");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("ok", Vec::new(), 0);
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "mine");
    assert_eq!(session.history.position(), Some((3, 4)));
    handle_key(&mut session, ctrl('n'));
    assert_eq!(session.input(), "mine");
    assert_eq!(session.history.position(), Some((1, 2)));
    handle_key(&mut session, ctrl('n'));
    assert_eq!(session.input(), "mine");
    assert_eq!(session.history.position(), Some((3, 4)));
}

/// Alt is the word motion on a recalled prompt, and Ctrl is in a line that is being typed.
#[test]
fn the_word_keys_still_move_the_caret_outside_a_recalled_prompt() {
    let mut session = with_earlier_sessions();
    type_line(&mut session, "read src now");
    handle_key(&mut session, ctrl_key(KeyCode::Left));
    assert_eq!(session.caret(), "read src ".len());
    assert_eq!(session.history.scope(), crate::history::Scope::All);

    let mut session = with_earlier_sessions();
    handle_key(&mut session, key(KeyCode::Up));
    let on_screen = session.input().to_string();
    let before = session.caret();
    handle_key(
        &mut session,
        KeyEvent::new(KeyCode::Left, KeyModifiers::ALT),
    );
    assert_eq!(session.history.scope(), crate::history::Scope::All);
    assert_eq!(session.input(), on_screen);
    assert!(session.caret() < before, "Alt-Left did not move the caret");
}

/// A turn can begin while a stored prompt is on screen, and the chord means the same there as
/// at rest: it narrows and stays on the prompt, and pressed again comes back.
#[test]
fn the_scope_chord_works_while_a_turn_runs() {
    let mut session = with_earlier_sessions();
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "mine");
    session.status = Status::Working;

    handle_key_while_working(&mut session, ctrl('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::Session);
    assert_eq!(session.input(), "mine");
    assert_eq!(session.history.position(), Some((1, 1)));

    handle_key_while_working(&mut session, ctrl('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::All);
    assert_eq!(session.history.position(), Some((3, 3)));
}

/// A prompt queued behind a running turn is one the person has sent, so it is in this
/// session's scope from the moment it is queued.
#[test]
fn a_prompt_queued_during_a_turn_is_one_this_session_sent() {
    let mut session = Session::new("none");
    session.history =
        crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
            "old one", None,
        )]);
    session.status = Status::Working;
    type_line(&mut session, "queued one");
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(session.queued.len(), 1);

    session.queued.clear();
    session.status = Status::Idle;
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "queued one");
    handle_key(&mut session, ctrl('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::Session);
    assert_eq!(session.history.position(), Some((1, 1)));
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(
        session.input(),
        "queued one",
        "Up walked past this session's prompt"
    );
}

/// `/clear` starts a new conversation, whose own prompts are none yet, so the walk is over
/// every stored prompt and the chord has no session's prompts to narrow to.
#[test]
fn clearing_the_session_empties_the_scope_the_chord_narrows_to() {
    let mut session = with_earlier_sessions();
    handle_key(&mut session, key(KeyCode::Up));
    handle_key(&mut session, ctrl('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::Session);
    handle_key(&mut session, key(KeyCode::Down));
    session.clear();
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.history.scope(), crate::history::Scope::All);
    assert_eq!(session.input(), "mine");
    handle_key(&mut session, ctrl('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::All);
    assert_eq!(session.input(), "mine");
}

/// A new session has sent nothing, so Up walks the stored prompts and the chord is not on
/// offer: it stays in the wide scope.
#[test]
fn up_in_a_new_session_walks_the_stored_prompts_and_offers_no_switch() {
    let mut session = Session::new("none");
    session.history =
        crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
            "old one", None,
        )]);
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "old one");
    assert_eq!(session.history.position(), Some((1, 1)));

    handle_key(&mut session, ctrl('n'));
    assert_eq!(session.history.scope(), crate::history::Scope::All);
    assert_eq!(session.input(), "old one");
}

/// The readline bindings, because a terminal may send nothing at all for the named keys and
/// then the middle of a line would be unreachable.
#[test]
fn the_readline_bindings_move_and_delete_too() {
    let mut session = typed_into("some words here");
    handle_key(&mut session, ctrl('a'));
    assert_eq!(session.caret(), 0);
    handle_key(&mut session, ctrl('e'));
    assert_eq!(session.caret(), "some words here".len());

    handle_key(&mut session, ctrl('w'));
    assert_eq!(session.input(), "some words ");
    handle_key(&mut session, ctrl('u'));
    assert_eq!(session.input(), "");
}

/// Ctrl-K takes the rest of the line, which is the other half of Ctrl-U.
#[test]
fn ctrl_k_takes_the_rest_of_the_line() {
    let mut session = typed_into("keep this drop that");
    for _ in 0..2 {
        handle_key(&mut session, ctrl_key(KeyCode::Left));
    }
    handle_key(&mut session, ctrl('k'));
    assert_eq!(session.input(), "keep this ");
}

/// The stop arm the event loop reaches takes the prompt the turn began with, so it is the
/// half that can hand it back to the box. An interjection the turn had already taken is part
/// of the conversation it carries on with, so neither prompt moves: the opening one stays sent
/// and the stop is recorded under both.
#[test]
fn stopping_a_turn_that_took_a_prompt_mid_turn_hands_nothing_back() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);
    for c in "second".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    // What [`crate::remote_confirm::ToMain::Interjected`] does when the turn says it took one.
    session.interjected();

    finish_cancelled_turn(&mut session, "first", Some(0));

    assert_eq!(
        session.input(),
        "",
        "the opening prompt was handed back over an interjection"
    );
    assert_eq!(
        session
            .transcript
            .iter()
            .filter(|entry| entry.speaker == crate::state::Speaker::User)
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"],
        "the interjection was lifted back out of the transcript"
    );
    assert!(
        session
            .transcript
            .last()
            .is_some_and(|entry| entry.speaker == crate::state::Speaker::Stopped),
        "nothing recorded that it stopped"
    );
}

/// An answer from an exchange that met something untrusted holds no `kept`, and the driver
/// does not count it: removing the guard would offer text that content may have shaped as the
/// person's next prompt. The request is charged either way.
#[test]
fn a_suggestion_from_an_exchange_that_met_something_untrusted_is_not_offered() {
    let answer = |kept: Option<&str>| bravebot_agent::aside::Answered {
        shown: "run the tests again".to_string(),
        kept: kept.map(str::to_string),
        usage: bravebot_aichat::protocol::Usage {
            prompt_tokens: 70,
            completion_tokens: 30,
            ..Default::default()
        },
    };
    assert_eq!(suggested_text(Some(answer(None))).0, None);
    for kept in [None, Some("run the tests again")] {
        assert_eq!(
            suggested_text(Some(answer(kept))).1,
            answer(kept).usage,
            "the request was not charged for what it read and wrote"
        );
    }
    assert_eq!(
        suggested_text(Some(answer(Some("run the tests again")))).0,
        Some("run the tests again".to_string())
    );
    assert_eq!(suggested_text(None), (None, Default::default()));
}

/// INPUT-43: Tab and Right move the suggestion into the box as text, and nothing sends it.
#[test]
fn tab_and_right_move_the_suggestion_into_the_box_and_send_nothing() {
    for code in [KeyCode::Tab, KeyCode::Right] {
        let mut session = offered("run the tests again");
        assert_eq!(handle_key(&mut session, pressed(code)), Action::None);
        assert_eq!(session.input(), "run the tests again");
        assert_eq!(session.suggestion(), None);
    }
}

/// a suggestion that begins with a slash is only text once taken, and Enter is what runs
/// it. Taking it must not run the command.
#[test]
fn a_suggestion_that_begins_with_a_slash_is_text_until_enter() {
    let mut session = offered("/clear the old notes");
    assert_eq!(
        handle_key(&mut session, pressed(KeyCode::Tab)),
        Action::None
    );
    assert_eq!(session.input(), "/clear the old notes");
}

/// Any other key removes it, and the key is then read as it always was.
#[test]
fn any_other_key_removes_the_suggestion_and_still_types() {
    let mut session = offered("run the tests again");
    handle_key(&mut session, pressed(KeyCode::Char('x')));
    assert_eq!(session.suggestion(), None);
    assert_eq!(session.input(), "x");

    let mut session = offered("run the tests again");
    handle_key(&mut session, pressed(KeyCode::Esc));
    assert_eq!(session.suggestion(), None);
    assert_eq!(session.input(), "");
    // And Tab, with nothing on offer, takes nothing into the box.
    handle_key(&mut session, pressed(KeyCode::Tab));
    assert_eq!(session.input(), "");
}

/// A modifier pressed on its own is not a key anybody typed.
#[test]
fn a_modifier_on_its_own_leaves_the_suggestion_standing() {
    let mut session = offered("run the tests again");
    handle_key(
        &mut session,
        pressed(KeyCode::Modifier(
            ratatui::crossterm::event::ModifierKeyCode::LeftShift,
        )),
    );
    assert_eq!(session.suggestion(), Some("run the tests again"));
}

/// A suggestion is not offered where the setting is off, where something has been typed, or
/// where the session is not idle, and one that goes stale is dropped.
#[test]
fn a_suggestion_is_offered_only_to_an_idle_empty_box_with_the_setting_on() {
    let mut off = Session::new("none");
    off.offer_suggestion("run the tests again".to_string());
    assert_eq!(off.suggestion(), None);

    let mut typed = Session::new("none");
    typed.adopt_suggestions(true);
    handle_key(&mut typed, pressed(KeyCode::Char('a')));
    typed.offer_suggestion("run the tests again".to_string());
    assert_eq!(typed.suggestion(), None);

    let mut working = Session::new("none");
    working.adopt_suggestions(true);
    working.status = Status::Working;
    working.offer_suggestion("run the tests again".to_string());
    assert_eq!(working.suggestion(), None);

    let mut stale = offered("run the tests again");
    stale.status = Status::Working;
    stale.settle_suggestion();
    assert_eq!(stale.suggestion(), None);
}

/// The request is made after a turn that answered, with the setting on, and in none of the
/// places the issue lists.
#[test]
fn a_suggestion_is_asked_for_only_after_a_turn_that_answered() {
    let done = Some(Ending::Done);
    let ask = PermissionMode::Ask;
    assert!(suggestion_is_due(true, false, ask, done, false));
    assert!(!suggestion_is_due(false, false, ask, done, false));
    assert!(!suggestion_is_due(true, true, ask, done, false));
    assert!(!suggestion_is_due(
        true,
        false,
        PermissionMode::Plan,
        done,
        false
    ));
    assert!(!suggestion_is_due(true, false, ask, None, false));
    assert!(!suggestion_is_due(
        true,
        false,
        ask,
        Some(Ending::Stopped { attempts: None }),
        false
    ));
    assert!(!suggestion_is_due(true, false, ask, done, true));
}

/// The request's cost is in the session's total and in the figure for the turn it followed.
#[test]
fn a_suggestions_cost_is_charged_to_the_session() {
    let mut session = Session::new("none");
    session.charge_suggestion(bravebot_aichat::protocol::Usage {
        prompt_tokens: 90,
        completion_tokens: 30,
        cached: bravebot_aichat::protocol::Cached {
            read_tokens: 60,
            written_tokens: 0,
            stated: true,
        },
    });
    assert_eq!(session.tokens, 120);
    assert_eq!(session.spend_by_turn().get(&0).copied(), Some(120));
    assert_eq!(
        session.split(),
        bravebot_session::sessions::Split {
            read: 90,
            written: 30,
            cache: bravebot_aichat::protocol::Cached {
                read_tokens: 60,
                written_tokens: 0,
                stated: true,
            },
        },
        "a suggestion's request was charged without how it divided"
    );
}

fn a_store_for(name: &str) -> (std::path::PathBuf, bravebot_session::sessions::Handle) {
    let root = crate::test_profile::project(name);
    std::fs::create_dir_all(&root).unwrap();
    let stored = bravebot_session::sessions::Handle::begin(
        &root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    (root, stored)
}

fn a_request_in_flight() -> (Option<Suggesting>, mpsc::Sender<Suggested>) {
    let (worker, answer) = mpsc::channel();
    (Some(Suggesting { answer }), worker)
}

/// The request came to `tokens`, a quarter of them written.
fn what_the_request_did(text: Option<&str>, tokens: u64) -> Suggested {
    Suggested {
        text: text.map(str::to_string),
        usage: bravebot_aichat::protocol::Usage {
            prompt_tokens: tokens - tokens / 4,
            completion_tokens: tokens / 4,
            ..Default::default()
        },
        events: vec![bravebot_session::audit::Stamped {
            at: 1,
            from: None,
            event: bravebot_core::event::Event::GatePassed {
                gate: "file_read",
                detail: "suggestion".into(),
            },
        }],
    }
}

fn a_session_that_suggests() -> Session {
    let mut session = Session::new("none");
    session.adopt_suggestions(true);
    session
}

/// INPUT-43: an answer that comes back is charged to the session, its trail is kept, and the
/// suggestion is in the box. The screen is then redrawn and the request is not waited on again.
#[test]
fn an_answered_request_is_charged_recorded_and_offered() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let (root, mut stored) = a_store_for("suggestion-answered");
    let mut session = a_session_that_suggests();
    let (mut suggesting, worker) = a_request_in_flight();
    worker
        .send(what_the_request_did(Some("run the tests again"), 120))
        .unwrap();

    assert!(take_suggestion(&mut session, &mut stored, &mut suggesting));

    assert!(suggesting.is_none());
    assert_eq!(session.tokens, 120);
    assert_eq!(session.suggestion(), Some("run the tests again"));
    let trail = bravebot_session::sessions::audit_of(&root, stored.id());
    assert_eq!(trail[&session.turns].len(), 1);
}

/// The request went out whatever it returned, so an answer outside the counts costs the same
/// and leaves the same trail, and puts nothing in the box.
#[test]
fn an_answer_that_is_not_usable_is_still_charged_and_recorded() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let (root, mut stored) = a_store_for("suggestion-unusable");
    let mut session = a_session_that_suggests();
    let (mut suggesting, worker) = a_request_in_flight();
    worker.send(what_the_request_did(None, 30)).unwrap();

    take_suggestion(&mut session, &mut stored, &mut suggesting);

    assert!(suggesting.is_none());
    assert_eq!(session.tokens, 30);
    assert_eq!(session.suggestion(), None);
    let trail = bravebot_session::sessions::audit_of(&root, stored.id());
    assert_eq!(trail[&session.turns].len(), 1);
}

/// A request that has not been answered yet changes nothing and is still waited on, so the
/// answer is taken when it comes.
#[test]
fn a_request_not_yet_answered_is_waited_on() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let (_root, mut stored) = a_store_for("suggestion-pending");
    let mut session = a_session_that_suggests();
    let (mut suggesting, worker) = a_request_in_flight();

    assert!(!take_suggestion(&mut session, &mut stored, &mut suggesting));
    assert!(suggesting.is_some());
    assert_eq!(session.tokens, 0);
    assert_eq!(session.suggestion(), None);

    worker
        .send(what_the_request_did(Some("run the tests again"), 120))
        .unwrap();
    assert!(take_suggestion(&mut session, &mut stored, &mut suggesting));
    assert_eq!(session.suggestion(), Some("run the tests again"));
}

/// A worker that ends without answering leaves nothing to wait for, and nothing to charge or
/// record.
#[test]
fn a_worker_that_ends_without_answering_is_forgotten() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let (root, mut stored) = a_store_for("suggestion-disconnected");
    let mut session = a_session_that_suggests();
    let (mut suggesting, worker) = a_request_in_flight();
    drop(worker);

    assert!(!take_suggestion(&mut session, &mut stored, &mut suggesting));

    assert!(suggesting.is_none());
    assert_eq!(session.tokens, 0);
    assert_eq!(session.suggestion(), None);
    assert!(bravebot_session::sessions::audit_of(&root, stored.id()).is_empty());
}

/// An answer that comes after the person began typing is a guess about a state that has
/// passed. It is not put in the box, what was typed is kept, and the request is still charged.
#[test]
fn an_answer_that_arrives_after_typing_is_charged_and_not_offered() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let (_root, mut stored) = a_store_for("suggestion-after-typing");
    let mut session = a_session_that_suggests();
    let (mut suggesting, worker) = a_request_in_flight();
    handle_key(&mut session, pressed(KeyCode::Char('a')));
    worker
        .send(what_the_request_did(Some("run the tests again"), 120))
        .unwrap();

    take_suggestion(&mut session, &mut stored, &mut suggesting);

    assert_eq!(session.tokens, 120);
    assert_eq!(session.suggestion(), None);
    assert_eq!(session.input(), "a");
}

/// No request goes out where the setting is off, before any turn has ended, or after one that
/// did not answer. The worker of a request that did go out cannot be driven without a model,
/// so these are the cases that return before it starts.
#[test]
fn no_request_is_made_where_the_setting_is_off_or_no_turn_answered() {
    let config = a_config_needing_no_sign_in();
    let conversation = Conversation::new();
    let trust = TrustStore::new(workspace_for_test().root());

    let mut off = Session::new("none");
    off.complete("done", vec![], 10);
    assert!(suggest_next(&off, &config, &conversation, &trust).is_none());

    let before_any_turn = a_session_that_suggests();
    assert!(suggest_next(&before_any_turn, &config, &conversation, &trust).is_none());

    let mut failed = a_session_that_suggests();
    failed.fail(
        "safe failure",
        Ending::Failed(bravebot_agent::Diagnosis::of(
            bravebot_agent::Category::Transport,
        )),
    );
    assert!(suggest_next(&failed, &config, &conversation, &trust).is_none());
}

/// INPUT-9, INPUT-48. Rejects: a request over the conversation a running turn is writing to,
/// and the chord falling through to the ladder, which types a letter for a chord that carries
/// Alt: the one a settings file moved it to would write an `e` into the line.
#[test]
fn the_enhance_chord_does_nothing_while_a_turn_runs() {
    let mut session = a_turn_running_on("first");
    for c in "second".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key_while_working(&mut session, ctrl('p')),
        Action::None,
        "the chord asked for a rewrite mid-turn"
    );
    assert_eq!(session.input(), "second");
    assert!(session.queued.is_empty());

    let mut moved = std::collections::BTreeMap::new();
    moved.insert("enhance".to_string(), "alt-e".to_string());
    session.adopt_keybindings(&moved);
    assert_eq!(
        handle_key_while_working(&mut session, alt('e')),
        Action::None,
        "the moved chord reached the ladder"
    );
    assert_eq!(session.input(), "second", "the moved chord was typed");
}
