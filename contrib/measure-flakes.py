#!/usr/bin/env python3
"""Runs the suite repeatedly against one build and names every test that disagreed with itself.

docs/development/checks.md treats a failure that does not reproduce as a flake and says to carry on.
That is sound advice only if somebody knows which tests actually do that, and nothing measured it,
so the answer was a shrug rather than a name and a rate.

One build, N runs of it, and a report of every test whose outcome was not the same each time. The
report is the product rather than an exit code: a test failing three runs in thirty is what that
policy is describing, and going red over it would teach people to ignore the job. Only a broken
measurement or a report that could not be delivered fails here.

    contrib/measure-flakes.py --runs 30
    contrib/measure-flakes.py --runs 30 --test-threads 4
    contrib/measure-flakes.py --runs 30 --file-issues brave/bravebot --dry-run
"""

import argparse
import json
import os
import re

# Only ever invoked with an argument list, never a shell string.
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
from collections import Counter, deque

# Cargo names each test binary on stderr before that binary writes anything on stdout, so the two
# streams are read as one: the announcement is the only thing that says which binary a test name
# belongs to.
BINARY = re.compile(r"^\s*Running (?:unittests )?\S+ \(([^)]+)\)\s*$")
DOC_TESTS = re.compile(r"^\s*Doc-tests \S+\s*$")
OUTCOME = re.compile(r"^test (.+?) \.\.\. (ok|FAILED|ignored)")
FAILURE_TEXT = re.compile(r"^---- (.+?) stdout ----$")
BUILD_HASH = re.compile(r"-[0-9a-f]{8,}$")

# What is measured. `--no-fail-fast` because a run that stops at the first failing binary leaves
# every later test unmeasured, and the rates would then depend on which binary failed.
SUITE = ["cargo", "test", "--all", "--locked", "--no-fail-fast"]

# Enough of a failure to recognise it and group it with its family, not enough to bury the table it
# sits under. A test can print a great deal before it fails, so the tail of the block is kept and
# trimmed around the failure rather than truncated at the top.
SAMPLE_LINES = 12
SAMPLE_LIMIT = 500

# The title the one hand-filed flake here already carries (#76), so a test somebody has opened an
# issue for is recognised as covered rather than filed a second time.
TITLE = "Flaky test"

# How many issues one run may open. A suite nobody has measured could have twenty flaky tests in it,
# and twenty issues arriving at once is a thing people close in a batch without reading. The rest
# are named in the report and the next run files them.
ISSUE_LIMIT = 10


def qualify(binary, name):
    """A test name that says where to look for it.

    A doc-test names its own file and line, so it is already unambiguous; every other test name is
    unique only within its binary, and `exec` is both the binary stem and the source file.
    """
    return f"{binary}::{name}" if binary else name


def trim(captured):
    """The failure, rather than the first lines of whatever the test printed before it.

    A test that logs twenty lines and then panics would otherwise be reported as twenty lines of
    log with the panic cut off, which is the one thing the sample is for.
    """
    lines = list(captured)
    for index, line in enumerate(lines):
        if "panicked at" in line:
            return lines[index : index + SAMPLE_LINES]
    return lines[-SAMPLE_LINES:]


