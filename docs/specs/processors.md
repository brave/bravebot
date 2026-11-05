---
id: PROC
title: Processors
status: normative
governs:
  - crates/core/src/processor.rs
  - crates/core/src/policy.rs
  - crates/agent/src/commit_message.rs
  - crates/agent/src/processor.rs
  - crates/choice/src/lib.rs
  - crates/choice/src/number.rs
guards:
  - symbol: ProcessorSpec::new
  - symbol: Policy::before_processor
  - symbol: Policy::before_chooser
  - symbol: Policy::read_a_number
  - symbol: Policy::compose_processor_input
  - symbol: Policy::write_belongs_here
  - symbol: Policy::declassify_into_workspace
documented-by: docs/website/docs/how-it-works.md
---

## Scope

The one component in the system that reads untrusted content. What it is given, what it may do,
and how what it produces is labelled and written.

## Why it exists

An agent that may not read a file also cannot change it: `edit_file` refuses on an untrusted file
and a whole-file `write_file` would need a body the planner could only have guessed.
Without processors the agent could answer questions about a repository nobody vouched for and do
no work in one.

## Clauses

<a id="PROC-1"></a>
### PROC-1: a processor holds no capabilities at all

| | |
|---|---|
| Tools | none, and the request carries no tool list at all |
| Memory | none: the messages are built from nothing each time |
| Conversation | one request, one reply, no loop to steer |
| Reads | exactly the references named in `reads`, and nothing else |
| Writes | at most one new reference, and nothing else |

A call mints **one slot or none**: the document, when the answer said where the document begins,
and nothing at all when it did not. The remark is a second output but never a slot: it is reported
to the person watching and cannot be read, written or passed to another processor.

**Why.** A processor with one tool is a second planner with untrusted content in its context,
which is the thing this design refuses. A loop would give a reply something to steer.

`verified-by: bravebot_core::policy::only_the_slots_it_was_given_reach_a_processor`
`verified-by: bravebot_agent::turn::a_tool_call_from_a_processor_does_nothing`
`verified-by: bravebot_agent::turn::a_processors_request_has_no_tool_list_no_history_and_no_second_round`
`verified-by: bravebot_core::policy::a_reference_to_nothing_is_refused`
`verified-by: bravebot_core::policy::a_processor_with_nothing_to_read_is_refused`
`verified-by: bravebot_core::policy::naming_the_same_reference_twice_is_refused`

<a id="PROC-2"></a>
### PROC-2: the spec is built by the driver and frozen before the run

The driver builds the spec, in one place, and nothing widens one afterwards.

`verified-by: by-construction (building a spec takes a SpecAuthority, minted only inside policy.rs, which is one of the files this spec governs, so no other module can build one at all; every field is private and no method takes &mut self, so nothing widens one once it is built)`

<a id="PROC-3"></a>
### PROC-3: the output label is computed before the processor runs

By taint over the inputs. Nothing the processor writes has any say in how what it writes is
labelled.

`verified-by: bravebot_core::policy::an_output_is_labelled_by_taint_over_the_inputs`

<a id="PROC-4"></a>
### PROC-4: the input is assembled by the policy layer

The policy layer, the part of `bravebot-core` that owns the gates, concatenates the slots. The driver carries the result wrapped and hands it to the call
without seeing it. The `instruction` comes from the planner and must not
be private.

