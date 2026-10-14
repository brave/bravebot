#!/usr/bin/env python3
"""The decisions session-review.py makes without a model or the network."""

import argparse
import contextlib
import importlib.util
import io
import json
import os
import re
import stat
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("session_review", HERE / "session-review.py")
sr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sr)

NOW = 1_800_000_000
IDLE = 30 * 60


def quiet(function, *args, **kwargs):
    with contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()):
        code = function(*args, **kwargs)
    return code, out.getvalue()


def session(sid, turns=3, age=3600, marker=None, **extra):
    return dict(
        {"id": sid, "turns": turns, "updated": NOW - age, "marker": marker, "workspace": "/w", "build": "1.0", "directory": "/d"},
        **extra,
    )


class Keys(unittest.TestCase):
    def test_a_working_directory_is_named_the_way_the_session_store_names_it(self):
        """crates/agent/src/home.rs key_for: anything but a letter, digit, dot or underscore becomes a dash."""
        self.assertEqual(sr.key_for(Path("/Users/me/my project/x_y.z")), "-Users-me-my-project-x_y.z")
        self.assertEqual(sr.key_for(Path("/")), "root")


class Choosing(unittest.TestCase):
    def ids(self, found, named=(), limit=8):
        return [(s["id"], s["from_turn"]) for s in sr.select(found, list(named), limit, NOW, IDLE)]

    def test_a_session_is_offered_until_reviewed_and_again_only_for_the_turns_it_has_grown_by(self):
        found = [
            session("new"),
            session("done", turns=3, marker={"turns": 3}),
            session("grown", turns=5, marker={"turns": 3}),
        ]
        self.assertEqual(sorted(self.ids(found)), [("grown", 3), ("new", 0)])

    def test_sessions_that_say_nothing_about_bravebot_or_may_still_be_running_are_passed_over(self):
        found = [
            session("claude-code-abc"),
            session("empty", turns=0),
            session("running", age=60),
            session("ok", age=IDLE + 1),
        ]
        self.assertEqual(self.ids(found), [("ok", 0)])

    def test_the_newest_come_first_and_the_limit_keeps_the_newest(self):
        found = [session("old", age=9000), session("newest", age=4000), session("middle", age=6000)]
        self.assertEqual([i for i, _ in self.ids(found, limit=2)], ["newest", "middle"])

    def test_a_named_session_is_reviewed_from_its_first_turn_whatever_its_marker_or_age(self):
        found = [session("2026-a1", marker={"turns": 3}, age=10), session("2026-b2")]
        self.assertEqual(self.ids(found, ["2026-a"]), [("2026-a1", 0)])

    def test_a_name_that_matches_nothing_or_several_or_a_session_without_turns_is_refused(self):
        found = [session("2026-a1"), session("2026-a2"), session("empty", turns=0)]
        for name, text in (("nope", "no session"), ("2026-a", "matches 2"), ("empty", "no turns")):
            with self.subTest(name=name):
                with self.assertRaisesRegex(sr.Problem, text):
                    sr.select(found, [name], 8, NOW, IDLE)

    def test_an_exact_id_wins_over_a_longer_id_it_prefixes(self):
        found = [session("2026-a"), session("2026-a1")]
        self.assertEqual(self.ids(found, ["2026-a"]), [("2026-a", 0)])


