# Specs

<!-- applicability: always -->

Prose rules for a clause, checked by reading it. Clause ids, front matter, coverage and what a spec
may refer to are in [../specs/README.md](../specs/README.md), and `make check-spec` enforces them.
Read that file before writing or changing a spec, rather than copying the shape of whatever spec is
nearest.

---

<a id="SP-001"></a>

## A spec says what is true now

**Present tense, no history.** A clause describes the behaviour as it stands and the reasoning that
holds it in place, so it reads the same whether it was written today or two years ago.

Keep out of a spec: what an earlier version did, what was tried and abandoned, and which bug
prompted a change. A spec is read by somebody who has never seen any other version of this system,
and what changed is in the commit that changed it.

This binds the commentary and the **Why** of a clause as much as the clause itself: argue from what
the alternative costs, in the present tense, not from what the code did last week.

A measurement is different from a story about one. "Denying writes leaves the index unsettled" is a
fact about the system and belongs in the clause it justifies. "I tried denying writes and it broke"
is the same fact wearing a diary entry, and does not.

---

<a id="SP-002"></a>

## A known cost is a weakness, not a lesson

**A known cost is present tense too:** it names a weakness the design still has, not a mistake
somebody made on the way, and not a limitation that arrived. If the sentence is only interesting
because of who learned it and when, it is not a known cost.

An **open question** is for a decision genuinely unsettled, not for a thing that was settled and is
being justified after the fact.

---

<a id="SP-003"></a>

## A spec is written to be checked, not admired

**A clause is read by somebody deciding whether a diff obeys it,** so every sentence should be one
they could hold a diff against. Cut the cadence, the flourishes and the sentences that only set a
mood: plain declarative statements, and a **Why** that gives the reason rather than performing it.

---

<a id="SP-004"></a>

## A change agrees with the specs that govern its concern

**A pull request is read against every spec whose subject it touches**, which can be more than the
specs whose `governs` list names a changed file. Find them from the table in
[../specs/README.md](../specs/README.md) and from what the change is for, read the clauses that bear
on it, and hold the diff to them. `/check-spec changed` reads the specs governing the touched files;
the rest is for the author and the reviewer to find.

The change agrees when:

- **It contradicts no clause.** Where code and clause disagree, the code is wrong. A clause is not
  reworded to fit a diff unless the pull request is a deliberate change to that behaviour.
- **A change to specified behaviour carries the clause.** The clause, the behaviour and the tests the
  clause names change in the same pull request, as
  [spec-enforced-development.md](../development/spec-enforced-development.md) describes.
- **New behaviour in a specified area has a clause.** It is covered by an existing clause or adds one,
  rather than sitting beside the spec unmentioned.
- **The description names the specs it was checked against.** A reviewer can then see which clauses
  were considered, and that none was skipped.

**Why:** `make check-spec` checks that clauses are well formed and covered by tests. It cannot tell
whether a diff obeys a clause, and a change that contradicts the spec passes every check while
making the spec false.
