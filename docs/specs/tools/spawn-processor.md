---
id: SPAWN
title: spawn_processor
status: normative
governs:
  - crates/agent/src/processor.rs
documented-by: docs/website/docs/reference/tools.md
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
`verified-by: bravebot_agent::tools::a_processor_cannot_be_started_from_a_context_that_met_untrusted_content`

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

<a id="SPAWN-5"></a>
### SPAWN-5: a call that cannot be taken says what is wrong with it

A call whose `reads` is missing, is sent under the name `read`, or is a string holding an array
rather than the array, is told which of those it was. A call whose `reads` names no reference is
told that a processor works only on content already quarantined, and that a file made from nothing
is written with `write_file`. Each is a fixed sentence chosen from the argument's name and JSON
type ([TOOL-4](tool-surface.md#TOOL-4)), and the empty list is answered before the kernel's refusal
of it ([SPAWN-1](#SPAWN-1)) is reached.

**Why.** Told only that `reads` was required, a planner that had sent `read` holding a string sent
the same call again word for word. It then sent an empty list to have a processor draw a picture
from nothing, and the kernel's refusal said there was nothing quarantined to work on without saying
that the planner could write the file itself.

`verified-by: bravebot_agent::tools::a_processor_call_naming_its_references_wrongly_is_told_what_is_wrong`
`verified-by: bravebot_agent::tools::a_processor_given_nothing_to_read_is_pointed_at_write_file`

<a id="SPAWN-6"></a>
### SPAWN-6: the description says that text made from content the planner was not shown is a processor's to write

A summary, a translation, a rewrite, an explanation or a list of the points made of content the
planner was not shown is written by a processor, because the planner cannot write what it has not
read. The description must say so, naming those kinds, and must tell the planner how the answer
reaches a program that uses it:

- The instruction names the form wanted. The description gives the example of a plain-text summary
  of each of six stories, two sentences each, with each link kept. The `instruction` parameter asks
  for the document alone with "no remarks about what it did", and says to name the form and its
  length where the document is itself a summary, a translation or an explanation. The planning
  prompt gives the same advice for a changed file in the words "no commentary on what was changed or
  why".
- A script the planner writes is not the way, because it can only cut the original down.
- An answer about a reference that is no file, such as a fetched page or a run's output, is for no
  document and can be written nowhere. The original is saved to a file first, in the scratch
  directory when the planner was given one: either the run line that made it ends with `> path`, or
  `write_file` writes a fetched page from its reference. `read_file` on that file then gives the
  reference to name in `reads`, since a path is not a reference. The description says not to save the
  original with a script that opens the file itself, and gives no reason. The reason is that a file a
  script opens on its own is not recorded as holding what the planner was not shown
  ([TRUST-2](../trust-map.md#TRUST-2) and the known costs of [trust-map.md](../trust-map.md)), so a
  planner that saved a page that way could read the page back. A planner told that would know which
  way of saving a page does it, so the reason is kept here and the model-facing text carries the
  prohibition alone.
- The answer is written over the same file with `write_file`, and the program that sends it is run
  with the file's path as an argument. Or the answer's reference is given to `run` as `stdin_ref`,
  which needs no file.
- The `contents_ref` parameter of `write_file` says the same from its side. It names a fetched page
  as something it writes out and says that what a processor produced may be written only over the
  file the call was about. On a turn that holds `run` it also points at this description for saving
  a page to a file first. It offers no processor's answer as something to write anywhere, since the
  write of an answer about a page or an output is refused before anyone is asked.
- The recipient and the subject are the planner's. Only the body comes from the processor.
- The final message says what was sent and to whom, and calls the text a summary only if a
  processor wrote one.

A turn whose list holds no `run` is told which tool writes such text, to name the form, and when to
call the text a summary. The rest of the list names a script, a run line or `stdin_ref`, so it is
left out for that turn, and the `contents_ref` parameter of `write_file` does not point at it
because this description holds no route for that turn. The `run` description's pointer at the
processor is in [run.md](run.md).

**What stays true.** A processor is still given only the references named in `reads` and an
instruction from a context holding nothing private, still holds no tools, and its answer is still
never shown to the planner. The answer is labelled by taint over its inputs, so the file it is
written over is recorded as untrusted, and the driver carries it into that file and into the
program without reading it or branching on it. It is written only over the file the call was about,
which for a processor given exactly one file-backed reference is that file, so this adds no place an
answer may land.

**Where the answer lands, and who is asked about sending it.** The answer becomes a file body only
in the file the call was about, written back inside the workspace it came from
([PROC-8](../processors.md#PROC-8)). That is the one step in this flow that releases the answer, and
it is not used for a network body, a command line or a message. Sending is a separate `run` line the
planner types. Its program, arguments (the recipient, the subject and the file's path) and directory
are routing, and it meets the gate every `run` line meets ([RUN-5](run.md#RUN-5)). The person is not
always asked:

- A line that names the file as an argument is an ordinary line. That the file is untrusted adds no
  question and removes none, so the line is put to the person unless a vouched entry, a remembered
  line, a rule they wrote or their trust of the directory answers it. The audited table does not,
  because it needs every path the line reads to be vouched for.
- A line given the answer as `stdin_ref` uses the road [RUN-3](run.md#RUN-3) already gives, and this
  clause adds none. It is put to the person unless a rule they wrote decides it. The audited table
  could also answer it, by proving that the line writes nothing and reads only vouched paths
  ([RUN-3](run.md#RUN-3)), but a line that sends the answer is never so proved, because the table
  needs every program in it to write no file, start no process and open no socket
  ([CMDLINE-8](command-line.md#CMDLINE-8)). A vouched entry, a remembered line and the trust of the
  directory are not consulted for it, and the classifier is not asked
  ([AUTO-3](../auto-mode.md#AUTO-3)). The exception is an answer made from private content, such as
  a summary of a run's output. Its label follows what the processor read
  ([PROC-3](../processors.md#PROC-3)), so it is private, and no rule decides a line that is fed
  private input ([PERM-9](../permissions.md#PERM-9)). That line is put to the person whatever rules
  they wrote.

A person who chose to bypass permissions ([MODE-4](../permission-modes.md#MODE-4)) has answered both
in advance.

**Why.** A planner asked to send a summary of a page it may not read has two tools that can
produce one. A processor reads the page. A script cannot summarise, and cuts the page's own
sentences into the body without anyone reading them. Which one the planner picks depends on what the
descriptions it reads say a processor is for, so the description says that a summary, a translation,
a rewrite, an explanation or a list of the points made of such content is a processor's to write.
The `instruction` parameter asks for the document alone and to name the form and its length where
the document is itself a summary, so a planner that asks a processor for a summary is following it.
The route to the sending program is spelled out because `write_file` refuses an answer about a
fetched page or a run's output before anyone is asked. A planner told only to write the answer to a
file is refused and has no way on.

`verified-by: bravebot_agent::tools::a_processor_is_described_as_the_writer_of_text_made_from_content_not_shown`
`verified-by: bravebot_agent::tools::the_text_made_from_content_not_shown_names_run_only_where_the_turn_holds_one`
`verified-by: bravebot_agent::tools::the_processor_instruction_does_not_forbid_asking_for_a_summary`
`verified-by: bravebot_agent::tools::the_write_file_reference_text_does_not_offer_a_processors_answer_as_a_free_write`
`verified-by: bravebot_agent::turn::a_summary_of_a_scraped_page_reaches_the_sending_program_in_the_file_the_page_was_saved_to`
`verified-by: bravebot_agent::turn::a_summary_of_a_fetched_page_reaches_the_sending_program_once_the_page_is_saved_to_a_file`
`verified-by: bravebot_agent::turn::an_answer_about_a_reference_that_is_no_file_cannot_be_written_to_the_file_the_sender_reads`
`verified-by: bravebot_agent::turn::a_processor_is_not_given_the_path_of_the_file_a_page_was_saved_to`
`verified-by: bravebot_agent::turn::a_summary_of_a_fetched_page_reaches_the_sending_program_on_its_standard_input`
`verified-by: bravebot_core::policy::a_rule_decides_a_line_fed_a_public_reference_and_not_one_fed_a_private_reference`
