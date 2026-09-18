### the gates, and a witness minted outside them

Reading a label's content needs a `Declassification`, and `Declassification::authorise` is
`pub(in crate::policy)` so that no other module and no other crate can mint one. Four gates are the
only places content is supposed to come out:

{gate_sites}

`{review_doc}` states the rule this lane checks, as shape 3: **a witness is not permission to
inspect.** A function that holds one may release the bytes it was minted for and hand them on. It
may not treat holding one as licence to look at something else, and it may not pass one along to a
caller that will use it for a different value.

Check, in this order:

1. **Is `authorise` reachable from anywhere but `policy`?** Confirm the visibility is still
   `pub(in crate::policy)` and that nothing re-exports it, wraps it in a public constructor, or
   returns a `Declassification` from a public function. A public function that hands one out is the
   whole gate gone.
2. **Does any release happen outside the four gates?** A `declassify` in `bravebot-agent` is
   ordinary, since the agent holds witnesses the policy minted. What matters is whether the value it
   releases is the value the witness was minted for.
3. **Is a witness held longer than the one release it was for?** One stored in a struct, put in a
   collection, or returned upward is a witness looking for a second use.
4. **Does a gate return more than it was asked for?** `Policy::read_planner_argument` is refused once
   the context has met anything untrusted, which is LABEL-5. Check that the refusal is on the path
   and not merely available.

**Every place a witness is minted:**

{witness_sites}

Read `crates/core/src/policy.rs` around `Declassification` and `crates/core/src/value.rs` for what
`declassify` requires. The interesting answer here is usually "the visibility holds and every use is
inside `policy`", and saying that with the count you confirmed is a result.
