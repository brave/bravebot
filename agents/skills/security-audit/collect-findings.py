#!/usr/bin/env python3
"""Merge the mechanical findings with the verified candidates and render one report.

Reads the work directory, applies each verifier's verdict to the candidate it was given, and prints
what survived. Exits non-zero when anything at severity error stands, so the skill and a shell get the
same answer from the same place.

    python3 agents/skills/security-audit/collect-findings.py --work-dir "$WORK_DIR"

A dropped candidate is not silently discarded. The count of what was dropped and why is the evidence
that the verification pass did anything, and a run where nothing was dropped is a run to be suspicious
of.
"""

import argparse
import importlib.util
import json
import sys
from pathlib import Path

_shared = importlib.util.spec_from_file_location(
    "work_dir", Path(__file__).resolve().parents[2] / "work_dir.py"
)
workdir = importlib.util.module_from_spec(_shared)
_shared.loader.exec_module(workdir)

read_manifest = workdir.read_manifest

ERROR = "error"
WARNING = "warning"

CONFIRMED, DROPPED, UNCLEAR = "CONFIRMED", "DROPPED", "UNCLEAR"
VERDICTS = (CONFIRMED, DROPPED, UNCLEAR)
IMPACTS = ("high", "medium", "low")

# A confirmed candidate's severity follows its impact, because impact is the thing a reader was asked
# to judge and a second scale judged by nobody would only disagree with it.
SEVERITY_FOR = {"high": ERROR, "medium": ERROR, "low": WARNING}


def merge(candidate, verdict):
    """The candidate as the verifier left it. Corrections win, since the verifier opened the files."""
    corrected = verdict.get("corrected") or {}
    impact = verdict.get("impact") or candidate.get("impact") or "medium"
    if impact not in IMPACTS:
        impact = "medium"
    evidence = corrected.get("evidence") or candidate.get("evidence") or []
    return {
        "severity": SEVERITY_FOR[impact],
        "impact": impact,
        "kind": candidate.get("kind") or "violation",
        "lane": candidate.get("lane"),
        "summary": corrected.get("summary") or candidate.get("summary") or "(no summary)",
        "place": corrected.get("place") or candidate.get("place"),
        "entry": candidate.get("entry"),
        "decision": candidate.get("decision"),
        "gain": corrected.get("gain") or candidate.get("gain"),
        "failure": corrected.get("failure"),
        "evidence": evidence if isinstance(evidence, list) else [str(evidence)],
        "clause": corrected.get("clause") or candidate.get("clause"),
        "area": corrected.get("area") or candidate.get("area"),
        "fix": corrected.get("fix") or candidate.get("fix"),
        "refutation": candidate.get("refutation_considered"),
        "verified_reason": verdict.get("reason"),
        "test_wanted": verdict.get("test_that_would_pin_it"),
        # What a reader could see, and what they could run. The verifier decides both, because it is
        # the pass that opened the files and knows whether the claim survives being run.
        "screen": verdict.get("screen") or candidate.get("screen"),
        "reproduce": verdict.get("reproduce") or candidate.get("reproduce"),
        "source": "audit",
        "verdict": CONFIRMED,
    }


def settle(entry, candidate, verdict, findings, dropped, unclear, alongside=()):
    """File one candidate under what its verdict says.

    `alongside` holds the other candidates a confirmed verdict stands for, so the finding names every
    lane that reported the defect.
    """
    decided = (verdict.get("verdict") or "").upper()
    if decided == CONFIRMED:
        finding = merge(candidate, verdict)
        lanes = [one.get("lane") for one in alongside if one.get("lane") != candidate.get("lane")]
        if lanes:
            finding["also_reported_by"] = list(dict.fromkeys(lanes))
        # A site only the other lane named is still a site of the defect, and a fix that misses it is
        # not a fix.
        sites = list(finding["evidence"])
        for one in alongside:
            for site in [one.get("place"), *(one.get("evidence") or [])]:
                if isinstance(site, str) and site and site not in sites:
                    sites.append(site)
        finding["evidence"] = sites
        findings.append(finding)
    elif decided == UNCLEAR:
        unclear.append({**entry, "reason": verdict.get("reason")})
    else:
        dropped.append(
            {
                **entry,
                "reason": verdict.get("reason"),
                "existing_issue": verdict.get("existing_issue"),
            }
        )


