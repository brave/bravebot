#!/usr/bin/env python3
"""Selftest for pr_worktree.py, which rebase.py and pr-fix.py share, against real repositories where git decides."""

import contextlib
import importlib.util
import io
import json
import os
import shutil
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("pr_worktree", HERE / "pr_worktree.py")
pr_worktree = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pr_worktree)

# Nothing of the caller's git config applies: no signing key, no hooks, no default branch.
os.environ.update(
    GIT_CONFIG_GLOBAL=os.devnull,
    GIT_CONFIG_NOSYSTEM="1",
    GIT_AUTHOR_NAME="selftest",
    GIT_AUTHOR_EMAIL="selftest@example.invalid",
    GIT_COMMITTER_NAME="selftest",
    GIT_COMMITTER_EMAIL="selftest@example.invalid",
)
os.environ.pop(pr_worktree.WORKTREES_ENV, None)

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


def test_the_base_is_whichever_remote_names_brave_bravebot():
    fork = pr_worktree.github_remotes(FORK_CLONE)
    assert fork["brave/bravebot"][0] == "upstream", fork
    assert fork["someone/bravebot"][0] == "origin", fork
    direct = pr_worktree.github_remotes(DIRECT_CLONE)
    assert direct["brave/bravebot"][0] == "origin", direct
    assert len(direct) == 1, "a host that only ends in github.com is not GitHub"


def test_an_ssh_alias_for_github_names_the_repo_it_points_at():
    listing = (
        "upstream\tgit@github-work:brave/bravebot.git (fetch)\n"
        "origin\tgit@github-other:someone/bravebot.git (fetch)\n"
        "evil\tgit@github-evil.example.com:evil/bravebot.git (fetch)\n"
        "mirror\tgit@gitlab.com:mirror/bravebot.git (fetch)\n"
    )
    found = pr_worktree.github_remotes(listing)
    assert found["brave/bravebot"][0] == "upstream", found
    assert found["someone/bravebot"][0] == "origin", found
    assert set(found) == {"brave/bravebot", "someone/bravebot"}, "only github.com and its aliases count"


def test_a_new_fork_remote_does_not_inherit_the_base_remotes_ssh_alias():
    url = pr_worktree.fork_url("someone/bravebot", "git@github-work:brave/bravebot.git")
    assert url == "git@github.com:someone/bravebot.git", url


def test_a_worktree_goes_in_an_ignored_directory_of_the_clone_unless_a_directory_is_named():
    main = Path("/work/bravebot")
    assert pr_worktree.worktree_path(main, 9) == main / ".worktrees" / "pr-9"
    with mock.patch.dict(os.environ, {pr_worktree.WORKTREES_ENV: "/elsewhere"}):
        assert pr_worktree.worktree_path(main, 9) == Path("/elsewhere/bravebot-9")
    with mock.patch.dict(os.environ, {pr_worktree.WORKTREES_ENV: "../trees"}):
        assert pr_worktree.worktree_path(main, 9) == main / ".." / "trees" / "bravebot-9"
    gitignore = (HERE.parent / ".gitignore").read_text()
    assert "/.worktrees/" in gitignore.splitlines(), "git must ignore the default directory"


def test_a_pull_request_is_named_by_number_or_by_its_url():
    for arg in ("806", "#806", "https://github.com/brave/bravebot/pull/806/files"):
        assert pr_worktree.pr_number(arg) == 806, arg
    for arg in ("https://github.com/other/bravebot/pull/806", "main"):
        code, _ = quietly(pr_worktree.pr_number, arg)
        assert code == 1, arg


def test_a_fork_is_reached_the_way_the_base_is():
    assert pr_worktree.fork_url("a/b", "https://github.com/brave/bravebot") == "https://github.com/a/b.git"
    assert pr_worktree.fork_url("a/b", "git@github.com:brave/bravebot.git") == "git@github.com:a/b.git"


def test_a_head_named_like_the_base_does_not_take_the_clones_own_branch():
    assert pr_worktree.local_branch("main", "main", 7) == "pr-7"
    assert pr_worktree.local_branch("fix", "main", 7) == "fix"


