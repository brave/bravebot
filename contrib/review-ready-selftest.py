#!/usr/bin/env python3
"""What review-ready.py lists and how long it waits, against a stand-in for `gh`.

The end-to-end tests run the script as `make review-ready` does, with a `gh` first on PATH. That `gh`
serves a scripted sequence of reads of `gh pr list` (the last one repeats) and logs its arguments, so
a test reads the number of reads from the log. The timing tests call `watch` with a clock and a sleep
of their own, so they wait for nothing.
"""

import importlib.util
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
# The script and make are run with argument lists and no shell.
import subprocess  # nosemgrep: gitlab.bandit.B404

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "review-ready.py"
REVIEWER = "netzenbot-reviewer"

FAKE_GH = """#!{python}
import json, os, sys
log = os.environ["FAKE_LOG"]
with open(log, "a") as handle:
    handle.write(json.dumps(sys.argv[1:]) + "\\n")
reads = json.load(open(os.environ["FAKE_READS"]))
with open(log) as handle:
    calls = sum(1 for _ in handle)
read = reads[min(calls, len(reads)) - 1]
if isinstance(read, dict):
    sys.stderr.write(read.get("stderr", ""))
    if "raw" in read:
        sys.stdout.write(read["raw"])
    sys.exit(read.get("exit", 0))
json.dump(read, sys.stdout)
"""


