#!/usr/bin/env python3
"""Review bravebot's diagnostic log for defects in bravebot, and file an issue for each.

    python3 agents/skills/log-review/log-review.py pending [--max N] [--idle MINUTES]
        [--logs-dir DIR] [--state-dir DIR] [LOG-NAME ...]
    python3 agents/skills/log-review/log-review.py verify --work-dir DIR
    python3 agents/skills/log-review/log-review.py draft --work-dir DIR
    python3 agents/skills/log-review/log-review.py filed --work-dir DIR --id ID --issue N [--existing]
    python3 agents/skills/log-review/log-review.py record --work-dir DIR [--dry-run]

The unit is one log file under <state dir>/logs, written by crates/diag
(docs/specs/diagnostic-log.md). A log is reviewed once: `record` appends a line to
<state dir>/log-review/<log name>.analysis.jsonl, and `pending` offers a log again only when it has
grown since. The marker is not in logs/, because retention deletes only files it made and a marker
there would outlive its log. `pending` removes the marker of a log that no longer exists.

Every value on a log line is a host, a number or a fixed word (DIAG-1). This script reads a log file
for two things only: to copy it into the work directory, and to count what it holds into
DIR/digest.json. The digest holds numbers and fixed words, and a host appears in it only as a
pseudonym (host-1, host-2) that is not written anywhere else. A line that does not parse is counted
and never copied. No model output decides anything this script does, and no line of a log is ever
printed. The draft step removes every host found in the copied logs from every field a model wrote.

The issue drafting and the `filed` command are shared with the peer-advisories skill and the
helpers of session-review.py. This script files nothing: each draft in DIR/drafts.json is handed to
the `issue-poster` definition (agents/agents/issue-poster.md), and `filed` records the issue number
it was given in DIR/filed.json, which `record` reads.
"""

import argparse
import datetime
import importlib.util
import json
import os
import re
import shutil
import stat
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def _load_session_review():
    spec = importlib.util.spec_from_file_location("session_review", HERE.parent / "session-review" / "session-review.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The helpers shared with session-review and peer-advisories: prompt rendering, the tracker listing,
# the commit check, title and draft cleaning, verdict reading and the `filed` command.
sr = _load_session_review()
pa = sr.pa
workdir = pa.workdir
Problem = pa.Problem
one_line = pa.one_line

MAX = 8
IDLE_MINUTES = 30
MAX_PROBLEMS = 8
MARKER_DIR = "log-review"
MARKER_SUFFIX = ".analysis.jsonl"
SUMMARY_LIMIT = 600
HOST_PLACEHOLDER = "<host>"
MAX_LINE = 2000

# crates/diag/src/lib.rs `is_log_name`: YYYYMMDDTHHMMSSZ-<pid>.log, ASCII digits only.
LOG_NAME = re.compile(r"[0-9]{8}T[0-9]{6}Z-[0-9]+\.log")

# A line is `<timestamp> <LEVEL> <event> key=value ...` (crates/diag/src/lib.rs `format_line`).
LINE = re.compile(
    r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}Z (ERROR|INFO|DEBUG) ([a-z][a-z_.]{0,39})"
    r"((?: [a-z_]{1,24}=[A-Za-z0-9._:\[\]?-]{0,253})*)"
)
WORD = re.compile(r"[A-Za-z][A-Za-z0-9_-]{0,31}")
# The events the program writes: net.fetch, client_pair_reread and client_pair_reread_failed in
# crates/net, aichat.retry and aichat.stream_failure in crates/aichat, bedrock.retry in
# crates/bedrock, mcp.* in crates/mcp, lsp.* in crates/lsp, window.fault in crates/ui-bridge. A
# line naming another event is counted as `other`, so the digest holds no event name a file chose.
EVENTS = (
    "net.fetch",
    "client_pair_reread",
    "client_pair_reread_failed",
    "aichat.retry",
    "aichat.stream_failure",
    "bedrock.retry",
    "mcp.launch",
    "mcp.initialize",
    "mcp.list_tools",
    "mcp.list_prompts",
    "lsp.launch",
    "lsp.exit",
    "window.fault",
)
START_EVENTS = ("lsp.launch", "mcp.launch", "mcp.initialize", "mcp.list_tools", "mcp.list_prompts")
RETRY_EVENTS = ("aichat.retry", "bedrock.retry")
NUMBER_KEYS = ("transient", "elapsed_ms", "status", "attempt", "backoff_ms", "exit")
WORD_KEYS = ("kind", "event", "code", "outcome", "how", "language", "reason", "file", "changed", "name", "process")
KEYS = ("host",) + NUMBER_KEYS + WORD_KEYS
KIND_KEYS = ("kind", "outcome", "how", "reason", "changed", "event")


