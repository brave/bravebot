#!/usr/bin/env python3
"""One issue body per finding, written to files for a person to read before anything is posted.

A run reports what it found and then it is over, so a finding nobody acts on that week is a
finding nobody acts on. An issue outlives the run. What has stopped that being worth doing is
the shape of the bodies: a clause id and a paragraph of review prose, handed to somebody who has
not read the clause and cannot see what went wrong, is a bug report they have to redo before they
can start. So a body here is shaped like a pull request instead. What is wrong, shown; then how to
reach it; then every detail the review already produced, which is where the fixer starts.

Nothing here posts anything. It writes files and prints a table, because which of thirty findings
deserves an issue is a person's call, and a run that files them itself takes that call away.

    python3 agents/skills/check-spec/draft-issues.py --work-dir "$WORK_DIR"

A finding `make check-spec` already fails on gets no draft. Those are red in CI on the branch that
caused them and are fixed there, so an issue for one is closed before anybody reads it. What a
green CI run still leaves unsaid is the review pass and the clauses nothing pins, and that is what
this drafts.

Where a reviewer said the finding is something a person can look at, the draft is marked as wanting
a screen and names the file to put one in. Filling that file in is a separate step, since capturing
a screen means running the interface: `contrib/drive_tui.py --raw` to capture and
`contrib/terminal-screenshot.py` to replay. A draft with a screen in it is a bug report somebody can
act on without reproducing it first.
"""

import argparse
import importlib.util
import json
import os
import re
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from specs import load_specs  # noqa: E402

_collector = importlib.util.spec_from_file_location(
    "collect_findings", Path(__file__).resolve().parent / "collect-findings.py"
)
collect = importlib.util.module_from_spec(_collector)
_collector.loader.exec_module(collect)

# Findings about the run rather than about the tree. A missing or unreadable reviewer result says to
# rerun the chunk, and `unclear` says the review could not tell from the governed files. Neither is
# something to hand somebody, since neither names anything they could go and fix.
NOT_A_BUG = {"review-incomplete", "review-unreadable", "unclear"}

# The kind labels, which say what the issue is. The three axes that say what to do about it are the
# [triage-issues skill](../triage-issues/SKILL.md)'s job, and a run that guessed at them would be
# claiming a place in somebody's queue for a finding nobody has read.
#
# Two of these go on a divergence, because a clause and today's behaviour are different questions. A
# person searching `is:open label:bug` is asking what is broken, and a clause whose behaviour was
# never built breaks nothing: it is missing.
MISMATCH, COVERAGE = "spec-mismatch", "spec-coverage"
BROKEN, MISSING = "bug", "enhancement"

# What a reader is being told they are affected by. Composed from the finding rather than written
# per issue, because the alternative is a sentence about impact that nobody measured.
IMPACT = {
    "violation": "a guarantee the spec makes does not hold in the code that ships.",
    "untested": (
        "none today. The behaviour looks right and no test pins it, so the next change through "
        "this code can break it and stay green."
    ),
}
UNPINNED_IMPACT = (
    "none today. Nothing pins this clause, so a change that breaks it passes every check."
)

HOW_IT_WAS_FOUND = (
    "Found by the `check-spec` skill reading {spec} against the code it governs, at severity "
    "`{severity}` ({kind}). Drafted by the check rather than written by a person, so nothing here "
    "is triaged: the clause is the authority, and a finding that disagrees with it is wrong. "
    "`make check-spec` runs the mechanical half of the same check in seconds."
)

# Enough of a clause to decide with, and not so much that the body becomes the spec. A clause longer
# than this says where the rest of it is instead.
QUOTE_LINES = 14
CONTEXT_BEFORE, CONTEXT_AFTER = 2, 5
# GitHub's own cap on an issue title. Nothing here shortens a title to reach it. A reviewer's
# summary states the defect first and its cost second, so cutting the end takes the cost off and
# leaves a sentence that stops in the middle: that is how #509 and #498 were filed, from a run where
# all 60 summaries ran past the 110 characters this used to cut at. A draft over the cap is reported
# as wanting a shorter title instead, and the person posting it shortens it keeping both halves.
TITLE_LIMIT = 256
ANCHOR = re.compile(r'<a id="[^"]*"></a>')
# `crates/core/src/policy.rs:40 the split is made here`: the place, then what is there.
SITE = re.compile(r"([\w./\\-]+\.\w+):(\d+)\s*(.*)")


