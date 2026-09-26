---
id: BACKEND
title: Backends
status: normative
governs:
  - crates/agent/src/backend.rs
  - crates/agent/src/outcome.rs
  - crates/agent/src/subscription.rs
  - crates/cli/src/main.rs
  - crates/cli/src/plain.rs
  - crates/bedrock/src/credentials.rs
  - crates/tui/src/app.rs
  - crates/tui/src/status.rs
  - crates/tui/src/state.rs
  - crates/config/src/bedrock.rs
  - crates/config/src/env_var.rs
  - crates/config/src/lib.rs
  - crates/config/src/managed.rs
  - crates/bedrock/src/lib.rs
  - crates/aichat/src/lib.rs
  - crates/aichat/src/models.rs
  - crates/config/src/provider.rs
  - crates/config/src/settings.rs
documented-by: docs/website/docs/customize/configuration.md
---

## Scope

Where a request for a reply goes. Three services can answer: the aichat endpoint Brave runs, AWS
Bedrock through somebody's own account, and an OpenAI-compatible gateway somebody configured.
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

**Note.** The name is not content. It comes from a configured default or from a person picking off a
list they read, and a model's own output never reaches it.

`verified-by: bravebot_agent::backend::a_configured_bedrock_model_selects_the_bedrock_backend`
`verified-by: bravebot_agent::backend::a_brave_model_still_reaches_aichat_while_bedrock_is_configured`
`verified-by: bravebot_agent::backend::without_bedrock_configured_the_aichat_backend_is_selected`
`verified-by: bravebot_agent::backend::a_configured_gateway_model_selects_the_gateway_backend`
`verified-by: bravebot_agent::backend::a_gateway_reports_the_model_it_was_asked_for`

<a id="BACKEND-4"></a>
### BACKEND-4: what a person may choose is every model any reachable service offers

Configuring a second backend puts its models on offer beside the first one's rather than in place of
them.

**Why.** A roster that replaced the other left somebody who named a single tier with a picker
offering exactly one model and no way back to the ones every build has. Reaching more models is not
a reason to stop reaching the existing ones.

`verified-by: bravebot_tui::app::configured_tiers_are_offered_alongside_the_brave_roster`
`verified-by: bravebot_tui::app::gateway_models_are_offered_alongside_the_other_rosters`
`verified-by: bravebot_agent::backend::a_gateway_does_not_take_the_other_rosters_away`

<a id="BACKEND-5"></a>
### BACKEND-5: only models that can actually be reached are offered

A tier appears when the configuration names a model for it, and not otherwise. A service's own
roster is offered only where this build holds the credentials to reach it. Nothing is invented for a
tier that was left unset, and a gateway nothing can authenticate is not asked for a roster nobody
could then use.

**Why.** A name on the list is a promise that picking it works. An ARN cannot be derived from a
model name, so an entry guessed for an unnamed tier is a choice that fails remotely, and a build
pointed only at AWS has no Brave credentials, so offering that roster would list models whose every
request fails unsigned.

`verified-by: bravebot_tui::app::a_tier_with_no_model_configured_is_not_offered`
`verified-by: bravebot_config::lib::without_brave_credentials_the_default_is_the_strongest_bedrock_tier`
`verified-by: bravebot_tui::app::only_the_gateway_models_the_file_named_are_offered`
`verified-by: bravebot_config::provider::a_provider_may_offer_no_models`
`verified-by: bravebot_config::provider::a_provider_without_a_base_url_is_not_offered`

<a id="BACKEND-6"></a>
### BACKEND-6: a row says which service will answer it

