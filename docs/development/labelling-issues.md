# Labelling an issue

The kind labels say what an issue **is**: `bug`, `security`, `spec-mismatch`, `spec-coverage`,
`spec-bug`, `parity`, `enhancement`. The three axes say what to **do** about it, and every open
issue carries one value from each: an `importance`, an `urgency`, and a `size`.

More than one kind can be true at once, and a `spec-mismatch` usually carries a second: `bug` where
the code attempts the clause's behaviour and gets it wrong, `enhancement` where nothing attempts it
at all. Which it is answers a question the clause id cannot, and `is:open label:bug` is how somebody
asks what is broken today rather than what diverges. A `spec-coverage` takes neither, because a
clause nothing pins is a clause whose behaviour is right.

A missing axis is not a low value. It means nobody has judged the issue, and an unjudged issue is
invisible to every query built over the backlog: it is in no importance ordering, no urgency queue,
and no list of what fits in an afternoon.

**Importance is not urgency**, and they come apart in both directions. A hole in the guarantee that
nothing currently reaches is `importance/p1` and `urgency/p3`: it has to be fixed, and the tree is
no worse tomorrow for it. A parsing slip that eats a person's typed line is `importance/p4` and
`urgency/p2`: small, and it costs somebody something every day it stands. Collapsing the two into
one number is what loses the work in the middle.

| `importance` | What leaving it costs |
|---|---|
| `importance/p1` | the guarantee, or a person's data; everything else is downstream of it holding |
| `importance/p2` | central to what the tool is for |
| `importance/p3` | an ordinary defect, or a capability worth having |
| `importance/p4` | a narrow audience, or a small gain |
| `importance/p5` | cosmetic, the tree is no worse for leaving it |

| `urgency` | When it gets done |
|---|---|
| `urgency/p1` | stop other work and fix it |
| `urgency/p2` | next, ahead of planned work |
| `urgency/p3` | scheduled work, take it in turn |
| `urgency/p4` | no deadline, do it when the area is open |
| `urgency/p5` | indefinite, waiting on a decision or on work not done |

`urgency/p1` says every other thing in flight should be put down, so the list is expected to be
empty, and applying one is a claim about everybody's day rather than about the issue.

What earns `urgency/p2` is a cost that grows: reachable now, worsening with time, or taking the
session down under somebody who is using it. An issue that is merely important does not earn it,
because `importance` already records that.

| `size` | What it takes |
|---|---|
| `size/1` | an hour or less: one call site, and the test that pins it |
| `size/2` | a sitting: one subsystem, a handful of call sites |
| `size/3` | a few days: several files, a spec clause, new tests |
| `size/4` | a week or more: crosses crates, or a design to settle first |
| `size/5` | a project: a design document, several commits, a staged landing |

Size counts everything that ships in the commit, the tests and the spec clause included, and not
the lines of the fix alone. It is scheduling information and never a reason to skip something: a
`size/4` at `importance/p1` is work to split, not work to leave.

The three read together. Importance orders the backlog, urgency interrupts that order, and size
says what fits in the time available.

The [triage-issues skill](../../agents/skills/triage-issues/SKILL.md) assigns the triple to every
issue that survives a run, and takes the meanings from here.

## Where the work is

An `area` label says which part of the tree an issue lands in: `area/trust`, `area/tools`,
`area/turns`, `area/interface`, `area/cli`, `area/delegation`, `area/backends`, `area/skus`,
`area/i18n`. `infrastructure` is the one that is not an `area`, because the build and the tracker
are not part of the product and an issue about a workflow belongs to whoever owns the pipeline.

An area is a fact about the issue rather than a judgement, so it can be applied by whoever files
it. Apply none where the answer is not clear from the issue: an area guessed wrong is worse than
one missing, because it routes the issue away from the person who would have found it.

## Security

`security` says the issue is about the guarantee this repository exists for, and it goes on with a
kind label rather than instead of one. A `security` issue is almost always a `bug`, a `spec-bug` or
a `spec-coverage` as well, and which of those it is decides what the fix is.

`needs-security-review` says the finding has not been read by a person yet. Filing something at
`severity/high` is a claim, and this label is what distinguishes a claim from an accepted one.
Remove it once somebody has read the issue and agrees with it.

| `severity` | What fails |
|---|---|
| `severity/high` | the rule does not hold on this path: untrusted content reaches a context closed to it, or steers an effect a person approved a different version of |
| `severity/medium` | a real defect needing a precondition an attacker does not control on their own |
| `severity/low` | nothing today: the behaviour holds, and what is wrong is that nothing keeps it holding |

Severity is about reach and not about how interesting the defect is, which is why it is separate
from `importance`: `importance` is what leaving the issue costs the project, and `severity` is what
the defect costs on the path it is on. A `severity/low` finding that nothing pins can still be
`importance/p1`, because the guarantee holding by accident is the thing this repository is against.

The [security-audit skill](../../agents/skills/security-audit/SKILL.md) applies `security`,
`needs-security-review`, a kind, a `severity` and an `area` to every finding it files, and no
axis: an unread finding does not belong in anybody's queue.
