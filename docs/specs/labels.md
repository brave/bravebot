---
id: LABEL
title: Labels and who may read what
status: normative
governs:
  - crates/core/src/label.rs
  - crates/core/src/value.rs
  - crates/core/src/reference.rs
  - crates/core/src/slot.rs
guards:
  - symbol: Labelled::new
    sites:
      - crates/agent/src/lsp.rs: 4
      - crates/agent/src/manifest.rs: 3
      - crates/agent/src/tools.rs: 22
      - crates/agent/src/workspace.rs: 7
      - crates/agent/tests/workspace.rs: 28
      - crates/aichat/src/lib.rs: 4
      - crates/bedrock/src/lib.rs: 5
      - crates/core/src/policy.rs: 90
      - crates/core/src/slot.rs: 5
      - crates/core/src/value.rs: 7
      - crates/mcp/src/http.rs: 2
      - crates/mcp/src/lib.rs: 1
      - crates/mcp/src/protocol.rs: 2
      - crates/mcp/src/stdio.rs: 2
      - crates/net/src/lib.rs: 2
  - symbol: Labelled::declassify
    sites:
      - crates/agent/src/aside.rs: 2
      - crates/agent/src/manifest.rs: 5
      - crates/agent/src/processor.rs: 1
      - crates/agent/src/tools.rs: 30
      - crates/agent/src/turn.rs: 4
      - crates/agent/src/vet.rs: 1
      - crates/agent/src/workspace.rs: 1
      - crates/agent/tests/workspace.rs: 41
      - crates/aichat/tests/client.rs: 2
      - crates/bedrock/src/lib.rs: 1
      - crates/core/src/policy.rs: 51
      - crates/core/src/value.rs: 1
      - crates/mcp/tests/http.rs: 1
      - crates/mcp/tests/stdio.rs: 2
      - crates/ui-bridge/tests/workspace.rs: 1
  - symbol: Labelled::trusted
    sites:
      - crates/agent/src/agents.rs: 1
      - crates/agent/src/attached.rs: 1
      - crates/agent/src/manifest.rs: 10
      - crates/agent/src/preamble.rs: 1
      - crates/agent/src/skills.rs: 3
      - crates/agent/src/tools.rs: 16
      - crates/agent/src/turn.rs: 3
      - crates/agent/src/workspace.rs: 9
      - crates/tui/tests/sessions.rs: 4
      - crates/agent/tests/workspace.rs: 158
      - crates/core/src/policy.rs: 23
      - crates/core/src/value.rs: 3
      - crates/ui-bridge/tests/workspace.rs: 2
  - symbol: Labelled::relabel
    sites:
      - crates/core/src/policy.rs: 1
      - crates/core/src/slot.rs: 1
      - crates/core/src/value.rs: 4
  - symbol: Declassification::authorise
    sites:
      - crates/core/src/policy.rs: 48
  - symbol: SlotStore::path_of
    sites:
      - crates/core/src/policy.rs: 5
      - crates/core/src/slot.rs: 1
  - symbol: SlotStore::verbatim_of
    sites:
      - crates/core/src/policy.rs: 2
      - crates/core/src/slot.rs: 1
  - symbol: SlotStore::home_of
    sites:
      - crates/core/src/policy.rs: 1
      - crates/core/src/slot.rs: 1
  - symbol: SlotStore::origin_of
    sites:
      - crates/core/src/policy.rs: 1
      - crates/core/src/slot.rs: 1
  - symbol: SlotStore::deferred
    sites:
      - crates/core/src/policy.rs: 2
      - crates/core/src/slot.rs: 1
  - symbol: Policy::where_a_slot_came_from
    sites:
      - crates/core/src/policy.rs: 3
  - symbol: Policy::release_where_a_slot_came_from
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 1
  - symbol: VettingSpec::where_it_came_from
    sites:
      - crates/core/src/policy.rs: 4
      - crates/core/src/vetting.rs: 1
  - symbol: Policy::present
    sites:
      - crates/agent/src/aside.rs: 1
      - crates/agent/src/attached.rs: 1
      - crates/agent/src/goal.rs: 1
      - crates/agent/src/lsp.rs: 3
      - crates/agent/src/turn.rs: 9
      - crates/core/src/policy.rs: 8
  - symbol: Policy::render_in_place
    sites:
      - crates/agent/src/manifest.rs: 7
      - crates/agent/src/skills.rs: 2
      - crates/agent/src/tools.rs: 19
      - crates/agent/src/turn.rs: 1
      - crates/core/src/policy.rs: 6
  - symbol: Policy::render_pair_in_place
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 1
  - symbol: note_for
    sites:
      - crates/agent/src/tools.rs: 9
  - symbol: Policy::label_model_output
    sites:
      - crates/agent/src/tools.rs: 4
      - crates/core/src/policy.rs: 16
  - symbol: Policy::adopt_model_output
    sites:
      - crates/agent/src/aside.rs: 1
      - crates/agent/src/compact.rs: 1
      - crates/agent/src/goal.rs: 1
      - crates/agent/src/manifest.rs: 1
      - crates/agent/src/tools.rs: 1
      - crates/agent/src/turn.rs: 3
      - crates/core/src/policy.rs: 4
  - symbol: Policy::read_planner_argument
    sites:
      - crates/agent/src/tools.rs: 9
      - crates/agent/src/workspace.rs: 1
      - crates/core/src/policy.rs: 5
  - symbol: Policy::decode_transport
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/aichat/src/lib.rs: 2
      - crates/aichat/src/models.rs: 2
      - crates/bedrock/src/lib.rs: 2
      - crates/core/src/policy.rs: 2
      - crates/mcp/src/http.rs: 1
      - crates/net/src/lib.rs: 1
      - crates/net/tests/egress.rs: 2
      - crates/tui/src/update.rs: 1
