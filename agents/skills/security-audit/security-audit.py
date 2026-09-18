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
is how a value gets a better label than its inputs had is pinned somewhere, and that every workflow
step names a commit rather than a tag its owner can move. A rule that can be written as one of these
belongs here rather than in a reviewer's head.
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
GUARANTEE_SPECS = (
    "labels.md",
    "routing.md",
    "layering.md",
    "trust-map.md",
    "processors.md",
    "vetting.md",
    "permissions.md",
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

# A third party step in a workflow, and the only form of it that names one immutable thing. A tag or
# a branch is whatever its owner points it at today, and every one of these runs with the repository
# checked out and a token in the environment.
WORKFLOWS = Path(".github/workflows")
USES = re.compile(r"^\s*(?:-\s*)?uses:\s*([^\s#]+)")
PINNED = re.compile(r"^[\w.-]+/[\w.:/-]+@[0-9a-f]{40}$")

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

    Not `mechanics.guard_sites`, which is right for what it is for and wrong here. That counts
    `.method(` as well as `Type::method`, because a method is called on a receiver, and it counts
    `fn method` in any file that mentions the qualifier so a renamed symbol cannot read as present.
    For `Labelled::declassify` all three forms are the same symbol. For `Labelled::new` the second
    and third catch every `fn new` and every `x.new(` in a file that says `Labelled` anywhere, which
    is most of them.

    An associated function cannot be called on a receiver, so the qualified name is the only way to
    write one and a literal count is exact. This matters beyond a tidier list: the fix these sites
    argue for is a `sites:` count in the spec, and a count including an unrelated `fn new` is a check
    that fails on the wrong change.
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
    """The constructor that can give a value a better label than its inputs had is pinned nowhere.

    `labels.md` pins `Labelled::declassify` to a count per file, so a new release cannot land
    quietly. The construction end has no such entry, and it is the other half of the same
    guarantee: a value built trusted out of untrusted bytes is read through `into_trusted` with no
    witness at all, because by then the label says it is allowed.
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
            f"per file, the way `{RELEASE}` already has one. The counts above are of the qualified "
            f"name alone; `check-spec` counts a guarded symbol three ways and two of them do not "
            f"apply to an associated function, so pinning these needs `guard_sites` to count "
            f"`Type::function(` exactly or the pin fails on the next unrelated `fn new`",
            gain="nothing on its own. What it buys is the next change: a call that hands this "
            "constructor bytes from a page or a process, with a label saying a person typed them, "
            "puts untrusted content in the planner's context and passes every check in the tree",
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


def check_unpinned_guarantee_clauses(specs):
    """Clauses of the guarantee specs that no test pins.

    `check-spec` already reports these, one warning among many across every spec. Gathered here they
    are a different thing: the list of guarantees a change can break while every check stays green.
    """
    for spec in specs:
        if spec.name not in GUARANTEE_SPECS:
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
        if spec.name in GUARANTEE_SPECS
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
    findings += list(check_pinned_actions())
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
