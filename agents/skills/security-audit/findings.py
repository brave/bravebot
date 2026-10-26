"""The shape of a decided finding, shared by the mechanical checks wherever they live."""

ERROR = "error"
WARNING = "warning"

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
