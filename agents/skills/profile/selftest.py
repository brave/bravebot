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


import unittest.mock  # noqa: E402

if __name__ == "__main__":
    unittest.main()
