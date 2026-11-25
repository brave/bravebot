#!/usr/bin/env python3
"""The decisions the profile skill makes without a model: how each lane reads what a tool printed,
what counts as a candidate, and how a measurement is carried or reported as a regression.

Each test says which wrong implementation it rejects.
"""

import contextlib
import importlib.util
import io
import json
import os
import signal
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import lanes  # noqa: E402
import ledger  # noqa: E402
import measure  # noqa: E402

spec = importlib.util.spec_from_file_location("profiling", HERE / "profiling.py")
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)

os.environ.update(
    GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1", GIT_AUTHOR_NAME="selftest",
    GIT_AUTHOR_EMAIL="selftest@example.invalid", GIT_COMMITTER_NAME="selftest",
    GIT_COMMITTER_EMAIL="selftest@example.invalid",
)

JUNIT = textwrap.dedent("""\
    <testsuites>
      <testsuite name="bravebot-agent::turn" tests="3" time="9.0">
        <testcase name="slow_one" classname="bravebot-agent::turn" time="40.0"/>
        <testcase name="quick_a" classname="bravebot-agent::turn" time="0.5"/>
        <testcase name="quick_b" classname="bravebot-agent::turn" time="0.5"/>
      </testsuite>
      <testsuite name="bravebot-core" tests="2" time="500.0">
        <testcase name="many_a" classname="bravebot-core" time="70.0"/>
        <testcase name="many_b" classname="bravebot-core" time="70.0"/>
      </testsuite>
    </testsuites>
""")


def quiet(function, *args):
    with contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()) as err:
        code = function(*args)
    return code, out.getvalue(), err.getvalue()


def keyed(rows):
    return {r["key"]: r for r in rows}


class Tests(unittest.TestCase):
    def test_a_test_time_is_the_testcase_time_and_not_the_time_of_the_suite_around_it(self):
        """An implementation reading the suite's `time` attribute reports 9.0 and 500.0 for what the
        cases say took 41.0 and 140.0, and names no test."""
        cases = lanes.parse_junit(JUNIT)
        self.assertEqual(
            sorted(cases),
            [("bravebot-agent::turn", "quick_a", 0.5), ("bravebot-agent::turn", "quick_b", 0.5),
             ("bravebot-agent::turn", "slow_one", 40.0), ("bravebot-core", "many_a", 70.0),
             ("bravebot-core", "many_b", 70.0)],
        )

    def test_a_binary_of_many_quick_enough_tests_is_found_by_adding_them_up(self):
        """An implementation listing only the slowest single tests passes `bravebot-core`, whose
        tests are each over the test threshold but whose sum is what makes the binary slow, and
        misses a binary of many tests each under the threshold."""
        many = [("bravebot-many", f"t{i}", 10.0) for i in range(20)]
        rows = keyed(lanes.test_rows(many))
        self.assertIn("tests:bravebot-many", rows)
        self.assertEqual(rows["tests:bravebot-many"]["value"], 200.0)
        self.assertIsNotNone(lanes.judge(rows["tests:bravebot-many"], None))
        self.assertIsNone(lanes.judge(rows["tests:bravebot-many::t0"], None))

    def test_the_report_time_fallback_reads_each_binary_and_leaves_ok_lines_without_a_time_alone(self):
        """An implementation that attributes every test to the last `Running` line, or that fails on a
        line with no `<time>`, mixes the binaries up or loses the cases after an ignored test."""
        output = textwrap.dedent("""\
            Running unittests src/lib.rs (target/debug/deps/bravebot_core-0123456789abcdef)
            test a::first ... ok <1.250s>
            test a::skipped ... ignored
            Running tests/turn.rs (target/debug/deps/turn-fedcba9876543210)
            test second ... FAILED <2.500s>
            Doc-tests bravebot-core
            test doc ... ok <0.100s>
        """)
        self.assertEqual(
            lanes.parse_report_time(output),
            [("bravebot_core", "a::first", 1.25), ("turn", "second", 2.5), ("bravebot-core doc-tests", "doc", 0.1)],
        )

    def test_a_missing_time_attribute_drops_the_case_without_stopping_the_parse(self):
        """An implementation indexing `time` directly raises on one malformed case and reports nothing."""
        text = '<testcase name="a" classname="b"/><testcase name="c" classname="d" time="3.0"/>'
        self.assertEqual(lanes.parse_junit(text), [("d", "c", 3.0)])


class Ranking(unittest.TestCase):
    def row(self, value, threshold=10.0, key="k", **more):
        return lanes.row("tests", key, "a thing", value, "seconds", threshold, **more)

    def test_a_value_at_the_threshold_is_a_candidate_and_one_under_it_is_not(self):
        """An implementation comparing with `>` leaves out a measurement exactly at its threshold,
        and one comparing with `>=` against half of it reports ordinary tests."""
        self.assertIsNotNone(lanes.judge(self.row(10.0), None))
        self.assertIsNone(lanes.judge(self.row(9.99), None))

    def test_candidates_are_ranked_by_how_far_over_the_threshold_and_not_by_the_raw_number(self):
        """An implementation sorting by the value puts a 100 MiB-over binary's bytes above every time,
        and a test 3 times over its limit below a script 1.1 times over."""
        small_unit = lanes.judge(lanes.row("size", "s", "bin", 150 * 1024 * 1024.0, "bytes", float(lanes.BINARY_BYTES)), None)
        slow_test = lanes.judge(self.row(30.0), None)
        self.assertGreater(slow_test["rank"], small_unit["rank"])

    def test_a_regression_needs_growth_past_the_margin_and_a_meaningful_size(self):
        """An implementation reporting any increase flags 1.00 to 1.01; one with no floor flags 1 ms
        becoming 2 ms; one reporting only what is over the threshold misses 5 s becoming 8 s."""
        under = dict(threshold=10.0)
        self.assertIsNone(lanes.judge(self.row(5.1, **under), 5.0))
        self.assertIsNone(lanes.judge(self.row(0.002, **under), 0.001))
        grown = lanes.judge(self.row(8.0, **under), 5.0)
        self.assertTrue(grown["regressed"])
        self.assertIn("1.60 times the 5.00s the ledger recorded", grown["detail"])

    def test_a_size_regression_uses_the_smaller_margin_of_sizes(self):
        """An implementation using the time margin for a binary misses a 15 percent growth that the
        size margin reports."""
        entry = lanes.size_rows({"bravebot": 60 * 1024 * 1024}, {})[0]
        before = 50 * 1024 * 1024.0
        self.assertTrue(lanes.judge(entry, before)["regressed"])
        time_entry = dict(entry, growth=lanes.REGRESSION)
        self.assertIsNone(lanes.judge(time_entry, before))


