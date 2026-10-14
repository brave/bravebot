#!/usr/bin/env python3
"""Selftest for pr_worktree.py, which rebase.py and pr-fix.py share, against real repositories where git decides."""

import contextlib
import importlib.util
import io
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
        assert found == {"main": clone}, found
        assert main == clone


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


def main():
    tests = [(name, one) for name, one in globals().items() if name.startswith("test_")]
    for name, one in tests:
        one()
        print(f"  ok    {name[len('test_'):].replace('_', ' ')}")
    print("\npr_worktree selftest passed")


if __name__ == "__main__":
    sys.exit(main())
