#!/usr/bin/env python3
"""The decisions pr-fix.py makes without a model, against real repositories where git decides."""

import contextlib
import importlib.util
import io
import os
import shutil
import subprocess
import sys
import tempfile
import threading
from pathlib import Path
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("pr_fix", HERE / "pr-fix.py")
pr_fix = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pr_fix)

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
    fork = pr_fix.github_remotes(FORK_CLONE)
    assert fork["brave/bravebot"][0] == "upstream", fork
    assert fork["someone/bravebot"][0] == "origin", fork
    direct = pr_fix.github_remotes(DIRECT_CLONE)
    assert direct["brave/bravebot"][0] == "origin", direct
    assert len(direct) == 1, "a host that only ends in github.com is not GitHub"


def test_a_pull_request_is_named_by_number_or_by_its_url():
    for arg in ("806", "#806", "https://github.com/brave/bravebot/pull/806/files"):
        assert pr_fix.pr_number(arg) == 806, arg
    for arg in ("https://github.com/other/bravebot/pull/806", "main"):
        code, _ = quietly(pr_fix.pr_number, arg)
        assert code == 1, arg


def test_a_fork_is_reached_the_way_the_base_is():
    assert pr_fix.fork_url("a/b", "https://github.com/brave/bravebot") == "https://github.com/a/b.git"
    assert pr_fix.fork_url("a/b", "git@github.com:brave/bravebot.git") == "git@github.com:a/b.git"


def test_a_head_named_like_the_base_does_not_take_the_clones_own_branch():
    assert pr_fix.local_branch("main", "main", 7) == "pr-7"
    assert pr_fix.local_branch("fix", "main", 7) == "fix"


def test_only_a_whole_marker_line_starts_or_ends_a_conflict():
    text = "a\n<<<<<<< HEAD\nb\n=======\nc\n>>>>>>> 1234 (x)\nTitle\n=======\n<<<<<<<<\n"
    assert pr_fix.marker_ranges(text) == [(2, 6)], pr_fix.marker_ranges(text)


def sign_head(cwd):
    """Replace HEAD with the same commit carrying a signature header; verifying one needs a key, telling one is there does not."""
    raw = subprocess.run(
        ["git", "cat-file", "commit", "HEAD"], cwd=cwd, check=True, capture_output=True, text=True
    ).stdout
    headers, message = raw.split("\n\n", 1)
    signature = "gpgsig -----BEGIN SSH SIGNATURE-----\n selftest\n -----END SSH SIGNATURE-----"
    forged = f"{headers}\n{signature}\n\n{message}"
    sha = subprocess.run(
        ["git", "hash-object", "-t", "commit", "-w", "--stdin"],
        cwd=cwd, check=True, capture_output=True, text=True, input=forged,
    ).stdout.strip()
    git("reset", "-q", "--hard", sha, cwd=cwd)


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
    return lambda here=clone: pr_fix.pull(9, view, here, remotes), seed, fork


def test_a_conflict_is_reported_resolved_and_pushed_from_a_worktree_of_its_own():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, fork = world(Path(tmp).resolve())
        code, out = quietly(pr_fix.start, pr())
        assert code == 1, out
        assert "  a.txt:1-5" in out and "main moves" in out, out
        tree = pr().tree
        assert tree.name == "bravebot-9", tree

        code, out = quietly(pr_fix.resume, pr())
        assert code == 1 and "markers remain" in out, "a marker left in place is not staged"

        (tree / "a.txt").write_text("from main and the pull request\n")
        code, out = quietly(pr_fix.resume, pr())
        assert code == 0 and "push 9" in out, out
        assert pr_fix.resolved(tree) == ["a.txt"], "a resolved file is what a check is chosen from"

        code, out = quietly(pr_fix.check, pr(), [])
        assert code == 0 and "CI runs the rest" in out, "a file no rule names runs nothing here"

        code, out = quietly(pr_fix.push, pr())
        assert code == 0, out
        assert git("rev-parse", "feature^", cwd=fork) == git("rev-parse", "main", cwd=fork.parent / "brave.git")
        assert git("show", "feature:a.txt", cwd=fork) == "from main and the pull request"

        code, out = quietly(pr_fix.start, pr())
        assert code == 0 and "nothing to push" in out, out