class Startup(unittest.TestCase):
    def test_a_start_is_the_median_of_its_runs_with_the_worst_kept_in_the_detail(self):
        """An implementation using the mean reports 2.2 for runs of 1, 1, 1, 8; one using the worst
        reports 8; both flag a start the median says is fine."""
        entry = lanes.startup_row("version", "x", [1.0, 1.0, 1.0, 8.0])
        self.assertEqual(entry["value"], 1.0)
        self.assertEqual(entry["worst"], 8.0)
        self.assertEqual(lanes.median([3.0, 1.0, 2.0]), 2.0)

    def test_the_terminal_client_has_a_longer_threshold_than_the_version_flag(self):
        """An implementation using one threshold flags a terminal screen that takes a second to draw
        or lets `--version` take as long as the client."""
        self.assertLess(lanes.START_SECONDS["version"], lanes.START_SECONDS["terminal"])
        self.assertIsNone(lanes.judge(lanes.startup_row("terminal", "t", [1.0]), None))
        self.assertIsNotNone(lanes.judge(lanes.startup_row("version", "v", [1.0]), None))


def units(*rows):
    return [dict(i=i, name=n, version="1.0", mode="build", start=s, duration=d, unlocked_units=u)
            for i, n, s, d, u in rows]


class Build(unittest.TestCase):
    def page(self, data):
        return f"<html><script>\nconst UNIT_DATA = {json.dumps(data)};\n</script></html>"

    def test_the_critical_path_follows_the_unit_each_one_waited_for(self):
        """An implementation taking the longest unit that unlocked `c` as the one it waited for names
        `b`, which ran 35 s and finished at 35 s, before `a` finished at 40 s."""
        data = units((0, "a", 20, 20, [2]), (1, "b", 0, 35, [2]), (2, "c", 40, 70, []))
        chain = lanes.critical_path(lanes.parse_units(self.page(data)))
        self.assertEqual([u["name"] for u in chain], ["c", "a"])

    def test_a_unit_on_the_critical_path_is_held_to_the_shorter_threshold(self):
        """An implementation using one threshold for every unit either misses `a`, which decided the
        build at 40 s, or reports `b`, which at 35 s saved nothing."""
        data = units((0, "a", 0, 40, [2]), (1, "b", 0, 35, [2]), (2, "c", 40, 70, []))
        found = {r["name"]: lanes.judge(r, None) for r in lanes.unit_rows(lanes.parse_units(self.page(data)))}
        self.assertIsNotNone(found["a 1.0"])
        self.assertIsNone(found["b 1.0"])
        self.assertIsNotNone(found["c 1.0"])

    def test_a_page_without_unit_data_gives_no_units_and_does_not_raise(self):
        """An implementation indexing into the match raises on a cargo that changed its page."""
        self.assertEqual(lanes.parse_units("<html>nothing</html>"), [])
        self.assertEqual(lanes.parse_units("const UNIT_DATA = [not json];\n"), [])

    def test_a_build_script_run_is_named_as_one_and_not_as_its_crate(self):
        """An implementation naming both units `foo 1.0` merges the compile of a crate with the run
        of its build script in the ledger, so one regression hides the other."""
        unit = dict(name="foo", version="1.0", mode="run-custom-build")
        self.assertEqual(lanes.unit_name(unit), "foo 1.0 (build script run)")
        self.assertEqual(lanes.unit_name(dict(name="foo", version="1.0", mode="build")), "foo 1.0")


class Size(unittest.TestCase):
    def test_bloat_output_gives_the_file_size_and_each_crate(self):
        """An implementation reading `text` sizes, or summing the crates, reports the code a binary
        holds and not the file a person downloads."""
        text = json.dumps({"file-size": 123456, "crates": [{"name": "std", "size": 100}, {"name": "regex", "size": 50}]})
        self.assertEqual(lanes.parse_bloat(text), (123456, [("std", 100), ("regex", 50)]))
        self.assertEqual(lanes.parse_bloat("not json"), (0, []))

    def test_a_size_row_is_the_file_on_disk_and_names_the_crates_it_was_given(self):
        """An implementation dropping the crates leaves a growth report that cannot say what grew."""
        entry = lanes.size_rows({"bravebot": 7}, {"bravebot": [("std", 5)]})[0]
        self.assertEqual((entry["key"], entry["value"], entry["crates"]), ("size:bravebot", 7.0, [("std", 5)]))

    def test_only_executable_regular_files_are_release_binaries(self):
        """An implementation listing every entry reports `bravebot.d`, `deps` and the build
        fingerprints as binaries."""
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            (directory / "bravebot").write_text("x")
            (directory / "bravebot").chmod(0o755)
            (directory / "bravebot.d").write_text("x")
            (directory / "bravebot.d").chmod(0o755)
            (directory / "notes").write_text("x")
            (directory / "deps").mkdir()
            self.assertEqual(sorted(measure.executables(directory)), ["bravebot"])


class Scripts(unittest.TestCase):
    MAKEFILE = textwrap.dedent("""\
        check-scripts: a-selftest b-selftest \\
        \tc-selftest
        \t@echo run
        check-scripts-extra: other
    """)

    def test_prerequisites_join_a_continued_line_and_match_only_the_named_target(self):
        """An implementation reading one line misses `c-selftest`, and one matching by prefix reads
        `check-scripts-extra` as the target."""
        self.assertEqual(lanes.prerequisites(self.MAKEFILE, "check-scripts"), ["a-selftest", "b-selftest", "c-selftest"])
        self.assertEqual(lanes.prerequisites(self.MAKEFILE, "absent"), [])

    def test_a_slow_script_is_a_candidate_and_a_quick_one_is_not(self):
        """An implementation with no threshold reports every selftest."""
        rows = lanes.script_rows([("check-spec", 45.0), ("check-versions", 1.0)])
        self.assertEqual([bool(lanes.judge(r, None)) for r in rows], [True, False])


