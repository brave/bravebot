#!/usr/bin/env python3
"""Pair every candidate with a pass whose job is to kill it.

A lane produces candidates, and a candidate is not a finding. The pass that found something is the
worst judge of whether it is real, because it has spent its whole run building the case. So each one
is handed to a reader who starts from the opposite position and has the repository's own counter
argument in front of them.

    python3 agents/skills/security-audit/verify-findings.py --work-dir "$WORK_DIR"

This writes one prompt file and one results file per verification and prints how many are waiting. A
verification is one candidate, or several that name the same file and make the same claim, which one
reader checks together so the code is read once. It spends no model tokens itself. The reason this step exists rather than being folded into the lanes is
in `docs/development/reviewing-for-the-rule.md`: a wrong trust argument "reads exactly like a safety
feature", and it "is more likely to be waved through than a plain design mistake". A false finding
here does not waste somebody's afternoon, it gets a guarantee that was holding weakened to satisfy it.
"""

import argparse
import importlib.util
import json
import re
import sys
from pathlib import Path

VERIFIER = Path(__file__).resolve().parent / "verify.md"

# Two candidates make the same claim when their summaries share this share of the shorter one's
# words, or share a quarter of them and point at the same few lines of one file. Half the words is
# where two lanes describing one defect in different terms land and where two defects that happen to
# sit in one file do not. A shared clause says nothing: one clause governs many defects, and two lanes
# often cite different clauses for one.
CLAIM_OVERLAP = 0.5
NEAR_OVERLAP = 0.25
# Words two summaries must share however short they are, so two short summaries that share one word
# are not one claim.
MIN_SHARED = 3
# The most candidates one verifier is given. A longer chain is cut into groups of this size.
GROUP_LIMIT = 4
# Lines apart that still count as the same place.
NEAR_LINES = 12
WORD = re.compile(r"[a-z][a-z0-9_]{3,}")
# Words every summary has, which make two of them look alike.
COMMON = frozenset(
    "that this with from then than they their there which where when into only same after before "
    "while does done have been will would could should other more some also each every"
    .split()
)
LOCATION = re.compile(r"([\w./+-]+\.\w+):(\d+)")


def load_candidates(manifest, tracked=None):
    """Every candidate the lanes returned, and a finding for any lane that returned nothing.

    `tracked`, where given, collects what each lane left out because the tracker held it, as
    `{"lane", "issue", "claim"}`.
    """
    candidates, problems = [], []
    for entry in manifest["lanes"]:
        path = Path(entry["results_file"])
        if not path.exists():
            problems.append(
                {
                    "severity": "error",
                    "kind": "lane-incomplete",
                    "summary": f"the `{entry['lane']}` lane wrote no result, so it was never audited",
                    "evidence": [str(path)],
                    "fix": "rerun that lane before trusting the report",
                    "source": "mechanical",
                    "lane": entry["lane"],
                }
            )
            continue
        try:
            result = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as problem:
            problems.append(
                {
                    "severity": "error",
                    "kind": "lane-unreadable",
                    "summary": f"the `{entry['lane']}` lane's result could not be read: {problem}",
                    "evidence": [str(path)],
                    "source": "mechanical",
                    "lane": entry["lane"],
                }
            )
            continue
        for candidate in result.get("candidates", []):
            candidate["lane"] = entry["lane"]
            candidates.append(candidate)
        if tracked is not None:
            for one in result.get("already_tracked") or []:
                if isinstance(one, dict):
                    tracked.append({"lane": entry["lane"], **one})
    return candidates, problems


def locations_of(candidate):
    """Every `file:line` a candidate's place names, in the order it names them."""
    return [(name, int(line)) for name, line in LOCATION.findall(candidate.get("place") or "")]


def claim_words(candidate):
    return {
        word
        for word in WORD.findall((candidate.get("summary") or "").lower())
        if word not in COMMON
    }


def same_claim(first, second):
    """Whether two candidates name one file and make one claim.

    The file is the first one each place names. The claim is the same where the summaries share half
    the words of the shorter, or where the two point within `NEAR_LINES` lines of each other in a file
    both name and share a quarter of the words. Word sharing counts only from `MIN_SHARED` words, so
    two short summaries with one word in common are not one claim. A candidate with no place is never
    grouped, because nothing says it is the same one.
    """
    where_a, where_b = locations_of(first), locations_of(second)
    if not where_a or not where_b or where_a[0][0] != where_b[0][0]:
        return False
    words_a, words_b = claim_words(first), claim_words(second)
    shorter = min(len(words_a), len(words_b))
    held = len(words_a & words_b)
    shared = held / shorter if shorter else 0.0
    wording = held >= MIN_SHARED
    if wording and shared >= CLAIM_OVERLAP:
        return True
    near = any(
        file_a == file_b and abs(line_a - line_b) <= NEAR_LINES
        for file_a, line_a in where_a
        for file_b, line_b in where_b
    )
    return near and wording and shared >= NEAR_OVERLAP


