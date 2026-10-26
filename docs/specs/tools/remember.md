---
id: REMEMBER
title: remember
status: normative
governs:
  - crates/agent/src/tools.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Keeping one fact in the person's memory on the planner's own judgement ([RECALL-6](../recall.md#RECALL-6)). `key`, `scope`, `supersedes` and `new` shape the call and `text` is the fact. What a fact is, where it lives and how it is compared with the ones already kept are [RECALL-5](../recall.md#RECALL-5) to [RECALL-9](../recall.md#RECALL-9); this file is the tool's own surface.

## Clauses

<a id="REMEMBER-1"></a>
### REMEMBER-1: the text is the planner's own words, and a key is a slug and nothing else

`text` is content. It is released into a fact only while the planner's context is trusted, because a fact is trusted ([RECALL-9](../recall.md#RECALL-9)), and the fact states the confidentiality the context had at the call. The schema has no `_ref` field, so a quarantined slot cannot be kept. `key` and `supersedes` name files the tool writes, so each must be a slug path ([RECALL-5](../recall.md#RECALL-5)); a key that is not one is refused before anything is written. Whatever the text holds, it is only a body: a text that begins with a front matter block is saved as an agent fact whose body is that text. Saving over a key keeps the fact's `created` date and tags; a fact the person asked for (`source: person`) is never overwritten, and a key already replaced by another fact is not saved over, the answer pointing at the supersede route or the replacing key instead.

`verified-by: bravebot_agent::tools::remember_offers_no_reference_field`
`verified-by: bravebot_agent::tools::a_bad_key_is_refused`
`verified-by: bravebot_agent::tools::a_fact_from_a_context_that_met_something_untrusted_is_refused`
`verified-by: bravebot_agent::tools::a_fact_saved_from_a_private_context_is_private`
`verified-by: bravebot_agent::tools::a_text_that_looks_like_front_matter_is_only_a_body`
`verified-by: bravebot_agent::tools::a_session_id_with_a_control_character_is_not_written`
`verified-by: bravebot_agent::tools::saving_over_a_key_keeps_its_created_date_and_tags`
`verified-by: bravebot_agent::tools::a_fact_the_person_asked_for_is_never_overwritten`
`verified-by: bravebot_agent::tools::a_superseded_key_is_not_saved_over`

<a id="REMEMBER-2"></a>
### REMEMBER-2: only the turn the person is watching is offered it, and an incognito session keeps nothing

A delegate is offered no `remember`, and a call from one is answered as an unknown name is. A session that writes nothing under the state directory answers that it kept nothing and writes no file. The tool works in every permission mode, plan mode included, since it writes nothing in the workspace.

`verified-by: bravebot_agent::tools::a_delegate_cannot_save_a_fact`
`verified-by: bravebot_agent::tools::a_delegate_reaching_the_handler_anyway_is_refused`
`verified-by: bravebot_agent::tools::an_incognito_session_keeps_no_fact`
`verified-by: bravebot_agent::tools::remember_works_in_plan_mode`

<a id="REMEMBER-3"></a>
### REMEMBER-3: the answer names other facts by key and share, quotes none of them, and carries their confidentiality

A refusal for a near duplicate and the list of close facts on a save give each fact's key and the percentage of the new text it holds. No kept fact's text is quoted. The share is still derived from the fact, and which facts are listed or refused rests on every live fact in the save's scope that was compared, so the answer takes the confidentiality of every fact compared, named or not: the context holds it as it holds a private fact that was read, and the answer is labelled with it ([RECALL-9](../recall.md#RECALL-9)). A planner cannot probe a private fact by guessing texts and send what it learns to a public surface.

`verified-by: bravebot_agent::tools::a_fact_close_to_a_kept_one_is_refused_naming_it`
`verified-by: bravebot_agent::tools::every_save_lists_facts_above_45`
`verified-by: bravebot_agent::tools::a_refusal_naming_a_private_fact_makes_the_context_private`
`verified-by: bravebot_agent::tools::a_listing_of_a_private_fact_makes_the_context_private`
`verified-by: bravebot_agent::tools::a_private_fact_compared_but_not_named_makes_the_context_private`
`verified-by: bravebot_agent::tools::an_answer_naming_only_public_facts_leaves_the_context_public`