class Measuring(unittest.TestCase):
    def test_a_measured_program_gets_a_home_of_its_own_and_no_bravebot_home(self):
        """An implementation passing the environment through lets a measurement read and write the
        person's `~/.bravebot`, and a session's `BRAVEBOT_HOME` redirects the state somewhere that is
        not the scratch directory."""
        with tempfile.TemporaryDirectory() as temp:
            before = dict(os.environ)
            os.environ.update(HOME="/the/real/home", BRAVEBOT_HOME="/the/real/state")
            try:
                context = measure.Context(temp, Path(temp) / "scratch")
                env = context.env()
            finally:
                os.environ.clear()
                os.environ.update(before)
        self.assertEqual(env["HOME"], str(Path(temp) / "scratch" / "home"))
        self.assertFalse("BRAVEBOT_HOME" in env, "BRAVEBOT_HOME reached a measured program")

    def test_a_program_that_outlasts_its_deadline_returns_no_exit_code(self):
        """An implementation with no timeout waits for ever, and one reporting the kill as exit
        code 0 records a time for a run that did not finish."""
        code, _, _, seconds = measure.run([sys.executable, "-c", "import time; time.sleep(30)"], ".", dict(os.environ), 0.3)
        self.assertIsNone(code)
        self.assertLess(seconds, 10)

    def test_a_merged_run_keeps_stderr_and_stdout_in_the_order_they_were_written(self):
        """An implementation capturing the streams apart gives cargo's stderr `Running` line no place
        among the stdout test lines, so every test lands under the empty binary."""
        script = ("import sys; sys.stderr.write('Running a\\n'); sys.stderr.flush(); "
                  "print('test x', flush=True); sys.stderr.write('Doc-tests b\\n')")
        _, out, err, _ = measure.run([sys.executable, "-c", script], ".", dict(os.environ), 30, merge_stderr=True)
        self.assertEqual(out.splitlines(), ["Running a", "test x", "Doc-tests b"])
        self.assertEqual(err, "")

    def test_the_tests_lane_does_not_report_a_junit_file_an_earlier_run_left(self):
        """An implementation that leaves the old file in place reports its times, and records them as
        the baseline, when the build fails or the run times out."""
        with tempfile.TemporaryDirectory() as temp:
            report = Path(temp) / "target" / "profile-tests" / "nextest" / "profile" / "junit.xml"
            report.parent.mkdir(parents=True)
            report.write_text(JUNIT, encoding="utf-8")
            context = measure.Context(temp, Path(temp) / "scratch")
            for outcome in ((None, "", "timed out", 1.0), (101, "", "build failed", 1.0)):
                report.write_text(JUNIT, encoding="utf-8")
                with unittest.mock.patch.object(measure.shutil, "which", return_value="/usr/bin/x"), \
                        unittest.mock.patch.object(measure, "run", return_value=outcome):
                    rows, note = measure.run_tests(context)
                self.assertEqual(rows, [], outcome)
                self.assertIsNotNone(note)
                self.assertFalse(report.exists())

    def test_the_report_time_fallback_asks_for_cargos_status_lines_so_each_test_keeps_its_binary(self):
        """An implementation of the fallback that reads only stdout gets the test lines without the
        `Running` and `Doc-tests` lines cargo writes to stderr, and files every test under one binary."""
        status = "Running unittests src/lib.rs (target/debug/deps/core-0123456789abcdef)\n"
        tests = "test a ... ok <1.000s>\n"

        def cargo(args, cwd, env, timeout, merge_stderr=False):
            return (0, status + tests, "", 1.0) if merge_stderr else (0, tests, status, 1.0)

        with tempfile.TemporaryDirectory() as temp:
            context = measure.Context(temp, Path(temp) / "scratch")
            with unittest.mock.patch.object(measure.shutil, "which", lambda name: None if name == "cargo-nextest" else "/usr/bin/x"), \
                    unittest.mock.patch.object(measure, "run", cargo):
                rows, _ = measure.run_tests(context)
        self.assertIn("tests:core", keyed(rows))
        self.assertNotIn("tests:", keyed(rows))

    @unittest.skipUnless(hasattr(os, "fork"), "needs a pty")
    def test_a_terminal_start_ends_when_the_output_is_still_and_a_silent_program_has_none(self):
        """An implementation timing to the end of the process reports the sleep after the screen was
        drawn, and one returning the limit for a program that prints nothing records a start."""
        env = dict(os.environ)
        drawn = measure.terminal_start(
            [sys.executable, "-c", "import time; print('screen', flush=True); time.sleep(5)"], env, ".")
        silent = measure.terminal_start([sys.executable, "-c", "import time; time.sleep(1)"], env, ".")
        self.assertIsNotNone(drawn)
        self.assertLess(drawn, 4.0)
        self.assertIsNone(silent)


def fake_runners(rows_by_lane, notes=None):
    return {lane: (lambda context, lane=lane: (rows_by_lane.get(lane, []), (notes or {}).get(lane)))
            for lane in p.LANES}


def test_row(value, key="tests:b::t"):
    return lanes.row("tests", key, f"test {key}", value, "seconds", lanes.TEST_SECONDS)