def test_a_push_made_since_start_is_refused_rather_than_overwritten():
    with tempfile.TemporaryDirectory() as tmp:
        pr, seed, fork = world(Path(tmp).resolve())
        quietly(pr_fix.start, pr())
        (pr().tree / "a.txt").write_text("resolved\n")
        quietly(pr_fix.resume, pr())

        git("checkout", "-q", "feature", cwd=seed)
        (seed / "b.txt").write_text("somebody else\n")
        git("add", "b.txt", cwd=seed)
        git("commit", "-q", "-m", "pushed meanwhile", cwd=seed)
        git("push", "-q", str(fork), "feature", cwd=seed)

        code, out = quietly(pr_fix.push, pr())
        assert code == 1, out
        assert git("log", "-1", "--format=%s", "feature", cwd=fork) == "pushed meanwhile"


def test_a_branch_already_rebased_here_is_rebased_again_rather_than_refused():
    with tempfile.TemporaryDirectory() as tmp:
        pr, seed, _ = world(Path(tmp).resolve())
        quietly(pr_fix.start, pr())
        (pr().tree / "a.txt").write_text("resolved\n")
        quietly(pr_fix.resume, pr())

        (seed / "c.txt").write_text("later\n")
        git("add", "c.txt", cwd=seed)
        git("commit", "-q", "-m", "main moves again", cwd=seed)
        git("push", "-q", "origin", "main", cwd=seed)

        code, out = quietly(pr_fix.start, pr())
        assert code == 0 and "push 9" in out, out
        assert git("log", "-1", "--format=%s", "HEAD^", cwd=pr().tree) == "main moves again"


def test_a_rebase_without_a_conflict_leaves_nothing_resolved_to_check():
    with tempfile.TemporaryDirectory() as tmp:
        pr, seed, _ = world(Path(tmp).resolve())
        quietly(pr_fix.start, pr())
        (pr().tree / "a.txt").write_text("resolved\n")
        quietly(pr_fix.resume, pr())
        assert pr_fix.resolved(pr().tree) == ["a.txt"]

        (seed / "c.txt").write_text("later\n")
        git("add", "c.txt", cwd=seed)
        git("commit", "-q", "-m", "main moves again", cwd=seed)
        git("push", "-q", "origin", "main", cwd=seed)

        code, out = quietly(pr_fix.start, pr())
        assert code == 0, out
        assert pr_fix.resolved(pr().tree) == [], "the earlier rebase's files are not this one's"
        assert pr_fix.changed(pr().tree) == [], "the pull request's own commits are not changes made here"
        code, out = quietly(pr_fix.check, pr(), [])
        assert code == 0 and "CI runs the rest" in out, out


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

        code, out = quietly(pr_fix.start, pr())
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
        quietly(pr_fix.start, pr())
        (pr().tree / "a.txt").write_text("resolved\n")

        code, out = quietly(pr_fix.resume, pr())
        assert code == 1, out
        assert "stopped without a conflict; git said:" in out, out
        assert "  error: gpg failed to sign the data:" in out, "git's own error is the reason it stopped"
        assert "hint:" not in out and "edit-todo" not in out, "the hints are not the reason"


def test_a_step_counter_git_did_not_state_is_left_out_rather_than_printed_as_a_question_mark():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, _ = world(Path(tmp).resolve())
        quietly(pr_fix.start, pr())
        state = pr_fix.rebase_dir(pr().tree)
        assert (state / "msgnum").is_file() and (state / "end").is_file()
        _, out = quietly(pr_fix.report, pr())
        assert "stopped at 1/1: " in out, out

        (state / "end").unlink()
        _, out = quietly(pr_fix.report, pr())
        assert "stopped: " in out and "?" not in out, out


def test_what_git_said_is_cut_to_its_error_and_leaves_the_hints_behind():
    said = "Rebasing (1/2)\rerror: gpg failed to sign the data:\n(no gpg output)\nerror: failed to write commit object\nhint: Could not execute\nhint:\nhint:     git rebase --continue\n"
    assert pr_fix.stop_reason(said) == [
        "error: gpg failed to sign the data:",
        "(no gpg output)",
        "error: failed to write commit object",
    ]
    assert pr_fix.stop_reason("") == []
    assert pr_fix.stop_reason("plain words\nmore words\n") == ["more words"], "no error line: the last line"


