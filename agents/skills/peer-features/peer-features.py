#!/usr/bin/env python3
"""Compare what bravebot does with what other coding agents document, and file the gaps.

    python3 agents/skills/peer-features/peer-features.py pending [--max N] [UNIT ...]
    python3 agents/skills/peer-features/peer-features.py verify --work-dir DIR
    python3 agents/skills/peer-features/peer-features.py merge --work-dir DIR
    python3 agents/skills/peer-features/peer-features.py draft --work-dir DIR
    python3 agents/skills/peer-features/peer-features.py post --work-dir DIR [--dry-run]
        [--assignee LOGIN] [--max N]
    python3 agents/skills/peer-features/peer-features.py record --work-dir DIR [--dry-run]

A unit is one spec (`spec:HOOK`) or one other coding agent (`peer:codex`). docs/peer-features-reviewed
holds one line per unit already reviewed and one per gap already decided, so a run reads only what is
new. No model takes part in anything this script does.
"""

import argparse
import datetime
import importlib.util
import json
import re
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The advisory skill owns the list of tools, the tracker listing, the title and body cleaning and the
# posting, so a tool added there is compared here and a fix to the posting reaches both.
pa = load("peer_advisories", HERE.parent / "peer-advisories" / "peer-advisories.py")
specs = load("check_spec_specs", HERE.parent / "check-spec" / "specs.py")

Problem = pa.Problem

LEDGER = "docs/peer-features-reviewed"
REPO = pa.REPO
MAX_UNITS = 6
MAX_GAPS = 5
MAX_PEER_GAPS = 8
STALE_DAYS = 90
MAX_SOURCES = 5
MAX_QUOTE = 300

KINDS = ("parity", "beyond")
UNIT_VERDICT = "reviewed"
GAP_VERDICTS = ("filed", "tracked", "declined", "covered", "merged")
VERIFY_VERDICTS = ("confirmed", "covered", "declined", "unsupported", "tracked")
AREAS = pa.AREAS + ("infrastructure",)
GAP_LABELS = ("parity", "beyond-parity")
ISSUE_SUMMARY = 300
MAX_SPEC_HOME = 200

PEERS = HERE / "peers.tsv"
PEER_KINDS = ("terminal", "ide", "cloud")
PEER_FIELDS = ("slug", "name", "kind", "docs", "changelog", "repo")
OWNER_NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+")

UNIT_KEY = re.compile(r"(?:spec:[A-Z][A-Z0-9_]*|peer:[a-z0-9]+(?:-[a-z0-9]+)*)")
GAP_KEY = re.compile(r"(?:parity|beyond)-[a-z0-9]+(?:-[a-z0-9]+)*")
URL = re.compile(r"https://[^\s<>`()\[\]]+")
TEXT = ("title", "summary", "peer", "peer_behaviour", "bravebot_today", "proposal")

HEADER = """\
# Units reviewed and gaps decided by the peer-features skill, one per line, sorted by key:
#
#   <key> <verdict> <issue or -> <commit> <date> <reason>
#
# A unit is a spec or another coding agent:
#
# spec:<ID>, peer:<tool>   reviewed  compared with what the other side documents at <commit>
#
# A gap is a difference a review found, keyed parity-<name> or beyond-<name>:
#
# filed     an issue was opened for it, and <issue> tracks it
# tracked   the tracker already held it, and <issue> is that issue
# declined  bravebot should not build it, and the reason says which rule or clause forbids it
# covered   bravebot already does what the other tool does
# merged    the same change as the gap the reason names after "into", and <issue> tracks both
#
# A reviewed unit is offered again after 90 days, since what other tools document keeps changing.
# A decided gap is final until somebody removes its line. The peer-features skill writes this file:
# agents/skills/peer-features/SKILL.md.
"""


# The ledger.


def ledger_line_ok(key, verdict):
    if UNIT_KEY.fullmatch(key):
        return verdict == UNIT_VERDICT
    return bool(GAP_KEY.fullmatch(key)) and verdict in GAP_VERDICTS


def read_ledger(path):
    entries = {}
    if not path.exists():
        return entries
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split(maxsplit=5)
        if len(parts) < 5 or not ledger_line_ok(parts[0], parts[1]):
            raise Problem(f"{path}:{number}: not a ledger line: {line!r}")
        try:
            datetime.date.fromisoformat(parts[4])
        except ValueError as problem:
            raise Problem(f"{path}:{number}: {parts[4]!r} is not a date") from problem
        entries[parts[0]] = {
            "verdict": parts[1],
            "issue": parts[2],
            "commit": parts[3],
            "date": parts[4],
            "reason": parts[5] if len(parts) > 5 else "",
        }
    return entries


