#!/usr/bin/env python3
"""One issue body per confirmed finding.

A run reports what it found and then it is over, so a finding nobody acts on that week is a finding
nobody acts on. An issue outlives the run. The bodies are shaped the way this tracker's own security
issues are shaped: what happens, shown; then the walk; then what it buys an attacker; then the
argument for the label it carries; then a plan naming files, the clause and the test.

    python3 agents/skills/security-audit/draft-issues.py --work-dir "$WORK_DIR"

This writes files. `post-issues.py` is what files them, and it reads what this wrote, so a body can
be edited between the two steps and the edit is what gets posted.

The text helpers are `check-spec`'s. Two skills writing issue bodies against the same specs should
quote a clause and excerpt a line the same way, and a second copy would be a second thing to fix.
"""

import argparse
import importlib.util
import json
import re
import sys
from pathlib import Path

_here = Path(__file__).resolve().parent
_drafts = importlib.util.spec_from_file_location(
    "check_spec_drafts", _here.parent / "check-spec" / "draft-issues.py"
)
shared = importlib.util.module_from_spec(_drafts)
_drafts.loader.exec_module(shared)

TITLE_LIMIT = shared.TITLE_LIMIT

# Words that only make sense with what comes next, so a title ending on one has been cut rather than
# shortened.
DANGLING = re.compile(
    r"(?:\s+(?:a|an|the|that|to|for|of|and|or|in|on|at|with|as|is|it|so|which|into|from|by))+$",
    re.IGNORECASE,
)

# Every issue this skill files is about the guarantee, so `security` is on all of them. The kind label
# says which kind of defect it is, and the two are not alternatives.
SECURITY = "security"
NEEDS_REVIEW = "needs-security-review"

KIND_LABEL = {
    "violation": "bug",
    "laundering": "bug",
    "reachable": "bug",
    "clause-permits": "spec-bug",
    "unpinned": "spec-coverage",
    "labelled-impl": "bug",
    "unpinned-action": "bug",
    "exception-count-disagreement": "documentation",
    "exception-count-unstated": "documentation",
    "construction-unpinned": "spec-coverage",
    "unpinned-guarantee": "spec-coverage",
}

# What each kind is, and what it is not, so the body can argue its own label the way this tracker's
# security issues do rather than leaving a reader to wonder why it is not the other one.
WHY_THIS_LABEL = {
    "bug": (
        "Filed as `bug` because a clause already forbids this and the code does it anyway. Where a "
        "reviewer reads the clauses differently and finds nothing broken, `spec-bug` is the relabel "
        "and the clause is what has to change."
    ),
    "spec-bug": (
        "Filed as `spec-bug` rather than `bug` because no clause is violated, which is the problem. "
        "An implementation can satisfy every clause here and still break the rule, so the fix is a "
        "clause and the decision is a person's."
    ),
    "spec-coverage": (
        "Filed as `spec-coverage` because the behaviour is right today and nothing pins it. There is "
        "nothing to fix in the tree, and a change through this code breaks the guarantee with every "
        "check green."
    ),
    "documentation": (
        "Filed as `documentation` because no code is wrong. What is wrong is what a reviewer is told "
        "before they read a diff, and this repository's review pass is prose that a person follows."
    ),
}

IMPACT_LINE = {
    "high": (
        "the rule this repository exists for does not hold on this path. Untrusted content reaches a "
        "context that is supposed to be closed to it, or steers an effect a person approved a "
        "different version of."
    ),
    "medium": (
        "a real defect that needs a precondition an attacker does not control on their own. It does "
        "not stand on its own today and it is a step somebody else can stand on."
    ),
    "low": (
        "none today. The behaviour holds, and what is wrong is that nothing keeps it holding or that "
        "two documents disagree about it."
    ),
}

FOUND_BY = (
    "Found by the `security-audit` skill: the `{lane}` lane read the code and a second pass tried to "
    "disprove the finding before it was drafted. Filed by a tool rather than by a person, so nothing "
    "here is triaged and no `importance`, `urgency` or `size` has been judged. The verifier's reason "
    "for letting it stand: {reason}"
)