def test_only_a_whole_marker_line_starts_or_ends_a_conflict():
    text = "a\n<<<<<<< HEAD\nb\n=======\nc\n>>>>>>> 1234 (x)\nTitle\n=======\n<<<<<<<<\n"
    assert pr_worktree.marker_ranges(text) == [(2, 6)], pr_worktree.marker_ranges(text)


def test_the_resolved_files_choose_the_checks_and_a_file_no_rule_names_chooses_none():
    with tempfile.TemporaryDirectory() as tmp:
        tree = Path(tmp)
        for crate in ("agent", "tui"):
            (tree / "crates" / crate).mkdir(parents=True)
            (tree / "crates" / crate / "Cargo.toml").write_text(f'[package]\nname = "bravebot-{crate}"\n')
        plan = lambda *files: pr_worktree.plan(tree, list(files))

        assert plan("ui/src/app.ts", "docs/website/docs/index.md", "README.md", "docs/development/checks.md") == []
        assert plan("docs/specs/labels.md") == [["make", "-k", "check-spec"]]
        assert plan("crates/i18n/locales/fr.ftl") == [["make", "-k", "check-locales"]]
        assert plan("Cargo.lock") == [pr_worktree.LOCK_RESOLVES, ["make", "-k", "check-versions"]]
        assert plan(".github/workflows/ci.yml") == [["make", "-k", "check-security"]]
        assert plan("ui/package.json") == [["make", "-k", "check-versions"]]
        assert plan("ui/pnpm-lock.yaml", "package-lock.json") == [], "no lockfile states the version"

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


def a_clone_with_a_deleted_worktree(root):
    """A repository whose worktree `feature` was removed by hand, so git still lists it until pruned."""
    clone = root / "clone"
    git("init", "-q", "-b", "main", str(clone), cwd=root)
    (clone / "a.txt").write_text("one\n")
    git("add", "a.txt", cwd=clone)
    git("commit", "-q", "-m", "one", cwd=clone)
    tree = root / "clone-feature"
    git("worktree", "add", "-q", "-b", "feature", str(tree), cwd=clone)
    shutil.rmtree(tree)
    return clone, tree


def test_a_worktree_whose_directory_was_deleted_by_hand_holds_no_branch():
    with tempfile.TemporaryDirectory() as tmp:
        clone, tree = a_clone_with_a_deleted_worktree(Path(tmp).resolve())
        assert "branch refs/heads/feature" in git("worktree", "list", "--porcelain", cwd=clone)
        found, main = pr_worktree.worktrees(clone)
        assert {branch: one.path for branch, one in found.items()} == {"main": clone}, found
        assert main == clone


def a_failure_printing(stderr, stdout):
    # The text goes in through the environment because the failure message repeats the command line.
    script = "import os, sys; sys.stderr.write(os.environ['ERR']); sys.stdout.write(os.environ['OUT']); sys.exit(3)"
    env = {**os.environ, "ERR": stderr, "OUT": stdout}
    return quietly(lambda: pr_worktree.run(sys.executable, "-c", script, env=env))


def test_a_failed_command_reports_what_it_wrote_to_stderr_and_to_stdout():
    # gh api writes an error response's JSON, which holds the reason, to stdout and only a summary to stderr.
    for stderr, stdout in (("the summary", "the reason"), ("", "the reason"), ("the summary", "")):
        code, out = a_failure_printing(stderr, stdout)
        assert code == 1, out
        assert out.splitlines()[1:] == [text for text in (stderr, stdout) if text], (stderr, stdout, out)


