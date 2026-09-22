---
id: COMPACT
title: Compacting a conversation
status: normative
governs:
  - crates/agent/src/compact.rs
  - crates/agent/src/conversation.rs
guards:
  - symbol: Policy::adopt_summary
documented-by:
  - docs/website/docs/using/sessions.md
  - docs/website/docs/customize/configuration.md
---

## Scope

Every round re-sends the whole conversation, so a long session grows its own request until the
server refuses it. Compaction replaces the older part of the exchange, **in the request only**,
with a summary of it. `/compact` asks for the same thing on demand.

## Why the summariser is not a processor

Compaction is never routed through a processor.

What licenses the call instead is that there is nothing new to read. Every message in a
conversation has already been past the gate that decides what the planner may see: either it was
judged trusted and shown, or what went in was a reference and the bytes stayed in quarantine. So
the summariser's context **is** the planner's context, and its answer is labelled from that
context exactly as the planner's own words are. Nothing is upgraded.

## Clauses

<a id="COMPACT-1"></a>
### COMPACT-1: a summary is adopted only while the context is trusted

Once the context has gone untrusted the summary is refused and the conversation is left exactly as
it was. It is not quarantined instead, and it is not relabelled to get past the gate.

**Why.** A reference to the planner's own history is not a history. A conversation that cannot be
shortened stays long, which is the one outcome here that is never wrong.

**This cannot happen today.** A context only becomes untrusted by resuming one that already was,
and nothing makes one untrusted in the first place: untrusted content is quarantined rather than
shown, and the only place the context absorbs anything is where content was trusted enough to show.
The gate is here anyway, because what makes that closure safe to rely on is that something refuses
if it ever stops holding. If a change ever lets untrusted bytes into the planner's context, this is
what catches it.

`verified-by: bravebot_core::policy::a_summary_of_a_trusted_conversation_is_adopted`
`verified-by: bravebot_core::policy::a_summary_of_an_untrusted_conversation_is_refused_rather_than_adopted`
`verified-by: bravebot_agent::turn::a_summary_of_an_untrusted_conversation_leaves_the_conversation_whole`

<a id="COMPACT-2"></a>
### COMPACT-2: the summariser is offered no tools

It is asked for text and given nothing to act with.

`verified-by: bravebot_agent::turn::the_summariser_is_offered_no_tools`
`verified-by: bravebot_agent::turn::compacting_on_request_grants_itself_nothing_but_reaching_the_model`

<a id="COMPACT-3"></a>
### COMPACT-3: three things compaction never touches

The **quarantine**, which holds the only copy of what a surviving reference names; the **reference
counter**, since slots are written once and a name handed out twice would collide; and the
**integrity**, since nothing here has un-read what the conversation read.

None of these may be relaxed to save room.

`verified-by: bravebot_agent::conversation::a_reference_minted_before_compaction_still_names_its_content_after_it`
`verified-by: bravebot_agent::conversation::compaction_does_not_rewind_the_reference_counter`
`verified-by: bravebot_agent::conversation::compaction_does_not_restore_integrity_the_conversation_had_lost`

<a id="COMPACT-4"></a>
### COMPACT-4: the cut never lands inside a round

A call is never separated from its results, and a round in progress is not a place to cut. Whole
exchanges are given up first, since the boundary between two of them is the one a person would
draw. A turn that has gone long by itself has no earlier exchange to give, because it adds one
prompt however many rounds follow, so it gives up earlier **rounds** instead.

**Why.** A cut between a call and its results leaves the head saying the call never ran and the
tail holding an answer to a call that is not there.

`verified-by: bravebot_agent::conversation::compaction_never_separates_a_call_from_its_results`
`verified-by: bravebot_agent::conversation::a_round_in_progress_is_never_a_place_to_cut`
`verified-by: bravebot_agent::conversation::a_prose_shaped_round_is_as_indivisible_as_an_api_shaped_one`
`verified-by: bravebot_agent::conversation::compaction_keeps_the_most_recent_exchanges_word_for_word`
`verified-by: bravebot_agent::turn::a_long_turn_summarises_its_earlier_rounds_partway_through`

<a id="COMPACT-5"></a>
### COMPACT-5: a cut must give up at least as much as it keeps

Summarising costs a model call, and a request has a floor it cannot go below: the system prompt and
the tool schemas. A budget under that floor is unreachable however much history is given up, so a
cut that would not free more than it retains is not made at all.

**Why.** Without it, a turn in that position summarises itself once per round for the rest of its
life and shortens nothing. Measured at 35 summaries in a turn that should have made none. Never
relax this to compact sooner.

`verified-by: bravebot_agent::conversation::a_cut_that_would_give_up_less_than_it_keeps_is_not_worth_a_request`
`verified-by: bravebot_agent::conversation::a_long_turn_does_not_summarise_itself_once_per_round`
`verified-by: bravebot_agent::conversation::a_head_that_is_only_an_earlier_summary_is_not_compacted_again`
`verified-by: bravebot_agent::conversation::compacting_forgets_a_measurement_of_the_conversation_it_replaced`
`verified-by: bravebot_agent::conversation::a_conversation_with_nothing_but_recent_exchanges_is_not_compacted`
`verified-by: bravebot_agent::turn::a_conversation_nobody_has_measured_is_not_compacted`

