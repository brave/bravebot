---
id: RECALLTOOL
title: recall
status: normative
governs:
  - crates/agent/src/tools.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Reading the person's kept facts for the planner ([RECALL-10](../recall.md#RECALL-10)). `query` and `key` shape the call and there is no content argument. What a fact is and what its label means are [RECALL-4](../recall.md#RECALL-4) and [RECALL-8](../recall.md#RECALL-8); this file is the tool's own surface.

## Clauses

<a id="RECALLTOOL-1"></a>
### RECALLTOOL-1: exactly one of a query and a key, over the project's facts and every project's

A call gives `query` or `key`, never both and never neither. A query ranks the live facts of the project and of every project together and answers with at most eight ([RECALL-10](../recall.md#RECALL-10)), each with its key, strength and `verified` date, then its body. When the best strength is under 35, or nothing matches, the answer begins with "Probably nothing is stored about this. Say so rather than assume." For each of the top three results the facts its body links to with `[[key]]` follow, one hop and at most five in all, each marked `via <key>`; a link to a replaced fact or to a fact already shown is skipped. A key reads that one fact whether or not it was replaced, adding `superseded by <key>` when it was, and a key no fact has is an error.

`verified-by: bravebot_agent::tools::recall_finds_a_fact_by_its_words`
`verified-by: bravebot_agent::tools::recall_reads_the_project_and_every_project`
`verified-by: bravebot_agent::tools::recall_says_weak_when_nothing_matches`
`verified-by: bravebot_agent::tools::recall_answers_at_most_the_result_count`
`verified-by: bravebot_agent::tools::recall_leaves_superseded_facts_out_of_a_query`
`verified-by: bravebot_agent::tools::reading_a_superseded_fact_by_key_names_its_replacement`
`verified-by: bravebot_agent::tools::recall_takes_exactly_one_of_query_and_key`
`verified-by: bravebot_agent::tools::recall_follows_links_one_hop_from_the_top_three`
`verified-by: bravebot_agent::tools::recall_follows_at_most_the_link_cap`

<a id="RECALLTOOL-2"></a>
### RECALLTOOL-2: every fact an answer names gives the context its confidentiality

A fact is trusted text at the confidentiality its `label` states. The key, strength, date, link and text of a fact are all derived from it, so the answer is labelled with the join of the labels of every fact it names and the context holds that confidentiality from then on ([RECALL-8](../recall.md#RECALL-8)). A planner cannot read a private fact and send it to a public surface. A query's answer takes the join of the labels of every live fact that was ranked, shown or not, because corpus size, average length and term frequency shape what a public fact shows; so a weak answer over a corpus holding a private fact is private, and a key read, which ranks nothing, takes only the label of the fact read.

`verified-by: bravebot_agent::tools::recalling_a_private_fact_makes_the_context_private`
`verified-by: bravebot_agent::tools::a_private_fact_linked_to_makes_the_context_private`
`verified-by: bravebot_agent::tools::a_private_fact_removed_by_a_phrase_still_makes_the_context_private`
`verified-by: bravebot_agent::tools::a_private_fact_cut_off_past_the_result_count_still_makes_the_context_private`
`verified-by: bravebot_agent::tools::a_corpus_of_public_facts_leaves_the_context_public`

<a id="RECALLTOOL-3"></a>
### RECALLTOOL-3: only the turn the person is watching is offered it

A delegate is offered no `recall`, and a call from one is answered as an unknown name is.

`verified-by: bravebot_agent::tools::a_delegate_cannot_recall`
`verified-by: bravebot_agent::tools::a_delegate_reaching_the_recall_handlers_anyway_is_refused`