class Pipeline(unittest.TestCase):
    def scan(self, rows, previous=None, **options):
        with tempfile.TemporaryDirectory() as temp:
            result, every = p.scan(temp, ("tests",), Path(temp) / "s", fake_runners({"tests": rows}),
                                   previous=previous, **options)
        return result["tests"], every

    def test_a_row_a_reviewer_kept_is_carried_until_it_grows_past_the_margin(self):
        """An implementation carrying every recorded key hides a regression in a test somebody once
        kept; one carrying none reads the same keep again every run."""
        previous = {"tests:b::t": dict(value=60.0, verdict="keep", date="d", commit="c", reason="")}
        same, _ = self.scan([test_row(62.0)], previous)
        grown, _ = self.scan([test_row(90.0)], previous)
        fresh, _ = self.scan([test_row(62.0)], previous, fresh=True)
        self.assertEqual((len(same["candidates"]), same["carried"]), (0, 1))
        self.assertEqual(len(grown["candidates"]), 1)
        self.assertTrue(grown["candidates"][0]["regressed"])
        self.assertEqual(len(fresh["candidates"]), 1)

    def test_a_row_the_ledger_only_measured_is_a_candidate_again_while_it_is_over_its_threshold(self):
        """An implementation carrying `measured` rows never reports a slow test twice but also never
        shows it to a reviewer, since nobody read it."""
        previous = {"tests:b::t": dict(value=60.0, verdict="measured", date="d", commit="c", reason="")}
        found, _ = self.scan([test_row(61.0)], previous)
        self.assertEqual(len(found["candidates"]), 1)

    def test_candidates_are_numbered_by_lane_after_ranking_and_capped(self):
        """An implementation numbering before ranking gives `T1` to the mildest row, and one with no
        cap sends a reviewer every slow test."""
        rows = [test_row(40.0, "tests:b::mild"), test_row(400.0, "tests:b::awful"), test_row(100.0, "tests:b::bad")]
        found, _ = self.scan(rows, limit=2)
        self.assertEqual([(c["id"], c["key"]) for c in found["candidates"]],
                         [("T1", "tests:b::awful"), ("T2", "tests:b::bad")])
        self.assertEqual(found["left_out"], 1)

    def test_a_lane_note_reaches_the_report_and_a_lane_with_no_tool_has_no_candidates(self):
        """An implementation raising when a tool is missing stops the whole run, and one saying
        nothing leaves a person reading an empty lane as a clean one."""
        with tempfile.TemporaryDirectory() as temp:
            result, _ = p.scan(temp, ("size",), Path(temp) / "s",
                               fake_runners({}, {"size": "cargo-bloat is not installed"}))
        self.assertEqual(result["size"]["candidates"], [])
        self.assertIn("note: cargo-bloat is not installed", p.report(result))

    def test_record_writes_every_measured_row_so_the_next_run_has_a_baseline(self):
        """An implementation recording only the candidates leaves a test that was 20 s with no value,
        so its growth to 24 s next month is not a regression anyone is told about."""
        with tempfile.TemporaryDirectory() as temp:
            manifest = {"root": temp, "commit": "0123456789abcdef", "rows": [test_row(20.0, "tests:b::fine"), test_row(80.0)],
                        "candidates": {"T1": dict(test_row(80.0), id="T1")}}
            path, count = p.record(manifest, {"T1": {"verdict": "optimize", "reason": "sleeps"}}, "2026-11-24")
            entries, problems = ledger.read(temp)
        self.assertEqual((count, problems), (2, []))
        self.assertIn("tests:b::fine", entries)
        self.assertEqual(entries["tests:b::fine"]["verdict"], "measured")
        self.assertEqual(entries["tests:b::fine"]["value"], 20.0)
        self.assertEqual((entries["tests:b::t"]["verdict"], entries["tests:b::t"]["reason"]), ("optimize", "sleeps"))
        self.assertEqual(entries["tests:b::t"]["commit"], "0123456789ab")
        self.assertTrue(str(path).endswith("docs/profile-reviewed"))

    def test_the_ledger_round_trips_a_key_with_spaces_and_percent_signs(self):
        """An implementation splitting on whitespace reads a key with a space as two fields and
        loses the value."""
        with tempfile.TemporaryDirectory() as temp:
            key = "build:foo 1.0 (build script run) 100%"
            ledger.write(temp, {key: dict(value=1.5, date="d", commit="c", verdict="keep", reason="why not")})
            entries, problems = ledger.read(temp)
        self.assertEqual(problems, [])
        self.assertEqual(entries[key]["value"], 1.5)
        self.assertEqual(entries[key]["reason"], "why not")

    def test_a_malformed_ledger_line_is_reported_and_skipped(self):
        """An implementation raising on a hand-edited line stops every run, and one skipping it
        without a word leaves its key unreviewed for no visible reason."""
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / ledger.LEDGER
            path.parent.mkdir()
            path.write_text("good 1.0 d c keep\nbad nan-ish d c keep\nshort 1\n", encoding="utf-8")
            entries, problems = ledger.read(temp)
        self.assertEqual(list(entries), ["good"])
        self.assertEqual(len(problems), 2)

    def test_optimize_is_the_verdict_that_needs_a_finding_and_cleanup_is_not_one_here(self):
        """An implementation inheriting maintenance's `cleanup` never asks for the finding behind an
        `optimize`, so a draft is lost with no gap reported."""
        with tempfile.TemporaryDirectory() as temp:
            results = Path(temp) / "r.json"
            results.write_text(json.dumps({"candidates": [{"id": "T1", "verdict": "optimize"},
                                                         {"id": "T2", "verdict": "cleanup"}]}))
            manifest = {"lanes": [{"lane": "tests", "reviewer_prompts": [
                {"results_file": str(results), "candidates": ["T1", "T2"]}]}]}
            _, _, problems = p.pipeline.load_reviews(manifest, p.CHANGE)
        self.assertEqual([(q["candidates"], q["summary"]) for q in problems],
                         [(["T1"], "called optimize, but no finding says what the change is")])

    def test_labels_follow_the_finding_and_the_scripts_lane_is_infrastructure(self):
        """An implementation labelling every finding `refactor` files an optimization as a cleanup;
        one trusting the reviewer's area applies a label the tracker does not have."""
        self.assertEqual(p.labels_for({"lane": "tests"}, {}), ["enhancement"])
        self.assertEqual(p.labels_for({"lane": "tests", "bug": True, "area": "trust"}, {}), ["bug", "area/trust"])
        self.assertEqual(p.labels_for({"lane": "tests", "area": "made-up"}, {}), ["enhancement"])
        self.assertEqual(p.labels_for({"lane": "scripts"}, {}), ["enhancement", "infrastructure"])

    def test_the_reviewer_prompt_states_the_rule_for_labelled_content_in_every_lane(self):
        """An implementation building a lane's prompt without the rule lets a reviewer recommend a
        cache or an early return on a decision about labelled content."""
        chunk = [dict(test_row(80.0), id="T1", detail="d")]
        for lane in p.LANES:
            text = p.build_prompt(lane, chunk, "/r.json", "reading", "/root")
            self.assertIn("never have untrusted content in their context", text, lane)
            self.assertIn("Never recommend an optimization that moves", text, lane)
            self.assertIn("/r.json", text)
        self.assertEqual(sorted(p.prompts.QUESTION), sorted(p.LANES))
        self.assertEqual(sorted(p.prompts.LANE_NOTES), sorted(p.LANES))

    def test_the_claim_readers_get_the_same_rule(self):
        """An implementation passing no rule to the claim prompt lets a reader hold a draft that
        speeds up a decision about labelled content as `holds`."""
        with tempfile.TemporaryDirectory() as temp:
            body = Path(temp) / "a.md"
            body.write_text("x")
            text = p.pipeline.build_claim_prompt([{"slug": "a", "body_file": str(body)}], "/r.json", "reading", p.prompts.READER_RULE)
        self.assertIn("Never recommend an optimization that moves", text)

    def test_measure_only_prints_candidates_and_exits_zero_and_an_unknown_lane_exits_two(self):
        """An implementation exiting 1 on candidates makes `make check-profile` fail on a slow test,
        and one ignoring an unknown lane name measures every lane when a person typed one wrongly."""
        with tempfile.TemporaryDirectory() as temp:
            before = os.getcwd()
            try:
                with unittest.mock.patch.dict(measure.RUNNERS, fake_runners({"tests": [test_row(80.0)]})):
                    code, out, _ = quiet(p.main, ["tests", "--measure-only", "--root", temp])
                    bad = quiet(p.main, ["tets", "--measure-only", "--root", temp])
            finally:
                os.chdir(before)
        self.assertEqual(code, 0)
        self.assertIn("T1", out)
        self.assertEqual(bad[0], 2)

    def test_prepare_reads_a_checkout_of_the_commit_and_writes_one_prompt_per_chunk(self):
        """An implementation sending every candidate of a lane to one reviewer gives a reader more
        than a chunk, and one with no checkout lets the working tree move under the reviewers."""
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "repo"
            root.mkdir()
            (root / "a.txt").write_text("x")
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            subprocess.run(["git", "-C", str(root), "add", "-A"], check=True)
            subprocess.run(["git", "-C", str(root), "commit", "-q", "-m", "x"], check=True)
            rows = [test_row(40.0 + i, f"tests:b::t{i}") for i in range(p.CHUNK + 1)]
            work = Path(temp) / "work"
            with unittest.mock.patch.dict(measure.RUNNERS, fake_runners({"tests": rows})):
                _, manifest = p.prepare(root, ("tests",), work, limit=p.CHUNK + 1)
            data = json.loads(manifest.read_text())
            tree_exists = (work / "tree" / "a.txt").exists()
            prompts_written = [len(x["candidates"]) for x in data["lanes"][0]["reviewer_prompts"]]
            p.pipeline.clean(data, work)
        self.assertTrue(tree_exists)
        self.assertEqual(prompts_written, [p.CHUNK, 1])
        self.assertEqual(len(data["rows"]), p.CHUNK + 1)