documented-by: docs/website/docs/how-it-works.md
---

## Scope

What a label is, how one is assigned, which direction it may move, and which components may read
what carries one. Where an effect can land, meaning a write, a program run or a request out of this
process, is in [routing.md](routing.md). Which paths a person vouched for is in
[trust-map.md](trust-map.md).

## The lattice

<a id="LABEL-1"></a>
### LABEL-1: two axes, and a genuine lattice

```
L = I × C      I ∈ {T, U}      trusted / untrusted
               C ∈ {pub, priv} public  / private
```

Untrusted input degrades integrity; private input raises confidentiality. `(U,priv)` and `(T,pub)`
are **incomparable**, so this is a lattice rather than a pair of booleans and no total order may be
imposed on it. Integrity meets pessimistically and confidentiality joins pessimistically.

`verified-by: bravebot_core::label::middle_elements_are_incomparable`
`verified-by: bravebot_core::label::bottom_flows_everywhere_and_top_flows_nowhere`
`verified-by: bravebot_core::label::integrity_meet_is_pessimistic`
`verified-by: bravebot_core::label::confidentiality_join_is_pessimistic`
`verified-by: bravebot_core::label::ordering_is_reflexive_and_antisymmetric`
`verified-by: bravebot_core::label::ordering_is_transitive`
`verified-by: bravebot_core::label::display_matches_the_canonical_notation`

<a id="LABEL-2"></a>
### LABEL-2: a derived value is labelled by taint over its inputs

One untrusted input taints the result. One private input makes it private. The axes degrade
independently, the order of the inputs does not matter, and taint only ever degrades on each axis.
No inputs means no taint.

`verified-by: bravebot_core::label::taint_result_is_a_degradation_of_its_inputs`
`verified-by: bravebot_core::label::one_untrusted_input_taints_the_result`
`verified-by: bravebot_core::label::one_private_input_makes_the_result_private`
`verified-by: bravebot_core::label::axes_degrade_independently`
`verified-by: bravebot_core::label::taint_is_order_independent`
`verified-by: bravebot_core::label::taint_only_degrades_on_each_axis`
`verified-by: bravebot_core::label::no_inputs_means_no_taint`

## Who may read what

<a id="LABEL-3"></a>
### LABEL-3: nothing untrusted in the planner's context

Untrusted content is never placed in a message to the model. It is quarantined in a write-once
slot and the planner is given a **reference**: origin, line count, byte count, label, and for an
entry out of a listing whether it stands for a file or for a directory the walk stopped at. The
planner acts on content it cannot read by naming that reference, and the policy layer resolves it
when the write or the call actually happens. Where the content has to be changed rather than moved, it goes
to a processor, described in [processors.md](processors.md).

