#!/usr/bin/env python3
"""Enumerate the surface the rule is held on, and write one prompt file per audit lane.

`make check-spec` pins how many times untrusted bytes are released. It cannot pin what is decided
afterwards, it cannot fault a clause, and it does not reach code no spec governs. This prepares the
audit that covers those three, and it spends no model tokens doing it: every lane starts from a list
of places rather than from an instruction to go and look at things.

    python3 agents/skills/security-audit/security-audit.py [lane ...] [--mechanical-only]

Stdout is `{"work_dir": ..., "manifest": ...}`. The mechanical report goes to stderr, because those
findings are already decided and a reader should see them before any lane runs.

The mechanical half checks what a tool can check and the specs do not yet: that the two documents
naming the admitted exceptions agree on how many there are, that nothing has been implemented on
`Labelled` that would let a caller read a label's content without asking, that the constructor which
is how a value gets a better label than its inputs had is pinned somewhere, that every workflow step
names a commit rather than a tag its owner can move, and that no job holding a credential installs or
runs a dependency beside it. A rule that can be written as one of these belongs here rather than in a
reviewer's head.
"""

import argparse
import importlib.util
import json
import re
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "check-spec"))

from specs import load_specs  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    "check_spec", Path(__file__).resolve().parent.parent / "check-spec" / "check-spec.py"
)
mechanics = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mechanics)

ERROR = "error"
WARNING = "warning"

REVIEW_DOC = Path("docs/development/reviewing-for-the-rule.md")
LABELS_SPEC = Path("docs/specs/labels.md")

# The carrier, and the two ways a value gets one. `declassify` is pinned by `labels.md`; these are
# not, and they are the other end of the same guarantee.
CONSTRUCTORS = ("Labelled::new", "Labelled::trusted")
UNWITNESSED_READ = "into_trusted"
RELEASE = "Labelled::declassify"

# What `Labelled` is allowed to implement. Everything else would hand a caller a way to read or
# compare content without a witness, which is the guarantee the absent implementations are.
# `labels.md` states this as holding by construction, so nothing fails today if one is added.
ALLOWED_IMPLS = {"fmt::Debug"}
IMPL_FOR_LABELLED = re.compile(r"^\s*impl(?:<[^>]*>)?\s+(?!Labelled\b)(.+?)\s+for\s+Labelled\b")

# The gates. A witness may be minted nowhere else, and reaching content through anything but one of
# these is shape 3.
GATES = (
    "Policy::present",
    "Policy::render_in_place",
    "Policy::read_trusted_content",
    "Policy::read_planner_argument",
)
WITNESS = "Declassification::authorise"

# The specs the guarantee rests on. A clause of one of these that nothing pins is worth reporting
# even though `check-spec` already counts it, because there it is one warning among many and here it
# is the list of what a change could break while staying green.
# Spelled relative to `docs/specs`, so a spec in a subdirectory can be named. `tools/run.md` is
# here because the standing answers a run prompt records are what four of this repository's reported
# findings turned on, and a clause governing them is as able to permit a violation as one about a
# label.
GUARANTEE_SPECS = (
    "labels.md",
    "routing.md",
    "layering.md",
    "trust-map.md",
    "processors.md",
    "vetting.md",
    "permissions.md",
    "tools/run.md",
)

NUMBERS = {
    "one": 1,
    "two": 2,
    "three": 3,
    "four": 4,
    "five": 5,
    "six": 6,
    "seven": 7,
    "eight": 8,
}
PLACES = re.compile(
    r"\b(" + "|".join(NUMBERS) + r")\s+places?\s+(?:in|do|that)", re.IGNORECASE
)
FUNCTION = re.compile(r"\bfn\s+([A-Za-z0-9_]+)")

# The struct a standing answer is keyed on, and where its fields are declared. A key built out of
# fewer of them than the prompt displayed covers a line nobody read, which is the defect RUN-8's
# environment, tree and path-bytes paragraphs were each written after.
STEP_STRUCT = Path("crates/core/src/command.rs")
STEP_DECLARATION = re.compile(r"^\s*pub\s+struct\s+Step\s*\{")
STEP_FIELD = re.compile(r"^\s*pub\s+([a-z_][a-z0-9_]*)\s*:")
STEP_READ = re.compile(r"\bstep\.([a-z_][a-z0-9_]*)\b")
STEP_DESTRUCTURE = re.compile(r"\blet\s+(?:Remembered|Written)?Step\s*\{")
RUN_SPEC = Path("docs/specs/tools/run.md")
# A function may read a `Step` field by field for something that is not a key: the lines a deny rule
# is matched against, or the read set a pure plan is proven by. Those are named in the spec rather
# than here, so admitting one is an edit somebody reviews.
NOT_A_KEY = "reads_a_step_without_keying"

# A third party step in a workflow, and the only form of it that names one immutable thing. A tag or
# a branch is whatever its owner points it at today, and every one of these runs with the repository
# checked out and a token in the environment.
WORKFLOWS = Path(".github/workflows")
USES = re.compile(r"^\s*(?:-\s*)?uses:\s*([^\s#]+)")
PINNED = re.compile(r"^[\w.-]+/[\w.:/-]+@[0-9a-f]{40}$")