def test_a_json_error_body_is_cut_to_its_message_and_errors():
    # A GraphQL response that failed in part also carries the data the query asked for, which is not the reason.
    partial = {"data": {"repository": "THE-REVIEW-THREADS"}, "errors": [{"message": "Resource limit exceeded"}]}
    refused = {"message": "Validation Failed", "errors": ["Review cannot be requested from pull request author."]}
    cases = (
        (partial, {"errors": partial["errors"]}),
        ({**refused, "documentation_url": "https://docs.github.com/rest", "status": "422"}, refused),
    )
    for body, reason in cases:
        code, out = a_failure_printing("gh: failed", json.dumps(body))
        assert code == 1 and "THE-REVIEW-THREADS" not in out and "documentation_url" not in out, out
        assert out.splitlines()[1] == "gh: failed" and json.loads(out.splitlines()[2]) == reason, out
        assert len(out.splitlines()) == 3, out


def test_a_failure_body_that_is_not_an_error_object_is_printed_as_it_came():
    bodies = (
        '{"detail": "no message here"}',
        '{"errors": null, "detail": "rate limited"}',
        '"an error message"',
        '["a", "b"]',
        '{"data": 1, "errors": [{"message": "cut off',
        "[" * 200000,
    )
    for body in bodies:
        assert pr_worktree.printed(SimpleNamespace(stderr="", stdout=body)) == body, body[:60]


def test_a_line_separator_in_an_error_message_cannot_start_a_new_line():
    # Text a response echoes back is printed on the one line the reason takes.
    message = "bad\u2028name\u0085here"
    out = pr_worktree.printed(SimpleNamespace(stderr="", stdout=json.dumps({"message": message})))
    assert len(out.splitlines()) == 1 and json.loads(out) == {"message": message}, out


def a_plain_clone(root):
    clone = root / "clone"
    git("init", "-q", "-b", "main", str(clone), cwd=root)
    (clone / "a.txt").write_text("one\n")
    git("add", "a.txt", cwd=clone)
    git("commit", "-q", "-m", "one", cwd=clone)
    return clone


def the_pull_request_in(clone, here=None):
    """Pull request 9, whose head branch is `feature`, as `pull` reads it from a clone, run from `here`."""
    view = {
        "url": "https://github.com/brave/bravebot/pull/9",
        "state": "OPEN",
        "baseRefName": "main",
        "headRefName": "feature",
        "headRepository": {"name": "bravebot"},
        "headRepositoryOwner": {"login": "someone"},
    }
    remotes = {"brave/bravebot": ("upstream", "u"), "someone/bravebot": ("origin", "o")}
    return pr_worktree.pull(9, view, here or clone, remotes, False, "python3 script.py")


def feature_in(clone, tree, lock=None, start="HEAD"):
    """Check `feature` out in a new linked worktree at `tree`, locked with `lock` as the reason when given."""
    git("worktree", "add", "-q", "-b", "feature", str(tree), start, cwd=clone)
    if lock is not None:
        git("worktree", "lock", "--reason", lock, str(tree), cwd=clone)


def the_forks_feature_in(clone, tree, lock=None):
    """`feature_in` for a clone with the fork as `origin`, the worktree starting at the pull request's head."""
    git("fetch", "-q", "origin", "feature", cwd=clone)
    feature_in(clone, tree, lock, "origin/feature")


def test_a_lock_and_its_reason_belong_to_the_worktree_that_carries_them():
    """A reason read off the wrong worktree sends the person to free a tree that nothing is using."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        for name in ("a", "b", "c", "d"):
            git("worktree", "add", "-q", "-b", name, str(root / name), cwd=clone)
        git("worktree", "lock", "--reason", "build running", str(root / "b"), cwd=clone)
        git("worktree", "lock", str(root / "d"), cwd=clone)
        found, _ = pr_worktree.worktrees(clone)
        got = {branch: (one.locked, one.reason) for branch, one in found.items()}
        assert got == {
            "main": (False, ""),
            "a": (False, ""),
            "b": (True, "build running"),
            "c": (False, ""),
            "d": (True, ""),
        }, got


def test_a_worktree_in_the_middle_of_a_rebase_still_holds_its_branch():
    """Git lists it as detached while the rebase runs, and a later step that cannot find it looks in the wrong place."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        feature_in(clone, root / "elsewhere")
        git("worktree", "add", "-q", "--detach", str(root / "detached"), cwd=clone)
        (root / "elsewhere" / "a.txt").write_text("from the branch\n")
        git("commit", "-q", "-am", "branch", cwd=root / "elsewhere")
        (clone / "a.txt").write_text("from main\n")
        git("commit", "-q", "-am", "main", cwd=clone)

        found, _ = pr_worktree.worktrees(clone)
        assert found["feature"].path == root / "elsewhere"
        subprocess.run(["git", "rebase", "main"], cwd=root / "elsewhere", capture_output=True)
        assert pr_worktree.rebase_dir(root / "elsewhere"), "the rebase stopped, which is the fixture's point"

        found, _ = pr_worktree.worktrees(clone)
        assert {branch: one.path for branch, one in found.items()} == {
            "main": clone,
            "feature": root / "elsewhere",
        }, "a rebase holds its branch, and a worktree that is only detached holds none"