def parse(stream):
    """One run's outcomes as `{test: ok|FAILED|ignored}`, and the failure text of what failed."""
    outcomes = {}
    samples = {}
    failed = set()
    binary = None
    failing = None
    # The tail, because a test that writes megabytes before it panics should cost neither the memory
    # nor the panic line.
    captured = deque(maxlen=SAMPLE_LIMIT)

    for raw in stream:
        line = raw.rstrip("\n")
        # A test is entitled to print a line shaped exactly like libtest's heading, and taking that
        # at its word attributes the panic after it to a test nothing ran. libtest prints a binary's
        # outcome lines before any of its captured output, so a heading naming a test that did not
        # fail is a test talking.
        heading = FAILURE_TEXT.match(line)
        starts_sample = heading is not None and qualify(binary, heading.group(1)) in failed

        if failing is not None:
            # libtest closes the captured output of one test with the next heading, the list of
            # failed names, or the binary's result line.
            if not (
                starts_sample
                or line.startswith("failures:")
                or line.startswith("test result:")
            ):
                captured.append(line)
                continue
            samples.setdefault(failing, "\n".join(trim(captured)).strip())
            failing = None
            captured.clear()

        if starts_sample:
            failing = qualify(binary, heading.group(1))
            continue

        found = BINARY.match(line)
        if found:
            binary = BUILD_HASH.sub("", os.path.basename(found.group(1)))
            continue

        if DOC_TESTS.match(line):
            binary = None
            continue

        found = OUTCOME.match(line)
        if found:
            test = qualify(binary, found.group(1))
            outcomes[test] = found.group(2)
            if found.group(2) == "FAILED":
                failed.add(test)

    if failing is not None:
        samples.setdefault(failing, "\n".join(trim(captured)).strip())

    return outcomes, {test: text for test, text in samples.items() if text}


def tally(per_run):
    """Every test seen across the runs, with how each run ended for it."""
    counts = {}
    for outcomes in per_run:
        for test, outcome in outcomes.items():
            counts.setdefault(test, Counter())[outcome] += 1
    return counts


def disagreed(counts, runs):
    """The tests that did not fail every run, worst rate first, and those that did.

    A test that fails every run of the suite is not a flake. It is a broken test or broken code,
    and saying so separately keeps it out of a table people read as a list of things to distrust.
    A test that failed every run it *reported* in, having gone missing from the others, is a flake
    of the worst kind: its binary stopped early, so the rest of that binary went unmeasured too.
    """
    flaky = []
    broken = []
    for test, outcome in counts.items():
        failed = outcome["FAILED"]
        if not failed:
            continue
        reported = failed + outcome["ok"]
        if failed == runs:
            broken.append((test, failed, reported))
        else:
            flaky.append((test, failed, reported))
    flaky.sort(key=lambda row: (-row[1] / row[2], row[0]))
    broken.sort()
    return flaky, broken


def plural(count, one, many):
    return f"{count} {one if count == 1 else many}"


def unseen_floor(runs):
    """The failure rate this many runs can account for, as a percentage.

    A test that loses runs at rate p survives N of them with probability (1 - p) ** N. Below the
    rate this returns, that probability is worse than one in twenty, which is where a clean sweep
    stops being evidence. Reporting it is what keeps "every test agreed with itself" from being
    read as "there are no flaky tests here": ten runs cannot say that, and said it anyway.
    """
    return 100 * (1 - 0.05 ** (1 / runs))


def machine():
    """What the machine reports, since a rate without it invites the wrong conclusion.

    The same `--test-threads` on a wider or busier machine is not the same measurement. A hosted
    runner and a container on a developer's laptop can pass the identical flag and put a different
    number of tests in flight, and the rate is a property of that, not only of the test.
    """
    count = os.cpu_count()
    return f"{count} CPUs" if count else "an unknown number of CPUs"


