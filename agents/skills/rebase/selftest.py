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
from types import SimpleNamespace

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
    with tempfile.TemporaryDirectory() as tmp:
        bin = Path(tmp)
        (bin / "ssh").write_text(FAKE_SSH)
        (bin / "ssh-add").write_text("#!/bin/sh\ncat <<'EOF'\n" + "".join(k + "\n" for k in keys) + "EOF\n")
        for tool in ("ssh", "ssh-add"):
            (bin / tool).chmod(0o755)
        path = os.environ["PATH"]
        os.environ["PATH"] = f"{bin}{os.pathsep}{path}"
        try:
            yield
        finally:
            os.environ["PATH"] = path


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
        keys = ["ssh-ed25519 AAAAbad bot@example.invalid", "ssh-ed25519 AAAAgood person@example.invalid"]
        with an_agent_holding(keys):
            code, out = quietly(rebase.push, pr)
        assert code == 0 and "person@example.invalid" in out, out
        assert "bot@example.invalid" not in out, out
        assert git("rev-parse", "feature", cwd=remote) == git("rev-parse", "HEAD", cwd=pr.tree)


def test_a_push_no_key_in_the_agent_can_make_is_refused_with_githubs_message():
    with tempfile.TemporaryDirectory() as tmp:
        pr, remote = an_ssh_remote(Path(tmp).resolve())
        before = git("rev-parse", "feature", cwd=remote)
        with an_agent_holding(["ssh-ed25519 AAAAbad bot@example.invalid"]):
            code, out = quietly(rebase.push, pr)
        assert code == 1 and "Permission to brave/bravebot.git denied" in out, out
        assert git("rev-parse", "feature", cwd=remote) == before


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


def test_the_resolved_files_choose_the_checks_and_a_file_no_rule_names_chooses_none():
    with tempfile.TemporaryDirectory() as tmp:
        tree = Path(tmp)
        for crate in ("agent", "tui"):
            (tree / "crates" / crate).mkdir(parents=True)
            (tree / "crates" / crate / "Cargo.toml").write_text(f'[package]\nname = "bravebot-{crate}"\n')
        plan = lambda *files: rebase.plan(tree, list(files))

        assert plan("ui/src/app.ts", "docs/website/docs/index.md", "README.md", "docs/development/checks.md") == []
        assert plan("docs/specs/labels.md") == [["make", "-k", "check-spec"]]
        assert plan("crates/i18n/locales/fr.ftl") == [["make", "-k", "check-locales"]]
        assert plan("Cargo.lock") == [rebase.LOCK_RESOLVES, ["make", "-k", "check-versions"]]
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


def main():
    tests =[(name, one) for name, one in globals().items() if name.startswith("test_")]
    for name, one in tests:
        one()
        print(f"  ok    {name[len('test_'):].replace('_', ' ')}")
    print("\nrebase selftest passed")


if __name__ == "__main__":
    sys.exit(main())