What the planner writes is private once its context holds private content, which
[LABEL-8](labels.md#LABEL-8) lists the ways of reaching: a slot `vet_content` let through, output
`read_output` read to it, a delegate's report, a resumed conversation that already held some, or, for
a delegate, the private content its parent's context held when it was started
([DELEGATE-8](delegation.md#DELEGATE-8)).
From then on every `spawn_processor` call is refused. The refusal does not ask for a rewording, since none helps, and
names what still reaches a quarantined reference: a filter such as `grep` or `tail` run with
`stdin_ref` set to it, whose printed result `read_output` shows, and `vet_content` for a reference
that holds no text. `run`, `read_output` and the file tools keep working. The refusal names a route
only where the run is offered it: the filter where `run` and `read_output` are offered, and
`vet_content` where it is. A delegate that was shown none of the content is told it was started
holding what its parent's context held, that no processor can be started in the run, and to say in
its answer that it could not have a processor look at the content, so whoever asked can. A
delegate that starts closed is not offered `spawn_processor`, and its prompt does not say it may
hand files to processors. A reference made in such a
context is described without the move that names a processor, and the tool's description says that
`spawn_processor` is refused once the context holds private content and that those tools keep
working.

**Why.** A planner told to give a reference to a processor, and refused for the rest of the
conversation when it does, reads the whole of a long output with `read_output` instead, which is
the content the processor was the way to avoid reading.

`verified-by: bravebot_core::policy::a_private_instruction_cannot_direct_a_processor`
`verified-by: bravebot_agent::turn::a_planner_shown_private_content_cannot_instruct_a_processor`
`verified-by: bravebot_core::policy::the_refusal_of_a_processor_in_a_private_context_names_what_still_works`
`verified-by: bravebot_core::policy::the_refusal_in_a_private_context_names_only_the_routes_the_run_is_offered`
`verified-by: bravebot_core::policy::the_refusal_to_a_delegate_started_private_says_so_and_names_only_what_it_is_offered`
`verified-by: bravebot_agent::turn::a_delegate_started_under_private_content_is_not_offered_processors_and_is_told_why`
`verified-by: bravebot_core::policy::a_reference_names_no_processor_exactly_when_the_context_holds_private_content`
`verified-by: bravebot_core::reference::a_reference_closed_to_processors_names_none_and_keeps_the_other_moves`
`verified-by: bravebot_agent::tools::the_processor_is_described_as_refused_once_private_content_has_been_shown`
`verified-by: bravebot_agent::turn::a_planner_that_cannot_start_a_processor_is_not_pointed_at_one`
`verified-by: bravebot_agent::turn::reading_a_quarantined_file_by_reference_names_no_processor_in_a_private_context`

<a id="PROC-5"></a>
### PROC-5: the output is never shown to the planner

It is presented like any other untrusted content: a reference, and nothing else.

`verified-by: bravebot_agent::turn::the_planner_is_told_the_shape_of_what_a_processor_produced`
`verified-by: bravebot_agent::turn::what_a_processor_says_reaches_the_person_and_no_model`

<a id="PROC-6"></a>
### PROC-6: an answer is for one document, and belongs only where the planner said

A write of a processor's answer is refused anywhere but the file the planner said the call was
about. Where the planner said nothing, the answer belongs nowhere and may be written nowhere,
unless the whole of what the processor was given is one document, which is then the document its
answer is for. One file beside any other input is not that: an answer over a document and an
earlier answer is for neither of them.

A plan fixed before the run is no exception. It names its destinations in advance, which says
nothing about which document an answer is for.

**An answer about references that are no file.** A processor given only references that are no
file has no document to name: a page a fetch gave, what a program printed, or an answer made from
those. Its answer is for those references, and the one place it may be written is a file that does
not exist yet in the session's scratch directory, judged by where the host resolves the path.
The write is refused before anyone is asked when the path is outside the directory, climbs out of
it, names a file that is already there, or the session has no scratch directory. The refusal names
the answer's reference and no file, and says to give the answer to `run` as `stdin_ref`, which needs
no file. The file is recorded untrusted like any file an answer is written to. An answer over a
file beside any other input, or made from an answer about a file, is unchanged: it is for the file
the call was about, or for nothing. A plan names its files inside the workspace, so it has no
scratch path to write such an answer to, and the write is refused.

**Why.** This is not a label rule and cannot be one. Every gate passed when a planner wrote a
game's HTML into a Python script, because the destination was a path it named and a person
approved it.

A file input fixes where its answer goes, and a reference that is no file fixes nothing. Saving the
page to a file first would give the answer a path the planner chooses anyway, and costs two more
calls, one to save the page and one to read it back. Saving it with a script that opens the file
itself leaves the page readable, since such a file is not recorded as content the planner was not
shown ([TRUST-2](trust-map.md#TRUST-2) and Known costs in the same spec). The scratch directory
belongs to the session and is outside the project. A file already there is never replaced, because
a line a person approved earlier names a program and its arguments ([RUN-3](tools/run.md#RUN-3)) and
not the bytes of the file it reads.

`verified-by: bravebot_agent::turn::an_answer_about_nothing_in_particular_can_be_written_nowhere`
`verified-by: bravebot_agent::turn::an_answer_cannot_be_written_to_a_file_it_is_not_about`
`verified-by: bravebot_agent::manifest::a_planned_answer_about_nothing_in_particular_is_written_nowhere`
`verified-by: bravebot_agent::manifest::a_planned_answer_cannot_be_written_to_a_file_it_is_not_about`
`verified-by: bravebot_agent::manifest::a_second_transform_does_not_find_a_document_for_an_answer`
`verified-by: bravebot_agent::manifest::two_spellings_of_one_path_are_one_document`
`verified-by: bravebot_agent::manifest::a_planned_answer_cannot_be_written_to_a_name_that_differs_by_trailing_whitespace`
`verified-by: bravebot_agent::turn::an_answer_cannot_be_written_to_a_file_named_like_its_own_with_trailing_whitespace`
`verified-by: bravebot_core::policy::an_answer_about_references_that_are_no_file_may_become_a_new_file_in_the_scratch_directory_only`
`verified-by: bravebot_core::policy::an_answer_over_a_file_is_for_that_file_or_for_nothing_and_never_a_new_file_in_scratch`
`verified-by: bravebot_core::policy::an_answer_made_from_an_answer_about_no_file_is_for_no_file_either`
`verified-by: bravebot_agent::turn::a_summary_of_a_fetched_page_is_written_to_a_new_file_with_no_page_saved_first`
`verified-by: bravebot_agent::turn::an_answer_about_a_programs_output_and_an_answer_made_from_it_are_written_to_a_new_file`
`verified-by: bravebot_agent::turn::an_answer_about_a_reference_that_is_no_file_is_refused_outside_a_new_scratch_file`
`verified-by: bravebot_agent::turn::an_answer_about_a_reference_that_is_no_file_cannot_be_written_through_a_link_out_of_the_scratch_directory`
`verified-by: bravebot_agent::turn::an_answer_over_a_page_and_a_saved_file_is_still_written_nowhere`
`verified-by: bravebot_agent::turn::an_answer_about_a_reference_that_is_no_file_is_refused_where_there_is_no_scratch_directory`
`verified-by: bravebot_agent::manifest::a_planned_answer_about_a_fetched_slot_cannot_be_written_into_the_workspace`
`verified-by: bravebot_core::policy::a_file_beside_another_input_is_not_taken_for_the_document`

<a id="PROC-7"></a>
### PROC-7: nothing a processor writes is a file unless it says where the file begins

Everything before the document marker is a remark for the person watching: it reaches a
screen and stops, no model reads it, it is not part of any file, and it cannot be another
processor's input. Everything after the line is the document. An answer with no line names no
document and can be written nowhere.

**Why.** That way round on purpose. It was the other way, prose being the default and the line the
exception, and a processor explaining why it was leaving a Python script alone wrote the
explanation over the script. A processor has one output and has always wanted two, so forgetting
which is which has to fail towards changing nothing. Which document the answer is for is likewise
marked on the document, not left to the instruction to describe: one given two files and told in
prose the answer was for the second returned the first, and the first went into the second's file.

`verified-by: bravebot_core::policy::an_answer_without_the_line_names_no_document`
`verified-by: bravebot_core::policy::the_document_to_return_is_marked_on_the_document`
`verified-by: bravebot_core::policy::what_a_processor_says_is_split_from_what_it_produced`
`verified-by: bravebot_core::policy::a_processor_can_say_why_it_left_a_document_alone`

<a id="PROC-8"></a>
### PROC-8: quarantined content becomes a file body, and never anything else

A private slot may become a file body. That is sound only
because the destination is inside the boundary the bytes came from, so nothing leaves. The trust map
then records that path as untrusted, so reading it back does not launder it.

A slot that holds a picture or a PDF becomes the file it encodes, byte for byte. The same holds:
the destination is a path the planner named and a person approved, the write is made at the label
the slot had with its confidentiality lowered, and the effect that writes the file decodes it while
the driver reads nothing ([WRITE-9](tools/write-file.md#WRITE-9)).

**Never** reach for it for a network body, a command line, or a message to someone.

`verified-by: bravebot_core::policy::a_write_back_into_the_workspace_lowers_only_confidentiality`
`verified-by: bravebot_agent::turn::a_picture_slot_is_written_as_the_file_it_encodes_after_the_person_approves`

## Boundaries

<a id="PROC-9"></a>
### PROC-9: the confinement is the capability set, not an operating system boundary

There is no untrusted code involved: the call is made by the same trusted driver that makes every
other call. `bravebot-sandbox` confines processes running code we did not write, and putting a
processor in a subprocess would confine the wrong thing.

`verified-by: by-construction (a processor is a request this driver makes to a model, so the path holds no code of anybody else's for an operating system to contain, and what the reply may do is the set PROC-1 gives it and PROC-2 freezes before the call: none)`

<a id="PROC-10"></a>
### PROC-10: a check over one slot is not put in a subprocess either, and for the same reason

Asking a second model whether one quarantined slot looks like an injection attempt is a call of
the same shape as a processor's, so PROC-9 settles it: the code making the call is ours, and a
subprocess would hold the wrong thing. What such a check holds is narrower still, since it can
write nothing at all; [vetting.md](vetting.md) is where that is set out.

Recorded here so that the next person to ask finds the answer rather than reopening PROC-9. The
technique this is ported from does use a subprocess, and what it is containing there is an agent
with tools, a filesystem and a network, which can *act* when it is injected rather than merely
answer wrongly. Nothing here has any of those to begin with.

`verified-by: by-construction (a check is a request of the same shape, so PROC-9's reason holds unchanged, and what fixes one is narrower still: no output label, no output reference and no destination of any kind, which vetting.md sets out)`

## What the person sees

<a id="PROC-11"></a>
### PROC-11: a remark is drawn as untrusted content and nothing else

The remark reaches the person watching through the quarantined path and no other: reported as
content in no model's context, and drawn inside the margin the renderer owns, with every control
character replaced. So it cannot paint a margin of its own, cannot make its words look like
something bravebot said, and says on the screen whose words they are.

**Why.** The remark is free text a processor authors over an untrusted file, so it is
attacker-influenced like anything else a processor produces, and the whole of what it can do is
say something untrue. That ceiling is held by how the remark is drawn rather than by where it
goes, and nothing pinned it to that path: the reach it is reported with is set in one place,
[PROC-5](#PROC-5)'s test asserts the remark is shown and that no model saw it but nothing about
how it is drawn, and no test in the interface drew a remark at all.

`verified-by: bravebot_agent::turn::what_a_processor_says_reaches_the_person_and_no_model`
`verified-by: bravebot_tui::marking::a_remark_is_drawn_inside_the_margin_it_cannot_forge`

<a id="PROC-12"></a>
### PROC-12: a remark is put beside the write it describes

The remark is carried with the document it accompanies and drawn in the question about writing
that document, above the diff, as untrusted content. A write the planner wrote itself has no
remark and shows none.

It decides nothing. No gate reads it, an approval is given from the diff of the real bytes, and a
write goes the same way with the remark as without it. What it is for is that the claim and the
evidence are read in one place.

**Why.** Nothing checks a remark against the document it accompanies and nothing could, so a
processor can say it fixed one line while the document does something else. What kept that from
mattering was that the remark is not the decision, and what was left was the ordering: the remark
enters the transcript when the processor returns, and the question comes rounds later, so a person
read the diff with the claim about it some way up the screen. A claim can only be caught out
against the thing it is a claim about. "I only fixed the typo" beside three hundred changed lines
is visibly untrue; remembered from earlier it is not.

Capped, for the same reason it is drawn at all: a remark long enough to push the diff out of the
box would be the reader losing the evidence instead of gaining the claim. In lines and in width
where it is released, and again in drawn rows where it is drawn, because a line wider than the box
is several rows and only the box knows how wide it is
([PROMPT-4](prompting.md#PROMPT-4)). What either cap left out is said, and the transcript above
holds the fuller preview.

`verified-by: bravebot_agent::turn::what_a_processor_said_is_put_beside_the_write_it_describes`
`verified-by: bravebot_agent::turn::a_write_the_planner_wrote_itself_carries_no_claim`
`verified-by: bravebot_core::policy::a_remark_line_wider_than_the_box_is_cut_where_it_is_released`
`verified-by: bravebot_agent::manifest::a_planned_write_carries_what_the_processor_said_about_it`
`verified-by: bravebot_core::policy::what_was_said_about_a_document_is_released_for_the_screen_it_is_approved_on`
`verified-by: bravebot_core::policy::a_document_nobody_said_anything_about_has_nothing_to_show`
`verified-by: bravebot_tui::confirm::write::what_a_processor_said_is_drawn_beside_the_diff_it_describes`

<a id="PROC-13"></a>
### PROC-13: the `processorModel` setting names the model a processor runs on

`processorModel` holds one model name. A processor runs on it, resolved the way `--model` is, so a
tier word means that tier's configured model and anything else is sent as written. Absent, empty or
`inherit`, a processor runs on the model of the turn that started it. The planner's own rounds, and
vetting and compaction, are not affected: compaction and goal checks have `summaryModel`
([COMPACT-15](compaction.md#COMPACT-15)).

The name is configuration, not content. It is read from the person's own settings file and from the
file `--settings` names, and from no project or local layer ([BACKEND-24](backends.md#BACKEND-24)):
a layer that names it is dropped and `doctor` names the file. No tool argument, planner word or
file content chooses it, so the destination of a processor's input stays the one the person
configured. `--processor-model` ([CLI-34](cli.md#CLI-34)) names it for one run from the person's own
command line and outranks the file.

A model that needs a sign-in this machine has not made, that the managed layer refuses
([BACKEND-48](backends.md#BACKEND-48)), or that no configured service answers for is not swapped
for the turn's model. The call does not run, nothing is sent, and the person is told which model
the setting named. The planner is told only that the call did not run.

A processor answered by a model other than the one the setting named says so, naming the model as
the setting wrote it. It is compared as [DELEGATE-22](delegation.md#DELEGATE-22) compares a
delegate's: against the name that was sent, and not where the name is a handle the reply resolves.
The name that answered is left out of the sentence, which is the driver's own words.

The trail records the model each processor call asked for ([TRACE-10](trace.md#TRACE-10)). A manifest
run's processing steps use the same model ([MANIFEST-12](manifest.md#MANIFEST-12)).

**Why refuse rather than fall back.** A person naming a cheaper model for bulk rewrites is drawing a
cost boundary, and running the call on the turn's model would spend past it without anybody
choosing to.

**Why this does not change PROC-1 to PROC-3.** The model name is a request field. The processor
still holds no tools and no memory, makes one request, and has its output label fixed before the
call. A weaker model may rewrite a file worse, and a write is approved from the diff of the real
bytes ([PROC-12](#PROC-12)).

`verified-by: bravebot_config::settings::a_project_layer_cannot_name_the_processor_model`
`verified-by: bravebot_config::lib::the_processor_model_is_resolved_like_a_model_name`
`verified-by: bravebot_agent::turn::a_processor_request_carries_the_configured_model_while_the_planner_keeps_the_turns`
`verified-by: bravebot_agent::turn::a_processor_with_no_configured_model_runs_on_the_turns_own`
`verified-by: bravebot_agent::turn::a_processor_model_needing_a_sign_in_sends_nothing_and_says_so`
`verified-by: bravebot_agent::turn::a_processor_answered_by_another_model_than_the_configured_one_says_so`
`verified-by: bravebot_cli::running::doctor_names_the_file_of_a_checkouts_processor_model_that_is_not_obeyed`

<a id="PROC-14"></a>
### PROC-14: a command may run a processor over a diff the driver read, and release the draft to the person's box alone

`/commit-message` ([CMD-23](commands.md#CMD-23)) is a processor call that no planner asked for. The
driver reads a diff with the `read_git` reader under a policy of its own that holds `FileRead` and
the network, puts it in one slot at the label the read gave it, fixes the spec with
[`Policy::before_processor`](#PROC-2), and runs it. Everything PROC-1 to PROC-3 say holds: no tool,
no memory, one request, and an output labelled by taint over the diff. The instruction is the
driver's own fixed words and the format the person typed after the command, both public.

What differs from a call a planner makes is where the output goes. The document is released by the
display gate to the person's input box, and the remark is never released. The driver reads two
counts of the reader's own answer, whether it showed no file and whether it stopped at a cap, and,
for a diff nobody vouched for, one yes or no from a credential scan of its text. It reads nothing
else of the diff and nothing of the draft. A diff that holds what looks like a credential is not given to the
processor, whether or not its files are vouched for, since nobody is at a prompt to agree to it as
for a planner's read ([CRED-15](credential-protection.md#CRED-15)).

**Why.** The diff and the draft stay out of both contexts, as they do for a processor a planner
starts, and the person is the only reader of the draft before they decide what to send.

`verified-by: bravebot_agent::commit_message::a_range_is_drafted_from_its_diff_and_the_request_holds_no_tool`
`verified-by: bravebot_agent::commit_message::a_diff_holding_a_credential_is_not_sent`
`verified-by: bravebot_agent::commit_message::an_untrusted_diff_holding_a_credential_is_not_sent`
`verified-by: bravebot_agent::commit_message::a_repository_the_map_does_not_trust_is_not_read_or_sent`
`verified-by: bravebot_agent::commit_message::an_answer_with_no_marker_is_no_draft`
`verified-by: bravebot_tui::app::a_draft_answered_by_another_model_than_the_configured_one_says_so`

<a id="PROC-15"></a>
### PROC-15: a picture is given to a processor as a picture, and what it answers is text

A slot that holds a picture is part of a processor's input as a part of its own, with its media
type, and not as text in the body. The slot is one a file was read as ([READ-5](tools/read-file.md#READ-5))
or a server's tool result carried ([MCP-6](mcp.md#MCP-6)), and the reference to it says what kind of
thing it is. A processor given one answers as it does for any input: one document or none, as text,
labelled by taint over its inputs ([PROC-3](#PROC-3)), and the planner is given a reference to it
and never the text ([PROC-5](#PROC-5)).

Looking at a picture through a processor comes before `vet_content` lets it through. A picture
`vet_content` lets through makes the context private ([PROC-4](#PROC-4)), and from then on a
processor is refused. A delegate started after the planner's context became private starts closed,
so it cannot look at a picture through a processor either, and a task that needs one looked at is
started before then or left to the person.

**Why.** A picture carries whatever words are in it, so a planner that could look at one could be
instructed by it, and a model told to read a data URI reads it as characters. A processor holds no
tools and returns text the planner cannot read, so it can be asked what a picture shows without the
picture or its answer entering the planner's context.

`verified-by: bravebot_agent::turn::a_processor_is_given_a_picture_as_a_picture`
`verified-by: bravebot_agent::mcp::a_processor_is_given_a_servers_picture_as_a_picture`
`verified-by: bravebot_agent::turn::a_delegate_started_under_private_content_is_not_offered_processors_and_is_told_why`

<a id="PROC-16"></a>
### PROC-16: a processor may be asked for one number, and a crate outside the driver reads it from the answer

`spawn_processor` takes `choose_upto`, a whole number from 2 to 99 that the planner writes and that
is held to the context like every argument ([LABEL-5](labels.md#LABEL-5)). The call takes no
`about`, and a plan has no field for it ([MANIFEST-15](manifest.md#MANIFEST-15)).
`Policy::before_chooser` builds the spec as `Policy::before_processor` builds any other
([PROC-2](#PROC-2)), and refuses a limit outside that range. The processor is told that its whole
answer is one whole number from 1 to the limit, or 0 where no entry fits, and no remark and no
marker line are asked of it.

The answer is read once, by `bravebot-choice`. The policy layer releases the text to its reader
through `Policy::read_a_number` and takes one byte back. The byte is the number where the text,
with the white space at both ends taken off, is one or two ASCII digits with no sign and no leading
zero and the value is from 1 to the limit, and 0 in every other case, an empty text included. An
answer of no bytes at all is refused by the transport before anything reads it, as for every
processor, and no choice is made. A request that fails, whether on that answer, on content the
service refuses, on a reply cut at the token ceiling or on one the service reports as incomplete,
tells the planner one word of a closed set the driver wrote, as it does for every processor. The byte is labelled as the answer is, by taint over what the
processor was given ([PROC-3](#PROC-3)). Nothing else of the answer is kept, and neither this
process nor the gate compares the text or the byte. The gate refuses a spec that was not built to
choose, so the answer of a processor that writes a document is read by no one for a number.

The number is stored under a reference of its own kind, apart from the text slots, so it is no
document, no path and no body, and the only argument that takes it is one a person's declaration
names, as [ROUTE-10](routing.md#ROUTE-10) and [MCP-14](mcp.md#MCP-14) say ([PROC-6](#PROC-6),
[ROUTE-6](routing.md#ROUTE-6)). It is quarantined whatever its label. The planner is told the same
sentence whatever the answer read was, 0 included: a processor chose one entry of the list, the choice
is held as `ref:N`, and it is not shown. The reference states the limit and the label and no count of
lines or bytes ([LABEL-3](labels.md#LABEL-3)), and the person watching is told that a choice was made
and not which. The person asked about the call it fills is shown the number
([ROUTE-10](routing.md#ROUTE-10)). An answer with no bytes at all is refused by the transport for
every processor, and is not read.

**Why.** The decision is taken by a crate that holds no witness and no constructor, from text this
process does not read. What this process does afterwards depends on no value of the number, so a page
that talks the processor round buys one of at most a hundred numbers, or a failed request and the
word for it. The read is the ninth of the
[Known costs](labels.md#known-costs).

`verified-by: bravebot_choice::number::a_number_is_read_only_from_one_or_two_ascii_digits_in_range`
`verified-by: bravebot_choice::number::the_limit_is_the_planners_and_not_the_crates`
`verified-by: bravebot_choice::number::every_number_up_to_the_largest_limit_is_read_and_the_next_is_not`
`verified-by: bravebot_choice::lib::reading_an_answer_applies_the_planners_limit_and_labels_the_byte_as_the_answer_was`
`verified-by: bravebot_choice::lib::a_processor_that_was_not_asked_to_choose_has_no_number_read_from_its_answer`
`verified-by: bravebot_core::policy::reading_a_number_is_refused_for_a_spec_that_asked_for_a_document`
`verified-by: bravebot_core::policy::the_reader_is_handed_the_answers_text_and_the_planners_limit`
`verified-by: bravebot_core::policy::a_number_read_from_an_answer_keeps_the_label_of_the_answer`
`verified-by: bravebot_core::policy::the_trail_of_reading_a_number_holds_no_digit_and_is_the_same_for_every_answer`
`verified-by: bravebot_core::policy::a_chooser_carries_the_planners_limit_and_names_no_document`
`verified-by: bravebot_core::policy::a_chooser_is_refused_a_limit_outside_two_to_the_largest`
`verified-by: bravebot_core::policy::a_chooser_is_refused_for_the_reasons_a_processor_is`
`verified-by: bravebot_core::slot::a_choice_is_kept_apart_from_text_and_no_text_accessor_returns_it`
`verified-by: bravebot_core::policy::a_choice_is_no_reference_to_any_gate_that_takes_text`
`verified-by: bravebot_core::policy::a_stored_choice_is_a_reference_whatever_its_label_and_the_trail_holds_no_digit`
`verified-by: bravebot_core::reference::a_choice_reference_states_its_limit_and_no_count`
`verified-by: bravebot_agent::turn::a_processor_asked_to_choose_is_asked_for_a_number_and_not_a_document`
`verified-by: bravebot_agent::processor::a_processor_not_asked_to_choose_is_refused_before_anything_is_sent`
`verified-by: bravebot_agent::turn::a_choice_reaches_the_planner_as_a_reference_to_a_choice_and_nothing_else`
`verified-by: bravebot_agent::turn::the_planner_is_told_the_same_words_whatever_the_processor_answered`
`verified-by: bravebot_agent::turn::a_choice_cannot_be_a_path_a_body_or_a_document_for_a_processor`
`verified-by: bravebot_agent::browser_group_servers::a_free_text_answer_cannot_be_named_as_a_choice`
`verified-by: bravebot_agent::turn::a_call_asking_for_a_number_is_refused_unless_it_asks_in_the_planners_own_terms`
`verified-by: by-construction (Policy::read_a_number takes a function pointer that captures nothing of the caller's and returns a u8, so one byte is all of an answer that leaves the policy layer, and labels.md pins its uses file by file, so a second reader of an answer is a moved count that check-spec fails on)`

## Known costs

- **An untrusted file's contents reach the backend.** A processor is a model call, so working on
  a file nobody vouched for sends it where before it would have stayed on the machine. The
  destination is the one a trusted directory has always sent its files to, or the one
  [PROC-13](#PROC-13) names where the person configured a model for processors. What is new is only
  that the reader holds nothing.
- **One approval per candidate, not per change.** Where several files could be the one, each is
  read into its own slot, transformed with the same instruction, and written back to the path it
  came from. A candidate the answer marked no document for is not written at all, so the
  approvals count the files that changed rather than the files considered.
