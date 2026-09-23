#!/usr/bin/env python3
"""File the drafted issues, skipping the ones the tracker already holds.

    python3 agents/skills/security-audit/post-issues.py --work-dir "$WORK_DIR" [--dry-run]
        [--assignee LOGIN]

This posts. It is the one step of this skill that changes something outside the checkout, and the
three things keeping that safe are here rather than in a prompt, because a model cannot be relied on
to pace itself, to compare a title against forty existing ones, or to stop at a cap it was told
about several thousand tokens ago:

- **Nothing is posted twice.** A run that files what a previous run already filed turns the tracker
  into a record of how many times the audit was run.
- **One issue every ten seconds or so.** Six issues arriving in the same second read as a script
  having got loose, and the jitter is so the gap does not read as one either.
- **A cap.** A lane that goes wrong goes wrong at scale, and a shared tracker is the wrong place to
  find that out.

A label that does not exist is a refusal rather than a warning. `gh issue create` fails on one, and
finding that out on the fourth of six issues leaves half a report posted.
"""

import argparse
import importlib.util
import json
import random
import re
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import time
from pathlib import Path

REPO = "brave/bravebot"

# One every ten seconds, plus one to five more. Both are the ceiling on how fast this writes to
# something other people read, not a guess at an API limit.
PACE = 10.0
JITTER = (1.0, 5.0)

# A lane that malfunctions produces candidates by the dozen, and every one of them would be posted.
CAP = 12

# How much of a draft's wording an existing issue has to share, once it already names the same
# distinctive thing, before this treats it as the same issue. The second number is for the case where
# there is no distinctive thing to go on and the wording is the whole of the evidence.
OVERLAP = 0.5
OVERLAP_UNANCHORED = 0.8

STOP = {
    "the", "and", "for", "not", "that", "this", "with", "from", "which", "what", "when", "where",
    "how", "than", "then", "its", "are", "was", "has", "have", "been", "into", "onto", "out", "off",
    "any", "all", "one", "two", "new", "site", "sites", "does", "did", "can", "could", "would",
    "still", "already", "every", "each", "some", "there", "their", "they", "them", "because", "but",
    "nothing", "anything", "something",
}


def significant(title):
    """The words of a title that distinguish it from another title."""
    words = re.findall(r"[A-Za-z_][A-Za-z0-9_:]+", title.lower())
    return {word for word in words if len(word) > 2 and word not in STOP}