def stacks(*groups):
    """Samples from `(count, [function, ...])` pairs, the running function first."""
    return [list(stack) for count, stack in groups for _ in range(count)]


PERF_SCRIPT = (
    "\t55d0 regex::compile+0x10 (/x/bravebot)\n"
    "\t55d1 bravebot::load::h0123456789abcdef+0x4 (/x/bravebot)\n"
    "\t55d2 main+0x1 (/x/bravebot)\n"
    "\n"
    "\t55d0 regex::compile+0x20 (/x/bravebot)\n"
    "\t55d2 main+0x1 (/x/bravebot)\n"
    "\n"
    "\t7f00 [unknown] ([vdso])\n"
    "\t55d2 main+0x1 (/x/bravebot)\n"
)


class Functions(unittest.TestCase):
    def test_a_sample_is_the_stack_between_blank_lines_with_each_name_cleaned(self):
        """An implementation keeping the offset, the object file or the hash gives one function a name
        per call site, so its samples never add up. One reading the whole output as a single stack, or
        ending a sample only at a header, merges every sample into one."""
        self.assertEqual(
            lanes.parse_perf_script(PERF_SCRIPT),
            [["regex::compile", "bravebot::load", "main"], ["regex::compile", "main"], [lanes.UNKNOWN, "main"]],
        )
        self.assertEqual(lanes.parse_perf_script("header line\n\t1 a (/x)\nheader again\n\t2 b (/x)\n"), [["a"], ["b"]])
        self.assertEqual(lanes.parse_perf_script(""), [])

    def test_functions_are_ranked_by_the_samples_they_hold_themselves_and_not_by_the_stacks_they_are_on(self):
        """An implementation ranking by inclusive time puts `main`, which is on every stack and runs
        nothing itself, above the function the time went in."""
        samples = stacks((60, ["hot", "mid", "main"]), (30, ["cold", "mid", "main"]), (10, ["main"]))
        total, functions = lanes.rank_functions(samples)
        self.assertEqual(total, 100)
        self.assertEqual([(f["name"], f["self"], f["inclusive"]) for f in functions][:3],
                         [("hot", 60, 60), ("cold", 30, 30), ("main", 10, 100)])
        self.assertEqual({f["name"]: f["inclusive"] for f in functions}["mid"], 90)

    def test_a_function_on_a_stack_many_times_is_counted_once_for_that_sample(self):
        """An implementation counting every frame reports a recursive function on 3 of 1 samples, and a
        share over 100%."""
        total, functions = lanes.rank_functions([["f", "f", "f", "main"]])
        self.assertEqual({f["name"]: (f["self"], f["inclusive"]) for f in functions}, {"f": (1, 1), "main": (0, 1)})

    def test_a_frame_with_no_name_is_a_sample_and_never_a_function(self):
        """An implementation leaving unnamed frames out of the total reports every named function's share
        too high, and one listing them reports `[unknown]` as the hottest function."""
        total, functions = lanes.rank_functions(stacks((3, [lanes.UNKNOWN, "main"]), (1, ["f", "main"])))
        self.assertEqual(total, 4)
        self.assertNotIn(lanes.UNKNOWN, [f["name"] for f in functions])

    def command(self):
        return lanes.judge(lanes.row("tests", "tests:b::t", "test t in b", 80.0, "seconds", 30.0, binary="b", test="t"), None)

    def test_a_function_is_a_candidate_by_its_share_and_one_on_every_stack_is_not_listed_by_inclusive_time(self):
        """An implementation with one threshold for self and inclusive time reports `mid` as slow as
        `hot`, and one listing inclusive time with no exclusion reports `main`, which every command is
        on, as the cause of every slow command."""
        samples = stacks((60, ["hot", "mid", "main"]), (30, ["cold", "mid", "main"]), (10, ["main"]))
        rows, total = lanes.function_rows(self.command(), samples)
        found = {r["key"]: lanes.judge(r, None) for r in rows}
        key = "functions:tests:b::t:"
        self.assertEqual(total, 100)
        self.assertIsNotNone(found[key + "self:hot"])
        self.assertIsNotNone(found[key + "self:cold"])
        self.assertIsNone(found[key + "self:main"])
        self.assertIsNotNone(found[key + "inclusive:mid"])
        self.assertIsNone(found[key + "inclusive:cold"])
        self.assertNotIn(key + "inclusive:main", found)
        self.assertAlmostEqual(found[key + "self:hot"]["value"], 0.6)
        self.assertEqual(found[key + "self:hot"]["command"], "tests:b::t")

    def test_too_few_samples_give_no_function_at_all(self):
        """An implementation ranking whatever it was given reports a function that held 1 of 3 samples
        as a third of the command."""
        rows, total = lanes.function_rows(self.command(), stacks((3, ["f", "main"])))
        self.assertEqual((rows, total), ([], 3))

    def test_a_function_row_carries_the_location_that_was_looked_up_for_it(self):
        """An implementation looking up places for every function, or for none, gives a reviewer a list
        of names with no code to read, or spends a lookup on functions it dropped."""
        asked = []
        samples = stacks((25, ["hot", "main"]))
        rows, _ = lanes.function_rows(self.command(), samples, lambda names: asked.append(names) or {"hot": "a.rs:3"})
        self.assertEqual(asked, [["hot"]])
        self.assertEqual({r["function"]: r["location"] for r in rows}, {"hot": "a.rs:3"})

    def test_only_a_single_test_and_a_start_are_sampled_and_the_worst_are_first(self):
        """An implementation sampling every candidate runs a test binary added up, a compile unit and a
        size as commands, and one sorting by the raw value puts a bigger unit first."""
        def candidate(lane, key, value, threshold, **more):
            return lanes.judge(lanes.row(lane, key, key, value, "seconds", threshold, **more), None)
        found = [
            candidate("tests", "tests:b::slow", 40.0, 30.0, binary="b", test="slow"),
            candidate("tests", "tests:b::slower", 90.0, 30.0, binary="b", test="slower"),
            candidate("tests", "tests:b", 500.0, 120.0, binary="b"),
            candidate("build", "build:x", 900.0, 60.0),
            candidate("startup", "startup:terminal", 9.0, 3.0),
        ]
        self.assertEqual([c["key"] for c in lanes.sampled_commands(found)],
                         ["startup:terminal", "tests:b::slower", "tests:b::slow"])

    def test_the_arguments_of_a_test_run_name_that_test_exactly(self):
        """An implementation passing the name as a filter also runs every test whose name contains it,
        so the samples are of a different command than the one that was slow."""
        self.assertEqual(lanes.test_command("/w/b", "a::t"), ["/w/b", "--exact", "a::t", "--nocapture", "--test-threads", "1"])

    def test_test_binaries_are_named_as_the_tests_lane_names_them(self):
        """An implementation naming every binary by its target finds none of nextest's `pkg::test` or
        `pkg::bin/name`, so no slow test has a binary to run, and one taking a build of the library for
        a test binary samples a program that runs no test."""
        def artifact(package, kind, name, executable, test=True):
            return json.dumps({"reason": "compiler-artifact", "package_id": f"path+file:///w/crates/{package}#{package}@0.1.0",
                               "manifest_path": f"/w/crates/{package}/Cargo.toml", "target": {"kind": [kind], "name": name},
                               "profile": {"test": test}, "executable": executable})
        text = "\n".join([
            artifact("core", "lib", "core", "/t/core-0123456789abcdef"),
            artifact("agent", "test", "turn", "/t/turn-fedcba9876543210"),
            artifact("cli", "bin", "bravebot", "/t/bravebot-00112233aabbccdd"),
            artifact("core", "lib", "core", "/t/notest-0123456789abcdef", test=False),
            "not json",
            json.dumps({"reason": "build-finished"}),
        ])
        found = lanes.parse_test_executables(text)
        self.assertEqual(found["core"], ("/t/core-0123456789abcdef", "/w/crates/core"))
        self.assertEqual(found["agent::turn"][0], "/t/turn-fedcba9876543210")
        self.assertEqual(found["cli::bin/bravebot"][0], "/t/bravebot-00112233aabbccdd")
        self.assertEqual(found["turn"][0], "/t/turn-fedcba9876543210")
        self.assertEqual([v[0] for v in found.values() if "core" in v[0]], ["/t/core-0123456789abcdef"])
        self.assertNotIn("/t/notest-0123456789abcdef", [v[0] for v in found.values()])

    def test_the_lowest_address_of_a_name_is_where_it_is_looked_up_and_other_names_are_left(self):
        """An implementation taking the last address of a generic function looks up one copy of it that
        may be inlined elsewhere, and one keeping every name builds a lookup the size of the binary."""
        text = ("0000000000002000 t f<u8>\n0000000000001000 t f<u8>\n0000000000003000 T other\n"
                "0000000000004000 b a_static\n0000000000005000 t g::h0123456789abcdef\n")
        self.assertEqual(lanes.parse_nm(text, {"f<u8>", "g"}), {"f<u8>": 0x1000, "g": 0x5000})
        self.assertEqual(sorted(lanes.parse_nm(text)), ["f<u8>", "g", "other"])

    def test_addr2line_gives_a_place_for_each_address_and_none_where_it_found_nothing(self):
        """An implementation reading the next line as the place of an address that has none attributes
        `??:0` to the function, and one keeping the discriminator names a line that does not exist."""
        text = "0x1000\n/w/src/a.rs:7 (discriminator 3)\n0x2000\n??:0\n0x3000\n/w/src/b.rs:9\n"
        self.assertEqual(lanes.parse_addr2line(text), {0x1000: "/w/src/a.rs:7", 0x2000: None, 0x3000: "/w/src/b.rs:9"})

    def test_a_location_is_written_from_the_checkout_and_the_standard_library_is_written_from_library(self):
        """An implementation printing the full path puts the builder's directory in a draft, where a
        reader of the checkout cannot open it."""
        self.assertEqual(lanes.short_location("/w/crates/a/src/lib.rs:7", "/w"), "crates/a/src/lib.rs:7")
        self.assertEqual(lanes.short_location("/b/s/rust-src/library/core/src/cmp.rs:9", "/w"), "library/core/src/cmp.rs:9")
        self.assertIsNone(lanes.short_location(None, "/w"))

    def test_a_function_that_grew_since_the_ledger_is_reported_and_its_key_reads_back(self):
        """An implementation keying the ledger on the function's text unescaped loses a name with spaces
        (`<A as B>::f`), so its growth is never reported."""
        key = "functions:tests:b::t:self:<core::fmt::Arguments as core::fmt::Display>::fmt"
        with tempfile.TemporaryDirectory() as temp:
            entry = lanes.row("functions", key, "fmt", 0.234, "share", lanes.FUNCTION_SHARE)
            p.record({"root": temp, "commit": "0123456789abcdef", "rows": [entry], "candidates": {}}, {}, "2026-11-25")
            entries, problems = ledger.read(temp)
        self.assertEqual((problems, entries[key]["value"]), ([], 0.234))
        grown = lanes.judge(lanes.row("functions", key, "fmt", 0.30, "share", lanes.FUNCTION_SHARE), entries[key]["value"])
        self.assertTrue(grown["regressed"])
        self.assertIn("30.0% of the samples", grown["detail"])


