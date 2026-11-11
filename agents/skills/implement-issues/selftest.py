#!/usr/bin/env python3
"""What implement-issues.py does around the session, against a stand-in for GitHub and for the client.

Each test runs the script as a person does, on a terminal, with a `gh` and a `bravebot` first on PATH.
The `gh` keeps its issues in a JSON file and logs every call. The `bravebot` records its arguments and the
state of the issues at the moment the session opens, which is what the tests read the claim from.
"""

import datetime
import json
import os
import pty
import re
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
CLAIMED = re.compile(r"Claimed: at (\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ), before this session opened, implement-issues\.py (.*)")

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
if os.environ.get("FAKE_UNLABEL"):
    world = json.load(open(os.environ["FAKE_WORLD"]))
    for one in world["issues"].values():
        one["labels"] = [name for name in one["labels"] if name != os.environ["FAKE_UNLABEL"]]
    json.dump(world, open(os.environ["FAKE_WORLD"], "w"))
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
        # The Makefile target runs the client the environment names in place of the one on PATH, so a
        # person with BRAVEBOT exported would have their real client opened on the test's terminal
        # (brave/bravebot#3591).
        self.env.pop("BRAVEBOT", None)

    def start(self, argument, tty=True, client=None, notes=None, flags=()):
        command = [
            sys.executable, str(SCRIPT), *(["--bravebot", client] if client else []),
            *(["--notes", notes] if notes is not None else []), *flags, "--", argument,
        ]
        return self.spawn(command, self.root, tty)

    def spawn(self, command, cwd, tty=True):
        self.world_path.write_text(json.dumps(self.world))
        # A session of its own, so the command and everything it starts are one process group that
        # `reap` can end. In the test's group, a command that hung would outlive the run.
        if not tty:
            process = subprocess.Popen(
                command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                env=self.env, cwd=cwd, text=True, start_new_session=True,
            )
            running = SimpleNamespace(process=process, master=None, seen=bytearray(), reader=None)
            self.addCleanup(self.reap, running)
            return running
        master, slave = pty.openpty()
        process = subprocess.Popen(
            command, stdin=slave, stdout=slave, stderr=subprocess.PIPE, env=self.env, cwd=cwd, text=True,
            start_new_session=True,
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
        running = SimpleNamespace(process=process, master=master, seen=seen, reader=reader)
        self.addCleanup(self.reap, running)
        return running

    def reap(self, running):
        """End the command and everything it started, and let go of its terminal.

        `communicate` raises when the time is up and leaves the child running, and what a recipe starts
        is not the child. A client left on a terminal nobody holds any more reads it for ever and takes
        a core until it is killed."""
        if running.process.poll() is None:
            try:
                os.killpg(running.process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            try:
                running.process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                # Held open by something that left the group, which is not this method's to end.
                pass
        if running.master is not None:
            if running.reader:
                running.reader.join(10)
            os.close(running.master)
            running.master = None

    def finish(self, running, timeout=60):
        try:
            piped, err = running.process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.reap(running)
            raise
        if running.reader:
            running.reader.join(10)
            os.close(running.master)
            running.master = None
        out = bytes(running.seen) + (piped or "").encode()
        return SimpleNamespace(code=running.process.returncode, out=out.decode(errors="replace"), err=err)

    def run_script(self, argument, tty=True, client=None, notes=None, flags=(), **env):
        self.env.update(env)
        return self.finish(self.start(argument, tty, client, notes, flags))

    def searched(self):
        return [call for call in self.calls() if "search/issues" in call]

    def state(self):
        return json.loads(self.world_path.read_text())["issues"]

    def session(self):
        return json.loads(self.session_path.read_text()) if self.session_path.exists() else None

    def sent(self):
        """The prompt the session opened on, which follows `-i` as the last argument."""
        argv = self.session()["argv"]
        self.assertEqual(argv[-2], "-i")
        return argv[-1]

    def told(self):
        """The prompt the session opened on without its Claimed line, and that line."""
        lines = self.sent().split("\n")
        at = next(index for index, line in enumerate(lines) if line.startswith("Claimed: "))
        return "\n".join(lines[:at] + lines[at + 1:]), lines[at]

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
        self.assertIn("FORCE=1", result.err)
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
        self.assertEqual(self.told()[0], "/implement-issues 10, 12")
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
        self.assertEqual(self.told()[0], f"/implement-issues {argument}")

    def test_notes_reach_the_skill_after_the_argument_and_name_no_issue(self):
        """The commas and words of the notes would be searched if they were read as pieces of the list."""
        notes = "do 4, group related ones\nand \"quote\" it's $HOME"
        result = self.run_script("10", notes=notes)
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.told()[0], f"/implement-issues 10\nNotes: {notes}")
        self.assertEqual(self.searched(), [])
        self.assertEqual(self.state()["12"], issue())
        self.assertEqual(self.session()["world"]["issues"]["10"], issue(assignees=["netzenbot"], labels=[LABEL]))

    def test_notes_written_in_the_argument_are_not_read_as_issues_either(self):
        argument = "10\nNotes: group them, 12 can wait"
        result = self.run_script(argument)
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.told()[0], f"/implement-issues {argument}")
        self.assertEqual(self.searched(), [])
        self.assertEqual(self.state()["12"], issue())

    def test_a_notes_word_in_the_middle_of_a_line_is_still_part_of_the_list(self):
        self.world["search"] = {"repo:brave/bravebot Notes: x is:open": [12]}
        result = self.run_script("10, Notes: x")
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.state()["12"]["assignees"], ["netzenbot"])

    def test_notes_with_no_list_open_no_session(self):
        for result in (self.run_script("  ", notes="do 4"), self.run_script("Notes: do 4")):
            self.assertNotEqual(result.code, 0)
            self.assertIn("name the issues to implement", result.err)
        self.assertEqual(self.writes(), [])
        self.assertIsNone(self.session())

    def test_empty_notes_add_nothing_to_the_argument(self):
        self.run_script("10", notes=" \n ")
        self.assertEqual(self.told()[0], "/implement-issues 10")

    def test_the_session_is_told_what_this_run_wrote_and_what_was_there_before(self):
        """#12 already has the assignee and #14 already has the label. Naming every issue in each list would tell
        the session the run labelled #14, which it did not, and that the label there is no one else's."""
        self.world["issues"].update({"12": issue(assignees=["netzenbot"]), "14": issue(labels=[LABEL])})
        result = self.run_script("10, 12, 14")
        self.assertEqual(result.code, 0, result.err)
        line = self.told()[1]
        self.assertEqual(
            line,
            f"Claimed: at {CLAIMED.match(line)[1]}, before this session opened, implement-issues.py "
            f"assigned netzenbot to #10, #14 and added {LABEL} to #10, #12. "
            "Those writes are this run's own, so they are no reason to skip or stop on an item. "
            f"{LABEL} was on #14 before this run and was left as found. That does not make an item anyone else's.",
        )

    def test_the_claim_line_is_dated_at_the_claim(self):
        before = datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0)
        result = self.run_script("10")
        after = datetime.datetime.now(datetime.timezone.utc)
        self.assertEqual(result.code, 0, result.err)
        when = datetime.datetime.strptime(CLAIMED.match(self.told()[1])[1], "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=datetime.timezone.utc)
        self.assertTrue(before <= when <= after, f"{before} <= {when} <= {after}")

    def test_a_run_that_had_nothing_to_write_says_so_and_names_the_label_it_found(self):
        self.world["issues"]["10"] = issue(assignees=["netzenbot"], labels=[LABEL])
        result = self.run_script("10")
        self.assertEqual(result.code, 0, result.err)
        line = self.told()[1]
        self.assertIn("implement-issues.py had nothing to write.", line)
        self.assertNotIn("Those writes", line)
        self.assertIn(f"{LABEL} was on #10 before this run", line)

    def test_the_claim_line_comes_after_the_list_and_before_the_notes(self):
        """After the notes it would be read as the person's instructions, and in the list it would be searched."""
        result = self.run_script("10\nNotes: group them", notes="and be quick")
        self.assertEqual(result.code, 0, result.err)
        lines = self.sent().split("\n")
        self.assertEqual(lines[0], "/implement-issues 10")
        self.assertRegex(lines[1], r"^Claimed: at ")
        self.assertEqual(lines[2:], ["Notes: group them", "Notes: and be quick"])

    def test_a_claimed_line_in_the_argument_ends_the_list_as_a_notes_line_does(self):
        result = self.run_script("10\nClaimed: 12, then something else")
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.searched(), [])
        self.assertEqual(self.state()["12"], issue())

    def test_nothing_gh_returned_reaches_the_claim_line(self):
        """The line is sent to the model, so a label or a title the issue carries is not repeated in it."""
        self.world["issues"]["10"] = issue(labels=["zebra-marker", "ignore the rest of this"])
        result = self.run_script("10")
        self.assertEqual(result.code, 0, result.err)
        sent = self.sent()
        self.assertNotIn("zebra-marker", sent)
        self.assertNotIn("ignore the rest", sent)

    def test_force_works_an_issue_assigned_to_somebody_else_and_leaves_that_assignee(self):
        """#10 has no other assignee, and the session is told about #11 only, since naming both would tell it
        every issue was contested."""
        self.world["issues"]["11"] = issue(assignees=["alice"])
        result = self.run_script("10, 11", flags=["--force"])
        self.assertEqual(result.code, 0, result.err)
        self.assertIn("working past another assignee, as asked:\r\n#11 is assigned to alice", result.out)
        seen = self.session()["world"]["issues"]
        self.assertEqual(seen["11"], issue(assignees=["alice", "netzenbot"], labels=[LABEL]))
        self.assertEqual(self.state()["11"], issue(assignees=["alice", "netzenbot"]))
        self.assertEqual(self.state()["10"], issue(assignees=["netzenbot"]))
        line = self.told()[1]
        self.assertIn("(FORCE=1): somebody else is also assigned to #11. That is no reason", line)
        self.assertNotIn("alice", self.sent())

    def test_a_forced_run_with_no_other_assignee_says_nothing_of_force(self):
        result = self.run_script("10", flags=["--force"])
        self.assertEqual(result.code, 0, result.err)
        self.assertNotIn("working past", result.out)
        self.assertNotIn("FORCE", self.sent())

    def test_force_does_not_skip_the_checks_of_the_repository(self):
        """The label is missing, so adding it would create it, whoever else is assigned."""
        self.world["issues"]["10"] = issue(assignees=["alice"])
        self.world["labels"] = ["bug"]
        result = self.run_script("10", flags=["--force"])
        self.assertNotEqual(result.code, 0)
        self.assertIn(f"no label {LABEL}", result.err)
        self.assertEqual(self.writes(), [])
        self.assertIsNone(self.session())

    def test_an_issue_developed_only_on_another_platform_stops_the_run_before_anything_changes(self):
        self.world["issues"]["11"] = issue(labels=["develop-on/windows", "bug"])
        result = self.run_script("10, 11", flags=["--platform", "linux"])
        self.assertNotEqual(result.code, 0)
        self.assertIn("#11 is labelled develop-on/windows and this host is linux", result.err)
        self.assertIn("FORCE=1", result.err)
        self.assertEqual(self.writes(), [])
        self.assertIsNone(self.session())
        self.assertEqual(self.state()["10"], issue())

    def test_an_issue_labelled_for_this_platform_or_for_none_is_worked(self):
        self.world["issues"]["10"] = issue(labels=["develop-on/macos"])
        self.world["issues"]["12"] = issue(labels=["bug", "develop-on-windows", "develop-on/"])
        result = self.run_script("10, 12", flags=["--platform", "macos"])
        self.assertEqual(result.code, 0, result.err)
        self.assertNotIn("working past", result.out)
        self.assertNotIn("FORCE", self.sent())

    def test_an_issue_labelled_for_several_platforms_is_worked_on_any_of_them(self):
        self.world["issues"]["10"] = issue(labels=["develop-on/linux", "develop-on/windows"])
        self.assertEqual(self.run_script("10", flags=["--platform", "windows"]).code, 0)
        refused = self.run_script("10", flags=["--platform", "macos"])
        self.assertNotEqual(refused.code, 0)
        self.assertIn("#10 is labelled develop-on/linux, develop-on/windows and this host is macos", refused.err)

    def test_the_platform_defaults_to_the_host_the_script_runs_on(self):
        here = {"darwin": "macos", "linux": "linux", "win32": "windows"}[sys.platform]
        other = "linux" if here != "linux" else "macos"
        self.world["issues"]["10"] = issue(labels=[f"develop-on/{here}"])
        self.world["issues"]["12"] = issue(labels=[f"develop-on/{other}"])
        result = self.run_script("10, 12")
        self.assertNotEqual(result.code, 0)
        self.assertIn(f"#12 is labelled develop-on/{other} and this host is {here}", result.err)
        self.assertNotIn("#10 is labelled", result.err)

    def test_force_works_an_issue_developed_on_another_platform_and_keeps_its_label(self):
        self.world["issues"]["11"] = issue(labels=["develop-on/windows"])
        result = self.run_script("10, 11", flags=["--force", "--platform", "linux"])
        self.assertEqual(result.code, 0, result.err)
        self.assertIn("working past the platform labels, as asked:\r\n#11 is labelled develop-on/windows", result.out)
        self.assertEqual(self.state()["11"], issue(assignees=["netzenbot"], labels=["develop-on/windows"]))
        line = self.told()[1]
        self.assertIn("past the platform labels (FORCE=1): #11 is labelled develop-on/windows. That is no reason", line)
        self.assertNotIn("#10 is labelled", line)

    def test_force_reports_both_an_other_assignee_and_another_platform(self):
        self.world["issues"]["10"] = issue(assignees=["alice"])
        self.world["issues"]["12"] = issue(labels=["develop-on/windows"])
        refused = self.run_script("10, 12", flags=["--platform", "linux"])
        self.assertNotEqual(refused.code, 0)
        self.assertIn("#10 is assigned to alice", refused.err)
        self.assertIn("#12 is labelled develop-on/windows and this host is linux", refused.err)
        forced = self.run_script("10, 12", flags=["--force", "--platform", "linux"])
        self.assertEqual(forced.code, 0, forced.err)
        self.assertIn("working past another assignee, as asked:\r\n#10 is assigned to alice", forced.out)
        self.assertIn("working past the platform labels, as asked:\r\n#12 is labelled develop-on/windows", forced.out)
        line = self.told()[1]
        self.assertIn("(FORCE=1): somebody else is also assigned to #10. That is no reason", line)
        self.assertIn("past the platform labels (FORCE=1): #12 is labelled develop-on/windows. That is no reason", line)
        self.assertNotIn("alice", self.sent())

    def test_bypass_opens_the_client_with_the_flag_for_no_prompts_and_no_sandbox(self):
        """`--dangerously-skip-permissions` alone leaves the sandbox on, and the flag after `-i` would be part of
        the prompt, so the whole argument list is the assertion."""
        result = self.run_script("10", flags=["--bypass"])
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.session()["argv"], ["--dangerously-bypass-approvals-and-sandbox", "-i", self.sent()])
        self.assertTrue(self.sent().startswith("/implement-issues 10\nClaimed: at "))
        self.assertIn("bypass: the session runs with --dangerously-bypass-approvals-and-sandbox", result.out)

    def test_a_run_without_bypass_passes_the_client_nothing_but_the_prompt(self):
        result = self.run_script("10")
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.session()["argv"], ["-i", self.sent()])
        self.assertNotIn("bypass", result.out)

    def test_the_label_is_removed_when_a_bypassed_session_fails(self):
        result = self.run_script("10", flags=["--bypass"], FAKE_EXIT="3")
        self.assertEqual(result.code, 3)
        self.assertEqual(self.state()["10"]["labels"], [])

    def test_the_label_is_removed_when_the_session_fails(self):
        result = self.run_script("10", FAKE_EXIT="3")
        self.assertEqual(result.code, 3)
        self.assertEqual(self.state()["10"]["labels"], [])

    def test_a_label_the_session_already_removed_is_not_reported_as_an_error(self):
        """GitHub answers 404 `Label does not exist` for a label that is off the issue, which is the state the
        run wants, so it prints nothing and exits 0."""
        result = self.run_script("10, 12", FAKE_UNLABEL=LABEL)
        self.assertEqual(result.code, 0, result.err)
        self.assertNotIn("could not remove", result.err)
        self.assertNotIn("remove it with", result.err)
        self.assertEqual(self.state()["10"]["labels"], [])
        self.assertEqual(self.state()["12"]["labels"], [])

    def test_a_label_that_cannot_be_removed_for_another_reason_is_still_reported(self):
        self.world["fail"] = [f"DELETE {ISSUES}/10/labels/bot%2Fin-progress"]
        result = self.run_script("10, 12")
        self.assertEqual(result.code, 1)
        self.assertIn(f"could not remove {LABEL} from #10: {{\"message\": \"boom\"}}", result.err)
        self.assertIn(f"gh issue edit 10 --repo brave/bravebot --remove-label {LABEL}", result.err)
        self.assertNotIn("#12", result.err)
        self.assertEqual(self.state()["12"]["labels"], [])

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
        self.assertEqual(self.told()[0], "/implement-issues label:area/cli no:assignee")
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

    LEDGER = (
        "| item | group | worktree | branch | status | commits |\n"
        "|---|---|---|---|---|---|\n"
        "| #10 | a | /w/a | fix-a | working | |\n"
        "| #12 | b | /w/b | fix-b | `committed` | abc |\n"
        "| #14 | c | /w/c | fix-c | done | def |\n"
        "| #16 | | | | skipped | |\n"
        "| #18 | | | | blocked | |\n"
    )

    def write_ledger(self, text=LEDGER, name="fix-i18n-gaps"):
        (self.root / "implement-issues").mkdir(exist_ok=True)
        (self.root / "implement-issues" / f"{name}.md").write_text(text)

    def test_a_batch_branch_with_a_ledger_claims_the_items_still_to_work_and_resumes_it(self):
        """#14, #16 and #18 are finished, so only #10 and #12 are claimed, and the skill gets the branch name."""
        self.world["issues"].update({str(n): issue() for n in (14, 16, 18)})
        self.write_ledger()
        result = self.run_script("fix-i18n-gaps")
        self.assertEqual(result.code, 0, result.err)
        held = self.session()
        self.assertEqual(self.told()[0], "/implement-issues fix-i18n-gaps")
        self.assertEqual({n: (i["assignees"], i["labels"]) for n, i in held["world"]["issues"].items() if i["assignees"] or i["labels"]},
                         {"10": (["netzenbot"], [LABEL]), "12": (["netzenbot"], [LABEL])})
        self.assertEqual(self.state()["10"]["labels"], [])
        self.assertEqual(self.state()["14"], issue())

    def test_a_ledger_item_assigned_to_somebody_else_stops_the_resume(self):
        self.world["issues"]["12"] = issue(assignees=["jane"])
        self.write_ledger()
        result = self.run_script("fix-i18n-gaps")
        self.assertNotEqual(result.code, 0)
        self.assertIn("#12 is assigned to jane", result.err)
        self.assertEqual(self.writes(), [])
        self.assertIsNone(self.session())

    def test_a_ledger_with_no_item_rows_is_refused_rather_than_resumed_unclaimed(self):
        self.write_ledger("ledger\n")
        result = self.run_script("fix-i18n-gaps")
        self.assertNotEqual(result.code, 0)
        self.assertIn("has no row whose item is #<number>", result.err)
        self.assertEqual(self.writes(), [])
        self.assertIsNone(self.session())

    def test_a_ledger_with_every_item_finished_opens_no_session(self):
        self.write_ledger("| #10 | a | w | b | done | c |\n")
        result = self.run_script("fix-i18n-gaps")
        self.assertNotEqual(result.code, 0)
        self.assertIn("no open issue to implement", result.err)
        self.assertIsNone(self.session())

    def make_tree(self):
        """A copy of the files the make target runs, so that it is run as a person runs it."""
        tree = self.root / "tree"
        (tree / "agents/skills/implement-issues").mkdir(parents=True)
        for name in ("Makefile", "agents/pr_worktree.py", "agents/issue_helper.py", "agents/skills/implement-issues/implement-issues.py"):
            shutil.copy(HERE.parents[2] / name, tree / name)
        return tree

    def test_the_make_target_passes_the_issues_and_the_client_through(self):
        """The recipe takes ISSUES from the environment, so a quote or a `$` in it is not the shell's."""
        tree = self.make_tree()
        client = self.root / "bin" / "other-client"
        shutil.copy(self.root / "bin" / "bravebot", client)
        argument = "10, it's \"a\" $$HOME"
        running = self.spawn(["make", "--no-print-directory", "implement-issues", f"ISSUES={argument}", f"BRAVEBOT={client}"], tree)
        result = self.finish(running)
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.session()["client"], "other-client")
        self.assertEqual(self.told()[0], "/implement-issues 10, it's \"a\" $HOME")
        self.assertEqual(self.state()["10"]["assignees"], ["netzenbot"])

    def test_the_make_target_passes_the_notes_through(self):
        """The recipe takes NOTES from the environment, so a quote or a `$` in it is not the shell's, and
        `--notes=` keeps a note that starts with a dash from being read as a flag."""
        tree = self.make_tree()
        for given, passed in (
            ("group them, it's \"a\" $$HOME", "/implement-issues 10\nNotes: group them, it's \"a\" $HOME"),
            ("-quickly", "/implement-issues 10\nNotes: -quickly"),
            (None, "/implement-issues 10"),
        ):
            notes = [f"NOTES={given}"] if given is not None else []
            running = self.spawn(["make", "--no-print-directory", "implement-issues", "ISSUES=10", *notes], tree)
            result = self.finish(running)
            self.assertEqual(result.code, 0, result.err)
            self.assertEqual(self.told()[0], passed)

    def test_the_make_target_forces_a_run_only_for_force_1(self):
        """FORCE=0 and an empty FORCE are the likeliest ways to mean no, and `$(if)` alone reads both as yes."""
        tree = self.make_tree()
        self.world["issues"]["10"] = issue(assignees=["alice"])
        for given in ("0", "", "yes"):
            result = self.finish(self.spawn(["make", "--no-print-directory", "implement-issues", "ISSUES=10", f"FORCE={given}"], tree))
            self.assertNotEqual(result.code, 0, given)
            self.assertIn("#10 is assigned to alice", result.err)
            self.assertIsNone(self.session(), given)
        result = self.finish(self.spawn(["make", "--no-print-directory", "implement-issues", "ISSUES=10", "FORCE=1"], tree))
        self.assertEqual(result.code, 0, result.err)
        self.assertEqual(self.state()["10"]["assignees"], ["alice", "netzenbot"])

    def test_the_make_target_bypasses_only_for_bypass_1(self):
        """BYPASS=0 and an empty BYPASS are the likeliest ways to mean no, and `$(if)` alone reads both as yes,
        which would turn the sandbox off."""
        tree = self.make_tree()
        for given, flags in (("0", ["-i"]), ("", ["-i"]), ("yes", ["-i"]), ("1", ["--dangerously-bypass-approvals-and-sandbox", "-i"])):
            result = self.finish(self.spawn(["make", "--no-print-directory", "implement-issues", "ISSUES=10", f"BYPASS={given}"], tree))
            self.assertEqual(result.code, 0, result.err)
            self.assertEqual(self.session()["argv"][:-1], flags, given)
            self.session_path.unlink()

    def test_a_client_the_environment_names_is_not_the_one_the_make_target_runs(self):
        """brave/bravebot#3591: the recipe takes BRAVEBOT from the environment, so the notes test opened a
        person's real client on its terminal and waited for it for the full minute."""
        done = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), "ImplementIssues.test_the_make_target_passes_the_notes_through"],
            env=dict(os.environ, BRAVEBOT=str(self.root / "no-such-client")),
            capture_output=True, text=True, timeout=120,
        )
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_a_command_that_runs_past_its_time_is_ended_with_everything_it_started(self):
        """A recipe's children are not the child `communicate` gives up on, and a hang that left them
        running was the whole of what kept thirty clients spinning for a day and a half."""
        started = self.root / "grandchild"
        running = self.spawn(["sh", "-c", f"sleep 300 & echo $! > '{started}'; wait"], self.root, tty=False)
        until = time.monotonic() + 10
        while time.monotonic() < until and not (started.exists() and started.read_text().strip()):
            time.sleep(0.05)
        grandchild = int(started.read_text())
        with self.assertRaises(subprocess.TimeoutExpired):
            self.finish(running, timeout=1)
        until = time.monotonic() + 10
        while time.monotonic() < until and self.alive(grandchild):
            time.sleep(0.05)
        self.assertFalse(self.alive(grandchild), "what the command started outlived the run")
        self.assertIsNotNone(running.process.poll())

    @staticmethod
    def alive(pid):
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        return True

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
