---
name: clean-branches-and-worktrees
description:
  'Delete local branches whose GitHub pull request merged, and remove the clean linked worktrees
  holding them. Keeps any branch with local-only work. Can also delete ignored build directories from
  the worktrees it keeps. Triggers on: clean branches, delete merged branches, prune branches, branch
  cleanup, clean worktrees, reclaim disk space, delete build output.'
allowed-tools: Bash(python3 agents/skills/clean-branches-and-worktrees/clean-branches-and-worktrees.py *), Bash(gh auth status)
---

# Clean branches and worktrees

`clean-branches-and-worktrees.py` decides everything. Run it from the checkout to clean, usually the main clone:

1. `gh auth status`. If it fails, stop and tell the person.
2. `python3 agents/skills/clean-branches-and-worktrees/clean-branches-and-worktrees.py --dry-run`, and show the person what it would delete.
3. When the person agrees, run it again without `--dry-run`.
4. Report the deleted branches, the removed worktrees, and each skipped branch with its reason.

When the person wants disk space back, add `--prune-build-dirs` to both runs. Each linked worktree that is still there afterwards, merged or not, loses every directory named `target`, `node_modules` or `dist` that Git ignores and that holds no tracked file. The dry run lists each one with its size. Other ignored files, such as a local `.env`, are kept. The main checkout, the checkout the script runs from, and locked worktrees keep their build directories. Tell the person that the next build in those worktrees starts from nothing, and that a build running in one of them will fail.

It fetches `origin` with `--prune`, in a dry run too, so both runs decide from the same refs. It runs `git worktree prune` the same way, which forgets only unlocked worktrees whose folder is already gone. Then, for each local branch, it looks up a merged pull request with that head on GitHub. When that pull request's head commit is not in the local repository, as after a force-push to the branch, it fetches `refs/pull/<number>/head` from `origin` to compare against. It deletes the branch with `git branch -D`, so a squash-merged branch is deleted, after removing every linked worktree that holds it. It keeps a branch when:

- it is checked out in the checkout the script runs from or in the main checkout, or is the default branch
- no merged pull request has it as head, or `gh` failed to look one up
- it has commits past the merged pull request's head, or past its upstream
- a linked worktree holding it is dirty, locked or unreadable

A removed worktree takes its ignored files with it, such as `target/` or a local `.env`. Neither run lists them, so tell the person before they agree.

After deleting anything it runs `git worktree prune` and `git remote prune origin`. It never deletes a remote branch.

Do not unlock a worktree, clean a dirty one, or delete a skipped branch yourself. The skip reason says what the person can do.
