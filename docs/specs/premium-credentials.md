---
id: PREM
title: Leo Premium credentials
status: normative
governs:
  - crates/skus/src/device.rs
  - crates/skus/src/profile.rs
  - crates/skus/src/store.rs
  - crates/aichat/src/lib.rs
  - crates/agent/src/subscription.rs
  - crates/agent/src/backend.rs
  - crates/agent/src/turn.rs
  - crates/tui/src/status.rs
  - crates/tui/src/logo.rs
documented-by: docs/website/docs/customize/premium.md
---

## Scope

Importing a Leo Premium subscription and spending its credentials. This is an auth flow: it
carries no workspace content and no model output, so no labelled value passes through it. What it
does carry is a user's subscription, which is why the rules below are about not spending or
leaking something that is not ours to spend.

```
bravebot import-leo-creds            # from Brave (stable)
bravebot import-leo-creds nightly    # or beta, or development
bravebot import-leo-creds --forget   # discard what was imported
```

## Clauses

<a id="PREM-1"></a>
### PREM-1: this registers as a new device and never borrows the browser's credentials

Only the subscription's **order id** is read from the browser profile. The credentials themselves
are generated here and signed by Brave, exactly as a second browser on another machine would. The
browser keeps its own, and nothing it holds is spent.

`verified-by: bravebot_skus::profile::the_browsers_own_credentials_are_not_read`
`verified-by: bravebot_skus::profile::an_order_id_that_is_not_a_uuid_is_refused_rather_than_used`
`verified-by: bravebot_skus::profile::uuids_are_recognised_and_near_misses_are_not`

<a id="PREM-2"></a>
### PREM-2: a credential is never sent to the non-premium host

The premium host and the credential travel together, because a credential belongs to a
deployment. A build with no premium host sends no credential rather than sending one where it
does not belong.

`verified-by: bravebot_aichat::client::a_subscribed_request_goes_to_the_premium_host_with_the_credential`
`verified-by: bravebot_aichat::client::without_a_premium_host_no_credential_is_attached`

<a id="PREM-3"></a>
### PREM-3: an order is checked before anything is registered against it

An unpaid order, one asking for no credentials, one asking for an implausible number, and one
without interval metadata are all refused. The Leo item is picked out of a multi-item order rather
than assumed to be the only one.

`verified-by: bravebot_skus::device::an_unpaid_order_cannot_be_registered_against`
`verified-by: bravebot_skus::device::an_order_asking_for_no_credentials_is_refused`
`verified-by: bravebot_skus::device::an_implausible_credential_count_is_refused`
`verified-by: bravebot_skus::device::an_order_without_interval_metadata_is_refused`
`verified-by: bravebot_skus::device::the_leo_item_is_picked_out_of_a_multi_item_order`
`verified-by: bravebot_skus::device::an_order_says_how_many_credentials_to_ask_for`

<a id="PREM-4"></a>
### PREM-4: a batch is verified against the issuer's key before it is stored

A batch signed by the wrong key does not verify, one without a proof is refused, and one with no
signed credentials is an error. Tokens are matched by value, never by position, and a token echoed
twice is matched once. A response holding only tokens that were never submitted matches nothing.

Verification covers the pairs this device matched, so a response proved over the whole of what it
lists does not verify when it lists anything that was not submitted, and the pairs that did match are
not kept.

`verified-by: bravebot_skus::device::a_batch_signed_by_the_wrong_key_does_not_verify`
`verified-by: bravebot_skus::device::a_batch_without_a_proof_is_refused`
`verified-by: bravebot_skus::device::a_response_with_no_signed_credentials_is_an_error`
`verified-by: bravebot_skus::device::tokens_are_matched_by_value_not_by_position`
`verified-by: bravebot_skus::device::a_batch_echoing_tokens_that_were_never_submitted_matches_nothing`
`verified-by: bravebot_skus::device::a_batch_proved_over_more_tokens_than_were_submitted_does_not_verify`
`verified-by: bravebot_skus::device::a_token_echoed_twice_is_matched_once_so_the_batch_does_not_verify`
`verified-by: bravebot_skus::device::a_well_formed_batch_is_decoded`

<a id="PREM-5"></a>
### PREM-5: a credential is single-use and is never offered twice

Credentials arrive in batches covering a few days and are spent one per request. A spent one is
never offered again, consecutive spends hand out different credentials, and spending past the end
of a batch is refused. A moment outside every validity window yields no credential.

`verified-by: bravebot_skus::store::a_spent_credential_is_never_offered_again`
`verified-by: bravebot_skus::store::consecutive_spends_hand_out_different_credentials`
`verified-by: bravebot_skus::store::spending_past_the_end_of_the_batch_is_refused`
`verified-by: bravebot_skus::store::the_next_usable_credential_is_the_one_valid_at_that_moment`
`verified-by: bravebot_skus::store::a_moment_outside_every_window_yields_no_credential`
`verified-by: bravebot_skus::store::a_window_does_not_include_its_own_end`

