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
naming the admitted exceptions agree on how many there are, that every crate keeping a network
client of its own is one the egress register admits and no comment claims a dependency a manifest
beside it declares, that the register admitting them leaves
as many prompts out of a verdict's reach as the clause deciding that does, that a field documented
as read in one place is read in one place, that nothing has been implemented on
`Labelled` that would let a caller read a label's content without asking, that the constructor which
is how a value gets a better label than its inputs had is pinned somewhere, that every gate handing
released content to a closure the driver wrote is counted rather than merely named, and so is every
function forwarding its caller's closure to one, that every workflow step
names a commit rather than a tag its owner can move, that every container image this tree runs names a
digest rather than a tag its publisher can move, that no job holding a credential installs or runs a
dependency beside it, that a checkout of this tree names a kind of ref rather than a bare name a
branch and a tag can share, and that the contexts a merge is held to are written down, name jobs
that exist, and cover every job that runs a check. A rule that can be written as one of these
belongs here rather than in a reviewer's head.
"""

import argparse
import importlib.util
import json
import os
import re
import shlex
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
NET_EGRESS_SPEC = Path("docs/specs/network-egress.md")

# The crate holding the policy gate, which is permitted its client by being the gate.
GATE_CRATE = "bravebot-net"

# The libraries a crate takes on to open a socket of its own. The manifest is read rather than the
# code because the manifest is where the decision is made: a crate that has one of these can open a
# socket with it, and whether today's code happens to is not what a reviewer a year from now holds.
# This is the list a reviewer would recognise rather than everything that could ever be a client, so
# a client outside it is the bound on this check and the lane that reads code is what covers it.
NETWORK_CLIENTS = frozenset(
    {
        "attohttpc",
        "curl",
        "hyper",
        "isahc",
        "reqwest",
        "surf",
        "tokio-tungstenite",
        "tungstenite",
        "ureq",
    }
)

# `[dependencies]`, `[dependencies.ureq]`, `[target.'cfg(windows)'.dependencies]` and the same with a
# named subtable. Development and build dependencies are deliberately not matched: neither is in the
# process this guarantee is about.
DEPENDENCY_TABLE = re.compile(r"^\[(?:target\.[^\]]+\.)?dependencies(?:\.([A-Za-z0-9_-]+))?\]$")
PACKAGE_NAME = re.compile(r'^name\s*=\s*"([^"]+)"')
RENAMED = re.compile(r'\bpackage\s*=\s*"([A-Za-z0-9_-]+)"')

# The `## Known costs` bullet that admits a second egress, and the crates it names. Only a bullet
# about opening a socket is read, which keeps a crate named elsewhere in that section for some other
# reason out of the permitted set. It is prose either way: a bullet saying a crate does *not* open
# one reads the same to this, so what the register is held to is drift rather than an author working
# around it, and the stale direction below is a warning for the same reason.
KNOWN_COSTS = "## Known costs"
OPENS_A_SOCKET = re.compile(r"\bsockets?\b|\bHTTP client\b")
WORKSPACE_CRATE = re.compile(r"`(bravebot-[a-z0-9-]+)`")

# A claim in the source that names a crate nothing else may depend on. The crate has to be named for
# this to match: NET-1 states the same property of "it" with the exception recorded in Known costs
# beside it, and issue #87 settled that the clause body stays that way. A sentence in the code that
# names the library has no Known costs beside it, and a reader who checks it against a manifest
# finds it either true or false.
EXCLUSIVE_DEPENDENCY = re.compile(r"no other crate (?:depends on|uses|links|takes) `([a-z0-9_-]+)`")

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
    "Policy::render_pair_in_place",
    "Policy::read_trusted_content",
    "Policy::read_planner_argument",
)
WITNESS = "Declassification::authorise"

# What a gate calls to turn untrusted content away before it releases anything. A gate that never
# reaches it releases content the driver may not read, which is the difference between one a spec
# can name and one whose call sites have to be counted.
REFUSAL = "refuse_untrusted"
# The release itself. A refusal after one says nothing: the bytes are already out.
RELEASED = ".declassify("

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
# A name bound to one of the `Fn` traits, which is a parameter whose value is code the caller wrote
# or a generic the parameter is declared with. The name is what a call has to pass on for the
# function holding it to be a way into a gate rather than a function that takes a callback.
CLOSURE_BOUND = re.compile(r"\b([A-Za-z0-9_]+)\s*:\s*[^,;()]*\bFn(?:Once|Mut)?\s*\(")
STRING_LITERAL = re.compile(r'"(?:[^"\\]|\\.)*"')
# How far a call may run before this stops reading. A call written over more lines than this is
# not one of these, and reading to the end of the file would balance on an unrelated parenthesis.
CALL_LINES = 40

# The clause that decides which of the prompts a check runs for a safe verdict may answer, the cell
# in its table that says one of them is answered, and the sentence in `labels.md` counting the rest.
# `other` falls on either side of the number depending on how the sentence is phrased, and the
# sentence wraps, so both orders are read and the search is over a paragraph rather than a line.
VETTING_SPEC = Path("docs/specs/vetting.md")
PROMPT_SPLIT = "CHECK-12"
ANSWERED = "a safe verdict answers"
OUT_OF_REACH = re.compile(
    r"\b(" + "|".join(NUMBERS) + r")\s+(?:other\s+)?prompts?\s+a\s+check\s+runs\s+for",
    re.IGNORECASE,
)

# A field whose doc names the functions that read it and says those are all of them. The claim is
# what a reviewer meeting one of those functions reads instead of grepping for the others, and it
# is read as the span it occupies rather than as the doc it sits in: a name below it is prose.
CLAIM = re.compile(r"Read by (.*?)\band by nothing else\b")
DOC_LINE = re.compile(r"^\s*///(.*)$")
FIELD_DECLARATION = re.compile(r"^\s*pub(?:\([^)]*\))?\s+([a-z_][a-z0-9_]*)\s*:")
ATTRIBUTE = re.compile(r"^\s*#\[")
NAMED = re.compile(r"`([A-Za-z_][A-Za-z0-9_]*)`")

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
# A job's own `name:`, which is the name its check run reports under and so the name a required
# context has to match. Four columns is the job's; a step's is deeper and a workflow's is at zero.
# An unquoted scalar ends at a ` #`, so the comment is not part of the name.
DISPLAY_NAME = re.compile(r"^ {4}name:\s*(.+?)\s*$")
TRAILING_COMMENT = re.compile(r"\s+#.*$")
EXPRESSION = re.compile(r"\$\{\{")
# A `make` invocation and the check targets in it. Read as the whole command rather than the word
# after `make`, since a target can arrive behind a flag, behind a variable, or second in a list.
MAKE = re.compile(r"\bmake\b[^\n;&|]*")
CHECK_TARGET = re.compile(r"\bcheck-[\w-]+\b")

# Which of those check runs a merge is actually held to, and where the tree says so. The list the
# protection enforces is a repository setting rather than a file, so this is the record of what it
# has to hold, and the documentation is where a target's promise to fail a pull request is written.
REQUIRED_CHECKS = Path("contrib/required-checks.txt")
CHECKS_DOC = Path("docs/development/checks.md")
DOC_TARGET = re.compile(r"`make (check-[\w-]+)`")
GATES_A_PULL_REQUEST = ("fails a pull request", "fails rather than")

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


def workspace_crates():
    """Each crate under `crates/`: its package name and the dependencies its manifest declares.

    A workspace member is read from its own manifest rather than from the lockfile, because what is
    being held here is what a crate asked for. A transitive copy of a client is somebody else's
    dependency arriving through a crate that never names it, which is not a second egress and not a
    line anyone could have reviewed.
    """
    crates = {}
    for manifest in sorted(Path("crates").glob("*/Cargo.toml")):
        name, deps = manifest.parent.name, set()
        in_package, in_dependencies, in_one_dependency = False, False, False
        for raw in manifest.read_text(encoding="utf-8", errors="replace").split("\n"):
            line = raw.strip()
            if line.startswith("["):
                # A comment may follow a table header, and reading the whole line as the header
                # leaves the parse inside the table before it, which is a dependency table hidden
                # or a development one counted as runtime.
                header = line.split("]", 1)[0] + "]"
                table = DEPENDENCY_TABLE.match(header)
                in_package = header == "[package]"
                # `[dependencies.ureq]` names the dependency in the header, and its body is that
                # one crate's own keys rather than more dependencies.
                in_dependencies = bool(table) and not table.group(1)
                in_one_dependency = bool(table) and bool(table.group(1))
                if in_one_dependency:
                    deps.add(table.group(1))
                continue
            if line.startswith("#"):
                continue
            if in_package:
                stated = PACKAGE_NAME.match(line)
                if stated:
                    name = stated.group(1)
                continue
            if not (in_dependencies or in_one_dependency) or "=" not in line:
                continue
            # `package = "ureq"` is the real crate behind a renamed dependency, whether the rename
            # is an inline table or a table of its own.
            renamed = RENAMED.search(line)
            if renamed:
                deps.add(renamed.group(1))
            if in_dependencies:
                # `ureq.workspace = true` is the same declaration as `ureq = { workspace = true }`,
                # and this tree writes its `[package]` keys that way throughout.
                deps.add(line.split("=", 1)[0].strip().split(".", 1)[0].strip())
        crates[manifest.parent.name] = {"name": name, "deps": deps, "manifest": str(manifest)}
    return crates


def recorded_network_clients():
    """The crates `network-egress.md` admits under `## Known costs` as opening their own socket.

    The spec is the register: NET-1 states the property and the Known costs beside it state the
    exceptions, which is the editorial split this repository already uses. So the permitted set is
    read out of the register rather than written down a second time here, where it would be one
    more list to keep in step.
    """
    if not NET_EGRESS_SPEC.is_file():
        return set()
    named, inside, bullet = set(), False, []
    lines = NET_EGRESS_SPEC.read_text(encoding="utf-8").split("\n") + [""]
    for raw in lines:
        if raw.startswith("## "):
            inside = raw.strip() == KNOWN_COSTS
            continue
        if not inside:
            continue
        if raw.startswith("- ") or not raw.strip():
            joined = " ".join(bullet)
            if OPENS_A_SOCKET.search(joined):
                named.update(WORKSPACE_CRATE.findall(joined))
            bullet = []
        if raw.strip():
            bullet.append(raw.strip())
    joined = " ".join(bullet)
    if OPENS_A_SOCKET.search(joined):
        named.update(WORKSPACE_CRATE.findall(joined))
    return named


def comment_blocks(lines):
    """Every run of comment lines joined into one, with where each piece of it came from.

    A sentence in a doc comment wraps wherever the margin falls, so where a claim breaks is an
    accident of the sentence before it, and a regex over single lines reads half of one. Adding a
    word three paragraphs up is enough to move a claim out of reach of a scan that reads one line.

    Yields the joined text and the offset each source line starts at within it, so a match can be
    reported at the line it is on rather than at the top of the block. A finding at the head of a
    module comment is one a reader has to search for, and it is the excerpt that gets filed.
    """
    block, spans, offset = [], [], 0
    for number, raw in enumerate(lines, start=1):
        stripped = raw.strip()
        if stripped.startswith("//"):
            text = stripped.lstrip("/").lstrip("!").strip()
            if text:
                spans.append((offset, number))
                block.append(text)
                offset += len(text) + 1
            continue
        if block:
            yield " ".join(block), spans
        block, spans, offset = [], [], 0
    if block:
        yield " ".join(block), spans


def line_at(spans, offset):
    """The source line an offset into a joined comment block came from."""
    found = spans[0][1]
    for start, number in spans:
        if start > offset:
            break
        found = number
    return found


def check_network_clients_are_recorded(sources):
    """Which crates may open a socket of their own is the register's to say, not a comment's.

    `bravebot-net` is the policy gate and `network-egress.md`'s Known costs admit the one crate that
    keeps a client beside it. Nothing held that: `deny.toml` asks what a crate is licensed under and
    not whether it may be depended on, `check-spec` reads no manifest, and the only enumeration of a
    second client greps one library's name into a lane prompt. So a third client would arrive with
    every check green, and the first paragraph of the gate crate told a reviewer in advance that it
    could not exist, which is the state `reviewing-for-the-rule.md` says an unlisted exception
    produces: meeting one means choosing between calling it a violation and assuming somebody
    already vetted it.

    The stale direction is a warning rather than an error because it is read out of prose: a bullet
    reworded until it no longer names a crate is drift worth reporting and not worth failing a build
    over, where a manifest naming a client nothing admits is decided.
    """
    crates = workspace_crates()
    by_name = {crate["name"]: crate for crate in crates.values()}
    declaring = {
        crate["name"]: sorted(crate["deps"] & NETWORK_CLIENTS)
        for crate in crates.values()
        if crate["deps"] & NETWORK_CLIENTS
    }
    permitted = recorded_network_clients() | {GATE_CRATE}

    for name in sorted(set(declaring) - permitted):
        clients = ", ".join(f"`{one}`" for one in declaring[name])
        yield finding(
            ERROR,
            "unrecorded-network-client",
            "a crate keeps a network client that the egress register does not admit, so a second "
            "way out of this process arrives unremarked",
            f"`{name}` declares {clients} and `{NET_EGRESS_SPEC}` names it nowhere under "
            f"`{KNOWN_COSTS}`, so nothing says what that traffic carries or what it does not get",
            "trust",
            "low",
            evidence=[
                f"{by_name[name]['manifest']} declares {clients}",
                f"the gate is `{GATE_CRATE}` and {NET_EGRESS_SPEC} admits "
                + (
                    ", ".join(f"`{one}`" for one in sorted(permitted - {GATE_CRATE}))
                    or "no crate beside it"
                ),
            ],
            fix=f"record it under `{KNOWN_COSTS}` in `{NET_EGRESS_SPEC}` with what its traffic "
            "carries and which clauses do not reach it, the way `bravebot-skus` is recorded, or "
            "route it through the gate and drop the dependency",
            gain="a second egress that the gate never sees, reviewed by nobody because the register "
            "a reviewer checks it against does not mention it",
        )

    for name in sorted(permitted - set(declaring) - {GATE_CRATE}):
        yield finding(
            WARNING,
            "network-client-record-is-stale",
            "the egress register admits a second client that no manifest declares, so the standing "
            "exception is for whatever is next given that name",
            f"`{NET_EGRESS_SPEC}` names `{name}` under `{KNOWN_COSTS}` as keeping a client of its "
            "own, and no manifest under `crates/` declares one for it",
            "trust",
            "low",
            fix=f"drop `{name}` from `{KNOWN_COSTS}` in `{NET_EGRESS_SPEC}`, since an admission "
            "nothing uses is a grant decided by nobody",
        )

    declarers = {}
    for crate in crates.values():
        for dependency in crate["deps"]:
            declarers.setdefault(dependency, set()).add(crate["name"])

    for path in sorted(sources):
        if "/tests/" in str(path):
            continue
        owner = crates.get(path.parts[1], {}).get("name", "") if len(path.parts) > 1 else ""
        for text, spans in comment_blocks(sources[path]):
            for match in EXCLUSIVE_DEPENDENCY.finditer(text):
                dependency = match.group(1)
                others = sorted(declarers.get(dependency, set()) - {owner})
                if not others:
                    continue
                number = line_at(spans, match.start())
                excerpt = text[max(0, match.start() - 60) : match.end() + 60]
                yield finding(
                    ERROR,
                    "exclusive-dependency-claim-is-false",
                    "the source claims a dependency is this crate's alone and a manifest in the "
                    "same workspace declares it, so a reviewer is told the exception cannot exist",
                    f"`{path}:{number}` says no other crate depends on `{dependency}`, and "
                    + ", ".join(f"`{one}`" for one in others)
                    + " declares it",
                    "trust",
                    "low",
                    evidence=[f"{path}:{number} {excerpt}"]
                    + [
                        f"{by_name[one]['manifest']} declares `{dependency}`"
                        for one in others
                        if one in by_name
                    ],
                    fix="state the property that holds and point at where the exception is "
                    "recorded, rather than an absolute a reader can check against a manifest and "
                    "find wrong",
                    gain="a reviewer who reads the claim and stops looking, which is what an "
                    "unlisted exception costs",
                )


def paragraphs(lines):
    """Every block between blank lines, joined into one line, with the line it starts at.

    A sentence in a document wraps wherever the margin falls, so a regex over single lines reads
    half of one and a count written either side of a break is invisible.
    """
    start, held = 0, []
    for number, raw in enumerate(lines, start=1):
        if raw.strip():
            if not held:
                start = number
            held.append(raw.strip())
            continue
        if held:
            yield start, " ".join(held)
            held = []
    if held:
        yield start, " ".join(held)


def clause_body(lines, clause):
    """The lines of one clause, from its anchor to the next clause or heading."""
    anchor = f'<a id="{clause}">'
    body = []
    for raw in lines:
        # Past the clause's own title, since that follows the anchor it belongs to.
        if len(body) > 1 and (raw.lstrip().startswith("<a id=") or raw.startswith("#")):
            break
        if body or anchor in raw:
            body.append(raw)
    return body


def table_rows(lines):
    """The data rows of the first table in `lines`, each one its cells.

    The header and the dashes under it are dropped, so what comes back is what the table asserts
    rather than how it is drawn.
    """
    rows = []
    for raw in lines:
        stripped = raw.strip()
        if not stripped.startswith("|"):
            if rows:
                break
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(set(cell) <= set("-: ") for cell in cells):
            continue
        rows.append(cells)
    return rows[1:]


def check_prompt_split():
    """The clause and the register leave the same number of prompts out of a verdict's reach.

    `CHECK-12` is where the split is decided: its table names every prompt a check runs for and says
    of each whether a safe verdict answers it. The third Known cost in `labels.md` is what a
    reviewer meeting a branch on a verdict reads instead, and it counts the prompts the mode does
    not reach.
    Either way the two disagree is a finding. A register short of the clause leaves a live branch
    site unlisted, and `reviewing-for-the-rule.md` makes an unlisted exception indistinguishable
    from a violation, which is how a genuinely new branch passes a review as already admitted. A
    register past the clause is the other half: it admits a prompt the clause says still asks.
    """
    if not VETTING_SPEC.is_file() or not LABELS_SPEC.is_file():
        return

    clause = clause_body(VETTING_SPEC.read_text(encoding="utf-8").split("\n"), PROMPT_SPLIT)
    rows = table_rows(clause)
    answered = [row for row in rows if any(ANSWERED in cell for cell in row)]

    # The first such sentence in the document, as the exception count above is read: there is one,
    # and a second would be making the same claim about the same three prompts.
    counted = None
    for number, paragraph in paragraphs(LABELS_SPEC.read_text(encoding="utf-8").split("\n")):
        found = OUT_OF_REACH.search(paragraph)
        if found:
            counted = (NUMBERS[found.group(1).lower()], number, found.group(0))
            break

    if not rows or not answered or counted is None:
        missing = []
        if not rows:
            missing.append(f"`{VETTING_SPEC}` has no table under {PROMPT_SPLIT}")
        elif not answered:
            missing.append(
                f"no row of {PROMPT_SPLIT}'s table says `{ANSWERED}`, so the cell this reads no "
                "longer says which prompts the mode reaches"
            )
        if counted is None:
            missing.append(f"`{LABELS_SPEC}` counts no prompts a check runs for")
        yield finding(
            WARNING,
            "prompt-split-unstated",
            "the prompts a verdict may answer are counted in only one of the two documents, so "
            "nothing holds them to one split",
            " and ".join(missing),
            "trust",
            "low",
            fix=f"state the split in both: {PROMPT_SPLIT}'s table is where it is decided and the "
            f"third Known cost in `{LABELS_SPEC}` is what a reviewer reads",
        )
        return

    reach = len(rows) - len(answered)
    if counted[0] != reach:
        yield finding(
            ERROR,
            "prompt-split-disagreement",
            "the register and the clause count a different number of prompts out of a verdict's "
            "reach, so an admitted branch and an unlisted one read alike",
            f"{PROMPT_SPLIT} gives a safe verdict {len(answered)} of the {len(rows)} prompts a "
            f"check runs for, leaving {reach} out of reach, and `{LABELS_SPEC}` says {counted[0]}",
            "trust",
            "low",
            evidence=[
                f"{VETTING_SPEC} {PROMPT_SPLIT}: "
                + "; ".join(f"{row[0][:60]} -> {row[-1][:40]}" for row in rows),
                f"{LABELS_SPEC}:{counted[1]} {counted[2][:120]}",
            ],
            fix=f"bring the Known cost to {PROMPT_SPLIT}'s split, which is where it is decided and "
            "which the code follows",
            gain="a branch on a verdict is read against a list that does not describe the code: "
            "short of the clause, it makes a live site look unadmitted and the next unadmitted one "
            "look live; past it, it admits a prompt the clause says still asks",
        )


def check_exhaustive_reader_docs(sources):
    """A field doc claiming to name every reader of the field has to name every reader of it.

    `Read by ... and by nothing else` is the claim, and a reviewer meeting a branch on the field
    reads it rather than grepping for the other sites. A second reader landing without touching the
    doc turns it into a reason to believe the site in front of them is the only one, which is worse
    than no claim at all.

    Read within the file that declares the field, since a field name is not unique in a tree and a
    match on `.name` elsewhere is as likely to be another struct's. Two bounds come with that: a
    reader in another file is out of reach, and two structs in one file sharing a field name are
    read as one. The doc is still where a reader is recorded, in either case.

    Only the claim is checked, not its converse. A name in it that reads nothing here reads
    something in another file as readily as it reads nothing at all, and this cannot tell those
    apart.
    """
    for path, lines in sources.items():
        stripped = None
        for number, raw in enumerate(lines, start=1):
            field = FIELD_DECLARATION.match(raw)
            if not field:
                continue
            doc = []
            index = number - 2
            while index >= 0 and ATTRIBUTE.match(lines[index]):
                index -= 1
            while index >= 0 and DOC_LINE.match(lines[index]):
                doc.append(DOC_LINE.match(lines[index]).group(1).strip())
                index -= 1
            claim = CLAIM.search(" ".join(reversed(doc)))
            if claim is None:
                continue

            named = set(NAMED.findall(claim.group(1)))
            if stripped is None:
                stripped = mechanics.strip_comments(lines)
            reads = re.compile(r"\.\s*" + field.group(1) + r"\b(?!\s*\()")
            unnamed = {}
            for other, text in enumerate(stripped, start=1):
                if other == number or not reads.search(text):
                    continue
                if in_test_module(sources, path, other):
                    continue
                inside = enclosing(stripped, other)
                if inside not in named:
                    unnamed.setdefault(inside, other)
            if not unnamed:
                continue

            yield finding(
                ERROR,
                "reader-doc-incomplete",
                f"the doc on {field.group(1)} says it names every reader of it and does not, so a "
                "second branch on the field reads as the only one",
                f"`{field.group(1)}` in `{path}` is documented as read by "
                + ", ".join(f"`{one}`" for one in sorted(named))
                + " and by nothing else, and is read by "
                + ", ".join(f"`{one}`" for one in sorted(unnamed))
                + " as well",
                "trust",
                "low",
                evidence=[
                    f"{path}:{line} {lines[line - 1].strip()[:120]}"
                    for line in sorted(unnamed.values())
                ],
                fix="name every reader in the doc, or drop the claim that it names them all",
                gain="a reviewer checking one branch against the field's own documentation is told "
                "the other branches do not exist",
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
        outside = outside_the_kernel(sites_for(constructor, sources), sources)
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


def outside_the_kernel(sites, sources):
    """The sites of a symbol that driver code outside `crates/core` writes.

    The kernel is where reading content belongs, and the tests hold the largest concentration of
    every primitive because a test that proves something about a label has to build one. What a
    pin is for is the next site somebody writes in the driver.
    """
    return [
        site
        for site in sites
        if not site["core"]
        and not site["test_file"]
        and not in_test_module(sources, Path(site["path"]), site["line"])
    ]


def function_body(lines, start):
    """The lines of the function beginning at `start`, by brace depth."""
    depth = 0
    body = []
    for raw in lines[start:]:
        body.append(raw)
        depth += raw.count("{") - raw.count("}")
        if "{" in raw and depth <= 0:
            break
    return body


def refuses_before_releasing(body):
    """Whether a body turns untrusted content away before it releases any.

    A refusal below the release is not one: by the time it runs the bytes are already in the
    caller's hands, and reading only whether the name appears anywhere would let a gate be
    exempted from being counted by a line that does nothing.
    """
    refusal = next((number for number, raw in enumerate(body) if REFUSAL in raw), None)
    release = next((number for number, raw in enumerate(body) if RELEASED in raw), None)
    return refusal is not None and (release is None or refusal < release)


def gate_refuses_untrusted(gate, sources):
    """Whether every definition of a gate turns untrusted content away before releasing any.

    `Policy::read_trusted_content` refuses twice over, so every closure it can reach runs on bytes
    the driver was already allowed to read and a spec naming it catches the only thing left, a
    rename. `Policy::render_in_place` has no refusal on the label at all, because releasing
    untrusted content to a closure is what it is for, so for that one a name is not enough.

    Every definition rather than any, since a second function of the name in another module is a
    second way in, and a gate is only as refusing as the one a caller reaches.
    """
    bare = gate.split("::")[-1]
    opens = re.compile(rf"\bfn\s+{re.escape(bare)}\b")
    bodies = []
    for path, lines in sources.items():
        if not str(path).startswith("crates/core/"):
            continue
        code = mechanics.strip_comments(lines)
        for number, raw in enumerate(code):
            if opens.search(raw):
                bodies.append(function_body(code, number))
    return bool(bodies) and all(refuses_before_releasing(body) for body in bodies)


def generic_names(signature):
    """The names inside a function's angle brackets, which are types rather than parameters."""
    found = re.search(r"\bfn\s+[A-Za-z0-9_]+\s*<", signature)
    if not found:
        return set()
    # `->` inside a bound closes nothing, and reading it as a bracket ends the list at the first
    # `FnOnce(T) -> R`, which is the one signature this has to get right.
    text = signature[found.end() - 1 :].replace("->", "  ")
    depth = 0
    held = []
    for char in text:
        if char == "<":
            depth += 1
        elif char == ">":
            depth -= 1
            if depth == 0:
                break
        if depth:
            held.append(char)
    return set(re.findall(r"\b([A-Za-z0-9_]+)\b", "".join(held)))