def describe(runs, threads, counts, lost=0):
    """The report, as markdown, because where it is read is a job summary rather than a terminal."""
    parallelism = (
        f"`--test-threads={threads}`" if threads else "cargo's default parallelism"
    )
    flaky, broken = disagreed(counts, runs)
    lines = [
        "## Test determinism",
        "",
        f"{runs} runs of `{' '.join(SUITE)}` against one build, at {parallelism}, on a machine "
        f"reporting {machine()}. {len(counts)} tests reported.",
        "",
        f"A test that loses more than {unseen_floor(runs):.0f}% of its runs was unlikely to get "
        f"through {runs} of them unseen. One that loses fewer than that had better than a one in "
        "twenty chance of passing every run here, so whatever is not named below is bounded rather "
        "than ruled out.",
        "",
    ]
    if lost:
        lines += [
            f"{plural(lost, 'further run', 'further runs')} reported no tests at all and "
            "counted for nothing.",
            "",
        ]

    if not flaky and not broken:
        lines += [f"Every test agreed with itself across all {runs} runs.", ""]

    if flaky:
        headline = plural(
            len(flaky), "test disagreed with itself", "tests disagreed with themselves"
        )
        lines += [
            f"{headline}:",
            "",
            "| Test | Failed | Ran | Rate |",
            "|---|---|---|---|",
        ]
        for test, failed, reported in flaky:
            lines.append(
                f"| `{test}` | {failed} | {reported} | {100 * failed / reported:.0f}% |"
            )
        lines.append("")
        if any(reported < runs for _, _, reported in flaky):
            lines += [
                f"A `Ran` below {runs} means that test did not report in every run: its binary "
                "stopped early and took the tests after it with it.",
                "",
            ]

    if broken:
        lines += [
            f"{plural(len(broken), 'test', 'tests')} failed every run, which is a failure "
            "rather than a flake:",
            "",
        ]
        lines += [f"- `{test}`" for test, _, _ in broken]
        lines.append("")

    return "\n".join(lines)


def fence(text):
    """A code fence long enough to hold text that contains one.

    The failures quoted here are panics from an agent, so their messages carry model output and
    markdown. A sample containing a fence of its own would otherwise close the block early and spill
    the rest of the panic into the page as prose.
    """
    longest = max((len(run) for run in re.findall(r"`+", text)), default=0)
    return "`" * max(3, longest + 1)


def failures(flaky, broken, samples):
    """What each failure said, once per test, so a shared cause is visible as one."""
    lines = []
    for test, failed, reported in flaky + broken:
        if test not in samples:
            continue
        wall = fence(samples[test])
        lines += [
            f"<details><summary>{test} ({failed} of {reported})</summary>",
            "",
            wall,
            samples[test],
            wall,
            "",
            "</details>",
            "",
        ]
    return "\n".join(lines)


def bare(test):
    """The test's own name, without the binary that ran it.

    An issue title is what somebody searches for, and a binary stem is not what they would type. It
    also has to match the title of the flake filed here by hand, which named the test and nothing
    else.
    """
    return test.split("::", 1)[-1]


def run_url():
    """A link back to the job that measured this, when a job is what ran it."""
    server = os.environ.get("GITHUB_SERVER_URL")
    repo = os.environ.get("GITHUB_REPOSITORY")
    attempt = os.environ.get("GITHUB_RUN_ID")
    if not (server and repo and attempt):
        return None
    return f"{server}/{repo}/actions/runs/{attempt}"


def issue_body(test, failed, reported, threads, url, sample):
    """What one flaky test's issue says.

    Enough to recognise the failure and to know it was not a person who decided this was worth
    filing, since nobody has looked at it and the reader should not assume otherwise.
    """
    parallelism = (
        f"`--test-threads={threads}`" if threads else "cargo's default parallelism"
    )
    rate = f"{100 * failed / reported:.0f}%"
    # A doc-test's name is its own file and line, so there is no binary stem to point at.
    binary = test.split("::", 1)[0] if "::" in test else None
    lines = [
        f"`{bare(test)}` failed {failed} of {reported} runs ({rate}) of one build, "
        f"at {parallelism}, on a machine reporting {machine()}.",
        "",
        "Nothing in the tree changed between those runs, so the difference is the test rather than "
        "the code under it." + (f" The binary that ran it is `{binary}`." if binary else ""),
        "",
    ]
    if sample:
        wall = fence(sample)
        lines += [wall, sample, wall, ""]
    # Only the workflow may claim to be the workflow. A rate measured on somebody's laptop and
    # filed under a job's name is a reader trusting an environment that never ran it.
    filer = (
        "Filed by the Test determinism workflow"
        if url
        else "Filed by `contrib/measure-flakes.py`, run by hand rather than by the workflow"
    )
    lines += [
        f"{filer} rather than by a person, so nothing here is triaged. It opens one issue per test "
        "and files nothing for a test that already has one, so this will not arrive again every "
        "week. `contrib/measure-flakes.py --runs 30` asks the same question of a local machine.",
        "",
    ]
    if url:
        lines += [f"The run that measured it: {url}", ""]
    return "\n".join(lines)