def test_the_resolved_files_choose_the_checks_and_a_file_no_rule_names_chooses_none():
    with tempfile.TemporaryDirectory() as tmp:
        tree = Path(tmp)
        for crate in ("agent", "tui"):
            (tree / "crates" / crate).mkdir(parents=True)
            (tree / "crates" / crate / "Cargo.toml").write_text(f'[package]\nname = "bravebot-{crate}"\n')
        plan = lambda *files: pr_fix.plan(tree, list(files))

        assert plan("ui/src/app.ts", "docs/website/docs/index.md", "README.md", "docs/development/checks.md") == []
        assert plan("docs/specs/labels.md") == [["make", "-k", "check-spec"]]
        assert plan("crates/i18n/locales/fr.ftl") == [["make", "-k", "check-locales"]]
        assert plan("Cargo.lock") == [pr_fix.LOCK_RESOLVES, ["make", "-k", "check-versions"]]
        assert plan(".github/workflows/ci.yml") == [["make", "-k", "check-security"]]

        rust = plan("crates/agent/src/lib.rs", "crates/agent/tests/exec.rs", "crates/tui/src/view.rs")
        assert rust == [
            ["cargo", "fmt", "--all", "--", "--check"],
            [
                "cargo", "clippy", "-p", "bravebot-agent", "-p", "bravebot-tui",
                "--all-targets", "--all-features", "--", "-D", "warnings",
            ],
            ["cargo", "test", "-p", "bravebot-agent", "--locked", "--test", "exec"],
            ["make", "-k", "check-spec"],
        ], rust
        assert plan("crates/gone/src/lib.rs") == [["make", "-k", "check-spec"]], "a deleted crate is not linted"


def test_the_files_changed_since_the_rebase_choose_the_checks():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, _ = world(Path(tmp).resolve())
        quietly(pr_fix.start, pr())
        tree = pr().tree
        (tree / "a.txt").write_text("resolved\n")
        quietly(pr_fix.resume, pr())
        (tree / "docs").mkdir()
        (tree / "docs" / "fixed.md").write_text("committed\n")
        git("add", "docs/fixed.md", cwd=tree)
        git("commit", "-q", "-m", "a fix", cwd=tree)
        (tree / "b.txt").write_text("not added\n")
        (tree / "a.txt").write_text("edited after\n")
        assert pr_fix.changed(tree) == ["a.txt", "b.txt", "docs/fixed.md"], pr_fix.changed(tree)


def test_a_push_leaves_out_what_was_not_committed():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, fork = world(Path(tmp).resolve())
        quietly(pr_fix.start, pr())
        (pr().tree / "a.txt").write_text("resolved\n")
        quietly(pr_fix.resume, pr())
        (pr().tree / "a.txt").write_text("edited after\n")
        before = git("rev-parse", "feature", cwd=fork)
        code, out = quietly(pr_fix.push, pr())
        assert code == 1 and "commit them first" in out, out
        assert git("rev-parse", "feature", cwd=fork) == before


def test_signed_commits_are_not_replaced_by_unsigned_ones():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, fork = world(Path(tmp).resolve(), signed=True)
        quietly(pr_fix.start, pr())
        tree = pr().tree
        (tree / "a.txt").write_text("resolved\n")
        quietly(pr_fix.resume, pr())
        assert not pr_fix.signed("HEAD", tree), "the rebase could not sign the commit it wrote"
        before = git("rev-parse", "feature", cwd=fork)

        code, out = quietly(pr_fix.push, pr())
        assert code == 1 and "update-branch" in out and "unsigned" in out, out
        assert git("rev-parse", "feature", cwd=fork) == before, "nothing was pushed"

        (tree / "fix.txt").write_text("a fix\n")
        git("add", "fix.txt", cwd=tree)
        git("commit", "-q", "-m", "a signed fix", cwd=tree)
        sign_head(tree)
        code, out = quietly(pr_fix.push, pr())
        assert code == 1, "a signed head over an unsigned rebased commit still drops the author's signature"
        assert git("rev-parse", "feature", cwd=fork) == before

        git("reset", "-q", "--hard", "HEAD~1", cwd=tree)
        sign_head(tree)
        (tree / "fix.txt").write_text("a fix\n")
        git("add", "fix.txt", cwd=tree)
        git("commit", "-q", "-m", "a signed fix", cwd=tree)
        sign_head(tree)
        code, out = quietly(pr_fix.push, pr())
        assert code == 0, out
        assert git("rev-parse", "feature", cwd=fork) == git("rev-parse", "HEAD", cwd=tree)

        (tree / "other.txt").write_text("unsigned\n")
        git("add", "other.txt", cwd=tree)
        git("commit", "-q", "-m", "an unsigned fix", cwd=tree)
        pushed = git("rev-parse", "feature", cwd=fork)
        code, out = quietly(pr_fix.push, pr())
        assert code == 1, "an unsigned commit is not added to a signed pull request"
        assert git("rev-parse", "feature", cwd=fork) == pushed


