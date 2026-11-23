#!/usr/bin/env python3
"""Lists the open pull requests a reviewer can pick up now, waiting for GitHub to compute mergeability.

    review-ready.py [--repo OWNER/NAME] [--reviewer LOGIN] [--limit N] [--wait SECONDS]
                    [--eligible-file PATH | --enqueue]

A pull request is ready when it is mergeable, is not a draft, has every check passed or skipped,
has an approval as the latest review by the reviewer, has no latest review, by anyone, that
requests changes and is not in the merge queue. Each is printed to stdout as `<url>  <title>`, once,
after the last read, with the pull request that has waited longest first and the most recent last.
A pull request has waited since the reviewer's latest approval, or since it was opened where that
has no time.

A check counts by its latest run. GitHub keeps every run of a check on the commit, including one
cancelled because a newer run of the workflow started, and the pull request page shows only the
latest, so an earlier run does not count once a later run of the same workflow and name has started.

GitHub computes `mergeable` lazily and reports UNKNOWN for every open pull request after the base
branch moves, until each one has been read. A pull request that is not a draft, is approved and is
green but still UNKNOWN is pending: the list is read again every INTERVAL seconds for up to
`--wait` seconds in total, and a line on stderr names the pending ones. Any still UNKNOWN at the end
are named on stderr as missing from the list. A draft, an unapproved, a failing or a conflicting
pull request is never waited for. Every field is evaluated again on every read.

A listed pull request is eligible for the merge queue when it also has at least two approvals as
latest reviews and no unresolved review thread. `--eligible-file` writes the eligible ones to PATH in
the same `<url>  <title>` form, an empty file when there are none, and leaves stdout as it is.
`--enqueue` prints nothing of the list and runs `gh pr merge` for each eligible pull request, pinned
to the head commit that was read so that a push after the read is not queued unreviewed. It prints
the `<url>  <title>` of each one queued, names each one that `gh` refused on stderr and exits 1 if
any was refused. A pull request with more than 100 review threads is not eligible.

A failure of `gh` exits 1.

Standard library only, like the other tools in this directory. `review-ready-selftest.py` runs it
against a stand-in for `gh`.
"""

import argparse
import io
import json
import os
import sys
import time
# gh is run with an argument list and no shell.
import subprocess  # nosemgrep: gitlab.bandit.B404

REPO = "brave/bravebot"
REVIEWER = "netzenbot-reviewer"
LIMIT = 500
WAIT = 30.0
INTERVAL = 3.0

FIELDS = "number,title,url,createdAt,mergeable,latestReviews,isDraft,statusCheckRollup,headRefOid"
# A pull request in the merge queue needs nobody. gh has no field for queue membership, so GitHub's
# search leaves them out. The `=` form keeps gh from reading the leading `-` as a flag.
NOT_QUEUED = "--search=-is:queued"
PASSED = ("SUCCESS", "NEUTRAL", "SKIPPED")
MIN_APPROVALS = 2
THREADS_QUERY = ("query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) "
                 "{ pullRequest(number: $number) { reviewThreads(first: 100) "
                 "{ pageInfo { hasNextPage } nodes { isResolved } } } } }")

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


def approvals(pr):
    """How many people's latest review of the pull request is an approval."""
    return sum(1 for review in pr.get("latestReviews") or [] if review.get("state") == "APPROVED")


def numbers(prs):
    return ", ".join(f"#{pr['number']}" for pr in sorted(prs, key=lambda pr: pr["number"]))


def noun(count):
    return f"{count} pull request" + ("" if count == 1 else "s")


class GhFailed(Exception):
    pass


def run_gh(arguments):
    """One `gh` command, its standard output."""
    try:
        done = subprocess.run(["gh", *arguments], capture_output=True, text=True, check=False,
                              stdin=subprocess.DEVNULL)
    except OSError as error:
        raise GhFailed(f"gh could not be run: {error}") from None
    if done.returncode != 0:
        raise GhFailed(f"gh exited {done.returncode}: {done.stderr.strip()}")
    return done.stdout


