---
id: SPAWN
title: spawn_processor
status: normative
governs:
  - crates/agent/src/processor.rs
---

## Scope

The call that starts a processor. `reads` is routing; `instruction` is content. The result is a
reference. What a processor is and what it may do is [processors.md](../processors.md); this spec
is the call surface.

## Clauses

<a id="SPAWN-1"></a>
### SPAWN-1: the call names the slots it may read, and gets nothing else

A processor is given exactly the references named in `reads`. A reference naming nothing is
refused, a call with nothing to read is refused, and naming the same reference twice is refused.

`verified-by: bravebot_core::policy::only_the_slots_it_was_given_reach_a_processor`
`verified-by: bravebot_core::policy::a_reference_to_nothing_is_refused`
`verified-by: bravebot_core::policy::a_processor_with_nothing_to_read_is_refused`
`verified-by: bravebot_core::policy::naming_the_same_reference_twice_is_refused`

<a id="SPAWN-2"></a>
### SPAWN-2: the instruction comes from the planner and may not be private

It is the only thing steering the call, and it comes from a context holding nothing an attacker
wrote.

`verified-by: bravebot_core::policy::a_private_instruction_cannot_direct_a_processor`

<a id="SPAWN-3"></a>
### SPAWN-3: the result is a reference, never text

The planner is told the shape of what was produced and no more, so an instruction can ask the
processor to decide as well as to rewrite without any of that judgement reaching the planner.

`verified-by: bravebot_agent::turn::the_planner_is_told_the_shape_of_what_a_processor_produced`
`verified-by: bravebot_agent::turn::a_quarantined_file_is_rewritten_by_a_processor`

<a id="SPAWN-4"></a>
### SPAWN-4: the call asks for no cache of the content it hands over

A processor's request marks its own instructions for caching and marks nothing on the end of the
pieces it carries.

**Why.** A cache write is charged above the fresh tokens it covers, and it buys something only where
a later request sends the same prefix again. A processor has no memory and is asked once, about
pieces assembled for that call alone, so the prefix a mark here would store is never asked for
again. The pieces are also the whole of what such a request carries, which puts the premium on the
longest part of it.

**The instructions keep their mark**, being the same bytes every time the same instruction runs,
which is what marking a prompt is for. What is given up is a write and no read.

`verified-by: bravebot_agent::turn::a_processor_asks_for_no_cache_of_the_content_it_reads`
