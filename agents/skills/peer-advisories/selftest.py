#!/usr/bin/env python3
"""The decisions peer-advisories.py makes without a model or the network."""

import argparse
import contextlib
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

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("peer_advisories", HERE / "peer-advisories.py")
pa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pa)

os.environ.update(
    GIT_CONFIG_GLOBAL=os.devnull,
    GIT_CONFIG_NOSYSTEM="1",
    GIT_AUTHOR_NAME="selftest",
    GIT_AUTHOR_EMAIL="selftest@example.invalid",
    GIT_COMMITTER_NAME="selftest",
    GIT_COMMITTER_EMAIL="selftest@example.invalid",
)

A, B, C, D = (f"GHSA-{c * 4}-{c * 4}-{c * 4}" for c in "abcd")


def raw(ghsa, **extra):
    return dict(
        {
            "ghsa_id": ghsa,
            "summary": f"summary of {ghsa}",
            "description": "short",
            "severity": "high",
            "published_at": "2026-01-01T00:00:00Z",
            "html_url": f"https://github.com/advisories/{ghsa}",
            "cwes": [{"cwe_id": "CWE-78"}],
            "vulnerabilities": [{"package": {"ecosystem": "npm", "name": "tool"}}],
        },
        **extra,
    )


def quiet(function, *args, **kwargs):
    with contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()):
        code = function(*args, **kwargs)
    return code, out.getvalue()