# The structure of a workflow, to the depth these checks read: the jobs, what each one is granted,
# and the text of every `run` in it.
JOBS = re.compile(r"^jobs:\s*$")
JOB = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
PERMISSIONS = re.compile(r"^\s*permissions:\s*(.*)$")
RUN = re.compile(r"^\s*(?:-\s+)?run:\s*(.*)$")
BLOCK = ("", "|", "|-", "|+", ">", ">-", ">+")

# What a job can hold that is worth stealing, and the commands that run bytes nobody here wrote. The
# grant is a permission to request a token rather than a token, which is the whole of the difference:
# the runner hands every step in the job the two variables that turn it into one.
#
# Every spelling of the grant, because the check is worth no more than the narrowest one it reads:
# a block, an inline map, a quoted value, and `write-all`, which grants this along with the rest.
GRANT = re.compile(r"""id-token\s*:\s*["']?write\b|\bwrite-all\b""")
SECRET = re.compile(r"\bsecrets\.[A-Za-z_]")
DEPENDENCY = (
    (
        re.compile(r"(?:^|[\s;&|(])(?:npm|pnpm|yarn)\s+(?:ci|install|i|add)\b"),
        "installs a dependency",
    ),
    (
        re.compile(
            r"(?:^|[\s;&|(])(?:(?:npm|pnpm|yarn)\s+(?:run|exec|test|start|dlx)\b|npx\b"
            r"|\.?/?node_modules/\.bin/)"
        ),
        "runs a dependency",
    ),
)

LANES = (
    "laundering",
    "decisions-after-release",
    "gates",
    "known-costs",
    "entry-to-planner",
    "clause-permits-violation",
    "unpinned-guarantee",
    "supply-chain",
)


REPRODUCE = "python3 agents/skills/security-audit/security-audit.py --mechanical-only"


def finding(severity, kind, title, summary, area, impact, evidence=None, fix=None, gain=None):
    """A decided finding.

    `title` is separate from `summary` because a title has to say the mechanism and the consequence in
    one line and a summary has room for the counts. Truncating the second into the first produces a
    title that stops mid clause, which is what somebody scanning a tracker reads.
    """
    return {
        "severity": severity,
        "kind": kind,
        "title": title,
        "summary": summary,
        "area": area,
        "impact": impact,
        "evidence": evidence,
        "fix": fix,
        "gain": gain,
        "reproduce": [REPRODUCE],
        "source": "mechanical",
        "lane": "mechanical",
    }


def enclosing(lines, number):
    """The function a line is inside, by walking back to the nearest `fn`.

    A release site says nothing on its own. What matters is whether the function holding it goes on
    to decide something, so every enumerated site carries the name of the function to read.
    """
    for index in range(number - 1, -1, -1):
        found = FUNCTION.search(lines[index])
        if found:
            return found.group(1)
    return "(top level)"


def construction_hits(symbol, sources):
    """Every call of an associated function, matched on the qualified name and nothing else.

    Not `mechanics.guard_sites`, which counts the same symbol to answer a different question. A
    pin has to cover every way a symbol can be reached, so that counts the definition and a bare
    `Labelled::new` handed to `map` as well as the calls. This list is read by a person deciding
    whether each site should exist, and neither of those is a site to decide about.

    An associated function cannot be called on a receiver, so the qualified name with its paren is
    the only way to write a call and a literal count is exact.
    """
    literal = f"{symbol}("
    hits = []
    for path, lines in sources.items():
        for number, raw in enumerate(mechanics.strip_comments(lines), start=1):
            count = raw.count(literal)
            if count:
                hits.append((str(path), number, raw.strip(), count))
    return hits


def sites_for(symbol, sources):
    """Every use of a symbol, as dicts, with the function each one is in."""
    found = []
    hits = construction_hits(symbol, sources) if symbol in CONSTRUCTORS else mechanics.guard_sites(
        symbol, sources
    )
    for path, number, text, count in hits:
        found.append(
            {
                "path": path,
                "line": number,
                "text": text.strip()[:200],
                "count": count,
                "function": enclosing(sources[Path(path)], number),
                "test_file": "/tests/" in path or path.endswith("_test.rs"),
                "core": path.startswith("crates/core/"),
            }
        )
    return found


def in_test_module(sources, path, line):
    """Whether a line sits under a `#[cfg(test)]`, so a count can exclude the tests.

    The tests hold the largest concentration of both primitives and are supposed to: a test that
    proves an untrusted value cannot be read has to build one. Counting them with the rest would
    bury the sites that matter.
    """
    for index in range(min(line, len(sources[path])) - 1, -1, -1):
        stripped = sources[path][index].strip()
        if stripped.startswith("#[cfg(test)]"):
            return True
        if stripped.startswith("impl ") or (stripped.startswith("pub fn ") and index < line - 1):
            return False
    return False