class Sampling(unittest.TestCase):
    ARTIFACT = json.dumps({
        "reason": "compiler-artifact", "package_id": "path+file:///w/crates/b#b@0.1.0", "manifest_path": "/w/crates/b/Cargo.toml",
        "target": {"kind": ["lib"], "name": "b"}, "profile": {"test": True}, "executable": "/w/target/b-0123456789abcdef",
    })

    def candidate(self):
        return lanes.judge(lanes.row("tests", "tests:b::t", "test t in b", 80.0, "seconds", 30.0, binary="b", test="t"), None)

    def context(self, temp):
        context = measure.Context("/w", Path(temp) / "scratch")
        context.lanes = ("tests",)
        context.candidates = [self.candidate()]
        return context

    def test_a_machine_perf_cannot_record_on_gets_a_note_and_nothing_is_built(self):
        """An implementation that builds and runs before it asks whether perf works spends the build of
        the whole workspace to learn that the kernel refuses it, and one that says nothing leaves an
        empty lane to be read as a clean one."""
        calls = []

        def refused(args, cwd, env, timeout, merge_stderr=False):
            calls.append([str(a) for a in args])
            return 255, "", "Access to performance monitoring and observability operations is limited", 0.1

        with tempfile.TemporaryDirectory() as temp:
            with unittest.mock.patch.object(measure.shutil, "which", return_value="/usr/bin/perf"), \
                    unittest.mock.patch.object(measure, "run", refused):
                rows, note = measure.run_functions(self.context(temp))
        self.assertEqual(rows, [])
        self.assertIn("perf cannot record here", note)
        self.assertIn("limited", note)
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][1], "record")

    def test_a_missing_perf_and_nothing_to_sample_are_each_said(self):
        """An implementation raising on a missing tool stops the run, and one that does not say why a
        lane is empty reads as a lane that found nothing slow."""
        with tempfile.TemporaryDirectory() as temp:
            context = self.context(temp)
            with unittest.mock.patch.object(measure.shutil, "which", return_value=None):
                _, missing = measure.run_functions(context)
            context.candidates = []
            _, nothing = measure.run_functions(context)
            context.lanes = ("functions",)
            _, unmeasured = measure.run_functions(context)
        self.assertIn("perf is not installed", missing)
        self.assertIn("nothing to sample", nothing)
        self.assertIn("neither ran", unmeasured)

    def test_a_slow_test_is_run_alone_under_perf_with_a_scratch_home_and_its_functions_are_located(self):
        """An implementation passing the person's environment lets the sampled test read and write
        `~/.bravebot`, one running the test by filter samples other tests as well, and one that drops
        the lookup gives the reviewer function names with no file."""
        perf_text = "".join("\t1 hot+0x1 (/w/b)\n\t2 main+0x1 (/w/b)\n\n" for _ in range(30))
        calls = []

        def fake(args, cwd, env, timeout, merge_stderr=False):
            args = [str(a) for a in args]
            calls.append((args, str(cwd), env))
            if args[0] == "cargo":
                return 0, self.ARTIFACT + "\n", "", 1.0
            if args[1:2] == ["record"]:
                Path(args[args.index("-o") + 1]).write_bytes(b"data")
                return 0, "", "", 1.0
            if args[1:2] == ["script"]:
                return 0, perf_text, "", 1.0
            if "--defined-only" in args:
                return 0, "0000000000001000 T hot\n", "", 1.0
            if "-a" in args:
                return 0, "0x1000\n/w/crates/b/src/lib.rs:7\n", "", 1.0
            raise AssertionError(args)

        with tempfile.TemporaryDirectory() as temp:
            context = self.context(temp)
            with unittest.mock.patch.dict(os.environ, {"BRAVEBOT_HOME": "/the/real/state", "HOME": "/the/real/home"}), \
                    unittest.mock.patch.object(measure.shutil, "which", lambda name: f"/usr/bin/{name}"), \
                    unittest.mock.patch.object(measure, "run", fake):
                rows, note = measure.run_functions(context)
            scratch_home = str(context.home)
        self.assertIsNone(note)
        self.assertEqual([(r["function"], r["time_kind"], r["location"]) for r in rows], [("hot", "self", "crates/b/src/lib.rs:7")])
        recorded = [c for c in calls if c[0][1:2] == ["record"] and "-F" in c[0]]
        self.assertEqual(len(recorded), 1)
        argv, cwd, env = recorded[0]
        self.assertEqual(argv[argv.index("--") + 1:],
                         ["/w/target/b-0123456789abcdef", "--exact", "t", "--nocapture", "--test-threads", "1"])
        self.assertEqual((cwd, env["CARGO_MANIFEST_DIR"], env["HOME"]), ("/w/crates/b", "/w/crates/b", scratch_home))
        self.assertNotIn("BRAVEBOT_HOME", env)
        self.assertEqual(context.hot["tests:b::t"][0]["name"], "hot")

    def test_the_profiling_build_leaves_the_release_profile_and_the_release_target_alone(self):
        """An implementation building into the release target directory replaces the binary the size lane
        measures with one that carries line tables, and one that edits `[profile.release]` in the
        manifest changes what every release build ships."""
        calls = []

        def fake(args, cwd, env, timeout, merge_stderr=False):
            calls.append(([str(a) for a in args], env))
            return 0, "", "", 1.0

        with tempfile.TemporaryDirectory() as temp:
            context = measure.Context(temp, Path(temp) / "scratch")
            built = context.scratch / "profiling-target" / "release" / "bravebot"
            built.parent.mkdir(parents=True)
            built.write_text("x")
            with unittest.mock.patch.object(measure, "run", fake):
                binary, note = measure.profiling_build(context)
                again = measure.profiling_build(context)
        self.assertEqual((binary, note), (built, None))
        self.assertEqual(again, (binary, note))
        self.assertEqual(len(calls), 1)
        args, env = calls[0]
        self.assertIn(measure.PROFILING_CONFIG, args)
        self.assertEqual(args[args.index("--config") - 1], "bravebot-cli")
        self.assertTrue(env["CARGO_TARGET_DIR"].endswith("profiling-target"))
        self.assertNotIn("release-target", env["CARGO_TARGET_DIR"])

    @unittest.skipUnless(hasattr(os, "fork"), "needs a pty")
    def test_a_sampler_started_under_a_terminal_is_stopped_with_sigint_so_it_writes_its_file(self):
        """An implementation ending it with SIGKILL, as the start lane does, leaves perf with no file to
        read."""
        with tempfile.TemporaryDirectory() as temp:
            marker = Path(temp) / "wrote"
            script = ("import signal, sys, time\n"
                      f"signal.signal(signal.SIGINT, lambda *a: (open({str(marker)!r}, 'w').close(), sys.exit(0)))\n"
                      "print('screen', flush=True)\ntime.sleep(20)\n")
            measure.terminal_start([sys.executable, "-c", script], dict(os.environ), ".", stop=signal.SIGINT)
            self.assertTrue(marker.exists())

    def test_a_start_is_sampled_under_a_terminal_and_stopped_with_sigint(self):
        """An implementation that records the terminal client with the start lane's SIGKILL leaves perf
        with no file, so the start has no functions, and one that runs it without a terminal samples
        a client that exits at once because it has no screen."""
        seen = {}

        def started(argv, env, cwd, stop=signal.SIGKILL):
            seen.update(argv=[str(a) for a in argv], stop=stop)
            Path(argv[argv.index("-o") + 1]).write_bytes(b"data")
            return 1.0

        with tempfile.TemporaryDirectory() as temp:
            context = measure.Context(temp, Path(temp) / "scratch")
            with unittest.mock.patch.object(measure.shutil, "which", lambda name: f"/usr/bin/{name}"), \
                    unittest.mock.patch.object(measure, "terminal_start", started), \
                    unittest.mock.patch.object(measure, "run", lambda *a, **k: (0, "", "", 0.1)):
                samples, problem = measure.sample(context, ["/w/bravebot"], temp, 4999, True)
        self.assertEqual((samples, problem), ([], None))
        self.assertEqual(seen["stop"], signal.SIGINT)
        self.assertEqual(seen["argv"][seen["argv"].index("--") + 1:], ["/w/bravebot"])


