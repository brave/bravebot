#!/usr/bin/env python3
"""Names the changes to the code that no test notices, which is what a test suite is for.

A passing suite says every assertion held. It does not say the assertions would fail if the code
were wrong, and `docs/best-practices/tests.md` (TS-003) asks for exactly that proof of a new test.
cargo-mutants makes it for a whole crate: it changes one thing at a time, such as a `<` to a `<=` or
a function body to `true`, runs the tests, and reports each change they did not fail on as missed.

The report is the product and not an exit code. A crate's missed changes are a list for a person to
read, ranked by the function with the most of them, and a job that went red over the list would be
one people learn to ignore. Only a measurement that did not happen fails here.

A whole crate takes hours, so one run takes one shard of it and `report` joins the shards:

    contrib/measure-mutants.py run --package bravebot-core --shard 0/6 --save shard-0.json
    contrib/measure-mutants.py report shard-*.json --summary summary.md
"""

import argparse
import contextlib
import io
import json
import shutil
# Only ever invoked with an argument list, never a shell string.
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# cargo-mutants exits 0 when every change was caught, 2 when some were missed and 3 when some tests
# timed out. 1 is a usage error and 4 is a suite that already fails before anything is changed,
# and neither measured anything.
MEASURED = (0, 2, 3)

# What a scenario's `summary` says, by the name this report gives it.
OUTCOMES = {
    "CaughtMutant": "caught",
    "MissedMutant": "missed",
    "Timeout": "timeout",
    "Unviable": "unviable",
}

# How many functions and sites the report lists. The rest are in the saved file.
FUNCTIONS = 25
SITES_PER_FUNCTION = 6


def measured(exit_code):
    """Whether cargo-mutants changed the code and ran the tests, whatever it found."""
    return exit_code in MEASURED


def command(package, shard, jobs, output, extra):
    return [
        "cargo", "mutants", "--package", package, "--output", str(output),
        *(["--shard", shard] if shard else []),
        *(["--jobs", str(jobs)] if jobs else []),
        *extra,
    ]


def run(package, shard, jobs, extra, save):
    """Run cargo-mutants over one shard of a package and save its outcomes."""
    if shutil.which("cargo") is None:
        raise SystemExit("measure-mutants: cargo is not installed")
    with tempfile.TemporaryDirectory(prefix="mutants-") as output:
        # The program and arguments are fixed above; no shell reads them.
        done = subprocess.run(  # nosemgrep: gitlab.bandit.B603
            command(package, shard, jobs, output, extra), cwd=ROOT, check=False
        )
        outcomes = Path(output) / "mutants.out" / "outcomes.json"
        if not measured(done.returncode):
            raise SystemExit(f"measure-mutants: cargo mutants exited {done.returncode} and measured nothing")
        if not outcomes.exists():
            raise SystemExit("measure-mutants: cargo mutants wrote no outcomes.json")
        Path(save).write_text(outcomes.read_text(encoding="utf-8"), encoding="utf-8")
    return 0


def mutants(documents):
    """(outcome, file, function, line, name) for every changed program across the documents. The
    baseline run, which changes nothing, is not one."""
    for document in documents:
        for entry in document.get("outcomes", []):
            scenario = entry.get("scenario")
            if not isinstance(scenario, dict) or "Mutant" not in scenario:
                continue
            mutant = scenario["Mutant"]
            outcome = OUTCOMES.get(entry.get("summary"))
            if outcome is None:
                continue
            function = (mutant.get("function") or {}).get("function_name") or "(no function)"
            line = (mutant.get("span") or {}).get("start", {}).get("line", 0)
            yield outcome, mutant.get("file", "?"), function, line, mutant.get("name", "")


def summarize(documents):
    """Totals, the score, and the missed changes grouped by file and by function."""
    totals, by_file, by_function = Counter(), {}, {}
    for outcome, file, function, line, name in mutants(documents):
        totals[outcome] += 1
        by_file.setdefault(file, Counter())[outcome] += 1
        if outcome == "missed":
            by_function.setdefault((file, function), []).append((line, name))
    tested = totals["caught"] + totals["missed"]
    return {
        "totals": dict(totals),
        # Timeouts are left out: a change that makes the code loop is not one the tests were
        # shown to notice or to miss.
        "score": totals["caught"] / tested if tested else None,
        "files": by_file,
        "functions": sorted(by_function.items(), key=lambda item: (-len(item[1]), item[0])),
    }