def check_exception_counts():
    """The two documents naming the admitted exceptions have to agree on how many there are.

    `labels.md` is where an exception is recorded and `reviewing-for-the-rule.md` is what a reviewer
    reads before reviewing a label diff. Where the review document names fewer, a reviewer meeting
    the unnamed one has to decide between calling a recorded exception a violation and assuming the
    counts already vetted it. Both are wrong, and the document itself says why: an unlisted exception
    is indistinguishable from a violation.
    """
    if not REVIEW_DOC.is_file() or not LABELS_SPEC.is_file():
        return

    counted = {}
    for path in (REVIEW_DOC, LABELS_SPEC):
        lines = path.read_text(encoding="utf-8").split("\n")
        for number, raw in enumerate(lines, start=1):
            found = PLACES.search(raw)
            if found and "untrusted" in raw.lower():
                counted[path] = (NUMBERS[found.group(1).lower()], number, raw.strip())
                break

    missing = [str(path) for path in (REVIEW_DOC, LABELS_SPEC) if path not in counted]
    if missing:
        yield finding(
            WARNING,
            "exception-count-unstated",
            "the admitted exceptions are counted nowhere, so a reviewer has nothing to check a "
            "diff against",
            "no sentence counting the admitted exceptions was found in "
            + ", ".join(f"`{one}`" for one in missing),
            "trust",
            "low",
            fix="the count is what a reviewer checks a diff against, so it has to be findable",
        )
        return

    review, spec = counted[REVIEW_DOC], counted[LABELS_SPEC]
    if review[0] != spec[0]:
        yield finding(
            ERROR,
            "exception-count-disagreement",
            "the review pass names fewer admitted exceptions than the spec, so an unnamed one "
            "reads as a violation",
            f"`{REVIEW_DOC}` says {review[0]} places in the kernel branch on untrusted bytes and "
            f"`{LABELS_SPEC}` names {spec[0]}",
            "trust",
            "low",
            evidence=[
                f"{REVIEW_DOC}:{review[1]} {review[2][:120]}",
                f"{LABELS_SPEC}:{spec[1]} {spec[2][:120]}",
            ],
            fix="bring the review document to the number the spec records, since the spec is where "
            "an exception is written down and the review document is what is read before a diff",
        )


def check_labelled_impls(sources):
    """Nothing may be implemented on `Labelled` that reads or compares its content.

    The guarantee is partly the implementations that are absent: no `Deref`, no `PartialEq`, no
    `Display`, so a caller cannot reach the value without a witness. `labels.md` records that as
    holding by construction, which means no check fails if somebody adds one.
    """
    for path, lines in sources.items():
        for number, raw in enumerate(mechanics.strip_comments(lines), start=1):
            found = IMPL_FOR_LABELLED.match(raw)
            if not found:
                continue
            trait = found.group(1).strip()
            if trait in ALLOWED_IMPLS:
                continue
            yield finding(
                ERROR,
                "labelled-impl",
                f"{trait} is implemented for Labelled, so content can be read without a witness",
                f"`{trait}` is implemented for `Labelled`, which hands a caller a way to reach a "
                "label's content without a witness",
                "trust",
                "high",
                evidence=[f"{path}:{number} {raw.strip()[:120]}"],
                fix=f"remove it, or record it in `{LABELS_SPEC}` as an exception with what it costs",
                gain="content the planner may not read can be compared against a guess, one guess at "
                "a time, which is how a quarantined secret is read out of a program that never "
                "shows it",
            )


def check_construction_pinned(specs, sources):
    """A constructor that can give a value a better label than its inputs had, pinned nowhere.

    `labels.md` pins `Labelled::declassify` to a count per file, so a new release cannot land
    quietly. A constructor with no such entry leaves the other half of the same guarantee open: a
    value built trusted out of untrusted bytes is read through `into_trusted` with no witness at
    all, because by then the label says it is allowed.
    """
    pinned = set()
    for spec in specs:
        pinned.update(spec.allowlists)

    for constructor in CONSTRUCTORS:
        if constructor in pinned:
            continue
        sites = sites_for(constructor, sources)
        outside = [
            site
            for site in sites
            if not site["core"]
            and not site["test_file"]
            and not in_test_module(sources, Path(site["path"]), site["line"])
        ]
        if not outside:
            continue
        by_file = {}
        for site in outside:
            by_file[site["path"]] = by_file.get(site["path"], 0) + site["count"]
        worst = sorted(by_file.items(), key=lambda item: -item[1])[:6]
        yield finding(
            ERROR,
            "construction-unpinned",
            f"no spec pins {constructor}, so a new site labelling untrusted bytes trusted lands green",
            f"no spec pins `{constructor}`, so a new site that labels untrusted bytes trusted "
            f"lands green; there are {sum(by_file.values())} outside `crates/core` in non-test code",
            "trust",
            "medium",
            evidence=[f"{path} {count} uses" for path, count in worst],
            fix=f"add a `guards` entry for `{constructor}` to `{LABELS_SPEC}` with a `sites:` count "
            f"per file, the way `{RELEASE}` already has one. The counts above are the calls "
            f"outside `crates/core` in non-test code, and a `sites:` list pins the whole tree, so "
            f"read the counts to pin out of `check-spec` rather than off this finding",
            gain="nothing on its own. What it buys is the next change: a call that hands this "
            "constructor bytes from a page or a process, with a label saying a person typed them, "
            "puts untrusted content in the planner's context and passes every check in the tree",
        )


