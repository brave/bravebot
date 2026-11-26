//! Repeating a prompt the person typed.
//!
//! The scheduler is [`bravebot_agent::repeat`], which every front end shares. What is the
//! terminal's own is what the first tick of a loop carries besides its line, [`Carried`], and where
//! every tick reads its names from, [`crate::state::NamesFrom`].
//!
//! Nothing here is written to disk. A loop lives as long as the session that started it.

pub use bravebot_agent::repeat::*;

/// What the first tick of a loop carries besides its line, and no later tick has.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Carried {
    /// The pictures pasted into the line.
    pub pasted: Vec<crate::state::AttachedImage>,
    /// The files dropped onto it.
    pub attached: Vec<crate::state::Attached>,
}

/// A loop that is running in the terminal.
pub type Running = bravebot_agent::repeat::Running<Carried, crate::state::NamesFrom>;

/// The loops a terminal session holds, oldest first.
pub type Loops = bravebot_agent::repeat::Loops<Carried, crate::state::NamesFrom>;

/// The request in an argument that starts a loop, for the tests of what a loop then does.
///
/// Those tests want a running loop rather than the reading of an argument, and going through
/// [`parse`] for it keeps them from disagreeing with it.
#[cfg(test)]
pub(crate) fn request(argument: &str) -> Request {
    parse(argument)
        .started()
        .unwrap_or_else(|| panic!("{argument:?} started no loop"))
}