class Ledger(unittest.TestCase):
    def test_round_trip_is_sorted_one_line_and_keeps_the_header(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "ledger"
            entries = {
                B: {"verdict": "absent", "issue": "-", "commit": "abc", "date": "2026-09-29", "reason": "no\nfetch \u2014 tool"},
                A: {"verdict": "affected", "issue": "#12", "commit": "abc", "date": "2026-09-29", "reason": "x"},
            }
            pa.write_ledger(path, entries)
            text = path.read_text()
            self.assertTrue(text.startswith(pa.HEADER))
            body = [line for line in text.splitlines() if not line.startswith("#")]
            self.assertEqual([line.split()[0] for line in body], [A, B])
            self.assertNotIn("\u2014", text)
            back = pa.read_ledger(path)
            self.assertEqual(back[B]["reason"], "no fetch, tool")
            self.assertEqual(back[A]["issue"], "#12")

    def test_a_malformed_line_is_refused_rather_than_dropped(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "ledger"
            for line in (f"{A} fixed - abc 2026-09-29 x", "GHSA-bad absent - abc 2026-09-29 x", f"{A} absent -"):
                path.write_text(pa.HEADER + line + "\n")
                with self.assertRaises(pa.Problem):
                    pa.read_ledger(path)


class Sources(unittest.TestCase):
    def test_one_advisory_from_two_sources_is_one_entry_naming_both_tools(self):
        merged = pa.gather([
            ("repo", "Codex", [raw(A)]),
            ("npm", "Codex CLI", [raw(A, description="a longer description", cwes=[{"cwe_id": "CWE-22"}])]),
        ])
        self.assertEqual(list(merged), [A])
        self.assertEqual(merged[A]["tools"], ["Codex", "Codex CLI"])
        self.assertEqual(merged[A]["description"], "a longer description")
        self.assertEqual(merged[A]["cwes"], ["CWE-22", "CWE-78"])

    def test_withdrawn_unpublished_malware_and_malformed_entries_are_left_out(self):
        merged = pa.gather([("s", "T", [
            raw(A, withdrawn_at="2026-01-02T00:00:00Z"),
            raw(B, state="draft"),
            raw(C, type="malware"),
            raw("GHSA-../../x"),
            raw(D, html_url="https://evil.example/", cve_id="not a cve"),
        ])])
        self.assertEqual(list(merged), [D])
        self.assertEqual(merged[D]["url"], f"https://github.com/advisories/{D}")
        self.assertIsNone(merged[D]["cve_id"])

    def test_the_unvetted_come_first_by_severity_then_date_and_deferred_last(self):
        advisories = pa.gather([("s", "T", [
            raw(A, severity="low"),
            raw(B, severity="critical", published_at="2025-01-01T00:00:00Z"),
            raw(C, severity="critical", published_at="2026-06-01T00:00:00Z"),
            raw(D, severity="critical"),
        ])])
        ledger = {
            D: {"verdict": "deferred"},
            "GHSA-eeee-eeee-eeee": {"verdict": "absent"},
        }
        self.assertEqual([a["ghsa_id"] for a in pa.select(advisories, ledger, [], 10)], [C, B, A, D])
        self.assertEqual([a["ghsa_id"] for a in pa.select(advisories, ledger, [], 2)], [C, B])

    def test_a_final_verdict_is_not_offered_unless_named(self):
        advisories = pa.gather([("s", "T", [raw(A), raw(B)])])
        ledger = {A: {"verdict": "holds"}}
        self.assertEqual([a["ghsa_id"] for a in pa.select(advisories, ledger, [], 10)], [B])
        self.assertEqual([a["ghsa_id"] for a in pa.select(advisories, ledger, [A], 10)], [A])


class Prompts(unittest.TestCase):
    def test_vendor_text_cannot_close_its_fence(self):
        text = "before\n```\nescaped\n````\nstill inside"
        block = pa.fenced(text)
        fence = block.split("text\n", 1)[0]
        self.assertEqual(fence, "`" * 5)
        inner = block[len(fence) + len("text\n"):-len(fence)]
        self.assertNotIn(fence, inner)

    def test_a_value_is_substituted_once_and_not_expanded(self):
        self.assertEqual(pa.render("{{a}} {{b}}", {"a": "{{b}}", "b": "x"}), "{{b}} x")

    def test_the_prompt_templates_name_only_values_the_script_supplies(self):
        for name, supplied in (
            ("vet.md", {"ghsa_id", "facts", "advisory", "root", "commit", "ref", "tracker", "results_file"}),
            ("verify.md", {"ghsa_id", "facts", "advisory", "claim", "root", "commit", "results_file"}),
        ):
            used = set(re.findall(r"\{\{(\w+)\}\}", (HERE / name).read_text()))
            self.assertLessEqual(used, supplied, name)
            self.assertIn("results_file", used, name)


ISSUE = {
    "title": "The shell tool runs a command nobody approved \u2014 so a workspace file runs code",
    "key": "shell",
    "kind": "bug",
    "security": True,
    "severity": "high",
    "area": "tools",
    "what_happens": "A file in the workspace \u2014 any file \u2014 asks @someone to look.",
    "code_path": "`crates/x/src/lib.rs` line 3, and `@not-a-mention` in code",
    "reproduce": "```\n@inside a fence\n```\nRun it in ROOT/sub.",
    "fix": "Check first.",
}


class Work(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name) / "tree"
        (self.root / "docs").mkdir(parents=True)
        self.work = Path(temp.name) / "work"
        (self.work / "vet").mkdir(parents=True)
        (self.work / "verify").mkdir()
        advisories = pa.gather([("s", "Codex", [raw(g) for g in (A, B, C, D)])])
        manifest = {
            "root": str(self.root),
            "ref": "upstream/main",
            "commit": "0123456789abcdef",
            "advisories": advisories,
            "vet": [
                {"id": g, "prompt_file": str(self.work / "vet" / f"{g}.md"), "results_file": str(self.work / "vet" / f"{g}.json")}
                for g in (A, B, C, D)
            ],
        }
        (self.work / "manifest.json").write_text(json.dumps(manifest))
        self.issue = dict(ISSUE, reproduce=ISSUE["reproduce"].replace("ROOT", str(self.root)))

    def vet(self, ghsa, **result):
        (self.work / "vet" / f"{ghsa}.json").write_text(json.dumps(dict({"ghsa_id": ghsa, "reason": "because"}, **result)))

    def check(self, ghsa, **result):
        (self.work / "verify" / f"{ghsa}.json").write_text(json.dumps(dict({"ghsa_id": ghsa, "reason": "checked"}, **result)))

    def args(self, **extra):
        return argparse.Namespace(**dict({"work_dir": str(self.work), "dry_run": False}, **extra))

    def test_a_result_that_cannot_be_drafted_is_left_undecided(self):
        for result, problem in (
            ({"verdict": "fixed"}, "verdict"),
            ({"verdict": "affected"}, "no issue fields"),
            ({"verdict": "affected", "issue": dict(self.issue, security=True, severity=None)}, "severity"),
            ({"verdict": "affected", "issue": dict(self.issue, kind="security")}, "kind"),
            ({"verdict": "holds", "existing_issue": "12"}, "existing_issue"),
            ({"verdict": "holds", "ghsa_id": B}, "another advisory"),
        ):
            with self.subTest(problem=problem):
                self.vet(A, **result)
                self.assertIn(problem, pa.outcomes(self.work)[1][A]["reason"])
                self.assertEqual(pa.outcomes(self.work)[1][A]["state"], "pending")

    def test_only_an_unfiled_affected_verdict_goes_to_a_verifier_and_the_verifier_decides(self):
        self.vet(A, verdict="affected", issue=self.issue)
        self.vet(B, verdict="affected", existing_issue=40)
        self.vet(C, verdict="absent")
        self.vet(D, verdict="affected", issue=self.issue)
        code, out = quiet(pa.verify, self.args())
        self.assertEqual(code, 0)
        self.assertEqual([v["id"] for v in json.loads(out)["verify"]], [A, D])
        self.assertTrue((self.work / "verify" / f"{A}.md").exists())

        self.check(A, verdict="confirmed", severity="medium")
        self.check(D, verdict="holds", reason="the approval prompt stops it")
        decided = pa.outcomes(self.work)[1]
        self.assertEqual(decided[A]["state"], "file")
        self.assertEqual(decided[A]["fields"]["severity"], "medium")
        self.assertEqual((decided[B]["verdict"], decided[B]["issue"]), ("affected", 40))
        self.assertEqual(decided[C]["verdict"], "absent")
        self.assertEqual((decided[D]["verdict"], decided[D]["reason"]), ("holds", "the approval prompt stops it"))

    def test_a_verifier_can_lower_the_severity_or_drop_security_but_not_raise_them(self):
        self.vet(A, verdict="affected", issue=dict(self.issue, severity="low"))
        self.check(A, verdict="confirmed", severity="high")
        self.assertEqual(pa.outcomes(self.work)[1][A]["fields"]["severity"], "low")
        self.vet(A, verdict="affected", issue=dict(self.issue, security=False))
        self.check(A, verdict="confirmed", security=True, severity="high")
        self.assertEqual(pa.labels_for(pa.outcomes(self.work)[1][A]["fields"]), ["area/tools", "bug"])
        self.vet(A, verdict="affected", issue=self.issue)
        self.check(A, verdict="confirmed", security=False)
        self.assertEqual(pa.labels_for(pa.outcomes(self.work)[1][A]["fields"]), ["area/tools", "bug"])

    def test_a_draft_has_no_em_dash_pings_nobody_and_does_not_name_the_machine(self):
        self.vet(A, verdict="affected", issue=self.issue)
        self.check(A, verdict="confirmed")
        quiet(pa.draft, self.args())
        drafts = json.loads((self.work / "drafts.json").read_text())
        self.assertEqual(len(drafts), 1)
        self.assertEqual(drafts[0]["labels"], ["area/tools", "bug", "needs-security-review", "security", "severity/high"])
        self.assertNotIn("\u2014", drafts[0]["title"])
        self.assertEqual(drafts[0]["key"], "shell")
        body = Path(drafts[0]["body_file"]).read_text()
        self.assertNotIn("\u2014", body)
        self.assertIn("`@someone`", body)
        self.assertIn("`@not-a-mention`", body)
        self.assertNotIn("``@", body)
        self.assertIn("```\n@inside a fence\n```", body)
        self.assertNotIn(str(self.root), body)
        self.assertIn(f"[{A}](https://github.com/advisories/{A})", body)
        self.assertIn("`0123456789ab`", body)

    def test_record_writes_only_what_is_decided_and_filed(self):
        self.vet(A, verdict="affected", issue=self.issue)
        self.check(A, verdict="confirmed")
        self.vet(B, verdict="holds", existing_issue=7)
        self.vet(C, verdict="deferred", reason="needs a Windows host")
        ledger = self.root / pa.LEDGER

        quiet(pa.record, self.args(dry_run=True), today="2026-09-29")
        self.assertFalse(ledger.exists())

        quiet(pa.record, self.args(), today="2026-09-29")
        entries = pa.read_ledger(ledger)
        self.assertEqual(sorted(entries), [B, C])
        self.assertEqual((entries[B]["verdict"], entries[B]["issue"], entries[B]["commit"]), ("holds", "#7", "0123456789ab"))
        self.assertEqual(entries[C]["verdict"], "deferred")

        (self.work / "filed.json").write_text(json.dumps({A: {"issue": 99, "how": "filed"}}))
        quiet(pa.record, self.args(), today="2026-09-30")
        entries = pa.read_ledger(ledger)
        self.assertEqual((entries[A]["verdict"], entries[A]["issue"], entries[A]["date"]), ("affected", "#99", "2026-09-30"))
        self.assertEqual(sorted(entries), [A, B, C])

    def test_an_advisory_this_run_took_up_and_did_not_settle_loses_its_earlier_line(self):
        """An earlier verdict left in place keeps an advisory marked as vetted after a run that tried to vet it again and failed."""
        other = "GHSA-eeee-eeee-eeee"
        ledger = self.root / pa.LEDGER
        earlier = {"issue": "-", "commit": "fedcba987654", "date": "2026-01-01", "reason": "an earlier run"}
        pa.write_ledger(ledger, {g: dict(earlier, verdict=v) for g, v in ((A, "known"), (B, "holds"), (C, "absent"), (D, "known"), (other, "absent"))})
        self.vet(B, verdict="affected", issue=self.issue)
        self.vet(C, verdict="affected", issue=self.issue)
        self.check(C, verdict="confirmed")
        self.vet(D, verdict="holds")
        self.assertEqual({g: o["state"] for g, o in pa.outcomes(self.work)[1].items()}, {A: "pending", B: "verify", C: "file", D: "final"})

        quiet(pa.record, self.args(dry_run=True), today="2026-09-30")
        self.assertEqual(sorted(pa.read_ledger(ledger)), [A, B, C, D, other])

        quiet(pa.record, self.args(), today="2026-09-30")
        entries = pa.read_ledger(ledger)
        self.assertEqual(sorted(entries), [D, other])
        self.assertEqual((entries[D]["verdict"], entries[D]["commit"]), ("holds", "0123456789ab"))
        self.assertEqual((entries[other]["verdict"], entries[other]["commit"]), ("absent", "fedcba987654"))
        advisories = json.loads((self.work / "manifest.json").read_text())["advisories"]
        self.assertEqual(sorted(a["ghsa_id"] for a in pa.select(advisories, entries, [], 10)), [A, B, C])

    def poster(self, cited=(), duplicate=None, labels=None):
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
            existing_labels=lambda repo: set(labels or ["bug", "security", "needs-security-review", "severity/high", "area/tools"]),
            assignable=lambda repo, login: True,
            already_filed=lambda repo, draft: duplicate,
            JITTER=(0.0, 0.0),
        )

    def post_args(self, **extra):
        return self.args(**dict({"repo": "brave/bravebot", "assignee": None, "max": 12, "pace": 0.0}, **extra))

    def drafted(self, *ids):
        for ghsa in ids:
            self.vet(ghsa, verdict="affected", issue=self.issue)
            self.check(ghsa, verdict="confirmed")
        quiet(pa.draft, self.args())

    def test_post_skips_an_advisory_the_tracker_already_cites_and_records_both(self):
        self.drafted(A, B)
        calls, poster = self.poster(cited=[A])
        code, _ = quiet(pa.post, self.post_args(), poster=poster)
        self.assertEqual(code, 0)
        self.assertIn(f'"{A}" in:body', calls[0][calls[0].index("--search") + 1])
        self.assertEqual([c for c in calls if c[0] == "create"], [["create", B]])
        filed = json.loads((self.work / "filed.json").read_text())
        self.assertEqual(filed[A], {"issue": 5, "how": "existing"})
        self.assertEqual(filed[B]["how"], "filed")

        calls.clear()
        quiet(pa.post, self.post_args(), poster=poster)
        self.assertEqual(calls, [])

    def test_a_dry_run_or_a_missing_label_posts_and_records_nothing(self):
        self.drafted(A, B)
        calls, poster = self.poster(cited=[B])
        quiet(pa.post, self.post_args(dry_run=True), poster=poster)
        self.assertFalse((self.work / "filed.json").exists())
        self.assertEqual([c for c in calls if c[0] == "create"], [])

        calls, poster = self.poster(labels=["bug"])
        code, _ = quiet(pa.post, self.post_args(), poster=poster)
        self.assertEqual(code, 2)
        self.assertEqual(calls, [])


class Tree(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.git("init", "-q", "-b", "main")
        (self.root / "docs").mkdir()
        (self.root / "code.rs").write_text("fn main() {}\n")
        self.git("add", ".")
        self.git("commit", "-q", "-m", "base")

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.root), *args], check=True, capture_output=True, text=True).stdout.strip()

    def test_main_itself_and_main_plus_the_ledger_are_vettable(self):
        tip = self.git("rev-parse", "main")
        self.assertEqual(pa.checked_commit(self.root), ("main", tip))
        self.git("checkout", "-q", "-b", "ledger")
        (self.root / pa.LEDGER).write_text(pa.HEADER)
        self.assertEqual(pa.checked_commit(self.root), ("main", tip))
        self.git("add", ".")
        self.git("commit", "-q", "-m", "ledger")
        self.assertEqual(pa.checked_commit(self.root), ("main", tip))

    def test_a_tree_that_differs_from_main_is_refused(self):
        self.git("checkout", "-q", "-b", "work")
        (self.root / "code.rs").write_text("fn main() { edited }\n")
        with self.assertRaisesRegex(pa.Problem, "code.rs"):
            pa.checked_commit(self.root)
        self.git("commit", "-q", "-am", "edit")
        with self.assertRaisesRegex(pa.Problem, "code.rs"):
            pa.checked_commit(self.root)

    def test_a_tree_behind_main_is_refused(self):
        self.git("checkout", "-q", "--detach")
        self.git("checkout", "-q", "main")
        (self.root / "code.rs").write_text("fn main() { newer }\n")
        self.git("commit", "-q", "-am", "newer")
        self.git("checkout", "-q", "HEAD~1")
        with self.assertRaisesRegex(pa.Problem, "does not contain main"):
            pa.checked_commit(self.root)


if __name__ == "__main__":
    unittest.main()
