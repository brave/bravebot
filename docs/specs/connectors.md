---
id: CONNECTOR
title: Connectors
status: normative
governs:
  - crates/connector/Cargo.toml
  - crates/connector/src/lib.rs
  - crates/config/src/connectors.rs
  - crates/config/src/connectors/forms.rs
  - crates/agent/src/connectors.rs
documented-by: none (gap: a manifest run fetches through a connector, and the page saying how to install a definition and its authority is owed)
---

## Scope

The client half of a connector call, and the installed definitions it is made against. A
credential authority, a separate program the person
installed, holds every credential a connector uses and makes every call to the service. This
client starts it, hands it one request, and reads one reply. This process reads only the surface of
a connector's definition, what a plan may name; the wire part, how to call the service, is in the
authority's own copy. How a plan names a connector step, and what the authority enforces on its own
side, are
[../design/connectors.md](../design/connectors.md) and
[../design/credential-brokering.md](../design/credential-brokering.md).

A manifest run reads the installed definitions, tells the planner what they offer, holds the plan
to them, and fetches through the client ([manifest.md](manifest.md)). A plan may not yet act
through a connector.

## Clauses

<a id="CONNECTOR-1"></a>
### CONNECTOR-1: the client holds no credential and names no service

A request names the connector, the authority's definition, the credential item, the resource, the
step and the routing the plan locked, and the reply's fields are the authority's own protocol.
Nothing in either is a token or a key, and nothing selects a service: the authority's definition
for the named connector decides what the call is.

**Why.** A credential this process never holds is one no prompt injection can make it send, and a
client that knows no service is one a new connector needs no code in.

`verified-by: by-construction (of the workspace's crates it depends on bravebot-core and bravebot-sandbox alone, reads no file, passes the authority only the variables CONNECTOR-4 names, and its request type has no field that can carry a secret the authority holds; the authority's definition, not this crate, names the service)`

<a id="CONNECTOR-2"></a>
### CONNECTOR-2: the authority is a program at an absolute path

The authority is started from the absolute path its definition gives, and a relative path is
refused before anything starts.

**Why.** A name looked up on `PATH` is one a checkout's environment can answer, and whatever answers
receives the request and any text an act carries.

`verified-by: bravebot_connector::lib::an_authority_named_by_a_relative_path_is_refused`

<a id="CONNECTOR-3"></a>
### CONNECTOR-3: the request goes on standard input, never on the command line

One request is written to the authority's standard input as one JSON document, with an act's text
inside it, and one reply is read from its standard output. The command line carries one fixed word
and nothing from the request.

**Why.** Another process on the machine can read a command line. It cannot read a pipe.

`verified-by: bravebot_connector::lib::the_request_and_its_text_go_on_standard_input_and_never_on_the_command_line`
`verified-by: bravebot_connector::lib::an_authority_that_answers_is_read_whole`

<a id="CONNECTOR-4"></a>
### CONNECTOR-4: the authority starts with a cleared environment, outside the checkout

The authority is started with only `HOME`, `XDG_RUNTIME_DIR`, `PATH`, `TMPDIR`, `LANG` and
`DBUS_SESSION_BUS_ADDRESS`, each where this process has it set, and with nothing else of this
process's environment. It starts in the root directory, not the one this process runs in.

**Why.** The rest of the environment may hold this process's own credentials, which are not the
authority's to read, and a relative path the authority reads should not be one a checkout answers.

`verified-by: bravebot_connector::lib::the_authority_starts_with_only_the_variables_it_is_passed`
`verified-by: bravebot_connector::lib::the_authority_starts_outside_the_checkout`

<a id="CONNECTOR-5"></a>
### CONNECTOR-5: a call needs its capability

A call the policy has not granted the named capability is refused before the authority starts.

`verified-by: bravebot_connector::lib::a_call_without_the_capability_is_refused_before_the_authority_starts`

