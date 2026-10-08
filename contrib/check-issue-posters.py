#!/usr/bin/env python3
"""Holds every way of filing an issue here to the one definition that says how.

agents/agents/issue-poster.md is the procedure for filing an issue: search the backlog, label,
assign, post, read back. docs/development/labelling-issues.md is what the labels mean. Without a
check, the next skill that posts restates both in its own text, which is how five posters came to
carry five variants of the rules and a change to a scale came to be a change to five files.

This fails when the definition would not load as a delegate definition (the rules of
crates/agent/src/agents.rs `read_definition`), when it writes a label name that
labelling-issues.md does not, when a skill that runs `gh issue create` does not name
`issue-poster`, and when agents/AGENTS.md does not point at it.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

AGENT = "agents/agents/issue-poster.md"
LABELLING = "docs/development/labelling-issues.md"
INSTRUCTIONS = "agents/AGENTS.md"
SKILLS = "agents/skills"
POSTER = "issue-poster"

KINDS = ("reader", "checker", "worker")

# A label is one lower-case word, or a prefix and a value. A `*` stands for any value of the prefix.
LABEL = re.compile(r"^[a-z][a-z0-9-]*(/[a-z0-9*-]+)?$")
# Prefixes that only a label has, so a span carrying one is a label wherever it is written.
PREFIXES = ("area", "importance", "urgency", "size", "severity")
LABELS_HEADING = "## Labels"

# Both spellings of the call: typed as a command, and as the arguments a script hands to `gh`.
POSTS = re.compile(r"""\bgh\s+issue\s+create\b|["']issue["']\s*,\s*["']create["']""")

FENCE = re.compile(r"^```.*?^```[ \t]*$", re.DOTALL | re.MULTILINE)
SPAN = re.compile(r"`([^`\n]+)`")
LABEL_FLAG = re.compile(r"--(?:add-)?label[ =]+['\"]?([^\s'\"\\]+)")


def declarations(text):
    """The `key: value` pairs of the front matter, or None where there is none.

    Read the way crates/agent/src/skills.rs `declarations` reads it: the first line is `---`, a
    line indented under a key continues it, and an unclosed block is not front matter.
    """
    lines = text.splitlines()
    if not lines or lines[0].rstrip() != "---":
        return None
    block = []
    for line in lines[1:]:
        if line.rstrip() == "---":
            break
        block.append(line)
    else:
        return None

    declared = {}
    at = 0
    while at < len(block):
        line = block[at]
        at += 1
        wrapped = []
        while at < len(block) and (
            not block[at].strip() or len(block[at]) - len(block[at].lstrip()) > len(line) - len(line.lstrip())
        ):
            wrapped.append(block[at].strip())
            at += 1
        if ":" not in line:
            continue
        key, first = line.split(":", 1)
        declared[key.strip()] = " ".join(part for part in [first.strip(), *wrapped] if part).strip("'\"")
    return declared


def definition_problems(text):
    """Why this text would not load as a delegate definition, one line each."""
    declared = declarations(text)
    if declared is None:
        return ["it has no closed front matter, so it is not a delegate definition"]
    problems = []
    name = declared.get("name", "")
    if not name:
        problems.append("it needs a name")
    elif name.startswith("-") or ":" in name:
        problems.append("its name may not begin with '-' or contain a colon")
    elif name in KINDS:
        problems.append("reader, checker and worker are the kinds' own names")
    if not declared.get("description"):
        problems.append("it needs a description saying when to use it")
    if "kind" not in declared:
        problems.append("it needs a kind")
    elif declared["kind"] not in KINDS:
        problems.append("its kind is not one of reader, checker or worker")
    return problems


def labels_written(text):
    """Every label the definition writes: in a `--label` flag, with a label prefix, or under Labels."""
    labels = set()
    for fenced in FENCE.findall(text):
        for flag in LABEL_FLAG.findall(fenced):
            labels.update(part for part in flag.split(",") if LABEL.match(part))
    prose = FENCE.sub("", text)

    section = False
    for line in prose.splitlines():
        if line.startswith("## "):
            section = line.strip() == LABELS_HEADING
        for span in SPAN.findall(line):
            if LABEL.match(span) and (section or span.split("/")[0] in PREFIXES and "/" in span):
                labels.add(span)
    return labels