Where the same model is reachable through more than one service, every model offered carries which
service answers it, in terms that cannot collide with a name a service chose for itself. Brave's own
roster carries nothing, being the one every build has. What a person reads is drawn from that,
under [VIEW-15](terminal-transcript.md#VIEW-15), and the note left when a model is chosen says it
too, since the row that carried it is gone by the time that note is read.

**Why.** The two are billed differently and authenticate differently, so which one answers is the
whole of what is being chosen between. Naming the service is not enough: Brave serves part of its own
roster through Bedrock and says so in the names it sends, so that word appeared on both halves of the
list and distinguished nothing. Carried beside the name rather than composed into it, because a
roster of several services is read as sections, and a service composed into every name is that name
repeated down a hundred rows.

`verified-by: bravebot_tui::app::a_configured_tier_is_not_confusable_with_a_brave_model_served_through_bedrock`
`verified-by: bravebot_tui::app::a_tier_with_no_profile_configured_still_names_the_account`
`verified-by: bravebot_tui::app::a_gateway_row_says_which_service_answers_it`
`verified-by: bravebot_aichat::models::a_fetched_row_carries_the_gateway_that_serves_it`

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

`verified-by: bravebot_agent::backend::a_brave_model_never_needs_an_aws_sign_in`
`verified-by: bravebot_agent::backend::without_bedrock_configured_nothing_needs_a_sign_in`
`verified-by: bravebot_agent::backend::signing_in_for_a_model_no_aws_account_serves_does_nothing`
`verified-by: bravebot_agent::backend::a_gateway_model_never_needs_an_aws_sign_in`
`verified-by: bravebot_agent::backend::a_model_an_aws_block_named_needs_a_sign_in_of_its_own`
`verified-by: bravebot_agent::backend::signing_in_for_a_model_an_aws_block_named_reaches_that_account`
`verified-by: bravebot_agent::backend::an_aws_block_leaves_the_other_rosters_needing_no_sign_in`

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

<a id="BACKEND-11"></a>
### BACKEND-11: a settings file names the model above the build, and a checkout's above a pick

Where a settings file names a model, that name is what a request uses, in preference to the model
compiled into the binary and to one exported as `BRAVE_AI_CHAT_DEFAULT_MODEL`. The exported variable
answers where no file names one, above the build.

A choice recorded with `/model` ranks as the person's own file, `~/.bravebot/settings.json`, does.
It outranks the `model` key in that file, and is outranked by one in a checkout's
`.bravebot/settings.json` or `.bravebot/settings.local.json`, or in the file `--settings` named. A
key spelled blank, or as something other than a string, names nothing and does not outrank a pick.
It still displaces the key a lower file named, so with nothing recorded the exported variable or the
build answers. Above all of it are `--model` and a model picked in the session that is running.

**Why.** Every release bakes a default model in, so this value ranked like the rest of the file would
lose on every binary anybody was given: the key would parse, `doctor` would report it, and nothing
would change outside a source build.

The variable is named as a default, and a default is how it is used: a `.envrc` that exports it for
every checkout of a project is saying what answers when nothing else does. Ranked above the file it
would outrank every `model` key on any machine that sources one, the key a checkout wrote to state
its model included. Claude Code ranks its counterpart, `ANTHROPIC_DEFAULT_MODEL`, last.

A pick is recorded once per person and read back in every checkout. Ranked above a checkout's file,
it is the one thing a checkout cannot override: two checkouts in one account cannot want different
models, and a project that states its model is undone by whatever its reader last picked anywhere.
Claude Code's `/model` writes the person's own settings file, which every other settings file
outranks, and this is that rung. The pick outranks that file's own key because both are the same
person speaking at the same rung, and the pick is the later of the two.

`verified-by: bravebot_config::lib::a_model_in_the_settings_file_outranks_the_baked_in_one`
`verified-by: bravebot_config::lib::a_model_in_the_settings_file_outranks_an_exported_one`
`verified-by: bravebot_config::lib::the_env_block_spelling_stays_below_the_baked_in_value`
`verified-by: bravebot_config::settings::a_layer_above_the_home_one_outranks_a_saved_pick`
`verified-by: bravebot_config::settings::the_home_layer_does_not_outrank_a_saved_pick`
`verified-by: bravebot_config::settings::a_layer_above_that_names_nothing_does_not_outrank_a_saved_pick`
`verified-by: bravebot_session::store::a_checkouts_model_outranks_the_saved_pick_and_the_home_file_does_not`
`verified-by: bravebot_tui::persist::a_recorded_model_answers_between_a_checkouts_file_and_the_persons_own`
`verified-by: bravebot_cli::running::a_run_asks_for_a_checkouts_model_over_the_recorded_one`
`verified-by: bravebot_cli::running::a_run_asks_for_the_settings_model_over_an_exported_default`
`verified-by: bravebot_cli::running::doctor_names_a_checkouts_model_rather_than_the_pick_it_outranks`

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
written in the file. It is read at the point a request needs it, and a request whose block named a
credential that nothing holds is refused with the remedy named rather than sent.

**Why.** Read once at startup, a credential goes stale in a session where somebody exported a new
one. Sent without one, the request fails at the far end for a reason nothing local could explain,
which is the same argument that stops a model name being guessed for an unconfigured tier. Naming a
variable is also the only way to keep a long-lived token out of a file people paste into issues,
which is why it is preferred where the block offers both and why a value in the file never displaces
one a variable holds.

**An entry naming AWS is the exception**, and not a token kept somewhere else: Bedrock takes a
signature over the request, so there is no credential for a block to name and none is read from one.
Which set of AWS credentials to sign with comes from the profile the block names, resolved when a
request needs it, which is the same moment and the same reason a token is read.

**A block naming no credential is the second exception.** An empty `env` and no `options.apiKey` is
the person saying this gateway wants none, so its requests carry no `authorization` header and its
roster is asked for without one. A block that does name somewhere for a credential to live, and finds
nothing there, is a stale or missing token and is still refused.

**Why the distinction is where it is.** The reason above holds for the second case and not the first:
a gateway that wants no credential answers an unauthenticated request, so nothing fails at the far end
for a reason nothing local could explain, and a refusal names a remedy that does not exist. Requiring
a value instead would require a field of a block that does not have one, which BACKEND-13 rules out,
and a dummy `apiKey` teaches people to write fake credentials into a file they paste into issues.
Deciding it by endpoint rather than by what the block says would refuse the same local service reached
across a LAN or through a reverse proxy. BACKEND-5 is unaffected: such a gateway is reachable, so
offering its models is not offering rows that fail when picked.

`verified-by: bravebot_config::provider::a_named_variable_holds_the_token_before_the_file_does`
`verified-by: bravebot_config::provider::a_token_written_into_the_file_is_still_read`
`verified-by: bravebot_config::provider::a_provider_with_nothing_holding_a_token_has_none`
`verified-by: bravebot_config::provider::a_provider_naming_no_credential_needs_none`
`verified-by: bravebot_agent::backend::a_gateway_with_nothing_holding_a_token_refuses_the_request`
`verified-by: bravebot_agent::backend::a_gateway_naming_no_credential_sends_unauthenticated`
`verified-by: bravebot_aichat::lib::a_gateway_needing_no_credential_sends_no_authorization_header`
`verified-by: bravebot_aichat::client::a_gateway_needing_no_credential_is_asked_for_its_roster_unauthenticated`
`verified-by: bravebot_cli::main::a_gateway_needing_no_credential_is_reported_as_needing_none`

<a id="BACKEND-17"></a>
### BACKEND-17: a gateway named by a name this system knows needs no endpoint written down

A block naming a gateway this system already knows an endpoint for reaches it without stating one. An
endpoint the block does state is where its requests go regardless. Where neither holds, the entry
configures no service.

The set of known names is compiled in. Nothing is fetched to resolve one, and no service is asked what
its own endpoint is.

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
`verified-by: bravebot_config::provider::a_stated_endpoint_beats_the_one_compiled_in`
`verified-by: bravebot_config::provider::a_provider_without_a_base_url_is_not_offered`

<a id="BACKEND-18"></a>
### BACKEND-18: a model name may say which gateway is meant, and only the rest of it is sent

A name selecting a gateway model may carry the gateway's own identifier ahead of the name that
gateway knows the model by, separated once. The identifier selects the service; only the remainder is
sent to it, and it is also what a reply's name is compared against. A name no configured gateway
claims reaches the service that does recognise it, as before.

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

<a id="BACKEND-19"></a>
### BACKEND-19: a gateway that was told no models is asked what it serves

Where a gateway block names its models, those are what is offered and nothing is asked over the
network. Where it names none, the gateway itself is asked, and what it answers is offered. A listing
that cannot be fetched contributes nothing and takes nothing away from the rest of the roster.

What the credential in use may reach is asked for ahead of what the service offers generally, and the
wider roster answers only where the narrower question does not. A block naming no credential has no
account for that narrower question to be about, so only the wider one is asked. Nothing is capped:
every model reported that can call tools is offered, ordered with the model a session would use first
and the rest by name.

**Why.** A block naming no models is the ordinary case, not a mistake: the tool this shape is borrowed
from resolves a roster from a registry, so the commonest block copied in names a credential and
nothing else. Offering nothing for such a block means a gateway configured exactly as that tool
configures it appears in a diagnostic and is unusable, which was the state this replaced.

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

`verified-by: bravebot_aichat::models::a_gateway_roster_is_offered_under_names_that_say_which_gateway_serves_them`
`verified-by: bravebot_aichat::models::a_window_a_gateway_reports_is_taken_from_the_listing`
`verified-by: bravebot_aichat::models::a_fetched_model_with_no_reported_window_gets_the_assumed_one`
`verified-by: bravebot_aichat::models::a_window_the_block_stated_outranks_the_one_reported`
`verified-by: bravebot_aichat::models::a_gateway_model_that_cannot_call_tools_is_not_offered`
`verified-by: bravebot_aichat::models::a_gateway_that_reports_no_capabilities_still_offers_its_models`
`verified-by: bravebot_aichat::models::a_fetched_entry_with_no_usable_name_is_dropped`
`verified-by: bravebot_aichat::models::fetched_gateway_models_are_not_marked_premium`
`verified-by: bravebot_aichat::client::a_gateway_with_a_credential_is_asked_what_that_account_may_reach`
`verified-by: bravebot_aichat::client::a_gateway_needing_no_credential_is_asked_for_its_roster_unauthenticated`
`verified-by: bravebot_tui::app::a_fetched_roster_leads_with_the_model_in_force`
`verified-by: bravebot_tui::app::a_fetched_roster_nobody_has_chosen_from_is_still_sorted`

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

`verified-by: bravebot_aichat::protocol::a_request_nobody_asked_a_level_of_mentions_no_effort`
`verified-by: bravebot_aichat::protocol::a_word_naming_no_level_is_not_a_choice`
`verified-by: bravebot_aichat::protocol::a_level_is_named_whatever_case_it_was_written_in`
`verified-by: bravebot_bedrock::protocol::a_request_nobody_asked_a_level_of_carries_no_output_config`
`verified-by: bravebot_agent::turn::without_a_chosen_effort_no_level_is_requested`


<a id="BACKEND-21"></a>
### BACKEND-21: each service is sent the level in its own protocol's field

One word ranks the levels, and where that word goes in the body is the wire protocol's business. No
service is sent the other's shape.

| Service | Where the level is sent |
|---|---|
| The aichat endpoint Brave runs | `reasoning_effort`, beside the model |
| An OpenAI-compatible gateway | `reasoning_effort`, beside the model |
| AWS Bedrock | `effort`, inside `output_config`, among the fields handed to the model unread |

This says where a level is sent, not what becomes of it. What a service does with the field is that
service's own behaviour, observable only by measuring it, and one of the three is known to discard it
altogether. That is recorded under Known costs rather than here, because a clause pinned to a remote
service's current behaviour is a clause that goes stale without anything in this repository changing.

**Why.** The two protocols state the same idea differently, and the conversion between them already
happens in one place for every other field. Sending one service the other's shape is a field it
certainly does not read.

`verified-by: bravebot_aichat::protocol::a_level_is_sent_in_the_name_this_protocol_gives_the_field`
`verified-by: bravebot_bedrock::protocol::a_level_is_sent_inside_the_object_this_api_states`
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

**The level is the last concession given up.** A request also carries cache breakpoints nobody asked
for, and either field is refused with the same status, so the two are given up in order: a request
still marking a prefix is sent again without the marks first, and the level goes only where that
request is refused too. Giving up the level first would read a refusal of the caching as the model
refusing to be told how hard to think, and stop sending a level to a model that reads one. Where the
request that answered had given up both, both are remembered, the status naming no field: a service
that reads a breakpoint and refuses a level gives up the caching as well for the life of the process.

**Why.** The judgment is the only description these services offer, and throwing it away leaves the
interface reporting a charge somebody chose and stopped getting, which is the thing this clause
exists to prevent. It is also all that stands between a service that refuses the field and a
conversation in which no turn can succeed, the level being in every request such a turn makes.
Learned rather than declared because an inference-profile ARN does not say which provider is behind
it, and a settings file cannot state what its author does not know either.

`verified-by: bravebot_bedrock::lib::what_a_model_refused_outlives_the_client_that_found_out`
`verified-by: bravebot_bedrock::lib::a_probe_that_settled_nothing_is_not_remembered`
`verified-by: bravebot_bedrock::lib::one_model_refusing_says_nothing_about_another`
`verified-by: bravebot_aichat::client::a_level_a_gateway_refuses_costs_the_field_and_not_the_turn`
`verified-by: bravebot_aichat::client::a_gateway_that_refused_a_level_is_not_sent_one_again`
`verified-by: bravebot_aichat::client::a_level_refusal_the_retry_did_not_fix_is_not_remembered`
`verified-by: bravebot_aichat::client::one_model_refusing_a_level_says_nothing_about_another_on_the_same_gateway`
`verified-by: bravebot_aichat::client::a_level_a_block_wrote_down_is_not_given_up`
`verified-by: bravebot_aichat::lib::a_model_whose_service_refused_a_level_is_reported_as_reading_none`
`verified-by: bravebot_aichat::models::a_row_whose_service_refused_a_level_reads_none_however_silent_the_roster`
`verified-by: bravebot_aichat::models::a_row_the_service_refused_reads_none_however_loudly_the_roster_advertises_it`
`verified-by: bravebot_aichat::models::a_brave_roster_row_the_endpoint_refused_reads_none`
`verified-by: bravebot_aichat::models::a_gateway_model_that_does_not_take_the_effort_parameter_says_so`
`verified-by: bravebot_aichat::models::a_gateway_that_states_no_parameters_is_not_taken_to_read_no_level`
`verified-by: bravebot_cli::running::a_run_withholds_a_level_the_roster_says_the_model_does_not_read`
`verified-by: bravebot_cli::running::a_run_sends_a_level_the_roster_says_the_model_reads`
`verified-by: bravebot_cli::running::a_session_in_lines_withholds_a_level_the_roster_says_the_model_does_not_read`
`verified-by: bravebot_cli::running::a_service_that_refuses_the_level_it_advertised_is_reported_as_reading_none`
`verified-by: bravebot_tui::app::a_level_is_withheld_from_a_model_that_reads_none`
`verified-by: bravebot_tui::app::a_level_a_model_cannot_use_is_kept_rather_than_forgotten`
`verified-by: bravebot_tui::app::asking_for_a_level_a_model_cannot_use_says_so`
`verified-by: bravebot_tui::app::a_model_the_listing_did_not_describe_still_takes_a_level`
`verified-by: bravebot_tui::app::a_level_the_service_refused_stops_being_reported_as_in_force`
`verified-by: bravebot_tui::app::a_turn_that_refused_nothing_does_not_restore_a_level_the_roster_says_is_unread`
`verified-by: bravebot_tui::status::a_level_the_model_does_not_read_is_reported_as_unread`


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
| `env`, `provider`, `attribution`, `keybindings`, `search` | per name, one level down; the value under a name is replaced whole |
| `run.scrubEnv`, every list under `permissions`, `mcp.request` | every layer's entries are kept |
| `model`, anything else | the closest layer that set it wins |

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

The two names under `attribution` combine per name for the same reason `env` does: they are
unrelated destinations that happen to share a block, and a file answering for one must not answer
for the other by omission. The chords under `keybindings` and the two caps under `search` are the
same case: a file moving one action's key is no statement about the other six, and a checkout
widening a search's walk for its own size is none about how long a read may take.

The lists are the exception because an entry in one only ever narrows what is possible: a name under
`scrubEnv` takes a variable away from a subprocess, and a rule under `permissions` refuses something
that was otherwise allowed. Overriding either would let a layer hand back what a weaker one withheld,
and a permission removed by a file somebody did not open is the one outcome worth ruling out. A model
is one choice rather than a list, so it resolves like any other single value.

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
`verified-by: bravebot_config::settings::every_layer_adds_to_the_directories_a_file_makes_reachable`
`verified-by: bravebot_config::settings::every_layers_request_is_read_and_each_alias_is_kept_once`
`verified-by: bravebot_config::settings::the_closest_layer_that_named_a_model_wins`
`verified-by: bravebot_config::settings::a_layer_answering_for_one_attribution_name_leaves_the_other`
`verified-by: bravebot_config::settings::a_layer_capping_one_side_of_a_search_leaves_the_other`
`verified-by: bravebot_config::settings::a_layer_naming_no_model_leaves_the_one_below_it`
`verified-by: bravebot_config::settings::a_project_layer_replaces_one_gateway_and_leaves_the_others`
`verified-by: bravebot_config::settings::a_project_gateway_naming_no_host_replaces_one_that_did`
`verified-by: bravebot_config::settings::an_unparseable_project_layer_leaves_the_global_one_in_force`
`verified-by: bravebot_config::settings::an_oversized_project_layer_leaves_the_global_one_in_force`
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
### BACKEND-27: a Bedrock request marks the prefix it will send again

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

**A breakpoint carries no label and asks for nothing.** It marks a prefix of a request that has
already been assembled, after every gate that decided what may be in it. Which bytes the service
kept from a previous request of the same session cannot put a byte into this one that was not sent,
so nothing in [labels.md](labels.md) reads differently for a request that carries breakpoints than
for one that does not.

`verified-by: bravebot_bedrock::protocol::the_system_prompt_carries_a_breakpoint`
`verified-by: bravebot_bedrock::protocol::the_last_block_of_the_conversation_carries_a_breakpoint`
`verified-by: bravebot_bedrock::protocol::a_conversation_ending_in_a_tool_result_is_marked_too`
`verified-by: bravebot_bedrock::protocol::a_request_giving_up_its_conversation_keeps_the_prompts_breakpoint_alone`
`verified-by: bravebot_bedrock::protocol::a_reply_without_a_breakpoint_still_parses`
`verified-by: bravebot_bedrock::protocol::cached_tokens_are_counted_as_the_prompt_they_were`
`verified-by: bravebot_bedrock::protocol::a_request_without_breakpoints_keeps_everything_that_was_asked_for`
`verified-by: bravebot_bedrock::lib::what_a_model_refused_outlives_the_client_that_found_out`
`verified-by: bravebot_bedrock::lib::a_request_refused_on_its_contents_is_asked_again_without_the_breakpoints`
`verified-by: bravebot_bedrock::lib::only_a_refusal_on_the_contents_drops_the_breakpoints`

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
fails somewhere far from the mistake.

**What a row says.** A model named here has no tier, so a picker row carries what the block called
it, and its id where the block called it nothing. An inference-profile ARN is not a name anybody
reads, which is why this is worth stating rather than left to fall out of the id.

`verified-by: bravebot_config::provider::an_aws_block_configures_bedrock_rather_than_a_gateway`
`verified-by: bravebot_config::provider::an_aws_model_the_block_did_not_name_is_shown_by_its_id`
`verified-by: bravebot_config::provider::an_aws_block_without_a_region_configures_nothing`
`verified-by: bravebot_config::lib::a_model_an_aws_block_named_reaches_bedrock_rather_than_a_gateway`
`verified-by: bravebot_config::lib::a_name_qualified_by_the_aws_id_is_not_a_gateway_either`
`verified-by: bravebot_agent::backend::a_model_an_aws_block_named_selects_the_bedrock_backend`
`verified-by: bravebot_tui::app::a_bedrock_model_a_block_named_is_shown_under_that_name`

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

**A stated value is handed over as text to copy rather than as a sentence addressed to the
planner.** The block resolves over the three layers of [BACKEND-24](#BACKEND-24) and the middle of
them is a file in the tree being worked on, so the string is whatever that checkout says. It is
configuration rather than content, on the footing `env` and the permission rules are read on, but
it is free text where those are structured, so it is quoted and the planner is told in the same
breath that nothing inside it is addressed to it.

`verified-by: bravebot_config::settings::an_empty_attribution_is_a_choice_of_nothing`
`verified-by: bravebot_config::settings::an_attribution_name_no_file_wrote_is_unset`
`verified-by: bravebot_agent::preamble::an_empty_attribution_tells_the_planner_to_carry_nothing_on_a_commit`
`verified-by: bravebot_agent::preamble::an_attribution_no_file_named_is_not_stated_at_all`
`verified-by: bravebot_agent::preamble::an_attribution_a_file_named_is_carried_word_for_word`
`verified-by: bravebot_agent::turn::a_turn_is_told_what_the_settings_say_a_commit_message_may_carry`

<a id="BACKEND-31"></a>
### BACKEND-31: a reply reports how much of the prompt the service did not have to read

A reply's usage carries two figures beside the prompt total: how much of the prompt the service
answered out of its own cache, and how much this request wrote into that cache for the next one to
read. Both are counted inside the prompt total rather than beside it, so one figure says what a
round sent and the other two say what it cost. Bedrock states both. An OpenAI-compatible reply
states the read alone, in `prompt_tokens_details.cached_tokens`, there being no field in that
protocol for a write. A service stating neither leaves both at zero.

A turn sums them over its rounds, as it sums the total, and a delegate's are added to the turn that
spawned it.

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

**The status panel reports the last turn's, beside what the session cost.** The last turn rather
than a total over the session, because caching is a property of a request: a session that compacted
part way through has turns whose prefix survived and turns whose prefix was rewritten, and a total
averages away the thing the figures are for. Nothing about a cache is kept in a session record, so a
resumed session reports nothing until a turn has run. Clearing goes with what the conversation spent
rather than with the model the user chose, the figures describing a prompt that has been thrown away.
Anything else that changes which turn is the last one moves the figures with it: rewinding a turn
puts back what the turn before it read, and a turn that failed or was stopped reports nothing rather
than leaving the turn before it on the panel.

**Presented as two figures and never as their sum.** They are priced in opposite directions, a read
at a fraction of a fresh token and a write above one, so a turn that saved almost the whole prompt
and a turn that paid a premium on it add up the same. The heading carries no total of its own, and
says which turn it speaks for, the counts beside it being the session's.

`verified-by: bravebot_bedrock::protocol::a_cached_round_is_told_apart_from_one_that_read_the_whole_prompt`
`verified-by: bravebot_bedrock::lib::both_halves_of_the_cost_survive_the_stream`
`verified-by: bravebot_aichat::protocol::the_cache_figure_is_read_out_of_the_details_object`
`verified-by: bravebot_aichat::protocol::a_usage_object_without_cache_figures_still_parses`
`verified-by: bravebot_aichat::protocol::a_null_cache_figure_reads_as_silence_rather_than_failing_the_reply`
`verified-by: bravebot_agent::turn::a_turn_sums_what_its_rounds_read_out_of_the_cache`
`verified-by: bravebot_agent::turn::a_turn_against_a_server_that_says_nothing_about_a_cache_reports_nothing`
`verified-by: bravebot_agent::turn::a_turn_counts_what_its_delegates_read_out_of_the_cache`
`verified-by: bravebot_tui::status::the_panel_says_how_much_of_the_prompt_came_out_of_the_cache`
`verified-by: bravebot_tui::status::a_backend_that_reports_nothing_about_a_cache_gets_no_cache_lines`
`verified-by: bravebot_tui::status::a_turn_that_only_wrote_to_the_cache_does_not_report_a_read_of_zero`
`verified-by: bravebot_tui::state::clearing_forgets_what_the_last_turn_read_out_of_the_cache`
`verified-by: bravebot_tui::state::the_cache_figure_follows_which_turn_is_the_last_one`
`verified-by: bravebot_tui::sessions::a_rewind_point_keeps_no_cache_figure_in_the_record`

<a id="BACKEND-32"></a>
### BACKEND-32: an aichat request marks the prefix it will send again

A request in the OpenAI-compatible wire format marks two prefixes: the system prompt, which the tool
schemas travel in front of, and the last thing the user said. The second is left off where the caller
says the conversation is given up once it has been answered, marking the prompt alone. The mark is a
`cache_control` of `{"type": "ephemeral"}` on the content block the prefix ends at, which is the field
the Anthropic API defines for this and the field a gateway fronting such a model reads. A marked
message carries its words as a block rather than as a bare string, that being the only shape the
field exists in.

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

**Remembered against the service as well as the model.** A model id says nothing about who serves
it: two gateways can offer the same name, and one of them can be the name Brave's own endpoint
answers to. A refusal recorded against the id alone would stop the asking everywhere one appeared.

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
`verified-by: bravebot_aichat::protocol::a_result_the_assistant_asked_for_is_not_marked`
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
### BACKEND-33: an `env` block names these thirteen variables

The `env` block of a settings file sets variables under their own names, and these are the names
something reads:

| Name | What it decides |
|---|---|
| `SERVICES_KEY_AICHAT` | the key a request to Brave's endpoint is signed with |
| `BRAVE_SERVICES_KEY_ID` | which key that signature is checked against |
| `BRAVE_AI_CHAT_ENDPOINT` | the host Brave's endpoint is reached at |
| `BRAVE_AI_CHAT_PREMIUM_ENDPOINT` | the host an imported subscription is spent against |
| `BRAVE_AI_CHAT_DEFAULT_MODEL` | which model answers before anybody has chosen one |
| `BRAVEBOT_CONTEXT_BUDGET` | how many prompt tokens a conversation may reach before it is shortened |
| `BRAVEBOT_OUTPUT_BUDGET` | how many tokens one reply may run to before the service cuts it off |
| `BRAVEBOT_USE_BEDROCK` | `1` to reach models through somebody's own AWS account |
| `AWS_REGION` | which region that account is reached in |
| `AWS_PROFILE` | which profile in the AWS configuration names the credentials to sign with |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | the model the tier word `opus` names |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | the model the tier word `sonnet` names |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | the model the tier word `haiku` names |

A gateway is not configured from this block: it is a `provider` entry, whose shape BACKEND-13
states and whose credential BACKEND-16 names. The top-level `model` key is not one of these either, being a
choice rather than a variable, and BACKEND-11 is what ranks it.

**Why.** A configuration surface that is described but never named is one nobody can use without
reading the source. Anything written *about* this system (the site somebody installs it from, a
message telling a person what to set) is written from what is stated here, so a backend whose
variables are named nowhere is a backend that reaches people undocumented however completely its
behaviour is specified. Naming them is also what makes the set reviewable: a fourteenth variable is
a change to this table, which a person reads, rather than a constant added to a file nobody is
asked to look at.

The AWS names keep the spelling another tool already gave them, and the switch and the two budgets
carry this program's own prefix, for the reason BACKEND-24 gives about the file as a whole: a block
copied from elsewhere should work unedited, while a name that decides what *this* program does
belongs to this program and must not collide with whatever else a shared shell profile wanted.

`verified-by: bravebot_config::lib::every_name_a_settings_block_may_set_reaches_the_configuration`
`verified-by: bravebot_config::settings::an_env_block_is_read`

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

Keeping a name is not the same as acting on one, and the distance between the two is the whole of
what makes the file safe to read. A block that could switch off credential scrubbing would be a
block that hands this agent's secrets to every command it runs, decided by whatever last edited a
file in a checkout.

`verified-by: bravebot_config::lib::a_name_nothing_consults_changes_nothing`
`verified-by: bravebot_config::settings::a_name_this_crate_does_not_know_is_still_read`
`verified-by: bravebot_config::settings::the_names_are_reportable_and_the_values_are_not`

<a id="BACKEND-37"></a>
### BACKEND-37: failures carry safe reasons and measured request counts

Backend failures report a fixed category, an HTTP status when known, and the number of requests
handed to egress. The count includes retries and capability probes for streamed and whole replies.
A failure before egress has zero attempts; an uncounted error has an unknown count. A retry announced
before backoff does not count until it starts. Cancelling that wait retains the requests already sent.

Categories come from structured errors. Error bodies, headers, credentials, URLs, and raw transport
messages do not enter these details. Processor failures use a fixed category in their tool results;
delegate failures tell the planner only that the delegate did not finish. Compaction failure
messages also use the category. These paths do not copy raw backend errors into the conversation.

Cancellation is distinct from failure. A processor carries cancellation and its attempt count
separately from its tool-result text, so the parent can report the stop without parsing that text.

Known Bedrock stream exceptions map to fixed categories: validation to refused, throttling to
rate-limited, and service-unavailable or internal-server errors to unavailable. Other exception
names map to incomplete. Retry eligibility is unchanged.

A reply stopped at its output ceiling reports that ceiling alongside the category. It is this
program's own configured figure rather than anything a service said, so it is not a detail taken
from a reply, and without it the report names a limit and no way to change it.

`verified-by: bravebot_agent::backend::each_status_a_service_answers_with_is_reported_as_what_it_means`
`verified-by: bravebot_agent::backend::a_gateway_with_nothing_holding_a_token_is_reported_as_unconfigured`
`verified-by: bravebot_agent::backend::aws_refusing_the_credentials_it_was_signed_with_is_reported_as_unauthorized`
`verified-by: bravebot_agent::backend::a_hop_refused_for_leaving_https_is_reported_as_a_gate_rather_than_a_failed_request`
`verified-by: bravebot_agent::backend::what_is_kept_about_a_failure_carries_nothing_the_service_or_the_setting_said`
`verified-by: bravebot_agent::backend::a_reply_stopped_at_the_ceiling_reports_which_ceiling`
`verified-by: bravebot_tui::state::a_reply_stopped_at_a_ceiling_says_which_ceiling`
`verified-by: bravebot_agent::turn::a_service_that_kept_refusing_is_reported_with_its_status_and_the_attempts_made`
`verified-by: bravebot_agent::turn::a_reply_that_stopped_early_is_reported_as_unfinished_with_no_status`
`verified-by: bravebot_agent::turn::a_refusal_counts_the_cache_probe_as_a_second_request`
`verified-by: bravebot_bedrock::lib::request_counts_include_capability_probes`
`verified-by: bravebot_bedrock::lib::cancellation_in_backoff_counts_only_sent_requests`
`verified-by: bravebot_aichat::client::a_stop_does_not_wait_out_the_pause_between_attempts`
`verified-by: bravebot_aichat::client::a_stop_between_attempts_at_a_whole_reply_does_not_wait_out_the_pause`
`verified-by: bravebot_bedrock::lib::a_stop_between_attempts_at_a_whole_reply_does_not_wait_out_the_pause`
`verified-by: bravebot_bedrock::lib::framed_service_exceptions_keep_their_kind_and_request_count`
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
name in the file decides nothing, the signing key and the key id included. A pinned name is resolved
from this file alone, and a name it does not pin resolves exactly as it would with no such file.

No credential is read from this file. A gateway entry's `apiKey` is dropped, and the entry's host,
models and variable names are honoured without it.

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

Where the model in force would be sent to Brave's own aichat endpoint and no Leo Premium
subscription is in hand to spend on it, nothing is configured to serve the turn and no work starts.
A session does not open and a one-shot run asks nothing. What is said instead is the three ways to
configure a service that can answer, each naming the thing to type or to write: an AWS account
through Bedrock, an OpenAI-compatible gateway, and importing a Leo Premium subscription. `doctor`
says the same and fails, which is [CLI-7](cli.md#CLI-7).

The model decides, as it does everywhere here: a model a configured service serves is not this
case, whatever the aichat fields hold. The model asked about is the one the run will actually
request, which is a name given on the command line, then the one a session recorded, then the
configured default.

An endpoint that is not one of Brave's own deployments is not this case either, development channel
and production alike. That is somebody's own host, a local model server or a private deployment or
a proxy in front of either, and nobody is handed a configuration pointing at one. A port written
into an endpoint does not change which deployment it names.

**A service configured while the model in force is Brave's own is told apart, and gets one line
rather than the three.** That is where a settings block copied out of the tool BACKEND-13 borrows
its shape from lands: those blocks name their models and name no default, so the model stays the
one the build baked in. What is said there is the key that names one of the configured service's
models, and the three routes are left out.

The refusal is the configuration ending of [CLI-6](cli.md#CLI-6) rather than a status of its own.
Where a subscription is stored and could not be read, what the store said about it is said first.
A machine with nowhere to keep credentials has none stored rather than a batch it could not read.

**Why.** Nothing has been set up yet. A released binary arrives pointed at Brave's own aichat
endpoint, and being pointed at it is not the same as having a service configured to do this work: no
account was named, no gateway was written down, and no subscription was imported. A run that went
ahead would send the turn there anyway and hand back whatever that request became, and whatever it
became is not the agent doing the work. What a person is left with is a session that failed, or one
that answered poorly, and nothing on the screen saying the missing piece was a configuration. The
conclusion drawn from a first session is the one that sticks.

Said before anything runs because that is the only moment where the answer is "configure a
service". Afterwards the question a person has is why the answers are bad, which is a question the
configuration cannot be reached from.

Refusing rather than letting the request go, for the reason
[PREM-8](premium-credentials.md#PREM-8) reports a subscription it could not read rather than passing
over it: what comes back from a request that was never going to be served carries no sign of why,
and nothing about a bad session points at where a request went.

Three routes rather than one, and each named by what to type, because a refusal that states the
problem and no way out of it has moved the work to the person and told them nothing they could not
already see. Leo Premium is last and says why it is last: those models are reached through Brave's
AI gateway, which has problems of its own being worked on. It is still the shortest route for
somebody who already subscribes, which is why it is offered rather than left out.

Telling the configured case apart is the same argument in the other direction. Somebody there is
one settings key from working, and three ways to set up a service would be three things to read
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
`verified-by: bravebot_agent::subscription::braves_own_deployments_are_told_from_a_host_somebody_chose`
`verified-by: bravebot_agent::subscription::a_port_is_not_part_of_the_host`
`verified-by: bravebot_cli::running::a_first_run_with_no_service_configured_says_how_to_configure_one`
`verified-by: bravebot_cli::running::a_configured_gateway_is_not_refused`
`verified-by: bravebot_cli::running::a_service_configured_with_no_model_of_its_own_named_says_to_name_one`
`verified-by: bravebot_cli::running::a_model_named_on_the_command_line_is_not_refused`
`verified-by: bravebot_cli::running::a_session_in_lines_with_no_service_configured_says_how_to_configure_one`
`verified-by: bravebot_cli::running::a_session_in_lines_with_a_configured_gateway_opens`

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

<a id="BACKEND-41"></a>
### BACKEND-41: how long a reply may run belongs to the model, and an exported figure outranks it

A Bedrock request states a ceiling on the reply. A configured model may state its own, out of the
same `limit` block its context window comes from and under the same rule, and where it states none
the figure assumed is one deliberately below what the model is likely to allow. One exported
variable states a ceiling for every model this build reaches and outranks whatever any of them
stated. Nothing is asked over the network to find out, and nobody is required to supply it.

The aichat backend states no ceiling at all, so nothing here applies to it: its requests carry no
such field and whatever bounds a reply there belongs to the service.

**Why.** This is BACKEND-14's question with its asymmetry reversed, so the defaulting is the same
and the reason is not. A ceiling below what the model allows costs the tail of a long answer; one
above what it allows is a request the service refuses outright and refuses every time, so a guess
upward does not cost a reply its ending, it costs the model the ability to answer at all. Bedrock
fronts models from several providers whose ceilings differ by an order of magnitude, an
inference-profile ARN does not say which model is behind it, and no endpoint there reports the
figure, so there is nothing to resolve a guess against and the assumed number has to hold for the
unrecognised case.

Which makes the assumed number low enough to be felt, and stating a better one is the answer to
that rather than guessing a better one. The variable exists because the three tier names have no
block to state anything in, and they are how most people reach this backend: without it the only
models whose ceiling could be raised would be the ones a `provider` block already named.

`verified-by: bravebot_config::provider::a_stated_reply_ceiling_is_read_per_model`
`verified-by: bravebot_config::provider::a_limit_missing_either_half_states_no_window`
`verified-by: bravebot_config::lib::an_exported_reply_ceiling_outranks_every_stated_one`
`verified-by: bravebot_config::lib::a_reply_ceiling_that_is_not_a_figure_leaves_the_stated_one_standing`
`verified-by: bravebot_bedrock::lib::a_request_carries_the_ceiling_its_own_model_states`

<a id="BACKEND-42"></a>
### BACKEND-42: a reply the ceiling stopped is kept for what it said and never for what it asked

A reply that reaches its ceiling having written something is returned as a reply, marked as having
stopped short, with its reported usage. Its tool calls are not returned: a reply cut off carries
none, whatever the service sent, so the round it ends is the last one. A reply that reaches the
ceiling having written nothing is a failure, and the failure names the ceiling.

**Why.** Everything the model wrote before the cutoff is the turn's work, and reporting only that
it was too long destroys it to say so. The same argument settled the same question for a capped
`run`, whose output is collected after the kill rather than thrown away with the error. The usage
half of this is BACKEND-40 and was settled first; the text is worth more than the bill.

The calls are the exception because a cutoff lands wherever the model happened to be. Arguments
that stopped mid-string are not arguments, and a streamed call whose arguments had not begun
arrives as a call with none at all: `write_file` with an empty object, which is a call nobody
asked for being handed to a turn loop that would run it. Nothing distinguishes that from a
finished call except the stop reason, and the stop reason says not to trust any of them.

`verified-by: bravebot_bedrock::lib::a_reply_the_ceiling_stopped_keeps_its_text_and_asks_for_no_tools`
`verified-by: bravebot_bedrock::lib::output_limit_keeps_completed_usage`
`verified-by: bravebot_bedrock::lib::reaching_the_token_ceiling_is_not_retried`

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
`verified-by: bravebot_config::settings::a_layer_above_the_home_one_outranks_a_saved_pick`
`verified-by: bravebot_config::settings::the_home_layer_does_not_outrank_a_saved_pick`
`verified-by: bravebot_config::settings::a_layer_above_that_names_nothing_does_not_outrank_a_saved_pick`
`verified-by: bravebot_session::store::a_checkouts_level_outranks_the_saved_pick_and_the_home_file_does_not`
`verified-by: bravebot_session::store::a_settings_file_naming_no_level_asks_for_none`
`verified-by: bravebot_tui::persist::a_recorded_level_answers_between_a_checkouts_file_and_the_persons_own`
`verified-by: bravebot_cli::running::a_run_sends_the_level_a_settings_file_named_where_nothing_is_recorded`
`verified-by: bravebot_cli::running::a_run_sends_a_checkouts_level_over_the_recorded_one`
`verified-by: bravebot_cli::running::a_run_sends_the_level_the_command_line_named_over_every_other`
`verified-by: bravebot_cli::main::an_effort_flag_names_the_level_a_run_asks_for`
`verified-by: bravebot_cli::main::an_effort_flag_naming_no_level_is_refused`
`verified-by: bravebot_config::managed::a_name_outside_the_pinnable_set_pins_nothing`

## Known costs

- **The refusal is made at startup, and a model chosen mid-session is not checked again.**
  BACKEND-39 is asked once, before a session opens, so somebody who opens on a configured model and
  then picks one of Brave's from `/model` keeps it for the rest of that session.
  Checking per turn would put a file read and a refusal in front of every round, and the choice
  there is a person naming a model from a list that says which service answers it (BACKEND-6),
  which is the opposite of the case the clause exists for: nobody arrives at it without knowing
  what they picked.

- **Which model answers is the individual's, and a managed layer cannot pin it.** A pinned default
  model would lose to a `/model` choice wherever one is in force, so it would pin nothing, and
  making that choice unavailable is a change to what a person is offered rather than to where a
  request goes. An organisation with a reason to care, a cost or a data-handling consequence
  attached to one model, has the endpoint and the account to say it with and not the name.

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

- **The effort level is the one field in a Bedrock request that a single provider defines.** The
  body Bedrock states for every provider it hosts has no field for how hard to think, so the level
  travels in the field that service hands to the model without reading, spelled the way the
  Anthropic API spells it. A tier naming a model from another provider is reachable and answers,
  and a level chosen against one is refused by that model on the field name. Nothing here can tell
  the two apart, an inference-profile ARN not saying which provider serves it, and the alternative
  is withholding a level from every Bedrock model including the ones that read it.

- **Which models a product is served is the service's decision, and this holds no copy of it.** The
  roster is whatever the endpoint returns for `bravebot`, so a model becoming unsuitable for agentic
  work is a change nothing here would notice, and one wrongly dropped from the curated set is a model
  a person cannot pick however well it would have worked. The alternative is a list of names compiled
  in, which BACKEND-12 already declines for the tier words and for the same reason: the service owns
  the names, and a copy here is a copy that goes stale.

- **A layer a checkout carries is trusted as far as the person's own file is.** A `.bravebot`
  directory arrives with whatever produced the checkout, so a `settings.json` in one can name the host
  every request goes to and the credential that signs it, and somebody who has not read it would not
  know. Nothing here distinguishes the layers, because the resolution being copied does not. For every
  field but one, what limits the damage is the same rule that limits it anywhere: a file names a
  destination and grants no capability, so the worst it does is send a request somewhere useless or
  somewhere watching. Refusing the fields that name a destination in the project layers is the fix if
  that trade stops being worth it, and it would cost the main reason to put a value in a checkout at
  all.

  The one field that is not a destination is `attribution`, whose value [BACKEND-30](#BACKEND-30)
  states to the planner. A checkout can therefore put a string of its own choosing in front of every
  round, which is a wider thing than naming a host, and it is the one place a settings layer reaches
  the planner's context at all. What is quoted is fenced and said to be text to copy rather than
  written in as a sentence addressed to the planner, and the fence is sized to the value so a value
  holding one cannot close it. That is a presentation, not a guarantee, and it is the whole of the
  mitigation: the standing instructions the same checkout carries go through a trust gate that can
  refuse them and this does not, because it is configuration and the resolution being copied reads
  configuration the same wherever it came from. Stating only the layer in the person's own directory
  is the fix if that trade stops being worth it, and it would cost a checkout the ability to say what
  its own history carries.

- **The aichat endpoint Brave runs discards the effort level.** Measured against that endpoint: a
  nonsense value in `reasoning_effort` is answered `200` with usage identical to a request that omits
  the field, so it is not validated, and on `near-glm-5`, which reports a non-zero reasoning-token
  count for an ordinary prompt, that count does not move with the level. The premium rows of the
  roster were not measured, an unsubscribed request being substituted to a weaker model before the
  request lands, so nothing here is established about them. A level chosen against a Brave-served
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

- **A gateway that wants a credential and was told of none is refused by the service rather than
  here.** BACKEND-16 reads a block naming no credential as the person saying none is wanted, so a
  block naming a service that does want one, and naming none, sends an unauthenticated request and
  gets that service's own rejection where a local refusal named the remedy. What the block says is
  the only statement available about whether a credential is wanted; the endpoint does not answer it,
  a private deployment behind a name this system knows being free to want none and a local service
  being reachable at one of those names. Deciding by endpoint would buy the better message for the
  names BACKEND-17 compiles in and pay for it by refusing every gateway outside them that wants
  nothing. A block whose `apiKey` is written blank, or whose `env` lists only blank names, names no
  credential by that same reading, since a blank in this file is read as nothing having been written
  there. A placeholder somebody meant to fill in later is therefore read as them saying none is
  wanted, and the refusal that would have pointed at it does not happen.

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

- **An ephemeral cache entry expires on a few minutes of inactivity, and nothing here tracks it.** A
  person who thinks between turns misses more often than the token arithmetic suggests, and a miss
  looks identical to a service that reports nothing: the reply carries a zero read either way.
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

- **The request BACKEND-32 sends again is outside the retry budget, and spends a credential.** It is
  not a retry: nothing failed in transit, and the same body with a field removed is a different
  request. So it does not count against the three attempts a failure gets, which means the worst case
  is four requests rather than three. On a premium subscription it also costs one more single-use
  credential, each request presenting its own. That is once per service and model per process, and
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

- **The assumed reply ceiling is low, and a tier that does not raise it keeps hitting it.** For the
  reason BACKEND-41 gives, the figure has to hold for a model nothing here can identify, so a
  session on a model that would have written sixty thousand tokens still stops at the assumed one
  until somebody exports a better figure. What that costs is now the tail of an answer rather than
  the answer, since BACKEND-42 keeps what was written, but it is still an answer that stops short
  for a reason belonging to this program rather than to the model.

- **A reply that stopped short reads like one that finished.** BACKEND-42 keeps the text, and text
  is all it is: the sentence ends wherever the ceiling fell. What says otherwise is a line beside
  it, which somebody reading only the answer does not have to notice.
