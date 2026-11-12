#!/usr/bin/env python3
"""What clean-branches-and-worktrees.py deletes and keeps, against real repositories and a fake gh."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "clean-branches-and-worktrees.py"

FAKE_GH = f"""#!{sys.executable}
import json, os, sys
args = sys.argv[1:]
if args[:2] == ["auth", "status"]:
    sys.exit(0)
if args[:2] == ["repo", "view"]:
    print(json.dumps({{"defaultBranchRef": {{"name": "main"}}}}))
    sys.exit(0)
if args[:2] == ["pr", "list"]:
    prs = json.loads(open(os.environ["FAKE_GH_PRS"]).read())
    print(json.dumps(prs.get(args[args.index("--head") + 1], [])))
    sys.exit(0)
sys.exit(1)
"""


# No person's git configuration, hooks or signing reaches these repositories.
GIT_ENV = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
GIT_ENV.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1",
               GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@example.com",
               GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@example.com")


def git(*args, cwd):
    return subprocess.run(["git", *args], cwd=cwd, env=GIT_ENV, check=True, capture_output=True,
                          text=True).stdout.strip()


class Clean(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name).resolve()
        self.env = dict(GIT_ENV)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        (bin_dir / "gh").write_text(FAKE_GH)
        (bin_dir / "gh").chmod(0o755)
        self.prs_file = root / "prs.json"
        self.env.update(PATH=f"{bin_dir}{os.pathsep}{self.env['PATH']}", FAKE_GH_PRS=str(self.prs_file))
        self.prs = {}

        self.root = root
        self.origin = root / "origin.git"
        git("init", "-q", "--bare", "-b", "main", str(self.origin), cwd=root)
        self.clone = root / "clone"
        git("clone", "-q", str(self.origin), str(self.clone), cwd=root)
        git("commit", "-q", "--allow-empty", "-m", "base", cwd=self.clone)
        git("push", "-q", "origin", "main", cwd=self.clone)
        self.worktrees = self.clone / ".worktrees"

    def branch(self, name, merged=True, worktree=False):
        """A branch with one commit; a merged pull request at that commit when `merged`."""
        git("branch", name, "main", cwd=self.clone)
        if worktree:
            path = self.worktrees / name
            git("worktree", "add", "-q", str(path), name, cwd=self.clone)
            git("commit", "-q", "--allow-empty", "-m", name, cwd=path)
        else:
            git("checkout", "-q", name, cwd=self.clone)
            git("commit", "-q", "--allow-empty", "-m", name, cwd=self.clone)
            git("checkout", "-q", "main", cwd=self.clone)
        if merged:
            self.merged(name, git("rev-parse", name, cwd=self.clone))
        return self.worktrees / name

    def merged(self, name, head):
        self.prs[name] = [{"number": 1, "title": name, "url": "u", "mergedAt": "2026-01-01T00:00:00Z",
                           "headRefName": name, "headRefOid": head}]

    def run_script(self, *args):
        self.prs_file.write_text(json.dumps(self.prs))
        result = subprocess.run([sys.executable, str(SCRIPT), *args], cwd=self.clone, env=self.env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def branches(self):
        return set(git("for-each-ref", "refs/heads", "--format=%(refname:short)", cwd=self.clone).split())

    def test_deletes_a_merged_branch_and_removes_its_clean_worktree(self):
        path = self.branch("done", worktree=True)
        out = self.run_script()
        self.assertNotIn("done", self.branches())
        self.assertFalse(path.exists())
        self.assertIn("Deleted 1 branch(es).", out)
        self.assertIn("Removed 1 linked worktree(s).", out)

    def test_deletes_a_squash_merged_branch_that_tracks_the_default_branch(self):
        git("branch", "--track", "squashed", "origin/main", cwd=self.clone)
        git("checkout", "-q", "squashed", cwd=self.clone)
        git("commit", "-q", "--allow-empty", "-m", "squashed", cwd=self.clone)
        git("checkout", "-q", "main", cwd=self.clone)
        self.merged("squashed", git("rev-parse", "squashed", cwd=self.clone))
        out = self.run_script()
        self.assertNotIn("squashed", self.branches(), out)

    def test_keeps_a_branch_ahead_of_its_own_upstream(self):
        self.branch("ahead", merged=False)
        git("push", "-q", "-u", "origin", "ahead", cwd=self.clone)
        self.merged("ahead", git("rev-parse", "ahead", cwd=self.clone))
        git("checkout", "-q", "ahead", cwd=self.clone)
        git("commit", "-q", "--allow-empty", "-m", "unpushed", cwd=self.clone)
        git("checkout", "-q", "main", cwd=self.clone)
        # The PR head moves to the unpushed commit so only the upstream check can keep the branch.
        self.merged("ahead", git("rev-parse", "ahead", cwd=self.clone))
        out = self.run_script()
        self.assertIn("ahead", self.branches())
        self.assertIn("1 unpushed tracked commit", out)

    def test_dry_run_deletes_nothing(self):
        path = self.branch("done", worktree=True)
        out = self.run_script("--dry-run")
        self.assertIn("done", self.branches())
        self.assertTrue(path.exists())
        self.assertIn("Would delete 1 branch(es).", out)
        self.assertIn("Would remove 1 linked worktree(s).", out)

    def test_deletes_a_merged_branch_whose_remote_branch_was_deleted(self):
        self.branch("gone", merged=False)
        git("push", "-q", "-u", "origin", "gone", cwd=self.clone)
        self.merged("gone", git("rev-parse", "gone", cwd=self.clone))
        git("push", "-q", "origin", "--delete", "gone", cwd=self.clone)
        out = self.run_script()
        self.assertNotIn("gone", self.branches(), out)

    def test_keeps_the_default_branch_when_another_is_checked_out(self):
        self.branch("open", merged=False)
        git("checkout", "-q", "open", cwd=self.clone)
        out = self.run_script()
        self.assertEqual(self.branches(), {"main", "open"})
        self.assertIn("- main: repository default branch", out)
        self.assertIn("- open: currently checked out", out)

    def test_dry_run_fetches_a_merged_head_pushed_from_elsewhere(self):
        """The local branch is behind a head only origin has, so the plan needs the fetch."""
        self.branch("pushed", merged=False)
        git("push", "-q", "origin", "pushed", cwd=self.clone)
        other = self.root / "other"
        git("clone", "-q", "-b", "pushed", str(self.origin), str(other), cwd=self.root)
        git("commit", "-q", "--allow-empty", "-m", "from elsewhere", cwd=other)
        git("push", "-q", "origin", "pushed", cwd=other)
        self.merged("pushed", git("rev-parse", "HEAD", cwd=other))
        out = self.run_script("--dry-run")
        self.assertIn("Would delete 1 branch(es).", out)
        self.assertIn("pushed", self.branches())

    def test_keeps_the_current_branch_and_unmerged_work(self):
        self.branch("open", merged=False)
        out = self.run_script()
        self.assertEqual(self.branches(), {"main", "open"})
        self.assertIn("- main: currently checked out", out)
        self.assertIn("- open: no merged pull request found", out)

    def test_keeps_a_branch_with_commits_past_the_merged_head(self):
        path = self.branch("more", worktree=True)
        git("commit", "-q", "--allow-empty", "-m", "after merge", cwd=path)
        out = self.run_script()
        self.assertIn("more", self.branches())
        self.assertIn("local branch has commits beyond the merged PR head", out)

    def test_keeps_a_branch_whose_worktree_is_dirty(self):
        path = self.branch("dirty", worktree=True)
        (path / "wip.txt").write_text("wip\n")
        out = self.run_script()
        self.assertIn("dirty", self.branches())
        self.assertTrue((path / "wip.txt").exists())
        self.assertIn("dirty linked worktree(s)", out)

    def test_keeps_an_untracked_file_the_persons_config_hides_from_status(self):
        git("config", "status.showUntrackedFiles", "no", cwd=self.clone)
        path = self.branch("hidden", worktree=True)
        (path / "wip.txt").write_text("wip\n")
        out = self.run_script()
        self.assertIn("hidden", self.branches())
        self.assertTrue((path / "wip.txt").exists())
        self.assertIn("dirty linked worktree(s)", out)

    def test_keeps_a_branch_whose_worktree_is_locked(self):
        path = self.branch("held", worktree=True)
        git("worktree", "lock", "--reason", "in use", str(path), cwd=self.clone)
        out = self.run_script()
        self.assertIn("held", self.branches())
        self.assertTrue(path.exists())
        self.assertIn("locked linked worktree(s)", out)


if __name__ == "__main__":
    unittest.main()
