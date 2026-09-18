### laundering: a `Labelled` built by hand

This is shape 4, and `{review_doc}` says of it: "This is the shape no check pins: the constructor is
how the program labels its own data, so it is used everywhere legitimately, and only a reader can tell
the two apart."

You are the reader. `Labelled::new` and `Labelled::trusted` take a value and a label and return the
value wearing it. Nothing stops a caller passing `Label::trusted_public()` for bytes that came out of
a file, a fetch, or a subprocess. Once a value is wearing that label, `into_trusted()` reads its
content with no witness at all, because the label already says it is allowed.

For each site below, answer one question: **does the label handed to the constructor dominate the
label of every input the value was derived from?**

- A literal, a constant, or something the program computed from its own configuration: trusted is
  correct. This is the common case and most of the list will be it.
- A value derived from anything that entered through one of the roads in LABEL-8: trusted is
  laundering, whichever crate it happens in.
- A value derived from a mix: the label has to be the meet of the inputs' labels, not the best of
  them. `Label::meet` and the free function `taint_all` in `crates/core/src/label.rs` are what doing
  this correctly looks like.

`{review_doc}` states the test for the hard cases: "If a value derived from untrusted input has to be
trusted for something to work, the design is wrong, not the label."

Where you find one, trace it forward to whether the laundered value is then read through
`into_trusted`, used as a routing field, or put into a message to the model. A laundered label that
nothing reads is still a defect, and a laundered label that reaches the planner is the whole rule.

**{construction_count} construction sites outside `crates/core` in non-test code:**

{construction_sites}

**Unwitnessed reads, which is where a laundered label pays off:**

{unwitnessed_reads}

Read `crates/core/src/value.rs` first, so you know exactly what the constructors and `into_trusted`
promise. Then work the list. Where a file has many sites doing the same correct thing, say so once
rather than clearing them one at a time.
