#!/usr/bin/env python3
"""The decisions rebase.py makes without a model, against real repositories where git decides."""

import importlib.util
import shutil
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("rebase", HERE / "rebase.py")
rebase = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rebase)


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The git, quietly, signing and ssh helpers are shared with the other selftests of this pair.
helpers = _load("pr_worktree_selftest", HERE.parents[1] / "pr_worktree_selftest.py")
git = helpers.git
quietly = helpers.quietly
sign_head = helpers.sign_head


def world(root, signed=False):
    """brave/bravebot and a fork as bare repositories, a clone of both, and a conflicting PR."""
    brave, fork, seed, clone = (root / name for name in ("brave.git", "fork.git", "seed", "bravebot"))
    git("init", "-q", "--bare", "-b", "main", str(brave), cwd=root)
    git("init", "-q", "--bare", "-b", "main", str(fork), cwd=root)
    git("clone", "-q", str(brave), str(seed), cwd=root)
    (seed / "a.txt").write_text("one\n")
    git("add", "a.txt", cwd=seed)
    git("commit", "-q", "-m", "one", cwd=seed)
    git("push", "-q", "origin", "main", cwd=seed)
    git("push", "-q", str(fork), "main:feature", cwd=seed)
    git("clone", "-q", "-o", "upstream", str(brave), str(clone), cwd=root)
    git("remote", "add", "origin", str(fork), cwd=clone)

    git("checkout", "-q", "-b", "feature", cwd=seed)
    (seed / "a.txt").write_text("from the pull request\n")
    git("commit", "-q", "-am", "the pull request", cwd=seed)
    if signed:
        sign_head(seed)
    git("push", "-q", str(fork), "feature", cwd=seed)
    git("checkout", "-q", "main", cwd=seed)
    (seed / "a.txt").write_text("from main\n")
    git("commit", "-q", "-am", "main moves", cwd=seed)
    git("push", "-q", "origin", "main", cwd=seed)

    view = {
        "url": "https://github.com/brave/bravebot/pull/9",
        "state": "OPEN",
        "baseRefName": "main",
        "headRefName": "feature",
        "headRepository": {"name": "bravebot"},
        "headRepositoryOwner": {"login": "someone"},
    }
    remotes = {"brave/bravebot": ("upstream", str(brave)), "someone/bravebot": ("origin", str(fork))}
    return lambda: rebase.pull(9, view, clone, remotes), seed, fork


def test_a_conflict_is_reported_resolved_and_pushed_from_a_worktree_of_its_own():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, fork = world(Path(tmp).resolve())
        code, out = quietly(rebase.start, pr())
        assert code == 1, out
        assert "  a.txt:1-5" in out and "main moves" in out, out
        tree = pr().tree
        assert tree == Path(tmp).resolve() / "bravebot" / ".worktrees" / "pr-9", tree

        code, out = quietly(rebase.resume, pr())
        assert code == 1 and "markers remain" in out, "a marker left in place is not staged"

        (tree / "a.txt").write_text("from main and the pull request\n")
        code, out = quietly(rebase.resume, pr())
        assert code == 0 and "push 9" in out, out
        assert rebase.resolved(tree) == ["a.txt"], "a resolved file is what a check is chosen from"

        code, out = quietly(rebase.check, pr(), [])
        assert code == 0 and "CI runs the rest" in out, "a file no rule names runs nothing here"

        code, out = quietly(rebase.push, pr())
        assert code == 0, out
        assert git("rev-parse", "feature^", cwd=fork) == git("rev-parse", "main", cwd=fork.parent / "brave.git")
        assert git("show", "feature:a.txt", cwd=fork) == "from main and the pull request"

        code, out = quietly(rebase.start, pr())
        assert code == 0 and "nothing to push" in out, out


def test_a_push_made_since_start_is_refused_rather_than_overwritten():
    with tempfile.TemporaryDirectory() as tmp:
        pr, seed, fork = world(Path(tmp).resolve())
        quietly(rebase.start, pr())
        (pr().tree / "a.txt").write_text("resolved\n")
        quietly(rebase.resume, pr())

        git("checkout", "-q", "feature", cwd=seed)
        (seed / "b.txt").write_text("somebody else\n")
        git("add", "b.txt", cwd=seed)
        git("commit", "-q", "-m", "pushed meanwhile", cwd=seed)
        git("push", "-q", str(fork), "feature", cwd=seed)

        code, out = quietly(rebase.push, pr())
        assert code == 1, out
        assert git("log", "-1", "--format=%s", "feature", cwd=fork) == "pushed meanwhile"


def test_a_push_github_refuses_is_retried_with_each_other_key_the_agent_holds():
    with tempfile.TemporaryDirectory() as tmp:
        pr, remote = helpers.an_ssh_remote(Path(tmp).resolve())
        keys = ["ssh-ed25519 AAAAbad bot@example.invalid", "ssh-ed25519 AAAAgood person@example.invalid"]
        with helpers.an_agent_holding(keys):
            code, out = quietly(rebase.push, pr)
        assert code == 0 and "person@example.invalid" in out, out
        assert "bot@example.invalid" not in out, out
        assert git("rev-parse", "feature", cwd=remote) == git("rev-parse", "HEAD", cwd=pr.tree)