class Store(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.store = Path(temp.name)
        self.directory = self.store / "-w-project"
        self.directory.mkdir()

    def record(self, sid, **fields):
        (self.directory / f"{sid}.json").write_text(json.dumps(dict({"id": sid, "updated": 5, "turns": 2, "directory": "/w/project"}, **fields)))

    def test_only_session_records_are_candidates_and_a_marker_file_is_not_one(self):
        self.record("s1")
        sr.write_marker(self.directory, "s1", {"turns": 2})
        (self.directory / "s1.audit.jsonl").write_text("{}\n")
        (self.directory / "notes.json").write_text("[1]")
        (self.directory / "bad.json").write_text("{not json")
        found = sr.candidates(self.store)
        self.assertEqual([s["id"] for s in found], ["s1"])
        self.assertEqual(found[0]["marker"]["turns"], 2)

    def test_a_working_directory_limits_the_search_to_its_own_sessions(self):
        self.record("s1")
        other = self.store / "-elsewhere"
        other.mkdir()
        (other / "s2.json").write_text(json.dumps({"id": "s2", "updated": 5, "turns": 1}))
        self.assertEqual(sorted(s["id"] for s in sr.candidates(self.store)), ["s1", "s2"])
        self.assertEqual([s["id"] for s in sr.candidates(self.store, Path("/w/project"))], ["s1"])

    def test_the_marker_is_private_and_each_review_adds_a_line_the_newest_of_which_is_read(self):
        sr.write_marker(self.directory, "s1", {"turns": 2})
        sr.write_marker(self.directory, "s1", {"turns": 5})
        path = self.directory / f"s1{sr.MARKER_SUFFIX}"
        self.assertEqual(len(path.read_text().splitlines()), 2)
        self.assertEqual(sr.read_marker(self.directory, "s1")["turns"], 5)
        if os.name == "posix":
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)

    def test_a_damaged_marker_line_is_skipped_for_the_one_before_it(self):
        sr.write_marker(self.directory, "s1", {"turns": 2})
        with open(self.directory / f"s1{sr.MARKER_SUFFIX}", "a") as out:
            out.write("{broken\n")
        self.assertEqual(sr.read_marker(self.directory, "s1")["turns"], 2)

    def test_the_marker_suffix_is_not_one_the_session_store_reads_as_a_record(self):
        self.assertFalse(sr.MARKER_SUFFIX.endswith(".json"))


PROBLEM = {
    "title": "The resume picker lists a session twice \u2014 so choosing one opens the other",
    "key": "resume",
    "kind": "bug",
    "area": "interface",
    "security": False,
    "turn": 4,
    "what_happens": "Opened in /w/project and ~/work by @someone, the list shows it twice.",
    "code_path": "`crates/session/src/sessions.rs` line 2181",
    "reproduce": "Run it in /home/me/dir with two sessions.",
    "fix": "Deduplicate by id.",
    "evidence": "turn 4",
}


