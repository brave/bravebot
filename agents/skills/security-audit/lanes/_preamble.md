# Audit one lane of the rule

This repository is predicated on one statement, and it is not a matter of degree:

**Untrusted content never enters the driver's context or the planner's.**

The planner is the model deciding what to do next. The driver is the Rust code, meaning
`bravebot-core` and `bravebot-agent` both. The driver may **carry** untrusted content and **hand it
to** an effect without seeing it. It may not **branch** on it: no `if`, `match`, comparison, or early
return whose condition derives from untrusted bytes. Moving such a branch from `bravebot-agent` into
`bravebot-core` does not fix it, because `bravebot-core` is the driver too.

The tree to read is `{repo_root}`, and every other path here is relative to it.

Every list below is cut short where it is long. `{surface_file}` holds the whole of what was
enumerated, as JSON, and it is the list to work from where a lane's own list ends in a count.

Read `{review_doc}` before you start. It is the review pass this lane is one part of, and it names
the four shapes a violation takes. Read `{labels_spec}` for what a label is and which exceptions are
recorded.

## What a candidate has to be

You are looking for a defect, not for something that makes you uneasy. A candidate needs all four:

1. **A place.** `file:line`, and the function it is in.
2. **The bytes.** Which untrusted input reaches that place, and by which road. Name the entry point.
3. **The decision or the reach.** What the code does with them that carrying them would not require:
   the branch taken, the context they land in, the effect they redirect.
4. **What that buys an attacker who owns the bytes.** Concretely. If the answer is "nothing they did
   not already have", it is not a candidate.

A candidate you cannot walk somebody through is one you have not verified, and it does not go in.

## The mistake that is more expensive than missing something

`{review_doc}` has a section called "The inverse mistake: inventing a violation". Read it. The rule
"is about content that could reach the planner or steer a turn in progress. It is not a general
prohibition on reading bytes that arrived over a network, and it says nothing about this program's
own configuration and startup."

The example to hold on to: `GET /v1/models` carries `Label::untrusted_public()` and the code branches
on it freely, filtering on capabilities and comparing access against premium. That is correct. The
label "records where bytes came from, not that the data is injection-sensitive".

So before you write a candidate, ask whether the bytes could reach a model's context or influence a
turn already running. If they could not, the question is an ordinary engineering one about latency or
staleness, and it is not yours. A wrong trust argument reads exactly like a safety feature, and it is
more likely to be waved through than a plain design mistake. It also talks somebody out of the better
implementation for a reason that does not exist.

Before you report anything, look for the code that would refute it. Guarantees here are often
enforced somewhere other than the place you are reading, in a type that offers no way to do the wrong
thing. Take the refutation seriously.

## Your lane