def test_regions(lines):
    """The line ranges under a `#[cfg(test)]`, by brace depth.

    `in_test_module` walks back to the nearest marker and gives up at an `impl`, which is right for
    a call site inside an inherent method and wrong for a function inside a test module that holds
    one. A key check reports a function rather than a call, so it needs the module's extent.
    """
    spans = []
    depth = None
    counter = 0
    armed = False
    for number, raw in enumerate(lines, start=1):
        if depth is None and raw.strip().startswith("#[cfg(test)]"):
            armed = True
        opens = raw.count("{")
        closes = raw.count("}")
        if armed and opens:
            depth = counter
            armed = False
            start = number
        counter += opens - closes
        if depth is not None and counter <= depth:
            spans.append((start, number))
            depth = None
    if depth is not None:
        spans.append((start, len(lines)))
    return spans


def carries_the_guarantee(spec):
    """Whether a spec is one the guarantee rests on, by its path under `docs/specs`.

    One predicate because the two callers have to agree. Comparing base names answers `False` for
    every spec in a subdirectory whatever `GUARANTEE_SPECS` says, so the list and the check would
    disagree quietly, which is how `tools/run.md` went unread while appearing to be named.
    """
    try:
        return Path(spec.rel).relative_to("docs/specs").as_posix() in GUARANTEE_SPECS
    except ValueError:
        return False


def step_fields(sources):
    """The fields of `Step`, read from the declaration rather than listed here.

    Read rather than hardcoded so that adding a field widens this check instead of leaving it
    describing the struct as it used to be.
    """
    lines = sources.get(STEP_STRUCT)
    if not lines:
        return []
    found = []
    inside = False
    for raw in lines:
        if not inside:
            if STEP_DECLARATION.match(raw):
                inside = True
            continue
        if raw.startswith("}"):
            break
        field = STEP_FIELD.match(raw)
        if field:
            found.append(field.group(1))
    return found


def check_key_sites_exhaustive(specs, sources):
    """A function that builds a standing answer's key out of a `Step`, field by field.

    Three of the reports this repository has had were one defect: a key holding less than the prompt
    displayed, so an entry covered a line nobody answered for. The environment was missing, then the
    tree, then the path's own bytes. Each was fixed where it was found, and each could have been
    written again in the next function, because reading `step.resolved` and `step.args` and stopping
    there is not an error a compiler has any reason to report.

    An exhaustive `let Step { .. }` is what makes it one. A field added to the struct then stops the
    build at every site that has to decide about it, which is the difference between a rule somebody
    remembers and a rule something enforces. So this faults a function that reads two or more fields
    without destructuring, and a function reading a `Step` for something other than a key says so in
    `run.md` instead.
    """
    fields = set(step_fields(sources))
    if not fields:
        yield finding(
            ERROR,
            "key-sites-unreadable",
            "the Step declaration could not be read, so nothing checks what a key holds",
            f"`{STEP_STRUCT}` has no `pub struct Step {{` this check can read, so it cannot tell "
            "which fields a key is built from and is silently passing",
            "trust",
            "medium",
            fix=f"restore the declaration in `{STEP_STRUCT}`, or update `STEP_DECLARATION` here to "
            "match where it moved to",
            gain="nothing directly. It removes the check that would report the next key built out "
            "of fewer fields than the prompt showed",
        )
        return

    admitted = set()
    for spec in specs:
        for entry in spec.front.get(NOT_A_KEY, []):
            admitted.add(entry.strip() if isinstance(entry, str) else str(entry))

    reads = {}
    destructures = set()
    for path, lines in sources.items():
        if "/tests/" in str(path):
            continue
        spans = test_regions(lines)
        stripped = mechanics.strip_comments(lines)
        for number, raw in enumerate(stripped, start=1):
            if any(start <= number <= end for start, end in spans):
                continue
            if STEP_DESTRUCTURE.search(raw):
                destructures.add(f"{path}::{enclosing(lines, number)}")
            for field in STEP_READ.findall(raw):
                if field not in fields:
                    continue
                where = f"{path}::{enclosing(lines, number)}"
                reads.setdefault(where, {}).setdefault(field, number)

    # An admission outlives the function it was written about. Left in place it is a standing grant
    # for whatever is next given that name, decided by nobody, which is what this check exists to
    # stop.
    for where in sorted(admitted - set(reads)):
        yield finding(
            WARNING,
            "key-site-admission-stale",
            f"{RUN_SPEC.name} still admits {where}, which reads no Step field",
            f"`{where}` is named under `{NOT_A_KEY}:` in `{RUN_SPEC}` and does not read a `Step` "
            "field by field, so the admission now covers whatever is next written under that name "
            "rather than the function somebody reviewed",
            "trust",
            "low",
            evidence=[f"{RUN_SPEC}: {NOT_A_KEY}: {where}"],
            fix=f"drop the entry from `{NOT_A_KEY}:` in `{RUN_SPEC}`, or correct it to where the "
            "function moved to",
        )

    loose = {
        where: found
        for where, found in reads.items()
        if len(found) >= 2 and where not in destructures and where not in admitted
    }
    if not loose:
        return

    worst = sorted(loose.items(), key=lambda item: -len(item[1]))
    for where, found in worst:
        missing = sorted(fields - set(found))
        yield finding(
            ERROR,
            "key-site-not-exhaustive",
            f"{where.split('::')[-1]} reads a Step field by field, so a new field lands outside it",
            f"`{where}` reads {len(found)} of the {len(fields)} fields of `Step` individually and "
            "does not destructure it, so a field added to the struct is left out of whatever this "
            "function builds and nothing fails",
            "trust",
            "medium" if missing else "low",
            evidence=[f"{where} reads {', '.join(sorted(found))}"]
            + ([f"does not read {', '.join(missing)}"] if missing else []),
            fix=f"open with `let Step {{ {', '.join(sorted(fields))} }} = step;`, naming a field "
            f"`_` where it is deliberately not part of the key, so that a sixth field stops the "
            f"build here. Where this function is not building a key, add `{where}` under "
            f"`{NOT_A_KEY}:` in `{RUN_SPEC}` with the reason",
            gain="a field the next change adds to a command — another way a step differs from the "
            "one a person was shown — is absent from this key, so one answer covers both. That is "
            "the defect RUN-8's environment, tree and path-bytes paragraphs were each written "
            "after, arriving a fourth time in a function nobody thought to re-read",
        )


