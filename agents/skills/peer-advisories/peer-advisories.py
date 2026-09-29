#!/usr/bin/env python3
"""Vet other coding agents' published security advisories against this tree.

    python3 agents/skills/peer-advisories/peer-advisories.py pending [--max N] [GHSA-ID ...]
    python3 agents/skills/peer-advisories/peer-advisories.py verify --work-dir DIR
    python3 agents/skills/peer-advisories/peer-advisories.py draft --work-dir DIR
    python3 agents/skills/peer-advisories/peer-advisories.py post --work-dir DIR [--dry-run]
        [--assignee LOGIN] [--max N]
    python3 agents/skills/peer-advisories/peer-advisories.py record --work-dir DIR [--dry-run]

docs/peer-advisories-vetted holds one line per advisory already vetted, keyed by its GHSA id, so a
run reads only what is new. No model takes part in anything this script does.
"""

import argparse
import datetime
import importlib.util
import json
import random
import re
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
LEDGER = "docs/peer-advisories-vetted"
REPO = "brave/bravebot"
MAX = 8

REPOSITORIES = (
    ("anthropics/claude-code", "Claude Code"),
    ("cursor/cursor", "Cursor"),
    ("openai/codex", "Codex"),
    ("sst/opencode", "OpenCode"),
    ("cline/cline", "Cline"),
    ("RooCodeInc/Roo-Code", "Roo Code"),
    ("zed-industries/zed", "Zed"),
    ("block/goose", "Goose"),
    ("All-Hands-AI/OpenHands", "OpenHands"),
    ("github/copilot-cli", "GitHub Copilot CLI"),
    ("google-gemini/gemini-cli", "Gemini CLI"),
    ("QwenLM/qwen-code", "Qwen Code"),
)
PACKAGES = (
    ("npm", "@anthropic-ai/claude-code", "Claude Code"),
    ("npm", "@openai/codex", "Codex"),
    ("npm", "opencode-ai", "OpenCode"),
    ("npm", "@google/gemini-cli", "Gemini CLI"),
    ("npm", "@qwen-code/qwen-code", "Qwen Code"),
    ("npm", "@github/copilot", "GitHub Copilot CLI"),
    ("npm", "@kilocode/cli", "Kilo Code"),
    ("pip", "aider-chat", "Aider"),
)

GHSA = re.compile(r"GHSA(?:-[a-z0-9]{4}){3}")
CWE = re.compile(r"CWE-\d+")
SEVERITY_RANK = {"critical": 4, "high": 3, "medium": 2, "moderate": 2, "low": 1}

VERDICTS = ("affected", "known", "holds", "absent", "deferred")
FINAL = ("affected", "known", "holds", "absent")
VERIFY_VERDICTS = ("confirmed", "known", "holds", "absent")
KINDS = ("bug", "spec-bug", "spec-mismatch", "enhancement")
SEVERITIES = ("high", "medium", "low")
AREAS = ("trust", "tools", "turns", "interface", "cli", "delegation", "backends", "skus", "i18n")
ISSUE_TEXT = ("title", "what_happens", "code_path", "reproduce", "fix")
TITLE_LIMIT = 120
EM_DASH = "\u2014"

HEADER = """\
# Peer advisories vetted against this tree, one per line, sorted by id:
#
#   <GHSA id> <verdict> <issue or -> <commit> <date> <reason>
#
# affected  bravebot has the defect, and <issue> tracks it
# known     bravebot has it, and a spec records it as an accepted cost
# holds     bravebot has the surface, and the attack fails on it
# absent    bravebot has nothing the attack needs
# deferred  not decided; every run offers it again
#
# <commit> is the main commit the advisory was vetted against. The peer-advisories skill writes
# this file: agents/skills/peer-advisories/SKILL.md.
"""


class Problem(Exception):
    pass


def run(args, cwd=None):
    return subprocess.run(args, cwd=cwd, capture_output=True, text=True, check=False)


def git(root, *args):
    result = run(["git", "-C", str(root), *args])
    if result.returncode != 0:
        raise Problem(f"git {' '.join(args)}: {result.stderr.strip()}")
    return result.stdout.strip()


def gh(args):
    result = run(["gh", *args])
    if result.returncode != 0:
        raise RuntimeError((result.stderr or result.stdout).strip())
    return result.stdout


