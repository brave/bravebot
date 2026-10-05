#!/usr/bin/env python3
"""The decisions peer-features.py makes without a model or the network."""

import argparse
import contextlib
import datetime
import importlib.util
import io
import json
import os
import re
# These tests run git with argument lists against temporary repositories.
import subprocess  # nosemgrep: gitlab.bandit.B404
import tempfile
import types
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("peer_features", HERE / "peer-features.py")
pf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pf)

os.environ.update(
    GIT_CONFIG_GLOBAL=os.devnull,
    GIT_CONFIG_NOSYSTEM="1",
    GIT_AUTHOR_NAME="selftest",
    GIT_AUTHOR_EMAIL="selftest@example.invalid",
    GIT_COMMITTER_NAME="selftest",
    GIT_COMMITTER_EMAIL="selftest@example.invalid",
)

EM_DASH = chr(0x2014)


def quiet(function, *args, **kwargs):
    with contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()):
        code = function(*args, **kwargs)
    return code, out.getvalue()


def gap(gid="parity-session-fork", **extra):
    kind = gid.split("-", 1)[0]
    return dict(
        {
            "id": gid,
            "kind": kind,
            "title": "Add session forking so a person can try a second approach without losing the first",
            "key": "forking",
            "area": "turns",
            "summary": f"bravebot cannot branch a session. A person who wants to try @someone's idea restarts.",
            "peer": "Codex",
            "sources": ["https://example.com/docs/fork"],
            "peer_behaviour": "`/fork` copies the session up to the current turn into a new one.",
            "quote": "Fork the current conversation",
            "bravebot_today": "Resuming a session continues it in place.",
            "evidence": ["docs/specs/sessions.md SESSION-3", "crates/x/src/lib.rs:12"],
            "proposal": "Add `/fork`. The copy keeps every label.",
            "delta": "Codex forks the whole session; fork from any earlier turn." if kind == "beyond" else "",
            "constraints": "",
            "existing_issue": None,
        },
        **extra,
    )