<a id="COMPACT-6"></a>
### COMPACT-6: the request is shortened, never the record

The replaced messages go to an archive the transcript still reads and the session record still
stores. The user owns their transcript; compaction is about what gets sent.

`verified-by: bravebot_agent::conversation::what_compaction_took_out_of_the_request_is_still_recounted_to_the_person`
`verified-by: bravebot_agent::conversation::a_compacted_conversation_survives_being_written_down`
`verified-by: bravebot_agent::conversation::the_summary_is_not_shown_as_something_the_user_said`

<a id="COMPACT-7"></a>
### COMPACT-7: the budget is configurable, and a nonsensical one falls back

A budget that makes no sense falls back to the default rather than disabling compaction, so a
misconfiguration cannot quietly turn the mechanism off.

`verified-by: bravebot_config::lib::the_context_budget_has_a_default`
`verified-by: bravebot_config::lib::the_context_budget_can_be_overridden`
`verified-by: bravebot_config::lib::a_budget_that_makes_no_sense_falls_back_rather_than_disabling_compaction`
`verified-by: bravebot_agent::turn::a_conversation_past_the_budget_is_summarised_before_the_next_request`
`verified-by: bravebot_agent::turn::compacting_on_request_reaches_the_model_and_shortens_the_conversation`

<a id="COMPACT-8"></a>
### COMPACT-8: compaction is tried before a request whose conversation is already over budget

The figure compared is what the server said the **last** round's request came to, so the check is
one round late by construction and the budget has to sit below the window rather than at it. A
turn that has not measured anything yet compacts nothing: there is no figure to compare, and a
guess would be one.

It is tried before every round that qualifies, not once per turn. Ordinarily there is nothing
worth cutting and nothing is sent, which costs nothing and says nothing. Only a summariser that
**failed** stops it being tried again for the rest of the turn, because a call that errored once
is not free to repeat.

`/compact` asks for the same work on demand, at any size, and does not consult the budget.

**Why the default sits well below any real window.** A budget above the window never fires, so
being wrong upward does not make compaction late, it removes it. The default was once set above
anything the backend serves and sessions ran to exhaustion having never been summarised. A default
is therefore checked against the smallest window worth using rather than chosen to be generous.
Where the endpoint advertises a window for the model in use, that is used instead, so the default is
a fallback rather than the usual case: see COMPACT-9.

`verified-by: bravebot_agent::turn::a_conversation_past_the_budget_is_summarised_before_the_next_request`
`verified-by: bravebot_agent::turn::a_conversation_nobody_has_measured_is_not_compacted`
`verified-by: bravebot_agent::conversation::a_long_turn_does_not_summarise_itself_once_per_round`
`verified-by: by-construction (the default budget is asserted below the smallest useful window at compile time)`

<a id="COMPACT-9"></a>
### COMPACT-9: the budget is the window the endpoint advertises, where it advertises one

`GET /v1/models` reports a figure per model, and it is used as the budget for whichever model the
person chose. The default only stands in: for `automatic-bravebot`, whose model is resolved per
request so no one window describes it, and for an entry that reports nothing. A budget set by hand
outranks both.
A figure the endpoint advertises is believed even where it is small, and never raised toward
something more comfortable.

**The window is looked up whenever a model is in force, not only when one is picked.** A choice
outlives the session that made it, and nothing on disk remembers the window that came with it, so a
session starting on a model chosen earlier asks the endpoint again. A listing that cannot be fetched
leaves the default in place and says nothing: a session that is merely offline should not open with a
complaint about a request nobody asked for.

**Why.** The default was a single constant standing in for a figure that varies across the roster by
a factor of thirty. Sessions compacted at 24,000 tokens against a model advertising 102,400, giving
up three quarters of the conversation it could have held, while models advertising 6,400 had a budget
their window could never reach, so compaction could not fire for them at all. One number cannot be
right for both.

**On the unit.** The field is named `long_conversation_warning_character_limit` and holds **tokens**.
The endpoint computes it as `conversation_token_limit * 0.8`, so it is a token count with a fifth
already held back for the reply, and it is usable as a budget with no conversion. Reading the name
literally and dividing by a characters-per-token estimate would compact roughly four times sooner
than necessary, which is a mistake this project made before measuring the endpoint.

