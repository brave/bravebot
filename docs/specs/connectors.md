---
id: CONNECTOR
title: Connector authority client
status: normative
governs:
  - crates/connector/Cargo.toml
  - crates/connector/src/lib.rs
documented-by: none (gap: a page is owed once a manifest run calls a connector; until then nothing a person runs reaches this client)
---

## Scope

The client half of a connector call. A credential authority, a separate program the person
installed, holds every credential a connector uses and makes every call to the service. This
client starts it, hands it one request, and reads one reply. What a definition is, how a plan names
a connector step, and what the authority enforces on its own side are
[../design/connectors.md](../design/connectors.md) and
[../design/credential-brokering.md](../design/credential-brokering.md).

Nothing in a run calls this client yet. These clauses are what it owes the driver that will.

## Clauses

<a id="CONNECTOR-1"></a>
### CONNECTOR-1: the client holds no credential and names no service

A request names the connector, the authority's definition, the credential item, the resource, the
step and the routing the plan locked, and the reply's fields are the authority's own protocol.
Nothing in either is a token or a key, and nothing selects a service: the authority's definition
for the named connector decides what the call is.

**Why.** A credential this process never holds is one no prompt injection can make it send, and a
client that knows no service is one a new connector needs no code in.

`verified-by: by-construction (the crate depends on bravebot-core and bravebot-sandbox alone, reads no file, passes the authority only the variables CONNECTOR-4 names, and its request type has no field that can carry a secret the authority holds; the authority's definition, not this crate, names the service)`

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
### CONNECTOR-4: the authority starts with a cleared environment

The authority is started with only `HOME`, `XDG_RUNTIME_DIR`, `PATH`, `TMPDIR`, `LANG` and
`DBUS_SESSION_BUS_ADDRESS`, each where this process has it set, and with nothing else of this
process's environment.

**Why.** The rest of the environment may hold this process's own credentials, which are not the
authority's to read.

`verified-by: bravebot_connector::lib::the_authority_starts_with_only_the_variables_it_is_passed`

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
form is replaced or dropped rather than shown:

| Field | Form | Out of form |
|---|---|---|
| outcome | accepted, refused, not connected or unknown | the reply is not understood |
| standing | sufficient or insufficient | the reply is not understood |
| reason code, floor | lower-case kebab starting with a letter, at most 48 | replaced by `unreadable-code` |
| count withheld | a whole number | the reply is not understood |
| link | one `https://` URL of printable ASCII with no space, at most 512 | dropped, not shown in part |

`verified-by: bravebot_connector::lib::a_reason_code_is_shown_only_in_the_authoritys_own_form`
`verified-by: bravebot_connector::lib::a_link_is_kept_only_when_it_is_one_https_url_on_one_line`
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

A call still waiting on the authority ends when the run is cancelled, or when its bound passes,
ten minutes unless the caller sets another. The authority is stopped and reaped either way, and the
failure says whether the call happened is not known. An authority that answered and then does not
exit is stopped at the same bound.

**Why.** An act may already have reached the service when the wait ends, so it is never reported as
not having happened, and a hung authority never holds the run.

`verified-by: bravebot_connector::lib::a_cancelled_run_stops_waiting_on_a_silent_authority`
`verified-by: bravebot_connector::lib::an_authority_that_never_answers_is_given_up_on`