_LOADED = []
_TRACKED = {}


def _specs():
    """Every spec, parsed once. A run drafts dozens of findings over the same few files."""
    if not _LOADED:
        _LOADED.extend(load_specs())
    return _LOADED


def fence(text):
    """A code fence long enough to hold text that contains one.

    A screen replayed out of the interface can hold anything the model wrote into the transcript,
    backticks included, and a fence that closes early spills the rest of the screen into the page
    as prose.
    """
    longest = max((len(run) for run in re.findall(r"`+", text)), default=0)
    return "`" * max(3, longest + 1)


def evidence_items(value):
    """The evidence as separate places, however the finding carried it."""
    if not value:
        return []
    if isinstance(value, list):
        return [str(item).strip() for item in value if str(item).strip()]
    return [part.strip() for part in str(value).split("; ") if part.strip()]


def tracked(root):
    """Every path git tracks under `root`, as written rather than as followed.

    Read once per run and held, since a run drafts dozens of findings against the one tree. A tree
    git cannot answer for yields nothing, so a drafter run outside a checkout quotes no code rather
    than quoting whatever it was pointed at.

    Names, not destinations: resolving here would put the target of every tracked symlink into the
    set, and a link committed as `crates/demo/src/linked.rs` would then admit the file it points at
    under its own name too.
    """
    if root not in _TRACKED:
        listed = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z"],
            capture_output=True,
            text=True,
            check=False,
        )
        names = listed.stdout.split("\0") if listed.returncode == 0 else []
        _TRACKED[root] = frozenset(root / name for name in names if name)
    return _TRACKED[root]


def first_site(items):
    """The first evidence item naming a line of a file that is already public.

    A finding about a clause's coverage points at the clause, and quoting the spec back as the code
    it happens in is both wrong and already above.

    A place is written by a model reading a tree somebody else wrote, so it is bytes an attacker may
    have chosen, and the caller inlines whatever this returns into a body that gets posted to a
    public tracker. What may be published is therefore what is published already, which is what the
    tree tracks and not what happens to sit in it: an absolute
    `/Users/somebody/.config/gh/hosts.yml:2` and a `../`-climbing path both satisfy the pattern as
    readily as a source file does, and so do the credentials
    [credential-protection.md](../../../docs/specs/credential-protection.md) says a checkout holds,
    `./.envrc:1` and `.bravebot/settings.local.json:1`, which git ignores and a body must not carry.

    The name has to be tracked and so has what it points at, because either alone still opens an
    ignored file: a tracked symlink names a path the tree publishes and reads one it does not.
    """
    root = Path.cwd().resolve()
    public = tracked(root)
    for item in items:
        match = SITE.match(item)
        if not match:
            continue
        path, line = Path(match.group(1)), int(match.group(2))
        named = Path(os.path.normpath(root / path))
        if named not in public or (root / path).resolve() not in public:
            continue
        if path.is_file() and path.suffix != ".md":
            return path, line
    return None, None


def as_bullet(item):
    """One evidence item, with the place in a code span and what is there outside it."""
    match = SITE.match(item)
    if not match:
        return f"- {item}"
    place = f"- `{match.group(1)}:{match.group(2)}`"
    return f"{place} {match.group(3)}".rstrip()


def sentence(text):
    """Ended, since these arrive as a phrase as often as as a sentence."""
    text = text.strip()
    return text if not text or text[-1] in ".!?:" else f"{text}."


def code_at(path, line):
    """The lines around `line`, numbered, so the body says where it is and shows it."""
    try:
        source = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return []
    first = max(1, line - CONTEXT_BEFORE)
    last = min(len(source), line + CONTEXT_AFTER)
    if first > len(source):
        return []
    width = len(str(last))
    body = [f"{str(n).rjust(width)} | {source[n - 1]}" for n in range(first, last + 1)]
    wall = fence("\n".join(body))
    return [f"`{path}:{line}`", "", wall + "text", *body, wall, ""]