def run_gh(arguments, body=None):
    """gh, as an argument list, never a shell string. Its token comes from the environment.

    Returns what it printed on success, and what it complained about otherwise.
    """
    process = subprocess.run(
        ["gh", *arguments],
        input=body.encode("utf-8") if body is not None else None,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    stream = process.stdout if process.returncode == 0 else process.stderr
    return process.returncode, stream.decode("utf-8", "replace").strip()


def dry_gh(arguments, body=None):
    """What a real run would send, so the shape of it can be checked against no repo at all."""
    print(f"gh {' '.join(arguments)}", file=sys.stderr)
    if body is not None:
        print(body, file=sys.stderr)
    if arguments[:2] == ["issue", "list"]:
        return 0, "[]"
    return 0, "nothing, since this was a dry run"


def existing_issues(repo, gh):
    """Every flake issue this repo already has, open or closed, by title. `None` if unanswerable.

    Asked once for all of them rather than once per test, and the answer is matched exactly here
    rather than trusted to a search: a query that came back empty for the wrong reason would file a
    second issue for every test that already had one.
    """
    code, output = gh(
        [
            "issue",
            "list",
            "--repo",
            repo,
            "--search",
            f'in:title "{TITLE}"',
            "--state",
            "all",
            "--limit",
            "200",
            "--json",
            "number,title",
        ]
    )
    if code != 0:
        print(f"the issues on {repo} could not be listed: {output}", file=sys.stderr)
        return None
    try:
        listed = json.loads(output)
    except ValueError:
        print("gh printed something other than the issue list", file=sys.stderr)
        return None
    return {issue["title"]: issue["number"] for issue in listed}


def file_issues(flaky, samples, threads, repo, limit, gh):
    """One issue per flaky test, and nothing at all for a test that already has one.

    Returns the section to append to the report, and whether every write that was meant to happen
    did. A test that already has an issue is named with its number instead: a weekly job that files
    the same thing every Monday is a job whose issues get closed unread.
    """
    heading = ["### Issues", ""]
    existing = existing_issues(repo, gh)
    if existing is None:
        return (
            "\n".join(
                heading
                + [f"The issues on {repo} could not be listed, so nothing was filed.", ""]
            ),
            False,
        )

    url = run_url()
    lines = []
    held = []
    filed = 0
    delivered = True
    for test, failed, reported in flaky:
        title = f"{TITLE}: {bare(test)}"
        if title in existing:
            lines.append(f"- #{existing[title]} already names `{bare(test)}`.")
            continue
        if filed >= limit:
            held.append(test)
            continue
        code, output = gh(
            ["issue", "create", "--repo", repo, "--title", title, "--body-file", "-"],
            issue_body(test, failed, reported, threads, url, samples.get(test)),
        )
        if code != 0:
            delivered = False
            lines.append(f"- `{bare(test)}` could not be filed: {output}")
            print(f"filing for {test} failed: {output}", file=sys.stderr)
            continue
        filed += 1
        lines.append(f"- Filed {(output.splitlines() or ['an issue'])[-1]}.")

    if held:
        lines.append(
            f"- {plural(len(held), 'test is', 'tests are')} named above and unfiled: one run "
            f"files at most {limit}, and the next one files the rest."
        )
    return "\n".join(heading + (lines or ["Nothing to file."]) + [""]), delivered


def cargo_env():
    """Colour off, so the parse reads test names rather than escape sequences."""
    env = dict(os.environ)
    env["CARGO_TERM_COLOR"] = "never"
    return env


def run_suite(command):
    with subprocess.Popen(
        command,
        env=cargo_env(),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    ) as process:
        # Decoded here rather than by Popen so that a test writing bytes that are not text costs a
        # replacement character instead of an hour of measurement.
        return parse(line.decode("utf-8", "replace") for line in process.stdout)


def measure(runs, threads):
    """Build once, run that build `runs` times, and return what each run reported.

    Building first is what separates a broken tree from a failing test: after this, a run that
    reports nothing is a run that went wrong rather than a suite that is clean.
    """
    build = ["cargo", "test", "--all", "--locked", "--no-run"]
    if subprocess.run(build, env=cargo_env()).returncode != 0:
        print("the tests do not build, so nothing here is measurable", file=sys.stderr)
        return None, None, 0

    command = list(SUITE)
    if threads is not None:
        command += ["--", f"--test-threads={threads}"]

    # `--no-run` builds no doc-tests, so without this the first measured run would pay rustdoc's
    # compile while competing with the timing-sensitive tests it is measuring, and its rate would
    # not be comparable with the rest.
    print("warming up, so the doc-tests are built before anything counts", file=sys.stderr)
    if not run_suite(command)[0]:
        print("the warm-up run reported no tests at all", file=sys.stderr)
        return None, None, 0

    per_run = []
    samples = {}
    lost = 0
    for attempt in range(1, runs + 1):
        outcomes, run_samples = run_suite(command)
        # One run that goes wrong is not worth discarding the ones that did not, since the report
        # is the only thing this produces.
        if not outcomes:
            lost += 1
            print(f"run {attempt} of {runs} reported no tests at all", file=sys.stderr)
            continue
        per_run.append(outcomes)
        for test, text in run_samples.items():
            samples.setdefault(test, text)
        failed = sum(1 for outcome in outcomes.values() if outcome == "FAILED")
        print(f"run {attempt} of {runs}: {failed} failed", file=sys.stderr)
    return per_run, samples, lost


def at_least_one(value):
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("has to be at least 1")
    return number


def repetition(value):
    number = int(value)
    # Determinism is a claim about repetition, and one run of anything agrees with itself.
    if number < 2:
        raise argparse.ArgumentTypeError("has to be at least 2 to measure determinism")
    return number


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--runs",
        type=repetition,
        default=20,
        help="how many times to run the suite (default: 20)",
    )
    parser.add_argument(
        "--test-threads",
        type=at_least_one,
        help="what to pass libtest; omitted, cargo picks as it does for `make check`",
    )
    parser.add_argument(
        "--summary",
        help="a file to append the report to, such as $GITHUB_STEP_SUMMARY",
    )
    parser.add_argument(
        "--file-issues",
        metavar="OWNER/REPO",
        # Empty rather than absent, because the workflow matrix passes this per leg and only one leg
        # may file: two legs measuring the same tests would otherwise race to open the same issue.
        help="open an issue for each flaky test that has none; an empty value files nothing",
    )
    parser.add_argument(
        "--issue-limit",
        type=at_least_one,
        default=ISSUE_LIMIT,
        help=f"how many issues one run may open (default: {ISSUE_LIMIT})",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print what would be filed instead of filing it",
    )
    parser.add_argument(
        "--selftest", action="store_true", help="check the parser and report nothing"
    )
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    per_run, samples, lost = measure(args.runs, args.test_threads)
    if not per_run:
        return 1

    counts = tally(per_run)
    flaky, broken = disagreed(counts, len(per_run))
    report = describe(len(per_run), args.test_threads, counts, lost)
    delivered = True
    if args.file_issues:
        section, delivered = file_issues(
            flaky,
            samples,
            args.test_threads,
            args.file_issues,
            args.issue_limit,
            dry_gh if args.dry_run else run_gh,
        )
        report = f"{report}\n{section}"
    detail = failures(flaky, broken, samples)
    if detail:
        report = f"{report}\n{detail}"

    print(report)
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as handle:
            handle.write(f"{report}\n")
    # A rate never fails this. An issue that was meant to be filed and was not does, because the
    # report is the whole product and a filer that quietly stopped working looks exactly like a
    # suite that stopped being flaky.
    return 0 if delivered else 1


