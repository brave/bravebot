#!/usr/bin/env python3
"""docs/profile-reviewed: what each run measured, so a later run reports growth as well as slowness.

One line per measured key, sorted:

    <key> <value> <date> <commit> <verdict> <reason>

`value` is the number the measurement had, in seconds or bytes. `verdict` is `measured` where no
reviewer read it, and `keep` or `optimize` where one did. A later run takes `value` as the earlier
measurement of the same key and reports a candidate that grew past it, and leaves out a candidate
a reviewer already read that has not grown.
"""

import re
from pathlib import Path

LEDGER = "docs/profile-reviewed"
VERDICTS = ("measured", "keep", "optimize")

HEADER = """\
# Measurements the profile skill has taken, one per line, sorted by key:
#
#   <key> <value> <date> <commit> <verdict> <reason>
#
# <value> is seconds, or bytes for a size. A run compares what it measures with this value and
# reports a regression where the new one is higher by more than the skill's stated margin.
# measured   nobody read it; the value is the baseline for the next run
# keep       a reviewer read it and the code should stay as it is
# optimize   a reviewer read it and a draft was written, which a person posted or declined
#
# A measurement comes from one machine, so a regression is a reason to measure again before it is
# a reason to change code. In <key>, `%`, whitespace are written as `%XX`.
# The profile skill writes this file: agents/skills/profile/SKILL.md.
"""


def field(text):
    return re.sub(r"[%\s]", lambda found: "%%%02X" % ord(found.group(0)), text)


def unfield(text):
    return re.sub(r"%([0-9A-F]{2})", lambda found: chr(int(found.group(1), 16)), text)


def read(root):
    """The recorded measurements of the checkout at `root`, by key. A line that does not parse is
    reported through `problems` and skipped."""
    entries, problems = {}, []
    path = Path(root) / LEDGER
    if not path.exists():
        return entries, problems
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split(None, 5)
        try:
            ok = len(parts) >= 5 and parts[4] in VERDICTS
            value = float(parts[1]) if ok else 0.0
        except ValueError:
            ok = False
        if not ok:
            problems.append(f"{LEDGER}:{number}: not a measurement line, so it is ignored")
            continue
        entries[unfield(parts[0])] = {
            "value": value, "date": parts[2], "commit": parts[3], "verdict": parts[4],
            "reason": parts[5] if len(parts) > 5 else "",
        }
    return entries, problems


def write(root, entries):
    lines = []
    for key in sorted(entries):
        entry = entries[key]
        reason = " ".join(str(entry.get("reason") or "").split())
        lines.append(
            f"{field(key)} {entry['value']:.3f} {entry['date']} {entry['commit']} {entry['verdict']} {reason}".rstrip()
        )
    path = Path(root) / LEDGER
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(HEADER + "\n".join(lines) + ("\n" if lines else ""), encoding="utf-8")
    return path