def closure_parameters(signature):
    """The parameters of a signature whose value is code the caller wrote.

    Three spellings reach the same place: an `impl FnOnce(..)` in the parameter list, a `dyn` one
    behind a pointer, and a generic bound to `Fn` in the brackets or a `where` clause and spelled
    by name where the parameter is declared. Reading only the first would be answered by the
    rewrite anybody would make.
    """
    generics = generic_names(signature)
    names = set()
    for one in CLOSURE_BOUND.findall(signature):
        if one not in generics:
            names.add(one)
            continue
        carries = rf"\b([A-Za-z0-9_]+)\s*:\s*(?:&\s*)?(?:mut\s+)?{re.escape(one)}\s*[,)]"
        names.update(found.group(1) for found in re.finditer(carries, signature))
    return names - generics


def enclosing_signature(lines, index):
    """The function a line sits in, as its name and the closures its caller hands it.

    `enclosing` answers the first half for a list a person reads. This answers the second, which
    takes the parameter list rather than the name: a parameter of closure type is the one way a
    caller's own code runs inside a function it does not own.
    """
    for start in range(index, -1, -1):
        found = FUNCTION.search(lines[start])
        if not found:
            continue
        signature = []
        for raw in lines[start:]:
            signature.append(raw)
            if "{" in raw:
                break
        return found.group(1), closure_parameters(" ".join(signature))
    return None, set()