def check_pinned_actions():
    """Every workflow step names an immutable commit, not a tag somebody else can move.

    A step runs with this repository checked out and a token in its environment, so whoever controls
    what a tag points at controls what runs. Every step in the tree is pinned today and nothing keeps
    it that way, which is the same shape as the constructor above: right now, held by nobody.
    """
    if not WORKFLOWS.is_dir():
        return
    for path in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        lines = path.read_text(encoding="utf-8").split("\n")
        for number, raw in enumerate(lines, start=1):
            found = USES.match(raw)
            if not found:
                continue
            step = found.group(1).strip("\"'")
            # A local action or a container image is not a third party tag.
            if step.startswith("./") or step.startswith("docker://"):
                continue
            if PINNED.match(step):
                continue
            yield finding(
                ERROR,
                "unpinned-action",
                f"{path.name} runs {step} unpinned, so its owner decides what runs here",
                f"`{step}` is not pinned to a commit, so whoever owns that repository decides what "
                "runs here with the tree checked out",
                "infrastructure",
                "high",
                evidence=[f"{path}:{number} {raw.strip()[:120]}"],
                fix="pin it to the full 40 character commit of the release, with the version as a "
                "trailing comment, the way every other step in this workflow already is",
            )


def indented(raw):
    return len(raw) - len(raw.lstrip(" "))


def nested(lines, start, base=None):
    """The lines under a key, by indentation. A blank line does not end a block; a shallower key does.

    `base` is the column the key itself is at, and is worth passing where the key is the first in a
    sequence entry: the `-` sits left of the key, so the dash column would take the entry's other
    keys for the key's own content.

    A comment line is not content. Inside a `run` block it is a line the shell ignores, and above a
    key it is prose, so neither is a command or a permission however it reads.
    """
    base = indented(lines[start]) if base is None else base
    found = []
    for raw in lines[start + 1 :]:
        if not raw.strip():
            continue
        if indented(raw) <= base:
            break
        if raw.lstrip().startswith("#"):
            continue
        found.append(raw)
    return found


def workflow_jobs(lines):
    """Every job in a workflow, with what it is granted and the text of every `run` in it.

    Indentation rather than a YAML parser, because the rest of the mechanical half is standard
    library Python over the checkout and this reads two keys deep. `permissions` at job scope
    replaces the workflow's rather than adding to it, which is what GitHub does with it, so a job
    without its own block is given the workflow's.

    A secret is attributed to the job whose text names it and to every job where the name is above
    `jobs:`, since a workflow-level `env` is in the environment of all of them.

    Jobs are read at one column, which is every job or none rather than some of them: sibling keys in
    YAML share a column, so a job written at another one is a key inside the job above it, which
    GitHub refuses to run at all. A file whose jobs are all at a column this does not read is the
    `workflow-unreadable` error below.
    """
    start = next((number for number, raw in enumerate(lines) if JOBS.match(raw)), len(lines))
    everywhere = any(
        SECRET.search(raw) for raw in lines[:start] if not raw.lstrip().startswith("#")
    )

    grants = []
    jobs = []
    current = None
    in_jobs = False
    for number, raw in enumerate(lines):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        column = indented(raw)
        if column == 0:
            in_jobs = bool(JOBS.match(raw))
            current = None
            found = PERMISSIONS.match(raw)
            if found:
                grants = [found.group(1)] + nested(lines, number)
            continue
        found = JOB.match(raw) if in_jobs and column == 2 else None
        if found:
            current = {
                "name": found.group(1),
                "line": number + 1,
                "grants": None,
                "secret": everywhere,
                "steps": [],
            }
            jobs.append(current)
            continue
        if current is None:
            continue
        if SECRET.search(raw):
            current["secret"] = True
        found = PERMISSIONS.match(raw) if column == 4 else None
        if found:
            current["grants"] = [found.group(1)] + nested(lines, number)
        found = RUN.match(raw)
        if found:
            inline = found.group(1).strip()
            current["steps"].append(
                {
                    "line": number + 1,
                    "body": nested(lines, number, base=raw.index("run:"))
                    if inline in BLOCK
                    else [inline],
                }
            )
    for job in jobs:
        if job["grants"] is None:
            job["grants"] = grants
    return jobs


