#!/usr/bin/env python3
"""Bring a brave/bravebot pull request up to date and green, in a worktree of its own.

    pr-fix.py start <pr>                fetch, check the head out in .worktrees/pr-<pr>, rebase
    pr-fix.py continue <pr>             stage the resolved files and carry on
    pr-fix.py ci <pr> [--wait]          the checks GitHub ran on the head, with the log of each failure;
                                        --wait polls until one fails or --wait-seconds N (default 540)
                                        have passed since the script started, counting every poll
    pr-fix.py comments <pr>             the review threads and reviews still waiting on a change
    pr-fix.py check <pr> [--target T]   run the checks the changed files call for, or the make
                                        targets named, printing only a failure
    pr-fix.py push <pr>                 push over the head `start` fetched, and nothing newer; refuses to
                                        make a signed pull request unsigned
    pr-fix.py review <pr>               ask netzenbot-reviewer to review the pull request again

Every step that needs no judgement is decided here, so what it prints is only what a model has to
act on: the line ranges of each conflict, the log of each failing job, the text of each open review
comment. <pr> is a number or a pull request URL, several of them separated by commas or spaces, or
`all` for every open pull request in brave/bravebot by the user git is configured as. Several run in
parallel, each in a process of its own, and each one's output is printed as soon as it finishes. Several
that wait share one wait, so they take no longer than one. It works from the main clone or from any
worktree.

A branch already checked out in another worktree is used there and a `reused:` line says where. Each
step that works in the worktree refuses a locked one, which something else may be using, unless given
--use-locked, which takes one pull request.
"""

import argparse
import concurrent.futures
import hashlib
import importlib.util
import json
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

try:
    import fcntl
except ImportError:
    fcntl = None

REVIEWER = "netzenbot-reviewer"
SCRIPT = "python3 agents/skills/pr-fix/pr-fix.py"
TREE_STEPS = ("start", "continue", "check", "push")