def gh(args, repo=None):
    """A `gh` call, as JSON where it asked for JSON.

    Every argument is its own list element and no shell is involved, so a title or a search term is
    an argument whatever it holds. That matters more here than in most scripts: a title is written by
    a lane reading code that an attacker may have chosen the wording of.

    `repo` is left out for the calls that name the repository in the path themselves, since `gh api`
    takes no `--repo`.
    """
    result = subprocess.run(
        ["gh", *args, *(["--repo", repo] if repo else [])],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError((result.stderr or result.stdout).strip())
    return result.stdout


def existing_labels(repo):
    listed = json.loads(
        gh(["label", "list", "--limit", "200", "--json", "name"], repo)
    )
    return {one["name"] for one in listed}


def assignable(repo, login):
    """Whether GitHub would accept this login as an assignee on this repository.

    Checked before the first issue goes out, for the same reason a missing label is: `gh issue
    create` fails on a login the repository would not take, and finding that out on the fourth of
    six issues leaves half a report filed.
    """
    try:
        gh(["api", f"repos/{repo}/assignees/{login}"])
    except RuntimeError:
        return False
    return True


def already_filed(repo, draft):
    """The issue this draft would duplicate, or None.

    Two tests, and both have to pass. The first is that an existing title names the same distinctive
    thing: the symbol, the clause id or the file the finding is about. The second is that enough of
    the rest of the wording matches. The first alone would call a second finding about the same
    symbol a duplicate. The second alone is worse: the findings about specs with unpinned clauses are
    one sentence with one word changed, and the first of them would swallow the rest.

    Closed issues count. Somebody who read a finding and closed it has answered it, and filing it
    again next week is arguing with them by machine.
    """
    key = (draft.get("key") or "").lower()
    wanted = significant(draft["title"])
    if not wanted:
        return None

    # A long title loses its tail to `TITLE_LIMIT`, and a key that went with it is a key no existing
    # title carries either. So the key stops being the test and the wording has to carry it alone,
    # which takes a higher bar to be worth acting on.
    anchored = bool(key) and key in draft["title"].lower()
    bar = OVERLAP if anchored else OVERLAP_UNANCHORED

    # And what is searched for has to follow the same rule. Searching titles for a key no title holds
    # returns nothing, which reads as "nothing like this is filed" and is how a duplicate gets in.
    if anchored:
        terms = re.sub(r"[^A-Za-z0-9_ .]+", " ", key).split()
    else:
        terms = sorted(wanted, key=len, reverse=True)[:3]
    query = f"{' '.join(terms)} in:title"
    try:
        found = json.loads(
            gh(
                [
                    "issue", "list", "--state", "all", "--limit", "60",
                    "--search", query,
                    "--json", "number,title,state,url",
                ],
                repo,
            )
        )
    except RuntimeError:
        found = []

    for one in found:
        if anchored and key not in one["title"].lower():
            continue
        theirs = significant(one["title"])
        if len(wanted & theirs) / len(wanted) >= bar:
            return one
    return None


def post(repo, draft, assignee=None):
    args = ["issue", "create", "--title", draft["title"], "--body-file", draft["body_file"]]
    for label in draft["labels"]:
        args += ["--label", label]
    if assignee:
        args += ["--assignee", assignee]
    return gh(args, repo).strip().splitlines()[-1]


def load_drafts(work_dir):
    here = Path(__file__).resolve().parent
    spec = importlib.util.spec_from_file_location("audit_drafts", here / "draft-issues.py")
    drafts = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(drafts)

    collector = importlib.util.spec_from_file_location("audit_collect", here / "collect-findings.py")
    collect = importlib.util.module_from_spec(collector)
    collector.loader.exec_module(collect)

    manifest = json.loads((Path(work_dir) / "manifest.json").read_text(encoding="utf-8"))
    findings = list(manifest.get("mechanical_findings", []))
    confirmed, _, _, _ = collect.load_verdicts(manifest)
    findings.extend(confirmed)
    out = Path(work_dir) / "issues"
    return drafts.draft(drafts.draftable(findings), out)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--repo", default=REPO)
    parser.add_argument("--dry-run", action="store_true", help="say what would be posted, post none")
    parser.add_argument("--max", type=int, default=CAP)
    parser.add_argument("--pace", type=float, default=PACE)
    parser.add_argument("--assignee", default=None, help="the login every issue is assigned to")
    args = parser.parse_args()

    drafts = load_drafts(args.work_dir)
    if not drafts:
        print("no drafts to post")
        return 0

    wanted = sorted({label for draft in drafts for label in draft["labels"]})
    try:
        have = existing_labels(args.repo)
    except RuntimeError as problem:
        print(f"could not read the labels of {args.repo}: {problem}", file=sys.stderr)
        return 2
    missing = [label for label in wanted if label not in have]
    if missing:
        print(f"{args.repo} has no label {', '.join(missing)}", file=sys.stderr)
        print("", file=sys.stderr)
        print("Nothing was posted. Creating a label changes what everybody sees, so it is not", file=sys.stderr)
        print("this script's to do:", file=sys.stderr)
        for label in missing:
            print(f"  gh label create {label} --repo {args.repo} --description ... --color ...", file=sys.stderr)
        return 2

    if args.assignee and not assignable(args.repo, args.assignee):
        print(f"{args.repo} would not take {args.assignee} as an assignee", file=sys.stderr)
        print("", file=sys.stderr)
        print("Nothing was posted. A login the repository refuses fails the create it is passed to,", file=sys.stderr)
        print("so it is checked here rather than partway through a report.", file=sys.stderr)
        return 2

    posted, skipped, left = [], [], []
    for draft in drafts:
        if len(posted) >= args.max:
            left.append(draft)
            continue

        try:
            duplicate = already_filed(args.repo, draft)
        except RuntimeError as problem:
            print(f"could not search {args.repo}: {problem}", file=sys.stderr)
            return 2
        if duplicate:
            skipped.append((draft, duplicate))
            print(f"  skip   {draft['title']}")
            print(f"         #{duplicate['number']} says this already ({duplicate['state'].lower()})")
            continue

        if args.dry_run:
            posted.append((draft, "(dry run)"))
            print(f"  would  {draft['title']}")
            print(f"         {', '.join(draft['labels'])}")
            if args.assignee:
                print(f"         assigned to {args.assignee}")
            continue

        if posted:
            time.sleep(args.pace + random.uniform(*JITTER))
        try:
            url = post(args.repo, draft, args.assignee)
        except RuntimeError as problem:
            print(f"could not file {draft['title']!r}: {problem}", file=sys.stderr)
            print(f"{len(posted)} posted before this; the rest were not attempted", file=sys.stderr)
            return 1
        posted.append((draft, url))
        print(f"  filed  {draft['title']}")
        print(f"         {url}")

    print("")
    verb = "would be filed" if args.dry_run else "filed"
    print(f"{len(posted)} {verb}, {len(skipped)} already on the tracker")
    if left:
        print(f"{len(left)} not attempted, at the cap of {args.max}:")
        for draft in left:
            print(f"  {draft['title']}")
        print(f"raise it with --max once the {len(posted)} above read right")
    return 0


if __name__ == "__main__":
    sys.exit(main())