<a id="CONNECTOR-6"></a>
### CONNECTOR-6: what the service sent carries the capability's label

A reply's payload and its failure detail are what the service sent back, so each is labelled with
the call's capability label and read nowhere in the client.

`verified-by: bravebot_connector::lib::what_the_service_sent_carries_the_capabilitys_label_and_the_authoritys_fields_do_not`

<a id="CONNECTOR-7"></a>
### CONNECTOR-7: the authority's own fields are held to their form

The rest of a reply is printed on a step's line, so each field has a fixed form, and a value out of
form is replaced or dropped rather than shown. A receipt is not among them: it is printed nowhere,
and only goes back to the authority, which issued it.

| Field | Form | Out of form |
|---|---|---|
| outcome | accepted, refused, not connected or unknown | the reply is not understood |
| standing | sufficient or insufficient | the reply is not understood |
| reason code, floor | lower-case kebab starting with a letter, at most 48 | replaced by `unreadable-code` |
| count withheld | a whole number | the reply is not understood |
| link | one `https://` URL of printable ASCII with no space, at most 512 | dropped, not shown in part |

`verified-by: bravebot_connector::lib::a_reason_code_is_shown_only_in_the_authoritys_own_form`
`verified-by: bravebot_connector::lib::a_link_is_kept_only_when_it_is_one_https_url_on_one_line`
`verified-by: bravebot_connector::lib::a_reply_keeps_its_link_only_when_it_is_well_formed`
`verified-by: bravebot_agent::manifest::a_fetch_the_authority_cannot_vouch_for_fails_its_step`
`verified-by: bravebot_connector::lib::what_the_service_sent_carries_the_capabilitys_label_and_the_authoritys_fields_do_not`

<a id="CONNECTOR-8"></a>
### CONNECTOR-8: a reply not understood is a failure that quotes nothing

A reply in another protocol version, one over 16 MiB, and one that does not parse are each a
failure saying whether the call happened is not known. A reply that does not parse is reported by
where it stopped parsing, never by the value it could not read.

**Why.** That value can be text the service wrote, and the failure is printed.

`verified-by: bravebot_connector::lib::a_reply_in_another_protocol_is_not_understood`
`verified-by: bravebot_connector::lib::a_reply_over_the_bound_is_not_read`
`verified-by: bravebot_connector::lib::a_reply_that_does_not_parse_is_reported_without_quoting_it`

<a id="CONNECTOR-9"></a>
### CONNECTOR-9: a call ends when the run is stopped or its bound passes

A run already stopped starts no authority, and says nothing was sent. A call still waiting on the
authority ends when the run is cancelled, or when its bound passes, ten minutes unless the caller
sets another. The authority is stopped and reaped either way, and the failure says whether the call
happened is not known. An authority that answered and then does not exit is stopped at the same
bound, and its reply is kept.

Once the authority is started, its reply is read whatever became of the request, since it may
answer, and act, without reading all of it. Once the authority itself has exited, what it wrote is
its reply, though a program it started still holds its output open. A started authority whose reply
cannot be read is reported as not known, never as unreachable.

**Why.** An act may already have reached the service when the wait ends, so it is never reported as
not having happened, and a hung authority never holds the run.

`verified-by: bravebot_connector::lib::a_cancelled_run_stops_waiting_on_a_silent_authority`
`verified-by: bravebot_connector::lib::an_authority_that_never_answers_is_given_up_on`
`verified-by: bravebot_connector::lib::a_run_stopped_before_the_call_starts_no_authority`
`verified-by: bravebot_connector::lib::an_authority_that_answers_and_does_not_exit_is_stopped_at_the_bound`
`verified-by: bravebot_connector::lib::a_reply_is_read_though_the_authority_did_not_read_the_whole_request`
`verified-by: bravebot_connector::lib::a_started_authority_whose_reply_is_lost_is_not_said_to_have_done_nothing`
`verified-by: bravebot_connector::lib::a_reply_is_kept_though_a_program_the_authority_started_holds_its_output`
`verified-by: bravebot_agent::manifest::a_run_stopped_while_a_fetch_waits_stops_waiting`