<a id="PREM-6"></a>
### PREM-6: nothing is written back unless a credential was actually spent

A session that spends nothing never writes, and a detached batch has nowhere to write. A spend
reaches the file when the wallet is flushed and when the session ends, not when the spend is made.

**Why.** A whole batch is hundreds of credentials and one is spent per model request, so writing
per spend would rewrite the file several times a turn to change one boolean.

`verified-by: bravebot_skus::store::spending_does_not_write_until_asked_to`
`verified-by: bravebot_skus::store::a_session_that_spends_nothing_never_writes`
`verified-by: bravebot_skus::store::a_spend_is_written_back_only_on_a_flush`
`verified-by: bravebot_skus::store::a_spend_is_written_back_when_the_session_ends`
`verified-by: bravebot_skus::store::a_detached_batch_has_nowhere_to_write`

<a id="PREM-7"></a>
### PREM-7: credentials live in one mode-0600 file under `~/.bravebot`

One file, not one per channel: a person has one subscription however many Brave builds they have
installed, so importing from Nightly replaces what was imported from Stable rather than sitting
beside it. The channel names where to read the order id from, which is a fact about the machine's
browsers rather than about the agent. `--forget` therefore takes no channel.

The file is created 0600 before anything is written to it, and is still 0600 after a re-import over
an existing file. A platform with no way to restrict a file to one account has the import refused
rather than written unrestricted, since what is kept here is a bearer token. The files beside it are
written either way, which [state-directory.md](state-directory.md) records as the cost it is. With no
profile directory named there is nowhere a secret belongs, and that is reported rather than guessed
at. `--forget` removes the file and is not an error when there is nothing to remove.

A malformed or empty file is reported as such rather than treated as absent credentials, and a
credential without a token is rejected on load.

