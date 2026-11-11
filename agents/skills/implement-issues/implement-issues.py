#!/usr/bin/env python3
"""Open the implement-issues skill in the terminal client, after the steps that need no model.

    python3 agents/skills/implement-issues/implement-issues.py [--bravebot PATH] [--notes TEXT] [--force] [--bypass] [--platform NAME] -- <argument>

`make implement-issues ISSUES=<argument> [NOTES=<notes>] [FORCE=1] [BYPASS=1]` runs it. <argument> is what the skill takes:
issue numbers, links and search queries separated by commas. The skill receives it unchanged, since the
session is `bravebot -i "/implement-issues <argument>"` and opens on that line as its first turn.
With `--bypass` it is `bravebot --dangerously-bypass-approvals-and-sandbox -i "..."`, which answers every
permission question and turns the sandbox off for the whole session.

The notes are the person's instructions for the run, such as how to group the issues. They begin at a
line of <argument> that starts with `Notes:`, and `--notes` adds such a line after <argument>. The skill
reads them (step 1). This script takes no issue, link or query from them, and they do not change which
issues it claims.

Once the claim is written, the session is given a line that starts with `Claimed:`, between <argument>
and the notes. It says when the script assigned the account and added the label, to which issues, and
that those writes are this run's, so the skill does not read them as another session's work. It holds
the account, the time and issue numbers, and nothing that GitHub returned. With `--force`, it also
names the issues that have another assignee and the issues labelled for another platform. A line of
<argument> that starts with `Claimed:` ends the list as one that starts with `Notes:` does.

Before the session opens, from the same reading of <argument> as step 1 of the skill, and with no
model:

1. every open issue is read, and the run stops with nothing changed when one is assigned to anybody
   but the account `gh` writes as (`gh api user`), or when one is labelled develop-on/linux,
   develop-on/macos or develop-on/windows and none of its develop-on labels names the platform this
   host is (`--platform` names another, for tests), unless `--force` is given, which works those issues
   anyway and leaves the other assignees and the labels where they are. An issue with no develop-on
   label can be developed on any host;
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
import datetime
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
# The labels triage-issues puts on an issue that can only be developed on one platform, by the name
# sys.platform gives that host. An issue with none of them can be developed anywhere.
DEVELOP_ON = "develop-on/"
HOSTS = {"darwin": "macos", "linux": "linux", "win32": "windows"}
PLATFORMS = sorted(HOSTS.values())
SKILL = "implement-issues"
# The client's flag for no permission prompts and no sandbox, which `--bypass` puts ahead of `-i`.
BYPASS_FLAG = "--dangerously-bypass-approvals-and-sandbox"
NAME = re.compile(r"[A-Za-z0-9-]+")
NUMBER = re.compile(r"#?(\d+)")
LINK = re.compile(r"https?://\S+")
# A query that names a state is searched as written, and any other is limited to open issues.
STATE = re.compile(r"\b(?:is|state):(?:open|closed)\b", re.IGNORECASE)
READ = "{state: .state, pull: (.pull_request != null), assignees: [.assignees[].login], labels: [.labels[].name]}"
SEARCH = ".items[] | select(.pull_request == null) | .number"
READ_JOBS = 8
# The issues of an argument end at the first line that starts with this: the notes run from there to the
# end of the argument, and the claim line is the one this script writes ahead of them.
LIST_ENDS = re.compile(r"^(?:Notes|Claimed):", re.MULTILINE)
# Statuses after which the skill does nothing more with an item (SKILL.md, step 0), so it needs no claim.
FINISHED = {"done", "skipped", "blocked"}
# What GitHub answers (HTTP 404) when the label is not on the issue, which the session may have removed already.
LABEL_GONE = "Label does not exist"


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


def split_list(argument):
    """The part of the argument that names the issues, and what follows it, or an empty string."""
    found = LIST_ENDS.search(argument)
    return (argument[: found.start()], argument[found.start():]) if found else (argument, "")


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


def host_platform():
    """The name of the platform this script runs on, as the develop-on labels spell it."""
    return HOSTS.get(sys.platform, sys.platform)


def elsewhere(issues, here):
    """The develop-on labels of each issue that can only be developed on a platform other than `here`.

    An issue that has a develop-on label for `here` is fine, whatever else it has, and so is one with
    none. Only the three labels triage-issues applies count, and they are written from the constants
    here, so no label text that GitHub returned is printed.
    """
    known = {DEVELOP_ON + name: DEVELOP_ON + name for name in PLATFORMS}
    found = {}
    for number, issue in sorted(issues.items()):
        wanted = sorted({known[label.lower()] for label in issue["labels"] if label.lower() in known})
        if wanted and DEVELOP_ON + here not in wanted:
            found[number] = wanted
    return found


def conflicts(issues, me):
    """The logins, as printed, of everybody but `me` that each issue is assigned to, for the issues with any."""
    found = {}
    for number, issue in sorted(issues.items()):
        others = [shown(login) for login in issue["assignees"] if login.lower() != me.lower()]
        if others:
            found[number] = others
    return found


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
    """Remove the label from the issues it was added to, and return the numbers it could not be removed from.

    An issue that no longer has the label is not one of them: the label is off, which is what was asked.
    """
    stuck = []
    for number in labelled:
        problem = api("DELETE", f"repos/{BASE_REPO}/issues/{number}/labels/{quote(LABEL, safe='')}")
        if problem and LABEL_GONE not in problem:
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


def stamp():
    """The time as a UTC timestamp to the second, which is how the claim line dates the writes."""
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def claim_line(me, when, issues, assigned, labelled, forced, away=None):
    """The line that tells the session which assignments and labels this run wrote, and when.

    `pr-fix.py issue` prints the labels of an issue, and a session that finds `bot/in-progress` on an
    issue it was given has stopped on it as another session's work, although this script added it. The
    line holds the account, the time and issue numbers only, so no text that GitHub returned reaches the
    model.
    """
    wrote = []
    if assigned:
        wrote.append(f"assigned {me} to {listed(assigned)}")
    if labelled:
        wrote.append(f"added {LABEL} to {listed(labelled)}")
    said = [f"Claimed: at {when}, before this session opened, implement-issues.py {' and '.join(wrote) or 'had nothing to write'}."]
    if wrote:
        said.append("Those writes are this run's own, so they are no reason to skip or stop on an item.")
    before = [number for number, issue in issues.items() if LABEL in issue["labels"]]
    if before:
        said.append(f"{LABEL} was on {listed(before)} before this run and was left as found. That does not make an item anyone else's.")
    if forced:
        said.append(
            f"The person forced this run past other assignees (FORCE=1): somebody else is also assigned to "
            f"{listed(forced)}. That is no reason to skip or stop on an item."
        )
    if away:
        each = "; ".join(listed([number]) + " is labelled " + ", ".join(labels) for number, labels in away.items())
        said.append(
            f"The person forced this run past the platform labels (FORCE=1): {each}. "
            "That is no reason to skip or stop on an item."
        )
    return " ".join(said)


def prompt_for(argument, notes, claimed):
    """The line the client opens on: the argument as written with the claim line after its list, then the notes.

    The claim goes where the notes begin, so that the lines before it are still the list and the notes
    are still everything from the first `Notes:` line to the end.
    """
    named_in, rest = split_list(argument)
    prompt = f"/{SKILL} {named_in.rstrip(chr(10))}\n{claimed}"
    if rest:
        prompt += f"\n{rest}"
    if notes.strip():
        prompt += f"\nNotes: {notes.strip()}"
    return prompt


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
    parser.add_argument("--notes", default="", help="notes for the skill, which reads them; no issue is taken from them")
    parser.add_argument("--force", action="store_true", help="work an issue that is assigned to somebody else as well")
    parser.add_argument("--bypass", action="store_true", help=f"open the client with {BYPASS_FLAG}")
    parser.add_argument("--platform", choices=PLATFORMS, help="the platform develop-on labels are checked against (default: this host's)")
    parser.add_argument("argument", help="what the skill takes, which it receives unchanged")
    args = parser.parse_args(argv)
    named_in, _ = split_list(args.argument)
    if not named_in.strip():
        die("name the issues to implement, or a search for them. Nothing was changed.")
    if not (sys.stdin.isatty() and sys.stdout.isatty()):
        die("the session needs a terminal on stdin and stdout. Nothing was changed.")
    client = shutil.which(args.bravebot)
    if client is None:
        die(f"{args.bravebot} is not on PATH. Nothing was changed.")
    me = current_user()

    numbers, notes = resolve(named_in)
    issues, more = open_issues(numbers)
    if notes or more:
        print(f"left alone: {'; '.join(notes + more)}", flush=True)
    if not issues:
        die("no open issue to implement. Nothing was changed.")
    taken = conflicts(issues, me)
    here = args.platform or host_platform()
    away = elsewhere(issues, here)
    lines = [f"#{number} is assigned to {', '.join(others)}" for number, others in taken.items()]
    away_lines = [f"#{number} is labelled {', '.join(labels)} and this host is {here}" for number, labels in away.items()]
    if (taken or away) and not args.force:
        die("\n".join([*lines, *away_lines, "", "Nothing was changed. FORCE=1 (--force) works them anyway."]))
    if taken:
        print("\n".join(["working past another assignee, as asked:", *lines]), flush=True)
    if away:
        print("\n".join(["working past the platform labels, as asked:", *away_lines]), flush=True)
    check_repository(me)

    try:
        assigned, labelled = claim(issues, me)
    except RuntimeError as problem:
        die(f"{problem}\nWhat had been written was undone.")
    prompt = prompt_for(args.argument, args.notes, claim_line(me, stamp(), issues, assigned, labelled, list(taken), away))
    print(f"issues: {listed(issues)}", flush=True)
    print(f"assigned {me}: {listed(assigned) or 'none, already theirs'}", flush=True)
    print(f"labelled {LABEL}: {listed(labelled) or 'none, already labelled'}", flush=True)

    command = [client, *([BYPASS_FLAG] if args.bypass else []), "-i", prompt]
    if args.bypass:
        print(f"bypass: the session runs with {BYPASS_FLAG}, so nothing asks before a command and no sandbox confines it", flush=True)
    status = 1
    try:
        status = session(command)
    finally:
        stuck = release(labelled)
        if labelled and not stuck:
            print(f"removed {LABEL} from {listed(labelled)}", flush=True)
    return status or int(bool(stuck))


if __name__ == "__main__":
    sys.exit(main())