def load_script():
    spec = importlib.util.spec_from_file_location("review_ready", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


review_ready = load_script()


def pr(number, mergeable="MERGEABLE", draft=False, reviews=None, checks=None, created="2026-10-01T00:00:00Z",
       approved=None):
    """A pull request the reviewer can pick up unless an argument says otherwise. `created` is when it was
    opened and `approved` when the reviewer approved it, which is left out of the review when None."""
    if reviews is None:
        review = {"author": {"login": REVIEWER}, "state": "APPROVED"}
        if approved:
            review["submittedAt"] = approved
        reviews = [review]
    if checks is None:
        checks = [{"conclusion": "SUCCESS"}]
    return {
        "number": number,
        "title": f"title {number}",
        "url": f"https://github.com/brave/bravebot/pull/{number}",
        "createdAt": created,
        "mergeable": mergeable,
        "isDraft": draft,
        "latestReviews": reviews,
        "statusCheckRollup": checks,
    }


def line(number):
    return f"https://github.com/brave/bravebot/pull/{number}  title {number}"


def run(conclusion, started, name="Process narration", workflow="Narration"):
    """One run of a check on the head commit. GitHub lists every run, not only the latest, and
    `started` is the minute and second the run began on 2026-11-01 at 14:00 UTC."""
    return {"__typename": "CheckRun", "workflowName": workflow, "name": name, "conclusion": conclusion,
            "startedAt": f"2026-11-01T14:{started}Z"}


def status(state, started, context="ci/build"):
    """One commit status, which has a context and a state where a check run has a name and a conclusion."""
    return {"__typename": "StatusContext", "context": context, "state": state,
            "startedAt": f"2026-11-01T14:{started}Z"}


class Result:
    def __init__(self, done, log_path):
        self.code, self.out, self.err = done.returncode, done.stdout, done.stderr
        self.calls = [json.loads(text) for text in log_path.read_text().splitlines()] if log_path.exists() else []


class ReviewReady(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root, True)
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        (bin_dir / "gh").write_text(FAKE_GH.format(python=sys.executable))
        (bin_dir / "gh").chmod(0o755)
        self.log = self.root / "log"
        self.reads_path = self.root / "reads"
        self.env = dict(os.environ, PATH=f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
                        FAKE_LOG=str(self.log), FAKE_READS=str(self.reads_path))

    def serve(self, *reads):
        self.reads_path.write_text(json.dumps(list(reads)))

    def run_command(self, command, cwd=None):
        done = subprocess.run(command, cwd=cwd, env=self.env, capture_output=True, text=True, timeout=60)
        return Result(done, self.log)

    def run_script(self, *args, wait="30", interval="0.01"):
        return self.run_command([sys.executable, str(SCRIPT), "--wait", wait, "--interval", interval, *args])

    def test_a_pull_request_that_resolves_on_the_third_read_is_printed_once_and_the_wait_ends_there(self):
        """A person who runs the target straight after main moves sees the pull request on the read
        GitHub finishes computing it, and the run does not go on to the end of the wait."""
        self.serve([pr(7, "UNKNOWN")], [pr(7, "UNKNOWN")], [pr(7)], [pr(7)])
        result = self.run_script()
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(result.out.splitlines(), [line(7)])
        self.assertEqual(len(result.calls), 3)
        self.assertEqual(result.err.count("waiting for"), 2)

    def test_stdout_holds_only_url_and_title_lines(self):
        """The target's output is piped, so the waiting and incomplete messages stay on stderr."""
        self.serve([pr(3), pr(4, "UNKNOWN")], [pr(3), pr(4, "UNKNOWN")])
        result = self.run_script(wait="0.3")
        self.assertEqual(result.out, line(3) + "\n")
        self.assertIn("incomplete", result.err)

    def test_each_ready_pull_request_is_printed_once_across_reads(self):
        """A pull request seen mergeable on one read is still on the next, and is not listed twice."""
        self.serve([pr(1), pr(2, "UNKNOWN")], [pr(1), pr(2, "UNKNOWN")], [pr(1), pr(2)])
        result = self.run_script()
        self.assertEqual(result.out.splitlines(), [line(1), line(2)])

    def test_a_ready_pull_request_is_printed_after_the_last_read_in_the_order_of_waiting(self):
        """A pull request seen on a later read can have waited longer than one seen earlier, so nothing is
        printed until the reads end, and then the list is in the order of waiting."""
        printed = []

        class Out:
            def __init__(self, into):
                self.into = into

            def write(self, text):
                self.into.append(text)

            def flush(self):
                pass

        seen_at_second_read = []
        reads = iter([[pr(1, created="2026-10-05T00:00:00Z"), pr(2, "UNKNOWN")],
                      [pr(1, created="2026-10-05T00:00:00Z"), pr(2, created="2026-10-02T00:00:00Z")]])

        def fetch():
            seen_at_second_read.append("".join(printed))
            return next(reads)

        review_ready.watch(fetch, REVIEWER, 30, 0, Out(printed), Out([]), sleep=lambda seconds: None)
        self.assertEqual(seen_at_second_read[1], "")
        self.assertEqual("".join(printed), line(2) + "\n" + line(1) + "\n")

    def test_the_list_runs_from_the_longest_waiting_to_the_most_recent(self):
        """A pull request waits from the reviewer's approval, so one opened early but approved late is
        after one opened later and approved earlier. Without an approval time it waits from when it was
        opened, and a tie goes by number."""
        self.serve([
            pr(1, created="2026-10-01T00:00:00Z", approved="2026-10-09T00:00:00Z"),
            pr(2, created="2026-10-08T00:00:00Z", approved="2026-10-03T00:00:00Z"),
            pr(3, created="2026-10-04T00:00:00Z"),
            pr(5, created="2026-10-10T00:00:00Z"),
            pr(4, created="2026-10-10T00:00:00Z"),
        ])
        self.assertEqual(self.run_script().out.splitlines(), [line(2), line(3), line(1), line(4), line(5)])

    def test_the_reviewers_latest_approval_is_when_the_wait_began(self):
        """An approval by somebody else does not start the wait."""
        other = {"author": {"login": "other"}, "state": "APPROVED", "submittedAt": "2026-10-20T00:00:00Z"}
        mine = {"author": {"login": REVIEWER}, "state": "APPROVED", "submittedAt": "2026-10-06T00:00:00Z"}
        self.serve([pr(1, reviews=[other, mine]), pr(2, created="2026-10-07T00:00:00Z")])
        self.assertEqual(self.run_script().out.splitlines(), [line(1), line(2)])

    def test_one_that_stays_unknown_for_the_whole_wait_is_named_as_missing_from_the_list(self):
        """The list is incomplete, and the person is told which pull requests it leaves out."""
        self.serve([pr(5), pr(8, "UNKNOWN"), pr(9, "UNKNOWN")])
        result = self.run_script(wait="0.3")
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(result.out.splitlines(), [line(5)])
        last = result.err.splitlines()[-1]
        self.assertIn("incomplete", last)
        self.assertIn("#8, #9", last)
        self.assertNotIn("#5", last)

    def test_a_wait_of_zero_reads_once_and_still_names_the_unknown_ones(self):
        """The wait is the only thing that makes the run read again."""
        self.serve([pr(8, "UNKNOWN")])
        result = self.run_script(wait="0")
        self.assertEqual(len(result.calls), 1)
        self.assertIn("incomplete", result.err)
        self.assertIn("#8", result.err)

    def test_a_conflicting_pull_request_causes_no_wait(self):
        """GitHub has computed it: a conflict does not clear by waiting."""
        self.serve([pr(6, "CONFLICTING")])
        result = self.run_script()
        self.assertEqual((result.code, result.out, result.err), (0, "", ""))
        self.assertEqual(len(result.calls), 1)

    def test_unknown_pull_requests_nobody_could_pick_up_cause_no_wait(self):
        """Waiting only helps a pull request that would be listed once GitHub answers."""
        approved = [{"author": {"login": REVIEWER}, "state": "APPROVED"}]
        cases = {
            "draft": pr(1, "UNKNOWN", draft=True),
            "approved by somebody else": pr(1, "UNKNOWN", reviews=[{"author": {"login": "other"}, "state": "APPROVED"}]),
            "not reviewed": pr(1, "UNKNOWN", reviews=[]),
            "changes requested by the reviewer": pr(1, "UNKNOWN", reviews=[{"author": {"login": REVIEWER}, "state": "CHANGES_REQUESTED"}]),
            "changes requested by another": pr(1, "UNKNOWN", reviews=approved + [{"author": {"login": "other"}, "state": "CHANGES_REQUESTED"}]),
            "failing check": pr(1, "UNKNOWN", checks=[{"conclusion": "SUCCESS"}, {"conclusion": "FAILURE"}]),
            "failing status": pr(1, "UNKNOWN", checks=[{"conclusion": None, "state": "FAILURE"}]),
            "check still running": pr(1, "UNKNOWN", checks=[{"conclusion": "", "status": "IN_PROGRESS"}]),
        }
        for name, case in cases.items():
            with self.subTest(name):
                self.log.unlink(missing_ok=True)
                self.serve([case])
                result = self.run_script()
                self.assertEqual((result.code, result.out, result.err), (0, "", ""))
                self.assertEqual(len(result.calls), 1)

    def test_a_pull_request_that_turns_unlistable_while_pending_stops_the_wait(self):
        """Every field is read again, so a pending pull request that becomes a draft, fails a check or
        conflicts is no longer waited for."""
        for name, later in (("conflicting", pr(2, "CONFLICTING")), ("draft", pr(2, "UNKNOWN", draft=True)),
                            ("failing", pr(2, "UNKNOWN", checks=[{"conclusion": "FAILURE"}]))):
            with self.subTest(name):
                self.log.unlink(missing_ok=True)
                self.serve([pr(2, "UNKNOWN")], [later])
                result = self.run_script()
                self.assertEqual(len(result.calls), 2)
                self.assertEqual(result.out, "")
                self.assertEqual(result.err.count("waiting for"), 1)
                self.assertNotIn("incomplete", result.err)

    def test_the_waiting_line_counts_and_names_the_pending_ones_and_follows_each_read(self):
        """The count and the numbers shrink as pull requests resolve."""
        self.serve([pr(3, "UNKNOWN"), pr(4, "UNKNOWN"), pr(5, "UNKNOWN")],
                   [pr(3), pr(4), pr(5, "UNKNOWN")], [pr(3), pr(4), pr(5)])
        result = self.run_script()
        self.assertEqual(result.err.splitlines(), [
            "waiting for 3 pull requests GitHub has not computed as mergeable (UNKNOWN): #3, #4, #5",
            "waiting for 1 pull request GitHub has not computed as mergeable (UNKNOWN): #5",
        ])
        self.assertEqual(result.out.splitlines(), [line(3), line(4), line(5)])

    def test_the_reads_use_the_repository_reviewer_and_limit_given(self):
        """The reviewer decides who counts as approving, and the repository and limit reach gh."""
        other = [{"author": {"login": "someone"}, "state": "APPROVED"}]
        self.serve([pr(1, reviews=other)])
        listed = self.run_script("--reviewer", "someone", "--repo", "o/r", "--limit", "7")
        self.assertEqual(listed.out, line(1) + "\n")
        self.assertEqual(listed.calls[0][:7], ["pr", "list", "--repo", "o/r", "--limit", "7", "--json"])
        self.log.unlink()
        self.assertEqual(self.run_script().out, "")

    def test_a_null_conclusion_falls_back_to_the_status_state(self):
        """A commit status has a state and no conclusion, and skipped and neutral checks pass."""
        self.serve([pr(1, checks=[{"conclusion": None, "state": "SUCCESS"}, {"conclusion": "SKIPPED"},
                                  {"conclusion": "NEUTRAL"}]), pr(2, checks=[])])
        self.assertEqual(self.run_script().out.splitlines(), [line(1), line(2)])

    def test_a_run_that_a_later_run_of_the_same_check_replaced_does_not_count(self):
        """A new run of a workflow cancels the one still going, and GitHub keeps both on the commit.
        The pull request page shows the later run, so the pull request is listed when that one passed."""
        passed = run("SUCCESS", "04:15")
        cases = {
            "cancelled, then passed": [run("CANCELLED", "04:00"), passed],
            "passed, listed before the cancelled run it replaced": [passed, run("CANCELLED", "04:00")],
            "failed, then passed on a re-run": [run("FAILURE", "04:00"), passed],
            "status failed, then passed": [status("FAILURE", "04:00"), status("SUCCESS", "04:15")],
        }
        for name, checks in cases.items():
            with self.subTest(name):
                self.serve([pr(1, checks=checks + [run("SUCCESS", "04:05", "Specs", "CI")])])
                self.assertEqual(self.run_script(wait="0").out, line(1) + "\n")

    def test_only_the_latest_run_of_the_same_check_is_looked_past_and_no_other_check_is_merged_into_it(self):
        """The newest run decides, and a different check, another workflow's check of the same name, or
        a run that cannot be ordered never stands in for it."""
        cases = {
            "passed, then failed on a re-run": [run("SUCCESS", "04:00"), run("FAILURE", "04:15")],
            "passed, then running again": [run("SUCCESS", "04:00"), {**run("", "04:15"), "status": "IN_PROGRESS"}],
            "passed, then queued with no start time": [
                run("SUCCESS", "04:00"), {**run("", "04:15"), "status": "QUEUED", "startedAt": "0001-01-01T00:00:00Z"}],
            "status passed, then failed": [status("SUCCESS", "04:00"), status("FAILURE", "04:15")],
            "cancelled and never replaced": [run("CANCELLED", "04:00")],
            "failed, then another check passed": [run("FAILURE", "04:00", "Specs", "CI"), run("SUCCESS", "04:15", "Locales", "CI")],
            "failed, then the same name passed in another workflow": [
                run("FAILURE", "04:00", "Affected checks", "CI"), run("SUCCESS", "04:15", "Affected checks", "Dependencies")],
            "failed and passed at the same time": [run("FAILURE", "04:00"), run("SUCCESS", "04:00")],
            "failed and passed with no start time": [
                {k: v for k, v in run(result, "04:00").items() if k != "startedAt"} for result in ("FAILURE", "SUCCESS")],
        }
        for name, checks in cases.items():
            with self.subTest(name):
                self.log.unlink(missing_ok=True)
                self.serve([pr(1, checks=checks)])
                result = self.run_script()
                self.assertEqual((result.code, result.out, result.err), (0, "", ""))
                self.assertEqual(len(result.calls), 1)

    def test_a_failure_of_gh_exits_non_zero_on_the_first_read_and_on_a_later_one(self):
        """A failed read must not look like a list with nothing ready."""
        cases = {
            "first": [{"exit": 4, "stderr": "boom"}],
            "later": [[pr(1, "UNKNOWN")], {"exit": 4, "stderr": "boom"}],
            "not JSON": [{"raw": "not json"}],
            "not a list": [{"raw": "{}"}],
        }
        for name, reads in cases.items():
            with self.subTest(name):
                self.log.unlink(missing_ok=True)
                self.serve(*reads)
                result = self.run_script()
                self.assertEqual(result.code, 1, result.err)
                self.assertEqual(result.out, "")
                self.assertIn("review-ready:", result.err)
                if name in ("first", "later"):
                    self.assertIn("boom", result.err)

    def test_the_wait_is_spent_in_full_and_no_further_when_a_pull_request_stays_unknown(self):
        """Thirty seconds in total by default, in steps of the interval, and no read after that."""
        for wait, interval, reads in ((30, 3, 11), (10, 3, 5), (0, 3, 1)):
            with self.subTest(wait=wait):
                clock, slept, calls = [0.0], [], []

                def sleep(seconds):
                    slept.append(seconds)
                    clock[0] += seconds
                    self.assertLess(len(slept), 100)

                def fetch():
                    calls.append(clock[0])
                    return [pr(1, "UNKNOWN")]

                class Sink:
                    def write(self, text):
                        pass

                    def flush(self):
                        pass

                review_ready.watch(fetch, REVIEWER, wait, interval, Sink(), Sink(), clock=lambda: clock[0], sleep=sleep)
                self.assertEqual(len(calls), reads)
                self.assertEqual(sum(slept), wait)

    def test_the_defaults_wait_thirty_seconds_between_reads_of_three(self):
        """The constants are what the issue asks for, and the target does not override them."""
        self.assertEqual((review_ready.WAIT, review_ready.INTERVAL), (30.0, 3.0))

    def test_the_make_target_passes_the_repository_reviewer_limit_and_wait_through(self):
        """The recipe's variables reach the script, and without them the defaults do."""
        tree = self.root / "tree"
        (tree / "contrib").mkdir(parents=True)
        shutil.copy(HERE.parent / "Makefile", tree / "Makefile")
        shutil.copy(SCRIPT, tree / "contrib/review-ready.py")
        self.serve([pr(1, reviews=[{"author": {"login": "x"}, "state": "APPROVED"}]), pr(2, "UNKNOWN")])
        result = self.run_command(["make", "--no-print-directory", "review-ready", "REPO=o/r",
                                   "REVIEWER=x", "LIMIT=7", "WAIT=0"], tree)
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(result.out, line(1) + "\n")
        self.assertEqual(result.calls[0][:6], ["pr", "list", "--repo", "o/r", "--limit", "7"])
        self.assertEqual(len(result.calls), 1)
        shown = self.run_command(["make", "--no-print-directory", "-n", "review-ready"], tree)
        self.assertIn("--repo 'brave/bravebot' --reviewer 'netzenbot-reviewer'", shown.out)
        self.assertIn("--limit '500' --wait '30'", shown.out)


if __name__ == "__main__":
    unittest.main()