def clause_quote(spec, clause_id):
    """The clause itself, quoted, and where to read the rest of it.

    Whoever reads this has not read the spec, and a finding stated as a breach of `PROC-6` with no
    `PROC-6` in it asks them to go and find out what was even claimed. Cited as a path and an
    anchor rather than as a link, because a body written here has no idea which repository it is
    about to be posted to and a link that guesses is a link that goes nowhere.
    """
    for one in _specs():
        if one.rel != spec and one.name != Path(spec).name:
            continue
        where = f"`{one.rel}#{clause_id}`"
        for clause in one.clauses:
            if clause.id != clause_id:
                continue
            # The tests it names are in the report already, and the anchor at the end of the body
            # belongs to the clause after this one.
            kept = [
                line
                for line in clause.text.splitlines()
                if not line.strip().startswith("`verified-by:")
                and not ANCHOR.match(line.strip())
            ]
            while kept and not kept[-1].strip():
                kept.pop()
            while kept and not kept[0].strip():
                kept.pop(0)
            cut = len(kept) > QUOTE_LINES
            quoted = [f"> ### {clause.id}: {clause.title}"]
            if kept:
                quoted.append(">")
            quoted += [f"> {line}".rstrip() for line in kept[:QUOTE_LINES]]
            if cut:
                quoted.append(">")
                quoted.append(f"> (the clause goes on; the rest of it is in `{one.rel}`)")
            return where, quoted, list(one.governs)
        return where, [], list(one.governs)
    return f"`{spec}`", [], []


def title_for(finding):
    """Clause id first, because that is the thing an issue, a commit and a test all point at.

    No backticks: a title is not rendered as markdown anywhere it is read, and searching for one
    that has them means guessing where they were.

    Whole, however long the summary runs. `TITLE_LIMIT` says which of these a person has to shorten
    before posting it; nothing here does that for them.
    """
    lead = finding.get("clause") or Path(finding["spec"]).stem
    summary = (finding.get("summary") or "the code does not match the spec").rstrip(".")
    title = summary if summary.replace("`", "").startswith(lead) else f"{lead}: {summary}"
    return title.replace("`", "")


def slug_for(finding, taken):
    """A file name for this finding and no other.

    One clause can carry two findings, a review verdict and the mechanical one about its coverage,
    and sharing a file between them means the second silently replaces the first. The name has to
    hold still between runs as well, because a screen captured against one run is read back by the
    next.
    """
    lead = finding.get("clause") or Path(finding["spec"]).stem
    base = re.sub(r"[^A-Za-z0-9._-]+", "-", f"{lead}-{finding.get('kind', 'finding')}").strip("-")
    slug, extra = base, 1
    while slug in taken:
        extra += 1
        slug = f"{base}-{extra}"
    taken.add(slug)
    return slug


def labels_for(finding):
    """Which clause the finding is about, and what it is against the code that ships today.

    A clause nothing pins gets the one label: the behaviour is right, so nothing is broken, and
    nothing is missing either.
    """
    if finding.get("kind") != "violation":
        return [COVERAGE]
    return [MISMATCH, MISSING if finding.get("absent") else BROKEN]


def impact_for(finding):
    if finding.get("source") == "review":
        return IMPACT.get(finding.get("kind"), IMPACT["violation"])
    return UNPINNED_IMPACT


def body_for(finding, screen=None, session=None):
    """The issue, shaped like a pull request: what is wrong shown first, then everything else."""
    clause_id = finding.get("clause") or ""
    where, quoted, governs = clause_quote(finding["spec"], clause_id)
    items = evidence_items(finding.get("evidence"))
    path, line = first_site(items)

    lines = [f"User impact: {impact_for(finding)}", "", "## The problem", ""]
    lines += [sentence(finding.get("summary") or "the code does not match the spec"), ""]
    if quoted:
        lines += [f"{where} says:", "", *quoted, ""]
    else:
        lines += [f"The clause is {where}.", ""]

    if screen:
        wall = fence(screen)
        lines += [
            "This is the screen it draws:",
            "",
            wall + "text",
            screen.rstrip("\n"),
            wall,
            "",
        ]
    elif path is not None:
        lines += ["The code it happens in:", "", *code_at(path, line)]

    lines += ["## Reproduce", ""]
    if session:
        lines += [
            "A scripted session that reaches it, one step per line as `timeout keys`:",
            "",
            "```text",
            session.rstrip("\n"),
            "```",
            "",
            "```sh",
            "contrib/drive_tui.py session.txt --raw capture.txt -- target/debug/bravebot",
            "contrib/terminal-screenshot.py capture.txt --strict",
            "```",
            "",
        ]
    if finding.get("failure"):
        lines += [sentence(finding["failure"]), ""]
    elif not session:
        lines += [
            (
                "Not reduced to a session. The review found this by reading the governed code "
                "rather than by running it."
            )
            if finding.get("source") == "review"
            else "Nothing to run. `make check-spec` reports this from the spec tree itself.",
            "",
        ]
    if items:
        lines += ["Where it is:", ""]
        lines += [as_bullet(item) for item in items]
        lines.append("")
    if governs:
        lines += [
            "This spec governs " + ", ".join(f"`{one}`" for one in governs) + ".",
            "",
        ]

    lines += ["## The fix", ""]
    lines += [
        sentence(finding.get("fix") or "")
        or "Not proposed. The clause says what has to hold; how is the fixer's decision.",
        "",
    ]

    lines += [
        "## How this was found",
        "",
        HOW_IT_WAS_FOUND.format(
            spec=f"`{finding['spec']}`",
            severity=finding.get("severity", "error"),
            kind=f"`{finding.get('kind', 'violation')}`",
        ),
        "",
    ]
    return "\n".join(lines)


