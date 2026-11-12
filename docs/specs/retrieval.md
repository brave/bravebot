---
id: RETRIEVE
title: Retrieving from a long bot conversation instead of summarising it
status: normative
governs:
  - crates/config/src/managed.rs
  - crates/config/src/embeddings.rs
  - crates/config/src/embeddings_key.rs
  - crates/aichat/src/embeddings.rs
  - crates/agent/src/conversation.rs
  - crates/agent/src/retrieval.rs
  - crates/agent/src/turn/round_steps.rs
  - crates/session/src/vectors.rs
  - crates/session/src/indexer.rs
  - crates/session/src/retrieve.rs
  - crates/session/src/retriever.rs
  - crates/session/src/sessions.rs
  - crates/session/src/retention.rs
  - crates/tui/src/app/records.rs
  - crates/ui-bridge/src/bridge.rs
  - crates/ui-bridge/src/retrieval.rs
  - crates/cli/src/doctor.rs
documented-by:
  - none (gap: a page on the retrieval setting, what it sends and what it keeps, owed once the setting exists)
---

## Scope

A bot's conversation runs for weeks, and compaction ([compaction.md](compaction.md)) loses most of
what was said in it once it has summarised a few times. With retrieval on, the older part of a bot
conversation is not summarised. Each stored message is embedded as it is saved, and a turn over its
budget sends the recent exchanges whole and, in front of them, the earlier excerpts nearest the
prompt. The record is never shortened, and compaction stays as the fallback for any failure
except vectors still being built, which leave the earlier messages out without excerpts until
they are ready.