The origin is routing, and it is trusted where it is presented. The counts are numbers and the
label is an enum, but the origin is a string that reaches the planner's context, the trace and the
transcript verbatim, so it is one the driver chose: a path, a slot, or an argument a person
approved. It is never taken from the content it describes, nor from anything that answered the
request for that content. An origin a server wrote is untrusted content in the planner's context
wearing the driver's attribution, which is the thing this clause exists to stop.

`verified-by: bravebot_core::policy::untrusted_content_is_presented_as_a_reference`
`verified-by: bravebot_core::policy::trusted_content_is_presented_visibly`
`verified-by: bravebot_core::reference::a_quarantined_presentation_shows_no_content`
`verified-by: bravebot_core::reference::a_visible_presentation_shows_the_content`
`verified-by: bravebot_core::reference::a_description_names_the_shape_and_not_the_content`
`verified-by: bravebot_core::reference::a_description_says_how_to_refer_to_the_content`
`verified-by: bravebot_agent::turn::a_fetched_page_names_the_url_that_was_asked_for_and_not_where_a_redirect_went`
`verified-by: bravebot_agent::tools::an_edit_by_reference_that_cannot_be_read_does_not_name_the_file`

<a id="LABEL-4"></a>
### LABEL-4: nothing untrusted in the driver's context

The driver may **carry** a labelled value and hand it to an effect, but may not **read** one. The
type that carries a label offers no equality, no formatting, no dereference and no infallible
accessor, and its debug output redacts the value it holds. Reading requires a declassification
witness, which only the policy layer can mint: the part of `bravebot-core` that owns the gates, and
the only code here allowed to read untrusted bytes at all. Asking for them anywhere else returns a
refusal naming this rule, not a value.

The one accessor that needs no witness is fallible and refuses everything that is not already
`(T,pub)`, handing the value back untouched rather than a value it declined to release, so a caller
cannot take the untrusted case by discarding an error. Counting a value's lines and bytes is not
reading it: those two numbers are what LABEL-3 already puts in front of the planner, and the bytes
they were counted from never leave.

Everything else the driver needs out of a labelled value it asks a gate for, and every gate records
the read: the planner's own arguments, a transport envelope a decoder has to see, and a model's
reply on its way out of that envelope. A driver holding the bytes without one is the shape this
clause exists to stop.

**A slot's address is untrusted content too, and is not carried in a labelled value.** The file a
slot names, the file its bytes are a copy of, the file an answer produced from it belongs to, the
sentence saying where its bytes came from, and the file it is still waiting on are bare strings,
and where the slot came from a quarantined listing every one of them is a filename the planner was
never shown. So each accessor that hands one back takes a witness minted in the same module the
declassification witness is minted in, and the `guards` list above pins their uses file by file,
the way it pins a declassification. A decision relocated into the kernel and taken from a slot's
address therefore moves a pinned count, and one relocated into any module of `bravebot-core`
outside the gates does not compile at all, bar the single release named next.

One of those sentences does leave the kernel, because the person being asked whether to let a
slot's bytes through has to be told which file they are. It leaves through a single gate that
records the release, and the `guards` list pins that gate's uses file by file too, so a second
reader of a slot's address anywhere downstream is a line in a diff rather than a branch nobody
is shown. Apart from that release, outside the kernel a slot answers one question about its
address, whether asking for its bytes will open a file, which names nothing.

**`bravebot-core` and `bravebot-agent` are both the driver.** Moving a branch from one
into the other does not remove it.

