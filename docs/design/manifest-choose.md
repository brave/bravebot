# Choose: deferred branching under information-flow control

**Status:** Proposal.

## Goal

Plan-then-execute keeps untrusted data from changing what a run may do by fixing control flow
before that data is read. That excludes tasks whose possible actions are known in advance but whose
correct action can only be chosen after observing data, for example:

- accept a change or request revisions;
- classify an item and apply the corresponding action;
- reply to a message, file it, or defer it.

This proposal adds `choose`, which fixes every possible branch in the plan and lets one of them be
selected at run time only by a trusted selector. Untrusted data may shape evidence, summaries,
drafts and recommendations. It cannot introduce a branch or select an effectful one.

## Where this fits in bravebot

bravebot separates trusted control from untrusted data with its IFC labels.
[LABEL-5](../specs/labels.md#LABEL-5) requires a decision to depend only on trusted content, and a
processor's output takes the integrity of its inputs ([PROC-3](../specs/processors.md#PROC-3)), so
output derived from untrusted observations stays untrusted. `choose` applies that rule to deferred
control flow and adds no new trust model.

## The problem

Some tasks cannot choose the correct branch before reading the data:

```text
if the change is good:
    accept it
else:
    request revisions
```

The planner knows both possible actions in advance, but not which one is correct. Letting a
processor return `accept` or `revise` leaves the choice with untrusted data, because the processor's
answer is derived from what it read and stays untrusted. What is missing is a way to fix the
possible branches in advance and let a trusted value select one of them later.

## Design

A manifest may declare a `choose` node with a finite set of branches:

```text
read item into item
process item into assessment

choose decision {
    evidence: [item, assessment]

    accept:
        ...

    revise:
        ...

    defer:
        ...
}
```

Before execution, the manifest fixes the `choose` node, every branch, every effect in each branch,
routing and destinations, each act's `option`, releases, and the evidence made available for the
decision. Only the selection of a branch is deferred. Run-time observations cannot add or change a
branch, or widen the effects the frozen plan authorises.

## Model

The frozen manifest defines a control-flow graph. Each `choose` node *c* in it has a set of
branches *A(c)*, fixed during plan validation.

At run time, `choose` receives a labelled selector *s*: a value naming a branch, with its label. A
transition from *c* to branch *a* is permitted only if all four hold:

- *s* has trusted integrity;
- *s* names *a*, and *a* is in *A(c)*;
- *a* is currently enabled ([Branch availability](#branch-availability));
- *s* was issued for this instance of *c* ([Binding a human choice](#binding-a-human-choice)).

```text
Trusted("accept")   -> accept may run
Untrusted("accept") -> refused
```

The branch set limits what may happen, and the selector's integrity decides whether it may pick
among those branches.

## Decision evidence

A delayed decision needs a defined boundary on what it may depend on, so a `choose` may declare
evidence:

```text
choose decision {
    evidence: [item, assessment]

    accept: ...
    revise: ...
}
```

Evidence may be trusted or untrusted and keeps its labels. For a human decision, the declared
evidence may be shown to the person. Showing a value as evidence neither makes it trusted nor lets
the driver inspect it for control flow ([LABEL-6](../specs/labels.md#LABEL-6)). Evidence informs
whoever decides; the selector is the value that controls the branch, and it must be trusted.

## Producing a trusted selector

### Person

A person may read trusted and untrusted evidence and decide:

```text
untrusted evidence
       |
       v
     person
       |
       v
Trusted("revise")
```

The evidence keeps its label. The person's answer is a new value with trusted provenance. This lets
a decision be informed by untrusted observations while those observations hold no control.

### Trusted computation

A computation over trusted inputs may produce a trusted selector under the normal IFC rules:

```text
trusted inputs
      |
      v
trusted computation
      |
      v
Trusted("defer")
```

If untrusted data contributes to the computation, the result is untrusted and cannot control
`choose`.

### Processor recommendation

A processor may produce `Untrusted("accept")` as evidence or a recommendation. It may be shown to a
person, and it cannot select the branch itself.

## Execution

Execution has four stages.

1. **Observe.** The planned reads run. Their outputs keep their normal labels and stay quarantined.
2. **Prepare.** Planned processors produce the summaries, classifications or drafts the branches
   need. Where possible, effects are prepared without changing external state.
3. **Decide.** The permitted decision process produces a trusted selector. For a human decision,
   the declared evidence and the prepared alternatives may be shown to the person, and no branch is
   selected by default.
4. **Execute.** The selector passes through the `choose` policy gate, and exactly one declared
   branch is entered. Effects in that branch still pass their normal IFC and authorisation checks;
   selecting a branch grants no authority to its effects.

## Security properties

`choose` keeps the plan-first bound on authority while letting one control decision wait.

- **Fixed authority.** The complete set of effectful branches is fixed before untrusted
  observations are read. Run-time data cannot introduce an effect or widen a branch.
- **Trusted control dependence.** The branch taken at a `choose` node depends only on a selector
  with trusted integrity. A value derived from untrusted data cannot select a branch, even one it
  names correctly.
- **No laundering.** Showing untrusted evidence, or constraining an untrusted value to a closed
  set, leaves its integrity as it was. A person's decision is a new value; no observation is
  relabelled.
- **Fail closed.** An invalid, untrusted or unavailable selection runs no branch. A failure never
  falls back to another branch.
- **No replanning from observations.** Untrusted observations never return to the planner, and a
  run-time choice stays within the graph the manifest fixed.

## Branch availability

Run-time state may make a planned branch unavailable: a target may move, a required object may
disappear, or preparation may fail. An unavailable branch stays in the frozen graph but cannot be
selected, and its failure never makes another branch run. Untrusted state can therefore stop a
branch from running, and it can never select a different one.

## Binding a human choice

A person's decision is scoped to the `choose` instance that asked for it. Where the person decides
while reviewing a prepared effect, the decision is also bound to that exact prepared effect, and a
later material change invalidates it instead of substituting a different request. The trusted
selector applies to that one choice; it is not reusable as trusted text and grants no capability.

## Relation to SPA

SPA is the closest general architecture to this proposal. It combines plan-first execution with
information-flow control and tracks integrity through both explicit data flows and control
dependencies[^spa]. `choose` applies the same view to one local branch in bravebot:

1. the outgoing branch set is fixed in the plan;
2. the later control dependency is an explicit, labelled selector;
3. the transition is allowed only when that selector has trusted integrity.

The proposal is narrower than SPA: it defines one branch primitive for manifest execution, where
SPA is an architecture for persistent agents.

## Other related work

CaMeL[^camel] and f-secure[^fsecure] separate untrusted data from trusted planning and control. ACE
bounds later execution with a plan made from trusted information[^ace]. NOVA shows that fixing the
branching graph is not enough when an untrusted component still selects the branch[^nova]. APPA
makes the matching IFC point that constraining an untrusted result to a closed schema does not give
it trusted integrity[^appa]. `choose` uses bravebot's existing rule directly: the graph is fixed in
advance, and only trusted data may select a path through it.

## Connector applications

The mechanism is generic. GitHub and Gmail connectors are its first applications.

### GitHub

A pull request may be reviewed as:

```text
choose decision {
    evidence: [pull, review_draft]

    comment:
        review COMMENT

    revise:
        review REQUEST_CHANGES

    approve:
        review APPROVE
}
```

The processor may draft the review, and the verdict is chosen by the selector. For `APPROVE`, the
evidence should identify the exact revision being approved as well as the generated review.

Issue classification uses the same mechanism:

```text
choose decision {
    evidence: [issue, summary]

    bug:
        add_label bug

    feature:
        add_label feature

    question:
        add_label question
}
```

### Gmail

A message may be triaged as:

```text
choose decision {
    evidence: [message, draft]

    reply:
        reply with draft

    file:
        add label
        archive

    defer:
        star
}
```

The message and the processor's output may inform the decision, and only the selector makes it.

## Security claim

The manifest fixes every possible effectful branch before untrusted observations are read.
Run-time control dependence is permitted only through an explicit `choose` whose selector has
trusted integrity. This recovers bounded data-dependent control flow without giving untrusted
observations authority over branches.

[^spa]: Girrens and Wang, [*SPA: Securing Persistent LLM Agents Across Queries with Plan-First
    Information-Flow Control*](https://arxiv.org/abs/2608.27234), arXiv:2608.27234.
[^camel]: Debenedetti et al., [*Defeating Prompt Injections by
    Design*](https://arxiv.org/abs/2503.18813), arXiv:2503.18813.
[^fsecure]: Wu, Cecchetti and Xiao, [*System-Level Defense against Indirect Prompt Injection
    Attacks: An Information Flow Control Perspective*](https://arxiv.org/abs/2409.19091),
    arXiv:2409.19091.
[^ace]: Li et al., [*ACE: A Security Architecture for LLM-Integrated App
    Systems*](https://arxiv.org/abs/2504.20984), arXiv:2504.20984.
[^nova]: Foerster et al., [*CaMeLs Can Use Computers Too: System-level Security for Computer Use
    Agents*](https://arxiv.org/abs/2601.09923), arXiv:2601.09923.
[^appa]: Kravchenko et al., [*APPA: Recoverable Information-Flow Control for Real-World LLM
    Agents*](https://arxiv.org/abs/2607.24625), arXiv:2607.24625.
