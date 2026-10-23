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

die = pr_worktree.die
run = pr_worktree.run
pr_number = pr_worktree.pr_number
rebase_dir = pr_worktree.rebase_dir
resolved = pr_worktree.resolved
plan = pr_worktree.plan


# What `gh pr view` is asked for differs by script.
FIELDS = "url,state,baseRefName,headRefName,headRepository,headRepositoryOwner"


def pull(number, view, here, remotes, create=False):
    return pr_worktree.pull(number, view, here, remotes, create, SCRIPT)


def lookup(number, create):
    return pr_worktree.lookup(number, create, SCRIPT, FIELDS)


def report(pr, said=""):
    return pr_worktree.report(pr, SCRIPT, said)


def start(pr):
    pr_worktree.announce(pr)
    pr_worktree.add_worktree(pr)
    tree = pr.tree
    if rebase_dir(tree):
        return report(pr)
    pr_worktree.catch_up_with_pull(pr, tree)
    return report(pr, pr_worktree.rebase_onto(pr, tree))


def resume(pr):
    pr_worktree.announce(pr)
    tree = pr.tree
    if rebase_dir(tree) is None:
        return report(pr)
    said = pr_worktree.resolve_or_stop(tree)
    if said is None:
        return 1
    return report(pr, said)


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
    return pr_worktree.push(pr, SCRIPT)


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
