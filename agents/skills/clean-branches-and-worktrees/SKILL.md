---
name: clean-branches-and-worktrees
description:
  'Delete local branches whose GitHub pull request merged, and remove the clean linked worktrees
  holding them. Keeps any branch with local-only work. Triggers on: clean branches, delete merged
  branches, prune branches, branch cleanup, clean worktrees.'
allowed-tools: Bash(python3 agents/skills/clean-branches-and-worktrees/clean-branches-and-worktrees.py *), Bash(gh auth status)
---

# Clean branches and worktrees

`clean-branches-and-worktrees.py` decides everything. Run it from the checkout to clean, usually the main clone:

1. `gh auth status`. If it fails, stop and tell the person.
2. `python3 agents/skills/clean-branches-and-worktrees/clean-branches-and-worktrees.py --dry-run`, and show the person what it would delete.
3. When the person agrees, run it again without `--dry-run`.
4. Report the deleted branches, the removed worktrees, and each skipped branch with its reason.

It fetches `origin` with `--prune`, then for each local branch looks up a merged pull request with that head on GitHub. It deletes the branch with `git branch -D`, so a squash-merged branch is deleted, after removing every linked worktree that holds it. It keeps a branch when:

- it is checked out in the checkout the script runs from, or is the default branch
- no merged pull request has it as head
- it has commits past the merged pull request's head, or past its upstream
- a linked worktree holding it is dirty, locked or unreadable

After deleting anything it runs `git worktree prune` and `git remote prune origin`. It never deletes a remote branch.

Do not unlock a worktree, clean a dirty one, or delete a skipped branch yourself. The skip reason says what the person can do.