# One binary that loses a test to a race and one that fails the same test every time, in the shape
# cargo prints them: the announcement on stderr, the outcomes on stdout, and the captured output
# between the two summaries. The verbose failure prints more than a sample holds and then a
# heading of its own, which is how a test says something the parse could mistake for libtest's.
FIXTURE_FLAKED = """\
     Running unittests src/lib.rs (target/debug/deps/bravebot_agent-99a962bf03b1)

running 2 tests
test turns::a_turn_that_stops_reports_what_it_printed ... ok
test turns::a_reply_arrives_before_the_deadline ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s

     Running tests/exec.rs (target/debug/deps/exec-de4c3831203a)

running 3 tests
test a_reply_arrives_before_the_deadline ... FAILED
test a_pipeline_is_stopped_at_the_limit ... FAILED
test a_frame_is_drawn_once_per_reply ... FAILED

failures:

---- a_reply_arrives_before_the_deadline stdout ----

thread 'a_reply_arrives_before_the_deadline' (14033052) panicked at crates/agent/tests/exec.rs:325:6:
NotStarted { program: "serve", detail: "Text file busy (os error 26)" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- a_pipeline_is_stopped_at_the_limit stdout ----

thread 'a_pipeline_is_stopped_at_the_limit' (14033053) panicked at crates/agent/tests/exec.rs:410:6:
the limit was never reached

---- a_frame_is_drawn_once_per_reply stdout ----
frame 1
frame 2
frame 3
frame 4
frame 5
frame 6
frame 7
frame 8
frame 9
frame 10
frame 11
frame 12
frame 13
---- what the frame held stdout ----

thread 'a_frame_is_drawn_once_per_reply' (14033054) panicked at crates/agent/tests/exec.rs:88:6:
the reply was drawn twice
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

failures:
    a_reply_arrives_before_the_deadline
    a_pipeline_is_stopped_at_the_limit
    a_frame_is_drawn_once_per_reply

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.04s

   Doc-tests bravebot_agent

running 1 test
test crates/agent/src/lib.rs - (line 7) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
"""