def test_a_branch_where_a_new_worktree_would_go_is_not_called_a_reuse():
    """A note on every run teaches the model to skip it, so it is printed only when the place is unexpected."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        pr = the_pull_request_in(clone)
        assert not pr.checked_out and pr.tree == clone / ".worktrees" / "pr-9"
        assert pr_worktree.reuse_note(pr) == "", "no branch is checked out, so a new worktree is made"

        feature_in(clone, clone / ".worktrees" / "pr-9")
        pr = the_pull_request_in(clone)
        assert pr.checked_out and pr_worktree.reuse_note(pr) == "", "the skill's own worktree is not a surprise"

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        feature_in(clone, root / "trees" / "clone-9")
        with mock.patch.dict(os.environ, {pr_worktree.WORKTREES_ENV: "../trees"}):
            pr = the_pull_request_in(clone)
            assert pr.checked_out and pr_worktree.reuse_note(pr) == "", "the named directory is the standard one"


def test_a_branch_checked_out_elsewhere_is_reported_with_where_and_how_to_free_the_standard_place():
    """The standard path is where the skill says the work is, so a different one has to be said."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        standard = clone / ".worktrees" / "pr-9"
        feature_in(clone, root / "elsewhere")
        note = pr_worktree.reuse_note(the_pull_request_in(clone))
        assert "\n" not in note and note.startswith("reused: feature is already checked out in "), note
        assert f"{root / 'elsewhere'} (outside the main clone {clone}" in note, note
        assert "the file tools refuse it until it is opened" in note, note
        assert f"{standard}, the person removes it: git worktree remove {root / 'elsewhere'}" in note, note
        assert "locked" not in note and "unlock" not in note, note

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        feature_in(clone, clone / ".worktrees" / "other")
        note = pr_worktree.reuse_note(the_pull_request_in(clone))
        assert f"checked out in {clone / '.worktrees' / 'other'}, so it is used" in note, note
        assert "outside" not in note and "file tools" not in note, "inside the clone the file tools reach it"
        assert "git worktree remove" in note, note

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        git("switch", "-q", "-c", "feature", cwd=clone)
        note = pr_worktree.reuse_note(the_pull_request_in(clone))
        assert "(the main clone itself)" in note, note
        assert "git worktree remove" not in note, "the main worktree cannot be removed"


def test_a_locked_worktree_is_reported_with_its_reason_and_unlocked_before_it_is_removed():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        feature_in(clone, root / "elsewhere", lock="build running")
        note = pr_worktree.reuse_note(the_pull_request_in(clone))
        assert "locked: build running" in note, note
        unlock, remove = (f"git worktree {step} {root / 'elsewhere'}" for step in ("unlock", "remove"))
        assert note.endswith(f"{unlock}; {remove}"), "a locked worktree is not removed until it is unlocked"

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        feature_in(clone, clone / ".worktrees" / "pr-9", lock="build running")
        pr = the_pull_request_in(clone)
        assert pr.held.locked and pr.held.reason == "build running"
        assert pr_worktree.reuse_note(pr) == "", "the lock is said by the refusal, not by a reuse note"


