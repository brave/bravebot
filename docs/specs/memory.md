---
id: MEM
title: Persistent memory
status: normative
---

## Scope

Long-lived memory split into **core** (small stable facts on every turn, no embeddings) and
**episodic** (embedded task or project notes, recalled by cosine similarity).

Incognito reads both stores but does not write or dream.

### Why Memory is Split

Core memories are facts that are useful across all sessions and chats, for example a user's name
or general high level preferences.

Episodic memories are task specific preferences or details that, whilst they are useful across
some tasks, are not needed for every chat.

More generally the memory system should support different types of memory being added, beyond core 
and episodic, if required.

### Embeddings

Since not all episodic memories are retrieved on each turn, a retrieval mechanism is needed to add
relevant memories to the chat context. This can be achieved by embedding episodic memories when written,
embedding a user prompt at inference time, and then retrieving the top N episodic memories above a 
cosine similarity threshold. This has benefits over using a generative LLM to select memories; it is faster,
cheaper, and deterministic. 

Embeddings can be generated and stored locally.

#### Embedding Model

The embedding model should be small and multilingual, capable of running on-device:

[embedding-gemma300M](https://huggingface.co/unsloth/embeddinggemma-300m-GGUF) quantised and available under the [gemma licence](https://ai.google.dev/gemma/terms), approximately 300MB under 8 bit quantisation.

## Clauses

<a id="MEM-1"></a>
### MEM-1: Only the planner can write memories and access the remember tool.

Data that is passed to the remember tool can only be trusted data from the planner. This prevents memory poisoning and ensures all memories are trusted content.

`verified-by: bravebot_core::policy::a_remember_without_an_endorsement_is_refused`
`verified-by: bravebot_core::policy::an_endorsement_for_one_row_does_not_remember_another`
`verified-by: bravebot_core::policy::a_private_argument_is_refused_rather_than_read`
`verified-by: by-construction (remember is omitted from tools::for_delegate, dispatch handles it only when tools.delegated is false, and the handler calls Policy::read_planner_argument on every field; see crates/agent/src/tools.rs)`

<a id="MEM-2"></a>
### MEM-2: Only the planner can read memories.

Memories should not be accessible by a processor. If something in a memory would be useful for a processor to complete it's tasks, the planner should explicitly pass this information to the processor in the message.

`verified-by: bravebot_core::policy::only_the_slots_it_was_given_reach_a_processor`
`verified-by: bravebot_agent::turn::a_tool_call_from_a_processor_does_nothing`
`verified-by: by-construction (processor::run never loads core rows or calls preamble::compose; PROC-1 in processors.md)`

<a id="MEM-3"></a>
### MEM-3: incognito reads and does not write.

An incognito session may recall from an existing store but does not remember, dream, or change memory files on disk.

<a id="MEM-4"></a>
### MEM-4: recalled memories should appear in the session logs;

Both core and episodic memories that are used during a session should be observable and auditable by a user.

`verified-by: bravebot_agent::turn::what_did_not_load_reaches_the_interface_when_it_is_learned`
`verified-by: bravebot_agent::turn::what_did_not_load_is_reported_even_when_the_turn_never_finishes`
`verified-by: bravebot_agent::conversation::every_call_in_a_round_is_recounted`
`verified-by: by-construction (live core rows recalled at turn start are reported through reporter.notice; remember tool calls are stored and recounted like any other tool; see crates/agent/src/turn.rs and crates/agent/src/conversation.rs)`

<a id="MEM-5"></a>
### MEM-5: users can edit memories directly via a /memory command.

If a user wishes to manually add, remove or alter memories this allows them to do it without interfacing with a model.

`verified-by: bravebot_tui::app::every_command_in_the_table_dispatches`
`verified-by: bravebot_tui::app::no_command_is_sent_as_a_prompt_while_a_turn_runs`
`verified-by: bravebot_agent::memory::replace_rewrites_the_file`
`verified-by: by-construction (/memory is in commands(), dispatches to Action::EditMemory, and memory_prompt::edit persists with replace_live_core; see crates/tui/src/app.rs and crates/tui/src/memory_prompt.rs)`

<a id="MEM-6"></a>
### MEM-6: dreaming is episodic-only and at most once per day

A turn schedules dream consolidation on a background thread when it is due, at most once per calendar day. The turn does not wait for dream to finish; opening the store for a later turn waits until any in-flight dream for that directory completes. Dreaming decays, merges, caps, and promotes episodic rows with `hits` greater than X into core memory. Core memories are not modified by decay or merge.

<a id="MEM-7"></a>
### MEM-7: drift in the text store is repaired on load

A live episodic line with no vector is embedded on load in one batch rather than crashing. A corrupt or missing vector file is rebuilt from the live JSONL. Any edited memories are re-embedded and embeddings from deleted memories are removed.

<a id="MEM-8"></a>
### MEM-8: core reaches the system prompt; episodic reaches the user turn

When memory is enabled, all live core rows from [INSTR-1](instructions.md#INSTR-1) appear in a
**Core** block in the system prompt on every turn.

When episodic memory is configured, the task prompt for the turn is embedded as a query, up to five
episodic hits with cosine similarity at least Z are ranked, and the hits are injected in a
**Memories** block in the user message for that turn, before what the person typed. Episodic recall
is not standing instructions and is not part of the system prompt. Like the preamble, it is
assembled afresh each turn and is not stored in the conversation as its own message; [MEM-4](#MEM-4)
is where recall is shown for audit.

`verified-by: bravebot_agent::turn::the_preamble_is_not_stored_in_the_conversation`
`verified-by: by-construction (live core rows are loaded in one_turn and passed only to preamble::compose, which adds a ## Core memory section; see crates/agent/src/turn.rs and crates/agent/src/preamble.rs)`

<a id="MEM-9"></a>
### MEM-9: embedding requests use input_type and client prefixes

Embed calls send `input_type` of `query` or `document`. The client prepends `queryPrefix` and `docPrefix` from settings when set, or EmbeddingGemma defaults when the model name matches. Dream merge compares stored document vectors and does not apply query prefixes. Pairs with cosine similarity at least Y and the same `mtype` are merged.

<a id="MEM-10"></a>
### MEM-10: core and episodic files

Core rows live in `core.jsonl` (no vectors). Episodic rows live in `episodic.jsonl` with `vectors.bin`. JSONL lines store `id`, `mtype`, `text`, `confidence`, `confirmed`, and episodic rows also store `hits`. Core rows are capped at twenty live entries.

`verified-by: bravebot_agent::memory::append_and_load_round_trip`
`verified-by: bravebot_agent::memory::replace_rewrites_the_file`
`verified-by: bravebot_agent::memory::append_refuses_at_cap`

<a id="MEM-11"></a>
### MEM-11: memory directory

By default, core memory lives under `~/.bravebot/agent_memory`. Settings may set
`autoMemoryDirectory` to another path (`~/…` expands against the profile home).

`verified-by: bravebot_agent::memory::default_store_is_under_state_home`
`verified-by: bravebot_config::settings::auto_memory_directory_is_read_from_settings`

<a id="MEM-12"></a>
### MEM-12: writes use the home helpers

Creates and writes under the memory directory go through `home::create_directory`,
`home::write_file`, and `home::append_to_file` so modes match [STATE-1](state-directory.md#STATE-1).

`verified-by: by-construction (append_core and replace_live_core use home::create_directory, home::append_to_file, and home::write_file on core.jsonl; see crates/agent/src/memory.rs)`
