#!/usr/bin/env python3
"""Bring a brave/bravebot pull request up to date and green, in a worktree of its own.

    pr-fix.py start <pr>                fetch, check the head out in .worktrees/pr-<pr>, rebase
    pr-fix.py continue <pr>             stage the resolved files and carry on
    pr-fix.py ci <pr> [--wait]          the checks GitHub ran on the head, with the log of each failure
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
parallel, each in a process of its own. It works from the main clone or from any worktree.
"""

import argparse
import concurrent.futures
import hashlib
import importlib.util
import json
import os
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


def _load_shared():
    path = Path(__file__).resolve().parents[2] / "pr_worktree.py"
    spec = importlib.util.spec_from_file_location("pr_worktree", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The skill scripts are not an importable package, so the module they share is loaded by path.
pr_worktree = _load_shared()

BASE_REPO = pr_worktree.BASE_REPO
GITHUB_URL = pr_worktree.GITHUB_URL
PULL_URL = pr_worktree.PULL_URL
OURS = pr_worktree.OURS
THEIRS = pr_worktree.THEIRS
MAKE_FAILED = pr_worktree.MAKE_FAILED
SPEC_RECORDS = pr_worktree.SPEC_RECORDS
LOCK_RESOLVES = pr_worktree.LOCK_RESOLVES
die = pr_worktree.die
run = pr_worktree.run
git = pr_worktree.git
succeeds = pr_worktree.succeeds
pr_number = pr_worktree.pr_number
github_remotes = pr_worktree.github_remotes
fork_url = pr_worktree.fork_url
local_branch = pr_worktree.local_branch
marker_ranges = pr_worktree.marker_ranges
worktrees = pr_worktree.worktrees
fetch = pr_worktree.fetch
rev = pr_worktree.rev
rebase_dir = pr_worktree.rebase_dir
unmerged = pr_worktree.unmerged
resolved_log = pr_worktree.resolved_log
resolved = pr_worktree.resolved
remember_resolved = pr_worktree.remember_resolved
package_of = pr_worktree.package_of
plan = pr_worktree.plan
describe = pr_worktree.describe
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
REASON_LINES = 3
WIDTH = 300
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


def stop_reason(text):
    """What git said when a rebase stopped, from its first error on, without the hints that follow."""
    lines = [one.strip() for one in re.split(r"[\r\n]+", text)]
    lines = [one for one in lines if one and not one.startswith(("hint:", "Rebasing ("))]
    first = next((n for n, one in enumerate(lines) if one.startswith(("error:", "fatal:"))), len(lines) - 1)
    return [one[:WIDTH] for one in lines[max(first, 0) :][:REASON_LINES]]


def report(pr, said=""):
    tree = pr.tree
    print(f"worktree {tree}")
    state = rebase_dir(tree)
    if state is None:
        if not succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
            die(f"{pr.branch} is not on {pr.onto}; `{SCRIPT} start {pr.number}`")
        tip, now = rev(pr.tip, tree), rev("HEAD", tree)
        if now == tip:
            print(f"{pr.url} is already on {pr.onto}; nothing to push")
            return 0
        count = git("rev-list", "--count", f"{pr.onto}..HEAD", cwd=tree)
        print(f"{count} commit(s) on {pr.onto} {rev(pr.onto, tree)[:8]}: {tip[:8]} -> {now[:8]}")
        print(f"next: {SCRIPT} push {pr.number}")
        return 0

    def read(name):
        path = state / name
        return path.read_text().strip() if path.is_file() else ""

    number, total = (read("msgnum"), read("end")) if (state / "msgnum").is_file() else (read("next"), read("last"))
    step = f" at {number}/{total}" if number and total else ""
    stopped = run("git", "log", "-1", "--format=%h %s", "REBASE_HEAD", cwd=tree, check=False)
    print(f"stopped{step}: {stopped.stdout.strip()}")
    files = unmerged(tree)
    if not files:
        reason = stop_reason(said)
        print("stopped without a conflict" + ("; git said:" if reason else ""))
        for line in reason:
            print(f"  {line}")
        print(f"see `git -C {tree} status`")
        return 1
    print("conflicts:")
    for name in files:
        print(f"  {describe(tree, name)}")
    onto, orig = read("onto"), read("orig-head")
    fork = run("git", "merge-base", orig, onto, cwd=tree, check=False).stdout.strip()
    if fork:
        touching = git("log", "--oneline", "--no-decorate", "-n", "10", f"{fork}..{onto}", "--", *files, cwd=tree)
        if touching:
            print(f"{pr.onto} commits on these files:")
            print("\n".join(f"  {line}" for line in touching.splitlines()))
    print(f"resolve each range, then: {SCRIPT} continue {pr.number}")
    return 1


def start(pr):
    git("worktree", "prune", cwd=pr.here)
    fetch(pr.here, pr.base_remote, pr.base)
    fetch(pr.here, pr.head_remote, pr.head)
    tree = pr.tree
    if not pr.checked_out:
        if tree.exists():
            die(f"{tree} exists and is not a worktree on {pr.branch}")
        if succeeds("rev-parse", "--verify", "--quiet", f"refs/heads/{pr.branch}", cwd=pr.here):
            git("worktree", "add", str(tree), pr.branch, cwd=pr.here)
        else:
            git("worktree", "add", "-b", pr.branch, str(tree), pr.tip, cwd=pr.here)
    shared.release()
    if rebase_dir(tree):
        return report(pr)
    if git("status", "--porcelain", "--untracked-files=no", cwd=tree):
        die(f"{tree} has uncommitted changes")

    def subjects(ref):
        return set(git("log", "--format=%s", f"{pr.onto}..{ref}", cwd=tree).splitlines())

    if succeeds("merge-base", "--is-ancestor", "HEAD", pr.tip, cwd=tree):
        git("merge", "--ff-only", "--quiet", pr.tip, cwd=tree)
    elif succeeds("merge-base", "--is-ancestor", pr.tip, "HEAD", cwd=tree):
        print(f"{pr.branch} has commits the pull request does not; they are kept")
    elif not subjects(pr.tip) <= subjects("HEAD"):
        die(f"{pr.branch} in {tree} and {pr.tip} have diverged")
    said = ""
    if not succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
        resolved_log(tree).unlink(missing_ok=True)
        done = run("git", "rebase", pr.onto, cwd=tree, check=False)
        if done.returncode and rebase_dir(tree) is None:
            die(f"git rebase {pr.onto} failed:\n{(done.stderr or done.stdout).strip()}")
        said = done.stderr
    if rebase_dir(tree) is None:
        mark_base(tree)
    return report(pr, said)


def resume(pr):
    tree = pr.tree
    if rebase_dir(tree) is None:
        return report(pr)
    files = unmerged(tree)
    left = [
        one
        for one in files
        if Path(tree, one).is_file()
        and marker_ranges(Path(tree, one).read_text(encoding="utf-8", errors="replace"))
    ]
    if left:
        print("markers remain:")
        for name in left:
            print(f"  {describe(tree, name)}")
        return 1
    if files:
        git("add", "-A", "--", *files, cwd=tree)
        remember_resolved(tree, files)
    done = run(
        "git", "rebase", "--continue", cwd=tree, check=False, env={**os.environ, "GIT_EDITOR": "true"}
    )
    if done.returncode and rebase_dir(tree) is None:
        die(f"git rebase --continue failed:\n{(done.stderr or done.stdout).strip()}")
    if rebase_dir(tree) is None:
        mark_base(tree)
    return report(pr, done.stderr)


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
    tree = pr.tree
    if rebase_dir(tree):
        die(f"finish the rebase first: `{SCRIPT} continue {pr.number}`")
    if git("status", "--porcelain", "--untracked-files=no", cwd=tree):
        die(f"{tree} has uncommitted changes; commit them first")
    if not succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
        die(f"{pr.branch} is not on {pr.onto}; `{SCRIPT} start {pr.number}`")
    tip, now = rev(pr.tip, tree), rev("HEAD", tree)
    if now == tip:
        print(f"{pr.url} is already at {now[:8]}; nothing to push")
        return 0
    pr_worktree.refuse_unsigning(pr, tip, now, tree)
    pr_worktree.push_head(pr, tree, tip)
    print(f"pushed {tip[:8]} -> {now[:8]} to {pr.tip}\n{pr.url}")
    return 0


def review(pr):
    done = run(
        "gh", "api", "--method", "POST",
        f"repos/{BASE_REPO}/pulls/{pr.number}/requested_reviewers",
        "-f", f"reviewers[]={REVIEWER}",
        check=False,
    )
    if done.returncode:
        print(f"could not request {REVIEWER} on {pr.url}:\n{(done.stderr or done.stdout).strip()}", file=sys.stderr)
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


def ci(pr, wait):
    tree = pr.tree
    if tree.is_dir() and rev("HEAD", tree) != pr.head_sha:
        print(f"{tree} is at {rev('HEAD', tree)[:8]} and GitHub tests {pr.head_sha[:8]}; "
              f"these results are for {pr.head_sha[:8]}")
    rollup, waited = pr.rollup, 0
    while True:
        failing, pending, passing = classify(rollup)
        running = bool(pending) or not rollup
        if failing or not running or not wait or waited >= WAIT_SECONDS:
            break
        time.sleep(POLL_SECONDS)
        waited += POLL_SECONDS
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


def fan_out(step, numbers, flags):
    """Run the step for each pull request in a process of its own, printing each one's output in order."""

    def one(number):
        done = subprocess.run(
            child_command(step, number, flags), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
        )
        return done.returncode, done.stdout

    with concurrent.futures.ThreadPoolExecutor(max_workers=min(len(numbers), MAX_PARALLEL)) as pool:
        running = [pool.submit(one, number) for number in numbers]
        codes = []
        for number, future in zip(numbers, running):
            code, out = future.result()
            print(f"== #{number}")
            print(out.rstrip("\n"))
            codes.append(code)
    return 1 if 1 in codes else max(codes)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("step", choices=["start", "continue", "ci", "comments", "check", "push", "review"])
    parser.add_argument("prs", nargs="+", metavar="pr")
    parser.add_argument("--target", action="append", default=[], help="a make target for check to run")
    parser.add_argument("--wait", action="store_true")
    args = parser.parse_args()
    if args.target and args.step != "check":
        parser.error("only check takes make targets")
    if args.wait and args.step != "ci":
        parser.error("only ci can wait")
    here = Path(git("rev-parse", "--show-toplevel", cwd=None))
    numbers = pr_numbers(args.prs, here)
    if not numbers:
        sys.exit(0)
    if len(numbers) > 1:
        flags = (["--wait"] if args.wait else []) + [f for t in args.target for f in ("--target", t)]
        sys.exit(fan_out(args.step, numbers, flags))
    steps = {
        "start": start,
        "continue": resume,
        "ci": lambda pr: ci(pr, args.wait),
        "comments": comments,
        "check": lambda pr: check(pr, args.target),
        "push": push,
        "review": review,
    }
    try:
        if args.step == "start":
            shared.acquire(here)
        sys.exit(steps[args.step](lookup(numbers[0], create=args.step == "start")))
    finally:
        shared.release()


if __name__ == "__main__":
    main()