def _load_shared():
    path = Path(__file__).resolve().parents[2] / "pr_worktree.py"
    spec = importlib.util.spec_from_file_location("pr_worktree", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The skill scripts are not an importable package, so the module they share is loaded by path.
pr_worktree = _load_shared()

BASE_REPO = pr_worktree.BASE_REPO
die = pr_worktree.die
run = pr_worktree.run
git = pr_worktree.git
pr_number = pr_worktree.pr_number
fetch = pr_worktree.fetch
rev = pr_worktree.rev
rebase_dir = pr_worktree.rebase_dir
resolved = pr_worktree.resolved
plan = pr_worktree.plan
signed = pr_worktree.signed


# What `gh pr view` is asked for differs by script.
FIELDS = "url,state,baseRefName,headRefName,headRefOid,headRepository,headRepositoryOwner,statusCheckRollup"


def pull(number, view, here, remotes, create=False):
    return pr_worktree.pull(number, view, here, remotes, create, SCRIPT)


def lookup(number, create):
    return pr_worktree.lookup(number, create, SCRIPT, FIELDS)
ANSI = re.compile(r"\x1b\[[0-9;]*m")
LOG_PREFIX = re.compile(r"^[^\t]*\t[^\t]*\t\d{4}-\d\d-\d\dT[\d:.]+Z ?")
JOB_URL = re.compile(r"/actions/runs/(\d+)/job/(\d+)")
PASSED = {"SUCCESS", "NEUTRAL", "SKIPPED"}
STILL_RUNNING = {"PENDING", "EXPECTED"}
WAIT_SECONDS = 540
MAX_PARALLEL = 8
POLL_SECONDS = 30
LOG_LINES = 40
WIDTH = pr_worktree.WIDTH
BODY_WIDTH = 1500
THREADS = """
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100) {
        pageInfo { hasNextPage }
        nodes {
          isResolved isOutdated path line originalLine
          comments(first: 20) { nodes { author { login } authorAssociation body } }
        }
      }
      reviews(last: 50) { nodes { author { login } authorAssociation state body } }
    }
  }
}
"""


def lock_path(here):
    """The lock file for the repository `here` belongs to, named from its common git directory.

    It is in the temp directory because on macOS the sandbox refuses a write under .git from any
    program but git. It only has to serialise these scripts, and every worktree of one clone has the
    same common git directory.
    """
    common = Path(here, git("rev-parse", "--git-common-dir", cwd=here)).resolve()
    digest = hashlib.sha256(str(common).encode()).hexdigest()[:16]
    return Path(tempfile.gettempdir()) / f"pr-fix-{digest}.lock"


class SharedRepo:
    """One process at a time changes what every worktree shares: remotes, fetched refs, the worktree list."""

    def __init__(self):
        self.handle = None

    def acquire(self, here):
        if fcntl is None or self.handle:
            return
        self.handle = open(lock_path(here), "w")
        fcntl.flock(self.handle, fcntl.LOCK_EX)

    def release(self):
        if self.handle:
            self.handle.close()
            self.handle = None


shared = SharedRepo()


def base_marker(tree):
    return Path(git("rev-parse", "--absolute-git-dir", cwd=tree)) / "pr-fix-base"


def mark_base(tree):
    """Record where the branch stands once it is on its base, so later work can be told apart."""
    base_marker(tree).write_text(git("rev-parse", "HEAD", cwd=tree), encoding="utf-8")


def changed(tree):
    """What a check is chosen from: the files resolved in the rebase and those changed since."""
    names = set(resolved(tree))
    marker = base_marker(tree)
    if marker.is_file():
        since = marker.read_text(encoding="utf-8").strip()
        names |= {one for one in git("diff", "--name-only", "-z", since, cwd=tree).split("\0") if one}
    names |= {one for one in git("ls-files", "--others", "--exclude-standard", "-z", cwd=tree).split("\0") if one}
    return sorted(names)


def report(pr, said=""):
    return pr_worktree.report(pr, SCRIPT, said)


def start(pr):
    pr_worktree.announce(pr)
    pr_worktree.add_worktree(pr)
    shared.release()
    tree = pr.tree
    if rebase_dir(tree):
        return report(pr)
    pr_worktree.catch_up_with_pull(pr, tree)
    said = pr_worktree.rebase_onto(pr, tree)
    if rebase_dir(tree) is None:
        mark_base(tree)
    return report(pr, said)


def resume(pr):
    pr_worktree.announce(pr)
    tree = pr.tree
    if rebase_dir(tree) is None:
        return report(pr)
    said = pr_worktree.resolve_or_stop(tree)
    if said is None:
        return 1
    if rebase_dir(tree) is None:
        mark_base(tree)
    return report(pr, said)


def check(pr, targets):
    tree = pr.tree
    if rebase_dir(tree):
        die(f"finish the rebase first: `{SCRIPT} continue {pr.number}`")
    commands = [["make", "-k", *targets]] if targets else plan(tree, changed(tree))
    if not commands:
        print("nothing changed needs a local check; CI runs the rest")
        return 0
    return pr_worktree.run_commands(tree, commands, "pr-fix-check.log")


def push(pr):
    return pr_worktree.push(pr, SCRIPT)


def review(pr):
    done = run(
        "gh", "api", "--method", "POST",
        f"repos/{BASE_REPO}/pulls/{pr.number}/requested_reviewers",
        "-f", f"reviewers[]={REVIEWER}",
        check=False,
    )
    if done.returncode:
        print(f"could not request {REVIEWER} on {pr.url}:\n{pr_worktree.printed(done)}", file=sys.stderr)
        return 1
    print(f"requested {REVIEWER} on {pr.url}")
    return 0


def classify(rollup):
    """Split a pull request's checks into (failing, pending, passing), each as (name, link, verdict)."""
    failing, pending, passing = [], [], []
    for one in rollup:
        if one.get("__typename") == "StatusContext":
            name, link, verdict = one["context"], one.get("targetUrl") or "", one["state"]
            bucket = passing if verdict == "SUCCESS" else pending if verdict in STILL_RUNNING else failing
        else:
            workflow = one.get("workflowName")
            name = f"{workflow} / {one['name']}" if workflow else one["name"]
            link = one.get("detailsUrl") or ""
            if one.get("status") != "COMPLETED":
                verdict, bucket = one.get("status", "PENDING"), pending
            else:
                verdict = one.get("conclusion") or "UNKNOWN"
                bucket = passing if verdict in PASSED else failing
        bucket.append((name, link, verdict))
    return failing, pending, passing


def tail(text, count=LOG_LINES):
    cleaned = (LOG_PREFIX.sub("", ANSI.sub("", one)) for one in text.splitlines())
    lines = [one[:WIDTH] for one in cleaned if one.strip()]
    return "\n".join(lines[-count:])


def failure_log(link):
    job = JOB_URL.search(link)
    if job is None:
        return ""
    done = run(
        "gh", "run", "view", job[1], "--repo", BASE_REPO, "--job", job[2], "--log-failed", check=False
    )
    return tail(done.stdout or done.stderr)


def ci(pr, until):
    """Report the checks on the head. With `until`, a `time.monotonic()` value, poll for them to settle until then."""
    tree = pr.tree
    if tree.is_dir() and rev("HEAD", tree) != pr.head_sha:
        print(f"{tree} is at {rev('HEAD', tree)[:8]} and GitHub tests {pr.head_sha[:8]}; "
              f"these results are for {pr.head_sha[:8]}")
    rollup = pr.rollup
    while True:
        failing, pending, passing = classify(rollup)
        running = bool(pending) or not rollup
        left = None if until is None else until - time.monotonic()
        if failing or not running or left is None or left <= 0:
            break
        time.sleep(min(POLL_SECONDS, left))
        rollup = json.loads(
            run("gh", "pr", "view", str(pr.number), "--repo", BASE_REPO, "--json", "statusCheckRollup").stdout
        ).get("statusCheckRollup") or []
    print(f"{len(passing)} passed, {len(failing)} failed, {len(pending)} pending")
    for name, link, verdict in failing:
        print(f"failed ({verdict.lower()}): {name} {link}")
        log = failure_log(link)
        if log:
            print("\n".join(f"    {line}" for line in log.splitlines()))
    for name, link, verdict in pending:
        print(f"pending: {name} {link}")
    if not rollup:
        print("GitHub has reported no checks for this head yet")
    return 1 if failing else 2 if pending or not rollup else 0


def quoted(node):
    author = node["author"]
    who = f"{author['login']}, {node['authorAssociation'].lower()}" if author else "a deleted account"
    text = node["body"].strip()
    text = text if len(text) <= BODY_WIDTH else text[:BODY_WIDTH] + " [cut]"
    return f"    {who}:\n" + "\n".join(f"      {line}" for line in text.splitlines())


def open_comments(data):
    """The unresolved threads and the latest change-asking review of each reviewer, as lines."""
    request = data["data"]["repository"]["pullRequest"]
    lines = []
    threads = request["reviewThreads"]
    if threads["pageInfo"]["hasNextPage"]:
        lines.append("more than 100 review threads; only the first 100 are shown")
    for thread in threads["nodes"]:
        if thread["isResolved"]:
            continue
        where = thread["path"] + (f":{thread['line'] or thread['originalLine']}" if thread["line"] or thread["originalLine"] else "")
        lines.append(f"thread on {where}" + (" (outdated: the code it was on has changed)" if thread["isOutdated"] else ""))
        lines.extend(quoted(c) for c in thread["comments"]["nodes"])
    latest = {}
    for review in request["reviews"]["nodes"]:
        latest[review["author"]["login"] if review["author"] else ""] = review
    for review in latest.values():
        if review["state"] in ("CHANGES_REQUESTED", "COMMENTED") and review["body"].strip():
            lines.append(f"review ({review['state'].lower().replace('_', ' ')})")
            lines.append(quoted(review))
    return lines


def comments(pr):
    owner, name = BASE_REPO.split("/")
    data = json.loads(
        run(
            "gh", "api", "graphql", "-f", f"query={THREADS}", "-F", f"owner={owner}", "-F", f"name={name}",
            "-F", f"number={pr.number}",
        ).stdout
    )
    lines = open_comments(data)
    if not lines:
        print("no review comment is waiting on a change")
        return 0
    print("\n".join(lines))
    return 1


def my_pulls(here):
    login = git("config", "user.name", cwd=here)
    if not login:
        die("git has no user.name, so there is no author to list pull requests for")
    listing = run(
        "gh", "pr", "list", "--repo", BASE_REPO, "--author", login, "--state", "open", "--limit", "100",
        "--json", "number",
    ).stdout
    numbers = sorted(one["number"] for one in json.loads(listing))
    if not numbers:
        print(f"no open {BASE_REPO} pull request by {login}")
    return numbers


def pr_numbers(args, here):
    """The pull requests named, in order and once each. `all` is every open one by the configured user."""
    numbers = []
    for arg in args:
        for word in filter(None, re.split(r"[,\s]+", arg)):
            numbers.extend(my_pulls(here) if word.lower() == "all" else [pr_number(word)])
    return list(dict.fromkeys(numbers))


def child_command(step, number, flags):
    return [sys.executable, str(Path(__file__).resolve()), step, str(number), *flags]


def fan_out(step, numbers, flags, queued=True):
    """Run the step for each pull request in a process of its own, printing each one's output as it finishes."""

    def one(number):
        done = subprocess.run(
            child_command(step, number, flags), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
        )
        return number, done.returncode, done.stdout

    # A wait is never queued: one behind a full pool would start after the others had spent the shared time.
    workers = min(len(numbers), MAX_PARALLEL) if queued else len(numbers)
    codes = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        for future in concurrent.futures.as_completed([pool.submit(one, number) for number in numbers]):
            number, code, out = future.result()
            # Flushed because stdout is a pipe, so a run stopped before the last one settles still returns these.
            print(f"== #{number}\n" + out.rstrip("\n"), flush=True)
            codes.append(code)
    return 1 if 1 in codes else max(codes)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("step", choices=["start", "continue", "ci", "comments", "check", "push", "review"])
    parser.add_argument("prs", nargs="+", metavar="pr")
    parser.add_argument("--target", action="append", default=[], help="a make target for check to run")
    parser.add_argument("--wait", action="store_true")
    parser.add_argument(
        "--wait-seconds", type=int, metavar="N",
        help=f"how long --wait may take, counted from when the script started (default {WAIT_SECONDS})",
    )
    parser.add_argument(
        "--use-locked", action="store_true", help="work in a locked worktree the branch is already checked out in"
    )
    args = parser.parse_args()
    if args.target and args.step != "check":
        parser.error("only check takes make targets")
    if args.wait and args.step != "ci":
        parser.error("only ci can wait")
    if args.wait_seconds is not None and not args.wait:
        parser.error("--wait-seconds needs --wait")
    if args.use_locked and args.step not in TREE_STEPS:
        parser.error(f"only {', '.join(TREE_STEPS[:-1])} and {TREE_STEPS[-1]} work in a worktree")
    seconds = WAIT_SECONDS if args.wait_seconds is None else args.wait_seconds
    until = time.monotonic() + seconds if args.wait else None
    here = Path(git("rev-parse", "--show-toplevel", cwd=None))
    numbers = pr_numbers(args.prs, here)
    if not numbers:
        sys.exit(0)
    if args.use_locked and len(numbers) > 1:
        parser.error("--use-locked takes the one pull request whose locked worktree the person agreed to")
    if len(numbers) > 1:
        flags = [f for t in args.target for f in ("--target", t)]
        if until is not None:
            # Each process counts from its own start, so what is left of the one shared wait is handed over now.
            flags += ["--wait", "--wait-seconds", str(max(int(until - time.monotonic()), 0))]
        sys.exit(fan_out(args.step, numbers, flags, queued=until is None))
    steps = {
        "start": start,
        "continue": resume,
        "ci": lambda pr: ci(pr, until),
        "comments": comments,
        "check": lambda pr: check(pr, args.target),
        "push": push,
        "review": review,
    }
    try:
        if args.step == "start":
            shared.acquire(here)
        pr = lookup(numbers[0], create=args.step == "start")
        if args.step in TREE_STEPS:
            words = [args.step, str(pr.number)] + [f for t in args.target for f in ("--target", t)]
            pr_worktree.refuse_locked(pr, SCRIPT, words, args.use_locked)
        sys.exit(steps[args.step](pr))
    finally:
        shared.release()


if __name__ == "__main__":
    main()