def test_a_lock_reason_is_shown_on_one_printable_line_and_cut_short():
    """The reason is whatever the person or program that locked the worktree wrote."""
    reason = lambda text: pr_worktree.lock_reason(SimpleNamespace(reason=text))
    assert reason("") == "no reason given"
    assert reason("a\x1b[2Jb\nc") == "a?[2Jb?c"
    assert reason("x" * 120) == "x" * 120
    assert reason("x" * 121) == "x" * 120 + "..."


def test_a_step_in_a_locked_worktree_is_refused_with_the_choice_the_person_has():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        feature_in(clone, root / "elsewhere", lock="build running")
        pr = the_pull_request_in(clone)
        for step in ("start", "continue", "check", "push"):
            code, out = quietly(pr_worktree.refuse_locked, pr, "python3 script.py", [step, "9"], False)
            assert code == 1, (step, out)
            assert "(build running)" in out and f"`python3 script.py {step} 9 --use-locked`" in out, out
            assert f"unlock and remove it themselves: git worktree unlock {root / 'elsewhere'}; " in out, out
            assert f"git worktree remove {root / 'elsewhere'}" in out, out
            assert f"use {clone / '.worktrees' / 'pr-9'} instead" in out, out
            assert "no worktree was touched" in out and out.count("build running") == 1, out
        code, out = quietly(pr_worktree.refuse_locked, pr, "python3 script.py", ["start", "9"], True)
        assert code is None and out == "", "the person's yes is the only thing that lets it through"
        code, out = quietly(pr_worktree.refuse_locked, pr, "python3 script.py", ["check", "9", "check spec"], False)
        assert "`python3 script.py check 9 'check spec' --use-locked`" in out, "the retry is the step that was refused"

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        code, out = quietly(pr_worktree.refuse_locked, the_pull_request_in(clone), "python3 script.py", ["start"], False)
        assert code is None and out == "", "a branch nobody has checked out has no lock"
        feature_in(clone, root / "elsewhere")
        code, out = quietly(pr_worktree.refuse_locked, the_pull_request_in(clone), "python3 script.py", ["start"], False)
        assert code is None and out == "", "an unlocked worktree is used without asking"


