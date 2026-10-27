#!/usr/bin/env python3
"""Open the implement-issues skill in the terminal client, after the steps that need no model.

    python3 agents/skills/implement-issues/implement-issues.py [--bravebot PATH] -- <argument>

`make implement-issues ISSUES=<argument>` runs it. <argument> is what the skill takes: issue numbers,
links and search queries separated by commas. The skill receives it unchanged, since the session is
`bravebot -i "/implement-issues <argument>"` and opens on that line as its first turn.

Before the session opens, from the same reading of <argument> as step 1 of the skill, and with no
model:

1. every open issue is read, and the run stops with nothing changed when one is assigned to anybody
   but the account `gh` writes as (`gh api user`);
2. that account is assigned to each of them;
3. each of them gets the label bot/in-progress.

When the session ends, however it ends, the label is removed from the issues this run added it to.
An issue that had the label already is left as it was found. Pull requests, closed issues and links
into other repositories are not claimed, since the skill leaves them alone too. A batch branch to
resume, or a pull request to give a round of review comments, names no issue to claim: start those
from the client. A kill that cannot be caught leaves the label, and the description of the label
names the sweep that removes it.
"""

import argparse
import concurrent.futures
import importlib.util
import json
import os
import re
import shutil
import signal
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
from pathlib import Path
from urllib.parse import quote

LABEL = "bot/in-progress"
SKILL = "implement-issues"
NAME = re.compile(r"[A-Za-z0-9-]+")
NUMBER = re.compile(r"#?(\d+)")
LINK = re.compile(r"https?://\S+")
# A query that names a state is searched as written, and any other is limited to open issues.
STATE = re.compile(r"\b(?:is|state):(?:open|closed)\b", re.IGNORECASE)
READ = "{state: .state, pull: (.pull_request != null), assignees: [.assignees[].login], labels: [.labels[].name]}"
SEARCH = ".items[] | select(.pull_request == null) | .number"
READ_JOBS = 8
# Statuses after which the skill does nothing more with an item (SKILL.md, step 0), so it needs no claim.
FINISHED = {"done", "skipped", "blocked"}


def _load(name):
    path = Path(__file__).resolve().parents[2] / f"{name}.py"
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The skill scripts are not an importable package, so the modules they share are loaded by path.
pr_worktree = _load("pr_worktree")
issue_helper = _load("issue_helper")

BASE_REPO = pr_worktree.BASE_REPO
die = pr_worktree.die
run = pr_worktree.run


def listed(numbers):
    return ", ".join(f"#{number}" for number in numbers)


def shown(login):
    """A login as printed. One that is not a login is `unknown`, so what GitHub returned cannot reach the screen."""
    return login if NAME.fullmatch(login) else "unknown"


def current_user():
    """The login `gh` writes as: the account that makes the claim, and the one `pr-fix.py open` assigns."""
    done = run("gh", "api", "user", "--jq", ".login", check=False)
    login = done.stdout.strip()
    if done.returncode or not NAME.fullmatch(login):
        die(f"could not read the login gh writes as: {pr_worktree.printed(done) or 'no login returned'}")
    return login


def pieces(argument):
    """The parts of the argument, split on commas and newlines as step 1 of the skill splits it."""
    return [piece.strip() for piece in re.split(r"[,\n]", argument) if piece.strip()]


def ledger(piece):
    """The ledger of the batch branch a piece names, which the skill resumes, or None."""
    if not NAME.fullmatch(piece):
        return None
    path = Path(os.environ.get("TMPDIR") or "/tmp") / SKILL / f"{piece}.md"
    return path if path.is_file() else None


def ledger_items(path):
    """The issue numbers of the items in a ledger that the skill still has work for.

    The ledger is the markdown table of SKILL.md step 0, `item | group | worktree | branch | status |
    commits`, with the issue as `#<n>` in the item cell. The header and separator rows have no such cell.
    """
    numbers, rows = [], 0
    for line in path.read_text(errors="replace").splitlines():
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if not line.strip().startswith("|") or len(cells) < 5:
            continue
        item = NUMBER.fullmatch(cells[0].strip("`"))
        if not item:
            continue
        rows += 1
        if cells[4].strip("`").lower() not in FINISHED:
            numbers.append(int(item[1]))
    if not rows:
        die(f"{path} has no row whose item is #<number>, so the issues of the run cannot be read. Nothing was changed.")
    return numbers


