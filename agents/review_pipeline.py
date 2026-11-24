#!/usr/bin/env python3
"""The parts of a reviewed-findings pipeline that do not depend on what is scanned.

The maintenance and profile skills share one shape: a scan ranks candidates, reviewers read the code
behind each and write verdicts to files, a collector merges them, drafts become issues a person
chooses from, and a ledger records what was read. What those steps do with git, with the files a
ledger names and with the lines a draft cites does not depend on the scan, so it is here and both
skills load it by path, the way they load `work_dir.py`.
"""

import hashlib
import importlib.util
import json
import re
import subprocess  # nosemgrep: gitlab.bandit.B404
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


drafting = _load("check_spec_draft_issues", HERE / "skills" / "check-spec" / "draft-issues.py")
TITLE_LIMIT = drafting.TITLE_LIMIT


NEAR = 3

FUNCTION = re.compile(r"\b(?:fn|def)\s+(\w+)|\b([A-Za-z]\w*)\(|\b([a-z][a-z0-9]*(?:_[a-z0-9]+)+)\b")

CITE = re.compile(r"((?:[\w.@-]+/)+[\w.@-]+\.\w+):(\d+)(?:-(\d+))?")

HUNK = re.compile(r"^@@ -(\d+)(?:,(\d+))? \+\d+(?:,(\d+))? @@", re.MULTILINE)

READING_PINNED = """\
## Where to read

The code is at `{tree}`, a checkout of commit `{commit}` made for this run. Read it there and cite
`file:line` from it, so each line number refers to that one commit however far `main` moves while
you work. It holds tracked files only, so `git grep -n NAME` searches all of it in a second and a
plain `grep -r` is as fast. Do not search `{root}`: it holds `target/` and `node_modules/`, and a
`grep -r` over them runs past the tool's time limit, which moves it to the background. History is in
`{root}`: `git -C {root} log -- PATH`.
"""

READING_LIVE = """\
## Where to read

The code is in `{root}`, your working directory. Search it with `git grep -n NAME`, which reads
tracked files only. A plain `grep -r .` also walks `target/` and `node_modules/`, runs past the
tool's time limit, and is moved to the background.
"""

def short_hash(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:12]

def ledger_field(text, also=""):
    """Text for one whitespace-separated ledger field, without loss: `%`, whitespace and the
    characters in `also` become `%XX` per UTF-8 byte, so two names that differ only there keep two keys."""
    return re.sub(
        r"[%\s" + re.escape(also) + r"]",
        lambda found: "".join("%%%02X" % b for b in found.group(0).encode("utf-8", "surrogateescape")),
        text,
    )

def ledger_value(field):
    return re.sub(
        r"(?:%[0-9A-F]{2})+",
        lambda found: bytes.fromhex(found.group(0).replace("%", "")).decode("utf-8", "surrogateescape"),
        field,
    )

def digest_of(sources, paths, cache):
    """A short hash of the text of the files at `paths`: what a recorded verdict was given for."""
    parts = []
    for path in sorted(set(paths)):
        if path not in cache:
            source = sources.get(path)
            text = source.text if source else ""
            cache[path] = hashlib.sha256(text.encode("utf-8")).hexdigest()
        parts.append(f"{path}:{cache[path]}")
    return hashlib.sha256("|".join(parts).encode("utf-8")).hexdigest()[:16]

def git(root, *args):
    # The program and arguments are fixed here or come from git itself; no shell reads them.
    return subprocess.run(  # nosemgrep: gitlab.bandit.B603
        ["git", "-C", str(root), *args], capture_output=True, text=True, check=False
    )

def read_commit(root):
    """The commit `root` is on, or None where it has none."""
    done = git(root, "rev-parse", "--verify", "-q", "HEAD")
    return done.stdout.strip() if done.returncode == 0 else None

def has_uncommitted_changes(root):
    """Whether a tracked file differs from the commit it is on."""
    return bool(git(root, "status", "--porcelain", "--untracked-files=no").stdout.strip())