def undash(text):
    return text.replace(f" {EM_DASH} ", ", ").replace(EM_DASH, ", ")


def one_line(text, limit=240):
    text = undash(re.sub(r"\s+", " ", str(text or "")).strip())
    return text if len(text) <= limit else text[: limit - 3].rsplit(" ", 1)[0] + "..."


# The ledger.


def read_ledger(path):
    entries = {}
    if not path.exists():
        return entries
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split(maxsplit=5)
        if len(parts) < 5 or not GHSA.fullmatch(parts[0]) or parts[1] not in VERDICTS:
            raise Problem(f"{path}:{number}: not a ledger line: {line!r}")
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
        f"{ghsa} {e['verdict']} {e['issue']} {e['commit']} {e['date']} {one_line(e['reason'])}".rstrip()
        for ghsa, e in sorted(entries.items())
    ]
    path.write_text(HEADER + "".join(line + "\n" for line in lines), encoding="utf-8")


# Which commit is being vetted.


def main_ref(root):
    for ref in ("upstream/main", "origin/main", "main"):
        if run(["git", "-C", str(root), "rev-parse", "--verify", "--quiet", f"{ref}^{{commit}}"]).returncode == 0:
            return ref
    raise Problem("no upstream/main, origin/main or main to vet against")


def checked_commit(root):
    """The main commit this tree is, refusing a tree that differs from it anywhere but the ledger."""
    ref = main_ref(root)
    tip = git(root, "rev-parse", ref)
    remedy = f"fetch, then vet a tree at {ref}:\n  git worktree add --detach ../bravebot-advisories {ref}"
    if run(["git", "-C", str(root), "merge-base", "--is-ancestor", tip, "HEAD"]).returncode != 0:
        raise Problem(f"HEAD does not contain {ref} ({tip[:12]}); {remedy}")
    changed = [p for p in git(root, "diff", "--name-only", tip, "--").splitlines() if p != LEDGER]
    if changed:
        raise Problem(
            f"this tree differs from {ref} in {len(changed)} file(s) besides the ledger, "
            f"{changed[0]} first; a verdict has to be about main, so {remedy}"
        )
    return ref, tip


# Fetching and normalising advisories.


def sources():
    for repo, tool in REPOSITORIES:
        yield f"{repo} advisories", f"/repos/{repo}/security-advisories?per_page=100", tool
    for ecosystem, package, tool in PACKAGES:
        yield f"{ecosystem}:{package}", f"/advisories?affects={package}&ecosystem={ecosystem}&per_page=100", tool


def fetch(path):
    out = gh(["api", "--paginate", path, "--jq", ".[]"])
    return [json.loads(line) for line in out.splitlines() if line.strip()]


def normalise(raw, tool):
    """An advisory as this skill reads it, or None for one that is not a live published defect."""
    ghsa = str(raw.get("ghsa_id") or "")
    if not GHSA.fullmatch(ghsa):
        return None
    if raw.get("withdrawn_at") or raw.get("state", "published") != "published":
        return None
    if raw.get("type") == "malware":
        return None
    url = str(raw.get("html_url") or "")
    packages = set()
    for vulnerability in raw.get("vulnerabilities") or []:
        package = vulnerability.get("package") or {}
        if package.get("name"):
            packages.add(f"{package.get('ecosystem') or '?'}:{package['name']}")
    return {
        "ghsa_id": ghsa,
        "cve_id": raw.get("cve_id") if re.fullmatch(r"CVE-\d{4}-\d+", str(raw.get("cve_id") or "")) else None,
        "summary": str(raw.get("summary") or ""),
        "description": str(raw.get("description") or ""),
        "severity": str(raw.get("severity") or "unknown").lower(),
        "published_at": str(raw.get("published_at") or ""),
        "url": url if url.startswith("https://github.com/") else f"https://github.com/advisories/{ghsa}",
        "cwes": sorted({c["cwe_id"] for c in raw.get("cwes") or [] if CWE.fullmatch(str(c.get("cwe_id")))}),
        "packages": sorted(packages),
        "tools": [tool],
    }


