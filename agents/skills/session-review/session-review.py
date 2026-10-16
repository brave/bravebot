#!/usr/bin/env python3
"""Review recorded bravebot sessions for the problems they hit, and file an issue for each.

    python3 agents/skills/session-review/session-review.py pending [--max N] [--idle MINUTES]
        [--workspace DIR] [SESSION-ID ...]
    python3 agents/skills/session-review/session-review.py verify --work-dir DIR
    python3 agents/skills/session-review/session-review.py draft --work-dir DIR
    python3 agents/skills/session-review/session-review.py filed --work-dir DIR --id ID --issue N [--existing]
    python3 agents/skills/session-review/session-review.py record --work-dir DIR [--dry-run]

Sessions are the records under ~/.bravebot/sessions, one directory per working directory. A session
is reviewed once: `record` appends a line to `<id>.analysis.jsonl` beside the session, and `pending`
offers a session again only when it has had more turns or more notes since. A note is a line the
person typed with `/note` (docs/specs/sessions.md SESSION-36). It is written to a file beside the
record copy and named to the analyzer and verifier as data from the person, never put in a prompt.
The marker is a local file because
sessions are per machine. Its name does not end in `.json`, because the session store reads every
`.json` file in a directory as a session record.

No model takes part in anything this script does. The issue drafting and the `filed` command are
shared with the peer-advisories skill. This script files nothing: each draft in DIR/drafts.json is
handed to the `issue-poster` definition (agents/agents/issue-poster.md), and `filed` records the
issue number it was given in DIR/filed.json, which `record` reads.
"""

