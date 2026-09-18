#!/usr/bin/env python3
"""Pair every candidate with a pass whose job is to kill it.

A lane produces candidates, and a candidate is not a finding. The pass that found something is the
worst judge of whether it is real, because it has spent its whole run building the case. So each one
is handed to a reader who starts from the opposite position and has the repository's own counter
argument in front of them.

    python3 agents/skills/security-audit/verify-findings.py --work-dir "$WORK_DIR"

This writes one prompt file and one results file per candidate and prints how many are waiting. It
spends no model tokens itself. The reason this step exists rather than being folded into the lanes is
in `docs/development/reviewing-for-the-rule.md`: a wrong trust argument "reads exactly like a safety
feature", and it "is more likely to be waved through than a plain design mistake". A false finding
here does not waste somebody's afternoon, it gets a guarantee that was holding weakened to satisfy it.
"""

import argparse
import json
import sys
from pathlib import Path

VERIFIER = Path(__file__).resolve().parent / "verify.md"


def load_candidates(manifest):
    """Every candidate the lanes returned, and a finding for any lane that returned nothing."""
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
    return candidates, problems


def build_prompt(candidate, results_file):
    return VERIFIER.read_text(encoding="utf-8").format(
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

    work_dir = Path(args.work_dir)
    manifest_path = work_dir / "manifest.json"
    if not manifest_path.exists():
        print(f"no manifest at {manifest_path}", file=sys.stderr)
        return 2
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

    candidates, problems = load_candidates(manifest)
    manifest.setdefault("mechanical_findings", []).extend(problems)

    out = work_dir / "verify"
    out.mkdir(parents=True, exist_ok=True)
    verifications = []
    for number, candidate in enumerate(candidates, start=1):
        results_file = out / f"candidate{number}_results.json"
        prompt_file = out / f"candidate{number}_prompt.md"
        prompt_file.write_text(build_prompt(candidate, results_file), encoding="utf-8")
        verifications.append(
            {
                "id": number,
                "lane": candidate.get("lane"),
                "summary": candidate.get("summary"),
                "candidate": candidate,
                "prompt_file": str(prompt_file),
                "results_file": str(results_file),
            }
        )

    manifest["verifications"] = verifications
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    if args.json:
        print(json.dumps({"verifications": verifications}, indent=2))
        return 0

    # The path goes on the line rather than being left for the caller to construct, because a caller
    # that constructs it is a caller that reads the manifest back to check, and the manifest now holds
    # every candidate in full.
    for entry in verifications:
        print(f"  {entry['id']:>3}  [{entry['lane']}] {entry['summary']}")
        print(f"       {entry['prompt_file']}")
    counted = f"{len(verifications)} candidate" + ("" if len(verifications) == 1 else "s")
    print("")
    print(f"{counted} to verify. Prompts in {out}.")
    if problems:
        print(f"{len(problems)} lanes returned nothing usable; the report will say which.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