/// The request in an argument that arms `/after`, for the tests of what a held line then does.
#[cfg(test)]
pub(crate) fn request_after(argument: &str) -> Request {
    match parse_after(argument) {
        Delayed::Start(request) => request,
        Delayed::Unreadable => panic!("{argument:?} armed nothing"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn every(request: &Request) -> Duration {
        match request.pacing {
            Pacing::Every(duration) => duration,
            Pacing::SelfPaced | Pacing::Once(_) => panic!("expected an interval"),
        }
    }

    /// The form the command was documented with, and the one a person types when they already
    /// know what they want repeated.
    #[test]
    fn an_interval_written_first_is_taken_off_the_front() {
        let request = request("5m check the deploy");
        assert_eq!(every(&request), Duration::from_secs(300));
        assert_eq!(request.prompt, "check the deploy");
    }

    #[test]
    fn every_unit_letter_is_understood_at_the_front() {
        for (line, seconds) in [
            ("90s watch", 90),
            ("5m watch", 300),
            ("2h watch", 7_200),
            ("1d watch", 86_400),
        ] {
            let request = request(line);
            assert_eq!(every(&request), Duration::from_secs(seconds), "{line}");
        }
    }

    /// The other way people write it, which reads as a sentence rather than as an argument.
    #[test]
    fn an_interval_written_last_is_taken_off_the_end() {
        let request = request("check the deploy every 20m");
        assert_eq!(every(&request), Duration::from_secs(1_200));
        assert_eq!(request.prompt, "check the deploy");
    }

    #[test]
    fn a_trailing_interval_may_spell_its_unit_out() {
        for line in [
            "check it every 20 minutes",
            "check it every 20 min",
            "check it every 20m",
            "check it EVERY 20 MINUTES",
        ] {
            let request = request(line);
            assert_eq!(every(&request), Duration::from_secs(1_200), "{line}");
            assert_eq!(request.prompt, "check it", "{line}");
        }
    }

    /// The whole reason the trailing form insists on a time expression. Without it, half the
    /// sentences somebody would loop over would lose their last two words.
    #[test]
    fn every_without_a_time_after_it_is_words_rather_than_an_interval() {
        for line in ["check every PR", "look at every so often", "review every"] {
            let request = request(line);
            assert_eq!(request.pacing, Pacing::SelfPaced, "{line}");
            assert_eq!(request.prompt, line, "{line}");
        }
    }

    /// A time expression after `every` is the clause only when nothing follows it. Read off the
    /// front of what follows, `every 20m until it is green` would take the interval and drop the
    /// words after it from every tick.
    #[test]
    fn a_time_followed_by_more_words_is_not_a_trailing_interval() {
        for line in [
            "check the deploy every 20m until it is green",
            "check it every 20 minutes and report",
        ] {
            let request = request(line);
            assert_eq!(request.pacing, Pacing::SelfPaced, "{line}");
            assert_eq!(request.prompt, line, "{line}");
        }
    }

    /// A word that only ends in every is not the clause either: `every` needs whitespace or the
    /// start of the line before it, or `thievery 20m` would send `write about thi` on each tick.
    #[test]
    fn a_word_that_merely_ends_with_every_is_not_an_interval() {
        let request = request("write about thievery 20m");
        assert_eq!(request.pacing, Pacing::SelfPaced);
        assert_eq!(request.prompt, "write about thievery 20m");
    }

    /// A word that merely begins with every is not an every clause, and the sentence keeps it.
    #[test]
    fn a_word_that_merely_starts_with_every_is_not_an_interval() {
        for line in [
            "check everything 20m",
            "notify everyone 30 minutes",
            "search everywhere 1h",
            "practice everyday 10s",
        ] {
            let request = request(line);
            assert_eq!(request.pacing, Pacing::SelfPaced, "{line}");
            assert_eq!(request.prompt, line, "{line}");
        }

        let with_trailing = request("check everything 20m every 5m");
        assert_eq!(every(&with_trailing), Duration::from_secs(300));
        assert_eq!(with_trailing.prompt, "check everything 20m");
    }

    /// The front of the line is read first, so a line with both is paced by the one written as
    /// an argument rather than by the one written in the sentence.
    #[test]
    fn a_leading_interval_wins_over_a_trailing_one() {
        let request = request("5m check the deploy every 20m");
        assert_eq!(every(&request), Duration::from_secs(300));
        assert_eq!(request.prompt, "check the deploy every 20m");
    }

    #[test]
    fn a_line_with_no_interval_is_paced_by_the_planner() {
        let request = request("watch the build");
        assert_eq!(request.pacing, Pacing::SelfPaced);
        assert_eq!(request.prompt, "watch the build");
    }

    /// Nothing to send is not a loop, however it was written. Kept apart from the bare word so
    /// that a person who typed an interval and stopped is told what the command needs, rather
    /// than handed a report about the loop they were in the middle of replacing.
    #[test]
    fn an_interval_with_nothing_to_send_is_not_a_request() {
        for line in ["5m", "  5m  ", "every 5m"] {
            assert_eq!(parse(line), Asked::Unreadable, "{line}");
        }
    }

    /// What is going to happen next without anybody typing is the one thing about a session that
    /// its transcript does not hold, so the bare word asks for it rather than being a mistake.
    #[test]
    fn the_bare_command_asks_what_is_repeating() {
        for line in ["", "   "] {
            assert_eq!(parse(line), Asked::Report, "{line}");
        }
    }

    #[test]
    fn stop_on_its_own_ends_the_loop() {
        for line in ["stop", "  stop  "] {
            assert_eq!(parse(line), Asked::Stop(None), "{line}");
        }
    }

    /// A terminal that capitalises the first letter of a line, or a shift held a beat too long.
    /// Read as a line instead, the word becomes a loop sending `Stop` to a model on its own pace
    /// until somebody presses ctrl-c twice, which is the one wrong reading here that spends money.
    #[test]
    fn the_word_ends_the_loop_whatever_its_case() {
        for line in ["Stop", "STOP", "  StOp  "] {
            assert_eq!(parse(line), Asked::Stop(None), "{line}");
        }
    }

    /// `stop every 5m` is somebody ending the loop that runs every five minutes, and `5m stop` is
    /// the same sentence the other way round. Read as lines they replace that loop with one sending
    /// the bare word `stop` to a model, which is the one remainder that cannot be a prompt.
    #[test]
    fn an_interval_beside_the_word_still_ends_the_loop() {
        for line in ["stop every 5m", "5m stop", "  STOP every 1h  "] {
            assert_eq!(parse(line), Asked::Stop(None), "{line}");
        }
    }

    /// LOOP-15: the number is the whole of what follows the word, with or without an interval.
    #[test]
    fn stop_and_a_number_ends_that_loop() {
        for (line, number) in [
            ("stop 2", 2),
            ("STOP   13 ", 13),
            ("stop 2 every 5m", 2),
            ("5m stop 4", 4),
        ] {
            assert_eq!(parse(line), Asked::Stop(Some(number)), "{line}");
        }
        assert_eq!(
            parse("stop 99999999999999999999999"),
            Asked::Stop(Some(usize::MAX)),
            "a number too large for a loop names none, and is not a line to repeat"
        );
    }

    /// A number is a number and nothing else: anything after it makes the line a sentence.
    #[test]
    fn a_number_with_words_around_it_is_still_a_line_to_repeat() {
        for line in [
            "stop 2 the deploy",
            "stop two",
            "stop -1",
            "stop 2.5",
            "stop #2",
        ] {
            assert!(
                matches!(parse(line), Asked::Start(_)),
                "{line} did not start a loop"
            );
        }
    }

    /// LOOP-17: the soonest due tick goes first, ties go to the older loop, and a loop with a tick
    /// in flight is not in the running.
    #[test]
    fn the_soonest_due_loop_goes_next_and_a_tie_goes_to_the_older() {
        let now = Instant::now();
        let ago = |secs| now - Duration::from_secs(secs);
        let mut loops = Loops::default();
        loops.start(Running::armed(
            "a".into(),
            Wakeup::asked(60, false),
            ago(100),
        ));
        loops.start(Running::armed(
            "b".into(),
            Wakeup::asked(60, false),
            ago(300),
        ));
        loops.start(Running::armed(
            "c".into(),
            Wakeup::asked(60, false),
            ago(300),
        ));
        assert_eq!(
            loops.next_due(now),
            Some(2),
            "the earliest due, older on a tie"
        );

        let mut running = loops.get_mut(2).expect("loop 2");
        running.dispatching();
        assert_eq!(
            loops.next_due(now),
            Some(3),
            "a tick in flight is not due again"
        );
        running = loops.get_mut(3).expect("loop 3");
        running.dispatching();
        assert_eq!(loops.next_due(now), Some(1));
    }

    /// LOOP-15: a number is never reused, because a stale `/loop stop N` typed after loop N ended
    /// would otherwise end a different loop.
    #[test]
    fn a_number_is_never_issued_twice() {
        let mut loops = Loops::default();
        let one = loops.start(Running::begin(request("5m a")));
        let two = loops.start(Running::begin(request("5m b")));
        assert!(loops.stop(two));
        assert!(!loops.stop(two), "a loop ended twice");
        let three = loops.start(Running::begin(request("5m c")));
        assert_eq!((one, two, three), (1, 2, 3));
        assert_eq!(loops.next_number(), 4);
    }

    /// The one place in the command where a reserved word could eat a line somebody meant to have
    /// repeated. `stop` ends the loop as the whole of the line to be sent, and is an ordinary first
    /// word of a prompt anywhere else.
    #[test]
    fn a_line_that_begins_with_stop_is_still_a_line_to_repeat() {
        for line in ["stop the deploy", "stop 5m", "stopping at noon"] {
            let asked = request(line);
            assert_eq!(asked.pacing, Pacing::SelfPaced, "{line}");
            assert_eq!(asked.prompt, line, "{line}");
        }

        let with_interval = request("stop the deploy every 5m");
        assert_eq!(every(&with_interval), Duration::from_secs(300));
        assert_eq!(with_interval.prompt, "stop the deploy");
    }

    /// A word that merely starts with a digit is not an interval, and the line keeps it.
    #[test]
    fn a_word_that_is_not_a_time_stays_part_of_the_prompt() {
        let request = request("5minutes to midnight");
        assert_eq!(request.pacing, Pacing::SelfPaced);
        assert_eq!(request.prompt, "5minutes to midnight");
    }

    /// The person's own number is honoured as far down as a loop goes, which is far enough that
    /// the interval somebody actually types is usually the interval they get.
    #[test]
    fn an_interval_the_person_gave_is_kept_down_to_the_floor() {
        let request = request("10s watch");
        assert_eq!(every(&request), Duration::from_secs(10));
        assert_eq!(request.adjusted, None);
    }

    #[test]
    fn an_interval_faster_than_the_floor_is_raised_to_it_and_said_so() {
        let request = request("1s watch");
        assert_eq!(every(&request), FLOOR);
        assert_eq!(request.adjusted, Some(Held::Raised(FLOOR)));
    }

    #[test]
    fn an_interval_longer_than_a_loop_may_live_is_capped() {
        let request = request("30d watch");
        assert_eq!(every(&request), MAX_AGE);
        assert_eq!(request.adjusted, Some(Held::Capped(MAX_AGE)));
    }

    /// A number nothing could hold is a word made of digits, not a schedule, and reading it as
    /// one would have to invent a duration nobody asked for.
    #[test]
    fn a_count_too_large_to_be_a_duration_is_not_an_interval() {
        let request = request("check it every 99999999999999999999 days");
        assert_eq!(request.pacing, Pacing::SelfPaced);
    }

    #[test]
    fn an_interval_within_the_bounds_is_reported_as_unadjusted() {
        let request = request("5m watch");
        assert_eq!(request.adjusted, None);
    }

    /// The driver keeps its own time, so a planner cannot lengthen or shorten a loop the person
    /// set the pace of.
    #[test]
    fn a_paced_loop_ignores_what_a_turn_asked_for() {
        let mut running = Running::begin(request("5m watch"));
        let now = Instant::now();
        running.dispatching();
        assert!(running.ended(Some(Wakeup::asked(3_600, false)), now,));
        assert_eq!(running.until(now), Some(Duration::from_secs(300)));
    }

    /// The pictures are handed over once and then there are none left to hand over, so "the first
    /// tick and no other" holds because the loop no longer has them rather than because something
    /// counted the ticks. What the ticks after it send is the line with the markers put back.
    #[test]
    fn a_pasted_picture_goes_to_one_tick_and_the_settled_line_to_every_other() {
        let picture = crate::state::AttachedImage {
            marker: "[Image #1]".to_string(),
            media_type: "image/png",
            bytes: b"pixels".to_vec(),
        };
        let carried = |pasted, attached| Carried { pasted, attached };
        let mut running = Running::begin(request("5m look at [Image #1]")).carrying(
            carried(vec![picture.clone()], Vec::new()),
            "look at the picture".to_string(),
        );

        assert_eq!(
            running.dispatching(),
            (
                "look at [Image #1]".to_string(),
                carried(vec![picture], Vec::new())
            )
        );
        assert_eq!(
            running.dispatching(),
            (
                "look at the picture".to_string(),
                carried(Vec::new(), Vec::new())
            )
        );
        assert_eq!(
            running.dispatching(),
            (
                "look at the picture".to_string(),
                carried(Vec::new(), Vec::new())
            )
        );
    }

    /// A dropped file leaves with the same tick and for the same reason, and what the ticks after it
    /// send is its name. A name is what a turn can act on: it reads the file through the gate it
    /// reads any other file through, so a later tick looks at the same file rather than at a marker
    /// standing for nothing.
    #[test]
    fn a_dropped_file_goes_to_one_tick_and_its_name_to_every_other() {
        let file = crate::state::Attached {
            marker: "[Image #1]".to_string(),
            name: "shot.png".to_string(),
            shown: "/home/me/shot.png".to_string(),
            kind: bravebot_filetype::by_name::Kind::Attachment("image/png"),
        };
        let carried = |pasted, attached| Carried { pasted, attached };
        let mut running = Running::begin(request("5m look at [Image #1]")).carrying(
            carried(Vec::new(), vec![file.clone()]),
            "look at shot.png".to_string(),
        );

        assert_eq!(
            running.dispatching(),
            (
                "look at [Image #1]".to_string(),
                carried(Vec::new(), vec![file])
            )
        );
        assert_eq!(
            running.dispatching(),
            (
                "look at shot.png".to_string(),
                carried(Vec::new(), Vec::new())
            )
        );
    }

    #[test]
    fn a_self_paced_loop_waits_as_long_as_the_turn_asked() {
        let mut running = Running::begin(request("watch"));
        let now = Instant::now();
        running.dispatching();
        assert!(running.ended(Some(Wakeup::asked(900, false)), now,));
        assert_eq!(running.until(now), Some(Duration::from_secs(900)));
    }

    /// The failure a loop a turn started would otherwise have. A person who types `/loop` has not
    /// seen the work done once, so the first tick goes immediately; a turn that asked for a later
    /// look has just taken one and is reporting it, so an immediate tick would send the line again
    /// before the person has read the first answer and reply with the same look twice.
    #[test]
    fn a_loop_a_turn_asked_for_starts_a_wait_away_rather_than_now() {
        let now = Instant::now();
        let running = Running::armed(
            "tell me when a.txt changes".to_string(),
            Wakeup::asked(900, false),
            now,
        );
        assert_eq!(running.until(now), Some(Duration::from_secs(900)));
        assert!(
            !running.due(now),
            "a loop a turn asked for sent its first tick at once"
        );
        assert_eq!(running.prompt(), "tell me when a.txt changes");
    }

    /// A turn cannot give an interval, only the wait until the next look, and it is asked again
    /// then. So the loop it starts is self-paced whatever it said, and a loop reported as the
    /// person's would show their pace in the status bar and lose the tool that keeps it alive.
    #[test]
    fn a_loop_a_turn_asked_for_is_paced_by_the_turns() {
        let running = Running::armed(
            "tell me when a.txt changes".to_string(),
            Wakeup::asked(900, false),
            Instant::now(),
        );
        assert_eq!(running.pacing(), Pacing::SelfPaced);
    }

    /// The bounds are the tool's, so this is the loop honouring what the tool already held the
    /// number to rather than a second clamp of its own.
    #[test]
    fn a_wait_a_turn_asked_for_is_held_to_the_bounds() {
        for (asked, held) in [(0, Wakeup::FLOOR), (86_400, Wakeup::CEILING)] {
            let mut running = Running::begin(request("watch"));
            let now = Instant::now();
            running.dispatching();
            running.ended(Some(Wakeup::asked(asked, false)), now);
            assert_eq!(running.until(now), Some(held), "{asked}");
        }
    }

    /// One forgotten turn is a turn that forgot. Two is a loop nobody is running, and waking it
    /// every twenty minutes for the rest of the session helps nobody.
    #[test]
    fn a_self_paced_turn_that_says_nothing_is_woken_once_more_and_then_the_loop_ends() {
        let mut running = Running::begin(request("watch"));
        let now = Instant::now();

        running.dispatching();
        assert!(running.ended(None, now));
        assert_eq!(
            running.until(now),
            Some(Duration::from_secs(20 * 60)),
            "the fallback wake is not twenty minutes away"
        );

        running.dispatching();
        assert!(!running.ended(None, now));
    }

    /// A deliberate stop is not a silence: the loop ends on the turn that said it, with the
    /// fallback wake still unspent.
    #[test]
    fn a_turn_that_says_the_loop_is_finished_ends_it_without_a_keepalive() {
        let mut running = Running::begin(request("watch"));
        let now = Instant::now();

        running.dispatching();
        assert!(!running.ended(Some(Wakeup::finished(true)), now));
    }

    /// The budget is for turns that stopped saying when to wake, not for the one that did.
    #[test]
    fn a_turn_that_says_when_to_wake_restores_the_fallback() {
        let mut running = Running::begin(request("watch"));
        let now = Instant::now();

        running.dispatching();
        running.ended(None, now);
        running.dispatching();
        running.ended(Some(Wakeup::asked(120, false)), now);

        running.dispatching();
        assert!(running.ended(None, now));
    }

    #[test]
    fn quiet_ticks_are_counted_until_one_reports_something() {
        let mut running = Running::begin(request("watch"));
        let now = Instant::now();
        for expected in [1, 2, 3] {
            running.dispatching();
            running.ended(Some(Wakeup::asked(60, true)), now);
            assert_eq!(running.quiet(), expected);
        }
        running.dispatching();
        running.ended(Some(Wakeup::asked(60, false)), now);
        assert_eq!(running.quiet(), 0);
    }

    /// The delay is the first word, as an interval is. A wrong reading takes an `every` clause at the
    /// end for a delay, which would send a sentence with its last words missing; or reads nothing at
    /// all and sends the whole line, delay and all.
    #[test]
    fn after_reads_a_delay_off_the_front_and_sends_the_rest_unchanged() {
        let Delayed::Start(request) = parse_after("5m check the deploy every 20m") else {
            panic!("a delay at the front was not read");
        };
        assert_eq!(request.pacing, Pacing::Once(Duration::from_secs(300)));
        assert_eq!(request.prompt, "check the deploy every 20m");
        assert_eq!(request.adjusted, None);
    }

    /// Without a delay in front there is nothing to wait for, and a trailing `every` is repetition,
    /// which this does not do. A wrong reading sends the line at once or repeats it.
    #[test]
    fn after_without_a_leading_delay_is_not_a_request() {
        for argument in [
            "",
            "5m",
            "check the deploy every 20m",
            "check the deploy",
            "minutes of silence",
        ] {
            assert_eq!(parse_after(argument), Delayed::Unreadable, "{argument:?}");
        }
    }

    /// The delay is held to the bounds a person's interval is, and says which way it moved. Without
    /// the cap a delay of more than a loop's life would be a schedule that outlives the bound.
    #[test]
    fn an_after_delay_outside_the_bounds_is_held_and_reported() {
        let Delayed::Start(fast) = parse_after("1s check") else {
            panic!("unread");
        };
        assert_eq!(fast.pacing, Pacing::Once(FLOOR));
        assert_eq!(fast.adjusted, Some(Held::Raised(FLOOR)));
        let Delayed::Start(slow) = parse_after("30d check") else {
            panic!("unread");
        };
        assert_eq!(slow.pacing, Pacing::Once(MAX_AGE));
        assert_eq!(slow.adjusted, Some(Held::Capped(MAX_AGE)));
    }

    /// Not due before the delay, due at it, gone once the turn ends, and not a tick the planner is
    /// told about. A wrong implementation arms another tick, is due at once, or tells the turn it is
    /// inside a loop.
    #[test]
    fn a_line_sent_once_is_due_at_its_delay_and_is_gone_after_its_turn() {
        let now = Instant::now();
        let mut running = Running::after(request_after("5m check"), now);
        assert!(!running.due(now + Duration::from_secs(299)));
        assert!(running.due(now + Duration::from_secs(300)));
        assert!(!running.aged_out(now + MAX_AGE + Duration::from_secs(1)));

        running.dispatching();
        assert!(running.tick().is_none(), "the turn was told it is a tick");
        assert!(!running.ended(None, now + Duration::from_secs(301)));
    }

    /// A tick already running is not due again. Without this a turn that outlasts its interval
    /// would have a second tick waiting the moment it drew breath.
    #[test]
    fn a_tick_in_flight_is_not_due_again() {
        let mut running = Running::begin(request("5m watch"));
        let now = Instant::now();
        running.dispatching();
        running.ended(None, now);
        let due = now + Duration::from_secs(600);
        assert!(running.due(due));
        running.dispatching();
        assert!(!running.due(due));
    }

    /// A turn that outlasts its own interval must not be due the moment it ends: the gap is the
    /// time between runs, counted from the end of the one before.
    #[test]
    fn the_gap_is_measured_from_the_end_of_a_tick_and_not_its_start() {
        let start = Instant::now();
        let ended = start + Duration::from_secs(600);

        let mut every = Running::begin(request("5m watch"));
        every.dispatching();
        assert!(every.ended(None, ended));
        assert!(!every.due(ended), "an interval was counted from the start");
        assert!(!every.due(ended + Duration::from_secs(299)));
        assert!(every.due(ended + Duration::from_secs(300)));

        let mut paced = Running::begin(request("watch"));
        paced.dispatching();
        assert!(paced.ended(Some(Wakeup::asked(900, false)), ended));
        assert!(!paced.due(ended + Duration::from_secs(899)));
        assert!(paced.due(ended + Duration::from_secs(900)));
    }

    #[test]
    fn a_loop_with_nothing_armed_is_not_due() {
        let running = Running::begin(request("watch"));
        assert!(!running.due(Instant::now() + Duration::from_secs(86_400)));
    }

    #[test]
    fn a_loop_older_than_a_week_has_aged_out() {
        let running = Running::begin(request("5m watch"));
        let now = Instant::now();
        assert!(!running.aged_out(now + MAX_AGE - Duration::from_secs(1)));
        assert!(running.aged_out(now + MAX_AGE));
    }

    #[test]
    fn a_duration_is_spelled_in_the_units_it_was_asked_for() {
        assert_eq!(spell(Duration::from_secs(45)), "45s");
        assert_eq!(spell(Duration::from_secs(300)), "5m");
        assert_eq!(spell(Duration::from_secs(90)), "1m 30s");
        assert_eq!(spell(Duration::from_secs(5_400)), "1h 30m");
        assert_eq!(spell(Duration::from_secs(86_400)), "1d");
        assert_eq!(spell(Duration::ZERO), "0s");
    }
}