def write_ledger(path, entries):
    lines = [
        f"{key} {e['verdict']} {e['issue']} {e['commit']} {e['date']} {pa.one_line(e['reason'])}".rstrip()
        for key, e in sorted(entries.items())
    ]
    path.write_text(HEADER + "".join(line + "\n" for line in lines), encoding="utf-8")


# Which commit is being compared.


def checked_commit(root):
    """The main commit this tree is, refusing a tree that differs from it anywhere but the ledger."""
    ref = pa.main_ref(root)
    tip = pa.git(root, "rev-parse", ref)
    remedy = f"fetch, then review a tree at {ref}:\n  git worktree add --detach ../bravebot-peer-gaps {ref}"
    if pa.run(["git", "-C", str(root), "merge-base", "--is-ancestor", tip, "HEAD"]).returncode != 0:
        raise Problem(f"HEAD does not contain {ref} ({tip[:12]}); {remedy}")
    changed = [p for p in pa.git(root, "diff", "--name-only", tip, "--").splitlines() if p != LEDGER]
    if changed:
        raise Problem(
            f"this tree differs from {ref} in {len(changed)} file(s) besides the ledger, "
            f"{changed[0]} first; what bravebot lacks has to be read off main, so {remedy}"
        )
    return ref, tip


# The units.


def slug(name):
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def read_peers(path):
    """The rows of peers.tsv, each a dict, refusing a malformed one with its line number."""
    rows, seen = [], set()
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        where = f"{path.name}:{number}"
        fields = line.split("\t")
        if len(fields) != len(PEER_FIELDS):
            raise Problem(f"{where}: {len(fields)} fields, not {len(PEER_FIELDS)} ({', '.join(PEER_FIELDS)})")
        row = dict(zip(PEER_FIELDS, fields))
        if not UNIT_KEY.fullmatch(f"peer:{row['slug']}"):
            raise Problem(f"{where}: slug {row['slug']!r} is not lowercase words joined by hyphens")
        if row["slug"] in seen:
            raise Problem(f"{where}: slug {row['slug']!r} is listed twice")
        seen.add(row["slug"])
        if not row["name"].strip() or row["name"] != row["name"].strip():
            raise Problem(f"{where}: the name is empty or has spaces around it")
        if row["kind"] not in PEER_KINDS:
            raise Problem(f"{where}: kind {row['kind']!r} is not among {', '.join(PEER_KINDS)}")
        if not URL.fullmatch(row["docs"]):
            raise Problem(f"{where}: docs {row['docs']!r} is not an https url")
        if row["changelog"] != "-" and not URL.fullmatch(row["changelog"]):
            raise Problem(f"{where}: changelog {row['changelog']!r} is neither an https url nor -")
        if row["repo"] != "-" and not OWNER_NAME.fullmatch(row["repo"]):
            raise Problem(f"{where}: repo {row['repo']!r} is neither owner/name nor -")
        rows.append({k: (None if v == "-" else v) for k, v in row.items()})
    return rows


def peer_tools(path=PEERS):
    """Every tool to compare, by slug: the ones the advisory skill watches, then the ones in peers.tsv.
    A row's repository replaces the advisory skill's for the same tool."""
    tools = {}
    for repo, name in pa.REPOSITORIES:
        tools[slug(name)] = {"name": name, "repo": repo}
    for _, _, name in pa.PACKAGES:
        tools.setdefault(slug(name), {"name": name, "repo": None})
    for row in read_peers(path):
        known = tools.get(row["slug"], {})
        tools[row["slug"]] = {**row, "repo": row["repo"] or known.get("repo")}
    return tools


def all_units(root):
    root = Path(root).resolve()
    found = []
    for spec in specs.load_specs(root / "docs" / "specs"):
        unit = f"spec:{spec.id}"
        if not UNIT_KEY.fullmatch(unit):
            continue
        found.append(
            {
                "unit": unit,
                "kind": "spec",
                "name": spec.id,
                "title": str(spec.front.get("title") or spec.id),
                "path": str(spec.path.resolve().relative_to(root)),
                "governs": [g for g in spec.governs if isinstance(g, str)],
            }
        )
    for name, tool in peer_tools().items():
        found.append({"unit": f"peer:{name}", "kind": "peer", "name": tool["name"], "repo": tool["repo"],
                      "tool_kind": tool.get("kind"), "docs": tool.get("docs"), "changelog": tool.get("changelog")})
    return found


def named_units(tokens, units):
    """The units a person named: `spec:HOOK`, `peer:codex`, or a bare id or tool name."""
    by_key = {u["unit"]: u for u in units}
    chosen = []
    for token in dict.fromkeys(tokens):
        candidates = [token] if ":" in token else [f"spec:{token}", f"peer:{slug(token)}"]
        match = next((c for c in candidates if c in by_key), None)
        if match is None:
            raise Problem(f"no spec or tool is called {token!r}")
        chosen.append(by_key[match])
    return chosen