def gather(fetched):
    """Every advisory once, from (source, tool, raw entries) triples."""
    merged = {}
    for _, tool, entries in fetched:
        for raw in entries:
            advisory = normalise(raw, tool)
            if not advisory:
                continue
            have = merged.get(advisory["ghsa_id"])
            if have is None:
                merged[advisory["ghsa_id"]] = advisory
                continue
            if tool not in have["tools"]:
                have["tools"].append(tool)
            if len(advisory["description"]) > len(have["description"]):
                have["description"] = advisory["description"]
            have["packages"] = sorted(set(have["packages"]) | set(advisory["packages"]))
            have["cwes"] = sorted(set(have["cwes"]) | set(advisory["cwes"]))
    return merged


def select(advisories, ledger, named, limit):
    """What this run vets: the ones named, else the unvetted, most severe and newest first."""
    if named:
        return [advisories[ghsa] for ghsa in named]
    left = [a for a in advisories.values() if ledger.get(a["ghsa_id"], {}).get("verdict") not in FINAL]
    # Newest first within a severity, then a stable sort puts severity over it and deferred last.
    left.sort(key=lambda a: a["published_at"], reverse=True)
    left.sort(key=lambda a: (a["ghsa_id"] in ledger, -SEVERITY_RANK.get(a["severity"], 0)))
    return left[:limit]


# Prompts.


def fenced(text):
    """Text in a code fence it cannot close, whatever backticks it holds."""
    longest = max((len(ticks) for ticks in re.findall(r"`+", text)), default=0)
    fence = "`" * max(3, longest + 1)
    return f"{fence}text\n{text.rstrip()}\n{fence}"


def render(template, values):
    # One pass, so a value holding `{{name}}` is left as written rather than expanded.
    return re.sub(r"\{\{(\w+)\}\}", lambda m: values[m.group(1)], template)


def advisory_block(advisory):
    return fenced(f"{advisory['summary'].strip()}\n\n{advisory['description'].strip()}")


def advisory_facts(advisory):
    return "\n".join(
        [
            f"- Id: `{advisory['ghsa_id']}`" + (f" ({advisory['cve_id']})" if advisory["cve_id"] else ""),
            f"- Tool: {', '.join(advisory['tools'])}",
            f"- Severity the vendor gave it: {advisory['severity']}",
            f"- Weakness: {', '.join(advisory['cwes']) or 'none given'}",
            f"- Published: {advisory['published_at'][:10] or 'unknown'}",
            f"- Advisory: {advisory['url']}",
        ]
    )


def tracker(path):
    """Every issue and pull request on the tracker as `number state title` lines."""
    rows = []
    for kind in ("issue", "pr"):
        listed = json.loads(
            gh([kind, "list", "--repo", REPO, "--state", "all", "--limit", "5000", "--json", "number,state,title"])
        )
        rows += [(one["number"], one["state"].lower(), one_line(one["title"], 300)) for one in listed]
    path.write_text("".join(f"{n}\t{state}\t{title}\n" for n, state, title in sorted(rows)), encoding="utf-8")
    return len(rows)