def test_an_unsigned_pull_request_is_pushed_unsigned():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, fork = world(Path(tmp).resolve())
        quietly(pr_fix.start, pr())
        tree = pr().tree
        (tree / "a.txt").write_text("resolved\n")
        quietly(pr_fix.resume, pr())
        assert not pr_fix.signed(pr().tip, tree)
        code, out = quietly(pr_fix.push, pr())
        assert code == 0, out
        assert git("rev-parse", "feature", cwd=fork) == git("rev-parse", "HEAD", cwd=tree)


def test_a_fix_committed_on_top_is_pushed_without_rewriting_the_pull_request():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, fork = world(Path(tmp).resolve())
        quietly(pr_fix.start, pr())
        tree = pr().tree
        (tree / "a.txt").write_text("resolved\n")
        quietly(pr_fix.resume, pr())
        quietly(pr_fix.push, pr())
        (tree / "fix.txt").write_text("a fix\n")
        git("add", "fix.txt", cwd=tree)
        git("commit", "-q", "-m", "a fix", cwd=tree)
        before = git("rev-parse", "feature", cwd=fork)
        code, out = quietly(pr_fix.push, pr())
        assert code == 0, out
        assert git("rev-parse", "feature^", cwd=fork) == before, "the fix is a new commit on the pushed head"


def test_it_works_from_a_worktree_and_puts_the_pull_request_beside_the_main_clone():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        pr, _, _ = world(root)
        clone = root / "bravebot"
        elsewhere = root / "nested" / "other"
        git("worktree", "add", "-q", "-b", "other", str(elsewhere), "upstream/main", cwd=clone)
        code, out = quietly(pr_fix.start, pr(elsewhere))
        assert code == 1, out
        tree = pr(elsewhere).tree
        assert tree == root / "bravebot-9", tree
        (tree / "a.txt").write_text("resolved\n")
        code, out = quietly(pr_fix.resume, pr(elsewhere))
        assert code == 0 and "push 9" in out, out
        code, out = quietly(pr_fix.push, pr(elsewhere))
        assert code == 0, out

        code, out = quietly(pr_fix.start, pr(tree))
        assert code == 0 and "nothing to push" in out, "the pull request's own worktree is a place to run from"


def test_a_worktree_deleted_by_hand_does_not_stop_the_pull_request_being_checked_out_again():
    with tempfile.TemporaryDirectory() as tmp:
        pr, _, _ = world(Path(tmp).resolve())
        quietly(pr_fix.start, pr())
        tree = pr().tree
        shutil.rmtree(tree)
        code, out = quietly(pr_fix.start, pr())
        assert code == 1 and "a.txt" in out, out
        assert tree.is_dir()


ROLLUP = [
    {"__typename": "CheckRun", "name": "fmt", "workflowName": "CI", "status": "COMPLETED", "conclusion": "SUCCESS", "detailsUrl": "u1"},
    {"__typename": "CheckRun", "name": "docs", "workflowName": "CI", "status": "COMPLETED", "conclusion": "SKIPPED", "detailsUrl": "u2"},
    {"__typename": "CheckRun", "name": "test", "workflowName": "CI", "status": "COMPLETED", "conclusion": "FAILURE",
     "detailsUrl": "https://github.com/brave/bravebot/actions/runs/5/job/6"},
    {"__typename": "CheckRun", "name": "lint", "workflowName": "", "status": "IN_PROGRESS", "conclusion": "", "detailsUrl": "u4"},
    {"__typename": "StatusContext", "context": "deploy", "state": "PENDING", "targetUrl": "u5"},
    {"__typename": "StatusContext", "context": "legacy", "state": "ERROR", "targetUrl": "u6"},
    {"__typename": "StatusContext", "context": "ok", "state": "SUCCESS", "targetUrl": "u7"},
]


