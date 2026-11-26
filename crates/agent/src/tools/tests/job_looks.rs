use super::*;

/// A command proof cannot authorize output captured after shared file authority changes.
#[test]
fn an_ended_job_revalidates_its_file_proof_before_releasing_output() {
    use bravebot_core::TrustStore;
    use bravebot_core::file_authority::FileAuthority;
    use std::time::{Duration, Instant};
    for changed in [false, true] {
        let root = std::env::current_dir().unwrap();
        // No rules in play: the line is this test's own and no settings file is read.
        let plan =
            crate::cmdline::compile("printf JOB_PROOF_SENTINEL", &root, None, &mut |_, _| Ok(()))
                .unwrap();
        let bravebot_core::command::Steps::Pipeline(steps) = &plan.steps else {
            panic!("one pipeline");
        };
        let mut running = crate::exec::start_steps_within(
            steps,
            &root,
            None,
            None,
            &bravebot_core::cancel::Cancel::new(),
            crate::exec::LIMIT,
        )
        .unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while !running.ended() {
            assert!(Instant::now() < until, "job did not end");
            std::thread::yield_now();
        }
        let authority = FileAuthority::new(TrustStore::new(&root));
        let mut jobs = Jobs::new();
        jobs.keep(
            running,
            JobLines::new(
                &plan.display(),
                &crate::cmdline::display_as_written(&plan, None),
            ),
            Label::trusted_public(),
            authority.clone(),
            0,
            false,
        );
        if changed {
            // Even a same-label effect invalidates the earlier proof. No output is inspected.
            let effect = authority.capture().begin("independent.txt").unwrap();
            drop(effect);
        }
        let ended = jobs.ended(OUTPUT_CAP);
        assert_eq!(ended.len(), 1);
        let printed = ended[0]
            .printed
            .as_ref()
            .expect("the actual process printed");
        assert_eq!(printed.label().is_trusted(), !changed);
    }
}