def named(word, notes):
    """The number of the brave/bravebot issue or pull request a number or link names, or None."""
    bare = NUMBER.fullmatch(word)
    if bare:
        return int(bare[1])
    for url in (pr_worktree.PULL_URL, pr_worktree.ISSUE_URL):
        found = url.search(word)
        if found:
            if f"{found[1]}/{found[2]}".lower() == BASE_REPO:
                return int(found[3])
            break
    notes.append(f"{word} is not in {BASE_REPO}")
    return None


def search(query):
    """The numbers of the issues a query finds, which is what the skill searches for."""
    scope = f"repo:{BASE_REPO} {query}" + ("" if STATE.search(query) else " is:open")
    found = run(
        "gh", "api", "-X", "GET", "search/issues", "-f", f"q={scope}", "-f", "per_page=100", "--paginate",
        "--jq", SEARCH,
    ).stdout
    return [int(number) for number in found.split()]


def resolve(argument):
    """The numbers the skill may work, and a note for each link it leaves alone."""
    numbers, notes = [], []
    for piece in pieces(argument):
        resumed = ledger(piece)
        if resumed:
            numbers.extend(ledger_items(resumed))
            continue
        words = piece.split()
        if all(NUMBER.fullmatch(word) or LINK.fullmatch(word) for word in words):
            numbers.extend(number for number in (named(word, notes) for word in words) if number)
        else:
            numbers.extend(search(piece))
    return sorted(set(numbers)), notes


def read(number):
    return json.loads(run("gh", "api", f"repos/{BASE_REPO}/issues/{number}", "--jq", READ).stdout)


def open_issues(numbers):
    """The open issues among the numbers, each as GitHub holds it now, and a note for each one left alone.

    A search can lag behind an assignment, so the issues are read one by one even where a search found
    them, and it is that reading the check of the assignees is made on.
    """
    with concurrent.futures.ThreadPoolExecutor(max_workers=READ_JOBS) as pool:
        read_all = dict(zip(numbers, pool.map(read, numbers)))
    issues, notes = {}, []
    for number, issue in read_all.items():
        if issue["pull"]:
            notes.append(f"#{number} is a pull request")
        elif issue["state"] != "open":
            notes.append(f"#{number} is {issue['state']}")
        else:
            issues[number] = issue
    return issues, notes


def assigned_to(issue, login):
    return login.lower() in {one.lower() for one in issue["assignees"]}


def conflicts(issues, me):
    """A line for each issue assigned to somebody other than `me`."""
    lines = []
    for number, issue in sorted(issues.items()):
        others = [shown(login) for login in issue["assignees"] if login.lower() != me.lower()]
        if others:
            lines.append(f"#{number} is assigned to {', '.join(others)}")
    return lines


def check_repository(me):
    """Stop where a write would not do what it says: the label is missing, so adding it would create it,
    or the login cannot be assigned, so GitHub would drop it without an error."""
    try:
        has_label = LABEL in issue_helper.existing_labels(BASE_REPO)
        can_assign = issue_helper.assignable(BASE_REPO, me)
    except RuntimeError as problem:
        die(f"could not read {BASE_REPO}: {problem}")
    if not has_label:
        die(f"{BASE_REPO} has no label {LABEL}, and adding one that does not exist would create it. Nothing was changed.")
    if not can_assign:
        die(f"{BASE_REPO} would not take {me} as an assignee. Nothing was changed.")


def api(method, path, *fields):
    """A `gh api` write, and the reason it failed, or None."""
    done = run("gh", "api", "--method", method, path, *fields, check=False)
    if done.returncode:
        return pr_worktree.printed(done) or f"gh exited {done.returncode}"
    return None


