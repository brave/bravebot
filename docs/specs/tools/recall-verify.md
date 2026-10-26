---
id: RECALLVERIFY
title: recall_verify
status: normative
governs:
  - crates/agent/src/tools.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Marking a kept fact as checked today ([RECALL-11](../recall.md#RECALL-11)). `key` is the only argument and names a fact; it writes nothing else.

## Clauses

<a id="RECALLVERIFY-1"></a>
### RECALLVERIFY-1: it moves `verified` to today and tells the planner to check first

The tool's description ends "First check that any file or function the fact names still exists." The fact is backed up before it is rewritten, under the spelling the store gives its file, so the session's undo restores the old date ([RECALL-6](../recall.md#RECALL-6)). Only `verified` changes. A key no fact has is refused and nothing is written. The answer names the key, so it is labelled with that fact's confidentiality ([RECALLTOOL-2](recall.md#RECALLTOOL-2)).

`verified-by: bravebot_agent::tools::recall_verify_moves_the_date`
`verified-by: bravebot_agent::tools::recall_verify_is_offered_with_the_description_the_spec_gives`
`verified-by: bravebot_agent::tools::recall_verify_refuses_a_key_that_is_not_kept`
`verified-by: bravebot_agent::tools::recall_verify_of_a_private_fact_makes_the_context_private`

<a id="RECALLVERIFY-2"></a>
### RECALLVERIFY-2: a delegate and an incognito session verify nothing

A delegate is offered no `recall_verify`, and a call from one is answered as an unknown name is. A session that writes nothing under the state directory answers that it changed nothing and writes no file.

`verified-by: bravebot_agent::tools::a_delegate_can_neither_recall_nor_verify`
`verified-by: bravebot_agent::tools::a_delegate_reaching_the_recall_handlers_anyway_is_refused`
`verified-by: bravebot_agent::tools::an_incognito_session_verifies_nothing`