def call_text(lines, index, gate):
    """A call beginning on this line, to the parenthesis that closes it.

    An argument list is what says which of the function's own parameters the call passes on, and a
    call here is as often written over five lines as over one. String literals are blanked first,
    because a `(` inside one balances nothing and a scan that ran past the call would read the
    rest of the file as its arguments. Nothing comes back for a call this cannot find the end of.
    """
    bare = gate.split("::")[-1]
    blanked = [STRING_LITERAL.sub('""', raw) for raw in lines[index : index + CALL_LINES]]
    column = -1
    for form in (f".{bare}(", f"{gate}("):
        column = blanked[0].find(form)
        if column >= 0:
            break
    if column < 0:
        return ""
    text = []
    depth = 0
    for raw in blanked:
        piece = raw[column:] if not text else raw
        text.append(piece)
        depth += piece.count("(") - piece.count(")")
        if depth <= 0:
            return " ".join(text)
    return ""


def gate_forwarders(gate, sources):
    """Driver-side functions that hand a closure of their own caller's to a gate.

    The count on a gate reaches the line that calls it. It does not reach the closure, and a
    function that takes one from its caller and forwards it is a second front door: every caller
    of it writes code that runs on released content, and none of them moves the gate's count.
    Matching the parameter by name inside the call is what separates such a wrapper from a
    function that happens to take a callback for something else.
    """
    found = []
    seen = set()
    for site in outside_the_kernel(sites_for(gate, sources), sources):
        code = mechanics.strip_comments(sources[Path(site["path"])])
        index = site["line"] - 1
        name, closures = enclosing_signature(code, index)
        if not closures or (site["path"], name) in seen:
            continue
        passed = call_text(code, index, gate)
        if any(re.search(rf"\b{re.escape(one)}\b", passed) for one in closures):
            seen.add((site["path"], name))
            found.append((name, site["path"], site["line"]))
    return found


