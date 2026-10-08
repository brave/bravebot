#!/usr/bin/env python3
"""Selftest for issue_helper.py, which every poster of an issue shares."""

import contextlib
import importlib.util
import io
import sys
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("issue_helper", HERE / "issue_helper.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)

LABELS = {"bug", "area/tools"}


def fake_gh(labels=LABELS, assignees=("netzenbot",)):
    calls = []

    def gh(args, repo=None):
        calls.append(list(args))
        if args[:2] == ["label", "list"]:
            return "[" + ",".join(f'{{"name": "{name}"}}' for name in sorted(labels)) + "]"
        if args[0] == "api":
            if args[1].rsplit("/", 1)[-1] in assignees:
                return ""
            raise RuntimeError("Not Found")
        return "https://github.com/brave/bravebot/issues/7\n"

    return calls, gh


class Refusal(unittest.TestCase):
    def test_a_missing_label_is_refused_with_the_line_that_creates_it(self):
        _, gh = fake_gh()
        with mock.patch.object(helper, "gh", gh):
            why = helper.refusal("brave/bravebot", ["bug", "importance/p3"])
        self.assertIn("brave/bravebot has no label importance/p3", why[0])
        self.assertTrue(any(line.startswith("  gh label create importance/p3 ") for line in why))

    def test_a_login_the_repository_refuses_is_refused(self):
        _, gh = fake_gh()
        with mock.patch.object(helper, "gh", gh):
            why = helper.refusal("brave/bravebot", ["bug"], "nobody")
        self.assertIn("would not take nobody", why[0])

    def test_labels_that_exist_and_a_login_that_can_be_assigned_are_clear(self):
        _, gh = fake_gh()
        with mock.patch.object(helper, "gh", gh):
            self.assertIsNone(helper.refusal("brave/bravebot", ["bug", "area/tools"], "netzenbot"))

    def test_no_assignee_is_not_looked_up(self):
        calls, gh = fake_gh()
        with mock.patch.object(helper, "gh", gh):
            helper.refusal("brave/bravebot", ["bug"])
        self.assertFalse([call for call in calls if call[0] == "api"])

    def test_labels_that_cannot_be_read_are_a_refusal_not_a_pass(self):
        def broken(args, repo=None):
            raise RuntimeError("no network")

        with mock.patch.object(helper, "gh", broken):
            why = helper.refusal("brave/bravebot", ["bug"])
        self.assertIn("could not read the labels", why[0])

    def test_a_missing_label_is_reported_before_a_bad_login(self):
        _, gh = fake_gh()
        with mock.patch.object(helper, "gh", gh):
            why = helper.refusal("brave/bravebot", ["nope"], "nobody")
        self.assertIn("has no label nope", why[0])


class Create(unittest.TestCase):
    def test_each_label_and_the_assignee_reach_gh_issue_create(self):
        calls, gh = fake_gh()
        with mock.patch.object(helper, "gh", gh):
            url = helper.create("brave/bravebot", "t", "b.md", ["bug", "area/tools"], "netzenbot")
        args = calls[-1]
        self.assertEqual(url, "https://github.com/brave/bravebot/issues/7")
        self.assertEqual([args[i + 1] for i, a in enumerate(args) if a == "--label"], ["bug", "area/tools"])
        self.assertEqual(args[args.index("--assignee") + 1], "netzenbot")

    def test_no_login_means_no_assignee(self):
        calls, gh = fake_gh()
        with mock.patch.object(helper, "gh", gh):
            helper.create("brave/bravebot", "t", "b.md", ["bug"])
        self.assertNotIn("--assignee", calls[-1])


class Command(unittest.TestCase):
    def run_post(self, argv, **kwargs):
        calls, gh = fake_gh(**kwargs)
        out, err = io.StringIO(), io.StringIO()
        with mock.patch.object(helper, "gh", gh), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = helper.main(["post", "--title", "t", "--body-file", "b.md", *argv])
        return code, out.getvalue(), err.getvalue(), calls

    def test_a_refused_post_creates_nothing(self):
        code, out, err, calls = self.run_post(["--label", "missing"])
        self.assertEqual(code, 2)
        self.assertIn("gh label create missing", err)
        self.assertFalse([call for call in calls if call[:2] == ["issue", "create"]])

    def test_a_clear_post_prints_the_url(self):
        code, out, _, calls = self.run_post(["--label", "bug", "--assignee", "netzenbot"])
        self.assertEqual((code, out.strip()), (0, "https://github.com/brave/bravebot/issues/7"))
        self.assertEqual(sum(call[:2] == ["issue", "create"] for call in calls), 1)


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    sys.exit(0 if result.wasSuccessful() else 1)