def test_checks_are_split_into_failing_pending_and_passing():
    failing, pending, passing = pr_fix.classify(ROLLUP)
    assert [one[0] for one in failing] == ["CI / test", "legacy"], failing
    assert [one[0] for one in pending] == ["lint", "deploy"], pending
    assert [one[0] for one in passing] == ["CI / fmt", "CI / docs", "ok"], passing
    cancelled = [{"__typename": "CheckRun", "name": "x", "status": "COMPLETED", "conclusion": "CANCELLED"}]
    assert pr_fix.classify(cancelled)[0], "a cancelled job did not pass"


def test_ci_exits_by_what_is_left_to_do():
    def result(rollup):
        pr = SimpleNamespace(number=9, tree=Path("/nonexistent"), head_sha="abc", rollup=rollup)
        return quietly(pr_fix.ci, pr, False)

    code, out = result([{"__typename": "StatusContext", "context": "ok", "state": "SUCCESS"}])
    assert code == 0 and "1 passed, 0 failed, 0 pending" in out, out
    code, out = result([{"__typename": "StatusContext", "context": "slow", "state": "PENDING", "targetUrl": "u"}])
    assert code == 2 and "pending: slow u" in out, out
    code, out = result([
        {"__typename": "StatusContext", "context": "slow", "state": "PENDING"},
        {"__typename": "StatusContext", "context": "bad", "state": "FAILURE", "targetUrl": "u"},
    ])
    assert code == 1 and "failed (failure): bad u" in out, "a failure is reported without waiting for the rest"
    code, out = result([])
    assert code == 2 and "no checks" in out, "a head GitHub has reported nothing on is not green"


def test_a_job_log_is_cut_to_its_last_lines():
    log = "\n".join(f"line {n}" for n in range(100)) + "\n\n"
    cut = pr_fix.tail(log)
    assert cut.splitlines()[0] == "line 60" and cut.splitlines()[-1] == "line 99", cut
    assert pr_fix.failure_log("https://example.invalid/not-a-job") == ""
    raw = "Lint\tClippy\t2026-10-04T19:35:18.4647472Z \x1b[1m\x1b[91merror\x1b[0m: cannot find `unix`\n"
    assert pr_fix.tail(raw) == "error: cannot find `unix`", pr_fix.tail(raw)


def thread(resolved, outdated, path, line, *texts):
    return {
        "isResolved": resolved, "isOutdated": outdated, "path": path, "line": line, "originalLine": line,
        "comments": {"nodes": [
            {"author": {"login": who}, "authorAssociation": "MEMBER", "body": body} for who, body in texts
        ]},
    }


def test_only_a_review_comment_still_waiting_on_a_change_is_shown():
    data = {"data": {"repository": {"pullRequest": {
        "reviewThreads": {"pageInfo": {"hasNextPage": False}, "nodes": [
            thread(True, False, "done.rs", 1, ("ann", "already fixed")),
            thread(False, False, "src/a.rs", 12, ("ann", "rename this"), ("bob", "agreed")),
            thread(False, True, "src/b.rs", 3, ("ann", "x" * 2000)),
        ]},
        "reviews": {"nodes": [
            {"author": {"login": "ann"}, "authorAssociation": "MEMBER", "state": "CHANGES_REQUESTED", "body": "split this"},
            {"author": {"login": "bob"}, "authorAssociation": "MEMBER", "state": "COMMENTED", "body": "first thoughts"},
            {"author": {"login": "bob"}, "authorAssociation": "MEMBER", "state": "APPROVED", "body": "looks good"},
            {"author": None, "authorAssociation": "NONE", "state": "COMMENTED", "body": ""},
        ]},
    }}}}
    text = "\n".join(pr_fix.open_comments(data))
    assert "done.rs" not in text and "already fixed" not in text, text
    assert "thread on src/a.rs:12\n" in text and "ann, member" in text and "agreed" in text, text
    assert "thread on src/b.rs:3 (outdated" in text and "[cut]" in text, text
    assert "split this" in text, "the latest review of a reviewer who has not approved since"
    assert "first thoughts" not in text and "looks good" not in text, "an approval is not a request"


def test_all_lists_the_open_pull_requests_of_the_configured_user():
    with tempfile.TemporaryDirectory() as tmp:
        clone = Path(tmp)
        git("init", "-q", cwd=clone)
        git("config", "user.name", "netzenbot", cwd=clone)
        calls = []
        real = pr_fix.run

        def fake(*args, **kwargs):
            if args[0] != "gh":
                return real(*args, **kwargs)
            calls.append(args)
            return SimpleNamespace(stdout='[{"number": 12}, {"number": 3}]', returncode=0)

        pr_fix.run = fake
        try:
            assert pr_fix.my_pulls(clone) == [3, 12]
        finally:
            pr_fix.run = real
        gh = next(one for one in calls if one[0] == "gh")
        assert gh[gh.index("--author") + 1] == "netzenbot" and gh[gh.index("--repo") + 1] == "brave/bravebot", gh