MECHANICAL_FOUND_BY = (
    "Found by the mechanical half of the `security-audit` skill, which no model takes part in and "
    "which decides this deterministically. Filed by a tool rather than by a person, so nothing here "
    "is triaged and no `importance`, `urgency` or `size` has been judged."
)

AREAS = ("trust", "tools", "turns", "interface", "cli", "delegation", "backends", "skus", "i18n")

# The build and the tracker rather than the product. There is no `area/` for it, and a finding about a
# workflow belongs to whoever owns the pipeline.
INFRASTRUCTURE = "infrastructure"


def label_set(finding):
    """Every label this issue carries, and none that belong to triage.

    `importance`, `urgency` and `size` are the triage-issues skill's to judge. Guessing at one here
    would put a finding nobody has read into somebody's queue.
    """
    labels = [SECURITY, NEEDS_REVIEW]
    kind = KIND_LABEL.get(finding.get("kind"))
    if kind:
        labels.append(kind)
    impact = finding.get("impact")
    if impact in ("high", "medium", "low"):
        labels.append(f"severity/{impact}")
    area = (finding.get("area") or "").strip().removeprefix("area/")
    if area in AREAS:
        labels.append(f"area/{area}")
    elif area == INFRASTRUCTURE:
        labels.append(INFRASTRUCTURE)
    return sorted(dict.fromkeys(labels))


def unlocal(text):
    """Nothing in a body naming the machine the run happened on.

    A lane and a verifier are both told the tree to check is an absolute path, because one resolving
    a relative place against its own working directory audits a different checkout. So they write
    commands naming it, and a body that keeps them tells everybody reading the issue where somebody's
    checkout is and hands them a command that runs nowhere else.
    """
    root = str(Path.cwd())
    return text.replace(root + "/", "").replace(root, ".")


def prose(text):
    """A finding's phrase as a sentence in a body.

    A summary is written to be scanned in a terse report, where every line starts lowercase. The same
    string opening a paragraph in an issue reads as a fragment.
    """
    ended = shared.sentence(text)
    return ended[:1].upper() + ended[1:] if ended else ended


def subsystem(finding):
    """What the title leads with: the clause, else the area, else the lane."""
    if finding.get("clause"):
        return finding["clause"]
    area = (finding.get("area") or "").strip().removeprefix("area/")
    if area in AREAS or area == INFRASTRUCTURE:
        return area
    lane = finding.get("lane")
    return "audit" if lane in (None, "mechanical") else lane


def title_for(finding):
    """`subsystem: the mechanism that is wrong, so the consequence`.

    A finding that carries its own title uses it. A lane's `summary` has room for the counts and a
    title does not, and truncating the second into the first stops it mid clause.

    No backticks: a title is not rendered as markdown anywhere it is read, and searching for one that
    has them means guessing where they were.
    """
    said = (finding.get("title") or finding.get("summary") or "the guarantee does not hold").strip()
    lead = subsystem(finding)

    bare = said.rstrip(".").replace("`", "")
    if bare.lower().startswith(lead.lower()):
        title = bare
    else:
        # Lowercase after the colon, where what follows is a sentence. A symbol keeps the case it is
        # spelled with, and two things say it is one: a lane writes it in backticks, and a name like
        # `PartialEq` or `LABEL-3` carries an uppercase letter past the first.
        word = said.split(" ", 1)[0]
        sentence = not said.startswith("`") and not any(one.isupper() for one in word[1:])
        if sentence:
            bare = bare[:1].lower() + bare[1:]
        title = f"{lead}: {bare}"
    if len(title) > TITLE_LIMIT:
        cut = title[:TITLE_LIMIT]
        # A title cut at a word boundary still dangles: "that interpolates Display for" ends on a
        # preposition whose object went over the limit. So prefer the last clause boundary that fits,
        # and where there is none, drop the words that were leading somewhere.
        comma = cut.rfind(", ")
        title = cut[:comma] if comma > TITLE_LIMIT // 2 else cut.rsplit(" ", 1)[0]
        title = DANGLING.sub("", title).rstrip(" ,;:")
    return title


