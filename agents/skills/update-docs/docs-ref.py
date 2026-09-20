#!/usr/bin/env python3
"""Track how far the site under docs/website has fallen behind the specs.

Every page on the site describes behaviour specified clause by clause in docs/specs.
`docs-updated-to-sha` at the repository root records the commit the site has been brought
up to, so an update knows where to start reading rather than re-reading the whole history.

This script is the deterministic half of the update-docs skill. It reports what has landed
since the recorded commit and rewrites the record. No model is involved, so its output is
reproducible and cheap.

Usage:
    python3 agents/skills/update-docs/docs-ref.py show
    python3 agents/skills/update-docs/docs-ref.py changes [--all] [--full]
    python3 agents/skills/update-docs/docs-ref.py set <rev>
    python3 agents/skills/update-docs/docs-ref.py defer <rev> <why>
    python3 agents/skills/update-docs/docs-ref.py resolve <rev>

A commit deliberately left for a later run is recorded with `defer`, because the baseline
alone cannot express it: `set` claims everything up to a sha has been folded in, and the
next span starts after it, so a commit skipped mid-span becomes invisible to every run
that follows. `changes` replays whatever is still deferred ahead of the new span.
"""

import argparse
import re
# Only ever invoked with an argument list, never a shell string.
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
from pathlib import Path

# .../<repo>/agents/skills/update-docs/docs-ref.py -> parents[3] == <repo>
_ROOT = Path(__file__).resolve().parents[3]
_REF_FILE = _ROOT / "docs-updated-to-sha"

# Commits reviewed and consciously left undocumented, one `<sha> <reason>` per line. Kept
# beside the baseline rather than in it because the two answer different questions: the
# baseline is how far reading got, this is what reading decided to come back to.
_DEFERRED_FILE = _ROOT / "docs-deferred"

_SOURCE_URL = "https://github.com/brave/bravebot"
_SHA = re.compile(r"^[0-9a-f]{40}$")

# A behaviour change is supposed to arrive with the spec clause that governs it, so these are
# the paths that can make the site wrong. Code under crates/ matters only through its specs,
# and `--all` is there for when that assumption needs checking.
#
# Listed rather than taking docs/ wholesale for two reasons. docs/development, docs/design and
# docs/best-practices are how this repository is worked on rather than what the product does,
# and the site describes the product. And docs/website is the site itself, so including it
# would count a page edit as work the site is behind on and make every baseline bump enqueue
# itself.
_DOC_PATHS = [
    "docs/specs/",
    "docs/getting-started.md",
    "docs/command-line-explained.md",
    "README.md",
]


class Problem(Exception):
    """Anything the user has to fix, reported without a traceback."""


def _git(*args: str) -> str:
    result = subprocess.run(["git", "-C", str(_ROOT), *args],
                            capture_output=True,
                            text=True)
    if result.returncode != 0:
        raise Problem(f"git {' '.join(args)} failed:\n{result.stderr.strip()}")
    return result.stdout.strip()


def read_ref() -> str:
    """The recorded sha: the last line that is neither blank nor a comment."""
    if not _REF_FILE.is_file():
        raise Problem(f"{_REF_FILE.name} is missing.")
    lines = [
        line.strip() for line in _REF_FILE.read_text().splitlines()
        if line.strip() and not line.strip().startswith("#")
    ]
    if not lines:
        raise Problem(f"{_REF_FILE.name} records no sha.")
    sha = lines[-1]
    if not _SHA.match(sha):
        raise Problem(f"{_REF_FILE.name} holds {sha!r}, not a 40 character sha.")
    return sha


def write_ref(sha: str, subject: str, date: str) -> None:
    """Rewrite the record, keeping the commit link in step with the sha."""
    _REF_FILE.write_text(f"""# The commit the documentation site is current as of.
#
# Every page under docs/website describes behaviour specified clause by clause in
# docs/specs. This file records how far along that history the site has been brought, so
# the next update knows where to start reading.
#
# Run `make docs-changes` to see what has landed since, and the update-docs skill
# (agents/skills/update-docs) to fold it in. The skill rewrites the sha below; nothing
# else should.
#
# {subject}
# {date}
# {_SOURCE_URL}/commit/{sha}

{sha}
""")


