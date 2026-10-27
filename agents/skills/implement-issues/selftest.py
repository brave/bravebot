#!/usr/bin/env python3
"""What implement-issues.py does around the session, against a stand-in for GitHub and for the client.

Each test runs the script as a person does, on a terminal, with a `gh` and a `bravebot` first on PATH.
The `gh` keeps its issues in a JSON file and logs every call. The `bravebot` records its arguments and the
state of the issues at the moment the session opens, which is what the tests read the claim from.
"""

import json
import os
import pty
import shutil
import signal
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "implement-issues.py"
LABEL = "bot/in-progress"
ISSUES = "repos/brave/bravebot/issues"
LOGIN_LOOKUP = ["api", "user", "--jq", ".login"]

FAKE_GH = """#!{python}
import json, os, sys
path = os.environ["FAKE_WORLD"]
world = json.load(open(path))
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps(args) + "\\n")


def refuse(message="boom"):
    print(json.dumps({{"message": message}}), file=sys.stderr)
    sys.exit(1)


def save():
    json.dump(world, open(path, "w"))


if args[0] == "label":
    print(json.dumps([{{"name": name}} for name in world["labels"]]))
    sys.exit(0)
method, fields, route, rest = "GET", {{}}, None, iter(args[1:])
for arg in rest:
    if arg in ("-X", "--method"):
        method = next(rest)
    elif arg in ("-f", "-F"):
        key, _, value = next(rest).partition("=")
        fields.setdefault(key, []).append(value)
    elif arg == "--jq":
        next(rest)
    elif not arg.startswith("-") and route is None:
        route = arg
if f"{{method}} {{route}}" in world.get("fail", []):
    refuse()
if route == "user":
    if world["login"] is None:
        refuse("Bad credentials")
    print(world["login"])
    sys.exit(0)
if route == "search/issues":
    print("\\n".join(str(number) for number in world["search"].get(fields["q"][0], [])))
    sys.exit(0)
parts = route.split("/")
if route.startswith("repos/brave/bravebot/assignees/"):
    sys.exit(0 if parts[-1] in world["assignable"] else 1)
issue = world["issues"].get(parts[4])
if issue is None:
    refuse("Not Found")
if len(parts) == 5:
    print(json.dumps(issue))
    sys.exit(0)
if parts[5] == "assignees" and method == "POST":
    issue["assignees"] += fields["assignees[]"]
elif parts[5] == "assignees" and method == "DELETE":
    issue["assignees"] = [one for one in issue["assignees"] if one not in fields["assignees[]"]]
elif parts[5] == "labels" and method == "POST":
    issue["labels"] += fields["labels[]"]
elif parts[5] == "labels" and method == "DELETE":
    name = parts[6].replace("%2F", "/")
    if name not in issue["labels"]:
        refuse("Label does not exist")
    issue["labels"].remove(name)
else:
    refuse("unexpected " + route)
save()
"""

FAKE_BRAVEBOT = """#!{python}
import json, os, sys, time
json.dump({{"client": os.path.basename(sys.argv[0]), "argv": sys.argv[1:], "world": json.load(open(os.environ["FAKE_WORLD"]))}}, open(os.environ["FAKE_SESSION"], "w"))
if os.environ.get("FAKE_HOLD"):
    open(os.environ["FAKE_HOLD"], "w").close()
    time.sleep(60)
sys.exit(int(os.environ.get("FAKE_EXIT", "0")))
"""


def issue(state="open", pull=False, assignees=(), labels=()):
    return {"state": state, "pull": pull, "assignees": list(assignees), "labels": list(labels)}


