#!/usr/bin/env python3
"""Rebase a brave/bravebot pull request onto its base, in a worktree of its own.

    rebase.py start <pr>                fetch, check the head out in .worktrees/pr-<pr>, rebase
    rebase.py continue <pr>             stage the resolved files and carry on
    rebase.py check <pr> [target ...]   run the checks the resolved files call for, or the make
                                        targets named, printing only a failure
    rebase.py push <pr>                 push over the head `start` fetched, and nothing newer; refuses to
                                        make a signed pull request unsigned

Every step that needs no judgement is decided here, so what it prints is only what a model has to
act on: the line ranges of each conflict. <pr> is a number or a pull request URL.

A branch already checked out in another worktree is rebased there and a `reused:` line says where.
Every step refuses a locked one, which something else may be using, unless given --use-locked.
"""

import argparse
import importlib.util
import os
import sys
from pathlib import Path

SCRIPT = "python3 agents/skills/rebase/rebase.py"


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


# What `gh pr view` is asked for differs by script.
FIELDS = "url,state,baseRefName,headRefName,headRepository,headRepositoryOwner"


def pull(number, view, here, remotes, create=False):
    return pr_worktree.pull(number, view, here, remotes, create, SCRIPT)


def lookup(number, create):
    return pr_worktree.lookup(number, create, SCRIPT, FIELDS)


def report(pr):
    tree = pr.tree
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
        return path.read_text().strip() if path.is_file() else "?"

    step = f"{read('msgnum')}/{read('end')}" if (state / "msgnum").is_file() else f"{read('next')}/{read('last')}"
    stopped = run("git", "log", "-1", "--format=%h %s", "REBASE_HEAD", cwd=tree, check=False)
    print(f"stopped at {step}: {stopped.stdout.strip()}")
    files = unmerged(tree)
    if not files:
        print(f"stopped without a conflict; see `git -C {tree} status`")
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
    pr_worktree.announce(pr)
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
    if not succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
        resolved_log(tree).unlink(missing_ok=True)
        done = run("git", "rebase", pr.onto, cwd=tree, check=False)
        if done.returncode and rebase_dir(tree) is None:
            die(f"git rebase {pr.onto} failed:\n{(done.stderr or done.stdout).strip()}")
    return report(pr)


def resume(pr):
    pr_worktree.announce(pr)
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
    return report(pr)


def check(pr, targets):
    tree = pr.tree
    if rebase_dir(tree):
        die(f"finish the rebase first: `{SCRIPT} continue {pr.number}`")
    commands = [["make", "-k", *targets]] if targets else plan(tree, resolved(tree))
    if not commands:
        print("nothing resolved needs a local check; CI runs the rest")
        return 0
    return pr_worktree.run_commands(tree, commands, "rebase-check.log")


def push(pr):
    tree = pr.tree
    if rebase_dir(tree):
        die(f"finish the rebase first: `{SCRIPT} continue {pr.number}`")
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


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("step", choices=["start", "continue", "check", "push"])
    parser.add_argument("pr")
    parser.add_argument("targets", nargs="*")
    parser.add_argument(
        "--use-locked", action="store_true", help="work in a locked worktree the branch is already checked out in"
    )
    args = parser.parse_args()
    if args.targets and args.step != "check":
        parser.error("only check takes make targets")
    pr = lookup(pr_number(args.pr), create=args.step == "start")
    pr_worktree.refuse_locked(pr, SCRIPT, [args.step, str(pr.number), *args.targets], args.use_locked)
    if args.step == "start":
        sys.exit(start(pr))
    if args.step == "continue":
        sys.exit(resume(pr))
    if args.step == "check":
        sys.exit(check(pr, args.targets))
    sys.exit(push(pr))


if __name__ == "__main__":
    main()
