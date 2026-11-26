use super::*;

/// A wait is a way to spend part of one turn watching. A planner that reads it as a way to be
/// told about something after the turn ends waits for news that cannot arrive, so the tool says
/// where the wait ends and what happens to the job then.
#[test]
fn job_output_says_its_wait_ends_with_the_turn() {
    let said = tool_description(Scheduling::ArrangingALook, "job_output");
    for stated in [
        "a wait inside this turn",
        "anything that happens after the turn ends",
        "says whether it is killed then or kept",
    ] {
        assert!(
            said.contains(stated),
            "job_output no longer says '{stated}': {said}"
        );
    }
}

/// A tool's description is the only instruction the planner reliably reads, so wording that
/// changes behaviour is behaviour. This one has to say that narrowing inside the line is the
/// cheapest thing available, and give the shapes rather than gesture at them.
#[test]
fn the_run_description_tells_the_planner_to_filter_at_the_source() {
    let described = run_description();
    for shape in ["head -n", "-l", "-c", "sed -n"] {
        assert!(
            described.contains(shape),
            "the description does not give `{shape}` as a way to narrow: {described}"
        );
    }
    assert!(
        described.contains("returns less"),
        "the description does not say to prefer whichever returns less: {described}"
    );
}

/// The planner learns which forms are refused only from this description, and a `$?` written to
/// report an exit status is refused for a reason it cannot guess: the run result already says how
/// each step ended. A description that names only `$VAR` leaves it writing `echo "exit=$?"`.
#[test]
fn the_run_description_names_the_special_parameters_and_the_dollar_quote_as_refused() {
    let described = run_description();
    for form in [
        "`$?`", "`$$`", "`$@`", "`$*`", "`$#`", "`$-`", "`$!`", "`$'...'`",
    ] {
        assert!(
            described.contains(form),
            "the description does not name {form} as refused: {described}"
        );
    }
    assert!(
        described.contains("result of a run already says how each step ended")
            && described.contains("a newline comes from a file or from standard input"),
        "the description does not say what to do instead: {described}"
    );
    assert!(
        described.contains("Double quotes do not")
            && described.contains("no name, brace, parenthesis, special parameter or quote"),
        "the description does not say what quoting settles or when a `$` is ordinary: {described}"
    );
}

/// The planner learns which spelling is safe only from this description, so it gives both
/// cases: a pattern given to `find -name` is quoted, since a file matching it would replace
/// it, and a pattern after an option's `=` is not.
#[test]
fn the_run_description_says_when_a_pattern_for_the_program_is_quoted() {
    let described = run_description();
    assert!(
        described.contains("`find . -name '*.md'`"),
        "the description does not show the quoted pattern: {described}"
    );
    assert!(
        described.contains("replaced by the files it matches here")
            && described.contains("passed as written only when none matches"),
        "the description does not say what an unquoted pattern becomes: {described}"
    );
    assert!(
        described.contains("`grep -r x . --include=*.md`, is passed to the program as written"),
        "the description does not say an option's pattern is passed as written: {described}"
    );
}

/// A planner that writes `cd sub && cat a.txt` gets a line whose later steps run at the root.
/// The description is the only place it can learn before writing the line that `directory` is
/// the way to run elsewhere. A description that drops the sentence about `cd`, or keeps it but
/// leaves out that the `directory` argument is how a line runs elsewhere, fails this test, and
/// the planner goes on writing `cd` lines that are refused.
#[test]
fn the_run_description_says_cd_moves_nothing_and_directory_is_how_a_line_runs_elsewhere() {
    let described = run_description();
    assert!(
        described.contains("`cd` step is refused")
            && described.contains("`directory` argument is how a line runs elsewhere"),
        "the description does not say cd is refused and directory is the route: {described}"
    );
}

/// A planner that writes `ulimit -n 10240 && cargo test` is refused when the line is compiled,
/// and on a host with `/usr/bin/ulimit` the line used to run with a stage that changed
/// nothing. The description is the only place the planner can learn before writing the line
/// that the whole line goes inside `sh -c`. A description that names `ulimit` without the
/// `sh -c` spelling, or names `cd` alone, fails this test.
#[test]
fn the_run_description_says_a_limit_or_mask_step_is_refused_and_goes_inside_sh_c() {
    let described = run_description();
    for part in [
        "`ulimit`, `umask`, `unset`, `alias`, `unalias`, `hash` or `wait` step is refused",
        "changes nothing for the steps after it",
        "`sh -c '... ulimit ...'`",
        "approved as one line",
    ] {
        assert!(
            described.contains(part),
            "the description lacks `{part}`: {described}"
        );
    }
}