class Work(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.temp = Path(temp.name)
        self.work = self.temp / "work"
        for name in ("analyze", "verify", "sessions"):
            (self.work / name).mkdir(parents=True)
        self.directories = {}
        for sid in ("a", "b", "c"):
            self.directories[sid] = self.temp / f"store-{sid}"
            self.directories[sid].mkdir()
        manifest = {
            "root": str(self.temp / "tree"),
            "ref": "upstream/main",
            "commit": "0123456789abcdef",
            "sessions": [
                {
                    "id": sid,
                    "directory": str(self.directories[sid]),
                    "turns": 6,
                    "from_turn": 2,
                    "workspace": "/w/project",
                    "build": "1.0",
                    "prompt_file": str(self.work / "analyze" / f"{sid}.md"),
                    "results_file": str(self.work / "analyze" / f"{sid}.json"),
                }
                for sid in ("a", "b", "c")
            ],
        }
        (self.work / "manifest.json").write_text(json.dumps(manifest))

    def args(self, **extra):
        return argparse.Namespace(**dict({"work_dir": str(self.work), "dry_run": False}, **extra))

    def analysis(self, sid, problems, **extra):
        (self.work / "analyze" / f"{sid}.json").write_text(json.dumps(dict({"session": sid, "summary": "s", "problems": problems}, **extra)))

    def check(self, pid, **result):
        (self.work / "verify" / f"{pid}.json").write_text(json.dumps(dict({"problem": pid, "reason": "checked"}, **result)))

    def states(self):
        return {sid: [p["state"] for p in s["problems"]] if s["state"] == "read" else s["state"] for sid, s in sr.outcomes(self.work)[1].items()}

    def test_a_result_that_cannot_be_drafted_leaves_the_session_or_problem_undecided(self):
        self.analysis("a", [dict(PROBLEM, kind="security")])
        self.assertEqual(self.states()["a"], ["pending"])
        self.analysis("a", [dict(PROBLEM, turn=0)])
        self.assertEqual(self.states()["a"], ["pending"])
        self.analysis("a", [dict(PROBLEM, evidence="")])
        self.assertEqual(self.states()["a"], ["pending"])
        self.analysis("a", [PROBLEM] * (sr.MAX_PROBLEMS + 1))
        self.assertEqual(self.states()["a"], "pending")
        self.analysis("a", [], session="b")
        self.assertEqual(self.states()["a"], "pending")
        (self.work / "analyze" / "a.json").write_text("{broken")
        self.assertEqual(self.states()["a"], "pending")

    def test_only_a_confirmed_problem_with_no_issue_on_the_tracker_is_filed(self):
        self.analysis("a", [PROBLEM, PROBLEM, PROBLEM, PROBLEM])
        self.check("a-p1", verdict="confirmed")
        self.check("a-p2", verdict="fixed")
        self.check("a-p3", verdict="known", existing_issue=40)
        self.assertEqual(self.states()["a"], ["file", "final", "final", "verify"])
        self.assertEqual(sr.outcomes(self.work)[1]["a"]["problems"][2]["issue"], 40)
        self.check("a-p4", verdict="confirmed", existing_issue=41)
        self.assertEqual(self.states()["a"][3], "final")

    def test_a_security_concern_is_held_and_never_sent_to_a_verifier_or_drafted(self):
        self.analysis("a", [dict(PROBLEM, security=True)])
        self.assertEqual(self.states()["a"], ["held"])
        code, out = quiet(sr.verify, self.args())
        self.assertEqual(json.loads(out)["verify"], [])
        quiet(sr.draft, self.args())
        self.assertEqual(json.loads((self.work / "drafts.json").read_text()), [])

    def test_a_verifier_is_given_the_claim_and_the_session_and_nothing_else_the_template_lacks(self):
        self.analysis("a", [PROBLEM])
        (self.work / "sessions" / "a.json").write_text("{}")
        code, out = quiet(sr.verify, self.args())
        self.assertEqual([v["id"] for v in json.loads(out)["verify"]], ["a-p1"])
        prompt = (self.work / "verify" / "a-p1.md").read_text()
        self.assertIn(str(self.work / "sessions" / "a.json"), prompt)
        self.assertIn(str(self.work / "verify" / "a-p1.json"), prompt)
        self.assertIn("resume picker", prompt)
        self.assertNotIn("{{", prompt)

    def test_a_draft_has_no_em_dash_pings_nobody_and_names_neither_the_machine_nor_the_project(self):
        self.analysis("a", [PROBLEM])
        self.check("a-p1", verdict="confirmed")
        manifest = sr.pa.manifest_of(self.work)
        manifest["sessions"][0]["workspace"] = "/w/project"
        (self.work / "manifest.json").write_text(json.dumps(manifest))
        with mock_home("/home/me"):
            quiet(sr.draft, self.args())
        drafts = json.loads((self.work / "drafts.json").read_text())
        self.assertEqual(len(drafts), 1)
        self.assertEqual(drafts[0]["labels"], ["area/interface", "bug", "session-analysis"])
        self.assertNotIn("\u2014", drafts[0]["title"])
        body = Path(drafts[0]["body_file"]).read_text()
        self.assertNotIn("\u2014", body)
        self.assertIn("`@someone`", body)
        for private in ("/w/project", "/home/me"):
            self.assertNotIn(private, body)
        self.assertIn("<workspace>", body)
        self.assertIn("turn 4", body)
        self.assertIn("`0123456789ab`", body)
        self.assertIn("a-p1", body)

    def test_no_axis_label_and_no_security_label_is_ever_drafted(self):
        for kind in ("bug", "enhancement"):
            for area in ("trust", "", "nonsense"):
                labels = sr.labels_for(dict(PROBLEM, kind=kind, area=area))
                self.assertEqual(labels[0] if not area or area == "nonsense" else labels[1], kind)
                self.assertFalse([l for l in labels if l.split("/")[0] in ("importance", "urgency", "size", "severity", "security")])

    def test_every_draft_carries_session_analysis_whatever_its_kind_and_area(self):
        wanted = {
            ("bug", "trust"): ["area/trust", "bug", "session-analysis"],
            ("bug", ""): ["bug", "session-analysis"],
            ("bug", "nonsense"): ["bug", "session-analysis"],
            ("enhancement", "trust"): ["area/trust", "enhancement", "session-analysis"],
            ("enhancement", ""): ["enhancement", "session-analysis"],
            ("enhancement", "nonsense"): ["enhancement", "session-analysis"],
        }
        for (kind, area), labels in wanted.items():
            with self.subTest(kind=kind, area=area):
                self.assertEqual(sr.labels_for(dict(PROBLEM, kind=kind, area=area)), labels)

    def test_a_session_is_marked_only_once_every_problem_in_it_is_settled(self):
        self.analysis("a", [PROBLEM, PROBLEM])
        self.check("a-p1", verdict="confirmed")
        self.check("a-p2", verdict="not-a-defect")
        self.analysis("b", [PROBLEM])
        self.check("b-p1", verdict="confirmed")
        self.analysis("c", [dict(PROBLEM, security=True)])
        (self.work / "filed.json").write_text(json.dumps({"a-p1": {"issue": 99, "how": "filed"}}))

        quiet(sr.record, self.args(dry_run=True))
        self.assertEqual([p.name for p in self.temp.rglob(f"*{sr.MARKER_SUFFIX}")], [])

        quiet(sr.record, self.args(), today="2026-10-14")
        self.assertEqual(sr.read_marker(self.directories["a"], "a")["issues"], [99])
        self.assertEqual(sr.read_marker(self.directories["a"], "a")["turns"], 6)
        self.assertEqual(sr.read_marker(self.directories["a"], "a")["dropped"], 1)
        self.assertIsNone(sr.read_marker(self.directories["b"], "b"))
        self.assertEqual(sr.read_marker(self.directories["c"], "c")["held"], 1)

    def test_a_session_with_no_problems_is_marked_and_one_with_no_result_is_not(self):
        self.analysis("a", [])
        quiet(sr.record, self.args())
        self.assertEqual(sr.read_marker(self.directories["a"], "a")["problems"], 0)
        self.assertIsNone(sr.read_marker(self.directories["b"], "b"))

    def test_a_marked_session_is_not_offered_again_until_it_grows(self):
        self.analysis("a", [])
        quiet(sr.record, self.args())
        found = [dict(session("a", turns=6), marker=sr.read_marker(self.directories["a"], "a"))]
        self.assertEqual(sr.select(found, [], 8, NOW, IDLE), [])
        found[0]["turns"] = 7
        self.assertEqual([s["from_turn"] for s in sr.select(found, [], 8, NOW, IDLE)], [6])


class mock_home:
    """Make `Path.home()` answer a fixed directory for the length of a block."""

    def __init__(self, home):
        self.home = home

    def __enter__(self):
        self.saved = Path.home
        Path.home = classmethod(lambda cls: Path(self.home))

    def __exit__(self, *exc):
        Path.home = self.saved


class Prompts(unittest.TestCase):
    def test_the_prompt_templates_name_only_values_the_script_supplies(self):
        for name, supplied in (
            ("analyze.md", {"session_id", "build", "turns", "from_turn", "record_file", "audit_file", "tracker", "root", "commit", "ref", "results_file"}),
            ("verify.md", {"session_id", "problem_id", "build", "claim", "record_file", "audit_file", "root", "commit", "results_file"}),
        ):
            used = set(re.findall(r"\{\{(\w+)\}\}", (HERE / name).read_text()))
            self.assertLessEqual(used, supplied, name)
            self.assertIn("results_file", used, name)

    def test_neither_template_nor_the_skill_contains_an_em_dash(self):
        for name in ("analyze.md", "verify.md", "SKILL.md", "session-review.py"):
            self.assertNotIn("\u2014", (HERE / name).read_text(), name)

    def test_the_skill_names_the_issue_poster_definition(self):
        self.assertIn("issue-poster", (HERE / "SKILL.md").read_text())


if __name__ == "__main__":
    unittest.main()