def key_for(finding, title):
    """The one string that tells this issue apart from every other issue.

    Dedup runs on it against the titles already on the tracker, so of the things a finding names it
    has to be one the title kept. That is why the title is an argument: a summary says
    `docs/specs/layering.md` where the title says `layering.md`, and a key nothing can match sends
    dedup back to comparing wording. The findings about specs with unpinned clauses are one sentence
    with one word changed, so on wording alone the first of them swallows the rest.
    """
    named = []
    if finding.get("clause"):
        named.append(finding["clause"])
    for text in (finding.get("title"), finding.get("summary")):
        named.extend(re.findall(r"`([^`]+)`", text or ""))

    lowered = title.lower()
    for one in named:
        if one.lower() in lowered:
            return one
        # A path the title shortened to its file name still tells two specs apart.
        tail = one.rsplit("/", 1)[-1]
        if tail != one and tail.lower() in lowered:
            return tail
    return named[0] if named else subsystem(finding)


def slug_for(finding, taken):
    base = re.sub(
        r"[^A-Za-z0-9._-]+", "-", f"{subsystem(finding)}-{finding.get('kind', 'finding')}"
    ).strip("-")
    slug, extra = base, 1
    while slug in taken:
        extra += 1
        slug = f"{base}-{extra}"
    taken.add(slug)
    return slug


def place_excerpt(finding):
    """The code the finding happens in, numbered, from the first evidence item that resolves."""
    items = shared.evidence_items(finding.get("evidence"))
    if finding.get("place"):
        items = [finding["place"]] + items
    path, line = shared.first_site(items)
    if path is None:
        return []
    return ["The code it happens in:", "", *shared.code_at(path, line)]


def reproduce(finding, screen, session):
    """The section that decides whether somebody can act on this without rebuilding it first.

    A screen replayed out of the interface is the strongest form, because a reader sees the failure
    rather than being told about it. Capturing one means running the interface, so it arrives as a
    sidecar file a person fills in and the draft says which file and what to aim at.
    """
    lines = ["## Reproduce", ""]

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
    if screen:
        wall = shared.fence(screen)
        lines += [
            "This is the screen it draws:",
            "",
            wall + "text",
            screen.rstrip("\n"),
            wall,
            "",
        ]

    commands = finding.get("reproduce") or []
    if isinstance(commands, str):
        commands = [commands]
    if commands:
        lines += ["```sh", *[str(one) for one in commands], "```", ""]

    if finding.get("failure"):
        lines += [prose(finding["failure"]), ""]
    elif not (session or commands):
        lines += [
            "Not reduced to a session. This was found by reading the code rather than by running it, "
            "and the walk above is what to follow.",
            "",
        ]
    return lines


def body_for(finding, screen=None, session=None):
    kind_label = KIND_LABEL.get(finding.get("kind"), "bug")
    impact = finding.get("impact", "medium")

    lines = [f"User impact: {IMPACT_LINE.get(impact, IMPACT_LINE['medium'])}", ""]
    lines += ["## What happens", "", prose(finding.get("summary") or ""), ""]

    if finding.get("clause"):
        where, quoted, governs = shared.clause_quote(
            finding.get("spec") or "docs/specs/labels.md", finding["clause"]
        )
        if quoted:
            lines += [f"{where} says:", "", *quoted, ""]

    lines += place_excerpt(finding)

    if finding.get("entry"):
        lines += ["**Where the bytes come from.** " + prose(finding["entry"]), ""]
    if finding.get("decision"):
        lines += ["**What the code does with them.** " + prose(finding["decision"]), ""]

    lines += reproduce(finding, screen, session)

    if finding.get("gain"):
        lines += [
            "## What this buys an attacker who owns the bytes",
            "",
            prose(finding["gain"]),
            "",
        ]

    items = shared.evidence_items(finding.get("evidence"))
    if items:
        lines += ["Where it is:", ""]
        lines += [shared.as_bullet(item) for item in items]
        lines.append("")

    other = "spec-bug" if kind_label == "bug" else "bug"
    lines += [
        f"## Why {kind_label} and not {other}",
        "",
        WHY_THIS_LABEL.get(kind_label, WHY_THIS_LABEL["bug"]),
        "",
    ]

    lines += ["## The fix", ""]
    lines += [
        prose(finding.get("fix") or "")
        or "Not proposed. What has to hold is above; how is the fixer's decision.",
        "",
    ]
    if finding.get("test_wanted"):
        lines += [
            "The assertion that would pin it, which does not exist today:",
            "",
            prose(finding["test_wanted"]),
            "",
        ]

    if finding.get("refutation"):
        lines += [
            "## The argument that this is fine",
            "",
            prose(finding["refutation"]),
            "",
        ]

    lines += ["## How this was found", ""]
    if finding.get("source") == "audit":
        lines.append(
            FOUND_BY.format(
                lane=finding.get("lane") or "unknown",
                reason=prose(finding.get("verified_reason") or "not recorded"),
            )
        )
    else:
        lines.append(MECHANICAL_FOUND_BY)
    lines.append("")
    return unlocal("\n".join(lines))