<a id="CONNECTOR-10"></a>
### CONNECTOR-10: a definition is read from the person's own directory, writable by them alone

A definition is one file under `connectors/` in the person's state directory, and no settings layer
a checkout may carry can add one. The file and that folder must each be the person's own, owned by
whoever owns the state directory, writable by nobody else and not a link. One that is not, or whose
owner or mode cannot be read, is refused, and so is every definition on a platform with no such
mode to read. The authority it names must exist, and the program and its folder, followed through
any link, and the folder the definition names it in, must each be owned by the person or by the
system and writable by nobody else.

**Why.** A definition names the program a run hands its text to. Anyone who could write one could
choose that program.

`verified-by: bravebot_config::connectors::a_definition_others_could_have_written_is_refused`
`verified-by: bravebot_config::connectors::a_definition_that_is_a_dangling_link_is_refused`
`verified-by: bravebot_config::connectors::a_file_or_owner_that_cannot_be_read_is_open_to_others`
`verified-by: bravebot_config::connectors::an_authority_others_could_replace_is_refused`
`verified-by: bravebot_config::connectors::an_authority_the_system_owns_is_accepted`
`verified-by: bravebot_config::connectors::a_definition_that_is_a_link_is_refused_though_it_points_at_one_that_reads`
`verified-by: bravebot_config::connectors::only_the_persons_own_file_writable_by_them_alone_is_closed_to_others`
`verified-by: bravebot_config::connectors::no_connectors_directory_means_nothing_installed`

<a id="CONNECTOR-11"></a>
### CONNECTOR-11: a definition holds its five fields and nothing else

A definition holds a `name`, a `definition_id`, an `authority`, an `item` and a `surface`. A file
holding any other field, at the top or anywhere in its surface, is refused rather than read in
part.

**Why.** A field this process does not read is one it cannot be shown to ignore safely, and a
credential copied into the file by mistake is refused rather than kept beside a run.

`verified-by: bravebot_config::connectors::a_definition_carrying_a_field_beside_its_five_is_refused`
`verified-by: bravebot_config::connectors::a_surface_with_a_key_nothing_knows_is_refused`

<a id="CONNECTOR-12"></a>
### CONNECTOR-12: a definition is known by its file name, and spelled as the authority spells it

A connector's name is its file's name, and a file naming itself otherwise is refused. A name is
lower-case letters, digits, `_` and `-`, and its credential item is `svc:<name>:<account>`. The
authority is an absolute path.

**Why.** A grant is written against the name, so the name has to be one the person chose by
installing the file.

`verified-by: bravebot_config::connectors::a_definition_is_read_by_its_file_name`
`verified-by: bravebot_config::connectors::a_definition_naming_itself_otherwise_or_a_relative_authority_is_kept_as_an_error`
`verified-by: bravebot_config::connectors::a_surface_or_name_the_authority_would_refuse_is_refused`

<a id="CONNECTOR-13"></a>
### CONNECTOR-13: a definition carries the id the authority checks

A definition without a `definition_id`, or with one that is not 64 lower-case hexadecimal
characters, is refused with a reason that says to reinstall it. This process checks the id's form
and sends it on every call, and never computes it.

**Why.** The authority refuses a call made against a copy of a definition other than the one it
holds, which only works if every call names the copy it was planned against.

`verified-by: bravebot_config::connectors::a_definition_without_a_well_formed_definition_id_is_refused`

<a id="CONNECTOR-14"></a>
### CONNECTOR-14: a refused definition is kept as its reason

A definition that cannot be read is kept, by its name, as the reason it was refused, rather than
dropped, so a plan naming that connector is refused with the reason and a plan naming another is
not affected.