# Where the logs and the markers are.


def state_dir():
    """The state directory: BRAVEBOT_HOME where set, else ~/.bravebot (docs/specs/state-directory.md)."""
    home = os.environ.get("BRAVEBOT_HOME")
    return Path(home) if home and os.path.isabs(home) else Path.home() / ".bravebot"


def is_log_name(name):
    return LOG_NAME.fullmatch(str(name)) is not None


def list_logs(directory):
    """The regular files in `directory` named the way crates/diag names a log, by name."""
    found = {}
    try:
        names = sorted(os.listdir(directory))
    except OSError:
        return found
    for name in names:
        if not is_log_name(name):
            continue
        path = Path(directory) / name
        try:
            info = path.lstat()
        except OSError:
            continue
        if stat.S_ISREG(info.st_mode):
            found[name] = {"name": name, "path": str(path), "size": info.st_size, "mtime": info.st_mtime}
    return found


def marker_path(markers, name):
    return Path(markers) / f"{name}{MARKER_SUFFIX}"


def read_marker(markers, name):
    """The newest review line recorded for a log, or None."""
    try:
        lines = marker_path(markers, name).read_text(encoding="utf-8").splitlines()
    except OSError:
        return None
    for line in reversed(lines):
        try:
            entry = json.loads(line)
        except ValueError:
            continue
        if isinstance(entry, dict) and isinstance(entry.get("size"), int) and not isinstance(entry.get("size"), bool):
            return entry
    return None