def dependency_commands(body):
    """The commands in a `run` that install something from the lockfile or execute what it installed.

    `npm publish` is neither: it uploads the files `package.json` lists and needs nothing installed.
    A line the shell treats as a comment is not a command, so a `run` that mentions one in passing is
    not a use of it.
    """
    found = []
    for raw in body:
        line = raw.strip()
        if line.startswith("#"):
            continue
        for pattern, verb in DEPENDENCY:
            if pattern.search(line):
                found.append((verb, line[:120]))
                break
    return found


def check_privileged_job_runs_only_its_own_code():
    """A job that can mint a credential installs and runs nothing from `node_modules`.

    A grant or a secret is readable by every step of the job holding it, so a job is the smallest
    boundary either has. A step that installs from `package-lock.json` and then runs what it
    installed executes bytes nobody here wrote, deliberately: that is what a lint is. In a job
    holding `id-token: write` those bytes can exchange the runner's OIDC token for a publishing
    credential at the registry and ship a tarball with this repository's provenance on it, and the
    same two commands in a job holding `contents: read` reach a green check and nothing else. So the
    finding is the pairing rather than either half.

    What it reads is the npm tree, which is the third-party code a workflow here installs and runs by
    name. A job that compiled the tree would run a crate's build script in the same environment and
    this says nothing about that, which is a widening of this check rather than a second one.
    """
    if not WORKFLOWS.is_dir():
        return
    for path in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        lines = path.read_text(encoding="utf-8").split("\n")
        jobs = workflow_jobs(lines)
        if not jobs:
            if any(JOBS.match(raw) for raw in lines):
                yield finding(
                    ERROR,
                    "workflow-unreadable",
                    f"{path.name} declares jobs this check cannot read, so it is passing in silence",
                    f"`{path}` has a `jobs:` key and no job this check could read under it, so it "
                    "cannot tell what any of them is granted and reports nothing whatever they run",
                    "infrastructure",
                    "medium",
                    fix="write the jobs at the two space indentation every workflow here uses, or "
                    "widen `JOB` in this file to match where they moved to",
                    gain="nothing directly. It removes the check that would report the next "
                    "credential handed to a job that runs somebody else's code",
                )
            continue
        for job in jobs:
            granted = any(GRANT.search(raw) for raw in job["grants"])
            if not granted and not job["secret"]:
                continue
            reached = [
                f"{path}:{step['line']} {verb}: {command}"
                for step in job["steps"]
                for verb, command in dependency_commands(step["body"])
            ]
            if not reached:
                continue
            holds = "id-token: write" if granted else "a secret"
            yield finding(
                ERROR,
                "privileged-job-runs-dependencies",
                f"{path.name} runs a dependency in {job['name']}, the job that holds {holds}, so "
                "whoever owns that dependency can use it",
                f"`{job['name']}` in `{path}` holds {holds} and runs "
                f"{len(reached)} command"
                + ("" if len(reached) == 1 else "s")
                + " that install or execute something from `node_modules`, which every step in that "
                "job can read the credential from",
                "infrastructure",
                "medium",
                evidence=[f"{path}:{job['line']} job `{job['name']}` holds {holds}"] + reached,
                fix="move the install and everything it runs into a job of their own holding only "
                "`contents: read`, and give this job a `needs:` on that one. Where the grant is "
                "declared for the whole workflow, declare it on this job instead: a job is the "
                "smallest boundary it has",
                gain="whoever owns one of the installed packages runs code beside the credential "
                "this job exists to mint, and what they publish with it carries this repository's "
                "provenance",
            )


def check_guarantee_specs_exist():
    """A named guarantee spec that no file answers to.

    Every use of the list skips what it cannot resolve, so a renamed or misspelled spec leaves the
    list looking complete while nothing reads the file. That is the shape of the gap this list was
    widened to close, and a rename is the ordinary way it comes back.
    """
    for one in GUARANTEE_SPECS:
        if not (Path("docs/specs") / one).is_file():
            yield finding(
                ERROR,
                "unpinned-guarantee",
                f"`GUARANTEE_SPECS` names `{one}`, which is not a file, so no pass reads it",
                "the lanes are given the specs the guarantee rests on, and each use of the list "
                f"skips an entry it cannot resolve, so `{one}` is named and unread",
                "trust",
                "high",
                evidence=[f"agents/skills/security-audit/security-audit.py: {one}"],
                fix="point the entry at where the spec moved to, or drop it if the spec is gone",
            )