def pending(args):
    root = Path(args.root)
    ledger = read_ledger(root / LEDGER)
    ref, tip = checked_commit(root)

    fetched, failed = [], []
    for name, path, tool in sources():
        try:
            fetched.append((name, tool, fetch(path)))
        except (RuntimeError, ValueError) as problem:
            failed.append(name)
            print(f"warning: could not read {name}: {one_line(problem, 160)}", file=sys.stderr)
    if not fetched:
        raise Problem("no advisory source could be read")
    advisories = gather(fetched)

    named = list(dict.fromkeys(args.ids))
    unknown = [ghsa for ghsa in named if ghsa not in advisories]
    if unknown:
        raise Problem(f"no source lists {', '.join(unknown)}, or it is withdrawn")
    chosen = select(advisories, ledger, named, args.max)

    work = Path(tempfile.mkdtemp(prefix="peer-advisories-"))
    (work / "vet").mkdir()
    tracker_file = work / "tracker.tsv"
    try:
        count = tracker(tracker_file)
    except (RuntimeError, ValueError) as problem:
        raise Problem(f"could not list the issues on {REPO}: {one_line(problem, 160)}") from problem

    template = (HERE / "vet.md").read_text(encoding="utf-8")
    vet = []
    for advisory in chosen:
        ghsa = advisory["ghsa_id"]
        prompt, result = work / "vet" / f"{ghsa}.md", work / "vet" / f"{ghsa}.json"
        prompt.write_text(
            render(
                template,
                {
                    "ghsa_id": ghsa,
                    "facts": advisory_facts(advisory),
                    "advisory": advisory_block(advisory),
                    "root": str(root),
                    "commit": tip,
                    "ref": ref,
                    "tracker": str(tracker_file),
                    "results_file": str(result),
                },
            ),
            encoding="utf-8",
        )
        vet.append({"id": ghsa, "prompt_file": str(prompt), "results_file": str(result)})

    manifest = {
        "root": str(root),
        "ref": ref,
        "commit": tip,
        "advisories": {a["ghsa_id"]: a for a in chosen},
        "vet": vet,
    }
    (work / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    vetted = sum(1 for ghsa in advisories if ledger.get(ghsa, {}).get("verdict") in FINAL)
    print(
        f"{len(advisories)} advisories from {len(fetched)} sources"
        + (f" ({len(failed)} unread)" if failed else "")
        + f", {vetted} vetted already, {len(chosen)} to vet against {ref} {tip[:12]};"
        f" {count} issues and pull requests listed",
        file=sys.stderr,
    )
    print(json.dumps({"work_dir": str(work), "vet": [{"id": v["id"], "prompt_file": v["prompt_file"]} for v in vet]}))
    return 0


# Reading what the agents wrote.


def read_json(path):
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    return data if isinstance(data, dict) else None


def issue_number(value):
    return value if isinstance(value, int) and not isinstance(value, bool) and value > 0 else None


def check_issue(issue):
    """Why these issue fields cannot be drafted, or None."""
    if not isinstance(issue, dict):
        return "no issue fields"
    missing = [field for field in ISSUE_TEXT if not str(issue.get(field) or "").strip()]
    if missing:
        return f"no {', '.join(missing)}"
    kinds = issue.get("kind")
    kinds = [kinds] if isinstance(kinds, str) else kinds
    if not kinds or not all(kind in KINDS for kind in kinds):
        return f"kind {issue.get('kind')!r} is not among {', '.join(KINDS)}"
    if not isinstance(issue.get("security"), bool):
        return "security is not true or false"
    if issue["security"] and issue.get("severity") not in SEVERITIES:
        return f"security with severity {issue.get('severity')!r}"
    return None


def load_vet(path, ghsa):
    data = read_json(path)
    if data is None:
        return None, "no readable result"
    if data.get("ghsa_id") != ghsa:
        return None, "the result names another advisory"
    if data.get("verdict") not in VERDICTS:
        return None, f"verdict {data.get('verdict')!r} is not among {', '.join(VERDICTS)}"
    if not one_line(data.get("reason")):
        return None, "no reason"
    if data.get("existing_issue") is not None and issue_number(data["existing_issue"]) is None:
        return None, "existing_issue is not an issue number"
    if data["verdict"] == "affected" and data.get("existing_issue") is None:
        problem = check_issue(data.get("issue"))
        if problem:
            return None, f"affected, but {problem}"
    return data, None


def load_verify(path, ghsa):
    data = read_json(path)
    if data is None:
        return None, "no readable verdict"
    if data.get("ghsa_id") != ghsa:
        return None, "the verdict names another advisory"
    if data.get("verdict") not in VERIFY_VERDICTS:
        return None, f"verdict {data.get('verdict')!r} is not among {', '.join(VERIFY_VERDICTS)}"
    if not one_line(data.get("reason")):
        return None, "no reason"
    if data.get("existing_issue") is not None and issue_number(data["existing_issue"]) is None:
        return None, "existing_issue is not an issue number"
    if data.get("severity") is not None and data["severity"] not in SEVERITIES:
        return None, f"severity {data['severity']!r} is not among {', '.join(SEVERITIES)}"
    if data.get("security") is not None and not isinstance(data["security"], bool):
        return None, "security is not true or false"
    return data, None


def manifest_of(work):
    manifest = read_json(Path(work) / "manifest.json")
    if manifest is None:
        raise Problem(f"{work} has no manifest.json; run pending first")
    return manifest


def verify_paths(work, ghsa):
    return Path(work) / "verify" / f"{ghsa}.md", Path(work) / "verify" / f"{ghsa}.json"


def outcomes(work):
    """Per advisory: what the agents decided, as {state, verdict, issue, reason, fields}.

    `state` is `final` for a verdict the ledger can take, `file` for a confirmed defect nothing on
    the tracker holds yet, `verify` for one no verifier has answered, and `pending` for a result
    that cannot be read.
    """
    manifest = manifest_of(work)
    decided = {}
    for entry in manifest["vet"]:
        ghsa = entry["id"]
        vet, problem = load_vet(entry["results_file"], ghsa)
        if vet is None:
            decided[ghsa] = {"state": "pending", "reason": problem}
            continue
        existing = vet.get("existing_issue")
        if vet["verdict"] != "affected" or existing is not None:
            decided[ghsa] = {"state": "final", "verdict": vet["verdict"], "issue": existing, "reason": vet["reason"]}
            continue
        check, problem = load_verify(verify_paths(work, ghsa)[1], ghsa)
        if check is None:
            decided[ghsa] = {"state": "verify", "reason": problem, "vet": vet}
            continue
        if check["verdict"] != "confirmed":
            decided[ghsa] = {"state": "final", "verdict": check["verdict"], "issue": None, "reason": check["reason"]}
            continue
        if check.get("existing_issue") is not None:
            decided[ghsa] = {"state": "final", "verdict": "affected", "issue": check["existing_issue"], "reason": vet["reason"]}
            continue
        fields = dict(vet["issue"])
        if check.get("security") is False:
            fields["security"] = False
        if fields["security"] and SEVERITIES.index(check.get("severity") or "high") > SEVERITIES.index(fields["severity"]):
            fields["severity"] = check["severity"]
        decided[ghsa] = {"state": "file", "reason": vet["reason"], "fields": fields, "confirmed": check["reason"]}
    return manifest, decided


def verify(args):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    template = (HERE / "verify.md").read_text(encoding="utf-8")
    (work / "verify").mkdir(exist_ok=True)
    verifiers = []
    for ghsa, outcome in decided.items():
        if outcome["state"] != "verify":
            print(f"  {ghsa}  {outcome.get('verdict') or outcome['state']}: {one_line(outcome['reason'], 100)}", file=sys.stderr)
            continue
        prompt, result = verify_paths(work, ghsa)
        claim = {k: outcome["vet"].get(k) for k in ("reason", "evidence", "issue")}
        prompt.write_text(
            render(
                template,
                {
                    "ghsa_id": ghsa,
                    "facts": advisory_facts(manifest["advisories"][ghsa]),
                    "advisory": advisory_block(manifest["advisories"][ghsa]),
                    "claim": fenced(json.dumps(claim, indent=2)),
                    "root": manifest["root"],
                    "commit": manifest["commit"],
                    "results_file": str(result),
                },
            ),
            encoding="utf-8",
        )
        verifiers.append({"id": ghsa, "prompt_file": str(prompt)})
        print(f"  {ghsa}  affected, to verify", file=sys.stderr)
    print(json.dumps({"work_dir": str(work), "verify": verifiers}))
    return 0


# Drafting.


def unmention(text):
    """`@name` outside code wrapped in backticks, so a draft pings nobody."""
    parts = re.split(r"(```.*?```|`[^`\n]*`)", text, flags=re.DOTALL)
    for i in range(0, len(parts), 2):
        parts[i] = re.sub(r"(?<![\w`])@([A-Za-z0-9][\w-]*(?:/[\w.-]+)?)", r"`@\1`", parts[i])
    return "".join(parts)


def clean(text, root):
    text = undash(str(text).strip())
    text = text.replace(str(root) + "/", "").replace(str(root), ".")
    return unmention(text)


def labels_for(fields):
    kinds = fields["kind"]
    labels = [kinds] if isinstance(kinds, str) else list(kinds)
    if fields["security"]:
        labels += ["security", "needs-security-review", f"severity/{fields['severity']}"]
    area = str(fields.get("area") or "").strip().removeprefix("area/")
    if area in AREAS:
        labels.append(f"area/{area}")
    elif area == "infrastructure":
        labels.append("infrastructure")
    return sorted(set(labels))


def title_for(fields):
    title = one_line(fields["title"], 1000).replace("`", "").rstrip(".")
    if len(title) > TITLE_LIMIT:
        title = title[:TITLE_LIMIT].rsplit(" ", 1)[0].rstrip(" ,;:")
    return title


def body_for(advisory, fields, commit, root):
    source = f"[{advisory['ghsa_id']}]({advisory['url']})"
    if advisory["cve_id"]:
        source += f" ({advisory['cve_id']})"
    sections = [
        clean(fields["what_happens"], root),
        "## Where\n\n" + clean(fields["code_path"], root),
        "## Reproduce\n\n" + clean(fields["reproduce"], root),
        "## Fix\n\n" + clean(fields["fix"], root),
        "## Where this comes from\n\n"
        f"The same class of defect was reported against {' and '.join(advisory['tools'])} as {source}. "
        f"The `peer-advisories` skill vetted it against `{commit[:12]}`, and a second pass tried to "
        "disprove it before it was filed. Filed by a tool rather than a person, so no `importance`, "
        "`urgency` or `size` has been judged.",
    ]
    return "\n\n".join(sections) + "\n"


def draft(args):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    out = work / "issues"
    out.mkdir(exist_ok=True)
    drafts = []
    for ghsa, outcome in decided.items():
        if outcome["state"] != "file":
            continue
        fields = outcome["fields"]
        body = out / f"{ghsa}.md"
        body.write_text(
            body_for(manifest["advisories"][ghsa], fields, manifest["commit"], manifest["root"]), encoding="utf-8"
        )
        title = title_for(fields)
        drafts.append(
            {
                "id": ghsa,
                "title": title,
                "labels": labels_for(fields),
                "key": one_line(fields.get("key"), 80) if one_line(fields.get("key")).lower() in title.lower() else "",
                "body_file": str(body),
            }
        )
        print(f"  {ghsa}  {title}\n{'':21}{', '.join(labels_for(fields))}")
    (work / "drafts.json").write_text(json.dumps(drafts, indent=2), encoding="utf-8")
    waiting = [ghsa for ghsa, o in decided.items() if o["state"] in ("verify", "pending")]
    print(f"{len(drafts)} drafted" + (f", {len(waiting)} undecided: {', '.join(waiting)}" if waiting else ""))
    return 0


# Posting.


def load_poster():
    spec = importlib.util.spec_from_file_location("audit_post", HERE.parent / "security-audit" / "post-issues.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def cited_by(poster, repo, ghsa):
    """The issue whose body already names this advisory, or None."""
    found = json.loads(
        poster.gh(
            ["issue", "list", "--state", "all", "--limit", "5", "--search", f'"{ghsa}" in:body',
             "--json", "number,title,state,url"],
            repo,
        )
    )
    return found[0] if found else None


def post(args, poster=None):
    poster = poster or load_poster()
    work = Path(args.work_dir)
    drafts = read_json_list(work / "drafts.json")
    if not drafts:
        print("no drafts to post")
        return 0
    filed_path = work / "filed.json"
    filed = read_json(filed_path) or {}

    wanted = sorted({label for d in drafts for label in d["labels"]})
    try:
        missing = [label for label in wanted if label not in poster.existing_labels(args.repo)]
    except RuntimeError as problem:
        print(f"could not read the labels of {args.repo}: {problem}", file=sys.stderr)
        return 2
    if missing:
        print(f"{args.repo} has no label {', '.join(missing)}. Nothing was posted; creating one is", file=sys.stderr)
        print("a person's call:", file=sys.stderr)
        for label in missing:
            print(f"  gh label create {label} --repo {args.repo} --description ... --color ...", file=sys.stderr)
        return 2
    if args.assignee and not poster.assignable(args.repo, args.assignee):
        print(f"{args.repo} would not take {args.assignee} as an assignee. Nothing was posted.", file=sys.stderr)
        return 2

    posted, held, left = 0, 0, []
    for d in drafts:
        if d["id"] in filed:
            print(f"  done   {d['id']}  #{filed[d['id']]['issue']}")
            continue
        if posted >= args.max:
            left.append(d)
            continue
        try:
            duplicate = cited_by(poster, args.repo, d["id"]) or poster.already_filed(args.repo, d)
        except (RuntimeError, ValueError) as problem:
            print(f"could not search {args.repo}: {problem}", file=sys.stderr)
            return 2
        if duplicate:
            held += 1
            print(f"  skip   {d['id']}  #{duplicate['number']} holds it ({duplicate['state'].lower()})")
            if not args.dry_run:
                filed[d["id"]] = {"issue": duplicate["number"], "how": "existing"}
                filed_path.write_text(json.dumps(filed, indent=2), encoding="utf-8")
            continue
        if args.dry_run:
            posted += 1
            print(f"  would  {d['id']}  {d['title']}\n{'':9}{', '.join(d['labels'])}")
            continue
        if posted:
            time.sleep(args.pace + random.uniform(*poster.JITTER))
        try:
            url = poster.post(args.repo, d, args.assignee)
        except RuntimeError as problem:
            print(f"could not file {d['id']}: {problem}", file=sys.stderr)
            return 1
        number = re.search(r"/issues/(\d+)$", url)
        if not number:
            print(f"filed {d['id']} but could not read an issue number from {url!r}", file=sys.stderr)
            return 1
        posted += 1
        filed[d["id"]] = {"issue": int(number.group(1)), "how": "filed"}
        filed_path.write_text(json.dumps(filed, indent=2), encoding="utf-8")
        print(f"  filed  {d['id']}  {url}")

    print(f"{posted} {'would be filed' if args.dry_run else 'filed'}, {held} already on the tracker")
    if left:
        print(f"{len(left)} not attempted, at the cap of {args.max}: {', '.join(d['id'] for d in left)}")
    return 0


def read_json_list(path):
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return []
    return data if isinstance(data, list) else []


# Recording.


def record(args, today=None):
    work = Path(args.work_dir)
    manifest, decided = outcomes(work)
    filed = read_json(work / "filed.json") or {}
    path = Path(manifest["root"]) / LEDGER
    ledger = read_ledger(path)
    today = today or datetime.date.today().isoformat()
    written, waiting = 0, 0
    for ghsa, outcome in decided.items():
        if outcome["state"] == "file" and ghsa in filed:
            outcome = {"state": "final", "verdict": "affected", "issue": filed[ghsa]["issue"], "reason": outcome["reason"]}
        if outcome["state"] != "final":
            waiting += 1
            why = "confirmed and not filed" if outcome["state"] == "file" else outcome["reason"]
            print(f"  left   {ghsa}  {why}")
            continue
        issue = f"#{outcome['issue']}" if outcome.get("issue") else "-"
        ledger[ghsa] = {
            "verdict": outcome["verdict"],
            "issue": issue,
            "commit": manifest["commit"][:12],
            "date": today,
            "reason": outcome["reason"],
        }
        written += 1
        print(f"  {outcome['verdict']:<8} {ghsa}  {issue}  {one_line(outcome['reason'], 90)}")
    if not args.dry_run and written:
        write_ledger(path, ledger)
    verb = "would record" if args.dry_run else "recorded"
    print(f"{verb} {written} in {LEDGER}, {waiting} left for the next run")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)

    one = commands.add_parser("pending", help="fetch the advisories and write a prompt per unvetted one")
    one.add_argument("ids", nargs="*", metavar="GHSA-ID", help="vet these, whatever the ledger says")
    one.add_argument("--max", type=int, default=MAX)
    one.add_argument("--root", default=str(ROOT))

    for name, text in (("verify", "write a verifier prompt per affected verdict"),
                       ("draft", "write an issue body per confirmed defect")):
        commands.add_parser(name, help=text).add_argument("--work-dir", required=True)

    one = commands.add_parser("post", help="file the drafts the tracker does not hold")
    one.add_argument("--work-dir", required=True)
    one.add_argument("--repo", default=REPO)
    one.add_argument("--dry-run", action="store_true")
    one.add_argument("--assignee")
    one.add_argument("--max", type=int, default=12)
    one.add_argument("--pace", type=float, default=10.0)

    one = commands.add_parser("record", help="write the decided verdicts into the ledger")
    one.add_argument("--work-dir", required=True)
    one.add_argument("--dry-run", action="store_true")

    args = parser.parse_args(argv)
    for ghsa in getattr(args, "ids", []):
        if not GHSA.fullmatch(ghsa):
            parser.error(f"{ghsa!r} is not a GHSA id")
    try:
        return {"pending": pending, "verify": verify, "draft": draft, "post": post, "record": record}[args.command](args)
    except Problem as problem:
        print(f"peer-advisories: {problem}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