def draftable(findings, errors_only):
    """The findings a green CI run leaves unsaid, which are the ones worth an issue."""
    keep = []
    for finding in findings:
        if finding.get("kind") in NOT_A_BUG:
            continue
        if finding.get("source") != "review" and finding.get("severity") == collect.ERROR:
            continue
        if errors_only and finding.get("severity") != collect.ERROR:
            continue
        keep.append(finding)
    return keep


def draft(findings, out):
    """One file per finding, plus the screen each one is still waiting for."""
    out.mkdir(parents=True, exist_ok=True)
    drafts, taken = [], set()
    for finding in findings:
        slug = slug_for(finding, taken)
        screen_file = out / f"{slug}.screen.txt"
        session_file = out / f"{slug}.session.txt"
        screen = screen_file.read_text(encoding="utf-8") if screen_file.is_file() else None
        session = session_file.read_text(encoding="utf-8") if session_file.is_file() else None
        body_file = out / f"{slug}.md"
        body_file.write_text(body_for(finding, screen, session), encoding="utf-8")
        title = title_for(finding)
        drafts.append(
            {
                "title": title,
                "title_too_long": len(title) > TITLE_LIMIT,
                "slug": slug,
                "body_file": str(body_file),
                "spec": finding["spec"],
                "clause": finding.get("clause"),
                "severity": finding.get("severity"),
                "kind": finding.get("kind"),
                "labels": labels_for(finding),
                "screen_wanted": finding.get("screen"),
                "screen_file": str(screen_file),
                "session_file": str(session_file),
                "capture_file": str(out / f"{slug}.capture.txt"),
                "has_screen": screen is not None,
            }
        )
    return drafts


def report(drafts, out):
    lines = []
    for entry in drafts:
        mark = "error" if entry["severity"] == collect.ERROR else "warn "
        lines.append(f"  {mark}  {entry['title']}")
        lines.append(f"         {' '.join(entry['labels'])}, body {entry['body_file']}")
        if entry["title_too_long"]:
            lines.append(
                f"         needs a shorter title: {len(entry['title'])} characters, "
                f"and GitHub takes {TITLE_LIMIT}. Keep the defect and the cost"
            )
        if entry["screen_wanted"] and not entry["has_screen"]:
            lines.append(f"         wants a screen: {entry['screen_wanted']}")
            lines.append(
                f"         script {entry['session_file']}, "
                f"capture {entry['capture_file']}, screen {entry['screen_file']}"
            )
    lines.append("")
    counted = f"{len(drafts)} draft" + ("" if len(drafts) == 1 else "s")
    lines.append(f"{counted} in {out}. Nothing has been posted.")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--out", default=None, help="default: WORK_DIR/issues")
    parser.add_argument("--errors", action="store_true", help="skip the warnings")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    manifest_path = Path(args.work_dir) / "manifest.json"
    if not manifest_path.exists():
        print(f"no manifest at {manifest_path}", file=sys.stderr)
        return 2
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

    findings = list(manifest.get("mechanical_findings", []))
    reviews, _ = collect.load_reviews(manifest)
    findings.extend(reviews)

    out = Path(args.out) if args.out else Path(args.work_dir) / "issues"
    drafts = draft(draftable(findings, args.errors), out)

    if args.json:
        print(json.dumps({"drafts": drafts, "out": str(out)}, indent=2))
    else:
        print(report(drafts, out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