def reviewed_on(ledger, unit):
    entry = ledger.get(unit["unit"])
    return datetime.date.fromisoformat(entry["date"]) if entry else None


def select(units, ledger, named, limit, today=None):
    """What this run reviews: the ones named, else those never reviewed, then the stalest."""
    if named:
        return named
    today = today or datetime.date.today()
    cutoff = today - datetime.timedelta(days=STALE_DAYS)
    never = [u for u in units if reviewed_on(ledger, u) is None]
    stale = sorted((u for u in units if (d := reviewed_on(ledger, u)) is not None and d < cutoff),
                   key=lambda u: reviewed_on(ledger, u))
    return (never + stale)[:limit]


# Prompts.


def peers_block(tools):
    return "\n".join(
        f"- {t['name']}" + (f": https://github.com/{t['repo']}" if t["repo"] else f": {t['docs']}" if t.get("docs") else "")
        for t in tools.values()
    )


def gap_cap(unit):
    """A tool review can show far more missing capabilities than one spec can."""
    return MAX_PEER_GAPS if unit.startswith("peer:") else MAX_GAPS


def known_gaps(ledger, path):
    rows = [(key, e) for key, e in sorted(ledger.items()) if GAP_KEY.fullmatch(key)]
    path.write_text(
        "".join(f"{key}\t{e['verdict']}\t{e['issue']}\t{pa.one_line(e['reason'], 200)}\n" for key, e in rows),
        encoding="utf-8",
    )
    return len(rows)