/// A session with a browser already running and its debugging port open asked the person
/// whether it could quit and relaunch the browser, because every way of looking it tried for
/// the process was refused and it never tried the port. The description tells the planner to
/// try the port first and to ask about quitting only after that is refused, in every turn that
/// is offered `run`, which is each kind of scheduling and a delegate that holds `run`. The
/// probe sentence says nothing of whether the probe needs a scope, network access or
/// `loopback`, because whether a line may connect to a port of this machine depends on the
/// platform and on a host list, which the description does not know (SANDBOX-3). None of the
/// three sentences names a tool, so a delegate and an addressed turn read nothing they cannot
/// act on (TOOL-6), and none names `loopback`, which the `scopes` entry and the confinement
/// text name.
///
/// The faults it rejects are no sentence, the question allowed before the probe, a probe
/// sentence that says the probe needs no scope, network access or `loopback`, which would not
/// hold under a host list and may not hold on Windows, and would leave a planner whose probe
/// was refused asking about the browser, a sentence that names a tool, and a sentence missing
/// from one kind of scheduling or from a delegate.
#[test]
fn the_run_description_says_to_probe_a_browsers_debugging_port_before_asking_about_quitting_it() {
    use bravebot_core::delegate::Kind;
    let everything: Vec<String> = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .map(|tool| tool.function.name)
    .collect();
    let read = |at: &str, described: &str| {
        let first = sentence_with(described, "Before asking");
        assert!(first.starts_with("Before asking"), "{at}: {first}");
        assert!(
            first.contains("whether a browser is running")
                && first.contains("whether to quit or relaunch it"),
            "{at}: {first}"
        );
        let probe = sentence_with(described, "/json/version");
        assert!(
            probe.contains("`curl http://127.0.0.1:9222/json/version`")
                && probe.contains("--remote-debugging-port=9222"),
            "{at}: {probe}"
        );
        for said in ["scope", "network access", "loopback"] {
            assert!(!probe.contains(said), "{at}: {said} in {probe}");
        }
        let ask = sentence_with(described, "only after that probe");
        assert!(
            ask.starts_with("Ask about quitting or relaunching a browser only after")
                && ask.contains("is refused"),
            "{at}: {ask}"
        );
        let order = [first, probe, ask].map(|sentence| described.find(sentence));
        assert!(
            order[0] < order[1] && order[1] < order[2] && order[0].is_some(),
            "{at}: asked before the probe is described: {described}"
        );

        // TOOL-6: no word of the three sentences is the name of a tool this turn may lack.
        for sentence in [first, probe, ask] {
            for word in sentence.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                assert!(!everything.iter().any(|tool| tool == word), "{at}: {word}");
            }
            assert!(!sentence.contains("loopback"), "{at}: {sentence}");
        }
    };

    for scheduling in [
        Scheduling::ArrangingALook,
        Scheduling::PacingALoop,
        Scheduling::TheirInterval,
        Scheduling::NoLaterLook,
    ] {
        read(
            &format!("{scheduling:?}"),
            &tool_description(scheduling, "run"),
        );
    }

    // A delegate is built from the same table with a list of its own, so each kind that holds
    // `run` reads the sentences too, and at least one kind does.
    let mut holding = 0;
    for name in Kind::NAMES {
        let kind = Kind::from_name(name).expect("enumerated");
        let held = for_delegate(&kind.capabilities(), None, None, Deadlines::BUILT_IN);
        if let Some(run) = held.iter().find(|tool| tool.function.name == "run") {
            holding += 1;
            read(&format!("delegate {name}"), &run.function.description);
        }
    }
    assert!(holding > 0, "no kind of delegate is offered `run`");
}