def test_a_session_started_in_a_locked_worktree_is_not_stopped_by_its_lock():
    """Starting there was the person's choice, and the lock exists to keep other work out of it."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        feature_in(clone, root / "elsewhere", lock="build running")
        pr = the_pull_request_in(clone, here=root / "elsewhere")
        code, out = quietly(pr_worktree.refuse_locked, pr, "python3 script.py", ["start"], False)
        assert code is None and out == "", out
        note = pr_worktree.reuse_note(pr)
        assert "(the checkout this was run from; locked: build running)" in note, note
        assert "outside" not in note and "git worktree remove" not in note, "it is the tree the person is in"


def test_a_step_says_which_worktree_it_works_in_and_then_where_that_came_from():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        pr = the_pull_request_in(clone)
        assert quietly(pr_worktree.announce, pr) == (None, f"worktree {clone / '.worktrees' / 'pr-9'}\n")
        feature_in(clone, root / "elsewhere")
        _, out = quietly(pr_worktree.announce, the_pull_request_in(clone))
        first, second = out.splitlines()
        assert first == f"worktree {root / 'elsewhere'}" and second.startswith("reused: feature is already"), out


def test_the_worktree_line_comes_before_the_error_that_follows_it_when_both_are_captured_together():
    """A pipe buffers stdout and not stderr, so an unflushed line arrives after the error that explains it."""
    script = (
        "import importlib.util\n"
        "from pathlib import Path\n"
        "from types import SimpleNamespace\n"
        f"spec = importlib.util.spec_from_file_location('pw', {str(HERE / 'pr_worktree.py')!r})\n"
        "pw = importlib.util.module_from_spec(spec)\n"
        "spec.loader.exec_module(pw)\n"
        "pw.announce(SimpleNamespace(tree=Path('/x/tree'), checked_out=False, main=Path('/x'), number=9))\n"
        "pw.die('the error')\n"
    )
    done = subprocess.run(
        [sys.executable, "-c", script], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    )
    assert done.stdout.splitlines() == ["worktree /x/tree", "the error"], done.stdout


def test_a_lock_reason_git_wrote_with_quotes_or_a_blank_line_leaves_one_line_and_the_worktree_found():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        clone = a_plain_clone(root)
        feature_in(clone, root / "elsewhere", lock='say "hi"\n\nthen wait')
        pr = the_pull_request_in(clone)
        note = pr_worktree.reuse_note(pr)
        assert pr.checked_out and pr.tree == root / "elsewhere", "the worktree is still the branch's"
        assert "\n" not in note and "locked: " in note and "hi" in note, note

    # An older git prints the reason as it was written, and a blank line in it ends the record.
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        listing = (
            f"worktree {root}\nHEAD 0\nbranch refs/heads/main\n\n"
            f"worktree {root}\nHEAD 0\nbranch refs/heads/feature\nlocked one\n\ntwo"
        )
        with mock.patch.object(pr_worktree, "git", lambda *args, cwd: listing):
            found, main = pr_worktree.worktrees(root)
        assert {branch: (one.locked, one.reason) for branch, one in found.items()} == {
            "main": (False, ""),
            "feature": (True, "one"),
        }, found


def test_each_command_is_logged_and_only_a_failure_prints_its_tail():
    with tempfile.TemporaryDirectory() as tmp:
        tree = Path(tmp).resolve()
        git("init", "-q", str(tree), cwd=tree)
        passing = [sys.executable, "-c", "print('quiet')"]
        # The make line is more than 30 lines from the end, so only the filter for it can print it, and
        # the command text does not spell it out.
        failing = ["sh", "-c", "m=make; echo \"$m: *** [x] Error 1\"; seq 1 40; echo 'the last line'; exit 3"]
        code, out = quietly(pr_worktree.run_commands, tree, [passing], "check.log")
        assert code == 0 and out == f"passed: {' '.join(passing)}\n", out
        code, out = quietly(pr_worktree.run_commands, tree, [passing, failing], "check.log")
        assert code == 1, out
        assert f"passed: {' '.join(passing)}" in out and "failed: sh -c" in out, out
        assert "make: *** [x] Error 1" in out.splitlines() and "the last line" in out, out
        log = tree / ".git" / "check.log"
        assert f"$ {' '.join(failing)}" in log.read_text() and f"full log {log}" in out, out


def test_a_pull_request_from_a_remote_this_clone_lacks_names_the_script_that_adds_it():
    view = {
        "url": "https://github.com/brave/bravebot/pull/9",
        "state": "OPEN",
        "baseRefName": "main",
        "headRefName": "feature",
        "headRepository": {"name": "bravebot"},
        "headRepositoryOwner": {"login": "someone"},
    }
    remotes = {"brave/bravebot": ("upstream", "git@github.com:brave/bravebot.git")}
    for script in ("python3 one.py", "python3 two.py"):
        code, out = quietly(pr_worktree.pull, 9, view, Path("."), remotes, False, script)
        assert code == 1 and f"`{script} start 9` adds one" in out, out


FAKE_SSH = """\
#!/bin/sh
# Refuses the key ssh offers unprompted, and any named key whose file lacks the word `good`.
if [ "$1" = "-i" ] && grep -q good "$2"; then
    for last; do :; done
    exec sh -c "$last"