def write_marker(markers, name, entry):
    """Append one line to the log's marker file. The directory is 0700 and the file 0600."""
    markers = Path(markers)
    if markers.is_symlink():
        raise Problem("the marker directory is a link")
    markers.mkdir(mode=0o700, parents=True, exist_ok=True)
    descriptor = os.open(marker_path(markers, name), os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
    if os.name == "posix":
        os.chmod(markers, 0o700)
        os.fchmod(descriptor, 0o600)
    with os.fdopen(descriptor, "a", encoding="utf-8") as out:
        out.write(json.dumps(entry, sort_keys=True) + "\n")


def prune_markers(markers, existing):
    """Remove the marker of a log that no longer exists. Returns how many were removed."""
    try:
        entries = os.listdir(markers)
    except OSError:
        return 0
    removed = 0
    for entry in entries:
        if not entry.endswith(MARKER_SUFFIX):
            continue
        name = entry[: -len(MARKER_SUFFIX)]
        if is_log_name(name) and name not in existing:
            try:
                (Path(markers) / entry).unlink()
                removed += 1
            except OSError:
                pass
    return removed


# Choosing.


def due(logs, markers, now, idle):
    """The logs a default run would review, newest first, before the limit is applied.

    A log modified within `idle` seconds may still be written and is passed over. A log reviewed
    before is due only when it is larger than the size its marker records.
    """
    left = []
    for name in sorted(logs, reverse=True):
        log = logs[name]
        if log["size"] < 1 or now - log["mtime"] < idle:
            continue
        marker = read_marker(markers, name)
        if marker is not None and not log["size"] > marker["size"]:
            continue
        left.append(log)
    return left


def select(logs, markers, named, limit, now, idle):
    """The logs to review: the ones named, whatever the marker says, else the unreviewed or grown."""
    if named:
        chosen = []
        for name in dict.fromkeys(named):
            if name not in logs:
                raise Problem(f"no log {name}")
            chosen.append(logs[name])
        return chosen
    return due(logs, markers, now, idle)[:limit]


def plan(logs, markers, chosen, args, now, idle):
    """One line saying how many logs this run reviews, from what was asked, and how many stay for later runs."""
    left = {log["name"] for log in due(logs, markers, now, idle)} - {log["name"] for log in chosen}
    if args.names:
        asked = "the ones you named"
    elif args.max != MAX:
        asked = f"up to {args.max} as you asked"
    else:
        asked = f"up to {MAX}, the default"
    reviewing = f"reviewing {len(chosen)} log{'s' if len(chosen) != 1 else ''} ({asked})"
    remain = f"{len(left)} more due will remain for future runs" if left else "none more due will remain"
    return f"{reviewing}; {remain}"


# The digest: what code can count from the logs, so a model is not asked to.


class Pseudonyms:
    """host-1, host-2 in the order hosts are first seen in one run. The map is never written."""

    def __init__(self):
        self.names = {}

    def __call__(self, host):
        if host == "?":
            return "host-unknown"
        return self.names.setdefault(host, f"host-{len(self.names) + 1}")


def parse_line(line):
    """(level, event, fields) for a well-formed line, or None. Nothing of the line is kept but what the patterns allow."""
    if len(line) > MAX_LINE:
        return None
    found = LINE.fullmatch(line)
    if found is None:
        return None
    level, event = found.group(1), found.group(2)
    fields = {}
    for token in found.group(3).split():
        key, _, value = token.partition("=")
        if key not in KEYS:
            continue
        if key == "host":
            fields[key] = value
        elif key in NUMBER_KEYS:
            if not (value.isascii() and value.isdigit()) or len(value) > 18:
                return None
            fields[key] = int(value)
        else:
            if WORD.fullmatch(value) is None:
                return None
            fields[key] = value
    return level, event if event in EVENTS else "other", fields


def kind_of(fields):
    for key in KIND_KEYS:
        if key in fields:
            return str(fields[key])
    return "none"


def status_class(fields):
    status = fields.get("status")
    return f"{status // 100}xx" if isinstance(status, int) else "none"


def build_digest(files, pseudonym=None):
    """Counts over every log in `files`, a list of (name, path). Holds numbers, fixed words and host pseudonyms."""
    pseudonym = pseudonym or Pseudonyms()
    events, retries, hosts, starts, per_file = {}, {}, {}, {}, []
    total = unparsed_total = 0
    for name, path in sorted(files):
        try:
            text = Path(path).read_text(encoding="utf-8", errors="replace")
        except OSError:
            per_file.append({"file": name, "lines": 0, "unparsed": 0, "unreadable": True, "levels": {}, "events": {}})
            continue
        row = {"file": name, "lines": 0, "unparsed": 0, "levels": {}, "events": {}}
        for line in text.split("\n"):
            if not line:
                continue
            row["lines"] += 1
            parsed = parse_line(line)
            if parsed is None:
                row["unparsed"] += 1
                continue
            level, event, fields = parsed
            kind, status = kind_of(fields), status_class(fields)
            row["levels"][level] = row["levels"].get(level, 0) + 1
            row["events"][event] = row["events"].get(event, 0) + 1
            entry = events.setdefault((event, kind, status), {"files": set(), "count": 0})
            entry["files"].add(name)
            entry["count"] += 1
            if "host" in fields:
                host = hosts.setdefault(pseudonym(fields["host"]), {"files": set(), "count": 0})
                host["files"].add(name)
                host["count"] += 1
            if event in RETRY_EVENTS and "attempt" in fields:
                retry = retries.setdefault(event, {"files": set(), "count": 0, "attempts": {}})
                retry["files"].add(name)
                retry["count"] += 1
                retry["attempts"][fields["attempt"]] = retry["attempts"].get(fields["attempt"], 0) + 1
            if event in START_EVENTS:
                start = starts.setdefault(event, {"ok": 0, "failed": {}})
                if level == "ERROR":
                    failed = start["failed"].setdefault(kind, {"files": set(), "count": 0})
                    failed["files"].add(name)
                    failed["count"] += 1
                elif fields.get("outcome") == "ok":
                    start["ok"] += 1
        total += row["lines"]
        unparsed_total += row["unparsed"]
        per_file.append(row)

    def counted(table):
        return {k: {"files": len(v["files"]), "count": v["count"]} for k, v in sorted(table.items())}

    return {
        "note": "Numbers and fixed words only. A host is a pseudonym that holds for this run. A line that did not parse is counted and not copied.",
        "files": len(per_file),
        "lines": total,
        "unparsed": unparsed_total,
        "events": [
            {"event": e, "kind": k, "status": s, "files": len(v["files"]), "count": v["count"]}
            for (e, k, s), v in sorted(events.items(), key=lambda item: (-len(item[1]["files"]), -item[1]["count"], item[0]))
        ],
        "retries": {
            event: {
                "files": len(v["files"]),
                "count": v["count"],
                "max_attempt": max(v["attempts"]),
                "lines_at_max_attempt": v["attempts"][max(v["attempts"])],
                "lines_per_attempt": {str(a): n for a, n in sorted(v["attempts"].items())},
            }
            for event, v in sorted(retries.items())
        },
        "server_starts": {
            event: {"ok": v["ok"], "failed_by_kind": counted(v["failed"])} for event, v in sorted(starts.items())
        },
        "hosts": counted(hosts),
        "per_file": per_file,
    }


# Pending.


def pending(args, now=None):
    root = Path(args.root)
    ref, tip = pa.checked_commit(root, ledger="", verb="review", worktree="log-review")
    state = Path(args.state_dir) if args.state_dir else state_dir()
    logs_dir = Path(args.logs_dir) if args.logs_dir else state / "logs"
    if not logs_dir.is_dir():
        raise Problem(f"no logs directory at {logs_dir}")
    markers = state / MARKER_DIR
    logs = list_logs(logs_dir)
    # Markers are removed only against the state directory's own logs, so a log directory named for
    # a look at some other machine's files cannot remove the markers of this one.
    pruned = 0
    if logs_dir.resolve() == (state / "logs").resolve():
        pruned = prune_markers(markers, set(logs))
    now = time.time() if now is None else now
    idle = args.idle * 60
    chosen = select(logs, markers, args.names, args.max, now, idle)
    print(plan(logs, markers, chosen, args, now, idle), file=sys.stderr)
    reviewed = sum(1 for name in logs if read_marker(markers, name) is not None)
    if not chosen:
        print(f"{len(logs)} logs found, {reviewed} reviewed before, none to review; {pruned} markers removed", file=sys.stderr)
        print(json.dumps({"work_dir": None, "analyze": []}))
        return 0

    work = Path(tempfile.mkdtemp(prefix="log-review-"))
    for name in ("logs", "analyze", "verify"):
        (work / name).mkdir()
    tracker_file = work / "tracker.tsv"
    try:
        count = pa.tracker(tracker_file)
    except (RuntimeError, ValueError) as problem:
        raise Problem(f"could not list the issues on {pa.REPO}: {one_line(problem, 160)}") from problem

    entries = []
    for log in chosen:
        copy = work / "logs" / log["name"]
        try:
            shutil.copyfile(log["path"], copy)
            os.chmod(copy, 0o600)
        except OSError:
            print(f"warning: {log['name']} could not be copied, skipped", file=sys.stderr)
            continue
        entries.append({"name": log["name"], "size": copy.stat().st_size, "copy": str(copy)})
    digest = build_digest([(e["name"], e["copy"]) for e in entries])
    digest_file = work / "digest.json"
    digest_file.write_text(json.dumps(digest, indent=2), encoding="utf-8")

    prompt, result = work / "analyze" / "review.md", work / "analyze" / "review.json"
    prompt.write_text(
        pa.render(
            (HERE / "analyze.md").read_text(encoding="utf-8"),
            {
                "review_id": work.name,
                "file_count": str(len(entries)),
                "file_list": ", ".join(f"`{e['name']}`" for e in entries),
                "logs_dir": str(work / "logs"),
                "digest_file": str(digest_file),
                "max_problems": str(MAX_PROBLEMS),
                "tracker": str(tracker_file),
                "root": str(root),
                "commit": tip,
                "ref": ref,
                "results_file": str(result),
            },
        ),
        encoding="utf-8",
    )
    manifest = {
        "root": str(root),
        "ref": ref,
        "commit": tip,
        "review": work.name,
        "markers": str(markers),
        "logs": entries,
        "prompt_file": str(prompt),
        "results_file": str(result),
    }
    workdir.write_manifest(work, manifest)
    print(
        f"{len(logs)} logs found, {reviewed} reviewed before, {len(entries)} to review against {ref} {tip[:12]};"
        f" {digest['lines']} lines, {digest['unparsed']} unparsed; {count} issues and pull requests listed;"
        f" {pruned} markers removed",
        file=sys.stderr,
    )
    print(json.dumps({"work_dir": str(work), "analyze": [{"id": "review", "prompt_file": str(prompt)}]}))
    return 0


# Reading what the agents wrote.


def problem_id(number):
    return f"p{number}"


def check_problem(problem, names):
    """Why this problem cannot be drafted, or None."""
    if not isinstance(problem, dict):
        return "not an object"
    missing = [field for field in sr.PROBLEM_TEXT if not str(problem.get(field) or "").strip()]
    if missing:
        return f"no {', '.join(missing)}"
    if problem.get("kind") not in sr.KINDS:
        return f"kind {problem.get('kind')!r} is not among {', '.join(sr.KINDS)}"
    files = problem.get("files")
    if not isinstance(files, list) or not files or not all(isinstance(f, str) for f in files):
        return "files is not a list of log names"
    if any(f not in names for f in files):
        return "files names a log that was not reviewed"
    if not isinstance(problem.get("security"), bool):
        return "security is not true or false"
    return None


def load_analysis(path, review):
    data = pa.read_json(path)
    if data is None:
        return None, "no readable result"
    if data.get("review") != review:
        return None, "the result names another review"
    problems = data.get("problems")
    if not isinstance(problems, list):
        return None, "problems is not a list"
    if len(problems) > MAX_PROBLEMS:
        return None, f"{len(problems)} problems, more than the {MAX_PROBLEMS} a review may report"
    return data, None


def outcomes(work):
    """{state, reason, problems} for the one analysis of this run, each problem {id, state, ...}.

    A problem is `file` once a verifier confirmed it and nothing on the tracker holds it, `final`
    when a verifier dropped it or the tracker holds it, `held` when it is flagged security and so is
    never drafted, `verify` while no verifier has answered, and `pending` when what the analyzer
    wrote for it cannot be drafted. The review is `pending` when its result cannot be read.
    """
    manifest = pa.manifest_of(work)
    names = {e["name"] for e in manifest["logs"]}
    analysis, why = load_analysis(manifest["results_file"], manifest["review"])
    if analysis is None:
        return manifest, {"state": "pending", "reason": why, "problems": []}
    problems = []
    for number, problem in enumerate(analysis["problems"], 1):
        pid = problem_id(number)
        why = check_problem(problem, names)
        if why:
            problems.append({"id": pid, "state": "pending", "reason": why})
        elif problem["security"]:
            problems.append({"id": pid, "state": "held", "title": problem["title"], "files": len(problem["files"])})
        else:
            check, why = sr.load_verify(sr.verify_paths(work, pid)[1], pid)
            if check is None:
                problems.append({"id": pid, "state": "verify", "reason": why, "problem": problem})
            elif check["verdict"] != "confirmed" or check.get("existing_issue") is not None:
                problems.append(
                    {"id": pid, "state": "final", "verdict": check["verdict"], "issue": check.get("existing_issue"), "reason": check["reason"]}
                )
            else:
                problems.append({"id": pid, "state": "file", "fields": problem, "reason": check["reason"]})
    return manifest, {"state": "read", "reason": one_line(analysis.get("summary"), SUMMARY_LIMIT), "problems": problems}


def verify(args):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    verifiers = []
    if decided["state"] == "pending":
        print(f"  review  undecided: {decided['reason']}", file=sys.stderr)
    template = (HERE / "verify.md").read_text(encoding="utf-8")
    for problem in decided["problems"]:
        if problem["state"] != "verify":
            print(f"  {problem['id']}  {problem.get('verdict') or problem['state']}", file=sys.stderr)
            continue
        prompt, result = sr.verify_paths(work, problem["id"])
        claim = {k: problem["problem"].get(k) for k in ("files", "kind", "title", "what_happens", "code_path", "reproduce", "evidence")}
        prompt.write_text(
            pa.render(
                template,
                {
                    "review_id": manifest["review"],
                    "problem_id": problem["id"],
                    "claim": pa.fenced(json.dumps(claim, indent=2)),
                    "logs_dir": str(work / "logs"),
                    "digest_file": str(work / "digest.json"),
                    "tracker": str(work / "tracker.tsv"),
                    "root": manifest["root"],
                    "commit": manifest["commit"],
                    "results_file": str(result),
                },
            ),
            encoding="utf-8",
        )
        verifiers.append({"id": problem["id"], "prompt_file": str(prompt)})
        print(f"  {problem['id']}  to verify", file=sys.stderr)
    print(json.dumps({"work_dir": str(work), "verify": verifiers}))
    return 0


# Drafting.


def hosts_in(work):
    """Every host value in the logs copied into `work`, longest first, with and without a port."""
    found = set()
    for path in sorted((Path(work) / "logs").glob("*.log")):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for host in re.findall(r"(?<![A-Za-z0-9_])host=(\S+)", text):
            if host == "?":
                continue
            found.add(host)
            bare = re.sub(r":[0-9]+$", "", host)
            if bare != host and bare:
                found.add(bare)
    return sorted(found, key=lambda h: (-len(h), h))


def scrub(text, manifest, hosts):
    """A draft's wording with every host of the reviewed logs and this machine's home directory removed."""
    text = str(text)
    for path, name in ((str(Path.home()), "~"), (manifest["markers"], "<state>")):
        if path and path != "/":
            text = text.replace(path, name)
    for host in hosts:
        text = re.sub(r"(?<![A-Za-z0-9_-])" + re.escape(host) + r"(?![A-Za-z0-9_-])", HOST_PLACEHOLDER, text, flags=re.IGNORECASE)
    return pa.clean(text, manifest["root"])


def labels_for(fields):
    """The kind, and `area/*` where the area is one the tracker has. No `session-analysis`, no axis."""
    labels = [fields["kind"]]
    area = str(fields.get("area") or "").strip().removeprefix("area/")
    if area in pa.AREAS:
        labels.append(f"area/{area}")
    return sorted(set(labels))


def body_for(fields, pid, manifest, hosts):
    def text(field):
        return scrub(fields[field], manifest, hosts)

    seen, reviewed = len(set(fields["files"])), len(manifest["logs"])
    sections = [
        text("what_happens"),
        f"## Where\n\n{text('code_path')}",
        f"## Reproduce\n\n{text('reproduce')}",
        f"## Fix\n\n{text('fix')}",
        "## Where this comes from\n\n"
        f"Found in {seen} of {reviewed} diagnostic log files reviewed by the `log-review` skill "
        f"(review id {manifest['review']}, problem {pid}). A second pass checked it against "
        f"`{manifest['commit'][:12]}` and could not disprove it. The logs hold hosts, numbers and fixed "
        "words, and none of them is quoted here. Filed by a tool rather than a person, so no `importance`, "
        "`urgency` or `size` has been judged.",
    ]
    return "\n\n".join(sections) + "\n"


def draft(args):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    hosts = hosts_in(work)
    out = work / "issues"
    out.mkdir(exist_ok=True)
    drafts, held, waiting = [], [], []
    if decided["state"] == "pending":
        waiting.append("review")
    for problem in decided["problems"]:
        if problem["state"] == "held":
            held.append(problem)
        elif problem["state"] in ("verify", "pending"):
            waiting.append(problem["id"])
        elif problem["state"] == "file":
            fields, pid = problem["fields"], problem["id"]
            body = out / f"{pid}.md"
            body.write_text(body_for(fields, pid, manifest, hosts), encoding="utf-8")
            title = pa.title_for(dict(fields, title=scrub(fields["title"], manifest, hosts)))
            drafts.append(
                {
                    "id": pid,
                    "title": title,
                    "labels": labels_for(fields),
                    "key": one_line(fields.get("key"), 80) if one_line(fields.get("key")).lower() in title.lower() else "",
                    "body_file": str(body),
                }
            )
            print(f"  {pid}  {title}\n{'':8}{', '.join(labels_for(fields))}")
    (work / "drafts.json").write_text(json.dumps(drafts, indent=2), encoding="utf-8")
    for problem in held:
        print(f"  held   {problem['id']}  in {problem['files']} logs: {one_line(scrub(problem['title'], manifest, hosts), 200)}")
    print(f"{len(drafts)} drafted, {len(held)} held back as security concerns" + (f", {len(waiting)} undecided: {', '.join(waiting)}" if waiting else ""))
    return 0


# Recording.


def record(args, today=None):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    filed = pa.read_json(work / "filed.json") or {}
    today = today or datetime.date.today().isoformat()
    issues, dropped, held, open_ = [], 0, 0, []
    for problem in decided["problems"]:
        if problem["state"] == "held":
            held += 1
        elif problem["state"] == "final":
            dropped += problem.get("issue") is None
            if problem.get("issue") is not None:
                issues.append(problem["issue"])
        elif problem["state"] == "file" and problem["id"] in filed:
            issues.append(filed[problem["id"]]["issue"])
        else:
            open_.append(problem["id"])
    if decided["state"] == "pending" or open_:
        why = decided["reason"] if decided["state"] == "pending" else f"{', '.join(open_)} not settled"
        print(f"  left   {len(manifest['logs'])} logs  {why}")
        print(f"marked 0 logs reviewed, {len(manifest['logs'])} left for the next run")
        return 0
    verb = "would mark" if args.dry_run else "marked"
    for log in manifest["logs"]:
        line = {
            "date": today,
            "commit": manifest["commit"][:12],
            "size": log["size"],
            "problems": len(decided["problems"]),
            "issues": sorted(set(issues)),
            "dropped": dropped,
            "held": held,
        }
        if not args.dry_run:
            write_marker(Path(manifest["markers"]), log["name"], line)
        print(f"  {verb} {log['name']}  {len(decided['problems'])} problems, issues {line['issues'] or '-'}, {held} held")
    print(f"{verb} {len(manifest['logs'])} logs reviewed, 0 left for the next run")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)

    one = commands.add_parser("pending", help="choose the logs to review, copy them and write the digest and a prompt")
    one.add_argument("names", nargs="*", metavar="LOG-NAME", help="review these, whatever their markers say")
    one.add_argument("--max", type=int, default=MAX)
    one.add_argument("--idle", type=int, default=IDLE_MINUTES, help="skip a log modified within this many minutes")
    one.add_argument("--logs-dir", help="the logs directory (default <state directory>/logs)")
    one.add_argument("--state-dir", help="the state directory (default $BRAVEBOT_HOME or ~/.bravebot)")
    one.add_argument("--root", default=str(ROOT))

    for name, text in (("verify", "write a verifier prompt per problem"), ("draft", "write an issue body per confirmed problem")):
        commands.add_parser(name, help=text).add_argument("--work-dir", required=True)

    pa.add_filed_command(commands)

    one = commands.add_parser("record", help="mark the settled logs as reviewed")
    one.add_argument("--work-dir", required=True)
    one.add_argument("--dry-run", action="store_true")

    args = parser.parse_args(argv)
    for name in getattr(args, "names", []):
        if not is_log_name(name):
            parser.error(f"{name!r} is not the name of a log file")
    try:
        return {"pending": pending, "verify": verify, "draft": draft, "filed": pa.filed, "record": record}[args.command](args)
    except Problem as problem:
        print(f"log-review: {problem}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