def test_review_asks_netzenbot_reviewer_through_the_requested_reviewers_api():
    calls = []
    real = pr_fix.run

    def fake(*args, **kwargs):
        calls.append(args)
        return SimpleNamespace(stdout="", stderr="", returncode=0)

    pr_fix.run = fake
    try:
        code, out = quietly(pr_fix.review, SimpleNamespace(number=12, url="https://github.com/brave/bravebot/pull/12"))
    finally:
        pr_fix.run = real
    assert code == 0 and calls == [
        (
            "gh", "api", "--method", "POST", "repos/brave/bravebot/pulls/12/requested_reviewers",
            "-f", "reviewers[]=netzenbot-reviewer",
        )
    ], (calls, out)


def test_a_refused_review_request_fails_the_step():
    real = pr_fix.run
    pr_fix.run = lambda *args, **kwargs: SimpleNamespace(stdout="", stderr="HTTP 403", returncode=1)
    try:
        code, out = quietly(pr_fix.review, SimpleNamespace(number=12, url="u"))
    finally:
        pr_fix.run = real
    assert code == 1 and "HTTP 403" in out, out


def test_pull_requests_are_named_by_commas_spaces_or_links_and_once_each():
    named = pr_fix.pr_numbers(
        ["12,7", "https://github.com/brave/bravebot/pull/9/files #7", " 3 "], Path("."),
    )
    assert named == [12, 7, 9, 3], named
    code, _ = quietly(pr_fix.pr_numbers, ["12,nonsense"], Path("."))
    assert code == 1, "one bad name stops the run before any pull request is touched"


def test_several_pull_requests_run_at_once_and_are_reported_in_order():
    with tempfile.TemporaryDirectory() as tmp:
        meet = Path(tmp)
        script = (
            "import sys, time, pathlib\n"
            f"d = pathlib.Path({str(meet)!r}); n = sys.argv[1]\n"
            "(d / n).write_text('here')\n"
            "end = time.time() + 20\n"
            "while len(list(d.iterdir())) < 3 and time.time() < end: time.sleep(0.05)\n"
            "print('saw', len(list(d.iterdir())), 'of 3 for', n)\n"
            "sys.exit({'1': 0, '2': 2, '3': 1}[n])\n"
        )
        real = pr_fix.child_command
        pr_fix.child_command = lambda step, number, flags: [sys.executable, "-c", script, str(number)]
        try:
            code, out = quietly(pr_fix.fan_out, "ci", [3, 1, 2], [])
        finally:
            pr_fix.child_command = real
        assert out.index("== #3") < out.index("== #1") < out.index("== #2"), out
        assert out.count("saw 3 of 3") == 3, "each ran while the others were running"
        assert code == 1, "a failure outranks a check still running"


def test_the_command_each_pull_request_runs_carries_the_step_and_its_flags():
    command = pr_fix.child_command("check", 9, ["--target", "check-spec"])
    assert command[2:] == ["check", "9", "--target", "check-spec"], command
    assert Path(command[1]).name == "pr-fix.py", command


def test_only_one_process_changes_the_shared_repo_at_a_time():
    with tempfile.TemporaryDirectory() as tmp:
        clone = Path(tmp)
        git("init", "-q", cwd=clone)
        first, second = pr_fix.SharedRepo(), pr_fix.SharedRepo()
        first.acquire(clone)
        got = threading.Event()
        waiter = threading.Thread(target=lambda: (second.acquire(clone), got.set()))
        waiter.start()
        assert not got.wait(0.5), "the second waits while the first holds it"
        first.release()
        first.release()
        assert got.wait(10), "the second proceeds once the first lets go"
        waiter.join()
        second.release()


def main():
    tests =[(name, one) for name, one in globals().items() if name.startswith("test_")]
    for name, one in tests:
        one()
        print(f"  ok    {name[len('test_'):].replace('_', ' ')}")
    print("\npr-fix selftest passed")


if __name__ == "__main__":
    sys.exit(main())