fi
echo "ERROR: Permission to brave/bravebot.git denied to netzenbot." >&2
exit 128
"""


@contextlib.contextmanager
def an_agent_holding(keys):
    # An ssh command the caller exported would run in place of the fake for the first push.
    with tempfile.TemporaryDirectory() as tmp:
        bin = Path(tmp)
        (bin / "ssh").write_text(FAKE_SSH)
        (bin / "ssh-add").write_text("#!/bin/sh\ncat <<'EOF'\n" + "".join(k + "\n" for k in keys) + "EOF\n")
        for tool in ("ssh", "ssh-add"):
            (bin / tool).chmod(0o755)
        env = {name: value for name, value in os.environ.items() if name not in ("GIT_SSH_COMMAND", "GIT_SSH")}
        env["PATH"] = f"{bin}{os.pathsep}{os.environ['PATH']}"
        with mock.patch.dict(os.environ, env, clear=True):
            yield


def an_ssh_remote(root):
    """A clone whose pushes go to a bare repository over ssh, with a local commit that replaces its head."""
    remote, clone = root / "remote.git", root / "clone"
    git("init", "-q", "--bare", "-b", "main", str(remote), cwd=root)
    git("clone", "-q", str(remote), str(clone), cwd=root)
    (clone / "a.txt").write_text("one\n")
    git("add", "a.txt", cwd=clone)
    git("commit", "-q", "-m", "one", cwd=clone)
    git("push", "-q", "origin", "HEAD:main", cwd=clone)
    git("commit", "-q", "--allow-empty", "-m", "two", cwd=clone)
    git("push", "-q", "origin", "HEAD:feature", cwd=clone)
    git("fetch", "-q", "origin", cwd=clone)
    git("commit", "-q", "--amend", "--allow-empty", "-m", "two again", cwd=clone)
    git("remote", "set-url", "--push", "origin", f"ssh://git@example.invalid{remote}", cwd=clone)
    pr = SimpleNamespace(
        number=9, url="https://github.com/brave/bravebot/pull/9", tree=clone, branch="feature",
        head="feature", head_remote="origin", onto="origin/main", tip="origin/feature",
    )
    return pr, remote


def test_a_push_github_refuses_is_retried_with_each_other_key_the_agent_holds():
    with tempfile.TemporaryDirectory() as tmp:
        pr, remote = an_ssh_remote(Path(tmp).resolve())
        tip = git("rev-parse", pr.tip, cwd=pr.tree)
        keys = ["ssh-ed25519 AAAAbad bot@example.invalid", "ssh-ed25519 AAAAgood person@example.invalid"]
        with an_agent_holding(keys):
            code, out = quietly(pr_worktree.push_head, pr, pr.tree, tip)
        assert code is None and "person@example.invalid" in out, out
        assert "bot@example.invalid" not in out, out
        assert git("rev-parse", "feature", cwd=remote) == git("rev-parse", "HEAD", cwd=pr.tree)


def test_a_push_no_key_in_the_agent_can_make_is_refused_with_githubs_message():
    with tempfile.TemporaryDirectory() as tmp:
        pr, remote = an_ssh_remote(Path(tmp).resolve())
        tip = git("rev-parse", pr.tip, cwd=pr.tree)
        before = git("rev-parse", "feature", cwd=remote)
        with an_agent_holding(["ssh-ed25519 AAAAbad bot@example.invalid"]):
            code, out = quietly(pr_worktree.push_head, pr, pr.tree, tip)
        assert code == 1 and "Permission to brave/bravebot.git denied" in out, out
        assert git("rev-parse", "feature", cwd=remote) == before


def test_what_git_said_is_cut_to_its_error_and_leaves_the_hints_behind():
    said = "Rebasing (1/2)\rerror: gpg failed to sign the data:\n(no gpg output)\nerror: failed to write commit object\nhint: Could not execute\nhint:\nhint:     git rebase --continue\n"
    assert pr_worktree.stop_reason(said) == [
        "error: gpg failed to sign the data:",
        "(no gpg output)",
        "error: failed to write commit object",
    ]
    assert pr_worktree.stop_reason("") == []
    assert pr_worktree.stop_reason("plain words\nmore words\n") == ["more words"], "no error line: the last line"


def main():
    tests = [(name, one) for name, one in globals().items() if name.startswith("test_")]
    for name, one in tests:
        one()
        print(f"  ok    {name[len('test_'):].replace('_', ' ')}")
    print("\npr_worktree selftest passed")


if __name__ == "__main__":
    sys.exit(main())