def labels_documented(text):
    return {span for span in SPAN.findall(FENCE.sub("", text)) if LABEL.match(span)}


def unknown_labels(written, documented):
    """The written labels the document does not have. `area/*` needs only one `area/` label there."""
    unknown = []
    for label in sorted(written):
        if label.endswith("/*"):
            if not any(known.startswith(label[:-1]) for known in documented):
                unknown.append(label)
        elif label not in documented:
            unknown.append(label)
    return unknown


def read(path):
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return None


def posting_skills(skills):
    """Every skill with a file that files an issue, as (skill, file)."""
    found = []
    for skill in sorted(skills.iterdir()) if skills.is_dir() else []:
        for path in sorted(skill.rglob("*")):
            text = read(path) if path.is_file() else None
            if text is not None and POSTS.search(text):
                found.append((skill, path))
                break
    return found


def problems(root):
    complaints = []

    agent = read(root / AGENT)
    labelling = read(root / LABELLING)
    if agent is None:
        complaints.append(f"{AGENT}: not found, so there is no definition for a poster to use")
    else:
        complaints += [f"{AGENT}: {problem}" for problem in definition_problems(agent)]
        if labelling is None:
            complaints.append(f"{LABELLING}: not found, so the labels in {AGENT} cannot be checked")
        else:
            for label in unknown_labels(labels_written(agent), labels_documented(labelling)):
                complaints.append(
                    f"{AGENT}: writes the label `{label}`, which {LABELLING} does not. "
                    "Add it there with its meaning or drop it here."
                )

    instructions = read(root / INSTRUCTIONS)
    if instructions is None or POSTER not in instructions:
        complaints.append(f"{INSTRUCTIONS}: does not name `{POSTER}`, so a session filing an issue is not told to use it")

    for skill, path in posting_skills(root / SKILLS):
        text = read(skill / "SKILL.md")
        if text is None or POSTER not in text:
            complaints.append(
                f"{skill.relative_to(root)}/SKILL.md: {path.name} files an issue and the skill does not "
                f"name `{POSTER}`. Point it at {AGENT} rather than restating the labelling rules."
            )
    return complaints


def check(root):
    complaints = problems(root)
    for complaint in complaints:
        print(complaint, file=sys.stderr)
    if complaints:
        return 1
    print("check-issue-posters: the definition loads, writes only documented labels, and every poster names it")
    return 0


GOOD_AGENT = """---
name: issue-poster
description:
  'Files one issue. Use it in place of a bare gh issue create.'
kind: worker
---

## Labels

- One kind, or two for a `spec-mismatch`. Never `urgency/p1`.
- One `area/*`, and `infrastructure`.
- Axes `importance/*` and `size/*`.

```bash
gh issue create --title '<title>' --label <label> --label bug --label area/trust
gh label create <name> --color <hex>
```

Reading `labelling-issues.md` is not a label.
"""

GOOD_LABELLING = """# Filing an issue

Kinds: `bug`, `spec-mismatch`. Areas: `area/trust`, `area/tools`. `infrastructure` is not an area.
| `importance/p1` | x |
| `urgency/p1` | x |
| `size/1` | x |
"""

GOOD_SKILL = "---\nname: x\n---\nFile it with [issue-poster](../../agents/issue-poster.md).\n"


def tree(root, agent=GOOD_AGENT, labelling=GOOD_LABELLING, instructions="Use issue-poster.\n", skills=None):
    """A tree holding what the check reads, each part replaceable so one case breaks one thing."""
    for name, text in ((AGENT, agent), (LABELLING, labelling), (INSTRUCTIONS, instructions)):
        if text is not None:
            (root / name).parent.mkdir(parents=True, exist_ok=True)
            (root / name).write_text(text, encoding="utf-8")
    for skill, files in (skills if skills is not None else {"plain": {"SKILL.md": GOOD_SKILL}}).items():
        for name, text in files.items():
            (root / SKILLS / skill / name).parent.mkdir(parents=True, exist_ok=True)
            (root / SKILLS / skill / name).write_text(text, encoding="utf-8")


def swap(text, old, new):
    assert old in text, old
    return text.replace(old, new)


