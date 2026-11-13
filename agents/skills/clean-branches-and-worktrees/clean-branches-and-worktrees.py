#!/usr/bin/env python3
"""Delete local branches whose pull request merged, and the clean linked worktrees holding them.

    clean-branches-and-worktrees.py [--dry-run]

A branch is kept when it is checked out in the checkout this runs from, is the default branch, has
no merged pull request, has commits past the merged head or its upstream, or is held by a linked
worktree that is dirty, locked or unreadable. Remote branches are never deleted.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path


def run_command(cmd: list[str], cwd: Path | None, check: bool = True) -> subprocess.CompletedProcess[str]:
    if cwd is not None and not cwd.exists():
        raise RuntimeError(f"Working directory not found: {cwd}")
    try:
        proc = subprocess.run(
            cmd,
            cwd=str(cwd) if cwd is not None else None,
            text=True,
            capture_output=True,
        )
    except FileNotFoundError as exc:
        if cwd is not None and not cwd.exists():
            raise RuntimeError(f"Working directory not found: {cwd}") from exc
        raise RuntimeError(f"Command not found: {cmd[0]}") from exc
    if check and proc.returncode != 0:
        rendered = " ".join(cmd)
        raise RuntimeError(
            f"Command failed ({proc.returncode}): {rendered}\nstdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
        )
    return proc


def git_stdout(repo: Path | None, args: list[str], check: bool = True) -> str:
    return run_command(["git", *args], repo, check=check).stdout.strip()


def gh_json(repo: Path, args: list[str]) -> object:
    raw = run_command(["gh", *args], repo, check=True).stdout
    return json.loads(raw or "null")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true", help="Show what would be removed without deleting anything.")
    return parser.parse_args(argv)


def discover_repo_root() -> Path:
    return Path(git_stdout(None, ["rev-parse", "--show-toplevel"])).resolve()


def ensure_gh_authenticated(repo: Path) -> None:
    run_command(["gh", "auth", "status"], repo, check=True)


def fetch_origin(repo: Path) -> None:
    # A dry run fetches too: it touches only remote-tracking refs, and the plan depends on them.
    run_command(["git", "fetch", "--prune", "origin"], repo, check=True)


def get_current_branch(repo: Path) -> str:
    return git_stdout(repo, ["rev-parse", "--abbrev-ref", "HEAD"])


def gh_default_branch(repo: Path) -> str | None:
    payload = gh_json(repo, ["repo", "view", "--json", "defaultBranchRef"])
    if not isinstance(payload, dict):
        return None
    default_branch_ref = payload.get("defaultBranchRef")
    if not isinstance(default_branch_ref, dict):
        return None
    name = default_branch_ref.get("name")
    if isinstance(name, str) and name.strip():
        return name.strip()
    return None


def remote_show_default_branch(repo: Path) -> str | None:
    proc = run_command(["git", "remote", "show", "origin"], repo, check=False)
    if proc.returncode != 0:
        return None
    for line in proc.stdout.splitlines():
        stripped = line.strip()
        if stripped.startswith("HEAD branch:"):
            name = stripped.split(":", 1)[1].strip()
            if not name or name == "(unknown)":
                return None
            remote_ref = f"refs/remotes/origin/{name}"
            local_ref = f"refs/heads/{name}"
            remote_exists = run_command(["git", "show-ref", "--verify", "--quiet", remote_ref], repo, check=False).returncode == 0
            local_exists = run_command(["git", "show-ref", "--verify", "--quiet", local_ref], repo, check=False).returncode == 0
            if remote_exists or local_exists:
                return name
            return None
    return None


def get_default_branch(repo: Path) -> str:
    gh_default = gh_default_branch(repo)
    if gh_default:
        return gh_default

    proc = run_command(["git", "symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"], repo, check=False)
    if proc.returncode == 0:
        remote_head = proc.stdout.strip()
        if remote_head.startswith("origin/"):
            return remote_head.split("/", 1)[1]

    remote_show = remote_show_default_branch(repo)
    if remote_show:
        return remote_show

    raise RuntimeError("Could not determine repository default branch; aborting cleanup")


def list_local_branches(repo: Path) -> list[str]:
    out = git_stdout(repo, ["for-each-ref", "refs/heads", "--format=%(refname:lstrip=2)"])
    return [line.strip() for line in out.splitlines() if line.strip()]


def get_branch_head(repo: Path, branch: str) -> str:
    return git_stdout(repo, ["rev-parse", "--verify", "--end-of-options", f"refs/heads/{branch}"])


def resolve_revision(repo: Path, revision: str) -> str:
    return git_stdout(repo, ["rev-parse", "--verify", "--end-of-options", revision])


def try_resolve_revision(repo: Path, revision: str) -> str | None:
    try:
        return resolve_revision(repo, revision)
    except RuntimeError:
        return None


def find_merged_pull_request(repo: Path, branch: str) -> dict[str, object] | None:
    payload = gh_json(
        repo,
        [
            "pr",
            "list",
            "--state",
            "merged",
            "--head",
            branch,
            "--json",
            "number,title,url,mergedAt,headRefName,headRefOid",
        ],
    )
    if not isinstance(payload, list):
        return None
    merged = [
        item
        for item in payload
        if isinstance(item, dict) and item.get("mergedAt") and item.get("headRefName") == branch
    ]
    if not merged:
        return None
    merged.sort(key=lambda item: str(item.get("mergedAt") or ""))
    return merged[-1]


def diff_counts(repo: Path, left: str, right: str) -> tuple[int, int] | None:
    left_oid = try_resolve_revision(repo, left)
    right_oid = try_resolve_revision(repo, right)
    if left_oid is None or right_oid is None:
        return None
    proc = run_command(["git", "rev-list", "--left-right", "--count", f"{left_oid}...{right_oid}"], repo, check=False)
    if proc.returncode != 0:
        return None
    parts = proc.stdout.strip().split()
    if len(parts) != 2:
        raise RuntimeError(f"Unexpected git rev-list output for {left}...{right}: {proc.stdout!r}")
    return int(parts[0]), int(parts[1])


def branch_matches_merged_pr(repo: Path, branch: str, pr_head_sha: str) -> tuple[bool, str]:
    local_head = get_branch_head(repo, branch)
    if local_head == pr_head_sha:
        return True, ""
    counts = diff_counts(repo, pr_head_sha, f"refs/heads/{branch}")
    if counts is None:
        return False, f"could not compare local branch against merged PR head {pr_head_sha[:12]}"
    _, branch_only = counts
    if branch_only > 0:
        return False, "local branch has commits beyond the merged PR head"
    return True, ""


def get_tracking_branch(repo: Path, branch: str) -> str | None:
    upstream = git_stdout(repo, ["for-each-ref", "--format=%(upstream:short)", f"refs/heads/{branch}"], check=False)
    return upstream or None


def branch_has_unpushed_commits(repo: Path, branch: str, upstream: str) -> tuple[bool, int, int] | None:
    # A merged PR often deletes the remote branch, so a missing upstream ref
    # should not block local cleanup once the PR-head safety gate has passed.
    if try_resolve_revision(repo, upstream) is None:
        return (False, 0, 0)
    counts = diff_counts(repo, f"refs/heads/{branch}", upstream)
    if counts is None:
        return None
    ahead, behind = counts
    return ahead > 0, ahead, behind


def list_worktrees(repo: Path) -> list[dict[str, object]]:
    out = git_stdout(repo, ["worktree", "list", "--porcelain"])
    worktrees: list[dict[str, object]] = []
    current: dict[str, object] = {}
    for line in [*out.splitlines(), ""]:
        if not line:
            if current:
                path = Path(str(current["path"])).resolve()
                branch_ref = str(current.get("branch") or "")
                branch = branch_ref.split("refs/heads/", 1)[1] if branch_ref.startswith("refs/heads/") else None
                worktrees.append(
                    {
                        "path": str(path),
                        "branch": branch,
                        # git lists the main checkout first, whichever worktree this runs from.
                        "is_primary": not worktrees,
                        "locked": bool(current.get("locked")),
                    }
                )
                current = {}
            continue
        key, _, value = line.partition(" ")
        if key == "worktree":
            current["path"] = value
        elif key == "branch":
            current["branch"] = value
        elif key == "locked":
            current["locked"] = True
    return worktrees


def worktree_is_clean(worktree_path: str) -> bool:
    # Explicit, because status.showUntrackedFiles=no would hide a file that worktree remove deletes.
    return git_stdout(Path(worktree_path), ["status", "--porcelain", "--untracked-files=all"]) == ""


def classify_linked_worktrees(linked_worktrees: list[dict[str, object]]) -> tuple[list[str], list[str]]:
    dirty_paths: list[str] = []
    unreadable_paths: list[str] = []
    for worktree in linked_worktrees:
        worktree_path = str(worktree["path"])
        try:
            if not worktree_is_clean(worktree_path):
                dirty_paths.append(worktree_path)
        except RuntimeError:
            unreadable_paths.append(worktree_path)
    return dirty_paths, unreadable_paths


def remove_worktree(repo: Path, worktree_path: str, dry_run: bool) -> None:
    if dry_run:
        print(f"[dry-run] ({repo}) $ git worktree remove -- {worktree_path}")
        return
    run_command(["git", "worktree", "remove", "--", worktree_path], repo, check=True)


def delete_branch(repo: Path, branch: str, dry_run: bool) -> None:
    if dry_run:
        print(f"[dry-run] ({repo}) $ git branch -D -- {branch}")
        return
    run_command(["git", "branch", "-D", "--", branch], repo, check=True)


def prune_worktrees(repo: Path, dry_run: bool) -> None:
    if dry_run:
        print(f"[dry-run] ({repo}) $ git worktree prune")
        return
    run_command(["git", "worktree", "prune"], repo, check=True)


def prune_remote(repo: Path, dry_run: bool) -> None:
    if dry_run:
        print(f"[dry-run] ({repo}) $ git remote prune origin")
        return
    run_command(["git", "remote", "prune", "origin"], repo, check=True)


def format_pr(pr: dict[str, object]) -> str:
    return f"#{pr['number']} {pr['title']}"


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        repo = discover_repo_root()
        ensure_gh_authenticated(repo)
        fetch_origin(repo)
        default_branch = get_default_branch(repo)
        current_branch = get_current_branch(repo)
        # Drops only records of worktree folders already gone, so a dry run prunes too.
        run_command(["git", "worktree", "prune"], repo, check=True)
        worktrees = list_worktrees(repo)
        deleted_branches: list[str] = []
        removed_worktrees: list[tuple[str, str]] = []
        skipped: list[tuple[str, str]] = []

        for branch in list_local_branches(repo):
            if branch == current_branch:
                skipped.append((branch, "currently checked out"))
                continue
            if default_branch and branch == default_branch:
                skipped.append((branch, "repository default branch"))
                continue
            if any(worktree["is_primary"] and worktree["branch"] == branch for worktree in worktrees):
                skipped.append((branch, "checked out in the main checkout"))
                continue

            try:
                pr = find_merged_pull_request(repo, branch)
            except (RuntimeError, ValueError) as exc:
                skipped.append((branch, f"could not look up its pull request: {str(exc).splitlines()[0]}"))
                continue
            if pr is None:
                skipped.append((branch, "no merged pull request found"))
                continue

            pr_head_sha = str(pr.get("headRefOid") or "")
            if not pr_head_sha:
                skipped.append((branch, f"{format_pr(pr)} is missing a PR head SHA"))
                continue

            matches_pr, mismatch_reason = branch_matches_merged_pr(repo, branch, pr_head_sha)
            if not matches_pr:
                skipped.append((branch, f"{format_pr(pr)}: {mismatch_reason}"))
                continue

            upstream = get_tracking_branch(repo, branch)
            # A squash merge leaves a branch tracking the default branch ahead of it, and the
            # merged-head check above already holds every one of those commits.
            if upstream is not None and upstream != f"origin/{default_branch}":
                unpushed_result = branch_has_unpushed_commits(repo, branch, upstream)
                if unpushed_result is None:
                    skipped.append((branch, f"{format_pr(pr)}: could not compare branch against upstream {upstream}"))
                    continue
                has_unpushed, ahead, _behind = unpushed_result
                if has_unpushed:
                    plural = "" if ahead == 1 else "s"
                    skipped.append((branch, f"{format_pr(pr)}: {ahead} unpushed tracked commit{plural}"))
                    continue

            linked_worktrees = [
                worktree for worktree in worktrees if worktree.get("branch") == branch and not worktree.get("is_primary")
            ]
            locked_paths = [str(worktree["path"]) for worktree in linked_worktrees if worktree.get("locked")]
            dirty_paths, unreadable_paths = classify_linked_worktrees(linked_worktrees)
            if locked_paths or dirty_paths or unreadable_paths:
                reasons: list[str] = []
                if locked_paths:
                    reasons.append(f"locked linked worktree(s): {', '.join(locked_paths)}")
                if dirty_paths:
                    reasons.append(f"dirty linked worktree(s): {', '.join(dirty_paths)}")
                if unreadable_paths:
                    reasons.append(f"unreadable linked worktree(s): {', '.join(unreadable_paths)}")
                skipped.append((branch, f"{format_pr(pr)}: {'; '.join(reasons)}"))
                continue

            try:
                for worktree in linked_worktrees:
                    worktree_path = str(worktree["path"])
                    remove_worktree(repo, worktree_path, args.dry_run)
                    removed_worktrees.append((branch, worktree_path))
                delete_branch(repo, branch, args.dry_run)
            except RuntimeError as exc:
                skipped.append((branch, f"{format_pr(pr)}: {str(exc).splitlines()[0]}"))
                continue
            deleted_branches.append(f"{branch} ({format_pr(pr)})")

        if deleted_branches or removed_worktrees:
            prune_worktrees(repo, args.dry_run)
            prune_remote(repo, args.dry_run)

        action = "Would delete" if args.dry_run else "Deleted"
        removed_action = "Would remove" if args.dry_run else "Removed"
        print(f"{action} {len(deleted_branches)} branch(es).")
        if deleted_branches:
            print("Branches:")
            for item in deleted_branches:
                print(f"- {item}")

        print(f"{removed_action} {len(removed_worktrees)} linked worktree(s).")
        if removed_worktrees:
            print("Worktrees:")
            for branch, path in removed_worktrees:
                print(f"- {path} ({branch})")

        if skipped:
            print("Skipped:")
            for branch, reason in skipped:
                print(f"- {branch}: {reason}")

        return 0
    except RuntimeError as exc:
        print(str(exc), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
