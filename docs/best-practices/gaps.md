# Gaps

<!-- applicability: always -->

What a reviewer checks in a pull request description, which no tool reads for meaning. A defect it
says is left unfixed has an issue or a Known costs entry that records it, and the record is about
that defect. Every statement in it is true of the head commit, including after later pushes. Finding
the statements takes reading the whole description, in whatever words it uses.

---

<a id="GAP-001"></a>

## A pull request that leaves a defect unfixed cites the issue that tracks it

**A pull request that says another call site, platform or program has the weakness it fixes, or that
it found another defect and did not fix it, cites the issue that tracks the defect, or the Known
costs entry that records it as accepted.** The issue is filed when the description is written or
edited, so its number is in the text the reviewer reads. Once the issue exists, the defect is
tracked wherever the description mentions it.

A reviewer reads the description for statements of this kind in any wording, such as "may apply
to", "none has an issue", "is not fixed here" and "left for a follow-up", and for each one checks
that:

- the number or the entry is given where the statement is made;
- the issue describes that defect, since an issue about a nearby one does not track it;
- an accepted defect points to a bullet under Known costs in the spec that governs it, which the
  same pull request adds where it does not exist yet.

A description with no such statement says nothing about it, and a change that finds no defect has
nothing to list. A feature, an improvement or a test the change does not add, and a decision left to
a person, are not defects. The description says so in a sentence where it matters, a spec records
what is not built, and no issue is filed for them.

No tool checks this. The statements come in any wording, and the cited issue has to describe the
same defect, so the reviewer reads the whole description.

[PATH-004](paths.md#PATH-004) is this rule for the other spellings of a path, with an issue as the
only record.

```text
Wrong: `aws` has the same weakness and is not changed here.

Right: `aws` has the same weakness (#NNNN).

Right: Hooks named by a relative path are left as they are, which Known costs in
       docs/specs/hooks.md records.

Right: The editor's terminal is not used by this change.
       (A feature that is not built is not a defect.)
```

**Why:** a defect recorded only in a description is not in the issue tracker, so nobody is assigned
to it. It stays unfixed until someone else finds it, which can be an outside reporter filing it as a
new defect.

---

<a id="GAP-002"></a>

## A pull request description is edited when a later commit changes what it states

**When a commit pushed after review changes a decision, a wire format or a behaviour the
description states, the description is edited in the same push, so that every statement in it is
true of the head commit.** The statements that go stale are the decisions, the fix and the test
plan.

A reviewer compares each of those statements with the final diff of the pull request, from its
base to its head, and for each one checks that:

- a decision describes the design the code has now, and the choice it records is the one in the
  diff;
- a wire format the description names is the one the code reads and writes;
- a behaviour described as added, removed or changed is so in the final diff;
- the test plan names tests and commands that exist at the head.

`make check-narration` reads the wording of the body. It cannot read whether a sentence is still
true of the code, so the reviewer checks it.

```text
Wrong: Decision 2: the setting is stored in `config.json`.
       (The head commit stores it in `state.json`.)

Right: Decision 2: the setting is stored in `state.json`.
```

**Why:** the description is what a maintainer approves from, and it stays as the record after the
merge. A decision that names a design the code no longer has leads a reviewer to judge a change
that is not the one being merged, and leaves a later reader with a record of the wrong behaviour.