def check_unpinned_guarantee_clauses(specs):
    """Clauses of the guarantee specs that no test pins.

    `check-spec` already reports these, one warning among many across every spec. Gathered here they
    are a different thing: the list of guarantees a change can break while every check stays green.
    """
    for spec in specs:
        if not carries_the_guarantee(spec):
            continue
        loose = [
            clause
            for clause in spec.clauses
            if not clause.withdrawn and any(one == "none" for one in clause.verified_by)
        ]
        if not loose:
            continue
        yield finding(
            WARNING,
            "unpinned-guarantee",
            f"nothing pins {len(loose)} clause"
            + ("" if len(loose) == 1 else "s")
            + f" of {spec.name}, so a change that breaks one passes every check",
            f"`{spec.rel}` has {len(loose)} clause"
            + ("" if len(loose) == 1 else "s")
            + " at `verified-by: none`, so nothing fails when one stops holding",
            "trust",
            "low",
            evidence=[f"{spec.rel}:{clause.line} {clause.id}: {clause.title}" for clause in loose],
            fix="a clause of one of these specs is what the guarantee is; each wants a test that "
            "fails against the behaviour it forbids",
        )


def bracket_clauses(specs):
    """The clauses of the guarantee specs answered by a bracket rather than by a test.

    A clause at `verified-by: none` is counted by the mechanical pass, and the moment somebody
    answers one the debt moves here, where nothing mechanical can decide whether what the bracket
    names is what holds the clause. So the lane is handed the list rather than asked to find it.
    """
    return [
        (spec, clause)
        for spec in specs
        if carries_the_guarantee(spec)
        for clause in spec.clauses
        if not clause.withdrawn
        and any(one.startswith("by-construction") for one in clause.verified_by)
    ]


def surface(sources, specs):
    """Everything the lanes are given, gathered once."""
    construction = []
    for constructor in CONSTRUCTORS:
        construction.extend(sites_for(constructor, sources))
    governed = {path for spec in specs for path in spec.governs}
    label_touching = sorted(
        {
            site["path"]
            for site in construction
            if not site["test_file"] and site["path"] not in governed
        }
    )
    return {
        "construction": construction,
        "unwitnessed_reads": sites_for(UNWITNESSED_READ, sources),
        "releases": sites_for(RELEASE, sources),
        "witness": sites_for(WITNESS, sources),
        "gates": {gate: sites_for(gate, sources) for gate in GATES},
        "ungoverned_label_files": label_touching,
        "workflows": sorted(str(one) for one in WORKFLOWS.glob("*.y*ml")),
        "dependency_files": sorted(
            str(one) for one in (Path("deny.toml"), Path("Cargo.lock")) if one.is_file()
        ),
        "second_client": sorted(
            str(path)
            for path in sources
            if not path.is_relative_to(Path("crates/net"))
            and "/tests/" not in str(path)
            and any("ureq::" in line for line in sources[path])
        ),
    }


def as_paths(paths):
    return "\n".join(f"- `{one}`" for one in paths) or "- (none)"


def as_list(sites, limit=60):
    lines = []
    for site in sites[:limit]:
        lines.append(f"- `{site['path']}:{site['line']}` in `{site['function']}`")
    if len(sites) > limit:
        lines.append(f"- ... and {len(sites) - limit} more; the enumerator wrote them all to the "
                     "manifest, read it rather than guessing")
    return "\n".join(lines) or "- (none)"


def read_prompt(name):
    """One lane's prompt: what every lane is told, then the lane, then what every lane returns.

    The rule an auditor works to and the shape of the answer are the same for all of them, and a
    lane file that restated either would be a second place for them to drift.
    """
    lanes = Path(__file__).resolve().parent / "lanes"
    return "\n".join(
        (lanes / part).read_text(encoding="utf-8").rstrip()
        for part in ("_preamble.md", f"{name}.md", "_output.md")
    )


def build_lane(name, found, specs, results_file, surface_file=None):
    """One lane's instructions, with the places it starts from filled in."""
    interesting = [
        site
        for site in found["construction"]
        if not site["core"] and not site["test_file"]
    ]
    releases = [site for site in found["releases"] if not site["test_file"]]
    filling = {
        "results_file": results_file,
        # Absolute, because a lane is read by a subagent whose working directory is its own business,
        # and every other path in here is relative to this one.
        "repo_root": str(Path.cwd()),
        "surface_file": str(surface_file) if surface_file else "(not written)",
        "construction_count": len(interesting),
        "construction_sites": as_list(interesting),
        "unwitnessed_reads": as_list(
            [s for s in found["unwitnessed_reads"] if not s["test_file"]]
        ),
        "release_count": len(releases),
        "release_sites": as_list(releases, limit=90),
        "witness_sites": as_list(found["witness"], limit=50),
        "gate_sites": "\n".join(
            f"- `{gate}`: {len(sites)} uses" for gate, sites in found["gates"].items()
        ),
        "ungoverned": "\n".join(f"- `{one}`" for one in found["ungoverned_label_files"])
        or "- (none)",
        "bracket_clauses": "\n".join(
            f"- `{spec.rel}:{clause.line}` {clause.id}: {clause.title}"
            for spec, clause in bracket_clauses(specs)
        )
        or "- (none)",
        "guarantee_specs": "\n".join(
            f"- `docs/specs/{one}`" for one in GUARANTEE_SPECS if Path(f"docs/specs/{one}").is_file()
        ),
        "labels_spec": str(LABELS_SPEC),
        "review_doc": str(REVIEW_DOC),
        "workflow_files": as_paths(found["workflows"]),
        "dependency_files": as_paths(found["dependency_files"]),
        "second_client": as_paths(found["second_client"]),
    }
    return read_prompt(name).format(**filling)


