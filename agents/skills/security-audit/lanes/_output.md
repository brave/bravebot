
## Output

Write JSON to `{results_file}` and nothing else. Do not edit any file in the tree.

```json
{{
  "lane": "the lane name from the heading above",
  "read": ["files you actually read, so a thin pass is visible as one"],
  "candidates": [
    {{
      "summary": "one sentence: the mechanism that is wrong, then what it costs",
      "severity": "error | warning",
      "kind": "violation | laundering | reachable | clause-permits | unpinned",
      "place": "crates/.../file.rs:123 in function_name",
      "entry": "which untrusted input reaches it, and by which road",
      "decision": "what the code does with the bytes beyond carrying them",
      "gain": "what this buys an attacker who owns the bytes",
      "evidence": ["crates/.../file.rs:123 what is there", "docs/specs/x.md:45 the clause"],
      "clause": "LABEL-4, where a clause is involved, else omit",
      "area": "one of: trust, tools, turns, interface, cli, delegation, backends",
      "impact": "high | medium | low",
      "fix": "what should happen instead, in a sentence",
      "screen": "what a person would be looking at when it happens, where the interface shows it",
      "reproduce": ["a command that shows it, where one exists"],
      "refutation_considered": "the argument that this is fine, and why it does not hold"
    }}
  ]
}}
```

`impact` is what the defect costs if nobody fixes it, and it is about reach rather than about how
interesting the bug is:

- **high**: the rule fails. Untrusted bytes reach the planner's context or the driver's, the driver
  branches on them, or an effect a person approved is redirected to somewhere they did not approve.
- **medium**: a real defect that needs a precondition an attacker does not control on their own, or a
  clause that permits one, or a laundered label nothing reads yet.
- **low**: a residue the specs already admit, a document that disagrees with another, or a guarantee
  that holds today and nothing pins.

Propose one. The pass after yours decides whether the candidate survives at all and will set the
final value, so an honest `medium` is worth more than a hopeful `high`.

`screen` and `reproduce` are how a reader acts on this without rebuilding it first. Omit either where
there is no honest answer, and do not invent a command. Nothing about a count in a document, or a
symbol nothing pins, has a screen.

`refutation_considered` is not optional. A candidate that has not been argued against is a candidate
nobody has checked, and the next pass will be checking it rather than the code.

Return an empty `candidates` list where the lane is clean. That is a real result and the common one.
Do not pad it. A lane that reports nothing and names what it read is worth more than a lane that
found something arguable.
