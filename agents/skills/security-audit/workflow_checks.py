"""The checks on what runs in CI and what this tree pulls in: workflow steps, images and downloads.

Split from security-audit.py, which shares only `finding`, `ERROR` and `REPRODUCE` with them and
calls them from `mechanical_findings`.
"""

import os
import re
import shlex
from pathlib import Path

from findings import ERROR, finding

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
# Either `environment: name` or the mapping form with `name:` beneath it.
ENVIRONMENT = re.compile(r"^\s*environment:")
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

# Who owns what, and the files a workflow step runs by name. A step runs the copy of a file in the
# checkout it is in, which for a pull request is the pull request's own, so a file a step runs that
# CODEOWNERS leaves out can be rewritten under two ordinary approvals.
CODEOWNERS = Path(".github/CODEOWNERS")
RUNS_A_FILE = re.compile(r"(?:^|[\s;&|(])(?:python3?|bash|sh|node)\s+([\w./-]+)")
RUNS_MAKE = re.compile(r"(?:^|[\s;&|(])make\b")

# What a job can hold that is worth stealing, and the commands that run bytes nobody here wrote. The
# grant is a permission to request a token rather than a token, which is the whole of the difference:
# the runner hands every step in the job the two variables that turn it into one.
#
# Every spelling of the grant, because the check is worth no more than the narrowest one it reads:
# a block, an inline map, a quoted value, and `write-all`, which grants this along with the rest.
GRANT = re.compile(r"""id-token\s*:\s*["']?write\b|\bwrite-all\b""")
SECRET = re.compile(r"\bsecrets\.[A-Za-z_]")
# A command is read as the manager, then any options, then the subcommand, because `npm --prefix ui ci`
# and `pnpm --dir ui install` are the same commands as the ones without the option, and a pattern that
# needs the subcommand next to the manager misses them.
PACKAGE_MANAGER = re.compile(r"(?:^|[\s;&|(])(npm|pnpm|yarn)(?=\s|$)([^;&|()#]*)")
# A bare `pnpm build` runs the script `build`. Only a manager at the start of a command is read that
# way, so prose such as `pnpm and the cache` in a step name or an echo is not.
COMMAND_START = re.compile(r"(?:^|[;&|(]\s*|\brun:\s*|\bsudo\s+|\bcorepack\s+)$")
# Options whose next token is their value, not the subcommand. `-w` is a flag for pnpm (the workspace
# root) and a value for npm, so it is not listed.
OPTIONS_WITH_A_VALUE = {"--prefix", "--dir", "-C", "--filter", "-F", "--workspace", "--cwd"}
INSTALLS = {"ci", "install", "i", "add", "rebuild", "rb", "approve-builds"}
RUNS = {"run", "run-script", "exec", "x", "dlx", "test", "t", "start"}
# pnpm subcommands that run no package code. Any other word after pnpm is a script name.
PNPM_RUNS_NOTHING = {
    "publish", "pack", "config", "c", "get", "set", "view", "audit", "outdated", "list", "ls", "ll",
    "why", "root", "bin", "store", "env", "setup", "self-update", "licenses", "deprecate",
    "unpublish", "patch", "patch-commit", "remove", "rm", "uninstall", "un", "unlink", "prune",
    "link", "ln", "version", "help", "update", "up", "upgrade", "dedupe", "fetch", "import",
}
# What a Rust job runs that executes bytes nobody here wrote: cargo compiles every build script and
# proc macro in Cargo.lock and runs them, and the tests after them. `contrib/measure-flakes.py` and
# `contrib/measure-mutants.py run` are the scripts known to do it, and the first does not when it is
# given `--load`, which is its filing mode.
CARGO_RUNS = re.compile(
    r"(?:^|[\s;&|(])cargo(?:\s+\+\S+)?\s+(?:build|b|test|t|clippy|run|r|nextest|install|check|c|bench|doc)\b"
)
MEASURES_FLAKES = re.compile(r"(?:^|[\s;&|(])(?:\S*/)?contrib/measure-flakes\.py\b(?![^;&|\n]*--load)")
# `contrib/measure-mutants.py` builds and tests the crate once per change in its `run` mode, and reads
# files in its `report` mode.
MEASURES_MUTANTS = re.compile(r"(?:^|[\s;&|(])(?:\S*/)?contrib/measure-mutants\.py\s+run\b")
WRITE_GRANT = re.compile(r"\bwrite\b|\bwrite-all\b")
NOT_PERSISTED = re.compile(r"persist-credentials\s*:\s*[\"']?false\b")
DEPENDENCY = (
    (
        re.compile(
            r"(?:^|[\s;&|(])(?:npx|pnpx)(?=\s|$)|(?:^|[\s;&|(])\.?/?node_modules/\.bin/"
        ),
        "runs a dependency",
    ),
)