`verified-by: bravebot_config::connectors::a_definition_naming_itself_otherwise_or_a_relative_authority_is_kept_as_an_error`
`verified-by: bravebot_config::connectors::a_definition_naming_a_form_nothing_defines_is_kept_as_an_error`

<a id="CONNECTOR-15"></a>
### CONNECTOR-15: a value's form is one of a closed set

A surface may require a field's value to have a named form, and the names are a closed set every
definition shares. A definition naming any other form is refused. A value with a space or a control
character has no form.

| Form | A value of it |
|---|---|
| `owner/name`, `relative-path`, `branch` | slash-separated segments of letters, digits, `.`, `_` and `-`, none empty, `.` or `..`; a branch is one git reads as a new branch and nothing else, with no segment ending in `.lock` |
| `positive-integer`, `opaque-id`, `service-id`, `hex-id` | a number with no leading zero; an id of letters, digits, `_` and `-`; one path segment; 7 to 64 hexadecimal digits |
| `address`, `addresses` | one recipient at a real domain, or up to ten distinct ones |
| `date`, `datetime`, `date-dmy`, `local-datetime`, `time-zone` | a day that exists; an instant with its offset; a wall-clock time read in a named IANA zone |
| `iata`, `country-code`, `e164`, `personal-name` | three or two upper-case letters; a phone number; one word of a name |

**Why.** The authority checks the same forms before it puts a value into a call, so a form both
sides know is one a value cannot slip between.

`verified-by: bravebot_config::connectors::a_definition_naming_a_form_nothing_defines_is_kept_as_an_error`
`verified-by: bravebot_config::connectors::forms::a_form_nothing_defines_is_refused_after_the_value_is_held_to_no_space`
`verified-by: bravebot_config::connectors::forms::a_branch_is_one_git_would_read_as_a_new_branch_and_nothing_else`
`verified-by: bravebot_config::connectors::forms::an_address_is_one_recipient_and_a_real_domain`
`verified-by: bravebot_config::connectors::forms::a_date_and_a_datetime_are_real_instants_with_their_zone`
`verified-by: bravebot_config::connectors::forms::a_local_datetime_and_a_time_zone`
`verified-by: bravebot_config::connectors::forms::a_personal_name_and_an_e164_number`
`verified-by: bravebot_config::connectors::forms::an_iata_code_and_a_day_month_year_date`
`verified-by: bravebot_config::connectors::forms::a_service_id_is_one_path_segment`
`verified-by: bravebot_config::connectors::forms::an_owner_name_is_two_plain_segments`
`verified-by: bravebot_config::connectors::forms::a_relative_path_never_climbs_out`
`verified-by: bravebot_config::connectors::forms::a_number_an_id_and_a_hash_are_one_segment_each`
`verified-by: bravebot_config::connectors::forms::addresses_are_up_to_ten_distinct_recipients`

<a id="CONNECTOR-16"></a>
### CONNECTOR-16: a surface the authority would refuse is refused here

A surface is held to the rules the authority holds it to when it loads the definition, so no plan
is made against one the authority would refuse:

- a fetch that has to narrow takes a selector, and narrows only by terms that selector offers;
- a resource declares its `with` values exactly when it takes `with`, none can be nothing, and none
  is named for a field a call already fills;
- only a fetch with a receipt records fields, and only an act takes them, among its own `with`
  values;
- a resource taking an option lists its values, and narrows by option only for a value it offers;
- only an act is high-impact, takes no text, narrows by option or binds values from an item, and
  only one that takes `on` binds them;
- `with` is never half of an either-or, and a default names one field.

`verified-by: bravebot_config::connectors::a_fetch_narrows_only_by_terms_its_own_select_offers`
`verified-by: bravebot_config::connectors::with_values_are_declared_with_a_form_exactly_when_a_resource_uses_with`
`verified-by: bravebot_config::connectors::a_surface_or_name_the_authority_would_refuse_is_refused`

