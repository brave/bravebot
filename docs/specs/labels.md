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
      - crates/agent/src/backend.rs: 2
      - crates/agent/src/classifier.rs: 1
      - crates/agent/src/commit_message.rs: 1
      - crates/agent/src/conversation.rs: 1
      - crates/agent/src/hunks.rs: 2
      - crates/agent/src/lsp.rs: 6
      - crates/agent/src/manifest.rs: 4
      - crates/agent/src/mcp.rs: 2
      - crates/agent/src/skills.rs: 1
      - crates/agent/src/tools.rs: 31
      - crates/agent/src/tools/tests/untrusted_refusals.rs: 2
      - crates/agent/src/turn.rs: 1
      - crates/agent/src/workspace.rs: 19
      - crates/agent/tests/incognito.rs: 1
      - crates/agent/tests/workspace.rs: 47
      - crates/aichat/src/anthropic.rs: 6
      - crates/aichat/src/call.rs: 1
      - crates/aichat/src/generate_content.rs: 5
      - crates/aichat/src/responses.rs: 4
      - crates/aichat/src/lib.rs: 8
      - crates/aichat/src/ollama_chat.rs: 5
      - crates/bedrock/src/lib.rs: 6
      - crates/connector/src/lib.rs: 2
      - crates/core/src/policy.rs: 158
      - crates/core/src/slot.rs: 14
      - crates/core/src/value.rs: 9
      - crates/core/tests/browser_group_aliases.rs: 1
      - crates/mcp/src/http.rs: 2
      - crates/mcp/src/lib.rs: 1
      - crates/mcp/src/protocol.rs: 6
      - crates/mcp/src/stdio.rs: 2
      - crates/net/src/lib.rs: 4
  - symbol: Labelled::declassify
    every-spelling: yes
    sites:
      - crates/agent/src/aside.rs: 3
      - crates/agent/src/commit_message.rs: 2
      - crates/agent/src/file_preview.rs: 1
      - crates/agent/src/images.rs: 1
      - crates/agent/src/manifest.rs: 5
      - crates/agent/src/mcp.rs: 4
      - crates/agent/src/processor.rs: 1
      - crates/agent/src/servers.rs: 1
      - crates/agent/src/tools.rs: 30
      - crates/agent/src/tools/recall.rs: 1
      - crates/agent/src/tools/tests/arguments.rs: 1
      - crates/agent/src/tools/tests/delegate_offers.rs: 1
      - crates/agent/src/tools/tests/editing.rs: 5
      - crates/agent/src/tools/tests/questions.rs: 1
      - crates/agent/src/tools/tests/scheduling.rs: 3
      - crates/agent/src/tools/tests/todos.rs: 1
      - crates/agent/src/tools/tests/watching.rs: 1
      - crates/agent/src/turn.rs: 6
      - crates/agent/src/vet.rs: 2
      - crates/agent/src/workspace.rs: 4
      - crates/agent/tests/workspace.rs: 75
      - crates/aichat/tests/client.rs: 14
      - crates/bedrock/src/lib.rs: 4
      - crates/connector/src/lib.rs: 1
      - crates/core/src/policy.rs: 79
      - crates/core/src/value.rs: 1
      - crates/mcp/tests/http.rs: 6
      - crates/mcp/tests/openshell.rs: 1
      - crates/mcp/tests/stdio.rs: 9
      - crates/ui-bridge/tests/workspace.rs: 1
  - symbol: Labelled::trusted
    sites:
      - crates/agent/src/agents.rs: 1
      - crates/agent/src/attached.rs: 1
      - crates/agent/src/commit_message.rs: 3
      - crates/agent/src/connectors.rs: 1
      - crates/agent/src/tools/message.rs: 1
      - crates/agent/src/tools/link.rs: 1
      - crates/agent/src/manifest.rs: 13
      - crates/agent/src/preamble.rs: 1
      - crates/agent/src/server_calls.rs: 2
      - crates/agent/src/skills.rs: 5
      - crates/agent/src/styles.rs: 1
      - crates/agent/src/tools.rs: 16
      - crates/agent/src/tools/tests/activity.rs: 3
      - crates/agent/src/tools/tests/editing.rs: 9
      - crates/agent/src/tools/tests/todos.rs: 1
      - crates/agent/src/tools/tests/untrusted_refusals.rs: 1
      - crates/agent/src/turn.rs: 10
      - crates/agent/src/workspace.rs: 19
      - crates/tui/tests/sessions.rs: 4
      - crates/agent/tests/incognito.rs: 1
      - crates/agent/tests/turn.rs: 2
      - crates/agent/tests/workspace.rs: 330
      - crates/core/src/policy.rs: 32
      - crates/core/src/value.rs: 3
      - crates/ui-bridge/tests/workspace.rs: 2
  - symbol: Labelled::relabel
    sites:
      - crates/core/src/policy.rs: 1
      - crates/core/src/slot.rs: 1
      - crates/core/src/value.rs: 6
  - symbol: Declassification::authorise
    sites:
      - crates/core/src/policy.rs: 73
  - symbol: SlotStore::path_of
    sites:
      - crates/core/src/policy.rs: 5
      - crates/core/src/slot.rs: 1
  - symbol: SlotStore::verbatim_of
    sites:
      - crates/core/src/policy.rs: 3
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
      - crates/agent/src/tools.rs: 2
      - crates/core/src/policy.rs: 1
  - symbol: Policy::names_for_display
    sites:
      - crates/agent/src/tools.rs: 5
      - crates/agent/src/turn.rs: 2
      - crates/core/src/policy.rs: 1
  - symbol: Policy::promote_reference_for_read
    sites:
      - crates/agent/src/tools.rs: 2
      - crates/core/src/policy.rs: 7
  - symbol: Policy::destination_from_reference
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 6
  - symbol: VettingSpec::where_it_came_from
    sites:
      - crates/core/src/policy.rs: 5
      - crates/core/src/vetting.rs: 1
  - symbol: Policy::present
    sites:
      - crates/agent/src/aside.rs: 1
      - crates/agent/src/attached.rs: 1
      - crates/agent/src/classifier.rs: 1
      - crates/agent/src/goal.rs: 1
      - crates/agent/src/lsp.rs: 4
      - crates/agent/src/turn.rs: 15
      - crates/core/src/policy.rs: 11
  - symbol: Policy::render_in_place
    sites:
      - crates/agent/src/aside.rs: 1
      - crates/agent/src/commit_message.rs: 2
      - crates/agent/src/file_preview.rs: 1
      - crates/agent/src/images.rs: 1
      - crates/agent/src/manifest.rs: 8
      - crates/agent/src/mcp.rs: 3
      - crates/agent/src/servers.rs: 1
      - crates/agent/src/skills.rs: 2
      - crates/agent/src/tools.rs: 43
      - crates/agent/src/tools/recall.rs: 4
      - crates/agent/src/tools/web_search.rs: 1
      - crates/agent/src/turn.rs: 1
      - crates/agent/src/workspace.rs: 1
      - crates/core/src/policy.rs: 7
  - symbol: Policy::render_pair_in_place
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 1
  - symbol: note_for
    sites:
      - crates/agent/src/tools.rs: 12
  - symbol: Policy::label_model_output
    sites:
      - crates/agent/src/mcp.rs: 1
      - crates/agent/src/tools.rs: 6
      - crates/agent/src/tools/recall.rs: 3
      - crates/core/src/policy.rs: 26
  - symbol: Policy::adopt_model_output
    sites:
      - crates/agent/src/advisor.rs: 1
      - crates/agent/src/aside.rs: 1
      - crates/agent/src/classifier.rs: 1
      - crates/agent/src/compact.rs: 1
      - crates/agent/src/goal.rs: 1
      - crates/agent/src/manifest.rs: 1
      - crates/agent/src/tools.rs: 2
      - crates/agent/src/turn.rs: 2
      - crates/core/src/policy.rs: 4
  - symbol: Policy::declassify_into_workspace
    sites:
      - crates/agent/src/manifest.rs: 1
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 2
  - symbol: Policy::declassify_written_into_workspace
    sites:
      - crates/agent/src/tools.rs: 2
      - crates/core/src/policy.rs: 2
  - symbol: Policy::declassify_checkout_into_workspace
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 1
  - symbol: Policy::promote_a_tool_list
    sites:
      - crates/agent/src/mcp.rs: 1
      - crates/core/src/policy.rs: 4
  - symbol: Policy::promote_a_server_move
    sites:
      - crates/agent/src/mcp.rs: 1
      - crates/agent/src/servers.rs: 1
      - crates/core/src/policy.rs: 5
  - symbol: Policy::adopt_edited_body
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 3
  - symbol: Policy::promote_vetted
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 20
  - symbol: Policy::promote_vetted_picture
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 7
  - symbol: Policy::read_planner_argument
    sites:
      - crates/agent/src/mcp.rs: 1
      - crates/agent/src/tools.rs: 18
      - crates/agent/src/tools/link.rs: 1
      - crates/agent/src/tools/web_search.rs: 1
      - crates/agent/src/workspace.rs: 1
      - crates/core/src/policy.rs: 7
  - symbol: Policy::promote_confined_read
    sites:
      - crates/agent/src/lsp.rs: 1
      - crates/agent/src/tools.rs: 20
      - crates/agent/src/tools/recall.rs: 2
      - crates/agent/src/workspace.rs: 1
      - crates/core/src/policy.rs: 6
  - symbol: Policy::accept_reference
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 4
  - symbol: Policy::decode_transport
    sites:
      - crates/agent/src/images.rs: 1
      - crates/aichat/src/anthropic.rs: 2
      - crates/aichat/src/embeddings.rs: 2
      - crates/aichat/src/generate_content.rs: 2
      - crates/aichat/src/responses.rs: 1
      - crates/aichat/src/lib.rs: 3
      - crates/aichat/src/models.rs: 2
      - crates/aichat/src/ollama.rs: 1
      - crates/aichat/src/ollama_chat.rs: 2
      - crates/bedrock/src/lib.rs: 2
      - crates/core/src/policy.rs: 2
      - crates/mcp/src/http.rs: 1
      - crates/net/src/lib.rs: 1
      - crates/net/tests/egress.rs: 3
      - crates/update/src/lib.rs: 1
  - symbol: Policy::read_a_number
    sites:
      - crates/choice/src/lib.rs: 1
      - crates/core/src/policy.rs: 8
  - symbol: Policy::before_advice
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/core/src/policy.rs: 6
  - symbol: Policy::before_processor
    sites:
      - crates/agent/src/commit_message.rs: 1
      - crates/agent/src/manifest.rs: 1
      - crates/agent/src/processor.rs: 1
      - crates/agent/src/tools.rs: 1
      - crates/agent/tests/workspace.rs: 1
      - crates/choice/src/lib.rs: 2
      - crates/core/src/policy.rs: 28
  - symbol: Policy::before_chooser
    sites:
      - crates/agent/src/tools.rs: 1
      - crates/choice/src/lib.rs: 1
      - crates/core/src/policy.rs: 14
  - symbol: Policy::before_vetting
    sites:
      - crates/agent/src/tools.rs: 4
      - crates/core/src/policy.rs: 10
  - symbol: ProcessorSpec::instruction
    sites:
      - crates/agent/src/permission_mode.rs: 4
      - crates/agent/src/processor.rs: 3
      - crates/agent/src/turn/setup.rs: 1
      - crates/core/src/policy.rs: 1
      - crates/core/src/processor.rs: 1
  - symbol: VettingSpec::expects
    sites:
      - crates/agent/src/tools.rs: 2
      - crates/core/src/policy.rs: 4
      - crates/core/src/vetting.rs: 1
  - symbol: Policy::store_a_choice
    sites:
      - crates/agent/src/turn.rs: 1
      - crates/core/src/policy.rs: 4
      - crates/mcp/src/protocol.rs: 1
  - symbol: Policy::carry_a_choice
    sites:
      - crates/agent/src/mcp.rs: 1
      - crates/core/src/policy.rs: 10
      - crates/mcp/src/protocol.rs: 1
  - symbol: Policy::choice_for_display
    sites:
      - crates/agent/src/mcp.rs: 1
      - crates/core/src/policy.rs: 6
  - symbol: Policy::number_a_list
    sites:
      - crates/choice/src/lib.rs: 2
      - crates/core/src/policy.rs: 1
  - symbol: Policy::resolve_proposal
    sites:
      - crates/choice/src/lib.rs: 2
      - crates/core/src/policy.rs: 1
  - symbol: Policy::proposal_for_display
    sites:
      - crates/agent/src/server_calls.rs: 2
      - crates/core/src/policy.rs: 1
  - symbol: Policy::proposes_nothing
    sites:
      - crates/agent/src/server_calls.rs: 1
      - crates/core/src/policy.rs: 1
  - symbol: Policy::keep_applied
    sites:
      - crates/agent/src/server_calls.rs: 1
      - crates/core/src/policy.rs: 2
  - symbol: Policy::carry_applied_call
    sites:
      - crates/agent/src/mcp.rs: 1
      - crates/core/src/policy.rs: 5
  - symbol: Carried::into_wire
    sites:
      - crates/core/src/policy.rs: 4
      - crates/mcp/src/protocol.rs: 1
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

