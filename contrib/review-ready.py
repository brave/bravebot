#!/usr/bin/env python3
"""Lists the open pull requests a reviewer can pick up now, waiting for GitHub to compute mergeability.

    review-ready.py [--repo OWNER/NAME] [--reviewer LOGIN] [--limit N] [--wait SECONDS]

A pull request is ready when it is mergeable, is not a draft, has every check passed or skipped,
has an approval as the latest review by the reviewer and has no latest review, by anyone, that
requests changes. Each is printed to stdout as `<url>  <title>`, once, after the last read, with the
pull request that has waited longest first and the most recent last. A pull request has waited since
the reviewer's latest approval, or since it was opened where that has no time.

A check counts by its latest run. GitHub keeps every run of a check on the commit, including one
cancelled because a newer run of the workflow started, and the pull request page shows only the
latest, so an earlier run does not count once a later run of the same workflow and name has started.

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

FIELDS = "number,title,url,createdAt,mergeable,latestReviews,isDraft,statusCheckRollup"
PASSED = ("SUCCESS", "NEUTRAL", "SKIPPED")

READY = "ready"
PENDING = "pending"


def check_result(check):
    """A check run's conclusion, or a status context's state where there is no conclusion."""
    result = check.get("conclusion")
    if result is None or result is False:
        result = check.get("state")
    return result


def check_key(check):
    """What names a check across its runs: the workflow and name of a check run, or the context of a
    commit status. None when the entry has neither, so it is never taken for a run of another."""
    if check.get("name"):
        return ("run", check.get("workflowName"), check["name"])
    if check.get("context"):
        return ("status", check["context"])
    return None


def latest_runs(checks):
    """The checks whose result counts. GitHub keeps every run of a check on the commit, such as one
    cancelled because a newer run of the workflow started, and shows only the latest. An entry is left
    out when another entry for the same check started later. An entry that has no result yet, or that
    cannot be ordered because it has no name or no start time or shares its start time, always counts."""
    newest = {}
    for check in checks:
        key, started = check_key(check), check.get("startedAt")
        if key is not None and started:
            newest[key] = max(newest.get(key, started), started)
    return [check for check in checks
            if not check_result(check) or not check.get("startedAt") or check_key(check) is None
            or check["startedAt"] == newest[check_key(check)]]


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
    if any(check_result(check) not in PASSED for check in latest_runs(pr.get("statusCheckRollup") or [])):
        return None
    return READY if mergeable == "MERGEABLE" else PENDING


def waiting_since(pr, reviewer):
    """When the pull request began waiting for a person: the time of the reviewer's approval, else the
    time it was opened, else the empty string. Both are UTC timestamps, which order as text."""
    approvals = [review.get("submittedAt") or "" for review in pr.get("latestReviews") or []
                 if (review.get("author") or {}).get("login") == reviewer and review.get("state") == "APPROVED"]
    return max(approvals, default="") or pr.get("createdAt") or ""


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
    """Reads again while any pull request is pending until `wait` seconds have gone by, then prints each
    ready one once, the one that has waited longest first. Nothing is printed before the last read,
    since a pull request seen later can have waited longer than one seen earlier. `fetch` returns one
    list of pull requests."""
    deadline = clock() + wait
    ready = {}

    def emit():
        for pr in sorted(ready.values(), key=lambda pr: (waiting_since(pr, reviewer), pr["number"])):
            print(f"{pr['url']}  {pr['title']}", file=out, flush=True)

    while True:
        pending = []
        for pr in fetch():
            if pr["number"] in ready:
                continue
            verdict = classify(pr, reviewer)
            if verdict == READY:
                ready[pr["number"]] = pr
            elif verdict == PENDING:
                pending.append(pr)
        if not pending:
            emit()
            return
        remaining = deadline - clock()
        if remaining <= 0:
            emit()
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
