---
id: TRIGGER
title: Event-triggered runs
status: normative
governs:
  - crates/cli/src/trigger.rs
  - crates/schedule/src/firing.rs
  - crates/signing/src/webhook.rs
documented-by:
  - docs/website/docs/reference/cli.md
  - docs/website/docs/using/headless.md
---

## Scope

`bravebot trigger`: a prompt a person registered under a name and an event name, and a listener on
the loopback address that starts that prompt as a one-shot run when a delivery signed with the
trigger's secret arrives. What a trigger holds, how a delivery is checked, what the listener
answers, what a started run is given, and how many runs a trigger may have.

A run a delivery starts is the one-shot run of [cli.md](cli.md), started by a program that is not
bravebot's terminal, and everything that file says about a one-shot run holds for it. It is held to
the same limits as a stored schedule's run ([schedule.md](schedule.md)): nobody is present, so it
stops at its first question and keeps it ([SCHEDULE-2](schedule.md#SCHEDULE-2)). The entry file
format is the trigger's own; the question a run stopped at is kept, shown and cleared by the code
`schedule` uses (`crates/schedule/src/firing.rs`), under `triggers/` instead of `schedules/`.

What this spec settles that the first proposal left open:

- The trigger a delivery is for is the path of the request, `/<name>`. Nothing in the body or in
  a header selects it ([TRIGGER-10](#TRIGGER-10)).
- The event is the one registered with the trigger. It is part of the message that is signed, so a
  sender who does not hold the secret cannot make a delivery verify for it, and no header that names
  an event is read ([TRIGGER-3](#TRIGGER-3)). GitHub's `X-Hub-Signature-256` signs the body alone
  and is not this scheme, so a GitHub webhook is not delivered to this listener directly.
- The secret is a file of its own in the state directory, private to the account
  ([TRIGGER-4](#TRIGGER-4)). `keys.rs` is the gateway keys' file and is not reused.
- The body is capped at 1 MiB and refused whole, not truncated ([TRIGGER-7](#TRIGGER-7)).
- The caps are one run at a time and six starts in an hour, each adjustable to a stated maximum
  ([TRIGGER-8](#TRIGGER-8)).
- No filter on a payload is offered ([TRIGGER-9](#TRIGGER-9)).

Not offered, because the peers' feature is not requested here: several triggers combined by OR,
native sources for chat and issue trackers, a URL that starts a run without a secret, truncating a
body to a length and running on the rest, and secrets from a software development kit for an
untrusted pull request.

Not where the state directory is or how its files are protected, which is
[state-directory.md](state-directory.md). Not how a delivery's body is labelled, which is
[labels.md](labels.md).

## Clauses

<a id="TRIGGER-1"></a>
### TRIGGER-1: `trigger` has six subcommands and starts no session

`add`, `list`, `remove`, `pending`, `clear` and `serve`. A line that names none of them, names one
with the wrong words, repeats an option or gives an option that selects on the content of a delivery
is refused with the status for an argument and a usage line, and nothing is stored or started. The
command opens no session and takes none of the flags that shape one, as the other commands that
start no session do.

**Why.** An option read as something near what was meant would store a trigger nobody asked for, and
the trigger keeps starting runs after the terminal it was typed in is closed.

`verified-by: bravebot_cli::trigger::add_takes_the_prompt_after_double_dash_and_no_payload_filter`
`verified-by: bravebot_cli::running::trigger_add_refusals_store_nothing`

<a id="TRIGGER-2"></a>
### TRIGGER-2: `add` checks everything before it stores anything and `list` shows what a run can do

A name of lowercase letters, digits and hyphens that does not start with a hyphen and is at most 64
bytes. An event of letters, digits and `. _ : -`, at most 64 bytes. Limits that are whole numbers in
their range. A prompt that is not blank and not one word starting with a hyphen. A `--workspace`
that is a directory, which is the current directory where it is not given. A state directory that
can be written. Each refusal has the status for an argument, except the missing state directory,
which has the one for configuration, and nothing is stored. A name in use is refused and the stored
trigger and secret are left as they were. `list` prints one row per stored trigger in name order:
the name, the event, the directory, whether a run can write (`reads only` or `can write`) and the
prompt shortened; `--json` prints one object per line with `can_write` as a boolean. With nothing
stored it says so and succeeds. `remove` deletes the entry, the secret and what was kept beside
them, and refuses a name nothing is stored under.

**Why.** The limit a run works under is fixed when the trigger is written, and a person reviewing
what starts unattended needs it beside the name. A name becomes a file name and a path.

`verified-by: bravebot_cli::trigger::events_and_limits_are_what_add_takes`
`verified-by: bravebot_cli::trigger::an_entry_round_trips_and_an_edited_file_is_not_run`
`verified-by: bravebot_cli::running::trigger_add_prints_the_secret_once_and_list_and_remove_keep_to_it`
`verified-by: bravebot_cli::running::trigger_list_states_per_trigger_whether_a_run_can_write`

<a id="TRIGGER-3"></a>
### TRIGGER-3: a delivery carries `sha256=<hex>` of an HMAC over the event, a line feed and the body

The sender computes `hmac_sha256(secret, event + "\n" + body)` and sends it as
`sha256=<lowercase hex>` in the header `X-Bravebot-Signature`. The event is the one registered with
the trigger. The comparison takes the same time wherever the two differ. A delivery signed for
another event, with another trigger's secret, with one byte of the body changed, or with a value
that is not `sha256=` and an even number of hex digits does not verify.

**Why.** The event is signed so that it is chosen by the person who registered the trigger and
cannot be named by a sender. Comparing a header the sender wrote with the registered event would
be a branch on bytes nobody vouched for ([TRIGGER-10](#TRIGGER-10)).

`verified-by: bravebot_signing::webhook::hmac_matches_the_rfc_4231_vector`
`verified-by: bravebot_signing::webhook::the_signature_covers_the_event_a_line_feed_and_the_body`
`verified-by: bravebot_signing::webhook::a_signature_verifies_only_for_what_it_signed`
`verified-by: bravebot_cli::trigger::the_path_selects_the_trigger_and_no_header_or_body_does`

<a id="TRIGGER-4"></a>
### TRIGGER-4: the secret is made at `add`, shown once and kept in a private file

`add` generates a random secret, prints it once with how to sign, and stores it as
`<state directory>/triggers/<name>.secret`, readable by the account only. No other command prints it,
`list` reads a file that does not hold it, and it is in no record. A secret that cannot be written
leaves no entry, so there is never a trigger nothing can sign for. `remove` deletes the file.

**Why.** A secret shown again would be one more place it could be read from. A trigger without a
secret would be one a delivery could not reach, or one that skipped the check.

`verified-by: bravebot_cli::trigger::the_secret_is_kept_private_never_listed_and_removed_with_the_entry`
`verified-by: bravebot_cli::running::trigger_add_prints_the_secret_once_and_list_and_remove_keep_to_it`

<a id="TRIGGER-5"></a>
### TRIGGER-5: a trigger's run writes only when the bypass flag was typed at `add`

The run is `bravebot -p <prompt>` with the entry's flags, in the entry's directory. It reads and
reports. It can write only when the entry holds `--dangerously-skip-permissions`, which the person
typed beside `add` and which is the only flag an entry holds. `--accept-edits`, `--auto`,
`--trust-workspace` and `--approve-input-lines` ([CLI-36](cli.md#CLI-36)) are not stored, so a
stored run never carries `--approve-input-lines`. The flag is refused at `add` where a settings layer made the
bypass mode unreachable, with the key named, and beside every other subcommand. Whether the run
trusts its directory follows a one-shot run: only a kept answer settles it.

**Why.** Nobody is present when it runs, so the permission it has is decided where somebody is, and
it is visible in `list`. A delivery from outside cannot widen it.

`verified-by: bravebot_cli::trigger::the_run_is_the_one_shot_run_with_the_stored_flags_and_no_body`
`verified-by: bravebot_cli::running::trigger_add_refuses_the_bypass_flag_where_a_layer_made_it_unreachable`
`verified-by: bravebot_cli::running::trigger_add_refusals_store_nothing`

<a id="TRIGGER-6"></a>
### TRIGGER-6: the listener binds the loopback address and answers one of five fixed pages

`serve` binds `127.0.0.1` on the port given, or 7471, and accepts a connection from any program on
this machine. A person who wants a delivery from another machine forwards to it. Each answer is a
fixed status and a fixed page that repeats nothing from the request: 202 started, 404 not found,
413 too large, 429 busy and 500 failed. A request that is not `POST /<name>` over HTTP/1.x with one
`Content-Length` of digits, one signature header and no `Transfer-Encoding`, that names no stored
trigger, or that does not verify is the same 404. The request line and headers are read to at most 8
KiB, a connection that does not finish its request in 5 seconds is dropped, and at most 32
connections are served at once. After the answer is written the listener ends its side of the
connection and discards what the sender is still sending, up to the most a request may hold and for
at most 5 seconds, so that a request refused before its body was read still receives the answer
instead of a reset. The listener makes no request, so [network-egress.md](network-egress.md)
does not apply to it.

**Why.** A refusal that differed by cause would tell a sender which names are stored and which
check failed. A listener on a wider address would accept deliveries from the network without a
person having agreed to it. Closing a connection with request bytes unread makes the system reset
it and drop the answer the sender has not yet read, which would make a refusal look like a network
fault.

Known cost: a body over the cap is answered 413 for a name that is stored and 404 for one that is
not, before any signature is checked, so a sender can find out which names are stored. Checking the
signature first would require reading the whole body, which the cap exists to avoid.

`verified-by: bravebot_cli::trigger::a_delivery_that_does_not_verify_starts_nothing_and_every_refusal_is_one_page`
`verified-by: bravebot_cli::trigger::the_listener_binds_the_loopback_address_and_answers_over_a_socket`
`verified-by: bravebot_cli::trigger::a_refusal_decided_before_the_body_is_read_is_still_answered`
`verified-by: bravebot_cli::running::a_signed_delivery_starts_the_prompt_and_the_planner_never_sees_the_body`

<a id="TRIGGER-7"></a>
### TRIGGER-7: the body is capped, refused whole and handed to the run as piped input

The `Content-Length` is read to decide whether to take the body. A body over 1 MiB is refused with
413 and none of it is read or kept; what the sender is still sending is discarded without being
looked at once the answer is written (TRIGGER-6). Otherwise exactly that many bytes are read,
checked as a whole against the signature, and written to the run's standard input unchanged, where
the one-shot run labels them untrusted and private as it labels piped input
([CLI-3](cli.md#CLI-3), [CLI-4](cli.md#CLI-4)).
The planner is told only a fixed sentence that a delivery arrived for the trigger, and reads the
content through a processor ([processors.md](processors.md)). The body is never parsed, searched or
compared by this program.

**Why.** A body cut to a length and run on would be a different message from the one signed.

`verified-by: bravebot_cli::trigger::a_body_over_the_cap_is_refused_and_one_at_the_cap_is_taken`
`verified-by: bravebot_cli::trigger::the_body_reaches_the_run_unchanged_whatever_it_holds`
`verified-by: bravebot_cli::running::a_signed_delivery_starts_the_prompt_and_the_planner_never_sees_the_body`

<a id="TRIGGER-8"></a>
### TRIGGER-8: a trigger has a most of concurrent runs and of starts in an hour

`--max-concurrent` defaults to 1 and takes 1 to 4. `--max-per-hour` defaults to 6 and takes 1 to
120. A verified delivery that would exceed either is answered 429 and starts nothing. A run that
ends frees its running place, and a start that fails frees its running place and does not count
toward the hour. A run that started counts toward the hour whether or not it has ended. The counts
are of the trigger, not of the content of any delivery.

**Why.** A secret that leaked, or a sender that repeated, would otherwise start runs without end.

`verified-by: bravebot_cli::trigger::the_concurrent_and_hourly_limits_answer_429_and_a_finished_run_frees_its_place`
`verified-by: bravebot_cli::trigger::a_start_that_fails_answers_500_and_frees_its_place`

<a id="TRIGGER-9"></a>
### TRIGGER-9: a trigger's run stops at its first question and no filter on a payload exists

`serve` starts the run with `BRAVEBOT_TRIGGER_ENTRY` set to the trigger's name. The name counts only
where it is a valid name of a stored trigger. When the planner asks such a run a question, the run
answers none of its choices, ends the turn and records the question as [SCHEDULE-2](schedule.md#SCHEDULE-2)
says, under the trigger. `pending` prints every stored question and `clear <name>` deletes one and
takes no answer. No flag or setting selects a delivery by what its body says: no pattern, path
expression, substring or glob.

**Why.** Nobody is present to answer. A filter on the body would be a branch on untrusted bytes
([TRIGGER-10](#TRIGGER-10)).

`verified-by: bravebot_cli::trigger::the_trigger_variable_names_a_firing_only_for_a_stored_trigger`
`verified-by: bravebot_cli::trigger::add_takes_the_prompt_after_double_dash_and_no_payload_filter`
`verified-by: bravebot_cli::running::a_trigger_run_stops_at_the_first_question_and_clear_deletes_it`

<a id="TRIGGER-10"></a>
### TRIGGER-10: the listener decides on nothing the sender wrote, except which of the person's own names the path is

Decision, recorded against [the rule](../development/reviewing-for-the-rule.md).

- **The body length.** The `Content-Length` is a number the sender wrote and the listener compares
  it with the cap. The comparison is on the framing of the message, not on the content: the length
  says how many bytes to read and nothing about what they say. The same comparison is made for the
  piped input of [CLI-4](cli.md#CLI-4). The outcomes it selects between are reading the body or
  refusing it with a fixed page, and neither gives the sender influence over which trigger runs, what
  the run is told or what it may do. It is recorded here as an exception to a rule that bytes from
  outside do not decide, because the alternative is to read a body of any size.
- **The HTTP framing.** `parse_head` in `crates/cli/src/trigger.rs` refuses a request whose method
  is not `POST`, whose version is not `HTTP/1.x`, that has a `Transfer-Encoding` header, that has
  no `Content-Length` or more than one, whose `Content-Length` is not digits, that has no signature
  header or more than one, or whose path is not
  a name. Each refusal is framing decided from the request line and the headers only, never from
  the body, which has not been read when it is decided. Each is answered with the same fixed page as
  any other refusal ([TRIGGER-6](#TRIGGER-6)) and starts nothing.
- **The path.** The request path is looked up as a name in the person's own `triggers/` directory.
  The lookup is a read of a file the person wrote; its result, which entry is found, is chosen by
  the person's registration and the sender can only pick among names the person created, each of
  which the person agreed would start its own prompt. A name that does not match the name syntax is
  not looked up. This is selection by the person's own labelled value, not a branch on content.
- **The signature.** The check compares bytes the sender wrote with a value computed from the
  person's secret. Its outcome is whether the sender holds the secret, which is the authority the
  person gave. A delivery that does not verify is dropped with one page and starts nothing.
- **The event and the body.** The event is the person's, signed into the message. The body is
  carried to the run's standard input and is read by nothing here.

A change that makes a header, a path suffix or any byte of the body select a trigger, an event, a
flag, a directory or a limit breaks this clause.

`verified-by: bravebot_cli::trigger::the_path_selects_the_trigger_and_no_header_or_body_does`
`verified-by: bravebot_cli::trigger::the_body_reaches_the_run_unchanged_whatever_it_holds`
`verified-by: bravebot_cli::running::a_signed_delivery_starts_the_prompt_and_the_planner_never_sees_the_body`

<a id="TRIGGER-11"></a>
### TRIGGER-11: nothing a turn does stores or changes a trigger

There is no tool for it. The prompt, the event and the flags are fixed when the entry is written, and
nothing a delivery carried, a run printed or a model said is read by this command. A program a turn
starts is held to the places a confined program can write, which exclude the state directory, so it
can neither store a trigger nor read a secret.

**Why.** An unattended run that could add triggers could keep itself running with a prompt of its
own choosing.

Known cost: with `--sandbox off` a `run` is not held to those places and can start
`bravebot trigger add`. The person who turned the sandbox off has agreed to that.

`verified-by: bravebot_agent::confine::a_bypass_stage_still_cannot_write_a_credential_a_file_run_later_or_a_new_entry_in_home`
