"""The record of what a check-spec run reviewed, so that a later run reads only what changed.

docs/check-spec-reviewed holds one line per clause: the verdict a reviewer gave it, the commit that
was reviewed, and a hash of what the verdict was read from besides the code. check-spec.py reads it
to decide how much of a clause to review again, collect-findings.py reads it to carry a verdict
into the report, and record-reviewed.py writes it. Standard library only.
"""

import datetime
import hashlib
import json
import os
import re
from pathlib import Path

from specs import EM_DASH

LEDGER = Path("docs/check-spec-reviewed")

# `unclear` is not here. It says the review decided nothing, so there is no verdict to keep and the
# clause is reviewed again from the start.
VERDICTS = ("conforms", "violation", "untested")
SEVERITIES = ("error", "warning", "-")
# What a verdict is when the reviewer gave it no severity the record can hold.
SEVERITY = {"conforms": "-", "violation": "error", "untested": "warning"}

CLAUSE = re.compile(r"[A-Z]+-\d+")
COMMIT = re.compile(r"[0-9a-f]{12,40}")
DIGEST = re.compile(r"[0-9a-f]{16}")
SUMMARY_LIMIT = 240

HEADER = """\
# Spec clauses a check-spec run has reviewed, one per line, sorted by clause id:
#
#   <clause> <verdict> <severity> <commit> <hash> <date> <summary>
#
# conforms   the code does what the clause says, and severity is -
# violation  it does not, and severity is error or warning
# untested   the code is right and no test the clause names pins it, and severity is error or warning
#
# <commit> is the main commit the verdict was reached at. <hash> is taken over the clause, the paths
# its spec governs, the symbols it guards and every use of them, the spec's text outside its clauses
# and the reviewer's instructions, so a change to any of them reviews the clause again. A clause the
# review could not decide has no line. The check-spec skill writes this file:
# agents/skills/check-spec/SKILL.md.
"""


class Problem(Exception):
    pass


def digest(spec, clause, state=""):
    """What a verdict on `clause` was read from, other than the code a diff shows.

    The clause is what is judged. The governed paths say which code it is judged against, and a
    path added to them was never read for this clause, whatever the diff since the last review
    shows. The rest of the spec is what the reviewer is given for context, and it is where a spec
    lists the exceptions it admits, so a verdict reached before one was added no longer stands.
    `state` is the rest of what the reviewer is given: the guarded symbols and every place each is
    used, which no diff of the watched paths shows where the use is in a file nobody watches, and
    the instructions the review was run under."""
    text = "\0".join(
        [clause.canonical, json.dumps(sorted(spec.governs)), spec.frame_text, state]
    )
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:16]


def severity_of(verdict, given):
    """The severity to record: the reviewer's where the record can hold it, else the verdict's own."""
    return "-" if verdict == "conforms" else given if given in ("error", "warning") else SEVERITY[verdict]


def one_line(text):
    text = re.sub(r"\s+", " ", str(text or "")).strip()
    text = text.replace(f" {EM_DASH} ", ", ").replace(EM_DASH, ", ")
    if len(text) <= SUMMARY_LIMIT:
        return text
    return text[: SUMMARY_LIMIT - 3].rsplit(" ", 1)[0] + "..."


def order(clause):
    prefix, number = clause.rsplit("-", 1)
    return prefix, int(number)


def read_ledger(path):
    """Clause id to its entry, or nothing where there is no file. A line that is not one of the
    shapes above is refused with its number, since reading it as a missing record would review
    that clause in full and say nothing."""
    entries = {}
    path = Path(path)
    if not path.exists():
        return entries
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as problem:
        raise Problem(f"{path}: cannot be read: {problem}") from problem
    for number, line in enumerate(lines, 1):
        if not line.strip() or line.startswith("#"):
            continue
        where = f"{path}:{number}"
        parts = line.split(maxsplit=6)
        if len(parts) < 6:
            raise Problem(f"{where}: not a record line: {line!r}")
        clause, verdict, severity, commit, hashed, date = parts[:6]
        if not CLAUSE.fullmatch(clause):
            raise Problem(f"{where}: {clause!r} is not a clause id")
        if clause in entries:
            raise Problem(f"{where}: {clause} is recorded twice")
        if verdict not in VERDICTS:
            raise Problem(f"{where}: {verdict!r} is not one of {', '.join(VERDICTS)}")
        if severity not in SEVERITIES or (verdict == "conforms") != (severity == "-"):
            raise Problem(f"{where}: severity {severity!r} does not go with {verdict}")
        if not COMMIT.fullmatch(commit):
            raise Problem(f"{where}: {commit!r} is not a commit")
        if not DIGEST.fullmatch(hashed):
            raise Problem(f"{where}: {hashed!r} is not a hash")
        try:
            datetime.date.fromisoformat(date)
        except ValueError as problem:
            raise Problem(f"{where}: {date!r} is not a date") from problem
        entries[clause] = {
            "verdict": verdict,
            "severity": severity,
            "commit": commit,
            "hash": hashed,
            "date": date,
            "summary": parts[6] if len(parts) > 6 else "",
        }
    return entries


def write_ledger(path, entries):
    lines = [
        f"{clause} {e['verdict']} {e['severity']} {e['commit']} {e['hash']} {e['date']} "
        f"{one_line(e['summary'])}".rstrip()
        for clause, e in sorted(entries.items(), key=lambda item: order(item[0]))
    ]
    # Whole or not at all: a file cut short by a kill or a full disk is one the next run refuses.
    path = Path(path)
    scratch = path.with_name(f"{path.name}.new")
    scratch.write_text(HEADER + "".join(f"{line}\n" for line in lines), encoding="utf-8")
    os.replace(scratch, path)
