#!/usr/bin/env python3
"""The decisions rebase.py makes without a model, against real repositories where git decides."""

import importlib.util
import shutil
import sys
import tempfile
from pathlib import Path
from unittest import mock

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


def test_a_branch_already_checked_out_elsewhere_is_rebased_there_and_every_report_says_so():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        pr, _, _ = world(root)
        elsewhere = root / "elsewhere"
        helpers.the_forks_feature_in(root / "bravebot", elsewhere)
        code, out = quietly(rebase.start, pr())
        assert code == 1 and "  a.txt:1-5" in out, out
        assert f"worktree {elsewhere}" in out, out
        assert f"reused: feature is already checked out in {elsewhere} (outside the main clone" in out, out
        assert not (root / "bravebot" / ".worktrees").exists(), "the standard place is not made as well"

        (elsewhere / "a.txt").write_text("from main and the pull request\n")
        code, out = quietly(rebase.resume, pr())
        assert code == 0 and "push 9" in out and "reused: feature" in out, out


def test_a_reused_worktree_with_uncommitted_changes_says_it_was_reused_when_start_refuses_it():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        pr, _, _ = world(root)
        elsewhere = root / "elsewhere"
        helpers.the_forks_feature_in(root / "bravebot", elsewhere)
        (elsewhere / "a.txt").write_text("the person's own edit\n")
        code, out = quietly(rebase.start, pr())
        assert code == 1 and f"{elsewhere} has uncommitted changes" in out, out
        assert "reused: feature is already checked out in" in out, out


def run_main(pr, *argv):
    with mock.patch.object(rebase, "lookup", lambda number, create: pr):
        with mock.patch.object(sys, "argv", ["rebase.py", *argv]):
            return quietly(rebase.main)


def test_no_step_works_in_a_locked_worktree_until_the_person_agrees():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        pr, _, _ = world(root)
        clone, elsewhere = root / "bravebot", root / "elsewhere"
        helpers.the_forks_feature_in(clone, elsewhere, lock="build running")
        head, base = git("rev-parse", "HEAD", cwd=elsewhere), git("rev-parse", "upstream/main", cwd=clone)

        for step in ("start", "continue", "check", "push"):
            code, out = run_main(pr(), step, "9")
            assert code == 1 and "build running" in out, (step, out)
            assert f"{rebase.SCRIPT} {step} 9 --use-locked" in out, (step, out)
            assert "a.txt" not in out, "no conflict was reached"
        assert git("rev-parse", "HEAD", cwd=elsewhere) == head and rebase.rebase_dir(elsewhere) is None
        assert git("rev-parse", "upstream/main", cwd=clone) == base, "not even a fetch happened"

        code, out = run_main(pr(), "check", "9", "check-spec")
        assert code == 1 and f"{rebase.SCRIPT} check 9 check-spec --use-locked" in out, "the targets asked for are kept"

        code, out = run_main(pr(), "start", "9", "--use-locked")
        assert code == 1 and "  a.txt:1-5" in out and "locked: build running" in out, out
        (elsewhere / "a.txt").write_text("from main and the pull request\n")
        code, out = run_main(pr(), "continue", "9")
        assert code == 1 and "--use-locked" in out, "a yes given to one step is not given to the next"
        code, out = run_main(pr(), "continue", "9", "--use-locked")
        assert code == 0 and "push 9" in out, out


def unsignable(clone):
    """Every commit is to be signed and the signing program fails, as when the signing key is missing."""
    git("config", "commit.gpgsign", "true", cwd=clone)
    git("config", "gpg.program", "false", cwd=clone)


def test_a_rebase_stopped_by_a_signing_failure_says_so_when_it_starts():
    with tempfile.TemporaryDirectory() as tmp:
        pr, seed, fork = world(Path(tmp).resolve())
        git("checkout", "-q", "-b", "clean", "main~1", cwd=seed)
        (seed / "b.txt").write_text("no conflict\n")
        git("add", "b.txt", cwd=seed)
        git("commit", "-q", "-m", "clean change", cwd=seed)
        git("push", "-q", "-f", str(fork), "clean:feature", cwd=seed)
        unsignable(pr().here)

        code, out = quietly(rebase.start, pr())
        assert code == 1, out
        assert "stopped at 1/1: " in out and "clean change" in out, out
        assert "stopped without a conflict; git said:" in out, out
        assert "  error: gpg failed to sign the data:" in out, "git's own error is the reason it stopped"
        assert "hint:" not in out and "edit-todo" not in out, "the hints are not the reason"


def test_a_rebase_stopped_by_a_signing_failure_says_so_when_it_continues():
    with tempfile.TemporaryDirectory() as tmp:
        pr, seed, fork = world(Path(tmp).resolve())
        git("checkout", "-q", "feature", cwd=seed)
        (seed / "b.txt").write_text("second\n")
        git("add", "b.txt", cwd=seed)
        git("commit", "-q", "-m", "second", cwd=seed)
        git("push", "-q", str(fork), "feature", cwd=seed)
        unsignable(pr().here)
        quietly(rebase.start, pr())
        (pr().tree / "a.txt").write_text("resolved\n")

        code, out = quietly(rebase.resume, pr())
        assert code == 1, out
        assert "stopped without a conflict; git said:" in out, out
        assert "  error: gpg failed to sign the data:" in out, "git's own error is the reason it stopped"


def test_a_step_counter_git_did_not_state_is_left_out_rather_than_printed_as_a_question_mark():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, _ = world(Path(tmp).resolve())
        quietly(rebase.start, pr())
        state = rebase.rebase_dir(pr().tree)
        assert (state / "msgnum").is_file() and (state / "end").is_file()
        _, out = quietly(rebase.report, pr())
        assert "stopped at 1/1: " in out, out

        (state / "end").unlink()
        _, out = quietly(rebase.report, pr())
        assert "stopped: " in out and "?" not in out, out


def test_a_push_from_a_worktree_with_uncommitted_changes_is_refused():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, fork = world(Path(tmp).resolve())
        quietly(rebase.start, pr())
        (pr().tree / "a.txt").write_text("resolved\n")
        quietly(rebase.resume, pr())
        before = git("rev-parse", "feature", cwd=fork)
        (pr().tree / "a.txt").write_text("edited after the rebase\n")

        code, out = quietly(rebase.push, pr())
        assert code == 1 and "has uncommitted changes; commit them first" in out, out
        assert git("rev-parse", "feature", cwd=fork) == before


def main():
    tests =[(name, one) for name, one in globals().items() if name.startswith("test_")]
    for name, one in tests:
        one()
        print(f"  ok    {name[len('test_'):].replace('_', ' ')}")
    print("\nrebase selftest passed")


if __name__ == "__main__":
    sys.exit(main())
