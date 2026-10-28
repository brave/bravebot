---
id: PREM
title: The premium key
status: normative
governs:
  - crates/config/src/env_var.rs
  - crates/config/src/lib.rs
  - crates/config/src/scrub.rs
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
  - crates/cli/src/doctor.rs
  - crates/cli/src/main.rs
  - crates/tui/src/status.rs
  - crates/tui/src/logo.rs
documented-by: docs/website/docs/customize/premium.md
---

## Scope

Authorising requests to Brave's premium service with a key held in the build or the environment.
This is an auth flow: it carries no workspace content and no model output, so no labelled value
passes through it. What it does carry is a bearer key, which is why the rules below are about not
sending it where it does not belong and not leaking it.

```
BRAVE_AI_CHAT_PREMIUM_ENDPOINT   # the premium host
BRAVE_AI_CHAT_PREMIUM_KEY        # the key that host authorises a request with
```

Both are optional, and premium is used only when both are held. `bravebot import-leo-creds`, which
earlier versions used to import a subscription from a Brave profile, is retired ([PREM-15](#PREM-15)).

## Clauses

<a id="PREM-1"></a>
### PREM-1: withdrawn, this registers as a new device and never borrows the browser's credentials

Replaced by [PREM-10](#PREM-10). Bravebot no longer registers a device or reads a browser profile,
and a premium request is authorised by a key held in the build or the environment.

<a id="PREM-2"></a>
### PREM-2: the key is never sent to the free host or to a host that is not Brave's

The premium host and the key travel together, because a key belongs to a deployment. A build with
no premium host sends no key rather than sending one where it does not belong, and a premium host
that is not one of Brave's is sent none. A request that is not sent the key goes to the free
endpoint, signed with the services key.

A premium endpoint is one of Brave's deployments when its host ends in `.brave.com`,
`.brave.software` or `.bravesoftware.com`. The host is the one the transport connects to: the text
after the scheme up to the first `/`, `?` or `#`, with any userinfo and port left out. A Brave
suffix after a `?` or a `#` belongs to the query or the fragment, so
`https://attacker.example#.brave.com` is `attacker.example`'s and is sent no key, and a Brave name
in front of an `@` is userinfo, not the host. A premium endpoint holding a query or a fragment is
also dropped by the configuration ([PREM-11](#PREM-11)), so the string requests are sent to and the
string the decision reads are never two different hosts.

**Why.** The Brave suffix test is what keeps a key off other hosts. On a build with nothing built
in, a settings file in a checkout sets the premium endpoint ([BACKEND-35](backends.md#BACKEND-35)),
so the test has to hold for every value that file can hold. A key that reaches a host other than
Brave's can be read there, and it stays usable until Brave revokes it. A host that is not Brave's
is a local model server or a private deployment somebody chose, and no key belongs near one.

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
presented, and bravebot writes no premium file at all ([PREM-15](#PREM-15)).

<a id="PREM-7"></a>
### PREM-7: withdrawn, credentials live in one file under `~/.bravebot` that no other account can read

Replaced by [PREM-14](#PREM-14), which says where the key is held: in the build or the environment,
with no file of bravebot's own for it.

<a id="PREM-8"></a>
### PREM-8: withdrawn, a stored subscription that cannot be used is reported rather than skipped

Replaced by [PREM-10](#PREM-10) and [PREM-13](#PREM-13). There is no stored subscription to be
unusable. A source that cannot present the key fails the request with the remedy named, and a
configuration naming no model service says what to configure
([BACKEND-39](backends.md#BACKEND-39)).

<a id="PREM-9"></a>
### PREM-9: the tier reported is the one the last turn ran on, not the one the build was compiled with

What `/status` says about the tier is what the last turn actually did: that the premium key was sent
or that it was not. Before the first turn it says premium is available, and that nothing has been
sent yet, where a key is held, and that no premium key is configured where none is. It does not
claim the tier is or is not in use.

The opening screen draws the tier beside the confinement, from the configuration, in the same words
`/status` uses before a turn has run. It reads whether the configuration holds a key and sends
nothing: a key that is held may still be refused by the premium host, so holding one does not settle
the tier either. A pane too narrow for the wordmark still reports both.

Where the server reports using a model other than the one requested, both are shown: the choice that
was made and the model that actually answered. Said once when it starts happening rather than every
turn. `automatic-bravebot` resolving to a concrete model is not a substitution, since that is the
server choosing per request, which is what the automatic entry means.

**Why.** A build that holds a key would otherwise report itself as premium, which is a fact about
the build and not about any request. A session reported "premium configured" while ten consecutive
requests went out without the key and were answered by a model a third the size, which then
announced tool calls it never emitted and stalled the turn. A panel that cannot be trusted on this
point is worse than one that omits it, and the substituted model is what a person is most likely to
notice first when a request did not carry the key.

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

A request to the Brave backend made while the key is held goes to the premium host's
`/v1/chat/completions` with `Authorization: Bearer <key>`. The premium host authorises on the key
alone, so that request is not signed: it carries no `digest` and no `Signature` authorization, and
the key is not sent in a cookie. A request made without the key goes to the free endpoint, signed
with the services key.

A redirect in answer to a premium request is a failure, with the status it came with, and is not
followed: nothing is sent to the address it names.

A source that cannot present the key fails the request rather than sending it without. The failure
reads "the premium key could not be used", gives the reason, and says to unset
`BRAVE_AI_CHAT_PREMIUM_KEY` to send requests to the free endpoint instead. It does not mention an AWS
sign-in.

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
### PREM-11: the key is held only with a premium host, and a blank key is no key

`BRAVE_AI_CHAT_PREMIUM_KEY` is resolved by the order every other name is
([BACKEND-35](backends.md#BACKEND-35)): an exported variable, then the build, then a settings file's
`env` block. A machine-level file does not pin it ([BACKEND-38](backends.md#BACKEND-38)). The
configuration holds the key only together with a premium endpoint. With no endpoint it holds no key,
so a key exported with nowhere to go is dropped rather than held, and an endpoint with no key leaves
premium unused.

A value that is empty or only whitespace is no key, and whitespace around a value is not part of it,
so a variable exported blank does not send `Authorization: Bearer ` with nothing after it.

The premium endpoint is a base the request path is appended to. One that is not an `http://` or
`https://` address is dropped, and so is one holding a query or a fragment, since a transport ends
the host at the first `/`, `?` or `#` and a path appended to such a base lands inside the query or
the fragment. The key is held no longer once its endpoint is dropped. A port and a path are part of
the base and are kept.

**Why.** A key with no premium host would have to be sent to the free endpoint or to nothing, and
the free endpoint is a deployment the key does not belong to. A blank value is what an `.envrc` that
exports an empty variable produces, and sending it would be a premium request that cannot be
authorised. An endpoint with a query or a fragment is dropped rather than used as written, so the
host a request is sent to and the host the decision about the key reads are the same host.

`verified-by: bravebot_config::lib::a_premium_key_is_held_only_with_a_premium_host`
`verified-by: bravebot_config::lib::a_blank_premium_key_is_no_key`
`verified-by: bravebot_config::lib::a_premium_endpoint_with_a_query_or_a_fragment_is_discarded`
`verified-by: bravebot_config::lib::a_premium_endpoint_with_a_port_and_a_path_is_kept`
`verified-by: bravebot_config::lib::every_name_a_settings_block_may_set_reaches_the_configuration`

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
configuration with a key held is a configured service for a turn whose model Brave serves
([BACKEND-39](backends.md#BACKEND-39)).

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
### PREM-14: the key is in the record of credentials, and programs bravebot starts do not have it

Whenever the configuration holds a key ([PREM-11](#PREM-11)) and the Brave backend is reachable, the
key is in the record of credentials this build holds ([CRED-2](credential-protection.md#CRED-2)). `bravebot doctor` lists it as "the
premium key" with its tier and what would end it, and never prints its value. Its `premium` line
reads `<url>, with a premium key`, `<url>, without a premium key, so requests go to the endpoint
above`, or `not configured`.

`BRAVE_AI_CHAT_PREMIUM_KEY` is removed from the environment of every program bravebot starts, as the
services key and its id are ([RUN-12](tools/run.md#RUN-12)), and a settings file cannot switch that
off.

Bravebot writes no file for the key. It is held in the build, masked in the binary rather than
encrypted, or in the environment, or in a settings file's `env` block that the person wrote.

**Why.** A program the planner chose is doing whatever the person approved, and never
authenticates as this agent, so none has a use for a key that reaches Brave's service. One that was
handed it could read it or send it on. The key is a bearer secret that nothing decides each use of
and that only Brave's premium service revokes, which is the Held tier.

`verified-by: bravebot_config::lib::a_premium_key_is_a_credential_this_configuration_holds`
`verified-by: bravebot_config::scrub::this_agents_credentials_are_withheld_without_being_configured`
`verified-by: bravebot_config::scrub::the_credentials_told_to_the_planner_are_the_fixed_ones`
`verified-by: bravebot_cli::running::doctor_accounts_for_the_premium_key_at_the_tier_its_walk_stopped_at`

<a id="PREM-15"></a>
### PREM-15: `import-leo-creds` is retired, says so, and starts no work

`bravebot import-leo-creds`, with any arguments and in an incognito session or not, prints that
bravebot no longer imports a Leo subscription from Brave and that premium requests use the key in
`BRAVE_AI_CHAT_PREMIUM_KEY`, which `bravebot doctor` reports. It exits with the argument status
([CLI-6](cli.md#CLI-6)) and starts no work: it prints nothing on standard output and writes nothing to the state directory.

`~/.bravebot/leo-premium.json` and `leo-premium.lock`, which earlier versions wrote, are neither read
nor written. A person can delete them. The sandbox still denies the programs it confines access to
`leo-premium.json`, so a file left from an earlier version stays protected
([SANDBOX-12](sandboxing.md#SANDBOX-12)).

**Why.** Left as a word nothing recognises, the command would be taken as the task a one-shot run was
asked to do and sent to a model. A command that did nothing without saying so would leave somebody
with the old setup believing premium was still authorised by it.

`verified-by: bravebot_cli::running::the_retired_import_command_says_so_and_starts_no_work`

## Requirements and limits

- The build or the environment must know the premium host and a key. Without both, premium is
  unavailable and requests go to the free endpoint ([PREM-11](#PREM-11)).
- There is no platform special case. Nothing is imported, no file is read and none is written, so
  macOS, Linux and Windows are alike, and a machine with no desktop session needs nothing.
- The key is a bearer secret in the build or the environment, as the services key is, and not a
  file. A binary built with it holds it masked, which keeps it out of `strings` and does not encrypt
  it, so a binary built with a key is to be treated as holding one. In the environment it is readable
  by the account's own processes and is withheld from the programs bravebot starts ([PREM-14](#PREM-14)).
  A key written in a settings file's `env` block is plain text in that file.
- The key is presented only to a premium host that is Brave's ([PREM-2](#PREM-2)).
- `bravebot doctor` reports whether a key is held, on the `premium` line and among the credentials
  held, and never prints the value.