/// What the screen is given of a running job is read from where the planner last looked and
/// moves only the screen's own cursor, so a look afterwards still returns every line and the
/// screen is given each line once.
#[cfg(unix)]
#[test]
fn unread_output_leaves_the_planners_cursor_where_it_was() {
    use bravebot_core::TrustStore;
    use bravebot_core::file_authority::FileAuthority;

    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("bravebot-unread-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("serve");
    std::fs::write(&script, "#!/bin/sh\necho first\nsleep 30\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut jobs = Jobs::new();
    let plan = crate::cmdline::compile("./serve", &root, None, &mut |_, _| Ok(())).unwrap();
    let bravebot_core::command::Steps::Pipeline(steps) = &plan.steps else {
        panic!("one pipeline");
    };
    let running = crate::exec::start_steps_within(
        steps,
        &root,
        None,
        None,
        &bravebot_core::cancel::Cancel::new(),
        crate::exec::LIMIT,
    )
    .unwrap();
    jobs.keep(
        running,
        JobLines::new(
            &plan.display(),
            &crate::cmdline::display_as_written(&plan, None),
        ),
        Label::trusted_public(),
        FileAuthority::new(TrustStore::new(&root)),
        0,
        false,
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let first = loop {
        let unread = jobs.unread_output();
        if let Some(unread) = unread.into_iter().next() {
            break unread;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the job's line never became unread"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert_eq!(first.name, "job:1");
    assert!(
        jobs.unread_output().is_empty(),
        "the same lines were given to the screen twice"
    );

    let job = jobs.running.get_mut("job:1").unwrap();
    let looked = job.running.since(&mut job.seen);
    assert!(
        looked.contains("first"),
        "giving the screen the lines moved the planner's cursor past them: {looked:?}"
    );
    drop(jobs);
    let _ = std::fs::remove_dir_all(&root);
}

/// Starts a script that prints `first`, waits for the file `gate` to exist, then prints
/// `second` and exits, as a job under `jobs`, and waits until `first` has arrived.
///
/// Gated on a file and not on a sleep, so a look made on a loaded machine cannot catch the
/// second burst early.
#[cfg(unix)]
fn job_printing_in_two_bursts(
    jobs: &mut Jobs,
    root: &std::path::Path,
    first: &str,
    second: &str,
) -> std::path::PathBuf {
    job_running_commands(
        jobs,
        root,
        &format!("printf '{first}'"),
        &format!("printf '{second}'"),
    )
}

/// As [`job_printing_in_two_bursts`], with each burst given as shell commands, so a burst may
/// write to standard error or print a line with no newline.
#[cfg(unix)]
pub(super) fn job_running_commands(
    jobs: &mut Jobs,
    root: &std::path::Path,
    first: &str,
    second: &str,
) -> std::path::PathBuf {
    use bravebot_core::TrustStore;
    use bravebot_core::file_authority::FileAuthority;
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(root).unwrap();
    let gate = root.join("go");
    let script = root.join("bursts");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n{first}\nwhile [ ! -e '{}' ]; do sleep 0.05; done\n{second}\n",
            gate.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let plan = crate::cmdline::compile("./bursts", root, None, &mut |_, _| Ok(())).unwrap();
    let bravebot_core::command::Steps::Pipeline(steps) = &plan.steps else {
        panic!("one pipeline");
    };
    let running = crate::exec::start_steps_within(
        steps,
        root,
        None,
        None,
        &bravebot_core::cancel::Cancel::new(),
        crate::exec::LIMIT,
    )
    .unwrap();
    jobs.keep(
        running,
        JobLines::new(
            &plan.display(),
            &crate::cmdline::display_as_written(&plan, None),
        ),
        Label::trusted_public(),
        FileAuthority::new(TrustStore::new(root)),
        0,
        false,
    );
    let job = jobs.running.get_mut("job:1").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !job.running.has_more(&job.seen) {
        assert!(
            std::time::Instant::now() < deadline,
            "the first burst never arrived"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    gate
}

/// Waits until the job under `jobs` has ended and its pipes are drained, so a look taken next
/// holds everything the job printed.
#[cfg(unix)]
pub(super) fn wait_for_the_job_to_end(jobs: &mut Jobs) {
    let job = jobs.running.get_mut("job:1").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !job.running.ended() {
        assert!(std::time::Instant::now() < deadline, "the job never ended");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// A line cut between two looks is counted once. The first look holds `ab` and the second the
/// rest of that line and one more.
///
/// Rejects counting `text.lines().count()` per look, which reads the cut line in both looks
/// (the second look would count 2 lines, begin at 2, and the third look would begin at 4).
#[cfg(unix)]
#[test]
fn a_line_cut_between_two_looks_is_counted_once() {
    let root = std::env::temp_dir().join(format!("bravebot-cut-{}", std::process::id()));
    let mut jobs = Jobs::new();
    let gate = job_printing_in_two_bursts(&mut jobs, &root, "ab", "c\\nd\\n");

    let job = jobs.running.get_mut("job:1").unwrap();
    let (text, first) = job.take();
    assert_eq!(text, "ab");
    assert_eq!((first.first, first.lines), (1, 1));

    std::fs::write(&gate, "").unwrap();
    wait_for_the_job_to_end(&mut jobs);
    let job = jobs.running.get_mut("job:1").unwrap();
    let (text, second) = job.take();
    assert_eq!(text, "c\nd\n");
    // The look holds the rest of line 1 and line 2: it starts after line 1 and adds one line.
    assert_eq!((second.first, second.lines), (2, 1), "{second:?}");

    let job = jobs.running.get_mut("job:1").unwrap();
    let (_, third) = job.take();
    // Rejects a count that is still shifted after the cut line: the next start is line 3.
    assert_eq!((third.first, third.lines), (3, 0), "{third:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// A look whose text has no trailing newline counts the line it ends in, and the look that
/// finishes the line adds nothing for it. The next line starts the count where the job's
/// output does.
///
/// Rejects counting newline bytes (the unterminated `b` would count in neither look, so the
/// first look would be 1 line and the next start 2), and rejects counting `lines()` per look
/// (the second look would add a line for the rest of `b`).
#[cfg(unix)]
#[test]
fn a_look_with_no_trailing_newline_counts_its_last_line_and_the_next_look_does_not_again() {
    let root = std::env::temp_dir().join(format!("bravebot-bare-{}", std::process::id()));
    let mut jobs = Jobs::new();
    let gate = job_printing_in_two_bursts(&mut jobs, &root, "a\\nb", "c");

    let job = jobs.running.get_mut("job:1").unwrap();
    let (text, first) = job.take();
    assert_eq!(text, "a\nb");
    assert_eq!((first.first, first.lines), (1, 2));

    std::fs::write(&gate, "").unwrap();
    wait_for_the_job_to_end(&mut jobs);
    let job = jobs.running.get_mut("job:1").unwrap();
    let (text, second) = job.take();
    assert_eq!(text, "c");
    // Only the rest of line 2: no line begins in this look, so it counts none and the next
    // look starts at line 3.
    assert_eq!((second.first, second.lines), (3, 0), "{second:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// The sentence beside a look is the same words whatever the numbers are: 0, 1 and many lines
/// and a first line of 1 or later read alike, and a job's earlier looks are the only addition.
/// The numbers are inserted into the words and nothing in the words is chosen by them.
///
/// Rejects an implementation that spells `line` for one and `lines` for the rest, or that
/// words a look differently when `lines` is 0 or `first` is 1 (the "the rest of line N" and
/// "since it started" variants this sentence replaced), or that leaves the earlier looks out.
#[test]
fn the_sentence_beside_a_look_is_the_same_words_for_every_count() {
    let words = |lines: usize, first: usize| {
        Look {
            first,
            lines,
            earlier: Vec::new(),
        }
        .note("job:1")
    };
    let expected = |lines: usize, first: usize| {
        format!(
            " It holds the part of the output of job:1 that arrived since the previous look, \
             or since the job started when there was none. Lines of the job's whole output \
             that begin in it: {lines}, numbered from {first}. A line a look cuts is counted \
             in the look that holds its first byte, and the next look begins with the rest \
             of it."
        )
    };
    for (lines, first) in [(0, 1), (1, 1), (2, 1), (0, 4), (1, 4), (7, 4)] {
        assert_eq!(words(lines, first), expected(lines, first));
    }
    let with_earlier = Look {
        first: 3,
        lines: 0,
        earlier: vec!["ref:1".to_string(), "ref:2".to_string()],
    }
    .note("job:1");
    assert_eq!(
        with_earlier,
        format!(
            "{} Earlier looks at job:1 are ref:1, ref:2; the lines before these are in them, \
             and none of them is repeated here.",
            expected(0, 3)
        )
    );
}

/// Standard error between two looks does not move the next look's start: the label line and
/// the newline that joins the streams are not lines of the job.
///
/// Rejects counting `text.lines()` of the composed text, which counts `standard error:` as a
/// line (the second look would count 3 lines and the next start would be 5).
#[cfg(unix)]
#[test]
fn standard_error_between_looks_does_not_shift_the_next_start() {
    let root = std::env::temp_dir().join(format!("bravebot-err-{}", std::process::id()));
    let mut jobs = Jobs::new();
    let gate = job_running_commands(
        &mut jobs,
        &root,
        "printf 'a\\n'",
        "printf 'e1' >&2; printf 'c\\n'",
    );

    let job = jobs.running.get_mut("job:1").unwrap();
    let (text, first) = job.take();
    assert_eq!(text, "a\n");
    assert_eq!((first.first, first.lines), (1, 1));

    std::fs::write(&gate, "").unwrap();
    wait_for_the_job_to_end(&mut jobs);
    let job = jobs.running.get_mut("job:1").unwrap();
    let (text, second) = job.take();
    assert_eq!(text, "c\nstandard error:\ne1");
    // One line of standard output and one of standard error: lines 2 to 3, not the four the
    // text spells.
    assert_eq!((second.first, second.lines), (2, 2), "{second:?}");

    let job = jobs.running.get_mut("job:1").unwrap();
    let (_, third) = job.take();
    assert_eq!((third.first, third.lines), (4, 0), "{third:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// Each look at a job says which lines of everything the job printed it holds, and names the
/// looks before it that kept their lines apart. The count is the job's own, kept across looks:
/// a look that counted only its own lines would read as lines 1 to 3 every time.
#[cfg(unix)]
#[test]
fn a_look_counts_the_lines_the_job_has_handed_over_across_looks() {
    let root = std::env::temp_dir().join(format!("bravebot-looks-{}", std::process::id()));
    let mut jobs = Jobs::new();
    let gate = job_printing_in_two_bursts(&mut jobs, &root, "a\\nb\\n", "c\\nd\\ne\\n");

    // The screen is given the unread lines without the planner having looked, so it must not
    // move the count. Rejects counting in `unread_output`.
    assert_eq!(jobs.unread_output().len(), 1);

    let job = jobs.running.get_mut("job:1").unwrap();
    let (text, first) = job.take();
    assert_eq!(text, "a\nb\n");
    // Rejects a count that starts at 0, or one that is not kept from the first look, or one
    // already moved by the screen's look above.
    assert_eq!((first.first, first.lines), (1, 2));
    // Rejects naming a look that did not happen.
    assert!(first.earlier.is_empty(), "{first:?}");

    let job = jobs.running.get_mut("job:1").unwrap();
    let (text, nothing) = job.take();
    assert_eq!(text, "");
    // Rejects an empty look that moves the count, or one that reports line 0.
    assert_eq!((nothing.first, nothing.lines), (3, 0));

    jobs.record_look("job:1", "ref:7".to_string());
    std::fs::write(&gate, "").unwrap();
    let job = jobs.running.get_mut("job:1").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !job.running.has_more(&job.seen) {
        assert!(
            std::time::Instant::now() < deadline,
            "the second burst never arrived"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let (text, second) = job.take();
    assert_eq!(text, "c\nd\ne\n");
    // Rejects an empty look that advanced the count (3 would be 4), and a count that restarts
    // at each look (1).
    assert_eq!((second.first, second.lines), (3, 3));
    // Rejects a look that forgets the references recorded against its job.
    assert_eq!(second.earlier, vec!["ref:7".to_string()]);
    let _ = std::fs::remove_dir_all(&root);
}

/// The account of a job that ended takes its output through the same count as a look does, so
/// the range it states follows the lines an earlier look already handed over. A route that read
/// the output without the count would state lines 1 to 1 for the line after two others.
#[cfg(unix)]
#[test]
fn the_account_of_an_ended_job_continues_the_count_of_the_looks_before_it() {
    let root = std::env::temp_dir().join(format!("bravebot-ended-{}", std::process::id()));
    let mut jobs = Jobs::new();
    let gate = job_printing_in_two_bursts(&mut jobs, &root, "a\\nb\\n", "c\\n");

    let job = jobs.running.get_mut("job:1").unwrap();
    let (_, first) = job.take();
    assert_eq!((first.first, first.lines), (1, 2));
    jobs.record_look("job:1", "ref:4".to_string());

    std::fs::write(&gate, "").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let ended = loop {
        let mut ended = jobs.ended(OUTPUT_CAP);
        if let Some(ended) = ended.pop() {
            break ended;
        }
        assert!(std::time::Instant::now() < deadline, "the job never ended");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let printed = ended.printed.map(|printed| printed.into_trusted().ok());
    assert_eq!(printed, Some(Some("c\n".to_string())));
    assert_eq!((ended.look.first, ended.look.lines), (3, 1));
    assert_eq!(ended.look.earlier, vec!["ref:4".to_string()]);
    let _ = std::fs::remove_dir_all(&root);
}

/// The notes that start a job say a look is a slice: each returns what arrived since the
/// previous one, and the earlier lines stay in the references of the earlier looks. A note
/// that still promised "what is new" or the whole of what the job has printed leaves the
/// planner expecting the last reference to hold everything.
#[test]
fn the_notes_that_start_a_job_say_each_look_is_a_slice() {
    // Built through an existing constructor, so this test adds no `Labelled::trusted` site to
    // the count LABEL pins; both notes replace the text anyway.
    let blank = || Produced::problem(String::new());
    for (which, produced) in [
        (
            "started",
            blank().started_in_the_background("job:3".to_string(), false, Beside::every()),
        ),
        (
            "moved",
            blank().moved_to_the_background(
                "job:3".to_string(),
                std::time::Duration::from_secs(2),
                false,
                Beside::every(),
            ),
        ),
    ] {
        let told = produced.text.into_trusted().expect("trusted text");
        assert!(
            told.contains("since the previous look") && told.contains("earlier looks"),
            "{which}: the note does not say a look is a slice: {told}"
        );
        assert!(
            !told.contains("for what is new") && !told.contains("printed so far"),
            "{which}: the note still promises the whole of the output: {told}"
        );
    }
}

/// TOOL-6. The notes that start a job name `job_output` only where the turn's list holds it.
/// Without it the note says what happens to the job and nothing about looking at it, and the
/// full list reads the text written before the fact existed. The regression it rejects is a
/// note that names `job_output` to a turn that was not offered it.
#[test]
fn the_notes_that_start_a_job_name_job_output_only_where_it_is_offered() {
    let blank = || Produced::problem(String::new());
    let without = Beside::every().without(Named::JobOutput);
    let kept = what_becomes_of_a_job(false);
    let started = |beside| {
        blank()
            .started_in_the_background("job:3".to_string(), false, beside)
            .text
            .into_trusted()
            .expect("trusted text")
    };
    let moved = |beside| {
        blank()
            .moved_to_the_background(
                "job:3".to_string(),
                std::time::Duration::from_secs(2),
                false,
                beside,
            )
            .text
            .into_trusted()
            .expect("trusted text")
    };
    assert_eq!(
        started(Beside::every()),
        format!(
            "started in the background as job:3. Nothing has been read from it yet. If it ends \
             while this turn is still going you are told so, with how it ended and what it \
             printed, without having to ask; call job_output with \"job:3\" before then to see \
             what it has printed. Each look returns what arrived since the previous look, and \
             the output before it stays in the references of the earlier looks. {kept}"
        )
    );
    assert_eq!(
        started(without),
        format!(
            "started in the background as job:3. Nothing has been read from it yet. If it ends \
             while this turn is still going you are told so, with how it ended and what it \
             printed, without having to ask. {kept}"
        )
    );
    assert_eq!(
        moved(Beside::every()),
        format!(
            "the user moved this command to the background after 2.0s, and it is still running \
             as job:3. Do not run it again. Nothing has been read from it yet, including what it \
             printed before the move. If it ends while this turn is still going you are told \
             so, with how it ended and what it printed, without having to ask; call job_output \
             with \"job:3\" before then to see what it has printed. Each look returns what \
             arrived since the previous look, and the output before it stays in the references \
             of the earlier looks. It no longer has a deadline. {kept}"
        )
    );
    let told = moved(without);
    assert!(told.contains("Do not run it again."), "{told}");
    assert!(!told.contains("job_output"), "{told}");
    assert!(
        told.contains("without having to ask. It no longer has a deadline."),
        "{told}"
    );
}

/// The reference to an earlier look is recorded only while its job is held: a name the job no
/// longer answers to has nothing to list it under, and recording one must not panic or make an
/// entry.
#[test]
fn a_look_recorded_against_a_job_that_is_not_held_is_dropped() {
    let mut jobs = Jobs::new();
    jobs.record_look("job:9", "ref:1".to_string());
    assert!(jobs.is_empty());
}

/// A stop the person asked for that no round was left to read is still theirs at the turn's
/// end: the screen and the trail say they stopped it, and a job nobody asked about is still
/// the one the turn ended.
#[test]
fn a_stop_no_round_read_is_reported_at_the_turns_end_as_the_persons() {
    use crate::report::{JobEvent, Outcome};
    use bravebot_core::TrustStore;
    use bravebot_core::capability::CapabilitySet;
    use bravebot_core::event::{Event, RecordingSink};
    use bravebot_core::file_authority::FileAuthority;
    use bravebot_core::policy::{ReleasePlan, Routing};

    let root = std::env::current_dir().unwrap();
    let mut jobs = Jobs::new();
    let mut stops = Vec::new();
    for _ in 0..2 {
        let plan = crate::cmdline::compile("sleep 30", &root, None, &mut |_, _| Ok(())).unwrap();
        let bravebot_core::command::Steps::Pipeline(steps) = &plan.steps else {
            panic!("one pipeline");
        };
        let running = crate::exec::start_steps_within(
            steps,
            &root,
            None,
            None,
            &bravebot_core::cancel::Cancel::new(),
            crate::exec::LIMIT,
        )
        .unwrap();
        let (_, stop) = jobs.keep(
            running,
            JobLines::new(
                &plan.display(),
                &crate::cmdline::display_as_written(&plan, None),
            ),
            Label::trusted_public(),
            FileAuthority::new(TrustStore::new(&root)),
            0,
            false,
        );
        stops.push(stop);
    }
    stops[0].request();

    let mut sink = RecordingSink::new();
    let mut reporter = crate::report::RecordingReporter::default();
    let mut routing = Routing::new();
    routing.insert_trusted("task", "start two jobs");
    {
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::none(),
            &mut sink,
        )
        .expect("policy");
        jobs.stop_all(&mut policy, &mut reporter);
    }

    assert!(
        matches!(
            reporter.jobs.as_slice(),
            [
                JobEvent::Ended { name, outcome: Outcome::StoppedByTheUser(_) },
                JobEvent::Dropped { name: dropped },
            ] if name == "job:1" && dropped == "job:2"
        ),
        "{:?}",
        reporter.jobs
    );
    let stopped: Vec<&String> = sink
        .events()
        .iter()
        .filter_map(|event| match event {
            Event::GatePassed {
                gate: "job_stop",
                detail,
            } => Some(detail),
            _ => None,
        })
        .collect();
    assert!(
        matches!(stopped.as_slice(), [detail] if detail.starts_with("job:1 stopped by the user")),
        "{stopped:?}"
    );
}

/// A job that exited before any step read the person's stop is reported as what it did, at a
/// round and at the turn's end. Nothing was stopped, and its exit code is the news.
#[test]
fn a_job_that_exited_before_its_stop_was_read_is_reported_by_its_exit() {
    use crate::report::{JobEvent, Outcome};
    use bravebot_core::TrustStore;
    use bravebot_core::capability::CapabilitySet;
    use bravebot_core::event::{Event, RecordingSink};
    use bravebot_core::file_authority::FileAuthority;
    use bravebot_core::policy::{ReleasePlan, Routing};

    let root = std::env::current_dir().unwrap();
    let mut jobs = Jobs::new();
    let exited_and_asked = |jobs: &mut Jobs| {
        let plan = crate::cmdline::compile("true", &root, None, &mut |_, _| Ok(())).unwrap();
        let bravebot_core::command::Steps::Pipeline(steps) = &plan.steps else {
            panic!("one pipeline");
        };
        let running = crate::exec::start_steps_within(
            steps,
            &root,
            None,
            None,
            &bravebot_core::cancel::Cancel::new(),
            crate::exec::LIMIT,
        )
        .unwrap();
        let (name, stop) = jobs.keep(
            running,
            JobLines::new(
                &plan.display(),
                &crate::cmdline::display_as_written(&plan, None),
            ),
            Label::trusted_public(),
            FileAuthority::new(TrustStore::new(&root)),
            0,
            false,
        );
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !jobs.running.get_mut(&name).unwrap().running.ended() {
            assert!(std::time::Instant::now() < until, "`true` did not exit");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        stop.request();
    };

    exited_and_asked(&mut jobs);
    let at_the_round: Vec<(String, Outcome)> = jobs
        .ended(OUTPUT_CAP)
        .into_iter()
        .map(|ended| (ended.name, ended.outcome))
        .collect();
    assert_eq!(at_the_round, [("job:1".to_string(), Outcome::Succeeded)]);

    exited_and_asked(&mut jobs);
    let mut sink = RecordingSink::new();
    let mut reporter = crate::report::RecordingReporter::default();
    let mut routing = Routing::new();
    routing.insert_trusted("task", "start a job");
    {
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::none(),
            &mut sink,
        )
        .expect("policy");
        jobs.stop_all(&mut policy, &mut reporter);
    }
    assert!(
        matches!(
            reporter.jobs.as_slice(),
            [JobEvent::Ended { name, outcome: Outcome::Succeeded }] if name == "job:2"
        ),
        "{:?}",
        reporter.jobs
    );
    assert!(
        !sink.events().iter().any(|event| matches!(
            event,
            Event::GatePassed {
                gate: "job_stop",
                ..
            }
        )),
        "{:?}",
        sink.events()
    );
}

/// Start `line` as a job of `jobs` and wait, to a bound, until its steps have exited. The pipes
/// are not waited for: `Background::ended` would pay the drain's grace on top.
fn start_a_job_and_wait_for_its_steps(jobs: &mut Jobs, line: &str) {
    let (name, _stop) = jobs.start_for_a_test(line, Label::trusted_public());
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !jobs.running.get_mut(&name).unwrap().running.steps_exited() {
        assert!(std::time::Instant::now() < until, "`{line}` did not exit");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// End the turn over `jobs` and return what the person was told about each.
fn end_the_turn_over(jobs: &mut Jobs) -> Vec<crate::report::JobEvent> {
    use bravebot_core::capability::CapabilitySet;
    use bravebot_core::event::RecordingSink;
    use bravebot_core::policy::{ReleasePlan, Routing};

    let mut sink = RecordingSink::new();
    let mut reporter = crate::report::RecordingReporter::default();
    let mut routing = Routing::new();
    routing.insert_trusted("task", "start jobs");
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::none(),
        &mut sink,
    )
    .expect("policy");
    jobs.stop_all(&mut policy, &mut reporter);
    reporter.jobs
}

/// Ending a turn over jobs that ignore the request to end takes one grace period for all of
/// them. Each job is stopped with the same deadline and not given a fresh one after the job
/// before it, so two such jobs take about two seconds and not four. Every job is still
/// reported as the turn's stop, since each was classified before any was asked.
#[cfg(unix)]
#[test]
fn jobs_that_ignore_the_request_to_end_share_one_grace_at_the_turns_end() {
    use crate::report::JobEvent;

    let mut jobs = Jobs::new();
    let names: Vec<String> = (0..2)
        .map(|_| {
            jobs.start_for_a_test(
                "sh -c 'trap \"\" TERM; echo ready; while :; do sleep 1; done'",
                Label::trusted_public(),
            )
            .0
        })
        .collect();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    for name in &names {
        while !jobs.running[name].running.printed().contains("ready") {
            assert!(
                std::time::Instant::now() < until,
                "{name} never became ready"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    let started = std::time::Instant::now();
    let told = end_the_turn_over(&mut jobs);
    let took = started.elapsed();

    assert!(
        took < std::time::Duration::from_millis(3500),
        "ending the turn over two jobs that ignore the request took {took:?}, one grace each"
    );
    assert!(
        matches!(
            told.as_slice(),
            [JobEvent::Dropped { .. }, JobEvent::Dropped { .. }]
        ),
        "{told:?}"
    );
}

/// RUN-26. A job whose steps exited before the turn ended is reported at its end as what it
/// did, from its exit codes, though nobody asked to stop it. Saying the turn stopped a program
/// that had already finished would be false, and a failed one would read as a success.
#[test]
fn a_job_that_exited_by_itself_is_reported_by_its_codes_at_the_turns_end() {
    use crate::report::{JobEvent, Outcome};

    let mut jobs = Jobs::new();
    for line in ["true", "false"] {
        start_a_job_and_wait_for_its_steps(&mut jobs, line);
    }

    let told = end_the_turn_over(&mut jobs);

    assert!(
        matches!(
            told.as_slice(),
            [
                JobEvent::Ended { name: first, outcome: Outcome::Succeeded },
                JobEvent::Ended { name: second, outcome: Outcome::Failed { detail, .. } },
            ] if first == "job:1" && second == "job:2" && detail == "step 1 exited 1"
        ),
        "{told:?}"
    );
}

/// RUN-26. The turn's end asks whether a job's steps exited and not whether its pipes drained.
/// A step that leaves a child holding the pipe open would otherwise hold the end of the turn
/// for the drain's grace, once for each job.
///
/// The shell exits at once and leaves the sleep holding its end of the output pipe. The bound
/// is half of the two seconds `exec` gives a pipe to drain, which a turn that waited on it
/// would spend whole.
#[test]
fn the_turns_end_does_not_wait_for_a_jobs_pipes_to_drain() {
    use crate::report::{JobEvent, Outcome};

    let mut jobs = Jobs::new();
    start_a_job_and_wait_for_its_steps(&mut jobs, "sh -c 'sleep 4 & exit 0'");

    let asked = std::time::Instant::now();
    let told = end_the_turn_over(&mut jobs);
    let waited = asked.elapsed();

    assert!(
        waited < std::time::Duration::from_secs(1),
        "the turn's end waited {waited:?} for a job's pipes to drain"
    );
    assert!(
        matches!(
            told.as_slice(),
            [JobEvent::Ended {
                outcome: Outcome::Succeeded,
                ..
            }]
        ),
        "{told:?}"
    );
}