def test_signed_commits_are_not_replaced_by_unsigned_ones():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, fork = world(Path(tmp).resolve(), signed=True)
        quietly(rebase.start, pr())
        tree = pr().tree
        (tree / "a.txt").write_text("resolved\n")
        quietly(rebase.resume, pr())
        assert not rebase.pr_worktree.signed("HEAD", tree), "the rebase could not sign the commit it wrote"
        before = git("rev-parse", "feature", cwd=fork)

        code, out = quietly(rebase.push, pr())
        assert code == 1 and "update-branch" in out and "unsigned" in out, out
        assert git("rev-parse", "feature", cwd=fork) == before, "nothing was pushed"


def test_a_branch_already_rebased_here_is_rebased_again_rather_than_refused():
    with tempfile.TemporaryDirectory() as tmp:
        pr, seed, _ = world(Path(tmp).resolve())
        quietly(rebase.start, pr())
        (pr().tree / "a.txt").write_text("resolved\n")
        quietly(rebase.resume, pr())

        (seed / "c.txt").write_text("later\n")
        git("add", "c.txt", cwd=seed)
        git("commit", "-q", "-m", "main moves again", cwd=seed)
        git("push", "-q", "origin", "main", cwd=seed)

        code, out = quietly(rebase.start, pr())
        assert code == 0 and "push 9" in out, out
        assert git("log", "-1", "--format=%s", "HEAD^", cwd=pr().tree) == "main moves again"


def test_a_rebase_without_a_conflict_leaves_nothing_resolved_to_check():
    with tempfile.TemporaryDirectory() as tmp:
        pr, seed, _ = world(Path(tmp).resolve())
        quietly(rebase.start, pr())
        (pr().tree / "a.txt").write_text("resolved\n")
        quietly(rebase.resume, pr())
        assert rebase.resolved(pr().tree) == ["a.txt"]

        (seed / "c.txt").write_text("later\n")
        git("add", "c.txt", cwd=seed)
        git("commit", "-q", "-m", "main moves again", cwd=seed)
        git("push", "-q", "origin", "main", cwd=seed)

        code, out = quietly(rebase.start, pr())
        assert code == 0, out
        assert rebase.resolved(pr().tree) == [], "the earlier rebase's files are not this one's"
        code, out = quietly(rebase.check, pr(), [])
        assert code == 0 and "CI runs the rest" in out, out


def test_a_worktree_deleted_by_hand_mid_rebase_is_checked_out_again():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, _ = world(Path(tmp).resolve())
        quietly(rebase.start, pr())
        tree = pr().tree
        shutil.rmtree(tree)
        code, out = quietly(rebase.start, pr())
        assert code == 1 and "a.txt" in out, out
        assert tree.is_dir()


def test_a_worktree_deleted_by_hand_after_the_rebase_finished_is_checked_out_again():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, _ = world(Path(tmp).resolve())
        quietly(rebase.start, pr())
        tree = pr().tree
        (tree / "a.txt").write_text("from main and the pull request\n")
        code, out = quietly(rebase.resume, pr())
        assert code == 0 and "push 9" in out, out
        shutil.rmtree(tree)
        code, out = quietly(rebase.start, pr())
        assert code == 0 and "push 9" in out, out
        assert tree.is_dir()


def test_a_worktree_locked_by_something_else_is_not_fetched_into_or_rebased():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        pr, _, _ = world(root)
        clone = root / "bravebot"
        tree = root / "bravebot-elsewhere"
        git("fetch", "-q", "origin", cwd=clone)
        helpers.check_out_as_another_workflow(clone, tree, "feature", "origin/feature", lock="another run, slot 7")
        head, upstream = git("rev-parse", "HEAD", cwd=tree), git("rev-parse", "upstream/main", cwd=clone)

        code, out = quietly(rebase.start, pr())
        assert code == 1, out
        assert f"feature is already checked out at {tree}, which is locked" in out, out
        assert 'with the reason "another run, slot 7" and outside' in out, out
        assert rebase.rebase_dir(tree) is None and git("rev-parse", "HEAD", cwd=tree) == head
        assert git("rev-parse", "upstream/main", cwd=clone) == upstream, "nothing was fetched"


def test_start_run_again_while_its_own_rebase_is_stopped_reports_the_conflict_again():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, _ = world(Path(tmp).resolve())
        code, out = quietly(rebase.start, pr())
        assert code == 1 and "a.txt:1-5" in out, out

        code, out = quietly(rebase.start, pr())
        assert code == 1 and "a.txt:1-5" in out, out
        assert "exists and is not a worktree" not in out, "the tree holds the branch while its rebase is stopped"


def test_continue_check_and_push_leave_a_locked_worktree_alone_as_start_does():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        pr, _, _ = world(root)
        clone = root / "bravebot"
        tree = root / "bravebot-elsewhere"
        git("fetch", "-q", "origin", cwd=clone)
        helpers.check_out_as_another_workflow(clone, tree, "feature", "origin/feature", lock="another run, slot 7")
        helpers.refuses_every_step_in_a_locked_worktree(rebase, pr, clone, tree, root / "fork.git")


def main():
    tests =[(name, one) for name, one in globals().items() if name.startswith("test_")]
    for name, one in tests:
        one()
        print(f"  ok    {name[len('test_'):].replace('_', ' ')}")
    print("\nrebase selftest passed")


if __name__ == "__main__":
    sys.exit(main())