def load_verdicts(manifest):
    """Apply each verdict, and report a candidate whose verifier left nothing.

    A verification may cover several candidates. The verdict is the first one's. Each of the others is
    named in the verdict's `grouped` list: one it calls the same defect shares the verdict, and one it
    calls a different defect takes the verdict the verifier gave it under `separate`. A candidate the
    verdict does not account for is unverified, since nobody decided it.
    """
    findings, dropped, unclear, missing = [], [], [], []
    for entry in manifest.get("verifications", []):
        path = Path(entry["results_file"])
        candidate = entry["candidate"]
        others = entry.get("also") or []
        if not path.exists():
            missing.extend([entry, *[hold(entry, one) for one in others]])
            continue
        try:
            verdict = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as problem:
            missing.extend(
                {**one, "problem": str(problem)}
                for one in [entry, *[hold(entry, one) for one in others]]
            )
            continue

        # Keyed by the number as text, so a verifier that writes 2 and one that writes "2" are read alike.
        said = {
            str(one.get("number")): one
            for one in verdict.get("grouped") or []
            if isinstance(one, dict)
        }
        separate = verdict.get("separate") if isinstance(verdict.get("separate"), dict) else {}
        same = [one for one in others if (said.get(str(one["number"])) or {}).get("same_defect") is True]
        settle(entry, candidate, verdict, findings, dropped, unclear, [one["candidate"] for one in same])
        for one in same:
            # Dropped or unsettled, the candidates it stands for are reported beside the first, so a
            # reader counting what a lane returned finds every one of them.
            if (verdict.get("verdict") or "").upper() != CONFIRMED:
                settle(hold(entry, one), one["candidate"], verdict, findings, dropped, unclear)
        for one in others:
            if one in same:
                continue
            own = separate.get(str(one["number"]))
            decided = (own.get("verdict") or "").upper() if isinstance(own, dict) else ""
            if (said.get(str(one["number"])) or {}).get("same_defect") is False and decided in VERDICTS:
                settle(hold(entry, one), one["candidate"], own, findings, dropped, unclear)
            else:
                missing.append({**hold(entry, one), "problem": "its verifier did not decide it"})
    return findings, dropped, unclear, missing


def hold(entry, one):
    """A verification's entry as one of its grouped candidates would be reported."""
    return {
        **{key: value for key, value in entry.items() if key not in ("also", "candidate")},
        "lane": one.get("lane"),
        "summary": one.get("summary"),
        "candidate": one["candidate"],
    }


def render(findings, mechanical, dropped, unclear, missing, manifest):
    lines = []
    order = {ERROR: 0, WARNING: 1}
    rank = {"high": 0, "medium": 1, "low": 2}

    if mechanical:
        lines.append("")
        lines.append("mechanical")
        for item in sorted(mechanical, key=lambda f: order.get(f["severity"], 2)):
            mark = "error" if item["severity"] == ERROR else "warn "
            lines.append(f"  {mark}  {item['summary']}")
            for one in item.get("evidence") or []:
                lines.append(f"         at {one}")

    if findings:
        lines.append("")
        lines.append("confirmed")
        for item in sorted(findings, key=lambda f: rank.get(f["impact"], 3)):
            lanes = ", ".join([item["lane"], *item.get("also_reported_by", [])])
            lines.append(f"  {item['impact']:<6} [{lanes}] {item['summary']}")
            if item.get("place"):
                lines.append(f"         at {item['place']}")
            if item.get("gain"):
                lines.append(f"         buys: {item['gain']}")

    if dropped:
        lines.append("")
        lines.append(f"dropped by verification ({len(dropped)})")
        for item in dropped:
            note = f" (#{item['existing_issue']})" if item.get("existing_issue") else ""
            lines.append(f"         [{item['lane']}] {item['summary']}{note}")
            if item.get("reason"):
                lines.append(f"           {item['reason']}")

    if unclear:
        lines.append("")
        lines.append(f"unsettled ({len(unclear)}), not filed")
        for item in unclear:
            lines.append(f"         [{item['lane']}] {item['summary']}")

    if missing:
        lines.append("")
        lines.append(f"unverified ({len(missing)}), not filed")
        for item in missing:
            lines.append(f"         [{item['lane']}] {item['summary']}")

    high = sum(1 for f in findings if f["impact"] == "high")
    lines.append("")
    candidates = sum(1 + len(one.get("also") or []) for one in manifest.get("verifications", []))
    lines.append(
        f"{len(manifest.get('lanes', []))} lanes, "
        f"{candidates} candidates, "
        f"{len(findings)} confirmed ({high} high), {len(dropped)} dropped, "
        f"{len(mechanical)} mechanical"
    )
    if not findings and not mechanical:
        lines.append("nothing survived verification, and the mechanical pass is clean")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    manifest = read_manifest(args.work_dir)
    if manifest is None:
        return 2

    mechanical = list(manifest.get("mechanical_findings", []))
    findings, dropped, unclear, missing = load_verdicts(manifest)

    everything = mechanical + findings
    failed = any(f["severity"] == ERROR for f in everything)

    if args.json:
        print(
            json.dumps(
                {
                    "findings": everything,
                    "dropped": dropped,
                    "unclear": unclear,
                    "unverified": missing,
                    "failed": failed,
                },
                indent=2,
            )
        )
    else:
        print(render(findings, mechanical, dropped, unclear, missing, manifest))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