/// A session asked to watch a file read it once, said what it held, and left nothing watching.
/// Both techniques already existed and nothing joined a watch request to either, so the
/// description has to: which one a bound picks, which one outlives a turn, and that the turn
/// arranges the later look itself instead of asking the person to arrange it.
#[test]
fn the_run_description_routes_a_watch_request_to_one_of_the_two_techniques() {
    let described = run_description();
    for stated in [
        "watch something",
        "background: true",
        "wait_seconds",
        "killed when the turn ends",
        "call schedule_next at the end of the turn",
    ] {
        assert!(
            described.contains(stated),
            "the description does not say '{stated}': {described}"
        );
    }
    // A file is watched by comparing read_file's token, and `tail -f` was the wrong recipe
    // twice over: it is not in the read-proven table, so every watch of a file cost an
    // approval, and it sees appends only, so a file truncated or replaced looked untouched.
    assert!(
        described.contains("read_file hands back a change token"),
        "the description does not route a file to the token it can compare: {described}"
    );
    assert!(
        !described.contains("tail -f"),
        "the description still names tail -f as the way to watch a file: {described}"
    );
    // The failure that shipped: an open-ended request landed in the loop branch, which asks for
    // nothing to be done, so the turn made no tool call at all and reported no watch.
    assert!(
        described.contains("take the first look now"),
        "the description lets an unbounded watch request end the turn with no look: {described}"
    );
    assert!(
        described.contains("scheduled no further look, say that too"),
        "the description does not say to report that nothing is watching: {described}"
    );
    // The same sentence spanning the join, so a formatting pass that swallows the line break
    // is caught here rather than reaching a planner as a run of spaces mid-sentence.
    assert!(
        described.contains("rather than a time of day"),
        "the description came out with its continuation baked in: {described}"
    );
    // Said, because the planner has no clock: the preamble gives it today's date and tells it
    // not to run `date`, so an instruction to date a sample invites it to invent a time.
    assert!(
        described.contains("no clock for"),
        "the description asks the planner for a time of day it cannot know: {described}"
    );
}

/// RUN-29. A planner writing the line that sends a message reads `run`'s description and the
/// `stdin_ref` parameter, and neither said that the body of a summary or a translation of
/// something it was not shown is a processor's to write. The output sentence is where `run`
/// already names the processor as a place a quarantined output goes, so the pointer is there,
/// and the how stays in the processor's description (SPAWN-6) rather than being said twice.
/// `stdin_ref` is where a processor's answer is handed to the program with no file.
#[test]
fn the_run_description_points_text_made_from_an_output_at_the_processor() {
    let described = run_description();
    assert!(
        described.contains(
            "hand it to spawn_processor, or write it to a file with write_file. A summary, \
             a translation or other text made from it is written by spawn_processor, whose \
             description says how that text reaches a program."
        ),
        "the output sentence does not send derived text to the processor: {described}"
    );

    let offered = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let stdin = offered
        .iter()
        .find(|t| t.function.name == "run")
        .expect("run is offered")
        .function
        .parameters["properties"]["stdin_ref"]["description"]
        .as_str()
        .expect("a description")
        .to_string();
    assert!(
        stdin.contains(
            "A program that sends text, such as a summary a processor wrote, takes its body \
             this way: name the processor's answer here."
        ),
        "stdin_ref does not say a processor's answer can be a body: {stdin}"
    );
    // The line that already stood in this parameter still stands.
    assert!(
        stdin.contains("do not also add a '<' redirection to the line"),
        "stdin_ref lost its advice about redirection: {stdin}"
    );
}

/// The two descriptions that route a request to be told when something changes have to point
/// at a later look that exists. Where a wait would be discarded there is none, and a planner
/// told to arrange one spends a round on a name that is not there and then has a refusal to
/// explain to somebody who asked to be told about a change.
///
/// Each turn is checked for the wrong sentence as well as the right one: a description that
/// carried all three answers at once would satisfy any test that only looked for the one it
/// wanted, and would leave the planner picking whichever it read first.
#[test]
fn what_a_watch_request_is_told_about_the_next_look_matches_what_this_turn_can_arrange() {
    let described = |scheduling, name: &str| {
        available(
            scheduling,
            Arming::Allowed { free: 1 },
            Deadlines::BUILT_IN,
            Running::Offered,
        )
        .into_iter()
        .find(|t| t.function.name == name)
        .unwrap_or_else(|| panic!("{name} is offered"))
        .function
        .description
    };

    for name in ["read_file", "run"] {
        let arranging = described(Scheduling::ArrangingALook, name);
        assert!(
            arranging.contains("call schedule_next at the end of th"),
            "{name} did not tell a turn that can arrange a look to arrange one: {arranging}"
        );

        // A tick's next look is the next tick, so a tick told to schedule one would be
        // arranging a second look nobody asked for.
        let pacing = described(Scheduling::PacingALoop, name);
        assert!(
            pacing.contains("Inside a loop"),
            "{name} did not tell a tick its next look is the next tick: {pacing}"
        );
        assert!(
            !pacing.contains("call schedule_next at the end of th"),
            "{name} had a tick arrange a look the loop is already taking: {pacing}"
        );

        // And a turn nothing will ask again has no later look at all, so the honest answer is
        // to say that nothing is watching rather than to promise a report.
        let last = described(Scheduling::NoLaterLook, name);
        assert!(
            last.to_lowercase().contains("nothing will ask you again"),
            "{name} did not tell a turn nothing will ask again to say so: {last}"
        );
        assert!(
            !last.contains("schedule_next"),
            "{name} sent a turn nothing will ask again to a tool it was not offered: {last}"
        );
    }
}