def make_snapshot(root, commit, tree):
    """Check `commit` out at `tree`, detached. Returns None when that worked, else what git said.
    Reviewers read it in place of the working tree, which other sessions move while a run is under
    way, so every line they cite is a line of one commit. Hooks are off because this repository's
    post-checkout hook copies `.envrc` and runs `direnv allow`, which a checkout made for reading
    has no use for. A checkout deleted by hand is still registered in the repository, and git
    refuses to add one at a path it holds, so registrations whose directory is gone are pruned
    first, whether or not `tree` exists."""
    git(root, "worktree", "prune")
    if tree.exists():
        remove_snapshot(root, tree)
    done = git(root, "-c", "core.hooksPath=/dev/null", "worktree", "add", "--detach", str(tree), commit)
    return None if done.returncode == 0 else done.stderr.strip() or f"git exited {done.returncode}"

def remove_snapshot(root, tree):
    """Remove the checkout `make_snapshot` made, and forget it in the repository."""
    git(root, "worktree", "remove", "--force", str(tree))
    git(root, "worktree", "prune")
    return not Path(tree).exists()

def reading_for(root, tree, commit):
    if tree is None:
        return READING_LIVE.format(root=root)
    return READING_PINNED.format(root=root, tree=tree, commit=commit)

def cited_ranges(items):
    """The `(path, first line, last line)` each place in `items` cites."""
    return [(path, int(first), int(last or first)) for path, first, last in CITE.findall(" ".join(items))]

def named_functions(items):
    """The function names the evidence text uses: `fn name`, `def name`, `name(` or a snake_case word.
    The cited places are taken out first, so a file name is not read as a function."""
    text = CITE.sub(" ", " ".join(items))
    return {next(g for g in found if g) for found in FUNCTION.findall(text)}

def lines_cited(ranges, files):
    """How many distinct lines `ranges` cites in `files`."""
    return len({(path, n) for path, first, last in ranges if path in files for n in range(first, last + 1)})

def overlap_of(mine, theirs):
    """Whether two drafts' evidence proposes one change: both cite one file within NEAR lines of each
    other, or both name one function. Then whether `mine` cites `more`, `fewer` or `as many` of the
    lines in the files both cite; None where they do not overlap."""
    (places, names), (other_places, other_names) = mine, theirs
    near = any(
        path == other and first <= last_other + NEAR and first_other <= last + NEAR
        for path, first, last in places
        for other, first_other, last_other in other_places
    )
    if not near and not names & other_names:
        return None
    shared = {p for p, _, _ in places} & {p for p, _, _ in other_places}
    ahead, behind = lines_cited(places, shared), lines_cited(other_places, shared)
    return "more" if ahead > behind else "fewer" if ahead < behind else "as many"

def hunk_count(text):
    return int(text) if text else 1

def changes_to(root, commit, path):
    """How `path` changed from `commit` to HEAD, as `(first old line, old lines, new lines)` for each
    hunk, or None where the file is gone. A file that did not change has no hunks."""
    if git(root, "cat-file", "-e", f"HEAD:{path}").returncode != 0:
        return None
    diff = git(root, "diff", "--no-renames", "-U0", commit, "HEAD", "--", path).stdout
    return [(int(old), hunk_count(count), hunk_count(new)) for old, count, new in HUNK.findall(diff)]

def line_after(hunks, line):
    """Where an old line is after `hunks`, or None where the line is one of those they changed."""
    offset = 0
    for old, count, new in hunks:
        first = old if count else old + 1
        if count and first <= line < first + count:
            return None
        if line < first:
            break
        offset += new - count
    return line + offset

def verify(manifest, out):
    """The places the drafts in `out` cite whose lines differ between the commit the reviewers read
    and HEAD: moved to a line that is known, in code that changed, or in a file that is gone. None
    where the manifest names no commit."""
    root, commit = manifest["root"], manifest.get("commit")
    if commit is None:
        return None
    known, drafts = {}, []
    for body in sorted(Path(out).glob("*.md")):
        cites, placed = [], set()
        for path, first, last in CITE.findall(body.read_text(encoding="utf-8")):
            place = f"{path}:{first}" + (f"-{last}" if last else "")
            if place in placed:
                continue
            placed.add(place)
            if path not in known:
                exists = git(root, "cat-file", "-e", f"{commit}:{path}").returncode == 0
                known[path] = changes_to(root, commit, path) if exists else False
            hunks = known[path]
            if hunks is False or hunks == []:
                continue
            if hunks is None:
                cites.append({"place": place, "state": "gone"})
                continue
            now = [line_after(hunks, int(n)) for n in (first, last or first)]
            if None in now:
                cites.append({"place": place, "state": "changed"})
            elif now != [int(first), int(last or first)]:
                cites.append({"place": place, "state": "moved", "now": f"{now[0]}" + (f"-{now[1]}" if last else "")})
        if cites:
            drafts.append({"draft": body.name, "cites": cites})
    return drafts