FIXTURE_PASSED = """\
     Running unittests src/lib.rs (target/debug/deps/bravebot_agent-99a962bf03b1)

running 2 tests
test turns::a_turn_that_stops_reports_what_it_printed ... ok
test turns::a_reply_arrives_before_the_deadline ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s

     Running tests/exec.rs (target/debug/deps/exec-de4c3831203a)

running 3 tests
test a_reply_arrives_before_the_deadline ... ok
test a_frame_is_drawn_once_per_reply ... ok
test a_pipeline_is_stopped_at_the_limit ... FAILED

failures:

---- a_pipeline_is_stopped_at_the_limit stdout ----

thread 'a_pipeline_is_stopped_at_the_limit' (14033061) panicked at crates/agent/tests/exec.rs:410:6:
the limit was never reached

failures:
    a_pipeline_is_stopped_at_the_limit

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.98s

   Doc-tests bravebot_agent

running 1 test
test crates/agent/src/lib.rs - (line 7) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
"""


def recording_gh(existing=(), listable=True):
    """A stand-in for gh that answers with a known set of issues and keeps what it was asked.

    Filing is the one part of this that writes to something shared, so it is checked against a
    recording rather than a repository.
    """
    calls = []

    def gh(arguments, body=None):
        calls.append((arguments, body))
        if arguments[:2] == ["issue", "list"]:
            if not listable:
                return 1, "a refusal the selftest asked for, not a real one"
            listed = [{"number": number, "title": title} for number, title in existing]
            return 0, json.dumps(listed)
        return 0, "https://github.com/owner/repo/issues/900"

    return gh, calls


def created(calls):
    """The title and body of each issue a recording was asked to open."""
    return [
        (arguments[arguments.index("--title") + 1], body)
        for arguments, body in calls
        if arguments[:2] == ["issue", "create"]
    ]