This spec covers the setting, which messages are embedded, the file the vectors are kept in, the
request a retrieving turn sends and what a person is told about it. The embeddings come from the
service the person chooses ([RETRIEVE-33](#RETRIEVE-33)): Brave's endpoint, authenticated by a Brave
Premium key ([premium-credentials.md](premium-credentials.md)), OpenAI or OpenRouter with the key
their provider block already holds, or an endpoint of the person's own. Until Brave's endpoint is
public, Brave's is offered only to a developer who asks for it ([RETRIEVE-34](#RETRIEVE-34)).

All of it is built. [RETRIEVE-11](#RETRIEVE-11) reads `verified-by: none` until Brave's endpoint
is public, since no build reaches it before then.

## Clauses

<a id="RETRIEVE-1"></a>
### RETRIEVE-1: retrieval is for bots, under one setting the person's own file sets

One global setting, `retrieval.enabled`, turns retrieval on, and there is no setting per bot. It is
off unless it is a real `true`, and a quoted `"true"`, a number or a block without it leave it off.
Only the person's own settings file and the file `--settings` names may turn it on: a checkout's
file naming the block is not obeyed, and `doctor` names that file.

**Why.** Turning it on sends a bot's conversation to the embeddings endpoint, which makes it a
destination, and a checkout cannot choose where the conversation goes.

`verified-by: bravebot_config::settings::retrieval_is_off_unless_the_persons_own_file_says_true`
`verified-by: bravebot_config::settings::each_home_only_key_is_refused_to_the_layers_its_rule_names`
`verified-by: bravebot_cli::doctor::a_checkout_naming_any_home_only_key_is_reported_with_the_key_and_the_file`

<a id="RETRIEVE-2"></a>
### RETRIEVE-2: the record is never shortened

A retrieving turn changes what is sent and nothing that is kept. The record holds every message,
as it does without retrieval.

**Why.** The excerpts are read out of the record. A record that lost messages would lose them from
every later retrieval too.

`verified-by: bravebot_agent::conversation::a_retrieval_cut_leaves_the_record_whole`

<a id="RETRIEVE-3"></a>
### RETRIEVE-3: only what the planner was shown as trusted is embedded

A message is embedded by its role and the tag the agent composed it under, never by what it says:
what the person typed, what the planner replied, a call's result, a file somebody attached, a
skill's instructions and a message from another session under a trusted label. A copy another
program kept is not, nor is a summary, a notice, a resume's note, a vetted picture or PDF, or any
other message the agent or a front end composed for itself. A conversation whose context is not
trusted embeds nothing. Quarantined content is never embedded, sent or retrieved: the record holds
only the reference naming it, and that is embedded as the words around it.

**Why.** The record carries no labels, because the quarantine is never saved
([STATE-1](state-directory.md#STATE-1)). What it does carry is the role and the tag, which the
driver wrote, and the conversation's integrity. Excerpts are put in front of the planner, so
anything embedded has to be something the planner could already have been shown as trusted.

`verified-by: bravebot_session::vectors::an_imported_message_is_not_indexed`
`verified-by: bravebot_session::vectors::an_untrusted_context_indexes_nothing`
`verified-by: bravebot_session::indexer::an_untrusted_conversation_writes_no_vectors_and_asks_for_none`
`verified-by: bravebot_session::vectors::a_quarantined_reference_is_indexed_only_as_its_name`
`verified-by: bravebot_session::indexer::a_conversation_that_turns_untrusted_keeps_no_rows_and_asks_for_none`
`verified-by: bravebot_session::vectors::what_the_agent_composed_for_itself_is_not_indexed`
`verified-by: bravebot_session::vectors::pictures_pdfs_and_notes_about_other_messages_are_not_indexed`
`verified-by: bravebot_session::vectors::what_the_person_the_planner_and_a_call_said_is_indexed_with_who_said_it`

<a id="RETRIEVE-4"></a>
### RETRIEVE-4: embedding never holds up a turn

Messages are embedded off the turn, from a queue. A full queue drops what it cannot hold, and what
was dropped is embedded later from the record. Each save of a bot's turn while the setting is on
offers the record to the queue, and no other save does: a turn that is not a bot's, or one taken
with the setting off, embeds nothing and asks the endpoint nothing. A pass reads the setting again
as it starts and before each batch after the first, so one queued or running when the setting went
off sends nothing more.

**Why.** The endpoint is another service. A turn that waited on it would be as slow as the slowest
reply.

`verified-by: bravebot_session::indexer::a_full_queue_drops_the_pass_and_a_later_one_embeds_what_it_would_have`
`verified-by: bravebot_ui_bridge::indexing_tests::a_bot_turn_with_retrieval_on_has_its_record_embedded`
`verified-by: bravebot_ui_bridge::indexing_tests::a_turn_without_the_setting_or_not_a_bots_asks_for_no_vectors`
`verified-by: bravebot_session::indexer::a_pass_sends_nothing_once_retrieval_is_off`
`verified-by: bravebot_session::indexer::a_pass_queued_before_retrieval_went_off_sends_nothing`
`verified-by: bravebot_ui_bridge::indexing_tests::the_indexers_switch_reads_the_setting_each_time_it_is_asked`

<a id="RETRIEVE-5"></a>
### RETRIEVE-5: vectors from another endpoint or model are rebuilt rather than read

The vectors file names the endpoint that embedded it, the model, the number of dimensions and the
version of the rule chunks were cut by. The endpoint is its URL without the query, the fragment or
any user and password, so no key is written. A file in an earlier layout is built again. When the
service, model or length chosen no longer matches a session's file, the file is neither searched
nor appended to: a turn falls back as it does for vectors that are not ready, and the next pass builds the file again from the record for the new choice,
which the status reports as rebuilding. One file is kept per session, so switching back to an
earlier choice builds it again too. An append that was cut short loses only the row it was writing.

**Why.** Vectors from two models cannot be compared, and two hosts can serve a model under the same
name, so the name alone cannot tell them apart. A byte range means nothing under another chunking
rule. A damaged file costs a rebuild, never a crash or a vector read from the wrong bytes.

`verified-by: bravebot_session::vectors::a_new_model_rebuilds_the_index`
`verified-by: bravebot_session::vectors::another_endpoint_rebuilds_the_index`
`verified-by: bravebot_session::vectors::a_sidecar_in_an_earlier_layout_is_rebuilt`
`verified-by: bravebot_session::indexer::switching_the_endpoint_model_or_length_rebuilds_without_mixing`
`verified-by: bravebot_session::indexer::the_indexer_rebuilds_for_the_endpoint_chosen_as_a_pass_starts`
`verified-by: bravebot_session::retriever::vectors_from_another_endpoint_are_stale_and_not_searched`
`verified-by: bravebot_aichat::embeddings::an_endpoint_s_identity_holds_no_query_fragment_or_user`
`verified-by: bravebot_ui_bridge::retrieval::vectors_from_another_endpoint_read_as_rebuilding`
`verified-by: bravebot_session::vectors::a_damaged_sidecar_is_rebuilt_rather_than_read`
`verified-by: bravebot_session::vectors::an_append_cut_short_keeps_the_rows_before_it_and_is_cut_off_on_resume`

<a id="RETRIEVE-6"></a>
### RETRIEVE-6: the vectors file holds no text and goes with its record

The vectors are kept beside the record, as `<id>.vectors`. Each row names a message by its place
in the record and a chunk by a byte range of that message's text, and holds no text itself. The
file is readable only by its user, is written nowhere in an incognito session, goes when the session
is deleted and with the record when the sweep removes it ([STATE-4](state-directory.md#STATE-4)),
and loses the rows naming messages a rewind took out of the record. A summary or a resume's note
holds no place in the record, so a compaction moves no message to another place.

**Why.** Text in the file would be a second copy of the conversation outside the record and its
rules. A row left naming a rewound message would name whatever message took its place.

`verified-by: bravebot_session::vectors::the_vectors_hold_no_text_and_only_this_user_reads_them`
`verified-by: bravebot_session::sessions::deleting_a_session_removes_its_vectors`
`verified-by: bravebot_session::vectors::a_rewind_truncates_the_vectors`
`verified-by: bravebot_session::sessions::a_rewound_session_cuts_its_vectors_to_the_record`
`verified-by: bravebot_session::retention::a_record_older_than_the_period_is_removed_with_what_is_beside_it_and_a_newer_one_is_kept`
`verified-by: bravebot_session::vectors::a_compaction_moves_no_message_to_another_ordinal`
`verified-by: bravebot_session::vectors::a_link_where_the_sidecar_goes_is_neither_read_through_nor_written_through`
`verified-by: bravebot_ui_bridge::incognito_retrieval::the_same_turns_outside_incognito_keep_vectors`
`verified-by: bravebot_ui_bridge::incognito_retrieval::an_incognito_bot_turn_keeps_no_vectors_and_queues_nothing`
`verified-by: bravebot_ui_bridge::incognito_retrieval::an_incognito_bot_turn_asks_the_embeddings_endpoint_nothing`

<a id="RETRIEVE-7"></a>
### RETRIEVE-7: any failure falls back to compaction, and says so once

A missing key, an endpoint that fails, a vectors file no pass has begun or anything else that stops
a turn retrieving makes the turn compact as it would without the setting, except vectors that are
still being built, behind the cut or to be rebuilt, which [RETRIEVE-31](#RETRIEVE-31) answers with
a cut without excerpts. The trail says why in a fixed word. The person is told once, as "Earlier messages
weren't recalled this turn:" with the reason and "Summarised instead.", and not again until
retrieval has worked in between. Whether it was said is not kept in the record, so a session
reopened says it again the first time.

**Why.** Retrieval is an improvement on compaction and never a condition for a turn to run.

`verified-by: bravebot_agent::turn::every_reason_retrieval_cannot_run_falls_back_to_a_summary`
`verified-by: bravebot_agent::turn::a_fallback_is_said_once_and_again_only_after_retrieval_worked`
`verified-by: bravebot_agent::turn::a_reopened_session_says_a_fallback_again`
`verified-by: bravebot_session::retriever::an_endpoint_out_of_reach_or_out_of_time_falls_back_as_that`
`verified-by: bravebot_session::retriever::a_refused_key_falls_back_as_the_key`
`verified-by: bravebot_session::retriever::an_endpoint_that_errs_falls_back_as_failed`
`verified-by: bravebot_session::retriever::a_query_vector_that_cannot_be_searched_with_falls_back_as_failed`
`verified-by: bravebot_aichat::embeddings::a_failed_request_is_named_by_why_it_failed`
`verified-by: bravebot_session::retriever::vectors_behind_the_cut_are_not_ready_and_no_vectors_are_missing`
`verified-by: bravebot_session::retriever::a_sidecar_that_has_to_be_rebuilt_is_stale`
`verified-by: bravebot_session::retriever::a_conversation_that_met_untrusted_content_retrieves_nothing`

<a id="RETRIEVE-8"></a>
### RETRIEVE-8: excerpts are one message, and the cut moves in steps

The excerpts a turn retrieved are kept as one message in front of its prompt, and the next turn
sends that message again byte for byte. The point before which messages are no longer sent moves
in steps rather than every turn, and a turn over its budget whose cut has no step to take sends its
request as it is.

**Why.** A prefix that changes every turn is a cache miss every turn.

`verified-by: bravebot_agent::conversation::the_retrieval_cut_moves_only_in_steps`
`verified-by: bravebot_agent::conversation::excerpts_are_one_message_in_front_of_the_prompt`
`verified-by: bravebot_agent::turn::the_next_turn_sends_the_previous_turn_byte_for_byte`
`verified-by: bravebot_agent::turn::a_cut_with_too_little_to_pass_neither_retrieves_nor_summarises`

<a id="RETRIEVE-9"></a>
### RETRIEVE-9: excerpts keep their order and name who said them

Excerpts are sent in the order they were said, and each says who said it: the person, the planner,
a tool, a file, a skill or another session. No excerpt's text can end its block or open another.

**Why.** An excerpt with no speaker lets what a file or a tool said read as what the person asked.

`verified-by: bravebot_session::retrieve::the_excerpts_fit_the_budget_and_come_in_the_order_they_were_said`
`verified-by: bravebot_session::retriever::excerpts_name_who_said_them_in_the_order_they_were_said`
`verified-by: bravebot_session::retriever::an_excerpt_cannot_close_its_own_block`
`verified-by: bravebot_session::retriever::each_speaker_is_named_by_its_word`

<a id="RETRIEVE-10"></a>
### RETRIEVE-10: the cut keeps rounds whole and pinned messages ahead of it

The cut never falls inside a round, and what compaction would carry across a cut, such as a skill
the planner was told to follow, is sent after it.

**Why.** A result without its call, or a skill's instructions dropped, changes what the planner
does next.

`verified-by: bravebot_agent::conversation::a_retrieval_cut_keeps_rounds_whole`
`verified-by: bravebot_agent::conversation::a_pinned_skill_is_sent_after_the_cut`

<a id="RETRIEVE-11"></a>
### RETRIEVE-11: the key goes only to the host it was issued for

The Premium key is sent only to the host compiled into the build
([PREM-2](premium-credentials.md#PREM-2)), and each request is counted
([PREM-12](premium-credentials.md#PREM-12)).

**Why.** The key is the person's, and a request that is not counted is one nobody can account for.

`verified-by: none`

<a id="RETRIEVE-12"></a>
### RETRIEVE-12: the trail says what retrieval sent, in counts

A retrieving turn writes one line to the trail with how many excerpts it sent and how many tokens
they came to, and none of their content, as compaction's line does
([COMPACT-10](compaction.md#COMPACT-10)).

**Why.** The trail is read by people auditing a session, and the content is already in the record.

`verified-by: bravebot_agent::turn::the_trail_says_what_retrieval_sent_in_counts`

<a id="RETRIEVE-13"></a>
### RETRIEVE-13: retrieval replaces compaction while the setting is on

A bot turn over its budget retrieves and does not summarise. A turn that fell back to a summary is
followed by retrieval once the vectors are ready.

**Why.** Summarising and retrieving the same history would send it twice.

`verified-by: bravebot_agent::turn::an_over_budget_bot_turn_retrieves_and_does_not_summarise`
`verified-by: bravebot_agent::turn::a_fallback_summary_is_followed_by_retrieval_once_the_index_is_ready`

<a id="RETRIEVE-14"></a>
### RETRIEVE-14: a turn says how many excerpts it used and which

A retrieving turn tells the front end how many excerpts it sent and the places in the record they
came from, and nothing of their text. The count of messages no longer sent only rises. A resumed
transcript still knows what each turn retrieved.

**Why.** A person should be able to see that a turn answered from excerpts, and which, without the
front end being sent the words again.

`verified-by: bravebot_agent::turn::a_retrieval_reports_where_its_excerpts_were_said_and_not_what`
`verified-by: bravebot_ui_bridge::recalled_tests::a_turns_retrievals_are_told_once_and_again_only_when_they_add_a_message`
`verified-by: bravebot_ui_bridge::interop::a_resumed_session_reports_where_its_excerpts_were_said_and_not_what`
`verified-by: bravebot_ui_bridge::retrieval_cut::a_turn_that_moves_the_cut_reports_more_left_out_of_its_request`

<a id="RETRIEVE-15"></a>
### RETRIEVE-15: vectors are asked for in batches, and a reply that does not fit is refused

Text is embedded by one request per 32 texts at most, each naming the model and carrying every
text behind the prefix of what it is embedded as: one for a query and another for a passage. The
reply is read as one vector per text, in the order the texts were sent whatever order the reply
lists them in. A reply missing a text, naming one twice or one that was not sent, holding a vector
of another length than the one configured, holding a number no vector can (one too large is read
as an infinity), or not an embeddings reply at all is a failure, and no vector of it is used. A
query vector is checked again before it is searched with, whatever embedded it. The endpoint, the model, the length, both prefixes and any token are one
configuration, and a token is sent to the configured endpoint alone.

**Why.** The endpoint's model answers more than 32 texts with an error. A vector read for the
wrong text would retrieve the wrong excerpt with nothing to show it was wrong.

`verified-by: bravebot_aichat::embeddings::passages_go_in_batches_of_32_behind_their_prefix_and_come_back_in_order`
`verified-by: bravebot_aichat::embeddings::a_query_goes_behind_its_prefix_with_the_token_where_one_is_set`
`verified-by: bravebot_aichat::embeddings::a_reply_that_does_not_match_the_inputs_is_refused`
`verified-by: bravebot_aichat::embeddings::no_texts_send_no_request`
`verified-by: bravebot_aichat::embeddings::a_redirect_is_not_followed_and_the_token_goes_nowhere_else`
`verified-by: bravebot_session::indexer::a_wrong_length_vector_in_a_later_batch_fails_the_pass_and_keeps_the_batch_before`

<a id="RETRIEVE-16"></a>
### RETRIEVE-16: a query is asked once and quickly, a passage up to three times

A query is embedded while the turn waits: the whole of its request, from building it to the last
byte of the reply, is given one and a half seconds, and it is sent once. A passage is embedded off the turn: its connection is bounded to five
seconds and its reply to fifteen, and it is sent up to three times in all, again only after a
failure [NET-6](network-egress.md#NET-6) lets be sent again, with the pause it sets. Every failure
comes back as an error the turn can fall back on, never as vectors.

**Why.** A turn held up by the endpoint is slower than the compaction it falls back to. A passage
can wait, and a refusal that would be the same next time is not worth asking again.

`verified-by: bravebot_aichat::embeddings::a_passage_request_is_sent_again_after_a_passing_failure_three_times_at_most`
`verified-by: bravebot_aichat::embeddings::a_refusal_and_a_query_are_sent_once`
`verified-by: bravebot_aichat::embeddings::a_query_that_is_never_answered_gives_up_within_its_bound`
`verified-by: bravebot_aichat::embeddings::a_query_slow_to_start_and_slow_to_finish_fails_at_the_deadline`
`verified-by: bravebot_aichat::embeddings::a_rate_limited_passage_waits_as_asked_unless_the_wait_is_past_the_ceiling`

<a id="RETRIEVE-17"></a>
### RETRIEVE-17: the bridge marks the turn that may retrieve

The setting applies only to a bot's turns. The bridge marks a bot turn as one that may retrieve.
The turn does not decide it from the definition it was addressed to. A session without the setting
sends exactly what it sent before. The turn reads the vectors kept beside the session's record, so
a session with no record yet has nothing to retrieve and compacts as any other.

**Why.** A bot's conversation is the one that outlives compaction's usefulness. A field the bridge
sets is one place to look, where a definition's `standing` flag is also true of a session started
with `--agent`.

`verified-by: bravebot_ui_bridge::indexing_tests::a_bot_turn_with_retrieval_on_retrieves_from_the_vectors_beside_its_record`
`verified-by: bravebot_ui_bridge::indexing_tests::a_turn_without_the_setting_or_not_a_bots_is_given_no_retriever`
`verified-by: bravebot_agent::turn::a_turn_without_a_retriever_compacts_as_it_did`

<a id="RETRIEVE-18"></a>
### RETRIEVE-18: excerpts are ranked by meaning and by the prompt's words

Each chunk before the cut is ranked by how near its vector is to the prompt's, and by its BM25
score for the prompt's words, and the two ranks are fused by reciprocal rank. A chunk's word score
counts only when it is at least 80% of the best chunk's, so a chunk sharing a common word with a
reworded question does not outrank one that means what was asked. Nothing from the cut on is
chosen, since it is sent whole.

**Why.** Meaning alone misses a name, a number or a code said once; words alone miss a question
asked in other words. Counting every word match, on questions worded away from the conversation,
lost more than the exact terms gained.

`verified-by: bravebot_session::retrieve::an_exact_term_outranks_a_chunk_only_nearer_in_meaning`
`verified-by: bravebot_session::retrieve::a_chunk_sharing_no_word_with_the_prompt_is_found_by_meaning`
`verified-by: bravebot_session::retrieve::a_weak_word_match_adds_nothing_to_a_chunks_rank`
`verified-by: bravebot_session::retrieve::nothing_from_the_cut_on_is_chosen`
`verified-by: bravebot_session::retrieve::an_exchange_the_cut_falls_inside_brings_no_reply_from_the_cut_on`

<a id="RETRIEVE-19"></a>
### RETRIEVE-19: excerpts fit a budget, with room for what changed and what was asked

What is chosen costs at most the budget the turn gives, at a fixed cost per excerpt for its
framing, and the words that open the excerpts' message are charged against the same budget. The turn gives half of what the messages sent whole leave of the context budget, at most
a quarter of the context budget, and never more than 6,000 tokens. Part of the budget is held back
for later chunks on the subject of the best hits, what the person said and what reads as an update
first, and part for the prompt and the planner's last reply in each exchange an excerpt came from,
where neither is chosen already. What a reserve leaves unspent goes to the next best hits.

**Why.** The best hit for a question is often where a thing was first said, and a later message
changed it. An excerpt read without the question it answered, or without what its exchange
settled on, is easy to misread. Past about 6,000 tokens, more excerpts add little: on the bench, a
200k window's 50,000-token share sent some 480 excerpts a round, while the ceiling found nearly as
many of the facts a question needed as an 8,000-token share did.

`verified-by: bravebot_session::retrieve::the_excerpts_fit_the_budget_and_come_in_the_order_they_were_said`
`verified-by: bravebot_session::retrieve::a_later_change_comes_with_the_hit_it_changes`
`verified-by: bravebot_session::retrieve::a_reserve_left_unspent_goes_to_the_next_best_hits`
`verified-by: bravebot_session::retrieve::an_excerpt_of_a_reply_brings_the_prompt_it_answered`
`verified-by: bravebot_session::retrieve::an_excerpt_of_a_prompt_brings_the_last_reply_of_its_exchange`
`verified-by: bravebot_session::retrieve::each_row_is_read_from_its_message_in_the_exchange_its_prompt_opened`
`verified-by: bravebot_agent::turn::round_steps::excerpts_stop_at_a_fixed_ceiling_whatever_the_window`
`verified-by: bravebot_session::retrieve::excerpts_and_their_context_lines_fit_the_budget_together`
`verified-by: bravebot_session::retriever::the_preamble_is_charged_against_the_budget`

<a id="RETRIEVE-20"></a>
### RETRIEVE-20: the vectors catch up from the record, and a pass cut short keeps what it wrote

Each pass works out from the record which chunks the vectors file should hold, and embeds those it
does not, a batch at a time, appending each batch as it comes back. A file that is missing, needs a
rebuild ([RETRIEVE-5](#RETRIEVE-5)) or holds a row the record no longer agrees with, by its place,
its speaker, its byte range or a digest of the words in that range, is started again, so a message
replaced by one of the same length is embedded again. A pass the endpoint fails partway leaves the rows
it wrote, and the next pass embeds only the rest. A batch is written whole or not at all. A pass over a record read before a rewind cut
the file, or before the session was deleted, writes nothing more, and one that a cut or a removal
overtakes between batches stops there. A second process appending to the same file can only make
it read as needing a rebuild: a row repeated, contradicted or past the record is never read as
ready.

**Why.** Working what is owed out of the record each time is what lets a full queue drop a pass, a
failed pass be finished later, and a resumed session or a newly enabled setting catch up, all the
same way. A row written for a record a rewind has since cut would name a message that is not there.

`verified-by: bravebot_session::indexer::a_pass_embeds_what_the_record_owes_and_the_next_carries_on_from_it`
`verified-by: bravebot_session::indexer::a_failed_pass_keeps_its_rows_and_the_next_finishes_them`
`verified-by: bravebot_session::indexer::a_sidecar_the_record_disagrees_with_is_built_again`
`verified-by: bravebot_session::indexer::a_message_replaced_by_one_of_the_same_length_is_embedded_again`
`verified-by: bravebot_session::indexer::a_pass_over_a_record_read_before_a_cut_writes_nothing`
`verified-by: bravebot_session::indexer::a_wrong_length_vector_in_a_later_batch_fails_the_pass_and_keeps_the_batch_before`
`verified-by: bravebot_session::indexer::a_cut_or_a_removal_between_batches_abandons_the_pass`
`verified-by: bravebot_session::indexer::a_second_writer_appending_only_causes_a_rebuild`

<a id="RETRIEVE-21"></a>
### RETRIEVE-21: the vectors are ready for a cut once every chunk before it is written

The vectors are ready for the messages before a place in the record when the file holds every chunk
of every message before it that is embedded, from the model asked about, and every row agrees with
the record. Until then, a turn that would retrieve from them does not.

**Why.** A turn retrieving from part of what came before would answer as if the rest had never been
said.

`verified-by: bravebot_session::indexer::the_vectors_are_ready_once_every_chunk_before_the_place_is_written`
`verified-by: bravebot_session::retriever::vectors_behind_the_cut_are_not_ready_and_no_vectors_are_missing`

<a id="RETRIEVE-22"></a>
### RETRIEVE-22: embedding a session is in that session's trail

What the gates decided about each request a pass sends is written to the trail of the session the
pass is for, under the turn whose save started it.

**Why.** Embedding sends the conversation to another service, and a person auditing the session has
to be able to see that it did, and when.

`verified-by: bravebot_session::indexer::a_pass_writes_what_its_requests_gates_decided_to_the_sessions_trail`

<a id="RETRIEVE-23"></a>
### RETRIEVE-23: an administrator can turn retrieval off and cannot turn it on

`retrieval.enabled: false` in the managed file keeps retrieval off whatever the person's own file
and the file `--settings` names say, and `doctor` names `retrieval.enabled` among the managed pins.
A managed `true`, or a value that is not a boolean, pins nothing.

**Why.** Turning retrieval off sends nothing anywhere, so a machine's administrator may choose it.
Turning it on sends the person's conversation to another service, which is the person's choice.

`verified-by: bravebot_config::managed::a_managed_retrieval_false_is_a_pin_and_nothing_else_is`
`verified-by: bravebot_config::settings::a_managed_false_keeps_retrieval_off_and_a_managed_true_is_not_obeyed`

<a id="RETRIEVE-24"></a>
### RETRIEVE-24: the desktop switch writes the person's own file unless something else fixes the key

The desktop's switch writes `retrieval.enabled` into the person's own settings file, keeping the
rest of the file, and turning it off writes `false`. The window is told whether retrieval is on,
the file that decided it, and what fixes the key where something does: the managed file turning it
off, or the file `--settings` names stating it, which is a choice for that run. A change asked for
while the key is fixed is refused, naming the file that fixes it, and writes nothing.

**Why.** The switch has to show what is in force. Writing the person's file while another file
decides would show a change that did not happen.

`verified-by: bravebot_ui_bridge::retrieval::the_switch_writes_the_persons_own_file_and_keeps_the_rest`
`verified-by: bravebot_ui_bridge::retrieval::a_managed_pin_or_a_run_file_stating_the_key_refuses_the_switch`
`verified-by: bravebot_config::import::a_destination_sets_retrieval_and_keeps_the_rest_of_the_file`
`verified-by: bravebot_config::settings::the_file_that_decided_retrieval_is_the_last_entitled_one_or_the_managed_file`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so ui/scripts/recall.test.mjs holds that the row names the file the switch writes and, where the key is fixed, says whether the administrator's file or the run's file fixed it; make check-ui runs it. ui/scripts/drive-recall.mjs draws Bots first on Agent settings and holds that a locked switch sends no request)`

<a id="RETRIEVE-25"></a>
### RETRIEVE-25: the desktop turns retrieval on only after showing where it sends

Turning retrieval on from the window is refused unless the request names the endpoint retrieval
sends to, which the window names after showing the person what is sent there and what is never
sent, or the person was shown that endpoint before. The endpoint shown is kept, and a different
endpoint is shown again. Turning it off needs nothing, and a key a person wrote by hand is obeyed
without it.

**Why.** Turning it on sends every bot conversation to another service. Checking in the bridge,
not only in the window, means a window that skipped the explanation cannot turn it on.

`verified-by: bravebot_ui_bridge::retrieval::turning_retrieval_on_needs_the_endpoint_to_have_been_shown`
`verified-by: bravebot_session::store::the_disclosed_endpoint_is_the_first_line_and_a_blank_one_is_none`
`verified-by: by-construction (ui/scripts/recall.test.mjs holds that turning the switch on opens the dialog until the endpoint was shown, that Turn on recall names the endpoint the dialog showed, and that the dialog and the caption name the host and what is never sent; make check-ui runs it. ui/scripts/drive-recall.mjs holds that Cancel sends nothing and leaves the switch off, and that once shown, off and on again ask nothing)`

<a id="RETRIEVE-26"></a>
### RETRIEVE-26: doctor names the setting, the file that decided it, and the service it sends to

`bravebot doctor` says whether retrieval is on and names the file that decided it, the managed
file included. Where it is on, it also names the embeddings service and its host, the model and
its dimensions, and makes one test call of a fixed text, saying how long the service took or why
the call failed. The call is not made where retrieval is off, or where the network section already
says no request can leave. None of these lines changes what `doctor` exits with.

**Why.** A person who turned recall on in the window has no other place in the terminal to see that
it is on, where it sends, and whether that service answers. A failed call is not a failure of the
configuration: a turn without retrieval summarises instead.

`verified-by: bravebot_cli::doctor::the_retrieval_lines_name_the_file_the_service_and_what_the_test_call_found`
`verified-by: bravebot_cli::doctor::the_test_call_is_not_made_where_the_network_says_no_request_can_leave`

<a id="RETRIEVE-27"></a>
### RETRIEVE-27: a turn that retrieved says where its excerpts were said, and never what they said

Each round that retrieves tells the interface which messages its excerpts came from, once each and
in the order they were said: the exchange each is in, as the prompt that began it, and whether it
was that prompt, which reply of the model's, or which call's result. It also gives about how many
tokens the excerpts came to, and a round that could not retrieve says why it summarised instead.
None of it carries a word of an excerpt. The words cross only when a person asks for them
([RETRIEVE-32](#RETRIEVE-32)).

**Why.** The window already holds the transcript, so it can draw each row from its own copy and
scroll to the message. Sending the excerpts again would put the same words across the pipe twice.

`verified-by: bravebot_agent::turn::a_retrieval_reports_where_its_excerpts_were_said_and_not_what`
`verified-by: bravebot_agent::turn::every_reason_retrieval_cannot_run_falls_back_to_a_summary`
`verified-by: bravebot_agent::conversation::a_retrieved_message_is_placed_by_its_exchange_and_its_kind`
`verified-by: bravebot_agent::conversation::two_retrievals_in_one_turn_are_both_the_prompts`
`verified-by: bravebot_ui_bridge::recalled_tests::a_turns_retrievals_are_told_once_and_again_only_when_they_add_a_message`
`verified-by: bravebot_ui_bridge::recalled_tests::a_recounted_transcript_carries_what_each_exchange_recalled`

<a id="RETRIEVE-28"></a>
### RETRIEVE-28: the interface is told how many messages the request leaves out, and the count only rises

Beside the count of messages compaction has taken out, the interface is told how many earlier
messages the request no longer carries: those compaction took and those the retrieval cut leaves
behind, together. A turn's end, an opened session and a rewound one each carry it. It never falls,
even when compaction folds the cut's messages into its summary, and a resumed session starts from
the count it was saved with.

**Why.** The window re-grounds a bot and consolidates its memory when the request stops carrying
what it carried. With retrieval on that happens as the cut moves, while the archive may not grow,
so the archive alone would miss it. A count that fell would read as the request gaining back what
it never regains. The consolidation turn opens by saying which happened: after a compaction, that
the conversation has just been summarised, and after the cut alone, that the earlier part is no
longer sent with each request and comes back only as the excerpts retrieved when relevant, since
nothing was summarised.

`verified-by: bravebot_agent::conversation::what_the_request_leaves_out_only_rises_and_survives_a_resume`
`verified-by: bravebot_ui_bridge::interop::a_resumed_session_reports_what_compaction_took_out_of_it`
`verified-by: bravebot_ui_bridge::retrieval_cut::a_turn_that_moves_the_cut_reports_more_left_out_of_its_request`

<a id="RETRIEVE-29"></a>
### RETRIEVE-29: the interface can ask how far a bot's vectors have caught up, in messages

`retrieval.status`, given a session's directory and id, answers `off` where the setting is off for
that project, and `unconfigured` where the settings choose no embeddings service that is set up.
Otherwise it answers how many of the messages the record owes the vectors file are
held whole and how many it owes, and whether the file is `ready`, still `indexing`, `rebuilding`
for another model, or `untrusted` because the conversation met untrusted content and owes nothing.
A message held only in part is not counted. While the file is `indexing` or `rebuilding`, a last
pass that the service in force gave no vectors, and that added no rows, adds why as `failure`: `key`,
`unreachable`, `out of time` or `failed`. That reason stays until a pass adds rows or catches up.
The answer carries counts and that reason only. The host does not offer the method.

**Why.** A person who turned recall on needs to see whether it can work yet. Counting messages,
not chunks, matches what the person sees in the transcript. A refused key would otherwise read as
indexing that never ends.

`verified-by: bravebot_session::indexer::readiness_counts_whole_messages_and_names_a_rebuild`
`verified-by: bravebot_session::indexer::the_indexer_keeps_why_the_endpoint_gave_nothing_until_a_pass_gets_on`
`verified-by: bravebot_ui_bridge::retrieval::the_status_counts_messages_and_names_an_untrusted_conversation`

<a id="RETRIEVE-30"></a>
### RETRIEVE-30: each prompt after the cut gets excerpts of its own

Once a cut has been made, a bot turn within its budget still retrieves for its own prompt and keeps
what it found as one message in front of that prompt, as [RETRIEVE-8](#RETRIEVE-8) keeps a cut's.
The cut does not move. The excerpts already sent stay whole until it next does, so the request
before the new prompt is sent again byte for byte. The budget is the one [RETRIEVE-19](#RETRIEVE-19)
gives. Excerpts that cannot be found leave the prompt without them: the request fits, so nothing is
summarised and nothing is said in the conversation. The trail says which ([RETRIEVE-12](#RETRIEVE-12)).

**Why.** Retrieval that ran only when a turn was over its budget left every later prompt with the
excerpts found for an earlier one. Everything before the cut was invisible to a new question until
the conversation grew past the budget again.

`verified-by: bravebot_agent::turn::a_prompt_after_the_cut_gets_excerpts_of_its_own`
`verified-by: bravebot_agent::turn::a_prompt_within_its_budget_goes_without_excerpts_that_cannot_be_found`
`verified-by: bravebot_agent::conversation::excerpts_for_a_later_prompt_leave_the_earlier_ones_whole`

<a id="RETRIEVE-31"></a>
### RETRIEVE-31: vectors still being built leave earlier messages out without excerpts

A bot turn over its budget whose vectors are behind the cut, or are to be rebuilt, does not
summarise. The cut moves where it would for excerpts, the request leaves out what it passes as
it would then, and nothing is put in front of the prompt. The messages stay in the record and
are indexed as any are, and the trail says "the index is still being built" with how many
messages are left out. The prompt is owed its excerpts, and a later round finds them for it as
[RETRIEVE-30](#RETRIEVE-30) says once the vectors are ready. A request refused after such a cut
is answered as any refusal is. A vectors file that is not there at all, which no pass has begun,
falls back to a summary as [RETRIEVE-7](#RETRIEVE-7) says.

**Why.** Turning retrieval on starts a catch-up pass over the whole conversation, which takes tens
of seconds for a long one. A question asked meanwhile used to summarise a thousand messages,
replacing exactly what retrieval would find once the pass finished. Sending the request uncut
instead would not fit: with an advertised window the budget is the window. A pass writes its file
before its first batch, so a file that is missing is one no pass is building, and leaving messages
out for it could leave them out for good.

`verified-by: bravebot_agent::turn::an_index_still_being_built_leaves_earlier_messages_out_without_excerpts`
`verified-by: bravebot_agent::turn::a_prompt_cut_without_excerpts_gets_them_once_the_index_is_ready`
`verified-by: bravebot_agent::turn::a_request_refused_after_a_cut_without_excerpts_is_summarised`
`verified-by: bravebot_agent::turn::a_missing_index_still_falls_back_to_a_summary`
`verified-by: bravebot_session::retriever::vectors_behind_the_cut_are_not_ready_and_no_vectors_are_missing`

<a id="RETRIEVE-32"></a>
### RETRIEVE-32: a person who opens a recalled row is shown the words that were sent

The one exception to [RETRIEVE-27](#RETRIEVE-27). Each place a turn's retrievals drew on also names
the message by its place in the record's numbering. `retrieval.excerpts`, given a session's
directory, its id and an exchange's prompt, answers from the saved record with what each of that
exchange's retrievals sent, in the order they were put in: the label the request view gives it,
whether it was found for a later prompt without moving the cut, whether a later cut in the same
exchange replaced it, the model that ranked it, and each excerpt's place, chunk, the word its block
names the speaker by, whether it went as a context line cut short, and its words as they were sent,
with the length a context line is cut to. Only that exchange's own retrievals are given, so
excerpts an earlier exchange found and a later request still carried are not. An excerpt the record lists with no block of its own is left out.
Every word is made printable first. The answer is never an event: the window asks when the row is
opened and only for a turn that has ended, since the record is written when the turn ends. A
record that is not there is no such session, and an id that is not a session's name is refused by
this method and `retrieval.status` alike before any file is read. The words of each block read
back as they were before the block's escaping, whatever escapes they held. The host does not offer
the method.

The record keeps which excerpts went as context lines, and lists only the excerpts it wrote a block
for, so each listed excerpt pairs with a block. A record written before it kept context lines reads
as one with none.

**Why.** A person checking why a bot answered as it did needs the exact words it was given, which
for a context line or one chunk of a long message are not the row the transcript shows. Asked for
one exchange at a time, and only when opened, the words still cross once.

`verified-by: bravebot_agent::conversation::the_context_lines_a_retrieval_sent_survive_a_resume_and_older_records_read`
`verified-by: bravebot_session::retriever::the_words_of_a_block_read_back_whatever_escapes_they_held`
`verified-by: bravebot_ui_bridge::retrieval::an_id_that_is_not_a_session_name_is_refused`
`verified-by: bravebot_agent::turn::the_record_keeps_the_context_lines_each_retrieval_sent`
`verified-by: bravebot_session::retriever::the_context_lines_are_named_by_their_place_among_the_excerpts`
`verified-by: bravebot_session::retriever::an_excerpt_with_no_block_is_not_listed_as_sent`
`verified-by: bravebot_session::retriever::the_blocks_read_back_as_they_were_written`
`verified-by: bravebot_ui_bridge::retrieval::each_excerpt_pairs_with_its_own_block`
`verified-by: bravebot_ui_bridge::retrieval::an_excerpt_with_no_block_is_left_out`
`verified-by: bravebot_ui_bridge::retrieval::the_words_come_back_as_said_with_control_characters_pictured`
`verified-by: bravebot_ui_bridge::retrieval::a_replaced_send_and_a_refreshed_one_are_told_apart_and_other_exchanges_left_out`
`verified-by: bravebot_ui_bridge::retrieval::excerpts_of_a_record_that_is_not_there_are_no_such_session`
`verified-by: bravebot_ui_bridge::dispatch::retrieval_excerpts_is_dispatched_and_names_a_missing_record`
`verified-by: bravebot_ui_bridge::recalled_tests::a_turns_retrievals_are_told_once_and_again_only_when_they_add_a_message`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so ui/scripts/recalled.test.mjs holds that a numbered place is kept by its number so two drawn from one row are both kept, that a place read without a number is kept once by its row, that a row opens to each block its exchange sent for its message and only those, and that an exchange is read once for all its rows while a failed read is asked again and a forgotten one read afresh; ui/scripts/drive-retrieval-indicator.mjs opens the real rows against a stand-in bridge and asserts the one request each exchange makes, the replaced send, the context line of a row not in the transcript, a long block scrolling in its own box, and the failure and its Retry)`

<a id="RETRIEVE-33"></a>
### RETRIEVE-33: the person chooses one embeddings service and keeps what they chose for each

`retrieval.embeddings` in the person's own settings file, or the file the process was started on,
names at most one `selected` service: `openai`, `brave`, `openrouter` or `custom`. Beside it is a
block for each service holding what was chosen for it: its `model`, the `dimensions` a test call
found its vectors to have, for the custom endpoint its `url`, and the `queryPrefix` and
`passagePrefix` put in front of a query and a passage, read as written. Selecting another service
changes `selected` alone, so what was kept for each of the others is there to switch back to.
Removing a value leaves the rest of its block. A block that is not an object is left as it is and
the write refused. A selected word this build does not know is no selection, and a malformed value
is absence.

The custom endpoint's key is not in the settings: it is kept in its own file beside them,
`embeddings-key.json`, written as the Brave Premium key's file is.

**Why.** A person trying two models switches between them, and having to type a URL, a model and
its length again for each switch is what makes them stop trying. The key stays out of a file a
person copies between machines and shows others, and out of the list of gateways, where a key
would be offered for inference.

`verified-by: bravebot_config::embeddings::nothing_written_is_no_choice`
`verified-by: bravebot_config::embeddings::the_selection_and_every_services_values_are_read`
`verified-by: bravebot_config::embeddings::an_unknown_service_or_a_malformed_field_is_absence`
`verified-by: bravebot_config::import::a_destination_selects_an_embeddings_service_and_keeps_each_ones_values`
`verified-by: bravebot_config::embeddings_key::a_stored_key_is_read_from_its_own_file`
`verified-by: bravebot_ui_bridge::retrieval::the_run_file_chooses_and_a_custom_endpoint_without_a_key_sends_none`

<a id="RETRIEVE-34"></a>
### RETRIEVE-34: the endpoint is resolved from the service in force, and no endpoint means no call

The service in force is the one selected, or Brave where none is and Brave's is offered. A
service is offered only where it can be used: OpenAI and OpenRouter where a provider block with
that id is configured, Brave where this build has a Premium endpoint and `BRAVEBOT_BRAVE_EMBEDDINGS`
is `1` in the environment, and the custom endpoint always. Brave's embeddings endpoint is not public
yet, so without that variable nothing selected is nothing in force, and a selection of `brave` is
a service not offered. OpenAI and OpenRouter send to `/embeddings` under their provider block's own base URL with
that block's key, so the key goes nowhere it does not already go. OpenAI defaults to
`text-embedding-3-small` and knows the length of each model whose length OpenAI publishes;
OpenRouter needs a model and its length kept. The custom endpoint needs an `http` or `https` URL,
a model and its length, and sends its stored key where there is one.

Where the settings choose no endpoint (nothing in force, a service not offered, no key, no model,
no URL or no length) nothing is sent: a turn's retrieval is unavailable as not set up and falls
back to a summary as [RETRIEVE-7](#RETRIEVE-7) says, an indexing pass is skipped and leaves the
vectors there are, `retrieval.status` says `unconfigured`, the window's state names no endpoint and
says why, `doctor` says why and makes no test call, and turning retrieval on is refused with that
reason ([RETRIEVE-38](#RETRIEVE-38)). The endpoint is resolved as each turn, pass and request
starts, so a change to the settings is used without a restart.

**Why.** An endpoint guessed at, or a key sent to a host it was not given for, is worse than no
retrieval. A pass that ran with no model would find the vectors' header unlike its own and rebuild
the file, losing every vector for a setting the person may be halfway through changing. Brave
standing in where nothing is selected keeps retrieval as it was for those who turned it on before
there was a choice. Until its endpoint is public, Brave's is a development deployment, and a
build that offered it unasked would send a person's conversations there whenever they chose nothing.

`verified-by: bravebot_aichat::embeddings::a_gateway_service_resolves_to_its_providers_host_and_key`
`verified-by: bravebot_aichat::embeddings::a_chatgpt_sign_in_does_not_offer_openai`
`verified-by: bravebot_aichat::embeddings::a_service_not_set_up_is_refused_and_no_selection_is_brave`
`verified-by: bravebot_aichat::embeddings::the_custom_endpoint_needs_its_url_model_and_length`
`verified-by: bravebot_aichat::embeddings::each_gateway_rule_holds_for_lengths_models_prefixes_and_keys`
`verified-by: bravebot_ui_bridge::retrieval::a_configured_openai_block_is_resolved_from_the_settings`
`verified-by: bravebot_ui_bridge::retrieval::the_status_counts_messages_and_names_an_untrusted_conversation`
`verified-by: bravebot_ui_bridge::indexing_tests::the_bridges_endpoint_is_resolved_each_time_it_is_asked`
`verified-by: bravebot_agent::turn::every_reason_retrieval_cannot_run_falls_back_to_a_summary`
`verified-by: bravebot_session::indexer::a_pass_with_no_endpoint_keeps_the_vectors_it_finds`
`verified-by: bravebot_session::retriever::with_no_endpoint_chosen_retrieval_is_not_set_up`
`verified-by: bravebot_ui_bridge::retrieval::with_no_endpoint_chosen_nothing_is_named_and_retrieval_stays_off`
`verified-by: bravebot_ui_bridge::retrieval::a_custom_endpoint_resolves_with_its_stored_key`
`verified-by: bravebot_cli::doctor::the_retrieval_lines_name_the_chosen_service_or_why_there_is_none`
`verified-by: bravebot_ui_bridge::retrieval::brave_embeddings_are_asked_for_by_a_one_alone`
`verified-by: bravebot_ui_bridge::retrieval::brave_is_offered_only_to_a_build_that_reaches_it_and_asked_for_it`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::without_the_flag_nothing_selected_is_nothing_in_force_and_brave_is_not_listed`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::without_the_flag_a_selection_of_brave_is_a_service_not_offered`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::with_the_flag_brave_is_listed_and_in_force_where_the_build_reaches_brave`

<a id="RETRIEVE-35"></a>
### RETRIEVE-35: the window offers every service with what was kept for it, and writes the choice

The window's retrieval state carries `embeddings`: the service `selected`, the one `inForce`, what
`locked` the choice where something does, and an option for each service in a fixed order, Brave's
left out where it is not offered, since nothing the person can set up would offer it. An
option says whether the service is `available` (offered, as [RETRIEVE-34](#RETRIEVE-34) says), and
the `reason` it cannot be used as it stands where it cannot, including a length not yet found. It
also gives the `url` and `host` it sends to, and its `model`, `dimensions` and prefixes as kept,
whether or not it is selected. The custom endpoint's option says whether a key is stored and never
holds the key. OpenAI's option lists the models whose lengths are published.

`retrieval.embeddings.set` selects a service, keeps values for one, or both, in the person's own
settings file through the writers [RETRIEVE-33](#RETRIEVE-33) names; the custom key goes to its own
file, readable by this user alone, and `null` or an empty key removes it. A `url` or `key` for a
service other than the custom endpoint, a model for Brave, an unknown field or a request naming
nothing to change is refused. An administrator's `retrieval.enabled: false` locks the choice as it
locks the switch, and so does the file the process was started on where it states
`retrieval.embeddings` or `retrieval.enabled`; a locked request is refused naming the file and
writes nothing. A changed model or URL forgets the length found for the old one.

**Why.** A person picks a service from what the window shows, so the window has to show what
each would use before it is chosen, and why one cannot be used yet. A setting the administrator
or the run fixed cannot be quietly overwritten by a file that does not count.

`verified-by: bravebot_ui_bridge::embeddings_choice_tests::every_service_is_offered_with_what_was_kept_and_never_the_key`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::a_set_up_provider_is_offered_at_its_host`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::the_window_state_carries_the_choice_and_the_answer_its_outcome`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::values_for_an_unselected_service_are_kept_untested`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::a_changed_model_for_an_unselected_service_forgets_its_length`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::switching_away_keeps_what_was_kept`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::a_managed_pin_or_a_run_file_choosing_locks_the_choice`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::the_custom_key_is_kept_privately_and_removed_by_null`
`verified-by: bravebot_agent::key_entry::an_embeddings_key_is_stored_private_and_removed`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::a_request_that_cannot_be_applied_is_refused`
`verified-by: bravebot_ui_bridge::dispatch::retrieval_embeddings_set_is_dispatched_and_refuses_an_empty_request`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::without_the_flag_nothing_selected_is_nothing_in_force_and_brave_is_not_listed`

<a id="RETRIEVE-36"></a>
### RETRIEVE-36: a choice touching the selected service is tested, and the length found is kept

Where `retrieval.embeddings.set` selects a service or changes the one selected, and that service
can be sent to, one constant passage is embedded there, once and without retries, and the answer
says how: `{ok: true, dimensions, millis}`, or `{ok: false, kind, message}` with `kind` one of
`key` (401 or 403), `not-found` (404), `shape` (a reply that is not one vector, or not the length
published), `network` or `other`. The length is kept for a service whose length is not published;
a published one that differs is a `shape` failure. Values kept for a service not selected are not
tested. `test` is `null` where nothing was tested.

**Why.** The length cannot be typed reliably and a wrong one rebuilds the vectors; asking the
service is the one way to know it, and the moment of choosing is when a refused key or a wrong
model is easiest to fix. A passage of the person's own would be sent somewhere they have not yet
seen work.

`verified-by: bravebot_ui_bridge::embeddings_choice_tests::selecting_a_service_tests_it_and_keeps_the_length_found`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::a_failed_test_call_is_told_by_kind`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::a_reply_of_another_length_than_published_is_a_shape_failure`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::a_changed_model_whose_test_fails_forgets_the_old_length`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::the_length_found_is_kept_for_the_service_selected`
`verified-by: bravebot_aichat::embeddings::a_probe_sends_one_passage_once_and_takes_its_length`
`verified-by: bravebot_aichat::embeddings::a_candidate_may_have_no_length_yet`

<a id="RETRIEVE-37"></a>
### RETRIEVE-37: OpenRouter's embedding models are listed for the window to choose from

`retrieval.embeddings.models` asks `/embeddings/models` under OpenRouter's provider base URL with
that block's key, and answers each model's `id`, `name` (the id where there is none),
`contextLength` and `pricing.prompt`. A listing is answered from memory for five minutes unless
`refresh` is given or the key has changed; the memory holds a fingerprint of the key, never
the key. Where OpenRouter is not set up or has no key nothing is asked and the answer
says so; a refused key, another HTTP status or an unreadable reply is a failure in a sentence the
window shows.

**Why.** OpenRouter's embedding models change, and a list typed into this build would be wrong
within weeks. The key is the one the block already sends to that host.

`verified-by: bravebot_ui_bridge::embeddings_choice_tests::the_models_are_listed_and_answered_from_memory`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::the_models_of_an_openrouter_not_set_up_are_not_asked_for`
`verified-by: bravebot_aichat::embeddings::the_models_beside_an_embeddings_url_are_listed`

<a id="RETRIEVE-38"></a>
### RETRIEVE-38: retrieval is on only with an endpoint, and only one the person was shown

Turning retrieval on is refused, with the reason, while the settings choose no endpoint. While
retrieval is on, a `retrieval.embeddings.set` that would leave the service in force unusable is
refused and writes nothing. One that would move retrieval to a URL other than the one the person
was last shown writes nothing, sends nothing, and answers `needsDisclosure` with that URL; asked
again with `disclosed` naming it, the change goes ahead. While retrieval is on, a change leaving
the service in force with no length known (a service selected untested, or its model or URL
changed) is tested before anything is written: a failed test refuses the change with its reason and
writes nothing, and a passing one writes the change and the length found together. The URL is kept
as shown once the change can go ahead. The disclosure is of the URL
retrieval sends to, whichever service that is.

**Why.** On means messages are sent somewhere. A switch that reads on while nothing can be sent
misleads, and a change of service is a change of who receives the person's messages, which they
have to see before it happens.

`verified-by: bravebot_ui_bridge::retrieval::with_no_endpoint_chosen_nothing_is_named_and_retrieval_stays_off`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::with_retrieval_on_an_unusable_selection_is_refused`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::with_retrieval_on_a_new_endpoint_must_be_shown_first`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::with_retrieval_on_an_untested_change_is_tested_before_it_is_written`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::with_retrieval_on_an_untested_selection_that_fails_is_refused`
`verified-by: bravebot_ui_bridge::embeddings_choice_tests::with_retrieval_on_a_changed_custom_url_must_be_shown_first`