def render_verify(manifest, moved):
    commit = manifest.get("commit")
    if commit is None:
        return "the manifest names no commit, so there is nothing to compare the cited lines with"
    head = read_commit(manifest["root"])
    lines = [f"lines cited from commit {commit[:12]}, compared with {head[:12] if head else 'HEAD'}"]
    if not moved:
        lines.append("every cited line stands")
    for entry in moved:
        lines.append(entry["draft"])
        for cite in entry["cites"]:
            what = {"moved": f"moved to {cite.get('now')}", "changed": "is in changed code", "gone": "is gone"}
            lines.append(f"    {cite['place']} {what[cite['state']]}")
    lines.append("Line numbers written without a path are not checked here: the claim check's readers read them.")
    return "\n".join(lines)


# Which verdicts `record` writes down, and the file it writes them to.
RECORDED = ("keep", "cleanup")


DRAFTS_FILE = "drafts.json"


# A reader checks the claims of this many drafts, taken in file order so that drafts about one file
# mostly go to one reader.
CLAIMS_DIR = "claims"


CLAIM_GROUP = 5


CLAIM_VERDICTS = ("holds", "rejected", "unclear")


HEAD_TREE = "head"


def incomplete(lane, ids, summary, evidence, waiting=False):
    """A gap in a review. `waiting` marks the gaps that mean the reviewer has not finished: no result,
    or one that does not give a verdict for every candidate it was asked about."""
    return {
        "lane": lane, "kind": "review-incomplete", "candidates": ids, "summary": summary,
        "evidence": [evidence], "waiting": waiting,
    }


def load_reviews(manifest, change="cleanup"):
    """Every finding the reviewers wrote, plus one per gap in what they were asked to cover. `change`
    is the verdict that says a change is worth making, which has to come with a finding."""
    findings, verdicts, problems = [], {}, []
    for entry in manifest["lanes"]:
        lane = entry["lane"]
        for prompt in entry["reviewer_prompts"]:
            path = Path(prompt["results_file"])
            asked = list(prompt["candidates"])
            if not path.exists():
                problems.append(
                    incomplete(lane, asked, "no reviewer result: these candidates were never read", str(path), True)
                )
                continue
            try:
                result = json.loads(path.read_text(encoding="utf-8"))
                given = {c["id"]: c for c in result.get("candidates", []) if isinstance(c, dict) and "id" in c}
                written = [f for f in result.get("findings", []) if isinstance(f, dict)]
            except (json.JSONDecodeError, OSError, AttributeError, TypeError, KeyError) as problem:
                problems.append(
                    {"lane": lane, "kind": "review-unreadable", "candidates": asked, "waiting": True,
                     "summary": f"reviewer result could not be read: {problem}", "evidence": [str(path)]}
                )
                continue
            missing = [i for i in asked if i not in given]
            if missing:
                problems.append(
                    incomplete(lane, missing, "the reviewer returned no verdict for these candidates", str(path), True)
                )
            covered = set()
            for finding in written:
                named = [i for i in finding.get("candidates", []) if i in asked]
                covered.update(named)
                findings.append(dict(finding, lane=lane, candidates=named, kind="finding"))
            for one in asked:
                if one in given:
                    verdicts[one] = given[one]
            orphans = [i for i in asked if given.get(i, {}).get("verdict") == change and i not in covered]
            if orphans:
                problems.append(
                    incomplete(lane, orphans, f"called {change}, but no finding says what the change is", str(path))
                )
    return findings, verdicts, problems