class ImplementIssues(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root, True)
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        for name, text in (("gh", FAKE_GH), ("bravebot", FAKE_BRAVEBOT)):
            (bin_dir / name).write_text(text.format(python=sys.executable))
            (bin_dir / name).chmod(0o755)
        (self.root / "gitconfig").write_text("[user]\n\tname = Jane Doe\n")
        self.world_path, self.log_path, self.session_path = (self.root / n for n in ("world", "log", "session"))
        self.world = {
            "login": "netzenbot",
            "labels": [LABEL, "bug"],
            "assignable": ["netzenbot"],
            "search": {},
            "issues": {"10": issue(), "12": issue()},
        }
        self.env = dict(
            os.environ,
            PATH=f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
            GIT_CONFIG_GLOBAL=str(self.root / "gitconfig"),
            GIT_CONFIG_NOSYSTEM="1",
            TMPDIR=str(self.root),
            FAKE_WORLD=str(self.world_path),
            FAKE_LOG=str(self.log_path),
            FAKE_SESSION=str(self.session_path),
        )

    def start(self, argument, tty=True, client=None):
        command = [sys.executable, str(SCRIPT), *(["--bravebot", client] if client else []), "--", argument]
        return self.spawn(command, self.root, tty)

    def spawn(self, command, cwd, tty=True):
        self.world_path.write_text(json.dumps(self.world))
        if not tty:
            process = subprocess.Popen(
                command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                env=self.env, cwd=cwd, text=True,
            )
            return SimpleNamespace(process=process, master=None, seen=bytearray(), reader=None)
        master, slave = pty.openpty()
        process = subprocess.Popen(
            command, stdin=slave, stdout=slave, stderr=subprocess.PIPE, env=self.env, cwd=cwd, text=True,
        )
        os.close(slave)
        seen = bytearray()

        def drain():
            # macOS drops what a terminal holds once its last slave end closes, so it is read as it comes.
            while True:
                try:
                    chunk = os.read(master, 65536)
                except OSError:
                    return
                if not chunk:
                    return
                seen.extend(chunk)

        reader = threading.Thread(target=drain, daemon=True)
        reader.start()
        return SimpleNamespace(process=process, master=master, seen=seen, reader=reader)

    def finish(self, running):
        piped, err = running.process.communicate(timeout=60)
        if running.reader:
            running.reader.join(10)
            os.close(running.master)
        out = bytes(running.seen) + (piped or "").encode()
        return SimpleNamespace(code=running.process.returncode, out=out.decode(errors="replace"), err=err)

    def run_script(self, argument, tty=True, client=None, **env):
        self.env.update(env)
        return self.finish(self.start(argument, tty, client))

    def state(self):
        return json.loads(self.world_path.read_text())["issues"]

    def session(self):
        return json.loads(self.session_path.read_text()) if self.session_path.exists() else None

    def calls(self):
        if not self.log_path.exists():
            return []
        return [json.loads(line) for line in self.log_path.read_text().splitlines()]

    def writes(self):
        return [call for call in self.calls() if "--method" in call]

    def test_an_issue_assigned_to_somebody_else_stops_the_run_before_anything_changes(self):
        """The check covers every issue before the first one is written, so the unassigned #10 is untouched."""
        self.world["issues"]["11"] = issue(assignees=["alice"])
        result = self.run_script("10, 11")
        self.assertNotEqual(result.code, 0)
        self.assertIn("#11 is assigned to alice", result.err)
        self.assertEqual(self.writes(), [])
        self.assertIsNone(self.session())
        self.assertEqual(self.state()["10"], issue())

    def test_an_issue_assigned_to_the_current_user_and_somebody_else_is_refused(self):
        self.world["issues"]["10"] = issue(assignees=["NetzenBot", "alice"])
        result = self.run_script("10")
        self.assertNotEqual(result.code, 0)
        self.assertIn("#10 is assigned to alice\n", result.err)
        self.assertEqual(self.writes(), [])

    def test_a_run_claims_before_the_session_and_gives_back_only_the_label(self):
        self.world["issues"]["12"] = issue(assignees=["netzenbot"])
        result = self.run_script("10, 12")
        self.assertEqual(result.code, 0, result.err)
        seen = self.session()
        self.assertEqual(seen["argv"], ["-i", "/implement-issues 10, 12"])
        self.assertEqual(seen["world"]["issues"]["10"], issue(assignees=["netzenbot"], labels=[LABEL]))
        self.assertEqual(seen["world"]["issues"]["12"], issue(assignees=["netzenbot"], labels=[LABEL]))
        self.assertEqual(self.state()["10"], issue(assignees=["netzenbot"]))
        self.assertEqual(self.state()["12"], issue(assignees=["netzenbot"]))
        assigned = [call for call in self.writes() if call[-1].startswith("assignees[]")]
        self.assertEqual([call[3] for call in assigned], [f"{ISSUES}/10/assignees"])
        self.assertIn(f"removed {LABEL} from #10, #12", result.out)

    def test_the_argument_reaches_the_skill_as_written(self):
        argument = "  10,\n#12  \"quoted\" it's $HOME  "
        self.run_script(argument)
        self.assertEqual(self.session()["argv"], ["-i", f"/implement-issues {argument}"])

    def test_the_label_is_removed_when_the_session_fails(self):
        result = self.run_script("10", FAKE_EXIT="3")
        self.assertEqual(result.code, 3)
        self.assertEqual(self.state()["10"]["labels"], [])

    def test_the_label_is_removed_when_the_client_cannot_be_started(self):
        broken = self.root / "broken"
        broken.write_text("not a program\n")
        broken.chmod(0o755)
        result = self.run_script("10", client=str(broken))
        self.assertNotEqual(result.code, 0)
        self.assertIn("could not start", result.err)
        self.assertEqual(self.state()["10"]["labels"], [])

    def test_the_label_is_removed_when_the_run_is_terminated(self):
        hold = self.root / "held"
        self.env["FAKE_HOLD"] = str(hold)
        running = self.start("10")
        deadline = time.time() + 30
        while not hold.exists() and time.time() < deadline:
            time.sleep(0.05)
        self.assertTrue(hold.exists(), "the session never opened")
        self.assertEqual(self.state()["10"]["labels"], [LABEL])
        running.process.send_signal(signal.SIGTERM)
        result = self.finish(running)
        self.assertNotEqual(result.code, 0)
        self.assertEqual(self.state()["10"]["labels"], [])

    def test_a_label_the_issue_already_had_is_neither_added_nor_removed(self):
        self.world["issues"]["10"] = issue(labels=[LABEL])
        result = self.run_script("10, 12")
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.state()["10"]["labels"], [LABEL])
        self.assertEqual(self.state()["12"]["labels"], [])
        labels = [call[3] for call in self.writes() if "labels" in call[3]]
        self.assertEqual(labels, [f"{ISSUES}/12/labels", f"{ISSUES}/12/labels/bot%2Fin-progress"])

    def test_pull_requests_closed_issues_and_other_repositories_are_left_alone(self):
        self.world["issues"].update({"20": issue(pull=True, assignees=["alice"]), "30": issue(state="closed", assignees=["bob"])})
        result = self.run_script("10, 20 30, https://github.com/other/repo/issues/5, https://github.com/brave/bravebot/issues/12")
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.state()["20"], issue(pull=True, assignees=["alice"]))
        self.assertEqual(self.state()["30"], issue(state="closed", assignees=["bob"]))
        self.assertTrue(all(call[3].split("/")[4] in ("10", "12") for call in self.writes()))
        self.assertIn("#20 is a pull request", result.out)
        self.assertIn("#30 is closed", result.out)
        self.assertIn("not in brave/bravebot", result.out)

    def test_a_query_is_searched_the_way_the_skill_searches_and_what_it_finds_is_claimed(self):
        self.world["search"] = {"repo:brave/bravebot label:area/cli no:assignee is:open": [12]}
        result = self.run_script("label:area/cli no:assignee")
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.session()["argv"], ["-i", "/implement-issues label:area/cli no:assignee"])
        self.assertEqual(self.state()["12"]["assignees"], ["netzenbot"])
        self.assertEqual(self.state()["10"], issue())

    def test_a_query_that_names_a_state_is_not_limited_to_open_issues(self):
        self.world["issues"]["30"] = issue(state="closed")
        self.world["search"] = {"repo:brave/bravebot label:bug is:closed": [30]}
        result = self.run_script("label:bug is:closed")
        self.assertNotEqual(result.code, 0)
        self.assertIn("no open issue to implement", result.err)
        self.assertIn("#30 is closed", result.out)
        self.assertIsNone(self.session())
        self.assertEqual(self.writes(), [])

    def test_a_search_that_finds_nothing_opens_no_session(self):
        result = self.run_script("label:nothing")
        self.assertNotEqual(result.code, 0)
        self.assertIn("no open issue to implement", result.err)
        self.assertIsNone(self.session())
        self.assertEqual(self.writes(), [])

    def test_a_failed_write_undoes_what_was_written_and_opens_no_session(self):
        self.world["fail"] = [f"POST {ISSUES}/12/labels"]
        result = self.run_script("10, 12")
        self.assertNotEqual(result.code, 0)
        self.assertIn("could not add bot/in-progress to #12", result.err)
        self.assertEqual(self.state()["10"], issue())
        self.assertEqual(self.state()["12"], issue())
        self.assertIsNone(self.session())

    def test_the_login_is_the_account_gh_writes_as_and_not_the_git_display_name(self):
        """`git config user.name` is `Jane Doe` here, as it is for most people, and the claim is made as gh's account."""
        self.world["login"] = "octocat"
        self.world["assignable"] = ["octocat"]
        self.world["issues"]["12"] = issue(assignees=["netzenbot"])
        result = self.run_script("10, 12")
        self.assertNotEqual(result.code, 0)
        self.assertIn("#12 is assigned to netzenbot", result.err)
        self.assertEqual(self.writes(), [])
        del self.world["issues"]["12"]
        result = self.run_script("10")
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.state()["10"]["assignees"], ["octocat"])

    def test_a_login_gh_cannot_read_stops_the_run_before_anything_changes(self):
        self.world["login"] = None
        result = self.run_script("10")
        self.assertNotEqual(result.code, 0)
        self.assertIn("could not read the login gh writes as", result.err)
        self.assertEqual(self.writes(), [])
        self.assertIsNone(self.session())

    def test_a_label_the_repository_lacks_is_refused_rather_than_created(self):
        self.world["labels"] = ["bug"]
        result = self.run_script("10")
        self.assertNotEqual(result.code, 0)
        self.assertIn(f"no label {LABEL}", result.err)
        self.assertEqual(self.writes(), [])

    def test_a_login_the_repository_will_not_assign_is_refused(self):
        self.world["assignable"] = []
        result = self.run_script("10")
        self.assertNotEqual(result.code, 0)
        self.assertIn("would not take netzenbot as an assignee", result.err)
        self.assertEqual(self.writes(), [])

    def test_without_a_terminal_nothing_is_changed(self):
        result = self.run_script("10", tty=False)
        self.assertNotEqual(result.code, 0)
        self.assertIn("needs a terminal", result.err)
        self.assertEqual(self.calls(), [])
        self.assertIsNone(self.session())

    def test_a_batch_branch_with_a_ledger_is_resumed_from_the_client_and_not_searched_for(self):
        (self.root / "implement-issues").mkdir()
        (self.root / "implement-issues" / "fix-i18n-gaps.md").write_text("ledger\n")
        result = self.run_script("fix-i18n-gaps")
        self.assertNotEqual(result.code, 0)
        self.assertIn("fix-i18n-gaps resumes an earlier run", result.err)
        self.assertEqual([call for call in self.calls() if call != LOGIN_LOOKUP], [])

    def test_the_make_target_passes_the_issues_and_the_client_through(self):
        """The recipe takes ISSUES from the environment, so a quote or a `$` in it is not the shell's."""
        tree = self.root / "tree"
        (tree / "agents/skills/implement-issues").mkdir(parents=True)
        for name in ("Makefile", "agents/pr_worktree.py", "agents/issue_helper.py", "agents/skills/implement-issues/implement-issues.py"):
            shutil.copy(HERE.parents[2] / name, tree / name)
        client = self.root / "bin" / "other-client"
        shutil.copy(self.root / "bin" / "bravebot", client)
        argument = "10, it's \"a\" $$HOME"
        running = self.spawn(["make", "--no-print-directory", "implement-issues", f"ISSUES={argument}", f"BRAVEBOT={client}"], tree)
        result = self.finish(running)
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.session()["client"], "other-client")
        self.assertEqual(self.session()["argv"], ["-i", "/implement-issues 10, it's \"a\" $HOME"])
        self.assertEqual(self.state()["10"]["assignees"], ["netzenbot"])

    def test_a_word_that_is_no_ledger_is_a_query(self):
        self.world["search"] = {"repo:brave/bravebot fix-i18n-gaps is:open": [10]}
        result = self.run_script("fix-i18n-gaps")
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.state()["10"]["assignees"], ["netzenbot"])

    def test_an_assignee_that_is_not_a_login_is_not_printed(self):
        self.world["issues"]["10"] = issue(assignees=["bad\x1b[31m name"])
        result = self.run_script("10")
        self.assertIn("#10 is assigned to unknown", result.err)
        self.assertNotIn("\x1b", result.err)


if __name__ == "__main__":
    unittest.main()