/// The instruction that measurably steered a session into the expensive path. A capped
/// structured search returns thousands of tokens of truncated matches where `grep -rl` returns
/// a dozen lines, and a planner obeying that sentence pays the difference every round.
#[test]
fn the_run_description_does_not_send_the_planner_to_the_other_tools_instead() {
    let described = run_description().to_lowercase();
    for steer in [
        "do not use it to read something read_file",
        "prefer read_file",
        "prefer search",
    ] {
        assert!(
            !described.contains(steer),
            "the description steers away from a command line: {described}"
        );
    }
}

fn run_description() -> String {
    tool_description(Scheduling::ArrangingALook, "run")
}

/// Blind output is the default, not the rule, and describing it as the rule is what made
/// compiling look pointless: a planner told it will never see what a program printed has no
/// reason to run a build. Vouching is the way out and the description has to say so.
#[test]
fn run_says_a_vouched_command_comes_back_readable() {
    let tool = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "run")
    .expect("run is offered");
    let description = &tool.function.description;

    assert!(
        description.contains("vouched"),
        "run's description does not mention vouching, so blind output reads as permanent"
    );
    assert!(
        description.contains("compile and test"),
        "run's description does not say to use it for building and testing"
    );
    // The old wording promised the output would never be shown, which is false for a vouched
    // command and is the sentence a planner reasoned from.
    assert!(
        !description.contains("You will NOT be shown the output"),
        "run's description still claims output is never shown"
    );
}

/// A read is a sample of one moment, so a planner told nothing else answers a question about
/// change from a snapshot: it reads the file, describes what is in it, and leaves nothing to
/// compare. The description has to hand it the comparison instead, and say what the comparison
/// does not settle.
///
/// What it must not send the planner to is a program that reports a time or a hash. Those are
/// not in the read-proven table, so their output comes back quarantined, and a planner told to
/// compare three values it is handed as references cannot compare anything. The token exists
/// because the driver can hand over what those programs cannot.
#[test]
fn read_file_sends_a_question_about_change_to_a_token_it_can_compare() {
    let tool = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "read_file")
    .expect("read_file is offered");
    let description = &tool.function.description;

    for stated in [
        "change token",
        "comparing it with the token from",
        // The baseline is taken in the turn the question is asked, rather than deferred to a
        // loop that nobody has started: a turn that names /loop and reads nothing answers less
        // than the single read this clause exists to correct.
        "Take that baseline in this turn",
        "not what changed",
        "no clock for",
        // A session told somebody to type `/loop 10s`, which starts nothing. Handing over a
        // line for the person to type was the wrong shape of answer: the turn can arrange the
        // look itself, so it does.
        "call schedule_next at the end of this turn",
        "rather than telling somebody to arrange it themselves",
        "not scheduled another, say so",
    ] {
        assert!(
            description.contains(stated),
            "read_file's description no longer says '{stated}'"
        );
    }
    for absent in ["stat -f", "stat -c", "shasum"] {
        assert!(
            !description.contains(absent),
            "read_file's description sends the planner to '{absent}', whose output is \
             quarantined and so cannot be compared"
        );
    }
}

/// The description says what the token settles, for a file the planner may be shown and one it
/// may not, and the two ways a comparison falls short. A planner not told the second reports a
/// moved token as a changed file, or an unmoved one as a file nobody wrote.
#[test]
fn read_file_says_what_a_change_token_does_and_does_not_settle() {
    let description = read_file_description(Scheduling::ArrangingALook);

    for stated in [
        "The same token on a later read means nobody wrote the file in between",
        "a different one means somebody did",
        "the size in its reference is what there is to compare instead",
        "say which looks you compared",
        "a rewrite of the same bytes moves it too",
        "a change that leaves the file's modification time alone moves nothing",
    ] {
        assert!(
            description.contains(stated),
            "read_file's description no longer says '{stated}'"
        );
    }
}

