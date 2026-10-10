#!/usr/bin/env python3
"""Warns before `make bump-version` about things a release should not go out with, and asks.

Three things are checked, none of them with a model:

- open issues labelled release-blocking, listed with their links;
- messages a translation is missing, from contrib/untranslated-messages.txt;
- whether the peer advisory check has run in the last seven days, taken from the newest run
  recorded in a comment on issue #1901, or from the last commit to docs/peer-advisories-vetted
  where there is no such comment or gh cannot read it.

Any of them prints a warning and asks whether to go on. Declining exits 1, which stops the bump
before it changes a file. A run with nothing to warn about prints nothing.

The commit date is only a lower bound on when the advisory check last ran, because a run that finds
nothing new commits nothing. The peer-advisories skill records every run in the issue comment, so
the comment is the date used when it can be read.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
import contextlib
import io
import json
import re
import subprocess
import sys
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

REPO = "brave/bravebot"
BLOCKER_LABEL = "release-blocking"
ADVISORY_LEDGER = "docs/peer-advisories-vetted"
ADVISORY_ISSUE = 1901
# The line agents/skills/peer-advisories/peer-advisories.py writes for each run; its selftest holds
# the two to the same format.
ADVISORY_RUN = re.compile(r"peer-advisories ran (\d{4}-\d{2}-\d{2}) against ([0-9a-f]{12})")
# Only a comment from someone with write access counts as a run.
TRUSTED_AUTHORS = ("OWNER", "MEMBER", "COLLABORATOR")
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


def recorded_run(run, today):
    """The newest run recorded on ADVISORY_ISSUE as ((date, commit), None), or (None, why not)."""
    code, out, err = run("gh", "issue", "view", str(ADVISORY_ISSUE), "-R", REPO, "--json", "comments")
    if code:
        return None, f"gh could not read issue #{ADVISORY_ISSUE}: {plain(err.strip() or 'gh failed')}"
    try:
        comments = json.loads(out)["comments"]
    except (ValueError, KeyError, TypeError):
        return None, f"gh gave no comments for issue #{ADVISORY_ISSUE}"
    runs = []
    for comment in comments:
        if not isinstance(comment, dict) or comment.get("authorAssociation") not in TRUSTED_AUTHORS:
            continue
        found = ADVISORY_RUN.fullmatch(str(comment.get("body", "")).strip())
        if not found:
            continue
        try:
            when = date.fromisoformat(found.group(1))
        except ValueError:
            continue
        # The skill writes the date where it runs, which can be a day ahead of UTC; anything later
        # than that would keep the warning quiet until its date arrived.
        if when <= today + timedelta(days=1):
            runs.append((when, found.group(2)))
    if not runs:
        return None, f"issue #{ADVISORY_ISSUE} has no recorded run yet"
    return max(runs), None


def advisories(run, now):
    """Whether the peer advisory check ran within MAX_ADVISORY_AGE, and which record says so."""
    found, why_not = recorded_run(run, now.date())
    if found:
        when, commit = found
        age = now.date() - when
        if age <= MAX_ADVISORY_AGE:
            return []
        return [(f"the peer advisory check last ran {age.days} days ago ({when}, against {commit})",
                 [f"from the newest run recorded on issue #{ADVISORY_ISSUE}",
                  "run the peer-advisories skill"])]
    code, out, _ = run("git", "log", "-1", "--format=%H%x09%cI", "--", ADVISORY_LEDGER)
    if code or "\t" not in out:
        return [(f"no commit has recorded a peer advisory check ({ADVISORY_LEDGER} has no history here)",
                 [why_not, "run the peer-advisories skill"])]
    sha, when = out.strip().split("\t")
    age = now - datetime.fromisoformat(when)
    if age <= MAX_ADVISORY_AGE:
        return []
    return [(f"the peer advisory check was last recorded {age.days} days ago, in {sha[:12]} ({when[:10]})",
             [f"from the last commit to {ADVISORY_LEDGER}, which a run that finds nothing does not make; {why_not}",
              "run the peer-advisories skill"])]


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

    def runs(*comments):
        return (("gh", "issue", "view"), (0, json.dumps({"comments": [
            {"authorAssociation": assoc, "body": f"peer-advisories ran {(now - days).date()} against {'b' * 12}"
             if isinstance(days, timedelta) else days}
            for assoc, days in comments]}), ""))

    old_ledger = (("git", "log"), (0, f"{'a' * 40}\t{(now - timedelta(days=30)).isoformat()}\n", ""))
    fresh = advisories(fake([runs(("MEMBER", timedelta(days=1))), old_ledger]), now)
    checks.append(("a run a day ago passes though the ledger was last committed a month ago", fresh == [], fresh))
    edge = advisories(fake([runs(("MEMBER", timedelta(days=7))), old_ledger]), now)
    checks.append(("a run seven days ago passes", edge == [], edge))
    late = advisories(fake([runs(("COLLABORATOR", timedelta(days=8))), old_ledger]), now)
    checks.append(("a run eight days ago warns, naming the comment, its date and its commit",
                   len(late) == 1 and "8 days ago" in repr(late) and str((now - timedelta(days=8)).date()) in repr(late)
                   and "b" * 12 in repr(late) and f"issue #{ADVISORY_ISSUE}" in repr(late) and "aaaaaaaaaaaa" not in repr(late),
                   late))
    new_ledger = (("git", "log"), (0, f"{'a' * 40}\t{now.isoformat()}\n", ""))
    over = advisories(fake([runs(("OWNER", timedelta(days=9))), new_ledger]), now)
    checks.append(("the comment is used in place of a newer ledger commit", len(over) == 1 and "9 days ago" in repr(over), over))
    newest = advisories(fake([runs(("MEMBER", timedelta(days=20)), ("MEMBER", timedelta(days=2)),
                                   ("MEMBER", timedelta(days=12))), old_ledger]), now)
    checks.append(("the newest of several runs is the one used", newest == [], newest))
    forged = advisories(fake([runs(("NONE", timedelta(days=0)), ("CONTRIBUTOR", timedelta(days=0))), old_ledger]), now)
    checks.append(("a run recorded by someone without write access is ignored, so the ledger date is used",
                   len(forged) == 1 and "30 days ago" in repr(forged) and "aaaaaaaaaaaa" in repr(forged), forged))
    other = advisories(fake([runs(("MEMBER", f"peer-advisories ran {now.date()} against {'b' * 12}\nand more"),
                                  ("MEMBER", f"peer-advisories ran 2026-13-45 against {'b' * 12}"),
                                  ("MEMBER", "thanks, this is done")), old_ledger]), now)
    checks.append(("a comment that is not exactly one run line is ignored",
                   len(other) == 1 and "30 days ago" in repr(other) and "no recorded run yet" in repr(other), other))
    ahead = advisories(fake([runs(("MEMBER", -timedelta(days=30))), old_ledger]), now)
    checks.append(("a run dated a month ahead is ignored, so the ledger date is used",
                   len(ahead) == 1 and "30 days ago" in repr(ahead), ahead))
    tomorrow = advisories(fake([runs(("MEMBER", -timedelta(days=1))), old_ledger]), now)
    checks.append(("a run dated tomorrow, from a clock ahead of UTC, counts", tomorrow == [], tomorrow))
    nothing = advisories(fake([runs(), old_ledger]), now)
    checks.append(("no comment yet falls back to the ledger and says which source it used",
                   len(nothing) == 1 and "last commit to docs/peer-advisories-vetted" in repr(nothing)
                   and "no recorded run yet" in repr(nothing), nothing))
    unread = advisories(fake([(("gh", "issue", "view"), (1, "", "boom")), old_ledger]), now)
    checks.append(("a gh failure falls back to the ledger and says why",
                   len(unread) == 1 and "last commit to docs/peer-advisories-vetted" in repr(unread)
                   and "boom" in repr(unread), unread))
    garbled = advisories(fake([(("gh", "issue", "view"), (0, "not json", "")), old_ledger]), now)
    checks.append(("an unreadable reply falls back to the ledger", len(garbled) == 1 and "30 days ago" in repr(garbled), garbled))
    fresh_ledger = advisories(fake([runs(), new_ledger]), now)
    checks.append(("with no comment a fresh ledger commit still passes", fresh_ledger == [], fresh_ledger))

    both = preflight(fake([labels, (("gh", "issue", "list"), (0, "[]", "")),
                           (("git", "log"), (0, f"{'a' * 40}\t{now.isoformat()}\n", ""))]),
                     now, "fr:a\n")
    checks.append(("a clean run apart from one gap gives one warning", len(both) == 1, both))
    unreadable = preflight(fake([labels, (("gh", "issue", "list"), (0, "[]", "")),
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