`verified-by: bravebot_core::value::untrusted_values_cannot_be_read_without_a_witness`
`verified-by: bravebot_core::value::trusted_public_values_need_no_witness`
`verified-by: bravebot_core::policy::a_witness_permits_reading`
`verified-by: bravebot_core::value::debug_redacts_the_value`
`verified-by: bravebot_core::value::content_can_be_measured_without_being_read`
`verified-by: bravebot_core::policy::reading_an_argument_is_recorded`
`verified-by: bravebot_agent::turn::a_write_reads_the_file_it_replaces_through_a_gate_that_records_it`
`verified-by: bravebot_core::policy::decoding_a_transport_envelope_is_recorded_and_hands_back_the_label`
`verified-by: by-construction (Deref, PartialEq and Display are not implemented for Labelled, and its only witness-free accessor returns Err on anything but (T,pub))`
`verified-by: by-construction (Declassification::authorise is pub(in crate::policy), so no other module of bravebot-core and no crate downstream of it can mint a witness)`
`verified-by: by-construction (PathAuthority::mint is pub(in crate::policy) too, so SlotStore::path_of, SlotStore::verbatim_of, SlotStore::home_of, SlotStore::origin_of, SlotStore::deferred, Policy::where_a_slot_came_from and VettingSpec::where_it_came_from cannot be called from another module of bravebot-core or from any crate downstream of it, and Deferred is pub(crate); Policy::release_where_a_slot_came_from is the one accessor that can, and it records the release. Those eight are the whole of the surface returning a slot's address and every use of each is pinned above)`
`verified-by: bravebot_core::policy::a_listed_files_name_does_not_decide_how_the_trail_words_the_check`
`verified-by: bravebot_core::slot::a_slot_is_unread_only_while_it_is_waiting_on_its_file`

<a id="LABEL-5"></a>
### LABEL-5: a decision may be taken only from trusted content

Comparing text is a decision. On trusted content that is fine, because a vouched-for path holds
nothing an attacker wrote. On untrusted content it is refused: the gate for reading content hands
over the bytes when they are trusted and **refuses** otherwise, so a caller cannot quietly take the
untrusted case. This is why editing a file requires a trusted one: locating a passage to replace
is a comparison.

Integrity is the only axis that matters here. Workspace content is private as a matter of course,
and examining it in-process releases nothing.

A tool's arguments are held to the same rule, by the context rather than by the wrapper they
arrive in. Every argument is wrapped `(U,pub)` as a pessimism, which is what forces a proposed
path through a promotion gate rather than letting it pass for routing a person chose; the bytes
themselves are the planner's own words, and the integrity of those is the integrity of the context
the planner wrote them in. So an argument may be read while that context has met nothing untrusted,
and is refused once it has. Locating a passage to replace is the case that shows why: `old_text` is
compared against the file, and a comparison decides whether the write happens at all.

An argument naming something the driver minted is held the same way, since the planner is the one
who picks which of those names a call is about. [routing.md](routing.md) is where that case is
settled.

`verified-by: bravebot_core::policy::requesting_untrusted_content_is_refused`
`verified-by: bravebot_core::policy::an_argument_cannot_be_read_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_core::policy::a_reference_cannot_be_named_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_core::policy::a_private_argument_is_refused_rather_than_read`
`verified-by: bravebot_agent::tools::an_edit_from_a_trusted_context_replaces_the_passage`
`verified-by: bravebot_agent::tools::an_edit_is_refused_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_agent::tools::a_command_line_and_a_directory_are_read_from_a_trusted_context`
`verified-by: bravebot_agent::tools::a_command_line_is_refused_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_agent::tools::a_url_is_read_from_a_trusted_context`
`verified-by: bravebot_agent::tools::a_url_is_refused_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_agent::tools::a_job_name_is_read_from_a_trusted_context`
`verified-by: bravebot_agent::tools::a_job_name_is_refused_once_the_context_has_met_something_untrusted`

<a id="LABEL-6"></a>
### LABEL-6: minting a witness is not permission to inspect