def selftest():
    """Prove the parse and the report say the thing the job exists to say.

    A parser that quietly matches nothing produces a clean report forever, which is the state this
    replaces rather than an improvement on it. Every claim the report makes is checked here against
    output whose answer is known, since the alternative is learning it was wrong from a month of
    green summaries.
    """
    outcomes, samples = parse(FIXTURE_FLAKED.splitlines())
    second, _ = parse(FIXTURE_PASSED.splitlines())
    counts = tally([outcomes, second])
    flaky, broken = disagreed(counts, 2)

    checks = [
        # An exact set, because a phantom entry read out of a summary line is as wrong as a missing
        # one and neither shows up as a failure anywhere else.
        (
            "the tests are exactly the ones the output named",
            set(outcomes)
            == {
                "bravebot_agent::turns::a_turn_that_stops_reports_what_it_printed",
                "bravebot_agent::turns::a_reply_arrives_before_the_deadline",
                "exec::a_reply_arrives_before_the_deadline",
                "exec::a_pipeline_is_stopped_at_the_limit",
                "exec::a_frame_is_drawn_once_per_reply",
                "crates/agent/src/lib.rs - (line 7)",
            },
        ),
        # The same test name lives in two binaries here, and one of them is the flake. Merging them
        # would report the wrong test and hide the right one.
        (
            "a name is kept apart from the same name in another binary",
            outcomes.get("exec::a_reply_arrives_before_the_deadline") == "FAILED"
            and outcomes.get(
                "bravebot_agent::turns::a_reply_arrives_before_the_deadline"
            )
            == "ok",
        ),
        (
            "a doc-test is named by its own file and line",
            outcomes.get("crates/agent/src/lib.rs - (line 7)") == "ok",
        ),
        (
            "the tests that both passed and failed are the ones reported",
            flaky
            == [
                ("exec::a_frame_is_drawn_once_per_reply", 1, 2),
                ("exec::a_reply_arrives_before_the_deadline", 1, 2),
            ],
        ),
        (
            "a test that failed every run is not called a flake",
            broken == [("exec::a_pipeline_is_stopped_at_the_limit", 2, 2)],
        ),
        (
            "the failure is captured and stops before the next one",
            "Text file busy"
            in samples.get("exec::a_reply_arrives_before_the_deadline", "")
            and "the limit was never reached"
            not in samples.get("exec::a_reply_arrives_before_the_deadline", ""),
        ),
        # A test that prints more than the sample holds is the likeliest thing here to be a flake,
        # and keeping the top of its output would report everything about it except why it failed.
        (
            "a failure under a wall of output is reported as the failure",
            "the reply was drawn twice"
            in samples.get("exec::a_frame_is_drawn_once_per_reply", ""),
        ),
        # The wall of output above ends with a line shaped exactly like libtest's heading.
        (
            "text that looks like a heading invents no test",
            not any("what the frame held" in test for test in samples),
        ),
        (
            "the report states the rate and the parallelism it was measured at",
            "50%" in describe(2, 4, counts) and "--test-threads=4" in describe(2, 4, counts),
        ),
        (
            "a run that reported nothing is counted as nothing rather than hidden",
            "further run reported no tests" in describe(2, None, counts, lost=1),
        ),
    ]

    steady = {"exec::a_pipeline_is_stopped_at_the_limit": "ok"}
    clean = describe(2, None, tally([steady, steady]))
    checks.append(
        (
            "a suite that agreed with itself says so, with no table",
            "agreed with itself" in clean and "| Test |" not in clean,
        )
    )
    # The sentence above is the one somebody reads and stops reading, so it has to be false the
    # moment anything failed, flake or not.
    checks.append(
        (
            "a suite with a hard failure never claims every test agreed",
            "agreed with itself" not in describe(2, None, counts),
        )
    )
    # That sentence read as proof once, which is how a ten run sweep came to be treated as evidence
    # that six tests failing one run in ten were not there. It arrives with its own bound now.
    checks.append(
        (
            "a clean report states the rate it could have seen rather than implying none exists",
            "bounded rather than ruled out" in clean
            and f"{unseen_floor(2):.0f}%" in clean,
        )
    )
    checks.append(
        (
            "more runs claim a tighter bound than fewer",
            unseen_floor(30) < unseen_floor(10) < unseen_floor(2),
        )
    )

    # Filing is the half that writes to a repository everybody shares, and the failure that costs
    # most is a second issue for a test that already has one.
    gh, calls = recording_gh()
    section, delivered = file_issues(flaky, samples, 16, "owner/repo", ISSUE_LIMIT, gh)
    opened = created(calls)
    checks.append(
        (
            "one issue is filed per flake, and none for a test that failed every run",
            delivered
            and [title for title, _ in opened]
            == [
                "Flaky test: a_frame_is_drawn_once_per_reply",
                "Flaky test: a_reply_arrives_before_the_deadline",
            ],
        )
    )
    checks.append(
        (
            "the issue says the rate, the parallelism and what the failure was",
            all(
                "50%" in body and "--test-threads=16" in body for _, body in opened
            )
            and any("the reply was drawn twice" in body for _, body in opened),
        )
    )
    # The same flag on a wider or busier machine is a different measurement, and a rate quoted
    # without one invites the reader to assume it was theirs.
    checks.append(
        (
            "a rate arrives with the machine that produced it",
            machine() in clean and all(machine() in body for _, body in opened),
        )
    )

    covered, calls = recording_gh(
        existing=[(76, "Flaky test: a_reply_arrives_before_the_deadline")]
    )
    section, _ = file_issues(flaky, samples, 16, "owner/repo", ISSUE_LIMIT, covered)
    checks.append(
        (
            "a test that already has an issue is named rather than filed again",
            [title for title, _ in created(calls)]
            == ["Flaky test: a_frame_is_drawn_once_per_reply"]
            and "#76" in section,
        )
    )

    # Filing blind is how one test ends up with five issues, so an unanswerable question files
    # nothing and says the report did not get out.
    blind, calls = recording_gh(listable=False)
    section, delivered = file_issues(flaky, samples, 16, "owner/repo", ISSUE_LIMIT, blind)
    checks.append(
        (
            "a listing that failed files nothing and does not pass silently",
            not created(calls) and not delivered and "could not be listed" in section,
        )
    )

    capped, calls = recording_gh()
    section, _ = file_issues(flaky, samples, 16, "owner/repo", 1, capped)
    checks.append(
        (
            "one run files no more issues than its limit, worst rate first",
            [title for title, _ in created(calls)]
            == ["Flaky test: a_frame_is_drawn_once_per_reply"]
            and "unfiled" in section,
        )
    )

    # A panic here quotes model output, so a fence inside one would end the block early and leave
    # the rest of the failure to be read as prose.
    quoted = issue_body(
        "exec::a_frame_is_drawn_once_per_reply",
        1,
        2,
        16,
        None,
        "assertion failed\n```\nnot the end\n```\nthe panic",
    )
    checks.append(
        (
            "a failure containing a code fence is still quoted whole",
            "the panic" in quoted and quoted.count("\n````\n") == 2,
        )
    )

    # Crediting the workflow for a laptop's measurement asks the reader to trust an environment that
    # never ran the test, and the rate is the part they would trust.
    by_hand = issue_body("exec::a_frame_is_drawn_once_per_reply", 1, 2, 4, None, "")
    by_job = issue_body(
        "exec::a_frame_is_drawn_once_per_reply", 1, 2, 4, "https://example/runs/1", ""
    )
    checks.append(
        (
            "only a run that was a job claims the workflow filed it",
            "Test determinism workflow" in by_job
            and "Test determinism workflow" not in by_hand
            and "run by hand" in by_hand,
        )
    )

    broke = [claim for claim, held in checks if not held]
    for claim in broke:
        print(f"selftest: {claim}", file=sys.stderr)
    if broke:
        print(f"{len(broke)} of {len(checks)} checks failed", file=sys.stderr)
        return 1
    print(f"selftest: {len(checks)} checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
