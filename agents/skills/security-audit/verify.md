# Try to kill this candidate

A security audit lane claims it found a defect. Your job is to determine whether it is real, and you
start from the position that it is not. Most candidates are not. The ones that are will survive an
honest attempt to break them, and the attempt is what makes them worth filing.

Do not confirm this to be helpful. A false finding on this repository's guarantee is more expensive
than a missed one, because the fix for a defect that does not exist is a weakened guarantee that was
holding.

## The candidate

Lane: `{lane}`
Claim: {summary}
Place: `{place}`

```json
{candidate}
```

The tree to check is `{repo_root}`, and every path in the claim is relative to it.

## Read these first

- `docs/development/reviewing-for-the-rule.md`, all of it, and the section called "The inverse
  mistake: inventing a violation" twice.
- `docs/specs/labels.md`, at least the clauses the claim touches and the whole of `## Known costs`.
- The code at the place named, and the function around it, and its callers.

## The five questions, in this order

**1. Could these bytes reach a model's context or influence a turn already running?**

This is dispositive and it comes first. The rule "is about content that could reach the planner or
steer a turn in progress. It is not a general prohibition on reading bytes that arrived over a
network, and it says nothing about this program's own configuration and startup."

`GET /v1/models` is the case to measure against. It carries `Label::untrusted_public()`, and `usable`
filters on its capabilities, `access` is compared against premium, and `adopt_window` takes a number
out of it. All correct, because the label "records where bytes came from, not that the data is
injection-sensitive".

If the answer is no, the verdict is `DROPPED` and the reason is that this is an ordinary engineering
question about latency, staleness, offline behaviour, or how many code paths there are. Say so
plainly. Do not soften a drop into a warning.

**2. Is this already inside a gate, or already a recorded exception?**

The four gates are `Policy::present`, `Policy::render_in_place`, `Policy::read_trusted_content` and
`Policy::read_planner_argument`. The reshape gate has a second entry point,
`Policy::render_pair_in_place`, for a reshape of two pieces of content at once, such as a diff. The exceptions are the entries under `## Known costs` in
`docs/specs/labels.md`, each with the attacker's whole gain enumerated beside it. A candidate that
describes one of those, without naming something missing from its list, is `DROPPED`.

**3. Does something else already stop this?**

Guarantees here are often enforced somewhere other than the place being read: a type with no way to do
the wrong thing, a visibility, an absent trait implementation, a refusal on an earlier line. Look for
the refutation before you accept the claim. `Labelled` has no `Deref`, no `PartialEq` and no
`Display`, and that absence is a guarantee. `Declassification::authorise` is `pub(in crate::policy)`.

Then look for a test. Search `crates/core/src/policy.rs`, `crates/core/src/value.rs`,
`crates/agent/tests/turn.rs` and `crates/agent/tests/workspace.rs` for a test that pins the behaviour
the claim says is broken. If one exists and passes, the claim is probably wrong about what the code
does. Run it if you can. If a test exists and would pass against the broken behaviour too, that is a
different and real finding: say so.

**4. Is the evidence current and does it say what the claim says?**

Open every `file:line`. Confirm the line is there, the function is the one named, and the code reads
the way the claim reports. A candidate built on a line number from a different tree is `DROPPED`. Fix
a place that has merely moved, and record the correction.

**5. Has somebody already filed this?**

```sh
gh issue list --repo brave/bravebot --state all --limit 60 --search "<a distinctive term> in:title"
```

An open issue on the same mechanism means `DROPPED` with the number. A closed one that was fixed means
`DROPPED` with the commit. A closed one that was declined is worth knowing about and is still
`DROPPED`, citing the reasoning. If `gh` is not available, say so and leave the verdict on the merits.

## Then set the impact

Only for a candidate you confirmed. About reach, not about how interesting it is:

- **high**: the rule fails. Untrusted bytes reach the planner's context or the driver's, the driver
  branches on them, or an effect a person approved is redirected somewhere they did not approve.
- **medium**: a real defect needing a precondition an attacker does not control on their own, or a
  clause that permits one, or a laundered label nothing reads yet.
- **low**: a residue the specs already admit, a document that disagrees with another, or a guarantee
  that holds today and nothing pins.

The lane proposed one. Overrule it where the reach is not what it thought, and say why.

## Output

Write JSON to `{results_file}` and nothing else. Do not edit any file in the tree.

```json
{{
  "verdict": "CONFIRMED | DROPPED | UNCLEAR",
  "reason": "why it survived, or which question killed it, naming the question",
  "impact": "high | medium | low, only when CONFIRMED",
  "read": ["files and tests you actually opened"],
  "corrected": {{
    "summary": "the claim restated accurately, where the lane's wording overstated it",
    "place": "crates/.../file.rs:123 in function_name",
    "evidence": ["corrected file:line list"],
    "failure": "input or state, the path through the code, the outcome, as a walk somebody can follow",
    "gain": "what this buys an attacker who owns the bytes",
    "fix": "what should happen instead",
    "area": "trust | tools | turns | interface | cli | delegation | backends",
    "clause": "the clause id where one is involved, else omit"
  }},
  "existing_issue": "the number, where question 5 found one, else omit",
  "test_that_would_pin_it": "the assertion that would fail against this behaviour and pass after a fix",
  "reproduce": ["a command somebody can run to see this, where one exists"],
  "screen": "what a person would be looking at when it happens, where the interface shows it, else omit"
}}
```

`reproduce` is a shell command and nothing else: a test that fails, a `cargo run` with an argument, a
`grep` that returns the site. Leave it out rather than inventing one.

`screen` is what makes a finding actionable without being reproduced first, so answer it wherever the
answer is not "nothing, this is not visible". Name the state to get into and what is wrong on the
screen once you are there. A person captures it afterwards with `contrib/drive_tui.py --raw` and
replays it with `contrib/terminal-screenshot.py`, and the draft names the files to put it in. A
finding about a count in a document, or a symbol nothing pins, has no screen.

`UNCLEAR` is for a claim you could not settle without running something you cannot run. Say what you
would need. It is an honest answer and it does not get filed.

`test_that_would_pin_it` is required on a `CONFIRMED`. A defect nobody can write a failing test for is
usually a defect nobody has pinned down, and the assertion is the first thing whoever fixes this
needs.