def release(labelled):
    """Remove the label from the issues it was added to, and return the numbers it could not be removed from."""
    stuck = []
    for number in labelled:
        problem = api("DELETE", f"repos/{BASE_REPO}/issues/{number}/labels/{quote(LABEL, safe='')}")
        if problem:
            stuck.append(number)
            print(
                f"could not remove {LABEL} from #{number}: {problem}\n"
                f"remove it with: gh issue edit {number} --repo {BASE_REPO} --remove-label {LABEL}",
                file=sys.stderr,
            )
    return stuck


def undo(assigned, labelled, me):
    release(labelled)
    for number in assigned:
        problem = api("DELETE", f"repos/{BASE_REPO}/issues/{number}/assignees", "-f", f"assignees[]={me}")
        if problem:
            print(f"could not unassign {me} from #{number}: {problem}", file=sys.stderr)


def claim(issues, me):
    """Assign `me` to each issue that lacks them and label each that lacks the label.

    Returns the numbers assigned and the numbers labelled. When a write fails, or the run is
    interrupted, what was written is undone first.
    """
    assigned, labelled = [], []
    try:
        for number, issue in issues.items():
            path = f"repos/{BASE_REPO}/issues/{number}"
            if not assigned_to(issue, me):
                problem = api("POST", f"{path}/assignees", "-f", f"assignees[]={me}")
                if problem:
                    raise RuntimeError(f"could not assign {me} to #{number}: {problem}")
                assigned.append(number)
            if LABEL not in issue["labels"]:
                problem = api("POST", f"{path}/labels", "-f", f"labels[]={LABEL}")
                if problem:
                    raise RuntimeError(f"could not add {LABEL} to #{number}: {problem}")
                labelled.append(number)
    except BaseException:
        undo(assigned, labelled, me)
        raise
    return assigned, labelled


def session(command):
    """Run the client on this terminal until it ends, and return its exit status.

    A termination or hangup is passed to the client and waited out, so the label is removed after the
    client has gone. Ctrl-C reaches the client through the terminal and ends this run only by ending it.
    """
    child = []

    def pass_on(number, frame):
        if not child:
            raise SystemExit(128 + number)
        child[0].send_signal(number)

    previous = {number: signal.signal(number, pass_on) for number in (signal.SIGTERM, signal.SIGHUP)}
    previous[signal.SIGINT] = signal.signal(signal.SIGINT, lambda number, frame: None)
    try:
        try:
            child.append(subprocess.Popen(command))
        except OSError as problem:
            die(f"could not start {command[0]}: {problem}")
        status = child[0].wait()
    finally:
        for number, handler in previous.items():
            signal.signal(number, handler)
    return 128 - status if status < 0 else status


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bravebot", default="bravebot", help="the terminal client to open, by name or path")
    parser.add_argument("argument", help="what the skill takes, which it receives unchanged")
    args = parser.parse_args(argv)
    if not args.argument.strip():
        die("name the issues to implement, or a search for them. Nothing was changed.")
    if not (sys.stdin.isatty() and sys.stdout.isatty()):
        die("the session needs a terminal on stdin and stdout. Nothing was changed.")
    client = shutil.which(args.bravebot)
    if client is None:
        die(f"{args.bravebot} is not on PATH. Nothing was changed.")
    me = current_user()

    numbers, notes = resolve(args.argument)
    issues, more = open_issues(numbers)
    if notes or more:
        print(f"left alone: {'; '.join(notes + more)}", flush=True)
    if not issues:
        die("no open issue to implement. Nothing was changed.")
    taken = conflicts(issues, me)
    if taken:
        die("\n".join([*taken, "", "Nothing was changed."]))
    check_repository(me)

    try:
        assigned, labelled = claim(issues, me)
    except RuntimeError as problem:
        die(f"{problem}\nWhat had been written was undone.")
    print(f"issues: {listed(issues)}", flush=True)
    print(f"assigned {me}: {listed(assigned) or 'none, already theirs'}", flush=True)
    print(f"labelled {LABEL}: {listed(labelled) or 'none, already labelled'}", flush=True)

    status = 1
    try:
        status = session([client, "-i", f"/{SKILL} {args.argument}"])
    finally:
        stuck = release(labelled)
        if labelled and not stuck:
            print(f"removed {LABEL} from {listed(labelled)}", flush=True)
    return status or int(bool(stuck))


if __name__ == "__main__":
    sys.exit(main())