def package_manager_verb(tool, rest, at_command_start):
    """What one `npm`, `pnpm` or `yarn` command does with dependencies, or None if nothing."""
    tokens = rest.split()
    index = 0
    while index < len(tokens) and tokens[index].startswith("-"):
        index += 2 if tokens[index] in OPTIONS_WITH_A_VALUE else 1
    if index >= len(tokens):
        return None
    word = tokens[index]
    if word in INSTALLS:
        return "installs a dependency"
    if word in RUNS:
        return "runs a dependency"
    if (
        tool == "pnpm"
        and at_command_start
        and word not in PNPM_RUNS_NOTHING
        and re.fullmatch(r"[A-Za-z0-9:_-]+", word)
    ):
        return "runs a dependency"
    return None


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
                "environment": False,
                "secret": everywhere,
                "secret_above_steps": everywhere,
                "persists": False,
                "in_steps": False,
                "steps": [],
            }
            jobs.append(current)
            continue
        if current is None:
            continue
        if column == 4 and raw.strip() == "steps:":
            current["in_steps"] = True
        if SECRET.search(raw):
            current["secret"] = True
            if not current["in_steps"]:
                current["secret_above_steps"] = True
        found = USES.match(raw)
        if found and CHECKOUT.match(unquoted(found.group(1))):
            if not any(NOT_PERSISTED.search(line) for line in step_text(lines, number)):
                current["persists"] = True
        found = PERMISSIONS.match(raw) if column == 4 else None
        if found:
            current["grants"] = [found.group(1)] + nested(lines, number)
        if column == 4 and ENVIRONMENT.match(raw):
            current["environment"] = True
        found = RUN.match(raw)
        if found:
            inline = found.group(1).strip()
            current["steps"].append(
                {
                    "line": number + 1,
                    "body": nested(lines, number, base=raw.index("run:"))
                    if inline in BLOCK
                    else [inline],
                    "secret": any(SECRET.search(line) for line in step_text(lines, number)),
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


def step_text(lines, number):
    """The lines of the step a line is in, including a line that is the `- key:` opening it."""
    found = STEP_START.match(lines[number])
    if found:
        return [lines[number]] + nested(lines, number, base=len(found.group(1)))
    return enclosing_step(lines, number)


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
        verbs = [
            package_manager_verb(
                match.group(1), match.group(2), bool(COMMAND_START.search(line[: match.start(1)]))
            )
            for match in PACKAGE_MANAGER.finditer(line)
        ]
        verbs += [verb for pattern, verb in DEPENDENCY if pattern.search(line)]
        verbs = [verb for verb in verbs if verb]
        if verbs:
            verb = "installs a dependency" if "installs a dependency" in verbs else verbs[0]
            found.append((verb, line[:120]))
    return found


def runs_project_code(path, job, granted):
    """The steps of one job that compile or run this repository's dependencies beside a credential.

    Reported for a step that runs `cargo build`, `test`, `clippy`, `run`, `nextest` or `install`, or
    `contrib/measure-flakes.py` in its measuring mode (#2789) or `contrib/measure-mutants.py run`,
    when what it can read is a token:

    - `id-token: write` on the job, which every step is handed the means to use;
    - a secret named by the step itself, or by the job above its steps, or the workflow above its
      jobs, since each of those is in the step's environment. A secret in another step of the job is
      not, which is what lets one job measure in one step and hold the token in none of the steps
      that run code;
    - a write grant on a job whose `actions/checkout` leaves its credential in the checkout, because
      the checkout is what every later step reads and `persist-credentials: false` is the only way
      the token is not in it.
    """
    write = any(WRITE_GRANT.search(raw) for raw in job["grants"])
    for step in job["steps"]:
        ran = [
            line.strip()[:120]
            for line in step["body"]
            if not line.strip().startswith("#")
            and (CARGO_RUNS.search(line) or MEASURES_FLAKES.search(line) or MEASURES_MUTANTS.search(line))
        ]
        if not ran:
            continue
        if granted:
            holds = "id-token: write"
        elif step["secret"] or job["secret_above_steps"]:
            holds = "a secret in its environment"
        elif write and job["persists"]:
            holds = "a write grant its checkout leaves in the working tree"
        else:
            continue
        yield finding(
            ERROR,
            "privileged-job-runs-project-code",
            f"{path.name} runs the build or the tests in {job['name']}, a step that holds {holds}, "
            "so a build script, proc macro or test of any crate in Cargo.lock can use it",
            f"`{job['name']}` in `{path}` runs `{ran[0]}` where {holds} is readable, and cargo "
            "runs the build script and proc macro of every crate in `Cargo.lock` and then their "
            "tests, in the environment and working tree of that step",
            "infrastructure",
            "medium",
            evidence=[f"{path}:{job['line']} job `{job['name']}`", f"{path}:{step['line']} {ran[0]}"],
            fix="measure in a step or job that holds no token and writes what it found to a file, "
            "and file from a job that holds the grant and runs only `gh`. Set "
            "`persist-credentials: false` on the checkout either way",
            gain="whoever owns one crate in the lockfile reads the token and acts as the workflow "
            "on this repository for as long as the job runs",
        )


def check_privileged_job_runs_only_its_own_code():
    """A job that can mint a credential installs and runs nothing from `node_modules`.

    A grant or a secret is readable by every step of the job holding it, so a job is the smallest
    boundary either has. A step that installs from `pnpm-lock.yaml` and then runs what it
    installed executes bytes nobody here wrote, deliberately: that is what a lint is. In a job
    holding `id-token: write` those bytes can exchange the runner's OIDC token for a publishing
    credential at the registry and ship a tarball with this repository's provenance on it, and the
    same two commands in a job holding `contents: read` reach a green check and nothing else. So the
    finding is the pairing rather than either half.

    It reads the npm and pnpm tree, and the Rust one: a job that compiles the tree runs a crate's
    build script in the same environment, so `runs_project_code` pairs `cargo` commands and
    `contrib/measure-flakes.py` with the same credentials (#2789).
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
            yield from runs_project_code(path, job, granted)
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


def check_grant_is_behind_an_environment():
    """A job that can mint an OIDC token names an `environment:`, so a reviewer stands in front of it.

    `workflow_dispatch` runs the file at the ref it is dispatched on, and the registry's trusted
    publisher keys on the repository and the filename, not the ref. A person with write access can
    push a branch holding a copy of the publish workflow without its refusals and dispatch that, and
    the token it mints is accepted. An `environment:` is the one control the dispatcher's copy of the
    file cannot remove for the real one to keep: the environment's required reviewer is a setting of
    the repository, and the registry compares the environment claim once the job declares one.

    Only the key is read. That the environment has a reviewer, and that npmjs.com names it, are
    settings of the repository and the registry that no checkout holds, which is recorded in
    `docs/development/branch-protection.md`.
    """
    if not WORKFLOWS.is_dir():
        return
    for path in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        for job in workflow_jobs(path.read_text(encoding="utf-8").split("\n")):
            if not any(GRANT.search(raw) for raw in job["grants"]) or job["environment"]:
                continue
            yield finding(
                ERROR,
                "grant-without-environment",
                f"{path.name} gives {job['name']} id-token: write and no environment, so no "
                "reviewer stands between a writer and the credential",
                f"`{job['name']}` in `{path}` can request an OIDC token and names no "
                "`environment:`, so a dispatch of a branch's copy of this file mints a token the "
                "registry or cloud account accepts, with nothing waiting for an approval",
                "infrastructure",
                "medium",
                evidence=[f"{path}:{job['line']} job `{job['name']}` holds id-token: write"],
                fix="add `environment: <name>` to the job and give that GitHub environment a "
                "required reviewer who is not the person dispatching. Name the environment on the "
                "registry's trusted-publisher record too, and record both in "
                "`docs/development/branch-protection.md`",
                gain="whoever can push a branch to this repository publishes under this "
                "repository's identity from a copy of the workflow nobody reviewed",
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
    it. `contrib/Dockerfile.cross` compiles every shipped binary with the checkout at `/src` and the
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


DOWNLOAD = re.compile(r"\b(curl|wget)\b")
# What turns downloaded bytes into code that runs: piped straight into an interpreter or unpacker,
# or later made executable or unpacked.
DOWNLOAD_PIPED = re.compile(r"\|\s*(sudo\s+)?(tar|unzip|sh|bash|zsh|python3?|ruby|node)\b")
DOWNLOAD_RUN = re.compile(r"\bchmod\s+(-\w+\s+)?(\+\w*x|[0-7]*[1357][0-7]{0,2}\b)|\b(tar|unzip)\s+-?[xz]")
DOWNLOAD_COMPARED = re.compile(r"sha256|sha512|shasum|dgst|matches_pin|cosign|gpg\s+--verify|minisign")


def download_files():
    """The scripts under `contrib/` and the Makefile: what a person or the bot runs on a checkout."""
    yield from sorted(Path("contrib").glob("*.sh"))
    if Path("Makefile").is_file():
        yield Path("Makefile")


def check_downloads_verified():
    """Nothing fetched with `curl` or `wget` is made executable or unpacked without a digest check.

    The tree pins its actions to commits and its images to digests so that whoever owns a tag cannot
    decide what runs here. A release asset fetched by tag in a helper script is the same decision
    left to the same person, and the script runs as whoever invokes it. A download piped into an
    interpreter or `tar` has nowhere to compare anything, so it is always a finding; one written to
    a file is a finding unless a comparison appears between the fetch and the `chmod` or unpacking.
    """
    for path in download_files():
        lines = [
            (number, line) for number, line in joined_lines(path.read_text(encoding="utf-8"))
            if not line.lstrip().startswith("#")
        ]
        for index, (number, line) in enumerate(lines):
            fetch = DOWNLOAD.search(line)
            if not fetch:
                continue
            if DOWNLOAD_PIPED.search(line[fetch.end():]):
                unverified = "is piped into a program that runs it"
            else:
                unverified = None
                for _, later in lines[index + 1:]:
                    if DOWNLOAD_COMPARED.search(later):
                        break
                    if DOWNLOAD_RUN.search(later):
                        unverified = "is made executable or unpacked with no digest compared first"
                        break
                if unverified is None:
                    continue
            yield finding(
                ERROR,
                "unverified-download",
                f"{path.name} fetches a file that {unverified}, so whoever can replace it decides "
                "what runs as the invoking user",
                f"the download at {path}:{number} {unverified}, and nothing fixes its content: a "
                "release tag can be re-pointed by its owner",
                "infrastructure",
                "high",
                evidence=[f"{path}:{number}"],
                fix="pin the sha256 of the asset in the script and compare it before `chmod` or "
                "unpacking, failing closed on a mismatch or a missing pin",
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


def code_owner_patterns():
    """CODEOWNERS as (regex, owners) in file order, where the last match wins, or None if absent."""
    if not CODEOWNERS.is_file():
        return None
    rules = []
    for raw in CODEOWNERS.read_text(encoding="utf-8").split("\n"):
        words = raw.split("#")[0].split()
        if not words:
            continue
        pattern = words[0]
        body = re.escape(pattern.strip("/")).replace(r"\*\*", ".*").replace(r"\*", "[^/]*")
        head = "" if pattern.startswith("/") or "/" in pattern.rstrip("/") else "(?:.*/)?"
        tail = "(?:/.*)?"
        rules.append((re.compile(f"^{head}{body}{tail}$"), words[1:]))
    return rules


def owners_of(path, rules):
    """The owners of `path` under the last rule that matches it, or none."""
    owners = []
    for pattern, who in rules:
        if pattern.match(path):
            owners = who
    return owners


def files_workflows_run():
    """Each file a workflow step runs by path, with where it is named: `python3 <path>`, and the
    Makefile wherever a step calls `make`."""
    found = {}
    if not WORKFLOWS.is_dir():
        return found
    for path in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        for number, raw in enumerate(path.read_text(encoding="utf-8").split("\n"), start=1):
            if raw.lstrip().startswith("#"):
                continue
            for named in RUNS_A_FILE.findall(raw):
                if Path(named).is_file():
                    found.setdefault(named, f"{path}:{number}")
            if RUN.match(raw) and RUNS_MAKE.search(raw) and Path("Makefile").is_file():
                found.setdefault("Makefile", f"{path}:{number}")
    return found


def check_run_files_have_owners():
    """Every file a workflow step runs by path has a code owner.

    The two approvals every pull request needs are the whole of review for a path nobody owns.
    A file a required job runs is code that decides what the job reports, and the classifier that
    decides which required jobs run at all is the sharpest case: a pull request that edits it to
    report every area unaffected makes seven required contexts skip as passing, and owns no path.
    `.github/workflows/` and `contrib/required-checks.txt` were owned for that reason while the
    files the same steps run were not.

    Only a file a step names is read, so a script a recipe calls in turn is not. That is the
    Makefile's entry to cover, which is why `make` counts as naming it.
    """
    rules = code_owner_patterns()
    if rules is None:
        yield finding(
            ERROR,
            "unrecorded-code-owners",
            f"`{CODEOWNERS}` is gone, so no path needs a code owner's approval",
            f"`{CODEOWNERS}` is the only statement of which paths a merge needs an owner for",
            "infrastructure",
            "medium",
            evidence=[f"{CODEOWNERS}: absent"],
            fix="restore the file",
        )
        return
    for path, where in sorted(files_workflows_run().items()):
        if owners_of(path, rules):
            continue
        yield finding(
            ERROR,
            "unowned-workflow-file",
            f"`{path}` is run by a workflow and has no code owner, so a pull request can rewrite "
            "what a required check does under two ordinary approvals",
            f"`{where}` runs `{path}` from the pull request's own checkout, and `{CODEOWNERS}` "
            "has no entry for it, so a change to it needs no code owner's review while the "
            "workflows that call it do",
            "infrastructure",
            "medium",
            evidence=[f"{where} runs {path}", f"{CODEOWNERS}: no entry for {path}"],
            fix=f"add `/{path}` to `{CODEOWNERS}` beside `/.github/workflows/`",
        )