/// Inside a loop the next tick is the next look, so the description must not send the planner
/// to arrange one. Told to schedule a look from inside a loop, it starts a second clock beside
/// the one already running.
#[test]
fn inside_a_loop_read_file_says_there_is_nothing_to_arrange() {
    for scheduling in [Scheduling::PacingALoop, Scheduling::TheirInterval] {
        let description = read_file_description(scheduling);

        assert!(
            description.contains("there is nothing to arrange"),
            "a loop's read_file does not say the next tick is the next look: {description}"
        );
        assert!(
            !description.contains("call schedule_next"),
            "a loop's read_file sends the planner to schedule a look: {description}"
        );
    }
    assert!(
        !read_file_description(Scheduling::ArrangingALook).contains("there is nothing to arrange"),
        "a turn with a look to arrange is told there is nothing to arrange"
    );
}

fn read_file_description(scheduling: Scheduling) -> String {
    tool_description(scheduling, "read_file")
}

fn tool_description(scheduling: Scheduling, name: &str) -> String {
    available(
        scheduling,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == name)
    .unwrap_or_else(|| panic!("{name} is offered"))
    .function
    .description
}

/// The sentence of `description` that holds `needle`. A condition counts only when it opens
/// that sentence: one placed in a neighbour, or trailing after the instruction, leaves the
/// instruction unconditional for a planner that acts on the sentence it is reading.
fn sentence_with<'a>(description: &'a str, needle: &str) -> &'a str {
    description
        .split(". ")
        .find(|sentence| sentence.contains(needle))
        .unwrap_or_else(|| panic!("no sentence says '{needle}': {description}"))
}

const WATCH_REQUEST: &str = "Where the request is to watch something or to be told when it changes";

/// An answer to a request that had nothing to do with watching or change ended on a line
/// saying no later look was scheduled, which reads as if a first look had been taken and a
/// repeat could have been arranged. The line is for a request to watch, so the sentence that
/// asks for it opens with that condition, and every other request is told it gets none.
#[test]
fn no_further_look_is_reported_only_for_a_request_to_watch_or_be_told_of_a_change() {
    for scheduling in [
        Scheduling::ArrangingALook,
        Scheduling::PacingALoop,
        Scheduling::TheirInterval,
        Scheduling::NoLaterLook,
    ] {
        for (name, instruction) in [
            ("read_file", "not scheduled another, say so"),
            ("run", "scheduled no further look, say that too"),
        ] {
            let description = tool_description(scheduling, name);
            assert!(
                sentence_with(&description, instruction).starts_with(WATCH_REQUEST),
                "{name} has the planner report no further look for any request: {description}"
            );
            assert!(
                description.contains("Any other request gets no sentence about a later look"),
                "{name} does not tell the planner a request that did not ask gets no such line: \
                 {description}"
            );
        }
    }
}

/// A turn nothing will ask again, which is what a delegate and the desktop app's turns are,
/// is told to say plainly that no later look is coming. A delegate that read a file and ran
/// one command for a request that had nothing to do with change ended its report on that
/// line, and the planner repeated it in the final answer. The request a delegate holds is
/// its task, so the condition names a request and not the user.
#[test]
fn a_turn_nothing_asks_again_announces_no_later_look_only_for_a_watch_request() {
    use bravebot_core::capability::{Capability, CapabilitySet};
    let delegate = for_delegate(
        &CapabilitySet::from_iter([Capability::FileRead, Capability::ShellExec]),
        None,
        None,
        Deadlines::BUILT_IN,
    );
    for (name, announcement) in [
        ("read_file", "no later look is coming"),
        ("run", "no later one is coming"),
    ] {
        let given_to_a_delegate = delegate
            .iter()
            .find(|t| t.function.name == name)
            .unwrap_or_else(|| panic!("{name} is offered to this delegate"))
            .function
            .description
            .clone();
        for description in [
            tool_description(Scheduling::NoLaterLook, name),
            given_to_a_delegate,
        ] {
            assert!(
                sentence_with(&description, announcement).starts_with(WATCH_REQUEST),
                "{name} has a turn nothing asks again announce it for any request: \
                 {description}"
            );
        }
    }
}

/// The tool must tell the planner it will not see the output, or it spends rounds running
/// things to read results that never come back to it.
#[test]
fn run_says_its_output_does_not_come_back_to_the_planner() {
    let tool = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "run")
    .expect("run is offered");
    let described = tool.function.description.to_lowercase();
    assert!(
        described.contains("not be shown") || described.contains("reference"),
        "run does not say the output is quarantined: {described}"
    );
    assert!(
        described.contains("approve"),
        "run does not say the user approves it first"
    );
}