def render_review(manifest, findings, verdicts, problems, change="cleanup"):
    lines = []
    for entry in manifest["lanes"]:
        lane = entry["lane"]
        mine = [f for f in findings if f["lane"] == lane]
        counted = Counter(
            verdicts[i].get("verdict", "unclear")
            for p in entry["reviewer_prompts"]
            for i in p["candidates"]
            if i in verdicts
        )
        lines.append("")
        lines.append(
            f"{lane}: {entry['candidates']} candidates, "
            + ", ".join(f"{counted[v]} {v}" for v in (change, "keep", "unclear"))
            + f", {len(mine)} findings"
        )
        for finding in mine:
            mark = "bug  " if finding.get("bug") else "clean"
            named = ", ".join(finding["candidates"]) or "found while reading"
            lines.append(f"  {mark}  {finding.get('summary') or '(no summary)'} [{named}]")
            for item in finding.get("evidence", [])[:3]:
                lines.append(f"         at {item}")
            if finding.get("fix"):
                lines.append(f"         fix: {finding['fix']}")
        for one in (p for p in problems if p["lane"] == lane):
            lines.append(f"  error  {one['kind']}: {one['summary']} [{', '.join(one['candidates'])}]")
            lines.append(f"         at {one['evidence'][0]}")
        for p in entry["reviewer_prompts"]:
            for i in p["candidates"]:
                if verdicts.get(i, {}).get("verdict") == "unclear":
                    lines.append(f"  warn   {i} unclear: {verdicts[i].get('reason', '')}")
    lines.append("")
    lines.append(
        f"{sum(e['candidates'] for e in manifest['lanes'])} candidates, {len(findings)} findings, "
        f"{len(problems)} gaps in the review"
    )
    return "\n".join(lines)


def mark_overlaps(drafts, findings):
    """Add `overlaps` to each draft: the other drafts, by their number in the table, that propose
    the same change. Reviewers work one lane at a time, so only this sees two lanes agree."""
    evidence = []
    for finding in findings:
        items = drafting.evidence_items(finding.get("evidence"))
        evidence.append((cited_ranges(items), named_functions(items)))
    for number, entry in enumerate(drafts):
        entry["overlaps"] = [
            {"draft": other + 1, "cites": how}
            for other, theirs in enumerate(evidence)
            if other != number and (how := overlap_of(evidence[number], theirs))
        ]


def render_drafts(drafts, out, claims=None):
    """The table a person chooses from. `claims` is what `read_claims` returned, or None before the
    claims have been checked: a draft a reader rejected says why, and so does one it could not decide."""
    lines = []
    for number, entry in enumerate(drafts, start=1):
        lines.append(f"  {number:>2}. {entry['title']}")
        lines.append(f"      {' '.join(entry['labels'])}, assignee {entry['assignee']}, body {entry['body_file']}")
        for one in entry["overlaps"]:
            says = "" if one["cites"] == "as many" else f", and cites {one['cites']} of its lines"
            lines.append(f"      overlaps {one['draft']}{says}")
        if entry["title_too_long"]:
            lines.append(
                f"      needs a shorter title: {len(entry['title'])} characters, and GitHub takes {TITLE_LIMIT}"
            )
        if claims is not None:
            lines += claim_lines(claims.get(entry["slug"]))
    lines.append("")
    counted = f"{len(drafts)} draft" + ("" if len(drafts) == 1 else "s")
    lines.append(f"{counted} in {out}. Nothing has been posted.")
    return "\n".join(lines)


def claim_lines(verdict):
    """What the table says under a draft about the reader's check of its claim."""
    if verdict is None:
        return ["      claim not checked: no reader gave a verdict"]
    lines = []
    if verdict["verdict"] == "rejected":
        lines.append(f"      REJECTED by the claim check: {verdict['reason']}")
    elif verdict["verdict"] == "unclear":
        lines.append(f"      claim check could not decide: {verdict['reason']}")
    for fix in verdict["corrections"]:
        lines.append(f"      line numbers: {fix['written']} is {fix['now']} at HEAD")
    return lines


def claim_drafts(out):
    """The drafts `draft` wrote to `out`, or None where it has not run."""
    path = Path(out) / DRAFTS_FILE
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else None


def claim_readers(drafts):
    """The drafts split among readers: at most `CLAIM_GROUP` each, in the order of the file the body
    cites first, so that the drafts about one file go to one reader unless a boundary falls between them."""
    def first_file(entry):
        match = CITE.search(Path(entry["body_file"]).read_text(encoding="utf-8"))
        return match.group(1) if match else ""

    ordered = sorted(drafts, key=lambda entry: (first_file(entry), entry["slug"]))
    return [ordered[i : i + CLAIM_GROUP] for i in range(0, len(ordered), CLAIM_GROUP)]