import argparse
import datetime
import importlib.util
import json
import os
import re
import shutil
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def _load_peer():
    spec = importlib.util.spec_from_file_location("peer_advisories", HERE.parent / "peer-advisories" / "peer-advisories.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# Prompt rendering, the tracker listing, label and title helpers, the draft cleaning and the `filed`
# command are peer-advisories.py, which this reuses rather than restating.
pa = _load_peer()
workdir = pa.workdir
Problem = pa.Problem
one_line = pa.one_line

MAX = 8
IDLE_MINUTES = 30
MAX_PROBLEMS = 5
MARKER_SUFFIX = ".analysis.jsonl"
# Sessions imported from another tool say nothing about bravebot.
IMPORTED = "claude-code-"
SESSION_ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*")

KINDS = ("bug", "enhancement")
SESSION_ANALYSIS = "session-analysis"
PROBLEM_TEXT = ("title", "what_happens", "code_path", "reproduce", "fix", "evidence")
VERIFY_VERDICTS = ("confirmed", "fixed", "known", "not-a-defect")


# Where sessions are.


def sessions_root():
    return Path.home() / ".bravebot" / "sessions"


def key_for(project):
    """The directory name for a working directory: crates/agent/src/home.rs `key_for`."""
    mangled = "".join(c if re.fullmatch(r"[A-Za-z0-9._]", c) else "-" for c in str(project))
    return mangled if mangled.strip("-") else "root"


def read_marker(directory, session):
    """The newest analysis line recorded for a session, or None."""
    try:
        lines = (directory / f"{session}{MARKER_SUFFIX}").read_text(encoding="utf-8").splitlines()
    except OSError:
        return None
    for line in reversed(lines):
        try:
            entry = json.loads(line)
        except ValueError:
            continue
        if isinstance(entry, dict) and isinstance(entry.get("turns"), int):
            return entry
    return None


def write_marker(directory, session, entry):
    """Append one line to the session's marker file, created readable by this user only."""
    descriptor = os.open(directory / f"{session}{MARKER_SUFFIX}", os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
    with os.fdopen(descriptor, "a", encoding="utf-8") as out:
        out.write(json.dumps(entry, sort_keys=True) + "\n")


def notes_of(record):
    """The notes a person typed on a session, numbered from 1 in the order they were added.

    Only the shape is checked: an entry without text is not a note. What a note says is never read
    by this script beyond that, and reaches no prompt except through the file it is written to.
    """
    found = []
    for entry in (record or {}).get("notes") or []:
        if not isinstance(entry, dict) or not isinstance(entry.get("text"), str) or not entry["text"].strip():
            continue
        turn = entry.get("turn")
        found.append(
            {
                "number": len(found) + 1,
                "turn": turn if isinstance(turn, int) and not isinstance(turn, bool) else 0,
                "by": entry.get("by") if isinstance(entry.get("by"), str) else "person",
                "text": entry["text"],
            }
        )
    return found


def safe_build(value):
    text = str(value or "")
    return text if re.fullmatch(r"[\w.+ ()/,-]{1,80}", text) else "unknown"


def candidates(store, workspace=None):
    """Every session record under the store, as dicts. Files that are not session records are skipped."""
    if workspace is not None:
        directories = [store / key_for(workspace)]
    else:
        directories = sorted(p for p in store.iterdir() if p.is_dir()) if store.is_dir() else []
    found = []
    for directory in directories:
        try:
            paths = sorted(directory.glob("*.json"))
        except OSError:
            continue
        for path in paths:
            record = pa.read_json(path)
            if record is None or not SESSION_ID.fullmatch(path.stem) or not isinstance(record.get("updated"), int):
                print(f"warning: {path.name} is not a session record, skipped", file=sys.stderr)
                continue
            found.append(
                {
                    "id": path.stem,
                    "directory": str(directory),
                    "record": str(path),
                    "audit": str(directory / f"{path.stem}.audit.jsonl"),
                    "turns": record["turns"] if isinstance(record.get("turns"), int) else 0,
                    "notes": len(notes_of(record)),
                    "updated": record["updated"],
                    "build": safe_build(record.get("build")),
                    "workspace": str(record.get("directory") or ""),
                    "marker": read_marker(directory, path.stem),
                }
            )
    return found


def select(found, named, limit, now, idle):
    """The sessions to review: the ones named, else the unreviewed or grown, newest first.

    A named session is reviewed whatever its marker says and from its first turn and first note,
    since naming one is how a session is reviewed again. Otherwise a session that is imported, has
    no turns, or was written within `idle` seconds (so may still be running) is passed over, and
    one reviewed before is offered only when it has more turns or more notes than the review
    considered, for the turns and notes after those.
    """
    if named:
        chosen = []
        for name in dict.fromkeys(named):
            exact = [s for s in found if s["id"] == name]
            matches = exact or [s for s in found if s["id"].startswith(name)]
            if not matches:
                raise Problem(f"no session {name}")
            if len(matches) > 1:
                raise Problem(f"{name} matches {len(matches)} sessions; give more of the id")
            if matches[0]["turns"] < 1:
                raise Problem(f"session {matches[0]['id']} has had no turns")
            chosen.append(dict(matches[0], from_turn=0, from_note=0))
        return chosen

    left = []
    for session in found:
        marker = session["marker"]
        if session["id"].startswith(IMPORTED) or session["turns"] < 1 or now - session["updated"] < idle:
            continue
        if marker is not None and session["turns"] <= marker["turns"] and session.get("notes", 0) <= marker.get("notes", 0):
            continue
        left.append(
            dict(
                session,
                from_turn=marker["turns"] if marker else 0,
                from_note=marker.get("notes", 0) if marker else 0,
            )
        )
    left.sort(key=lambda s: s["updated"], reverse=True)
    return left[:limit]


def pending(args, now=None):
    root = Path(args.root)
    ref, tip = pa.checked_commit(root, ledger="", verb="review", worktree="session-review")
    store = Path(args.sessions_root) if args.sessions_root else sessions_root()
    if not store.is_dir():
        raise Problem(f"no sessions under {store}")
    workspace = Path(args.workspace).resolve() if args.workspace else None
    found = candidates(store, workspace)
    chosen = select(found, args.ids, args.max, time.time() if now is None else now, args.idle * 60)

    work = Path(tempfile.mkdtemp(prefix="session-review-"))
    for name in ("sessions", "analyze", "verify"):
        (work / name).mkdir()
    tracker_file = work / "tracker.tsv"
    count = 0
    if chosen:
        try:
            count = pa.tracker(tracker_file)
        except (RuntimeError, ValueError) as problem:
            raise Problem(f"could not list the issues on {pa.REPO}: {one_line(problem, 160)}") from problem

    template = (HERE / "analyze.md").read_text(encoding="utf-8")
    entries = []
    for session in chosen:
        sid = session["id"]
        record_copy, audit_copy = work / "sessions" / f"{sid}.json", work / "sessions" / f"{sid}.audit.jsonl"
        shutil.copyfile(session["record"], record_copy)
        have_audit = Path(session["audit"]).is_file()
        if have_audit:
            shutil.copyfile(session["audit"], audit_copy)
        prompt, result = work / "analyze" / f"{sid}.md", work / "analyze" / f"{sid}.json"
        notes = notes_of(pa.read_json(record_copy))
        notes_copy = work / "sessions" / f"{sid}.notes.json"
        if notes:
            notes_copy.write_text(json.dumps(notes, indent=2), encoding="utf-8")
        session = dict(session, notes=len(notes))
        prompt.write_text(
            pa.render(
                template,
                {
                    "session_id": sid,
                    "build": session["build"],
                    "turns": str(session["turns"]),
                    "from_turn": str(session["from_turn"]),
                    "notes": str(len(notes)),
                    "from_note": str(min(session["from_note"], len(notes))),
                    "notes_file": str(notes_copy) if notes else "(this session has no notes)",
                    "record_file": str(record_copy),
                    "audit_file": str(audit_copy) if have_audit else "(this session has no audit trail)",
                    "tracker": str(tracker_file),
                    "root": str(root),
                    "commit": tip,
                    "ref": ref,
                    "results_file": str(result),
                },
            ),
            encoding="utf-8",
        )
        entries.append(dict(session, prompt_file=str(prompt), results_file=str(result)))

    manifest = {"root": str(root), "ref": ref, "commit": tip, "sessions": entries}
    workdir.write_manifest(work, manifest)
    reviewed = sum(1 for s in found if s["marker"] is not None)
    print(
        f"{len(found)} sessions found, {reviewed} reviewed before, {len(chosen)} to review against {ref} {tip[:12]};"
        f" {count} issues and pull requests listed",
        file=sys.stderr,
    )
    print(json.dumps({"work_dir": str(work), "analyze": [{"id": e["id"], "prompt_file": e["prompt_file"]} for e in entries]}))
    return 0


# Reading what the agents wrote.


def problem_id(session, number):
    return f"{session}-p{number}"


def verify_paths(work, pid):
    return Path(work) / "verify" / f"{pid}.md", Path(work) / "verify" / f"{pid}.json"


def check_problem(problem):
    """Why this problem cannot be drafted, or None."""
    if not isinstance(problem, dict):
        return "not an object"
    missing = [field for field in PROBLEM_TEXT if not str(problem.get(field) or "").strip()]
    if missing:
        return f"no {', '.join(missing)}"
    if problem.get("kind") not in KINDS:
        return f"kind {problem.get('kind')!r} is not among {', '.join(KINDS)}"
    turn = problem.get("turn")
    if not isinstance(turn, int) or isinstance(turn, bool) or turn < 1:
        return "turn is not a turn number"
    if not isinstance(problem.get("security"), bool):
        return "security is not true or false"
    return None


def load_analysis(path, session):
    data = pa.read_json(path)
    if data is None:
        return None, "no readable result"
    if data.get("session") != session:
        return None, "the result names another session"
    problems = data.get("problems")
    if not isinstance(problems, list):
        return None, "problems is not a list"
    if len(problems) > MAX_PROBLEMS:
        return None, f"{len(problems)} problems, more than the {MAX_PROBLEMS} a session may report"
    return data, None


def load_verify(path, pid):
    data = pa.read_json(path)
    if data is None:
        return None, "no readable verdict"
    if data.get("problem") != pid:
        return None, "the verdict names another problem"
    if data.get("verdict") not in VERIFY_VERDICTS:
        return None, f"verdict {data.get('verdict')!r} is not among {', '.join(VERIFY_VERDICTS)}"
    if not one_line(data.get("reason")):
        return None, "no reason"
    if data.get("existing_issue") is not None and pa.issue_number(data["existing_issue"]) is None:
        return None, "existing_issue is not an issue number"
    return data, None


def outcomes(work):
    """Per session: {state, reason, problems}, each problem {id, state, ...}.

    A problem is `file` once a verifier confirmed it and nothing on the tracker holds it, `final`
    when a verifier dropped it or the tracker holds it, `held` when it is flagged security and so
    is never drafted, `verify` while no verifier has answered, and `pending` when what the analyzer
    wrote for it cannot be drafted. A session is `pending` when its result cannot be read.
    """
    manifest = pa.manifest_of(work)
    decided = {}
    for entry in manifest["sessions"]:
        sid = entry["id"]
        analysis, why = load_analysis(entry["results_file"], sid)
        if analysis is None:
            decided[sid] = {"state": "pending", "reason": why, "problems": []}
            continue
        problems = []
        for number, problem in enumerate(analysis["problems"], 1):
            pid = problem_id(sid, number)
            why = check_problem(problem)
            if why:
                problems.append({"id": pid, "state": "pending", "reason": why})
            elif problem["security"]:
                problems.append({"id": pid, "state": "held", "title": one_line(problem["title"], 200), "turn": problem["turn"]})
            else:
                check, why = load_verify(verify_paths(work, pid)[1], pid)
                if check is None:
                    problems.append({"id": pid, "state": "verify", "reason": why, "problem": problem})
                elif check["verdict"] != "confirmed" or check.get("existing_issue") is not None:
                    problems.append(
                        {"id": pid, "state": "final", "verdict": check["verdict"], "issue": check.get("existing_issue"), "reason": check["reason"]}
                    )
                else:
                    problems.append({"id": pid, "state": "file", "fields": problem, "reason": check["reason"]})
        decided[sid] = {"state": "read", "reason": one_line(analysis.get("summary")), "problems": problems}
    return manifest, decided


def verify(args):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    entries = {e["id"]: e for e in manifest["sessions"]}
    template = (HERE / "verify.md").read_text(encoding="utf-8")
    verifiers = []
    for sid, session in decided.items():
        if session["state"] == "pending":
            print(f"  {sid}  undecided: {session['reason']}", file=sys.stderr)
            continue
        entry = entries[sid]
        record_copy = Path(work) / "sessions" / f"{sid}.json"
        audit_copy = Path(work) / "sessions" / f"{sid}.audit.jsonl"
        notes_copy = Path(work) / "sessions" / f"{sid}.notes.json"
        for problem in session["problems"]:
            if problem["state"] != "verify":
                print(f"  {problem['id']}  {problem.get('verdict') or problem['state']}", file=sys.stderr)
                continue
            prompt, result = verify_paths(work, problem["id"])
            claim = {k: problem["problem"].get(k) for k in ("turn", "kind", "title", "what_happens", "code_path", "reproduce", "evidence")}
            prompt.write_text(
                pa.render(
                    template,
                    {
                        "session_id": sid,
                        "problem_id": problem["id"],
                        "build": entry["build"],
                        "claim": pa.fenced(json.dumps(claim, indent=2)),
                        "record_file": str(record_copy),
                        "audit_file": str(audit_copy) if audit_copy.is_file() else "(this session has no audit trail)",
                        "notes_file": str(notes_copy) if notes_copy.is_file() else "(this session has no notes)",
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


def scrub(text, manifest, session):
    """A draft's wording with this machine's paths removed, and no one pinged."""
    text = str(text)
    for path, name in ((session["workspace"], "<workspace>"), (str(Path.home()), "~")):
        if path and path != "/":
            text = text.replace(path, name)
    return pa.clean(text, manifest["root"])


def labels_for(fields):
    labels = [fields["kind"], SESSION_ANALYSIS]
    area = str(fields.get("area") or "").strip().removeprefix("area/")
    if area in pa.AREAS:
        labels.append(f"area/{area}")
    return sorted(set(labels))


def body_for(fields, pid, manifest, session):
    def text(field):
        return scrub(fields[field], manifest, session)

    sections = [
        text("what_happens"),
        f"## Where\n\n{text('code_path')}",
        f"## Reproduce\n\n{text('reproduce')}",
        f"## Fix\n\n{text('fix')}",
        "## Where this comes from\n\n"
        f"Found at turn {fields['turn']} of a recorded bravebot session by the `session-review` skill "
        f"(review id {pid}). A second pass checked it against `{manifest['commit'][:12]}` and could not "
        "disprove it. Filed by a tool rather than a person, so no `importance`, `urgency` or `size` "
        "has been judged.",
    ]
    return "\n\n".join(sections) + "\n"


def draft(args):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    entries = {e["id"]: e for e in manifest["sessions"]}
    out = work / "issues"
    out.mkdir(exist_ok=True)
    drafts, held, waiting = [], [], []
    for sid, session in decided.items():
        if session["state"] == "pending":
            waiting.append(sid)
            continue
        for problem in session["problems"]:
            if problem["state"] == "held":
                held.append(problem)
            elif problem["state"] in ("verify", "pending"):
                waiting.append(problem["id"])
            elif problem["state"] == "file":
                fields, pid = problem["fields"], problem["id"]
                body = out / f"{pid}.md"
                body.write_text(body_for(fields, pid, manifest, entries[sid]), encoding="utf-8")
                title = pa.title_for(fields)
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
        print(f"  held   {problem['id']}  turn {problem['turn']}: {problem['title']}")
    print(f"{len(drafts)} drafted, {len(held)} held back as security concerns" + (f", {len(waiting)} undecided: {', '.join(waiting)}" if waiting else ""))
    return 0


# Recording.


def record(args, today=None):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    filed = pa.read_json(work / "filed.json") or {}
    today = today or datetime.date.today().isoformat()
    marked = waiting = 0
    for entry in manifest["sessions"]:
        sid, session = entry["id"], decided[entry["id"]]
        issues, dropped, held, open_ = [], 0, 0, []
        for problem in session["problems"]:
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
        if session["state"] == "pending" or open_:
            waiting += 1
            why = session["reason"] if session["state"] == "pending" else f"{', '.join(open_)} not settled"
            print(f"  left   {sid}  {why}")
            continue
        marked += 1
        line = {
            "analyzed": today,
            "commit": manifest["commit"][:12],
            "turns": entry["turns"],
            "from_turn": entry["from_turn"],
            "notes": entry.get("notes", 0),
            "from_note": entry.get("from_note", 0),
            "problems": len(session["problems"]),
            "issues": sorted(set(issues)),
            "dropped": dropped,
            "held": held,
        }
        if not args.dry_run:
            write_marker(Path(entry["directory"]), sid, line)
        print(f"  marked {sid}  {len(session['problems'])} problems, issues {line['issues'] or '-'}, {held} held")
    verb = "would mark" if args.dry_run else "marked"
    print(f"{verb} {marked} sessions reviewed, {waiting} left for the next run")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)

    one = commands.add_parser("pending", help="choose the sessions to review and write a prompt for each")
    one.add_argument("ids", nargs="*", metavar="SESSION-ID", help="review these, whatever their markers say; a prefix will do")
    one.add_argument("--max", type=int, default=MAX)
    one.add_argument("--idle", type=int, default=IDLE_MINUTES, help="skip a session written within this many minutes")
    one.add_argument("--workspace", help="only the sessions that ran in this directory")
    one.add_argument("--sessions-root", help="the sessions directory (default ~/.bravebot/sessions)")
    one.add_argument("--root", default=str(ROOT))

    for name, text in (("verify", "write a verifier prompt per problem"), ("draft", "write an issue body per confirmed problem")):
        commands.add_parser(name, help=text).add_argument("--work-dir", required=True)

    pa.add_filed_command(commands)

    one = commands.add_parser("record", help="mark the settled sessions as reviewed")
    one.add_argument("--work-dir", required=True)
    one.add_argument("--dry-run", action="store_true")

    args = parser.parse_args(argv)
    for name in getattr(args, "ids", []):
        if not SESSION_ID.fullmatch(name):
            parser.error(f"{name!r} is not a session id")
    try:
        return {"pending": pending, "verify": verify, "draft": draft, "filed": pa.filed, "record": record}[args.command](args)
    except Problem as problem:
        print(f"session-review: {problem}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
