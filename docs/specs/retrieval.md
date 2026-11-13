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
request a retrieving turn sends and what an administrator may pin. The embeddings come from an
endpoint with a key, which can be Brave's, authenticated by a Brave Premium key
([premium-credentials.md](premium-credentials.md)).

[RETRIEVE-11](#RETRIEVE-11) reads `verified-by: none` until Brave's endpoint is public, since no
build reaches it before then. The desktop bridge that starts the indexing passes and marks a bot's
turn as one that may retrieve, and the clauses for what the window and `doctor` show of retrieval,
are not in this file yet.

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
was dropped is embedded later from the record. A pass reads the setting again as it starts and
before each batch after the first, so one queued or running when the setting went off sends
nothing more.

**Why.** The endpoint is another service. A turn that waited on it would be as slow as the slowest
reply.

`verified-by: bravebot_session::indexer::a_full_queue_drops_the_pass_and_a_later_one_embeds_what_it_would_have`
`verified-by: bravebot_session::indexer::a_pass_sends_nothing_once_retrieval_is_off`
`verified-by: bravebot_session::indexer::a_pass_queued_before_retrieval_went_off_sends_nothing`

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

<a id="RETRIEVE-7"></a>
### RETRIEVE-7: any failure falls back to compaction, and says so once

A missing key, an endpoint that fails, a vectors file no pass has begun or anything else that stops
a turn retrieving makes the turn compact as it would without the setting, except vectors that are
still being built, behind the cut or to be rebuilt, which are answered with a cut that leaves the
earlier messages out without excerpts. The trail says why in a fixed word. The person is told once, as "Earlier messages
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
### RETRIEVE-17: a turn retrieves only when it is given a retriever

The setting applies only to a bot's turns, so a turn retrieves only when its caller gives it a
retriever. The turn does not decide that from the definition it was addressed to. A turn given none
sends exactly what it sent before. The turn reads the vectors kept beside the session's record, so
a session with no record yet has nothing to retrieve and compacts as any other.

**Why.** A bot's conversation is the one that outlives compaction's usefulness. A retriever the
caller gives is one place to look, where a definition's `standing` flag is also true of a session
started with `--agent`.

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
A managed `true` pins nothing. A `retrieval` value that is not a block, or a block whose `enabled`
is not a boolean, such as `"enabled": "false"` or `"retrieval": false`, pins nothing either, and
`doctor` names the managed file as holding a `retrieval` value it reads as absent. A block with no
`enabled` says nothing about the switch and is not reported.

**Why.** Turning retrieval off sends nothing anywhere, so a machine's administrator may choose it.
Turning it on sends the person's conversation to another service, which is the person's choice. A
value that is not a boolean is read as absence, as it is in the person's own file, so it is
reported: an administrator who quoted `false` in a file nobody at the machine can edit is the
person least likely to find out that the pin does nothing, and the person's own `true` then sends
the conversation to the endpoint.

`verified-by: bravebot_config::managed::a_managed_retrieval_false_is_a_pin_and_nothing_else_is`
`verified-by: bravebot_config::settings::a_managed_false_keeps_retrieval_off_and_a_managed_true_is_not_obeyed`
`verified-by: bravebot_cli::doctor::doctor_reports_a_managed_retrieval_value_that_pins_nothing`