`verified-by: bravebot_aichat::models::the_advertised_window_is_kept_as_tokens`
`verified-by: bravebot_aichat::models::an_entry_that_advertises_no_window_reports_none`
`verified-by: bravebot_aichat::models::the_placeholder_window_is_read_as_nothing_said`
`verified-by: bravebot_aichat::models::automatic_advertises_no_window`
`verified-by: bravebot_config::lib::an_advertised_window_replaces_the_default`
`verified-by: bravebot_config::lib::a_small_advertised_window_is_believed_rather_than_raised`
`verified-by: bravebot_config::lib::the_placeholder_window_is_not_adopted`
`verified-by: bravebot_config::lib::nothing_advertised_leaves_the_default_alone`
`verified-by: bravebot_config::lib::a_budget_set_by_hand_is_not_replaced_by_an_advertised_one`
`verified-by: bravebot_config::lib::adopting_the_budget_already_in_use_reports_no_change`
`verified-by: bravebot_tui::app::the_window_of_a_model_chosen_earlier_is_found_in_the_listing`
`verified-by: bravebot_tui::app::nothing_chosen_has_no_advertised_window`
`verified-by: bravebot_tui::app::a_model_the_listing_no_longer_offers_has_no_window`
`verified-by: bravebot_tui::app::a_model_that_advertises_nothing_has_no_window`
`verified-by: bravebot_tui::app::a_run_with_no_session_adopts_the_window_of_the_model_in_force`
`verified-by: bravebot_tui::app::a_run_whose_model_no_roster_describes_keeps_the_default`

<a id="COMPACT-10"></a>
### COMPACT-10: every compaction is recorded, with what it gave up and where it happened

The trail says how many messages were summarised, how many were kept word for word, what the
summary cost, and which tool-calling round it landed on. A compaction asked for between rounds
reports round zero, having interrupted nothing. Counts and nothing else, so the trail carries no
more content than it did before.

Recorded after the conversation is shortened, so a summary refused on the way back in leaves no
line claiming one was made.

**Why.** A compaction is the point a session stops being able to remember what it did, and it is
read back afterwards to ask exactly two things: where that happened, and whether it helped. A
record saying only that a summary was adopted answers neither, since one that gave up ninety
messages and one that gave up three read identically. The round matters most for a compaction the
budget forced, which lands in the middle of a turn's work: a timestamp says when in the session, and
only the round says how far into the work.

`verified-by: bravebot_agent::turn::the_trail_says_what_a_compaction_gave_up_and_what_it_cost`
`verified-by: bravebot_agent::turn::the_trail_says_which_round_a_compaction_landed_on`

<a id="COMPACT-11"></a>
### COMPACT-11: the summariser asks for no cache of the exchange it gives up

The summariser's request marks its own instructions for caching and marks nothing on the end of the
exchange it carries. Both wire formats a backend speaks are told the same thing, so which service
answers does not change what is asked for.

**Why.** A cache write is charged above the fresh tokens it covers, and it buys something only where
a later request sends the same prefix again. Nothing sends this prefix again: the request opens with
instructions no other request uses, ends with an instruction of its own, and the exchange between
them is about to be replaced. It is also among the longest prefixes a session sends, being the
conversation down to the cut, so marking the end of it pays the premium where it costs most and buys
a cache nothing can read.

**The instructions keep their mark, for the ordinary reason.** They are the same bytes every time a
summary is asked for, which is what marking a prompt is for. Nothing here rests on a service storing
them: these instructions are a few hundred tokens, short enough that a service with a minimum
cacheable prefix stores nothing at all, so the mark buys a read where one is offered and costs a
write at most where it is not.

**Marking the end is the default a request opts out of.** The arithmetic is the other way round for
the rounds of a turn, each of which sends the round before it again, and that is the case the default
is written for. This is a request that says otherwise.

`verified-by: bravebot_agent::turn::the_summariser_asks_for_no_cache_of_the_exchange_it_gives_up`
`verified-by: bravebot_aichat::protocol::a_request_giving_up_its_conversation_marks_the_prompt_alone`
`verified-by: bravebot_bedrock::protocol::a_request_giving_up_its_conversation_keeps_the_prompts_breakpoint_alone`

<a id="COMPACT-12"></a>
### COMPACT-12: what a compaction costs the cache is the conversation

A compaction rewrites the conversation and rewrites nothing of the system prompt or the tool schemas,
so the round that follows one inside the same turn sends that prefix byte for byte and reads it back.
Nothing of the conversation is read back, and that round pays a write to establish the shortened one.
When a compaction happens is COMPACT-8's: the figure it compares is what the last request measured,
and what a cache holds is no part of it.

**Inside the turn, and nothing claimed past it.** A turn assembles its prompt and its tool schemas
once and every round of it sends the same ones, which is what makes the round after a compaction
comparable with the round before. A prompt assembled for a later turn can differ for reasons a
compaction has no part in, among them the date, a tick's number under a loop, and an instruction file
re-read from disk; and a turn that has spent its tool budget sends no schemas at all from then on.

**Why it is stated rather than worked out.** A request marks two prefixes and a compaction rewrites
one of them, so taking the pair as a unit gives the wrong answer twice: it reads as though a
compaction cost the prompt's cache as well, which makes compacting look more expensive than it is,
and it leaves a reader expecting the conversation's cache to survive a rewrite of the conversation.

**Why the write is paid rather than avoided.** The alternative is holding a compaction back to keep a
prefix warm, which trades a request the server will refuse for one cache read. A conversation that
cannot be shortened stays long, and that is the outcome this document is here to prevent.

`verified-by: bravebot_agent::turn::a_compaction_leaves_the_prompt_a_breakpoint_covers_alone`
