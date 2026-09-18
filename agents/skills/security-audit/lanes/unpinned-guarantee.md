### a guarantee nothing pins

A clause with no test is a sentence. It holds today because somebody wrote the code carefully, and it
stops holding the first time somebody changes that code for a good reason, with every check green.

Three kinds of gap, and all of them are yours.

**Clauses at `verified-by: none`.** The mechanical pass has already listed them for the guarantee
specs and `unverified-clauses.txt` holds the whole set. Do not report that they are unpinned, since
that is counted already. For each one, answer the question the count cannot: **what would a change
that broke this look like, and would anything else catch it?** Say whether a test is possible and
what it would assert. Where a clause cannot be pinned by a test, say what does hold it: a type that
offers no way to do the wrong thing, a visibility, a count in a spec. That answer is worth as much as
a test, and `check-spec` has a `by-construction` form for recording it.

**Clauses at `by-construction`.** Nothing mechanical reads the bracket, so the reason is worth only
what a reader can check, and a clause answered this way has left the count above. These are the
brackets of the guarantee specs:

{bracket_clauses}

Take each to the tree and ask whether the thing it names is what holds the clause. A private field,
an authority minted in one file, or a count a check holds are answers; care taken by whoever wrote
the code is a clause at `none` wearing one; and a bracket resting on another bracket is worth what
that one is worth, no more. Report a bracket whose reason does not hold, and say which of the three
it is. Two to start from, because they are the load-bearing ones:

- `LAYER-2` in `docs/specs/layering.md` says core and agent are both the driver, so moving a branch
  between them launders nothing. Its bracket rests on a labelled value exposing no accessor for its
  contents and on `labels.md` pinning every use of the witness that reads one, file by file. A
  change that broke the clause is a decision relocated into the kernel, which is shape 2 and the
  subtlest of the four, so the question is whether that pin would move with it, and whether the
  bytes a decision could be taken on are only reachable through the witness.
- `PROC-9` in `docs/specs/processors.md` says the confinement is the capability set rather than an
  operating system boundary. Its bracket rests on a processor holding no capabilities at all, which
  tests of that spec pin, and on its spec being frozen before the call, which is a bracket of its
  own.

**Code no spec reaches.** `check-spec`'s scope is `docs/specs` plus the paths the specs list under
`governs`. Code outside that is ordinary code reviewed as ordinary code, which is correct for most of
the tree and wrong for anything that handles a label. These files build labelled values and no spec
governs them:

{ungoverned}

For each: does it decide anything about a label, or does it only carry one? A file that constructs a
labelled value from its own literals is ordinary. A file that chooses a label, or that computes one
from an input, is doing policy work outside the policy, and the finding is that a spec should govern
it. Adding a spec is a person's decision, so name the path, say what it decides, and say which spec
should reach it.

A finding here is `spec-coverage`: "a clause nothing pins: `verified-by: none`, or a test that passes
against the buggy code." Where you find a named test that would pass against a broken implementation,
that is the same label and a better finding than an absent one.
