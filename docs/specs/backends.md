---
id: BACKEND
title: Backends
status: normative
governs:
  - crates/agent/src/backend.rs
  - crates/agent/src/outcome.rs
  - crates/agent/src/turn.rs
  - crates/agent/src/turn/hooked.rs
  - crates/agent/src/turn/result.rs
  - crates/agent/src/turn/delegate_end.rs
  - crates/agent/src/turn/setup.rs
  - crates/agent/src/turn/round_steps.rs
  - crates/agent/src/turn/prompts.rs
  - crates/agent/src/turn/task.rs
  - crates/agent/src/turn/sentences.rs
  - crates/agent/src/subscription.rs
  - crates/cli/src/import.rs
  - crates/cli/src/main.rs
  - crates/cli/src/invocation.rs
  - crates/cli/src/flags.rs
  - crates/cli/src/model.rs
  - crates/cli/src/doctor.rs
  - crates/cli/src/plain.rs
  - crates/bedrock/src/credentials.rs
  - crates/agent/src/gcloud.rs
  - crates/tui/src/app.rs
  - crates/tui/src/app/turns.rs
  - crates/tui/src/app/models.rs
  - crates/tui/src/app/terminal.rs
  - crates/tui/src/status.rs
  - crates/tui/src/state.rs
  - crates/tui/src/state/attachments.rs
  - crates/tui/src/state/accounts.rs
  - crates/tui/src/state/settings.rs
  - crates/config/src/bedrock.rs
  - crates/config/src/env_var.rs
  - crates/config/src/lib.rs
  - crates/config/src/managed.rs
  - crates/bedrock/src/lib.rs
  - crates/agent/src/bedrock_refusals.rs
  - crates/aichat/src/lib.rs
  - crates/aichat/src/call.rs
  - crates/aichat/src/anthropic.rs
  - crates/aichat/src/anthropic/protocol.rs
  - crates/aichat/src/generate_content.rs
  - crates/aichat/src/generate_content/protocol.rs
  - crates/aichat/src/conversation.rs
  - crates/aichat/src/models.rs
  - crates/aichat/src/roster.rs
  - crates/config/src/provider.rs
  - crates/config/src/settings.rs
  - crates/ui-bridge/src/models.rs
documented-by:
  - docs/website/docs/customize/configuration.md
  - docs/website/docs/customize/providers/anthropic.md
  - docs/website/docs/customize/providers/bedrock.md
  - docs/website/docs/customize/providers/openai-compatible.md
  - docs/website/docs/customize/providers/ollama.md
  - docs/website/docs/customize/providers/lm-studio.md
  - docs/website/docs/customize/providers/llama-server.md
  - docs/website/docs/customize/providers/openai.md
  - docs/website/docs/customize/providers/chatgpt.md
  - docs/website/docs/customize/providers/google-vertex.md
  - docs/website/docs/customize/providers/openrouter.md
  - docs/website/docs/customize/model.md
  - docs/website/docs/customize/effort.md
  - docs/website/docs/customize/managed-settings.md
---

## Scope

Where a request for a reply goes. Four services can answer: the aichat endpoint Brave runs, AWS
Bedrock through somebody's own account, a gateway somebody configured, which speaks the
OpenAI-compatible protocol or, for Anthropic's own API, the Messages one, and the ChatGPT account a person
signed in to ([CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9)), which speaks the Responses one.
This file governs which of them serves a given request, what a person is offered to choose from, and
what a configuration may decide.

The wire protocol of either service is ordinary code. So is signing, which
[network-egress.md](network-egress.md) covers as the one way out. What a reply is labelled once it
arrives is [labels.md](labels.md).

## Clauses

<a id="BACKEND-1"></a>
### BACKEND-1: a settings file may name a destination and never a permission

What a person's settings may say is which region, which credential profile, which host, which model
each tier names, which model to request when nobody has chosen one, and what to add to a request sent
to that host. Nothing in a settings file grants a capability or vouches for a path. None of them names
a command to run.

The block does not become the process environment either. A value is consulted where a variable
would be, and reaches a subprocess only where that subprocess is the thing it configures.

**Why.** These files are read before anything runs and are the easiest thing on the machine to write
to, so a capability that could be granted from one would be a capability granted by whatever last
edited it. That holds hardest for the layers a checkout carries, which arrive with the checkout.
Installing the names globally would put every one of them in front of every command the agent ever
starts, which is a far larger claim than "this is how I reach the backend". A command named in a file
would be the largest claim of all, since running it is an effect nobody approved: that is why a
gateway block names a variable holding a credential rather than a way to produce one.

**Note.** A `permissions` block is not an exception, and the route is what makes it one rather than
the prohibition alone. Its `deny` and `ask` rules only ever narrow what would otherwise be allowed,
so every layer's are read. The two names in it that would grant do not take effect on being read, and
both take the same route: a request put to the person when the session opens. The directories it
names are one question each, and one they decline is neither reachable nor vouched for; the `allow`
rules a checkout wrote are one question listing every one, and a rule nobody granted answers no
prompt. Nothing in a block grants an effect that was refused without it; only a person does.
[permissions.md](permissions.md) is what a block may say, and where both of those routes are written
down.

`verified-by: by-construction (values are consulted by name and never exported; the only value handed to a subprocess is the AWS profile, passed as an argument to the tool that owns it; no field is read as a path to execute, and a gateway's pass-through options reach a request body and nothing else)`
`verified-by: bravebot_tui::trust_prompt::a_directory_a_file_named_is_opened_only_where_the_person_accepts_it`
`verified-by: bravebot_config::settings::a_project_layer_allow_rule_is_not_granted`
`verified-by: bravebot_agent::permissions::a_checkout_cannot_write_a_rule_that_answers_a_prompt`
`verified-by: bravebot_tui::trust_prompt::the_rules_are_granted_only_where_the_person_accepts_them`

<a id="BACKEND-2"></a>
### BACKEND-2: configuring a second backend takes nothing away from the first

A settings block naming AWS tiers or a gateway does not change which model answers when nobody has
chosen one, and does not change how large a request may get before the conversation is shortened.

**Why.** Every build can reach Brave, and that is what somebody has before they configure anything.
Adding a way to reach more models should not quietly move the default onto one of them, nor set a
budget from a window that belongs to a model the session may never use.

`verified-by: bravebot_config::lib::a_bedrock_block_does_not_change_the_default_model`
`verified-by: bravebot_config::lib::a_bedrock_block_does_not_move_the_budget_off_the_default`
`verified-by: bravebot_config::lib::a_provider_block_changes_neither_the_default_model_nor_the_budget`

<a id="BACKEND-3"></a>
### BACKEND-3: the model names the service, and nothing else selects one

A request goes to whichever service offers the model it names. No other fact participates: not
which configuration is present, not which service answered last, not which one a person used
first.

**Why.** Where both are reachable a configuration cannot say where a request belongs. Bedrock
refuses a model it does not recognise rather than substituting one, and the aichat endpoint has
never heard of an inference-profile ARN, so a request sent on the strength of anything but the name
fails at the far end for a reason nothing local could explain.

**The ChatGPT account** is named by the identifier `chatgpt`, as `chatgpt/<name>`
([CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9)). That name selects the account, and only the part after the
identifier is sent. The bare `<name>` never selects it, so it keeps its meaning, and a `provider` entry that
has the id `chatgpt` and states an endpoint keeps the name. A gateway of another id that lists a model whose
name begins `chatgpt/` does not take it: that model is reached as `<id>/chatgpt/<name>`
([BACKEND-18](#BACKEND-18)). A Bedrock account that lists one does not take it either, and its model is
not reached by any spelling. The protocol a request is made in follows from the
service and not from anything else: the account is always spoken to in the Responses protocol
([BACKEND-61](#BACKEND-61)), and no request to its host is made in another
([CHATGPT-10](chatgpt-sign-in.md#CHATGPT-10)).

**Note.** The name is not content. It comes from a configured default or from a person picking off a
list they read, and a model's own output never reaches it.

`verified-by: bravebot_agent::backend::a_configured_bedrock_model_selects_the_bedrock_backend`
`verified-by: bravebot_agent::backend::a_brave_model_still_reaches_aichat_while_bedrock_is_configured`
`verified-by: bravebot_agent::backend::without_bedrock_configured_the_aichat_backend_is_selected`
`verified-by: bravebot_agent::backend::a_configured_gateway_model_selects_the_gateway_backend`
`verified-by: bravebot_agent::backend::a_gateway_reports_the_model_it_was_asked_for`
`verified-by: bravebot_agent::backend::an_anthropic_model_selects_the_gateway_backend_in_the_messages_protocol`
`verified-by: bravebot_agent::backend::a_google_vertex_model_selects_the_protocol_its_entry_names_a_project_for`
`verified-by: bravebot_agent::backend::a_chatgpt_model_selects_the_account_and_a_bare_name_selects_nothing_of_it`
`verified-by: bravebot_agent::backend::a_gateway_entry_named_chatgpt_keeps_its_name_and_no_model_is_the_accounts`
`verified-by: bravebot_agent::chatgpt::the_account_is_one_provider_at_the_serving_host`
`verified-by: bravebot_agent::backend::a_gateway_listing_a_chatgpt_name_does_not_take_it_from_the_account`
`verified-by: bravebot_agent::backend::a_bedrock_account_listing_a_chatgpt_name_does_not_take_it_from_the_account`

<a id="BACKEND-4"></a>
### BACKEND-4: what a person may choose is every model any reachable service offers

Configuring a second backend puts its models on offer beside the first one's rather than in place of
them. The models of a ChatGPT account a person signed in to are offered the same way
([CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9)): beside the other rosters, taking none of theirs away. A
listing of them that cannot be had leaves the other rosters as they were and adds the compiled names that
clause describes.

**Why.** A roster that replaced the other left somebody who named a single tier with a picker
offering exactly one model and no way back to the ones every build has. Reaching more models is not
a reason to stop reaching the existing ones.

`verified-by: bravebot_tui::app::configured_tiers_are_offered_alongside_the_brave_roster`
`verified-by: bravebot_tui::app::gateway_models_are_offered_alongside_the_other_rosters`
`verified-by: bravebot_agent::backend::a_gateway_does_not_take_the_other_rosters_away`
`verified-by: bravebot_ui_bridge::models::the_accounts_models_join_the_roster_and_a_failed_listing_adds_the_compiled_list_and_a_warning`
`verified-by: bravebot_tui::app::the_accounts_rows_are_met_in_reading_order_listed_or_compiled_and_nothing_else_adds_one`

<a id="BACKEND-5"></a>
### BACKEND-5: only models that can actually be reached are offered

A tier appears when the configuration names a model for it, and not otherwise. A service's own
roster is offered only where this build holds the credentials to reach it. Nothing is invented for a
tier that was left unset, and a gateway nothing can authenticate is not asked for a roster nobody
could then use. The ChatGPT account is such a service: its models are offered only where a sign-in is
stored, and where none is, nothing is asked of it ([CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9)). An entry
named `chatgpt` that states no endpoint and names models offers them on the same condition, where any other
entry that states no endpoint offers nothing ([BACKEND-17](#BACKEND-17)).

**Why.** A name on the list is a promise that picking it works. An ARN cannot be derived from a
model name, so an entry guessed for an unnamed tier is a choice that fails remotely, and a build
pointed only at AWS has no Brave credentials, so offering that roster would list models whose every
request fails unsigned.

`verified-by: bravebot_tui::app::a_tier_with_no_model_configured_is_not_offered`
`verified-by: bravebot_config::lib::without_brave_credentials_the_default_is_the_strongest_bedrock_tier`
`verified-by: bravebot_tui::app::only_the_gateway_models_the_file_named_are_offered`
`verified-by: bravebot_config::provider::a_provider_may_offer_no_models`
`verified-by: bravebot_config::provider::a_provider_without_a_base_url_is_not_offered`
`verified-by: bravebot_tui::app::the_brave_roster_is_offered_only_where_this_build_can_sign_for_it`
`verified-by: bravebot_tui::app::a_gateway_whose_credential_nothing_holds_is_not_asked_for_its_models`
`verified-by: bravebot_tui::app::an_aws_provider_entrys_models_are_offered_under_names_bedrock_answers_to`
`verified-by: bravebot_ui_bridge::models::the_window_asks_for_no_brave_roster_this_build_cannot_sign_for`
`verified-by: bravebot_ui_bridge::models::the_window_asks_no_gateway_whose_credential_nothing_holds`
`verified-by: bravebot_ui_bridge::models::the_window_offers_an_aws_provider_entry_under_its_bare_id`
`verified-by: bravebot_agent::chatgpt::a_machine_nobody_signed_in_on_offers_nothing_and_asks_the_service_nothing`
`verified-by: bravebot_agent::chatgpt::the_models_an_entry_names_are_offered_only_where_somebody_is_signed_in`

<a id="BACKEND-6"></a>
### BACKEND-6: a row says which service will answer it

Where the same model is reachable through more than one service, every model offered carries which
service answers it, in terms that cannot collide with a name a service chose for itself. Brave's own
roster carries nothing, being the one every build has. What a person reads is drawn from that,
under [VIEW-15](terminal-transcript.md#VIEW-15), and the note left when a model is chosen says it
too, since the row that carried it is gone by the time that note is read. The rows of the ChatGPT account
are keyed `chatgpt/<name>` and say ChatGPT, so the `openai` gateway's `<name>` and the account's
`chatgpt/<name>` are two rows where both offer the name ([CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9)).

**Why.** The two are billed differently and authenticate differently, so which one answers is the
whole of what is being chosen between. Naming the service is not enough: Brave serves part of its own
roster through Bedrock and says so in the names it sends, so that word appeared on both halves of the
list and distinguished nothing. Carried beside the name rather than composed into it, because a
roster of several services is read as sections, and a service composed into every name is that name
repeated down a hundred rows.

`verified-by: bravebot_tui::app::a_configured_tier_is_not_confusable_with_a_brave_model_served_through_bedrock`
`verified-by: bravebot_tui::app::a_tier_with_no_profile_configured_still_names_the_account`
`verified-by: bravebot_tui::app::a_gateway_row_says_which_service_answers_it`
`verified-by: bravebot_tui::app::the_note_left_on_choosing_a_model_names_its_service`
`verified-by: bravebot_aichat::models::a_fetched_row_carries_the_gateway_that_serves_it`
`verified-by: bravebot_aichat::models::only_a_row_the_account_lists_and_takes_in_the_api_is_offered_under_the_accounts_identifier`
`verified-by: bravebot_aichat::models::the_models_an_entry_names_are_rows_under_the_accounts_identifier_with_the_window_it_states`
`verified-by: bravebot_aichat::models::the_compiled_list_is_rows_under_the_accounts_identifier_one_to_a_name`

<a id="BACKEND-7"></a>
### BACKEND-7: the conversation budget belongs to the model in force

How large a request may get before the conversation is shortened is taken from the model that will
answer it, at the moment that model is chosen.

**Why.** A budget above the real window does not shorten a conversation late, it stops shortening it
at all, silently: every round asks, no round qualifies, and the session runs to exhaustion looking
like one with nothing to summarise.

`verified-by: bravebot_tui::app::a_bedrock_entry_carries_the_window_the_budget_is_taken_from`
`verified-by: bravebot_tui::app::the_window_of_a_model_chosen_earlier_is_found_in_the_listing`
`verified-by: bravebot_tui::app::a_gateway_entry_carries_the_window_the_budget_is_taken_from`

<a id="BACKEND-8"></a>
### BACKEND-8: an unreachable listing costs only what it described

One service failing to say what it offers does not withdraw models known from configuration alone. A
choice is refused only when there is nothing left that could be chosen.

**Why.** Configured tiers need no network to know. Refusing the whole picker because one half was
unreachable would leave the only models this configuration can definitely reach unpickable, which is
the position somebody offline is most likely to be in.

`verified-by: bravebot_tui::app::an_unreachable_listing_still_offers_the_configured_tiers`
`verified-by: bravebot_tui::app::an_unreachable_listing_with_no_tiers_configured_is_still_a_failure`
`verified-by: bravebot_tui::app::an_unreachable_listing_still_offers_the_gateway_models`

<a id="BACKEND-9"></a>
### BACKEND-9: a sign-in is asked for before work starts, by the model about to answer

Where a service authenticates interactively and has no usable session, the sign-in happens before a
request is attempted, and only for the service the next request will actually go to. What it asks of
the person is shown where they are already reading, line by line as it is written, and the interface
keeps its display throughout.

**Why.** A sign-in prints a URL and a code and then waits for them to be used, so those lines are the
flow rather than a report of it: shown after the fact, or collected and printed at the end, they
arrive once the code has stopped working. Giving the screen away instead puts them under a display
that is about to redraw over them, and leaves somebody in a terminal that no longer resembles the
program they were using. Doing it up front is what keeps it off the request path, where the work has
begun and nobody is being asked anything. Asking by the model rather than by what is configured
matters because otherwise a turn served entirely by one backend stops to authenticate against
another it will never call.

**The ChatGPT account starts no sign-in.** A model named `chatgpt/<name>` needs no sign-in before work
starts, and no turn, hook, tool or slash command starts one: `bravebot auth login chatgpt` is typed by a
person ([CHATGPT-1](chatgpt-sign-in.md#CHATGPT-1)). A turn on that model that finds no usable sign-in ends
before a request is sent, naming the command. Replacing an access token that is about to expire is not a
sign-in and is done inside the turn ([CHATGPT-7](chatgpt-sign-in.md#CHATGPT-7)).

The xAI sign-in starts none either: `bravebot auth login xai` is typed by a person
([XAI-7](xai-sign-in.md#XAI-7)). A model named `xai/<name>` needs no sign-in before work starts, and a turn
on it that finds no usable sign-in ends before a request is sent, naming the command
([XAI-17](xai-sign-in.md#XAI-17)).

`verified-by: bravebot_agent::backend::a_brave_model_never_needs_an_aws_sign_in`
`verified-by: bravebot_agent::backend::without_bedrock_configured_nothing_needs_a_sign_in`
`verified-by: bravebot_agent::backend::signing_in_for_a_model_no_aws_account_serves_does_nothing`
`verified-by: bravebot_agent::backend::a_gateway_model_never_needs_an_aws_sign_in`
`verified-by: bravebot_agent::backend::a_model_an_aws_block_named_needs_a_sign_in_of_its_own`
`verified-by: bravebot_agent::backend::signing_in_for_a_model_an_aws_block_named_reaches_that_account`
`verified-by: bravebot_agent::backend::an_aws_block_leaves_the_other_rosters_needing_no_sign_in`
`verified-by: bravebot_agent::backend::the_account_answers_for_its_own_models_and_for_nothing_a_credential_of_another_service_is_for`
`verified-by: bravebot_cli::layering::the_chatgpt_sign_in_is_started_by_the_command_and_nothing_else`
`verified-by: bravebot_cli::layering::the_xai_sign_in_is_started_by_the_command_and_nothing_else`
`verified-by: bravebot_agent::backend::a_turn_on_an_xai_model_with_no_usable_sign_in_ends_before_a_request_naming_the_command`
`verified-by: bravebot_cli::running::a_turn_on_a_chatgpt_model_with_no_sign_in_ends_naming_the_command_and_stores_nothing`

<a id="BACKEND-10"></a>
### BACKEND-10: asking whether a session is good costs nothing once it is known to be

Establishing that a service has a usable session runs its tool once. Until the credential that
answer came from is close enough to its own stated expiry to be no use to the request that follows,
the same question is answered without running anything. A session that is not good is never reported
as one, and an answer with no stated expiry is not kept.

A kept answer is dropped the moment a request proves it wrong, and the next check runs the tool
again.

**Why.** The check happens before every turn, and the tool that answers it takes most of a second, so
paid each time it is a pause between pressing Enter and seeing the line appear. The expiry is the
credential's own word about how long the answer stays true, which is why it and not a fixed interval
is what bounds this. Stopping short of it matters because the answer is used to decide whether to
sign in before work that then has to be signed: taken at the last second, the request that follows
carries a credential that has already expired.

An expiry is what the credential says, not a promise. A session can be revoked, ended from another
machine, or lose what it granted, and then a kept answer is wrong before the time it named. Held on
to, it is worse than never having cached at all: every check before every turn repeats the stale yes,
so the sign-in that a person can see never runs, and each turn instead fails to one that reports to
nobody. Dropping it on the first request that disproves it is what keeps the caching an optimisation
rather than a way to get stuck.

`verified-by: bravebot_bedrock::credentials::a_session_already_shown_to_be_good_is_not_asked_about_again`
`verified-by: bravebot_bedrock::credentials::a_session_that_has_run_out_is_asked_about_again`
`verified-by: bravebot_bedrock::credentials::a_session_about_to_run_out_is_treated_as_already_gone`
`verified-by: bravebot_bedrock::credentials::one_profile_being_good_says_nothing_about_another`
`verified-by: bravebot_bedrock::credentials::the_default_profile_is_remembered_like_any_other`
`verified-by: bravebot_bedrock::credentials::a_session_with_no_stated_expiry_is_not_kept`
`verified-by: bravebot_bedrock::credentials::an_expiry_is_converted_to_the_instant_it_names`
`verified-by: bravebot_bedrock::credentials::the_expiry_the_cli_reports_is_read_from_the_process_format`
`verified-by: bravebot_bedrock::credentials::an_expiry_that_is_not_the_expected_shape_is_not_guessed_at`
`verified-by: bravebot_bedrock::credentials::a_session_shown_to_be_bad_is_no_longer_remembered_as_good`
`verified-by: bravebot_bedrock::credentials::forgetting_one_profile_leaves_the_others_alone`
`verified-by: bravebot_bedrock::credentials::a_refused_export_drops_the_kept_answer_before_the_sign_in`
`verified-by: bravebot_bedrock::credentials::a_good_export_and_another_profiles_refusal_leave_a_kept_answer_alone`

<a id="BACKEND-11"></a>
### BACKEND-11: a settings file names the model above the build, and a pick above a checkout's

Where a settings file names a model, that name is what a request uses, in preference to the model
compiled into the binary and to one exported as `BRAVEBOT_DEFAULT_MODEL`. The exported variable
answers where no file names one, above the build.

A choice recorded with `/model` ranks as the person's own file, `~/.bravebot/settings.json`, does.
It outranks the `model` key in that file, is outranked by one in the file `--settings` named
([BACKEND-24](#BACKEND-24) reads that file after all three, and naming it is the person's own act),
and a checkout's `.bravebot/settings.json` or `.bravebot/settings.local.json` cannot name a model at
all. A key spelled blank, or as something other than a string, names nothing and does not outrank a
pick. It still displaces the key a lower file named, so with nothing recorded the exported variable
or the build answers. Above all of it are `--model` and a model picked in the session that is
running. The `/model` picker, at rest and during a turn, opens on the model in force and marks it as
current: the pick where the session has one, and otherwise the model this ranking settles on, so a session
on the `model` key marks that model and Brave's `automatic` only where `automatic` is what the ranking
settles on. The same model in force is the one the roster is read for the window, the inputs and whether a
level is read, and the one the sign-in a session makes and the refusal of a level a turn learned are asked
about, so none of them is read for the pick alone.

**Why.** Every release bakes a default model in, so this value ranked like the rest of the file would
lose on every binary anybody was given: the key would parse, `doctor` would report it, and nothing
would change outside a source build.

The variable is named as a default, and a default is how it is used: a `.envrc` that exports it for
every checkout of a project is saying what answers when nothing else does. Ranked above the file it
would outrank every `model` key on any machine that sources one. Claude Code ranks its counterpart,
`ANTHROPIC_DEFAULT_MODEL`, last. It carries this program's prefix rather than one for Brave's
endpoint because it names the default for every service, Bedrock and a `provider` block's gateways
included.

A pick is recorded once per person and read back in every checkout. Ranked above a checkout's file,
it is the one thing a checkout cannot override: two checkouts in one account cannot want different
models, and a project that states its model is undone by whatever its reader last picked anywhere.
Claude Code's `/model` writes the person's own settings file, and the pick ranks as that file does.
The pick outranks that file's own key because both are the same person speaking at the same rung,
and the pick is the later of the two.

`verified-by: bravebot_config::lib::a_model_in_the_settings_file_outranks_the_baked_in_one`
`verified-by: bravebot_config::lib::a_model_in_the_settings_file_outranks_an_exported_one`
`verified-by: bravebot_config::lib::the_env_block_spelling_stays_below_the_baked_in_value`
`verified-by: bravebot_config::settings::a_layer_above_the_home_one_does_not_pick_a_model`
`verified-by: bravebot_config::settings::the_home_layer_does_not_outrank_a_saved_pick`
`verified-by: bravebot_config::settings::a_layer_above_that_names_nothing_does_not_outrank_a_saved_pick`
`verified-by: bravebot_config::settings::a_named_file_spelling_the_model_blank_displaces_the_home_key_and_names_nothing`
`verified-by: bravebot_cli::running::doctor_says_a_checkouts_provider_block_is_not_obeyed`
`verified-by: bravebot_session::store::a_checkouts_model_cannot_displace_the_saved_pick_and_the_home_file_does_not`
`verified-by: bravebot_tui::persist::a_recorded_model_answers_over_a_checkouts_file_and_the_persons_own`
`verified-by: bravebot_cli::running::a_run_asks_for_the_recorded_model_ignoring_a_checkouts`
`verified-by: bravebot_cli::running::a_run_asks_for_the_settings_model_over_an_exported_default`
`verified-by: bravebot_cli::running::a_run_asks_for_an_exported_default_where_no_file_names_a_model`
`verified-by: bravebot_cli::running::doctor_names_the_pick_a_checkouts_model_cannot_displace`
`verified-by: bravebot_tui::app::the_picker_marks_the_model_in_force_which_is_the_default_where_nothing_is_picked`
`verified-by: bravebot_tui::app::the_list_the_model_command_opens_marks_the_model_in_force_and_a_row_picked_is_put_in_force`
`verified-by: bravebot_tui::app::the_refusal_of_a_level_is_asked_of_the_model_in_force_and_its_answer_is_returned`
`verified-by: bravebot_tui::app::the_inputs_the_roster_lists_are_those_of_the_model_in_force_which_is_the_default_unpicked`

<a id="BACKEND-12"></a>
### BACKEND-12: a tier word names a model some reachable service serves

`opus`, `sonnet` and `haiku` name a tier rather than a model. Each resolves to a model that can
actually be reached: the model an AWS account named for that tier, and otherwise that tier's name on
the roster every build can reach. A tier word is never sent as the bare word. Any other name is used
as written.

**Why.** Those three words are what a settings file written for another tool puts in this key, so
they are the common case rather than an edge one. Sent unresolved they reach a service that has never
heard of them: Bedrock refuses an unknown model, and the aichat endpoint silently resets one, which
makes the key appear to work while changing nothing. An AWS account that named the tier wins because
naming it is asking for it, and a tier it left unset falls through rather than being guessed at,
since an ARN cannot be derived from a word. The exception is a build holding no Brave credentials,
where a Brave name reaches a service it cannot sign for.

**Note.** The Brave names are compiled in rather than matched against the model listing. A
configuration is built without touching the network, and a one-shot run never asks for that listing:
only the interactive picker does. Resolving a word against it would put a round trip in front of every
one-shot run to expand one word, and would fail with no network where it currently succeeds. The cost
is that the service owns those names, and a renamed one is reset by the endpoint to
`automatic-bravebot`, which is where somebody with no `model` key already starts.

`verified-by: bravebot_config::lib::a_tier_alias_resolves_to_the_model_that_tier_names`
`verified-by: bravebot_config::lib::a_tier_alias_without_bedrock_resolves_against_the_brave_roster`
`verified-by: bravebot_config::lib::an_alias_for_an_unconfigured_tier_falls_through_to_brave`
`verified-by: bravebot_config::lib::without_brave_credentials_an_unconfigured_tier_stays_on_aws`
`verified-by: bravebot_config::lib::a_tier_word_resolves_against_an_aws_account_a_provider_block_named`
`verified-by: bravebot_config::lib::a_tier_word_resolves_to_a_model_the_block_actually_offers`
`verified-by: bravebot_config::lib::with_brave_credentials_a_tier_word_still_falls_through_to_brave`
`verified-by: bravebot_config::lib::a_model_that_is_not_a_tier_alias_is_used_as_written`
`verified-by: bravebot_config::bedrock::every_tier_names_a_brave_model`
`verified-by: bravebot_config::bedrock::the_tiers_name_different_brave_models`

<a id="BACKEND-13"></a>
### BACKEND-13: a gateway block is read in the shape the tool that already reads it uses

What configures a gateway is a block whose field names, nesting and optionality are another tool's,
so that a block copied out of that tool's configuration works here unedited. Nothing is required that
it does not require, no field is added to the block however useful it would be, and a field this
system does not know is read past rather than refused.

**Why.** The whole value of the shape is that somebody already knows it and an editor already
validates it. A field added here would be one the other tool rejects, and a requirement added here
would refuse a block it accepts, so either one costs exactly the property the borrowing was for.
Reading past an unknown field is what makes a copy work, which is why it is the deliberate behaviour
and not a shortcoming.

Optionality is the part of the shape easiest to take and then quietly not honour. That tool resolves a
gateway's endpoint and its roster from a registry it fetches, so its blocks leave both out and the
commonest one names a credential and nothing else. Requiring either field here refuses a block it
accepts as surely as adding a field would.

An entry whose id is `chatgpt` and that states no endpoint is read for its `models` and for nothing else
([CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9)). It names no gateway, so the account stays reachable. It is
the one entry with no endpoint that is read for something, where any other is read for nothing.

A configured gateway also works in a development build without Brave service credentials. Those
credentials remain required when neither a gateway nor Bedrock is configured. A gateway's token is
resolved when it is used; its absence is a gateway authentication error, not a request for Brave keys.
The selected model names the gateway, for example `openrouter/z-ai/glm-4.6` in the top-level `model`
key of `~/.bravebot/settings.json`. Adding a gateway does not replace a configured Brave or Bedrock
backend or override the chosen model.

`verified-by: bravebot_config::lib::a_gateway_configures_without_brave_credentials_or_a_model_roster`
`verified-by: bravebot_config::lib::a_gateway_can_read_its_own_environment_token_without_brave_credentials`
`verified-by: bravebot_config::lib::an_absent_or_invalid_gateway_does_not_relax_brave_validation`
`verified-by: bravebot_config::lib::a_gateway_keeps_bedrock_available_without_brave_credentials`
`verified-by: bravebot_config::provider::a_chatgpt_entry_without_an_endpoint_names_the_accounts_models_and_is_not_a_gateway`
`verified-by: bravebot_config::provider::a_chatgpt_entry_with_a_blank_endpoint_still_names_the_accounts_models`
`verified-by: bravebot_config::lib::the_models_a_chatgpt_entry_names_reach_the_configuration_and_the_account_stays_reachable`

`verified-by: bravebot_config::provider::a_provider_block_is_read`
`verified-by: bravebot_config::provider::a_model_entry_may_be_empty`
`verified-by: bravebot_config::provider::fields_this_crate_does_not_know_are_read_past`
`verified-by: bravebot_config::provider::a_limit_missing_either_half_states_no_window`
`verified-by: bravebot_config::settings::a_provider_block_is_read_beside_the_env_block`
`verified-by: bravebot_config::settings::a_provider_block_is_read_without_an_env_block`

<a id="BACKEND-14"></a>
### BACKEND-14: a window nobody stated is assumed low, never asked for and never guessed high

A configured model may state the size of its context window, and where it does not, the figure
assumed is one deliberately below what the model is likely to have. Nothing is asked over the network
to find out, and nobody is required to supply it.

**Why.** A budget above the real window does not shorten a conversation late, it removes shortening
altogether and silently. So the error has to lean low, which is the same reasoning already applied to
the single figure assumed for an opaque AWS profile. Requiring the number instead asks for one the
person writing the file does not have, since a window belongs to the model and the upstream serving
it, and a figure typed to satisfy a requirement looks authoritative in a way a default does not.
Asking the service costs a round trip before a picker can draw and breaks the case where nothing is
reachable.

Stating one still has to be possible, because one gateway serves models whose windows differ by more
than an order of magnitude, and pinning a request to a particular upstream can cap the window well
below what the model offers elsewhere.

A provider's `options.contextWindow` states the window of every model of that provider that states
none, for a server whose window is one setting of the server, such as a local Ollama. It is the only
place to keep a figure for an entry that lists no models, since `limit` belongs to a listed model
and listing models turns off the roster. A model's own `limit.context` outranks it, and it outranks
a window the roster reports. A value that is not a whole number above zero states nothing.

`verified-by: bravebot_config::provider::a_provider_window_is_the_default_for_models_that_state_none`
`verified-by: bravebot_config::provider::an_unusable_provider_window_is_not_read`
`verified-by: bravebot_aichat::models::a_provider_window_is_taken_for_fetched_models`
`verified-by: bravebot_config::provider::a_model_without_a_stated_window_gets_the_assumed_one`
`verified-by: bravebot_config::provider::a_stated_window_is_read`
`verified-by: bravebot_tui::app::a_gateway_entry_carries_the_window_the_budget_is_taken_from`
`verified-by: bravebot_tui::app::a_gateway_model_with_no_stated_window_still_carries_one`

<a id="BACKEND-15"></a>
### BACKEND-15: what a gateway block adds to a request is carried, never interpreted

A configured model may carry a block of options that reaches the request body as it stands. Nothing
here parses it, knows what any of its fields mean, or validates them, and it cannot replace what the
turn itself put in the request.

**Why.** A gateway's routing controls are its own invention, so a schema enumerating them is one that
has to change when the gateway adds a field, and supporting the shape of gateways generally is what
makes this something other than support for one of them. It comes from the person's own configuration
surface and no model output reaches it, so it is trusted as far as a variable they exported would be.
That footing is what also bounds it: a destination may be named from a file, and a file overwriting
the model or the messages a turn built would be deciding what was asked rather than where it goes.

`verified-by: bravebot_aichat::lib::model_options_are_merged_into_the_request_body`
`verified-by: bravebot_aichat::lib::model_options_cannot_overwrite_what_the_turn_built`
`verified-by: bravebot_aichat::lib::a_model_with_no_options_adds_nothing_to_the_body`
`verified-by: bravebot_config::provider::model_options_are_carried_without_being_interpreted`

<a id="BACKEND-16"></a>
### BACKEND-16: a gateway credential is named rather than resolved ahead of time

Where a gateway's credential lives is named by its block: variables that may hold it, or a value
written in the file. A key `bravebot auth login gateway` stored for the block's id is a third place,
kept in a file of its own rather than in settings ([CLI-18](cli.md#CLI-18)). A variable is read
first, then the stored key, then the value in the file. It is read at the point a request needs it,
and a request whose block named a credential that nothing holds is refused with the remedy named
rather than sent.

**Why.** Read once at startup, a credential goes stale in a session where somebody exported a new
one. Sent without one, the request fails at the far end for a reason nothing local could explain,
which is the same argument that stops a model name being guessed for an unconfigured tier. Naming a
variable is also the only way to keep a long-lived token out of a file people paste into issues,
which is why it is preferred where the block offers both and why a value in the file never displaces
one a variable holds. A stored key is out of the settings file too, so it comes before the value in
it. A variable still comes first, because one exported for a single session is the more specific
thing somebody said.

**The stored key is read when the program starts**, not at each request, so a key stored while a
session is running is sent by the sessions started after it. The key is only stored, and never
named by the block, so a key stored for an id no block names configures nothing, and the managed
layer's gateways ([BACKEND-38](#BACKEND-38)) take one as a person's own do.

**A known name with no block.** `bravebot auth login gateway` stores a key for a known name
([BACKEND-17](#BACKEND-17)) before any block names it, and then adds an entry that holds no field to
the person's own settings file ([cli.md](cli.md)). The entry names the name and nothing else, so the
key is sent to the host compiled in for it, as [BACKEND-17](#BACKEND-17) says that name is reached:
as `authorization: Bearer` for `openrouter` and `openai`, and as `x-api-key` for `anthropic`. It
names no variable, so the variable that name's own service reads, such as `OPENAI_API_KEY` for
`openai`, is not read: a variable is read only where a block names it. The sign-in offers no known
name where the machine-level layer names the gateways, since a block recorded in the person's file
would not be read there.

**An entry naming AWS is the exception**, and not a token kept somewhere else: Bedrock takes a
signature over the request, so there is no credential for a block to name and none is read from one.
Which set of AWS credentials to sign with comes from the profile the block names, resolved when a
request needs it, which is the same moment and the same reason a token is read.

**A block naming no credential is the second exception.** An empty `env`, no `options.apiKey` and no
key stored for its id is the person saying this gateway wants none, so its requests carry no
`authorization` header and its roster is asked for without one. A key stored for such a block is
sent, because storing one is somebody saying the gateway wants one. A block that does name somewhere
for a credential to live, and finds nothing there, is a stale or missing token and is still refused.

**A known name reached at its compiled-in host is the exception to that exception.** A block whose id
is one [BACKEND-17](#BACKEND-17) compiles an endpoint in for, and which states no `baseURL`, is
reached at a service that refuses every request without a key, so its naming no credential and finding
no stored key is a credential that is missing, not one that is not wanted. It is refused with the
remedy named, `auth status` says it is not signed in and `doctor` says none was found. A block stating
a `baseURL` of its own keeps the rule above, so a local service reached under a known id still needs
no key, and so does a block that carries the credential in a header it names in `headers`.

**Why the distinction is where it is.** The reason above holds for the second case and not the first:
a gateway that wants no credential answers an unauthenticated request, so nothing fails at the far end
for a reason nothing local could explain, and a refusal names a remedy that does not exist. Requiring
a value instead would require a field of a block that does not have one, which BACKEND-13 rules out,
and a dummy `apiKey` teaches people to write fake credentials into a file they paste into issues.
Deciding it by endpoint alone would refuse the same local service reached
across a LAN or through a reverse proxy; the exception above reads the id and the absence of a stated
endpoint, which such a service does not match. BACKEND-5 is unaffected: such a gateway is reachable, so
offering its models is not offering rows that fail when picked.

`verified-by: bravebot_config::provider::a_named_variable_holds_the_token_before_the_file_does`
`verified-by: bravebot_config::provider::a_stored_key_is_read_after_a_variable_and_before_the_file`
`verified-by: bravebot_config::provider::a_stored_key_is_sent_to_a_block_naming_no_credential`
`verified-by: bravebot_config::lib::a_stored_key_reaches_the_gateway_its_id_names`
`verified-by: bravebot_config::lib::an_entry_that_names_nothing_under_a_known_id_is_sent_the_stored_key_and_no_variable`
`verified-by: bravebot_config::lib::a_known_id_recorded_as_an_empty_entry_is_sent_its_key_in_the_header_its_protocol_uses`
`verified-by: bravebot_cli::running::a_run_sends_the_key_stored_for_its_gateway`
`verified-by: bravebot_config::provider::a_token_written_into_the_file_is_still_read`
`verified-by: bravebot_config::provider::a_provider_with_nothing_holding_a_token_has_none`
`verified-by: bravebot_config::provider::a_provider_naming_no_credential_needs_none`
`verified-by: bravebot_config::provider::a_known_id_at_its_compiled_in_host_naming_no_credential_is_missing_one`
`verified-by: bravebot_agent::backend::a_known_gateway_naming_no_credential_refuses_the_request`
`verified-by: bravebot_cli::doctor::a_known_gateway_naming_no_credential_is_reported_as_missing_one`
`verified-by: bravebot_cli::running::auth_status_known_gateway_naming_no_credential_is_not_signed_in`
`verified-by: bravebot_agent::backend::a_gateway_with_nothing_holding_a_token_refuses_the_request`
`verified-by: bravebot_agent::backend::a_gateway_naming_no_credential_sends_unauthenticated`
`verified-by: bravebot_aichat::lib::a_gateway_needing_no_credential_sends_no_authorization_header`
`verified-by: bravebot_aichat::client::a_gateway_needing_no_credential_is_asked_for_its_roster_unauthenticated`
`verified-by: bravebot_cli::doctor::a_gateway_needing_no_credential_is_reported_as_needing_none`

<a id="BACKEND-17"></a>
### BACKEND-17: a gateway named by a name this system knows needs no endpoint written down

A block naming a gateway this system already knows an endpoint for reaches it without stating one. An
endpoint the block does state is where its requests go regardless. Where neither holds, the entry
configures no service. The one entry read for something in that case is `chatgpt`: with no endpoint it is
no gateway, and its `models` are the ones the ChatGPT account is offered
([CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9)). `chatgpt` is not a known name. The account's host is a
constant of the account ([CHATGPT-2](chatgpt-sign-in.md#CHATGPT-2)), and an entry under that id that states
an endpoint is a gateway like any other and keeps the name. No other entry takes it by listing a model
whose name begins `chatgpt/` ([BACKEND-18](#BACKEND-18)).

The set of known names is compiled in: `openrouter`, reached at `https://openrouter.ai/api/v1`,
`openai`, reached at `https://api.openai.com/v1`, and `anthropic`, reached at
`https://api.anthropic.com/v1` in the protocol [BACKEND-57](#BACKEND-57) states. Nothing is fetched to
resolve one, and no service is asked what its own endpoint is.

`bravebot auth login gateway` offers a key for exactly these names where no block names them, and
for none of them where the machine-level layer names the gateways. It shows each with the endpoint
its entry would reach: a known name asked for alone is read as an entry that holds no field is, so
the host it shows is the one a session sends the key to.

A known name supplies an endpoint and no credential. An `openai` entry is reached as any other
gateway is, with its key as a bearer token in `authorization`, and names where that key lives as
[BACKEND-16](#BACKEND-16) says. It is reached in the Responses protocol ([BACKEND-61](#BACKEND-61)) where it
reaches OpenAI's own host, whether it states no endpoint or states that host, and in chat completions where it
names `@ai-sdk/openai-compatible` in `npm`, which is how a person asks for them at OpenAI's host, or states
the endpoint of a proxy, which may speak chat completions alone. What else differs at OpenAI's own host
follows the host and port a request goes to, whatever the entry is called. [BACKEND-19](#BACKEND-19) states
the models its roster leaves out, those that only the Responses API takes where the route is chat
completions and those past their `shutdown_date`, and [BACKEND-32](#BACKEND-32) states that no cache marks
are sent. A proxy keyed `openai` is a gateway as any other is, and an entry under another id that states
OpenAI's endpoint is treated as OpenAI's for those two, and keeps chat completions. An `anthropic` entry names the key the same way, and is the one known name
whose entry may say it is not Anthropic's: one that names `@ai-sdk/openai-compatible` in `npm`, as
opencode names the SDK for a gateway, is an OpenAI-compatible gateway like any other and states an
endpoint of its own, since the address compiled in is the one the other protocol is spoken at. Its models are asked for as [BACKEND-19](#BACKEND-19) says, and
[Known costs](#known-costs) records what that offers for this service.

An entry naming `@ai-sdk/openai` in `npm` is reached in the Responses protocol whatever its id,
except `google-vertex`, whose id fixes its protocols ([BACKEND-61](#BACKEND-61)). Under the id `anthropic` it states an endpoint of its own, as one naming
`@ai-sdk/openai-compatible` does, since the address compiled in is the one the Messages API is spoken at.

One thing differs for `openai` and `anthropic`: the name a reply reports is not compared with the name
asked for, as [BACKEND-18](#BACKEND-18) compares a gateway's. A name such as `gpt-4o` or
`claude-haiku-4-5` is an alias for a dated model, and the reply names the model behind it, which is
the alias working and not a substitution.

**Why.** The tool this block's shape is borrowed from resolves an endpoint from a registry it fetches,
so a block copied out of it names one nowhere and requiring the field refuses a block that tool
accepts. That is exactly the property the borrowing exists for, and the shape is worth nothing if the
commonest block copied still has to be edited.

Compiled in rather than fetched because this value is where a bearer credential is sent. A service that
could decide it could have somebody's token by answering a request, which is the one thing a
destination may never be derived from, so the table is one somebody reviewed and shipped. Keeping it
short costs nothing: an absent name is served by writing the endpoint, so the price of not knowing a
gateway is a line of configuration rather than an unreachable service. A stated endpoint winning is
what keeps a known name usable against a proxy or a private deployment.

`verified-by: bravebot_config::provider::a_known_provider_name_supplies_its_own_endpoint`
`verified-by: bravebot_config::provider::the_known_ids_are_the_ids_an_endpoint_is_compiled_in_for_and_no_other`
`verified-by: bravebot_config::provider::a_known_id_asked_for_alone_is_read_as_an_entry_that_names_nothing`
`verified-by: bravebot_config::provider::an_id_with_no_row_in_the_table_is_not_a_known_provider`
`verified-by: bravebot_config::provider::a_stated_endpoint_beats_the_one_compiled_in`
`verified-by: bravebot_config::provider::an_openai_entry_is_reached_at_openais_host_like_any_other_gateway`
`verified-by: bravebot_config::provider::an_openai_entry_is_reached_in_the_responses_protocol_at_openais_own_api_only`
`verified-by: bravebot_config::provider::a_stated_endpoint_beats_the_one_compiled_in_for_openai`
`verified-by: bravebot_config::provider::openais_own_api_follows_the_host_a_request_goes_to_whatever_the_entry_is_called`
`verified-by: bravebot_agent::backend::an_openai_alias_is_not_expected_to_be_the_name_that_comes_back`
`verified-by: bravebot_config::provider::an_anthropic_entry_is_reached_through_the_messages_api_at_anthropics_host`
`verified-by: bravebot_config::provider::a_stated_endpoint_beats_the_one_compiled_in_for_anthropic`
`verified-by: bravebot_config::provider::an_anthropic_block_naming_no_key_names_no_credential`
`verified-by: bravebot_agent::backend::an_anthropic_alias_is_not_expected_to_be_the_name_that_comes_back`
`verified-by: bravebot_config::provider::an_anthropic_entry_naming_the_openai_compatible_sdk_is_a_gateway_like_any_other`
`verified-by: bravebot_config::provider::what_decides_the_protocol_of_an_entry_is_its_id_and_the_sdk_it_names`
`verified-by: bravebot_config::provider::a_provider_without_a_base_url_is_not_offered`
`verified-by: bravebot_config::provider::an_entry_naming_the_openai_sdk_is_reached_through_the_responses_api`
`verified-by: bravebot_config::provider::only_a_chatgpt_entry_that_states_no_endpoint_names_the_accounts_models`
`verified-by: bravebot_config::lib::a_provider_entry_named_chatgpt_keeps_the_name_and_no_model_is_the_accounts`

<a id="BACKEND-18"></a>
### BACKEND-18: a model name may say which gateway is meant, and only the rest of it is sent

A name selecting a gateway model may carry the gateway's own identifier ahead of the name that
gateway knows the model by, separated once. The identifier selects the service; only the remainder is
sent to it, and it is also what a reply's name is compared against. A name no configured gateway
claims reaches the service that does recognise it, as before.

The ChatGPT account of [CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9) is named the same way, by the identifier
`chatgpt`, and a `provider` entry with that id that states an endpoint keeps it: the entry's models go to its
gateway and none is the account's. An entry with that id that states no endpoint is no gateway, and its models
are the ones the account is offered as. A bare name that begins `chatgpt/` is the account's even where a gateway
of another id lists it, and that gateway's model is reached by naming it with the gateway's identifier ahead of
the name, as for any model: `<id>/chatgpt/<name>`.

The xAI account of [XAI-13](xai-sign-in.md#XAI-13) is named the same way, by the identifier `xai`, and a
`provider` entry with that id that states an endpoint keeps it: the entry's models go to its gateway and none is
the account's. An entry with that id that states no endpoint and is no known name is no gateway and holds
nothing, and its `models` are not read for the account. A bare name that begins `xai/` is the account's even where
a gateway of another id lists it, and that gateway's model is reached as `<id>/xai/<name>`.

**Why.** One model is reachable through more than one service, billed and credentialled differently,
and a bare name cannot say which was chosen. That mattered less while a gateway served only what a
settings file listed, because the file was the record of the choice. It decides correctness once a
roster is discovered rather than written down, since then nothing local can say which service a bare
name belonged to and a remembered choice would silently change service.

Sending only the remainder is what makes the qualified form usable at all: it is this system's own
filing, and the service has never heard of it. Comparing against the remainder too, because a reply
naming the model the gateway knows is the request working, and comparing against the qualified form
would report a substitution on every gateway turn. That is the same reasoning already applied to a
handle standing for whatever it resolves to.

Splitting once, rather than at every separator, because the remainder is the gateway's to spell and
most of those names contain one. A name whose leading segment matches no configured gateway is not a
qualified name at all, which is what keeps the other rosters' spellings out of this.

`verified-by: bravebot_config::lib::a_name_qualified_by_a_provider_id_names_the_gateway_and_the_model_separately`
`verified-by: bravebot_config::lib::a_bare_name_the_block_lists_still_finds_its_gateway`
`verified-by: bravebot_config::lib::a_name_no_gateway_was_configured_for_reaches_no_gateway`
`verified-by: bravebot_aichat::lib::only_the_name_the_gateway_knows_reaches_it`
`verified-by: bravebot_aichat::lib::a_qualified_name_still_finds_the_options_its_model_configured`
`verified-by: bravebot_tui::status::a_gateway_answering_under_its_own_name_is_not_a_substitution`
`verified-by: bravebot_config::lib::only_the_qualified_name_selects_the_chatgpt_account_and_only_the_rest_of_it_is_sent`
`verified-by: bravebot_config::lib::a_provider_entry_named_chatgpt_keeps_the_name_and_no_model_is_the_accounts`
`verified-by: bravebot_agent::backend::a_gateway_entry_named_chatgpt_keeps_its_name_and_no_model_is_the_accounts`
`verified-by: bravebot_config::lib::a_gateway_listing_a_name_that_begins_with_chatgpt_is_reached_by_its_qualified_name_alone`
`verified-by: bravebot_agent::backend::a_gateway_listing_a_chatgpt_name_does_not_take_it_from_the_account`
`verified-by: bravebot_config::lib::only_the_qualified_name_selects_the_xai_account_and_only_the_rest_of_it_is_sent`
`verified-by: bravebot_config::lib::a_provider_entry_named_xai_keeps_the_name_and_no_model_is_the_accounts`
`verified-by: bravebot_config::lib::an_entry_named_xai_that_states_no_endpoint_holds_nothing_and_its_models_are_not_read`
`verified-by: bravebot_config::lib::a_gateway_listing_a_name_that_begins_with_xai_is_reached_by_its_qualified_name_alone`

<a id="BACKEND-19"></a>
### BACKEND-19: a gateway that was told no models is asked what it serves

Where a gateway block names its models, those are what is offered and nothing is asked over the
network. Where it names none, the gateway itself is asked, and what it answers is offered. A Google
Vertex service has no listing to ask, and is offered the list [BACKEND-49](#BACKEND-49) compiles in
instead. A listing that cannot be read contributes nothing and takes nothing away from the rest of
the roster, and the picker says which gateway's could not be read, by the name its block gives it. The
terminal says it in a note before the picker draws, in the catalog's words, and adds the commands that name
one of its models, `bravebot -i "<prompt>" --model <id>/<model>` and `bravebot "<task>" --model <id>/<model>`.
The desktop says it in a warning that names the gateway and says to
try again, and gives no command line, since a window has none. A gateway whose block names a credential
nothing holds is not asked, and the picker does not say it could not be read, since `doctor` says the
credential is missing. A gateway whose block names its models is not asked and is not mentioned.

The xAI account's models are asked for beside a gateway's when a person signed in to xAI, and are said in the same two
places when the listing could not be had ([XAI-19](xai-sign-in.md#XAI-19)).

What the credential in use may reach is asked for ahead of what the service offers generally, and the
wider roster answers only where the narrower question does not. A block naming no credential has no
account for that narrower question to be about, so only the wider one is asked. Nothing is capped:
every model reported that can call tools is offered, ordered with the model a session would use first
and the rest by name.

**Why.** A block naming no models is the ordinary case, not a mistake: the tool this shape is borrowed
from resolves a roster from a registry, so the commonest block copied in names a credential and
nothing else. Offering nothing for such a block means a gateway configured exactly as that tool
configures it appears in a diagnostic and is unusable, which was the state this replaced.

A listing that could not be read is said, where it was once left silent, because a person who sees no
rows of a gateway they configured cannot tell a gateway that serves none from one that did not answer,
and the first is not something trying again changes. A missing credential is left to `doctor` and to the
turn that needs it, which already say it with the variable to set, where a note about a listing would
send somebody to try again.

Asking only where nothing was named is what keeps the block worth writing. A stated roster costs no
round trip and works with no network, which is the position somebody offline is in, and it stays the
way to pin a short list out of a service that offers hundreds.

The listing is content and the pick is routing, the same footing the roster from Brave's endpoint
arrives on: names are drawn for a person, that person chooses, and their choice is the endorsement for
the field it lands in. What may not come from a service is where the request went, and that is
configuration here rather than anything fetched.

Asking what the credential may reach is asking the question a person actually has. A model their key
cannot serve is a row that fails the moment it is picked, and the two answers differ by a factor of
three, so the wide roster is mostly rows that would not work. It is a fallback rather than the only
request because that narrower route is a gateway's own extension: one that does not answer it has to
end up with a roster anyway. With no credential named there is nothing for the narrow answer to be
narrower than, so that request spends a round trip to be told what the wide one says.

No cap, because a picker filters as somebody types and any limit is this system deciding they may not
choose a model their gateway serves. Ordering does that work instead, and it is needed precisely
because nothing is dropped: a roster arriving newest-first opens on models nobody has heard of and
buries the one in use. A configured roster is left alone, the file being the order somebody chose.

A window the gateway reports is taken, since it is the one fact about a fetched model nobody can type,
and a window the block stated outranks it as the figure somebody pinned deliberately. Failing both,
the same conservative default a stated roster gets.

**An `anthropic` service is asked once, for a whole page.** Its listing returns twenty models unless
the request names a page size, so the request names the most the service allows, and the route that
answers what the credential may reach, which the service does not have, is not asked. The key and the
version of the API travel as [BACKEND-57](#BACKEND-57) says. The listing states each model's window as
`max_input_tokens`, taken as a reported window is, and states under `capabilities` whether the model
reads an effort level, which [BACKEND-22](#BACKEND-22) reads. A value in either place that is absent,
null, zero or not a whole number states nothing, and a roster that is not in that shape for another
gateway is still read: what a gateway puts under `capabilities` is its own.

**An `openai` service is asked as any gateway is, and its listing is read for ids.** `/models/user` is asked
first and is expected to be refused, then `/models`. A row states an id and no capability, so nothing is dropped
for what a model can do, and embedding, speech and image models are offered beside the chat ones ([Known
costs](#known-costs)). A row states a `shutdown_date` for some models, and a model dated today or earlier is not
offered where the entry is reached at `api.openai.com`, in either protocol, since the service no longer serves it:
the date is read as its first second in UTC, and a row with no date, a `null` or a date in any shape but
`YYYY-MM-DD` is offered. Another host is not read for the field. One list compiled into the binary leaves ids
out: where an entry is reached in the chat completions protocol at `api.openai.com`, which an `openai` entry
is only where it names `@ai-sdk/openai-compatible` ([BACKEND-17](#BACKEND-17)), the models that take requests
through the Responses API alone are not offered, since that route cannot reach them. The list is read from two public documents: the `ResponsesOnlyModel` list
of OpenAI's published API description (version 2.3.0), and the model pages at
`developers.openai.com/api/docs/models/<id>`, read on 2026-10-27 for each id of the listing of 2026-10-26. On
2026-10-27 the route answered none of its 28 ids with a 200 when each was asked once with one short message and a
ceiling of 16 tokens, and named the Responses API in its refusal of three of them (the observations below). An id is
taken from a page when the page's Endpoints block marks `v1/chat/completions` unsupported and `v1/responses`
supported and marks no other endpoint supported but `v1/batch`, so a model that the Images or Audio endpoints also
serve is not on it. A dated snapshot is taken where its alias is taken, when the alias page lists the snapshot and
the snapshot has no page of its own. A developer reads the pages and writes the ids into the source, so a running program never reads a page, and a page
can change its markup or its statement without the list changing. A row's id is compared with that list only, and
the rows go to a picker. The place is told by the host and port of the entry's endpoint,
whatever the entry's id: the host is compared in lower case without a trailing dot, a port that is the scheme's
default is the same place whether or not it is written, and a port that is not the default is another service, so
it is not `api.openai.com`. A proxy keyed `openai` is therefore offered its whole listing, and so is an entry
reached in the Responses protocol ([BACKEND-61](#BACKEND-61)), which an `openai` entry at `api.openai.com` is
by default. The list changes
with a release of either document. A block naming `models` is offered those and no others, so a model on the list is
still reached by naming it.

**What is known of the exchange with OpenAI's own API.** [BACKEND-22](#BACKEND-22),
[BACKEND-31](#BACKEND-31) and [BACKEND-32](#BACKEND-32) add what bears on the effort level, on usage and on cache marks.

- Observed on 2026-10-26. A row of `GET /v1/models` held no capability, no parameter, no window and no modality, and
  the ids were chat, reasoning, embedding, speech, transcription, image, audio, realtime, moderation and video models.
  `GET /v1/models/user` answered 404 with the error type `invalid_request_error` and the code `model_not_found`.
- Observed on 2026-10-27, with a key whose account had credit. `GET /v1/models` answered 200 with an object holding
  `object` and `data`, and `data` held 134 rows: the 128 of the day before, and `gpt-4`, `gpt-4-0613`, `gpt-4-turbo`,
  `gpt-4-turbo-2024-04-09`, `o1-pro` and `o1-pro-2025-03-19`. Every row held an `id`, a `created` and an `owned_by`,
  and `shutdown_date` was a date on 82 rows and `null` or not present on 52, 36 of the dates being on or before
  2026-10-27. `owned_by` was `system` for 127 rows, `openai-internal` for 4 and `openai` for 3. `GET /v1/models/user`
  answered 404.
- Observed on 2026-10-27, one sample for each id. A chat completion with one user message (`Reply with the single word:
  ok`), `max_completion_tokens` of 16 and no stream, which is not the request this program sends, was sent once for each
  of the 134 listed ids and for the 12 ids of the compiled list that the listing does not hold, 146 requests. It answered:
  - 200 for 40 ids: `chat-latest`, `gpt-4.1`, `gpt-4.1-mini`, `gpt-4o`, `gpt-4o-mini`, `gpt-5`, `gpt-5-mini`,
    `gpt-5-nano`, `gpt-5-search-api`, `gpt-5.1`, `gpt-5.2`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.4-nano`, `gpt-5.5`,
    `gpt-5.6-luna`, `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-6-astra`, `gpt-6-luna`, `gpt-6-sol`, `gpt-6.1-sol`,
    `gpt-6.1-terra` and `o3`, with 16 dated snapshots of them. Ten of the 40 (`gpt-5`, `gpt-5-mini`, `gpt-5-nano`,
    `gpt-5.5` and `o3`, with their dated snapshots) stopped with the finish reason `length` and an empty content, all 16
    completion tokens having been spent as reasoning tokens, so the ceiling was reached before a word. The other 30
    stopped with `stop`.
  - 404 `invalid_request_error`, a null code and the param `model`, "This is not a chat model and thus not supported in
    the v1/chat/completions endpoint. Did you mean to use v1/completions?", for 35 ids: the transcription, speech,
    realtime, `gpt-live-` and moderation ids, `whisper-1`, `text-embedding-ada-002`, and `gpt-5.2-pro`, `gpt-5.4-pro`
    and `gpt-5.5-pro` with their dated snapshots.
  - 404 `model_not_found`, "The model `<id>` has been deprecated, learn more here", for 44 ids: 38 of the listed ids
    and 6 of the compiled list that the listing does not hold (`o3-deep-research*`, `o4-mini-deep-research*` and
    `computer-use-preview*`). 34 of the 38 hold a `shutdown_date` on or before 2026-10-27 and 4 hold none
    (`gpt-3.5-turbo-16k`, `gpt-3.5-turbo-instruct-0914`, `gpt-4o-mini-search-preview` and `gpt-4o-search-preview`). The
    four ids `gpt-3.5-turbo`, `o1`, `o3-mini` and `o4-mini` each hold the `shutdown_date` 2026-10-23. No row with a
    later date answered so.
  - 404 `model_not_found`, "The model `<id>` does not exist or you do not have access to it.", for the 6 ids of the
    compiled list that the listing does not hold and that are not in the group above: `o3-pro`, `o3-pro-2025-06-10`,
    `gpt-daybreak-blue-latest`, `gpt-daybreak-red-latest`, `gpt-5.6-cyber` and `gpt-rosalind-research`.
  - 404 with a null code and the param `model`, "This model is only supported in v1/responses and not in
    v1/chat/completions.", for `gpt-5-pro`, `gpt-5-pro-2025-10-06` and `gpt-image-1-mini`, and "This model is not
    supported in the v1/chat/completions endpoint. Use the v1/responses endpoint instead." for `gpt-5.3-codex`.
  - 400 `invalid_request_error`, the code `invalid_value` and the param `model`, "This model requires that either input
    content or output modality contain audio.", for `gpt-audio`, `gpt-audio-1.5`, `gpt-audio-2025-08-28`,
    `gpt-audio-mini` and `gpt-audio-mini-2025-12-15`.
  - 403 `invalid_request_error`, a null code, "You are not allowed to sample from this model", for
    `text-embedding-3-large` and `text-embedding-3-small`. The 19 requests that followed the first with the same key,
    one for each id, answered 403 for `text-embedding-3-small` and 404 for the other 18, so in these samples a 403 for
    one id did not refuse the key for the ids after it.
  - 503 `invalid_request_error`, "No available capacity was found for the model", for `gpt-image-2.5-flare`,
    `gpt-image-2.5-sunburst`, their two dated snapshots, `sora-2` and `sora-2-pro`, and 500 `server_error` for
    `chatgpt-image-latest`, `gpt-image-1.5`, `gpt-image-2` and `gpt-image-2-2026-04-21`.
- Observed on 2026-10-27, from the same requests. None of the 28 ids of the compiled list answered 200: 3 answered with
  Responses wording (`gpt-5-pro`, `gpt-5-pro-2025-10-06` and `gpt-5.3-codex`), 6 with "not a chat model" (the `-pro`
  ids), 13 with "deprecated" and 6 with "does not exist or you do not have access". 16 of the 28 are in the listing.
  An id the list leaves out, `gpt-image-1-mini`, answered with the Responses wording. None of the 40 ids that answered
  200 is on the list. Of the 118 listed ids that the list leaves in, 40 answered 200, 61 answered 404 (29 "not a chat
  model", 31 "deprecated" and `gpt-image-1-mini`), and 17 answered 400, 403, 500 or 503. Of the 36 ids with a
  `shutdown_date` on or before 2026-10-27, 34 answered "deprecated" and `sora-2` and `sora-2-pro` answered 503.
- Not shown: a 200 for an audio id, which needs `modalities`; whether the 500 and 503 answers of the image and video ids
  are about the route; a request that carries tools; whether an id that answered 200 answers the request this program
  sends, which has no ceiling; a reply of any length from the ten ids that stopped at the ceiling of 16 (`gpt-5-mini`
  and `gpt-5.5` completed replies to the requests below, and `gpt-5`, `gpt-5-nano` and `o3` with their dated snapshots
  were asked once); and why `o3-pro`, which a model page marks as Responses-only, answered that it does not exist or
  the key has no access to it.
- Stated by OpenAI's published API description (version 2.3.0): a listed model has `id`, `created`, `object`,
  `owned_by` and an optional `shutdown_date`, and the description has no `/models/user` route. A chat completion
  request takes `max_completion_tokens` and `reasoning_effort` (`none`, `minimal`, `low`, `medium`, `high`, `xhigh` or
  `max`, of which not every reasoning model supports every value), and `max_tokens` is deprecated and not compatible
  with the o-series. The models of its `ResponsesOnlyModel` list belong to the Responses request alone.
- Stated by OpenAI's model pages (read on 2026-10-27), and not observed: the Endpoints block of the page of
  `gpt-5.1-codex`, `gpt-5.1-codex-mini`, `gpt-5.2-codex`, `gpt-5.3-codex`, `gpt-5.2-pro` and `gpt-5.4-pro` marks
  `v1/chat/completions` unsupported and `v1/responses` supported, as the pages of `gpt-5-codex`, `gpt-5-pro`,
  `gpt-5.1-codex-max`, `gpt-5.5-pro`, `o3-pro` and `o1-pro` do for models the description lists, and the pages of
  `gpt-5.4`, `gpt-4o-mini` and `o3` mark both supported. The page of `gpt-5.1-codex` also says it is available in
  the Responses API only. The page of `gpt-image-1` marks chat completions unsupported and Responses, Batch and the
  Images endpoints supported, so it is not on the list. The dated ids `gpt-5.2-pro-2025-12-11` and
  `gpt-5.4-pro-2026-03-05` have no page and are listed under Snapshots on the page of their alias. Of the 128
  ids the listing held on 2026-10-26, 81 have a page and 47, most of them dated snapshots, do not.
- Assumed: that a model the listing holds later, that is not on the compiled list, is served by chat completions where
  the account may use it. The 118 listed ids that the list leaves in were each asked once on 2026-10-27. 40 answered
  200, and of the 78 that did not, 29 answered that the id is not a chat model, 31 that it is deprecated, and 18 with
  the Responses wording, 400, 403, 500 or 503. Being listed and absent from the compiled list did not say that the
  route serves a model.

**The ChatGPT account is asked as [CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9) says,** and only where a sign-in
is stored. It is a roster beside the others, its rows are keyed by its own identifier, and a listing that cannot
be had contributes the compiled names of [CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9) and takes nothing away from
the rest. A listing refused in a code that names the account contributes nothing. The listing is asked of
the account, so a model it does not list is not offered. Where an entry named `chatgpt` that states no
endpoint names models, those are what is offered and nothing is asked, as for a gateway, and the access
token is not renewed for it.

`verified-by: bravebot_aichat::models::a_gateway_roster_is_offered_under_names_that_say_which_gateway_serves_them`
`verified-by: bravebot_aichat::models::a_window_a_gateway_reports_is_taken_from_the_listing`
`verified-by: bravebot_aichat::models::a_fetched_model_with_no_reported_window_gets_the_assumed_one`
`verified-by: bravebot_aichat::models::a_window_the_block_stated_outranks_the_one_reported`
`verified-by: bravebot_aichat::models::a_gateway_model_that_cannot_call_tools_is_not_offered`
`verified-by: bravebot_aichat::models::a_gateway_that_reports_no_capabilities_still_offers_its_models`
`verified-by: bravebot_aichat::client::an_openai_listing_that_reports_no_capabilities_offers_every_model_it_lists`
`verified-by: bravebot_aichat::models::an_openai_listing_is_not_offered_the_models_only_its_responses_api_takes`
`verified-by: bravebot_aichat::models::an_openai_listing_is_not_offered_a_model_on_or_past_its_shutdown_date`
`verified-by: bravebot_aichat::models::a_shutdown_date_takes_effect_at_the_start_of_its_day`
`verified-by: bravebot_aichat::models::a_shutdown_date_is_read_at_openais_own_api_and_nowhere_else`
`verified-by: bravebot_config::provider::openais_own_api_follows_the_host_a_request_goes_to_whatever_the_entry_is_called`
`verified-by: bravebot_config::provider::the_spellings_of_openais_host_are_its_own_api_and_another_host_or_port_is_not`
`verified-by: bravebot_config::provider::the_responses_only_list_holds_the_marked_ids_and_leaves_out_a_sample_that_stays_offered`
`verified-by: bravebot_aichat::models::a_fetched_entry_with_no_usable_name_is_dropped`
`verified-by: bravebot_aichat::client::an_anthropic_listing_is_asked_for_once_as_a_whole_page_with_the_key_in_its_header`
`verified-by: bravebot_aichat::models::an_anthropic_listing_is_offered_under_the_providers_id_with_the_window_it_states`
`verified-by: bravebot_aichat::models::an_anthropic_window_of_zero_or_nothing_is_the_assumed_one`
`verified-by: bravebot_aichat::models::a_gateway_stating_capabilities_in_another_shape_still_has_its_roster_read`
`verified-by: bravebot_aichat::models::a_window_in_a_form_that_is_not_a_whole_number_leaves_the_roster_readable`
`verified-by: bravebot_aichat::models::fetched_gateway_models_are_not_marked_premium`
`verified-by: bravebot_aichat::client::a_gateway_with_a_credential_is_asked_what_that_account_may_reach`
`verified-by: bravebot_aichat::client::a_gateway_needing_no_credential_is_asked_for_its_roster_unauthenticated`
`verified-by: bravebot_aichat::client::a_gateway_that_cannot_answer_the_account_question_is_asked_for_its_whole_roster`
`verified-by: bravebot_aichat::client::a_responses_entry_is_asked_what_its_key_may_reach_before_its_whole_roster`
`verified-by: bravebot_tui::app::a_fetched_roster_leads_with_the_model_in_force`
`verified-by: bravebot_tui::app::a_fetched_roster_nobody_has_chosen_from_is_still_sorted`
`verified-by: bravebot_agent::chatgpt::a_signed_in_account_is_offered_what_the_service_lists_under_its_own_identifier`
`verified-by: bravebot_tui::app::the_accounts_rows_are_met_in_reading_order_listed_or_compiled_and_nothing_else_adds_one`
`verified-by: bravebot_tui::app::the_picker_says_which_gateway_could_not_be_listed_and_says_nothing_of_the_rest`
`verified-by: bravebot_ui_bridge::models::the_window_says_which_gateway_could_not_be_listed`
`verified-by: bravebot_ui_bridge::models::the_window_lists_an_openai_entry_with_its_key_and_offers_every_row_under_openai`
`verified-by: bravebot_agent::chatgpt::an_unreadable_listing_is_told_apart_from_an_empty_one`
`verified-by: bravebot_agent::chatgpt::the_models_an_entry_names_are_offered_in_place_of_the_listing_and_need_no_token`

<a id="BACKEND-20"></a>
### BACKEND-20: how hard to think is carried, never inferred

A request says how hard the model should think only where a turn was given a level. Nothing derives
one from the prompt, from how long the conversation is, from which tools are offered, or from what a
previous turn cost. Where no level was given the field is absent and the service applies its own
default, so a build nobody has asked sends the body it always sent.

**Why.** How hard to think is a bill, and inferring one is this program spending somebody's money on
a guess about work it has not done yet. The absent case is what keeps the field honest: an endpoint
that has never seen it is not sent it, so adding it cannot break a service that would reject it.

A level is not content. It comes from a person picking off a list they read, on the footing the
model name beside it arrives on, and a word that names no level is no choice at all rather than a
choice of something.

**A ChatGPT account is sent one level for a whole session.** The level in force when a session begins is the
one the account is sent for all of it ([CHATGPT-10](chatgpt-sign-in.md#CHATGPT-10)): a request that asks for
a level carries that one, in the field [BACKEND-21](#BACKEND-21) states, whatever was chosen afterwards, and
a request that asks for none carries none. What is chosen applies to the account from the next session, and
to every other service at once. A model of the account that refuses a request carrying the level is the
exception, as [BACKEND-22](#BACKEND-22) has it for any model: that refusal outranks the level the session
began with, and no later request carries a level to that model.

`verified-by: bravebot_aichat::protocol::a_request_nobody_asked_a_level_of_mentions_no_effort`
`verified-by: bravebot_aichat::protocol::a_word_naming_no_level_is_not_a_choice`
`verified-by: bravebot_aichat::protocol::a_level_is_named_whatever_case_it_was_written_in`
`verified-by: bravebot_bedrock::protocol::a_request_nobody_asked_a_level_of_carries_no_output_config`
`verified-by: bravebot_agent::turn::without_a_chosen_effort_no_level_is_requested`
`verified-by: bravebot_aichat::client::a_session_held_to_the_level_it_began_with_sends_that_level_whatever_a_request_asks_for`
`verified-by: bravebot_tui::app::a_chatgpt_session_keeps_the_level_it_began_with_and_says_so_when_another_is_chosen`
`verified-by: bravebot_aichat::client::an_account_request_carries_the_level_it_was_given_as_reasoning_effort_and_none_it_was_not`


<a id="BACKEND-21"></a>
### BACKEND-21: each service is sent the level in its own protocol's field

One word ranks the levels, and where that word goes in the body is the wire protocol's business. No
service is sent the other's shape.

| Service | Where the level is sent |
|---|---|
| The aichat endpoint Brave runs | `reasoning_effort`, beside the model |
| An OpenAI-compatible gateway | `reasoning_effort`, beside the model |
| AWS Bedrock | `effort`, inside `output_config`, among the fields handed to the model unread |
| Anthropic's Messages API | `effort`, inside `output_config`, a field of the request body |
| Google's `generateContent` API | `thinkingBudget`, inside `thinkingConfig`, inside `generationConfig`, as a token count that rises with the level |
| OpenAI's Responses API, and the ChatGPT account | `effort`, inside `reasoning`, beside a `summary` of `auto` ([BACKEND-61](#BACKEND-61), [CHATGPT-10](chatgpt-sign-in.md#CHATGPT-10)) |

This says where a level is sent, not what becomes of it. What a service does with the field is that
service's own behaviour, observable only by measuring it, and one of them is known to discard it
altogether. That is recorded under Known costs rather than here, because a clause pinned to a remote
service's current behaviour is a clause that goes stale without anything in this repository changing.

**Why.** The two protocols state the same idea differently, and the conversion between them already
happens in one place for every other field. Sending one service the other's shape is a field it
certainly does not read.

`verified-by: bravebot_aichat::protocol::a_level_is_sent_in_the_name_this_protocol_gives_the_field`
`verified-by: bravebot_bedrock::protocol::a_level_is_sent_inside_the_object_this_api_states`
`verified-by: bravebot_aichat::anthropic::protocol::a_level_is_sent_inside_the_object_this_api_states`
`verified-by: bravebot_aichat::generate_content::protocol::a_ceiling_a_level_and_a_schema_go_in_the_generation_config`
`verified-by: bravebot_aichat::responses::protocol::reasoning_is_the_level_and_a_summary_only_where_a_level_was_given`
`verified-by: bravebot_aichat::client::an_account_request_carries_the_level_it_was_given_as_reasoning_effort_and_none_it_was_not`
`verified-by: bravebot_agent::turn::a_chosen_effort_is_the_one_requested`


<a id="BACKEND-22"></a>
### BACKEND-22: a level is sent only where the roster says it is read

Where the listing describing the model in force states which parameters it takes and an effort level
is not among them, no level is sent and the person is told the model reads none. The choice itself is
kept: it applies again the moment a model that reads one is chosen, so what a request carries does
not depend on the order two commands were typed in.

A model the listing does not describe is not a model stated to read nothing. A name that came from a
settings file, a roster that reports no parameters at all, and a listing that could not be fetched
all leave the level to go out and be judged at the far end.

Anthropic's listing states the capability per model, as an object with a `supported` flag under
`effort`, and only a `false` there says the model reads none. A model whose entry omits the object,
or states it as anything else, is a model the listing does not describe.

The xAI account is sent no level and is reported as reading none ([XAI-16](xai-sign-in.md#XAI-16)). Its listing
states `reasoning_effort` levels for some models in a field this clause does not read for a gateway, and the
service was not asked with a level.

The ChatGPT account's listing states it per model in `supported_reasoning_levels`
([CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9)). A row that lists levels reads one, a row whose list is empty
reads none, and a row that does not mention the field, or states it as anything but a list, is a model the
listing does not describe. A model an entry named `chatgpt` names, and one of the compiled names, is described
by no listing.

**Why.** A service that reads the field and one that discards it are indistinguishable from the
outside: both answer, and the reply of a model that ignored the level looks exactly like the reply of
one that honoured it. So a level sent where it is not read is a charge somebody chose and did not
get, reported to them as in force. Where a roster answers the question there is no reason to guess,
and where it does not, withholding what somebody asked for on the strength of a listing that never
mentioned the subject would be deciding against them from silence.

**Where there is no listing, a refusal is the answer.** Neither AWS Bedrock nor a settings block
naming its models says which parameters a model takes, and a fetched roster need not say either, so
where nothing describes the field the level goes out to be judged. A model that refuses the field has
answered the same question the listing answers elsewhere: no later request carries a level to it, and
it is reported as reading none rather than as having one in force. What one model refused says nothing
about another, and a request refused with the level gone as well settles nothing and is not
remembered, that status being also what a prompt too long for the model comes back as.

**A refusal outranks a listing that names the field.** A row stating the parameter among those its
model takes is what the service says it would do, and a status refusing the request that carried one
is what it did. Where the two disagree the request is the answer: the model is reported as reading
none, and no later request carries a level to it.

**A session asks again after every turn.** A refusal is learned in the middle of one, so asking the
roster when a session opens and again only when somebody chooses another model leaves the level
reported as in force for the rest of that session after the requests carrying one have stopped. Each
turn ends by asking what the service has refused for the model in force, which costs nothing over
the wire: the answer is what a request already sent came back with. It only ever takes the level
away, a listing that stated its parameters and did not name the field having said the model reads
none already, and a turn that was refused nothing does not say otherwise. The person is told on the
turn that learned it and not again, the condition holding for the rest of the session. A one-shot
run asks once, before its turn, there being no later request for the answer to govern and nobody
left to report it to.

**A level a block wrote down is not this program's to give up.** BACKEND-15 carries a configured
model's options into the body as they stand, so a level written there fills the field again after this
concession has been given up, and a service that refuses it refuses the request as it would refuse
any other option it does not take. What is given up is the level somebody chose in the interface,
that being the one this program decided to send.

**On a gateway, the level is the last concession given up.** A request to a gateway other than
OpenAI's own API also carries cache breakpoints nobody asked for, and either field is refused with
the same status, so the two are given up in order: a request still marking a prefix is sent again without the marks first, and the
level goes only where that request is refused too. Giving up the level first would read a refusal of
the caching as the model refusing to be told how hard to think, and stop sending a level to a model
that reads one. Where the request that answered had given up both, both are remembered, the status
naming no field: a service that reads a breakpoint and refuses a level gives up the caching as well
for the life of the process.

**At OpenAI's own API, the level is the one concession.** A request to `api.openai.com` carries no
breakpoints from the first request ([BACKEND-32](#BACKEND-32)), so the breakpoints are already given up
and the level is the only field this program gives up. A refusal does not say which field it is about: a field
the service does not know is refused with the same status and error type as a level, and is told from one only by
the request being refused again without the level, which settles nothing and is not remembered. A request refused
on its contents is sent again once, without the level, and then the refusal is reported. A request that carried no
level is reported as it was refused. A proxy keyed `openai` is sent the breakpoints and is a gateway as the paragraph above says.

**What is known of a level at OpenAI's own API.** [BACKEND-19](#BACKEND-19) holds what OpenAI's published API
description states of the field.

- Observed on 2026-10-27, one sample for each model and level, in a chat completion that was not streamed, with one
  user message and a `max_completion_tokens` of 64 or 800. Two models that do not reason refused the field:
  `gpt-4o-mini` for `none` and `low`, and `gpt-4.1-mini` for `low`, answered 400 with the error type
  `invalid_request_error`, a null `code`, a null `param` and the message "Unrecognized request argument supplied:
  reasoning_effort". Three reasoning models refused a level they do not take with 400, the error type
  `invalid_request_error`, the code `unsupported_value`, the `param` `reasoning_effort` and a message that names the
  levels they take ("Unsupported value: 'reasoning_effort' does not support 'xhigh' with this model. Supported values
  are: ..."). `gpt-5.4-mini` and `gpt-5.6-luna` took `none`,
  `low`, `medium`, `high` and `xhigh` and refused `minimal` and `max`. `gpt-5-mini` took `minimal`, `low`, `medium` and
  `high` and refused `none`, `xhigh` and `max`. A level that was taken answered 200, and the reasoning tokens reported
  were 0 for most of them (10, 12 and 10 for `medium`, `high` and `xhigh` on `gpt-5.4-mini`, and 64 for `high` on
  `gpt-5-mini`), so a taken level is not a measured one. Of the levels this program sends (`low`, `medium`, `high`,
  `xhigh` and `max`), `xhigh` was refused by one of the three reasoning models and `max` by all three.
- Observed on 2026-10-27, through this program. `bravebot -p` with `--effort high` on `openai/gpt-4o-mini` and with
  `--effort xhigh` on `openai/gpt-5-mini` each exited 0 and printed a reply, and the diagnostic log of each held one
  refused request with the status 400.
- Not shown: a level of `max` taken by any model, the levels of a model that was not tried, and a level in a streamed
  request.

**On Vertex express mode, the level is the one concession.** The request carries no breakpoints, so
a request refused on its contents is sent again once without the level and not again, and a request
that carried no level is reported as it was refused.

**On Bedrock, four fields are given up in order.** A Bedrock request carries cache breakpoints too, a
streamed one asks for the model's thinking as it happens ([BACKEND-46](#BACKEND-46)), and a streamed
one offering a tool asks for its arguments as they are written ([BACKEND-44](#BACKEND-44)). There
the breakpoints are refused with a status of their own, the one an expired credential also gets, so
they are given up on that status and never for a refusal of any field. The ask to show thinking,
the level, the assumed ceiling of [BACKEND-41](#BACKEND-41) and the ask for arguments share one
status, so they are given up in that order, each only where the request is refused again without the
one before. Each is given up only by a request that carried it, so a request that carried no level
never records the model as refusing one, and a stated ceiling is never given up. The order keeps
what a model does read, measured: Claude Sonnet 5 refuses the ask to show thinking and reads the
level, so the ask goes first, where the other order would cost Sonnet its level and still be
refused; Claude Haiku 4.5 refuses both and takes the ask for arguments, and giving that up first
would cost it the field that keeps a long argument from being cut off and still be refused. The
ceiling goes before the ask for arguments because, given up wrongly, it costs a model its replies
longer than the fallback, where the ask costs it every long argument, written into a silence the
idle bound cuts. Where the request that answered had given up more than one, all of them are
remembered, as on a gateway. A request refused with nothing left to give up settles nothing, and
what an earlier request settled for that model stands.

**Why.** The judgment is the only description these services offer, and throwing it away leaves the
interface reporting a charge somebody chose and stopped getting, which is the thing this clause
exists to prevent. It is also all that stands between a service that refuses the field and a
conversation in which no turn can succeed, the level being in every request such a turn makes.
Learned rather than declared because an inference-profile ARN does not say which provider is behind
it, and a settings file cannot state what its author does not know either.

`verified-by: bravebot_bedrock::lib::what_a_model_refused_outlives_the_client_that_found_out`
`verified-by: bravebot_bedrock::lib::a_probe_that_settled_nothing_is_not_remembered`
`verified-by: bravebot_bedrock::lib::one_model_refusing_says_nothing_about_another`
`verified-by: bravebot_bedrock::lib::the_level_is_given_up_before_the_ask_for_arguments_as_written`
`verified-by: bravebot_bedrock::lib::the_ask_to_show_thinking_is_given_up_before_the_level`
`verified-by: bravebot_bedrock::lib::a_refused_ask_for_arguments_as_written_is_given_up_and_remembered`
`verified-by: bravebot_bedrock::lib::a_probe_that_settled_nothing_leaves_what_was_known`
`verified-by: bravebot_aichat::client::a_level_a_gateway_refuses_costs_the_field_and_not_the_turn`
`verified-by: bravebot_aichat::client::a_gateway_that_refused_a_level_is_not_sent_one_again`
`verified-by: bravebot_aichat::client::a_level_refusal_the_retry_did_not_fix_is_not_remembered`
`verified-by: bravebot_aichat::client::one_model_refusing_a_level_says_nothing_about_another_on_the_same_gateway`
`verified-by: bravebot_aichat::client::a_level_a_block_wrote_down_is_not_given_up`
`verified-by: bravebot_aichat::client::a_level_refused_in_either_body_openai_answers_with_is_given_up_and_the_turn_completes`
`verified-by: bravebot_aichat::client::a_refusal_naming_another_field_does_not_make_the_level_one_a_model_refuses`
`verified-by: bravebot_aichat::lib::a_refusal_at_openais_own_api_gives_up_the_level_and_nothing_else`
`verified-by: bravebot_aichat::client::a_model_that_refuses_the_thinking_budget_is_asked_again_without_it`
`verified-by: bravebot_aichat::client::a_refusal_of_a_request_asking_for_no_level_is_reported_after_one_request`
`verified-by: bravebot_aichat::call::the_order_fields_are_given_up_in_is_the_callers`
`verified-by: bravebot_aichat::call::a_probe_that_failed_as_well_remembers_nothing_and_restores_what_was_known`
`verified-by: bravebot_aichat::lib::a_model_whose_service_refused_a_level_is_reported_as_reading_none`
`verified-by: bravebot_aichat::models::a_row_whose_service_refused_a_level_reads_none_however_silent_the_roster`
`verified-by: bravebot_aichat::models::a_row_the_service_refused_reads_none_however_loudly_the_roster_advertises_it`
`verified-by: bravebot_aichat::models::a_brave_roster_row_the_endpoint_refused_reads_none`
`verified-by: bravebot_aichat::models::a_gateway_model_that_does_not_take_the_effort_parameter_says_so`
`verified-by: bravebot_aichat::models::an_anthropic_model_the_listing_says_reads_no_effort_says_so`
`verified-by: bravebot_aichat::models::an_account_row_reads_an_effort_level_only_where_the_listing_does_not_say_it_takes_none`
`verified-by: bravebot_aichat::client::an_account_row_whose_levels_are_not_a_list_is_not_one_that_says_it_takes_none`
`verified-by: bravebot_aichat::models::the_models_an_entry_names_are_rows_under_the_accounts_identifier_with_the_window_it_states`
`verified-by: bravebot_aichat::models::the_compiled_list_is_rows_under_the_accounts_identifier_one_to_a_name`
`verified-by: bravebot_aichat::models::a_gateway_that_states_no_parameters_is_not_taken_to_read_no_level`
`verified-by: bravebot_cli::running::a_run_withholds_a_level_the_roster_says_the_model_does_not_read`
`verified-by: bravebot_cli::running::a_run_sends_a_level_the_roster_says_the_model_reads`
`verified-by: bravebot_cli::running::a_session_in_lines_withholds_a_level_the_roster_says_the_model_does_not_read`
`verified-by: bravebot_cli::running::a_service_that_refuses_the_level_it_advertised_is_reported_as_reading_none`
`verified-by: bravebot_bedrock::lib::a_model_that_refused_the_level_once_is_remembered_as_refusing_it`
`verified-by: bravebot_agent::backend::before_any_refusal_every_service_reads_the_level`
`verified-by: bravebot_agent::backend::a_level_one_gateway_refused_is_not_refused_at_another_service_with_the_slug`
`verified-by: bravebot_agent::backend::a_level_the_default_endpoint_refused_is_reported_for_its_model_only`
`verified-by: bravebot_agent::backend::a_level_the_chatgpt_account_refused_is_reported_for_its_model_only`
`verified-by: bravebot_agent::backend::a_model_a_bedrock_account_answers_for_is_not_asked_of_the_default_endpoint`
`verified-by: bravebot_agent::backend::a_level_the_default_endpoint_refused_is_not_refused_at_a_gateway_offering_the_bare_slug`
`verified-by: bravebot_agent::backend::a_model_bedrock_serves_is_asked_of_bedrock_and_not_of_a_gateway_with_the_same_name`
`verified-by: bravebot_agent::backend::a_level_the_account_refused_is_reported_for_its_model_only`
`verified-by: bravebot_agent::backend::a_level_a_bedrock_model_refused_is_reported_for_that_model_only`
`verified-by: bravebot_tui::app::a_level_is_withheld_from_a_model_that_reads_none`
`verified-by: bravebot_tui::app::a_level_a_model_cannot_use_is_kept_rather_than_forgotten`
`verified-by: bravebot_tui::app::asking_for_a_level_a_model_cannot_use_says_so`
`verified-by: bravebot_tui::app::a_model_the_listing_did_not_describe_still_takes_a_level`
`verified-by: bravebot_tui::app::a_default_model_the_roster_says_reads_no_level_is_withheld_one_when_nothing_is_picked`
`verified-by: bravebot_tui::app::a_level_the_service_refused_stops_being_reported_as_in_force`
`verified-by: bravebot_tui::app::a_turn_that_refused_nothing_does_not_restore_a_level_the_roster_says_is_unread`
`verified-by: bravebot_tui::status::a_level_the_model_does_not_read_is_reported_as_unread`
`verified-by: bravebot_agent::backend::the_xai_account_reads_no_level_is_one_service_and_is_asked_for_the_name_after_its_identifier`


<a id="BACKEND-23"></a>
### BACKEND-23: a profile that is not configured is said so, never signed in to

Before a sign-in is attempted for a named profile, the tool is asked which profiles it has. Where it
answers and the name is not among them, no sign-in runs and the person is told the name is not
configured, along with the ones that are. Where no profile was named, or the tool could not be asked,
the sign-in goes ahead.

The question is asked of the tool as a list, never inferred from the wording of the failure that
prompted it, and only once something has already failed, so establishing that a session is good still
costs one call and no more.

**Why.** No sign-in fixes a profile that does not exist: it fails for the same reason the export did,
which spends a browser on a certainty and then replaces the real diagnosis with "the sign-in did not
complete". The remedy is also different, and advice to run `aws sso login` is advice that cannot
work, so a person following it learns nothing. Naming the profiles that do exist is what turns the
report into something actionable, since the usual cause is a name that was right on another machine.

Reading the failure's wording would answer the same question, and is how this must not be done: the
wording belongs to a tool that may change it, and being wrong costs either a browser opened for
nothing or somebody told their configuration is broken when their session merely expired.

`verified-by: bravebot_bedrock::credentials::a_profile_the_cli_does_not_have_is_not_signed_in_to`
`verified-by: bravebot_bedrock::credentials::a_profile_the_cli_does_have_still_gets_a_sign_in`
`verified-by: bravebot_bedrock::credentials::a_listing_that_could_not_be_read_does_not_withhold_a_sign_in`
`verified-by: bravebot_bedrock::credentials::naming_no_profile_is_not_naming_a_missing_one`
`verified-by: bravebot_bedrock::credentials::a_machine_with_no_profiles_at_all_says_so`


<a id="BACKEND-24"></a>
### BACKEND-24: settings layers resolve a name at a time, closest first

Settings are read from three files: `settings.json` in the user's own directory, then
`settings.json` in a `.bravebot` directory beside the work, then `settings.local.json` beside that
one. A later file overrides an earlier one per name rather than wholesale, so a file setting one thing
leaves everything else in force.

A file the command line named is read after all three, by these same rules, so what it sets beats
every file that was found. The flag that names one, and the path it refuses, are in
[cli.md](cli.md).

| What | How layers combine |
|---|---|
| `env`, `keybindings`, `search` | per name, one level down; the value under a name is replaced whole |
| the fifteen `env` names of the network, Google and Bedrock sets ([BACKEND-33](#BACKEND-33)) | home layer, and the file `--settings` names when it is outside the checkout; a project or local layer naming one is ignored and reported by `doctor` |
| `run.scrubEnv`, every list under `permissions`, `mcp.request` | every layer's entries are kept |
| `provider`, `model`, `advisorModel`, `fallbackModel`, `summaryModel`, `planModel`, `processorModel`, `commitMessage`, `attribution` | home layer, and the file `--settings` names; a project or local layer naming any of them is ignored, and the file is reported as one whose naming was dropped |
| anything else | the closest layer that set it wins |

A layer that spells `permissions` or `run` as something other than an object, or `run.scrubEnv` or
a list under `permissions` as something other than an array, sets nothing there: the weaker layers'
block or list stays in force.

The project layers are read from the directory the process started in and no ancestor of it. Each
layer fails independently: one that is missing, larger than 64 KB, or unparseable leaves the others
in force.

**Why.** An account is not the only scope a value belongs to. A credential profile is a property of
the person, the gateway a particular checkout talks to is a property of that checkout, and something
one machine needs is neither, so a single file makes one of those three overwrite the others.
Overriding per name is what makes putting one value in a checkout worth doing, since the alternative
is restating an entire configuration to change a host. Going deeper than a name would make one
request's destination the product of two files with no single place to read that says where it goes,
which is why a gateway entry is replaced whole and a project file naming one must name its host too.

`provider`, `model`, `advisorModel`, `fallbackModel`, `summaryModel`, `planModel` and `processorModel` are the exception on the other side: which host every request goes to,
which credential signs it and which headers it carries is exactly what a file nobody opened must not
decide, and the credential value comes from the person's own environment under names the layer would
choose. A `headers` block is inside `provider`, so dropping the block drops it
([BACKEND-60](#BACKEND-60)). A project or
local layer naming one is dropped whole and reported, so the destination of every request stays
something the person's own files or command line named. The file `--settings` names is the person's
own act and is read.

`attribution` is the exception for the reason [BACKEND-30](#BACKEND-30) gives: its free text reaches
the planner on every round, so a project or local layer carrying one is dropped whole and reported.
Between the two files that may write it, the two names under it combine per name for the same
reason `env` does: they are unrelated destinations that happen to share a block, and a file
answering for one must not answer for the other by omission. The chords under `keybindings` and the two caps under `search` are the
same case: a file moving one action's key is no statement about the other six, and a checkout
widening a search's walk for its own size is none about how long a read may take.

The lists are the exception because an entry in one only ever narrows what is possible: a name under
`scrubEnv` takes a variable away from a subprocess, and a rule under `permissions` refuses something
that was otherwise allowed. Overriding either would let a layer hand back what a weaker one withheld,
and a permission removed by a file somebody did not open is the one outcome worth ruling out. A value
of another shape names no entry, so letting it replace a list would remove every entry under it and
add none. A model is one choice rather than a list, so it resolves like any other single value, and
so does any other name in those blocks whatever shape a weaker layer gave it.

`mcp.request` is kept from every layer for a different reason that arrives at the same rule. An entry
names a server a person must already have declared in their own directory and then approved, so a
layer adding one widens nothing, and a layer replacing another's would drop a server a checkout
asked for without saying so ([SERVERS-2](mcp-servers.md#SERVERS-2)).

Searching upward for the project layer is what this declines to do, because then what configures a
session would depend on which directory somebody happened to change into, and the file found could sit
above the thing being worked on. Refusing the whole stack over one bad layer is the other thing it
declines: a mistake in a checkout must not decide that somebody's own profile no longer applies.

The bound is there because these files are a handful of short strings and a session must start
without waiting on one. A file that grew by accident, or that is not a settings file at all, is
skipped rather than parsed, and 64 KB is far above anything a person writes by hand.

The order and the merge rules are Claude Code's, down to the name `settings.local.json`, so that
knowing where to put a value for one tool is knowing it for the other.

The file a command line named is a fourth scope rather than a fourth place to look: it is a property
of the invocation, which none of the three is, and it is above them because naming a file explicitly
is a stronger statement than finding one where it was looked for. It fails independently as they do,
so a named file that has gone missing under a running process does not take a person's own profile
with it.

`verified-by: bravebot_config::settings::a_project_layer_overrides_a_name_the_global_one_set`
`verified-by: bravebot_config::settings::a_name_only_the_global_layer_set_survives_a_project_layer`
`verified-by: bravebot_config::settings::the_local_layer_beats_the_one_a_checkout_carries`
`verified-by: bravebot_config::settings::every_layer_adds_to_the_names_kept_from_a_program`
`verified-by: bravebot_config::settings::every_layer_adds_to_the_permission_rules`
`verified-by: bravebot_config::settings::a_layer_that_is_not_a_list_of_rules_takes_no_rule_away`
`verified-by: bravebot_config::settings::a_layer_that_is_not_a_list_of_names_takes_no_name_away`
`verified-by: bravebot_config::settings::a_weaker_layer_spelling_a_mode_as_a_list_does_not_outrank_a_stronger_one`
`verified-by: bravebot_cli::running::doctor_keeps_the_home_rules_under_a_checkout_block_that_is_not_an_object`
`verified-by: bravebot_config::settings::every_layer_adds_to_the_directories_a_file_makes_reachable`
`verified-by: bravebot_config::settings::every_layers_request_is_read_and_each_alias_is_kept_once`
`verified-by: bravebot_config::settings::a_project_layer_cannot_pick_a_backend`
`verified-by: bravebot_config::settings::a_project_provider_block_leaves_the_home_ones_alone`
`verified-by: bravebot_config::settings::a_named_layer_picks_a_backend`
`verified-by: bravebot_config::settings::a_project_layer_cannot_smuggle_a_provider_env_name`
`verified-by: bravebot_config::settings::a_project_layer_cannot_pick_a_model`
`verified-by: bravebot_config::settings::a_project_model_leaves_the_default_in_force`
`verified-by: bravebot_config::settings::a_layer_above_the_home_one_does_not_pick_a_model`
`verified-by: bravebot_config::settings::a_layer_above_that_names_nothing_does_not_outrank_a_saved_pick`
`verified-by: bravebot_cli::running::doctor_says_a_checkouts_provider_block_is_not_obeyed`
`verified-by: bravebot_config::settings::a_layer_answering_for_one_attribution_name_leaves_the_other`
`verified-by: bravebot_config::settings::a_layer_capping_one_side_of_a_search_leaves_the_other`
`verified-by: bravebot_config::settings::a_layer_naming_no_model_leaves_the_one_below_it`
`verified-by: bravebot_cli::running::doctor_says_a_checkouts_advisor_model_is_not_obeyed`
`verified-by: bravebot_config::settings::a_project_or_local_layer_cannot_name_an_advisor`
`verified-by: bravebot_config::settings::an_unparseable_project_layer_leaves_the_global_one_in_force`
`verified-by: bravebot_config::settings::an_oversized_project_layer_leaves_the_global_one_in_force`
`verified-by: bravebot_config::settings::a_project_layer_that_is_a_device_is_skipped`
`verified-by: bravebot_config::settings::a_read_stops_one_byte_past_the_bound`
`verified-by: bravebot_config::settings::a_directory_with_no_project_layer_reads_the_global_one_alone`
`verified-by: bravebot_config::settings::a_command_line_file_adds_to_the_names_kept_from_a_program`
`verified-by: bravebot_config::settings::a_command_line_file_is_reported_as_the_layer_that_won_a_name`
`verified-by: bravebot_config::settings::a_command_line_file_that_is_not_there_leaves_the_found_layers_in_force`
`verified-by: bravebot_config::settings::a_command_line_file_that_is_already_a_layer_is_read_once`
`verified-by: bravebot_config::settings::the_layers_that_were_read_are_reported_weakest_first`
`verified-by: bravebot_config::settings::a_name_more_than_one_layer_set_reports_the_file_that_won`
`verified-by: bravebot_config::settings::an_override_reports_the_name_and_the_file_and_never_the_value`
`verified-by: bravebot_config::lib::the_environment_outranks_the_settings_file`


<a id="BACKEND-25"></a>
### BACKEND-25: a request to Brave's endpoint says which product is asking

Every request to the aichat endpoint Brave runs carries `Brave-Product: bravebot`, both the one
asking for a reply and the one asking what models exist. The endpoint answers a listing curated for
this product, and resolves that product's own automatic entry. Nothing decides whether to send the
header: it is a literal on the one path that builds a request to that endpoint.

A gateway is sent no such header, and neither is a gateway's roster request. What a third-party
service is sent is the shape it documents, and a header naming a product it has never heard of is
this system telling it something it cannot act on.

**Why.** The endpoint serves Leo as well, whose roster is chosen for a chat assistant. A good
fraction of it cannot call tools at all, and choosing one of those produces an agent that can read
and write nothing, which is the promise BACKEND-5 makes about a name on the list. Asking as a product
is what makes that promise keepable without this code carrying a hand-written list of which models
are suitable, since a list compiled in here goes stale the moment the roster changes and the service
is the thing that knows.

`verified-by: bravebot_aichat::lib::a_request_without_a_gateway_is_still_signed`
`verified-by: bravebot_aichat::lib::a_gateway_request_goes_to_the_configured_host_with_a_bearer_token`
`verified-by: bravebot_aichat::client::the_model_listing_is_fetched_from_the_models_path`
`verified-by: bravebot_aichat::client::a_gateways_roster_request_carries_no_brave_product_header`


<a id="BACKEND-26"></a>
### BACKEND-26: the automatic entry names this product, and Leo's name resolves to it

The name requested when nobody has chosen a model is `automatic-bravebot`. Where a settings file,
an exported variable or a choice recorded earlier says `automatic`, that name resolves to
`automatic-bravebot` before it reaches a request. Every other name is used as written, under
BACKEND-12.

**Why.** `automatic` is Leo's triage entry and routes by a policy chosen for a chat assistant, so the
two words name different behaviour rather than the same behaviour twice. A recorded choice outlives
the version that made it and a settings file is copied between machines, so the older name is in
circulation and reaching the endpoint. Left alone it is not refused: it selects the other product's
routing, which is a working request that quietly is not what this agent asked for.

**Note.** The resolution is one-way, so `automatic` cannot be requested. Reaching Leo's routing
deliberately is not something the picker offers, that entry not being on this product's roster.

`verified-by: bravebot_config::lib::the_legacy_automatic_name_becomes_the_brave_bot_default`
`verified-by: bravebot_session::store::the_legacy_automatic_name_is_rewritten_on_read`
`verified-by: bravebot_aichat::models::automatic_is_offered_once_even_if_the_server_lists_it_too`
`verified-by: bravebot_agent::turn::without_a_choice_the_configured_default_is_requested`

<a id="BACKEND-27"></a>
### BACKEND-27: a Bedrock request, and one to Anthropic, marks the prefix it will send again

Two cache breakpoints go on a request to Bedrock: one at the end of the system prompt, which covers
the tool schemas in front of it, and one on the last block of the conversation, which moves to the
end as the conversation grows. The second comes off where the caller says no later request sends that
conversation again, leaving such a request the system prompt's breakpoint alone. Neither changes what
is sent, only what the service has to read again.

**Why.** A turn re-sends its whole history every round. One session reached 104,633 tokens over
twenty-seven rounds and paid for every token of every round at full price, in latency as much as in
money, and the great majority of those tokens were bytes the service had already read a minute
earlier. The prompt and the schemas are identical on every round of every session; the messages in
front of the last one are identical to a round ago.

**Rolling rather than fixed.** Each request writes the round before it into the cache and reads
back everything older, which is what makes the second breakpoint worth a cache write. A conversation
ending in an image or a tool call is left with the breakpoint on the system prompt alone, and costs
a cache write and nothing else. A request whose conversation no later request sends is left the same
way: the write on the end of that exchange is charged above the tokens it covers and buys a cache
nothing reads back, while the prompt in front of it is the same bytes every time such a request is
made. Which requests those are is the caller's to say, and
[compaction.md](compaction.md), [goal.md](goal.md), [watching.md](watching.md) and
[tools/spawn-processor.md](tools/spawn-processor.md) are where they say it.

**The reported prompt is what was sent, not what was read.** This API states `inputTokens` net of
the cache and reports the cached tokens beside it, so the three are added back together on the way
into a `Usage`. Without that a cached round reads as a conversation that shrank while it grew. They
are also carried through separately, which is what BACKEND-31 requires and the only way anything can
say whether this clause is working.

**A model that refuses them is not sent them again.** Prompt caching is not something every model
this backend can reach offers, and one that does not refuses the whole request rather than reading
past the breakpoints. A request refused on its contents is therefore sent once more without them,
and where that answers, no later request in the session carries them. Only a refusal does this:
every other failure leaves the breakpoints in place.

**Why ask rather than know.** An inference-profile ARN does not say which model is behind it, which
is the same fact that makes one figure stand in for every tier's context window. Asking costs one
extra round trip the first time such a model is used; not asking costs every request to it, since
the breakpoints are a part of the request nobody asked for and the service refuses the lot.

**Not only this backend.** The aichat endpoint and an OpenAI-compatible gateway take a different
wire format, which marks a prefix a different way and marks less of one: the system prompt and the
last thing the user said, never a result. That is BACKEND-32, and it turns on the same trade, that
asking a service which refuses costs one round trip while not asking costs every request.

**How long the service keeps it is a person's choice.** A `promptCacheTtl` of `5m` or `1h` in the
settings puts a `ttl` of that word on each breakpoint, the prompt's and the conversation's alike.
Absent, or any other word, nothing is added and the service keeps its own default, so a build nobody
has configured sends breakpoints with no `ttl` field. The longer lifetime is charged at a higher write rate, which
is why the default is no lifetime rather than the longest. The value comes from the settings and
never from message content, so no branch depends on an untrusted byte.

**A model that refuses the lifetime keeps its breakpoints.** A model that reads a breakpoint and not
a lifetime on it refuses the request in the same way a model without caching does. A refusal of a
request carrying a lifetime is therefore sent once more with the lifetime removed and the breakpoints
kept, ahead of the retry that removes the breakpoints, and where that answers no later request in the
process sends that model a lifetime. A request carrying no lifetime is not affected.

**A request to an `anthropic` entry marks the same two prefixes.** The breakpoint is a `cache_control`
field on the block it follows, and not a block of its own, the shape that API states: on the last text
block of the system prompt, and on the last block of the conversation where that is text or a tool
result. The conversation's comes off where the caller says no later request sends it, the lifetime
is a `ttl` on each, and a model that refuses a lifetime keeps its breakpoints, while one that refuses
a breakpoint is not sent one again. What a model refused is remembered against the service and the
name the model goes out under, so two services serving one slug are asked separately, and
[BACKEND-57](#BACKEND-57) says in what order a refused request gives its fields up.

**A breakpoint carries no label and asks for nothing.** It marks a prefix of a request that has
already been assembled, after every gate that decided what may be in it. Which bytes the service
kept from a previous request of the same session cannot put a byte into this one that was not sent,
so nothing in [labels.md](labels.md) reads differently for a request that carries breakpoints than
for one that does not.

`verified-by: bravebot_bedrock::protocol::the_system_prompt_carries_a_breakpoint`
`verified-by: bravebot_bedrock::protocol::a_chosen_lifetime_is_stated_on_every_breakpoint`
`verified-by: bravebot_bedrock::protocol::no_lifetime_is_sent_where_none_was_chosen`
`verified-by: bravebot_bedrock::lib::the_chosen_lifetime_is_sent_until_the_model_refuses_it`
`verified-by: bravebot_bedrock::lib::a_refused_lifetime_is_given_up_before_the_breakpoints_and_remembered`
`verified-by: bravebot_config::settings::only_five_minutes_or_an_hour_names_a_cache_lifetime`
`verified-by: bravebot_config::lib::the_cache_lifetime_setting_reaches_every_account`
`verified-by: bravebot_bedrock::protocol::the_last_block_of_the_conversation_carries_a_breakpoint`
`verified-by: bravebot_bedrock::protocol::a_conversation_ending_in_a_tool_result_is_marked_too`
`verified-by: bravebot_bedrock::protocol::a_request_giving_up_its_conversation_keeps_the_prompts_breakpoint_alone`
`verified-by: bravebot_bedrock::protocol::a_reply_without_a_breakpoint_still_parses`
`verified-by: bravebot_bedrock::protocol::cached_tokens_are_counted_as_the_prompt_they_were`
`verified-by: bravebot_bedrock::protocol::a_request_without_breakpoints_keeps_everything_that_was_asked_for`
`verified-by: bravebot_bedrock::lib::what_a_model_refused_outlives_the_client_that_found_out`
`verified-by: bravebot_bedrock::lib::a_request_refused_on_its_contents_is_asked_again_without_the_breakpoints`
`verified-by: bravebot_bedrock::lib::only_a_refusal_on_the_contents_drops_the_breakpoints`
`verified-by: bravebot_aichat::anthropic::protocol::the_system_prompt_and_the_end_of_the_conversation_carry_a_breakpoint`
`verified-by: bravebot_aichat::anthropic::protocol::a_conversation_ending_in_a_tool_result_is_marked_on_the_result`
`verified-by: bravebot_aichat::anthropic::protocol::a_conversation_ending_in_a_call_is_marked_nowhere_but_the_prompt`
`verified-by: bravebot_aichat::anthropic::protocol::a_request_giving_up_its_conversation_keeps_the_prompts_breakpoint_alone`
`verified-by: bravebot_aichat::anthropic::protocol::a_chosen_lifetime_is_stated_on_every_breakpoint_and_none_is_added`
`verified-by: bravebot_aichat::anthropic::protocol::a_request_without_breakpoints_keeps_everything_that_was_asked_for`
`verified-by: bravebot_aichat::client::an_anthropic_request_refused_on_its_contents_is_asked_again_without_what_nobody_asked_for`
`verified-by: bravebot_aichat::client::an_anthropic_refused_lifetime_is_given_up_before_the_breakpoints`
`verified-by: bravebot_aichat::client::a_probe_that_failed_as_well_leaves_what_was_known_alone`

<a id="BACKEND-28"></a>
### BACKEND-28: a Bedrock tier may name any model that account can reach

A request to AWS Bedrock is built the same way whatever model it names, in the body that service
states for every provider it hosts rather than in any one provider's own. A tier may therefore name
a model from any of them, and an inference profile standing for one, without anything here
recognising which provider is behind it.

**Why.** Bedrock fronts several providers, and an inference-profile ARN does not say which one
serves it. A body shaped for a single provider therefore makes which models are reachable a
property of this code rather than of the account: the name is accepted, signed, sent, and refused at
the far end on the body, which is the failure BACKEND-3 argues against for a name no service
recognises. It is worse here, because the account can reach the model and nothing a person writes
in a settings file closes the gap. Where a role is scoped to inference profiles, which is how
per-user cost allocation is granted, the routes a bearer token can use refuse those profiles
outright, so this is the only one that answers at all.

**Note.** The tier words stay `opus`, `sonnet` and `haiku`. They name a slot in a settings file
rather than a model family, and a tier is whichever model the account named for it.

`verified-by: bravebot_bedrock::protocol::the_request_names_no_provider_of_its_own`
`verified-by: bravebot_config::bedrock::streaming_and_buffered_requests_have_different_routes`
`verified-by: bravebot_config::bedrock::a_model_arn_is_encoded_into_the_path`

<a id="BACKEND-29"></a>
### BACKEND-29: a settings block may name an AWS account, and every model it lists is reachable

A `provider` entry may name AWS Bedrock instead of an OpenAI-compatible gateway. It states a region,
optionally a credential profile, and as many models as the file lists, each reached by the name it
is keyed under. Such an entry is served by the Bedrock backend and signed with the AWS credential
chain, never sent to a gateway, and it adds to what the tier variables already name rather than
replacing it.

**Why.** The tier variables are three, named for one provider's model families, and a fourth model
can only be had by giving up one of the three. That is a limit of the shape those variables have,
not of the account, which reaches as many models as it is entitled to. The block this borrows is
already how the other tool configures the same account, keyed by model rather than by tier, so
taking it costs nothing and removes the limit.

Signed rather than authenticated with a token because that is what the service takes, and it is
also what makes the entry worth having: the routes a bearer token can use refuse the inference
profile ARNs a per-user role is commonly scoped to, so a gateway entry pointed at the same account
reaches nothing.

**The region is required and the endpoint is not.** The host carries the region, so one value
produces the other and there is nothing left to state. An entry naming no region configures no
service, for the reason a Bedrock block without one does not: a guessed region is a request that
fails somewhere far from the mistake. An entry whose region is not lowercase letters, digits and `-`
starting with a letter or a digit configures no service either, since the host is built from it and
one holding `/`, `@`, `?`, `#`, `:` or `.` would send the signed request to another host or another
route than the one the person named. [BACKEND-33](#BACKEND-33) gives the same rule for `AWS_REGION`.

**What a row says.** A model named here has no tier, so a picker row carries what the block called
it, and its id where the block called it nothing. An inference-profile ARN is not a name anybody
reads, which is why this is worth stating rather than left to fall out of the id.

`verified-by: bravebot_config::provider::an_aws_block_configures_bedrock_rather_than_a_gateway`
`verified-by: bravebot_config::provider::an_aws_model_the_block_did_not_name_is_shown_by_its_id`
`verified-by: bravebot_config::provider::an_aws_block_without_a_region_configures_nothing`
`verified-by: bravebot_config::provider::an_aws_block_whose_region_would_move_the_request_configures_nothing`
`verified-by: bravebot_config::bedrock::a_provider_account_whose_region_would_move_the_request_is_not_built`
`verified-by: bravebot_config::lib::a_model_an_aws_block_named_reaches_bedrock_rather_than_a_gateway`
`verified-by: bravebot_config::lib::a_name_qualified_by_the_aws_id_is_not_a_gateway_either`
`verified-by: bravebot_agent::backend::a_model_an_aws_block_named_selects_the_bedrock_backend`
`verified-by: bravebot_tui::app::a_bedrock_model_a_block_named_is_shown_under_that_name`
`verified-by: bravebot_tui::app::an_aws_provider_entrys_models_are_offered_under_names_bedrock_answers_to`
`verified-by: bravebot_ui_bridge::models::the_window_offers_an_aws_provider_entry_under_its_bare_id`

<a id="BACKEND-30"></a>
### BACKEND-30: what a commit or a pull request carries is a settings key, and empty says none

An `attribution` block names what this program may add to a commit message it writes and to a pull
request it opens: `commit` and `pr`, a string each. The empty string is an answer and means carry
nothing. A name no layer wrote is the settings having said nothing about that destination, which is
a different answer from empty and is reported as unset. Anything that is not a string is read as
absence, on the footing every other malformed value here is read.

The resolved answer is stated to the planner, in the standing text put in front of every round of
every turn, rather than left where something writing a commit message would have to go and look it
up. A destination the block named is stated there, empty included; one it did not name is not
mentioned at all. A delegate is told what the turn that spawned it was told, a delegate writing in
the same tree for the same person.

**Why.** A trailer nobody asked for is a small thing on one commit and a permanent thing in a
history, and asking for none of it in the instructions puts the answer somewhere a model has to be
reading at the moment it writes one. A key states it once, with nothing to re-read and nothing to
drop on a long turn. Empty has to be a value for that to work at all: read as absence it would be
the one thing the block exists to say and the one thing it could not.

Absence is kept distinct from empty because they ask for different things. Empty is a decision that
nothing is carried; unset leaves the decision with whoever writes the commit, and collapsing the two
would make a file that mentions the block at all speak for names it never named.

**Only the person's own files may write the block.** It is read from the home file and from the
file `--settings` names, and removed from the project and local layers before the merge, each file
that wrote one being reported. The string reaches the planner on every round, and untrusted bytes
never enter that context: a checkout arrives with whatever produced it, and nobody vouched for what
it says. [references.md](references.md) refuses `references` from a checkout for the same reason. A
named file that resolves inside the workspace is a checkout's file under another name.

**A stated value is handed over as text to copy rather than as a sentence addressed to the
planner.** The string is the person's own, so this is presentation rather than a defence: it is
quoted and the planner is told in the same breath that nothing inside it is addressed to it.

`verified-by: bravebot_config::settings::an_empty_attribution_is_a_choice_of_nothing`
`verified-by: bravebot_config::settings::an_attribution_name_no_file_wrote_is_unset`
`verified-by: bravebot_config::settings::a_checkout_layer_cannot_set_attribution`
`verified-by: bravebot_config::settings::a_named_file_inside_the_workspace_cannot_set_attribution`
`verified-by: bravebot_agent::preamble::an_empty_attribution_tells_the_planner_to_carry_nothing_on_a_commit`
`verified-by: bravebot_agent::preamble::an_attribution_no_file_named_is_not_stated_at_all`
`verified-by: bravebot_agent::preamble::an_attribution_a_file_named_is_carried_word_for_word`
`verified-by: bravebot_agent::turn::a_turn_is_told_what_the_settings_say_a_commit_message_may_carry`

<a id="BACKEND-31"></a>
### BACKEND-31: a reply reports how much of the prompt the service did not have to read

A reply's usage carries two figures beside the prompt total: how much of the prompt the service
answered out of its own cache, and how much this request wrote into that cache for the next one to
read, so the prompt total says what a round sent and the two figures say what it cost. Bedrock states
both, and so does Anthropic, as `cache_read_input_tokens` and `cache_creation_input_tokens` beside an
`input_tokens` that is net of the cache, so the prompt is the three added together and both figures are
counted inside it. An OpenAI-compatible reply states the read in `prompt_tokens_details.cached_tokens`
and, where a service states one, the write in `prompt_tokens_details.cache_write_tokens`; the prompt total
is not adjusted by either, and a write that is absent, `null` or not written in the JSON as an unsigned
integer is zero, which takes text, a negative number and a number written with a fraction or an exponent,
so `90.0` and `9e1` read as zero as `90.5` does. What OpenAI states of the two figures for a chat
completions route, and what has been seen of them, is under "What OpenAI's own API reports" below. The
Responses API states the read in `input_tokens_details.cached_tokens` and the write in
`input_tokens_details.cache_write_tokens`, both counted inside `input_tokens`, and the ChatGPT account is
read as the Responses API is ([CHATGPT-10](chatgpt-sign-in.md#CHATGPT-10)). A service stating neither
leaves both at zero, and a figure Anthropic states as `null` is one it did not state.

A turn sums them over its rounds, as it sums the total, and a delegate's are added to the turn that
spawned it. A sum of the two figures stops at the largest number a count holds, since a gateway states them and
one may state that number.

**What OpenAI's own API reports.**

- Observed on 2026-10-27, in replies to chat completions sent to `api.openai.com` with a key whose account had credit.
  - The request this program sends (the keys `messages`, `model`, `stream`, `stream_options` and `tools`, 22 tools, a
    system message and a user message with string contents, no `max_tokens`, `max_completion_tokens`,
    `reasoning_effort` or `cache_control`) was answered 200 by `gpt-4o-mini` with the content type
    `text/event-stream; charset=utf-8`, and a `bravebot -p` turn on `openai/gpt-4o-mini` exited 0 and printed the
    reply.
  - A streamed reply held four data chunks and `[DONE]`: a chunk whose `delta` held `content`, `refusal` and `role`, a
    chunk with the words, a chunk with an empty `delta` and the `finish_reason` `stop`, and a last chunk whose
    `choices` was empty and whose `usage` held the figures. Every chunk held `choices`, `created`, `id`, `model`,
    `obfuscation`, `object`, `service_tier`, `system_fingerprint` and `usage`, and `usage` was `null` on the first
    three. A reply that was not streamed held `choices`, `created`, `id`, `model`, `object`, `service_tier`,
    `system_fingerprint` and `usage`, and its `usage` had the shapes of a streamed one.
  - `usage` held `prompt_tokens`, `completion_tokens`, `total_tokens`, which is their sum, `prompt_tokens_details` and
    `completion_tokens_details` (`reasoning_tokens`, `audio_tokens`, `accepted_prediction_tokens` and
    `rejected_prediction_tokens`). `prompt_tokens_details` held `cached_tokens` and `audio_tokens`.
  - It also held `cache_write_tokens` in the reply to the one short request sent for each id (13 prompt tokens on all
    eight, not streamed) for 8 of the 40 ids that answered 200: `gpt-5.6-luna`, `gpt-5.6-sol`, `gpt-5.6-terra`,
    `gpt-6-astra`, `gpt-6-luna`, `gpt-6-sol`, `gpt-6.1-sol` and `gpt-6.1-terra`, each with the value 0. The other 32 did
    not hold it: `chat-latest` and the ids of the `o3`, `gpt-4o`, `gpt-4.1`, `gpt-5` and `gpt-5.1` to `gpt-5.5`
    families, dated snapshots included. The replies of the pair of requests below agreed with that: the key was held
    for `gpt-5.6-luna` (streamed and not) and `gpt-6-luna` (streamed), and not held for `gpt-4o-mini`, `gpt-4.1-mini`,
    `gpt-5-mini` and `gpt-5.4-mini` (streamed and not) and `gpt-5.5` (streamed).
  - A prompt of about 1670 tokens (a system message headed by a tag that differed for each run, and the user message
    `Reply with the single word: ok`) was sent with no mark in 12 runs of two or three requests each, over seven models:
    streamed to each, and not streamed to `gpt-4o-mini`, `gpt-4.1-mini`, `gpt-5-mini`, `gpt-5.4-mini` and
    `gpt-5.6-luna`. Each request carried a `max_completion_tokens` of 64, or of 400 to 800, which the request this
    program sends does not. The second request of a run was sent about 9 seconds after the first reply ended, and a
    third was sent about 11 seconds after the second reply ended where the second had read nothing. `cached_tokens` is
    counted inside `prompt_tokens`: the first request of `gpt-4o-mini` streamed reported 0 of 1671 and the second 1536
    of 1671. The second request read from the cache in 8 of the 12 runs: 1536 tokens in both runs of `gpt-4o-mini` and
    of `gpt-4.1-mini` and in `gpt-5-mini` not streamed, and the `gpt-5.6-luna` and `gpt-6-luna` runs below. It did not
    in 3 runs: `gpt-5-mini` streamed and `gpt-5.4-mini` not streamed read 0, and `gpt-5.4-mini` streamed answered 500
    `server_error`, the second request of each `gpt-5.4-mini` run having taken 22 seconds. The third request of those
    three read 1536, and was sent about 21 seconds after the first reply ended for `gpt-5-mini` and about 42 seconds
    after it for `gpt-5.4-mini`. In the twelfth run, `gpt-5.5` streamed, three requests, the last about 21 seconds
    after the first reply ended, read 0. The reads of 1536 and the read of 16256 on `gpt-4o-mini` below were multiples
    of 128, and those of the `luna` models were not.
  - `gpt-5.6-luna` reported `cached_tokens` 0 and a `cache_write_tokens` of 1670 for a prompt of 1673 on the first
    request (1669 of 1672 not streamed, and 1673 of 1676 on `gpt-6-luna`, streamed), counted inside `prompt_tokens`,
    which was not adjusted: `total_tokens` was 1677 for 4 completion tokens. The second request reported the same
    number as `cached_tokens` (1670, 1669 and 1673) and a `cache_write_tokens` of 0, present in the object. No mark
    had been sent. These two models are the ones of the eight that were sent the pair.
  - The body this program sends to `gpt-4o-mini`, with no ceiling, sent a second time about four minutes after the
    first, reported 16256 `cached_tokens` of 16322 where the first had reported 0.
- Not shown: a write above 0 on any of the other six ids that hold the key, what a write is charged (no price was
  read), whether a mark changes the figures (no mark was sent in these requests, and the marked request of
  [BACKEND-32](#BACKEND-32) is not comparable with them), a `prompt_cache_key`, tool results in the prompt, and
  `gpt-5.5` reading after more than 21 seconds.
- Stated by OpenAI's published API description (version 2.3.0): a request with
  `stream_options.include_usage` gets one more chunk before the end of the stream, whose `usage` holds
  `prompt_tokens` and `completion_tokens` and whose `choices` is empty; `usage.prompt_tokens_details` holds
  `cached_tokens`, the cached tokens present in the prompt, and `cache_write_tokens`, the unadjusted number
  of prompt tokens written to the cache. The `openai-python` type of that object
  (`src/openai/types/completion_usage.py`, read on 2026-10-27) names the same two fields in the same words, as an
  optional whole number each. OpenAI's prompt caching guide, read on 2026-10-27, shows `cache_write_tokens` for the
  Responses API only, in `usage.input_tokens_details`, and says nothing of it for chat completions.
- Shown by an example in OpenAI's cookbook, which is neither this program's observation nor the API's present
  statement: the notebook `Prompt_Caching101.ipynb`, read on 2026-10-27, prints non-streamed chat completions
  replies of 1 October 2024 from `gpt-4o-mini-2024-07-18` and `gpt-4o-2024-08-06`. Their `prompt_tokens_details`
  holds `cached_tokens` alone. The reply of the first that read from the cache has a `prompt_tokens` of 1136, a
  `cached_tokens` of 1024, 64 completion tokens and a `total_tokens` of 1200, and the second 1548, 1280, 86 and
  1634. They are examples of two models on one day. No source read states whether `prompt_tokens` counts a
  write.
- The figure is read where it is sent, and a turn reports a write of zero for a model that states none, as it does for
  any other gateway that states none ([Known costs](#known-costs)). A gateway that states a write in another field or
  shape is not read for it.

**Why.** A cached token is charged at a fraction of a fresh one and a written one above it, so the
prompt total is the one figure that does not move when caching begins working. Two sessions of the
same length, one whose every breakpoint hit and one whose every breakpoint missed, send an identical
prompt and differ about tenfold in money and in latency. Without the split, BACKEND-27 is a
requirement whose effect nothing can observe, and a regression in it costs an order of magnitude
while every figure reported stays where it was.

**Summed rather than kept per round.** A turn's first round writes the prefix that its later rounds
read back, so the round that pays and the rounds that profit are different rounds. One round's
figure reports one or the other.

**Zero is silence, not a miss.** Both figures at zero says a service that reports nothing about a
cache exactly as much as it says a round whose cache missed, and nothing here distinguishes them.
Whatever presents these figures presents nothing when they are zero, rather than presenting a miss
that may not have happened. That holds of each figure alone: a turn that established a prefix and
read nothing back reports the write and says nothing about the read.

**The status panel and footer report the last turn's.** A turn in flight reports its own running
totals from the first round that completed a request ([PANEL-8](info-panel.md#PANEL-8)), and the
last turn's until then. The last turn rather than a total over the
session, because caching is a property of a request: a session that compacted part way through has
turns whose prefix survived and turns whose prefix was rewritten, and a total averages away the
thing the figures are for. The footer shows the cached read as a percentage of the turn's prompt
tokens. Nothing about a cache is kept in a session record, so a resumed session reports nothing
until a turn has run. Clearing goes with what the conversation spent rather than with the model the
user chose, the figures describing a prompt that has been thrown away. Anything else that changes
which turn is the last one moves the figures with it: rewinding a turn puts back what the turn
before it read, and a turn that failed or was stopped reports nothing rather than leaving the turn
before it on the panel.

**The lifetime asked for is reported beside them.** The status panel's cache lines end with the
`promptCacheTtl` the settings chose, or say none was and the service's own applies. That is the
setting and not a reading: no reply says how long a service kept a prefix, and a model that refuses
the field is sent the request without it (BACKEND-27, BACKEND-32). The line is drawn from the setting
whichever service answered ([Known costs](#known-costs)). It sits there because a read of
zero after a pause means a different thing under five minutes than under an hour.

**Presented as two figures and never as their sum.** They are priced in opposite directions, a read
at a fraction of a fresh token and a write above one, so a turn that saved almost the whole prompt
and a turn that paid a premium on it add up the same. The heading carries no total of its own, and
says which turn it speaks for, the counts beside it being the session's.

`verified-by: bravebot_bedrock::protocol::a_cached_round_is_told_apart_from_one_that_read_the_whole_prompt`
`verified-by: bravebot_bedrock::lib::both_halves_of_the_cost_survive_the_stream`
`verified-by: bravebot_aichat::anthropic::protocol::cached_tokens_are_counted_as_the_prompt_they_were_and_carried_apart`
`verified-by: bravebot_aichat::anthropic::protocol::a_null_cache_figure_reads_as_silence`
`verified-by: bravebot_aichat::anthropic::protocol::a_prompt_too_large_to_add_saturates_and_never_wraps`
`verified-by: bravebot_aichat::responses::protocol::a_stream_ends_in_one_of_three_events_and_reports_usage`
`verified-by: bravebot_aichat::anthropic::protocol::a_later_report_replaces_what_it_restates_and_keeps_the_rest`
`verified-by: bravebot_aichat::client::a_whole_anthropic_reply_is_read_into_text_calls_model_and_cost`
`verified-by: bravebot_aichat::client::an_anthropic_stream_is_assembled_from_its_events_and_asks_for_arguments_as_written`
`verified-by: bravebot_aichat::protocol::the_cache_figure_is_read_out_of_the_details_object`
`verified-by: bravebot_aichat::protocol::a_cache_write_figure_beside_the_cached_one_is_read_as_the_write`
`verified-by: bravebot_aichat::protocol::a_cache_write_is_read_with_no_cached_figure_beside_it`
`verified-by: bravebot_aichat::protocol::a_cache_write_not_written_as_an_unsigned_integer_reads_as_silence_and_does_not_fail_the_reply`
`verified-by: bravebot_aichat::protocol::cache_figures_stated_at_the_largest_number_saturate_when_summed`
`verified-by: bravebot_aichat::protocol::a_cache_write_in_the_usage_chunk_is_what_the_stream_reports`
`verified-by: bravebot_aichat::protocol::a_whole_reply_as_openai_sends_it_reports_the_cache_figures_it_states`
`verified-by: bravebot_aichat::protocol::the_usage_chunk_with_no_choices_after_the_finish_reason_is_the_usage_the_stream_reports`
`verified-by: bravebot_aichat::client::a_reply_as_openai_sends_it_reports_its_cache_figures_whole_and_streamed`
`verified-by: bravebot_aichat::protocol::a_usage_object_without_cache_figures_still_parses`
`verified-by: bravebot_aichat::protocol::a_null_cache_figure_reads_as_silence_rather_than_failing_the_reply`
`verified-by: bravebot_agent::turn::a_turn_sums_what_its_rounds_read_out_of_the_cache`
`verified-by: bravebot_agent::turn::a_turn_against_a_server_that_says_nothing_about_a_cache_reports_nothing`
`verified-by: bravebot_agent::turn::a_turn_counts_what_its_delegates_read_out_of_the_cache`
`verified-by: bravebot_tui::status::the_panel_says_how_much_of_the_prompt_came_out_of_the_cache`
`verified-by: bravebot_tui::status::the_panel_says_which_cache_lifetime_the_settings_asked_for`
`verified-by: bravebot_tui::status::a_backend_that_reports_nothing_about_a_cache_gets_no_cache_lines`
`verified-by: bravebot_tui::status::a_turn_that_only_wrote_to_the_cache_does_not_report_a_read_of_zero`
`verified-by: bravebot_tui::panel::a_zero_cache_figure_is_not_drawn`
`verified-by: bravebot_tui::panel::a_turn_ended_before_any_request_reports_no_cache_figures`
`verified-by: bravebot_tui::render::the_hint_line_shows_the_last_turns_cache_hit_rate`
`verified-by: bravebot_tui::render::the_hint_line_says_nothing_for_a_write_only_cache_turn`
`verified-by: bravebot_tui::render::the_hint_line_says_nothing_where_the_backend_reported_nothing`
`verified-by: bravebot_tui::state::clearing_forgets_what_the_last_turn_read_out_of_the_cache`
`verified-by: bravebot_tui::state::the_cache_figure_follows_which_turn_is_the_last_one`
`verified-by: bravebot_tui::sessions::a_rewind_point_keeps_no_cache_figure_in_the_record`
`verified-by: bravebot_aichat::client::an_account_reply_is_read_from_its_events_and_its_cost_from_the_final_one`

<a id="BACKEND-32"></a>
### BACKEND-32: an aichat request marks the prefix it will send again, except at OpenAI's own API

A request in the OpenAI-compatible wire format marks two prefixes: the system prompt, which the tool
schemas travel in front of, and the last thing the user said. The second is left off where the caller
says the conversation is given up once it has been answered, marking the prompt alone. The mark is a
`cache_control` of `{"type": "ephemeral"}` on the content block the prefix ends at, which is the field
the Anthropic API defines for this and the field a gateway fronting such a model reads. A marked
message carries its words as a block rather than as a bare string, that being the only shape the
field exists in. A request to `api.openai.com` is the exception: it carries no mark and no lifetime, as
"OpenAI's own API is sent none" below says.

**Why.** The arithmetic is BACKEND-27's, and so is the measurement behind it: a turn re-sends its
whole history every round, the system prompt and the schemas are identical on every round of every
session, and a cached token is charged at a fraction of a fresh one. None of that is a property of
Bedrock. What differs is the shape the request states it in, and how much of the conversation it
dares state it about.

**Less than BACKEND-27 marks, and deliberately.** That clause's second breakpoint rolls to the end
of the conversation whatever the last block is, a result included. Here a result is never marked. To
carry a mark, a `tool` message's content would have to go out as a list of blocks, which is a shape
not every service reading this format accepts, and a service that rejects the request loses the
system prompt's breakpoint with it. What that costs is the rounds within one turn: the prefix
through the last user turn is written by the first round and read back by every round after it,
since what a round appends is a call and its result on the end, so what goes unread is only what
those rounds appended. A turn whose last block is a picture keeps the breakpoint on the system prompt
alone, because what a service makes of a mark on an image block is not a thing to guess at.

**A conversation nothing sends again is not worth marking.** The mark on the last thing the user said
is worth a cache write because the request after it sends everything in front of it again. A request
that gives its conversation up once it has been answered has no request after it, and the write is
charged above the tokens it covers for a prefix nothing can read back. The prompt keeps its mark,
being the same bytes every time such a request is made, so what is given up is a write and no read.
[compaction.md](compaction.md), [goal.md](goal.md), [watching.md](watching.md) and
[tools/spawn-processor.md](tools/spawn-processor.md) are where a request says its conversation is
not sent again.

**Marked on the way out and nowhere else.** The mark is put on a copy as the body is built, so the
request a turn assembled does not carry one and neither does anything a session records. A
breakpoint belongs to the request that sends it: one read back out of a session file would be sent
again by a request that never asked for it, and against a service that had already refused.

**A service that refuses it is asked again without it, and not asked again after that.** Neither
this endpoint's roster nor a gateway's says whether a model's service reads a breakpoint, so the
request asks. A service that will not take the body answers an invalid-request status, and that
request is sent once more with no breakpoints on it; where that answers, no later request in the
process marks anything for that model on that service. Where that retry is refused as well and still
carried an effort level, the level is what is given up next, and a request that answers with both
gone is remembered as having had both refused: BACKEND-22 states that order and the caching it costs.
A refusal with nothing further to give up proves nothing, an invalid-request status being also what a
prompt too long for the model is answered with, so nothing is remembered and the next request asks
again. This is BACKEND-27's rule and its reason, in the statuses this protocol says it with.

**OpenAI's own API is sent none.** A request to `api.openai.com` carries no breakpoints, and they are given up
before the first request, so no request is spent finding out, and a refusal that follows is read as a refusal of
the level, since no marks were sent. The observations below show a repeated prompt read from the cache with no mark
sent, and a `cache_control` that was sent answered 200 and not refused. The one marked request whose cache figure was
read is not comparable with an unmarked one, so whether a mark changes what is read is not known. The host a
request goes to decides this, compared as [BACKEND-19](#BACKEND-19) says, so a proxy keyed `openai` that reads the marks
is sent them, and an entry under another id that states OpenAI's endpoint is not. A `promptCacheTtl` rides on the marks
and is not sent there either.

**What is known of OpenAI's own API and these marks.** [BACKEND-31](#BACKEND-31) holds the cache figures its replies
state.

- Observed on 2026-10-27, one sample for each, in chat completions sent to `api.openai.com`. A `cache_control` of
  `{"type": "ephemeral"}` on a user text part, and the same with `"ttl": "1h"`, was sent with a `max_completion_tokens`
  and answered 200 by `gpt-4o-mini` and by `gpt-5.4-mini`, with the `prompt_tokens` of the same message sent without
  it in the sweep of [BACKEND-19](#BACKEND-19) (14 and 13), so it was not refused. A body in the shape this program builds for a proxy keyed `openai` (two marks, contents as blocks, 22 tools, streamed,
  no ceiling, 78404 bytes) was answered 200 by `gpt-4o-mini` about seven minutes after the first send of this
  program's body for `api.openai.com` (contents as strings, no marks, 78278 bytes) and reported 2816 `cached_tokens`
  of 16322; its marks were on the system message part and on the user message part. That body, sent about four
  minutes after its first send, had reported 16256 of 16322 ([BACKEND-31](#BACKEND-31)). The two figures are of two
  bodies sent at different times, once each, and are not compared. A field at the top of the body that the service
  does not know, sent with a `max_completion_tokens`, was refused with 400 and the error type `invalid_request_error`:
  by `gpt-4o-mini` with a null `code`, a null `param` and the message "Unrecognized request argument supplied:
  probe_unknown_field", and by `gpt-5.4-mini` with the code `unknown_parameter`, the `param` `probe_unknown_field` and
  the message "Unknown parameter: 'probe_unknown_field'.". That is the status and the type of the refusal of a level ([BACKEND-22](#BACKEND-22)).
- Observed on 2026-10-27, with no mark sent. A prompt sent again was read from the cache by each model tried except
  `gpt-5.5`: on the second request, or for `gpt-5-mini` streamed and `gpt-5.4-mini` on the third, sent about 21 and about
  42 seconds after the first reply ended. `gpt-5.5` read nothing in three requests, the last about 21 seconds after
  the first reply. The body this program sends to `api.openai.com`, which carries no mark, read 16256 of its 16322
  tokens when sent a second time about four minutes after the first ([BACKEND-31](#BACKEND-31)).
- Stated by OpenAI's published API description (version 2.3.0): a text content part has `type`, `text` and
  `prompt_cache_breakpoint`, an explicit mark for gpt-5.6 and later that this program does not send, and no
  `cache_control`.
- Not shown: a `cache_control` on a `tool` message part, on a `system` message part with none on the user message,
  in a request with tool results, or on a model other than the two named; and a field nested elsewhere that the
  service does not know.

**Remembered against the service as well as the model.** A model id says nothing about who serves
it: two gateways can offer the same name, and one of them can be the name Brave's own endpoint
answers to. A refusal recorded against the id alone would stop the asking everywhere one appeared.

**A gateway may be asked how long to keep it.** With a `promptCacheTtl` setting, `cache_control`
becomes `{"type": "ephemeral", "ttl": "1h"}` (or `"5m"`) on a request to a gateway, and is unchanged
on a request to Brave's own endpoint, which reads no such field. A request to `api.openai.com` carries no
`cache_control`, so it carries no lifetime. The refusal rule is BACKEND-27's: a
gateway that refuses a request carrying a lifetime is sent it again without the lifetime and with the
breakpoints, and is not sent one again for that model on that service.

**Nothing here claims a service reads it.** What is established is that the request asks and that a
service refusing it with an invalid-request status does not cost the turn. A service that objects
some other way refuses the request as it would refuse any other, and the breakpoints are not what
gets dropped. Brave's endpoint reports no cache figure at all, so an accepted breakpoint cannot be
told there from an ignored one, and BACKEND-31 is what would say otherwise for a service that states
one.

**A breakpoint carries no label and asks for nothing.** BACKEND-27's argument holds here unchanged:
it marks a prefix of a request already assembled, after every gate that decided what may be in it,
and which bytes a service kept from an earlier request cannot put a byte into this one that was not
sent.

`verified-by: bravebot_aichat::protocol::the_system_prompt_and_the_last_thing_the_user_said_are_marked`
`verified-by: bravebot_aichat::lib::a_chosen_lifetime_goes_to_a_gateway_and_to_nothing_else`
`verified-by: bravebot_aichat::lib::a_request_to_openais_own_api_is_sent_no_breakpoints_and_one_to_a_proxy_is`
`verified-by: bravebot_config::provider::openais_own_api_follows_the_host_a_request_goes_to_whatever_the_entry_is_called`
`verified-by: bravebot_config::provider::the_spellings_of_openais_host_are_its_own_api_and_another_host_or_port_is_not`
`verified-by: bravebot_aichat::client::a_lifetime_a_gateway_refuses_is_given_up_before_the_breakpoints_and_remembered`
`verified-by: bravebot_aichat::protocol::a_result_the_assistant_asked_for_is_not_marked`
`verified-by: bravebot_aichat::protocol::the_last_user_turn_is_marked_through_several_rounds_of_results`
`verified-by: bravebot_aichat::protocol::a_request_giving_up_its_conversation_marks_the_prompt_alone`
`verified-by: bravebot_aichat::protocol::a_turn_ending_in_a_picture_is_left_unmarked`
`verified-by: bravebot_aichat::protocol::the_words_after_a_picture_carry_the_mark`
`verified-by: bravebot_aichat::protocol::a_marked_body_changes_nothing_but_the_messages`
`verified-by: bravebot_aichat::protocol::a_marked_block_is_the_block_it_came_from_and_the_mark`
`verified-by: bravebot_aichat::protocol::an_empty_turn_is_left_alone`
`verified-by: bravebot_aichat::client::the_request_asks_the_service_to_cache_the_prefix_it_will_be_sent_again`
`verified-by: bravebot_aichat::client::a_request_refused_on_its_contents_is_asked_again_without_the_breakpoints`
`verified-by: bravebot_aichat::client::a_service_that_refused_the_breakpoints_is_not_asked_for_them_again`
`verified-by: bravebot_aichat::client::a_refusal_on_one_service_does_not_stop_the_asking_on_another`
`verified-by: bravebot_aichat::client::a_refusal_the_retry_did_not_fix_is_not_remembered`
`verified-by: bravebot_aichat::client::a_level_a_gateway_refuses_costs_the_field_and_not_the_turn`
`verified-by: bravebot_aichat::client::a_request_the_server_refused_is_not_sent_again_unchanged`
`verified-by: bravebot_agent::turn::a_turn_without_attachments_sends_the_prompt_and_nothing_beside_it`

<a id="BACKEND-33"></a>
### BACKEND-33: an `env` block names these twenty-two variables

The `env` block of a settings file sets variables under their own names, and these are the names
something reads:

| Name | What it decides |
|---|---|
| `SERVICES_KEY_AICHAT` | the key a request to Brave's endpoint is signed with |
| `BRAVE_SERVICES_KEY_ID` | which key that signature is checked against |
| `BRAVE_AI_CHAT_ENDPOINT` | the host Brave's endpoint is reached at |
| `BRAVE_AI_CHAT_PREMIUM_ENDPOINT` | the host the premium key is presented to, in place of the one compiled in |
| `BRAVEBOT_DEFAULT_MODEL` | which model answers before anybody has chosen one |
| `BRAVEBOT_CONTEXT_BUDGET` | how many prompt tokens a conversation may reach before it is shortened |
| `BRAVEBOT_OUTPUT_BUDGET` | how many tokens one reply may run to before the service cuts it off |
| `BRAVEBOT_USE_BEDROCK` | `1` to reach models through somebody's own AWS account |
| `AWS_REGION` | which region that account is reached in |
| `AWS_PROFILE` | which profile in the AWS configuration names the credentials to sign with |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | the model the tier word `opus` names |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | the model the tier word `sonnet` names |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | the model the tier word `haiku` names |
| `GOOGLE_API_KEY` | the key a request to Google Vertex AI is sent with |
| `GOOGLE_CLOUD_PROJECT` | the Google Cloud project that request is made under, where there is one |
| `VERTEX_LOCATION` | the location it is made in, where that is not `global`, and not read with no project |
| `SSL_CERT_FILE` | a bundle of certificate authorities to trust in place of the built-in set |
| `SSL_CERT_DIR` | a directory of them |
| `ALL_PROXY` | the proxy every request is routed through, ahead of the two below |
| `HTTPS_PROXY` | the proxy, where `ALL_PROXY` is not set |
| `HTTP_PROXY` | the proxy, where neither is set |
| `NO_PROXY` | the hosts the proxy is not used for |

The last six rows are the network names. Three more rows are the Google names, and six are the
Bedrock names: `BRAVEBOT_USE_BEDROCK`, `AWS_REGION`, `AWS_PROFILE` and the three
`ANTHROPIC_DEFAULT_*_MODEL` tiers. All three sets come from the person's own layers alone, the
user's settings file and a file the command line named that is not inside the checkout. The project
and local files arrive with a clone, and a proxy or an authority they chose would decide where
conversation content is sent and who may read it, and a Google key or project, or an AWS account,
region, profile or tier model, they chose would decide which account receives it. So one of these
fifteen names in either file is reported by `doctor` with the file that wrote it and decides nothing,
and it does not displace what a layer
entitled to state it said. An exported variable still wins, as for every name here, and each of the
two answers comes whole from one place: [NET-7](network-egress.md#NET-7) and
[NET-8](network-egress.md#NET-8) say how. The names are spelled as the environment spells them in
upper case, and a lower-case spelling is not read from a file.

`AWS_REGION` is written into the host a Bedrock request is signed for and sent to, so it names a
region only as lowercase letters, digits and `-` starting with a letter or a digit, the characters
a Google location is held to ([BACKEND-49](#BACKEND-49)). A value with any other character, `/`,
`@`, `?`, `#`, `:` and `.` among them, configures no account, as an absent one does. That holds for
every layer that may set the name and for an exported variable, and the
`options.region` of a `provider` entry is held to it too ([BACKEND-29](#BACKEND-29)).

A gateway is not configured from this block: it is a `provider` entry, whose shape BACKEND-13
states and whose credential BACKEND-16 names. The three Google names are the one exception, and only
together: BACKEND-50 says when they name a service. The top-level `model` key is not one of these either, being a
choice rather than a variable, and BACKEND-11 is what ranks it.

**Why.** A configuration surface that is described but never named is one nobody can use without
reading the source. Anything written *about* this system (the site somebody installs it from, a
message telling a person what to set) is written from what is stated here, so a backend whose
variables are named nowhere is a backend that reaches people undocumented however completely its
behaviour is specified. Naming them is also what makes the set reviewable: a twenty-third variable is
a change to this table, which a person reads, rather than a constant added to a file nobody is
asked to look at.

The AWS names keep the spelling another tool already gave them, and the switch, the default model
and the two budgets carry this program's own prefix, for the reason BACKEND-24 gives about the file as a whole: a block
copied from elsewhere should work unedited, while a name that decides what *this* program does
belongs to this program and must not collide with whatever else a shared shell profile wanted.

The region is checked because the host is built from it. A
file names only the region, and the rest of the host is compiled in, so a value that passes the
check gives a host under `amazonaws.com`.

`verified-by: bravebot_config::lib::every_name_a_settings_block_may_set_reaches_the_configuration`
`verified-by: bravebot_config::settings::an_env_block_is_read`
`verified-by: bravebot_config::settings::a_proxy_and_an_authority_in_the_persons_own_file_are_handed_over`
`verified-by: bravebot_config::settings::a_checkouts_proxy_and_authority_are_reported_and_never_handed_over`
`verified-by: bravebot_config::settings::a_checkouts_proxy_alone_leaves_nothing_handed_over_and_its_other_names_alone`
`verified-by: bravebot_config::settings::a_checkouts_google_names_configure_no_service_and_the_persons_own_still_do`
`verified-by: bravebot_config::settings::a_named_file_states_the_network_unless_it_sits_in_the_checkout`
`verified-by: bravebot_config::settings::a_blank_or_non_string_network_value_states_nothing`
`verified-by: bravebot_agent::transport::the_settings_and_the_transport_name_the_same_variables`
`verified-by: bravebot_agent::transport::a_proxy_in_the_persons_file_is_the_one_a_client_carries_and_a_checkouts_is_not`
`verified-by: bravebot_config::bedrock::a_region_that_would_move_the_request_configures_nothing`
`verified-by: bravebot_config::bedrock::a_region_holding_only_the_characters_aws_uses_is_accepted`
`verified-by: bravebot_config::settings::a_checkouts_bedrock_names_configure_no_account_and_the_persons_own_still_do`
`verified-by: bravebot_config::settings::every_pinnable_name_is_listed_by_whether_a_checkouts_files_decide_it`
`verified-by: bravebot_config::settings::the_persons_own_files_decide_every_pinnable_name`

<a id="BACKEND-34"></a>
### BACKEND-34: a settings value is a string, and anything else is not a value

Every value in a settings block is a JSON string. A name spelled with a number, a boolean, a null, a
list or an object holds nothing, and holds nothing in the layers underneath it either: the closest
layer that spelled the name is the layer that answered for it. Every other name in that file is
unaffected.

A block spelled as anything but a block is answered for the same way, one level up. A layer whose
`env` is a number, a string, a list or a null sets no variable, and no variable is read from the
layers under it either. The file still parses, so this is not the failed-layer case BACKEND-24
describes, and the other blocks in it are read as they would have been.

**Why.** What a variable takes is a string, and JSON has a distinct spelling for each of the other
kinds. Coercing one invents a spelling the writer did not choose, `1` and `true` being far from
obviously `"1"` and `"true"` to whoever reads the value back later, and the values here are hosts
and credentials, where a guess about spelling is a guess about where a request goes. Dropping the name
rather than the file keeps the damage to the thing that was mistyped, since the rest of the file
still describes a working backend.

A weaker layer is not fallen through to because the name was answered, by the file closest to the
work. Falling through would make a typo in a checkout resolve quietly to a value from a file the
person was not looking at, which is the one outcome worse than the name being unset. That is the
reason the same thing happens to a whole block, and it is also where the rule costs the most: a
single mistyped `env` takes a person's own variables away with the checkout's.

`verified-by: bravebot_config::settings::a_value_that_is_not_a_string_is_left_out`
`verified-by: bravebot_config::settings::a_value_that_is_not_a_string_leaves_the_name_unset_in_every_layer`
`verified-by: bravebot_config::settings::a_block_that_is_not_a_block_leaves_no_names_under_it`
`verified-by: bravebot_config::settings::a_model_that_is_blank_or_not_a_string_names_nothing`
`verified-by: bravebot_config::settings::a_block_that_is_not_a_block_leaves_the_rest_of_its_file_read`

<a id="BACKEND-35"></a>
### BACKEND-35: an exported variable, then the build, then the file

For every name but the top-level `model` key, a value exported into the process environment outranks
one this binary was built with, which outranks the `env` block. A variable exported blank does not
displace a value the build carries; where the build carries none, the blank is what the
configuration holds, so a missing credential is reported as empty rather than as absent.

**Why.** An exported variable is the most specific thing a person said, and a file that overrode it
would make `AWS_PROFILE=other bravebot` do nothing. The build sits above the file so that a released
binary reaches the host it was given and signs with the credentials it was given, whatever the
`.bravebot` directory of the checkout somebody happens to be working in says. That layer is the
easiest thing on the machine to write to, which is the same reason BACKEND-1 gives for a file
granting no capability at all.

On a binary built with nothing, which is a source build, the file is what answers for all of it:
the endpoint, the key id and the signing key included, and a project layer can name any of them.
That is the case the file exists for, and what it costs is under Known costs.

BACKEND-11 is the one exception and says why: a `model` key ranked here would lose to the baked-in
default on every binary anybody was given, and an exported default would outrank every `model` key
on a machine whose `.envrc` sets one. A name a machine-level file pinned is resolved from that
file and from none of these three, which is BACKEND-38.

`verified-by: bravebot_config::lib::the_environment_outranks_the_settings_file`
`verified-by: bravebot_config::lib::a_baked_in_value_outranks_the_settings_file`
`verified-by: bravebot_config::lib::the_settings_file_applies_when_the_environment_is_silent`
`verified-by: bravebot_config::lib::a_blank_variable_does_not_shadow_a_built_in_value`
`verified-by: bravebot_config::lib::a_blank_variable_survives_when_nothing_was_built_in`
`verified-by: bravebot_config::lib::an_exported_value_outranks_the_build_which_outranks_the_file`
`verified-by: bravebot_config::lib::a_blank_export_hides_the_file_but_not_the_build`

<a id="BACKEND-36"></a>
### BACKEND-36: a name nothing reads is kept and decides nothing

Every name an `env` block sets is read into the settings and reported by `doctor` among the names
that file set, whether or not anything consults it. A name outside BACKEND-33's table decides
nothing: it configures nothing, it is not an error, and, like every name in the block, it is not
exported. The switch that hands this agent's own credentials back to a program it starts is read
from the process environment alone, so a file spelling that name changes nothing about what a
subprocess is given.

**Why.** This is the person's own configuration surface and a file people copy between tools, so it
holds names written for something else and names written for a later version of this one. Refusing
one would make a settings file from a newer release stop an older binary from starting, and
discarding one silently would make a typo and a forward-looking entry look identical to whoever is
debugging it, which is why `doctor` reports the names rather than only the ones that landed.

A top-level key beside the ones this build reads is kept on the same terms. The rest of the file
applies, the key configures nothing and restricts nothing, and `doctor` names it with the file that
set it, weakest first, every file that set one being named. The key is reported and what it was set
to is not. A variable in `env` is not reported this way, being named already among the names the file
set, and neither is a key inside a block this build reads.

That holds because the reason above is about the file rather than about the `env` block. The file is
documented as largely the shape of another tool's, so a block pasted from one arrives holding
`hooks` or `statusLine`, and the first reads to whoever wrote it as a check in force. Reporting the
key is also why the report still passes: a key written for a later release stopping an older binary
is the refusal this clause exists to avoid, and a report that failed on one would be that refusal
by a longer route.

Keeping a name is not the same as acting on one, and the distance between the two is the whole of
what makes the file safe to read. A block that could switch off credential scrubbing would be a
block that hands this agent's secrets to every command it runs, decided by whatever last edited a
file in a checkout.

`verified-by: bravebot_config::lib::a_name_nothing_consults_changes_nothing`
`verified-by: bravebot_config::settings::a_name_this_crate_does_not_know_is_still_read`
`verified-by: bravebot_config::settings::the_names_are_reportable_and_the_values_are_not`
`verified-by: bravebot_config::settings::a_key_beside_the_ones_this_build_reads_is_named_with_the_file_that_set_it`
`verified-by: bravebot_config::settings::a_key_this_build_reads_is_never_reported_as_unread`
`verified-by: bravebot_config::settings::a_variable_nothing_consults_is_named_among_the_names_and_not_as_an_unread_key`
`verified-by: bravebot_cli::running::doctor_names_a_top_level_key_it_does_not_read`

<a id="BACKEND-37"></a>
### BACKEND-37: failures carry safe reasons and measured request counts

Backend failures report a fixed category, an HTTP status when known, and the number of requests
handed to egress. The count includes retries and capability probes for streamed and whole replies.
A failure before egress has zero attempts; an uncounted error has an unknown count. A retry announced
before backoff does not count until it starts. Cancelling that wait retains the requests already sent.

Categories come from structured errors. Error bodies, headers, credentials, URLs, and raw transport
messages do not enter these details. Processor failures use a fixed category in their tool results;
delegate failures tell the planner that the delegate did not finish, and nothing about the failure
beyond that. Where the run had spent its round bound, the driver's own count of that bound is said
too ([DELEGATE-26](delegation.md#DELEGATE-26)), which is a number this process fixed before the run
started rather than anything read out of the answer that failed. Compaction failure messages also
use the category, except that one that failed on the ChatGPT account says that failure's sentence
([CHATGPT-8](chatgpt-sign-in.md#CHATGPT-8)). These paths do not copy raw backend errors into the
conversation.

Cancellation is distinct from failure. A processor carries cancellation and its attempt count
separately from its tool-result text, so the parent can report the stop without parsing that text.

Known Bedrock stream exceptions map to fixed categories: validation to refused, throttling to
rate-limited, and service-unavailable or internal-server errors to unavailable. Other exception
names map to incomplete. Retry eligibility is unchanged.

A streamed Bedrock reply that stops arriving once any of its body has arrived is incomplete, and
never a request that did not get through, whatever the transport said about the read that failed.
A read that fails before any of the body has arrived stays a transport failure and is not sent
again. A reply that stopped is sent again where its request asked for tool arguments as they are
written ([BACKEND-44](#BACKEND-44)) or offered no tool, and not where it offered one without the ask:
there the silence may be the service holding an argument back until the model has finished it, and a
second request waits out the same silence and is billed for it.

A streamed reply from an OpenAI-compatible service that stops arriving once any of its body has
arrived is incomplete in the same way, and is sent again as any reply that ended early is. A read
that fails before any of its body has arrived stays a transport failure. That client has no ask for
arguments as they are written, so nothing tells it a service holding one back from a connection
that died, and it sends the request again as it did before either was told apart.

A reply cut at the time its request stated for it ([NET-5](network-egress.md#NET-5)) is incomplete
too, streamed or asked for whole, and never a request that did not get through: the service had the
request and was still writing. It is not asked for again, since the same reply takes as long again.

A reply stopped at its output ceiling reports that ceiling alongside the category. It is this
program's own configured figure rather than anything a service said, so it is not a detail taken
from a reply, and without it the report names a limit and no way to change it.

A failure of a ChatGPT account reports which one it was beside its category
([CHATGPT-8](chatgpt-sign-in.md#CHATGPT-8)): the step of the sign-in that failed, the refusal that
names the account, the refusal of a model the account does not offer, or the class of a failed renewal.
It is one of a closed set, chosen by a step or by the client that sent the request comparing a code with
a closed list or with the one code of a model not offered, and no text from a reply is in it. A refusal
of the model is read apart from the closed list and is not a refusal of the sign-in. A failure of any
other kind reports none. A refusal that names the account also reports the status of the reply that
refused and, at a 429, the wait its `Retry-After` asked for, as any other refusal does, and one reported
in an event of a stream reports neither. A refusal of the model reports neither. The planner is told the
category and the request count and no more, as for every failure.

`verified-by: bravebot_agent::backend::each_status_a_service_answers_with_is_reported_as_what_it_means`
`verified-by: bravebot_agent::backend::a_gateway_with_nothing_holding_a_token_is_reported_as_unconfigured`
`verified-by: bravebot_agent::backend::aws_refusing_the_credentials_it_was_signed_with_is_reported_as_unauthorized`
`verified-by: bravebot_agent::backend::a_hop_refused_for_leaving_https_is_reported_as_a_gate_rather_than_a_failed_request`
`verified-by: bravebot_agent::backend::what_is_kept_about_a_failure_carries_nothing_the_service_or_the_setting_said`
`verified-by: bravebot_agent::backend::a_reply_stopped_at_the_ceiling_reports_which_ceiling`
`verified-by: bravebot_session::sessions::a_reply_stopped_at_a_ceiling_says_which_ceiling`
`verified-by: bravebot_agent::backend::a_diagnosis_carries_which_failure_of_the_account_it_was`
`verified-by: bravebot_agent::backend::a_failure_with_no_account_in_it_carries_no_account_value`
`verified-by: bravebot_agent::chatgpt_compaction::a_summary_that_failed_on_the_account_is_narrated_in_the_accounts_sentence`
`verified-by: bravebot_agent::report::a_failed_summary_says_the_accounts_sentence_where_it_was_the_account_and_the_category_otherwise`
`verified-by: bravebot_tui::app::a_compact_that_failed_on_the_chatgpt_account_says_its_own_sentence`
`verified-by: bravebot_agent::backend::a_refusal_that_names_the_account_keeps_its_status_and_wait_in_the_diagnosis`
`verified-by: bravebot_aichat::client::a_refusal_that_names_the_account_keeps_the_status_and_the_wait_it_came_with`
`verified-by: bravebot_agent::conversation::the_note_about_a_failed_turn_holds_the_category_and_request_count_and_nothing_else`
`verified-by: bravebot_agent::turn::a_service_that_kept_refusing_is_reported_with_its_status_and_the_attempts_made`
`verified-by: bravebot_agent::turn::a_reply_that_stopped_early_is_reported_as_unfinished_with_no_status`
`verified-by: bravebot_agent::turn::a_refusal_counts_the_cache_probe_as_a_second_request`
`verified-by: bravebot_bedrock::lib::request_counts_include_capability_probes`
`verified-by: bravebot_bedrock::lib::cancellation_in_backoff_counts_only_sent_requests`
`verified-by: bravebot_aichat::client::a_stop_does_not_wait_out_the_pause_between_attempts`
`verified-by: bravebot_aichat::client::a_stop_between_attempts_at_a_whole_reply_does_not_wait_out_the_pause`
`verified-by: bravebot_bedrock::lib::a_stop_between_attempts_at_a_whole_reply_does_not_wait_out_the_pause`
`verified-by: bravebot_bedrock::lib::framed_service_exceptions_keep_their_kind_and_request_count`
`verified-by: bravebot_bedrock::lib::a_reply_that_went_quiet_after_it_began_is_reported_as_stopping_and_asked_for_again`
`verified-by: bravebot_aichat::call::beginning_an_attempt_keeps_what_the_earlier_ones_were_billed`
`verified-by: bravebot_aichat::call::a_stop_that_lands_between_attempts_is_answered_before_the_next_is_sent`
`verified-by: bravebot_aichat::call::attempts_are_capped_and_a_concession_is_not_one`
`verified-by: bravebot_aichat::call::a_mended_failure_is_sent_again_at_once_and_counts_against_nothing`
`verified-by: bravebot_aichat::call::beginning_a_call_clears_the_count_and_both_usages`
`verified-by: bravebot_aichat::call::a_wait_ends_early_and_says_so_when_a_stop_lands_in_the_middle_of_it`
`verified-by: bravebot_aichat::call::a_wait_nobody_stops_finishes_and_says_so`
`verified-by: bravebot_aichat::call::a_wait_started_after_a_stop_does_not_sleep`
`verified-by: bravebot_bedrock::lib::a_reply_that_never_began_is_still_a_request_that_did_not_get_through`
`verified-by: bravebot_bedrock::lib::a_reply_that_went_quiet_while_an_argument_was_held_back_is_not_asked_for_again`
`verified-by: bravebot_bedrock::lib::only_a_request_whose_arguments_are_held_back_stalls_rather_than_stopping`
`verified-by: bravebot_aichat::client::a_stream_that_breaks_after_it_began_is_a_reply_that_stopped`
`verified-by: bravebot_aichat::client::a_stream_that_breaks_before_any_of_it_arrived_is_still_a_connection_failure`
`verified-by: bravebot_agent::backend::a_reply_that_stopped_arriving_is_reported_as_unfinished_and_not_as_unreachable`
`verified-by: bravebot_net::egress::a_reply_outlasting_the_time_its_request_stated_is_given_up_on`
`verified-by: bravebot_net::egress::a_reply_written_before_any_of_it_is_sent_is_out_of_time_when_it_outlasts_its_stated_bound`
`verified-by: bravebot_agent::failure_categories::service_exception_keeps_its_actionable_category`
`verified-by: bravebot_agent::turn::compaction_failure_narration_keeps_credentials_out`
`verified-by: bravebot_agent::turn::what_the_planner_is_told_about_a_failed_delegate_carries_nothing_of_the_endpoint`
`verified-by: bravebot_agent::turn::a_failed_processor_reports_a_category_and_nothing_the_service_or_the_setting_said`
`verified-by: bravebot_agent::turn::a_stop_counts_the_requests_that_were_sent_and_no_others`
`verified-by: bravebot_agent::turn::a_stop_while_a_processor_runs_is_reported_as_a_stop_with_what_it_sent`

<a id="BACKEND-38"></a>
### BACKEND-38: one machine-level file pins a destination above everything a person can set

A file in the directory the platform reserves for an administrator answers for the names it pins,
above the process environment and therefore above every other source. It is
`/etc/bravebot/managed.json`, `/Library/Application Support/bravebot/managed.json` on macOS, and
`C:\ProgramData\bravebot\managed.json` on Windows. The path is a literal and no variable names it.

These names may be pinned, being the ones that decide where a request goes:
`BRAVE_AI_CHAT_ENDPOINT`, `BRAVE_AI_CHAT_PREMIUM_ENDPOINT`, `BRAVEBOT_USE_BEDROCK`, `AWS_REGION`,
`AWS_PROFILE`, the three tier models of BACKEND-33's table, and the `provider` block. Every other
name in the file decides nothing, the signing key and the key id included, save the server lists
below. A pinned name is resolved from this file alone, and a name it does not pin resolves exactly
as it would with no such file.

The file may also keep an MCP server from starting, and that is the one thing it decides that is
not a destination: `"mcp": { "allow": [...], "deny": [...] }` names servers by the host a url
reaches or the command a program runs, and a session starts none the lists refuse.
[SERVERS-12](mcp-servers.md#SERVERS-12) is those keys, and says why the layer may keep a server
from starting and never add one.

No credential is read from this file. A gateway entry's `apiKey` is dropped, and the entry's host,
models and variable names are honoured without it. A key the person stored for the entry's id with
`bravebot auth login gateway` is sent to it, as a variable it names is.

The `provider` block is pinned whole rather than a name at a time, and a block that is present and
empty says that there are no gateways. A file that does not have the block, or that spells it as
anything but a block, leaves the gateways a person configured in force.

Refusing every account but the organisation's takes both halves: the switch pinned off and the
`provider` block pinned, since a gateway entry can name an AWS account too.

The file is read as the settings files of BACKEND-24 are: one that is missing, larger than 64 KB or
unparseable pins nothing, and a value that is blank or is not a string pins nothing under that name.

**Why.** Every other source is ultimately the individual's. The environment sits at the top of
BACKEND-35's order so that a released binary can be pointed at a local backend without rebuilding
it, and that convenience is what this deliberately inverts: a pin an exported variable outranked
would pin nothing, so an organisation requiring that inference traffic reach an approved endpoint,
or refusing to have models reached through somebody's personal cloud account, would have no way to
say it.

The authority is the filesystem's rather than this program's. The file sits in the directory the
platform reserves for administration, and nothing here checks who owns it or what the permissions on
it are: somebody who can write that path can replace this binary, so a check would add a thing to
get wrong and settle nothing. How far that argument holds per platform is under Known costs. It is
also why the path is a literal. `%ProgramData%` and the rest are stated in the environment of the process,
which is the environment of the person this layer binds, so reading one would let them choose which
file answers for them.

A name at a time, and only these names, because a layer that can pin a preference is a layer
somebody uses to pin a preference. What two parties have a legitimate say in is where a request goes
and whose account pays for it; which theme is on and which keys do what are neither, and pinning one
of those is an administrator reaching past the thing they have a stake in. The credential names are
out for a different reason: a file here names a destination and grants nothing, which is BACKEND-1's
rule and holds hardest for a file a person cannot read in their own directory. A gateway's own
credential field is dropped rather than obeyed for the same reason plus one more: everyone on the
machine can read this file, so a token in it is a token handed to every account rather than one held
by its owner. Dropping it rather than refusing the entry keeps the host, which is the part worth
pinning, and the service says what is missing on the first request.

The gateway block is whole because pinning an endpoint pins nothing while anybody may add a
destination beside it, and a gateway entry can name an AWS account as readily as a host, which is
why refusing a personal account needs the block and not just the switch. Saying there are none has
to be sayable, since "our endpoint or nothing" is half of what an organisation deploying this file
means, and absence has to stay distinguishable from it, or a file pinning only a host would silently
take away a gateway it never mentioned. A `provider` spelled as anything but a block is a mistyped
file rather than either statement: taking every gateway on the machine away on the strength of a
stray `null` is the one reading of it nobody would intend.

Failing softly on a bad file is BACKEND-24's argument one layer up: a mistake in a file nobody can
edit must not decide that the program no longer starts, and the remedy is with whoever can write
that path rather than with the person in front of the screen.

`verified-by: bravebot_config::managed::an_endpoint_is_pinnable`
`verified-by: bravebot_config::managed::a_name_outside_the_pinnable_set_pins_nothing`
`verified-by: bravebot_config::managed::every_name_the_clause_lists_is_pinnable_and_the_set_holds_no_other`
`verified-by: bravebot_config::managed::the_file_is_read_from_the_directory_the_platform_reserves`
`verified-by: bravebot_config::managed::the_switch_that_reaches_a_personal_account_is_pinnable`
`verified-by: bravebot_config::managed::an_empty_gateway_block_says_there_are_no_gateways`
`verified-by: bravebot_config::managed::a_file_with_no_gateway_block_leaves_the_gateways_alone`
`verified-by: bravebot_config::managed::a_provider_key_that_is_not_a_block_decides_nothing`
`verified-by: bravebot_config::managed::a_gateway_block_names_the_gateways_in_force`
`verified-by: bravebot_config::managed::a_token_written_into_the_file_is_not_read`
`verified-by: bravebot_config::managed::an_absent_file_pins_nothing_and_is_not_reported`
`verified-by: bravebot_config::managed::an_unparseable_file_pins_nothing_and_is_still_named`
`verified-by: bravebot_config::managed::a_blank_value_pins_nothing`
`verified-by: bravebot_config::lib::a_managed_pin_outranks_an_exported_variable`
`verified-by: bravebot_config::lib::a_name_the_managed_layer_did_not_pin_resolves_as_it_would_have`
`verified-by: bravebot_config::lib::a_pinned_switch_outranks_the_exported_one`
`verified-by: bravebot_config::lib::refusing_a_personal_cloud_account_takes_the_switch_and_the_gateways`
`verified-by: bravebot_config::lib::a_managed_gateway_block_replaces_the_one_in_the_settings`
`verified-by: bravebot_config::lib::an_empty_managed_gateway_block_leaves_no_gateways`
`verified-by: bravebot_config::lib::a_managed_layer_silent_on_gateways_keeps_the_configured_ones`

<a id="BACKEND-39"></a>
### BACKEND-39: a configuration naming no model service says what to configure

Where the model in force would be sent to Brave's own aichat endpoint and no premium key is stored to
present to it ([PREM-13](premium-credentials.md#PREM-13)), nothing is configured to serve the turn and
no work starts.
A session does not open and a one-shot run asks nothing. What is said instead is the five ways to
configure a service that can answer, each naming the thing to type or to write: an AWS account
through Bedrock, an OpenAI-compatible gateway, signing in with a ChatGPT account
(`bravebot auth login chatgpt`, [CHATGPT-1](chatgpt-sign-in.md#CHATGPT-1)), signing in with an xAI
account (`bravebot auth login xai`, [XAI-7](xai-sign-in.md#XAI-7)), and Brave's premium service for
somebody who has a key for it, stored by running `bravebot auth login brave` in a terminal and typing
the key. The line for each sign-in says that the account's models are then
offered in `/model` as `chatgpt/<model>` and `xai/<model>`. `doctor` says the same and fails, which is
[CLI-7](cli.md#CLI-7).
`bravebot --bg <prompt>` is refused the same way, in the terminal that typed it, before the
background process is started.

The model decides, as it does everywhere here: a model a configured service serves is not this
case, whatever the aichat fields hold. The model asked about is the one the run will actually
request, which is a name given on the command line, then the one a session recorded unless
[BACKEND-47](#BACKEND-47) sets it aside, then the configured default. A start by `bravebot`,
`bravebot -i`, `bravebot --plain`, `bravebot --bg` or a one-shot run asks about that model, and the
desktop window does not. `bravebot -i "<prompt>" --model chatgpt/<model>` therefore opens where a stored
ChatGPT sign-in serves the model named, and is refused where nothing serves it, whichever model would be
in force without the flag.

An endpoint that is not one of Brave's own deployments is not this case either, development channel
and production alike. That is somebody's own host, a local model server or a private deployment or
a proxy in front of either, and nobody is handed a configuration pointing at one. A port written
into an endpoint does not change which deployment it names.

**A service configured while the model in force is Brave's own is told apart, and gets one line
rather than the routes.** That is where a settings block copied out of the tool BACKEND-13 borrows
its shape from lands: those blocks name their models and name no default, so the model stays the
one the build baked in. What is said there is the key that names one of the configured service's
models, and the routes are left out.

**A stored ChatGPT or xAI sign-in is such a service.** Each is a way to configure one, beside a Bedrock
block, a gateway block and a premium key, and is read only where none of those has answered. A
Bedrock or gateway block that is configured keeps the line above whatever else is stored: neither file is
read, so a person who set up an AWS account or a gateway and also signed in to ChatGPT or xAI is asked
which of that service's models to use where a person can be asked (below), and is told to name a model with
the `model` key where that is declined or cannot be asked, and neither the question nor that line is about
an account. A sign-in file that cannot be read is not said there, since it is not read; `doctor` reports it. A start whose
model in force is Brave's own and that finds a sign-in stored and nothing else configured, and that is not
asked which model to use (below) or declines, is told which sign-in is stored, and to name one of the
account's models in a command that works as typed: `bravebot -i "<prompt>" --model chatgpt/<model>` or
`--model xai/<model>` opens a session on it, and `bravebot "<task>" --model chatgpt/<model>` or `--model
xai/<model>` runs a task on it. `bravebot --model xai/<model>` alone starts nothing, since a task is
required, and is not named. `/model` is named only as what lists the account's models once a session is
open. It says that `bravebot` started on its own asks which one to use, in a terminal outside an incognito
session, where the account lists its models, because a one-shot run, a start that names a model and `doctor`
end in the same refusal and ask nothing. It is not shown the routes, and no import is offered, since a
service is configured. Where both sign-ins are stored it says so once, in one line that names both, and
names a command for each account's models. A sign-in file that cannot be read configures nothing and is said
first, with the remedy of [CHATGPT-11](chatgpt-sign-in.md#CHATGPT-11) or of
[XAI-8](xai-sign-in.md#XAI-8), above the routes. `doctor` says it once, in its own line for the sign-in. A
model that a configured service serves does not read either file.

**A start with a stored sign-in asks which of the accounts' models to use before it refuses.** Where the
start would be refused for the stored sign-in or sign-ins alone, no model was named on the command line, an
account lists at least one model, stdin, stdout and stderr are all terminals and the session is not
incognito, the rows of the accounts that listed some are shown as numbered lines, the ChatGPT account's
first and the xAI account's after them, each account's in the order and without the models the pickers
leave out, and the start asks which to use. Only an account whose sign-in is stored is asked. A heading
over the rows names the accounts that gave them, so a number is the one row it numbers whichever account
it belongs to, and a pick of a row is recorded under its own identifier. The question goes to stderr and is read from
stdin a line at a time, as the question about an import is ([import.md](import.md)). A number picks
its row. The pick is recorded as a `/model` pick is, and the start goes on and opens on it, so the next
start has it in force and asks nothing. The start says the pick is remembered only where what it reads
back is that name. Where it is not, because the name is too long to keep or the write failed, it says
it could not remember the name, does not say it is using it, and is refused as a decline is. An empty
answer, the end of the input and an answer that is not the number of a row decline, and a decline
records nothing: the start is refused as it is where nothing can be asked. Nothing is asked, and
nothing is read from stdin, where a person cannot be asked or a model was named on the command line.
The start never chooses a model for the person.

An account is asked for its listing only where a person can be asked, and that may renew the access
token and write the sign-in file, as any turn does ([CHATGPT-14](chatgpt-sign-in.md#CHATGPT-14),
[XAI-17](xai-sign-in.md#XAI-17)), or remove it where the sign-in has ended. Where an account offers no row,
because the listing could not be had, was refused with a code that names the account, is empty, or the
sign-in ended while it was asked for, that account is said, in the sentence the sign-in command says it in
for ChatGPT and in a sentence of its own for xAI. Where another account gave rows the sentence is said above
them and the rows are offered; where none did, nothing is asked and the refusal says why, under its first
line. The compiled names that stand in for a listing in a picker are not rows here, and no list is built in
for the xAI account. The refusal is built from the sign-ins as they stand after the listings, so a sign-in a
listing found ended is said to have ended, with the routes, and is not said to be stored.

**A start with a Bedrock or gateway block asks the same question about that service's models.** Where
the start would be refused for the model in force being Brave's own, a Bedrock or gateway block is
configured, no model was named on the command line, a configured service lists at least one model that
the machine-level layer allows, stdin, stdout and stderr are all terminals and the session is not
incognito, those models are shown as numbered lines in the picker's order and the start asks which to
use, answered, recorded and declined exactly as above. A stored sign-in beside such a block is not
asked about and not read, so the question is about the block's services alone. A gateway whose block
names no models is asked for its listing, and one that cannot be asked offers nothing, so the start
goes on to the refusal that names the `model` key. A one-shot run, a start that names a model and
`doctor` ask nothing.

The refusal is the configuration ending of [CLI-6](cli.md#CLI-6) rather than a status of its own.

**Why.** Nothing has been set up yet. A released binary arrives pointed at Brave's own aichat
endpoint, and being pointed at it is not the same as having a service configured to do this work: no
account was named, no gateway was written down, and no premium key was stored. A run that went
ahead would send the turn there anyway and hand back whatever that request became, and whatever it
became is not the agent doing the work. What a person is left with is a session that failed, or one
that answered poorly, and nothing on the screen saying the missing piece was a configuration. The
conclusion drawn from a first session is the one that sticks.

Said before anything runs because that is the only moment where the answer is "configure a
service". Afterwards the question a person has is why the answers are bad, which is a question the
configuration cannot be reached from.

Refusing rather than letting the request go, for the reason
[PREM-10](premium-credentials.md#PREM-10) fails a request that cannot present its key rather than
sending it without one: what comes back from a request that was never going to be served carries no
sign of why, and nothing about a bad session points at where a request went.

Five routes rather than one, and each named by what to type, because a refusal that states the
problem and no way out of it has moved the work to the person and told them nothing they could not
already see. The premium key is last and says why it is last: those models are reached through
Brave's AI gateway, which is still in development. It is offered rather than left out, for somebody
who has a key for it. The ChatGPT
and xAI sign-ins come before it, since each reaches its own service and not Brave's gateway, and each says
the command to type because a session never starts a sign-in itself
([CHATGPT-1](chatgpt-sign-in.md#CHATGPT-1), [XAI-7](xai-sign-in.md#XAI-7)).

A person who has only signed in has no model name to give. The names are known from the account's
listing, and a session that would show the listing does not open until a model is served. Asking at the
start puts the listing in front of the person at the moment the start would be refused, and the answer
applies at once, to a sign-in made earlier as well as to one made now. The xAI account's start is the
same question over the same mechanism, so the two accounts are asked by one start: a person with both
signed in is asked once, over one list, and one account that is down does not keep the other's models
from being chosen. A model named on the command
line is left alone because it outranks a recorded pick, so a pick made there would change nothing.

Telling the configured case apart is the same argument in the other direction. Somebody there is
one settings key from working, and five ways to set up a service would be five things to read
past on the way to the one that applies, the first of which is the thing they already did.

The test is the endpoint rather than a switch somebody sets. A build pointed at a host that is not
Brave's has been configured by whoever pointed it, and that covers every development build and
every local service without a preference to store. A switch would be the thing set once and
forgotten, which is how an unconfigured build comes back on the machine of the person who was going
to configure something properly later.

What would change this clause is a released binary arriving with a model service already set up. The
refusal exists because one does not.

`verified-by: bravebot_agent::backend::braves_endpoint_with_nothing_beside_it_has_no_service_configured`
`verified-by: bravebot_agent::backend::a_model_a_configured_service_serves_has_one`
`verified-by: bravebot_agent::backend::a_service_configured_while_the_model_is_braves_own_has_none_for_that_model`
`verified-by: bravebot_agent::backend::an_endpoint_that_is_not_braves_is_a_service_that_was_chosen`
`verified-by: bravebot_agent::backend::a_premium_key_with_a_braves_premium_host_is_a_service_configured`
`verified-by: bravebot_agent::subscription::braves_own_deployments_are_told_from_a_host_somebody_chose`
`verified-by: bravebot_agent::subscription::a_port_is_not_part_of_the_host`
`verified-by: bravebot_cli::running::a_first_run_with_no_service_configured_says_how_to_configure_one`
`verified-by: bravebot_cli::running::a_configured_gateway_is_not_refused`
`verified-by: bravebot_cli::running::a_service_configured_with_no_model_of_its_own_named_says_to_name_one`
`verified-by: bravebot_cli::running::a_model_named_on_the_command_line_is_not_refused`
`verified-by: bravebot_cli::running::a_session_in_lines_with_no_service_configured_says_how_to_configure_one`
`verified-by: bravebot_cli::running::a_session_in_lines_with_a_configured_gateway_opens`
`verified-by: bravebot_cli::running::a_background_session_with_no_service_configured_says_how_to_configure_one`
`verified-by: bravebot_cli::model::the_routes_of_an_unconfigured_start_each_name_what_to_type_or_write_and_leave_the_premium_key_last`
`verified-by: bravebot_cli::model::a_configured_service_is_told_to_name_a_model_and_is_not_offered_the_routes`
`verified-by: bravebot_cli::running::a_first_run_names_the_chatgpt_sign_in_in_the_language_it_is_read_in`
`verified-by: bravebot_agent::backend::a_stored_chatgpt_sign_in_is_a_service_configured_and_an_unreadable_one_is_not`
`verified-by: bravebot_agent::backend::a_configured_bedrock_or_gateway_block_keeps_its_line_whatever_a_sign_in_holds`
`verified-by: bravebot_cli::running::a_stored_chatgpt_sign_in_beside_a_configured_gateway_leaves_the_line_that_names_the_model_key`
`verified-by: bravebot_agent::chatgpt::a_stored_account_is_told_apart_from_an_absent_one_and_from_one_that_cannot_be_read`
`verified-by: bravebot_cli::chatgpt_start::a_start_with_a_configured_gateway_asks_which_model_and_the_pick_is_what_the_next_start_opens_on`
`verified-by: bravebot_cli::chatgpt_start::a_declined_gateway_question_records_nothing_and_ends_at_the_refusal`
`verified-by: bravebot_cli::chatgpt_start::nothing_is_asked_where_a_model_is_served_or_a_gateway_is_configured`
`verified-by: bravebot_cli::model::a_stored_chatgpt_sign_in_is_told_to_name_one_of_the_accounts_models_and_not_offered_the_routes`
`verified-by: bravebot_cli::running::a_stored_chatgpt_sign_in_is_a_service_configured_for_the_first_run`
`verified-by: bravebot_cli::running::doctor_treats_a_stored_chatgpt_sign_in_as_a_service_configured`
`verified-by: bravebot_cli::running::doctor_ends_on_the_configuration_status_where_nothing_will_serve_a_turn`
`verified-by: bravebot_cli::chatgpt_start::a_session_on_a_prompt_starts_on_the_model_the_command_line_names_where_the_account_serves_it`
`verified-by: bravebot_cli::chatgpt_start::a_session_on_a_prompt_naming_a_model_nothing_serves_is_refused_whatever_the_record_serves`
`verified-by: bravebot_cli::chatgpt_start::a_start_with_only_a_sign_in_asks_which_model_and_the_pick_is_what_the_session_and_the_next_start_open_on`
`verified-by: bravebot_cli::chatgpt_start::the_interface_that_draws_asks_the_same_question_and_draws_once_a_row_is_picked`
`verified-by: bravebot_cli::chatgpt_start::an_answer_that_is_not_the_number_of_a_row_declines_and_records_nothing`
`verified-by: bravebot_cli::chatgpt_start::nothing_is_asked_where_the_account_lists_no_model_and_the_refusal_says_why`
`verified-by: bravebot_cli::chatgpt_start::a_sign_in_that_ended_while_the_listing_was_asked_for_is_not_said_to_be_stored`
`verified-by: bravebot_cli::chatgpt_start::a_background_start_asks_in_the_terminal_and_a_decline_ends_it`
`verified-by: bravebot_cli::plain::a_typed_number_picks_the_row_it_numbers_and_nothing_else`
`verified-by: bravebot_cli::chatgpt_start::a_start_that_is_not_in_a_terminal_asks_nothing_reads_nothing_and_does_not_ask_the_account`
`verified-by: bravebot_cli::chatgpt_start::an_incognito_start_asks_nothing_and_records_nothing`
`verified-by: bravebot_cli::chatgpt_start::nothing_is_asked_where_a_model_is_served_or_a_gateway_is_configured`
`verified-by: bravebot_cli::chatgpt_start::a_model_named_on_the_command_line_is_not_asked_about`
`verified-by: bravebot_cli::chatgpt_start::a_pick_the_start_cannot_read_back_is_not_said_to_be_remembered_and_ends_the_start`
`verified-by: bravebot_cli::chatgpt_start::every_command_the_refusal_names_works_as_typed`
`verified-by: bravebot_cli::chatgpt_start::the_refusal_says_which_start_asks_and_the_starts_that_show_it_without_asking_say_so`
`verified-by: bravebot_cli::model::the_rows_are_numbered_from_one_in_the_order_given_and_the_number_typed_picks_its_row`
`verified-by: bravebot_cli::model::an_answer_that_is_not_the_number_of_a_row_picks_none`
`verified-by: bravebot_cli::model::a_name_the_service_gave_is_shown_without_its_control_characters`
`verified-by: bravebot_agent::backend::a_stored_xai_sign_in_is_a_service_configured_and_an_unreadable_one_is_not`
`verified-by: bravebot_agent::xai::a_stored_sign_in_is_told_apart_from_an_absent_one_and_from_one_that_cannot_be_read`
`verified-by: bravebot_cli::model::a_stored_xai_sign_in_is_told_to_name_one_of_the_accounts_models_and_not_offered_the_routes`
`verified-by: bravebot_cli::model::where_both_accounts_are_signed_in_the_refusal_says_both`
`verified-by: bravebot_cli::model::an_xai_sign_in_that_cannot_be_read_configures_nothing_and_the_routes_stay`
`verified-by: bravebot_cli::model::a_start_asks_the_accounts_that_are_stored_and_nothing_else`
`verified-by: bravebot_cli::model::an_xai_listing_that_gave_no_row_says_which_way_and_the_heading_names_the_accounts_that_offered`
`verified-by: bravebot_tui::app::a_start_is_offered_what_the_xai_account_listed_in_the_pickers_order_and_nothing_else`
`verified-by: bravebot_cli::xai_sign_in::a_start_with_only_an_xai_sign_in_asks_which_model_and_the_pick_is_what_the_session_and_the_next_start_open_on`
`verified-by: bravebot_cli::xai_sign_in::the_interface_that_draws_asks_the_same_question_for_an_xai_sign_in_and_draws_once_a_row_is_picked`
`verified-by: bravebot_cli::xai_sign_in::a_start_with_both_accounts_signed_in_asks_over_both_lists_and_records_the_pick_of_either`
`verified-by: bravebot_cli::xai_sign_in::an_account_that_gave_no_row_is_said_above_the_rows_the_other_account_gave`
`verified-by: bravebot_cli::xai_sign_in::an_xai_listing_that_gave_no_row_ends_the_start_with_the_sentence_that_says_which_way_it_did_not`
`verified-by: bravebot_cli::xai_sign_in::an_xai_sign_in_that_ended_while_the_listing_was_asked_for_is_not_said_to_be_stored`
`verified-by: bravebot_cli::xai_sign_in::a_start_that_is_not_in_a_terminal_asks_nothing_reads_nothing_and_names_the_xai_commands`
`verified-by: bravebot_cli::xai_sign_in::the_routes_offered_where_nothing_is_stored_include_xai_with_the_command_and_the_name_its_models_have`
`verified-by: bravebot_cli::xai_sign_in::an_incognito_start_with_an_xai_sign_in_asks_nothing_and_records_nothing`
`verified-by: bravebot_cli::xai_sign_in::nothing_is_asked_where_a_model_is_named_or_served_or_a_gateway_is_configured`
`verified-by: bravebot_cli::xai_sign_in::an_unreadable_xai_sign_in_beside_a_configured_gateway_is_not_read_at_the_start`
`verified-by: bravebot_cli::xai_sign_in::every_command_the_xai_refusal_names_works_as_typed`
`verified-by: bravebot_cli::xai_sign_in::doctor_says_an_xai_sign_in_is_stored_where_a_start_would_and_asks_the_account_nothing`
`verified-by: bravebot_cli::xai_sign_in::an_xai_sign_in_that_cannot_be_read_is_said_before_the_routes_and_the_report_says_it_once`
`verified-by: bravebot_tui::app::a_start_is_offered_what_the_account_listed_in_the_pickers_order_and_nothing_else`
`verified-by: bravebot_tui::app::a_start_is_not_offered_a_model_the_machine_level_layer_refuses`
`verified-by: bravebot_tui::app::a_start_is_not_offered_an_xai_model_the_machine_level_layer_refuses`

<a id="BACKEND-40"></a>
### BACKEND-40: completed replies keep reported usage even when their content is unusable

Clients decode and validate usage separately from assistant content. A completed reply with empty
content, malformed content or tool calls, or an output-limit error retains valid reported usage.
Earlier valid text does not make a malformed reply usable, including when a later JSON frame is
damaged. The error keeps its original category, request count and safe reporting path.

For a whole reply, the response body must have arrived. For a stream, the protocol must say the
reply ended: an aichat finish reason or `[DONE]`, or a Bedrock message stop. Transport EOF is not
that signal. Cancellation after protocol completion retains reported usage even while the socket
is open, and still returns cancellation. Usage metadata on its own does not complete a reply.
A later stream exception or framing error does not erase usage already reported for a completed
reply, even when it arrives in the same read as the completion and usage frames. If it
causes a retry, the completed attempt's cost remains charged exactly once whether the later
attempt succeeds, fails, or is cancelled during backoff. Reply content and progress restart on
retry. Each attempt starts with unknown usage; completed costs from earlier attempts remain
separate and are added to the call total. Unfinished attempts contribute no estimate. A total
containing a measured zero remains known; a call with no completed reported usage remains unknown.
The final attempt's prompt measurement is separate from these cumulative costs.

Retained usage resets at the start of each call, including a call cancelled before sending.
Unknown usage remains distinct from a measured zero. Both input and output token counts must be
present as unsigned integers; missing counts do not mean zero.
Invalid or absent usage, and usage from an unfinished reply, contributes no estimate to a failed
or cancelled request. Backend errors carry
known usage to callers without exposing response content or changing the failure diagnosis.

**Why.** A service can finish and charge for a reply that the caller cannot use. Losing its bill
would understate the cost; charging an unfinished reply would claim a cost not yet known.

`verified-by: bravebot_aichat::client::completed_retry_usage_survives_success_failure_and_backoff_cancellation`
`verified-by: bravebot_bedrock::lib::completed_retry_usage_survives_success_failure_and_backoff_cancellation`
`verified-by: bravebot_aichat::client::malformed_whole_reply_keeps_known_usage`
`verified-by: bravebot_aichat::client::malformed_streamed_reply_keeps_known_usage`
`verified-by: bravebot_bedrock::lib::malformed_replies_keep_only_valid_reported_usage`
`verified-by: bravebot_bedrock::lib::output_limit_keeps_completed_usage`
`verified-by: bravebot_bedrock::lib::completed_usage_survives_later_exception`
`verified-by: bravebot_bedrock::lib::completed_usage_survives_later_corrupt_frame`
`verified-by: bravebot_bedrock::eventstream::events_before_corruption_survive_any_read_boundary`
`verified-by: bravebot_aichat::client::malformed_json_stream_keeps_usage_without_accepting_earlier_text`
`verified-by: bravebot_bedrock::lib::cancellation_before_eof_keeps_only_protocol_completed_usage`
`verified-by: bravebot_agent::turn::completed_stream_keeps_usage_when_cancelled_before_socket_closes`
`verified-by: bravebot_aichat::call::the_total_is_the_retried_usage_plus_the_completed_usage`
`verified-by: bravebot_aichat::call::a_measured_zero_stays_known_and_nothing_measured_stays_unknown`
`verified-by: bravebot_aichat::call::an_attempt_that_reports_nothing_replaces_the_one_before_it_with_nothing`
`verified-by: bravebot_aichat::call::billing_adds_the_earlier_attempts_to_the_reply`
`verified-by: bravebot_aichat::call::usage_of_the_earlier_attempts_is_carried_into_the_reply`

<a id="BACKEND-41"></a>
### BACKEND-41: how long a reply may run belongs to the model, and an exported figure outranks it

A Bedrock request states a ceiling on the reply, and so does a request to an `anthropic` entry, whose
API requires one. A configured model may state its own, out of the same `limit` block its context
window comes from and under the same rule. One exported variable
states a ceiling for every model this build reaches and outranks whatever any of them stated. Where
nothing states one, the request carries an assumed 32,000 tokens. A model that refuses a request
carrying the assumed figure is asked again with 8,192, and no later request carries the assumed
figure to that model, on the terms [BACKEND-22](#BACKEND-22) gives the level. A stated figure is
never lowered. Nothing is asked over the network to find out, and nobody is required to supply it.

The ceiling a request carries also says how long its reply is given
([NET-5](network-egress.md#NET-5)): two minutes to read the prompt, then as long as the ceiling
takes to write at 25 tokens a second, and never less than the bound the egress layer puts on a
reply of unstated length. A ceiling of 32,000 tokens is given 23 minutes and 20 seconds, one of
8,192 the egress layer's ten minutes. A streamed reply is given it from the moment it begins, and
waits to begin as long as any other reply; one asked for whole is given it for that wait as well,
since the model writes it before any of it is sent. The gap allowed between two pieces of the reply
is the egress layer's whatever the ceiling, or the one the account's block states
([BACKEND-55](#BACKEND-55)).

The aichat endpoint and an OpenAI-compatible gateway state no ceiling at all, so nothing here applies
to them: their requests carry no such field and whatever bounds a reply there belongs to the service.
A Responses request carries `max_output_tokens` where somebody stated a ceiling and is given the time it
allows on the same terms; where none was stated it carries none and is bounded as a gateway's is.
An `anthropic` entry is given the time its ceiling allows on the same terms as Bedrock.

**Why.** A ceiling below what the model allows costs the tail of a long answer, and a tool call the
ceiling cuts off costs the call, [BACKEND-42](#BACKEND-42) keeping the text and never the calls. One
above what it allows is a request the service refuses. Bedrock fronts models from several providers
whose ceilings differ by an order of magnitude, an inference-profile ARN does not say which model is
behind it, and no endpoint there reports the figure, so there is nothing to resolve a guess against.
The refusal is the answer, and it costs one request per model per process, so the assumed figure is
one that holds for the models a tier usually names rather than for the least of them: 32,000 is
within what every model on Anthropic's current lineup allows, the smallest allowing 64,000. The
fallback has to hold for a model nothing here can identify, so it is low.

The assumed figure is not the largest those models allow, because it is also a reservation. Bedrock
deducts a request's input and its ceiling from the account's tokens-per-minute quota when the request
starts, and refunds the difference when it ends, so on an inference profile a team shares a larger
ceiling is fewer requests at once before any of them is throttled.

The time a reply is given follows its ceiling because the ceiling is the one figure here that says
how long the reply can run. At the slowest rate measured on Bedrock, about 69 tokens a second on
Claude Opus 5.5, the egress layer's ten minutes end any reply past about 41,000 tokens while the
model is still writing it, and a stated ceiling of 128,000 needs half an hour. The reply cut off is
billed, and asked for again where [BACKEND-37](#BACKEND-37) sends a stopped reply again, to be cut
off at the same point. The rate assumed is well under half the slowest measured so that a slow day
still finishes, and the bound is never lowered below the egress layer's because a reply short
enough for that bound was never the problem.

A stated figure is somebody's statement about the model. Sending less in its place would cut short
the replies they said could run, and a refusal of the figure they chose is theirs to see. The
variable exists because the three tier names have no block to state anything in, and they are how
most people reach this backend: without it the only models whose ceiling could be raised would be
the ones a `provider` block already named.

`verified-by: bravebot_config::provider::a_stated_reply_ceiling_is_read_per_model`
`verified-by: bravebot_config::provider::a_limit_missing_either_half_states_no_window`
`verified-by: bravebot_config::lib::an_exported_reply_ceiling_outranks_every_stated_one`
`verified-by: bravebot_config::lib::a_reply_ceiling_that_is_not_a_figure_leaves_the_stated_one_standing`
`verified-by: bravebot_config::lib::an_exported_reply_ceiling_reaches_an_anthropic_entry_ahead_of_its_models_own`
`verified-by: bravebot_bedrock::lib::a_request_carries_the_ceiling_its_own_model_states`
`verified-by: bravebot_bedrock::lib::an_assumed_ceiling_a_model_refuses_is_stepped_down_and_remembered`
`verified-by: bravebot_bedrock::lib::a_stated_ceiling_is_never_stepped_down`
`verified-by: bravebot_bedrock::lib::the_level_is_given_up_before_the_ask_for_arguments_as_written`
`verified-by: bravebot_bedrock::lib::the_time_a_reply_is_given_follows_the_ceiling_its_request_carries`
`verified-by: bravebot_bedrock::lib::a_reply_still_being_written_is_not_cut_off_by_a_bound_its_ceiling_did_not_set`
`verified-by: bravebot_config::provider::an_anthropic_reply_ceiling_is_the_exported_budget_then_the_models_own`
`verified-by: bravebot_aichat::client::an_assumed_ceiling_a_model_refuses_is_stepped_down_and_a_stated_one_never_is`
`verified-by: bravebot_aichat::client::an_exported_reply_ceiling_is_the_one_an_anthropic_request_carries`
`verified-by: bravebot_aichat::anthropic::a_reply_is_given_the_time_its_ceiling_takes_to_write`

<a id="BACKEND-42"></a>
### BACKEND-42: a reply the ceiling stopped is kept for what it said and never for what it asked

A reply that reaches its ceiling having written something is returned as a reply, marked as having
stopped short, with its reported usage. Its tool calls are not returned: a reply cut off carries
none, whatever the service sent, so no call from it is made; the turn tells the planner why and
asks once for less ([TURN-7](turns.md#TURN-7)). A reply that reaches the ceiling having written
nothing is a failure, and the failure names the ceiling.

Either way what the reply was doing when it stopped travels with it: the call it was part way
through, where its last block was one, with how many bytes of its arguments had arrived, and
whether any of its reasoning arrived. The call's tool is named as the request offered it, and a name the request never offered is reported as a tool
call and nothing more. A failure says which of the three it was: part way through a call,
thinking, or neither.

**Why.** Everything the model wrote before the cutoff is the turn's work, and reporting only that
it was too long destroys it to say so. The same argument settled the same question for a capped
`run`, whose output is collected after the kill rather than thrown away with the error. The usage
half of this is BACKEND-40 and was settled first; the text is worth more than the bill.

The calls are the exception because a cutoff lands wherever the model happened to be. Arguments
that stopped mid-string are not arguments, and a streamed call whose arguments had not begun
arrives as a call with none at all: `write_file` with an empty object, which is a call nobody
asked for being handed to a turn loop that would run it. Nothing distinguishes that from a
finished call except the stop reason, and the stop reason says not to trust any of them.

What the reply was doing is kept because the remedies differ and nothing else that survives the stop
says which applies. A reply that spent the ceiling on one file's worth of argument is asked for in
smaller parts, and one that spent it thinking is not helped by that. All of it is read from which
events arrived, never from what they said, and the one name is copied from the request, since the
report reaches the person and the planner and the reply's spelling of a name is text a page could
have written. The cost is a call to a tool the request did not offer being
reported without its name. A call with text after it is not the one reported: the model finished
it and was writing the text when the ceiling fell.

`verified-by: bravebot_bedrock::lib::a_reply_the_ceiling_stopped_keeps_its_text_and_asks_for_no_tools`
`verified-by: bravebot_bedrock::lib::output_limit_keeps_completed_usage`
`verified-by: bravebot_bedrock::lib::reaching_the_token_ceiling_is_not_retried`
`verified-by: bravebot_bedrock::lib::a_stopped_call_to_a_tool_nobody_offered_is_not_named`
`verified-by: bravebot_bedrock::lib::a_reply_the_ceiling_stopped_while_it_was_thinking_says_so`
`verified-by: bravebot_bedrock::lib::a_reply_the_ceiling_stopped_says_how_much_of_its_call_had_arrived`
`verified-by: bravebot_bedrock::lib::a_call_followed_by_text_is_not_the_one_the_ceiling_stopped`
`verified-by: bravebot_aichat::client::a_reply_the_ceiling_stopped_keeps_its_text_and_asks_for_no_tools`
`verified-by: bravebot_aichat::client::a_reply_the_ceiling_stopped_before_it_wrote_anything_names_the_ceiling`
`verified-by: bravebot_agent::failure_categories::a_reply_the_ceiling_stopped_before_it_wrote_anything_is_diagnosed_at_its_ceiling`
`verified-by: bravebot_agent::backend::a_reply_stopped_at_the_ceiling_reports_which_ceiling`
`verified-by: bravebot_session::sessions::a_reply_stopped_at_the_ceiling_says_what_it_was_writing`
`verified-by: bravebot_ui_bridge::wire::a_reply_stopped_at_the_ceiling_crosses_with_the_ceiling_and_what_it_was_writing`

<a id="BACKEND-43"></a>
### BACKEND-43: a settings file names the effort level, and a checkout's outranks a pick

An `effort` key in the settings files names how hard the model is asked to think, taking the words
`/effort` takes. A level recorded with `/effort` ranks as the person's own file does, which is the
rung [BACKEND-11](#BACKEND-11) gives a model pick: it outranks the key in `~/.bravebot/settings.json`,
and is outranked by one in a checkout's `.bravebot/settings.json` or `.bravebot/settings.local.json`,
or in the file `--settings` named. Above all of it are `--effort`, for the one run it starts, and a
level picked with `/effort` in the session that is running. Whatever answers is what every surface
asks for: the interface, a session in lines, and a one-shot run.

The word is read on the terms [SESSION-15](sessions.md#SESSION-15) reads the recorded one on, by the
same rule and in one place. A word this program does not define is no level at all rather than a
level of something, so it never reaches a request field. Named in a layer that outranks the record,
it still outranks the record, and the run asks for no level: the layer said something, and what it
said was not a level. A blank, or a value that is not a string, is absence on the footing the
`model` key's is: it does not outrank a pick, and it still displaces the key a lower file named, so
with nothing recorded the run asks for no level. `--effort` takes only a word this program defines,
and refuses any other before a run starts. Whether the level then goes out at all is still
[BACKEND-22](#BACKEND-22)'s question, and a level the model in force reads none of is withheld and
not forgotten, whichever of them named it. Nothing is recorded: a level a file named is not a pick,
and writing one down would make reading a file once enough to outlive the file.

Nothing else names a level: no release bakes one in, no variable is read for one, and the
machine-level layer of [BACKEND-38](#BACKEND-38) does not pin it.

**Why.** A pick is stored once per person and read back by every run. Ranked above every file, it
would be the one thing a checkout cannot override: two checkouts in one account cannot want
different levels, and "the work in this repository is worth thinking hard about" is undone by
whatever the reader last picked anywhere else. Ranked below every file, a level somebody picked in
the interface would lose to one they wrote into their own settings once and forgot. The person's own
file and the pick are the same person at the same rung, so the later of the two answers, and a
checkout outranks both because it is the one thing that can tell one checkout from another. A script
that wants its own level for one run says so with `--effort`, the way [CLI-9](cli.md#CLI-9) lets it
say `--model`.

[BACKEND-11](#BACKEND-11)'s other half does not transfer: the `model` key sits above the baked-in
default because every release bakes a model in, and a key ranked below it would change nothing on
any binary anybody was given. Nothing bakes in a level, so there is no such rung here.

**One rule reads both words.** Both come out of a file somebody may have edited by hand, so a
settings file naming nonsense is read the way a hand-edited record naming nonsense is, rather than
each being trusted where it came from. Two spellings of that rule is where the two would drift, and
what drifting costs is a word no service defines in a request field.

`verified-by: bravebot_config::settings::a_top_level_effort_key_is_read`
`verified-by: bravebot_config::settings::an_effort_word_is_read_as_the_file_spelled_it`
`verified-by: bravebot_config::settings::an_effort_that_is_blank_or_not_a_string_names_nothing`
`verified-by: bravebot_config::settings::the_closest_layer_that_named_an_effort_wins`
`verified-by: bravebot_config::settings::a_layer_above_the_home_one_does_not_pick_a_model`
`verified-by: bravebot_config::settings::the_home_layer_does_not_outrank_a_saved_pick`
`verified-by: bravebot_config::settings::a_layer_above_that_names_nothing_does_not_outrank_a_saved_pick`
`verified-by: bravebot_cli::running::doctor_says_a_checkouts_provider_block_is_not_obeyed`
`verified-by: bravebot_session::store::a_checkouts_level_outranks_the_saved_pick_and_the_home_file_does_not`
`verified-by: bravebot_session::store::a_settings_file_naming_no_level_asks_for_none`
`verified-by: bravebot_tui::persist::a_recorded_level_answers_between_a_checkouts_file_and_the_persons_own`
`verified-by: bravebot_cli::running::a_run_sends_the_level_a_settings_file_named_where_nothing_is_recorded`
`verified-by: bravebot_cli::running::a_run_sends_a_checkouts_level_over_the_recorded_one`
`verified-by: bravebot_cli::running::a_run_sends_the_level_the_command_line_named_over_every_other`
`verified-by: bravebot_cli::main::an_effort_flag_names_the_level_a_run_asks_for`
`verified-by: bravebot_cli::main::an_effort_flag_naming_no_level_is_refused`
`verified-by: bravebot_config::managed::a_name_outside_the_pinnable_set_pins_nothing`

<a id="BACKEND-44"></a>
### BACKEND-44: a streamed Bedrock request offering a tool asks for its arguments as they are written

A streamed Bedrock request that offers the model a tool asks for each tool argument as the model
writes it, in the field that service hands to the model without reading, as the beta the Anthropic
API names for it. A whole-reply request and a request offering no tool do not ask. A model that
refuses a request carrying the ask is asked again without it, and no later request carries it to
that model, on the terms [BACKEND-22](#BACKEND-22) gives the level: what one model refused says
nothing about another, a request refused without the ask as well settles nothing and is not
remembered, and the order the concessions are given up in is that clause's to state. So a request
refused on its contents for a reason of its own, a prompt too long for the model among them, is sent
once more without the ask before that refusal is reported, and so is the next one.

A streamed request to an `anthropic` entry asks the same of the service in the field that API
documents for it, `eager_input_streaming` on each tool it offers, and not in the beta Bedrock is asked
with. It is given up last of the concessions that request carries, after the cache, the level and the
ceiling, and is remembered on the same terms. A whole-reply request does not carry it.

An argument that arrives this way is handed to the turn loop exactly as the model wrote it. One that
does not parse is a failed call the planner is told about, and it never runs: nothing repairs it or
closes it on the way to being dispatched. The conversation sent on the next round carries that call
with an empty object for its arguments, this service taking nothing but an object there, beside the
result saying it failed.

**Why.** Without the ask the service holds an argument back until the model has finished writing it,
so a `write_file` whose contents are a whole program is the tool's name and then silence for as long
as the program takes to write. The egress layer cuts a connection that has sent nothing for two
minutes, which a long enough file outlasts, and reports it as a request that did not get through.
Measured on Bedrock against Claude Opus 5.5, one call writing a file of about 1,900 tokens sent
nothing for 17 seconds and then all of it; with the ask, no gap in the same call was longer than 2.4
seconds. The per-tool field the Anthropic API documents for the same thing is not what is sent,
because this API drops a key it does not define from a tool's description: sent, it left the silence
as long, and a made-up key in the same place is answered as readily.

Streamed only, because a whole reply arrives at once however its arguments were written, so there
the ask buys nothing and gives up the service's check that an argument parses. That check is the
ask's cost where it is sent, which is why a call that does not parse fails where it is dispatched: a
repaired argument is a call the model never finished, and a file written from one is reported
written.

`verified-by: bravebot_bedrock::protocol::the_level_and_the_ask_for_arguments_as_written_travel_together`
`verified-by: bravebot_bedrock::lib::only_a_streamed_request_offering_a_tool_asks_for_arguments_as_they_are_written`
`verified-by: bravebot_bedrock::lib::a_refused_ask_for_arguments_as_written_is_given_up_and_remembered`
`verified-by: bravebot_bedrock::lib::the_level_is_given_up_before_the_ask_for_arguments_as_written`
`verified-by: bravebot_bedrock::lib::a_streamed_argument_that_does_not_parse_is_handed_on_as_written`
`verified-by: bravebot_bedrock::lib::a_probe_that_settled_nothing_leaves_what_was_known`
`verified-by: bravebot_bedrock::protocol::unparseable_arguments_become_an_empty_object_rather_than_dropping_the_call`
`verified-by: bravebot_agent::tools::a_call_whose_arguments_do_not_parse_fails_and_writes_nothing`
`verified-by: bravebot_aichat::anthropic::protocol::asking_for_arguments_as_written_marks_every_tool_and_nothing_else`
`verified-by: bravebot_aichat::client::a_whole_anthropic_request_asks_for_neither_a_stream_nor_arguments_as_written`
`verified-by: bravebot_aichat::client::an_anthropic_stream_refused_on_its_contents_gives_up_the_ask_for_arguments_last`

<a id="BACKEND-45"></a>
### BACKEND-45: a streamed reply reports the call being written, and its count moves with the argument

While a streamed reply arrives, the output count it reports goes up by one for each piece of a tool
argument, as it does for each piece of the reply's words, from Brave's endpoint, an OpenAI-compatible gateway
and Bedrock alike.
The service's own figure replaces that tally once it arrives, and a piece arriving after it adds
nothing. The name of the call the model is writing is reported with the same progress from the
moment the call opens, before any of its argument has arrived, labelled as the reply's words are.

The tally of argument pieces is never charged. A reply is charged the service's figure, and one that
ends without a figure is charged the tally of its words alone, however much argument it carried.
The turn counts each round on from what its count last showed where that is more than the round was
charged, so the count a person watches does not fall between rounds.

**Why.** With [BACKEND-44](#BACKEND-44) an argument arrives in pieces, but a count that moved only
for words stood still for as long as a file took to write, which on a screen reads as a stall. The
tally is for that screen. Counting it in the charge as well would raise what every reply without a
figure costs a person, which is a question about the estimate and not about the screen.

`verified-by: bravebot_bedrock::lib::an_argument_is_tallied_as_it_arrives_and_its_call_named_from_the_start`
`verified-by: bravebot_bedrock::lib::progress_names_the_call_being_written_and_counts_its_argument`
`verified-by: bravebot_aichat::protocol::a_call_being_written_is_named_and_counted`
`verified-by: bravebot_agent::turn::the_call_being_written_reaches_the_interface_before_it_runs`

<a id="BACKEND-46"></a>
### BACKEND-46: a streamed Bedrock request asks for the model's thinking as it happens

A streamed Bedrock request asks for the model's thinking to be sent as it thinks, in the field that
service hands to the model without reading, saying how thinking is shown and never whether the model
thinks, which stays the model's own default. A whole-reply request does not ask. The thinking that
arrives is handed to the caller as it arrives, for a screen to draw on request
([VIEW-33](terminal-transcript.md#VIEW-33)) and for nothing else: none of it becomes the reply's
words or its record in the conversation, and a reply that carried it is as readable as one that did
not. A model that refuses a request carrying
the ask is asked again without it, and no later request carries it to that model, on the terms
[BACKEND-22](#BACKEND-22) gives the level, that clause stating the order the concessions go in.
A later session is told what this one found, as [BACKEND-63](#BACKEND-63) says.

**Why.** A model that thinks before it writes sends nothing while it does unless asked, and the
egress layer cuts a connection that has sent nothing for two minutes. Measured on Bedrock, Claude
Opus 5.5 at the level `max` thought for 32,000 tokens and sent nothing for 379 seconds. With the ask
the same model sent its thinking throughout: the longest gap was 6.0 seconds in 88 at the level
`high`, and 7.6 seconds in 24 with no level. The ask does not make it think harder. With no level it
wrote 2,016 tokens in 24.1 seconds with the ask and 1,949 in 25.0 without, and without it opened the
reply only after 11.5 seconds of silence, with an empty block where the thinking would have been. A
round that sends back a tool call without the thinking before it is answered, so the thinking read
past costs the next round nothing. Streamed only, because a whole reply arrives at once however it
was thought.

`verified-by: bravebot_bedrock::protocol::the_level_and_the_ask_for_arguments_as_written_travel_together`
`verified-by: bravebot_bedrock::lib::only_a_streamed_request_asks_to_show_thinking`
`verified-by: bravebot_bedrock::lib::thinking_that_arrives_stays_out_of_the_reply`
`verified-by: bravebot_bedrock::lib::thinking_reaches_progress_and_never_the_reply`
`verified-by: bravebot_bedrock::lib::the_ask_to_show_thinking_is_given_up_before_the_level`
`verified-by: bravebot_bedrock::lib::a_refused_ask_for_arguments_as_written_is_given_up_and_remembered`
`verified-by: bravebot_bedrock::lib::a_probe_that_settled_nothing_leaves_what_was_known`

<a id="BACKEND-47"></a>
### BACKEND-47: a pick is recorded as the tier it was, and a pick nothing serves is set aside

A model picked with `/model` that an AWS account named for a tier is recorded as that tier's word,
`opus`, `sonnet` or `haiku`. Any other pick is recorded as it is. A recorded name is read back as
[BACKEND-12](#BACKEND-12) reads a tier word, so a recorded tier names whatever model that tier names
at the next start.

Where the recorded pick is in force under [BACKEND-11](#BACKEND-11) and [BACKEND-39](#BACKEND-39)
would refuse it, while the configured model beneath it would be served, the pick is set aside for
that start. The interface, a session in lines and a one-shot run given no `--model` open on the
configured model and say which pick they set aside. `doctor` reports the configured model with the
pick beside it. The record is left as it is. Where the configured model would be refused as well,
nothing is set aside and the refusal is BACKEND-39's.

**Why.** An inference-profile ARN changes when the profile is recreated, and the tier variable is
what a person edits when that happens, which leaves a recorded ARN naming the old profile. That
record outranks the `model` key in the person's own file, reaches Brave's endpoint, and is refused
with advice to set the key it already outranks. Recording the tier word removes that case for picks
made from now on. A record written before this, or one naming a model of a gateway block that was
removed, still names nothing a configured service offers, and refusing it leaves somebody stuck on a
file no message names. The record is kept because the configuration that served it can return, such
as an `.envrc` that exports the tier variables in one checkout only.

`verified-by: bravebot_config::lib::a_tier_model_is_recorded_as_its_word_and_read_back_through_the_variable`
`verified-by: bravebot_agent::backend::a_recorded_tier_word_names_the_model_its_variable_names`
`verified-by: bravebot_agent::backend::a_pick_is_set_aside_only_for_a_default_that_is_served`
`verified-by: bravebot_tui::persist::a_picked_tier_is_recorded_as_its_word_and_follows_the_variable`
`verified-by: bravebot_tui::persist::a_pick_nothing_serves_is_set_aside_and_named`
`verified-by: bravebot_cli::running::a_recorded_pick_nothing_serves_is_set_aside_for_the_configured_model`
`verified-by: bravebot_cli::running::doctor_names_a_pick_it_sets_aside`
`verified-by: bravebot_cli::running::a_session_in_lines_sets_aside_a_pick_nothing_serves`

<a id="BACKEND-48"></a>
### BACKEND-48: the managed layer may say which models a machine requests and never add one

An administrator's layer may list the models a machine may request and the models it may not, by the
name a request would carry. It may not add a model to any roster, make one reachable, or name which
model is the default.

A model it refuses is not requested by any route: the `model` key, an exported
`BRAVEBOT_DEFAULT_MODEL`, the baked-in default, a recorded `/model` pick, `--model`, and a
delegate definition alike. The name is matched after a tier word is resolved
([BACKEND-12](#BACKEND-12)), so `opus` is checked as the model that tier names. `/model` does not
offer a refused model, and neither does the desktop window's roster, the configured model included; a
turn that window is asked to start on one is refused with the file that refused it. A refused delegate
definition is refused where it is read, naming the model and the file, and nothing is started for it.
`doctor` names `models.allow` and `models.deny` among the names the layer pins.

A recorded pick the layer refuses is set aside for the configured model, as a pick nothing serves is
under [BACKEND-47](#BACKEND-47), and the record is left as it is. The interface, a session in lines
and a one-shot run open on the configured model and say which pick they set aside and why. Where the
configured model is refused as well there is nothing to fall to: nothing is set aside, and the start
is refused with the file that refused and which of its lists did. A run whose model is refused for any
other reason is refused the same way.

**Why.** The names this layer may pin decide where a request goes, and after it has pinned the
endpoint, the AWS switch, the region, the profile and the three tiers, every other model name still
reaches the pinned account and is billed to it. So the layer answers where the traffic goes and not
what it spends, which is half of the case `managed.rs` exists for: an organisation that will not have
models reached through somebody's personal cloud account has nothing to say which models its own
account may serve.

A list of models can only remove capability, which is the test this layer's keys are already admitted
by ([SERVERS-12](mcp-servers.md#SERVERS-12)): there is nowhere in the layer to hold a roster, so an
entry permits a name some service already offers rather than making one reachable, and neither list is
a preference.

Not offering a refused model rather than refusing a chosen one, because a list holding a name that
cannot be asked for misdescribes the machine, and being refused after reading a row and choosing it is
a worse answer than the row not being there.

A refused pick falls to the configured model rather than stopping the session, for the reason
BACKEND-47 sets a pick aside: an administrator's list changes, the configuration that served the pick
can return, and a session that refused to open would leave somebody with no way to work while the
model under their pick is one the layer allows.

**How it is built.** The keys are `"models": { "allow": [...], "deny": [...] }` in `managed.json`,
the file [BACKEND-38](#BACKEND-38) reads. An entry is the model's name as a request carries it,
compared exactly once trimmed: an inference-profile ARN, or a roster slug such as `z-ai/glm-4.6`.
A gateway model has two spellings, `openrouter/z-ai/glm-4.6` and the `z-ai/glm-4.6` the gateway is
sent, and an entry naming either one answers for the model however it was asked for: a deny entry
refuses it and an allow entry permits it.

Without an `allow` list, every model not denied is requested as before. With one, a model no entry
names is refused, and `"allow": []` refuses every model. A deny entry wins over an allow entry naming
the same model. Only a list counts: a string, an object, `null` or anything else under either key
decides nothing, and an empty `deny` denies nothing, which `doctor` does not report either. An entry
that is not a name is skipped. In a deny list it denies nothing; in an allow list it allows nothing,
which is one fewer model requested rather than a list that decides nothing.

`verified-by: bravebot_config::managed::a_model_allow_list_requests_only_what_it_names`
`verified-by: bravebot_config::managed::an_empty_model_allow_list_requests_nothing`
`verified-by: bravebot_config::managed::a_denied_model_is_refused_even_where_the_allow_list_names_it`
`verified-by: bravebot_config::managed::without_a_model_allow_list_only_what_is_denied_is_refused`
`verified-by: bravebot_config::managed::a_models_block_that_is_not_a_pair_of_lists_decides_nothing`
`verified-by: bravebot_config::managed::a_model_allow_entry_adds_nothing_to_any_roster`
`verified-by: bravebot_config::lib::a_tier_word_is_checked_as_the_model_it_names`
`verified-by: bravebot_config::lib::a_managed_layer_silent_on_models_refuses_none`
`verified-by: bravebot_config::lib::a_gateway_model_is_matched_by_either_of_its_spellings`
`verified-by: bravebot_agent::backend::a_model_the_managed_layer_refuses_is_served_by_nothing`
`verified-by: bravebot_agent::backend::a_pick_the_managed_layer_refuses_is_set_aside_with_the_file_that_refused_it`
`verified-by: bravebot_cli::import::a_start_on_a_model_the_managed_layer_refuses_is_ended`
`verified-by: bravebot_tui::app::the_picker_does_not_offer_a_model_this_machine_may_not_request`
`verified-by: bravebot_tui::app::a_definition_naming_a_model_this_machine_may_not_request_is_refused`
`verified-by: bravebot_tui::persist::a_pick_the_managed_layer_refuses_is_set_aside_and_names_the_file`
`verified-by: bravebot_ui_bridge::models::the_window_neither_offers_nor_requests_a_model_this_machine_refuses`
`verified-by: bravebot_agent::turn::a_delegate_whose_model_the_managed_layer_denies_sends_nothing_and_names_the_file`
`verified-by: bravebot_agent::turn::an_addressed_definition_whose_model_the_managed_layer_denies_sends_nothing_and_names_the_file`
`verified-by: bravebot_agent::turn::a_skill_whose_model_the_managed_layer_denies_keeps_the_sessions_model_and_names_the_file`

<a id="BACKEND-49"></a>
### BACKEND-49: a `google-vertex` entry is reached at a host built from its project and location

A `provider` entry keyed `google-vertex` that states `options.project` reaches Google Vertex AI
through its OpenAI-compatible endpoint, and may state `options.location`, which is `global` where it
does not. An entry that states no project is reached in express mode ([BACKEND-58](#BACKEND-58)). The endpoint is built from the two: `https://aiplatform.googleapis.com/v1/projects/<project>/locations/global/endpoints/openapi`
for `global`, and `https://<location>-aiplatform.googleapis.com/v1/projects/<project>/locations/<location>/endpoints/openapi`
for any other. An `options.baseURL` the entry states is where its requests go regardless, as
[BACKEND-17](#BACKEND-17) says of every known name.

An entry whose project or location holds a character outside the ones Google allows in it
configures no service, and is not read as express mode for having a project: a project is letters, digits, `_`, `.`, `:` and `-`
and starts with a letter or a digit, and a location is lowercase letters, digits and `-`.

The entry is not asked for a roster, since the endpoint has no model listing a key can call. In
place of [BACKEND-19](#BACKEND-19)'s request, an entry that lists no models is offered a list
compiled in: `google/gemini-3.1-flash-lite`, `google/gemini-2.5-pro`, `google/gemini-2.5-flash` and
`google/gemini-2.5-flash-lite`. An id goes on it only once the `global` endpoint has answered it
with a tool call and then answered the call's result, and no preview model goes on it. An entry that
lists models is offered those and no others: its list replaces the compiled one rather than adding
to it. The compiled list is offered whether or not a key is found, as a block's own models are, since
offering it sends nothing, and a turn sent without a key is refused as any gateway's is. The compiled
list is names to choose from and not a roster a bare name is routed by. A pick off it is recorded
qualified, `bravebot doctor` names each of its ids qualified, and a model on no list is named
qualified, as `google-vertex/google/gemini-3-flash-preview`, which [BACKEND-18](#BACKEND-18) routes
to it.

**Why.** This endpoint requires a project, and a request without one is refused, so a guessed project
is a request that fails somewhere far from the mistake, which is the argument [BACKEND-29](#BACKEND-29)
makes about a region. An entry naming no project does not guess one: it is the entry that
[BACKEND-58](#BACKEND-58) reaches by another route. The host carries the location, so it is built rather than stated, and that
is also why it is checked: a project or location is written into a URL, and one holding `/`, `@`, `?`
or `#` would send the key to another host or another route than the one the person named. Refusing
the entry is the only answer that cannot send it anywhere. The host is compiled in, with a stated
endpoint as the one way to change it, so it is a destination somebody reviewed and no host is
derived from anything a model or a fetched page said ([routing.md](routing.md)).

Not asked for a roster because the request would spend a round trip and carry the key to be told
404. The compiled list stands in for the answer because the environment route
([BACKEND-50](#BACKEND-50)) has no block to name models in, and a block written for a picker would
otherwise name every model and be kept current by hand. It is content, on the footing BACKEND-19
gives a fetched roster. A preview is left out because Google withdraws one without notice, and its
row would fail when picked until a release took it out. The list stays out of the entry's own models
so that a bare name routes as it did, and a block's list replaces it so that what a file pins is
exactly what is offered, as for any other gateway.

`verified-by: bravebot_config::provider::a_google_vertex_entry_is_reached_at_the_host_its_project_and_location_build`
`verified-by: bravebot_config::provider::a_google_vertex_entry_in_a_region_is_reached_at_that_regions_host`
`verified-by: bravebot_config::provider::a_google_vertex_entry_without_a_project_is_reached_in_express_mode`
`verified-by: bravebot_config::provider::a_project_or_location_that_would_move_the_request_configures_nothing`
`verified-by: bravebot_config::provider::the_compiled_vertex_list_is_the_four_answered_ids_and_no_preview`
`verified-by: bravebot_config::provider::a_project_and_location_holding_only_allowed_characters_are_accepted`
`verified-by: bravebot_config::provider::a_stated_endpoint_beats_the_google_vertex_host`
`verified-by: bravebot_aichat::client::a_google_vertex_entry_offers_the_compiled_models_without_asking`
`verified-by: bravebot_tui::app::the_picker_offers_the_compiled_models_for_a_google_vertex_service_naming_none`
`verified-by: bravebot_tui::app::a_google_vertex_block_naming_models_is_offered_those_alone`
`verified-by: bravebot_ui_bridge::models::the_window_offers_the_compiled_models_for_a_google_vertex_service_naming_none`
`verified-by: bravebot_ui_bridge::models::the_window_offers_a_google_vertex_block_its_own_models_alone`
`verified-by: bravebot_cli::doctor::doctor_names_the_compiled_models_a_google_vertex_service_is_offered`
`verified-by: bravebot_config::lib::a_name_qualified_by_the_google_vertex_id_names_that_service_and_the_rest_is_sent`

<a id="BACKEND-50"></a>
### BACKEND-50: the environment names a Google Vertex service when no block does

`GOOGLE_API_KEY` configures the `google-vertex` service, whether it is exported or set in the `env`
block of the person's own settings file ([BACKEND-33](#BACKEND-33) withholds the three names from a checkout's), and has to hold a value: a missing or blank key configures nothing, unless
`GOOGLE_CLOUD_PROJECT` is set, which configures the service of [BACKEND-49](#BACKEND-49) with no key
([BACKEND-65](#BACKEND-65)). With
`GOOGLE_CLOUD_PROJECT` beside the key, and `VERTEX_LOCATION` where the location is not `global`, the
service is the one of [BACKEND-49](#BACKEND-49), and a project or location that BACKEND-49 refuses
configures nothing. With no project, or a blank one, it is the express service of
[BACKEND-58](#BACKEND-58), and `VERTEX_LOCATION` is not read. The service it configures lists no
models, so it is offered the list BACKEND-49 compiles in.

The key is read from `GOOGLE_API_KEY` when a request needs it, as [BACKEND-16](#BACKEND-16) says of
any variable a gateway names, so a key exported after the session opened is the one sent. A key set
only in the person's `env` block is held by the configuration, and an exported one outranks it, as
[BACKEND-35](#BACKEND-35) ranks every name.

A `provider.google-vertex` entry that configures a service replaces this route, and so does a managed gateway list
([BACKEND-38](#BACKEND-38)), which is the only list there is: an organisation that pinned its
gateways is not given another by a variable. The block names its own project and location and reads
neither variable for them. Configuring the service changes no model that is in force: nothing is
sent to Google until a model named `google-vertex/...` is selected ([BACKEND-13](#BACKEND-13)).

**Why.** `GOOGLE_API_KEY` is a name other Google clients read, and a key exported for one of them
may be scoped to a service other than this one. What limits that is where the key goes and when: only
to the compiled-in Google host and only once the person has chosen a model that names this service.
A project is not required, because express mode needs none, so the key alone names the service; a
key scoped to another service is refused there as a failed request and not a leak, the host being
Google's own. The alternative, a name of this program's own, keeps the key away from every ambient
reader and gives up the convenience of the name the other tool reads, which is why the ambient one is
not required here: the block names any variable the person prefers. A key that names the service
also makes the name a gateway token, taken off the environment of every program this agent starts
([RUN-12](tools/run.md#RUN-12)), so a `gemini` started from `run` does not inherit it.

An entry replaces the route rather than merging with it, so that what a file says is not quietly
completed by a variable the file does not mention. An entry that configures nothing, for want of a
project, is dropped as any such entry is, and leaves the route open. A managed list is exempt for the reason
[BACKEND-38](#BACKEND-38) gives: a destination list a variable could add to is one that pins nothing.

`verified-by: bravebot_config::lib::the_environment_names_a_google_vertex_service_when_no_block_does`
`verified-by: bravebot_config::lib::a_google_vertex_service_from_the_environment_is_offered_the_compiled_models`
`verified-by: bravebot_config::lib::the_environment_route_needs_a_key_and_a_project_the_host_can_be_built_from`
`verified-by: bravebot_config::lib::a_key_alone_names_the_express_service_and_a_blank_project_is_no_project`
`verified-by: bravebot_config::lib::a_key_set_only_in_the_env_block_reaches_the_google_vertex_service`
`verified-by: bravebot_config::lib::an_exported_key_outranks_the_one_in_the_env_block`
`verified-by: bravebot_config::lib::a_google_vertex_block_replaces_the_environment_route`
`verified-by: bravebot_config::lib::a_managed_gateway_list_takes_no_google_vertex_from_the_environment`
`verified-by: bravebot_config::lib::a_google_vertex_service_from_the_environment_leaves_the_model_in_force_alone`
`verified-by: bravebot_config::lib::a_google_vertex_service_holds_a_credential_the_record_names_by_host`

<a id="BACKEND-51"></a>
### BACKEND-51: a Vertex key is sent in `x-goog-api-key` and nowhere else

Every request to a `google-vertex` service carries its key in the `x-goog-api-key` header and no
`authorization` header. The key is in no trace, no error text a refused request produces, and no
session record.

**Why.** The service refuses the key as a bearer token (401, "Expected OAuth 2 access token"), so
the header every other gateway takes is the one that cannot work here. Sending the key in both would
put a credential in a header the service does not read for it, which is a copy for anything on the
path to log. The absence from errors and records is [CRED-23](credential-protection.md#CRED-23)'s
concern applied to a new header: the value travels in a `Secret`, and a service that echoes a
request back in its error body is quoted without the header.

`verified-by: bravebot_aichat::lib::a_google_vertex_request_carries_its_key_in_x_goog_api_key_and_no_authorization_header`
`verified-by: bravebot_aichat::lib::another_gateway_still_sends_its_token_as_a_bearer`
`verified-by: bravebot_aichat::client::a_refused_google_vertex_request_does_not_repeat_the_key`
`verified-by: bravebot_aichat::client::a_vertex_key_is_in_no_error_and_no_recorded_event_on_either_path_for_any_refusal`
`verified-by: bravebot_aichat::client::a_refused_google_vertex_request_does_not_repeat_the_key`
`verified-by: bravebot_config::provider::a_google_vertex_key_is_sent_in_its_own_header`

<a id="BACKEND-52"></a>
### BACKEND-52: what a service attached to a tool call is sent back with it, to that service only

A tool call the service answered with an `extra_content` member is kept with that member, from the
streamed reply through the stored conversation, and sent back unchanged with the call on the next
request. A call with none carries none, and a session record written before the member existed reads
back unchanged. The member is sent only to a `google-vertex` service: any other service's request
is built without it. The Responses protocol's item for a call has no such member, so the ChatGPT account, and
any other service spoken to in that protocol, is sent the call alone ([BACKEND-61](#BACKEND-61)).

**Why.** Vertex attaches a thought signature to each call and refuses the next request, with 400,
on models that require it unless the call comes back with it. A call rebuilt from only its name and
arguments loses it. The member is opaque: nothing here reads inside it, so a change in what Google
puts there costs nothing. Sent to Google only because another service either ignores it or, on a
strict one, refuses the request, and a conversation can change service between two turns.

`verified-by: bravebot_aichat::protocol::a_tool_calls_extra_content_is_kept_from_the_stream_and_sent_back`
`verified-by: bravebot_aichat::protocol::a_tool_call_without_extra_content_is_built_as_before`
`verified-by: bravebot_aichat::protocol::a_record_written_before_extra_content_reads_back_unchanged`
`verified-by: bravebot_aichat::protocol::a_stored_call_carrying_extra_content_reads_back_with_it`
`verified-by: bravebot_aichat::protocol::extra_content_is_taken_from_every_call_in_a_body_and_the_call_stays`
`verified-by: bravebot_aichat::lib::extra_content_reaches_only_a_google_vertex_service`
`verified-by: bravebot_aichat::lib::extra_content_is_not_sent_to_braves_own_endpoint`
`verified-by: bravebot_aichat::client::a_member_another_service_attached_to_a_call_is_not_sent_in_the_responses_protocol`

<a id="BACKEND-53"></a>
### BACKEND-53: a turn whose model keeps failing moves to the fallback model a person named

The `fallbackModel` setting holds one model name. It is read from the person's own settings file and
from the file `--settings` names, and from no project or local layer ([BACKEND-24](#BACKEND-24)):
a layer that names it is dropped and `doctor` names the file. A tier word in it resolves as it does
for `model`. A model the managed layer refuses ([BACKEND-48](#BACKEND-48)) is not a fallback.

A request in the turn's own rounds that fails with the category `rate-limited` or `unavailable`
([BACKEND-37](#BACKEND-37)) is sent once more with the fallback model, and the rest of that turn
runs on it. The client has already made its attempts at the first model, so the fallback is the
request after them. The decision reads the category, which is derived from the status the service
answered with, and nothing the reply said. Every other category ends the turn as before.

The fallback is used only where the same service answers for both models: the same AWS account, the
same gateway, or the aichat endpoint. A fallback another service would answer is not used and the
failure ends the turn, because naming a model is the person's agreement to send the conversation
to the service that serves it ([BACKEND-3](#BACKEND-3)), and nothing here agreed to the first
model's service handing it on. A refusal of the ChatGPT account that names it
([CHATGPT-8](chatgpt-sign-in.md#CHATGPT-8)) is a refusal among the categories and so never moves the turn: the
account answers for every model it serves. A turn moves to the fallback at most once; a failure on the fallback
model ends the turn. A delegate, and a turn running on the model a definition named
([DELEGATE-22](delegation.md#DELEGATE-22)), do not fall back: that model is a boundary its file
drew.

The person is told which model failed and which one the turn moved to, and the trail records both
names with the category. Where the failure was one of the ChatGPT account's own, a renewal that was
rate-limited or met an unavailable service, the line says it in that failure's sentence
([CHATGPT-8](chatgpt-sign-in.md#CHATGPT-8)) beside the model names, and the front end is handed the
failure as a closed value so that the window can word it itself. The category of such a failure is
the one its closed value maps to, and the agent reads no status for it. The model that answered is
not then reported as a substitution for the session's own: it is the model the person named for this
case.

**Why.** A person running a long turn against a busy model has to notice the failure and switch by
hand. The name is configuration, so naming it is the endorsement for the request field it lands in,
as the `model` key is, and the switch is decided by a category the driver derived itself.

`verified-by: bravebot_agent::turn::a_turn_whose_model_keeps_failing_moves_to_the_fallback_model`
`verified-by: bravebot_agent::turn::a_model_the_service_keeps_failing_is_followed_by_a_request_naming_the_fallback`
`verified-by: bravebot_agent::turn::a_fallback_that_is_the_model_that_failed_is_not_asked_again`
`verified-by: bravebot_agent::turn::a_failure_that_is_not_an_overload_does_not_move_the_turn_to_the_fallback`
`verified-by: bravebot_agent::turn::a_fallback_another_service_would_answer_is_not_used`
`verified-by: bravebot_agent::turn::round_steps::a_move_to_the_fallback_after_the_accounts_failure_says_the_accounts_sentence`
`verified-by: bravebot_ui_bridge::notice_tests::a_move_to_the_fallback_after_the_account_failed_reaches_the_window_as_its_own_sentence`
`verified-by: bravebot_ui_bridge::history::a_move_to_the_fallback_model_reaches_the_window_as_a_narration`
`verified-by: bravebot_agent::turn::a_turn_moves_to_the_fallback_once`
`verified-by: bravebot_agent::turn::a_delegate_does_not_fall_back`
`verified-by: bravebot_config::settings::a_project_layer_cannot_name_the_fallback_model`
`verified-by: bravebot_config::lib::a_fallback_the_managed_layer_refuses_is_not_used`
`verified-by: bravebot_agent::backend::two_models_share_a_service_only_where_one_account_answers_for_both`
`verified-by: bravebot_agent::backend::a_refusal_that_names_the_account_never_falls_back_to_another_model_of_it`
`verified-by: bravebot_cli::running::doctor_says_a_checkouts_fallback_model_is_not_obeyed`

<a id="BACKEND-54"></a>
### BACKEND-54: a rate limit is waited out for as long as the service asked, up to a ceiling

A reply that is refused with a status that means not now ([NET-6](network-egress.md#NET-6)) is
sent again, at most twice unless the entry's block states another count
([BACKEND-55](#BACKEND-55)). Where the answer was a 429 and carried a `Retry-After`
header the wait before the next request is the one it asked for instead of the doubling from one
second; any other status keeps the doubling whatever the header said. The
header is read by the egress layer, as whole seconds or an HTTP date in the IMF-fixdate form, and
is carried on the failure as a duration of at most one hour. A value that is neither, and a header
that is absent, leave the doubling in force.

The wait is never shorter than that doubling, so `Retry-After: 0` sends the next request after the
pause it would have had. A wait of more than two minutes is not taken: the request is not sent
again and the failure ends the call at once. For the category `rate-limited` the failure line then
says the service asked for a wait of about that many minutes, rounded up. The figure is a number this program parsed
and bounded and the sentence around it is the fixed one of [BACKEND-37](#BACKEND-37), so no text the
service wrote is repeated. The wait only sets a timer and a line a person reads: it is never given
to the planner, and nothing branches on the header's text. The wait is announced as the retry it
is, and a stop ends it as it ends any pause between attempts.

**Why.** A limit that lifts in a minute otherwise ends the turn after three seconds, and a limit
that lifts in an hour gives the person nothing to act on. The status decides whether to wait, the
parsed number only how long.

`verified-by: bravebot_aichat::client::a_rate_limit_is_waited_out_for_as_long_as_the_service_asked`
`verified-by: bravebot_aichat::call::a_rate_limit_is_waited_out_and_announced_before_the_wait`
`verified-by: bravebot_aichat::client::a_rate_limit_is_waited_out_before_a_whole_reply_is_asked_for_again`
`verified-by: bravebot_aichat::client::a_wait_past_the_ceiling_fails_at_once_and_carries_the_figure`
`verified-by: bravebot_aichat::lib::a_transient_failure_is_retried_unless_a_429_asked_for_more_than_the_ceiling`
`verified-by: bravebot_aichat::client::a_retry_after_that_does_not_parse_falls_back_to_the_backoff`
`verified-by: bravebot_aichat::client::a_retry_after_of_zero_still_takes_the_backoff`
`verified-by: bravebot_aichat::client::a_retry_after_on_another_status_is_not_waited_for_or_refused`
`verified-by: bravebot_aichat::client::a_stop_does_not_wait_out_the_pause_a_service_asked_for`
`verified-by: bravebot_bedrock::lib::a_throttled_request_is_waited_out_for_as_long_as_the_service_asked`
`verified-by: bravebot_bedrock::lib::a_throttle_asking_for_longer_than_the_ceiling_fails_at_once_with_the_figure`
`verified-by: bravebot_aichat::client::a_google_429_is_waited_out_for_as_long_as_asked_unless_the_wait_is_past_the_ceiling`
`verified-by: bravebot_aichat::client::a_responses_429_is_waited_out_for_as_long_as_asked_unless_the_wait_is_past_the_ceiling`
`verified-by: bravebot_agent::backend::a_rate_limit_keeps_the_wait_the_service_asked_for`
`verified-by: bravebot_session::sessions::a_rate_limit_says_how_long_the_service_asked_for`

<a id="BACKEND-55"></a>
### BACKEND-55: a provider entry states how often a request is sent again and how long a reply may go quiet

An entry under `provider`, for a gateway and for an AWS account alike, may state two figures in its
`options`. `maxRetries` is how many times a request to it is sent again after a failure worth
another attempt ([NET-6](network-egress.md#NET-6)). `idleTimeoutMs` is the longest gap, in
milliseconds, between two pieces of a reply that is still arriving
([NET-5](network-egress.md#NET-5)).

Where `maxRetries` is not stated a request is sent again at most twice, and where `idleTimeoutMs` is
not stated the gap is two minutes. A count of zero sends every request once. A count above seven is
seven, and a gap above ten minutes is ten minutes. A value that is not a whole number, a negative
one, and a gap of zero state nothing, and the entry is otherwise read as written.

The figures set a count and a timer and decide nothing else. A request is sent again for the same
failures as without them, taken from the status or the kind of failure and never from anything a
reply said. The pause before each is the doubling from one second, a wait a service asked for is
bound as [BACKEND-54](#BACKEND-54) says, and the count of requests a failure reports is the one
[BACKEND-37](#BACKEND-37) states, so a stated count shows in it. A stated gap replaces the default
in either direction, and does not move the bound on the reply as a whole.

The block is read from the person's own settings, from the file `--settings` names and from the
machine-level file, and from no project or local layer ([BACKEND-24](#BACKEND-24)): a layer that
names a `provider` block with these keys is dropped whole and `doctor` names the file. The model
Brave's endpoint serves, and an account the tier variables alone configure, have no entry to state
them in and are held to the defaults. A streamed reply from a gateway on this machine is waited on
through any silence while a person can stop it ([NET-10](network-egress.md#NET-10)), so a stated
gap does not reach it, and a stated count does.

**Why.** The defaults suit a service answering over a link that works. A person whose gateway
thinks silently for longer than two minutes, or whose link drops more than two requests in a row,
has no way to raise either, and a person billed for each request has no way to lower the count. A
checkout does not get a say in either: a gap it stretched holds a turn on a connection that has
died, and a count it raised multiplies requests billed to the person's account, on the strength of
a file they did not open.

Seven retries pause for 127 seconds in all by the doubling, and none for longer than 64, which stays
inside the two minutes [BACKEND-54](#BACKEND-54) holds a service's own request to. A gap longer than
ten minutes would be longer than the bound on a reply of unstated length, so it could never be the
bound that ended one. Both figures are read as numbers and set a count and a timer, so a service has
no say in either.

`verified-by: bravebot_config::provider::a_block_states_how_often_a_request_is_sent_again_and_how_long_a_quiet_reply_is_waited_on`
`verified-by: bravebot_config::provider::zero_retries_is_a_statement_and_zero_milliseconds_is_not`
`verified-by: bravebot_config::provider::a_figure_above_the_cap_is_the_cap`
`verified-by: bravebot_config::provider::a_figure_that_is_not_a_whole_number_states_nothing`
`verified-by: bravebot_config::provider::an_aws_entry_states_the_same_two_figures`
`verified-by: bravebot_config::settings::a_project_layer_cannot_change_a_gateways_retry_count_or_idle_gap`
`verified-by: bravebot_config::lib::a_managed_gateway_states_its_own_retry_count_and_idle_gap`
`verified-by: bravebot_aichat::client::a_gateway_stating_no_retries_is_asked_once`
`verified-by: bravebot_aichat::client::a_gateway_stating_a_retry_count_is_asked_again_that_many_times`
`verified-by: bravebot_aichat::client::a_gateway_stating_nothing_is_asked_three_times`
`verified-by: bravebot_aichat::client::a_larger_retry_count_does_not_make_a_refusal_worth_asking_again`
`verified-by: bravebot_aichat::client::a_gateway_stating_an_idle_gap_waits_that_long_for_a_reply_gone_quiet`
`verified-by: bravebot_aichat::client::a_gateway_stating_a_shorter_idle_gap_gives_up_on_a_quiet_reply_sooner`
`verified-by: bravebot_aichat::client::a_gateway_on_this_machine_is_waited_on_through_silence_whatever_gap_was_stated`
`verified-by: bravebot_bedrock::lib::an_account_stating_no_retries_is_asked_once`
`verified-by: bravebot_bedrock::lib::an_account_stating_a_retry_count_is_asked_again_that_many_times`
`verified-by: bravebot_bedrock::lib::an_account_stating_a_retry_count_reaches_a_service_that_recovers_inside_it`
`verified-by: bravebot_bedrock::lib::a_larger_retry_count_does_not_make_a_refusal_worth_asking_again`
`verified-by: bravebot_bedrock::lib::an_account_stating_an_idle_gap_waits_that_long_for_a_reply_gone_quiet`
`verified-by: bravebot_bedrock::lib::an_account_stating_a_shorter_idle_gap_gives_up_on_a_quiet_reply_sooner`

<a id="BACKEND-56"></a>
### BACKEND-56: a turn that begins in plan mode runs on the model a person named for planning

The `planModel` setting holds one model name. It is read from the person's own settings file and
from the file `--settings` names, and from no project or local layer ([BACKEND-24](#BACKEND-24)): a
layer that names it is dropped and `doctor` names the file. A tier word in it resolves as it does
for `model` ([BACKEND-12](#BACKEND-12)).

A turn that begins in plan mode ([MODE-3](permission-modes.md#MODE-3)) is sent to that model in
every round, in place of the session's own, including one the person picked with `/model`. A turn
that begins in any other mode is sent to the session's model, so the next acting turn is on the
model the plan turn was not. The mode read is the one the turn began in: a mode chosen while the
turn runs ([MODE-8](permission-modes.md#MODE-8)) does not change the model, so a turn moved out of
plan mode finishes on the plan model and a turn moved into it finishes on the session's. A front
end that sizes anything from the mode before the turn starts reads it once and gives the turn that
reading, so a mode pressed as the turn opens cannot give the turn one model and the front end
another.

The setting yields to a model named more specifically: a model a definition names for the turn
([DELEGATE-22](delegation.md#DELEGATE-22)), and `--model` on a one-shot run ([CLI-9](cli.md#CLI-9)).
A delegate takes the model it would have had without the setting, since its definition's model is a
boundary its file drew.

A name that cannot be used, because the managed layer refuses it ([BACKEND-48](#BACKEND-48)), no
configured service answers for it, or it needs a sign-in this machine has not made, is not sent to.
The turn runs on the session's model and the person is told which name was set aside. This differs
from `summaryModel`, which refuses the request: that key caps a cost, and this one asks for a
stronger model where one is available, so refusing the turn would remove the planning it was set
for. The context budget of an interactive turn on the plan model is measured against that model's
window ([BACKEND-7](#BACKEND-7)). The order above is settled by one function in the agent, which
the turn asks for its model and the terminal asks for the window, so the window is the one of the
model the turn is sent to.

**Why.** A person who wants a stronger model to plan and a cheaper one to act has to switch with
`/model` by hand at every change of mode. The mode is a person's own answer and the name is their
own setting, so nothing untrusted decides either, and a name in the settings is the endorsement of
the service that serves it, as the `model` key is.

`verified-by: bravebot_agent::turn::a_plan_mode_turn_is_sent_to_the_plan_model_and_an_acting_turn_to_the_sessions`
`verified-by: bravebot_agent::turn::a_plan_turn_with_no_plan_model_named_stays_on_the_sessions`
`verified-by: bravebot_agent::turn::a_plan_model_the_machine_refuses_falls_back_to_the_sessions_and_says_so`
`verified-by: bravebot_agent::turn::a_model_named_on_the_command_line_outranks_the_plan_model`
`verified-by: bravebot_agent::turn::a_delegate_in_plan_mode_keeps_the_model_it_was_given`
`verified-by: bravebot_config::settings::a_project_layer_cannot_name_the_plan_model`
`verified-by: bravebot_config::lib::the_plan_setting_resolves_a_tier_word_and_is_absent_when_unset`
`verified-by: bravebot_agent::turn::a_mode_chosen_while_the_turn_runs_does_not_change_its_model`
`verified-by: bravebot_agent::turn::a_turn_begins_in_the_mode_it_was_told_and_not_the_one_held_when_it_opens`
`verified-by: bravebot_agent::turn::the_model_answering_is_the_model_the_turn_is_sent_to`
`verified-by: bravebot_agent::turn::a_more_specific_model_outranks_the_plan_model_in_every_mode`
`verified-by: bravebot_tui::app::a_plan_turn_takes_the_plan_models_window_and_no_other_turn_does`
`verified-by: bravebot_tui::app::a_mode_pressed_after_a_turn_is_settled_leaves_it_in_the_mode_its_window_was_sized_for`
`verified-by: bravebot_tui::app::the_model_a_window_is_read_for_follows_the_order_the_agent_settles`
`verified-by: bravebot_cli::running::doctor_says_a_checkouts_plan_model_is_not_obeyed`

<a id="BACKEND-57"></a>
### BACKEND-57: an `anthropic` entry is reached through the Messages API, with its key in `x-api-key` and nowhere else

A `provider` entry keyed `anthropic` reaches Anthropic's API at the endpoint [BACKEND-17](#BACKEND-17)
compiles in, in the Messages protocol and not in chat completions. A request goes to the endpoint's
`/messages` route and names the model by the part of the name after the id
([BACKEND-18](#BACKEND-18)). It carries the key, where the entry names a place for one to live
([BACKEND-16](#BACKEND-16)), in the `x-api-key` header, in no `authorization` header and in no part of
the body, and it carries `anthropic-version` with the date this build writes requests for, key or no
key. The key is attached as a socket header ([NET-2](network-egress.md#NET-2)): a redirect that
leaves the scheme, host and port of the endpoint is sent the conversation and not the key, on the
roster request as on a message. An entry naming no place for a key sends none, and an exported `ANTHROPIC_API_KEY` is read only
where the entry names it. An endpoint the entry states is where requests go regardless, and is spoken
to in the same protocol, unless the entry names the OpenAI-compatible SDK ([BACKEND-17](#BACKEND-17)).
The key is in no trace, no error text a refused request produces, and no session record.

The conversation is restated as turns of blocks by the function a Bedrock request uses as well: the
system prompt is a top-level list, a tool result is a block on a user turn, turns of one role are
merged, arguments cross as an object, and a request that offers no tool carries its calls and results
as text. A user message merged into a turn that already ends in text starts its first text block on a
new paragraph (`\n\n`), so that the start of one message is never read as the end of the one before it:
a line ending in a number followed by a note beginning with a number would otherwise read as one
number. Text with no word in it is no block, and a call's arguments are an object whatever the
model wrote, this API refusing a blank text block and anything but an object there. A call's id is
spelled with the letters, digits, `_` and `-` this API accepts, a character outside them becoming
`_`, on the call and on its result alike, so a pair stays a pair. A result a tool
wrote nothing for, or only whitespace, carries no content. An attachment is an inline `base64` block
under the type its media type names, and one of a type this API takes as neither a picture nor a
document is dropped.

No `thinking` field is sent, and the model's thinking is not sent back. Each model keeps its own
default, what it writes while thinking is no part of the reply, and a conversation carries no
thinking block from an earlier round. A model's own `options` reach the body as
[BACKEND-15](#BACKEND-15) says, and a `thinking` field in them is sent as written. It is not meant to
be one: a request that asks for thinking and carries a tool call from an earlier round wants the
thinking before it, which this build does not send back, so the service refuses it from the second
round of a tool turn (Known costs).

A request the service refuses on its contents is sent again one concession at a time, in the order
this API is likeliest to refuse them: without the level, with the fallback ceiling of
[BACKEND-41](#BACKEND-41), without the lifetime, without the breakpoints and last without the ask for
arguments ([BACKEND-44](#BACKEND-44)). What answered is remembered as
[BACKEND-22](#BACKEND-22) remembers the level, and a request refused with every concession given up
settles nothing.

A reply is read whole or as server-sent events. A stream is complete once it says so, and one that
stops before is a reply that did not finish, which is sent again. A failure the service reports part
way through is named from a list this build holds and never by what the service wrote, and is asked
for again only where a second attempt could get past it: an overloaded service, a fault of its own, a
rate limit, a service that timed the request out. The retries an entry states and the wait a service asks for with a rate limit apply as
[BACKEND-54](#BACKEND-54) and [BACKEND-55](#BACKEND-55) say. A reply that stops because the service
declined to write it, or because the conversation filled the model's window, is a failure and not a
reply, with what it cost still counted, and one that filled the window is answered as a prompt the
service refused as too long is, by a shorter conversation ([COMPACT-14](compaction.md#COMPACT-14)). A request the service answers with its overloaded status, 529, is
asked for again as one answered 503 is. The model that answered is the one the reply names, which for
an alias is the dated model behind it.

**Why.** The service takes a key in a header of its own and refuses a request that lacks its version
header, so a gateway client with the other header is a request that cannot work. A copy of the key
anywhere but the header the service reads is a copy for anything on the path to log, and the absence
from errors and records is [CRED-23](credential-protection.md#CRED-23)'s concern applied to a new
header. A block naming no key sends none because the table supplies an address and nothing else:
`ANTHROPIC_API_KEY` is a name other tools read, a key exported for one of them may be scoped for it,
and it is sent here only where the person wrote the name down.

The walk is shared because both services take the same shape and every rule in it answers a refusal of
the service's: two turns of one role, a call block with no tool declared, an empty text block and a
result with no call to answer are each rejected, and a second copy of the rules is a second place for
them to drift apart. What differs is how a block is spelled, which each wire states.

Thinking is left to the model because a field that says how to think is a request the models take
differently: a budget is refused by the newer ones, `adaptive` by the older ones, and a setting for
how thinking is shown needs one of the two beside it. Thinking blocks are not sent back because the
service documents a history with every one removed as the one it accepts when a block's signature
does not verify, and keeping them would need a record of blocks that no session stores. What that
costs is under Known costs.

A failure is one of a closed set because the name arrives in a field the service wrote, and a name
that reaches a person or a diagnostic is a place for text a page could have written to appear.

The order concessions are given up in follows what this API refuses. Models released before the level
refuse it, and models with a lower maximum refuse the assumed ceiling, while every model reads a
breakpoint. A refusal answered after one field was dropped is then read as that field's, and the
field that is never at fault is the last to be blamed, which is what keeps a refused ceiling from
turning prompt caching off for the rest of the session.

`verified-by: bravebot_aichat::client::an_anthropic_request_carries_its_key_in_x_api_key_and_nowhere_else`
`verified-by: bravebot_aichat::client::an_anthropic_block_naming_no_key_sends_none`
`verified-by: bravebot_aichat::client::a_refused_anthropic_request_does_not_repeat_the_key`
`verified-by: bravebot_aichat::client::a_models_options_reach_the_anthropic_body_and_cannot_overwrite_what_the_turn_built`
`verified-by: bravebot_aichat::client::a_whole_anthropic_reply_is_read_into_text_calls_model_and_cost`
`verified-by: bravebot_aichat::client::an_anthropic_stream_is_assembled_from_its_events_and_asks_for_arguments_as_written`
`verified-by: bravebot_aichat::client::a_failure_reported_mid_stream_is_asked_again_only_where_a_second_attempt_could_pass_it`
`verified-by: bravebot_aichat::client::an_anthropic_stream_that_never_says_it_finished_is_asked_again_and_then_reported`
`verified-by: bravebot_aichat::client::a_refusal_is_reported_without_being_asked_again_and_keeps_its_cost`
`verified-by: bravebot_aichat::client::a_stop_before_an_anthropic_request_is_sent_sends_nothing`
`verified-by: bravebot_aichat::client::an_anthropic_entry_stating_no_retries_is_asked_once`
`verified-by: bravebot_aichat::client::a_reply_that_filled_the_models_window_is_reported_for_a_shorter_conversation_to_answer`
`verified-by: bravebot_aichat::client::an_assumed_ceiling_a_model_refuses_is_stepped_down_and_a_stated_one_never_is`
`verified-by: bravebot_agent::backend::a_window_filled_mid_reply_is_a_refused_body_and_a_refusal_is_not`
`verified-by: bravebot_aichat::client::an_anthropic_rate_limit_the_service_asked_to_wait_out_is_waited_for`
`verified-by: bravebot_aichat::client::an_overloaded_anthropic_service_is_asked_again_and_a_refused_key_is_not`
`verified-by: bravebot_cli::doctor::doctor_names_the_messages_route_an_anthropic_entry_is_reached_at`
`verified-by: bravebot_aichat::anthropic::the_gap_a_block_states_is_the_one_allowed_between_pieces_of_a_reply`
`verified-by: bravebot_aichat::anthropic::protocol::a_request_is_the_body_this_api_states`
`verified-by: bravebot_aichat::anthropic::protocol::a_request_offering_no_tools_sends_no_tools_and_its_calls_as_text`
`verified-by: bravebot_aichat::anthropic::protocol::a_call_and_its_result_are_matched_blocks_and_the_arguments_an_object`
`verified-by: bravebot_aichat::anthropic::protocol::a_result_that_wrote_nothing_carries_no_content`
`verified-by: bravebot_aichat::anthropic::protocol::an_attachment_crosses_as_inline_data_under_the_block_its_type_names`
`verified-by: bravebot_aichat::anthropic::protocol::a_reply_is_read_back_into_text_and_calls`
`verified-by: bravebot_aichat::anthropic::protocol::a_reply_calling_a_tool_with_no_arguments_keeps_the_call`
`verified-by: bravebot_aichat::anthropic::protocol::a_block_kind_this_does_not_model_leaves_the_rest_of_the_reply_readable`
`verified-by: bravebot_aichat::anthropic::protocol::the_events_that_carry_a_reply_decode`
`verified-by: bravebot_aichat::anthropic::protocol::reasoning_arrives_as_reasoning_and_adds_nothing_to_the_reply`
`verified-by: bravebot_aichat::anthropic::protocol::an_event_this_does_not_know_is_ignored_rather_than_failing`
`verified-by: bravebot_aichat::anthropic::protocol::a_known_event_in_a_shape_this_cannot_read_is_unreadable_and_not_ignored`
`verified-by: bravebot_aichat::anthropic::protocol::a_reported_failure_is_one_of_a_closed_set_and_never_named_by_the_service`
`verified-by: bravebot_aichat::anthropic::protocol::a_call_id_this_api_would_refuse_is_changed_on_the_call_and_its_result_alike`
`verified-by: bravebot_aichat::client::an_anthropic_key_does_not_follow_a_redirect_to_another_host`
`verified-by: bravebot_bedrock::protocol::text_with_no_word_in_it_and_arguments_that_are_not_an_object_are_not_sent_as_written`
`verified-by: bravebot_aichat::conversation::system_turns_are_hoisted_and_joined_in_order`
`verified-by: bravebot_aichat::conversation::consecutive_turns_of_one_role_merge`
`verified-by: bravebot_aichat::conversation::a_result_without_a_call_id_is_dropped_rather_than_sent_as_a_user_turn`
`verified-by: bravebot_aichat::conversation::tool_arguments_cross_from_a_string_to_an_object_and_a_bad_string_to_an_empty_one`
`verified-by: bravebot_aichat::conversation::a_request_offering_no_tools_sends_its_calls_and_results_as_text`
`verified-by: bravebot_aichat::conversation::empty_text_is_no_block_and_a_turn_of_nothing_is_no_turn`
`verified-by: bravebot_aichat::conversation::text_with_no_word_in_it_is_no_block`
`verified-by: bravebot_aichat::conversation::arguments_that_are_not_an_object_become_an_empty_object`
`verified-by: bravebot_aichat::anthropic::protocol::a_system_prompt_of_only_whitespace_is_not_sent`
`verified-by: bravebot_aichat::anthropic::protocol::a_result_of_only_whitespace_carries_no_content`
`verified-by: bravebot_aichat::conversation::only_a_data_uri_with_bytes_is_offered_to_the_wire_and_every_one_takes_a_position`
`verified-by: bravebot_bedrock::protocol::a_system_turn_becomes_the_top_level_field`
`verified-by: bravebot_bedrock::protocol::consecutive_tool_results_merge_into_one_turn`
`verified-by: bravebot_bedrock::protocol::a_note_after_the_line_a_person_typed_is_a_block_on_a_new_paragraph`
`verified-by: bravebot_aichat::anthropic::protocol::a_note_after_the_line_a_person_typed_is_a_block_on_a_new_paragraph`
`verified-by: bravebot_aichat::conversation::the_paragraph_separator_follows_the_text_a_turn_ends_in_and_nothing_else`
`verified-by: bravebot_agent::turn::the_skills_overview_is_a_separate_paragraph_from_the_prompt_on_block_based_wires`
`verified-by: bravebot_config::provider::an_anthropic_entry_is_reached_through_the_messages_api_at_anthropics_host`
`verified-by: bravebot_agent::backend::a_request_reaches_the_route_and_header_its_providers_protocol_states`
`verified-by: bravebot_agent::backend::a_failed_messages_call_keeps_its_attempts_and_what_the_service_counted`
`verified-by: bravebot_agent::failure_categories::a_failure_anthropic_reports_keeps_its_actionable_category`
`verified-by: bravebot_aichat::conversation::a_user_message_after_user_text_starts_a_new_paragraph`

<a id="BACKEND-58"></a>
### BACKEND-58: a `google-vertex` entry naming no project is reached through the native `generateContent` API, with its key in `x-goog-api-key` and nowhere else

A `provider` entry keyed `google-vertex` that states no `options.project`, and the environment's
`GOOGLE_API_KEY` where no project is set ([BACKEND-50](#BACKEND-50)), reach Vertex AI express mode:
Google's native `generateContent` API at `https://aiplatform.googleapis.com/v1`, which takes a key and
no project. An entry that states a project is reached through the OpenAI-compatible endpoint
([BACKEND-49](#BACKEND-49)), so the project is what picks the route. No region is part of the
address, and Google chooses where a request is served, so neither `options.location` nor
`VERTEX_LOCATION` is read here. An entry that states an `options.baseURL` and no project keeps the
OpenAI-compatible route at that endpoint ([BACKEND-17](#BACKEND-17)) unless it names the SDK
`@ai-sdk/google-vertex`, which says the endpoint takes Vertex's own API: the route below then
follows the endpoint it states in place of the host and the `/v1` path.

A request goes to `/publishers/<publisher>/models/<model>:generateContent`, or for a streamed reply
`:streamGenerateContent?alt=sse`. The name after the id ([BACKEND-18](#BACKEND-18)) is
`<publisher>/<model>`, as `google/gemini-2.5-flash`, and a name with no publisher is a model of
`google`. Both parts are written into the path, so each holds only letters, digits, `-`, `_` and `.`,
is not empty and is not dots alone, and a model whose name holds anything else is sent nowhere and fails the request
with an error that names no key. The key travels as [BACKEND-51](#BACKEND-51) says, as a socket
header ([NET-2](network-egress.md#NET-2)), so a redirect off the endpoint's host is sent the
conversation and not the key. An entry that names no place for a key sends none. The key is in no
trace, no error text a refused request produces, and no session record.

The conversation is restated as turns of parts by the function a Bedrock and an Anthropic request
use as well: the system prompt is `systemInstruction`, a tool result is a part on a `user` turn,
the assistant's turns are `model` turns, turns of one role are merged, a user message merged into a turn
that already ends in text starts on a new paragraph as [BACKEND-57](#BACKEND-57) says, arguments cross
as an object, and a request that offers no tool carries its calls and results as text. Tools are one
`functionDeclarations` list with each schema in `parametersJsonSchema`. A call carries no id this
agent can match a result by, so one is made up when a reply arrives, and a result is sent as a
`functionResponse` named for the function of the call it follows. A result with no call before it in
the conversation is sent as text, since the service refuses a response that answers nothing. An
attachment is inline data where its media type is a picture, a PDF or another type the service takes
inline, and is dropped otherwise.

A `thoughtSignature` that arrives beside a call is kept with the call in the member of
[BACKEND-52](#BACKEND-52), as `google.thought_signature`, from the streamed or whole reply through the
stored conversation, and is sent back beside that call's part and no other. A call that arrived with
none carries none. A reasoning summary the service marks as a thought is no part of the reply.

A reply ceiling is sent as `maxOutputTokens` only where a person stated one ([BACKEND-15](#BACKEND-15),
[BACKEND-41](#BACKEND-41)), the models differing in what they allow. The level of
[BACKEND-21](#BACKEND-21) is a `thinkingBudget`, a schema a task states is `responseMimeType` with
`responseJsonSchema`, and a model's own `options` reach the body as BACKEND-15 says without replacing
what the turn built, the `generationConfig` among them joining the settings the turn wrote there. A reply is read whole or as server-sent events, each holding the next parts, and
a stream is complete once a piece carries a finish reason: one that stops before is a reply that did
not finish, which is sent again. A call arrives whole and is never assembled from fragments. A prompt
the service blocked, and a reply it stopped for what it would have said, are refusals and are not
asked for again. A call it wrote badly, a rate limit, an unavailable service, a fault of its own and
a timeout are asked for again, a failure written into a stream being named from the closed set
[BACKEND-57](#BACKEND-57) uses and never by what the service wrote. The retries an entry states and
the wait a service asks for apply as [BACKEND-54](#BACKEND-54) and [BACKEND-55](#BACKEND-55) say. A
reply that stopped at the ceiling is answered as [BACKEND-42](#BACKEND-42) says, with the ceiling
a person stated or, where none was sent, what the reply had written, and with the model counted as
having reasoned where the service states reasoning tokens, since it returns no reasoning unless it
is asked to. The model a completion names is the one the request named, and its prompt is the
figure the service states with the cached part inside it and its output the reply and the reasoning
together. The service leaves out a count that is zero, so a reply that is over is billed what the
service stated whether or not it stated an output figure.

**Why.** A person holding only a Vertex API key has no project to build the OpenAI-compatible
endpoint from, and express mode is the route Google gives that key. Making the project the choice
between routes keeps the first route's behaviour for everyone who stated one. The path carries the
model and the host carries the key, so the model name is held to the characters a path takes for the
reason BACKEND-49 holds a project to its own: a name that could end the path early would send the
key to another route. The conversation walk is shared for the reasons BACKEND-57 gives, and what
differs is how a part is spelled, which this wire states. A result is matched to its call by
position because the service gives a call no id: ids made up from a counter restart with the process,
so one conversation can hold the same id twice. The signature is opaque and kept unread, as
BACKEND-52 keeps it: the newer models refuse the next request with a 400 without it, and the older
ones accept the call with or without it. No ceiling is sent by default because a figure above a
model's own is refused, and a model that stops at its own ceiling is reported with the figure the
reply reached, which says how long it was. The level goes out as a budget because it is the field
every Gemini model reads and a figure within the smallest model's range is one none refuses on its
value, while which models read a budget at all is learned from a refusal as BACKEND-22 learns it.

`verified-by: bravebot_config::provider::a_google_vertex_entry_without_a_project_is_reached_in_express_mode`
`verified-by: bravebot_config::provider::an_express_request_goes_to_the_publisher_and_model_it_names`
`verified-by: bravebot_config::provider::a_model_name_that_would_move_an_express_request_has_no_url`
`verified-by: bravebot_config::provider::a_stated_endpoint_is_where_an_express_entry_is_reached`
`verified-by: bravebot_config::provider::a_stated_endpoint_with_no_project_keeps_chat_completions_unless_the_sdk_says_otherwise`
`verified-by: bravebot_config::lib::a_key_alone_names_the_express_service_and_a_blank_project_is_no_project`
`verified-by: bravebot_aichat::client::a_google_express_request_carries_its_key_in_x_goog_api_key_and_nowhere_else`
`verified-by: bravebot_aichat::client::a_google_express_request_without_a_key_sends_no_credential_header`
`verified-by: bravebot_aichat::client::a_refused_google_express_request_does_not_repeat_the_key`
`verified-by: bravebot_aichat::client::a_model_name_that_would_move_an_express_request_sends_nothing`
`verified-by: bravebot_aichat::client::a_whole_google_express_reply_is_read_into_text_calls_and_cost`
`verified-by: bravebot_aichat::client::a_google_express_stream_is_assembled_from_its_pieces`
`verified-by: bravebot_aichat::client::a_calls_signature_comes_back_with_it_in_the_next_request`
`verified-by: bravebot_aichat::client::a_google_express_reply_stopped_for_safety_is_a_refusal_on_both_paths`
`verified-by: bravebot_aichat::client::a_prompt_the_service_blocked_is_a_refusal`
`verified-by: bravebot_aichat::client::a_google_express_reply_cut_off_at_the_ceiling_keeps_what_it_wrote`
`verified-by: bravebot_aichat::client::a_google_express_stream_with_no_finish_reason_is_an_incomplete_reply`
`verified-by: bravebot_aichat::client::a_google_error_written_into_the_stream_is_reported_by_its_status`
`verified-by: bravebot_aichat::client::a_google_failure_reported_mid_stream_is_asked_again_where_a_second_attempt_could_pass_it`
`verified-by: bravebot_aichat::client::a_google_express_entry_stating_no_retries_is_asked_once`
`verified-by: bravebot_aichat::client::a_stop_before_a_google_express_request_is_sent_sends_nothing`
`verified-by: bravebot_aichat::client::a_models_options_reach_the_google_express_body_and_cannot_overwrite_what_the_turn_built`
`verified-by: bravebot_aichat::client::a_schema_goes_to_the_service_as_a_json_mime_type_and_a_schema`
`verified-by: bravebot_aichat::client::a_models_generation_settings_join_the_ones_the_turn_set_and_do_not_replace_them`
`verified-by: bravebot_aichat::client::a_reply_cut_off_while_reasoning_says_so_and_keeps_its_bill_on_both_paths`
`verified-by: bravebot_aichat::generate_content::protocol::a_request_is_contents_of_parts_with_the_system_prompt_and_tools_beside_them`
`verified-by: bravebot_aichat::generate_content::protocol::a_request_asking_nothing_extra_carries_no_generation_config`
`verified-by: bravebot_aichat::generate_content::protocol::a_call_and_its_result_are_matched_by_the_functions_name`
`verified-by: bravebot_aichat::generate_content::protocol::a_result_answers_the_call_it_follows_when_an_id_was_used_twice`
`verified-by: bravebot_aichat::generate_content::protocol::a_result_with_no_call_in_the_conversation_is_sent_as_text`
`verified-by: bravebot_aichat::generate_content::protocol::a_calls_thought_signature_is_sent_back_beside_its_part`
`verified-by: bravebot_aichat::generate_content::protocol::a_request_offering_no_tools_sends_calls_and_results_as_text`
`verified-by: bravebot_aichat::generate_content::protocol::an_attachment_is_inline_data_for_a_media_type_this_api_takes`
`verified-by: bravebot_aichat::generate_content::protocol::a_whole_reply_is_read_into_text_calls_and_cost`
`verified-by: bravebot_aichat::generate_content::protocol::a_call_naming_no_arguments_has_an_empty_object_and_each_call_its_own_id`
`verified-by: bravebot_aichat::generate_content::protocol::a_note_after_the_line_a_person_typed_is_a_part_on_a_new_paragraph`
`verified-by: bravebot_aichat::generate_content::protocol::a_reply_that_stopped_for_a_reason_is_a_failure_and_an_ordinary_stop_is_not`
`verified-by: bravebot_aichat::generate_content::protocol::a_google_status_word_maps_to_a_failure_in_the_closed_set`
`verified-by: bravebot_aichat::generate_content::protocol::a_streamed_piece_is_a_response_an_error_or_nothing_this_reads`
`verified-by: bravebot_agent::backend::a_request_reaches_the_route_and_header_its_providers_protocol_states`
`verified-by: bravebot_agent::backend::a_failed_generate_content_call_keeps_its_attempts_and_what_the_service_counted`
`verified-by: bravebot_aichat::conversation::a_user_message_after_user_text_starts_a_new_paragraph`

<a id="BACKEND-59"></a>
### BACKEND-59: a streamed chat-completions reply hands the thinking it sends in `reasoning_content` to the screen and to nothing else

A streamed reply from Brave's endpoint or from a gateway speaking the OpenAI-compatible protocol may
carry the model's working in a `reasoning_content` field beside the reply's words. That text is
handed to the caller as it arrives, in the order it arrives and once, under the label the reply's
words carry, for a screen to draw on request ([VIEW-33](terminal-transcript.md#VIEW-33)) and for
nothing else. None of it becomes the reply's words, the live count of what the reply wrote or its
record in the conversation, and a reply that carried it is as readable as one that did not.

Nothing in the request asks for it, so a model that sends none draws none, and what a model sends is
all there is to show. A field that holds anything but text is read as nothing, and the rest of the
chunk is read as it would have been without the field. A reply asked for whole shows none of it.

**Why.** [BACKEND-46](#BACKEND-46) asks Bedrock to send the thinking because that service sends none
unasked. A chat-completions service has no such ask, and several models sold through one put their
working in this field on their own, so what is left to decide is whether to read it. Reading it
leaves the request as it was, and a person who has not turned `/thinking` on sees what they saw
before, since the screen draws it only on request. The words are model output and are carried on the
reply's own footing, so the driver hands them on without reading them. A field in another shape is
read past because a chunk refused for it fails a turn that ran before the field was read.

`verified-by: bravebot_aichat::client::the_thinking_a_gateway_sends_reaches_progress_and_never_the_reply`
`verified-by: bravebot_aichat::protocol::thinking_is_taken_once_in_the_order_it_arrived_and_is_no_part_of_the_reply`
`verified-by: bravebot_aichat::protocol::reasoning_that_is_not_text_is_read_as_nothing_and_the_reply_still_arrives`
`verified-by: bravebot_aichat::protocol::reasoning_in_its_own_field_is_no_part_of_the_reply`
`verified-by: bravebot_aichat::protocol::a_reply_that_reasoned_in_its_own_field_answers_with_the_text_alone`

<a id="BACKEND-60"></a>
### BACKEND-60: a gateway block may name headers to send, from the person's own layers only

`options.headers` in a gateway block is a map of header name to value, and each one is attached to
every request that block's service answers, including the request that asks for its roster. Names
are case-insensitive, so one name written twice is one header. A value may be a whole-value
`{env:NAME}` reference, read from the environment at the point a request needs it, on
[BACKEND-16](#BACKEND-16)'s footing for a credential; a variable holding nothing sends no header.
The value is carried and attached, never interpreted ([BACKEND-15](#BACKEND-15)).

Headers are read from the person's own settings file, the file `--settings` names, and the managed
layer, and from no project or local layer. They are inside the `provider` block, so
[BACKEND-24](#BACKEND-24) already drops a project or local layer that names one and reports the
file.

A name the request's own framing or authentication depends on is refused when the block is read,
and the block's other headers are still sent: `host`, `content-length`, `content-type`,
`transfer-encoding`, `connection`, and the header this provider's credential goes in where the
block names a credential, which is `x-goog-api-key` for `google-vertex`
([BACKEND-51](#BACKEND-51)), `x-api-key` for an `anthropic` entry reached through the Messages API
([BACKEND-57](#BACKEND-57)), and `authorization` for every other gateway, whichever protocol it
answers in. A block naming no credential may write `authorization` itself, because then nothing
else is setting it. `anthropic-version` is refused on an `anthropic` entry the same way, since this
program sets it on every Messages request before a configured header is. An entry naming AWS sends
no configured header at all, Bedrock signing over the headers it sends.
A name that is not a field name, a value holding a line ending or another character a header
cannot carry, and a value holding `{env:` or `{file:` as part of something longer, are refused the
same way. A name with nothing under it, and a `headers` that is not a map, configure no header and
are not refusals.

A header value is a `Secret`: it is in no trace, no error text, and nothing a diagnostic prints
([CRED-23](credential-protection.md#CRED-23)).

`doctor` reports every refused header under the gateway block it was written on, with the name as
written and which of the four reasons refused it, and never with the value. The name goes through
the same printing as any other configured text, so a control character in a name does not reach the
terminal.

**Why.** A gateway in front of several services routes on a header of its own: a tenant id, a
routing key, a second credential. Without this the block cannot reach one. The values are
person-written configuration, so nothing untrusted is read here, and that is also the bound: a
project layer that could add a header could send the session's credential and context to a service
nobody in the checkout agreed to, which is what BACKEND-24 keeps out of a file nobody opened.
Refusing the framing names is what keeps the request the one this program built, since a header is
appended rather than replaced and two values under one name leave the service to choose. Refusing
the credential's own name keeps one request from carrying two things that authenticate it. A
reference inside a longer value is refused rather than sent because nothing here substitutes, so
what would go is the literal braces. A refusal that named no header leaves whoever wrote the block
with no line to edit, and one that quoted the value would print a routing key or a second
credential into wherever the output goes.

`verified-by: bravebot_config::provider::a_header_a_block_asked_for_is_read`
`verified-by: bravebot_config::provider::a_header_that_would_change_framing_is_refused`
`verified-by: bravebot_config::provider::one_name_written_twice_sends_one_header`
`verified-by: bravebot_config::provider::the_header_a_credential_goes_in_is_refused_only_where_one_is_named`
`verified-by: bravebot_config::provider::a_vertex_block_refuses_the_header_its_key_goes_in`
`verified-by: bravebot_config::provider::an_anthropic_block_refuses_the_header_its_key_goes_in`
`verified-by: bravebot_config::provider::an_anthropic_block_refuses_the_version_header_the_api_requires`
`verified-by: bravebot_config::provider::a_header_value_naming_a_variable_is_resolved_at_the_point_of_use`
`verified-by: bravebot_config::provider::a_reference_inside_a_longer_header_value_is_refused`
`verified-by: bravebot_config::provider::a_header_that_is_not_sendable_is_refused`
`verified-by: bravebot_config::provider::a_malformed_headers_block_configures_no_headers`
`verified-by: bravebot_config::provider::an_aws_block_sends_no_configured_header`
`verified-by: bravebot_config::provider::printing_a_block_does_not_print_a_header_value`
`verified-by: bravebot_aichat::client::a_header_the_block_asked_for_reaches_the_request`
`verified-by: bravebot_aichat::client::a_header_the_block_asked_for_reaches_the_roster_request`
`verified-by: bravebot_aichat::client::a_header_the_block_asked_for_reaches_an_anthropic_request`
`verified-by: bravebot_aichat::client::a_header_the_block_asked_for_reaches_a_google_express_request`
`verified-by: bravebot_agent::backend::a_header_the_block_asked_for_reaches_the_request_whichever_protocol_it_answers_in`
`verified-by: bravebot_agent::backend::a_header_naming_a_variable_is_read_where_the_token_is`
`verified-by: bravebot_config::settings::a_project_layer_cannot_add_a_request_header`
`verified-by: bravebot_config::provider::a_stored_key_refuses_a_written_authorization_once_it_is_known`
`verified-by: bravebot_config::lib::a_stored_key_reaches_through_from_sources_refuses_the_authorization_it_names`
`verified-by: bravebot_cli::doctor::a_refused_header_is_reported_with_its_name_and_reason_and_never_its_value`
`verified-by: bravebot_cli::running::doctor_names_a_refused_header_without_its_value`

<a id="BACKEND-61"></a>
### BACKEND-61: a service that takes the Responses protocol is sent a request in it and read in it, and nothing is asked to be kept

A request is made in the Responses protocol where the service is the ChatGPT account of
[CHATGPT-10](chatgpt-sign-in.md#CHATGPT-10), where a `provider` entry, or a model under one, names
`@ai-sdk/openai` in `npm`, as opencode names the SDK that speaks it, and where an `openai` entry reaches
OpenAI's own host without naming `@ai-sdk/openai-compatible` ([BACKEND-17](#BACKEND-17)), as opencode's
`openai` provider does. Every other service is spoken to in chat completions, and an entry naming no SDK
stays one, an `openai` entry that states a proxy's endpoint among them. The ChatGPT account is always spoken
to in this protocol, and no request is made to its host in another. A model's own `npm` outranks its
entry's, so one entry may offer models in both protocols. The SDK an entry names decides over its id, except
under `google-vertex`, whose id fixes its protocols and the header its key goes in: an `anthropic` entry that names `@ai-sdk/openai` is reached in this protocol, and states an endpoint of its own,
as one naming `@ai-sdk/openai-compatible` does ([BACKEND-17](#BACKEND-17)), since the address compiled in is
the one the Messages API is spoken at. The request goes to the endpoint's
`/responses` route, names the model by the part of the name after the id ([BACKEND-18](#BACKEND-18)), and
carries the key as [BACKEND-16](#BACKEND-16) says, as a bearer token in `authorization`. The roster is asked
for as [BACKEND-19](#BACKEND-19) says, at the route that service lists on.

The body is built from the conversation as these fields:

- `instructions` is the system prompt, and is absent where there is none.
- `input` is the turns in order. A turn's words are a `message` item with its role and `input_text` parts,
  or `output_text` parts for the model's own, and a picture is an `input_image` part holding a data URI. A
  call the model made is a `function_call` item with the `call_id`, the `name` and the arguments as a
  string, and its result is a `function_call_output` item with the same `call_id` and the result as text.
  Calls and results are paired, ordered and cleaned of what the service would refuse by the walk of the
  conversation that [BACKEND-57](#BACKEND-57) names, so a result with no call is dropped there and not here,
  and arguments that are not a JSON object cross as `{}`. A user message merged into a turn that already
  ends in text starts its first text part on a new paragraph, as [BACKEND-57](#BACKEND-57) says.
- `tools` is each tool as one object with `type` `function`, a `name`, a `description`, `parameters` and
  `strict` of `false`, and `tool_choice` is `auto` where any is offered.
- `stream` is `true` on every request, and the reply is read as server-sent events whatever the caller
  asked for, since a service in this protocol may refuse one asked for whole.
- `store` is `false` on every request. Nothing is asked to be kept, so the conversation is held by the
  session and by nothing at the service, and `previous_response_id` is never sent.
- `reasoning` is `effort`, the level's word, and `summary`, `auto`, where a level was given and the roster
  says the model reads it ([BACKEND-22](#BACKEND-22)). Where no level was given the object is absent and the
  service applies its own default ([BACKEND-20](#BACKEND-20)), so no summary is asked for and a model that
  sends none draws none for `/thinking`.
- `max_output_tokens` is the ceiling of [BACKEND-41](#BACKEND-41).
- No `temperature`, `top_p` or other sampling field is added by this program. On an entry, a model's own
  `options` reach the body as [BACKEND-15](#BACKEND-15) says and carry the fields a person chose to send,
  and none of them supplies or replaces a field the request owns, whether or not the turn set it: the model,
  `instructions`, `input`, `tools`, `tool_choice`, `stream`, `store` and `previous_response_id`.
  The ChatGPT account carries no `options` ([CHATGPT-10](chatgpt-sign-in.md#CHATGPT-10)).

The body is that JSON as it is built for every service but the ChatGPT account. A request to the account
carries it as one zstd frame with `content-encoding: zstd`, whatever its size, and this program sets no limit on
the frame ([CHATGPT-10](chatgpt-sign-in.md#CHATGPT-10)). It adds no `content-encoding` to a gateway's request.

The reply's events are read as they arrive. Words come from the text deltas of an output item. A call
comes from a `function_call` item, whose arguments arrive in pieces as the model writes them, so the count
of [BACKEND-45](#BACKEND-45) moves with them. The reply is assembled from the items the stream completes,
and never from the `output` list of its final event, which is empty in `response.completed` on the ChatGPT
host. A call is handed to the caller only in a reply that finished, so a call is never run for a reply that
is asked for again. The model's reasoning arrives as a summary and is handed to the caller as it arrives,
for a screen to draw on request ([VIEW-33](terminal-transcript.md#VIEW-33)) and for nothing else, as
[BACKEND-59](#BACKEND-59) hands on a chat-completions model's. An event this does not know is ignored, and
one it knows in a shape it cannot read is unreadable and not ignored. A stream ends in one of three events.
`response.completed` is a reply. `response.incomplete` is a reply the service stopped for the ceiling, and
is kept for what it said and never for what it asked ([BACKEND-42](#BACKEND-42)), not asked for again; one
stopped for its content is a refusal, named as [BACKEND-57](#BACKEND-57) names one. A reply that holds a
refusal content part is a refusal in the same way, whichever event announces the part: a
`response.refusal.delta` or `response.refusal.done` event, a `response.content_part.added` or
`response.content_part.done` event whose part is of type `refusal`, or a finished message item with a part of
type `refusal`. Only that the part exists is read from it, so the words the service wrote reach no screen, record
or error. The refusal is remembered until the final event, which cannot clear it, and applies to a reply that
also finished a call, whose call is handed on to nobody. A refusal in a stream that stopped before a final
event is asked for again with the rest of that stream. A final event in a shape
this cannot read is an unreadable reply and not one that stopped short. `response.failed` is a failure named
from a list this build holds and never by what the service wrote, and asked for again only where a second
attempt could get past it, as [BACKEND-57](#BACKEND-57) does for a failure reported part way through. The
codes with which the ChatGPT account says it has reached its usage limit or may not be used this way
([CHATGPT-8](chatgpt-sign-in.md#CHATGPT-8)), in a `response.failed` event or an `error` event, are read on
the stream as that account's refusal and not as a failure of this list, and end the turn in the sentence of
that clause without being asked for again. The code of an `error` event is the `code` of an `error` object
in it or, where that holds none, its own `code`, and the object's wins where both are strings. From any
other service they are a failure it did not name. A stream that stops before any of them is a reply that
did not finish and is asked for again, as many times as the entry's retries allow
([BACKEND-55](#BACKEND-55)) and no more.

A reasoning item a reply carried is not sent back. The conversation keeps a model's words and calls, so a
later request carries none of its earlier reasoning, which the service accepts. The request does not ask
for the encrypted form of it.

Usage is read from the final event. `input_tokens` is the whole prompt, with the part the service answered
from its cache in `input_tokens_details.cached_tokens` counted inside it and the part this request wrote
in `cache_write_tokens`, as [BACKEND-31](#BACKEND-31) asks of every service. `output_tokens` is the reply, with
`output_tokens_details.reasoning_tokens` counted inside it. A figure the service did not state is zero.

A refusal arrives as a status and a body, and the body's text, which the service wrote, reaches no person,
no diagnostic and no record. On the ChatGPT host that body holds a `detail` sentence, an `error` object with a
`message`, a `type`, a `param` and a `code`, or both. A field the service does not take is refused with a 400,
which is not asked for again, and a model the account does not offer ends the turn in the sentence of
[CHATGPT-8](chatgpt-sign-in.md#CHATGPT-8). The retries an entry states and the wait a service asks for with
a rate limit apply as [BACKEND-54](#BACKEND-54) and [BACKEND-55](#BACKEND-55) say.

**Observed.** The ChatGPT host has been seen to take a text turn, a function call and its result, a
reasoning turn with a summary, a picture, the model's own words as `output_text`, a refused access token and
the refusals above, to end a reply stopped at its ceiling with `response.incomplete`, and to count the
cached part of a prompt inside `input_tokens`
([What the exchange has to record](chatgpt-sign-in.md#what-the-exchange-has-to-record)). A `response.failed`
event has not been seen there, and an `error` event has not been seen on the POST event stream this protocol
reads. An `error` event was seen over the ChatGPT host's websocket on 2026-10-27, with its code in an `error`
object ([What the exchange has to record](chatgpt-sign-in.md#what-the-exchange-has-to-record)).

`api.openai.com` was observed on 2026-10-27, with a key whose account had credit. A chat completion that
offered one function tool and carried no `reasoning_effort` was answered 400 by `gpt-6-sol`, with the error
type `invalid_request_error`, the `param` `reasoning_effort` and the message "Function tools with
reasoning_effort are not supported for gpt-6-sol in /v1/chat/completions. To use function tools, use
/v1/responses or set reasoning_effort to 'none'.". With `reasoning_effort` `high`, `gpt-6-sol` and
`gpt-6.1-sol` each refused it with the same message for their own name. With `none`, `gpt-6-sol` took it and
returned a tool call, and `gpt-6.1-sol` refused `none` with 400, "Unsupported value: 'reasoning_effort' does
not support 'none' with this model. Supported values are: 'low', 'medium', 'high', and 'xhigh'.". A chat
completion without a tool answered 200 for `gpt-6-sol`, whole and streamed with `stream_options.include_usage`.
A request to `/responses` that offered the same function tool, with `store` `false`, completed for both
models. Through this program, `bravebot -p` on `openai/gpt-6-sol` and on `openai/gpt-6.1-sol` ended with HTTP
400 at `/chat/completions`, and ended with a reply and exit status 0 for both when the entry named
`@ai-sdk/openai`. A turn that asked for the word in a file, which the model read with a tool call, ended with
that word and exit status 0 for `gpt-6-sol`, `gpt-6.1-sol` and `gpt-4.1` at an `openai` entry that names no SDK,
and ended with HTTP 400 for `gpt-6-sol` where the entry named `@ai-sdk/openai-compatible`. Each was run once.
Sending the request again without the level, as a refusal of a level is answered
([BACKEND-22](#BACKEND-22)), does not get past the refusal for the tool, since no level was sent. Nothing has
been observed at `api.openai.com` in this protocol of cached tokens, a reasoning summary, a picture, a reply
stopped at its ceiling or a turn of several rounds. No service has been seen to send
`response.refusal.delta`, `response.refusal.done`, a content part event with a part of type `refusal` or a message
item with a part of type `refusal`; those names are read from the protocol's published description.

**Built:** the protocol for an entry that names `@ai-sdk/openai`, in `bravebot-aichat` beside the Anthropic
and Gemini clients: the request, the events, calls and their results, usage, the level, the three endings
of a stream, and the refusals, including a refusal content part, reached at the entry's `/responses` route
with its key as a bearer token.

**Not built:** an `npm` on a model, so the SDK an entry names decides for every model it lists; a run's
`--output-schema`, which the request does not carry; a document attachment, which is dropped
([#3121](https://github.com/brave/bravebot/issues/3121)); and [IMPORT-3](import.md#IMPORT-3) still leaves
an `openai` entry that names `@ai-sdk/openai`.

**Why.** The service decides the protocol, so the same JSON serves the ChatGPT account and a gateway that
takes it, and the account's frame is that JSON encoded at the last step, so there is no second body to drift. `store` is `false` because a
request that asks a service to keep a conversation puts the conversation somewhere this program cannot delete
it from, and because the session already holds all of it. The stream is always asked for because the host that
serves the subscription refuses a request that is not, and a reply read as events is read the same way whoever
asked. The reply is assembled from items because a final event with an empty list is a reply with nothing in
it to any reader that trusts it. A reasoning item is left out because the one a reply carries is an opaque
value that only the service reads, a conversation of rounds would send it again every round, and the
service answers the same request without it. This program adds no sampling field because it has no value to choose,
and the ChatGPT host refuses `temperature`, so a field added for every service would fail on the one this
clause was written for.

`verified-by: bravebot_config::provider::an_entry_naming_the_openai_sdk_is_reached_through_the_responses_api`
`verified-by: bravebot_config::provider::an_openai_entry_is_reached_in_the_responses_protocol_at_openais_own_api_only`
`verified-by: bravebot_aichat::responses::protocol::a_request_is_the_body_this_api_states`
`verified-by: bravebot_aichat::responses::protocol::a_listed_code_reported_on_the_stream_is_a_refusal_and_no_other_failure`
`verified-by: bravebot_aichat::responses::protocol::a_call_and_its_result_are_items_beside_the_messages`
`verified-by: bravebot_aichat::responses::protocol::a_request_offering_no_tools_sends_no_tool_fields_and_its_calls_as_text`
`verified-by: bravebot_aichat::responses::protocol::reasoning_is_the_level_and_a_summary_only_where_a_level_was_given`
`verified-by: bravebot_aichat::responses::protocol::a_picture_crosses_as_a_data_uri_and_a_document_is_dropped`
`verified-by: bravebot_aichat::responses::protocol::a_note_after_the_line_a_person_typed_is_a_part_on_a_new_paragraph`
`verified-by: bravebot_aichat::responses::protocol::the_events_that_carry_a_reply_decode`
`verified-by: bravebot_aichat::responses::protocol::a_stream_ends_in_one_of_three_events_and_reports_usage`
`verified-by: bravebot_aichat::responses::protocol::a_reported_failure_is_one_of_a_closed_set_and_never_named_by_the_service`
`verified-by: bravebot_aichat::responses::protocol::an_unknown_event_is_ignored_and_a_known_one_in_a_wrong_shape_is_not`
`verified-by: bravebot_aichat::responses::protocol::a_call_with_no_arguments_written_is_an_empty_object`
`verified-by: bravebot_aichat::client::a_responses_request_is_streamed_kept_nowhere_and_carries_its_key_as_a_bearer_only`
`verified-by: bravebot_aichat::client::a_responses_reply_is_read_from_its_events_and_its_cost_from_the_final_one`
`verified-by: bravebot_aichat::client::a_responses_reasoning_summary_reaches_progress_and_never_the_reply`
`verified-by: bravebot_aichat::client::a_responses_call_is_taken_from_its_item_and_named_while_it_is_written`
`verified-by: bravebot_aichat::client::a_responses_stream_that_stops_short_is_asked_again_and_hands_on_no_unfinished_call`
`verified-by: bravebot_aichat::client::an_incomplete_responses_reply_keeps_its_words_and_not_its_call`
`verified-by: bravebot_aichat::client::a_failed_responses_reply_is_named_from_a_closed_set_and_asked_again_only_where_it_could_pass`
`verified-by: bravebot_aichat::client::a_level_a_responses_service_refuses_costs_the_field_and_not_the_turn`
`verified-by: bravebot_aichat::client::a_models_options_reach_the_responses_body_and_cannot_overwrite_what_the_turn_built`
`verified-by: bravebot_aichat::client::a_refused_responses_request_does_not_repeat_the_key`
`verified-by: bravebot_aichat::client::a_responses_call_is_named_before_its_arguments_arrive_and_they_are_never_charged`
`verified-by: bravebot_aichat::client::a_final_responses_event_in_a_shape_this_cannot_read_is_not_asked_for_again`
`verified-by: bravebot_aichat::client::a_responses_reply_stopped_for_its_content_is_a_refusal_and_not_a_ceiling`
`verified-by: bravebot_aichat::client::a_responses_refusal_part_is_a_refusal_and_its_words_reach_nothing`
`verified-by: bravebot_aichat::client::a_responses_refusal_beside_a_call_hands_on_no_call_and_one_cut_short_is_asked_again`
`verified-by: bravebot_aichat::responses::protocol::a_refusal_part_decodes_to_an_event_that_carries_no_words`
`verified-by: bravebot_aichat::client::an_account_body_is_encoded_whatever_its_size`
`verified-by: bravebot_aichat::client::a_gateways_body_is_plain_json_with_no_content_encoding`
`verified-by: bravebot_agent::backend::a_request_reaches_the_route_and_header_its_providers_protocol_states`
`verified-by: bravebot_agent::backend::a_header_the_block_asked_for_reaches_the_request_whichever_protocol_it_answers_in`

<a id="BACKEND-62"></a>
### BACKEND-62: a `baseURL` or an endpoint that writes a port which cannot be read is refused where it is read, and the refusal names the port

A port is read from the authority of a url as ASCII digits, at least one, that fit a `u16`. After the
host, a `:` followed by anything else (nothing, letters, a sign, a value above 65535, a second `:`)
is a port that cannot be read. A client given such a url connects to the scheme's default port, which
is not the socket the url spells, and a request sent there carries no credential
([SERVERS-11](mcp-servers.md#SERVERS-11)). The rule is the one a declared MCP url is held to, read by
one function, so the readers cannot disagree.

A `provider` entry whose `options.baseURL` writes such a port is not a gateway. It is left out of the
entries that stand, and `bravebot doctor` names it by its id and by the text written where the port
belongs, empty where nothing follows the `:`. The rest of the url is never shown, since it may state
credentials. Entries that state a readable port, or none, are unaffected by another entry's refusal.
A `BRAVE_AI_CHAT_ENDPOINT` that writes such a port fails the configuration with an error that names
the text written there, as a missing scheme fails it.

The other readers of a url are unchanged. `BRAVE_AI_CHAT_PREMIUM_ENDPOINT` is not used when it is
malformed, the host compiled in standing in its place ([PREM-11](premium-credentials.md#PREM-11)), which this
clause leaves as it is, and the constants for the ChatGPT account and xAI state
their own ports. A block imported from `opencode.json` is written into the settings file as it
stands, so the rule applies when that file is read.

**Built:** `bravebot_config::provider::host_and_port`, which `remote` in `mcp.rs` and the two readers
above call; `Provider::refused`, `Settings::providers_refused` and the `doctor` line;
`ConfigError::UnreadablePort`.

**Not built:** a refusal shown anywhere but `bravebot doctor` and the error the configuration returns.

**Why.** A url that names a socket other than the one the client connects to is a request to a place
the person did not write, and with the credential dropped as a move it fails as an authentication error that
says nothing about the port. Refusing where the url is read puts the fault on the line the person wrote.

`verified-by: bravebot_config::provider::a_base_url_whose_port_cannot_be_read_is_refused_naming_the_port`
`verified-by: bravebot_config::provider::a_refused_base_url_leaves_the_other_entries_standing`
`verified-by: bravebot_config::provider::a_port_is_read_as_a_declared_mcp_url_reads_it`
`verified-by: bravebot_config::lib::an_endpoint_whose_port_cannot_be_read_is_rejected_naming_the_port`
`verified-by: bravebot_cli::doctor::a_gateway_dropped_for_its_port_is_reported_with_its_id_and_the_port_and_not_the_url`
`verified-by: bravebot_cli::running::doctor_names_a_provider_left_out_for_its_port_without_the_rest_of_its_url`

<a id="BACKEND-63"></a>
### BACKEND-63: a Bedrock model that refused the ask to show thinking is not asked again for thirty days

Where a streamed request to a Bedrock model was refused for carrying the ask to show thinking
([BACKEND-46](#BACKEND-46)) and the same request without it was answered, the model's name is kept
with the time of the finding in `bedrock-refusals.json` in the state directory. The first streamed
request to that model in a later session leaves the ask out, as a request after the finding in the
same session does.

An entry is believed while it is less than thirty days from now, in either direction. After that the
model is asked again, and a model that refuses again is kept again from that time. A stamp far in the
future, from a clock set wrong or a file edited by hand, is believed for no longer than one in the
past. The file names at most sixty-four models, and a write drops the oldest past that and every
entry no longer believed. Entries are by the name the request goes out under, so what one model
refused says nothing about another.

Nothing but the name and the time is kept, and nothing a service sent: the finding is made from
statuses alone. The file is read the first time a model is used in a process and written once, when
the process has found the model to refuse the ask. A file that is missing, larger than 64 KiB, not a
regular file, or not this shape holds nothing, and the next finding replaces it. No state directory,
or a write that fails, leaves the model to be asked as it was before this. The level, the caching and
the other concessions [BACKEND-22](#BACKEND-22) describes are not kept, and each process learns them
again.

An incognito session reads the file and writes nothing ([INCOG-5](incognito.md#INCOG-5)). The file is
written readable by its user only, like the rest of the directory ([STATE-1](state-directory.md#STATE-1)).

**Built:** `bravebot_agent::bedrock_refusals` (`Store`, `around`), which brackets a streamed request in
`Backend::complete_streaming`; `bravebot_bedrock::known_to_refuse_showing_thinking`.

**Not built:** keeping any other concession, and a way to forget one entry short of deleting the file
or waiting out the period.

**Why.** An inference-profile ARN does not name the model behind it, so a model is told from another
only by asking. Measured on Bedrock on 2026-10-28 with the field alone: Claude Opus 5.5 takes
`thinking: {"display": "summarized"}`, and Claude Sonnet 5.5 and Claude Haiku 4.5 answer 400
`thinking.type: Field required`. No `type` suits all three: `adaptive` turns thinking on for Sonnet,
which BACKEND-46 never does, and Haiku refuses it. A refusal kept only for the process is paid for
again in every new process, as a refused request at the start of the session and a diagnostic log for a
one-shot run that otherwise worked. Thirty days bounds how long a model that starts reading the ask
goes without it, which costs the silence BACKEND-46 describes only for a model that thinks before it
writes.

`verified-by: bravebot_agent::bedrock_refusals::a_refusal_kept_by_one_session_is_read_by_the_next_for_that_model_only`
`verified-by: bravebot_agent::bedrock_refusals::a_refusal_is_believed_for_thirty_days_and_not_after`
`verified-by: bravebot_agent::bedrock_refusals::a_stamp_in_the_future_is_believed_for_the_period_and_no_longer`
`verified-by: bravebot_agent::bedrock_refusals::a_file_that_is_not_this_record_holds_nothing_and_is_replaced_by_the_next_finding`
`verified-by: bravebot_agent::bedrock_refusals::the_file_keeps_the_newest_sixty_four_models_and_none_that_expired`
`verified-by: bravebot_bedrock::lib::a_model_an_earlier_session_found_refusing_the_ask_is_not_sent_it`
`verified-by: bravebot_agent::bedrock_refusals::what_an_earlier_session_kept_is_put_back_before_the_request_and_not_written_again`
`verified-by: bravebot_agent::bedrock_refusals::what_the_request_found_is_kept_after_it_and_once_for_the_model`
`verified-by: bravebot_agent::bedrock_refusals::a_request_that_found_nothing_keeps_nothing_and_returns_its_result`
`verified-by: bravebot_agent::bedrock_refusals::with_no_directory_the_request_runs_and_a_later_request_still_keeps_the_finding`
`verified-by: bravebot_agent::bedrock_refusals::a_streamed_request_puts_back_what_was_kept_and_keeps_what_was_found`
`verified-by: bravebot_agent::incognito::no_bedrock_refusal_is_written_down`
`verified-by: bravebot_agent::incognito::a_bedrock_refusal_an_earlier_session_kept_is_still_honoured`

<a id="BACKEND-64"></a>
### BACKEND-64: a start waits on no service, a lookup for one model on the service that answers it, and a picker on the slowest service

Nothing here changes which models are offered or which service answers one. It bounds how long a
person waits on the services that say what is offered.

**A lookup for one model's row asks the service that answers that model.** The window a session
opens on, the window a one-shot run takes, the window of the model a `/agent` line names and the
check that a model reads structured output each need one row. They ask the service
[BACKEND-3](#BACKEND-3) says answers the model and no other: the gateway whose id a qualified name
begins with, or whose block lists a bare name; the ChatGPT or xAI account; or Brave's endpoint for a
name no other service claims. A model an AWS account serves asks no one, since its rows come from the
configuration. `Config::serves` is the one function that says which, and the router takes its answer
from it, so a lookup cannot leave out the service a turn on that model goes to. A picker is not a
lookup for one model and asks every service.

**A roster asks its services at once.** The gateways whose blocks name no models, Brave's endpoint and
the two accounts are asked together, and a person waits for the slowest of them and not for all of
them in turn. The rows are offered in the order [BACKEND-4](#BACKEND-4) gives, and the sentences about
a listing that could not be read are said in the order [BACKEND-19](#BACKEND-19) gives, whichever
service answered first.

**A request for a roster is given up on in seconds.** Resolving takes at most 3 seconds, connecting 5,
sending 5, the reply to begin and then to finish 10 each, and a gap in the reply 5, where a model's
reply is allowed minutes. A service that does not answer in that time is a listing that could not be
read, and is said as [BACKEND-19](#BACKEND-19) says.

**The window of the model a session opened on does not hold the first frame.** It is asked for on a
thread of its own when the session is begun, and adopted, with the note that says the budget moved,
on the pass of the loop it arrives. A turn or a manifest run sent before it has arrived waits for it,
for as long as a request for a roster may take, so that no turn is sized against the default budget
for want of a lookup that was on its way. A turn waits for a lookup once: one it has waited out stays
for the loop to take when it arrives, and the next turn does not wait for it again. A lookup about a
model that is no longer the one in force is dropped before anything waits for it, because the pick
that replaced that model brought a window of its own ([BACKEND-7](#BACKEND-7)). A one-shot run has no
frame and takes its one lookup before it begins.

**Built:** `bravebot_config::Config::{serves, roster_scope_for_a_turn}` and `RosterScope`;
`bravebot_aichat::roster::ask`, which decides which gateways and whether Brave are asked, makes each
request through `bravebot_aichat::models::{concurrently, listing_egress}` and gives the entries back in
the configuration's order, and which the roster of the terminal (`bravebot_tui::app::models::roster`) and
of the window (`bravebot_ui_bridge::models::roster`) both take their answers from; the listings of the
two accounts (`bravebot_agent::chatgpt::account_models` and `bravebot_agent::xai::account_models`), which
ask through the same egress; `ask_for_the_budget_of_the_model_in_force` and `adopt_the_awaited_budget`
in the terminal.

**Not built:** rows of a picker appearing as each service answers, and a roster kept from one opening of
the picker to the next, so the picker still waits for its slowest service each time it opens. The
window of the model a session opened on is still looked up on every start.

**Why.** Each service that says what is offered is a network request, and a person waits on all of
them. Measured on 2026-10-28 against stand-in gateways on this machine, with six gateways that name no
models and four of them taking 0.5 s to answer, asking them one after the next takes 2.3 s for a start
to draw and 2.3 s for `/model`, the sum of their latencies. Asking them at once takes 0.54 s for
`/model`, the slowest gateway's 0.5 s, and a start on a model served by one gateway draws in 0.01 s
because it asks only that one. A gateway that takes the connection and never answers holds a request
for as long as a reply is allowed, 600 seconds unless the request states a bound. The model names the
service ([BACKEND-3](#BACKEND-3)), so a lookup about one model has no use for the rosters of the
others.

`verified-by: bravebot_config::lib::a_roster_for_a_model_asks_the_service_that_answers_it_and_no_other`
`verified-by: bravebot_tui::app::a_lookup_for_one_models_row_asks_only_the_gateway_that_serves_it`
`verified-by: bravebot_tui::app::a_lookup_asks_brave_only_for_a_model_brave_answers`
`verified-by: bravebot_ui_bridge::models::a_turn_in_the_window_asks_only_the_gateway_that_serves_its_model`
`verified-by: bravebot_aichat::models::the_asks_of_a_roster_are_in_flight_together_and_answered_in_the_order_given`
`verified-by: bravebot_aichat::models::a_panic_in_an_ask_is_raised_to_the_caller`
`verified-by: bravebot_tui::app::the_picker_asks_every_gateway_at_once_and_lists_them_in_the_configurations_order`
`verified-by: bravebot_ui_bridge::models::the_window_asks_every_gateway_at_once_and_lists_them_in_the_configurations_order`
`verified-by: bravebot_aichat::models::a_request_for_a_roster_is_given_up_on_in_seconds_and_not_minutes`
`verified-by: bravebot_aichat::roster::a_roster_gives_up_on_a_gateway_that_takes_the_connection_and_never_answers`
`verified-by: bravebot_aichat::roster::the_entries_are_in_the_configurations_order_and_each_kind_of_gateway_is_told_apart`
`verified-by: bravebot_cli::window_lookup::a_session_draws_while_its_window_is_asked_for_and_takes_it_when_it_comes`
`verified-by: bravebot_cli::window_lookup::a_turn_sent_before_the_window_arrives_waits_for_it`
`verified-by: bravebot_tui::app::asking_for_the_window_at_start_adopts_it_only_when_the_answer_is_taken`
`verified-by: bravebot_tui::app::a_pass_of_the_loop_takes_an_arrived_window_once_and_does_not_wait_for_one_that_has_not`
`verified-by: bravebot_tui::app::a_lookup_about_a_model_that_is_no_longer_in_force_is_dropped_and_not_waited_for`
`verified-by: bravebot_tui::app::a_turn_waits_for_a_lookup_once_and_the_loop_still_takes_its_late_answer`
`verified-by: bravebot_tui::app::a_turn_waits_for_the_window_it_is_sized_against_for_a_bounded_time`

<a id="BACKEND-65"></a>
### BACKEND-65: a Vertex AI project with no key is signed in by the Google Cloud CLI

Where a request goes to a `google-vertex` service that a project builds ([BACKEND-49](#BACKEND-49)), at
a Google Vertex host over `https`, and no key resolves ([BACKEND-50](#BACKEND-50)) and the block states no
`authorization` header, bravebot asks the Google Cloud CLI for an access token with
`gcloud auth application-default print-access-token` and sends it as `authorization: Bearer <token>`.
A key that resolves is sent as [BACKEND-51](#BACKEND-51) says and the program is not started. Express
mode, and an entry whose `baseURL` is not a Google Vertex host, are never sent a token.

The environment names the service by `GOOGLE_CLOUD_PROJECT` alone ([BACKEND-50](#BACKEND-50)). The
program is `gcloud`, found on `PATH` without the directories a stage may write, started with fixed arguments and
this agent's own credentials taken off its environment, and no value from a settings file, a model or the
workspace reaches it ([BACKEND-1](#BACKEND-1), [SANDBOX-30](sandboxing.md#SANDBOX-30)). It is not named by
a file. `GOOGLE_APPLICATION_CREDENTIALS` is read by the CLI and not by bravebot.

The sign-in is asked for before work starts, and only for a model this service answers
([BACKEND-9](#BACKEND-9)): where the CLI gives no token, `gcloud auth application-default login` is run and
each line it writes is shown as it arrives. A CLI that is missing, or that gives no token, ends the request before
anything is sent, naming `gcloud auth application-default login`, and the output it printed is not quoted.
The token is in no trace, no error and no session record.

**Why.** An organisation can disallow API keys for a project, and a person who is signed in with `gcloud`
has no key to give. The CLI resolves the credential file, impersonation and refresh, which are Google's to
change, and the token is a bearer at the host the project and location build, which is a destination
somebody reviewed ([routing.md](routing.md)). A `baseURL` that names a proxy would send the person's
Google sign-in to a host the program did not compile in, so it is sent a key or nothing.

No answer is kept, where [BACKEND-10](#BACKEND-10) keeps one until near its expiry: the command
states no expiry, and an answer without one is not kept. A token is asked for with each request and the
check before a turn is one more run of the program.

`verified-by: bravebot_config::provider::only_a_project_reached_at_a_google_vertex_host_takes_the_google_cloud_token`
`verified-by: bravebot_config::provider::a_key_or_an_authorization_header_of_the_blocks_own_leaves_no_need_for_a_token`
`verified-by: bravebot_config::lib::a_project_alone_names_the_service_the_google_cloud_cli_signs_in`
`verified-by: bravebot_aichat::lib::a_google_vertex_access_token_is_sent_as_a_bearer_and_not_in_the_key_header`
`verified-by: bravebot_agent::backend::a_vertex_project_with_no_key_is_sent_the_token_the_cli_gives`
`verified-by: bravebot_agent::backend::a_vertex_project_with_a_key_never_asks_the_cli`
`verified-by: bravebot_agent::backend::a_refusal_by_the_cli_ends_the_request_naming_the_cli`
`verified-by: bravebot_agent::backend::only_a_vertex_project_at_a_google_host_with_no_other_credential_is_given_the_cli_token`
`verified-by: bravebot_agent::backend::the_cli_sign_in_is_due_only_for_a_vertex_project_with_no_key`
`verified-by: bravebot_cli::doctor::a_vertex_project_with_no_key_is_reported_as_signed_in_by_the_google_cloud_cli`
`verified-by: bravebot_agent::gcloud::the_token_is_asked_for_with_the_application_default_credentials_command`
`verified-by: bravebot_agent::gcloud::the_program_is_given_the_application_default_credentials_arguments`
`verified-by: bravebot_agent::gcloud::a_token_the_cli_prints_is_returned_without_its_newline`
`verified-by: bravebot_agent::gcloud::a_refusal_reports_the_first_line_of_stderr_and_never_a_token_it_printed`
`verified-by: bravebot_agent::gcloud::a_reply_that_is_not_one_token_is_refused_and_not_quoted`
`verified-by: bravebot_agent::gcloud::a_gcloud_a_stage_may_write_is_not_the_one_run`
`verified-by: bravebot_agent::gcloud::the_sign_in_reports_each_line_the_cli_writes_and_its_failure`

## Known costs

- **Each change of mode costs a full prefix write.** BACKEND-27 and BACKEND-32 mark a cache prefix
  per request and a cache belongs to one model, so a turn that begins on the plan model after acting
  turns on another, and the next acting turn after it, each write the whole prefix again. A person
  who sets `planModel` is choosing that cost for the stronger planner.

- **A one-shot run in plan mode compacts against the session model's window.** The window is looked
  up once at startup for the model in force, and `planModel` is chosen per turn, so a plan model with
  a smaller window than the default can be sent a request over it. An interactive turn looks the
  window up for the plan model.

- **A plan turn is not on the model the person picked.** `/model` and the startup `--model` of an
  interactive session are outranked by `planModel` in plan mode, which is what the setting is for.

- **A fallback no service offers is tried anyway.** Where the name belongs to no gateway and no AWS
  account, the aichat endpoint is the service that would answer for it, as it is for any such name
  (BACKEND-3), so a mistyped `fallbackModel` passes BACKEND-53's same-service test. The turn moves to
  it, the endpoint refuses it, and the failure that ends the turn is that refusal rather than the
  overload that preceded it.

- **A Bedrock model that starts reading the ask to show thinking is not asked for up to thirty
  days.** BACKEND-63 keeps a refusal by the model's name, and nothing tells the program that a
  model behind the same name has changed. Within the period a model that thinks by default and now
  takes the ask goes without it, and a long silence while it thinks can be cut by the egress layer
  (BACKEND-46). Deleting `bedrock-refusals.json` asks again at once. The file also holds the
  inference-profile ARN, which includes the AWS account number, in a file only its user can read.

- **Every other concession is learned again by each process.** BACKEND-63 keeps the ask to show
  thinking, the one refusal a request to a Sonnet or Haiku model meets without anyone having chosen
  anything. A Bedrock model that refuses the effort level, the caching or the assumed ceiling costs a
  refused request in each process, as does a refusal learned by any other client, all of which are
  kept for the process only (BACKEND-22). The level is a choice a person made and is reported as not
  in force once refused, and the caching and the ceiling are refused only by models that lack them.

- **The refusal is made at startup, and a model chosen mid-session is not checked again.**
  BACKEND-39 is asked once, before a session opens, so somebody who opens on a configured model and
  then picks one of Brave's from `/model` keeps it for the rest of that session.
  Checking per turn would put a file read and a refusal in front of every round, and the choice
  there is a person naming a model from a list that says which service answers it (BACKEND-6),
  which is the opposite of the case the clause exists for: nobody arrives at it without knowing
  what they picked.

- **A recorded tier word follows the configuration of the checkout it is read in.** A `sonnet`
  recorded where the tier variables were exported names Brave's Sonnet, or the account's first
  model, where they are not. Two tier variables naming the same ARN record the first tier in
  Opus, Sonnet, Haiku order, whichever row was picked. A pick recorded before the tier word was
  written is an ARN, and is set aside once nothing serves it.

- **A pick that is set aside is named at every start.** The record is kept (BACKEND-47), so each
  start says it set the pick aside until another model is picked with `/model`. Somebody who only
  makes one-shot runs has no `/model`, and sees the line until they remove `~/.bravebot/model`.

- **Which model answers is the individual's, and a managed layer cannot pin one.** A pinned default
  model would lose to a `/model` choice wherever one is in force, so it would pin nothing, and
  making that choice unavailable is a change to what a person is offered rather than to where a
  request goes. An organisation with a reason to care, a cost or a data-handling consequence attached
  to one model, bounds the set with BACKEND-48 and chooses out of it with nothing: the person at the
  machine picks any model the lists allow.

- **A refused model is named exactly, so a renamed one is requested again.** BACKEND-48 compares a
  name and holds no pattern language, which is the reading [SERVERS-12](mcp-servers.md#SERVERS-12)
  gives a command: a near-match is a thing to get wrong in the direction of allowing what was meant
  to be refused. So a deny list is only as good as the names in it, and an organisation that wants a
  bounded set writes the allow list, which is the half that cannot be got around by a name nobody
  thought of. A gateway whose roster grows is the case this costs most: a new slug reaches a machine
  whose file only denies.

- **The refusal is made where a model is settled, and a model picked mid-session is not checked
  again.** The same cost BACKEND-39 carries and for the same reason: `/model` offers only what the
  machine may request, so the only way to a refused model mid-session is a build that listed one.

- **Asking for no level does not outlive the session against a file that names one.**
  [SESSION-15](sessions.md#SESSION-15) removes the record rather than writing an empty one, because
  absence and a chosen absence were the same request while nothing else could name a level. With
  BACKEND-43 they are not: somebody whose own settings say `high` and who picks no level is asking
  for none, and the next session reads the file and asks for `high` again. Distinguishing the two
  needs a recorded absence, which is a change to what that clause writes down and to what reads it,
  and the level is a preference somebody re-picks in one keystroke rather than an effect. Against a
  checkout's file a recorded absence would change nothing, since that file outranks any record.
  `/effort` is what says so for the session in front of them, `--effort` for one run, and the file
  is what says so for every session.

- **How hard a model thinks is not something an administrator pins.** The machine-level layer reads
  the names that decide where a request goes, and a level decides what a request costs and how long
  it takes at a destination already settled. An organisation with a reason to care about the bill has
  the endpoint and the account to say it with, and a layer that could pin this is a layer somebody
  uses to pin a preference, which is the argument BACKEND-38 makes about the theme.

- **The layer binds nobody who can write the file, and what that takes differs per platform.** Its
  whole authority is the permissions on the path. On a machine whose user is also its administrator,
  which is most machines this is installed on, a pin is a note to self, and checking an owner would
  not change that, the same account being able to replace the binary. Where it is thinner than that
  argument assumes is a shared machine: `/etc` is root's, but `/Library/Application Support` is
  writable by the admin group, and a `C:\ProgramData` subdirectory nobody has created yet can be
  created by any authenticated user, who could then pin another account's endpoint. So the layer is
  worth what the directory's permissions are worth, and on those two platforms an administrator has
  to create the directory with permissions of their choosing rather than leave it to this program,
  which never creates it.

- **On Windows the path names one drive.** `C:\ProgramData` is where it looks, because the variable
  that would say otherwise is the person's own to set, so a machine whose system drive is elsewhere
  has no managed layer at all rather than one that can be redirected.

- **Three fields in a Bedrock request are ones a single provider defines.** The body Bedrock states
  for every provider it hosts has no field for how hard to think, for how thinking is shown or for
  tool arguments as they are written, so the level, the ask to show thinking (BACKEND-46) and the
  ask for arguments (BACKEND-44) travel in the field that service hands to the model without
  reading, spelled the way the Anthropic API spells them. A tier naming a model from another
  provider is reachable and answers, and a request carrying any of them is refused by that model,
  costing the first streamed request one refused request for each of them it carries before the
  model's answer is remembered. Nothing here can tell the two apart, an inference-profile ARN not saying
  which provider serves it, and the alternative is withholding all three from every Bedrock model
  including the ones that read them.

- **A model that refuses the ask to show thinking and thinks for more than two minutes before it
  writes is cut off as a request that did not get through.** Claude Sonnet 5 and Claude Haiku 4.5
  refuse the ask (BACKEND-46), measured on Bedrock, wanting to be told whether to think as well as
  how, and asked nothing Sonnet at the level `high` sent nothing for 22 seconds before the start of
  its message. The egress layer's gap bound ([NET-5](network-egress.md#NET-5)) ends such a silence
  at two minutes, or at the gap the account's block states ([BACKEND-55](#BACKEND-55)), and a
  reply ended before any of its body arrived is not sent again
  ([BACKEND-37](#BACKEND-37)). Telling those models whether to think would close it, and would
  decide for them something that is now their own default; Haiku refuses being told to decide for
  itself. A model that refuses only a later concession, the level, the ceiling or the ask for
  arguments, gives up the ask to show thinking with it, since it goes first; none measured does.
  And a model that does show its thinking holds its output count still while it thinks, the
  thinking being read past rather than tallied, so the count BACKEND-45 moves stands still for as
  long as a think lasts while the connection under it stays busy.

- **Which models a product is served is the service's decision, and this holds no copy of it.** The
  roster is whatever the endpoint returns for `bravebot`, so a model becoming unsuitable for agentic
  work is a change nothing here would notice, and one wrongly dropped from the curated set is a model
  a person cannot pick however well it would have worked. The alternative is a list of names compiled
  in, which BACKEND-12 already declines for the tier words and for the same reason: the service owns
  the names, and a copy here is a copy that goes stale.

- **A layer a checkout carries is trusted as far as the person's own file is, except where it is
  not.** A `.bravebot` directory arrives with whatever produced the checkout, so a `settings.json` in
  one arrives unasked. The fields that name a destination and the credential that signs it
  (`provider` and `model`, [BACKEND-24](#BACKEND-24)) are refused from the project layers outright,
  because the credential value comes from the person's own environment under names the layer would
  choose, and a file nobody read must not pick either. The `env` names that choose the account a
  request goes to are refused from them too: the six network names, the three Google names and the
  six Bedrock names ([BACKEND-33](#BACKEND-33)). For the remaining fields, what limits the
  damage is the same rule that limits it anywhere: a file names a destination and grants no
  capability, so the worst it does is send a request somewhere useless or somewhere watching.

  The one field that is not a destination is `attribution`, whose value [BACKEND-30](#BACKEND-30)
  states to the planner. It is free text that reaches the planner's context on every round, so it is
  refused from the project layers outright, as `references` is, and reported by `doctor`. A checkout
  cannot say what its own history carries; the person says it once in their own file.

- **A region refused for what it holds is not reported.** A Bedrock region outside the characters of
  [BACKEND-33](#BACKEND-33), in an `env` block or in a `provider` entry, configures no account as an
  absent one does, and nothing names the value that was refused: `doctor` lists the names a file set,
  so an account missing from its output is the only sign, and a person who mistyped a region has the
  other backends in force. Importing from Claude Code ([IMPORT-3](import.md#IMPORT-3)) writes the
  region the file states, so one this refuses is imported and then not read, and an `amazon-bedrock`
  entry from opencode with one is left with the reason that it has no region.

- **The aichat endpoint Brave runs discards the effort level.** Measured against that endpoint: a
  nonsense value in `reasoning_effort` is answered `200` with usage identical to a request that omits
  the field, so it is not validated, and on `near-glm-5`, which reports a non-zero reasoning-token
  count for an ordinary prompt, that count does not move with the level. The premium rows of the
  roster were not measured, a request without the premium key being substituted to a weaker model
  before the request lands, so nothing here is established about them. A level chosen against a Brave-served
  model is therefore carried, sent, and dropped, while the interface goes on reporting it as in
  force. Bedrock is unaffected, the level reaching the model in the field that model defines.

- **A level a service does advertise may still not mean what this sends.** `xhigh` and `max` are
  levels the Anthropic API defines, and a gateway row advertising `reasoning_effort` says it reads
  the parameter without saying which words it accepts. A model may reject or silently round a level
  it does not know, and no listing distinguishes that from honouring it.

- **A blank exported variable reaches the file for the model and for nothing else.** BACKEND-11's
  resolution treats a blank as absence the whole way down, so the `env` block's spelling still
  answers on a binary built with nothing, and a `model` key outranks the variable whatever it holds.
  BACKEND-35's stops at the build: on a binary built with nothing, exporting a name blank leaves the
  configuration holding the blank and the value in the file unread. The two orders are the same
  argument, that a placeholder in a shell profile is not an instruction to discard anything, applied
  to one more source in one of them than in the other, and which behaviour a person meets depends on
  which name they blanked.

- **A gateway outside the known names that wants a credential and was told of none is refused by the
  service rather than here.** BACKEND-16 reads a block naming no credential as the person saying none
  is wanted, except for a known name at its compiled-in host, which is refused locally. A block stating
  its own `baseURL` is read by what it says, since the endpoint does not answer whether a credential
  is wanted: a private deployment behind a known name, or a local service reached at one, may want
  none. A block whose `apiKey` is written blank, or whose `env` lists only blank names, names no
  credential by that same reading, since a blank in this file is read as nothing having been written
  there. A placeholder somebody meant to fill in later at a stated endpoint is therefore read as them
  saying none is wanted, and the refusal that would have pointed at it does not happen.

- **A credential is resolved by running the AWS CLI.** Reaching Bedrock needs short-lived keys that
  expire during a session, and the tool that holds them is the one the person already signs in
  with. That is a process this code did not write, reading a configuration this code does not
  govern.

- **A planning round marks a conversation the round after it cannot read.** Planning a manifest is
  two requests over one growing conversation, so the second does send the first's messages again, and
  it sends them behind a different set of instructions. A cached prefix is matched from the start of
  a request, so a prompt that changed leaves nothing after it matchable, and the mark on the end of
  the first round's conversation is a write nothing reads. The second round is the last one, so its
  own mark buys nothing either. Saying so is the caller's to do, as it is everywhere else, and
  [manifest.md](manifest.md) is where it would be said. What it costs is two writes over a short
  conversation, a planner carrying the prompt a person typed and one reply rather than a session.

- **Compaction throws away the cache of the conversation.** A summary replaces the messages in front
  of the last few, which is a rewrite of the prefix the rolling breakpoint sits in, so the round
  after a compaction reads none of the conversation back and pays a cache write to establish the
  shortened one. The prompt's breakpoint covers a prefix a compaction does not touch and survives it,
  which [compaction.md](compaction.md) is where to read. Giving the rest up is the right way round,
  the point of compacting being that the old conversation is no longer worth sending at all, but it
  means the sessions that benefit most from caching are the ones that periodically lose part of it,
  and a session compacting often enough could write more than it ever reads. Nothing here measures
  that: BACKEND-31's figures are per turn, and the turn that compacted is charged for the write in
  the same figure as the rounds that profited from it.

- **An ephemeral cache entry expires on inactivity, and nothing here tracks it.** The lifetime is
  the provider's default of a few minutes unless the `promptCacheTtl` setting chooses one (BACKEND-27,
  BACKEND-32), and the longer one is charged at a higher write rate, which is why nothing is sent until
  a person chooses. A person who thinks between turns misses more often than the token arithmetic
  suggests, and a miss looks identical to a service that reports nothing: the reply carries a zero
  read either way.
  Neither figure says when the last request was, so nothing can distinguish a prefix that expired
  from one that was never established.

- **The aichat endpoint Brave runs reports nothing about a cache, so BACKEND-31 measures nothing
  there.** That server does not answer `include_usage` with an OpenAI `usage` block at all: it sends
  a `brave-chat.contentReceipt` carrying `total_tokens` and `trimmed_tokens`, and neither says
  whether any of the prompt was served from a cache. Whether the service caches, and whether asking
  it to would be worth anything, is therefore not a question this code can answer from a reply, and
  both figures stay at the zero BACKEND-31 defines as silence. The field
  `prompt_tokens_details.cached_tokens` is read for the gateway case, where a server states it,
  which costs a field and settles nothing about Brave's own endpoint. What BACKEND-32 sends that
  endpoint is inert rather than merely unmeasurable: its request model declares no `cache_control`
  field and it rebuilds every message from the one it parsed, so a breakpoint arrives and is dropped
  before anything upstream could read it, and the service places checkpoints of its own on the
  prefix instead. Observed in `brave/aichat` at `ca969f7d` and asked about there in issue 1806. What
  the mark costs that endpoint is its own bytes and nothing else, both tiers answering a marked body
  exactly as they answer an unmarked one. Nothing in BACKEND-32 rests on this holding: a service
  that begins reading the field gets what it asks for, and one that refuses it is the paragraph
  above.

- **A refusal remembered from one status may not have been about the breakpoints.** A gateway
  answers an invalid-request status for reasons of its own, an upstream failure it reports as one
  among them, and a retry that succeeds makes that indistinguishable from a service refusing the
  shape BACKEND-32 sends. What a wrong reading costs is caching for the rest of the process against
  a service that would have cached, which is the cost of not asking rather than a failed request.
  The alternative is asking again after every such status, which is the round trip the memory
  exists to spend once.

- **The request BACKEND-32 sends again is outside the retry budget, and counts as a premium request.** It is
  not a retry: nothing failed in transit, and the same body with a field removed is a different
  request. So it does not count against the attempts a failure gets, which means the worst case
  is one request more than the count allows, four rather than three where none is stated. On the
  premium host it also asks the key source once more, so a limit in credits counts it as one more
  premium request ([PREM-12](premium-credentials.md#PREM-12)). That is once per service and model per
  process, and
  the alternative is paying for the whole prompt on every request instead.

- **A gateway credential may be a plaintext string in the settings file.** The shape this block
  borrows has a field for one, and taking the shape means taking the field. Naming a variable is
  recommended and preferred where both are present, but nothing prevents the other, and the only
  real fix is a credential store this does not have.

- **A gateway's pass-through options are unvalidated.** A misspelled routing field is a request the
  gateway rejects, or worse one it silently routes somewhere unintended. The alternative is a schema
  that goes stale as the gateway changes, and that trade is what keeps this from being support for
  one particular gateway.

- **Fields another tool defines are read past in silence.** Somebody who knows the shape will expect
  its cost, modality and package fields to do something here, and they do nothing. That surprise is
  the price of a block that can be copied in either direction.

- **The assumed AWS window is a guess.** No endpoint there reports a context window, and an
  inference-profile ARN does not say which model it resolves to, so one figure stands in for every
  tier: the one an unresolvable profile actually gets. It is deliberately low, because being wrong
  upward removes shortening rather than delaying it.

- **The assumed reply ceiling is a guess that a refusal corrects in one direction only.** A model
  allowing more than 32,000 tokens still stops there until somebody states a better figure, and one
  allowing less costs a refused request per process before it is sent the fallback, which then
  holds for it however much it allows between the two. A request refused for any other reason, a
  prompt too long for the model among them, costs one more request for the ceiling as well, and
  where giving up something else afterwards is what got an answer, the ceiling is remembered as
  refused beside it, the status naming no field: that model's replies stop at 8,192 for the rest of
  the process. So do they after a prompt close enough to the model's window that the assumed ceiling
  does not fit beside it, which the fallback does, since nothing tells that refusal from a model
  allowing less; with the default context budget a prompt that long is compacted first. What a ceiling that is too low costs is the tail of an answer rather than the answer,
  since BACKEND-42 keeps what was written, but it is still an answer that stops short for a reason
  belonging to this program rather than to the model.

- **A reply that stopped short reads like one that finished.** BACKEND-42 keeps the text, and text
  is all it is: the sentence ends wherever the ceiling fell. What says otherwise is a line beside
  it, which somebody reading only the answer does not have to notice.

- **Vertex is reached by key alone, and a block does not read the environment's project.** A
  `provider.google-vertex` entry takes its project and location from itself, so somebody with
  `GOOGLE_CLOUD_PROJECT` exported, as opencode reads it, still states a project in the block, and a
  block that states none is reached in express mode whatever the environment holds.
  Signing in with Google Cloud credentials, a service account or application default credentials
  is not supported; a key is the one credential this reaches it with.

- **`GOOGLE_API_KEY` is a name other tools read.** The route BACKEND-50 describes sends the key
  somebody exported for another Google client to Vertex, once they have chosen a model that names it.
  A key scoped to another Google service is refused there, which is a failed request rather than a
  leak, since the host is Google's own. The key alone names the service, so exporting it for any
  other Google tool also takes it off the environment of the programs this agent starts.

- **The compiled Gemini list is unchecked against express mode.** Each id on BACKEND-49's list is
  one the OpenAI-compatible endpoint answers, and express mode is assumed to serve the same ids under
  the native API without that having been checked for each, so a row that express mode does not
  serve fails when picked. A block naming models replaces the list, and any model is reachable named qualified.

- **Express mode reports no thinking to `/thinking`.** The service returns a summary of the model's
  reasoning only when the request asks for one, and this build does not ask, so the screen has none
  to draw for a Vertex express reply and the reasoning is known only from its token count.

- **Express mode has no region and reads no location.** Google chooses where a request is served, so
  a person who needs a regional endpoint states a project, which is reached through BACKEND-49.

- **Express mode states the level as a budget on every Gemini model.** The newer models read a named
  level of their own and treat a budget as the older form of it, which may not rank the five levels
  as that model would. A model that refuses the budget is sent none, as BACKEND-22 says.

- **A schema and tools in one request is the service's to accept.** The request carries both where a
  task states a schema and the turn offers tools, and a model that refuses the pair refuses the
  request.

- **The compiled Gemini list goes stale between releases.** A model Google retires stays offered to
  a `google-vertex` service until a release takes it off BACKEND-49's list, and a model Google adds is
  absent until a release puts it on. Either costs a shortcut and not a capability: any model is
  reachable named qualified, and a block naming models replaces the list.

- **The compiled Gemini list is what the `global` location serves.** An entry naming a regional
  `location` is offered the same list, and a model on it that the region does not serve fails when
  picked. A block naming that region's models replaces the list.

- **The name `google-vertex` means Vertex's behaviour at any endpoint.** An entry keyed `google-vertex`
  sends its key in `x-goog-api-key`, is offered the compiled list rather than asked for a roster,
  and sends `extra_content` back, even where its `baseURL` names a proxy. A proxy that wants a
  bearer token is reached by keying the entry with another name, which gets the behaviour of any
  other gateway.

- **An opencode import does not carry a `headers` block across.** The importer copies `baseURL`,
  and `project` and `location` on a Vertex entry, and writes nothing else under `options`
  ([IMPORT-6](import.md#IMPORT-6)), so a block imported from `opencode.json` gets its host and its
  credential and whoever imported it writes the headers again. Copying them would mean importing a
  value from a file as a header on every request, which is the decision BACKEND-60 keeps in the
  person's own layers; a block pasted by hand is the person's own act and is read whole.

- **A header in the managed layer is a value every account on the machine can read.** The layer
  drops `options.apiKey` for that reason and keeps `headers`, because a header is how that layer
  names a tenant or a route and dropping it would leave the gateway it approves unreachable. So a
  header value written into `managed.json` directly, rather than as `{env:NAME}`, is readable by
  anybody on the machine, and one holding a second credential is that credential handed to every
  account. Naming a variable is the form that avoids it.

- **An `aws` that lives only in a directory a stage can write is not found.** The same holds for
  a bare hook program, a language server and the Ctrl-G editor, which read as not found, not
  installed and not configured. The AWS CLI is
  started unconfined, so [SANDBOX-30](sandboxing.md#SANDBOX-30) searches only the `PATH` entries a
  confined stage cannot write. Somebody whose only `aws` is in an activated virtualenv, or in the
  session's own directory or the temporary directory, gets `NotInstalled` until they install it
  elsewhere or open the session from a directory that does not hold it. A session opened on a
  directory that holds the install, `~/.local/bin` under a session opened on the home directory
  among them, is the same case. The same holds for a
  directory a toolchain was granted writes in once a stage has run with it, so a CLI installed under
  such a cache is found before the first such stage and not after. Where the permission mode
  answers every question and the sandbox mode is workspace-write, a stage is given every path the account
  can write, so an `aws` in a directory the account owns, such as a Homebrew prefix, is found before
  the first stage runs and not after, and every Bedrock request from then until bravebot exits gets
  `NotInstalled`. The CLI is also started with a `PATH` of those same entries, so a program it starts
  by name, the interpreter of an `aws` that is a script or a bare `credential_process` command, is
  not found by it in a directory left out.
  An editor found outside those directories still starts, with a `PATH` of the searched entries
  alone, so a program it starts by name that lives only in a directory left out is not found by it:
  a tool a plugin runs from a project's `node_modules/.bin`, or one in `~/.local/bin` under a
  session opened on the home directory. A program the editor leaves running keeps that `PATH`.
  A clipboard tool, `osascript` included, is given the same `PATH`, so a launcher among them that
  starts a program by name finds it only among the searched entries. A tool that then fails passes
  for one that is absent: the next tool is tried, and with none left a copy goes out as the
  terminal's own escape sequence and a paste reads as an empty clipboard.

- **An `openai` entry naming no models is offered everything OpenAI's listing returns, less a
  model past its `shutdown_date` and one compiled-in list.** The listing states an id and an owner for
  each model and nothing about what it can do, so BACKEND-19 has nothing to drop on for a capability,
  and the picker holds embedding, speech and image models beside the chat ones. That waives
  BACKEND-5's promise that picking a listed name works, for the rows of this one service. A model
  whose listed `shutdown_date` is today or earlier is left out, and a row with no date or a date in
  another shape is offered. The ids of the compiled list are those of the `ResponsesOnlyModel` list of
  OpenAI's published API description and of the model pages that mark chat completions unsupported
  and Responses supported ([BACKEND-19](#BACKEND-19)), and they are left out only where the route is
  chat completions at `api.openai.com`, which an `openai` entry reaches only by naming
  `@ai-sdk/openai-compatible`. The list goes stale with each release of either. Filtering by anything else
  an id says would be a guess about OpenAI's naming, and a model named otherwise would be hidden
  from somebody who could use it. A list of the chat models compiled into the binary, as BACKEND-49
  compiles one for Vertex AI, would go stale the same way, and each id on it would first have to be
  answered with a tool call and its result by the service. A block naming `models` is offered those
  and no others.

- **A model the chat completions route does not serve is offered and fails when picked.** The route is chat
  completions where an `openai` entry names `@ai-sdk/openai-compatible`, and the listing does not say which
  models take either.
  The compiled list holds the ids the description and the model pages read on 2026-10-27 name, so a model
  released since, or one whose page states no endpoints, is offered, and so is `gpt-image-1-mini`, which the route
  refuses as Responses-only. The list holds ids the route answers as deprecated or unknown as well: of its 28 ids the
  route named the Responses API for 3, answered "not a chat model" for 6, "deprecated" for 13 and "does not exist or
  you do not have access" for 6, and none answered 200. A model whose listed `shutdown_date` has passed is left out
  ([BACKEND-19](#BACKEND-19)): of the 36 ids of the listing of 2026-10-27 with a date on or before that day, 34
  answered that the model had been deprecated and two (`sora-2` and `sora-2-pro`) answered 503, and 4 ids with no
  date answered it as well and are offered. The figures below count the rows of the listing of 2026-10-27, those
  past their `shutdown_date` included. The route answered the same for `gpt-3.5-turbo`, `o1`, `o3-mini` and
  `o4-mini`, whose pages mark them as taking chat completions. The rows of embedding, speech, transcription, realtime, moderation, audio, image and video models are
  offered. Asked once each on 2026-10-27 with one short message and no `modalities`, the embedding ids answered 403 or
  404, the speech, transcription, realtime and moderation ids 404, five of the six audio ids 400 for want of an audio
  modality, and the image and video ids 500 or 503, which may not be about the route, except two image ids that
  answered 404. Of the 118 ids of the listing of 2026-10-27 that the compiled list leaves in, 40 answered 200 to a
  request with a ceiling of 16 tokens, 10 of them with an empty content cut at that ceiling, and 78 did not
  ([BACKEND-19](#BACKEND-19)). An `openai` entry that names no SDK is reached at `/responses`
  ([BACKEND-17](#BACKEND-17)), and nothing has been observed of what that route answers for the rows of
  embedding, speech, transcription, realtime, moderation, audio, image and video models, which the listing holds
  as well. A block naming `models` offers only the ones it names.

- **A cache write figure is stated by some of OpenAI's models and not by others.** BACKEND-31 reads
  `prompt_tokens_details.cache_write_tokens` where a reply states it. On 2026-10-27 eight of the 40 ids that answered 200 to
  a request with a ceiling of 16 tokens stated it (`gpt-5.6-luna`, `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-6-astra`,
  `gpt-6-luna`, `gpt-6-sol`, `gpt-6.1-sol` and `gpt-6.1-terra`) and the other 32 did not, although a prompt sent a second time was read from the
  cache by `gpt-4o-mini`, `gpt-4.1-mini`, `gpt-5-mini` and `gpt-5.4-mini` among them. A turn through a model that
  states none reports a write of zero and the status panel draws no write line for it, whatever was written.

- **The status panel names the `promptCacheTtl` setting after a turn that sent no lifetime.** The
  line reads the setting whichever route the turn took. A request to `api.openai.com` carries
  no lifetime ([BACKEND-32](#BACKEND-32)), and neither does one reached in the Responses protocol,
  the ChatGPT account's among them, so after a reply of either that reported cached tokens the line
  reads `5 minutes` or `1 hour`. The panel holds the model chosen for the next turn, and the turn
  that reported the figures may have run on another, so naming the route from it can be wrong
  after `/model`.

- **Each OpenAI roster costs a request that is refused.** BACKEND-19 asks what the credential may
  reach at `/models/user` before it asks for the whole listing. OpenAI answered it with 404 on
  2026-10-26 and on 2026-10-27, and its published API description has no such route. The listing is
  asked second.

- **A refusal of the `cache_control` markers of BACKEND-32 by a service keyed `openai` at another host
  costs one refused request.** A request to `api.openai.com` carries none. `gpt-4o-mini` and `gpt-5.4-mini` answered 200
  to a `cache_control` on a content part, so a request there that carried one was not refused on 2026-10-27. Whether a
  proxy keyed `openai` reads them is not known, so one is sent them, and a
  refusal with an invalid-request status is asked again without them, once for each model in each
  process. An accepted request is marked as BACKEND-32 says.

- **A level that one OpenAI model refuses takes every level from it.** The three reasoning models tried on
  2026-10-27 refused the levels they do not take with the status of any other refused body, and the program
  reads the status and not the body ([BACKEND-22](#BACKEND-22)). `gpt-5-mini` took `low`, `medium` and `high`
  and refused `xhigh`, so after a person chose `xhigh` no request to it carries `high` either for the rest
  of the process, and the model is reported as reading none. A level a model does not take is refused with
  the status of a field it does not know, and the two are told apart only by sending the request again
  without the level.

- **An entry keyed `openai` is read as OpenAI's for the model a reply names, at any endpoint.** An
  entry keyed `openai` is not checked for a substituted model, even where its `baseURL` names a
  proxy that does substitute one. A proxy whose replies should be compared is reached by keying the
  entry with another name.

- **A bare name beginning `openai/` reaches an `openai` entry.** BACKEND-18 reads a name as
  qualified before it reads it as a bare name a block lists, so `openai/gpt-oss-120b` listed by
  another gateway and written without that gateway's id goes to OpenAI. A name picked from `/model`
  is recorded qualified, as `openrouter/openai/gpt-oss-120b`, and is not affected.

- **A bare name beginning `anthropic/` reaches an `anthropic` entry.** For the same reason, and with
  more reach: OpenRouter lists its Claude models as `anthropic/claude-sonnet-4.5`, so a block of its
  that lists one, picked by writing that slug bare beside an `anthropic` entry, goes to Anthropic with
  Anthropic's key. A name picked from `/model` is recorded qualified, as
  `openrouter/anthropic/claude-sonnet-4.5`, and is not affected.

- **Brave's endpoint and an account the tier variables alone configure take neither figure of
  BACKEND-55.** Both figures live in a `provider` entry's `options`. The model Brave's endpoint
  serves has no entry, and the tier variables of BACKEND-33 have no block to state anything in, so a
  person reaching either is held to two retries and a two minute gap. Tier words that resolve
  through an `amazon-bedrock` entry take that entry's figures. Stating them for the tier variables
  would take a name in BACKEND-33's table or a top-level key, and neither is decided here.

- **A stated gap does not lengthen the bound on a gateway's reply as a whole.** A reply whose
  request states no length is ended ten minutes after it began
  ([NET-5](network-egress.md#NET-5)), so a gateway whose model is silent for longer than that is
  still cut off whatever `idleTimeoutMs` says. The cap on the gap is that bound, so no stated figure
  is wasted, and a person whose model needs longer than ten minutes has no setting that gives it.

- **A wait a service asks for is taken on every retry, so a large count lengthens it.** A rate limit
  that keeps asking for the two minutes BACKEND-54 takes in full is waited out before each of up to
  seven retries, about fourteen minutes with the requests themselves, and the pause the doubling
  gives is only the least of each wait. The count is the person's, and nothing shortens a wait the
  service asked for within the two minutes, since sending sooner is the failure that clause exists
  to avoid.

- **Thinking is neither asked for nor kept for an Anthropic model.** No `thinking` field is sent and no
  thinking block is sent back (BACKEND-57), so a model that would have continued from its own
  reasoning begins each round from the conversation alone, and what it thought is not shown. A
  stream that goes quiet for longer than the idle bound ([NET-5](network-egress.md#NET-5)) while a
  model thinks, with neither a delta nor a keepalive, is ended as a request that did not get through,
  as BACKEND-46's cost says of Bedrock.

- **Thinking is read under one name, and the count does not move while a model thinks.** Only
  `reasoning_content` is read (BACKEND-59), so a service that sends its working under another name
  draws nothing under `/thinking`. Before the service reports its own figure the live count is of the
  reply's words and tool arguments, so a round spent thinking shows no count until the figure arrives.

- **A `thinking` field in a model's `options` breaks a tool turn.** Options reach the body as written
  (BACKEND-15) and this build neither reads nor replays thinking, so a request that asks for it and
  carries a tool call from an earlier round is one the service refuses for the missing block. The
  first round answers and the second does not, and nothing here removes the field, which would be
  deciding against what somebody wrote. A block copied from opencode with a budget in it fails this
  way, and a model of this family that no longer takes a budget refuses it from the first round.

- **A level a model refuses is a level it is never sent.** The ladder reads a refusal of the request
  as the refusal of the level it carried, as BACKEND-22 does, so a model that reads `high` and
  refuses the `max` somebody chose is reported as reading none and is sent nothing at any level for
  the rest of the session. Telling the two apart would take the levels a model reads, which the
  listing does not state.

- **A request that answers after several concessions records all of them as refused.** The ladder
  gives up one field per attempt and the answer says only that the last attempt was accepted, so a
  model that refused only the ceiling, sent a request that carried a level, is remembered as
  refusing the level as well, which went first, and is sent neither for the rest of the session. The
  order the concessions go in (above) is what keeps this from falling on the cache.

- **A block keyed `anthropic` that names no SDK is reached in Anthropic's protocol, whatever its
  endpoint speaks.** A block that gives the id an OpenAI-compatible endpoint and names no SDK is sent
  Messages requests, which that endpoint answers with a refusal. Naming
  `@ai-sdk/openai-compatible` in the block makes it a gateway speaking chat completions
  (BACKEND-17). Telling the two blocks apart by the endpoint is not done, since a proxy in front of
  Anthropic's API is also a block with an endpoint and no SDK, and is what opencode reads that block
  as.

- **An `@ai-sdk/anthropic` entry under another id is not reached in Anthropic's protocol.** Only the
  `anthropic` id is, so a proxy kept under a name of its own is configured as a gateway speaking chat
  completions, and the import leaves such an entry ([IMPORT-3](import.md#IMPORT-3)).

- **An Anthropic request does not carry a run's `--output-schema`.** The reply is still checked
  against the schema once it arrives, and a run whose reply is off it ends as
  [CLI-28](cli.md#CLI-28) says. A Bedrock request carries none either.

- **An Anthropic roster is the first page of its listing.** One request names the most the service
  allows in a page, which is a thousand models, and a listing longer than that is cut where the page
  ends, without a second request for the rest.

- **The ceiling an Anthropic request carries is not read from the listing.** The listing states each
  model's maximum, and a request states 32,000 unless somebody stated another (BACKEND-41), so a
  model that allows more is held to the figure and one that allows less is asked once with it first.

- **A reasoning item is not sent back in the Responses protocol.** A model that reasons before a tool call
  starts the next round without that reasoning, and what it does with the loss is the model's behaviour,
  measured only by the quality of its answers, which nothing here measures. The service accepts the request
  without the item and with it, so this is a choice and not a refusal, and it is the one
  [BACKEND-57](#BACKEND-57) makes for thinking blocks.

- **A model's protocol is the entry's or the model's, never the listing's.** A service whose listing does not
  say which protocol a model takes is offered under the protocol its entry names, so a model that takes only
  the other one is refused at the first request. Only the ChatGPT account states its protocol itself.

- **A Responses request does not carry a run's `--output-schema`.** The reply is still checked against the
  schema once it arrives, and a run whose reply is off it ends as [CLI-28](cli.md#CLI-28) says, as it does
  for an Anthropic request.

- **A level the service refuses inside the stream is not given up.** Only a refusal by status is, so a
  service that answers 200 and then reports the level as an unsupported value fails the turn at that level,
  and the refusal is not remembered. No service has been seen to do this.