def group_candidates(candidates):
    """The candidates as lists, each holding the ones that make one claim about one file.

    A candidate that matches a member of two groups joins them, so a chain of near duplicates is one
    group. Groups and their members stay in the order the lanes returned them in, and a group's first
    member is the one the verifier decides first.
    """
    parent = list(range(len(candidates)))

    def root(index):
        while parent[index] != index:
            parent[index] = parent[parent[index]]
            index = parent[index]
        return index

    for later in range(len(candidates)):
        for earlier in range(later):
            if same_claim(candidates[earlier], candidates[later]):
                parent[root(later)] = root(earlier)
    members = {}
    for index in range(len(candidates)):
        members.setdefault(root(index), []).append(index)
    return [
        [candidates[index] for index in group[at : at + GROUP_LIMIT]]
        for group in sorted(members.values(), key=lambda group: group[0])
        for at in range(0, len(group), GROUP_LIMIT)
    ]


GROUPED = """
## Candidates grouped with this one

COUNT other candidates name the same file and make a similar claim, and are listed here so that one
reader checks them with the code open once. The candidate above is number 1.

```json
OTHERS
```

Decide candidate 1 as above. Then decide each of these, and add two keys to the JSON you write:

- `grouped`: one entry for each candidate listed here, as `{"number": 2, "same_defect": true}`. `true`
  means it describes the defect you decided for candidate 1, so your verdict stands for it and the
  issue will name every lane that reported it. `false` means it describes a different defect.
- `separate`: an object keyed by the number of each candidate you set to `false`, holding a verdict
  for it in the same shape as the one above (`verdict`, `reason`, `impact`, `read`, `corrected` and
  the rest).

Each candidate's `lane` decides which form of question 1 applies to it, so a candidate from one of the
lanes that question 1 names is not dropped under the rule's own form of the question. A candidate whose
number is missing from `grouped`, or whose verdict under `separate` is none of `CONFIRMED`, `DROPPED`
and `UNCLEAR`, is treated as not verified and is not filed. Do not set `same_defect` to `true` for a
candidate that names a different place or a different mechanism because it names the same file.
"""


def grouped_section(others):
    """The part of a verifier's prompt that hands it the other candidates of its group, or nothing."""
    if not others:
        return ""
    listed = [{"number": number, **one} for number, one in enumerate(others, start=2)]
    return GROUPED.replace("COUNT", str(len(others))).replace("OTHERS", json.dumps(listed, indent=2))


def build_prompt(candidate, results_file, others=()):
    return VERIFIER.read_text(encoding="utf-8").format(
        grouped=grouped_section(list(others)),
        candidate=json.dumps(candidate, indent=2),
        summary=candidate.get("summary", "(no summary)"),
        place=candidate.get("place", "(no place given)"),
        lane=candidate.get("lane", "(unknown)"),
        results_file=results_file,
        # The candidate's places are relative, and a verifier that resolves them against its own
        # working directory checks a different tree and reports the claim as stale.
        repo_root=str(Path.cwd()),
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    collector = importlib.util.spec_from_file_location(
        "audit_collect", Path(__file__).resolve().parent / "collect-findings.py"
    )
    collect = importlib.util.module_from_spec(collector)
    collector.loader.exec_module(collect)

    work_dir = Path(args.work_dir)
    manifest = collect.read_manifest(work_dir)
    if manifest is None:
        return 2

    tracked = []
    candidates, problems = load_candidates(manifest, tracked)
    manifest.setdefault("mechanical_findings", []).extend(problems)
    manifest["already_tracked"] = tracked

    out = work_dir / "verify"
    out.mkdir(parents=True, exist_ok=True)
    verifications = []
    for number, group in enumerate(group_candidates(candidates), start=1):
        primary, others = group[0], group[1:]
        results_file = out / f"candidate{number}_results.json"
        prompt_file = out / f"candidate{number}_prompt.md"
        prompt_file.write_text(build_prompt(primary, results_file, others), encoding="utf-8")
        verifications.append(
            {
                "id": number,
                "lane": primary.get("lane"),
                "summary": primary.get("summary"),
                "candidate": primary,
                "also": [
                    {
                        "number": at,
                        "lane": one.get("lane"),
                        "summary": one.get("summary"),
                        "candidate": one,
                    }
                    for at, one in enumerate(others, start=2)
                ],
                "prompt_file": str(prompt_file),
                "results_file": str(results_file),
            }
        )

    manifest["verifications"] = verifications
    collect.workdir.write_manifest(work_dir, manifest)

    if args.json:
        print(json.dumps({"verifications": verifications, "already_tracked": tracked}, indent=2))
        return 0

    # The path goes on the line rather than being left for the caller to construct, because a caller
    # that constructs it is a caller that reads the manifest back to check, and the manifest now holds
    # every candidate in full.
    for entry in verifications:
        print(f"  {entry['id']:>3}  [{entry['lane']}] {entry['summary']}")
        if entry["also"]:
            lanes = ", ".join(one["lane"] or "?" for one in entry["also"])
            print(f"       checked together with {len(entry['also'])} more, from: {lanes}")
        print(f"       {entry['prompt_file']}")
    counted = f"{len(candidates)} candidate" + ("" if len(candidates) == 1 else "s")
    grouped = f" in {len(verifications)} verifications" if len(verifications) != len(candidates) else ""
    print("")
    print(f"{counted}{grouped} to verify. Prompts in {out}.")
    if tracked:
        named = ", ".join(f"#{one.get('issue')} ({one['lane']})" for one in tracked)
        print(f"{len(tracked)} defects left out because the tracker holds them: {named}.")
    if problems:
        print(f"{len(problems)} lanes returned nothing usable; the report will say which.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
