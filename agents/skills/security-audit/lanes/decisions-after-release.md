### decisions after a release

`{labels_spec}` pins every `Labelled::declassify` to a count per file, so a new release cannot land
without `make check-spec` going red. `{review_doc}` says exactly what that does not cover:

> One added inside a function that already holds a counted `declassify` moves nothing: the counts pin
> how many times the bytes are released, not how many decisions are then taken from them, so
> `crates/core/src/policy.rs` still has to be read rather than counted.

That is your lane. A release is legitimate: the released bytes have to be handed to an effect, drawn
on a screen inside a margin the renderer owns, or written to a file. What you are looking for is
control flow that depends on them afterwards.

For each site, read the function it is in, from the release to the end, and decide:

- **Carried.** The value is passed on, written, drawn, or handed to an effect. Correct.
- **Decided on.** An `if`, a `match`, a comparison, an early return, a loop bound, an index, a
  length check, a sort key, or a lookup whose key derives from the released bytes. That is shape 1,
  and shape 2 if the branch sits in `bravebot-core` rather than `bravebot-agent`.

Two things that look like decisions and are not, so do not report them without more:

- The three exceptions recorded under `## Known costs` in `{labels_spec}`. Splitting a processor's
  answer at the mark, the trailing-newline check, and reading a one-word verdict out of a check are
  admitted, with the attacker's whole gain enumerated beside each. If a site is one of those, it
  belongs to the `known-costs` lane, not this one.
- Measuring without reading. `Labelled::shape` returns lines and bytes and is documented as not a
  read. A length used to decide whether to paginate is a side channel the spec accepts.

What raises a site from a note to a candidate is where the decision leads. A branch that picks a
message to draw is worth reporting. A branch that picks **which file is written, which host is
reached, which command runs, or what goes into the planner's context** is the rule failing, because
those are the effects a person approved a specific version of.

**{release_count} release sites in non-test code:**

{release_sites}

`crates/core/src/policy.rs` is over ten thousand lines and holds most of them. Read the functions,
not the file. Where a function releases and then only assembles a value to return, say so and move
on.
