### a clause that permits a violation

Every other pass over this repository runs in one direction. `check-spec` says so about itself: when
the code and the spec disagree, the code is wrong, and it "NEVER edits a spec, adds a clause, softens
a clause, or reports 'the spec is unrealistic' as a finding."

That leaves one question nobody asks, and it is yours: **could an implementation satisfy every clause
and still break the rule?**

You are not looking for a clause you disagree with, a clause you would word differently, or a clause
whose behaviour you would rather have some other way. You are looking for a gap between what the
clauses require and what the guarantee needs. Three shapes it takes:

1. **A clause that permits what it means to forbid.** It names a case and the code has another. It
   says "a path" where the code has a path and a symlinked second spelling of it. It says "on read"
   where a write reaches the same place.
2. **A clause satisfied by accident.** The behaviour holds because of something the clause does not
   mention, so a change that keeps the clause true breaks the guarantee. These are the valuable ones,
   because nothing will catch the change.
3. **A boundary two clauses each leave to the other.** Two specs govern the same path and each assumes
   the other refuses. The trust map and routing are where this is most likely.

The specs that carry the guarantee:

{guarantee_specs}

How to work it: for each spec, read what it requires, then ask what the smallest conforming
implementation would be. Not the one in the tree, the worst one that still passes every clause and its
named tests. Where that implementation breaks the rule, the clause is the finding and the fix is a
clause, not a patch.

State it as an amendment. Quote the clause, say what a conforming implementation could still do, and
say what the clause would have to require instead. A finding here is labelled `spec-bug`, and
`{labels_spec}` calls the label "the spec itself is wrong or under-specified; needs a human decision".
That last phrase is the standard: your job is to put the decision in front of somebody, not to take
it.

Where a clause is fine, do not say so clause by clause. Name the specs you read and what you tried
against them.
