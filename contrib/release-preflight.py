#!/usr/bin/env python3
"""Warns before `make bump-version` about things a release should not go out with, and asks.

Three things are checked, none of them with a model:

- open issues labelled release-blocking, listed with their links;
- messages a translation is missing, from contrib/untranslated-messages.txt;
- whether the peer advisory check has been recorded in the last seven days, taken from the last
  commit to docs/peer-advisories-vetted.

Any of them prints a warning and asks whether to go on. Declining exits 1, which stops the bump
before it changes a file. A run with nothing to warn about prints nothing.

The commit date is a lower bound on when the advisory check last ran: a run that found nothing
new commits nothing. The date of the last run itself is not recorded anywhere yet.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
import contextlib
import io
import json
import re
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

REPO = "brave/bravebot"
BLOCKER_LABEL = "release-blocking"
ADVISORY_LEDGER = "docs/peer-advisories-vetted"
UNTRANSLATED = "contrib/untranslated-messages.txt"
MAX_ADVISORY_AGE = timedelta(days=7)

# A title is typed by whoever filed the issue and is printed to a terminal here.
CONTROL = re.compile(r"[\x00-\x1f\x7f-\x9f]")


def plain(text):
    return CONTROL.sub("", text)


def sh(*cmd):
    try:
        done = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as error:
        return 1, "", str(error)
    return done.returncode, done.stdout, done.stderr


def blockers(run):
    """Open issues labelled release-blocking, or why they could not be listed."""
    code, out, err = run("gh", "label", "list", "-R", REPO, "--limit", "500", "--json", "name")
    if code:
        return [(f"could not read the labels of {REPO}, so release blockers are unknown",
                 [plain(err.strip() or "gh failed")])]
    if BLOCKER_LABEL not in {label["name"] for label in json.loads(out)}:
        return [(f"the label {BLOCKER_LABEL} does not exist in {REPO}, so release blockers cannot be listed", [])]
    code, out, err = run("gh", "issue", "list", "-R", REPO, "-l", BLOCKER_LABEL, "-s", "open",
                         "--limit", "100", "--json", "number,title,url")
    if code:
        return [(f"could not list the open {BLOCKER_LABEL} issues of {REPO}",
                 [plain(err.strip() or "gh failed")])]
    issues = json.loads(out)
    if not issues:
        return []
    return [(f"{len(issues)} open {BLOCKER_LABEL} issue(s):",
             [f"{plain(issue['url'])}  {plain(issue['title'])}" for issue in issues])]


def untranslated(record):
    """The messages each catalog is knowingly missing, counted per locale."""
    counts = {}
    for line in record.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        locale = line.split(":", 1)[0]
        counts[locale] = counts.get(locale, 0) + 1
    if not counts:
        return []
    total = sum(counts.values())
    return [(f"{total} message(s) are not translated and will show in English:",
             [f"{locale}: {count}" for locale, count in sorted(counts.items())]
             + [f"see {UNTRANSLATED}, or run make locales"])]


def advisories(run, now):
    """Whether the peer advisory check was recorded within MAX_ADVISORY_AGE."""
    code, out, _ = run("git", "log", "-1", "--format=%H%x09%cI", "--", ADVISORY_LEDGER)
    if code or "\t" not in out:
        return [(f"no commit has recorded a peer advisory check ({ADVISORY_LEDGER} has no history here)",
                 ["run the peer-advisories skill"])]
    sha, when = out.strip().split("\t")
    age = now - datetime.fromisoformat(when)
    if age <= MAX_ADVISORY_AGE:
        return []
    return [(f"the peer advisory check was last recorded {age.days} days ago, in {sha[:12]} ({when[:10]})",
             ["run the peer-advisories skill"])]


def read_record():
    try:
        return (ROOT / UNTRANSLATED).read_text()
    except OSError:
        return None


def proceed(warnings, yes, interactive, ask):
    """Whether the bump goes on. Warnings need a yes, from the flag or from a person at a terminal."""
    if not warnings:
        return True
    for headline, details in warnings:
        print(f"warning: {headline}", file=sys.stderr)
        for detail in details:
            print(f"  {detail}", file=sys.stderr)
    if yes:
        print("continuing: YES was given", file=sys.stderr)
        return True
    if not interactive:
        print("not a terminal, so there is nobody to ask; run make bump-version with YES=1 to go on anyway",
              file=sys.stderr)
        return False
    return ask("Bump the version anyway? [y/N] ").strip().lower() in ("y", "yes")


def preflight(run, now, record):
    warnings = blockers(run)
    if record is None:
        warnings.append((f"could not read {UNTRANSLATED}, so untranslated messages are unknown", []))
    else:
        warnings += untranslated(record)
    return warnings + advisories(run, now)


def selftest():
    now = datetime(2026, 10, 9, tzinfo=timezone.utc)
    checks = []

    def fake(answers):
        def run(*cmd):
            for prefix, result in answers:
                if cmd[: len(prefix)] == prefix:
                    return result
            return 1, "", "no such command"
        return run

    labels = (("gh", "label"), (0, json.dumps([{"name": BLOCKER_LABEL}]), ""))
    issue = {"number": 7, "title": "bad\x1b[31m title", "url": "https://github.com/brave/bravebot/issues/7"}

    found = blockers(fake([labels, (("gh", "issue"), (0, json.dumps([issue]), ""))]))
    text = repr(found)
    checks.append(("an open blocker is reported with its link", issue["url"] in text, found))
    checks.append(("a control character in a title is dropped", "\\x1b" not in text, found))
    checks.append(("no open blocker is no warning",
                   blockers(fake([labels, (("gh", "issue"), (0, "[]", ""))])) == [], None))
    missing = blockers(fake([(("gh", "label"), (0, "[]", ""))]))
    checks.append(("a label that does not exist is a warning", len(missing) == 1, missing))
    failed = blockers(fake([]))
    checks.append(("a gh failure is a warning, not silence", len(failed) == 1, failed))
    failed = blockers(fake([labels, (("gh", "issue"), (1, "", "boom"))]))
    checks.append(("a failed issue listing is a warning", len(failed) == 1, failed))

    gaps = untranslated("# header\n\nfr:a\nfr:b\nde:c\n")
    checks.append(("gaps are counted per locale", gaps[0][1][:2] == ["de: 1", "fr: 2"], gaps))
    checks.append(("a record of only comments is no warning", untranslated("# header\n\n") == [], None))

    def ledger(days):
        when = (now - days).isoformat()
        return fake([(("git", "log"), (0, f"{'a' * 40}\t{when}\n", ""))])

    checks.append(("a check from seven days ago passes", advisories(ledger(timedelta(days=7)), now) == [], None))
    stale = advisories(ledger(timedelta(days=8)), now)
    checks.append(("a check from eight days ago warns with its commit", "aaaaaaaaaaaa" in repr(stale), stale))
    none = advisories(fake([(("git", "log"), (0, "", ""))]), now)
    checks.append(("a ledger with no history warns", len(none) == 1, none))

    both = preflight(fake([labels, (("gh", "issue"), (0, "[]", "")),
                           (("git", "log"), (0, f"{'a' * 40}\t{now.isoformat()}\n", ""))]),
                     now, "fr:a\n")
    checks.append(("a clean run apart from one gap gives one warning", len(both) == 1, both))
    unreadable = preflight(fake([labels, (("gh", "issue"), (0, "[]", "")),
                                 (("git", "log"), (0, f"{'a' * 40}\t{now.isoformat()}\n", ""))]),
                           now, None)
    checks.append(("an unreadable record is a warning", len(unreadable) == 1, unreadable))

    warning = [("x", [])]
    asked = []

    def ask(prompt):
        asked.append(prompt)
        return answer[0]

    with contextlib.redirect_stderr(io.StringIO()):
        answer = ["y"]
        checks.append(("no warnings goes on without asking", proceed([], False, False, ask) and not asked, asked))
        checks.append(("a yes at the prompt goes on", proceed(warning, False, True, ask), None))
        answer[0] = ""
        checks.append(("an empty answer stops", not proceed(warning, False, True, ask), None))
        answer[0] = "n"
        checks.append(("no stops", not proceed(warning, False, True, ask), None))
        asked.clear()
        checks.append(("no terminal and no YES stops without asking",
                       not proceed(warning, False, False, ask) and not asked, asked))
        checks.append(("YES goes on without asking", proceed(warning, True, False, ask) and not asked, asked))


    broke = [(claim, got) for claim, ok, got in checks if not ok]
    for claim, got in broke:
        print(f"selftest: {claim}, got {got!r}", file=sys.stderr)
    if broke:
        print(f"{len(broke)} of {len(checks)} checks failed", file=sys.stderr)
        return 1
    print(f"selftest: {len(checks)} checks passed")
    return 0


def main():
    ap = argparse.ArgumentParser(description="Warn before a version bump, and ask whether to go on.")
    ap.add_argument("--yes", action="store_true", help="go on after warning, without asking")
    ap.add_argument("--selftest", action="store_true", help="check the verdicts on made-up answers")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    warnings = preflight(sh, datetime.now(timezone.utc), read_record())
    if proceed(warnings, args.yes, sys.stdin.isatty(), input):
        return 0
    print("bump cancelled", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