def counted_files(specs):
    """Each pinned symbol and the files a spec counts its uses in.

    A `guards` entry for a bare name is satisfied by any symbol of that name anywhere, so
    clearing a wrapper on the name alone would let one be exempted by an unrelated entry that
    happens to share it. The file the wrapper is in has to be one the entry counts.
    """
    where = {}
    for spec in specs:
        for symbol, sites in spec.allowlists.items():
            if not isinstance(sites, list):
                continue
            for item in sites:
                head, separator, _ = str(item).rpartition(":")
                where.setdefault(symbol, set()).add(head.strip() if separator else str(item).strip())
    return where


def check_gates_pinned(specs, sources):
    """A release gate no spec counts, and the driver-side wrappers that forward a closure to one.

    `labels.md` pins how many times `Labelled::declassify` is called, per file, so a new release
    cannot land quietly. A gate with no such entry leaves the same hole one level up.
    `Policy::render_in_place` hands raw content to a closure compiled outside the kernel and gives
    back a value that is still labelled, so a new call site needs no release of its own and moves
    no count anywhere: the one gate whose contract only a reader can check is then the one gate
    nothing asks anybody to read. A gate that refuses untrusted content before releasing any is
    the exception, and a spec naming it is enough, because its callers' closures only ever see
    bytes the driver could have read for itself.

    Counting the gate is necessary and not sufficient. A function in the driver that takes a
    closure from its own caller and passes it to a gate hides every later caller from that count,
    so it is counted beside the gate it forwards to.
    """
    pinned = set()
    named = set()
    for spec in specs:
        pinned.update(spec.allowlists)
        named.update(spec.guards)
    counted = counted_files(specs)

    for gate in GATES:
        sites = outside_the_kernel(sites_for(gate, sources), sources)
        if not sites:
            continue
        by_file = {}
        for site in sites:
            by_file[site["path"]] = by_file.get(site["path"], 0) + site["count"]
        if gate not in pinned and not (gate in named and gate_refuses_untrusted(gate, sources)):
            yield finding(
                ERROR,
                "gate-unpinned",
                f"no spec counts {gate}, so a new closure over released content lands green",
                f"no spec pins `{gate}` to a count per file, so a new call site handing released "
                f"content to a closure compiled outside the kernel lands green; there are "
                f"{sum(by_file.values())} outside `crates/core` in non-test code",
                "trust",
                "low",
                evidence=[f"{path} {count} uses" for path, count in sorted(by_file.items())],
                fix=f"add a `guards` entry for `{gate}` to `{LABELS_SPEC}` with a `sites:` count "
                f"per file, the way `Policy::present` already has one. A `sites:` list pins the "
                f"whole tree, so read the counts to pin out of `check-spec` rather than off this "
                f"finding",
                gain="nothing on its own. What it buys is the next closure: one that drops an "
                "entry from a listing, blanks an excerpt, or picks one string over another out of "
                "what the bytes say. Its result reaches the planner's context or a file still "
                "labelled, so nothing downstream reads it again either",
            )
        for name, path, line in gate_forwarders(gate, sources):
            if path in counted.get(name, set()):
                continue
            yield finding(
                ERROR,
                "gate-forwarder-unpinned",
                f"{name} forwards a caller's own closure to {gate}, and no spec counts it",
                f"`{name}` at `{path}:{line}` takes a closure from its caller and hands it to "
                f"`{gate}`, so another caller of it writes a new closure over released content "
                f"while moving no count at all",
                "trust",
                "low",
                evidence=[f"{path}:{line} {name} forwards to {gate}"],
                fix=f"add a `guards` entry for `{name}` to `{LABELS_SPEC}` with a `sites:` count "
                f"per file, or inline it into its callers so that the gate's own count reaches "
                f"them",
                gain="the same as the unpinned gate, reached through a helper the count does not "
                "see: the closure is written by the caller and runs on content the gate released",
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


# The other half of pinning a `uses:` step: which ref the step is pointed at when the step is this
# repository checking itself out. Handed a ref that names no kind, `actions/checkout` looks for a
# remote branch of that name and takes a tag of it only when there is none (`src/ref-helper.ts`,
# `getCheckoutInfo`), so a name a branch and a tag can both carry resolves to whichever exists,
# decided by what has been pushed rather than by anything in this tree. `publish-npm.yml` takes
# that name from a dispatch input and publishes the tree the checkout produced.
CHECKOUT = re.compile(r"^[\w.-]+/checkout(?:@|$)")
REF = re.compile(r"^\s*ref:\s*(.*?)\s*$")
STEP_START = re.compile(r"^(\s*)-\s+\S")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
# A ref written as an expression cannot be read here, so it is read by name. Anything with `sha` in
# it is taken for a commit, which is not a ref at all and so never reaches the branch-before-tag
# resolution. The cost is an input actually called `sha` passing; the other direction, reporting
# every workflow that rebuilds `github.sha`, would be noise nobody could act on.
NAMES_A_COMMIT = re.compile(r"sha\b", re.IGNORECASE)


def enclosing_step(lines, number):
    """The lines of the sequence entry a line sits in, or nothing where it is in none.

    A step is a `-` entry under `steps:`, so the entry begins at the nearest `-` above this line
    indented less than it, and ends at the next line indented no further than that `-`. Walking up
    rather than tracking `steps:` downwards is what makes the order of a step's keys irrelevant: a
    `with:` written above its `uses:` is the same block either way.
    """
    column = indented(lines[number])
    for index in range(number - 1, -1, -1):
        raw = lines[index]
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        found = STEP_START.match(raw)
        if found and len(found.group(1)) < column:
            return [raw] + nested(lines, index, base=len(found.group(1)))
        if indented(raw) == 0:
            break
    return []


def check_checkout_ref_is_qualified():
    """A checkout of this tree names a kind of ref, not a name a branch and a tag can share.

    `refs/tags/v0.9.0` is one object. `v0.9.0` is whichever of a branch and a tag of that name
    exists, and where both do it is the branch, because that is the order the action resolves in.
    The workflow that publishes to npm is handed that name by whoever dispatches it, and every
    refusal after the checkout reads either the checked out tree or the GitHub release of the tag,
    so a branch of the version's name satisfies all of them and the registry gets a tree nobody
    reviewed under a version somebody released.

    A step with no `ref:` is not read: it takes the commit that triggered the run, which is a
    commit rather than a name. What this cannot read is a `ref:` inside an inline mapping, since
    the rest of this half is line-oriented too; no workflow here writes one.
    """
    if not WORKFLOWS.is_dir():
        return
    for path in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        lines = path.read_text(encoding="utf-8").split("\n")
        for number, raw in enumerate(lines):
            found = REF.match(raw)
            if not found:
                continue
            step = [USES.match(line) for line in enclosing_step(lines, number)]
            if not any(one and CHECKOUT.match(unquoted(one.group(1))) for one in step):
                continue
            value = unquoted(found.group(1))
            if value.startswith("refs/") or COMMIT.match(value) or NAMES_A_COMMIT.search(value):
                continue
            yield finding(
                ERROR,
                "unqualified-checkout-ref",
                f"{path.name} checks out `{value}`, a name a branch and a tag can share, so a "
                "branch decides what this run reads",
                f"`{path}` checks out `{value}`, which names no kind of ref, and `actions/checkout` "
                "prefers a remote branch of that name over a tag of it, so what the run reads is "
                "whichever of the two has been pushed",
                "infrastructure",
                "high",
                evidence=[f"{path}:{number + 1} {raw.strip()[:120]}"],
                fix="write the kind: `refs/tags/` for a release, `refs/heads/` for a branch. A "
                "qualified ref fetches that one ref, so a dispatch naming a tag that does not "
                "exist fails the run rather than resolving to something else",
                gain="whoever can push a branch to this repository chooses the tree a dispatch of "
                "the tag of that name reads, and in the publish workflow that tree is what reaches "
                "the registry under the released version",
            )


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


# The other thing that runs with this tree inside it. A container image on a tag is whatever its
# publisher points at today, exactly as an action on a tag is, and the pass above skips `docker://`
# for the narrow reason that an image is not a `uses:` step, so until this nothing read one at all.
# The cross-build compiles every shipped binary with the whole checkout at /src and the configured
# credential mounted, and `make strip` rewrites each finished asset inside the same image, both
# before anything is signed. A digest names bytes. A tag names whoever can push to it.
DOCKER_COMMAND = re.compile(r"\bdocker\s+(?:run|create|pull)\b")
FROM_LINE = re.compile(r"^\s*FROM\s+(.+?)\s*$", re.IGNORECASE)
COPY_FROM = re.compile(r"^\s*COPY\s+.*?--from=(\S+)", re.IGNORECASE)
ARG_DEFAULT = re.compile(r"^\s*ARG\s+([A-Za-z_][A-Za-z0-9_]*)=(\S+)\s*$", re.IGNORECASE)
YAML_IMAGE = re.compile(r"^\s*image:\s*([^\s#]+)")
BY_DIGEST = re.compile(r"@sha256:[0-9a-f]{64}$")
ASSIGNMENT = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\s*[:?]?=\s*(.*?)\s*$")
# Directories that name no image this tree decides, and that walking costs minutes. `.claude` holds
# the generated discovery links and, under `worktrees`, whole second checkouts of this repository:
# an image named in one of those is another commit's to pin, so reporting it names a file nothing
# here can fix and fails the check on a clean tree.
NOT_WALKED = {".git", ".claude", "target", "node_modules", "dist", ".venv"}
# The `docker run` options that take the token after them as their value. An option missing from
# this set is read as taking none, so its value is reported as an unpinned image: a false report
# somebody answers by adding the option here. The other direction -- reading past the image and
# finding nothing to report -- is the failure this check exists so as not to have.
TAKES_A_VALUE = {
    "--add-host", "--cap-add", "--cap-drop", "--device", "--entrypoint", "--env", "--env-file",
    "--label", "--memory", "--mount", "--name", "--network", "--platform", "--publish",
    "--security-opt", "--tmpfs", "--ulimit", "--user", "--volume", "--volumes-from", "--workdir",
    "-e", "-l", "-m", "-p", "-u", "-v", "-w",
}


def joined_lines(text):
    """A file's lines with backslash continuations joined, each keeping the number it starts on.

    A `docker run` in a Makefile recipe puts its options on one line and its image on the next, so a
    pass that reads a line at a time reads the options and never the image.
    """
    start, held = None, ""
    for number, raw in enumerate(text.split("\n"), start=1):
        if start is None:
            start, held = number, ""
        if raw.endswith("\\"):
            held += raw[:-1] + " "
            continue
        yield start, held + raw
        start = None
    if start is not None:
        yield start, held


def tokens_of(line):
    """A shell line split into tokens, as far as the lexer can read it.

    Quoting matters twice over: `-v "$(PWD):/src:ro"` is one argument, and a split on whitespace
    makes the token after `-v` something other than that option's value, which moves what is read as
    the image on to the token after that. A line the lexer cannot finish is returned as far as it
    got, and an image past that point is `None` to the caller rather than absent from it.
    """
    lexer = shlex.shlex(line, posix=True)
    lexer.whitespace_split = True
    lexer.commenters = ""
    found = []
    try:
        for token in lexer:
            found.append(token)
    except ValueError:
        pass
    return found


def image_argument(tokens):
    """The image a `docker run` names: the first token that is neither an option nor one's value."""
    at = 0
    while at < len(tokens):
        token = tokens[at]
        if token in TAKES_A_VALUE:
            at += 2
            continue
        if token.startswith("-"):
            at += 1
            continue
        return token
    return None


def images_run(line, values):
    """Every image a line's `docker run`, `docker create` or `docker pull` names, and `None` for one
    that could not be read.

    Read from each command rather than from the start of the line, so a command inside a quoted
    script -- `sh -c 'docker run ...'` -- is read too, where a pass over the line's own tokens sees
    that whole script as one token and never looks inside it.

    A reference with no tag and no registry path is the image the build itself produced a line or
    two earlier, and there is nothing upstream of it to pin. A reference assembled by a `$(shell
    ...)` is the limit of this pass: it is left alone rather than run.
    """
    found = []
    for match in DOCKER_COMMAND.finditer(line):
        image = image_argument(tokens_of(line[match.end():]))
        if image is not None:
            image = expanded(unquoted(image), values)
            if "$" in image and ":" not in image and "/" not in image:
                continue
        found.append(image)
    return found


def workflow_images(line):
    """The image a workflow names outside a `run:` block: a job or service container, and the step
    form `check_pinned_actions` passes over because an image is not a `uses:` step."""
    named = YAML_IMAGE.match(line)
    if named:
        yield unquoted(named.group(1))
    step = USES.match(line)
    if step and unquoted(step.group(1)).startswith("docker://"):
        yield unquoted(step.group(1))[len("docker://"):]


def unquoted(token):
    return token.strip("\"'")


def make_values(text):
    """The `NAME = value` assignments of a Makefile, which is how an image reference in one can be
    written. A value that is computed rather than written -- a `$(shell ...)` -- is left alone,
    because resolving it means running it."""
    values = {}
    for raw in text.split("\n"):
        found = ASSIGNMENT.match(raw)
        if found and "$(shell" not in found.group(2):
            values[found.group(1)] = found.group(2)
    return values


def expanded(reference, values):
    """A reference with the variables it is written through substituted in.

    Longest name first, so `$BASE` does not rewrite the front of `$BASE_IMAGE`.
    """
    for _ in range(3):
        before = reference
        for name in sorted(values, key=len, reverse=True):
            for form in (f"$({name})", "${%s}" % name, f"${name}"):
                reference = reference.replace(form, values[name])
        if reference == before:
            break
    return reference


def image_files():
    """Every file that can name an image: the Dockerfiles, the Makefile, and the workflows."""
    for root, directories, names in os.walk("."):
        directories[:] = sorted(
            one for one in directories
            if one not in NOT_WALKED and not (Path(root) / one / ".git").exists()
        )
        for name in sorted(names):
            if name.startswith("Dockerfile") or name == "Makefile":
                yield Path(root) / name
    for path in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        yield path


def dockerfile_images(text):
    """`(number, reference)` for each image a Dockerfile pulls: what its stages are built on, and
    what a `COPY --from` reaches into.

    `scratch` is the empty image and has no bytes to name, a name this file declared with `AS` is a
    stage of this same build, and `--from=0` is one by position. Everything else is pulled, and a
    base written through an `ARG` is read through the default that `ARG` declares rather than passed
    over for having a `$` in it: parameterising the base is how one stops being written down.
    """
    stages, values = set(), {}
    for number, line in joined_lines(text):
        default = ARG_DEFAULT.match(line)
        if default:
            values[default.group(1)] = default.group(2)
        copied = COPY_FROM.match(line)
        if copied:
            reference = expanded(copied.group(1), values)
            if reference not in stages and not reference.isdigit():
                yield number, reference
            continue
        found = FROM_LINE.match(line)
        if not found:
            continue
        words = [one for one in found.group(1).split() if not one.startswith("--")]
        if not words:
            continue
        if len(words) >= 3 and words[1].upper() == "AS":
            stages.add(words[2])
        if words[0] == "scratch" or words[0] in stages:
            continue
        yield number, expanded(words[0], values)


def check_pinned_images():
    """Every container image this tree runs names a digest, not a tag its publisher can move.

    The same question `check_pinned_actions` asks, about the other thing that runs with the tree in
    it. `Dockerfile.cross` compiles every shipped binary with the checkout at `/src` and the
    configured credential mounted, and `make strip` rewrites each finished asset inside a second
    container, both before Jenkins signs what comes out; the three check targets run a third with
    the tree mounted. An image on a tag is a decision left to whoever can push that tag.
    """
    for path in image_files():
        text = path.read_text(encoding="utf-8")
        if path.name.startswith("Dockerfile"):
            found = list(dockerfile_images(text))
        else:
            found = []
            values = make_values(text) if path.name == "Makefile" else {}
            for number, line in joined_lines(text):
                # A comment naming a command is prose about it, not a command.
                if line.lstrip().startswith("#"):
                    continue
                found.extend((number, one) for one in images_run(line, values))
                if path.is_relative_to(WORKFLOWS):
                    found.extend((number, one) for one in workflow_images(line))
        for number, reference in found:
            if reference is not None and BY_DIGEST.search(reference):
                continue
            named = f"`{reference}`" if reference else "an image this pass could not read"
            yield finding(
                ERROR,
                "unpinned-image",
                f"{path.name} runs {reference or 'an image nothing can read'} unpinned, so its "
                "publisher decides what runs with this tree",
                f"{named} is not pinned to a digest, so whoever can push that tag decides what "
                "runs with the whole checkout inside it",
                "infrastructure",
                "high",
                evidence=[f"{path}:{number}"],
                fix="name the digest as well as the tag, `image:tag@sha256:<64 hex>`, the way "
                "every workflow step here already names a commit as well as a version",
            )


def job_display_names(lines):
    """Every job in a workflow by the name its check run reports under, with the targets it runs.

    A required context matches a check run's name, which is the job's `name:` where it has one and
    its key where it does not. A name built from an expression is one no checkout can resolve --
    the cross-build names its six legs from a matrix -- so it is left out rather than guessed at,
    and a context could not name it either.
    """
    found = {}
    for job in workflow_jobs(lines):
        block = nested(lines, job["line"] - 1)
        written = next((one.group(1) for one in map(DISPLAY_NAME.match, block) if one), job["name"])
        if not written.startswith(("'", '"')):
            written = TRAILING_COMMENT.sub("", written).strip()
        display = unquoted(written)
        if EXPRESSION.search(display):
            continue
        targets = set()
        for step in job["steps"]:
            for raw in step["body"]:
                for command in MAKE.findall(raw):
                    targets.update(CHECK_TARGET.findall(command))
        found.setdefault(display, set()).update(targets)
    return found


def required_contexts():
    """The contexts `contrib/required-checks.txt` names, or nothing where the file is gone."""
    if not REQUIRED_CHECKS.is_file():
        return None
    return [
        line
        for line in (raw.strip() for raw in REQUIRED_CHECKS.read_text(encoding="utf-8").split("\n"))
        if line and not line.startswith("#")
    ]


def gated_targets():
    """The make targets `checks.md` promises will fail a pull request, by the line it promises on.

    A paragraph rather than the whole document, because the promise and the target it is about are
    written together: `make check-locales` is named in four paragraphs and only one of them says
    what CI does with it.
    """
    if not CHECKS_DOC.is_file():
        return {}
    found = {}
    number = 1
    for paragraph in CHECKS_DOC.read_text(encoding="utf-8").split("\n\n"):
        flat = " ".join(paragraph.split())
        if any(one in flat for one in GATES_A_PULL_REQUEST):
            for target in DOC_TARGET.findall(paragraph):
                found.setdefault(target, number)
        number += paragraph.count("\n") + 2
    return found


def check_required_checks_are_the_gates():
    """The contexts a merge is held to are written down, and they name jobs that exist.

    A red job stops nothing by itself. What stops a merge is branch protection's list of required
    contexts, which is a repository setting: no file in a checkout can read it, and a job renamed
    here leaves that list naming a check run nothing produces -- always green, because it never
    reports. This repository has one of those already, since the required lower-case `security` is
    the organisation's scan and not CI's `Security`, and the cost is a check the documentation says
    fails a pull request sitting red above an enabled merge button.

    So the intended list is a file, and this decides the three things about it a checkout can: that
    every context named is a job that exists, that every job running a `make check-` target is
    named, since a job whose whole purpose is a check and which nothing requires reports its
    verdict beside an enabled merge button, and that a target the documentation promises will fail
    a pull request is run by some job at all. Comparing the file against what the protection really
    requires is the part that needs the network, and `contrib/required-checks.txt` says how.

    What this does not read is when a job runs. A check target moved into a job that runs only on a
    push or a schedule would be demanded here and never report on a pull request, which holds every
    merge rather than letting one through, so it is a state whoever made it finds out about at once.
    """
    contexts = required_contexts()
    if contexts is None:
        yield finding(
            ERROR,
            "unrecorded-required-check",
            f"`{REQUIRED_CHECKS}` is gone, so nothing in the tree says which checks hold a merge",
            f"`{REQUIRED_CHECKS}` is the only statement anywhere in this repository of which check "
            "contexts branch protection has to require; without it a job can be renamed, or a "
            "check added, with nothing to compare the protection against",
            "infrastructure",
            "medium",
            evidence=[f"{REQUIRED_CHECKS}: absent"],
            fix="restore the file, listing the display name of every job a merge is held to",
        )
        return

    jobs = {}
    if WORKFLOWS.is_dir():
        for path in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
            lines = path.read_text(encoding="utf-8").split("\n")
            for display, targets in job_display_names(lines).items():
                jobs.setdefault(display, set()).update(targets)

    for context in contexts:
        if context in jobs:
            continue
        yield finding(
            ERROR,
            "orphaned-required-check",
            f"no job is called `{context}`, so requiring it requires a check run nothing produces",
            f"`{REQUIRED_CHECKS}` names `{context}`, which is the display name of no job in "
            "`.github/workflows/`. A required context that nothing reports is not a gate: it is "
            "pending forever, or it is satisfied by whatever else happens to report under that "
            "name",
            "infrastructure",
            "high",
            evidence=[f"{REQUIRED_CHECKS}: {context}"],
            fix="spell it exactly as the job's `name:`, or drop it where the job is gone. A job "
            "renamed on one side and not the other is how a required context comes to name "
            "something else",
        )

    promised = gated_targets()
    for display in sorted(jobs):
        targets = sorted(jobs[display])
        if not targets or display in contexts:
            continue
        yield finding(
            ERROR,
            "ungated-check",
            f"`{display}` runs `make {targets[0]}` and is not a context a merge is held to, so it "
            "reports its verdict beside an enabled merge button",
            f"`{display}` exists to run {', '.join(f'`make {one}`' for one in targets)}. "
            f"`{REQUIRED_CHECKS}` does not name it, and a check nothing requires decides nothing: "
            "the job goes red and the merge button stays enabled"
            + (
                f". `{CHECKS_DOC}` promises that `make {targets[0]}` fails a pull request"
                if targets[0] in promised
                else ""
            ),
            "infrastructure",
            "medium",
            evidence=[f"{display} runs make {one}" for one in targets]
            + [f"{CHECKS_DOC}:{promised[one]}" for one in targets if one in promised],
            fix=f"add `{display}` to `{REQUIRED_CHECKS}` and require the context, or move the "
            "target out of a job of its own where it is not meant to hold a merge",
        )

    run_somewhere = {one for targets in jobs.values() for one in targets}
    for target, number in sorted(promised.items()):
        if target in run_somewhere:
            continue
        yield finding(
            ERROR,
            "unrun-promise",
            f"`{CHECKS_DOC.name}` says `make {target}` fails a pull request, and no job runs it",
            f"`{CHECKS_DOC}` promises that `make {target}` fails a pull request rather than "
            "holding only for whoever remembers to run it, and no job in `.github/workflows/` "
            "runs that target, so nothing runs it on a pull request at all",
            "infrastructure",
            "medium",
            evidence=[f"{CHECKS_DOC}:{number}"],
            fix="give the target a job and require its context, or amend the paragraph to say "
            "what actually runs it",
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
        one.startswith(".github/")
        or Path(one).name.startswith("Dockerfile")
        or one.endswith(("Makefile", "Cargo.toml", "Cargo.lock", "deny.toml"))
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
    findings += list(check_network_clients_are_recorded(sources))
    findings += list(check_prompt_split())
    findings += list(check_labelled_impls(sources))
    findings += list(check_exhaustive_reader_docs(sources))
    findings += list(check_construction_pinned(specs, sources))
    findings += list(check_gates_pinned(specs, sources))
    findings += list(check_key_sites_exhaustive(specs, sources))
    findings += list(check_pinned_actions())
    findings += list(check_pinned_images())
    findings += list(check_privileged_job_runs_only_its_own_code())
    findings += list(check_checkout_ref_is_qualified())
    findings += list(check_required_checks_are_the_gates())
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
