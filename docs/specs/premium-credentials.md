---
id: PREM
title: The premium key
status: normative
governs:
  - crates/config/src/env_var.rs
  - crates/config/src/lib.rs
  - crates/config/src/brave_key.rs
  - crates/aichat/src/lib.rs
  - crates/agent/src/subscription.rs
  - crates/agent/src/backend.rs
  - crates/agent/src/turn.rs
  - crates/agent/src/turn/hooked.rs
  - crates/agent/src/turn/result.rs
  - crates/agent/src/turn/delegate_end.rs
  - crates/agent/src/turn/setup.rs
  - crates/agent/src/turn/round_steps.rs
  - crates/agent/src/turn/prompts.rs
  - crates/agent/src/turn/task.rs
  - crates/agent/src/turn/sentences.rs
  - crates/agent/src/delegate.rs
  - crates/agent/src/shared.rs
  - crates/cli/src/brave.rs
  - crates/cli/src/auth.rs
  - crates/cli/src/doctor.rs
  - crates/cli/src/main.rs
  - crates/tui/src/status.rs
  - crates/tui/src/logo.rs
documented-by: docs/website/docs/customize/premium.md
---

## Scope

Authorising requests to Brave's premium service with a key a person stored. This is an auth flow: it
carries no workspace content and no model output, so no labelled value passes through it. What it
does carry is a bearer key, which is why the rules below are about not sending it where it does not
belong and not leaking it.

```
bravebot auth login brave    # type the key at a hidden prompt; it is kept in ~/.bravebot/brave-key.json
bravebot auth status brave   # say whether one is stored
bravebot auth logout brave   # forget it
```