def render(report):
    totals = report["totals"]
    lines = ["# Mutation testing", ""]
    if report["score"] is None:
        return "\n".join(lines + ["No change was both built and tested, so there is no score.", ""])
    lines.append(
        f"{totals.get('caught', 0)} caught, {totals.get('missed', 0)} missed, "
        f"{totals.get('timeout', 0)} timed out and {totals.get('unviable', 0)} would not compile. "
        f"{report['score']:.1%} of the changes that were tested made a test fail."
    )
    lines += ["", "## Files with the most missed changes", "", "| File | Missed | Caught |", "|---|---|---|"]
    ranked = sorted(report["files"].items(), key=lambda item: (-item[1]["missed"], item[0]))
    lines += [f"| `{file}` | {count['missed']} | {count['caught']} |" for file, count in ranked if count["missed"]]
    lines += ["", "## Functions with the most missed changes", ""]
    for (file, function), sites in report["functions"][:FUNCTIONS]:
        lines.append(f"- `{function}` in `{file}`: {len(sites)}")
        for line, name in sorted(sites)[:SITES_PER_FUNCTION]:
            lines.append(f"  - line {line}: {name.split(': ', 1)[-1]}")
        if len(sites) > SITES_PER_FUNCTION:
            lines.append(f"  - and {len(sites) - SITES_PER_FUNCTION} more")
    if len(report["functions"]) > FUNCTIONS:
        lines.append(f"- and {len(report['functions']) - FUNCTIONS} more functions")
    return "\n".join(lines + [""])


def report_command(files, summary, shards=None):
    if shards is not None and len(files) != shards:
        raise SystemExit(
            f"measure-mutants: {shards} shards were measured and {len(files)} outcomes files were given, "
            "so the report would be of part of the crate"
        )
    documents = []
    for file in files:
        try:
            documents.append(json.loads(Path(file).read_text(encoding="utf-8")))
        except (OSError, json.JSONDecodeError) as problem:
            raise SystemExit(f"measure-mutants: {file} is not an outcomes file: {problem}")
    if not documents:
        raise SystemExit("measure-mutants: no outcomes files were given")
    text = render(summarize(documents))
    print(text)
    if summary:
        with open(summary, "a", encoding="utf-8") as handle:
            handle.write(text)
    return 0


def mutant(outcome, file, function, line, name=None):
    return {
        "scenario": {
            "Mutant": {
                "name": name or f"{file}:{line}:5: replace {function} -> bool with true",
                "file": file,
                "function": {"function_name": function},
                "span": {"start": {"line": line, "column": 5}},
            }
        },
        "summary": outcome,
    }


class Reading(unittest.TestCase):
    def document(self, *entries):
        return {"outcomes": [{"scenario": "Baseline", "summary": "Success"}, *entries]}

    def test_the_baseline_run_is_not_a_changed_program_whatever_it_says_happened(self):
        document = {"outcomes": [{"scenario": "Baseline", "summary": "MissedMutant"}]}
        self.assertEqual(list(mutants([document])), [])

    def test_each_outcome_is_counted_under_its_own_name_and_an_unknown_one_is_not_counted(self):
        document = self.document(
            mutant("CaughtMutant", "a.rs", "f", 1),
            mutant("MissedMutant", "a.rs", "f", 2),
            mutant("Timeout", "a.rs", "g", 3),
            mutant("Unviable", "a.rs", "g", 4),
            mutant("Failure", "a.rs", "g", 5),
        )
        self.assertEqual(
            summarize([document])["totals"], {"caught": 1, "missed": 1, "timeout": 1, "unviable": 1}
        )

    def test_the_score_is_the_caught_over_the_caught_and_missed_and_leaves_timeouts_out(self):
        document = self.document(
            *[mutant("CaughtMutant", "a.rs", "f", n) for n in range(3)],
            mutant("MissedMutant", "a.rs", "f", 9),
            *[mutant("Timeout", "a.rs", "f", 20 + n) for n in range(10)],
        )
        self.assertEqual(summarize([document])["score"], 0.75)
        self.assertIsNone(summarize([self.document(mutant("Timeout", "a.rs", "f", 1))])["score"])

    def test_shards_are_joined_into_one_report(self):
        first = self.document(mutant("MissedMutant", "a.rs", "f", 1), mutant("CaughtMutant", "a.rs", "f", 2))
        second = self.document(mutant("MissedMutant", "b.rs", "g", 1), mutant("MissedMutant", "b.rs", "g", 2))
        report = summarize([first, second])
        self.assertEqual(report["totals"], {"missed": 3, "caught": 1})
        self.assertEqual([(key, len(sites)) for key, sites in report["functions"]], [(("b.rs", "g"), 2), (("a.rs", "f"), 1)])

    def test_the_report_ranks_files_and_functions_by_what_was_missed_and_names_the_sites(self):
        document = self.document(
            *[mutant("MissedMutant", "zworst.rs", "worst", n, f"zworst.rs:{n}:5: replace < with <=") for n in range(1, 9)],
            mutant("MissedMutant", "asmall.rs", "one", 4),
            *[mutant("CaughtMutant", "asmall.rs", "one", 10 + n) for n in range(5)],
            mutant("CaughtMutant", "clean.rs", "fine", 4),
        )
        text = render(summarize([document]))
        self.assertLess(text.index("`zworst.rs` | 8"), text.index("`asmall.rs` | 1"))
        self.assertLess(text.index("- `worst` in `zworst.rs`: 8"), text.index("- `one` in `asmall.rs`: 1"))
        self.assertNotIn("`clean.rs`", text)
        self.assertIn("  - line 1: replace < with <=", text)
        self.assertIn("  - and 2 more", text)

    def test_a_run_that_tested_nothing_says_there_is_no_score(self):
        self.assertIn("no score", render(summarize([self.document()])))