Untrusted input degrades integrity; private input raises confidentiality. A label degrades into
another when the other is no more trusted and no less private than it. `(T,priv)` and `(U,pub)` are
**incomparable**: neither degrades into the other. So this is a lattice rather than a pair of
booleans, and no total order may be imposed on it: `Label` implements neither `PartialOrd` nor
`Ord`. Integrity meets pessimistically and confidentiality joins pessimistically.

`verified-by: bravebot_core::label::middle_elements_are_incomparable`
`verified-by: bravebot_core::label::degradation_has_one_source_and_one_sink`
`verified-by: bravebot_core::label::degradation_is_reflexive`
`verified-by: bravebot_core::label::degradation_is_antisymmetric`
`verified-by: bravebot_core::label::degradation_is_transitive`
`verified-by: bravebot_core::label::integrity_meet_is_pessimistic`
`verified-by: bravebot_core::label::confidentiality_join_is_pessimistic`
`verified-by: bravebot_core::label::display_matches_the_canonical_notation`
`verified-by: bravebot_core::spec::degradation_is_a_partial_order`
`verified-by: bravebot_core::spec::the_middle_labels_are_incomparable`
`verified-by: bravebot_core::spec::taint_is_the_least_upper_bound`
`verified-by: bravebot_core::spec::every_two_labels_have_a_greatest_lower_bound`
`verified-by: by-construction (Label implements neither PartialOrd nor Ord, so two labels cannot be ranked with <, <=, > or >=; the compile_fail doctest on Label in crates/core/src/label.rs pins the < case)`

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
entry out of a listing whether it stands for a file or for a directory the walk stopped at, said once
in the listing's result for all its entries ([LIST-2](tools/list-files.md#LIST-2)). The
planner acts on content it cannot read by naming that reference, and the policy layer resolves it
when the write or the call actually happens. Where the content has to be changed rather than moved, it goes
to a processor, described in [processors.md](processors.md). A reference to a number a processor chose
states its limit and its label and neither count, since the length of the digits would say more about
the number than the planner is shown ([PROC-16](processors.md#PROC-16)).

The origin is routing, and it is trusted where it is presented. The counts are numbers and the
label is an enum, but the origin is a string that reaches the planner's context, the trace and the
transcript verbatim, so it is one the driver chose: a path, a slot, or an argument a person
approved. It is never taken from the content it describes, nor from anything that answered the
request for that content. An origin a server wrote is untrusted content in the planner's context
wearing the driver's attribution, which is the thing this clause exists to stop. The origin of what
a `run` printed names the directory in the words the planner named it with, and not the directory a
symbolic link in the tree led to, because whoever wrote a tree nobody vouched for chose that name
([CMDLINE-12](tools/command-line.md#CMDLINE-12)).

`verified-by: bravebot_core::policy::untrusted_content_is_presented_as_a_reference`
`verified-by: bravebot_core::policy::trusted_content_is_presented_visibly`
`verified-by: bravebot_core::reference::a_quarantined_presentation_shows_no_content`
`verified-by: bravebot_core::reference::a_visible_presentation_shows_the_content`
`verified-by: bravebot_core::reference::a_description_names_the_shape_and_not_the_content`
`verified-by: bravebot_core::reference::a_description_says_how_to_refer_to_the_content`
`verified-by: bravebot_core::reference::a_choice_reference_states_its_limit_and_no_count`
`verified-by: bravebot_agent::turn::a_fetched_page_names_the_host_that_was_asked_for_and_not_where_a_redirect_went`
`verified-by: bravebot_agent::tools::tests::editing::an_edit_by_reference_that_cannot_be_read_does_not_name_the_file`
`verified-by: bravebot_agent::turn::a_run_result_names_the_directory_as_typed_and_not_where_a_link_led`
`verified-by: bravebot_agent::turn::a_finished_jobs_account_names_the_directory_as_typed_and_not_where_a_link_led`
`verified-by: bravebot_agent::turn::a_checkout_with_changes_names_a_redirection_through_a_link_by_the_typed_spelling`
`verified-by: bravebot_agent::turn::a_kept_checkouts_redirection_through_a_link_is_named_by_the_typed_spelling`

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
the read: the planner's own arguments, a transport envelope a decoder has to see, a model's reply on
its way out of that envelope, a processor's answer to the one reader that takes a number out of
it ([PROC-16](processors.md#PROC-16)), and the question the planner puts to its advisor. A driver
holding the bytes without one is the shape this clause exists to stop.

Four gates release a string the planner wrote, whatever its label, and decide nothing from the
label: `Policy::before_advice` (the question put to the advisor, recorded with the label the
question carried and none of its words), `Policy::before_processor` and `Policy::before_chooser`
(a processor's instruction, whose label is folded into the answer's), and `Policy::before_vetting`
(what the planner says a slot holds). The instruction and the expectation are two more strings
that may be untrusted and that the kernel holds outside a labelled value, once a gate has released
them into a spec, so the reads of each are pinned too: `ProcessorSpec::instruction` and
`VettingSpec::expects`. Each gate's call sites are pinned file by file in `guards`. The checker
matches a method by its bare name, so the count for
`ProcessorSpec::instruction` also holds the uses of `PermissionMode::instruction` in
`crates/agent/src/permission_mode.rs` and `crates/agent/src/turn/setup.rs`, and the test helper
named `instruction` in `crates/core/src/policy.rs`. The reads of a processor's instruction are the
two in `crates/agent/src/processor.rs`, and `crates/core/src/processor.rs` counts its definition.

**A slot's address is untrusted content too, and is not carried in a labelled value.** The file a
slot names, the file its bytes are a copy of, the file an answer produced from it belongs to, the
sentence saying where its bytes came from, and the file it is still waiting on are bare strings,
and where the slot came from a quarantined listing every one of them is a filename the planner was
never shown. So each accessor that hands one back takes a witness minted in the same module the
declassification witness is minted in, and the `guards` list above pins their uses file by file,
the way it pins a declassification. A decision relocated into the kernel and taken from a slot's
address moves a pinned count where it releases the address again, and one relocated into any module
of `bravebot-core` outside the gates does not compile at all, bar the single release named next.
The counts pin releases, not the decisions taken from bytes a counted release already produced, so
a branch added inside a function that holds a counted use moves nothing and is found by reading the
diff ([reviewing-for-the-rule.md](../development/reviewing-for-the-rule.md)).

Formatting is a release as well, and no witness stands in front of it. A derived `Debug` prints
every field, so `SlotStore`, `Entry`, `Deferred`, `Home`, `VettingSpec` and `Origin` write theirs
by hand. Each prints slot ids, labels and kinds and never a path, a verbatim path, the path of a
`Home::Only` or an origin, and `VettingSpec` leaves out what it names and what the planner expected
of it. `SlotWriter` and `Conversation` derive `Debug` and inherit that, since each holds a
`SlotStore`. `check_slot_address_debug` in the security-audit skill fails a derived `Debug` on any
of the six, found by the file that declares each. It does not read what a hand-written one prints,
so a field added to one of those bodies is found by reading the diff, and so is a new type that
holds an address.

Three gates let a slot's address leave the kernel, each recorded and each pinned file by file in
`guards`, so a further reader of a slot's address anywhere downstream is a line in a diff rather
than a branch nobody is shown:

- **The sentence saying where its bytes came from**, through `release_where_a_slot_came_from`,
  because the person being asked whether to let a slot's bytes through has to be told which file
  they are.
- **The file a reference names, for a line a person reads**, through `names_for_display`. It
  returns every slot's filename as a plain string to be worded into a preview, a note or a task
  line, and records one `Declassified` event and one `display` allow for the call. It is a display
  release and nothing it returns may become a path an effect uses. A file `/export` writes is read
  by later turns, so it holds the call as the planner had it and not these names
  ([SESSION-17](sessions.md#SESSION-17)).
- **The file a reference names, as a read's path or a destination**, through
  `promote_reference_for_read` and `destination_from_reference`, which [routing.md](routing.md)
  settles and which share `path_of_reference`.

Apart from those, outside the kernel a slot answers two questions about its address, whether asking
for its bytes will open a file and whether the slot is no file's contents and no answer about one,
which name nothing.

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
`verified-by: by-construction (Deref, PartialEq and Display are not implemented for Labelled, and its only witness-free accessor returns Err on anything but (T,pub); the compile_fail doctests on Labelled in crates/core/src/value.rs pin equality, Hash, Display, Deref, AsRef and a conversion into the content, and check_labelled_impls in the security-audit skill reads a derive, a nested-generic bound and a wrapped impl header for the rest, every function of the type's own impl blocks against the seven the type has (new, trusted, label, declassify, into_trusted, relabel and shape) by name, visibility and signature, every impl whose subject is Labelled, a Box of it, a type alias of it or a wrapper declared beside it, an impl in that file whose subject is a generic parameter or a projection, and every impl for another type that takes a Labelled as an argument of its trait, such as From<Labelled<String>> for String, with comments, strings, raw strings and character literals blanked before any brace is counted; it does not see a wrapper declared in another file, which has no access to the private fields)`
`verified-by: by-construction (Declassification::authorise is pub(in crate::policy), so no other module of bravebot-core and no crate downstream of it can mint a witness)`
`verified-by: by-construction (PathAuthority::mint is pub(in crate::policy) too, so SlotStore::path_of, SlotStore::verbatim_of, SlotStore::home_of, SlotStore::origin_of, SlotStore::deferred, Policy::where_a_slot_came_from and VettingSpec::where_it_came_from cannot be called from another module of bravebot-core or from any crate downstream of it, and Deferred is pub(crate); Policy::release_where_a_slot_came_from, Policy::names_for_display, Policy::promote_reference_for_read and Policy::destination_from_reference are the four that can, and each records its release. Those eight and these four are the whole of the surface returning a slot's address and every use of each is pinned above; formatting a type that holds one is the other way out, which SlotStore, Entry, Deferred, Home, VettingSpec and Origin close by writing Debug by hand to print ids, labels and kinds, and check_slot_address_debug in the security-audit skill fails a derive of Debug on any of them, so a hand-written Debug that printed an address would be found by reading the diff and a type that began to hold one is found the same way)`
`verified-by: bravebot_core::slot::formatting_a_store_does_not_print_the_name_of_a_file_waiting_to_be_read`
`verified-by: bravebot_core::slot::formatting_a_store_does_not_print_the_name_a_read_slot_keeps`
`verified-by: bravebot_core::slot::formatting_a_home_does_not_print_the_one_file_it_is_restricted_to`
`verified-by: bravebot_core::slot::formatting_a_store_lists_a_choice_by_name_and_label_only`
`verified-by: bravebot_core::slot::formatting_a_store_lists_its_slots_in_the_order_they_were_minted`
`verified-by: bravebot_core::slot::formatting_a_writer_does_not_print_the_names_in_the_store_it_writes_into`
`verified-by: bravebot_core::vetting::formatting_an_origin_does_not_print_the_recorded_name`
`verified-by: bravebot_core::policy::formatting_a_store_with_a_reserved_listing_does_not_print_the_filename`
`verified-by: bravebot_core::policy::formatting_a_store_after_the_listed_file_is_read_does_not_print_the_filename`
`verified-by: bravebot_core::policy::formatting_a_vetting_spec_does_not_print_the_filename_it_was_built_from`
`verified-by: bravebot_core::policy::formatting_a_vetting_spec_does_not_print_what_it_names_or_what_was_expected_of_it`
`verified-by: bravebot_agent::conversation::formatting_a_conversation_does_not_print_the_names_its_quarantine_holds`
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

The reader that takes the issues and pull requests a session names out of a `run` command and a
`fetch_url` or `download_url` address ([PANEL-16](info-panel.md#PANEL-16)) is one such reader. It
runs after the gate that reads the argument and only where that gate has handed the bytes over, so a
context that has met something untrusted collects nothing from the planner's arguments, and it adds
no gate of its own.

An argument naming something the driver minted is held the same way, since the planner is the one
who picks which of those names a call is about. [routing.md](routing.md) is where that case is
settled.

One exception is named, and it is narrow: within a manifest plan a person approved, the calls of an
`apply` step whose tools the person marked plan-covered are selected by a processor that read
untrusted content, among calls of the plan's tools, arguments and lists, each acceptable on its own,
within the caps the plan states ([manifest.md](manifest.md#MANIFEST-17)).

`verified-by: bravebot_core::policy::requesting_untrusted_content_is_refused`
`verified-by: bravebot_core::policy::an_argument_cannot_be_read_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_core::policy::a_reference_cannot_be_named_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_core::policy::a_proposal_cannot_be_promoted_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_core::policy::a_private_argument_is_refused_rather_than_read`
`verified-by: bravebot_agent::tools::tests::editing::an_edit_from_a_trusted_context_replaces_the_passage`
`verified-by: bravebot_agent::tools::tests::editing::an_edit_is_refused_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_agent::tools::tests::arguments::a_command_line_and_a_directory_are_read_from_a_trusted_context`
`verified-by: bravebot_agent::tools::tests::arguments::a_command_line_is_refused_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_agent::tools::tests::arguments::a_url_is_read_from_a_trusted_context`
`verified-by: bravebot_agent::tools::tests::arguments::a_url_is_refused_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_agent::tools::tests::arguments::a_job_name_is_read_from_a_trusted_context`
`verified-by: bravebot_agent::tools::tests::arguments::a_job_name_is_refused_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_agent::tools::tests::arguments::a_skill_name_is_refused_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_agent::tools::tests::references::a_context_that_has_met_something_untrusted_collects_nothing_from_the_planners_arguments`
`verified-by: bravebot_agent::tools::tests::references::a_command_the_planner_wrote_names_the_items_it_works_on_even_where_the_call_then_fails`
`verified-by: bravebot_agent::manifest_server_calls::a_step_whose_tools_are_all_marked_runs_on_the_plans_approval`

<a id="LABEL-6"></a>
### LABEL-6: minting a witness is not permission to inspect

A witness records that bytes moved somewhere they were already allowed to go: a filesystem write,
an HTTP body, a human's screen, or the standard input of a program a person endorsed
([run.md](tools/run.md#RUN-3)). A connector's authority is such a program, named by the person in
their own connector definition, and what its standard input is given is the HTTP body it becomes.
Each of those destinations has a gate of its own, one for putting content in front of the planner,
one for reshaping it for display, and one for reading trusted content. A declassification anywhere
else is almost certainly a violation.

The copy of a picture a person is asked to open ([VET-4](tools/vet-content.md#VET-4)) is one more
filesystem write on this list, at a gate of its own for that one file.

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

`Policy::promote_confined_read` and `Policy::accept_reference` take no closure and are counted for
another reason. Each is handed an argument the planner wrote, wrapped `(U,pub)`, and returns it as
a `(T,pub)` path or as a slot id. What they refuse is a private value and a context that has met
something untrusted. They do not refuse a public value because of where its bytes came from, so a
caller that passed them a fetched page or a redirect target would get it back as trusted text or as
a slot name, and the label would not show it. A caller's argument is the only thing that makes
the result the planner's own, so each is counted per file in `guards` like
`Policy::read_planner_argument`, which makes the same two refusals, and a spec that only names
either one does not account for a new caller of it.

`verified-by: bravebot_agent::tools::tests::activity::a_call_line_names_its_reference_inside_the_kernel`
`verified-by: bravebot_agent::tools::tests::todos::a_task_list_is_named_inside_the_reshape_that_builds_it`
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
`verified-by: bravebot_agent::turn::a_fetched_pages_lines_are_counted_inside_the_kernel`
`verified-by: bravebot_agent::turn::an_asides_cut_is_made_inside_the_kernel`
`verified-by: bravebot_agent::manifest::the_answer_steps_line_count_is_taken_inside_the_kernel`
`verified-by: bravebot_agent::manifest::a_connector_fetch_fills_one_unread_slot_and_the_act_sends_the_released_text`

## Which direction a label may move

<a id="LABEL-7"></a>
### LABEL-7: labels only ever degrade

Integrity may go trusted to untrusted and never the reverse. Relabelling yields nothing rather than
upgrading, and refuses incomparable labels outright. Never build a labelled value by hand to give it
a better label than its inputs had: that is laundering, whichever crate it happens in. If a value
derived from untrusted input needs to be trusted for something to work, the
design is wrong, not the label.

A name added to a question the trust map is asked can only lower a label. `read_git` asks about a
repository and the paths an answer showed under every name each has ([GIT-3](tools/read-git.md#GIT-3),
[TRUST-4](trust-map.md#TRUST-4)), and `Policy::observe_repository_under_names` takes the meet of the
answers, so the label is never better than the one for the name the planner typed.

`verified-by: bravebot_core::value::relabel_may_degrade`
`verified-by: bravebot_core::value::relabel_may_not_upgrade`
`verified-by: bravebot_core::value::relabel_refuses_incomparable_labels`
`verified-by: bravebot_core::value::relabel_may_not_make_private_content_public`
`verified-by: bravebot_core::label::degradation_never_raises_integrity_or_lowers_confidentiality`
`verified-by: bravebot_core::label::trusted_input_cannot_launder_untrusted`
`verified-by: bravebot_core::label::top_of_taint_degrades_from_everything`
`verified-by: bravebot_core::policy::a_repository_answer_is_asked_under_every_name_of_the_git_directory_and_each_path`
`verified-by: bravebot_agent::workspace::a_commit_shown_through_a_link_is_untrusted_where_the_map_distrusts_the_file_it_lands_on`

<a id="LABEL-8"></a>
### LABEL-8: a first label comes from provenance, and is not an upgrade

Model output is a function of the model's context and nothing else, so when the context holds only
trusted input, what it produced is labelled accordingly. The same road labels every other carrier in
the table below. Each is the **first** label such a value ever receives, assigned from provenance
the policy layer tracked.

If you find yourself relabelling a value that already has a label, stop: that is LABEL-7.

The first label of an answer of `read_git` comes from the trust map's answers for `.git` and the
paths shown, under the names the file system gives each of them ([GIT-3](tools/read-git.md#GIT-3)).
The path names an answer shows are read from the tree objects of a repository the map vouched for,
and they choose which files are looked up. No byte of a blob or of a file's contents decides which
names are asked or what the label is. That is sound because the lookups only add names to the
questions the map is asked, and an added name can only lower the label, so a path name that adds a
lookup cannot make an answer trusted that the name typed would not.

The sentence holds for every model this program requests, and the program keeps it true by not
requesting one it does not hold for. A model that searches the web before it replies, which OpenAI's
Chat Completions search models do on every request, adds the pages its provider's search returns to
its context, so its reply is a function of more than the context built here and the context's label
would be given to bytes that came from the web. The models on the list `RETRIEVING_MODELS` in
`crates/config/src/provider.rs` are therefore not offered in a roster and not requested over chat
completions, as `model`, `summaryModel`, `advisorModel`, `planModel`, `processorModel`, a
definition's `model` or a recorded pick, and the refusal is the machine-level layer's, asked in
`Config::model_refused` without a file to name ([BACKEND-19](backends.md#BACKEND-19),
[BACKEND-48](backends.md#BACKEND-48)). Over the Responses protocol search is a tool a request has to ask
for, and a request this program builds asks for none, so the same id is offered there. The ids on the
list come from OpenAI's web search guide, read on 2026-11-13, and the claim that they search on every
request has not been exercised against the service. A model that retrieves and is not on the list is
not detected: see [Known costs](#known-costs).

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
| the file names a language server reported, and the file the question was asked at for the operations that start from one | private, and trusted only where every path it names is and, for `goToDefinition`, `findReferences`, `goToImplementation`, `incomingCalls`, `outgoingCalls` and `documentSymbol`, the file the planner asked at is too, so one nobody vouched for quarantines the answer even where it names only vouched files or none; the same road as a listing, and the capability's `(U,priv)` is where it starts | `verified-by: bravebot_agent::lsp::locations_are_labelled_by_the_files_they_name` `verified-by: bravebot_agent::lsp::a_location_answer_asked_in_a_distrusted_file_is_untrusted` `verified-by: bravebot_agent::lsp::a_position_asked_in_an_unvouched_file_is_not_in_the_planners_context` |
| the severities and line numbers a language server reported for a file it was asked about | trusted and private, from the capability's own label and not from the queried file's entry, because a severity is a fixed enumeration and a line is an integer and the sentence carries no word the server wrote; the numbers are still computed from bytes nobody may have vouched for, which is the fifth entry of [Known costs](#known-costs) | `verified-by: bravebot_agent::lsp::diagnostics_for_a_file_nobody_vouched_for_report_lines_and_not_messages` |
| the lines of a file a person clicked a path to see | untrusted and private whatever the trust map says of the path, from the label a write's review reads the file it replaces under, because the lines go to a screen as a quarantined block and a click has not asked for the file to be read by anything that acts on it ([MOUSE-42](mouse.md#MOUSE-42)) | `verified-by: bravebot_agent::file_preview::a_path_the_map_vouches_for_is_still_shown_as_untrusted` |
| what a connector's authority says about a call besides its payload and detail: the outcome, the standing, reason codes, counts withheld and a link | none, because none of it reaches a model: each field has a fixed form, an outcome or standing from a closed list, a count, a reason in lower-case kebab and one `https://` URL, and a value out of form is replaced or dropped rather than shown. The payload and the detail are the capability's, the first row | `verified-by: bravebot_connector::lib::a_reason_code_is_shown_only_in_the_authoritys_own_form` `verified-by: bravebot_agent::manifest::an_accepted_acts_link_is_shown_only_when_it_is_well_formed` `verified-by: bravebot_agent::manifest::a_refused_connector_call_fails_its_step_without_quoting_the_service` |
| a file the user named in a prompt or dropped on the window | trusted, because naming a file is vouching for it | `verified-by: bravebot_core::policy::a_file_the_user_named_is_read_as_trusted_though_nothing_else_is` |
| what a program printed | untrusted and private, since what it did is unknown | `verified-by: bravebot_core::policy::an_opaque_program_always_yields_untrusted_private_output` |
| what a program that can only transform its input printed, fed content | private, and trusted only where what was fed in is, because a program prints what it was given and a run is not a release | `verified-by: bravebot_core::policy::a_filter_fed_an_untrusted_label_prints_untrusted_output` `verified-by: bravebot_core::policy::a_filter_fed_a_trusted_label_prints_trusted_private_output` |
| what such a program printed with no input at all | trusted and private, because it is a function of nothing an attacker influenced, and private because a run is not a release | `verified-by: bravebot_core::policy::a_filter_with_no_input_prints_trusted_private_output` |
| what a line whose every step is accounted for printed | trusted and private, because the steps were vouched for and the output is a function of what went in | `verified-by: bravebot_core::policy::a_line_that_only_reads_vouched_for_paths_comes_back_trusted` |
| a line the user ran themselves | trusted and private, because nobody steers a keystroke | `verified-by: bravebot_core::policy::what_a_command_the_user_typed_printed_is_trusted_and_private` |
| input piped into the process | untrusted and private, because a pipe has no path anyone could vouch for. The body of a delivery to `bravebot trigger serve` is this row: it is written to the standard input of the run it starts and takes no label of its own ([TRIGGER-7](triggers.md#TRIGGER-7)) | `verified-by: bravebot_core::policy::piped_input_is_labelled_untrusted_and_private` `verified-by: bravebot_cli::running::a_signed_delivery_starts_the_prompt_and_the_planner_never_sees_the_body` |
| the user's own configuration, and a skill kept beside it | trusted and public, because putting a file there is the grant. A checkout's settings file is not the person's own: its text is not carried into a sentence the planner gets, which names only the list key, a closed reason and the file ([SANDBOX-25](sandboxing.md#SANDBOX-25)) | `verified-by: bravebot_core::policy::configuration_the_user_placed_is_trusted_from_where_it_came_from` |
| a fact kept by `remember` or the person | trusted, with the confidentiality its `label` field states, private when missing, because a kept fact is in the user's own memory and is written only by the person or by a save they were shown; a private one makes what the planner writes private once it is read | `verified-by: bravebot_core::policy::a_kept_fact_is_trusted_with_the_confidentiality_it_states` `verified-by: bravebot_core::policy::labelling_a_kept_fact_is_recorded_in_the_audit_trail` `verified-by: bravebot_core::policy::reading_a_private_fact_makes_the_context_private` `verified-by: bravebot_core::policy::reading_a_public_fact_leaves_the_context_as_it_was` |
| what the planner wrote | the integrity of the context it was written in, and private once private content was let out of quarantine for the planner: a slot `vet_content` let through, output `read_output` read to it, a delegate's report it was given, or a context it resumed that already held one. A workspace file it was shown does not count, because that is the project's own data and counting it would make every call after the first read ask | `verified-by: bravebot_core::policy::model_output_from_a_clean_context_is_trusted` `verified-by: bravebot_core::policy::a_vetted_private_slot_makes_what_the_planner_writes_private` `verified-by: bravebot_core::policy::a_vetted_public_slot_leaves_what_the_planner_writes_public` `verified-by: bravebot_core::policy::a_vetted_private_picture_makes_what_the_planner_writes_private` `verified-by: bravebot_core::policy::output_read_aloud_makes_what_the_planner_writes_private` `verified-by: bravebot_core::policy::a_private_report_from_a_delegate_is_held` `verified-by: bravebot_core::policy::a_resumed_context_keeps_what_it_held_and_cannot_be_made_public` |
| a reply taken out of a transport's envelope | the context's, never the network's | `verified-by: bravebot_core::policy::adopting_model_output_takes_the_context_s_label_not_the_transport_s` |
| the auto mode's classifier's reply | the context's, as any reply taken out of a transport's envelope gets. The driver reads one word of it, and only where that label is trusted, whether or not it is private; a reply labelled untrusted is read as a refusal to allow ([AUTO-6](auto-mode.md#AUTO-6)) | `verified-by: bravebot_agent::turn::an_untrusted_context_is_not_put_to_the_classifier` `verified-by: bravebot_agent::turn::a_private_context_is_put_to_the_classifier_and_its_word_answers` `verified-by: bravebot_agent::classifier::a_context_that_has_met_something_untrusted_is_not_consulted` `verified-by: bravebot_agent::classifier::a_trusted_context_is_consulted_whether_or_not_it_holds_private_content` `verified-by: bravebot_agent::classifier::only_the_allow_word_alone_allows` |
| an answer a person typed to a question | trusted, because a person wrote it, and as private as the questions it answers, since the reply repeats them | `verified-by: bravebot_core::policy::a_typed_answer_is_trusted_because_a_person_wrote_it` `verified-by: bravebot_core::policy::private_questions_are_asked_and_the_reply_stays_private` |
| the body a person saved from their editor over a proposed write | trusted and public, from `Policy::adopt_edited_body`, because a person wrote it and it goes into their workspace as the proposal it replaces was already going to; refused where that proposal was untrusted or still private | `verified-by: bravebot_core::policy::an_edited_body_is_trusted_and_public` `verified-by: bravebot_core::policy::a_proposal_that_was_not_trusted_and_public_is_not_made_so_by_editing_it` |
| what a processor produced | taint over the inputs it was given | `verified-by: bravebot_core::policy::an_output_is_labelled_by_taint_over_the_inputs` |
| a number a processor chose from a list | untrusted and private, by taint over what the processor was given, assigned by the policy layer as the reader returns its byte and no better than the answer's. The number is all that is kept of the answer, and which number is read is decided by the crate that reads it, which is the ninth entry of [Known costs](#known-costs). It reaches a call at that label, on the person's answer to the call ([ROUTE-10](routing.md#ROUTE-10), [MCP-14](mcp.md#MCP-14)) | `verified-by: bravebot_core::policy::a_number_read_from_an_answer_keeps_the_label_of_the_answer` `verified-by: bravebot_core::policy::a_stored_choice_is_a_reference_whatever_its_label_and_the_trail_holds_no_digit` `verified-by: bravebot_core::policy::carry_a_choice_consumes_the_endorsement_spends_the_choice_and_refuses_before_spending` |
| one slot's bytes a person read on their screen and vouched for | trusted and private, from `Policy::promote_vetted`, because a person read them and said so, and the slot itself keeps what it had | `verified-by: bravebot_core::policy::output_a_person_vouched_for_comes_back_trusted` `verified-by: bravebot_core::policy::vetted_content_a_person_vouched_for_comes_back_trusted` |
| a server's tool list a person vouched for, or the mode that answers for them | trusted and public, from `Policy::promote_a_tool_list`, because a person read the whole list as it is drawn and said yes, every session, since nothing the driver runs compares a server's bytes with an earlier list | `verified-by: bravebot_core::policy::a_tool_list_reaches_the_planner_only_through_an_endorsement` `verified-by: bravebot_agent::mcp::a_list_is_asked_about_every_session_and_a_yes_records_nothing` |
| where a remote server's reply pointed, once a person says the server moved there | trusted and public, from `Policy::promote_a_server_move`, because a person read the url as it is drawn and the declaration it is written into is theirs | `verified-by: bravebot_core::policy::a_server_move_is_promoted_only_through_an_endorsement` |
| a part of a server's tool result: its text, and each picture it carries | untrusted and private, the capability's own label, the same for a picture as for the text and the same whatever the parts hold. Which parts are taken, and under which of four media types, is decided by the transport crate from the `type`, the `mimeType` and the size of the data the server wrote, and is the eighth entry of [Known costs](#known-costs) | `verified-by: bravebot_mcp::stdio::a_tool_result_picture_is_labelled_untrusted_and_private` `verified-by: bravebot_mcp::http::a_tool_result_picture_is_labelled_untrusted_and_private` |
| the text of a prompt a person ran from a server | untrusted and private, the capability's own label, whoever chose the prompt and typed its arguments, because choosing to run a prompt does not vouch for what it says ([MCP-11](mcp.md#MCP-11)) | `verified-by: bravebot_mcp::stdio::a_prompt_text_is_labelled_untrusted_and_private` `verified-by: bravebot_mcp::http::a_prompt_text_is_labelled_untrusted_and_private` `verified-by: bravebot_agent::mcp::a_prompt_a_person_ran_reaches_the_server_with_their_arguments_and_the_planner_gets_a_reference` |
| what a session fetched with a tool of what a person held out in the browser, a selection, a link or a tab | untrusted and private, as any part of a server's tool result is. Holding it out gives the bytes no label: they are the page's whichever hand selected them, a selection includes the text the page hides, and the extension that keeps them is not the driver ([BROWSER-39](browser.md#BROWSER-39)) | `verified-by: bravebot_agent::browser_group_servers::what_a_person_held_out_in_the_browser_is_a_reference_and_the_planner_never_holds_it` `verified-by: bravebot_agent::browser_group_servers::an_item_held_out_reaches_the_planner_only_when_a_person_has_read_it_and_every_call_after_is_put_to_the_person` |
| a picture or a PDF `vet_content` let through | trusted and private, from `Policy::promote_vetted_picture`, on the endorsement that let it through, as promoted text is, and the slot keeps what it had; it goes to the planner in a message of its own rather than joining the user's, so it takes a label where a paste takes none | `verified-by: bravebot_core::policy::a_picture_is_promoted_once_by_any_endorsement_and_attached_as_itself` |
| a picture pasted at the keyboard | none, because it joins the user's own message, which carries none either, so it is recorded instead | `verified-by: bravebot_core::policy::a_pasted_image_is_recorded_in_the_audit_trail` |
| pasted text | none, for the same reason, but with the characters a terminal draws as nothing taken out first and counted on the screen ([PASTE-10](pasting.md#PASTE-10)): a character map that reads no meaning from the text and decides no effect | `verified-by: bravebot_tui::state::a_paste_loses_the_characters_a_terminal_draws_as_nothing_and_says_how_many` |
| a prompt typed while a turn is running | none, for the same reason, and recorded the same way | `verified-by: bravebot_core::policy::an_interjection_is_recorded_in_the_audit_trail` |
| a line sent while a turn is running on an input channel the person vouched for with `--trust-input-lines` | none, for the reason a typed line gets none: the flag the person typed is the vouching, and the trail records it as that and not as typed ([CLI-36](cli.md#CLI-36)) | `verified-by: bravebot_core::policy::a_vouched_line_is_recorded_as_that_and_does_not_lower_the_context` `verified-by: bravebot_agent::turn::a_line_from_a_vouched_channel_is_recorded_as_vouched_for_and_not_as_typed` |
| the words of an `answer` line to a question of one prompt, on an input channel the person vouched for with `--trust-input-lines` | trusted and private, as an answer a person typed is, because the flag the person typed is the vouching. The trail records it as words from a vouched channel and not as typed, and a position among the choices the planner offered is recorded as neither. A channel nobody vouched for has its words refused ([CLI-36](cli.md#CLI-36)) | `verified-by: bravebot_agent::tools::tests::questions::words_from_a_vouched_channel_are_recorded_as_that_and_a_position_is_not` `verified-by: bravebot_cli::running::words_are_refused_on_a_channel_nobody_vouched_for` |
| a line on an input channel nobody vouched for | untrusted and private, as input piped into the process is, because the launching program is not necessarily the person ([CLI-36](cli.md#CLI-36)) | `verified-by: bravebot_cli::running::lines_nobody_vouched_for_never_reach_the_planner_and_vouched_ones_do` |
| a message another background session sent | the label it was sent with, read back from the word beside the text, so a message never arrives better than it left; untrusted and private where there is no word or the word names no label ([MSG-9](session-messages.md#MSG-9)) | `verified-by: bravebot_core::policy::a_message_from_another_session_keeps_the_label_it_was_sent_with` `verified-by: bravebot_core::policy::a_message_whose_label_cannot_be_read_is_untrusted_and_private` `verified-by: bravebot_agent::turn::a_message_that_is_not_labelled_trusted_reaches_the_planner_as_a_reference` |
| a system prompt named on the command line | none, for the reason the user's own message gets none: the person who typed the flag is the person the planner works for, and the words are plain text that grants no permission | `verified-by: bravebot_agent::turn::words_that_allow_writes_allow_none` |

Where a path is known, integrity is the trust map's answer about that path rather than the
capability's, which is what the three rows for reads say and why the first row is the label a read
starts from. Which paths a person vouched for is in [trust-map.md](trust-map.md).

Three carriers a reader may go looking for are absent, none of which takes a first label. A
delegate's reply is model output, labelled in the delegate's own run by the row for what the planner
wrote, and a delegate starts out holding what the context that wrote its task held, so what it writes is
private from its first round ([DELEGATE-8](delegation.md#DELEGATE-8)). Content restored
from a resumed session keeps the labels it was given when it first arrived, and what resuming does
to the integrity of the context is LABEL-9. What the context held is kept the same way: a resumed,
forked or compacted session holds whatever the original held, and a session file that does not say
reads as holding private content. The trail says which of these made the context private, so a
resumed run is not recorded as letting anything out of quarantine. The user's own message is not labelled at all, which is why the
two carriers that join it take no label either.

The reply that ends a turn and an answer given between calls enter the conversation by the same
step. In a context that has met something untrusted the reply is labelled untrusted, so the planner
is told that it answered and given a reference to the reply, and the reply is in no later request.

`verified-by: bravebot_core::policy::adopting_model_output_from_a_fallen_context_stays_untrusted`
`verified-by: bravebot_agent::turn::the_final_reply_of_a_turn_is_held_under_a_reference_once_something_untrusted_was_met`
`verified-by: bravebot_core::policy::only_a_value_a_transport_labelled_can_be_adopted_as_model_output`
`verified-by: bravebot_core::policy::a_delegate_starts_out_holding_what_its_parent_held`
`verified-by: bravebot_core::policy::the_trail_says_how_the_context_came_to_hold_private_content`
`verified-by: bravebot_agent::conversation::a_private_conversation_does_not_come_back_public`
`verified-by: bravebot_agent::conversation::a_public_conversation_comes_back_public`
`verified-by: bravebot_agent::conversation::an_unreadable_holds_word_is_read_as_private`
`verified-by: bravebot_agent::conversation::a_session_file_written_before_holds_was_recorded_reads_as_private`
`verified-by: bravebot_agent::conversation::what_the_planner_was_shown_only_ever_rises`
`verified-by: bravebot_agent::conversation::compaction_does_not_make_a_private_conversation_public`
`verified-by: bravebot_ui_bridge::fork::a_conversation_whose_planner_was_shown_private_content_forks_private`
`verified-by: bravebot_config::lib::a_model_that_searches_the_web_is_refused_over_chat_completions_only`
`verified-by: bravebot_aichat::models::a_listing_is_not_offered_a_model_that_searches_the_web_over_chat_completions`

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
content states one: a preview of quarantined bytes, the bytes a person is asked to read in a
prompt, the sentence a check wrote about them, and a remark accompanying a write. The boundary
neither reads the content nor decides the label, so what arrives is what the gate released, and a
carrier added later states one too.

The copy of a picture a person is asked to open ([VET-4](tools/vet-content.md#VET-4)) is the one
exception: it is opened in the person's own viewer, which reads no label, and that is stated there
as a cost.

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

`verified-by: bravebot_ui_bridge::fetch::the_session_view_orders_prompts_approvals_and_labelled_results`
`verified-by: bravebot_ui_bridge::wire::released_content_crosses_the_transport_with_the_label_it_was_released_under`
`verified-by: bravebot_ui_bridge::wire::quarantined_content_says_how_much_it_left_out`
`verified-by: bravebot_ui_bridge::wire::an_approval_prompt_carries_the_labels_its_bytes_and_its_check_were_released_under`
`verified-by: bravebot_agent::turn::approval_prompts_carry_the_labels_the_gates_released_their_bytes_under`
`verified-by: bravebot_agent::turn::a_prompt_to_read_output_carries_the_label_the_slot_was_released_under`
`verified-by: bravebot_agent::turn::the_preview_of_a_mixed_listing_states_the_label_of_the_untrusted_entries`

## Known costs

- **Eleven places do look at untrusted bytes in order to decide something.** Three are in the policy
  layer, the fourth is the search, which is in `bravebot-agent`, the fifth is the diagnostics
  sentence, the sixth is `read_git` and the seventh is the text-or-binary sniff and decoding of a
  read, which are in `bravebot-agent` too, the eighth is the choice of which parts of a tool
  result are taken, which is in `bravebot-mcp`, the ninth is the read of a processor's answer
  for one number or for an `apply` step's rows, and the numbering of the list those rows point
  into, which are in `bravebot-choice`, the tenth is the trigger listener's reading
  of a request's framing and path, which is in `bravebot-cli`, and the eleventh is the device
  listener's reading of a connection before it is a device, which is in `bravebot-listener`. The clauses above say
  nothing may, so these are exceptions, and they are written down rather than left to be found.

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
    something from happening, which is the direction this is built to fail in. It still decides
    which of two driver-worded sentences the planner is shown next: a reference to the document,
    or a sentence saying that answer marked no document and asking the planner to process it
    again. That is one bit, no larger than the sizes a reference already carries for any
    document the attacker's file produces, and no byte of the reply is in either sentence. In a
    manifest, where there is no planner to ask again, the same bit makes the driver retry the
    `process` step once itself, with the plan's instruction plus one fixed sentence, which is a
    second paid call to the model for every file an attacker can make answer unmarked.
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
  - **Make the processor's request fail.** By content the service refuses, by an answer cut at the
    token ceiling, by one the service reports as incomplete, or by answering with no bytes at all,
    which the transport refuses for every processor. The planner is told in a sentence the driver
    wrote that the request failed, with one word of a closed set the driver wrote (`refused`,
    `too-long`, `incomplete` or `undecodable`), as it is for every processor. In a manifest, where
    there is no planner to ask again, a refused, too-long or incomplete request fails the step at
    once, which ends the run. An undecodable one is an empty reply, asked for once more as it
    stands instead ([MANIFEST-8](manifest.md#MANIFEST-8)), sharing the one further try the mark
    left out above already spends: only a second failure, of either kind, ends the run there,
    which costs an attacker a second paid call for every file they can make the processor answer
    with nothing.

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
    its own. At a prompt about a picture ([VET-4](tools/vet-content.md#VET-4)), the picture is a
    file to open rather than bytes on the screen, so the quieter sentence can be all a person
    reads; it still buys no yes.
  - **Put the key that turns auto-vetting on in front of the person.** The standing key at a vetting
    prompt is drawn only where the check found nothing, so forcing `safe` is what offers it
    ([PROMPT-6](prompting.md#PROMPT-6)). It buys the offer and not the grant: the person has to
    press it, with the bytes on the same screen and a line beside the key saying what it turns on.
    At a prompt about a picture ([VET-4](tools/vet-content.md#VET-4)), the bytes are a path away
    rather than on the screen.
    It is drawn only on the two prompts that promote one slot's bytes, and the other direction is
    held shut, since a warning or a check that could not be made offers nothing. The reason the
    offer is put where the mode would have saved a keystroke rather than left out of the interface
    is that the alternative is somebody editing a settings file to get it, which is a decision made
    further from the thing it is about.
  - **Force the word `safe` where somebody turned auto-vetting on.** Then it promotes, and nobody
    reads the bytes. This is the one thing on the list that puts the attacker's bytes in the planner's context
    without a person in between, and it is why the mode is off until somebody asks for it in one of the three
    ways [CHECK-11](vetting.md#CHECK-11) names, and why the settings key that asks for it cannot be
    written into a checkout. What it buys is bounded by everything the verdict does not decide: one
    slot, whichever one the planner named, once, at `(T,priv)` so nothing leaves the machine and
    nothing becomes routing, with no trust rule written, and only on the routes
    [CHECK-12](vetting.md#CHECK-12) covers, which are the `vet_content` and `read_output` prompts and
    the result of a `run` that passed `read: true` ([RUN-22](tools/run.md#RUN-22)), so vouching for a
    path and a server's tool list are still asked about. The slot is still the planner's choice and not the content's. A picture
    ([VET-4](tools/vet-content.md#VET-4)) is the easier case of this: no encoding stands between the
    words drawn in it and the checker ([CHECK-15](vetting.md#CHECK-15)), and what it buys is the
    same one slot, once.
  - **Force the word `unsafe`, or reply with nothing a verdict can be read out of.** That lands on
    the prompt with the warning, which is the direction this is built to fail in. Where nobody
    can be asked, which is a run that bypasses permissions with auto-vetting on, there is no
    prompt: the verdict alone answers, anything but `safe` is a refusal, the planner is told
    the slot was kept back and not to ask again, and the person is told that auto-vetting kept it
    back ([CHECK-9](vetting.md#CHECK-9), [CHECK-12](vetting.md#CHECK-12), "nobody to fall
    back to", and the Known cost "A screened run with nobody to ask" in [vetting.md](vetting.md)).
    A `run` that passed `read: true` is the same in every mode: a word other than `safe` keeps the
    output back with no prompt drawn, and the person is asked when the planner calls `read_output`.
    What it buys is the denial of one slot read the planner asked for, decided by the check's
    reply with no person in between, at no cost to the attacker. It promotes nothing, chooses no
    destination, changes no label and writes no trust rule, and none of their bytes reach the
    planner on this path. The same refusal follows a check that could not complete. One forced
    `unsafe` word lasts for the rest of the turn for that slot, since the driver remembers the slot
    and not the reason, so the later `read_output` or `vet_content` on it uses the word and makes no
    second check ([CHECK-12](vetting.md#CHECK-12)).
  - **Put their words in the reason.** It reaches a person's screen and stops there. It is drawn
    inside a margin it cannot forge, no model is given it, and it is kept out of the audit trail.
    It can still *lie*, since nothing holds a reason against the content it describes and nothing
    could. The residue is that a plausible sentence might persuade somebody to skim, which is the
    same residue as the remark above and is
    [issue #23](https://github.com/brave/bravebot/issues/23).

  What is **not** on the list: choosing which slot is checked, choosing any destination, lowering
  confidentiality, writing a trust rule, or answering either of the two other prompts a check runs
  for: the vouch offer, which is the one that writes such a rule, and a server's tool list, which
  puts a server's words in front of the planner for every turn after
  ([CHECK-12](vetting.md#CHECK-12)). While auto-vetting is off, reaching the planner at all is not
  on it either, and a verdict is advice about bytes already on a person's screen. A picture
  ([VET-4](tools/vet-content.md#VET-4)) is a path away from it instead. The number of tokens a
  check wrote is not on it either: a delegate's token budget does not count a check's tokens, so
  steering the length of a reply moves no figure a delegate is told and no point at which it loses
  its tools ([DELEGATE-29](delegation.md#DELEGATE-29)). The same holds for the number of tokens a
  processor wrote, which a delegate's token budget does not count either.

- **The fourth is the search comparing its pattern against files nobody vouched for.** The search
  tool walks the files its directory and `include` select and tests each line against the pattern,
  whether or not a person vouched for the file. Which lines match, how many do, where the cap on
  matches and the boundary of a page fall, and whether the walk ran out of time are therefore decided
  by bytes an attacker may have written. The comparison is in `bravebot-agent`, which is the driver
  too ([LABEL-4](#LABEL-4)). Where every file the walk reaches is vouched for, the bytes are trusted
  and the comparison is the case [LABEL-5](#LABEL-5) allows. The rest of this entry is about a walk
  that also reaches a file nobody vouched for.

  The result is labelled from the paths the walk reached, before any file is opened, and one
  unvouched path makes the whole result untrusted ([search.md](tools/search.md)). The lines the
  comparison keeps therefore reach the planner only as a reference. What does reach it is a handful
  of numbers the comparison decided. Suppose an attacker owns a file the search reads. Everything
  that buys them is on this list:

  - **Set the result's line and byte counts.** Within the caps the search keeps, which are 200
    matches, 1,000 context lines and 200 files in a tally, with a line cut at 500 bytes and the
    lines of one result held to 20,000 characters. The
    planner is shown both numbers for any quarantined content ([LABEL-3](#LABEL-3)), and an attacker
    who owns a file already sets them for a read of it.
  - **Make the search report itself incomplete.** By filling the cap on matches, the cap on context
    lines, the cap on files in a tally or the 20,000 characters of a result, or by making files slow
    enough to read that the time cap
    stops the walk before it reaches the rest. The planner is told the result is incomplete and,
    where the cap on matches or the size of the result was the reason, is given the offset for the
    next page, which is the offset it asked for plus the number of matches that were returned.
    Where the cap on matches was the reason that number is the cap, and where the size was it is how
    many matched lines of the lengths the attacker chose fit in 20,000 characters, so the offset
    also tells the planner roughly how long those lines were. Being given that offset also tells it
    which cap was reached.
    That is the fact of being cut short, which cap did it, the rough length of the lines that
    matched, and a number made of the offset the
    planner supplied and the number returned. It can cost the planner searches, since each page reads
    the tree again, and nothing else.
  - **Tell the planner how many matches there are.** Only when the planner asks for a page past the
    last match, and then as one number, which the caps above do not bound.
  - **Choose which matches fall on which page, and make one of their own files contribute
    nothing.** The first happens inside a result the planner cannot read. The second is a file that
    is not valid text, which the search skips.
  - **Make one search expensive.** The time cap is checked between files and a file is read whole,
    so a single very large file costs its full size in reading time and memory before any line is
    tested.
  - **Set the tally the person sees.** The line shown to the person watching counts the matches and
    the files read, and the person is shown whatever those came to.

  What is **not** on the list is the thing that would matter. Which files the walk reaches is
  decided by the directory, the glob, the permission rules and the names in the tree, and never by
  a byte of any file. The label of the result is decided before any file is read. The pattern, the
  directory, the glob and the offset are the planner's own arguments, and no byte of a file
  reaches any of them. No text of any line reaches the planner. Choosing any destination stays the
  planner's proposal and a person's approval, since a search writes, fetches and runs nothing. The
  residue is that the planner reads these numbers and can be moved by them, as far as a count, an
  incomplete notice, an offset and a total can move it.

  Two alternatives cost more than this does. A search that opens only vouched files finds nothing in
  a tree nobody vouched for, and a workspace whose startup question was declined is such a tree. A
  search run as a processor job is a model call that produces a document or a remark, so it would no
  longer be a pattern match. Moving the comparison into `bravebot-core` is not a third, since that
  crate is the driver as well.

- **The fifth is the diagnostics sentence, whose numbers a language server computed from files
  nobody vouched for.** The `lsp` tool's `diagnostics` operation tells the planner how many errors a
  server found, the line each starts on (up to 20, then "and N more"), a warning count and a count of
  notes of any other kind ([LSP-13](tools/lsp.md#LSP-13)). The sentence is worded by `bravebot-agent`
  from those numbers, so the driver's wording branches on them, and it is labelled from the
  capability's own `(T,priv)` because it names no path the server reported
  ([LABEL-8](#LABEL-8)). The server computes the numbers from the queried file and from every file it
  indexed that file against, so a file in a vendored, cloned or distrusted tree, or a dependency of a
  vouched-for one, sets them. Suppose an attacker owns such a file. Everything that buys them is on
  this list:

  - **Set how many errors, warnings and other notes the planner is told about.** Any count the server
    can be made to publish.
  - **Set which line numbers are named.** Up to 20 error lines per call, and whether an "and N more"
    clause follows. That is a few hundred bits per call.
  - **Choose whether the sentence says "no errors reported yet".** It is worded as nothing stronger,
    so it is not a statement that the file is clean ([LSP-12](tools/lsp.md#LSP-12)).

  The driver supplies every word around those numbers, and none of the server's text is carried: a
  diagnostic's message, code and source travel as a reference the planner cannot read
  ([LSP-13](tools/lsp.md#LSP-13)). So the gain is attention and not content. It can move the planner
  toward a build or a read, and it is a covert channel with no decoder in the attacker's hands.

  What is **not** on the list is the thing that would matter. The planner picks the `path`, a routing
  field, and the attacker's bytes never choose it. No destination is chosen, since `edit_file` refuses
  an untrusted file ([EDIT-2](tools/edit-file.md#EDIT-2)) and every effect is still approved. No text
  the attacker wrote reaches the planner. The residue is that the planner reads these numbers and can
  be moved by them, as far as an error count and a list of lines can move it. This is the same shape
  as the search entry above: a handful of numbers decided by untrusted bytes.

  Two alternatives cost more. Labelling the sentence from the capability's `(U,priv)` quarantines it
  whole for any unvouched answer, and the planner then cannot be told that a file it is working on has
  errors. Gating the sentence on the queried file's trust-map entry, as the write-time sentence is
  gated, does not close it, since the counts depend on other files too and
  [LSP-3](tools/lsp.md#LSP-3) rules out borrowing the queried file's entry for what a server computed.

- **The sixth is `read_git` comparing blobs committed from paths the map distrusts.** `read_git`
  opens a repository whose whole `.git` the map vouches for ([GIT-2](tools/read-git.md#GIT-2)), and
  a blob in it holds the bytes of the working-tree file it was committed from, which may be a path
  the map distrusts. A `search` tests each line of such a blob against the pattern, a `diff`
  compares both sides of it to decide the hunks and whether the two are too different to compare,
  and `show` checks whether it is binary to choose between a description and its text. The
  comparison is in `bravebot-agent`, which is the driver too ([LABEL-4](#LABEL-4)). Where every
  path an answer shows is vouched for, the bytes are trusted and the comparison is the case
  [LABEL-5](#LABEL-5) allows. The rest of this entry is about an answer that also shows a path
  nobody vouched for.

  Each path is recorded as shown before its blob is read, and the answer is labelled from those
  paths once the walk has finished ([GIT-3](tools/read-git.md#GIT-3)), so one unvouched path makes
  the whole answer untrusted. The lines the comparison keeps reach the planner only as a reference,
  as the search's do. Suppose an attacker owns the committed bytes of such a path. Everything that
  buys them is on this list:

  - **Set the answer's line and byte counts.** Within the caps `read_git` keeps: 2,000 lines, 2,000
    characters a line, 20,000 bytes a page of a log with messages, 1 MiB a blob, 4 MiB a side of a
    diff, and the count cap. The planner is shown both numbers for
    any quarantined content ([LABEL-3](#LABEL-3)), and an attacker who owns a file already sets
    them for a read of it.
  - **Make the answer report itself incomplete.** By filling a cap, or by making the walk run to
    its deadline, or a diff run to its bound of four million steps. The planner is told the answer
    is incomplete and nothing else: the skip that lists the next page is printed inside the
    quarantined body, so unlike the search's offset it does not reach the planner.
  - **Tell the planner by a count whether the pattern it chose matched.** The line count of a
    search over a single distrusted blob is one number the blob decided.
  - **Choose which matches or hunks fall on which page, and make one of their own files print
    nothing.** The first happens inside the body the planner cannot read. The second is a blob that
    is binary or past the size cap, which a search names or describes and does not print.

  What is **not** on the list: the label, which depends only on the paths shown and on `.git`, each
  recorded before its blob is read and each asked under every name its file or directory has (the
  names are the file system's, found from names and metadata and never from a blob's bytes,
  [TRUST-4](trust-map.md#TRUST-4)), any destination, any argument of a later call, and a line of the
  blob. The pattern, the path and the skip are the
  planner's own arguments and no byte of a blob reaches them. This entry cannot take the search's
  wording that the result is labelled "before any file is opened" or that which files the walk
  reaches is never decided by a byte of a file. `read_git` labels an answer after the walk has
  finished, and a page boundary that earlier blobs' matching lines decided stops the walk, so those
  bytes choose which later paths are reached. That is sound only because every path read is pushed
  to the shown list before its blob is, which is what keeps the label from being steerable.

  Two alternatives cost more than this does. Leaving unread every blob whose path the map distrusts
  and reporting how many were left out would close the entry, and it changes what a history
  question can find in a repository that holds vendored code, which is a product decision for the
  maintainers and not made here. Moving the comparison into `bravebot-core` is not another, since
  that crate is the driver as well.

  The two claims the entry rests on, that only the counts and the incomplete notice follow what the
  blob holds and that the skip stays in the body, are pinned under
  [GIT-14](tools/read-git.md#GIT-14).

- **The seventh is the text-or-binary decision on a read of a file nobody vouched for.** For a path
  the map does not vouch for, `read_file` does not open the file to hand back text, it defers a
  slot ([READ-1](tools/read-file.md#READ-1)). It still has to say now whether the file is text, as
  [READ-3](tools/read-file.md#READ-3) requires, so `Workspace::survey` in `bravebot-agent` reads the
  first 8 KiB and `bravebot_filetype::decode_text` reads it: a UTF-16 or UTF-32 byte order mark is
  honoured, and the result is binary where the NUL byte or control character test fails. A file that
  fails is refused to the planner in the driver's own words, "is a binary file, so it cannot be read
  as text", and one that passes becomes a reference with a byte count. A slot is filled with the
  same `decode_text` over the whole file, and the slot records whether its text is a decoding, a
  second bit about the shape of the file's first bytes. The slot's consumers read that bit: a
  processor's input and a check take the text, and everything that writes it or hands it to a
  program refuses it. A whole read an edit works from and a plan's page keep to text that is UTF-8
  as it stands (`Workspace::read` and `page`). A page the planner
  is shown goes through `bravebot_filetype::decode_text`, which looks for a UTF-16 or UTF-32
  byte order mark before the NUL test, decodes a file with one, and reads any other file that
  passes the test as UTF-8 with each invalid byte replaced, unless the replacements are more than
  30 percent of it. It then applies the same sniff to the decoded text. A page that is a decoding
  carries a note naming it ([READ-3](tools/read-file.md#READ-3)). The comparisons are in
  `bravebot-agent`, which is the driver too ([LABEL-4](#LABEL-4)), and in the `filetype` crate it
  calls. That crate holds no `declassify`, so no pinned count moves when it changes. Where the path is vouched for, the bytes are trusted and the comparison is the case
  [LABEL-5](#LABEL-5) allows. The rest of this entry is about a path nobody vouched for.

  `Workspace::peek_labelled_for_write` also sniffs file bytes, for [WRITE-8](tools/write-file.md#WRITE-8):
  whether the file is valid UTF-8 and, if not, whether `decode_text` reads it. It reads the file
  whichever label the path has, and labels the answer as the trust map holds the path.
  `holds_another_encoding` consults the answer only when that label is trusted, so the sniff
  decides something only for a file the trust map vouches for, and nothing follows from the bytes
  of a file nobody vouched for.

  Suppose an attacker owns the file. Everything that buys them is on this list:

  - **Choose which of two driver-worded results the planner is shown for the file.** A reference
    with a size, or the refusal above. That is one bit, and the planner was already free to read a
    different path.
  - **Put the file out of reach of a processor.** A NUL in the first 8 KiB, or invalid UTF-8 past
    the 30 percent replacement limit, is what makes a fill of the slot fail with a refusal in the
    driver's own words. A byte order mark, or invalid UTF-8 within that limit, fills the slot as a
    decoding instead, so a processor still reads it; the slot is refused only where something
    writes it back or hands it to a program.
  - **Choose how the bytes are split into lines** for a page of the file that is read through a
    manifest step. A mark makes the units two or four bytes, and an invalid byte makes one
    character. The line and byte counts the planner is told for a quarantined page are already the
    attacker's to set ([LABEL-3](#LABEL-3)).

  What is **not** on the list: the label, which depends on the path alone, any destination, any
  argument of a later call, and any byte of the file. The size the planner is told is the file's own
  size and already the attacker's to set, and the sentence is one the driver wrote. The search
  skipping a file that is not valid text, in the fourth entry, is the same kind of decision.

  The alternative costs behaviour and is a product decision for the maintainers, not made here: a
  quarantined read decides from the path and the size alone, and the text-or-binary answer is found
  when a processor or a write reads the bytes. That would give the planner one result shape for
  every unvouched file, and it changes [READ-3](tools/read-file.md#READ-3). Moving the sniff into
  `bravebot-core` is not another, since that crate is the driver as well.

  The result shape is pinned by `a_binary_read_tells_the_model_it_is_binary` in
  `crates/agent/tests/turn.rs`, which fails if the refusal stops being said.

- **The eighth is the choice of which parts of a tool result are taken.** A server's result is a
  list of parts, each with a `type`, and `bravebot-mcp` decides from what the server wrote what each
  part is ([MCP-6](mcp.md#MCP-6)). A part of `type` `text` is text. A part of `type` `image` is a
  picture where its `mimeType` is one of `image/png`, `image/jpeg`, `image/gif` and `image/webp`, its
  `data` is base64 in its canonical form, and it decodes to between one byte and 8 MiB. Every other
  part is left out and counted. The comparison with `text` is older than the pictures and is recorded
  here with them. The crate is not the driver ([LABEL-4](#LABEL-4)) and holds no `declassify`, so no
  pinned release moves when this changes. The constructors it labels with are pinned, and a second
  comparison of the same kind elsewhere in the crate is found by reading the diff.

  The crate hands `bravebot-agent` the result already sorted: the text, the pictures, each under a
  type from the list, and a count of the parts left out. The driver acts on that shape and on no
  byte. The turn mints a reference for the text and one for each picture, and the planner is told how
  many parts were left out. Those are decisions taken from a shape the server chose, and this entry
  covers them too.

  The crate also decides from what the server wrote how a call ended, and the driver reads that. A
  tool that reports `isError`, a JSON-RPC error and a reply the crate cannot use are each a failure
  of the call. A reply cannot be used when it does not parse, holds neither a result nor an error,
  comes with an HTTP status that is not a success, or is a line past 16 MiB
  ([MCP-13](mcp.md#MCP-13)). The driver tells the planner in its own words that the call failed. In
  a plan run ([MANIFEST-15](manifest.md#MANIFEST-15)) it reads one distinction in these, which is
  whether a reply arrived. Every reply is on the same side of it, whatever it holds, so the steps
  after one run, except that an `apply` step skips its calls that depend on a call the server
  reported failed ([MANIFEST-16](manifest.md#MANIFEST-16)). A call that got no reply, because its connection closed or its bound passed, ends
  the run. [MCP-5](mcp.md#MCP-5) says which failures are replies.

  The sentence is one of two the driver wrote, chosen by which kind of failure it was: a JSON-RPC
  error, or a reply that cannot be used. It holds the name of the tool the planner called and
  nothing the server chose. The error code a server picked, the HTTP status it answered with and the
  byte count of a reply are numbers the server chooses, so they are not in the sentence and are
  not a cost ([MCP-8](mcp.md#MCP-8)).

  `verified-by: bravebot_agent::mcp::a_code_or_a_status_a_server_chose_never_reaches_the_planner_over_http`
  `verified-by: bravebot_agent::mcp::a_code_a_server_chose_never_reaches_the_planner_over_stdio`

  Suppose a server, or a page that steers a tool the server relays, is hostile. Everything that buys
  it is on this list:

  - **Choose whether the result has a text part and how many pictures it has.** The planner is given
    a reference for each, so it learns the count. A server already sets the number of references a
    result has by what it returns.
  - **Choose which of four media types a picture is held under.** The reference names it. It is one
    of four words the driver wrote, picked by comparing the server's string with the list, and the
    server's string is not kept. The same word decides the extension a `write_file` of that slot
    must have ([WRITE-9](tools/write-file.md#WRITE-9)), the extension of the copy `vet_content`
    writes, and the media type sent to a backend after `vet_content`. A server can make a planner's
    write of a picture refuse by naming another type, and cannot make one land at a path the
    planner and the person did not choose.
  - **Choose how many parts are left out.** By sending parts of other types, pictures of another
    type or over the cap, or data that is not canonical base64. The planner is told that number, in a
    sentence the driver wrote.
  - **Choose a picture's size within the cap.** The reference gives the byte count of the `data:`
    URI, as it does for any content, and a server already sets that. The row the person is shown
    gives the number of bytes the base64 decodes to, which the crate counts as it checks the base64.
    The driver carries that number into a sentence and does not compare it with anything.
  - **Decide whether a plan run goes on past a call, by not replying.** A server that closes its
    connection or stays silent past its bound ends the run at that call, so the approved steps after
    it do not run. Nothing it sends changes which steps run, and it cannot make a step run that the
    plan did not hold. A server that is not there ends a run the same way.
  - **Skip an `apply` step's calls that depend on a call, by reporting that call failed.** The
    step's line counts them as skipped, and the person sees what became of each. The server could
    leave the calls undone as well by not acting on them.

  That is a few counts, a choice among four words and whether a plan run goes on, for each call, in
  the driver's sentences. A server could already set each by choosing what to return and whether to
  answer.

  What is **not** on the list: the label of any part, which is the capability's own whatever the parts
  hold ([LABEL-8](#LABEL-8)), any byte of a part reaching a decision or a message to the planner, which
  slot holds what, and any destination or argument of a later call. The `data:` URI held for a picture
  is this client's own writing, a type from the list and the base64 alphabet and nothing else, so
  turning it back into bytes cannot fail on what the picture holds. A picture's pixels reach a model
  in two ways. A processor is given it, which holds no tools and answers in text the planner cannot
  read ([PROC-15](processors.md#PROC-15)). The planner is shown it after `vet_content`
  ([VET-4](tools/vet-content.md#VET-4)), which is the existing exception for a picture and costs
  there what it costs for a picture read from a file.

  Two alternatives cost more. Fixing the media type in the person's declaration of the server would
  remove the `mimeType` comparison and leave the one on `type`, and would refuse a server that returns
  more than one kind. Taking no picture leaves a tool such as a page's screenshot unable to return
  one. Moving the comparison into `bravebot-core` is not a third, since that crate is the driver as
  well.

  The claims that a part is taken only under these conditions and that nothing else a server wrote is
  kept are pinned under [MCP-6](mcp.md#MCP-6).

- **The ninth is the read of a processor's answer for one number.** A processor asked to choose
  ([PROC-16](processors.md#PROC-16)) answers in text. `bravebot-choice` decides from that text which
  whole number the processor chose, from 1 to the limit the planner wrote, or 0 where it chose none:
  with the white space at both ends taken off, the text must be one or two ASCII digits with no sign
  and no leading zero, and the number must not pass the limit. The crate is not the driver
  ([LABEL-4](#LABEL-4)). It holds no `declassify` and no `Labelled::new`. The text reaches it
  through one gate of the policy layer, `Policy::read_a_number`, which hands it to a `fn` the crate
  supplies, takes one byte back and labels that byte as the answer was labelled, by taint over what
  the processor was given. The gate compares nothing, its uses are pinned in `guards` as the other
  gates' are, and a second reader of an answer is a moved count. Everything else the crate does is
  `number_in`, and a second comparison of an answer's text elsewhere in it is found by reading the
  diff. The `fn` does not capture the caller's state, and what stops it keeping a copy of the text
  for a later call is `the_reader_of_a_number_keeps_nothing_between_calls`, which fails on a
  `static`, a `thread_local!`, interior mutability, a lock or an atomic in the crate's source.

  The driver carries the byte and decides nothing from it. It is stored as a labelled number under
  a reference of its own kind, drawn to the person as digits beside the call it will fill, and put
  into the one argument the person's declaration names after the person answers the call
  ([MCP-14](mcp.md#MCP-14)). A 0 is stored, drawn and carried like any other number, and the
  extension refuses it. The planner is told one sentence that does not depend on the number, and the
  reference states the limit and the label and no count of lines or bytes. The number leaves the
  slot store in two gates, `Policy::choice_for_display`, which returns the digits for the person's
  screen, and `Policy::carry_a_choice`, which returns an opaque `Carried`. Only
  `bravebot_mcp::protocol::with_choice` opens that, with `Carried::into_wire`, to write the number
  into the call. The uses of all three are pinned in `guards`.

  An `apply` step's rows ([manifest.md](manifest.md#MANIFEST-16)) are read the same way, through two
  more gates. `Policy::number_a_list` hands a read's reply to the crate, which decides which entries
  hold the fields the plan's arguments take and numbers them; the numbered text is quarantined for
  the processor at the list's label, so text inside an entry cannot shift the count.
  `Policy::resolve_proposal` hands the processor's answer to the crate, which reads its rows,
  resolves each against the plan's shape and the numbered entries, gives each value the type the
  server's list declares for its argument, drops a row that does not fit, holds a value that is not
  that type or repeats an earlier row, and counts each drop by a reason of a closed set. What comes
  back is the resolved list, labelled with everything it derives from, which the driver carries to
  the server and reads only through `Policy::proposal_for_display`, to draw it for the person. Which
  calls are kept from the person's ticks, which a failure skips, and whether the list holds any call
  are worked out in the policy layer too, by `Policy::keep_applied` and `Policy::proposes_nothing`,
  so the driver makes the calls it is handed by their places in the list. A hostile entry or answer
  can choose which of the calls the plan allows are proposed, within its caps, and nothing else.

  Suppose the page a processor read is hostile and talks the processor round. Everything that buys
  it is on this list:

  - **Choose the number, from 1 to the limit, or 0.** At most 100 values. The planner is shown no
    number. The person's question draws it and the extension receives it.
  - **Choose which element the person is asked about.** The page decides which element sits at
    which number and what each is called. Whether the action happens is decided by the person, in a
    window of the browser the page cannot rewrite ([ROUTE-10](routing.md#ROUTE-10)).
  - **Make the call fail,** by answering so that the text reads as 0, or by changing the page after
    the list. The planner learns that the call failed, as it does when a page hides an element.
  - **Make the processor's request fail,** by content the service refuses, by an answer cut at the
    token ceiling, by one the service reports as incomplete, or by answering with no bytes at all,
    which the transport refuses for every processor. The planner is told in a sentence the driver
    wrote that the request failed, with one word of a closed set the driver wrote (`refused`,
    `too-long`, `incomplete` or `undecodable`), as it is for every processor, and no number is
    stored.

  What is **not** on the list: the label of the number, the tab, the tool, the action, the text
  typed, the key, which argument the number fills, any byte of the answer beyond the one this crate
  returns, any sentence to the planner except that failure word and the one about a failed call,
  and whether the action happens.

  Two alternatives cost more. The person reading the list and typing the number removes this entry
  and leaves the person doing the processor's work, which is what the extension offers without a
  processor ([ROUTE-9](routing.md#ROUTE-9)). Moving the reader into `bravebot-core` is not another,
  since that crate is the driver as well.

  The claims that the reader takes one byte out of an answer and labels it as the answer was
  labelled are pinned under [PROC-16](processors.md#PROC-16), and the claims that the number reaches
  a call only through the gate that carries it, after the person's answer, are pinned under
  [ROUTE-10](routing.md#ROUTE-10) and [MCP-14](mcp.md#MCP-14).

- **The tenth is the trigger listener's reading of a request.** `bravebot trigger serve`
  ([TRIGGER-10](triggers.md#TRIGGER-10)) answers a request from a sender nobody vouched for, and
  four of its decisions are taken from bytes the sender wrote. It is in `bravebot-cli`, which is not
  the driver, and the body is never one of them.

  - **The framing.** `parse_head` in `crates/cli/src/trigger.rs` refuses a request whose method is
    not `POST`, whose version is not `HTTP/1.x`, that has a `Transfer-Encoding` header, that has no
    `Content-Length` or more than one, or whose `Content-Length` is not digits, that has no signature
    header or more than one, and a path that is
    not a name. Each is decided from the request line and the headers only, and none is decided from
    the body. The answer to each is the same fixed 404.
  - **The body length.** The `Content-Length` is compared with the cap to decide whether to read the
    body or refuse it whole with a fixed page. It says how many bytes to read and nothing about what
    they say.
  - **The path.** The path is looked up as a name among the entries the person stored. The sender can
    only pick among names the person made, each of which the person agreed would start its own
    prompt.
  - **The signature.** The signature header is compared with a value computed from the person's
    secret, and the event it covers is the person's own. A delivery that does not verify starts
    nothing.

  The listener holds no `declassify`. The body reaches the run's standard input unchanged and is read
  by nothing here. A header, a path suffix or a byte of the body that selects a trigger, an event, a
  flag, a directory or a limit is a violation, and not one of these four.

- **The eleventh is the device listener's reading of a connection that is not yet a device.** The
  host started with `--listen` ([LISTEN](device-listener.md)) accepts a connection from a program
  nobody vouched for and decides from what it sends whether to go on. The listener is in
  `bravebot-listener`, which is not the driver, depends on no crate that opens a session, and holds
  no `declassify`. Four of its decisions are taken from bytes the sender wrote.

  - **The key.** `public_key_hash` in `crates/listener/src/identity.rs` reads the one position of
    the certificate that holds the public key, and names no key for any shape but a P-256 key. The
    hash it returns is looked up in the registry the person made, and the lookup decides whether the
    connection goes on.
  - **The first line.** A connection that is not a paired device may send one line, which
    `first_line` and `begin_of` in `crates/listener/src/server.rs` read: at most 4,096 bytes within
    the time of the handshake, as JSON with the method `pair.begin`, a `proof` of hexadecimal digits
    and optionally a `label`. A line that is too long, late or of another shape ends the connection.
    The label is carried to the person's screen escaped and is compared with nothing.
  - **The proof and the code.** The proof is checked against a value the host computes from the
    window's secret, the connection's TLS exporter value and the sender's key hash, and the code
    typed on the host against one computed from the same inputs. Each check takes the same time
    whatever the sender sent, and a mismatch closes the window.
  - **The lines of a paired device.** A paired device is the person's own phone, and its lines are
    read as a client's are, by method. A line the host can read as a request asks to read or to
    control as the host's allowlist says for its method, and every other line asks to control.
    That is the operation the gate is asked about. The service the host gives a device holds no way
    to a session, a bridge or a tool, and answers a method with a fixed answer or a fixed refusal.

  Nothing a device sends reaches a turn, a prompt or a tool, and no byte of it reaches the planner.
  A branch in the listener on any other byte of a connection, or a service that gives a device a way
  to a session before the scope check and the destination policy of stage 6 exist, is a violation
  and not one of these four.

- **A bounded listing's counts of the files and the subdirectories left out of each directory it
  cut are not one of the eleven.** A listing with a depth takes at most 200 files and at most 200
  subdirectories from any one directory ([LIST-5](tools/list-files.md#LIST-5)). The walk sorts a
  directory's files by name, drops each file the include pattern does not match or a permission
  rule covers, and keeps the first 200 of the rest, so which 200 are kept is decided by the name
  order that already decides what the cap over the whole walk keeps. It sorts the directory's
  subdirectories by name in the same way, and keeps the first 200 of those the walk's existing
  filters leave (ignored directory names, links, the stores and the permission rules), whether it
  goes into them or stops at them, from a budget the files do not spend. The count of files left
  out is taken after the include pattern and the rules, which the walk applies to every file it
  reports whether or not a cap applies. The count of subdirectories left out is taken after the
  rules, so a subdirectory a rule covers is not counted, and the include pattern does not apply to
  directories. A cut directory is recorded under the name its caller passes to `walk_filtered`:
  `Workspace::list` passes the walk root's name once, `.` when the resolved directory argument is
  the workspace root and its relative name otherwise, and each recursive call passes the relative
  name it built for the subdirectory, so no name read from a directory entry is tested to tell the
  root from a subdirectory. A directory has one record in `partly_listed` however it was cut. The
  pass over its files adds the record when files were left out, and the pass over its
  subdirectories adds it when none exists and otherwise adds to the subdirectory count of the one
  that does. Each cut directory counts as one entry against the cap over the whole walk, in
  `Collected::len` during the walk and in `at_the_cap` after it. `Workspace::list` sets
  `at_the_cap` when the files, the directories stopped at and the cut directories together pass
  that cap, and sets `truncated` to `at_the_cap || !partly_listed.is_empty()`. It sorts the cut
  directories by name and truncates them to what the cap leaves after the files and the directories
  stopped at, the same sort by name and truncation it applies to those two lists. Each pair of
  counts and `at_the_cap` are released inside the existing release in `list_files`, as a pair of
  numbers for each directory, and each pair reaches the planner as numbers beside the directory's
  reference, with no name. The driver branches on the comparison of the files taken from a
  directory with 200, the comparison of the subdirectories taken from it with 200
  (`taken_directories >= most` in `walk_filtered`, a count of directory entries kept by the filters
  above, no name read), whether the count of files left out is above zero, and whether a directory
  already has a record, in the walk; on whether any directory was cut, whether any cut left files
  out, whether any left subdirectories out and
  `at_the_cap`, which together choose the sentences in `list_files` and in `listing_capped_note`
  (`cut.is_empty() || at_the_cap`); and on each count's value, which `left_out_clause` compares
  with zero to decide whether to say it and `tally` compares with 1 to write "file" or "files" and
  "subdirectory" or "subdirectories". Apart from the include pattern, the rules, the fixed list of
  ignored directory names, the resolved directory argument and the name order above, no name is in
  any of these conditions. A name in that sentence, or a choice of which directory to cut or how to
  word the sentence made from a name, is a violation and not covered here.

- **A listing's walk is stopped by a clock and skips a directory a pattern's prefix rules out, and
  neither is one of the eleven.** `Workspace::list` gives the walk a deadline, `search.maxSeconds`
  from the time of the call ([LIST-4](tools/list-files.md#LIST-4)). `walk_filtered` asks the clock
  before each file and each subdirectory it takes, and stops when it has run out. That is a
  comparison of the time with the deadline, which no name enters, and it decides where the walk
  stops and never which names it keeps. The walk records only that it stopped, as one flag released
  inside the existing release in `list_files` with the cap and the unreadable-directory flag, and the
  flag reaches the planner as one fixed sentence with no name. A search's walk and a repository
  map's have no clock. The walk also does not open a subdirectory when the pattern cannot match
  beneath it ([LIST-3](tools/list-files.md#LIST-3)). `Wanted::could_hold_an_admitted_path` decides
  that from the expanded pattern and the subdirectory's relative name, both readings, by
  `glob::could_match_beneath`: it compares the pattern's segments with the directory's segments,
  which are the two inputs `Wanted::admits` compares for a file. It runs after the permission rules
  and after the 200-subdirectory count and the depth stop, so which directories a bounded walk
  takes, names and leaves out is the same as with no pattern, and it skips only the descent. The
  pattern is the planner's argument and a directory's name is a name the walk already compares in
  `walk_filtered`, so the condition is on the include pattern and the name order of the
  directory, which the sentence above already lists. No file's bytes are read, and a name in a
  sentence, or a choice of what the walk keeps made from a name other than the pattern's comparison
  with it, is a violation and not covered here.

- **The reason a checkout's refused filesystem entry is named is chosen from the entry's spelling,
  and is not one of the eleven.** A `run` is not started while an entry of `sandbox.filesystem.denyRead`
  or `denyWrite` cannot be applied ([SANDBOX-25](sandboxing.md#SANDBOX-25)). Where the project or
  local settings file of a checkout wrote that entry, the sentence the planner reads names the list,
  one reason and the file, and not the spelling. The reason is one of five sentences the driver
  wrote (`sandbox-rule-*` in the catalog): the entry starts with a variable, starts with `~` in a
  session with no home directory, climbs out of the directory it is read from, holds a wildcard in a
  write list, or holds a wildcard that looks at more of the disk than a pattern may. `resolve` in
  `bravebot-sandbox` picks it by reading the entry's text, and for the last one by counting the
  paths the wildcard visits on the disk. The same function, reading the same text, already decided
  whether the entry is refused, and so whether the stage started and whether the planner was told it
  did not. What is new is which of five sentences that refusal carries.

  What the planner can learn is which of those five the first refused entry hit: one of five, about
  2.3 bits, and for the last one that the wildcard's directory held more entries than a pattern may
  visit. This is a branch on bytes a checkout wrote whose result reaches the planner, and it is
  recorded here for that reason. It is not counted among the eleven because the same function, reading
  the same text, already decided that the entry is refused and the stage not started, which the
  planner was already told, and because the sentence chosen carries no byte of the entry. The file
  that holds the entry is a checkout's file the person did not trust, and its author wrote the entry
  and already knows which reason it hits. The choice names no tool, argument, destination or label
  and changes no effect: the stage is refused whichever of the five is named, a checkout can add a
  refusal and never an allowance ([SANDBOX-25](sandboxing.md#SANDBOX-25)), and which entry is
  reported is the first refused one in list order. A sentence that put any part of the spelling in
  it, a reason outside the closed set, or a different sentence chosen from the entry for any other
  purpose is a violation and not covered here.

- **Whether a `run` line is refused for standing for too many names, or for reading too many
  directories, is not one of the eleven, and is one bit per call about a directory nobody vouched
  for.** [CMDLINE-4](tools/command-line.md#CMDLINE-4) bounds a word at 100 arguments and a walk at
  4,096 directories. The compile compares the number of names the patterns matched with the first,
  and the number of directories it read with the second, and a line past either is refused. The
  refusal states the cap, which is a constant, and states no count taken from the directory's
  entries, so no number out of the directory is spelled into a sentence the driver writes. The
  brace bound is counted from the planner's own word before anything is read and its count is
  still spelled. What remains is that a line is accepted or refused according to whether the count
  passes the bound. A planner that pads a pattern with `k` literal alternatives, as in
  `dir/{p*,{1..k}}`, learns from one call whether the names starting with `p` number more than
  `100 - k`, and finds a count by about seven calls. The comparison reads no name and no byte of a
  file: it reads how many entries a directory listing held, and the planner learns one bit per call
  from the answer, as it learns whether a path exists from whether a read of it succeeds. The
  bit is bounded by the calls the planner makes, each of which a person may be asked about when
  it is not covered by a rule, and no name is revealed by it. The same kind of bit is in the
  reason a checkout's refused filesystem entry is named, when the wildcard visits more paths
  than a pattern may, which is the entry above this one. The refusal carries no name, no
  count and no fact about which pattern matched. A change that spells the count, or a name, or
  chooses the sentence from either, is a violation of shape 1 or 3 and is not covered here.

- **Whether a checkout is refused for an entry under a path nobody vouched for is one bit per call
  about that path.** [CHECKOUT-4](checkouts.md#CHECKOUT-4) refuses a checkout with one fixed
  sentence where the cause is an entry at or under a distrusted path, so the sentence names no
  check, no name, no count and no size. The refusal itself is decided by comparing the names, the
  file and directory counts and the sizes under that path with the checkout's rules and bounds, and
  a spawn that is refused differs from one that is made. A planner that can place entries in a
  repository learns from one spawn whether any of them failed a check, as it learns whether a path
  exists from whether a read of it succeeds. Whether the sentence is the fixed one is chosen from
  the map's answer for the entry's path and from nothing the entry holds. For a tree that holds
  trusted and distrusted entries, a bound both pass together is the fixed sentence once a
  distrusted entry has been counted, so which sentence a mixed tree gets depends on the order the
  walk meets its entries. The bit is bounded by the spawns the planner makes, each of which a person
  may be asked about, and no name or number is revealed. A change that names the check, the entry
  or a count in the fixed sentence, or that chooses the sentence from the entry's name or size, is a
  violation of shape 1 or 3 and is not covered here.

- **Whether a `cell` edit of a file named by `path_ref` is refused as not a notebook is chosen from
  the extension of the name behind the reference, and only after a person vouched for the file.**
  [EDIT-8](tools/edit-file.md#EDIT-8) refuses a `cell` edit of a file nobody vouched for with the
  refusal [EDIT-2](tools/edit-file.md#EDIT-2) gives, whatever the name ends in. For a file a person
  vouched for, `edit_file` tests whether the name ends in `.ipynb`, and `output_of` on `read_file`
  does the same, so the planner learns from the answer whether the name behind a reference it holds
  is a notebook. The name came out of a directory, and the test is not made until the person has
  vouched for that file, which they did knowing its name, so what the planner learns is a fact the
  person already accepted. It is one bit about one vouched file per call, from a closed test of the
  extension against one suffix. A change that makes the test before the vouch, or that tests any
  other part of the name, is a violation of shape 1 and is not covered here.

- **A model that searches the web is kept out by a list of ids, and a model nobody listed is not.**
  Whether a model adds retrieved pages to its context is not something a request or a reply states,
  so `RETRIEVING_MODELS` is a list a developer writes from a provider's documentation
  ([LABEL-8](#LABEL-8)). A search model released since, an id under a proxy's own name, and a model
  that a gateway routes to a search backend are requested and labelled from the context this program
  built, which is the label the pages did not earn. The list is compared with the name a request
  carries and with nothing a reply says, so no byte from outside decides it.

- **The line count and the byte count a reference states for a line's output depend on whether, and
  on how many names, a pattern in a position the program does not read as a file matched.**
  `basename incoming/a*` is asked about and its output is untrusted whether or not a name begins
  with `a` ([CMDLINE-4](tools/command-line.md#CMDLINE-4)), so neither the question nor the label
  depends on the names in `incoming/`. The reference states the number of lines and the number of
  bytes of what the program printed. Where no name matched, the program prints the pattern, so
  the reference states one line and the pattern's length. Where names matched, the program prints
  its result for each of them, so the reference states a line for each name printed and the total
  of their lengths. A planner that knows the pattern's length can tell from a different line count
  or byte count that some name matched, and from the line count how many were printed. The two
  numbers are the ones the planner is shown for any untrusted output ([LABEL-3](#LABEL-3)). The
  driver carries them and does not branch on them, and no name is spelled to the planner, only the
  lengths. Quoting the pattern passes it to the program unmatched.

- **The refusal of a word that stands for more than 100 arguments states how many names a pattern
  matched.** Where an unquoted pattern matches more names than a person can be shown at one
  prompt, the compile is refused and the sentence the planner is told carries the count of names
  the walk found ([CMDLINE-4](tools/command-line.md#CMDLINE-4)). The sentence names the word as
  the planner wrote it and no name. In a directory nobody vouched for, the names are chosen by
  whoever wrote it, so the planner can learn from the refusal, and from the count in it, how many
  names match a pattern it chose. The count is a number and the bound is fixed, so the comparison
  with 100 is the only decision the driver takes from it. Quoting the pattern passes it to the
  program unmatched and is not refused.