**Why not the keychain.** It was the keychain, and that was wrong on both halves of the trade. The
browser these are imported from keeps the same secret unencrypted in `skus.state` and
`brave.ai_chat.premium_credential_cache`, with no OSCrypt anywhere in brave-core's `components/skus`
or `components/ai_chat`, so a keychain here guarded a copy of something already readable in the file
the copy came from. Nor did it hold against the threat it was written for, a program `run` launches
reading the file: those are deliberately unconfined ([RUN-10](tools/run.md#RUN-10)), and the AWS
credentials that sign every model request are cached by the `aws` CLI in plain 0600 JSON, so anything
that can read a file here can already take the larger secret. What it did cost was availability: the
keychain crate builds one Linux backend, the D-Bus Secret Service, so a machine reached over SSH with
no desktop session had no store to open and every such user was silently spending no subscription.

`verified-by: bravebot_skus::store::importing_again_replaces_the_previous_batch`
`verified-by: bravebot_skus::store::a_batch_written_to_the_file_is_read_back`
`verified-by: bravebot_skus::store::the_file_is_not_readable_by_anyone_else`
`verified-by: bravebot_skus::store::the_file_lives_in_the_users_own_directory`
`verified-by: bravebot_skus::store::no_home_directory_is_reported_rather_than_guessed`
`verified-by: bravebot_skus::store::forgetting_removes_the_file_and_is_repeatable`
`verified-by: bravebot_skus::store::an_empty_file_is_reported_rather_than_read_as_absent`
`verified-by: bravebot_skus::store::a_batch_that_is_not_json_is_reported_as_malformed`
`verified-by: bravebot_skus::store::a_file_that_is_not_json_is_reported_when_loaded`
`verified-by: bravebot_skus::store::a_credential_without_a_token_is_rejected_on_load`
`verified-by: bravebot_skus::store::an_entry_missing_its_order_is_reported_as_malformed`
`verified-by: bravebot_skus::store::a_batch_survives_a_round_trip_through_the_stored_form`

<a id="PREM-8"></a>
### PREM-8: a stored subscription that cannot be used is reported rather than skipped

Coming back empty has two causes and they are not the same fact. Nothing imported is not a fault
and is said nothing about. A batch that **exists and cannot be spent** is reported
to the person, with the reason and what to do about it. That covers a file that could not be read,
one holding nothing, one another version wrote, and one imported for an environment this endpoint
does not accept, since a credential only verifies against the deployment that signed it. An
endpoint belonging to no environment, such as a local one, is the first case and not the second: no
credential belongs near it by design. So is a machine with nowhere to keep credentials at all,
which [STATE-2](state-directory.md#STATE-2) makes a state this program supports: the store answers
that with the same error it gives a file that would not read, and reported as the second it told
somebody who has never held a subscription that theirs could not be used.

**Why.** The request then goes out with no credential, where the endpoint answers a premium model
name by **substituting** a weaker model rather than by failing, with a 200 and an ordinary reply. So a
request that silently lost its credential still returns something that reads like an answer, and
nothing on screen connects that to the credential store. The downgrade has to be said out loud,
because its only other symptom is the agent appearing to get worse for no reason.

**Asked by the model, of a turn that could have spent one.** Which service answers is decided by the
model ([BACKEND-3](backends.md#BACKEND-3)), and only Brave's endpoint has a notion of a subscription
at all. A turn whose model is served by AWS Bedrock or by a gateway therefore reads the store for
nothing and is told nothing about it, however unusable what is stored may be.

**Why.** There is no downgrade to report: such a turn ran on the model that was chosen, and the
credential it did not spend is one that service would not have accepted. The line said otherwise on
both halves: that the turn had gone out without a subscription, which is not the story of a request
signed for AWS, and that re-importing was the remedy, which would have changed nothing about the
answer. That is the same reasoning [BACKEND-9](backends.md#BACKEND-9) applies to a sign-in, and a
turn served entirely by one backend has no business acting on, or reporting on, the credentials of
another it will never call.

`verified-by: bravebot_agent::subscription::an_unreadable_batch_is_reported_and_an_absent_one_is_not`
`verified-by: bravebot_agent::subscription::an_endpoint_in_no_environment_is_not_a_complaint`
`verified-by: bravebot_cli::running::a_machine_with_no_profile_directory_has_nothing_imported`
`verified-by: bravebot_agent::home::a_subscription_imported_for_another_environment_is_reported`
`verified-by: bravebot_agent::home::an_empty_credentials_file_is_reported_rather_than_read_as_absent`
`verified-by: bravebot_agent::home::a_turn_on_another_backend_is_not_told_about_an_unusable_batch`
`verified-by: bravebot_agent::backend::only_the_aichat_backend_spends_an_imported_subscription`

<a id="PREM-9"></a>
### PREM-9: the tier reported is the one the last turn ran on, not the one the build was compiled with

What `/status` says about the tier is what the last turn actually did. Before the first turn it says
premium is available rather than claiming it is or is not in use.

The opening screen draws the tier beside the confinement, from the configuration, in the same words
`/status` uses before a turn has run. It does **not** read the credential store: a stored batch may
be expired, exhausted, or issued for another environment, so finding one would not settle the tier
either. A pane too narrow for the wordmark still reports both.

Where the server reports using a model other than the one requested, both are shown: the choice that
was made and the model that actually answered. Said once when it starts happening rather than every
turn. `automatic-brave-bot` resolving to a concrete model is not a substitution, since that is the
server choosing per request, which is what the automatic entry means.

**Why.** Every build that knows a premium host would otherwise report itself as premium, which is a
fact about compilation and not about any request. A session reported "premium configured" while ten
consecutive requests went out spending no subscription and were answered by a model a third the
size, which then announced tool calls it never emitted and stalled the turn. A panel that cannot be trusted on
this point is worse than one that omits it, and the substituted model is the half of PREM-8 a person
is most likely to notice first.

`verified-by: bravebot_tui::status::the_tier_reported_is_the_one_the_last_turn_actually_ran_on`
`verified-by: bravebot_tui::status::a_session_with_no_turn_yet_does_not_claim_a_tier`
`verified-by: bravebot_tui::status::the_opening_line_and_the_panel_say_the_same_thing_before_a_turn_runs`
`verified-by: bravebot_tui::logo::the_mark_names_the_agent_its_confinement_and_its_tier`
`verified-by: bravebot_tui::logo::a_narrow_pane_still_reports_the_confinement_and_the_tier`
`verified-by: bravebot_tui::status::a_substituted_model_is_reported_beside_the_one_asked_for`
`verified-by: bravebot_tui::status::automatic_being_resolved_to_a_real_model_is_not_a_substitution`
`verified-by: bravebot_tui::state::picking_automatic_and_being_answered_by_a_model_is_not_a_substitution`

## Requirements and limits

- **macOS and Linux**, including a machine with no desktop session, since nothing here needs one.
  Windows is not supported: the store is a Unix-mode file (PREM-7) and no browser profile is located
  for it.
- The build must know the premium host. Without it premium is unavailable (PREM-2).
- A credential only works against the deployment that issued it, so import from the Brave channel
  matching the environment the binary is configured for. A mismatch is refused before a request is
  made, rather than sent and answered with a 401 (PREM-8).
- Sign in to Leo in that Brave install first: a subscription that is not in the profile cannot be
  imported.
- The stored batch is a bearer secret in a file the user owns (PREM-7). It is not encrypted at rest,
  which is what the browser does with the same secret, and anything running as the user can read it.
- `bravebot doctor` reports how much is left.