def read_deferred() -> list[tuple[str, str]]:
    """Every (sha, reason) still waiting, oldest recorded first."""
    if not _DEFERRED_FILE.is_file():
        return []
    out = []
    for line in _DEFERRED_FILE.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        sha, _, reason = line.partition(" ")
        if _SHA.match(sha):
            out.append((sha, reason.strip()))
    return out


def write_deferred(entries: list[tuple[str, str]]) -> None:
    """Rewrite the ledger, or remove it once nothing is deferred."""
    if not entries:
        if _DEFERRED_FILE.is_file():
            _DEFERRED_FILE.unlink()
        return
    body = "\n".join(f"{sha} {reason}" for sha, reason in entries)
    _DEFERRED_FILE.write_text(
        "# Commits reviewed by the update-docs skill and deliberately left for a later\n"
        "# run, with why. The baseline in docs-updated-to-sha cannot hold these: it says\n"
        "# everything before it was folded in, and the next span begins after it, so a\n"
        "# commit skipped mid-span would never be offered again.\n"
        "#\n"
        "# `docs-ref.py changes` replays these ahead of the new span. Drop one with\n"
        "# `docs-ref.py resolve <sha>` once its behaviour is documented.\n"
        "\n"
        f"{body}\n")


def _describe(rev: str) -> tuple[str, str, str]:
    """(sha, date, subject) for one revision, or a Problem if it is unknown here."""
    try:
        line = _git("log", "-1", "--format=%H%x00%cs%x00%s", rev)
    except Problem:
        raise Problem(f"{rev} is not a commit here.\n"
                      f"If it is newer than this checkout, fetch first: git fetch")
    sha, date, subject = line.split("\0")
    return sha, date, subject


def _resolve_head() -> str:
    """Prefer a fetched canonical main over whatever the checkout happens to be on.

    A local checkout is frequently mid-branch, and the site tracks what has been
    published, not somebody's work in progress.
    """
    for rev in ("upstream/main", "origin/main", "main", "HEAD"):
        try:
            return _git("rev-parse", "--verify", f"{rev}^{{commit}}")
        except Problem:
            continue
    raise Problem("Cannot resolve a head commit.")


def show(_args) -> int:
    ref = read_ref()
    ref_sha, ref_date, ref_subject = _describe(ref)
    head_sha, head_date, head_subject = _describe(_resolve_head())

    behind = _git("rev-list", "--count", f"{ref_sha}..{head_sha}")
    doc_behind = _git("rev-list", "--count", f"{ref_sha}..{head_sha}", "--",
                      *_DOC_PATHS) if behind != "0" else "0"

    print(f"docs current as of   {ref_sha[:9]}  {ref_date}  {ref_subject}")
    print(f"head                 {head_sha[:9]}  {head_date}  {head_subject}")
    print(f"link                 {_SOURCE_URL}/commit/{ref_sha}")
    print()
    deferred = read_deferred()
    if deferred:
        print(f"deferred             {len(deferred)} commit(s) awaiting a decision "
              f"(see `make docs-changes`)")
    if behind == "0":
        print("Up to date." if not deferred else
              "Baseline is current, but deferred commits are still undocumented.")
    else:
        print(f"{behind} commit(s) behind, {doc_behind} of them touching "
              f"{', '.join(_DOC_PATHS)}.")
        print("Run `make docs-changes` to see them.")
    return 0


def _print_deferred() -> None:
    """Replay the ledger ahead of the span, since the span itself excludes it."""
    entries = read_deferred()
    if not entries:
        return
    print("Deferred by earlier runs. These are behind the baseline and will not appear")
    print("below. Decide each one again:")
    for sha, reason in entries:
        try:
            _, date, subject = _describe(sha)
        except Problem:
            print(f"  {sha[:9]}  (not in this checkout: fetch, or resolve it)  {reason}")
            continue
        print(f"  {sha[:9]}  {date}  {subject}")
        print(f"             deferred because: {reason}")
    print()