def read_claims(work_dir, drafts=None):
    """What the readers decided, as `(verdicts by draft slug, problems)`, or None where
    `claims_prepare` has not run. A draft is missing from the verdicts when its reader wrote no
    readable result, gave it no verdict, or rejected it with no reason."""
    index_file = Path(work_dir) / CLAIMS_DIR / "index.json"
    if not index_file.exists():
        return None
    verdicts, problems = {}, []
    for reader in json.loads(index_file.read_text(encoding="utf-8"))["readers"]:
        path = Path(reader["results_file"])
        if not path.exists():
            problems.append(f"{path.name}: no result, so these drafts were never checked: {', '.join(reader['drafts'])}")
            continue
        try:
            given = {
                d["draft"]: d for d in json.loads(path.read_text(encoding="utf-8"))["drafts"] if isinstance(d, dict)
            }
        except (json.JSONDecodeError, OSError, KeyError, TypeError) as problem:
            problems.append(f"{path.name}: the result could not be read: {problem}")
            continue
        for slug in reader["drafts"]:
            entry = given.get(slug)
            if entry is None or entry.get("verdict") not in CLAIM_VERDICTS:
                problems.append(f"{path.name}: no verdict for {slug}")
                continue
            reason = str(entry.get("reason") or "").strip()
            if entry["verdict"] != "holds" and not reason:
                problems.append(f"{path.name}: {slug} is {entry['verdict']} with no reason")
                continue
            fixes = [
                {"written": str(c.get("written", "")), "now": str(c.get("now", ""))}
                for c in entry.get("corrections") or []
                if isinstance(c, dict) and c.get("written") and c.get("now")
            ]
            verdicts[slug] = {"verdict": entry["verdict"], "reason": reason, "corrections": fixes}
    return verdicts, problems


def reviewer_status(manifest, problems):
    """One `(reviewer, why it is not done or None)` row per reviewer, from the gaps `load_reviews`
    found. A reviewer is done when its result parses and gives a verdict for every candidate it was
    asked about. Whether the findings are complete is for `collect` to say."""
    waiting = {p["evidence"][0]: p["summary"] for p in problems if p.get("waiting")}
    return [
        (Path(prompt["results_file"]).name.removesuffix("_results.json"), waiting.get(prompt["results_file"]))
        for entry in manifest["lanes"]
        for prompt in entry["reviewer_prompts"]
    ]


def render_status(rows):
    done = sum(1 for _, why in rows if why is None)
    if done == len(rows):
        return f"every reviewer has written a readable result ({done} of {len(rows)})"
    lines = [f"{done} of {len(rows)} reviewers have written a readable result"]
    lines += [f"  waiting: {name} ({why})" for name, why in rows if why is not None]
    return "\n".join(lines)


def clean(manifest, work_dir):
    """Remove the checkouts the reviewers and the claim readers read. Returns whether none is left.
    Only `work_dir/tree` and `work_dir/head` are removed, whatever the manifest names."""
    head = Path(work_dir) / HEAD_TREE
    gone = remove_snapshot(manifest["root"], head) if head.exists() else True
    tree = manifest.get("tree")
    if not tree:
        return gone
    if Path(tree).resolve() != (Path(work_dir) / "tree").resolve():
        return False
    return remove_snapshot(manifest["root"], Path(tree)) and gone


FINISH = """\
## How to finish

- Start no background command. If a command ran past its time limit and the shell moved it to the
  background, stop it before you answer.
- Do not run `gh`, file or comment on an issue, or start another agent. The person running this
  skill reads your file and decides what is posted.
- When the results file is written and parses, answer with one line saying so and stop. Nothing
  reads your answer.
"""