class Ledger(unittest.TestCase):
    def test_round_trip_is_sorted_one_line_and_keeps_the_header(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "ledger"
            entries = {
                "spec:HOOK": {"verdict": "reviewed", "issue": "-", "commit": "abc", "date": "2026-10-01", "reason": "2 candidates\n1 filed"},
                "parity-a-b": {"verdict": "filed", "issue": "#12", "commit": "abc", "date": "2026-10-01", "reason": f"x {EM_DASH} y"},
                "peer:claude-code": {"verdict": "reviewed", "issue": "-", "commit": "abc", "date": "2026-10-01", "reason": ""},
                "parity-c-d": {"verdict": "merged", "issue": "#12", "commit": "abc", "date": "2026-10-01", "reason": "into parity-a-b: one change"},
            }
            pf.write_ledger(path, entries)
            text = path.read_text()
            self.assertTrue(text.startswith(pf.HEADER))
            self.assertNotIn(EM_DASH, text)
            body = [line for line in text.splitlines() if not line.startswith("#")]
            self.assertEqual([line.split()[0] for line in body], ["parity-a-b", "parity-c-d", "peer:claude-code", "spec:HOOK"])
            back = pf.read_ledger(path)
            self.assertEqual(back["parity-a-b"]["issue"], "#12")
            self.assertEqual((back["parity-c-d"]["verdict"], back["parity-c-d"]["reason"]), ("merged", "into parity-a-b: one change"))
            self.assertEqual(back["spec:HOOK"]["reason"], "2 candidates 1 filed")

    def test_a_malformed_line_is_refused_rather_than_dropped(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "ledger"
            for line in (
                "spec:HOOK filed - abc 2026-10-01 x",
                "parity-a reviewed - abc 2026-10-01 x",
                "parity-A filed - abc 2026-10-01 x",
                "gap-a filed - abc 2026-10-01 x",
                "peer:Codex reviewed - abc 2026-10-01 x",
                "spec:HOOK reviewed - abc yesterday x",
                "spec:HOOK reviewed -",
            ):
                with self.subTest(line=line):
                    path.write_text(pf.HEADER + line + "\n")
                    with self.assertRaises(pf.Problem):
                        pf.read_ledger(path)


def write_spec(root, name, spec_id, title):
    path = root / "docs" / "specs" / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"---\nid: {spec_id}\ntitle: {title}\ngoverns:\n  - crates/x/src/{name}.rs\n---\n\n## Scope\n\nx\n\n### {spec_id}-1: one\n\nverified-by: none\n")


class Units(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        write_spec(self.root, "hooks.md", "HOOK", "Hooks")
        write_spec(self.root, "tools/run.md", "RUN", "The run tool")
        (self.root / "docs" / "specs" / "README.md").write_text("# Specs\n")

    def test_every_spec_and_every_tool_the_advisory_skill_watches_is_a_unit(self):
        units = pf.all_units(self.root)
        keys = [u["unit"] for u in units]
        self.assertEqual(keys[:2], ["spec:HOOK", "spec:RUN"])
        tools = {u["unit"]: u for u in units if u["kind"] == "peer"}
        self.assertEqual(len(tools), len({pf.slug(name) for _, name in pf.pa.REPOSITORIES} | {pf.slug(n) for _, _, n in pf.pa.PACKAGES}))
        self.assertEqual(tools["peer:claude-code"]["repo"], "anthropics/claude-code")
        self.assertEqual(tools["peer:github-copilot-cli"]["repo"], "github/copilot-cli")
        self.assertIsNone(tools["peer:aider"]["repo"])
        self.assertEqual(units[0]["path"], "docs/specs/hooks.md")
        self.assertEqual(units[1]["path"], "docs/specs/tools/run.md")
        self.assertEqual(units[0]["governs"], ["crates/x/src/hooks.md.rs"])

    def test_a_person_can_name_a_spec_or_a_tool_bare_or_prefixed(self):
        units = pf.all_units(self.root)
        named = pf.named_units(["HOOK", "codex", "peer:claude-code", "spec:RUN", "HOOK"], units)
        self.assertEqual([u["unit"] for u in named], ["spec:HOOK", "peer:codex", "peer:claude-code", "spec:RUN"])
        for bad in ("NOPE", "spec:NOPE", "peer:nobody"):
            with self.subTest(bad=bad):
                with self.assertRaises(pf.Problem):
                    pf.named_units([bad], units)

    def test_unreviewed_units_come_first_in_order_then_the_stalest_after_ninety_days(self):
        units = [{"unit": u} for u in ("spec:A", "spec:B", "spec:C", "peer:d", "peer:e")]
        today = datetime.date(2026, 10, 4)
        line = {"verdict": "reviewed", "issue": "-", "commit": "c", "reason": ""}
        ledger = {
            "spec:A": dict(line, date="2026-09-01"),
            "spec:C": dict(line, date="2026-01-01"),
            "peer:d": dict(line, date="2026-02-01"),
            "parity-x": {"verdict": "declined", "issue": "-", "commit": "c", "date": "2026-01-01", "reason": ""},
        }
        order = lambda limit: [u["unit"] for u in pf.select(units, ledger, [], limit, today)]
        self.assertEqual(order(10), ["spec:B", "peer:e", "spec:C", "peer:d"])
        self.assertEqual(order(2), ["spec:B", "peer:e"])
        ledger["spec:C"]["date"] = "2026-07-06"
        self.assertEqual(order(10), ["spec:B", "peer:e", "peer:d"])
        self.assertEqual([u["unit"] for u in pf.select(units, ledger, [units[0]], 1, today)], ["spec:A"])


class Templates(unittest.TestCase):
    def test_the_templates_name_only_values_the_script_supplies(self):
        shared = {"unit", "root", "commit", "ref", "today", "tracker", "known_gaps", "results_file", "max_gaps", "max_sources", "max_quote", "peers"}
        for name, supplied in (
            ("gaps.md", shared),
            ("research-spec.md", shared | {"spec_id", "spec_title", "spec_path", "governs", "gap_rules"}),
            ("research-peer.md", shared | {"peer_name", "peer_source", "gap_rules"}),
            ("verify.md", {"unit", "candidates", "root", "commit", "today", "tracker", "known_gaps", "results_file"}),
            ("merge.md", {"candidates", "issues", "results_file"}),
        ):
            with self.subTest(name=name):
                used = set(re.findall(r"\{\{(\w+)\}\}", (HERE / name).read_text()))
                self.assertLessEqual(used, supplied)
                self.assertIn("results_file" if name != "research-spec.md" and name != "research-peer.md" else "gap_rules", used)

    def test_no_template_or_skill_file_holds_an_em_dash(self):
        for path in HERE.glob("*"):
            if path.suffix in (".md", ".py"):
                with self.subTest(path=path.name):
                    self.assertNotIn(EM_DASH, path.read_text())


class Candidates(unittest.TestCase):
    def test_a_gap_that_cannot_be_drafted_is_refused_with_the_reason(self):
        for change, problem in (
            ({"kind": "security"}, "kind"),
            ({"id": "beyond-session-fork"}, "parity-"),
            ({"id": "Parity-Session"}, "lowercase"),
            ({"summary": " "}, "no summary"),
            ({"sources": []}, "sources"),
            ({"sources": ["http://example.com/x"]}, "https"),
            ({"sources": ["https://example.com/x y"]}, "https"),
            ({"sources": ["https://e.com/" + str(n) for n in range(6)]}, "sources"),
            ({"evidence": []}, "evidence"),
            ({"quote": "x" * 301}, "quote"),
            ({"area": "everywhere"}, "area"),
            ({"existing_issue": "12"}, "existing_issue"),
        ):
            with self.subTest(problem=problem, change=list(change)):
                self.assertIn(problem, pf.check_gap(gap(**change)))
        self.assertIsNone(pf.check_gap(gap()))

    def test_a_group_that_cannot_be_applied_is_refused_with_the_reason(self):
        confirmed = {"parity-a": None, "parity-b": None, "parity-c": None}
        for group, problem in (
            ("parity-a", "not an object"),
            ({"gaps": "parity-a", "reason": "r"}, "list of gap ids"),
            ({"gaps": [], "reason": "r"}, "list of gap ids"),
            ({"gaps": ["parity-a", "parity-z"], "reason": "r"}, "parity-z is not a gap"),
            ({"gaps": ["parity-a", "parity-a"], "reason": "r"}, "parity-a is in more than one group"),
            ({"gaps": ["parity-a", "parity-b"], "reason": "r"}, "parity-b is in more than one group"),
            ({"gaps": ["parity-a", "parity-c"], "existing_issue": 13, "reason": "r"}, "existing_issue 13"),
            ({"gaps": ["parity-a", "parity-c"], "existing_issue": "12", "reason": "r"}, "existing_issue '12'"),
            ({"gaps": ["parity-a"], "reason": "r"}, "one gap and no existing_issue"),
            ({"gaps": ["parity-a", "parity-c"]}, "no reason"),
        ):
            with self.subTest(problem=problem):
                self.assertIn(problem, pf.check_group(group, confirmed, {12}, {"parity-b"}))
        for group in ({"gaps": ["parity-a"], "existing_issue": 12, "reason": "r"}, {"gaps": ["parity-a", "parity-c"], "reason": "r"}):
            with self.subTest(group=group):
                self.assertIsNone(pf.check_group(group, confirmed, {12}, {"parity-b"}))

    def test_a_beyond_gap_must_say_what_it_beats(self):
        self.assertIsNone(pf.check_gap(gap("beyond-fork-anywhere")))
        self.assertIn("delta", pf.check_gap(gap("beyond-fork-anywhere", delta="")))

    def test_more_gaps_than_the_cap_are_cut_and_a_bad_one_discards_the_unit(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "r.json"
            many = [gap(f"parity-thing-{chr(97 + n)}") for n in range(pf.MAX_GAPS + 2)]
            path.write_text(json.dumps({"unit": "spec:HOOK", "gaps": many}))
            gaps, problem = pf.load_research(path, "spec:HOOK")
            self.assertEqual(len(gaps), pf.MAX_GAPS)
            self.assertIsNone(problem)
            self.assertEqual(pf.load_research(path, "spec:RUN")[1], "the result names another unit")
            path.write_text(json.dumps({"unit": "spec:HOOK", "gaps": [gap(), gap(), ]}))
            self.assertIn("used twice", pf.load_research(path, "spec:HOOK")[1])
            path.write_text(json.dumps({"unit": "spec:HOOK", "gaps": [gap(), gap("parity-b", sources=[])]}))
            self.assertIn("gap 2", pf.load_research(path, "spec:HOOK")[1])
            path.write_text("not json")
            self.assertEqual(pf.load_research(path, "spec:HOOK")[0], None)


class Work(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name) / "tree"
        (self.root / "docs").mkdir(parents=True)
        self.work = Path(temp.name) / "work"
        (self.work / "research").mkdir(parents=True)
        (self.work / "verify").mkdir()
        units = ["spec:HOOK", "peer:codex", "spec:RUN"]
        manifest = {
            "root": str(self.root),
            "ref": "upstream/main",
            "commit": "0123456789abcdef",
            "tracker": str(self.work / "tracker.tsv"),
            "known_gaps": str(self.work / "known-gaps.tsv"),
            "units": [
                {"unit": u, "prompt_file": "p", "results_file": str(self.work / "research" / f"{u.replace(':', '-')}.json")}
                for u in units
            ],
            "subjects": {
                "spec:HOOK": {"kind": "spec", "name": "HOOK", "path": "docs/specs/hooks.md"},
                "peer:codex": {"kind": "peer", "name": "Codex"},
                "spec:RUN": {"kind": "spec", "name": "RUN", "path": "docs/specs/tools/run.md"},
            },
        }
        (self.work / "manifest.json").write_text(json.dumps(manifest))

    def research(self, unit, *gaps):
        (self.work / "research" / f"{unit.replace(':', '-')}.json").write_text(json.dumps({"unit": unit, "gaps": list(gaps)}))

    def check(self, unit, *verdicts):
        entries = [dict({"reason": "checked"}, **v) for v in verdicts]
        (self.work / "verify" / f"{unit.replace(':', '-')}.json").write_text(json.dumps({"unit": unit, "gaps": entries}))

    def args(self, **extra):
        return argparse.Namespace(**dict({"work_dir": str(self.work), "dry_run": False}, **extra))

    def states(self, unit):
        info = pf.outcomes(self.work)[1][unit]
        return info["state"], {g: r["state"] for g, r in info["gaps"].items()}

    def test_a_unit_with_no_result_is_pending_and_one_with_no_gaps_needs_no_verifier(self):
        self.research("spec:RUN")
        self.assertEqual(self.states("spec:HOOK")[0], "pending")
        self.assertEqual(self.states("spec:RUN"), ("read", {}))
        code, out = quiet(pf.verify, self.args())
        self.assertEqual((code, json.loads(out)["verify"]), (0, []))

    def test_only_gaps_the_tracker_and_ledger_do_not_decide_go_to_a_verifier(self):
        ledger = self.root / pf.LEDGER
        pf.write_ledger(ledger, {"beyond-held-already": {"verdict": "declined", "issue": "-", "commit": "c", "date": "2026-01-01", "reason": "r"}})
        self.research(
            "spec:HOOK",
            gap("parity-session-fork"),
            gap("parity-on-tracker", existing_issue=40),
            gap("beyond-held-already"),
        )
        self.assertEqual(
            self.states("spec:HOOK")[1],
            {"parity-session-fork": "verify", "parity-on-tracker": "final", "beyond-held-already": "held"},
        )
        code, out = quiet(pf.verify, self.args())
        self.assertEqual([v["unit"] for v in json.loads(out)["verify"]], ["spec:HOOK"])
        prompt = (self.work / "verify" / "spec-HOOK.md").read_text()
        self.assertIn("parity-session-fork", prompt)
        self.assertNotIn("parity-on-tracker", prompt)
        self.assertNotIn("{{", prompt)

    def test_the_verifier_decides_each_gap(self):
        self.research("spec:HOOK", *(gap(f"parity-{n}") for n in ("a", "b", "c", "d", "e")))
        self.check(
            "spec:HOOK",
            {"id": "parity-a", "verdict": "confirmed"},
            {"id": "parity-b", "verdict": "covered"},
            {"id": "parity-c", "verdict": "declined"},
            {"id": "parity-d", "verdict": "unsupported"},
            {"id": "parity-e", "verdict": "tracked", "existing_issue": 7},
        )
        self.assertEqual(
            self.states("spec:HOOK")[1],
            {"parity-a": "file", "parity-b": "final", "parity-c": "final", "parity-d": "dropped", "parity-e": "final"},
        )

    def test_a_gap_the_verifier_skipped_stays_pending(self):
        self.research("spec:HOOK", gap("parity-a"), gap("parity-b"))
        self.check("spec:HOOK", {"id": "parity-a", "verdict": "confirmed"})
        self.assertEqual(self.states("spec:HOOK")[1], {"parity-a": "file", "parity-b": "pending"})

    def test_a_verdict_file_that_cannot_be_read_leaves_the_gaps_waiting(self):
        self.research("spec:HOOK", gap())
        for verdicts in (
            [{"id": "parity-session-fork", "verdict": "fine"}],
            [{"id": "parity-session-fork", "verdict": "tracked"}],
            [{"id": "parity-session-fork", "verdict": "confirmed", "reason": ""}],
            [{"verdict": "confirmed"}],
        ):
            with self.subTest(verdicts=verdicts):
                (self.work / "verify" / "spec-HOOK.json").write_text(json.dumps({"unit": "spec:HOOK", "gaps": verdicts}))
                self.assertEqual(self.states("spec:HOOK")[1], {"parity-session-fork": "verify"})

    def confirmed(self, *gaps, unit="spec:HOOK"):
        self.research(unit, *gaps)
        self.check(unit, *({"id": g["id"], "verdict": "confirmed"} for g in gaps))

    def grouped(self, *groups, issues=()):
        """The merge step's files for the gaps confirmed so far, holding these groups."""
        folder = self.work / "merge"
        folder.mkdir(exist_ok=True)
        confirmed = list(pf.confirmed_gaps(pf.outcomes(self.work)[1]))
        (folder / "asked.json").write_text(json.dumps({"gaps": confirmed, "issues": list(issues)}))
        (folder / "groups.json").write_text(json.dumps({"groups": [dict({"reason": "One change closes both."}, **g) for g in groups]}))

    def test_a_draft_is_labelled_by_kind_has_no_em_dash_pings_nobody_and_cites_its_id(self):
        self.confirmed(
            gap(summary=f"bravebot {EM_DASH} cannot fork. Ask @someone, or `@kept`.", proposal=f"Add it {EM_DASH} carefully. {self.root}/x"),
            gap("beyond-fork-anywhere", area="interface", constraints="SESSION-3 applies."),
        )
        self.grouped()
        code, _ = quiet(pf.draft, self.args())
        self.assertEqual(code, 0)
        drafts = {d["id"]: d for d in json.loads((self.work / "drafts.json").read_text())}
        self.assertEqual(drafts["parity-session-fork"]["labels"], ["area/turns", "parity"])
        self.assertEqual(drafts["beyond-fork-anywhere"]["labels"], ["area/interface", "beyond-parity", "enhancement"])
        self.assertEqual(drafts["parity-session-fork"]["key"], "forking")
        body = Path(drafts["parity-session-fork"]["body_file"]).read_text()
        self.assertNotIn(EM_DASH, body)
        self.assertIn("`@someone`", body)
        self.assertIn("`@kept`", body)
        self.assertNotIn("``@", body)
        self.assertNotIn(str(self.root), body)
        self.assertIn("- <https://example.com/docs/fork>", body)
        self.assertIn("> Fork the current conversation", body)
        self.assertIn("## What Codex does", body)
        self.assertIn("Read: `docs/specs/sessions.md SESSION-3`, `crates/x/src/lib.rs:12`", body)
        self.assertIn("`docs/specs/hooks.md`", body)
        self.assertIn("Gap id: `parity-session-fork`", body)
        self.assertNotIn("Where it goes past parity", body)
        beyond = Path(drafts["beyond-fork-anywhere"]["body_file"]).read_text()
        self.assertIn("## Where it goes past parity\n\nCodex forks the whole session", beyond)
        self.assertIn("## Constraints\n\nSESSION-3 applies.", beyond)

    def test_a_tool_review_names_the_tool_as_the_source_and_a_gap_found_twice_is_drafted_once(self):
        self.confirmed(gap(), unit="spec:HOOK")
        self.confirmed(gap(), unit="peer:codex")
        self.grouped()
        quiet(pf.draft, self.args())
        drafts = json.loads((self.work / "drafts.json").read_text())
        self.assertEqual([d["unit"] for d in drafts], ["spec:HOOK"])
        self.assertEqual(len(list((self.work / "issues").iterdir())), 1)

    def test_merge_lists_each_confirmed_gap_once_with_every_parity_and_beyond_parity_issue(self):
        calls = []

        def gh(args):
            calls.append(args)
            fork = {"number": 12, "state": "OPEN", "title": "Add forking", "body": "bravebot cannot fork.\n\n## What Codex does\n\nx"}
            older = {"number": 3, "state": "CLOSED", "title": "Beyond x", "body": None}
            return json.dumps({"parity": [fork], "beyond-parity": [fork, older]}[args[args.index("--label") + 1]])

        with mock.patch.object(pf.pa, "gh", gh):
            code, out = quiet(pf.merge, self.args())
            self.assertEqual((code, json.loads(out)["merge"], calls), (0, [], []))

            self.confirmed(gap(), gap("parity-loop-detection", title="Stop a turn that repeats one call"), unit="spec:HOOK")
            self.confirmed(gap(), unit="peer:codex")
            self.research("spec:RUN", gap("parity-unverified"))
            code, out = quiet(pf.merge, self.args())
        self.assertEqual(code, 0)
        self.assertEqual(sorted(c[c.index("--label") + 1] for c in calls), ["beyond-parity", "parity"])
        self.assertEqual({c[c.index("--state") + 1] for c in calls}, {"all"})
        prompt = Path(json.loads(out)["merge"][0]["prompt_file"]).read_text()
        self.assertEqual(prompt.count('"id": "parity-session-fork"'), 1)
        self.assertIn('"title": "Stop a turn that repeats one call"', prompt)
        self.assertNotIn("parity-unverified", prompt)
        self.assertIn("#3 closed: Beyond x\n#12 open: Add forking | bravebot cannot fork.\n", prompt)
        self.assertNotIn("What Codex does", prompt)
        self.assertNotIn("{{", prompt)
        asked = json.loads((self.work / "merge" / "asked.json").read_text())
        self.assertEqual(asked, {"gaps": ["parity-session-fork", "parity-loop-detection"], "issues": [3, 12]})

        def down(args):
            raise RuntimeError("HTTP 502")

        with mock.patch.object(pf.pa, "gh", down):
            self.assertEqual(quiet(pf.main, ["merge", "--work-dir", str(self.work)])[0], 2)

    def test_a_group_is_drafted_as_one_issue_that_says_what_each_other_review_adds(self):
        self.confirmed(gap(), unit="spec:HOOK")
        self.confirmed(
            gap(
                "parity-fork-conversation",
                peer_behaviour="`/branch` starts a copy of the conversation.",
                sources=["https://example.com/docs/fork", "https://example.org/branch"],
                proposal="Add `/branch`.",
            ),
            unit="peer:codex",
        )
        self.confirmed(gap("parity-image-paste", title="Paste an image into the prompt"), unit="spec:RUN")
        self.grouped({"gaps": ["parity-session-fork", "parity-fork-conversation"], "reason": "Both ask for one `/fork` command."})
        code, out = quiet(pf.draft, self.args())
        self.assertEqual(code, 0)
        drafts = json.loads((self.work / "drafts.json").read_text())
        self.assertEqual([(d["id"], d["merged"]) for d in drafts], [("parity-session-fork", ["parity-fork-conversation"]), ("parity-image-paste", [])])
        self.assertEqual(sorted(p.name for p in (self.work / "issues").iterdir()), ["parity-image-paste.md", "parity-session-fork.md"])
        self.assertIn("also parity-fork-conversation", out)
        body = Path(drafts[0]["body_file"]).read_text()
        self.assertIn("## Also found by\n\nOther reviews in this run found the same change. Both ask for one `/fork` command.", body)
        self.assertIn(
            "### What Codex does, from the Codex documentation\n\n`/branch` starts a copy of the conversation.\n\n"
            "Sources:\n\n- <https://example.org/branch>\n\nIts proposal:\n\nAdd `/branch`.\n\nGap id: `parity-fork-conversation`",
            body,
        )
        self.assertEqual(body.count("https://example.com/docs/fork"), 1)
        self.assertLess(body.index("## Also found by"), body.index("## Where this comes from"))
        self.assertTrue(body.endswith("Gap id: `parity-session-fork`\n"))
        alone = Path(drafts[1]["body_file"]).read_text()
        self.assertNotIn("Also found by", alone)

    def test_a_group_an_issue_already_asks_for_files_nothing_and_is_recorded_tracked_and_merged(self):
        self.confirmed(gap(), gap("parity-fork-conversation"))
        self.grouped({"gaps": ["parity-session-fork", "parity-fork-conversation"], "existing_issue": 12, "reason": "#12 asks for `/fork`."}, issues=[12])
        quiet(pf.draft, self.args())
        self.assertEqual(json.loads((self.work / "drafts.json").read_text()), [])
        quiet(pf.record, self.args(), today="2026-10-04")
        entries = pf.read_ledger(self.root / pf.LEDGER)
        self.assertEqual(
            {g: (entries[g]["verdict"], entries[g]["issue"], entries[g]["reason"]) for g in ("parity-session-fork", "parity-fork-conversation")},
            {
                "parity-session-fork": ("tracked", "#12", "#12 asks for `/fork`."),
                "parity-fork-conversation": ("merged", "#12", "into parity-session-fork: #12 asks for `/fork`."),
            },
        )
        self.assertEqual(entries["spec:HOOK"]["reason"], "2 candidates, 1 merged, 1 tracked")

    def test_a_merged_gap_is_recorded_once_the_gap_it_joined_has_an_issue(self):
        self.confirmed(gap(), unit="spec:HOOK")
        self.confirmed(gap("parity-fork-conversation"), unit="peer:codex")
        self.grouped({"gaps": ["parity-session-fork", "parity-fork-conversation"]})
        _, out = quiet(pf.record, self.args(), today="2026-10-04")
        self.assertFalse((self.root / pf.LEDGER).exists())
        self.assertIn("left   parity-fork-conversation  merged into parity-session-fork, which is not filed", out)

        (self.work / "filed.json").write_text(json.dumps({"parity-session-fork": {"issue": 99, "how": "filed"}}))
        quiet(pf.record, self.args(), today="2026-10-04")
        entries = pf.read_ledger(self.root / pf.LEDGER)
        self.assertEqual((entries["parity-session-fork"]["verdict"], entries["parity-session-fork"]["issue"]), ("filed", "#99"))
        self.assertEqual(
            (entries["parity-fork-conversation"]["verdict"], entries["parity-fork-conversation"]["issue"], entries["parity-fork-conversation"]["reason"]),
            ("merged", "#99", "into parity-session-fork: One change closes both."),
        )
        self.assertEqual(entries["peer:codex"]["reason"], "1 candidates, 1 merged")
        self.assertEqual(self.states("peer:codex")[1], {"parity-fork-conversation": "held"})

    def test_draft_refuses_while_the_merge_is_missing_stale_or_unreadable(self):
        self.confirmed(gap(), gap("parity-fork-conversation"))
        with self.assertRaisesRegex(pf.Problem, "2 confirmed gaps are not drafted: merge has not run"):
            quiet(pf.draft, self.args())
        self.assertFalse((self.work / "drafts.json").exists())

        self.grouped({"gaps": ["parity-session-fork"]})
        with self.assertRaisesRegex(pf.Problem, "group 1: one gap and no existing_issue"):
            quiet(pf.draft, self.args())

        self.grouped()
        self.confirmed(gap("parity-image-paste"), unit="peer:codex")
        with self.assertRaisesRegex(pf.Problem, "another set of confirmed gaps"):
            quiet(pf.draft, self.args())
        self.assertEqual(quiet(pf.main, ["draft", "--work-dir", str(self.work)])[0], 2)

        self.grouped()
        (self.work / "merge" / "groups.json").write_text("not json")
        with self.assertRaisesRegex(pf.Problem, "holds no readable groups"):
            quiet(pf.draft, self.args())
        self.assertFalse((self.work / "drafts.json").exists())

        self.grouped()
        self.assertEqual(quiet(pf.draft, self.args())[0], 0)
        self.assertEqual(len(json.loads((self.work / "drafts.json").read_text())), 3)

    def test_a_dropped_gap_or_a_decided_one_is_not_drafted_and_unfinished_ones_are_named(self):
        self.research("spec:HOOK", gap("parity-a"), gap("parity-b"), gap("parity-c"))
        self.check("spec:HOOK", {"id": "parity-a", "verdict": "unsupported"}, {"id": "parity-b", "verdict": "covered"})
        code, out = quiet(pf.draft, self.args())
        self.assertEqual(json.loads((self.work / "drafts.json").read_text()), [])
        self.assertIn("0 drafted", out)
        self.assertIn("spec:HOOK:parity-c", out)
        self.assertIn("spec:RUN", out)

    def test_record_marks_a_unit_reviewed_only_when_every_gap_in_it_is_decided_and_filed(self):
        ledger = self.root / pf.LEDGER
        self.research("spec:HOOK", gap("parity-a"), gap("parity-b"), gap("beyond-c"))
        self.check(
            "spec:HOOK",
            {"id": "parity-a", "verdict": "confirmed"},
            {"id": "parity-b", "verdict": "declined", "reason": "LABEL-3 forbids it"},
            {"id": "beyond-c", "verdict": "unsupported"},
        )
        self.research("spec:RUN")

        quiet(pf.record, self.args(dry_run=True), today="2026-10-04")
        self.assertFalse(ledger.exists())

        quiet(pf.record, self.args(), today="2026-10-04")
        entries = pf.read_ledger(ledger)
        self.assertEqual(sorted(entries), ["parity-b", "spec:RUN"])
        self.assertEqual((entries["parity-b"]["verdict"], entries["parity-b"]["reason"]), ("declined", "LABEL-3 forbids it"))
        self.assertEqual(entries["spec:RUN"]["commit"], "0123456789ab")

        (self.work / "filed.json").write_text(json.dumps({"parity-a": {"issue": 99, "how": "filed"}}))
        quiet(pf.record, self.args(), today="2026-10-05")
        entries = pf.read_ledger(ledger)
        self.assertEqual((entries["parity-a"]["verdict"], entries["parity-a"]["issue"]), ("filed", "#99"))
        self.assertEqual(entries["spec:HOOK"]["reason"], "3 candidates, 1 already decided, 1 dropped, 1 filed")
        self.assertEqual(entries["spec:HOOK"]["date"], "2026-10-05")
        self.assertNotIn("peer:codex", entries)

    def test_a_gap_the_tracker_already_held_is_recorded_as_tracked(self):
        self.confirmed(gap())
        (self.work / "filed.json").write_text(json.dumps({"parity-session-fork": {"issue": 5, "how": "existing"}}))
        quiet(pf.record, self.args(), today="2026-10-04")
        entries = pf.read_ledger(self.root / pf.LEDGER)
        self.assertEqual((entries["parity-session-fork"]["verdict"], entries["parity-session-fork"]["issue"]), ("tracked", "#5"))

    def test_a_later_review_does_not_reopen_a_decided_gap(self):
        ledger = self.root / pf.LEDGER
        pf.write_ledger(ledger, {"parity-session-fork": {"verdict": "declined", "issue": "-", "commit": "c", "date": "2026-01-01", "reason": "the rule"}})
        self.research("spec:HOOK", gap())
        self.assertEqual(self.states("spec:HOOK")[1], {"parity-session-fork": "held"})
        quiet(pf.record, self.args(), today="2026-10-04")
        entries = pf.read_ledger(ledger)
        self.assertEqual(entries["parity-session-fork"]["reason"], "the rule")
        self.assertEqual(entries["spec:HOOK"]["reason"], "1 candidates, 1 already decided")

    def poster(self, cited=(), labels=None):
        calls = []

        def gh(args, repo=None):
            calls.append(args)
            query = args[args.index("--search") + 1]
            return json.dumps([{"number": 5, "title": "t", "state": "OPEN", "url": "u"}] if any(g in query for g in cited) else [])

        def post(repo, draft, assignee=None):
            calls.append(["create", draft["id"]])
            return f"https://github.com/brave/bravebot/issues/{100 + len(calls)}"

        return calls, types.SimpleNamespace(
            gh=gh,
            post=post,
            existing_labels=lambda repo: set(labels or ["parity", "enhancement", "beyond-parity", "area/turns", "area/interface"]),
            assignable=lambda repo, login: True,
            already_filed=lambda repo, draft: None,
            JITTER=(0.0, 0.0),
        )

    def post_args(self, **extra):
        return self.args(**dict({"repo": "brave/bravebot", "assignee": None, "max": 12, "pace": 0.0}, **extra))

    def test_post_skips_a_gap_the_tracker_already_cites_and_a_missing_label_stops_everything(self):
        self.confirmed(gap(), gap("beyond-fork-anywhere", area="interface"))
        self.grouped()
        quiet(pf.draft, self.args())
        calls, poster = self.poster(cited=["parity-session-fork"], labels=["parity"])
        code, _ = quiet(pf.pa.post, self.post_args(), poster=poster)
        self.assertEqual((code, calls), (2, []))

        calls, poster = self.poster(cited=["parity-session-fork"])
        code, _ = quiet(pf.pa.post, self.post_args(), poster=poster)
        self.assertEqual(code, 0)
        self.assertIn('"parity-session-fork" in:body', calls[0][calls[0].index("--search") + 1])
        self.assertEqual([c for c in calls if c[0] == "create"], [["create", "beyond-fork-anywhere"]])
        filed = json.loads((self.work / "filed.json").read_text())
        self.assertEqual(filed["parity-session-fork"], {"issue": 5, "how": "existing"})
        self.assertEqual(filed["beyond-fork-anywhere"]["how"], "filed")

    def test_post_files_a_hundred_by_default_and_names_the_drafts_past_the_cap(self):
        """A confirmed gap left unfiled keeps its unit unreviewed for the next run, and a runaway run must still stop."""
        drafts = [{"id": f"parity-gap-{n}", "title": f"gap {n}", "labels": ["parity"]} for n in range(101)]
        (self.work / "drafts.json").write_text(json.dumps(drafts))
        calls, poster = self.poster()
        with mock.patch.object(pf.pa, "load_poster", lambda: poster):
            code, out = quiet(pf.main, ["post", "--work-dir", str(self.work), "--pace", "0"])
        self.assertEqual(code, 0)
        self.assertEqual([c[1] for c in calls if c[0] == "create"], [d["id"] for d in drafts[:100]])
        self.assertEqual(out.splitlines()[-1], f"1 not attempted, at the cap of 100: {drafts[100]['id']}")
        self.assertNotIn(drafts[100]["id"], json.loads((self.work / "filed.json").read_text()))


class Tree(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        self.git("init", "-q", "-b", "main")
        write_spec(self.root, "hooks.md", "HOOK", "Hooks")
        self.git("add", ".")
        self.git("commit", "-q", "-m", "base")

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.root), *args], check=True, capture_output=True, text=True).stdout.strip()

    def test_main_plus_the_ledger_is_comparable_and_a_tree_that_differs_is_refused(self):
        tip = self.git("rev-parse", "main")
        self.git("checkout", "-q", "-b", "work")
        (self.root / pf.LEDGER).write_text(pf.HEADER)
        self.assertEqual(pf.checked_commit(self.root), ("main", tip))
        (self.root / "docs" / "specs" / "hooks.md").write_text("edited\n")
        with self.assertRaisesRegex(pf.Problem, "hooks.md"):
            pf.checked_commit(self.root)

    def test_pending_writes_a_prompt_per_unit_from_the_real_templates(self):
        pf.pa.tracker = lambda path: path.write_text("12\topen\tAdd session forking\n") or 1
        write_ledger = {"peer:codex": {"verdict": "reviewed", "issue": "-", "commit": "c", "date": datetime.date.today().isoformat(), "reason": ""},
                        "parity-old-thing": {"verdict": "declined", "issue": "-", "commit": "c", "date": "2026-01-01", "reason": "LABEL-3"}}
        pf.write_ledger(self.root / pf.LEDGER, write_ledger)
        code, out = quiet(pf.main, ["pending", "--root", str(self.root), "--max", "3", "HOOK", "peer:claude-code", "aider"])
        self.assertEqual(code, 0)
        research = json.loads(out)["research"]
        self.assertEqual([r["unit"] for r in research], ["spec:HOOK", "peer:claude-code", "peer:aider"])
        work = Path(json.loads(out)["work_dir"])
        spec_prompt = Path(research[0]["prompt_file"]).read_text()
        self.assertIn("`docs/specs/hooks.md`", spec_prompt)
        self.assertIn("`crates/x/src/hooks.md.rs`", spec_prompt)
        self.assertIn("- Claude Code: https://github.com/anthropics/claude-code", spec_prompt)
        self.assertIn(str(work / "research" / "spec-HOOK.json"), spec_prompt)
        self.assertNotIn("{{", spec_prompt)
        self.assertEqual((work / "known-gaps.tsv").read_text(), "parity-old-thing\tdeclined\t-\tLABEL-3\n")
        self.assertIn("12\topen\tAdd session forking", (work / "tracker.tsv").read_text())
        peer_prompt = Path(research[1]["prompt_file"]).read_text()
        self.assertIn("https://github.com/anthropics/claude-code", peer_prompt)
        self.assertNotIn("{{", peer_prompt)
        self.assertIn("none known", Path(research[2]["prompt_file"]).read_text())
        manifest = json.loads((work / "manifest.json").read_text())
        self.assertEqual(manifest["subjects"]["spec:HOOK"]["path"], "docs/specs/hooks.md")

    def test_pending_refuses_a_name_that_is_neither_a_spec_nor_a_tool(self):
        code, _ = quiet(pf.main, ["pending", "--root", str(self.root), "NOPE"])
        self.assertEqual(code, 2)


if __name__ == "__main__":
    unittest.main()