def changes(args) -> int:
    ref_sha, _, _ = _describe(read_ref())
    head_sha, _, _ = _describe(_resolve_head())
    span = f"{ref_sha}..{head_sha}"

    if ref_sha == head_sha:
        print(f"Up to date at {ref_sha[:9]}. Nothing new to fold in.")
        print()
        _print_deferred()
        return 0

    paths = [] if args.all else _DOC_PATHS
    scope = "every path" if args.all else ", ".join(_DOC_PATHS)
    print(f"{ref_sha[:9]}..{head_sha[:9]}, commits touching {scope}, oldest first")
    print(f"new ref once folded in: {head_sha}")
    print()

    _print_deferred()

    count = 0
    for record in _git("log", "--reverse", "--no-merges", "--format=%H%x00%cs%x00%s",
                       span, "--", *paths).splitlines():
        if not record.strip():
            continue
        sha, date, subject = record.split("\0")
        count += 1
        print(f"{sha[:9]}  {date}  {subject}")
        if args.full:
            body = _git("log", "-1", "--format=%b", sha).strip()
            for line in body.splitlines():
                print(f"    {line}")
            files = _git("show", "--name-status", "--format=", sha, "--", *paths)
            for line in files.splitlines():
                print(f"    | {line}")
            print()

    if not count:
        print("(none)")
    else:
        print()
        print(f"{count} commit(s). Files changed across all of them:")
        for line in _git("diff", "--name-status", span, "--", *paths).splitlines():
            print(f"  {line}")

    if not args.all:
        total = _git("rev-list", "--count", "--no-merges", span)
        if int(total) > count:
            print()
            print(f"({int(total) - count} further commit(s) touched only code or tests. "
                  f"Re-run with --all if a spec was missed.)")
    return 0


def set_ref(args) -> int:
    sha, date, subject = _describe(args.rev)
    previous = read_ref()
    if sha == previous:
        print(f"Already recorded at {sha[:9]}. Nothing written.")
        return 0
    write_ref(sha, subject, date)
    print(f"{_REF_FILE.name}: {previous[:9]} -> {sha[:9]}  {date}  {subject}")
    print(f"{_SOURCE_URL}/commit/{sha}")
    return 0


def defer(args) -> int:
    sha, date, subject = _describe(args.rev)
    reason = " ".join(args.why).strip()
    if not reason:
        raise Problem("A reason is required: it is what the next run reads to decide again.")
    entries = read_deferred()
    if any(existing == sha for existing, _ in entries):
        entries = [(s, reason if s == sha else r) for s, r in entries]
        print(f"{sha[:9]} already deferred; reason updated.")
    else:
        entries.append((sha, reason))
        print(f"deferred {sha[:9]}  {date}  {subject}")
    write_deferred(entries)
    print(f"{_DEFERRED_FILE.name}: {len(entries)} commit(s) awaiting a decision.")
    return 0


def resolve(args) -> int:
    sha, _, subject = _describe(args.rev)
    entries = read_deferred()
    remaining = [(s, r) for s, r in entries if s != sha]
    if len(remaining) == len(entries):
        print(f"{sha[:9]} was not deferred. Nothing written.")
        return 0
    write_deferred(remaining)
    print(f"resolved {sha[:9]}  {subject}")
    print(f"{_DEFERRED_FILE.name}: {len(remaining)} commit(s) awaiting a decision.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command")

    sub.add_parser("show", help="Where the site stands against the specs.")

    p_changes = sub.add_parser("changes", help="What has landed since the recorded commit.")
    p_changes.add_argument("--all",
                           action="store_true",
                           help="Every commit, not only those touching docs.")
    p_changes.add_argument("--full",
                           action="store_true",
                           help="Include commit bodies and per-commit file lists.")

    p_set = sub.add_parser("set", help="Record a new commit as the docs baseline.")
    p_set.add_argument("rev", help="A sha, tag, or ref resolved in this checkout.")

    p_defer = sub.add_parser(
        "defer", help="Record a commit as reviewed but deliberately not documented yet.")
    p_defer.add_argument("rev", help="The commit being left for a later run.")
    p_defer.add_argument("why", nargs="+", help="Why it was left, in a few words.")

    p_resolve = sub.add_parser("resolve", help="Drop a commit from the deferred ledger.")
    p_resolve.add_argument("rev", help="The commit whose behaviour is now documented.")

    args = parser.parse_args()
    handler = {
        "show": show,
        "changes": changes,
        "set": set_ref,
        "defer": defer,
        "resolve": resolve,
    }.get(args.command or "show")

    try:
        return handler(args)
    except Problem as e:
        print(f"error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
