#!/usr/bin/env python3
"""Lists the open pull requests a reviewer can pick up now, waiting for GitHub to compute mergeability.

    review-ready.py [--repo OWNER/NAME] [--reviewer LOGIN] [--limit N] [--wait SECONDS]

A pull request is ready when it is mergeable, is not a draft, has every check passed or skipped,
has an approval as the latest review by the reviewer and has no latest review, by anyone, that
requests changes. Each is printed to stdout as `<url>  <title>`, once, as soon as it is seen.

GitHub computes `mergeable` lazily and reports UNKNOWN for every open pull request after the base
branch moves, until each one has been read. A pull request that is not a draft, is approved and is
green but still UNKNOWN is pending: the list is read again every INTERVAL seconds for up to
`--wait` seconds in total, and a line on stderr names the pending ones. Any still UNKNOWN at the end
are named on stderr as missing from the list. A draft, an unapproved, a failing or a conflicting
pull request is never waited for. Every field is evaluated again on every read.

A failure of `gh` exits 1.

Standard library only, like the other tools in this directory. `review-ready-selftest.py` runs it
against a stand-in for `gh`.
"""

import argparse
import json
import sys
import time
# gh is run with an argument list and no shell.
import subprocess  # nosemgrep: gitlab.bandit.B404

REPO = "brave/bravebot"
REVIEWER = "netzenbot-reviewer"
LIMIT = 500
WAIT = 30.0
INTERVAL = 3.0

FIELDS = "number,title,url,mergeable,latestReviews,isDraft,statusCheckRollup"
PASSED = ("SUCCESS", "NEUTRAL", "SKIPPED")

READY = "ready"
PENDING = "pending"


def check_result(check):
    """A check run's conclusion, or a status context's state where there is no conclusion."""
    result = check.get("conclusion")
    if result is None or result is False:
        result = check.get("state")
    return result


def classify(pr, reviewer):
    """READY when mergeable, PENDING when GitHub has not said, None when the pull request is not one
    the reviewer can pick up whatever mergeable turns out to be, or conflicts."""
    mergeable = pr.get("mergeable")
    if mergeable not in ("MERGEABLE", "UNKNOWN"):
        return None
    if pr.get("isDraft") is not False:
        return None
    reviews = pr.get("latestReviews") or []
    approved = any((review.get("author") or {}).get("login") == reviewer and review.get("state") == "APPROVED"
                   for review in reviews)
    if not approved or any(review.get("state") == "CHANGES_REQUESTED" for review in reviews):
        return None
    if any(check_result(check) not in PASSED for check in pr.get("statusCheckRollup") or []):
        return None
    return READY if mergeable == "MERGEABLE" else PENDING


def numbers(prs):
    return ", ".join(f"#{pr['number']}" for pr in sorted(prs, key=lambda pr: pr["number"]))


def noun(count):
    return f"{count} pull request" + ("" if count == 1 else "s")


class GhFailed(Exception):
    pass


def read(repo, limit):
    """One `gh pr list`, parsed."""
    command = ["gh", "pr", "list", "--repo", repo, "--limit", str(limit), "--json", FIELDS]
    try:
        done = subprocess.run(command, capture_output=True, text=True, check=False)
    except OSError as error:
        raise GhFailed(f"gh could not be run: {error}") from None
    if done.returncode != 0:
        raise GhFailed(f"gh exited {done.returncode}: {done.stderr.strip()}")
    try:
        prs = json.loads(done.stdout)
    except ValueError:
        raise GhFailed("gh printed something that is not JSON") from None
    if not isinstance(prs, list):
        raise GhFailed("gh did not print a list of pull requests")
    return prs


def watch(fetch, reviewer, wait, interval, out, err, clock=time.monotonic, sleep=time.sleep):
    """Prints each ready pull request once, and reads again while any are pending until `wait` seconds
    have gone by. `fetch` returns one list of pull requests."""
    deadline = clock() + wait
    printed = set()
    while True:
        pending = []
        for pr in fetch():
            if pr["number"] in printed:
                continue
            verdict = classify(pr, reviewer)
            if verdict == READY:
                printed.add(pr["number"])
                print(f"{pr['url']}  {pr['title']}", file=out, flush=True)
            elif verdict == PENDING:
                pending.append(pr)
        if not pending:
            return
        remaining = deadline - clock()
        if remaining <= 0:
            print(f"incomplete: {noun(len(pending))} still not computed as mergeable (UNKNOWN) after "
                  f"{wait:g} seconds, so the list above leaves them out: {numbers(pending)}",
                  file=err, flush=True)
            return
        print(f"waiting for {noun(len(pending))} GitHub has not computed as mergeable (UNKNOWN): "
              f"{numbers(pending)}", file=err, flush=True)
        sleep(min(interval, remaining))


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--repo", default=REPO)
    parser.add_argument("--reviewer", default=REVIEWER)
    parser.add_argument("--limit", type=int, default=LIMIT)
    parser.add_argument("--wait", type=float, default=WAIT, help="seconds to wait in total for UNKNOWN")
    parser.add_argument("--interval", type=float, default=INTERVAL, help="seconds between reads")
    args = parser.parse_args(argv)
    try:
        watch(lambda: read(args.repo, args.limit), args.reviewer, args.wait, args.interval,
              sys.stdout, sys.stderr)
    except GhFailed as error:
        print(f"review-ready: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