class Scanning(unittest.TestCase):
    def test_the_functions_lane_runs_after_the_lanes_it_samples_and_names_its_functions_beside_them(self):
        """An implementation running it first has no candidates to sample, one running only the lanes
        named leaves a scoped `functions` run with nothing, and one that does not carry the functions
        to the candidate gives a reviewer a slow test with no function to read."""
        seen = []

        def functions(context):
            seen.append(sorted(c["key"] for c in context.candidates))
            context.hot["tests:b::t"] = [{"name": "f", "self": 0.5, "inclusive": 0.6, "location": "a.rs:1"}]
            return [], None

        runners = fake_runners({"tests": [test_row(80.0)]})
        runners["functions"] = functions
        with tempfile.TemporaryDirectory() as temp:
            result, _ = p.scan(temp, ("functions",), Path(temp) / "s", runners)
        self.assertEqual(seen, [["tests:b::t"]])
        self.assertEqual(sorted(result), ["functions", "startup", "tests"])
        candidate = result["tests"]["candidates"][0]
        self.assertEqual(candidate["hot"][0]["name"], "f")
        self.assertIn("hot: `f` at `a.rs:1`, self 50.0%, inclusive 60.0%", p.render_candidate(candidate))

    def test_a_function_candidate_is_rendered_with_its_samples_and_place_and_numbered_from_f(self):
        """An implementation numbering function candidates from the tests lane's `T` collides with a
        test candidate in the results file a reviewer writes."""
        entry = lanes.row("functions", "functions:tests:b::t:self:f", "f (self time) in test t in b", 0.5, "share",
                          lanes.FUNCTION_SHARE, command="tests:b::t", function="f", time_kind="self", self_share=0.5,
                          inclusive_share=0.6, samples=200, location="a.rs:1")
        with tempfile.TemporaryDirectory() as temp:
            result, _ = p.scan(temp, ("functions",), Path(temp) / "s", fake_runners({"functions": [entry]}))
        candidate = result["functions"]["candidates"][0]
        self.assertEqual(candidate["id"], "F1")
        text = p.render_candidate(candidate)
        self.assertIn("200 samples, self 50.0%, inclusive 60.0%", text)
        self.assertIn("at `a.rs:1`", text)


import unittest.mock  # noqa: E402

if __name__ == "__main__":
    unittest.main()