A witness records that bytes moved somewhere they were already allowed to go: a filesystem write,
an HTTP body, a human's screen, or the standard input of a program a person endorsed
([run.md](tools/run.md#RUN-3)). Each of those destinations has a gate of its own, one for
putting content in front of the planner, one for reshaping it for display, and one for reading
trusted content. A declassification anywhere else is almost certainly a violation.

A program's standard input is on that list for the reason the other three are, and not because a
subprocess is trusted: the bytes are carried to a descriptor and read by something that is neither
the driver nor the planner, and which argv reads them is routing a person approved. What happens to
them past that point is [sandboxing.md](sandboxing.md)'s question, not this one.

The witness a gate mints says the bytes may go to that destination. It does not say they may be
examined on the way, so a caller holding released bytes may not then search them, count them or
branch on them. Whatever needs reading is done by the policy layer, on a value that is still
labelled, and recorded where it happens. Reshaping content for display is that: it is not a fourth
destination, and it is a read a driver may not do for itself.

A gate that releases content to a closure of the caller's is counted, not merely named: its call
sites are pinned per file in the `guards` front matter, so the next reshape is a line in a diff
rather than ordinary code. A function outside the kernel that takes a closure from its own caller
and hands it to such a gate is counted the same way, because the closure it forwards is written in
the driver and a new caller of the wrapper adds one without moving the gate's own count.
`Policy::read_trusted_content` is named and not counted, in [skills.md](skills.md), because it
refuses anything untrusted before it releases a byte: there is no content its callers' closures
could read that the driver may not read for itself.

`verified-by: bravebot_agent::tools::a_call_line_names_its_reference_inside_the_kernel`
`verified-by: bravebot_agent::tools::a_task_list_is_named_inside_the_reshape_that_builds_it`
`verified-by: bravebot_agent::turn::what_a_write_changed_is_diffed_inside_the_kernel_before_it_is_released`
`verified-by: bravebot_agent::turn::a_writes_reshape_is_labelled_by_the_file_it_replaces_and_not_by_the_body_alone`
`verified-by: bravebot_agent::turn::what_an_edit_changed_is_diffed_inside_the_kernel_before_it_is_released`
`verified-by: bravebot_agent::manifest::what_a_planned_write_changed_is_diffed_inside_the_kernel_before_it_is_released`
`verified-by: bravebot_agent::turn::the_lines_an_output_prompt_states_are_counted_inside_the_kernel`
`verified-by: bravebot_agent::turn::the_lines_a_vetting_prompt_states_are_counted_inside_the_kernel`
`verified-by: bravebot_agent::confirm::a_prompt_line_states_the_comparison_it_was_given_and_not_the_bytes`
`verified-by: bravebot_agent::turn::a_proposed_url_is_read_through_the_argument_gate`
`verified-by: bravebot_agent::turn::a_job_name_is_read_through_the_argument_gate`
`verified-by: bravebot_agent::turn::a_command_line_and_its_directory_are_read_through_the_argument_gate`

## Which direction a label may move

<a id="LABEL-7"></a>
### LABEL-7: labels only ever degrade

Integrity may go trusted to untrusted and never the reverse. Relabelling yields nothing rather than
upgrading, and refuses incomparable labels outright. Never build a labelled value by hand to give it
a better label than its inputs had: that is laundering, whichever crate it happens in. If a value
derived from untrusted input needs to be trusted for something to work, the
design is wrong, not the label.

`verified-by: bravebot_core::value::relabel_may_degrade`
`verified-by: bravebot_core::value::relabel_may_not_upgrade`
`verified-by: bravebot_core::value::relabel_refuses_incomparable_labels`
`verified-by: bravebot_core::label::degradation_is_not_the_lattice_ordering`
`verified-by: bravebot_core::label::trusted_input_cannot_launder_untrusted`
`verified-by: bravebot_core::label::top_of_taint_degrades_from_everything`

<a id="LABEL-8"></a>
### LABEL-8: a first label comes from provenance, and is not an upgrade

Model output is a function of the model's context and nothing else, so when the context holds only
trusted input, what it produced is labelled accordingly. The same road labels every other carrier in
the table below. Each is the **first** label such a value ever receives, assigned from provenance
the policy layer tracked.

If you find yourself relabelling a value that already has a label, stop: that is LABEL-7.

A reply arrives from a backend wrapped in the label the *network* gave it, because a JSON string
carries no provenance and a transport can know nothing else. Taking it out of that envelope and
giving it the context's label is one step in the policy layer, not two in a driver: a driver doing
it itself holds model output unlabelled in between, with nothing recording that it did.

**These are the roads in, all of them.** Every carrier a provenance decision is taken about is a
row, including the two where the decision is that there is no label to give, so opening a new road
means adding a row here, where a reviewer sees it. No row reads what it labels: provenance decides,
and the content has no say in it.

| What enters | The label it gets | Pinned by |
|---|---|---|
| what a capability observed | the capability's own, one per capability, and an effect has none to give | `verified-by: bravebot_core::policy::observation_labels_come_from_the_capability` |
| a file read from the workspace | private, and trusted only where somebody vouched for the path | `verified-by: bravebot_core::policy::a_read_from_a_trusted_path_is_trusted` |
| a listing or a search across several paths | private, and trusted only where every path it visited is | `verified-by: bravebot_core::policy::a_read_over_several_paths_is_trusted_only_where_every_path_is` |
| a file the user named in a prompt or dropped on the window | trusted, because naming a file is vouching for it | `verified-by: bravebot_core::policy::a_file_the_user_named_is_read_as_trusted_though_nothing_else_is` |
| what a program printed | untrusted and private, since what it did is unknown | `verified-by: bravebot_core::policy::an_opaque_program_always_yields_untrusted_private_output` |
| what a program that can only transform its input printed | its input's label, carried through unchanged | `verified-by: bravebot_core::policy::a_filter_passes_an_untrusted_label_through_unchanged` |
| what such a program printed with no input at all | trusted and public, because it is a function of nothing an attacker influenced | `verified-by: bravebot_core::policy::a_filter_with_no_input_yields_trusted_output` |
| what a line whose every step is accounted for printed | trusted and private, because the steps were vouched for and the output is a function of what went in | `verified-by: bravebot_core::policy::a_line_that_only_reads_vouched_for_paths_comes_back_trusted` |
| a line the user ran themselves | trusted and private, because nobody steers a keystroke | `verified-by: bravebot_core::policy::what_a_command_the_user_typed_printed_is_trusted_and_private` |
| input piped into the process | untrusted and private, because a pipe has no path anyone could vouch for | `verified-by: bravebot_core::policy::piped_input_is_labelled_untrusted_and_private` |
| the user's own configuration, and a skill kept beside it | trusted and public, because putting a file there is the grant | `verified-by: bravebot_core::policy::configuration_the_user_placed_is_trusted_from_where_it_came_from` |
| what the planner wrote | public, at the integrity of the context it was written in | `verified-by: bravebot_core::policy::model_output_from_a_clean_context_is_trusted` |
| a reply taken out of a transport's envelope | the context's, never the network's | `verified-by: bravebot_core::policy::adopting_model_output_takes_the_context_s_label_not_the_transport_s` |
| an answer a person typed to a question | trusted and public, because a person wrote it | `verified-by: bravebot_core::policy::a_typed_answer_is_trusted_because_a_person_wrote_it` |
| what a processor produced | taint over the inputs it was given | `verified-by: bravebot_core::policy::an_output_is_labelled_by_taint_over_the_inputs` |
| one slot's bytes a person read on their screen and vouched for | trusted and private, because a person read them and said so, and the slot itself keeps what it had | `verified-by: bravebot_core::policy::output_a_person_vouched_for_comes_back_trusted` `verified-by: bravebot_core::policy::vetted_content_a_person_vouched_for_comes_back_trusted` |
| a picture pasted at the keyboard | none, because it joins the user's own message, which carries none either, so it is recorded instead | `verified-by: bravebot_core::policy::a_pasted_image_is_recorded_in_the_audit_trail` |
| a prompt typed while a turn is running | none, for the same reason, and recorded the same way | `verified-by: bravebot_core::policy::an_interjection_is_recorded_in_the_audit_trail` |

Where a path is known, integrity is the trust map's answer about that path rather than the
capability's, which is what the three rows for reads say and why the first row is the label a read
starts from. Which paths a person vouched for is in [trust-map.md](trust-map.md).

Three carriers a reader may go looking for are absent, none of which takes a first label. A
delegate's reply is model output, labelled in the delegate's own run by the row for what the planner
wrote. Content restored from a resumed session keeps the labels it was given when it first arrived,
and what resuming does to the integrity of the context is LABEL-9. The user's own message is not
labelled at all, which is why the two carriers that join it take no label either.

`verified-by: bravebot_core::policy::adopting_model_output_from_a_fallen_context_stays_untrusted`
`verified-by: bravebot_core::policy::only_a_value_a_transport_labelled_can_be_adopted_as_model_output`

<a id="LABEL-9"></a>
### LABEL-9: context integrity falls when the planner is shown something, never when a turn reads it

Context integrity only ever falls, and it falls when content is put in front of the planner. A
quarantined read
puts a reference in the context, not the bytes, and a slot id with a line count carries no
instruction. A paste does not lower it either, and resuming cannot raise it.

**Why.** Lowering integrity at the observation would label the planner's own words untrusted on the
strength of a file it never saw, and `present` would then quarantine the planner from itself.
Never move this back to the observation.

**This cannot happen today.** A context only becomes untrusted by resuming one that already was,
and nothing makes one untrusted in the first place: untrusted content is quarantined rather than
shown, and the only place the context absorbs anything is where content was trusted enough to show.
The gate is here anyway, because what makes that closure safe to rely on is that something refuses
if it ever stops holding. If a change ever lets untrusted bytes into the planner's context, this is
what catches it.

`verified-by: bravebot_core::policy::context_integrity_never_recovers`
`verified-by: bravebot_core::policy::a_quarantined_read_leaves_the_planner_able_to_see_its_own_words`
`verified-by: bravebot_core::policy::a_pasted_image_does_not_lower_what_the_context_has_met`
`verified-by: bravebot_core::policy::resuming_cannot_raise_the_integrity_of_a_context`
`verified-by: bravebot_core::policy::answering_never_raises_a_context_that_has_already_fallen`
`verified-by: bravebot_agent::turn::what_the_planner_writes_after_a_quarantined_read_stays_trusted`

## Carrying a label out of this process

<a id="LABEL-10"></a>
### LABEL-10: content released to a surface in another process carries its label with it

Where released content leaves this process for a surface to draw, the label it was released under
travels with it, as the label rather than as a word composed at the boundary. Every carrier of such
content states one: a preview of quarantined bytes, and a remark accompanying a write. The boundary
neither reads the content nor decides the label, so what arrives is what the gate released, and a
carrier added later states one too.

**Why.** A surface can only mark content it can still tell apart, and out there the label is the
whole of what it has to tell it apart by: the bytes arrive over a pipe carrying no provenance of
their own, so a boundary that dropped the label would not have lost a detail, it would have made the
content trusted by moving it. Everything on the far side would then look alike, and the surface
would have nothing distinguishing quarantined bytes from the planner's own words.

A word composed at the boundary is the same failure wearing the right shape. It is a function of the
boundary rather than of the gate, so it agrees with the label only while somebody keeps the two in
step, and the first label it stops distinguishing is the one nobody told it about.

What such a surface must then *do* with the label is addressed to surfaces rather than to this
process, and is [layering.md](layering.md)'s: a rule about the crates here cannot reach a program
this workspace does not compile. This clause is the half enforceable here, which is that the label
arrives at all.

`verified-by: bravebot_ui_bridge::wire::released_content_crosses_the_transport_with_the_label_it_was_released_under`
`verified-by: bravebot_ui_bridge::wire::quarantined_content_says_how_much_it_left_out`

## Known costs

- **Three places in the policy layer do look at untrusted bytes in order to decide something.**
  The clauses above say nothing may, so these are exceptions, and they are written down rather
  than left to be found.

  The first is splitting a processor's answer. A processor hands back a single piece of text that
  holds two things: a remark meant for the person watching, and the document to be written. It
  marks the line where the document starts, and the policy layer searches the text for that mark to
  find where to cut. Searching text is a decision, and this text came from a processor, so it is
  untrusted. The second is smaller: before a file is written back, the code checks whether the file
  being replaced ended in a newline, so the new one can end the same way.

  The mark is not a boundary and cannot be forged, because there is nothing to forge: the processor
  writes the whole answer, so it is entitled to put the mark wherever it likes, including more than
  once, and the first one is the one that counts. It is a declaration, not a guarantee.

  What makes that acceptable is worth spelling out, because "it looks at untrusted bytes" is
  exactly the shape of a real hole. Suppose an attacker owns the file, so they steer what the
  processor writes and where it puts the mark. Everything that buys them is on this list:

  - **Leave the mark out.** Then there is no document and the write is refused. They stopped
    something from happening, which is the direction this is built to fail in.
  - **Put the mark somewhere else, or more than once.** That shifts where the text is cut, so
    their words land in the document rather than in the remark. But the document becomes the body
    of a file the planner named and a person approved from a diff, and its contents were already
    coming from the attacker's file. They gain nothing they did not already have.
  - **Put their words in the remark.** It reaches a person's screen and stops there. It is drawn as
    untrusted content, inside a margin it cannot forge, and no model is given it
    ([PROC-11](processors.md#PROC-11)). It can still *lie*: the remark is free text attributed to
    the processor, so it can claim the document only fixes a typo when it does something else.
    Nothing checks a remark against the document it accompanies, and nothing could. What keeps
    that from mattering is that the remark is not the decision. The write is approved later, from
    a diff of the actual bytes, so a person who reads the diff sees what happens whatever the
    remark said, and the remark is drawn beside that diff so the two are read in one place
    ([PROC-12](processors.md#PROC-12)). The residue is that a plausible remark might still
    persuade somebody to skim the diff it sits above.
  - **Add or drop a trailing newline.**

  What is not on the list is the thing that would matter: choosing *which* file is written. That
  stays the planner's choice plus a person's approval, and no amount of steering the text changes
  it. The clauses above forbid decisions that redirect an effect, and none of these redirect
  anything.

- **The third is reading a verdict out of a check.** A second model is shown quarantined content (a
  slot the planner named, or a file somebody is about to be asked to vouch for) and answers with
  one word about whether the content looks like an attempt to give instructions. Reading that word
  is a decision taken from a reply that is a function of untrusted content, so it is untrusted too.
  [vetting.md](vetting.md) is the whole of what such a check is and may say, and
  [CHECK-10](vetting.md#CHECK-10) is which prompts run one.

  Suppose an attacker owns the content, so they steer both the content and, through it, what the
  check replies. Everything that buys them is on this list:

  - **Force the word `safe`.** With auto-vetting off, which is the default and every session
    nobody turned it on for, what that reaches is the banner on the prompt. The bytes are drawn
    below it either way, the keys that answer the question are offered either way, and nothing is
    promoted until a person says so, so this buys a quieter sentence above content the reader is
    still reading. On the vouch prompt the bytes drawn are the head of the file rather than all of
    it, so what the quieter sentence sits above is a preview; the answer still writes nothing on
    its own.
  - **Put the key that turns auto-vetting on in front of the person.** The standing key at a vetting
    prompt is drawn only where the check found nothing, so forcing `safe` is what offers it
    ([PROMPT-6](prompting.md#PROMPT-6)). It buys the offer and not the grant: the person has to
    press it, with the bytes on the same screen and a line beside the key saying what it turns on.
    It is drawn only on the two prompts that promote one slot's bytes, and the other direction is
    held shut, since a warning or a check that could not be made offers nothing. The reason the
    offer is put where the mode would have saved a keystroke rather than left out of the interface
    is that the alternative is somebody editing a settings file to get it, which is a decision made
    further from the thing it is about.
  - **Force the word `safe` where somebody turned auto-vetting on.** Then it promotes, and nobody
    reads the bytes. This is the one thing on the list that reaches the planner's context without a
    person in between, and it is why the mode is off until somebody asks for it in one of the three
    ways [CHECK-11](vetting.md#CHECK-11) names, and why the settings key that asks for it cannot be
    written into a checkout. What it buys is bounded by everything the verdict does not decide: one
    slot, whichever one the planner named, once, at `(T,priv)` so nothing leaves the machine and
    nothing becomes routing, with no trust rule written, and only on the routes
    [CHECK-12](vetting.md#CHECK-12) covers, so vouching for a path is still asked about. The slot is
    still the planner's choice and not the content's.
  - **Force the word `unsafe`, or reply with nothing a verdict can be read out of.** That lands on
    the prompt with the warning, which is the direction this is built to fail in.
  - **Put their words in the reason.** It reaches a person's screen and stops there. It is drawn
    inside a margin it cannot forge, no model is given it, and it is kept out of the audit trail.
    It can still *lie*, since nothing holds a reason against the content it describes and nothing
    could. The residue is that a plausible sentence might persuade somebody to skim, which is the
    same residue as the remark above and is
    [issue #23](https://github.com/brave/bravebot/issues/23).

  What is **not** on the list: choosing which slot is checked, choosing any destination, lowering
  confidentiality, writing a trust rule, or answering the one other prompt a check runs for, which
  is the vouch offer and is the one that writes such a rule
  ([CHECK-12](vetting.md#CHECK-12)). While auto-vetting is off, reaching the planner at all is not
  on it either, and a verdict is advice about bytes already on a person's screen.