<a id="CONNECTOR-17"></a>
### CONNECTOR-17: a plan's connector steps are held to their definitions before anybody is asked

Each connector step in a frozen plan is held to the definition it names before the plan is put to
anybody: a connector that is installed and readable, a resource it offers for that step, only the
fields that resource takes and exactly one of each either-or, each value in its form and among the
values listed for its field, selector terms the resource offers, and one of its narrowing terms
where it lists any. A field the definition gives a default for is filled before the plan is locked,
unless the plan gave it in any shape or it is half of an either-or whose other half was given, so
the line a person approves holds the value that will be sent.

**Why.** The authority holds every request to the same rules, so a plan that breaks one would only
fail at its step, after earlier steps had run. Here it fails before anything has.

`verified-by: bravebot_agent::connectors::a_step_is_held_to_its_resource_fields_values_and_selector_terms`
`verified-by: bravebot_core::selector::a_malformed_term_is_refused`
`verified-by: bravebot_core::selector::a_pattern_matches_its_key_and_its_value_or_the_rest_of_it`
`verified-by: bravebot_agent::connectors::a_fetch_names_a_place_only_when_its_resource_uses_one`
`verified-by: bravebot_agent::connectors::a_value_outside_its_named_form_is_refused_before_anything_is_asked`
`verified-by: bravebot_agent::connectors::mail_reads_take_a_selector_or_an_opaque_id_and_nothing_else`
`verified-by: bravebot_agent::connectors::a_resource_that_lists_its_own_values_takes_only_those`
`verified-by: bravebot_agent::connectors::a_fetch_whose_surface_lists_narrow_terms_uses_one_in_its_select`
`verified-by: bravebot_agent::connectors::a_default_is_not_filled_beside_the_other_half_of_an_either_or`
`verified-by: bravebot_agent::connectors::a_default_never_replaces_a_value_the_plan_gave_in_another_shape`
`verified-by: bravebot_agent::connectors::a_connector_that_is_not_installed_or_could_not_be_read_refuses_the_plan`
`verified-by: bravebot_agent::manifest::a_plan_that_breaks_a_connector_surface_is_refused_before_anybody_is_asked`

<a id="CONNECTOR-18"></a>
### CONNECTOR-18: the planner is told what a definition offers, and only that

Where a connector is usable, the planner is told each one's fetch resources in the definition's own
words, with the fields, selector terms, narrowing terms and listed values each takes, and today's
date and the machine's time zone where it names one. The goal call is told the same resources in
the definition's summaries and no step or field name. Where none is usable, no connector step is
offered at all. Both calls are told which definitions are installed but refused, with why, whether
or not another is usable.

`verified-by: bravebot_agent::connectors::the_catalogue_offers_each_fetch_and_no_act`
`verified-by: bravebot_agent::connectors::the_goal_call_is_told_each_connector_and_what_it_fetches_in_plain_words`
`verified-by: bravebot_agent::connectors::the_planner_is_told_a_value_with_a_space_is_quoted_and_such_a_term_parses`
`verified-by: bravebot_agent::manifest::the_catalogue_offers_no_connector_step_where_none_is_installed`
`verified-by: bravebot_agent::manifest::a_refused_definition_offers_no_step_and_is_named_with_its_reason`

<a id="CONNECTOR-19"></a>
### CONNECTOR-19: a grant is for one connector

A run holds the connector capability for each connector its frozen plan names and for no other, and
a grant for one connector is no grant for another. What a connector returns is untrusted and
private. The call to the service is the authority's, made from its own process, so no request to
the service passes this process's network path.

`verified-by: bravebot_core::capability::a_connector_is_an_untrusted_private_effect_that_this_process_sends_nowhere`
`verified-by: bravebot_agent::manifest::a_connector_fetch_fills_one_slot_that_no_planner_reads`
`verified-by: bravebot_agent::manifest::a_run_is_granted_only_the_connectors_its_plan_names`
