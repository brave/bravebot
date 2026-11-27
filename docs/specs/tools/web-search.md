---
id: WEBSEARCH
title: web_search
status: normative
governs:
  - crates/agent/src/tools.rs
  - crates/agent/src/tools/offered.rs
  - crates/agent/src/tools/web_search.rs
  - crates/agent/src/web_search.rs
  - crates/config/src/key_file.rs
  - crates/config/src/web_search_key.rs
  - crates/core/src/capability.rs
  - crates/core/src/delegate.rs
  - crates/core/src/permissions.rs
  - crates/core/src/policy.rs
guards:
  - symbol: Policy::before_search_rules
  - symbol: Policy::before_search
  - symbol: Policy::search_needs_approval
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Searching the web through the Brave Search API with a key a person configured. The tool is
`web_search`. Its routing argument is `service`, one word from a closed set that the driver reads as a
literal. Its content arguments are the query `q` and, for place search, `location`, which are the
planner's own words and go on the wire as text. Every other argument is a number, a flag or a word
from a closed set, which is routing on the footing [TOOL-1](tool-surface.md#TOOL-1) gives a flag or
a number. The `why` every tool takes ([TOOL-5](tool-surface.md#TOOL-5)) and the `read` that asks for
the result in the same call ([WEBSEARCH-9](#WEBSEARCH-9)) reach no request.

The result is a reference to a quarantined body, as a fetched page is ([fetch-url.md](fetch-url.md)).
Where a rule is written and how one is matched is [permissions.md](../permissions.md). What a label
means is [labels.md](../labels.md). Where the key lives and what ends it is
[credential-protection.md](../credential-protection.md).

## What it covers

`service` is `web`, `news`, `videos`, `images`, `llm_context`, `suggest`, `spellcheck` or `places`.
Each is one `GET` of the vendor's reference, with the parameters the vendor documents for it, under
the vendor's names.

Three things the vendor offers are not offered here.

- **Goggles.** A goggle is a URL Brave fetches, or a definition that steers ranking. A URL the
  planner chose that another host then fetches is a destination nobody approved, and a definition is
  free text that decides what the planner is shown.
- **Answers.** It is a `POST` that streams, and a research mode that runs for minutes. Neither fits
  a request that returns one reference.
- **Rich results, local points of interest and descriptions.** The identifiers they take come from
  the bytes of an earlier result, which the planner cannot read, so it could not name one.

The `X-Loc-*` headers are not sent. A location is given only through the `places` parameters.

## Why a query is admissible

A query is the planner's own words. A person can read it and say yes or no to it before it leaves, and
nothing in it names a place the request goes: the host and the path are written in this program's
source. That is the test [TOOL-2](tool-surface.md#TOOL-2) applies. What would fail it is an argument
that makes Brave fetch something the planner chose, which is why no argument takes a URL.

## Clauses

<a id="WEBSEARCH-1"></a>
### WEBSEARCH-1: a search is its own capability, and what it returns is untrusted and public

The label comes from `Capability::WebSearch`, which is not `Capability::WebFetch`, so it is fixed before
the request goes out and nothing about the reply can raise it. The planner gets a reference and never
the bytes: it may hand them to a processor, or write them to a file with `contents_ref`, and it cannot
read them or be told what they say. Approving a query does not trust the results. The release a
person, a check or the mode makes is [vet-content.md](vet-content.md)'s.

**Why separate from `WebFetch`.** The destination is one host fixed in source and what is sent is the
planner's own query, so a person's answer about a search is not an answer about fetching a page. A rule
about one says nothing about the other, as `LanguageServer` is separate from `FileRead`.

**Why untrusted.** An index returns what the owners of the pages wrote, and the request being to a
service this program trusts does not make the text so. Public because a search result is not the
person's data.

`verified-by: bravebot_core::capability::a_search_is_its_own_capability_with_a_fetched_body_label`
`verified-by: bravebot_core::policy::approving_a_search_never_trusts_what_comes_back_and_covers_one_request`
`verified-by: bravebot_agent::web_search::a_search_returns_a_reference_sends_the_key_in_one_header_and_asks_about_the_query`

<a id="WEBSEARCH-2"></a>
### WEBSEARCH-2: every search asks, unless a `WebSearch` rule allows it and nothing private has been shown

A person is shown the whole URL the search will send, with the query in it, and the fixed host. An
`allow` rule for `WebSearch` answers the prompt, a `deny` rule refuses without asking, and an `ask`
rule asks. A rule takes the family alone: `WebSearch(weather)` and `WebSearch(domain:example.com)` are
unreadable ([PERM-11](../permissions.md#PERM-11)), since a specifier would match the host, which is
fixed, or the query, which is content and not something the driver decides on. A `WebFetch` rule does
not cover a search.

An `allow` rule does not answer the prompt once the planner's context holds private content. The
query is the planner's own words, so it may carry some, and an `allow` rule is a person agreeing in
advance to searches nobody has read. Which of the two holds is the label of the context and nothing
the query says. A `deny` rule holds whatever the context holds.

Bypassing answers the question without putting it to anybody, as it answers a fetch's
([MODE-4](../permission-modes.md#MODE-4)). Auto does not put a search to its classifier, which is
consulted for `fetch_url` only ([AUTO-3](../auto-mode.md#AUTO-3)), so in auto the question goes to the
person. A search that is put to a person and not approved is a refusal under `approval` in the trail,
in fixed words that name the tool and carry nothing of the query ([TRACE-1](../trace.md#TRACE-1)).

**Why.** A query is where a model can send a person's own data to a third party, so the question is
about what is sent, and a standing grant for it has to stop where private content enters.

`verified-by: bravebot_core::policy::a_search_asks_unless_a_websearch_rule_allows_it`
`verified-by: bravebot_core::policy::an_allow_rule_does_not_cover_a_search_made_after_private_content_was_shown`
`verified-by: bravebot_core::policy::a_websearch_deny_rule_refuses_and_a_webfetch_deny_rule_does_not`
`verified-by: bravebot_core::permissions::a_websearch_rule_takes_no_specifier`
`verified-by: bravebot_agent::web_search::a_websearch_rule_answers_the_prompt_and_a_webfetch_rule_does_not`
`verified-by: bravebot_agent::web_search::a_search_the_person_declines_is_not_sent`
`verified-by: bravebot_cli::running::permissions_check_decides_a_web_search_by_its_own_family`
`verified-by: bravebot_agent::web_search::bypass_answers_a_search_and_auto_leaves_it_to_the_person_without_a_classifier_word`
`verified-by: bravebot_core::policy::a_search_the_person_did_not_approve_is_a_refusal_under_approval`

<a id="WEBSEARCH-3"></a>
### WEBSEARCH-3: an approval is bound to the URL it was given for, and nothing is remembered

The endorsement names the exact URL, query included, and is spent once. There is no "always" at
this prompt. Standing permission is a rule in a settings file.

**Why.** The same reason as [FETCH-3](fetch-url.md#FETCH-3): an answer about one query is not an
answer about the next.

`verified-by: bravebot_core::policy::approving_a_search_never_trusts_what_comes_back_and_covers_one_request`

<a id="WEBSEARCH-4"></a>
### WEBSEARCH-4: the tool is offered only where a key source is configured

`web_search` is on the planner's list when a variable `webSearch.env` names holds text, or
`webSearch.keyFile` names a file that exists. With neither, it is not on the list and the name is
answered as an unknown tool ([TOOL-3](tool-surface.md#TOOL-3)), so the planner is not sent after a tool
that can only fail. Which of these holds is read from the configuration, the process environment and
the file's metadata, and the key is not read to decide it.

The list is settled when a turn starts. A variable exported after that turn began is offered from the
next turn, and one removed after it began leaves the tool on the list for the rest of the turn: a
search then reads the key when it is made and is reported as having no usable key, in words that
name the two settings and neither a variable nor a path.

A key source that exists and holds no usable key is reported when the tool is called, before anybody
is asked, and the planner's sentence names the settings and not a variable, a path or what either held.

**Why.** A tool that is offered and always fails costs a round and a question to a person.

`verified-by: bravebot_agent::web_search::the_tool_is_offered_only_where_a_key_file_is_configured`
`verified-by: bravebot_agent::web_search::a_variable_alone_offers_the_tool_and_the_key_reaches_only_the_header`
`verified-by: bravebot_agent::web_search::variables_that_hold_nothing_do_not_offer_the_tool`
`verified-by: bravebot_agent::web_search::a_variable_removed_after_the_turn_started_is_reported_as_an_unusable_key`
`verified-by: bravebot_agent::web_search::an_unusable_key_file_is_reported_before_a_question_without_its_path_or_content`
`verified-by: bravebot_config::web_search_key::a_named_variable_that_holds_nothing_offers_no_key`
`verified-by: bravebot_config::web_search_key::a_variable_alone_offers_a_key`
`verified-by: bravebot_config::lib::web_search_is_offered_only_for_a_key_file_that_exists`
`verified-by: bravebot_config::key_file::offered_looks_at_the_file_and_not_at_what_it_holds`

<a id="WEBSEARCH-5"></a>
### WEBSEARCH-5: arguments are checked against what the vendor documents before anything is sent

A call is refused, in words this program wrote and naming the argument from a fixed table, when
`service` is missing or not one of the eight, when `q` is missing (place search alone may go without
one), empty, over its service's limit of characters or words or holding a control character, when a
number is outside the range the service documents, when a word is outside the set, when a flag is not
a boolean, when an argument belongs to another service, when `latitude` and `longitude` are not given
together, and when an argument is none of the vendor's. A value is never coerced: `"3"` is not `3`. No
argument takes a URL, a path, a host or a goggle.

The URL is written from the fixed base, the service's path and the checked parameters, in a fixed order
with each value percent-encoded, so a query cannot add a parameter or end the URL. A refusal repeats
nothing the planner typed ([TOOL-4](tool-surface.md#TOOL-4)), and it is made before any question is put
and before any request is sent.

**Why.** The vendor answers a bad parameter with an error the driver would then have to word, a person
would have been asked about a request that could not succeed, and a parameter that is not checked is a
place a planner's text reaches the wire as something other than a query.

`verified-by: bravebot_agent::web_search::each_service_has_its_own_path_under_the_fixed_base`
`verified-by: bravebot_agent::web_search::a_query_cannot_add_a_parameter_or_end_the_url`
`verified-by: bravebot_agent::web_search::parameters_keep_the_vendors_names_and_a_fixed_order`
`verified-by: bravebot_agent::web_search::count_is_bounded_by_the_service_asked`
`verified-by: bravebot_agent::web_search::an_argument_the_service_does_not_document_is_refused`
`verified-by: bravebot_agent::web_search::image_search_takes_no_moderate_safesearch`
`verified-by: bravebot_agent::web_search::a_query_is_held_to_its_services_limits`
`verified-by: bravebot_agent::web_search::only_place_search_may_go_without_a_query`
`verified-by: bravebot_agent::web_search::a_place_is_named_one_way_and_coordinates_come_as_a_pair`
`verified-by: bravebot_agent::web_search::freshness_is_a_preset_or_an_ordered_range_of_dates`
`verified-by: bravebot_agent::web_search::a_result_filter_is_a_list_of_documented_result_types`
`verified-by: bravebot_agent::web_search::types_are_not_coerced`
`verified-by: bravebot_agent::web_search::llm_context_limits_are_the_documented_ranges`
`verified-by: bravebot_agent::web_search::no_argument_names_a_host_a_path_or_a_goggle`
`verified-by: bravebot_agent::web_search::a_missing_or_unknown_service_is_refused`
`verified-by: bravebot_agent::web_search::a_refusal_repeats_nothing_the_planner_typed`
`verified-by: bravebot_agent::web_search::an_argument_off_the_documented_range_is_refused_before_a_question_or_a_request`

<a id="WEBSEARCH-6"></a>
### WEBSEARCH-6: the key is a variable or a file a person named in their own settings, and it reaches one header

`webSearch.env` and `webSearch.keyFile` are read from `~/.bravebot/settings.json` and from the file
`--settings` names, and from no other: a project or local file that names either is ignored and
reported, since a checkout could otherwise choose which variable or file on this machine is sent to a
host. `webSearch.env` is a list of variable names, tried in the order written, and an entry that is
blank or is not a string is dropped, as an entry of a provider's `env` is. `webSearch.keyFile` is an
absolute path.

The key is read when a search is made, so a variable exported in an open session is used by the next
search. The first variable of `webSearch.env` that holds text is the key, and a variable that is unset
or holds only whitespace is passed over. Where no variable holds text, the file `webSearch.keyFile`
names is the key. A variable that holds text is used before the file, whether or not the text is a
usable key: one that is not is refused and the file is not read in its place, so a variable exported
wrongly is reported and not hidden by a file that works.

Whichever held it, the text is refused when it is over 4,096 bytes, is empty after trimming, or holds a
space, a control character or a character outside ASCII, and a file is refused as well when it is not a
regular file or is not UTF-8. The text and the bytes it was read through are overwritten, and no error
carries a variable's name, a path or any text. The refusal the planner reads names the two settings and
nothing the person wrote. The value is held as a `Secret`, and is sent in one header,
`X-Subscription-Token`, of the request to the search host. It is not in the URL, a result, a prompt,
the trail, the log, an error or anything the planner is sent. A hop of the request that leaves the host
is sent without it ([WEBSEARCH-7](#WEBSEARCH-7)). The header is held on the request as a socket header,
and a `Request` formats as the names of its headers and never their values
([NET-1](../network-egress.md#NET-1)).

No command-line argument carries the key ([CRED-24](../credential-protection.md#CRED-24)).

The variables `webSearch.env` names are removed from the environment of every program this agent
starts, in the way the variables a provider's `env` names are ([RUN-12](run.md#RUN-12)), so the key
is read by this program and handed to no program a turn runs. The planner is not told the names,
which are strings the person wrote.

`bravebot doctor` prints a `web search` line that says which source the key would come from: a
variable that holds text, said by its place in `webSearch.env` and never by its name, or the file, said
with the path the person wrote, or that neither exists. It never prints the key or a variable's name.
Its credential record names the key at Held, and names the variable, the setting and the file as what
ends this machine's custody and leaves the key working at the search service until it is revoked in
the dashboard ([CRED-2](../credential-protection.md#CRED-2),
[CRED-25](../credential-protection.md#CRED-25)).

**Why a variable is allowed.** The environment is a way of handing a program a credential that
[CRED-24](../credential-protection.md#CRED-24) lists as allowed, and a provider's token is read from
one. What a variable costs is that a program the `run` tool starts inherits the environment of this
process, so a key held there would be handed to each program a turn starts. [RUN-12](run.md#RUN-12)
answers that by withholding the named variables from those programs, and a file is not inherited by
anything.
**Why home only.** The settings choose which variable or file of the person's is sent to a host, and
which variables are kept from their programs.

`verified-by: bravebot_net::lib::a_request_prints_its_header_names_and_never_their_values`
`verified-by: bravebot_config::settings::a_project_layer_cannot_name_a_web_search_key_file`
`verified-by: bravebot_config::settings::a_project_layer_cannot_name_a_web_search_variable`
`verified-by: bravebot_config::settings::a_web_search_block_that_is_not_an_object_takes_neither_setting_away`
`verified-by: bravebot_config::lib::debugging_the_config_does_not_leak_the_key`
`verified-by: bravebot_config::settings::the_two_web_search_settings_are_each_refused_to_a_project_layer_on_their_own`
`verified-by: bravebot_config::settings::web_search_variable_names_are_read_as_a_providers_env_is`
`verified-by: bravebot_config::settings::a_file_that_only_names_a_web_search_variable_is_not_empty_settings`
`verified-by: bravebot_config::settings::the_variables_web_search_env_names_are_withheld_from_a_program`
`verified-by: bravebot_config::settings::a_web_search_key_file_must_be_an_absolute_path`
`verified-by: bravebot_config::lib::the_web_search_setting_reaches_the_configuration`
`verified-by: bravebot_config::lib::the_web_search_variables_reach_the_configuration_as_names`
`verified-by: bravebot_config::lib::the_variables_the_search_key_is_read_from_are_withheld_like_a_gateways`
`verified-by: bravebot_config::lib::a_search_key_is_held_exactly_when_web_search_is_offered`
`verified-by: bravebot_config::lib::a_search_key_from_a_variable_is_held_without_a_file`
`verified-by: bravebot_config::web_search_key::a_variable_wins_over_the_file_and_the_file_answers_where_none_holds_text`
`verified-by: bravebot_config::web_search_key::the_first_variable_with_text_wins_and_an_empty_one_is_skipped`
`verified-by: bravebot_config::web_search_key::a_variable_holding_no_usable_key_is_refused_and_the_file_is_not_used_instead`
`verified-by: bravebot_config::web_search_key::a_variables_text_is_trimmed_as_a_files_is`
`verified-by: bravebot_agent::web_search::a_variable_beats_the_key_file`
`verified-by: bravebot_agent::web_search::the_key_file_answers_where_no_named_variable_holds_text`
`verified-by: bravebot_agent::web_search::the_first_named_variable_that_holds_text_is_the_key`
`verified-by: bravebot_agent::web_search::a_variable_holding_no_usable_key_is_refused_without_its_name_or_its_text`
`verified-by: bravebot_agent::exec::a_variable_web_search_env_names_does_not_reach_a_program_it_runs`
`verified-by: bravebot_agent::exec::a_steps_own_assignment_of_a_search_key_variable_does_not_reach_that_step`
`verified-by: bravebot_cli::doctor::the_web_search_source_is_reported_by_place_and_never_by_the_variable_or_its_text`
`verified-by: bravebot_config::key_file::a_key_file_is_read_and_trimmed`
`verified-by: bravebot_config::key_file::a_blank_file_is_empty`
`verified-by: bravebot_config::key_file::a_file_over_the_bound_is_too_large`
`verified-by: bravebot_config::key_file::a_text_that_cannot_be_a_header_value_is_not_a_key`
`verified-by: bravebot_config::key_file::a_file_that_is_not_text_is_unreadable`
`verified-by: bravebot_config::key_file::a_missing_file_and_a_directory_are_unreadable_and_not_offered`
`verified-by: bravebot_cli::doctor::the_web_search_key_is_reported_by_its_setting_and_never_by_its_value`
`verified-by: bravebot_agent::web_search::a_search_returns_a_reference_sends_the_key_in_one_header_and_asks_about_the_query`

<a id="WEBSEARCH-7"></a>
### WEBSEARCH-7: the request goes through the egress gate to one host, and a redirect off it is refused

The host is `api.search.brave.com`, written in `bravebot-config` and replaceable only by a test. No
setting, variable, flag or argument reaches it. Every hop passes the egress gate with the response cap
of [NET-4](../network-egress.md#NET-4), and a body cut at the cap is reported in this program's words.

While a search is in flight the gate refuses any hop whose host is not the search host, whatever
rules allow, and asks nobody. This differs from [FETCH-4](fetch-url.md#FETCH-4), which puts a redirect
to a person: a fetch's host is one a person chose from a page, while the search host is not a
destination anybody could approve instead. The refused hop is not kept for a question, no request goes
to it, the key header does not, and the planner's sentence names no host the server chose
([FETCH-6](fetch-url.md#FETCH-6)).

The address check of [FETCH-8](fetch-url.md#FETCH-8) applies to a search as well, because
`Policy::fetch_in_flight` is true while either is in flight and the network crate scopes the check by
it. A test pins that the flag is true from the approval of a search until it finishes. No test sends a
search to a name that resolves to a non-public address: the tests here use a stand-in on a loopback
address, which that check allows when it is written as an address, and the host a search reaches is a
name this program wrote.

**Why.** An index host that could redirect the request would receive the key header with it.

`verified-by: bravebot_core::policy::a_search_cannot_be_redirected_to_another_host_and_the_host_is_not_asked_about`
`verified-by: bravebot_core::policy::a_search_is_in_flight_from_its_approval_until_it_finishes`
`verified-by: bravebot_agent::web_search::a_redirect_off_the_search_host_is_refused_and_sent_neither_a_request_nor_the_key`
`verified-by: bravebot_agent::web_search::a_body_past_the_cap_is_reported_as_incomplete`

<a id="WEBSEARCH-8"></a>
### WEBSEARCH-8: a delegate cannot search

`web_search` is not offered to a delegate, and a call to it from one is answered as an unknown name,
with nobody asked and nothing sent. Every kind holds `Capability::WebSearch`, because a turn a person
addressed to a definition keeps only what the definition's kind holds, and a kind without it would
take the tool from a turn the person was watching.

**Why.** A delegate's query is a planner's sentence that the person never saw, written for a sub-task
they did not set, and the prompt of [WEBSEARCH-2](#WEBSEARCH-2) is for a person reading the query.

`verified-by: bravebot_agent::web_search::a_delegate_naming_web_search_is_answered_as_an_unknown_name_and_sends_nothing`
`verified-by: bravebot_agent::tools::tests::delegate_offers::no_kind_is_offered_a_tool_that_reaches_the_network`

<a id="WEBSEARCH-9"></a>
### WEBSEARCH-9: a search may ask for its results in the same result, and gets them where the mode or a check answers

`web_search` takes a boolean `read`, offered only on a list that holds `vet_content`, and it
behaves as [FETCH-9](fetch-url.md#FETCH-9) says for `fetch_url`. The request, the label and the
quarantine are those of any search, and a release is the release `vet_content` makes. A `read` that is
not a boolean is refused before anything is asked or sent, and absent and null are `false`. A search
that failed has no body to release.

**Why.** The same as for a fetch: a second round that only asks for what a mode or a check would release
costs a model call and the conversation sent again.

`verified-by: bravebot_agent::web_search::a_search_that_asks_to_read_is_released_as_a_fetch_is_and_one_that_does_not_is_not`
`verified-by: bravebot_agent::web_search::a_search_whose_read_is_not_a_boolean_is_refused_before_a_question_or_a_request`

<a id="WEBSEARCH-10"></a>
### WEBSEARCH-10: a failure is worded by the driver from its kind, and carries nothing the service wrote

A refused hop, a timeout, a stop, a redirect that was not followed and a service that could not be
reached each have one sentence of this program's. An error status has one sentence that is the same for
every status: it gives the number, says what 401, 403, 402, 400, 422 and 429 usually mean and what to do
next in each case, and no part of it is chosen from the number. The error body, its `detail`, its `meta`
and the rate-limit headers are not read.

**Why.** The error body is text the service wrote, and a refusal is trusted prose
([TOOL-4](tool-surface.md#TOOL-4)).

`verified-by: bravebot_agent::web_search::a_failure_is_worded_by_the_driver_and_carries_none_of_the_vendors_body`
`verified-by: bravebot_agent::web_search::a_service_that_cannot_be_reached_is_reported_without_a_transport_message`

## Known costs

- **The key is read before the prompt.** A call reads and checks the key before the rules and the
  prompt are consulted, from the first variable `webSearch.env` names that holds text or else from the
  key file, so a search a `deny` rule refuses or a person declines has still read the variable or the
  file. The value is dropped without being sent, and nothing of it reaches a refusal or the trail.
- **The `spawn_agent` description does not name `web_search`.** A planner that writes a delegate's task
  is told that a delegate cannot call `fetch_url` or `download_url`, and is not told the same of
  `web_search`. A delegate told to search is answered as for an unknown name and the planner learns it
  from that answer.
- **The prompt reads as a fetch prompt in the front ends.** The search is put to a person through
  the fetch confirmation. It draws the search URL, with the query in it, and the fixed host, under the
  title and wording of a fetch ("fetch this?"). A person can read what is sent, and a dedicated wording
  for a search is not written.
- **The auto and bypass modes treat the question as a fetch's.** The classifier of the auto mode is
  not consulted for a search, which is recorded as not asked, and bypassing answers it as it answers
  a fetch.
- **The status number reaches the planner.** It is formatted into the sentence, as `fetch_url` words
  an error status, and nothing chooses between sentences on it. The sentence says what the usual
  numbers mean, so a number the service sends that is not one of them is told as a number alone.
- **A search reveals the query to Brave.** The prompt is the only gate on that, and an `allow` rule is
  a person's decision in advance, up to the point private content enters the context.
- **Nothing here is verified on Linux or Windows beyond what the shared tests run on.** The tests use
  a stand-in server on loopback, and the live service was called from a macOS checkout only.