CLAIM_PROMPT = """\
# Claim check: {count} drafts

Each file below is the draft of a GitHub issue that a reviewer wrote about this tree. Nobody has
checked what it says, and it will be posted if a person chooses it. You check the claim, and you
read only these drafts. You did not write them and you have not seen how they were found.

## The drafts

{drafts}

{rule}
{reading}
## The job

For every draft, read the code it names at the checkout above, not what the draft says is there:

1. **Does the claim hold?** Check each fact the draft states against the code: that the defect or
   the duplication is present, every count (recount them: the number of fields, callers, copies or
   lines), every caller it lists or says there are none of, and that a line range it cites does not
   end in the middle of a block. Search for callers and copies yourself, in strings, macros,
   the Makefile, `.github/` and other languages too. Quote the lines that show it.
2. **Did something decide against it?** Search the specs in `docs/specs/` and the code's comments for
   a reason it is the way it is. Search closed and open issues for the symbol or path with
   `gh issue list --state all --search TEXT`, and read the ones that touch it. An issue that excluded
   this move, or one that already holds this change, is a reason.
3. **Does the fix move, merge or hide code that reads, labels or declassifies content?** If it does,
   the rule above makes the draft wrong, whatever it saves.
4. **Are the line numbers written without a path right?** A draft says "lines 8376-8459" with the path
   earlier in the sentence, and a script reads only a number written with its path. For each, say
   where those lines are at this checkout, in `corrections`: `written` is the words in the draft and
   `now` is `path:first-last`. Leave `corrections` empty when every one is right.

Give each draft one verdict:

- `holds`: the claim is true here, nothing decided against it, and the fix does not break the rule.
- `rejected`: a fact is wrong, something decided against the change, or the fix breaks the rule. Say
  which, in a sentence a person can check, with the issue number or `file:line` that shows it.
- `unclear`: you could not tell. Say what you would need.

## Output

Write JSON to `{results_file}` and nothing else to stdout:

```json
{{
  "drafts": [
    {{
      "draft": "{first}",
      "verdict": "holds | rejected | unclear",
      "reason": "one sentence",
      "quote": "path:line the lines that show it",
      "corrections": [{{"written": "lines 8376-8459", "now": "path:8380-8470"}}]
    }}
  ]
}}
```

One entry for every draft. No prose outside the file. Do not edit any file.

{finish}"""


CLAIM_FINISH = FINISH.replace(
    "- Do not run `gh`, file or comment on an issue, or start another agent.",
    "- Run `gh` only to read issues (`gh issue list`, `gh issue view`). Never file or comment on one, and\n"
    "  do not start another agent.",
)


def build_claim_prompt(group, results_file, reading, rule):
    return CLAIM_PROMPT.format(
        count=len(group),
        drafts="\n".join(f"- `{entry['slug']}`: `{entry['body_file']}`" for entry in group),
        rule=rule,
        reading=reading,
        results_file=results_file,
        first=group[0]["slug"],
        finish=CLAIM_FINISH,
    )


def claims_prepare(manifest, work_dir, out, rule):
    """Write a prompt for each group of drafts, and check HEAD out in `work_dir/head` for the readers.
    `rule` is the text each reader is given about what a draft may not propose. Returns the index of
    the readers, or None where `draft` has not run. The index is written next to the prompts."""
    drafts = claim_drafts(out)
    if drafts is None:
        return None
    root = manifest["root"]
    commit = read_commit(root)
    head = Path(work_dir) / HEAD_TREE
    problem = make_snapshot(root, commit, head) if commit is not None else None
    tree = head if commit is not None and problem is None else None
    reading = reading_for(root, tree, commit)
    folder = Path(work_dir) / CLAIMS_DIR
    folder.mkdir(parents=True, exist_ok=True)
    readers = []
    for number, group in enumerate(claim_readers(drafts), start=1):
        prompt_file, results_file = folder / f"claims-{number}_prompt.md", folder / f"claims-{number}_results.json"
        prompt_file.write_text(build_claim_prompt(group, results_file, reading, rule), encoding="utf-8")
        readers.append(
            {"prompt_file": str(prompt_file), "results_file": str(results_file), "drafts": [e["slug"] for e in group]}
        )
    index = {"commit": commit, "tree": str(tree) if tree else None, "readers": readers}
    (folder / "index.json").write_text(json.dumps(index, indent=2), encoding="utf-8")
    return index


AREAS = (
    "area/trust", "area/tools", "area/turns", "area/interface", "area/cli",
    "area/delegation", "area/backends", "area/skus", "area/i18n", "area/acp", "area/plugins",
    "area/mcp", "area/background", "area/remote", "area/sandbox", "area/browser", "area/desktop",
    "area/release", "area/connectors",
)


INFRASTRUCTURE = "infrastructure"


INFRASTRUCTURE_PATHS = ("contrib/", "agents/", ".github/", "Makefile", "scripts/")


ASSIGNEE = "netzenbot"


def slug_for(finding, taken):
    lead = finding["candidates"][0] if finding["candidates"] else "read"
    base = re.sub(r"[^A-Za-z0-9._-]+", "-", f"{finding['lane']}-{lead}").strip("-")
    slug, extra = base, 1
    while slug in taken:
        extra += 1
        slug = f"{base}-{extra}"
    taken.add(slug)
    return slug