def read(repo, limit):
    """One `gh pr list`, parsed."""
    output = run_gh(["pr", "list", "--repo", repo, "--limit", str(limit), "--json", FIELDS, NOT_QUEUED])
    try:
        prs = json.loads(output)
    except ValueError:
        raise GhFailed("gh printed something that is not JSON") from None
    if not isinstance(prs, list):
        raise GhFailed("gh did not print a list of pull requests")
    return prs


def unresolved_threads(repo, number):
    """How many review threads of the pull request are not resolved, or None when it has more threads
    than one read returns, so the count is not known."""
    owner, _, name = repo.partition("/")
    output = run_gh(["api", "graphql", "-f", f"query={THREADS_QUERY}", "-f", f"owner={owner}",
                     "-f", f"name={name}", "-F", f"number={number}"])
    try:
        threads = json.loads(output)["data"]["repository"]["pullRequest"]["reviewThreads"]
        nodes, more = threads["nodes"], threads["pageInfo"]["hasNextPage"]
        unresolved = sum(1 for node in nodes if node.get("isResolved") is not True)
    except (ValueError, KeyError, TypeError, AttributeError):
        raise GhFailed(f"gh did not print the review threads of #{number}") from None
    return None if more else unresolved


def queue_eligible(repo, ready):
    """The listed pull requests that have enough approvals and no unresolved review thread, in the order
    given. The threads of one with too few approvals are not read."""
    return [pr for pr in ready
            if approvals(pr) >= MIN_APPROVALS and unresolved_threads(repo, pr["number"]) == 0]


def enqueue(repo, prs, out, err):
    """Adds each pull request to the merge queue, pinned to the head commit that was read. Without a
    merge strategy, `gh pr merge` queues a pull request whose branch requires the merge queue. Returns 1
    when `gh` refused any, after trying all of them."""
    failed = 0
    for pr in prs:
        command = ["pr", "merge", str(pr["number"]), "--repo", repo, "--match-head-commit", pr["headRefOid"]]
        try:
            run_gh(command)
        except GhFailed as error:
            failed = 1
            print(f"review-ready: #{pr['number']} was not added to the merge queue: {error}", file=err, flush=True)
            continue
        print(f"{pr['url']}  {pr['title']}", file=out, flush=True)
    return failed


def write_list(path, prs):
    """Writes the `<url>  <title>` lines whole or not at all, so a reader never sees half of them."""
    temporary = f"{path}.{os.getpid()}.tmp"
    with open(temporary, "w", encoding="utf-8") as handle:
        for pr in prs:
            handle.write(f"{pr['url']}  {pr['title']}\n")
    os.replace(temporary, path)


def watch(fetch, reviewer, wait, interval, out, err, clock=time.monotonic, sleep=time.sleep):
    """Reads again while any pull request is pending until `wait` seconds have gone by, then prints each
    ready one once, the one that has waited longest first. Nothing is printed before the last read,
    since a pull request seen later can have waited longer than one seen earlier. `fetch` returns one
    list of pull requests. Returns the ready ones in the order printed."""
    deadline = clock() + wait
    ready = {}

    def emit():
        ordered = sorted(ready.values(), key=lambda pr: (waiting_since(pr, reviewer), pr["number"]))
        for pr in ordered:
            print(f"{pr['url']}  {pr['title']}", file=out, flush=True)
        return ordered

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
            return emit()
        remaining = deadline - clock()
        if remaining <= 0:
            ordered = emit()
            print(f"incomplete: {noun(len(pending))} still not computed as mergeable (UNKNOWN) after "
                  f"{wait:g} seconds, so the list above leaves them out: {numbers(pending)}",
                  file=err, flush=True)
            return ordered
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
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--eligible-file", help="write the pull requests eligible for the merge queue here")
    modes.add_argument("--enqueue", action="store_true",
                       help="add the pull requests eligible for the merge queue to it")
    args = parser.parse_args(argv)
    try:
        ready = watch(lambda: read(args.repo, args.limit), args.reviewer, args.wait, args.interval,
                      io.StringIO() if args.enqueue else sys.stdout, sys.stderr)
        if args.eligible_file:
            write_list(args.eligible_file, queue_eligible(args.repo, ready))
        elif args.enqueue:
            return enqueue(args.repo, queue_eligible(args.repo, ready), sys.stdout, sys.stderr)
    except GhFailed as error:
        print(f"review-ready: {error}", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"review-ready: could not write the eligible list: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