def changed_lanes(base):
    """The lanes worth running for what a branch touched.

    A branch that edits no Rust still wants the two documentation lanes, and one that touches
    `policy.rs` wants everything.
    """
    touched = set(mechanics.changed_files(base))
    if not touched:
        return list(LANES)
    rust = {one for one in touched if one.endswith(".rs")}
    lanes = set()
    if rust:
        lanes.update({"laundering", "gates", "entry-to-planner"})
    if any("policy.rs" in one or "value.rs" in one or "label.rs" in one for one in rust):
        lanes.update({"decisions-after-release", "known-costs"})
    if any(one.startswith("docs/specs/") for one in touched):
        lanes.update({"clause-permits-violation", "unpinned-guarantee"})
    if any(
        one.startswith(".github/") or one.endswith(("Cargo.toml", "Cargo.lock", "deny.toml"))
        for one in touched
    ):
        lanes.add("supply-chain")
    return sorted(lanes) or list(LANES)


def render(findings):
    lines = []
    for item in sorted(findings, key=lambda f: 0 if f["severity"] == ERROR else 1):
        mark = "error" if item["severity"] == ERROR else "warn "
        lines.append(f"  {mark}  {item['summary']}")
        for one in item.get("evidence") or []:
            lines.append(f"         at {one}")
        if item.get("fix"):
            lines.append(f"         fix: {item['fix']}")
    errors = sum(1 for f in findings if f["severity"] == ERROR)
    lines.append("")
    lines.append(f"{len(findings)} mechanical findings, {errors} at error")
    if not findings:
        lines.append("the mechanical pass found nothing")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("lanes", nargs="*", help=f"default: every lane. One of: {', '.join(LANES)}")
    parser.add_argument("--work-dir", default=None)
    parser.add_argument("--mechanical-only", action="store_true", help="no lanes, no model")
    parser.add_argument("--changed", metavar="BASE", default=None, help="lanes for a branch's diff")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    if not Path("docs/specs").is_dir():
        print("run this from the repository root", file=sys.stderr)
        return 2

    specs = load_specs()
    sources = mechanics.load_sources()

    findings = list(check_exception_counts())
    findings += list(check_labelled_impls(sources))
    findings += list(check_construction_pinned(specs, sources))
    findings += list(check_key_sites_exhaustive(specs, sources))
    findings += list(check_pinned_actions())
    findings += list(check_privileged_job_runs_only_its_own_code())
    findings += list(check_guarantee_specs_exist())
    findings += list(check_unpinned_guarantee_clauses(specs))

    unknown = [one for one in args.lanes if one not in LANES]
    if unknown:
        print(f"no such lane: {', '.join(unknown)}", file=sys.stderr)
        return 2
    chosen = args.lanes or (changed_lanes(args.changed) if args.changed else list(LANES))
    if args.mechanical_only:
        chosen = []

    work_dir = Path(args.work_dir or tempfile.mkdtemp(prefix="security-audit-"))
    work_dir.mkdir(parents=True, exist_ok=True)
    found = surface(sources, specs)

    # The surface is every site in the tree and it is already baked into the lane prompts, but a lane
    # prompt lists at most ninety of anything: past that it is a wall nobody reads and the lane spends
    # its run on the first page. So the whole of it goes to a file the prompts name.
    surface_file = work_dir / "surface.json"
    surface_file.write_text(json.dumps(found, indent=2), encoding="utf-8")

    lanes = []
    for name in chosen:
        results_file = work_dir / f"{name}_results.json"
        prompt_file = work_dir / f"{name}_prompt.md"
        prompt_file.write_text(
            build_lane(name, found, specs, results_file, surface_file), encoding="utf-8"
        )
        lanes.append(
            {
                "lane": name,
                "prompt_file": str(prompt_file),
                "results_file": str(results_file),
            }
        )

    # The manifest holds a pointer to it rather than the surface itself, because the manifest is the
    # one thing the session orchestrating a run reads, and a session that reads two hundred kilobytes
    # of call sites is the session this pipeline exists to avoid.
    manifest = {
        "work_dir": str(work_dir),
        "lanes": lanes,
        "mechanical_findings": findings,
        "surface_file": str(surface_file),
        "progress_lines": [
            f"{len(found['construction'])} construction sites, "
            f"{len(found['releases'])} releases, {len(lanes)} lanes",
        ],
    }
    manifest_path = work_dir / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    print(render(findings), file=sys.stderr)
    if args.json:
        print(json.dumps(manifest, indent=2))
    else:
        print(json.dumps({"work_dir": str(work_dir), "manifest": str(manifest_path)}))
    return 1 if any(f["severity"] == ERROR for f in findings) else 0


if __name__ == "__main__":
    sys.exit(main())