def draftable(findings):
    """A finding about the run rather than about the tree names nothing anybody can fix."""
    return [
        finding
        for finding in findings
        if finding.get("kind") not in ("lane-incomplete", "lane-unreadable")
    ]


def draft(findings, out):
    """One file per finding, plus the screen each one that can be shown is still waiting for."""
    out.mkdir(parents=True, exist_ok=True)
    drafts, taken = [], set()
    for finding in findings:
        title = title_for(finding)
        slug = slug_for(finding, taken)
        screen_file = out / f"{slug}.screen.txt"
        session_file = out / f"{slug}.session.txt"
        screen = screen_file.read_text(encoding="utf-8") if screen_file.is_file() else None
        session = session_file.read_text(encoding="utf-8") if session_file.is_file() else None
        body_file = out / f"{slug}.md"
        body_file.write_text(body_for(finding, screen, session), encoding="utf-8")
        drafts.append(
            {
                "title": title,
                "slug": slug,
                "body_file": str(body_file),
                "labels": label_set(finding),
                "impact": finding.get("impact"),
                "kind": finding.get("kind"),
                "lane": finding.get("lane"),
                "key": key_for(finding, title),
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
        lines.append(f"  {entry['impact'] or 'n/a':<6} {entry['title']}")
        lines.append(f"         {', '.join(entry['labels'])}")
        lines.append(f"         body {entry['body_file']}")
        if entry["screen_wanted"] and not entry["has_screen"]:
            lines.append(f"         wants a screen: {entry['screen_wanted']}")
            lines.append(
                f"         script {entry['session_file']}, "
                f"capture {entry['capture_file']}, screen {entry['screen_file']}"
            )
    lines.append("")
    counted = f"{len(drafts)} draft" + ("" if len(drafts) == 1 else "s")
    lines.append(f"{counted} in {out}. Post them with:")
    lines.append("")
    lines.append(f"  python3 agents/skills/security-audit/post-issues.py --work-dir {out.parent}")
    waiting = sum(1 for one in drafts if one["screen_wanted"] and not one["has_screen"])
    if waiting:
        lines.append(
            f"{waiting} of them can be shown on a screen and none has one yet. A reader who can see "
            "the failure does not have to reproduce it first."
        )
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--out", default=None, help="default: WORK_DIR/issues")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    collector = importlib.util.spec_from_file_location(
        "audit_collect", _here / "collect-findings.py"
    )
    collect = importlib.util.module_from_spec(collector)
    collector.loader.exec_module(collect)

    manifest_path = Path(args.work_dir) / "manifest.json"
    if not manifest_path.exists():
        print(f"no manifest at {manifest_path}", file=sys.stderr)
        return 2
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

    findings = list(manifest.get("mechanical_findings", []))
    confirmed, _, _, _ = collect.load_verdicts(manifest)
    findings.extend(confirmed)

    out = Path(args.out) if args.out else Path(args.work_dir) / "issues"
    drafts = draft(draftable(findings), out)

    if args.json:
        print(json.dumps({"drafts": drafts, "out": str(out)}, indent=2))
    else:
        print(report(drafts, out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
