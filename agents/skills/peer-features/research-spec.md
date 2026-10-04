# What do other coding agents offer that bravebot's `{{spec_id}}` spec does not?

Compare one area of bravebot with the coding agents that do the same job, and find the gaps worth an
issue. The area is the spec below. Today is {{today}}.

## The area

- Spec: `{{spec_path}}` ({{spec_title}}, clause ids `{{spec_id}}-N`)
- Code it governs:
{{governs}}

The tree is `{{root}}`, which is `{{ref}}` at `{{commit}}`. Every path is relative to it. Read files
and run commands in it, but edit nothing: no tracked file, no configuration, no test.

## The tools to compare with

{{peers}}

Those are the coding agents this project watches. Look at every one that plausibly has something like
the features in this spec, and skip the ones that plainly have nothing to compare. Each has public
documentation and a public repository or changelog; use the vendor's own site. Use whatever web search
and fetch tools you have.

## Read these first

- `agents/AGENTS.md`, for the rule that overrides everything. A gap is only a gap if closing it keeps it.
- `docs/specs/README.md`, then the spec above in full, including its `## Scope` and `## Known costs`.
  A cost listed there is a decision, and it is not a gap unless another tool shows the cost can be
  avoided without breaking the rule.
- The pages the spec's `documented-by` line names, to see what a person is told bravebot does.
- Enough of the governed code to know what bravebot does on this commit. The spec says what should
  happen, and the code says what does.
- `docs/development/labelling-issues.md`, for how a title reads.

## The steps

1. List the features this spec covers, one short sentence each. A feature is something a person does
   or sees, not a clause.
2. For each feature, find what the other tools document. Read the current page. Note what they do
   that bravebot does not, and what they do that bravebot does but could do measurably better.
3. Classify each candidate as `parity` or `beyond`, drop everything that fails the limits below, and
   keep at most {{max_gaps}}.
4. Check `{{known_gaps}}` and `{{tracker}}` for each one you keep.
5. Write the result.

{{gap_rules}}