def pending(args):
    root = Path(args.root).resolve()
    ledger = read_ledger(root / LEDGER)
    ref, tip = checked_commit(root)

    units = all_units(root)
    chosen = select(units, ledger, named_units(args.units, units), args.max)

    work = Path(tempfile.mkdtemp(prefix="peer-features-"))
    (work / "research").mkdir()
    tracker_file, known_file = work / "tracker.tsv", work / "known-gaps.tsv"
    try:
        count = pa.tracker(tracker_file)
    except (RuntimeError, ValueError) as problem:
        raise Problem(f"could not list the issues on {REPO}: {pa.one_line(problem, 160)}") from problem
    known = known_gaps(ledger, known_file)

    today = datetime.date.today().isoformat()
    rules = (HERE / "gaps.md").read_text(encoding="utf-8")
    research = []
    for unit in chosen:
        name = unit["unit"].replace(":", "-")
        prompt, result = work / "research" / f"{name}.md", work / "research" / f"{name}.json"
        values = {
            "unit": unit["unit"],
            "root": str(root),
            "commit": tip,
            "ref": ref,
            "today": today,
            "tracker": str(tracker_file),
            "known_gaps": str(known_file),
            "results_file": str(result),
            "max_gaps": str(gap_cap(unit["unit"])),
            "max_sources": str(MAX_SOURCES),
            "max_quote": str(MAX_QUOTE),
            "peers": peers_block(peer_tools()),
        }
        if unit["kind"] == "spec":
            values.update(
                spec_id=unit["name"],
                spec_title=unit["title"],
                spec_path=unit["path"],
                governs="\n".join(f"- `{path}`" for path in unit["governs"]) or "- none listed",
            )
            template = "research-spec.md"
        else:
            values.update(
                peer_name=unit["name"],
                peer_kind=unit["tool_kind"] or "unknown",
                peer_docs=unit["docs"] or "none known; find its official site",
                peer_changelog=unit["changelog"] or "none published; read the repository's releases and tags",
                peer_repo=f"https://github.com/{unit['repo']}" if unit["repo"] else "none known",
            )
            template = "research-peer.md"
        values["gap_rules"] = pa.render(rules, values)
        prompt.write_text(pa.render((HERE / template).read_text(encoding="utf-8"), values), encoding="utf-8")
        research.append({"unit": unit["unit"], "prompt_file": str(prompt), "results_file": str(result)})

    manifest = {
        "root": str(root),
        "ref": ref,
        "commit": tip,
        "tracker": str(tracker_file),
        "known_gaps": str(known_file),
        "units": research,
        "subjects": {u["unit"]: u for u in chosen},
    }
    (work / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    reviewed = sum(1 for u in units if u["unit"] in ledger)
    print(
        f"{len(units)} units, {reviewed} reviewed already, {len(chosen)} to review against {ref} {tip[:12]};"
        f" {known} gaps decided, {count} issues and pull requests listed",
        file=sys.stderr,
    )
    print(json.dumps({"work_dir": str(work), "research": [{k: r[k] for k in ("unit", "prompt_file")} for r in research]}))
    return 0


# Reading what the agents wrote.


def check_gap(gap):
    """Why this candidate cannot be drafted, or None."""
    if not isinstance(gap, dict):
        return "not an object"
    gid, kind = str(gap.get("id") or ""), gap.get("kind")
    if kind not in KINDS:
        return f"kind {kind!r} is not among {', '.join(KINDS)}"
    if not GAP_KEY.fullmatch(gid) or not gid.startswith(f"{kind}-"):
        return f"id {gid!r} is not {kind}- followed by lowercase words joined by hyphens"
    missing = [field for field in TEXT if not str(gap.get(field) or "").strip()]
    if kind == "beyond" and not str(gap.get("delta") or "").strip():
        missing.append("delta")
    if missing:
        return f"no {', '.join(missing)}"
    sources = gap.get("sources")
    if not isinstance(sources, list) or not 1 <= len(sources) <= MAX_SOURCES:
        return f"sources is not a list of 1 to {MAX_SOURCES} urls"
    if not all(isinstance(s, str) and URL.fullmatch(s) for s in sources):
        return "a source is not an https url"
    evidence = gap.get("evidence")
    if not isinstance(evidence, list) or not evidence or not all(isinstance(e, str) and e.strip() for e in evidence):
        return "evidence is not a list of the places bravebot was read"
    if len(str(gap.get("quote") or "")) > MAX_QUOTE:
        return f"quote is longer than {MAX_QUOTE} characters"
    area = str(gap.get("area") or "").strip().removeprefix("area/")
    if area and area not in AREAS:
        return f"area {area!r} is not among {', '.join(AREAS)}"
    if len(str(gap.get("spec_home") or "")) > MAX_SPEC_HOME:
        return f"spec_home is longer than {MAX_SPEC_HOME} characters"
    if gap.get("existing_issue") is not None and pa.issue_number(gap["existing_issue"]) is None:
        return "existing_issue is not an issue number"
    return None


def load_research(path, unit):
    data = pa.read_json(path)
    if data is None:
        return None, "no readable result"
    if data.get("unit") != unit:
        return None, "the result names another unit"
    gaps = data.get("gaps")
    if not isinstance(gaps, list):
        return None, "gaps is not a list"
    seen, cap = set(), gap_cap(unit)
    for number, gap in enumerate(gaps[:cap], 1):
        problem = check_gap(gap)
        if problem is None and gap["id"] in seen:
            problem = f"id {gap['id']} is used twice"
        if problem:
            return None, f"gap {number}: {problem}"
        seen.add(gap["id"])
    return gaps[:cap], None


def load_verify(path, unit):
    """The verifier's verdict per gap id, or (None, why)."""
    data = pa.read_json(path)
    if data is None:
        return None, "no readable verdict"
    if data.get("unit") != unit:
        return None, "the verdict names another unit"
    verdicts = {}
    for entry in data.get("gaps") if isinstance(data.get("gaps"), list) else []:
        if not isinstance(entry, dict) or not GAP_KEY.fullmatch(str(entry.get("id") or "")):
            return None, "a verdict names no gap"
        if entry.get("verdict") not in VERIFY_VERDICTS:
            return None, f"verdict {entry.get('verdict')!r} is not among {', '.join(VERIFY_VERDICTS)}"
        if not pa.one_line(entry.get("reason")):
            return None, f"{entry['id']} has no reason"
        if entry["verdict"] == "tracked" and pa.issue_number(entry.get("existing_issue")) is None:
            return None, f"{entry['id']} is tracked with no issue number"
        verdicts[entry["id"]] = entry
    return verdicts, None


def manifest_of(work):
    manifest = pa.read_json(Path(work) / "manifest.json")
    if manifest is None:
        raise Problem(f"{work} has no manifest.json; run pending first")
    return manifest


def verify_paths(work, unit):
    name = unit.replace(":", "-")
    return Path(work) / "verify" / f"{name}.md", Path(work) / "verify" / f"{name}.json"


def outcomes(work):
    """Per unit, {state, reason, gaps}, where each gap is {state, ...}.

    A gap's `state` is `final` for a verdict the ledger can take, `file` for a confirmed gap nothing
    on the tracker holds, `held` for one the ledger already decided, `dropped` for one the
    verifier found unsupported, `verify` while no verifier has answered, and `pending` for one its
    verifier skipped. A unit is `pending` while its research cannot be read.
    """
    manifest = manifest_of(work)
    ledger = read_ledger(Path(manifest["root"]) / LEDGER)
    decided = {}
    for entry in manifest["units"]:
        unit = entry["unit"]
        gaps, problem = load_research(entry["results_file"], unit)
        if gaps is None:
            decided[unit] = {"state": "pending", "reason": problem, "gaps": {}}
            continue
        verdicts, _ = load_verify(verify_paths(work, unit)[1], unit)
        results = {}
        for gap in gaps:
            gid = gap["id"]
            if ledger.get(gid, {}).get("verdict") in GAP_VERDICTS:
                results[gid] = {"state": "held", "gap": gap}
            elif gap.get("existing_issue") is not None:
                results[gid] = {"state": "final", "verdict": "tracked", "issue": gap["existing_issue"],
                                "reason": f"the tracker holds it: {gap['title']}", "gap": gap}
            elif verdicts is None:
                results[gid] = {"state": "verify", "gap": gap}
            elif gid not in verdicts:
                results[gid] = {"state": "pending", "reason": "the verifier did not answer for it", "gap": gap}
            else:
                verdict = verdicts[gid]
                if verdict["verdict"] == "confirmed":
                    results[gid] = {"state": "file", "reason": verdict["reason"], "gap": gap}
                elif verdict["verdict"] == "unsupported":
                    results[gid] = {"state": "dropped", "reason": verdict["reason"], "gap": gap}
                else:
                    results[gid] = {"state": "final", "verdict": verdict["verdict"],
                                    "issue": verdict.get("existing_issue"), "reason": verdict["reason"], "gap": gap}
        decided[unit] = {"state": "read", "reason": "", "gaps": results}
    return manifest, decided


def verify(args):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    template = (HERE / "verify.md").read_text(encoding="utf-8")
    (work / "verify").mkdir(exist_ok=True)
    verifiers = []
    for unit, info in decided.items():
        if info["state"] == "pending":
            print(f"  {unit}  research unreadable: {pa.one_line(info['reason'], 100)}", file=sys.stderr)
            continue
        waiting = [r["gap"] for r in info["gaps"].values() if r["state"] == "verify"]
        if not waiting:
            print(f"  {unit}  nothing to verify ({len(info['gaps'])} candidates)", file=sys.stderr)
            continue
        prompt, result = verify_paths(work, unit)
        prompt.write_text(
            pa.render(
                template,
                {
                    "unit": unit,
                    "candidates": pa.fenced(json.dumps(waiting, indent=2)),
                    "root": manifest["root"],
                    "commit": manifest["commit"],
                    "today": datetime.date.today().isoformat(),
                    "tracker": manifest["tracker"],
                    "known_gaps": manifest["known_gaps"],
                    "results_file": str(result),
                },
            ),
            encoding="utf-8",
        )
        verifiers.append({"unit": unit, "prompt_file": str(prompt)})
        print(f"  {unit}  {len(waiting)} to verify", file=sys.stderr)
    print(json.dumps({"work_dir": str(work), "verify": verifiers}))
    return 0


# Merging the same change found by more than one unit.


def merge_paths(work):
    folder = Path(work) / "merge"
    return folder / "prompt.md", folder / "asked.json", folder / "groups.json", folder / "applied.json"


def confirmed_gaps(decided):
    """Every gap to be filed, once per id, as (unit, gap) for the first unit that found it."""
    found = {}
    for unit, info in decided.items():
        for gid, result in info["gaps"].items():
            if result["state"] == "file":
                found.setdefault(gid, (unit, result["gap"]))
    return found


def labelled_issues():
    """Every open and closed issue carrying a label this skill files under, in number order."""
    found = {}
    for label in GAP_LABELS:
        listed = json.loads(
            pa.gh(["issue", "list", "--repo", REPO, "--label", label, "--state", "all", "--limit", "5000",
                   "--json", "number,state,title,body"])
        )
        for one in listed:
            found[one["number"]] = one
    return [found[number] for number in sorted(found)]


def issue_line(one):
    """An issue as number, state, title and the first paragraph of its body, which for an issue this
    skill filed is the gap's summary."""
    body = str(one.get("body") or "").replace("\r\n", "\n").strip()
    summary = pa.one_line(body.split("\n\n", 1)[0], ISSUE_SUMMARY)
    return f"#{one['number']} {one['state'].lower()}: {pa.one_line(one['title'], 300)}" + (f" | {summary}" if summary else "")


def merge(args):
    work = Path(args.work_dir)
    _, decided = outcomes(work)
    confirmed = confirmed_gaps(decided)
    if not confirmed:
        print("no confirmed gaps to merge", file=sys.stderr)
        print(json.dumps({"work_dir": str(work), "merge": []}))
        return 0
    try:
        issues = labelled_issues()
    except (RuntimeError, ValueError) as problem:
        raise Problem(f"could not list the {' and '.join(GAP_LABELS)} issues on {REPO}: {pa.one_line(problem, 160)}") from problem
    # An issue this run filed holds one of the gaps listed, so it is not another place to merge into.
    ours = {how.get("issue") for how in (pa.read_json(work / "filed.json") or {}).values() if isinstance(how, dict)}
    issues = [one for one in issues if one["number"] not in ours]

    prompt, asked, result, _ = merge_paths(work)
    prompt.parent.mkdir(exist_ok=True)
    result.unlink(missing_ok=True)
    candidates = [
        {"id": gid, "kind": gap["kind"], "unit": unit, "title": gap["title"], "summary": gap["summary"],
         "proposal": gap["proposal"]}
        for gid, (unit, gap) in confirmed.items()
    ]
    listed = "\n".join(issue_line(one) for one in issues)
    prompt.write_text(
        pa.render(
            (HERE / "merge.md").read_text(encoding="utf-8"),
            {
                "candidates": pa.fenced(json.dumps(candidates, indent=2)),
                "issues": pa.fenced(listed or "none"),
                "results_file": str(result),
            },
        ),
        encoding="utf-8",
    )
    asked.write_text(json.dumps({"gaps": list(confirmed), "issues": [one["number"] for one in issues]}), encoding="utf-8")
    print(f"  {len(confirmed)} confirmed gaps from {len({u for u, _ in confirmed.values()})} units,"
          f" {len(issues)} {' or '.join(GAP_LABELS)} issues", file=sys.stderr)
    print(json.dumps({"work_dir": str(work), "merge": [{"prompt_file": str(prompt)}]}))
    return 0


def check_group(group, confirmed, issues, placed):
    """Why this group cannot be applied, or None."""
    if not isinstance(group, dict):
        return "not an object"
    ids = group.get("gaps")
    if not isinstance(ids, list) or not ids or not all(isinstance(g, str) for g in ids):
        return "gaps is not a list of gap ids"
    unknown = [g for g in ids if g not in confirmed]
    if unknown:
        return f"{unknown[0]} is not a gap this merge was asked about"
    again = [g for n, g in enumerate(ids) if g in placed or g in ids[:n]]
    if again:
        return f"{again[0]} is in more than one group"
    if len({confirmed[g][1]["kind"] for g in ids}) > 1:
        return "it puts a parity gap and a beyond gap together"
    issue = group.get("existing_issue")
    if issue is not None and pa.issue_number(issue) not in issues:
        return f"existing_issue {issue!r} is not one of the {' or '.join(GAP_LABELS)} issues listed"
    if len(ids) < 2 and issue is None:
        return "one gap and no existing_issue"
    if not pa.one_line(group.get("reason")):
        return "no reason"
    return None


def load_merge(work, confirmed):
    """The merge subagent's groups, or (None, why)."""
    _, asked_path, groups_path, _ = merge_paths(work)
    asked = pa.read_json(asked_path)
    if asked is None:
        return None, "merge has not run"
    if sorted(asked.get("gaps") or []) != sorted(confirmed):
        return None, "the merge was asked about another set of confirmed gaps"
    data = pa.read_json(groups_path)
    if data is None or not isinstance(data.get("groups"), list):
        return None, f"{groups_path} holds no readable groups"
    issues, placed = set(asked.get("issues") or []), set()
    for number, group in enumerate(data["groups"], 1):
        problem = check_group(group, confirmed, issues, placed)
        if problem:
            return None, f"group {number}: {problem}"
        placed.update(group["gaps"])
    return data["groups"], None


def merge_plan(work, decided):
    """What the merge's groups change, by gap id, and why they could not be applied, or None.

    A group's first gap stays `file` and `absorbs` the others, or becomes `final` and `tracked`
    where the group names an existing issue. Each other gap becomes `merged`, `into` the first.
    """
    confirmed = confirmed_gaps(decided)
    if not confirmed:
        return {}, None
    groups, problem = load_merge(work, confirmed)
    if groups is None:
        return {}, problem
    plan = {}
    for group in groups:
        first, *rest = group["gaps"]
        reason, issue = pa.one_line(group["reason"]), group.get("existing_issue")
        if issue is None:
            plan[first] = {"absorbs": rest, "group_reason": reason}
        else:
            plan[first] = {"state": "final", "verdict": "tracked", "issue": issue, "reason": reason}
        for gid in rest:
            plan[gid] = {"state": "merged", "into": first, "issue": issue, "reason": reason}
    return plan, None


def apply_plan(decided, plan):
    for info in decided.values():
        for gid, result in info["gaps"].items():
            if result["state"] == "file" and gid in plan:
                result.update(plan[gid])
    return decided


# Drafting.


def labels_for(gap):
    labels = ["parity"] if gap["kind"] == "parity" else ["enhancement", "beyond-parity"]
    area = str(gap.get("area") or "").strip().removeprefix("area/")
    if area == "infrastructure":
        labels.append("infrastructure")
    elif area in pa.AREAS:
        labels.append(f"area/{area}")
    return sorted(set(labels))


def subject_of(manifest, unit):
    subject = manifest["subjects"][unit]
    if subject["kind"] == "spec":
        return f"`{subject['path']}`"
    return f"the {subject['name']} documentation"


def also_found(gap, also, manifest):
    """What the gaps merged into this one add to it: each peer's behaviour, sources not yet listed, its
    proposal, constraints and delta where they differ, and its id, so a search for that id finds this
    issue."""
    root = manifest["root"]
    listed = set(gap["sources"])
    parts = [f"## Also found by\n\nThis run found the same change more than once. {pa.clean(also['reason'], root)}"]
    for unit, other in also["gaps"]:
        part = f"### What {pa.clean(other['peer'], root)} does, found by the review of {subject_of(manifest, unit)}\n\n"
        part += pa.clean(other["peer_behaviour"], root)
        new = [url for url in other["sources"] if url not in listed]
        listed.update(new)
        if new:
            part += "\n\nSources:\n\n" + "\n".join(f"- <{url}>" for url in new)
        for field, label in (("proposal", "Its proposal"), ("delta", "Where it goes past parity"),
                             ("constraints", "Its constraints")):
            text = pa.one_line(other.get(field))
            if text and text != pa.one_line(gap.get(field)):
                part += f"\n\n{label}:\n\n" + pa.clean(other[field], root)
        parts.append(part + f"\n\nGap id: `{other['id']}`")
    return "\n\n".join(parts)


def body_for(gap, unit, manifest, also=None):
    root, commit = manifest["root"], manifest["commit"]
    quote = str(gap.get("quote") or "").strip()
    sources = "\n".join(f"- <{url}>" for url in gap["sources"])
    does = f"## What {pa.clean(gap['peer'], root)} does\n\n{pa.clean(gap['peer_behaviour'], root)}"
    if quote:
        does += "\n\n" + "\n".join(f"> {line}" for line in pa.clean(quote, root).splitlines())
    does += f"\n\nSources:\n\n{sources}"
    today = "## What bravebot does today\n\n" + pa.clean(gap["bravebot_today"], root)
    today += "\n\nRead: " + ", ".join(f"`{pa.undash(e.strip()).replace('`', '')}`" for e in gap["evidence"])
    sections = [pa.clean(gap["summary"], root), does, today, "## Proposal\n\n" + pa.clean(gap["proposal"], root)]
    if str(gap.get("spec_home") or "").strip():
        sections.append("## Where it lands\n\n" + pa.clean(gap["spec_home"], root))
    if gap["kind"] == "beyond":
        sections.append("## Where it goes past parity\n\n" + pa.clean(gap["delta"], root))
    if str(gap.get("constraints") or "").strip():
        sections.append("## Constraints\n\n" + pa.clean(gap["constraints"], root))
    if also:
        sections.append(also_found(gap, also, manifest))
    sections.append(
        "## Where this comes from\n\n"
        f"The `peer-features` skill compared {subject_of(manifest, unit)} with what other coding agents "
        f"publish, at `{commit[:12]}`. A second pass checked that each source says what is claimed, that "
        "bravebot lacks it and that it fits the specs. Filed by a tool rather than a person, so no "
        "`importance`, `urgency` or `size` has been judged.\n\n"
        f"Gap id: `{gap['id']}`"
    )
    return "\n\n".join(sections) + "\n"


def draft(args):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    confirmed = confirmed_gaps(decided)
    plan, problem = merge_plan(work, decided)
    applied = merge_paths(work)[3]
    if problem:
        (work / "drafts.json").unlink(missing_ok=True)
        applied.unlink(missing_ok=True)
        raise Problem(f"{len(confirmed)} confirmed gaps are not drafted: {problem}; run merge and its subagent first")
    if plan:
        applied.write_text(json.dumps(plan, indent=2), encoding="utf-8")
    else:
        applied.unlink(missing_ok=True)
    decided = apply_plan(decided, plan)
    out = work / "issues"
    out.mkdir(exist_ok=True)
    drafts, seen = [], set()
    for unit, info in decided.items():
        for gid, result in info["gaps"].items():
            if result["state"] != "file" or gid in seen:
                continue
            seen.add(gid)
            gap, absorbed = result["gap"], result.get("absorbs", [])
            also = {"reason": result["group_reason"], "gaps": [confirmed[g] for g in absorbed]} if absorbed else None
            body = out / f"{gid}.md"
            body.write_text(body_for(gap, unit, manifest, also), encoding="utf-8")
            title = pa.title_for(gap)
            key = pa.one_line(gap.get("key"), 80)
            drafts.append(
                {
                    "id": gid,
                    "unit": unit,
                    "title": title,
                    "labels": labels_for(gap),
                    "key": key if key and key.lower() in title.lower() else "",
                    "body_file": str(body),
                    "merged": absorbed,
                }
            )
            print(f"  {gid}  {title}\n{'':4}{', '.join(labels_for(gap))}")
            if absorbed:
                print(f"{'':4}also {', '.join(absorbed)}")
    (work / "drafts.json").write_text(json.dumps(drafts, indent=2), encoding="utf-8")
    waiting = [
        f"{unit}:{gid}" if gid else unit
        for unit, info in decided.items()
        for gid in ([""] if info["state"] == "pending" else [g for g, r in info["gaps"].items() if r["state"] in ("verify", "pending")])
    ]
    print(f"{len(drafts)} drafted" + (f", {len(waiting)} undecided: {', '.join(waiting)}" if waiting else ""))
    return 0


# Recording.


def summary_of(info):
    tally = {}
    for result in info["gaps"].values():
        name = result.get("verdict") or {"file": "filed", "held": "already decided"}.get(result["state"], result["state"])
        tally[name] = tally.get(name, 0) + 1
    return f"{len(info['gaps'])} candidates" + "".join(f", {n} {name}" for name, n in sorted(tally.items()))


def record(args, today=None):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    # The plan draft applied, since a gap recorded earlier no longer counts as confirmed and so
    # changes the set the merge was asked about.
    decided = apply_plan(decided, pa.read_json(merge_paths(work)[3]) or {})
    filed = pa.read_json(work / "filed.json") or {}
    path = Path(manifest["root"]) / LEDGER
    ledger = read_ledger(path)
    today = today or datetime.date.today().isoformat()
    commit = manifest["commit"][:12]
    written, waiting = 0, 0

    def put(key, verdict, issue, reason):
        ledger[key] = {"verdict": verdict, "issue": f"#{issue}" if issue else "-", "commit": commit,
                       "date": today, "reason": reason}

    for unit, info in decided.items():
        if info["state"] == "pending":
            waiting += 1
            print(f"  left   {unit}  {info['reason']}")
            continue
        open_gaps = 0
        for gid, result in info["gaps"].items():
            state = result["state"]
            if state == "file" and gid in filed:
                how = filed[gid]
                put(gid, "tracked" if how.get("how") == "existing" else "filed", how["issue"], result["reason"])
                written += 1
            elif state == "final":
                put(gid, result["verdict"], result.get("issue"), result["reason"])
                written += 1
            elif state == "merged":
                issue = result["issue"] or filed.get(result["into"], {}).get("issue")
                if issue:
                    put(gid, "merged", issue, f"into {result['into']}: {result['reason']}")
                    written += 1
                else:
                    open_gaps += 1
                    print(f"  left   {gid}  merged into {result['into']}, which is not filed")
            elif state in ("file", "verify", "pending"):
                open_gaps += 1
                print(f"  left   {gid}  {'confirmed and not filed' if state == 'file' else result.get('reason', 'not verified')}")
        if open_gaps:
            waiting += 1
            continue
        put(unit, UNIT_VERDICT, None, summary_of(info))
        written += 1
        print(f"  {UNIT_VERDICT:<8} {unit}  {summary_of(info)}")
    if not args.dry_run and written:
        write_ledger(path, ledger)
    verb = "would record" if args.dry_run else "recorded"
    print(f"{verb} {written} lines in {LEDGER}, {waiting} units left for the next run")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)

    one = commands.add_parser("pending", help="write a research prompt per unit still to review")
    one.add_argument("units", nargs="*", metavar="UNIT", help="review these, whatever the ledger says")
    one.add_argument("--max", type=int, default=MAX_UNITS)
    one.add_argument("--root", default=str(ROOT))

    for name, text in (("verify", "write a verifier prompt per unit with candidate gaps"),
                       ("merge", "write one prompt listing every confirmed gap and the issues already filed"),
                       ("draft", "write an issue body per confirmed gap or group of them")):
        commands.add_parser(name, help=text).add_argument("--work-dir", required=True)

    one = commands.add_parser("post", help="file the drafts the tracker does not hold")
    one.add_argument("--work-dir", required=True)
    one.add_argument("--repo", default=REPO)
    one.add_argument("--dry-run", action="store_true")
    one.add_argument("--assignee")
    one.add_argument("--max", type=int, default=pa.MAX_POSTS)
    one.add_argument("--pace", type=float, default=10.0)

    one = commands.add_parser("record", help="write the units reviewed and gaps decided into the ledger")
    one.add_argument("--work-dir", required=True)
    one.add_argument("--dry-run", action="store_true")

    args = parser.parse_args(argv)
    try:
        return {"pending": pending, "verify": verify, "merge": merge, "draft": draft, "post": pa.post,
                "record": record}[args.command](args)
    except Problem as problem:
        print(f"peer-features: {problem}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