class Running(unittest.TestCase):
    def test_a_shard_is_asked_for_with_its_package_and_nothing_else_by_default(self):
        self.assertEqual(
            command("bravebot-core", "1/6", 2, "out", ["--timeout", "60"]),
            ["cargo", "mutants", "--package", "bravebot-core", "--output", "out", "--shard", "1/6",
             "--jobs", "2", "--timeout", "60"],
        )
        self.assertEqual(
            command("bravebot-core", None, None, "out", []),
            ["cargo", "mutants", "--package", "bravebot-core", "--output", "out"],
        )

    def test_finding_missed_changes_or_timeouts_is_a_measurement_and_a_usage_error_or_a_failing_suite_is_not(self):
        self.assertEqual([measured(code) for code in (0, 2, 3)], [True, True, True])
        self.assertEqual([measured(code) for code in (1, 4, 101, -9)], [False, False, False, False])

    def test_a_report_of_a_file_that_is_not_outcomes_fails_and_one_of_none_fails(self):
        with tempfile.TemporaryDirectory() as temp:
            bad = Path(temp) / "bad.json"
            bad.write_text("not json")
            with self.assertRaises(SystemExit):
                report_command([str(bad)], None)
        with self.assertRaises(SystemExit):
            report_command([], None)

    def test_a_report_of_fewer_shards_than_were_measured_fails_rather_than_reporting_part_of_the_crate(self):
        with tempfile.TemporaryDirectory() as temp:
            outcomes = Path(temp) / "o.json"
            outcomes.write_text(json.dumps({"outcomes": []}))
            with self.assertRaises(SystemExit) as raised:
                report_command([str(outcomes)], None, shards=6)
            self.assertIn("6 shards", str(raised.exception))
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(report_command([str(outcomes)], None, shards=1), 0)

    def test_the_summary_is_appended_to_what_is_already_there(self):
        with tempfile.TemporaryDirectory() as temp:
            outcomes = Path(temp) / "o.json"
            outcomes.write_text(json.dumps({"outcomes": [mutant("MissedMutant", "a.rs", "f", 1)]}))
            summary = Path(temp) / "summary.md"
            summary.write_text("earlier\n")
            with contextlib.redirect_stdout(io.StringIO()):
                report_command([str(outcomes)], str(summary))
            text = summary.read_text()
        self.assertTrue(text.startswith("earlier\n# Mutation testing"))


def main():
    parser = argparse.ArgumentParser(description="Report the changes to the code that no test notices.")
    parser.add_argument("--selftest", action="store_true", help="check the report on known inputs")
    sub = parser.add_subparsers(dest="mode")
    runner = sub.add_parser("run", help="run cargo-mutants over one shard of a package")
    runner.add_argument("--package", required=True)
    runner.add_argument("--shard", help="K/N: run the Kth of N slices, counting from 0")
    runner.add_argument("--jobs", type=int, help="builds and test runs at once")
    runner.add_argument("--save", required=True, help="where to write the outcomes")
    runner.add_argument("extra", nargs="*", help="further arguments for cargo mutants, after --")
    reporter = sub.add_parser("report", help="join outcomes files and say what was missed")
    reporter.add_argument("files", nargs="*")
    reporter.add_argument("--summary", help="append the report to this file, such as $GITHUB_STEP_SUMMARY")
    reporter.add_argument("--shards", type=int, help="fail unless this many files are given")
    args = parser.parse_args()
    if args.selftest:
        result = unittest.TextTestRunner(stream=sys.stderr, verbosity=0).run(
            unittest.defaultTestLoader.loadTestsFromModule(sys.modules[__name__])
        )
        return 0 if result.wasSuccessful() else 1
    if args.mode == "run":
        return run(args.package, args.shard, args.jobs, args.extra, args.save)
    if args.mode == "report":
        return report_command(args.files, args.summary, args.shards)
    parser.error("name run or report, or --selftest")


if __name__ == "__main__":
    sys.exit(main())
