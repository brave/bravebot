#!/usr/bin/env python3
"""The decisions rebase.py makes without a model, against real repositories where git decides."""

import contextlib
import importlib.util
import io
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("rebase", HERE / "rebase.py")
rebase = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rebase)

# Nothing of the caller's git config applies: no signing key, no hooks, no default branch.
os.environ.update(
    GIT_CONFIG_GLOBAL=os.devnull,
    GIT_CONFIG_NOSYSTEM="1",
    GIT_AUTHOR_NAME="selftest",
    GIT_AUTHOR_EMAIL="selftest@example.invalid",
    GIT_COMMITTER_NAME="selftest",
    GIT_COMMITTER_EMAIL="selftest@example.invalid",
)

FORK_CLONE = """\
origin\tgit@github.com:someone/bravebot.git (fetch)
origin\tgit@github.com:someone/bravebot.git (push)
upstream\tgit@github.com:brave/bravebot.git (fetch)
upstream\tgit@github.com:brave/bravebot.git (push)
"""
DIRECT_CLONE = """\
origin\thttps://github.com/Brave/BraveBot (fetch)
origin\thttps://github.com/Brave/BraveBot (push)
lookalike\thttps://notgithub.com/brave/bravebot.git (fetch)
"""


def git(*args, cwd):
    return subprocess.run(
        ["git", *args], cwd=cwd, check=True, capture_output=True, text=True
    ).stdout.strip()


def quietly(step, *args):
    out = io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(out):
        try:
            code = step(*args)
        except SystemExit as exit:
            code = exit.code
    return code, out.getvalue()


def test_the_base_is_whichever_remote_names_brave_bravebot():
    fork = rebase.github_remotes(FORK_CLONE)
    assert fork["brave/bravebot"][0] == "upstream", fork
    assert fork["someone/bravebot"][0] == "origin", fork
    direct = rebase.github_remotes(DIRECT_CLONE)
    assert direct["brave/bravebot"][0] == "origin", direct
    assert len(direct) == 1, "a host that only ends in github.com is not GitHub"


def test_a_pull_request_is_named_by_number_or_by_its_url():
    for arg in ("806", "#806", "https://github.com/brave/bravebot/pull/806/files"):
        assert rebase.pr_number(arg) == 806, arg
    for arg in ("https://github.com/other/bravebot/pull/806", "main"):
        code, _ = quietly(rebase.pr_number, arg)
        assert code == 1, arg


def test_a_fork_is_reached_the_way_the_base_is():
    assert rebase.fork_url("a/b", "https://github.com/brave/bravebot") == "https://github.com/a/b.git"
    assert rebase.fork_url("a/b", "git@github.com:brave/bravebot.git") == "git@github.com:a/b.git"


def test_a_head_named_like_the_base_does_not_take_the_clones_own_branch():
    assert rebase.local_branch("main", "main", 7) == "pr-7"
    assert rebase.local_branch("fix", "main", 7) == "fix"


def test_only_a_whole_marker_line_starts_or_ends_a_conflict():
    text = "a\n<<<<<<< HEAD\nb\n=======\nc\n>>>>>>> 1234 (x)\nTitle\n=======\n<<<<<<<<\n"
    assert rebase.marker_ranges(text) == [(2, 6)], rebase.marker_ranges(text)


def world(root):
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
        assert tree.name == "bravebot-9", tree

        code, out = quietly(rebase.resume, pr())
        assert code == 1 and "markers remain" in out, "a marker left in place is not staged"

        (tree / "a.txt").write_text("from main and the pull request\n")
        code, out = quietly(rebase.resume, pr())
        assert code == 0 and "push 9" in out, out

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


def main():
    tests = [(name, one) for name, one in globals().items() if name.startswith("test_")]
    for name, one in tests:
        one()
        print(f"  ok    {name[len('test_'):].replace('_', ' ')}")
    print("\nrebase selftest passed")


if __name__ == "__main__":
    sys.exit(main())