# One fixture per way a poster can drift, each breaking one thing, because a check that reads the
# label section and not the commands, or the skill text and not the script, passes on this tree
# today and would pass on the sixth poster too.
CASES = [
    ("a consistent tree passes", {}, None),
    ("a label invented under Labels", {"agent": swap(GOOD_AGENT, "`infrastructure`", "`needs-triage`")}, "needs-triage"),
    ("a prefixed label invented in prose", {"agent": swap(GOOD_AGENT, "`size/*`", "`effort/3`")}, "effort/3"),
    ("a label invented in a command", {"agent": swap(GOOD_AGENT, "--label bug", "--label wontfix")}, "wontfix"),
    ("a wildcard over a prefix the document lacks", {"agent": swap(GOOD_AGENT, "`size/*`", "`effort/*`")}, "effort/*"),
    ("a label invented in a comma list", {"agent": swap(GOOD_AGENT, "--label bug", "--label bug,wontfix")}, "wontfix"),
    ("a label invented in an edit", {"agent": swap(GOOD_AGENT, "gh label create", "gh issue edit 1 --add-label wontfix\ngh label create")}, "wontfix"),
    ("a label the document lost", {"labelling": swap(GOOD_LABELLING, "`bug`, ", "")}, "`bug`"),
    ("no front matter", {"agent": "## Labels\n"}, "front matter"),
    ("front matter never closed", {"agent": swap(GOOD_AGENT, "kind: worker\n---", "kind: worker")}, "front matter"),
    ("no kind", {"agent": swap(GOOD_AGENT, "kind: worker\n", "")}, "needs a kind"),
    ("a kind nobody enumerated", {"agent": swap(GOOD_AGENT, "kind: worker", "kind: admin")}, "not one of"),
    ("no description", {"agent": swap(GOOD_AGENT, "description:\n  'Files one issue. Use it in place of a bare gh issue create.'\n", "")}, "description"),
    ("a kind's own name", {"agent": swap(GOOD_AGENT, "name: issue-poster", "name: worker")}, "kinds' own names"),
    ("no definition at all", {"agent": None}, "not found"),
    ("instructions that do not point at it", {"instructions": "Be nice.\n"}, "does not name"),
    (
        "a skill typing the command without naming it",
        {"skills": {"filer": {"SKILL.md": "Run `gh issue create --label bug`.\n"}}},
        "agents/skills/filer/SKILL.md",
    ),
    (
        "a skill whose script files the issue",
        {"skills": {"scripted": {"SKILL.md": "Runs a script.\n", "post.py": 'gh(["issue", "create", "--title", t])\n'}}},
        "post.py",
    ),
    (
        "a skill that posts and names it",
        {"skills": {"filer": {"SKILL.md": "Use issue-poster.\n", "post.py": 'gh(["issue", "create"])\n'}}},
        None,
    ),
    (
        "a skill that lists issues without filing one",
        {"skills": {"reader": {"SKILL.md": "Run `gh issue list` and `gh issue view`.\n"}}},
        None,
    ),
]


def selftest():
    failures = []
    for name, changes, blamed in CASES:
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            tree(root, **changes)
            said = problems(root)
        if (blamed is None) != (not said):
            failures.append(f"{name}: {'failed' if said else 'passed'} when it should have {'passed' if blamed is None else 'failed'}: {said}")
        elif blamed is not None and not any(blamed in line for line in said):
            failures.append(f"{name}: did not name {blamed!r}: {said}")

    for failure in failures:
        print(f"selftest: {failure}", file=sys.stderr)
    if failures:
        print(f"{len(failures)} of {len(CASES)} cases failed", file=sys.stderr)
        return 1
    print(f"selftest: {len(CASES)} cases passed")
    return 0


def main():
    ap = argparse.ArgumentParser(description="Check that every poster uses the issue-poster definition.")
    ap.add_argument("--root", type=Path, default=ROOT, help="the tree to read (default: this one)")
    ap.add_argument("--selftest", action="store_true", help="check the verdict on fixture trees, reading no tree")
    args = ap.parse_args()
    return selftest() if args.selftest else check(args.root)


if __name__ == "__main__":
    sys.exit(main())