The premium host is compiled in, and `BRAVE_AI_CHAT_PREMIUM_ENDPOINT` is an optional override of it
([PREM-11](#PREM-11)). `bravebot import-leo-creds`, which earlier versions used to import a
subscription from a Brave profile, is retired ([PREM-15](#PREM-15)).

## Clauses

<a id="PREM-1"></a>
### PREM-1: withdrawn, this registers as a new device and never borrows the browser's credentials

Replaced by [PREM-10](#PREM-10). Bravebot no longer registers a device or reads a browser profile,
and a premium request is authorised by a key the person stored with `bravebot auth login brave`.

<a id="PREM-2"></a>
### PREM-2: the key is never sent to the free host or to a host that is not Brave's

The premium host and the key travel together, because a key belongs to a deployment. A build with
no premium host, which is one that reaches no Brave backend, sends no key rather than sending one
where it does not belong, and a premium host that is not one of Brave's, which only an override can
name, is sent none. A request that is not sent the key goes to the free endpoint, signed with the
services key.

A premium endpoint is one of Brave's deployments when its host ends in `.brave.com`,
`.brave.software` or `.bravesoftware.com`. The host is the one the transport connects to: the text
after the scheme up to the first `/`, `?` or `#`, with any userinfo and port left out. A Brave
suffix after a `?` or a `#` belongs to the query or the fragment, so
`https://attacker.example#.brave.com` is `attacker.example`'s and is sent no key, and a Brave name
in front of an `@` is userinfo, not the host. An override holding a query or a fragment is not used
by the configuration, which keeps the host compiled in ([PREM-11](#PREM-11)), so the string requests
are sent to and the string the decision reads are never two different hosts.

**Why.** The Brave suffix test is what keeps a key off other hosts. A settings file in a checkout
can set the premium endpoint override ([BACKEND-35](backends.md#BACKEND-35)), so the test has to hold
for every value that file can hold. A key that reaches a host other than Brave's can be read there,
and it stays usable until Brave revokes it. A host that is not Brave's is a local model server or a
private deployment somebody chose, and no key belongs near one.

`verified-by: bravebot_aichat::client::without_a_premium_host_no_key_is_attached`
`verified-by: bravebot_agent::subscription::a_key_is_presented_only_to_a_premium_host_that_is_braves`
`verified-by: bravebot_agent::subscription::braves_own_deployments_are_told_from_a_host_somebody_chose`
`verified-by: bravebot_agent::subscription::a_port_is_not_part_of_the_host`
`verified-by: bravebot_agent::subscription::a_suffix_after_a_hash_or_a_question_mark_does_not_make_a_host_braves`
`verified-by: bravebot_agent::subscription::a_brave_name_in_the_userinfo_does_not_make_a_host_braves`

<a id="PREM-3"></a>
### PREM-3: withdrawn, an order is checked before anything is registered against it

Replaced by [PREM-15](#PREM-15), which retires the command that registered against an order. No
order is read or registered against, so nothing is checked first.

<a id="PREM-4"></a>
### PREM-4: withdrawn, a batch is verified against the issuer's key before it is stored

Replaced by [PREM-10](#PREM-10). No batch is minted or stored, so none is verified here, and the
premium host checks the key it is sent.

<a id="PREM-5"></a>
### PREM-5: withdrawn, a credential is single-use and is never offered twice

Replaced by [PREM-12](#PREM-12). The key is not single-use: it is presented with every premium
request, and what is counted is the number of times it is asked for.

<a id="PREM-6"></a>
### PREM-6: withdrawn, a spend is on the file before the credential is presented, and nothing else writes

Replaced by [PREM-12](#PREM-12). Nothing is spent from a file, so nothing is written before a key is
presented. The one file for the key is written only by `bravebot auth login brave`
([PREM-14](#PREM-14), [PREM-16](#PREM-16)).

<a id="PREM-7"></a>
### PREM-7: withdrawn, credentials live in one file under `~/.bravebot` that no other account can read

Replaced by [PREM-14](#PREM-14), which says where the key is held: one file under `~/.bravebot` that
no other account can read.

<a id="PREM-8"></a>
### PREM-8: withdrawn, a stored subscription that cannot be used is reported rather than skipped

Replaced by [PREM-10](#PREM-10), [PREM-13](#PREM-13) and [PREM-16](#PREM-16). There is no stored
subscription to be unusable. A source that cannot present the key fails the request with the remedy
named, a file of the key that cannot be read is said by `doctor`, and a configuration naming no model
service says what to configure ([BACKEND-39](backends.md#BACKEND-39)).

<a id="PREM-9"></a>
### PREM-9: the tier reported is the one the last turn ran on, not the one the build was compiled with

What `/status` says about the tier is what the last turn actually did: that the premium key was sent
or that it was not. Before the first turn it says premium is available, and that nothing has been
sent yet, where a key is stored, and that no premium key is configured where none is. It does not
claim the tier is or is not in use.

The opening screen draws the tier beside the confinement, from the configuration, in the same words
`/status` uses before a turn has run. It reads whether the configuration holds a key and sends
nothing: a key that is stored may still be refused by the premium host, so holding one does not
settle the tier either. A pane too narrow for the wordmark still reports both.

Where the server reports using a model other than the one requested, both are shown: the choice that
was made and the model that actually answered. Said once when it starts happening rather than every
turn. `automatic-bravebot` resolving to a concrete model is not a substitution, since that is the
server choosing per request, which is what the automatic entry means.

**Why.** A configuration that holds a key would otherwise report itself as premium, which is a fact
about the configuration and not about any request. A session reported "premium configured" while ten
consecutive requests went out without the key and were answered by a model a third the size, which
then announced tool calls it never emitted and stalled the turn. A panel that cannot be trusted on
this point is worse than one that omits it, and the substituted model is what a person is most
likely to notice first when a request did not carry the key.

`verified-by: bravebot_tui::status::the_tier_reported_is_the_one_the_last_turn_actually_ran_on`
`verified-by: bravebot_tui::status::a_session_with_no_turn_yet_does_not_claim_a_tier`
`verified-by: bravebot_tui::status::the_opening_line_and_the_panel_say_the_same_thing_before_a_turn_runs`
`verified-by: bravebot_tui::logo::the_mark_names_the_agent_its_confinement_and_its_tier`
`verified-by: bravebot_tui::logo::a_narrow_pane_still_reports_the_confinement_and_the_tier`
`verified-by: bravebot_tui::status::a_substituted_model_is_reported_beside_the_one_asked_for`
`verified-by: bravebot_tui::status::automatic_being_resolved_to_a_real_model_is_not_a_substitution`
`verified-by: bravebot_tui::status::picking_a_model_after_a_turn_reports_the_turn_that_ran`
`verified-by: bravebot_tui::state::picking_automatic_and_being_answered_by_a_model_is_not_a_substitution`
`verified-by: bravebot_tui::status::the_opening_screen_draws_the_tier_the_configuration_settles`
`verified-by: bravebot_tui::app::a_model_the_configuration_named_is_reported_as_substituted_too`
`verified-by: bravebot_tui::app::a_substitution_that_carries_on_is_said_once`
`verified-by: bravebot_tui::app::the_configured_automatic_entry_resolving_to_a_model_says_nothing`
`verified-by: bravebot_tui::app::a_turn_on_a_definitions_model_records_its_tier_and_model`
`verified-by: bravebot_tui::app::a_turn_on_a_skills_model_replaces_an_earlier_turns_tier`

<a id="PREM-10"></a>
### PREM-10: the key is presented as an unsigned bearer token to the premium host, and a redirect from it is a failure

A request to the Brave backend made while a key is stored goes to the premium host's
`/v1/chat/completions` with `Authorization: Bearer <key>`. The premium host authorises on the key
alone, so that request is not signed: it carries no `digest` and no `Signature` authorization, and
the key is not sent in a cookie. A request made without the key goes to the free endpoint, signed
with the services key.

A redirect in answer to a premium request is a failure, with the status it came with, and is not
followed: nothing is sent to the address it names.

A source that cannot present the key fails the request rather than sending it without. The failure
reads "the premium key could not be used", gives the reason, and says to run
`bravebot auth login brave` to replace the key or `bravebot auth logout brave` to send requests to
the free endpoint instead. It does not mention an AWS sign-in.

**Why.** The free endpoint answers a premium model name by substituting a weaker model rather than
by failing, with a 200 and an ordinary reply. A request that silently lost its key would therefore
still return something that reads like an answer, and the only other symptom of the downgrade is the
agent appearing to get worse for no reason. A redirect would re-send the conversation to an address
the premium host chose, and the key is a bearer secret meant for the host it was configured for.

`verified-by: bravebot_aichat::client::a_premium_request_goes_to_the_premium_host_with_the_key_as_its_bearer_token`
`verified-by: bravebot_aichat::client::an_unusable_premium_key_fails_rather_than_downgrading`
`verified-by: bravebot_aichat::client::a_redirect_from_the_premium_host_is_a_failure_and_goes_nowhere`
`verified-by: bravebot_agent::backend::each_backend_keeps_its_own_explanation`

<a id="PREM-11"></a>
### PREM-11: the premium host is compiled in, an override that is malformed is not used, and the key is held only with a host

The premium host is compiled in, so no variable or settings file decides where a key a person typed
is sent. A build whose free endpoint is under `brave.software` or `bravesoftware.com` has the
development host, and every other build has the production host. The two are one value while the
premium service is live only in development. A build that names no premium host has the one compiled
in, and a build that reaches no Brave backend has none, since it has no free endpoint to say whose
environment it is in.

`BRAVE_AI_CHAT_PREMIUM_ENDPOINT` is an optional override of the host. It is resolved by the order
every other name is ([BACKEND-35](backends.md#BACKEND-35)): an exported variable, then the build,
then a settings file's `env` block. It is a base the request path is appended to. A value that is
not an `http://` or `https://` address, or that holds a query or a fragment, is not used, and the
host compiled in stands. A port and a path are part of the base and are kept.

The configuration holds the stored key ([PREM-14](#PREM-14)) only together with a premium host. On a
build with none, a key that is stored is dropped rather than held, and a build with a host and no key
stored sends every request to the free endpoint.

**Why.** Failing every run over a badly spelled override would punish everyone who never stored a
key, and sending the key to an address nobody chose would be worse, so the override that cannot be
used gives way to the host compiled in. A transport ends the host at the first `/`, `?` or `#`, so
the text after either is not part of the host although a suffix test on the whole value reads it as
one, and a path appended to a base holding either lands inside the query or the fragment. Not using
such a value keeps the host a request is sent to and the host the decision about the key reads the
same host. A key with no premium host would have to be sent to the free endpoint or to nothing, and
the free endpoint is a deployment the key does not belong to.

`verified-by: bravebot_config::lib::a_build_that_names_no_premium_host_has_the_one_compiled_in`
`verified-by: bravebot_config::lib::a_build_that_reaches_no_brave_backend_has_no_premium_host`
`verified-by: bravebot_config::lib::a_stored_premium_key_is_held_with_the_host_compiled_in`
`verified-by: bravebot_config::lib::a_stored_premium_key_is_dropped_where_there_is_no_brave_backend`
`verified-by: bravebot_config::lib::a_malformed_premium_endpoint_is_not_used`
`verified-by: bravebot_config::lib::a_premium_endpoint_with_a_query_or_a_fragment_is_discarded`
`verified-by: bravebot_config::lib::a_premium_endpoint_with_a_port_and_a_path_is_kept`
`verified-by: bravebot_config::lib::every_name_a_settings_block_may_set_reaches_the_configuration`
`verified-by: bravebot_agent::subscription::a_key_is_presented_only_to_a_premium_host_that_is_braves`

<a id="PREM-12"></a>
### PREM-12: every premium request asks the key source once, so a limit in credits counts premium requests and a delegate's are counted with its turn's

A premium request asks its key source for the key once, as it is made. A request that does not go to
the premium host asks it nothing. Asking does not use the key up: the source gives the same key on
every ask, so no request finds it spent.

A session's `limit` in credits ([TURN-8](turns.md#TURN-8)) counts these asks, so it counts premium
requests. A turn lends its source to the delegates it starts. A delegate presents the key through
the source it was lent and takes none from the configuration, so its requests are counted with the
turn's.

**Why.** A person who sets a limit in credits is limiting premium requests, and the source is where
they are counted. A delegate that took the key from the configuration itself would present it
without being counted, and the limit would be passed without anyone being asked.

`verified-by: bravebot_agent::subscription::the_key_is_presented_on_every_request`
`verified-by: bravebot_agent::shared::two_runs_holding_one_source_both_ask_it`
`verified-by: bravebot_agent::turn::a_delegate_spends_the_wallet_the_turn_lent_it`
`verified-by: bravebot_aichat::client::without_a_premium_host_no_key_is_attached`

<a id="PREM-13"></a>
### PREM-13: a turn whose model another backend serves is not handed the key

Which service answers is decided by the model ([BACKEND-3](backends.md#BACKEND-3)), and only Brave's
aichat endpoint has a notion of a premium key. A turn whose model is served by AWS Bedrock, by a
gateway or by a ChatGPT or xAI account is handed no key, and its client is given none to present. A
stored key with a premium host that is Brave's is a configured service for a turn whose model Brave
serves ([BACKEND-39](backends.md#BACKEND-39)).

**Why.** Such a turn ran on the model that was chosen, and the key is not one the service that
answered would accept. Handing it to that client would send a credential to a service that did not
issue it. That is the same reasoning [BACKEND-9](backends.md#BACKEND-9) applies to a sign-in: a turn
served entirely by one backend has no business acting on the credentials of another it will never
call.

`verified-by: bravebot_agent::subscription::a_turn_on_another_backend_is_not_handed_the_premium_key`
`verified-by: bravebot_agent::backend::only_the_aichat_backend_spends_an_imported_subscription`
`verified-by: bravebot_agent::backend::a_premium_key_with_a_braves_premium_host_is_a_service_configured`
`verified-by: bravebot_agent::backend::the_account_answers_for_its_own_models_and_for_nothing_a_credential_of_another_service_is_for`

<a id="PREM-14"></a>
### PREM-14: the key is one file under `~/.bravebot` that no other account can read, and the programs a turn runs are refused it

The key is kept in `brave-key.json` in the state directory ([STATE-2](state-directory.md#STATE-2)),
a file of its own beside the gateway keys and not in them. It holds a version and the key, and
nothing else. The file is created before anything is written to it, reachable by the account that
stored the key and by no other: mode 0600 on Unix and an access-control list for one account on
Windows, with nothing inherited. A platform with neither has the key refused. It is written beside
the file and renamed over it, so a write that stops part way leaves the last good file. No settings
layer, `provider` block or environment variable holds the key or names one, and where the key goes is
not stored with it.

The file is read with a bound of 16 KiB. One that has another field, another version, or a blank or
non-text key is refused whole and not read in part, because the file is written whole and a field
skipped when reading would be gone after the next write. The text it was read into and the document
the parse made are overwritten before the read answers.

The sandbox refuses the file to the programs a stage of `run` starts, as it refuses every file of the
state directory that it does not lift ([SANDBOX-12](sandboxing.md#SANDBOX-12)). No variable carries
the key, so there is none to take out of the environment of a program.

Whenever the configuration holds a key ([PREM-11](#PREM-11)) and the Brave backend is reachable, the
key is in the record of credentials this build holds ([CRED-2](credential-protection.md#CRED-2)).
`bravebot doctor` lists it as "the premium key" with its tier and what would end it: signing out with
`bravebot auth logout brave` ends this machine's custody and leaves the key working at Brave's
premium service. It never prints the value. Its `premium` line reads `<url>, with a premium key`,
`<url>, without a premium key, so requests go to the endpoint above`, or `not configured`.

**Why.** A file of its own because every id in the gateway keys is a gateway that can be signed out of
by name, and a Brave key kept there would be listed as a gateway no block names. A bearer key a
person typed is kept as the other keys are: readable by the account alone, in the directory the
sandbox already refuses to the programs a turn runs, which do not authenticate as this agent and have
no use for a key that reaches Brave's service. The key is a bearer secret that nothing decides each
use of and that only Brave's premium service revokes, which is the Held tier.

`verified-by: bravebot_config::brave_key::a_stored_key_is_read_back`
`verified-by: bravebot_config::brave_key::no_file_is_no_key_and_a_written_file_is_read`
`verified-by: bravebot_config::brave_key::a_file_that_is_not_this_shape_is_refused`
`verified-by: bravebot_config::brave_key::a_key_that_needs_escaping_round_trips`
`verified-by: bravebot_cli::brave::a_typed_key_is_stored_private_and_never_said`
`verified-by: bravebot_skus::store::the_file_is_not_readable_by_anyone_else`
`verified-by: bravebot_skus::store::the_windows_list_grants_one_account_and_inherits_nothing`
`verified-by: bravebot_sandbox::base::the_run_base_reads_the_session_records_and_no_other_file_in_the_state_directory`
`verified-by: bravebot_agent::confine::a_stage_reads_the_machine_and_is_refused_the_credential_locations`
`verified-by: bravebot_config::lib::a_premium_key_is_a_credential_this_configuration_holds`
`verified-by: bravebot_cli::running::doctor_accounts_for_the_premium_key_at_the_tier_its_walk_stopped_at`

<a id="PREM-15"></a>
### PREM-15: `import-leo-creds` is retired, says so, and starts no work

`bravebot import-leo-creds`, with any arguments and in an incognito session or not, prints that
bravebot no longer imports a Leo subscription from Brave and that premium requests use a key stored
with `bravebot auth login brave`. It exits with the argument status ([CLI-6](cli.md#CLI-6)) and
starts no work: it prints nothing on standard output and writes nothing to the state directory.

`~/.bravebot/leo-premium.json` and `leo-premium.lock`, which earlier versions wrote, are neither read
nor written. A person can delete them. The sandbox still denies the programs it confines access to
`leo-premium.json`, so a file left from an earlier version stays protected
([SANDBOX-12](sandboxing.md#SANDBOX-12)).

**Why.** Left as a word nothing recognises, the command would be taken as the task a one-shot run was
asked to do and sent to a model. A command that did nothing without saying so would leave somebody
with the old setup believing premium was still authorised by it.

`verified-by: bravebot_cli::running::the_retired_import_command_says_so_and_starts_no_work`

<a id="PREM-16"></a>
### PREM-16: `auth login brave` stores a typed key on a terminal, `auth logout brave` forgets it, and `auth status brave` says whether one is stored

`bravebot auth login brave` asks for the key at a prompt that draws nothing as it is typed, and stores
it in the file of [PREM-14](#PREM-14). It needs a terminal on stdin and on stderr: where either is
not one it is refused with the argument status, and nothing is stored. In an incognito session it is
refused with the failure status before the key is asked for, because a premium key is kept on disk,
which an incognito session will not do ([INCOG-7](incognito.md#INCOG-7)); where both apply, the
incognito refusal is the one given. Where the machine has nowhere to keep a key, it is refused with
the failure status.

A key that is stored is replaced only on a yes. The question is asked first, anything but a yes
(no, nothing typed, the end of the input) leaves the key as it is, and the key is asked for only
after the yes. Nothing typed stores nothing and is not a failure. A file that cannot be read as a key
is neither read as none nor written over: the login is refused with the failure status, naming the
file, and leaves it. The line said after a store names the file and never the key.

`bravebot auth logout brave` removes the file. It is allowed in an incognito session, since it
leaves less behind, and is not an error where nothing is stored. It says the key still works at
Brave's premium service until it is revoked there.

`bravebot auth status brave` reads the file and writes nothing, so an incognito session may ask. It
prints one line, never the key:

- `brave: signed in: a premium key is stored and is sent to Brave's premium service alone`, with exit
  status 0.
- `brave: not signed in: no premium key is stored; run bravebot auth login brave`, or
  `brave: not signed in: this machine has nowhere to keep a key, so none is stored`, with exit status 1.
- `brave: unusable: ` and a sentence naming the file, where it cannot be read, with exit status 1.

A turn reads a file that cannot be read as no key, so its premium requests go to the free endpoint.
`bravebot doctor` says so on a `premium key` line that names the file and ends in failure.

**Why.** The key is typed at a hidden prompt rather than passed as an argument or read from a
variable, because an argument is readable by other programs on the machine and kept in the shell's
history, and a variable is inherited by every program started. A key asked for on a redirected stream
is one nobody typed, and a pipe on stdin would be answering for the person, so a terminal is
required. A key is replaced only on a yes because the one replaced is gone once the file is written.
A file that cannot be read is left because writing over it would lose a key the reader could not
parse, which is a key somebody has to go back to Brave for. A turn does not fail on it, since a run
that never used the premium tier has no use for the file, and `doctor` is where it is said.

`verified-by: bravebot_cli::brave::a_typed_key_is_stored_private_and_never_said`
`verified-by: bravebot_cli::brave::no_key_typed_stores_nothing`
`verified-by: bravebot_cli::brave::a_stored_key_is_replaced_only_on_a_yes`
`verified-by: bravebot_cli::brave::an_unreadable_file_is_refused_and_left`
`verified-by: bravebot_cli::brave::the_sign_in_is_refused_where_a_key_cannot_be_asked_for_or_kept`
`verified-by: bravebot_cli::brave::forgetting_removes_the_file_and_is_repeatable`
`verified-by: bravebot_cli::brave::the_standing_follows_the_file`
`verified-by: bravebot_cli::running::auth_login_brave_is_refused_where_a_key_cannot_be_typed_or_kept`
`verified-by: bravebot_cli::running::auth_logout_brave_forgets_the_key_and_only_that_one`
`verified-by: bravebot_cli::running::auth_status_brave_with_no_key_is_not_signed_in_and_exits_nonzero`
`verified-by: bravebot_cli::running::auth_status_brave_reports_a_stored_key_without_the_key`
`verified-by: bravebot_cli::running::doctor_accounts_for_the_premium_key_at_the_tier_its_walk_stopped_at`

## Requirements and limits

- The build must reach a Brave backend, which gives it the premium host compiled in
  ([PREM-11](#PREM-11)), and a key must be stored with `bravebot auth login brave`. Without both,
  premium is unavailable and requests go to the free endpoint.
- Storing a key needs a terminal and nothing else: no browser, no Brave profile and no desktop
  session. A platform that has neither a file mode nor an access-control list to restrict the file
  to the account has the key refused ([PREM-14](#PREM-14)).
- The key is a bearer secret in a file the account owns. It is not encrypted at rest, and anything
  running as the person can read it, as it can read the gateway keys.
- The key is presented only to a premium host that is Brave's ([PREM-2](#PREM-2)).
- `bravebot doctor` reports whether a key is held, on the `premium` line and among the credentials
  held, never prints the value, and says so when the file cannot be read ([PREM-16](#PREM-16)).
