# Gaps

<!-- applicability: always -->

What a reviewer checks when a pull request says a case is left undone: that an issue or a Known
costs entry records it, and that the record is about that case. Finding the statements takes
reading the whole description, in whatever words it uses.

---

<a id="GAP-001"></a>

## A pull request that says a case is left undone cites an issue or a Known costs entry

**A pull request that says a case is not handled, may be affected, or is left for later cites the
issue that tracks the case, or the Known costs entry that records it as accepted.** The case is
another call site, platform or program with the same weakness as the one the change fixes, or
another defect the author found. The issue is filed when the description is written or edited, so
its number is in the text the reviewer reads.

A reviewer reads the description for statements of this kind in any wording, such as "may apply
to", "none has an issue", "is not fixed here" and "left for a follow-up", and for each one checks
that:

- the number or the entry is given where the statement is made;
- the issue describes that case, since an issue about a nearby case does not track it;
- an accepted case points to a bullet under Known costs in the spec that governs it, which the same
  pull request adds where it does not exist yet.

The body lists such cases under its `Not covered:` line, and `make check-pr-body` checks that the
line exists and that each case under it names an issue
([commits.md](../development/commits.md#opening-the-pull-request)). It cannot read whether the list
is complete, whether a number is an issue, whether the issue is about that case, or a statement of this kind made elsewhere in the
description, which the reviewer checks.

[PATH-004](paths.md#PATH-004) is this rule for the other spellings of a path, with an issue as the
only record.

```text
Wrong: `aws` has the same weakness and is not changed here.

Right: `aws` has the same weakness (#NNNN).

Right: Hooks named by a relative path are left as they are, which Known costs in
       docs/specs/hooks.md records.
```

**Why:** a case recorded only in a description is not in the issue tracker, so nobody is assigned
to it. It stays undone until someone else finds it, which can be an outside reporter filing it as a
new defect.
